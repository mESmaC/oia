//! Historical precedent episodes, editable analogue comparison sets, conditional response
//! sequences, and `A15` forecast-lifecycle integration (`FORECAST-01`..=`FORECAST-04`, `#45`).
//!
//! This module implements the deterministic, in-memory historical-analogue forecasting kernel:
//! - Sourced event-response histories across government, institutional, industry, and community
//!   actors, keeping temporal sequence (`TemporalSequenceRelation::ObservedSequenceOnly`) distinct
//!   from assessed causation (`TemporalSequenceRelation::AssessedCausalLink`) and historical facts
//!   distinct from point-in-time knowability at a forecast's information cutoff (`FORECAST-01`).
//! - Multi-dimension analogue retrieval across event scale, geography, actor profile,
//!   institutional regime, tension level, economic condition, and information quality, with
//!   inspectable similarity/difference explanations, editable comparison sets (`ComparisonSetEdit`),
//!   counterexamples, documented non-response, censored follow-up, and cluster/lineage independence
//!   accounting (`FORECAST-02`).
//! - Conditional multi-step response branches, elapsed-time ranges, monitoring indicators,
//!   transferability assessments (`TransferabilityAssessment`), and uncalibrated historical
//!   frequency summaries that never masquerade as calibrated probabilities (`FORECAST-03`).
//! - Direct bridging into [`crate::issuance::ForecastLedger`] so original and revised analogue
//!   forecasts remain immutable and evaluable alongside base-rate baselines, reliability summaries,
//!   elapsed-time error (`AnalogueTimingEvaluation`), precedent corrections, and unresolved outcomes
//!   (`FORECAST-04`, `A15`).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use oia_domain::{ActorId, OpaqueId, SchemaVersion, VersionId};

use crate::issuance::{
    CutoffAdmissibilityMode, CutoffSourceInput, ForecastDraft, ForecastLedger,
    ForecastVintageRecord, IssuanceError, MAX_FORECAST_COLLECTION_LEN, MAX_TEXT_FIELD_BYTES,
    MAX_VALID_UTC_MS, MIN_VALID_UTC_MS, OutcomeDefinition, OutcomeResolutionRecord,
    ResolutionState, RevisionRationale, validate_source_admissible_at_cutoff,
};
use crate::scoring::{Probability, ScoringError, UnresolvedReason};

/// Maximum number of precedent episodes stored in a single [`PrecedentLibrary`].
pub const MAX_PRECEDENT_EPISODES: usize = 5_000;

/// Maximum number of actor response steps in a single [`PrecedentEpisodeRecord`].
pub const MAX_RESPONSE_STEPS_PER_EPISODE: usize = 64;

/// Maximum number of edits recorded in an [`AnalogueComparisonSet`].
pub const MAX_COMPARISON_EDITS: usize = 1_024;

/// Maximum number of conditional response branches in a scenario model.
pub const MAX_SCENARIO_BRANCHES: usize = 64;

/// Maximum number of steps in a single [`ConditionalResponseBranch`].
pub const MAX_BRANCH_STEPS: usize = 16;

/// Maximum number of monitoring indicators attached to a scenario model.
pub const MAX_MONITORING_INDICATORS: usize = 128;

/// Tolerance for checking that a multi-step branch's chained probability matches the product
/// of its step conditional probabilities.
pub const CHAINED_PROBABILITY_TOLERANCE: f64 = 1e-9;

/// Mandatory caveat paired with every [`HistoricalFrequencySummary`] (`FORECAST-03`, `A15`, `AGENTS.md` §2).
pub const UNCALIBRATED_HISTORICAL_FREQUENCY_CAVEAT: &str = "Empirical historical precedent frequencies and independent cluster ratios are descriptive summaries of the selected comparison set, not calibrated event probabilities; empirical calibration requires held-out prospective or chronological evaluation.";

/// Mandatory caveat paired with observed chronological response steps (`FORECAST-01`, `AGENTS.md` §2).
pub const TEMPORAL_SEQUENCE_NOT_CAUSATION_CAVEAT: &str = "Observed chronological ordering within a precedent episode records temporal sequence only and does not establish causation unless backed by an explicit AssessedCausalLink and cited evidence.";

/// Mandatory note explaining documented non-response (`FORECAST-02`, `A15`).
pub const DOCUMENTED_NON_RESPONSE_NOTE: &str = "Documented non-response records an observed window in which the actor had a known trigger and took no overt action; it is distinct from missing data or censored follow-up.";

/// Actor sector for an event-response precedent (`FORECAST-01`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PrecedentSector {
    Government,
    Institution,
    Industry,
    Community,
}

impl fmt::Display for PrecedentSector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Government => write!(f, "government"),
            Self::Institution => write!(f, "institution"),
            Self::Industry => write!(f, "industry"),
            Self::Community => write!(f, "community"),
        }
    }
}

/// Scale of the initiating event (`FORECAST-02`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventScale {
    Local,
    Regional,
    National,
    MultilateralOrGlobal,
}

impl fmt::Display for EventScale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local => write!(f, "local"),
            Self::Regional => write!(f, "regional"),
            Self::National => write!(f, "national"),
            Self::MultilateralOrGlobal => write!(f, "multilateral_or_global"),
        }
    }
}

/// Institutional governance context shaping actor response constraints (`FORECAST-02`, `A15`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InstitutionalRegime {
    DemocraticStatutory,
    RegulatoryAgency,
    ExecutiveEmergency,
    AuthoritarianCentralized,
    CorporateGovernance,
    CommunityInformal,
}

impl fmt::Display for InstitutionalRegime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DemocraticStatutory => write!(f, "democratic_statutory"),
            Self::RegulatoryAgency => write!(f, "regulatory_agency"),
            Self::ExecutiveEmergency => write!(f, "executive_emergency"),
            Self::AuthoritarianCentralized => write!(f, "authoritarian_centralized"),
            Self::CorporateGovernance => write!(f, "corporate_governance"),
            Self::CommunityInformal => write!(f, "community_informal"),
        }
    }
}

/// Baseline political, security, or operational tension level (`FORECAST-02`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TensionLevel {
    Routine,
    Elevated,
    Crisis,
    AcuteConflict,
}

impl fmt::Display for TensionLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Routine => write!(f, "routine"),
            Self::Elevated => write!(f, "elevated"),
            Self::Crisis => write!(f, "crisis"),
            Self::AcuteConflict => write!(f, "acute_conflict"),
        }
    }
}

/// Prevailing macroeconomic or sector condition (`FORECAST-02`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EconomicCondition {
    StableExpansion,
    InflationaryOrSupplyStress,
    RecessionaryOrCreditStress,
    AcuteMarketDislocation,
}

impl fmt::Display for EconomicCondition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StableExpansion => write!(f, "stable_expansion"),
            Self::InflationaryOrSupplyStress => write!(f, "inflationary_or_supply_stress"),
            Self::RecessionaryOrCreditStress => write!(f, "recessionary_or_credit_stress"),
            Self::AcuteMarketDislocation => write!(f, "acute_market_dislocation"),
        }
    }
}

/// Quality and completeness of reporting on the episode (`FORECAST-02`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InformationQuality {
    HighVerifiedPrimary,
    ModerateMixedSources,
    LowFragmentaryOrDelayed,
    ContestedOrCensored,
}

impl fmt::Display for InformationQuality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HighVerifiedPrimary => write!(f, "high_verified_primary"),
            Self::ModerateMixedSources => write!(f, "moderate_mixed_sources"),
            Self::LowFragmentaryOrDelayed => write!(f, "low_fragmentary_or_delayed"),
            Self::ContestedOrCensored => write!(f, "contested_or_censored"),
        }
    }
}

/// Seven-dimension context profile used for explicit analogue retrieval (`FORECAST-02`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalogueProfile {
    pub event_scale: EventScale,
    pub geography_region: String,
    pub actor_sector: PrecedentSector,
    pub actor_profile: String,
    pub institutional_regime: InstitutionalRegime,
    pub tension_level: TensionLevel,
    pub economic_condition: EconomicCondition,
    pub information_quality: InformationQuality,
}

/// Explicit distinction between chronological ordering and assessed causal linkage (`FORECAST-01`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemporalSequenceRelation {
    /// Step occurred after the initiating event or prior step, without claiming causation.
    ObservedSequenceOnly,
    /// Assessed causal link backed by explicit rationale and cited evidence versions.
    AssessedCausalLink {
        rationale: String,
        supporting_evidence_versions: Vec<VersionId>,
    },
}

/// One actor's announced intention and observed action within an episode (`FORECAST-01`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActorResponseStep {
    pub step_id: OpaqueId,
    pub actor_id: ActorId,
    pub action_summary: String,
    /// Announced policy or intention, kept distinct from actual action and outcome (`FORECAST-01`).
    pub announced_intention: Option<String>,
    pub action_utc_ms: i64,
    /// Elapsed milliseconds since `initiating_event_utc_ms` (`action_utc_ms - initiating_event_utc_ms`).
    pub elapsed_from_initiation_ms: i64,
    pub sequence_relation: TemporalSequenceRelation,
    pub evidence_versions: Vec<VersionId>,
}

/// Terminal disposition of a historical precedent episode (`FORECAST-01`, `FORECAST-02`, `A15`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrecedentOutcomeDisposition {
    /// The focal response occurred at `resolved_at_utc_ms`.
    TargetResponseOccurred {
        outcome_summary: String,
        resolved_at_utc_ms: i64,
        elapsed_from_initiation_ms: i64,
        outcome_evidence_versions: Vec<VersionId>,
    },
    /// The actor took a materially divergent response instead of the focal response.
    DivergentResponseOccurred {
        divergent_action_summary: String,
        resolved_at_utc_ms: i64,
        elapsed_from_initiation_ms: i64,
        outcome_evidence_versions: Vec<VersionId>,
    },
    /// Follow-up across `[initiating_event_utc_ms, observed_window_end_utc_ms)` documented that
    /// the actor took no response. Kept distinct from missing data (`FORECAST-02`, `A15`).
    DocumentedNonResponse {
        observed_window_end_utc_ms: i64,
        observation_duration_ms: i64,
        non_response_rationale: String,
        outcome_evidence_versions: Vec<VersionId>,
    },
    /// Follow-up ended early or remained incomplete/disputed as of `observed_until_utc_ms`.
    /// Never coerced to `DocumentedNonResponse` (`FORECAST-02`, `A15`).
    CensoredOrIncompleteFollowUp {
        observed_until_utc_ms: i64,
        censor_reason: UnresolvedReason,
        coverage_gap_note: String,
        partial_evidence_versions: Vec<VersionId>,
    },
}

impl PrecedentOutcomeDisposition {
    /// Return the outcome or partial evidence versions cited by this disposition.
    #[must_use]
    pub fn cited_evidence_versions(&self) -> &[VersionId] {
        match self {
            Self::TargetResponseOccurred {
                outcome_evidence_versions,
                ..
            }
            | Self::DivergentResponseOccurred {
                outcome_evidence_versions,
                ..
            }
            | Self::DocumentedNonResponse {
                outcome_evidence_versions,
                ..
            } => outcome_evidence_versions,
            Self::CensoredOrIncompleteFollowUp {
                partial_evidence_versions,
                ..
            } => partial_evidence_versions,
        }
    }
}

/// Episode dependency and lineage classification (`FORECAST-02`, `A15`).
///
/// Related episodes sharing a crisis cluster, policy cascade, or syndicated source lineage
/// must not automatically count as independent precedents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EpisodeDependencyKind {
    /// Episode has no declared shared crisis cluster or single-source duplication.
    IndependentEpisode,
    /// Multiple actor or regional episodes triggered by the same crisis or coordinated campaign.
    SharedCrisisOrCampaign {
        cluster_id: OpaqueId,
        dependency_note: String,
    },
    /// Episode is a retaliatory or spillover consequence of an earlier episode in the same cluster.
    PolicySpilloverOrRetaliation {
        cluster_id: OpaqueId,
        predecessor_episode_id: OpaqueId,
        dependency_note: String,
    },
    /// Multiple records trace back to a single upstream report or syndicated lineage (`AGENTS.md` §2).
    SharedSourceLineageOnly {
        upstream_source_version: VersionId,
        dependency_note: String,
    },
}

/// Versioned historical event-response episode (`FORECAST-01`, `FORECAST-02`, `FORECAST-04`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrecedentEpisodeRecord {
    pub episode_id: OpaqueId,
    pub version_id: VersionId,
    pub schema_version: SchemaVersion,
    pub recorded_by: ActorId,
    /// Timestamp when this precedent episode version was curated/recorded in OIA (`FORECAST-01`, `FORECAST-04`).
    /// Under [`CutoffAdmissibilityMode::ProspectiveStrict`], `recorded_at_utc_ms` must be `<= information_cutoff_utc_ms`
    /// so hindsight-curated precedent tags or dependency classifications cannot leak into strict prospective retrieval.
    pub recorded_at_utc_ms: i64,
    pub title: String,
    pub initiating_condition: String,
    pub initiating_event_utc_ms: i64,
    pub initiation_evidence_versions: Vec<VersionId>,
    pub profile: AnalogueProfile,
    pub dependency: EpisodeDependencyKind,
    pub response_steps: Vec<ActorResponseStep>,
    pub outcome: PrecedentOutcomeDisposition,
    pub sources: Vec<CutoffSourceInput>,
    /// Prior episode `VersionId` superseded when a later source or outcome correction updates this precedent (`FORECAST-04`, `AGENTS.md` §4).
    pub supersedes_version: Option<VersionId>,
    /// Mandatory explanation when `supersedes_version` is `Some`.
    pub correction_rationale: Option<String>,
}

impl PrecedentEpisodeRecord {
    /// Key used to deduplicate dependent episodes when counting independent precedent clusters.
    #[must_use]
    pub fn independence_cluster_key(&self) -> String {
        match &self.dependency {
            EpisodeDependencyKind::IndependentEpisode => {
                format!("episode:{}", self.episode_id.as_str())
            }
            EpisodeDependencyKind::SharedCrisisOrCampaign { cluster_id, .. }
            | EpisodeDependencyKind::PolicySpilloverOrRetaliation { cluster_id, .. } => {
                format!("cluster:{}", cluster_id.as_str())
            }
            EpisodeDependencyKind::SharedSourceLineageOnly {
                upstream_source_version,
                ..
            } => {
                format!("lineage:{}", upstream_source_version.as_str())
            }
        }
    }
}

/// What was knowable about a [`PrecedentEpisodeRecord`] at a forecast's `information_cutoff_utc_ms`
/// (`FORECAST-01`, `FORECAST-04`, `A15`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EpisodePointInTimeStatus {
    /// Both the initiating event and the terminal outcome (`TargetResponseOccurred`,
    /// `DivergentResponseOccurred`, or `DocumentedNonResponse`) occurred and were supported
    /// by cutoff-admissible evidence at or before `information_cutoff_utc_ms`.
    OutcomeKnowableAtCutoff {
        resolved_at_utc_ms: i64,
        latest_outcome_source_available_utc_ms: i64,
        admissible_response_steps: usize,
    },
    /// The initiating event and its evidence were admissible at `information_cutoff_utc_ms`,
    /// but the episode's outcome was either inherently `CensoredOrIncompleteFollowUp` or
    /// resolved / became publicly available only *after* `information_cutoff_utc_ms`.
    ///
    /// Treating a post-cutoff resolution as known at `information_cutoff_utc_ms` would introduce
    /// lookahead leakage (`FORECAST-01`, `FORECAST-04`).
    InitiatedButOutcomeCensoredAtCutoff {
        initiated_at_utc_ms: i64,
        censor_reason: UnresolvedReason,
        explanation: String,
        admissible_response_steps: usize,
    },
    /// The initiating event occurred after `information_cutoff_utc_ms`, the episode record was
    /// curated after `information_cutoff_utc_ms` under `ProspectiveStrict`, or its initiation
    /// evidence was not admissible under the active [`CutoffAdmissibilityMode`].
    InadmissibleAtCutoff { reason: String },
}

/// Retrieval dimension compared between a target scenario and a precedent episode (`FORECAST-02`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AnalogueDimension {
    EventScale,
    Geography,
    ActorSector,
    InstitutionalRegime,
    TensionLevel,
    EconomicCondition,
    InformationQuality,
}

impl fmt::Display for AnalogueDimension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EventScale => write!(f, "event_scale"),
            Self::Geography => write!(f, "geography"),
            Self::ActorSector => write!(f, "actor_sector"),
            Self::InstitutionalRegime => write!(f, "institutional_regime"),
            Self::TensionLevel => write!(f, "tension_level"),
            Self::EconomicCondition => write!(f, "economic_condition"),
            Self::InformationQuality => write!(f, "information_quality"),
        }
    }
}

/// Per-dimension similarity or difference explanation (`FORECAST-02`, `A15`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DimensionAssessment {
    pub dimension: AnalogueDimension,
    pub matches_target: bool,
    pub target_value: String,
    pub precedent_value: String,
    pub explanation: String,
}

/// Evidentiary role of an admissible precedent episode at the forecast cutoff (`FORECAST-02`, `A15`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrecedentRole {
    /// Outcome was knowable at cutoff and matched `TargetResponseOccurred`.
    SupportiveAnalogue,
    /// Outcome was knowable at cutoff and was `DivergentResponseOccurred`.
    DivergentCounterexample,
    /// Outcome was knowable at cutoff and was `DocumentedNonResponse`.
    DocumentedNonResponseCounterexample,
    /// Episode initiated at or before cutoff, but its outcome was censored or not yet knowable at cutoff.
    CensoredAtCutoff,
}

/// One precedent episode evaluated against [`AnalogueQueryCriteria`] (`FORECAST-02`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalogueMatchEntry {
    pub episode_id: OpaqueId,
    pub episode_version: VersionId,
    pub role: PrecedentRole,
    pub point_in_time_status: EpisodePointInTimeStatus,
    pub matched_dimension_count: usize,
    pub dimension_assessments: Vec<DimensionAssessment>,
    pub institutional_difference_note: Option<String>,
    pub transfer_limitations: Vec<String>,
    pub independence_cluster_key: String,
    pub dependency: EpisodeDependencyKind,
    /// Elapsed milliseconds from initiation to target response, present only when `role == SupportiveAnalogue`.
    pub knowable_target_elapsed_ms: Option<i64>,
    /// Cutoff-admissible source inputs for this episode at `information_cutoff_utc_ms`.
    pub cutoff_admissible_sources: Vec<CutoffSourceInput>,
    /// Outcome evidence versions cited by this episode, used to detect shared-source lineage across episodes.
    pub cited_outcome_evidence_versions: Vec<VersionId>,
}

/// Explicit criteria for retrieving candidate historical precedents (`FORECAST-02`, `A15`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalogueQueryCriteria {
    pub comparison_set_id: OpaqueId,
    pub initial_version_id: VersionId,
    pub actor: ActorId,
    pub created_at_utc_ms: i64,
    pub information_cutoff_utc_ms: i64,
    pub cutoff_mode: CutoffAdmissibilityMode,
    pub target_profile: AnalogueProfile,
    /// Minimum number of matching dimensions (`1..=7`) for automatic retrieval.
    pub min_matched_dimensions: usize,
    /// Minimum number of independent resolved precedent clusters recommended before
    /// treating the precedent base as non-sparse (`FORECAST-03`).
    pub min_independent_clusters: usize,
}

/// Action recorded when an analyst edits or advances a comparison set (`FORECAST-02`, `A15`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComparisonEditAction {
    /// Manually include a cutoff-admissible episode from the library (even if below `min_matched_dimensions`).
    IncludeEpisode {
        episode_id: OpaqueId,
        rationale: String,
        transfer_limitations: Vec<String>,
    },
    /// Exclude an episode from the active comparison set while retaining it in the audit trail.
    ExcludeEpisode {
        episode_id: OpaqueId,
        rationale: String,
    },
    /// Attach or refine institutional difference and transferability notes on an active episode.
    AnnotateTransferLimits {
        episode_id: OpaqueId,
        institutional_difference_note: Option<String>,
        additional_limitations: Vec<String>,
        rationale: String,
    },
    /// Re-evaluate the comparison set at a non-decreasing `new_cutoff_utc_ms`, producing a new
    /// comparison-set version while preserving prior exclusions and annotations (`FORECAST-02`, `FORECAST-04`).
    AdvanceCutoff {
        previous_cutoff_utc_ms: i64,
        new_cutoff_utc_ms: i64,
        rationale: String,
    },
}

/// Immutable audit entry for a user edit to an [`AnalogueComparisonSet`] (`FORECAST-02`, `A15`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComparisonSetEdit {
    pub edit_version: VersionId,
    pub actor: ActorId,
    pub edited_at_utc_ms: i64,
    pub action: ComparisonEditAction,
}

/// Excluded episode retained so counterexample removal remains visible (`FORECAST-02`, `AGENTS.md` §2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExcludedAnalogueEntry {
    pub entry: AnalogueMatchEntry,
    pub excluded_by_version: VersionId,
    pub exclusion_rationale: String,
}

/// Elapsed-time distribution over resolved supportive precedents (`FORECAST-01`, `FORECAST-03`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElapsedTimeRangeMs {
    pub resolved_episode_count: usize,
    pub min_elapsed_ms: i64,
    pub median_elapsed_ms: i64,
    pub max_elapsed_ms: i64,
}

/// Descriptive historical frequency summary over an [`AnalogueComparisonSet`] (`FORECAST-02`, `FORECAST-03`).
///
/// Always carries `is_calibrated_probability == false` and [`UNCALIBRATED_HISTORICAL_FREQUENCY_CAVEAT`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricalFrequencySummary {
    pub active_episode_count: usize,
    pub independent_cluster_count: usize,
    pub dependent_episode_count: usize,
    pub resolved_knowable_episode_count: usize,
    pub supportive_target_response_count: usize,
    pub divergent_counterexample_count: usize,
    pub documented_non_response_count: usize,
    pub total_counterexample_count: usize,
    pub censored_at_cutoff_count: usize,
    pub excluded_episode_count: usize,
    pub excluded_counterexample_count: usize,
    /// Number of independent clusters whose resolved episodes are *unanimously* `SupportiveAnalogue`.
    pub independent_supportive_cluster_count: usize,
    /// Number of independent clusters whose resolved episodes are *unanimously* counterexamples
    /// (`DivergentCounterexample` or `DocumentedNonResponseCounterexample`).
    pub independent_counterexample_cluster_count: usize,
    /// Number of independent clusters containing *both* at least one `SupportiveAnalogue` and at
    /// least one counterexample (`FORECAST-02`, `B2`). Kept out of `independent_supportive_cluster_count`
    /// while remaining in the denominator `independent_resolved_cluster_count` so cluster deduplication
    /// can never inflate apparent support over a mixed-outcome cluster.
    pub mixed_outcome_cluster_count: usize,
    pub independent_resolved_cluster_count: usize,
    /// Raw support ratio in basis points (`0..=10_000`) over `resolved_knowable_episode_count`,
    /// or `None` when zero resolved episodes exist.
    pub raw_resolved_support_ratio_bps: Option<u32>,
    /// Cluster-deduplicated support ratio in basis points (`0..=10_000`), computed as the mean of
    /// per-cluster resolved support fractions (`supportive_in_cluster / resolved_in_cluster`) across
    /// all `independent_resolved_cluster_count` clusters (`None` when zero resolved clusters exist).
    ///
    /// - Unanimously supportive clusters contribute weight `1.0` (`10_000` bps).
    /// - Unanimously counterexample clusters contribute weight `0.0` (`0` bps).
    /// - Mixed-outcome clusters contribute their exact within-cluster support fraction
    ///   `supportive_in_cluster / resolved_in_cluster`, ensuring deduplication never treats a
    ///   mixed cluster as wholly supportive (`B2`).
    pub independent_cluster_support_ratio_bps: Option<u32>,
    pub target_elapsed_time_range: Option<ElapsedTimeRangeMs>,
    /// Always `false`: historical counts are not calibrated probabilities (`FORECAST-03`, `AGENTS.md` §2).
    pub is_calibrated_probability: bool,
    pub calibration_caveat: &'static str,
}

/// Specific structural reason why a comparison set is sparse or poorly transferable (`FORECAST-03`, `A15`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferLimitationReason {
    /// Fewer independent resolved precedent clusters than `min_independent_clusters`.
    SparseIndependentPrecedents {
        independent_resolved_clusters: usize,
        minimum_recommended: usize,
    },
    /// At least half of the active episodes were censored or unresolved at `information_cutoff_utc_ms`.
    HighCensoredFollowUpShare {
        censored_count: usize,
        active_count: usize,
    },
    /// Majority of active episodes occurred under a different [`InstitutionalRegime`] than the target.
    InstitutionalRegimeMismatch {
        mismatched_count: usize,
        active_count: usize,
    },
    /// Majority of active episodes rely on `LowFragmentaryOrDelayed` or `ContestedOrCensored` information.
    LowInformationQualityPrecedents {
        low_quality_count: usize,
        active_count: usize,
    },
    /// One or more counterexample episodes were manually excluded from the comparison set.
    ExcludedCounterexamplesPresent {
        excluded_counterexample_count: usize,
    },
}

/// Summary assessment of whether historical precedents transfer reliably to the target scenario (`FORECAST-03`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferabilityAssessment {
    AdequatePrecedentBasis,
    SparseOrPoorlyTransferable {
        reasons: Vec<TransferLimitationReason>,
    },
}

/// Versioned, editable comparison set of historical precedents (`FORECAST-02`, `FORECAST-03`, `A15`).
///
/// Fields are private so callers cannot bypass the sparse-history gate, lookahead cutoff checks,
/// or the comparison-set edit audit trail (`B1`). Use the public getters, [`Self::apply_edit`],
/// and [`Self::re_retrieve_at_cutoff`] to inspect or update the comparison set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalogueComparisonSet {
    comparison_set_id: OpaqueId,
    initial_version_id: VersionId,
    current_version_id: VersionId,
    created_at_utc_ms: i64,
    information_cutoff_utc_ms: i64,
    cutoff_mode: CutoffAdmissibilityMode,
    target_profile: AnalogueProfile,
    min_matched_dimensions: usize,
    min_independent_clusters: usize,
    active_entries: Vec<AnalogueMatchEntry>,
    excluded_entries: Vec<ExcludedAnalogueEntry>,
    edit_history: Vec<ComparisonSetEdit>,
    /// Episodes rejected at retrieval because their initiating event or record curation was not admissible at cutoff.
    inadmissible_episode_ids: Vec<(OpaqueId, String)>,
}

/// One conditional step in a multi-step response sequence (`FORECAST-03`, `A15`).
#[derive(Debug, Clone, PartialEq)]
pub struct ConditionalResponseStep {
    pub step_number: usize,
    pub actor_sector: PrecedentSector,
    pub action_description: String,
    /// Conditional probability `P(step_k | step_1..step_{k-1})` in `0.0..=1.0`.
    pub conditional_probability: Probability,
    /// Mandatory explanation of how this step depends on prior steps or initiating conditions (`FORECAST-03`).
    pub dependency_rationale: String,
    pub expected_step_window_ms: Option<(i64, i64)>,
}

/// Alternative conditional response branch (`FORECAST-03`, `A15`).
#[derive(Debug, Clone, PartialEq)]
pub struct ConditionalResponseBranch {
    pub branch_id: OpaqueId,
    pub branch_label: String,
    /// True if this branch represents documented non-response or a divergent counter-scenario.
    pub is_non_response_or_divergent: bool,
    pub steps: Vec<ConditionalResponseStep>,
    /// Joint branch probability equal to the product of `steps[i].conditional_probability`
    /// within [`CHAINED_PROBABILITY_TOLERANCE`].
    pub chained_probability: Probability,
    pub expected_elapsed_range_ms: Option<ElapsedTimeRangeMs>,
    pub supporting_episode_versions: Vec<VersionId>,
}

/// Direction of an observable indicator relative to a [`ConditionalResponseBranch`] (`FORECAST-03`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndicatorDirection {
    ConfirmsBranch,
    DisconfirmsBranch,
    IndicatesNonResponse,
}

/// Observable indicator to monitor as a scenario unfolds (`FORECAST-03`, `A15`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitoringIndicator {
    pub indicator_id: OpaqueId,
    pub observable_signal: String,
    pub target_branch_id: OpaqueId,
    pub direction: IndicatorDirection,
    pub source_coverage_limits: String,
    pub false_positive_caveat: String,
}

/// Whether a scenario supports issuing a quantitative probability forecast or remains qualitative (`FORECAST-03`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PredictionJustification {
    /// Quantitative forecast justified by an explicit base-rate/adjustment rationale; if the
    /// comparison set is [`TransferabilityAssessment::SparseOrPoorlyTransferable`],
    /// `sparse_history_override_rationale` must be `Some` and non-empty.
    JustifiedQuantitativeForecast {
        calibration_and_adjustment_rationale: String,
        sparse_history_override_rationale: Option<String>,
    },
    /// Precedent history is too sparse or poorly transferable to justify numerical probability issuance.
    QualitativeScenarioOnly { reason: String },
}

/// Immutable snapshot of the conditional response branches, monitoring indicators, transferability
/// assessment, and justification frozen onto an issued [`ForecastVintageRecord`] (`FORECAST-03`, `FORECAST-04`, `A15`, `P1`).
#[derive(Debug, Clone, PartialEq)]
pub struct IssuedAnalogueScenarioSnapshot {
    pub comparison_set_id: OpaqueId,
    pub comparison_set_version: VersionId,
    pub branches: Vec<ConditionalResponseBranch>,
    pub indicators: Vec<MonitoringIndicator>,
    pub transferability: TransferabilityAssessment,
    pub justification: PredictionJustification,
}

/// Specification for issuing or revising an analogue-backed forecast in [`ForecastLedger`] (`FORECAST-04`, `A15`).
#[derive(Debug, Clone, PartialEq)]
pub struct AnalogueForecastIssuanceSpec {
    pub forecast_id: OpaqueId,
    pub version_id: VersionId,
    pub schema_version: SchemaVersion,
    pub actor: ActorId,
    pub issued_at_utc_ms: i64,
    pub origin_utc_ms: i64,
    pub horizon_start_utc_ms: i64,
    pub horizon_end_utc_ms: i64,
    pub target_id: OpaqueId,
    pub outcome_definition: OutcomeDefinition,
    pub baseline_version: VersionId,
    pub baseline_probability: Probability,
    pub forecast_probability: Probability,
    pub model_version: VersionId,
    pub model_knowledge_cutoff_utc_ms: Option<i64>,
    pub branches: Vec<ConditionalResponseBranch>,
    pub indicators: Vec<MonitoringIndicator>,
    pub justification: PredictionJustification,
    /// Additional current-scenario sources (e.g., initiating event report or new indicator evidence).
    pub scenario_sources: Vec<CutoffSourceInput>,
    pub assumptions: Vec<String>,
    pub predecessor_version: Option<VersionId>,
    pub revision_rationale: Option<RevisionRationale>,
}

/// Timing comparison between an observed outcome resolution and the precedent elapsed-time band (`FORECAST-04`, `A15`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalogueTimingEvaluation {
    /// Focal response occurred; compares `observed_elapsed_ms` against the precedent `ElapsedTimeRangeMs`.
    ResolvedOccurrence {
        observed_event_utc_ms: i64,
        observed_elapsed_ms: i64,
        precedent_range: ElapsedTimeRangeMs,
        signed_error_from_median_ms: i64,
        abs_error_from_median_ms: i64,
        within_precedent_min_max_band: bool,
        /// `0` when inside `[min_elapsed_ms, max_elapsed_ms]`, negative if earlier than `min_elapsed_ms`,
        /// positive if later than `max_elapsed_ms`.
        band_excursion_ms: i64,
    },
    /// Focal response occurred, but the comparison set had zero resolved supportive precedents.
    ResolvedOccurrenceWithoutPrecedentRange {
        observed_event_utc_ms: i64,
        observed_elapsed_ms: i64,
    },
    /// Focal response did not occur within the target horizon (`occurred == false`).
    ResolvedNonOccurrence {
        out_of_window_event_utc_ms: Option<i64>,
    },
    /// Outcome remains unresolved (`Pending`, `Censored`, or `Disputed`).
    Unresolved(UnresolvedReason),
    /// Outcome preconditions were invalidated.
    Invalidated { reason: String },
}

/// Typed errors for precedent registration, analogue retrieval, comparison-set edits,
/// conditional branch validation, and analogue-backed forecast issuance.
#[derive(Debug, Clone, PartialEq)]
pub enum AnalogueError {
    /// Library, step, branch, indicator, or edit budget was zero or exceeded bounds.
    BudgetExceeded {
        field: &'static str,
        actual: usize,
        limit: usize,
    },
    /// Required text field was empty/whitespace-only or exceeded [`MAX_TEXT_FIELD_BYTES`].
    InvalidTextField {
        field: &'static str,
        reason: &'static str,
    },
    /// UTC millisecond timestamp fell outside [`MIN_VALID_UTC_MS`]..=[`MAX_VALID_UTC_MS`].
    TimestampOutOfRange { field: &'static str, value: i64 },
    /// Duplicate episode or version identifier.
    DuplicateIdentifier {
        kind: &'static str,
        identifier: String,
    },
    /// Precedent episode referenced an unknown or duplicate source version.
    InvalidEpisodeSources(&'static str),
    /// Two records cited the same `source_version` with conflicting metadata timestamps.
    ConflictingSourceMetadata { source_version: VersionId },
    /// Response step or outcome timestamp/elapsed duration was inconsistent with `initiating_event_utc_ms`.
    InvalidEpisodeChronology {
        episode_id: OpaqueId,
        reason: &'static str,
    },
    /// `PolicySpilloverOrRetaliation` referenced a `predecessor_episode_id` not registered in the library
    /// or equal to `episode_id`.
    InvalidEpisodeDependency {
        episode_id: OpaqueId,
        reason: &'static str,
    },
    /// Episode supersession / correction violated linkage or chronology invariants (`B3`).
    InvalidEpisodeSupersession {
        episode_id: OpaqueId,
        reason: &'static str,
    },
    /// Retrieval query specified `min_matched_dimensions` outside `1..=7` or invalid timestamps.
    InvalidQueryCriteria(&'static str),
    /// Comparison-set edit referenced an unknown episode or an episode already in the target state.
    InvalidComparisonEdit {
        episode_id: OpaqueId,
        reason: &'static str,
    },
    /// Manual inclusion attempted to add an episode that was inadmissible at `information_cutoff_utc_ms`.
    EpisodeInadmissibleAtCutoff {
        episode_id: OpaqueId,
        reason: String,
    },
    /// Conditional response branch or monitoring indicator violated structural or probability invariants.
    InvalidScenarioModel(&'static str),
    /// Branch `chained_probability` deviated from the product of step `conditional_probability` values.
    InconsistentChainedProbability {
        branch_id: OpaqueId,
        supplied: f64,
        expected_product: f64,
    },
    /// Quantitative forecast issuance was requested for a [`PredictionJustification::QualitativeScenarioOnly`]
    /// scenario, or on sparse/poorly transferable history without an explicit `sparse_history_override_rationale`.
    UnjustifiedQuantitativeForecast { reason: &'static str },
    /// Underlying [`ForecastLedger`] issuance or scoring error.
    Issuance(IssuanceError),
}

impl fmt::Display for AnalogueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BudgetExceeded {
                field,
                actual,
                limit,
            } => write!(f, "{field} count {actual} is outside 1..={limit}"),
            Self::InvalidTextField { field, reason } => {
                write!(f, "invalid text field {field}: {reason}")
            }
            Self::TimestampOutOfRange { field, value } => write!(
                f,
                "timestamp {field}={value} is outside valid range {MIN_VALID_UTC_MS}..={MAX_VALID_UTC_MS}"
            ),
            Self::DuplicateIdentifier { kind, identifier } => {
                write!(f, "duplicate {kind} identifier {identifier}")
            }
            Self::InvalidEpisodeSources(msg) => write!(f, "invalid episode sources: {msg}"),
            Self::ConflictingSourceMetadata { source_version } => write!(
                f,
                "conflicting metadata timestamps for source_version {source_version}"
            ),
            Self::InvalidEpisodeChronology { episode_id, reason } => {
                write!(f, "episode {episode_id} chronology error: {reason}")
            }
            Self::InvalidEpisodeDependency { episode_id, reason } => {
                write!(f, "episode {episode_id} dependency error: {reason}")
            }
            Self::InvalidEpisodeSupersession { episode_id, reason } => {
                write!(f, "episode {episode_id} supersession error: {reason}")
            }
            Self::InvalidQueryCriteria(msg) => write!(f, "invalid analogue query criteria: {msg}"),
            Self::InvalidComparisonEdit { episode_id, reason } => {
                write!(f, "invalid comparison-set edit for {episode_id}: {reason}")
            }
            Self::EpisodeInadmissibleAtCutoff { episode_id, reason } => {
                write!(
                    f,
                    "episode {episode_id} is inadmissible at cutoff: {reason}"
                )
            }
            Self::InvalidScenarioModel(msg) => write!(f, "invalid scenario model: {msg}"),
            Self::InconsistentChainedProbability {
                branch_id,
                supplied,
                expected_product,
            } => write!(
                f,
                "branch {branch_id} chained_probability {supplied} differs from step product {expected_product}"
            ),
            Self::UnjustifiedQuantitativeForecast { reason } => {
                write!(f, "unjustified quantitative forecast: {reason}")
            }
            Self::Issuance(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for AnalogueError {}

impl From<IssuanceError> for AnalogueError {
    fn from(value: IssuanceError) -> Self {
        Self::Issuance(value)
    }
}

impl From<ScoringError> for AnalogueError {
    fn from(value: ScoringError) -> Self {
        Self::Issuance(IssuanceError::Scoring(value))
    }
}

/// Bounded in-memory repository of sourced historical precedent episodes (`FORECAST-01`, `FORECAST-02`, `FORECAST-04`).
///
/// Preserves every registered episode version in `episodes_by_version` and `episode_versions_by_id`
/// so later corrections (`record_episode_correction`) never overwrite historical versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrecedentLibrary {
    max_episodes: usize,
    used_versions: BTreeSet<VersionId>,
    sources_by_version: BTreeMap<VersionId, CutoffSourceInput>,
    episodes_by_version: BTreeMap<VersionId, PrecedentEpisodeRecord>,
    episode_versions_by_id: BTreeMap<OpaqueId, Vec<VersionId>>,
    episode_order: Vec<OpaqueId>,
}

impl PrecedentLibrary {
    /// Create a bounded precedent library accepting up to `max_episodes` versioned records (`1..=MAX_PRECEDENT_EPISODES`).
    pub fn new(max_episodes: usize) -> Result<Self, AnalogueError> {
        if !(1..=MAX_PRECEDENT_EPISODES).contains(&max_episodes) {
            return Err(AnalogueError::BudgetExceeded {
                field: "max_episodes",
                actual: max_episodes,
                limit: MAX_PRECEDENT_EPISODES,
            });
        }
        Ok(Self {
            max_episodes,
            used_versions: BTreeSet::new(),
            sources_by_version: BTreeMap::new(),
            episodes_by_version: BTreeMap::new(),
            episode_versions_by_id: BTreeMap::new(),
            episode_order: Vec::new(),
        })
    }

    /// Register an initial historical precedent episode after validating its sources,
    /// chronology, causal-link evidence, and dependency references (`FORECAST-01`, `FORECAST-02`).
    ///
    /// Initial registrations must have `supersedes_version == None` and `correction_rationale == None`.
    /// To supersede an existing episode with a corrected version, call [`Self::record_episode_correction`].
    pub fn register_episode(
        &mut self,
        episode: PrecedentEpisodeRecord,
    ) -> Result<&PrecedentEpisodeRecord, AnalogueError> {
        if self.episodes_by_version.len() >= self.max_episodes {
            return Err(AnalogueError::BudgetExceeded {
                field: "precedent_episodes",
                actual: self.episodes_by_version.len() + 1,
                limit: self.max_episodes,
            });
        }
        if self
            .episode_versions_by_id
            .contains_key(&episode.episode_id)
        {
            return Err(AnalogueError::DuplicateIdentifier {
                kind: "episode_id",
                identifier: episode.episode_id.as_str().to_owned(),
            });
        }
        if self.used_versions.contains(&episode.version_id) {
            return Err(AnalogueError::DuplicateIdentifier {
                kind: "version_id",
                identifier: episode.version_id.as_str().to_owned(),
            });
        }
        if episode.supersedes_version.is_some() || episode.correction_rationale.is_some() {
            return Err(AnalogueError::InvalidEpisodeSupersession {
                episode_id: episode.episode_id,
                reason: "initial episode registration must have supersedes_version=None and correction_rationale=None",
            });
        }

        validate_episode_record(
            &episode,
            &self.episode_versions_by_id,
            &self.sources_by_version,
        )?;

        for src in &episode.sources {
            self.sources_by_version
                .insert(src.source_version.clone(), src.clone());
        }
        let id = episode.episode_id.clone();
        let ver = episode.version_id.clone();
        self.used_versions.insert(ver.clone());
        self.episode_order.push(id.clone());
        self.episode_versions_by_id
            .entry(id)
            .or_default()
            .push(ver.clone());
        self.episodes_by_version.insert(ver.clone(), episode);
        self.episodes_by_version
            .get(&ver)
            .ok_or(AnalogueError::InvalidEpisodeSources(
                "internal episode lookup failed",
            ))
    }

    /// Record a superseding correction to an existing historical precedent episode while
    /// preserving all prior versions of the episode in the library (`FORECAST-02`, `FORECAST-04`, `B3`, `AGENTS.md` §4).
    ///
    /// - Validates that `corrected.supersedes_version` matches the latest registered version of
    ///   `corrected.episode_id` and that `corrected.correction_rationale` is non-empty.
    /// - Requires `corrected.recorded_at_utc_ms >= prior.recorded_at_utc_ms` and
    ///   `corrected.initiating_event_utc_ms == prior.initiating_event_utc_ms`.
    /// - Notifies `ledger` via [`ForecastLedger::record_precedent_supersession`] so any
    ///   [`crate::issuance::EvaluationRunRecord`] whose evaluated vintages pinned an earlier
    ///   version of `corrected.episode_id` in `comparison_set` is flagged with
    ///   `needs_reassessment = true` and [`crate::issuance::ReassessmentReason::PrecedentEpisodeSuperseded`]
    ///   without mutating any issued forecast vintage or historical score.
    pub fn record_episode_correction(
        &mut self,
        corrected: PrecedentEpisodeRecord,
        ledger: &mut ForecastLedger,
    ) -> Result<(&PrecedentEpisodeRecord, Vec<VersionId>), AnalogueError> {
        if self.episodes_by_version.len() >= self.max_episodes {
            return Err(AnalogueError::BudgetExceeded {
                field: "precedent_episodes",
                actual: self.episodes_by_version.len() + 1,
                limit: self.max_episodes,
            });
        }
        if self.used_versions.contains(&corrected.version_id) {
            return Err(AnalogueError::DuplicateIdentifier {
                kind: "version_id",
                identifier: corrected.version_id.as_str().to_owned(),
            });
        }

        let (Some(prior_ver), Some(rationale)) = (
            &corrected.supersedes_version,
            &corrected.correction_rationale,
        ) else {
            return Err(AnalogueError::InvalidEpisodeSupersession {
                episode_id: corrected.episode_id,
                reason: "episode correction must supply both supersedes_version and correction_rationale",
            });
        };
        validate_text("episode.correction_rationale", rationale)?;

        let history = self
            .episode_versions_by_id
            .get(&corrected.episode_id)
            .ok_or_else(|| AnalogueError::InvalidEpisodeSupersession {
                episode_id: corrected.episode_id.clone(),
                reason: "episode_id does not exist in PrecedentLibrary",
            })?;
        if history.last() != Some(prior_ver) {
            return Err(AnalogueError::InvalidEpisodeSupersession {
                episode_id: corrected.episode_id,
                reason: "supersedes_version must match the latest version of episode_id",
            });
        }
        let prior = self.episodes_by_version.get(prior_ver).ok_or_else(|| {
            AnalogueError::InvalidEpisodeSupersession {
                episode_id: corrected.episode_id.clone(),
                reason: "superseded episode version not found",
            }
        })?;
        if corrected.recorded_at_utc_ms < prior.recorded_at_utc_ms {
            return Err(AnalogueError::InvalidEpisodeSupersession {
                episode_id: corrected.episode_id,
                reason: "corrected episode recorded_at_utc_ms cannot precede superseded version",
            });
        }
        if corrected.initiating_event_utc_ms != prior.initiating_event_utc_ms {
            return Err(AnalogueError::InvalidEpisodeSupersession {
                episode_id: corrected.episode_id,
                reason: "corrected episode must preserve initiating_event_utc_ms",
            });
        }

        if corrected.title == prior.title
            && corrected.initiating_condition == prior.initiating_condition
            && corrected.initiation_evidence_versions == prior.initiation_evidence_versions
            && corrected.profile == prior.profile
            && corrected.dependency == prior.dependency
            && corrected.response_steps == prior.response_steps
            && corrected.outcome == prior.outcome
            && corrected.sources == prior.sources
        {
            return Err(AnalogueError::InvalidEpisodeSupersession {
                episode_id: corrected.episode_id,
                reason: "corrected episode must differ in at least one substantive field from the superseded version",
            });
        }

        validate_episode_record(
            &corrected,
            &self.episode_versions_by_id,
            &self.sources_by_version,
        )?;

        let prior_versions: Vec<VersionId> = history.clone();
        let affected_vintages = ledger.record_precedent_supersession(
            &corrected.episode_id,
            &prior_versions,
            &corrected.version_id,
        );

        for src in &corrected.sources {
            self.sources_by_version
                .insert(src.source_version.clone(), src.clone());
        }
        let id = corrected.episode_id.clone();
        let ver = corrected.version_id.clone();
        self.used_versions.insert(ver.clone());
        self.episode_versions_by_id
            .entry(id)
            .or_default()
            .push(ver.clone());
        self.episodes_by_version.insert(ver.clone(), corrected);
        let stored =
            self.episodes_by_version
                .get(&ver)
                .ok_or(AnalogueError::InvalidEpisodeSources(
                    "internal episode lookup failed",
                ))?;
        Ok((stored, affected_vintages))
    }

    /// Retrieve the latest version of an episode by `episode_id`.
    #[must_use]
    pub fn get(&self, episode_id: &OpaqueId) -> Option<&PrecedentEpisodeRecord> {
        let ver = self.episode_versions_by_id.get(episode_id)?.last()?;
        self.episodes_by_version.get(ver)
    }

    /// Retrieve a specific immutable episode version by its [`VersionId`].
    #[must_use]
    pub fn episode_version(&self, version_id: &VersionId) -> Option<&PrecedentEpisodeRecord> {
        self.episodes_by_version.get(version_id)
    }

    /// Return all registered versions of `episode_id` in chronological registration order.
    #[must_use]
    pub fn episode_history(&self, episode_id: &OpaqueId) -> Vec<&PrecedentEpisodeRecord> {
        self.episode_versions_by_id
            .get(episode_id)
            .map(|vers| {
                vers.iter()
                    .filter_map(|v| self.episodes_by_version.get(v))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Retrieve the latest version of `episode_id` that was admissible at `cutoff_utc_ms`
    /// under `cutoff_mode`. Returns `None` under `ProspectiveStrict` if no version had
    /// `recorded_at_utc_ms <= cutoff_utc_ms` so callers never receive a hindsight-curated version.
    #[must_use]
    pub fn get_as_of_cutoff(
        &self,
        episode_id: &OpaqueId,
        cutoff_utc_ms: i64,
        cutoff_mode: &CutoffAdmissibilityMode,
    ) -> Option<&PrecedentEpisodeRecord> {
        let vers = self.episode_versions_by_id.get(episode_id)?;
        match cutoff_mode {
            CutoffAdmissibilityMode::ProspectiveStrict => vers
                .iter()
                .rev()
                .filter_map(|v| self.episodes_by_version.get(v))
                .find(|ep| ep.recorded_at_utc_ms <= cutoff_utc_ms),
            CutoffAdmissibilityMode::RetrospectiveReconstructed { .. } => self.get(episode_id),
        }
    }

    /// Number of distinct logical precedent episodes (`episode_id`s) in the library.
    #[must_use]
    pub fn len(&self) -> usize {
        self.episode_versions_by_id.len()
    }

    /// True if no precedent episodes are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.episode_versions_by_id.is_empty()
    }

    /// Iterate over the latest version of each registered episode in initial registration order.
    pub fn episodes(&self) -> impl Iterator<Item = &PrecedentEpisodeRecord> {
        self.episode_order.iter().filter_map(|id| self.get(id))
    }

    /// Retrieve an initial [`AnalogueComparisonSet`] matching `criteria` (`FORECAST-02`, `A15`).
    ///
    /// Episodes whose initiating event, record curation (under `ProspectiveStrict`), or initiation
    /// evidence was not admissible at `criteria.information_cutoff_utc_ms` are excluded and recorded
    /// in `inadmissible_episode_ids`. Episodes that initiated before cutoff but resolved after
    /// cutoff are retained as [`PrecedentRole::CensoredAtCutoff`] so post-cutoff outcomes
    /// cannot leak into point-in-time historical base rates (`FORECAST-01`, `FORECAST-04`).
    pub fn retrieve_comparison_set(
        &self,
        criteria: AnalogueQueryCriteria,
    ) -> Result<AnalogueComparisonSet, AnalogueError> {
        validate_ts("created_at_utc_ms", criteria.created_at_utc_ms)?;
        validate_ts(
            "information_cutoff_utc_ms",
            criteria.information_cutoff_utc_ms,
        )?;
        if criteria.created_at_utc_ms < criteria.information_cutoff_utc_ms {
            return Err(AnalogueError::InvalidQueryCriteria(
                "created_at_utc_ms must be >= information_cutoff_utc_ms",
            ));
        }
        if let CutoffAdmissibilityMode::RetrospectiveReconstructed { limitation_note } =
            &criteria.cutoff_mode
        {
            validate_text("cutoff_mode.limitation_note", limitation_note)?;
        }
        validate_profile("target_profile", &criteria.target_profile)?;
        if !(1..=7).contains(&criteria.min_matched_dimensions) {
            return Err(AnalogueError::InvalidQueryCriteria(
                "min_matched_dimensions must be in 1..=7",
            ));
        }
        if criteria.min_independent_clusters == 0
            || criteria.min_independent_clusters > MAX_PRECEDENT_EPISODES
        {
            return Err(AnalogueError::InvalidQueryCriteria(
                "min_independent_clusters must be in 1..=MAX_PRECEDENT_EPISODES",
            ));
        }

        let mut active_entries = Vec::new();
        let mut inadmissible_episode_ids = Vec::new();

        for episode_id in &self.episode_order {
            let Some(episode) = self.get_as_of_cutoff(
                episode_id,
                criteria.information_cutoff_utc_ms,
                &criteria.cutoff_mode,
            ) else {
                if let Some(latest_ep) = self.get(episode_id) {
                    inadmissible_episode_ids.push((
                        episode_id.clone(),
                        format!(
                            "episode recorded_at_utc_ms ({}) is after strict information_cutoff_utc_ms ({})",
                            latest_ep.recorded_at_utc_ms, criteria.information_cutoff_utc_ms
                        ),
                    ));
                }
                continue;
            };
            let evaluated = evaluate_episode_against_target(
                episode,
                &criteria.target_profile,
                criteria.information_cutoff_utc_ms,
                &criteria.cutoff_mode,
            )?;
            match &evaluated.point_in_time_status {
                EpisodePointInTimeStatus::InadmissibleAtCutoff { reason } => {
                    inadmissible_episode_ids.push((episode.episode_id.clone(), reason.clone()));
                }
                EpisodePointInTimeStatus::OutcomeKnowableAtCutoff { .. }
                | EpisodePointInTimeStatus::InitiatedButOutcomeCensoredAtCutoff { .. } => {
                    if evaluated.matched_dimension_count >= criteria.min_matched_dimensions {
                        active_entries.push(evaluated);
                    }
                }
            }
        }

        recompute_effective_cluster_keys(&mut active_entries);

        Ok(AnalogueComparisonSet {
            comparison_set_id: criteria.comparison_set_id,
            initial_version_id: criteria.initial_version_id.clone(),
            current_version_id: criteria.initial_version_id,
            created_at_utc_ms: criteria.created_at_utc_ms,
            information_cutoff_utc_ms: criteria.information_cutoff_utc_ms,
            cutoff_mode: criteria.cutoff_mode,
            target_profile: criteria.target_profile,
            min_matched_dimensions: criteria.min_matched_dimensions,
            min_independent_clusters: criteria.min_independent_clusters,
            active_entries,
            excluded_entries: Vec::new(),
            edit_history: Vec::new(),
            inadmissible_episode_ids,
        })
    }
}

impl AnalogueComparisonSet {
    /// Logical identifier of this comparison set.
    #[must_use]
    pub fn comparison_set_id(&self) -> &OpaqueId {
        &self.comparison_set_id
    }

    /// Initial immutable version identifier assigned when this comparison set was retrieved (`P2`).
    #[must_use]
    pub fn initial_version_id(&self) -> &VersionId {
        &self.initial_version_id
    }

    /// Current immutable version identifier of this comparison set (updated on every edit or cutoff advance).
    #[must_use]
    pub fn current_version_id(&self) -> &VersionId {
        &self.current_version_id
    }

    /// Strict information cutoff timestamp in UTC milliseconds for this comparison set.
    #[must_use]
    pub fn information_cutoff_utc_ms(&self) -> i64 {
        self.information_cutoff_utc_ms
    }

    /// Active cutoff admissibility mode (`ProspectiveStrict` vs `RetrospectiveReconstructed`).
    #[must_use]
    pub fn cutoff_mode(&self) -> &CutoffAdmissibilityMode {
        &self.cutoff_mode
    }

    /// Target scenario profile used for 7-dimension comparison.
    #[must_use]
    pub fn target_profile(&self) -> &AnalogueProfile {
        &self.target_profile
    }

    /// Minimum matched dimensions (`1..=7`) configured for automatic retrieval.
    #[must_use]
    pub fn min_matched_dimensions(&self) -> usize {
        self.min_matched_dimensions
    }

    /// Minimum independent resolved precedent clusters required before treating history as non-sparse.
    #[must_use]
    pub fn min_independent_clusters(&self) -> usize {
        self.min_independent_clusters
    }

    /// Read-only slice of active precedent match entries in this comparison set.
    #[must_use]
    pub fn active_entries(&self) -> &[AnalogueMatchEntry] {
        &self.active_entries
    }

    /// Read-only slice of excluded precedent entries retained for auditability (`FORECAST-02`).
    #[must_use]
    pub fn excluded_entries(&self) -> &[ExcludedAnalogueEntry] {
        &self.excluded_entries
    }

    /// Read-only slice of all comparison-set edits and cutoff advances in chronological order.
    #[must_use]
    pub fn edit_history(&self) -> &[ComparisonSetEdit] {
        &self.edit_history
    }

    /// Episodes rejected during retrieval because their initiating event, record curation, or
    /// initiation sources were inadmissible at `information_cutoff_utc_ms`.
    #[must_use]
    pub fn inadmissible_episode_ids(&self) -> &[(OpaqueId, String)] {
        &self.inadmissible_episode_ids
    }

    /// Return all immutable episode [`VersionId`]s frozen in this comparison set's `active_entries`,
    /// prefixed by `current_version_id` so the exact comparison-set snapshot version is pinned
    /// as a structured [`VersionId`] on every issued [`ForecastVintageRecord`] (`FORECAST-02`, `FORECAST-04`).
    #[must_use]
    pub fn pinned_comparison_versions(&self) -> Vec<VersionId> {
        let mut versions = Vec::with_capacity(1 + self.active_entries.len());
        versions.push(self.current_version_id.clone());
        for entry in &self.active_entries {
            if !versions.contains(&entry.episode_version) {
                versions.push(entry.episode_version.clone());
            }
        }
        versions
    }

    fn has_used_version(&self, candidate: &VersionId) -> bool {
        candidate == &self.initial_version_id
            || candidate == &self.current_version_id
            || self
                .edit_history
                .iter()
                .any(|e| &e.edit_version == candidate)
    }

    fn latest_edit_or_creation_utc_ms(&self) -> i64 {
        self.edit_history
            .last()
            .map_or(self.created_at_utc_ms, |e| e.edited_at_utc_ms)
    }

    /// Re-retrieve and re-evaluate this comparison set against `library` at a non-decreasing
    /// `new_cutoff_utc_ms`, producing a new [`AnalogueComparisonSet`] with `new_version_id` and
    /// appending a [`ComparisonEditAction::AdvanceCutoff`] entry to `edit_history` (`FORECAST-02`, `FORECAST-04`, `B1`, `P2`).
    ///
    /// - Re-computes point-in-time knowability for all episodes at `new_cutoff_utc_ms` (so an
    ///   episode that was `CensoredAtCutoff` at the earlier cutoff and resolved before
    ///   `new_cutoff_utc_ms` transitions to its resolved role at the new cutoff).
    /// - Preserves manual exclusions (`excluded_entries`), manual inclusions, and transfer-limit
    ///   annotations from earlier edits.
    pub fn re_retrieve_at_cutoff(
        &self,
        library: &PrecedentLibrary,
        new_version_id: VersionId,
        actor: ActorId,
        edited_at_utc_ms: i64,
        new_cutoff_utc_ms: i64,
        rationale: String,
    ) -> Result<Self, AnalogueError> {
        if self.edit_history.len() >= MAX_COMPARISON_EDITS {
            return Err(AnalogueError::BudgetExceeded {
                field: "edit_history",
                actual: self.edit_history.len() + 1,
                limit: MAX_COMPARISON_EDITS,
            });
        }
        validate_ts("edited_at_utc_ms", edited_at_utc_ms)?;
        validate_ts("new_cutoff_utc_ms", new_cutoff_utc_ms)?;
        validate_text("AdvanceCutoff.rationale", &rationale)?;
        if new_cutoff_utc_ms < self.information_cutoff_utc_ms {
            return Err(AnalogueError::InvalidQueryCriteria(
                "new_cutoff_utc_ms cannot regress before current information_cutoff_utc_ms",
            ));
        }
        if edited_at_utc_ms < new_cutoff_utc_ms {
            return Err(AnalogueError::InvalidQueryCriteria(
                "edited_at_utc_ms must be >= new_cutoff_utc_ms",
            ));
        }
        if edited_at_utc_ms < self.latest_edit_or_creation_utc_ms() {
            return Err(AnalogueError::InvalidQueryCriteria(
                "edited_at_utc_ms cannot regress before created_at_utc_ms or the previous edit timestamp",
            ));
        }
        if self.has_used_version(&new_version_id) {
            return Err(AnalogueError::DuplicateIdentifier {
                kind: "edit_version",
                identifier: new_version_id.as_str().to_owned(),
            });
        }

        let excluded_ids: BTreeSet<&OpaqueId> = self
            .excluded_entries
            .iter()
            .map(|ex| &ex.entry.episode_id)
            .collect();
        let previously_active_ids: BTreeSet<&OpaqueId> =
            self.active_entries.iter().map(|e| &e.episode_id).collect();
        let prior_annotations: BTreeMap<&OpaqueId, (&Option<String>, &Vec<String>)> = self
            .active_entries
            .iter()
            .map(|e| {
                (
                    &e.episode_id,
                    (&e.institutional_difference_note, &e.transfer_limitations),
                )
            })
            .collect();

        let mut active_entries = Vec::new();
        let mut inadmissible_episode_ids = Vec::new();

        for episode_id in &library.episode_order {
            let Some(episode) =
                library.get_as_of_cutoff(episode_id, new_cutoff_utc_ms, &self.cutoff_mode)
            else {
                if let Some(latest_ep) = library.get(episode_id) {
                    inadmissible_episode_ids.push((
                        episode_id.clone(),
                        format!(
                            "episode recorded_at_utc_ms ({}) is after strict information_cutoff_utc_ms ({new_cutoff_utc_ms})",
                            latest_ep.recorded_at_utc_ms
                        ),
                    ));
                }
                continue;
            };
            let mut evaluated = evaluate_episode_against_target(
                episode,
                &self.target_profile,
                new_cutoff_utc_ms,
                &self.cutoff_mode,
            )?;
            match &evaluated.point_in_time_status {
                EpisodePointInTimeStatus::InadmissibleAtCutoff { reason } => {
                    inadmissible_episode_ids.push((episode.episode_id.clone(), reason.clone()));
                }
                EpisodePointInTimeStatus::OutcomeKnowableAtCutoff { .. }
                | EpisodePointInTimeStatus::InitiatedButOutcomeCensoredAtCutoff { .. } => {
                    if excluded_ids.contains(&episode.episode_id) {
                        continue;
                    }
                    if evaluated.matched_dimension_count >= self.min_matched_dimensions
                        || previously_active_ids.contains(&episode.episode_id)
                    {
                        if let Some((prior_inst_note, prior_limits)) =
                            prior_annotations.get(&episode.episode_id)
                        {
                            if let Some(note) = prior_inst_note {
                                evaluated.institutional_difference_note = Some(note.clone());
                            }
                            for lim in *prior_limits {
                                if !evaluated.transfer_limitations.contains(lim) {
                                    evaluated.transfer_limitations.push(lim.clone());
                                }
                            }
                        }
                        active_entries.push(evaluated);
                    }
                }
            }
        }

        recompute_effective_cluster_keys(&mut active_entries);

        let mut excluded_entries = Vec::with_capacity(self.excluded_entries.len());
        for ex in &self.excluded_entries {
            if let Some(ep) =
                library.get_as_of_cutoff(&ex.entry.episode_id, new_cutoff_utc_ms, &self.cutoff_mode)
            {
                let re_eval = evaluate_episode_against_target(
                    ep,
                    &self.target_profile,
                    new_cutoff_utc_ms,
                    &self.cutoff_mode,
                )?;
                excluded_entries.push(ExcludedAnalogueEntry {
                    entry: re_eval,
                    excluded_by_version: ex.excluded_by_version.clone(),
                    exclusion_rationale: ex.exclusion_rationale.clone(),
                });
            } else {
                excluded_entries.push(ex.clone());
            }
        }

        let mut edit_history = self.edit_history.clone();
        edit_history.push(ComparisonSetEdit {
            edit_version: new_version_id.clone(),
            actor,
            edited_at_utc_ms,
            action: ComparisonEditAction::AdvanceCutoff {
                previous_cutoff_utc_ms: self.information_cutoff_utc_ms,
                new_cutoff_utc_ms,
                rationale,
            },
        });

        Ok(Self {
            comparison_set_id: self.comparison_set_id.clone(),
            initial_version_id: self.initial_version_id.clone(),
            current_version_id: new_version_id,
            created_at_utc_ms: self.created_at_utc_ms,
            information_cutoff_utc_ms: new_cutoff_utc_ms,
            cutoff_mode: self.cutoff_mode.clone(),
            target_profile: self.target_profile.clone(),
            min_matched_dimensions: self.min_matched_dimensions,
            min_independent_clusters: self.min_independent_clusters,
            active_entries,
            excluded_entries,
            edit_history,
            inadmissible_episode_ids,
        })
    }

    /// Apply an auditable analyst edit (`IncludeEpisode`, `ExcludeEpisode`, `AnnotateTransferLimits`,
    /// or `AdvanceCutoff`) to this comparison set, advancing `current_version_id` and preserving
    /// `edit_history` (`FORECAST-02`, `A15`).
    pub fn apply_edit(
        &mut self,
        library: &PrecedentLibrary,
        edit: ComparisonSetEdit,
    ) -> Result<(), AnalogueError> {
        if let ComparisonEditAction::AdvanceCutoff {
            previous_cutoff_utc_ms,
            new_cutoff_utc_ms,
            rationale,
        } = &edit.action
        {
            if *previous_cutoff_utc_ms != self.information_cutoff_utc_ms {
                return Err(AnalogueError::InvalidQueryCriteria(
                    "AdvanceCutoff.previous_cutoff_utc_ms does not match current information_cutoff_utc_ms",
                ));
            }
            let updated = self.re_retrieve_at_cutoff(
                library,
                edit.edit_version,
                edit.actor,
                edit.edited_at_utc_ms,
                *new_cutoff_utc_ms,
                rationale.clone(),
            )?;
            *self = updated;
            return Ok(());
        }

        if self.edit_history.len() >= MAX_COMPARISON_EDITS {
            return Err(AnalogueError::BudgetExceeded {
                field: "edit_history",
                actual: self.edit_history.len() + 1,
                limit: MAX_COMPARISON_EDITS,
            });
        }
        validate_ts("edited_at_utc_ms", edit.edited_at_utc_ms)?;
        if edit.edited_at_utc_ms < self.information_cutoff_utc_ms {
            return Err(AnalogueError::InvalidQueryCriteria(
                "edited_at_utc_ms must be >= information_cutoff_utc_ms",
            ));
        }
        if edit.edited_at_utc_ms < self.latest_edit_or_creation_utc_ms() {
            return Err(AnalogueError::InvalidQueryCriteria(
                "edited_at_utc_ms cannot regress before created_at_utc_ms or the previous edit timestamp",
            ));
        }
        if self.has_used_version(&edit.edit_version) {
            return Err(AnalogueError::DuplicateIdentifier {
                kind: "edit_version",
                identifier: edit.edit_version.as_str().to_owned(),
            });
        }

        match &edit.action {
            ComparisonEditAction::IncludeEpisode {
                episode_id,
                rationale,
                transfer_limitations,
            } => {
                validate_text("IncludeEpisode.rationale", rationale)?;
                if transfer_limitations.len() > MAX_FORECAST_COLLECTION_LEN {
                    return Err(AnalogueError::BudgetExceeded {
                        field: "transfer_limitations",
                        actual: transfer_limitations.len(),
                        limit: MAX_FORECAST_COLLECTION_LEN,
                    });
                }
                for note in transfer_limitations {
                    validate_text("transfer_limitations[]", note)?;
                }
                if self
                    .active_entries
                    .iter()
                    .any(|e| &e.episode_id == episode_id)
                {
                    return Err(AnalogueError::InvalidComparisonEdit {
                        episode_id: episode_id.clone(),
                        reason: "episode is already in active_entries",
                    });
                }
                let Some(episode) = library.get_as_of_cutoff(
                    episode_id,
                    self.information_cutoff_utc_ms,
                    &self.cutoff_mode,
                ) else {
                    if let Some(latest_ep) = library.get(episode_id) {
                        return Err(AnalogueError::EpisodeInadmissibleAtCutoff {
                            episode_id: episode_id.clone(),
                            reason: format!(
                                "episode recorded_at_utc_ms ({}) is after strict information_cutoff_utc_ms ({})",
                                latest_ep.recorded_at_utc_ms, self.information_cutoff_utc_ms
                            ),
                        });
                    }
                    return Err(AnalogueError::InvalidComparisonEdit {
                        episode_id: episode_id.clone(),
                        reason: "episode not found in PrecedentLibrary",
                    });
                };
                let mut evaluated = evaluate_episode_against_target(
                    episode,
                    &self.target_profile,
                    self.information_cutoff_utc_ms,
                    &self.cutoff_mode,
                )?;
                if let EpisodePointInTimeStatus::InadmissibleAtCutoff { reason } =
                    &evaluated.point_in_time_status
                {
                    return Err(AnalogueError::EpisodeInadmissibleAtCutoff {
                        episode_id: episode_id.clone(),
                        reason: reason.clone(),
                    });
                }
                for note in transfer_limitations {
                    if !evaluated.transfer_limitations.contains(note) {
                        evaluated.transfer_limitations.push(note.clone());
                    }
                }
                self.excluded_entries
                    .retain(|ex| &ex.entry.episode_id != episode_id);
                self.active_entries.push(evaluated);
                recompute_effective_cluster_keys(&mut self.active_entries);
            }
            ComparisonEditAction::ExcludeEpisode {
                episode_id,
                rationale,
            } => {
                validate_text("ExcludeEpisode.rationale", rationale)?;
                let pos = self
                    .active_entries
                    .iter()
                    .position(|e| &e.episode_id == episode_id)
                    .ok_or_else(|| AnalogueError::InvalidComparisonEdit {
                        episode_id: episode_id.clone(),
                        reason: "episode is not present in active_entries",
                    })?;
                let removed = self.active_entries.remove(pos);
                self.excluded_entries.push(ExcludedAnalogueEntry {
                    entry: removed,
                    excluded_by_version: edit.edit_version.clone(),
                    exclusion_rationale: rationale.clone(),
                });
                recompute_effective_cluster_keys(&mut self.active_entries);
            }
            ComparisonEditAction::AnnotateTransferLimits {
                episode_id,
                institutional_difference_note,
                additional_limitations,
                rationale,
            } => {
                validate_text("AnnotateTransferLimits.rationale", rationale)?;
                if let Some(inst_note) = institutional_difference_note {
                    validate_text("institutional_difference_note", inst_note)?;
                }
                if additional_limitations.len() > MAX_FORECAST_COLLECTION_LEN {
                    return Err(AnalogueError::BudgetExceeded {
                        field: "additional_limitations",
                        actual: additional_limitations.len(),
                        limit: MAX_FORECAST_COLLECTION_LEN,
                    });
                }
                for note in additional_limitations {
                    validate_text("additional_limitations[]", note)?;
                }
                let entry = self
                    .active_entries
                    .iter_mut()
                    .find(|e| &e.episode_id == episode_id)
                    .ok_or_else(|| AnalogueError::InvalidComparisonEdit {
                        episode_id: episode_id.clone(),
                        reason: "episode is not present in active_entries",
                    })?;
                if let Some(inst_note) = institutional_difference_note {
                    entry.institutional_difference_note = Some(inst_note.clone());
                }
                for note in additional_limitations {
                    if !entry.transfer_limitations.contains(note) {
                        entry.transfer_limitations.push(note.clone());
                    }
                }
            }
            ComparisonEditAction::AdvanceCutoff { .. } => {}
        }

        self.current_version_id = edit.edit_version.clone();
        self.edit_history.push(edit);
        Ok(())
    }

    /// Compute the descriptive [`HistoricalFrequencySummary`] over the current comparison set (`FORECAST-02`, `FORECAST-03`).
    ///
    /// - Separates raw episode counts from cluster-deduplicated independent precedent counts.
    /// - Keeps unanimously supportive clusters, unanimously counterexample clusters, and
    ///   mixed-outcome clusters distinct, weighting each resolved cluster by its within-cluster
    ///   support fraction so deduplication never inflates apparent support (`B2`).
    /// - Explicitly flags `is_calibrated_probability == false` with [`UNCALIBRATED_HISTORICAL_FREQUENCY_CAVEAT`].
    #[must_use]
    pub fn summarize_frequencies(&self) -> HistoricalFrequencySummary {
        let mut all_clusters = BTreeSet::new();
        let mut cluster_counts: BTreeMap<&str, (usize, usize)> = BTreeMap::new();

        let mut supportive_target_response_count = 0usize;
        let mut divergent_counterexample_count = 0usize;
        let mut documented_non_response_count = 0usize;
        let mut censored_at_cutoff_count = 0usize;
        let mut elapsed_samples = Vec::new();

        for entry in &self.active_entries {
            let key = entry.independence_cluster_key.as_str();
            all_clusters.insert(key);
            match entry.role {
                PrecedentRole::SupportiveAnalogue => {
                    supportive_target_response_count += 1;
                    let counts = cluster_counts.entry(key).or_insert((0, 0));
                    counts.0 += 1;
                    if let Some(el) = entry.knowable_target_elapsed_ms {
                        elapsed_samples.push(el);
                    }
                }
                PrecedentRole::DivergentCounterexample => {
                    divergent_counterexample_count += 1;
                    let counts = cluster_counts.entry(key).or_insert((0, 0));
                    counts.1 += 1;
                }
                PrecedentRole::DocumentedNonResponseCounterexample => {
                    documented_non_response_count += 1;
                    let counts = cluster_counts.entry(key).or_insert((0, 0));
                    counts.1 += 1;
                }
                PrecedentRole::CensoredAtCutoff => {
                    censored_at_cutoff_count += 1;
                }
            }
        }

        let active_episode_count = self.active_entries.len();
        let independent_cluster_count = all_clusters.len();
        let dependent_episode_count =
            active_episode_count.saturating_sub(independent_cluster_count);
        let total_counterexample_count =
            divergent_counterexample_count + documented_non_response_count;
        let resolved_knowable_episode_count =
            supportive_target_response_count + total_counterexample_count;

        let mut independent_supportive_cluster_count = 0usize;
        let mut independent_counterexample_cluster_count = 0usize;
        let mut mixed_outcome_cluster_count = 0usize;
        let mut weighted_cluster_bps_sum = 0u128;

        for (sup, counter) in cluster_counts.values().copied() {
            let total_in_cluster = sup + counter;
            if total_in_cluster == 0 {
                continue;
            }
            if sup > 0 && counter == 0 {
                independent_supportive_cluster_count += 1;
            } else if counter > 0 && sup == 0 {
                independent_counterexample_cluster_count += 1;
            } else {
                mixed_outcome_cluster_count += 1;
            }
            weighted_cluster_bps_sum += (sup as u128 * 10_000) / (total_in_cluster as u128);
        }

        let independent_resolved_cluster_count = cluster_counts.len();

        let raw_resolved_support_ratio_bps = (resolved_knowable_episode_count > 0).then(|| {
            ((supportive_target_response_count as u128 * 10_000)
                / (resolved_knowable_episode_count as u128)) as u32
        });

        let independent_cluster_support_ratio_bps =
            (independent_resolved_cluster_count > 0).then(|| {
                (weighted_cluster_bps_sum / (independent_resolved_cluster_count as u128)) as u32
            });

        elapsed_samples.sort_unstable();
        let target_elapsed_time_range = if elapsed_samples.is_empty() {
            None
        } else {
            let n = elapsed_samples.len();
            let min_elapsed_ms = elapsed_samples[0];
            let max_elapsed_ms = elapsed_samples[n - 1];
            let median_elapsed_ms = if n % 2 == 1 {
                elapsed_samples[n / 2]
            } else {
                let a = elapsed_samples[n / 2 - 1] as i128;
                let b = elapsed_samples[n / 2] as i128;
                ((a + b) / 2) as i64
            };
            Some(ElapsedTimeRangeMs {
                resolved_episode_count: n,
                min_elapsed_ms,
                median_elapsed_ms,
                max_elapsed_ms,
            })
        };

        let excluded_counterexample_count = self
            .excluded_entries
            .iter()
            .filter(|ex| {
                matches!(
                    ex.entry.role,
                    PrecedentRole::DivergentCounterexample
                        | PrecedentRole::DocumentedNonResponseCounterexample
                )
            })
            .count();

        HistoricalFrequencySummary {
            active_episode_count,
            independent_cluster_count,
            dependent_episode_count,
            resolved_knowable_episode_count,
            supportive_target_response_count,
            divergent_counterexample_count,
            documented_non_response_count,
            total_counterexample_count,
            censored_at_cutoff_count,
            excluded_episode_count: self.excluded_entries.len(),
            excluded_counterexample_count,
            independent_supportive_cluster_count,
            independent_counterexample_cluster_count,
            mixed_outcome_cluster_count,
            independent_resolved_cluster_count,
            raw_resolved_support_ratio_bps,
            independent_cluster_support_ratio_bps,
            target_elapsed_time_range,
            is_calibrated_probability: false,
            calibration_caveat: UNCALIBRATED_HISTORICAL_FREQUENCY_CAVEAT,
        }
    }

    /// Evaluate whether the current comparison set provides an adequate or sparse/poorly
    /// transferable precedent basis for the target scenario (`FORECAST-03`, `A15`).
    #[must_use]
    pub fn assess_transferability(&self) -> TransferabilityAssessment {
        let summary = self.summarize_frequencies();
        let mut reasons = Vec::new();

        if summary.independent_resolved_cluster_count < self.min_independent_clusters {
            reasons.push(TransferLimitationReason::SparseIndependentPrecedents {
                independent_resolved_clusters: summary.independent_resolved_cluster_count,
                minimum_recommended: self.min_independent_clusters,
            });
        }

        if summary.active_episode_count > 0
            && summary.censored_at_cutoff_count * 2 >= summary.active_episode_count
        {
            reasons.push(TransferLimitationReason::HighCensoredFollowUpShare {
                censored_count: summary.censored_at_cutoff_count,
                active_count: summary.active_episode_count,
            });
        }

        let mismatched_regime_count = self
            .active_entries
            .iter()
            .filter(|e| {
                e.dimension_assessments.iter().any(|d| {
                    d.dimension == AnalogueDimension::InstitutionalRegime && !d.matches_target
                })
            })
            .count();
        if summary.active_episode_count > 0
            && mismatched_regime_count * 2 > summary.active_episode_count
        {
            reasons.push(TransferLimitationReason::InstitutionalRegimeMismatch {
                mismatched_count: mismatched_regime_count,
                active_count: summary.active_episode_count,
            });
        }

        let low_info_count = self
            .active_entries
            .iter()
            .filter(|e| {
                e.dimension_assessments.iter().any(|d| {
                    d.dimension == AnalogueDimension::InformationQuality
                        && (d.precedent_value == "low_fragmentary_or_delayed"
                            || d.precedent_value == "contested_or_censored")
                })
            })
            .count();
        if summary.active_episode_count > 0 && low_info_count * 2 > summary.active_episode_count {
            reasons.push(TransferLimitationReason::LowInformationQualityPrecedents {
                low_quality_count: low_info_count,
                active_count: summary.active_episode_count,
            });
        }

        if summary.excluded_counterexample_count > 0 {
            reasons.push(TransferLimitationReason::ExcludedCounterexamplesPresent {
                excluded_counterexample_count: summary.excluded_counterexample_count,
            });
        }

        if reasons.is_empty() {
            TransferabilityAssessment::AdequatePrecedentBasis
        } else {
            TransferabilityAssessment::SparseOrPoorlyTransferable { reasons }
        }
    }

    /// Issue an immutable forecast vintage (or linked revision) into [`ForecastLedger`] backed
    /// by this comparison set and a validated [`AnalogueForecastIssuanceSpec`] (`FORECAST-03`, `FORECAST-04`, `A15`, `P1`).
    ///
    /// - Validates conditional response branches, step dependencies, chained probabilities, and
    ///   monitoring indicators.
    /// - Rejects quantitative issuance if `justification` is [`PredictionJustification::QualitativeScenarioOnly`]
    ///   or if `assess_transferability()` is [`TransferabilityAssessment::SparseOrPoorlyTransferable`]
    ///   without a non-empty `sparse_history_override_rationale`.
    /// - Pins `self.current_version_id` as the first element of `ForecastDraft::comparison_set`
    ///   followed by each active episode's `VersionId`, merges cutoff-admissible precedent
    ///   sources with `spec.scenario_sources` (rejecting conflicting source metadata), and
    ///   retains an immutable [`IssuedAnalogueScenarioSnapshot`] on the issued [`ForecastVintageRecord`].
    pub fn issue_forecast<'a>(
        &self,
        ledger: &'a mut ForecastLedger,
        spec: AnalogueForecastIssuanceSpec,
    ) -> Result<&'a ForecastVintageRecord, AnalogueError> {
        if self.active_entries.is_empty() {
            return Err(AnalogueError::UnjustifiedQuantitativeForecast {
                reason: "cannot issue an analogue-backed forecast from an empty comparison set",
            });
        }

        let active_versions: BTreeSet<&VersionId> = self
            .active_entries
            .iter()
            .map(|e| &e.episode_version)
            .collect();

        validate_branches_and_indicators(&spec.branches, &spec.indicators, &active_versions)?;

        let transferability = self.assess_transferability();
        let (calibration_rationale, override_rationale) = match &spec.justification {
            PredictionJustification::QualitativeScenarioOnly { .. } => {
                return Err(AnalogueError::UnjustifiedQuantitativeForecast {
                    reason: "scenario is marked QualitativeScenarioOnly; numerical forecast issuance is not justified",
                });
            }
            PredictionJustification::JustifiedQuantitativeForecast {
                calibration_and_adjustment_rationale,
                sparse_history_override_rationale,
            } => {
                validate_text(
                    "justification.calibration_and_adjustment_rationale",
                    calibration_and_adjustment_rationale,
                )?;
                if matches!(
                    transferability,
                    TransferabilityAssessment::SparseOrPoorlyTransferable { .. }
                ) {
                    let Some(override_note) = sparse_history_override_rationale else {
                        return Err(AnalogueError::UnjustifiedQuantitativeForecast {
                            reason: "comparison set is SparseOrPoorlyTransferable; supply sparse_history_override_rationale or keep scenario QualitativeScenarioOnly",
                        });
                    };
                    validate_text(
                        "justification.sparse_history_override_rationale",
                        override_note,
                    )?;
                }
                (
                    calibration_and_adjustment_rationale.clone(),
                    sparse_history_override_rationale.clone(),
                )
            }
        };

        let comparison_set = self.pinned_comparison_versions();

        let mut merged_sources_by_version: BTreeMap<VersionId, CutoffSourceInput> = BTreeMap::new();
        for entry in &self.active_entries {
            for src in &entry.cutoff_admissible_sources {
                if let Some(existing) = merged_sources_by_version.get(&src.source_version) {
                    if existing != src {
                        return Err(AnalogueError::ConflictingSourceMetadata {
                            source_version: src.source_version.clone(),
                        });
                    }
                } else {
                    merged_sources_by_version.insert(src.source_version.clone(), src.clone());
                }
            }
        }
        for src in spec.scenario_sources {
            if let Some(existing) = merged_sources_by_version.get(&src.source_version) {
                if existing != &src {
                    return Err(AnalogueError::ConflictingSourceMetadata {
                        source_version: src.source_version,
                    });
                }
            } else {
                merged_sources_by_version.insert(src.source_version.clone(), src);
            }
        }
        let sources: Vec<CutoffSourceInput> = merged_sources_by_version.into_values().collect();

        let mut assumptions = spec.assumptions;
        assumptions.push(format!(
            "comparison_set_version={} (cutoff_utc_ms={}): {}",
            self.current_version_id, self.information_cutoff_utc_ms, calibration_rationale
        ));
        if let Some(override_note) = override_rationale {
            assumptions.push(format!("sparse_history_override: {override_note}"));
        }

        let scenario_snapshot = IssuedAnalogueScenarioSnapshot {
            comparison_set_id: self.comparison_set_id.clone(),
            comparison_set_version: self.current_version_id.clone(),
            branches: spec.branches,
            indicators: spec.indicators,
            transferability,
            justification: spec.justification,
        };

        let draft = ForecastDraft {
            forecast_id: spec.forecast_id,
            version_id: spec.version_id,
            schema_version: spec.schema_version,
            actor: spec.actor,
            issued_at_utc_ms: spec.issued_at_utc_ms,
            origin_utc_ms: spec.origin_utc_ms,
            information_cutoff_utc_ms: self.information_cutoff_utc_ms,
            horizon_start_utc_ms: spec.horizon_start_utc_ms,
            horizon_end_utc_ms: spec.horizon_end_utc_ms,
            cutoff_mode: self.cutoff_mode.clone(),
            target_id: spec.target_id,
            outcome_definition: spec.outcome_definition,
            baseline_version: spec.baseline_version,
            baseline_probability: spec.baseline_probability,
            forecast_probability: spec.forecast_probability,
            model_version: spec.model_version,
            model_knowledge_cutoff_utc_ms: spec.model_knowledge_cutoff_utc_ms,
            sources,
            comparison_set,
            assumptions,
            predecessor_version: spec.predecessor_version,
            revision_rationale: spec.revision_rationale,
        };

        let record = ledger.issue_forecast_with_analogue_scenario(draft, scenario_snapshot)?;
        Ok(record)
    }

    /// Evaluate timing error of an [`OutcomeResolutionRecord`] against this comparison set's
    /// precedent elapsed-time distribution (`FORECAST-04`, `A15`).
    pub fn evaluate_resolution_timing(
        &self,
        scenario_initiating_event_utc_ms: i64,
        resolution: &OutcomeResolutionRecord,
    ) -> Result<AnalogueTimingEvaluation, AnalogueError> {
        validate_ts(
            "scenario_initiating_event_utc_ms",
            scenario_initiating_event_utc_ms,
        )?;
        let summary = self.summarize_frequencies();
        match &resolution.state {
            ResolutionState::Resolved {
                occurred: true,
                observed_event_utc_ms: Some(obs_ms),
            } => {
                validate_ts("observed_event_utc_ms", *obs_ms)?;
                if *obs_ms < scenario_initiating_event_utc_ms {
                    return Err(AnalogueError::InvalidScenarioModel(
                        "observed_event_utc_ms cannot precede scenario_initiating_event_utc_ms",
                    ));
                }
                let observed_elapsed_ms = obs_ms
                    .checked_sub(scenario_initiating_event_utc_ms)
                    .ok_or(AnalogueError::InvalidScenarioModel(
                        "overflow computing observed_elapsed_ms",
                    ))?;
                if let Some(range) = summary.target_elapsed_time_range {
                    let signed_error_from_median_ms = observed_elapsed_ms
                        .checked_sub(range.median_elapsed_ms)
                        .ok_or(AnalogueError::InvalidScenarioModel(
                            "overflow computing signed_error_from_median_ms",
                        ))?;
                    let abs_error_from_median_ms = signed_error_from_median_ms
                        .checked_abs()
                        .ok_or(AnalogueError::InvalidScenarioModel(
                            "overflow computing abs_error_from_median_ms",
                        ))?;
                    let within_precedent_min_max_band = observed_elapsed_ms >= range.min_elapsed_ms
                        && observed_elapsed_ms <= range.max_elapsed_ms;
                    let band_excursion_ms = if observed_elapsed_ms < range.min_elapsed_ms {
                        observed_elapsed_ms - range.min_elapsed_ms
                    } else if observed_elapsed_ms > range.max_elapsed_ms {
                        observed_elapsed_ms - range.max_elapsed_ms
                    } else {
                        0
                    };
                    Ok(AnalogueTimingEvaluation::ResolvedOccurrence {
                        observed_event_utc_ms: *obs_ms,
                        observed_elapsed_ms,
                        precedent_range: range,
                        signed_error_from_median_ms,
                        abs_error_from_median_ms,
                        within_precedent_min_max_band,
                        band_excursion_ms,
                    })
                } else {
                    Ok(
                        AnalogueTimingEvaluation::ResolvedOccurrenceWithoutPrecedentRange {
                            observed_event_utc_ms: *obs_ms,
                            observed_elapsed_ms,
                        },
                    )
                }
            }
            ResolutionState::Resolved {
                occurred: true,
                observed_event_utc_ms: None,
            } => Err(AnalogueError::InvalidScenarioModel(
                "Resolved { occurred: true } must carry Some(observed_event_utc_ms)",
            )),
            ResolutionState::Resolved {
                occurred: false,
                observed_event_utc_ms,
            } => Ok(AnalogueTimingEvaluation::ResolvedNonOccurrence {
                out_of_window_event_utc_ms: *observed_event_utc_ms,
            }),
            ResolutionState::Unresolved(reason) => {
                Ok(AnalogueTimingEvaluation::Unresolved(*reason))
            }
            ResolutionState::Invalidated { reason } => Ok(AnalogueTimingEvaluation::Invalidated {
                reason: reason.clone(),
            }),
        }
    }
}

/// Evaluate what was knowable about `episode` at `information_cutoff_utc_ms` under `cutoff_mode`,
/// and compare its 7 dimensions against `target_profile` (`FORECAST-01`, `FORECAST-02`, `FORECAST-04`).
///
/// Validates `episode` structure and evidence references before evaluating point-in-time status (`B1`).
pub(crate) fn evaluate_episode_against_target(
    episode: &PrecedentEpisodeRecord,
    target_profile: &AnalogueProfile,
    information_cutoff_utc_ms: i64,
    cutoff_mode: &CutoffAdmissibilityMode,
) -> Result<AnalogueMatchEntry, AnalogueError> {
    validate_episode_standalone(episode)?;
    validate_profile("target_profile", target_profile)?;
    validate_ts("information_cutoff_utc_ms", information_cutoff_utc_ms)?;

    let source_by_ver: BTreeMap<&VersionId, &CutoffSourceInput> = episode
        .sources
        .iter()
        .map(|s| (&s.source_version, s))
        .collect();

    let (point_in_time_status, role, knowable_target_elapsed_ms, cutoff_admissible_sources) =
        evaluate_point_in_time_knowability(
            episode,
            &source_by_ver,
            information_cutoff_utc_ms,
            cutoff_mode,
        );

    let (
        dimension_assessments,
        matched_dimension_count,
        institutional_difference_note,
        transfer_limitations,
    ) = compare_profiles(&episode.profile, target_profile, &episode.dependency);

    Ok(AnalogueMatchEntry {
        episode_id: episode.episode_id.clone(),
        episode_version: episode.version_id.clone(),
        role,
        point_in_time_status,
        matched_dimension_count,
        dimension_assessments,
        institutional_difference_note,
        transfer_limitations,
        independence_cluster_key: episode.independence_cluster_key(),
        dependency: episode.dependency.clone(),
        knowable_target_elapsed_ms,
        cutoff_admissible_sources,
        cited_outcome_evidence_versions: episode.outcome.cited_evidence_versions().to_vec(),
    })
}

/// Compute connected components across `active_entries` using union-find over declared cluster keys,
/// `PolicySpilloverOrRetaliation` predecessor links, `SharedSourceLineageOnly` upstream sources, and
/// any shared `cited_outcome_evidence_versions` so chained or mixed-kind shared-source dependencies
/// always collapse into a single `independence_cluster_key` (`FORECAST-02`, `N2`, `AGENTS.md` §2).
fn recompute_effective_cluster_keys(entries: &mut [AnalogueMatchEntry]) {
    let n = entries.len();
    if n == 0 {
        return;
    }

    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }

    fn union(parent: &mut [usize], i: usize, j: usize) {
        let ri = find(parent, i);
        let rj = find(parent, j);
        if ri != rj {
            if ri < rj {
                parent[rj] = ri;
            } else {
                parent[ri] = rj;
            }
        }
    }

    let mut parent: Vec<usize> = (0..n).collect();
    let mut base_keys: Vec<String> = Vec::with_capacity(n);
    let mut key_to_first_idx: BTreeMap<String, usize> = BTreeMap::new();
    let mut episode_id_to_idx: BTreeMap<OpaqueId, usize> = BTreeMap::new();
    let mut source_to_episodes: BTreeMap<VersionId, Vec<usize>> = BTreeMap::new();

    for (idx, entry) in entries.iter().enumerate() {
        let base_key = match &entry.dependency {
            EpisodeDependencyKind::IndependentEpisode => {
                format!("episode:{}", entry.episode_id.as_str())
            }
            EpisodeDependencyKind::SharedCrisisOrCampaign { cluster_id, .. }
            | EpisodeDependencyKind::PolicySpilloverOrRetaliation { cluster_id, .. } => {
                format!("cluster:{}", cluster_id.as_str())
            }
            EpisodeDependencyKind::SharedSourceLineageOnly {
                upstream_source_version,
                ..
            } => {
                format!("lineage:{}", upstream_source_version.as_str())
            }
        };
        if let Some(&first_idx) = key_to_first_idx.get(&base_key) {
            union(&mut parent, idx, first_idx);
        } else {
            key_to_first_idx.insert(base_key.clone(), idx);
        }
        base_keys.push(base_key);
        episode_id_to_idx.insert(entry.episode_id.clone(), idx);

        for ver in &entry.cited_outcome_evidence_versions {
            source_to_episodes.entry(ver.clone()).or_default().push(idx);
        }
        if let EpisodeDependencyKind::SharedSourceLineageOnly {
            upstream_source_version,
            ..
        } = &entry.dependency
        {
            let list = source_to_episodes
                .entry(upstream_source_version.clone())
                .or_default();
            if !list.contains(&idx) {
                list.push(idx);
            }
        }
    }

    for (idx, entry) in entries.iter().enumerate() {
        if let EpisodeDependencyKind::PolicySpilloverOrRetaliation {
            predecessor_episode_id,
            ..
        } = &entry.dependency
            && let Some(&pred_idx) = episode_id_to_idx.get(predecessor_episode_id)
        {
            union(&mut parent, idx, pred_idx);
        }
    }

    let mut shared_sources_by_entry: Vec<Vec<VersionId>> = vec![Vec::new(); n];
    for (shared_ver, indices) in source_to_episodes {
        if indices.len() > 1 {
            let first = indices[0];
            for &idx in &indices[1..] {
                union(&mut parent, first, idx);
            }
            for idx in indices {
                shared_sources_by_entry[idx].push(shared_ver.clone());
            }
        }
    }

    let mut component_members: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for idx in 0..n {
        let root = find(&mut parent, idx);
        component_members.entry(root).or_default().push(idx);
    }

    for members in component_members.values() {
        let canonical_key = if members.len() == 1 {
            base_keys[members[0]].clone()
        } else {
            let unique_base_keys: BTreeSet<&str> =
                members.iter().map(|&i| base_keys[i].as_str()).collect();
            if unique_base_keys.len() == 1 {
                base_keys[members[0]].clone()
            } else {
                let mut component_shared_sources: BTreeSet<&str> = BTreeSet::new();
                for &i in members {
                    for ver in &shared_sources_by_entry[i] {
                        component_shared_sources.insert(ver.as_str());
                    }
                }
                if let Some(min_src) = component_shared_sources.first() {
                    format!("shared-outcome-source:{min_src}")
                } else {
                    base_keys[members[0]].clone()
                }
            }
        };

        for &idx in members {
            entries[idx].independence_cluster_key = canonical_key.clone();
            for shared_ver in &shared_sources_by_entry[idx] {
                let note = format!(
                    "shares outcome evidence source {shared_ver} with another active precedent episode; merged into {canonical_key} for cluster deduplication"
                );
                if !entries[idx].transfer_limitations.contains(&note) {
                    entries[idx].transfer_limitations.push(note);
                }
            }
        }
    }
}

fn evaluate_point_in_time_knowability(
    episode: &PrecedentEpisodeRecord,
    source_by_ver: &BTreeMap<&VersionId, &CutoffSourceInput>,
    cutoff_utc_ms: i64,
    cutoff_mode: &CutoffAdmissibilityMode,
) -> (
    EpisodePointInTimeStatus,
    PrecedentRole,
    Option<i64>,
    Vec<CutoffSourceInput>,
) {
    if matches!(cutoff_mode, CutoffAdmissibilityMode::ProspectiveStrict)
        && episode.recorded_at_utc_ms > cutoff_utc_ms
    {
        return (
            EpisodePointInTimeStatus::InadmissibleAtCutoff {
                reason: format!(
                    "episode recorded_at_utc_ms ({}) is after strict information_cutoff_utc_ms ({cutoff_utc_ms})",
                    episode.recorded_at_utc_ms
                ),
            },
            PrecedentRole::CensoredAtCutoff,
            None,
            Vec::new(),
        );
    }

    if episode.initiating_event_utc_ms > cutoff_utc_ms {
        return (
            EpisodePointInTimeStatus::InadmissibleAtCutoff {
                reason: format!(
                    "initiating_event_utc_ms ({}) is after information_cutoff_utc_ms ({cutoff_utc_ms})",
                    episode.initiating_event_utc_ms
                ),
            },
            PrecedentRole::CensoredAtCutoff,
            None,
            Vec::new(),
        );
    }

    if episode.initiation_evidence_versions.is_empty() {
        return (
            EpisodePointInTimeStatus::InadmissibleAtCutoff {
                reason: "episode has no initiation_evidence_versions".to_owned(),
            },
            PrecedentRole::CensoredAtCutoff,
            None,
            Vec::new(),
        );
    }

    for ev_ver in &episode.initiation_evidence_versions {
        let Some(src) = source_by_ver.get(ev_ver) else {
            return (
                EpisodePointInTimeStatus::InadmissibleAtCutoff {
                    reason: format!("missing initiation source {ev_ver}"),
                },
                PrecedentRole::CensoredAtCutoff,
                None,
                Vec::new(),
            );
        };
        if let Err(err) = validate_source_admissible_at_cutoff(src, cutoff_utc_ms, cutoff_mode) {
            return (
                EpisodePointInTimeStatus::InadmissibleAtCutoff {
                    reason: format!("initiation source {ev_ver} inadmissible at cutoff: {err}"),
                },
                PrecedentRole::CensoredAtCutoff,
                None,
                Vec::new(),
            );
        }
    }

    let mut admissible_sources_map: BTreeMap<VersionId, CutoffSourceInput> = BTreeMap::new();
    for ev_ver in &episode.initiation_evidence_versions {
        if let Some(src) = source_by_ver.get(ev_ver) {
            admissible_sources_map.insert((*ev_ver).clone(), (*src).clone());
        }
    }

    let mut admissible_steps = 0usize;
    for step in &episode.response_steps {
        if step.action_utc_ms <= cutoff_utc_ms
            && !step.evidence_versions.is_empty()
            && step.evidence_versions.iter().all(|v| {
                source_by_ver.get(v).is_some_and(|s| {
                    validate_source_admissible_at_cutoff(s, cutoff_utc_ms, cutoff_mode).is_ok()
                })
            })
        {
            admissible_steps += 1;
            for v in &step.evidence_versions {
                if let Some(src) = source_by_ver.get(v) {
                    admissible_sources_map.insert((*v).clone(), (*src).clone());
                }
            }
            if let TemporalSequenceRelation::AssessedCausalLink {
                supporting_evidence_versions,
                ..
            } = &step.sequence_relation
            {
                for v in supporting_evidence_versions {
                    if let Some(src) = source_by_ver.get(v)
                        && validate_source_admissible_at_cutoff(src, cutoff_utc_ms, cutoff_mode)
                            .is_ok()
                    {
                        admissible_sources_map.insert((*v).clone(), (*src).clone());
                    }
                }
            }
        }
    }

    match &episode.outcome {
        PrecedentOutcomeDisposition::CensoredOrIncompleteFollowUp {
            censor_reason,
            coverage_gap_note,
            partial_evidence_versions,
            ..
        } => {
            for v in partial_evidence_versions {
                if let Some(src) = source_by_ver.get(v)
                    && validate_source_admissible_at_cutoff(src, cutoff_utc_ms, cutoff_mode).is_ok()
                {
                    admissible_sources_map.insert((*v).clone(), (*src).clone());
                }
            }
            (
                EpisodePointInTimeStatus::InitiatedButOutcomeCensoredAtCutoff {
                    initiated_at_utc_ms: episode.initiating_event_utc_ms,
                    censor_reason: *censor_reason,
                    explanation: coverage_gap_note.clone(),
                    admissible_response_steps: admissible_steps,
                },
                PrecedentRole::CensoredAtCutoff,
                None,
                admissible_sources_map.into_values().collect(),
            )
        }
        PrecedentOutcomeDisposition::TargetResponseOccurred {
            resolved_at_utc_ms,
            elapsed_from_initiation_ms,
            outcome_evidence_versions,
            ..
        } => {
            match check_outcome_knowable_at_cutoff(
                *resolved_at_utc_ms,
                outcome_evidence_versions,
                source_by_ver,
                cutoff_utc_ms,
                cutoff_mode,
            ) {
                Ok(latest_pub_ms) => {
                    for v in outcome_evidence_versions {
                        if let Some(src) = source_by_ver.get(v) {
                            admissible_sources_map.insert((*v).clone(), (*src).clone());
                        }
                    }
                    (
                        EpisodePointInTimeStatus::OutcomeKnowableAtCutoff {
                            resolved_at_utc_ms: *resolved_at_utc_ms,
                            latest_outcome_source_available_utc_ms: latest_pub_ms,
                            admissible_response_steps: admissible_steps,
                        },
                        PrecedentRole::SupportiveAnalogue,
                        Some(*elapsed_from_initiation_ms),
                        admissible_sources_map.into_values().collect(),
                    )
                }
                Err(censor_explanation) => (
                    EpisodePointInTimeStatus::InitiatedButOutcomeCensoredAtCutoff {
                        initiated_at_utc_ms: episode.initiating_event_utc_ms,
                        censor_reason: UnresolvedReason::Pending,
                        explanation: censor_explanation,
                        admissible_response_steps: admissible_steps,
                    },
                    PrecedentRole::CensoredAtCutoff,
                    None,
                    admissible_sources_map.into_values().collect(),
                ),
            }
        }
        PrecedentOutcomeDisposition::DivergentResponseOccurred {
            resolved_at_utc_ms,
            outcome_evidence_versions,
            ..
        } => {
            match check_outcome_knowable_at_cutoff(
                *resolved_at_utc_ms,
                outcome_evidence_versions,
                source_by_ver,
                cutoff_utc_ms,
                cutoff_mode,
            ) {
                Ok(latest_pub_ms) => {
                    for v in outcome_evidence_versions {
                        if let Some(src) = source_by_ver.get(v) {
                            admissible_sources_map.insert((*v).clone(), (*src).clone());
                        }
                    }
                    (
                        EpisodePointInTimeStatus::OutcomeKnowableAtCutoff {
                            resolved_at_utc_ms: *resolved_at_utc_ms,
                            latest_outcome_source_available_utc_ms: latest_pub_ms,
                            admissible_response_steps: admissible_steps,
                        },
                        PrecedentRole::DivergentCounterexample,
                        None,
                        admissible_sources_map.into_values().collect(),
                    )
                }
                Err(censor_explanation) => (
                    EpisodePointInTimeStatus::InitiatedButOutcomeCensoredAtCutoff {
                        initiated_at_utc_ms: episode.initiating_event_utc_ms,
                        censor_reason: UnresolvedReason::Pending,
                        explanation: censor_explanation,
                        admissible_response_steps: admissible_steps,
                    },
                    PrecedentRole::CensoredAtCutoff,
                    None,
                    admissible_sources_map.into_values().collect(),
                ),
            }
        }
        PrecedentOutcomeDisposition::DocumentedNonResponse {
            observed_window_end_utc_ms,
            outcome_evidence_versions,
            ..
        } => {
            match check_outcome_knowable_at_cutoff(
                *observed_window_end_utc_ms,
                outcome_evidence_versions,
                source_by_ver,
                cutoff_utc_ms,
                cutoff_mode,
            ) {
                Ok(latest_pub_ms) => {
                    for v in outcome_evidence_versions {
                        if let Some(src) = source_by_ver.get(v) {
                            admissible_sources_map.insert((*v).clone(), (*src).clone());
                        }
                    }
                    (
                        EpisodePointInTimeStatus::OutcomeKnowableAtCutoff {
                            resolved_at_utc_ms: *observed_window_end_utc_ms,
                            latest_outcome_source_available_utc_ms: latest_pub_ms,
                            admissible_response_steps: admissible_steps,
                        },
                        PrecedentRole::DocumentedNonResponseCounterexample,
                        None,
                        admissible_sources_map.into_values().collect(),
                    )
                }
                Err(censor_explanation) => (
                    EpisodePointInTimeStatus::InitiatedButOutcomeCensoredAtCutoff {
                        initiated_at_utc_ms: episode.initiating_event_utc_ms,
                        censor_reason: UnresolvedReason::Pending,
                        explanation: censor_explanation,
                        admissible_response_steps: admissible_steps,
                    },
                    PrecedentRole::CensoredAtCutoff,
                    None,
                    admissible_sources_map.into_values().collect(),
                ),
            }
        }
    }
}

fn check_outcome_knowable_at_cutoff(
    terminal_utc_ms: i64,
    outcome_evidence_versions: &[VersionId],
    source_by_ver: &BTreeMap<&VersionId, &CutoffSourceInput>,
    cutoff_utc_ms: i64,
    cutoff_mode: &CutoffAdmissibilityMode,
) -> Result<i64, String> {
    if outcome_evidence_versions.is_empty() {
        return Err("resolved outcome must cite at least one outcome_evidence_version".to_owned());
    }
    if terminal_utc_ms > cutoff_utc_ms {
        return Err(format!(
            "terminal outcome timestamp ({terminal_utc_ms}) occurred after information_cutoff_utc_ms ({cutoff_utc_ms}); right-censored at cutoff"
        ));
    }
    let mut latest_pub_ms = terminal_utc_ms;
    for v in outcome_evidence_versions {
        let Some(src) = source_by_ver.get(v) else {
            return Err(format!("missing outcome source {v}"));
        };
        let pub_ms = validate_source_admissible_at_cutoff(src, cutoff_utc_ms, cutoff_mode)
            .map_err(|e| e.to_string())?;
        if pub_ms > latest_pub_ms {
            latest_pub_ms = pub_ms;
        }
    }
    Ok(latest_pub_ms)
}

fn compare_profiles(
    precedent: &AnalogueProfile,
    target: &AnalogueProfile,
    dependency: &EpisodeDependencyKind,
) -> (Vec<DimensionAssessment>, usize, Option<String>, Vec<String>) {
    let mut assessments = Vec::with_capacity(7);
    let mut matched = 0usize;
    let mut institutional_difference_note = None;
    let mut transfer_limitations = Vec::new();

    // 1. EventScale
    let scale_match = precedent.event_scale == target.event_scale;
    if scale_match {
        matched += 1;
    } else {
        transfer_limitations.push(format!(
            "event scale differs (precedent={}, target={})",
            precedent.event_scale, target.event_scale
        ));
    }
    assessments.push(DimensionAssessment {
        dimension: AnalogueDimension::EventScale,
        matches_target: scale_match,
        target_value: target.event_scale.to_string(),
        precedent_value: precedent.event_scale.to_string(),
        explanation: if scale_match {
            format!("Matching event scale ({})", target.event_scale)
        } else {
            format!(
                "Precedent scale ({}) differs from target ({})",
                precedent.event_scale, target.event_scale
            )
        },
    });

    // 2. Geography
    let geo_match = precedent
        .geography_region
        .eq_ignore_ascii_case(&target.geography_region);
    if geo_match {
        matched += 1;
    } else {
        transfer_limitations.push(format!(
            "geography differs (precedent='{}', target='{}')",
            precedent.geography_region, target.geography_region
        ));
    }
    assessments.push(DimensionAssessment {
        dimension: AnalogueDimension::Geography,
        matches_target: geo_match,
        target_value: target.geography_region.clone(),
        precedent_value: precedent.geography_region.clone(),
        explanation: if geo_match {
            format!("Matching region ({})", target.geography_region)
        } else {
            format!(
                "Cross-regional comparison ('{}' vs target '{}')",
                precedent.geography_region, target.geography_region
            )
        },
    });

    // 3. ActorSector
    let sector_match = precedent.actor_sector == target.actor_sector;
    if sector_match {
        matched += 1;
    } else {
        transfer_limitations.push(format!(
            "actor sector differs (precedent={}, target={})",
            precedent.actor_sector, target.actor_sector
        ));
    }
    assessments.push(DimensionAssessment {
        dimension: AnalogueDimension::ActorSector,
        matches_target: sector_match,
        target_value: format!("{} ({})", target.actor_sector, target.actor_profile),
        precedent_value: format!("{} ({})", precedent.actor_sector, precedent.actor_profile),
        explanation: if sector_match {
            format!(
                "Matching actor sector ({}); precedent actor '{}', target '{}'",
                target.actor_sector, precedent.actor_profile, target.actor_profile
            )
        } else {
            format!(
                "Different actor sector (precedent {} '{}', target {} '{}')",
                precedent.actor_sector,
                precedent.actor_profile,
                target.actor_sector,
                target.actor_profile
            )
        },
    });

    // 4. InstitutionalRegime
    let regime_match = precedent.institutional_regime == target.institutional_regime;
    if regime_match {
        matched += 1;
    } else {
        let diff_msg = format!(
            "institutional regime differs: precedent operated under '{}' whereas target operates under '{}'",
            precedent.institutional_regime, target.institutional_regime
        );
        institutional_difference_note = Some(diff_msg.clone());
        transfer_limitations.push(diff_msg);
    }
    assessments.push(DimensionAssessment {
        dimension: AnalogueDimension::InstitutionalRegime,
        matches_target: regime_match,
        target_value: target.institutional_regime.to_string(),
        precedent_value: precedent.institutional_regime.to_string(),
        explanation: if regime_match {
            format!(
                "Matching institutional regime ({})",
                target.institutional_regime
            )
        } else {
            format!(
                "Institutional difference: precedent '{}' vs target '{}'",
                precedent.institutional_regime, target.institutional_regime
            )
        },
    });

    // 5. TensionLevel
    let tension_match = precedent.tension_level == target.tension_level;
    if tension_match {
        matched += 1;
    } else {
        transfer_limitations.push(format!(
            "baseline tension differs (precedent={}, target={})",
            precedent.tension_level, target.tension_level
        ));
    }
    assessments.push(DimensionAssessment {
        dimension: AnalogueDimension::TensionLevel,
        matches_target: tension_match,
        target_value: target.tension_level.to_string(),
        precedent_value: precedent.tension_level.to_string(),
        explanation: if tension_match {
            format!("Matching tension level ({})", target.tension_level)
        } else {
            format!(
                "Precedent tension ({}) differs from target ({})",
                precedent.tension_level, target.tension_level
            )
        },
    });

    // 6. EconomicCondition
    let econ_match = precedent.economic_condition == target.economic_condition;
    if econ_match {
        matched += 1;
    } else {
        transfer_limitations.push(format!(
            "economic condition differs (precedent={}, target={})",
            precedent.economic_condition, target.economic_condition
        ));
    }
    assessments.push(DimensionAssessment {
        dimension: AnalogueDimension::EconomicCondition,
        matches_target: econ_match,
        target_value: target.economic_condition.to_string(),
        precedent_value: precedent.economic_condition.to_string(),
        explanation: if econ_match {
            format!(
                "Matching economic condition ({})",
                target.economic_condition
            )
        } else {
            format!(
                "Precedent economic condition ({}) differs from target ({})",
                precedent.economic_condition, target.economic_condition
            )
        },
    });

    // 7. InformationQuality
    let info_match = precedent.information_quality == target.information_quality;
    if info_match {
        matched += 1;
    }
    if matches!(
        precedent.information_quality,
        InformationQuality::LowFragmentaryOrDelayed | InformationQuality::ContestedOrCensored
    ) {
        transfer_limitations.push(format!(
            "precedent information quality is {}",
            precedent.information_quality
        ));
    }
    assessments.push(DimensionAssessment {
        dimension: AnalogueDimension::InformationQuality,
        matches_target: info_match,
        target_value: target.information_quality.to_string(),
        precedent_value: precedent.information_quality.to_string(),
        explanation: if info_match {
            format!(
                "Matching information quality ({})",
                target.information_quality
            )
        } else {
            format!(
                "Precedent information quality ({}) vs target ({})",
                precedent.information_quality, target.information_quality
            )
        },
    });

    match dependency {
        EpisodeDependencyKind::IndependentEpisode => {}
        EpisodeDependencyKind::SharedCrisisOrCampaign {
            cluster_id,
            dependency_note,
        } => {
            transfer_limitations.push(format!(
                "shared crisis cluster {cluster_id}: {dependency_note}"
            ));
        }
        EpisodeDependencyKind::PolicySpilloverOrRetaliation {
            cluster_id,
            predecessor_episode_id,
            dependency_note,
        } => {
            transfer_limitations.push(format!(
                "policy spillover from {predecessor_episode_id} in cluster {cluster_id}: {dependency_note}"
            ));
        }
        EpisodeDependencyKind::SharedSourceLineageOnly {
            upstream_source_version,
            dependency_note,
        } => {
            transfer_limitations.push(format!(
                "shared upstream source lineage {upstream_source_version}: {dependency_note}"
            ));
        }
    }

    (
        assessments,
        matched,
        institutional_difference_note,
        transfer_limitations,
    )
}

fn validate_episode_standalone(episode: &PrecedentEpisodeRecord) -> Result<(), AnalogueError> {
    validate_text("episode.title", &episode.title)?;
    validate_text(
        "episode.initiating_condition",
        &episode.initiating_condition,
    )?;
    validate_profile("episode.profile", &episode.profile)?;
    validate_ts("initiating_event_utc_ms", episode.initiating_event_utc_ms)?;
    validate_ts("recorded_at_utc_ms", episode.recorded_at_utc_ms)?;
    if episode.recorded_at_utc_ms < episode.initiating_event_utc_ms {
        return Err(AnalogueError::InvalidEpisodeChronology {
            episode_id: episode.episode_id.clone(),
            reason: "recorded_at_utc_ms cannot precede initiating_event_utc_ms",
        });
    }

    if episode.sources.is_empty() || episode.sources.len() > MAX_FORECAST_COLLECTION_LEN {
        return Err(AnalogueError::BudgetExceeded {
            field: "episode.sources",
            actual: episode.sources.len(),
            limit: MAX_FORECAST_COLLECTION_LEN,
        });
    }

    let mut source_map: BTreeMap<&VersionId, &CutoffSourceInput> = BTreeMap::new();
    for src in &episode.sources {
        if let Some(ev_ms) = src.event_utc_ms {
            validate_ts("source.event_utc_ms", ev_ms)?;
            if let Some(pub_ms) = src.public_available_at_utc_ms
                && ev_ms > pub_ms
            {
                return Err(AnalogueError::InvalidEpisodeSources(
                    "source event_utc_ms cannot exceed public_available_at_utc_ms",
                ));
            }
        }
        if let Some(pub_ms) = src.public_available_at_utc_ms {
            validate_ts("source.public_available_at_utc_ms", pub_ms)?;
        }
        if let Some(ing_ms) = src.ingested_at_utc_ms {
            validate_ts("source.ingested_at_utc_ms", ing_ms)?;
        }
        if source_map.insert(&src.source_version, src).is_some() {
            return Err(AnalogueError::InvalidEpisodeSources(
                "duplicate source_version in episode.sources",
            ));
        }
    }

    validate_evidence_refs(
        "episode.initiation_evidence_versions",
        &episode.initiation_evidence_versions,
        &source_map,
    )?;

    match &episode.dependency {
        EpisodeDependencyKind::IndependentEpisode => {}
        EpisodeDependencyKind::SharedCrisisOrCampaign {
            dependency_note, ..
        } => {
            validate_text("dependency.dependency_note", dependency_note)?;
        }
        EpisodeDependencyKind::PolicySpilloverOrRetaliation {
            predecessor_episode_id,
            dependency_note,
            ..
        } => {
            validate_text("dependency.dependency_note", dependency_note)?;
            if predecessor_episode_id == &episode.episode_id {
                return Err(AnalogueError::InvalidEpisodeDependency {
                    episode_id: episode.episode_id.clone(),
                    reason: "episode cannot reference itself as predecessor_episode_id",
                });
            }
        }
        EpisodeDependencyKind::SharedSourceLineageOnly {
            upstream_source_version,
            dependency_note,
        } => {
            validate_text("dependency.dependency_note", dependency_note)?;
            if !source_map.contains_key(upstream_source_version) {
                return Err(AnalogueError::InvalidEpisodeDependency {
                    episode_id: episode.episode_id.clone(),
                    reason: "upstream_source_version must be present in episode.sources",
                });
            }
        }
    }

    if episode.response_steps.len() > MAX_RESPONSE_STEPS_PER_EPISODE {
        return Err(AnalogueError::BudgetExceeded {
            field: "episode.response_steps",
            actual: episode.response_steps.len(),
            limit: MAX_RESPONSE_STEPS_PER_EPISODE,
        });
    }

    let mut seen_step_ids = BTreeSet::new();
    let mut prev_step_ms = episode.initiating_event_utc_ms;
    for step in &episode.response_steps {
        if !seen_step_ids.insert(&step.step_id) {
            return Err(AnalogueError::DuplicateIdentifier {
                kind: "step_id",
                identifier: step.step_id.as_str().to_owned(),
            });
        }
        validate_text("step.action_summary", &step.action_summary)?;
        if let Some(intent) = &step.announced_intention {
            validate_text("step.announced_intention", intent)?;
        }
        validate_ts("step.action_utc_ms", step.action_utc_ms)?;
        if step.action_utc_ms < prev_step_ms {
            return Err(AnalogueError::InvalidEpisodeChronology {
                episode_id: episode.episode_id.clone(),
                reason: "response_steps must be ordered non-decreasing in action_utc_ms and >= initiating_event_utc_ms",
            });
        }
        if step.action_utc_ms > episode.recorded_at_utc_ms {
            return Err(AnalogueError::InvalidEpisodeChronology {
                episode_id: episode.episode_id.clone(),
                reason: "step.action_utc_ms cannot exceed episode.recorded_at_utc_ms",
            });
        }
        let expected_elapsed = step
            .action_utc_ms
            .checked_sub(episode.initiating_event_utc_ms)
            .ok_or(AnalogueError::InvalidEpisodeChronology {
                episode_id: episode.episode_id.clone(),
                reason: "overflow computing step elapsed_from_initiation_ms",
            })?;
        if step.elapsed_from_initiation_ms != expected_elapsed {
            return Err(AnalogueError::InvalidEpisodeChronology {
                episode_id: episode.episode_id.clone(),
                reason: "step.elapsed_from_initiation_ms must equal action_utc_ms - initiating_event_utc_ms",
            });
        }
        validate_evidence_refs(
            "step.evidence_versions",
            &step.evidence_versions,
            &source_map,
        )?;
        if let TemporalSequenceRelation::AssessedCausalLink {
            rationale,
            supporting_evidence_versions,
        } = &step.sequence_relation
        {
            validate_text("step.sequence_relation.rationale", rationale)?;
            validate_evidence_refs(
                "step.sequence_relation.supporting_evidence_versions",
                supporting_evidence_versions,
                &source_map,
            )?;
        }
        prev_step_ms = step.action_utc_ms;
    }

    match &episode.outcome {
        PrecedentOutcomeDisposition::TargetResponseOccurred {
            outcome_summary,
            resolved_at_utc_ms,
            elapsed_from_initiation_ms,
            outcome_evidence_versions,
        } => {
            validate_text("outcome.outcome_summary", outcome_summary)?;
            validate_ts("outcome.resolved_at_utc_ms", *resolved_at_utc_ms)?;
            if *resolved_at_utc_ms < prev_step_ms
                || *resolved_at_utc_ms > episode.recorded_at_utc_ms
            {
                return Err(AnalogueError::InvalidEpisodeChronology {
                    episode_id: episode.episode_id.clone(),
                    reason: "outcome resolved_at_utc_ms must lie within [last_step_or_initiation_utc_ms, recorded_at_utc_ms]",
                });
            }
            let expected = resolved_at_utc_ms
                .checked_sub(episode.initiating_event_utc_ms)
                .ok_or(AnalogueError::InvalidEpisodeChronology {
                    episode_id: episode.episode_id.clone(),
                    reason: "overflow computing outcome elapsed_from_initiation_ms",
                })?;
            if *elapsed_from_initiation_ms != expected {
                return Err(AnalogueError::InvalidEpisodeChronology {
                    episode_id: episode.episode_id.clone(),
                    reason: "outcome elapsed_from_initiation_ms must equal resolved_at_utc_ms - initiating_event_utc_ms",
                });
            }
            validate_evidence_refs(
                "outcome.outcome_evidence_versions",
                outcome_evidence_versions,
                &source_map,
            )?;
        }
        PrecedentOutcomeDisposition::DivergentResponseOccurred {
            divergent_action_summary,
            resolved_at_utc_ms,
            elapsed_from_initiation_ms,
            outcome_evidence_versions,
        } => {
            validate_text("outcome.divergent_action_summary", divergent_action_summary)?;
            validate_ts("outcome.resolved_at_utc_ms", *resolved_at_utc_ms)?;
            if *resolved_at_utc_ms < prev_step_ms
                || *resolved_at_utc_ms > episode.recorded_at_utc_ms
            {
                return Err(AnalogueError::InvalidEpisodeChronology {
                    episode_id: episode.episode_id.clone(),
                    reason: "outcome resolved_at_utc_ms must lie within [last_step_or_initiation_utc_ms, recorded_at_utc_ms]",
                });
            }
            let expected = resolved_at_utc_ms
                .checked_sub(episode.initiating_event_utc_ms)
                .ok_or(AnalogueError::InvalidEpisodeChronology {
                    episode_id: episode.episode_id.clone(),
                    reason: "overflow computing outcome elapsed_from_initiation_ms",
                })?;
            if *elapsed_from_initiation_ms != expected {
                return Err(AnalogueError::InvalidEpisodeChronology {
                    episode_id: episode.episode_id.clone(),
                    reason: "outcome elapsed_from_initiation_ms must equal resolved_at_utc_ms - initiating_event_utc_ms",
                });
            }
            validate_evidence_refs(
                "outcome.outcome_evidence_versions",
                outcome_evidence_versions,
                &source_map,
            )?;
        }
        PrecedentOutcomeDisposition::DocumentedNonResponse {
            observed_window_end_utc_ms,
            observation_duration_ms,
            non_response_rationale,
            outcome_evidence_versions,
        } => {
            validate_text("outcome.non_response_rationale", non_response_rationale)?;
            validate_ts(
                "outcome.observed_window_end_utc_ms",
                *observed_window_end_utc_ms,
            )?;
            if *observed_window_end_utc_ms <= episode.initiating_event_utc_ms
                || *observed_window_end_utc_ms < prev_step_ms
                || *observed_window_end_utc_ms > episode.recorded_at_utc_ms
            {
                return Err(AnalogueError::InvalidEpisodeChronology {
                    episode_id: episode.episode_id.clone(),
                    reason: "DocumentedNonResponse observed_window_end_utc_ms must exceed initiating_event_utc_ms and all steps and be <= recorded_at_utc_ms",
                });
            }
            let expected = observed_window_end_utc_ms
                .checked_sub(episode.initiating_event_utc_ms)
                .ok_or(AnalogueError::InvalidEpisodeChronology {
                    episode_id: episode.episode_id.clone(),
                    reason: "overflow computing observation_duration_ms",
                })?;
            if *observation_duration_ms != expected {
                return Err(AnalogueError::InvalidEpisodeChronology {
                    episode_id: episode.episode_id.clone(),
                    reason: "observation_duration_ms must equal observed_window_end_utc_ms - initiating_event_utc_ms",
                });
            }
            validate_evidence_refs(
                "outcome.outcome_evidence_versions",
                outcome_evidence_versions,
                &source_map,
            )?;
        }
        PrecedentOutcomeDisposition::CensoredOrIncompleteFollowUp {
            observed_until_utc_ms,
            coverage_gap_note,
            partial_evidence_versions,
            ..
        } => {
            validate_text("outcome.coverage_gap_note", coverage_gap_note)?;
            validate_ts("outcome.observed_until_utc_ms", *observed_until_utc_ms)?;
            if *observed_until_utc_ms < prev_step_ms
                || *observed_until_utc_ms > episode.recorded_at_utc_ms
            {
                return Err(AnalogueError::InvalidEpisodeChronology {
                    episode_id: episode.episode_id.clone(),
                    reason: "observed_until_utc_ms must lie within [last_step_or_initiation_utc_ms, recorded_at_utc_ms]",
                });
            }
            validate_evidence_refs(
                "outcome.partial_evidence_versions",
                partial_evidence_versions,
                &source_map,
            )?;
        }
    }

    Ok(())
}

fn validate_episode_record(
    episode: &PrecedentEpisodeRecord,
    registered_episodes: &BTreeMap<OpaqueId, Vec<VersionId>>,
    library_sources: &BTreeMap<VersionId, CutoffSourceInput>,
) -> Result<(), AnalogueError> {
    validate_episode_standalone(episode)?;

    for src in &episode.sources {
        if let Some(existing) = library_sources.get(&src.source_version)
            && existing != src
        {
            return Err(AnalogueError::ConflictingSourceMetadata {
                source_version: src.source_version.clone(),
            });
        }
    }

    if let EpisodeDependencyKind::PolicySpilloverOrRetaliation {
        predecessor_episode_id,
        ..
    } = &episode.dependency
        && !registered_episodes.contains_key(predecessor_episode_id)
    {
        return Err(AnalogueError::InvalidEpisodeDependency {
            episode_id: episode.episode_id.clone(),
            reason: "predecessor_episode_id must already be registered in PrecedentLibrary",
        });
    }

    Ok(())
}

fn validate_branches_and_indicators(
    branches: &[ConditionalResponseBranch],
    indicators: &[MonitoringIndicator],
    active_episode_versions: &BTreeSet<&VersionId>,
) -> Result<(), AnalogueError> {
    if branches.is_empty() || branches.len() > MAX_SCENARIO_BRANCHES {
        return Err(AnalogueError::BudgetExceeded {
            field: "branches",
            actual: branches.len(),
            limit: MAX_SCENARIO_BRANCHES,
        });
    }
    if indicators.is_empty() || indicators.len() > MAX_MONITORING_INDICATORS {
        return Err(AnalogueError::BudgetExceeded {
            field: "indicators",
            actual: indicators.len(),
            limit: MAX_MONITORING_INDICATORS,
        });
    }

    let mut branch_ids = BTreeSet::new();
    for branch in branches {
        if !branch_ids.insert(&branch.branch_id) {
            return Err(AnalogueError::DuplicateIdentifier {
                kind: "branch_id",
                identifier: branch.branch_id.as_str().to_owned(),
            });
        }
        validate_text("branch.branch_label", &branch.branch_label)?;
        if branch.steps.is_empty() || branch.steps.len() > MAX_BRANCH_STEPS {
            return Err(AnalogueError::BudgetExceeded {
                field: "branch.steps",
                actual: branch.steps.len(),
                limit: MAX_BRANCH_STEPS,
            });
        }
        let mut expected_product = 1.0_f64;
        for (idx, step) in branch.steps.iter().enumerate() {
            if step.step_number != idx + 1 {
                return Err(AnalogueError::InvalidScenarioModel(
                    "branch steps must have sequential 1-indexed step_number",
                ));
            }
            validate_text("step.action_description", &step.action_description)?;
            validate_text("step.dependency_rationale", &step.dependency_rationale)?;
            if let Some((min_ms, max_ms)) = step.expected_step_window_ms
                && (min_ms < 0 || max_ms < min_ms)
            {
                return Err(AnalogueError::InvalidScenarioModel(
                    "step expected_step_window_ms must satisfy 0 <= min_ms <= max_ms",
                ));
            }
            expected_product *= step.conditional_probability.value();
        }
        let supplied = branch.chained_probability.value();
        if (supplied - expected_product).abs() > CHAINED_PROBABILITY_TOLERANCE {
            return Err(AnalogueError::InconsistentChainedProbability {
                branch_id: branch.branch_id.clone(),
                supplied,
                expected_product,
            });
        }
        if branch.supporting_episode_versions.len() > MAX_FORECAST_COLLECTION_LEN {
            return Err(AnalogueError::BudgetExceeded {
                field: "branch.supporting_episode_versions",
                actual: branch.supporting_episode_versions.len(),
                limit: MAX_FORECAST_COLLECTION_LEN,
            });
        }
        for ver in &branch.supporting_episode_versions {
            if !active_episode_versions.contains(ver) {
                return Err(AnalogueError::InvalidScenarioModel(
                    "branch supporting_episode_versions must reference episodes in the active comparison set",
                ));
            }
        }
    }

    let mut indicator_ids = BTreeSet::new();
    for ind in indicators {
        if !indicator_ids.insert(&ind.indicator_id) {
            return Err(AnalogueError::DuplicateIdentifier {
                kind: "indicator_id",
                identifier: ind.indicator_id.as_str().to_owned(),
            });
        }
        validate_text("indicator.observable_signal", &ind.observable_signal)?;
        validate_text(
            "indicator.source_coverage_limits",
            &ind.source_coverage_limits,
        )?;
        validate_text(
            "indicator.false_positive_caveat",
            &ind.false_positive_caveat,
        )?;
        if !branch_ids.contains(&ind.target_branch_id) {
            return Err(AnalogueError::InvalidScenarioModel(
                "indicator target_branch_id must reference a defined branch in branches",
            ));
        }
    }

    Ok(())
}

fn validate_profile(prefix: &'static str, profile: &AnalogueProfile) -> Result<(), AnalogueError> {
    validate_text(prefix, &profile.geography_region)?;
    validate_text(prefix, &profile.actor_profile)?;
    Ok(())
}

fn validate_text(field: &'static str, value: &str) -> Result<(), AnalogueError> {
    if value.trim().is_empty() {
        return Err(AnalogueError::InvalidTextField {
            field,
            reason: "must not be empty or whitespace-only",
        });
    }
    if value.len() > MAX_TEXT_FIELD_BYTES {
        return Err(AnalogueError::InvalidTextField {
            field,
            reason: "exceeds MAX_TEXT_FIELD_BYTES",
        });
    }
    Ok(())
}

fn validate_ts(field: &'static str, value: i64) -> Result<(), AnalogueError> {
    if !(MIN_VALID_UTC_MS..=MAX_VALID_UTC_MS).contains(&value) {
        return Err(AnalogueError::TimestampOutOfRange { field, value });
    }
    Ok(())
}

fn validate_evidence_refs(
    field: &'static str,
    versions: &[VersionId],
    source_map: &BTreeMap<&VersionId, &CutoffSourceInput>,
) -> Result<(), AnalogueError> {
    if versions.is_empty() || versions.len() > MAX_FORECAST_COLLECTION_LEN {
        return Err(AnalogueError::BudgetExceeded {
            field,
            actual: versions.len(),
            limit: MAX_FORECAST_COLLECTION_LEN,
        });
    }
    let mut seen = BTreeSet::new();
    for v in versions {
        if !seen.insert(v) {
            return Err(AnalogueError::InvalidEpisodeSources(
                "duplicate version reference in evidence list",
            ));
        }
        if !source_map.contains_key(v) {
            return Err(AnalogueError::InvalidEpisodeSources(
                "evidence version not present in episode.sources",
            ));
        }
    }
    Ok(())
}
