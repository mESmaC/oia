//! Targeted contract tests for personal UAV registration, capability-aware HUD projection,
//! clock-offset alignment, frame-copy forensic handoff, and non-destructive onboard-footage
//! linkage (`#33`, `DEVICE-01`–`DEVICE-04`, `A12`, `AGENTS.md` §§2, 4, 5, 6, 8).

use oia_devices::uav::{
    AltitudeReference, AltitudeSample, AttitudeDeg, BatterySample, CameraCalibration,
    ClockAlignmentRecord, FlightBookmarkInput, FlightStreamOrigin, FootprintStatus, FrameDropSpan,
    GimbalMode, GimbalOrientation, GnssFixKind, HeadingReference, HeadingSample, HudFieldState,
    LinkHealth, LinkState, MotionDynamicsFlags, NavigationHealth, OnboardRecordingLinkInput,
    OriginalFlightStreamInput, RecordingState, RegisteredUavDevice, RenderedHudDerivativeInput,
    StaleReason, TelemetryDisconnectionSpan, TelemetrySampleInput, TimeAlignment, UavAdapterKind,
    UavAdapterRegistrationInput, UavCapability, UavError, UavFlightSession, UavLimitKind,
    UavSessionLimits, UnavailableReason, UnsynchronizedReason, VelocityMps, VideoFrameInput,
    Wgs84Position,
};
use oia_domain::{ActorId, OpaqueId, SchemaVersion, VersionId};

fn sample_registered_uav() -> Result<RegisteredUavDevice, UavError> {
    RegisteredUavDevice::register(UavAdapterRegistrationInput {
        device_id: OpaqueId::new("device:a12-uav")?,
        registration_version: VersionId::new("ver:uav-reg-v1")?,
        schema_version: SchemaVersion::V1,
        registered_by: ActorId::new("actor:operator-1")?,
        adapter_kind: UavAdapterKind::RecordedFlightImport,
        // Includes ZWNJ (U+200C) and ZWJ (U+200D) to verify Persian/Indic/emoji compatibility.
        model_label: "Recon\u{200C}Quad\u{200D}V1".to_owned(),
        declared_capabilities: vec![
            UavCapability::VideoStream,
            UavCapability::TelemetryStream,
            UavCapability::RecordedMediaImport,
        ],
        auto_publish_footage_or_location: false,
    })
}

/// Risk addressed (`DEVICE-02`–`DEVICE-04`, `A12`): clock drift, dropped frames, telemetry
/// disconnections, missing fields, independent gimbal motion, calibrated footprint limitations,
/// calibration invalidation, non-blocking forensic frame handoff, and late onboard-recording
/// linkage must preserve original stream versions and never invent measurements.
#[test]
fn uav_hud_clock_drift_drops_bookmarks_and_onboard_linking_preserve_evidence()
-> Result<(), UavError> {
    let device = sample_registered_uav()?;
    let mut session = UavFlightSession::new(
        OpaqueId::new("session:a12-flight")?,
        device,
        OriginalFlightStreamInput {
            stream_id: OpaqueId::new("stream:live-downlink")?,
            stream_version: VersionId::new("ver:live-downlink-v1")?,
            origin: FlightStreamOrigin::RecordedFlightImport,
            sha256_hex: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                .to_owned(),
            byte_len: 12_500_000,
            media_type: "video/mp4".to_owned(),
        },
        UavSessionLimits::default_bounded(),
    )?;

    // Before clock alignment is set, timestamps report UnsynchronizedDegraded rather than
    // pretending receipt_utc_ms is a synchronized capture timestamp.
    assert_eq!(
        session.align_device_timestamp(10_000, 1_762_000_010_500),
        TimeAlignment::UnsynchronizedDegraded {
            device_clock_ms: 10_000,
            receipt_utc_ms: 1_762_000_010_500,
            reason: UnsynchronizedReason::NoClockAlignmentConfigured,
        }
    );

    // Configure clock alignment with +50 ppm linear drift valid on [0, 300_000) ms.
    session.set_clock_alignment(ClockAlignmentRecord {
        alignment_version: VersionId::new("ver:clock-align-v1")?,
        device_to_utc_offset_ms: 1_762_000_000_000,
        linear_drift_ppm: 50,
        uncertainty_ms: 3,
        reference_clock_label: "GNSS-PPS".to_owned(),
        method_label: "optical-slate-sync".to_owned(),
        valid_from_device_ms: 0,
        valid_until_device_ms: 300_000,
    })?;

    // At 200_000 ms elapsed, +50 ppm drift adds +10 ms to the base offset, and total_uncertainty_ms
    // reports the explicit 3 ms calibration uncertainty bound without unrequested 5 ppm padding.
    assert_eq!(
        session.align_device_timestamp(200_000, 1_762_000_200_999),
        TimeAlignment::Synchronized {
            aligned_utc_ms: 1_762_000_200_010,
            alignment_version: VersionId::new("ver:clock-align-v1")?,
            base_offset_ms: 1_762_000_000_000,
            drift_adjustment_ms: 10,
            total_uncertainty_ms: 3,
        }
    );

    // Outside the validity interval (t = 300_000 ms), alignment degrades explicitly.
    assert_eq!(
        session.align_device_timestamp(300_000, 1_762_000_300_100),
        TimeAlignment::UnsynchronizedDegraded {
            device_clock_ms: 300_000,
            receipt_utc_ms: 1_762_000_300_100,
            reason: UnsynchronizedReason::OutsideAlignmentValidityWindow {
                valid_from_device_ms: 0,
                valid_until_device_ms: 300_000,
            },
        }
    );

    // Sample 1 at t = 100_000 ms: independent gimbal motion, AGL altitude, velocity omitted (None).
    session.ingest_telemetry_sample(TelemetrySampleInput {
        sample_version: VersionId::new("ver:sample-1")?,
        device_clock_ms: 100_000,
        receipt_utc_ms: 1_762_000_100_050,
        heading: Some(HeadingSample {
            degrees: 90.0,
            reference: HeadingReference::TrueNorth,
        }),
        attitude: Some(AttitudeDeg {
            roll_deg: 0.0,
            pitch_deg: 0.0,
            yaw_deg: 90.0,
        }),
        position: Some(Wgs84Position {
            latitude_deg: 0.500_000,
            longitude_deg: 0.750_000,
            horizontal_uncertainty_m: Some(2.0),
        }),
        altitude: Some(AltitudeSample {
            meters: 100.0,
            reference: AltitudeReference::AboveGroundLevel,
            vertical_uncertainty_m: Some(1.0),
        }),
        velocity: None,
        gimbal: Some(GimbalOrientation {
            pitch_deg: -45.0,
            roll_deg: 0.0,
            yaw_deg: 180.0,
            mode: GimbalMode::IndependentWorldLocked,
        }),
        battery: Some(BatterySample {
            remaining_percent: 78.5,
            voltage_v: Some(22.4),
        }),
        link_health: Some(LinkHealth {
            state: LinkState::Nominal,
            rssi_dbm: Some(-62.0),
            packet_loss_percent: Some(0.0),
        }),
        navigation_health: Some(NavigationHealth {
            fix_kind: GnssFixKind::Standard3D,
            satellite_count: Some(14),
            hdop: Some(0.9),
        }),
        recording_state: RecordingState::Recording,
        motion_flags: MotionDynamicsFlags {
            high_vibration: true,
            rapid_gimbal_slew: true,
            rolling_shutter_risk: true,
            exposure_shift_detected: false,
        },
    })?;

    // Without camera calibration, HUD provides a directional viewing marker (establishes_slant_range == false)
    // and FootprintStatus::UncalibratedViewingRayOnly, while omitted velocity stays Unavailable.
    let uncalibrated_hud = session.project_hud_snapshot(100_500, 1_762_000_100_520);
    assert_eq!(
        uncalibrated_hud.velocity,
        HudFieldState::Unavailable {
            reason: UnavailableReason::MissingInSample {
                sample_device_ms: 100_000,
            },
        }
    );
    let marker = uncalibrated_hud
        .look_direction_marker
        .ok_or(UavError::InternalInvariant)?;
    assert!(!marker.establishes_slant_range);
    assert!((marker.world_azimuth_deg - 180.0).abs() < 1e-6);
    assert!((marker.depression_deg - 45.0).abs() < 1e-6);
    assert_eq!(
        uncalibrated_hud.footprint_status,
        FootprintStatus::UncalibratedViewingRayOnly
    );

    // Attach camera calibration: at 100 m AGL and 45 deg depression facing 180 deg (due south),
    // ground distance is 100 m and center_north_offset_m is -100 m.
    session.set_camera_calibration(CameraCalibration {
        calibration_version: VersionId::new("ver:cam-cal-v1")?,
        horizontal_fov_deg: 60.0,
        vertical_fov_deg: 40.0,
        angular_uncertainty_deg: 0.5,
    })?;
    let calibrated_hud = session.project_hud_snapshot(100_500, 1_762_000_100_520);
    match &calibrated_hud.footprint_status {
        FootprintStatus::CalibratedDerivedFootprint(fp) => {
            assert!(fp.is_derived_estimate);
            assert_eq!(fp.azimuth_reference, HeadingReference::TrueNorth);
            assert!((fp.center_ground_distance_m - 100.0).abs() < 1e-4);
            assert!((fp.center_north_offset_m - (-100.0)).abs() < 1e-4);
            assert!(fp.center_east_offset_m.abs() < 1e-4);
        }
        other => {
            return Err(UavError::InvalidNumericField {
                field: match other {
                    FootprintStatus::UncalibratedViewingRayOnly => "expected_calibrated_footprint",
                    _ => "unexpected_footprint_status",
                },
            });
        }
    }

    // Ingest video frame 100, attach a case bookmark, and export a frame-copy forensic handoff.
    session.ingest_video_frame(VideoFrameInput {
        frame_index: 100,
        frame_version: VersionId::new("ver:frame-100")?,
        device_clock_ms: 100_500,
        receipt_utc_ms: 1_762_000_100_530,
        exposure_duration_us: Some(2_500),
        sha256_hex: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
    })?;
    let bm_id = OpaqueId::new("bookmark:case-target-01")?;
    session.record_bookmark(FlightBookmarkInput {
        bookmark_id: bm_id.clone(),
        case_id: OpaqueId::new("case:a12")?,
        created_by: ActorId::new("actor:operator-1")?,
        device_clock_ms: 100_500,
        receipt_utc_ms: 1_762_000_100_540,
        pinned_frame_index: Some(100),
        label: "Target sighting during gimbal slew".to_owned(),
        note: Some("High vibration + world-locked gimbal".to_owned()),
    })?;
    let handoff = session.export_forensic_frame_handoff(
        OpaqueId::new("handoff:frame-100")?,
        100,
        Some(bm_id.clone()),
    )?;
    assert_eq!(handoff.session_id().as_str(), "session:a12-flight");
    assert_eq!(
        handoff.source_stream_version().as_str(),
        "ver:live-downlink-v1"
    );
    assert_eq!(
        handoff.source_stream_sha256_hex(),
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
    );
    assert_eq!(handoff.source_stream_byte_len(), 12_500_000);
    assert_eq!(
        handoff.hud_snapshot_at_frame().motion_flags,
        HudFieldState::Fresh {
            value: MotionDynamicsFlags {
                high_vibration: true,
                rapid_gimbal_slew: true,
                rolling_shutter_risk: true,
                exposure_shift_detected: false,
            },
            sample_device_ms: 100_000,
            age_ms: 500,
        }
    );
    let rendered_hud = handoff
        .hud_snapshot_at_frame()
        .render_hud_text(session.limits().max_output_bytes())?;
    assert!(rendered_hud.contains(
        "motion_flags: FRESH [sample_ms=100000, age=500 ms] vibration=true, gimbal_slew=true, rolling_shutter=true, exposure_shift=false"
    ));
    let rendered_manifest = handoff.render_handoff_manifest(session.limits().max_output_bytes())?;
    assert!(rendered_manifest.contains("session_id: session:a12-flight"));
    assert!(rendered_manifest.contains("calibration_at_frame: Valid(CameraCalibration"));

    // Reception continues after handoff without mutating the exported handoff:
    // record frame drops, telemetry disconnection, grazing-angle sample, barometric-only sample,
    // and calibration invalidation.
    session.record_frame_drop(FrameDropSpan {
        after_frame_index: Some(100),
        dropped_frame_count: 7,
        detected_at_device_ms: 100_800,
        reason: "RF interference burst".to_owned(),
    })?;
    session.record_telemetry_disconnection(TelemetryDisconnectionSpan {
        disconnected_at_device_ms: 101_000,
        reconnected_at_device_ms: Some(102_000),
        reason: "Downlink fade".to_owned(),
    })?;

    // During disconnection at t = 101_500 ms, HUD marks telemetry (including motion_flags)
    // Stale(TelemetryLinkDisconnected) and refuses to compute a fresh calibrated footprint.
    let disconnected_hud = session.project_hud_snapshot(101_500, 1_762_000_101_550);
    assert!(matches!(
        disconnected_hud.heading,
        HudFieldState::Stale {
            reason: StaleReason::TelemetryLinkDisconnected {
                disconnected_at_device_ms: 101_000
            },
            ..
        }
    ));
    assert!(matches!(
        disconnected_hud.motion_flags,
        HudFieldState::Stale {
            reason: StaleReason::TelemetryLinkDisconnected {
                disconnected_at_device_ms: 101_000
            },
            ..
        }
    ));
    assert_eq!(disconnected_hud.cumulative_dropped_frames, 7);
    assert_eq!(
        disconnected_hud.footprint_status,
        FootprintStatus::InsufficientFreshTelemetry {
            missing_or_stale_field: "heading",
        }
    );

    // Sample 2 at t = 105_000 ms: grazing gimbal pitch (-2 deg -> depression 2 deg < 5 deg) refuses unique range.
    session.ingest_telemetry_sample(TelemetrySampleInput {
        sample_version: VersionId::new("ver:sample-2")?,
        device_clock_ms: 105_000,
        receipt_utc_ms: 1_762_000_105_020,
        heading: Some(HeadingSample {
            degrees: 90.0,
            reference: HeadingReference::TrueNorth,
        }),
        attitude: Some(AttitudeDeg {
            roll_deg: 0.0,
            pitch_deg: 0.0,
            yaw_deg: 90.0,
        }),
        position: Some(Wgs84Position {
            latitude_deg: 0.500_100,
            longitude_deg: 0.750_100,
            horizontal_uncertainty_m: Some(2.0),
        }),
        altitude: Some(AltitudeSample {
            meters: 100.0,
            reference: AltitudeReference::AboveGroundLevel,
            vertical_uncertainty_m: Some(1.0),
        }),
        velocity: Some(VelocityMps {
            ground_speed_mps: 8.0,
            vertical_speed_mps: Some(0.0),
        }),
        gimbal: Some(GimbalOrientation {
            pitch_deg: -2.0,
            roll_deg: 0.0,
            yaw_deg: 0.0,
            mode: GimbalMode::FollowHeading,
        }),
        battery: None,
        link_health: None,
        navigation_health: None,
        recording_state: RecordingState::Recording,
        motion_flags: MotionDynamicsFlags::default(),
    })?;
    let grazing_hud = session.project_hud_snapshot(105_100, 1_762_000_105_120);
    assert_eq!(
        grazing_hud.footprint_status,
        FootprintStatus::UnresolvableGeometry {
            depression_deg: -18.0,
            min_required_depression_deg: 5.0,
        }
    );

    // Sample 3 at t = 106_000 ms: BarometricUnadjusted altitude refuses ground footprint projection.
    session.ingest_telemetry_sample(TelemetrySampleInput {
        sample_version: VersionId::new("ver:sample-3")?,
        device_clock_ms: 106_000,
        receipt_utc_ms: 1_762_000_106_020,
        heading: Some(HeadingSample {
            degrees: 90.0,
            reference: HeadingReference::TrueNorth,
        }),
        attitude: Some(AttitudeDeg {
            roll_deg: 0.0,
            pitch_deg: 0.0,
            yaw_deg: 90.0,
        }),
        position: Some(Wgs84Position {
            latitude_deg: 0.500_100,
            longitude_deg: 0.750_100,
            horizontal_uncertainty_m: Some(2.0),
        }),
        altitude: Some(AltitudeSample {
            meters: 450.0,
            reference: AltitudeReference::BarometricUnadjusted,
            vertical_uncertainty_m: Some(5.0),
        }),
        velocity: None,
        gimbal: Some(GimbalOrientation {
            pitch_deg: -45.0,
            roll_deg: 0.0,
            yaw_deg: 0.0,
            mode: GimbalMode::FollowHeading,
        }),
        battery: None,
        link_health: None,
        navigation_health: None,
        recording_state: RecordingState::Recording,
        motion_flags: MotionDynamicsFlags::default(),
    })?;
    let baro_hud = session.project_hud_snapshot(106_100, 1_762_000_106_120);
    assert_eq!(
        baro_hud.footprint_status,
        FootprintStatus::IncompatibleAltitudeReference {
            actual: AltitudeReference::BarometricUnadjusted,
        }
    );

    // Camera movement invalidates calibration and flags FootprintStatus::CalibrationInvalidated.
    session.invalidate_camera_calibration("Hard wind gust shifted gimbal mount stop")?;
    let invalidated_hud = session.project_hud_snapshot(106_100, 1_762_000_106_120);
    assert_eq!(
        invalidated_hud.footprint_status,
        FootprintStatus::CalibrationInvalidated {
            calibration_version: VersionId::new("ver:cam-cal-v1")?,
            reason: "Hard wind gust shifted gimbal mount stop".to_owned(),
        }
    );

    // Registering a rendered HUD derivative keeps can_replace_original_evidence() == false, and
    // later linking onboard footage preserves original stream and bookmark versions unchanged.
    let deriv = session.register_hud_derivative(RenderedHudDerivativeInput {
        derivative_id: OpaqueId::new("artifact:hud-render-1")?,
        derivative_version: VersionId::new("ver:hud-render-v1")?,
        derived_from_stream_version: VersionId::new("ver:live-downlink-v1")?,
        sha256_hex: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned(),
        byte_len: 4_096_000,
        media_type: "video/mp4".to_owned(),
        overlay_configuration_label: "HUD overlay v1".to_owned(),
    })?;
    assert!(!deriv.can_replace_original_evidence());

    let onboard = session.link_onboard_recording(OnboardRecordingLinkInput {
        link_id: OpaqueId::new("link:onboard-sd-1")?,
        onboard_stream_version: VersionId::new("ver:onboard-sd-v1")?,
        sha256_hex: "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".to_owned(),
        byte_len: 98_000_000,
        media_type: "video/mp4".to_owned(),
        stream_to_onboard_offset_ms: -8,
        linked_bookmark_ids: vec![bm_id.clone()],
        alignment_note: "Aligned by GNSS PPS trigger".to_owned(),
    })?;
    assert!(!onboard.overwrote_prior_evidence());
    assert_eq!(
        session.original_stream().stream_version().as_str(),
        "ver:live-downlink-v1"
    );
    let stored_bm = session
        .bookmark(&bm_id)
        .ok_or(UavError::InternalInvariant)?;
    assert_eq!(
        stored_bm.pinned_stream_version().as_str(),
        "ver:live-downlink-v1"
    );
    assert_eq!(
        handoff.source_stream_version().as_str(),
        "ver:live-downlink-v1"
    );

    Ok(())
}

/// Risk addressed (`DEVICE-01`, `DEVICE-04`, `AGENTS.md` §§2, 4, 5, 6): initial UAV adapters must
/// refuse flight-control capabilities and commands, forbid automatic local publication, enforce
/// capability separation between video and telemetry, reject incompatible altitude datum
/// conversions, reject positions when GNSS has no fix, and reject control/bidi text injection or
/// duplicate versions.
#[test]
fn rejects_flight_control_auto_publish_capability_violations_and_malformed_inputs()
-> Result<(), UavError> {
    // 1. FlightControl capability is rejected at registration (DEVICE-04, A12).
    let fc_reg = UavAdapterRegistrationInput {
        device_id: OpaqueId::new("device:fc-attempt")?,
        registration_version: VersionId::new("ver:fc-reg-v1")?,
        schema_version: SchemaVersion::V1,
        registered_by: ActorId::new("actor:operator-1")?,
        adapter_kind: UavAdapterKind::LocalTelemetryLink,
        model_label: "Quadrotor".to_owned(),
        declared_capabilities: vec![UavCapability::TelemetryStream, UavCapability::FlightControl],
        auto_publish_footage_or_location: false,
    };
    assert_eq!(
        Err(UavError::FlightControlUnsupported {
            device_id: OpaqueId::new("device:fc-attempt")?,
        }),
        RegisteredUavDevice::register(fc_reg)
    );

    // 2. Automatic publication of local footage/location is forbidden (DEVICE-01).
    let auto_pub_reg = UavAdapterRegistrationInput {
        device_id: OpaqueId::new("device:auto-pub")?,
        registration_version: VersionId::new("ver:auto-pub-v1")?,
        schema_version: SchemaVersion::V1,
        registered_by: ActorId::new("actor:operator-1")?,
        adapter_kind: UavAdapterKind::NetworkVideoStream,
        model_label: "Quadrotor".to_owned(),
        declared_capabilities: vec![UavCapability::VideoStream],
        auto_publish_footage_or_location: true,
    };
    assert_eq!(
        Err(UavError::AutoPublicationForbidden {
            device_id: OpaqueId::new("device:auto-pub")?,
        }),
        RegisteredUavDevice::register(auto_pub_reg)
    );

    // 3. Emitting a flight-control command against a valid receive-only device is forbidden (A12).
    let video_only_device = RegisteredUavDevice::register(UavAdapterRegistrationInput {
        device_id: OpaqueId::new("device:video-only")?,
        registration_version: VersionId::new("ver:video-only-v1")?,
        schema_version: SchemaVersion::V1,
        registered_by: ActorId::new("actor:operator-1")?,
        adapter_kind: UavAdapterKind::NetworkVideoStream,
        model_label: "Video-Only Receiver".to_owned(),
        declared_capabilities: vec![UavCapability::VideoStream],
        auto_publish_footage_or_location: false,
    })?;
    assert!(!video_only_device.can_emit_flight_control());
    assert_eq!(
        Err(UavError::FlightControlCommandForbidden {
            device_id: OpaqueId::new("device:video-only")?,
            command: "ARM_AND_TAKEOFF".to_owned(),
        }),
        video_only_device.attempt_flight_control_command("ARM_AND_TAKEOFF")
    );

    // 4. Rendered HUD overlay derivative cannot be registered as an original flight stream (DEVICE-03).
    assert!(matches!(
        UavFlightSession::new(
            OpaqueId::new("session:bad-origin")?,
            video_only_device.clone(),
            OriginalFlightStreamInput {
                stream_id: OpaqueId::new("stream:hud-burn-in")?,
                stream_version: VersionId::new("ver:hud-burn-in-v1")?,
                origin: FlightStreamOrigin::RenderedHudOverlayDerivative,
                sha256_hex: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_owned(),
                byte_len: 1024,
                media_type: "video/mp4".to_owned(),
            },
            UavSessionLimits::default_bounded(),
        ),
        Err(UavError::HudDerivativeCannotBeOriginalStream { .. })
    ));

    // 5. VideoStream capability alone does not permit ingest_telemetry_sample (AGENTS.md §4),
    // and projecting a HUD snapshot reports FootprintStatus::TelemetryCapabilityNotDeclared.
    let mut video_only_session = UavFlightSession::new(
        OpaqueId::new("session:video-only")?,
        video_only_device,
        OriginalFlightStreamInput {
            stream_id: OpaqueId::new("stream:video-1")?,
            stream_version: VersionId::new("ver:video-1")?,
            origin: FlightStreamOrigin::LiveSessionCapture,
            sha256_hex: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                .to_owned(),
            byte_len: 1024,
            media_type: "video/mp4".to_owned(),
        },
        UavSessionLimits::default_bounded(),
    )?;
    assert_eq!(
        video_only_session
            .project_hud_snapshot(1_000, 1_762_000_001_000)
            .footprint_status,
        FootprintStatus::TelemetryCapabilityNotDeclared
    );
    assert_eq!(
        Err(UavError::CapabilityNotDeclared {
            device_id: OpaqueId::new("device:video-only")?,
            required: UavCapability::TelemetryStream,
        }),
        video_only_session.ingest_telemetry_sample(TelemetrySampleInput {
            sample_version: VersionId::new("ver:telem-unpermitted")?,
            device_clock_ms: 1_000,
            receipt_utc_ms: 1_762_000_001_000,
            heading: None,
            attitude: None,
            position: None,
            altitude: None,
            velocity: None,
            gimbal: None,
            battery: None,
            link_health: None,
            navigation_health: None,
            recording_state: RecordingState::Unavailable,
            motion_flags: MotionDynamicsFlags::default(),
        })
    );

    // 6. Barometric altitude cannot be extracted as OrthometricMsl, and AltitudeReference::Unknown
    // is never convertible via try_in_reference (docs/architecture.md §3.2).
    let baro = AltitudeSample {
        meters: 310.0,
        reference: AltitudeReference::BarometricUnadjusted,
        vertical_uncertainty_m: Some(4.0),
    };
    assert_eq!(
        Err(UavError::IncompatibleAltitudeReference {
            actual: AltitudeReference::BarometricUnadjusted,
            requested: AltitudeReference::OrthometricMsl,
        }),
        baro.try_in_reference(AltitudeReference::OrthometricMsl)
    );
    let unknown_alt = AltitudeSample {
        meters: 100.0,
        reference: AltitudeReference::Unknown,
        vertical_uncertainty_m: None,
    };
    assert_eq!(
        Err(UavError::IncompatibleAltitudeReference {
            actual: AltitudeReference::Unknown,
            requested: AltitudeReference::Unknown,
        }),
        unknown_alt.try_in_reference(AltitudeReference::Unknown)
    );

    // 7. GnssFixKind::NoFix with a fabricated position is rejected, as are bidi/control chars and
    // overwriting the original stream version when linking onboard recordings.
    let mut full_session = UavFlightSession::new(
        OpaqueId::new("session:validation")?,
        sample_registered_uav()?,
        OriginalFlightStreamInput {
            stream_id: OpaqueId::new("stream:orig")?,
            stream_version: VersionId::new("ver:orig-stream-v1")?,
            origin: FlightStreamOrigin::RecordedFlightImport,
            sha256_hex: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                .to_owned(),
            byte_len: 2048,
            media_type: "video/mp4".to_owned(),
        },
        UavSessionLimits::default_bounded(),
    )?;

    assert_eq!(
        Err(UavError::PositionReportedWithoutGnssFix),
        full_session.ingest_telemetry_sample(TelemetrySampleInput {
            sample_version: VersionId::new("ver:no-fix-pos")?,
            device_clock_ms: 1_000,
            receipt_utc_ms: 1_762_000_001_000,
            heading: None,
            attitude: None,
            position: Some(Wgs84Position {
                latitude_deg: 0.0,
                longitude_deg: 0.0,
                horizontal_uncertainty_m: None,
            }),
            altitude: None,
            velocity: None,
            gimbal: None,
            battery: None,
            link_health: None,
            navigation_health: Some(NavigationHealth {
                fix_kind: GnssFixKind::NoFix,
                satellite_count: Some(0),
                hdop: None,
            }),
            recording_state: RecordingState::Unavailable,
            motion_flags: MotionDynamicsFlags::default(),
        })
    );

    // Bidi override U+202E in a bookmark label is rejected.
    assert_eq!(
        Err(UavError::ForbiddenTextCharacter {
            field: "bookmark.label",
            codepoint: 0x202E,
        }),
        full_session.record_bookmark(FlightBookmarkInput {
            bookmark_id: OpaqueId::new("bookmark:bidi")?,
            case_id: OpaqueId::new("case:a12")?,
            created_by: ActorId::new("actor:operator-1")?,
            device_clock_ms: 1_000,
            receipt_utc_ms: 1_762_000_001_000,
            pinned_frame_index: None,
            label: "Spoofed\u{202E}Label".to_owned(),
            note: None,
        })
    );

    // Reusing `ver:orig-stream-v1` when linking onboard footage is rejected.
    assert_eq!(
        Err(UavError::DuplicateOrOverwrittenVersion {
            version_id: VersionId::new("ver:orig-stream-v1")?,
        }),
        full_session.link_onboard_recording(OnboardRecordingLinkInput {
            link_id: OpaqueId::new("link:overwrite-attempt")?,
            onboard_stream_version: VersionId::new("ver:orig-stream-v1")?,
            sha256_hex: "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
                .to_owned(),
            byte_len: 8192,
            media_type: "video/mp4".to_owned(),
            stream_to_onboard_offset_ms: 0,
            linked_bookmark_ids: Vec::new(),
            alignment_note: "Attempted overwrite".to_owned(),
        })
    );

    // Output budget overflow on HUD rendering returns LimitExceeded(OutputBytes).
    let snap = full_session.project_hud_snapshot(1_000, 1_762_000_001_000);
    assert_eq!(
        Err(UavError::LimitExceeded {
            kind: UavLimitKind::OutputBytes,
            limit: 16,
        }),
        snap.render_hud_text(16)
    );

    // 8. Signed integer min-value clock offsets/drifts (`i64::MIN`, `i32::MIN`), zero uncertainty,
    // and near-`i64::MAX` timestamp alignment overflow are safely handled without panics or
    // silent saturation (`B1`, `B2`).
    assert_eq!(
        Err(UavError::ClockOffsetOutOfRange {
            offset_ms: i64::MIN,
        }),
        full_session.set_clock_alignment(ClockAlignmentRecord {
            alignment_version: VersionId::new("ver:clock-min-offset")?,
            device_to_utc_offset_ms: i64::MIN,
            linear_drift_ppm: 0,
            uncertainty_ms: 5,
            reference_clock_label: "GNSS".to_owned(),
            method_label: "PPS".to_owned(),
            valid_from_device_ms: 0,
            valid_until_device_ms: 1_000,
        })
    );
    assert_eq!(
        Err(UavError::ClockDriftOutOfRange {
            drift_ppm: i32::MIN,
        }),
        full_session.set_clock_alignment(ClockAlignmentRecord {
            alignment_version: VersionId::new("ver:clock-min-drift")?,
            device_to_utc_offset_ms: 0,
            linear_drift_ppm: i32::MIN,
            uncertainty_ms: 5,
            reference_clock_label: "GNSS".to_owned(),
            method_label: "PPS".to_owned(),
            valid_from_device_ms: 0,
            valid_until_device_ms: 1_000,
        })
    );
    assert_eq!(
        Err(UavError::ZeroClockUncertainty),
        full_session.set_clock_alignment(ClockAlignmentRecord {
            alignment_version: VersionId::new("ver:clock-zero-unc")?,
            device_to_utc_offset_ms: 0,
            linear_drift_ppm: 0,
            uncertainty_ms: 0,
            reference_clock_label: "GNSS".to_owned(),
            method_label: "PPS".to_owned(),
            valid_from_device_ms: 0,
            valid_until_device_ms: 1_000,
        })
    );
    assert_eq!(
        Err(UavError::ClockOffsetOutOfRange {
            offset_ms: i64::MIN,
        }),
        full_session.link_onboard_recording(OnboardRecordingLinkInput {
            link_id: OpaqueId::new("link:min-offset")?,
            onboard_stream_version: VersionId::new("ver:onboard-min-offset")?,
            sha256_hex: "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
                .to_owned(),
            byte_len: 4096,
            media_type: "video/mp4".to_owned(),
            stream_to_onboard_offset_ms: i64::MIN,
            linked_bookmark_ids: Vec::new(),
            alignment_note: "Overflow check".to_owned(),
        })
    );

    full_session.set_clock_alignment(ClockAlignmentRecord {
        alignment_version: VersionId::new("ver:clock-near-max")?,
        device_to_utc_offset_ms: 100_000,
        linear_drift_ppm: 0,
        uncertainty_ms: 5,
        reference_clock_label: "GNSS".to_owned(),
        method_label: "PPS".to_owned(),
        valid_from_device_ms: i64::MAX - 50_000,
        valid_until_device_ms: i64::MAX,
    })?;
    assert_eq!(
        full_session.align_device_timestamp(i64::MAX - 10, 1_762_000_000_000),
        TimeAlignment::UnsynchronizedDegraded {
            device_clock_ms: i64::MAX - 10,
            receipt_utc_ms: 1_762_000_000_000,
            reason: UnsynchronizedReason::AlignmentArithmeticOverflow,
        }
    );

    Ok(())
}

/// Risk addressed (`DEVICE-02`–`DEVICE-04`, `A12`, review items `B3`–`B6`, `Codex P2 #1`–`#3`):
/// nadir and past-nadir footprint geometry, world-locked vs. body-relative gimbal composition,
/// non-zero roll and non-TrueNorth footprint refusals, shallow-angle uncertainty propagation,
/// atomic `invalidate_camera_calibration` validation, bounded calibration history, and historical
/// forensic handoff immutability across later calibration or clock updates.
#[test]
fn footprint_nadir_geometry_uncertainty_and_historical_handoff_isolation() -> Result<(), UavError> {
    let mut session = UavFlightSession::new(
        OpaqueId::new("session:geometry-and-history")?,
        sample_registered_uav()?,
        OriginalFlightStreamInput {
            stream_id: OpaqueId::new("stream:geo-1")?,
            stream_version: VersionId::new("ver:geo-stream-v1")?,
            origin: FlightStreamOrigin::RecordedFlightImport,
            sha256_hex: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                .to_owned(),
            byte_len: 10_000,
            media_type: "video/mp4".to_owned(),
        },
        UavSessionLimits::default_bounded().with_max_calibration_updates(4)?,
    )?;

    session.set_clock_alignment(ClockAlignmentRecord {
        alignment_version: VersionId::new("ver:clock-c1")?,
        device_to_utc_offset_ms: 1_762_000_000_000,
        linear_drift_ppm: 0,
        uncertainty_ms: 4,
        reference_clock_label: "GNSS".to_owned(),
        method_label: "PPS".to_owned(),
        valid_from_device_ms: 0,
        valid_until_device_ms: 500_000,
    })?;

    session.set_camera_calibration(CameraCalibration {
        calibration_version: VersionId::new("ver:cal-v1")?,
        horizontal_fov_deg: 45.0,
        vertical_fov_deg: 45.0,
        angular_uncertainty_deg: 0.5,
    })?;

    // 1. Nadir camera (gimbal pitch -90 deg) at 100 m AGL with 45 deg vertical FOV:
    // symmetric frustum spans 2 * 100 * tan(22.5 deg) = 82.8427124746 m (Codex P2 #2).
    session.ingest_telemetry_sample(TelemetrySampleInput {
        sample_version: VersionId::new("ver:geo-sample-1")?,
        device_clock_ms: 1_000,
        receipt_utc_ms: 1_762_000_001_000,
        heading: Some(HeadingSample {
            degrees: 0.0,
            reference: HeadingReference::TrueNorth,
        }),
        attitude: Some(AttitudeDeg {
            roll_deg: 0.0,
            pitch_deg: 0.0,
            yaw_deg: 0.0,
        }),
        position: Some(Wgs84Position {
            latitude_deg: 0.100_000,
            longitude_deg: 0.200_000,
            horizontal_uncertainty_m: Some(1.0),
        }),
        altitude: Some(AltitudeSample {
            meters: 100.0,
            reference: AltitudeReference::AboveGroundLevel,
            vertical_uncertainty_m: Some(0.5),
        }),
        velocity: None,
        gimbal: Some(GimbalOrientation {
            pitch_deg: -90.0,
            roll_deg: 0.0,
            yaw_deg: 0.0,
            mode: GimbalMode::IndependentWorldLocked,
        }),
        battery: None,
        link_health: None,
        navigation_health: None,
        recording_state: RecordingState::Recording,
        motion_flags: MotionDynamicsFlags::default(),
    })?;
    session.ingest_video_frame(VideoFrameInput {
        frame_index: 1,
        frame_version: VersionId::new("ver:frame-1")?,
        device_clock_ms: 1_000,
        receipt_utc_ms: 1_762_000_001_000,
        exposure_duration_us: Some(2_000),
        sha256_hex: "1111111111111111111111111111111111111111111111111111111111111111".to_owned(),
    })?;

    let nadir_hud = session.project_hud_snapshot(1_000, 1_762_000_001_000);
    match &nadir_hud.footprint_status {
        FootprintStatus::CalibratedDerivedFootprint(fp) => {
            assert!(fp.center_ground_distance_m.abs() < 1e-6);
            assert!((fp.along_look_span_m - 82.842_712_474_6).abs() < 1e-6);
            assert!((fp.cross_look_span_m - 82.842_712_474_6).abs() < 1e-6);
        }
        other => {
            return Err(UavError::InvalidNumericField {
                field: match other {
                    FootprintStatus::UncalibratedViewingRayOnly => "expected_nadir_footprint",
                    _ => "unexpected_nadir_status",
                },
            });
        }
    }

    // 2. Rejected invalidate_camera_calibration("") leaves active calibration intact (Codex P2 #1).
    assert_eq!(
        Err(UavError::EmptyTextField {
            field: "invalidation_reason",
        }),
        session.invalidate_camera_calibration("   ")
    );
    assert!(matches!(
        session.camera_calibration_state(),
        oia_devices::uav::CameraCalibrationState::Valid(cal)
            if cal.calibration_version.as_str() == "ver:cal-v1"
    ));

    // 3. Invalidate v1 at t = 1_000 ms, then install v2 at t = 2_000 ms and clock c2 at t = 2_000 ms:
    // exporting frame 1 (@1000 ms) must preserve CalibrationInvalidated(v1) and clock alignment c1 (B6).
    session.invalidate_camera_calibration("Mount shock at 1000ms")?;

    // Install v2 after ingesting telemetry at t = 2_000 ms:
    // Past-nadir gimbal (-120 deg at 0 deg heading, 100 m AGL, 10 deg FOV) folds to 60 deg depression
    // along 180 deg azimuth (center_north_offset_m ≈ -57.735 m south), and IndependentWorldLocked
    // ignores non-zero airframe pitch (+15 deg) and MagneticNorth airframe heading (B3, B4).
    session.ingest_telemetry_sample(TelemetrySampleInput {
        sample_version: VersionId::new("ver:geo-sample-2")?,
        device_clock_ms: 2_000,
        receipt_utc_ms: 1_762_000_002_000,
        heading: Some(HeadingSample {
            degrees: 0.0,
            reference: HeadingReference::MagneticNorth,
        }),
        attitude: Some(AttitudeDeg {
            roll_deg: 0.0,
            pitch_deg: 15.0,
            yaw_deg: 0.0,
        }),
        position: Some(Wgs84Position {
            latitude_deg: 0.100_000,
            longitude_deg: 0.200_000,
            horizontal_uncertainty_m: Some(1.0),
        }),
        altitude: Some(AltitudeSample {
            meters: 100.0,
            reference: AltitudeReference::AboveGroundLevel,
            vertical_uncertainty_m: Some(0.5),
        }),
        velocity: None,
        gimbal: Some(GimbalOrientation {
            pitch_deg: -120.0,
            roll_deg: 0.0,
            yaw_deg: 0.0,
            mode: GimbalMode::IndependentWorldLocked,
        }),
        battery: None,
        link_health: None,
        navigation_health: None,
        recording_state: RecordingState::Recording,
        motion_flags: MotionDynamicsFlags::default(),
    })?;
    session.set_camera_calibration(CameraCalibration {
        calibration_version: VersionId::new("ver:cal-v2")?,
        horizontal_fov_deg: 10.0,
        vertical_fov_deg: 4.0,
        angular_uncertainty_deg: 1.0,
    })?;
    session.set_clock_alignment(ClockAlignmentRecord {
        alignment_version: VersionId::new("ver:clock-c2")?,
        device_to_utc_offset_ms: 1_762_000_000_500,
        linear_drift_ppm: 0,
        uncertainty_ms: 2,
        reference_clock_label: "GNSS".to_owned(),
        method_label: "PPS-recal".to_owned(),
        valid_from_device_ms: 0,
        valid_until_device_ms: 500_000,
    })?;

    // Historical handoff for frame 1 (@1000 ms) preserves clock-c1 and CalibrationInvalidated(cal-v1).
    let handoff_f1 =
        session.export_forensic_frame_handoff(OpaqueId::new("handoff:f1")?, 1, None)?;
    assert_eq!(
        handoff_f1.frame().time_alignment(),
        &TimeAlignment::Synchronized {
            aligned_utc_ms: 1_762_000_001_000,
            alignment_version: VersionId::new("ver:clock-c1")?,
            base_offset_ms: 1_762_000_000_000,
            drift_adjustment_ms: 0,
            total_uncertainty_ms: 4,
        }
    );
    assert_eq!(
        &handoff_f1.hud_snapshot_at_frame().time_alignment,
        handoff_f1.frame().time_alignment()
    );
    assert_eq!(
        handoff_f1.hud_snapshot_at_frame().footprint_status,
        FootprintStatus::CalibrationInvalidated {
            calibration_version: VersionId::new("ver:cal-v1")?,
            reason: "Mount shock at 1000ms".to_owned(),
        }
    );

    // At t = 2_000 ms, past-nadir gimbal (-120 deg) under cal-v2 yields 60 deg depression along 180 deg TrueNorth.
    let hud_2000 = session.project_hud_snapshot(2_000, 1_762_000_002_500);
    let marker_2000 = hud_2000
        .look_direction_marker
        .ok_or(UavError::InternalInvariant)?;
    assert_eq!(marker_2000.azimuth_reference, HeadingReference::TrueNorth);
    assert!((marker_2000.world_azimuth_deg - 180.0).abs() < 1e-6);
    assert!((marker_2000.depression_deg - 60.0).abs() < 1e-6);
    match &hud_2000.footprint_status {
        FootprintStatus::CalibratedDerivedFootprint(fp) => {
            assert_eq!(fp.calibration_version.as_str(), "ver:cal-v2");
            assert_eq!(fp.azimuth_reference, HeadingReference::TrueNorth);
            assert!((fp.center_ground_distance_m - 57.735_026_9).abs() < 1e-4);
            assert!((fp.center_north_offset_m - (-57.735_026_9)).abs() < 1e-4);
        }
        other => {
            return Err(UavError::InvalidNumericField {
                field: match other {
                    FootprintStatus::UncalibratedViewingRayOnly => "expected_past_nadir_footprint",
                    _ => "unexpected_past_nadir_status",
                },
            });
        }
    }

    // 4. Shallow depression (7 deg at 100 m AGL with 1 deg angular uncertainty) propagates
    // d(h * cot(theta))/d(theta) = h / sin^2(theta) > 115 m horizontal uncertainty (B5).
    session.ingest_telemetry_sample(TelemetrySampleInput {
        sample_version: VersionId::new("ver:geo-sample-3")?,
        device_clock_ms: 3_000,
        receipt_utc_ms: 1_762_000_003_500,
        heading: Some(HeadingSample {
            degrees: 0.0,
            reference: HeadingReference::TrueNorth,
        }),
        attitude: Some(AttitudeDeg {
            roll_deg: 0.0,
            pitch_deg: 0.0,
            yaw_deg: 0.0,
        }),
        position: Some(Wgs84Position {
            latitude_deg: 0.100_000,
            longitude_deg: 0.200_000,
            horizontal_uncertainty_m: Some(1.0),
        }),
        altitude: Some(AltitudeSample {
            meters: 100.0,
            reference: AltitudeReference::AboveGroundLevel,
            vertical_uncertainty_m: Some(0.5),
        }),
        velocity: None,
        gimbal: Some(GimbalOrientation {
            pitch_deg: -7.0,
            roll_deg: 0.0,
            yaw_deg: 0.0,
            mode: GimbalMode::FollowHeading,
        }),
        battery: None,
        link_health: None,
        navigation_health: None,
        recording_state: RecordingState::Recording,
        motion_flags: MotionDynamicsFlags::default(),
    })?;
    let shallow_hud = session.project_hud_snapshot(3_000, 1_762_000_003_500);
    match &shallow_hud.footprint_status {
        FootprintStatus::CalibratedDerivedFootprint(fp) => {
            assert!(fp.horizontal_uncertainty_m > 115.0);
        }
        other => {
            return Err(UavError::InvalidNumericField {
                field: match other {
                    FootprintStatus::UncalibratedViewingRayOnly => "expected_shallow_footprint",
                    _ => "unexpected_shallow_status",
                },
            });
        }
    }

    // 5. FollowHeading with MagneticNorth and non-zero roll are explicitly refused (B3, B4).
    session.ingest_telemetry_sample(TelemetrySampleInput {
        sample_version: VersionId::new("ver:geo-sample-4")?,
        device_clock_ms: 4_000,
        receipt_utc_ms: 1_762_000_004_500,
        heading: Some(HeadingSample {
            degrees: 10.0,
            reference: HeadingReference::MagneticNorth,
        }),
        attitude: Some(AttitudeDeg {
            roll_deg: 0.0,
            pitch_deg: 0.0,
            yaw_deg: 10.0,
        }),
        position: Some(Wgs84Position {
            latitude_deg: 0.100_000,
            longitude_deg: 0.200_000,
            horizontal_uncertainty_m: Some(1.0),
        }),
        altitude: Some(AltitudeSample {
            meters: 100.0,
            reference: AltitudeReference::AboveGroundLevel,
            vertical_uncertainty_m: Some(0.5),
        }),
        velocity: None,
        gimbal: Some(GimbalOrientation {
            pitch_deg: -45.0,
            roll_deg: 0.0,
            yaw_deg: 0.0,
            mode: GimbalMode::FollowHeading,
        }),
        battery: None,
        link_health: None,
        navigation_health: None,
        recording_state: RecordingState::Recording,
        motion_flags: MotionDynamicsFlags::default(),
    })?;
    assert_eq!(
        session
            .project_hud_snapshot(4_000, 1_762_000_004_500)
            .footprint_status,
        FootprintStatus::IncompatibleHeadingReference {
            actual: HeadingReference::MagneticNorth,
        }
    );

    session.ingest_telemetry_sample(TelemetrySampleInput {
        sample_version: VersionId::new("ver:geo-sample-5")?,
        device_clock_ms: 5_000,
        receipt_utc_ms: 1_762_000_005_500,
        heading: Some(HeadingSample {
            degrees: 10.0,
            reference: HeadingReference::TrueNorth,
        }),
        attitude: Some(AttitudeDeg {
            roll_deg: 12.5,
            pitch_deg: 0.0,
            yaw_deg: 10.0,
        }),
        position: Some(Wgs84Position {
            latitude_deg: 0.100_000,
            longitude_deg: 0.200_000,
            horizontal_uncertainty_m: Some(1.0),
        }),
        altitude: Some(AltitudeSample {
            meters: 100.0,
            reference: AltitudeReference::AboveGroundLevel,
            vertical_uncertainty_m: Some(0.5),
        }),
        velocity: None,
        gimbal: Some(GimbalOrientation {
            pitch_deg: -45.0,
            roll_deg: 0.0,
            yaw_deg: 0.0,
            mode: GimbalMode::FollowHeading,
        }),
        battery: None,
        link_health: None,
        navigation_health: None,
        recording_state: RecordingState::Recording,
        motion_flags: MotionDynamicsFlags::default(),
    })?;
    assert_eq!(
        session
            .project_hud_snapshot(5_000, 1_762_000_005_500)
            .footprint_status,
        FootprintStatus::NonZeroRollUnsupported {
            effective_roll_deg: 12.5,
        }
    );

    // 6. Calibration updates are bounded by max_calibration_updates (4) (Codex P2 #3).
    // Four calibration/clock updates (clock-c1, cal-v1, cal-v2, clock-c2) have been recorded,
    // so the 5th update hits LimitExceeded.
    assert_eq!(
        Err(UavError::LimitExceeded {
            kind: UavLimitKind::CalibrationUpdates,
            limit: 4,
        }),
        session.set_camera_calibration(CameraCalibration {
            calibration_version: VersionId::new("ver:cal-v3")?,
            horizontal_fov_deg: 50.0,
            vertical_fov_deg: 40.0,
            angular_uncertainty_deg: 0.5,
        })
    );

    Ok(())
}
