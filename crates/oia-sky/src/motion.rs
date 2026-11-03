//! Provider-independent two-body motion and one-way light-time correction.
//!
//! Every position is in AU, every velocity in AU/day, and every epoch is TDB.
//! The only frame currently accepted is the ecliptic of J2000.0. These are
//! modelled geometric states, not observations or apparent places.

use std::error::Error;
use std::fmt;

/// Identifies the motion models, independently of the observer `MODEL_ID`.
pub const MOTION_MODEL_ID: &str =
    "oia-sky-motion-v1: elliptical two-body propagation; fixed-point one-way light time";

/// Speed of light in AU/day, using the exact SI speed and IAU 2012 AU.
pub const SPEED_OF_LIGHT_AU_PER_DAY: f64 = 299_792.458 * 86_400.0 / 149_597_870.700;
/// Earliest supported TDB Julian date (1900-01-01T00:00 TDB).
pub const MIN_JD_TDB: f64 = 2_415_020.5;
/// Latest supported TDB Julian date (2100-01-01T00:00 TDB).
pub const MAX_JD_TDB: f64 = 2_488_069.5;
/// Maximum absolute propagation age from the element epoch (100 Julian years).
pub const MAX_ELEMENT_AGE_DAYS: f64 = 36_525.0;
/// Maximum absolute unwrapped mean anomaly. At this bound, one `f64` ulp is
/// about 1.2e-10 rad (17 m at 1 AU), rather than an unbounded phase error.
pub const MAX_UNWRAPPED_MEAN_ANOMALY_RAD: f64 = 1.0e6;
/// Largest accepted observer-to-target range for light-time correction.
pub const MAX_LIGHT_TIME_RANGE_AU: f64 = 1.0e9;

const MAX_INPUT_ANGLE_DEG: f64 = 360.0;
const MAX_METADATA_BYTES: usize = 128;

/// Fields named by motion validation errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MotionField {
    JdTdb,
    SemiMajorAxis,
    Eccentricity,
    Inclination,
    AscendingNode,
    ArgumentOfPeriapsis,
    MeanAnomaly,
    ElementAge,
    Range,
    GravitationalParameter,
    Iterations,
    Tolerance,
    Source,
    SourceVersion,
    Center,
}

/// Validation or numerical failure in a motion calculation.
#[derive(Clone, Debug, PartialEq)]
pub enum MotionError {
    NonFinite(MotionField),
    OutOfRange {
        field: MotionField,
        value: f64,
        min: f64,
        max: f64,
    },
    EmptyMetadata(MotionField),
    InvalidMetadata(MotionField),
    MetadataTooLong {
        field: MotionField,
        max_bytes: usize,
    },
    InvalidLimit(&'static str),
    NonFiniteDerived(&'static str),
    KeplerDidNotConverge {
        iterations: u32,
        residual_rad: f64,
    },
    LightTimeDidNotConverge {
        iterations: u32,
        residual_days: f64,
    },
    DegenerateRange,
    FrameMismatch,
    CenterMismatch,
    EpochMismatch,
}

impl fmt::Display for MotionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinite(field) => write!(f, "{field:?} is not finite"),
            Self::OutOfRange {
                field,
                value,
                min,
                max,
            } => write!(
                f,
                "{field:?} = {value} is outside the supported range [{min}, {max}]"
            ),
            Self::EmptyMetadata(field) => write!(f, "{field:?} must not be empty"),
            Self::InvalidMetadata(field) => {
                write!(f, "{field:?} contains a control character")
            }
            Self::MetadataTooLong { field, max_bytes } => {
                write!(f, "{field:?} exceeds {max_bytes} UTF-8 bytes")
            }
            Self::InvalidLimit(message) => f.write_str(message),
            Self::NonFiniteDerived(name) => write!(f, "derived {name} is not finite"),
            Self::KeplerDidNotConverge {
                iterations,
                residual_rad,
            } => write!(
                f,
                "Kepler solver did not converge after {iterations} iterations (residual {residual_rad} rad)"
            ),
            Self::LightTimeDidNotConverge {
                iterations,
                residual_days,
            } => write!(
                f,
                "light-time solver did not converge after {iterations} iterations (residual {residual_days} days)"
            ),
            Self::DegenerateRange => f.write_str("observer and target positions coincide"),
            Self::FrameMismatch => f.write_str("positions are not in one common frame"),
            Self::CenterMismatch => f.write_str("positions do not use one common center"),
            Self::EpochMismatch => f.write_str("sampler returned a state at the wrong TDB epoch"),
        }
    }
}

impl Error for MotionError {}

fn finite(field: MotionField, value: f64) -> Result<f64, MotionError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(MotionError::NonFinite(field))
    }
}

fn in_range(field: MotionField, value: f64, min: f64, max: f64) -> Result<f64, MotionError> {
    let value = finite(field, value)?;
    if (min..=max).contains(&value) {
        Ok(value)
    } else {
        Err(MotionError::OutOfRange {
            field,
            value,
            min,
            max,
        })
    }
}

fn normalized_angle(field: MotionField, value: f64) -> Result<f64, MotionError> {
    let value = in_range(field, value, -MAX_INPUT_ANGLE_DEG, MAX_INPUT_ANGLE_DEG)?;
    let normalized = value.rem_euclid(360.0) + 0.0;
    if normalized >= 360.0 {
        Ok(0.0)
    } else {
        Ok(normalized)
    }
}

fn metadata(field: MotionField, value: &str) -> Result<&str, MotionError> {
    if value.trim().is_empty() {
        return Err(MotionError::EmptyMetadata(field));
    }
    if value.len() > MAX_METADATA_BYTES {
        return Err(MotionError::MetadataTooLong {
            field,
            max_bytes: MAX_METADATA_BYTES,
        });
    }
    if value.chars().any(char::is_control) {
        return Err(MotionError::InvalidMetadata(field));
    }
    Ok(value)
}

/// A TDB Julian date in the supported 1900–2100 range. No UTC, UT1, or TT
/// conversion is performed. **Provisional pending #12:** this type will move to
/// or be replaced by shared higher-precision time and uncertainty primitives.
///
/// The absolute date is an `f64`; near 2024 its spacing is about 40.2 µs. Use a
/// returned light-time duration rather than subtracting absolute epochs when
/// microsecond-scale timing matters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TdbEpoch {
    jd_tdb: f64,
}

impl TdbEpoch {
    pub fn new(jd_tdb: f64) -> Result<Self, MotionError> {
        Ok(Self {
            jd_tdb: in_range(MotionField::JdTdb, jd_tdb, MIN_JD_TDB, MAX_JD_TDB)?,
        })
    }

    pub fn jd_tdb(self) -> f64 {
        self.jd_tdb
    }

    pub fn days_since(self, earlier: Self) -> f64 {
        self.jd_tdb - earlier.jd_tdb
    }

    fn offset_days(self, days: f64) -> Result<Self, MotionError> {
        let jd_tdb = self.jd_tdb + days;
        if jd_tdb.is_finite() {
            Self::new(jd_tdb)
        } else {
            Err(MotionError::NonFiniteDerived("TDB epoch"))
        }
    }
}

/// The reference frame of a Cartesian state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReferenceFrame {
    EclipticOfJ2000,
}

/// A caller-supplied center identity, not a gravitational-parameter catalog.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CenterId<'a> {
    name: &'a str,
}

impl<'a> CenterId<'a> {
    pub fn new(name: &'a str) -> Result<Self, MotionError> {
        Ok(Self {
            name: metadata(MotionField::Center, name)?,
        })
    }

    pub fn as_str(self) -> &'a str {
        self.name
    }
}

/// A finite Cartesian position in AU.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositionAu {
    x: f64,
    y: f64,
    z: f64,
}

impl PositionAu {
    pub fn new(x: f64, y: f64, z: f64) -> Result<Self, MotionError> {
        if !x.is_finite() || !y.is_finite() || !z.is_finite() {
            return Err(MotionError::NonFiniteDerived("position"));
        }
        Ok(Self { x, y, z })
    }

    pub fn x(self) -> f64 {
        self.x
    }

    pub fn y(self) -> f64 {
        self.y
    }

    pub fn z(self) -> f64 {
        self.z
    }

    fn sub(self, other: Self) -> Result<Self, MotionError> {
        Self::new(self.x - other.x, self.y - other.y, self.z - other.z)
    }

    fn norm(self) -> Result<f64, MotionError> {
        let norm = self.x.hypot(self.y).hypot(self.z);
        if norm.is_finite() {
            Ok(norm)
        } else {
            Err(MotionError::NonFiniteDerived("range"))
        }
    }
}

/// A finite Cartesian velocity in AU/day.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VelocityAuPerDay {
    x: f64,
    y: f64,
    z: f64,
}

impl VelocityAuPerDay {
    pub fn new(x: f64, y: f64, z: f64) -> Result<Self, MotionError> {
        if !x.is_finite() || !y.is_finite() || !z.is_finite() {
            return Err(MotionError::NonFiniteDerived("velocity"));
        }
        Ok(Self { x, y, z })
    }

    pub fn x(self) -> f64 {
        self.x
    }

    pub fn y(self) -> f64 {
        self.y
    }

    pub fn z(self) -> f64 {
        self.z
    }
}

/// A geometric state in one declared center and frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CartesianState<'a> {
    epoch: TdbEpoch,
    frame: ReferenceFrame,
    center: CenterId<'a>,
    position: PositionAu,
    velocity: VelocityAuPerDay,
}

impl<'a> CartesianState<'a> {
    pub fn new_geometric(
        epoch: TdbEpoch,
        frame: ReferenceFrame,
        center: CenterId<'a>,
        position: PositionAu,
        velocity: VelocityAuPerDay,
    ) -> Self {
        Self {
            epoch,
            frame,
            center,
            position,
            velocity,
        }
    }

    pub fn epoch(self) -> TdbEpoch {
        self.epoch
    }

    pub fn frame(self) -> ReferenceFrame {
        self.frame
    }

    pub fn center(self) -> CenterId<'a> {
        self.center
    }

    pub fn position(self) -> PositionAu {
        self.position
    }

    pub fn velocity(self) -> VelocityAuPerDay {
        self.velocity
    }
}

/// Exact source and version labels for caller-supplied orbital elements.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ElementProvenance<'a> {
    source: &'a str,
    version: &'a str,
}

impl<'a> ElementProvenance<'a> {
    pub fn new(source: &'a str, version: &'a str) -> Result<Self, MotionError> {
        Ok(Self {
            source: metadata(MotionField::Source, source)?,
            version: metadata(MotionField::SourceVersion, version)?,
        })
    }

    pub fn source(self) -> &'a str {
        self.source
    }

    pub fn version(self) -> &'a str {
        self.version
    }
}

/// Unvalidated orbital parameters. Angles are degrees; `mu` is AU³/day².
///
/// The ascending node Ω is measured from J2000 ecliptic +X toward +Y, the
/// inclination i is about the line of nodes, and the argument of periapsis ω is
/// measured from that node in the orbital plane. M₀ is the mean anomaly at the
/// element epoch, not the longitude of periapsis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeplerianParameters {
    pub semi_major_axis_au: f64,
    pub eccentricity: f64,
    pub inclination_deg: f64,
    pub ascending_node_deg: f64,
    pub argument_of_periapsis_deg: f64,
    pub mean_anomaly_at_epoch_deg: f64,
    pub gravitational_parameter_au3_day2: f64,
}

/// Validated elliptical elements at a stated TDB epoch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EllipticalElements<'a> {
    provenance: ElementProvenance<'a>,
    center: CenterId<'a>,
    epoch: TdbEpoch,
    parameters: KeplerianParameters,
}

impl<'a> EllipticalElements<'a> {
    /// Validates parameters and normalizes the bounded node, periapsis, and
    /// mean-anomaly angles to [0, 360). `parameters()` returns those normalized
    /// angles, not the original unnormalized representation.
    pub fn new(
        provenance: ElementProvenance<'a>,
        center: CenterId<'a>,
        epoch: TdbEpoch,
        mut parameters: KeplerianParameters,
    ) -> Result<Self, MotionError> {
        in_range(
            MotionField::SemiMajorAxis,
            parameters.semi_major_axis_au,
            f64::MIN_POSITIVE,
            f64::MAX,
        )?;
        finite(MotionField::Eccentricity, parameters.eccentricity)?;
        if !(0.0..1.0).contains(&parameters.eccentricity) {
            return Err(MotionError::OutOfRange {
                field: MotionField::Eccentricity,
                value: parameters.eccentricity,
                min: 0.0,
                max: 1.0,
            });
        }
        in_range(
            MotionField::Inclination,
            parameters.inclination_deg,
            0.0,
            180.0,
        )?;
        parameters.ascending_node_deg =
            normalized_angle(MotionField::AscendingNode, parameters.ascending_node_deg)?;
        parameters.argument_of_periapsis_deg = normalized_angle(
            MotionField::ArgumentOfPeriapsis,
            parameters.argument_of_periapsis_deg,
        )?;
        parameters.mean_anomaly_at_epoch_deg = normalized_angle(
            MotionField::MeanAnomaly,
            parameters.mean_anomaly_at_epoch_deg,
        )?;
        in_range(
            MotionField::GravitationalParameter,
            parameters.gravitational_parameter_au3_day2,
            f64::MIN_POSITIVE,
            f64::MAX,
        )?;
        Ok(Self {
            provenance,
            center,
            epoch,
            parameters,
        })
    }

    pub fn provenance(self) -> ElementProvenance<'a> {
        self.provenance
    }

    pub fn center(self) -> CenterId<'a> {
        self.center
    }

    pub fn epoch(self) -> TdbEpoch {
        self.epoch
    }

    /// Returns the validated parameters, with angles normalized to [0, 360).
    pub fn parameters(self) -> KeplerianParameters {
        self.parameters
    }
}

/// Iteration bound and angular residual tolerance for Kepler's equation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PropagationLimits {
    max_iterations: u32,
    tolerance_rad: f64,
}

impl PropagationLimits {
    pub fn new(max_iterations: u32, tolerance_rad: f64) -> Result<Self, MotionError> {
        if max_iterations == 0 || max_iterations > 10_000 {
            return Err(MotionError::InvalidLimit(
                "Kepler max_iterations must be in 1..=10000",
            ));
        }
        if !tolerance_rad.is_finite() || tolerance_rad <= 0.0 {
            return Err(MotionError::InvalidLimit(
                "Kepler tolerance must be finite and positive",
            ));
        }
        Ok(Self {
            max_iterations,
            tolerance_rad,
        })
    }
}

/// An elliptical two-body orbit with caller-supplied provenance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeplerOrbit<'a> {
    elements: EllipticalElements<'a>,
    mean_motion_rad_day: f64,
}

impl<'a> KeplerOrbit<'a> {
    pub fn new(elements: EllipticalElements<'a>) -> Result<Self, MotionError> {
        let p = elements.parameters;
        // sqrt(mu / a) / a avoids overflowing a³ for large finite inputs.
        let mean_motion_rad_day = (p.gravitational_parameter_au3_day2 / p.semi_major_axis_au)
            .sqrt()
            / p.semi_major_axis_au;
        if !mean_motion_rad_day.is_finite() || mean_motion_rad_day <= 0.0 {
            return Err(MotionError::NonFiniteDerived("mean motion"));
        }
        Ok(Self {
            elements,
            mean_motion_rad_day,
        })
    }

    pub fn elements(self) -> EllipticalElements<'a> {
        self.elements
    }

    /// Propagates without perturbations. The result retains the exact element
    /// source/version and signed age from the element epoch. It solves
    /// `E - e sin(E) = M₀ + nΔt` and rotates perifocal position and velocity by
    /// Rz(Ω) Rx(i) Rz(ω). The solver report records its quality settings.
    pub fn state_at(
        self,
        epoch: TdbEpoch,
        limits: PropagationLimits,
    ) -> Result<OrbitalState<'a>, MotionError> {
        let element_age_days = in_range(
            MotionField::ElementAge,
            epoch.days_since(self.elements.epoch),
            -MAX_ELEMENT_AGE_DAYS,
            MAX_ELEMENT_AGE_DAYS,
        )?;
        let mean_anomaly = self
            .elements
            .parameters
            .mean_anomaly_at_epoch_deg
            .to_radians()
            + self.mean_motion_rad_day * element_age_days;
        let mean_anomaly = in_range(
            MotionField::MeanAnomaly,
            mean_anomaly,
            -MAX_UNWRAPPED_MEAN_ANOMALY_RAD,
            MAX_UNWRAPPED_MEAN_ANOMALY_RAD,
        )?;
        let mean_anomaly = mean_anomaly.rem_euclid(2.0 * std::f64::consts::PI);
        let (eccentric_anomaly, kepler_report) =
            solve_kepler(mean_anomaly, self.elements.parameters.eccentricity, limits)?;
        let state = self.state_from_eccentric_anomaly(epoch, eccentric_anomaly)?;
        Ok(OrbitalState {
            state,
            elements: self.elements,
            element_age_days,
            kepler_report,
        })
    }

    fn state_from_eccentric_anomaly(
        self,
        epoch: TdbEpoch,
        eccentric_anomaly: f64,
    ) -> Result<CartesianState<'a>, MotionError> {
        let p = self.elements.parameters;
        let (sin_e, cos_e) = eccentric_anomaly.sin_cos();
        let root = (1.0 - p.eccentricity * p.eccentricity).sqrt();
        let denominator = 1.0 - p.eccentricity * cos_e;
        let eccentric_rate = self.mean_motion_rad_day / denominator;
        if !denominator.is_finite() || denominator <= 0.0 || !eccentric_rate.is_finite() {
            return Err(MotionError::NonFiniteDerived("eccentric anomaly rate"));
        }

        let x_perifocal = p.semi_major_axis_au * (cos_e - p.eccentricity);
        let y_perifocal = p.semi_major_axis_au * root * sin_e;
        let vx_perifocal = -p.semi_major_axis_au * eccentric_rate * sin_e;
        let vy_perifocal = p.semi_major_axis_au * eccentric_rate * root * cos_e;

        let (sin_node, cos_node) = p.ascending_node_deg.to_radians().sin_cos();
        let (sin_inclination, cos_inclination) = p.inclination_deg.to_radians().sin_cos();
        let (sin_arg, cos_arg) = p.argument_of_periapsis_deg.to_radians().sin_cos();
        let r11 = cos_node * cos_arg - sin_node * sin_arg * cos_inclination;
        let r12 = -cos_node * sin_arg - sin_node * cos_arg * cos_inclination;
        let r21 = sin_node * cos_arg + cos_node * sin_arg * cos_inclination;
        let r22 = -sin_node * sin_arg + cos_node * cos_arg * cos_inclination;
        let r31 = sin_arg * sin_inclination;
        let r32 = cos_arg * sin_inclination;

        let position = PositionAu::new(
            r11 * x_perifocal + r12 * y_perifocal,
            r21 * x_perifocal + r22 * y_perifocal,
            r31 * x_perifocal + r32 * y_perifocal,
        )?;
        let velocity = VelocityAuPerDay::new(
            r11 * vx_perifocal + r12 * vy_perifocal,
            r21 * vx_perifocal + r22 * vy_perifocal,
            r31 * vx_perifocal + r32 * vy_perifocal,
        )?;
        Ok(CartesianState::new_geometric(
            epoch,
            ReferenceFrame::EclipticOfJ2000,
            self.elements.center,
            position,
            velocity,
        ))
    }
}

/// Numerical quality of a converged Kepler solve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeplerReport {
    pub iterations: u32,
    pub residual_rad: f64,
    pub tolerance_rad: f64,
}

/// A propagated state and the provenance/age of its orbital elements.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbitalState<'a> {
    state: CartesianState<'a>,
    elements: EllipticalElements<'a>,
    element_age_days: f64,
    kepler_report: KeplerReport,
}

impl<'a> OrbitalState<'a> {
    pub fn state(self) -> CartesianState<'a> {
        self.state
    }

    pub fn provenance(self) -> ElementProvenance<'a> {
        self.elements.provenance
    }

    pub fn element_epoch(self) -> TdbEpoch {
        self.elements.epoch
    }

    pub fn element_age_days(self) -> f64 {
        self.element_age_days
    }

    pub fn kepler_report(self) -> KeplerReport {
        self.kepler_report
    }
}

fn solve_kepler(
    mean_anomaly: f64,
    eccentricity: f64,
    limits: PropagationLimits,
) -> Result<(f64, KeplerReport), MotionError> {
    let mut lower = 0.0;
    let mut upper = 2.0 * std::f64::consts::PI;
    let mut estimate = if eccentricity < 0.8 {
        mean_anomaly
    } else {
        std::f64::consts::PI
    };
    let mut residual = f64::NAN;

    for iteration in 1..=limits.max_iterations {
        let (sin_e, cos_e) = estimate.sin_cos();
        residual = estimate - eccentricity * sin_e - mean_anomaly;
        if residual.abs() <= limits.tolerance_rad {
            return Ok((
                estimate,
                KeplerReport {
                    iterations: iteration,
                    residual_rad: residual.abs(),
                    tolerance_rad: limits.tolerance_rad,
                },
            ));
        }
        if residual > 0.0 {
            upper = estimate;
        } else {
            lower = estimate;
        }
        let derivative = 1.0 - eccentricity * cos_e;
        let newton = estimate - residual / derivative;
        estimate = if newton.is_finite() && newton > lower && newton < upper {
            newton
        } else {
            lower + (upper - lower) / 2.0
        };
    }

    Err(MotionError::KeplerDidNotConverge {
        iterations: limits.max_iterations,
        residual_rad: residual,
    })
}

/// Iteration bound and light-time residual tolerance, in days. Because every
/// successful solve requires an initial evaluation and at least one update, a
/// limit of one is accepted for budget-failure probing but cannot succeed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightTimeLimits {
    max_iterations: u32,
    tolerance_days: f64,
}

impl LightTimeLimits {
    pub fn new(max_iterations: u32, tolerance_days: f64) -> Result<Self, MotionError> {
        if max_iterations == 0 || max_iterations > 10_000 {
            return Err(MotionError::InvalidLimit(
                "light-time max_iterations must be in 1..=10000",
            ));
        }
        if !tolerance_days.is_finite() || tolerance_days <= 0.0 {
            return Err(MotionError::InvalidLimit(
                "light-time tolerance must be finite and positive",
            ));
        }
        Ok(Self {
            max_iterations,
            tolerance_days,
        })
    }
}

/// A light-time failure that preserves which sampler failed.
#[derive(Clone, Debug, PartialEq)]
pub enum LightTimeError<E> {
    Target(E),
    Observer(E),
    Motion(MotionError),
}

impl<E: fmt::Display> fmt::Display for LightTimeError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Target(error) => write!(f, "target sampler failed: {error}"),
            Self::Observer(error) => write!(f, "observer sampler failed: {error}"),
            Self::Motion(error) => error.fmt(f),
        }
    }
}

impl<E: Error + 'static> Error for LightTimeError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Target(error) | Self::Observer(error) => Some(error),
            Self::Motion(error) => Some(error),
        }
    }
}

impl<E> From<MotionError> for LightTimeError<E> {
    fn from(error: MotionError) -> Self {
        Self::Motion(error)
    }
}

/// Observer-to-target position at the retarded emission epoch.
///
/// This is not an apparent direction, a barycentric state, or a corrected
/// velocity. The reported residual and tolerance make convergence quality
/// explicit. No aberration, gravitational bending, or refraction is applied.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightTimePosition<'a> {
    reception: TdbEpoch,
    emission: TdbEpoch,
    frame: ReferenceFrame,
    center: CenterId<'a>,
    observer_to_target_au: PositionAu,
    light_time_days: f64,
    range_au: f64,
    iterations: u32,
    residual_days: f64,
    tolerance_days: f64,
}

impl<'a> LightTimePosition<'a> {
    pub fn reception(self) -> TdbEpoch {
        self.reception
    }

    pub fn emission(self) -> TdbEpoch {
        self.emission
    }

    pub fn frame(self) -> ReferenceFrame {
        self.frame
    }

    pub fn center(self) -> CenterId<'a> {
        self.center
    }

    pub fn observer_to_target_au(self) -> PositionAu {
        self.observer_to_target_au
    }

    pub fn light_time_days(self) -> f64 {
        self.light_time_days
    }

    pub fn range_au(self) -> f64 {
        self.range_au
    }

    pub fn iterations(self) -> u32 {
        self.iterations
    }

    pub fn residual_days(self) -> f64 {
        self.residual_days
    }

    pub fn tolerance_days(self) -> f64 {
        self.tolerance_days
    }
}

/// Solves `tau = |target(t - tau) - observer(t)| / c` by bounded fixed-point
/// iteration. Both samplers must return geometric states in the same inertial
/// center and frame. The observer is evaluated only at the reception epoch.
/// A non-inertial center can add a center-motion error of about `v_center*tau`;
/// the retained heliocentric Mars comparison exhibits roughly 10 km from this
/// effect, so use an inertial SSB-centered reference for tight verification.
pub fn light_time_corrected_position<'a, E, F, G>(
    reception: TdbEpoch,
    mut target: F,
    mut observer: G,
    limits: LightTimeLimits,
) -> Result<LightTimePosition<'a>, LightTimeError<E>>
where
    F: FnMut(TdbEpoch) -> Result<CartesianState<'a>, E>,
    G: FnMut(TdbEpoch) -> Result<CartesianState<'a>, E>,
{
    let observer_state = observer(reception).map_err(LightTimeError::Observer)?;
    if observer_state.epoch != reception {
        return Err(MotionError::EpochMismatch.into());
    }

    let mut light_time_days = 0.0;
    let mut residual_days = f64::NAN;
    for iteration in 1..=limits.max_iterations {
        let emission = reception.offset_days(-light_time_days)?;
        let target_state = target(emission).map_err(LightTimeError::Target)?;
        if target_state.epoch != emission || observer_state.epoch != reception {
            return Err(MotionError::EpochMismatch.into());
        }
        if target_state.frame != observer_state.frame {
            return Err(MotionError::FrameMismatch.into());
        }
        if target_state.center != observer_state.center {
            return Err(MotionError::CenterMismatch.into());
        }

        let observer_to_target_au = target_state.position.sub(observer_state.position)?;
        let range_au = observer_to_target_au.norm()?;
        if range_au <= 0.0 {
            return Err(MotionError::DegenerateRange.into());
        }
        in_range(
            MotionField::Range,
            range_au,
            f64::MIN_POSITIVE,
            MAX_LIGHT_TIME_RANGE_AU,
        )?;
        let next_light_time_days = range_au / SPEED_OF_LIGHT_AU_PER_DAY;
        if !next_light_time_days.is_finite() {
            return Err(MotionError::NonFiniteDerived("light time").into());
        }
        residual_days = (next_light_time_days - light_time_days).abs();
        // Always apply at least one update; a coarse tolerance must not turn
        // the initial tau = 0 guess into a supposedly corrected result.
        if iteration > 1 && residual_days <= limits.tolerance_days {
            return Ok(LightTimePosition {
                reception,
                emission,
                frame: observer_state.frame,
                center: observer_state.center,
                observer_to_target_au,
                light_time_days,
                range_au,
                iterations: iteration,
                residual_days,
                tolerance_days: limits.tolerance_days,
            });
        }
        light_time_days = next_light_time_days;
    }

    Err(MotionError::LightTimeDidNotConverge {
        iterations: limits.max_iterations,
        residual_days,
    }
    .into())
}
