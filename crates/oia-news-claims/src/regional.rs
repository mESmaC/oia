//! Regional accounts of reported developments (`NEWS-02`, `GEO-03` counts, `A06`).
//!
//! [`build_regional_account`] assembles a region- and window-scoped account from
//! event candidates over a corpus that first passes [`verify_corpus`]. Each
//! candidate keeps its epistemic category (plan, observation, attributed
//! statement, allegation, arrest, charge, judgment); categories are never merged
//! or upgraded. Reported movement never establishes intent, mission, or
//! destination: a stated purpose is accepted only when it is anchored to a
//! passage and attributed to a speaker or to the issuing source of a plan.
//!
//! Grouping into proposed events is an explicit analyst decision supplied by the
//! caller (for example the current revision of a reversible clustering ledger).
//! This module does not cluster automatically. Article, article-version,
//! syndicated-reprint, candidate, derived independent-source, and declared
//! lineage-root counts stay separate, and no count is presented as an incident
//! frequency or crime rate. Independent sources are derived from the corpus
//! (shared record, source, declared root, identical content, or a quoting,
//! republishing, or superseding edge), so a declaration can merge sources but
//! never split them.

use std::error::Error;
use std::fmt;

use super::{
    AssessmentLimits, ClaimCheckError, CorpusAssessmentInput, LineageRelationKind, ReviewStatus,
    SourceRecordInput, StopReason, find_passage, find_record, is_valid_rfc3339_utc_second,
    verify_corpus,
};

/// Epistemic category of a reported development (`NEWS-02`).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum EpistemicCategory {
    Plan,
    Observation,
    AttributedStatement,
    Allegation,
    Arrest,
    Charge,
    Judgment,
}

impl EpistemicCategory {
    fn legal_stage_rank(self) -> Option<u8> {
        match self {
            Self::Arrest => Some(0),
            Self::Charge => Some(1),
            Self::Judgment => Some(2),
            _ => None,
        }
    }

    fn requires_attributed_speaker(self) -> bool {
        matches!(self, Self::AttributedStatement | Self::Allegation)
    }

    /// Categories in which a source may state a purpose without that purpose
    /// becoming an observation. The account keeps it attributed in every case.
    fn may_carry_stated_purpose(self) -> bool {
        matches!(
            self,
            Self::Plan | Self::AttributedStatement | Self::Allegation | Self::Judgment
        )
    }
}

/// Kind of regional development named in `NEWS-02`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DevelopmentKind {
    MilitaryMovement,
    Exercise,
    Deployment,
    CriminalActivity,
    PublicSafetyIncident,
    TransportDisruption,
    Other,
}

/// Basis of a reported location. A gazetteer centroid is an approximation, not
/// a measured position.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocationBasis {
    Polygon,
    PointWithUncertaintyRadius,
    GazetteerCentroid,
    NotReported,
}

/// Reported location basis and horizontal uncertainty in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReportedLocation {
    pub basis: LocationBasis,
    /// Required, finite, and positive unless `basis` is `NotReported`, in which
    /// case it must be `None`.
    pub uncertainty_m: Option<f64>,
}

/// A reported event interval in strict `YYYY-MM-DDTHH:MM:SSZ` UTC form with a
/// symmetric bound uncertainty. The original strings are preserved.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReportedInterval<'a> {
    pub start_utc: &'a str,
    pub end_utc: &'a str,
    pub uncertainty_s: u32,
}

/// What a stated purpose refers to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PurposeKind {
    Intent,
    Mission,
    Destination,
}

/// A purpose stated in the candidate's own source record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StatedPurposeInput<'a> {
    pub kind: PurposeKind,
    pub text: &'a str,
    /// Passage in the candidate's source record that states the purpose.
    pub passage_id: &'a str,
}

/// Typed regional description of a corpus event candidate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RegionalCandidateInput<'a> {
    /// Must name a candidate in [`CorpusAssessmentInput::candidates`].
    pub candidate_id: &'a str,
    /// Analyst-assigned proposed event (an explicit, reversible decision).
    pub proposed_event_id: &'a str,
    pub category: EpistemicCategory,
    pub development_kind: DevelopmentKind,
    /// Required for attributed statements and allegations.
    pub attributed_speaker: Option<&'a str>,
    /// Passages in the candidate's source record supporting this entry.
    pub supporting_passage_ids: &'a [&'a str],
    pub reported_time: Option<ReportedInterval<'a>>,
    pub location: ReportedLocation,
    /// The source reports movement of people, vehicles, vessels, or units.
    pub reports_movement: bool,
    pub stated_purpose: Option<StatedPurposeInput<'a>>,
    /// Required for arrests, charges, and judgments; forbidden otherwise.
    pub legal_matter_id: Option<&'a str>,
}

/// How a preserved conflict currently stands. No variant deletes either side.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConflictResolution {
    Unresolved,
    BothRetainedWithUncertainty,
    ResolvedByCorrectionHistoryPreserved,
}

/// A disagreement among member records that must survive grouping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EventConflictInput<'a> {
    pub conflict_id: &'a str,
    pub description: &'a str,
    pub conflicting_version_ids: &'a [&'a str],
    pub corrected_or_authoritative_version_ids: &'a [&'a str],
    pub resolution: ConflictResolution,
}

/// An analyst-proposed event and its current grouping decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProposedEventInput<'a> {
    pub event_id: &'a str,
    pub version_id: &'a str,
    pub region_id: &'a str,
    /// Grouping decision this membership comes from (for example a cluster revision).
    pub grouping_decision_id: &'a str,
    /// Member records with no typed candidate (for example syndicated reprints
    /// or corrected versions). Candidate records are members automatically.
    pub additional_member_version_ids: &'a [&'a str],
    pub conflicts: &'a [EventConflictInput<'a>],
}

/// Regional account request.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RegionalAccountInput<'a> {
    pub region_id: &'a str,
    pub window_start_utc: &'a str,
    pub window_end_utc: &'a str,
    pub events: &'a [ProposedEventInput<'a>],
    pub candidates: &'a [RegionalCandidateInput<'a>],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegionalLimitKind {
    Events,
    Candidates,
    MembersPerEvent,
    ConflictsPerEvent,
    PassagesPerCandidate,
    TextBytes,
}

/// Explicit budgets for a regional account.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegionalLimits {
    max_events: usize,
    max_candidates: usize,
    max_members_per_event: usize,
    max_conflicts_per_event: usize,
    max_passages_per_candidate: usize,
    max_text_bytes: usize,
}

impl RegionalLimits {
    pub fn new(
        max_events: usize,
        max_candidates: usize,
        max_members_per_event: usize,
        max_conflicts_per_event: usize,
        max_passages_per_candidate: usize,
        max_text_bytes: usize,
    ) -> Result<Self, RegionalAccountError> {
        for (value, kind) in [
            (max_events, RegionalLimitKind::Events),
            (max_candidates, RegionalLimitKind::Candidates),
            (max_members_per_event, RegionalLimitKind::MembersPerEvent),
            (
                max_conflicts_per_event,
                RegionalLimitKind::ConflictsPerEvent,
            ),
            (
                max_passages_per_candidate,
                RegionalLimitKind::PassagesPerCandidate,
            ),
            (max_text_bytes, RegionalLimitKind::TextBytes),
        ] {
            if value == 0 {
                return Err(RegionalAccountError::ZeroLimit(kind));
            }
        }
        Ok(Self {
            max_events,
            max_candidates,
            max_members_per_event,
            max_conflicts_per_event,
            max_passages_per_candidate,
            max_text_bytes,
        })
    }

    pub fn max_events(self) -> usize {
        self.max_events
    }

    pub fn max_candidates(self) -> usize {
        self.max_candidates
    }

    pub fn max_members_per_event(self) -> usize {
        self.max_members_per_event
    }

    pub fn max_conflicts_per_event(self) -> usize {
        self.max_conflicts_per_event
    }

    pub fn max_passages_per_candidate(self) -> usize {
        self.max_passages_per_candidate
    }

    pub fn max_text_bytes(self) -> usize {
        self.max_text_bytes
    }
}

/// How an entry's reported time relates to the account window.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowOverlap {
    /// The reported bounds overlap the window.
    ReportedBounds,
    /// Only the bounds widened by their stated uncertainty overlap the window.
    UncertaintyOnly,
    /// Outside the window even with uncertainty; shown because its event is included.
    Outside,
    /// No event time was reported; inclusion cannot be decided from time.
    TimeNotReported,
}

/// Who a purpose is attributed to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttributionBasis {
    /// A named speaker given on the candidate.
    NamedSpeaker,
    /// No speaker was given; the purpose is attributed to the source that
    /// published the record (for a news article, the outlet), not to an actor.
    IssuingSource,
}

/// A purpose kept attributed to whoever stated it.
///
/// The kernel checks that the cited passage exists in the candidate's own
/// record. Whether that passage actually states this purpose and speaker is
/// the analyst's typing decision and is not verified textually.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttributedPurpose<'a> {
    pub kind: PurposeKind,
    pub text: &'a str,
    pub passage_id: &'a str,
    /// The attributed speaker, or the issuing source when no speaker was given.
    pub attributed_to: &'a str,
    pub attribution_basis: AttributionBasis,
    /// Always the entry's own category; a purpose never becomes an observation.
    pub category: EpistemicCategory,
}

/// One typed entry in an event account.
#[derive(Clone, Debug, PartialEq)]
pub struct AccountEntry<'a> {
    pub candidate_id: &'a str,
    pub source_record_version_id: &'a str,
    pub source_id: &'a str,
    pub category: EpistemicCategory,
    pub development_kind: DevelopmentKind,
    pub attributed_speaker: Option<&'a str>,
    pub supporting_passage_ids: &'a [&'a str],
    pub reported_time: Option<ReportedInterval<'a>>,
    pub window_overlap: WindowOverlap,
    pub location: ReportedLocation,
    pub reports_movement: bool,
    pub purpose: Option<AttributedPurpose<'a>>,
    pub legal_matter_id: Option<&'a str>,
    pub review_status: ReviewStatus,
    pub needs_reassessment: bool,
    /// The source record version has a later version; the entry is kept.
    pub record_superseded: bool,
    pub from_syndicated_reprint: bool,
}

/// Per-category entry counts.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CategoryCounts {
    pub plans: usize,
    pub observations: usize,
    pub attributed_statements: usize,
    pub allegations: usize,
    pub arrests: usize,
    pub charges: usize,
    pub judgments: usize,
}

impl CategoryCounts {
    fn add(&mut self, category: EpistemicCategory) {
        let slot = match category {
            EpistemicCategory::Plan => &mut self.plans,
            EpistemicCategory::Observation => &mut self.observations,
            EpistemicCategory::AttributedStatement => &mut self.attributed_statements,
            EpistemicCategory::Allegation => &mut self.allegations,
            EpistemicCategory::Arrest => &mut self.arrests,
            EpistemicCategory::Charge => &mut self.charges,
            EpistemicCategory::Judgment => &mut self.judgments,
        };
        *slot = slot.saturating_add(1);
    }
}

/// Number of included proposed events with at least one entry of each kind.
/// One event may count under several kinds.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DevelopmentEventCounts {
    pub military_movement: usize,
    pub exercise: usize,
    pub deployment: usize,
    pub criminal_activity: usize,
    pub public_safety_incident: usize,
    pub transport_disruption: usize,
    pub other: usize,
}

impl DevelopmentEventCounts {
    fn add(&mut self, kind: DevelopmentKind) {
        let slot = match kind {
            DevelopmentKind::MilitaryMovement => &mut self.military_movement,
            DevelopmentKind::Exercise => &mut self.exercise,
            DevelopmentKind::Deployment => &mut self.deployment,
            DevelopmentKind::CriminalActivity => &mut self.criminal_activity,
            DevelopmentKind::PublicSafetyIncident => &mut self.public_safety_incident,
            DevelopmentKind::TransportDisruption => &mut self.transport_disruption,
            DevelopmentKind::Other => &mut self.other,
        };
        *slot = slot.saturating_add(1);
    }
}

/// Separate source counts for one proposed event. These describe coverage of
/// the event, not how often such events occur.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EventSourceCounts {
    /// Distinct source records (articles, bulletins, logs) among members.
    pub articles: usize,
    /// Distinct member record versions.
    pub article_versions: usize,
    pub syndicated_reprint_versions: usize,
    pub superseded_versions: usize,
    pub entries: usize,
    /// Independence classes among member versions that contain at least one
    /// non-reprint record in the corpus. Classes are derived, not taken from
    /// declarations: corpus records are joined when they share a record id,
    /// declared lineage root, source id, or byte-identical content, or are
    /// linked by a `Quotes`, `Republishes`, or `Supersedes` edge. `Supports`
    /// edges do not join. Declarations can only merge classes, never split them.
    pub independent_sources: usize,
    /// Classes made only of syndicated reprints with no identified original in
    /// the corpus. They are not counted as independent sources.
    pub unanchored_reprint_classes: usize,
    /// Distinct declared `independent_lineage_root_id` values among members.
    /// An upper bound that holds only if the declarations are correct.
    pub declared_lineage_roots: usize,
    pub by_category: CategoryCounts,
}

/// One stage of a legal matter as reported.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegalStageEntry<'a> {
    pub category: EpistemicCategory,
    pub candidate_id: &'a str,
    pub source_record_version_id: &'a str,
    pub reported_time: Option<ReportedInterval<'a>>,
    pub review_status: ReviewStatus,
    pub needs_reassessment: bool,
    /// The stage's source record version has a later version.
    pub record_superseded: bool,
}

/// Reported legal process for one matter, ordered arrest → charge → judgment.
/// A later stage never removes an earlier one; an arrest or charge is not a
/// finding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegalMatterAccount<'a> {
    pub matter_id: &'a str,
    pub stages: Vec<LegalStageEntry<'a>>,
    pub latest_reported_stage: EpistemicCategory,
    pub judgment_reported: bool,
    /// A later stage has a reported start before an earlier stage's reported
    /// start. The stage order is kept and the contradiction is surfaced, not
    /// resolved.
    pub chronology_conflict: bool,
}

/// Account of one included proposed event.
#[derive(Clone, Debug, PartialEq)]
pub struct EventAccount<'a> {
    pub event_id: &'a str,
    pub version_id: &'a str,
    pub grouping_decision_id: &'a str,
    pub entries: Vec<AccountEntry<'a>>,
    pub member_version_ids: Vec<&'a str>,
    /// Declared lineage roots among members (see [`EventSourceCounts::declared_lineage_roots`]).
    pub declared_lineage_roots: Vec<&'a str>,
    pub counts: EventSourceCounts,
    pub legal_matters: Vec<LegalMatterAccount<'a>>,
    pub conflicts: &'a [EventConflictInput<'a>],
    /// Any member uses a gazetteer centroid rather than a measured location.
    pub approximate_location_present: bool,
    pub tightest_location_uncertainty_m: Option<f64>,
    pub widest_location_uncertainty_m: Option<f64>,
    /// Any entry or member record needs reassessment or has been superseded.
    pub needs_reassessment: bool,
}

/// What the account considered and what it left out.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RegionalCoverage {
    pub events_considered: usize,
    pub events_included: usize,
    pub events_outside_region: usize,
    pub events_outside_window: usize,
    pub entries_included: usize,
    pub entries_time_not_reported: usize,
    pub entries_uncertainty_only: usize,
    pub entries_outside_window_in_included_events: usize,
    pub entries_needing_reassessment: usize,
    pub entries_not_reviewed: usize,
    /// Corpus event candidates the analyst did not type for this account.
    pub corpus_candidates_untyped: usize,
}

/// Region- and window-scoped account of reported developments.
#[derive(Clone, Debug, PartialEq)]
pub struct RegionalAccount<'a> {
    pub region_id: &'a str,
    pub window_start_utc: &'a str,
    pub window_end_utc: &'a str,
    /// Ordered by earliest reported start (unreported last), then event id.
    pub events: Vec<EventAccount<'a>>,
    pub category_totals: CategoryCounts,
    pub development_event_counts: DevelopmentEventCounts,
    /// Distinct source records across included events.
    pub distinct_articles: usize,
    /// Distinct anchored independence classes across included events
    /// (see [`EventSourceCounts::independent_sources`]).
    pub distinct_independent_sources: usize,
    /// Distinct unanchored reprint classes across included events.
    pub unanchored_reprint_classes: usize,
    /// Legal matter ids that appear in more than one included event. Each
    /// event shows its own stages; the kernel does not merge them.
    pub legal_matters_spanning_events: Vec<&'a str>,
    pub coverage: RegionalCoverage,
    pub limits: RegionalLimits,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegionalAccountError {
    Corpus(ClaimCheckError),
    ZeroLimit(RegionalLimitKind),
    BudgetExceeded {
        kind: RegionalLimitKind,
        actual: usize,
        limit: usize,
    },
    DuplicateIdentifier {
        kind: &'static str,
        id: String,
    },
    UnknownReference {
        context: String,
        target: String,
    },
    EmptyField {
        context: String,
        field: &'static str,
    },
    InvalidTimestamp {
        context: String,
        value: String,
    },
    InvalidInterval {
        context: String,
    },
    InvalidLocation {
        candidate_id: String,
        reason: &'static str,
    },
    MissingAttribution {
        candidate_id: String,
    },
    LegalMatterMismatch {
        candidate_id: String,
        reason: &'static str,
    },
    MovementPromotedToPurpose {
        candidate_id: String,
    },
    PurposeNotAttributable {
        candidate_id: String,
        category: EpistemicCategory,
    },
    EmptyEvent {
        event_id: String,
    },
    InvalidConflict {
        conflict_id: String,
        reason: &'static str,
    },
    AllocationFailed,
    Stopped(StopReason),
}

impl fmt::Display for RegionalAccountError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Corpus(err) => write!(f, "corpus verification failed: {err}"),
            Self::ZeroLimit(kind) => write!(f, "{kind:?} limit must be greater than zero"),
            Self::BudgetExceeded {
                kind,
                actual,
                limit,
            } => write!(f, "{kind:?} count/size {actual} exceeds limit {limit}"),
            Self::DuplicateIdentifier { kind, id } => {
                write!(f, "duplicate {kind} identifier: {id}")
            }
            Self::UnknownReference { context, target } => {
                write!(f, "{context} references unknown target {target}")
            }
            Self::EmptyField { context, field } => {
                write!(f, "{context} has an empty {field}")
            }
            Self::InvalidTimestamp { context, value } => write!(
                f,
                "{context} timestamp {value:?} is not a valid YYYY-MM-DDTHH:MM:SSZ UTC instant"
            ),
            Self::InvalidInterval { context } => {
                write!(f, "{context} interval ends before it starts")
            }
            Self::InvalidLocation {
                candidate_id,
                reason,
            } => write!(f, "candidate {candidate_id} location is invalid: {reason}"),
            Self::MissingAttribution { candidate_id } => write!(
                f,
                "candidate {candidate_id} is an attributed statement or allegation without a speaker (NEWS-02)"
            ),
            Self::LegalMatterMismatch {
                candidate_id,
                reason,
            } => write!(f, "candidate {candidate_id}: {reason}"),
            Self::MovementPromotedToPurpose { candidate_id } => write!(
                f,
                "candidate {candidate_id} derives intent, mission, or destination from observed movement (NEWS-02)"
            ),
            Self::PurposeNotAttributable {
                candidate_id,
                category,
            } => write!(
                f,
                "candidate {candidate_id} ({category:?}) cannot carry a stated purpose; only plans, attributed statements, allegations, and judgments may (NEWS-02)"
            ),
            Self::EmptyEvent { event_id } => {
                write!(f, "proposed event {event_id} has no typed candidates")
            }
            Self::InvalidConflict {
                conflict_id,
                reason,
            } => write!(f, "conflict {conflict_id} is invalid: {reason}"),
            Self::AllocationFailed => write!(f, "memory allocation failed"),
            Self::Stopped(reason) => write!(f, "regional account stopped: {reason:?}"),
        }
    }
}

impl Error for RegionalAccountError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Corpus(err) => Some(err),
            _ => None,
        }
    }
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Parses a strict `YYYY-MM-DDTHH:MM:SSZ` instant to Unix seconds, rejecting
/// out-of-range fields. Leap seconds (`:60`) are rejected.
fn parse_utc_second(ts: &str) -> Option<i64> {
    if !is_valid_rfc3339_utc_second(ts) {
        return None;
    }
    let field = |range: std::ops::Range<usize>| ts.get(range)?.parse::<i64>().ok();
    let year = field(0..4)?;
    let month = field(5..7)?;
    let day = field(8..10)?;
    let hour = field(11..13)?;
    let minute = field(14..16)?;
    let second = field(17..19)?;
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    if !(1..=days_in_month).contains(&day) || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    Some(days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second)
}

fn parse_ts(context: &str, value: &str) -> Result<i64, RegionalAccountError> {
    parse_utc_second(value).ok_or_else(|| RegionalAccountError::InvalidTimestamp {
        context: context.to_owned(),
        value: value.to_owned(),
    })
}

fn check_text(
    context: &str,
    field: &'static str,
    value: &str,
    limits: RegionalLimits,
) -> Result<(), RegionalAccountError> {
    if value.trim().is_empty() {
        return Err(RegionalAccountError::EmptyField {
            context: context.to_owned(),
            field,
        });
    }
    if value.len() > limits.max_text_bytes {
        return Err(RegionalAccountError::BudgetExceeded {
            kind: RegionalLimitKind::TextBytes,
            actual: value.len(),
            limit: limits.max_text_bytes,
        });
    }
    Ok(())
}

fn push<T>(vec: &mut Vec<T>, value: T) -> Result<(), RegionalAccountError> {
    vec.try_reserve(1)
        .map_err(|_| RegionalAccountError::AllocationFailed)?;
    vec.push(value);
    Ok(())
}

fn push_unique<T: PartialEq>(vec: &mut Vec<T>, value: T) -> Result<(), RegionalAccountError> {
    if !vec.contains(&value) {
        push(vec, value)?;
    }
    Ok(())
}

/// Refuses a list that is over `limit` or repeats an identifier.
fn check_id_list(
    ids: &[&str],
    kind: &'static str,
    limit_kind: RegionalLimitKind,
    limit: usize,
    check_control: &mut impl FnMut() -> Result<(), StopReason>,
) -> Result<(), RegionalAccountError> {
    if ids.len() > limit {
        return Err(RegionalAccountError::BudgetExceeded {
            kind: limit_kind,
            actual: ids.len(),
            limit,
        });
    }
    for (idx, id) in ids.iter().enumerate() {
        check_control().map_err(RegionalAccountError::Stopped)?;
        if ids.get(..idx).is_some_and(|prior| prior.contains(id)) {
            return Err(RegionalAccountError::DuplicateIdentifier {
                kind,
                id: (*id).to_owned(),
            });
        }
    }
    Ok(())
}

fn record_index(records: &[SourceRecordInput<'_>], version_id: &str) -> Option<usize> {
    records.iter().position(|r| r.version_id == version_id)
}

fn class_find(parent: &mut [usize], mut i: usize) -> usize {
    loop {
        let Some(&p) = parent.get(i) else {
            return i;
        };
        if p == i {
            return i;
        }
        let grandparent = parent.get(p).copied().unwrap_or(p);
        if let Some(slot) = parent.get_mut(i) {
            *slot = grandparent;
        }
        i = grandparent;
    }
}

/// Joins two classes, keeping the lower record index as the representative so
/// the result does not depend on join order.
fn class_union(parent: &mut [usize], a: usize, b: usize) {
    let ra = class_find(parent, a);
    let rb = class_find(parent, b);
    let (low, high) = if ra <= rb { (ra, rb) } else { (rb, ra) };
    if low != high
        && let Some(slot) = parent.get_mut(high)
    {
        *slot = low;
    }
}

fn content_fingerprint(bytes: &[u8]) -> u64 {
    // FNV-1a; only used to skip byte comparisons that cannot match.
    bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Independence classes over every corpus record (see
/// [`EventSourceCounts::independent_sources`]). Returns each record's class
/// representative and, per representative, whether the class holds at least
/// one non-reprint record.
fn independence_classes(
    corpus: &CorpusAssessmentInput<'_>,
    check_control: &mut impl FnMut() -> Result<(), StopReason>,
) -> Result<(Vec<usize>, Vec<bool>), RegionalAccountError> {
    let records = corpus.records;
    let mut parent: Vec<usize> = Vec::new();
    parent
        .try_reserve(records.len())
        .map_err(|_| RegionalAccountError::AllocationFailed)?;
    parent.extend(0..records.len());
    let mut fingerprints: Vec<u64> = Vec::new();
    fingerprints
        .try_reserve(records.len())
        .map_err(|_| RegionalAccountError::AllocationFailed)?;
    for record in records {
        check_control().map_err(RegionalAccountError::Stopped)?;
        fingerprints.push(content_fingerprint(record.raw_bytes));
    }

    for (i, a) in records.iter().enumerate() {
        check_control().map_err(RegionalAccountError::Stopped)?;
        let a_print = fingerprints.get(i).copied();
        for (j, b) in records.iter().enumerate().take(i) {
            let joined = a.record_id == b.record_id
                || a.source_id == b.source_id
                || (!a.independent_lineage_root_id.trim().is_empty()
                    && a.independent_lineage_root_id == b.independent_lineage_root_id)
                || (!a.raw_bytes.is_empty()
                    && a_print == fingerprints.get(j).copied()
                    && a.raw_bytes == b.raw_bytes);
            if joined {
                class_union(&mut parent, i, j);
            }
        }
    }
    for edge in corpus.lineage_edges {
        check_control().map_err(RegionalAccountError::Stopped)?;
        if edge.relation_kind == LineageRelationKind::Supports {
            continue;
        }
        if let (Some(from), Some(to)) = (
            record_index(records, edge.from_version_id),
            record_index(records, edge.to_version_id),
        ) {
            class_union(&mut parent, from, to);
        }
    }

    let mut class_of: Vec<usize> = Vec::new();
    class_of
        .try_reserve(records.len())
        .map_err(|_| RegionalAccountError::AllocationFailed)?;
    let mut anchored: Vec<bool> = Vec::new();
    anchored
        .try_reserve(records.len())
        .map_err(|_| RegionalAccountError::AllocationFailed)?;
    anchored.resize(records.len(), false);
    for (i, record) in records.iter().enumerate() {
        check_control().map_err(RegionalAccountError::Stopped)?;
        let class = class_find(&mut parent, i);
        class_of.push(class);
        if !record.is_syndicated_reprint
            && let Some(slot) = anchored.get_mut(class)
        {
            *slot = true;
        }
    }
    Ok((class_of, anchored))
}

/// True when a later legal stage (by rank) has a reported start before an
/// earlier-rank stage's reported start. Stages without a reported time are
/// not compared.
fn legal_chronology_conflict(stages: &[LegalStageEntry<'_>]) -> bool {
    let start =
        |s: &LegalStageEntry<'_>| s.reported_time.and_then(|t| parse_utc_second(t.start_utc));
    let rank = |s: &LegalStageEntry<'_>| s.category.legal_stage_rank();
    stages.iter().any(|later| {
        stages.iter().any(|earlier| {
            rank(earlier) < rank(later)
                && matches!((start(earlier), start(later)), (Some(e), Some(l)) if l < e)
        })
    })
}

fn validate_location(
    candidate_id: &str,
    location: ReportedLocation,
) -> Result<(), RegionalAccountError> {
    let invalid = |reason| RegionalAccountError::InvalidLocation {
        candidate_id: candidate_id.to_owned(),
        reason,
    };
    match (location.basis, location.uncertainty_m) {
        (LocationBasis::NotReported, None) => Ok(()),
        (LocationBasis::NotReported, Some(_)) => Err(invalid(
            "an unreported location cannot carry an uncertainty",
        )),
        (_, None) => Err(invalid("a reported location must state its uncertainty")),
        (_, Some(value)) if !value.is_finite() || value <= 0.0 => {
            Err(invalid("uncertainty must be finite and positive"))
        }
        (_, Some(_)) => Ok(()),
    }
}

/// Earliest reported start, the event account, and its anchored and
/// unanchored independence classes, kept until region totals are computed.
type PendingEvent<'a> = (Option<i64>, EventAccount<'a>, Vec<usize>, Vec<usize>);

struct ValidatedEntry<'a> {
    entry: AccountEntry<'a>,
    event_index: usize,
    start_s: Option<i64>,
}

/// Builds a regional account over a corpus that must pass [`verify_corpus`].
///
/// Events are included when their region matches and at least one entry's
/// reported time (widened by its stated uncertainty) overlaps the window or has
/// no reported time. An included event is shown whole; each entry records how
/// it relates to the window.
pub fn build_regional_account<'a>(
    corpus: &CorpusAssessmentInput<'a>,
    assessment_limits: AssessmentLimits,
    input: &RegionalAccountInput<'a>,
    limits: RegionalLimits,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<RegionalAccount<'a>, RegionalAccountError> {
    check_control().map_err(RegionalAccountError::Stopped)?;
    verify_corpus(corpus, assessment_limits, &mut check_control).map_err(|err| match err {
        ClaimCheckError::Stopped(reason) => RegionalAccountError::Stopped(reason),
        other => RegionalAccountError::Corpus(other),
    })?;

    if input.events.len() > limits.max_events {
        return Err(RegionalAccountError::BudgetExceeded {
            kind: RegionalLimitKind::Events,
            actual: input.events.len(),
            limit: limits.max_events,
        });
    }
    if input.candidates.len() > limits.max_candidates {
        return Err(RegionalAccountError::BudgetExceeded {
            kind: RegionalLimitKind::Candidates,
            actual: input.candidates.len(),
            limit: limits.max_candidates,
        });
    }
    check_text("regional account", "region_id", input.region_id, limits)?;
    let window_start = parse_ts("account window start", input.window_start_utc)?;
    let window_end = parse_ts("account window end", input.window_end_utc)?;
    if window_end < window_start {
        return Err(RegionalAccountError::InvalidInterval {
            context: "account window".to_owned(),
        });
    }

    // Events.
    for (idx, event) in input.events.iter().enumerate() {
        check_control().map_err(RegionalAccountError::Stopped)?;
        let context = format!("event {}", event.event_id);
        check_text(&context, "event_id", event.event_id, limits)?;
        check_text(&context, "version_id", event.version_id, limits)?;
        check_text(&context, "region_id", event.region_id, limits)?;
        check_text(
            &context,
            "grouping_decision_id",
            event.grouping_decision_id,
            limits,
        )?;
        if input
            .events
            .get(..idx)
            .is_some_and(|prior| prior.iter().any(|e| e.event_id == event.event_id))
        {
            return Err(RegionalAccountError::DuplicateIdentifier {
                kind: "event_id",
                id: event.event_id.to_owned(),
            });
        }
        if event.conflicts.len() > limits.max_conflicts_per_event {
            return Err(RegionalAccountError::BudgetExceeded {
                kind: RegionalLimitKind::ConflictsPerEvent,
                actual: event.conflicts.len(),
                limit: limits.max_conflicts_per_event,
            });
        }
        check_id_list(
            event.additional_member_version_ids,
            "additional_member_version_id",
            RegionalLimitKind::MembersPerEvent,
            limits.max_members_per_event,
            &mut check_control,
        )?;
        for version_id in event.additional_member_version_ids {
            check_control().map_err(RegionalAccountError::Stopped)?;
            if find_record(corpus.records, version_id).is_none() {
                return Err(RegionalAccountError::UnknownReference {
                    context: context.clone(),
                    target: (*version_id).to_owned(),
                });
            }
        }
        for conflict in event.conflicts {
            check_id_list(
                conflict.conflicting_version_ids,
                "conflicting_version_id",
                RegionalLimitKind::MembersPerEvent,
                limits.max_members_per_event,
                &mut check_control,
            )?;
            check_id_list(
                conflict.corrected_or_authoritative_version_ids,
                "corrected_or_authoritative_version_id",
                RegionalLimitKind::MembersPerEvent,
                limits.max_members_per_event,
                &mut check_control,
            )?;
        }
    }

    // Candidates.
    let mut validated: Vec<ValidatedEntry<'a>> = Vec::new();
    validated
        .try_reserve(input.candidates.len())
        .map_err(|_| RegionalAccountError::AllocationFailed)?;
    for (idx, cand) in input.candidates.iter().enumerate() {
        check_control().map_err(RegionalAccountError::Stopped)?;
        let context = format!("candidate {}", cand.candidate_id);
        check_text(&context, "candidate_id", cand.candidate_id, limits)?;
        if input
            .candidates
            .get(..idx)
            .is_some_and(|prior| prior.iter().any(|c| c.candidate_id == cand.candidate_id))
        {
            return Err(RegionalAccountError::DuplicateIdentifier {
                kind: "regional candidate_id",
                id: cand.candidate_id.to_owned(),
            });
        }
        let Some(base) = corpus
            .candidates
            .iter()
            .find(|c| c.candidate_id == cand.candidate_id)
        else {
            return Err(RegionalAccountError::UnknownReference {
                context: "regional account".to_owned(),
                target: cand.candidate_id.to_owned(),
            });
        };
        let Some(event_index) = input
            .events
            .iter()
            .position(|e| e.event_id == cand.proposed_event_id)
        else {
            return Err(RegionalAccountError::UnknownReference {
                context: context.clone(),
                target: cand.proposed_event_id.to_owned(),
            });
        };
        let record: &SourceRecordInput<'a> =
            find_record(corpus.records, base.source_record_version_id).ok_or_else(|| {
                RegionalAccountError::UnknownReference {
                    context: context.clone(),
                    target: base.source_record_version_id.to_owned(),
                }
            })?;

        if cand.supporting_passage_ids.is_empty() {
            return Err(RegionalAccountError::EmptyField {
                context,
                field: "supporting_passage_ids",
            });
        }
        check_id_list(
            cand.supporting_passage_ids,
            "supporting_passage_id",
            RegionalLimitKind::PassagesPerCandidate,
            limits.max_passages_per_candidate,
            &mut check_control,
        )?;
        for passage_id in cand.supporting_passage_ids {
            if find_passage(record, passage_id).is_none() {
                return Err(RegionalAccountError::UnknownReference {
                    context: format!("{context} in {}", record.version_id),
                    target: (*passage_id).to_owned(),
                });
            }
        }

        match cand.attributed_speaker {
            Some(speaker) => check_text(&context, "attributed_speaker", speaker, limits)?,
            None if cand.category.requires_attributed_speaker() => {
                return Err(RegionalAccountError::MissingAttribution {
                    candidate_id: cand.candidate_id.to_owned(),
                });
            }
            None => {}
        }

        match (cand.category.legal_stage_rank(), cand.legal_matter_id) {
            (Some(_), Some(matter)) => check_text(&context, "legal_matter_id", matter, limits)?,
            (Some(_), None) => {
                return Err(RegionalAccountError::LegalMatterMismatch {
                    candidate_id: cand.candidate_id.to_owned(),
                    reason: "arrests, charges, and judgments must name a legal matter",
                });
            }
            (None, Some(_)) => {
                return Err(RegionalAccountError::LegalMatterMismatch {
                    candidate_id: cand.candidate_id.to_owned(),
                    reason: "only arrests, charges, and judgments may name a legal matter",
                });
            }
            (None, None) => {}
        }

        let purpose = match cand.stated_purpose {
            None => None,
            Some(stated) => {
                if !cand.category.may_carry_stated_purpose() {
                    if cand.category == EpistemicCategory::Observation && cand.reports_movement {
                        return Err(RegionalAccountError::MovementPromotedToPurpose {
                            candidate_id: cand.candidate_id.to_owned(),
                        });
                    }
                    return Err(RegionalAccountError::PurposeNotAttributable {
                        candidate_id: cand.candidate_id.to_owned(),
                        category: cand.category,
                    });
                }
                check_text(&context, "stated purpose text", stated.text, limits)?;
                if find_passage(record, stated.passage_id).is_none() {
                    return Err(RegionalAccountError::UnknownReference {
                        context: format!("{context} stated purpose in {}", record.version_id),
                        target: stated.passage_id.to_owned(),
                    });
                }
                Some(AttributedPurpose {
                    kind: stated.kind,
                    text: stated.text,
                    passage_id: stated.passage_id,
                    attributed_to: cand.attributed_speaker.unwrap_or(record.source_id),
                    attribution_basis: if cand.attributed_speaker.is_some() {
                        AttributionBasis::NamedSpeaker
                    } else {
                        AttributionBasis::IssuingSource
                    },
                    category: cand.category,
                })
            }
        };

        validate_location(cand.candidate_id, cand.location)?;

        let (window_overlap, start_s) = match cand.reported_time {
            None => (WindowOverlap::TimeNotReported, None),
            Some(interval) => {
                let start = parse_ts(&context, interval.start_utc)?;
                let end = parse_ts(&context, interval.end_utc)?;
                if end < start {
                    return Err(RegionalAccountError::InvalidInterval { context });
                }
                let widen = i64::from(interval.uncertainty_s);
                let overlap = if start <= window_end && end >= window_start {
                    WindowOverlap::ReportedBounds
                } else if start.saturating_sub(widen) <= window_end
                    && end.saturating_add(widen) >= window_start
                {
                    WindowOverlap::UncertaintyOnly
                } else {
                    WindowOverlap::Outside
                };
                (overlap, Some(start))
            }
        };

        let entry = AccountEntry {
            candidate_id: cand.candidate_id,
            source_record_version_id: record.version_id,
            source_id: record.source_id,
            category: cand.category,
            development_kind: cand.development_kind,
            attributed_speaker: cand.attributed_speaker,
            supporting_passage_ids: cand.supporting_passage_ids,
            reported_time: cand.reported_time,
            window_overlap,
            location: cand.location,
            reports_movement: cand.reports_movement,
            purpose,
            legal_matter_id: cand.legal_matter_id,
            review_status: base.review_status,
            needs_reassessment: base.needs_reassessment || record.needs_reassessment,
            record_superseded: record.superseded_by_version_id.is_some(),
            from_syndicated_reprint: record.is_syndicated_reprint,
        };
        validated.push(ValidatedEntry {
            entry,
            event_index,
            start_s,
        });
    }

    // Assemble events.
    let mut coverage = RegionalCoverage {
        events_considered: input.events.len(),
        ..RegionalCoverage::default()
    };
    for base in corpus.candidates {
        check_control().map_err(RegionalAccountError::Stopped)?;
        if !input
            .candidates
            .iter()
            .any(|c| c.candidate_id == base.candidate_id)
        {
            coverage.corpus_candidates_untyped =
                coverage.corpus_candidates_untyped.saturating_add(1);
        }
    }
    let (class_of, class_anchored) = independence_classes(corpus, &mut check_control)?;
    let mut events: Vec<PendingEvent<'a>> = Vec::new();
    for (event_index, event) in input.events.iter().enumerate() {
        check_control().map_err(RegionalAccountError::Stopped)?;
        let mut members: Vec<&ValidatedEntry<'a>> = validated
            .iter()
            .filter(|v| v.event_index == event_index)
            .collect();
        if members.is_empty() {
            return Err(RegionalAccountError::EmptyEvent {
                event_id: event.event_id.to_owned(),
            });
        }

        members.sort_by(|a, b| {
            let a_key = (a.start_s.is_none(), a.start_s, a.entry.category);
            let b_key = (b.start_s.is_none(), b.start_s, b.entry.category);
            a_key
                .cmp(&b_key)
                .then_with(|| a.entry.candidate_id.cmp(b.entry.candidate_id))
        });

        let mut member_versions: Vec<&'a str> = Vec::new();
        let mut entries: Vec<AccountEntry<'a>> = Vec::new();
        entries
            .try_reserve(members.len())
            .map_err(|_| RegionalAccountError::AllocationFailed)?;
        let mut counts = EventSourceCounts::default();
        for v in &members {
            push_unique(&mut member_versions, v.entry.source_record_version_id)?;
            counts.by_category.add(v.entry.category);
            entries.push(v.entry.clone());
        }
        for version_id in event.additional_member_version_ids {
            push_unique(&mut member_versions, version_id)?;
        }
        if member_versions.len() > limits.max_members_per_event {
            return Err(RegionalAccountError::BudgetExceeded {
                kind: RegionalLimitKind::MembersPerEvent,
                actual: member_versions.len(),
                limit: limits.max_members_per_event,
            });
        }

        let mut record_ids: Vec<&'a str> = Vec::new();
        let mut roots: Vec<&'a str> = Vec::new();
        let mut anchored_classes: Vec<usize> = Vec::new();
        let mut unanchored_classes: Vec<usize> = Vec::new();
        let mut needs_reassessment = entries.iter().any(|e| e.needs_reassessment);
        for version_id in &member_versions {
            check_control().map_err(RegionalAccountError::Stopped)?;
            let Some((record, class)) = record_index(corpus.records, version_id)
                .and_then(|i| Some((corpus.records.get(i)?, class_of.get(i).copied()?)))
            else {
                return Err(RegionalAccountError::UnknownReference {
                    context: format!("event {}", event.event_id),
                    target: (*version_id).to_owned(),
                });
            };
            if record.independent_lineage_root_id.trim().is_empty() {
                return Err(RegionalAccountError::EmptyField {
                    context: format!("member record {}", record.version_id),
                    field: "independent_lineage_root_id",
                });
            }
            push_unique(&mut record_ids, record.record_id)?;
            push_unique(&mut roots, record.independent_lineage_root_id)?;
            if class_anchored.get(class).copied().unwrap_or(false) {
                push_unique(&mut anchored_classes, class)?;
            } else {
                push_unique(&mut unanchored_classes, class)?;
            }
            if record.is_syndicated_reprint {
                counts.syndicated_reprint_versions =
                    counts.syndicated_reprint_versions.saturating_add(1);
            }
            if record.superseded_by_version_id.is_some() {
                counts.superseded_versions = counts.superseded_versions.saturating_add(1);
                needs_reassessment = true;
            }
            needs_reassessment |= record.needs_reassessment;
        }
        counts.articles = record_ids.len();
        counts.article_versions = member_versions.len();
        counts.entries = entries.len();
        counts.independent_sources = anchored_classes.len();
        counts.unanchored_reprint_classes = unanchored_classes.len();
        counts.declared_lineage_roots = roots.len();

        for conflict in event.conflicts {
            check_control().map_err(RegionalAccountError::Stopped)?;
            let context = format!("conflict {}", conflict.conflict_id);
            check_text(&context, "conflict_id", conflict.conflict_id, limits)?;
            check_text(&context, "description", conflict.description, limits)?;
            if conflict.conflicting_version_ids.is_empty() {
                return Err(RegionalAccountError::InvalidConflict {
                    conflict_id: conflict.conflict_id.to_owned(),
                    reason: "a conflict must name at least one conflicting version",
                });
            }
            for version_id in conflict
                .conflicting_version_ids
                .iter()
                .chain(conflict.corrected_or_authoritative_version_ids)
            {
                check_control().map_err(RegionalAccountError::Stopped)?;
                if !member_versions.contains(version_id) {
                    return Err(RegionalAccountError::InvalidConflict {
                        conflict_id: conflict.conflict_id.to_owned(),
                        reason: "every referenced version must be a member of the event",
                    });
                }
            }
            if conflict.resolution == ConflictResolution::ResolvedByCorrectionHistoryPreserved
                && conflict.corrected_or_authoritative_version_ids.is_empty()
            {
                return Err(RegionalAccountError::InvalidConflict {
                    conflict_id: conflict.conflict_id.to_owned(),
                    reason: "a correction-resolved conflict must name the correcting versions",
                });
            }
        }

        // Exclusion is decided only after the event's input has been validated.
        if event.region_id != input.region_id {
            coverage.events_outside_region = coverage.events_outside_region.saturating_add(1);
            continue;
        }
        if entries
            .iter()
            .all(|e| e.window_overlap == WindowOverlap::Outside)
        {
            coverage.events_outside_window = coverage.events_outside_window.saturating_add(1);
            continue;
        }

        let mut legal_matters: Vec<LegalMatterAccount<'a>> = Vec::new();
        for entry in &entries {
            let Some(matter_id) = entry.legal_matter_id else {
                continue;
            };
            let stage = LegalStageEntry {
                category: entry.category,
                candidate_id: entry.candidate_id,
                source_record_version_id: entry.source_record_version_id,
                reported_time: entry.reported_time,
                review_status: entry.review_status,
                needs_reassessment: entry.needs_reassessment,
                record_superseded: entry.record_superseded,
            };
            if let Some(matter) = legal_matters.iter_mut().find(|m| m.matter_id == matter_id) {
                push(&mut matter.stages, stage)?;
            } else {
                let mut stages = Vec::new();
                push(&mut stages, stage)?;
                push(
                    &mut legal_matters,
                    LegalMatterAccount {
                        matter_id,
                        stages,
                        latest_reported_stage: entry.category,
                        judgment_reported: false,
                        chronology_conflict: false,
                    },
                )?;
            }
        }
        for matter in &mut legal_matters {
            check_control().map_err(RegionalAccountError::Stopped)?;
            // Stable sort keeps the chronological entry order within a stage.
            matter
                .stages
                .sort_by_key(|s| s.category.legal_stage_rank().unwrap_or(u8::MAX));
            if let Some(last) = matter.stages.last() {
                matter.latest_reported_stage = last.category;
            }
            matter.judgment_reported = matter
                .stages
                .iter()
                .any(|s| s.category == EpistemicCategory::Judgment);
            matter.chronology_conflict = legal_chronology_conflict(&matter.stages);
        }

        let mut approximate_location_present = false;
        let mut tightest: Option<f64> = None;
        let mut widest: Option<f64> = None;
        for entry in &entries {
            approximate_location_present |=
                entry.location.basis == LocationBasis::GazetteerCentroid;
            if let Some(u) = entry.location.uncertainty_m {
                tightest = Some(tightest.map_or(u, |t| t.min(u)));
                widest = Some(widest.map_or(u, |w| w.max(u)));
            }
            coverage.entries_included = coverage.entries_included.saturating_add(1);
            match entry.window_overlap {
                WindowOverlap::TimeNotReported => {
                    coverage.entries_time_not_reported =
                        coverage.entries_time_not_reported.saturating_add(1);
                }
                WindowOverlap::UncertaintyOnly => {
                    coverage.entries_uncertainty_only =
                        coverage.entries_uncertainty_only.saturating_add(1);
                }
                WindowOverlap::Outside => {
                    coverage.entries_outside_window_in_included_events = coverage
                        .entries_outside_window_in_included_events
                        .saturating_add(1);
                }
                WindowOverlap::ReportedBounds => {}
            }
            if entry.needs_reassessment {
                coverage.entries_needing_reassessment =
                    coverage.entries_needing_reassessment.saturating_add(1);
            }
            if entry.review_status != ReviewStatus::Reviewed {
                coverage.entries_not_reviewed = coverage.entries_not_reviewed.saturating_add(1);
            }
        }

        let earliest = members.iter().filter_map(|v| v.start_s).min();
        push(
            &mut events,
            (
                earliest,
                EventAccount {
                    event_id: event.event_id,
                    version_id: event.version_id,
                    grouping_decision_id: event.grouping_decision_id,
                    entries,
                    member_version_ids: member_versions,
                    declared_lineage_roots: roots,
                    counts,
                    legal_matters,
                    conflicts: event.conflicts,
                    approximate_location_present,
                    tightest_location_uncertainty_m: tightest,
                    widest_location_uncertainty_m: widest,
                    needs_reassessment,
                },
                anchored_classes,
                unanchored_classes,
            ),
        )?;
    }

    events.sort_by(|(a_start, a, ..), (b_start, b, ..)| {
        (a_start.is_none(), a_start)
            .cmp(&(b_start.is_none(), b_start))
            .then_with(|| a.event_id.cmp(b.event_id))
    });

    let mut category_totals = CategoryCounts::default();
    let mut development_event_counts = DevelopmentEventCounts::default();
    let mut region_records: Vec<&'a str> = Vec::new();
    let mut region_anchored: Vec<usize> = Vec::new();
    let mut region_unanchored: Vec<usize> = Vec::new();
    let mut matters_seen: Vec<&'a str> = Vec::new();
    let mut matters_spanning: Vec<&'a str> = Vec::new();
    let mut out_events = Vec::new();
    out_events
        .try_reserve(events.len())
        .map_err(|_| RegionalAccountError::AllocationFailed)?;
    for (_, event, anchored, unanchored) in events {
        check_control().map_err(RegionalAccountError::Stopped)?;
        let mut kinds: Vec<DevelopmentKind> = Vec::new();
        for entry in &event.entries {
            category_totals.add(entry.category);
            if !kinds.contains(&entry.development_kind) {
                push(&mut kinds, entry.development_kind)?;
                development_event_counts.add(entry.development_kind);
            }
        }
        for version_id in &event.member_version_ids {
            if let Some(record) = find_record(corpus.records, version_id) {
                push_unique(&mut region_records, record.record_id)?;
            }
        }
        for class in anchored {
            push_unique(&mut region_anchored, class)?;
        }
        for class in unanchored {
            push_unique(&mut region_unanchored, class)?;
        }
        for matter in &event.legal_matters {
            if matters_seen.contains(&matter.matter_id) {
                push_unique(&mut matters_spanning, matter.matter_id)?;
            } else {
                push(&mut matters_seen, matter.matter_id)?;
            }
        }
        out_events.push(event);
    }
    coverage.events_included = out_events.len();

    Ok(RegionalAccount {
        region_id: input.region_id,
        window_start_utc: input.window_start_utc,
        window_end_utc: input.window_end_utc,
        events: out_events,
        category_totals,
        development_event_counts,
        distinct_articles: region_records.len(),
        distinct_independent_sources: region_anchored.len(),
        unanchored_reprint_classes: region_unanchored.len(),
        legal_matters_spanning_events: matters_spanning,
        coverage,
        limits,
    })
}

#[cfg(test)]
mod tests {
    use super::parse_utc_second;

    #[test]
    fn utc_parser_matches_known_instants_and_rejects_invalid_fields() {
        assert_eq!(parse_utc_second("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(
            parse_utc_second("2024-03-15T19:12:00Z"),
            Some(1_710_529_920)
        );
        assert_eq!(
            parse_utc_second("2024-02-29T00:00:00Z"),
            Some(1_709_164_800)
        );
        for bad in [
            "2023-02-29T00:00:00Z",
            "2024-13-01T00:00:00Z",
            "2024-03-15T24:00:00Z",
            "2024-03-15T23:59:60Z",
            "2024-03-15T19:12:00+00:00",
        ] {
            assert_eq!(parse_utc_second(bad), None, "{bad}");
        }
    }
}
