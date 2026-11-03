//! Deterministic synthetic demonstration of camera directory registration,
//! viewing-cone geometry projection, periodic still freshness vs. stale-hold
//! downgrade, restricted-source cutoff and explicit re-registration, bounded
//! retention ring-buffer eviction, and retained clip manifest handoff into
//! [`CameraAnomalyMonitor`] (`#28`, `CAM-01`, `CAM-02`, `CAM-04`, `A09`).

use oia_domain::{ActorId, OpaqueId, SchemaVersion, VersionId};
use oia_media::camera_anomaly::{
    AnomalyCategory, AnomalyLimits, CameraAnomalyMonitor, CameraHealthState, DetectionRegionConfig,
    DetectorRunMetadata, FeedObservationKind, FrameArtifactOrigin, ObservationStepInput,
    SceneBaselineKind, SceneTriggerInput, SourceUsePermission,
};
use oia_media::camera_directory::{
    ArchiveCapability, CameraDirectoryError, CameraDirectoryLimits,
    CameraDirectoryRegistrationInput, CameraDirectoryRegistry, CameraFrameSubmission,
    CameraHorizontalPosition, CameraSourceTerms, CameraViewKind, CameraViewingOrientation,
    ClipExtractionRequest, IntentionalPublicationStatus, OrdinaryAccessMethod, PrivacyMaskZone,
    PrivacyViewAssessment, SourceLifecycleHealth, StillRefreshCapability, StreamCapability,
    ViewingConeClass,
};

fn hex64(digit: char) -> String {
    digit.to_string().repeat(64)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir_limits = CameraDirectoryLimits::new(8, 8, 16, 4, 16_384, 8, 8, 8, 32_768)?;
    let mut registry = CameraDirectoryRegistry::new(dir_limits);
    let operator = ActorId::new("actor:operator-demo")?;

    // 1. Register a calibrated sky/observatory camera via an existing directory listing
    // where owner intentional publication is not separately established (`CAM-01`).
    let north_id = OpaqueId::new("cam:mesa-north-sky-01")?;
    registry.register_camera(CameraDirectoryRegistrationInput {
        camera_id: north_id.clone(),
        upstream_feed_id: OpaqueId::new("feed:mesa-observatory-north")?,
        source_version: VersionId::new("srcver:mesa-north:v1")?,
        schema_version: SchemaVersion::new(1)?,
        registered_by: operator.clone(),
        registered_at_ms: 1_000,
        provider_id: OpaqueId::new("prov:mesa-public-directory")?,
        provider_attribution: "Mesa Public Sky Directory (simulated fixture)".to_owned(),
        listing_reference: "https://example.invalid/directory/mesa-north-sky-01".to_owned(),
        access_method: OrdinaryAccessMethod::ExistingDirectoryListing,
        intentional_publication: IntentionalPublicationStatus::NotEstablishedOrdinaryAccess,
        view_kind: CameraViewKind::SkyOrObservatory,
        privacy_assessment: PrivacyViewAssessment::ClearPublicView {
            assessed_by: operator.clone(),
            assessed_at_ms: 950,
        },
        stream_capability: StreamCapability::LiveStreamSupported {
            nominal_frame_interval_ms: 200,
        },
        still_capability: StillRefreshCapability::None,
        archive_capability: ArchiveCapability::VerifiedArchiveWindow {
            archive_version: VersionId::new("archver:mesa-north:2026-11")?,
            earliest_capture_ms: 0,
            latest_capture_ms: 5_000,
        },
        terms: CameraSourceTerms {
            automated_processing: SourceUsePermission::Permitted,
            clip_retention: SourceUsePermission::Permitted,
            redistribution: SourceUsePermission::Prohibited,
            max_retention_window_ms: Some(2_000),
            policy_reference: "Mesa Directory Terms v1 (retention<=2s, no redistribution)"
                .to_owned(),
            checked_at_ms: 900,
        },
        position: CameraHorizontalPosition::KnownWgs84 {
            latitude_deg: 33.4152,
            longitude_deg: -111.8315,
            horizontal_uncertainty_m: 2.5,
        },
        orientation: CameraViewingOrientation::Calibrated {
            calibration_version: VersionId::new("cal:mesa-north:v1")?,
            azimuth_deg: 270.0,
            pitch_deg: Some(25.0),
            roll_deg: Some(0.0),
            horizontal_fov_deg: 60.0,
            vertical_fov_deg: Some(40.0),
            max_angular_error_mdeg: 120,
        },
        initial_health: SourceLifecycleHealth::Available,
    })?;

    // 2. Register a periodic-still weather/traffic camera with residential exterior mask zones
    // and uncalibrated reported compass bearing (`CAM-02`).
    let traffic_id = OpaqueId::new("cam:harbor-weather-still-02")?;
    registry.register_camera(CameraDirectoryRegistrationInput {
        camera_id: traffic_id.clone(),
        upstream_feed_id: OpaqueId::new("feed:harbor-dot-still-02")?,
        source_version: VersionId::new("srcver:harbor-still:v1")?,
        schema_version: SchemaVersion::new(1)?,
        registered_by: operator.clone(),
        registered_at_ms: 1_100,
        provider_id: OpaqueId::new("prov:harbor-dot")?,
        provider_attribution: "Harbor DOT Weather Stills (simulated fixture)".to_owned(),
        listing_reference: "https://example.invalid/dot/harbor-02".to_owned(),
        access_method: OrdinaryAccessMethod::SupportedFeedOrApi,
        intentional_publication: IntentionalPublicationStatus::EstablishedByOwner,
        view_kind: CameraViewKind::ResidentialExteriorStreetView,
        privacy_assessment: PrivacyViewAssessment::RequiresMaskZones {
            assessed_by: operator.clone(),
            assessed_at_ms: 1_050,
            zones: vec![PrivacyMaskZone {
                zone_id: OpaqueId::new("zone:harbor-apt-windows")?,
                label: "Adjacent apartment second-floor windows".to_owned(),
                mask_applied: true,
            }],
        },
        stream_capability: StreamCapability::None,
        still_capability: StillRefreshCapability::PeriodicStillSupported {
            nominal_refresh_interval_ms: 5_000,
            stale_after_ms: 10_000,
        },
        archive_capability: ArchiveCapability::NoArchiveEstablished,
        terms: CameraSourceTerms {
            automated_processing: SourceUsePermission::Permitted,
            clip_retention: SourceUsePermission::Prohibited,
            redistribution: SourceUsePermission::Prohibited,
            max_retention_window_ms: None,
            policy_reference: "Harbor DOT Still Policy (live inspection only)".to_owned(),
            checked_at_ms: 1_000,
        },
        position: CameraHorizontalPosition::KnownWgs84 {
            latitude_deg: 33.4200,
            longitude_deg: -111.8400,
            horizontal_uncertainty_m: 15.0,
        },
        orientation: CameraViewingOrientation::ReportedApproximate {
            azimuth_deg: 180.0,
            horizontal_fov_deg: 90.0,
            bearing_uncertainty_deg: 10.0,
        },
        initial_health: SourceLifecycleHealth::Available,
    })?;

    // 3. Project viewing cones: calibrated vs. illustrative reported bearing.
    let north_cone = registry.compute_viewing_cone(&north_id, 2_500.0, 5)?;
    let traffic_cone = registry.compute_viewing_cone(&traffic_id, 1_000.0, 5)?;
    assert!(north_cone.is_calibrated());
    assert!(matches!(
        traffic_cone.cone_class(),
        ViewingConeClass::IllustrativeReportedBearing { .. }
    ));

    // 4. Ingest periodic stills on `traffic_id`: first still is fresh; unchanged still held past
    // `stale_after_ms` (10_000 ms) is downgraded to `StaleStillHold` and cannot be evidence.
    let fresh_still = registry.ingest_frame(
        &traffic_id,
        CameraFrameSubmission {
            frame_version: VersionId::new("ver:harbor:still:1")?,
            sha256_hex: hex64('a'),
            byte_len: 1_024,
            sequence_number: 1,
            declared_kind: FeedObservationKind::PeriodicUpstreamStill,
            origin: FrameArtifactOrigin::OriginalRetainedFrame,
            source_capture_time_ms: Some(2_000),
            clock_uncertainty_ms: 250,
            ingestion_time_ms: 2_100,
            archive_version: None,
        },
    )?;
    assert_eq!(
        fresh_still.effective_kind(),
        FeedObservationKind::PeriodicUpstreamStill
    );
    assert!(fresh_still.eligible_as_event_evidence());
    assert!(!fresh_still.retained_in_ring_buffer());

    let stale_still = registry.ingest_frame(
        &traffic_id,
        CameraFrameSubmission {
            frame_version: VersionId::new("ver:harbor:still:2")?,
            sha256_hex: hex64('a'),
            byte_len: 1_024,
            sequence_number: 2,
            declared_kind: FeedObservationKind::PeriodicUpstreamStill,
            origin: FrameArtifactOrigin::OriginalRetainedFrame,
            source_capture_time_ms: Some(2_000),
            clock_uncertainty_ms: 250,
            ingestion_time_ms: 15_000,
            archive_version: None,
        },
    )?;
    assert_eq!(
        stale_still.effective_kind(),
        FeedObservationKind::StaleStillHold
    );
    assert!(!stale_still.eligible_as_event_evidence());

    // 5. Transition `traffic_id` to `SourceRestricted`, verify cutoff, then explicitly
    // re-register with a new `source_version` (`CAM-01`, `A09`).
    registry.record_health_transition(
        &traffic_id,
        OpaqueId::new("trans:harbor:restricted-01")?,
        SourceLifecycleHealth::UnavailableRestricted,
        Some(CameraHealthState::SourceRestricted),
        16_000,
        "Upstream returned HTTP 403 login wall; halting acquisition".to_owned(),
    )?;
    let cutoff_err = registry.ingest_frame(
        &traffic_id,
        CameraFrameSubmission {
            frame_version: VersionId::new("ver:harbor:still:3")?,
            sha256_hex: hex64('b'),
            byte_len: 1_024,
            sequence_number: 3,
            declared_kind: FeedObservationKind::PeriodicUpstreamStill,
            origin: FrameArtifactOrigin::OriginalRetainedFrame,
            source_capture_time_ms: Some(16_500),
            clock_uncertainty_ms: 250,
            ingestion_time_ms: 16_600,
            archive_version: None,
        },
    );
    assert!(matches!(
        cutoff_err,
        Err(CameraDirectoryError::SourceRestricted { .. })
    ));

    registry.reregister_ordinarily_accessible_version(
        &traffic_id,
        OpaqueId::new("trans:harbor:reregister-02")?,
        VersionId::new("srcver:harbor-still:v2")?,
        "https://example.invalid/dot/public-v2/harbor-02".to_owned(),
        20_000,
        "Operator registered new public DOT v2 feed endpoint".to_owned(),
    )?;

    // 6. Ingest live stream frames on `north_id` across a retention window to trigger
    // deterministic FIFO window expiry (`max_retention_window_ms = 2_000`) and sequence gap tracking,
    // plus a non-evidentiary fallback placeholder that must not enter the ring buffer.
    for (ver, digit, seq, cap_ms, ing_ms) in [
        ("ver:north:f1", '1', 10_u64, 10_000_i64, 10_020_i64),
        ("ver:north:f2", '2', 11_u64, 11_500_i64, 11_520_i64),
        ("ver:north:f3", '3', 14_u64, 12_200_i64, 12_220_i64),
        ("ver:north:f4", '4', 15_u64, 12_500_i64, 12_520_i64),
        ("ver:north:f5", '5', 16_u64, 12_900_i64, 12_920_i64),
    ] {
        registry.ingest_frame(
            &north_id,
            CameraFrameSubmission {
                frame_version: VersionId::new(ver)?,
                sha256_hex: hex64(digit),
                byte_len: 2_048,
                sequence_number: seq,
                declared_kind: FeedObservationKind::UpstreamCameraFeed,
                origin: FrameArtifactOrigin::OriginalRetainedFrame,
                source_capture_time_ms: Some(cap_ms),
                clock_uncertainty_ms: 30,
                ingestion_time_ms: ing_ms,
                archive_version: None,
            },
        )?;
    }

    let placeholder = registry.ingest_frame(
        &north_id,
        CameraFrameSubmission {
            frame_version: VersionId::new("ver:north:placeholder")?,
            sha256_hex: hex64('f'),
            byte_len: 512,
            sequence_number: 17,
            declared_kind: FeedObservationKind::SyntheticPlaceholder,
            origin: FrameArtifactOrigin::OriginalRetainedFrame,
            source_capture_time_ms: None,
            clock_uncertainty_ms: 30,
            ingestion_time_ms: 12_950,
            archive_version: None,
        },
    )?;
    assert!(!placeholder.eligible_as_event_evidence());
    assert!(!placeholder.retained_in_ring_buffer());

    // 7. Extract a retained clip manifest for `[12_100, 12_600)` with a 2_500 ms pre-window
    // (`pre_start_ms = 9_600`) so the expired frame at 10_000 ms surfaces as `PartialBufferExpired`
    // and the sequence discontinuity (seq 11 -> 14, 2 dropped frames) is preserved.
    let manifest = registry
        .extract_retained_clip_manifest(
            &north_id,
            ClipExtractionRequest {
                clip_id: OpaqueId::new("clip:north:001")?,
                clip_version: VersionId::new("clipver:north:001:v1")?,
                source_session_id: OpaqueId::new("sess:north:2026-11-03")?,
                event_start_ms: 12_100,
                event_end_ms: 12_600,
                requested_pre_context_ms: 2_500,
                requested_post_context_ms: 500,
                extracted_at_ms: 13_000,
                trigger_explanation:
                    "Fast horizon streak across western sky sector (simulated fixture)".to_owned(),
            },
        )?
        .clone();

    // 8. Hand off the directory's `CameraSourceProfile`, `primary_frame`, and `context_window`
    // directly into `CameraAnomalyMonitor` (`AGENTS.md` §4).
    let north_entry = registry
        .camera(&north_id)
        .ok_or("missing registered north camera")?;
    let anomaly_limits = AnomalyLimits::new(4, 16, 16, 8, 8, 8, 8, 16_384)?;
    let detector = DetectorRunMetadata {
        detector_id: OpaqueId::new("det:sky-diff")?,
        detector_version: VersionId::new("detver:sky-diff:v1")?,
        method_description: "Simulated temporal differencing".to_owned(),
        config_digest_sha256: hex64('d'),
    };
    let region = DetectionRegionConfig {
        region_id: OpaqueId::new("reg:west-sky")?,
        label: "Western sky sector".to_owned(),
        baseline: SceneBaselineKind::NighttimeLowLight,
        score_threshold_bps: 7_000,
        min_persistence_frames: 2,
        min_persistence_ms: 200,
        deduplication_window_ms: 2_000,
        requires_privacy_mask: false,
        privacy_mask_applied: false,
    };
    let mut monitor = CameraAnomalyMonitor::new(
        north_entry.to_anomaly_source_profile(),
        detector,
        vec![region.clone()],
        anomaly_limits,
    )?;
    monitor.record_step(ObservationStepInput::SceneTrigger(Box::new(
        SceneTriggerInput {
            observation_id: OpaqueId::new("obs:north:clip-001")?,
            candidate_id: OpaqueId::new("cand:north:clip-001")?,
            candidate_version: VersionId::new("candver:north:clip-001:v1")?,
            region_id: region.region_id,
            category: AnomalyCategory::SkyTransient,
            baseline_at_trigger: SceneBaselineKind::NighttimeLowLight,
            event_start_ms: manifest.event_start_ms(),
            event_end_ms: manifest.event_end_ms(),
            detection_emitted_ms: manifest.extracted_at_ms(),
            observed_persistence_frames: 2,
            raw_score_bps: 8_200,
            clock_uncertainty_ms: 30,
            trigger_explanation: manifest.trigger_explanation().to_owned(),
            primary_frame: manifest.primary_frame().clone(),
            context_window: manifest.context_window().clone(),
        },
    )))?;

    println!(
        "camera_directory_demo: cameras={} north_cone_vertices={} north_buffered_frames={} expired_by_window={} dropped_frames_in_clip={} anomaly_candidates={}",
        registry.cameras().len(),
        north_cone.arc_vertices_lat_lon_deg().len(),
        north_entry.buffered_frames().len(),
        north_entry.eviction_summary().expired_by_window_frames(),
        manifest.dropped_frame_count_in_span(),
        monitor.candidates().len(),
    );
    Ok(())
}
