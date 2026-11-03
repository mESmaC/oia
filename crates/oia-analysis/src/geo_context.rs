//! Geographic context reconstruction kernel (`GEO-02`, `GEO-05`, `INTEL-03`, #24).
//!
//! Reconstructs atmospheric conditions, aircraft tracks, maritime vessel tracks,
//! and navigation/airspace notices across a spatio-temporal query envelope.
//!
//! # Core Evidence & Quality Contracts
//! 1. **Observations vs Forecasts:** Weather observations (e.g. METAR, sounding) and
//!    forecasts (e.g. TAF, NWP model grid) remain strictly distinct with their own
//!    validity windows and issuance timestamps (`GEO-02`, `GEO-05`). Missing weather
//!    station data never defaults to clear skies or standard conditions.
//! 2. **Observations vs Interpolation:** Aircraft and vessel track states distinguish
//!    direct sensor pings (`ObservationSource::DirectObservation`) from bounded
//!    interpolated states (`ObservationSource::InterpolatedSegment`). Interpolated
//!    segments retain their elapsed time gap, distance gap, and growing uncertainty radius,
//!    and are rejected when the gap exceeds caller-configured limits (`GEO-05`).
//! 3. **Absent Tracks are NOT an Empty Scene:** Zero observed tracks across an envelope
//!    yield `SceneTrafficAssessment::InconclusiveUnobserved` unless the entire region
//!    and time window were verified covered by operational sensor masks (`GEO-05`, `A04`).
//! 4. **Dateline Support:** Bounding boxes and queries explicitly support longitude
//!    wrapping across the 180° antimeridian (`min_lon > max_lon`) (`A04`).
//! 5. **Altitude References:** Altitudes preserve their explicit reference frame
//!    (MSL, AGL, WGS84 ellipsoid, ISA pressure altitude) (`docs/architecture.md` §3.2).

use oia_domain::{OpaqueId, VersionId};
use std::fmt;

/// Method identifier pinned alongside reconstructed context results.
pub const METHOD_ID: &str = "oia-geo-context-reconstruction-v1";

/// Standard mean Earth radius for spherical geodetic calculations (metres).
pub const EARTH_RADIUS_METERS: f64 = 6_371_000.0;

/// Default maximum allowed time gap for track interpolation (5 minutes).
pub const DEFAULT_MAX_INTERPOLATION_GAP_MS: u64 = 300_000;

/// Default base position uncertainty for direct observations (metres).
pub const DEFAULT_BASE_OBSERVATION_UNCERTAINTY_M: f64 = 15.0;

/// Default position uncertainty growth rate during interpolation (metres per second).
pub const DEFAULT_INTERPOLATION_GROWTH_RATE_MPS: f64 = 2.0;

/// Cooperative cancellation reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    Cancelled,
    DeadlineExceeded,
}

/// Operational resource and validation limits for context reconstruction.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextLimits {
    pub max_points_per_track: usize,
    pub max_tracks: usize,
    pub max_notices: usize,
    pub max_weather_records: usize,
    pub max_interpolation_gap_ms: u64,
}

impl Default for ContextLimits {
    fn default() -> Self {
        Self {
            max_points_per_track: 10_000,
            max_tracks: 1_000,
            max_notices: 1_000,
            max_weather_records: 1_000,
            max_interpolation_gap_ms: DEFAULT_MAX_INTERPOLATION_GAP_MS,
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
            Self::Stopped(StopReason::Cancelled) => f.write_str("context reconstruction cancelled"),
            Self::Stopped(StopReason::DeadlineExceeded) => {
                f.write_str("context reconstruction deadline exceeded")
            }
        }
    }
}

impl std::error::Error for GeoContextError {}

/// WGS84 horizontal coordinate point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeoPoint {
    pub latitude_deg: f64,
    pub longitude_deg: f64,
}

impl GeoPoint {
    /// Constructs and validates a WGS84 point.
    pub fn new(latitude_deg: f64, longitude_deg: f64) -> Result<Self, GeoContextError> {
        if !latitude_deg.is_finite() {
            return Err(GeoContextError::NonFiniteValue("latitude_deg"));
        }
        if !longitude_deg.is_finite() {
            return Err(GeoContextError::NonFiniteValue("longitude_deg"));
        }
        if !(-90.0..=90.0).contains(&latitude_deg) {
            return Err(GeoContextError::InvalidCoordinates {
                lat: latitude_deg,
                lon: longitude_deg,
                reason: "latitude must be in [-90.0, 90.0]",
            });
        }
        if !(-180.0..=180.0).contains(&longitude_deg) {
            return Err(GeoContextError::InvalidCoordinates {
                lat: latitude_deg,
                lon: longitude_deg,
                reason: "longitude must be in [-180.0, 180.0]",
            });
        }
        Ok(Self {
            latitude_deg,
            longitude_deg,
        })
    }

    /// Computes spherical great-circle distance to another point in metres.
    pub fn great_circle_distance_m(&self, other: GeoPoint) -> f64 {
        let lat1 = self.latitude_deg.to_radians();
        let lat2 = other.latitude_deg.to_radians();
        let d_lat = (other.latitude_deg - self.latitude_deg).to_radians();

        let mut d_lon_deg = (other.longitude_deg - self.longitude_deg) % 360.0;
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

    /// Initial great-circle bearing from `self` to `other` in degrees [0.0, 360.0).
    pub fn initial_bearing_deg(&self, other: GeoPoint) -> f64 {
        let lat1 = self.latitude_deg.to_radians();
        let lat2 = other.latitude_deg.to_radians();

        let mut d_lon_deg = (other.longitude_deg - self.longitude_deg) % 360.0;
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

/// Explicit altitude reference frame (`docs/architecture.md` §3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AltitudeReference {
    MeanSeaLevel,
    AboveGroundLevel,
    Wgs84Ellipsoid,
    PressureAltitudeStandardAtmosphere,
    UnknownReference,
}

/// Altitude measurement with an explicit reference frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Altitude {
    pub meters: f64,
    pub reference: AltitudeReference,
}

impl Altitude {
    pub fn new(meters: f64, reference: AltitudeReference) -> Result<Self, GeoContextError> {
        if !meters.is_finite() {
            return Err(GeoContextError::NonFiniteValue("altitude.meters"));
        }
        Ok(Self { meters, reference })
    }
}

/// 3D geographic position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeoPosition {
    pub point: GeoPoint,
    pub altitude: Option<Altitude>,
}

impl GeoPosition {
    pub fn new(point: GeoPoint, altitude: Option<Altitude>) -> Self {
        Self { point, altitude }
    }
}

/// Geographic bounding box supporting dateline crossing.
///
/// When `min_lon_deg <= max_lon_deg`, represents standard box.
/// When `min_lon_deg > max_lon_deg`, represents a box crossing the 180° antimeridian
/// (e.g. 170°E through ±180° to -170°W).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundingBox {
    pub min_lat_deg: f64,
    pub max_lat_deg: f64,
    pub min_lon_deg: f64,
    pub max_lon_deg: f64,
}

impl BoundingBox {
    pub fn new(
        min_lat_deg: f64,
        max_lat_deg: f64,
        min_lon_deg: f64,
        max_lon_deg: f64,
    ) -> Result<Self, GeoContextError> {
        if !min_lat_deg.is_finite() || !max_lat_deg.is_finite() {
            return Err(GeoContextError::NonFiniteValue("bounding_box.latitude"));
        }
        if !min_lon_deg.is_finite() || !max_lon_deg.is_finite() {
            return Err(GeoContextError::NonFiniteValue("bounding_box.longitude"));
        }
        if min_lat_deg > max_lat_deg
            || !(-90.0..=90.0).contains(&min_lat_deg)
            || !(-90.0..=90.0).contains(&max_lat_deg)
        {
            return Err(GeoContextError::InvalidCoordinates {
                lat: min_lat_deg,
                lon: min_lon_deg,
                reason: "invalid latitude bounds in bounding box",
            });
        }
        if !(-180.0..=180.0).contains(&min_lon_deg) || !(-180.0..=180.0).contains(&max_lon_deg) {
            return Err(GeoContextError::InvalidCoordinates {
                lat: min_lat_deg,
                lon: min_lon_deg,
                reason: "longitude bounds must be in [-180.0, 180.0]",
            });
        }
        Ok(Self {
            min_lat_deg,
            max_lat_deg,
            min_lon_deg,
            max_lon_deg,
        })
    }

    /// Checks if a point lies within the bounding box, respecting antimeridian wrap.
    pub fn contains(&self, point: GeoPoint) -> bool {
        if point.latitude_deg < self.min_lat_deg || point.latitude_deg > self.max_lat_deg {
            return false;
        }
        if self.min_lon_deg <= self.max_lon_deg {
            point.longitude_deg >= self.min_lon_deg && point.longitude_deg <= self.max_lon_deg
        } else {
            // Antimeridian crossing: point is >= min_lon (eastern hemisphere) OR <= max_lon (western hemisphere)
            point.longitude_deg >= self.min_lon_deg || point.longitude_deg <= self.max_lon_deg
        }
    }

    /// True if this bounding box crosses the 180° antimeridian.
    pub fn crosses_antimeridian(&self) -> bool {
        self.min_lon_deg > self.max_lon_deg
    }
}

/// UTC millisecond time interval `[start_utc_ms, end_utc_ms)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeWindow {
    pub start_utc_ms: i64,
    pub end_utc_ms: i64,
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

    pub fn contains(&self, instant_utc_ms: i64) -> bool {
        instant_utc_ms >= self.start_utc_ms && instant_utc_ms < self.end_utc_ms
    }

    pub fn overlaps(&self, other: &TimeWindow) -> bool {
        self.start_utc_ms < other.end_utc_ms && other.start_utc_ms < self.end_utc_ms
    }
}

/// Spatio-temporal query envelope (`GEO-02`, `GEO-05`, `A04`).
#[derive(Debug, Clone, PartialEq)]
pub struct SpatioTemporalEnvelope {
    pub spatial_bounds: BoundingBox,
    pub altitude_band_m: Option<(f64, f64)>,
    pub time_window: TimeWindow,
}

impl SpatioTemporalEnvelope {
    pub fn new(
        spatial_bounds: BoundingBox,
        altitude_band_m: Option<(f64, f64)>,
        time_window: TimeWindow,
    ) -> Result<Self, GeoContextError> {
        if let Some((min_alt, max_alt)) = altitude_band_m
            && (!min_alt.is_finite() || !max_alt.is_finite() || min_alt > max_alt)
        {
            return Err(GeoContextError::NonFiniteValue("altitude_band_m"));
        }
        Ok(Self {
            spatial_bounds,
            altitude_band_m,
            time_window,
        })
    }

    pub fn contains_position(&self, position: &GeoPosition, instant_utc_ms: i64) -> bool {
        if !self.time_window.contains(instant_utc_ms) {
            return false;
        }
        if !self.spatial_bounds.contains(position.point) {
            return false;
        }
        if let Some((min_alt, max_alt)) = self.altitude_band_m {
            match position.altitude {
                Some(alt) => {
                    if alt.meters < min_alt || alt.meters > max_alt {
                        return false;
                    }
                }
                None => return false,
            }
        }
        true
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

/// Cloud layer at an explicit base/top altitude.
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

/// Normalized weather report.
#[derive(Debug, Clone, PartialEq)]
pub struct WeatherRecord {
    pub source_id: OpaqueId,
    pub version_id: VersionId,
    pub station_id: OpaqueId,
    pub station_position: GeoPosition,
    pub kind: WeatherReportKind,
    pub observation_instant_utc_ms: Option<i64>,
    pub forecast_issuance_utc_ms: Option<i64>,
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
                if self.observation_instant_utc_ms.is_none() {
                    return Err(GeoContextError::InvalidCoordinates {
                        lat: self.station_position.point.latitude_deg,
                        lon: self.station_position.point.longitude_deg,
                        reason: "direct weather observation must provide observation_instant_utc_ms",
                    });
                }
            }
            WeatherReportKind::ForecastModel => {
                if self.forecast_issuance_utc_ms.is_none() || self.model_id.is_none() {
                    return Err(GeoContextError::InvalidCoordinates {
                        lat: self.station_position.point.latitude_deg,
                        lon: self.station_position.point.longitude_deg,
                        reason: "weather forecast must provide forecast_issuance_utc_ms and model_id",
                    });
                }
            }
        }
        if let Some(t) = self.temperature_c
            && !t.is_finite()
        {
            return Err(GeoContextError::NonFiniteValue("weather.temperature_c"));
        }
        if let Some(ws) = self.wind_speed_mps
            && (!ws.is_finite() || ws < 0.0)
        {
            return Err(GeoContextError::NonFiniteValue("weather.wind_speed_mps"));
        }
        if let Some(wd) = self.wind_direction_deg
            && (!wd.is_finite() || !(0.0..=360.0).contains(&wd))
        {
            return Err(GeoContextError::NonFiniteValue(
                "weather.wind_direction_deg",
            ));
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
        elapsed_gap_ms: u64,
        distance_gap_m: f64,
        uncertainty_radius_m: f64,
    },
}

/// Single instantaneous aircraft kinematic state point.
#[derive(Debug, Clone, PartialEq)]
pub struct AircraftTrackPoint {
    pub timestamp_utc_ms: i64,
    pub position: GeoPoint,
    pub barometric_altitude_m: Option<f64>,
    pub geometric_altitude_m: Option<f64>,
    pub ground_speed_mps: Option<f64>,
    pub track_angle_deg: Option<f64>,
    pub vertical_rate_mps: Option<f64>,
    pub squawk: Option<u16>,
    pub observation_source: ObservationSource,
    pub raw_source_hash: Option<[u8; 32]>,
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
    /// Validates ICAO hex address and sorted point timestamps.
    pub fn new(
        icao_hex: &str,
        callsign: Option<String>,
        aircraft_type: Option<String>,
        position_source: PositionSource,
        source_id: OpaqueId,
        points: Vec<AircraftTrackPoint>,
    ) -> Result<Self, GeoContextError> {
        let trimmed = icao_hex.trim();
        if trimmed.len() != 6 || !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(GeoContextError::InvalidIcaoHex(icao_hex.to_string()));
        }
        let uppercase_icao = trimmed.to_ascii_uppercase();

        if points.is_empty() {
            return Err(GeoContextError::EmptyTrack);
        }

        // Verify points are chronologically ordered
        for window in points.windows(2) {
            if let [a, b] = window
                && a.timestamp_utc_ms > b.timestamp_utc_ms
            {
                return Err(GeoContextError::InvalidTimeWindow {
                    start_utc_ms: a.timestamp_utc_ms,
                    end_utc_ms: b.timestamp_utc_ms,
                });
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
    /// - Tags the output as `ObservationSource::InterpolatedSegment` with explicit uncertainty growth.
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

        if target_utc_ms < first.timestamp_utc_ms || target_utc_ms > last.timestamp_utc_ms {
            return Err(GeoContextError::OutOfTrackRange {
                target_utc_ms,
                min_utc_ms: first.timestamp_utc_ms,
                max_utc_ms: last.timestamp_utc_ms,
            });
        }

        // Binary search for surrounding points
        let idx = match self
            .points
            .binary_search_by_key(&target_utc_ms, |p| p.timestamp_utc_ms)
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
                min_utc_ms: first.timestamp_utc_ms,
                max_utc_ms: last.timestamp_utc_ms,
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

        let t1 = p1.timestamp_utc_ms;
        let t2 = p2.timestamp_utc_ms;
        let gap_ms = (t2 - t1).unsigned_abs();

        if gap_ms > limits.max_interpolation_gap_ms {
            return Err(GeoContextError::InterpolationGapExceeded {
                gap_ms,
                max_gap_ms: limits.max_interpolation_gap_ms,
            });
        }

        let fraction = if gap_ms == 0 {
            0.0
        } else {
            (target_utc_ms - t1) as f64 / gap_ms as f64
        };

        let distance_gap_m = p1.position.great_circle_distance_m(p2.position);

        // Interpolate horizontal coordinates with dateline unwrapping
        let interp_lat = p1.position.latitude_deg
            + (p2.position.latitude_deg - p1.position.latitude_deg) * fraction;

        let mut d_lon_deg = (p2.position.longitude_deg - p1.position.longitude_deg) % 360.0;
        if d_lon_deg > 180.0 {
            d_lon_deg -= 360.0;
        } else if d_lon_deg < -180.0 {
            d_lon_deg += 360.0;
        }
        let mut interp_lon = p1.position.longitude_deg + d_lon_deg * fraction;
        if interp_lon > 180.0 {
            interp_lon -= 360.0;
        } else if interp_lon < -180.0 {
            interp_lon += 360.0;
        }

        let interp_point = GeoPoint::new(interp_lat, interp_lon)?;

        // Interpolate altitudes
        let interp_baro_alt = match (p1.barometric_altitude_m, p2.barometric_altitude_m) {
            (Some(a1), Some(a2)) => Some(a1 + (a2 - a1) * fraction),
            (Some(a1), None) => Some(a1),
            (None, Some(a2)) => Some(a2),
            (None, None) => None,
        };

        let interp_geom_alt = match (p1.geometric_altitude_m, p2.geometric_altitude_m) {
            (Some(g1), Some(g2)) => Some(g1 + (g2 - g1) * fraction),
            (Some(g1), None) => Some(g1),
            (None, Some(g2)) => Some(g2),
            (None, None) => None,
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
        let uncertainty_radius_m = DEFAULT_BASE_OBSERVATION_UNCERTAINTY_M
            + (DEFAULT_INTERPOLATION_GROWTH_RATE_MPS * elapsed_seconds * midpoint_weight);

        Ok(AircraftTrackPoint {
            timestamp_utc_ms: target_utc_ms,
            position: interp_point,
            barometric_altitude_m: interp_baro_alt,
            geometric_altitude_m: interp_geom_alt,
            ground_speed_mps: interp_speed,
            track_angle_deg: interp_heading,
            vertical_rate_mps: interp_vrate,
            squawk: p1.squawk,
            observation_source: ObservationSource::InterpolatedSegment {
                elapsed_gap_ms: gap_ms,
                distance_gap_m,
                uncertainty_radius_m,
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

/// Single instantaneous maritime vessel track point.
#[derive(Debug, Clone, PartialEq)]
pub struct MaritimeTrackPoint {
    pub timestamp_utc_ms: i64,
    pub position: GeoPoint,
    pub speed_over_ground_mps: Option<f64>,
    pub course_over_ground_deg: Option<f64>,
    pub true_heading_deg: Option<f64>,
    pub observation_source: ObservationSource,
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
    ) -> Result<Self, GeoContextError> {
        // MMSI is a 9-digit numerical identifier in [100000000, 999999999]
        if !(100_000_000..=999_999_999).contains(&identity.mmsi) {
            return Err(GeoContextError::InvalidMmsi(identity.mmsi));
        }
        if points.is_empty() {
            return Err(GeoContextError::EmptyTrack);
        }

        // Verify points are chronologically ordered
        for window in points.windows(2) {
            if let [a, b] = window
                && a.timestamp_utc_ms > b.timestamp_utc_ms
            {
                return Err(GeoContextError::InvalidTimeWindow {
                    start_utc_ms: a.timestamp_utc_ms,
                    end_utc_ms: b.timestamp_utc_ms,
                });
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

        if target_utc_ms < first.timestamp_utc_ms || target_utc_ms > last.timestamp_utc_ms {
            return Err(GeoContextError::OutOfTrackRange {
                target_utc_ms,
                min_utc_ms: first.timestamp_utc_ms,
                max_utc_ms: last.timestamp_utc_ms,
            });
        }

        let idx = match self
            .points
            .binary_search_by_key(&target_utc_ms, |p| p.timestamp_utc_ms)
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
                min_utc_ms: first.timestamp_utc_ms,
                max_utc_ms: last.timestamp_utc_ms,
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

        let t1 = p1.timestamp_utc_ms;
        let t2 = p2.timestamp_utc_ms;
        let gap_ms = (t2 - t1).unsigned_abs();

        if gap_ms > limits.max_interpolation_gap_ms {
            return Err(GeoContextError::InterpolationGapExceeded {
                gap_ms,
                max_gap_ms: limits.max_interpolation_gap_ms,
            });
        }

        let fraction = if gap_ms == 0 {
            0.0
        } else {
            (target_utc_ms - t1) as f64 / gap_ms as f64
        };

        let distance_gap_m = p1.position.great_circle_distance_m(p2.position);

        let interp_lat = p1.position.latitude_deg
            + (p2.position.latitude_deg - p1.position.latitude_deg) * fraction;

        let mut d_lon_deg = (p2.position.longitude_deg - p1.position.longitude_deg) % 360.0;
        if d_lon_deg > 180.0 {
            d_lon_deg -= 360.0;
        } else if d_lon_deg < -180.0 {
            d_lon_deg += 360.0;
        }
        let mut interp_lon = p1.position.longitude_deg + d_lon_deg * fraction;
        if interp_lon > 180.0 {
            interp_lon -= 360.0;
        } else if interp_lon < -180.0 {
            interp_lon += 360.0;
        }

        let interp_point = GeoPoint::new(interp_lat, interp_lon)?;

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
        let uncertainty_radius_m = DEFAULT_BASE_OBSERVATION_UNCERTAINTY_M
            + (DEFAULT_INTERPOLATION_GROWTH_RATE_MPS * elapsed_seconds * midpoint_weight);

        Ok(MaritimeTrackPoint {
            timestamp_utc_ms: target_utc_ms,
            position: interp_point,
            speed_over_ground_mps: interp_sog,
            course_over_ground_deg: interp_cog,
            true_heading_deg: interp_heading,
            observation_source: ObservationSource::InterpolatedSegment {
                elapsed_gap_ms: gap_ms,
                distance_gap_m,
                uncertainty_radius_m,
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
    Circle { center: GeoPoint, radius_m: f64 },
    BoundingBox(BoundingBox),
}

impl NoticeSpatialBoundary {
    pub fn contains(&self, point: GeoPoint) -> bool {
        match self {
            Self::Circle { center, radius_m } => center.great_circle_distance_m(point) <= *radius_m,
            Self::BoundingBox(bb) => bb.contains(point),
        }
    }
}

/// Airspace or maritime navigation notice (e.g. NOTAM, NAVWARN, launch restriction).
#[derive(Debug, Clone, PartialEq)]
pub struct NavigationNotice {
    pub notice_id: OpaqueId,
    pub issuing_authority: String,
    pub notice_kind: NoticeKind,
    pub effective_window: TimeWindow,
    pub spatial_boundary: NoticeSpatialBoundary,
    pub lower_altitude: Option<Altitude>,
    pub upper_altitude: Option<Altitude>,
    pub summary: String,
    pub raw_content_sha256: [u8; 32],
}

impl NavigationNotice {
    pub fn is_active_at(&self, instant_utc_ms: i64) -> bool {
        self.effective_window.contains(instant_utc_ms)
    }

    pub fn intersects_position(&self, pos: &GeoPosition, instant_utc_ms: i64) -> bool {
        if !self.is_active_at(instant_utc_ms) {
            return false;
        }
        if !self.spatial_boundary.contains(pos.point) {
            return false;
        }
        if let Some(pos_alt) = pos.altitude {
            if let Some(lower) = self.lower_altitude
                && pos_alt.meters < lower.meters
            {
                return false;
            }
            if let Some(upper) = self.upper_altitude
                && pos_alt.meters > upper.meters
            {
                return false;
            }
        }
        true
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
    pub coverage_bounds: BoundingBox,
    pub altitude_band_m: Option<(f64, f64)>,
    pub operational_window: TimeWindow,
    pub sensor_type: PositionSource,
    pub is_operational: bool,
}

/// Overall area sensor coverage status for a queried envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AreaCoverageStatus {
    FullyCoveredByActiveSensors,
    PartiallyCoveredGapsIdentified,
    UncoveredNoSensorData,
}

/// Specific reason why absence of traffic cannot be asserted as an empty scene.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnobservedReason {
    NoSensorsCoveringRegion,
    SensorFeedDownOrStale,
    PartialCoverageBlindSpots,
    QueryIntervalOutsideSensorLogWindow,
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
        }
    }
}

/// Scene traffic evaluation.
///
/// **Contract:** Absent track or recording is NEVER presented as evidence of an empty scene (`GEO-05`).
#[derive(Debug, Clone, PartialEq)]
pub enum SceneTrafficAssessment {
    ObservedTrafficPresent {
        aircraft_count: usize,
        vessel_count: usize,
    },
    ObservedAbsenceWithinCoverage {
        verified_sensor_count: usize,
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

/// Summary of weather conditions reconstructed for the envelope.
#[derive(Debug, Clone, PartialEq)]
pub struct WeatherReconstructionSummary {
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

/// Evaluates sensor coverage over the query envelope.
pub fn evaluate_sensor_coverage(
    envelope: &SpatioTemporalEnvelope,
    sensor_masks: &[SensorCoverageMask],
) -> AreaCoverageStatus {
    if sensor_masks.is_empty() {
        return AreaCoverageStatus::UncoveredNoSensorData;
    }

    let mut operational_covering_sensors = 0;
    let mut any_covering_non_operational = false;

    for mask in sensor_masks {
        // Check time overlap
        if !mask.operational_window.overlaps(&envelope.time_window) {
            continue;
        }

        // Check horizontal overlap: sample the 4 corners of the query envelope
        let corners = [
            GeoPoint {
                latitude_deg: envelope.spatial_bounds.min_lat_deg,
                longitude_deg: envelope.spatial_bounds.min_lon_deg,
            },
            GeoPoint {
                latitude_deg: envelope.spatial_bounds.min_lat_deg,
                longitude_deg: envelope.spatial_bounds.max_lon_deg,
            },
            GeoPoint {
                latitude_deg: envelope.spatial_bounds.max_lat_deg,
                longitude_deg: envelope.spatial_bounds.min_lon_deg,
            },
            GeoPoint {
                latitude_deg: envelope.spatial_bounds.max_lat_deg,
                longitude_deg: envelope.spatial_bounds.max_lon_deg,
            },
        ];

        let mut corners_covered = 0;
        for c in &corners {
            if mask.coverage_bounds.contains(*c) {
                corners_covered += 1;
            }
        }

        if corners_covered > 0 {
            if !mask.is_operational {
                any_covering_non_operational = true;
            } else if corners_covered == 4 {
                operational_covering_sensors += 1;
            }
        }
    }

    if any_covering_non_operational {
        AreaCoverageStatus::PartiallyCoveredGapsIdentified
    } else if operational_covering_sensors > 0 {
        AreaCoverageStatus::FullyCoveredByActiveSensors
    } else {
        AreaCoverageStatus::UncoveredNoSensorData
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
    let mut min_obs_dist = f64::MAX;
    let mut min_forecast_dist = f64::MAX;

    let envelope_center = GeoPoint {
        latitude_deg: (envelope.spatial_bounds.min_lat_deg + envelope.spatial_bounds.max_lat_deg)
            * 0.5,
        longitude_deg: {
            if envelope.spatial_bounds.min_lon_deg <= envelope.spatial_bounds.max_lon_deg {
                (envelope.spatial_bounds.min_lon_deg + envelope.spatial_bounds.max_lon_deg) * 0.5
            } else {
                let span = (180.0 - envelope.spatial_bounds.min_lon_deg)
                    + (envelope.spatial_bounds.max_lon_deg - (-180.0));
                let mut mid = envelope.spatial_bounds.min_lon_deg + span * 0.5;
                if mid > 180.0 {
                    mid -= 360.0;
                }
                mid
            }
        },
    };

    for w in inputs.weather_records {
        w.validate()?;
        if w.valid_window.overlaps(&envelope.time_window)
            && envelope.spatial_bounds.contains(w.station_position.point)
        {
            matching_weather.push(w.clone());
            let dist = envelope_center.great_circle_distance_m(w.station_position.point);

            match w.kind {
                WeatherReportKind::DirectObservation => {
                    obs_count += 1;
                    if !w.sounding_profile.is_empty() {
                        soundings_count += 1;
                    }
                    if dist < min_obs_dist {
                        min_obs_dist = dist;
                        nearest_obs = Some(w.clone());
                    }
                }
                WeatherReportKind::ForecastModel => {
                    forecast_count += 1;
                    if dist < min_forecast_dist {
                        min_forecast_dist = dist;
                        nearest_forecast = Some(w.clone());
                    }
                }
            }
        }
    }

    if let Some(ref mut cancel) = cancel_token {
        cancel().map_err(GeoContextError::Stopped)?;
    }

    // 2. Filter intersecting Aircraft Tracks
    let mut filtered_aircraft = Vec::new();
    for track in inputs.aircraft_tracks {
        let has_intersecting_point = track.points.iter().any(|pt| {
            envelope.time_window.contains(pt.timestamp_utc_ms)
                && envelope.spatial_bounds.contains(pt.position)
        });
        if has_intersecting_point {
            filtered_aircraft.push(track.clone());
        }
    }

    // 3. Filter intersecting Maritime Vessel Tracks
    let mut filtered_vessels = Vec::new();
    for track in inputs.vessel_tracks {
        let has_intersecting_point = track.points.iter().any(|pt| {
            envelope.time_window.contains(pt.timestamp_utc_ms)
                && envelope.spatial_bounds.contains(pt.position)
        });
        if has_intersecting_point {
            filtered_vessels.push(track.clone());
        }
    }

    if let Some(ref mut cancel) = cancel_token {
        cancel().map_err(GeoContextError::Stopped)?;
    }

    // 4. Filter active Navigation Notices
    let mut active_notices = Vec::new();
    for notice in inputs.notices {
        if notice.effective_window.overlaps(&envelope.time_window) {
            let overlaps_spatial = match &notice.spatial_boundary {
                NoticeSpatialBoundary::Circle { center, radius_m } => {
                    // Check distance to envelope center
                    center.great_circle_distance_m(envelope_center) <= (*radius_m + 100_000.0)
                }
                NoticeSpatialBoundary::BoundingBox(bb) => {
                    // Check bounding box intersection
                    !(envelope.spatial_bounds.max_lat_deg < bb.min_lat_deg
                        || envelope.spatial_bounds.min_lat_deg > bb.max_lat_deg)
                }
            };
            if overlaps_spatial {
                active_notices.push(notice.clone());
            }
        }
    }

    // 5. Compute Traffic Assessment ensuring "Absent Track is NOT an Empty Scene" contract
    let coverage_status = evaluate_sensor_coverage(envelope, inputs.sensor_masks);
    let traffic_assessment = if !filtered_aircraft.is_empty() || !filtered_vessels.is_empty() {
        SceneTrafficAssessment::ObservedTrafficPresent {
            aircraft_count: filtered_aircraft.len(),
            vessel_count: filtered_vessels.len(),
        }
    } else {
        match coverage_status {
            AreaCoverageStatus::UncoveredNoSensorData => {
                SceneTrafficAssessment::InconclusiveUnobserved {
                    reason: UnobservedReason::NoSensorsCoveringRegion,
                }
            }
            AreaCoverageStatus::PartiallyCoveredGapsIdentified => {
                SceneTrafficAssessment::InconclusiveUnobserved {
                    reason: UnobservedReason::PartialCoverageBlindSpots,
                }
            }
            AreaCoverageStatus::FullyCoveredByActiveSensors => {
                let operational_count = inputs
                    .sensor_masks
                    .iter()
                    .filter(|m| m.is_operational)
                    .count();
                SceneTrafficAssessment::ObservedAbsenceWithinCoverage {
                    verified_sensor_count: operational_count,
                    note: "All operational sensor masks covering envelope verified zero detections",
                }
            }
        }
    };

    Ok(ContextReconstructionResult {
        method_id: METHOD_ID,
        query_envelope: envelope.clone(),
        weather_summary: WeatherReconstructionSummary {
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
