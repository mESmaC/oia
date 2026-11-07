//! Shared validated WGS84 geometry, altitude categories, UTC timeline intervals,
//! civil calendar / RFC 3339 parsing, original-context time representations,
//! IERS leap-second scales (`UTC`/`TAI`/`TT`), and source-clock alignment (`#12`).
//!
//! Model-backed height conversion, physical units, and `FrameSelection` runtime
//! correspondence remain in `#12`. Geometry calculations use documented spherical
//! or planar approximations, not ellipsoidal geodesy or measured physical accuracy.

mod error;
mod geometry;
mod time;

pub use error::TimeGeoError;
pub use geometry::{
    Altitude, AltitudeBand, AltitudeReference, EARTH_RADIUS_METERS, GeoBoundingBoxWgs84,
    GeoPointWgs84, GeoPosition, nearest_longitude_on_box,
};
pub use time::{
    AlignedCaptureTime, CalendarDate, ClockAlignment, ClockSyncState, DstAmbiguity,
    LeapSecondTable, LocalTimeContext, TT_MINUS_TAI_MS, TimeRepresentation, TimeScale, TimeWindow,
    civil_from_days, days_from_civil, days_in_month, has_rfc3339_utc_second_shape, is_leap_year,
    parse_rfc3339_utc_nanos, parse_rfc3339_utc_seconds,
};
