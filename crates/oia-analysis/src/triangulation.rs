//! Two calibrated sightlines in a caller-defined metric Cartesian frame.
//!
//! Computes a nominal closest approach and a conditional outer uncertainty
//! sphere, not an observed position or a calibrated probability. Input reviews,
//! calibration validity and error bounds are caller assertions, not verified
//! here. See `docs/implementation/two-view-triangulation.md` for the derivation,
//! numerical domain, and integration limits (FIELD-03 / GEO-04 / MEDIA-02).

use std::{error::Error, fmt};

use oia_domain::{OpaqueId, VersionId};

pub const METHOD_ID: &str = "oia-two-view-closest-approach-v1";
/// Coordinate magnitudes, ranges and length-error inputs must not exceed this.
pub const MAX_LENGTH_M: f64 = 10_000_000.0;
/// Smaller crossing angles are outside this implementation's numerical domain.
pub const MIN_CROSSING_ANGLE_RAD: f64 = std::f64::consts::PI / 180.0;
/// Added to each geometric error tube; not a physical accuracy claim.
pub const NUMERICAL_ALLOWANCE_M: f64 = 0.001;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum View {
    First,
    Second,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputKind {
    Observation,
    Simulation,
}

/// This status must be supplied from calibration validity checks at integration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CalibrationStatus {
    CallerConfirmedValidAtCapture,
    ExpiredAtCapture,
    Unknown,
}

/// Transient numerical input, not a replacement for shared geometry/time records.
#[derive(Clone, Debug, PartialEq)]
pub struct Sightline {
    pub observation_version: VersionId,
    /// Distinct identifiers alone do not establish independent acquisition.
    pub source: OpaqueId,
    /// Exact coordinate frame definition, including axes, origin and metre units.
    pub frame_version: VersionId,
    pub calibration_version: VersionId,
    pub calibrated_pose_version: VersionId,
    pub capture_pose_version: VersionId,
    pub calibration_status: CalibrationStatus,
    pub kind: InputKind,
    pub origin_m: [f64; 3],
    /// Any finite nonzero vector; normalized internally without changing input.
    pub direction: [f64; 3],
    /// Absolute Euclidean camera-position error bound, not a standard deviation.
    pub origin_error_bound_m: Option<f64>,
    /// Absolute direction error in radians, including calibration/lens error.
    pub angular_error_bound_rad: Option<f64>,
    /// Absolute target displacement from capture to the common target epoch.
    /// Must account for timing uncertainty and motion; unknown is not zero.
    pub timing_displacement_bound_m: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PairContext {
    /// Reference to the common reconstruction instant and its time scale.
    pub target_epoch_version: VersionId,
    pub clock_alignment_version: Option<VersionId>,
    /// Review justifying that the two features belong to the same target.
    pub correspondence_review_version: Option<VersionId>,
    /// Review of independent acquisition, including shared/rebroadcast sources.
    pub independence_review_version: Option<VersionId>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    /// Assumed maximum true camera-to-target distance at each capture (metres).
    pub max_range_m: f64,
    /// Reject a result whose outer sphere is wider than this requested radius.
    pub max_position_radius_m: f64,
    /// Acute angle between the two unoriented lines, in [1 degree, pi/2].
    pub min_crossing_angle_rad: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TriangulationInput {
    pub views: [Sightline; 2],
    pub context: PairContext,
    pub limits: Limits,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResultKind {
    DerivedFromObservations,
    /// Includes mixed observation/simulation inputs; never an observation.
    Simulation,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate {
    pub center_m: [f64; 3],
    pub closest_points_m: [[f64; 3]; 2],
    pub nominal_ranges_m: [f64; 2],
    pub separation_m: f64,
    pub crossing_angle_rad: f64,
    /// Outer enclosure in ideal arithmetic, conditional on ALL supplied bounds.
    /// Not a confidence interval, and not proof that any feasible point exists.
    pub position_radius_m: f64,
    /// Per-view error tube radii used for the enclosure, including numerical allowance.
    pub transverse_bounds_m: [f64; 2],
}

/// Keeps the exact supplied inputs and parameters alongside the derived result.
#[derive(Clone, Debug, PartialEq)]
pub struct Reconstruction {
    input: TriangulationInput,
    candidate: Candidate,
    kind: ResultKind,
}

impl Reconstruction {
    pub fn input(&self) -> &TriangulationInput {
        &self.input
    }

    pub fn candidate(&self) -> &Candidate {
        &self.candidate
    }

    pub fn kind(&self) -> ResultKind {
        self.kind
    }

    pub fn method_id(&self) -> &'static str {
        METHOD_ID
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Prerequisite {
    ClockAlignment,
    CorrespondenceReview,
    IndependenceReview,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Bound {
    Origin,
    Direction,
    TimingDisplacement,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TriangulationError {
    MissingPrerequisite(Prerequisite),
    FrameMismatch,
    ReusedObservation,
    ReusedSource,
    InvalidCalibration {
        view: View,
        status: CalibrationStatus,
    },
    CameraMoved(View),
    UnknownBound {
        view: View,
        bound: Bound,
    },
    InvalidNumber {
        view: Option<View>,
        field: &'static str,
    },
    ZeroDirection(View),
    CoincidentOrigins,
    DegenerateGeometry {
        crossing_angle_rad: f64,
    },
    NominalRangeOutsideLimit {
        view: View,
        range_m: f64,
    },
    InconsistentSightlines {
        separation_m: f64,
        allowed_m: f64,
    },
    RegionTooWide {
        radius_m: f64,
        max_m: f64,
    },
    NumericalFailure,
}

impl fmt::Display for TriangulationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingPrerequisite(item) => write!(f, "missing {item:?} reference"),
            Self::FrameMismatch => {
                f.write_str("sightlines use different coordinate frame versions")
            }
            Self::ReusedObservation => {
                f.write_str("two distinct observation versions are required")
            }
            Self::ReusedSource => f.write_str("two distinct acquisition sources are required"),
            Self::InvalidCalibration { view, status } => {
                write!(f, "{view:?} calibration is {status:?}")
            }
            Self::CameraMoved(view) => {
                write!(f, "{view:?} capture pose differs from calibrated pose")
            }
            Self::UnknownBound { view, bound } => {
                write!(f, "{view:?} {bound:?} error bound is unknown")
            }
            Self::InvalidNumber { view, field } => write!(
                f,
                "{view:?} {field} is non-finite or outside the supported domain"
            ),
            Self::ZeroDirection(view) => write!(f, "{view:?} sightline has no direction"),
            Self::CoincidentOrigins => {
                f.write_str("co-located views do not provide a triangulation baseline")
            }
            Self::DegenerateGeometry { crossing_angle_rad } => write!(
                f,
                "crossing angle {crossing_angle_rad} rad cannot meet the requested geometry limit"
            ),
            Self::NominalRangeOutsideLimit { view, range_m } => write!(
                f,
                "{view:?} nominal range {range_m} m is behind the camera or exceeds the range limit"
            ),
            Self::InconsistentSightlines {
                separation_m,
                allowed_m,
            } => write!(
                f,
                "sightline separation {separation_m} m exceeds combined error bound {allowed_m} m"
            ),
            Self::RegionTooWide { radius_m, max_m } => write!(
                f,
                "position radius {radius_m} m exceeds requested maximum {max_m} m"
            ),
            Self::NumericalFailure => {
                f.write_str("triangulation produced non-finite intermediate values")
            }
        }
    }
}

impl Error for TriangulationError {}

/// Fixed-cost, synchronous calculation on exactly two views; no IO, publication,
/// queues, iteration to convergence, coordinate conversion or calibration fitting.
pub fn reconstruct(input: TriangulationInput) -> Result<Reconstruction, TriangulationError> {
    let limits = input.limits;
    valid_number(
        limits.max_range_m,
        f64::MIN_POSITIVE,
        MAX_LENGTH_M,
        None,
        "maximum range",
    )?;
    valid_number(
        limits.max_position_radius_m,
        f64::MIN_POSITIVE,
        MAX_LENGTH_M,
        None,
        "maximum position radius",
    )?;
    valid_number(
        limits.min_crossing_angle_rad,
        MIN_CROSSING_ANGLE_RAD,
        std::f64::consts::FRAC_PI_2,
        None,
        "minimum crossing angle",
    )?;
    for (reference, prerequisite) in [
        (
            &input.context.clock_alignment_version,
            Prerequisite::ClockAlignment,
        ),
        (
            &input.context.correspondence_review_version,
            Prerequisite::CorrespondenceReview,
        ),
        (
            &input.context.independence_review_version,
            Prerequisite::IndependenceReview,
        ),
    ] {
        if reference.is_none() {
            return Err(TriangulationError::MissingPrerequisite(prerequisite));
        }
    }
    let [first, second] = &input.views;
    if first.frame_version != second.frame_version {
        return Err(TriangulationError::FrameMismatch);
    }
    if first.observation_version == second.observation_version {
        return Err(TriangulationError::ReusedObservation);
    }
    if first.source == second.source {
        return Err(TriangulationError::ReusedSource);
    }
    let (u, first_bound) = validate_view(first, View::First, limits.max_range_m)?;
    let (v, second_bound) = validate_view(second, View::Second, limits.max_range_m)?;
    let normal = cross(u, v);
    let sin_angle = norm(normal);
    let abs_cos = dot(u, v).abs().min(1.0);
    // atan2 retains small crossing angles; the absolute cosine treats anti-
    // parallel views as degenerate too. No division until this check passes.
    let crossing_angle_rad = sin_angle.atan2(abs_cos);
    if crossing_angle_rad < limits.min_crossing_angle_rad {
        return Err(TriangulationError::DegenerateGeometry { crossing_angle_rad });
    }
    let sin_squared = dot(normal, normal);
    let baseline = subtract(second.origin_m, first.origin_m);
    if norm(baseline) == 0.0 {
        return Err(TriangulationError::CoincidentOrigins);
    }
    let first_range = dot(cross(baseline, v), normal) / sin_squared;
    let second_range = dot(cross(baseline, u), normal) / sin_squared;
    for (range_m, view) in [(first_range, View::First), (second_range, View::Second)] {
        if !range_m.is_finite() {
            return Err(TriangulationError::NumericalFailure);
        }
        if range_m <= 0.0 || range_m > limits.max_range_m {
            return Err(TriangulationError::NominalRangeOutsideLimit { view, range_m });
        }
    }
    let first_point = add(first.origin_m, scale(u, first_range));
    let second_point = add(second.origin_m, scale(v, second_range));
    let center_m = scale(add(first_point, second_point), 0.5);
    let separation_m = norm(subtract(first_point, second_point));
    let allowed_m = first_bound + second_bound;
    if separation_m > allowed_m {
        return Err(TriangulationError::InconsistentSightlines {
            separation_m,
            allowed_m,
        });
    }
    // Smallest eigenvalue of (I-u*u^T)+(I-v*v^T) is 1-|u.v|.
    // Equivalent sin^2/(1+|cos|) avoids subtracting nearly equal numbers.
    let lambda_min = sin_squared / (1.0 + abs_cos);
    let position_radius_m = first_bound.hypot(second_bound) / lambda_min.sqrt();
    if !position_radius_m.is_finite() || !center_m.into_iter().all(f64::is_finite) {
        return Err(TriangulationError::NumericalFailure);
    }
    if position_radius_m > limits.max_position_radius_m {
        return Err(TriangulationError::RegionTooWide {
            radius_m: position_radius_m,
            max_m: limits.max_position_radius_m,
        });
    }
    let kind = if first.kind == InputKind::Simulation || second.kind == InputKind::Simulation {
        ResultKind::Simulation
    } else {
        ResultKind::DerivedFromObservations
    };
    Ok(Reconstruction {
        candidate: Candidate {
            center_m,
            closest_points_m: [first_point, second_point],
            nominal_ranges_m: [first_range, second_range],
            separation_m,
            crossing_angle_rad,
            position_radius_m,
            transverse_bounds_m: [first_bound, second_bound],
        },
        input,
        kind,
    })
}

fn validate_view(
    line: &Sightline,
    view: View,
    max_range_m: f64,
) -> Result<([f64; 3], f64), TriangulationError> {
    if line.calibration_status != CalibrationStatus::CallerConfirmedValidAtCapture {
        return Err(TriangulationError::InvalidCalibration {
            view,
            status: line.calibration_status,
        });
    }
    if line.calibrated_pose_version != line.capture_pose_version {
        return Err(TriangulationError::CameraMoved(view));
    }
    for coordinate in line.origin_m {
        valid_number(
            coordinate,
            -MAX_LENGTH_M,
            MAX_LENGTH_M,
            Some(view),
            "origin coordinate",
        )?;
    }
    for component in line.direction {
        if !component.is_finite() {
            return Err(TriangulationError::InvalidNumber {
                view: Some(view),
                field: "direction component",
            });
        }
    }
    // Scale before normalizing, including for very large or subnormal vectors.
    let [x, y, z] = line.direction;
    let largest = x.abs().max(y.abs()).max(z.abs());
    if largest == 0.0 {
        return Err(TriangulationError::ZeroDirection(view));
    }
    let scaled = [x / largest, y / largest, z / largest];
    let direction = scale(scaled, 1.0 / norm(scaled));
    let origin_error = read_bound(line.origin_error_bound_m, view, Bound::Origin, MAX_LENGTH_M)?;
    let angle_error = read_bound(
        line.angular_error_bound_rad,
        view,
        Bound::Direction,
        std::f64::consts::FRAC_PI_2,
    )?;
    let timing_error = read_bound(
        line.timing_displacement_bound_m,
        view,
        Bound::TimingDisplacement,
        MAX_LENGTH_M,
    )?;
    let transverse_bound =
        origin_error + max_range_m * angle_error.sin() + timing_error + NUMERICAL_ALLOWANCE_M;
    Ok((direction, transverse_bound))
}

fn read_bound(
    value: Option<f64>,
    view: View,
    bound: Bound,
    max: f64,
) -> Result<f64, TriangulationError> {
    let value = value.ok_or(TriangulationError::UnknownBound { view, bound })?;
    let field = match bound {
        Bound::Origin => "origin error bound",
        Bound::Direction => "direction error bound",
        Bound::TimingDisplacement => "timing displacement bound",
    };
    valid_number(value, 0.0, max, Some(view), field)?;
    Ok(value)
}

fn valid_number(
    value: f64,
    min: f64,
    max: f64,
    view: Option<View>,
    field: &'static str,
) -> Result<(), TriangulationError> {
    if !value.is_finite() || !(min..=max).contains(&value) {
        return Err(TriangulationError::InvalidNumber { view, field });
    }
    Ok(())
}

fn add([ax, ay, az]: [f64; 3], [bx, by, bz]: [f64; 3]) -> [f64; 3] {
    [ax + bx, ay + by, az + bz]
}

fn subtract([ax, ay, az]: [f64; 3], [bx, by, bz]: [f64; 3]) -> [f64; 3] {
    [ax - bx, ay - by, az - bz]
}

fn scale([x, y, z]: [f64; 3], factor: f64) -> [f64; 3] {
    [x * factor, y * factor, z * factor]
}

fn dot([ax, ay, az]: [f64; 3], [bx, by, bz]: [f64; 3]) -> f64 {
    ax * bx + ay * by + az * bz
}

fn cross([ax, ay, az]: [f64; 3], [bx, by, bz]: [f64; 3]) -> [f64; 3] {
    [ay * bz - az * by, az * bx - ax * bz, ax * by - ay * bx]
}

fn norm([x, y, z]: [f64; 3]) -> f64 {
    x.hypot(y).hypot(z)
}
