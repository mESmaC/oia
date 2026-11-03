//! Deterministic synthetic UAV flight replay demonstrating capability-aware registration,
//! clock-offset synchronization, HUD projection, dropped frames, telemetry disconnection,
//! frame-copy forensic handoff while reception continues, and late onboard-recording linkage
//! (`#33`, `DEVICE-01`–`DEVICE-04`, deterministic subset of `A12`).

use oia_devices::uav::{
    AltitudeReference, AltitudeSample, AttitudeDeg, BatterySample, CameraCalibration,
    ClockAlignmentRecord, FlightBookmarkInput, FlightStreamOrigin, FrameDropSpan, GimbalMode,
    GimbalOrientation, GnssFixKind, HeadingReference, HeadingSample, LinkHealth, LinkState,
    MotionDynamicsFlags, NavigationHealth, OnboardRecordingLinkInput, OriginalFlightStreamInput,
    RecordingState, RegisteredUavDevice, RenderedHudDerivativeInput, TelemetryDisconnectionSpan,
    TelemetrySampleInput, UavAdapterKind, UavAdapterRegistrationInput, UavCapability, UavError,
    UavFlightSession, UavSessionLimits, VelocityMps, VideoFrameInput, Wgs84Position,
};
use oia_domain::{ActorId, OpaqueId, SchemaVersion, VersionId};

fn main() -> Result<(), UavError> {
    let device = RegisteredUavDevice::register(UavAdapterRegistrationInput {
        device_id: OpaqueId::new("device:uav-quad-01")?,
        registration_version: VersionId::new("ver:uav-reg-v1")?,
        schema_version: SchemaVersion::V1,
        registered_by: ActorId::new("actor:field-operator")?,
        adapter_kind: UavAdapterKind::SimulatedTelemetryReplay,
        model_label: "Synthetic Quadrotor Recon Frame (receive-only fixture)".to_owned(),
        declared_capabilities: vec![
            UavCapability::VideoStream,
            UavCapability::TelemetryStream,
            UavCapability::RecordedMediaImport,
        ],
        auto_publish_footage_or_location: false,
    })?;

    let mut session = UavFlightSession::new(
        OpaqueId::new("session:a12-synthetic-flight")?,
        device,
        OriginalFlightStreamInput {
            stream_id: OpaqueId::new("stream:downlink-live-01")?,
            stream_version: VersionId::new("ver:downlink-live-v1")?,
            origin: FlightStreamOrigin::SimulatedFlightFixture,
            sha256_hex: "1111111111111111111111111111111111111111111111111111111111111111"
                .to_owned(),
            byte_len: 48_200_000,
            media_type: "video/mp4".to_owned(),
        },
        UavSessionLimits::default_bounded(),
    )?;

    session.set_clock_alignment(ClockAlignmentRecord {
        alignment_version: VersionId::new("ver:clock-cal-v1")?,
        device_to_utc_offset_ms: 1_762_000_000_000,
        linear_drift_ppm: 25,
        uncertainty_ms: 4,
        reference_clock_label: "GNSS-PPS-ground-reference".to_owned(),
        method_label: "pre-flight-optical-timecode".to_owned(),
        valid_from_device_ms: 0,
        valid_until_device_ms: 600_000,
    })?;

    session.set_camera_calibration(CameraCalibration {
        effective_from_device_ms: 0,
        calibration_version: VersionId::new("ver:cam-cal-24mm-v1")?,
        horizontal_fov_deg: 68.0,
        vertical_fov_deg: 42.0,
        angular_uncertainty_deg: 0.5,
    })?;

    // Sample 1 at t = 100_000 ms: nominal flight with calibrated AGL altitude and independent gimbal.
    session.ingest_telemetry_sample(TelemetrySampleInput {
        sample_version: VersionId::new("ver:telem-001")?,
        device_clock_ms: 100_000,
        receipt_utc_ms: 1_762_000_100_012,
        heading: Some(HeadingSample {
            degrees: 45.0,
            reference: HeadingReference::TrueNorth,
        }),
        attitude: Some(AttitudeDeg {
            roll_deg: 1.2,
            pitch_deg: -2.0,
            yaw_deg: 45.0,
        }),
        position: Some(Wgs84Position {
            latitude_deg: 0.125_000,
            longitude_deg: 0.250_000,
            horizontal_uncertainty_m: Some(1.8),
        }),
        altitude: Some(AltitudeSample {
            meters: 120.0,
            reference: AltitudeReference::AboveGroundLevel,
            vertical_uncertainty_m: Some(0.9),
        }),
        velocity: Some(VelocityMps {
            ground_speed_mps: 11.4,
            vertical_speed_mps: Some(0.2),
        }),
        gimbal: Some(GimbalOrientation {
            pitch_deg: -35.0,
            roll_deg: 0.0,
            yaw_deg: 135.0,
            mode: GimbalMode::IndependentWorldLocked,
        }),
        battery: Some(BatterySample {
            remaining_percent: 84.0,
            voltage_v: Some(22.8),
        }),
        link_health: Some(LinkHealth {
            state: LinkState::Nominal,
            rssi_dbm: Some(-64.0),
            packet_loss_percent: Some(0.2),
        }),
        navigation_health: Some(NavigationHealth {
            fix_kind: GnssFixKind::DifferentialOrRtk,
            satellite_count: Some(18),
            hdop: Some(0.7),
        }),
        recording_state: RecordingState::Recording,
        motion_flags: MotionDynamicsFlags {
            high_vibration: false,
            rapid_gimbal_slew: true,
            rolling_shutter_risk: false,
            exposure_shift_detected: false,
        },
    })?;

    session.ingest_video_frame(VideoFrameInput {
        frame_index: 3000,
        frame_version: VersionId::new("ver:frame-3000")?,
        device_clock_ms: 100_000,
        receipt_utc_ms: 1_762_000_100_015,
        exposure_duration_us: Some(4_000),
        sha256_hex: "2222222222222222222222222222222222222222222222222222222222222222".to_owned(),
    })?;

    // Bookmark frame 3000 and export a non-blocking forensic frame handoff while reception continues.
    let bm_id = OpaqueId::new("bookmark:a12-anomaly-pass-1")?;
    session.record_bookmark(FlightBookmarkInput {
        bookmark_id: bm_id.clone(),
        case_id: OpaqueId::new("case:a12-uav-recon")?,
        created_by: ActorId::new("actor:field-operator")?,
        device_clock_ms: 100_000,
        receipt_utc_ms: 1_762_000_100_020,
        pinned_frame_index: Some(3000),
        label: "Independent gimbal track across ridge".to_owned(),
        note: Some("Gimbal world-locked at 135 deg while airframe tracks 45 deg".to_owned()),
    })?;

    let handoff = session.export_forensic_frame_handoff(
        OpaqueId::new("handoff:frame-3000-forensic")?,
        3000,
        Some(bm_id.clone()),
    )?;
    println!(
        "{}",
        handoff.render_handoff_manifest(session.limits().max_output_bytes())?
    );

    // Reception continues: record a 12-frame downlink drop and a telemetry disconnection span.
    session.record_frame_drop(FrameDropSpan {
        after_frame_index: Some(3000),
        dropped_frame_count: 12,
        detected_at_device_ms: 100_400,
        reason: "Downlink multipath fade".to_owned(),
    })?;
    session.record_telemetry_disconnection(TelemetryDisconnectionSpan {
        disconnected_at_device_ms: 101_500,
        reconnected_at_device_ms: Some(104_000),
        reason: "Brief terrain masking behind ridge".to_owned(),
    })?;

    // Project HUD during the disconnection window at t = 103_000 ms (telemetry is marked STALE).
    let stale_hud = session.project_hud_snapshot(103_000, 1_762_000_103_020);
    println!(
        "{}",
        stale_hud.render_hud_text(session.limits().max_output_bytes())?
    );

    // Register a separate rendered HUD derivative and later link higher-quality onboard footage.
    session.register_hud_derivative(RenderedHudDerivativeInput {
        derivative_id: OpaqueId::new("artifact:hud-overlay-preview")?,
        derivative_version: VersionId::new("ver:hud-overlay-v1")?,
        derived_from_stream_version: VersionId::new("ver:downlink-live-v1")?,
        sha256_hex: "3333333333333333333333333333333333333333333333333333333333333333".to_owned(),
        byte_len: 19_400_000,
        media_type: "video/mp4".to_owned(),
        overlay_configuration_label: "Military-glass HUD v1 (SI meters, WGS84, UTC)".to_owned(),
    })?;

    let onboard = session.link_onboard_recording(OnboardRecordingLinkInput {
        link_id: OpaqueId::new("link:onboard-sd-card-01")?,
        onboard_stream_version: VersionId::new("ver:onboard-4k-raw-v1")?,
        sha256_hex: "4444444444444444444444444444444444444444444444444444444444444444".to_owned(),
        byte_len: 412_800_000,
        media_type: "video/mp4".to_owned(),
        stream_to_onboard_offset_ms: -15,
        linked_bookmark_ids: vec![bm_id],
        alignment_note: "Matched via optical timecode slate and GNSS PPS".to_owned(),
    })?;

    println!(
        "Linked onboard recording `{}` (prior live stream preserved: `{}`, overwrote_prior_evidence={})",
        onboard.onboard_stream_version().as_str(),
        onboard.prior_live_stream_version().as_str(),
        onboard.overwrote_prior_evidence()
    );

    Ok(())
}
