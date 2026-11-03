//! Sensor surveillance coverage and empty-scene absence verification (`GEO-05`, `A04`).

use std::fmt;

use crate::geo_context::provisional::{AltitudeBand, GeoBoundingBoxWgs84, TimeWindow};
use crate::geo_context::tracks::PositionSource;
use crate::geo_context::{ContextLimits, GeoContextError, SpatioTemporalEnvelope};
use oia_domain::OpaqueId;

/// Surveillance sensor registration mask describing operational bounds and feed freshness.
#[derive(Debug, Clone, PartialEq)]
pub struct SensorCoverageMask {
    pub sensor_id: OpaqueId,
    pub source_id: OpaqueId,
    pub coverage_bounds: GeoBoundingBoxWgs84,
    pub altitude_band: Option<AltitudeBand>,
    pub operational_window: TimeWindow,
    pub sensor_type: PositionSource,
    pub is_operational: bool,
    /// Timestamp of last successfully received and processed sensor telemetry packet.
    pub last_feed_update_utc_ms: i64,
}

impl SensorCoverageMask {
    pub fn validate(&self) -> Result<(), GeoContextError> {
        self.coverage_bounds.validate()?;
        self.operational_window.validate()?;
        if let Some(ref band) = self.altitude_band {
            band.min_altitude()
                .compare_in_same_reference(&band.max_altitude())?;
        }
        Ok(())
    }
}

/// Detailed reason why a spatio-temporal region cannot claim observed absence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnobservedReason {
    NoSensorsCoveringRegion,
    SensorFeedDownOrStale,
    PartialCoverageBlindSpots,
    IncompleteAltitudeCoverage,
    QueryIntervalOutsideSensorLogWindow,
}

impl fmt::Display for UnobservedReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSensorsCoveringRegion => {
                f.write_str("no active sensor feeds registered covering this region")
            }
            Self::PartialCoverageBlindSpots => {
                f.write_str("registered sensors have horizontal blind spots across the query area")
            }
            Self::SensorFeedDownOrStale => {
                f.write_str("sensor feed is down, offline, or telemetry packets are stale")
            }
            Self::IncompleteAltitudeCoverage => f.write_str(
                "altitude band is not completely encompassed by sensor vertical coverage",
            ),
            Self::QueryIntervalOutsideSensorLogWindow => {
                f.write_str("query time interval falls outside sensor operational logging window")
            }
        }
    }
}

/// Area-level coverage verification status over a query envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AreaCoverageStatus {
    UncoveredNoSensorData,
    PartiallyCoveredGapsIdentified {
        reason: UnobservedReason,
    },
    FullyCoveredByActiveSensors {
        verified_covering_sensor_count: usize,
    },
}

/// Scene traffic assessment ensuring "Absent Track is NEVER an Empty Scene" contract (`GEO-05`, `A04`).
#[derive(Debug, Clone, PartialEq)]
pub enum SceneTrafficAssessment {
    ObservedTrafficPresent {
        observed_aircraft_count: usize,
        observed_vessel_count: usize,
        coverage_status: AreaCoverageStatus,
    },
    ObservedAbsenceWithinCoverage {
        verified_covering_sensor_count: usize,
        note: &'static str,
    },
    InconclusiveUnobserved {
        reason: UnobservedReason,
    },
}

/// Evaluates sensor coverage over the query envelope with strict time, dateline-aware spatial,
/// altitude band containment, and feed staleness validation.
///
/// **Note on Union Coverage:** Currently, coverage is verified on a per-mask basis; each verified
/// sensor must encompass the query envelope in its entirety. Multi-mask geometric unions are deferred.
pub fn evaluate_sensor_coverage(
    envelope: &SpatioTemporalEnvelope,
    sensor_masks: &[SensorCoverageMask],
    limits: &ContextLimits,
    evaluation_instant_utc_ms: i64,
) -> Result<AreaCoverageStatus, GeoContextError> {
    limits.validate()?;

    if sensor_masks.is_empty() {
        return Ok(AreaCoverageStatus::UncoveredNoSensorData);
    }

    let mut covering_sensors = 0;
    let mut any_sensor_stale = false;
    let mut any_sensor_offline = false;
    let mut any_outside_log = false;
    let mut any_altitude_gap = false;
    let mut any_horizontal_partial = false;

    for mask in sensor_masks {
        mask.validate()?;

        // Refuse sensor feeds with last_update in the future relative to the evaluation instant
        if mask.last_feed_update_utc_ms > evaluation_instant_utc_ms {
            return Err(GeoContextError::InvalidTemporalOrdering(
                "sensor feed last_feed_update_utc_ms cannot be later than evaluation_instant_utc_ms",
            ));
        }

        // Skip sensors that do not overlap the query envelope spatially
        if !mask.coverage_bounds.overlaps_box(&envelope.spatial_bounds) {
            continue;
        }

        // Liveness check: sensor must be operational
        if !mask.is_operational {
            any_sensor_offline = true;
            continue;
        }

        // Temporal logging window containment check
        if !mask
            .operational_window
            .contains_window(&envelope.time_window)
        {
            any_outside_log = true;
            continue;
        }

        // Staleness check: feed must have reported through the envelope's time window.
        // If last update is before the window's end, telemetry inside the window is incomplete.
        if mask.last_feed_update_utc_ms < envelope.time_window.end_utc_ms() {
            any_sensor_stale = true;
            continue;
        }

        // Age check: applies ONLY when the envelope window reaches into or beyond the evaluation instant
        // (i.e. real-time or in-progress evaluation). For an archived window that ended in the past,
        // log completeness through envelope.time_window.end_utc_ms() is sufficient regardless of evaluation age.
        if envelope.time_window.end_utc_ms() >= evaluation_instant_utc_ms
            && evaluation_instant_utc_ms.saturating_sub(mask.last_feed_update_utc_ms)
                > limits.max_sensor_staleness_ms as i64
        {
            any_sensor_stale = true;
            continue;
        }

        // Horizontal spatial containment check: mask must completely contain the envelope box
        if !mask.coverage_bounds.contains_box(&envelope.spatial_bounds) {
            any_horizontal_partial = true;
            continue;
        }

        // Altitude containment check:
        // - If envelope specifies an altitude band: mask must encompass it in the same reference frame.
        // - If envelope has NO altitude band: envelope is unbounded across all altitudes; a bounded sensor leaves gaps!
        match (&envelope.altitude_band, &mask.altitude_band) {
            (Some(env_band), Some(mask_band)) => match mask_band.contains_band(env_band) {
                Ok(true) => {}
                _ => {
                    any_altitude_gap = true;
                    continue;
                }
            },
            (Some(_), None) => {
                // Mask has unbounded vertical coverage; covers bounded envelope band.
            }
            (None, Some(_)) => {
                // Envelope is unbounded across all altitudes; a bounded sensor cannot claim complete coverage.
                any_altitude_gap = true;
                continue;
            }
            (None, None) => {
                // Both unbounded vertically.
            }
        }

        covering_sensors += 1;
    }

    if covering_sensors > 0 {
        Ok(AreaCoverageStatus::FullyCoveredByActiveSensors {
            verified_covering_sensor_count: covering_sensors,
        })
    } else if any_sensor_stale || any_sensor_offline {
        Ok(AreaCoverageStatus::PartiallyCoveredGapsIdentified {
            reason: UnobservedReason::SensorFeedDownOrStale,
        })
    } else if any_outside_log {
        Ok(AreaCoverageStatus::PartiallyCoveredGapsIdentified {
            reason: UnobservedReason::QueryIntervalOutsideSensorLogWindow,
        })
    } else if any_altitude_gap {
        Ok(AreaCoverageStatus::PartiallyCoveredGapsIdentified {
            reason: UnobservedReason::IncompleteAltitudeCoverage,
        })
    } else if any_horizontal_partial {
        Ok(AreaCoverageStatus::PartiallyCoveredGapsIdentified {
            reason: UnobservedReason::PartialCoverageBlindSpots,
        })
    } else {
        Ok(AreaCoverageStatus::UncoveredNoSensorData)
    }
}
