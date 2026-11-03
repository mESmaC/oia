//! Personal UAV registration, capability-aware HUD projection, clock-offset alignment,
//! frame-copy forensic handoff, and non-destructive onboard-recording linkage (`#33`).
//!
//! This module implements the bounded, dependency-independent domain and replay kernel for
//! `DEVICE-01`, `DEVICE-02`, `DEVICE-03`, and `DEVICE-04` toward acceptance scenario `A12`
//! (`docs/spec.md` §5.4, `docs/architecture.md` §§2.2, 3.1, 3.2, 8, `docs/delivery.md`).
//!
//! It does not select production `D08` hardware/protocol dependencies, open network sockets or
//! serial links, decode video bitstreams, persist records to `oia-store`, or claim a formal Lean
//! theorem.

use std::collections::{BTreeMap, BTreeSet};
use std::convert::Infallible;
use std::fmt;

use oia_domain::{ActorId, IdError, OpaqueId, SchemaVersion, VersionId};

/// Maximum UTF-8 byte length of a single free-text label, reason, or note field.
pub const MAX_TEXT_FIELD_BYTES: usize = 4_096;

/// Maximum magnitude of a device-to-UTC clock offset in milliseconds (`9999-12-31T23:59:59.999Z`),
/// supporting both small wall-clock skews and boot-relative monotonic device clocks.
pub const MAX_ABS_CLOCK_OFFSET_MS: i64 = 253_402_300_799_999;

/// Maximum magnitude of linear clock drift in parts per million (`100_000 ppm` = 10%).
pub const MAX_ABS_CLOCK_DRIFT_PPM: i32 = 100_000;

/// Minimum depression angle below the local horizon (in degrees) required to resolve a flat-ground
/// camera footprint without grazing-ray divergence.
pub const MIN_FOOTPRINT_DEPRESSION_DEG: f64 = 5.0;

/// Resource and staleness budgets for a bounded [`UavFlightSession`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UavSessionLimits {
    max_telemetry_samples: usize,
    max_video_frames: usize,
    max_drop_or_disconnection_spans: usize,
    max_bookmarks: usize,
    max_hud_derivatives: usize,
    max_onboard_links: usize,
    max_calibration_updates: usize,
    max_text_bytes: usize,
    max_output_bytes: usize,
    stale_telemetry_after_ms: i64,
}

impl UavSessionLimits {
    /// Constructs validated session budgets. Every count/byte limit and `stale_telemetry_after_ms`
    /// must be strictly positive.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        max_telemetry_samples: usize,
        max_video_frames: usize,
        max_drop_or_disconnection_spans: usize,
        max_bookmarks: usize,
        max_hud_derivatives: usize,
        max_onboard_links: usize,
        max_text_bytes: usize,
        max_output_bytes: usize,
        stale_telemetry_after_ms: i64,
    ) -> Result<Self, UavError> {
        for (kind, value) in [
            (UavLimitKind::TelemetrySamples, max_telemetry_samples),
            (UavLimitKind::VideoFrames, max_video_frames),
            (
                UavLimitKind::DropOrDisconnectionSpans,
                max_drop_or_disconnection_spans,
            ),
            (UavLimitKind::Bookmarks, max_bookmarks),
            (UavLimitKind::HudDerivatives, max_hud_derivatives),
            (UavLimitKind::OnboardLinks, max_onboard_links),
            (UavLimitKind::TextBytes, max_text_bytes),
            (UavLimitKind::OutputBytes, max_output_bytes),
        ] {
            if value == 0 {
                return Err(UavError::ZeroLimit { kind });
            }
        }
        if stale_telemetry_after_ms <= 0 {
            return Err(UavError::InvalidStaleThreshold {
                stale_telemetry_after_ms,
            });
        }
        Ok(Self {
            max_telemetry_samples,
            max_video_frames,
            max_drop_or_disconnection_spans,
            max_bookmarks,
            max_hud_derivatives,
            max_onboard_links,
            max_calibration_updates: 64,
            max_text_bytes,
            max_output_bytes,
            stale_telemetry_after_ms,
        })
    }

    /// Default bounded session limits (`2_000 ms` telemetry staleness threshold).
    pub fn default_bounded() -> Self {
        Self {
            max_telemetry_samples: 4_096,
            max_video_frames: 4_096,
            max_drop_or_disconnection_spans: 512,
            max_bookmarks: 256,
            max_hud_derivatives: 64,
            max_onboard_links: 64,
            max_calibration_updates: 64,
            max_text_bytes: 65_536,
            max_output_bytes: 65_536,
            stale_telemetry_after_ms: 2_000,
        }
    }

    /// Overrides the maximum number of clock-alignment and camera-calibration updates retained in a
    /// session (`> 0`).
    pub fn with_max_calibration_updates(
        mut self,
        max_calibration_updates: usize,
    ) -> Result<Self, UavError> {
        if max_calibration_updates == 0 {
            return Err(UavError::ZeroLimit {
                kind: UavLimitKind::CalibrationUpdates,
            });
        }
        self.max_calibration_updates = max_calibration_updates;
        Ok(self)
    }

    /// Maximum telemetry samples retained in a session.
    pub fn max_telemetry_samples(&self) -> usize {
        self.max_telemetry_samples
    }

    /// Maximum video frame records retained in a session.
    pub fn max_video_frames(&self) -> usize {
        self.max_video_frames
    }

    /// Maximum dropped-frame and telemetry-disconnection spans retained in a session.
    pub fn max_drop_or_disconnection_spans(&self) -> usize {
        self.max_drop_or_disconnection_spans
    }

    /// Maximum bookmarks retained in a session.
    pub fn max_bookmarks(&self) -> usize {
        self.max_bookmarks
    }

    /// Maximum rendered HUD derivatives registered against a session.
    pub fn max_hud_derivatives(&self) -> usize {
        self.max_hud_derivatives
    }

    /// Maximum linked onboard recordings registered against a session.
    pub fn max_onboard_links(&self) -> usize {
        self.max_onboard_links
    }

    /// Maximum clock-alignment and camera-calibration records retained in a session.
    pub fn max_calibration_updates(&self) -> usize {
        self.max_calibration_updates
    }

    /// Maximum cumulative UTF-8 text bytes across all session records.
    pub fn max_text_bytes(&self) -> usize {
        self.max_text_bytes
    }

    /// Maximum UTF-8 bytes emitted by summary/manifest rendering.
    pub fn max_output_bytes(&self) -> usize {
        self.max_output_bytes
    }

    /// Maximum age (in device-clock milliseconds) before a telemetry sample is marked stale.
    pub fn stale_telemetry_after_ms(&self) -> i64 {
        self.stale_telemetry_after_ms
    }
}

/// Which session budget was zero or exceeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UavLimitKind {
    /// Maximum number of telemetry samples in the session.
    TelemetrySamples,
    /// Maximum number of video frame metadata records in the session.
    VideoFrames,
    /// Maximum combined dropped-frame and telemetry-disconnection spans.
    DropOrDisconnectionSpans,
    /// Maximum number of flight bookmarks.
    Bookmarks,
    /// Maximum number of rendered HUD derivative records.
    HudDerivatives,
    /// Maximum number of linked onboard recordings.
    OnboardLinks,
    /// Maximum number of clock-alignment and camera-calibration history records.
    CalibrationUpdates,
    /// Maximum byte length of a single text field (`MAX_TEXT_FIELD_BYTES`).
    SingleTextFieldBytes,
    /// Maximum cumulative UTF-8 text bytes across the session.
    TextBytes,
    /// Maximum UTF-8 bytes in a rendered HUD or handoff manifest output.
    OutputBytes,
}

/// Distinct device capabilities negotiated at registration (`DEVICE-01`, `AGENTS.md` §4).
///
/// Video reception does not imply telemetry or flight control; telemetry does not imply video or
/// media import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UavCapability {
    /// Live or replayed video frame reception.
    VideoStream,
    /// Live or replayed flight telemetry reception.
    TelemetryStream,
    /// Import of recorded flight logs and onboard media files.
    RecordedMediaImport,
    /// Active airframe or flight-path command emission (`DEVICE-01`, `DEVICE-04`).
    ///
    /// Initial integration leaves flight control to the existing controller and rejects any
    /// registration or command requesting this capability (`A12`).
    FlightControl,
}

/// Declared adapter family for a registered personal drone session (`DEVICE-01`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UavAdapterKind {
    /// Offline import of a recorded flight log and companion video.
    RecordedFlightImport,
    /// Deterministic replay of a labeled simulation or test fixture.
    SimulatedTelemetryReplay,
    /// Passive local/network video stream reception.
    NetworkVideoStream,
    /// Passive local telemetry link (e.g., read-only ground-station telemetry tap).
    LocalTelemetryLink,
}

/// Input specification for registering a personal UAV adapter (`DEVICE-01`, `DEVICE-04`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UavAdapterRegistrationInput {
    /// Stable opaque identifier of the registered drone or ground-station source.
    pub device_id: OpaqueId,
    /// Immutable version of this device registration record.
    pub registration_version: VersionId,
    /// Schema version of the registration record.
    pub schema_version: SchemaVersion,
    /// Actor registering the device.
    pub registered_by: ActorId,
    /// Adapter path used to receive or import data.
    pub adapter_kind: UavAdapterKind,
    /// Human-inspectable aircraft or adapter model label.
    pub model_label: String,
    /// Explicitly declared capabilities for this adapter.
    pub declared_capabilities: Vec<UavCapability>,
    /// Whether connecting the local device automatically publishes footage or location externally.
    ///
    /// Must be `false` (`DEVICE-01`: "Local connection must not automatically publish
    /// footage/location").
    pub auto_publish_footage_or_location: bool,
}

/// Validated, capability-scoped personal UAV registration (`DEVICE-01`, `DEVICE-04`, `A12`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredUavDevice {
    device_id: OpaqueId,
    registration_version: VersionId,
    schema_version: SchemaVersion,
    registered_by: ActorId,
    adapter_kind: UavAdapterKind,
    model_label: String,
    declared_capabilities: Vec<UavCapability>,
}

impl RegisteredUavDevice {
    /// Validates and constructs a capability-scoped UAV registration.
    pub fn register(input: UavAdapterRegistrationInput) -> Result<Self, UavError> {
        let mut text_budget = MAX_TEXT_FIELD_BYTES;
        Self::register_with_budget(input, &mut text_budget)
    }

    fn register_with_budget(
        input: UavAdapterRegistrationInput,
        remaining_text_budget: &mut usize,
    ) -> Result<Self, UavError> {
        validate_single_line_text("model_label", &input.model_label, remaining_text_budget)?;
        if input.auto_publish_footage_or_location {
            return Err(UavError::AutoPublicationForbidden {
                device_id: input.device_id,
            });
        }
        if input.declared_capabilities.is_empty() {
            return Err(UavError::EmptyCapabilities {
                device_id: input.device_id,
            });
        }
        let mut seen = BTreeSet::new();
        for &cap in &input.declared_capabilities {
            if cap == UavCapability::FlightControl {
                return Err(UavError::FlightControlUnsupported {
                    device_id: input.device_id,
                });
            }
            if !seen.insert(cap) {
                return Err(UavError::DuplicateCapability {
                    device_id: input.device_id,
                    capability: cap,
                });
            }
        }
        Ok(Self {
            device_id: input.device_id,
            registration_version: input.registration_version,
            schema_version: input.schema_version,
            registered_by: input.registered_by,
            adapter_kind: input.adapter_kind,
            model_label: input.model_label,
            declared_capabilities: input.declared_capabilities,
        })
    }

    /// Stable device identifier.
    pub fn device_id(&self) -> &OpaqueId {
        &self.device_id
    }

    /// Immutable registration version.
    pub fn registration_version(&self) -> &VersionId {
        &self.registration_version
    }

    /// Registration schema version.
    pub fn schema_version(&self) -> SchemaVersion {
        self.schema_version
    }

    /// Actor who registered the device.
    pub fn registered_by(&self) -> &ActorId {
        &self.registered_by
    }

    /// Adapter family.
    pub fn adapter_kind(&self) -> UavAdapterKind {
        self.adapter_kind
    }

    /// Aircraft or adapter model label.
    pub fn model_label(&self) -> &str {
        &self.model_label
    }

    /// Declared capabilities.
    pub fn declared_capabilities(&self) -> &[UavCapability] {
        &self.declared_capabilities
    }

    /// Returns `true` only if [`UavCapability::VideoStream`] was explicitly declared.
    pub fn supports_video(&self) -> bool {
        self.declared_capabilities
            .contains(&UavCapability::VideoStream)
    }

    /// Returns `true` only if [`UavCapability::TelemetryStream`] was explicitly declared.
    pub fn supports_telemetry(&self) -> bool {
        self.declared_capabilities
            .contains(&UavCapability::TelemetryStream)
    }

    /// Returns `true` only if [`UavCapability::RecordedMediaImport`] was explicitly declared.
    pub fn supports_recorded_import(&self) -> bool {
        self.declared_capabilities
            .contains(&UavCapability::RecordedMediaImport)
    }

    /// Always returns `false`: local connection never auto-publishes footage or location (`DEVICE-01`).
    pub fn auto_publishes_footage_or_location(&self) -> bool {
        false
    }

    /// Always returns `false`: initial UAV integration leaves flight control to the existing
    /// controller (`DEVICE-04`, `A12`).
    pub fn can_emit_flight_control(&self) -> bool {
        false
    }

    /// Explicitly refuses any attempt to emit a flight-control command (`DEVICE-04`, `A12`).
    pub fn attempt_flight_control_command(
        &self,
        command_name: &str,
    ) -> Result<Infallible, UavError> {
        let mut budget = MAX_TEXT_FIELD_BYTES;
        validate_single_line_text("command_name", command_name, &mut budget)?;
        Err(UavError::FlightControlCommandForbidden {
            device_id: self.device_id.clone(),
            command: command_name.to_owned(),
        })
    }
}

/// Origin class of a flight stream artifact (`DEVICE-03`, `AGENTS.md` §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlightStreamOrigin {
    /// Imported recorded flight artifact.
    RecordedFlightImport,
    /// Captured live stream session artifact.
    LiveSessionCapture,
    /// Labeled deterministic simulation or replay fixture.
    SimulatedFlightFixture,
    /// Rendered HUD overlay derivative (`DEVICE-03`).
    ///
    /// Rejected when submitted as an original flight stream so rendered HUD recordings never
    /// masquerade as or replace original flight evidence.
    RenderedHudOverlayDerivative,
}

/// Input specification for the immutable original flight stream of a session (`DEVICE-03`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalFlightStreamInput {
    /// Logical stream artifact identifier.
    pub stream_id: OpaqueId,
    /// Immutable version of the original flight stream.
    pub stream_version: VersionId,
    /// Declared origin class of the stream.
    pub origin: FlightStreamOrigin,
    /// Lowercase 64-character hexadecimal SHA-256 digest of the retained original bytes.
    pub sha256_hex: String,
    /// Retained byte length of the original stream (`> 0`).
    pub byte_len: u64,
    /// MIME media type (for example `"video/mp4"` or `"application/vnd.oia.flight-log"`).
    pub media_type: String,
}

/// Validated original flight stream metadata (`DEVICE-03`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalFlightStream {
    stream_id: OpaqueId,
    stream_version: VersionId,
    origin: FlightStreamOrigin,
    sha256_hex: String,
    byte_len: u64,
    media_type: String,
}

impl OriginalFlightStream {
    /// Logical stream identifier.
    pub fn stream_id(&self) -> &OpaqueId {
        &self.stream_id
    }

    /// Immutable version identifier of the original stream.
    pub fn stream_version(&self) -> &VersionId {
        &self.stream_version
    }

    /// Origin class of the stream (guaranteed not to be `RenderedHudOverlayDerivative`).
    pub fn origin(&self) -> FlightStreamOrigin {
        self.origin
    }

    /// Returns `true` when the stream is a labeled simulation fixture (`AGENTS.md` §2).
    pub fn is_simulated(&self) -> bool {
        self.origin == FlightStreamOrigin::SimulatedFlightFixture
    }

    /// Lowercase 64-character hexadecimal SHA-256 digest of the original stream bytes.
    pub fn sha256_hex(&self) -> &str {
        &self.sha256_hex
    }

    /// Retained byte length of the original stream.
    pub fn byte_len(&self) -> u64 {
        self.byte_len
    }

    /// MIME media type of the original stream.
    pub fn media_type(&self) -> &str {
        &self.media_type
    }
}

/// Input for registering a separate rendered HUD derivative recording (`DEVICE-03`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedHudDerivativeInput {
    /// Artifact identifier for the rendered HUD recording.
    pub derivative_id: OpaqueId,
    /// Immutable version identifier for the rendered HUD recording.
    pub derivative_version: VersionId,
    /// Pinned source stream version from which the HUD recording was rendered.
    pub derived_from_stream_version: VersionId,
    /// Lowercase 64-character hexadecimal SHA-256 digest of the rendered derivative bytes.
    pub sha256_hex: String,
    /// Byte length of the rendered derivative (`> 0`).
    pub byte_len: u64,
    /// MIME media type of the rendered derivative.
    pub media_type: String,
    /// Inspectable description of the HUD overlay layout and units rendered into the derivative.
    pub overlay_configuration_label: String,
}

/// Separate derivative record for a rendered HUD recording (`DEVICE-03`, `AGENTS.md` §2).
///
/// Any future pixel-overlay renderer producing a [`RenderedHudDerivative`] must visibly render
/// `Fresh`, `Stale`, `Unavailable`, `UnsynchronizedDegraded`, and `DERIVED_ESTIMATE` states in the
/// overlay rather than presenting stale, missing, or derived values as fresh direct measurements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedHudDerivative {
    derivative_id: OpaqueId,
    derivative_version: VersionId,
    derived_from_stream_version: VersionId,
    sha256_hex: String,
    byte_len: u64,
    media_type: String,
    overlay_configuration_label: String,
}

impl RenderedHudDerivative {
    /// Derivative artifact identifier.
    pub fn derivative_id(&self) -> &OpaqueId {
        &self.derivative_id
    }

    /// Immutable version of the rendered HUD derivative.
    pub fn derivative_version(&self) -> &VersionId {
        &self.derivative_version
    }

    /// Pinned original or onboard stream version from which this HUD recording was rendered.
    pub fn derived_from_stream_version(&self) -> &VersionId {
        &self.derived_from_stream_version
    }

    /// Lowercase 64-character hexadecimal SHA-256 digest.
    pub fn sha256_hex(&self) -> &str {
        &self.sha256_hex
    }

    /// Byte length of the derivative.
    pub fn byte_len(&self) -> u64 {
        self.byte_len
    }

    /// Media type of the derivative.
    pub fn media_type(&self) -> &str {
        &self.media_type
    }

    /// Overlay layout and unit configuration label.
    pub fn overlay_configuration_label(&self) -> &str {
        &self.overlay_configuration_label
    }

    /// Always returns `false`: rendered HUD recordings remain conventional derivatives and never
    /// replace original footage or telemetry (`DEVICE-03`, `AGENTS.md` §2).
    pub fn can_replace_original_evidence(&self) -> bool {
        false
    }
}

/// Documented clock-offset and linear drift alignment record (`DEVICE-04`, `docs/architecture.md` §3.1).
///
/// Within `[valid_from_device_ms, valid_until_device_ms)`, aligned UTC timestamps follow the sign
/// convention:
/// `aligned_utc_ms = device_clock_ms + device_to_utc_offset_ms + ((device_clock_ms - valid_from_device_ms) * linear_drift_ppm) / 1_000_000`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClockAlignmentRecord {
    /// Immutable version of this clock alignment calibration.
    pub alignment_version: VersionId,
    /// Base offset in milliseconds added to `device_clock_ms` at `valid_from_device_ms` to obtain
    /// UTC milliseconds (`[-MAX_ABS_CLOCK_OFFSET_MS, MAX_ABS_CLOCK_OFFSET_MS]`).
    pub device_to_utc_offset_ms: i64,
    /// Linear clock drift in parts per million relative to `valid_from_device_ms`
    /// (`[-MAX_ABS_CLOCK_DRIFT_PPM, MAX_ABS_CLOCK_DRIFT_PPM]`).
    pub linear_drift_ppm: i32,
    /// Declared alignment uncertainty (± milliseconds, `> 0`).
    pub uncertainty_ms: u32,
    /// Reference clock identifier or label (for example `"GNSS-PPS"` or `"ground-station-NTP"`).
    pub reference_clock_label: String,
    /// Alignment estimation method (for example `"pre-flight-optical-timecode"`).
    pub method_label: String,
    /// Inclusive start of the validity window on the device clock (`ms`).
    pub valid_from_device_ms: i64,
    /// Exclusive end of the validity window on the device clock (`ms`, `> valid_from_device_ms`).
    pub valid_until_device_ms: i64,
}

/// Why a device timestamp could not be aligned to UTC (`docs/architecture.md` §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsynchronizedReason {
    /// No clock alignment record has been configured for the session.
    NoClockAlignmentConfigured,
    /// The device timestamp fell outside the configured alignment validity interval
    /// `[valid_from_device_ms, valid_until_device_ms)`.
    OutsideAlignmentValidityWindow {
        /// Inclusive start of the configured validity interval.
        valid_from_device_ms: i64,
        /// Exclusive end of the configured validity interval.
        valid_until_device_ms: i64,
    },
    /// Checked addition of `device_clock_ms + device_to_utc_offset_ms + drift_adjustment_ms`
    /// overflowed `i64`; alignment is withheld rather than returning a clamped timestamp.
    AlignmentArithmeticOverflow,
}

/// Result of aligning a device-clock timestamp (`DEVICE-04`, `docs/architecture.md` §3.1).
///
/// Loss of synchronization degrades confidence explicitly rather than aligning frames by receipt
/// time as if capture time were known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimeAlignment {
    /// Device timestamp was aligned within a valid [`ClockAlignmentRecord`] interval.
    Synchronized {
        /// Aligned capture/sample timestamp in UTC milliseconds.
        aligned_utc_ms: i64,
        /// Pinned version of the [`ClockAlignmentRecord`] used.
        alignment_version: VersionId,
        /// Base offset in milliseconds (`device_to_utc_offset_ms`).
        base_offset_ms: i64,
        /// Computed linear drift adjustment in milliseconds over the elapsed window.
        drift_adjustment_ms: i64,
        /// Declared alignment uncertainty (± milliseconds).
        total_uncertainty_ms: u32,
    },
    /// Synchronization was absent or outside the validity interval; raw device clock and receipt
    /// times are kept distinct without pretending receipt time is capture time.
    UnsynchronizedDegraded {
        /// Unaligned device-clock timestamp in milliseconds.
        device_clock_ms: i64,
        /// Ground-station or importer receipt timestamp in UTC milliseconds.
        receipt_utc_ms: i64,
        /// Reason synchronization could not be established.
        reason: UnsynchronizedReason,
    },
}

impl TimeAlignment {
    /// Returns `true` only when the timestamp was aligned within a valid clock-alignment window.
    pub fn is_synchronized(&self) -> bool {
        matches!(self, Self::Synchronized { .. })
    }
}

/// North reference for a UAV heading measurement (`DEVICE-02`, `docs/architecture.md` §3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadingReference {
    /// Geodetic true north.
    TrueNorth,
    /// Local magnetic north without declination correction.
    MagneticNorth,
}

/// Validated aircraft heading in degrees `[0.0, 360.0)` with explicit north reference.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeadingSample {
    /// Heading angle in degrees `[0.0, 360.0)`.
    pub degrees: f64,
    /// North reference.
    pub reference: HeadingReference,
}

/// Validated aircraft attitude Euler angles in degrees (`DEVICE-02`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AttitudeDeg {
    /// Bank/roll angle in degrees `[-180.0, 180.0]`.
    pub roll_deg: f64,
    /// Pitch angle in degrees `[-90.0, 90.0]` (positive nose up).
    pub pitch_deg: f64,
    /// Yaw angle in degrees `[0.0, 360.0)`.
    pub yaw_deg: f64,
}

/// Validated WGS84 horizontal position (`DEVICE-02`, `docs/architecture.md` §3.2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wgs84Position {
    /// Geodetic latitude in degrees `[-90.0, 90.0]`.
    pub latitude_deg: f64,
    /// Geodetic longitude in degrees `[-180.0, 180.0]`.
    pub longitude_deg: f64,
    /// Optional 1-sigma horizontal position uncertainty in meters (`>= 0.0`).
    pub horizontal_uncertainty_m: Option<f64>,
}

/// Vertical datum or height reference for an altitude measurement (`DEVICE-02`, `docs/architecture.md` §3.2).
///
/// Barometric altitude must not be treated as physically corrected orthometric or ellipsoidal
/// height by merely adding geoid undulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AltitudeReference {
    /// Height above the WGS84 reference ellipsoid.
    Wgs84Ellipsoid,
    /// Orthometric height above mean sea level (geoid-referenced).
    OrthometricMsl,
    /// Measured or terrain-referenced height above local ground level (AGL).
    AboveGroundLevel,
    /// Uncorrected barometric pressure altitude.
    BarometricUnadjusted,
    /// Height relative to the takeoff or home point.
    RelativeToTakeoff,
    /// Unlabeled or unverified altitude datum (`docs/architecture.md` §3.2).
    Unknown,
}

/// Validated altitude measurement in meters with explicit [`AltitudeReference`] (`DEVICE-02`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AltitudeSample {
    /// Altitude in meters (`[-1_000.0, 100_000.0]`).
    pub meters: f64,
    /// Explicit vertical reference.
    pub reference: AltitudeReference,
    /// Optional 1-sigma vertical uncertainty in meters (`>= 0.0`).
    pub vertical_uncertainty_m: Option<f64>,
}

impl AltitudeSample {
    /// Extracts the altitude value in the requested [`AltitudeReference`], or fails with
    /// [`UavError::IncompatibleAltitudeReference`] if the sample carries a different reference or
    /// [`AltitudeReference::Unknown`] (`docs/architecture.md` §3.2).
    pub fn try_in_reference(&self, requested: AltitudeReference) -> Result<f64, UavError> {
        if self.reference == AltitudeReference::Unknown
            || requested == AltitudeReference::Unknown
            || self.reference != requested
        {
            return Err(UavError::IncompatibleAltitudeReference {
                actual: self.reference,
                requested,
            });
        }
        Ok(self.meters)
    }
}

/// Validated airframe ground and vertical speeds in meters per second (`DEVICE-02`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VelocityMps {
    /// Horizontal ground speed in meters per second (`[0.0, 500.0]`).
    pub ground_speed_mps: f64,
    /// Optional vertical climb/descent rate in meters per second (`[-200.0, 200.0]`, positive up).
    pub vertical_speed_mps: Option<f64>,
}

/// Gimbal stabilization and angle reference frame (`DEVICE-02`, `DEVICE-04`, `A12`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GimbalMode {
    /// Body-coupled mode: `pitch_deg`, `roll_deg`, and `yaw_deg` are relative to the airframe nose
    /// and body attitude (`AttitudeDeg` and `HeadingSample`).
    FollowHeading,
    /// World-stabilized mode: `pitch_deg` and `roll_deg` are stabilized relative to the local
    /// horizontal plane and `yaw_deg` is stabilized relative to geodetic [`HeadingReference::TrueNorth`]
    /// (`A12`). Airframe pitch, roll, and heading are already compensated by the gimbal.
    IndependentWorldLocked,
}

/// Validated gimbal orientation in degrees (`DEVICE-02`, `DEVICE-04`, `A12`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GimbalOrientation {
    /// Gimbal pitch in degrees `[-120.0, 45.0]` (negative = looking downward; interpreted in the
    /// [`GimbalMode`] frame; angles past `-90.0°` fold across nadir onto `azimuth + 180.0°`).
    pub pitch_deg: f64,
    /// Gimbal roll in degrees `[-90.0, 90.0]` (interpreted in the [`GimbalMode`] frame).
    pub roll_deg: f64,
    /// Gimbal yaw in degrees `[-180.0, 360.0)` (relative to airframe nose in
    /// [`GimbalMode::FollowHeading`] or geodetic true north in [`GimbalMode::IndependentWorldLocked`]).
    pub yaw_deg: f64,
    /// Coordinate frame and stabilization mode for `pitch_deg`, `roll_deg`, and `yaw_deg`.
    pub mode: GimbalMode,
}

/// Validated battery state (`DEVICE-02`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BatterySample {
    /// Remaining state of charge in percent `[0.0, 100.0]`.
    pub remaining_percent: f64,
    /// Optional pack voltage in volts `(0.0, 100.0]`.
    pub voltage_v: Option<f64>,
}

/// Telemetry and video link health state (`DEVICE-02`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkState {
    /// Link is operating normally.
    Nominal,
    /// Link is experiencing elevated packet loss or low signal margin.
    Degraded,
    /// Link is disconnected.
    Disconnected,
}

/// Validated link health readout (`DEVICE-02`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinkHealth {
    /// Link state.
    pub state: LinkState,
    /// Optional received signal strength indicator in dBm `[-150.0, 20.0]`.
    pub rssi_dbm: Option<f64>,
    /// Optional packet loss percentage `[0.0, 100.0]`.
    pub packet_loss_percent: Option<f64>,
}

/// GNSS/navigation fix classification (`DEVICE-02`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GnssFixKind {
    /// No position fix; horizontal/vertical position must not be fabricated.
    NoFix,
    /// Standard 2D/3D GNSS fix.
    Standard3D,
    /// Differential or RTK float/fixed solution.
    DifferentialOrRtk,
    /// Dead-reckoning or optical-flow aided estimate without fresh GNSS fix.
    DeadReckoningEstimate,
}

/// Validated navigation health readout (`DEVICE-02`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NavigationHealth {
    /// Navigation fix classification.
    pub fix_kind: GnssFixKind,
    /// Optional tracked satellite count.
    pub satellite_count: Option<u8>,
    /// Optional horizontal dilution of precision `(0.0, 100.0]`.
    pub hdop: Option<f64>,
}

/// Onboard recording state reported by the aircraft (`DEVICE-02`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingState {
    /// Aircraft reports active onboard media recording.
    Recording,
    /// Aircraft reports onboard recorder is idle.
    Idle,
    /// Onboard recording status is not reported by this telemetry frame.
    Unavailable,
}

/// Moving-camera dynamic disturbance flags preserved for downstream forensic analysis
/// (`DEVICE-04`: "Moving-camera analyses must account for vibration, gimbal motion, parallax,
/// and exposure changes").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MotionDynamicsFlags {
    /// Airframe vibration exceeded nominal threshold during this sample.
    pub high_vibration: bool,
    /// Gimbal was actively slewing independently of the airframe during this sample.
    pub rapid_gimbal_slew: bool,
    /// Fast angular motion or vibration introduces rolling-shutter distortion risk.
    pub rolling_shutter_risk: bool,
    /// Auto-exposure or gain changed abruptly across this sample.
    pub exposure_shift_detected: bool,
}

/// Single telemetry sample ingested from a live or recorded UAV flight (`DEVICE-02`, `DEVICE-04`).
///
/// Missing fields are represented as `None` and remain [`HudFieldState::Unavailable`] in HUD
/// projections rather than defaulting to `0.0` or holding stale values silently (`AGENTS.md` §2).
#[derive(Debug, Clone, PartialEq)]
pub struct TelemetrySampleInput {
    /// Immutable version identifier of this telemetry sample record.
    pub sample_version: VersionId,
    /// Monotonic aircraft/device clock timestamp in milliseconds.
    pub device_clock_ms: i64,
    /// Ground-station or importer receipt timestamp in UTC milliseconds.
    pub receipt_utc_ms: i64,
    /// Optional heading measurement.
    pub heading: Option<HeadingSample>,
    /// Optional airframe attitude measurement.
    pub attitude: Option<AttitudeDeg>,
    /// Optional WGS84 horizontal position.
    pub position: Option<Wgs84Position>,
    /// Optional altitude measurement with explicit [`AltitudeReference`].
    pub altitude: Option<AltitudeSample>,
    /// Optional ground/vertical velocity measurement.
    pub velocity: Option<VelocityMps>,
    /// Optional gimbal orientation measurement.
    pub gimbal: Option<GimbalOrientation>,
    /// Optional battery readout.
    pub battery: Option<BatterySample>,
    /// Optional link health readout.
    pub link_health: Option<LinkHealth>,
    /// Optional navigation health readout.
    pub navigation_health: Option<NavigationHealth>,
    /// Reported onboard recording state (`RecordingState::Unavailable` when omitted).
    pub recording_state: RecordingState,
    /// Moving-camera disturbance flags for forensic consumers (`DEVICE-04`).
    pub motion_flags: MotionDynamicsFlags,
}

/// Validated telemetry sample stored in a [`UavFlightSession`].
#[derive(Debug, Clone, PartialEq)]
pub struct TelemetrySampleRecord {
    sample_version: VersionId,
    device_clock_ms: i64,
    receipt_utc_ms: i64,
    time_alignment: TimeAlignment,
    needs_reassessment: bool,
    heading: Option<HeadingSample>,
    attitude: Option<AttitudeDeg>,
    position: Option<Wgs84Position>,
    altitude: Option<AltitudeSample>,
    velocity: Option<VelocityMps>,
    gimbal: Option<GimbalOrientation>,
    battery: Option<BatterySample>,
    link_health: Option<LinkHealth>,
    navigation_health: Option<NavigationHealth>,
    recording_state: RecordingState,
    motion_flags: MotionDynamicsFlags,
}

impl TelemetrySampleRecord {
    /// Immutable sample version.
    pub fn sample_version(&self) -> &VersionId {
        &self.sample_version
    }

    /// Device-clock timestamp in milliseconds.
    pub fn device_clock_ms(&self) -> i64 {
        self.device_clock_ms
    }

    /// Receipt timestamp in UTC milliseconds.
    pub fn receipt_utc_ms(&self) -> i64 {
        self.receipt_utc_ms
    }

    /// Clock alignment pinned when this sample was ingested (`docs/architecture.md` §8).
    pub fn time_alignment(&self) -> &TimeAlignment {
        &self.time_alignment
    }

    /// Returns `true` if a clock alignment covering `device_clock_ms` was attached after this
    /// sample was ingested, marking the pinned `time_alignment` as needing reassessment rather than
    /// silently rewriting it in place (`docs/architecture.md` §8).
    pub fn needs_reassessment(&self) -> bool {
        self.needs_reassessment
    }

    /// Optional heading sample.
    pub fn heading(&self) -> Option<HeadingSample> {
        self.heading
    }

    /// Optional attitude sample.
    pub fn attitude(&self) -> Option<AttitudeDeg> {
        self.attitude
    }

    /// Optional WGS84 position sample.
    pub fn position(&self) -> Option<Wgs84Position> {
        self.position
    }

    /// Optional altitude sample.
    pub fn altitude(&self) -> Option<AltitudeSample> {
        self.altitude
    }

    /// Optional velocity sample.
    pub fn velocity(&self) -> Option<VelocityMps> {
        self.velocity
    }

    /// Optional gimbal orientation sample.
    pub fn gimbal(&self) -> Option<GimbalOrientation> {
        self.gimbal
    }

    /// Optional battery sample.
    pub fn battery(&self) -> Option<BatterySample> {
        self.battery
    }

    /// Optional link health sample.
    pub fn link_health(&self) -> Option<LinkHealth> {
        self.link_health
    }

    /// Optional navigation health sample.
    pub fn navigation_health(&self) -> Option<NavigationHealth> {
        self.navigation_health
    }

    /// Onboard recording state.
    pub fn recording_state(&self) -> RecordingState {
        self.recording_state
    }

    /// Moving-camera disturbance flags.
    pub fn motion_flags(&self) -> MotionDynamicsFlags {
        self.motion_flags
    }
}

/// Input metadata for an ingested video frame (`DEVICE-03`, `DEVICE-04`, `A12`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoFrameInput {
    /// Zero-based sequential frame index in the source stream.
    pub frame_index: u64,
    /// Immutable version identifier of this extracted or indexed frame record.
    pub frame_version: VersionId,
    /// Monotonic device-clock capture timestamp in milliseconds.
    pub device_clock_ms: i64,
    /// Ground-station or importer receipt timestamp in UTC milliseconds.
    pub receipt_utc_ms: i64,
    /// Optional shutter exposure duration in microseconds (`> 0`).
    pub exposure_duration_us: Option<u32>,
    /// Lowercase 64-character hexadecimal SHA-256 digest of the unaltered frame bytes.
    pub sha256_hex: String,
}

/// Validated video frame metadata record in a [`UavFlightSession`].
#[derive(Debug, Clone, PartialEq)]
pub struct VideoFrameRecord {
    frame_index: u64,
    frame_version: VersionId,
    device_clock_ms: i64,
    receipt_utc_ms: i64,
    time_alignment: TimeAlignment,
    camera_calibration_state_at_capture: CameraCalibrationState,
    needs_reassessment: bool,
    exposure_duration_us: Option<u32>,
    sha256_hex: String,
}

impl VideoFrameRecord {
    /// Frame index in the source stream.
    pub fn frame_index(&self) -> u64 {
        self.frame_index
    }

    /// Immutable frame version identifier.
    pub fn frame_version(&self) -> &VersionId {
        &self.frame_version
    }

    /// Device-clock timestamp in milliseconds.
    pub fn device_clock_ms(&self) -> i64 {
        self.device_clock_ms
    }

    /// Receipt timestamp in UTC milliseconds.
    pub fn receipt_utc_ms(&self) -> i64 {
        self.receipt_utc_ms
    }

    /// Clock alignment status pinned when this frame was ingested (`docs/architecture.md` §8).
    pub fn time_alignment(&self) -> &TimeAlignment {
        &self.time_alignment
    }

    /// Camera calibration state pinned when this frame was ingested (`docs/architecture.md` §8).
    pub fn camera_calibration_state_at_capture(&self) -> &CameraCalibrationState {
        &self.camera_calibration_state_at_capture
    }

    /// Returns `true` if a clock alignment or camera calibration/invalidation covering
    /// `device_clock_ms` was attached after this frame was ingested, marking its pinned capture
    /// metadata as needing reassessment rather than silently rewriting it (`docs/architecture.md` §8).
    pub fn needs_reassessment(&self) -> bool {
        self.needs_reassessment
    }

    /// Optional shutter exposure duration in microseconds.
    pub fn exposure_duration_us(&self) -> Option<u32> {
        self.exposure_duration_us
    }

    /// Lowercase 64-character hexadecimal SHA-256 digest of the unaltered frame bytes.
    pub fn sha256_hex(&self) -> &str {
        &self.sha256_hex
    }
}

/// Explicit record of dropped video frames (`DEVICE-04`, `docs/architecture.md` §8, `A12`).
///
/// Playback or analysis must never conceal missing frames by interpolating or repeating prior
/// frames as if they were observed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameDropSpan {
    /// Last observed frame index prior to the drop, if any.
    pub after_frame_index: Option<u64>,
    /// Number of consecutive missing frames (`> 0`).
    pub dropped_frame_count: u32,
    /// Device-clock timestamp (in milliseconds) at which the drop gap was detected.
    pub detected_at_device_ms: i64,
    /// Human-inspectable reason (for example `"RTP sequence gap on downlink"`).
    pub reason: String,
}

/// Explicit record of a telemetry link disconnection interval (`DEVICE-04`, `A12`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryDisconnectionSpan {
    /// Device-clock timestamp (in milliseconds) when telemetry reception was lost.
    pub disconnected_at_device_ms: i64,
    /// Optional device-clock timestamp (in milliseconds, `> disconnected_at_device_ms`) when
    /// telemetry reception resumed. `None` indicates an ongoing disconnection.
    pub reconnected_at_device_ms: Option<i64>,
    /// Human-inspectable reason (for example `"LOS terrain masking"`).
    pub reason: String,
}

/// Camera lens and mount calibration parameters used for ground-footprint estimation
/// (`DEVICE-02`, `FIELD-03`, `A12`).
#[derive(Debug, Clone, PartialEq)]
pub struct CameraCalibration {
    /// Immutable version of the camera calibration record.
    pub calibration_version: VersionId,
    /// Device-clock timestamp (in milliseconds) from which this calibration is in force (`docs/architecture.md` §8).
    pub effective_from_device_ms: i64,
    /// Horizontal field of view in degrees `(1.0, 170.0)`.
    pub horizontal_fov_deg: f64,
    /// Vertical field of view in degrees `(1.0, 170.0)`.
    pub vertical_fov_deg: f64,
    /// Combined 1-sigma angular alignment uncertainty in degrees `[0.0, 45.0]`.
    pub angular_uncertainty_deg: f64,
}

/// Current camera calibration validity state for a [`UavFlightSession`] (`DEVICE-02`, `A12`, `A13`).
#[derive(Debug, Clone, PartialEq)]
pub enum CameraCalibrationState {
    /// No camera calibration has been attached to the session at the queried timestamp.
    Uncalibrated,
    /// Active validated calibration version.
    Valid(CameraCalibration),
    /// Previous calibration was invalidated by camera/mount movement or mechanical shock.
    Invalidated {
        /// Version of the calibration that was invalidated.
        calibration_version: VersionId,
        /// Device-clock timestamp (in milliseconds) at which the invalidation took effect.
        effective_from_device_ms: i64,
        /// Reason the calibration is no longer valid.
        reason: String,
    },
}

impl CameraCalibrationState {
    /// Returns the device-clock timestamp (`ms`) from which this state took effect, or `None` for
    /// [`CameraCalibrationState::Uncalibrated`].
    pub fn effective_from_device_ms(&self) -> Option<i64> {
        match self {
            Self::Uncalibrated => None,
            Self::Valid(cal) => Some(cal.effective_from_device_ms),
            Self::Invalidated {
                effective_from_device_ms,
                ..
            } => Some(*effective_from_device_ms),
        }
    }
}

/// Why a HUD telemetry field is unavailable at the inspection timestamp (`DEVICE-02`, `AGENTS.md` §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnavailableReason {
    /// The registered device does not declare [`UavCapability::TelemetryStream`].
    TelemetryCapabilityNotDeclared,
    /// No telemetry sample has been received at or before the inspection timestamp.
    NoSampleReceivedAtOrBeforeInstant,
    /// A telemetry sample was received, but this specific field was absent (`None`).
    MissingInSample {
        /// Device-clock timestamp of the sample in which the field was omitted.
        sample_device_ms: i64,
    },
}

/// Why a HUD telemetry field is marked stale rather than fresh (`DEVICE-02`, `A12`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaleReason {
    /// Sample age exceeded `stale_telemetry_after_ms`.
    AgeExceededThreshold {
        /// Configured staleness threshold in milliseconds.
        threshold_ms: i64,
    },
    /// A telemetry link disconnection occurred after the sample and covers or precedes the
    /// inspection instant.
    TelemetryLinkDisconnected {
        /// Device-clock timestamp when the disconnection began.
        disconnected_at_device_ms: i64,
    },
}

/// Freshness-qualified HUD field state (`DEVICE-02`, `A12`).
///
/// Missing values stay [`HudFieldState::Unavailable`] and stale values carry their sample timestamp
/// and age rather than masquerading as current observations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HudFieldState<T> {
    /// Field was reported in the latest applicable sample and is within the freshness window.
    Fresh {
        /// Reported value.
        value: T,
        /// Device-clock timestamp of the sample (`ms`).
        sample_device_ms: i64,
        /// Age of the sample at the inspection instant (`ms`, `>= 0`).
        age_ms: i64,
    },
    /// Field was reported in the latest applicable sample, but that sample is stale or separated by
    /// a link disconnection.
    Stale {
        /// Last reported value (explicitly flagged as stale).
        value: T,
        /// Device-clock timestamp of the sample (`ms`).
        sample_device_ms: i64,
        /// Age of the sample at the inspection instant (`ms`).
        age_ms: i64,
        /// Reason the sample is stale.
        reason: StaleReason,
    },
    /// Field is unavailable at the inspection instant; no default or zero value is substituted.
    Unavailable {
        /// Reason the field is unavailable.
        reason: UnavailableReason,
    },
}

impl<T> HudFieldState<T> {
    /// Returns `true` if the field is [`HudFieldState::Fresh`].
    pub fn is_fresh(&self) -> bool {
        matches!(self, Self::Fresh { .. })
    }

    /// Returns `true` if the field is [`HudFieldState::Stale`].
    pub fn is_stale(&self) -> bool {
        matches!(self, Self::Stale { .. })
    }

    /// Returns `true` if the field is [`HudFieldState::Unavailable`].
    pub fn is_unavailable(&self) -> bool {
        matches!(self, Self::Unavailable { .. })
    }

    /// Returns the underlying value only when [`HudFieldState::Fresh`].
    pub fn fresh_value(&self) -> Option<&T> {
        match self {
            Self::Fresh { value, .. } => Some(value),
            Self::Stale { .. } | Self::Unavailable { .. } => None,
        }
    }
}

/// Uncalibrated or directional camera viewing marker (`DEVICE-02`: "A viewing marker does not
/// itself measure range").
#[allow(clippy::manual_non_exhaustive)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LookDirectionMarker {
    /// Derived world azimuth of the camera optical axis in degrees `[0.0, 360.0)`.
    pub world_azimuth_deg: f64,
    /// Derived depression angle below the local horizontal plane in degrees `[-90.0, 90.0]`
    /// (positive = looking downward).
    pub depression_deg: f64,
    /// North reference for `world_azimuth_deg` ([`HeadingReference::TrueNorth`] in
    /// [`GimbalMode::IndependentWorldLocked`] or inherited from the heading sample in
    /// [`GimbalMode::FollowHeading`]).
    pub azimuth_reference: HeadingReference,
    /// `true` when the optical axis points vertically at nadir or zenith (`|depression_deg| == 90.0°`),
    /// where horizontal world azimuth is geometrically degenerate (`N1`).
    pub azimuth_undefined_at_nadir: bool,
    /// Whether any telemetry input used to derive this ray was stale.
    pub derived_from_stale_input: bool,
    /// Always `true`: the viewing ray is a derived kinematic estimate, not a direct optical or
    /// rangefinder target measurement (`DEVICE-02`, `AGENTS.md` §2).
    pub is_derived_estimate: bool,
    /// Always `false`: a directional viewing marker never measures slant range (`DEVICE-02`).
    pub establishes_slant_range: bool,
    _sealed: (),
}

/// Derived flat-ground camera footprint estimate when valid calibration and `AboveGroundLevel`
/// clearance are available (`DEVICE-02`, `A12`).
#[allow(clippy::manual_non_exhaustive)]
#[derive(Debug, Clone, PartialEq)]
pub struct CalibratedFootprint {
    /// Pinned camera calibration version used for this projection.
    pub calibration_version: VersionId,
    /// North reference for `center_north_offset_m` and `center_east_offset_m`
    /// (always [`HeadingReference::TrueNorth`]).
    pub azimuth_reference: HeadingReference,
    /// Derived optical-axis slant range to the flat ground plane in meters (`> 0.0`).
    pub center_slant_range_m: f64,
    /// Derived horizontal ground distance from the nadir point to the footprint center (`m`).
    pub center_ground_distance_m: f64,
    /// Derived true-north offset from aircraft nadir to footprint center (`m`).
    pub center_north_offset_m: f64,
    /// Derived true-east offset from aircraft nadir to footprint center (`m`).
    pub center_east_offset_m: f64,
    /// Approximate along-look ground span in meters (`> 0.0`), preserving opposite-side ground
    /// intersection when the camera frustum spans across nadir.
    pub along_look_span_m: f64,
    /// Approximate cross-look ground span in meters (`> 0.0`).
    pub cross_look_span_m: f64,
    /// First-order linear horizontal footprint center uncertainty bound in meters (`>= 0.0`),
    /// combining horizontal GNSS uncertainty, vertical AGL uncertainty projected by `|cot(dep)|`,
    /// and angular calibration uncertainty projected along-track by `slant_range / sin(dep)`.
    pub horizontal_uncertainty_m: f64,
    /// Explicit assumption disclosure (`docs/architecture.md` §§3.2, 8).
    pub terrain_assumption: &'static str,
    /// Always `true`: footprint geometry is a derived model estimate, not a direct rangefinder
    /// observation (`DEVICE-02`, `AGENTS.md` §2).
    pub is_derived_estimate: bool,
    _sealed: (),
}

/// Outcome of evaluating the camera ground footprint for a HUD snapshot (`DEVICE-02`, `A12`).
#[derive(Debug, Clone, PartialEq)]
pub enum FootprintStatus {
    /// The registered device does not declare [`UavCapability::TelemetryStream`].
    TelemetryCapabilityNotDeclared,
    /// No camera calibration is attached; only a directional viewing ray is available.
    UncalibratedViewingRayOnly,
    /// Camera calibration was invalidated by camera/mount movement or mechanical shock (`A12`, `A13`).
    CalibrationInvalidated {
        /// Invalidated calibration version.
        calibration_version: VersionId,
        /// Device-clock timestamp (in milliseconds) at which the invalidation took effect.
        effective_from_device_ms: i64,
        /// Recorded invalidation reason.
        reason: String,
    },
    /// Required telemetry for footprint projection (`heading`, `attitude`, `position`, `altitude`,
    /// `gimbal`, or `navigation_health`) is unavailable or stale.
    InsufficientFreshTelemetry {
        /// Name of the first missing or stale required field.
        missing_or_stale_field: &'static str,
    },
    /// Heading was reported in [`HeadingReference::MagneticNorth`] without declination correction
    /// while the gimbal is in [`GimbalMode::FollowHeading`], so true-north ground offsets cannot be
    /// resolved without inventing magnetic declination (`docs/architecture.md` §3.2).
    IncompatibleHeadingReference {
        /// Reported heading reference.
        actual: HeadingReference,
    },
    /// Altitude was reported in a datum that does not establish clearance above local terrain
    /// (for example `BarometricUnadjusted`, `OrthometricMsl`, `Wgs84Ellipsoid`, or `Unknown`).
    IncompatibleAltitudeReference {
        /// Reported altitude reference.
        actual: AltitudeReference,
    },
    /// Reported `AboveGroundLevel` height is non-positive (`<= 0.0 m`).
    NonPositiveHeightAboveGround {
        /// Reported AGL height in meters.
        height_m: f64,
    },
    /// Composed image roll modulo 180 degrees is non-zero (`|roll_deg| > 1e-6`), so an
    /// unrotated along-look/cross-look pinhole ground span is withheld rather than misprojected.
    NonZeroRollUnsupported {
        /// Composed image roll modulo 180 degrees, in `[-90, 90)`.
        effective_roll_deg: f64,
    },
    /// Optical axis or frustum edge points above the horizon or within the grazing-angle exclusion
    /// zone (`< MIN_FOOTPRINT_DEPRESSION_DEG`), so a unique bounded ground intersection cannot be
    /// resolved (`A12`, `A13`).
    UnresolvableGeometry {
        /// Minimum depression clearance angle below the horizontal plane in degrees.
        depression_deg: f64,
        /// Minimum required depression angle in degrees (`MIN_FOOTPRINT_DEPRESSION_DEG`).
        min_required_depression_deg: f64,
    },
    /// Validated derived ground footprint estimate under a local flat-ground assumption.
    CalibratedDerivedFootprint(CalibratedFootprint),
}

/// Complete capability- and freshness-aware UAV HUD projection at an inspection instant
/// (`DEVICE-02`, `DEVICE-04`, `A12`).
#[allow(clippy::manual_non_exhaustive)]
#[derive(Debug, Clone, PartialEq)]
pub struct UavHudSnapshot {
    /// Pinned capture metadata has been superseded by later evidence and requires review.
    pub needs_reassessment: bool,
    /// Device-clock timestamp (in milliseconds) at which the HUD was projected.
    pub inspect_device_ms: i64,
    /// Clock alignment status at `inspect_device_ms`.
    pub time_alignment: TimeAlignment,
    /// Pinned original stream version.
    pub stream_version: VersionId,
    /// Whether the underlying flight stream is a simulation fixture.
    pub is_simulated_stream: bool,
    /// Heading field state.
    pub heading: HudFieldState<HeadingSample>,
    /// Attitude field state.
    pub attitude: HudFieldState<AttitudeDeg>,
    /// Horizontal position field state.
    pub position: HudFieldState<Wgs84Position>,
    /// Altitude field state (with explicit [`AltitudeReference`]).
    pub altitude: HudFieldState<AltitudeSample>,
    /// Velocity field state.
    pub velocity: HudFieldState<VelocityMps>,
    /// Independent or heading-following gimbal orientation field state.
    pub gimbal: HudFieldState<GimbalOrientation>,
    /// Battery field state.
    pub battery: HudFieldState<BatterySample>,
    /// Link health field state.
    pub link_health: HudFieldState<LinkHealth>,
    /// Navigation health field state.
    pub navigation_health: HudFieldState<NavigationHealth>,
    /// Onboard recording state.
    pub recording_state: HudFieldState<RecordingState>,
    /// Freshness-qualified moving-camera dynamic disturbance flags (`DEVICE-04`).
    pub motion_flags: HudFieldState<MotionDynamicsFlags>,
    /// Directional viewing marker (explicitly marked `establishes_slant_range: false`).
    pub look_direction_marker: Option<LookDirectionMarker>,
    /// Camera ground footprint status (calibrated derived estimate vs. explicit refusal reason).
    pub footprint_status: FootprintStatus,
    /// Total cumulative dropped video frames recorded at or before `inspect_device_ms`.
    pub cumulative_dropped_frames: u64,
    /// Number of telemetry disconnection spans recorded at or before `inspect_device_ms`.
    pub disconnection_span_count: usize,
    _sealed: (),
}

impl UavHudSnapshot {
    /// Renders a deterministic, human-inspectable plain-text HUD readout bounded by
    /// `max_output_bytes`.
    pub fn render_hud_text(&self, max_output_bytes: usize) -> Result<String, UavError> {
        if max_output_bytes == 0 {
            return Err(UavError::ZeroLimit {
                kind: UavLimitKind::OutputBytes,
            });
        }
        let mut out = BoundedWriter::new(max_output_bytes);
        if self.needs_reassessment {
            out.write_str("NEEDS_REASSESSMENT: pinned capture metadata; inspect current calibration/alignment before reuse.\n")?;
        }
        out.write_fmt(format_args!(
            "UAV HUD Snapshot (device_clock_ms={}, stream_version={}, simulated={})\n",
            self.inspect_device_ms,
            self.stream_version.as_str(),
            self.is_simulated_stream
        ))?;
        match &self.time_alignment {
            TimeAlignment::Synchronized {
                aligned_utc_ms,
                alignment_version,
                base_offset_ms,
                drift_adjustment_ms,
                total_uncertainty_ms,
            } => {
                out.write_fmt(format_args!(
                    "  time_alignment: SYNCHRONIZED utc_ms={} (±{} ms, offset={} ms, drift_adj={} ms, cal={})\n",
                    aligned_utc_ms,
                    total_uncertainty_ms,
                    base_offset_ms,
                    drift_adjustment_ms,
                    alignment_version.as_str()
                ))?;
            }
            TimeAlignment::UnsynchronizedDegraded {
                device_clock_ms,
                receipt_utc_ms,
                reason,
            } => {
                out.write_fmt(format_args!(
                    "  time_alignment: UNSYNCHRONIZED_DEGRADED device_ms={} receipt_utc_ms={} reason={:?}\n",
                    device_clock_ms, receipt_utc_ms, reason
                ))?;
            }
        }
        write_hud_line(&mut out, "heading", &self.heading, |h| {
            format!("{:.1} deg ({:?})", h.degrees, h.reference)
        })?;
        write_hud_line(&mut out, "attitude", &self.attitude, |a| {
            format!(
                "roll={:.1} deg, pitch={:.1} deg, yaw={:.1} deg",
                a.roll_deg, a.pitch_deg, a.yaw_deg
            )
        })?;
        write_hud_line(&mut out, "position", &self.position, |p| {
            match p.horizontal_uncertainty_m {
                Some(unc) => format!(
                    "lat={:.6} deg, lon={:.6} deg (±{:.1} m WGS84)",
                    p.latitude_deg, p.longitude_deg, unc
                ),
                None => format!(
                    "lat={:.6} deg, lon={:.6} deg (WGS84, uncertainty unrecorded)",
                    p.latitude_deg, p.longitude_deg
                ),
            }
        })?;
        write_hud_line(&mut out, "altitude", &self.altitude, |alt| {
            match alt.vertical_uncertainty_m {
                Some(unc) => format!("{:.1} m {:?} (±{:.1} m)", alt.meters, alt.reference, unc),
                None => format!("{:.1} m {:?}", alt.meters, alt.reference),
            }
        })?;
        write_hud_line(&mut out, "velocity", &self.velocity, |v| {
            match v.vertical_speed_mps {
                Some(vz) => format!(
                    "ground={:.1} m/s, vertical={:.1} m/s",
                    v.ground_speed_mps, vz
                ),
                None => format!("ground={:.1} m/s, vertical=UNAVAILABLE", v.ground_speed_mps),
            }
        })?;
        write_hud_line(&mut out, "gimbal", &self.gimbal, |g| {
            format!(
                "pitch={:.1} deg, roll={:.1} deg, yaw={:.1} deg ({:?})",
                g.pitch_deg, g.roll_deg, g.yaw_deg, g.mode
            )
        })?;
        write_hud_line(&mut out, "battery", &self.battery, |b| match b.voltage_v {
            Some(v) => format!("{:.1}% ({:.2} V)", b.remaining_percent, v),
            None => format!("{:.1}% (voltage UNAVAILABLE)", b.remaining_percent),
        })?;
        write_hud_line(&mut out, "link_health", &self.link_health, |l| {
            format!(
                "{:?} (rssi_dbm={:?}, loss_pct={:?})",
                l.state, l.rssi_dbm, l.packet_loss_percent
            )
        })?;
        write_hud_line(
            &mut out,
            "navigation_health",
            &self.navigation_health,
            |n| {
                format!(
                    "{:?} (sats={:?}, hdop={:?})",
                    n.fix_kind, n.satellite_count, n.hdop
                )
            },
        )?;
        write_hud_line(&mut out, "recording", &self.recording_state, |r| {
            format!("{:?}", r)
        })?;
        write_hud_line(&mut out, "motion_flags", &self.motion_flags, |m| {
            format!(
                "vibration={}, gimbal_slew={}, rolling_shutter={}, exposure_shift={}",
                m.high_vibration,
                m.rapid_gimbal_slew,
                m.rolling_shutter_risk,
                m.exposure_shift_detected
            )
        })?;
        match &self.look_direction_marker {
            Some(marker) => {
                out.write_fmt(format_args!(
                    "  look_marker: DERIVED azimuth={:.1} deg ({:?}), depression={:.1} deg, azimuth_undefined_at_nadir={}, stale_input={}, establishes_slant_range={}\n",
                    marker.world_azimuth_deg,
                    marker.azimuth_reference,
                    marker.depression_deg,
                    marker.azimuth_undefined_at_nadir,
                    marker.derived_from_stale_input,
                    marker.establishes_slant_range
                ))?;
            }
            None => {
                out.write_str("  look_marker: UNAVAILABLE\n")?;
            }
        }
        match &self.footprint_status {
            FootprintStatus::CalibratedDerivedFootprint(fp) => {
                out.write_fmt(format_args!(
                    "  footprint: DERIVED_ESTIMATE cal={} ref={:?} slant_range={:.1} m ground_dist={:.1} m offset_ne=({:.1} m, {:.1} m) span=({:.1} m x {:.1} m) ±{:.1} m [{}]\n",
                    fp.calibration_version.as_str(),
                    fp.azimuth_reference,
                    fp.center_slant_range_m,
                    fp.center_ground_distance_m,
                    fp.center_north_offset_m,
                    fp.center_east_offset_m,
                    fp.along_look_span_m,
                    fp.cross_look_span_m,
                    fp.horizontal_uncertainty_m,
                    fp.terrain_assumption
                ))?;
            }
            other => {
                out.write_fmt(format_args!(
                    "  footprint: NO_UNIQUE_FOOTPRINT ({:?})\n",
                    other
                ))?;
            }
        }
        out.write_fmt(format_args!(
            "  drops_and_disconnections: cumulative_dropped_frames={}, disconnection_spans={}\n",
            self.cumulative_dropped_frames, self.disconnection_span_count
        ))?;
        Ok(out.finish())
    }
}

fn write_hud_line<T, F>(
    out: &mut BoundedWriter,
    label: &str,
    state: &HudFieldState<T>,
    fmt_val: F,
) -> Result<(), UavError>
where
    F: FnOnce(&T) -> String,
{
    match state {
        HudFieldState::Fresh {
            value,
            sample_device_ms,
            age_ms,
        } => out.write_fmt(format_args!(
            "  {}: FRESH [sample_ms={}, age={} ms] {}\n",
            label,
            sample_device_ms,
            age_ms,
            fmt_val(value)
        )),
        HudFieldState::Stale {
            value,
            sample_device_ms,
            age_ms,
            reason,
        } => out.write_fmt(format_args!(
            "  {}: STALE [sample_ms={}, age={} ms, reason={:?}] {}\n",
            label,
            sample_device_ms,
            age_ms,
            reason,
            fmt_val(value)
        )),
        HudFieldState::Unavailable { reason } => {
            out.write_fmt(format_args!("  {}: UNAVAILABLE ({:?})\n", label, reason))
        }
    }
}

/// Input for attaching a synchronized event bookmark to an investigation case (`DEVICE-03`, `A12`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlightBookmarkInput {
    /// Unique identifier for the bookmark.
    pub bookmark_id: OpaqueId,
    /// Target case identifier to which this bookmark is attached.
    pub case_id: OpaqueId,
    /// Actor recording the bookmark.
    pub created_by: ActorId,
    /// Device-clock timestamp (in milliseconds) of the bookmarked flight event.
    pub device_clock_ms: i64,
    /// Receipt timestamp in UTC milliseconds.
    pub receipt_utc_ms: i64,
    /// Optional zero-based video frame index pinned by this bookmark.
    pub pinned_frame_index: Option<u64>,
    /// Short single-line title of the bookmark.
    pub label: String,
    /// Optional single-line analytical or observation note.
    pub note: Option<String>,
}

/// Immutable flight bookmark bound to the original stream version used when it was created
/// (`DEVICE-03`, `A12`).
///
/// Later linking of higher-quality onboard recordings adds an [`OnboardRecordingLink`] without
/// overwriting `pinned_stream_version` or `pinned_frame_version`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlightBookmark {
    needs_reassessment: bool,
    bookmark_id: OpaqueId,
    case_id: OpaqueId,
    created_by: ActorId,
    device_clock_ms: i64,
    time_alignment: TimeAlignment,
    pinned_stream_version: VersionId,
    pinned_frame_index: Option<u64>,
    pinned_frame_version: Option<VersionId>,
    label: String,
    note: Option<String>,
}

impl FlightBookmark {
    /// The pinned alignment differs from subsequently attached clock evidence.
    pub fn needs_reassessment(&self) -> bool {
        self.needs_reassessment
    }

    /// Bookmark identifier.
    pub fn bookmark_id(&self) -> &OpaqueId {
        &self.bookmark_id
    }

    /// Target case identifier.
    pub fn case_id(&self) -> &OpaqueId {
        &self.case_id
    }

    /// Actor who recorded the bookmark.
    pub fn created_by(&self) -> &ActorId {
        &self.created_by
    }

    /// Device-clock timestamp in milliseconds.
    pub fn device_clock_ms(&self) -> i64 {
        self.device_clock_ms
    }

    /// Clock alignment status captured at the bookmark timestamp.
    pub fn time_alignment(&self) -> &TimeAlignment {
        &self.time_alignment
    }

    /// Immutable version of the original flight stream pinned by this bookmark.
    pub fn pinned_stream_version(&self) -> &VersionId {
        &self.pinned_stream_version
    }

    /// Optional pinned frame index.
    pub fn pinned_frame_index(&self) -> Option<u64> {
        self.pinned_frame_index
    }

    /// Optional immutable frame version pinned by this bookmark.
    pub fn pinned_frame_version(&self) -> Option<&VersionId> {
        self.pinned_frame_version.as_ref()
    }

    /// Bookmark label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Optional bookmark note.
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }
}

/// Self-contained, immutable frame-copy forensic handoff package (`DEVICE-03`, `DEVICE-04`, `A12`).
///
/// Exporting a [`ForensicFrameHandoff`] copies the frame metadata, HUD/telemetry snapshot, clock
/// alignment, camera calibration state at capture time, and moving-camera disturbance flags by
/// value so ongoing telemetry and frame ingestion on [`UavFlightSession`] never blocks or mutates
/// the handoff.
#[derive(Debug, Clone, PartialEq)]
pub struct ForensicFrameHandoff {
    handoff_id: OpaqueId,
    session_id: OpaqueId,
    source_device_id: OpaqueId,
    source_stream_version: VersionId,
    source_stream_sha256_hex: String,
    source_stream_byte_len: u64,
    frame: VideoFrameRecord,
    linked_bookmark_id: Option<OpaqueId>,
    camera_calibration_state_at_frame: CameraCalibrationState,
    current_camera_calibration_state_at_frame: CameraCalibrationState,
    current_time_alignment_at_frame: TimeAlignment,
    frame_drops_at_or_before_frame: Vec<FrameDropSpan>,
    hud_snapshot_at_frame: UavHudSnapshot,
    parallax_and_range_caveat: &'static str,
}

/// Mandatory moving-camera parallax and uncalibrated-range caveat attached to every forensic
/// frame handoff (`DEVICE-02`, `DEVICE-04`, `MEDIA-02`).
pub const MOVING_CAMERA_FORENSIC_CAVEAT: &str = "Moving-camera frame handoff: apparent target motion includes airframe translation/parallax, attitude changes, gimbal slew, vibration, rolling shutter, and exposure shifts; a directional viewing marker alone does not establish slant range.";

impl ForensicFrameHandoff {
    /// Reassessment status at export. Earlier exported copies remain immutable; consult the
    /// session's frame record or export again to discover subsequently attached corrections.
    pub fn needs_reassessment(&self) -> bool {
        self.frame.needs_reassessment()
    }

    /// Current dated interpretation at export, distinct from the capture-pinned camera state.
    pub fn current_camera_calibration_state_at_frame(&self) -> &CameraCalibrationState {
        &self.current_camera_calibration_state_at_frame
    }

    /// Current clock interpretation at export, distinct from the frame's pinned alignment.
    pub fn current_time_alignment_at_frame(&self) -> &TimeAlignment {
        &self.current_time_alignment_at_frame
    }

    /// Handoff identifier.
    pub fn handoff_id(&self) -> &OpaqueId {
        &self.handoff_id
    }

    /// Session identifier from which the frame was exported.
    pub fn session_id(&self) -> &OpaqueId {
        &self.session_id
    }

    /// Registered device identifier.
    pub fn source_device_id(&self) -> &OpaqueId {
        &self.source_device_id
    }

    /// Pinned original stream version from which the frame was copied.
    pub fn source_stream_version(&self) -> &VersionId {
        &self.source_stream_version
    }

    /// Lowercase 64-character hexadecimal SHA-256 digest of the source flight stream.
    pub fn source_stream_sha256_hex(&self) -> &str {
        &self.source_stream_sha256_hex
    }

    /// Byte length of the source flight stream.
    pub fn source_stream_byte_len(&self) -> u64 {
        self.source_stream_byte_len
    }

    /// Pinned unaltered video frame record.
    pub fn frame(&self) -> &VideoFrameRecord {
        &self.frame
    }

    /// Optional bookmark identifier associated with this handoff.
    pub fn linked_bookmark_id(&self) -> Option<&OpaqueId> {
        self.linked_bookmark_id.as_ref()
    }

    /// Camera calibration state in force at the frame's capture timestamp (`docs/architecture.md` §8).
    pub fn camera_calibration_state_at_frame(&self) -> &CameraCalibrationState {
        &self.camera_calibration_state_at_frame
    }

    /// Frame-drop spans detected at or before the exported frame's capture timestamp.
    pub fn frame_drops_at_or_before_frame(&self) -> &[FrameDropSpan] {
        &self.frame_drops_at_or_before_frame
    }

    /// Synchronized HUD and telemetry state at the frame's device timestamp.
    pub fn hud_snapshot_at_frame(&self) -> &UavHudSnapshot {
        &self.hud_snapshot_at_frame
    }

    /// Moving-camera parallax, vibration, gimbal, and range caveat.
    pub fn parallax_and_range_caveat(&self) -> &'static str {
        self.parallax_and_range_caveat
    }

    /// Renders a deterministic plain-text forensic handoff manifest bounded by `max_output_bytes`.
    pub fn render_handoff_manifest(&self, max_output_bytes: usize) -> Result<String, UavError> {
        if max_output_bytes == 0 {
            return Err(UavError::ZeroLimit {
                kind: UavLimitKind::OutputBytes,
            });
        }
        let mut out = BoundedWriter::new(max_output_bytes);
        out.write_fmt(format_args!(
            "Forensic Frame Handoff `{}`\n  session_id: {}\n  device_id: {}\n  source_stream_version: {}\n  source_stream_sha256: {}\n  source_stream_byte_len: {}\n  frame_index: {}\n  frame_version: {}\n  frame_sha256: {}\n  exposure_duration_us: {:?}\n  linked_bookmark_id: {:?}\n  calibration_at_frame: {:?}\n  prior_drop_spans: {}\n  caveat: \"{}\"\n",
            self.handoff_id.as_str(),
            self.session_id.as_str(),
            self.source_device_id.as_str(),
            self.source_stream_version.as_str(),
            self.source_stream_sha256_hex,
            self.source_stream_byte_len,
            self.frame.frame_index(),
            self.frame.frame_version().as_str(),
            self.frame.sha256_hex(),
            self.frame.exposure_duration_us(),
            self.linked_bookmark_id.as_ref().map(OpaqueId::as_str),
            self.camera_calibration_state_at_frame,
            self.frame_drops_at_or_before_frame.len(),
            self.parallax_and_range_caveat
        ))?;
        out.write_fmt(format_args!(
            "  needs_reassessment: {}\n  current_calibration_at_frame: {:?}\n  current_alignment_at_frame: {:?}\n",
            self.needs_reassessment(), self.current_camera_calibration_state_at_frame,
            self.current_time_alignment_at_frame
        ))?;
        let hud_text = self
            .hud_snapshot_at_frame
            .render_hud_text(max_output_bytes)?;
        out.write_str(&hud_text)?;
        Ok(out.finish())
    }
}

/// Input for linking higher-quality onboard recording media after a live or recorded flight
/// without overwriting earlier stream versions or bookmarks (`DEVICE-03`, `A12`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnboardRecordingLinkInput {
    /// Unique identifier for this onboard-recording link record.
    pub link_id: OpaqueId,
    /// Immutable version identifier of the higher-quality onboard media artifact.
    ///
    /// Must not collide with the original live stream version, any HUD derivative version, or any
    /// previously linked onboard version.
    pub onboard_stream_version: VersionId,
    /// Lowercase 64-character hexadecimal SHA-256 digest of the onboard media bytes.
    pub sha256_hex: String,
    /// Byte length of the onboard media artifact (`> 0`).
    pub byte_len: u64,
    /// MIME media type of the onboard media artifact.
    pub media_type: String,
    /// Documented clock offset (in milliseconds) mapping session `device_clock_ms` to the onboard
    /// recording timeline (`[-MAX_ABS_CLOCK_OFFSET_MS, MAX_ABS_CLOCK_OFFSET_MS]`).
    pub stream_to_onboard_offset_ms: i64,
    /// Existing session bookmark IDs linked to this onboard recording without mutating the
    /// bookmarks' original `pinned_stream_version`.
    pub linked_bookmark_ids: Vec<OpaqueId>,
    /// Human-inspectable provenance note explaining how the onboard file was matched.
    pub alignment_note: String,
}

/// Immutable link associating a higher-quality onboard recording with an existing flight session
/// (`DEVICE-03`, `A12`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnboardRecordingLink {
    link_id: OpaqueId,
    onboard_stream_version: VersionId,
    prior_live_stream_version: VersionId,
    sha256_hex: String,
    byte_len: u64,
    media_type: String,
    stream_to_onboard_offset_ms: i64,
    linked_bookmark_ids: Vec<OpaqueId>,
    alignment_note: String,
}

impl OnboardRecordingLink {
    /// Link record identifier.
    pub fn link_id(&self) -> &OpaqueId {
        &self.link_id
    }

    /// Immutable version of the higher-quality onboard recording.
    pub fn onboard_stream_version(&self) -> &VersionId {
        &self.onboard_stream_version
    }

    /// Immutable version of the original live/session stream preserved alongside this onboard link.
    pub fn prior_live_stream_version(&self) -> &VersionId {
        &self.prior_live_stream_version
    }

    /// Lowercase 64-character hexadecimal SHA-256 digest of the onboard media bytes.
    pub fn sha256_hex(&self) -> &str {
        &self.sha256_hex
    }

    /// Byte length of the onboard media artifact.
    pub fn byte_len(&self) -> u64 {
        self.byte_len
    }

    /// Media type of the onboard artifact.
    pub fn media_type(&self) -> &str {
        &self.media_type
    }

    /// Offset in milliseconds mapping session `device_clock_ms` to the onboard media timeline.
    pub fn stream_to_onboard_offset_ms(&self) -> i64 {
        self.stream_to_onboard_offset_ms
    }

    /// Bookmarks associated with this onboard recording.
    pub fn linked_bookmark_ids(&self) -> &[OpaqueId] {
        &self.linked_bookmark_ids
    }

    /// Alignment and provenance note.
    pub fn alignment_note(&self) -> &str {
        &self.alignment_note
    }

    /// Always returns `false`: linking onboard media preserves prior live stream versions and
    /// existing bookmark bindings unchanged (`DEVICE-03`, `A12`).
    pub fn overwrote_prior_evidence(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ClockAlignmentEntry {
    record: ClockAlignmentRecord,
}

#[derive(Debug, Clone, PartialEq)]
struct CameraCalibrationEntry {
    state: CameraCalibrationState,
    effective_from_device_ms: i64,
}

/// Bounded UAV flight session tracking capability-scoped telemetry, video frames, clock alignment,
/// drop/disconnection gaps, camera calibration validity, bookmarks, HUD derivatives, and linked
/// onboard recordings (`#33`).
#[derive(Debug, Clone)]
pub struct UavFlightSession {
    session_id: OpaqueId,
    device: RegisteredUavDevice,
    original_stream: OriginalFlightStream,
    limits: UavSessionLimits,
    remaining_text_budget: usize,
    known_versions: BTreeSet<VersionId>,
    clock_alignments: Vec<ClockAlignmentEntry>,
    calibration_history: Vec<CameraCalibrationEntry>,
    telemetry_samples: Vec<TelemetrySampleRecord>,
    video_frames: Vec<VideoFrameRecord>,
    frame_drops: Vec<FrameDropSpan>,
    disconnections: Vec<TelemetryDisconnectionSpan>,
    bookmarks: BTreeMap<OpaqueId, FlightBookmark>,
    bookmark_order: Vec<OpaqueId>,
    hud_derivatives: Vec<RenderedHudDerivative>,
    onboard_links: Vec<OnboardRecordingLink>,
}

impl UavFlightSession {
    /// Creates a new bounded [`UavFlightSession`] for a registered UAV and original flight stream.
    pub fn new(
        session_id: OpaqueId,
        device: RegisteredUavDevice,
        original_stream_input: OriginalFlightStreamInput,
        limits: UavSessionLimits,
    ) -> Result<Self, UavError> {
        let mut remaining_text_budget = limits.max_text_bytes();
        // Charge the device's model_label against the session's cumulative text budget.
        charge_text_budget(&device.model_label, &mut remaining_text_budget)?;

        if original_stream_input.origin == FlightStreamOrigin::RenderedHudOverlayDerivative {
            return Err(UavError::HudDerivativeCannotBeOriginalStream {
                stream_id: original_stream_input.stream_id,
            });
        }
        match original_stream_input.origin {
            FlightStreamOrigin::RecordedFlightImport => {
                if !device.supports_recorded_import() {
                    return Err(UavError::CapabilityNotDeclared {
                        device_id: device.device_id().clone(),
                        required: UavCapability::RecordedMediaImport,
                    });
                }
            }
            FlightStreamOrigin::LiveSessionCapture | FlightStreamOrigin::SimulatedFlightFixture => {
                if !device.supports_video() && !device.supports_telemetry() {
                    return Err(UavError::CapabilityNotDeclared {
                        device_id: device.device_id().clone(),
                        required: UavCapability::TelemetryStream,
                    });
                }
            }
            FlightStreamOrigin::RenderedHudOverlayDerivative => {}
        }
        if original_stream_input.byte_len == 0 {
            return Err(UavError::ZeroByteArtifact {
                artifact_id: original_stream_input.stream_id,
            });
        }
        validate_sha256_hex(
            "original_stream.sha256_hex",
            &original_stream_input.sha256_hex,
        )?;
        validate_single_line_text(
            "original_stream.media_type",
            &original_stream_input.media_type,
            &mut remaining_text_budget,
        )?;

        let mut known_versions = BTreeSet::new();
        known_versions.insert(device.registration_version().clone());
        if !known_versions.insert(original_stream_input.stream_version.clone()) {
            return Err(UavError::DuplicateOrOverwrittenVersion {
                version_id: original_stream_input.stream_version,
            });
        }

        let original_stream = OriginalFlightStream {
            stream_id: original_stream_input.stream_id,
            stream_version: original_stream_input.stream_version,
            origin: original_stream_input.origin,
            sha256_hex: original_stream_input.sha256_hex,
            byte_len: original_stream_input.byte_len,
            media_type: original_stream_input.media_type,
        };

        Ok(Self {
            session_id,
            device,
            original_stream,
            limits,
            remaining_text_budget,
            known_versions,
            clock_alignments: Vec::new(),
            calibration_history: Vec::new(),
            telemetry_samples: Vec::new(),
            video_frames: Vec::new(),
            frame_drops: Vec::new(),
            disconnections: Vec::new(),
            bookmarks: BTreeMap::new(),
            bookmark_order: Vec::new(),
            hud_derivatives: Vec::new(),
            onboard_links: Vec::new(),
        })
    }

    /// Session identifier.
    pub fn session_id(&self) -> &OpaqueId {
        &self.session_id
    }

    /// Registered UAV device.
    pub fn device(&self) -> &RegisteredUavDevice {
        &self.device
    }

    /// Immutable original flight stream record.
    pub fn original_stream(&self) -> &OriginalFlightStream {
        &self.original_stream
    }

    /// Configured session limits.
    pub fn limits(&self) -> UavSessionLimits {
        self.limits
    }

    fn calibration_update_count(&self) -> usize {
        self.clock_alignments
            .len()
            .saturating_add(self.calibration_history.len())
    }

    /// Latest state by effective device time, including any future-dated entry.
    /// Use [`Self::camera_calibration_state_at`] to inspect a particular instant.
    pub fn camera_calibration_state(&self) -> CameraCalibrationState {
        self.camera_calibration_state_at(i64::MAX)
    }

    /// Camera calibration state in force at `device_clock_ms` (`docs/architecture.md` §8).
    ///
    /// Preserves prior calibration versions and dated invalidation events for earlier frames even
    /// if a new calibration or invalidation is later recorded on the session.
    pub fn camera_calibration_state_at(&self, device_clock_ms: i64) -> CameraCalibrationState {
        self.calibration_history
            .iter()
            .filter(|e| e.effective_from_device_ms <= device_clock_ms)
            .max_by_key(|e| e.effective_from_device_ms)
            .map_or(CameraCalibrationState::Uncalibrated, |e| e.state.clone())
    }

    /// Full chronological history of camera calibration states recorded in the session, preserving
    /// any invalidated calibration versions alongside later replacements.
    pub fn camera_calibration_history(&self) -> Vec<CameraCalibrationState> {
        let mut entries: Vec<_> = self.calibration_history.iter().collect();
        entries.sort_by_key(|e| e.effective_from_device_ms);
        entries.into_iter().map(|e| e.state.clone()).collect()
    }

    /// Configures or updates the session's documented clock-alignment record (`DEVICE-04`).
    /// Its explicit validity window applies regardless of import order. Overlapping windows
    /// select the covering record with the latest start; equal starts are rejected as ambiguous.
    /// Already-pinned records are marked for reassessment, never silently realigned.
    pub fn set_clock_alignment(&mut self, record: ClockAlignmentRecord) -> Result<(), UavError> {
        if self.calibration_update_count() >= self.limits.max_calibration_updates() {
            return Err(UavError::LimitExceeded {
                kind: UavLimitKind::CalibrationUpdates,
                limit: self.limits.max_calibration_updates(),
            });
        }
        if record.valid_from_device_ms >= record.valid_until_device_ms {
            return Err(UavError::InvalidClockValidityWindow {
                valid_from_device_ms: record.valid_from_device_ms,
                valid_until_device_ms: record.valid_until_device_ms,
            });
        }
        if self
            .clock_alignments
            .iter()
            .any(|e| e.record.valid_from_device_ms == record.valid_from_device_ms)
        {
            return Err(UavError::DuplicateClockAlignmentStart {
                device_clock_ms: record.valid_from_device_ms,
            });
        }
        if record.device_to_utc_offset_ms.unsigned_abs() > MAX_ABS_CLOCK_OFFSET_MS as u64 {
            return Err(UavError::ClockOffsetOutOfRange {
                offset_ms: record.device_to_utc_offset_ms,
            });
        }
        if record.linear_drift_ppm.unsigned_abs() > MAX_ABS_CLOCK_DRIFT_PPM as u32 {
            return Err(UavError::ClockDriftOutOfRange {
                drift_ppm: record.linear_drift_ppm,
            });
        }
        if record.uncertainty_ms == 0 {
            return Err(UavError::ZeroClockUncertainty);
        }
        if self.known_versions.contains(&record.alignment_version) {
            return Err(UavError::DuplicateOrOverwrittenVersion {
                version_id: record.alignment_version,
            });
        }
        let mut budget_preview = self.remaining_text_budget;
        // `VersionId::new` already enforces non-empty single-line visible ASCII; charge byte budget.
        charge_text_budget(record.alignment_version.as_str(), &mut budget_preview)?;
        validate_single_line_text(
            "reference_clock_label",
            &record.reference_clock_label,
            &mut budget_preview,
        )?;
        validate_single_line_text("method_label", &record.method_label, &mut budget_preview)?;

        self.remaining_text_budget = budget_preview;
        self.known_versions.insert(record.alignment_version.clone());
        self.clock_alignments.push(ClockAlignmentEntry { record });
        self.flag_reassessment();
        Ok(())
    }

    /// Attaches a validated camera calibration to the session (`DEVICE-02`, `FIELD-03`).
    pub fn set_camera_calibration(
        &mut self,
        calibration: CameraCalibration,
    ) -> Result<(), UavError> {
        if self.calibration_update_count() >= self.limits.max_calibration_updates() {
            return Err(UavError::LimitExceeded {
                kind: UavLimitKind::CalibrationUpdates,
                limit: self.limits.max_calibration_updates(),
            });
        }
        validate_finite_range(
            "horizontal_fov_deg",
            calibration.horizontal_fov_deg,
            1.0,
            170.0,
        )?;
        validate_finite_range("vertical_fov_deg", calibration.vertical_fov_deg, 1.0, 170.0)?;
        validate_finite_range(
            "angular_uncertainty_deg",
            calibration.angular_uncertainty_deg,
            0.0,
            45.0,
        )?;
        if self
            .known_versions
            .contains(&calibration.calibration_version)
        {
            return Err(UavError::DuplicateOrOverwrittenVersion {
                version_id: calibration.calibration_version,
            });
        }
        let mut budget_preview = self.remaining_text_budget;
        // `VersionId::new` already enforces non-empty single-line visible ASCII; charge byte budget.
        charge_text_budget(
            calibration.calibration_version.as_str(),
            &mut budget_preview,
        )?;

        let effective_from_device_ms = calibration.effective_from_device_ms;
        self.validate_camera_boundary(effective_from_device_ms)?;

        self.remaining_text_budget = budget_preview;
        self.known_versions
            .insert(calibration.calibration_version.clone());
        let state = CameraCalibrationState::Valid(calibration);
        self.calibration_history.push(CameraCalibrationEntry {
            state: state.clone(),
            effective_from_device_ms,
        });
        self.flag_reassessment();
        Ok(())
    }

    /// Marks the active camera calibration as invalidated due to mount movement, shock, or lens
    /// change (`A12`, `A13`).
    ///
    /// Appends an explicitly dated invalidation, including for recorded imports. The state at
    /// that time must be valid. Equal-time events are rejected; each state covers a nonempty
    /// interval until the next event. Pinned frames keep their issued state and are flagged for
    /// reassessment if this event changes the current interpretation of their capture time.
    pub fn invalidate_camera_calibration(
        &mut self,
        effective_from_device_ms: i64,
        reason: &str,
    ) -> Result<(), UavError> {
        if self.calibration_update_count() >= self.limits.max_calibration_updates() {
            return Err(UavError::LimitExceeded {
                kind: UavLimitKind::CalibrationUpdates,
                limit: self.limits.max_calibration_updates(),
            });
        }
        self.validate_camera_boundary(effective_from_device_ms)?;
        let active_version = match self.camera_calibration_state_at(effective_from_device_ms) {
            CameraCalibrationState::Valid(cal) => cal.calibration_version,
            _ => return Err(UavError::NoActiveCalibrationToInvalidate),
        };
        let mut budget_preview = self.remaining_text_budget;
        validate_single_line_text("invalidation_reason", reason, &mut budget_preview)?;
        self.remaining_text_budget = budget_preview;

        self.calibration_history.push(CameraCalibrationEntry {
            state: CameraCalibrationState::Invalidated {
                calibration_version: active_version,
                effective_from_device_ms,
                reason: reason.to_owned(),
            },
            effective_from_device_ms,
        });
        self.flag_reassessment();
        Ok(())
    }

    fn validate_camera_boundary(&self, device_clock_ms: i64) -> Result<(), UavError> {
        if self
            .calibration_history
            .iter()
            .any(|e| e.effective_from_device_ms == device_clock_ms)
        {
            return Err(UavError::DuplicateCameraCalibrationTime { device_clock_ms });
        }
        Ok(())
    }

    fn flag_reassessment(&mut self) {
        // Compute against immutable history first, then mutate only the sticky reassessment flags.
        let telemetry_flags: Vec<_> = self
            .telemetry_samples
            .iter()
            .map(|sample| {
                self.align_device_timestamp(sample.device_clock_ms, sample.receipt_utc_ms)
                    != sample.time_alignment
            })
            .collect();
        let frame_flags: Vec<_> = self
            .video_frames
            .iter()
            .map(|frame| {
                self.align_device_timestamp(frame.device_clock_ms, frame.receipt_utc_ms)
                    != frame.time_alignment
                    || self.camera_calibration_state_at(frame.device_clock_ms)
                        != frame.camera_calibration_state_at_capture
            })
            .collect();
        let bookmark_flags: Vec<_> = self
            .bookmarks
            .values()
            .map(|bookmark| {
                let receipt_utc_ms = match bookmark.time_alignment {
                    TimeAlignment::UnsynchronizedDegraded { receipt_utc_ms, .. } => receipt_utc_ms,
                    TimeAlignment::Synchronized { .. } => 0,
                };
                self.align_device_timestamp(bookmark.device_clock_ms, receipt_utc_ms)
                    != bookmark.time_alignment
            })
            .collect();
        for (sample, changed) in self.telemetry_samples.iter_mut().zip(telemetry_flags) {
            sample.needs_reassessment |= changed;
        }
        for (frame, changed) in self.video_frames.iter_mut().zip(frame_flags) {
            frame.needs_reassessment |= changed;
        }
        for (bookmark, changed) in self.bookmarks.values_mut().zip(bookmark_flags) {
            bookmark.needs_reassessment |= changed;
        }
    }

    /// Aligns a device-clock timestamp using the [`ClockAlignmentRecord`] in force at
    /// `device_clock_ms` when within its validity window, or returns
    /// [`TimeAlignment::UnsynchronizedDegraded`] (`docs/architecture.md` §3.1).
    pub fn align_device_timestamp(
        &self,
        device_clock_ms: i64,
        receipt_utc_ms: i64,
    ) -> TimeAlignment {
        let active_entry = self
            .clock_alignments
            .iter()
            .filter(|e| {
                e.record.valid_from_device_ms <= device_clock_ms
                    && device_clock_ms < e.record.valid_until_device_ms
            })
            .max_by_key(|e| e.record.valid_from_device_ms);
        match active_entry {
            None => TimeAlignment::UnsynchronizedDegraded {
                device_clock_ms,
                receipt_utc_ms,
                reason: self
                    .clock_alignments
                    .iter()
                    .max_by_key(|e| e.record.valid_from_device_ms)
                    .map_or(UnsynchronizedReason::NoClockAlignmentConfigured, |e| {
                        UnsynchronizedReason::OutsideAlignmentValidityWindow {
                            valid_from_device_ms: e.record.valid_from_device_ms,
                            valid_until_device_ms: e.record.valid_until_device_ms,
                        }
                    }),
            },
            Some(entry) => {
                let cal = &entry.record;
                if device_clock_ms < cal.valid_from_device_ms
                    || device_clock_ms >= cal.valid_until_device_ms
                {
                    TimeAlignment::UnsynchronizedDegraded {
                        device_clock_ms,
                        receipt_utc_ms,
                        reason: UnsynchronizedReason::OutsideAlignmentValidityWindow {
                            valid_from_device_ms: cal.valid_from_device_ms,
                            valid_until_device_ms: cal.valid_until_device_ms,
                        },
                    }
                } else {
                    let elapsed_ms =
                        i128::from(device_clock_ms) - i128::from(cal.valid_from_device_ms);
                    let drift_adjustment_i128 =
                        (elapsed_ms * i128::from(cal.linear_drift_ppm)) / 1_000_000;
                    let aligned_pair =
                        i64::try_from(drift_adjustment_i128)
                            .ok()
                            .and_then(|drift_ms| {
                                device_clock_ms
                                    .checked_add(cal.device_to_utc_offset_ms)
                                    .and_then(|base_utc| base_utc.checked_add(drift_ms))
                                    .map(|utc| (utc, drift_ms))
                            });
                    let Some((aligned_utc_ms, drift_adjustment_ms)) = aligned_pair else {
                        return TimeAlignment::UnsynchronizedDegraded {
                            device_clock_ms,
                            receipt_utc_ms,
                            reason: UnsynchronizedReason::AlignmentArithmeticOverflow,
                        };
                    };
                    TimeAlignment::Synchronized {
                        aligned_utc_ms,
                        alignment_version: cal.alignment_version.clone(),
                        base_offset_ms: cal.device_to_utc_offset_ms,
                        drift_adjustment_ms,
                        total_uncertainty_ms: cal.uncertainty_ms,
                    }
                }
            }
        }
    }

    /// Ingests a validated telemetry sample into the session (`DEVICE-02`, `DEVICE-04`).
    pub fn ingest_telemetry_sample(
        &mut self,
        input: TelemetrySampleInput,
    ) -> Result<&TelemetrySampleRecord, UavError> {
        if !self.device.supports_telemetry() {
            return Err(UavError::CapabilityNotDeclared {
                device_id: self.device.device_id().clone(),
                required: UavCapability::TelemetryStream,
            });
        }
        if self.telemetry_samples.len() >= self.limits.max_telemetry_samples() {
            return Err(UavError::LimitExceeded {
                kind: UavLimitKind::TelemetrySamples,
                limit: self.limits.max_telemetry_samples(),
            });
        }
        if let Some(prev) = self.telemetry_samples.last()
            && input.device_clock_ms < prev.device_clock_ms
        {
            return Err(UavError::NonMonotonicDeviceClock {
                previous_ms: prev.device_clock_ms,
                received_ms: input.device_clock_ms,
            });
        }
        if self.known_versions.contains(&input.sample_version) {
            return Err(UavError::DuplicateOrOverwrittenVersion {
                version_id: input.sample_version,
            });
        }

        if let Some(h) = input.heading {
            validate_half_open_angle("heading.degrees", h.degrees, 0.0, 360.0)?;
        }
        if let Some(att) = input.attitude {
            validate_finite_range("attitude.roll_deg", att.roll_deg, -180.0, 180.0)?;
            validate_finite_range("attitude.pitch_deg", att.pitch_deg, -90.0, 90.0)?;
            validate_half_open_angle("attitude.yaw_deg", att.yaw_deg, 0.0, 360.0)?;
        }
        if let Some(pos) = input.position {
            validate_finite_range("position.latitude_deg", pos.latitude_deg, -90.0, 90.0)?;
            validate_finite_range("position.longitude_deg", pos.longitude_deg, -180.0, 180.0)?;
            if let Some(unc) = pos.horizontal_uncertainty_m {
                validate_finite_range("position.horizontal_uncertainty_m", unc, 0.0, 100_000.0)?;
            }
        }
        if let Some(alt) = input.altitude {
            validate_finite_range("altitude.meters", alt.meters, -1_000.0, 100_000.0)?;
            if let Some(unc) = alt.vertical_uncertainty_m {
                validate_finite_range("altitude.vertical_uncertainty_m", unc, 0.0, 100_000.0)?;
            }
        }
        if let Some(vel) = input.velocity {
            validate_finite_range(
                "velocity.ground_speed_mps",
                vel.ground_speed_mps,
                0.0,
                500.0,
            )?;
            if let Some(vz) = vel.vertical_speed_mps {
                validate_finite_range("velocity.vertical_speed_mps", vz, -200.0, 200.0)?;
            }
        }
        if let Some(gim) = input.gimbal {
            validate_finite_range("gimbal.pitch_deg", gim.pitch_deg, -120.0, 45.0)?;
            validate_finite_range("gimbal.roll_deg", gim.roll_deg, -90.0, 90.0)?;
            validate_half_open_angle("gimbal.yaw_deg", gim.yaw_deg, -180.0, 360.0)?;
        }
        if let Some(bat) = input.battery {
            validate_finite_range(
                "battery.remaining_percent",
                bat.remaining_percent,
                0.0,
                100.0,
            )?;
            if let Some(v) = bat.voltage_v
                && (!v.is_finite() || v <= 0.0 || v > 100.0)
            {
                return Err(UavError::InvalidNumericField {
                    field: "battery.voltage_v",
                });
            }
        }
        if let Some(link) = input.link_health {
            if let Some(rssi) = link.rssi_dbm {
                validate_finite_range("link_health.rssi_dbm", rssi, -150.0, 20.0)?;
            }
            if let Some(loss) = link.packet_loss_percent {
                validate_finite_range("link_health.packet_loss_percent", loss, 0.0, 100.0)?;
            }
        }
        if let Some(nav) = input.navigation_health {
            if nav.fix_kind == GnssFixKind::NoFix && input.position.is_some() {
                return Err(UavError::PositionReportedWithoutGnssFix);
            }
            if let Some(hdop) = nav.hdop
                && (!hdop.is_finite() || hdop <= 0.0 || hdop > 100.0)
            {
                return Err(UavError::InvalidNumericField {
                    field: "navigation_health.hdop",
                });
            }
        }

        let time_alignment =
            self.align_device_timestamp(input.device_clock_ms, input.receipt_utc_ms);
        self.known_versions.insert(input.sample_version.clone());
        self.telemetry_samples.push(TelemetrySampleRecord {
            needs_reassessment: false,
            sample_version: input.sample_version,
            device_clock_ms: input.device_clock_ms,
            receipt_utc_ms: input.receipt_utc_ms,
            time_alignment,
            heading: input.heading,
            attitude: input.attitude,
            position: input.position,
            altitude: input.altitude,
            velocity: input.velocity,
            gimbal: input.gimbal,
            battery: input.battery,
            link_health: input.link_health,
            navigation_health: input.navigation_health,
            recording_state: input.recording_state,
            motion_flags: input.motion_flags,
        });
        self.telemetry_samples
            .last()
            .ok_or(UavError::InternalInvariant)
    }

    /// Ingests a validated video frame metadata record (`DEVICE-03`, `DEVICE-04`).
    pub fn ingest_video_frame(
        &mut self,
        input: VideoFrameInput,
    ) -> Result<&VideoFrameRecord, UavError> {
        if !self.device.supports_video() {
            return Err(UavError::CapabilityNotDeclared {
                device_id: self.device.device_id().clone(),
                required: UavCapability::VideoStream,
            });
        }
        if self.video_frames.len() >= self.limits.max_video_frames() {
            return Err(UavError::LimitExceeded {
                kind: UavLimitKind::VideoFrames,
                limit: self.limits.max_video_frames(),
            });
        }
        if let Some(prev) = self.video_frames.last() {
            if input.frame_index <= prev.frame_index {
                return Err(UavError::NonMonotonicFrameIndex {
                    previous_index: prev.frame_index,
                    received_index: input.frame_index,
                });
            }
            if input.device_clock_ms < prev.device_clock_ms {
                return Err(UavError::NonMonotonicDeviceClock {
                    previous_ms: prev.device_clock_ms,
                    received_ms: input.device_clock_ms,
                });
            }
        }
        if let Some(exp) = input.exposure_duration_us
            && exp == 0
        {
            return Err(UavError::InvalidNumericField {
                field: "exposure_duration_us",
            });
        }
        validate_sha256_hex("frame.sha256_hex", &input.sha256_hex)?;
        if self.known_versions.contains(&input.frame_version) {
            return Err(UavError::DuplicateOrOverwrittenVersion {
                version_id: input.frame_version,
            });
        }

        let time_alignment =
            self.align_device_timestamp(input.device_clock_ms, input.receipt_utc_ms);
        let camera_calibration_state_at_capture =
            self.camera_calibration_state_at(input.device_clock_ms);
        self.known_versions.insert(input.frame_version.clone());
        self.video_frames.push(VideoFrameRecord {
            needs_reassessment: false,
            frame_index: input.frame_index,
            frame_version: input.frame_version,
            device_clock_ms: input.device_clock_ms,
            receipt_utc_ms: input.receipt_utc_ms,
            time_alignment,
            camera_calibration_state_at_capture,
            exposure_duration_us: input.exposure_duration_us,
            sha256_hex: input.sha256_hex,
        });
        self.video_frames.last().ok_or(UavError::InternalInvariant)
    }

    /// Records a span of dropped video frames (`DEVICE-04`, `A12`).
    pub fn record_frame_drop(&mut self, span: FrameDropSpan) -> Result<(), UavError> {
        let total_spans = self
            .frame_drops
            .len()
            .saturating_add(self.disconnections.len());
        if total_spans >= self.limits.max_drop_or_disconnection_spans() {
            return Err(UavError::LimitExceeded {
                kind: UavLimitKind::DropOrDisconnectionSpans,
                limit: self.limits.max_drop_or_disconnection_spans(),
            });
        }
        if span.dropped_frame_count == 0 {
            return Err(UavError::ZeroDroppedFrameCount);
        }
        let mut budget_preview = self.remaining_text_budget;
        validate_single_line_text("frame_drop.reason", &span.reason, &mut budget_preview)?;
        self.remaining_text_budget = budget_preview;
        self.frame_drops.push(span);
        Ok(())
    }

    /// Records a telemetry disconnection span (`DEVICE-04`, `A12`).
    pub fn record_telemetry_disconnection(
        &mut self,
        span: TelemetryDisconnectionSpan,
    ) -> Result<(), UavError> {
        let total_spans = self
            .frame_drops
            .len()
            .saturating_add(self.disconnections.len());
        if total_spans >= self.limits.max_drop_or_disconnection_spans() {
            return Err(UavError::LimitExceeded {
                kind: UavLimitKind::DropOrDisconnectionSpans,
                limit: self.limits.max_drop_or_disconnection_spans(),
            });
        }
        if let Some(reconnected_ms) = span.reconnected_at_device_ms
            && reconnected_ms <= span.disconnected_at_device_ms
        {
            return Err(UavError::InvalidDisconnectionSpan {
                disconnected_at_device_ms: span.disconnected_at_device_ms,
                reconnected_at_device_ms: reconnected_ms,
            });
        }
        let mut budget_preview = self.remaining_text_budget;
        validate_single_line_text("disconnection.reason", &span.reason, &mut budget_preview)?;
        self.remaining_text_budget = budget_preview;
        self.disconnections.push(span);
        Ok(())
    }

    /// Projects the capability- and freshness-aware UAV HUD state at `inspect_device_ms`
    /// (`DEVICE-02`, `DEVICE-04`, `A12`).
    pub fn project_hud_snapshot(
        &self,
        inspect_device_ms: i64,
        inspect_receipt_utc_ms: i64,
    ) -> UavHudSnapshot {
        let calibration_state = self.camera_calibration_state_at(inspect_device_ms);
        self.project_hud_snapshot_with_calibration(
            inspect_device_ms,
            inspect_receipt_utc_ms,
            &calibration_state,
        )
    }

    fn project_hud_snapshot_with_calibration(
        &self,
        inspect_device_ms: i64,
        inspect_receipt_utc_ms: i64,
        calibration_state: &CameraCalibrationState,
    ) -> UavHudSnapshot {
        let time_alignment = self.align_device_timestamp(inspect_device_ms, inspect_receipt_utc_ms);
        let cumulative_dropped_frames: u64 = self
            .frame_drops
            .iter()
            .filter(|d| d.detected_at_device_ms <= inspect_device_ms)
            .map(|d| u64::from(d.dropped_frame_count))
            .sum();
        let disconnection_span_count = self
            .disconnections
            .iter()
            .filter(|d| d.disconnected_at_device_ms <= inspect_device_ms)
            .count();

        if !self.device.supports_telemetry() {
            let unavail = UnavailableReason::TelemetryCapabilityNotDeclared;
            return UavHudSnapshot {
                needs_reassessment: false,
                inspect_device_ms,
                time_alignment,
                stream_version: self.original_stream.stream_version().clone(),
                is_simulated_stream: self.original_stream.is_simulated(),
                heading: HudFieldState::Unavailable { reason: unavail },
                attitude: HudFieldState::Unavailable { reason: unavail },
                position: HudFieldState::Unavailable { reason: unavail },
                altitude: HudFieldState::Unavailable { reason: unavail },
                velocity: HudFieldState::Unavailable { reason: unavail },
                gimbal: HudFieldState::Unavailable { reason: unavail },
                battery: HudFieldState::Unavailable { reason: unavail },
                link_health: HudFieldState::Unavailable { reason: unavail },
                navigation_health: HudFieldState::Unavailable { reason: unavail },
                recording_state: HudFieldState::Unavailable { reason: unavail },
                motion_flags: HudFieldState::Unavailable { reason: unavail },
                look_direction_marker: None,
                footprint_status: FootprintStatus::TelemetryCapabilityNotDeclared,
                cumulative_dropped_frames,
                disconnection_span_count,
                _sealed: (),
            };
        }

        let latest_sample = self
            .telemetry_samples
            .iter()
            .rev()
            .find(|s| s.device_clock_ms <= inspect_device_ms);

        let Some(sample) = latest_sample else {
            let unavail = UnavailableReason::NoSampleReceivedAtOrBeforeInstant;
            return UavHudSnapshot {
                needs_reassessment: false,
                inspect_device_ms,
                time_alignment,
                stream_version: self.original_stream.stream_version().clone(),
                is_simulated_stream: self.original_stream.is_simulated(),
                heading: HudFieldState::Unavailable { reason: unavail },
                attitude: HudFieldState::Unavailable { reason: unavail },
                position: HudFieldState::Unavailable { reason: unavail },
                altitude: HudFieldState::Unavailable { reason: unavail },
                velocity: HudFieldState::Unavailable { reason: unavail },
                gimbal: HudFieldState::Unavailable { reason: unavail },
                battery: HudFieldState::Unavailable { reason: unavail },
                link_health: HudFieldState::Unavailable { reason: unavail },
                navigation_health: HudFieldState::Unavailable { reason: unavail },
                recording_state: HudFieldState::Unavailable { reason: unavail },
                motion_flags: HudFieldState::Unavailable { reason: unavail },
                look_direction_marker: None,
                footprint_status: FootprintStatus::InsufficientFreshTelemetry {
                    missing_or_stale_field: "heading",
                },
                cumulative_dropped_frames,
                disconnection_span_count,
                _sealed: (),
            };
        };

        let sample_ms = sample.device_clock_ms;
        let age_ms = inspect_device_ms.saturating_sub(sample_ms);
        let disconnecting_span = self
            .disconnections
            .iter()
            .filter(|d| {
                d.disconnected_at_device_ms <= inspect_device_ms
                    && d.reconnected_at_device_ms
                        .is_none_or(|reconn| sample_ms < reconn)
            })
            .max_by_key(|d| d.disconnected_at_device_ms);

        let stale_reason = if let Some(disc) = disconnecting_span {
            Some(StaleReason::TelemetryLinkDisconnected {
                disconnected_at_device_ms: disc.disconnected_at_device_ms,
            })
        } else if age_ms > self.limits.stale_telemetry_after_ms() {
            Some(StaleReason::AgeExceededThreshold {
                threshold_ms: self.limits.stale_telemetry_after_ms(),
            })
        } else {
            None
        };

        let heading = qualify_field(sample.heading, sample_ms, age_ms, stale_reason);
        let attitude = qualify_field(sample.attitude, sample_ms, age_ms, stale_reason);
        let position = qualify_field(sample.position, sample_ms, age_ms, stale_reason);
        let altitude = qualify_field(sample.altitude, sample_ms, age_ms, stale_reason);
        let velocity = qualify_field(sample.velocity, sample_ms, age_ms, stale_reason);
        let gimbal = qualify_field(sample.gimbal, sample_ms, age_ms, stale_reason);
        let battery = qualify_field(sample.battery, sample_ms, age_ms, stale_reason);
        let link_health = qualify_field(sample.link_health, sample_ms, age_ms, stale_reason);
        let navigation_health =
            qualify_field(sample.navigation_health, sample_ms, age_ms, stale_reason);
        let recording_state = match sample.recording_state {
            RecordingState::Unavailable => HudFieldState::Unavailable {
                reason: UnavailableReason::MissingInSample {
                    sample_device_ms: sample_ms,
                },
            },
            other => qualify_field(Some(other), sample_ms, age_ms, stale_reason),
        };
        let motion_flags =
            qualify_field(Some(sample.motion_flags), sample_ms, age_ms, stale_reason);

        let look_direction_marker = derive_look_marker(&heading, &attitude, &gimbal);
        let footprint_status = Self::evaluate_footprint(
            calibration_state,
            &heading,
            &attitude,
            &position,
            &altitude,
            &gimbal,
            &navigation_health,
        );

        UavHudSnapshot {
            needs_reassessment: false,
            inspect_device_ms,
            time_alignment,
            stream_version: self.original_stream.stream_version().clone(),
            is_simulated_stream: self.original_stream.is_simulated(),
            heading,
            attitude,
            position,
            altitude,
            velocity,
            gimbal,
            battery,
            link_health,
            navigation_health,
            recording_state,
            motion_flags,
            look_direction_marker,
            footprint_status,
            cumulative_dropped_frames,
            disconnection_span_count,
            _sealed: (),
        }
    }

    fn evaluate_footprint(
        calibration_state: &CameraCalibrationState,
        heading: &HudFieldState<HeadingSample>,
        attitude: &HudFieldState<AttitudeDeg>,
        position: &HudFieldState<Wgs84Position>,
        altitude: &HudFieldState<AltitudeSample>,
        gimbal: &HudFieldState<GimbalOrientation>,
        navigation_health: &HudFieldState<NavigationHealth>,
    ) -> FootprintStatus {
        let cal = match calibration_state {
            CameraCalibrationState::Uncalibrated => {
                return FootprintStatus::UncalibratedViewingRayOnly;
            }
            CameraCalibrationState::Invalidated {
                calibration_version,
                effective_from_device_ms,
                reason,
            } => {
                return FootprintStatus::CalibrationInvalidated {
                    calibration_version: calibration_version.clone(),
                    effective_from_device_ms: *effective_from_device_ms,
                    reason: reason.clone(),
                };
            }
            CameraCalibrationState::Valid(cal) => cal.clone(),
        };

        let Some(gim) = gimbal.fresh_value() else {
            return FootprintStatus::InsufficientFreshTelemetry {
                missing_or_stale_field: "gimbal",
            };
        };
        let world_locked = gim.mode == GimbalMode::IndependentWorldLocked;
        let hdg = if world_locked {
            None
        } else {
            let Some(hdg) = heading.fresh_value() else {
                return FootprintStatus::InsufficientFreshTelemetry {
                    missing_or_stale_field: "heading",
                };
            };
            Some(hdg)
        };
        let att = if world_locked {
            None
        } else {
            let Some(att) = attitude.fresh_value() else {
                return FootprintStatus::InsufficientFreshTelemetry {
                    missing_or_stale_field: "attitude",
                };
            };
            Some(att)
        };
        let Some(pos) = position.fresh_value() else {
            return FootprintStatus::InsufficientFreshTelemetry {
                missing_or_stale_field: "position",
            };
        };
        let Some(pos_unc_m) = pos.horizontal_uncertainty_m else {
            return FootprintStatus::InsufficientFreshTelemetry {
                missing_or_stale_field: "position.horizontal_uncertainty_m",
            };
        };
        let Some(alt) = altitude.fresh_value() else {
            return FootprintStatus::InsufficientFreshTelemetry {
                missing_or_stale_field: "altitude",
            };
        };
        let Some(alt_unc_m) = alt.vertical_uncertainty_m else {
            return FootprintStatus::InsufficientFreshTelemetry {
                missing_or_stale_field: "altitude.vertical_uncertainty_m",
            };
        };

        if alt.reference != AltitudeReference::AboveGroundLevel {
            return FootprintStatus::IncompatibleAltitudeReference {
                actual: alt.reference,
            };
        }
        if alt.meters <= 0.0 {
            return FootprintStatus::NonPositiveHeightAboveGround {
                height_m: alt.meters,
            };
        }

        let orientation = match gim.mode {
            GimbalMode::FollowHeading => {
                let Some(hdg) = hdg else {
                    return FootprintStatus::InsufficientFreshTelemetry {
                        missing_or_stale_field: "heading",
                    };
                };
                let Some(att) = att else {
                    return FootprintStatus::InsufficientFreshTelemetry {
                        missing_or_stale_field: "attitude",
                    };
                };
                if hdg.reference != HeadingReference::TrueNorth {
                    return FootprintStatus::IncompatibleHeadingReference {
                        actual: hdg.reference,
                    };
                }
                compose_camera_orientation(hdg.degrees, att.pitch_deg, att.roll_deg, gim)
            }
            GimbalMode::IndependentWorldLocked => compose_camera_orientation(0.0, 0.0, 0.0, gim),
        };
        if orientation.image_roll_deg.abs() > 1e-6 {
            return FootprintStatus::NonZeroRollUnsupported {
                effective_roll_deg: orientation.image_roll_deg,
            };
        }
        let world_azimuth_deg = orientation.azimuth_deg;
        let depression_deg = orientation.depression_deg;

        let half_vfov_deg = cal.vertical_fov_deg * 0.5;
        let far_dep_deg = depression_deg - half_vfov_deg;
        let near_dep_deg = depression_deg + half_vfov_deg;
        let rear_horizon_clearance_deg = 180.0 - near_dep_deg;
        let min_clearance_deg = depression_deg
            .min(far_dep_deg)
            .min(rear_horizon_clearance_deg);
        if min_clearance_deg < MIN_FOOTPRINT_DEPRESSION_DEG {
            return FootprintStatus::UnresolvableGeometry {
                depression_deg: min_clearance_deg,
                min_required_depression_deg: MIN_FOOTPRINT_DEPRESSION_DEG,
            };
        }

        let Some(nav) = navigation_health.fresh_value() else {
            return FootprintStatus::InsufficientFreshTelemetry {
                missing_or_stale_field: "navigation_health",
            };
        };
        if matches!(
            nav.fix_kind,
            GnssFixKind::NoFix | GnssFixKind::DeadReckoningEstimate
        ) {
            return FootprintStatus::InsufficientFreshTelemetry {
                missing_or_stale_field: "navigation_health.fix_kind",
            };
        }

        let dep_rad = depression_deg.to_radians();
        let sin_dep = dep_rad.sin();
        let cos_dep = dep_rad.cos();
        let az_rad = world_azimuth_deg.to_radians();
        let center_slant_range_m = alt.meters / sin_dep;
        let center_ground_distance_m = alt.meters * (cos_dep / sin_dep);
        let center_north_offset_m = center_ground_distance_m * az_rad.cos();
        let center_east_offset_m = center_ground_distance_m * az_rad.sin();

        let half_hfov_rad = (cal.horizontal_fov_deg * 0.5).to_radians();
        let cross_look_span_m = 2.0 * center_slant_range_m * half_hfov_rad.tan();

        // Signed horizontal distance along the look azimuth: x(theta) = h * cot(theta) = h * cos(theta) / sin(theta).
        // When near_dep_deg > 90 deg (frustum straddles nadir), cos(near_rad) < 0 so near_ground_x_m < 0
        // behind the aircraft and (far_ground_x_m - near_ground_x_m) sums both sides of nadir cleanly.
        let far_rad = far_dep_deg.to_radians();
        let near_rad = near_dep_deg.to_radians();
        let far_ground_x_m = alt.meters * (far_rad.cos() / far_rad.sin());
        let near_ground_x_m = alt.meters * (near_rad.cos() / near_rad.sin());
        let along_look_span_m = (far_ground_x_m - near_ground_x_m).abs();

        // Linearized ground-center propagation along the look direction:
        // d(h * cot(theta)) / d(theta) = -h / sin^2(theta) = -center_slant_range_m / sin(theta).
        let alt_ground_unc_m = alt_unc_m * (cos_dep.abs() / sin_dep);
        let ang_ground_unc_m =
            (center_slant_range_m / sin_dep) * cal.angular_uncertainty_deg.to_radians();
        let horizontal_uncertainty_m = pos_unc_m + alt_ground_unc_m + ang_ground_unc_m;

        FootprintStatus::CalibratedDerivedFootprint(CalibratedFootprint {
            calibration_version: cal.calibration_version,
            azimuth_reference: HeadingReference::TrueNorth,
            center_slant_range_m,
            center_ground_distance_m,
            center_north_offset_m,
            center_east_offset_m,
            along_look_span_m,
            cross_look_span_m,
            horizontal_uncertainty_m,
            terrain_assumption: "Local flat-ground approximation using AboveGroundLevel height; does not account for sloped terrain or obstacle occlusion.",
            is_derived_estimate: true,
            _sealed: (),
        })
    }

    /// Attaches an immutable event bookmark to a case, pinning the current original stream version
    /// (`DEVICE-03`, `A12`).
    pub fn record_bookmark(
        &mut self,
        input: FlightBookmarkInput,
    ) -> Result<&FlightBookmark, UavError> {
        if self.bookmarks.len() >= self.limits.max_bookmarks() {
            return Err(UavError::LimitExceeded {
                kind: UavLimitKind::Bookmarks,
                limit: self.limits.max_bookmarks(),
            });
        }
        if self.bookmarks.contains_key(&input.bookmark_id) {
            return Err(UavError::DuplicateBookmarkId {
                bookmark_id: input.bookmark_id,
            });
        }
        let pinned_frame_version = match input.pinned_frame_index {
            Some(idx) => {
                let frame = self
                    .video_frames
                    .iter()
                    .find(|f| f.frame_index == idx)
                    .ok_or(UavError::UnknownFrameIndex { frame_index: idx })?;
                Some(frame.frame_version.clone())
            }
            None => None,
        };

        let mut budget_preview = self.remaining_text_budget;
        validate_single_line_text("bookmark.label", &input.label, &mut budget_preview)?;
        if let Some(note) = &input.note {
            validate_single_line_text("bookmark.note", note, &mut budget_preview)?;
        }
        self.remaining_text_budget = budget_preview;

        let time_alignment =
            self.align_device_timestamp(input.device_clock_ms, input.receipt_utc_ms);
        let bookmark_id = input.bookmark_id.clone();
        let bookmark = FlightBookmark {
            needs_reassessment: false,
            bookmark_id: input.bookmark_id,
            case_id: input.case_id,
            created_by: input.created_by,
            device_clock_ms: input.device_clock_ms,
            time_alignment,
            pinned_stream_version: self.original_stream.stream_version().clone(),
            pinned_frame_index: input.pinned_frame_index,
            pinned_frame_version,
            label: input.label,
            note: input.note,
        };
        self.bookmark_order.push(bookmark_id.clone());
        self.bookmarks.insert(bookmark_id.clone(), bookmark);
        self.bookmarks
            .get(&bookmark_id)
            .ok_or(UavError::InternalInvariant)
    }

    /// Exports an immutable, self-contained [`ForensicFrameHandoff`] copy for a video frame
    /// without blocking or mutating ongoing telemetry/frame reception (`DEVICE-03`, `DEVICE-04`, `A12`).
    pub fn export_forensic_frame_handoff(
        &self,
        handoff_id: OpaqueId,
        frame_index: u64,
        linked_bookmark_id: Option<OpaqueId>,
    ) -> Result<ForensicFrameHandoff, UavError> {
        let frame = self
            .video_frames
            .iter()
            .find(|f| f.frame_index == frame_index)
            .ok_or(UavError::UnknownFrameIndex { frame_index })?
            .clone();

        if let Some(bm_id) = &linked_bookmark_id {
            let bm = self
                .bookmarks
                .get(bm_id)
                .ok_or_else(|| UavError::UnknownBookmarkId {
                    bookmark_id: bm_id.clone(),
                })?;
            if let Some(pinned_idx) = bm.pinned_frame_index
                && pinned_idx != frame_index
            {
                return Err(UavError::BookmarkFrameMismatch {
                    bookmark_id: bm_id.clone(),
                    bookmark_frame_index: pinned_idx,
                    requested_frame_index: frame_index,
                });
            }
        }

        let camera_calibration_state_at_frame = frame.camera_calibration_state_at_capture.clone();
        let mut hud_snapshot_at_frame = self.project_hud_snapshot_with_calibration(
            frame.device_clock_ms,
            frame.receipt_utc_ms,
            &camera_calibration_state_at_frame,
        );
        hud_snapshot_at_frame.time_alignment = frame.time_alignment.clone();
        hud_snapshot_at_frame.needs_reassessment = frame.needs_reassessment;
        let current_camera_calibration_state_at_frame =
            self.camera_calibration_state_at(frame.device_clock_ms);
        let current_time_alignment_at_frame =
            self.align_device_timestamp(frame.device_clock_ms, frame.receipt_utc_ms);

        let frame_drops_at_or_before_frame = self
            .frame_drops
            .iter()
            .filter(|d| d.detected_at_device_ms <= frame.device_clock_ms)
            .cloned()
            .collect();

        Ok(ForensicFrameHandoff {
            handoff_id,
            session_id: self.session_id.clone(),
            source_device_id: self.device.device_id().clone(),
            source_stream_version: self.original_stream.stream_version().clone(),
            source_stream_sha256_hex: self.original_stream.sha256_hex().to_owned(),
            source_stream_byte_len: self.original_stream.byte_len(),
            frame,
            linked_bookmark_id,
            camera_calibration_state_at_frame,
            current_camera_calibration_state_at_frame,
            current_time_alignment_at_frame,
            frame_drops_at_or_before_frame,
            hud_snapshot_at_frame,
            parallax_and_range_caveat: MOVING_CAMERA_FORENSIC_CAVEAT,
        })
    }

    /// Registers a separate rendered HUD derivative artifact without replacing the original stream
    /// (`DEVICE-03`).
    pub fn register_hud_derivative(
        &mut self,
        input: RenderedHudDerivativeInput,
    ) -> Result<&RenderedHudDerivative, UavError> {
        if self.hud_derivatives.len() >= self.limits.max_hud_derivatives() {
            return Err(UavError::LimitExceeded {
                kind: UavLimitKind::HudDerivatives,
                limit: self.limits.max_hud_derivatives(),
            });
        }
        let matches_original =
            input.derived_from_stream_version == *self.original_stream.stream_version();
        let matches_onboard = self
            .onboard_links
            .iter()
            .any(|l| l.onboard_stream_version == input.derived_from_stream_version);
        if !matches_original && !matches_onboard {
            return Err(UavError::UnknownSourceStreamVersion {
                version_id: input.derived_from_stream_version,
            });
        }
        if self.known_versions.contains(&input.derivative_version) {
            return Err(UavError::DuplicateOrOverwrittenVersion {
                version_id: input.derivative_version,
            });
        }
        if input.byte_len == 0 {
            return Err(UavError::ZeroByteArtifact {
                artifact_id: input.derivative_id,
            });
        }
        validate_sha256_hex("derivative.sha256_hex", &input.sha256_hex)?;

        let mut budget_preview = self.remaining_text_budget;
        // `VersionId::new` already enforces non-empty single-line visible ASCII; charge byte budget.
        charge_text_budget(input.derivative_version.as_str(), &mut budget_preview)?;
        validate_single_line_text(
            "derivative.media_type",
            &input.media_type,
            &mut budget_preview,
        )?;
        validate_single_line_text(
            "derivative.overlay_configuration_label",
            &input.overlay_configuration_label,
            &mut budget_preview,
        )?;
        self.remaining_text_budget = budget_preview;

        self.known_versions.insert(input.derivative_version.clone());
        self.hud_derivatives.push(RenderedHudDerivative {
            derivative_id: input.derivative_id,
            derivative_version: input.derivative_version,
            derived_from_stream_version: input.derived_from_stream_version,
            sha256_hex: input.sha256_hex,
            byte_len: input.byte_len,
            media_type: input.media_type,
            overlay_configuration_label: input.overlay_configuration_label,
        });
        self.hud_derivatives
            .last()
            .ok_or(UavError::InternalInvariant)
    }

    /// Links a higher-quality onboard recording to the session and selected bookmarks without
    /// overwriting the original live/recorded stream version or mutating prior bookmark bindings
    /// (`DEVICE-03`, `A12`).
    pub fn link_onboard_recording(
        &mut self,
        input: OnboardRecordingLinkInput,
    ) -> Result<&OnboardRecordingLink, UavError> {
        if self.onboard_links.len() >= self.limits.max_onboard_links() {
            return Err(UavError::LimitExceeded {
                kind: UavLimitKind::OnboardLinks,
                limit: self.limits.max_onboard_links(),
            });
        }
        if self
            .onboard_links
            .iter()
            .any(|l| l.link_id == input.link_id)
        {
            return Err(UavError::DuplicateOnboardLinkId {
                link_id: input.link_id,
            });
        }
        if self.known_versions.contains(&input.onboard_stream_version) {
            return Err(UavError::DuplicateOrOverwrittenVersion {
                version_id: input.onboard_stream_version,
            });
        }
        if input.byte_len == 0 {
            return Err(UavError::ZeroByteArtifact {
                artifact_id: input.link_id,
            });
        }
        if input.stream_to_onboard_offset_ms.unsigned_abs() > MAX_ABS_CLOCK_OFFSET_MS as u64 {
            return Err(UavError::ClockOffsetOutOfRange {
                offset_ms: input.stream_to_onboard_offset_ms,
            });
        }
        validate_sha256_hex("onboard.sha256_hex", &input.sha256_hex)?;

        let mut seen_bookmarks = BTreeSet::new();
        for bm_id in &input.linked_bookmark_ids {
            if !self.bookmarks.contains_key(bm_id) {
                return Err(UavError::UnknownBookmarkId {
                    bookmark_id: bm_id.clone(),
                });
            }
            if !seen_bookmarks.insert(bm_id.clone()) {
                return Err(UavError::DuplicateBookmarkId {
                    bookmark_id: bm_id.clone(),
                });
            }
        }

        let mut budget_preview = self.remaining_text_budget;
        // `VersionId::new` already enforces non-empty single-line visible ASCII; charge byte budget.
        charge_text_budget(input.onboard_stream_version.as_str(), &mut budget_preview)?;
        validate_single_line_text("onboard.media_type", &input.media_type, &mut budget_preview)?;
        validate_single_line_text(
            "onboard.alignment_note",
            &input.alignment_note,
            &mut budget_preview,
        )?;
        self.remaining_text_budget = budget_preview;

        self.known_versions
            .insert(input.onboard_stream_version.clone());
        self.onboard_links.push(OnboardRecordingLink {
            link_id: input.link_id,
            onboard_stream_version: input.onboard_stream_version,
            prior_live_stream_version: self.original_stream.stream_version().clone(),
            sha256_hex: input.sha256_hex,
            byte_len: input.byte_len,
            media_type: input.media_type,
            stream_to_onboard_offset_ms: input.stream_to_onboard_offset_ms,
            linked_bookmark_ids: input.linked_bookmark_ids,
            alignment_note: input.alignment_note,
        });
        self.onboard_links.last().ok_or(UavError::InternalInvariant)
    }

    /// Ingested telemetry samples in chronological order.
    pub fn telemetry_samples(&self) -> &[TelemetrySampleRecord] {
        &self.telemetry_samples
    }

    /// Ingested video frame metadata records in sequential order.
    pub fn video_frames(&self) -> &[VideoFrameRecord] {
        &self.video_frames
    }

    /// Recorded dropped-frame spans.
    pub fn frame_drops(&self) -> &[FrameDropSpan] {
        &self.frame_drops
    }

    /// Recorded telemetry disconnection spans.
    pub fn disconnections(&self) -> &[TelemetryDisconnectionSpan] {
        &self.disconnections
    }

    /// Looks up a bookmark by ID.
    pub fn bookmark(&self, bookmark_id: &OpaqueId) -> Option<&FlightBookmark> {
        self.bookmarks.get(bookmark_id)
    }

    /// Returns all session bookmarks in creation order.
    pub fn bookmarks(&self) -> Vec<&FlightBookmark> {
        self.bookmark_order
            .iter()
            .filter_map(|id| self.bookmarks.get(id))
            .collect()
    }

    /// Registered rendered HUD derivatives.
    pub fn hud_derivatives(&self) -> &[RenderedHudDerivative] {
        &self.hud_derivatives
    }

    /// Linked higher-quality onboard recordings.
    pub fn onboard_links(&self) -> &[OnboardRecordingLink] {
        &self.onboard_links
    }
}

fn qualify_field<T: Copy>(
    opt_value: Option<T>,
    sample_device_ms: i64,
    age_ms: i64,
    stale_reason: Option<StaleReason>,
) -> HudFieldState<T> {
    match opt_value {
        None => HudFieldState::Unavailable {
            reason: UnavailableReason::MissingInSample { sample_device_ms },
        },
        Some(value) => match stale_reason {
            None => HudFieldState::Fresh {
                value,
                sample_device_ms,
                age_ms,
            },
            Some(reason) => HudFieldState::Stale {
                value,
                sample_device_ms,
                age_ms,
                reason,
            },
        },
    }
}

/// Camera axes in local north/east/down coordinates. Both the optical axis and image right/down
/// axes matter: yawing a gimbal on a pitched airframe induces image roll even with zero input roll.
#[derive(Debug, Clone, Copy)]
struct CameraOrientation {
    azimuth_deg: f64,
    depression_deg: f64,
    image_roll_deg: f64,
    vertical_axis: bool,
}

fn rotate_zyx(v: (f64, f64, f64), yaw: f64, pitch: f64, roll: f64) -> (f64, f64, f64) {
    let (sr, cr) = roll.to_radians().sin_cos();
    let (sp, cp) = pitch.to_radians().sin_cos();
    let (sy, cy) = yaw.to_radians().sin_cos();
    let (x, y, z) = (v.0, cr * v.1 - sr * v.2, sr * v.1 + cr * v.2);
    let (x, z) = (cp * x + sp * z, -sp * x + cp * z);
    (cy * x - sy * y, sy * x + cy * y, z)
}

/// Intrinsic ZYX composition: R_world_body * R_body_camera. World-locked cameras use an identity
/// body rotation. At a vertical optical axis, azimuth is undefined; the image right axis supplies
/// a declared footprint-axis convention. A rectangular frustum is invariant under 180-degree roll.
fn compose_camera_orientation(
    body_yaw_deg: f64,
    body_pitch_deg: f64,
    body_roll_deg: f64,
    gimbal: &GimbalOrientation,
) -> CameraOrientation {
    let rotate = |axis| {
        rotate_zyx(
            rotate_zyx(axis, gimbal.yaw_deg, gimbal.pitch_deg, gimbal.roll_deg),
            body_yaw_deg,
            body_pitch_deg,
            body_roll_deg,
        )
    };
    let forward = rotate((1.0, 0.0, 0.0));
    let right = rotate((0.0, 1.0, 0.0));
    let down = rotate((0.0, 0.0, 1.0));
    let horizontal = forward.0.hypot(forward.1);
    let vertical_axis = horizontal <= 1e-12;
    let azimuth_deg = if vertical_axis {
        (-right.0).atan2(right.1).to_degrees().rem_euclid(360.0)
    } else {
        forward.1.atan2(forward.0).to_degrees().rem_euclid(360.0)
    };
    let image_roll_deg = if vertical_axis {
        0.0
    } else {
        // Vertical components equal cos(depression) * sin(roll), cos(depression) * cos(roll).
        // Reduce modulo 180 because turning the rectangular image upside down preserves its span.
        (right.2.atan2(down.2).to_degrees() + 90.0).rem_euclid(180.0) - 90.0
    };
    CameraOrientation {
        azimuth_deg,
        depression_deg: forward.2.atan2(horizontal).to_degrees(),
        image_roll_deg,
        vertical_axis,
    }
}

fn derive_look_marker(
    heading: &HudFieldState<HeadingSample>,
    attitude: &HudFieldState<AttitudeDeg>,
    gimbal: &HudFieldState<GimbalOrientation>,
) -> Option<LookDirectionMarker> {
    let (gim, gim_stale) = match gimbal {
        HudFieldState::Fresh { value, .. } => (value, false),
        HudFieldState::Stale { value, .. } => (value, true),
        HudFieldState::Unavailable { .. } => return None,
    };

    match gim.mode {
        GimbalMode::IndependentWorldLocked => {
            let orientation = compose_camera_orientation(0.0, 0.0, 0.0, gim);
            Some(LookDirectionMarker {
                world_azimuth_deg: orientation.azimuth_deg,
                depression_deg: orientation.depression_deg,
                azimuth_undefined_at_nadir: orientation.vertical_axis,
                is_derived_estimate: true,
                azimuth_reference: HeadingReference::TrueNorth,
                derived_from_stale_input: gim_stale,
                establishes_slant_range: false,
                _sealed: (),
            })
        }
        GimbalMode::FollowHeading => {
            let (hdg, hdg_stale) = match heading {
                HudFieldState::Fresh { value, .. } => (value, false),
                HudFieldState::Stale { value, .. } => (value, true),
                HudFieldState::Unavailable { .. } => return None,
            };
            let (att, att_stale) = match attitude {
                HudFieldState::Fresh { value, .. } => (value, false),
                HudFieldState::Stale { value, .. } => (value, true),
                HudFieldState::Unavailable { .. } => return None,
            };
            let orientation =
                compose_camera_orientation(hdg.degrees, att.pitch_deg, att.roll_deg, gim);
            Some(LookDirectionMarker {
                world_azimuth_deg: orientation.azimuth_deg,
                depression_deg: orientation.depression_deg,
                azimuth_undefined_at_nadir: orientation.vertical_axis,
                is_derived_estimate: true,
                azimuth_reference: hdg.reference,
                derived_from_stale_input: hdg_stale || gim_stale || att_stale,
                establishes_slant_range: false,
                _sealed: (),
            })
        }
    }
}

/// Typed errors returned by the UAV registration, telemetry, HUD, bookmark, and handoff kernel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UavError {
    /// Clock records with equal validity starts have ambiguous precedence.
    DuplicateClockAlignmentStart {
        /// Conflicting start on the device clock.
        device_clock_ms: i64,
    },
    /// Camera state changes must have distinct effective times.
    DuplicateCameraCalibrationTime {
        /// Conflicting event time on the device clock.
        device_clock_ms: i64,
    },
    /// Underlying identifier validation error.
    InvalidId(IdError),
    /// A configured session limit was zero.
    ZeroLimit {
        /// Which limit was zero.
        kind: UavLimitKind,
    },
    /// Configured `stale_telemetry_after_ms` was `<= 0`.
    InvalidStaleThreshold {
        /// Supplied threshold in milliseconds.
        stale_telemetry_after_ms: i64,
    },
    /// A session or field budget was exceeded.
    LimitExceeded {
        /// Which budget was exceeded.
        kind: UavLimitKind,
        /// Configured maximum value.
        limit: usize,
    },
    /// A required single-line text field was empty or whitespace-only.
    EmptyTextField {
        /// Field name.
        field: &'static str,
    },
    /// A text field contained a forbidden control, line-break, bidi, or invisible character.
    ForbiddenTextCharacter {
        /// Field name.
        field: &'static str,
        /// Unicode scalar value of the rejected character.
        codepoint: u32,
    },
    /// A SHA-256 hex digest string was not 64 lowercase hexadecimal ASCII characters.
    InvalidSha256Hex {
        /// Field name.
        field: &'static str,
    },
    /// Automatic publication of local footage or location was requested on device registration
    /// (`DEVICE-01`).
    AutoPublicationForbidden {
        /// Device identifier.
        device_id: OpaqueId,
    },
    /// Device registration declared no capabilities.
    EmptyCapabilities {
        /// Device identifier.
        device_id: OpaqueId,
    },
    /// Device registration listed the same capability more than once.
    DuplicateCapability {
        /// Device identifier.
        device_id: OpaqueId,
        /// Duplicated capability.
        capability: UavCapability,
    },
    /// Registration requested [`UavCapability::FlightControl`], which is unsupported in the
    /// initial receive-only adapter (`DEVICE-04`, `A12`).
    FlightControlUnsupported {
        /// Device identifier.
        device_id: OpaqueId,
    },
    /// Caller attempted to emit a flight-control command (`DEVICE-04`, `A12`).
    FlightControlCommandForbidden {
        /// Device identifier.
        device_id: OpaqueId,
        /// Attempted command name.
        command: String,
    },
    /// Operation required a capability not declared on the registered UAV device (`DEVICE-01`).
    CapabilityNotDeclared {
        /// Device identifier.
        device_id: OpaqueId,
        /// Missing required capability.
        required: UavCapability,
    },
    /// Caller attempted to register a rendered HUD overlay derivative as the session's original
    /// flight stream (`DEVICE-03`).
    HudDerivativeCannotBeOriginalStream {
        /// Stream identifier.
        stream_id: OpaqueId,
    },
    /// An artifact declared a zero byte length.
    ZeroByteArtifact {
        /// Artifact identifier.
        artifact_id: OpaqueId,
    },
    /// A version identifier was reused or would overwrite an existing version (`DEVICE-03`, `A12`).
    DuplicateOrOverwrittenVersion {
        /// Reused version identifier.
        version_id: VersionId,
    },
    /// Clock alignment validity window had `valid_from_device_ms >= valid_until_device_ms`.
    InvalidClockValidityWindow {
        /// Start timestamp (`ms`).
        valid_from_device_ms: i64,
        /// End timestamp (`ms`).
        valid_until_device_ms: i64,
    },
    /// Clock alignment specified `uncertainty_ms == 0`, which is forbidden for physical clock
    /// synchronization (`A12`).
    ZeroClockUncertainty,
    /// Clock offset magnitude exceeded [`MAX_ABS_CLOCK_OFFSET_MS`].
    ClockOffsetOutOfRange {
        /// Supplied offset (`ms`).
        offset_ms: i64,
    },
    /// Linear clock drift magnitude exceeded [`MAX_ABS_CLOCK_DRIFT_PPM`].
    ClockDriftOutOfRange {
        /// Supplied drift (`ppm`).
        drift_ppm: i32,
    },
    /// Telemetry or video frame device-clock timestamp was non-monotonic.
    NonMonotonicDeviceClock {
        /// Previous timestamp (`ms`).
        previous_ms: i64,
        /// Rejected timestamp (`ms`).
        received_ms: i64,
    },
    /// Video frame index was not strictly increasing.
    NonMonotonicFrameIndex {
        /// Previous frame index.
        previous_index: u64,
        /// Rejected frame index.
        received_index: u64,
    },
    /// A numeric field was non-finite (`NaN`/`±∞`) or outside its physical domain.
    InvalidNumericField {
        /// Field name.
        field: &'static str,
    },
    /// Telemetry sample reported a horizontal position while `GnssFixKind::NoFix` was active.
    PositionReportedWithoutGnssFix,
    /// Caller requested an altitude in a different [`AltitudeReference`] datum (`docs/architecture.md` §3.2).
    IncompatibleAltitudeReference {
        /// Actual datum of the altitude sample.
        actual: AltitudeReference,
        /// Requested datum.
        requested: AltitudeReference,
    },
    /// Caller attempted to invalidate camera calibration when none was active.
    NoActiveCalibrationToInvalidate,
    /// Frame-drop record specified zero dropped frames.
    ZeroDroppedFrameCount,
    /// Telemetry disconnection span had `reconnected_at_device_ms <= disconnected_at_device_ms`.
    InvalidDisconnectionSpan {
        /// Disconnection timestamp (`ms`).
        disconnected_at_device_ms: i64,
        /// Reconnection timestamp (`ms`).
        reconnected_at_device_ms: i64,
    },
    /// Duplicate bookmark identifier in the same session or link list.
    DuplicateBookmarkId {
        /// Bookmark identifier.
        bookmark_id: OpaqueId,
    },
    /// Referenced bookmark identifier does not exist in the session.
    UnknownBookmarkId {
        /// Bookmark identifier.
        bookmark_id: OpaqueId,
    },
    /// Referenced video frame index does not exist in the session.
    UnknownFrameIndex {
        /// Frame index.
        frame_index: u64,
    },
    /// Bookmark was pinned to a different frame index than requested in the forensic handoff.
    BookmarkFrameMismatch {
        /// Bookmark identifier.
        bookmark_id: OpaqueId,
        /// Frame index pinned on the bookmark.
        bookmark_frame_index: u64,
        /// Frame index requested for the handoff.
        requested_frame_index: u64,
    },
    /// Rendered HUD derivative referenced a source stream version not belonging to this session.
    UnknownSourceStreamVersion {
        /// Unrecognized stream version.
        version_id: VersionId,
    },
    /// Duplicate onboard-recording link identifier.
    DuplicateOnboardLinkId {
        /// Link identifier.
        link_id: OpaqueId,
    },
    /// Internal collection invariant sentinel.
    InternalInvariant,
}

impl From<IdError> for UavError {
    fn from(err: IdError) -> Self {
        Self::InvalidId(err)
    }
}

impl fmt::Display for UavError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for UavError {}

fn validate_single_line_text(
    field: &'static str,
    value: &str,
    remaining_text_budget: &mut usize,
) -> Result<(), UavError> {
    if value.trim().is_empty() {
        return Err(UavError::EmptyTextField { field });
    }
    let byte_len = value.len();
    if byte_len > MAX_TEXT_FIELD_BYTES {
        return Err(UavError::LimitExceeded {
            kind: UavLimitKind::SingleTextFieldBytes,
            limit: MAX_TEXT_FIELD_BYTES,
        });
    }
    for ch in value.chars() {
        if is_forbidden_single_line_char(ch) {
            return Err(UavError::ForbiddenTextCharacter {
                field,
                codepoint: u32::from(ch),
            });
        }
    }
    charge_text_budget(value, remaining_text_budget)
}

fn charge_text_budget(value: &str, remaining_text_budget: &mut usize) -> Result<(), UavError> {
    let byte_len = value.len();
    if byte_len > *remaining_text_budget {
        return Err(UavError::LimitExceeded {
            kind: UavLimitKind::TextBytes,
            limit: *remaining_text_budget,
        });
    }
    *remaining_text_budget -= byte_len;
    Ok(())
}

/// Rejects ASCII/Unicode control characters, line/paragraph separators, bidi overrides/isolates,
/// and invisible formatting/tag characters while allowing `U+200C` (ZWNJ) and `U+200D` (ZWJ) for
/// Persian/Indic/emoji text.
fn is_forbidden_single_line_char(ch: char) -> bool {
    if ch == '\u{200C}' || ch == '\u{200D}' {
        return false;
    }
    ch.is_control()
        || matches!(
            ch,
            '\u{0085}'
                | '\u{180E}'
                | '\u{200B}'
                | '\u{200E}'
                | '\u{200F}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2060}'..='\u{206F}'
                | '\u{3164}'
                | '\u{FEFF}'
                | '\u{FFF9}'..='\u{FFFB}'
                | '\u{E0000}'..='\u{E007F}'
        )
}

fn validate_sha256_hex(field: &'static str, value: &str) -> Result<(), UavError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(UavError::InvalidSha256Hex { field });
    }
    Ok(())
}

fn validate_finite_range(
    field: &'static str,
    value: f64,
    min_inclusive: f64,
    max_inclusive: f64,
) -> Result<(), UavError> {
    if !value.is_finite() || value < min_inclusive || value > max_inclusive {
        return Err(UavError::InvalidNumericField { field });
    }
    Ok(())
}

fn validate_half_open_angle(
    field: &'static str,
    value: f64,
    min_inclusive: f64,
    max_exclusive: f64,
) -> Result<(), UavError> {
    if !value.is_finite() || value < min_inclusive || value >= max_exclusive {
        return Err(UavError::InvalidNumericField { field });
    }
    Ok(())
}

struct BoundedWriter {
    buf: String,
    max_bytes: usize,
}

impl BoundedWriter {
    fn new(max_bytes: usize) -> Self {
        Self {
            buf: String::new(),
            max_bytes,
        }
    }

    fn write_str(&mut self, s: &str) -> Result<(), UavError> {
        if self.buf.len().saturating_add(s.len()) > self.max_bytes {
            return Err(UavError::LimitExceeded {
                kind: UavLimitKind::OutputBytes,
                limit: self.max_bytes,
            });
        }
        self.buf.push_str(s);
        Ok(())
    }

    fn write_fmt(&mut self, args: fmt::Arguments<'_>) -> Result<(), UavError> {
        struct IncrementingAdapter<'a> {
            inner: &'a mut BoundedWriter,
            overflowed: bool,
        }

        impl fmt::Write for IncrementingAdapter<'_> {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                if self.inner.write_str(s).is_err() {
                    self.overflowed = true;
                    return Err(fmt::Error);
                }
                Ok(())
            }
        }

        let max_bytes = self.max_bytes;
        let mut adapter = IncrementingAdapter {
            inner: self,
            overflowed: false,
        };
        if fmt::Write::write_fmt(&mut adapter, args).is_err() {
            return Err(UavError::LimitExceeded {
                kind: UavLimitKind::OutputBytes,
                limit: max_bytes,
            });
        }
        Ok(())
    }

    fn finish(self) -> String {
        self.buf
    }
}
