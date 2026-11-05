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
    InvalidCalendarDate {
        year: i32,
        month: u8,
        day: u8,
    },
    InvalidRfc3339Timestamp {
        value: String,
    },
    InvalidUtcOffsetMinutes(i16),
    InvalidApproximateBounds {
        earliest_utc_ms: i64,
        latest_utc_ms: i64,
    },
    EmptyField(&'static str),
    PreLeapSecondEpoch(i64),
    LeapTableExpired {
        utc_ms: i64,
        expires_at_utc_ms: i64,
    },
    UnsynchronizedClock,
    ClockAlignmentOutOfWindow {
        aligned_ms: i64,
    },
    TimestampArithmeticOverflow,
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
            Self::InvalidCalendarDate { year, month, day } => {
                write!(f, "invalid calendar date {year:04}-{month:02}-{day:02}")
            }
            Self::InvalidRfc3339Timestamp { value } => {
                write!(f, "invalid RFC 3339 timestamp {value:?}")
            }
            Self::InvalidUtcOffsetMinutes(minutes) => {
                write!(f, "UTC offset {minutes} minutes is outside [-840, 840]")
            }
            Self::InvalidApproximateBounds {
                earliest_utc_ms,
                latest_utc_ms,
            } => write!(
                f,
                "invalid approximate time bounds: earliest ({earliest_utc_ms}) exceeds latest ({latest_utc_ms})"
            ),
            Self::EmptyField(field) => write!(f, "{field} must not be empty"),
            Self::PreLeapSecondEpoch(utc_ms) => {
                write!(
                    f,
                    "timestamp {utc_ms} ms precedes 1972-01-01 leap-second coverage"
                )
            }
            Self::LeapTableExpired {
                utc_ms,
                expires_at_utc_ms,
            } => write!(
                f,
                "timestamp {utc_ms} ms is at or past leap-second table expiry {expires_at_utc_ms} ms"
            ),
            Self::UnsynchronizedClock => write!(
                f,
                "cannot align device timestamp: clock is unsynchronized and receipt time must not substitute for capture time"
            ),
            Self::ClockAlignmentOutOfWindow { aligned_ms } => write!(
                f,
                "aligned timestamp {aligned_ms} ms is outside clock validity window"
            ),
            Self::TimestampArithmeticOverflow => {
                write!(f, "timestamp arithmetic overflowed i64 millisecond bounds")
            }
        }
    }
}

impl std::error::Error for TimeGeoError {}
