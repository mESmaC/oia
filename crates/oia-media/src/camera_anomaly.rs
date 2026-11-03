//! Camera anomaly candidate packaging, health classification, deduplication,
//! append-only human review history, cross-camera association eligibility,
//! and held-out operational benchmark evaluation (`CAM-01`–`CAM-05`, `A09`).
//!
//! This module implements a dependency-independent in-memory validation and
//! evaluation kernel for `#29`:
//! - Enforces `CAM-01`/`CAM-02` camera admission rules before automated anomaly
//!   processing: home interiors (`HomeInterior` and `ViewIntoHomeInterior`, or
//!   unmasked private windows), restricted sources, unpermitted or unknown
//!   automated-processing terms, and non-live substitutes (`StillFallback`,
//!   `StreetViewFallback`, `SyntheticPlaceholder`) are refused as scene-anomaly
//!   event triggers, with restricted sources recorded as unavailable rather than
//!   triggering alternate access attempts.
//! - Separates camera-health and feed-fault observations (`FrozenFrame`,
//!   `ExposureShift`, `CameraRepositioned`, `RepeatedRebroadcast`, `TimingDrift`,
//!   `CoverageGap`, `SourceRestricted`) from candidate scene events
//!   (`SkyTransient`, `UnusualApparentMotion`, `RoadObstruction`,
//!   `StoppedVehicle`, `WrongWayTravel`, `EnvironmentalChange`) so camera faults
//!   never publish as scene anomalies (`CAM-03`, `A09`). Late-arriving faults
//!   retroactively quarantine overlapping candidates and flag any existing reviews
//!   for reassessment.
//! - Packages candidate scene events with pinned original frame/clip `VersionId`s
//!   and 64-character lowercase hex SHA-256 digests, bounded pre/post context
//!   frames (or explicit buffer-expiry/retention-policy omission status),
//!   trigger explanation, detector model/version/configuration identity, and
//!   raw uncalibrated detector scores paired with [`UNCALIBRATED_SCORE_CAVEAT`]
//!   and [`CANDIDATE_EVENT_CAVEAT`] (`CAM-04`, `CAM-05`).
//! - Deduplicates overlapping or nearby triggers within a configured window by
//!   retaining every merged trigger segment (`RetainedTriggerSegment`), advancing
//!   `current_version` to the new trigger's `candidate_version`, and marking any
//!   prior human reviews `needs_reassessment = true` (`CAM-04`, `AGENTS.md` §2).
//! - Maintains append-only human review history (`ArtifactOrFault`,
//!   `LikelyOrdinaryExplanation`, `InsufficientEvidence`,
//!   `RetainedForInvestigation`) with optimistic concurrency (`expected_current_version`)
//!   and non-reusable `VersionId`s (`CAM-05`).
//! - Validates cross-camera association separately from multi-view triangulation
//!   readiness: uncalibrated independent views may be associated with
//!   `eligible_for_triangulation() == false`, whereas triangulation requires
//!   active unmodified calibration on both cameras (`CAM-05`, `A09`).
//! - Computes held-out operational benchmark metrics with half-open `[start, end)`
//!   intervals, pinned evaluation corpus identity/version, active-coverage
//!   recall (separating truth events outside the evaluation window or inside
//!   coverage gaps), window-scoped deduplication and fault counters, early
//!   detection counts, and false alerts per observed camera-hour (`CAM-03`–`CAM-05`,
//!   `A09`, `AGENTS.md` §8).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use oia_domain::{ActorId, IdError, OpaqueId, SchemaVersion, VersionId};

use crate::is_valid_lowercase_sha256_hex;

/// Mandatory caveat attached to every candidate camera anomaly event (`CAM-04`).
pub const CANDIDATE_EVENT_CAVEAT: &str = "Candidate camera anomaly for human review only; not an identification of a UAP, crime, or emergency.";

/// Mandatory caveat attached to raw detector scores (`CAM-05`, `AGENTS.md` §2).
pub const UNCALIBRATED_SCORE_CAVEAT: &str = "Raw detector score is an uncalibrated ranking signal, not a calibrated class probability.";

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
    /// Public street or outdoor view that includes residential building exteriors (`CAM-01` permits
    /// ordinary exterior street views when private windows/interiors are masked where needed).
    ResidentialExteriorStreetView,
    /// Camera placed inside a home (`CAM-01` unconditionally excludes).
    HomeInterior,
    /// Camera view directed into a private home interior (`CAM-01` unconditionally excludes).
    ViewIntoHomeInterior,
}

/// Explicit permission status for source data-use conditions (`CAM-02`, `AGENTS.md` §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceUsePermission {
    Permitted,
    Prohibited,
    Unknown,
}

impl fmt::Display for SourceUsePermission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Permitted => f.write_str("permitted"),
            Self::Prohibited => f.write_str("prohibited"),
            Self::Unknown => f.write_str("unknown"),
        }
    }
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
    pub automated_processing: SourceUsePermission,
    pub clip_retention: SourceUsePermission,
    pub redistribution: SourceUsePermission,
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
    /// Whether the region includes private windows or areas that require an active privacy mask (`CAM-02`).
    pub requires_privacy_mask: bool,
    /// Whether a privacy mask excluding private windows/areas is applied (`CAM-02`).
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
    /// Both pre-event and post-event context frames were retained within policy bounds (at least one frame).
    Complete {
        pre_frames: Vec<RetainedFrameRef>,
        post_frames: Vec<RetainedFrameRef>,
    },
    /// Some context frames expired from the bounded ring buffer before capture completed (`expired_frame_count >= 1`).
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

/// Input payload for a candidate scene detection emitted by a scoped detector (`CAM-03`, `CAM-04`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneTriggerInput {
    pub observation_id: OpaqueId,
    pub candidate_id: OpaqueId,
    pub candidate_version: VersionId,
    pub region_id: OpaqueId,
    pub category: AnomalyCategory,
    pub baseline_at_trigger: SceneBaselineKind,
    /// Half-open `[event_start_ms, event_end_ms)` scene event interval.
    pub event_start_ms: i64,
    pub event_end_ms: i64,
    pub detection_emitted_ms: i64,
    pub observed_persistence_frames: u32,
    pub raw_score_bps: u16,
    pub clock_uncertainty_ms: u64,
    pub trigger_explanation: String,
    pub primary_frame: RetainedFrameRef,
    pub context_window: ContextWindowStatus,
}

/// Input payload for a camera-health fault or coverage interruption (`CAM-03`, `A09`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthEventInput {
    pub observation_id: OpaqueId,
    pub health_state: CameraHealthState,
    /// Half-open `[start_ms, end_ms)` fault interval (`end_ms > start_ms`).
    pub start_ms: i64,
    pub end_ms: i64,
    pub detail: String,
}

/// Observation step submitted to the camera anomaly monitor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservationStepInput {
    /// A candidate scene detection emitted by a scoped detector on an upstream frame.
    SceneTrigger(Box<SceneTriggerInput>),
    /// A camera-health fault or coverage interruption observed on the feed (`CAM-03`, `A09`).
    HealthEvent(HealthEventInput),
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

/// Retained trigger segment preserved when triggers are recorded or deduplicated (`CAM-04`, `AGENTS.md` §2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetainedTriggerSegment {
    observation_id: OpaqueId,
    segment_version: VersionId,
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
}

impl RetainedTriggerSegment {
    #[must_use]
    pub fn observation_id(&self) -> &OpaqueId {
        &self.observation_id
    }

    #[must_use]
    pub fn segment_version(&self) -> &VersionId {
        &self.segment_version
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
    pub const fn detection_emitted_ms(&self) -> i64 {
        self.detection_emitted_ms
    }

    #[must_use]
    pub const fn observed_persistence_frames(&self) -> u32 {
        self.observed_persistence_frames
    }

    #[must_use]
    pub const fn raw_score_bps(&self) -> u16 {
        self.raw_score_bps
    }

    #[must_use]
    pub const fn clock_uncertainty_ms(&self) -> u64 {
        self.clock_uncertainty_ms
    }

    #[must_use]
    pub fn trigger_explanation(&self) -> &str {
        &self.trigger_explanation
    }

    #[must_use]
    pub fn primary_frame(&self) -> &RetainedFrameRef {
        &self.primary_frame
    }

    #[must_use]
    pub fn context_window(&self) -> &ContextWindowStatus {
        &self.context_window
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
    needs_reassessment: bool,
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

    /// True if a subsequent deduplicated trigger merge or late-arriving camera fault
    /// altered the candidate after this review was recorded (`AGENTS.md` §2).
    #[must_use]
    pub const fn needs_reassessment(&self) -> bool {
        self.needs_reassessment
    }
}

/// Validated candidate camera anomaly event with retained trigger segments and review history (`CAM-04`, `CAM-05`).
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
    trigger_segments: Vec<RetainedTriggerSegment>,
    reviews: Vec<AnomalyReviewEntry>,
    quarantined_by_fault: Option<CameraHealthState>,
    needs_reassessment: bool,
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

    /// Returns all retained trigger segments (initial trigger plus any deduplicated follow-up segments).
    #[must_use]
    pub fn trigger_segments(&self) -> &[RetainedTriggerSegment] {
        &self.trigger_segments
    }

    #[must_use]
    pub fn reviews(&self) -> &[AnomalyReviewEntry] {
        &self.reviews
    }

    #[must_use]
    pub fn current_disposition(&self) -> Option<AnomalyReviewDisposition> {
        self.reviews.last().map(AnomalyReviewEntry::disposition)
    }

    /// If a late-arriving camera fault overlapped this candidate's uncertainty-widened interval,
    /// returns the fault that quarantined it (`CAM-03`, `A09`).
    #[must_use]
    pub const fn quarantined_by_fault(&self) -> Option<CameraHealthState> {
        self.quarantined_by_fault
    }

    /// True if a deduplicated trigger merge or late-arriving camera fault occurred after a human
    /// review was recorded, requiring reassessment (`AGENTS.md` §2).
    #[must_use]
    pub const fn needs_reassessment(&self) -> bool {
        self.needs_reassessment
    }
}

/// Outcome of processing a scene trigger step (`CAM-04`, `A09`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerOutcome {
    /// Emitted a new candidate notification with its assigned `candidate_id` and `candidate_version`.
    PublishedNewCandidate {
        candidate_id: OpaqueId,
        candidate_version: VersionId,
    },
    /// Merged into an existing active candidate within the region's deduplication window,
    /// advancing the candidate's `current_version` and retaining all segment frames/metadata.
    DeduplicatedIntoExisting {
        candidate_id: OpaqueId,
        new_candidate_version: VersionId,
        merged_trigger_count: u32,
        flagged_reviews_for_reassessment: bool,
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
    /// Half-open `[start_ms, end_ms)` ground-truth interval (`end_ms > start_ms`).
    pub start_ms: i64,
    pub end_ms: i64,
}

/// Snapshot of a region's configured threshold and persistence parameters included in
/// an operational benchmark summary (`CAM-03`, `A09`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionBenchmarkConfig {
    pub region_id: OpaqueId,
    pub baseline: SceneBaselineKind,
    pub score_threshold_bps: u16,
    pub min_persistence_frames: u32,
    pub min_persistence_ms: u64,
    pub deduplication_window_ms: u64,
}

/// Operational evaluation metrics on held-out camera data (`CAM-03`–`CAM-05`, `A09`, `AGENTS.md` §8).
///
/// Reports operational false alerts per observed camera-hour, labeled-event recall on
/// active coverage, detection latency, and coverage loss rather than substituting raw
/// frame-level classifier accuracy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationalEvaluationSummary {
    pub camera_id: OpaqueId,
    pub detector_id: OpaqueId,
    pub detector_version: VersionId,
    pub detector_config_digest_sha256: String,
    pub evaluation_corpus_id: OpaqueId,
    pub evaluation_corpus_version: VersionId,
    pub eval_start_ms: i64,
    pub eval_end_ms: i64,
    pub region_configs: Vec<RegionBenchmarkConfig>,
    pub total_window_ms: u64,
    pub missing_coverage_ms: u64,
    pub active_coverage_ms: u64,
    pub coverage_bps: u16,
    /// Total ground-truth events supplied by the caller.
    pub total_supplied_truth_events: usize,
    /// Ground-truth events completely outside `[eval_start_ms, eval_end_ms)`.
    pub truth_events_outside_window_count: usize,
    /// Ground-truth events inside `[eval_start_ms, eval_end_ms)` that overlapped a coverage gap or fault.
    pub truth_events_in_coverage_gap_count: usize,
    /// Observable ground-truth events within active coverage.
    pub observable_labeled_event_count: usize,
    pub matched_labeled_event_count: usize,
    /// Recall in basis points (`0..=10_000`) over `observable_labeled_event_count`, or `None` when `0`.
    pub event_recall_bps: Option<u16>,
    pub total_candidate_alerts: usize,
    pub quarantined_fault_candidates_in_window: usize,
    pub true_positive_alerts: usize,
    pub false_alert_count: usize,
    /// Operational false alerts per active observed camera-hour scaled by `1_000` (milli-alerts/hour).
    pub false_alerts_per_camera_hour_milli: u64,
    pub early_detection_count: usize,
    pub min_detection_latency_ms: Option<u64>,
    pub mean_detection_latency_ms: Option<u64>,
    pub max_detection_latency_ms: Option<u64>,
    pub deduplicated_repeat_triggers_in_window: u32,
    pub health_events_overlapping_window: usize,
}

/// Readiness of an associated cross-camera pair for multi-view geometric triangulation (`CAM-05`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriangulationReadiness {
    /// Both cameras have active, unmodified calibration versions.
    Eligible {
        first_calibration_version: VersionId,
        second_calibration_version: VersionId,
    },
    /// The cameras are associated temporally and by independent source lineage, but at least
    /// one camera is uncalibrated so geometric triangulation is unavailable (`CAM-05`).
    UncalibratedAssociationOnly { uncalibrated_camera_ids: Vec<OpaqueId> },
}

/// Validated cross-camera association and multi-view triangulation readiness (`CAM-05`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrossCameraAssociation {
    first_candidate_id: OpaqueId,
    second_candidate_id: OpaqueId,
    first_camera_id: OpaqueId,
    second_camera_id: OpaqueId,
    triangulation_readiness: TriangulationReadiness,
    temporal_skew_ms: u64,
    combined_clock_uncertainty_ms: u64,
    correspondence_rationale: String,
}

impl CrossCameraAssociation {
    #[must_use]
    pub fn first_candidate_id(&self) -> &OpaqueId {
        &self.first_candidate_id
    }

    #[must_use]
    pub fn second_candidate_id(&self) -> &OpaqueId {
        &self.second_candidate_id
    }

    #[must_use]
    pub fn first_camera_id(&self) -> &OpaqueId {
        &self.first_camera_id
    }

    #[must_use]
    pub fn second_camera_id(&self) -> &OpaqueId {
        &self.second_camera_id
    }

    #[must_use]
    pub fn triangulation_readiness(&self) -> &TriangulationReadiness {
        &self.triangulation_readiness
    }

    #[must_use]
    pub const fn eligible_for_triangulation(&self) -> bool {
        matches!(
            self.triangulation_readiness,
            TriangulationReadiness::Eligible { .. }
        )
    }

    #[must_use]
    pub const fn temporal_skew_ms(&self) -> u64 {
        self.temporal_skew_ms
    }

    #[must_use]
    pub const fn combined_clock_uncertainty_ms(&self) -> u64 {
        self.combined_clock_uncertainty_ms
    }

    #[must_use]
    pub fn correspondence_rationale(&self) -> &str {
        &self.correspondence_rationale
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
        view_kind: CameraViewKind,
    },
    SourceRestricted {
        camera_id: OpaqueId,
        policy_notice: &'static str,
    },
    AutomatedProcessingNotPermitted {
        camera_id: OpaqueId,
        status: SourceUsePermission,
    },
    PrivacyMaskRequiredForRegion {
        camera_id: OpaqueId,
        region_id: OpaqueId,
    },
    NonObservationFeedSubstitute {
        camera_id: OpaqueId,
        feed_kind: FeedObservationKind,
    },
    RetentionPolicyForbidsClips {
        camera_id: OpaqueId,
        status: SourceUsePermission,
    },
    EmptyCompleteContextFrames,
    ZeroExpiredFrameCountInPartialContext,
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
    ReviewTimestampBeforeCandidateStart {
        candidate_id: OpaqueId,
        event_start_ms: i64,
        reviewed_at_ms: i64,
    },
    ReviewTimestampRegression {
        candidate_id: OpaqueId,
        previous_reviewed_at_ms: i64,
        reviewed_at_ms: i64,
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
            Self::HomeInteriorExcluded {
                camera_id,
                view_kind,
            } => write!(
                f,
                "camera `{camera_id}` has excluded view `{view_kind:?}` under CAM-01/CAM-02"
            ),
            Self::SourceRestricted {
                camera_id,
                policy_notice,
            } => write!(f, "camera `{camera_id}` restricted: {policy_notice}"),
            Self::AutomatedProcessingNotPermitted { camera_id, status } => write!(
                f,
                "camera `{camera_id}` automated processing permission is `{status}`, must be `permitted`"
            ),
            Self::PrivacyMaskRequiredForRegion {
                camera_id,
                region_id,
            } => write!(
                f,
                "camera `{camera_id}` region `{region_id}` requires an active privacy mask before monitoring"
            ),
            Self::NonObservationFeedSubstitute {
                camera_id,
                feed_kind,
            } => write!(
                f,
                "camera `{camera_id}` supplies `{feed_kind}`, which cannot serve as scene-anomaly observation evidence"
            ),
            Self::RetentionPolicyForbidsClips { camera_id, status } => write!(
                f,
                "camera `{camera_id}` clip retention status is `{status}`; pre/post context frames cannot be retained"
            ),
            Self::EmptyCompleteContextFrames => write!(
                f,
                "ContextWindowStatus::Complete requires at least one pre_frame or post_frame"
            ),
            Self::ZeroExpiredFrameCountInPartialContext => write!(
                f,
                "ContextWindowStatus::PartialBufferExpired requires expired_frame_count >= 1"
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
            Self::ZeroPersistenceFrames { region_id } => {
                write!(f, "region `{region_id}` min_persistence_frames must be >= 1")
            }
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
                "invalid half-open time interval [{start_ms}, {end_ms}): end_ms must be > start_ms"
            ),
            Self::DetectionBeforeEventStart {
                event_start_ms,
                detection_emitted_ms,
            } => write!(
                f,
                "detection timestamp {detection_emitted_ms} precedes event start {event_start_ms}"
            ),
            Self::ReviewTimestampBeforeCandidateStart {
                candidate_id,
                event_start_ms,
                reviewed_at_ms,
            } => write!(
                f,
                "review for candidate `{candidate_id}` at {reviewed_at_ms} precedes candidate start {event_start_ms}"
            ),
            Self::ReviewTimestampRegression {
                candidate_id,
                previous_reviewed_at_ms,
                reviewed_at_ms,
            } => write!(
                f,
                "review for candidate `{candidate_id}` at {reviewed_at_ms} precedes prior review at {previous_reviewed_at_ms}"
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
                "temporal skew {skew_ms} ms + combined clock uncertainty {combined_uncertainty_ms} ms exceeds tolerance {max_allowed_skew_ms} ms"
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

fn check_single_line_text(
    value: &str,
    field: &'static str,
    staged_text_bytes: &mut usize,
    limits: AnomalyLimits,
) -> Result<(), AnomalyError> {
    if value.trim().is_empty() {
        return Err(AnomalyError::EmptyField { field });
    }
    if value.chars().any(is_disallowed_single_line_char) {
        return Err(AnomalyError::ControlCharacterInField { field });
    }
    let next = staged_text_bytes.saturating_add(value.len());
    if next > limits.max_text_bytes {
        return Err(AnomalyError::LimitExceeded {
            kind: AnomalyLimitKind::TextBytes,
            actual: next,
            max: limits.max_text_bytes,
        });
    }
    *staged_text_bytes = next;
    Ok(())
}

fn validate_sha256_hex(digest: &str, field: &'static str) -> Result<(), AnomalyError> {
    if !is_valid_lowercase_sha256_hex(digest) {
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

/// Checks whether two half-open intervals `[a_start, a_end)` and `[b_start, b_end)` overlap.
const fn half_open_overlaps(a_start: i64, a_end: i64, b_start: i64, b_end: i64) -> bool {
    a_start < b_end && b_start < a_end
}

/// Widens a half-open interval `[start_ms, end_ms)` by `uncertainty_ms` on both sides.
const fn widen_interval(start_ms: i64, end_ms: i64, uncertainty_ms: u64) -> (i64, i64) {
    let delta = if uncertainty_ms > i64::MAX as u64 {
        i64::MAX
    } else {
        uncertainty_ms as i64
    };
    (start_ms.saturating_sub(delta), end_ms.saturating_add(delta))
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
    restricted_since_ms: Option<i64>,
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
        if matches!(
            source.view_kind,
            CameraViewKind::HomeInterior | CameraViewKind::ViewIntoHomeInterior
        ) {
            return Err(AnomalyError::HomeInteriorExcluded {
                camera_id: source.camera_id,
                view_kind: source.view_kind,
            });
        }
        if source.access_status == CameraAccessStatus::RestrictedOrUnavailable {
            return Err(AnomalyError::SourceRestricted {
                camera_id: source.camera_id,
                policy_notice: RESTRICTED_SOURCE_POLICY_NOTICE,
            });
        }
        if source.automated_processing != SourceUsePermission::Permitted {
            return Err(AnomalyError::AutomatedProcessingNotPermitted {
                camera_id: source.camera_id,
                status: source.automated_processing,
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
        check_single_line_text(
            &source.provider_attribution,
            "source.provider_attribution",
            &mut used_text_bytes,
            limits,
        )?;
        check_single_line_text(
            &source.listing_reference,
            "source.listing_reference",
            &mut used_text_bytes,
            limits,
        )?;
        check_single_line_text(
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
            check_single_line_text(
                &region.label,
                "region.label",
                &mut used_text_bytes,
                limits,
            )?;
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
            if (region.requires_privacy_mask
                || source.view_kind == CameraViewKind::ResidentialExteriorStreetView)
                && !region.privacy_mask_applied
            {
                return Err(AnomalyError::PrivacyMaskRequiredForRegion {
                    camera_id: source.camera_id,
                    region_id: region.region_id,
                });
            }
            let region_id = region.region_id.clone();
            if region_map.insert(region_id.clone(), region).is_some() {
                return Err(AnomalyError::DuplicateRegionId(region_id));
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
            restricted_since_ms: None,
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

    /// Returns the active camera-health fault overlapping the uncertainty-widened half-open
    /// interval `[start_ms - clock_uncertainty_ms, end_ms + clock_uncertainty_ms)`, if any.
    #[must_use]
    pub fn active_fault_overlapping(
        &self,
        start_ms: i64,
        end_ms: i64,
        clock_uncertainty_ms: u64,
    ) -> Option<CameraHealthState> {
        let (widened_start, widened_end) = widen_interval(start_ms, end_ms, clock_uncertainty_ms);
        self.health_events
            .iter()
            .find(|ev| half_open_overlaps(widened_start, widened_end, ev.start_ms, ev.end_ms))
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
            ObservationStepInput::HealthEvent(HealthEventInput {
                observation_id,
                health_state,
                start_ms,
                end_ms,
                detail,
            }) => {
                if health_state == CameraHealthState::Nominal {
                    return Err(AnomalyError::NominalHealthEventForbidden { observation_id });
                }
                if end_ms <= start_ms {
                    return Err(AnomalyError::InvalidTimeInterval { start_ms, end_ms });
                }
                if let Some(prev) = self.last_observation_start_ms
                    && start_ms < prev
                {
                    return Err(AnomalyError::OutOfOrderObservation {
                        previous_start_ms: prev,
                        current_start_ms: start_ms,
                    });
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
                let mut staged_text_bytes = self.used_text_bytes;
                check_single_line_text(
                    &detail,
                    "health_event.detail",
                    &mut staged_text_bytes,
                    self.limits,
                )?;

                if health_state == CameraHealthState::CameraRepositioned
                    && let CameraCalibrationState::Valid {
                        calibration_version,
                        ..
                    } = &self.source.calibration
                {
                    self.source.calibration = CameraCalibrationState::InvalidatedByMovement {
                        prior_calibration_version: calibration_version.clone(),
                        detected_at_ms: start_ms,
                    };
                }
                if health_state == CameraHealthState::SourceRestricted {
                    self.source.access_status = CameraAccessStatus::RestrictedOrUnavailable;
                    self.restricted_since_ms = Some(
                        self.restricted_since_ms
                            .map_or(start_ms, |prev| prev.min(start_ms)),
                    );
                }

                // Retroactively quarantine any existing candidate whose uncertainty-widened interval
                // overlaps this late-arriving fault, and flag its reviews for reassessment.
                for cand in &mut self.candidates {
                    let (w_start, w_end) = widen_interval(
                        cand.event_start_ms,
                        cand.event_end_ms,
                        cand.clock_uncertainty_ms,
                    );
                    if half_open_overlaps(w_start, w_end, start_ms, end_ms) {
                        if cand.quarantined_by_fault.is_none() {
                            cand.quarantined_by_fault = Some(health_state);
                        }
                        if !cand.reviews.is_empty() {
                            cand.needs_reassessment = true;
                            for rev in &mut cand.reviews {
                                rev.needs_reassessment = true;
                            }
                        }
                    }
                }

                self.used_text_bytes = staged_text_bytes;
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
            ObservationStepInput::SceneTrigger(trigger) => {
                let SceneTriggerInput {
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
                } = *trigger;

                if self.source.access_status == CameraAccessStatus::RestrictedOrUnavailable {
                    return Err(AnomalyError::SourceRestricted {
                        camera_id: self.source.camera_id.clone(),
                        policy_notice: RESTRICTED_SOURCE_POLICY_NOTICE,
                    });
                }
                if event_end_ms <= event_start_ms {
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
                if let Some(prev) = self.last_observation_start_ms
                    && event_start_ms < prev
                {
                    return Err(AnomalyError::OutOfOrderObservation {
                        previous_start_ms: prev,
                        current_start_ms: event_start_ms,
                    });
                }
                if self.seen_observation_ids.contains(&observation_id) {
                    return Err(AnomalyError::DuplicateObservationId(observation_id));
                }
                if let Some(active_fault) = self.active_fault_overlapping(
                    event_start_ms,
                    event_end_ms,
                    clock_uncertainty_ms,
                ) {
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
                    let mut staged_text_bytes = self.used_text_bytes;

                    validate_frame_ref(&primary_frame, &mut temp_versions)?;
                    self.validate_context_window(
                        &context_window,
                        event_start_ms,
                        event_end_ms,
                        &mut temp_versions,
                        &mut staged_text_bytes,
                    )?;
                    check_single_line_text(
                        &trigger_explanation,
                        "scene_trigger.trigger_explanation",
                        &mut staged_text_bytes,
                        self.limits,
                    )?;

                    self.seen_versions = temp_versions;
                    self.used_text_bytes = staged_text_bytes;
                    self.seen_observation_ids.insert(observation_id.clone());
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
                    existing.current_version = candidate_version.clone();

                    let flagged_reviews = !existing.reviews.is_empty();
                    if flagged_reviews {
                        existing.needs_reassessment = true;
                        for rev in &mut existing.reviews {
                            rev.needs_reassessment = true;
                        }
                    }

                    existing.trigger_segments.push(RetainedTriggerSegment {
                        observation_id,
                        segment_version: candidate_version.clone(),
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
                    });

                    return Ok(Some(TriggerOutcome::DeduplicatedIntoExisting {
                        candidate_id: existing.candidate_id.clone(),
                        new_candidate_version: candidate_version,
                        merged_trigger_count: existing.deduplicated_trigger_count,
                        flagged_reviews_for_reassessment: flagged_reviews,
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
                let mut staged_text_bytes = self.used_text_bytes;

                validate_frame_ref(&primary_frame, &mut temp_versions)?;
                self.validate_context_window(
                    &context_window,
                    event_start_ms,
                    event_end_ms,
                    &mut temp_versions,
                    &mut staged_text_bytes,
                )?;
                check_single_line_text(
                    &trigger_explanation,
                    "scene_trigger.trigger_explanation",
                    &mut staged_text_bytes,
                    self.limits,
                )?;

                self.seen_versions = temp_versions;
                self.used_text_bytes = staged_text_bytes;
                self.seen_observation_ids.insert(observation_id.clone());
                self.last_observation_start_ms = Some(event_start_ms);

                let initial_segment = RetainedTriggerSegment {
                    observation_id,
                    segment_version: candidate_version.clone(),
                    baseline_at_trigger,
                    event_start_ms,
                    event_end_ms,
                    detection_emitted_ms,
                    observed_persistence_frames,
                    raw_score_bps,
                    clock_uncertainty_ms,
                    trigger_explanation: trigger_explanation.clone(),
                    primary_frame: primary_frame.clone(),
                    context_window: context_window.clone(),
                };

                let idx = self.candidates.len();
                self.candidate_index.insert(candidate_id.clone(), idx);
                self.candidates.push(CandidateAnomalyEvent {
                    candidate_id: candidate_id.clone(),
                    initial_version: candidate_version.clone(),
                    current_version: candidate_version.clone(),
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
                    trigger_segments: vec![initial_segment],
                    reviews: Vec::new(),
                    quarantined_by_fault: None,
                    needs_reassessment: false,
                });

                Ok(Some(TriggerOutcome::PublishedNewCandidate {
                    candidate_id,
                    candidate_version,
                }))
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
            if cand.quarantined_by_fault.is_some()
                || cand.region_id != *region_id
                || cand.category != category
                || event_start_ms < cand.event_start_ms
            {
                return false;
            }
            let gap_ms = if event_start_ms <= cand.event_end_ms {
                0
            } else {
                event_start_ms.saturating_sub(cand.event_end_ms) as u64
            };
            gap_ms <= deduplication_window_ms
        })
    }

    fn validate_context_window(
        &self,
        context_window: &ContextWindowStatus,
        event_start_ms: i64,
        event_end_ms: i64,
        seen_versions: &mut BTreeSet<VersionId>,
        staged_text_bytes: &mut usize,
    ) -> Result<(), AnomalyError> {
        match context_window {
            ContextWindowStatus::Complete {
                pre_frames,
                post_frames,
            } => {
                if pre_frames.is_empty() && post_frames.is_empty() {
                    return Err(AnomalyError::EmptyCompleteContextFrames);
                }
                self.validate_retained_context_frames(
                    pre_frames,
                    post_frames,
                    event_start_ms,
                    event_end_ms,
                    seen_versions,
                )?;
            }
            ContextWindowStatus::PartialBufferExpired {
                pre_frames,
                post_frames,
                expired_frame_count,
            } => {
                if *expired_frame_count == 0 {
                    return Err(AnomalyError::ZeroExpiredFrameCountInPartialContext);
                }
                if !pre_frames.is_empty() || !post_frames.is_empty() {
                    self.validate_retained_context_frames(
                        pre_frames,
                        post_frames,
                        event_start_ms,
                        event_end_ms,
                        seen_versions,
                    )?;
                }
            }
            ContextWindowStatus::OmittedByRetentionPolicy { reason } => {
                check_single_line_text(
                    reason,
                    "context_window.reason",
                    staged_text_bytes,
                    self.limits,
                )?;
            }
        }
        Ok(())
    }

    fn validate_retained_context_frames(
        &self,
        pre_frames: &[RetainedFrameRef],
        post_frames: &[RetainedFrameRef],
        event_start_ms: i64,
        event_end_ms: i64,
        seen_versions: &mut BTreeSet<VersionId>,
    ) -> Result<(), AnomalyError> {
        if self.source.clip_retention != SourceUsePermission::Permitted {
            return Err(AnomalyError::RetentionPolicyForbidsClips {
                camera_id: self.source.camera_id.clone(),
                status: self.source.clip_retention,
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

        let Some(existing_cand) = self.candidates.get(idx) else {
            return Err(AnomalyError::UnknownCandidateId(input.candidate_id));
        };
        if input.expected_current_version != existing_cand.current_version {
            return Err(AnomalyError::StaleCandidateVersion {
                candidate_id: input.candidate_id,
                expected: input.expected_current_version,
                actual: existing_cand.current_version.clone(),
            });
        }
        if input.reviewed_at_ms < existing_cand.event_start_ms {
            return Err(AnomalyError::ReviewTimestampBeforeCandidateStart {
                candidate_id: input.candidate_id,
                event_start_ms: existing_cand.event_start_ms,
                reviewed_at_ms: input.reviewed_at_ms,
            });
        }
        if let Some(last_review) = existing_cand.reviews.last()
            && input.reviewed_at_ms < last_review.reviewed_at_ms
        {
            return Err(AnomalyError::ReviewTimestampRegression {
                candidate_id: input.candidate_id,
                previous_reviewed_at_ms: last_review.reviewed_at_ms,
                reviewed_at_ms: input.reviewed_at_ms,
            });
        }

        let next_reviews = existing_cand.reviews.len().saturating_add(1);
        if next_reviews > self.limits.max_reviews_per_candidate {
            return Err(AnomalyError::LimitExceeded {
                kind: AnomalyLimitKind::ReviewsPerCandidate,
                actual: next_reviews,
                max: self.limits.max_reviews_per_candidate,
            });
        }

        let known_frames = collect_candidate_frame_versions(existing_cand);
        for cited in &input.cited_frame_versions {
            if !known_frames.contains(cited) {
                return Err(AnomalyError::UnknownCitedFrameVersion {
                    candidate_id: input.candidate_id,
                    frame_version: cited.clone(),
                });
            }
        }

        let mut staged_text_bytes = self.used_text_bytes;
        check_single_line_text(
            &input.rationale,
            "review.rationale",
            &mut staged_text_bytes,
            self.limits,
        )?;

        self.used_text_bytes = staged_text_bytes;
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
            needs_reassessment: false,
        };
        candidate.current_version = input.new_review_version;
        candidate.needs_reassessment = false;
        candidate.reviews.push(entry);
        Ok(candidate)
    }

    /// Computes operational performance metrics against held-out labeled scene events
    /// over the half-open window `[eval_start_ms, eval_end_ms)` (`CAM-03`–`CAM-05`, `A09`, `AGENTS.md` §8).
    pub fn evaluate_held_out(
        &self,
        evaluation_corpus_id: OpaqueId,
        evaluation_corpus_version: VersionId,
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
            if truth.end_ms <= truth.start_ms {
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

        let merged_fault_intervals =
            self.compute_merged_missing_intervals(eval_start_ms, eval_end_ms);
        let missing_coverage_ms: u64 = merged_fault_intervals
            .iter()
            .map(|&(s, e)| e.saturating_sub(s) as u64)
            .sum();
        let total_window_ms = eval_end_ms.saturating_sub(eval_start_ms) as u64;
        let active_coverage_ms = total_window_ms.saturating_sub(missing_coverage_ms);
        if active_coverage_ms == 0 {
            return Err(AnomalyError::EmptyEvaluationActiveCoverage);
        }

        let coverage_bps = ((active_coverage_ms as u128 * 10_000) / (total_window_ms as u128))
            .min(10_000) as u16;

        // Classify ground-truth events:
        // 1. Outside `[eval_start_ms, eval_end_ms)`
        // 2. Inside window, but overlapping a coverage gap / fault interval
        // 3. Observable within active coverage
        let mut truth_events_outside_window_count = 0usize;
        let mut truth_events_in_coverage_gap_count = 0usize;
        let mut observable_truth_indices = Vec::new();

        for (idx, truth) in labeled_events.iter().enumerate() {
            if !half_open_overlaps(truth.start_ms, truth.end_ms, eval_start_ms, eval_end_ms) {
                truth_events_outside_window_count =
                    truth_events_outside_window_count.saturating_add(1);
                continue;
            }
            let clip_start = truth.start_ms.max(eval_start_ms);
            let clip_end = truth.end_ms.min(eval_end_ms);
            let overlaps_gap = merged_fault_intervals
                .iter()
                .any(|&(gs, ge)| half_open_overlaps(clip_start, clip_end, gs, ge));
            if overlaps_gap {
                truth_events_in_coverage_gap_count =
                    truth_events_in_coverage_gap_count.saturating_add(1);
            } else {
                observable_truth_indices.push(idx);
            }
        }

        let mut matched_observable_truths = BTreeSet::new();
        let mut true_positive_alerts = 0usize;
        let mut window_candidates = 0usize;
        let mut quarantined_fault_candidates_in_window = 0usize;
        let mut deduplicated_repeat_triggers_in_window = 0u32;
        let mut early_detection_count = 0usize;
        let mut latencies_ms = Vec::new();

        for cand in &self.candidates {
            if !half_open_overlaps(
                cand.event_start_ms,
                cand.event_end_ms,
                eval_start_ms,
                eval_end_ms,
            ) {
                continue;
            }
            if cand.quarantined_by_fault.is_some() {
                quarantined_fault_candidates_in_window =
                    quarantined_fault_candidates_in_window.saturating_add(1);
                continue;
            }
            window_candidates = window_candidates.saturating_add(1);

            // Count repeat trigger segments that fell within `[eval_start_ms, eval_end_ms)`.
            let repeats_in_win = cand
                .trigger_segments
                .iter()
                .skip(1)
                .filter(|seg| {
                    half_open_overlaps(
                        seg.event_start_ms,
                        seg.event_end_ms,
                        eval_start_ms,
                        eval_end_ms,
                    )
                })
                .count() as u32;
            deduplicated_repeat_triggers_in_window =
                deduplicated_repeat_triggers_in_window.saturating_add(repeats_in_win);

            let (cand_start, cand_end) = widen_interval(
                cand.event_start_ms,
                cand.event_end_ms,
                cand.clock_uncertainty_ms,
            );
            let mut matched_any = false;
            for &truth_idx in &observable_truth_indices {
                let Some(truth) = labeled_events.get(truth_idx) else {
                    continue;
                };
                if cand.region_id == truth.region_id
                    && cand.category == truth.category
                    && half_open_overlaps(cand_start, cand_end, truth.start_ms, truth.end_ms)
                {
                    matched_any = true;
                    if matched_observable_truths.insert(truth_idx) {
                        if cand.first_detection_emitted_ms < truth.start_ms {
                            early_detection_count = early_detection_count.saturating_add(1);
                        } else {
                            let latency =
                                (cand.first_detection_emitted_ms - truth.start_ms) as u64;
                            latencies_ms.push(latency);
                        }
                    }
                }
            }
            if matched_any {
                true_positive_alerts = true_positive_alerts.saturating_add(1);
            }
        }

        let false_alert_count = window_candidates.saturating_sub(true_positive_alerts);

        // False alerts per active camera-hour in milli-alerts/hour:
        // (false_alert_count * 3_600_000 ms/hr * 1_000) / active_coverage_ms
        let false_alerts_per_camera_hour_milli = ((false_alert_count as u128)
            .saturating_mul(3_600_000_000u128)
            / (active_coverage_ms as u128)) as u64;

        let observable_labeled_event_count = observable_truth_indices.len();
        let matched_labeled_event_count = matched_observable_truths.len();
        let event_recall_bps = if observable_labeled_event_count == 0 {
            None
        } else {
            Some(
                ((matched_labeled_event_count as u128 * 10_000)
                    / (observable_labeled_event_count as u128))
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

        let health_events_overlapping_window = self
            .health_events
            .iter()
            .filter(|ev| {
                let effective_end = if ev.health_state == CameraHealthState::SourceRestricted {
                    ev.end_ms.max(eval_end_ms)
                } else {
                    ev.end_ms
                };
                half_open_overlaps(ev.start_ms, effective_end, eval_start_ms, eval_end_ms)
            })
            .count();

        let region_configs = self
            .regions
            .values()
            .map(|r| RegionBenchmarkConfig {
                region_id: r.region_id.clone(),
                baseline: r.baseline,
                score_threshold_bps: r.score_threshold_bps,
                min_persistence_frames: r.min_persistence_frames,
                min_persistence_ms: r.min_persistence_ms,
                deduplication_window_ms: r.deduplication_window_ms,
            })
            .collect();

        Ok(OperationalEvaluationSummary {
            camera_id: self.source.camera_id.clone(),
            detector_id: self.detector.detector_id.clone(),
            detector_version: self.detector.detector_version.clone(),
            detector_config_digest_sha256: self.detector.config_digest_sha256.clone(),
            evaluation_corpus_id,
            evaluation_corpus_version,
            eval_start_ms,
            eval_end_ms,
            region_configs,
            total_window_ms,
            missing_coverage_ms,
            active_coverage_ms,
            coverage_bps,
            total_supplied_truth_events: labeled_events.len(),
            truth_events_outside_window_count,
            truth_events_in_coverage_gap_count,
            observable_labeled_event_count,
            matched_labeled_event_count,
            event_recall_bps,
            total_candidate_alerts: window_candidates,
            quarantined_fault_candidates_in_window,
            true_positive_alerts,
            false_alert_count,
            false_alerts_per_camera_hour_milli,
            early_detection_count,
            min_detection_latency_ms,
            mean_detection_latency_ms,
            max_detection_latency_ms,
            deduplicated_repeat_triggers_in_window,
            health_events_overlapping_window,
        })
    }

    fn compute_merged_missing_intervals(
        &self,
        eval_start_ms: i64,
        eval_end_ms: i64,
    ) -> Vec<(i64, i64)> {
        let mut intervals: Vec<(i64, i64)> = self
            .health_events
            .iter()
            .filter_map(|ev| {
                let s = ev.start_ms.max(eval_start_ms);
                let e = ev.end_ms.min(eval_end_ms);
                (e > s).then_some((s, e))
            })
            .collect();
        if let Some(restricted_at) = self.restricted_since_ms {
            let s = restricted_at.max(eval_start_ms);
            if eval_end_ms > s {
                intervals.push((s, eval_end_ms));
            }
        }
        if intervals.is_empty() {
            return Vec::new();
        }
        intervals.sort_by_key(|&(s, e)| (s, e));
        let mut merged = Vec::with_capacity(intervals.len());
        let mut cur_start = intervals.first().map_or(eval_start_ms, |&(s, _)| s);
        let mut cur_end = intervals.first().map_or(eval_start_ms, |&(_, e)| e);
        for &(s, e) in intervals.iter().skip(1) {
            if s <= cur_end {
                cur_end = cur_end.max(e);
            } else {
                merged.push((cur_start, cur_end));
                cur_start = s;
                cur_end = e;
            }
        }
        merged.push((cur_start, cur_end));
        merged
    }
}

fn collect_candidate_frame_versions(candidate: &CandidateAnomalyEvent) -> BTreeSet<VersionId> {
    let mut set = BTreeSet::new();
    for seg in &candidate.trigger_segments {
        set.insert(seg.primary_frame.frame_version.clone());
        match &seg.context_window {
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
    }
    set
}

/// Validates cross-camera association and multi-view triangulation readiness between
/// two candidate anomaly events (`CAM-05`, `A09`).
///
/// Requires distinct cameras, independent upstream feeds (`first_monitor.source.upstream_feed_id != second_monitor.source.upstream_feed_id`),
/// non-quarantined candidates, and temporal alignment within `max_allowed_skew_ms`
/// after accounting for both cameras' clock uncertainties.
///
/// If `require_triangulation` is `true`, both cameras must have `CameraCalibrationState::Valid`.
/// If `require_triangulation` is `false`, uncalibrated cameras may still be associated with
/// `TriangulationReadiness::UncalibratedAssociationOnly` (`eligible_for_triangulation() == false`),
/// while movement-invalidated calibrations are always rejected (`CAM-05`, `A09`).
#[allow(clippy::too_many_arguments)]
pub fn validate_cross_camera_association(
    first_monitor: &CameraAnomalyMonitor,
    first_candidate_id: &OpaqueId,
    second_monitor: &CameraAnomalyMonitor,
    second_candidate_id: &OpaqueId,
    require_triangulation: bool,
    max_allowed_skew_ms: u64,
    correspondence_rationale: String,
    limits: AnomalyLimits,
) -> Result<CrossCameraAssociation, AnomalyError> {
    let mut used_text_bytes = 0usize;
    check_single_line_text(
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

    let first_cal = match &first_monitor.source.calibration {
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
        CameraCalibrationState::Uncalibrated => {
            if require_triangulation {
                return Err(AnomalyError::CalibrationRequiredForTriangulation {
                    camera_id: first_monitor.source.camera_id.clone(),
                });
            }
            None
        }
        CameraCalibrationState::Valid {
            calibration_version,
            ..
        } => Some(calibration_version.clone()),
    };

    let second_cal = match &second_monitor.source.calibration {
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
        CameraCalibrationState::Uncalibrated => {
            if require_triangulation {
                return Err(AnomalyError::CalibrationRequiredForTriangulation {
                    camera_id: second_monitor.source.camera_id.clone(),
                });
            }
            None
        }
        CameraCalibrationState::Valid {
            calibration_version,
            ..
        } => Some(calibration_version.clone()),
    };

    let triangulation_readiness = match (first_cal, second_cal) {
        (Some(first_calibration_version), Some(second_calibration_version)) => {
            TriangulationReadiness::Eligible {
                first_calibration_version,
                second_calibration_version,
            }
        }
        (first_opt, second_opt) => {
            let mut uncalibrated_camera_ids = Vec::new();
            if first_opt.is_none() {
                uncalibrated_camera_ids.push(first_monitor.source.camera_id.clone());
            }
            if second_opt.is_none() {
                uncalibrated_camera_ids.push(second_monitor.source.camera_id.clone());
            }
            TriangulationReadiness::UncalibratedAssociationOnly {
                uncalibrated_camera_ids,
            }
        }
    };

    let Some(&first_idx) = first_monitor.candidate_index.get(first_candidate_id) else {
        return Err(AnomalyError::UnknownCandidateId(first_candidate_id.clone()));
    };
    let Some(first_cand) = first_monitor.candidates.get(first_idx) else {
        return Err(AnomalyError::UnknownCandidateId(first_candidate_id.clone()));
    };
    if let Some(fault) = first_cand.quarantined_by_fault {
        return Err(AnomalyError::CameraFaultBlocksSceneTrigger {
            camera_id: first_monitor.source.camera_id.clone(),
            active_health: fault,
        });
    }

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
    if let Some(fault) = second_cand.quarantined_by_fault {
        return Err(AnomalyError::CameraFaultBlocksSceneTrigger {
            camera_id: second_monitor.source.camera_id.clone(),
            active_health: fault,
        });
    }

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
        triangulation_readiness,
        temporal_skew_ms,
        combined_clock_uncertainty_ms,
        correspondence_rationale,
    })
}
