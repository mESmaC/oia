//! Shared validated WGS84 geometry, altitude categories, and UTC timeline intervals.
//!
//! Scientific scales, calendar/timezone conversion, clock alignment and model-backed
//! height conversion remain in `#12`. Geometry calculations use documented spherical
//! or planar approximations, not ellipsoidal geodesy or measured physical accuracy.

mod error;
mod geometry;
mod time;

pub use error::TimeGeoError;
pub use geometry::{
    Altitude, AltitudeBand, AltitudeReference, EARTH_RADIUS_METERS, GeoBoundingBoxWgs84,
    GeoPointWgs84, GeoPosition, nearest_longitude_on_box,
};
pub use time::TimeWindow;
