//! Synthetic two-camera calculation; no captured imagery or measured calibration.
use oia_analysis::triangulation::{
    CalibrationStatus, InputKind, Limits, PairContext, Sightline, TriangulationInput, reconstruct,
};
use oia_domain::{OpaqueId, VersionId};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let first = Sightline {
        observation_version: VersionId::new("synthetic-ray-a-v1")?,
        source: OpaqueId::new("synthetic-camera-a")?,
        frame_version: VersionId::new("synthetic-cartesian-metres-v1")?,
        calibration_version: VersionId::new("synthetic-calibration-a-v1")?,
        calibrated_pose_version: VersionId::new("synthetic-pose-a-v1")?,
        capture_pose_version: VersionId::new("synthetic-pose-a-v1")?,
        calibration_status: CalibrationStatus::CallerConfirmedValidAtCapture,
        kind: InputKind::Simulation,
        origin_m: [0.0, 0.0, 0.0],
        direction: [1.0, 2.0, 0.0],
        origin_error_bound_m: Some(0.01),
        angular_error_bound_rad: Some(0.001),
        timing_displacement_bound_m: Some(0.02),
    };
    let second = Sightline {
        observation_version: VersionId::new("synthetic-ray-b-v1")?,
        source: OpaqueId::new("synthetic-camera-b")?,
        calibration_version: VersionId::new("synthetic-calibration-b-v1")?,
        calibrated_pose_version: VersionId::new("synthetic-pose-b-v1")?,
        capture_pose_version: VersionId::new("synthetic-pose-b-v1")?,
        origin_m: [10.0, 0.0, 0.0],
        direction: [-1.0, 2.0, 0.0],
        ..first.clone()
    };
    let input = TriangulationInput {
        views: [first, second],
        context: PairContext {
            target_epoch_version: VersionId::new("synthetic-common-epoch-v1")?,
            clock_alignment_version: Some(VersionId::new("synthetic-clock-alignment-v1")?),
            correspondence_review_version: Some(VersionId::new("synthetic-correspondence-v1")?),
            independence_review_version: Some(VersionId::new("synthetic-independence-v1")?),
        },
        limits: Limits {
            max_range_m: 100.0,
            max_position_radius_m: 1.0,
            min_crossing_angle_rad: 1.0_f64.to_radians(),
        },
    };
    let result = reconstruct(input.clone())?;
    println!("SIMULATION: invented views and caller-asserted calibration/timing reviews.");
    println!("Method: {}", result.method_id());
    println!("Frame: {}", result.input().views[0].frame_version);
    println!(
        "Target epoch: {}",
        result.input().context.target_epoch_version
    );
    println!(
        "Input observation versions: {} / {}",
        result.input().views[0].observation_version,
        result.input().views[1].observation_version
    );
    println!(
        "Kind: {:?}; candidate: {:?}",
        result.kind(),
        result.candidate()
    );
    println!("Conditional outer sphere; not a measured position or a confidence interval.");
    let mut moved = input.clone();
    moved.views[1].capture_pose_version = VersionId::new("synthetic-pose-b-moved-v2")?;
    if let Err(error) = reconstruct(moved) {
        println!("Moved-camera example: {error}");
    }
    let mut parallel = input;
    parallel.views[1].direction = parallel.views[0].direction;
    if let Err(error) = reconstruct(parallel) {
        println!("Parallel-view example: {error}");
    }
    Ok(())
}
