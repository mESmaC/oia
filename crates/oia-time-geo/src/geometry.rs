use crate::TimeGeoError;

/// IUGG R1 mean Earth radius in metres (Moritz 2000), for spherical approximations.
pub const EARTH_RADIUS_METERS: f64 = 6_371_008.8;

/// 2D geographic point on the WGS84 ellipsoid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeoPointWgs84 {
    lat_deg: f64,
    lon_deg: f64,
}

impl GeoPointWgs84 {
    pub fn new(lat_deg: f64, lon_deg: f64) -> Result<Self, TimeGeoError> {
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

    pub fn validate(&self) -> Result<(), TimeGeoError> {
        if !self.lat_deg.is_finite() || !(-90.0..=90.0).contains(&self.lat_deg) {
            return Err(TimeGeoError::InvalidCoordinates {
                lat: self.lat_deg,
                lon: self.lon_deg,
                reason: "latitude must be finite and within [-90.0, 90.0] degrees",
            });
        }
        if !self.lon_deg.is_finite() || !(-180.0..=180.0).contains(&self.lon_deg) {
            return Err(TimeGeoError::InvalidCoordinates {
                lat: self.lat_deg,
                lon: self.lon_deg,
                reason: "longitude must be finite and within [-180.0, 180.0] degrees",
            });
        }
        Ok(())
    }

    /// Great-circle spherical distance in metres using the Haversine formula.
    /// Uses the IUGG mean radius; not an ellipsoidal distance or an accuracy guarantee.
    #[must_use]
    pub fn great_circle_distance_m(&self, other: GeoPointWgs84) -> f64 {
        if self.lat_deg == other.lat_deg
            && (self.lat_deg.abs() == 90.0
                || (self.lon_deg - other.lon_deg).rem_euclid(360.0) == 0.0)
        {
            return 0.0;
        }
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

    /// Spherical initial bearing in degrees `[0.0, 360.0)`.
    /// Undefined for coincident/antipodal points or departure at a pole; callers
    /// must exclude these cases. The floating-point result is not a measured heading.
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

/// Vertical reference category. Matching categories alone do not establish a common
/// geoid, terrain surface, pressure correction or local origin; callers must pin those models.
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
    /// Height relative to a particular takeoff/home point, not a global datum.
    RelativeToTakeoff,
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
    pub fn new(meters: f64, reference: AltitudeReference) -> Result<Self, TimeGeoError> {
        if !meters.is_finite() {
            return Err(TimeGeoError::NonFiniteValue("altitude.meters"));
        }
        Ok(Self { meters, reference })
    }

    pub fn validate(&self) -> Result<(), TimeGeoError> {
        if !self.meters.is_finite() {
            return Err(TimeGeoError::NonFiniteValue("altitude.meters"));
        }
        Ok(())
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

    /// Verifies that both altitudes share the same declared reference category.
    /// Comparisons across different categories, unknown datums, or unbound takeoff
    /// origins fail closed. Other model/terrain compatibility remains a caller precondition.
    pub fn compare_in_same_reference(
        &self,
        other: &Altitude,
    ) -> Result<std::cmp::Ordering, TimeGeoError> {
        if self.reference == AltitudeReference::RelativeToTakeoff
            || self.reference == AltitudeReference::Unknown
            || other.reference == AltitudeReference::Unknown
            || self.reference != other.reference
        {
            return Err(TimeGeoError::IncompatibleAltitudeReferences {
                actual: self.reference,
                expected: other.reference,
            });
        }
        // Construction guarantees finite values; preserve numerical equality of signed zero.
        Ok(if self.meters < other.meters {
            std::cmp::Ordering::Less
        } else if self.meters > other.meters {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        })
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

/// Closed latitude extent and eastward longitude arc on WGS84 (`A04`).
///
/// Equal west/east means a meridian, not the full world. `[-180, 180]` explicitly
/// selects all longitudes; `[180, -180]` is the seam meridian. Endpoints are retained.
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
    ) -> Result<Self, TimeGeoError> {
        let bb = Self {
            south_lat_deg,
            north_lat_deg,
            west_lon_deg,
            east_lon_deg,
        };
        bb.validate()?;
        Ok(bb)
    }

    pub fn validate(&self) -> Result<(), TimeGeoError> {
        if !self.south_lat_deg.is_finite()
            || !self.north_lat_deg.is_finite()
            || !self.west_lon_deg.is_finite()
            || !self.east_lon_deg.is_finite()
        {
            return Err(TimeGeoError::NonFiniteValue("GeoBoundingBoxWgs84"));
        }
        if self.south_lat_deg > self.north_lat_deg
            || !(-90.0..=90.0).contains(&self.south_lat_deg)
            || !(-90.0..=90.0).contains(&self.north_lat_deg)
        {
            return Err(TimeGeoError::InvalidCoordinates {
                lat: self.south_lat_deg,
                lon: self.west_lon_deg,
                reason: "invalid latitude bounds (south must be <= north and within [-90.0, 90.0])",
            });
        }
        if !(-180.0..=180.0).contains(&self.west_lon_deg)
            || !(-180.0..=180.0).contains(&self.east_lon_deg)
        {
            return Err(TimeGeoError::InvalidCoordinates {
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

    /// Tests the closed geographic extent. Both seam representations are equivalent;
    /// at either pole all longitudes denote the same point. Stored coordinates are unchanged.
    #[must_use]
    pub fn contains_point(&self, point: GeoPointWgs84) -> bool {
        if point.lat_deg < self.south_lat_deg || point.lat_deg > self.north_lat_deg {
            return false;
        }
        point.lat_deg.abs() == 90.0 || self.contains_longitude(point.lon_deg)
    }

    /// True if the supplied longitude endpoints wrap across the antimeridian.
    /// `[-180, 180]` covers all longitudes; `[180, -180]` covers only the seam.
    #[must_use]
    pub fn crosses_antimeridian(&self) -> bool {
        self.west_lon_deg > self.east_lon_deg
    }

    fn longitude_span_deg(&self) -> f64 {
        let span = self.east_lon_deg - self.west_lon_deg;
        if span < 0.0 { span + 360.0 } else { span }
    }

    fn contains_longitude(&self, longitude: f64) -> bool {
        (longitude - self.west_lon_deg).rem_euclid(360.0) <= self.longitude_span_deg()
    }

    /// True if this closed geographic extent contains every point of `other`.
    #[must_use]
    pub fn contains_box(&self, other: &GeoBoundingBoxWgs84) -> bool {
        self.contains_box_with_tolerance(other, 0.0)
    }

    /// True if the closed extents share any point, including a seam edge or pole.
    #[must_use]
    pub fn overlaps_box(&self, other: &GeoBoundingBoxWgs84) -> bool {
        if self.north_lat_deg < other.south_lat_deg || self.south_lat_deg > other.north_lat_deg {
            return false;
        }
        (self.north_lat_deg == 90.0 && other.north_lat_deg == 90.0)
            || (self.south_lat_deg == -90.0 && other.south_lat_deg == -90.0)
            || self.contains_longitude(other.west_lon_deg)
            || other.contains_longitude(self.west_lon_deg)
    }

    /// Tests positive interior overlap exceeding `min_span_deg` on both axes.
    /// Edge/pole contact is not area. Negative or non-finite thresholds return false.
    /// The threshold is angular extent, not a physical area or accuracy claim.
    #[must_use]
    pub fn overlaps_interior(&self, other: &GeoBoundingBoxWgs84, min_span_deg: f64) -> bool {
        if !min_span_deg.is_finite() || min_span_deg < 0.0 {
            return false;
        }
        let south = self.south_lat_deg.max(other.south_lat_deg);
        let north = self.north_lat_deg.min(other.north_lat_deg);
        if north - south <= min_span_deg {
            return false;
        }
        let outer_span = self.longitude_span_deg();
        let inner_span = other.longitude_span_deg();
        if outer_span == 360.0 || inner_span == 360.0 {
            return outer_span.min(inner_span) > min_span_deg;
        }
        let offset = (other.west_lon_deg - self.west_lon_deg).rem_euclid(360.0);
        [offset - 360.0, offset]
            .into_iter()
            .any(|start| outer_span.min(start + inner_span) - 0.0_f64.max(start) > min_span_deg)
    }

    /// Tests containment after expanding each outer angular edge by `tol_deg`.
    /// Negative or non-finite tolerances return false. A tolerance is an explicit
    /// comparison allowance, not measurement uncertainty. Zero is exact containment.
    #[must_use]
    pub fn contains_box_with_tolerance(&self, other: &GeoBoundingBoxWgs84, tol_deg: f64) -> bool {
        if !tol_deg.is_finite() || tol_deg < 0.0 {
            return false;
        }
        if self.south_lat_deg > other.south_lat_deg + tol_deg
            || self.north_lat_deg + tol_deg < other.north_lat_deg
        {
            return false;
        }
        // A degenerate box at a pole is one point regardless of longitude.
        if other.south_lat_deg == other.north_lat_deg && other.south_lat_deg.abs() == 90.0 {
            return true;
        }
        // Avoid overflow from a large finite tolerance and handle the full circle explicitly.
        if tol_deg >= 180.0 || self.longitude_span_deg() + 2.0 * tol_deg >= 360.0 {
            return true;
        }
        let span = self.longitude_span_deg() + 2.0 * tol_deg;
        let offset = (other.west_lon_deg - self.west_lon_deg + tol_deg).rem_euclid(360.0);
        offset + other.longitude_span_deg() <= span
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

    /// Nearest point on the bounding box to a target point via coordinate-aligned planar approximation.
    ///
    /// NOTE: Clamping latitude and selecting the closest boundary longitude provides a fast
    /// planar equirectangular approximation of the nearest point. At higher latitudes, this planar
    /// approximation overestimates true spherical geodesic distance (e.g., +1.6 km at 60° latitude,
    /// +25 km at (60°, -15°)), which can yield false negatives (spurious `NoOverlap`) for circular notices
    /// near bounding box corners. True spherical point-to-polygon geodesic computation is deferred.
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
///
/// Low-level helper: inputs must be finite longitude degrees in `[-180, 180]` and
/// the crossing flag must match `west_lon > east_lon`. Prefer validated boxes.
/// Equal-distance ties choose west. This is angular, not a geodesic nearest point.
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

/// Bounded vertical band with an explicit vertical datum.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AltitudeBand {
    min_altitude: Altitude,
    max_altitude: Altitude,
}

impl AltitudeBand {
    pub fn new(min_altitude: Altitude, max_altitude: Altitude) -> Result<Self, TimeGeoError> {
        min_altitude.compare_in_same_reference(&max_altitude)?;
        if min_altitude.meters > max_altitude.meters {
            return Err(TimeGeoError::InvalidAltitudeBand {
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
    pub fn contains_altitude(&self, altitude: &Altitude) -> Result<bool, TimeGeoError> {
        let cmp_min = altitude.compare_in_same_reference(&self.min_altitude)?;
        let cmp_max = altitude.compare_in_same_reference(&self.max_altitude)?;
        Ok(!cmp_min.is_lt() && !cmp_max.is_gt())
    }

    /// Verifies if this altitude band completely covers `other`.
    pub fn contains_band(&self, other: &AltitudeBand) -> Result<bool, TimeGeoError> {
        let cmp_min = self
            .min_altitude
            .compare_in_same_reference(&other.min_altitude)?;
        let cmp_max = self
            .max_altitude
            .compare_in_same_reference(&other.max_altitude)?;
        Ok(!cmp_min.is_gt() && !cmp_max.is_lt())
    }

    /// Checks if this altitude band overlaps with `other`.
    pub fn overlaps_band(&self, other: &AltitudeBand) -> Result<bool, TimeGeoError> {
        self.min_altitude
            .compare_in_same_reference(&other.min_altitude)?;
        Ok(self.min_altitude.meters <= other.max_altitude.meters
            && other.min_altitude.meters <= self.max_altitude.meters)
    }
}
