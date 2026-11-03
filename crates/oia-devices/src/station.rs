//! Bounded event capture planning over caller-supplied retained observation references (#34).
//! No device access, byte copying, persistence, publication, or time conversion occurs here.

use oia_domain::{OpaqueId, VersionId};
use std::{collections::BTreeSet, error::Error, fmt};

/// Half-open interval in caller-normalized UTC epoch milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    start: i64,
    end: i64,
}
impl Window {
    pub fn new(start: i64, end: i64) -> Result<Self, CaptureError> {
        if start >= end {
            return Err(CaptureError::InvalidWindow);
        }
        Ok(Self { start, end })
    }
    pub fn start(self) -> i64 {
        self.start
    }
    pub fn end(self) -> i64 {
        self.end
    }
    fn intersection(self, other: Self) -> Option<Self> {
        let start = self.start.max(other.start);
        let end = self.end.min(other.end);
        (start < end).then_some(Self { start, end })
    }
}

/// Alignment is supplied by the adapter; receipt time must not substitute for capture time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Timing {
    Unknown,
    Aligned {
        interval: Window,
        alignment_version: VersionId,
        uncertainty_ms: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Observation,
    Simulation,
}

/// Health at manifest creation; an offline device may still have retained evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    Available,
    Degraded,
    Disconnected,
    Unknown,
}

/// A retained artifact version reference. Expiry is exclusive: at expiry it is unavailable.
/// The caller must validate byte availability/access and pin storage before using this plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetainedObservation {
    pub artifact_version: VersionId,
    pub timing: Timing,
    pub expires_at_utc_ms: i64,
    pub origin: Origin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInventory {
    pub device_id: OpaqueId,
    pub source_version: VersionId,
    pub health: Health,
    pub observations: Vec<RetainedObservation>,
}

/// Hard ceilings prevent a configuration from removing resource bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureLimits {
    devices: usize,
    observations: usize,
}
impl CaptureLimits {
    pub fn new(devices: usize, observations: usize) -> Result<Self, CaptureError> {
        if devices == 0 || devices > 256 || observations == 0 || observations > 65_536 {
            return Err(CaptureError::InvalidLimits);
        }
        Ok(Self {
            devices,
            observations,
        })
    }
}
impl Default for CaptureLimits {
    fn default() -> Self {
        Self {
            devices: 32,
            observations: 4096,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    /// Zero declared clock uncertainty and temporal overlap; not a truth/authenticity verdict.
    Selected,
    /// Retained but overlap is uncertain; does not contribute to recorded coverage.
    UncertainTiming,
    Expired,
    OutsideWindow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedObservation {
    observation: RetainedObservation,
    disposition: Disposition,
    overlap: Option<Window>,
}
impl PlannedObservation {
    pub fn observation(&self) -> &RetainedObservation {
        &self.observation
    }
    pub fn disposition(&self) -> Disposition {
        self.disposition
    }
    /// Clipped nominal overlap, absent when unavailable or capture time is unknown.
    pub fn overlap(&self) -> Option<Window> {
        self.overlap
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCapture {
    device_id: OpaqueId,
    source_version: VersionId,
    health: Health,
    observations: Vec<PlannedObservation>,
    recorded_coverage: Vec<Window>,
    gaps: Vec<Window>,
}
impl DeviceCapture {
    pub fn device_id(&self) -> &OpaqueId {
        &self.device_id
    }
    pub fn source_version(&self) -> &VersionId {
        &self.source_version
    }
    pub fn health(&self) -> Health {
        self.health
    }
    pub fn observations(&self) -> &[PlannedObservation] {
        &self.observations
    }
    /// Union of selected real observations only. Simulations never fill evidence gaps.
    pub fn recorded_coverage(&self) -> &[Window] {
        &self.recorded_coverage
    }
    /// Uncovered request intervals; these indicate missing retained coverage, not observed absence.
    pub fn gaps(&self) -> &[Window] {
        &self.gaps
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureManifest {
    event_version: VersionId,
    requested: Window,
    planned_at_utc_ms: i64,
    devices: Vec<DeviceCapture>,
}
impl CaptureManifest {
    pub fn event_version(&self) -> &VersionId {
        &self.event_version
    }
    pub fn requested(&self) -> Window {
        self.requested
    }
    pub fn planned_at_utc_ms(&self) -> i64 {
        self.planned_at_utc_ms
    }
    pub fn devices(&self) -> &[DeviceCapture] {
        &self.devices
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureError {
    InvalidWindow,
    InvalidLimits,
    DeviceLimit,
    ObservationLimit,
    DuplicateDevice,
    DuplicateArtifact,
    ClockRange,
}
impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidWindow => "capture window must have start before end",
            Self::InvalidLimits => {
                "limits must be within 1..=256 devices and 1..=65536 observations"
            }
            Self::DeviceLimit => "device inventory exceeds capture budget",
            Self::ObservationLimit => "observation inventory exceeds capture budget",
            Self::DuplicateDevice => "device identity is repeated",
            Self::DuplicateArtifact => "artifact version is repeated across the station",
            Self::ClockRange => "clock uncertainty interval exceeds supported i64 UTC range",
        })
    }
}
impl Error for CaptureError {}

/// Plans an immutable data snapshot without reading, pinning, or publishing bytes.
/// Inventory must already be scoped to the caller's authorized station. No access policy is
/// inferred from a local connection. Input ordering is normalized by device/artifact identity.
/// All inventory is validated, including expired records; budget overflow refuses the whole plan.
pub fn plan_capture(
    event_version: VersionId,
    requested: Window,
    planned_at_utc_ms: i64,
    inventory: &[DeviceInventory],
    limits: CaptureLimits,
) -> Result<CaptureManifest, CaptureError> {
    if inventory.len() > limits.devices {
        return Err(CaptureError::DeviceLimit);
    }
    let mut count = 0usize;
    for device in inventory {
        count = count
            .checked_add(device.observations.len())
            .ok_or(CaptureError::ObservationLimit)?;
        if count > limits.observations {
            return Err(CaptureError::ObservationLimit);
        }
    }
    let mut device_ids = BTreeSet::new();
    let mut artifact_ids = BTreeSet::new();
    let mut devices = Vec::with_capacity(inventory.len());
    for device in inventory {
        if !device_ids.insert(device.device_id.clone()) {
            return Err(CaptureError::DuplicateDevice);
        }
        let mut observations = Vec::with_capacity(device.observations.len());
        let mut spans = Vec::new();
        for observation in &device.observations {
            if !artifact_ids.insert(observation.artifact_version.clone()) {
                return Err(CaptureError::DuplicateArtifact);
            }
            let (nominal, possible, exact) = match &observation.timing {
                Timing::Unknown => (None, true, false),
                Timing::Aligned {
                    interval,
                    uncertainty_ms,
                    ..
                } => {
                    let uncertainty =
                        i64::try_from(*uncertainty_ms).map_err(|_| CaptureError::ClockRange)?;
                    let possible = Window {
                        start: interval
                            .start
                            .checked_sub(uncertainty)
                            .ok_or(CaptureError::ClockRange)?,
                        end: interval
                            .end
                            .checked_add(uncertainty)
                            .ok_or(CaptureError::ClockRange)?,
                    };
                    (
                        interval.intersection(requested),
                        possible.intersection(requested).is_some(),
                        uncertainty == 0,
                    )
                }
            };
            let disposition = if observation.expires_at_utc_ms <= planned_at_utc_ms {
                Disposition::Expired
            } else if !possible {
                Disposition::OutsideWindow
            } else if !exact {
                Disposition::UncertainTiming
            } else {
                Disposition::Selected
            };
            let overlap = if matches!(
                disposition,
                Disposition::Selected | Disposition::UncertainTiming
            ) {
                nominal
            } else {
                None
            };
            if disposition == Disposition::Selected
                && observation.origin == Origin::Observation
                && let Some(span) = overlap
            {
                spans.push(span);
            }
            observations.push(PlannedObservation {
                observation: observation.clone(),
                disposition,
                overlap,
            });
        }
        observations.sort_by(|a, b| {
            a.observation
                .artifact_version
                .cmp(&b.observation.artifact_version)
        });
        spans.sort_by_key(|s| (s.start, s.end));
        let mut merged: Vec<Window> = Vec::new();
        for span in spans {
            if let Some(last) = merged.last_mut().filter(|last| span.start <= last.end) {
                last.end = last.end.max(span.end);
            } else {
                merged.push(span);
            }
        }
        let mut gaps = Vec::new();
        let mut cursor = requested.start;
        for span in &merged {
            if cursor < span.start {
                gaps.push(Window {
                    start: cursor,
                    end: span.start,
                });
            }
            cursor = span.end;
        }
        if cursor < requested.end {
            gaps.push(Window {
                start: cursor,
                end: requested.end,
            });
        }
        devices.push(DeviceCapture {
            device_id: device.device_id.clone(),
            source_version: device.source_version.clone(),
            health: device.health,
            observations,
            recorded_coverage: merged,
            gaps,
        });
    }
    devices.sort_by(|a, b| a.device_id.cmp(&b.device_id));
    Ok(CaptureManifest {
        event_version,
        requested,
        planned_at_utc_ms,
        devices,
    })
}
