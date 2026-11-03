//! Geographic context reconstruction kernel (`GEO-02`, `GEO-05`, `INTEL-03`, #24).
//!
//! Reconstructs atmospheric conditions, aircraft tracks, maritime vessel tracks,
//! and navigation/airspace notices across a spatio-temporal query envelope.
//!
//! # Architecture and Primitives Migration Note
//! The geometric and time primitives in this module (`GeoPointWgs84`, `GeoBoundingBoxWgs84`,
//! `Altitude`, `AltitudeReference`, `AltitudeBand`, `TimeWindow`) are **provisional domain models**
//! grouped in [`provisional`] and scoped to `oia-analysis` pending completion of `#12`
//! (`crates/oia-time-geo`). Upon delivery of `#12`, these types will migrate directly to the
//! shared `oia-time-geo` contracts without changing the analytical semantics or evidence
//! preservation rules of this kernel.
//!
//! # Core Evidence & Quality Contracts
//! 1. **Observations vs Forecasts:** Weather observations (e.g. METAR, sounding) and
//!    forecasts (e.g. TAF, NWP model grid) remain strictly distinct with their own
//!    validity windows, observation/issuance timestamps, and retrieval timestamps (`GEO-02`, `GEO-05`).
//!    Missing weather station data never defaults to clear skies, calm winds, or standard atmosphere.
//! 2. **Observations vs Interpolation:** Aircraft and vessel track states distinguish
//!    direct sensor pings (`ObservationSource::DirectObservation`) from bounded
//!    interpolated states (`ObservationSource::InterpolatedSegment`). Interpolations
//!    use exact spherical linear interpolation (SLERP) on unit-sphere great circles,
//!    retaining elapsed gap, distance gap, method ID, and uncertainty radius, and are
//!    rejected when the gap exceeds caller-configured limits (`GEO-05`). Interpolated
//!    states never count toward detected traffic in presence/absence assessments.
//! 3. **Absent Tracks are NOT an Empty Scene:** Zero observed tracks across an envelope
//!    yield `SceneTrafficAssessment::InconclusiveUnobserved` unless the entire 4D volume
//!    (space, altitude, time window) was verified covered by operational, non-stale sensor masks (`GEO-05`, `A04`).
//!    Direct observations with missing/unresolved altitude within an envelope prevent declaring absence.
//! 4. **Dateline Support:** Bounding boxes, notices, and queries explicitly support longitude
//!    wrapping across the 180° antimeridian (`west_lon_deg > east_lon_deg`) (`A04`).
//! 5. **Altitude References:** Altitudes preserve their explicit reference frame
//!    (MSL, AGL, WGS84 ellipsoid, BarometricUnadjusted) (`docs/architecture.md` §3.2).
//!    Comparisons between differing vertical reference frames are strictly rejected
//!    without explicit geoid/terrain models, and incompatible datums do not abort reconstructions.

use oia_domain::{OpaqueId, VersionId};
use std::fmt;

/// Re-export the shared crate StopReason from multilingual module to maintain single definition in oia-analysis.
pub use crate::multilingual::StopReason;

/// Method identifier pinned alongside reconstructed context results.
pub const METHOD_ID: &str = "oia-geo-context-reconstruction-v1";

/// Interpolation method identifier recorded on interpolated segments.
pub const INTERPOLATION_METHOD_ID: &str = "bounded-great-circle-spherical-v1";

/// Standard mean Earth radius for spherical geodetic calculations (metres).
pub const EARTH_RADIUS_METERS: f64 = 6_371_000.0;

/// Operational resource and validation limits for context reconstruction.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextLimits {
    pub max_points_per_track: usize,
    pub max_tracks: usize,
    pub max_notices: usize,
    pub max_weather_records: usize,
    pub max_interpolation_gap_ms: u64,
    /// Base observation uncertainty in metres (default 15.0m per RTCA DO-260B NACp >= 9 GPS bounds).
    pub base_observation_uncertainty_m: f64,
    /// Kinetic divergence growth rate in m/s (default 2.0 m/s transport aircraft dead-reckoning wind/drift bound).
    pub interpolation_growth_rate_mps: f64,
    /// Maximum sensor feed staleness tolerance in milliseconds (default 300,000 ms = 5 minutes).
    pub max_sensor_staleness_ms: u64,
}

impl Default for ContextLimits {
    fn default() -> Self {
        Self {
            max_points_per_track: 10_000,
            max_tracks: 1_000,
            max_notices: 1_000,
            max_weather_records: 1_000,
            max_interpolation_gap_ms: 300_000, // 5 minutes
            base_observation_uncertainty_m: 15.0,
            interpolation_growth_rate_mps: 2.0,
            max_sensor_staleness_ms: 300_000, // 5 minutes
        }
    }
}

impl ContextLimits {
    /// Validates numeric sanity of operational limits.
    pub fn validate(&self) -> Result<(), GeoContextError> {
        if !self.base_observation_uncertainty_m.is_finite()
            || self.base_observation_uncertainty_m < 0.0
        {
            return Err(GeoContextError::NonFiniteValue(
                "ContextLimits.base_observation_uncertainty_m",
            ));
        }
        if !self.interpolation_growth_rate_mps.is_finite()
            || self.interpolation_growth_rate_mps < 0.0
        {
            return Err(GeoContextError::NonFiniteValue(
                "ContextLimits.interpolation_growth_rate_mps",
            ));
        }
        if self.max_interpolation_gap_ms == 0 {
            return Err(GeoContextError::NonFiniteValue(
                "ContextLimits.max_interpolation_gap_ms must be > 0",
            ));
        }
        if self.max_sensor_staleness_ms == 0 {
            return Err(GeoContextError::NonFiniteValue(
                "ContextLimits.max_sensor_staleness_ms must be > 0",
            ));
        }
        Ok(())
    }
}

/// Actionable errors returned by the geographic context reconstruction kernel.
#[derive(Debug, Clone, PartialEq)]
pub enum GeoContextError {
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
    LimitExceeded {
        limit: &'static str,
        actual: usize,
        max: usize,
    },
    InterpolationGapExceeded {
        gap_ms: u64,
        max_gap_ms: u64,
    },
    OutOfTrackRange {
        target_utc_ms: i64,
        min_utc_ms: i64,
        max_utc_ms: i64,
    },
    EmptyTrack,
    DuplicateTimestamp(i64),
    InvalidTemporalOrdering(&'static str),
    InvalidIcaoHex(String),
    InvalidMmsi(u32),
    InvalidImoNumber(u32),
    InvalidSquawk(String),
    InvalidWeatherTimes(&'static str),
    IncompatibleAltitudeReferences {
        actual: AltitudeReference,
        expected: AltitudeReference,
    },
    Stopped(StopReason),
}

impl fmt::Display for GeoContextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCoordinates { lat, lon, reason } => {
                write!(f, "invalid coordinates ({lat}, {lon}): {reason}")
            }
            Self::InvalidTimeWindow {
                start_utc_ms,
                end_utc_ms,
            } => {
                write!(
                    f,
                    "invalid time window: start ({start_utc_ms}) must precede end ({end_utc_ms})"
                )
            }
            Self::InvalidAltitudeBand {
                min_meters,
                max_meters,
            } => {
                write!(
                    f,
                    "invalid altitude band: min ({min_meters} m) must be <= max ({max_meters} m)"
                )
            }
            Self::NonFiniteValue(field) => write!(f, "non-finite floating point value in {field}"),
            Self::LimitExceeded { limit, actual, max } => {
                write!(f, "{limit} exceeded: actual {actual}, max {max}")
            }
            Self::InterpolationGapExceeded { gap_ms, max_gap_ms } => {
                write!(
                    f,
                    "track interpolation gap of {gap_ms} ms exceeds maximum allowed {max_gap_ms} ms"
                )
            }
            Self::OutOfTrackRange {
                target_utc_ms,
                min_utc_ms,
                max_utc_ms,
            } => {
                write!(
                    f,
                    "target time {target_utc_ms} is outside track range [{min_utc_ms}, {max_utc_ms}]"
                )
            }
            Self::EmptyTrack => f.write_str("track has no data points"),
            Self::DuplicateTimestamp(ts) => {
                write!(f, "conflicting duplicate track point timestamp: {ts} ms")
            }
            Self::InvalidTemporalOrdering(reason) => {
                write!(f, "invalid temporal ordering: {reason}")
            }
            Self::InvalidIcaoHex(s) => write!(f, "invalid ICAO 24-bit hex address: {s}"),
            Self::InvalidMmsi(mmsi) => write!(f, "invalid maritime MMSI: {mmsi}"),
            Self::InvalidImoNumber(imo) => {
                write!(f, "invalid IMO ship number check digit checksum: {imo}")
            }
            Self::InvalidSquawk(sq) => write!(
                f,
                "invalid transponder squawk (must be 4 octal digits [0-7]): {sq}"
            ),
            Self::InvalidWeatherTimes(reason) => {
                write!(f, "invalid weather report timestamps: {reason}")
            }
            Self::IncompatibleAltitudeReferences { actual, expected } => {
                write!(
                    f,
                    "incompatible altitude datums: actual {actual:?} vs expected {expected:?}"
                )
            }
            Self::Stopped(StopReason::Cancelled) => f.write_str("context reconstruction cancelled"),
            Self::Stopped(StopReason::DeadlineExceeded) => {
                f.write_str("context reconstruction deadline exceeded")
            }
            Self::Stopped(StopReason::Superseded) => {
                f.write_str("context reconstruction superseded by newer request")
            }
        }
    }
}

impl std::error::Error for GeoContextError {}

// -----------------------------------------------------------------------------
// Provisional Geometry & Datum Primitives (Pending #12 `oia-time-geo`)
// -----------------------------------------------------------------------------

/// Provisional geometry and temporal primitives scoped to `oia-analysis` pending completion of `#12`.
pub mod provisional {
    use super::*;

    /// 2D geographic point on the WGS84 ellipsoid.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct GeoPointWgs84 {
        pub(crate) lat_deg: f64,
        pub(crate) lon_deg: f64,
    }

    impl GeoPointWgs84 {
        pub fn new(lat_deg: f64, lon_deg: f64) -> Result<Self, GeoContextError> {
            if !lat_deg.is_finite() || !(-90.0..=90.0).contains(&lat_deg) {
                return Err(GeoContextError::InvalidCoordinates {
                    lat: lat_deg,
                    lon: lon_deg,
                    reason: "latitude must be finite and within [-90.0, 90.0] degrees",
                });
            }
            if !lon_deg.is_finite() || !(-180.0..=180.0).contains(&lon_deg) {
                return Err(GeoContextError::InvalidCoordinates {
                    lat: lat_deg,
                    lon: lon_deg,
                    reason: "longitude must be finite and within [-180.0, 180.0] degrees",
                });
            }
            Ok(Self { lat_deg, lon_deg })
        }

        #[inline]
        #[must_use]
        pub fn latitude_deg(&self) -> f64 {
            self.lat_deg
        }

        #[inline]
        #[must_use]
        pub fn longitude_deg(&self) -> f64 {
            self.lon_deg
        }

        /// Great-circle spherical distance in metres using the Haversine formula.
        #[must_use]
        pub fn great_circle_distance_m(&self, other: GeoPointWgs84) -> f64 {
            let lat1 = self.lat_deg.to_radians();
            let lat2 = other.lat_deg.to_radians();
            let d_lat = (other.lat_deg - self.lat_deg).to_radians();

            let mut d_lon_deg = (other.lon_deg - self.lon_deg) % 360.0;
            if d_lon_deg > 180.0 {
                d_lon_deg -= 360.0;
            } else if d_lon_deg < -180.0 {
                d_lon_deg += 360.0;
            }
            let d_lon = d_lon_deg.to_radians();

            let a =
                (d_lat * 0.5).sin().powi(2) + lat1.cos() * lat2.cos() * (d_lon * 0.5).sin().powi(2);
            let c = 2.0 * a.clamp(0.0, 1.0).sqrt().asin();

            EARTH_RADIUS_METERS * c
        }

        /// Great-circle initial bearing from `self` to `other` in degrees `[0.0, 360.0)`.
        #[must_use]
        pub fn initial_bearing_deg(&self, other: GeoPointWgs84) -> f64 {
            let lat1 = self.lat_deg.to_radians();
            let lat2 = other.lat_deg.to_radians();

            let mut d_lon_deg = (other.lon_deg - self.lon_deg) % 360.0;
            if d_lon_deg > 180.0 {
                d_lon_deg -= 360.0;
            } else if d_lon_deg < -180.0 {
                d_lon_deg += 360.0;
            }
            let d_lon = d_lon_deg.to_radians();

            let y = d_lon.sin() * lat2.cos();
            let x = lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * d_lon.cos();
            let bearing_rad = y.atan2(x);
            (bearing_rad.to_degrees() + 360.0) % 360.0
        }
    }

    /// Explicit vertical datum reference aligned with architecture §3.2 and PR #95 (`crates/oia-devices`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum AltitudeReference {
        /// Height above the WGS84 reference ellipsoid.
        Wgs84Ellipsoid,
        /// Orthometric height above mean sea level (geoid-referenced MSL).
        OrthometricMsl,
        /// Measured height above local ground level (AGL).
        AboveGroundLevel,
        /// Uncorrected barometric pressure altitude (ISA 1013.25 hPa reference).
        BarometricUnadjusted,
        /// Unverified or unspecified altitude reference datum.
        Unknown,
    }

    /// Validated altitude measurement with an explicit vertical datum.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct Altitude {
        meters: f64,
        reference: AltitudeReference,
    }

    impl Altitude {
        pub fn new(meters: f64, reference: AltitudeReference) -> Result<Self, GeoContextError> {
            if !meters.is_finite() {
                return Err(GeoContextError::NonFiniteValue("altitude.meters"));
            }
            Ok(Self { meters, reference })
        }

        #[inline]
        #[must_use]
        pub fn meters(&self) -> f64 {
            self.meters
        }

        #[inline]
        #[must_use]
        pub fn reference(&self) -> AltitudeReference {
            self.reference
        }

        /// Verifies that both altitudes share the exact same reference datum.
        /// Comparisons across different vertical datums fail closed.
        pub fn compare_in_same_reference(
            &self,
            other: &Altitude,
        ) -> Result<std::cmp::Ordering, GeoContextError> {
            if self.reference == AltitudeReference::Unknown
                || other.reference == AltitudeReference::Unknown
                || self.reference != other.reference
            {
                return Err(GeoContextError::IncompatibleAltitudeReferences {
                    actual: self.reference,
                    expected: other.reference,
                });
            }
            Ok(self
                .meters
                .partial_cmp(&other.meters)
                .unwrap_or(std::cmp::Ordering::Equal))
        }
    }

    /// 3D geographic position combining a horizontal WGS84 point with an optional vertical datum measurement.
    #[derive(Debug, Clone, PartialEq)]
    pub struct GeoPosition {
        point: GeoPointWgs84,
        altitude: Option<Altitude>,
    }

    impl GeoPosition {
        pub fn new(point: GeoPointWgs84, altitude: Option<Altitude>) -> Self {
            Self { point, altitude }
        }

        #[inline]
        #[must_use]
        pub fn point(&self) -> GeoPointWgs84 {
            self.point
        }

        #[inline]
        #[must_use]
        pub fn altitude(&self) -> Option<Altitude> {
            self.altitude
        }
    }

    /// Geodetic bounding box with explicit support for 180° antimeridian crossing (`A04`).
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct GeoBoundingBoxWgs84 {
        south_lat_deg: f64,
        north_lat_deg: f64,
        west_lon_deg: f64,
        east_lon_deg: f64,
    }

    impl GeoBoundingBoxWgs84 {
        pub fn new(
            south_lat_deg: f64,
            north_lat_deg: f64,
            west_lon_deg: f64,
            east_lon_deg: f64,
        ) -> Result<Self, GeoContextError> {
            if !south_lat_deg.is_finite()
                || !north_lat_deg.is_finite()
                || !west_lon_deg.is_finite()
                || !east_lon_deg.is_finite()
            {
                return Err(GeoContextError::NonFiniteValue("GeoBoundingBoxWgs84"));
            }
            if south_lat_deg > north_lat_deg
                || !(-90.0..=90.0).contains(&south_lat_deg)
                || !(-90.0..=90.0).contains(&north_lat_deg)
            {
                return Err(GeoContextError::InvalidCoordinates {
                    lat: south_lat_deg,
                    lon: west_lon_deg,
                    reason: "invalid latitude bounds (south must be <= north and within [-90.0, 90.0])",
                });
            }
            if !(-180.0..=180.0).contains(&west_lon_deg)
                || !(-180.0..=180.0).contains(&east_lon_deg)
            {
                return Err(GeoContextError::InvalidCoordinates {
                    lat: south_lat_deg,
                    lon: west_lon_deg,
                    reason: "longitude bounds must be in [-180.0, 180.0]",
                });
            }
            Ok(Self {
                south_lat_deg,
                north_lat_deg,
                west_lon_deg,
                east_lon_deg,
            })
        }

        #[inline]
        #[must_use]
        pub fn south_lat_deg(&self) -> f64 {
            self.south_lat_deg
        }

        #[inline]
        #[must_use]
        pub fn north_lat_deg(&self) -> f64 {
            self.north_lat_deg
        }

        #[inline]
        #[must_use]
        pub fn west_lon_deg(&self) -> f64 {
            self.west_lon_deg
        }

        #[inline]
        #[must_use]
        pub fn east_lon_deg(&self) -> f64 {
            self.east_lon_deg
        }

        /// Checks if a point lies within the bounding box, respecting antimeridian wrap.
        #[must_use]
        pub fn contains_point(&self, point: GeoPointWgs84) -> bool {
            if point.lat_deg < self.south_lat_deg || point.lat_deg > self.north_lat_deg {
                return false;
            }
            if self.west_lon_deg <= self.east_lon_deg {
                point.lon_deg >= self.west_lon_deg && point.lon_deg <= self.east_lon_deg
            } else {
                point.lon_deg >= self.west_lon_deg || point.lon_deg <= self.east_lon_deg
            }
        }

        /// True if this bounding box crosses the 180° antimeridian.
        #[must_use]
        pub fn crosses_antimeridian(&self) -> bool {
            self.west_lon_deg > self.east_lon_deg
        }

        /// True if `self` completely spatially contains the entire extent of `other`.
        #[must_use]
        pub fn contains_box(&self, other: &GeoBoundingBoxWgs84) -> bool {
            if other.south_lat_deg < self.south_lat_deg || other.north_lat_deg > self.north_lat_deg
            {
                return false;
            }
            match (self.crosses_antimeridian(), other.crosses_antimeridian()) {
                (false, false) => {
                    self.west_lon_deg <= other.west_lon_deg
                        && other.east_lon_deg <= self.east_lon_deg
                }
                (true, false) => {
                    (self.west_lon_deg <= other.west_lon_deg && other.east_lon_deg <= 180.0)
                        || (-180.0 <= other.west_lon_deg && other.east_lon_deg <= self.east_lon_deg)
                }
                (true, true) => {
                    self.west_lon_deg <= other.west_lon_deg
                        && other.east_lon_deg <= self.east_lon_deg
                }
                (false, true) => false,
            }
        }

        /// True if `self` overlaps with `other` in latitude and longitude.
        #[must_use]
        pub fn overlaps_box(&self, other: &GeoBoundingBoxWgs84) -> bool {
            if self.north_lat_deg < other.south_lat_deg || self.south_lat_deg > other.north_lat_deg
            {
                return false;
            }
            let lon_overlaps = |w1: f64, e1: f64, w2: f64, e2: f64| w1 <= e2 && w2 <= e1;
            match (self.crosses_antimeridian(), other.crosses_antimeridian()) {
                (false, false) => lon_overlaps(
                    self.west_lon_deg,
                    self.east_lon_deg,
                    other.west_lon_deg,
                    other.east_lon_deg,
                ),
                (true, false) => {
                    lon_overlaps(
                        self.west_lon_deg,
                        180.0,
                        other.west_lon_deg,
                        other.east_lon_deg,
                    ) || lon_overlaps(
                        -180.0,
                        self.east_lon_deg,
                        other.west_lon_deg,
                        other.east_lon_deg,
                    )
                }
                (false, true) => {
                    lon_overlaps(
                        self.west_lon_deg,
                        self.east_lon_deg,
                        other.west_lon_deg,
                        180.0,
                    ) || lon_overlaps(
                        self.west_lon_deg,
                        self.east_lon_deg,
                        -180.0,
                        other.east_lon_deg,
                    )
                }
                (true, true) => true,
            }
        }

        /// Approximate center point of the bounding box.
        #[must_use]
        pub fn center_point(&self) -> GeoPointWgs84 {
            let lat = (self.south_lat_deg + self.north_lat_deg) * 0.5;
            let lon = if self.west_lon_deg <= self.east_lon_deg {
                (self.west_lon_deg + self.east_lon_deg) * 0.5
            } else {
                let span = (180.0 - self.west_lon_deg) + (self.east_lon_deg - (-180.0));
                let mut mid = self.west_lon_deg + span * 0.5;
                if mid > 180.0 {
                    mid -= 360.0;
                }
                mid
            };
            GeoPointWgs84 {
                lat_deg: lat,
                lon_deg: lon,
            }
        }

        /// Nearest point on the bounding box to a target point.
        ///
        /// NOTE: Clamping latitude and selecting the closest boundary longitude provides a fast,
        /// coordinate-aligned approximation of the nearest point on the sphere for conservative
        /// spatial overlap checks.
        #[must_use]
        pub fn nearest_point_to(&self, pt: GeoPointWgs84) -> GeoPointWgs84 {
            let clamped_lat = pt.lat_deg.clamp(self.south_lat_deg, self.north_lat_deg);
            let nearest_lon = nearest_longitude_on_box(
                pt.lon_deg,
                self.west_lon_deg,
                self.east_lon_deg,
                self.crosses_antimeridian(),
            );
            GeoPointWgs84 {
                lat_deg: clamped_lat,
                lon_deg: nearest_lon,
            }
        }
    }

    /// Finds the longitude in `[west_lon, east_lon]` that minimizes angular distance to `lon_deg`.
    #[must_use]
    pub fn nearest_longitude_on_box(
        lon_deg: f64,
        west_lon_deg: f64,
        east_lon_deg: f64,
        crosses_antimeridian: bool,
    ) -> f64 {
        let angular_diff = |l1: f64, l2: f64| -> f64 {
            let mut d = (l1 - l2) % 360.0;
            if d > 180.0 {
                d -= 360.0;
            } else if d < -180.0 {
                d += 360.0;
            }
            d.abs()
        };

        if crosses_antimeridian {
            if lon_deg >= west_lon_deg || lon_deg <= east_lon_deg {
                return lon_deg;
            }
            let d_west = angular_diff(lon_deg, west_lon_deg);
            let d_east = angular_diff(lon_deg, east_lon_deg);
            if d_west <= d_east {
                west_lon_deg
            } else {
                east_lon_deg
            }
        } else {
            if lon_deg >= west_lon_deg && lon_deg <= east_lon_deg {
                return lon_deg;
            }
            let d_west = angular_diff(lon_deg, west_lon_deg);
            let d_east = angular_diff(lon_deg, east_lon_deg);
            if d_west <= d_east {
                west_lon_deg
            } else {
                east_lon_deg
            }
        }
    }

    /// Half-open time interval `[start_utc_ms, end_utc_ms)`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct TimeWindow {
        start_utc_ms: i64,
        end_utc_ms: i64,
    }

    impl TimeWindow {
        pub fn new(start_utc_ms: i64, end_utc_ms: i64) -> Result<Self, GeoContextError> {
            if start_utc_ms >= end_utc_ms {
                return Err(GeoContextError::InvalidTimeWindow {
                    start_utc_ms,
                    end_utc_ms,
                });
            }
            Ok(Self {
                start_utc_ms,
                end_utc_ms,
            })
        }

        #[inline]
        #[must_use]
        pub fn start_utc_ms(&self) -> i64 {
            self.start_utc_ms
        }

        #[inline]
        #[must_use]
        pub fn end_utc_ms(&self) -> i64 {
            self.end_utc_ms
        }

        #[inline]
        #[must_use]
        pub fn contains(&self, instant_utc_ms: i64) -> bool {
            instant_utc_ms >= self.start_utc_ms && instant_utc_ms < self.end_utc_ms
        }

        #[inline]
        #[must_use]
        pub fn overlaps(&self, other: &TimeWindow) -> bool {
            self.start_utc_ms < other.end_utc_ms && other.start_utc_ms < self.end_utc_ms
        }

        #[inline]
        #[must_use]
        pub fn contains_window(&self, other: &TimeWindow) -> bool {
            self.start_utc_ms <= other.start_utc_ms && self.end_utc_ms >= other.end_utc_ms
        }
    }

    /// Bounded vertical band with an explicit vertical datum.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct AltitudeBand {
        min_altitude: Altitude,
        max_altitude: Altitude,
    }

    impl AltitudeBand {
        pub fn new(
            min_altitude: Altitude,
            max_altitude: Altitude,
        ) -> Result<Self, GeoContextError> {
            min_altitude.compare_in_same_reference(&max_altitude)?;
            if min_altitude.meters > max_altitude.meters {
                return Err(GeoContextError::InvalidAltitudeBand {
                    min_meters: min_altitude.meters,
                    max_meters: max_altitude.meters,
                });
            }
            Ok(Self {
                min_altitude,
                max_altitude,
            })
        }

        #[inline]
        #[must_use]
        pub fn min_altitude(&self) -> Altitude {
            self.min_altitude
        }

        #[inline]
        #[must_use]
        pub fn max_altitude(&self) -> Altitude {
            self.max_altitude
        }

        #[inline]
        #[must_use]
        pub fn reference(&self) -> AltitudeReference {
            self.min_altitude.reference()
        }

        /// Verifies if an altitude measurement falls within this band.
        pub fn contains_altitude(&self, altitude: &Altitude) -> Result<bool, GeoContextError> {
            let cmp_min = altitude.compare_in_same_reference(&self.min_altitude)?;
            let cmp_max = altitude.compare_in_same_reference(&self.max_altitude)?;
            Ok(!cmp_min.is_lt() && !cmp_max.is_gt())
        }

        /// Verifies if this altitude band completely covers `other`.
        pub fn contains_band(&self, other: &AltitudeBand) -> Result<bool, GeoContextError> {
            let cmp_min = self
                .min_altitude
                .compare_in_same_reference(&other.min_altitude)?;
            let cmp_max = self
                .max_altitude
                .compare_in_same_reference(&other.max_altitude)?;
            Ok(!cmp_min.is_gt() && !cmp_max.is_lt())
        }

        /// Checks if this altitude band overlaps with `other`.
        pub fn overlaps_band(&self, other: &AltitudeBand) -> Result<bool, GeoContextError> {
            self.min_altitude
                .compare_in_same_reference(&other.min_altitude)?;
            Ok(self.min_altitude.meters <= other.max_altitude.meters
                && other.min_altitude.meters <= self.max_altitude.meters)
        }
    }
}

// Re-export provisional geometry & temporal primitives for module users
pub use provisional::*;

/// Spherical linear interpolation (SLERP) along the great circle between two points on the WGS84 unit sphere.
///
/// Converts geodetic latitude/longitude to 3D Cartesian coordinates on the unit sphere,
/// computes the shortest-arc spherical angle $\theta$, and interpolates via:
/// $$\mathbf{v}(f) = \frac{\sin((1-f)\theta)}{\sin\theta} \mathbf{v}_1 + \frac{\sin(f\theta)}{\sin\theta} \mathbf{v}_2$$
///
/// This eliminates planar projection distortion across all latitudes and across the 180° antimeridian.
#[must_use]
pub fn spherical_slerp(p1: GeoPointWgs84, p2: GeoPointWgs84, f: f64) -> GeoPointWgs84 {
    let lat1_rad = p1.latitude_deg().to_radians();
    let lon1_rad = p1.longitude_deg().to_radians();
    let lat2_rad = p2.latitude_deg().to_radians();
    let lon2_rad = p2.longitude_deg().to_radians();

    let v1 = [
        lat1_rad.cos() * lon1_rad.cos(),
        lat1_rad.cos() * lon1_rad.sin(),
        lat1_rad.sin(),
    ];
    let v2 = [
        lat2_rad.cos() * lon2_rad.cos(),
        lat2_rad.cos() * lon2_rad.sin(),
        lat2_rad.sin(),
    ];

    let dot = (v1[0] * v2[0] + v1[1] * v2[1] + v1[2] * v2[2]).clamp(-1.0, 1.0);
    let theta = dot.acos();

    if theta < 1e-7 {
        return p1;
    }

    let sin_theta = theta.sin();
    let w1 = ((1.0 - f) * theta).sin() / sin_theta;
    let w2 = (f * theta).sin() / sin_theta;

    let x = w1 * v1[0] + w2 * v2[0];
    let y = w1 * v1[1] + w2 * v2[1];
    let z = w1 * v1[2] + w2 * v2[2];

    let norm = (x * x + y * y + z * z).sqrt();
    let (nx, ny, nz) = if norm > 1e-9 {
        (x / norm, y / norm, z / norm)
    } else {
        (v1[0], v1[1], v1[2])
    };

    let lat_deg = nz.clamp(-1.0, 1.0).asin().to_degrees();
    let lon_deg = ny.atan2(nx).to_degrees();

    GeoPointWgs84 { lat_deg, lon_deg }
}

/// 4D spatio-temporal query envelope (`GEO-02`, `GEO-05`, `A04`).
#[derive(Debug, Clone, PartialEq)]
pub struct SpatioTemporalEnvelope {
    pub spatial_bounds: GeoBoundingBoxWgs84,
    pub altitude_band: Option<AltitudeBand>,
    pub time_window: TimeWindow,
}

impl SpatioTemporalEnvelope {
    pub fn new(
        spatial_bounds: GeoBoundingBoxWgs84,
        altitude_band: Option<AltitudeBand>,
        time_window: TimeWindow,
    ) -> Self {
        Self {
            spatial_bounds,
            altitude_band,
            time_window,
        }
    }

    /// Tests if a 3D position at a specific timestamp falls inside the envelope.
    ///
    /// - If position altitude has an incompatible datum with the envelope's band,
    ///   returns `Err(IncompatibleAltitudeReferences)` to let caller handle unresolved altitude.
    /// - If position has no altitude measurement when envelope specifies a band, returns `Ok(false)`
    ///   (unknown altitude cannot assert presence inside a vertical band).
    pub fn contains_position(
        &self,
        position: &GeoPosition,
        instant_utc_ms: i64,
    ) -> Result<bool, GeoContextError> {
        if !self.time_window.contains(instant_utc_ms) {
            return Ok(false);
        }
        if !self.spatial_bounds.contains_point(position.point()) {
            return Ok(false);
        }
        if let Some(ref band) = self.altitude_band {
            match position.altitude() {
                Some(ref alt) => band.contains_altitude(alt),
                None => Ok(false),
            }
        } else {
            Ok(true)
        }
    }
}

// -----------------------------------------------------------------------------
// Atmospheric & Meteorological Reconstructions (`GEO-02`, `GEO-05`)
// -----------------------------------------------------------------------------

/// Classification of a meteorological record as a direct sensor measurement vs. forecast model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeatherReportKind {
    DirectObservation,
    ForecastModel,
}

/// Cloud coverage classification (WMO / ICAO code).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudCoverage {
    Clear,
    Few,
    Scattered,
    Broken,
    Overcast,
}

/// Single atmospheric cloud layer report.
#[derive(Debug, Clone, PartialEq)]
pub struct CloudLayer {
    pub coverage: CloudCoverage,
    pub base_altitude: Altitude,
    pub top_altitude: Option<Altitude>,
}

/// Upper-air vertical atmospheric sounding profile level.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundingLevel {
    pub altitude: Altitude,
    pub temperature_c: f64,
    pub pressure_hpa: f64,
    pub wind_speed_mps: f64,
    pub wind_direction_deg: f64,
}

/// Normalized weather observation or forecast model report.
#[derive(Debug, Clone, PartialEq)]
pub struct WeatherRecord {
    pub source_id: OpaqueId,
    pub version_id: VersionId,
    pub station_id: OpaqueId,
    pub station_position: GeoPosition,
    pub kind: WeatherReportKind,
    pub observation_instant_utc_ms: Option<i64>,
    pub forecast_issuance_utc_ms: Option<i64>,
    pub retrieval_instant_utc_ms: i64,
    pub valid_window: TimeWindow,
    pub model_id: Option<OpaqueId>,
    pub temperature_c: Option<f64>,
    pub dew_point_c: Option<f64>,
    pub relative_humidity_pct: Option<f64>,
    pub wind_speed_mps: Option<f64>,
    pub wind_direction_deg: Option<f64>,
    pub wind_gust_mps: Option<f64>,
    pub visibility_m: Option<f64>,
    pub visibility_restriction: Option<String>,
    pub pressure_altimeter_hpa: Option<f64>,
    pub cloud_layers: Vec<CloudLayer>,
    pub sounding_profile: Vec<SoundingLevel>,
    pub raw_text_hash_sha256: [u8; 32],
}

impl WeatherRecord {
    /// Validates structural consistency and temporal integrity.
    pub fn validate(&self) -> Result<(), GeoContextError> {
        match self.kind {
            WeatherReportKind::DirectObservation => {
                let obs =
                    self.observation_instant_utc_ms
                        .ok_or(GeoContextError::InvalidWeatherTimes(
                            "direct observation missing observation_instant_utc_ms",
                        ))?;
                if self.forecast_issuance_utc_ms.is_some() {
                    return Err(GeoContextError::InvalidWeatherTimes(
                        "direct observation must not specify forecast_issuance_utc_ms",
                    ));
                }
                if self.model_id.is_some() {
                    return Err(GeoContextError::InvalidWeatherTimes(
                        "direct observation must not specify model_id",
                    ));
                }
                if obs < self.valid_window.start_utc_ms() || obs > self.valid_window.end_utc_ms() {
                    return Err(GeoContextError::InvalidWeatherTimes(
                        "observation_instant_utc_ms falls outside valid_window",
                    ));
                }
                if self.retrieval_instant_utc_ms < obs {
                    return Err(GeoContextError::InvalidWeatherTimes(
                        "retrieval_instant_utc_ms must be >= observation_instant_utc_ms",
                    ));
                }
            }
            WeatherReportKind::ForecastModel => {
                let iss =
                    self.forecast_issuance_utc_ms
                        .ok_or(GeoContextError::InvalidWeatherTimes(
                            "forecast model missing forecast_issuance_utc_ms",
                        ))?;
                if self.model_id.is_none() {
                    return Err(GeoContextError::InvalidWeatherTimes(
                        "forecast model missing model_id",
                    ));
                }
                if self.observation_instant_utc_ms.is_some() {
                    return Err(GeoContextError::InvalidWeatherTimes(
                        "forecast model must not specify observation_instant_utc_ms",
                    ));
                }
                if iss > self.valid_window.start_utc_ms() {
                    return Err(GeoContextError::InvalidWeatherTimes(
                        "forecast_issuance_utc_ms occurs after valid_window start",
                    ));
                }
                if self.retrieval_instant_utc_ms < iss {
                    return Err(GeoContextError::InvalidWeatherTimes(
                        "retrieval_instant_utc_ms must be >= forecast_issuance_utc_ms",
                    ));
                }
            }
        }

        if let Some(t) = self.temperature_c
            && !t.is_finite()
        {
            return Err(GeoContextError::NonFiniteValue("temperature_c"));
        }
        if let Some(dp) = self.dew_point_c
            && !dp.is_finite()
        {
            return Err(GeoContextError::NonFiniteValue("dew_point_c"));
        }
        if let Some(rh) = self.relative_humidity_pct
            && (!rh.is_finite() || !(0.0..=100.0).contains(&rh))
        {
            return Err(GeoContextError::NonFiniteValue("relative_humidity_pct"));
        }
        if let Some(ws) = self.wind_speed_mps
            && (!ws.is_finite() || ws < 0.0)
        {
            return Err(GeoContextError::NonFiniteValue("wind_speed_mps"));
        }
        if let Some(wd) = self.wind_direction_deg
            && (!wd.is_finite() || !(0.0..=360.0).contains(&wd))
        {
            return Err(GeoContextError::NonFiniteValue("wind_direction_deg"));
        }
        if let Some(wg) = self.wind_gust_mps
            && (!wg.is_finite() || wg < 0.0)
        {
            return Err(GeoContextError::NonFiniteValue("wind_gust_mps"));
        }
        if let Some(vis) = self.visibility_m
            && (!vis.is_finite() || vis < 0.0)
        {
            return Err(GeoContextError::NonFiniteValue("visibility_m"));
        }
        if let Some(p) = self.pressure_altimeter_hpa
            && (!p.is_finite() || p < 0.0)
        {
            return Err(GeoContextError::NonFiniteValue("pressure_altimeter_hpa"));
        }

        // Validate cloud layers
        for layer in &self.cloud_layers {
            if let Some(ref top) = layer.top_altitude {
                layer.base_altitude.compare_in_same_reference(top)?;
                if layer.base_altitude.meters() > top.meters() {
                    return Err(GeoContextError::InvalidAltitudeBand {
                        min_meters: layer.base_altitude.meters(),
                        max_meters: top.meters(),
                    });
                }
            }
        }

        // Validate sounding profile
        for snd in &self.sounding_profile {
            if !snd.temperature_c.is_finite() {
                return Err(GeoContextError::NonFiniteValue("sounding.temperature_c"));
            }
            if !snd.pressure_hpa.is_finite() || snd.pressure_hpa < 0.0 {
                return Err(GeoContextError::NonFiniteValue("sounding.pressure_hpa"));
            }
            if !snd.wind_speed_mps.is_finite() || snd.wind_speed_mps < 0.0 {
                return Err(GeoContextError::NonFiniteValue("sounding.wind_speed_mps"));
            }
            if !snd.wind_direction_deg.is_finite()
                || !(0.0..=360.0).contains(&snd.wind_direction_deg)
            {
                return Err(GeoContextError::NonFiniteValue(
                    "sounding.wind_direction_deg",
                ));
            }
        }

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Aviation & Aircraft Tracking (`GEO-05`)
// -----------------------------------------------------------------------------

/// Sensor or transmission source category for kinematic state points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionSource {
    Adsb,
    Mlat,
    PrimaryRadar,
    SecondaryRadar,
    SatelliteAis,
    TerrestrialAis,
}

/// Provenance tag separating direct sensor detections from derived interpolated states (`GEO-05`).
#[derive(Debug, Clone, PartialEq)]
pub enum ObservationSource {
    DirectObservation,
    InterpolatedSegment {
        method_id: &'static str,
        elapsed_gap_ms: u64,
        distance_gap_m: f64,
        uncertainty_radius_m: f64,
        base_uncertainty_m: f64,
        growth_rate_mps: f64,
    },
}

/// Single instantaneous aircraft kinematic state point.
#[derive(Debug, Clone, PartialEq)]
pub struct AircraftTrackPoint {
    pub transmit_instant_utc_ms: Option<i64>,
    pub receiver_instant_utc_ms: Option<i64>,
    pub retrieval_instant_utc_ms: Option<i64>,
    pub target_interpolation_instant_utc_ms: Option<i64>,
    pub position: GeoPointWgs84,
    pub barometric_altitude: Option<Altitude>,
    pub geometric_altitude: Option<Altitude>,
    pub ground_speed_mps: Option<f64>,
    pub track_angle_deg: Option<f64>,
    pub vertical_rate_mps: Option<f64>,
    pub squawk_octal: Option<String>,
    pub observation_source: ObservationSource,
    pub raw_source_hash: Option<[u8; 32]>,
}

impl AircraftTrackPoint {
    /// Effective point timestamp: interpolation target time if interpolated, else receiver/transmit time.
    #[must_use]
    pub fn effective_instant_utc_ms(&self) -> i64 {
        self.target_interpolation_instant_utc_ms
            .or(self.receiver_instant_utc_ms)
            .or(self.transmit_instant_utc_ms)
            .or(self.retrieval_instant_utc_ms)
            .unwrap_or(0)
    }

    /// Validates field values, datums, squawk codes, and temporal ordering.
    pub fn validate(&self) -> Result<(), GeoContextError> {
        match self.observation_source {
            ObservationSource::DirectObservation => {
                if self.target_interpolation_instant_utc_ms.is_some() {
                    return Err(GeoContextError::InvalidTemporalOrdering(
                        "direct observation must not set target_interpolation_instant_utc_ms",
                    ));
                }
                if self.transmit_instant_utc_ms.is_none()
                    && self.receiver_instant_utc_ms.is_none()
                    && self.retrieval_instant_utc_ms.is_none()
                {
                    return Err(GeoContextError::InvalidTemporalOrdering(
                        "direct observation must have at least one observed timestamp",
                    ));
                }
                if let (Some(tx), Some(rx)) =
                    (self.transmit_instant_utc_ms, self.receiver_instant_utc_ms)
                    && tx > rx
                {
                    return Err(GeoContextError::InvalidTemporalOrdering(
                        "transmit_instant_utc_ms must be <= receiver_instant_utc_ms",
                    ));
                }
                if let (Some(rx), Some(ret)) =
                    (self.receiver_instant_utc_ms, self.retrieval_instant_utc_ms)
                    && rx > ret
                {
                    return Err(GeoContextError::InvalidTemporalOrdering(
                        "receiver_instant_utc_ms must be <= retrieval_instant_utc_ms",
                    ));
                }
                if let (Some(tx), Some(ret)) =
                    (self.transmit_instant_utc_ms, self.retrieval_instant_utc_ms)
                    && tx > ret
                {
                    return Err(GeoContextError::InvalidTemporalOrdering(
                        "transmit_instant_utc_ms must be <= retrieval_instant_utc_ms",
                    ));
                }
            }
            ObservationSource::InterpolatedSegment { .. } => {
                if self.target_interpolation_instant_utc_ms.is_none() {
                    return Err(GeoContextError::InvalidTemporalOrdering(
                        "interpolated segment must set target_interpolation_instant_utc_ms",
                    ));
                }
                if self.transmit_instant_utc_ms.is_some()
                    || self.receiver_instant_utc_ms.is_some()
                    || self.retrieval_instant_utc_ms.is_some()
                {
                    return Err(GeoContextError::InvalidTemporalOrdering(
                        "interpolated segment must not fabricate observed transmit/receiver/retrieval instants",
                    ));
                }
            }
        }

        if let Some(ref alt) = self.barometric_altitude
            && alt.reference() != AltitudeReference::BarometricUnadjusted
        {
            return Err(GeoContextError::IncompatibleAltitudeReferences {
                actual: alt.reference(),
                expected: AltitudeReference::BarometricUnadjusted,
            });
        }
        if let Some(ref alt) = self.geometric_altitude
            && alt.reference() != AltitudeReference::Wgs84Ellipsoid
            && alt.reference() != AltitudeReference::OrthometricMsl
        {
            return Err(GeoContextError::IncompatibleAltitudeReferences {
                actual: alt.reference(),
                expected: AltitudeReference::Wgs84Ellipsoid,
            });
        }
        if let Some(spd) = self.ground_speed_mps
            && (!spd.is_finite() || spd < 0.0)
        {
            return Err(GeoContextError::NonFiniteValue("ground_speed_mps"));
        }
        if let Some(ang) = self.track_angle_deg
            && (!ang.is_finite() || !(0.0..=360.0).contains(&ang))
        {
            return Err(GeoContextError::NonFiniteValue("track_angle_deg"));
        }
        if let Some(vr) = self.vertical_rate_mps
            && !vr.is_finite()
        {
            return Err(GeoContextError::NonFiniteValue("vertical_rate_mps"));
        }
        if let Some(ref sq) = self.squawk_octal
            && (sq.len() != 4 || !sq.chars().all(|c| ('0'..='7').contains(&c)))
        {
            return Err(GeoContextError::InvalidSquawk(sq.clone()));
        }
        Ok(())
    }
}

/// Complete kinematic flight track for an aircraft.
#[derive(Debug, Clone, PartialEq)]
pub struct AircraftTrack {
    icao_hex: String,
    callsign: Option<String>,
    aircraft_type: Option<String>,
    position_source: PositionSource,
    source_id: OpaqueId,
    points: Vec<AircraftTrackPoint>,
}

impl AircraftTrack {
    pub fn new(
        icao_hex: &str,
        callsign: Option<String>,
        aircraft_type: Option<String>,
        position_source: PositionSource,
        source_id: OpaqueId,
        points: Vec<AircraftTrackPoint>,
        limits: &ContextLimits,
    ) -> Result<Self, GeoContextError> {
        let uppercase_icao = icao_hex.trim().to_uppercase();
        if uppercase_icao.len() != 6 || !uppercase_icao.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(GeoContextError::InvalidIcaoHex(icao_hex.to_string()));
        }

        if points.len() > limits.max_points_per_track {
            return Err(GeoContextError::LimitExceeded {
                limit: "track.points",
                actual: points.len(),
                max: limits.max_points_per_track,
            });
        }

        // Validate all points and verify strict monotonic ordering without duplicates
        for (i, p) in points.iter().enumerate() {
            p.validate()?;
            if i > 0 {
                let prev = &points[i - 1];
                if p.effective_instant_utc_ms() < prev.effective_instant_utc_ms() {
                    return Err(GeoContextError::InvalidCoordinates {
                        lat: p.position.latitude_deg(),
                        lon: p.position.longitude_deg(),
                        reason: "track points must be monotonically ordered by timestamp",
                    });
                }
                if p.effective_instant_utc_ms() == prev.effective_instant_utc_ms() {
                    return Err(GeoContextError::DuplicateTimestamp(
                        p.effective_instant_utc_ms(),
                    ));
                }
            }
        }

        Ok(Self {
            icao_hex: uppercase_icao,
            callsign,
            aircraft_type,
            position_source,
            source_id,
            points,
        })
    }

    #[inline]
    #[must_use]
    pub fn icao_hex(&self) -> &str {
        &self.icao_hex
    }

    #[inline]
    #[must_use]
    pub fn callsign(&self) -> Option<&str> {
        self.callsign.as_deref()
    }

    #[inline]
    #[must_use]
    pub fn aircraft_type(&self) -> Option<&str> {
        self.aircraft_type.as_deref()
    }

    #[inline]
    #[must_use]
    pub fn position_source(&self) -> PositionSource {
        self.position_source
    }

    #[inline]
    #[must_use]
    pub fn source_id(&self) -> &OpaqueId {
        &self.source_id
    }

    #[inline]
    #[must_use]
    pub fn points(&self) -> &[AircraftTrackPoint] {
        &self.points
    }

    /// Re-validates track points and boundaries against limits.
    pub fn validate(&self, limits: &ContextLimits) -> Result<(), GeoContextError> {
        if self.points.len() > limits.max_points_per_track {
            return Err(GeoContextError::LimitExceeded {
                limit: "track.points",
                actual: self.points.len(),
                max: limits.max_points_per_track,
            });
        }
        for (i, p) in self.points.iter().enumerate() {
            p.validate()?;
            if i > 0
                && p.effective_instant_utc_ms() <= self.points[i - 1].effective_instant_utc_ms()
            {
                return Err(GeoContextError::DuplicateTimestamp(
                    p.effective_instant_utc_ms(),
                ));
            }
        }
        Ok(())
    }

    /// Interpolates aircraft kinematic state at `target_utc_ms` (`GEO-05`).
    ///
    /// - If `target_utc_ms` matches an exact observation, returns that direct observation.
    /// - If between observations, interpolates position via spherical SLERP on the unit sphere.
    /// - Rejects interpolation if the time gap exceeds `limits.max_interpolation_gap_ms`.
    /// - Never copies held single-endpoint values: fields missing at either endpoint become `None`.
    pub fn interpolate_at(
        &self,
        target_utc_ms: i64,
        limits: &ContextLimits,
    ) -> Result<AircraftTrackPoint, GeoContextError> {
        limits.validate()?;

        if self.points.is_empty() {
            return Err(GeoContextError::EmptyTrack);
        }

        let first = match self.points.first() {
            Some(p) => p,
            None => return Err(GeoContextError::EmptyTrack),
        };
        let last = match self.points.last() {
            Some(p) => p,
            None => return Err(GeoContextError::EmptyTrack),
        };

        let min_t = first.effective_instant_utc_ms();
        let max_t = last.effective_instant_utc_ms();

        if target_utc_ms < min_t || target_utc_ms > max_t {
            return Err(GeoContextError::OutOfTrackRange {
                target_utc_ms,
                min_utc_ms: min_t,
                max_utc_ms: max_t,
            });
        }

        let idx = match self
            .points
            .binary_search_by_key(&target_utc_ms, |p| p.effective_instant_utc_ms())
        {
            Ok(exact_idx) => {
                if let Some(p) = self.points.get(exact_idx) {
                    return Ok(p.clone());
                }
                return Err(GeoContextError::EmptyTrack);
            }
            Err(insert_idx) => insert_idx,
        };

        if idx == 0 || idx >= self.points.len() {
            return Err(GeoContextError::OutOfTrackRange {
                target_utc_ms,
                min_utc_ms: min_t,
                max_utc_ms: max_t,
            });
        }

        let p1 = match self.points.get(idx - 1) {
            Some(p) => p,
            None => return Err(GeoContextError::EmptyTrack),
        };
        let p2 = match self.points.get(idx) {
            Some(p) => p,
            None => return Err(GeoContextError::EmptyTrack),
        };

        let t1 = p1.effective_instant_utc_ms();
        let t2 = p2.effective_instant_utc_ms();
        let gap_ms = t1.abs_diff(t2);

        if gap_ms > limits.max_interpolation_gap_ms {
            return Err(GeoContextError::InterpolationGapExceeded {
                gap_ms,
                max_gap_ms: limits.max_interpolation_gap_ms,
            });
        }

        let f = if gap_ms == 0 {
            0.0
        } else {
            (target_utc_ms.abs_diff(t1) as f64) / (gap_ms as f64)
        };

        // Spherical SLERP on unit sphere
        let interp_point = spherical_slerp(p1.position, p2.position, f);

        // Interpolate barometric altitude only if present and matching datum on both endpoints
        let interp_baro = match (&p1.barometric_altitude, &p2.barometric_altitude) {
            (Some(a1), Some(a2)) if a1.reference() == a2.reference() => {
                let m = a1.meters() + f * (a2.meters() - a1.meters());
                Some(Altitude::new(m, a1.reference())?)
            }
            _ => None,
        };

        // Interpolate geometric altitude only if present and matching datum on both endpoints
        let interp_geom = match (&p1.geometric_altitude, &p2.geometric_altitude) {
            (Some(a1), Some(a2)) if a1.reference() == a2.reference() => {
                let m = a1.meters() + f * (a2.meters() - a1.meters());
                Some(Altitude::new(m, a1.reference())?)
            }
            _ => None,
        };

        let interp_spd = match (p1.ground_speed_mps, p2.ground_speed_mps) {
            (Some(s1), Some(s2)) => Some(s1 + f * (s2 - s1)),
            _ => None,
        };

        let interp_track = match (p1.track_angle_deg, p2.track_angle_deg) {
            (Some(a1), Some(a2)) => {
                let mut diff = (a2 - a1) % 360.0;
                if diff > 180.0 {
                    diff -= 360.0;
                } else if diff < -180.0 {
                    diff += 360.0;
                }
                Some((a1 + f * diff + 360.0) % 360.0)
            }
            _ => None,
        };

        let interp_vr = match (p1.vertical_rate_mps, p2.vertical_rate_mps) {
            (Some(v1), Some(v2)) => Some(v1 + f * (v2 - v1)),
            _ => None,
        };

        let interp_sq = match (&p1.squawk_octal, &p2.squawk_octal) {
            (Some(s1), Some(s2)) if s1 == s2 => Some(s1.clone()),
            _ => None,
        };

        let distance_gap_m = p1.position.great_circle_distance_m(p2.position);
        let dt_seconds = (gap_ms as f64) / 1000.0;
        let uncertainty_growth =
            limits.interpolation_growth_rate_mps * dt_seconds * 4.0 * f * (1.0 - f);
        let uncertainty_radius_m = limits.base_observation_uncertainty_m + uncertainty_growth;

        Ok(AircraftTrackPoint {
            transmit_instant_utc_ms: None,
            receiver_instant_utc_ms: None,
            retrieval_instant_utc_ms: None,
            target_interpolation_instant_utc_ms: Some(target_utc_ms),
            position: interp_point,
            barometric_altitude: interp_baro,
            geometric_altitude: interp_geom,
            ground_speed_mps: interp_spd,
            track_angle_deg: interp_track,
            vertical_rate_mps: interp_vr,
            squawk_octal: interp_sq,
            observation_source: ObservationSource::InterpolatedSegment {
                method_id: INTERPOLATION_METHOD_ID,
                elapsed_gap_ms: gap_ms,
                distance_gap_m,
                uncertainty_radius_m,
                base_uncertainty_m: limits.base_observation_uncertainty_m,
                growth_rate_mps: limits.interpolation_growth_rate_mps,
            },
            raw_source_hash: None,
        })
    }
}

// -----------------------------------------------------------------------------
// Maritime Tracking (`GEO-05`, `INTEL-03`)
// -----------------------------------------------------------------------------

/// AIS navigational status classification (ITU-R M.1371).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationalStatus {
    UnderwayUsingEngine,
    AtAnchor,
    NotUnderCommand,
    RestrictedManoeuverability,
    ConstrainedByDraught,
    Moored,
    Aground,
    EngagedInFishing,
    UnderwaySailing,
    Unknown,
}

/// Maritime vessel category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VesselType {
    Cargo,
    Tanker,
    Passenger,
    Fishing,
    TugOrTow,
    PleasureCraft,
    HighSpeedCraft,
    PilotVessel,
    SearchAndRescue,
    LawEnforcement,
    MilitaryOps,
    Unknown,
}

/// Immutable vessel identity attributes.
#[derive(Debug, Clone, PartialEq)]
pub struct VesselIdentity {
    pub mmsi: u32,
    pub vessel_name: Option<String>,
    pub imo_number: Option<u32>,
    pub callsign: Option<String>,
    pub vessel_type: VesselType,
    pub nav_status: NavigationalStatus,
    pub draught_m: Option<f64>,
}

impl VesselIdentity {
    /// Validates 9-digit MMSI and 7-digit IMO checksum.
    pub fn validate(&self) -> Result<(), GeoContextError> {
        if self.mmsi < 100_000_000 || self.mmsi > 999_999_999 {
            return Err(GeoContextError::InvalidMmsi(self.mmsi));
        }

        if let Some(imo) = self.imo_number {
            if !(1_000_000..=9_999_999).contains(&imo) {
                return Err(GeoContextError::InvalidImoNumber(imo));
            }
            let digits: Vec<u32> = imo
                .to_string()
                .chars()
                .filter_map(|c| c.to_digit(10))
                .collect();
            if digits.len() != 7 {
                return Err(GeoContextError::InvalidImoNumber(imo));
            }
            let check_sum = digits[0] * 7
                + digits[1] * 6
                + digits[2] * 5
                + digits[3] * 4
                + digits[4] * 3
                + digits[5] * 2;
            let expected_check = check_sum % 10;
            if digits[6] != expected_check {
                return Err(GeoContextError::InvalidImoNumber(imo));
            }
        }

        if let Some(d) = self.draught_m
            && (!d.is_finite() || d < 0.0)
        {
            return Err(GeoContextError::NonFiniteValue("draught_m"));
        }

        Ok(())
    }
}

/// Single instantaneous maritime vessel track point.
#[derive(Debug, Clone, PartialEq)]
pub struct MaritimeTrackPoint {
    pub transmit_instant_utc_ms: Option<i64>,
    pub receiver_instant_utc_ms: Option<i64>,
    pub retrieval_instant_utc_ms: Option<i64>,
    pub target_interpolation_instant_utc_ms: Option<i64>,
    pub position: GeoPointWgs84,
    pub speed_over_ground_mps: Option<f64>,
    pub course_over_ground_deg: Option<f64>,
    pub true_heading_deg: Option<f64>,
    pub observation_source: ObservationSource,
}

impl MaritimeTrackPoint {
    #[must_use]
    pub fn effective_instant_utc_ms(&self) -> i64 {
        self.target_interpolation_instant_utc_ms
            .or(self.receiver_instant_utc_ms)
            .or(self.transmit_instant_utc_ms)
            .or(self.retrieval_instant_utc_ms)
            .unwrap_or(0)
    }

    pub fn validate(&self) -> Result<(), GeoContextError> {
        match self.observation_source {
            ObservationSource::DirectObservation => {
                if self.target_interpolation_instant_utc_ms.is_some() {
                    return Err(GeoContextError::InvalidTemporalOrdering(
                        "direct vessel observation must not set target_interpolation_instant_utc_ms",
                    ));
                }
                if self.transmit_instant_utc_ms.is_none()
                    && self.receiver_instant_utc_ms.is_none()
                    && self.retrieval_instant_utc_ms.is_none()
                {
                    return Err(GeoContextError::InvalidTemporalOrdering(
                        "direct vessel observation must have at least one observed timestamp",
                    ));
                }
                if let (Some(tx), Some(rx)) =
                    (self.transmit_instant_utc_ms, self.receiver_instant_utc_ms)
                    && tx > rx
                {
                    return Err(GeoContextError::InvalidTemporalOrdering(
                        "transmit_instant_utc_ms must be <= receiver_instant_utc_ms",
                    ));
                }
                if let (Some(rx), Some(ret)) =
                    (self.receiver_instant_utc_ms, self.retrieval_instant_utc_ms)
                    && rx > ret
                {
                    return Err(GeoContextError::InvalidTemporalOrdering(
                        "receiver_instant_utc_ms must be <= retrieval_instant_utc_ms",
                    ));
                }
            }
            ObservationSource::InterpolatedSegment { .. } => {
                if self.target_interpolation_instant_utc_ms.is_none() {
                    return Err(GeoContextError::InvalidTemporalOrdering(
                        "interpolated vessel segment must set target_interpolation_instant_utc_ms",
                    ));
                }
                if self.transmit_instant_utc_ms.is_some()
                    || self.receiver_instant_utc_ms.is_some()
                    || self.retrieval_instant_utc_ms.is_some()
                {
                    return Err(GeoContextError::InvalidTemporalOrdering(
                        "interpolated vessel segment must not fabricate observed timestamps",
                    ));
                }
            }
        }

        if let Some(sog) = self.speed_over_ground_mps
            && (!sog.is_finite() || sog < 0.0)
        {
            return Err(GeoContextError::NonFiniteValue("speed_over_ground_mps"));
        }
        if let Some(cog) = self.course_over_ground_deg
            && (!cog.is_finite() || !(0.0..=360.0).contains(&cog))
        {
            return Err(GeoContextError::NonFiniteValue("course_over_ground_deg"));
        }
        if let Some(hdg) = self.true_heading_deg
            && (!hdg.is_finite() || !(0.0..=360.0).contains(&hdg))
        {
            return Err(GeoContextError::NonFiniteValue("true_heading_deg"));
        }
        Ok(())
    }
}

/// Complete maritime vessel track.
#[derive(Debug, Clone, PartialEq)]
pub struct MaritimeVesselTrack {
    identity: VesselIdentity,
    source_id: OpaqueId,
    points: Vec<MaritimeTrackPoint>,
}

impl MaritimeVesselTrack {
    pub fn new(
        identity: VesselIdentity,
        source_id: OpaqueId,
        points: Vec<MaritimeTrackPoint>,
        limits: &ContextLimits,
    ) -> Result<Self, GeoContextError> {
        identity.validate()?;

        if points.len() > limits.max_points_per_track {
            return Err(GeoContextError::LimitExceeded {
                limit: "track.points",
                actual: points.len(),
                max: limits.max_points_per_track,
            });
        }

        for (i, p) in points.iter().enumerate() {
            p.validate()?;
            if i > 0 {
                let prev = &points[i - 1];
                if p.effective_instant_utc_ms() < prev.effective_instant_utc_ms() {
                    return Err(GeoContextError::InvalidCoordinates {
                        lat: p.position.latitude_deg(),
                        lon: p.position.longitude_deg(),
                        reason: "maritime track points must be monotonically ordered by timestamp",
                    });
                }
                if p.effective_instant_utc_ms() == prev.effective_instant_utc_ms() {
                    return Err(GeoContextError::DuplicateTimestamp(
                        p.effective_instant_utc_ms(),
                    ));
                }
            }
        }

        Ok(Self {
            identity,
            source_id,
            points,
        })
    }

    #[inline]
    #[must_use]
    pub fn identity(&self) -> &VesselIdentity {
        &self.identity
    }

    #[inline]
    #[must_use]
    pub fn source_id(&self) -> &OpaqueId {
        &self.source_id
    }

    #[inline]
    #[must_use]
    pub fn points(&self) -> &[MaritimeTrackPoint] {
        &self.points
    }

    pub fn validate(&self, limits: &ContextLimits) -> Result<(), GeoContextError> {
        self.identity.validate()?;
        if self.points.len() > limits.max_points_per_track {
            return Err(GeoContextError::LimitExceeded {
                limit: "track.points",
                actual: self.points.len(),
                max: limits.max_points_per_track,
            });
        }
        for (i, p) in self.points.iter().enumerate() {
            p.validate()?;
            if i > 0
                && p.effective_instant_utc_ms() <= self.points[i - 1].effective_instant_utc_ms()
            {
                return Err(GeoContextError::DuplicateTimestamp(
                    p.effective_instant_utc_ms(),
                ));
            }
        }
        Ok(())
    }

    /// Interpolates vessel kinematic state at `target_utc_ms` (`GEO-05`).
    pub fn interpolate_at(
        &self,
        target_utc_ms: i64,
        limits: &ContextLimits,
    ) -> Result<MaritimeTrackPoint, GeoContextError> {
        limits.validate()?;

        if self.points.is_empty() {
            return Err(GeoContextError::EmptyTrack);
        }

        let first = match self.points.first() {
            Some(p) => p,
            None => return Err(GeoContextError::EmptyTrack),
        };
        let last = match self.points.last() {
            Some(p) => p,
            None => return Err(GeoContextError::EmptyTrack),
        };

        let min_t = first.effective_instant_utc_ms();
        let max_t = last.effective_instant_utc_ms();

        if target_utc_ms < min_t || target_utc_ms > max_t {
            return Err(GeoContextError::OutOfTrackRange {
                target_utc_ms,
                min_utc_ms: min_t,
                max_utc_ms: max_t,
            });
        }

        let idx = match self
            .points
            .binary_search_by_key(&target_utc_ms, |p| p.effective_instant_utc_ms())
        {
            Ok(exact_idx) => {
                if let Some(p) = self.points.get(exact_idx) {
                    return Ok(p.clone());
                }
                return Err(GeoContextError::EmptyTrack);
            }
            Err(insert_idx) => insert_idx,
        };

        if idx == 0 || idx >= self.points.len() {
            return Err(GeoContextError::OutOfTrackRange {
                target_utc_ms,
                min_utc_ms: min_t,
                max_utc_ms: max_t,
            });
        }

        let p1 = match self.points.get(idx - 1) {
            Some(p) => p,
            None => return Err(GeoContextError::EmptyTrack),
        };
        let p2 = match self.points.get(idx) {
            Some(p) => p,
            None => return Err(GeoContextError::EmptyTrack),
        };

        let t1 = p1.effective_instant_utc_ms();
        let t2 = p2.effective_instant_utc_ms();
        let gap_ms = t1.abs_diff(t2);

        if gap_ms > limits.max_interpolation_gap_ms {
            return Err(GeoContextError::InterpolationGapExceeded {
                gap_ms,
                max_gap_ms: limits.max_interpolation_gap_ms,
            });
        }

        let f = if gap_ms == 0 {
            0.0
        } else {
            (target_utc_ms.abs_diff(t1) as f64) / (gap_ms as f64)
        };

        let interp_point = spherical_slerp(p1.position, p2.position, f);

        let interp_sog = match (p1.speed_over_ground_mps, p2.speed_over_ground_mps) {
            (Some(s1), Some(s2)) => Some(s1 + f * (s2 - s1)),
            _ => None,
        };

        let interp_cog = match (p1.course_over_ground_deg, p2.course_over_ground_deg) {
            (Some(c1), Some(c2)) => {
                let mut diff = (c2 - c1) % 360.0;
                if diff > 180.0 {
                    diff -= 360.0;
                } else if diff < -180.0 {
                    diff += 360.0;
                }
                Some((c1 + f * diff + 360.0) % 360.0)
            }
            _ => None,
        };

        let interp_heading = match (p1.true_heading_deg, p2.true_heading_deg) {
            (Some(h1), Some(h2)) => {
                let mut diff = (h2 - h1) % 360.0;
                if diff > 180.0 {
                    diff -= 360.0;
                } else if diff < -180.0 {
                    diff += 360.0;
                }
                Some((h1 + f * diff + 360.0) % 360.0)
            }
            _ => None,
        };

        let distance_gap_m = p1.position.great_circle_distance_m(p2.position);
        let dt_seconds = (gap_ms as f64) / 1000.0;
        let uncertainty_growth =
            limits.interpolation_growth_rate_mps * dt_seconds * 4.0 * f * (1.0 - f);
        let uncertainty_radius_m = limits.base_observation_uncertainty_m + uncertainty_growth;

        Ok(MaritimeTrackPoint {
            transmit_instant_utc_ms: None,
            receiver_instant_utc_ms: None,
            retrieval_instant_utc_ms: None,
            target_interpolation_instant_utc_ms: Some(target_utc_ms),
            position: interp_point,
            speed_over_ground_mps: interp_sog,
            course_over_ground_deg: interp_cog,
            true_heading_deg: interp_heading,
            observation_source: ObservationSource::InterpolatedSegment {
                method_id: INTERPOLATION_METHOD_ID,
                elapsed_gap_ms: gap_ms,
                distance_gap_m,
                uncertainty_radius_m,
                base_uncertainty_m: limits.base_observation_uncertainty_m,
                growth_rate_mps: limits.interpolation_growth_rate_mps,
            },
        })
    }
}

// -----------------------------------------------------------------------------
// Airspace & Navigation Notices (`INTEL-03`)
// -----------------------------------------------------------------------------

/// Category of navigation or airspace notice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    AirspaceRestriction,
    RocketLaunchCorridor,
    MaritimeHazardWarning,
    MilitaryExercise,
    NavigationHazard,
}

/// Horizontal boundary representation for notices.
#[derive(Debug, Clone, PartialEq)]
pub enum NoticeSpatialBoundary {
    Circle {
        center: GeoPointWgs84,
        radius_m: f64,
    },
    BoundingBox(GeoBoundingBoxWgs84),
}

impl NoticeSpatialBoundary {
    #[must_use]
    pub fn contains(&self, point: GeoPointWgs84) -> bool {
        match self {
            Self::Circle { center, radius_m } => center.great_circle_distance_m(point) <= *radius_m,
            Self::BoundingBox(bb) => bb.contains_point(point),
        }
    }

    #[must_use]
    pub fn intersects_box(&self, target_box: &GeoBoundingBoxWgs84) -> bool {
        match self {
            Self::BoundingBox(bb) => bb.overlaps_box(target_box),
            Self::Circle { center, radius_m } => {
                let nearest = target_box.nearest_point_to(*center);
                center.great_circle_distance_m(nearest) <= *radius_m
            }
        }
    }
}

/// Airspace or maritime navigation notice (e.g. NOTAM, NAVWARN, launch restriction).
#[derive(Debug, Clone, PartialEq)]
pub struct NavigationNotice {
    pub notice_id: OpaqueId,
    pub issuing_authority: String,
    pub notice_kind: NoticeKind,
    pub issuance_instant_utc_ms: i64,
    pub retrieval_instant_utc_ms: i64,
    pub effective_window: TimeWindow,
    pub spatial_boundary: NoticeSpatialBoundary,
    pub lower_altitude: Option<Altitude>,
    pub upper_altitude: Option<Altitude>,
    pub summary: String,
    pub raw_content_sha256: [u8; 32],
}

impl NavigationNotice {
    #[must_use]
    pub fn is_active_at(&self, instant_utc_ms: i64) -> bool {
        self.effective_window.contains(instant_utc_ms)
    }

    /// Verifies if a 3D position at timestamp falls inside this notice volume.
    ///
    /// Incompatible vertical datums return `false` without aborting the query.
    #[must_use]
    pub fn intersects_position(&self, pos: &GeoPosition, instant_utc_ms: i64) -> bool {
        if !self.is_active_at(instant_utc_ms) {
            return false;
        }
        if !self.spatial_boundary.contains(pos.point()) {
            return false;
        }

        if self.lower_altitude.is_some() || self.upper_altitude.is_some() {
            match pos.altitude() {
                Some(ref pos_alt) => {
                    if let Some(ref lower) = self.lower_altitude {
                        match pos_alt.compare_in_same_reference(lower) {
                            Ok(ord) if ord.is_lt() => return false,
                            Err(_) => return false,
                            _ => {}
                        }
                    }
                    if let Some(ref upper) = self.upper_altitude {
                        match pos_alt.compare_in_same_reference(upper) {
                            Ok(ord) if ord.is_gt() => return false,
                            Err(_) => return false,
                            _ => {}
                        }
                    }
                }
                None => {
                    return false;
                }
            }
        }

        true
    }

    /// Verifies if this notice overlaps with the 4D query envelope.
    ///
    /// Incompatible vertical datums evaluate to `false` without aborting.
    #[must_use]
    pub fn overlaps_envelope(&self, envelope: &SpatioTemporalEnvelope) -> bool {
        if !self.effective_window.overlaps(&envelope.time_window) {
            return false;
        }
        if !self
            .spatial_boundary
            .intersects_box(&envelope.spatial_bounds)
        {
            return false;
        }

        if let Some(ref env_band) = envelope.altitude_band {
            match (&self.lower_altitude, &self.upper_altitude) {
                (Some(lower), Some(upper)) => {
                    if let Ok(notice_band) = AltitudeBand::new(*lower, *upper) {
                        env_band.overlaps_band(&notice_band).unwrap_or_default()
                    } else {
                        false
                    }
                }
                (Some(lower), None) => {
                    match lower.compare_in_same_reference(&env_band.max_altitude()) {
                        Ok(ord) => !ord.is_gt(),
                        Err(_) => false,
                    }
                }
                (None, Some(upper)) => {
                    match upper.compare_in_same_reference(&env_band.min_altitude()) {
                        Ok(ord) => !ord.is_lt(),
                        Err(_) => false,
                    }
                }
                (None, None) => true,
            }
        } else {
            true
        }
    }
}

// -----------------------------------------------------------------------------
// Sensor Coverage & Absence Verification (`GEO-05`, `A04`)
// -----------------------------------------------------------------------------

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
        if !self.coverage_bounds.south_lat_deg().is_finite()
            || !self.coverage_bounds.north_lat_deg().is_finite()
            || !self.coverage_bounds.west_lon_deg().is_finite()
            || !self.coverage_bounds.east_lon_deg().is_finite()
        {
            return Err(GeoContextError::NonFiniteValue("coverage_bounds"));
        }
        Ok(())
    }
}

/// Area surveillance coverage status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AreaCoverageStatus {
    FullyCoveredByActiveSensors {
        verified_covering_sensor_count: usize,
    },
    PartiallyCoveredGapsIdentified {
        reason: UnobservedReason,
    },
    UncoveredNoSensorData,
}

/// Explicit reason why an unobserved volume cannot assert absence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnobservedReason {
    NoSensorsCoveringRegion,
    PartialCoverageBlindSpots,
    SensorFeedDownOrStale,
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

/// Final assessment of traffic within the query envelope.
///
/// **Contract:** Absent track or recording is NEVER presented as evidence of an empty scene (`GEO-05`).
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

// -----------------------------------------------------------------------------
// Context Reconstruction Orchestrator
// -----------------------------------------------------------------------------

/// Bundled dataset inputs for context reconstruction.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextReconstructionInputs<'a> {
    pub weather_records: &'a [WeatherRecord],
    pub aircraft_tracks: &'a [AircraftTrack],
    pub vessel_tracks: &'a [MaritimeVesselTrack],
    pub notices: &'a [NavigationNotice],
    pub sensor_masks: &'a [SensorCoverageMask],
}

/// Weather coverage classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeatherCoverageStatus {
    ObservedWeatherAvailable,
    ForecastModelOnly,
    UnobservedCoverageGap,
}

/// Summary of weather conditions reconstructed for the envelope.
#[derive(Debug, Clone, PartialEq)]
pub struct WeatherReconstructionSummary {
    pub coverage_status: WeatherCoverageStatus,
    pub matching_records_count: usize,
    pub direct_observations_count: usize,
    pub forecast_models_count: usize,
    pub nearest_observation: Option<WeatherRecord>,
    pub nearest_forecast: Option<WeatherRecord>,
    pub upper_air_soundings_count: usize,
}

/// Unified reconstructed geographic context result.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextReconstructionResult {
    pub method_id: &'static str,
    pub query_envelope: SpatioTemporalEnvelope,
    pub weather_summary: WeatherReconstructionSummary,
    pub aircraft_tracks: Vec<AircraftTrack>,
    pub vessel_tracks: Vec<MaritimeVesselTrack>,
    pub active_notices: Vec<NavigationNotice>,
    pub traffic_assessment: SceneTrafficAssessment,
    pub reconstruction_timestamp_utc_ms: i64,
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

        // Check if sensor overlaps spatially
        if !mask.coverage_bounds.overlaps_box(&envelope.spatial_bounds) {
            continue;
        }

        // Check if sensor is operational
        if !mask.is_operational {
            any_sensor_offline = true;
            continue;
        }

        // Time containment check: mask operational window must completely contain query time window
        if !mask
            .operational_window
            .contains_window(&envelope.time_window)
        {
            any_outside_log = true;
            continue;
        }

        // Staleness check: feed must have reported through the envelope's time window.
        // If last update is before the window's end, telemetry inside the window is missing.
        if mask.last_feed_update_utc_ms < envelope.time_window.end_utc_ms() {
            any_sensor_stale = true;
            continue;
        }
        // If evaluation_instant is inside/after envelope, verify feed is not stale relative to evaluation_instant
        if evaluation_instant_utc_ms > mask.last_feed_update_utc_ms
            && evaluation_instant_utc_ms.abs_diff(mask.last_feed_update_utc_ms)
                > limits.max_sensor_staleness_ms
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
                // Envelope is unbounded vertically; bounded sensor cannot cover all altitudes.
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

/// Reconstructs geographic, atmospheric, aviation, maritime, and notice context
/// for a target spatio-temporal query envelope (`GEO-02`, `GEO-05`, `INTEL-03`, `A04`).
pub fn reconstruct_geographic_context(
    envelope: &SpatioTemporalEnvelope,
    inputs: &ContextReconstructionInputs<'_>,
    limits: &ContextLimits,
    reconstruction_instant_utc_ms: i64,
    mut cancel_token: Option<&mut dyn FnMut() -> Result<(), StopReason>>,
) -> Result<ContextReconstructionResult, GeoContextError> {
    if let Some(ref mut cancel) = cancel_token {
        cancel().map_err(GeoContextError::Stopped)?;
    }

    limits.validate()?;

    // Validate limits
    if inputs.aircraft_tracks.len() > limits.max_tracks {
        return Err(GeoContextError::LimitExceeded {
            limit: "aircraft_tracks",
            actual: inputs.aircraft_tracks.len(),
            max: limits.max_tracks,
        });
    }
    if inputs.vessel_tracks.len() > limits.max_tracks {
        return Err(GeoContextError::LimitExceeded {
            limit: "vessel_tracks",
            actual: inputs.vessel_tracks.len(),
            max: limits.max_tracks,
        });
    }
    if inputs.notices.len() > limits.max_notices {
        return Err(GeoContextError::LimitExceeded {
            limit: "notices",
            actual: inputs.notices.len(),
            max: limits.max_notices,
        });
    }
    if inputs.weather_records.len() > limits.max_weather_records {
        return Err(GeoContextError::LimitExceeded {
            limit: "weather_records",
            actual: inputs.weather_records.len(),
            max: limits.max_weather_records,
        });
    }

    // 1. Process Weather
    let mut matching_weather = Vec::new();
    let mut obs_count = 0;
    let mut forecast_count = 0;
    let mut soundings_count = 0;
    let mut nearest_obs: Option<WeatherRecord> = None;
    let mut nearest_forecast: Option<WeatherRecord> = None;
    let mut min_obs_cost = f64::MAX;
    let mut min_forecast_cost = f64::MAX;

    let envelope_center = envelope.spatial_bounds.center_point();
    // Overflow-safe midpoint using i128
    let envelope_mid_time = ((envelope.time_window.start_utc_ms() as i128
        + envelope.time_window.end_utc_ms() as i128)
        / 2) as i64;

    for w in inputs.weather_records {
        w.validate()?;
        if w.valid_window.overlaps(&envelope.time_window)
            && envelope
                .spatial_bounds
                .contains_point(w.station_position.point())
        {
            matching_weather.push(w.clone());
            let dist_m = envelope_center.great_circle_distance_m(w.station_position.point());
            let ref_time = w
                .observation_instant_utc_ms
                .or(w.forecast_issuance_utc_ms)
                .unwrap_or_else(|| w.valid_window.start_utc_ms());
            let time_diff_s = (ref_time.abs_diff(envelope_mid_time) as f64) / 1000.0;
            // Spatio-temporal distance cost: distance + velocity scale (10.0 m/s typical atmospheric advection) * time diff
            let st_cost = dist_m + time_diff_s * 10.0;

            match w.kind {
                WeatherReportKind::DirectObservation => {
                    obs_count += 1;
                    if !w.sounding_profile.is_empty() {
                        soundings_count += 1;
                    }
                    if st_cost < min_obs_cost {
                        min_obs_cost = st_cost;
                        nearest_obs = Some(w.clone());
                    }
                }
                WeatherReportKind::ForecastModel => {
                    forecast_count += 1;
                    if st_cost < min_forecast_cost {
                        min_forecast_cost = st_cost;
                        nearest_forecast = Some(w.clone());
                    }
                }
            }
        }
    }

    let weather_coverage = if obs_count > 0 {
        WeatherCoverageStatus::ObservedWeatherAvailable
    } else if forecast_count > 0 {
        WeatherCoverageStatus::ForecastModelOnly
    } else {
        WeatherCoverageStatus::UnobservedCoverageGap
    };

    if let Some(ref mut cancel) = cancel_token {
        cancel().map_err(GeoContextError::Stopped)?;
    }

    // 2. Filter intersecting Aircraft Tracks
    // B2 contract: only direct observations count towards ObservedTrafficPresent.
    // If a direct observation lies inside 2D box & time window but altitude is missing/unresolved,
    // we cannot assert absence in that altitude band.
    let mut filtered_aircraft = Vec::new();
    let mut observed_aircraft_count = 0;
    let mut has_unresolved_altitude_observation = false;

    for track in inputs.aircraft_tracks {
        track.validate(limits)?;
        let mut track_matches = false;
        let mut has_direct_obs_in_envelope = false;

        for pt in track.points() {
            pt.validate()?;
            let ts = pt.effective_instant_utc_ms();
            if !envelope.time_window.contains(ts) {
                continue;
            }
            if !envelope.spatial_bounds.contains_point(pt.position) {
                continue;
            }

            if let Some(ref env_band) = envelope.altitude_band {
                let pt_alt = pt.geometric_altitude.or(pt.barometric_altitude);
                match pt_alt {
                    Some(ref alt) => match env_band.contains_altitude(alt) {
                        Ok(true) => {
                            track_matches = true;
                            if pt.observation_source == ObservationSource::DirectObservation {
                                has_direct_obs_in_envelope = true;
                            }
                        }
                        Ok(false) => {}
                        Err(_) => {
                            if pt.observation_source == ObservationSource::DirectObservation {
                                has_unresolved_altitude_observation = true;
                            }
                        }
                    },
                    None => {
                        if pt.observation_source == ObservationSource::DirectObservation {
                            has_unresolved_altitude_observation = true;
                        }
                    }
                }
            } else {
                track_matches = true;
                if pt.observation_source == ObservationSource::DirectObservation {
                    has_direct_obs_in_envelope = true;
                }
            }
        }

        if track_matches {
            filtered_aircraft.push(track.clone());
            if has_direct_obs_in_envelope {
                observed_aircraft_count += 1;
            }
        }
    }

    // 3. Filter intersecting Maritime Vessel Tracks
    // Surface vessels operate at sea level (0.0 m MSL / geoid / ellipsoid).
    let mut filtered_vessels = Vec::new();
    let mut observed_vessel_count = 0;

    for track in inputs.vessel_tracks {
        track.validate(limits)?;
        let mut track_matches = false;
        let mut has_direct_obs_in_envelope = false;

        for pt in track.points() {
            pt.validate()?;
            let ts = pt.effective_instant_utc_ms();
            if !envelope.time_window.contains(ts) {
                continue;
            }
            if !envelope.spatial_bounds.contains_point(pt.position) {
                continue;
            }

            if let Some(ref env_band) = envelope.altitude_band {
                let sea_level_alt = Altitude::new(0.0, env_band.reference())?;
                if let Ok(true) = env_band.contains_altitude(&sea_level_alt) {
                    track_matches = true;
                    if pt.observation_source == ObservationSource::DirectObservation {
                        has_direct_obs_in_envelope = true;
                    }
                }
            } else {
                track_matches = true;
                if pt.observation_source == ObservationSource::DirectObservation {
                    has_direct_obs_in_envelope = true;
                }
            }
        }

        if track_matches {
            filtered_vessels.push(track.clone());
            if has_direct_obs_in_envelope {
                observed_vessel_count += 1;
            }
        }
    }

    if let Some(ref mut cancel) = cancel_token {
        cancel().map_err(GeoContextError::Stopped)?;
    }

    // 4. Filter active Navigation Notices
    let mut active_notices = Vec::new();
    for notice in inputs.notices {
        if notice.overlaps_envelope(envelope) {
            active_notices.push(notice.clone());
        }
    }

    if let Some(ref mut cancel) = cancel_token {
        cancel().map_err(GeoContextError::Stopped)?;
    }

    // 5. Compute Traffic Assessment ensuring "Absent Track is NOT an Empty Scene" contract
    let coverage_status = evaluate_sensor_coverage(
        envelope,
        inputs.sensor_masks,
        limits,
        reconstruction_instant_utc_ms,
    )?;

    let traffic_assessment = if observed_aircraft_count > 0 || observed_vessel_count > 0 {
        SceneTrafficAssessment::ObservedTrafficPresent {
            observed_aircraft_count,
            observed_vessel_count,
            coverage_status,
        }
    } else if has_unresolved_altitude_observation {
        SceneTrafficAssessment::InconclusiveUnobserved {
            reason: UnobservedReason::IncompleteAltitudeCoverage,
        }
    } else {
        match coverage_status {
            AreaCoverageStatus::UncoveredNoSensorData => {
                SceneTrafficAssessment::InconclusiveUnobserved {
                    reason: UnobservedReason::NoSensorsCoveringRegion,
                }
            }
            AreaCoverageStatus::PartiallyCoveredGapsIdentified { reason } => {
                SceneTrafficAssessment::InconclusiveUnobserved { reason }
            }
            AreaCoverageStatus::FullyCoveredByActiveSensors {
                verified_covering_sensor_count,
            } => SceneTrafficAssessment::ObservedAbsenceWithinCoverage {
                verified_covering_sensor_count,
                note: "All operational sensor masks covering envelope verified zero direct detections",
            },
        }
    };

    Ok(ContextReconstructionResult {
        method_id: METHOD_ID,
        query_envelope: envelope.clone(),
        weather_summary: WeatherReconstructionSummary {
            coverage_status: weather_coverage,
            matching_records_count: matching_weather.len(),
            direct_observations_count: obs_count,
            forecast_models_count: forecast_count,
            nearest_observation: nearest_obs,
            nearest_forecast,
            upper_air_soundings_count: soundings_count,
        },
        aircraft_tracks: filtered_aircraft,
        vessel_tracks: filtered_vessels,
        active_notices,
        traffic_assessment,
        reconstruction_timestamp_utc_ms: reconstruction_instant_utc_ms,
    })
}
