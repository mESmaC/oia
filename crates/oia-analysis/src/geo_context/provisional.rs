//! Provisional geometry and temporal primitives scoped to `oia-analysis` pending completion of `#12`.
//!
//! ### Provisional Architecture Notice (`#12`, `#24`)
//! The geometric and time primitives in this module (`GeoPointWgs84`, `GeoBoundingBoxWgs84`,
//! `AltitudeBand`, `AltitudeReference`, `TimeWindow`) are provisional implementations local
//! to `oia-analysis::geo_context::provisional`.
//!
//! Per architecture §1 rule 5 and issue #12, these types will be replaced by the shared
//! domain models in `oia-time-geo` once merged. Callers should access them explicitly through
//! `oia_analysis::geo_context::provisional::*` to ease downstream migration.

use crate::geo_context::{EARTH_RADIUS_METERS, GeoContextError};

/// 2D geographic point on the WGS84 ellipsoid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeoPointWgs84 {
    pub(crate) lat_deg: f64,
    pub(crate) lon_deg: f64,
}

impl GeoPointWgs84 {
    pub fn new(lat_deg: f64, lon_deg: f64) -> Result<Self, GeoContextError> {
        let pt = Self { lat_deg, lon_deg };
        pt.validate()?;
        Ok(pt)
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

    pub fn validate(&self) -> Result<(), GeoContextError> {
        if !self.lat_deg.is_finite() || !(-90.0..=90.0).contains(&self.lat_deg) {
            return Err(GeoContextError::InvalidCoordinates {
                lat: self.lat_deg,
                lon: self.lon_deg,
                reason: "latitude must be finite and within [-90.0, 90.0] degrees",
            });
        }
        if !self.lon_deg.is_finite() || !(-180.0..=180.0).contains(&self.lon_deg) {
            return Err(GeoContextError::InvalidCoordinates {
                lat: self.lat_deg,
                lon: self.lon_deg,
                reason: "longitude must be finite and within [-180.0, 180.0] degrees",
            });
        }
        Ok(())
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

        let a = (d_lat * 0.5).sin().powi(2) + lat1.cos() * lat2.cos() * (d_lon * 0.5).sin().powi(2);
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
        let bb = Self {
            south_lat_deg,
            north_lat_deg,
            west_lon_deg,
            east_lon_deg,
        };
        bb.validate()?;
        Ok(bb)
    }

    pub fn validate(&self) -> Result<(), GeoContextError> {
        if !self.south_lat_deg.is_finite()
            || !self.north_lat_deg.is_finite()
            || !self.west_lon_deg.is_finite()
            || !self.east_lon_deg.is_finite()
        {
            return Err(GeoContextError::NonFiniteValue("GeoBoundingBoxWgs84"));
        }
        if self.south_lat_deg > self.north_lat_deg
            || !(-90.0..=90.0).contains(&self.south_lat_deg)
            || !(-90.0..=90.0).contains(&self.north_lat_deg)
        {
            return Err(GeoContextError::InvalidCoordinates {
                lat: self.south_lat_deg,
                lon: self.west_lon_deg,
                reason: "invalid latitude bounds (south must be <= north and within [-90.0, 90.0])",
            });
        }
        if !(-180.0..=180.0).contains(&self.west_lon_deg)
            || !(-180.0..=180.0).contains(&self.east_lon_deg)
        {
            return Err(GeoContextError::InvalidCoordinates {
                lat: self.south_lat_deg,
                lon: self.west_lon_deg,
                reason: "longitude bounds must be in [-180.0, 180.0]",
            });
        }
        Ok(())
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
        if other.south_lat_deg < self.south_lat_deg || other.north_lat_deg > self.north_lat_deg {
            return false;
        }
        match (self.crosses_antimeridian(), other.crosses_antimeridian()) {
            (false, false) => {
                self.west_lon_deg <= other.west_lon_deg && other.east_lon_deg <= self.east_lon_deg
            }
            (true, false) => {
                (self.west_lon_deg <= other.west_lon_deg && other.east_lon_deg <= 180.0)
                    || (-180.0 <= other.west_lon_deg && other.east_lon_deg <= self.east_lon_deg)
            }
            (true, true) => {
                self.west_lon_deg <= other.west_lon_deg && other.east_lon_deg <= self.east_lon_deg
            }
            (false, true) => false,
        }
    }

    /// True if `self` overlaps with `other` in latitude and longitude.
    #[must_use]
    pub fn overlaps_box(&self, other: &GeoBoundingBoxWgs84) -> bool {
        if self.north_lat_deg < other.south_lat_deg || self.south_lat_deg > other.north_lat_deg {
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
    /// coordinate-aligned rectangular approximation of the nearest point on the sphere for conservative
    /// spatial overlap checks. At high latitudes, this planar clamp may overestimate geodesic distance
    /// (e.g. by ~1.6 km at 60° latitude) rather than computing the exact spherical geodesic point.
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
        let tw = Self {
            start_utc_ms,
            end_utc_ms,
        };
        tw.validate()?;
        Ok(tw)
    }

    pub fn validate(&self) -> Result<(), GeoContextError> {
        if self.start_utc_ms >= self.end_utc_ms {
            return Err(GeoContextError::InvalidTimeWindow {
                start_utc_ms: self.start_utc_ms,
                end_utc_ms: self.end_utc_ms,
            });
        }
        Ok(())
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
    pub fn min_meters(&self) -> f64 {
        self.min_altitude.meters
    }

    #[inline]
    #[must_use]
    pub fn max_meters(&self) -> f64 {
        self.max_altitude.meters
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
