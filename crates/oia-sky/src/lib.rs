//! Provider-independent observer sky geometry.
//!
//! Given a geocentric apparent place from any ephemeris (true equator and
//! equinox of date), an observer on the WGS84 ellipsoid, and caller-supplied
//! UT1 and TT instants, this crate computes:
//!
//! 1. Earth rotation: GMST (IAU 1982) plus the equation of the equinoxes from a
//!    truncated IAU 1980 nutation series, giving GAST.
//! 2. The topocentric apparent place: parallax from the observer's geocentric
//!    position, and first-order diurnal aberration.
//! 3. Airless horizontal coordinates relative to the ellipsoid normal
//!    (geodetic vertical), with azimuth measured from north through east.
//! 4. Optionally, refraction from Saemundsson's formula under caller-stated
//!    pressure and temperature. Below the formula's domain, no refracted value
//!    is produced.
//!
//! It does not select or evaluate an ephemeris, convert UTC to UT1 or TT, or
//! apply polar motion. See `docs/implementation/sky-observer-geometry.md` for
//! the accuracy budget and the Horizons reference comparison.

use std::error::Error;
use std::fmt;

/// Identifies the combination of models applied by this crate. Record it with
/// any result so the method can be reproduced or reassessed.
pub const MODEL_ID: &str = "oia-sky-observer-v1: GMST IAU1982; nutation IAU1980 truncated (4 terms, |dpsi| err <= 0.5 arcsec); WGS84; first-order diurnal aberration; Saemundsson refraction";

/// Earliest supported UT1 Julian date (1900-01-01T00:00 UT1).
pub const MIN_JD_UT1: f64 = 2_415_020.5;
/// Latest supported UT1 Julian date (2100-01-01T00:00 UT1).
pub const MAX_JD_UT1: f64 = 2_488_069.5;
/// Accepted range of TT − UT1 in seconds. Historical and predicted values for
/// 1900–2100 lie inside it; a value outside usually means UTC or UT1 was passed
/// as TT, or the arguments were swapped.
pub const TT_MINUS_UT1_RANGE_S: (f64, f64) = (-10.0, 400.0);
/// Accepted ellipsoidal height range in metres.
pub const HEIGHT_RANGE_M: (f64, f64) = (-12_000.0, 100_000.0);
/// Smallest accepted geocentric target distance in kilometres. Targets closer
/// than this would be at or below the Earth's surface.
pub const MIN_TARGET_DISTANCE_KM: f64 = 6_400.0;
/// Lowest airless (true) elevation for which refraction is modelled, in degrees.
pub const REFRACTION_MIN_ELEVATION_DEG: f64 = -1.0;
/// Accepted pressure range for refraction, in hectopascals.
pub const PRESSURE_RANGE_HPA: (f64, f64) = (0.0, 1_100.0);
/// Accepted temperature range for refraction, in degrees Celsius.
pub const TEMPERATURE_RANGE_C: (f64, f64) = (-90.0, 60.0);

const J2000_JD: f64 = 2_451_545.0;
const DAYS_PER_CENTURY: f64 = 36_525.0;
const SECONDS_PER_DAY: f64 = 86_400.0;
const ARCSEC_PER_DEG: f64 = 3_600.0;
const WGS84_A_M: f64 = 6_378_137.0;
const WGS84_F: f64 = 1.0 / 298.257_223_563;
/// Nominal mean angular velocity of the Earth (IERS), rad/s.
const EARTH_ROTATION_RAD_S: f64 = 7.292_115_855_3e-5;
const SPEED_OF_LIGHT_M_S: f64 = 299_792_458.0;

/// The input field a validation error refers to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Field {
    JdUt1,
    JdTt,
    TtMinusUt1,
    Latitude,
    Longitude,
    Height,
    RightAscension,
    Declination,
    Distance,
    Pressure,
    Temperature,
    Elevation,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SkyError {
    /// The value is NaN or infinite.
    NonFinite(Field),
    /// The value is finite but outside the supported range.
    OutOfRange {
        field: Field,
        value: f64,
        min: f64,
        max: f64,
    },
    /// The target and observer positions nearly coincide, so the topocentric
    /// direction is undefined.
    DegenerateGeometry,
}

impl fmt::Display for SkyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinite(field) => write!(f, "{field:?} is not a finite number"),
            Self::OutOfRange {
                field,
                value,
                min,
                max,
            } => write!(
                f,
                "{field:?} = {value} is outside the supported range [{min}, {max}]"
            ),
            Self::DegenerateGeometry => {
                f.write_str("target and observer positions coincide; direction undefined")
            }
        }
    }
}

impl Error for SkyError {}

fn finite(field: Field, value: f64) -> Result<f64, SkyError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(SkyError::NonFinite(field))
    }
}

fn in_range(field: Field, value: f64, min: f64, max: f64) -> Result<f64, SkyError> {
    let value = finite(field, value)?;
    if (min..=max).contains(&value) {
        Ok(value)
    } else {
        Err(SkyError::OutOfRange {
            field,
            value,
            min,
            max,
        })
    }
}

/// An instant expressed in both UT1 (for Earth rotation) and TT (for the
/// nutation arguments), as Julian dates.
///
/// This crate does not derive either scale from UTC. UT1 needs measured Earth
/// orientation data; each 0.1 s of UT1 error rotates the sky by about 1.5″.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Instant {
    jd_ut1: f64,
    jd_tt: f64,
}

impl Instant {
    pub fn new(jd_ut1: f64, jd_tt: f64) -> Result<Self, SkyError> {
        let jd_ut1 = in_range(Field::JdUt1, jd_ut1, MIN_JD_UT1, MAX_JD_UT1)?;
        let jd_tt = finite(Field::JdTt, jd_tt)?;
        let (min, max) = TT_MINUS_UT1_RANGE_S;
        in_range(
            Field::TtMinusUt1,
            (jd_tt - jd_ut1) * SECONDS_PER_DAY,
            min,
            max,
        )?;
        Ok(Self { jd_ut1, jd_tt })
    }

    pub fn jd_ut1(self) -> f64 {
        self.jd_ut1
    }

    pub fn jd_tt(self) -> f64 {
        self.jd_tt
    }
}

/// An observer on the WGS84 ellipsoid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeodeticSite {
    latitude_deg: f64,
    longitude_east_deg: f64,
    height_m: f64,
}

impl GeodeticSite {
    /// `latitude_deg` is geodetic, in [−90, 90]. `longitude_east_deg` is
    /// east-positive and may be given in [−180, 180] or [0, 360]; it is stored
    /// normalised to [−180, 180). `height_m` is ellipsoidal height, not height
    /// above mean sea level; the difference can be tens of metres.
    pub fn new(
        latitude_deg: f64,
        longitude_east_deg: f64,
        height_m: f64,
    ) -> Result<Self, SkyError> {
        let latitude_deg = in_range(Field::Latitude, latitude_deg, -90.0, 90.0)?;
        let longitude = in_range(Field::Longitude, longitude_east_deg, -180.0, 360.0)?;
        let (hmin, hmax) = HEIGHT_RANGE_M;
        let height_m = in_range(Field::Height, height_m, hmin, hmax)?;
        Ok(Self {
            latitude_deg,
            longitude_east_deg: wrap_signed_deg(longitude),
            height_m,
        })
    }

    pub fn latitude_deg(self) -> f64 {
        self.latitude_deg
    }

    pub fn longitude_east_deg(self) -> f64 {
        self.longitude_east_deg
    }

    pub fn height_m(self) -> f64 {
        self.height_m
    }
}

/// Distance of a target from the geocentre.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TargetDistance {
    /// A finite geocentric distance. Parallax is applied.
    Kilometres(f64),
    /// The caller states that the target is distant enough for parallax to be
    /// ignored, as for stars. This is an explicit assumption, not a default.
    ParallaxNegligible,
}

/// A geocentric apparent place referred to the true equator and equinox of
/// date, as produced by an ephemeris provider.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ApparentPlace {
    ra_deg: f64,
    dec_deg: f64,
    distance: TargetDistance,
}

impl ApparentPlace {
    /// `ra_deg` in [0, 360), `dec_deg` in [−90, 90].
    pub fn new(ra_deg: f64, dec_deg: f64, distance: TargetDistance) -> Result<Self, SkyError> {
        let ra_deg = finite(Field::RightAscension, ra_deg)?;
        if !(0.0..360.0).contains(&ra_deg) {
            return Err(SkyError::OutOfRange {
                field: Field::RightAscension,
                value: ra_deg,
                min: 0.0,
                max: 360.0,
            });
        }
        let dec_deg = in_range(Field::Declination, dec_deg, -90.0, 90.0)?;
        if let TargetDistance::Kilometres(km) = distance {
            in_range(Field::Distance, km, MIN_TARGET_DISTANCE_KM, f64::MAX)?;
        }
        Ok(Self {
            ra_deg,
            dec_deg,
            distance,
        })
    }

    pub fn ra_deg(self) -> f64 {
        self.ra_deg
    }

    pub fn dec_deg(self) -> f64 {
        self.dec_deg
    }

    pub fn distance(self) -> TargetDistance {
        self.distance
    }
}

/// Earth rotation angles at an instant, in degrees unless stated.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EarthRotation {
    /// Greenwich mean sidereal time (IAU 1982), [0, 360).
    pub gmst_deg: f64,
    /// Equation of the equinoxes from the truncated nutation series, arcseconds.
    pub equation_of_equinoxes_arcsec: f64,
    /// Greenwich apparent sidereal time, [0, 360).
    pub gast_deg: f64,
}

/// Computes GMST, the equation of the equinoxes, and GAST.
pub fn earth_rotation(instant: Instant) -> EarthRotation {
    let d = instant.jd_ut1 - J2000_JD;
    let t_ut1 = d / DAYS_PER_CENTURY;
    let gmst = 280.460_618_37 + 360.985_647_366_29 * d + 0.000_387_933 * t_ut1 * t_ut1
        - t_ut1 * t_ut1 * t_ut1 / 38_710_000.0;

    let t = (instant.jd_tt - J2000_JD) / DAYS_PER_CENTURY;
    let omega = (125.044_52 - 1_934.136_261 * t).to_radians();
    let sun_l = (280.466_5 + 36_000.769_8 * t).to_radians();
    let moon_l = (218.316_5 + 481_267.881_3 * t).to_radians();
    let dpsi_arcsec =
        -17.20 * omega.sin() - 1.32 * (2.0 * sun_l).sin() - 0.23 * (2.0 * moon_l).sin()
            + 0.21 * (2.0 * omega).sin();
    let mean_obliquity_arcsec =
        84_381.448 - 46.815_0 * t - 0.000_59 * t * t + 0.001_813 * t * t * t;
    let eqeq_arcsec = dpsi_arcsec * (mean_obliquity_arcsec / ARCSEC_PER_DEG).to_radians().cos();

    EarthRotation {
        gmst_deg: wrap_deg(gmst),
        equation_of_equinoxes_arcsec: eqeq_arcsec,
        gast_deg: wrap_deg(gmst + eqeq_arcsec / ARCSEC_PER_DEG),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Vec3 {
    x: f64,
    y: f64,
    z: f64,
}

impl Vec3 {
    fn from_spherical(ra_deg: f64, dec_deg: f64) -> Self {
        let (sr, cr) = ra_deg.to_radians().sin_cos();
        let (sd, cd) = dec_deg.to_radians().sin_cos();
        Self {
            x: cd * cr,
            y: cd * sr,
            z: sd,
        }
    }

    fn scale(self, k: f64) -> Self {
        Self {
            x: self.x * k,
            y: self.y * k,
            z: self.z * k,
        }
    }

    fn add(self, o: Self) -> Self {
        Self {
            x: self.x + o.x,
            y: self.y + o.y,
            z: self.z + o.z,
        }
    }

    fn sub(self, o: Self) -> Self {
        self.add(o.scale(-1.0))
    }

    fn dot(self, o: Self) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    fn norm(self) -> f64 {
        self.dot(self).sqrt()
    }

    fn unit(self) -> Result<Self, SkyError> {
        let n = self.norm();
        if n.is_finite() && n > 1e-9 {
            Ok(self.scale(1.0 / n))
        } else {
            Err(SkyError::DegenerateGeometry)
        }
    }

    fn ra_dec_deg(self) -> (f64, f64) {
        let ra = wrap_deg(self.y.atan2(self.x).to_degrees());
        let dec = self.z.clamp(-1.0, 1.0).asin().to_degrees();
        (ra, dec)
    }
}

/// Observer position (m) and velocity (m/s) in the true-of-date equatorial
/// frame. Polar motion is neglected.
fn observer_state(site: GeodeticSite, gast_deg: f64) -> (Vec3, Vec3) {
    let e2 = WGS84_F * (2.0 - WGS84_F);
    let (sp, cp) = site.latitude_deg.to_radians().sin_cos();
    let n = WGS84_A_M / (1.0 - e2 * sp * sp).sqrt();
    let rxy = (n + site.height_m) * cp;
    let z = (n * (1.0 - e2) + site.height_m) * sp;
    let (st, ct) = (gast_deg + site.longitude_east_deg).to_radians().sin_cos();
    let position = Vec3 {
        x: rxy * ct,
        y: rxy * st,
        z,
    };
    let velocity = Vec3 {
        x: -EARTH_ROTATION_RAD_S * position.y,
        y: EARTH_ROTATION_RAD_S * position.x,
        z: 0.0,
    };
    (position, velocity)
}

/// A topocentric apparent place referred to the true equator and equinox of
/// date.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TopocentricPlace {
    pub ra_deg: f64,
    pub dec_deg: f64,
    /// Observer-to-target distance, if the input distance was finite.
    pub distance_km: Option<f64>,
}

/// Converts a geocentric apparent place to the observer's topocentric apparent
/// place, applying parallax (finite distances only) and diurnal aberration.
pub fn topocentric_place(
    place: ApparentPlace,
    site: GeodeticSite,
    rotation: EarthRotation,
) -> Result<TopocentricPlace, SkyError> {
    let (position, velocity) = observer_state(site, rotation.gast_deg);
    let direction = Vec3::from_spherical(place.ra_deg, place.dec_deg);
    let (line_of_sight, distance_km) = match place.distance {
        TargetDistance::Kilometres(km) => {
            let topo = direction.scale(km * 1_000.0).sub(position);
            let distance = topo.norm() / 1_000.0;
            (topo.unit()?, Some(distance))
        }
        TargetDistance::ParallaxNegligible => (direction, None),
    };
    // First-order aberration for the observer's rotational velocity (≤ 0.32″).
    let beta = velocity.scale(1.0 / SPEED_OF_LIGHT_M_S);
    let apparent = line_of_sight
        .add(beta)
        .sub(line_of_sight.scale(line_of_sight.dot(beta)))
        .unit()?;
    let (ra_deg, dec_deg) = apparent.ra_dec_deg();
    Ok(TopocentricPlace {
        ra_deg,
        dec_deg,
        distance_km,
    })
}

/// Airless horizontal coordinates relative to the geodetic vertical.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Horizontal {
    /// Azimuth from north through east, [0, 360). Undefined at the zenith and
    /// nadir, where it is reported as computed by `atan2` and carries no
    /// meaning.
    pub azimuth_deg: f64,
    pub elevation_deg: f64,
    /// Local apparent hour angle, [−180, 180).
    pub hour_angle_deg: f64,
}

/// Converts a topocentric place (true equator and equinox of date) to airless
/// azimuth and elevation.
pub fn horizontal(
    place: TopocentricPlace,
    site: GeodeticSite,
    rotation: EarthRotation,
) -> Horizontal {
    let hour_angle = wrap_signed_deg(rotation.gast_deg + site.longitude_east_deg - place.ra_deg);
    let (sh, ch) = hour_angle.to_radians().sin_cos();
    let (sp, cp) = site.latitude_deg.to_radians().sin_cos();
    let (sd, cd) = place.dec_deg.to_radians().sin_cos();
    let elevation = (sp * sd + cp * cd * ch).clamp(-1.0, 1.0).asin();
    let azimuth = (-cd * sh).atan2(sd * cp - cd * ch * sp);
    Horizontal {
        azimuth_deg: wrap_deg(azimuth.to_degrees()),
        elevation_deg: elevation.to_degrees(),
        hour_angle_deg: hour_angle,
    }
}

/// Atmospheric conditions at the observer, stated by the caller.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Atmosphere {
    pressure_hpa: f64,
    temperature_c: f64,
}

impl Atmosphere {
    pub fn new(pressure_hpa: f64, temperature_c: f64) -> Result<Self, SkyError> {
        let (pmin, pmax) = PRESSURE_RANGE_HPA;
        let (tmin, tmax) = TEMPERATURE_RANGE_C;
        Ok(Self {
            pressure_hpa: in_range(Field::Pressure, pressure_hpa, pmin, pmax)?,
            temperature_c: in_range(Field::Temperature, temperature_c, tmin, tmax)?,
        })
    }

    /// 1010 hPa and 10 °C: the conventional reference atmosphere for these
    /// formulas. Using it is an assumption, not a measurement of local
    /// conditions, and should be recorded as such.
    pub fn conventional_reference() -> Self {
        Self {
            pressure_hpa: 1_010.0,
            temperature_c: 10.0,
        }
    }

    pub fn pressure_hpa(self) -> f64 {
        self.pressure_hpa
    }

    pub fn temperature_c(self) -> f64 {
        self.temperature_c
    }
}

/// Result of applying the refraction model to an airless elevation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Refraction {
    /// The model was applied. `elevation_deg` is the refracted (apparent)
    /// elevation; `refraction_arcsec` is the amount added.
    Applied {
        elevation_deg: f64,
        refraction_arcsec: f64,
    },
    /// The airless elevation is below `REFRACTION_MIN_ELEVATION_DEG`. No
    /// refracted value is produced; the airless elevation remains valid.
    BelowModelDomain { airless_elevation_deg: f64 },
}

/// Applies Saemundsson's refraction formula, scaled for pressure and
/// temperature, to an airless elevation.
///
/// Near the horizon, real refraction varies by several arcminutes with the
/// temperature profile. The model gives a conventional value, not an
/// observation of the local atmosphere.
pub fn refract(airless_elevation_deg: f64, atmosphere: Atmosphere) -> Result<Refraction, SkyError> {
    let h = in_range(Field::Elevation, airless_elevation_deg, -90.0, 90.0)?;
    if h < REFRACTION_MIN_ELEVATION_DEG {
        return Ok(Refraction::BelowModelDomain {
            airless_elevation_deg: h,
        });
    }
    let arg_deg = h + 10.3 / (h + 5.11);
    let mut r_arcmin = 1.02 / arg_deg.to_radians().tan();
    // The formula is calibrated near the horizon. At the zenith it gives a tiny
    // negative value; refraction there is zero.
    if r_arcmin < 0.0 || h >= 90.0 {
        r_arcmin = 0.0;
    }
    let scale = (atmosphere.pressure_hpa / 1_010.0) * (283.0 / (273.0 + atmosphere.temperature_c));
    let refraction_arcsec = r_arcmin * 60.0 * scale;
    Ok(Refraction::Applied {
        elevation_deg: (h + refraction_arcsec / ARCSEC_PER_DEG).min(90.0),
        refraction_arcsec,
    })
}

/// Everything computed for one target, observer and instant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyPosition {
    pub rotation: EarthRotation,
    pub topocentric: TopocentricPlace,
    pub airless: Horizontal,
    /// `None` when the caller did not request refraction.
    pub refraction: Option<Refraction>,
}

/// Runs the full chain: Earth rotation, topocentric place, horizontal
/// coordinates and, if `atmosphere` is given, refraction.
pub fn observe(
    place: ApparentPlace,
    site: GeodeticSite,
    instant: Instant,
    atmosphere: Option<Atmosphere>,
) -> Result<SkyPosition, SkyError> {
    let rotation = earth_rotation(instant);
    let topocentric = topocentric_place(place, site, rotation)?;
    let airless = horizontal(topocentric, site, rotation);
    let refraction = match atmosphere {
        Some(atm) => Some(refract(airless.elevation_deg, atm)?),
        None => None,
    };
    Ok(SkyPosition {
        rotation,
        topocentric,
        airless,
        refraction,
    })
}

/// Angular separation between two directions, in degrees (Vincenty form, stable
/// at small and large angles).
pub fn angular_separation_deg(ra1_deg: f64, dec1_deg: f64, ra2_deg: f64, dec2_deg: f64) -> f64 {
    let (s1, c1) = dec1_deg.to_radians().sin_cos();
    let (s2, c2) = dec2_deg.to_radians().sin_cos();
    let (sdl, cdl) = (ra2_deg - ra1_deg).to_radians().sin_cos();
    let num = ((c2 * sdl).powi(2) + (c1 * s2 - s1 * c2 * cdl).powi(2)).sqrt();
    let den = s1 * s2 + c1 * c2 * cdl;
    num.atan2(den).to_degrees()
}

fn wrap_deg(x: f64) -> f64 {
    let r = x.rem_euclid(360.0);
    // rem_euclid can return exactly 360.0 for tiny negative inputs.
    if r >= 360.0 { 0.0 } else { r }
}

fn wrap_signed_deg(x: f64) -> f64 {
    let r = wrap_deg(x + 180.0) - 180.0;
    if r < -180.0 { r + 360.0 } else { r }
}
