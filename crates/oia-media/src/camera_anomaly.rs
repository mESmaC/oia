//! Camera anomaly candidate packaging, health classification, deduplication,
//! append-only human review history, cross-camera association eligibility,
//! and held-out operational benchmark evaluation (`CAM-01`–`CAM-05`, `A09`).
//!
//! This module implements a dependency-independent in-memory validation and
//! evaluation kernel for `#29`:
//! - Enforces `CAM-01`/`CAM-02` camera admission rules before automated anomaly
//!   processing: home interiors, restricted sources, unpermitted automated
//!   processing, and non-live substitutes (`StillFallback`, `StreetViewFallback`,
//!   `SyntheticPlaceholder`) are refused as scene-anomaly event triggers, with
//!   restricted sources recorded as unavailable rather than triggering alternate
//!   access attempts.
//! - Separates camera-health and feed-fault observations (`FrozenFrame`,
//!   `ExposureShift`, `CameraRepositioned`, `RepeatedRebroadcast`, `TimingDrift`,
//!   `CoverageGap`, `SourceRestricted`) from candidate scene events
//!   (`SkyTransient`, `UnusualApparentMotion`, `RoadObstruction`,
//!   `StoppedVehicle`, `WrongWayTravel`, `EnvironmentalChange`) so camera faults
//!   never publish as scene anomalies (`CAM-03`, `A09`).
//! - Packages candidate scene events with pinned original frame/clip `VersionId`s
//!   and 64-character lowercase hex SHA-256 digests, bounded pre/post context
//!   frames (or explicit buffer-expiry/retention-policy omission status),
//!   trigger explanation, detector model/version/configuration identity, and
//!   raw uncalibrated detector scores paired with [`UNCALIBRATED_SCORE_CAVEAT`]
//!   and [`CANDIDATE_EVENT_CAVEAT`] (`CAM-04`, `CAM-05`).
//! - Deduplicates repeated notifications for persistent candidates within a
//!   configured window while preserving merged observation counts and explicit
//!   coverage-loss spans (`CAM-04`, `A09`).
//! - Maintains append-only human review history (`ArtifactOrFault`,
//!   `LikelyOrdinaryExplanation`, `InsufficientEvidence`,
//!   `RetainedForInvestigation`) with optimistic concurrency (`expected_current_version`)
//!   and non-reusable `VersionId`s (`CAM-05`).
//! - Validates cross-camera association and multi-view triangulation readiness,
//!   refusing shared upstream feeds, unsynchronized clocks, uncalibrated views,
//!   or calibrations invalidated by camera movement (`CAM-05`, `A09`).
//! - Computes held-out operational benchmark metrics: false alerts per observed
//!   camera-hour, labeled-event recall, detection latency summary, coverage
//!   fraction, and fault-rejection count (`CAM-03`–`CAM-05`, `A09`, `AGENTS.md` §8).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use oia_domain::{ActorId, IdError, OpaqueId, SchemaVersion, VersionId};

/// Mandatory caveat attached to every candidate camera anomaly event (`CAM-04`).
pub const CANDIDATE_EVENT_CAVEAT: &str = "Candidate camera anomaly for human review only; not an identification of a UAP, crime, or emergency.";

/// Mandatory caveat attached to raw detector scores (`CAM-05`, `AGENTS.md` §2).
pub const UNCALIBRATED_SCORE_CAVEAT: &str =
    "Raw detector score is an uncalibrated ranking signal, not a calibrated class probability.";

/// Mandatory policy notice when a camera source becomes restricted (`CAM-01`, `A09`).
pub const RESTRICTED_SOURCE_POLICY_NOTICE: &str = "Source is restricted or unavailable; OIA records unavailability and does not probe hidden endpoints, guess credentials, or seek alternate access.";

/// Which resource budget was zero or exceeded in the camera anomaly kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnomalyLimitKind {
    Regions,
    Observations,
    Candidates,
    ContextFrames,
    ReviewsPerCandidate,
    CoverageGaps,
    LabeledTruthEvents,
    TextBytes,
}

impl fmt::Display for AnomalyLimitKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Regions => f.write_str("max_regions"),
            Self::Observations => f.write_str("max_observations"),
            Self::Candidates => f.write_str("max_candidates"),
            Self::ContextFrames => f.write_str("max_context_frames"),
            Self::ReviewsPerCandidate => f.write_str("max_reviews_per_candidate"),
            Self::CoverageGaps => f.write_str("max_coverage_gaps"),
            Self::LabeledTruthEvents => f.write_str("max_labeled_truth_events"),
            Self::TextBytes => f.write_str("max_text_bytes"),
        }
    }
}

/// Explicit resource bounds for camera anomaly monitoring, review, and evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnomalyLimits {
    max_regions: usize,
    max_observations: usize,
    max_candidates: usize,
    max_context_frames: usize,
    max_reviews_per_candidate: usize,
    max_coverage_gaps: usize,
    max_labeled_truth_events: usize,
    max_text_bytes: usize,
}

impl AnomalyLimits {
    /// Validates that every budget is at least `1`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        max_regions: usize,
        max_observations: usize,
        max_candidates: usize,
        max_context_frames: usize,
        max_reviews_per_candidate: usize,
        max_coverage_gaps: usize,
        max_labeled_truth_events: usize,
        max_text_bytes: usize,
    ) -> Result<Self, AnomalyError> {
        if max_regions == 0 {
            return Err(AnomalyError::ZeroLimit(AnomalyLimitKind::Regions));
        }
        if max_observations == 0 {
            return Err(AnomalyError::ZeroLimit(AnomalyLimitKind::Observations));
        }
        if max_candidates == 0 {
            return Err(AnomalyError::ZeroLimit(AnomalyLimitKind::Candidates));
        }
        if max_context_frames == 0 {
            return Err(AnomalyError::ZeroLimit(AnomalyLimitKind::ContextFrames));
        }
        if max_reviews_per_candidate == 0 {
            return Err(AnomalyError::ZeroLimit(
                AnomalyLimitKind::ReviewsPerCandidate,
            ));
        }
        if max_coverage_gaps == 0 {
            return Err(AnomalyError::ZeroLimit(AnomalyLimitKind::CoverageGaps));
        }
        if max_labeled_truth_events == 0 {
            return Err(AnomalyError::ZeroLimit(
                AnomalyLimitKind::LabeledTruthEvents,
            ));
        }
        if max_text_bytes == 0 {
            return Err(AnomalyError::ZeroLimit(AnomalyLimitKind::TextBytes));
        }
        Ok(Self {
            max_regions,
            max_observations,
            max_candidates,
            max_context_frames,
            max_reviews_per_candidate,
            max_coverage_gaps,
            max_labeled_truth_events,
            max_text_bytes,
        })
    }

    #[must_use]
    pub const fn max_regions(self) -> usize {
        self.max_regions
    }

    #[must_use]
    pub const fn max_observations(self) -> usize {
        self.max_observations
    }

    #[must_use]
    pub const fn max_candidates(self) -> usize {
        self.max_candidates
    }

    #[must_use]
    pub const fn max_context_frames(self) -> usize {
        self.max_context_frames
    }

    #[must_use]
    pub const fn max_reviews_per_candidate(self) -> usize {
        self.max_reviews_per_candidate
    }

    #[must_use]
    pub const fn max_coverage_gaps(self) -> usize {
        self.max_coverage_gaps
    }

    #[must_use]
    pub const fn max_labeled_truth_events(self) -> usize {
        self.max_labeled_truth_events
    }

    #[must_use]
    pub const fn max_text_bytes(self) -> usize {
        self.max_text_bytes
    }
}

/// Ordinary-access status of a listed camera source (`CAM-01`, `CAM-02`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraAccessStatus {
    /// Feed is accessible through an existing directory, published viewing link, or supported embed/API.
    OrdinarilyAccessible,
    /// Feed became access-restricted or unavailable; OIA must stop and never seek alternate access.
    RestrictedOrUnavailable,
}

/// Assessed view classification for `CAM-01`/`CAM-02` privacy exclusion rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraViewKind {
    PublicStreetOrTraffic,
    SkyOrObservatory,
    WeatherOrEnvironment,
    CommercialExterior,
    /// Strictly excluded by `CAM-01`/`CAM-02`.
    HomeInteriorOrPrivateResidence,
}

/// Origin kind of imagery supplied for a camera source (`CAM-02`, `A09`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedObservationKind {
    /// Live or sequentially captured upstream camera frames/clips eligible for observation evidence.
    UpstreamCameraFeed,
    /// Static stale fallback image; must not masquerade as live observation.
    StillFallback,
    /// Street-level archival imagery substitute; must not serve as live event evidence.
    StreetViewFallback,
    /// Synthetic placeholder frame; must never serve as observation evidence.
    SyntheticPlaceholder,
}

impl fmt::Display for FeedObservationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UpstreamCameraFeed => f.write_str("upstream_camera_feed"),
            Self::StillFallback => f.write_str("still_fallback"),
            Self::StreetViewFallback => f.write_str("street_view_fallback"),
            Self::SyntheticPlaceholder => f.write_str("synthetic_placeholder"),
        }
    }
}

/// Status of geometric camera calibration (`CAM-05`, `A09`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CameraCalibrationState {
    /// Camera orientation/FOV is uncalibrated.
    Uncalibrated,
    /// Valid calibration pinned to an immutable calibration version and angular error bound.
    Valid {
        calibration_version: VersionId,
        max_angular_error_mdeg: u32,
    },
    /// Calibration was invalidated because the camera moved, zoomed, or repositioned.
    InvalidatedByMovement {
        prior_calibration_version: VersionId,
        detected_at_ms: i64,
    },
}

/// Camera source profile validated under `CAM-01` and `CAM-02`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraSourceProfile {
    pub camera_id: OpaqueId,
    pub upstream_feed_id: OpaqueId,
    pub provider_attribution: String,
    pub listing_reference: String,
    pub access_status: CameraAccessStatus,
    pub view_kind: CameraViewKind,
    pub feed_kind: FeedObservationKind,
    pub automated_processing_permitted: bool,
    pub clip_retention_permitted: bool,
    pub max_retention_window_ms: Option<u64>,
    pub calibration: CameraCalibrationState,
}

/// Scene baseline regime for adaptive anomaly thresholds (`CAM-03`, `A09`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SceneBaselineKind {
    DaylightClear,
    NighttimeLowLight,
    TwilightTransition,
    AdverseWeatherOrPrecipitation,
}

impl fmt::Display for SceneBaselineKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DaylightClear => f.write_str("daylight_clear"),
            Self::NighttimeLowLight => f.write_str("nighttime_low_light"),
            Self::TwilightTransition => f.write_str("twilight_transition"),
            Self::AdverseWeatherOrPrecipitation => f.write_str("adverse_weather_or_precipitation"),
        }
    }
}

/// Scoped detection region and sensitivity configuration (`CAM-03`, `CAM-04`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectionRegionConfig {
    pub region_id: OpaqueId,
    pub label: String,
    pub baseline: SceneBaselineKind,
    /// Minimum raw score in basis points (`1..=10_000`) required to trigger a candidate.
    pub score_threshold_bps: u16,
    /// Minimum consecutive qualifying frames (`>= 1`) required before triggering.
    pub min_persistence_frames: u32,
    /// Minimum persistence duration in milliseconds (`>= 0`).
    pub min_persistence_ms: u64,
    /// Window in milliseconds within which repeated triggers of the same kind/region deduplicate.
    pub deduplication_window_ms: u64,
    /// Whether the region includes a privacy mask excluding private windows/areas (`CAM-02`).
    pub privacy_mask_applied: bool,
}

/// Candidate scene anomaly category (`CAM-03`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AnomalyCategory {
    SkyTransient,
    UnusualApparentMotion,
    RoadObstruction,
    StoppedVehicle,
    WrongWayTravel,
    EnvironmentalChange,
}

impl fmt::Display for AnomalyCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SkyTransient => f.write_str("sky_transient"),
            Self::UnusualApparentMotion => f.write_str("unusual_apparent_motion"),
            Self::RoadObstruction => f.write_str("road_obstruction"),
            Self::StoppedVehicle => f.write_str("stopped_vehicle"),
            Self::WrongWayTravel => f.write_str("wrong_way_travel"),
            Self::EnvironmentalChange => f.write_str("environmental_change"),
        }
    }
}

/// Distinct camera-health or feed-fault state (`CAM-03`, `A09`).
///
/// Faults are tracked separately from scene events and must never emit a
/// candidate scene anomaly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CameraHealthState {
    Nominal,
    FrozenFrame,
    ExposureShift,
    CameraRepositioned,
    RepeatedRebroadcast,
    TimingDrift,
    CoverageGap,
    SourceRestricted,
}

impl fmt::Display for CameraHealthState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nominal => f.write_str("nominal"),
            Self::FrozenFrame => f.write_str("frozen_frame"),
            Self::ExposureShift => f.write_str("exposure_shift"),
            Self::CameraRepositioned => f.write_str("camera_repositioned"),
            Self::RepeatedRebroadcast => f.write_str("repeated_rebroadcast"),
            Self::TimingDrift => f.write_str("timing_drift"),
            Self::CoverageGap => f.write_str("coverage_gap"),
            Self::SourceRestricted => f.write_str("source_restricted"),
        }
    }
}

/// Origin class of a retained frame or clip artifact (`AGENTS.md` §2, `CAM-03`, `MEDIA-03`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameArtifactOrigin {
    /// Original retained upstream camera bytes.
    OriginalRetainedFrame,
    /// Forbidden as original evidence: predictive restoration, super-resolution, or interpolated frame.
    PredictiveOrGeneratedDerivative,
}

/// Pinned original frame or clip reference (`CAM-04`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetainedFrameRef {
    pub frame_version: VersionId,
    pub sha256_hex: String,
    pub capture_time_ms: i64,
    pub ingestion_time_ms: i64,
    pub origin: FrameArtifactOrigin,
}

/// Availability of pre-event and post-event context frames (`CAM-04`, `A09`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextWindowStatus {
    /// Both pre-event and post-event context frames were retained within policy bounds.
    Complete {
        pre_frames: Vec<RetainedFrameRef>,
        post_frames: Vec<RetainedFrameRef>,
    },
    /// Some context frames expired from the bounded ring buffer before capture completed.
    PartialBufferExpired {
        pre_frames: Vec<RetainedFrameRef>,
        post_frames: Vec<RetainedFrameRef>,
        expired_frame_count: u32,
    },
    /// Source terms permit live inspection only and forbid retaining context buffers.
    OmittedByRetentionPolicy { reason: String },
}

/// Detector model and configuration metadata (`CAM-04`, `AGENTS.md` §3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectorRunMetadata {
    pub detector_id: OpaqueId,
    pub detector_version: VersionId,
    pub method_description: String,
    pub config_digest_sha256: String,
}

/// Observation step submitted to the camera anomaly monitor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservationStepInput {
    /// A candidate scene detection emitted by a scoped detector on an upstream frame.
    SceneTrigger {
        observation_id: OpaqueId,
        candidate_id: OpaqueId,
        candidate_version: VersionId,
        region_id: OpaqueId,
        category: AnomalyCategory,
        baseline_at_trigger: SceneBaselineKind,
        event_start_ms: i64,
        event_end_ms: i64,
        detection_emitted_ms: i64,
        observed_persistence_frames: u32,
        raw_score_bps: u16,
        clock_uncertainty_ms: u64,
        trigger_explanation: String,
        primary_frame: RetainedFrameRef,
        context_window: ContextWindowStatus,
    },
    /// A camera-health fault or coverage interruption observed on the feed (`CAM-03`, `A09`).
    HealthEvent {
        observation_id: OpaqueId,
        health_state: CameraHealthState,
        start_ms: i64,
        end_ms: i64,
        detail: String,
    },
}

/// Recorded camera-health or coverage-loss event (`CAM-03`, `CAM-04`, `A09`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedHealthEvent {
    observation_id: OpaqueId,
    health_state: CameraHealthState,
    start_ms: i64,
    end_ms: i64,
    detail: String,
}

impl RecordedHealthEvent {
    #[must_use]
    pub fn observation_id(&self) -> &OpaqueId {
        &self.observation_id
    }

    #[must_use]
    pub const fn health_state(&self) -> CameraHealthState {
        self.health_state
    }

    #[must_use]
    pub const fn start_ms(&self) -> i64 {
        self.start_ms
    }

    #[must_use]
    pub const fn end_ms(&self) -> i64 {
        self.end_ms
    }

    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }

    #[must_use]
    pub const fn duration_ms(&self) -> u64 {
        self.end_ms.saturating_sub(self.start_ms) as u64
    }
}

/// Human review disposition for a candidate camera anomaly (`CAM-05`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnomalyReviewDisposition {
    /// Camera compression artifact, glare, insect, sensor noise, or feed fault.
    ArtifactOrFault,
    /// Likely ordinary activity (aircraft, bird, vehicle headlights, weather shift).
    LikelyOrdinaryExplanation,
    /// Available frames/context cannot establish or rule out an explanation.
    InsufficientEvidence,
    /// Retained as an open investigation item with preserved original context.
    RetainedForInvestigation,
}

impl fmt::Display for AnomalyReviewDisposition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ArtifactOrFault => f.write_str("artifact_or_fault"),
            Self::LikelyOrdinaryExplanation => f.write_str("likely_ordinary_explanation"),
            Self::InsufficientEvidence => f.write_str("insufficient_evidence"),
            Self::RetainedForInvestigation => f.write_str("retained_for_investigation"),
        }
    }
}

/// Immutable entry in a candidate anomaly's human review history (`CAM-05`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnomalyReviewEntry {
    review_version: VersionId,
    supersedes_version: VersionId,
    reviewer: ActorId,
    reviewed_at_ms: i64,
    disposition: AnomalyReviewDisposition,
    rationale: String,
    cited_frame_versions: Vec<VersionId>,
}

impl AnomalyReviewEntry {
    #[must_use]
    pub fn review_version(&self) -> &VersionId {
        &self.review_version
    }

    #[must_use]
    pub fn supersedes_version(&self) -> &VersionId {
        &self.supersedes_version
    }

    #[must_use]
    pub fn reviewer(&self) -> &ActorId {
        &self.reviewer
    }

    #[must_use]
    pub const fn reviewed_at_ms(&self) -> i64 {
        self.reviewed_at_ms
    }

    #[must_use]
    pub const fn disposition(&self) -> AnomalyReviewDisposition {
        self.disposition
    }

    #[must_use]
    pub fn rationale(&self) -> &str {
        &self.rationale
    }

    #[must_use]
    pub fn cited_frame_versions(&self) -> &[VersionId] {
        &self.cited_frame_versions
    }
}

/// Validated candidate camera anomaly event with original context and review history (`CAM-04`, `CAM-05`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateAnomalyEvent {
    candidate_id: OpaqueId,
    initial_version: VersionId,
    current_version: VersionId,
    schema_version: SchemaVersion,
    camera_id: OpaqueId,
    region_id: OpaqueId,
    category: AnomalyCategory,
    baseline_at_trigger: SceneBaselineKind,
    event_start_ms: i64,
    event_end_ms: i64,
    first_detection_emitted_ms: i64,
    observed_persistence_frames: u32,
    peak_raw_score_bps: u16,
    clock_uncertainty_ms: u64,
    deduplicated_trigger_count: u32,
    trigger_explanation: String,
    detector: DetectorRunMetadata,
    primary_frame: RetainedFrameRef,
    context_window: ContextWindowStatus,
    reviews: Vec<AnomalyReviewEntry>,
}

impl CandidateAnomalyEvent {
    #[must_use]
    pub fn candidate_id(&self) -> &OpaqueId {
        &self.candidate_id
    }

    #[must_use]
    pub fn initial_version(&self) -> &VersionId {
        &self.initial_version
    }

    #[must_use]
    pub fn current_version(&self) -> &VersionId {
        &self.current_version
    }

    #[must_use]
    pub const fn schema_version(&self) -> SchemaVersion {
        self.schema_version
    }

    #[must_use]
    pub fn camera_id(&self) -> &OpaqueId {
        &self.camera_id
    }

    #[must_use]
    pub fn region_id(&self) -> &OpaqueId {
        &self.region_id
    }

    #[must_use]
    pub const fn category(&self) -> AnomalyCategory {
        self.category
    }

    #[must_use]
    pub const fn baseline_at_trigger(&self) -> SceneBaselineKind {
        self.baseline_at_trigger
    }

    #[must_use]
    pub const fn event_start_ms(&self) -> i64 {
        self.event_start_ms
    }

    #[must_use]
    pub const fn event_end_ms(&self) -> i64 {
        self.event_end_ms
    }

    #[must_use]
    pub const fn first_detection_emitted_ms(&self) -> i64 {
        self.first_detection_emitted_ms
    }

    #[must_use]
    pub const fn detection_latency_ms(&self) -> u64 {
        self.first_detection_emitted_ms
            .saturating_sub(self.event_start_ms) as u64
    }

    #[must_use]
    pub const fn observed_persistence_frames(&self) -> u32 {
        self.observed_persistence_frames
    }

    #[must_use]
    pub const fn peak_raw_score_bps(&self) -> u16 {
        self.peak_raw_score_bps
    }

    #[must_use]
    pub const fn is_score_calibrated_probability(&self) -> bool {
        false
    }

    #[must_use]
    pub const fn score_caveat(&self) -> &'static str {
        UNCALIBRATED_SCORE_CAVEAT
    }

    #[must_use]
    pub const fn candidate_caveat(&self) -> &'static str {
        CANDIDATE_EVENT_CAVEAT
    }

    #[must_use]
    pub const fn clock_uncertainty_ms(&self) -> u64 {
        self.clock_uncertainty_ms
    }

    #[must_use]
    pub const fn deduplicated_trigger_count(&self) -> u32 {
        self.deduplicated_trigger_count
    }

    #[must_use]
    pub fn trigger_explanation(&self) -> &str {
        &self.trigger_explanation
    }

    #[must_use]
    pub fn detector(&self) -> &DetectorRunMetadata {
        &self.detector
    }

    #[must_use]
    pub fn primary_frame(&self) -> &RetainedFrameRef {
        &self.primary_frame
    }

    #[must_use]
    pub fn context_window(&self) -> &ContextWindowStatus {
        &self.context_window
    }

    #[must_use]
    pub fn reviews(&self) -> &[AnomalyReviewEntry] {
        &self.reviews
    }

    #[must_use]
    pub fn current_disposition(&self) -> Option<AnomalyReviewDisposition> {
        self.reviews.last().map(AnomalyReviewEntry::disposition)
    }
}

/// Outcome of processing a scene trigger step (`CAM-04`, `A09`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerOutcome {
    /// Emitted a new candidate notification with its assigned `candidate_id`.
    PublishedNewCandidate { candidate_id: OpaqueId },
    /// Merged into an existing active candidate within the region's deduplication window.
    DeduplicatedIntoExisting {
        candidate_id: OpaqueId,
        merged_trigger_count: u32,
    },
}

/// Input for appending a human review to a candidate camera anomaly (`CAM-05`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnomalyReviewInput {
    pub candidate_id: OpaqueId,
    pub expected_current_version: VersionId,
    pub new_review_version: VersionId,
    pub reviewer: ActorId,
    pub reviewed_at_ms: i64,
    pub disposition: AnomalyReviewDisposition,
    pub rationale: String,
    pub cited_frame_versions: Vec<VersionId>,
}

/// Held-out ground-truth labeled scene event for operational detector evaluation (`A09`, `AGENTS.md` §8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabeledSceneEvent {
    pub truth_id: OpaqueId,
    pub region_id: OpaqueId,
    pub category: AnomalyCategory,
    pub start_ms: i64,
    pub end_ms: i64,
}

/// Operational evaluation metrics on held-out camera data (`CAM-03`–`CAM-05`, `A09`, `AGENTS.md` §8).
///
/// Reports operational false alerts per observed camera-hour, labeled-event recall,
/// detection latency, and coverage loss rather than substituting raw frame-level
/// classifier accuracy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationalEvaluationSummary {
    pub camera_id: OpaqueId,
    pub detector_id: OpaqueId,
    pub detector_version: VersionId,
    pub total_window_ms: u64,
    pub missing_coverage_ms: u64,
    pub active_coverage_ms: u64,
    pub coverage_bps: u16,
    pub labeled_event_count: usize,
    pub matched_labeled_event_count: usize,
    /// Recall in basis points (`0..=10_000`), or `None` when `labeled_event_count == 0`.
    pub event_recall_bps: Option<u16>,
    pub total_candidate_alerts: usize,
    pub true_positive_alerts: usize,
    pub false_alert_count: usize,
    /// Operational false alerts per active observed camera-hour scaled by `1_000` (milli-alerts/hour).
    pub false_alerts_per_camera_hour_milli: u64,
    pub min_detection_latency_ms: Option<u64>,
    pub mean_detection_latency_ms: Option<u64>,
    pub max_detection_latency_ms: Option<u64>,
    pub deduplicated_repeat_triggers: u32,
    pub fault_events_excluded_from_alerts: usize,
}

/// Validated cross-camera association and multi-view triangulation readiness (`CAM-05`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrossCameraAssociation {
    pub first_candidate_id: OpaqueId,
    pub second_candidate_id: OpaqueId,
    pub first_camera_id: OpaqueId,
    pub second_camera_id: OpaqueId,
    pub first_calibration_version: VersionId,
    pub second_calibration_version: VersionId,
    pub temporal_skew_ms: u64,
    pub combined_clock_uncertainty_ms: u64,
    pub correspondence_rationale: String,
}

impl CrossCameraAssociation {
    #[must_use]
    pub const fn eligible_for_triangulation(&self) -> bool {
        true
    }
}

/// Errors returned by the camera anomaly detection and review kernel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnomalyError {
    InvalidId(IdError),
    ZeroLimit(AnomalyLimitKind),
    LimitExceeded {
        kind: AnomalyLimitKind,
        actual: usize,
        max: usize,
    },
    EmptyField {
        field: &'static str,
    },
    ControlCharacterInField {
        field: &'static str,
    },
    HomeInteriorExcluded {
        camera_id: OpaqueId,
    },
    SourceRestricted {
        camera_id: OpaqueId,
        policy_notice: &'static str,
    },
    AutomatedProcessingNotPermitted {
        camera_id: OpaqueId,
    },
    NonObservationFeedSubstitute {
        camera_id: OpaqueId,
        feed_kind: FeedObservationKind,
    },
    RetentionPolicyForbidsClips {
        camera_id: OpaqueId,
    },
    ContextExceedsRetentionWindow {
        camera_id: OpaqueId,
        span_ms: u64,
        max_window_ms: u64,
    },
    InvalidScoreThresholdBps(u16),
    InvalidRawScoreBps(u16),
    ZeroPersistenceFrames {
        region_id: OpaqueId,
    },
    DuplicateRegionId(OpaqueId),
    UnknownRegionId(OpaqueId),
    DuplicateObservationId(OpaqueId),
    DuplicateCandidateId(OpaqueId),
    UnknownCandidateId(OpaqueId),
    DuplicateTruthEventId(OpaqueId),
    ReusedVersionId(VersionId),
    StaleCandidateVersion {
        candidate_id: OpaqueId,
        expected: VersionId,
        actual: VersionId,
    },
    InvalidTimeInterval {
        start_ms: i64,
        end_ms: i64,
    },
    DetectionBeforeEventStart {
        event_start_ms: i64,
        detection_emitted_ms: i64,
    },
    OutOfOrderObservation {
        previous_start_ms: i64,
        current_start_ms: i64,
    },
    InvalidSha256Hex {
        field: &'static str,
    },
    GeneratedFrameForbiddenAsEvidence {
        frame_version: VersionId,
    },
    CameraFaultBlocksSceneTrigger {
        camera_id: OpaqueId,
        active_health: CameraHealthState,
    },
    NominalHealthEventForbidden {
        observation_id: OpaqueId,
    },
    BelowScoreThreshold {
        score_bps: u16,
        threshold_bps: u16,
    },
    BelowPersistenceRequirement {
        observed_frames: u32,
        required_frames: u32,
        observed_ms: u64,
        required_ms: u64,
    },
    EmptyCitedFrames {
        candidate_id: OpaqueId,
    },
    UnknownCitedFrameVersion {
        candidate_id: OpaqueId,
        frame_version: VersionId,
    },
    SameCameraAssociation {
        camera_id: OpaqueId,
    },
    SharedUpstreamFeedDependency {
        upstream_feed_id: OpaqueId,
    },
    CalibrationRequiredForTriangulation {
        camera_id: OpaqueId,
    },
    CalibrationInvalidatedByMovement {
        camera_id: OpaqueId,
        prior_calibration_version: VersionId,
        detected_at_ms: i64,
    },
    TemporalSkewExceedsTolerance {
        skew_ms: u64,
        combined_uncertainty_ms: u64,
        max_allowed_skew_ms: u64,
    },
    EmptyEvaluationActiveCoverage,
}

impl fmt::Display for AnomalyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId(err) => write!(f, "invalid identifier: {err}"),
            Self::ZeroLimit(kind) => write!(f, "anomaly limit {kind} must be >= 1"),
            Self::LimitExceeded { kind, actual, max } => {
                write!(f, "{kind} count/size {actual} exceeds limit {max}")
            }
            Self::EmptyField { field } => write!(f, "field `{field}` must not be empty"),
            Self::ControlCharacterInField { field } => write!(
                f,
                "field `{field}` contains disallowed control, line-break, bidi, or zero-width characters"
            ),
            Self::HomeInteriorExcluded { camera_id } => write!(
                f,
                "camera `{camera_id}` views a home interior or private residence and is excluded by CAM-01/CAM-02"
            ),
            Self::SourceRestricted {
                camera_id,
                policy_notice,
            } => write!(f, "camera `{camera_id}` restricted: {policy_notice}"),
            Self::AutomatedProcessingNotPermitted { camera_id } => write!(
                f,
                "camera `{camera_id}` source terms do not permit automated anomaly processing"
            ),
            Self::NonObservationFeedSubstitute {
                camera_id,
                feed_kind,
            } => write!(
                f,
                "camera `{camera_id}` supplies `{feed_kind}`, which cannot serve as scene-anomaly observation evidence"
            ),
            Self::RetentionPolicyForbidsClips { camera_id } => write!(
                f,
                "camera `{camera_id}` retention policy forbids retaining pre/post context frames"
            ),
            Self::ContextExceedsRetentionWindow {
                camera_id,
                span_ms,
                max_window_ms,
            } => write!(
                f,
                "camera `{camera_id}` retained context span {span_ms} ms exceeds permitted window {max_window_ms} ms"
            ),
            Self::InvalidScoreThresholdBps(bps) => {
                write!(f, "score threshold {bps} bps must be within 1..=10000")
            }
            Self::InvalidRawScoreBps(bps) => {
                write!(f, "raw score {bps} bps must be within 0..=10000")
            }
            Self::ZeroPersistenceFrames { region_id } => write!(
                f,
                "region `{region_id}` min_persistence_frames must be >= 1"
            ),
            Self::DuplicateRegionId(id) => write!(f, "duplicate region_id `{id}`"),
            Self::UnknownRegionId(id) => write!(f, "unknown region_id `{id}`"),
            Self::DuplicateObservationId(id) => write!(f, "duplicate observation_id `{id}`"),
            Self::DuplicateCandidateId(id) => write!(f, "duplicate candidate_id `{id}`"),
            Self::UnknownCandidateId(id) => write!(f, "unknown candidate_id `{id}`"),
            Self::DuplicateTruthEventId(id) => write!(f, "duplicate truth_id `{id}`"),
            Self::ReusedVersionId(version) => {
                write!(f, "version_id `{version}` cannot be reused")
            }
            Self::StaleCandidateVersion {
                candidate_id,
                expected,
                actual,
            } => write!(
                f,
                "candidate `{candidate_id}` version mismatch: expected `{expected}`, current is `{actual}`"
            ),
            Self::InvalidTimeInterval { start_ms, end_ms } => write!(
                f,
                "invalid time interval [{start_ms}, {end_ms}): end_ms must be >= start_ms"
            ),
            Self::DetectionBeforeEventStart {
                event_start_ms,
                detection_emitted_ms,
            } => write!(
                f,
                "detection timestamp {detection_emitted_ms} precedes event start {event_start_ms}"
            ),
            Self::OutOfOrderObservation {
                previous_start_ms,
                current_start_ms,
            } => write!(
                f,
                "observation at {current_start_ms} precedes previous observation at {previous_start_ms}"
            ),
            Self::InvalidSha256Hex { field } => write!(
                f,
                "field `{field}` must be a 64-character lowercase hexadecimal SHA-256 digest"
            ),
            Self::GeneratedFrameForbiddenAsEvidence { frame_version } => write!(
                f,
                "frame `{frame_version}` is a predictive or generated derivative and cannot serve as original anomaly evidence"
            ),
            Self::CameraFaultBlocksSceneTrigger {
                camera_id,
                active_health,
            } => write!(
                f,
                "camera `{camera_id}` has active fault `{active_health}`; faults must not emit scene anomalies"
            ),
            Self::NominalHealthEventForbidden { observation_id } => write!(
                f,
                "health event `{observation_id}` must record a non-nominal fault or coverage gap"
            ),
            Self::BelowScoreThreshold {
                score_bps,
                threshold_bps,
            } => write!(
                f,
                "raw score {score_bps} bps is below region threshold {threshold_bps} bps"
            ),
            Self::BelowPersistenceRequirement {
                observed_frames,
                required_frames,
                observed_ms,
                required_ms,
            } => write!(
                f,
                "persistence ({observed_frames} frames, {observed_ms} ms) is below required ({required_frames} frames, {required_ms} ms)"
            ),
            Self::EmptyCitedFrames { candidate_id } => write!(
                f,
                "review for candidate `{candidate_id}` must cite at least one retained frame version"
            ),
            Self::UnknownCitedFrameVersion {
                candidate_id,
                frame_version,
            } => write!(
                f,
                "review for candidate `{candidate_id}` cites unknown frame version `{frame_version}`"
            ),
            Self::SameCameraAssociation { camera_id } => write!(
                f,
                "cross-camera association requires distinct cameras, got `{camera_id}` for both"
            ),
            Self::SharedUpstreamFeedDependency { upstream_feed_id } => write!(
                f,
                "cameras share upstream feed `{upstream_feed_id}` and are not independent views"
            ),
            Self::CalibrationRequiredForTriangulation { camera_id } => write!(
                f,
                "camera `{camera_id}` is uncalibrated and cannot be used for multi-view triangulation"
            ),
            Self::CalibrationInvalidatedByMovement {
                camera_id,
                prior_calibration_version,
                detected_at_ms,
            } => write!(
                f,
                "camera `{camera_id}` calibration `{prior_calibration_version}` was invalidated by camera movement at {detected_at_ms}"
            ),
            Self::TemporalSkewExceedsTolerance {
                skew_ms,
                combined_uncertainty_ms,
                max_allowed_skew_ms,
            } => write!(
                f,
                "temporal skew {skew_ms} ms + combined clock uncertainty {combined_clock_uncertainty_ms} ms exceeds tolerance {max_allowed_skew_ms} ms",
                combined_clock_uncertainty_ms = combined_uncertainty_ms
            ),
            Self::EmptyEvaluationActiveCoverage => write!(
                f,
                "evaluation window has zero active camera coverage after subtracting coverage gaps"
            ),
        }
    }
}

impl std::error::Error for AnomalyError {}

fn is_disallowed_single_line_char(ch: char) -> bool {
    ch.is_control()
        || matches!(
            ch,
            '\u{0085}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{200B}'..='\u{200F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2060}'..='\u{2069}'
                | '\u{FEFF}'
        )
}

fn validate_single_line_text(
    value: &str,
    field: &'static str,
    used_text_bytes: &mut usize,
    limits: AnomalyLimits,
) -> Result<(), AnomalyError> {
    if value.trim().is_empty() {
        return Err(AnomalyError::EmptyField { field });
    }
    if value.chars().any(is_disallowed_single_line_char) {
        return Err(AnomalyError::ControlCharacterInField { field });
    }
    let next = used_text_bytes.saturating_add(value.len());
    if next > limits.max_text_bytes {
        return Err(AnomalyError::LimitExceeded {
            kind: AnomalyLimitKind::TextBytes,
            actual: next,
            max: limits.max_text_bytes,
        });
    }
    *used_text_bytes = next;
    Ok(())
}

fn validate_sha256_hex(digest: &str, field: &'static str) -> Result<(), AnomalyError> {
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(AnomalyError::InvalidSha256Hex { field });
    }
    Ok(())
}

fn validate_frame_ref(
    frame: &RetainedFrameRef,
    seen_versions: &mut BTreeSet<VersionId>,
) -> Result<(), AnomalyError> {
    if frame.origin != FrameArtifactOrigin::OriginalRetainedFrame {
        return Err(AnomalyError::GeneratedFrameForbiddenAsEvidence {
            frame_version: frame.frame_version.clone(),
        });
    }
    validate_sha256_hex(&frame.sha256_hex, "frame.sha256_hex")?;
    if frame.ingestion_time_ms < frame.capture_time_ms {
        return Err(AnomalyError::InvalidTimeInterval {
            start_ms: frame.capture_time_ms,
            end_ms: frame.ingestion_time_ms,
        });
    }
    if !seen_versions.insert(frame.frame_version.clone()) {
        return Err(AnomalyError::ReusedVersionId(frame.frame_version.clone()));
    }
    Ok(())
}

/// Bounded in-memory monitor for a single camera source, tracking health states,
/// candidate scene anomalies, deduplication, and append-only human reviews (`CAM-01`–`CAM-05`).
#[derive(Debug, Clone)]
pub struct CameraAnomalyMonitor {
    source: CameraSourceProfile,
    detector: DetectorRunMetadata,
    limits: AnomalyLimits,
    regions: BTreeMap<OpaqueId, DetectionRegionConfig>,
    candidates: Vec<CandidateAnomalyEvent>,
    candidate_index: BTreeMap<OpaqueId, usize>,
    health_events: Vec<RecordedHealthEvent>,
    seen_observation_ids: BTreeSet<OpaqueId>,
    seen_versions: BTreeSet<VersionId>,
    last_observation_start_ms: Option<i64>,
    used_text_bytes: usize,
    total_deduplicated_repeats: u32,
}

impl CameraAnomalyMonitor {
    /// Validates `source`, `detector`, and `regions` against `CAM-01`–`CAM-03` before
    /// admitting any observations.
    pub fn new(
        source: CameraSourceProfile,
        detector: DetectorRunMetadata,
        regions: Vec<DetectionRegionConfig>,
        limits: AnomalyLimits,
    ) -> Result<Self, AnomalyError> {
        if source.view_kind == CameraViewKind::HomeInteriorOrPrivateResidence {
            return Err(AnomalyError::HomeInteriorExcluded {
                camera_id: source.camera_id,
            });
        }
        if source.access_status == CameraAccessStatus::RestrictedOrUnavailable {
            return Err(AnomalyError::SourceRestricted {
                camera_id: source.camera_id,
                policy_notice: RESTRICTED_SOURCE_POLICY_NOTICE,
            });
        }
        if !source.automated_processing_permitted {
            return Err(AnomalyError::AutomatedProcessingNotPermitted {
                camera_id: source.camera_id,
            });
        }
        if source.feed_kind != FeedObservationKind::UpstreamCameraFeed {
            return Err(AnomalyError::NonObservationFeedSubstitute {
                camera_id: source.camera_id,
                feed_kind: source.feed_kind,
            });
        }
        if regions.is_empty() {
            return Err(AnomalyError::EmptyField { field: "regions" });
        }
        if regions.len() > limits.max_regions {
            return Err(AnomalyError::LimitExceeded {
                kind: AnomalyLimitKind::Regions,
                actual: regions.len(),
                max: limits.max_regions,
            });
        }

        let mut used_text_bytes = 0usize;
        validate_single_line_text(
            &source.provider_attribution,
            "source.provider_attribution",
            &mut used_text_bytes,
            limits,
        )?;
        validate_single_line_text(
            &source.listing_reference,
            "source.listing_reference",
            &mut used_text_bytes,
            limits,
        )?;
        validate_single_line_text(
            &detector.method_description,
            "detector.method_description",
            &mut used_text_bytes,
            limits,
        )?;
        validate_sha256_hex(
            &detector.config_digest_sha256,
            "detector.config_digest_sha256",
        )?;

        let mut seen_versions = BTreeSet::new();
        seen_versions.insert(detector.detector_version.clone());
        match &source.calibration {
            CameraCalibrationState::Uncalibrated => {}
            CameraCalibrationState::Valid {
                calibration_version,
                ..
            } => {
                if !seen_versions.insert(calibration_version.clone()) {
                    return Err(AnomalyError::ReusedVersionId(calibration_version.clone()));
                }
            }
            CameraCalibrationState::InvalidatedByMovement {
                prior_calibration_version,
                ..
            } => {
                if !seen_versions.insert(prior_calibration_version.clone()) {
                    return Err(AnomalyError::ReusedVersionId(
                        prior_calibration_version.clone(),
                    ));
                }
            }
        }

        let mut region_map = BTreeMap::new();
        for region in regions {
            validate_single_line_text(&region.label, "region.label", &mut used_text_bytes, limits)?;
            if region.score_threshold_bps == 0 || region.score_threshold_bps > 10_000 {
                return Err(AnomalyError::InvalidScoreThresholdBps(
                    region.score_threshold_bps,
                ));
            }
            if region.min_persistence_frames == 0 {
                return Err(AnomalyError::ZeroPersistenceFrames {
                    region_id: region.region_id,
                });
            }
            if region_map
                .insert(region.region_id.clone(), region)
                .is_some()
            {
                if let Some((dup_id, _)) = region_map.iter().next_back() {
                    return Err(AnomalyError::DuplicateRegionId(dup_id.clone()));
                }
            }
        }

        Ok(Self {
            source,
            detector,
            limits,
            regions: region_map,
            candidates: Vec::new(),
            candidate_index: BTreeMap::new(),
            health_events: Vec::new(),
            seen_observation_ids: BTreeSet::new(),
            seen_versions,
            last_observation_start_ms: None,
            used_text_bytes,
            total_deduplicated_repeats: 0,
        })
    }

    #[must_use]
    pub fn source(&self) -> &CameraSourceProfile {
        &self.source
    }

    #[must_use]
    pub fn detector(&self) -> &DetectorRunMetadata {
        &self.detector
    }

    #[must_use]
    pub fn candidates(&self) -> &[CandidateAnomalyEvent] {
        &self.candidates
    }

    #[must_use]
    pub fn health_events(&self) -> &[RecordedHealthEvent] {
        &self.health_events
    }

    #[must_use]
    pub const fn total_deduplicated_repeats(&self) -> u32 {
        self.total_deduplicated_repeats
    }

    /// Returns the active camera-health fault at `[start_ms, end_ms]`, if any.
    #[must_use]
    pub fn active_fault_overlapping(
        &self,
        start_ms: i64,
        end_ms: i64,
    ) -> Option<CameraHealthState> {
        self.health_events
            .iter()
            .find(|ev| ev.start_ms <= end_ms && start_ms <= ev.end_ms)
            .map(RecordedHealthEvent::health_state)
    }

    /// Ingests either a camera-health/fault event or a candidate scene trigger (`CAM-03`, `CAM-04`).
    pub fn record_step(
        &mut self,
        step: ObservationStepInput,
    ) -> Result<Option<TriggerOutcome>, AnomalyError> {
        let next_obs_count = self.seen_observation_ids.len().saturating_add(1);
        if next_obs_count > self.limits.max_observations {
            return Err(AnomalyError::LimitExceeded {
                kind: AnomalyLimitKind::Observations,
                actual: next_obs_count,
                max: self.limits.max_observations,
            });
        }

        match step {
            ObservationStepInput::HealthEvent {
                observation_id,
                health_state,
                start_ms,
                end_ms,
                detail,
            } => {
                if health_state == CameraHealthState::Nominal {
                    return Err(AnomalyError::NominalHealthEventForbidden { observation_id });
                }
                if end_ms < start_ms {
                    return Err(AnomalyError::InvalidTimeInterval { start_ms, end_ms });
                }
                if let Some(prev) = self.last_observation_start_ms {
                    if start_ms < prev {
                        return Err(AnomalyError::OutOfOrderObservation {
                            previous_start_ms: prev,
                            current_start_ms: start_ms,
                        });
                    }
                }
                if self.seen_observation_ids.contains(&observation_id) {
                    return Err(AnomalyError::DuplicateObservationId(observation_id));
                }
                let next_gaps = self.health_events.len().saturating_add(1);
                if next_gaps > self.limits.max_coverage_gaps {
                    return Err(AnomalyError::LimitExceeded {
                        kind: AnomalyLimitKind::CoverageGaps,
                        actual: next_gaps,
                        max: self.limits.max_coverage_gaps,
                    });
                }
                validate_single_line_text(
                    &detail,
                    "health_event.detail",
                    &mut self.used_text_bytes,
                    self.limits,
                )?;

                if health_state == CameraHealthState::CameraRepositioned {
                    if let CameraCalibrationState::Valid {
                        calibration_version,
                        ..
                    } = &self.source.calibration
                    {
                        self.source.calibration = CameraCalibrationState::InvalidatedByMovement {
                            prior_calibration_version: calibration_version.clone(),
                            detected_at_ms: start_ms,
                        };
                    }
                }
                if health_state == CameraHealthState::SourceRestricted {
                    self.source.access_status = CameraAccessStatus::RestrictedOrUnavailable;
                }

                self.seen_observation_ids.insert(observation_id.clone());
                self.last_observation_start_ms = Some(start_ms);
                self.health_events.push(RecordedHealthEvent {
                    observation_id,
                    health_state,
                    start_ms,
                    end_ms,
                    detail,
                });
                Ok(None)
            }
            ObservationStepInput::SceneTrigger {
                observation_id,
                candidate_id,
                candidate_version,
                region_id,
                category,
                baseline_at_trigger,
                event_start_ms,
                event_end_ms,
                detection_emitted_ms,
                observed_persistence_frames,
                raw_score_bps,
                clock_uncertainty_ms,
                trigger_explanation,
                primary_frame,
                context_window,
            } => {
                if self.source.access_status == CameraAccessStatus::RestrictedOrUnavailable {
                    return Err(AnomalyError::SourceRestricted {
                        camera_id: self.source.camera_id.clone(),
                        policy_notice: RESTRICTED_SOURCE_POLICY_NOTICE,
                    });
                }
                if event_end_ms < event_start_ms {
                    return Err(AnomalyError::InvalidTimeInterval {
                        start_ms: event_start_ms,
                        end_ms: event_end_ms,
                    });
                }
                if detection_emitted_ms < event_start_ms {
                    return Err(AnomalyError::DetectionBeforeEventStart {
                        event_start_ms,
                        detection_emitted_ms,
                    });
                }
                if let Some(prev) = self.last_observation_start_ms {
                    if event_start_ms < prev {
                        return Err(AnomalyError::OutOfOrderObservation {
                            previous_start_ms: prev,
                            current_start_ms: event_start_ms,
                        });
                    }
                }
                if self.seen_observation_ids.contains(&observation_id) {
                    return Err(AnomalyError::DuplicateObservationId(observation_id));
                }
                if let Some(active_fault) =
                    self.active_fault_overlapping(event_start_ms, event_end_ms)
                {
                    return Err(AnomalyError::CameraFaultBlocksSceneTrigger {
                        camera_id: self.source.camera_id.clone(),
                        active_health: active_fault,
                    });
                }
                let Some(region) = self.regions.get(&region_id).cloned() else {
                    return Err(AnomalyError::UnknownRegionId(region_id));
                };
                if raw_score_bps > 10_000 {
                    return Err(AnomalyError::InvalidRawScoreBps(raw_score_bps));
                }
                if raw_score_bps < region.score_threshold_bps {
                    return Err(AnomalyError::BelowScoreThreshold {
                        score_bps: raw_score_bps,
                        threshold_bps: region.score_threshold_bps,
                    });
                }
                let observed_ms = event_end_ms.saturating_sub(event_start_ms) as u64;
                if observed_persistence_frames < region.min_persistence_frames
                    || observed_ms < region.min_persistence_ms
                {
                    return Err(AnomalyError::BelowPersistenceRequirement {
                        observed_frames: observed_persistence_frames,
                        required_frames: region.min_persistence_frames,
                        observed_ms,
                        required_ms: region.min_persistence_ms,
                    });
                }

                // Check if this trigger deduplicates into an existing candidate in the same region and category.
                if let Some(existing_idx) = self.find_deduplication_target(
                    &region_id,
                    category,
                    event_start_ms,
                    region.deduplication_window_ms,
                ) {
                    if self.seen_versions.contains(&candidate_version) {
                        return Err(AnomalyError::ReusedVersionId(candidate_version));
                    }
                    let mut temp_versions = self.seen_versions.clone();
                    temp_versions.insert(candidate_version.clone());
                    validate_frame_ref(&primary_frame, &mut temp_versions)?;
                    self.validate_context_window(
                        &context_window,
                        event_start_ms,
                        event_end_ms,
                        &mut temp_versions,
                    )?;
                    validate_single_line_text(
                        &trigger_explanation,
                        "scene_trigger.trigger_explanation",
                        &mut self.used_text_bytes,
                        self.limits,
                    )?;

                    self.seen_versions = temp_versions;
                    self.seen_observation_ids.insert(observation_id);
                    self.last_observation_start_ms = Some(event_start_ms);
                    self.total_deduplicated_repeats =
                        self.total_deduplicated_repeats.saturating_add(1);

                    let Some(existing) = self.candidates.get_mut(existing_idx) else {
                        return Err(AnomalyError::UnknownCandidateId(candidate_id));
                    };
                    existing.event_end_ms = existing.event_end_ms.max(event_end_ms);
                    existing.observed_persistence_frames = existing
                        .observed_persistence_frames
                        .saturating_add(observed_persistence_frames);
                    existing.peak_raw_score_bps = existing.peak_raw_score_bps.max(raw_score_bps);
                    existing.clock_uncertainty_ms =
                        existing.clock_uncertainty_ms.max(clock_uncertainty_ms);
                    existing.deduplicated_trigger_count =
                        existing.deduplicated_trigger_count.saturating_add(1);
                    return Ok(Some(TriggerOutcome::DeduplicatedIntoExisting {
                        candidate_id: existing.candidate_id.clone(),
                        merged_trigger_count: existing.deduplicated_trigger_count,
                    }));
                }

                if self.candidate_index.contains_key(&candidate_id) {
                    return Err(AnomalyError::DuplicateCandidateId(candidate_id));
                }
                let next_candidates = self.candidates.len().saturating_add(1);
                if next_candidates > self.limits.max_candidates {
                    return Err(AnomalyError::LimitExceeded {
                        kind: AnomalyLimitKind::Candidates,
                        actual: next_candidates,
                        max: self.limits.max_candidates,
                    });
                }
                if self.seen_versions.contains(&candidate_version) {
                    return Err(AnomalyError::ReusedVersionId(candidate_version));
                }

                let mut temp_versions = self.seen_versions.clone();
                temp_versions.insert(candidate_version.clone());
                validate_frame_ref(&primary_frame, &mut temp_versions)?;
                self.validate_context_window(
                    &context_window,
                    event_start_ms,
                    event_end_ms,
                    &mut temp_versions,
                )?;
                validate_single_line_text(
                    &trigger_explanation,
                    "scene_trigger.trigger_explanation",
                    &mut self.used_text_bytes,
                    self.limits,
                )?;

                self.seen_versions = temp_versions;
                self.seen_observation_ids.insert(observation_id);
                self.last_observation_start_ms = Some(event_start_ms);

                let idx = self.candidates.len();
                self.candidate_index.insert(candidate_id.clone(), idx);
                self.candidates.push(CandidateAnomalyEvent {
                    candidate_id: candidate_id.clone(),
                    initial_version: candidate_version.clone(),
                    current_version: candidate_version,
                    schema_version: SchemaVersion::V1,
                    camera_id: self.source.camera_id.clone(),
                    region_id,
                    category,
                    baseline_at_trigger,
                    event_start_ms,
                    event_end_ms,
                    first_detection_emitted_ms: detection_emitted_ms,
                    observed_persistence_frames,
                    peak_raw_score_bps: raw_score_bps,
                    clock_uncertainty_ms,
                    deduplicated_trigger_count: 1,
                    trigger_explanation,
                    detector: self.detector.clone(),
                    primary_frame,
                    context_window,
                    reviews: Vec::new(),
                });

                Ok(Some(TriggerOutcome::PublishedNewCandidate { candidate_id }))
            }
        }
    }

    fn find_deduplication_target(
        &self,
        region_id: &OpaqueId,
        category: AnomalyCategory,
        event_start_ms: i64,
        deduplication_window_ms: u64,
    ) -> Option<usize> {
        self.candidates.iter().rposition(|cand| {
            cand.region_id == *region_id
                && cand.category == category
                && event_start_ms >= cand.event_start_ms
                && (event_start_ms.saturating_sub(cand.event_end_ms) as u64)
                    <= deduplication_window_ms
        })
    }

    fn validate_context_window(
        &mut self,
        context_window: &ContextWindowStatus,
        event_start_ms: i64,
        event_end_ms: i64,
        seen_versions: &mut BTreeSet<VersionId>,
    ) -> Result<(), AnomalyError> {
        match context_window {
            ContextWindowStatus::Complete {
                pre_frames,
                post_frames,
            }
            | ContextWindowStatus::PartialBufferExpired {
                pre_frames,
                post_frames,
                ..
            } => {
                if !self.source.clip_retention_permitted
                    && (!pre_frames.is_empty() || !post_frames.is_empty())
                {
                    return Err(AnomalyError::RetentionPolicyForbidsClips {
                        camera_id: self.source.camera_id.clone(),
                    });
                }
                let total_ctx = pre_frames.len().saturating_add(post_frames.len());
                if total_ctx > self.limits.max_context_frames {
                    return Err(AnomalyError::LimitExceeded {
                        kind: AnomalyLimitKind::ContextFrames,
                        actual: total_ctx,
                        max: self.limits.max_context_frames,
                    });
                }
                let mut earliest_ms = event_start_ms;
                let mut latest_ms = event_end_ms;
                for frame in pre_frames.iter().chain(post_frames.iter()) {
                    validate_frame_ref(frame, seen_versions)?;
                    earliest_ms = earliest_ms.min(frame.capture_time_ms);
                    latest_ms = latest_ms.max(frame.capture_time_ms);
                }
                if let Some(max_window_ms) = self.source.max_retention_window_ms {
                    let span_ms = latest_ms.saturating_sub(earliest_ms) as u64;
                    if span_ms > max_window_ms {
                        return Err(AnomalyError::ContextExceedsRetentionWindow {
                            camera_id: self.source.camera_id.clone(),
                            span_ms,
                            max_window_ms,
                        });
                    }
                }
            }
            ContextWindowStatus::OmittedByRetentionPolicy { reason } => {
                validate_single_line_text(
                    reason,
                    "context_window.reason",
                    &mut self.used_text_bytes,
                    self.limits,
                )?;
            }
        }
        Ok(())
    }

    /// Appends an immutable human review entry to a candidate anomaly without overwriting
    /// earlier assessments (`CAM-05`).
    pub fn review_candidate(
        &mut self,
        input: AnomalyReviewInput,
    ) -> Result<&CandidateAnomalyEvent, AnomalyError> {
        let Some(&idx) = self.candidate_index.get(&input.candidate_id) else {
            return Err(AnomalyError::UnknownCandidateId(input.candidate_id));
        };
        if self.seen_versions.contains(&input.new_review_version) {
            return Err(AnomalyError::ReusedVersionId(input.new_review_version));
        }
        if input.cited_frame_versions.is_empty() {
            return Err(AnomalyError::EmptyCitedFrames {
                candidate_id: input.candidate_id,
            });
        }

        let current_version = self
            .candidates
            .get(idx)
            .map(|c| c.current_version.clone())
            .ok_or_else(|| AnomalyError::UnknownCandidateId(input.candidate_id.clone()))?;
        if input.expected_current_version != current_version {
            return Err(AnomalyError::StaleCandidateVersion {
                candidate_id: input.candidate_id,
                expected: input.expected_current_version,
                actual: current_version,
            });
        }

        let next_reviews = self
            .candidates
            .get(idx)
            .map_or(0, |c| c.reviews.len().saturating_add(1));
        if next_reviews > self.limits.max_reviews_per_candidate {
            return Err(AnomalyError::LimitExceeded {
                kind: AnomalyLimitKind::ReviewsPerCandidate,
                actual: next_reviews,
                max: self.limits.max_reviews_per_candidate,
            });
        }

        let known_frames = self
            .candidates
            .get(idx)
            .map(collect_candidate_frame_versions)
            .unwrap_or_default();
        for cited in &input.cited_frame_versions {
            if !known_frames.contains(cited) {
                return Err(AnomalyError::UnknownCitedFrameVersion {
                    candidate_id: input.candidate_id,
                    frame_version: cited.clone(),
                });
            }
        }

        validate_single_line_text(
            &input.rationale,
            "review.rationale",
            &mut self.used_text_bytes,
            self.limits,
        )?;

        self.seen_versions.insert(input.new_review_version.clone());
        let Some(candidate) = self.candidates.get_mut(idx) else {
            return Err(AnomalyError::UnknownCandidateId(input.candidate_id));
        };
        let entry = AnomalyReviewEntry {
            review_version: input.new_review_version.clone(),
            supersedes_version: candidate.current_version.clone(),
            reviewer: input.reviewer,
            reviewed_at_ms: input.reviewed_at_ms,
            disposition: input.disposition,
            rationale: input.rationale,
            cited_frame_versions: input.cited_frame_versions,
        };
        candidate.current_version = input.new_review_version;
        candidate.reviews.push(entry);
        Ok(candidate)
    }

    /// Computes operational performance metrics against held-out labeled scene events
    /// over `[eval_start_ms, eval_end_ms)` (`CAM-03`–`CAM-05`, `A09`, `AGENTS.md` §8).
    pub fn evaluate_held_out(
        &self,
        eval_start_ms: i64,
        eval_end_ms: i64,
        labeled_events: &[LabeledSceneEvent],
    ) -> Result<OperationalEvaluationSummary, AnomalyError> {
        if eval_end_ms <= eval_start_ms {
            return Err(AnomalyError::InvalidTimeInterval {
                start_ms: eval_start_ms,
                end_ms: eval_end_ms,
            });
        }
        if labeled_events.len() > self.limits.max_labeled_truth_events {
            return Err(AnomalyError::LimitExceeded {
                kind: AnomalyLimitKind::LabeledTruthEvents,
                actual: labeled_events.len(),
                max: self.limits.max_labeled_truth_events,
            });
        }

        let mut seen_truth_ids = BTreeSet::new();
        for truth in labeled_events {
            if truth.end_ms < truth.start_ms {
                return Err(AnomalyError::InvalidTimeInterval {
                    start_ms: truth.start_ms,
                    end_ms: truth.end_ms,
                });
            }
            if !self.regions.contains_key(&truth.region_id) {
                return Err(AnomalyError::UnknownRegionId(truth.region_id.clone()));
            }
            if !seen_truth_ids.insert(truth.truth_id.clone()) {
                return Err(AnomalyError::DuplicateTruthEventId(truth.truth_id.clone()));
            }
        }

        let total_window_ms = eval_end_ms.saturating_sub(eval_start_ms) as u64;
        let missing_coverage_ms = self.compute_missing_coverage_ms(eval_start_ms, eval_end_ms);
        let active_coverage_ms = total_window_ms.saturating_sub(missing_coverage_ms);
        if active_coverage_ms == 0 {
            return Err(AnomalyError::EmptyEvaluationActiveCoverage);
        }

        let coverage_bps =
            ((active_coverage_ms as u128 * 10_000) / (total_window_ms as u128)).min(10_000) as u16;

        let mut matched_truth_indices = BTreeSet::new();
        let mut true_positive_alerts = 0usize;
        let mut latencies_ms = Vec::new();

        for cand in &self.candidates {
            if cand.event_end_ms < eval_start_ms || cand.event_start_ms >= eval_end_ms {
                continue;
            }
            let mut matched_any = false;
            for (idx, truth) in labeled_events.iter().enumerate() {
                if cand.region_id == truth.region_id
                    && cand.category == truth.category
                    && cand.event_start_ms <= truth.end_ms
                    && truth.start_ms <= cand.event_end_ms
                {
                    matched_any = true;
                    if matched_truth_indices.insert(idx) {
                        let latency = cand
                            .first_detection_emitted_ms
                            .saturating_sub(truth.start_ms)
                            .max(0) as u64;
                        latencies_ms.push(latency);
                    }
                }
            }
            if matched_any {
                true_positive_alerts = true_positive_alerts.saturating_add(1);
            }
        }

        let window_candidates = self
            .candidates
            .iter()
            .filter(|c| c.event_end_ms >= eval_start_ms && c.event_start_ms < eval_end_ms)
            .count();
        let false_alert_count = window_candidates.saturating_sub(true_positive_alerts);

        // False alerts per active camera-hour in milli-alerts/hour:
        // (false_alert_count * 3_600_000 ms/hr * 1_000) / active_coverage_ms
        let false_alerts_per_camera_hour_milli = ((false_alert_count as u128)
            .saturating_mul(3_600_000_000u128)
            / (active_coverage_ms as u128)) as u64;

        let matched_labeled_event_count = matched_truth_indices.len();
        let event_recall_bps = if labeled_events.is_empty() {
            None
        } else {
            Some(
                ((matched_labeled_event_count as u128 * 10_000) / (labeled_events.len() as u128))
                    .min(10_000) as u16,
            )
        };

        let (min_detection_latency_ms, mean_detection_latency_ms, max_detection_latency_ms) =
            if latencies_ms.is_empty() {
                (None, None, None)
            } else {
                let min_lat = latencies_ms.iter().copied().min();
                let max_lat = latencies_ms.iter().copied().max();
                let sum: u128 = latencies_ms.iter().map(|&v| u128::from(v)).sum();
                let mean_lat = Some((sum / (latencies_ms.len() as u128)) as u64);
                (min_lat, mean_lat, max_lat)
            };

        Ok(OperationalEvaluationSummary {
            camera_id: self.source.camera_id.clone(),
            detector_id: self.detector.detector_id.clone(),
            detector_version: self.detector.detector_version.clone(),
            total_window_ms,
            missing_coverage_ms,
            active_coverage_ms,
            coverage_bps,
            labeled_event_count: labeled_events.len(),
            matched_labeled_event_count,
            event_recall_bps,
            total_candidate_alerts: window_candidates,
            true_positive_alerts,
            false_alert_count,
            false_alerts_per_camera_hour_milli,
            min_detection_latency_ms,
            mean_detection_latency_ms,
            max_detection_latency_ms,
            deduplicated_repeat_triggers: self.total_deduplicated_repeats,
            fault_events_excluded_from_alerts: self.health_events.len(),
        })
    }

    fn compute_missing_coverage_ms(&self, eval_start_ms: i64, eval_end_ms: i64) -> u64 {
        let mut intervals: Vec<(i64, i64)> = self
            .health_events
            .iter()
            .filter_map(|ev| {
                let s = ev.start_ms.max(eval_start_ms);
                let e = ev.end_ms.min(eval_end_ms);
                (e > s).then_some((s, e))
            })
            .collect();
        if intervals.is_empty() {
            return 0;
        }
        intervals.sort_by_key(|&(s, e)| (s, e));
        let mut merged_ms = 0u64;
        let mut cur_start = intervals.first().map_or(eval_start_ms, |&(s, _)| s);
        let mut cur_end = intervals.first().map_or(eval_start_ms, |&(_, e)| e);
        for &(s, e) in intervals.iter().skip(1) {
            if s <= cur_end {
                cur_end = cur_end.max(e);
            } else {
                merged_ms = merged_ms.saturating_add(cur_end.saturating_sub(cur_start) as u64);
                cur_start = s;
                cur_end = e;
            }
        }
        merged_ms.saturating_add(cur_end.saturating_sub(cur_start) as u64)
    }
}

fn collect_candidate_frame_versions(candidate: &CandidateAnomalyEvent) -> BTreeSet<VersionId> {
    let mut set = BTreeSet::new();
    set.insert(candidate.primary_frame.frame_version.clone());
    match &candidate.context_window {
        ContextWindowStatus::Complete {
            pre_frames,
            post_frames,
        }
        | ContextWindowStatus::PartialBufferExpired {
            pre_frames,
            post_frames,
            ..
        } => {
            for f in pre_frames.iter().chain(post_frames.iter()) {
                set.insert(f.frame_version.clone());
            }
        }
        ContextWindowStatus::OmittedByRetentionPolicy { .. } => {}
    }
    set
}

/// Validates cross-camera association and multi-view triangulation readiness between
/// two candidate anomaly events (`CAM-05`, `A09`).
///
/// Requires distinct cameras, independent upstream feeds (`first_monitor.source.upstream_feed_id != second_monitor.source.upstream_feed_id`),
/// active unmodified calibration on both cameras, and temporal alignment within
/// `max_allowed_skew_ms` after accounting for both cameras' clock uncertainties.
pub fn validate_cross_camera_association(
    first_monitor: &CameraAnomalyMonitor,
    first_candidate_id: &OpaqueId,
    second_monitor: &CameraAnomalyMonitor,
    second_candidate_id: &OpaqueId,
    max_allowed_skew_ms: u64,
    correspondence_rationale: String,
    limits: AnomalyLimits,
) -> Result<CrossCameraAssociation, AnomalyError> {
    let mut used_text_bytes = 0usize;
    validate_single_line_text(
        &correspondence_rationale,
        "correspondence_rationale",
        &mut used_text_bytes,
        limits,
    )?;

    if first_monitor.source.camera_id == second_monitor.source.camera_id {
        return Err(AnomalyError::SameCameraAssociation {
            camera_id: first_monitor.source.camera_id.clone(),
        });
    }
    if first_monitor.source.upstream_feed_id == second_monitor.source.upstream_feed_id {
        return Err(AnomalyError::SharedUpstreamFeedDependency {
            upstream_feed_id: first_monitor.source.upstream_feed_id.clone(),
        });
    }

    let first_cal_version = match &first_monitor.source.calibration {
        CameraCalibrationState::Uncalibrated => {
            return Err(AnomalyError::CalibrationRequiredForTriangulation {
                camera_id: first_monitor.source.camera_id.clone(),
            });
        }
        CameraCalibrationState::InvalidatedByMovement {
            prior_calibration_version,
            detected_at_ms,
        } => {
            return Err(AnomalyError::CalibrationInvalidatedByMovement {
                camera_id: first_monitor.source.camera_id.clone(),
                prior_calibration_version: prior_calibration_version.clone(),
                detected_at_ms: *detected_at_ms,
            });
        }
        CameraCalibrationState::Valid {
            calibration_version,
            ..
        } => calibration_version.clone(),
    };

    let second_cal_version = match &second_monitor.source.calibration {
        CameraCalibrationState::Uncalibrated => {
            return Err(AnomalyError::CalibrationRequiredForTriangulation {
                camera_id: second_monitor.source.camera_id.clone(),
            });
        }
        CameraCalibrationState::InvalidatedByMovement {
            prior_calibration_version,
            detected_at_ms,
        } => {
            return Err(AnomalyError::CalibrationInvalidatedByMovement {
                camera_id: second_monitor.source.camera_id.clone(),
                prior_calibration_version: prior_calibration_version.clone(),
                detected_at_ms: *detected_at_ms,
            });
        }
        CameraCalibrationState::Valid {
            calibration_version,
            ..
        } => calibration_version.clone(),
    };

    let Some(&first_idx) = first_monitor.candidate_index.get(first_candidate_id) else {
        return Err(AnomalyError::UnknownCandidateId(first_candidate_id.clone()));
    };
    let Some(first_cand) = first_monitor.candidates.get(first_idx) else {
        return Err(AnomalyError::UnknownCandidateId(first_candidate_id.clone()));
    };

    let Some(&second_idx) = second_monitor.candidate_index.get(second_candidate_id) else {
        return Err(AnomalyError::UnknownCandidateId(
            second_candidate_id.clone(),
        ));
    };
    let Some(second_cand) = second_monitor.candidates.get(second_idx) else {
        return Err(AnomalyError::UnknownCandidateId(
            second_candidate_id.clone(),
        ));
    };

    let temporal_skew_ms = if first_cand.event_end_ms < second_cand.event_start_ms {
        second_cand
            .event_start_ms
            .saturating_sub(first_cand.event_end_ms) as u64
    } else if second_cand.event_end_ms < first_cand.event_start_ms {
        first_cand
            .event_start_ms
            .saturating_sub(second_cand.event_end_ms) as u64
    } else {
        0
    };
    let combined_clock_uncertainty_ms = first_cand
        .clock_uncertainty_ms
        .saturating_add(second_cand.clock_uncertainty_ms);
    if temporal_skew_ms.saturating_add(combined_clock_uncertainty_ms) > max_allowed_skew_ms {
        return Err(AnomalyError::TemporalSkewExceedsTolerance {
            skew_ms: temporal_skew_ms,
            combined_uncertainty_ms: combined_clock_uncertainty_ms,
            max_allowed_skew_ms,
        });
    }

    Ok(CrossCameraAssociation {
        first_candidate_id: first_cand.candidate_id.clone(),
        second_candidate_id: second_cand.candidate_id.clone(),
        first_camera_id: first_monitor.source.camera_id.clone(),
        second_camera_id: second_monitor.source.camera_id.clone(),
        first_calibration_version: first_cal_version,
        second_calibration_version: second_cal_version,
        temporal_skew_ms,
        combined_clock_uncertainty_ms,
        correspondence_rationale,
    })
}
