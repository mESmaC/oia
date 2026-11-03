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

/// Maximum number of forecast vintages, resolutions, evaluation runs, or total evaluated
/// vintage summaries in a single bounded ledger.
pub const MAX_LEDGER_RECORDS: usize = 10_000;

/// Maximum number of source inputs, comparison entries, or assumptions on a single forecast vintage.
pub const MAX_FORECAST_COLLECTION_LEN: usize = 1_024;

/// Maximum byte length of a free-text proposition, rule, assumption, or rationale field.
pub const MAX_TEXT_FIELD_BYTES: usize = 16_384;

/// Minimum admissible timestamp in UTC milliseconds (`0001-01-01T00:00:00.000Z`).
pub const MIN_VALID_UTC_MS: i64 = -62_135_596_800_000;

/// Maximum admissible timestamp in UTC milliseconds (`9999-12-31T23:59:59.999Z`).
pub const MAX_VALID_UTC_MS: i64 = 253_402_300_799_999;

/// How a forecast's information cutoff is enforced (`docs/architecture.md` §3.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CutoffAdmissibilityMode {
    /// Only source versions both publicly available AND ingested into OIA at or before
    /// `information_cutoff_utc_ms` qualify, and the model's `model_knowledge_cutoff_utc_ms`
    /// must be known (`Some`) and `<= information_cutoff_utc_ms`. Unknown timestamps or
    /// post-cutoff model knowledge are rejected.
    ProspectiveStrict,
    /// Historical reconstruction may admit externally documented public availability at or
    /// before `information_cutoff_utc_ms` even when OIA ingested the record later, provided
    /// an explicit non-empty limitation note is recorded. Unknown public availability is
    /// still rejected, and unknown or post-cutoff model knowledge is visibly disclosed.
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
    /// Every version listed here must appear in the revision's admissible `sources` and
    /// must not already have been cited in the predecessor vintage's `sources`.
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
    /// Timestamp when this vintage was recorded in UTC milliseconds (`issued_at_utc_ms < horizon_end_utc_ms`).
    pub issued_at_utc_ms: i64,
    /// Forecast origin timestamp in UTC milliseconds (`information_cutoff_utc_ms <= origin_utc_ms <= issued_at_utc_ms`).
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
    /// Model training/knowledge cutoff timestamp in UTC milliseconds (`docs/architecture.md` §6.4).
    /// Required (`Some(t)` with `t <= information_cutoff_utc_ms`) under `ProspectiveStrict`.
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
    /// True when retrospective mode admitted a model whose knowledge cutoff is unknown (`None`)
    /// or exceeds `information_cutoff_utc_ms` (`docs/architecture.md` §6.4).
    pub post_cutoff_model_knowledge_disclosed: bool,
    pub sources: Vec<CutoffSourceInput>,
    pub comparison_set: Vec<VersionId>,
    pub assumptions: Vec<String>,
    pub predecessor_version: Option<VersionId>,
    pub revision_rationale: Option<RevisionRationale>,
    /// Immutable conditional scenario and monitoring specification frozen at issuance when
    /// issued via [`crate::analogues::AnalogueComparisonSet::issue_forecast`] (`FORECAST-03`, `FORECAST-04`, `A15`, `P1`).
    pub analogue_scenario: Option<crate::analogues::IssuedAnalogueScenarioSnapshot>,
}

/// Outcome resolution state keeping resolved, unresolved/censored, and invalidated outcomes
/// distinct (`docs/architecture.md` §6.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionState {
    /// Defined event resolved as occurred (`true`) inside `[horizon_start_utc_ms, horizon_end_utc_ms)`
    /// or did not occur (`false`) within the target horizon.
    ///
    /// - When `occurred == true`, `observed_event_utc_ms` must be `Some(t)` with
    ///   `horizon_start_utc_ms <= t < horizon_end_utc_ms`.
    /// - When `occurred == false`, `recorded_at_utc_ms` must be `>= horizon_end_utc_ms`, and
    ///   `observed_event_utc_ms` may be `None` (never occurred) or `Some(t)` outside
    ///   `[horizon_start_utc_ms, horizon_end_utc_ms)` (an early or late event outside the window).
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
    /// Event occurred before `horizon_start_utc_ms` by `early_by_ms` milliseconds (and is therefore
    /// scored as `occurred: false` for the target horizon).
    Early { early_by_ms: i64 },
    /// Event occurred at or after `horizon_end_utc_ms` by `late_by_ms` milliseconds (and is therefore
    /// scored as `occurred: false` for the target horizon).
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
    /// True if the forecast vintage disclosed unknown or post-cutoff model knowledge in retrospective mode.
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
    /// A historical precedent episode version frozen in an evaluated vintage's `comparison_set`
    /// was superseded by a corrected precedent version (`FORECAST-02`, `FORECAST-04`, `AGENTS.md` §4).
    PrecedentEpisodeSuperseded {
        episode_id: OpaqueId,
        previous_episode_version: VersionId,
        new_episode_version: VersionId,
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
    /// A UTC millisecond timestamp fell outside [`MIN_VALID_UTC_MS`]..=[`MAX_VALID_UTC_MS`].
    TimestampOutOfRange { field: &'static str, value: i64 },
    /// A version identifier was already used by another record in the ledger.
    DuplicateVersionId(VersionId),
    /// Initial forecast series already exists; subsequent vintages must use revision semantics.
    ForecastSeriesAlreadyExists(OpaqueId),
    /// Another forecast series already registered `outcome_id` with a different definition,
    /// target, or target horizon (`FORECAST-04`, `B3`).
    ConflictingOutcomeContract {
        outcome_id: OpaqueId,
        field: &'static str,
    },
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
    /// Revision attempted to alter the frozen target, outcome definition, target horizon,
    /// baseline, or strict cutoff mode.
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
    /// Revision cited a source version in `new_evidence_versions` that was already present in its predecessor.
    RevisionEvidenceNotNew {
        forecast_id: OpaqueId,
        evidence_version: VersionId,
    },
    /// Forecast timestamps violated `information_cutoff_utc_ms <= origin_utc_ms <= issued_at_utc_ms < horizon_end_utc_ms`
    /// or `horizon_start_utc_ms < horizon_end_utc_ms`.
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
    /// Under `ProspectiveStrict`, a model's knowledge cutoff was unknown (`None`) or exceeded `information_cutoff_utc_ms`.
    ModelKnowledgeAfterStrictCutoff {
        model_version: VersionId,
        model_knowledge_cutoff_utc_ms: Option<i64>,
        cutoff_utc_ms: i64,
    },
    /// Outcome resolution referenced an `outcome_id` that has not been registered by any forecast.
    UnknownOutcomeDefinition(OpaqueId),
    /// Outcome resolution violated evidence, `resolution_id`, or supersession requirements.
    InvalidOutcomeResolution(&'static str),
    /// `ResolutionState::Resolved` had an `observed_event_utc_ms` inconsistent with `occurred`
    /// and the outcome's frozen horizon `[horizon_start_utc_ms, horizon_end_utc_ms)` (`B2`).
    ResolutionOutsideTargetHorizon {
        outcome_id: OpaqueId,
        occurred: bool,
        observed_event_utc_ms: i64,
        horizon_start_utc_ms: i64,
        horizon_end_utc_ms: i64,
    },
    /// A non-occurrence (`occurred: false`) was recorded before `horizon_end_utc_ms` closed (`B4`).
    NonOccurrenceResolvedBeforeHorizonEnd {
        outcome_id: OpaqueId,
        recorded_at_utc_ms: i64,
        horizon_end_utc_ms: i64,
    },
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
    /// Observed event occurred at or before the forecast vintage's cutoff, origin, or issue time (`B1`).
    EventObservedAtOrBeforeForecastIssuance {
        forecast_version: VersionId,
        information_cutoff_utc_ms: i64,
        issued_at_utc_ms: i64,
        observed_event_utc_ms: i64,
    },
    /// Evaluation timestamp precedes a forecast issuance or outcome resolution timestamp in the cohort.
    EvaluationPredatesInputs {
        evaluation_version: VersionId,
        evaluated_at_utc_ms: i64,
        input_version: VersionId,
        input_timestamp_utc_ms: i64,
    },
    /// Cohort included a revised vintage without including all earlier vintages of the same forecast series.
    IncompleteForecastSeriesInCohort {
        forecast_id: OpaqueId,
        missing_vintage: VersionId,
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
            Self::TimestampOutOfRange { field, value } => {
                write!(
                    f,
                    "timestamp {field}={value} is outside valid range {MIN_VALID_UTC_MS}..={MAX_VALID_UTC_MS}"
                )
            }
            Self::DuplicateVersionId(v) => write!(f, "duplicate version identifier {v}"),
            Self::ForecastSeriesAlreadyExists(id) => {
                write!(
                    f,
                    "forecast series {id} already exists; issue a linked revision instead"
                )
            }
            Self::ConflictingOutcomeContract { outcome_id, field } => write!(
                f,
                "outcome {outcome_id} is already registered with a different {field}"
            ),
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
            Self::RevisionEvidenceNotNew {
                forecast_id,
                evidence_version,
            } => write!(
                f,
                "revision for forecast {forecast_id} cites evidence {evidence_version} already present in predecessor"
            ),
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
                "model {model_version} knowledge cutoff {model_knowledge_cutoff_utc_ms:?} is unknown or exceeds strict cutoff {cutoff_utc_ms}"
            ),
            Self::UnknownOutcomeDefinition(outcome_id) => {
                write!(f, "unknown outcome definition {outcome_id}")
            }
            Self::InvalidOutcomeResolution(msg) => write!(f, "invalid outcome resolution: {msg}"),
            Self::ResolutionOutsideTargetHorizon {
                outcome_id,
                occurred,
                observed_event_utc_ms,
                horizon_start_utc_ms,
                horizon_end_utc_ms,
            } => write!(
                f,
                "outcome {outcome_id} resolution occurred={occurred} at {observed_event_utc_ms} is inconsistent with target horizon [{horizon_start_utc_ms}, {horizon_end_utc_ms})"
            ),
            Self::NonOccurrenceResolvedBeforeHorizonEnd {
                outcome_id,
                recorded_at_utc_ms,
                horizon_end_utc_ms,
            } => write!(
                f,
                "outcome {outcome_id} non-occurrence recorded at {recorded_at_utc_ms} before horizon end {horizon_end_utc_ms}"
            ),
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
            Self::EventObservedAtOrBeforeForecastIssuance {
                forecast_version,
                information_cutoff_utc_ms,
                issued_at_utc_ms,
                observed_event_utc_ms,
            } => write!(
                f,
                "observed event at {observed_event_utc_ms} occurred at or before forecast {forecast_version} cutoff ({information_cutoff_utc_ms}) or issuance ({issued_at_utc_ms})"
            ),
            Self::EvaluationPredatesInputs {
                evaluation_version,
                evaluated_at_utc_ms,
                input_version,
                input_timestamp_utc_ms,
            } => write!(
                f,
                "evaluation {evaluation_version} timestamp {evaluated_at_utc_ms} precedes input {input_version} timestamp {input_timestamp_utc_ms}"
            ),
            Self::IncompleteForecastSeriesInCohort {
                forecast_id,
                missing_vintage,
            } => write!(
                f,
                "cohort includes a revision of {forecast_id} but omits earlier vintage {missing_vintage}"
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct RegisteredOutcomeContract {
    outcome_definition: OutcomeDefinition,
    target_id: OpaqueId,
    horizon_start_utc_ms: i64,
    horizon_end_utc_ms: i64,
}

/// Bounded in-memory ledger of immutable forecast vintages, independent outcome resolution
/// versions, and version-pinned evaluation runs.
#[derive(Debug, Clone, PartialEq)]
pub struct ForecastLedger {
    max_records: usize,
    used_versions: BTreeSet<VersionId>,
    outcome_contracts: BTreeMap<OpaqueId, RegisteredOutcomeContract>,
    vintages_by_version: BTreeMap<VersionId, ForecastVintageRecord>,
    series_vintages: BTreeMap<OpaqueId, Vec<VersionId>>,
    resolutions_by_version: BTreeMap<VersionId, OutcomeResolutionRecord>,
    outcome_resolutions: BTreeMap<OpaqueId, Vec<VersionId>>,
    superseded_precedents: BTreeMap<VersionId, (OpaqueId, VersionId)>,
    evaluations_by_version: BTreeMap<VersionId, EvaluationRunRecord>,
    evaluation_order: Vec<VersionId>,
    total_evaluated_vintages: usize,
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
            outcome_contracts: BTreeMap::new(),
            vintages_by_version: BTreeMap::new(),
            series_vintages: BTreeMap::new(),
            resolutions_by_version: BTreeMap::new(),
            outcome_resolutions: BTreeMap::new(),
            superseded_precedents: BTreeMap::new(),
            evaluations_by_version: BTreeMap::new(),
            evaluation_order: Vec::new(),
            total_evaluated_vintages: 0,
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

        validate_timestamp_range("information_cutoff_utc_ms", draft.information_cutoff_utc_ms)?;
        validate_timestamp_range("origin_utc_ms", draft.origin_utc_ms)?;
        validate_timestamp_range("issued_at_utc_ms", draft.issued_at_utc_ms)?;
        validate_timestamp_range("horizon_start_utc_ms", draft.horizon_start_utc_ms)?;
        validate_timestamp_range("horizon_end_utc_ms", draft.horizon_end_utc_ms)?;

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
        if draft.issued_at_utc_ms >= draft.horizon_end_utc_ms {
            return Err(IssuanceError::InvalidTimestampOrder(
                "issued_at_utc_ms must precede horizon_end_utc_ms",
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
                if let Some(existing_contract) = self
                    .outcome_contracts
                    .get(&draft.outcome_definition.outcome_id)
                {
                    if existing_contract.outcome_definition != draft.outcome_definition {
                        return Err(IssuanceError::ConflictingOutcomeContract {
                            outcome_id: draft.outcome_definition.outcome_id,
                            field: "outcome_definition",
                        });
                    }
                    if existing_contract.target_id != draft.target_id {
                        return Err(IssuanceError::ConflictingOutcomeContract {
                            outcome_id: draft.outcome_definition.outcome_id,
                            field: "target_id",
                        });
                    }
                    if existing_contract.horizon_start_utc_ms != draft.horizon_start_utc_ms
                        || existing_contract.horizon_end_utc_ms != draft.horizon_end_utc_ms
                    {
                        return Err(IssuanceError::ConflictingOutcomeContract {
                            outcome_id: draft.outcome_definition.outcome_id,
                            field: "horizon",
                        });
                    }
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

                // Revisions must preserve target, outcome definition, target horizon, baseline,
                // and cannot downgrade ProspectiveStrict to RetrospectiveReconstructed (FORECAST-04, A15).
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
                if draft.baseline_version != pred.baseline_version
                    || draft.baseline_probability != pred.baseline_probability
                {
                    return Err(IssuanceError::RevisionMutatedFrozenContract {
                        forecast_id: draft.forecast_id,
                        field: "baseline",
                    });
                }
                if matches!(pred.cutoff_mode, CutoffAdmissibilityMode::ProspectiveStrict)
                    && matches!(
                        draft.cutoff_mode,
                        CutoffAdmissibilityMode::RetrospectiveReconstructed { .. }
                    )
                {
                    return Err(IssuanceError::RevisionMutatedFrozenContract {
                        forecast_id: draft.forecast_id,
                        field: "cutoff_mode",
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
                let pred_sources: BTreeSet<&VersionId> =
                    pred.sources.iter().map(|s| &s.source_version).collect();
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
                    if pred_sources.contains(ev) {
                        return Err(IssuanceError::RevisionEvidenceNotNew {
                            forecast_id: draft.forecast_id,
                            evidence_version: ev.clone(),
                        });
                    }
                }
                series.len() + 1
            }
        };

        self.outcome_contracts
            .entry(draft.outcome_definition.outcome_id.clone())
            .or_insert_with(|| RegisteredOutcomeContract {
                outcome_definition: draft.outcome_definition.clone(),
                target_id: draft.target_id.clone(),
                horizon_start_utc_ms: draft.horizon_start_utc_ms,
                horizon_end_utc_ms: draft.horizon_end_utc_ms,
            });

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
            analogue_scenario: None,
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

    /// Issue an immutable forecast vintage with a frozen [`crate::analogues::IssuedAnalogueScenarioSnapshot`] (`FORECAST-03`, `FORECAST-04`, `A15`, `P1`).
    pub(crate) fn issue_forecast_with_analogue_scenario(
        &mut self,
        draft: ForecastDraft,
        analogue_scenario: crate::analogues::IssuedAnalogueScenarioSnapshot,
    ) -> Result<&ForecastVintageRecord, IssuanceError> {
        let ver_key = draft.version_id.clone();
        self.issue_forecast(draft)?;
        if let Some(record) = self.vintages_by_version.get_mut(&ver_key) {
            record.analogue_scenario = Some(analogue_scenario);
        }
        self.vintages_by_version
            .get(&ver_key)
            .ok_or(IssuanceError::UnknownEvaluationReference {
                kind: "forecast",
                version_id: ver_key,
            })
    }

    /// Record an independent outcome resolution version (`docs/architecture.md` §6.4).
    ///
    /// If `supersedes_version` is set, the prior resolution remains intact in the ledger
    /// and any [`EvaluationRunRecord`] that pinned an earlier resolution version for the
    /// same `outcome_id` is flagged with `needs_reassessment = true`.
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

        let contract = self
            .outcome_contracts
            .get(&resolution.outcome_id)
            .ok_or_else(|| {
                IssuanceError::UnknownOutcomeDefinition(resolution.outcome_id.clone())
            })?;

        validate_timestamp_range("recorded_at_utc_ms", resolution.recorded_at_utc_ms)?;
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
                occurred: true,
                observed_event_utc_ms: None,
            } => {
                return Err(IssuanceError::InvalidOutcomeResolution(
                    "occurrence resolution (occurred: true) must supply observed_event_utc_ms",
                ));
            }
            ResolutionState::Resolved {
                occurred: true,
                observed_event_utc_ms: Some(obs_ms),
            } => {
                validate_timestamp_range("observed_event_utc_ms", *obs_ms)?;
                if *obs_ms > resolution.recorded_at_utc_ms {
                    return Err(IssuanceError::InvalidOutcomeResolution(
                        "observed_event_utc_ms cannot be after recorded_at_utc_ms",
                    ));
                }
                if *obs_ms < contract.horizon_start_utc_ms || *obs_ms >= contract.horizon_end_utc_ms
                {
                    return Err(IssuanceError::ResolutionOutsideTargetHorizon {
                        outcome_id: resolution.outcome_id,
                        occurred: true,
                        observed_event_utc_ms: *obs_ms,
                        horizon_start_utc_ms: contract.horizon_start_utc_ms,
                        horizon_end_utc_ms: contract.horizon_end_utc_ms,
                    });
                }
                if resolution.evidence_versions.is_empty() {
                    return Err(IssuanceError::InvalidOutcomeResolution(
                        "resolved outcome must cite at least one evidence version",
                    ));
                }
            }
            ResolutionState::Resolved {
                occurred: false,
                observed_event_utc_ms,
            } => {
                if resolution.recorded_at_utc_ms < contract.horizon_end_utc_ms {
                    return Err(IssuanceError::NonOccurrenceResolvedBeforeHorizonEnd {
                        outcome_id: resolution.outcome_id,
                        recorded_at_utc_ms: resolution.recorded_at_utc_ms,
                        horizon_end_utc_ms: contract.horizon_end_utc_ms,
                    });
                }
                if let Some(obs_ms) = observed_event_utc_ms {
                    validate_timestamp_range("observed_event_utc_ms", *obs_ms)?;
                    if *obs_ms > resolution.recorded_at_utc_ms {
                        return Err(IssuanceError::InvalidOutcomeResolution(
                            "observed_event_utc_ms cannot be after recorded_at_utc_ms",
                        ));
                    }
                    if *obs_ms >= contract.horizon_start_utc_ms
                        && *obs_ms < contract.horizon_end_utc_ms
                    {
                        return Err(IssuanceError::ResolutionOutsideTargetHorizon {
                            outcome_id: resolution.outcome_id,
                            occurred: false,
                            observed_event_utc_ms: *obs_ms,
                            horizon_start_utc_ms: contract.horizon_start_utc_ms,
                            horizon_end_utc_ms: contract.horizon_end_utc_ms,
                        });
                    }
                }
                if resolution.evidence_versions.is_empty() {
                    return Err(IssuanceError::InvalidOutcomeResolution(
                        "resolved outcome must cite at least one evidence version",
                    ));
                }
            }
            ResolutionState::Unresolved(UnresolvedReason::Disputed) => {
                if resolution.evidence_versions.is_empty() {
                    return Err(IssuanceError::InvalidOutcomeResolution(
                        "disputed outcome must cite at least one evidence version",
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
                if resolution.resolution_id != prior.resolution_id {
                    return Err(IssuanceError::InvalidOutcomeResolution(
                        "superseding resolution must retain the same resolution_id as its predecessor",
                    ));
                }
                if resolution.recorded_at_utc_ms < prior.recorded_at_utc_ms {
                    return Err(IssuanceError::InvalidOutcomeResolution(
                        "superseding resolution recorded_at_utc_ms cannot precede superseded resolution",
                    ));
                }

                // Flag dependent evaluation runs pinned to ANY earlier resolution version of this outcome
                // without editing their historical scores.
                let prior_versions: BTreeSet<&VersionId> = history.iter().collect();
                for eval in self.evaluations_by_version.values_mut() {
                    let mut pinned_prior_versions = Vec::new();
                    for v in &eval.vintages {
                        if prior_versions.contains(&v.resolution_version)
                            && !pinned_prior_versions.contains(&v.resolution_version)
                        {
                            pinned_prior_versions.push(v.resolution_version.clone());
                        }
                    }
                    for old_ver in pinned_prior_versions {
                        let reason = ReassessmentReason::OutcomeResolutionSuperseded {
                            outcome_id: resolution.outcome_id.clone(),
                            previous_resolution_version: old_ver,
                            new_resolution_version: resolution.version_id.clone(),
                        };
                        eval.needs_reassessment = true;
                        if !eval.reassessment_reasons.contains(&reason) {
                            eval.reassessment_reasons.push(reason);
                        }
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

    /// Evaluate a caller-selected cohort of `(forecast_version, resolution_version)` pairs.
    ///
    /// The caller selects which forecast series enter the cohort, but whenever any revised
    /// vintage of a series is included, all earlier vintages (`1..vintage_number`) of that
    /// series must also be included in the cohort so losing originals cannot be dropped.
    /// Records an immutable [`EvaluationRunRecord`].
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
        if self.total_evaluated_vintages.saturating_add(cohort.len()) > self.max_records {
            return Err(IssuanceError::BudgetExceeded {
                field: "total_evaluated_vintages",
                actual: self.total_evaluated_vintages.saturating_add(cohort.len()),
                limit: self.max_records,
            });
        }
        validate_timestamp_range("evaluated_at_utc_ms", evaluated_at_utc_ms)?;
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
        for (forecast_ver, _) in cohort {
            if !seen_forecast_versions.insert(forecast_ver) {
                return Err(IssuanceError::Scoring(
                    ScoringError::DuplicateForecastVersion(forecast_ver.clone()),
                ));
            }
        }

        let mut binary_rows = Vec::with_capacity(cohort.len());
        let mut row_indices = Vec::with_capacity(cohort.len());

        for (forecast_ver, resolution_ver) in cohort {
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

            // Require all predecessor vintages of an included series to be present in the cohort.
            if vintage.vintage_number > 1
                && let Some(series) = self.series_vintages.get(&vintage.forecast_id)
            {
                for earlier_ver in series.iter().take(vintage.vintage_number - 1) {
                    if !seen_forecast_versions.contains(earlier_ver) {
                        return Err(IssuanceError::IncompleteForecastSeriesInCohort {
                            forecast_id: vintage.forecast_id.clone(),
                            missing_vintage: earlier_ver.clone(),
                        });
                    }
                }
            }

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
            if evaluated_at_utc_ms < vintage.issued_at_utc_ms {
                return Err(IssuanceError::EvaluationPredatesInputs {
                    evaluation_version,
                    evaluated_at_utc_ms,
                    input_version: vintage.version_id.clone(),
                    input_timestamp_utc_ms: vintage.issued_at_utc_ms,
                });
            }
            if evaluated_at_utc_ms < resolution.recorded_at_utc_ms {
                return Err(IssuanceError::EvaluationPredatesInputs {
                    evaluation_version,
                    evaluated_at_utc_ms,
                    input_version: resolution.version_id.clone(),
                    input_timestamp_utc_ms: resolution.recorded_at_utc_ms,
                });
            }

            match &resolution.state {
                ResolutionState::Resolved {
                    occurred,
                    observed_event_utc_ms,
                } => {
                    if let Some(obs_ms) = observed_event_utc_ms
                        && (*obs_ms <= vintage.information_cutoff_utc_ms
                            || *obs_ms <= vintage.origin_utc_ms
                            || *obs_ms <= vintage.issued_at_utc_ms)
                    {
                        return Err(IssuanceError::EventObservedAtOrBeforeForecastIssuance {
                            forecast_version: vintage.version_id.clone(),
                            information_cutoff_utc_ms: vintage.information_cutoff_utc_ms,
                            issued_at_utc_ms: vintage.issued_at_utc_ms,
                            observed_event_utc_ms: *obs_ms,
                        });
                    }
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

            // If this vintage's frozen comparison_set cites an already-superseded precedent episode version,
            // disclose that immediately (`FORECAST-02`, `FORECAST-04`, `AGENTS.md` §4).
            for comp_ver in &vintage.comparison_set {
                if let Some((ep_id, latest_ep_ver)) = self.superseded_precedents.get(comp_ver) {
                    let reason = ReassessmentReason::PrecedentEpisodeSuperseded {
                        episode_id: ep_id.clone(),
                        previous_episode_version: comp_ver.clone(),
                        new_episode_version: latest_ep_ver.clone(),
                    };
                    if !reassessment_reasons.contains(&reason) {
                        reassessment_reasons.push(reason);
                    }
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
                    observed_event_utc_ms: Some(obs_ms),
                    ..
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

        self.total_evaluated_vintages = self.total_evaluated_vintages.saturating_add(cohort.len());
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

    /// Return the immutable [`VersionId`]s of all issued forecast vintages whose frozen
    /// `comparison_set` includes `comparison_version` (`FORECAST-02`, `FORECAST-04`).
    #[must_use]
    pub fn vintages_citing_comparison_version(
        &self,
        comparison_version: &VersionId,
    ) -> Vec<VersionId> {
        self.vintages_by_version
            .values()
            .filter(|v| v.comparison_set.contains(comparison_version))
            .map(|v| v.version_id.clone())
            .collect()
    }

    /// Record that a historical precedent episode's prior version(s) were superseded by
    /// `new_episode_version`, flagging any dependent [`EvaluationRunRecord`] whose evaluated
    /// vintages cited one of `prior_episode_versions` in `comparison_set` without rewriting
    /// the issued vintages or historical scores (`FORECAST-02`, `FORECAST-04`, `AGENTS.md` §4).
    ///
    /// Returns the list of issued forecast vintage [`VersionId`]s whose frozen `comparison_set`
    /// cited any of `prior_episode_versions`.
    pub(crate) fn record_precedent_supersession(
        &mut self,
        episode_id: &OpaqueId,
        prior_episode_versions: &[VersionId],
        new_episode_version: &VersionId,
    ) -> Vec<VersionId> {
        let prior_set: BTreeSet<&VersionId> = prior_episode_versions.iter().collect();
        for old_ver in prior_episode_versions {
            self.superseded_precedents.insert(
                old_ver.clone(),
                (episode_id.clone(), new_episode_version.clone()),
            );
        }

        let mut affected_vintages = Vec::new();
        for vintage in self.vintages_by_version.values() {
            if vintage
                .comparison_set
                .iter()
                .any(|ver| prior_set.contains(ver))
            {
                affected_vintages.push(vintage.version_id.clone());
            }
        }

        let vintages_by_ver = &self.vintages_by_version;
        for eval in self.evaluations_by_version.values_mut() {
            let mut pinned_old_versions = Vec::new();
            for ev_vintage in &eval.vintages {
                if let Some(vintage_rec) = vintages_by_ver.get(&ev_vintage.forecast_version) {
                    for comp_ver in &vintage_rec.comparison_set {
                        if prior_set.contains(comp_ver) && !pinned_old_versions.contains(comp_ver) {
                            pinned_old_versions.push(comp_ver.clone());
                        }
                    }
                }
            }
            for old_ver in pinned_old_versions {
                let reason = ReassessmentReason::PrecedentEpisodeSuperseded {
                    episode_id: episode_id.clone(),
                    previous_episode_version: old_ver,
                    new_episode_version: new_episode_version.clone(),
                };
                eval.needs_reassessment = true;
                if !eval.reassessment_reasons.contains(&reason) {
                    eval.reassessment_reasons.push(reason);
                }
            }
        }

        affected_vintages
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

fn validate_timestamp_range(field: &'static str, value: i64) -> Result<(), IssuanceError> {
    if !(MIN_VALID_UTC_MS..=MAX_VALID_UTC_MS).contains(&value) {
        return Err(IssuanceError::TimestampOutOfRange { field, value });
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

/// Shared per-source cutoff admissibility check used by [`ForecastLedger`] and [`crate::analogues`] (`AGENTS.md` §4).
///
/// Validates `public_available_at_utc_ms`, optional `event_utc_ms`, and optional `ingested_at_utc_ms`
/// against `cutoff_utc_ms` and `cutoff_mode`, returning `public_available_at_utc_ms` when admissible.
pub(crate) fn validate_source_admissible_at_cutoff(
    src: &CutoffSourceInput,
    cutoff_utc_ms: i64,
    cutoff_mode: &CutoffAdmissibilityMode,
) -> Result<i64, IssuanceError> {
    let Some(pub_ms) = src.public_available_at_utc_ms else {
        return Err(IssuanceError::SourcePublicAvailabilityAfterCutoff {
            source_version: src.source_version.clone(),
            public_available_at_utc_ms: None,
            cutoff_utc_ms,
        });
    };
    validate_timestamp_range("sources[].public_available_at_utc_ms", pub_ms)?;
    if pub_ms > cutoff_utc_ms {
        return Err(IssuanceError::SourcePublicAvailabilityAfterCutoff {
            source_version: src.source_version.clone(),
            public_available_at_utc_ms: Some(pub_ms),
            cutoff_utc_ms,
        });
    }
    if let Some(event_ms) = src.event_utc_ms {
        validate_timestamp_range("sources[].event_utc_ms", event_ms)?;
        let bound = pub_ms.min(cutoff_utc_ms);
        if event_ms > bound {
            return Err(IssuanceError::SourceEventAfterAvailabilityOrCutoff {
                source_version: src.source_version.clone(),
                event_utc_ms: event_ms,
                bound_utc_ms: bound,
            });
        }
    }
    if let Some(ing_ms) = src.ingested_at_utc_ms {
        validate_timestamp_range("sources[].ingested_at_utc_ms", ing_ms)?;
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
    Ok(pub_ms)
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
    if let Some(mc) = model_knowledge_cutoff_utc_ms {
        validate_timestamp_range("model_knowledge_cutoff_utc_ms", mc)?;
    }

    let post_cutoff_model_knowledge_disclosed = match (cutoff_mode, model_knowledge_cutoff_utc_ms) {
        (CutoffAdmissibilityMode::ProspectiveStrict, None) => {
            return Err(IssuanceError::ModelKnowledgeAfterStrictCutoff {
                model_version: model_version.clone(),
                model_knowledge_cutoff_utc_ms: None,
                cutoff_utc_ms,
            });
        }
        (CutoffAdmissibilityMode::ProspectiveStrict, Some(mc)) if mc > cutoff_utc_ms => {
            return Err(IssuanceError::ModelKnowledgeAfterStrictCutoff {
                model_version: model_version.clone(),
                model_knowledge_cutoff_utc_ms: Some(mc),
                cutoff_utc_ms,
            });
        }
        (CutoffAdmissibilityMode::RetrospectiveReconstructed { .. }, None) => true,
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
        validate_source_admissible_at_cutoff(src, cutoff_utc_ms, cutoff_mode)?;
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
            early_by_ms: horizon_start_utc_ms - observed_event_utc_ms,
        }
    } else if observed_event_utc_ms >= horizon_end_utc_ms {
        HorizonTiming::Late {
            late_by_ms: observed_event_utc_ms - horizon_end_utc_ms,
        }
    } else {
        HorizonTiming::WithinHorizon
    }
}
