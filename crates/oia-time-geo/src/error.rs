use crate::AltitudeReference;
use std::fmt;

/// Invalid input to a shared geometry or timeline primitive.
#[derive(Debug, Clone, PartialEq)]
pub enum TimeGeoError {
    InvalidCoordinates {
        lat: f64,
        lon: f64,
        reason: &'static str,
    },
    InvalidTimeWindow {
        start_utc_ms: i64,
        end_utc_ms: i64,
    },
    InvalidAltitudeBand {
        min_meters: f64,
        max_meters: f64,
    },
    NonFiniteValue(&'static str),
    IncompatibleAltitudeReferences {
        actual: AltitudeReference,
        expected: AltitudeReference,
    },
}

impl fmt::Display for TimeGeoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCoordinates { lat, lon, reason } => {
                write!(f, "invalid coordinates ({lat}, {lon}): {reason}")
            }
            Self::InvalidTimeWindow {
                start_utc_ms,
                end_utc_ms,
            } => write!(
                f,
                "invalid time window: start ({start_utc_ms}) must precede end ({end_utc_ms})"
            ),
            Self::InvalidAltitudeBand {
                min_meters,
                max_meters,
            } => write!(
                f,
                "invalid altitude band: min ({min_meters} m) must be <= max ({max_meters} m)"
            ),
            Self::NonFiniteValue(field) => write!(f, "non-finite floating point value in {field}"),
            Self::IncompatibleAltitudeReferences { actual, expected } => write!(
                f,
                "altitude reference {actual:?} cannot be compared in {expected:?} without compatible reference models and origins"
            ),
        }
    }
}

impl std::error::Error for TimeGeoError {}
