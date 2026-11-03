//! Aviation and maritime kinematic tracking with bounded spherical great-circle interpolation (`GEO-05`, `INTEL-03`).

use crate::geo_context::provisional::{Altitude, GeoPointWgs84};
use crate::geo_context::{
    ContextLimits, GeoContextError, INTERPOLATION_METHOD_ID, spherical_slerp,
};
use oia_domain::OpaqueId;

/// Surveillance or navigation positioning source technology (`GEO-05`).
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
    /// Effective point timestamp: interpolation target time if interpolated,
    /// else receiver or transmit time.
    ///
    /// Never falls back to retrieval time.
    #[must_use]
    pub fn effective_instant_utc_ms(&self) -> Option<i64> {
        self.target_interpolation_instant_utc_ms
            .or(self.receiver_instant_utc_ms)
            .or(self.transmit_instant_utc_ms)
    }

    /// Validates field values, datums, squawk codes, and temporal ordering.
    pub fn validate(&self) -> Result<(), GeoContextError> {
        self.position.validate()?;
        match self.observation_source {
            ObservationSource::DirectObservation => {
                if self.target_interpolation_instant_utc_ms.is_some() {
                    return Err(GeoContextError::InvalidTemporalOrdering(
                        "direct observation must not set target_interpolation_instant_utc_ms",
                    ));
                }
                if self.transmit_instant_utc_ms.is_none() && self.receiver_instant_utc_ms.is_none()
                {
                    return Err(GeoContextError::InvalidTemporalOrdering(
                        "direct observation must have transmit_instant_utc_ms or receiver_instant_utc_ms",
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
                        "interpolated segment must not fabricate transmit/receiver/retrieval timestamps",
                    ));
                }
            }
        }

        if let Some(ref sq) = self.squawk_octal {
            // Standard transponder squawk: 4 octal digits [0-7]
            if sq.len() != 4 || !sq.chars().all(|c| ('0'..='7').contains(&c)) {
                return Err(GeoContextError::InvalidSquawk(sq.clone()));
            }
        }

        if let Some(spd) = self.ground_speed_mps
            && (!spd.is_finite() || spd < 0.0)
        {
            return Err(GeoContextError::NonFiniteValue("ground_speed_mps"));
        }
        if let Some(trk) = self.track_angle_deg
            && (!trk.is_finite() || !(0.0..=360.0).contains(&trk))
        {
            return Err(GeoContextError::NonFiniteValue("track_angle_deg"));
        }
        if let Some(vr) = self.vertical_rate_mps
            && !vr.is_finite()
        {
            return Err(GeoContextError::NonFiniteValue("vertical_rate_mps"));
        }

        Ok(())
    }
}

/// Encapsulated, validated aircraft flight track history.
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
    /// Constructs and strictly validates a new aircraft flight track.
    pub fn new(
        icao_hex: &str,
        callsign: Option<String>,
        aircraft_type: Option<String>,
        position_source: PositionSource,
        source_id: OpaqueId,
        points: Vec<AircraftTrackPoint>,
        limits: &ContextLimits,
    ) -> Result<Self, GeoContextError> {
        limits.validate()?;

        let uppercase_icao = icao_hex.trim().to_uppercase();
        if uppercase_icao.len() != 6 || !uppercase_icao.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(GeoContextError::InvalidIcaoHex(icao_hex.to_string()));
        }

        if points.is_empty() {
            return Err(GeoContextError::EmptyTrack);
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
            let t_p =
                p.effective_instant_utc_ms()
                    .ok_or(GeoContextError::InvalidTemporalOrdering(
                        "track point missing effective timestamp",
                    ))?;
            if i > 0 {
                let prev = &points[i - 1];
                let t_prev = prev.effective_instant_utc_ms().ok_or(
                    GeoContextError::InvalidTemporalOrdering(
                        "track point missing effective timestamp",
                    ),
                )?;
                if t_p < t_prev {
                    return Err(GeoContextError::InvalidTemporalOrdering(
                        "track points must be monotonically ordered by timestamp",
                    ));
                }
                if t_p == t_prev {
                    return Err(GeoContextError::DuplicateTimestamp(t_p));
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

    /// Validates track constraints against limits.
    pub fn validate(&self, limits: &ContextLimits) -> Result<(), GeoContextError> {
        limits.validate()?;
        if self.points.is_empty() {
            return Err(GeoContextError::EmptyTrack);
        }
        if self.points.len() > limits.max_points_per_track {
            return Err(GeoContextError::LimitExceeded {
                limit: "track.points",
                actual: self.points.len(),
                max: limits.max_points_per_track,
            });
        }
        for (i, p) in self.points.iter().enumerate() {
            p.validate()?;
            let t_p =
                p.effective_instant_utc_ms()
                    .ok_or(GeoContextError::InvalidTemporalOrdering(
                        "track point missing effective timestamp",
                    ))?;
            if i > 0 {
                let t_prev = self.points[i - 1].effective_instant_utc_ms().ok_or(
                    GeoContextError::InvalidTemporalOrdering(
                        "track point missing effective timestamp",
                    ),
                )?;
                if t_p <= t_prev {
                    return Err(GeoContextError::DuplicateTimestamp(t_p));
                }
            }
        }
        Ok(())
    }

    /// Interpolates aircraft state at `target_utc_ms` via spherical great-circle SLERP.
    pub fn interpolate_at(
        &self,
        target_utc_ms: i64,
        limits: &ContextLimits,
    ) -> Result<AircraftTrackPoint, GeoContextError> {
        limits.validate()?;
        if self.points.is_empty() {
            return Err(GeoContextError::EmptyTrack);
        }

        let first = &self.points[0];
        let last = &self.points[self.points.len() - 1];
        let min_t =
            first
                .effective_instant_utc_ms()
                .ok_or(GeoContextError::InvalidTemporalOrdering(
                    "track point missing effective timestamp",
                ))?;
        let max_t =
            last.effective_instant_utc_ms()
                .ok_or(GeoContextError::InvalidTemporalOrdering(
                    "track point missing effective timestamp",
                ))?;

        if target_utc_ms < min_t || target_utc_ms > max_t {
            return Err(GeoContextError::OutOfTrackRange {
                target_utc_ms,
                min_utc_ms: min_t,
                max_utc_ms: max_t,
            });
        }

        // Exact match
        if let Ok(idx) = self
            .points
            .binary_search_by_key(&Some(target_utc_ms), |p| p.effective_instant_utc_ms())
        {
            return Ok(self.points[idx].clone());
        }

        // Find surrounding points
        let idx2 = match self
            .points
            .binary_search_by_key(&Some(target_utc_ms), |p| p.effective_instant_utc_ms())
        {
            Ok(i) => i,
            Err(i) => i,
        };

        let p1 = &self.points[idx2 - 1];
        let p2 = &self.points[idx2];

        let t1 = p1
            .effective_instant_utc_ms()
            .ok_or(GeoContextError::InvalidTemporalOrdering(
                "track point missing effective timestamp",
            ))?;
        let t2 = p2
            .effective_instant_utc_ms()
            .ok_or(GeoContextError::InvalidTemporalOrdering(
                "track point missing effective timestamp",
            ))?;
        let gap_ms = t2.abs_diff(t1);

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
        let interp_point = spherical_slerp(p1.position, p2.position, f)?;

        // Check implied velocity against speed limit
        let distance_gap_m = p1.position.great_circle_distance_m(p2.position);
        let dt_seconds = (gap_ms as f64) / 1000.0;
        if dt_seconds > 0.0 {
            let speed_mps = distance_gap_m / dt_seconds;
            if speed_mps > limits.max_aircraft_speed_mps {
                return Err(GeoContextError::ImpliedSpeedExceedsLimit {
                    speed_mps,
                    max_speed_mps: limits.max_aircraft_speed_mps,
                });
            }
        }

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
    /// Effective point timestamp: interpolation target time if interpolated,
    /// else receiver or transmit time.
    ///
    /// Never falls back to retrieval time.
    #[must_use]
    pub fn effective_instant_utc_ms(&self) -> Option<i64> {
        self.target_interpolation_instant_utc_ms
            .or(self.receiver_instant_utc_ms)
            .or(self.transmit_instant_utc_ms)
    }

    pub fn validate(&self) -> Result<(), GeoContextError> {
        self.position.validate()?;
        match self.observation_source {
            ObservationSource::DirectObservation => {
                if self.target_interpolation_instant_utc_ms.is_some() {
                    return Err(GeoContextError::InvalidTemporalOrdering(
                        "direct vessel observation must not set target_interpolation_instant_utc_ms",
                    ));
                }
                if self.transmit_instant_utc_ms.is_none() && self.receiver_instant_utc_ms.is_none()
                {
                    return Err(GeoContextError::InvalidTemporalOrdering(
                        "direct vessel observation must have transmit_instant_utc_ms or receiver_instant_utc_ms",
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
                        "interpolated vessel segment must set target_interpolation_instant_utc_ms",
                    ));
                }
                if self.transmit_instant_utc_ms.is_some()
                    || self.receiver_instant_utc_ms.is_some()
                    || self.retrieval_instant_utc_ms.is_some()
                {
                    return Err(GeoContextError::InvalidTemporalOrdering(
                        "interpolated vessel segment must not fabricate transmit/receiver/retrieval timestamps",
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

/// Encapsulated, validated maritime vessel track history.
#[derive(Debug, Clone, PartialEq)]
pub struct MaritimeVesselTrack {
    identity: VesselIdentity,
    source_id: OpaqueId,
    points: Vec<MaritimeTrackPoint>,
}

impl MaritimeVesselTrack {
    /// Constructs and strictly validates a new maritime vessel track.
    pub fn new(
        identity: VesselIdentity,
        source_id: OpaqueId,
        points: Vec<MaritimeTrackPoint>,
        limits: &ContextLimits,
    ) -> Result<Self, GeoContextError> {
        limits.validate()?;
        identity.validate()?;

        if points.is_empty() {
            return Err(GeoContextError::EmptyTrack);
        }

        if points.len() > limits.max_points_per_track {
            return Err(GeoContextError::LimitExceeded {
                limit: "track.points",
                actual: points.len(),
                max: limits.max_points_per_track,
            });
        }

        for (i, p) in points.iter().enumerate() {
            p.validate()?;
            let t_p =
                p.effective_instant_utc_ms()
                    .ok_or(GeoContextError::InvalidTemporalOrdering(
                        "vessel track point missing effective timestamp",
                    ))?;
            if i > 0 {
                let prev = &points[i - 1];
                let t_prev = prev.effective_instant_utc_ms().ok_or(
                    GeoContextError::InvalidTemporalOrdering(
                        "vessel track point missing effective timestamp",
                    ),
                )?;
                if t_p < t_prev {
                    return Err(GeoContextError::InvalidTemporalOrdering(
                        "vessel track points must be monotonically ordered by timestamp",
                    ));
                }
                if t_p == t_prev {
                    return Err(GeoContextError::DuplicateTimestamp(t_p));
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

    /// Validates vessel track constraints against limits.
    pub fn validate(&self, limits: &ContextLimits) -> Result<(), GeoContextError> {
        limits.validate()?;
        self.identity.validate()?;
        if self.points.is_empty() {
            return Err(GeoContextError::EmptyTrack);
        }
        if self.points.len() > limits.max_points_per_track {
            return Err(GeoContextError::LimitExceeded {
                limit: "track.points",
                actual: self.points.len(),
                max: limits.max_points_per_track,
            });
        }
        for (i, p) in self.points.iter().enumerate() {
            p.validate()?;
            let t_p =
                p.effective_instant_utc_ms()
                    .ok_or(GeoContextError::InvalidTemporalOrdering(
                        "vessel track point missing effective timestamp",
                    ))?;
            if i > 0 {
                let t_prev = self.points[i - 1].effective_instant_utc_ms().ok_or(
                    GeoContextError::InvalidTemporalOrdering(
                        "vessel track point missing effective timestamp",
                    ),
                )?;
                if t_p <= t_prev {
                    return Err(GeoContextError::DuplicateTimestamp(t_p));
                }
            }
        }
        Ok(())
    }

    /// Interpolates maritime vessel position at `target_utc_ms` via spherical great-circle SLERP.
    pub fn interpolate_at(
        &self,
        target_utc_ms: i64,
        limits: &ContextLimits,
    ) -> Result<MaritimeTrackPoint, GeoContextError> {
        limits.validate()?;
        if self.points.is_empty() {
            return Err(GeoContextError::EmptyTrack);
        }

        let first = &self.points[0];
        let last = &self.points[self.points.len() - 1];
        let min_t =
            first
                .effective_instant_utc_ms()
                .ok_or(GeoContextError::InvalidTemporalOrdering(
                    "vessel track point missing effective timestamp",
                ))?;
        let max_t =
            last.effective_instant_utc_ms()
                .ok_or(GeoContextError::InvalidTemporalOrdering(
                    "vessel track point missing effective timestamp",
                ))?;

        if target_utc_ms < min_t || target_utc_ms > max_t {
            return Err(GeoContextError::OutOfTrackRange {
                target_utc_ms,
                min_utc_ms: min_t,
                max_utc_ms: max_t,
            });
        }

        if let Ok(idx) = self
            .points
            .binary_search_by_key(&Some(target_utc_ms), |p| p.effective_instant_utc_ms())
        {
            return Ok(self.points[idx].clone());
        }

        let idx2 = match self
            .points
            .binary_search_by_key(&Some(target_utc_ms), |p| p.effective_instant_utc_ms())
        {
            Ok(i) => i,
            Err(i) => i,
        };

        let p1 = &self.points[idx2 - 1];
        let p2 = &self.points[idx2];

        let t1 = p1
            .effective_instant_utc_ms()
            .ok_or(GeoContextError::InvalidTemporalOrdering(
                "vessel track point missing effective timestamp",
            ))?;
        let t2 = p2
            .effective_instant_utc_ms()
            .ok_or(GeoContextError::InvalidTemporalOrdering(
                "vessel track point missing effective timestamp",
            ))?;
        let gap_ms = t2.abs_diff(t1);

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

        let interp_point = spherical_slerp(p1.position, p2.position, f)?;

        // Check implied velocity against speed limit
        let distance_gap_m = p1.position.great_circle_distance_m(p2.position);
        let dt_seconds = (gap_ms as f64) / 1000.0;
        if dt_seconds > 0.0 {
            let speed_mps = distance_gap_m / dt_seconds;
            if speed_mps > limits.max_vessel_speed_mps {
                return Err(GeoContextError::ImpliedSpeedExceedsLimit {
                    speed_mps,
                    max_speed_mps: limits.max_vessel_speed_mps,
                });
            }
        }

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
