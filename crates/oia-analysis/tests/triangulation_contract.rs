use oia_analysis::triangulation::{
    Bound, CalibrationStatus, InputKind, Limits, NUMERICAL_ALLOWANCE_M, PairContext, Prerequisite,
    ResultKind, Sightline, TriangulationError, TriangulationInput, View, reconstruct,
};
use oia_domain::{OpaqueId, VersionId};

fn fixture() -> Result<TriangulationInput, Box<dyn std::error::Error>> {
    let first = Sightline {
        observation_version: VersionId::new("ray-a-v1")?,
        source: OpaqueId::new("camera-a")?,
        frame_version: VersionId::new("local-metric-frame-v1")?,
        calibration_version: VersionId::new("cal-a-v1")?,
        calibrated_pose_version: VersionId::new("pose-a-v1")?,
        capture_pose_version: VersionId::new("pose-a-v1")?,
        calibration_status: CalibrationStatus::CallerConfirmedValidAtCapture,
        kind: InputKind::Simulation,
        origin_m: [0.0, 0.0, 0.0],
        direction: [1.0, 2.0, 0.0],
        origin_error_bound_m: Some(0.0),
        angular_error_bound_rad: Some(0.0),
        timing_displacement_bound_m: Some(0.0),
    };
    let second = Sightline {
        observation_version: VersionId::new("ray-b-v1")?,
        source: OpaqueId::new("camera-b")?,
        calibration_version: VersionId::new("cal-b-v1")?,
        calibrated_pose_version: VersionId::new("pose-b-v1")?,
        capture_pose_version: VersionId::new("pose-b-v1")?,
        origin_m: [10.0, 0.0, 0.0],
        direction: [-1.0, 2.0, 0.0],
        ..first.clone()
    };
    Ok(TriangulationInput {
        views: [first, second],
        context: PairContext {
            target_epoch_version: VersionId::new("epoch-v1")?,
            clock_alignment_version: Some(VersionId::new("clock-alignment-v1")?),
            correspondence_review_version: Some(VersionId::new("correspondence-v1")?),
            independence_review_version: Some(VersionId::new("independence-v1")?),
        },
        limits: Limits {
            max_range_m: 100.0,
            max_position_radius_m: 10.0,
            min_crossing_angle_rad: 1.0_f64.to_radians(),
        },
    })
}

fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{actual} != {expected} within {tolerance}"
    );
}

/// FIELD-03/A13: independent analytic intersections and skew-line geometry;
/// the sphere must reflect error bounds, not just a small best-fit residual.
#[test]
fn analytic_geometry_and_conditional_region() -> Result<(), Box<dyn std::error::Error>> {
    let input = fixture()?;
    let result = reconstruct(input.clone())?;
    for (actual, expected) in result
        .candidate()
        .center_m
        .into_iter()
        .zip([5.0, 10.0, 0.0])
    {
        close(actual, expected, 1e-12);
    }
    close(
        result.candidate().nominal_ranges_m[0],
        125.0_f64.sqrt(),
        1e-12,
    );
    close(result.candidate().separation_m, 0.0, 1e-12);
    // u.v = 3/5, lambda_min = 2/5, radius = sqrt(5)*tube_radius.
    close(
        result.candidate().position_radius_m,
        5.0_f64.sqrt() * NUMERICAL_ALLOWANCE_M,
        1e-12,
    );
    assert_eq!(result.input(), &input);
    assert_eq!(result.kind(), ResultKind::Simulation);

    let mut swapped = input.clone();
    swapped.views.swap(0, 1);
    let swapped = reconstruct(swapped)?;
    assert_eq!(swapped.candidate().center_m, result.candidate().center_m);
    let mut scaled = input.clone();
    scaled.views[0].direction = [f64::MAX / 2.0, f64::MAX, 0.0];
    scaled.views[1].direction = [-f64::MIN_POSITIVE, 2.0 * f64::MIN_POSITIVE, 0.0];
    assert_eq!(reconstruct(scaled)?.candidate(), result.candidate());

    let mut skew = input.clone();
    skew.views[1].origin_m[2] = 2.0;
    assert!(matches!(
        reconstruct(skew.clone()),
        Err(TriangulationError::InconsistentSightlines { .. })
    ));
    for line in &mut skew.views {
        line.origin_error_bound_m = Some(1.0);
    }
    let skew = reconstruct(skew)?;
    close(skew.candidate().center_m[2], 1.0, 1e-12);
    close(skew.candidate().separation_m, 2.0, 1e-12);
    close(
        skew.candidate().position_radius_m,
        5.0_f64.sqrt() * (1.0 + NUMERICAL_ALLOWANCE_M),
        1e-12,
    );

    // A known feasible target (5,10,0) with displaced nominal origins,
    // angular errors, and capture-to-epoch motion stays inside the region.
    let mut uncertain = input.clone();
    uncertain.views[0].origin_m = [0.01, 0.0, 0.02];
    uncertain.views[1].origin_m = [10.0, 0.01, -0.02];
    uncertain.views[0].direction = [1.001, 2.0, 0.0];
    uncertain.views[1].direction = [-1.001, 2.0, 0.0];
    for line in &mut uncertain.views {
        line.origin_error_bound_m = Some(0.03);
        line.angular_error_bound_rad = Some(0.001);
        line.timing_displacement_bound_m = Some(0.01);
    }
    let uncertain = reconstruct(uncertain)?;
    let [x, y, z] = uncertain.candidate().center_m;
    assert!((x - 5.0).hypot(y - 10.0).hypot(z) <= uncertain.candidate().position_radius_m);
    assert!(uncertain.candidate().position_radius_m > result.candidate().position_radius_m);

    // Numerical domain edge: a one-degree crossing, large coordinates/range,
    // with an independently constructed exact symmetric target.
    let mut edge = input;
    let angle = 0.5001_f64.to_radians();
    let half_baseline = 8_000_000.0 * angle.tan();
    edge.views[0].origin_m = [9_000_000.0 - half_baseline, -1_000_000.0, 1_000_000.0];
    edge.views[1].origin_m = [9_000_000.0 + half_baseline, -1_000_000.0, 1_000_000.0];
    edge.views[0].direction = [angle.sin(), angle.cos(), 0.0];
    edge.views[1].direction = [-angle.sin(), angle.cos(), 0.0];
    edge.limits.max_range_m = 10_000_000.0;
    let edge = reconstruct(edge)?;
    for (actual, expected) in
        edge.candidate()
            .center_m
            .into_iter()
            .zip([9_000_000.0, 7_000_000.0, 1_000_000.0])
    {
        close(actual, expected, NUMERICAL_ALLOWANCE_M);
    }
    Ok(())
}

/// A13: bad geometry, missing calibration or unbounded uncertainty must never
/// produce a plausible unique range or be repaired by a numerical default.
#[test]
fn refuses_degenerate_geometry_and_unsupported_numbers() -> Result<(), Box<dyn std::error::Error>> {
    let input = fixture()?;
    for direction in [[1.0, 2.0, 0.0], [-1.0, -2.0, 0.0], [1.00001, 2.0, 0.0]] {
        let mut invalid = input.clone();
        invalid.views[1].direction = direction;
        assert!(matches!(
            reconstruct(invalid),
            Err(TriangulationError::DegenerateGeometry { .. })
        ));
    }
    let mut invalid = input.clone();
    invalid.views[1].origin_m = invalid.views[0].origin_m;
    assert_eq!(
        reconstruct(invalid),
        Err(TriangulationError::CoincidentOrigins)
    );
    // A nonzero baseline must exceed both origin-error balls and the numerical
    // allowances; exact nominal intersections cannot rescue unresolved origins.
    let mut unresolved = input.clone();
    unresolved.views[1].origin_m = [1e-300, 0.0, 0.0];
    assert!(matches!(
        reconstruct(unresolved.clone()),
        Err(TriangulationError::BaselineNotResolved { .. })
    ));
    for view in &mut unresolved.views {
        view.origin_error_bound_m = Some(1.0);
    }
    unresolved.views[1].origin_m = [2.0 + 2.0 * NUMERICAL_ALLOWANCE_M, 0.0, 0.0];
    assert!(matches!(
        reconstruct(unresolved.clone()),
        Err(TriangulationError::BaselineNotResolved { .. })
    ));
    unresolved.views[1].origin_m[0] += 2.0 * NUMERICAL_ALLOWANCE_M;
    assert!(reconstruct(unresolved).is_ok());
    let mut invalid = input.clone();
    invalid.views[0].direction = [-1.0, -2.0, 0.0];
    assert!(matches!(
        reconstruct(invalid),
        Err(TriangulationError::NominalRangeOutsideLimit {
            view: View::First,
            ..
        })
    ));
    let mut invalid = input.clone();
    invalid.limits.max_range_m = 10.0;
    assert!(matches!(
        reconstruct(invalid),
        Err(TriangulationError::NominalRangeOutsideLimit { .. })
    ));
    let mut invalid = input.clone();
    invalid.limits.max_position_radius_m = 0.0001;
    assert!(matches!(
        reconstruct(invalid),
        Err(TriangulationError::RegionTooWide { .. })
    ));
    for value in [f64::NAN, f64::INFINITY, -1.0] {
        let mut invalid = input.clone();
        invalid.views[0].angular_error_bound_rad = Some(value);
        assert!(matches!(
            reconstruct(invalid),
            Err(TriangulationError::InvalidNumber { .. })
        ));
        let mut invalid = input.clone();
        invalid.limits.max_range_m = value;
        assert!(matches!(
            reconstruct(invalid),
            Err(TriangulationError::InvalidNumber { .. })
        ));
    }
    let mut invalid = input.clone();
    invalid.views[0].direction = [0.0; 3];
    assert_eq!(
        reconstruct(invalid),
        Err(TriangulationError::ZeroDirection(View::First))
    );
    let mut invalid = input.clone();
    invalid.views[0].origin_m[0] = f64::INFINITY;
    assert!(matches!(
        reconstruct(invalid),
        Err(TriangulationError::InvalidNumber { .. })
    ));
    let mut invalid = input;
    invalid.views[0].direction[2] = f64::NAN;
    assert!(matches!(
        reconstruct(invalid),
        Err(TriangulationError::InvalidNumber { .. })
    ));
    Ok(())
}

/// GEO-04/MEDIA-02: preserve exact versions and simulation labels, refuse
/// stale/moved calibration and require explicit timing/correspondence assumptions.
#[test]
fn preserves_evidence_and_requires_prerequisites() -> Result<(), Box<dyn std::error::Error>> {
    let input = fixture()?;
    let mut invalid = input.clone();
    invalid.views[1].capture_pose_version = VersionId::new("moved-v2")?;
    assert_eq!(
        reconstruct(invalid),
        Err(TriangulationError::CameraMoved(View::Second))
    );
    let mut invalid = input.clone();
    invalid.views[0].calibration_status = CalibrationStatus::ExpiredAtCapture;
    assert_eq!(
        reconstruct(invalid),
        Err(TriangulationError::InvalidCalibration {
            view: View::First,
            status: CalibrationStatus::ExpiredAtCapture
        })
    );
    let mut invalid = input.clone();
    invalid.views[0].timing_displacement_bound_m = None;
    assert_eq!(
        reconstruct(invalid),
        Err(TriangulationError::UnknownBound {
            view: View::First,
            bound: Bound::TimingDisplacement
        })
    );
    let mut invalid = input.clone();
    invalid.context.clock_alignment_version = None;
    assert_eq!(
        reconstruct(invalid),
        Err(TriangulationError::MissingPrerequisite(
            Prerequisite::ClockAlignment
        ))
    );
    let mut invalid = input.clone();
    invalid.context.correspondence_review_version = None;
    assert_eq!(
        reconstruct(invalid),
        Err(TriangulationError::MissingPrerequisite(
            Prerequisite::CorrespondenceReview
        ))
    );
    let mut invalid = input.clone();
    invalid.context.independence_review_version = None;
    assert_eq!(
        reconstruct(invalid),
        Err(TriangulationError::MissingPrerequisite(
            Prerequisite::IndependenceReview
        ))
    );
    let mut invalid = input.clone();
    invalid.views[1].frame_version = VersionId::new("different-frame-v2")?;
    assert_eq!(reconstruct(invalid), Err(TriangulationError::FrameMismatch));
    let mut invalid = input.clone();
    invalid.views[1].source = invalid.views[0].source.clone();
    assert_eq!(reconstruct(invalid), Err(TriangulationError::ReusedSource));
    let mut invalid = input.clone();
    invalid.views[1].observation_version = invalid.views[0].observation_version.clone();
    assert_eq!(
        reconstruct(invalid),
        Err(TriangulationError::ReusedObservation)
    );
    let mut observed = input;
    observed.views[0].kind = InputKind::Observation;
    assert_eq!(
        reconstruct(observed.clone())?.kind(),
        ResultKind::Simulation
    );
    observed.views[1].kind = InputKind::Observation;
    let result = reconstruct(observed.clone())?;
    assert_eq!(result.kind(), ResultKind::DerivedFromObservations);
    assert_eq!(result.input(), &observed);
    Ok(())
}
