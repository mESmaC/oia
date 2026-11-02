//! `oia-devices` — Explicit hardware registration, capability boundaries, receive-only radio
//! and telemetry-only drone enforcement, independent telemetry buffering, and UAV HUD state (`RADIO-01`–`RADIO-03`, `DEVICE-01`–`DEVICE-04`).

use std::collections::VecDeque;
use std::fmt;

use oia_domain::OpaqueId;
use oia_time_geo::{AltitudeValue, ClockAlignment, GeoPoint};

/// Permitted vs prohibited hardware capabilities under OIA's agreed capability boundaries (`AGENTS.md` Section 6).
///
/// Video does not imply telemetry or flight control; audio streams do not imply RF waterfall tuning;
/// initial radio scope is receive-only; initial UAV scope leaves flight control to the existing controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DeviceCapability {
    /// Live or recorded video stream reception.
    VideoStream,
    /// Read-only UAV or sensor telemetry reception.
    TelemetryReception,
    /// Post-flight or onboard media file import.
    OnboardMediaImport,
    /// Receive-only radio stream or SDR reception (`RADIO-03`).
    ReceiveOnlyRadio,
    /// True RF spectrum/waterfall from an SDR providing IQ/spectrum data (`RADIO-02`).
    RfSpectrumWaterfall,
    /// Audio-only spectrogram from demodulated audio (`RADIO-02`).
    AudioSpectrogramOnly,
    /// Prohibited in initial scope: UAV flight control command emission (`DEVICE-04`).
    FlightControlCommand,
    /// Prohibited in initial scope: radio transmission command (`RADIO-03`).
    RadioTransmitCommand,
}

impl DeviceCapability {
    /// Validates whether this capability is allowed in OIA's initial operational scope.
    pub const fn validate_scope(self) -> Result<(), DeviceError> {
        match self {
            Self::FlightControlCommand => Err(DeviceError::ProhibitedFlightControl),
            Self::RadioTransmitCommand => Err(DeviceError::ProhibitedRadioTransmission),
            _ => Ok(()),
        }
    }
}

/// Freshness state of an individual UAV HUD telemetry field (`DEVICE-02`, `A12`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelemetryFieldStatus {
    /// Freshly observed within nominal cadence.
    Fresh,
    /// Last sample exceeds nominal cadence; shown with an explicit stale indicator.
    Stale {
        /// Age in milliseconds at readout time.
        age_ms: u64,
    },
    /// Derived from other sensors/models rather than directly measured.
    Derived,
}

/// A typed UAV HUD readout field that preserves `None` when a sensor value is absent (`DEVICE-02`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HudField<T> {
    /// Value with explicit units/datum.
    pub value: T,
    /// Freshness or derivation status.
    pub status: TelemetryFieldStatus,
}

/// Snapshot of UAV HUD telemetry (`DEVICE-02`, `A12`).
///
/// Missing fields remain `None` rather than defaulting to `0.0`.
#[derive(Debug, Clone, PartialEq)]
pub struct UavTelemetrySample {
    /// Device session ID.
    pub session_id: OpaqueId,
    /// Device capture timestamp (UTC ms) or aligned timestamp.
    pub timestamp_utc_ms: i64,
    /// Magnetic or true heading in degrees `[0.0, 360.0)`.
    pub heading_deg: Option<HudField<f64>>,
    /// Pitch in degrees `[-90.0, 90.0]`.
    pub pitch_deg: Option<HudField<f64>>,
    /// Roll in degrees `[-180.0, 180.0]`.
    pub roll_deg: Option<HudField<f64>>,
    /// Horizontal position and optional altitude with explicit datum.
    pub position: Option<HudField<GeoPoint>>,
    /// Separate altitude field if reported independently of 2D position.
    pub altitude: Option<HudField<AltitudeValue>>,
    /// Ground speed in meters per second.
    pub ground_speed_mps: Option<HudField<f64>>,
    /// Independent gimbal pitch in degrees (kept distinct from airframe pitch, `A12`).
    pub gimbal_pitch_deg: Option<HudField<f64>>,
    /// Independent gimbal yaw in degrees.
    pub gimbal_yaw_deg: Option<HudField<f64>>,
    /// Remaining battery percentage (`0..=100`).
    pub battery_percent: Option<HudField<u8>>,
}

/// Independent bounded telemetry buffer that accepts live device samples without blocking on
/// background media analysis or model inference (`DEVICE-04`, `A12`).
#[derive(Debug, Clone)]
pub struct TelemetryChannel {
    device_id: OpaqueId,
    capabilities: Vec<DeviceCapability>,
    clock_alignment: Option<ClockAlignment>,
    max_samples: usize,
    samples: VecDeque<UavTelemetrySample>,
    dropped_sample_count: u64,
}

impl TelemetryChannel {
    /// Registers a bounded telemetry channel for `device_id`, rejecting prohibited control/transmit capabilities.
    pub fn new(
        device_id: OpaqueId,
        capabilities: Vec<DeviceCapability>,
        clock_alignment: Option<ClockAlignment>,
        max_samples: usize,
    ) -> Result<Self, DeviceError> {
        if max_samples == 0 {
            return Err(DeviceError::ZeroBufferCapacity);
        }
        for &cap in &capabilities {
            cap.validate_scope()?;
        }
        Ok(Self {
            device_id,
            capabilities,
            clock_alignment,
            max_samples,
            samples: VecDeque::with_capacity(max_samples.min(256)),
            dropped_sample_count: 0,
        })
    }

    /// Returns the registered device ID.
    #[must_use]
    pub const fn device_id(&self) -> &OpaqueId {
        &self.device_id
    }

    /// Returns declared capabilities.
    #[must_use]
    pub fn capabilities(&self) -> &[DeviceCapability] {
        &self.capabilities
    }

    /// Returns Optional [`ClockAlignment`].
    #[must_use]
    pub const fn clock_alignment(&self) -> Option<&ClockAlignment> {
        self.clock_alignment.as_ref()
    }

    /// Non-blocking ingestion of a telemetry sample. If the bounded ring buffer is full,
    /// evicts the oldest transient sample and increments `dropped_sample_count` so drops remain visible (`DEVICE-04`).
    pub fn push_sample(&mut self, sample: UavTelemetrySample) -> Result<(), DeviceError> {
        if !self
            .capabilities
            .contains(&DeviceCapability::TelemetryReception)
        {
            return Err(DeviceError::CapabilityNotRegistered(
                DeviceCapability::TelemetryReception,
            ));
        }
        if self.samples.len() >= self.max_samples {
            self.samples.pop_front();
            self.dropped_sample_count += 1;
        }
        self.samples.push_back(sample);
        Ok(())
    }

    /// Returns the latest telemetry sample, if any.
    #[must_use]
    pub fn latest_sample(&self) -> Option<&UavTelemetrySample> {
        self.samples.back()
    }

    /// Returns the count of dropped samples due to buffer bounds.
    #[must_use]
    pub const fn dropped_sample_count(&self) -> u64 {
        self.dropped_sample_count
    }
}

/// Typed errors for `oia-devices`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceError {
    /// Flight-control commands are prohibited in initial UAV scope (`DEVICE-04`, `A12`).
    ProhibitedFlightControl,
    /// Radio transmission commands are prohibited in initial receive-only radio scope (`RADIO-03`, `A11`).
    ProhibitedRadioTransmission,
    /// Operation requires a capability not declared on this device session.
    CapabilityNotRegistered(DeviceCapability),
    /// Telemetry ring buffer capacity must be `>= 1`.
    ZeroBufferCapacity,
}

impl fmt::Display for DeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProhibitedFlightControl => write!(
                f,
                "UAV flight-control commands are prohibited; flight remains with the existing controller (DEVICE-04)"
            ),
            Self::ProhibitedRadioTransmission => write!(
                f,
                "radio transmission commands are prohibited; radio scope is receive-only (RADIO-03)"
            ),
            Self::CapabilityNotRegistered(cap) => {
                write!(f, "device session does not declare capability {cap:?}")
            }
            Self::ZeroBufferCapacity => write!(f, "telemetry buffer capacity must be >= 1"),
        }
    }
}

impl std::error::Error for DeviceError {}
