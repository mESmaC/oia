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
            max_text_bytes: 65_536,
            max_output_bytes: 65_536,
            stale_telemetry_after_ms: 2_000,
        }
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClockAlignmentRecord {
    /// Immutable version of this clock alignment calibration.
    pub alignment_version: VersionId,
    /// Base offset in milliseconds added to `device_clock_ms` at `valid_from_device_ms` to obtain
    /// UTC milliseconds (`[-86_400_000, 86_400_000]`).
    pub device_to_utc_offset_ms: i64,
    /// Linear clock drift in parts per million relative to `valid_from_device_ms`
    /// (`[-100_000, 100_000]`).
    pub linear_drift_ppm: i32,
    /// Declared alignment uncertainty (± milliseconds).
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
    /// [`UavError::IncompatibleAltitudeReference`] if the sample carries a different reference
    /// (`docs/architecture.md` §3.2).
    pub fn try_in_reference(&self, requested: AltitudeReference) -> Result<f64, UavError> {
        if self.reference != requested {
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

/// Gimbal stabilization mode (`DEVICE-02`, `DEVICE-04`, `A12`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GimbalMode {
    /// Gimbal yaw follows airframe heading (`yaw_deg` is relative to airframe nose).
    FollowHeading,
    /// Gimbal yaw is stabilized independently in world true-north coordinates (`A12`).
    IndependentWorldLocked,
}

/// Validated gimbal orientation in degrees (`DEVICE-02`, `DEVICE-04`, `A12`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GimbalOrientation {
    /// Gimbal pitch in degrees `[-120.0, 45.0]` (negative = looking downward below horizontal).
    pub pitch_deg: f64,
    /// Gimbal roll in degrees `[-90.0, 90.0]`.
    pub roll_deg: f64,
    /// Gimbal yaw in degrees `[-180.0, 360.0)` (interpreted according to [`GimbalMode`]).
    pub yaw_deg: f64,
    /// Whether gimbal yaw follows the airframe or moves independently in world coordinates.
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

    /// Computed clock alignment at ingestion.
    pub fn time_alignment(&self) -> &TimeAlignment {
        &self.time_alignment
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoFrameRecord {
    frame_index: u64,
    frame_version: VersionId,
    device_clock_ms: i64,
    receipt_utc_ms: i64,
    time_alignment: TimeAlignment,
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

    /// Clock alignment status for this frame.
    pub fn time_alignment(&self) -> &TimeAlignment {
        &self.time_alignment
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
    /// No camera calibration has been attached to the session.
    Uncalibrated,
    /// Active validated calibration version.
    Valid(CameraCalibration),
    /// Previous calibration was invalidated by camera/mount movement or mechanical shock.
    Invalidated {
        /// Version of the calibration that was invalidated.
        calibration_version: VersionId,
        /// Reason the calibration is no longer valid.
        reason: String,
    },
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
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LookDirectionMarker {
    /// Derived world azimuth of the camera optical axis in degrees `[0.0, 360.0)`.
    pub world_azimuth_deg: f64,
    /// Derived depression angle below the local horizontal plane in degrees `[-90.0, 90.0]`
    /// (positive = looking downward).
    pub depression_deg: f64,
    /// North reference inherited from the heading sample.
    pub azimuth_reference: HeadingReference,
    /// Whether either the heading or gimbal input used to derive this ray was stale.
    pub derived_from_stale_input: bool,
    /// Always `false`: a directional viewing marker never measures slant range (`DEVICE-02`).
    pub establishes_slant_range: bool,
}

/// Derived flat-ground camera footprint estimate when valid calibration and `AboveGroundLevel`
/// clearance are available (`DEVICE-02`, `A12`).
#[derive(Debug, Clone, PartialEq)]
pub struct CalibratedFootprint {
    /// Pinned camera calibration version used for this projection.
    pub calibration_version: VersionId,
    /// Derived optical-axis slant range to the flat ground plane in meters (`> 0.0`).
    pub center_slant_range_m: f64,
    /// Derived horizontal ground distance from the nadir point to the footprint center (`m`).
    pub center_ground_distance_m: f64,
    /// Derived north offset from aircraft nadir to footprint center (`m`).
    pub center_north_offset_m: f64,
    /// Derived east offset from aircraft nadir to footprint center (`m`).
    pub center_east_offset_m: f64,
    /// Approximate along-look ground span in meters (`> 0.0`).
    pub along_look_span_m: f64,
    /// Approximate cross-look ground span in meters (`> 0.0`).
    pub cross_look_span_m: f64,
    /// Propagated 1-sigma horizontal footprint center uncertainty in meters (`>= 0.0`).
    pub horizontal_uncertainty_m: f64,
    /// Explicit assumption disclosure (`docs/architecture.md` §§3.2, 8).
    pub terrain_assumption: &'static str,
    /// Always `true`: footprint geometry is a derived model estimate, not a direct rangefinder
    /// observation (`DEVICE-02`, `AGENTS.md` §2).
    pub is_derived_estimate: bool,
}

/// Outcome of evaluating the camera ground footprint for a HUD snapshot (`DEVICE-02`, `A12`).
#[derive(Debug, Clone, PartialEq)]
pub enum FootprintStatus {
    /// No camera calibration is attached; only a directional viewing ray is available.
    UncalibratedViewingRayOnly,
    /// Camera calibration was invalidated by camera/mount movement or mechanical shock (`A12`, `A13`).
    CalibrationInvalidated {
        /// Invalidated calibration version.
        calibration_version: VersionId,
        /// Recorded invalidation reason.
        reason: String,
    },
    /// Required telemetry for footprint projection (`heading`, `attitude`, `position`, `altitude`,
    /// or `gimbal`) is unavailable or stale.
    InsufficientFreshTelemetry {
        /// Name of the first missing or stale required field.
        missing_or_stale_field: &'static str,
    },
    /// Altitude was reported in a datum that does not establish clearance above local terrain
    /// (for example `BarometricUnadjusted`, `OrthometricMsl`, or `Wgs84Ellipsoid` without DEM).
    IncompatibleAltitudeReference {
        /// Reported altitude reference.
        actual: AltitudeReference,
    },
    /// Reported `AboveGroundLevel` height is non-positive (`<= 0.0 m`).
    NonPositiveHeightAboveGround {
        /// Reported AGL height in meters.
        height_m: f64,
    },
    /// Optical axis points above the horizon or within the grazing-angle exclusion zone
    /// (`depression_deg < MIN_FOOTPRINT_DEPRESSION_DEG`), so a unique bounded ground intersection
    /// cannot be resolved (`A12`, `A13`).
    UnresolvableGeometry {
        /// Computed depression angle below the horizontal plane in degrees.
        depression_deg: f64,
        /// Minimum required depression angle in degrees (`MIN_FOOTPRINT_DEPRESSION_DEG`).
        min_required_depression_deg: f64,
    },
    /// Validated derived ground footprint estimate under a local flat-ground assumption.
    CalibratedDerivedFootprint(CalibratedFootprint),
}

/// Complete capability- and freshness-aware UAV HUD projection at an inspection instant
/// (`DEVICE-02`, `DEVICE-04`, `A12`).
#[derive(Debug, Clone, PartialEq)]
pub struct UavHudSnapshot {
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
    /// Moving-camera dynamic disturbance flags from the latest fresh/stale sample.
    pub motion_flags: Option<MotionDynamicsFlags>,
    /// Directional viewing marker (explicitly marked `establishes_slant_range: false`).
    pub look_direction_marker: Option<LookDirectionMarker>,
    /// Camera ground footprint status (calibrated derived estimate vs. explicit refusal reason).
    pub footprint_status: FootprintStatus,
    /// Total cumulative dropped video frames recorded at or before `inspect_device_ms`.
    pub cumulative_dropped_frames: u64,
    /// Number of telemetry disconnection spans recorded at or before `inspect_device_ms`.
    pub disconnection_span_count: usize,
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
        match &self.look_direction_marker {
            Some(marker) => {
                out.write_fmt(format_args!(
                    "  look_marker: DERIVED azimuth={:.1} deg ({:?}), depression={:.1} deg, stale_input={}, establishes_slant_range={}\n",
                    marker.world_azimuth_deg,
                    marker.azimuth_reference,
                    marker.depression_deg,
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
                    "  footprint: DERIVED_ESTIMATE cal={} slant_range={:.1} m ground_dist={:.1} m offset_ne=({:.1} m, {:.1} m) span=({:.1} m x {:.1} m) ±{:.1} m [{}]\n",
                    fp.calibration_version.as_str(),
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
/// alignment, and moving-camera disturbance flags by value so ongoing telemetry and frame
/// ingestion on [`UavFlightSession`] never blocks or mutates the handoff.
#[derive(Debug, Clone, PartialEq)]
pub struct ForensicFrameHandoff {
    handoff_id: OpaqueId,
    source_device_id: OpaqueId,
    source_stream_version: VersionId,
    frame: VideoFrameRecord,
    linked_bookmark_id: Option<OpaqueId>,
    hud_snapshot_at_frame: UavHudSnapshot,
    parallax_and_range_caveat: &'static str,
}

/// Mandatory moving-camera parallax and uncalibrated-range caveat attached to every forensic
/// frame handoff (`DEVICE-02`, `DEVICE-04`, `MEDIA-02`).
pub const MOVING_CAMERA_FORENSIC_CAVEAT: &str = "Moving-camera frame handoff: apparent target motion includes airframe translation/parallax, attitude changes, gimbal slew, vibration, rolling shutter, and exposure shifts; a directional viewing marker alone does not establish slant range.";

impl ForensicFrameHandoff {
    /// Handoff identifier.
    pub fn handoff_id(&self) -> &OpaqueId {
        &self.handoff_id
    }

    /// Registered device identifier.
    pub fn source_device_id(&self) -> &OpaqueId {
        &self.source_device_id
    }

    /// Pinned original stream version from which the frame was copied.
    pub fn source_stream_version(&self) -> &VersionId {
        &self.source_stream_version
    }

    /// Pinned unaltered video frame record.
    pub fn frame(&self) -> &VideoFrameRecord {
        &self.frame
    }

    /// Optional bookmark identifier associated with this handoff.
    pub fn linked_bookmark_id(&self) -> Option<&OpaqueId> {
        self.linked_bookmark_id.as_ref()
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
            "Forensic Frame Handoff `{}`\n  device_id: {}\n  source_stream_version: {}\n  frame_index: {}\n  frame_version: {}\n  frame_sha256: {}\n  exposure_duration_us: {:?}\n  linked_bookmark_id: {:?}\n  caveat: \"{}\"\n",
            self.handoff_id.as_str(),
            self.source_device_id.as_str(),
            self.source_stream_version.as_str(),
            self.frame.frame_index(),
            self.frame.frame_version().as_str(),
            self.frame.sha256_hex(),
            self.frame.exposure_duration_us(),
            self.linked_bookmark_id.as_ref().map(OpaqueId::as_str),
            self.parallax_and_range_caveat
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
    /// recording timeline (`[-86_400_000, 86_400_000]`).
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
    clock_alignment: Option<ClockAlignmentRecord>,
    camera_calibration: Option<CameraCalibration>,
    calibration_invalidation_reason: Option<(VersionId, String)>,
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
            clock_alignment: None,
            camera_calibration: None,
            calibration_invalidation_reason: None,
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

    /// Current camera calibration state (`Uncalibrated`, `Valid`, or `Invalidated`).
    pub fn camera_calibration_state(&self) -> CameraCalibrationState {
        if let Some((cal_ver, reason)) = &self.calibration_invalidation_reason {
            CameraCalibrationState::Invalidated {
                calibration_version: cal_ver.clone(),
                reason: reason.clone(),
            }
        } else if let Some(cal) = &self.camera_calibration {
            CameraCalibrationState::Valid(cal.clone())
        } else {
            CameraCalibrationState::Uncalibrated
        }
    }

    /// Configures or updates the session's documented clock-alignment record (`DEVICE-04`).
    pub fn set_clock_alignment(&mut self, record: ClockAlignmentRecord) -> Result<(), UavError> {
        if record.valid_from_device_ms >= record.valid_until_device_ms {
            return Err(UavError::InvalidClockValidityWindow {
                valid_from_device_ms: record.valid_from_device_ms,
                valid_until_device_ms: record.valid_until_device_ms,
            });
        }
        if record.device_to_utc_offset_ms.abs() > MAX_ABS_CLOCK_OFFSET_MS {
            return Err(UavError::ClockOffsetOutOfRange {
                offset_ms: record.device_to_utc_offset_ms,
            });
        }
        if record.linear_drift_ppm.abs() > MAX_ABS_CLOCK_DRIFT_PPM {
            return Err(UavError::ClockDriftOutOfRange {
                drift_ppm: record.linear_drift_ppm,
            });
        }
        if self.known_versions.contains(&record.alignment_version) {
            return Err(UavError::DuplicateOrOverwrittenVersion {
                version_id: record.alignment_version,
            });
        }
        let mut budget_preview = self.remaining_text_budget;
        validate_single_line_text(
            "reference_clock_label",
            &record.reference_clock_label,
            &mut budget_preview,
        )?;
        validate_single_line_text("method_label", &record.method_label, &mut budget_preview)?;

        self.remaining_text_budget = budget_preview;
        self.known_versions.insert(record.alignment_version.clone());
        self.clock_alignment = Some(record);
        Ok(())
    }

    /// Attaches a validated camera calibration to the session (`DEVICE-02`, `FIELD-03`).
    pub fn set_camera_calibration(
        &mut self,
        calibration: CameraCalibration,
    ) -> Result<(), UavError> {
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
        self.known_versions
            .insert(calibration.calibration_version.clone());
        self.camera_calibration = Some(calibration);
        self.calibration_invalidation_reason = None;
        Ok(())
    }

    /// Marks the active camera calibration as invalidated due to mount movement, shock, or lens
    /// change (`A12`, `A13`).
    pub fn invalidate_camera_calibration(&mut self, reason: &str) -> Result<(), UavError> {
        let cal = self
            .camera_calibration
            .take()
            .ok_or(UavError::NoActiveCalibrationToInvalidate)?;
        let mut budget_preview = self.remaining_text_budget;
        validate_single_line_text("invalidation_reason", reason, &mut budget_preview)?;
        self.remaining_text_budget = budget_preview;
        self.calibration_invalidation_reason = Some((cal.calibration_version, reason.to_owned()));
        Ok(())
    }

    /// Aligns a device-clock timestamp using the active [`ClockAlignmentRecord`] when within its
    /// validity window, or returns [`TimeAlignment::UnsynchronizedDegraded`] (`docs/architecture.md` §3.1).
    pub fn align_device_timestamp(
        &self,
        device_clock_ms: i64,
        receipt_utc_ms: i64,
    ) -> TimeAlignment {
        match &self.clock_alignment {
            None => TimeAlignment::UnsynchronizedDegraded {
                device_clock_ms,
                receipt_utc_ms,
                reason: UnsynchronizedReason::NoClockAlignmentConfigured,
            },
            Some(cal) => {
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
                    let drift_adjustment_ms = i64::try_from(drift_adjustment_i128).unwrap_or(0);
                    let aligned_utc_ms = device_clock_ms
                        .saturating_add(cal.device_to_utc_offset_ms)
                        .saturating_add(drift_adjustment_ms);
                    let drift_uncertainty_ms = u32::try_from(
                        (elapsed_ms.unsigned_abs() / 100_000).min(u128::from(u32::MAX)),
                    )
                    .unwrap_or(u32::MAX);
                    TimeAlignment::Synchronized {
                        aligned_utc_ms,
                        alignment_version: cal.alignment_version.clone(),
                        base_offset_ms: cal.device_to_utc_offset_ms,
                        drift_adjustment_ms,
                        total_uncertainty_ms: cal
                            .uncertainty_ms
                            .saturating_add(drift_uncertainty_ms),
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
        self.known_versions.insert(input.frame_version.clone());
        self.video_frames.push(VideoFrameRecord {
            frame_index: input.frame_index,
            frame_version: input.frame_version,
            device_clock_ms: input.device_clock_ms,
            receipt_utc_ms: input.receipt_utc_ms,
            time_alignment,
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
                motion_flags: None,
                look_direction_marker: None,
                footprint_status: FootprintStatus::InsufficientFreshTelemetry {
                    missing_or_stale_field: "heading",
                },
                cumulative_dropped_frames,
                disconnection_span_count,
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
                motion_flags: None,
                look_direction_marker: None,
                footprint_status: FootprintStatus::InsufficientFreshTelemetry {
                    missing_or_stale_field: "heading",
                },
                cumulative_dropped_frames,
                disconnection_span_count,
            };
        };

        let sample_ms = sample.device_clock_ms;
        let age_ms = inspect_device_ms.saturating_sub(sample_ms);
        let disconnecting_span = self.disconnections.iter().find(|d| {
            d.disconnected_at_device_ms <= inspect_device_ms
                && d.reconnected_at_device_ms
                    .is_none_or(|reconn| sample_ms < reconn)
        });

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

        let look_direction_marker = derive_look_marker(&heading, &attitude, &gimbal);
        let footprint_status =
            self.evaluate_footprint(&heading, &attitude, &position, &altitude, &gimbal);

        UavHudSnapshot {
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
            motion_flags: Some(sample.motion_flags),
            look_direction_marker,
            footprint_status,
            cumulative_dropped_frames,
            disconnection_span_count,
        }
    }

    fn evaluate_footprint(
        &self,
        heading: &HudFieldState<HeadingSample>,
        attitude: &HudFieldState<AttitudeDeg>,
        position: &HudFieldState<Wgs84Position>,
        altitude: &HudFieldState<AltitudeSample>,
        gimbal: &HudFieldState<GimbalOrientation>,
    ) -> FootprintStatus {
        if let Some((cal_ver, reason)) = &self.calibration_invalidation_reason {
            return FootprintStatus::CalibrationInvalidated {
                calibration_version: cal_ver.clone(),
                reason: reason.clone(),
            };
        }
        let Some(cal) = &self.camera_calibration else {
            return FootprintStatus::UncalibratedViewingRayOnly;
        };

        let Some(hdg) = heading.fresh_value() else {
            return FootprintStatus::InsufficientFreshTelemetry {
                missing_or_stale_field: "heading",
            };
        };
        let Some(att) = attitude.fresh_value() else {
            return FootprintStatus::InsufficientFreshTelemetry {
                missing_or_stale_field: "attitude",
            };
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
        let Some(gim) = gimbal.fresh_value() else {
            return FootprintStatus::InsufficientFreshTelemetry {
                missing_or_stale_field: "gimbal",
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

        let world_azimuth_deg = match gim.mode {
            GimbalMode::FollowHeading => (hdg.degrees + gim.yaw_deg).rem_euclid(360.0),
            GimbalMode::IndependentWorldLocked => gim.yaw_deg.rem_euclid(360.0),
        };
        // Negative gimbal pitch looks downward below the horizontal plane; combine with airframe pitch.
        let depression_deg = (-gim.pitch_deg - att.pitch_deg).clamp(-90.0, 90.0);
        let half_vfov_deg = cal.vertical_fov_deg * 0.5;
        let far_dep_deg = depression_deg - half_vfov_deg;
        if depression_deg < MIN_FOOTPRINT_DEPRESSION_DEG
            || far_dep_deg < MIN_FOOTPRINT_DEPRESSION_DEG
        {
            return FootprintStatus::UnresolvableGeometry {
                depression_deg: depression_deg.min(far_dep_deg),
                min_required_depression_deg: MIN_FOOTPRINT_DEPRESSION_DEG,
            };
        }

        let dep_rad = depression_deg.to_radians();
        let az_rad = world_azimuth_deg.to_radians();
        let center_slant_range_m = alt.meters / dep_rad.sin();
        let center_ground_distance_m = alt.meters / dep_rad.tan();
        let center_north_offset_m = center_ground_distance_m * az_rad.cos();
        let center_east_offset_m = center_ground_distance_m * az_rad.sin();

        let half_hfov_rad = (cal.horizontal_fov_deg * 0.5).to_radians();
        let cross_look_span_m = 2.0 * center_slant_range_m * half_hfov_rad.tan();

        let near_dep_deg = (depression_deg + half_vfov_deg).min(89.5);
        let near_ground_m = alt.meters / near_dep_deg.to_radians().tan();
        let far_ground_m = alt.meters / far_dep_deg.to_radians().tan();
        let along_look_span_m = (far_ground_m - near_ground_m).abs();

        let ang_unc_m = center_slant_range_m * cal.angular_uncertainty_deg.to_radians();
        let horizontal_uncertainty_m = pos_unc_m + (alt_unc_m / dep_rad.tan()) + ang_unc_m;

        FootprintStatus::CalibratedDerivedFootprint(CalibratedFootprint {
            calibration_version: cal.calibration_version.clone(),
            center_slant_range_m,
            center_ground_distance_m,
            center_north_offset_m,
            center_east_offset_m,
            along_look_span_m,
            cross_look_span_m,
            horizontal_uncertainty_m,
            terrain_assumption: "Local flat-ground approximation using AboveGroundLevel height; does not account for sloped terrain or obstacle occlusion.",
            is_derived_estimate: true,
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

        let hud_snapshot_at_frame =
            self.project_hud_snapshot(frame.device_clock_ms, frame.receipt_utc_ms);

        Ok(ForensicFrameHandoff {
            handoff_id,
            source_device_id: self.device.device_id().clone(),
            source_stream_version: self.original_stream.stream_version().clone(),
            frame,
            linked_bookmark_id,
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
        if input.stream_to_onboard_offset_ms.abs() > MAX_ABS_CLOCK_OFFSET_MS {
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

fn derive_look_marker(
    heading: &HudFieldState<HeadingSample>,
    attitude: &HudFieldState<AttitudeDeg>,
    gimbal: &HudFieldState<GimbalOrientation>,
) -> Option<LookDirectionMarker> {
    let (hdg, hdg_stale) = match heading {
        HudFieldState::Fresh { value, .. } => (value, false),
        HudFieldState::Stale { value, .. } => (value, true),
        HudFieldState::Unavailable { .. } => return None,
    };
    let (gim, gim_stale) = match gimbal {
        HudFieldState::Fresh { value, .. } => (value, false),
        HudFieldState::Stale { value, .. } => (value, true),
        HudFieldState::Unavailable { .. } => return None,
    };
    let (att_pitch, att_stale) = match attitude {
        HudFieldState::Fresh { value, .. } => (value.pitch_deg, false),
        HudFieldState::Stale { value, .. } => (value.pitch_deg, true),
        HudFieldState::Unavailable { .. } => return None,
    };

    let world_azimuth_deg = match gim.mode {
        GimbalMode::FollowHeading => (hdg.degrees + gim.yaw_deg).rem_euclid(360.0),
        GimbalMode::IndependentWorldLocked => gim.yaw_deg.rem_euclid(360.0),
    };
    let depression_deg = (-gim.pitch_deg - att_pitch).clamp(-90.0, 90.0);

    Some(LookDirectionMarker {
        world_azimuth_deg,
        depression_deg,
        azimuth_reference: hdg.reference,
        derived_from_stale_input: hdg_stale || gim_stale || att_stale,
        establishes_slant_range: false,
    })
}

/// Typed errors returned by the UAV registration, telemetry, HUD, bookmark, and handoff kernel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UavError {
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
