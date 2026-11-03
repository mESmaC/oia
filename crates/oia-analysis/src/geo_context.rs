//! Geographic context reconstruction kernel (`GEO-02`, `GEO-05`, `INTEL-03`, #24).
//!
//! Reconstructs atmospheric conditions, aircraft tracks, maritime vessel tracks,
//! and navigation/airspace notices across a spatio-temporal query envelope.
//!
//! # Architecture and Primitives Migration Note
//! The geometric and time primitives in this module (`GeoPointWgs84`, `GeoBoundingBoxWgs84`,
//! `Altitude`, `AltitudeReference`, `TimeWindow`) are **provisional domain models**
//! implemented locally in `crates/oia-analysis/src/geo_context.rs` pending the completion
//! and merge of `#12` (`crates/oia-time-geo`). Upon delivery of `#12`, these types
//! will migrate directly to the shared `oia-time-geo` contracts without changing the analytical
//! semantics or evidence preservation rules of this kernel.
//!
//! # Core Evidence & Quality Contracts
//! 1. **Observations vs Forecasts:** Weather observations (e.g. METAR, sounding) and
//!    forecasts (e.g. TAF, NWP model grid) remain strictly distinct with their own
//!    validity windows, observation/issuance timestamps, and retrieval timestamps (`GEO-02`, `GEO-05`).
//!    Missing weather station data never defaults to clear skies, calm winds, or standard atmosphere.
//! 2. **Observations vs Interpolation:** Aircraft and vessel track states distinguish
//!    direct sensor pings (`ObservationSource::DirectObservation`) from bounded
//!    interpolated states (`ObservationSource::InterpolatedSegment`). Interpolated
//!    segments retain their elapsed time gap, distance gap, method ID, and growing uncertainty radius,
//!    and are rejected when the gap exceeds caller-configured limits (`GEO-05`).
//! 3. **Absent Tracks are NOT an Empty Scene:** Zero observed tracks across an envelope
//!    yield `SceneTrafficAssessment::InconclusiveUnobserved` unless the entire 3D region
//!    and time window were verified covered by operational, non-stale sensor masks (`GEO-05`, `A04`).
//! 4. **Dateline Support:** Bounding boxes and queries explicitly support longitude
//!    wrapping across the 180° antimeridian (`west_lon_deg > east_lon_deg`) (`A04`).
//! 5. **Altitude References:** Altitudes preserve their explicit reference frame
//!    (MSL, AGL, WGS84 ellipsoid, BarometricUnadjusted) (`docs/architecture.md` §3.2).
//!    Comparisons between differing vertical reference frames are strictly rejected
//!    without explicit geoid/terrain models.

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
    pub base_observation_uncertainty_m: f64,
    pub interpolation_growth_rate_mps: f64,
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
            max_sensor_staleness_ms: 600_000, // 10 minutes
        }
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
    InvalidIcaoHex(String),
    InvalidMmsi(u32),
    InvalidImoNumber(u32),
    InvalidSquawk(u16),
    InvalidWeatherTimes(&'static str),
    IncompatibleAltitudeReferences {
        actual: AltitudeReference,
        expected: AltitudeReference,
    },
    LatitudeOutOfDomainForLinearInterpolation {
        lat_deg: f64,
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
                    "invalid time window: start ({start_utc_ms}) must not exceed end ({end_utc_ms})"
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
            Self::InvalidIcaoHex(s) => write!(f, "invalid ICAO 24-bit hex address: {s}"),
            Self::InvalidMmsi(mmsi) => write!(f, "invalid maritime MMSI: {mmsi}"),
            Self::InvalidImoNumber(imo) => write!(f, "invalid IMO ship number check digit: {imo}"),
            Self::InvalidSquawk(sq) => write!(
                f,
                "invalid transponder squawk (must be 4 octal digits): {sq}"
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
            Self::LatitudeOutOfDomainForLinearInterpolation { lat_deg } => {
                write!(
                    f,
                    "latitude {lat_deg} is in polar singularity zone (> 80.0 deg) where planar lat/lon interpolation is invalid"
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

/// Validated WGS84 horizontal coordinate point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeoPointWgs84 {
    lat_deg: f64,
    lon_deg: f64,
}

impl GeoPointWgs84 {
    /// Constructs and validates a WGS84 point.
    pub fn new(lat_deg: f64, lon_deg: f64) -> Result<Self, GeoContextError> {
        if !lat_deg.is_finite() {
            return Err(GeoContextError::NonFiniteValue("latitude_deg"));
        }
        if !lon_deg.is_finite() {
            return Err(GeoContextError::NonFiniteValue("longitude_deg"));
        }
        if !(-90.0..=90.0).contains(&lat_deg) {
            return Err(GeoContextError::InvalidCoordinates {
                lat: lat_deg,
                lon: lon_deg,
                reason: "latitude must be in [-90.0, 90.0]",
            });
        }
        if !(-180.0..=180.0).contains(&lon_deg) {
            return Err(GeoContextError::InvalidCoordinates {
                lat: lat_deg,
                lon: lon_deg,
                reason: "longitude must be in [-180.0, 180.0]",
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

    /// Computes spherical great-circle distance to another point in metres.
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

        let sin_half_lat = (d_lat * 0.5).sin();
        let sin_half_lon = (d_lon * 0.5).sin();

        let a = sin_half_lat * sin_half_lat + lat1.cos() * lat2.cos() * sin_half_lon * sin_half_lon;
        let a_clamped = a.clamp(0.0, 1.0);
        let c = 2.0 * a_clamped.sqrt().atan2((1.0 - a_clamped).sqrt());
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
        self.meters
            .partial_cmp(&other.meters)
            .ok_or(GeoContextError::NonFiniteValue("altitude comparison"))
    }
}

/// 3D geographic position with explicit altitude datum.
#[derive(Debug, Clone, Copy, PartialEq)]
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

/// Geographic bounding box supporting dateline crossing.
///
/// When `west_lon_deg <= east_lon_deg`, represents standard box spanning Greenwich or continental areas.
/// When `west_lon_deg > east_lon_deg`, represents a box crossing the 180° antimeridian
/// (e.g. 170°E through ±180° to -170°W).
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
        if !south_lat_deg.is_finite() || !north_lat_deg.is_finite() {
            return Err(GeoContextError::NonFiniteValue("bounding_box.latitude"));
        }
        if !west_lon_deg.is_finite() || !east_lon_deg.is_finite() {
            return Err(GeoContextError::NonFiniteValue("bounding_box.longitude"));
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
        if !(-180.0..=180.0).contains(&west_lon_deg) || !(-180.0..=180.0).contains(&east_lon_deg) {
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
            // Antimeridian crossing: point is >= west_lon (eastern hemisphere) OR <= east_lon (western hemisphere)
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
        // Latitude check
        if other.south_lat_deg < self.south_lat_deg || other.north_lat_deg > self.north_lat_deg {
            return false;
        }

        // Longitude containment check
        match (self.crosses_antimeridian(), other.crosses_antimeridian()) {
            (false, false) => {
                // Both normal
                other.west_lon_deg >= self.west_lon_deg && other.east_lon_deg <= self.east_lon_deg
            }
            (false, true) => {
                // Non-wrapping box cannot contain an antimeridian-wrapping box
                false
            }
            (true, true) => {
                // Both wrap antimeridian
                other.west_lon_deg >= self.west_lon_deg && other.east_lon_deg <= self.east_lon_deg
            }
            (true, false) => {
                // self wraps, other does not
                (other.west_lon_deg >= self.west_lon_deg && other.east_lon_deg >= self.west_lon_deg)
                    || (other.west_lon_deg <= self.east_lon_deg
                        && other.east_lon_deg <= self.east_lon_deg)
            }
        }
    }

    /// True if `self` and `other` overlap horizontally.
    #[must_use]
    pub fn overlaps_box(&self, other: &GeoBoundingBoxWgs84) -> bool {
        if self.north_lat_deg < other.south_lat_deg || self.south_lat_deg > other.north_lat_deg {
            return false;
        }

        let lon_overlap = |w1: f64, e1: f64, w2: f64, e2: f64| w1 <= e2 && w2 <= e1;

        match (self.crosses_antimeridian(), other.crosses_antimeridian()) {
            (false, false) => lon_overlap(
                self.west_lon_deg,
                self.east_lon_deg,
                other.west_lon_deg,
                other.east_lon_deg,
            ),
            (true, false) => {
                // self is [west..180] U [-180..east]
                lon_overlap(
                    self.west_lon_deg,
                    180.0,
                    other.west_lon_deg,
                    other.east_lon_deg,
                ) || lon_overlap(
                    -180.0,
                    self.east_lon_deg,
                    other.west_lon_deg,
                    other.east_lon_deg,
                )
            }
            (false, true) => {
                lon_overlap(
                    self.west_lon_deg,
                    self.east_lon_deg,
                    other.west_lon_deg,
                    180.0,
                ) || lon_overlap(
                    self.west_lon_deg,
                    self.east_lon_deg,
                    -180.0,
                    other.east_lon_deg,
                )
            }
            (true, true) => {
                // Both wrap across antimeridian -> always intersect at antimeridian
                true
            }
        }
    }

    /// Nominal center point of the bounding box.
    #[must_use]
    pub fn center_point(&self) -> GeoPointWgs84 {
        let lat = 0.5 * (self.south_lat_deg + self.north_lat_deg);
        let lon = if self.crosses_antimeridian() {
            let span = (180.0 - self.west_lon_deg) + (self.east_lon_deg + 180.0);
            let mut mid = self.west_lon_deg + span * 0.5;
            if mid > 180.0 {
                mid -= 360.0;
            }
            mid
        } else {
            0.5 * (self.west_lon_deg + self.east_lon_deg)
        };
        GeoPointWgs84 {
            lat_deg: lat,
            lon_deg: lon,
        }
    }
}

/// UTC millisecond time interval `[start_utc_ms, end_utc_ms)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeWindow {
    start_utc_ms: i64,
    end_utc_ms: i64,
}

impl TimeWindow {
    pub fn new(start_utc_ms: i64, end_utc_ms: i64) -> Result<Self, GeoContextError> {
        if start_utc_ms > end_utc_ms {
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
    pub fn new(min_altitude: Altitude, max_altitude: Altitude) -> Result<Self, GeoContextError> {
        min_altitude.compare_in_same_reference(&max_altitude)?;
        if min_altitude.meters > max_altitude.meters {
            return Err(GeoContextError::NonFiniteValue(
                "altitude band: min must be <= max",
            ));
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
    /// Missing altitude on position when envelope specifies an altitude band returns `Ok(false)`
    /// rather than guessing sea level.
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
                Some(ref alt) => {
                    return band.contains_altitude(alt);
                }
                None => {
                    // Position has no altitude measurement; cannot verify presence inside altitude band
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}

// -----------------------------------------------------------------------------
// Weather Subsystem (`GEO-02`, `GEO-05`, `AGENTS.md` §2)
// -----------------------------------------------------------------------------

/// Weather report classification: Direct observation vs Forecast model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeatherReportKind {
    DirectObservation,
    ForecastModel,
}

/// Standard aviation/meteorological cloud coverage codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudCoverage {
    Clear,
    Few,
    Scattered,
    Broken,
    Overcast,
}

/// Cloud layer at an explicit base/top altitude with vertical datum.
#[derive(Debug, Clone, PartialEq)]
pub struct CloudLayer {
    pub coverage: CloudCoverage,
    pub base_altitude: Altitude,
    pub top_altitude: Option<Altitude>,
}

/// Atmospheric sounding level for upper-air observations.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundingLevel {
    pub altitude: Altitude,
    pub temperature_c: f64,
    pub pressure_hpa: f64,
    pub wind_speed_mps: f64,
    pub wind_direction_deg: f64,
}

/// Normalized weather report with explicit time kinds and validation.
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
    /// Validates core contracts for weather observations versus forecasts.
    pub fn validate(&self) -> Result<(), GeoContextError> {
        match self.kind {
            WeatherReportKind::DirectObservation => {
                let obs_time =
                    self.observation_instant_utc_ms
                        .ok_or(GeoContextError::InvalidWeatherTimes(
                            "direct observation must provide observation_instant_utc_ms",
                        ))?;
                if !self.valid_window.contains(obs_time) {
                    return Err(GeoContextError::InvalidWeatherTimes(
                        "observation instant must fall within the declared valid_window",
                    ));
                }
                if self.forecast_issuance_utc_ms.is_some() || self.model_id.is_some() {
                    return Err(GeoContextError::InvalidWeatherTimes(
                        "direct observation must not carry forecast issuance or model_id",
                    ));
                }
            }
            WeatherReportKind::ForecastModel => {
                let issuance_time =
                    self.forecast_issuance_utc_ms
                        .ok_or(GeoContextError::InvalidWeatherTimes(
                            "forecast model must provide forecast_issuance_utc_ms",
                        ))?;
                if self.model_id.is_none() {
                    return Err(GeoContextError::InvalidWeatherTimes(
                        "forecast model must declare model_id",
                    ));
                }
                if issuance_time > self.valid_window.end_utc_ms() {
                    return Err(GeoContextError::InvalidWeatherTimes(
                        "forecast issuance must not postdate valid window end",
                    ));
                }
                if self.observation_instant_utc_ms.is_some() {
                    return Err(GeoContextError::InvalidWeatherTimes(
                        "forecast model must not carry observation_instant_utc_ms",
                    ));
                }
            }
        }

        // Numeric checks
        if let Some(t) = self.temperature_c
            && !t.is_finite()
        {
            return Err(GeoContextError::NonFiniteValue("weather.temperature_c"));
        }
        if let Some(dp) = self.dew_point_c
            && !dp.is_finite()
        {
            return Err(GeoContextError::NonFiniteValue("weather.dew_point_c"));
        }
        if let Some(p) = self.pressure_altimeter_hpa
            && (!p.is_finite() || p <= 0.0)
        {
            return Err(GeoContextError::NonFiniteValue(
                "weather.pressure_altimeter_hpa",
            ));
        }
        if let Some(rh) = self.relative_humidity_pct
            && (!rh.is_finite() || !(0.0..=100.0).contains(&rh))
        {
            return Err(GeoContextError::NonFiniteValue(
                "weather.relative_humidity_pct (must be in 0..100)",
            ));
        }
        if let Some(ws) = self.wind_speed_mps
            && (!ws.is_finite() || ws < 0.0)
        {
            return Err(GeoContextError::NonFiniteValue("weather.wind_speed_mps"));
        }
        if let Some(wg) = self.wind_gust_mps
            && (!wg.is_finite() || wg < 0.0)
        {
            return Err(GeoContextError::NonFiniteValue("weather.wind_gust_mps"));
        }
        if let Some(wd) = self.wind_direction_deg
            && (!wd.is_finite() || !(0.0..=360.0).contains(&wd))
        {
            return Err(GeoContextError::NonFiniteValue(
                "weather.wind_direction_deg",
            ));
        }
        if let Some(vis) = self.visibility_m
            && (!vis.is_finite() || vis < 0.0)
        {
            return Err(GeoContextError::NonFiniteValue("weather.visibility_m"));
        }

        // Sounding profile checks
        for (i, lvl) in self.sounding_profile.iter().enumerate() {
            if !lvl.temperature_c.is_finite()
                || !lvl.pressure_hpa.is_finite()
                || lvl.pressure_hpa <= 0.0
                || !lvl.wind_speed_mps.is_finite()
                || lvl.wind_speed_mps < 0.0
                || !lvl.wind_direction_deg.is_finite()
                || !(0.0..=360.0).contains(&lvl.wind_direction_deg)
            {
                return Err(GeoContextError::NonFiniteValue(
                    "sounding level numerical parameter",
                ));
            }
            if i > 0 {
                // Ensure sounding is ordered in strictly increasing altitude
                let prev = match self.sounding_profile.get(i - 1) {
                    Some(p) => p,
                    None => return Err(GeoContextError::EmptyTrack),
                };
                let ord = lvl.altitude.compare_in_same_reference(&prev.altitude)?;
                if !ord.is_gt() {
                    return Err(GeoContextError::NonFiniteValue(
                        "sounding levels must be ordered strictly ascending in altitude",
                    ));
                }
            }
        }

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Aircraft Tracking Subsystem (`GEO-05`, `A04`)
// -----------------------------------------------------------------------------

/// Sensor or network source of aircraft position telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionSource {
    Adsb,
    ModeS,
    PrimaryRadar,
    SecondaryRadar,
    FusedSensor,
}

/// Provenance of an individual track state point.
///
/// Direct sensor observations are kept strictly distinct from interpolated points.
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
    pub transmit_instant_utc_ms: i64,
    pub receiver_instant_utc_ms: i64,
    pub retrieval_instant_utc_ms: i64,
    pub position: GeoPointWgs84,
    pub barometric_altitude: Option<Altitude>,
    pub geometric_altitude: Option<Altitude>,
    pub ground_speed_mps: Option<f64>,
    pub track_angle_deg: Option<f64>,
    pub vertical_rate_mps: Option<f64>,
    pub squawk_octal: Option<u16>,
    pub observation_source: ObservationSource,
    pub raw_source_hash: Option<[u8; 32]>,
}

impl AircraftTrackPoint {
    /// Validates field values and references.
    pub fn validate(&self) -> Result<(), GeoContextError> {
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
        if let Some(sq) = self.squawk_octal {
            // Squawk transponder codes are 4 octal digits: digits 0-7, in range [0000..7777] octal
            let d0 = sq % 10;
            let d1 = (sq / 10) % 10;
            let d2 = (sq / 100) % 10;
            let d3 = (sq / 1000) % 10;
            if d0 > 7 || d1 > 7 || d2 > 7 || d3 > 7 || sq > 7777 {
                return Err(GeoContextError::InvalidSquawk(sq));
            }
        }
        Ok(())
    }
}

/// Complete or partial aircraft flight track.
#[derive(Debug, Clone, PartialEq)]
pub struct AircraftTrack {
    pub icao_hex: String,
    pub callsign: Option<String>,
    pub aircraft_type: Option<String>,
    pub position_source: PositionSource,
    pub source_id: OpaqueId,
    pub points: Vec<AircraftTrackPoint>,
}

impl AircraftTrack {
    /// Validates ICAO hex address, limits, and sorted point timestamps.
    pub fn new(
        icao_hex: &str,
        callsign: Option<String>,
        aircraft_type: Option<String>,
        position_source: PositionSource,
        source_id: OpaqueId,
        points: Vec<AircraftTrackPoint>,
        limits: &ContextLimits,
    ) -> Result<Self, GeoContextError> {
        let trimmed = icao_hex.trim();
        if trimmed.len() != 6 || !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(GeoContextError::InvalidIcaoHex(icao_hex.to_string()));
        }
        let uppercase_icao = trimmed.to_ascii_uppercase();

        if points.is_empty() {
            return Err(GeoContextError::EmptyTrack);
        }
        if points.len() > limits.max_points_per_track {
            return Err(GeoContextError::LimitExceeded {
                limit: "track points",
                actual: points.len(),
                max: limits.max_points_per_track,
            });
        }

        // Validate each point and verify strictly ascending or tied timestamps
        for (i, p) in points.iter().enumerate() {
            p.validate()?;
            if i > 0 {
                let prev = match points.get(i - 1) {
                    Some(pt) => pt,
                    None => return Err(GeoContextError::EmptyTrack),
                };
                if p.receiver_instant_utc_ms < prev.receiver_instant_utc_ms {
                    return Err(GeoContextError::InvalidTimeWindow {
                        start_utc_ms: prev.receiver_instant_utc_ms,
                        end_utc_ms: p.receiver_instant_utc_ms,
                    });
                }
                // Disallow duplicate timestamps with conflicting positions (deterministic tie-breaking)
                if p.receiver_instant_utc_ms == prev.receiver_instant_utc_ms
                    && p.position != prev.position
                {
                    return Err(GeoContextError::InvalidCoordinates {
                        lat: p.position.latitude_deg(),
                        lon: p.position.longitude_deg(),
                        reason: "conflicting position reports at identical timestamp",
                    });
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

    /// Interpolates aircraft kinematic state at `target_utc_ms` (`GEO-05`).
    ///
    /// - If `target_utc_ms` matches an exact observation, returns that direct observation.
    /// - If between observations, interpolates position, altitude, speed, and heading.
    /// - Rejects interpolation if the time gap exceeds `limits.max_interpolation_gap_ms`.
    /// - Bounded valid domain: disallows linear lat/lon interpolation in polar singularity zones (> 80° latitude).
    /// - Never copies held single-endpoint values: fields missing at either endpoint become `None`.
    pub fn interpolate_at(
        &self,
        target_utc_ms: i64,
        limits: &ContextLimits,
    ) -> Result<AircraftTrackPoint, GeoContextError> {
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

        if target_utc_ms < first.receiver_instant_utc_ms
            || target_utc_ms > last.receiver_instant_utc_ms
        {
            return Err(GeoContextError::OutOfTrackRange {
                target_utc_ms,
                min_utc_ms: first.receiver_instant_utc_ms,
                max_utc_ms: last.receiver_instant_utc_ms,
            });
        }

        // Binary search for surrounding points
        let idx = match self
            .points
            .binary_search_by_key(&target_utc_ms, |p| p.receiver_instant_utc_ms)
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
                min_utc_ms: first.receiver_instant_utc_ms,
                max_utc_ms: last.receiver_instant_utc_ms,
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

        let t1 = p1.receiver_instant_utc_ms;
        let t2 = p2.receiver_instant_utc_ms;
        let gap_ms = t2.abs_diff(t1);

        if gap_ms > limits.max_interpolation_gap_ms {
            return Err(GeoContextError::InterpolationGapExceeded {
                gap_ms,
                max_gap_ms: limits.max_interpolation_gap_ms,
            });
        }

        // Domain check for linear spherical interpolation: disallow polar regions (> 80 deg) where planar lat/lon fails
        if p1.position.latitude_deg().abs() > 80.0 || p2.position.latitude_deg().abs() > 80.0 {
            return Err(GeoContextError::LatitudeOutOfDomainForLinearInterpolation {
                lat_deg: p1.position.latitude_deg(),
            });
        }

        let fraction = if gap_ms == 0 {
            0.0
        } else {
            // Safe subtraction guaranteed by ordering check
            let elapsed_target = target_utc_ms.abs_diff(t1);
            elapsed_target as f64 / gap_ms as f64
        };

        let distance_gap_m = p1.position.great_circle_distance_m(p2.position);

        // Interpolate horizontal coordinates with dateline unwrapping
        let interp_lat = p1.position.latitude_deg()
            + (p2.position.latitude_deg() - p1.position.latitude_deg()) * fraction;

        let mut d_lon_deg = (p2.position.longitude_deg() - p1.position.longitude_deg()) % 360.0;
        if d_lon_deg > 180.0 {
            d_lon_deg -= 360.0;
        } else if d_lon_deg < -180.0 {
            d_lon_deg += 360.0;
        }
        let mut interp_lon = p1.position.longitude_deg() + d_lon_deg * fraction;
        if interp_lon > 180.0 {
            interp_lon -= 360.0;
        } else if interp_lon < -180.0 {
            interp_lon += 360.0;
        }

        let interp_point = GeoPointWgs84::new(interp_lat, interp_lon)?;

        // Interpolate barometric altitude: ONLY if both endpoints carry same datum; NEVER copy held values
        let interp_baro_alt = match (&p1.barometric_altitude, &p2.barometric_altitude) {
            (Some(a1), Some(a2)) if a1.reference() == a2.reference() => {
                let m = a1.meters() + (a2.meters() - a1.meters()) * fraction;
                Some(Altitude::new(m, a1.reference())?)
            }
            _ => None,
        };

        // Interpolate geometric altitude: ONLY if both endpoints carry same datum
        let interp_geom_alt = match (&p1.geometric_altitude, &p2.geometric_altitude) {
            (Some(g1), Some(g2)) if g1.reference() == g2.reference() => {
                let m = g1.meters() + (g2.meters() - g1.meters()) * fraction;
                Some(Altitude::new(m, g1.reference())?)
            }
            _ => None,
        };

        let interp_speed = match (p1.ground_speed_mps, p2.ground_speed_mps) {
            (Some(s1), Some(s2)) => Some(s1 + (s2 - s1) * fraction),
            _ => None,
        };

        let interp_vrate = match (p1.vertical_rate_mps, p2.vertical_rate_mps) {
            (Some(v1), Some(v2)) => Some(v1 + (v2 - v1) * fraction),
            _ => None,
        };

        // Angular shortest-arc interpolation for heading
        let interp_heading = match (p1.track_angle_deg, p2.track_angle_deg) {
            (Some(h1), Some(h2)) => {
                let mut diff = (h2 - h1) % 360.0;
                if diff > 180.0 {
                    diff -= 360.0;
                } else if diff < -180.0 {
                    diff += 360.0;
                }
                let h = (h1 + diff * fraction + 360.0) % 360.0;
                Some(h)
            }
            _ => None,
        };

        // Uncertainty grows quadratically peaking at midpoint fraction = 0.5
        let elapsed_seconds = gap_ms as f64 / 1000.0;
        let midpoint_weight = 4.0 * fraction * (1.0 - fraction);
        let uncertainty_radius_m = limits.base_observation_uncertainty_m
            + (limits.interpolation_growth_rate_mps * elapsed_seconds * midpoint_weight);

        Ok(AircraftTrackPoint {
            transmit_instant_utc_ms: target_utc_ms,
            receiver_instant_utc_ms: target_utc_ms,
            retrieval_instant_utc_ms: target_utc_ms,
            position: interp_point,
            barometric_altitude: interp_baro_alt,
            geometric_altitude: interp_geom_alt,
            ground_speed_mps: interp_speed,
            track_angle_deg: interp_heading,
            vertical_rate_mps: interp_vrate,
            squawk_octal: None, // Disallow copying held transponder squawk on interpolated points
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
// Maritime Tracking Subsystem (AIS) (`GEO-05`, `INTEL-03`)
// -----------------------------------------------------------------------------

/// AIS Navigational Status according to ITU-R M.1371.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationalStatus {
    UnderwayUsingEngine,
    AtAnchor,
    NotUnderCommand,
    RestrictedManoeuvrability,
    ConstrainedByDraught,
    Moored,
    Aground,
    EngagedInFishing,
    UnderwaySailing,
    Reserved,
    Unknown,
}

/// Maritime vessel broad category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VesselType {
    Cargo,
    Tanker,
    Passenger,
    Fishing,
    TugOrTow,
    LawEnforcement,
    SearchAndRescue,
    MilitaryOrGovernment,
    PleasureCraft,
    HighSpeedCraft,
    Other,
    Unknown,
}

/// Core identity attributes of a maritime vessel.
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
    pub fn validate(&self) -> Result<(), GeoContextError> {
        // MMSI is a 9-digit numerical identifier in [100000000, 999999999]
        if !(100_000_000..=999_999_999).contains(&self.mmsi) {
            return Err(GeoContextError::InvalidMmsi(self.mmsi));
        }

        // IMO number validation: 7 digits with check digit:
        // sum_{i=0}^5 d_i * (7 - i) mod 10 == check digit d_6
        if let Some(imo) = self.imo_number {
            if !(1_000_000..=9_999_999).contains(&imo) {
                return Err(GeoContextError::InvalidImoNumber(imo));
            }
            let digits = [
                imo / 1_000_000,
                (imo / 100_000) % 10,
                (imo / 10_000) % 10,
                (imo / 1_000) % 10,
                (imo / 100) % 10,
                (imo / 10) % 10,
                imo % 10,
            ];
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
    pub transmit_instant_utc_ms: i64,
    pub receiver_instant_utc_ms: i64,
    pub position: GeoPointWgs84,
    pub speed_over_ground_mps: Option<f64>,
    pub course_over_ground_deg: Option<f64>,
    pub true_heading_deg: Option<f64>,
    pub observation_source: ObservationSource,
}

impl MaritimeTrackPoint {
    pub fn validate(&self) -> Result<(), GeoContextError> {
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
        if let Some(h) = self.true_heading_deg
            && (!h.is_finite() || !(0.0..=360.0).contains(&h))
        {
            return Err(GeoContextError::NonFiniteValue("true_heading_deg"));
        }
        Ok(())
    }
}

/// Complete or partial maritime vessel track.
#[derive(Debug, Clone, PartialEq)]
pub struct MaritimeVesselTrack {
    pub identity: VesselIdentity,
    pub source_id: OpaqueId,
    pub points: Vec<MaritimeTrackPoint>,
}

impl MaritimeVesselTrack {
    pub fn new(
        identity: VesselIdentity,
        source_id: OpaqueId,
        points: Vec<MaritimeTrackPoint>,
        limits: &ContextLimits,
    ) -> Result<Self, GeoContextError> {
        identity.validate()?;

        if points.is_empty() {
            return Err(GeoContextError::EmptyTrack);
        }
        if points.len() > limits.max_points_per_track {
            return Err(GeoContextError::LimitExceeded {
                limit: "vessel points",
                actual: points.len(),
                max: limits.max_points_per_track,
            });
        }

        // Verify points are chronologically ordered and valid
        for (i, p) in points.iter().enumerate() {
            p.validate()?;
            if i > 0 {
                let prev = match points.get(i - 1) {
                    Some(pt) => pt,
                    None => return Err(GeoContextError::EmptyTrack),
                };
                if p.receiver_instant_utc_ms < prev.receiver_instant_utc_ms {
                    return Err(GeoContextError::InvalidTimeWindow {
                        start_utc_ms: prev.receiver_instant_utc_ms,
                        end_utc_ms: p.receiver_instant_utc_ms,
                    });
                }
                if p.receiver_instant_utc_ms == prev.receiver_instant_utc_ms
                    && p.position != prev.position
                {
                    return Err(GeoContextError::InvalidCoordinates {
                        lat: p.position.latitude_deg(),
                        lon: p.position.longitude_deg(),
                        reason: "conflicting vessel position reports at identical timestamp",
                    });
                }
            }
        }

        Ok(Self {
            identity,
            source_id,
            points,
        })
    }

    /// Interpolates maritime vessel kinematic state at `target_utc_ms`.
    pub fn interpolate_at(
        &self,
        target_utc_ms: i64,
        limits: &ContextLimits,
    ) -> Result<MaritimeTrackPoint, GeoContextError> {
        let first = match self.points.first() {
            Some(p) => p,
            None => return Err(GeoContextError::EmptyTrack),
        };
        let last = match self.points.last() {
            Some(p) => p,
            None => return Err(GeoContextError::EmptyTrack),
        };

        if target_utc_ms < first.receiver_instant_utc_ms
            || target_utc_ms > last.receiver_instant_utc_ms
        {
            return Err(GeoContextError::OutOfTrackRange {
                target_utc_ms,
                min_utc_ms: first.receiver_instant_utc_ms,
                max_utc_ms: last.receiver_instant_utc_ms,
            });
        }

        let idx = match self
            .points
            .binary_search_by_key(&target_utc_ms, |p| p.receiver_instant_utc_ms)
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
                min_utc_ms: first.receiver_instant_utc_ms,
                max_utc_ms: last.receiver_instant_utc_ms,
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

        let t1 = p1.receiver_instant_utc_ms;
        let t2 = p2.receiver_instant_utc_ms;
        let gap_ms = t2.abs_diff(t1);

        if gap_ms > limits.max_interpolation_gap_ms {
            return Err(GeoContextError::InterpolationGapExceeded {
                gap_ms,
                max_gap_ms: limits.max_interpolation_gap_ms,
            });
        }

        let fraction = if gap_ms == 0 {
            0.0
        } else {
            let elapsed_target = target_utc_ms.abs_diff(t1);
            elapsed_target as f64 / gap_ms as f64
        };

        let distance_gap_m = p1.position.great_circle_distance_m(p2.position);

        let interp_lat = p1.position.latitude_deg()
            + (p2.position.latitude_deg() - p1.position.latitude_deg()) * fraction;

        let mut d_lon_deg = (p2.position.longitude_deg() - p1.position.longitude_deg()) % 360.0;
        if d_lon_deg > 180.0 {
            d_lon_deg -= 360.0;
        } else if d_lon_deg < -180.0 {
            d_lon_deg += 360.0;
        }
        let mut interp_lon = p1.position.longitude_deg() + d_lon_deg * fraction;
        if interp_lon > 180.0 {
            interp_lon -= 360.0;
        } else if interp_lon < -180.0 {
            interp_lon += 360.0;
        }

        let interp_point = GeoPointWgs84::new(interp_lat, interp_lon)?;

        let interp_sog = match (p1.speed_over_ground_mps, p2.speed_over_ground_mps) {
            (Some(s1), Some(s2)) => Some(s1 + (s2 - s1) * fraction),
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
                Some((c1 + diff * fraction + 360.0) % 360.0)
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
                Some((h1 + diff * fraction + 360.0) % 360.0)
            }
            _ => None,
        };

        let elapsed_seconds = gap_ms as f64 / 1000.0;
        let midpoint_weight = 4.0 * fraction * (1.0 - fraction);
        let uncertainty_radius_m = limits.base_observation_uncertainty_m
            + (limits.interpolation_growth_rate_mps * elapsed_seconds * midpoint_weight);

        Ok(MaritimeTrackPoint {
            transmit_instant_utc_ms: target_utc_ms,
            receiver_instant_utc_ms: target_utc_ms,
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
                // Precise circle-box overlap: distance from center to closest point on target_box
                // Clamp center lat to box lat
                let clamped_lat = center
                    .latitude_deg()
                    .clamp(target_box.south_lat_deg(), target_box.north_lat_deg());
                let clamped_lon = if target_box.contains_point(*center) {
                    center.longitude_deg()
                } else if !target_box.crosses_antimeridian() {
                    center
                        .longitude_deg()
                        .clamp(target_box.west_lon_deg(), target_box.east_lon_deg())
                } else {
                    // Antimeridian crossing: find closer edge
                    let d_west = (center.longitude_deg() - target_box.west_lon_deg()).abs();
                    let d_east = (center.longitude_deg() - target_box.east_lon_deg()).abs();
                    if d_west < d_east {
                        target_box.west_lon_deg()
                    } else {
                        target_box.east_lon_deg()
                    }
                };
                if let Ok(closest) = GeoPointWgs84::new(clamped_lat, clamped_lon) {
                    center.great_circle_distance_m(closest) <= *radius_m
                } else {
                    false
                }
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
    /// Comparisons across different altitude datums fail closed (`IncompatibleAltitudeReferences`).
    /// Position without altitude returns `false` (unknown altitude cannot assert intersection).
    pub fn intersects_position(
        &self,
        pos: &GeoPosition,
        instant_utc_ms: i64,
    ) -> Result<bool, GeoContextError> {
        if !self.is_active_at(instant_utc_ms) {
            return Ok(false);
        }
        if !self.spatial_boundary.contains(pos.point()) {
            return Ok(false);
        }

        // Check vertical limits
        if self.lower_altitude.is_some() || self.upper_altitude.is_some() {
            match pos.altitude() {
                Some(ref pos_alt) => {
                    if let Some(ref lower) = self.lower_altitude {
                        let ord = pos_alt.compare_in_same_reference(lower)?;
                        if ord.is_lt() {
                            return Ok(false);
                        }
                    }
                    if let Some(ref upper) = self.upper_altitude {
                        let ord = pos_alt.compare_in_same_reference(upper)?;
                        if ord.is_gt() {
                            return Ok(false);
                        }
                    }
                }
                None => {
                    // Altitude is unknown; position cannot assert intersection with altitude-bounded notice
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }

    /// Verifies if this notice overlaps a 4D spatio-temporal envelope.
    pub fn overlaps_envelope(
        &self,
        envelope: &SpatioTemporalEnvelope,
    ) -> Result<bool, GeoContextError> {
        if !self.effective_window.overlaps(&envelope.time_window) {
            return Ok(false);
        }
        if !self
            .spatial_boundary
            .intersects_box(&envelope.spatial_bounds)
        {
            return Ok(false);
        }

        // Vertical overlap check
        if let Some(ref env_band) = envelope.altitude_band {
            if let Some(ref notice_lower) = self.lower_altitude {
                let ord = env_band
                    .max_altitude()
                    .compare_in_same_reference(notice_lower)?;
                if ord.is_lt() {
                    return Ok(false);
                }
            }
            if let Some(ref notice_upper) = self.upper_altitude {
                let ord = env_band
                    .min_altitude()
                    .compare_in_same_reference(notice_upper)?;
                if ord.is_gt() {
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }
}

// -----------------------------------------------------------------------------
// Sensor Coverage & Empty-Scene Prevention Contract (`GEO-05`, `A04`)
// -----------------------------------------------------------------------------

/// Explicit sensor coverage mask representing active receiver / radar / feed coverage.
#[derive(Debug, Clone, PartialEq)]
pub struct SensorCoverageMask {
    pub sensor_id: OpaqueId,
    pub source_id: OpaqueId,
    pub coverage_bounds: GeoBoundingBoxWgs84,
    pub altitude_band: Option<AltitudeBand>,
    pub operational_window: TimeWindow,
    pub last_feed_update_utc_ms: i64,
    pub sensor_type: PositionSource,
    pub is_operational: bool,
}

/// Overall area sensor coverage status for a queried envelope.
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

/// Specific reason why absence of traffic cannot be asserted as an empty scene.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnobservedReason {
    NoSensorsCoveringRegion,
    SensorFeedDownOrStale,
    PartialCoverageBlindSpots,
    QueryIntervalOutsideSensorLogWindow,
    IncompleteAltitudeCoverage,
}

impl fmt::Display for UnobservedReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSensorsCoveringRegion => {
                f.write_str("no registered sensors cover this spatial region")
            }
            Self::SensorFeedDownOrStale => f.write_str(
                "one or more required sensor feeds were offline, disconnected, or stale",
            ),
            Self::PartialCoverageBlindSpots => f.write_str(
                "incomplete sensor coverage mask contains blind spots in query envelope",
            ),
            Self::QueryIntervalOutsideSensorLogWindow => {
                f.write_str("query time window is outside the sensor operational logging interval")
            }
            Self::IncompleteAltitudeCoverage => {
                f.write_str("registered sensors do not cover the requested altitude band")
            }
        }
    }
}

/// Scene traffic evaluation.
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
pub fn evaluate_sensor_coverage(
    envelope: &SpatioTemporalEnvelope,
    sensor_masks: &[SensorCoverageMask],
    limits: &ContextLimits,
    evaluation_instant_utc_ms: i64,
) -> Result<AreaCoverageStatus, GeoContextError> {
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
        // Check if sensor overlaps spatially
        if !mask.coverage_bounds.overlaps_box(&envelope.spatial_bounds) {
            continue;
        }

        // Check if sensor is operational
        if !mask.is_operational {
            any_sensor_offline = true;
            continue;
        }

        // Check staleness: last feed update must not exceed limit
        let feed_age_ms = evaluation_instant_utc_ms.abs_diff(mask.last_feed_update_utc_ms);
        if feed_age_ms > limits.max_sensor_staleness_ms {
            any_sensor_stale = true;
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

        // Horizontal spatial containment check: mask must completely contain the envelope box
        if !mask.coverage_bounds.contains_box(&envelope.spatial_bounds) {
            any_horizontal_partial = true;
            continue;
        }

        // Altitude containment check: if envelope specifies altitude band, mask must contain it
        if let Some(ref env_band) = envelope.altitude_band {
            match mask.altitude_band {
                Some(ref mask_band) => {
                    if !mask_band.contains_band(env_band)? {
                        any_altitude_gap = true;
                        continue;
                    }
                }
                None => {
                    any_altitude_gap = true;
                    continue;
                }
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
    let envelope_mid_time = envelope.time_window.start_utc_ms()
        + (envelope.time_window.end_utc_ms() - envelope.time_window.start_utc_ms()) / 2;

    for w in inputs.weather_records {
        w.validate()?;
        if w.valid_window.overlaps(&envelope.time_window)
            && envelope
                .spatial_bounds
                .contains_point(w.station_position.point())
        {
            matching_weather.push(w.clone());
            let dist_m = envelope_center.great_circle_distance_m(w.station_position.point());
            let time_diff_s =
                (w.valid_window.start_utc_ms().abs_diff(envelope_mid_time) as f64) / 1000.0;
            // Spatio-temporal distance cost: distance + velocity scale * time diff
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
    // Also respects the envelope's altitude band.
    let mut filtered_aircraft = Vec::new();
    let mut observed_aircraft_count = 0;

    for track in inputs.aircraft_tracks {
        let mut track_matches = false;
        let mut has_direct_obs_in_envelope = false;

        for pt in &track.points {
            let pos = GeoPosition::new(
                pt.position,
                pt.geometric_altitude.or(pt.barometric_altitude),
            );
            if envelope.contains_position(&pos, pt.receiver_instant_utc_ms)? {
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
    let mut filtered_vessels = Vec::new();
    let mut observed_vessel_count = 0;

    for track in inputs.vessel_tracks {
        let mut track_matches = false;
        let mut has_direct_obs_in_envelope = false;

        for pt in &track.points {
            let pos = GeoPosition::new(pt.position, None);
            if envelope.contains_position(&pos, pt.receiver_instant_utc_ms)? {
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

    // 4. Filter active Navigation Notices (B5 contract)
    let mut active_notices = Vec::new();
    for notice in inputs.notices {
        if notice.overlaps_envelope(envelope)? {
            active_notices.push(notice.clone());
        }
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
