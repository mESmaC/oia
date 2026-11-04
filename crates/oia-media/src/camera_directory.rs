//! Camera directory registration, viewing-cone geometry, feed/still/fallback/synthetic/archive
//! classification, freshness tracking, and bounded retention ring-buffer / clip-manifest kernel
//! (`#28`, `CAM-01`, `CAM-02`, `CAM-04`, `docs/architecture.md` §§3.2, 4.3, 4.4, 8, `A09`).
//!
//! This module shares its domain types directly with [`crate::camera_anomaly`] (`AGENTS.md` §4):
//! [`CameraAccessStatus`], [`CameraCalibrationState`], [`CameraHealthState`],
//! [`CameraSourceProfile`], [`CameraViewKind`], [`ContextWindowStatus`],
//! [`FeedObservationKind`], [`FrameArtifactOrigin`], [`RecordedHealthEvent`],
//! [`RetainedFrameRef`], [`SourceUsePermission`], [`MAX_CLOCK_UNCERTAINTY_MS`], and
//! [`RESTRICTED_SOURCE_POLICY_NOTICE`].
//!
//! Vertical-datum / altitude primitives belong to `oia-time-geo` (`#12`) and are not duplicated
//! here alongside `oia-analysis` or `oia-devices`.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

use oia_domain::{ActorId, IdError, OpaqueId, SchemaVersion, VersionId};

use crate::camera_anomaly::is_disallowed_single_line_char;
pub use crate::camera_anomaly::{
    CANDIDATE_EVENT_CAVEAT, CameraAccessStatus, CameraCalibrationState, CameraHealthState,
    CameraSourceProfile, CameraViewKind, ContextWindowStatus, FeedObservationKind,
    FrameArtifactOrigin, MAX_CLOCK_UNCERTAINTY_MS, RESTRICTED_SOURCE_POLICY_NOTICE,
    RecordedHealthEvent, RetainedFrameRef, SourceUsePermission,
};
use crate::is_valid_lowercase_sha256_hex;

/// Mean spherical Earth radius in meters (IUGG R1 mean radius; Moritz 2000) used for
/// horizontal viewing-cone projection on WGS84 coordinates.
pub const EARTH_MEAN_RADIUS_M: f64 = 6_371_008.8;

/// Maximum permitted viewing-cone projection range in meters (`100_000.0` m = 100 km).
pub const MAX_VIEWING_CONE_RANGE_M: f64 = 100_000.0;

/// Maximum absolute latitude in degrees at which horizontal compass bearings are non-degenerate (`89.9999` deg).
pub const MAX_NON_POLAR_CONE_LATITUDE_DEG: f64 = 89.9999;

/// Minimum number of outer-arc vertices for a projected viewing cone.
pub const MIN_VIEWING_CONE_ARC_VERTICES: usize = 2;

/// Maximum number of outer-arc vertices for a projected viewing cone.
pub const MAX_VIEWING_CONE_ARC_VERTICES: usize = 128;

/// Mandatory provenance notice attached to directory registrations (`CAM-01`, `CAM-02`, `AGENTS.md` §§3, 4).
pub const DIRECTORY_LISTING_PROVENANCE_NOTICE: &str = "A camera directory listing records ordinary access metadata and declared capabilities; it does not by itself establish live stream availability or historical archive access.";

/// Which resource budget was zero or exceeded in the camera directory and retention kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraDirectoryLimitKind {
    RegisteredCameras,
    PrivacyMaskZonesPerCamera,
    HealthTransitionsPerCamera,
    BufferedFramesPerCamera,
    BufferedBytesPerCamera,
    DiscontinuitySpansPerCamera,
    ClipManifestsPerCamera,
    FramesPerClipManifest,
    TextBytes,
}

impl fmt::Display for CameraDirectoryLimitKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RegisteredCameras => f.write_str("max_registered_cameras"),
            Self::PrivacyMaskZonesPerCamera => f.write_str("max_privacy_mask_zones_per_camera"),
            Self::HealthTransitionsPerCamera => f.write_str("max_health_transitions_per_camera"),
            Self::BufferedFramesPerCamera => f.write_str("max_buffered_frames_per_camera"),
            Self::BufferedBytesPerCamera => f.write_str("max_buffered_bytes_per_camera"),
            Self::DiscontinuitySpansPerCamera => f.write_str("max_discontinuity_spans_per_camera"),
            Self::ClipManifestsPerCamera => f.write_str("max_clip_manifests_per_camera"),
            Self::FramesPerClipManifest => f.write_str("max_frames_per_clip_manifest"),
            Self::TextBytes => f.write_str("max_text_bytes"),
        }
    }
}

/// Explicit operational bounds for camera directory cataloging, frame classification,
/// bounded ring-buffer retention, and clip manifest extraction (`AGENTS.md` §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CameraDirectoryLimits {
    max_registered_cameras: usize,
    max_privacy_mask_zones_per_camera: usize,
    max_health_transitions_per_camera: usize,
    max_buffered_frames_per_camera: usize,
    max_buffered_bytes_per_camera: usize,
    max_discontinuity_spans_per_camera: usize,
    max_clip_manifests_per_camera: usize,
    max_frames_per_clip_manifest: usize,
    max_text_bytes: usize,
}

impl CameraDirectoryLimits {
    /// Validates that every operational budget is at least `1`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        max_registered_cameras: usize,
        max_privacy_mask_zones_per_camera: usize,
        max_health_transitions_per_camera: usize,
        max_buffered_frames_per_camera: usize,
        max_buffered_bytes_per_camera: usize,
        max_discontinuity_spans_per_camera: usize,
        max_clip_manifests_per_camera: usize,
        max_frames_per_clip_manifest: usize,
        max_text_bytes: usize,
    ) -> Result<Self, CameraDirectoryError> {
        for (kind, value) in [
            (
                CameraDirectoryLimitKind::RegisteredCameras,
                max_registered_cameras,
            ),
            (
                CameraDirectoryLimitKind::PrivacyMaskZonesPerCamera,
                max_privacy_mask_zones_per_camera,
            ),
            (
                CameraDirectoryLimitKind::HealthTransitionsPerCamera,
                max_health_transitions_per_camera,
            ),
            (
                CameraDirectoryLimitKind::BufferedFramesPerCamera,
                max_buffered_frames_per_camera,
            ),
            (
                CameraDirectoryLimitKind::BufferedBytesPerCamera,
                max_buffered_bytes_per_camera,
            ),
            (
                CameraDirectoryLimitKind::DiscontinuitySpansPerCamera,
                max_discontinuity_spans_per_camera,
            ),
            (
                CameraDirectoryLimitKind::ClipManifestsPerCamera,
                max_clip_manifests_per_camera,
            ),
            (
                CameraDirectoryLimitKind::FramesPerClipManifest,
                max_frames_per_clip_manifest,
            ),
            (CameraDirectoryLimitKind::TextBytes, max_text_bytes),
        ] {
            if value == 0 {
                return Err(CameraDirectoryError::ZeroLimit(kind));
            }
        }
        Ok(Self {
            max_registered_cameras,
            max_privacy_mask_zones_per_camera,
            max_health_transitions_per_camera,
            max_buffered_frames_per_camera,
            max_buffered_bytes_per_camera,
            max_discontinuity_spans_per_camera,
            max_clip_manifests_per_camera,
            max_frames_per_clip_manifest,
            max_text_bytes,
        })
    }

    #[must_use]
    pub const fn max_registered_cameras(self) -> usize {
        self.max_registered_cameras
    }

    #[must_use]
    pub const fn max_privacy_mask_zones_per_camera(self) -> usize {
        self.max_privacy_mask_zones_per_camera
    }

    #[must_use]
    pub const fn max_health_transitions_per_camera(self) -> usize {
        self.max_health_transitions_per_camera
    }

    #[must_use]
    pub const fn max_buffered_frames_per_camera(self) -> usize {
        self.max_buffered_frames_per_camera
    }

    #[must_use]
    pub const fn max_buffered_bytes_per_camera(self) -> usize {
        self.max_buffered_bytes_per_camera
    }

    #[must_use]
    pub const fn max_discontinuity_spans_per_camera(self) -> usize {
        self.max_discontinuity_spans_per_camera
    }

    #[must_use]
    pub const fn max_clip_manifests_per_camera(self) -> usize {
        self.max_clip_manifests_per_camera
    }

    #[must_use]
    pub const fn max_frames_per_clip_manifest(self) -> usize {
        self.max_frames_per_clip_manifest
    }

    #[must_use]
    pub const fn max_text_bytes(self) -> usize {
        self.max_text_bytes
    }
}

/// Ordinary-access method through which a camera source is listed (`CAM-01`, `CAM-02`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrdinaryAccessMethod {
    /// Public camera website or directory listing.
    ExistingDirectoryListing,
    /// Published direct viewing page or link.
    PublishedViewingLink,
    /// Supported public embed widget or player.
    SupportedEmbed,
    /// Supported public feed or API endpoint.
    SupportedFeedOrApi,
}

impl fmt::Display for OrdinaryAccessMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExistingDirectoryListing => f.write_str("existing_directory_listing"),
            Self::PublishedViewingLink => f.write_str("published_viewing_link"),
            Self::SupportedEmbed => f.write_str("supported_embed"),
            Self::SupportedFeedOrApi => f.write_str("supported_feed_or_api"),
        }
    }
}

/// Recorded status of whether intentional publication by the camera owner is established
/// (`CAM-01`, `AGENTS.md` §6).
///
/// Intentional publication by the owner is metadata, never a blanket gate: `NotEstablishedOrdinaryAccess`
/// never refuses or degrades an otherwise ordinarily accessible camera source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentionalPublicationStatus {
    /// Operator or owner explicitly published the camera for public viewing.
    EstablishedByOwner,
    /// Camera is reachable via an existing directory or ordinary access link without established
    /// owner publication (`CAM-01` explicitly permits).
    NotEstablishedOrdinaryAccess,
}

impl fmt::Display for IntentionalPublicationStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EstablishedByOwner => f.write_str("established_by_owner"),
            Self::NotEstablishedOrdinaryAccess => f.write_str("not_established_ordinary_access"),
        }
    }
}

/// Declared live video stream capability of a cataloged camera (`CAM-02`, `docs/architecture.md` §4.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamCapability {
    /// No continuous live video stream is declared.
    None,
    /// Live stream is supported with a nominal frame interval in milliseconds (`>= 1`).
    LiveStreamSupported { nominal_frame_interval_ms: u64 },
}

/// Declared periodic still-image refresh capability of a cataloged camera (`CAM-02`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StillRefreshCapability {
    /// No periodic still refresh endpoint is declared.
    None,
    /// Periodic still refresh with nominal interval (`>= 1` ms) and explicit staleness threshold
    /// (`stale_after_ms >= nominal_refresh_interval_ms`).
    PeriodicStillSupported {
        nominal_refresh_interval_ms: u64,
        stale_after_ms: u64,
    },
}

/// Declared historical archive capability of a cataloged camera (`CAM-02`, `AGENTS.md` §§3, 4).
///
/// A directory listing or live feed does not imply historical archive access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchiveCapability {
    /// No historical archive access is established for this camera.
    NoArchiveEstablished,
    /// Verified historical archive window pinned to an archive dataset/catalog version.
    VerifiedArchiveWindow {
        archive_version: VersionId,
        earliest_capture_ms: i64,
        latest_capture_ms: i64,
    },
}

/// Privacy mask zone covering private windows or private areas in an exterior view (`CAM-01`, `CAM-02`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivacyMaskZone {
    pub zone_id: OpaqueId,
    pub label: String,
    pub mask_applied: bool,
}

/// Assessment of the camera's actual view for private activity (`CAM-01`, `CAM-02`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivacyViewAssessment {
    /// Public street, sky, weather, or commercial exterior view verified free of private interior views.
    ClearPublicView {
        assessed_by: ActorId,
        assessed_at_ms: i64,
    },
    /// View includes residential building exteriors or private windows that require mask zones (`CAM-02`).
    RequiresMaskZones {
        assessed_by: ActorId,
        assessed_at_ms: i64,
        zones: Vec<PrivacyMaskZone>,
    },
    /// Actual view exposes private activity that cannot be masked; automated processing and frame
    /// acquisition are refused while retaining the exclusion record (`CAM-02`).
    ExcludedPrivateActivity {
        assessed_by: ActorId,
        assessed_at_ms: i64,
        reason: String,
    },
}

/// Dated data-use terms for a camera source (`CAM-02`, `AGENTS.md` §§2, 6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraSourceTerms {
    pub automated_processing: SourceUsePermission,
    pub clip_retention: SourceUsePermission,
    pub redistribution: SourceUsePermission,
    pub max_retention_window_ms: Option<u64>,
    pub policy_reference: String,
    pub checked_at_ms: i64,
}

/// Horizontal camera position on WGS84 (`CAM-02`, `docs/architecture.md` §3.2).
///
/// Vertical-datum / altitude primitives belong to `oia-time-geo` (`#12`) and are not duplicated here.
#[derive(Debug, Clone, PartialEq)]
pub enum CameraHorizontalPosition {
    /// Surveyed or reported WGS84 coordinates with explicit horizontal uncertainty (`> 0.0` m).
    KnownWgs84 {
        latitude_deg: f64,
        longitude_deg: f64,
        horizontal_uncertainty_m: f64,
    },
    /// Gazetteer or city centroid explicitly labeled as an approximate locality (`docs/architecture.md` §3.2).
    ApproximateGazetteerCentroid {
        locality_label: String,
        latitude_deg: f64,
        longitude_deg: f64,
        uncertainty_radius_m: f64,
    },
    /// Camera coordinates are unknown; never converted to `(0.0, 0.0)` (`AGENTS.md` §2).
    Unknown { reason: String },
}

/// Viewing orientation and field-of-view state of a camera (`CAM-02`, `CAM-05`, `docs/architecture.md` §§3.2, 8).
#[derive(Debug, Clone, PartialEq)]
pub enum CameraViewingOrientation {
    /// Calibrated orientation pinned to an immutable calibration version and angular error bound.
    Calibrated {
        calibration_version: VersionId,
        /// Compass azimuth in degrees `[0.0, 360.0)`.
        azimuth_deg: f64,
        /// Optional pitch angle in degrees `[-90.0, 90.0]`.
        pitch_deg: Option<f64>,
        /// Optional roll angle in degrees `[-180.0, 180.0]`.
        roll_deg: Option<f64>,
        /// Horizontal field of view in degrees `(0.0, 360.0)`.
        horizontal_fov_deg: f64,
        /// Optional vertical field of view in degrees `(0.0, 180.0]`.
        vertical_fov_deg: Option<f64>,
        /// Maximum angular calibration error in millidegrees (`>= 1`).
        max_angular_error_mdeg: u32,
    },
    /// Uncalibrated reported compass bearing and approximate horizontal field of view.
    ReportedApproximate {
        /// Reported compass azimuth in degrees `[0.0, 360.0)`.
        azimuth_deg: f64,
        /// Reported horizontal field of view in degrees `(0.0, 360.0)`.
        horizontal_fov_deg: f64,
        /// Stated bearing uncertainty in degrees `(0.0, 180.0]`.
        bearing_uncertainty_deg: f64,
    },
    /// Orientation was invalidated because the camera moved, panned, zoomed, or repositioned (`CAM-05`, `A09`).
    InvalidatedByMovement {
        prior_calibration_version: Option<VersionId>,
        last_known_azimuth_deg: Option<f64>,
        detected_at_ms: i64,
        reason: String,
    },
    /// Camera orientation is unknown (`AGENTS.md` §2).
    Unknown { reason: String },
}

impl CameraViewingOrientation {
    /// Maps this orientation into the shared [`CameraCalibrationState`] used by [`crate::camera_anomaly`].
    #[must_use]
    pub fn to_calibration_state(&self) -> CameraCalibrationState {
        match self {
            Self::Calibrated {
                calibration_version,
                max_angular_error_mdeg,
                ..
            } => CameraCalibrationState::Valid {
                calibration_version: calibration_version.clone(),
                max_angular_error_mdeg: *max_angular_error_mdeg,
            },
            Self::InvalidatedByMovement {
                prior_calibration_version: Some(prior),
                detected_at_ms,
                ..
            } => CameraCalibrationState::InvalidatedByMovement {
                prior_calibration_version: prior.clone(),
                detected_at_ms: *detected_at_ms,
            },
            Self::ReportedApproximate { .. }
            | Self::InvalidatedByMovement {
                prior_calibration_version: None,
                ..
            }
            | Self::Unknown { .. } => CameraCalibrationState::Uncalibrated,
        }
    }
}

/// Source lifecycle health state tracked separately from media frame observations (`docs/architecture.md` §4.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceLifecycleHealth {
    Unconfigured,
    Connecting,
    Available,
    Degraded,
    Stale,
    UnavailableRestricted,
    DisabledByPolicy,
}

impl fmt::Display for SourceLifecycleHealth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unconfigured => f.write_str("unconfigured"),
            Self::Connecting => f.write_str("connecting"),
            Self::Available => f.write_str("available"),
            Self::Degraded => f.write_str("degraded"),
            Self::Stale => f.write_str("stale"),
            Self::UnavailableRestricted => f.write_str("unavailable_restricted"),
            Self::DisabledByPolicy => f.write_str("disabled_by_policy"),
        }
    }
}

/// Recorded source lifecycle health transition (`docs/architecture.md` §4.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceHealthTransition {
    transition_id: OpaqueId,
    from_state: SourceLifecycleHealth,
    to_state: SourceLifecycleHealth,
    occurred_at_ms: i64,
    fault_state: Option<CameraHealthState>,
    detail: String,
}

impl SourceHealthTransition {
    #[must_use]
    pub fn transition_id(&self) -> &OpaqueId {
        &self.transition_id
    }

    #[must_use]
    pub const fn from_state(&self) -> SourceLifecycleHealth {
        self.from_state
    }

    #[must_use]
    pub const fn to_state(&self) -> SourceLifecycleHealth {
        self.to_state
    }

    #[must_use]
    pub const fn occurred_at_ms(&self) -> i64 {
        self.occurred_at_ms
    }

    #[must_use]
    pub const fn fault_state(&self) -> Option<CameraHealthState> {
        self.fault_state
    }

    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

/// Epistemic class of a projected horizontal viewing cone (`CAM-02`, `CAM-05`, `docs/architecture.md` §8).
#[derive(Debug, Clone, PartialEq)]
pub enum ViewingConeClass {
    /// Derived from a calibrated orientation with a pinned calibration version and angular error bound.
    CalibratedFieldOfView {
        calibration_version: VersionId,
        max_angular_error_mdeg: u32,
    },
    /// Illustrative wedge derived from an uncalibrated reported compass bearing; not suitable for
    /// geometric triangulation.
    IllustrativeReportedBearing { bearing_uncertainty_deg: f64 },
}

/// Projected horizontal viewing cone on the WGS84 sphere (`CAM-02`, `docs/architecture.md` §§3.2, 8).
#[derive(Debug, Clone, PartialEq)]
pub struct ViewingConeGeometry {
    camera_id: OpaqueId,
    source_version: VersionId,
    cone_class: ViewingConeClass,
    apex_latitude_deg: f64,
    apex_longitude_deg: f64,
    horizontal_uncertainty_m: f64,
    azimuth_deg: f64,
    horizontal_fov_deg: f64,
    uncertainty_widened_horizontal_fov_deg: f64,
    range_m: f64,
    arc_vertices_lat_lon_deg: Vec<(f64, f64)>,
    uncertainty_widened_arc_vertices_lat_lon_deg: Vec<(f64, f64)>,
}

impl ViewingConeGeometry {
    #[must_use]
    pub fn camera_id(&self) -> &OpaqueId {
        &self.camera_id
    }

    #[must_use]
    pub fn source_version(&self) -> &VersionId {
        &self.source_version
    }

    #[must_use]
    pub fn cone_class(&self) -> &ViewingConeClass {
        &self.cone_class
    }

    #[must_use]
    pub const fn is_calibrated(&self) -> bool {
        matches!(
            self.cone_class,
            ViewingConeClass::CalibratedFieldOfView { .. }
        )
    }

    #[must_use]
    pub const fn apex_latitude_deg(&self) -> f64 {
        self.apex_latitude_deg
    }

    #[must_use]
    pub const fn apex_longitude_deg(&self) -> f64 {
        self.apex_longitude_deg
    }

    #[must_use]
    pub const fn horizontal_uncertainty_m(&self) -> f64 {
        self.horizontal_uncertainty_m
    }

    #[must_use]
    pub const fn azimuth_deg(&self) -> f64 {
        self.azimuth_deg
    }

    #[must_use]
    pub const fn horizontal_fov_deg(&self) -> f64 {
        self.horizontal_fov_deg
    }

    /// Horizontal FOV widened by `2 * (angular_uncertainty_deg + asin(horizontal_uncertainty_m / range_m))`
    /// (with `asin(...) = 180.0` deg when `horizontal_uncertainty_m >= range_m`), clamped below `360.0` deg
    /// (`CAM-02`, `CAM-05`, `docs/architecture.md` §§3.2, 8).
    #[must_use]
    pub const fn uncertainty_widened_horizontal_fov_deg(&self) -> f64 {
        self.uncertainty_widened_horizontal_fov_deg
    }

    #[must_use]
    pub const fn range_m(&self) -> f64 {
        self.range_m
    }

    /// Nominal outer-arc vertices projected from `(apex_latitude_deg, apex_longitude_deg)` across `horizontal_fov_deg`.
    #[must_use]
    pub fn arc_vertices_lat_lon_deg(&self) -> &[(f64, f64)] {
        &self.arc_vertices_lat_lon_deg
    }

    /// Outer-arc vertices projected from the nominal apex `(apex_latitude_deg, apex_longitude_deg)` across
    /// `uncertainty_widened_horizontal_fov_deg` (`horizontal_fov_deg` widened by both calibration/bearing
    /// angular uncertainty and `asin(horizontal_uncertainty_m / range_m)` at `range_m`).
    #[must_use]
    pub fn uncertainty_widened_arc_vertices_lat_lon_deg(&self) -> &[(f64, f64)] {
        &self.uncertainty_widened_arc_vertices_lat_lon_deg
    }
}

/// Registration input for cataloging a camera source in the directory (`CAM-01`, `CAM-02`).
#[derive(Debug, Clone, PartialEq)]
pub struct CameraDirectoryRegistrationInput {
    pub camera_id: OpaqueId,
    pub upstream_feed_id: OpaqueId,
    pub source_version: VersionId,
    pub schema_version: SchemaVersion,
    pub registered_by: ActorId,
    pub registered_at_ms: i64,
    pub provider_id: OpaqueId,
    pub provider_attribution: String,
    pub listing_reference: String,
    pub access_method: OrdinaryAccessMethod,
    pub intentional_publication: IntentionalPublicationStatus,
    pub view_kind: CameraViewKind,
    pub privacy_assessment: PrivacyViewAssessment,
    pub stream_capability: StreamCapability,
    pub still_capability: StillRefreshCapability,
    pub archive_capability: ArchiveCapability,
    pub terms: CameraSourceTerms,
    pub position: CameraHorizontalPosition,
    pub orientation: CameraViewingOrientation,
    pub initial_health: SourceLifecycleHealth,
}

/// Freshness and provenance evaluation attached to a classified camera frame (`CAM-02`, `AGENTS.md` §2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameFreshnessEvaluation {
    /// Frame from a live stream whose capture age (when `source_capture_time_ms` is present) is
    /// verified within the camera's retention/live staleness bound (`max(max_retention_window_ms, 2 * nominal_frame_interval_ms)`).
    LiveStream {
        nominal_frame_interval_ms: u64,
        source_capture_time_ms: Option<i64>,
        ingestion_time_ms: i64,
        observed_capture_age_ms: Option<u64>,
    },
    /// Periodic upstream still with an explicit `source_capture_time_ms` verified fresh against
    /// the declared `stale_after_ms` threshold.
    FreshPeriodicStill {
        stale_after_ms: u64,
        source_capture_time_ms: i64,
        ingestion_time_ms: i64,
        observed_capture_age_ms: u64,
        unchanged_hold_duration_ms: u64,
    },
    /// Periodic upstream still whose `source_capture_time_ms` was omitted by the source (`AGENTS.md` §2).
    /// Because true capture age cannot be verified without a source capture timestamp, this still
    /// is never treated as unconditionally fresh and is ineligible as event evidence.
    PeriodicStillCaptureTimeUnknown {
        stale_after_ms: u64,
        ingestion_time_ms: i64,
        unchanged_hold_duration_ms: u64,
    },
    /// Periodic upstream still downgraded to [`FeedObservationKind::StaleStillHold`] because its
    /// capture age or unchanged digest/timestamp hold duration exceeded `stale_after_ms`.
    StalePeriodicStill {
        stale_after_ms: u64,
        source_capture_time_ms: Option<i64>,
        ingestion_time_ms: i64,
        observed_capture_age_ms: Option<u64>,
        unchanged_hold_duration_ms: u64,
    },
    /// Frame retrieved from a verified historical archive window (`CAM-02`).
    VerifiedHistoricalArchive {
        archive_version: VersionId,
        window_earliest_ms: i64,
        window_latest_ms: i64,
        source_capture_time_ms: i64,
        ingestion_time_ms: i64,
    },
    /// Non-observation substitute (`StillFallback`, `StreetViewFallback`, `SyntheticPlaceholder`,
    /// or caller-declared `StaleStillHold`) that must never serve as event evidence.
    NonEvidentiarySubstitute {
        substitute_kind: FeedObservationKind,
        ingestion_time_ms: i64,
    },
}

/// Input frame or still sample delivered from a cataloged camera source (`CAM-02`, `CAM-04`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraFrameSubmission {
    pub frame_version: VersionId,
    pub sha256_hex: String,
    pub byte_len: usize,
    pub sequence_number: u64,
    pub declared_kind: FeedObservationKind,
    pub origin: FrameArtifactOrigin,
    /// Optional source-reported capture timestamp in UTC epoch milliseconds. Kept distinct from
    /// `ingestion_time_ms` and never synthesized when absent (`AGENTS.md` §2).
    pub source_capture_time_ms: Option<i64>,
    pub clock_uncertainty_ms: u64,
    pub ingestion_time_ms: i64,
    /// Required when `declared_kind == FeedObservationKind::HistoricalArchiveFrame`.
    pub archive_version: Option<VersionId>,
}

/// Classified camera frame record with explicit evidence eligibility, acquisition `source_version`,
/// and buffer disposition (`CAM-02`, `CAM-04`, `AGENTS.md` §2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifiedCameraFrame {
    frame_version: VersionId,
    source_version: VersionId,
    sha256_hex: String,
    byte_len: usize,
    sequence_number: u64,
    declared_kind: FeedObservationKind,
    effective_kind: FeedObservationKind,
    origin: FrameArtifactOrigin,
    source_capture_time_ms: Option<i64>,
    clock_uncertainty_ms: u64,
    ingestion_time_ms: i64,
    freshness: FrameFreshnessEvaluation,
    retained_in_ring_buffer: bool,
}

impl ClassifiedCameraFrame {
    #[must_use]
    pub fn frame_version(&self) -> &VersionId {
        &self.frame_version
    }

    /// Exact `source_version` of the camera registration under which this frame was acquired (`AGENTS.md` §2).
    #[must_use]
    pub fn source_version(&self) -> &VersionId {
        &self.source_version
    }

    #[must_use]
    pub fn sha256_hex(&self) -> &str {
        &self.sha256_hex
    }

    #[must_use]
    pub const fn byte_len(&self) -> usize {
        self.byte_len
    }

    #[must_use]
    pub const fn sequence_number(&self) -> u64 {
        self.sequence_number
    }

    #[must_use]
    pub const fn declared_kind(&self) -> FeedObservationKind {
        self.declared_kind
    }

    #[must_use]
    pub const fn effective_kind(&self) -> FeedObservationKind {
        self.effective_kind
    }

    #[must_use]
    pub const fn origin(&self) -> FrameArtifactOrigin {
        self.origin
    }

    #[must_use]
    pub const fn source_capture_time_ms(&self) -> Option<i64> {
        self.source_capture_time_ms
    }

    #[must_use]
    pub const fn clock_uncertainty_ms(&self) -> u64 {
        self.clock_uncertainty_ms
    }

    #[must_use]
    pub const fn ingestion_time_ms(&self) -> i64 {
        self.ingestion_time_ms
    }

    #[must_use]
    pub fn freshness(&self) -> &FrameFreshnessEvaluation {
        &self.freshness
    }

    /// True only when this frame is a qualifying camera observation with an explicit
    /// `source_capture_time_ms` (`UpstreamCameraFeed` with capture time, verified `FreshPeriodicStill`,
    /// or `VerifiedHistoricalArchive`).
    ///
    /// Stale holds, non-observation substitutes, and frames without an explicit `source_capture_time_ms`
    /// always return `false` so `ingestion_time_ms` is never fabricated as a capture time (`AGENTS.md` §2).
    #[must_use]
    pub const fn eligible_as_event_evidence(&self) -> bool {
        matches!(
            self.freshness,
            FrameFreshnessEvaluation::LiveStream {
                source_capture_time_ms: Some(_),
                ..
            } | FrameFreshnessEvaluation::FreshPeriodicStill { .. }
                | FrameFreshnessEvaluation::VerifiedHistoricalArchive { .. }
        )
    }

    #[must_use]
    pub const fn retained_in_ring_buffer(&self) -> bool {
        self.retained_in_ring_buffer
    }

    /// Converts a qualifying evidentiary frame with an explicit `source_capture_time_ms` into the
    /// shared [`RetainedFrameRef`] used by [`crate::camera_anomaly`].
    ///
    /// - Refuses non-evidentiary substitutes and stale holds (`NonEvidentiaryFrameCannotBecomeEvidence`).
    /// - Refuses frames without an explicit `source_capture_time_ms` (`MissingSourceCaptureTimeForEvidence`)
    ///   so `ingestion_time_ms` is never substituted for `capture_time_ms` (`AGENTS.md` §2).
    pub fn to_retained_frame_ref(&self) -> Result<RetainedFrameRef, CameraDirectoryError> {
        if !self.effective_kind.eligible_as_event_evidence() {
            return Err(
                CameraDirectoryError::NonEvidentiaryFrameCannotBecomeEvidence {
                    frame_version: self.frame_version.clone(),
                    effective_kind: self.effective_kind,
                },
            );
        }
        let Some(capture_time_ms) = self.source_capture_time_ms else {
            return Err(CameraDirectoryError::MissingSourceCaptureTimeForEvidence {
                frame_version: self.frame_version.clone(),
            });
        };
        Ok(RetainedFrameRef {
            frame_version: self.frame_version.clone(),
            sha256_hex: self.sha256_hex.clone(),
            capture_time_ms,
            ingestion_time_ms: self.ingestion_time_ms,
            origin: self.origin,
        })
    }
}

/// Recorded sequence gap / dropped-frame discontinuity on a camera session (`CAM-04`, `docs/architecture.md` §8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameDiscontinuitySpan {
    source_version: VersionId,
    previous_sequence_number: u64,
    next_sequence_number: u64,
    dropped_frame_count: u64,
    start_ms: i64,
    end_ms: i64,
}

impl FrameDiscontinuitySpan {
    #[must_use]
    pub fn source_version(&self) -> &VersionId {
        &self.source_version
    }

    #[must_use]
    pub const fn previous_sequence_number(&self) -> u64 {
        self.previous_sequence_number
    }

    #[must_use]
    pub const fn next_sequence_number(&self) -> u64 {
        self.next_sequence_number
    }

    #[must_use]
    pub const fn dropped_frame_count(&self) -> u64 {
        self.dropped_frame_count
    }

    #[must_use]
    pub const fn start_ms(&self) -> i64 {
        self.start_ms
    }

    #[must_use]
    pub const fn end_ms(&self) -> i64 {
        self.end_ms
    }
}

/// Temporal coverage gap inside `[pre_start_ms, post_end_ms]` exceeding the camera's coverage
/// tolerance (`2 * nominal_frame_interval_ms` or `2 * nominal_refresh_interval_ms`) (`CAM-04`, `AGENTS.md` §2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageGapSpan {
    start_ms: i64,
    end_ms: i64,
    gap_duration_ms: u64,
    tolerance_ms: u64,
}

impl CoverageGapSpan {
    #[must_use]
    pub const fn start_ms(&self) -> i64 {
        self.start_ms
    }

    #[must_use]
    pub const fn end_ms(&self) -> i64 {
        self.end_ms
    }

    #[must_use]
    pub const fn gap_duration_ms(&self) -> u64 {
        self.gap_duration_ms
    }

    #[must_use]
    pub const fn tolerance_ms(&self) -> u64 {
        self.tolerance_ms
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EvictedFrameRecord {
    capture_time_ms: i64,
    ingestion_time_ms: i64,
    byte_len: usize,
    by_window: bool,
}

/// Cumulative ledger of frames evicted from a camera's bounded retention ring buffer (`CAM-04`, `A09`).
///
/// Eviction order is strictly deterministic: FIFO by `(ingestion_time_ms, sequence_number, frame_version)`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RingBufferEvictionSummary {
    expired_by_window_frames: u64,
    expired_by_window_bytes: u64,
    evicted_by_capacity_frames: u64,
    evicted_by_capacity_bytes: u64,
    earliest_evicted_capture_time_ms: Option<i64>,
    latest_evicted_capture_time_ms: Option<i64>,
    evicted_records: VecDeque<EvictedFrameRecord>,
    dropped_from_ledger_frames: u64,
    latest_dropped_from_ledger_capture_time_ms: Option<i64>,
}

impl RingBufferEvictionSummary {
    #[must_use]
    pub const fn expired_by_window_frames(&self) -> u64 {
        self.expired_by_window_frames
    }

    #[must_use]
    pub const fn expired_by_window_bytes(&self) -> u64 {
        self.expired_by_window_bytes
    }

    #[must_use]
    pub const fn evicted_by_capacity_frames(&self) -> u64 {
        self.evicted_by_capacity_frames
    }

    #[must_use]
    pub const fn evicted_by_capacity_bytes(&self) -> u64 {
        self.evicted_by_capacity_bytes
    }

    #[must_use]
    pub const fn total_evicted_frames(&self) -> u64 {
        self.expired_by_window_frames
            .saturating_add(self.evicted_by_capacity_frames)
    }

    #[must_use]
    pub const fn earliest_evicted_capture_time_ms(&self) -> Option<i64> {
        self.earliest_evicted_capture_time_ms
    }

    #[must_use]
    pub const fn latest_evicted_capture_time_ms(&self) -> Option<i64> {
        self.latest_evicted_capture_time_ms
    }

    /// Number of per-frame eviction records that rolled off the bounded per-frame ledger.
    #[must_use]
    pub const fn dropped_from_ledger_frames(&self) -> u64 {
        self.dropped_from_ledger_frames
    }

    /// Counts evicted frames retained in the bounded ledger whose capture time falls within
    /// `[start_ms, end_ms]`, returning `(expired_by_window_in_span, evicted_by_capacity_in_span, ledger_truncated)`.
    ///
    /// When `ledger_truncated` is `true`, older eviction records whose capture time fell inside or
    /// after `start_ms` rolled off the bounded per-frame ledger, so `(expired_by_window_in_span, evicted_by_capacity_in_span)`
    /// are verified lower bounds rather than exact counts (`AGENTS.md` §2).
    #[must_use]
    pub fn evicted_counts_in_window(&self, start_ms: i64, end_ms: i64) -> (u32, u32, bool) {
        let mut by_window = 0u32;
        let mut by_capacity = 0u32;
        for rec in &self.evicted_records {
            if rec.capture_time_ms >= start_ms && rec.capture_time_ms <= end_ms {
                if rec.by_window {
                    by_window = by_window.saturating_add(1);
                } else {
                    by_capacity = by_capacity.saturating_add(1);
                }
            }
        }
        let ledger_truncated = self.dropped_from_ledger_frames > 0
            && self
                .latest_dropped_from_ledger_capture_time_ms
                .is_some_and(|dropped_cap_ms| dropped_cap_ms >= start_ms);
        (by_window, by_capacity, ledger_truncated)
    }

    fn record_eviction(
        &mut self,
        frame: &ClassifiedCameraFrame,
        capture_time_ms: i64,
        by_window: bool,
        max_records: usize,
    ) {
        self.earliest_evicted_capture_time_ms = Some(
            self.earliest_evicted_capture_time_ms
                .map_or(capture_time_ms, |prev| prev.min(capture_time_ms)),
        );
        self.latest_evicted_capture_time_ms = Some(
            self.latest_evicted_capture_time_ms
                .map_or(capture_time_ms, |prev| prev.max(capture_time_ms)),
        );
        if by_window {
            self.expired_by_window_frames = self.expired_by_window_frames.saturating_add(1);
            self.expired_by_window_bytes = self
                .expired_by_window_bytes
                .saturating_add(frame.byte_len as u64);
        } else {
            self.evicted_by_capacity_frames = self.evicted_by_capacity_frames.saturating_add(1);
            self.evicted_by_capacity_bytes = self
                .evicted_by_capacity_bytes
                .saturating_add(frame.byte_len as u64);
        }
        while self.evicted_records.len().saturating_add(1) > max_records {
            if let Some(dropped) = self.evicted_records.pop_front() {
                self.dropped_from_ledger_frames = self.dropped_from_ledger_frames.saturating_add(1);
                self.latest_dropped_from_ledger_capture_time_ms = Some(
                    self.latest_dropped_from_ledger_capture_time_ms
                        .map_or(dropped.capture_time_ms, |prev| {
                            prev.max(dropped.capture_time_ms)
                        }),
                );
            }
        }
        self.evicted_records.push_back(EvictedFrameRecord {
            capture_time_ms,
            ingestion_time_ms: frame.ingestion_time_ms,
            byte_len: frame.byte_len,
            by_window,
        });
    }
}

/// Request to extract a retained evidence clip manifest from a camera's bounded ring buffer
/// (`CAM-04`, `docs/architecture.md` §8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipExtractionRequest {
    pub clip_id: OpaqueId,
    pub clip_version: VersionId,
    pub source_session_id: OpaqueId,
    /// Half-open `[event_start_ms, event_end_ms)` event interval.
    pub event_start_ms: i64,
    pub event_end_ms: i64,
    pub requested_pre_context_ms: u64,
    pub requested_post_context_ms: u64,
    pub extracted_at_ms: i64,
    pub trigger_explanation: String,
}

/// Retained evidence clip manifest referencing original frame bytes, timing, sequence drops,
/// source session, and pre/post context completeness (`CAM-04`, `docs/architecture.md` §8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetainedCameraClipManifest {
    clip_id: OpaqueId,
    clip_version: VersionId,
    camera_id: OpaqueId,
    source_version: VersionId,
    source_session_id: OpaqueId,
    event_start_ms: i64,
    event_end_ms: i64,
    requested_pre_context_ms: u64,
    requested_post_context_ms: u64,
    extracted_at_ms: i64,
    trigger_explanation: String,
    primary_frame: RetainedFrameRef,
    pre_context_frames: Vec<ClassifiedCameraFrame>,
    event_frames: Vec<ClassifiedCameraFrame>,
    post_context_frames: Vec<ClassifiedCameraFrame>,
    excluded_cross_version_frames: Vec<ClassifiedCameraFrame>,
    restriction_intervals_in_window: Vec<(i64, Option<i64>)>,
    coverage_gaps_in_window: Vec<CoverageGapSpan>,
    context_window: ContextWindowStatus,
    expired_by_window_in_span: u32,
    evicted_by_capacity_in_span: u32,
    eviction_ledger_truncated: bool,
    dropped_frame_count_in_span: u64,
    empty_context_window_count: u8,
    missing_pre_context: bool,
    missing_post_context: bool,
    discontinuity_spans_in_window: Vec<FrameDiscontinuitySpan>,
    redistribution_permission: SourceUsePermission,
    policy_reference: String,
}

impl RetainedCameraClipManifest {
    #[must_use]
    pub fn clip_id(&self) -> &OpaqueId {
        &self.clip_id
    }

    #[must_use]
    pub fn clip_version(&self) -> &VersionId {
        &self.clip_version
    }

    #[must_use]
    pub fn camera_id(&self) -> &OpaqueId {
        &self.camera_id
    }

    #[must_use]
    pub fn source_version(&self) -> &VersionId {
        &self.source_version
    }

    #[must_use]
    pub fn source_session_id(&self) -> &OpaqueId {
        &self.source_session_id
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
    pub const fn requested_pre_context_ms(&self) -> u64 {
        self.requested_pre_context_ms
    }

    #[must_use]
    pub const fn requested_post_context_ms(&self) -> u64 {
        self.requested_post_context_ms
    }

    #[must_use]
    pub const fn extracted_at_ms(&self) -> i64 {
        self.extracted_at_ms
    }

    #[must_use]
    pub fn trigger_explanation(&self) -> &str {
        &self.trigger_explanation
    }

    #[must_use]
    pub fn primary_frame(&self) -> &RetainedFrameRef {
        &self.primary_frame
    }

    /// Full classified pre-context frames (including `byte_len`, `sequence_number`, and `source_version`).
    #[must_use]
    pub fn pre_context_frames(&self) -> &[ClassifiedCameraFrame] {
        &self.pre_context_frames
    }

    /// Full classified event-window frames (including `byte_len`, `sequence_number`, and `source_version`).
    #[must_use]
    pub fn event_frames(&self) -> &[ClassifiedCameraFrame] {
        &self.event_frames
    }

    /// Full classified post-context frames (including `byte_len`, `sequence_number`, and `source_version`).
    #[must_use]
    pub fn post_context_frames(&self) -> &[ClassifiedCameraFrame] {
        &self.post_context_frames
    }

    /// Buffered frames in `[pre_start_ms, post_end_ms]` that were excluded from the manifest because
    /// they were acquired under a different `source_version` across a restriction/re-registration boundary.
    #[must_use]
    pub fn excluded_cross_version_frames(&self) -> &[ClassifiedCameraFrame] {
        &self.excluded_cross_version_frames
    }

    /// Restricted/unavailable or policy-disabled intervals `(start_ms, end_ms)` overlapping `[pre_start_ms, post_end_ms]`.
    ///
    /// Transition timestamps (`occurred_at_ms` / `reregistered_at_ms`) are recorded on the operator/ingestion
    /// clock and compared directly against `[pre_start_ms, post_end_ms]` (subject to each frame's declared
    /// `clock_uncertainty_ms`).
    #[must_use]
    pub fn restriction_intervals_in_window(&self) -> &[(i64, Option<i64>)] {
        &self.restriction_intervals_in_window
    }

    /// Coverage gaps inside `[pre_start_ms, post_end_ms]` exceeding `2 * nominal_interval_ms`.
    #[must_use]
    pub fn coverage_gaps_in_window(&self) -> &[CoverageGapSpan] {
        &self.coverage_gaps_in_window
    }

    /// Shared [`ContextWindowStatus`] directly consumable by [`crate::camera_anomaly::SceneTriggerInput`].
    ///
    /// Returns [`ContextWindowStatus::PartialBufferExpired`] for any partial or degraded window
    /// (window expiry, capacity eviction, sequence drops, coverage gaps, restriction/disabled intervals,
    /// cross-version exclusions, or empty requested pre/post context windows); exact cause breakdowns
    /// are exposed via the dedicated accessors on [`RetainedCameraClipManifest`].
    #[must_use]
    pub fn context_window(&self) -> &ContextWindowStatus {
        &self.context_window
    }

    /// Window-expired frames recorded in the bounded ledger for `[pre_start_ms, post_end_ms]`.
    /// When [`Self::eviction_ledger_truncated`] is `true`, this is a verified lower bound.
    #[must_use]
    pub const fn expired_by_window_in_span(&self) -> u32 {
        self.expired_by_window_in_span
    }

    /// Capacity-evicted frames recorded in the bounded ledger for `[pre_start_ms, post_end_ms]`.
    /// When [`Self::eviction_ledger_truncated`] is `true`, this is a verified lower bound.
    #[must_use]
    pub const fn evicted_by_capacity_in_span(&self) -> u32 {
        self.evicted_by_capacity_in_span
    }

    /// True when the per-camera eviction ledger rolled off older records whose capture timestamps
    /// fell inside `[pre_start_ms, post_end_ms]`, meaning [`Self::expired_by_window_in_span`] and
    /// [`Self::evicted_by_capacity_in_span`] are verified lower bounds rather than exact counts (`AGENTS.md` §2).
    #[must_use]
    pub const fn eviction_ledger_truncated(&self) -> bool {
        self.eviction_ledger_truncated
    }

    #[must_use]
    pub const fn dropped_frame_count_in_span(&self) -> u64 {
        self.dropped_frame_count_in_span
    }

    /// Number of requested non-zero context windows (`0..=2`) that were completely empty or incomplete.
    /// Tracked separately from frame counts (`AGENTS.md` §2).
    #[must_use]
    pub const fn empty_context_window_count(&self) -> u8 {
        self.empty_context_window_count
    }

    #[must_use]
    pub const fn missing_pre_context(&self) -> bool {
        self.missing_pre_context
    }

    #[must_use]
    pub const fn missing_post_context(&self) -> bool {
        self.missing_post_context
    }

    #[must_use]
    pub fn discontinuity_spans_in_window(&self) -> &[FrameDiscontinuitySpan] {
        &self.discontinuity_spans_in_window
    }

    #[must_use]
    pub const fn redistribution_permission(&self) -> SourceUsePermission {
        self.redistribution_permission
    }

    #[must_use]
    pub fn policy_reference(&self) -> &str {
        &self.policy_reference
    }

    #[must_use]
    pub const fn candidate_caveat(&self) -> &'static str {
        CANDIDATE_EVENT_CAVEAT
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LastPeriodicStillTracker {
    sha256_hex: String,
    source_capture_time_ms: Option<i64>,
    first_seen_ingestion_ms: i64,
}

/// Cataloged camera entry with capabilities, privacy assessment, viewing geometry,
/// lifecycle health transitions, and bounded retention ring buffer (`CAM-01`, `CAM-02`, `CAM-04`).
#[derive(Debug, Clone)]
pub struct RegisteredCameraEntry {
    camera_id: OpaqueId,
    upstream_feed_id: OpaqueId,
    source_version: VersionId,
    prior_source_versions: Vec<VersionId>,
    schema_version: SchemaVersion,
    registered_by: ActorId,
    registered_at_ms: i64,
    provider_id: OpaqueId,
    provider_attribution: String,
    listing_reference: String,
    access_method: OrdinaryAccessMethod,
    intentional_publication: IntentionalPublicationStatus,
    access_status: CameraAccessStatus,
    restricted_since_ms: Option<i64>,
    restriction_intervals: Vec<(i64, Option<i64>)>,
    view_kind: CameraViewKind,
    privacy_assessment: PrivacyViewAssessment,
    stream_capability: StreamCapability,
    still_capability: StillRefreshCapability,
    archive_capability: ArchiveCapability,
    terms: CameraSourceTerms,
    position: CameraHorizontalPosition,
    orientation: CameraViewingOrientation,
    prior_orientations: Vec<CameraViewingOrientation>,
    current_health: SourceLifecycleHealth,
    health_transitions: Vec<SourceHealthTransition>,
    ring_buffer: VecDeque<ClassifiedCameraFrame>,
    buffered_bytes: usize,
    eviction_summary: RingBufferEvictionSummary,
    discontinuity_spans: Vec<FrameDiscontinuitySpan>,
    clip_manifests: Vec<RetainedCameraClipManifest>,
    last_live_ingested_frame: Option<(i64, u64)>,
    last_live_capture_time_ms: Option<i64>,
    last_substitute_ingested_frame: Option<(i64, u64)>,
    last_archive_ingested_frame: Option<(i64, u64)>,
    last_periodic_still: Option<LastPeriodicStillTracker>,
    non_evidentiary_substitute_count: u64,
    stale_still_hold_count: u64,
}

impl RegisteredCameraEntry {
    #[must_use]
    pub fn camera_id(&self) -> &OpaqueId {
        &self.camera_id
    }

    #[must_use]
    pub fn upstream_feed_id(&self) -> &OpaqueId {
        &self.upstream_feed_id
    }

    #[must_use]
    pub fn source_version(&self) -> &VersionId {
        &self.source_version
    }

    #[must_use]
    pub fn prior_source_versions(&self) -> &[VersionId] {
        &self.prior_source_versions
    }

    #[must_use]
    pub const fn schema_version(&self) -> SchemaVersion {
        self.schema_version
    }

    #[must_use]
    pub fn registered_by(&self) -> &ActorId {
        &self.registered_by
    }

    #[must_use]
    pub const fn registered_at_ms(&self) -> i64 {
        self.registered_at_ms
    }

    #[must_use]
    pub fn provider_id(&self) -> &OpaqueId {
        &self.provider_id
    }

    #[must_use]
    pub fn provider_attribution(&self) -> &str {
        &self.provider_attribution
    }

    #[must_use]
    pub fn listing_reference(&self) -> &str {
        &self.listing_reference
    }

    #[must_use]
    pub const fn access_method(&self) -> OrdinaryAccessMethod {
        self.access_method
    }

    #[must_use]
    pub const fn intentional_publication(&self) -> IntentionalPublicationStatus {
        self.intentional_publication
    }

    #[must_use]
    pub const fn access_status(&self) -> CameraAccessStatus {
        self.access_status
    }

    #[must_use]
    pub const fn restricted_since_ms(&self) -> Option<i64> {
        self.restricted_since_ms
    }

    /// Recorded `(start_ms, end_ms)` restriction or policy-disabled intervals for this camera.
    #[must_use]
    pub fn restriction_intervals(&self) -> &[(i64, Option<i64>)] {
        &self.restriction_intervals
    }

    #[must_use]
    pub const fn view_kind(&self) -> CameraViewKind {
        self.view_kind
    }

    #[must_use]
    pub fn privacy_assessment(&self) -> &PrivacyViewAssessment {
        &self.privacy_assessment
    }

    #[must_use]
    pub const fn stream_capability(&self) -> StreamCapability {
        self.stream_capability
    }

    #[must_use]
    pub const fn still_capability(&self) -> StillRefreshCapability {
        self.still_capability
    }

    #[must_use]
    pub fn archive_capability(&self) -> &ArchiveCapability {
        &self.archive_capability
    }

    #[must_use]
    pub fn terms(&self) -> &CameraSourceTerms {
        &self.terms
    }

    #[must_use]
    pub fn position(&self) -> &CameraHorizontalPosition {
        &self.position
    }

    #[must_use]
    pub fn orientation(&self) -> &CameraViewingOrientation {
        &self.orientation
    }

    /// Historical orientations recorded before movement invalidation or recalibration (`CAM-05`).
    #[must_use]
    pub fn prior_orientations(&self) -> &[CameraViewingOrientation] {
        &self.prior_orientations
    }

    #[must_use]
    pub const fn current_health(&self) -> SourceLifecycleHealth {
        self.current_health
    }

    #[must_use]
    pub fn health_transitions(&self) -> &[SourceHealthTransition] {
        &self.health_transitions
    }

    #[must_use]
    pub fn buffered_frames(&self) -> &VecDeque<ClassifiedCameraFrame> {
        &self.ring_buffer
    }

    #[must_use]
    pub const fn buffered_bytes(&self) -> usize {
        self.buffered_bytes
    }

    #[must_use]
    pub fn eviction_summary(&self) -> &RingBufferEvictionSummary {
        &self.eviction_summary
    }

    #[must_use]
    pub fn discontinuity_spans(&self) -> &[FrameDiscontinuitySpan] {
        &self.discontinuity_spans
    }

    #[must_use]
    pub fn clip_manifests(&self) -> &[RetainedCameraClipManifest] {
        &self.clip_manifests
    }

    #[must_use]
    pub const fn non_evidentiary_substitute_count(&self) -> u64 {
        self.non_evidentiary_substitute_count
    }

    #[must_use]
    pub const fn stale_still_hold_count(&self) -> u64 {
        self.stale_still_hold_count
    }

    #[must_use]
    pub const fn directory_provenance_notice(&self) -> &'static str {
        DIRECTORY_LISTING_PROVENANCE_NOTICE
    }

    /// True if all privacy-mask requirements for the camera's view are satisfied (`CAM-01`, `CAM-02`).
    #[must_use]
    pub fn privacy_ready_for_acquisition(&self) -> bool {
        match &self.privacy_assessment {
            PrivacyViewAssessment::ClearPublicView { .. } => {
                self.view_kind != CameraViewKind::ResidentialExteriorStreetView
            }
            PrivacyViewAssessment::RequiresMaskZones { zones, .. } => {
                !zones.is_empty() && zones.iter().all(|z| z.mask_applied)
            }
            PrivacyViewAssessment::ExcludedPrivateActivity { .. } => false,
        }
    }

    /// Builds the explicit [`ContextWindowStatus::OmittedByRetentionPolicy`] context status for
    /// cameras where `clip_retention != Permitted` (`CAM-04`, `docs/architecture.md` §8).
    #[must_use]
    pub fn omitted_retention_context_status(&self) -> ContextWindowStatus {
        ContextWindowStatus::OmittedByRetentionPolicy {
            reason: format!(
                "clip_retention is `{}` under policy `{}`",
                self.terms.clip_retention, self.terms.policy_reference
            ),
        }
    }

    /// Exports the shared [`CameraSourceProfile`] for [`crate::camera_anomaly::CameraAnomalyMonitor`]
    /// (`AGENTS.md` §4, `CAM-01`, `CAM-02`).
    ///
    /// - Refuses cameras excluded for private activity (`PrivateActivityViewExcluded`) or with
    ///   unapplied privacy mask zones (`UnmaskedPrivacyZone`).
    /// - Refuses cameras in `SourceLifecycleHealth::DisabledByPolicy` (`CameraDisabledByPolicy`).
    /// - Refuses catalog-only cameras that declare neither live stream nor periodic still
    ///   capability (`CatalogOnlyCameraHasNoLiveFeed`) rather than mislabeling them as `StillFallback`.
    pub fn to_anomaly_source_profile(&self) -> Result<CameraSourceProfile, CameraDirectoryError> {
        match &self.privacy_assessment {
            PrivacyViewAssessment::ClearPublicView { .. } => {
                if self.view_kind == CameraViewKind::ResidentialExteriorStreetView {
                    return Err(CameraDirectoryError::ResidentialExteriorRequiresMaskZones {
                        camera_id: self.camera_id.clone(),
                    });
                }
            }
            PrivacyViewAssessment::RequiresMaskZones { zones, .. } => {
                for zone in zones {
                    if !zone.mask_applied {
                        return Err(CameraDirectoryError::UnmaskedPrivacyZone {
                            camera_id: self.camera_id.clone(),
                            zone_id: zone.zone_id.clone(),
                        });
                    }
                }
            }
            PrivacyViewAssessment::ExcludedPrivateActivity { reason, .. } => {
                return Err(CameraDirectoryError::PrivateActivityViewExcluded {
                    camera_id: self.camera_id.clone(),
                    reason: reason.clone(),
                });
            }
        }
        if self.current_health == SourceLifecycleHealth::DisabledByPolicy {
            return Err(CameraDirectoryError::CameraDisabledByPolicy {
                camera_id: self.camera_id.clone(),
            });
        }
        let feed_kind = match (self.stream_capability, self.still_capability) {
            (StreamCapability::LiveStreamSupported { .. }, _) => {
                FeedObservationKind::UpstreamCameraFeed
            }
            (StreamCapability::None, StillRefreshCapability::PeriodicStillSupported { .. }) => {
                if self.current_health == SourceLifecycleHealth::Stale {
                    FeedObservationKind::StaleStillHold
                } else {
                    FeedObservationKind::PeriodicUpstreamStill
                }
            }
            (StreamCapability::None, StillRefreshCapability::None) => {
                return Err(CameraDirectoryError::CatalogOnlyCameraHasNoLiveFeed {
                    camera_id: self.camera_id.clone(),
                });
            }
        };
        Ok(CameraSourceProfile {
            camera_id: self.camera_id.clone(),
            upstream_feed_id: self.upstream_feed_id.clone(),
            provider_attribution: self.provider_attribution.clone(),
            listing_reference: self.listing_reference.clone(),
            access_status: self.access_status,
            view_kind: self.view_kind,
            feed_kind,
            automated_processing: self.terms.automated_processing,
            clip_retention: self.terms.clip_retention,
            redistribution: self.terms.redistribution,
            max_retention_window_ms: self.terms.max_retention_window_ms,
            calibration: self.orientation.to_calibration_state(),
        })
    }
}

/// Errors returned by the camera directory, viewing-cone, frame-classification, and clip-retention kernel.
#[derive(Debug, Clone, PartialEq)]
pub enum CameraDirectoryError {
    InvalidId(IdError),
    ZeroLimit(CameraDirectoryLimitKind),
    LimitExceeded {
        kind: CameraDirectoryLimitKind,
        actual: usize,
        max: usize,
    },
    EmptyField {
        field: &'static str,
    },
    ControlCharacterInField {
        field: &'static str,
    },
    DuplicateCameraId(OpaqueId),
    UnknownCameraId(OpaqueId),
    DuplicateTransitionId(OpaqueId),
    DuplicateClipId(OpaqueId),
    DuplicateZoneId {
        camera_id: OpaqueId,
        zone_id: OpaqueId,
    },
    ReusedVersionId(VersionId),
    HomeInteriorExcluded {
        camera_id: OpaqueId,
        view_kind: CameraViewKind,
    },
    ResidentialExteriorRequiresMaskZones {
        camera_id: OpaqueId,
    },
    UnmaskedPrivacyZone {
        camera_id: OpaqueId,
        zone_id: OpaqueId,
    },
    PrivateActivityViewExcluded {
        camera_id: OpaqueId,
        reason: String,
    },
    AutomatedProcessingNotPermitted {
        camera_id: OpaqueId,
        status: SourceUsePermission,
    },
    CameraDisabledByPolicy {
        camera_id: OpaqueId,
    },
    CatalogOnlyCameraHasNoLiveFeed {
        camera_id: OpaqueId,
    },
    SourceRestricted {
        camera_id: OpaqueId,
        policy_notice: &'static str,
    },
    RestrictedSourceRequiresExplicitReregistration {
        camera_id: OpaqueId,
    },
    CameraNotRestricted {
        camera_id: OpaqueId,
    },
    ReregistrationTimestampRegression {
        camera_id: OpaqueId,
        previous_ms: i64,
        new_ms: i64,
    },
    InvalidCapabilityConfiguration {
        camera_id: OpaqueId,
        reason: &'static str,
    },
    InvalidRetentionWindowConfiguration {
        camera_id: OpaqueId,
        reason: &'static str,
    },
    InvalidCoordinates {
        camera_id: OpaqueId,
        reason: &'static str,
    },
    InvalidOrientation {
        camera_id: OpaqueId,
        reason: &'static str,
    },
    InvalidViewingConeRequest {
        reason: &'static str,
    },
    PositionUnavailableForViewingCone {
        camera_id: OpaqueId,
    },
    PolarApexUnsupportedForViewingCone {
        camera_id: OpaqueId,
        latitude_deg: f64,
    },
    OrientationUnavailableForViewingCone {
        camera_id: OpaqueId,
    },
    InvalidTimeInterval {
        start_ms: i64,
        end_ms: i64,
    },
    HealthTransitionTimestampRegression {
        camera_id: OpaqueId,
        previous_ms: i64,
        current_ms: i64,
    },
    InvalidSha256Hex {
        field: &'static str,
    },
    ZeroFrameByteLen {
        frame_version: VersionId,
    },
    FrameExceedsBufferByteLimit {
        frame_version: VersionId,
        byte_len: usize,
        max_buffered_bytes: usize,
    },
    GeneratedFrameForbiddenAsEvidence {
        frame_version: VersionId,
    },
    ClockUncertaintyExceedsMax {
        uncertainty_ms: u64,
        max_ms: u64,
    },
    OutOfOrderFrameIngestion {
        camera_id: OpaqueId,
        previous_ingestion_ms: i64,
        previous_sequence: u64,
        current_ingestion_ms: i64,
        current_sequence: u64,
    },
    OutOfOrderFrameCaptureTime {
        camera_id: OpaqueId,
        previous_event_time_ms: i64,
        current_event_time_ms: i64,
    },
    LiveFrameExceedsStalenessBound {
        camera_id: OpaqueId,
        frame_version: VersionId,
        capture_age_ms: u64,
        max_age_ms: u64,
    },
    UndeclaredStreamCapability {
        camera_id: OpaqueId,
    },
    UndeclaredStillCapability {
        camera_id: OpaqueId,
    },
    ArchiveFrameOutsideVerifiedWindow {
        camera_id: OpaqueId,
        frame_version: VersionId,
        reason: &'static str,
    },
    NonEvidentiaryFrameCannotBecomeEvidence {
        frame_version: VersionId,
        effective_kind: FeedObservationKind,
    },
    MissingSourceCaptureTimeForEvidence {
        frame_version: VersionId,
    },
    NoEvidentiaryFramesInEventWindow {
        camera_id: OpaqueId,
        event_start_ms: i64,
        event_end_ms: i64,
    },
    ClipRetentionNotPermitted {
        camera_id: OpaqueId,
        status: SourceUsePermission,
    },
}

impl fmt::Display for CameraDirectoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId(err) => write!(f, "invalid identifier: {err}"),
            Self::ZeroLimit(kind) => write!(f, "camera directory limit {kind} must be >= 1"),
            Self::LimitExceeded { kind, actual, max } => {
                write!(f, "{kind} count/size {actual} exceeds limit {max}")
            }
            Self::EmptyField { field } => write!(f, "field `{field}` must not be empty"),
            Self::ControlCharacterInField { field } => write!(
                f,
                "field `{field}` contains disallowed control, line-break, bidi, or zero-width characters"
            ),
            Self::DuplicateCameraId(id) => write!(f, "duplicate camera_id `{id}`"),
            Self::UnknownCameraId(id) => write!(f, "unknown camera_id `{id}`"),
            Self::DuplicateTransitionId(id) => write!(f, "duplicate transition_id `{id}`"),
            Self::DuplicateClipId(id) => write!(f, "duplicate clip_id `{id}`"),
            Self::DuplicateZoneId { camera_id, zone_id } => {
                write!(
                    f,
                    "duplicate privacy mask zone_id `{zone_id}` on camera `{camera_id}`"
                )
            }
            Self::ReusedVersionId(version) => write!(f, "version_id `{version}` cannot be reused"),
            Self::HomeInteriorExcluded {
                camera_id,
                view_kind,
            } => write!(
                f,
                "camera `{camera_id}` has excluded home-interior view `{view_kind:?}` under CAM-01"
            ),
            Self::ResidentialExteriorRequiresMaskZones { camera_id } => write!(
                f,
                "camera `{camera_id}` has ResidentialExteriorStreetView and requires declared privacy mask zones under CAM-02"
            ),
            Self::UnmaskedPrivacyZone { camera_id, zone_id } => write!(
                f,
                "camera `{camera_id}` privacy mask zone `{zone_id}` is not applied; acquisition is refused until masked"
            ),
            Self::PrivateActivityViewExcluded { camera_id, reason } => write!(
                f,
                "camera `{camera_id}` is excluded due to private activity in view: {reason}"
            ),
            Self::AutomatedProcessingNotPermitted { camera_id, status } => write!(
                f,
                "camera `{camera_id}` automated_processing is `{status}`; frame acquisition and automated monitoring are not permitted"
            ),
            Self::CameraDisabledByPolicy { camera_id } => write!(
                f,
                "camera `{camera_id}` lifecycle health is DisabledByPolicy; frame acquisition and monitoring are refused"
            ),
            Self::CatalogOnlyCameraHasNoLiveFeed { camera_id } => write!(
                f,
                "camera `{camera_id}` is a catalog-only entry with no declared live stream or periodic still capability"
            ),
            Self::SourceRestricted {
                camera_id,
                policy_notice,
            } => write!(f, "camera `{camera_id}` restricted: {policy_notice}"),
            Self::RestrictedSourceRequiresExplicitReregistration { camera_id } => write!(
                f,
                "camera `{camera_id}` is restricted; retries cannot re-admit it without an explicit new source_version registration"
            ),
            Self::CameraNotRestricted { camera_id } => write!(
                f,
                "camera `{camera_id}` is not currently in RestrictedOrUnavailable state"
            ),
            Self::ReregistrationTimestampRegression {
                camera_id,
                previous_ms,
                new_ms,
            } => write!(
                f,
                "camera `{camera_id}` re-registration timestamp {new_ms} must be > restricted/prior timestamp {previous_ms}"
            ),
            Self::InvalidCapabilityConfiguration { camera_id, reason } => write!(
                f,
                "camera `{camera_id}` has invalid capability configuration: {reason}"
            ),
            Self::InvalidRetentionWindowConfiguration { camera_id, reason } => write!(
                f,
                "camera `{camera_id}` has invalid retention window configuration: {reason}"
            ),
            Self::InvalidCoordinates { camera_id, reason } => {
                write!(f, "camera `{camera_id}` has invalid coordinates: {reason}")
            }
            Self::InvalidOrientation { camera_id, reason } => {
                write!(f, "camera `{camera_id}` has invalid orientation: {reason}")
            }
            Self::InvalidViewingConeRequest { reason } => {
                write!(f, "invalid viewing cone request: {reason}")
            }
            Self::PositionUnavailableForViewingCone { camera_id } => write!(
                f,
                "camera `{camera_id}` requires KnownWgs84 position to project a viewing cone"
            ),
            Self::PolarApexUnsupportedForViewingCone {
                camera_id,
                latitude_deg,
            } => write!(
                f,
                "camera `{camera_id}` latitude {latitude_deg} deg is at a geographic pole; horizontal compass bearings are degenerate"
            ),
            Self::OrientationUnavailableForViewingCone { camera_id } => write!(
                f,
                "camera `{camera_id}` orientation is unknown or invalidated by movement; cannot project a viewing cone"
            ),
            Self::InvalidTimeInterval { start_ms, end_ms } => write!(
                f,
                "invalid time interval [{start_ms}, {end_ms}): end_ms must be > start_ms"
            ),
            Self::HealthTransitionTimestampRegression {
                camera_id,
                previous_ms,
                current_ms,
            } => write!(
                f,
                "camera `{camera_id}` health transition at {current_ms} precedes prior transition at {previous_ms}"
            ),
            Self::InvalidSha256Hex { field } => write!(
                f,
                "field `{field}` must be a 64-character lowercase hexadecimal SHA-256 digest"
            ),
            Self::ZeroFrameByteLen { frame_version } => {
                write!(f, "frame `{frame_version}` byte_len must be >= 1")
            }
            Self::FrameExceedsBufferByteLimit {
                frame_version,
                byte_len,
                max_buffered_bytes,
            } => write!(
                f,
                "frame `{frame_version}` size {byte_len} bytes exceeds per-camera ring buffer limit {max_buffered_bytes}"
            ),
            Self::GeneratedFrameForbiddenAsEvidence { frame_version } => write!(
                f,
                "frame `{frame_version}` is a predictive or generated derivative and cannot enter camera observation records"
            ),
            Self::ClockUncertaintyExceedsMax {
                uncertainty_ms,
                max_ms,
            } => write!(
                f,
                "clock uncertainty {uncertainty_ms} ms exceeds maximum {max_ms} ms"
            ),
            Self::OutOfOrderFrameIngestion {
                camera_id,
                previous_ingestion_ms,
                previous_sequence,
                current_ingestion_ms,
                current_sequence,
            } => write!(
                f,
                "camera `{camera_id}` frame ({current_ingestion_ms} ms, seq {current_sequence}) is not strictly after ({previous_ingestion_ms} ms, seq {previous_sequence})"
            ),
            Self::OutOfOrderFrameCaptureTime {
                camera_id,
                previous_event_time_ms,
                current_event_time_ms,
            } => write!(
                f,
                "camera `{camera_id}` live/still frame capture time {current_event_time_ms} ms precedes prior capture time {previous_event_time_ms} ms"
            ),
            Self::LiveFrameExceedsStalenessBound {
                camera_id,
                frame_version,
                capture_age_ms,
                max_age_ms,
            } => write!(
                f,
                "camera `{camera_id}` live frame `{frame_version}` capture age {capture_age_ms} ms exceeds maximum allowed age {max_age_ms} ms"
            ),
            Self::UndeclaredStreamCapability { camera_id } => write!(
                f,
                "camera `{camera_id}` received UpstreamCameraFeed frame without declared LiveStreamSupported capability"
            ),
            Self::UndeclaredStillCapability { camera_id } => write!(
                f,
                "camera `{camera_id}` received PeriodicUpstreamStill frame without declared PeriodicStillSupported capability"
            ),
            Self::ArchiveFrameOutsideVerifiedWindow {
                camera_id,
                frame_version,
                reason,
            } => write!(
                f,
                "camera `{camera_id}` historical archive frame `{frame_version}` rejected: {reason}"
            ),
            Self::NonEvidentiaryFrameCannotBecomeEvidence {
                frame_version,
                effective_kind,
            } => write!(
                f,
                "frame `{frame_version}` has kind `{effective_kind}`, which cannot serve as event evidence"
            ),
            Self::MissingSourceCaptureTimeForEvidence { frame_version } => write!(
                f,
                "frame `{frame_version}` has no source_capture_time_ms; ingestion_time_ms must never be fabricated as a capture time"
            ),
            Self::NoEvidentiaryFramesInEventWindow {
                camera_id,
                event_start_ms,
                event_end_ms,
            } => write!(
                f,
                "camera `{camera_id}` has no qualifying evidentiary frames in event interval [{event_start_ms}, {event_end_ms})"
            ),
            Self::ClipRetentionNotPermitted { camera_id, status } => write!(
                f,
                "camera `{camera_id}` clip_retention is `{status}`; retained clip extraction is not permitted"
            ),
        }
    }
}

impl std::error::Error for CameraDirectoryError {}

impl From<IdError> for CameraDirectoryError {
    fn from(err: IdError) -> Self {
        Self::InvalidId(err)
    }
}

fn check_single_line_text(
    value: &str,
    field: &'static str,
    staged_text_bytes: &mut usize,
    limits: CameraDirectoryLimits,
) -> Result<(), CameraDirectoryError> {
    if value.trim().is_empty() {
        return Err(CameraDirectoryError::EmptyField { field });
    }
    if value.chars().any(is_disallowed_single_line_char) {
        return Err(CameraDirectoryError::ControlCharacterInField { field });
    }
    let next = staged_text_bytes.saturating_add(value.len());
    if next > limits.max_text_bytes {
        return Err(CameraDirectoryError::LimitExceeded {
            kind: CameraDirectoryLimitKind::TextBytes,
            actual: next,
            max: limits.max_text_bytes,
        });
    }
    *staged_text_bytes = next;
    Ok(())
}

fn validate_position(
    camera_id: &OpaqueId,
    position: &CameraHorizontalPosition,
    staged_text_bytes: &mut usize,
    limits: CameraDirectoryLimits,
) -> Result<(), CameraDirectoryError> {
    match position {
        CameraHorizontalPosition::KnownWgs84 {
            latitude_deg,
            longitude_deg,
            horizontal_uncertainty_m,
        } => {
            if !latitude_deg.is_finite() || !(-90.0..=90.0).contains(latitude_deg) {
                return Err(CameraDirectoryError::InvalidCoordinates {
                    camera_id: camera_id.clone(),
                    reason: "latitude_deg must be finite and within [-90.0, 90.0]",
                });
            }
            if !longitude_deg.is_finite() || !(-180.0..=180.0).contains(longitude_deg) {
                return Err(CameraDirectoryError::InvalidCoordinates {
                    camera_id: camera_id.clone(),
                    reason: "longitude_deg must be finite and within [-180.0, 180.0]",
                });
            }
            if !horizontal_uncertainty_m.is_finite() || *horizontal_uncertainty_m <= 0.0 {
                return Err(CameraDirectoryError::InvalidCoordinates {
                    camera_id: camera_id.clone(),
                    reason: "horizontal_uncertainty_m must be finite and > 0.0",
                });
            }
        }
        CameraHorizontalPosition::ApproximateGazetteerCentroid {
            locality_label,
            latitude_deg,
            longitude_deg,
            uncertainty_radius_m,
        } => {
            check_single_line_text(
                locality_label,
                "position.locality_label",
                staged_text_bytes,
                limits,
            )?;
            if !latitude_deg.is_finite() || !(-90.0..=90.0).contains(latitude_deg) {
                return Err(CameraDirectoryError::InvalidCoordinates {
                    camera_id: camera_id.clone(),
                    reason: "latitude_deg must be finite and within [-90.0, 90.0]",
                });
            }
            if !longitude_deg.is_finite() || !(-180.0..=180.0).contains(longitude_deg) {
                return Err(CameraDirectoryError::InvalidCoordinates {
                    camera_id: camera_id.clone(),
                    reason: "longitude_deg must be finite and within [-180.0, 180.0]",
                });
            }
            if !uncertainty_radius_m.is_finite() || *uncertainty_radius_m <= 0.0 {
                return Err(CameraDirectoryError::InvalidCoordinates {
                    camera_id: camera_id.clone(),
                    reason: "uncertainty_radius_m must be finite and > 0.0",
                });
            }
        }
        CameraHorizontalPosition::Unknown { reason } => {
            check_single_line_text(reason, "position.reason", staged_text_bytes, limits)?;
        }
    }
    Ok(())
}

fn validate_orientation(
    camera_id: &OpaqueId,
    orientation: &CameraViewingOrientation,
    staged_text_bytes: &mut usize,
    limits: CameraDirectoryLimits,
) -> Result<(), CameraDirectoryError> {
    match orientation {
        CameraViewingOrientation::Calibrated {
            azimuth_deg,
            pitch_deg,
            roll_deg,
            horizontal_fov_deg,
            vertical_fov_deg,
            max_angular_error_mdeg,
            ..
        } => {
            if !azimuth_deg.is_finite() || !(0.0..360.0).contains(azimuth_deg) {
                return Err(CameraDirectoryError::InvalidOrientation {
                    camera_id: camera_id.clone(),
                    reason: "azimuth_deg must be finite and within [0.0, 360.0)",
                });
            }
            if !horizontal_fov_deg.is_finite()
                || *horizontal_fov_deg <= 0.0
                || *horizontal_fov_deg >= 360.0
            {
                return Err(CameraDirectoryError::InvalidOrientation {
                    camera_id: camera_id.clone(),
                    reason: "horizontal_fov_deg must be finite and within (0.0, 360.0)",
                });
            }
            if let Some(pitch) = pitch_deg
                && (!pitch.is_finite() || !(-90.0..=90.0).contains(pitch))
            {
                return Err(CameraDirectoryError::InvalidOrientation {
                    camera_id: camera_id.clone(),
                    reason: "pitch_deg must be finite and within [-90.0, 90.0]",
                });
            }
            if let Some(roll) = roll_deg
                && (!roll.is_finite() || !(-180.0..=180.0).contains(roll))
            {
                return Err(CameraDirectoryError::InvalidOrientation {
                    camera_id: camera_id.clone(),
                    reason: "roll_deg must be finite and within [-180.0, 180.0]",
                });
            }
            if let Some(vfov) = vertical_fov_deg
                && (!vfov.is_finite() || *vfov <= 0.0 || *vfov > 180.0)
            {
                return Err(CameraDirectoryError::InvalidOrientation {
                    camera_id: camera_id.clone(),
                    reason: "vertical_fov_deg must be finite and within (0.0, 180.0]",
                });
            }
            if *max_angular_error_mdeg == 0 || *max_angular_error_mdeg > 180_000 {
                return Err(CameraDirectoryError::InvalidOrientation {
                    camera_id: camera_id.clone(),
                    reason: "max_angular_error_mdeg must be within 1..=180_000",
                });
            }
        }
        CameraViewingOrientation::ReportedApproximate {
            azimuth_deg,
            horizontal_fov_deg,
            bearing_uncertainty_deg,
        } => {
            if !azimuth_deg.is_finite() || !(0.0..360.0).contains(azimuth_deg) {
                return Err(CameraDirectoryError::InvalidOrientation {
                    camera_id: camera_id.clone(),
                    reason: "azimuth_deg must be finite and within [0.0, 360.0)",
                });
            }
            if !horizontal_fov_deg.is_finite()
                || *horizontal_fov_deg <= 0.0
                || *horizontal_fov_deg >= 360.0
            {
                return Err(CameraDirectoryError::InvalidOrientation {
                    camera_id: camera_id.clone(),
                    reason: "horizontal_fov_deg must be finite and within (0.0, 360.0)",
                });
            }
            if !bearing_uncertainty_deg.is_finite()
                || *bearing_uncertainty_deg <= 0.0
                || *bearing_uncertainty_deg > 180.0
            {
                return Err(CameraDirectoryError::InvalidOrientation {
                    camera_id: camera_id.clone(),
                    reason: "bearing_uncertainty_deg must be finite and within (0.0, 180.0]",
                });
            }
        }
        CameraViewingOrientation::InvalidatedByMovement {
            last_known_azimuth_deg,
            reason,
            ..
        } => {
            if let Some(azimuth) = last_known_azimuth_deg
                && (!azimuth.is_finite() || !(0.0..360.0).contains(azimuth))
            {
                return Err(CameraDirectoryError::InvalidOrientation {
                    camera_id: camera_id.clone(),
                    reason: "last_known_azimuth_deg must be finite and within [0.0, 360.0)",
                });
            }
            check_single_line_text(reason, "orientation.reason", staged_text_bytes, limits)?;
        }
        CameraViewingOrientation::Unknown { reason } => {
            check_single_line_text(reason, "orientation.reason", staged_text_bytes, limits)?;
        }
    }
    Ok(())
}

/// Great-circle forward destination point on the mean sphere (`EARTH_MEAN_RADIUS_M`)
/// from `(lat_deg, lon_deg)` along `bearing_deg` for `distance_m`.
fn project_spherical_point(
    lat_deg: f64,
    lon_deg: f64,
    bearing_deg: f64,
    distance_m: f64,
) -> (f64, f64) {
    let angular_distance = distance_m / EARTH_MEAN_RADIUS_M;
    let lat1 = lat_deg.to_radians();
    let lon1 = lon_deg.to_radians();
    let brng = bearing_deg.to_radians();

    let sin_lat1 = lat1.sin();
    let cos_lat1 = lat1.cos();
    let sin_d = angular_distance.sin();
    let cos_d = angular_distance.cos();

    let sin_lat2 = (sin_lat1 * cos_d + cos_lat1 * sin_d * brng.cos()).clamp(-1.0, 1.0);
    let lat2 = sin_lat2.asin();
    let y = brng.sin() * sin_d * cos_lat1;
    let x = cos_d - sin_lat1 * sin_lat2;
    let lon2 = lon1 + y.atan2(x);

    let mut out_lon_deg = (lon2.to_degrees() + 540.0) % 360.0 - 180.0;
    if out_lon_deg == -180.0 && lon_deg > 0.0 {
        out_lon_deg = 180.0;
    }
    (lat2.to_degrees(), out_lon_deg)
}

/// Bounded in-memory registry and retention ring-buffer manager for ordinarily accessible
/// camera sources (`#28`, `CAM-01`, `CAM-02`, `CAM-04`, `A09`).
#[derive(Debug, Clone)]
pub struct CameraDirectoryRegistry {
    limits: CameraDirectoryLimits,
    cameras: BTreeMap<OpaqueId, RegisteredCameraEntry>,
    seen_versions: BTreeSet<VersionId>,
    seen_transition_ids: BTreeSet<OpaqueId>,
    seen_clip_ids: BTreeSet<OpaqueId>,
    used_text_bytes: usize,
}

impl CameraDirectoryRegistry {
    /// Creates an empty camera directory registry governed by `limits`.
    #[must_use]
    pub fn new(limits: CameraDirectoryLimits) -> Self {
        Self {
            limits,
            cameras: BTreeMap::new(),
            seen_versions: BTreeSet::new(),
            seen_transition_ids: BTreeSet::new(),
            seen_clip_ids: BTreeSet::new(),
            used_text_bytes: 0,
        }
    }

    #[must_use]
    pub const fn limits(&self) -> CameraDirectoryLimits {
        self.limits
    }

    #[must_use]
    pub fn camera(&self, camera_id: &OpaqueId) -> Option<&RegisteredCameraEntry> {
        self.cameras.get(camera_id)
    }

    #[must_use]
    pub fn cameras(&self) -> &BTreeMap<OpaqueId, RegisteredCameraEntry> {
        &self.cameras
    }

    /// Registers a camera source in the directory (`CAM-01`, `CAM-02`, `docs/architecture.md` §4.4).
    ///
    /// - Home interiors (`HomeInterior`, `ViewIntoHomeInterior`) are unconditionally refused.
    /// - `IntentionalPublicationStatus::NotEstablishedOrdinaryAccess` is recorded as provenance
    ///   metadata and never refuses or degrades an ordinarily accessible camera (`CAM-01`).
    /// - A camera can be cataloged even when `automated_processing` or `clip_retention` is
    ///   `Unknown` or `Prohibited`, or when no live stream / archive is available.
    pub fn register_camera(
        &mut self,
        input: CameraDirectoryRegistrationInput,
    ) -> Result<&RegisteredCameraEntry, CameraDirectoryError> {
        if self.cameras.contains_key(&input.camera_id) {
            return Err(CameraDirectoryError::DuplicateCameraId(input.camera_id));
        }
        let next_camera_count = self.cameras.len().saturating_add(1);
        if next_camera_count > self.limits.max_registered_cameras {
            return Err(CameraDirectoryError::LimitExceeded {
                kind: CameraDirectoryLimitKind::RegisteredCameras,
                actual: next_camera_count,
                max: self.limits.max_registered_cameras,
            });
        }
        if matches!(
            input.view_kind,
            CameraViewKind::HomeInterior | CameraViewKind::ViewIntoHomeInterior
        ) {
            return Err(CameraDirectoryError::HomeInteriorExcluded {
                camera_id: input.camera_id,
                view_kind: input.view_kind,
            });
        }
        if self.seen_versions.contains(&input.source_version) {
            return Err(CameraDirectoryError::ReusedVersionId(input.source_version));
        }
        if let ArchiveCapability::VerifiedArchiveWindow {
            ref archive_version,
            earliest_capture_ms,
            latest_capture_ms,
        } = input.archive_capability
        {
            if archive_version == &input.source_version
                || self.seen_versions.contains(archive_version)
            {
                return Err(CameraDirectoryError::ReusedVersionId(
                    archive_version.clone(),
                ));
            }
            if latest_capture_ms <= earliest_capture_ms {
                return Err(CameraDirectoryError::InvalidTimeInterval {
                    start_ms: earliest_capture_ms,
                    end_ms: latest_capture_ms,
                });
            }
        }
        if let CameraViewingOrientation::Calibrated {
            ref calibration_version,
            ..
        } = input.orientation
        {
            let collides_with_archive = matches!(
                input.archive_capability,
                ArchiveCapability::VerifiedArchiveWindow {
                    ref archive_version,
                    ..
                } if archive_version == calibration_version
            );
            if calibration_version == &input.source_version
                || collides_with_archive
                || self.seen_versions.contains(calibration_version)
            {
                return Err(CameraDirectoryError::ReusedVersionId(
                    calibration_version.clone(),
                ));
            }
        }

        if let StreamCapability::LiveStreamSupported {
            nominal_frame_interval_ms,
        } = input.stream_capability
            && (nominal_frame_interval_ms == 0
                || nominal_frame_interval_ms > MAX_CLOCK_UNCERTAINTY_MS)
        {
            return Err(CameraDirectoryError::InvalidCapabilityConfiguration {
                camera_id: input.camera_id,
                reason: "LiveStreamSupported nominal_frame_interval_ms must be within 1..=MAX_CLOCK_UNCERTAINTY_MS",
            });
        }
        if let StillRefreshCapability::PeriodicStillSupported {
            nominal_refresh_interval_ms,
            stale_after_ms,
        } = input.still_capability
        {
            if nominal_refresh_interval_ms == 0
                || nominal_refresh_interval_ms > MAX_CLOCK_UNCERTAINTY_MS
            {
                return Err(CameraDirectoryError::InvalidCapabilityConfiguration {
                    camera_id: input.camera_id,
                    reason: "PeriodicStillSupported nominal_refresh_interval_ms must be within 1..=MAX_CLOCK_UNCERTAINTY_MS",
                });
            }
            if stale_after_ms < nominal_refresh_interval_ms
                || stale_after_ms > MAX_CLOCK_UNCERTAINTY_MS
            {
                return Err(CameraDirectoryError::InvalidCapabilityConfiguration {
                    camera_id: input.camera_id,
                    reason: "PeriodicStillSupported stale_after_ms must be within nominal_refresh_interval_ms..=MAX_CLOCK_UNCERTAINTY_MS",
                });
            }
        }

        match (
            input.terms.clip_retention,
            input.terms.max_retention_window_ms,
        ) {
            (SourceUsePermission::Permitted, Some(0)) | (SourceUsePermission::Permitted, None) => {
                return Err(CameraDirectoryError::InvalidRetentionWindowConfiguration {
                    camera_id: input.camera_id,
                    reason: "clip_retention == Permitted requires max_retention_window_ms >= 1",
                });
            }
            (SourceUsePermission::Prohibited | SourceUsePermission::Unknown, Some(_)) => {
                return Err(CameraDirectoryError::InvalidRetentionWindowConfiguration {
                    camera_id: input.camera_id,
                    reason: "max_retention_window_ms must be None when clip_retention is not Permitted",
                });
            }
            _ => {}
        }

        let mut staged_text_bytes = self.used_text_bytes;
        check_single_line_text(
            &input.provider_attribution,
            "provider_attribution",
            &mut staged_text_bytes,
            self.limits,
        )?;
        check_single_line_text(
            &input.listing_reference,
            "listing_reference",
            &mut staged_text_bytes,
            self.limits,
        )?;
        check_single_line_text(
            &input.terms.policy_reference,
            "terms.policy_reference",
            &mut staged_text_bytes,
            self.limits,
        )?;

        match &input.privacy_assessment {
            PrivacyViewAssessment::ClearPublicView { .. } => {
                if input.view_kind == CameraViewKind::ResidentialExteriorStreetView {
                    return Err(CameraDirectoryError::ResidentialExteriorRequiresMaskZones {
                        camera_id: input.camera_id,
                    });
                }
            }
            PrivacyViewAssessment::RequiresMaskZones { zones, .. } => {
                if zones.is_empty() {
                    return Err(CameraDirectoryError::EmptyField {
                        field: "privacy_assessment.zones",
                    });
                }
                if zones.len() > self.limits.max_privacy_mask_zones_per_camera {
                    return Err(CameraDirectoryError::LimitExceeded {
                        kind: CameraDirectoryLimitKind::PrivacyMaskZonesPerCamera,
                        actual: zones.len(),
                        max: self.limits.max_privacy_mask_zones_per_camera,
                    });
                }
                let mut seen_zones = BTreeSet::new();
                for zone in zones {
                    if !seen_zones.insert(zone.zone_id.clone()) {
                        return Err(CameraDirectoryError::DuplicateZoneId {
                            camera_id: input.camera_id,
                            zone_id: zone.zone_id.clone(),
                        });
                    }
                    check_single_line_text(
                        &zone.label,
                        "privacy_assessment.zone.label",
                        &mut staged_text_bytes,
                        self.limits,
                    )?;
                }
            }
            PrivacyViewAssessment::ExcludedPrivateActivity { reason, .. } => {
                check_single_line_text(
                    reason,
                    "privacy_assessment.reason",
                    &mut staged_text_bytes,
                    self.limits,
                )?;
            }
        }

        validate_position(
            &input.camera_id,
            &input.position,
            &mut staged_text_bytes,
            self.limits,
        )?;
        validate_orientation(
            &input.camera_id,
            &input.orientation,
            &mut staged_text_bytes,
            self.limits,
        )?;

        let (access_status, restricted_since_ms, restriction_intervals) = match input.initial_health
        {
            SourceLifecycleHealth::UnavailableRestricted => (
                CameraAccessStatus::RestrictedOrUnavailable,
                Some(input.registered_at_ms),
                vec![(input.registered_at_ms, None)],
            ),
            SourceLifecycleHealth::DisabledByPolicy => (
                CameraAccessStatus::OrdinarilyAccessible,
                None,
                vec![(input.registered_at_ms, None)],
            ),
            _ => (CameraAccessStatus::OrdinarilyAccessible, None, Vec::new()),
        };

        self.seen_versions.insert(input.source_version.clone());
        if let ArchiveCapability::VerifiedArchiveWindow {
            ref archive_version,
            ..
        } = input.archive_capability
        {
            self.seen_versions.insert(archive_version.clone());
        }
        if let CameraViewingOrientation::Calibrated {
            ref calibration_version,
            ..
        } = input.orientation
        {
            self.seen_versions.insert(calibration_version.clone());
        }
        self.used_text_bytes = staged_text_bytes;

        let camera_id = input.camera_id.clone();
        let entry = RegisteredCameraEntry {
            camera_id: input.camera_id,
            upstream_feed_id: input.upstream_feed_id,
            source_version: input.source_version,
            prior_source_versions: Vec::new(),
            schema_version: input.schema_version,
            registered_by: input.registered_by,
            registered_at_ms: input.registered_at_ms,
            provider_id: input.provider_id,
            provider_attribution: input.provider_attribution,
            listing_reference: input.listing_reference,
            access_method: input.access_method,
            intentional_publication: input.intentional_publication,
            access_status,
            restricted_since_ms,
            restriction_intervals,
            view_kind: input.view_kind,
            privacy_assessment: input.privacy_assessment,
            stream_capability: input.stream_capability,
            still_capability: input.still_capability,
            archive_capability: input.archive_capability,
            terms: input.terms,
            position: input.position,
            orientation: input.orientation,
            prior_orientations: Vec::new(),
            current_health: input.initial_health,
            health_transitions: Vec::new(),
            ring_buffer: VecDeque::new(),
            buffered_bytes: 0,
            eviction_summary: RingBufferEvictionSummary::default(),
            discontinuity_spans: Vec::new(),
            clip_manifests: Vec::new(),
            last_live_ingested_frame: None,
            last_live_capture_time_ms: None,
            last_substitute_ingested_frame: None,
            last_archive_ingested_frame: None,
            last_periodic_still: None,
            non_evidentiary_substitute_count: 0,
            stale_still_hold_count: 0,
        };
        self.cameras.insert(camera_id.clone(), entry);
        self.cameras
            .get(&camera_id)
            .ok_or(CameraDirectoryError::UnknownCameraId(camera_id))
    }

    /// Updates a camera's viewing orientation with a new calibrated or reported orientation,
    /// e.g. after re-calibrating a repositioned camera (`CAM-02`, `CAM-05`), while preserving
    /// the prior orientation in [`RegisteredCameraEntry::prior_orientations`] (bounded by
    /// `max_health_transitions_per_camera`).
    pub fn update_camera_orientation(
        &mut self,
        camera_id: &OpaqueId,
        new_orientation: CameraViewingOrientation,
    ) -> Result<&RegisteredCameraEntry, CameraDirectoryError> {
        let entry = self
            .cameras
            .get(camera_id)
            .ok_or_else(|| CameraDirectoryError::UnknownCameraId(camera_id.clone()))?;
        let next_prior_orientations = entry.prior_orientations.len().saturating_add(1);
        if next_prior_orientations > self.limits.max_health_transitions_per_camera {
            return Err(CameraDirectoryError::LimitExceeded {
                kind: CameraDirectoryLimitKind::HealthTransitionsPerCamera,
                actual: next_prior_orientations,
                max: self.limits.max_health_transitions_per_camera,
            });
        }
        if let CameraViewingOrientation::Calibrated {
            ref calibration_version,
            ..
        } = new_orientation
            && self.seen_versions.contains(calibration_version)
        {
            return Err(CameraDirectoryError::ReusedVersionId(
                calibration_version.clone(),
            ));
        }
        let mut staged_text_bytes = self.used_text_bytes;
        validate_orientation(
            camera_id,
            &new_orientation,
            &mut staged_text_bytes,
            self.limits,
        )?;
        if let CameraViewingOrientation::Calibrated {
            ref calibration_version,
            ..
        } = new_orientation
        {
            self.seen_versions.insert(calibration_version.clone());
        }
        self.used_text_bytes = staged_text_bytes;
        let entry = self
            .cameras
            .get_mut(camera_id)
            .ok_or_else(|| CameraDirectoryError::UnknownCameraId(camera_id.clone()))?;
        let old_orientation = std::mem::replace(&mut entry.orientation, new_orientation);
        entry.prior_orientations.push(old_orientation);
        Ok(entry)
    }

    /// Records a source lifecycle health or camera-fault transition (`CAM-01`, `CAM-03`, `A09`,
    /// `docs/architecture.md` §4.3).
    ///
    /// - If the camera is already `RestrictedOrUnavailable`, any transition attempt (including
    ///   retries) is refused with [`CameraDirectoryError::RestrictedSourceRequiresExplicitReregistration`].
    /// - Recording [`SourceLifecycleHealth::UnavailableRestricted`] or
    ///   `Some(CameraHealthState::SourceRestricted)` permanently moves `access_status` to
    ///   [`CameraAccessStatus::RestrictedOrUnavailable`] and halts acquisition.
    /// - Recording `Some(CameraHealthState::CameraRepositioned)` automatically invalidates
    ///   any active calibrated or reported viewing orientation (`CAM-05`, `A09`).
    pub fn record_health_transition(
        &mut self,
        camera_id: &OpaqueId,
        transition_id: OpaqueId,
        to_state: SourceLifecycleHealth,
        fault_state: Option<CameraHealthState>,
        occurred_at_ms: i64,
        detail: String,
    ) -> Result<&SourceHealthTransition, CameraDirectoryError> {
        if self.seen_transition_ids.contains(&transition_id) {
            return Err(CameraDirectoryError::DuplicateTransitionId(transition_id));
        }
        let entry = self
            .cameras
            .get(camera_id)
            .ok_or_else(|| CameraDirectoryError::UnknownCameraId(camera_id.clone()))?;

        if entry.access_status == CameraAccessStatus::RestrictedOrUnavailable {
            return Err(
                CameraDirectoryError::RestrictedSourceRequiresExplicitReregistration {
                    camera_id: camera_id.clone(),
                },
            );
        }
        if let Some(prev) = entry.health_transitions.last()
            && occurred_at_ms < prev.occurred_at_ms
        {
            return Err(CameraDirectoryError::HealthTransitionTimestampRegression {
                camera_id: camera_id.clone(),
                previous_ms: prev.occurred_at_ms,
                current_ms: occurred_at_ms,
            });
        }
        let next_transitions = entry.health_transitions.len().saturating_add(1);
        if next_transitions > self.limits.max_health_transitions_per_camera {
            return Err(CameraDirectoryError::LimitExceeded {
                kind: CameraDirectoryLimitKind::HealthTransitionsPerCamera,
                actual: next_transitions,
                max: self.limits.max_health_transitions_per_camera,
            });
        }
        if fault_state == Some(CameraHealthState::CameraRepositioned) {
            let next_prior_orientations = entry.prior_orientations.len().saturating_add(1);
            if next_prior_orientations > self.limits.max_health_transitions_per_camera {
                return Err(CameraDirectoryError::LimitExceeded {
                    kind: CameraDirectoryLimitKind::HealthTransitionsPerCamera,
                    actual: next_prior_orientations,
                    max: self.limits.max_health_transitions_per_camera,
                });
            }
        }

        let mut staged_text_bytes = self.used_text_bytes;
        check_single_line_text(
            &detail,
            "health_transition.detail",
            &mut staged_text_bytes,
            self.limits,
        )?;

        let effective_to_state = if fault_state == Some(CameraHealthState::SourceRestricted) {
            SourceLifecycleHealth::UnavailableRestricted
        } else {
            to_state
        };

        let entry = self
            .cameras
            .get_mut(camera_id)
            .ok_or_else(|| CameraDirectoryError::UnknownCameraId(camera_id.clone()))?;

        if fault_state == Some(CameraHealthState::CameraRepositioned) {
            let (prior_cal, last_az) = match &entry.orientation {
                CameraViewingOrientation::Calibrated {
                    calibration_version,
                    azimuth_deg,
                    ..
                } => (Some(calibration_version.clone()), Some(*azimuth_deg)),
                CameraViewingOrientation::ReportedApproximate { azimuth_deg, .. } => {
                    (None, Some(*azimuth_deg))
                }
                CameraViewingOrientation::InvalidatedByMovement {
                    prior_calibration_version,
                    last_known_azimuth_deg,
                    ..
                } => (prior_calibration_version.clone(), *last_known_azimuth_deg),
                CameraViewingOrientation::Unknown { .. } => (None, None),
            };
            let old_orientation = std::mem::replace(
                &mut entry.orientation,
                CameraViewingOrientation::InvalidatedByMovement {
                    prior_calibration_version: prior_cal,
                    last_known_azimuth_deg: last_az,
                    detected_at_ms: occurred_at_ms,
                    reason: detail.clone(),
                },
            );
            entry.prior_orientations.push(old_orientation);
        }

        let from_state = entry.current_health;
        if from_state == SourceLifecycleHealth::DisabledByPolicy
            && effective_to_state != SourceLifecycleHealth::DisabledByPolicy
            && let Some((_, end_slot)) = entry.restriction_intervals.last_mut()
            && end_slot.is_none()
        {
            *end_slot = Some(occurred_at_ms);
        }
        if effective_to_state == SourceLifecycleHealth::UnavailableRestricted {
            entry.access_status = CameraAccessStatus::RestrictedOrUnavailable;
            entry.restricted_since_ms = Some(occurred_at_ms);
            entry.restriction_intervals.push((occurred_at_ms, None));
        } else if effective_to_state == SourceLifecycleHealth::DisabledByPolicy
            && from_state != SourceLifecycleHealth::DisabledByPolicy
        {
            entry.restriction_intervals.push((occurred_at_ms, None));
        }

        entry.current_health = effective_to_state;
        entry.health_transitions.push(SourceHealthTransition {
            transition_id: transition_id.clone(),
            from_state,
            to_state: effective_to_state,
            occurred_at_ms,
            fault_state,
            detail,
        });
        self.seen_transition_ids.insert(transition_id);
        self.used_text_bytes = staged_text_bytes;

        entry
            .health_transitions
            .last()
            .ok_or_else(|| CameraDirectoryError::UnknownCameraId(camera_id.clone()))
    }

    /// Explicitly re-admits a previously restricted camera on the **same** `listing_reference`
    /// only when the operator supplies a new `new_source_version` after verifying that the original
    /// listing has returned to ordinary public access (`CAM-01`, `AGENTS.md` §6, `A09`).
    ///
    /// - Re-admission never switches `listing_reference` to an alternate endpoint; a different
    ///   camera endpoint is a separate [`Self::register_camera`] entry with its own access and
    ///   privacy assessment.
    /// - Resets live sequence/still session state for the new `source_version` while preserving
    ///   prior `source_version`s, health transitions, restriction intervals, and per-frame `source_version` provenance.
    pub fn reregister_ordinarily_accessible_version(
        &mut self,
        camera_id: &OpaqueId,
        transition_id: OpaqueId,
        new_source_version: VersionId,
        reregistered_at_ms: i64,
        rationale: String,
    ) -> Result<&RegisteredCameraEntry, CameraDirectoryError> {
        if self.seen_versions.contains(&new_source_version) {
            return Err(CameraDirectoryError::ReusedVersionId(new_source_version));
        }
        if self.seen_transition_ids.contains(&transition_id) {
            return Err(CameraDirectoryError::DuplicateTransitionId(transition_id));
        }
        let entry = self
            .cameras
            .get(camera_id)
            .ok_or_else(|| CameraDirectoryError::UnknownCameraId(camera_id.clone()))?;
        if entry.access_status != CameraAccessStatus::RestrictedOrUnavailable {
            return Err(CameraDirectoryError::CameraNotRestricted {
                camera_id: camera_id.clone(),
            });
        }
        let min_after_ms = entry.restricted_since_ms.unwrap_or(entry.registered_at_ms);
        if reregistered_at_ms <= min_after_ms {
            return Err(CameraDirectoryError::ReregistrationTimestampRegression {
                camera_id: camera_id.clone(),
                previous_ms: min_after_ms,
                new_ms: reregistered_at_ms,
            });
        }
        let next_transitions = entry.health_transitions.len().saturating_add(1);
        if next_transitions > self.limits.max_health_transitions_per_camera {
            return Err(CameraDirectoryError::LimitExceeded {
                kind: CameraDirectoryLimitKind::HealthTransitionsPerCamera,
                actual: next_transitions,
                max: self.limits.max_health_transitions_per_camera,
            });
        }

        let mut staged_text_bytes = self.used_text_bytes;
        check_single_line_text(
            &rationale,
            "reregistration.rationale",
            &mut staged_text_bytes,
            self.limits,
        )?;

        let entry = self
            .cameras
            .get_mut(camera_id)
            .ok_or_else(|| CameraDirectoryError::UnknownCameraId(camera_id.clone()))?;
        let old_version = std::mem::replace(&mut entry.source_version, new_source_version.clone());
        entry.prior_source_versions.push(old_version);
        entry.access_status = CameraAccessStatus::OrdinarilyAccessible;
        entry.restricted_since_ms = None;
        if let Some((_, end_slot)) = entry.restriction_intervals.last_mut()
            && end_slot.is_none()
        {
            *end_slot = Some(reregistered_at_ms);
        }
        entry.last_live_ingested_frame = None;
        entry.last_live_capture_time_ms = None;
        entry.last_substitute_ingested_frame = None;
        entry.last_periodic_still = None;
        let from_state = entry.current_health;
        entry.current_health = SourceLifecycleHealth::Available;
        entry.health_transitions.push(SourceHealthTransition {
            transition_id: transition_id.clone(),
            from_state,
            to_state: SourceLifecycleHealth::Available,
            occurred_at_ms: reregistered_at_ms,
            fault_state: None,
            detail: rationale,
        });

        self.seen_versions.insert(new_source_version);
        self.seen_transition_ids.insert(transition_id);
        self.used_text_bytes = staged_text_bytes;
        Ok(entry)
    }

    /// Projects a horizontal viewing cone on the WGS84 sphere for a cataloged camera (`CAM-02`,
    /// `docs/architecture.md` §§3.2, 8).
    ///
    /// - Validates `range_m` and `arc_vertex_count` before allocating.
    /// - Requires [`CameraHorizontalPosition::KnownWgs84`] with `|latitude_deg| <= MAX_NON_POLAR_CONE_LATITUDE_DEG`;
    ///   approximate centroids, unknown positions, and degenerate geographic-pole apexes (`±90°`) are refused.
    /// - Separates [`ViewingConeClass::CalibratedFieldOfView`] from
    ///   [`ViewingConeClass::IllustrativeReportedBearing`], projects both nominal `arc_vertices_lat_lon_deg`
    ///   and `uncertainty_widened_arc_vertices_lat_lon_deg` (widened by calibration/bearing angular uncertainty
    ///   plus `atan2(horizontal_uncertainty_m, range_m)`), and refuses unknown or movement-invalidated orientations.
    pub fn compute_viewing_cone(
        &self,
        camera_id: &OpaqueId,
        range_m: f64,
        arc_vertex_count: usize,
    ) -> Result<ViewingConeGeometry, CameraDirectoryError> {
        if !range_m.is_finite() || range_m <= 0.0 || range_m > MAX_VIEWING_CONE_RANGE_M {
            return Err(CameraDirectoryError::InvalidViewingConeRequest {
                reason: "range_m must be finite and within (0.0, MAX_VIEWING_CONE_RANGE_M]",
            });
        }
        if !(MIN_VIEWING_CONE_ARC_VERTICES..=MAX_VIEWING_CONE_ARC_VERTICES)
            .contains(&arc_vertex_count)
        {
            return Err(CameraDirectoryError::InvalidViewingConeRequest {
                reason: "arc_vertex_count must be within MIN_VIEWING_CONE_ARC_VERTICES..=MAX_VIEWING_CONE_ARC_VERTICES",
            });
        }
        let entry = self
            .cameras
            .get(camera_id)
            .ok_or_else(|| CameraDirectoryError::UnknownCameraId(camera_id.clone()))?;

        let (apex_latitude_deg, apex_longitude_deg, horizontal_uncertainty_m) = match entry.position
        {
            CameraHorizontalPosition::KnownWgs84 {
                latitude_deg,
                longitude_deg,
                horizontal_uncertainty_m,
            } => (latitude_deg, longitude_deg, horizontal_uncertainty_m),
            CameraHorizontalPosition::ApproximateGazetteerCentroid { .. }
            | CameraHorizontalPosition::Unknown { .. } => {
                return Err(CameraDirectoryError::PositionUnavailableForViewingCone {
                    camera_id: camera_id.clone(),
                });
            }
        };

        if apex_latitude_deg.abs() > MAX_NON_POLAR_CONE_LATITUDE_DEG {
            return Err(CameraDirectoryError::PolarApexUnsupportedForViewingCone {
                camera_id: camera_id.clone(),
                latitude_deg: apex_latitude_deg,
            });
        }

        let (cone_class, azimuth_deg, horizontal_fov_deg, angular_uncertainty_deg) =
            match &entry.orientation {
                CameraViewingOrientation::Calibrated {
                    calibration_version,
                    azimuth_deg,
                    horizontal_fov_deg,
                    max_angular_error_mdeg,
                    ..
                } => (
                    ViewingConeClass::CalibratedFieldOfView {
                        calibration_version: calibration_version.clone(),
                        max_angular_error_mdeg: *max_angular_error_mdeg,
                    },
                    *azimuth_deg,
                    *horizontal_fov_deg,
                    f64::from(*max_angular_error_mdeg) / 1_000.0,
                ),
                CameraViewingOrientation::ReportedApproximate {
                    azimuth_deg,
                    horizontal_fov_deg,
                    bearing_uncertainty_deg,
                } => (
                    ViewingConeClass::IllustrativeReportedBearing {
                        bearing_uncertainty_deg: *bearing_uncertainty_deg,
                    },
                    *azimuth_deg,
                    *horizontal_fov_deg,
                    *bearing_uncertainty_deg,
                ),
                CameraViewingOrientation::InvalidatedByMovement { .. }
                | CameraViewingOrientation::Unknown { .. } => {
                    return Err(CameraDirectoryError::OrientationUnavailableForViewingCone {
                        camera_id: camera_id.clone(),
                    });
                }
            };

        let position_angular_uncertainty_deg = if horizontal_uncertainty_m >= range_m {
            180.0
        } else {
            (horizontal_uncertainty_m / range_m).asin().to_degrees()
        };
        let total_half_widening_deg = angular_uncertainty_deg + position_angular_uncertainty_deg;
        let uncertainty_widened_horizontal_fov_deg =
            (horizontal_fov_deg + 2.0 * total_half_widening_deg).min(359.999_999);

        let half_fov = horizontal_fov_deg * 0.5;
        let start_bearing = azimuth_deg - half_fov;
        let widened_half_fov = uncertainty_widened_horizontal_fov_deg * 0.5;
        let widened_start_bearing = azimuth_deg - widened_half_fov;
        let step_count = arc_vertex_count.saturating_sub(1);
        let mut arc_vertices_lat_lon_deg = Vec::with_capacity(arc_vertex_count);
        let mut uncertainty_widened_arc_vertices_lat_lon_deg = Vec::with_capacity(arc_vertex_count);
        for idx in 0..arc_vertex_count {
            let fraction = if step_count == 0 {
                0.0
            } else {
                (idx as f64) / (step_count as f64)
            };
            let bearing = (start_bearing + horizontal_fov_deg * fraction).rem_euclid(360.0);
            arc_vertices_lat_lon_deg.push(project_spherical_point(
                apex_latitude_deg,
                apex_longitude_deg,
                bearing,
                range_m,
            ));
            let widened_bearing = (widened_start_bearing
                + uncertainty_widened_horizontal_fov_deg * fraction)
                .rem_euclid(360.0);
            uncertainty_widened_arc_vertices_lat_lon_deg.push(project_spherical_point(
                apex_latitude_deg,
                apex_longitude_deg,
                widened_bearing,
                range_m,
            ));
        }

        Ok(ViewingConeGeometry {
            camera_id: entry.camera_id.clone(),
            source_version: entry.source_version.clone(),
            cone_class,
            apex_latitude_deg,
            apex_longitude_deg,
            horizontal_uncertainty_m,
            azimuth_deg,
            horizontal_fov_deg,
            uncertainty_widened_horizontal_fov_deg,
            range_m,
            arc_vertices_lat_lon_deg,
            uncertainty_widened_arc_vertices_lat_lon_deg,
        })
    }

    /// Classifies an incoming camera frame or still sample and, when permitted and evidentiary,
    /// records it in the camera's bounded retention ring buffer (`CAM-01`, `CAM-02`, `CAM-04`, `A09`).
    ///
    /// - Refuses restricted cameras (`SourceRestricted`), policy-disabled cameras (`CameraDisabledByPolicy`),
    ///   and cameras where `automated_processing != Permitted` (`AutomatedProcessingNotPermitted`).
    /// - Enforces privacy view exclusions and required mask zones before admitting frames.
    /// - Separates sequence spaces for live/still observations, historical archive frames, and
    ///   non-evidentiary substitutes so substitute or archive submissions never mask or fabricate
    ///   live sequence discontinuities.
    /// - Evaluates `PeriodicUpstreamStill` freshness against `stale_after_ms`:
    ///   - Stills with an explicit `source_capture_time_ms` within `stale_after_ms` and unchanged hold
    ///     within `stale_after_ms` are classified as `FreshPeriodicStill` (`eligible_as_event_evidence() == true`).
    ///   - Stills missing `source_capture_time_ms` (when not yet exceeding hold duration) are classified
    ///     as `PeriodicStillCaptureTimeUnknown` (`eligible_as_event_evidence() == false`).
    ///   - Stills whose capture age or unchanged hold duration exceeds `stale_after_ms` are downgraded
    ///     to [`FeedObservationKind::StaleStillHold`] (`eligible_as_event_evidence() == false`).
    /// - Admits only live evidentiary observations (`UpstreamCameraFeed` and `FreshPeriodicStill` with
    ///   explicit `source_capture_time_ms`) into the bounded ring buffer when `clip_retention == Permitted`,
    ///   evicting expired and over-capacity frames in deterministic FIFO `(ingestion_time_ms, sequence_number, frame_version)` order.
    pub fn ingest_frame(
        &mut self,
        camera_id: &OpaqueId,
        submission: CameraFrameSubmission,
    ) -> Result<ClassifiedCameraFrame, CameraDirectoryError> {
        if self.seen_versions.contains(&submission.frame_version) {
            return Err(CameraDirectoryError::ReusedVersionId(
                submission.frame_version,
            ));
        }
        if !is_valid_lowercase_sha256_hex(&submission.sha256_hex) {
            return Err(CameraDirectoryError::InvalidSha256Hex {
                field: "submission.sha256_hex",
            });
        }
        if submission.byte_len == 0 {
            return Err(CameraDirectoryError::ZeroFrameByteLen {
                frame_version: submission.frame_version,
            });
        }
        if submission.origin != FrameArtifactOrigin::OriginalRetainedFrame {
            return Err(CameraDirectoryError::GeneratedFrameForbiddenAsEvidence {
                frame_version: submission.frame_version,
            });
        }
        if submission.clock_uncertainty_ms > MAX_CLOCK_UNCERTAINTY_MS {
            return Err(CameraDirectoryError::ClockUncertaintyExceedsMax {
                uncertainty_ms: submission.clock_uncertainty_ms,
                max_ms: MAX_CLOCK_UNCERTAINTY_MS,
            });
        }
        if let Some(capture_ms) = submission.source_capture_time_ms
            && submission.ingestion_time_ms < capture_ms
        {
            return Err(CameraDirectoryError::InvalidTimeInterval {
                start_ms: capture_ms,
                end_ms: submission.ingestion_time_ms,
            });
        }

        let entry = self
            .cameras
            .get_mut(camera_id)
            .ok_or_else(|| CameraDirectoryError::UnknownCameraId(camera_id.clone()))?;

        if entry.access_status == CameraAccessStatus::RestrictedOrUnavailable {
            return Err(CameraDirectoryError::SourceRestricted {
                camera_id: camera_id.clone(),
                policy_notice: RESTRICTED_SOURCE_POLICY_NOTICE,
            });
        }
        if entry.current_health == SourceLifecycleHealth::DisabledByPolicy {
            return Err(CameraDirectoryError::CameraDisabledByPolicy {
                camera_id: camera_id.clone(),
            });
        }
        if entry.terms.automated_processing != SourceUsePermission::Permitted {
            return Err(CameraDirectoryError::AutomatedProcessingNotPermitted {
                camera_id: camera_id.clone(),
                status: entry.terms.automated_processing,
            });
        }

        match &entry.privacy_assessment {
            PrivacyViewAssessment::ClearPublicView { .. } => {}
            PrivacyViewAssessment::RequiresMaskZones { zones, .. } => {
                for zone in zones {
                    if !zone.mask_applied {
                        return Err(CameraDirectoryError::UnmaskedPrivacyZone {
                            camera_id: camera_id.clone(),
                            zone_id: zone.zone_id.clone(),
                        });
                    }
                }
            }
            PrivacyViewAssessment::ExcludedPrivateActivity { reason, .. } => {
                return Err(CameraDirectoryError::PrivateActivityViewExcluded {
                    camera_id: camera_id.clone(),
                    reason: reason.clone(),
                });
            }
        }

        let is_archive_submission =
            submission.declared_kind == FeedObservationKind::HistoricalArchiveFrame;
        let is_live_or_still_submission = matches!(
            submission.declared_kind,
            FeedObservationKind::UpstreamCameraFeed | FeedObservationKind::PeriodicUpstreamStill
        );

        if is_archive_submission {
            if let Some((prev_ing_ms, prev_seq)) = entry.last_archive_ingested_frame
                && (submission.ingestion_time_ms < prev_ing_ms
                    || submission.sequence_number <= prev_seq)
            {
                return Err(CameraDirectoryError::OutOfOrderFrameIngestion {
                    camera_id: camera_id.clone(),
                    previous_ingestion_ms: prev_ing_ms,
                    previous_sequence: prev_seq,
                    current_ingestion_ms: submission.ingestion_time_ms,
                    current_sequence: submission.sequence_number,
                });
            }
        } else if is_live_or_still_submission {
            if let Some((prev_ingestion_ms, prev_seq)) = entry.last_live_ingested_frame
                && (submission.ingestion_time_ms < prev_ingestion_ms
                    || submission.sequence_number <= prev_seq)
            {
                return Err(CameraDirectoryError::OutOfOrderFrameIngestion {
                    camera_id: camera_id.clone(),
                    previous_ingestion_ms: prev_ingestion_ms,
                    previous_sequence: prev_seq,
                    current_ingestion_ms: submission.ingestion_time_ms,
                    current_sequence: submission.sequence_number,
                });
            }
        } else if let Some((prev_sub_ms, prev_sub_seq)) = entry.last_substitute_ingested_frame
            && (submission.ingestion_time_ms < prev_sub_ms
                || submission.sequence_number <= prev_sub_seq)
        {
            return Err(CameraDirectoryError::OutOfOrderFrameIngestion {
                camera_id: camera_id.clone(),
                previous_ingestion_ms: prev_sub_ms,
                previous_sequence: prev_sub_seq,
                current_ingestion_ms: submission.ingestion_time_ms,
                current_sequence: submission.sequence_number,
            });
        }

        if is_live_or_still_submission
            && let Some(capture_ms) = submission.source_capture_time_ms
            && let Some(prev_capture_ms) = entry.last_live_capture_time_ms
            && capture_ms < prev_capture_ms
        {
            return Err(CameraDirectoryError::OutOfOrderFrameCaptureTime {
                camera_id: camera_id.clone(),
                previous_event_time_ms: prev_capture_ms,
                current_event_time_ms: capture_ms,
            });
        }

        let will_record_discontinuity = is_live_or_still_submission
            && entry.last_live_ingested_frame.is_some_and(|(_, prev_seq)| {
                submission.sequence_number > prev_seq.saturating_add(1)
            });
        if will_record_discontinuity {
            let next_spans = entry.discontinuity_spans.len().saturating_add(1);
            if next_spans > self.limits.max_discontinuity_spans_per_camera {
                return Err(CameraDirectoryError::LimitExceeded {
                    kind: CameraDirectoryLimitKind::DiscontinuitySpansPerCamera,
                    actual: next_spans,
                    max: self.limits.max_discontinuity_spans_per_camera,
                });
            }
        }

        let (effective_kind, freshness) = match submission.declared_kind {
            FeedObservationKind::UpstreamCameraFeed => {
                let StreamCapability::LiveStreamSupported {
                    nominal_frame_interval_ms,
                } = entry.stream_capability
                else {
                    return Err(CameraDirectoryError::UndeclaredStreamCapability {
                        camera_id: camera_id.clone(),
                    });
                };
                let observed_capture_age_ms = submission
                    .source_capture_time_ms
                    .map(|cap_ms| cap_ms.abs_diff(submission.ingestion_time_ms));
                let max_live_age_ms = entry
                    .terms
                    .max_retention_window_ms
                    .unwrap_or_else(|| nominal_frame_interval_ms.saturating_mul(10))
                    .max(nominal_frame_interval_ms.saturating_mul(2));
                if let Some(age_ms) = observed_capture_age_ms
                    && age_ms > max_live_age_ms
                {
                    return Err(CameraDirectoryError::LiveFrameExceedsStalenessBound {
                        camera_id: camera_id.clone(),
                        frame_version: submission.frame_version,
                        capture_age_ms: age_ms,
                        max_age_ms: max_live_age_ms,
                    });
                }
                (
                    FeedObservationKind::UpstreamCameraFeed,
                    FrameFreshnessEvaluation::LiveStream {
                        nominal_frame_interval_ms,
                        source_capture_time_ms: submission.source_capture_time_ms,
                        ingestion_time_ms: submission.ingestion_time_ms,
                        observed_capture_age_ms,
                    },
                )
            }
            FeedObservationKind::PeriodicUpstreamStill => {
                let StillRefreshCapability::PeriodicStillSupported { stale_after_ms, .. } =
                    entry.still_capability
                else {
                    return Err(CameraDirectoryError::UndeclaredStillCapability {
                        camera_id: camera_id.clone(),
                    });
                };
                let observed_capture_age_ms = submission
                    .source_capture_time_ms
                    .map(|cap_ms| cap_ms.abs_diff(submission.ingestion_time_ms));
                let unchanged_hold_duration_ms = match &entry.last_periodic_still {
                    Some(prev)
                        if prev.sha256_hex == submission.sha256_hex
                            || (submission.source_capture_time_ms.is_some()
                                && prev.source_capture_time_ms
                                    == submission.source_capture_time_ms) =>
                    {
                        prev.first_seen_ingestion_ms
                            .abs_diff(submission.ingestion_time_ms)
                    }
                    _ => 0,
                };
                let is_stale_by_age =
                    observed_capture_age_ms.is_some_and(|age| age > stale_after_ms);
                let is_stale_by_hold = unchanged_hold_duration_ms > stale_after_ms;
                if is_stale_by_age || is_stale_by_hold {
                    (
                        FeedObservationKind::StaleStillHold,
                        FrameFreshnessEvaluation::StalePeriodicStill {
                            stale_after_ms,
                            source_capture_time_ms: submission.source_capture_time_ms,
                            ingestion_time_ms: submission.ingestion_time_ms,
                            observed_capture_age_ms,
                            unchanged_hold_duration_ms,
                        },
                    )
                } else if let (Some(cap_ms), Some(age_ms)) =
                    (submission.source_capture_time_ms, observed_capture_age_ms)
                {
                    (
                        FeedObservationKind::PeriodicUpstreamStill,
                        FrameFreshnessEvaluation::FreshPeriodicStill {
                            stale_after_ms,
                            source_capture_time_ms: cap_ms,
                            ingestion_time_ms: submission.ingestion_time_ms,
                            observed_capture_age_ms: age_ms,
                            unchanged_hold_duration_ms,
                        },
                    )
                } else {
                    (
                        FeedObservationKind::PeriodicUpstreamStill,
                        FrameFreshnessEvaluation::PeriodicStillCaptureTimeUnknown {
                            stale_after_ms,
                            ingestion_time_ms: submission.ingestion_time_ms,
                            unchanged_hold_duration_ms,
                        },
                    )
                }
            }
            FeedObservationKind::HistoricalArchiveFrame => {
                let ArchiveCapability::VerifiedArchiveWindow {
                    ref archive_version,
                    earliest_capture_ms,
                    latest_capture_ms,
                } = entry.archive_capability
                else {
                    return Err(CameraDirectoryError::ArchiveFrameOutsideVerifiedWindow {
                        camera_id: camera_id.clone(),
                        frame_version: submission.frame_version,
                        reason: "camera has NoArchiveEstablished capability",
                    });
                };
                if submission.archive_version.as_ref() != Some(archive_version) {
                    return Err(CameraDirectoryError::ArchiveFrameOutsideVerifiedWindow {
                        camera_id: camera_id.clone(),
                        frame_version: submission.frame_version,
                        reason: "submission archive_version does not match camera's verified archive_version",
                    });
                }
                let Some(capture_ms) = submission.source_capture_time_ms else {
                    return Err(CameraDirectoryError::ArchiveFrameOutsideVerifiedWindow {
                        camera_id: camera_id.clone(),
                        frame_version: submission.frame_version,
                        reason: "historical archive frame requires an explicit source_capture_time_ms",
                    });
                };
                if capture_ms < earliest_capture_ms || capture_ms > latest_capture_ms {
                    return Err(CameraDirectoryError::ArchiveFrameOutsideVerifiedWindow {
                        camera_id: camera_id.clone(),
                        frame_version: submission.frame_version,
                        reason: "source_capture_time_ms is outside verified archive window",
                    });
                }
                (
                    FeedObservationKind::HistoricalArchiveFrame,
                    FrameFreshnessEvaluation::VerifiedHistoricalArchive {
                        archive_version: archive_version.clone(),
                        window_earliest_ms: earliest_capture_ms,
                        window_latest_ms: latest_capture_ms,
                        source_capture_time_ms: capture_ms,
                        ingestion_time_ms: submission.ingestion_time_ms,
                    },
                )
            }
            FeedObservationKind::StaleStillHold
            | FeedObservationKind::StillFallback
            | FeedObservationKind::StreetViewFallback
            | FeedObservationKind::SyntheticPlaceholder => (
                submission.declared_kind,
                FrameFreshnessEvaluation::NonEvidentiarySubstitute {
                    substitute_kind: submission.declared_kind,
                    ingestion_time_ms: submission.ingestion_time_ms,
                },
            ),
        };

        let is_live_evidentiary_with_capture_time = matches!(
            freshness,
            FrameFreshnessEvaluation::LiveStream {
                source_capture_time_ms: Some(_),
                ..
            } | FrameFreshnessEvaluation::FreshPeriodicStill { .. }
        );
        let should_buffer = is_live_evidentiary_with_capture_time
            && entry.terms.clip_retention == SourceUsePermission::Permitted;
        if should_buffer && submission.byte_len > self.limits.max_buffered_bytes_per_camera {
            return Err(CameraDirectoryError::FrameExceedsBufferByteLimit {
                frame_version: submission.frame_version,
                byte_len: submission.byte_len,
                max_buffered_bytes: self.limits.max_buffered_bytes_per_camera,
            });
        }

        if is_archive_submission {
            entry.last_archive_ingested_frame =
                Some((submission.ingestion_time_ms, submission.sequence_number));
        } else if is_live_or_still_submission {
            if let Some((prev_ingestion_ms, prev_seq)) = entry.last_live_ingested_frame
                && submission.sequence_number > prev_seq.saturating_add(1)
            {
                let span_start_ms = entry.last_live_capture_time_ms.unwrap_or(prev_ingestion_ms);
                let span_end_ms = submission
                    .source_capture_time_ms
                    .unwrap_or(submission.ingestion_time_ms)
                    .max(span_start_ms);
                entry.discontinuity_spans.push(FrameDiscontinuitySpan {
                    source_version: entry.source_version.clone(),
                    previous_sequence_number: prev_seq,
                    next_sequence_number: submission.sequence_number,
                    dropped_frame_count: submission.sequence_number - prev_seq - 1,
                    start_ms: span_start_ms,
                    end_ms: span_end_ms,
                });
            }
            if let Some(capture_ms) = submission.source_capture_time_ms {
                entry.last_live_capture_time_ms = Some(capture_ms);
            }
            entry.last_live_ingested_frame =
                Some((submission.ingestion_time_ms, submission.sequence_number));
        } else {
            entry.last_substitute_ingested_frame =
                Some((submission.ingestion_time_ms, submission.sequence_number));
        }

        if submission.declared_kind == FeedObservationKind::PeriodicUpstreamStill {
            let keep_first_seen = entry
                .last_periodic_still
                .as_ref()
                .filter(|prev| {
                    prev.sha256_hex == submission.sha256_hex
                        || (submission.source_capture_time_ms.is_some()
                            && prev.source_capture_time_ms == submission.source_capture_time_ms)
                })
                .map(|prev| prev.first_seen_ingestion_ms)
                .unwrap_or(submission.ingestion_time_ms);
            entry.last_periodic_still = Some(LastPeriodicStillTracker {
                sha256_hex: submission.sha256_hex.clone(),
                source_capture_time_ms: submission.source_capture_time_ms,
                first_seen_ingestion_ms: keep_first_seen,
            });
        }

        if effective_kind == FeedObservationKind::StaleStillHold {
            entry.stale_still_hold_count = entry.stale_still_hold_count.saturating_add(1);
            entry.current_health = SourceLifecycleHealth::Stale;
        } else if !effective_kind.eligible_as_event_evidence() {
            entry.non_evidentiary_substitute_count =
                entry.non_evidentiary_substitute_count.saturating_add(1);
        } else if is_live_evidentiary_with_capture_time
            && (entry.current_health == SourceLifecycleHealth::Stale
                || entry.current_health == SourceLifecycleHealth::Connecting)
        {
            entry.current_health = SourceLifecycleHealth::Available;
        }

        let classified = ClassifiedCameraFrame {
            frame_version: submission.frame_version.clone(),
            source_version: entry.source_version.clone(),
            sha256_hex: submission.sha256_hex,
            byte_len: submission.byte_len,
            sequence_number: submission.sequence_number,
            declared_kind: submission.declared_kind,
            effective_kind,
            origin: submission.origin,
            source_capture_time_ms: submission.source_capture_time_ms,
            clock_uncertainty_ms: submission.clock_uncertainty_ms,
            ingestion_time_ms: submission.ingestion_time_ms,
            freshness,
            retained_in_ring_buffer: should_buffer,
        };

        if should_buffer {
            let current_capture_ms = classified
                .source_capture_time_ms
                .unwrap_or(classified.ingestion_time_ms);
            let max_evicted_ledger = self
                .limits
                .max_buffered_frames_per_camera
                .saturating_mul(4)
                .max(16);

            if let Some(max_window_ms) = entry.terms.max_retention_window_ms {
                while let Some(oldest) = entry.ring_buffer.front() {
                    let oldest_capture_ms = oldest
                        .source_capture_time_ms
                        .unwrap_or(oldest.ingestion_time_ms);
                    let age_ms = oldest_capture_ms.abs_diff(current_capture_ms);
                    if current_capture_ms >= oldest_capture_ms && age_ms > max_window_ms {
                        if let Some(evicted) = entry.ring_buffer.pop_front() {
                            let ev_cap_ms = evicted
                                .source_capture_time_ms
                                .unwrap_or(evicted.ingestion_time_ms);
                            entry.buffered_bytes =
                                entry.buffered_bytes.saturating_sub(evicted.byte_len);
                            entry.eviction_summary.record_eviction(
                                &evicted,
                                ev_cap_ms,
                                true,
                                max_evicted_ledger,
                            );
                        }
                    } else {
                        break;
                    }
                }
            }

            while entry.ring_buffer.len().saturating_add(1)
                > self.limits.max_buffered_frames_per_camera
                || entry.buffered_bytes.saturating_add(classified.byte_len)
                    > self.limits.max_buffered_bytes_per_camera
            {
                if let Some(evicted) = entry.ring_buffer.pop_front() {
                    let ev_cap_ms = evicted
                        .source_capture_time_ms
                        .unwrap_or(evicted.ingestion_time_ms);
                    entry.buffered_bytes = entry.buffered_bytes.saturating_sub(evicted.byte_len);
                    entry.eviction_summary.record_eviction(
                        &evicted,
                        ev_cap_ms,
                        false,
                        max_evicted_ledger,
                    );
                } else {
                    break;
                }
            }

            entry.buffered_bytes = entry.buffered_bytes.saturating_add(classified.byte_len);
            entry.ring_buffer.push_back(classified.clone());
        }

        self.seen_versions.insert(submission.frame_version);
        Ok(classified)
    }

    /// Extracts a retained evidence clip manifest from a camera's bounded ring buffer (`CAM-04`,
    /// `docs/architecture.md` §8).
    ///
    /// - Refuses cameras currently in [`SourceLifecycleHealth::DisabledByPolicy`] (`CameraDisabledByPolicy`)
    ///   and cameras where `clip_retention != Permitted` (`ClipRetentionNotPermitted`); for unbuffered
    ///   cameras, callers use [`RegisteredCameraEntry::omitted_retention_context_status`].
    /// - Filters buffered frames and discontinuities to the acquisition `source_version` of the
    ///   event's primary frame while recording any excluded cross-version frames in
    ///   [`RetainedCameraClipManifest::excluded_cross_version_frames`] and any overlapping restriction
    ///   intervals in [`RetainedCameraClipManifest::restriction_intervals_in_window`] (`AGENTS.md` §2).
    /// - Evaluates temporal coverage gaps across `[pre_start_ms, post_end_ms]` against `2 * nominal_interval_ms`
    ///   and records any exceeded spans in [`RetainedCameraClipManifest::coverage_gaps_in_window`].
    /// - Tracks [`RetainedCameraClipManifest::eviction_ledger_truncated`] when older per-frame eviction
    ///   records rolled off the bounded eviction ledger so lower-bound eviction counts are never
    ///   presented as exact (`AGENTS.md` §2, §3).
    /// - Returns [`ContextWindowStatus::Complete`] **only** when every requested non-zero pre/post
    ///   context window is populated, `extracted_at_ms >= post_end_ms`, there are zero window evictions,
    ///   zero capacity evictions, zero ledger truncations, zero dropped frames, zero coverage gaps,
    ///   zero excluded cross-version frames, and zero restriction intervals in `[pre_start_ms, post_end_ms]`.
    ///   Otherwise returns [`ContextWindowStatus::PartialBufferExpired`].
    pub fn extract_retained_clip_manifest(
        &mut self,
        camera_id: &OpaqueId,
        request: ClipExtractionRequest,
    ) -> Result<&RetainedCameraClipManifest, CameraDirectoryError> {
        if self.seen_clip_ids.contains(&request.clip_id) {
            return Err(CameraDirectoryError::DuplicateClipId(request.clip_id));
        }
        if self.seen_versions.contains(&request.clip_version) {
            return Err(CameraDirectoryError::ReusedVersionId(request.clip_version));
        }
        if request.event_end_ms <= request.event_start_ms {
            return Err(CameraDirectoryError::InvalidTimeInterval {
                start_ms: request.event_start_ms,
                end_ms: request.event_end_ms,
            });
        }
        if request.extracted_at_ms < request.event_start_ms {
            return Err(CameraDirectoryError::InvalidTimeInterval {
                start_ms: request.event_start_ms,
                end_ms: request.extracted_at_ms,
            });
        }

        let entry = self
            .cameras
            .get(camera_id)
            .ok_or_else(|| CameraDirectoryError::UnknownCameraId(camera_id.clone()))?;
        if entry.current_health == SourceLifecycleHealth::DisabledByPolicy {
            return Err(CameraDirectoryError::CameraDisabledByPolicy {
                camera_id: camera_id.clone(),
            });
        }
        if entry.terms.clip_retention != SourceUsePermission::Permitted {
            return Err(CameraDirectoryError::ClipRetentionNotPermitted {
                camera_id: camera_id.clone(),
                status: entry.terms.clip_retention,
            });
        }
        let next_manifests = entry.clip_manifests.len().saturating_add(1);
        if next_manifests > self.limits.max_clip_manifests_per_camera {
            return Err(CameraDirectoryError::LimitExceeded {
                kind: CameraDirectoryLimitKind::ClipManifestsPerCamera,
                actual: next_manifests,
                max: self.limits.max_clip_manifests_per_camera,
            });
        }

        let mut staged_text_bytes = self.used_text_bytes;
        check_single_line_text(
            &request.trigger_explanation,
            "request.trigger_explanation",
            &mut staged_text_bytes,
            self.limits,
        )?;

        let pre_start_ms = request
            .event_start_ms
            .saturating_sub(i64::try_from(request.requested_pre_context_ms).unwrap_or(i64::MAX));
        let post_end_ms = request
            .event_end_ms
            .saturating_add(i64::try_from(request.requested_post_context_ms).unwrap_or(i64::MAX));

        let mut raw_event_frames = Vec::new();
        for frame in &entry.ring_buffer {
            if !frame.eligible_as_event_evidence() {
                continue;
            }
            let Some(t) = frame.source_capture_time_ms else {
                continue;
            };
            if t >= request.event_start_ms && t < request.event_end_ms {
                raw_event_frames.push(frame.clone());
            }
        }

        let Some(first_event_frame) = raw_event_frames.first().cloned() else {
            return Err(CameraDirectoryError::NoEvidentiaryFramesInEventWindow {
                camera_id: camera_id.clone(),
                event_start_ms: request.event_start_ms,
                event_end_ms: request.event_end_ms,
            });
        };
        let manifest_source_version = first_event_frame.source_version.clone();

        let mut pre_context_frames = Vec::new();
        let mut event_frames = Vec::new();
        let mut post_context_frames = Vec::new();
        let mut excluded_cross_version_frames = Vec::new();
        let mut pre_frames = Vec::new();
        let mut post_frames = Vec::new();

        for frame in &entry.ring_buffer {
            if !frame.eligible_as_event_evidence() {
                continue;
            }
            let Some(t) = frame.source_capture_time_ms else {
                continue;
            };
            if t < pre_start_ms || t > post_end_ms {
                continue;
            }
            if frame.source_version != manifest_source_version {
                excluded_cross_version_frames.push(frame.clone());
                continue;
            }
            if t < request.event_start_ms {
                pre_frames.push(frame.to_retained_frame_ref()?);
                pre_context_frames.push(frame.clone());
            } else if t < request.event_end_ms {
                event_frames.push(frame.clone());
            } else {
                post_frames.push(frame.to_retained_frame_ref()?);
                post_context_frames.push(frame.clone());
            }
        }

        let total_manifest_frames = pre_context_frames
            .len()
            .saturating_add(event_frames.len())
            .saturating_add(post_context_frames.len());
        if total_manifest_frames > self.limits.max_frames_per_clip_manifest {
            return Err(CameraDirectoryError::LimitExceeded {
                kind: CameraDirectoryLimitKind::FramesPerClipManifest,
                actual: total_manifest_frames,
                max: self.limits.max_frames_per_clip_manifest,
            });
        }

        let primary_frame = first_event_frame.to_retained_frame_ref()?;
        let (expired_by_window_in_span, evicted_by_capacity_in_span, eviction_ledger_truncated) =
            entry
                .eviction_summary
                .evicted_counts_in_window(pre_start_ms, post_end_ms);

        let discontinuity_spans_in_window: Vec<FrameDiscontinuitySpan> = entry
            .discontinuity_spans
            .iter()
            .filter(|span| {
                span.source_version == manifest_source_version
                    && span.start_ms < post_end_ms
                    && span.end_ms > pre_start_ms
            })
            .cloned()
            .collect();
        let dropped_frame_count_in_span = discontinuity_spans_in_window
            .iter()
            .fold(0u64, |acc, span| {
                acc.saturating_add(span.dropped_frame_count)
            });

        let restriction_intervals_in_window: Vec<(i64, Option<i64>)> = entry
            .restriction_intervals
            .iter()
            .filter(|&&(r_start, r_end_opt)| {
                let r_end = r_end_opt.unwrap_or(i64::MAX);
                r_start < post_end_ms && r_end > pre_start_ms
            })
            .copied()
            .collect();

        let coverage_tolerance_ms = match (entry.stream_capability, entry.still_capability) {
            (
                StreamCapability::LiveStreamSupported {
                    nominal_frame_interval_ms,
                },
                _,
            ) => Some(nominal_frame_interval_ms.saturating_mul(2)),
            (
                StreamCapability::None,
                StillRefreshCapability::PeriodicStillSupported {
                    nominal_refresh_interval_ms,
                    ..
                },
            ) => Some(nominal_refresh_interval_ms.saturating_mul(2)),
            (StreamCapability::None, StillRefreshCapability::None) => None,
        };

        let mut coverage_gaps_in_window = Vec::new();
        if let Some(tolerance_ms) = coverage_tolerance_ms {
            let mut retained_times: Vec<i64> = pre_context_frames
                .iter()
                .chain(event_frames.iter())
                .chain(post_context_frames.iter())
                .filter_map(|f| f.source_capture_time_ms)
                .collect();
            retained_times.sort_unstable();

            if let (Some(&first_t), Some(&last_t)) = (retained_times.first(), retained_times.last())
            {
                if first_t > pre_start_ms {
                    let lead_gap_ms = pre_start_ms.abs_diff(first_t);
                    if lead_gap_ms > tolerance_ms {
                        coverage_gaps_in_window.push(CoverageGapSpan {
                            start_ms: pre_start_ms,
                            end_ms: first_t,
                            gap_duration_ms: lead_gap_ms,
                            tolerance_ms,
                        });
                    }
                }
                for pair in retained_times.windows(2) {
                    if let [prev_t, next_t] = *pair
                        && next_t > prev_t
                    {
                        let step_gap_ms = prev_t.abs_diff(next_t);
                        if step_gap_ms > tolerance_ms {
                            coverage_gaps_in_window.push(CoverageGapSpan {
                                start_ms: prev_t,
                                end_ms: next_t,
                                gap_duration_ms: step_gap_ms,
                                tolerance_ms,
                            });
                        }
                    }
                }
                if post_end_ms > last_t {
                    let tail_gap_ms = last_t.abs_diff(post_end_ms);
                    if tail_gap_ms > tolerance_ms {
                        coverage_gaps_in_window.push(CoverageGapSpan {
                            start_ms: last_t,
                            end_ms: post_end_ms,
                            gap_duration_ms: tail_gap_ms,
                            tolerance_ms,
                        });
                    }
                }
            }
        }

        let missing_pre_context = request.requested_pre_context_ms > 0 && pre_frames.is_empty();
        let missing_post_context = request.requested_post_context_ms > 0
            && (post_frames.is_empty() || request.extracted_at_ms < post_end_ms);
        let empty_context_window_count =
            u8::from(missing_pre_context).saturating_add(u8::from(missing_post_context));

        let exact_frame_loss_count = u64::from(expired_by_window_in_span)
            .saturating_add(u64::from(evicted_by_capacity_in_span))
            .saturating_add(dropped_frame_count_in_span)
            .saturating_add(excluded_cross_version_frames.len() as u64);

        let is_partial = exact_frame_loss_count > 0
            || eviction_ledger_truncated
            || empty_context_window_count > 0
            || !restriction_intervals_in_window.is_empty()
            || !coverage_gaps_in_window.is_empty();

        let context_window = if is_partial {
            let expired_frame_count = u32::try_from(exact_frame_loss_count)
                .unwrap_or(u32::MAX)
                .max(1);
            ContextWindowStatus::PartialBufferExpired {
                pre_frames,
                post_frames,
                expired_frame_count,
            }
        } else if !pre_frames.is_empty() || !post_frames.is_empty() {
            ContextWindowStatus::Complete {
                pre_frames,
                post_frames,
            }
        } else {
            ContextWindowStatus::OmittedByRetentionPolicy {
                reason: "requested_pre_context_ms == 0 and requested_post_context_ms == 0"
                    .to_string(),
            }
        };

        let manifest = RetainedCameraClipManifest {
            clip_id: request.clip_id.clone(),
            clip_version: request.clip_version.clone(),
            camera_id: entry.camera_id.clone(),
            source_version: manifest_source_version,
            source_session_id: request.source_session_id,
            event_start_ms: request.event_start_ms,
            event_end_ms: request.event_end_ms,
            requested_pre_context_ms: request.requested_pre_context_ms,
            requested_post_context_ms: request.requested_post_context_ms,
            extracted_at_ms: request.extracted_at_ms,
            trigger_explanation: request.trigger_explanation,
            primary_frame,
            pre_context_frames,
            event_frames,
            post_context_frames,
            excluded_cross_version_frames,
            restriction_intervals_in_window,
            coverage_gaps_in_window,
            context_window,
            expired_by_window_in_span,
            evicted_by_capacity_in_span,
            eviction_ledger_truncated,
            dropped_frame_count_in_span,
            empty_context_window_count,
            missing_pre_context,
            missing_post_context,
            discontinuity_spans_in_window,
            redistribution_permission: entry.terms.redistribution,
            policy_reference: entry.terms.policy_reference.clone(),
        };

        let entry = self
            .cameras
            .get_mut(camera_id)
            .ok_or_else(|| CameraDirectoryError::UnknownCameraId(camera_id.clone()))?;
        entry.clip_manifests.push(manifest);
        self.seen_clip_ids.insert(request.clip_id);
        self.seen_versions.insert(request.clip_version);
        self.used_text_bytes = staged_text_bytes;

        entry
            .clip_manifests
            .last()
            .ok_or_else(|| CameraDirectoryError::UnknownCameraId(camera_id.clone()))
    }
}
