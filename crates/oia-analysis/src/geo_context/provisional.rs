//! Compatibility imports for the shared `oia-time-geo` primitives.
//!
//! New callers should import these types from `oia_time_geo` directly. Constructors
//! now return `TimeGeoError`; analysis operations map it to `GeoContextError`.

pub use oia_time_geo::{
    Altitude, AltitudeBand, AltitudeReference, GeoBoundingBoxWgs84, GeoPointWgs84, GeoPosition,
    TimeWindow, nearest_longitude_on_box,
};
