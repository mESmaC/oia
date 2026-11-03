//! Validation and simulation lab, controlled perturbation manifest, dataset split/provenance
//! validator, and operational resource/backpressure benchmark evaluation kernel (`#40`,
//! `FIELD-04`, `CAM-05`, `MEDIA-02`, `DEVICE-04`, `docs/delivery.md` §4, and deterministic
//! subsets of `A09`, `A10`, `A12`, `A13`).
//!
//! Note on scope: this kernel validates in-memory dataset manifests, caller-declared SHA-256
//! digest strings, split-disjointness rules, per-item held-out evaluation receipts, and
//! caller-supplied operational benchmark receipts. It does not read or hash external payload
//! files, verify physical source or ground-truth authenticity, fit `D09` score-calibration
//! models, or execute live hardware/camera/UAV benchmarks.
//!
//! ### Core Contracts and Integrity Invariants (`AGENTS.md` §§2–5, §8)
//! 1. **Labeled Real Fixtures vs. Controlled Simulations (`FIELD-04`, `MEDIA-02`, `DEVICE-04`, `A10`, `A13`):**
//!    Real or reference fixtures (`DatasetRecordOrigin::LabeledRealFixture`) and synthetic or
//!    perturbed simulations (`DatasetRecordOrigin::ControlledSimulation`) are distinct origin
//!    classes. Every simulation must carry the mandatory [`SIMULATION_LABEL`], a non-empty
//!    statement of how it differs from observations, and explicit perturbation parameters.
//!    Simulations can never be promoted to observation evidence.
//! 2. **Disjoint Splits, Per-Item Audit Trail, and Anti-Cherry-Picking (`FIELD-04`, `CAM-05`, `A09`):**
//!    Training, calibration/tuning, and held-out evaluation splits must not share declared
//!    payload SHA-256 digests, source versions, or base-fixture derivation lineage. Held-out
//!    evaluations must account for every held-out record in the pinned manifest (either evaluated
//!    or explicitly skipped with a typed reason), retain per-item outcome receipts in
//!    [`HeldOutEvaluationReport::item_receipts`] so misses and skip reasons remain inspectable,
//!    and report aggregate metrics separately for labeled real fixtures and controlled simulations.
//! 3. **Score Calibration and Generated-Detail Guards (`CAM-05`, `MEDIA-03`, `A09`, `A10`):**
//!    Because `D09` empirical calibration fitting and verification are out of scope for this
//!    kernel, claiming verified calibrated probabilities (`claims_calibrated_probabilities = true`)
//!    is always rejected. Caller-supplied [`CalibrationReceipt`]s must be bound to the exact
//!    detector/solver ID, version, domain, and `CalibrationOrTuning` split records, and are
//!    recorded as [`ScoreCalibrationStatus::CallerDeclaredReceiptUnverified`] with
//!    `is_calibrated_probability() == false`. Predictive or generated detail may never be used as
//!    a physical measurement or independent detection input.
//! 4. **Operational, Backpressure, Cancellation, and Visual-Parity Benchmarks (`DEVICE-04`, `A12`, `docs/delivery.md` §4):**
//!    OIA local processing latency is tracked separately from external provider and model-service
//!    latency. Runs verify that analysis does not block UAV/device telemetry or emit flight-control
//!    commands, expose backpressure activation and coverage drops honestly, reject late
//!    publications after cancellation or supersession, and require matching declared numerical
//!    output digests across `StandardGlass` and `ReducedEffects` visual modes.

#![deny(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

use oia_domain::{OpaqueId, SchemaVersion, VersionId};

pub use crate::multilingual::StopReason;

/// Method identifier for the validation and simulation lab kernel.
pub const METHOD_ID: &str = "oia-validation-simulation-lab-v1";

/// Mandatory visible label required on all controlled simulation records and reports (`FIELD-04`, `A13`).
pub const SIMULATION_LABEL: &str =
    "CONTROLLED SIMULATION — SYNTHETIC OR PERTURBED MATERIAL (NOT OBSERVATION EVIDENCE)";

/// Mandatory caveat paired with detector scores (`CAM-05`, `A09`, `AGENTS.md` §2).
pub const UNCALIBRATED_SCORE_CAVEAT: &str = "Detector score is an uncalibrated diagnostic score (or caller-declared unverified receipt); it must not be presented as a verified calibrated class probability without an executed D09 calibration verification.";

/// Mandatory caveat distinguishing proposed design targets from executed measurements (`AGENTS.md` §3, `docs/delivery.md` §4).
pub const TARGET_VS_MEASURED_CAVEAT: &str = "Reference targets in docs/delivery.md §4 are proposed design objectives; benchmark outcomes summarize caller-supplied run receipts for the declared commit, dataset digest, and environment.";

/// Maximum crossing angle in milliradians (`17 mrad = 0.017 rad < pi / 180 rad ≈ 17.453 mrad = 1 deg`)
/// accepted for [`PerturbationSpec::DegenerateSightlineGeometry`], matching the sub-1-degree
/// near-parallel rejection domain of [`crate::triangulation::MIN_CROSSING_ANGLE_RAD`].
pub const MAX_DEGENERATE_CROSSING_ANGLE_MRAD: u32 = 17;

/// Default upper bound on dataset records in a single validation manifest.
pub const DEFAULT_MAX_DATASET_RECORDS: usize = 4_096;
/// Default upper bound on controlled perturbations chained onto one simulation record.
pub const DEFAULT_MAX_PERTURBATIONS_PER_RECORD: usize = 16;
/// Default upper bound on evaluated or skipped items in a held-out run.
pub const DEFAULT_MAX_EVALUATED_ITEMS: usize = 4_096;
/// Default upper bound on latency samples per operational latency series.
pub const DEFAULT_MAX_LATENCY_SAMPLES: usize = 10_000;
/// Default upper bound on descriptive UTF-8 text fields in bytes.
pub const DEFAULT_MAX_TEXT_BYTES: usize = 4_096;

const CONTROL_CHECK_INTERVAL: usize = 64;

/// Which resource budget was zero or exceeded during validation-lab operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationLabLimitKind {
    DatasetRecords,
    PerturbationsPerRecord,
    EvaluatedItems,
    LatencySamples,
    TextBytes,
}

impl fmt::Display for ValidationLabLimitKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DatasetRecords => f.write_str("max_dataset_records"),
            Self::PerturbationsPerRecord => f.write_str("max_perturbations_per_record"),
            Self::EvaluatedItems => f.write_str("max_evaluated_items"),
            Self::LatencySamples => f.write_str("max_latency_samples"),
            Self::TextBytes => f.write_str("max_text_bytes"),
        }
    }
}

/// Explicit operational bounds for dataset manifest validation, held-out evaluation, and benchmarks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidationLabLimits {
    max_dataset_records: usize,
    max_perturbations_per_record: usize,
    max_evaluated_items: usize,
    max_latency_samples: usize,
    max_text_bytes: usize,
}

impl ValidationLabLimits {
    /// Constructs validated non-zero limits for the validation and simulation lab.
    pub fn new(
        max_dataset_records: usize,
        max_perturbations_per_record: usize,
        max_evaluated_items: usize,
        max_latency_samples: usize,
        max_text_bytes: usize,
    ) -> Result<Self, ValidationLabError> {
        if max_dataset_records == 0 {
            return Err(ValidationLabError::ZeroLimit(
                ValidationLabLimitKind::DatasetRecords,
            ));
        }
        if max_perturbations_per_record == 0 {
            return Err(ValidationLabError::ZeroLimit(
                ValidationLabLimitKind::PerturbationsPerRecord,
            ));
        }
        if max_evaluated_items == 0 {
            return Err(ValidationLabError::ZeroLimit(
                ValidationLabLimitKind::EvaluatedItems,
            ));
        }
        if max_latency_samples == 0 {
            return Err(ValidationLabError::ZeroLimit(
                ValidationLabLimitKind::LatencySamples,
            ));
        }
        if max_text_bytes == 0 {
            return Err(ValidationLabError::ZeroLimit(
                ValidationLabLimitKind::TextBytes,
            ));
        }
        Ok(Self {
            max_dataset_records,
            max_perturbations_per_record,
            max_evaluated_items,
            max_latency_samples,
            max_text_bytes,
        })
    }

    pub fn max_dataset_records(&self) -> usize {
        self.max_dataset_records
    }

    pub fn max_perturbations_per_record(&self) -> usize {
        self.max_perturbations_per_record
    }

    pub fn max_evaluated_items(&self) -> usize {
        self.max_evaluated_items
    }

    pub fn max_latency_samples(&self) -> usize {
        self.max_latency_samples
    }

    pub fn max_text_bytes(&self) -> usize {
        self.max_text_bytes
    }
}

impl Default for ValidationLabLimits {
    fn default() -> Self {
        Self {
            max_dataset_records: DEFAULT_MAX_DATASET_RECORDS,
            max_perturbations_per_record: DEFAULT_MAX_PERTURBATIONS_PER_RECORD,
            max_evaluated_items: DEFAULT_MAX_EVALUATED_ITEMS,
            max_latency_samples: DEFAULT_MAX_LATENCY_SAMPLES,
            max_text_bytes: DEFAULT_MAX_TEXT_BYTES,
        }
    }
}

/// Dataset partition role (`FIELD-04`, `CAM-05`, `A09`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DatasetSplit {
    /// Used for model or heuristic development; never eligible for calibration or held-out claims.
    Train,
    /// Used to fit score calibration or operating thresholds; disjoint from `HeldOutEvaluation`.
    CalibrationOrTuning,
    /// Locked out-of-sample evaluation split; must be evaluated without cherry-picking.
    HeldOutEvaluation,
}

impl fmt::Display for DatasetSplit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Train => f.write_str("train"),
            Self::CalibrationOrTuning => f.write_str("calibration_or_tuning"),
            Self::HeldOutEvaluation => f.write_str("held_out_evaluation"),
        }
    }
}

/// Domain modality of a validation fixture or simulation case (`A09`, `A10`, `A12`, `A13`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ValidationDomain {
    /// Camera stream and anomaly/fault sequence (`CAM-01`–`CAM-05`, `A09`).
    CameraAnomalyAndHealth,
    /// Still-image or video forensic provenance and geometry (`MEDIA-01`–`MEDIA-03`, `A10`).
    MediaForensicsAndGeometry,
    /// UAV video, telemetry, clock alignment, and gimbal/vibration sequence (`DEVICE-01`–`DEVICE-04`, `A12`).
    UavVideoAndTelemetry,
    /// Multi-observer calibration, sightline triangulation, or field sync (`FIELD-01`–`FIELD-04`, `A13`).
    FieldworkAndTriangulation,
}

/// Controlled physical, optical, codec, or timing perturbation applied in a simulation (`FIELD-04`, `MEDIA-02`, `DEVICE-04`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PerturbationSpec {
    /// Controlled additive or shot sensor noise (`sigma_norm_milli` in `1..=1_000`).
    SensorNoise { sigma_norm_milli: u32, seed: u64 },
    /// Controlled motion blur or optical defocus (`kernel_radius_px_milli > 0`, `motion_angle_deg_milli` in `[-180_000, 180_000]`).
    MotionOrDefocusBlur {
        kernel_radius_px_milli: u32,
        motion_angle_deg_milli: i32,
    },
    /// Controlled optical glare or bloom (`bloom_intensity_bps` and `affected_area_bps` in `1..=10_000`).
    OpticalGlare {
        bloom_intensity_bps: u16,
        affected_area_bps: u16,
    },
    /// Rolling-shutter line readout delay under angular motion (`line_readout_us > 0`).
    RollingShutterReadout {
        line_readout_us: u32,
        angular_rate_mrad_per_s: i32,
    },
    /// Lens radial distortion and perspective obliquity (`viewing_obliquity_deg_milli < 90_000`).
    LensAndPerspectiveDistortion {
        radial_k1_milli: i32,
        radial_k2_milli: i32,
        viewing_obliquity_deg_milli: u32,
    },
    /// Lossy recompression generation (`quality_step` in `1..=100`, `recompression_generations >= 1`).
    LossyCompression {
        codec_label: String,
        quality_step: u8,
        recompression_generations: u8,
    },
    /// Moving-camera vibration, gimbal rotation, parallax baseline, and exposure shift (`MEDIA-02`, `DEVICE-04`).
    CameraMotionAndParallax {
        vibration_rms_mrad: u32,
        gimbal_rate_mrad_per_s: i32,
        parallax_baseline_mm: u32,
        exposure_shift_ev_milli: i32,
    },
    /// Device clock offset, drift in parts-per-million, and preserved clock uncertainty (`FIELD-04`, `DEVICE-04`).
    ClockOffsetAndDrift {
        initial_offset_ms: i64,
        drift_ppm: i32,
        stated_uncertainty_ms: u64,
    },
    /// Dropped video frames or telemetry disconnection gap (`DEVICE-04`, `A12`).
    FrameAndTelemetryDrop {
        dropped_frame_count: u32,
        disconnected_duration_ms: u64,
    },
    /// Degenerate or near-parallel multi-view crossing geometry in milliradians
    /// (`crossing_angle_mrad <= MAX_DEGENERATE_CROSSING_ANGLE_MRAD`, i.e. `<= 17 mrad < 1 deg`) (`FIELD-03`, `A13`).
    DegenerateSightlineGeometry { crossing_angle_mrad: u32 },
}

/// Origin class distinguishing real labeled fixtures from controlled simulations (`FIELD-04`, `A13`, `AGENTS.md` §2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DatasetRecordOrigin {
    /// Labeled real observation or reference fixture with caller-declared ground-truth basis.
    LabeledRealFixture {
        source_id: OpaqueId,
        source_version: VersionId,
        calibration_version: Option<VersionId>,
        capture_time_uncertainty_ms: u64,
        ground_truth_basis: String,
    },
    /// Controlled synthetic or perturbed simulation; never observation evidence.
    ControlledSimulation {
        generator_id: OpaqueId,
        generator_version: VersionId,
        /// Optional link to a real fixture in the **same** split from which this perturbed simulation was derived.
        base_real_fixture_version: Option<VersionId>,
        seed: u64,
        perturbations: Vec<PerturbationSpec>,
        visible_simulation_label: String,
        difference_from_observation: String,
    },
}

/// Ground-truth target classification for a validation record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroundTruthLabel {
    /// Target phenomenon or anomaly is present and observable.
    PositiveTargetPresent,
    /// Ordinary scene, benign artifact, or camera fault where no scene target alert should fire.
    NegativeOrdinaryOrFault,
    /// Unresolvable / degenerate geometry where the system must refuse or report insufficient evidence (`A13`).
    DegenerateRequiresInsufficientEvidence,
}

/// A single record entry in a validation-lab dataset manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetRecord {
    pub record_id: OpaqueId,
    pub record_version: VersionId,
    pub split: DatasetSplit,
    pub domain: ValidationDomain,
    /// Caller-supplied 64-character lowercase hexadecimal SHA-256 digest of the record payload.
    pub payload_sha256: String,
    pub observed_duration_ms: u64,
    pub expected_label: GroundTruthLabel,
    pub origin: DatasetRecordOrigin,
    /// Must be `false` for all `ControlledSimulation` records (`AGENTS.md` §2, `A10`, `A13`).
    pub promoted_to_observation_evidence: bool,
}

impl DatasetRecord {
    /// Returns `true` when this record is a controlled simulation rather than a real fixture.
    pub fn is_simulation(&self) -> bool {
        matches!(
            self.origin,
            DatasetRecordOrigin::ControlledSimulation { .. }
        )
    }

    /// Returns `true` only for real fixtures; simulations can never serve as observation evidence.
    pub fn can_serve_as_observation_evidence(&self) -> bool {
        matches!(self.origin, DatasetRecordOrigin::LabeledRealFixture { .. })
            && !self.promoted_to_observation_evidence_forbidden()
    }

    fn promoted_to_observation_evidence_forbidden(&self) -> bool {
        self.is_simulation() && self.promoted_to_observation_evidence
    }
}

/// Input dataset manifest for validation-lab verification (`#40`, `FIELD-04`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetManifestInput {
    pub dataset_id: OpaqueId,
    pub dataset_version: VersionId,
    pub schema_version: SchemaVersion,
    pub description: String,
    pub records: Vec<DatasetRecord>,
}

/// Split-level record counts keeping real fixtures and simulations strictly separate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SplitComposition {
    pub labeled_real_fixture_count: usize,
    pub controlled_simulation_count: usize,
}

impl SplitComposition {
    pub fn total_count(&self) -> usize {
        self.labeled_real_fixture_count
            .saturating_add(self.controlled_simulation_count)
    }
}

/// Verified dataset manifest with validated declared digest metadata, split disjointness, and origin separation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedDatasetManifest {
    dataset_id: OpaqueId,
    dataset_version: VersionId,
    schema_version: SchemaVersion,
    description: String,
    records: Vec<DatasetRecord>,
    train_composition: SplitComposition,
    calibration_composition: SplitComposition,
    held_out_composition: SplitComposition,
}

impl ValidatedDatasetManifest {
    pub fn dataset_id(&self) -> &OpaqueId {
        &self.dataset_id
    }

    pub fn dataset_version(&self) -> &VersionId {
        &self.dataset_version
    }

    pub fn schema_version(&self) -> SchemaVersion {
        self.schema_version
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn records(&self) -> &[DatasetRecord] {
        &self.records
    }

    pub fn train_composition(&self) -> SplitComposition {
        self.train_composition
    }

    pub fn calibration_composition(&self) -> SplitComposition {
        self.calibration_composition
    }

    pub fn held_out_composition(&self) -> SplitComposition {
        self.held_out_composition
    }
}

/// Explicit reason why a held-out record was not scored, preventing silent cherry-picking (`#40`, `AGENTS.md` §2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeldOutSkipReason {
    /// Input lies outside the declared numerical or format domain (e.g. crossing angle below minimum).
    UnsupportedInputDomain { detail: String },
    /// Required camera or sensor calibration was missing or invalidated by movement (`CAM-05`, `A13`).
    MissingOrInvalidatedCalibration { detail: String },
    /// Stream or sensor drop prevented active coverage during the target window (`A09`, `A12`).
    CoverageGapOrDroppedStream { detail: String },
    /// Execution hit a configured resource budget or cooperative deadline (`AGENTS.md` §5).
    BudgetOrDeadlineStopped { detail: String },
}

/// System output classification on a single held-out record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemDecision {
    /// System emitted a candidate detection or reconstructed a target within bounds.
    PositiveCandidateOrReconstruction,
    /// System suppressed the event as normal background, camera fault, or ordinary explanation.
    NegativeSuppressedOrFaultClassified,
    /// System explicitly abstained / refused due to degenerate geometry or insufficient evidence (`A13`).
    AbstainedInsufficientEvidence,
}

/// Evaluation submission for a single held-out dataset item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldOutItemEvaluation {
    pub record_id: OpaqueId,
    pub record_version: VersionId,
    /// Either an evaluated decision with latency/score metadata, or an explicit skip reason.
    pub outcome: HeldOutItemOutcome,
}

/// Outcome of running a single held-out item through the system under test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeldOutItemOutcome {
    Evaluated {
        decision: SystemDecision,
        raw_score_bps: Option<u16>,
        processing_latency_ms: u64,
        /// Must be `false`: generated/predictive detail cannot be used as a measurement (`A10`, `MEDIA-03`).
        used_predictive_or_generated_detail_as_measurement: bool,
        /// For simulation inputs, the output report must preserve [`SIMULATION_LABEL`] (`FIELD-04`, `A13`).
        output_retained_simulation_label: bool,
    },
    Skipped {
        reason: HeldOutSkipReason,
    },
}

/// Retained per-item audit receipt in [`HeldOutEvaluationReport::item_receipts`] (`#40`, `AGENTS.md` §2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldOutItemReceipt {
    pub record_id: OpaqueId,
    pub record_version: VersionId,
    pub domain: ValidationDomain,
    pub is_simulation: bool,
    pub expected_label: GroundTruthLabel,
    pub observed_duration_ms: u64,
    pub outcome: HeldOutItemOutcome,
}

/// Model-, domain-, and split-bound calibration receipt (`CAM-05`).
///
/// Because this kernel does not execute or verify `D09` empirical calibration fitting, a validated
/// `CalibrationReceipt` is recorded as [`ScoreCalibrationStatus::CallerDeclaredReceiptUnverified`]
/// and never flips `is_calibrated_probability()` to `true`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalibrationReceipt {
    pub calibration_run_version: VersionId,
    pub calibration_split_dataset_version: VersionId,
    pub detector_or_solver_id: OpaqueId,
    pub detector_or_solver_version: VersionId,
    pub calibrated_domain: ValidationDomain,
    pub score_mapping_version: VersionId,
    /// Exact `CalibrationOrTuning` record versions evaluated to fit this receipt.
    pub evaluated_calibration_record_versions: Vec<VersionId>,
    pub empirical_brier_score_milli: u32,
}

/// Explicit calibration verification status of a held-out evaluation report (`CAM-05`, `AGENTS.md` §§2–3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScoreCalibrationStatus {
    /// Raw diagnostic scores only; no calibration receipt supplied.
    UncalibratedDiagnosticScores,
    /// Caller supplied a structurally validated `CalibrationReceipt` bound to the detector/solver,
    /// domain, and `CalibrationOrTuning` split records, but empirical calibration is not verified
    /// by this kernel (`D09` remains open).
    CallerDeclaredReceiptUnverified { receipt: CalibrationReceipt },
}

impl ScoreCalibrationStatus {
    /// Returns `false`: caller-supplied receipts are unverified declarations until `D09`
    /// empirical calibration fitting and verification are implemented.
    pub const fn is_verified_calibrated_probability(&self) -> bool {
        false
    }
}

/// Input to [`evaluate_held_out_run`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldOutEvaluationInput {
    pub evaluation_run_id: OpaqueId,
    pub evaluation_run_version: VersionId,
    pub detector_or_solver_id: OpaqueId,
    pub detector_or_solver_version: VersionId,
    /// Must be `false`: claiming verified calibrated probabilities before `D09` calibration
    /// verification is rejected with [`ValidationLabError::UncalibratedScorePresentedAsProbability`].
    pub claims_calibrated_probabilities: bool,
    pub calibration_receipt: Option<CalibrationReceipt>,
    pub items: Vec<HeldOutItemEvaluation>,
}

/// Partitioned operational metrics for one origin class (`LabeledRealFixture` vs. `ControlledSimulation`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionMetrics {
    pub manifest_record_count: usize,
    pub evaluated_record_count: usize,
    pub skipped_record_count: usize,
    /// Active evaluated coverage in basis points (`0..=10_000`) of manifest records in this partition.
    pub record_coverage_bps: u16,
    pub observed_active_duration_ms: u64,
    pub true_positive_count: usize,
    pub false_positive_count: usize,
    pub false_negative_count: usize,
    pub true_negative_count: usize,
    /// Degenerate-geometry cases correctly refused as `AbstainedInsufficientEvidence` (`A13`).
    pub correct_degenerate_abstention_count: usize,
    /// Degenerate-geometry cases where the system falsely claimed a unique positive reconstruction (`A13`).
    pub false_degenerate_resolution_count: usize,
    /// Labeled positive-event recall in basis points (`None` if no positive truth events were evaluated).
    pub positive_event_recall_bps: Option<u16>,
    /// False alerts per observed active hour in milli-alerts (`false_positive_count * 3_600_000 * 1_000 / observed_active_duration_ms`).
    pub false_alerts_per_hour_milli: Option<u64>,
    pub mean_processing_latency_ms: Option<u64>,
    pub p95_processing_latency_ms: Option<u64>,
}

/// Verified held-out evaluation report preserving per-item outcome receipts and separate
/// real-fixture vs. simulation metrics (`#40`, `AGENTS.md` §2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldOutEvaluationReport {
    pub method_id: &'static str,
    pub evaluation_run_id: OpaqueId,
    pub evaluation_run_version: VersionId,
    pub dataset_id: OpaqueId,
    pub dataset_version: VersionId,
    pub detector_or_solver_id: OpaqueId,
    pub detector_or_solver_version: VersionId,
    pub score_calibration_status: ScoreCalibrationStatus,
    pub score_caveat: &'static str,
    pub simulation_label: &'static str,
    pub labeled_real_fixture_metrics: PartitionMetrics,
    pub controlled_simulation_metrics: PartitionMetrics,
    /// Deterministic per-item audit receipts in manifest order, preserving individual decisions,
    /// raw scores, latencies, and explicit skip reasons/details (`#40`, `AGENTS.md` §2).
    pub item_receipts: Vec<HeldOutItemReceipt>,
}

impl HeldOutEvaluationReport {
    /// Returns whether detector scores in this report are verified calibrated probabilities
    /// (`false` until `D09` empirical calibration verification is implemented).
    pub const fn is_calibrated_probability(&self) -> bool {
        self.score_calibration_status
            .is_verified_calibrated_probability()
    }
}

/// Cache warmup state for an operational benchmark (`docs/delivery.md` §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchmarkCacheState {
    Cold,
    Warm,
}

/// Visual quality mode tested for numerical output parity (`AGENTS.md` §2, `docs/delivery.md` §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualQualityMode {
    StandardGlass,
    ReducedEffects,
}

/// Pinned execution environment and revision metadata for a benchmark run (`AGENTS.md` §3, `docs/delivery.md` §4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkEnvironmentManifest {
    pub benchmark_id: OpaqueId,
    pub run_version: VersionId,
    /// Exact 40-char or 64-char lowercase hex git commit or revision digest.
    pub git_revision_hex: String,
    pub dataset_manifest_version: VersionId,
    pub dataset_sha256: String,
    pub hardware_profile: String,
    pub os_description: String,
    pub renderer_description: String,
    pub worker_concurrency: u32,
    pub cache_state: BenchmarkCacheState,
}

/// Proposed design targets from `docs/delivery.md` §4 compared against a benchmark run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReferenceTargetProfile {
    pub ui_ack_p95_target_ms: u64,
    pub warm_query_p95_target_ms: u64,
    pub cancel_ack_target_ms: u64,
    pub worker_release_target_ms: u64,
    pub globe_min_fps_target: u32,
    pub max_allowed_telemetry_stall_ms: u64,
}

impl ReferenceTargetProfile {
    /// Returns the proposed v0.1 reference design targets from `docs/delivery.md` §4.
    pub const fn delivery_v0_1_default() -> Self {
        Self {
            ui_ack_p95_target_ms: 100,
            warm_query_p95_target_ms: 2_000,
            cancel_ack_target_ms: 250,
            worker_release_target_ms: 5_000,
            globe_min_fps_target: 30,
            max_allowed_telemetry_stall_ms: 100,
        }
    }
}

/// Telemetry and case-editing responsiveness observations during heavy analysis (`DEVICE-04`, `A12`, `docs/delivery.md` §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TelemetryAndEditResponsiveness {
    pub max_telemetry_dispatch_stall_ms: u64,
    pub dropped_telemetry_packets: u32,
    /// Must be `0`: initial UAV/device adapters are receive-only and never emit flight-control commands (`DEVICE-04`, `A12`).
    pub unhandled_flight_control_commands_emitted: u32,
    pub case_edit_responsive_under_backpressure: bool,
}

/// Queue backpressure and coverage impact measurements (`AGENTS.md` §2, `docs/delivery.md` §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackpressureAndCoverageInput {
    pub configured_queue_capacity: u32,
    pub peak_queue_depth: u32,
    pub backpressure_activated: bool,
    pub throttled_or_rejected_requests: u32,
    pub dropped_frames_or_samples: u32,
    /// Reported output coverage in basis points (`0..=10_000`); cannot be `10_000` if frames/requests were dropped.
    pub reported_coverage_bps: u16,
}

/// Cooperative cancellation and worker-lease recovery observations (`docs/delivery.md` §4, `A01`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CancellationAndRecoveryInput {
    pub cancel_acknowledgment_ms: u64,
    pub worker_resource_release_ms: u64,
    pub late_completions_observed: u32,
    /// Must be `0`: cancelled or superseded work must never publish late results (`AGENTS.md` §5, `A01`).
    pub published_after_cancel_or_supersession: u32,
    pub interrupted_import_left_partial_case: bool,
}

/// Verification that `StandardGlass` and `ReducedEffects` produce identical declared numerical outputs (`AGENTS.md` §2, `docs/delivery.md` §4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualQualityParityInput {
    pub marker_count: u32,
    pub standard_glass_fps: u32,
    pub reduced_effects_fps: u32,
    pub standard_glass_numerical_digest_sha256: String,
    pub reduced_effects_numerical_digest_sha256: String,
}

/// Input to [`evaluate_operational_benchmark`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationalBenchmarkInput {
    pub environment: BenchmarkEnvironmentManifest,
    pub targets: ReferenceTargetProfile,
    /// OIA local UI action acknowledgment latencies (excluding external acquisition/compute completion).
    pub ui_acknowledgment_latencies_ms: Vec<u64>,
    /// OIA local filtered text/spatial query latencies.
    pub local_query_latencies_ms: Vec<u64>,
    /// External data-provider network latencies, tracked separately from OIA local processing (`docs/delivery.md` §4).
    pub external_provider_latencies_ms: Vec<u64>,
    /// External or isolated model-service inference latencies, tracked separately from OIA local processing (`docs/delivery.md` §4).
    pub model_service_latencies_ms: Vec<u64>,
    pub responsiveness: TelemetryAndEditResponsiveness,
    pub backpressure: BackpressureAndCoverageInput,
    pub cancellation: CancellationAndRecoveryInput,
    pub visual_parity: VisualQualityParityInput,
}

/// Summary statistics (count, min, mean, p95, max) for a latency series in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LatencySummaryMs {
    pub sample_count: usize,
    pub min_ms: u64,
    pub mean_ms: u64,
    pub p95_ms: u64,
    pub max_ms: u64,
}

/// Comparison of a caller-supplied operational metric against its proposed design target in `docs/delivery.md` §4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetComparisonOutcome {
    WithinProposedTarget,
    ExceededProposedTarget,
}

/// Validated operational, resource, backpressure, and recovery benchmark report (`#40`, `docs/delivery.md` §4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationalBenchmarkReport {
    pub method_id: &'static str,
    pub target_vs_measured_caveat: &'static str,
    pub environment: BenchmarkEnvironmentManifest,
    pub targets: ReferenceTargetProfile,
    pub ui_acknowledgment_latency: LatencySummaryMs,
    pub local_query_latency: LatencySummaryMs,
    pub external_provider_latency: Option<LatencySummaryMs>,
    pub model_service_latency: Option<LatencySummaryMs>,
    pub ui_ack_target_outcome: TargetComparisonOutcome,
    pub local_query_target_outcome: TargetComparisonOutcome,
    pub cancel_ack_target_outcome: TargetComparisonOutcome,
    pub worker_release_target_outcome: TargetComparisonOutcome,
    pub globe_fps_target_outcome: TargetComparisonOutcome,
    pub telemetry_stall_target_outcome: TargetComparisonOutcome,
    pub responsiveness: TelemetryAndEditResponsiveness,
    pub backpressure: BackpressureAndCoverageInput,
    pub cancellation: CancellationAndRecoveryInput,
    pub visual_parity: VisualQualityParityInput,
}

/// Typed errors for dataset manifest validation, held-out evaluation, and operational benchmarks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationLabError {
    ZeroLimit(ValidationLabLimitKind),
    LimitExceeded {
        kind: ValidationLabLimitKind,
        limit: usize,
        actual: usize,
    },
    EmptyDatasetManifest,
    EmptyTextField {
        field: &'static str,
    },
    InvalidSha256Hex {
        field: &'static str,
    },
    InvalidGitRevisionHex,
    ZeroObservedDuration {
        record_id: OpaqueId,
    },
    DuplicateRecordId {
        record_id: OpaqueId,
    },
    DuplicateRecordVersion {
        record_version: VersionId,
    },
    SimulationPromotedToObservation {
        record_id: OpaqueId,
    },
    MissingOrInvalidSimulationLabel {
        record_id: OpaqueId,
    },
    InvalidPerturbationParameter {
        record_id: OpaqueId,
        reason: &'static str,
    },
    BaseFixtureNotFoundInSameSplit {
        simulation_record_id: OpaqueId,
        base_fixture_version: VersionId,
    },
    CrossSplitPayloadLeakage {
        payload_sha256: String,
        first_split: DatasetSplit,
        second_split: DatasetSplit,
    },
    CrossSplitSourceVersionLeakage {
        source_version: VersionId,
        first_split: DatasetSplit,
        second_split: DatasetSplit,
    },
    CrossSplitLineageLeakage {
        simulation_record_id: OpaqueId,
        simulation_split: DatasetSplit,
        base_fixture_version: VersionId,
        base_fixture_split: DatasetSplit,
    },
    InvalidHeldOutEvaluationTarget {
        record_id: OpaqueId,
        record_version: VersionId,
    },
    DuplicateHeldOutEvaluationItem {
        record_version: VersionId,
    },
    HeldOutRecordSilentlyOmitted {
        record_id: OpaqueId,
        record_version: VersionId,
    },
    UncalibratedScorePresentedAsProbability,
    InvalidCalibrationReceipt {
        reason: &'static str,
    },
    InvalidScoreBasisPoints {
        record_id: OpaqueId,
        raw_score_bps: u16,
    },
    GeneratedDetailUsedAsMeasurement {
        record_id: OpaqueId,
    },
    SimulationLabelDroppedInOutput {
        record_id: OpaqueId,
    },
    EmptyRequiredLatencySeries {
        series: &'static str,
    },
    ZeroWorkerConcurrency,
    ZeroQueueCapacity,
    FlightControlCommandEmitted {
        command_count: u32,
    },
    HiddenCoverageLossUnderBackpressure {
        dropped_frames_or_samples: u32,
        throttled_or_rejected_requests: u32,
        reported_coverage_bps: u16,
    },
    InvalidCoverageBasisPoints {
        reported_coverage_bps: u16,
    },
    LatePublicationAfterCancel {
        published_count: u32,
    },
    InterruptedImportCorruptedCase,
    VisualQualityAlteredNumericalResult {
        standard_glass_sha256: String,
        reduced_effects_sha256: String,
    },
    AllocationFailed,
    Stopped(StopReason),
}

impl fmt::Display for ValidationLabError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroLimit(kind) => write!(f, "validation lab limit `{kind}` must be non-zero"),
            Self::LimitExceeded {
                kind,
                limit,
                actual,
            } => write!(
                f,
                "validation lab limit `{kind}` exceeded: actual {actual} > limit {limit}"
            ),
            Self::EmptyDatasetManifest => {
                f.write_str("dataset manifest must contain at least one record")
            }
            Self::EmptyTextField { field } => {
                write!(f, "required text field `{field}` must not be empty")
            }
            Self::InvalidSha256Hex { field } => write!(
                f,
                "field `{field}` must be a 64-character lowercase hexadecimal SHA-256 digest"
            ),
            Self::InvalidGitRevisionHex => f.write_str(
                "git_revision_hex must be a 40- or 64-character lowercase hexadecimal digest",
            ),
            Self::ZeroObservedDuration { record_id } => write!(
                f,
                "record `{}` must declare a positive observed_duration_ms",
                record_id.as_str()
            ),
            Self::DuplicateRecordId { record_id } => write!(
                f,
                "duplicate record_id `{}` in dataset manifest",
                record_id.as_str()
            ),
            Self::DuplicateRecordVersion { record_version } => write!(
                f,
                "duplicate record_version `{}` in dataset manifest",
                record_version.as_str()
            ),
            Self::SimulationPromotedToObservation { record_id } => write!(
                f,
                "controlled simulation `{}` cannot be promoted to observation evidence",
                record_id.as_str()
            ),
            Self::MissingOrInvalidSimulationLabel { record_id } => write!(
                f,
                "controlled simulation `{}` must retain the exact mandatory SIMULATION_LABEL",
                record_id.as_str()
            ),
            Self::InvalidPerturbationParameter { record_id, reason } => write!(
                f,
                "invalid perturbation parameter on record `{}`: {reason}",
                record_id.as_str()
            ),
            Self::BaseFixtureNotFoundInSameSplit {
                simulation_record_id,
                base_fixture_version,
            } => write!(
                f,
                "simulation `{}` references base real fixture version `{}` that is not present as a LabeledRealFixture in the same split",
                simulation_record_id.as_str(),
                base_fixture_version.as_str()
            ),
            Self::CrossSplitPayloadLeakage {
                payload_sha256,
                first_split,
                second_split,
            } => write!(
                f,
                "payload SHA-256 `{payload_sha256}` appears in both `{first_split}` and `{second_split}` splits"
            ),
            Self::CrossSplitSourceVersionLeakage {
                source_version,
                first_split,
                second_split,
            } => write!(
                f,
                "source version `{}` appears in both `{first_split}` and `{second_split}` splits",
                source_version.as_str()
            ),
            Self::CrossSplitLineageLeakage {
                simulation_record_id,
                simulation_split,
                base_fixture_version,
                base_fixture_split,
            } => write!(
                f,
                "simulation `{}` in `{simulation_split}` derives from base fixture `{}` in `{base_fixture_split}`",
                simulation_record_id.as_str(),
                base_fixture_version.as_str()
            ),
            Self::InvalidHeldOutEvaluationTarget {
                record_id,
                record_version,
            } => write!(
                f,
                "item (`{}`, `{}`) is not an eligible HeldOutEvaluation record in the pinned manifest",
                record_id.as_str(),
                record_version.as_str()
            ),
            Self::DuplicateHeldOutEvaluationItem { record_version } => write!(
                f,
                "held-out record version `{}` was submitted more than once",
                record_version.as_str()
            ),
            Self::HeldOutRecordSilentlyOmitted {
                record_id,
                record_version,
            } => write!(
                f,
                "held-out record (`{}`, `{}`) was silently omitted from evaluation (cherry-picking is forbidden; evaluate or record an explicit HeldOutSkipReason)",
                record_id.as_str(),
                record_version.as_str()
            ),
            Self::UncalibratedScorePresentedAsProbability => f.write_str(
                "detector scores cannot be claimed as verified calibrated probabilities before D09 empirical calibration verification",
            ),
            Self::InvalidCalibrationReceipt { reason } => {
                write!(f, "invalid calibration receipt: {reason}")
            }
            Self::InvalidScoreBasisPoints {
                record_id,
                raw_score_bps,
            } => write!(
                f,
                "record `{}` has raw_score_bps {raw_score_bps} exceeding 10_000 bps",
                record_id.as_str()
            ),
            Self::GeneratedDetailUsedAsMeasurement { record_id } => write!(
                f,
                "record `{}` used predictive or generated detail as a measurement or detection feature (`A10` violation)",
                record_id.as_str()
            ),
            Self::SimulationLabelDroppedInOutput { record_id } => write!(
                f,
                "controlled simulation `{}` lost its visible simulation label during evaluation/reporting (`A13` violation)",
                record_id.as_str()
            ),
            Self::EmptyRequiredLatencySeries { series } => {
                write!(f, "required benchmark latency series `{series}` is empty")
            }
            Self::ZeroWorkerConcurrency => {
                f.write_str("benchmark worker_concurrency must be at least 1")
            }
            Self::ZeroQueueCapacity => {
                f.write_str("benchmark configured_queue_capacity must be at least 1")
            }
            Self::FlightControlCommandEmitted { command_count } => write!(
                f,
                "initial UAV/device integration must remain receive-only, but {command_count} flight-control commands were emitted (`DEVICE-04`, `A12`)"
            ),
            Self::HiddenCoverageLossUnderBackpressure {
                dropped_frames_or_samples,
                throttled_or_rejected_requests,
                reported_coverage_bps,
            } => write!(
                f,
                "benchmark reported full coverage ({reported_coverage_bps} bps) despite {dropped_frames_or_samples} dropped frames/samples and {throttled_or_rejected_requests} throttled/rejected requests"
            ),
            Self::InvalidCoverageBasisPoints {
                reported_coverage_bps,
            } => write!(
                f,
                "reported_coverage_bps {reported_coverage_bps} exceeds 10_000 bps"
            ),
            Self::LatePublicationAfterCancel { published_count } => write!(
                f,
                "{published_count} cancelled or superseded operations published results after revocation (`A01` violation)"
            ),
            Self::InterruptedImportCorruptedCase => f.write_str(
                "interrupted import or recovery benchmark left an existing case partially replaced (`AGENTS.md` §5)",
            ),
            Self::VisualQualityAlteredNumericalResult {
                standard_glass_sha256,
                reduced_effects_sha256,
            } => write!(
                f,
                "visual quality mode changed numerical analysis outputs: StandardGlass `{standard_glass_sha256}` != ReducedEffects `{reduced_effects_sha256}`"
            ),
            Self::AllocationFailed => f.write_str("bounded memory reservation failed"),
            Self::Stopped(reason) => write!(f, "validation lab operation stopped: {reason:?}"),
        }
    }
}

impl Error for ValidationLabError {}

fn is_valid_lowercase_sha256_hex(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn is_valid_git_revision_hex(revision: &str) -> bool {
    (revision.len() == 40 || revision.len() == 64)
        && revision
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn validate_text_field(
    text: &str,
    field: &'static str,
    limits: &ValidationLabLimits,
) -> Result<(), ValidationLabError> {
    if text.trim().is_empty() {
        return Err(ValidationLabError::EmptyTextField { field });
    }
    if text.len() > limits.max_text_bytes {
        return Err(ValidationLabError::LimitExceeded {
            kind: ValidationLabLimitKind::TextBytes,
            limit: limits.max_text_bytes,
            actual: text.len(),
        });
    }
    Ok(())
}

fn validate_perturbation(
    record: &DatasetRecord,
    perturbation: &PerturbationSpec,
    limits: &ValidationLabLimits,
) -> Result<(), ValidationLabError> {
    let record_id = &record.record_id;
    match perturbation {
        PerturbationSpec::SensorNoise {
            sigma_norm_milli, ..
        } => {
            if *sigma_norm_milli == 0 || *sigma_norm_milli > 1_000 {
                return Err(ValidationLabError::InvalidPerturbationParameter {
                    record_id: record_id.clone(),
                    reason: "SensorNoise sigma_norm_milli must be in 1..=1_000",
                });
            }
        }
        PerturbationSpec::MotionOrDefocusBlur {
            kernel_radius_px_milli,
            motion_angle_deg_milli,
        } => {
            if *kernel_radius_px_milli == 0 || *kernel_radius_px_milli > 500_000 {
                return Err(ValidationLabError::InvalidPerturbationParameter {
                    record_id: record_id.clone(),
                    reason: "MotionOrDefocusBlur kernel_radius_px_milli must be in 1..=500_000",
                });
            }
            if !(-180_000..=180_000).contains(motion_angle_deg_milli) {
                return Err(ValidationLabError::InvalidPerturbationParameter {
                    record_id: record_id.clone(),
                    reason: "MotionOrDefocusBlur motion_angle_deg_milli must be in -180_000..=180_000",
                });
            }
        }
        PerturbationSpec::OpticalGlare {
            bloom_intensity_bps,
            affected_area_bps,
        } => {
            if *bloom_intensity_bps == 0
                || *bloom_intensity_bps > 10_000
                || *affected_area_bps == 0
                || *affected_area_bps > 10_000
            {
                return Err(ValidationLabError::InvalidPerturbationParameter {
                    record_id: record_id.clone(),
                    reason: "OpticalGlare bloom_intensity_bps and affected_area_bps must be in 1..=10_000",
                });
            }
        }
        PerturbationSpec::RollingShutterReadout {
            line_readout_us, ..
        } => {
            if *line_readout_us == 0 || *line_readout_us > 1_000_000 {
                return Err(ValidationLabError::InvalidPerturbationParameter {
                    record_id: record_id.clone(),
                    reason: "RollingShutterReadout line_readout_us must be in 1..=1_000_000",
                });
            }
        }
        PerturbationSpec::LensAndPerspectiveDistortion {
            viewing_obliquity_deg_milli,
            ..
        } => {
            if *viewing_obliquity_deg_milli >= 90_000 {
                return Err(ValidationLabError::InvalidPerturbationParameter {
                    record_id: record_id.clone(),
                    reason: "LensAndPerspectiveDistortion viewing_obliquity_deg_milli must be < 90_000",
                });
            }
        }
        PerturbationSpec::LossyCompression {
            codec_label,
            quality_step,
            recompression_generations,
        } => {
            validate_text_field(codec_label, "codec_label", limits)?;
            if *quality_step == 0 || *quality_step > 100 || *recompression_generations == 0 {
                return Err(ValidationLabError::InvalidPerturbationParameter {
                    record_id: record_id.clone(),
                    reason: "LossyCompression requires quality_step in 1..=100 and recompression_generations >= 1",
                });
            }
        }
        PerturbationSpec::CameraMotionAndParallax {
            vibration_rms_mrad,
            gimbal_rate_mrad_per_s,
            parallax_baseline_mm,
            exposure_shift_ev_milli,
        } => {
            if *vibration_rms_mrad == 0
                && *gimbal_rate_mrad_per_s == 0
                && *parallax_baseline_mm == 0
                && *exposure_shift_ev_milli == 0
            {
                return Err(ValidationLabError::InvalidPerturbationParameter {
                    record_id: record_id.clone(),
                    reason: "CameraMotionAndParallax must declare at least one non-zero motion, parallax, or exposure perturbation",
                });
            }
        }
        PerturbationSpec::ClockOffsetAndDrift {
            initial_offset_ms,
            drift_ppm,
            stated_uncertainty_ms,
        } => {
            if *initial_offset_ms == 0 && *drift_ppm == 0 && *stated_uncertainty_ms == 0 {
                return Err(ValidationLabError::InvalidPerturbationParameter {
                    record_id: record_id.clone(),
                    reason: "ClockOffsetAndDrift must declare non-zero offset, drift, or clock uncertainty",
                });
            }
        }
        PerturbationSpec::FrameAndTelemetryDrop {
            dropped_frame_count,
            disconnected_duration_ms,
        } => {
            if *dropped_frame_count == 0 && *disconnected_duration_ms == 0 {
                return Err(ValidationLabError::InvalidPerturbationParameter {
                    record_id: record_id.clone(),
                    reason: "FrameAndTelemetryDrop must declare dropped_frame_count > 0 or disconnected_duration_ms > 0",
                });
            }
        }
        PerturbationSpec::DegenerateSightlineGeometry {
            crossing_angle_mrad,
        } => {
            if *crossing_angle_mrad > MAX_DEGENERATE_CROSSING_ANGLE_MRAD {
                return Err(ValidationLabError::InvalidPerturbationParameter {
                    record_id: record_id.clone(),
                    reason: "DegenerateSightlineGeometry crossing_angle_mrad must be <= 17 (< 1 deg / MIN_CROSSING_ANGLE_RAD)",
                });
            }
            if record.expected_label != GroundTruthLabel::DegenerateRequiresInsufficientEvidence {
                return Err(ValidationLabError::InvalidPerturbationParameter {
                    record_id: record_id.clone(),
                    reason: "DegenerateSightlineGeometry record must have expected_label == DegenerateRequiresInsufficientEvidence",
                });
            }
        }
    }
    Ok(())
}

/// Validates a dataset manifest, enforcing split disjointness, declared SHA-256 digest syntax and
/// uniqueness across splits, and separation between labeled real fixtures and controlled
/// simulations (`#40`, `FIELD-04`, `A09`, `A13`).
pub fn validate_dataset_manifest<F>(
    input: DatasetManifestInput,
    limits: &ValidationLabLimits,
    mut check_stop: F,
) -> Result<ValidatedDatasetManifest, ValidationLabError>
where
    F: FnMut() -> Result<(), StopReason>,
{
    check_stop().map_err(ValidationLabError::Stopped)?;
    validate_text_field(&input.description, "description", limits)?;

    if input.records.is_empty() {
        return Err(ValidationLabError::EmptyDatasetManifest);
    }
    if input.records.len() > limits.max_dataset_records {
        return Err(ValidationLabError::LimitExceeded {
            kind: ValidationLabLimitKind::DatasetRecords,
            limit: limits.max_dataset_records,
            actual: input.records.len(),
        });
    }

    let mut seen_record_ids = BTreeSet::new();
    let mut seen_record_versions = BTreeSet::new();
    let mut payload_split_map: BTreeMap<String, DatasetSplit> = BTreeMap::new();
    let mut source_version_split_map: BTreeMap<VersionId, DatasetSplit> = BTreeMap::new();
    let mut real_fixture_split_by_version: BTreeMap<VersionId, DatasetSplit> = BTreeMap::new();

    let mut train_composition = SplitComposition::default();
    let mut calibration_composition = SplitComposition::default();
    let mut held_out_composition = SplitComposition::default();

    // First pass: validate individual records, uniqueness, and cross-split payload/source leakage.
    for (idx, record) in input.records.iter().enumerate() {
        if idx % CONTROL_CHECK_INTERVAL == 0 {
            check_stop().map_err(ValidationLabError::Stopped)?;
        }

        if record.observed_duration_ms == 0 {
            return Err(ValidationLabError::ZeroObservedDuration {
                record_id: record.record_id.clone(),
            });
        }
        if !is_valid_lowercase_sha256_hex(&record.payload_sha256) {
            return Err(ValidationLabError::InvalidSha256Hex {
                field: "payload_sha256",
            });
        }
        if !seen_record_ids.insert(record.record_id.clone()) {
            return Err(ValidationLabError::DuplicateRecordId {
                record_id: record.record_id.clone(),
            });
        }
        if !seen_record_versions.insert(record.record_version.clone()) {
            return Err(ValidationLabError::DuplicateRecordVersion {
                record_version: record.record_version.clone(),
            });
        }

        if let Some(&prior_split) = payload_split_map.get(&record.payload_sha256) {
            if prior_split != record.split {
                return Err(ValidationLabError::CrossSplitPayloadLeakage {
                    payload_sha256: record.payload_sha256.clone(),
                    first_split: prior_split,
                    second_split: record.split,
                });
            }
        } else {
            payload_split_map.insert(record.payload_sha256.clone(), record.split);
        }

        let split_comp = match record.split {
            DatasetSplit::Train => &mut train_composition,
            DatasetSplit::CalibrationOrTuning => &mut calibration_composition,
            DatasetSplit::HeldOutEvaluation => &mut held_out_composition,
        };

        match &record.origin {
            DatasetRecordOrigin::LabeledRealFixture {
                source_version,
                ground_truth_basis,
                ..
            } => {
                validate_text_field(ground_truth_basis, "ground_truth_basis", limits)?;
                if let Some(&prior_split) = source_version_split_map.get(source_version) {
                    if prior_split != record.split {
                        return Err(ValidationLabError::CrossSplitSourceVersionLeakage {
                            source_version: source_version.clone(),
                            first_split: prior_split,
                            second_split: record.split,
                        });
                    }
                } else {
                    source_version_split_map.insert(source_version.clone(), record.split);
                }
                real_fixture_split_by_version.insert(record.record_version.clone(), record.split);
                split_comp.labeled_real_fixture_count =
                    split_comp.labeled_real_fixture_count.saturating_add(1);
            }
            DatasetRecordOrigin::ControlledSimulation {
                perturbations,
                visible_simulation_label,
                difference_from_observation,
                ..
            } => {
                if record.promoted_to_observation_evidence {
                    return Err(ValidationLabError::SimulationPromotedToObservation {
                        record_id: record.record_id.clone(),
                    });
                }
                if visible_simulation_label != SIMULATION_LABEL {
                    return Err(ValidationLabError::MissingOrInvalidSimulationLabel {
                        record_id: record.record_id.clone(),
                    });
                }
                validate_text_field(
                    difference_from_observation,
                    "difference_from_observation",
                    limits,
                )?;
                if perturbations.is_empty() {
                    return Err(ValidationLabError::InvalidPerturbationParameter {
                        record_id: record.record_id.clone(),
                        reason: "ControlledSimulation must declare at least one PerturbationSpec",
                    });
                }
                if perturbations.len() > limits.max_perturbations_per_record {
                    return Err(ValidationLabError::LimitExceeded {
                        kind: ValidationLabLimitKind::PerturbationsPerRecord,
                        limit: limits.max_perturbations_per_record,
                        actual: perturbations.len(),
                    });
                }
                for perturbation in perturbations {
                    validate_perturbation(record, perturbation, limits)?;
                }
                split_comp.controlled_simulation_count =
                    split_comp.controlled_simulation_count.saturating_add(1);
            }
        }
    }

    // Second pass: verify that every simulation referencing a base_real_fixture_version
    // resolves to a LabeledRealFixture in the exact same split (no cross-split derivation leakage).
    for (idx, record) in input.records.iter().enumerate() {
        if idx % CONTROL_CHECK_INTERVAL == 0 {
            check_stop().map_err(ValidationLabError::Stopped)?;
        }
        if let DatasetRecordOrigin::ControlledSimulation {
            base_real_fixture_version: Some(base_ver),
            ..
        } = &record.origin
        {
            match real_fixture_split_by_version.get(base_ver) {
                None => {
                    return Err(ValidationLabError::BaseFixtureNotFoundInSameSplit {
                        simulation_record_id: record.record_id.clone(),
                        base_fixture_version: base_ver.clone(),
                    });
                }
                Some(&base_split) if base_split != record.split => {
                    return Err(ValidationLabError::CrossSplitLineageLeakage {
                        simulation_record_id: record.record_id.clone(),
                        simulation_split: record.split,
                        base_fixture_version: base_ver.clone(),
                        base_fixture_split: base_split,
                    });
                }
                Some(_) => {}
            }
        }
    }

    check_stop().map_err(ValidationLabError::Stopped)?;
    Ok(ValidatedDatasetManifest {
        dataset_id: input.dataset_id,
        dataset_version: input.dataset_version,
        schema_version: input.schema_version,
        description: input.description,
        records: input.records,
        train_composition,
        calibration_composition,
        held_out_composition,
    })
}

#[derive(Default)]
struct PartitionAccumulator {
    manifest_record_count: usize,
    evaluated_record_count: usize,
    skipped_record_count: usize,
    observed_active_duration_ms: u64,
    true_positive_count: usize,
    false_positive_count: usize,
    false_negative_count: usize,
    true_negative_count: usize,
    correct_degenerate_abstention_count: usize,
    false_degenerate_resolution_count: usize,
    latencies_ms: Vec<u64>,
}

impl PartitionAccumulator {
    fn finalize(mut self) -> PartitionMetrics {
        let record_coverage_bps = if self.manifest_record_count == 0 {
            0
        } else {
            let num = (self.evaluated_record_count as u128).saturating_mul(10_000);
            let den = self.manifest_record_count as u128;
            (num / den) as u16
        };

        let total_positive_truth = self
            .true_positive_count
            .saturating_add(self.false_negative_count);
        let positive_event_recall_bps = if total_positive_truth == 0 {
            None
        } else {
            let num = (self.true_positive_count as u128).saturating_mul(10_000);
            let den = total_positive_truth as u128;
            Some((num / den) as u16)
        };

        let false_alerts_per_hour_milli = if self.observed_active_duration_ms == 0 {
            None
        } else {
            let num = (self.false_positive_count as u128)
                .saturating_mul(3_600_000)
                .saturating_mul(1_000);
            let den = self.observed_active_duration_ms as u128;
            Some((num / den) as u64)
        };

        let (mean_processing_latency_ms, p95_processing_latency_ms) =
            match summarize_latency_slice(&mut self.latencies_ms) {
                Some(summary) => (Some(summary.mean_ms), Some(summary.p95_ms)),
                None => (None, None),
            };

        PartitionMetrics {
            manifest_record_count: self.manifest_record_count,
            evaluated_record_count: self.evaluated_record_count,
            skipped_record_count: self.skipped_record_count,
            record_coverage_bps,
            observed_active_duration_ms: self.observed_active_duration_ms,
            true_positive_count: self.true_positive_count,
            false_positive_count: self.false_positive_count,
            false_negative_count: self.false_negative_count,
            true_negative_count: self.true_negative_count,
            correct_degenerate_abstention_count: self.correct_degenerate_abstention_count,
            false_degenerate_resolution_count: self.false_degenerate_resolution_count,
            positive_event_recall_bps,
            false_alerts_per_hour_milli,
            mean_processing_latency_ms,
            p95_processing_latency_ms,
        }
    }
}

fn summarize_latency_slice(samples: &mut [u64]) -> Option<LatencySummaryMs> {
    if samples.is_empty() {
        return None;
    }
    samples.sort_unstable();
    let n = samples.len();
    let min_ms = samples.first().copied().unwrap_or(0);
    let max_ms = samples.last().copied().unwrap_or(0);
    let sum: u128 = samples.iter().map(|&v| u128::from(v)).sum();
    let mean_ms = (sum / (n as u128)) as u64;
    let rank_1_indexed = n.saturating_mul(95).saturating_add(99) / 100;
    let p95_idx = rank_1_indexed.saturating_sub(1);
    let p95_ms = samples.get(p95_idx).copied().unwrap_or(max_ms);
    Some(LatencySummaryMs {
        sample_count: n,
        min_ms,
        mean_ms,
        p95_ms,
        max_ms,
    })
}

/// Evaluates a held-out run against a validated dataset manifest, enforcing anti-cherry-picking
/// coverage, score-calibration gating, generated-detail guards, per-item receipt retention, and
/// separate real vs. simulation metrics (`#40`, `FIELD-04`, `CAM-05`, `A09`, `A10`, `A13`).
pub fn evaluate_held_out_run<F>(
    manifest: &ValidatedDatasetManifest,
    input: HeldOutEvaluationInput,
    limits: &ValidationLabLimits,
    mut check_stop: F,
) -> Result<HeldOutEvaluationReport, ValidationLabError>
where
    F: FnMut() -> Result<(), StopReason>,
{
    check_stop().map_err(ValidationLabError::Stopped)?;

    if input.items.len() > limits.max_evaluated_items {
        return Err(ValidationLabError::LimitExceeded {
            kind: ValidationLabLimitKind::EvaluatedItems,
            limit: limits.max_evaluated_items,
            actual: input.items.len(),
        });
    }

    // Because D09 empirical calibration fitting and verification are not performed by this kernel,
    // asserting verified calibrated probabilities is always rejected (`CAM-05`, `AGENTS.md` §§2–3).
    if input.claims_calibrated_probabilities {
        return Err(ValidationLabError::UncalibratedScorePresentedAsProbability);
    }

    if let Some(receipt) = &input.calibration_receipt {
        if receipt.calibration_split_dataset_version != *manifest.dataset_version() {
            return Err(ValidationLabError::InvalidCalibrationReceipt {
                reason: "calibration_split_dataset_version does not match the validated manifest version",
            });
        }
        if receipt.detector_or_solver_id != input.detector_or_solver_id
            || receipt.detector_or_solver_version != input.detector_or_solver_version
        {
            return Err(ValidationLabError::InvalidCalibrationReceipt {
                reason: "CalibrationReceipt detector_or_solver_id/version does not match the evaluated run",
            });
        }
        if receipt.evaluated_calibration_record_versions.is_empty() {
            return Err(ValidationLabError::InvalidCalibrationReceipt {
                reason: "evaluated_calibration_record_versions must not be empty",
            });
        }
        if receipt.evaluated_calibration_record_versions.len() > limits.max_evaluated_items {
            return Err(ValidationLabError::LimitExceeded {
                kind: ValidationLabLimitKind::EvaluatedItems,
                limit: limits.max_evaluated_items,
                actual: receipt.evaluated_calibration_record_versions.len(),
            });
        }
        if receipt.empirical_brier_score_milli > 1_000 {
            return Err(ValidationLabError::InvalidCalibrationReceipt {
                reason: "empirical_brier_score_milli must be in 0..=1_000",
            });
        }

        let mut expected_cal_versions = BTreeSet::new();
        for record in manifest.records() {
            if record.split == DatasetSplit::CalibrationOrTuning
                && record.domain == receipt.calibrated_domain
            {
                expected_cal_versions.insert(record.record_version.clone());
            }
        }
        if expected_cal_versions.is_empty() {
            return Err(ValidationLabError::InvalidCalibrationReceipt {
                reason: "manifest has no CalibrationOrTuning records for receipt.calibrated_domain",
            });
        }

        let mut seen_cal_versions = BTreeSet::new();
        for ver in &receipt.evaluated_calibration_record_versions {
            if !seen_cal_versions.insert(ver.clone()) {
                return Err(ValidationLabError::InvalidCalibrationReceipt {
                    reason: "duplicate record version in evaluated_calibration_record_versions",
                });
            }
            if !expected_cal_versions.contains(ver) {
                return Err(ValidationLabError::InvalidCalibrationReceipt {
                    reason: "evaluated_calibration_record_versions contains a version outside the manifest's CalibrationOrTuning split for calibrated_domain",
                });
            }
        }
        if seen_cal_versions.len() != expected_cal_versions.len() {
            return Err(ValidationLabError::InvalidCalibrationReceipt {
                reason: "evaluated_calibration_record_versions must cover all CalibrationOrTuning records in calibrated_domain without omission",
            });
        }
    }

    let mut held_out_by_version: BTreeMap<VersionId, &DatasetRecord> = BTreeMap::new();
    let mut real_acc = PartitionAccumulator::default();
    let mut sim_acc = PartitionAccumulator::default();

    for record in manifest.records() {
        if record.split == DatasetSplit::HeldOutEvaluation {
            held_out_by_version.insert(record.record_version.clone(), record);
            if record.is_simulation() {
                sim_acc.manifest_record_count = sim_acc.manifest_record_count.saturating_add(1);
            } else {
                real_acc.manifest_record_count = real_acc.manifest_record_count.saturating_add(1);
            }
        }
    }

    real_acc
        .latencies_ms
        .try_reserve(real_acc.manifest_record_count)
        .map_err(|_| ValidationLabError::AllocationFailed)?;
    sim_acc
        .latencies_ms
        .try_reserve(sim_acc.manifest_record_count)
        .map_err(|_| ValidationLabError::AllocationFailed)?;

    let mut outcomes_by_version: BTreeMap<VersionId, HeldOutItemOutcome> = BTreeMap::new();
    let mut evaluated_in_calibrated_domain_count: usize = 0;

    for (idx, item) in input.items.into_iter().enumerate() {
        if idx % CONTROL_CHECK_INTERVAL == 0 {
            check_stop().map_err(ValidationLabError::Stopped)?;
        }

        let Some(&record) = held_out_by_version.get(&item.record_version) else {
            return Err(ValidationLabError::InvalidHeldOutEvaluationTarget {
                record_id: item.record_id,
                record_version: item.record_version,
            });
        };
        if record.record_id != item.record_id {
            return Err(ValidationLabError::InvalidHeldOutEvaluationTarget {
                record_id: item.record_id,
                record_version: item.record_version,
            });
        }
        if outcomes_by_version.contains_key(&item.record_version) {
            return Err(ValidationLabError::DuplicateHeldOutEvaluationItem {
                record_version: item.record_version,
            });
        }

        let acc = if record.is_simulation() {
            &mut sim_acc
        } else {
            &mut real_acc
        };

        match &item.outcome {
            HeldOutItemOutcome::Evaluated {
                decision,
                raw_score_bps,
                processing_latency_ms,
                used_predictive_or_generated_detail_as_measurement,
                output_retained_simulation_label,
            } => {
                if *used_predictive_or_generated_detail_as_measurement {
                    return Err(ValidationLabError::GeneratedDetailUsedAsMeasurement {
                        record_id: record.record_id.clone(),
                    });
                }
                if record.is_simulation() && !*output_retained_simulation_label {
                    return Err(ValidationLabError::SimulationLabelDroppedInOutput {
                        record_id: record.record_id.clone(),
                    });
                }
                if let Some(score) = raw_score_bps {
                    if *score > 10_000 {
                        return Err(ValidationLabError::InvalidScoreBasisPoints {
                            record_id: record.record_id.clone(),
                            raw_score_bps: *score,
                        });
                    }
                    if let Some(receipt) = &input.calibration_receipt
                        && record.domain != receipt.calibrated_domain
                    {
                        return Err(ValidationLabError::InvalidCalibrationReceipt {
                            reason: "scored held-out record domain does not match CalibrationReceipt calibrated_domain",
                        });
                    }
                }
                if let Some(receipt) = &input.calibration_receipt
                    && record.domain == receipt.calibrated_domain
                {
                    evaluated_in_calibrated_domain_count =
                        evaluated_in_calibrated_domain_count.saturating_add(1);
                }

                acc.evaluated_record_count = acc.evaluated_record_count.saturating_add(1);
                acc.observed_active_duration_ms = acc
                    .observed_active_duration_ms
                    .saturating_add(record.observed_duration_ms);
                acc.latencies_ms.push(*processing_latency_ms);

                match (record.expected_label, decision) {
                    (
                        GroundTruthLabel::PositiveTargetPresent,
                        SystemDecision::PositiveCandidateOrReconstruction,
                    ) => {
                        acc.true_positive_count = acc.true_positive_count.saturating_add(1);
                    }
                    (
                        GroundTruthLabel::PositiveTargetPresent,
                        SystemDecision::NegativeSuppressedOrFaultClassified
                        | SystemDecision::AbstainedInsufficientEvidence,
                    ) => {
                        acc.false_negative_count = acc.false_negative_count.saturating_add(1);
                    }
                    (
                        GroundTruthLabel::NegativeOrdinaryOrFault,
                        SystemDecision::PositiveCandidateOrReconstruction,
                    ) => {
                        acc.false_positive_count = acc.false_positive_count.saturating_add(1);
                    }
                    (
                        GroundTruthLabel::NegativeOrdinaryOrFault,
                        SystemDecision::NegativeSuppressedOrFaultClassified
                        | SystemDecision::AbstainedInsufficientEvidence,
                    ) => {
                        acc.true_negative_count = acc.true_negative_count.saturating_add(1);
                    }
                    (
                        GroundTruthLabel::DegenerateRequiresInsufficientEvidence,
                        SystemDecision::AbstainedInsufficientEvidence,
                    ) => {
                        acc.correct_degenerate_abstention_count =
                            acc.correct_degenerate_abstention_count.saturating_add(1);
                    }
                    (
                        GroundTruthLabel::DegenerateRequiresInsufficientEvidence,
                        SystemDecision::PositiveCandidateOrReconstruction,
                    ) => {
                        acc.false_degenerate_resolution_count =
                            acc.false_degenerate_resolution_count.saturating_add(1);
                        acc.false_positive_count = acc.false_positive_count.saturating_add(1);
                    }
                    (
                        GroundTruthLabel::DegenerateRequiresInsufficientEvidence,
                        SystemDecision::NegativeSuppressedOrFaultClassified,
                    ) => {
                        acc.true_negative_count = acc.true_negative_count.saturating_add(1);
                    }
                }
            }
            HeldOutItemOutcome::Skipped { reason } => {
                let detail = match reason {
                    HeldOutSkipReason::UnsupportedInputDomain { detail }
                    | HeldOutSkipReason::MissingOrInvalidatedCalibration { detail }
                    | HeldOutSkipReason::CoverageGapOrDroppedStream { detail }
                    | HeldOutSkipReason::BudgetOrDeadlineStopped { detail } => detail,
                };
                validate_text_field(detail, "held_out_skip_reason.detail", limits)?;
                acc.skipped_record_count = acc.skipped_record_count.saturating_add(1);
            }
        }

        outcomes_by_version.insert(item.record_version, item.outcome);
    }

    // Anti-cherry-picking and per-item receipt construction in deterministic manifest order.
    let mut item_receipts = Vec::new();
    item_receipts
        .try_reserve(held_out_by_version.len())
        .map_err(|_| ValidationLabError::AllocationFailed)?;

    for record in manifest.records() {
        if record.split == DatasetSplit::HeldOutEvaluation {
            let Some(outcome) = outcomes_by_version.remove(&record.record_version) else {
                return Err(ValidationLabError::HeldOutRecordSilentlyOmitted {
                    record_id: record.record_id.clone(),
                    record_version: record.record_version.clone(),
                });
            };
            item_receipts.push(HeldOutItemReceipt {
                record_id: record.record_id.clone(),
                record_version: record.record_version.clone(),
                domain: record.domain,
                is_simulation: record.is_simulation(),
                expected_label: record.expected_label,
                observed_duration_ms: record.observed_duration_ms,
                outcome,
            });
        }
    }

    if input.calibration_receipt.is_some() && evaluated_in_calibrated_domain_count == 0 {
        return Err(ValidationLabError::InvalidCalibrationReceipt {
            reason: "CalibrationReceipt cannot be attached when zero held-out items in calibrated_domain were evaluated",
        });
    }

    let score_calibration_status = match input.calibration_receipt {
        Some(receipt) => ScoreCalibrationStatus::CallerDeclaredReceiptUnverified { receipt },
        None => ScoreCalibrationStatus::UncalibratedDiagnosticScores,
    };

    check_stop().map_err(ValidationLabError::Stopped)?;
    Ok(HeldOutEvaluationReport {
        method_id: METHOD_ID,
        evaluation_run_id: input.evaluation_run_id,
        evaluation_run_version: input.evaluation_run_version,
        dataset_id: manifest.dataset_id().clone(),
        dataset_version: manifest.dataset_version().clone(),
        detector_or_solver_id: input.detector_or_solver_id,
        detector_or_solver_version: input.detector_or_solver_version,
        score_calibration_status,
        score_caveat: UNCALIBRATED_SCORE_CAVEAT,
        simulation_label: SIMULATION_LABEL,
        labeled_real_fixture_metrics: real_acc.finalize(),
        controlled_simulation_metrics: sim_acc.finalize(),
        item_receipts,
    })
}

/// Evaluates a caller-supplied operational, resource, backpressure, cancellation, and visual-parity
/// benchmark receipt against declared design targets (`#40`, `DEVICE-04`, `A09`, `A12`, `docs/delivery.md` §4).
pub fn evaluate_operational_benchmark<F>(
    mut input: OperationalBenchmarkInput,
    limits: &ValidationLabLimits,
    mut check_stop: F,
) -> Result<OperationalBenchmarkReport, ValidationLabError>
where
    F: FnMut() -> Result<(), StopReason>,
{
    check_stop().map_err(ValidationLabError::Stopped)?;

    if !is_valid_git_revision_hex(&input.environment.git_revision_hex) {
        return Err(ValidationLabError::InvalidGitRevisionHex);
    }
    if !is_valid_lowercase_sha256_hex(&input.environment.dataset_sha256) {
        return Err(ValidationLabError::InvalidSha256Hex {
            field: "dataset_sha256",
        });
    }
    validate_text_field(
        &input.environment.hardware_profile,
        "hardware_profile",
        limits,
    )?;
    validate_text_field(&input.environment.os_description, "os_description", limits)?;
    validate_text_field(
        &input.environment.renderer_description,
        "renderer_description",
        limits,
    )?;

    if input.environment.worker_concurrency == 0 {
        return Err(ValidationLabError::ZeroWorkerConcurrency);
    }
    if input.backpressure.configured_queue_capacity == 0 {
        return Err(ValidationLabError::ZeroQueueCapacity);
    }

    for (series_len, series_name) in [
        (
            input.ui_acknowledgment_latencies_ms.len(),
            "ui_acknowledgment_latencies_ms",
        ),
        (
            input.local_query_latencies_ms.len(),
            "local_query_latencies_ms",
        ),
        (
            input.external_provider_latencies_ms.len(),
            "external_provider_latencies_ms",
        ),
        (
            input.model_service_latencies_ms.len(),
            "model_service_latencies_ms",
        ),
    ] {
        if series_len > limits.max_latency_samples {
            return Err(ValidationLabError::LimitExceeded {
                kind: ValidationLabLimitKind::LatencySamples,
                limit: limits.max_latency_samples,
                actual: series_len,
            });
        }
        let _ = series_name;
    }

    let Some(ui_acknowledgment_latency) =
        summarize_latency_slice(&mut input.ui_acknowledgment_latencies_ms)
    else {
        return Err(ValidationLabError::EmptyRequiredLatencySeries {
            series: "ui_acknowledgment_latencies_ms",
        });
    };
    let Some(local_query_latency) = summarize_latency_slice(&mut input.local_query_latencies_ms)
    else {
        return Err(ValidationLabError::EmptyRequiredLatencySeries {
            series: "local_query_latencies_ms",
        });
    };

    check_stop().map_err(ValidationLabError::Stopped)?;
    let external_provider_latency =
        summarize_latency_slice(&mut input.external_provider_latencies_ms);
    let model_service_latency = summarize_latency_slice(&mut input.model_service_latencies_ms);

    // Enforce receive-only UAV/device invariant (`DEVICE-04`, `A12`).
    if input
        .responsiveness
        .unhandled_flight_control_commands_emitted
        > 0
    {
        return Err(ValidationLabError::FlightControlCommandEmitted {
            command_count: input
                .responsiveness
                .unhandled_flight_control_commands_emitted,
        });
    }

    // Enforce honest coverage under backpressure/drops (`AGENTS.md` §2, `docs/delivery.md` §4).
    if input.backpressure.reported_coverage_bps > 10_000 {
        return Err(ValidationLabError::InvalidCoverageBasisPoints {
            reported_coverage_bps: input.backpressure.reported_coverage_bps,
        });
    }
    if (input.backpressure.dropped_frames_or_samples > 0
        || input.backpressure.throttled_or_rejected_requests > 0)
        && input.backpressure.reported_coverage_bps == 10_000
    {
        return Err(ValidationLabError::HiddenCoverageLossUnderBackpressure {
            dropped_frames_or_samples: input.backpressure.dropped_frames_or_samples,
            throttled_or_rejected_requests: input.backpressure.throttled_or_rejected_requests,
            reported_coverage_bps: input.backpressure.reported_coverage_bps,
        });
    }

    // Enforce cancellation publication fence and atomic import recovery (`AGENTS.md` §5, `A01`).
    if input.cancellation.published_after_cancel_or_supersession > 0 {
        return Err(ValidationLabError::LatePublicationAfterCancel {
            published_count: input.cancellation.published_after_cancel_or_supersession,
        });
    }
    if input.cancellation.interrupted_import_left_partial_case {
        return Err(ValidationLabError::InterruptedImportCorruptedCase);
    }

    // Enforce numerical parity across visual quality modes (`AGENTS.md` §2, `docs/delivery.md` §4).
    if !is_valid_lowercase_sha256_hex(&input.visual_parity.standard_glass_numerical_digest_sha256) {
        return Err(ValidationLabError::InvalidSha256Hex {
            field: "standard_glass_numerical_digest_sha256",
        });
    }
    if !is_valid_lowercase_sha256_hex(&input.visual_parity.reduced_effects_numerical_digest_sha256)
    {
        return Err(ValidationLabError::InvalidSha256Hex {
            field: "reduced_effects_numerical_digest_sha256",
        });
    }
    if input.visual_parity.standard_glass_numerical_digest_sha256
        != input.visual_parity.reduced_effects_numerical_digest_sha256
    {
        return Err(ValidationLabError::VisualQualityAlteredNumericalResult {
            standard_glass_sha256: input.visual_parity.standard_glass_numerical_digest_sha256,
            reduced_effects_sha256: input.visual_parity.reduced_effects_numerical_digest_sha256,
        });
    }

    let ui_ack_target_outcome =
        if ui_acknowledgment_latency.p95_ms <= input.targets.ui_ack_p95_target_ms {
            TargetComparisonOutcome::WithinProposedTarget
        } else {
            TargetComparisonOutcome::ExceededProposedTarget
        };

    let local_query_target_outcome =
        if local_query_latency.p95_ms <= input.targets.warm_query_p95_target_ms {
            TargetComparisonOutcome::WithinProposedTarget
        } else {
            TargetComparisonOutcome::ExceededProposedTarget
        };

    let cancel_ack_target_outcome =
        if input.cancellation.cancel_acknowledgment_ms <= input.targets.cancel_ack_target_ms {
            TargetComparisonOutcome::WithinProposedTarget
        } else {
            TargetComparisonOutcome::ExceededProposedTarget
        };

    let worker_release_target_outcome = if input.cancellation.worker_resource_release_ms
        <= input.targets.worker_release_target_ms
    {
        TargetComparisonOutcome::WithinProposedTarget
    } else {
        TargetComparisonOutcome::ExceededProposedTarget
    };

    let globe_fps_target_outcome = if input.visual_parity.standard_glass_fps
        >= input.targets.globe_min_fps_target
        && input.visual_parity.reduced_effects_fps >= input.targets.globe_min_fps_target
    {
        TargetComparisonOutcome::WithinProposedTarget
    } else {
        TargetComparisonOutcome::ExceededProposedTarget
    };

    let telemetry_stall_target_outcome = if input.responsiveness.max_telemetry_dispatch_stall_ms
        <= input.targets.max_allowed_telemetry_stall_ms
        && input.responsiveness.case_edit_responsive_under_backpressure
    {
        TargetComparisonOutcome::WithinProposedTarget
    } else {
        TargetComparisonOutcome::ExceededProposedTarget
    };

    check_stop().map_err(ValidationLabError::Stopped)?;
    Ok(OperationalBenchmarkReport {
        method_id: METHOD_ID,
        target_vs_measured_caveat: TARGET_VS_MEASURED_CAVEAT,
        environment: input.environment,
        targets: input.targets,
        ui_acknowledgment_latency,
        local_query_latency,
        external_provider_latency,
        model_service_latency,
        ui_ack_target_outcome,
        local_query_target_outcome,
        cancel_ack_target_outcome,
        worker_release_target_outcome,
        globe_fps_target_outcome,
        telemetry_stall_target_outcome,
        responsiveness: input.responsiveness,
        backpressure: input.backpressure,
        cancellation: input.cancellation,
        visual_parity: input.visual_parity,
    })
}
