//! Immutable forecast issuance, point-in-time cutoff admissibility, linked revisions,
//! independent outcome resolution versions, and reassessment-aware cohort evaluation.
//!
//! This module implements the bounded lifecycle kernel for `FORECAST-04`, `MARKET-04`,
//! and `MARKET-05` toward scenarios `A14` and `A15` (`docs/architecture.md` §§2.2, 3.1, 6.4).
//! It does not persist to SQLite, fit calibration models, select `D12` production datasets,
//! or claim formal Lean proof coverage.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use oia_domain::{ActorId, OpaqueId, SchemaVersion, VersionId};

use crate::scoring::{
    BinaryForecastRow, BinaryOutcome, BrierScores, MAX_EVALUATION_ROWS, Probability, ScoringError,
    UnresolvedReason, evaluate_binary,
};

/// Maximum number of forecast vintages, resolutions, or evaluation runs in a single bounded ledger.
pub const MAX_LEDGER_RECORDS: usize = 10_000;

/// Maximum number of source inputs, comparison entries, or assumptions on a single forecast vintage.
pub const MAX_FORECAST_COLLECTION_LEN: usize = 1_024;

/// Maximum byte length of a free-text proposition, rule, assumption, or rationale field.
pub const MAX_TEXT_FIELD_BYTES: usize = 16_384;

/// How a forecast's information cutoff is enforced (`docs/architecture.md` §3.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CutoffAdmissibilityMode {
    /// Only source versions both publicly available AND ingested into OIA at or before
    /// `information_cutoff_utc_ms` qualify. Unknown timestamps or post-cutoff model
    /// knowledge are rejected.
    ProspectiveStrict,
    /// Historical reconstruction may admit externally documented public availability at or
    /// before `information_cutoff_utc_ms` even when OIA ingested the record later, provided
    /// an explicit non-empty limitation note is recorded. Unknown public availability is
    /// still rejected.
    RetrospectiveReconstructed {
        /// Mandatory explanation of retrospective reconstruction limits.
        limitation_note: String,
    },
}

/// Source or feature version cited by a forecast vintage, with distinct public-availability
/// and OIA-ingestion timestamps (`docs/architecture.md` §3.1, `MARKET-04`, `MARKET-05`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CutoffSourceInput {
    /// Immutable version of the source record, release, or feature snapshot.
    pub source_version: VersionId,
    /// Historical event or observation timestamp in UTC milliseconds, if applicable.
    pub event_utc_ms: Option<i64>,
    /// First documented public availability or release timestamp in UTC milliseconds.
    /// `None` means public availability is unknown and cannot satisfy a cutoff check.
    pub public_available_at_utc_ms: Option<i64>,
    /// Timestamp when this exact version was ingested into OIA in UTC milliseconds.
    /// `None` means OIA ingestion time is unrecorded.
    pub ingested_at_utc_ms: Option<i64>,
}

/// Frozen outcome definition and resolution criteria (`FORECAST-04`, `docs/architecture.md` §6.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutcomeDefinition {
    /// Stable identifier of the outcome target across revisions and resolution versions.
    pub outcome_id: OpaqueId,
    /// Exact binary proposition being forecast.
    pub proposition: String,
    /// Explicit rules and sources used to resolve occurrence, non-occurrence, or censorship.
    pub resolution_rules: String,
}

/// Mandatory explanation and new evidence links when issuing a revised forecast vintage
/// (`FORECAST-04`: "New evidence must explain revisions without rewriting previous predictions").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevisionRationale {
    /// Immutable version IDs of new or corrected evidence motivating the revision.
    /// Every version listed here must appear in the revision's admissible `sources`.
    pub new_evidence_versions: Vec<VersionId>,
    /// Human-inspectable explanation of why the forecast changed.
    pub summary: String,
}

/// Input specification for issuing an initial or revised binary forecast vintage.
#[derive(Debug, Clone, PartialEq)]
pub struct ForecastDraft {
    /// Logical forecast series identifier shared by an original forecast and its revisions.
    pub forecast_id: OpaqueId,
    /// Globally unique, immutable version identifier for this specific vintage.
    pub version_id: VersionId,
    /// Schema version of the forecast record.
    pub schema_version: SchemaVersion,
    /// Actor or process issuing this vintage.
    pub actor: ActorId,
    /// Timestamp when this vintage was recorded in UTC milliseconds.
    pub issued_at_utc_ms: i64,
    /// Forecast origin timestamp in UTC milliseconds (`information_cutoff_utc_ms <= origin_utc_ms`).
    pub origin_utc_ms: i64,
    /// Strict information cutoff timestamp in UTC milliseconds.
    pub information_cutoff_utc_ms: i64,
    /// Inclusive start of the target horizon `[horizon_start_utc_ms, horizon_end_utc_ms)`.
    pub horizon_start_utc_ms: i64,
    /// Exclusive end of the target horizon (`horizon_start_utc_ms < horizon_end_utc_ms`).
    pub horizon_end_utc_ms: i64,
    /// Cutoff enforcement mode (`ProspectiveStrict` vs `RetrospectiveReconstructed`).
    pub cutoff_mode: CutoffAdmissibilityMode,
    /// Target entity, region, or market instrument identifier.
    pub target_id: OpaqueId,
    /// Frozen outcome definition and resolution criteria.
    pub outcome_definition: OutcomeDefinition,
    /// Pinned baseline model/method version for the same target and horizon.
    pub baseline_version: VersionId,
    /// Validated baseline probability in `0.0..=1.0`.
    pub baseline_probability: Probability,
    /// Validated forecast probability in `0.0..=1.0`.
    pub forecast_probability: Probability,
    /// Pinned forecasting model or method version.
    pub model_version: VersionId,
    /// Optional model training/knowledge cutoff timestamp in UTC milliseconds (`docs/architecture.md` §6.4).
    pub model_knowledge_cutoff_utc_ms: Option<i64>,
    /// Admissible source and feature versions used at issuance.
    pub sources: Vec<CutoffSourceInput>,
    /// Explicit historical comparison or analogue episode versions (`FORECAST-02`, `FORECAST-04`).
    pub comparison_set: Vec<VersionId>,
    /// Explicit scenario or modeling assumptions frozen at issuance.
    pub assumptions: Vec<String>,
    /// Predecessor forecast vintage when issuing a revision.
    pub predecessor_version: Option<VersionId>,
    /// Revision rationale required if and only if `predecessor_version` is `Some`.
    pub revision_rationale: Option<RevisionRationale>,
}

/// Immutable issued forecast vintage stored in a [`ForecastLedger`].
#[derive(Debug, Clone, PartialEq)]
pub struct ForecastVintageRecord {
    pub forecast_id: OpaqueId,
    pub version_id: VersionId,
    pub vintage_number: usize,
    pub schema_version: SchemaVersion,
    pub actor: ActorId,
    pub issued_at_utc_ms: i64,
    pub origin_utc_ms: i64,
    pub information_cutoff_utc_ms: i64,
    pub horizon_start_utc_ms: i64,
    pub horizon_end_utc_ms: i64,
    pub cutoff_mode: CutoffAdmissibilityMode,
    pub target_id: OpaqueId,
    pub outcome_definition: OutcomeDefinition,
    pub baseline_version: VersionId,
    pub baseline_probability: Probability,
    pub forecast_probability: Probability,
    pub model_version: VersionId,
    pub model_knowledge_cutoff_utc_ms: Option<i64>,
    /// True when retrospective mode admitted a model whose knowledge cutoff exceeds
    /// `information_cutoff_utc_ms` (`docs/architecture.md` §6.4).
    pub post_cutoff_model_knowledge_disclosed: bool,
    pub sources: Vec<CutoffSourceInput>,
    pub comparison_set: Vec<VersionId>,
    pub assumptions: Vec<String>,
    pub predecessor_version: Option<VersionId>,
    pub revision_rationale: Option<RevisionRationale>,
}

/// Outcome resolution state keeping resolved, unresolved/censored, and invalidated outcomes
/// distinct (`docs/architecture.md` §6.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionState {
    /// Defined event resolved as occurred (`true`) or did not occur (`false`), with observed
    /// timestamp in UTC milliseconds (`None` when confirmed non-occurrence has no point instant).
    Resolved {
        occurred: bool,
        observed_event_utc_ms: Option<i64>,
    },
    /// Outcome remains pending, censored by incomplete follow-up, or disputed.
    Unresolved(UnresolvedReason),
    /// Outcome or scenario preconditions were invalidated; never coerced to `occurred: false`
    /// or silently omitted from cohort accounting.
    Invalidated { reason: String },
}

/// Independent outcome resolution version (`docs/architecture.md` §6.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutcomeResolutionRecord {
    /// Logical resolution series identifier.
    pub resolution_id: OpaqueId,
    /// Immutable version identifier for this resolution snapshot.
    pub version_id: VersionId,
    /// Outcome definition identifier (`OutcomeDefinition::outcome_id`) resolved by this record.
    pub outcome_id: OpaqueId,
    /// Actor or process recording the resolution.
    pub actor: ActorId,
    /// Timestamp when this resolution version was recorded in UTC milliseconds.
    pub recorded_at_utc_ms: i64,
    /// Supporting source or observation versions establishing the resolution state.
    pub evidence_versions: Vec<VersionId>,
    /// Resolution state (`Resolved`, `Unresolved`, or `Invalidated`).
    pub state: ResolutionState,
    /// Prior resolution `VersionId` superseded by a later correction (`docs/architecture.md` §6.4).
    pub supersedes_version: Option<VersionId>,
    /// Mandatory explanation when `supersedes_version` is `Some`.
    pub correction_rationale: Option<String>,
}

/// How an observed event timestamp compares with a forecast vintage's target horizon
/// `[horizon_start_utc_ms, horizon_end_utc_ms)` (`FORECAST-04`, `A15`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HorizonTiming {
    /// Event occurred inside `[horizon_start_utc_ms, horizon_end_utc_ms)`; timing error is `0`.
    WithinHorizon,
    /// Event occurred before `horizon_start_utc_ms` by `early_by_ms` milliseconds.
    Early { early_by_ms: i64 },
    /// Event occurred at or after `horizon_end_utc_ms` by `late_by_ms` milliseconds.
    Late { late_by_ms: i64 },
}

/// Per-vintage evaluation outcome within an [`EvaluationRunRecord`].
#[derive(Debug, Clone, PartialEq)]
pub struct EvaluatedVintageSummary {
    pub forecast_id: OpaqueId,
    pub forecast_version: VersionId,
    pub vintage_number: usize,
    pub predecessor_version: Option<VersionId>,
    pub baseline_version: VersionId,
    pub resolution_version: VersionId,
    pub forecast_probability: Probability,
    pub baseline_probability: Probability,
    pub resolution_state: ResolutionState,
    /// Present for `ResolutionState::Resolved`; `None` for `Unresolved` or `Invalidated`.
    pub brier_scores: Option<BrierScores>,
    /// Present when `ResolutionState::Resolved` supplies an `observed_event_utc_ms`.
    pub horizon_timing: Option<HorizonTiming>,
    /// True if the forecast vintage disclosed post-cutoff model knowledge in retrospective mode.
    pub post_cutoff_model_knowledge_disclosed: bool,
}

/// Why a previously issued [`EvaluationRunRecord`] requires reassessment (`docs/architecture.md` §§2.3, 6.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReassessmentReason {
    /// A pinned outcome resolution version was superseded by a corrected resolution version.
    OutcomeResolutionSuperseded {
        outcome_id: OpaqueId,
        previous_resolution_version: VersionId,
        new_resolution_version: VersionId,
    },
}

/// Immutable evaluation run pinning exact forecast, baseline, and outcome resolution versions.
///
/// Later outcome corrections do not mutate `vintages` or `mean_scores`; they append a
/// [`ReassessmentReason`] so callers can issue a new evaluation run while preserving history.
#[derive(Debug, Clone, PartialEq)]
pub struct EvaluationRunRecord {
    pub evaluation_version: VersionId,
    pub evaluated_at_utc_ms: i64,
    pub vintages: Vec<EvaluatedVintageSummary>,
    pub resolved_count: usize,
    pub unresolved_count: usize,
    pub invalidated_count: usize,
    pub mean_scores: Option<BrierScores>,
    pub needs_reassessment: bool,
    pub reassessment_reasons: Vec<ReassessmentReason>,
}

/// Typed errors for forecast issuance, revision, outcome resolution, and cohort evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssuanceError {
    /// Ledger capacity or per-record collection budget was zero or exceeded bounds.
    BudgetExceeded {
        field: &'static str,
        actual: usize,
        limit: usize,
    },
    /// A required text field was empty or whitespace-only, or exceeded [`MAX_TEXT_FIELD_BYTES`].
    InvalidTextField {
        field: &'static str,
        reason: &'static str,
    },
    /// A version identifier was already used by another record in the ledger.
    DuplicateVersionId(VersionId),
    /// Initial forecast series already exists; subsequent vintages must use revision semantics.
    ForecastSeriesAlreadyExists(OpaqueId),
    /// Referenced forecast series or predecessor vintage does not exist.
    UnknownForecastPredecessor {
        forecast_id: OpaqueId,
        predecessor_version: VersionId,
    },
    /// Revision predecessor is not the latest vintage in the series, or does not belong to `forecast_id`.
    NonLinearRevisionPredecessor {
        forecast_id: OpaqueId,
        supplied_predecessor: VersionId,
        expected_latest: VersionId,
    },
    /// Initial issuance supplied revision fields, or a revision omitted them.
    InvalidRevisionLinkage(&'static str),
    /// Revision attempted to alter the frozen target, outcome definition, or target horizon.
    RevisionMutatedFrozenContract {
        forecast_id: OpaqueId,
        field: &'static str,
    },
    /// Revision timestamps regressed relative to its predecessor vintage.
    NonMonotonicRevisionTime {
        forecast_id: OpaqueId,
        field: &'static str,
    },
    /// Revision `new_evidence_versions` was empty or cited a version absent from admissible `sources`.
    MissingRevisionEvidence {
        forecast_id: OpaqueId,
        evidence_version: Option<VersionId>,
    },
    /// Forecast timestamps violated `information_cutoff_utc_ms <= origin_utc_ms <= issued_at_utc_ms`
    /// or `horizon_start_utc_ms < horizon_end_utc_ms` or `origin_utc_ms < horizon_end_utc_ms`.
    InvalidTimestampOrder(&'static str),
    /// Forecast cited zero sources or duplicate source versions.
    InvalidSourceList(&'static str),
    /// A source input had unknown public availability or became available after `information_cutoff_utc_ms`.
    SourcePublicAvailabilityAfterCutoff {
        source_version: VersionId,
        public_available_at_utc_ms: Option<i64>,
        cutoff_utc_ms: i64,
    },
    /// Under `ProspectiveStrict`, a source had unknown OIA ingestion time or was ingested after cutoff.
    SourceIngestedAfterStrictCutoff {
        source_version: VersionId,
        ingested_at_utc_ms: Option<i64>,
        cutoff_utc_ms: i64,
    },
    /// A source's historical event timestamp occurred after its claimed public availability or cutoff.
    SourceEventAfterAvailabilityOrCutoff {
        source_version: VersionId,
        event_utc_ms: i64,
        bound_utc_ms: i64,
    },
    /// Under `ProspectiveStrict`, a model's knowledge cutoff exceeded `information_cutoff_utc_ms`.
    ModelKnowledgeAfterStrictCutoff {
        model_version: VersionId,
        model_knowledge_cutoff_utc_ms: i64,
        cutoff_utc_ms: i64,
    },
    /// Outcome resolution violated evidence or supersession requirements.
    InvalidOutcomeResolution(&'static str),
    /// Superseded resolution version does not exist or belongs to a different `outcome_id`.
    UnknownSupersededResolution {
        outcome_id: OpaqueId,
        supersedes_version: VersionId,
    },
    /// Referenced forecast vintage or outcome resolution was not found during evaluation.
    UnknownEvaluationReference {
        kind: &'static str,
        version_id: VersionId,
    },
    /// Supplied resolution version does not match the forecast vintage's `outcome_id`.
    OutcomeDefinitionMismatch {
        forecast_version: VersionId,
        expected_outcome_id: OpaqueId,
        supplied_outcome_id: OpaqueId,
    },
    /// Outcome resolution was recorded before the forecast vintage was issued.
    ResolutionPredatesForecastIssuance {
        forecast_version: VersionId,
        issued_at_utc_ms: i64,
        resolution_version: VersionId,
        recorded_at_utc_ms: i64,
    },
    /// Underlying binary Brier scoring error.
    Scoring(ScoringError),
}

impl fmt::Display for IssuanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BudgetExceeded {
                field,
                actual,
                limit,
            } => write!(f, "{field} count {actual} exceeds limit {limit}"),
            Self::InvalidTextField { field, reason } => {
                write!(f, "invalid text field {field}: {reason}")
            }
            Self::DuplicateVersionId(v) => write!(f, "duplicate version identifier {v}"),
            Self::ForecastSeriesAlreadyExists(id) => {
                write!(
                    f,
                    "forecast series {id} already exists; issue a linked revision instead"
                )
            }
            Self::UnknownForecastPredecessor {
                forecast_id,
                predecessor_version,
            } => write!(
                f,
                "unknown predecessor vintage {predecessor_version} for forecast {forecast_id}"
            ),
            Self::NonLinearRevisionPredecessor {
                forecast_id,
                supplied_predecessor,
                expected_latest,
            } => write!(
                f,
                "revision for forecast {forecast_id} must reference latest vintage {expected_latest}, got {supplied_predecessor}"
            ),
            Self::InvalidRevisionLinkage(msg) => write!(f, "invalid revision linkage: {msg}"),
            Self::RevisionMutatedFrozenContract { forecast_id, field } => write!(
                f,
                "revision for forecast {forecast_id} cannot modify frozen field {field}"
            ),
            Self::NonMonotonicRevisionTime { forecast_id, field } => write!(
                f,
                "revision for forecast {forecast_id} has non-monotonic timestamp in {field}"
            ),
            Self::MissingRevisionEvidence {
                forecast_id,
                evidence_version,
            } => match evidence_version {
                Some(v) => write!(
                    f,
                    "revision for forecast {forecast_id} cites new evidence {v} not present in admissible sources"
                ),
                None => write!(
                    f,
                    "revision for forecast {forecast_id} must cite at least one new evidence version"
                ),
            },
            Self::InvalidTimestampOrder(msg) => write!(f, "invalid timestamp order: {msg}"),
            Self::InvalidSourceList(msg) => write!(f, "invalid forecast source list: {msg}"),
            Self::SourcePublicAvailabilityAfterCutoff {
                source_version,
                public_available_at_utc_ms,
                cutoff_utc_ms,
            } => write!(
                f,
                "source {source_version} public availability {public_available_at_utc_ms:?} is unknown or after cutoff {cutoff_utc_ms}"
            ),
            Self::SourceIngestedAfterStrictCutoff {
                source_version,
                ingested_at_utc_ms,
                cutoff_utc_ms,
            } => write!(
                f,
                "source {source_version} ingestion time {ingested_at_utc_ms:?} is unknown or after strict cutoff {cutoff_utc_ms}"
            ),
            Self::SourceEventAfterAvailabilityOrCutoff {
                source_version,
                event_utc_ms,
                bound_utc_ms,
            } => write!(
                f,
                "source {source_version} event time {event_utc_ms} exceeds availability or cutoff bound {bound_utc_ms}"
            ),
            Self::ModelKnowledgeAfterStrictCutoff {
                model_version,
                model_knowledge_cutoff_utc_ms,
                cutoff_utc_ms,
            } => write!(
                f,
                "model {model_version} knowledge cutoff {model_knowledge_cutoff_utc_ms} exceeds strict cutoff {cutoff_utc_ms}"
            ),
            Self::InvalidOutcomeResolution(msg) => write!(f, "invalid outcome resolution: {msg}"),
            Self::UnknownSupersededResolution {
                outcome_id,
                supersedes_version,
            } => write!(
                f,
                "unknown superseded resolution {supersedes_version} for outcome {outcome_id}"
            ),
            Self::UnknownEvaluationReference { kind, version_id } => {
                write!(f, "unknown {kind} version {version_id} in evaluation")
            }
            Self::OutcomeDefinitionMismatch {
                forecast_version,
                expected_outcome_id,
                supplied_outcome_id,
            } => write!(
                f,
                "forecast {forecast_version} expects outcome {expected_outcome_id}, got {supplied_outcome_id}"
            ),
            Self::ResolutionPredatesForecastIssuance {
                forecast_version,
                issued_at_utc_ms,
                resolution_version,
                recorded_at_utc_ms,
            } => write!(
                f,
                "resolution {resolution_version} ({recorded_at_utc_ms}) predates issuance of forecast {forecast_version} ({issued_at_utc_ms})"
            ),
            Self::Scoring(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for IssuanceError {}

impl From<ScoringError> for IssuanceError {
    fn from(value: ScoringError) -> Self {
        Self::Scoring(value)
    }
}

/// Bounded in-memory ledger of immutable forecast vintages, independent outcome resolution
/// versions, and version-pinned evaluation runs.
#[derive(Debug, Clone, PartialEq)]
pub struct ForecastLedger {
    max_records: usize,
    used_versions: BTreeSet<VersionId>,
    vintages_by_version: BTreeMap<VersionId, ForecastVintageRecord>,
    series_vintages: BTreeMap<OpaqueId, Vec<VersionId>>,
    resolutions_by_version: BTreeMap<VersionId, OutcomeResolutionRecord>,
    outcome_resolutions: BTreeMap<OpaqueId, Vec<VersionId>>,
    evaluations_by_version: BTreeMap<VersionId, EvaluationRunRecord>,
    evaluation_order: Vec<VersionId>,
}

impl ForecastLedger {
    /// Create a bounded ledger accepting up to `max_records` entries per record family.
    pub fn new(max_records: usize) -> Result<Self, IssuanceError> {
        if !(1..=MAX_LEDGER_RECORDS).contains(&max_records) {
            return Err(IssuanceError::BudgetExceeded {
                field: "max_records",
                actual: max_records,
                limit: MAX_LEDGER_RECORDS,
            });
        }
        Ok(Self {
            max_records,
            used_versions: BTreeSet::new(),
            vintages_by_version: BTreeMap::new(),
            series_vintages: BTreeMap::new(),
            resolutions_by_version: BTreeMap::new(),
            outcome_resolutions: BTreeMap::new(),
            evaluations_by_version: BTreeMap::new(),
            evaluation_order: Vec::new(),
        })
    }

    /// Issue an immutable initial forecast vintage or a linked revision (`FORECAST-04`, `MARKET-04`, `MARKET-05`).
    pub fn issue_forecast(
        &mut self,
        draft: ForecastDraft,
    ) -> Result<&ForecastVintageRecord, IssuanceError> {
        if self.vintages_by_version.len() >= self.max_records {
            return Err(IssuanceError::BudgetExceeded {
                field: "forecast_vintages",
                actual: self.vintages_by_version.len() + 1,
                limit: self.max_records,
            });
        }
        if self.used_versions.contains(&draft.version_id) {
            return Err(IssuanceError::DuplicateVersionId(draft.version_id));
        }

        validate_text_field(
            "outcome_definition.proposition",
            &draft.outcome_definition.proposition,
        )?;
        validate_text_field(
            "outcome_definition.resolution_rules",
            &draft.outcome_definition.resolution_rules,
        )?;

        if draft.information_cutoff_utc_ms > draft.origin_utc_ms {
            return Err(IssuanceError::InvalidTimestampOrder(
                "information_cutoff_utc_ms must be <= origin_utc_ms",
            ));
        }
        if draft.origin_utc_ms > draft.issued_at_utc_ms {
            return Err(IssuanceError::InvalidTimestampOrder(
                "origin_utc_ms must be <= issued_at_utc_ms",
            ));
        }
        if draft.horizon_start_utc_ms >= draft.horizon_end_utc_ms {
            return Err(IssuanceError::InvalidTimestampOrder(
                "horizon_start_utc_ms must be < horizon_end_utc_ms",
            ));
        }
        if draft.origin_utc_ms >= draft.horizon_end_utc_ms {
            return Err(IssuanceError::InvalidTimestampOrder(
                "origin_utc_ms must precede horizon_end_utc_ms",
            ));
        }

        let post_cutoff_model_knowledge_disclosed = validate_cutoff_and_sources(
            &draft.cutoff_mode,
            draft.information_cutoff_utc_ms,
            &draft.model_version,
            draft.model_knowledge_cutoff_utc_ms,
            &draft.sources,
        )?;

        validate_collection_len("comparison_set", draft.comparison_set.len())?;
        let mut seen_comparisons = BTreeSet::new();
        for comp in &draft.comparison_set {
            if !seen_comparisons.insert(comp) {
                return Err(IssuanceError::InvalidSourceList(
                    "duplicate comparison_set version identifier",
                ));
            }
        }

        validate_collection_len("assumptions", draft.assumptions.len())?;
        for assumption in &draft.assumptions {
            validate_text_field("assumptions[]", assumption)?;
        }

        let source_version_set: BTreeSet<&VersionId> =
            draft.sources.iter().map(|s| &s.source_version).collect();

        let vintage_number = match (&draft.predecessor_version, &draft.revision_rationale) {
            (None, None) => {
                if self.series_vintages.contains_key(&draft.forecast_id) {
                    return Err(IssuanceError::ForecastSeriesAlreadyExists(
                        draft.forecast_id,
                    ));
                }
                1
            }
            (Some(_), None) | (None, Some(_)) => {
                return Err(IssuanceError::InvalidRevisionLinkage(
                    "predecessor_version and revision_rationale must both be present or both be None",
                ));
            }
            (Some(pred_ver), Some(rationale)) => {
                let series = self
                    .series_vintages
                    .get(&draft.forecast_id)
                    .ok_or_else(|| IssuanceError::UnknownForecastPredecessor {
                        forecast_id: draft.forecast_id.clone(),
                        predecessor_version: pred_ver.clone(),
                    })?;
                let latest_ver =
                    series
                        .last()
                        .ok_or_else(|| IssuanceError::UnknownForecastPredecessor {
                            forecast_id: draft.forecast_id.clone(),
                            predecessor_version: pred_ver.clone(),
                        })?;
                if pred_ver != latest_ver {
                    return Err(IssuanceError::NonLinearRevisionPredecessor {
                        forecast_id: draft.forecast_id.clone(),
                        supplied_predecessor: pred_ver.clone(),
                        expected_latest: latest_ver.clone(),
                    });
                }
                let pred = self.vintages_by_version.get(pred_ver).ok_or_else(|| {
                    IssuanceError::UnknownForecastPredecessor {
                        forecast_id: draft.forecast_id.clone(),
                        predecessor_version: pred_ver.clone(),
                    }
                })?;

                // Revisions must preserve target, outcome definition, and target horizon (FORECAST-04, A15).
                if draft.target_id != pred.target_id {
                    return Err(IssuanceError::RevisionMutatedFrozenContract {
                        forecast_id: draft.forecast_id,
                        field: "target_id",
                    });
                }
                if draft.outcome_definition != pred.outcome_definition {
                    return Err(IssuanceError::RevisionMutatedFrozenContract {
                        forecast_id: draft.forecast_id,
                        field: "outcome_definition",
                    });
                }
                if draft.horizon_start_utc_ms != pred.horizon_start_utc_ms
                    || draft.horizon_end_utc_ms != pred.horizon_end_utc_ms
                {
                    return Err(IssuanceError::RevisionMutatedFrozenContract {
                        forecast_id: draft.forecast_id,
                        field: "horizon",
                    });
                }
                if draft.information_cutoff_utc_ms < pred.information_cutoff_utc_ms {
                    return Err(IssuanceError::NonMonotonicRevisionTime {
                        forecast_id: draft.forecast_id,
                        field: "information_cutoff_utc_ms",
                    });
                }
                if draft.origin_utc_ms < pred.origin_utc_ms {
                    return Err(IssuanceError::NonMonotonicRevisionTime {
                        forecast_id: draft.forecast_id,
                        field: "origin_utc_ms",
                    });
                }
                if draft.issued_at_utc_ms < pred.issued_at_utc_ms {
                    return Err(IssuanceError::NonMonotonicRevisionTime {
                        forecast_id: draft.forecast_id,
                        field: "issued_at_utc_ms",
                    });
                }

                validate_text_field("revision_rationale.summary", &rationale.summary)?;
                validate_collection_len(
                    "revision_rationale.new_evidence_versions",
                    rationale.new_evidence_versions.len(),
                )?;
                if rationale.new_evidence_versions.is_empty() {
                    return Err(IssuanceError::MissingRevisionEvidence {
                        forecast_id: draft.forecast_id,
                        evidence_version: None,
                    });
                }
                let mut seen_new_evidence = BTreeSet::new();
                for ev in &rationale.new_evidence_versions {
                    if !seen_new_evidence.insert(ev) {
                        return Err(IssuanceError::InvalidRevisionLinkage(
                            "duplicate version in revision_rationale.new_evidence_versions",
                        ));
                    }
                    if !source_version_set.contains(ev) {
                        return Err(IssuanceError::MissingRevisionEvidence {
                            forecast_id: draft.forecast_id,
                            evidence_version: Some(ev.clone()),
                        });
                    }
                }
                series.len() + 1
            }
        };

        let version_key = draft.version_id.clone();
        let record = ForecastVintageRecord {
            forecast_id: draft.forecast_id.clone(),
            version_id: draft.version_id,
            vintage_number,
            schema_version: draft.schema_version,
            actor: draft.actor,
            issued_at_utc_ms: draft.issued_at_utc_ms,
            origin_utc_ms: draft.origin_utc_ms,
            information_cutoff_utc_ms: draft.information_cutoff_utc_ms,
            horizon_start_utc_ms: draft.horizon_start_utc_ms,
            horizon_end_utc_ms: draft.horizon_end_utc_ms,
            cutoff_mode: draft.cutoff_mode,
            target_id: draft.target_id,
            outcome_definition: draft.outcome_definition,
            baseline_version: draft.baseline_version,
            baseline_probability: draft.baseline_probability,
            forecast_probability: draft.forecast_probability,
            model_version: draft.model_version,
            model_knowledge_cutoff_utc_ms: draft.model_knowledge_cutoff_utc_ms,
            post_cutoff_model_knowledge_disclosed,
            sources: draft.sources,
            comparison_set: draft.comparison_set,
            assumptions: draft.assumptions,
            predecessor_version: draft.predecessor_version,
            revision_rationale: draft.revision_rationale,
        };

        self.used_versions.insert(version_key.clone());
        self.series_vintages
            .entry(draft.forecast_id)
            .or_default()
            .push(version_key.clone());
        self.vintages_by_version.insert(version_key.clone(), record);
        self.vintages_by_version.get(&version_key).ok_or(
            IssuanceError::UnknownEvaluationReference {
                kind: "forecast",
                version_id: version_key,
            },
        )
    }

    /// Record an independent outcome resolution version (`docs/architecture.md` §6.4).
    ///
    /// If `supersedes_version` is set, the prior resolution remains intact in the ledger
    /// and any [`EvaluationRunRecord`] that pinned `supersedes_version` is flagged with
    /// `needs_reassessment = true`.
    pub fn record_outcome_resolution(
        &mut self,
        resolution: OutcomeResolutionRecord,
    ) -> Result<&OutcomeResolutionRecord, IssuanceError> {
        if self.resolutions_by_version.len() >= self.max_records {
            return Err(IssuanceError::BudgetExceeded {
                field: "outcome_resolutions",
                actual: self.resolutions_by_version.len() + 1,
                limit: self.max_records,
            });
        }
        if self.used_versions.contains(&resolution.version_id) {
            return Err(IssuanceError::DuplicateVersionId(resolution.version_id));
        }

        validate_collection_len("evidence_versions", resolution.evidence_versions.len())?;
        let mut seen_evidence = BTreeSet::new();
        for ev in &resolution.evidence_versions {
            if !seen_evidence.insert(ev) {
                return Err(IssuanceError::InvalidOutcomeResolution(
                    "duplicate evidence version in outcome resolution",
                ));
            }
        }

        match &resolution.state {
            ResolutionState::Resolved {
                occurred: false,
                observed_event_utc_ms: Some(_),
            } => {
                return Err(IssuanceError::InvalidOutcomeResolution(
                    "non-occurrence resolution cannot carry an observed_event_utc_ms instant",
                ));
            }
            ResolutionState::Resolved {
                observed_event_utc_ms: Some(obs_ms),
                ..
            } => {
                if *obs_ms > resolution.recorded_at_utc_ms {
                    return Err(IssuanceError::InvalidOutcomeResolution(
                        "observed_event_utc_ms cannot be after recorded_at_utc_ms",
                    ));
                }
                if resolution.evidence_versions.is_empty() {
                    return Err(IssuanceError::InvalidOutcomeResolution(
                        "resolved outcome must cite at least one evidence version",
                    ));
                }
            }
            ResolutionState::Resolved {
                observed_event_utc_ms: None,
                ..
            }
            | ResolutionState::Unresolved(UnresolvedReason::Disputed) => {
                if resolution.evidence_versions.is_empty() {
                    return Err(IssuanceError::InvalidOutcomeResolution(
                        "resolved or disputed outcome must cite at least one evidence version",
                    ));
                }
            }
            ResolutionState::Unresolved(UnresolvedReason::Pending | UnresolvedReason::Censored) => {
            }
            ResolutionState::Invalidated { reason } => {
                validate_text_field("resolution.state.invalidated.reason", reason)?;
                if resolution.evidence_versions.is_empty() {
                    return Err(IssuanceError::InvalidOutcomeResolution(
                        "invalidated outcome must cite at least one evidence version",
                    ));
                }
            }
        }

        match (
            &resolution.supersedes_version,
            &resolution.correction_rationale,
        ) {
            (None, None) => {
                if self
                    .outcome_resolutions
                    .contains_key(&resolution.outcome_id)
                {
                    return Err(IssuanceError::InvalidOutcomeResolution(
                        "outcome already has a resolution; subsequent updates must set supersedes_version and correction_rationale",
                    ));
                }
            }
            (Some(_), None) | (None, Some(_)) => {
                return Err(IssuanceError::InvalidOutcomeResolution(
                    "supersedes_version and correction_rationale must both be present or both be None",
                ));
            }
            (Some(prior_ver), Some(rationale)) => {
                validate_text_field("correction_rationale", rationale)?;
                let history = self
                    .outcome_resolutions
                    .get(&resolution.outcome_id)
                    .ok_or_else(|| IssuanceError::UnknownSupersededResolution {
                        outcome_id: resolution.outcome_id.clone(),
                        supersedes_version: prior_ver.clone(),
                    })?;
                if history.last() != Some(prior_ver) {
                    return Err(IssuanceError::UnknownSupersededResolution {
                        outcome_id: resolution.outcome_id.clone(),
                        supersedes_version: prior_ver.clone(),
                    });
                }
                let prior = self.resolutions_by_version.get(prior_ver).ok_or_else(|| {
                    IssuanceError::UnknownSupersededResolution {
                        outcome_id: resolution.outcome_id.clone(),
                        supersedes_version: prior_ver.clone(),
                    }
                })?;
                if resolution.recorded_at_utc_ms < prior.recorded_at_utc_ms {
                    return Err(IssuanceError::InvalidOutcomeResolution(
                        "superseding resolution recorded_at_utc_ms cannot precede superseded resolution",
                    ));
                }

                // Flag dependent evaluation runs for reassessment without editing their scores.
                let reason = ReassessmentReason::OutcomeResolutionSuperseded {
                    outcome_id: resolution.outcome_id.clone(),
                    previous_resolution_version: prior_ver.clone(),
                    new_resolution_version: resolution.version_id.clone(),
                };
                for eval in self.evaluations_by_version.values_mut() {
                    let uses_prior = eval
                        .vintages
                        .iter()
                        .any(|v| &v.resolution_version == prior_ver);
                    if uses_prior {
                        eval.needs_reassessment = true;
                        eval.reassessment_reasons.push(reason.clone());
                    }
                }
            }
        }

        let ver_key = resolution.version_id.clone();
        self.used_versions.insert(ver_key.clone());
        self.outcome_resolutions
            .entry(resolution.outcome_id.clone())
            .or_default()
            .push(ver_key.clone());
        self.resolutions_by_version
            .insert(ver_key.clone(), resolution);
        self.resolutions_by_version
            .get(&ver_key)
            .ok_or(IssuanceError::UnknownEvaluationReference {
                kind: "resolution",
                version_id: ver_key,
            })
    }

    /// Evaluate an explicit cohort of `(forecast_version, resolution_version)` pairs,
    /// preserving every vintage (including superseded originals, unresolved cases, and
    /// invalidated outcomes) and recording an immutable [`EvaluationRunRecord`].
    pub fn evaluate_cohort(
        &mut self,
        evaluation_version: VersionId,
        evaluated_at_utc_ms: i64,
        cohort: &[(VersionId, VersionId)],
        row_limit: usize,
    ) -> Result<&EvaluationRunRecord, IssuanceError> {
        if self.evaluations_by_version.len() >= self.max_records {
            return Err(IssuanceError::BudgetExceeded {
                field: "evaluation_runs",
                actual: self.evaluations_by_version.len() + 1,
                limit: self.max_records,
            });
        }
        if self.used_versions.contains(&evaluation_version) {
            return Err(IssuanceError::DuplicateVersionId(evaluation_version));
        }
        if !(1..=MAX_EVALUATION_ROWS).contains(&row_limit) {
            return Err(IssuanceError::Scoring(ScoringError::InvalidRowLimit));
        }
        if cohort.len() > row_limit {
            return Err(IssuanceError::Scoring(ScoringError::RowLimitExceeded {
                supplied: cohort.len(),
                limit: row_limit,
            }));
        }

        let mut seen_forecast_versions = BTreeSet::new();
        let mut binary_rows = Vec::with_capacity(cohort.len());
        let mut row_indices = Vec::with_capacity(cohort.len());

        for (forecast_ver, resolution_ver) in cohort {
            if !seen_forecast_versions.insert(forecast_ver) {
                return Err(IssuanceError::Scoring(
                    ScoringError::DuplicateForecastVersion(forecast_ver.clone()),
                ));
            }
            let vintage = self.vintages_by_version.get(forecast_ver).ok_or_else(|| {
                IssuanceError::UnknownEvaluationReference {
                    kind: "forecast",
                    version_id: forecast_ver.clone(),
                }
            })?;
            let resolution = self
                .resolutions_by_version
                .get(resolution_ver)
                .ok_or_else(|| IssuanceError::UnknownEvaluationReference {
                    kind: "resolution",
                    version_id: resolution_ver.clone(),
                })?;

            if vintage.outcome_definition.outcome_id != resolution.outcome_id {
                return Err(IssuanceError::OutcomeDefinitionMismatch {
                    forecast_version: forecast_ver.clone(),
                    expected_outcome_id: vintage.outcome_definition.outcome_id.clone(),
                    supplied_outcome_id: resolution.outcome_id.clone(),
                });
            }
            if resolution.recorded_at_utc_ms < vintage.issued_at_utc_ms {
                return Err(IssuanceError::ResolutionPredatesForecastIssuance {
                    forecast_version: forecast_ver.clone(),
                    issued_at_utc_ms: vintage.issued_at_utc_ms,
                    resolution_version: resolution_ver.clone(),
                    recorded_at_utc_ms: resolution.recorded_at_utc_ms,
                });
            }

            match &resolution.state {
                ResolutionState::Resolved { occurred, .. } => {
                    let idx = binary_rows.len();
                    binary_rows.push(BinaryForecastRow {
                        forecast_version: vintage.version_id.clone(),
                        probability: vintage.forecast_probability,
                        baseline_version: vintage.baseline_version.clone(),
                        baseline_probability: vintage.baseline_probability,
                        outcome: BinaryOutcome::Resolved {
                            occurred: *occurred,
                            version: resolution.version_id.clone(),
                        },
                    });
                    row_indices.push(Some(idx));
                }
                ResolutionState::Unresolved(reason) => {
                    let idx = binary_rows.len();
                    binary_rows.push(BinaryForecastRow {
                        forecast_version: vintage.version_id.clone(),
                        probability: vintage.forecast_probability,
                        baseline_version: vintage.baseline_version.clone(),
                        baseline_probability: vintage.baseline_probability,
                        outcome: BinaryOutcome::Unresolved(*reason),
                    });
                    row_indices.push(Some(idx));
                }
                ResolutionState::Invalidated { .. } => {
                    row_indices.push(None);
                }
            }
        }

        let binary_eval = evaluate_binary(&binary_rows, row_limit)?;
        let mut vintages = Vec::with_capacity(cohort.len());
        let mut invalidated_count = 0usize;
        let mut reassessment_reasons = Vec::new();

        for ((forecast_ver, resolution_ver), maybe_row_idx) in cohort.iter().zip(row_indices.iter())
        {
            let vintage = self.vintages_by_version.get(forecast_ver).ok_or_else(|| {
                IssuanceError::UnknownEvaluationReference {
                    kind: "forecast",
                    version_id: forecast_ver.clone(),
                }
            })?;
            let resolution = self
                .resolutions_by_version
                .get(resolution_ver)
                .ok_or_else(|| IssuanceError::UnknownEvaluationReference {
                    kind: "resolution",
                    version_id: resolution_ver.clone(),
                })?;

            // If this evaluation pins an already-superseded resolution version, disclose that immediately.
            if let Some(history) = self.outcome_resolutions.get(&resolution.outcome_id)
                && let Some(latest_res_ver) = history.last()
                && latest_res_ver != resolution_ver
            {
                let reason = ReassessmentReason::OutcomeResolutionSuperseded {
                    outcome_id: resolution.outcome_id.clone(),
                    previous_resolution_version: resolution_ver.clone(),
                    new_resolution_version: latest_res_ver.clone(),
                };
                if !reassessment_reasons.contains(&reason) {
                    reassessment_reasons.push(reason);
                }
            }

            let brier_scores = match maybe_row_idx {
                Some(idx) => binary_eval.rows.get(*idx).and_then(|r| r.scores),
                None => {
                    invalidated_count += 1;
                    None
                }
            };

            let horizon_timing = match &resolution.state {
                ResolutionState::Resolved {
                    occurred: true,
                    observed_event_utc_ms: Some(obs_ms),
                } => Some(compute_horizon_timing(
                    *obs_ms,
                    vintage.horizon_start_utc_ms,
                    vintage.horizon_end_utc_ms,
                )),
                _ => None,
            };

            vintages.push(EvaluatedVintageSummary {
                forecast_id: vintage.forecast_id.clone(),
                forecast_version: vintage.version_id.clone(),
                vintage_number: vintage.vintage_number,
                predecessor_version: vintage.predecessor_version.clone(),
                baseline_version: vintage.baseline_version.clone(),
                resolution_version: resolution.version_id.clone(),
                forecast_probability: vintage.forecast_probability,
                baseline_probability: vintage.baseline_probability,
                resolution_state: resolution.state.clone(),
                brier_scores,
                horizon_timing,
                post_cutoff_model_knowledge_disclosed: vintage
                    .post_cutoff_model_knowledge_disclosed,
            });
        }

        let record = EvaluationRunRecord {
            evaluation_version: evaluation_version.clone(),
            evaluated_at_utc_ms,
            vintages,
            resolved_count: binary_eval.resolved_count,
            unresolved_count: binary_eval.unresolved_count,
            invalidated_count,
            mean_scores: binary_eval.mean_scores,
            needs_reassessment: !reassessment_reasons.is_empty(),
            reassessment_reasons,
        };

        self.used_versions.insert(evaluation_version.clone());
        self.evaluation_order.push(evaluation_version.clone());
        self.evaluations_by_version
            .insert(evaluation_version.clone(), record);
        self.evaluations_by_version.get(&evaluation_version).ok_or(
            IssuanceError::UnknownEvaluationReference {
                kind: "evaluation",
                version_id: evaluation_version,
            },
        )
    }

    /// Look up an issued forecast vintage by its immutable [`VersionId`].
    #[must_use]
    pub fn vintage(&self, version_id: &VersionId) -> Option<&ForecastVintageRecord> {
        self.vintages_by_version.get(version_id)
    }

    /// Return all immutable vintages for `forecast_id` in chronological issuance order.
    #[must_use]
    pub fn series_vintages(&self, forecast_id: &OpaqueId) -> Vec<&ForecastVintageRecord> {
        self.series_vintages
            .get(forecast_id)
            .map(|versions| {
                versions
                    .iter()
                    .filter_map(|v| self.vintages_by_version.get(v))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Look up an outcome resolution record by its immutable [`VersionId`].
    #[must_use]
    pub fn resolution(&self, version_id: &VersionId) -> Option<&OutcomeResolutionRecord> {
        self.resolutions_by_version.get(version_id)
    }

    /// Return all resolution versions for `outcome_id` in recording order.
    #[must_use]
    pub fn outcome_resolutions(&self, outcome_id: &OpaqueId) -> Vec<&OutcomeResolutionRecord> {
        self.outcome_resolutions
            .get(outcome_id)
            .map(|versions| {
                versions
                    .iter()
                    .filter_map(|v| self.resolutions_by_version.get(v))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Look up an evaluation run by its immutable [`VersionId`].
    #[must_use]
    pub fn evaluation(&self, version_id: &VersionId) -> Option<&EvaluationRunRecord> {
        self.evaluations_by_version.get(version_id)
    }

    /// Return all recorded evaluation runs in execution order.
    #[must_use]
    pub fn evaluations(&self) -> Vec<&EvaluationRunRecord> {
        self.evaluation_order
            .iter()
            .filter_map(|v| self.evaluations_by_version.get(v))
            .collect()
    }
}

fn validate_text_field(field: &'static str, value: &str) -> Result<(), IssuanceError> {
    if value.trim().is_empty() {
        return Err(IssuanceError::InvalidTextField {
            field,
            reason: "must not be empty or whitespace-only",
        });
    }
    if value.len() > MAX_TEXT_FIELD_BYTES {
        return Err(IssuanceError::InvalidTextField {
            field,
            reason: "exceeds MAX_TEXT_FIELD_BYTES",
        });
    }
    Ok(())
}

fn validate_collection_len(field: &'static str, actual: usize) -> Result<(), IssuanceError> {
    if actual > MAX_FORECAST_COLLECTION_LEN {
        return Err(IssuanceError::BudgetExceeded {
            field,
            actual,
            limit: MAX_FORECAST_COLLECTION_LEN,
        });
    }
    Ok(())
}

fn validate_cutoff_and_sources(
    cutoff_mode: &CutoffAdmissibilityMode,
    cutoff_utc_ms: i64,
    model_version: &VersionId,
    model_knowledge_cutoff_utc_ms: Option<i64>,
    sources: &[CutoffSourceInput],
) -> Result<bool, IssuanceError> {
    if sources.is_empty() {
        return Err(IssuanceError::InvalidSourceList(
            "forecast must cite at least one admissible source version",
        ));
    }
    validate_collection_len("sources", sources.len())?;

    if let CutoffAdmissibilityMode::RetrospectiveReconstructed { limitation_note } = cutoff_mode {
        validate_text_field("cutoff_mode.limitation_note", limitation_note)?;
    }

    let post_cutoff_model_knowledge_disclosed = match (cutoff_mode, model_knowledge_cutoff_utc_ms) {
        (CutoffAdmissibilityMode::ProspectiveStrict, Some(mc)) if mc > cutoff_utc_ms => {
            return Err(IssuanceError::ModelKnowledgeAfterStrictCutoff {
                model_version: model_version.clone(),
                model_knowledge_cutoff_utc_ms: mc,
                cutoff_utc_ms,
            });
        }
        (CutoffAdmissibilityMode::RetrospectiveReconstructed { .. }, Some(mc))
            if mc > cutoff_utc_ms =>
        {
            true
        }
        _ => false,
    };

    let mut seen_sources = BTreeSet::new();
    for src in sources {
        if !seen_sources.insert(&src.source_version) {
            return Err(IssuanceError::InvalidSourceList(
                "duplicate source_version in forecast sources",
            ));
        }
        let Some(pub_ms) = src.public_available_at_utc_ms else {
            return Err(IssuanceError::SourcePublicAvailabilityAfterCutoff {
                source_version: src.source_version.clone(),
                public_available_at_utc_ms: None,
                cutoff_utc_ms,
            });
        };
        if pub_ms > cutoff_utc_ms {
            return Err(IssuanceError::SourcePublicAvailabilityAfterCutoff {
                source_version: src.source_version.clone(),
                public_available_at_utc_ms: Some(pub_ms),
                cutoff_utc_ms,
            });
        }
        if let Some(event_ms) = src.event_utc_ms {
            let bound = pub_ms.min(cutoff_utc_ms);
            if event_ms > bound {
                return Err(IssuanceError::SourceEventAfterAvailabilityOrCutoff {
                    source_version: src.source_version.clone(),
                    event_utc_ms: event_ms,
                    bound_utc_ms: bound,
                });
            }
        }
        if matches!(cutoff_mode, CutoffAdmissibilityMode::ProspectiveStrict) {
            match src.ingested_at_utc_ms {
                Some(ing_ms) if ing_ms <= cutoff_utc_ms => {}
                other => {
                    return Err(IssuanceError::SourceIngestedAfterStrictCutoff {
                        source_version: src.source_version.clone(),
                        ingested_at_utc_ms: other,
                        cutoff_utc_ms,
                    });
                }
            }
        }
    }

    Ok(post_cutoff_model_knowledge_disclosed)
}

fn compute_horizon_timing(
    observed_event_utc_ms: i64,
    horizon_start_utc_ms: i64,
    horizon_end_utc_ms: i64,
) -> HorizonTiming {
    if observed_event_utc_ms < horizon_start_utc_ms {
        HorizonTiming::Early {
            early_by_ms: horizon_start_utc_ms.saturating_sub(observed_event_utc_ms),
        }
    } else if observed_event_utc_ms >= horizon_end_utc_ms {
        HorizonTiming::Late {
            late_by_ms: observed_event_utc_ms.saturating_sub(horizon_end_utc_ms),
        }
    } else {
        HorizonTiming::WithinHorizon
    }
}
