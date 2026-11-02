//! Bounded inspectable reasoning, abstraction, correlation, deduction, extrapolation,
//! and composed analysis graph kernel (`REASON-01`..=`REASON-04`, `A07`, `#35`).
//!
//! This module implements the pure domain and dependency-graph kernel for:
//! - **`REASON-01` (Deduction):** Exposes premises, explicit assumptions, named inference
//!   rules, and ordered derivation steps while keeping `AiProposed`, `HumanReviewed`, and
//!   `AssertedUnverifiedLeanRef` reasoning tiers distinct and marking every conclusion
//!   conditional on its premises and assumptions.
//! - **`REASON-02` (Correlation & Extrapolation):** Enforces explicit matching tolerances,
//!   coverage fractions, baseline rates (or documented absence rationale), source-dependency
//!   accounting, and multiple-comparison disclosures for correlations without asserting
//!   causation; and enforces observed domain bounds, unit compatibility, projection
//!   horizons, model versions, uncertainty methods, and sensitivity notes for extrapolations.
//! - **`REASON-03` (Abstraction):** Normalizes reports into versioned event/entity/relationship
//!   structures while preserving verbatim wording, explicit unknown values, conflicts,
//!   omissions, version history, and reversible entity-match and relationship decisions.
//! - **`REASON-04` / `A07` (Composed Analysis & Competing Hypotheses):** Enforces an acyclic
//!   dependency graph across premises and reasoning steps, preserves input/inference origin
//!   classes so inferences fed into later steps never masquerade as observations, retains
//!   competing hypotheses with contradictions and discriminating evidence, and propagates
//!   non-destructive reassessment notices when a premise or upstream step is revised.
//!
//! Note on scope: this kernel operates in memory within `oia-analysis` without SQLite
//! persistence (`oia-store`), background job orchestration (`oia-app`), or a live Lean
//! runtime. An `AssertedUnverifiedLeanRef` tier records a caller-asserted theorem
//! identifier, Lean toolchain, and model revision without claiming runtime verification
//! or formal Rust implementation correspondence (`AGENTS.md` §3).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use oia_domain::{ActorId, OpaqueId, SchemaVersion, VersionId};

/// Mandatory disclosure attached to every correlation step (`REASON-02`, `AGENTS.md` §2).
pub const CORRELATION_NON_CAUSATION_NOTICE: &str =
    "CORRELATION ONLY — DOES NOT ESTABLISH CAUSATION OR INDEPENDENT CORROBORATION";

/// Mandatory disclosure attached to every deduction step (`REASON-01`).
pub const DEDUCTION_CONDITIONALITY_NOTICE: &str =
    "CONDITIONAL DERIVATION — VALIDITY DEPENDS ON STATED PREMISES AND ASSUMPTIONS";

/// Mandatory disclosure attached to every extrapolation step (`REASON-02`).
pub const EXTRAPOLATION_PROJECTION_NOTICE: &str =
    "MODEL PROJECTION OUTSIDE OBSERVED DOMAIN — NOT AN OBSERVED MEASUREMENT";

/// Default maximum number of items per step collection (fields, candidates, links, steps).
pub const DEFAULT_MAX_ITEMS_PER_STEP: usize = 64;

/// Which budget was zero or exceeded in a [`ReasoningGraph`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReasoningLimitKind {
    Premises,
    Steps,
    Hypotheses,
    ItemsPerStep,
    TextBytes,
}

impl fmt::Display for ReasoningLimitKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Premises => f.write_str("max_premises"),
            Self::Steps => f.write_str("max_steps"),
            Self::Hypotheses => f.write_str("max_hypotheses"),
            Self::ItemsPerStep => f.write_str("max_items_per_step"),
            Self::TextBytes => f.write_str("max_text_bytes"),
        }
    }
}

/// Resource budgets for a bounded [`ReasoningGraph`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReasoningLimits {
    max_premises: usize,
    max_steps: usize,
    max_hypotheses: usize,
    max_items_per_step: usize,
    max_text_bytes: usize,
}

impl ReasoningLimits {
    /// Validates that all budgets are non-zero.
    pub fn new(
        max_premises: usize,
        max_steps: usize,
        max_hypotheses: usize,
        max_items_per_step: usize,
        max_text_bytes: usize,
    ) -> Result<Self, ReasoningError> {
        if max_premises == 0 {
            return Err(ReasoningError::ZeroLimit(ReasoningLimitKind::Premises));
        }
        if max_steps == 0 {
            return Err(ReasoningError::ZeroLimit(ReasoningLimitKind::Steps));
        }
        if max_hypotheses == 0 {
            return Err(ReasoningError::ZeroLimit(ReasoningLimitKind::Hypotheses));
        }
        if max_items_per_step == 0 {
            return Err(ReasoningError::ZeroLimit(ReasoningLimitKind::ItemsPerStep));
        }
        if max_text_bytes == 0 {
            return Err(ReasoningError::ZeroLimit(ReasoningLimitKind::TextBytes));
        }
        Ok(Self {
            max_premises,
            max_steps,
            max_hypotheses,
            max_items_per_step,
            max_text_bytes,
        })
    }

    #[must_use]
    pub const fn max_premises(self) -> usize {
        self.max_premises
    }

    #[must_use]
    pub const fn max_steps(self) -> usize {
        self.max_steps
    }

    #[must_use]
    pub const fn max_hypotheses(self) -> usize {
        self.max_hypotheses
    }

    #[must_use]
    pub const fn max_items_per_step(self) -> usize {
        self.max_items_per_step
    }

    #[must_use]
    pub const fn max_text_bytes(self) -> usize {
        self.max_text_bytes
    }
}

/// Typed errors for reasoning validation, domain checks, and dependency propagation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReasoningError {
    ZeroLimit(ReasoningLimitKind),
    BudgetExceeded {
        limit: ReasoningLimitKind,
        actual: usize,
        max: usize,
    },
    EmptyField(&'static str),
    ControlCharacter(&'static str),
    DuplicatePremiseId(OpaqueId),
    DuplicateStepId(OpaqueId),
    DuplicateHypothesisId(OpaqueId),
    DuplicateVersionId(VersionId),
    DuplicateItemId {
        context: &'static str,
        id: String,
    },
    UnknownPremiseVersion(VersionId),
    UnknownStepVersion(VersionId),
    UnknownInputNodeVersion(VersionId),
    UnknownEntityCandidate(OpaqueId),
    UnknownRelationship(OpaqueId),
    PremisePredecessorMismatch {
        premise_id: OpaqueId,
        expected_latest: VersionId,
        supplied: Option<VersionId>,
    },
    StepPredecessorMismatch {
        step_id: OpaqueId,
        expected_latest: VersionId,
        supplied: Option<VersionId>,
    },
    StepKindChangeForbidden {
        step_id: OpaqueId,
    },
    EmptyStepInputs,
    DuplicateStepInput(VersionId),
    InputOriginClassMismatch {
        node_version: VersionId,
        declared: EvidenceOriginClass,
        actual: EvidenceOriginClass,
    },
    ObservationPromotionForbidden {
        step_id: OpaqueId,
    },
    AbstractionMissingContent,
    DecisionReversalMissingRationale(&'static str),
    InvalidTolerance(&'static str),
    InvalidCoverageFraction,
    InvalidRateOrCount(&'static str),
    EmptyCorrelationPairs,
    InvalidMultipleComparisonCount {
        tested_hypotheses: u32,
        reported_matches: u32,
    },
    EmptyDeductionPremises,
    EmptyDeductionSteps,
    InvalidVerificationMetadata(&'static str),
    InvalidNumericValue(&'static str),
    InvalidObservedDomain,
    InvalidProjectionHorizon {
        observed_max: String,
        horizon_start: String,
        horizon_end: String,
    },
    UnitMismatch {
        expected_unit: String,
        actual_unit: String,
    },
    UnsupportedDomainValue {
        field: &'static str,
        value: String,
        domain_min: String,
        domain_max: String,
    },
    EmptyHypothesisLinks(&'static str),
    HypothesisSupportContradictionOverlap(VersionId),
    EmptyDiscriminatingLeads,
}

impl fmt::Display for ReasoningError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroLimit(kind) => write!(f, "reasoning limit {kind} must be >= 1"),
            Self::BudgetExceeded { limit, actual, max } => {
                write!(f, "reasoning limit {limit} exceeded ({actual} > {max})")
            }
            Self::EmptyField(field) => write!(f, "{field} must not be empty"),
            Self::ControlCharacter(field) => {
                write!(
                    f,
                    "{field} contains disallowed control or bidi/format characters"
                )
            }
            Self::DuplicatePremiseId(id) => write!(f, "duplicate premise ID {id}"),
            Self::DuplicateStepId(id) => write!(f, "duplicate reasoning step ID {id}"),
            Self::DuplicateHypothesisId(id) => write!(f, "duplicate hypothesis ID {id}"),
            Self::DuplicateVersionId(version) => {
                write!(f, "version ID {version} is already registered")
            }
            Self::DuplicateItemId { context, id } => {
                write!(f, "duplicate {context} identifier {id}")
            }
            Self::UnknownPremiseVersion(version) => {
                write!(f, "unknown premise version {version}")
            }
            Self::UnknownStepVersion(version) => {
                write!(f, "unknown reasoning step version {version}")
            }
            Self::UnknownInputNodeVersion(version) => {
                write!(f, "unknown input node version {version}")
            }
            Self::UnknownEntityCandidate(id) => {
                write!(f, "unknown entity match candidate {id}")
            }
            Self::UnknownRelationship(id) => {
                write!(f, "unknown abstraction relationship {id}")
            }
            Self::PremisePredecessorMismatch {
                premise_id,
                expected_latest,
                supplied,
            } => write!(
                f,
                "premise {premise_id} revision must cite latest version {expected_latest}, got {supplied:?}"
            ),
            Self::StepPredecessorMismatch {
                step_id,
                expected_latest,
                supplied,
            } => write!(
                f,
                "step {step_id} revision must cite latest version {expected_latest}, got {supplied:?}"
            ),
            Self::StepKindChangeForbidden { step_id } => write!(
                f,
                "step {step_id} revision cannot change its reasoning method family"
            ),
            Self::EmptyStepInputs => {
                f.write_str("reasoning step must declare at least one input node")
            }
            Self::DuplicateStepInput(version) => {
                write!(f, "duplicate input version {version} in reasoning step")
            }
            Self::InputOriginClassMismatch {
                node_version,
                declared,
                actual,
            } => write!(
                f,
                "input {node_version} declared origin class {declared}, but actual class is {actual}"
            ),
            Self::ObservationPromotionForbidden { step_id } => write!(
                f,
                "reasoning step {step_id} produces an analytical inference and cannot promote outputs to observations"
            ),
            Self::AbstractionMissingContent => f.write_str(
                "abstraction step must normalize at least one field, entity match, relationship, conflict, or omission",
            ),
            Self::DecisionReversalMissingRationale(context) => {
                write!(f, "reversible {context} decision requires a non-empty rationale")
            }
            Self::InvalidTolerance(field) => {
                write!(f, "{field} must be finite and strictly positive")
            }
            Self::InvalidCoverageFraction => {
                f.write_str("coverage fraction must be finite and within [0.0, 1.0]")
            }
            Self::InvalidRateOrCount(field) => {
                write!(f, "{field} is out of valid range")
            }
            Self::EmptyCorrelationPairs => {
                f.write_str("correlation step must record at least one matched pair")
            }
            Self::InvalidMultipleComparisonCount {
                tested_hypotheses,
                reported_matches,
            } => write!(
                f,
                "tested_hypotheses ({tested_hypotheses}) must be >= 1 and >= reported_matches ({reported_matches})"
            ),
            Self::EmptyDeductionPremises => {
                f.write_str("deduction step must cite at least one input premise or step")
            }
            Self::EmptyDeductionSteps => {
                f.write_str("deduction step must contain at least one derivation step")
            }
            Self::InvalidVerificationMetadata(field) => {
                write!(f, "invalid verification metadata for {field}")
            }
            Self::InvalidNumericValue(field) => {
                write!(f, "{field} must be a finite number")
            }
            Self::InvalidObservedDomain => {
                f.write_str("observed domain requires finite min < max and sample_count >= 2")
            }
            Self::InvalidProjectionHorizon {
                observed_max,
                horizon_start,
                horizon_end,
            } => write!(
                f,
                "projection horizon [{horizon_start}, {horizon_end}] must satisfy observed_max ({observed_max}) <= horizon_start < horizon_end"
            ),
            Self::UnitMismatch {
                expected_unit,
                actual_unit,
            } => write!(
                f,
                "unit mismatch: expected '{expected_unit}', got '{actual_unit}'"
            ),
            Self::UnsupportedDomainValue {
                field,
                value,
                domain_min,
                domain_max,
            } => write!(
                f,
                "{field} value {value} is outside supported observed domain [{domain_min}, {domain_max}]"
            ),
            Self::EmptyHypothesisLinks(field) => {
                write!(f, "hypothesis {field} must not be empty")
            }
            Self::HypothesisSupportContradictionOverlap(version) => write!(
                f,
                "hypothesis cannot cite version {version} as both supporting and contradicting"
            ),
            Self::EmptyDiscriminatingLeads => f.write_str(
                "competing hypothesis must specify at least one discriminating evidence lead",
            ),
        }
    }
}

impl std::error::Error for ReasoningError {}

/// Evidence and inference origin classes (`docs/architecture.md` §2.3, `AGENTS.md` §2).
///
/// Feeding an [`EvidenceOriginClass::AnalyticalInference`] into another reasoning step
/// preserves its inference origin and never promotes it to [`EvidenceOriginClass::NormalizedObservation`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EvidenceOriginClass {
    /// Retained source document, dataset, or media record.
    AcquiredSourceMaterial,
    /// Witness account or reported testimony.
    UserTestimony,
    /// Instrument or sensor measurement normalized with unit/time/geometry.
    NormalizedObservation,
    /// Proposition extracted from a source passage.
    ExtractedClaim,
    /// Output of a deduction, correlation, extrapolation, or abstraction step.
    AnalyticalInference,
    /// Issued point-in-time prediction or scenario output.
    Forecast,
    /// Non-predictive visual or format derivative.
    ConventionalDerivative,
    /// Learned restoration, interpolation, or super-resolution output.
    PredictiveDerivative,
    /// Labeled synthetic or simulated fixture data.
    Simulation,
    /// Labeled illustrative prompt or generated image recreation.
    IllustrativeRecreation,
}

impl EvidenceOriginClass {
    /// Returns `true` only when the record is an observation (`NormalizedObservation`).
    #[must_use]
    pub const fn is_observation(self) -> bool {
        matches!(self, Self::NormalizedObservation)
    }

    /// Returns `true` when the record is a reasoning inference, forecast, predictive
    /// derivative, simulation, or recreation that must retain non-observational warnings
    /// through downstream steps.
    #[must_use]
    pub const fn requires_non_observational_qualification(self) -> bool {
        matches!(
            self,
            Self::AnalyticalInference
                | Self::Forecast
                | Self::PredictiveDerivative
                | Self::Simulation
                | Self::IllustrativeRecreation
        )
    }
}

impl fmt::Display for EvidenceOriginClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AcquiredSourceMaterial => f.write_str("acquired_source_material"),
            Self::UserTestimony => f.write_str("user_testimony"),
            Self::NormalizedObservation => f.write_str("normalized_observation"),
            Self::ExtractedClaim => f.write_str("extracted_claim"),
            Self::AnalyticalInference => f.write_str("analytical_inference"),
            Self::Forecast => f.write_str("forecast"),
            Self::ConventionalDerivative => f.write_str("conventional_derivative"),
            Self::PredictiveDerivative => f.write_str("predictive_derivative"),
            Self::Simulation => f.write_str("simulation"),
            Self::IllustrativeRecreation => f.write_str("illustrative_recreation"),
        }
    }
}

/// Optional physical quantity with an explicit unit, domain bounds, and uncertainty (`REASON-02`, `A07`).
#[derive(Debug, Clone, PartialEq)]
pub struct MeasuredQuantity {
    pub value: f64,
    pub unit: String,
    pub uncertainty: Option<f64>,
    pub domain_min: f64,
    pub domain_max: f64,
}

/// Input specification for registering an initial or revised evidence premise (`REASON-01`, `REASON-04`).
#[derive(Debug, Clone, PartialEq)]
pub struct PremiseDraft {
    pub premise_id: OpaqueId,
    pub version_id: VersionId,
    pub predecessor_version: Option<VersionId>,
    pub schema_version: SchemaVersion,
    pub actor: ActorId,
    pub origin_class: EvidenceOriginClass,
    pub source_version: VersionId,
    pub citation_anchor: String,
    pub verbatim_excerpt: String,
    pub proposition: String,
    pub quantity: Option<MeasuredQuantity>,
    pub uncertainty_note: String,
    pub revision_rationale: Option<String>,
}

/// Immutable versioned premise record stored in a [`ReasoningGraph`].
#[derive(Debug, Clone, PartialEq)]
pub struct PremiseRecord {
    premise_id: OpaqueId,
    version_id: VersionId,
    predecessor_version: Option<VersionId>,
    vintage_number: u32,
    schema_version: SchemaVersion,
    actor: ActorId,
    origin_class: EvidenceOriginClass,
    source_version: VersionId,
    citation_anchor: String,
    verbatim_excerpt: String,
    proposition: String,
    quantity: Option<MeasuredQuantity>,
    uncertainty_note: String,
    revision_rationale: Option<String>,
}

impl PremiseRecord {
    #[must_use]
    pub const fn premise_id(&self) -> &OpaqueId {
        &self.premise_id
    }

    #[must_use]
    pub const fn version_id(&self) -> &VersionId {
        &self.version_id
    }

    #[must_use]
    pub const fn predecessor_version(&self) -> Option<&VersionId> {
        self.predecessor_version.as_ref()
    }

    #[must_use]
    pub const fn vintage_number(&self) -> u32 {
        self.vintage_number
    }

    #[must_use]
    pub const fn schema_version(&self) -> SchemaVersion {
        self.schema_version
    }

    #[must_use]
    pub const fn actor(&self) -> &ActorId {
        &self.actor
    }

    #[must_use]
    pub const fn origin_class(&self) -> EvidenceOriginClass {
        self.origin_class
    }

    #[must_use]
    pub const fn source_version(&self) -> &VersionId {
        &self.source_version
    }

    #[must_use]
    pub fn citation_anchor(&self) -> &str {
        &self.citation_anchor
    }

    #[must_use]
    pub fn verbatim_excerpt(&self) -> &str {
        &self.verbatim_excerpt
    }

    #[must_use]
    pub fn proposition(&self) -> &str {
        &self.proposition
    }

    #[must_use]
    pub const fn quantity(&self) -> Option<&MeasuredQuantity> {
        self.quantity.as_ref()
    }

    #[must_use]
    pub fn uncertainty_note(&self) -> &str {
        &self.uncertainty_note
    }

    #[must_use]
    pub fn revision_rationale(&self) -> Option<&str> {
        self.revision_rationale.as_deref()
    }
}

/// How an normalized abstraction field value is represented (`REASON-03`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbstractionFieldState {
    /// Normalized value with a non-empty verbatim phrase from the cited source.
    Normalized {
        normalized_value: String,
        verbatim_wording: String,
        source_version: VersionId,
    },
    /// Value is explicitly unknown rather than filled with a default (`AGENTS.md` §2).
    ExplicitUnknown {
        reason: String,
        verbatim_context: String,
        source_version: VersionId,
    },
    /// Sources disagree on this field; both competing values and their verbatim wording are retained.
    Conflicting {
        primary_value: String,
        primary_verbatim: String,
        primary_source_version: VersionId,
        competing_value: String,
        competing_verbatim: String,
        competing_source_version: VersionId,
        conflict_note: String,
    },
}

/// A named event/concept slot in an [`AbstractionSpec`] (`REASON-03`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbstractionField {
    pub field_name: String,
    pub state: AbstractionFieldState,
}

/// Reversible status of an entity-resolution or relationship decision (`REASON-03`, `A08`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionState {
    /// Proposed by an automated extractor or initial pass; not yet confirmed.
    Proposed { rationale: String },
    /// Confirmed by a reviewer with an explicit rationale.
    Confirmed { actor: ActorId, rationale: String },
    /// Explicitly rejected or reversed so false merges can be undone cleanly.
    Reversed {
        actor: ActorId,
        reversal_rationale: String,
    },
}

/// Reversible entity-resolution candidate in an [`AbstractionSpec`] (`REASON-03`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityMatchCandidate {
    pub candidate_id: OpaqueId,
    pub mention_verbatim: String,
    pub source_version: VersionId,
    pub target_entity_id: OpaqueId,
    pub taxonomy_concept: String,
    pub decision: DecisionState,
}

/// Explicit relationship semantics in an [`AbstractionSpec`] (`docs/architecture.md` §2.3, `REASON-03`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AbstractionRelationType {
    ObservedCoOccurrence,
    ReportedCommunication,
    ContractualAward,
    OrganizationalAffiliation,
    SpatialProximity,
    TemporalSequence,
    PossibleSameEvent,
}

impl AbstractionRelationType {
    /// Co-occurrence and proximity edges must not be presented as established affiliation or causation.
    #[must_use]
    pub const fn asserts_affiliation_or_causation(self) -> bool {
        false
    }
}

impl fmt::Display for AbstractionRelationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ObservedCoOccurrence => f.write_str("observed_co_occurrence"),
            Self::ReportedCommunication => f.write_str("reported_communication"),
            Self::ContractualAward => f.write_str("contractual_award"),
            Self::OrganizationalAffiliation => f.write_str("organizational_affiliation"),
            Self::SpatialProximity => f.write_str("spatial_proximity"),
            Self::TemporalSequence => f.write_str("temporal_sequence"),
            Self::PossibleSameEvent => f.write_str("possible_same_event"),
        }
    }
}

/// Typed, reversible relationship edge in an [`AbstractionSpec`] (`REASON-03`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbstractionRelationship {
    pub relationship_id: OpaqueId,
    pub from_entity_id: OpaqueId,
    pub to_entity_id: OpaqueId,
    pub relation_type: AbstractionRelationType,
    pub verbatim_basis: String,
    pub source_version: VersionId,
    pub decision: DecisionState,
}

/// Normalization of raw reports into editable event structures, taxonomy concepts, and
/// reversible entity/relationship graphs (`REASON-03`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbstractionSpec {
    pub event_schema: String,
    pub taxonomy_version: VersionId,
    pub fields: Vec<AbstractionField>,
    pub entity_matches: Vec<EntityMatchCandidate>,
    pub relationships: Vec<AbstractionRelationship>,
    pub explicit_omissions: Vec<String>,
}

/// Source independence classification for a correlated pair (`REASON-02`, `AGENTS.md` §2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceDependencyStatus {
    /// Both sides trace to distinct upstream sources with no known shared origin.
    IndependentSources,
    /// Both records trace to a common upstream source or syndication feed and do not
    /// provide independent corroboration.
    SharedUpstreamSource { common_source_version: VersionId },
    /// Lineage is incomplete; independence cannot be assumed.
    UnverifiedIndependence { limitation_note: String },
}

/// A matched pair of records in a [`CorrelationSpec`] (`REASON-02`).
#[derive(Debug, Clone, PartialEq)]
pub struct CorrelationPair {
    pub left_version: VersionId,
    pub right_version: VersionId,
    pub observed_temporal_delta_seconds: f64,
    pub observed_spatial_delta_km: f64,
    pub source_dependency: SourceDependencyStatus,
}

/// Base-rate context for a correlation analysis (`REASON-02`).
#[derive(Debug, Clone, PartialEq)]
pub enum BaselineRateSpec {
    /// Known empirical background coincidence rate in `[0.0, 1.0]`.
    EmpiricalRate {
        rate_per_window: f64,
        basis_description: String,
    },
    /// Background rate is unavailable; the limitation is disclosed explicitly.
    Unavailable { reason: String },
}

/// Correlation analysis specification retaining tolerances, coverage, baseline rates,
/// source dependencies, and multiple-comparison disclosures (`REASON-02`).
#[derive(Debug, Clone, PartialEq)]
pub struct CorrelationSpec {
    pub temporal_tolerance_seconds: f64,
    pub spatial_tolerance_km: f64,
    pub attribute_matching_rule: String,
    pub evaluated_pair_count: u64,
    pub matched_pairs: Vec<CorrelationPair>,
    pub coverage_fraction: f64,
    pub coverage_limitations: String,
    pub baseline_rate: BaselineRateSpec,
    pub tested_hypotheses_count: u32,
    pub multiple_comparison_method: String,
    pub correlation_summary: String,
}

/// Verification tier of a deductive derivation (`REASON-01`, `AGENTS.md` §3).
///
/// Keeps AI-proposed reasoning, human-reviewed reasoning, and caller-asserted (unverified)
/// Lean theorem references explicitly distinct. Because `proofs/` (`#15`) is not yet merged
/// on `main` and this in-memory kernel cannot verify a Lean theorem or establish Rust
/// implementation correspondence at runtime, any Lean reference is explicitly typed as
/// [`ReasoningVerificationTier::AssertedUnverifiedLeanRef`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReasoningVerificationTier {
    /// Proposed by a language model or automated heuristic; unreviewed.
    AiProposed {
        model_id: String,
        prompt_template_version: VersionId,
    },
    /// Reviewed by a human analyst with a recorded scope note.
    HumanReviewed {
        reviewer: ActorId,
        review_note: String,
    },
    /// Caller-asserted reference to a Lean theorem and model revision.
    ///
    /// **Not verified at runtime:** this kernel neither runs `lake` nor establishes formal
    /// correspondence between a Lean model and this Rust step (`AGENTS.md` §3).
    AssertedUnverifiedLeanRef {
        theorem_id: String,
        lean_toolchain: String,
        model_revision: String,
        assumptions_summary: String,
    },
}

impl ReasoningVerificationTier {
    /// Returns a stable tier label (`ai_proposed`, `human_reviewed`, or
    /// `asserted_unverified_lean_ref`).
    #[must_use]
    pub const fn tier_label(&self) -> &'static str {
        match self {
            Self::AiProposed { .. } => "ai_proposed",
            Self::HumanReviewed { .. } => "human_reviewed",
            Self::AssertedUnverifiedLeanRef { .. } => "asserted_unverified_lean_ref",
        }
    }

    /// Returns `false` unconditionally: this in-memory kernel never verifies a Lean proof
    /// or establishes formal Rust implementation correspondence (`AGENTS.md` §3).
    #[must_use]
    pub const fn establishes_lean_proof_or_correspondence(&self) -> bool {
        false
    }
}

/// A single inspectable inference step inside a [`DeductionSpec`] (`REASON-01`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivationStep {
    pub step_number: u32,
    pub rule_name: String,
    pub cited_input_versions: Vec<VersionId>,
    pub required_assumptions: Vec<String>,
    pub inferred_statement: String,
}

/// Deductive derivation exposing premises, assumptions, rules, and verification tier (`REASON-01`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeductionSpec {
    pub rule_system: String,
    pub explicit_assumptions: Vec<String>,
    pub derivation_steps: Vec<DerivationStep>,
    pub conditional_conclusion: String,
    pub verification_tier: ReasoningVerificationTier,
    pub contradictions_noted: Vec<String>,
}

/// Extrapolation specification stating observed domain, projection horizon, model,
/// uncertainty method, and sensitivity (`REASON-02`, `A07`).
#[derive(Debug, Clone, PartialEq)]
pub struct ExtrapolationSpec {
    pub domain_variable: String,
    pub domain_unit: String,
    pub observed_domain_min: f64,
    pub observed_domain_max: f64,
    pub observed_sample_count: u32,
    pub projection_horizon_start: f64,
    pub projection_horizon_end: f64,
    pub value_unit: String,
    pub projected_value: f64,
    pub uncertainty_lower: f64,
    pub uncertainty_upper: f64,
    pub model_name: String,
    pub model_version: VersionId,
    pub uncertainty_method: String,
    pub sensitivity_summary: String,
    pub explicit_assumptions: Vec<String>,
}

/// Reasoning method family for a [`ReasoningStepRecord`] (`REASON-01`..=`REASON-04`).
#[derive(Debug, Clone, PartialEq)]
pub enum ReasoningStepKind {
    Abstraction(AbstractionSpec),
    Correlation(CorrelationSpec),
    Deduction(DeductionSpec),
    Extrapolation(ExtrapolationSpec),
}

impl ReasoningStepKind {
    /// Returns the method family name (`abstraction`, `correlation`, `deduction`, `extrapolation`).
    #[must_use]
    pub const fn family_name(&self) -> &'static str {
        match self {
            Self::Abstraction(_) => "abstraction",
            Self::Correlation(_) => "correlation",
            Self::Deduction(_) => "deduction",
            Self::Extrapolation(_) => "extrapolation",
        }
    }
}

/// Explicit reference to an upstream premise version or reasoning step version (`REASON-04`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepInputRef {
    pub node_version: VersionId,
    /// Caller must declare the exact [`EvidenceOriginClass`] of the upstream node so
    /// inferences fed into later steps cannot silently masquerade as observations.
    pub declared_origin_class: EvidenceOriginClass,
}

/// Input specification for adding or revising a reasoning step in a [`ReasoningGraph`].
#[derive(Debug, Clone, PartialEq)]
pub struct ReasoningStepDraft {
    pub step_id: OpaqueId,
    pub version_id: VersionId,
    pub predecessor_version: Option<VersionId>,
    pub schema_version: SchemaVersion,
    pub actor: ActorId,
    pub inputs: Vec<StepInputRef>,
    pub kind: ReasoningStepKind,
    pub revision_rationale: Option<String>,
}

/// Why a downstream reasoning step or competing hypothesis requires reassessment (`REASON-04`, `A07`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReassessmentTrigger {
    PremiseRevised {
        premise_id: OpaqueId,
        old_version: VersionId,
        new_version: VersionId,
        rationale: String,
    },
    UpstreamStepRevised {
        step_id: OpaqueId,
        old_version: VersionId,
        new_version: VersionId,
        rationale: String,
    },
    AbstractionDecisionUpdated {
        step_id: OpaqueId,
        old_version: VersionId,
        new_version: VersionId,
        target_id: OpaqueId,
        rationale: String,
    },
}

/// Immutable versioned reasoning step in a [`ReasoningGraph`] (`REASON-01`..=`REASON-04`).
#[derive(Debug, Clone, PartialEq)]
pub struct ReasoningStepRecord {
    step_id: OpaqueId,
    version_id: VersionId,
    predecessor_version: Option<VersionId>,
    vintage_number: u32,
    schema_version: SchemaVersion,
    actor: ActorId,
    inputs: Vec<StepInputRef>,
    inherited_origin_classes: Vec<EvidenceOriginClass>,
    contains_non_observational_inputs: bool,
    kind: ReasoningStepKind,
    revision_rationale: Option<String>,
    needs_reassessment: bool,
    reassessment_triggers: Vec<ReassessmentTrigger>,
}

impl ReasoningStepRecord {
    #[must_use]
    pub const fn step_id(&self) -> &OpaqueId {
        &self.step_id
    }

    #[must_use]
    pub const fn version_id(&self) -> &VersionId {
        &self.version_id
    }

    #[must_use]
    pub const fn predecessor_version(&self) -> Option<&VersionId> {
        self.predecessor_version.as_ref()
    }

    #[must_use]
    pub const fn vintage_number(&self) -> u32 {
        self.vintage_number
    }

    #[must_use]
    pub const fn schema_version(&self) -> SchemaVersion {
        self.schema_version
    }

    #[must_use]
    pub const fn actor(&self) -> &ActorId {
        &self.actor
    }

    #[must_use]
    pub fn inputs(&self) -> &[StepInputRef] {
        &self.inputs
    }

    /// Every reasoning step output is an [`EvidenceOriginClass::AnalyticalInference`]
    /// and never promotes itself to an observation (`AGENTS.md` §2, `REASON-04`).
    #[must_use]
    pub const fn output_origin_class(&self) -> EvidenceOriginClass {
        EvidenceOriginClass::AnalyticalInference
    }

    /// Returns `false` unconditionally: a reasoning step output is never an observation.
    #[must_use]
    pub const fn can_serve_as_observation(&self) -> bool {
        false
    }

    /// Distinct origin classes present across the direct and transitive inputs of this step.
    #[must_use]
    pub fn inherited_origin_classes(&self) -> &[EvidenceOriginClass] {
        &self.inherited_origin_classes
    }

    /// `true` if any input (directly or transitively) is an inference, forecast, predictive
    /// derivative, simulation, or recreation rather than a direct source/observation.
    #[must_use]
    pub const fn contains_non_observational_inputs(&self) -> bool {
        self.contains_non_observational_inputs
    }

    #[must_use]
    pub const fn kind(&self) -> &ReasoningStepKind {
        &self.kind
    }

    #[must_use]
    pub fn revision_rationale(&self) -> Option<&str> {
        self.revision_rationale.as_deref()
    }

    #[must_use]
    pub const fn needs_reassessment(&self) -> bool {
        self.needs_reassessment
    }

    #[must_use]
    pub fn reassessment_triggers(&self) -> &[ReassessmentTrigger] {
        &self.reassessment_triggers
    }

    /// Mandatory methodological disclosure for this step's family (`REASON-01`, `REASON-02`).
    #[must_use]
    pub const fn mandatory_notice(&self) -> &'static str {
        match &self.kind {
            ReasoningStepKind::Abstraction(_) => {
                "NORMALIZED ABSTRACTION — VERBATIM SOURCE WORDING, UNKNOWNS, AND CONFLICTS RETAINED"
            }
            ReasoningStepKind::Correlation(_) => CORRELATION_NON_CAUSATION_NOTICE,
            ReasoningStepKind::Deduction(_) => DEDUCTION_CONDITIONALITY_NOTICE,
            ReasoningStepKind::Extrapolation(_) => EXTRAPOLATION_PROJECTION_NOTICE,
        }
    }
}

/// Evidence that would discriminate between competing explanations (`REASON-04`, `A07`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscriminatingEvidenceLead {
    pub lead_id: OpaqueId,
    pub description: String,
    pub expected_if_hypothesis_holds: String,
    pub expected_if_alternative_holds: String,
}

/// Input specification for registering a competing hypothesis (`REASON-04`, `A07`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompetingHypothesisDraft {
    pub hypothesis_id: OpaqueId,
    pub version_id: VersionId,
    pub schema_version: SchemaVersion,
    pub actor: ActorId,
    pub title: String,
    pub explanation: String,
    pub supporting_node_versions: Vec<VersionId>,
    pub contradicting_node_versions: Vec<VersionId>,
    pub explicit_assumptions: Vec<String>,
    pub discriminating_leads: Vec<DiscriminatingEvidenceLead>,
}

/// Immutable competing hypothesis record retained alongside alternative explanations (`REASON-04`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompetingHypothesisRecord {
    hypothesis_id: OpaqueId,
    version_id: VersionId,
    schema_version: SchemaVersion,
    actor: ActorId,
    title: String,
    explanation: String,
    supporting_node_versions: Vec<VersionId>,
    contradicting_node_versions: Vec<VersionId>,
    explicit_assumptions: Vec<String>,
    discriminating_leads: Vec<DiscriminatingEvidenceLead>,
    needs_reassessment: bool,
    reassessment_triggers: Vec<ReassessmentTrigger>,
}

impl CompetingHypothesisRecord {
    #[must_use]
    pub const fn hypothesis_id(&self) -> &OpaqueId {
        &self.hypothesis_id
    }

    #[must_use]
    pub const fn version_id(&self) -> &VersionId {
        &self.version_id
    }

    #[must_use]
    pub const fn schema_version(&self) -> SchemaVersion {
        self.schema_version
    }

    #[must_use]
    pub const fn actor(&self) -> &ActorId {
        &self.actor
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub fn explanation(&self) -> &str {
        &self.explanation
    }

    #[must_use]
    pub fn supporting_node_versions(&self) -> &[VersionId] {
        &self.supporting_node_versions
    }

    #[must_use]
    pub fn contradicting_node_versions(&self) -> &[VersionId] {
        &self.contradicting_node_versions
    }

    #[must_use]
    pub fn explicit_assumptions(&self) -> &[String] {
        &self.explicit_assumptions
    }

    #[must_use]
    pub fn discriminating_leads(&self) -> &[DiscriminatingEvidenceLead] {
        &self.discriminating_leads
    }

    #[must_use]
    pub const fn needs_reassessment(&self) -> bool {
        self.needs_reassessment
    }

    #[must_use]
    pub fn reassessment_triggers(&self) -> &[ReassessmentTrigger] {
        &self.reassessment_triggers
    }
}

fn is_disallowed_control_or_format(ch: char, multiline: bool) -> bool {
    if ch.is_control() && !(multiline && matches!(ch, '\n' | '\t')) {
        return true;
    }
    matches!(
        ch,
        '\u{0085}'
            | '\u{061C}'
            | '\u{180E}'
            | '\u{200B}'
            | '\u{200E}'
            | '\u{200F}'
            | '\u{2028}'..='\u{202E}'
            | '\u{2060}'..='\u{2069}'
            | '\u{3164}'
            | '\u{FEFF}'
            | '\u{FFF9}'..='\u{FFFB}'
            | '\u{E0000}'..='\u{E007F}'
    )
}

fn validate_text(
    field: &'static str,
    text: &str,
    max_bytes: usize,
    multiline: bool,
) -> Result<(), ReasoningError> {
    if text.len() > max_bytes {
        return Err(ReasoningError::BudgetExceeded {
            limit: ReasoningLimitKind::TextBytes,
            actual: text.len(),
            max: max_bytes,
        });
    }
    if text.trim().is_empty() {
        return Err(ReasoningError::EmptyField(field));
    }
    if text
        .chars()
        .any(|ch| is_disallowed_control_or_format(ch, multiline))
    {
        return Err(ReasoningError::ControlCharacter(field));
    }
    Ok(())
}

fn validate_finite(field: &'static str, value: f64) -> Result<(), ReasoningError> {
    if !value.is_finite() {
        return Err(ReasoningError::InvalidNumericValue(field));
    }
    Ok(())
}

/// Bounded, acyclic composed-reasoning graph (`REASON-01`..=`REASON-04`, `A07`).
#[derive(Debug, Clone)]
pub struct ReasoningGraph {
    limits: ReasoningLimits,
    premises_by_version: BTreeMap<VersionId, PremiseRecord>,
    premise_history: BTreeMap<OpaqueId, Vec<VersionId>>,
    steps_by_version: BTreeMap<VersionId, ReasoningStepRecord>,
    step_history: BTreeMap<OpaqueId, Vec<VersionId>>,
    hypotheses_by_version: BTreeMap<VersionId, CompetingHypothesisRecord>,
    hypothesis_ids: BTreeSet<OpaqueId>,
    all_versions: BTreeSet<VersionId>,
}

impl ReasoningGraph {
    /// Creates an empty [`ReasoningGraph`] governed by `limits`.
    #[must_use]
    pub const fn new(limits: ReasoningLimits) -> Self {
        Self {
            limits,
            premises_by_version: BTreeMap::new(),
            premise_history: BTreeMap::new(),
            steps_by_version: BTreeMap::new(),
            step_history: BTreeMap::new(),
            hypotheses_by_version: BTreeMap::new(),
            hypothesis_ids: BTreeSet::new(),
            all_versions: BTreeSet::new(),
        }
    }

    #[must_use]
    pub const fn limits(&self) -> ReasoningLimits {
        self.limits
    }

    #[must_use]
    pub fn premise(&self, version: &VersionId) -> Option<&PremiseRecord> {
        self.premises_by_version.get(version)
    }

    #[must_use]
    pub fn premise_versions(&self, premise_id: &OpaqueId) -> Option<&[VersionId]> {
        self.premise_history.get(premise_id).map(Vec::as_slice)
    }

    #[must_use]
    pub fn step(&self, version: &VersionId) -> Option<&ReasoningStepRecord> {
        self.steps_by_version.get(version)
    }

    #[must_use]
    pub fn step_versions(&self, step_id: &OpaqueId) -> Option<&[VersionId]> {
        self.step_history.get(step_id).map(Vec::as_slice)
    }

    #[must_use]
    pub fn hypothesis(&self, version: &VersionId) -> Option<&CompetingHypothesisRecord> {
        self.hypotheses_by_version.get(version)
    }

    /// Returns all competing hypotheses in deterministic version-ID order.
    pub fn hypotheses(&self) -> impl Iterator<Item = &CompetingHypothesisRecord> {
        self.hypotheses_by_version.values()
    }

    /// Registers an initial premise (`predecessor_version: None`).
    pub fn register_premise(&mut self, draft: PremiseDraft) -> Result<VersionId, ReasoningError> {
        if self.premise_history.contains_key(&draft.premise_id) {
            return Err(ReasoningError::DuplicatePremiseId(draft.premise_id));
        }
        if draft.predecessor_version.is_some() {
            return Err(ReasoningError::PremisePredecessorMismatch {
                premise_id: draft.premise_id,
                expected_latest: draft.version_id,
                supplied: draft.predecessor_version,
            });
        }
        self.insert_premise_internal(draft, 1)
    }

    /// Revises an existing premise, preserving all prior premise versions and transitively
    /// marking every dependent reasoning step and competing hypothesis `needs_reassessment = true`
    /// without mutating their historical outputs (`REASON-04`, `A07`).
    pub fn revise_premise(&mut self, draft: PremiseDraft) -> Result<VersionId, ReasoningError> {
        let Some(history) = self.premise_history.get(&draft.premise_id) else {
            return Err(ReasoningError::UnknownPremiseVersion(
                draft.predecessor_version.unwrap_or(draft.version_id),
            ));
        };
        let latest_version = history[history.len() - 1].clone();
        if draft.predecessor_version.as_ref() != Some(&latest_version) {
            return Err(ReasoningError::PremisePredecessorMismatch {
                premise_id: draft.premise_id,
                expected_latest: latest_version,
                supplied: draft.predecessor_version,
            });
        }
        let Some(ref rationale) = draft.revision_rationale else {
            return Err(ReasoningError::EmptyField("revision_rationale"));
        };
        validate_text(
            "revision_rationale",
            rationale,
            self.limits.max_text_bytes,
            true,
        )?;
        let rationale_owned = rationale.clone();
        let premise_id = draft.premise_id.clone();
        let next_vintage = u32::try_from(history.len() + 1).unwrap_or(u32::MAX);
        let new_version = self.insert_premise_internal(draft, next_vintage)?;

        let trigger = ReassessmentTrigger::PremiseRevised {
            premise_id,
            old_version: latest_version.clone(),
            new_version: new_version.clone(),
            rationale: rationale_owned,
        };
        self.propagate_reassessment(&latest_version, &trigger);
        Ok(new_version)
    }

    fn insert_premise_internal(
        &mut self,
        draft: PremiseDraft,
        vintage_number: u32,
    ) -> Result<VersionId, ReasoningError> {
        if self.premises_by_version.len() >= self.limits.max_premises {
            return Err(ReasoningError::BudgetExceeded {
                limit: ReasoningLimitKind::Premises,
                actual: self.premises_by_version.len() + 1,
                max: self.limits.max_premises,
            });
        }
        if self.all_versions.contains(&draft.version_id) {
            return Err(ReasoningError::DuplicateVersionId(draft.version_id));
        }
        validate_text(
            "citation_anchor",
            &draft.citation_anchor,
            self.limits.max_text_bytes,
            false,
        )?;
        validate_text(
            "verbatim_excerpt",
            &draft.verbatim_excerpt,
            self.limits.max_text_bytes,
            true,
        )?;
        validate_text(
            "proposition",
            &draft.proposition,
            self.limits.max_text_bytes,
            true,
        )?;
        validate_text(
            "uncertainty_note",
            &draft.uncertainty_note,
            self.limits.max_text_bytes,
            true,
        )?;

        if let Some(ref qty) = draft.quantity {
            validate_text(
                "quantity.unit",
                &qty.unit,
                self.limits.max_text_bytes,
                false,
            )?;
            validate_finite("quantity.value", qty.value)?;
            validate_finite("quantity.domain_min", qty.domain_min)?;
            validate_finite("quantity.domain_max", qty.domain_max)?;
            if qty.domain_min > qty.domain_max {
                return Err(ReasoningError::InvalidObservedDomain);
            }
            if qty.value < qty.domain_min || qty.value > qty.domain_max {
                return Err(ReasoningError::UnsupportedDomainValue {
                    field: "premise.quantity.value",
                    value: format!("{}", qty.value),
                    domain_min: format!("{}", qty.domain_min),
                    domain_max: format!("{}", qty.domain_max),
                });
            }
            if let Some(unc) = qty.uncertainty {
                validate_finite("quantity.uncertainty", unc)?;
                if unc < 0.0 {
                    return Err(ReasoningError::InvalidNumericValue("quantity.uncertainty"));
                }
            }
        }

        let version_id = draft.version_id.clone();
        let premise_id = draft.premise_id.clone();
        let record = PremiseRecord {
            premise_id: draft.premise_id,
            version_id: draft.version_id,
            predecessor_version: draft.predecessor_version,
            vintage_number,
            schema_version: draft.schema_version,
            actor: draft.actor,
            origin_class: draft.origin_class,
            source_version: draft.source_version,
            citation_anchor: draft.citation_anchor,
            verbatim_excerpt: draft.verbatim_excerpt,
            proposition: draft.proposition,
            quantity: draft.quantity,
            uncertainty_note: draft.uncertainty_note,
            revision_rationale: draft.revision_rationale,
        };

        self.all_versions.insert(version_id.clone());
        self.premises_by_version.insert(version_id.clone(), record);
        self.premise_history
            .entry(premise_id)
            .or_default()
            .push(version_id.clone());
        Ok(version_id)
    }

    /// Adds a new reasoning step (`predecessor_version: None`) or a revised reasoning step
    /// (`predecessor_version: Some(latest)`).
    pub fn add_step(&mut self, draft: ReasoningStepDraft) -> Result<VersionId, ReasoningError> {
        if self.steps_by_version.len() >= self.limits.max_steps {
            return Err(ReasoningError::BudgetExceeded {
                limit: ReasoningLimitKind::Steps,
                actual: self.steps_by_version.len() + 1,
                max: self.limits.max_steps,
            });
        }
        if self.all_versions.contains(&draft.version_id) {
            return Err(ReasoningError::DuplicateVersionId(draft.version_id));
        }

        let vintage_number = match self.step_history.get(&draft.step_id) {
            None => {
                if draft.predecessor_version.is_some() {
                    return Err(ReasoningError::StepPredecessorMismatch {
                        step_id: draft.step_id,
                        expected_latest: draft.version_id,
                        supplied: draft.predecessor_version,
                    });
                }
                1
            }
            Some(history) => {
                let latest = history[history.len() - 1].clone();
                if draft.predecessor_version.as_ref() != Some(&latest) {
                    return Err(ReasoningError::StepPredecessorMismatch {
                        step_id: draft.step_id,
                        expected_latest: latest,
                        supplied: draft.predecessor_version,
                    });
                }
                let prev_step = &self.steps_by_version[&latest];
                if prev_step.kind.family_name() != draft.kind.family_name() {
                    return Err(ReasoningError::StepKindChangeForbidden {
                        step_id: draft.step_id,
                    });
                }
                let Some(ref rationale) = draft.revision_rationale else {
                    return Err(ReasoningError::EmptyField("revision_rationale"));
                };
                validate_text(
                    "revision_rationale",
                    rationale,
                    self.limits.max_text_bytes,
                    true,
                )?;
                u32::try_from(history.len() + 1).unwrap_or(u32::MAX)
            }
        };

        if draft.inputs.is_empty() {
            return Err(ReasoningError::EmptyStepInputs);
        }
        if draft.inputs.len() > self.limits.max_items_per_step {
            return Err(ReasoningError::BudgetExceeded {
                limit: ReasoningLimitKind::ItemsPerStep,
                actual: draft.inputs.len(),
                max: self.limits.max_items_per_step,
            });
        }

        let mut seen_inputs = BTreeSet::new();
        let mut inherited_origins = BTreeSet::new();
        let mut upstream_needs_reassessment = false;
        let mut inherited_triggers = Vec::new();

        for input in &draft.inputs {
            if !seen_inputs.insert(input.node_version.clone()) {
                return Err(ReasoningError::DuplicateStepInput(
                    input.node_version.clone(),
                ));
            }
            if let Some(premise) = self.premises_by_version.get(&input.node_version) {
                let actual = premise.origin_class();
                if input.declared_origin_class != actual {
                    return Err(ReasoningError::InputOriginClassMismatch {
                        node_version: input.node_version.clone(),
                        declared: input.declared_origin_class,
                        actual,
                    });
                }
                inherited_origins.insert(actual);
                if let Some(history) = self.premise_history.get(premise.premise_id())
                    && history.last() != Some(&input.node_version)
                {
                    let newest_ver = history[history.len() - 1].clone();
                    let newest_rec = &self.premises_by_version[&newest_ver];
                    upstream_needs_reassessment = true;
                    inherited_triggers.push(ReassessmentTrigger::PremiseRevised {
                        premise_id: premise.premise_id().clone(),
                        old_version: input.node_version.clone(),
                        new_version: newest_ver,
                        rationale: newest_rec
                            .revision_rationale()
                            .unwrap_or("superseded premise")
                            .to_owned(),
                    });
                }
            } else if let Some(step) = self.steps_by_version.get(&input.node_version) {
                let actual = step.output_origin_class();
                if input.declared_origin_class != actual {
                    return Err(ReasoningError::InputOriginClassMismatch {
                        node_version: input.node_version.clone(),
                        declared: input.declared_origin_class,
                        actual,
                    });
                }
                inherited_origins.insert(actual);
                for origin in step.inherited_origin_classes() {
                    inherited_origins.insert(*origin);
                }
                if step.needs_reassessment() {
                    upstream_needs_reassessment = true;
                    for trig in step.reassessment_triggers() {
                        if !inherited_triggers.contains(trig) {
                            inherited_triggers.push(trig.clone());
                        }
                    }
                }
            } else {
                return Err(ReasoningError::UnknownInputNodeVersion(
                    input.node_version.clone(),
                ));
            }
        }

        self.validate_step_kind(&draft.inputs, &seen_inputs, &draft.kind)?;

        let inherited_origin_classes: Vec<EvidenceOriginClass> =
            inherited_origins.into_iter().collect();
        let contains_non_observational_inputs = inherited_origin_classes
            .iter()
            .any(|origin| origin.requires_non_observational_qualification());

        let predecessor_version = draft.predecessor_version.clone();
        let step_id = draft.step_id.clone();
        let version_id = draft.version_id.clone();
        let rationale_for_trigger = draft.revision_rationale.clone();

        let record = ReasoningStepRecord {
            step_id: draft.step_id,
            version_id: draft.version_id,
            predecessor_version: draft.predecessor_version,
            vintage_number,
            schema_version: draft.schema_version,
            actor: draft.actor,
            inputs: draft.inputs,
            inherited_origin_classes,
            contains_non_observational_inputs,
            kind: draft.kind,
            revision_rationale: draft.revision_rationale,
            needs_reassessment: upstream_needs_reassessment,
            reassessment_triggers: inherited_triggers,
        };

        self.all_versions.insert(version_id.clone());
        self.steps_by_version.insert(version_id.clone(), record);
        self.step_history
            .entry(step_id.clone())
            .or_default()
            .push(version_id.clone());

        if let Some(old_version) = predecessor_version {
            let trigger = ReassessmentTrigger::UpstreamStepRevised {
                step_id,
                old_version: old_version.clone(),
                new_version: version_id.clone(),
                rationale: rationale_for_trigger.unwrap_or_default(),
            };
            self.propagate_reassessment(&old_version, &trigger);
        }

        Ok(version_id)
    }

    /// Reverses or updates an entity-match decision in an abstraction step by issuing a new
    /// immutable version of that abstraction step and flagging downstream dependents for
    /// reassessment (`REASON-03`, `REASON-04`, `A07`, `A08`).
    pub fn update_entity_match_decision(
        &mut self,
        abstraction_step_version: &VersionId,
        candidate_id: &OpaqueId,
        new_version_id: VersionId,
        actor: ActorId,
        new_decision: DecisionState,
    ) -> Result<VersionId, ReasoningError> {
        if self.steps_by_version.len() >= self.limits.max_steps {
            return Err(ReasoningError::BudgetExceeded {
                limit: ReasoningLimitKind::Steps,
                actual: self.steps_by_version.len() + 1,
                max: self.limits.max_steps,
            });
        }
        if self.all_versions.contains(&new_version_id) {
            return Err(ReasoningError::DuplicateVersionId(new_version_id));
        }
        let rationale = validate_decision_state(&new_decision, self.limits.max_text_bytes)?;

        let Some(existing) = self.steps_by_version.get(abstraction_step_version).cloned() else {
            return Err(ReasoningError::UnknownStepVersion(
                abstraction_step_version.clone(),
            ));
        };
        let history = &self.step_history[existing.step_id()];
        let latest = &history[history.len() - 1];
        if latest != abstraction_step_version {
            return Err(ReasoningError::StepPredecessorMismatch {
                step_id: existing.step_id().clone(),
                expected_latest: latest.clone(),
                supplied: Some(abstraction_step_version.clone()),
            });
        }

        let ReasoningStepKind::Abstraction(mut spec) = existing.kind else {
            return Err(ReasoningError::StepKindChangeForbidden {
                step_id: existing.step_id,
            });
        };

        let Some(candidate) = spec
            .entity_matches
            .iter_mut()
            .find(|c| &c.candidate_id == candidate_id)
        else {
            return Err(ReasoningError::UnknownEntityCandidate(candidate_id.clone()));
        };
        candidate.decision = new_decision;

        let next_vintage = u32::try_from(history.len() + 1).unwrap_or(u32::MAX);
        let step_id = existing.step_id.clone();
        let record = ReasoningStepRecord {
            step_id: step_id.clone(),
            version_id: new_version_id.clone(),
            predecessor_version: Some(abstraction_step_version.clone()),
            vintage_number: next_vintage,
            schema_version: existing.schema_version,
            actor,
            inputs: existing.inputs,
            inherited_origin_classes: existing.inherited_origin_classes,
            contains_non_observational_inputs: existing.contains_non_observational_inputs,
            kind: ReasoningStepKind::Abstraction(spec),
            revision_rationale: Some(rationale.clone()),
            needs_reassessment: existing.needs_reassessment,
            reassessment_triggers: existing.reassessment_triggers,
        };

        self.all_versions.insert(new_version_id.clone());
        self.steps_by_version.insert(new_version_id.clone(), record);
        self.step_history
            .entry(step_id.clone())
            .or_default()
            .push(new_version_id.clone());

        let trigger = ReassessmentTrigger::AbstractionDecisionUpdated {
            step_id,
            old_version: abstraction_step_version.clone(),
            new_version: new_version_id.clone(),
            target_id: candidate_id.clone(),
            rationale,
        };
        self.propagate_reassessment(abstraction_step_version, &trigger);

        Ok(new_version_id)
    }

    /// Reverses or updates a relationship decision in an abstraction step by issuing a new
    /// immutable version of that abstraction step (`REASON-03`, `REASON-04`).
    pub fn update_relationship_decision(
        &mut self,
        abstraction_step_version: &VersionId,
        relationship_id: &OpaqueId,
        new_version_id: VersionId,
        actor: ActorId,
        new_decision: DecisionState,
    ) -> Result<VersionId, ReasoningError> {
        if self.steps_by_version.len() >= self.limits.max_steps {
            return Err(ReasoningError::BudgetExceeded {
                limit: ReasoningLimitKind::Steps,
                actual: self.steps_by_version.len() + 1,
                max: self.limits.max_steps,
            });
        }
        if self.all_versions.contains(&new_version_id) {
            return Err(ReasoningError::DuplicateVersionId(new_version_id));
        }
        let rationale = validate_decision_state(&new_decision, self.limits.max_text_bytes)?;

        let Some(existing) = self.steps_by_version.get(abstraction_step_version).cloned() else {
            return Err(ReasoningError::UnknownStepVersion(
                abstraction_step_version.clone(),
            ));
        };
        let history = &self.step_history[existing.step_id()];
        let latest = &history[history.len() - 1];
        if latest != abstraction_step_version {
            return Err(ReasoningError::StepPredecessorMismatch {
                step_id: existing.step_id().clone(),
                expected_latest: latest.clone(),
                supplied: Some(abstraction_step_version.clone()),
            });
        }

        let ReasoningStepKind::Abstraction(mut spec) = existing.kind else {
            return Err(ReasoningError::StepKindChangeForbidden {
                step_id: existing.step_id,
            });
        };

        let Some(rel) = spec
            .relationships
            .iter_mut()
            .find(|r| &r.relationship_id == relationship_id)
        else {
            return Err(ReasoningError::UnknownRelationship(relationship_id.clone()));
        };
        rel.decision = new_decision;

        let next_vintage = u32::try_from(history.len() + 1).unwrap_or(u32::MAX);
        let step_id = existing.step_id.clone();
        let record = ReasoningStepRecord {
            step_id: step_id.clone(),
            version_id: new_version_id.clone(),
            predecessor_version: Some(abstraction_step_version.clone()),
            vintage_number: next_vintage,
            schema_version: existing.schema_version,
            actor,
            inputs: existing.inputs,
            inherited_origin_classes: existing.inherited_origin_classes,
            contains_non_observational_inputs: existing.contains_non_observational_inputs,
            kind: ReasoningStepKind::Abstraction(spec),
            revision_rationale: Some(rationale.clone()),
            needs_reassessment: existing.needs_reassessment,
            reassessment_triggers: existing.reassessment_triggers,
        };

        self.all_versions.insert(new_version_id.clone());
        self.steps_by_version.insert(new_version_id.clone(), record);
        self.step_history
            .entry(step_id.clone())
            .or_default()
            .push(new_version_id.clone());

        let trigger = ReassessmentTrigger::AbstractionDecisionUpdated {
            step_id,
            old_version: abstraction_step_version.clone(),
            new_version: new_version_id.clone(),
            target_id: relationship_id.clone(),
            rationale,
        };
        self.propagate_reassessment(abstraction_step_version, &trigger);

        Ok(new_version_id)
    }

    /// Registers a competing hypothesis with supporting and contradicting evidence links,
    /// explicit assumptions, and discriminating evidence leads (`REASON-04`, `A07`).
    pub fn register_hypothesis(
        &mut self,
        draft: CompetingHypothesisDraft,
    ) -> Result<VersionId, ReasoningError> {
        if self.hypotheses_by_version.len() >= self.limits.max_hypotheses {
            return Err(ReasoningError::BudgetExceeded {
                limit: ReasoningLimitKind::Hypotheses,
                actual: self.hypotheses_by_version.len() + 1,
                max: self.limits.max_hypotheses,
            });
        }
        if self.hypothesis_ids.contains(&draft.hypothesis_id) {
            return Err(ReasoningError::DuplicateHypothesisId(draft.hypothesis_id));
        }
        if self.all_versions.contains(&draft.version_id) {
            return Err(ReasoningError::DuplicateVersionId(draft.version_id));
        }

        validate_text("title", &draft.title, self.limits.max_text_bytes, false)?;
        validate_text(
            "explanation",
            &draft.explanation,
            self.limits.max_text_bytes,
            true,
        )?;

        if draft.supporting_node_versions.is_empty() {
            return Err(ReasoningError::EmptyHypothesisLinks(
                "supporting_node_versions",
            ));
        }
        if draft.supporting_node_versions.len() > self.limits.max_items_per_step {
            return Err(ReasoningError::BudgetExceeded {
                limit: ReasoningLimitKind::ItemsPerStep,
                actual: draft.supporting_node_versions.len(),
                max: self.limits.max_items_per_step,
            });
        }
        if draft.contradicting_node_versions.len() > self.limits.max_items_per_step {
            return Err(ReasoningError::BudgetExceeded {
                limit: ReasoningLimitKind::ItemsPerStep,
                actual: draft.contradicting_node_versions.len(),
                max: self.limits.max_items_per_step,
            });
        }
        if draft.explicit_assumptions.len() > self.limits.max_items_per_step {
            return Err(ReasoningError::BudgetExceeded {
                limit: ReasoningLimitKind::ItemsPerStep,
                actual: draft.explicit_assumptions.len(),
                max: self.limits.max_items_per_step,
            });
        }
        if draft.discriminating_leads.is_empty() {
            return Err(ReasoningError::EmptyDiscriminatingLeads);
        }
        if draft.discriminating_leads.len() > self.limits.max_items_per_step {
            return Err(ReasoningError::BudgetExceeded {
                limit: ReasoningLimitKind::ItemsPerStep,
                actual: draft.discriminating_leads.len(),
                max: self.limits.max_items_per_step,
            });
        }

        let mut support_set = BTreeSet::new();
        let mut needs_reassessment = false;
        let mut triggers = Vec::new();

        for ver in &draft.supporting_node_versions {
            if !support_set.insert(ver.clone()) {
                return Err(ReasoningError::DuplicateItemId {
                    context: "supporting_node_versions",
                    id: ver.as_str().to_owned(),
                });
            }
            self.inspect_node_for_hypothesis(ver, &mut needs_reassessment, &mut triggers)?;
        }

        let mut contradict_set = BTreeSet::new();
        for ver in &draft.contradicting_node_versions {
            if support_set.contains(ver) {
                return Err(ReasoningError::HypothesisSupportContradictionOverlap(
                    ver.clone(),
                ));
            }
            if !contradict_set.insert(ver.clone()) {
                return Err(ReasoningError::DuplicateItemId {
                    context: "contradicting_node_versions",
                    id: ver.as_str().to_owned(),
                });
            }
            self.inspect_node_for_hypothesis(ver, &mut needs_reassessment, &mut triggers)?;
        }

        for assumption in &draft.explicit_assumptions {
            validate_text(
                "explicit_assumption",
                assumption,
                self.limits.max_text_bytes,
                true,
            )?;
        }

        let mut seen_leads = BTreeSet::new();
        for lead in &draft.discriminating_leads {
            if !seen_leads.insert(lead.lead_id.clone()) {
                return Err(ReasoningError::DuplicateItemId {
                    context: "discriminating_lead",
                    id: lead.lead_id.as_str().to_owned(),
                });
            }
            validate_text(
                "lead.description",
                &lead.description,
                self.limits.max_text_bytes,
                true,
            )?;
            validate_text(
                "lead.expected_if_hypothesis_holds",
                &lead.expected_if_hypothesis_holds,
                self.limits.max_text_bytes,
                true,
            )?;
            validate_text(
                "lead.expected_if_alternative_holds",
                &lead.expected_if_alternative_holds,
                self.limits.max_text_bytes,
                true,
            )?;
        }

        let version_id = draft.version_id.clone();
        let hypothesis_id = draft.hypothesis_id.clone();
        let record = CompetingHypothesisRecord {
            hypothesis_id: draft.hypothesis_id,
            version_id: draft.version_id,
            schema_version: draft.schema_version,
            actor: draft.actor,
            title: draft.title,
            explanation: draft.explanation,
            supporting_node_versions: draft.supporting_node_versions,
            contradicting_node_versions: draft.contradicting_node_versions,
            explicit_assumptions: draft.explicit_assumptions,
            discriminating_leads: draft.discriminating_leads,
            needs_reassessment,
            reassessment_triggers: triggers,
        };

        self.all_versions.insert(version_id.clone());
        self.hypothesis_ids.insert(hypothesis_id);
        self.hypotheses_by_version
            .insert(version_id.clone(), record);
        Ok(version_id)
    }

    fn inspect_node_for_hypothesis(
        &self,
        version: &VersionId,
        needs_reassessment: &mut bool,
        triggers: &mut Vec<ReassessmentTrigger>,
    ) -> Result<(), ReasoningError> {
        if let Some(premise) = self.premises_by_version.get(version) {
            if let Some(history) = self.premise_history.get(premise.premise_id())
                && history.last() != Some(version)
            {
                let newest_ver = history[history.len() - 1].clone();
                let newest_rec = &self.premises_by_version[&newest_ver];
                *needs_reassessment = true;
                let trig = ReassessmentTrigger::PremiseRevised {
                    premise_id: premise.premise_id().clone(),
                    old_version: version.clone(),
                    new_version: newest_ver,
                    rationale: newest_rec
                        .revision_rationale()
                        .unwrap_or("superseded premise")
                        .to_owned(),
                };
                if !triggers.contains(&trig) {
                    triggers.push(trig);
                }
            }
            Ok(())
        } else if let Some(step) = self.steps_by_version.get(version) {
            if step.needs_reassessment() {
                *needs_reassessment = true;
                for trig in step.reassessment_triggers() {
                    if !triggers.contains(trig) {
                        triggers.push(trig.clone());
                    }
                }
            }
            if let Some(history) = self.step_history.get(step.step_id())
                && history.last() != Some(version)
            {
                let newest_ver = history[history.len() - 1].clone();
                let newest_rec = &self.steps_by_version[&newest_ver];
                *needs_reassessment = true;
                let trig = ReassessmentTrigger::UpstreamStepRevised {
                    step_id: step.step_id().clone(),
                    old_version: version.clone(),
                    new_version: newest_ver,
                    rationale: newest_rec
                        .revision_rationale()
                        .unwrap_or("superseded step")
                        .to_owned(),
                };
                if !triggers.contains(&trig) {
                    triggers.push(trig);
                }
            }
            Ok(())
        } else {
            Err(ReasoningError::UnknownInputNodeVersion(version.clone()))
        }
    }

    fn propagate_reassessment(
        &mut self,
        superseded_version: &VersionId,
        trigger: &ReassessmentTrigger,
    ) {
        let mut affected_versions = BTreeSet::new();
        affected_versions.insert(superseded_version.clone());

        let step_keys: Vec<VersionId> = self.steps_by_version.keys().cloned().collect();
        let mut changed = true;
        while changed {
            changed = false;
            for step_ver in &step_keys {
                if affected_versions.contains(step_ver) {
                    continue;
                }
                let depends_on_affected = self.steps_by_version[step_ver]
                    .inputs
                    .iter()
                    .any(|inp| affected_versions.contains(&inp.node_version));
                if depends_on_affected {
                    affected_versions.insert(step_ver.clone());
                    changed = true;
                }
            }
        }

        for step_ver in &affected_versions {
            if step_ver == superseded_version {
                continue;
            }
            if let Some(step) = self.steps_by_version.get_mut(step_ver) {
                step.needs_reassessment = true;
                if !step.reassessment_triggers.contains(trigger) {
                    step.reassessment_triggers.push(trigger.clone());
                }
            }
        }

        for hypothesis in self.hypotheses_by_version.values_mut() {
            let cites_affected = hypothesis
                .supporting_node_versions
                .iter()
                .chain(hypothesis.contradicting_node_versions.iter())
                .any(|ver| affected_versions.contains(ver));
            if cites_affected {
                hypothesis.needs_reassessment = true;
                if !hypothesis.reassessment_triggers.contains(trigger) {
                    hypothesis.reassessment_triggers.push(trigger.clone());
                }
            }
        }
    }

    fn validate_step_kind(
        &self,
        inputs: &[StepInputRef],
        seen_inputs: &BTreeSet<VersionId>,
        kind: &ReasoningStepKind,
    ) -> Result<(), ReasoningError> {
        let max_items = self.limits.max_items_per_step;
        let max_bytes = self.limits.max_text_bytes;

        match kind {
            ReasoningStepKind::Abstraction(spec) => {
                validate_text("event_schema", &spec.event_schema, max_bytes, false)?;
                if spec.fields.is_empty()
                    && spec.entity_matches.is_empty()
                    && spec.relationships.is_empty()
                    && spec.explicit_omissions.is_empty()
                {
                    return Err(ReasoningError::AbstractionMissingContent);
                }
                if spec.fields.len() > max_items
                    || spec.entity_matches.len() > max_items
                    || spec.relationships.len() > max_items
                    || spec.explicit_omissions.len() > max_items
                {
                    return Err(ReasoningError::BudgetExceeded {
                        limit: ReasoningLimitKind::ItemsPerStep,
                        actual: spec
                            .fields
                            .len()
                            .max(spec.entity_matches.len())
                            .max(spec.relationships.len())
                            .max(spec.explicit_omissions.len()),
                        max: max_items,
                    });
                }

                let mut field_names = BTreeSet::new();
                for field in &spec.fields {
                    validate_text("field_name", &field.field_name, max_bytes, false)?;
                    if !field_names.insert(field.field_name.clone()) {
                        return Err(ReasoningError::DuplicateItemId {
                            context: "abstraction_field",
                            id: field.field_name.clone(),
                        });
                    }
                    match &field.state {
                        AbstractionFieldState::Normalized {
                            normalized_value,
                            verbatim_wording,
                            source_version: _,
                        } => {
                            validate_text("normalized_value", normalized_value, max_bytes, false)?;
                            validate_text("verbatim_wording", verbatim_wording, max_bytes, true)?;
                        }
                        AbstractionFieldState::ExplicitUnknown {
                            reason,
                            verbatim_context,
                            source_version: _,
                        } => {
                            validate_text("unknown_reason", reason, max_bytes, true)?;
                            validate_text("verbatim_context", verbatim_context, max_bytes, true)?;
                        }
                        AbstractionFieldState::Conflicting {
                            primary_value,
                            primary_verbatim,
                            primary_source_version: _,
                            competing_value,
                            competing_verbatim,
                            competing_source_version: _,
                            conflict_note,
                        } => {
                            validate_text("primary_value", primary_value, max_bytes, false)?;
                            validate_text("primary_verbatim", primary_verbatim, max_bytes, true)?;
                            validate_text("competing_value", competing_value, max_bytes, false)?;
                            validate_text(
                                "competing_verbatim",
                                competing_verbatim,
                                max_bytes,
                                true,
                            )?;
                            validate_text("conflict_note", conflict_note, max_bytes, true)?;
                        }
                    }
                }

                let mut candidate_ids = BTreeSet::new();
                for candidate in &spec.entity_matches {
                    if !candidate_ids.insert(candidate.candidate_id.clone()) {
                        return Err(ReasoningError::DuplicateItemId {
                            context: "entity_match_candidate",
                            id: candidate.candidate_id.as_str().to_owned(),
                        });
                    }
                    validate_text(
                        "mention_verbatim",
                        &candidate.mention_verbatim,
                        max_bytes,
                        true,
                    )?;
                    validate_text(
                        "taxonomy_concept",
                        &candidate.taxonomy_concept,
                        max_bytes,
                        false,
                    )?;
                    validate_decision_state(&candidate.decision, max_bytes)?;
                }

                let mut rel_ids = BTreeSet::new();
                for rel in &spec.relationships {
                    if !rel_ids.insert(rel.relationship_id.clone()) {
                        return Err(ReasoningError::DuplicateItemId {
                            context: "abstraction_relationship",
                            id: rel.relationship_id.as_str().to_owned(),
                        });
                    }
                    validate_text("verbatim_basis", &rel.verbatim_basis, max_bytes, true)?;
                    validate_decision_state(&rel.decision, max_bytes)?;
                }

                for omission in &spec.explicit_omissions {
                    validate_text("explicit_omission", omission, max_bytes, true)?;
                }
                Ok(())
            }
            ReasoningStepKind::Correlation(spec) => {
                validate_finite(
                    "temporal_tolerance_seconds",
                    spec.temporal_tolerance_seconds,
                )?;
                if spec.temporal_tolerance_seconds <= 0.0 {
                    return Err(ReasoningError::InvalidTolerance(
                        "temporal_tolerance_seconds",
                    ));
                }
                validate_finite("spatial_tolerance_km", spec.spatial_tolerance_km)?;
                if spec.spatial_tolerance_km <= 0.0 {
                    return Err(ReasoningError::InvalidTolerance("spatial_tolerance_km"));
                }
                validate_text(
                    "attribute_matching_rule",
                    &spec.attribute_matching_rule,
                    max_bytes,
                    true,
                )?;
                if spec.matched_pairs.is_empty() {
                    return Err(ReasoningError::EmptyCorrelationPairs);
                }
                if spec.matched_pairs.len() > max_items {
                    return Err(ReasoningError::BudgetExceeded {
                        limit: ReasoningLimitKind::ItemsPerStep,
                        actual: spec.matched_pairs.len(),
                        max: max_items,
                    });
                }
                if spec.evaluated_pair_count < spec.matched_pairs.len() as u64 {
                    return Err(ReasoningError::InvalidRateOrCount("evaluated_pair_count"));
                }
                validate_finite("coverage_fraction", spec.coverage_fraction)?;
                if !(0.0..=1.0).contains(&spec.coverage_fraction) {
                    return Err(ReasoningError::InvalidCoverageFraction);
                }
                validate_text(
                    "coverage_limitations",
                    &spec.coverage_limitations,
                    max_bytes,
                    true,
                )?;
                match &spec.baseline_rate {
                    BaselineRateSpec::EmpiricalRate {
                        rate_per_window,
                        basis_description,
                    } => {
                        validate_finite("rate_per_window", *rate_per_window)?;
                        if !(0.0..=1.0).contains(rate_per_window) {
                            return Err(ReasoningError::InvalidRateOrCount("rate_per_window"));
                        }
                        validate_text("basis_description", basis_description, max_bytes, true)?;
                    }
                    BaselineRateSpec::Unavailable { reason } => {
                        validate_text("baseline_unavailable_reason", reason, max_bytes, true)?;
                    }
                }
                let reported_matches = u32::try_from(spec.matched_pairs.len()).unwrap_or(u32::MAX);
                if spec.tested_hypotheses_count == 0
                    || spec.tested_hypotheses_count < reported_matches
                {
                    return Err(ReasoningError::InvalidMultipleComparisonCount {
                        tested_hypotheses: spec.tested_hypotheses_count,
                        reported_matches,
                    });
                }
                validate_text(
                    "multiple_comparison_method",
                    &spec.multiple_comparison_method,
                    max_bytes,
                    true,
                )?;
                validate_text(
                    "correlation_summary",
                    &spec.correlation_summary,
                    max_bytes,
                    true,
                )?;

                for pair in &spec.matched_pairs {
                    if !seen_inputs.contains(&pair.left_version) {
                        return Err(ReasoningError::UnknownInputNodeVersion(
                            pair.left_version.clone(),
                        ));
                    }
                    if !seen_inputs.contains(&pair.right_version) {
                        return Err(ReasoningError::UnknownInputNodeVersion(
                            pair.right_version.clone(),
                        ));
                    }
                    validate_finite(
                        "observed_temporal_delta_seconds",
                        pair.observed_temporal_delta_seconds,
                    )?;
                    if pair.observed_temporal_delta_seconds < 0.0
                        || pair.observed_temporal_delta_seconds > spec.temporal_tolerance_seconds
                    {
                        return Err(ReasoningError::UnsupportedDomainValue {
                            field: "observed_temporal_delta_seconds",
                            value: format!("{}", pair.observed_temporal_delta_seconds),
                            domain_min: "0".to_owned(),
                            domain_max: format!("{}", spec.temporal_tolerance_seconds),
                        });
                    }
                    validate_finite("observed_spatial_delta_km", pair.observed_spatial_delta_km)?;
                    if pair.observed_spatial_delta_km < 0.0
                        || pair.observed_spatial_delta_km > spec.spatial_tolerance_km
                    {
                        return Err(ReasoningError::UnsupportedDomainValue {
                            field: "observed_spatial_delta_km",
                            value: format!("{}", pair.observed_spatial_delta_km),
                            domain_min: "0".to_owned(),
                            domain_max: format!("{}", spec.spatial_tolerance_km),
                        });
                    }
                    if let SourceDependencyStatus::UnverifiedIndependence { limitation_note } =
                        &pair.source_dependency
                    {
                        validate_text("limitation_note", limitation_note, max_bytes, true)?;
                    }
                }
                Ok(())
            }
            ReasoningStepKind::Deduction(spec) => {
                validate_text("rule_system", &spec.rule_system, max_bytes, false)?;
                if spec.derivation_steps.is_empty() {
                    return Err(ReasoningError::EmptyDeductionSteps);
                }
                if spec.explicit_assumptions.len() > max_items
                    || spec.derivation_steps.len() > max_items
                    || spec.contradictions_noted.len() > max_items
                {
                    return Err(ReasoningError::BudgetExceeded {
                        limit: ReasoningLimitKind::ItemsPerStep,
                        actual: spec
                            .explicit_assumptions
                            .len()
                            .max(spec.derivation_steps.len())
                            .max(spec.contradictions_noted.len()),
                        max: max_items,
                    });
                }
                for assumption in &spec.explicit_assumptions {
                    validate_text("explicit_assumption", assumption, max_bytes, true)?;
                }
                for (idx, dstep) in spec.derivation_steps.iter().enumerate() {
                    let expected_num = u32::try_from(idx + 1).unwrap_or(u32::MAX);
                    if dstep.step_number != expected_num {
                        return Err(ReasoningError::InvalidRateOrCount("step_number"));
                    }
                    validate_text("rule_name", &dstep.rule_name, max_bytes, false)?;
                    if dstep.cited_input_versions.is_empty() {
                        return Err(ReasoningError::EmptyDeductionPremises);
                    }
                    if dstep.cited_input_versions.len() > max_items
                        || dstep.required_assumptions.len() > max_items
                    {
                        return Err(ReasoningError::BudgetExceeded {
                            limit: ReasoningLimitKind::ItemsPerStep,
                            actual: dstep
                                .cited_input_versions
                                .len()
                                .max(dstep.required_assumptions.len()),
                            max: max_items,
                        });
                    }
                    for cited in &dstep.cited_input_versions {
                        if !seen_inputs.contains(cited) {
                            return Err(ReasoningError::UnknownInputNodeVersion(cited.clone()));
                        }
                    }
                    for req in &dstep.required_assumptions {
                        validate_text("required_assumption", req, max_bytes, true)?;
                    }
                    validate_text(
                        "inferred_statement",
                        &dstep.inferred_statement,
                        max_bytes,
                        true,
                    )?;
                }
                validate_text(
                    "conditional_conclusion",
                    &spec.conditional_conclusion,
                    max_bytes,
                    true,
                )?;
                match &spec.verification_tier {
                    ReasoningVerificationTier::AiProposed {
                        model_id,
                        prompt_template_version: _,
                    } => {
                        validate_text("model_id", model_id, max_bytes, false)?;
                    }
                    ReasoningVerificationTier::HumanReviewed {
                        reviewer: _,
                        review_note,
                    } => {
                        validate_text("review_note", review_note, max_bytes, true)?;
                    }
                    ReasoningVerificationTier::AssertedUnverifiedLeanRef {
                        theorem_id,
                        lean_toolchain,
                        model_revision,
                        assumptions_summary,
                    } => {
                        validate_text("theorem_id", theorem_id, max_bytes, false)?;
                        validate_text("lean_toolchain", lean_toolchain, max_bytes, false)?;
                        validate_text("model_revision", model_revision, max_bytes, false)?;
                        validate_text("assumptions_summary", assumptions_summary, max_bytes, true)?;
                        if theorem_id.contains("sorry") || theorem_id.contains("admit") {
                            return Err(ReasoningError::InvalidVerificationMetadata("theorem_id"));
                        }
                    }
                }
                for contradiction in &spec.contradictions_noted {
                    validate_text("contradiction_noted", contradiction, max_bytes, true)?;
                }
                Ok(())
            }
            ReasoningStepKind::Extrapolation(spec) => {
                validate_text("domain_variable", &spec.domain_variable, max_bytes, false)?;
                validate_text("domain_unit", &spec.domain_unit, max_bytes, false)?;
                validate_finite("observed_domain_min", spec.observed_domain_min)?;
                validate_finite("observed_domain_max", spec.observed_domain_max)?;
                if spec.observed_domain_min >= spec.observed_domain_max
                    || spec.observed_sample_count < 2
                {
                    return Err(ReasoningError::InvalidObservedDomain);
                }
                validate_finite("projection_horizon_start", spec.projection_horizon_start)?;
                validate_finite("projection_horizon_end", spec.projection_horizon_end)?;
                if spec.projection_horizon_start < spec.observed_domain_max
                    || spec.projection_horizon_start >= spec.projection_horizon_end
                {
                    return Err(ReasoningError::InvalidProjectionHorizon {
                        observed_max: format!("{}", spec.observed_domain_max),
                        horizon_start: format!("{}", spec.projection_horizon_start),
                        horizon_end: format!("{}", spec.projection_horizon_end),
                    });
                }
                validate_text("value_unit", &spec.value_unit, max_bytes, false)?;
                validate_finite("projected_value", spec.projected_value)?;
                validate_finite("uncertainty_lower", spec.uncertainty_lower)?;
                validate_finite("uncertainty_upper", spec.uncertainty_upper)?;
                if spec.uncertainty_lower > spec.projected_value
                    || spec.projected_value > spec.uncertainty_upper
                {
                    return Err(ReasoningError::InvalidNumericValue(
                        "uncertainty bounds must bracket projected_value",
                    ));
                }
                validate_text("model_name", &spec.model_name, max_bytes, false)?;
                validate_text(
                    "uncertainty_method",
                    &spec.uncertainty_method,
                    max_bytes,
                    true,
                )?;
                validate_text(
                    "sensitivity_summary",
                    &spec.sensitivity_summary,
                    max_bytes,
                    true,
                )?;
                if spec.explicit_assumptions.is_empty() {
                    return Err(ReasoningError::EmptyField("explicit_assumptions"));
                }
                if spec.explicit_assumptions.len() > max_items {
                    return Err(ReasoningError::BudgetExceeded {
                        limit: ReasoningLimitKind::ItemsPerStep,
                        actual: spec.explicit_assumptions.len(),
                        max: max_items,
                    });
                }
                for assumption in &spec.explicit_assumptions {
                    validate_text("explicit_assumption", assumption, max_bytes, true)?;
                }

                // Verify unit/domain compatibility with any quantitative premise inputs (`A07`).
                for inp in inputs {
                    if let Some(premise) = self.premises_by_version.get(&inp.node_version)
                        && let Some(qty) = premise.quantity()
                    {
                        if qty.unit != spec.value_unit && qty.unit != spec.domain_unit {
                            return Err(ReasoningError::UnitMismatch {
                                expected_unit: spec.value_unit.clone(),
                                actual_unit: qty.unit.clone(),
                            });
                        }
                        if qty.unit == spec.domain_unit
                            && (qty.value < spec.observed_domain_min
                                || qty.value > spec.observed_domain_max)
                        {
                            return Err(ReasoningError::UnsupportedDomainValue {
                                field: "input_premise.quantity",
                                value: format!("{}", qty.value),
                                domain_min: format!("{}", spec.observed_domain_min),
                                domain_max: format!("{}", spec.observed_domain_max),
                            });
                        }
                    }
                }
                Ok(())
            }
        }
    }
}

fn validate_decision_state(
    decision: &DecisionState,
    max_bytes: usize,
) -> Result<String, ReasoningError> {
    match decision {
        DecisionState::Proposed { rationale } | DecisionState::Confirmed { rationale, .. } => {
            validate_text("decision_rationale", rationale, max_bytes, true)?;
            Ok(rationale.clone())
        }
        DecisionState::Reversed {
            reversal_rationale, ..
        } => {
            if reversal_rationale.trim().is_empty() {
                return Err(ReasoningError::DecisionReversalMissingRationale("decision"));
            }
            validate_text("reversal_rationale", reversal_rationale, max_bytes, true)?;
            Ok(reversal_rationale.clone())
        }
    }
}
