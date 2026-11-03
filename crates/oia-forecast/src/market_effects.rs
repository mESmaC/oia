//! Point-in-time market-effects forecasting from news, weather forecast vintages,
//! declassifications, disclosures, and conditional scenarios (`MARKET-03`..=`MARKET-05`, `#44`).
//!
//! This module implements the dependency-independent, in-memory kernel for `#44` and the
//! market-prediction subset of scenario `A14`:
//! - **Catalyst provenance and exposure pathways (`MARKET-03`):** Connects point-in-time
//!   news coverage, weather forecast vintages (refusing post-hoc weather observations
//!   disguised as forecast vintages), declassifications/public releases, corporate or
//!   regulatory disclosures, and macroeconomic release vintages to target market instruments
//!   through cited exposure mechanisms (`ExposurePathwayCitation`).
//! - **Current-condition vs. conditional-scenario separation (`MARKET-03`, `A14`):**
//!   Keeps unconditional `CurrentCondition` forecasts distinct from `ConditionalScenario`
//!   forecasts, and keeps trigger-event occurrence probability `P(E)` strictly separate
//!   from the market response conditional on the event `R | E`. When a scenario derives an
//!   unconditional binary outcome probability across complementary branches (`E` and `¬E`),
//!   it verifies the law of total probability within [`TOTAL_PROBABILITY_TOLERANCE`] and
//!   refuses any attempt to equate `P(E)` with `P(market_outcome)`.
//! - **Three-timestamp awareness, novelty/surprise, sensitivity, and qualitative fallback (`MARKET-04`):**
//!   Distinguishes each catalyst's `historical_event_utc_ms`, `public_release_utc_ms`, and
//!   `market_awareness_utc_ms` against `information_cutoff_utc_ms`; validates prior
//!   expectations/consensus and surprise deltas; records adjustable assumption ranges and
//!   sensitivity points (including sign flips); and requires `QualitativeOrInsufficient`
//!   outputs when quantitative precision is unsupported.
//! - **Leakage detection, vintage/interval/baseline evaluation, and friction-disclosed simulation (`MARKET-05`, `A14`):**
//!   Refuses lookahead leakage from revised economic/weather series vintages, later news or
//!   post-cutoff awareness, post-cutoff instrument/universe selection, and post-cutoff
//!   language-model knowledge under `ProspectiveStrict`. Bridges quantitative vintages into
//!   [`crate::issuance::ForecastLedger`] and [`crate::interval::evaluate_intervals`] while
//!   preserving original vintages, linked revisions, interval misses, and unresolved or
//!   invalidated outcomes.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use oia_domain::{ActorId, OpaqueId, SchemaVersion, VersionId};

use crate::interval::{
    IntervalError, IntervalEvaluation, IntervalForecastRow, IntervalScores, PredictionInterval,
    ScalarOutcome, evaluate_intervals,
};
use crate::issuance::{
    CutoffAdmissibilityMode, CutoffSourceInput, EvaluationRunRecord, ForecastDraft, ForecastLedger,
    IssuanceError, MAX_FORECAST_COLLECTION_LEN, MAX_LEDGER_RECORDS, MAX_TEXT_FIELD_BYTES,
    MAX_VALID_UTC_MS, MIN_VALID_UTC_MS, OutcomeDefinition, RevisionRationale,
    validate_source_admissible_at_cutoff,
};
use crate::scoring::{
    BrierScores, MAX_EVALUATION_ROWS, Probability, ScoringError, UnresolvedReason,
};

/// Maximum number of catalyst records attached to one market-effect forecast vintage.
pub const MAX_CATALYST_INPUTS: usize = 256;

/// Maximum number of cited exposure pathways attached to one market-effect forecast vintage.
pub const MAX_EXPOSURE_PATHWAYS: usize = 128;

/// Maximum number of adjustable assumption parameters on one market-effect forecast vintage.
pub const MAX_ASSUMPTION_PARAMETERS: usize = 64;

/// Maximum number of sensitivity evaluation points on one market-effect forecast vintage.
pub const MAX_SENSITIVITY_POINTS: usize = 64;

/// Maximum number of market-effect vintages, scalar resolutions, or evaluation runs in a book.
pub const MAX_MARKET_EFFECT_RECORDS: usize = MAX_LEDGER_RECORDS;

/// Numerical tolerance for checking total-probability consistency across complementary scenario branches.
pub const TOTAL_PROBABILITY_TOLERANCE: f64 = 1e-9;

/// Numerical tolerance for checking `stated_surprise == realized_release_value - prior_expectation_value`
/// and sensitivity base-case alignment.
pub const SURPRISE_AND_SENSITIVITY_TOLERANCE: f64 = 1e-9;

/// Maximum allowed round-trip friction in basis points (`10,000 bps == 100%`).
pub const MAX_FRICTION_BPS: f64 = 10_000.0;

/// Mandatory disclosure that real-world operational or physical effects do not automatically
/// imply incremental market price effects (`MARKET-04`, `AGENTS.md` §2).
pub const REAL_WORLD_EFFECT_NOT_INCREMENTAL_PRICE_CAVEAT: &str = "Real-world physical, operational, or policy effects do not automatically imply incremental market price effects when prior expectations already priced the event, when exposure is hedged or indirect, or when a publication restates earlier information.";

/// Mandatory disclosure that trigger-event probability and conditional market response are distinct (`MARKET-03`, `A14`).
pub const EVENT_PROBABILITY_DISTINCT_FROM_CONDITIONAL_RESPONSE_CAVEAT: &str = "The probability P(E) that a catalyst or scenario event occurs is distinct from the expected market response conditional on that event (R | E); P(E) must never be presented as the unconditional probability or magnitude of a market move.";

/// Mandatory disclosure paired with every hypothetical strategy simulation (`MARKET-05`, `AGENTS.md` §6).
pub const SIMULATED_STRATEGY_NOT_OBSERVED_RETURN_CAVEAT: &str = "Hypothetical strategy simulations deduct stated fee, half-spread, and slippage assumptions and represent simulated scenario calculations only, never observed market returns.";

/// Mandatory capability boundary note (`MARKET-05`, `AGENTS.md` §6).
pub const ORDER_EXECUTION_NOT_SUPPORTED_CAVEAT: &str = "OIA market modules perform research, scenario analysis, and forecast evaluation only; trade and order execution are not supported.";

/// Catalyst domain driving a market-effect forecast (`MARKET-03`, `A14`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalystDomain {
    /// Multi-source or regional news report.
    NewsCoverage { publisher_id: OpaqueId },
    /// Point-in-time weather forecast vintage issued before the target horizon (`MARKET-03`, `#44`).
    ///
    /// `is_observed_post_hoc_weather` must be `false`; supplying post-hoc weather observations
    /// as if they were point-in-time weather forecasts is rejected as lookahead leakage.
    WeatherForecastVintage {
        weather_model_id: OpaqueId,
        model_run_origin_utc_ms: i64,
        valid_window_start_utc_ms: i64,
        valid_window_end_utc_ms: i64,
        is_observed_post_hoc_weather: bool,
    },
    /// Government declassification or official public-record release (`MARKET-03`, `MARKET-04`).
    DeclassificationOrPublicRelease {
        releasing_authority: String,
        original_record_utc_ms: Option<i64>,
    },
    /// Corporate filing, earnings release, facility notice, or regulatory disclosure (`MARKET-01`, `MARKET-03`).
    CorporateOrRegulatoryDisclosure { filing_or_docket_kind: String },
    /// Point-in-time macroeconomic or commodity inventory release vintage (`MARKET-01`, `MARKET-04`, `MARKET-05`, `A14`).
    MacroeconomicRelease {
        series_id: OpaqueId,
        reference_period_label: String,
        release_vintage_number: u32,
        supersedes_release_version: Option<VersionId>,
    },
}

/// Point-in-time catalyst input with separate historical occurrence, public release, and
/// market awareness timestamps (`MARKET-04`, `MARKET-05`, `A14`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalystInputRecord {
    /// Stable identifier for this catalyst within the forecast.
    pub catalyst_id: OpaqueId,
    /// Immutable source record version backing this catalyst (must appear in `sources`).
    pub source_version: VersionId,
    /// Catalyst domain and domain-specific point-in-time metadata.
    pub domain: CatalystDomain,
    /// Concise summary of the catalyst content.
    pub summary: String,
    /// When the underlying real-world event, measurement period, or model run occurred in UTC ms.
    pub historical_event_utc_ms: i64,
    /// When this specific release vintage, disclosure, or article first became publicly available in UTC ms.
    /// `None` means public release timing is unknown and cannot pass a strict cutoff check.
    pub public_release_utc_ms: Option<i64>,
    /// When market participants could first react to the public release in UTC ms (`public_release_utc_ms <= market_awareness_utc_ms`).
    /// `None` means market awareness timing is unknown and cannot pass a cutoff check.
    pub market_awareness_utc_ms: Option<i64>,
    /// When this exact catalyst version was ingested into OIA in UTC ms.
    pub ingested_at_utc_ms: Option<i64>,
    /// If this record was drawn from a later revised release vintage, records the public release
    /// timestamp of that revision so pre-cutoff forecasts cannot leak post-cutoff revisions (`MARKET-05`, `A14`).
    pub revision_vintage_public_release_utc_ms: Option<i64>,
}

/// Economic or operational channel linking a catalyst to an instrument (`MARKET-02`, `MARKET-03`, `A14`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExposureMechanismKind {
    DirectOperationsOrFacility,
    SupplyChainInputOrLogistics,
    CustomerDemandOrRevenue,
    RegulatoryOrDeclassificationSpillover,
    WeatherOrPhysicalDisruption,
    MacroRateOrCommodityChannel,
}

impl fmt::Display for ExposureMechanismKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DirectOperationsOrFacility => write!(f, "direct_operations_or_facility"),
            Self::SupplyChainInputOrLogistics => write!(f, "supply_chain_input_or_logistics"),
            Self::CustomerDemandOrRevenue => write!(f, "customer_demand_or_revenue"),
            Self::RegulatoryOrDeclassificationSpillover => {
                write!(f, "regulatory_or_declassification_spillover")
            }
            Self::WeatherOrPhysicalDisruption => write!(f, "weather_or_physical_disruption"),
            Self::MacroRateOrCommodityChannel => write!(f, "macro_rate_or_commodity_channel"),
        }
    }
}

/// Qualitative direction of a predicted or conditional market response (`MARKET-03`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketDirection {
    Positive,
    Negative,
    AmbiguousOrTwoSided,
    MutedOrAlreadyPriced,
}

impl fmt::Display for MarketDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Positive => write!(f, "positive"),
            Self::Negative => write!(f, "negative"),
            Self::AmbiguousOrTwoSided => write!(f, "ambiguous_or_two_sided"),
            Self::MutedOrAlreadyPriced => write!(f, "muted_or_already_priced"),
        }
    }
}

/// Cited exposure pathway connecting a catalyst to the target market instrument (`MARKET-02`, `MARKET-03`, `A14`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExposurePathwayCitation {
    pub pathway_id: OpaqueId,
    pub catalyst_id: OpaqueId,
    pub subject_entity_or_facility_id: OpaqueId,
    pub target_instrument_id: OpaqueId,
    pub mechanism_kind: ExposureMechanismKind,
    pub mechanism_explanation: String,
    pub expected_direction: MarketDirection,
    /// Dated evidence versions establishing this exposure relationship; every version must appear
    /// in the forecast's cutoff-admissible `sources`.
    pub cited_evidence_versions: Vec<VersionId>,
}

/// Novelty classification of a catalyst relative to prior market information (`MARKET-04`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoveltyStatus {
    /// Information was not previously published or anticipated by consensus.
    NovelUnanticipated,
    /// Direction or theme was partially anticipated prior to release.
    PartiallyAnticipated,
    /// Scheduled economic or corporate release evaluated against a prior consensus/baseline expectation.
    ScheduledReleaseWithConsensus,
    /// Publication restates or republishes an earlier release (`original_release_version` must be in `sources`).
    RedundantRestatementOfPriorRelease { original_release_version: VersionId },
}

/// Quantitative comparison between a prior expectation/consensus and a realized release vintage (`MARKET-04`, `#44`).
#[derive(Debug, Clone, PartialEq)]
pub struct ExpectationSurpriseInput {
    pub metric_unit: OpaqueId,
    pub prior_expectation_value: f64,
    pub expectation_source_version: VersionId,
    pub realized_release_value: f64,
    pub realized_source_version: VersionId,
    /// Must equal `realized_release_value - prior_expectation_value` within [`SURPRISE_AND_SENSITIVITY_TOLERANCE`].
    pub stated_surprise: f64,
}

/// Validated novelty, prior-expectation surprise, and real-world vs. incremental price disclosure (`MARKET-04`).
#[derive(Debug, Clone, PartialEq)]
pub struct ExpectationAndNoveltyAssessment {
    novelty_status: NoveltyStatus,
    expectation_surprise: Option<ExpectationSurpriseInput>,
    real_world_vs_price_effect_rationale: String,
}

impl ExpectationAndNoveltyAssessment {
    #[must_use]
    pub fn novelty_status(&self) -> &NoveltyStatus {
        &self.novelty_status
    }

    #[must_use]
    pub fn expectation_surprise(&self) -> Option<&ExpectationSurpriseInput> {
        self.expectation_surprise.as_ref()
    }

    #[must_use]
    pub fn real_world_vs_price_effect_rationale(&self) -> &str {
        &self.real_world_vs_price_effect_rationale
    }

    #[must_use]
    pub const fn real_world_vs_incremental_price_caveat(&self) -> &'static str {
        REAL_WORLD_EFFECT_NOT_INCREMENTAL_PRICE_CAVEAT
    }
}

/// Adjustable scenario assumption parameter with low, base, and high bounds (`MARKET-04`, `#44`).
#[derive(Debug, Clone, PartialEq)]
pub struct ScenarioAssumptionParameter {
    pub parameter_id: OpaqueId,
    pub label: String,
    pub unit: OpaqueId,
    pub low_value: f64,
    pub base_value: f64,
    pub high_value: f64,
    pub rationale: String,
}

/// Sensitivity of the predicted scalar market effect to an adjustable assumption parameter (`MARKET-04`).
#[derive(Debug, Clone, PartialEq)]
pub struct AssumptionSensitivityPoint {
    pub parameter_id: OpaqueId,
    pub low_case_effect: f64,
    pub base_case_effect: f64,
    pub high_case_effect: f64,
    /// True when `low_case_effect` and `high_case_effect` have opposite strict signs.
    pub direction_flips_across_range: bool,
    pub sensitivity_note: String,
}

/// Conditional market response for one branch (`R | E` or `R | ¬E`) of a conditional scenario (`MARKET-03`, `A14`).
#[derive(Debug, Clone, PartialEq)]
pub struct ConditionalBranchResponse {
    pub branch_label: String,
    pub expected_direction: MarketDirection,
    /// Probability of the defined binary market outcome conditional on this branch (`P(M | branch)`).
    pub conditional_outcome_probability: Probability,
    /// Conditional scalar point estimate in the forecast's declared `effect_unit`.
    pub conditional_point_estimate: f64,
    /// Conditional central prediction interval in the forecast's declared `effect_unit`.
    pub conditional_interval: PredictionInterval,
}

/// Validated conditional scenario separating event occurrence probability `P(E)` from
/// conditional market response `R | E` (`MARKET-03`, `A14`).
#[derive(Debug, Clone, PartialEq)]
pub struct ConditionalMarketScenario {
    scenario_id: OpaqueId,
    trigger_event_id: OpaqueId,
    trigger_proposition: String,
    /// Probability that the initiating scenario event occurs (`P(E)`), kept distinct from `response_given_event`.
    trigger_occurrence_probability: Probability,
    /// Market response conditional on the trigger event occurring (`R | E`).
    response_given_event: ConditionalBranchResponse,
    /// Optional market response conditional on the trigger event not occurring (`R | ¬E`).
    /// Required whenever a conditional scenario also issues an unconditional binary market forecast.
    response_given_non_event: Option<ConditionalBranchResponse>,
    /// Implied unconditional market-outcome probability `P(E)*P(M|E) + (1-P(E))*P(M|¬E)` when both branches exist.
    implied_unconditional_outcome_probability: Option<Probability>,
}

impl ConditionalMarketScenario {
    #[must_use]
    pub fn scenario_id(&self) -> &OpaqueId {
        &self.scenario_id
    }

    #[must_use]
    pub fn trigger_event_id(&self) -> &OpaqueId {
        &self.trigger_event_id
    }

    #[must_use]
    pub fn trigger_proposition(&self) -> &str {
        &self.trigger_proposition
    }

    #[must_use]
    pub fn trigger_occurrence_probability(&self) -> Probability {
        self.trigger_occurrence_probability
    }

    #[must_use]
    pub fn response_given_event(&self) -> &ConditionalBranchResponse {
        &self.response_given_event
    }

    #[must_use]
    pub fn response_given_non_event(&self) -> Option<&ConditionalBranchResponse> {
        self.response_given_non_event.as_ref()
    }

    #[must_use]
    pub fn implied_unconditional_outcome_probability(&self) -> Option<Probability> {
        self.implied_unconditional_outcome_probability
    }

    #[must_use]
    pub const fn separation_caveat(&self) -> &'static str {
        EVENT_PROBABILITY_DISTINCT_FROM_CONDITIONAL_RESPONSE_CAVEAT
    }
}

/// Input specification for a [`ConditionalMarketScenario`].
#[derive(Debug, Clone, PartialEq)]
pub struct ConditionalMarketScenarioSpec {
    pub scenario_id: OpaqueId,
    pub trigger_event_id: OpaqueId,
    pub trigger_proposition: String,
    pub trigger_occurrence_probability: Probability,
    pub response_given_event: ConditionalBranchResponse,
    pub response_given_non_event: Option<ConditionalBranchResponse>,
}

/// Distinction between unconditional current-condition forecasts and conditional scenarios (`MARKET-03`).
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum MarketForecastMode {
    /// Unconditional forecast given admissible information at `information_cutoff_utc_ms`.
    CurrentCondition,
    /// Conditional scenario separating `P(E)` from `R | E` (and optional `R | ¬E`).
    ConditionalScenario(ConditionalMarketScenario),
}

/// Draft mode supplied by the caller prior to validation.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum MarketForecastModeSpec {
    CurrentCondition,
    ConditionalScenario(ConditionalMarketScenarioSpec),
}

/// Whether available evidence supports quantitative interval and probability precision (`MARKET-04`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrecisionSupport {
    /// Available point-in-time evidence and historical sample support quantitative ranges/probabilities.
    SupportedByAvailableEvidence { rationale: String },
    /// Quantitative precision is unsupported; forecast must emit a qualitative or insufficient-evidence output.
    UnsupportedPrecision { reason: String },
}

/// Qualitative disposition when quantitative precision is unsupported (`MARKET-04`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualitativeDisposition {
    /// Evidence supports a directional sign assessment only, without numeric intervals.
    DirectionalOnly { direction: MarketDirection },
    /// Evidence indicates heightened two-sided volatility/dispersion without directional precision.
    VolatilityExpansionOnly,
    /// Available evidence is insufficient to establish even a directional market effect.
    InsufficientEvidence,
}

/// Qualitative or insufficient-evidence output (`MARKET-04`: "Unsupported precision must yield
/// insufficient-evidence or qualitative outputs").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualitativeMarketOutput {
    pub disposition: QualitativeDisposition,
    pub explanation: String,
    pub missing_or_limiting_evidence: String,
}

/// Quantitative market-effect prediction with point estimate, central prediction interval,
/// matched baseline interval, and defined binary outcome probability (`MARKET-03`..=`MARKET-05`, `A14`).
#[derive(Debug, Clone, PartialEq)]
pub struct QuantitativeMarketPrediction {
    pub effect_unit: OpaqueId,
    pub expected_direction: MarketDirection,
    pub point_estimate: f64,
    pub prediction_interval: PredictionInterval,
    pub baseline_point_estimate: f64,
    pub baseline_interval: PredictionInterval,
    pub outcome_definition: OutcomeDefinition,
    pub forecast_probability: Probability,
    pub baseline_probability: Probability,
}

/// Validated market-effect prediction payload (`Quantitative` vs `QualitativeOrInsufficient`).
#[derive(Debug, Clone, PartialEq)]
pub enum MarketEffectPrediction {
    Quantitative(QuantitativeMarketPrediction),
    QualitativeOrInsufficient(QualitativeMarketOutput),
}

/// Explicit classification of lookahead or point-in-time leakage refused by the kernel (`MARKET-05`, `A14`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarketLeakageKind {
    /// Catalyst was a post-hoc weather observation rather than a point-in-time weather forecast vintage.
    ObservedWeatherUsedInsteadOfForecastVintage { catalyst_id: OpaqueId },
    /// Weather forecast model run origin was after `information_cutoff_utc_ms` or after its public release.
    WeatherModelRunAfterCutoffOrRelease {
        catalyst_id: OpaqueId,
        model_run_origin_utc_ms: i64,
        bound_utc_ms: i64,
    },
    /// Catalyst's `public_release_utc_ms` was unknown (`None`) or after `information_cutoff_utc_ms`
    /// even if `historical_event_utc_ms <= information_cutoff_utc_ms` (`A14`: "news published after an event").
    LaterNewsOrReleaseAfterEventCutoff {
        catalyst_id: OpaqueId,
        historical_event_utc_ms: i64,
        public_release_utc_ms: Option<i64>,
        cutoff_utc_ms: i64,
    },
    /// Catalyst's `market_awareness_utc_ms` was unknown (`None`) or after `information_cutoff_utc_ms`.
    MarketAwarenessAfterCutoff {
        catalyst_id: OpaqueId,
        market_awareness_utc_ms: Option<i64>,
        cutoff_utc_ms: i64,
    },
    /// Catalyst cited a revised economic or corporate release vintage published after `information_cutoff_utc_ms`
    /// (`A14`: "a revised economic release", `MARKET-05`: "revised series").
    RevisedReleaseVintageAfterCutoff {
        catalyst_id: OpaqueId,
        revision_public_release_utc_ms: i64,
        cutoff_utc_ms: i64,
    },
    /// Target instrument or universe membership was selected as of a timestamp after `information_cutoff_utc_ms`
    /// (`MARKET-05`: "Detect leakage from ... instrument selection").
    InstrumentSelectionAfterCutoff {
        target_instrument_id: OpaqueId,
        instrument_selection_as_of_utc_ms: i64,
        cutoff_utc_ms: i64,
    },
}

impl fmt::Display for MarketLeakageKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ObservedWeatherUsedInsteadOfForecastVintage { catalyst_id } => write!(
                f,
                "catalyst {catalyst_id} uses post-hoc observed weather instead of a point-in-time weather forecast vintage"
            ),
            Self::WeatherModelRunAfterCutoffOrRelease {
                catalyst_id,
                model_run_origin_utc_ms,
                bound_utc_ms,
            } => write!(
                f,
                "weather catalyst {catalyst_id} model run origin {model_run_origin_utc_ms} exceeds release or cutoff bound {bound_utc_ms}"
            ),
            Self::LaterNewsOrReleaseAfterEventCutoff {
                catalyst_id,
                historical_event_utc_ms,
                public_release_utc_ms,
                cutoff_utc_ms,
            } => write!(
                f,
                "catalyst {catalyst_id} (historical event {historical_event_utc_ms}) public release {public_release_utc_ms:?} is unknown or after cutoff {cutoff_utc_ms}"
            ),
            Self::MarketAwarenessAfterCutoff {
                catalyst_id,
                market_awareness_utc_ms,
                cutoff_utc_ms,
            } => write!(
                f,
                "catalyst {catalyst_id} market awareness {market_awareness_utc_ms:?} is unknown or after cutoff {cutoff_utc_ms}"
            ),
            Self::RevisedReleaseVintageAfterCutoff {
                catalyst_id,
                revision_public_release_utc_ms,
                cutoff_utc_ms,
            } => write!(
                f,
                "catalyst {catalyst_id} leaks revised release vintage published at {revision_public_release_utc_ms} after cutoff {cutoff_utc_ms}"
            ),
            Self::InstrumentSelectionAfterCutoff {
                target_instrument_id,
                instrument_selection_as_of_utc_ms,
                cutoff_utc_ms,
            } => write!(
                f,
                "target instrument {target_instrument_id} was selected using universe snapshot at {instrument_selection_as_of_utc_ms} after cutoff {cutoff_utc_ms}"
            ),
        }
    }
}

/// Draft input for issuing an initial or revised market-effect forecast vintage (`MARKET-03`..=`MARKET-05`, `#44`).
#[derive(Debug, Clone, PartialEq)]
pub struct MarketEffectForecastDraft {
    pub forecast_id: OpaqueId,
    pub version_id: VersionId,
    pub schema_version: SchemaVersion,
    pub actor: ActorId,
    pub issued_at_utc_ms: i64,
    pub origin_utc_ms: i64,
    pub information_cutoff_utc_ms: i64,
    pub horizon_start_utc_ms: i64,
    pub horizon_end_utc_ms: i64,
    pub cutoff_mode: CutoffAdmissibilityMode,
    pub target_instrument_id: OpaqueId,
    /// Point-in-time timestamp when `target_instrument_id` identity and universe eligibility were verified (`MARKET-01`, `MARKET-05`).
    pub instrument_selection_as_of_utc_ms: i64,
    pub baseline_version: VersionId,
    pub model_version: VersionId,
    pub model_knowledge_cutoff_utc_ms: Option<i64>,
    pub mode: MarketForecastModeSpec,
    pub catalysts: Vec<CatalystInputRecord>,
    pub exposure_pathways: Vec<ExposurePathwayCitation>,
    pub novelty_status: NoveltyStatus,
    pub expectation_surprise: Option<ExpectationSurpriseInput>,
    pub real_world_vs_price_effect_rationale: String,
    pub assumption_parameters: Vec<ScenarioAssumptionParameter>,
    pub sensitivity_points: Vec<AssumptionSensitivityPoint>,
    pub precision_support: PrecisionSupport,
    pub prediction: MarketEffectPrediction,
    pub sources: Vec<CutoffSourceInput>,
    pub comparison_set: Vec<VersionId>,
    pub predecessor_version: Option<VersionId>,
    pub revision_rationale: Option<RevisionRationale>,
}

/// Immutable issued market-effect forecast vintage (`MARKET-03`..=`MARKET-05`, `A14`).
#[derive(Debug, Clone, PartialEq)]
pub struct MarketEffectVintageRecord {
    forecast_id: OpaqueId,
    version_id: VersionId,
    vintage_number: usize,
    schema_version: SchemaVersion,
    actor: ActorId,
    issued_at_utc_ms: i64,
    origin_utc_ms: i64,
    information_cutoff_utc_ms: i64,
    horizon_start_utc_ms: i64,
    horizon_end_utc_ms: i64,
    cutoff_mode: CutoffAdmissibilityMode,
    target_instrument_id: OpaqueId,
    instrument_selection_as_of_utc_ms: i64,
    baseline_version: VersionId,
    model_version: VersionId,
    model_knowledge_cutoff_utc_ms: Option<i64>,
    post_cutoff_model_knowledge_disclosed: bool,
    mode: MarketForecastMode,
    catalysts: Vec<CatalystInputRecord>,
    exposure_pathways: Vec<ExposurePathwayCitation>,
    expectation_and_novelty: ExpectationAndNoveltyAssessment,
    assumption_parameters: Vec<ScenarioAssumptionParameter>,
    sensitivity_points: Vec<AssumptionSensitivityPoint>,
    precision_support: PrecisionSupport,
    prediction: MarketEffectPrediction,
    sources: Vec<CutoffSourceInput>,
    comparison_set: Vec<VersionId>,
    predecessor_version: Option<VersionId>,
    revision_rationale: Option<RevisionRationale>,
}

impl MarketEffectVintageRecord {
    #[must_use]
    pub fn forecast_id(&self) -> &OpaqueId {
        &self.forecast_id
    }

    #[must_use]
    pub fn version_id(&self) -> &VersionId {
        &self.version_id
    }

    #[must_use]
    pub fn vintage_number(&self) -> usize {
        self.vintage_number
    }

    #[must_use]
    pub fn schema_version(&self) -> SchemaVersion {
        self.schema_version
    }

    #[must_use]
    pub fn actor(&self) -> &ActorId {
        &self.actor
    }

    #[must_use]
    pub fn issued_at_utc_ms(&self) -> i64 {
        self.issued_at_utc_ms
    }

    #[must_use]
    pub fn origin_utc_ms(&self) -> i64 {
        self.origin_utc_ms
    }

    #[must_use]
    pub fn information_cutoff_utc_ms(&self) -> i64 {
        self.information_cutoff_utc_ms
    }

    #[must_use]
    pub fn horizon_start_utc_ms(&self) -> i64 {
        self.horizon_start_utc_ms
    }

    #[must_use]
    pub fn horizon_end_utc_ms(&self) -> i64 {
        self.horizon_end_utc_ms
    }

    #[must_use]
    pub fn cutoff_mode(&self) -> &CutoffAdmissibilityMode {
        &self.cutoff_mode
    }

    #[must_use]
    pub fn target_instrument_id(&self) -> &OpaqueId {
        &self.target_instrument_id
    }

    #[must_use]
    pub fn instrument_selection_as_of_utc_ms(&self) -> i64 {
        self.instrument_selection_as_of_utc_ms
    }

    #[must_use]
    pub fn baseline_version(&self) -> &VersionId {
        &self.baseline_version
    }

    #[must_use]
    pub fn model_version(&self) -> &VersionId {
        &self.model_version
    }

    #[must_use]
    pub fn model_knowledge_cutoff_utc_ms(&self) -> Option<i64> {
        self.model_knowledge_cutoff_utc_ms
    }

    #[must_use]
    pub fn post_cutoff_model_knowledge_disclosed(&self) -> bool {
        self.post_cutoff_model_knowledge_disclosed
    }

    #[must_use]
    pub fn mode(&self) -> &MarketForecastMode {
        &self.mode
    }

    #[must_use]
    pub fn catalysts(&self) -> &[CatalystInputRecord] {
        &self.catalysts
    }

    #[must_use]
    pub fn exposure_pathways(&self) -> &[ExposurePathwayCitation] {
        &self.exposure_pathways
    }

    #[must_use]
    pub fn expectation_and_novelty(&self) -> &ExpectationAndNoveltyAssessment {
        &self.expectation_and_novelty
    }

    #[must_use]
    pub fn assumption_parameters(&self) -> &[ScenarioAssumptionParameter] {
        &self.assumption_parameters
    }

    #[must_use]
    pub fn sensitivity_points(&self) -> &[AssumptionSensitivityPoint] {
        &self.sensitivity_points
    }

    #[must_use]
    pub fn precision_support(&self) -> &PrecisionSupport {
        &self.precision_support
    }

    #[must_use]
    pub fn prediction(&self) -> &MarketEffectPrediction {
        &self.prediction
    }

    #[must_use]
    pub fn sources(&self) -> &[CutoffSourceInput] {
        &self.sources
    }

    #[must_use]
    pub fn comparison_set(&self) -> &[VersionId] {
        &self.comparison_set
    }

    #[must_use]
    pub fn predecessor_version(&self) -> Option<&VersionId> {
        self.predecessor_version.as_ref()
    }

    #[must_use]
    pub fn revision_rationale(&self) -> Option<&RevisionRationale> {
        self.revision_rationale.as_ref()
    }
}

/// Scalar market outcome state keeping resolved values, unresolved states, and invalidated
/// preconditions distinct (`MARKET-05`, `A14`).
#[derive(Debug, Clone, PartialEq)]
pub enum MarketScalarResolutionState {
    /// Realized scalar market effect (e.g., cumulative abnormal return in bps) in `effect_unit`.
    Resolved {
        realized_value: f64,
        observed_at_utc_ms: i64,
    },
    /// Scalar market effect remains pending, censored, or disputed.
    Unresolved(UnresolvedReason),
    /// Scenario or market preconditions were invalidated (e.g., trading halt / delisting / cancelled release).
    Invalidated { reason: String },
}

/// Independent scalar market-effect resolution record (`MARKET-05`, `A14`).
#[derive(Debug, Clone, PartialEq)]
pub struct MarketScalarResolutionRecord {
    resolution_id: OpaqueId,
    version_id: VersionId,
    target_instrument_id: OpaqueId,
    effect_unit: OpaqueId,
    horizon_start_utc_ms: i64,
    horizon_end_utc_ms: i64,
    actor: ActorId,
    recorded_at_utc_ms: i64,
    evidence_versions: Vec<VersionId>,
    state: MarketScalarResolutionState,
}

impl MarketScalarResolutionRecord {
    #[must_use]
    pub fn resolution_id(&self) -> &OpaqueId {
        &self.resolution_id
    }

    #[must_use]
    pub fn version_id(&self) -> &VersionId {
        &self.version_id
    }

    #[must_use]
    pub fn target_instrument_id(&self) -> &OpaqueId {
        &self.target_instrument_id
    }

    #[must_use]
    pub fn effect_unit(&self) -> &OpaqueId {
        &self.effect_unit
    }

    #[must_use]
    pub fn horizon_start_utc_ms(&self) -> i64 {
        self.horizon_start_utc_ms
    }

    #[must_use]
    pub fn horizon_end_utc_ms(&self) -> i64 {
        self.horizon_end_utc_ms
    }

    #[must_use]
    pub fn actor(&self) -> &ActorId {
        &self.actor
    }

    #[must_use]
    pub fn recorded_at_utc_ms(&self) -> i64 {
        self.recorded_at_utc_ms
    }

    #[must_use]
    pub fn evidence_versions(&self) -> &[VersionId] {
        &self.evidence_versions
    }

    #[must_use]
    pub fn state(&self) -> &MarketScalarResolutionState {
        &self.state
    }
}

/// Input specification for recording a [`MarketScalarResolutionRecord`].
#[derive(Debug, Clone, PartialEq)]
pub struct MarketScalarResolutionDraft {
    pub resolution_id: OpaqueId,
    pub version_id: VersionId,
    pub target_instrument_id: OpaqueId,
    pub effect_unit: OpaqueId,
    pub horizon_start_utc_ms: i64,
    pub horizon_end_utc_ms: i64,
    pub actor: ActorId,
    pub recorded_at_utc_ms: i64,
    pub evidence_versions: Vec<VersionId>,
    pub state: MarketScalarResolutionState,
}

/// Cohort evaluation entry pairing a market-effect vintage with its binary outcome resolution
/// (for quantitative vintages) and scalar market-effect resolution (`MARKET-05`, `A14`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketCohortEvaluationItem {
    pub market_vintage_version: VersionId,
    /// Required for `Quantitative` vintages; must be `None` for `QualitativeOrInsufficient` vintages.
    pub binary_resolution_version: Option<VersionId>,
    /// Required for `Quantitative` vintages; optional for `QualitativeOrInsufficient` vintages.
    pub scalar_resolution_version: Option<VersionId>,
}

/// Explicit record of a prediction-interval miss or directional miss retained in evaluation (`MARKET-05`, `#44`).
#[derive(Debug, Clone, PartialEq)]
pub struct MarketPredictionMissRecord {
    pub forecast_id: OpaqueId,
    pub forecast_version: VersionId,
    pub vintage_number: usize,
    pub realized_value: f64,
    pub interval_lower: f64,
    pub interval_upper: f64,
    /// Distance by which `realized_value` fell outside `[interval_lower, interval_upper]` (`0.0` if covered).
    pub interval_excursion: f64,
    pub interval_missed: bool,
    pub direction_missed: bool,
}

/// Per-vintage summary in a [`MarketEffectEvaluationRecord`] (`MARKET-05`, `A14`).
#[derive(Debug, Clone, PartialEq)]
pub struct EvaluatedMarketVintageSummary {
    pub forecast_id: OpaqueId,
    pub forecast_version: VersionId,
    pub vintage_number: usize,
    pub is_quantitative: bool,
    pub post_cutoff_model_knowledge_disclosed: bool,
    pub brier_scores: Option<BrierScores>,
    pub interval_scores: Option<IntervalScores>,
    pub point_error: Option<f64>,
    pub baseline_point_error: Option<f64>,
    pub direction_matched: Option<bool>,
    pub scalar_resolution_state: Option<MarketScalarResolutionState>,
}

/// Immutable evaluation record over a cohort of market-effect forecast vintages (`MARKET-05`, `A14`).
#[derive(Debug, Clone, PartialEq)]
pub struct MarketEffectEvaluationRecord {
    evaluation_version: VersionId,
    binary_evaluation_version: Option<VersionId>,
    evaluated_at_utc_ms: i64,
    vintages: Vec<EvaluatedMarketVintageSummary>,
    quantitative_count: usize,
    qualitative_or_insufficient_count: usize,
    resolved_scalar_count: usize,
    unresolved_scalar_count: usize,
    invalidated_scalar_count: usize,
    forecast_interval_covered_count: usize,
    baseline_interval_covered_count: usize,
    mean_brier_scores: Option<BrierScores>,
    mean_forecast_interval_score: Option<f64>,
    mean_baseline_interval_score: Option<f64>,
    mean_abs_error: Option<f64>,
    mean_baseline_abs_error: Option<f64>,
    misses: Vec<MarketPredictionMissRecord>,
}

impl MarketEffectEvaluationRecord {
    #[must_use]
    pub fn evaluation_version(&self) -> &VersionId {
        &self.evaluation_version
    }

    #[must_use]
    pub fn binary_evaluation_version(&self) -> Option<&VersionId> {
        self.binary_evaluation_version.as_ref()
    }

    #[must_use]
    pub fn evaluated_at_utc_ms(&self) -> i64 {
        self.evaluated_at_utc_ms
    }

    #[must_use]
    pub fn vintages(&self) -> &[EvaluatedMarketVintageSummary] {
        &self.vintages
    }

    #[must_use]
    pub fn quantitative_count(&self) -> usize {
        self.quantitative_count
    }

    #[must_use]
    pub fn qualitative_or_insufficient_count(&self) -> usize {
        self.qualitative_or_insufficient_count
    }

    #[must_use]
    pub fn resolved_scalar_count(&self) -> usize {
        self.resolved_scalar_count
    }

    #[must_use]
    pub fn unresolved_scalar_count(&self) -> usize {
        self.unresolved_scalar_count
    }

    #[must_use]
    pub fn invalidated_scalar_count(&self) -> usize {
        self.invalidated_scalar_count
    }

    /// Exact integer coverage ratio `(covered_count, resolved_scalar_count)` for forecast intervals.
    #[must_use]
    pub fn forecast_interval_coverage_ratio(&self) -> (usize, usize) {
        (
            self.forecast_interval_covered_count,
            self.resolved_scalar_count,
        )
    }

    /// Exact integer coverage ratio `(covered_count, resolved_scalar_count)` for baseline intervals.
    #[must_use]
    pub fn baseline_interval_coverage_ratio(&self) -> (usize, usize) {
        (
            self.baseline_interval_covered_count,
            self.resolved_scalar_count,
        )
    }

    #[must_use]
    pub fn mean_brier_scores(&self) -> Option<BrierScores> {
        self.mean_brier_scores
    }

    #[must_use]
    pub fn mean_forecast_interval_score(&self) -> Option<f64> {
        self.mean_forecast_interval_score
    }

    #[must_use]
    pub fn mean_baseline_interval_score(&self) -> Option<f64> {
        self.mean_baseline_interval_score
    }

    #[must_use]
    pub fn mean_abs_error(&self) -> Option<f64> {
        self.mean_abs_error
    }

    #[must_use]
    pub fn mean_baseline_abs_error(&self) -> Option<f64> {
        self.mean_baseline_abs_error
    }

    #[must_use]
    pub fn misses(&self) -> &[MarketPredictionMissRecord] {
        &self.misses
    }
}

/// Explicit fee, spread, and slippage assumptions required for any hypothetical strategy simulation (`MARKET-05`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SimulatedFrictionConfig {
    pub commission_bps_per_side: f64,
    pub half_spread_bps_per_side: f64,
    pub slippage_bps_per_side: f64,
}

impl SimulatedFrictionConfig {
    /// Total round-trip friction (`2 * (commission + half_spread + slippage)`) in basis points.
    #[must_use]
    pub fn round_trip_friction_bps(self) -> f64 {
        2.0 * (self.commission_bps_per_side
            + self.half_spread_bps_per_side
            + self.slippage_bps_per_side)
    }
}

/// Validated hypothetical strategy simulation with mandatory friction deductions and non-observation labels (`MARKET-05`, `AGENTS.md` §6).
#[derive(Debug, Clone, PartialEq)]
pub struct SimulatedStrategyEvaluation {
    simulation_version: VersionId,
    forecast_version: VersionId,
    scalar_resolution_version: VersionId,
    friction_config: SimulatedFrictionConfig,
    gross_simulated_return_bps: f64,
    round_trip_friction_deducted_bps: f64,
    net_simulated_return_bps: f64,
}

impl SimulatedStrategyEvaluation {
    #[must_use]
    pub fn simulation_version(&self) -> &VersionId {
        &self.simulation_version
    }

    #[must_use]
    pub fn forecast_version(&self) -> &VersionId {
        &self.forecast_version
    }

    #[must_use]
    pub fn scalar_resolution_version(&self) -> &VersionId {
        &self.scalar_resolution_version
    }

    #[must_use]
    pub fn friction_config(&self) -> SimulatedFrictionConfig {
        self.friction_config
    }

    #[must_use]
    pub fn gross_simulated_return_bps(&self) -> f64 {
        self.gross_simulated_return_bps
    }

    #[must_use]
    pub fn round_trip_friction_deducted_bps(&self) -> f64 {
        self.round_trip_friction_deducted_bps
    }

    #[must_use]
    pub fn net_simulated_return_bps(&self) -> f64 {
        self.net_simulated_return_bps
    }

    /// Hypothetical simulations are never observed market returns (`MARKET-05`, `AGENTS.md` §2).
    #[must_use]
    pub const fn is_observed_market_return(&self) -> bool {
        false
    }

    /// Order execution is never supported (`MARKET-05`, `AGENTS.md` §6).
    #[must_use]
    pub const fn can_execute_orders(&self) -> bool {
        false
    }

    #[must_use]
    pub const fn simulation_caveat(&self) -> &'static str {
        SIMULATED_STRATEGY_NOT_OBSERVED_RETURN_CAVEAT
    }

    #[must_use]
    pub const fn execution_boundary_caveat(&self) -> &'static str {
        ORDER_EXECUTION_NOT_SUPPORTED_CAVEAT
    }
}

/// Typed errors for market-effects forecasting, scenario validation, leakage detection, and evaluation.
#[derive(Debug, Clone, PartialEq)]
pub enum MarketEffectError {
    /// Underlying forecast ledger issuance/evaluation error.
    Issuance(IssuanceError),
    /// Underlying prediction-interval evaluation error.
    Interval(IntervalError),
    /// Underlying binary scoring error.
    Scoring(ScoringError),
    /// Collection or book capacity budget was zero or exceeded bounds.
    BudgetExceeded {
        field: &'static str,
        actual: usize,
        limit: usize,
    },
    /// Required text field was empty, whitespace-only, or exceeded [`MAX_TEXT_FIELD_BYTES`].
    InvalidTextField {
        field: &'static str,
        reason: &'static str,
    },
    /// Timestamp fell outside [`MIN_VALID_UTC_MS`]..=[`MAX_VALID_UTC_MS`] or violated ordering.
    InvalidTimestamp {
        field: &'static str,
        reason: &'static str,
    },
    /// Duplicate identifier or version within a collection or book.
    DuplicateIdentifier { field: &'static str, id: String },
    /// Point-in-time or lookahead leakage detected and refused (`MARKET-04`, `MARKET-05`, `A14`).
    LeakageDetected(MarketLeakageKind),
    /// Catalyst, exposure pathway, or expectation cited a `VersionId` absent from cutoff-admissible `sources`.
    UncitedSourceVersion {
        context: &'static str,
        version_id: VersionId,
    },
    /// Forecast supplied zero catalysts, zero exposure pathways, or an exposure pathway targeting a different instrument or unknown catalyst.
    InvalidExposurePathway(&'static str),
    /// Conditional scenario conflated trigger-event probability `P(E)` with conditional market response `R | E`,
    /// omitted `R | ¬E` when issuing an unconditional probability, or violated the law of total probability (`MARKET-03`, `A14`).
    ConditionalScenarioContractViolation(&'static str),
    /// Prior expectation or surprise calculation contained non-finite values or did not match `realized - prior` (`MARKET-04`).
    InvalidExpectationSurprise(&'static str),
    /// Assumption parameter bounds or sensitivity point values were non-finite, unordered, or misaligned with base estimate (`MARKET-04`).
    InvalidAssumptionOrSensitivity(&'static str),
    /// Caller supplied a quantitative forecast when `PrecisionSupport::UnsupportedPrecision` was declared,
    /// or supplied a qualitative output when `PrecisionSupport::SupportedByAvailableEvidence` was declared (`MARKET-04`).
    PrecisionSupportMismatch(&'static str),
    /// Redundant restatement of a prior release claimed a non-Muted quantitative price shock without novel evidence (`MARKET-04`).
    RedundantReleaseClaimsIncrementalShock { original_release_version: VersionId },
    /// Quantitative prediction contained non-finite point estimates or mismatched interval nominal coverage.
    InvalidQuantitativePrediction(&'static str),
    /// Revision mutated frozen target instrument, horizon, baseline, or mode contract.
    RevisionContractViolation {
        forecast_id: OpaqueId,
        reason: &'static str,
    },
    /// Referenced market-effect vintage or scalar resolution was not found or did not match target/unit/horizon.
    InvalidEvaluationCohort(&'static str),
    /// Cohort included a revised market-effect vintage without all earlier vintages of the same series (`MARKET-05`).
    IncompleteVintageSeriesInCohort {
        forecast_id: OpaqueId,
        missing_vintage: VersionId,
    },
    /// Hypothetical strategy simulation omitted positive transaction frictions or supplied non-finite inputs (`MARKET-05`).
    InvalidStrategySimulation(&'static str),
}

impl fmt::Display for MarketEffectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Issuance(err) => write!(f, "{err}"),
            Self::Interval(err) => write!(f, "{err}"),
            Self::Scoring(err) => write!(f, "{err}"),
            Self::BudgetExceeded {
                field,
                actual,
                limit,
            } => write!(f, "{field} count {actual} exceeds limit {limit}"),
            Self::InvalidTextField { field, reason } => {
                write!(f, "invalid text field {field}: {reason}")
            }
            Self::InvalidTimestamp { field, reason } => {
                write!(f, "invalid timestamp {field}: {reason}")
            }
            Self::DuplicateIdentifier { field, id } => {
                write!(f, "duplicate identifier in {field}: {id}")
            }
            Self::LeakageDetected(kind) => write!(f, "point-in-time leakage refused: {kind}"),
            Self::UncitedSourceVersion {
                context,
                version_id,
            } => write!(
                f,
                "{context} cites version {version_id} not present in admissible forecast sources"
            ),
            Self::InvalidExposurePathway(msg) => write!(f, "invalid exposure pathway: {msg}"),
            Self::ConditionalScenarioContractViolation(msg) => {
                write!(f, "conditional scenario contract violation: {msg}")
            }
            Self::InvalidExpectationSurprise(msg) => {
                write!(f, "invalid expectation/surprise specification: {msg}")
            }
            Self::InvalidAssumptionOrSensitivity(msg) => {
                write!(f, "invalid assumption or sensitivity specification: {msg}")
            }
            Self::PrecisionSupportMismatch(msg) => {
                write!(f, "precision support mismatch: {msg}")
            }
            Self::RedundantReleaseClaimsIncrementalShock {
                original_release_version,
            } => write!(
                f,
                "redundant restatement of {original_release_version} cannot claim a non-muted quantitative incremental price shock"
            ),
            Self::InvalidQuantitativePrediction(msg) => {
                write!(f, "invalid quantitative market prediction: {msg}")
            }
            Self::RevisionContractViolation {
                forecast_id,
                reason,
            } => write!(f, "revision contract violation for {forecast_id}: {reason}"),
            Self::InvalidEvaluationCohort(msg) => write!(f, "invalid evaluation cohort: {msg}"),
            Self::IncompleteVintageSeriesInCohort {
                forecast_id,
                missing_vintage,
            } => write!(
                f,
                "evaluation cohort includes a revision of {forecast_id} without earlier vintage {missing_vintage}"
            ),
            Self::InvalidStrategySimulation(msg) => {
                write!(f, "invalid strategy simulation: {msg}")
            }
        }
    }
}

impl std::error::Error for MarketEffectError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Issuance(err) => Some(err),
            Self::Interval(err) => Some(err),
            Self::Scoring(err) => Some(err),
            _ => None,
        }
    }
}

impl From<IssuanceError> for MarketEffectError {
    fn from(value: IssuanceError) -> Self {
        Self::Issuance(value)
    }
}

impl From<IntervalError> for MarketEffectError {
    fn from(value: IntervalError) -> Self {
        Self::Interval(value)
    }
}

impl From<ScoringError> for MarketEffectError {
    fn from(value: ScoringError) -> Self {
        Self::Scoring(value)
    }
}

/// Bounded in-memory book of market-effect forecast vintages, scalar outcome resolutions,
/// and chronological holdout evaluations (`MARKET-03`..=`MARKET-05`, `A14`, `#44`).
#[derive(Debug, Clone, PartialEq)]
pub struct MarketEffectForecastBook {
    max_records: usize,
    used_versions: BTreeSet<VersionId>,
    vintages_by_version: BTreeMap<VersionId, MarketEffectVintageRecord>,
    series_vintages: BTreeMap<OpaqueId, Vec<VersionId>>,
    scalar_resolutions: BTreeMap<VersionId, MarketScalarResolutionRecord>,
    evaluations_by_version: BTreeMap<VersionId, MarketEffectEvaluationRecord>,
    evaluation_order: Vec<VersionId>,
}

impl MarketEffectForecastBook {
    /// Create a bounded book accepting up to `max_records` entries per family.
    pub fn new(max_records: usize) -> Result<Self, MarketEffectError> {
        if !(1..=MAX_MARKET_EFFECT_RECORDS).contains(&max_records) {
            return Err(MarketEffectError::BudgetExceeded {
                field: "max_records",
                actual: max_records,
                limit: MAX_MARKET_EFFECT_RECORDS,
            });
        }
        Ok(Self {
            max_records,
            used_versions: BTreeSet::new(),
            vintages_by_version: BTreeMap::new(),
            series_vintages: BTreeMap::new(),
            scalar_resolutions: BTreeMap::new(),
            evaluations_by_version: BTreeMap::new(),
            evaluation_order: Vec::new(),
        })
    }

    /// Issue an immutable market-effect forecast vintage (or linked revision) after enforcing
    /// point-in-time cutoff admissibility, leakage checks, exposure citations, conditional-scenario
    /// separation, and sensitivity invariants (`MARKET-03`..=`MARKET-05`, `A14`).
    ///
    /// When `draft.prediction` is `MarketEffectPrediction::Quantitative`, the vintage is also
    /// issued into `ledger` so binary outcome resolutions and reliability reports share the
    /// same immutable version graph.
    pub fn issue_market_effect_forecast(
        &mut self,
        ledger: &mut ForecastLedger,
        draft: MarketEffectForecastDraft,
    ) -> Result<&MarketEffectVintageRecord, MarketEffectError> {
        if self.vintages_by_version.len() >= self.max_records {
            return Err(MarketEffectError::BudgetExceeded {
                field: "market_effect_vintages",
                actual: self.vintages_by_version.len() + 1,
                limit: self.max_records,
            });
        }
        if self.used_versions.contains(&draft.version_id) {
            return Err(MarketEffectError::DuplicateIdentifier {
                field: "version_id",
                id: draft.version_id.as_str().to_owned(),
            });
        }

        validate_timestamp("information_cutoff_utc_ms", draft.information_cutoff_utc_ms)?;
        validate_timestamp("origin_utc_ms", draft.origin_utc_ms)?;
        validate_timestamp("issued_at_utc_ms", draft.issued_at_utc_ms)?;
        validate_timestamp("horizon_start_utc_ms", draft.horizon_start_utc_ms)?;
        validate_timestamp("horizon_end_utc_ms", draft.horizon_end_utc_ms)?;
        validate_timestamp(
            "instrument_selection_as_of_utc_ms",
            draft.instrument_selection_as_of_utc_ms,
        )?;

        if draft.information_cutoff_utc_ms > draft.origin_utc_ms {
            return Err(MarketEffectError::InvalidTimestamp {
                field: "information_cutoff_utc_ms",
                reason: "must be <= origin_utc_ms",
            });
        }
        if draft.origin_utc_ms > draft.issued_at_utc_ms {
            return Err(MarketEffectError::InvalidTimestamp {
                field: "origin_utc_ms",
                reason: "must be <= issued_at_utc_ms",
            });
        }
        if draft.horizon_start_utc_ms >= draft.horizon_end_utc_ms {
            return Err(MarketEffectError::InvalidTimestamp {
                field: "horizon_start_utc_ms",
                reason: "must be < horizon_end_utc_ms",
            });
        }
        if draft.issued_at_utc_ms >= draft.horizon_end_utc_ms {
            return Err(MarketEffectError::InvalidTimestamp {
                field: "issued_at_utc_ms",
                reason: "must precede horizon_end_utc_ms",
            });
        }

        // Detect survivorship / instrument-selection lookahead leakage (MARKET-05).
        if draft.instrument_selection_as_of_utc_ms > draft.information_cutoff_utc_ms {
            return Err(MarketEffectError::LeakageDetected(
                MarketLeakageKind::InstrumentSelectionAfterCutoff {
                    target_instrument_id: draft.target_instrument_id,
                    instrument_selection_as_of_utc_ms: draft.instrument_selection_as_of_utc_ms,
                    cutoff_utc_ms: draft.information_cutoff_utc_ms,
                },
            ));
        }

        // Validate cutoff mode, model knowledge cutoff, and admissible sources.
        if draft.sources.is_empty() {
            return Err(MarketEffectError::Issuance(
                IssuanceError::InvalidSourceList(
                    "forecast must cite at least one admissible source version",
                ),
            ));
        }
        if draft.sources.len() > MAX_FORECAST_COLLECTION_LEN {
            return Err(MarketEffectError::BudgetExceeded {
                field: "sources",
                actual: draft.sources.len(),
                limit: MAX_FORECAST_COLLECTION_LEN,
            });
        }
        if let CutoffAdmissibilityMode::RetrospectiveReconstructed { limitation_note } =
            &draft.cutoff_mode
        {
            validate_text("cutoff_mode.limitation_note", limitation_note)?;
        }
        if let Some(mc) = draft.model_knowledge_cutoff_utc_ms {
            validate_timestamp("model_knowledge_cutoff_utc_ms", mc)?;
        }
        let post_cutoff_model_knowledge_disclosed =
            match (&draft.cutoff_mode, draft.model_knowledge_cutoff_utc_ms) {
                (CutoffAdmissibilityMode::ProspectiveStrict, None) => {
                    return Err(MarketEffectError::Issuance(
                        IssuanceError::ModelKnowledgeAfterStrictCutoff {
                            model_version: draft.model_version,
                            model_knowledge_cutoff_utc_ms: None,
                            cutoff_utc_ms: draft.information_cutoff_utc_ms,
                        },
                    ));
                }
                (CutoffAdmissibilityMode::ProspectiveStrict, Some(mc))
                    if mc > draft.information_cutoff_utc_ms =>
                {
                    return Err(MarketEffectError::Issuance(
                        IssuanceError::ModelKnowledgeAfterStrictCutoff {
                            model_version: draft.model_version,
                            model_knowledge_cutoff_utc_ms: Some(mc),
                            cutoff_utc_ms: draft.information_cutoff_utc_ms,
                        },
                    ));
                }
                (CutoffAdmissibilityMode::RetrospectiveReconstructed { .. }, None) => true,
                (CutoffAdmissibilityMode::RetrospectiveReconstructed { .. }, Some(mc))
                    if mc > draft.information_cutoff_utc_ms =>
                {
                    true
                }
                _ => false,
            };

        let mut source_versions = BTreeSet::new();
        for src in &draft.sources {
            if !source_versions.insert(src.source_version.clone()) {
                return Err(MarketEffectError::DuplicateIdentifier {
                    field: "sources",
                    id: src.source_version.as_str().to_owned(),
                });
            }
            validate_source_admissible_at_cutoff(
                src,
                draft.information_cutoff_utc_ms,
                &draft.cutoff_mode,
            )?;
        }

        // Validate catalysts and three-timestamp awareness/leakage rules (MARKET-03, MARKET-04, MARKET-05, A14).
        if draft.catalysts.is_empty() {
            return Err(MarketEffectError::InvalidExposurePathway(
                "market-effect forecast must include at least one catalyst input",
            ));
        }
        if draft.catalysts.len() > MAX_CATALYST_INPUTS {
            return Err(MarketEffectError::BudgetExceeded {
                field: "catalysts",
                actual: draft.catalysts.len(),
                limit: MAX_CATALYST_INPUTS,
            });
        }
        let mut catalyst_ids = BTreeSet::new();
        for cat in &draft.catalysts {
            if !catalyst_ids.insert(cat.catalyst_id.clone()) {
                return Err(MarketEffectError::DuplicateIdentifier {
                    field: "catalysts.catalyst_id",
                    id: cat.catalyst_id.as_str().to_owned(),
                });
            }
            validate_text("catalysts[].summary", &cat.summary)?;
            if !source_versions.contains(&cat.source_version) {
                return Err(MarketEffectError::UncitedSourceVersion {
                    context: "catalyst.source_version",
                    version_id: cat.source_version.clone(),
                });
            }
            validate_timestamp(
                "catalysts[].historical_event_utc_ms",
                cat.historical_event_utc_ms,
            )?;

            if let Some(rev_pub_ms) = cat.revision_vintage_public_release_utc_ms {
                validate_timestamp(
                    "catalysts[].revision_vintage_public_release_utc_ms",
                    rev_pub_ms,
                )?;
                if rev_pub_ms > draft.information_cutoff_utc_ms {
                    return Err(MarketEffectError::LeakageDetected(
                        MarketLeakageKind::RevisedReleaseVintageAfterCutoff {
                            catalyst_id: cat.catalyst_id.clone(),
                            revision_public_release_utc_ms: rev_pub_ms,
                            cutoff_utc_ms: draft.information_cutoff_utc_ms,
                        },
                    ));
                }
            }

            let Some(pub_ms) = cat.public_release_utc_ms else {
                return Err(MarketEffectError::LeakageDetected(
                    MarketLeakageKind::LaterNewsOrReleaseAfterEventCutoff {
                        catalyst_id: cat.catalyst_id.clone(),
                        historical_event_utc_ms: cat.historical_event_utc_ms,
                        public_release_utc_ms: None,
                        cutoff_utc_ms: draft.information_cutoff_utc_ms,
                    },
                ));
            };
            validate_timestamp("catalysts[].public_release_utc_ms", pub_ms)?;
            if pub_ms > draft.information_cutoff_utc_ms {
                return Err(MarketEffectError::LeakageDetected(
                    MarketLeakageKind::LaterNewsOrReleaseAfterEventCutoff {
                        catalyst_id: cat.catalyst_id.clone(),
                        historical_event_utc_ms: cat.historical_event_utc_ms,
                        public_release_utc_ms: Some(pub_ms),
                        cutoff_utc_ms: draft.information_cutoff_utc_ms,
                    },
                ));
            }
            if cat.historical_event_utc_ms > pub_ms {
                return Err(MarketEffectError::InvalidTimestamp {
                    field: "catalysts[].historical_event_utc_ms",
                    reason: "historical_event_utc_ms cannot exceed public_release_utc_ms",
                });
            }

            let Some(aware_ms) = cat.market_awareness_utc_ms else {
                return Err(MarketEffectError::LeakageDetected(
                    MarketLeakageKind::MarketAwarenessAfterCutoff {
                        catalyst_id: cat.catalyst_id.clone(),
                        market_awareness_utc_ms: None,
                        cutoff_utc_ms: draft.information_cutoff_utc_ms,
                    },
                ));
            };
            validate_timestamp("catalysts[].market_awareness_utc_ms", aware_ms)?;
            if aware_ms < pub_ms {
                return Err(MarketEffectError::InvalidTimestamp {
                    field: "catalysts[].market_awareness_utc_ms",
                    reason: "market_awareness_utc_ms cannot precede public_release_utc_ms",
                });
            }
            if aware_ms > draft.information_cutoff_utc_ms {
                return Err(MarketEffectError::LeakageDetected(
                    MarketLeakageKind::MarketAwarenessAfterCutoff {
                        catalyst_id: cat.catalyst_id.clone(),
                        market_awareness_utc_ms: Some(aware_ms),
                        cutoff_utc_ms: draft.information_cutoff_utc_ms,
                    },
                ));
            }

            if let Some(ing_ms) = cat.ingested_at_utc_ms {
                validate_timestamp("catalysts[].ingested_at_utc_ms", ing_ms)?;
            }
            if matches!(
                draft.cutoff_mode,
                CutoffAdmissibilityMode::ProspectiveStrict
            ) {
                match cat.ingested_at_utc_ms {
                    Some(ing_ms) if ing_ms <= draft.information_cutoff_utc_ms => {}
                    other => {
                        return Err(MarketEffectError::Issuance(
                            IssuanceError::SourceIngestedAfterStrictCutoff {
                                source_version: cat.source_version.clone(),
                                ingested_at_utc_ms: other,
                                cutoff_utc_ms: draft.information_cutoff_utc_ms,
                            },
                        ));
                    }
                }
            }

            match &cat.domain {
                CatalystDomain::NewsCoverage { .. } => {}
                CatalystDomain::WeatherForecastVintage {
                    model_run_origin_utc_ms,
                    valid_window_start_utc_ms,
                    valid_window_end_utc_ms,
                    is_observed_post_hoc_weather,
                    ..
                } => {
                    if *is_observed_post_hoc_weather {
                        return Err(MarketEffectError::LeakageDetected(
                            MarketLeakageKind::ObservedWeatherUsedInsteadOfForecastVintage {
                                catalyst_id: cat.catalyst_id.clone(),
                            },
                        ));
                    }
                    validate_timestamp(
                        "weather.model_run_origin_utc_ms",
                        *model_run_origin_utc_ms,
                    )?;
                    validate_timestamp(
                        "weather.valid_window_start_utc_ms",
                        *valid_window_start_utc_ms,
                    )?;
                    validate_timestamp(
                        "weather.valid_window_end_utc_ms",
                        *valid_window_end_utc_ms,
                    )?;
                    if *valid_window_start_utc_ms >= *valid_window_end_utc_ms {
                        return Err(MarketEffectError::InvalidTimestamp {
                            field: "weather.valid_window_start_utc_ms",
                            reason: "must precede valid_window_end_utc_ms",
                        });
                    }
                    let bound = pub_ms.min(draft.information_cutoff_utc_ms);
                    if *model_run_origin_utc_ms > bound {
                        return Err(MarketEffectError::LeakageDetected(
                            MarketLeakageKind::WeatherModelRunAfterCutoffOrRelease {
                                catalyst_id: cat.catalyst_id.clone(),
                                model_run_origin_utc_ms: *model_run_origin_utc_ms,
                                bound_utc_ms: bound,
                            },
                        ));
                    }
                }
                CatalystDomain::DeclassificationOrPublicRelease {
                    releasing_authority,
                    original_record_utc_ms,
                } => {
                    validate_text("declassification.releasing_authority", releasing_authority)?;
                    if let Some(orig_ms) = original_record_utc_ms {
                        validate_timestamp("declassification.original_record_utc_ms", *orig_ms)?;
                        if *orig_ms > pub_ms {
                            return Err(MarketEffectError::InvalidTimestamp {
                                field: "declassification.original_record_utc_ms",
                                reason: "original_record_utc_ms cannot exceed public_release_utc_ms",
                            });
                        }
                    }
                }
                CatalystDomain::CorporateOrRegulatoryDisclosure {
                    filing_or_docket_kind,
                } => {
                    validate_text("disclosure.filing_or_docket_kind", filing_or_docket_kind)?;
                }
                CatalystDomain::MacroeconomicRelease {
                    reference_period_label,
                    release_vintage_number,
                    ..
                } => {
                    validate_text("macro.reference_period_label", reference_period_label)?;
                    if *release_vintage_number == 0 {
                        return Err(MarketEffectError::InvalidExpectationSurprise(
                            "macroeconomic release_vintage_number must be >= 1",
                        ));
                    }
                }
            }
        }

        // Validate cited exposure pathways (MARKET-02, MARKET-03, A14).
        if draft.exposure_pathways.is_empty() {
            return Err(MarketEffectError::InvalidExposurePathway(
                "market-effect forecast must cite at least one exposure pathway",
            ));
        }
        if draft.exposure_pathways.len() > MAX_EXPOSURE_PATHWAYS {
            return Err(MarketEffectError::BudgetExceeded {
                field: "exposure_pathways",
                actual: draft.exposure_pathways.len(),
                limit: MAX_EXPOSURE_PATHWAYS,
            });
        }
        let mut pathway_ids = BTreeSet::new();
        for pathway in &draft.exposure_pathways {
            if !pathway_ids.insert(pathway.pathway_id.clone()) {
                return Err(MarketEffectError::DuplicateIdentifier {
                    field: "exposure_pathways.pathway_id",
                    id: pathway.pathway_id.as_str().to_owned(),
                });
            }
            if !catalyst_ids.contains(&pathway.catalyst_id) {
                return Err(MarketEffectError::InvalidExposurePathway(
                    "exposure pathway references a catalyst_id not present in draft.catalysts",
                ));
            }
            if pathway.target_instrument_id != draft.target_instrument_id {
                return Err(MarketEffectError::InvalidExposurePathway(
                    "exposure pathway target_instrument_id must match draft.target_instrument_id",
                ));
            }
            validate_text(
                "exposure_pathways[].mechanism_explanation",
                &pathway.mechanism_explanation,
            )?;
            if pathway.cited_evidence_versions.is_empty() {
                return Err(MarketEffectError::InvalidExposurePathway(
                    "exposure pathway must cite at least one dated evidence version",
                ));
            }
            if pathway.cited_evidence_versions.len() > MAX_FORECAST_COLLECTION_LEN {
                return Err(MarketEffectError::BudgetExceeded {
                    field: "exposure_pathways[].cited_evidence_versions",
                    actual: pathway.cited_evidence_versions.len(),
                    limit: MAX_FORECAST_COLLECTION_LEN,
                });
            }
            let mut seen_ev = BTreeSet::new();
            for ev in &pathway.cited_evidence_versions {
                if !seen_ev.insert(ev) {
                    return Err(MarketEffectError::DuplicateIdentifier {
                        field: "exposure_pathways[].cited_evidence_versions",
                        id: ev.as_str().to_owned(),
                    });
                }
                if !source_versions.contains(ev) {
                    return Err(MarketEffectError::UncitedSourceVersion {
                        context: "exposure_pathway.cited_evidence_versions",
                        version_id: ev.clone(),
                    });
                }
            }
        }

        // Validate novelty, prior expectation, surprise, and real-world vs price rationale (MARKET-04).
        validate_text(
            "real_world_vs_price_effect_rationale",
            &draft.real_world_vs_price_effect_rationale,
        )?;
        if let NoveltyStatus::RedundantRestatementOfPriorRelease {
            original_release_version,
        } = &draft.novelty_status
            && !source_versions.contains(original_release_version)
        {
            return Err(MarketEffectError::UncitedSourceVersion {
                context: "novelty_status.original_release_version",
                version_id: original_release_version.clone(),
            });
        }
        if let Some(surprise) = &draft.expectation_surprise {
            if !surprise.prior_expectation_value.is_finite()
                || !surprise.realized_release_value.is_finite()
                || !surprise.stated_surprise.is_finite()
            {
                return Err(MarketEffectError::InvalidExpectationSurprise(
                    "prior expectation, realized release value, and stated surprise must be finite",
                ));
            }
            if !source_versions.contains(&surprise.expectation_source_version) {
                return Err(MarketEffectError::UncitedSourceVersion {
                    context: "expectation_surprise.expectation_source_version",
                    version_id: surprise.expectation_source_version.clone(),
                });
            }
            if !source_versions.contains(&surprise.realized_source_version) {
                return Err(MarketEffectError::UncitedSourceVersion {
                    context: "expectation_surprise.realized_source_version",
                    version_id: surprise.realized_source_version.clone(),
                });
            }
            let expected_delta = surprise.realized_release_value - surprise.prior_expectation_value;
            if !expected_delta.is_finite()
                || (surprise.stated_surprise - expected_delta).abs()
                    > SURPRISE_AND_SENSITIVITY_TOLERANCE
            {
                return Err(MarketEffectError::InvalidExpectationSurprise(
                    "stated_surprise must equal realized_release_value - prior_expectation_value within 1e-9",
                ));
            }
        }
        if matches!(
            draft.novelty_status,
            NoveltyStatus::ScheduledReleaseWithConsensus
        ) && draft.expectation_surprise.is_none()
        {
            return Err(MarketEffectError::InvalidExpectationSurprise(
                "ScheduledReleaseWithConsensus requires expectation_surprise to be supplied",
            ));
        }
        let expectation_and_novelty = ExpectationAndNoveltyAssessment {
            novelty_status: draft.novelty_status.clone(),
            expectation_surprise: draft.expectation_surprise.clone(),
            real_world_vs_price_effect_rationale: draft
                .real_world_vs_price_effect_rationale
                .clone(),
        };

        // Validate adjustable assumptions and sensitivity points (MARKET-04).
        if draft.assumption_parameters.len() > MAX_ASSUMPTION_PARAMETERS {
            return Err(MarketEffectError::BudgetExceeded {
                field: "assumption_parameters",
                actual: draft.assumption_parameters.len(),
                limit: MAX_ASSUMPTION_PARAMETERS,
            });
        }
        let mut param_ids = BTreeSet::new();
        for param in &draft.assumption_parameters {
            if !param_ids.insert(param.parameter_id.clone()) {
                return Err(MarketEffectError::DuplicateIdentifier {
                    field: "assumption_parameters.parameter_id",
                    id: param.parameter_id.as_str().to_owned(),
                });
            }
            validate_text("assumption_parameters[].label", &param.label)?;
            validate_text("assumption_parameters[].rationale", &param.rationale)?;
            if !param.low_value.is_finite()
                || !param.base_value.is_finite()
                || !param.high_value.is_finite()
                || param.low_value > param.base_value
                || param.base_value > param.high_value
                || param.low_value >= param.high_value
            {
                return Err(MarketEffectError::InvalidAssumptionOrSensitivity(
                    "assumption parameter values must be finite with low_value <= base_value <= high_value and low_value < high_value",
                ));
            }
        }

        if draft.sensitivity_points.len() > MAX_SENSITIVITY_POINTS {
            return Err(MarketEffectError::BudgetExceeded {
                field: "sensitivity_points",
                actual: draft.sensitivity_points.len(),
                limit: MAX_SENSITIVITY_POINTS,
            });
        }
        let mut seen_sens_params = BTreeSet::new();
        for sens in &draft.sensitivity_points {
            if !seen_sens_params.insert(sens.parameter_id.clone()) {
                return Err(MarketEffectError::DuplicateIdentifier {
                    field: "sensitivity_points.parameter_id",
                    id: sens.parameter_id.as_str().to_owned(),
                });
            }
            if !param_ids.contains(&sens.parameter_id) {
                return Err(MarketEffectError::InvalidAssumptionOrSensitivity(
                    "sensitivity point references unknown parameter_id",
                ));
            }
            validate_text(
                "sensitivity_points[].sensitivity_note",
                &sens.sensitivity_note,
            )?;
            if !sens.low_case_effect.is_finite()
                || !sens.base_case_effect.is_finite()
                || !sens.high_case_effect.is_finite()
            {
                return Err(MarketEffectError::InvalidAssumptionOrSensitivity(
                    "sensitivity point effects must be finite",
                ));
            }
            let expected_flip = (sens.low_case_effect > 0.0 && sens.high_case_effect < 0.0)
                || (sens.low_case_effect < 0.0 && sens.high_case_effect > 0.0);
            if sens.direction_flips_across_range != expected_flip {
                return Err(MarketEffectError::InvalidAssumptionOrSensitivity(
                    "sensitivity direction_flips_across_range does not match strict sign change between low_case_effect and high_case_effect",
                ));
            }
        }

        // Validate precision support against prediction variant (MARKET-04).
        match (&draft.precision_support, &draft.prediction) {
            (
                PrecisionSupport::SupportedByAvailableEvidence { rationale },
                MarketEffectPrediction::Quantitative(quant),
            ) => {
                validate_text("precision_support.rationale", rationale)?;
                if draft.assumption_parameters.is_empty() || draft.sensitivity_points.is_empty() {
                    return Err(MarketEffectError::InvalidAssumptionOrSensitivity(
                        "quantitative market-effect forecast must declare at least one assumption parameter and sensitivity point",
                    ));
                }
                if !quant.point_estimate.is_finite() || !quant.baseline_point_estimate.is_finite() {
                    return Err(MarketEffectError::InvalidQuantitativePrediction(
                        "point_estimate and baseline_point_estimate must be finite",
                    ));
                }
                if quant.point_estimate < quant.prediction_interval.lower()
                    || quant.point_estimate > quant.prediction_interval.upper()
                {
                    return Err(MarketEffectError::InvalidQuantitativePrediction(
                        "point_estimate must lie within prediction_interval [lower, upper]",
                    ));
                }
                if quant.baseline_point_estimate < quant.baseline_interval.lower()
                    || quant.baseline_point_estimate > quant.baseline_interval.upper()
                {
                    return Err(MarketEffectError::InvalidQuantitativePrediction(
                        "baseline_point_estimate must lie within baseline_interval [lower, upper]",
                    ));
                }
                if quant.prediction_interval.nominal_coverage()
                    != quant.baseline_interval.nominal_coverage()
                {
                    return Err(MarketEffectError::InvalidQuantitativePrediction(
                        "prediction_interval and baseline_interval must share the same nominal_coverage",
                    ));
                }
                for sens in &draft.sensitivity_points {
                    if (sens.base_case_effect - quant.point_estimate).abs()
                        > SURPRISE_AND_SENSITIVITY_TOLERANCE
                    {
                        return Err(MarketEffectError::InvalidAssumptionOrSensitivity(
                            "sensitivity base_case_effect must match quantitative point_estimate within 1e-9",
                        ));
                    }
                }
                if let NoveltyStatus::RedundantRestatementOfPriorRelease {
                    original_release_version,
                } = &draft.novelty_status
                    && (!matches!(
                        quant.expected_direction,
                        MarketDirection::MutedOrAlreadyPriced
                    ) || quant.point_estimate.abs() > SURPRISE_AND_SENSITIVITY_TOLERANCE)
                {
                    return Err(MarketEffectError::RedundantReleaseClaimsIncrementalShock {
                        original_release_version: original_release_version.clone(),
                    });
                }
            }
            (
                PrecisionSupport::UnsupportedPrecision { reason },
                MarketEffectPrediction::QualitativeOrInsufficient(qual),
            ) => {
                validate_text("precision_support.reason", reason)?;
                validate_text("prediction.explanation", &qual.explanation)?;
                validate_text(
                    "prediction.missing_or_limiting_evidence",
                    &qual.missing_or_limiting_evidence,
                )?;
            }
            (
                PrecisionSupport::UnsupportedPrecision { .. },
                MarketEffectPrediction::Quantitative(_),
            ) => {
                return Err(MarketEffectError::PrecisionSupportMismatch(
                    "UnsupportedPrecision cannot emit a Quantitative prediction; use QualitativeOrInsufficient",
                ));
            }
            (
                PrecisionSupport::SupportedByAvailableEvidence { .. },
                MarketEffectPrediction::QualitativeOrInsufficient(_),
            ) => {
                return Err(MarketEffectError::PrecisionSupportMismatch(
                    "QualitativeOrInsufficient prediction requires PrecisionSupport::UnsupportedPrecision",
                ));
            }
        }

        // Validate mode & conditional scenario separation of P(E) vs R | E (MARKET-03, A14).
        let validated_mode = match draft.mode {
            MarketForecastModeSpec::CurrentCondition => MarketForecastMode::CurrentCondition,
            MarketForecastModeSpec::ConditionalScenario(spec) => {
                validate_text("scenario.trigger_proposition", &spec.trigger_proposition)?;
                validate_branch_response(
                    "scenario.response_given_event",
                    &spec.response_given_event,
                )?;
                if let Some(comp) = &spec.response_given_non_event {
                    validate_branch_response("scenario.response_given_non_event", comp)?;
                }

                let implied_prob = if let Some(comp) = &spec.response_given_non_event {
                    let p_e = spec.trigger_occurrence_probability.value();
                    let p_m_given_e = spec
                        .response_given_event
                        .conditional_outcome_probability
                        .value();
                    let p_m_given_not_e = comp.conditional_outcome_probability.value();
                    let total = p_e * p_m_given_e + (1.0 - p_e) * p_m_given_not_e;
                    Some(Probability::new(total.clamp(0.0, 1.0))?)
                } else {
                    None
                };

                // When issuing a Quantitative prediction under a ConditionalScenario, verify that
                // P(E) is not conflated with P(M | E) or unconditional P(M).
                if let MarketEffectPrediction::Quantitative(quant) = &draft.prediction {
                    let Some(implied) = implied_prob else {
                        return Err(MarketEffectError::ConditionalScenarioContractViolation(
                            "quantitative ConditionalScenario forecast must supply response_given_non_event so unconditional forecast_probability follows the law of total probability rather than conflating P(E) with R | E",
                        ));
                    };
                    if (quant.forecast_probability.value() - implied.value()).abs()
                        > TOTAL_PROBABILITY_TOLERANCE
                    {
                        return Err(MarketEffectError::ConditionalScenarioContractViolation(
                            "quantitative forecast_probability does not match P(E)*P(M|E) + (1-P(E))*P(M|not E) within 1e-9",
                        ));
                    }
                    if quant.prediction_interval.nominal_coverage()
                        != spec
                            .response_given_event
                            .conditional_interval
                            .nominal_coverage()
                    {
                        return Err(MarketEffectError::ConditionalScenarioContractViolation(
                            "conditional_interval nominal_coverage must match prediction_interval nominal_coverage",
                        ));
                    }
                }

                MarketForecastMode::ConditionalScenario(ConditionalMarketScenario {
                    scenario_id: spec.scenario_id,
                    trigger_event_id: spec.trigger_event_id,
                    trigger_proposition: spec.trigger_proposition,
                    trigger_occurrence_probability: spec.trigger_occurrence_probability,
                    response_given_event: spec.response_given_event,
                    response_given_non_event: spec.response_given_non_event,
                    implied_unconditional_outcome_probability: implied_prob,
                })
            }
        };

        // Validate comparison_set uniqueness.
        if draft.comparison_set.len() > MAX_FORECAST_COLLECTION_LEN {
            return Err(MarketEffectError::BudgetExceeded {
                field: "comparison_set",
                actual: draft.comparison_set.len(),
                limit: MAX_FORECAST_COLLECTION_LEN,
            });
        }
        let mut seen_comp = BTreeSet::new();
        for comp in &draft.comparison_set {
            if !seen_comp.insert(comp) {
                return Err(MarketEffectError::DuplicateIdentifier {
                    field: "comparison_set",
                    id: comp.as_str().to_owned(),
                });
            }
        }

        // Validate initial vs revision linkage in MarketEffectForecastBook.
        let vintage_number = match (&draft.predecessor_version, &draft.revision_rationale) {
            (None, None) => {
                if self.series_vintages.contains_key(&draft.forecast_id) {
                    return Err(MarketEffectError::DuplicateIdentifier {
                        field: "forecast_id",
                        id: draft.forecast_id.as_str().to_owned(),
                    });
                }
                1
            }
            (Some(_), None) | (None, Some(_)) => {
                return Err(MarketEffectError::RevisionContractViolation {
                    forecast_id: draft.forecast_id,
                    reason: "predecessor_version and revision_rationale must both be Some or both be None",
                });
            }
            (Some(pred_ver), Some(rationale)) => {
                let series = self
                    .series_vintages
                    .get(&draft.forecast_id)
                    .ok_or_else(|| MarketEffectError::RevisionContractViolation {
                        forecast_id: draft.forecast_id.clone(),
                        reason: "unknown forecast_id for revision",
                    })?;
                if series.last() != Some(pred_ver) {
                    return Err(MarketEffectError::RevisionContractViolation {
                        forecast_id: draft.forecast_id,
                        reason: "predecessor_version must reference the latest vintage in the series",
                    });
                }
                let pred = self.vintages_by_version.get(pred_ver).ok_or_else(|| {
                    MarketEffectError::RevisionContractViolation {
                        forecast_id: draft.forecast_id.clone(),
                        reason: "predecessor vintage not found",
                    }
                })?;
                if draft.target_instrument_id != pred.target_instrument_id {
                    return Err(MarketEffectError::RevisionContractViolation {
                        forecast_id: draft.forecast_id,
                        reason: "revision cannot change target_instrument_id",
                    });
                }
                if draft.horizon_start_utc_ms != pred.horizon_start_utc_ms
                    || draft.horizon_end_utc_ms != pred.horizon_end_utc_ms
                {
                    return Err(MarketEffectError::RevisionContractViolation {
                        forecast_id: draft.forecast_id,
                        reason: "revision cannot change target horizon",
                    });
                }
                if draft.baseline_version != pred.baseline_version {
                    return Err(MarketEffectError::RevisionContractViolation {
                        forecast_id: draft.forecast_id,
                        reason: "revision cannot change baseline_version",
                    });
                }
                if matches!(pred.cutoff_mode, CutoffAdmissibilityMode::ProspectiveStrict)
                    && matches!(
                        draft.cutoff_mode,
                        CutoffAdmissibilityMode::RetrospectiveReconstructed { .. }
                    )
                {
                    return Err(MarketEffectError::RevisionContractViolation {
                        forecast_id: draft.forecast_id,
                        reason: "revision cannot downgrade ProspectiveStrict to RetrospectiveReconstructed",
                    });
                }
                if draft.information_cutoff_utc_ms < pred.information_cutoff_utc_ms
                    || draft.origin_utc_ms < pred.origin_utc_ms
                    || draft.issued_at_utc_ms < pred.issued_at_utc_ms
                {
                    return Err(MarketEffectError::RevisionContractViolation {
                        forecast_id: draft.forecast_id,
                        reason: "revision timestamps must be monotonic relative to predecessor",
                    });
                }
                validate_text("revision_rationale.summary", &rationale.summary)?;
                if rationale.new_evidence_versions.is_empty() {
                    return Err(MarketEffectError::RevisionContractViolation {
                        forecast_id: draft.forecast_id,
                        reason: "revision_rationale.new_evidence_versions must not be empty",
                    });
                }
                let pred_sources: BTreeSet<&VersionId> =
                    pred.sources.iter().map(|s| &s.source_version).collect();
                let mut seen_new_ev = BTreeSet::new();
                for ev in &rationale.new_evidence_versions {
                    if !seen_new_ev.insert(ev) {
                        return Err(MarketEffectError::DuplicateIdentifier {
                            field: "revision_rationale.new_evidence_versions",
                            id: ev.as_str().to_owned(),
                        });
                    }
                    if !source_versions.contains(ev) {
                        return Err(MarketEffectError::UncitedSourceVersion {
                            context: "revision_rationale.new_evidence_versions",
                            version_id: ev.clone(),
                        });
                    }
                    if pred_sources.contains(ev) {
                        return Err(MarketEffectError::RevisionContractViolation {
                            forecast_id: draft.forecast_id,
                            reason: "revision_rationale cites evidence version already present in predecessor",
                        });
                    }
                }
                series.len() + 1
            }
        };

        // When prediction is Quantitative, bridge into ForecastLedger so binary scoring,
        // outcome resolution, and reliability reports track this exact vintage (MARKET-05, A14).
        if let MarketEffectPrediction::Quantitative(quant) = &draft.prediction {
            let mut assumptions_text: Vec<String> = draft
                .assumption_parameters
                .iter()
                .map(|p| {
                    format!(
                        "{} ({})={:.6} [range {:.6}..={:.6}]: {}",
                        p.parameter_id,
                        p.unit,
                        p.base_value,
                        p.low_value,
                        p.high_value,
                        p.rationale
                    )
                })
                .collect();
            if assumptions_text.is_empty() {
                assumptions_text.push(draft.real_world_vs_price_effect_rationale.clone());
            }
            let ledger_draft = ForecastDraft {
                forecast_id: draft.forecast_id.clone(),
                version_id: draft.version_id.clone(),
                schema_version: draft.schema_version,
                actor: draft.actor.clone(),
                issued_at_utc_ms: draft.issued_at_utc_ms,
                origin_utc_ms: draft.origin_utc_ms,
                information_cutoff_utc_ms: draft.information_cutoff_utc_ms,
                horizon_start_utc_ms: draft.horizon_start_utc_ms,
                horizon_end_utc_ms: draft.horizon_end_utc_ms,
                cutoff_mode: draft.cutoff_mode.clone(),
                target_id: draft.target_instrument_id.clone(),
                outcome_definition: quant.outcome_definition.clone(),
                baseline_version: draft.baseline_version.clone(),
                baseline_probability: quant.baseline_probability,
                forecast_probability: quant.forecast_probability,
                model_version: draft.model_version.clone(),
                model_knowledge_cutoff_utc_ms: draft.model_knowledge_cutoff_utc_ms,
                sources: draft.sources.clone(),
                comparison_set: draft.comparison_set.clone(),
                assumptions: assumptions_text,
                predecessor_version: draft.predecessor_version.clone(),
                revision_rationale: draft.revision_rationale.clone(),
            };
            ledger.issue_forecast(ledger_draft)?;
        }

        let version_key = draft.version_id.clone();
        let record = MarketEffectVintageRecord {
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
            target_instrument_id: draft.target_instrument_id,
            instrument_selection_as_of_utc_ms: draft.instrument_selection_as_of_utc_ms,
            baseline_version: draft.baseline_version,
            model_version: draft.model_version,
            model_knowledge_cutoff_utc_ms: draft.model_knowledge_cutoff_utc_ms,
            post_cutoff_model_knowledge_disclosed,
            mode: validated_mode,
            catalysts: draft.catalysts,
            exposure_pathways: draft.exposure_pathways,
            expectation_and_novelty,
            assumption_parameters: draft.assumption_parameters,
            sensitivity_points: draft.sensitivity_points,
            precision_support: draft.precision_support,
            prediction: draft.prediction,
            sources: draft.sources,
            comparison_set: draft.comparison_set,
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
            MarketEffectError::InvalidEvaluationCohort("failed to retrieve newly issued vintage"),
        )
    }

    /// Record an independent scalar market-effect resolution version (`MARKET-05`, `A14`).
    pub fn record_scalar_resolution(
        &mut self,
        draft: MarketScalarResolutionDraft,
    ) -> Result<&MarketScalarResolutionRecord, MarketEffectError> {
        if self.scalar_resolutions.len() >= self.max_records {
            return Err(MarketEffectError::BudgetExceeded {
                field: "scalar_resolutions",
                actual: self.scalar_resolutions.len() + 1,
                limit: self.max_records,
            });
        }
        if self.used_versions.contains(&draft.version_id) {
            return Err(MarketEffectError::DuplicateIdentifier {
                field: "scalar_resolution.version_id",
                id: draft.version_id.as_str().to_owned(),
            });
        }
        validate_timestamp("horizon_start_utc_ms", draft.horizon_start_utc_ms)?;
        validate_timestamp("horizon_end_utc_ms", draft.horizon_end_utc_ms)?;
        validate_timestamp("recorded_at_utc_ms", draft.recorded_at_utc_ms)?;
        if draft.horizon_start_utc_ms >= draft.horizon_end_utc_ms {
            return Err(MarketEffectError::InvalidTimestamp {
                field: "horizon_start_utc_ms",
                reason: "must be < horizon_end_utc_ms",
            });
        }
        if draft.evidence_versions.len() > MAX_FORECAST_COLLECTION_LEN {
            return Err(MarketEffectError::BudgetExceeded {
                field: "scalar_resolution.evidence_versions",
                actual: draft.evidence_versions.len(),
                limit: MAX_FORECAST_COLLECTION_LEN,
            });
        }
        let mut seen_ev = BTreeSet::new();
        for ev in &draft.evidence_versions {
            if !seen_ev.insert(ev) {
                return Err(MarketEffectError::DuplicateIdentifier {
                    field: "scalar_resolution.evidence_versions",
                    id: ev.as_str().to_owned(),
                });
            }
        }

        match &draft.state {
            MarketScalarResolutionState::Resolved {
                realized_value,
                observed_at_utc_ms,
            } => {
                if !realized_value.is_finite() {
                    return Err(MarketEffectError::InvalidEvaluationCohort(
                        "resolved scalar realized_value must be finite",
                    ));
                }
                validate_timestamp("observed_at_utc_ms", *observed_at_utc_ms)?;
                if *observed_at_utc_ms < draft.horizon_start_utc_ms
                    || *observed_at_utc_ms > draft.horizon_end_utc_ms
                {
                    return Err(MarketEffectError::InvalidTimestamp {
                        field: "observed_at_utc_ms",
                        reason: "resolved scalar observation must fall within [horizon_start_utc_ms, horizon_end_utc_ms]",
                    });
                }
                if draft.recorded_at_utc_ms < *observed_at_utc_ms {
                    return Err(MarketEffectError::InvalidTimestamp {
                        field: "recorded_at_utc_ms",
                        reason: "recorded_at_utc_ms cannot precede observed_at_utc_ms",
                    });
                }
                if draft.evidence_versions.is_empty() {
                    return Err(MarketEffectError::InvalidEvaluationCohort(
                        "resolved scalar outcome must cite at least one evidence version",
                    ));
                }
            }
            MarketScalarResolutionState::Unresolved(UnresolvedReason::Disputed) => {
                if draft.evidence_versions.is_empty() {
                    return Err(MarketEffectError::InvalidEvaluationCohort(
                        "disputed scalar outcome must cite at least one evidence version",
                    ));
                }
            }
            MarketScalarResolutionState::Unresolved(
                UnresolvedReason::Pending | UnresolvedReason::Censored,
            ) => {}
            MarketScalarResolutionState::Invalidated { reason } => {
                validate_text("scalar_resolution.invalidated.reason", reason)?;
                if draft.evidence_versions.is_empty() {
                    return Err(MarketEffectError::InvalidEvaluationCohort(
                        "invalidated scalar outcome must cite at least one evidence version",
                    ));
                }
            }
        }

        let ver_key = draft.version_id.clone();
        let record = MarketScalarResolutionRecord {
            resolution_id: draft.resolution_id,
            version_id: draft.version_id,
            target_instrument_id: draft.target_instrument_id,
            effect_unit: draft.effect_unit,
            horizon_start_utc_ms: draft.horizon_start_utc_ms,
            horizon_end_utc_ms: draft.horizon_end_utc_ms,
            actor: draft.actor,
            recorded_at_utc_ms: draft.recorded_at_utc_ms,
            evidence_versions: draft.evidence_versions,
            state: draft.state,
        };
        self.used_versions.insert(ver_key.clone());
        self.scalar_resolutions.insert(ver_key.clone(), record);
        self.scalar_resolutions
            .get(&ver_key)
            .ok_or(MarketEffectError::InvalidEvaluationCohort(
                "failed to retrieve newly recorded scalar resolution",
            ))
    }

    /// Evaluate a chronological holdout cohort of market-effect vintages against both
    /// binary outcome resolutions (in `ledger`) and scalar prediction-interval resolutions
    /// (`MARKET-05`, `A14`).
    ///
    /// Preserves all earlier vintages of any included series, records explicit interval and
    /// directional misses, and keeps qualitative, unresolved, and invalidated vintages visible.
    pub fn evaluate_market_effect_cohort(
        &mut self,
        ledger: &mut ForecastLedger,
        evaluation_version: VersionId,
        binary_evaluation_version: Option<VersionId>,
        evaluated_at_utc_ms: i64,
        cohort: &[MarketCohortEvaluationItem],
        row_limit: usize,
    ) -> Result<&MarketEffectEvaluationRecord, MarketEffectError> {
        if self.evaluations_by_version.len() >= self.max_records {
            return Err(MarketEffectError::BudgetExceeded {
                field: "market_effect_evaluations",
                actual: self.evaluations_by_version.len() + 1,
                limit: self.max_records,
            });
        }
        if self.used_versions.contains(&evaluation_version) {
            return Err(MarketEffectError::DuplicateIdentifier {
                field: "evaluation_version",
                id: evaluation_version.as_str().to_owned(),
            });
        }
        validate_timestamp("evaluated_at_utc_ms", evaluated_at_utc_ms)?;
        if !(1..=MAX_EVALUATION_ROWS).contains(&row_limit) {
            return Err(MarketEffectError::Scoring(ScoringError::InvalidRowLimit));
        }
        if cohort.is_empty() {
            return Err(MarketEffectError::InvalidEvaluationCohort(
                "evaluation cohort must not be empty",
            ));
        }
        if cohort.len() > row_limit {
            return Err(MarketEffectError::Scoring(ScoringError::RowLimitExceeded {
                supplied: cohort.len(),
                limit: row_limit,
            }));
        }

        let mut seen_vintages = BTreeSet::new();
        for item in cohort {
            if !seen_vintages.insert(item.market_vintage_version.clone()) {
                return Err(MarketEffectError::DuplicateIdentifier {
                    field: "cohort.market_vintage_version",
                    id: item.market_vintage_version.as_str().to_owned(),
                });
            }
        }

        let mut binary_cohort = Vec::new();
        let mut interval_rows = Vec::new();
        let mut interval_row_index_by_vintage = BTreeMap::new();

        for item in cohort {
            let vintage = self
                .vintages_by_version
                .get(&item.market_vintage_version)
                .ok_or(MarketEffectError::InvalidEvaluationCohort(
                    "unknown market_vintage_version in cohort",
                ))?;

            // Enforce complete series history so unsuccessful earlier vintages cannot be omitted.
            if vintage.vintage_number > 1
                && let Some(series) = self.series_vintages.get(&vintage.forecast_id)
            {
                for earlier_ver in series.iter().take(vintage.vintage_number - 1) {
                    if !seen_vintages.contains(earlier_ver) {
                        return Err(MarketEffectError::IncompleteVintageSeriesInCohort {
                            forecast_id: vintage.forecast_id.clone(),
                            missing_vintage: earlier_ver.clone(),
                        });
                    }
                }
            }

            if evaluated_at_utc_ms < vintage.issued_at_utc_ms {
                return Err(MarketEffectError::InvalidEvaluationCohort(
                    "evaluated_at_utc_ms cannot predate forecast vintage issued_at_utc_ms",
                ));
            }

            match &vintage.prediction {
                MarketEffectPrediction::Quantitative(quant) => {
                    let Some(bin_res_ver) = &item.binary_resolution_version else {
                        return Err(MarketEffectError::InvalidEvaluationCohort(
                            "quantitative market-effect vintage requires binary_resolution_version",
                        ));
                    };
                    let Some(scalar_res_ver) = &item.scalar_resolution_version else {
                        return Err(MarketEffectError::InvalidEvaluationCohort(
                            "quantitative market-effect vintage requires scalar_resolution_version",
                        ));
                    };
                    let scalar_res = self.scalar_resolutions.get(scalar_res_ver).ok_or(
                        MarketEffectError::InvalidEvaluationCohort(
                            "unknown scalar_resolution_version in cohort",
                        ),
                    )?;
                    if scalar_res.target_instrument_id != vintage.target_instrument_id
                        || scalar_res.effect_unit != quant.effect_unit
                        || scalar_res.horizon_start_utc_ms != vintage.horizon_start_utc_ms
                        || scalar_res.horizon_end_utc_ms != vintage.horizon_end_utc_ms
                    {
                        return Err(MarketEffectError::InvalidEvaluationCohort(
                            "scalar resolution target, unit, or horizon does not match forecast vintage",
                        ));
                    }
                    if scalar_res.recorded_at_utc_ms < vintage.issued_at_utc_ms {
                        return Err(MarketEffectError::InvalidEvaluationCohort(
                            "scalar resolution recorded_at_utc_ms predates forecast issuance",
                        ));
                    }
                    if evaluated_at_utc_ms < scalar_res.recorded_at_utc_ms {
                        return Err(MarketEffectError::InvalidEvaluationCohort(
                            "evaluated_at_utc_ms predates scalar resolution recorded_at_utc_ms",
                        ));
                    }

                    binary_cohort.push((vintage.version_id.clone(), bin_res_ver.clone()));

                    match &scalar_res.state {
                        MarketScalarResolutionState::Resolved {
                            realized_value,
                            observed_at_utc_ms,
                        } => {
                            if *observed_at_utc_ms <= vintage.information_cutoff_utc_ms
                                || *observed_at_utc_ms <= vintage.origin_utc_ms
                                || *observed_at_utc_ms <= vintage.issued_at_utc_ms
                            {
                                return Err(MarketEffectError::InvalidEvaluationCohort(
                                    "scalar resolution observed_at_utc_ms occurred at or before forecast cutoff/origin/issuance",
                                ));
                            }
                            let idx = interval_rows.len();
                            interval_rows.push(IntervalForecastRow {
                                forecast_version: vintage.version_id.clone(),
                                interval: quant.prediction_interval,
                                baseline_version: vintage.baseline_version.clone(),
                                baseline_interval: quant.baseline_interval,
                                unit: quant.effect_unit.clone(),
                                outcome: ScalarOutcome::Resolved {
                                    value: *realized_value,
                                    version: scalar_res.version_id.clone(),
                                },
                            });
                            interval_row_index_by_vintage.insert(vintage.version_id.clone(), idx);
                        }
                        MarketScalarResolutionState::Unresolved(reason) => {
                            let idx = interval_rows.len();
                            interval_rows.push(IntervalForecastRow {
                                forecast_version: vintage.version_id.clone(),
                                interval: quant.prediction_interval,
                                baseline_version: vintage.baseline_version.clone(),
                                baseline_interval: quant.baseline_interval,
                                unit: quant.effect_unit.clone(),
                                outcome: ScalarOutcome::Unresolved(*reason),
                            });
                            interval_row_index_by_vintage.insert(vintage.version_id.clone(), idx);
                        }
                        MarketScalarResolutionState::Invalidated { .. } => {}
                    }
                }
                MarketEffectPrediction::QualitativeOrInsufficient(_) => {
                    if item.binary_resolution_version.is_some() {
                        return Err(MarketEffectError::InvalidEvaluationCohort(
                            "QualitativeOrInsufficient vintage cannot supply a binary_resolution_version",
                        ));
                    }
                }
            }
        }

        let binary_eval_record: Option<EvaluationRunRecord> = if !binary_cohort.is_empty() {
            let Some(bin_eval_ver) = binary_evaluation_version.clone() else {
                return Err(MarketEffectError::InvalidEvaluationCohort(
                    "binary_evaluation_version is required when cohort contains quantitative vintages",
                ));
            };
            Some(
                ledger
                    .evaluate_cohort(bin_eval_ver, evaluated_at_utc_ms, &binary_cohort, row_limit)?
                    .clone(),
            )
        } else {
            None
        };

        let interval_eval: Option<IntervalEvaluation<'_>> = if !interval_rows.is_empty() {
            Some(evaluate_intervals(&interval_rows, row_limit)?)
        } else {
            None
        };

        let mut brier_by_vintage = BTreeMap::new();
        if let Some(bin_rec) = &binary_eval_record {
            for v in &bin_rec.vintages {
                brier_by_vintage.insert(v.forecast_version.clone(), v.brier_scores);
            }
        }

        let mut summaries = Vec::with_capacity(cohort.len());
        let mut quantitative_count = 0usize;
        let mut qualitative_or_insufficient_count = 0usize;
        let mut resolved_scalar_count = 0usize;
        let mut unresolved_scalar_count = 0usize;
        let mut invalidated_scalar_count = 0usize;
        let mut forecast_interval_covered_count = 0usize;
        let mut baseline_interval_covered_count = 0usize;
        let mut abs_error_sum = 0.0;
        let mut baseline_abs_error_sum = 0.0;
        let mut misses = Vec::new();

        for item in cohort {
            let vintage = self
                .vintages_by_version
                .get(&item.market_vintage_version)
                .ok_or(MarketEffectError::InvalidEvaluationCohort(
                    "unknown market_vintage_version in cohort",
                ))?;

            match &vintage.prediction {
                MarketEffectPrediction::Quantitative(quant) => {
                    quantitative_count += 1;
                    let brier_scores = brier_by_vintage.get(&vintage.version_id).copied().flatten();
                    let interval_scores = interval_row_index_by_vintage
                        .get(&vintage.version_id)
                        .and_then(|idx| interval_eval.as_ref().and_then(|ev| ev.rows.get(*idx)))
                        .and_then(|row| row.scores);

                    let scalar_res = item
                        .scalar_resolution_version
                        .as_ref()
                        .and_then(|ver| self.scalar_resolutions.get(ver));
                    let scalar_state = scalar_res.map(|r| r.state.clone());

                    let mut point_error = None;
                    let mut baseline_point_error = None;
                    let mut direction_matched = None;

                    match &scalar_state {
                        Some(MarketScalarResolutionState::Resolved { realized_value, .. }) => {
                            resolved_scalar_count += 1;
                            let err = quant.point_estimate - *realized_value;
                            let base_err = quant.baseline_point_estimate - *realized_value;
                            point_error = Some(err);
                            baseline_point_error = Some(base_err);
                            abs_error_sum += err.abs();
                            baseline_abs_error_sum += base_err.abs();

                            let dir_ok =
                                direction_matches(quant.expected_direction, *realized_value);
                            direction_matched = Some(dir_ok);

                            let (interval_missed, baseline_covered) = match interval_scores {
                                Some(scores) => (!scores.forecast.covered, scores.baseline.covered),
                                None => (
                                    *realized_value < quant.prediction_interval.lower()
                                        || *realized_value > quant.prediction_interval.upper(),
                                    *realized_value >= quant.baseline_interval.lower()
                                        && *realized_value <= quant.baseline_interval.upper(),
                                ),
                            };
                            if !interval_missed {
                                forecast_interval_covered_count += 1;
                            }
                            if baseline_covered {
                                baseline_interval_covered_count += 1;
                            }

                            let direction_missed = dir_ok == Some(false);
                            if interval_missed || direction_missed {
                                let excursion =
                                    if *realized_value < quant.prediction_interval.lower() {
                                        quant.prediction_interval.lower() - *realized_value
                                    } else if *realized_value > quant.prediction_interval.upper() {
                                        *realized_value - quant.prediction_interval.upper()
                                    } else {
                                        0.0
                                    };
                                misses.push(MarketPredictionMissRecord {
                                    forecast_id: vintage.forecast_id.clone(),
                                    forecast_version: vintage.version_id.clone(),
                                    vintage_number: vintage.vintage_number,
                                    realized_value: *realized_value,
                                    interval_lower: quant.prediction_interval.lower(),
                                    interval_upper: quant.prediction_interval.upper(),
                                    interval_excursion: excursion,
                                    interval_missed,
                                    direction_missed,
                                });
                            }
                        }
                        Some(MarketScalarResolutionState::Unresolved(_)) => {
                            unresolved_scalar_count += 1;
                        }
                        Some(MarketScalarResolutionState::Invalidated { .. }) => {
                            invalidated_scalar_count += 1;
                        }
                        None => {}
                    }

                    summaries.push(EvaluatedMarketVintageSummary {
                        forecast_id: vintage.forecast_id.clone(),
                        forecast_version: vintage.version_id.clone(),
                        vintage_number: vintage.vintage_number,
                        is_quantitative: true,
                        post_cutoff_model_knowledge_disclosed: vintage
                            .post_cutoff_model_knowledge_disclosed,
                        brier_scores,
                        interval_scores,
                        point_error,
                        baseline_point_error,
                        direction_matched: direction_matched.flatten(),
                        scalar_resolution_state: scalar_state,
                    });
                }
                MarketEffectPrediction::QualitativeOrInsufficient(_) => {
                    qualitative_or_insufficient_count += 1;
                    let scalar_state = item
                        .scalar_resolution_version
                        .as_ref()
                        .and_then(|ver| self.scalar_resolutions.get(ver))
                        .map(|r| r.state.clone());
                    summaries.push(EvaluatedMarketVintageSummary {
                        forecast_id: vintage.forecast_id.clone(),
                        forecast_version: vintage.version_id.clone(),
                        vintage_number: vintage.vintage_number,
                        is_quantitative: false,
                        post_cutoff_model_knowledge_disclosed: vintage
                            .post_cutoff_model_knowledge_disclosed,
                        brier_scores: None,
                        interval_scores: None,
                        point_error: None,
                        baseline_point_error: None,
                        direction_matched: None,
                        scalar_resolution_state: scalar_state,
                    });
                }
            }
        }

        let mean_brier_scores = binary_eval_record.as_ref().and_then(|r| r.mean_scores);
        let (mean_forecast_interval_score, mean_baseline_interval_score) = match &interval_eval {
            Some(ev) => match ev.means {
                Some(means) => (Some(means.forecast.score), Some(means.baseline.score)),
                None => (None, None),
            },
            None => (None, None),
        };
        let mean_abs_error =
            (resolved_scalar_count > 0).then(|| abs_error_sum / resolved_scalar_count as f64);
        let mean_baseline_abs_error = (resolved_scalar_count > 0)
            .then(|| baseline_abs_error_sum / resolved_scalar_count as f64);

        let record = MarketEffectEvaluationRecord {
            evaluation_version: evaluation_version.clone(),
            binary_evaluation_version,
            evaluated_at_utc_ms,
            vintages: summaries,
            quantitative_count,
            qualitative_or_insufficient_count,
            resolved_scalar_count,
            unresolved_scalar_count,
            invalidated_scalar_count,
            forecast_interval_covered_count,
            baseline_interval_covered_count,
            mean_brier_scores,
            mean_forecast_interval_score,
            mean_baseline_interval_score,
            mean_abs_error,
            mean_baseline_abs_error,
            misses,
        };

        self.used_versions.insert(evaluation_version.clone());
        self.evaluation_order.push(evaluation_version.clone());
        self.evaluations_by_version
            .insert(evaluation_version.clone(), record);
        self.evaluations_by_version.get(&evaluation_version).ok_or(
            MarketEffectError::InvalidEvaluationCohort(
                "failed to retrieve newly recorded market-effect evaluation",
            ),
        )
    }

    /// Evaluate a hypothetical strategy simulation with mandatory fee, half-spread, and slippage
    /// deductions (`MARKET-05`, `AGENTS.md` §6).
    ///
    /// Refuses zero-friction simulations and guarantees `is_observed_market_return() == false`
    /// and `can_execute_orders() == false`.
    pub fn evaluate_simulated_strategy(
        &self,
        simulation_version: VersionId,
        forecast_version: &VersionId,
        scalar_resolution_version: &VersionId,
        friction: SimulatedFrictionConfig,
    ) -> Result<SimulatedStrategyEvaluation, MarketEffectError> {
        if !friction.commission_bps_per_side.is_finite()
            || !friction.half_spread_bps_per_side.is_finite()
            || !friction.slippage_bps_per_side.is_finite()
            || friction.commission_bps_per_side < 0.0
            || friction.half_spread_bps_per_side < 0.0
            || friction.slippage_bps_per_side < 0.0
        {
            return Err(MarketEffectError::InvalidStrategySimulation(
                "commission, half_spread, and slippage per side must be finite and >= 0.0",
            ));
        }
        let round_trip_friction = friction.round_trip_friction_bps();
        if !round_trip_friction.is_finite()
            || round_trip_friction <= 0.0
            || round_trip_friction > MAX_FRICTION_BPS
        {
            return Err(MarketEffectError::InvalidStrategySimulation(
                "round-trip friction must be finite, strictly positive (> 0.0 bps), and <= 10,000 bps",
            ));
        }

        let vintage = self.vintages_by_version.get(forecast_version).ok_or(
            MarketEffectError::InvalidStrategySimulation("unknown forecast_version"),
        )?;
        let MarketEffectPrediction::Quantitative(quant) = &vintage.prediction else {
            return Err(MarketEffectError::InvalidStrategySimulation(
                "strategy simulation requires a Quantitative market-effect vintage",
            ));
        };
        let scalar_res = self
            .scalar_resolutions
            .get(scalar_resolution_version)
            .ok_or(MarketEffectError::InvalidStrategySimulation(
                "unknown scalar_resolution_version",
            ))?;
        if scalar_res.target_instrument_id != vintage.target_instrument_id
            || scalar_res.effect_unit != quant.effect_unit
            || scalar_res.horizon_start_utc_ms != vintage.horizon_start_utc_ms
            || scalar_res.horizon_end_utc_ms != vintage.horizon_end_utc_ms
        {
            return Err(MarketEffectError::InvalidStrategySimulation(
                "scalar resolution does not match forecast target, unit, or horizon",
            ));
        }
        let MarketScalarResolutionState::Resolved { realized_value, .. } = scalar_res.state else {
            return Err(MarketEffectError::InvalidStrategySimulation(
                "strategy simulation requires a Resolved scalar market outcome",
            ));
        };

        let gross_simulated_return_bps = match quant.expected_direction {
            MarketDirection::Positive => realized_value,
            MarketDirection::Negative => -realized_value,
            MarketDirection::AmbiguousOrTwoSided | MarketDirection::MutedOrAlreadyPriced => {
                return Err(MarketEffectError::InvalidStrategySimulation(
                    "directional strategy simulation requires Positive or Negative expected_direction",
                ));
            }
        };
        let net_simulated_return_bps = gross_simulated_return_bps - round_trip_friction;
        if !gross_simulated_return_bps.is_finite() || !net_simulated_return_bps.is_finite() {
            return Err(MarketEffectError::InvalidStrategySimulation(
                "simulated return overflowed finite bounds",
            ));
        }

        Ok(SimulatedStrategyEvaluation {
            simulation_version,
            forecast_version: forecast_version.clone(),
            scalar_resolution_version: scalar_resolution_version.clone(),
            friction_config: friction,
            gross_simulated_return_bps,
            round_trip_friction_deducted_bps: round_trip_friction,
            net_simulated_return_bps,
        })
    }

    /// Look up an issued market-effect forecast vintage by its immutable [`VersionId`].
    #[must_use]
    pub fn vintage(&self, version_id: &VersionId) -> Option<&MarketEffectVintageRecord> {
        self.vintages_by_version.get(version_id)
    }

    /// Return all immutable vintages for `forecast_id` in chronological issuance order.
    #[must_use]
    pub fn series_vintages(&self, forecast_id: &OpaqueId) -> Vec<&MarketEffectVintageRecord> {
        self.series_vintages
            .get(forecast_id)
            .map(|vers| {
                vers.iter()
                    .filter_map(|v| self.vintages_by_version.get(v))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Look up a scalar market outcome resolution by its immutable [`VersionId`].
    #[must_use]
    pub fn scalar_resolution(
        &self,
        version_id: &VersionId,
    ) -> Option<&MarketScalarResolutionRecord> {
        self.scalar_resolutions.get(version_id)
    }

    /// Look up a market-effect evaluation run by its immutable [`VersionId`].
    #[must_use]
    pub fn evaluation(&self, version_id: &VersionId) -> Option<&MarketEffectEvaluationRecord> {
        self.evaluations_by_version.get(version_id)
    }
}

fn validate_text(field: &'static str, value: &str) -> Result<(), MarketEffectError> {
    if value.trim().is_empty() {
        return Err(MarketEffectError::InvalidTextField {
            field,
            reason: "must not be empty or whitespace-only",
        });
    }
    if value.len() > MAX_TEXT_FIELD_BYTES {
        return Err(MarketEffectError::InvalidTextField {
            field,
            reason: "exceeds MAX_TEXT_FIELD_BYTES",
        });
    }
    Ok(())
}

fn validate_timestamp(field: &'static str, value: i64) -> Result<(), MarketEffectError> {
    if !(MIN_VALID_UTC_MS..=MAX_VALID_UTC_MS).contains(&value) {
        return Err(MarketEffectError::InvalidTimestamp {
            field,
            reason: "timestamp outside valid UTC millisecond range",
        });
    }
    Ok(())
}

fn validate_branch_response(
    field: &'static str,
    branch: &ConditionalBranchResponse,
) -> Result<(), MarketEffectError> {
    validate_text(field, &branch.branch_label)?;
    if !branch.conditional_point_estimate.is_finite() {
        return Err(MarketEffectError::ConditionalScenarioContractViolation(
            "conditional_point_estimate must be finite",
        ));
    }
    if branch.conditional_point_estimate < branch.conditional_interval.lower()
        || branch.conditional_point_estimate > branch.conditional_interval.upper()
    {
        return Err(MarketEffectError::ConditionalScenarioContractViolation(
            "conditional_point_estimate must lie within conditional_interval [lower, upper]",
        ));
    }
    Ok(())
}

fn direction_matches(expected: MarketDirection, realized_value: f64) -> Option<bool> {
    match expected {
        MarketDirection::Positive => Some(realized_value > 0.0),
        MarketDirection::Negative => Some(realized_value < 0.0),
        MarketDirection::AmbiguousOrTwoSided | MarketDirection::MutedOrAlreadyPriced => None,
    }
}
