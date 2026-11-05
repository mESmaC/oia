//! Spatio-temporal geographic context reconstruction kernel (`GEO-02`, `GEO-05`, `INTEL-03`, `A04`).
//!
//! Reconstructs 4D geographic context (weather observations vs. forecast models, upper-air soundings,
//! aircraft flight tracks, maritime AIS vessel tracks, and airspace/maritime navigation notices) for a
//! bounded spatio-temporal query envelope with antimeridian (180° dateline) crossing support.
//!
//! ### Core Contract: Absence within Verified Coverage (§2; `GEO-05`, `A04`)
//! A region with zero reported tracks is **NEVER** presented as an empty sky or clear sea unless the
//! volume is verified completely covered by active, fresh surveillance sensors (`SensorCoverageMask`).
//! Uncovered, partially covered, stale, or out-of-band volumes fail closed to
//! `SceneTrafficAssessment::InconclusiveUnobserved`.

pub mod coverage;
pub mod notices;
pub mod provisional;
pub mod tracks;
pub mod weather;

pub use coverage::*;
pub use notices::*;
pub use tracks::*;
pub use weather::*;

use oia_time_geo::{
    Altitude, AltitudeBand, AltitudeReference, GeoBoundingBoxWgs84, GeoPointWgs84, GeoPosition,
    TimeWindow,
};
use std::fmt;

/// Method identifier for geographic context reconstruction.
pub const METHOD_ID: &str = "oia-geo-context-reconstruction-v1";

/// Kinematic interpolation method identifier for spherical great-circle interpolation.
pub const INTERPOLATION_METHOD_ID: &str = "bounded-great-circle-spherical-v1";

/// Mean Earth radius in metres (IUGG R1 spherical mean approximation; Moritz 2000).
pub use oia_time_geo::EARTH_RADIUS_METERS;

// Maintain single definition across crate
pub use crate::multilingual::StopReason;

/// Cooperative cancellation token hook.
pub type CancelToken<'a> = &'a mut (dyn FnMut() -> Result<(), StopReason> + 'a);

/// Operational and safety bounding limits for geographic context reconstruction (`AGENTS.md` §5).
#[derive(Debug, Clone, PartialEq)]
pub struct ContextLimits {
    pub max_points_per_track: usize,
    pub max_tracks: usize,
    pub max_notices: usize,
    pub max_weather_records: usize,
    pub max_interpolation_gap_ms: u64,
    /// Base observation uncertainty in metres (default 15.0m nominal baseline uncertainty,
    /// derived from RTCA DO-260B / FAA AC 20-165B Table 2-2 NACp >= 9 95% horizontal accuracy bound of < 30m;
    /// note 14 CFR 91.227(c)(1)(i) operational minimum for ADS-B Out is NACp >= 8 / < 92.6m).
    pub base_observation_uncertainty_m: f64,
    /// Kinetic divergence growth rate in m/s (default 2.0 m/s operational heuristic representing
    /// transport aircraft straight-line inertial wind-drift uncertainty bound during brief dropouts).
    /// Note: maritime vessels currently share this default parameter unless customized in ContextLimits.
    pub interpolation_growth_rate_mps: f64,
    /// Maximum sensor feed staleness tolerance in milliseconds (default 300,000 ms = 5 minutes).
    pub max_sensor_staleness_ms: u64,
    /// Maximum plausible aircraft speed in m/s (default 1000.0 m/s ~ Mach 3; rejects impossible kinematic jumps).
    pub max_aircraft_speed_mps: f64,
    /// Maximum plausible maritime vessel speed in m/s (default 50.0 m/s ~ 97 knots; rejects impossible jumps).
    pub max_vessel_speed_mps: f64,
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
            max_aircraft_speed_mps: 1000.0,
            max_vessel_speed_mps: 50.0,
        }
    }
}

impl ContextLimits {
    /// Validates numeric sanity of operational limits.
    pub fn validate(&self) -> Result<(), GeoContextError> {
        if !self.base_observation_uncertainty_m.is_finite()
            || self.base_observation_uncertainty_m < 0.0
        {
            return Err(GeoContextError::InvalidLimitValue {
                field: "ContextLimits.base_observation_uncertainty_m",
                reason: "must be finite and >= 0.0",
            });
        }
        if !self.interpolation_growth_rate_mps.is_finite()
            || self.interpolation_growth_rate_mps < 0.0
        {
            return Err(GeoContextError::InvalidLimitValue {
                field: "ContextLimits.interpolation_growth_rate_mps",
                reason: "must be finite and >= 0.0",
            });
        }
        if self.max_interpolation_gap_ms == 0 {
            return Err(GeoContextError::InvalidLimitValue {
                field: "ContextLimits.max_interpolation_gap_ms",
                reason: "must be > 0",
            });
        }
        if self.max_sensor_staleness_ms == 0 || self.max_sensor_staleness_ms > i64::MAX as u64 {
            return Err(GeoContextError::InvalidLimitValue {
                field: "ContextLimits.max_sensor_staleness_ms",
                reason: "must be > 0 and <= i64::MAX",
            });
        }
        if !self.max_aircraft_speed_mps.is_finite() || self.max_aircraft_speed_mps <= 0.0 {
            return Err(GeoContextError::InvalidLimitValue {
                field: "ContextLimits.max_aircraft_speed_mps",
                reason: "must be finite and > 0.0",
            });
        }
        if !self.max_vessel_speed_mps.is_finite() || self.max_vessel_speed_mps <= 0.0 {
            return Err(GeoContextError::InvalidLimitValue {
                field: "ContextLimits.max_vessel_speed_mps",
                reason: "must be finite and > 0.0",
            });
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
    InvalidLimitValue {
        field: &'static str,
        reason: &'static str,
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
    ImpliedSpeedExceedsLimit {
        speed_mps: f64,
        max_speed_mps: f64,
    },
    AntipodalPoints,
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
            Self::InvalidLimitValue { field, reason } => {
                write!(f, "invalid limit value for {field}: {reason}")
            }
            Self::NonFiniteValue(field) => write!(f, "non-finite floating point value in {field}"),
            Self::LimitExceeded { limit, actual, max } => {
                write!(f, "{limit} exceeded: actual {actual}, max {max}")
            }
            Self::InterpolationGapExceeded { gap_ms, max_gap_ms } => {
                write!(
                    f,
                    "interpolation gap {gap_ms} ms exceeds limit of {max_gap_ms} ms"
                )
            }
            Self::ImpliedSpeedExceedsLimit {
                speed_mps,
                max_speed_mps,
            } => {
                write!(
                    f,
                    "implied speed {speed_mps:.1} m/s exceeds limit of {max_speed_mps:.1} m/s"
                )
            }
            Self::AntipodalPoints => {
                write!(f, "cannot interpolate between exactly antipodal points")
            }
            Self::OutOfTrackRange {
                target_utc_ms,
                min_utc_ms,
                max_utc_ms,
            } => {
                write!(
                    f,
                    "target time {target_utc_ms} falls outside track range [{min_utc_ms}, {max_utc_ms}]"
                )
            }
            Self::EmptyTrack => write!(f, "track contains zero points"),
            Self::DuplicateTimestamp(ts) => {
                write!(f, "duplicate or non-increasing timestamp in track: {ts}")
            }
            Self::InvalidTemporalOrdering(reason) => {
                write!(f, "invalid temporal ordering: {reason}")
            }
            Self::InvalidIcaoHex(s) => write!(f, "invalid 24-bit ICAO hexadecimal address: '{s}'"),
            Self::InvalidMmsi(mmsi) => write!(f, "invalid 9-digit MMSI: {mmsi}"),
            Self::InvalidImoNumber(imo) => {
                write!(f, "invalid IMO ship number check digit checksum: {imo}")
            }
            Self::InvalidSquawk(s) => write!(f, "invalid transponder squawk: '{s}'"),
            Self::InvalidWeatherTimes(reason) => write!(f, "invalid weather times: {reason}"),
            Self::IncompatibleAltitudeReferences { actual, expected } => {
                write!(
                    f,
                    "incompatible altitude datums: actual {actual:?}, expected {expected:?}"
                )
            }
            Self::Stopped(reason) => write!(f, "processing stopped: {reason:?}"),
        }
    }
}

impl std::error::Error for GeoContextError {}

impl From<oia_time_geo::TimeGeoError> for GeoContextError {
    fn from(error: oia_time_geo::TimeGeoError) -> Self {
        use oia_time_geo::TimeGeoError;
        match error {
            TimeGeoError::InvalidCoordinates { lat, lon, reason } => {
                Self::InvalidCoordinates { lat, lon, reason }
            }
            TimeGeoError::InvalidTimeWindow {
                start_utc_ms,
                end_utc_ms,
            } => Self::InvalidTimeWindow {
                start_utc_ms,
                end_utc_ms,
            },
            TimeGeoError::InvalidAltitudeBand {
                min_meters,
                max_meters,
            } => Self::InvalidAltitudeBand {
                min_meters,
                max_meters,
            },
            TimeGeoError::NonFiniteValue(field) => Self::NonFiniteValue(field),
            TimeGeoError::IncompatibleAltitudeReferences { actual, expected } => {
                Self::IncompatibleAltitudeReferences { actual, expected }
            }
            TimeGeoError::InvalidApproximateBounds {
                earliest_utc_ms,
                latest_utc_ms,
            } => Self::InvalidTimeWindow {
                start_utc_ms: earliest_utc_ms,
                end_utc_ms: latest_utc_ms,
            },
            TimeGeoError::InvalidCalendarDate { .. } => {
                Self::InvalidTemporalOrdering("invalid proleptic Gregorian calendar date")
            }
            TimeGeoError::InvalidRfc3339Timestamp { .. } => {
                Self::InvalidTemporalOrdering("invalid RFC 3339 UTC timestamp")
            }
            TimeGeoError::InvalidUtcOffsetMinutes(_) => {
                Self::InvalidTemporalOrdering("UTC offset minutes out of valid range")
            }
            TimeGeoError::EmptyField(_) => {
                Self::InvalidTemporalOrdering("temporal metadata field must not be empty")
            }
            TimeGeoError::PreLeapSecondEpoch(_) => Self::InvalidTemporalOrdering(
                "timestamp precedes the 1972-01-01T00:00:00Z IERS leap-second epoch",
            ),
            TimeGeoError::LeapTableExpired { .. } => {
                Self::InvalidTemporalOrdering("leap-second table has expired for timestamp")
            }
            TimeGeoError::UnsynchronizedClock => Self::InvalidTemporalOrdering(
                "source clock is unsynchronized; receipt time cannot substitute for capture time",
            ),
            TimeGeoError::ClockAlignmentOutOfWindow { .. } => Self::InvalidTemporalOrdering(
                "source timestamp falls outside clock alignment calibration window",
            ),
            TimeGeoError::TimestampArithmeticOverflow => Self::InvalidTemporalOrdering(
                "timestamp arithmetic overflowed i64 millisecond bounds",
            ),
        }
    }
}

/// Spherical linear interpolation (SLERP) along the great circle between two points on the WGS84 unit sphere.
///
/// Converts geodetic latitude/longitude to 3D Cartesian coordinates on the unit sphere,
/// computes the shortest-arc spherical angle $\theta$, and interpolates via:
/// $$\mathbf{v}(f) = \frac{\sin((1-f)\theta)}{\sin\theta} \mathbf{v}_1 + \frac{\sin(f\theta)}{\sin\theta} \mathbf{v}_2$$
///
/// This eliminates planar projection distortion across all latitudes and across the 180° antimeridian.
pub fn spherical_slerp(
    p1: GeoPointWgs84,
    p2: GeoPointWgs84,
    f: f64,
) -> Result<GeoPointWgs84, GeoContextError> {
    if !f.is_finite() {
        return Err(GeoContextError::NonFiniteValue("slerp fraction"));
    }
    p1.validate()?;
    p2.validate()?;

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
    if dot <= -1.0 + 1e-12 {
        return Err(GeoContextError::AntipodalPoints);
    }
    let theta = dot.acos();

    if theta < 1e-7 {
        return Ok(p1);
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

    Ok(GeoPointWgs84::new(lat_deg, lon_deg)?)
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

    pub fn validate(&self) -> Result<(), GeoContextError> {
        self.spatial_bounds.validate()?;
        self.time_window.validate()?;
        if let Some(ref band) = self.altitude_band {
            band.min_altitude()
                .compare_in_same_reference(&band.max_altitude())?;
        }
        Ok(())
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
                Some(ref alt) => Ok(band.contains_altitude(alt)?),
                None => Ok(false),
            }
        } else {
            Ok(true)
        }
    }
}

/// Raw input data for geographic context reconstruction.
#[derive(Debug)]
pub struct ContextReconstructionInputs<'a> {
    pub weather_records: &'a [WeatherRecord],
    pub aircraft_tracks: &'a [AircraftTrack],
    pub vessel_tracks: &'a [MaritimeVesselTrack],
    pub notices: &'a [NavigationNotice],
    pub sensor_masks: &'a [SensorCoverageMask],
}

/// Basis for vessel vertical attribution in reconstructed context (`GEO-05`, `INTEL-03`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VesselAltitudeBasis {
    /// Assumed nominal sea surface at 0.0 m in Orthometric MSL / AGL datums.
    /// Note: Astronomical tide and dynamic ocean topography / sea-surface height are ignored.
    AssumedSeaSurface,
}

/// Unified reconstructed geographic context result.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextReconstructionResult {
    pub method_id: &'static str,
    pub query_envelope: SpatioTemporalEnvelope,
    pub weather_summary: WeatherReconstructionSummary,
    pub aircraft_tracks: Vec<AircraftTrack>,
    pub vessel_tracks: Vec<MaritimeVesselTrack>,
    pub vessel_altitude_basis: Option<VesselAltitudeBasis>,
    pub active_notices: Vec<NavigationNotice>,
    pub notices_with_vertical_overlap_unknown: Vec<NavigationNotice>,
    pub notice_summary: NoticeReconstructionSummary,
    pub traffic_assessment: SceneTrafficAssessment,
    pub reconstruction_timestamp_utc_ms: i64,
}

/// Reconstructs geographic, atmospheric, aviation, maritime, and notice context
/// for a target spatio-temporal query envelope (`GEO-02`, `GEO-05`, `INTEL-03`, `A04`).
pub fn reconstruct_geographic_context(
    envelope: &SpatioTemporalEnvelope,
    inputs: &ContextReconstructionInputs<'_>,
    limits: &ContextLimits,
    reconstruction_instant_utc_ms: i64,
    mut cancel_token: Option<CancelToken<'_>>,
) -> Result<ContextReconstructionResult, GeoContextError> {
    limits.validate()?;
    envelope.validate()?;

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
            let cost = dist_m + 10.0 * time_diff_s;

            match w.kind {
                WeatherReportKind::DirectObservation => {
                    obs_count += 1;
                    if !w.sounding_profile.is_empty() {
                        soundings_count += 1;
                    }
                    if cost < min_obs_cost {
                        min_obs_cost = cost;
                        nearest_obs = Some(w.clone());
                    }
                }
                WeatherReportKind::ForecastModel => {
                    forecast_count += 1;
                    if cost < min_forecast_cost {
                        min_forecast_cost = cost;
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
    let mut filtered_aircraft = Vec::new();
    let mut observed_aircraft_count = 0;
    let mut has_unresolved_altitude_observation = false;

    for track in inputs.aircraft_tracks {
        track.validate(limits)?;
        let mut track_matches = false;
        let mut has_direct_obs_in_envelope = false;

        for pt in track.points() {
            pt.validate()?;
            let Some(ts) = pt.effective_instant_utc_ms() else {
                continue;
            };
            if !envelope.time_window.contains(ts) {
                continue;
            }
            if !envelope.spatial_bounds.contains_point(pt.position) {
                continue;
            }

            // Horizontal & temporal match. Check vertical band if envelope specifies one.
            if let Some(ref env_band) = envelope.altitude_band {
                let mut matched_datum_alt = None;
                let mut has_any_alt = false;

                if let Some(ref geom) = pt.geometric_altitude {
                    has_any_alt = true;
                    if geom.reference() == env_band.reference() {
                        matched_datum_alt = Some(geom);
                    }
                }
                if let Some(ref baro) = pt.barometric_altitude {
                    has_any_alt = true;
                    if matched_datum_alt.is_none() && baro.reference() == env_band.reference() {
                        matched_datum_alt = Some(baro);
                    }
                }

                if let Some(alt) = matched_datum_alt {
                    match env_band.contains_altitude(alt) {
                        Ok(true) => {
                            track_matches = true;
                            if pt.observation_source == ObservationSource::DirectObservation {
                                has_direct_obs_in_envelope = true;
                            }
                        }
                        Ok(false) => {}
                        Err(_) => {
                            // Incompatible vertical datum: unresolved altitude inside 2D envelope
                            if pt.observation_source == ObservationSource::DirectObservation {
                                has_unresolved_altitude_observation = true;
                            }
                        }
                    }
                } else if has_any_alt {
                    // Has altitude observation(s), but none match the envelope's vertical datum
                    if pt.observation_source == ObservationSource::DirectObservation {
                        has_unresolved_altitude_observation = true;
                    }
                } else {
                    // Point inside 2D box & time window lacks altitude: cannot declare absence!
                    if pt.observation_source == ObservationSource::DirectObservation {
                        has_unresolved_altitude_observation = true;
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

    if let Some(ref mut cancel) = cancel_token {
        cancel().map_err(GeoContextError::Stopped)?;
    }

    // 3. Filter intersecting Maritime Vessel Tracks
    // Surface vessels operate at sea level (the physical surface).
    // Surface level can only be expressed in surface-compatible vertical datums:
    // OrthometricMsl or AboveGroundLevel. Against ellipsoidal or barometric bands,
    // the sea surface height is not 0 (due to local geoid undulation or pressure variation).
    let mut filtered_vessels = Vec::new();
    let mut observed_vessel_count = 0;
    let mut assumed_sea_surface_evaluated = false;

    for track in inputs.vessel_tracks {
        track.validate(limits)?;
        let mut track_matches = false;
        let mut has_direct_obs_in_envelope = false;

        for pt in track.points() {
            pt.validate()?;
            let Some(ts) = pt.effective_instant_utc_ms() else {
                continue;
            };
            if !envelope.time_window.contains(ts) {
                continue;
            }
            if !envelope.spatial_bounds.contains_point(pt.position) {
                continue;
            }

            if let Some(ref env_band) = envelope.altitude_band {
                match env_band.reference() {
                    AltitudeReference::OrthometricMsl | AltitudeReference::AboveGroundLevel => {
                        assumed_sea_surface_evaluated = true;
                        let sea_level_alt = Altitude::new(0.0, env_band.reference())?;
                        match env_band.contains_altitude(&sea_level_alt) {
                            Ok(true) => {
                                track_matches = true;
                                if pt.observation_source == ObservationSource::DirectObservation {
                                    has_direct_obs_in_envelope = true;
                                }
                            }
                            Ok(false) => {
                                // Band does not include surface level
                            }
                            Err(_) => {
                                if pt.observation_source == ObservationSource::DirectObservation {
                                    has_unresolved_altitude_observation = true;
                                }
                            }
                        }
                    }
                    _ => {
                        // Ellipsoidal or barometric datum cannot express sea surface without
                        // geoid or atmospheric profiles; vertical overlap is not comparable.
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
    let mut vertical_overlap_unknown_notices = Vec::new();

    for notice in inputs.notices {
        notice.validate()?;
        match notice.overlaps_envelope(envelope) {
            NoticeEnvelopeOverlap::DefiniteOverlap => {
                active_notices.push(notice.clone());
            }
            NoticeEnvelopeOverlap::VerticalOverlapUnknown => {
                vertical_overlap_unknown_notices.push(notice.clone());
            }
            NoticeEnvelopeOverlap::NoOverlap => {}
        }
    }

    let notice_summary = NoticeReconstructionSummary {
        active_matching_notices_count: active_notices.len(),
        vertical_overlap_unknown_count: vertical_overlap_unknown_notices.len(),
    };

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

    let vessel_altitude_basis = if assumed_sea_surface_evaluated {
        Some(VesselAltitudeBasis::AssumedSeaSurface)
    } else {
        None
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
        vessel_altitude_basis,
        active_notices,
        notices_with_vertical_overlap_unknown: vertical_overlap_unknown_notices,
        notice_summary,
        traffic_assessment,
        reconstruction_timestamp_utc_ms: reconstruction_instant_utc_ms,
    })
}
