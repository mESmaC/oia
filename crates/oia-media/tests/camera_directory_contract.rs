//! Contract tests for camera directory registration, viewing-cone geometry,
//! feed/still/fallback/synthetic/archive classification, freshness tracking,
//! restricted cutoff, and bounded retention ring-buffer / clip-manifest handoff
//! (`#28`, `CAM-01`, `CAM-02`, `CAM-04`, `docs/architecture.md` §§3.2, 4.3, 4.4, 8, `A09`).

use oia_domain::{ActorId, OpaqueId, SchemaVersion, VersionId};
use oia_media::camera_anomaly::{
    AnomalyCategory, AnomalyError, AnomalyLimits, CameraAccessStatus, CameraAnomalyMonitor,
    CameraCalibrationState, CameraHealthState, ContextWindowStatus, DetectionRegionConfig,
    DetectorRunMetadata, FeedObservationKind, FrameArtifactOrigin, ObservationStepInput,
    RESTRICTED_SOURCE_POLICY_NOTICE, SceneBaselineKind, SceneTriggerInput, SourceUsePermission,
};
use oia_media::camera_directory::{
    ArchiveCapability, CameraDirectoryError, CameraDirectoryLimitKind, CameraDirectoryLimits,
    CameraDirectoryRegistrationInput, CameraDirectoryRegistry, CameraFrameSubmission,
    CameraHorizontalPosition, CameraSourceTerms, CameraViewKind, CameraViewingOrientation,
    ClipExtractionRequest, DIRECTORY_LISTING_PROVENANCE_NOTICE, FrameFreshnessEvaluation,
    IntentionalPublicationStatus, MAX_CLOCK_UNCERTAINTY_MS, MAX_VIEWING_CONE_ARC_VERTICES,
    MAX_VIEWING_CONE_RANGE_M, OrdinaryAccessMethod, PrivacyMaskZone, PrivacyViewAssessment,
    SourceLifecycleHealth, StillRefreshCapability, StreamCapability, ViewingConeClass,
};

fn hex64(digit: char) -> String {
    digit.to_string().repeat(64)
}

fn default_limits() -> Result<CameraDirectoryLimits, CameraDirectoryError> {
    CameraDirectoryLimits::new(8, 8, 16, 4, 8_192, 8, 8, 8, 16_384)
}

fn sample_registration(
    camera_id: &str,
    source_version: &str,
) -> Result<CameraDirectoryRegistrationInput, CameraDirectoryError> {
    let actor = ActorId::new("actor:test-operator")?;
    Ok(CameraDirectoryRegistrationInput {
        camera_id: OpaqueId::new(camera_id)?,
        upstream_feed_id: OpaqueId::new(format!("feed:{camera_id}"))?,
        source_version: VersionId::new(source_version)?,
        schema_version: SchemaVersion::new(1)?,
        registered_by: actor.clone(),
        registered_at_ms: 1_000,
        provider_id: OpaqueId::new("prov:public-directory")?,
        provider_attribution: "Public Observatory Directory".to_owned(),
        listing_reference: format!("https://example.invalid/cameras/{camera_id}"),
        access_method: OrdinaryAccessMethod::ExistingDirectoryListing,
        intentional_publication: IntentionalPublicationStatus::NotEstablishedOrdinaryAccess,
        view_kind: CameraViewKind::SkyOrObservatory,
        privacy_assessment: PrivacyViewAssessment::ClearPublicView {
            assessed_by: actor,
            assessed_at_ms: 950,
        },
        stream_capability: StreamCapability::LiveStreamSupported {
            nominal_frame_interval_ms: 250,
        },
        still_capability: StillRefreshCapability::PeriodicStillSupported {
            nominal_refresh_interval_ms: 5_000,
            stale_after_ms: 10_000,
        },
        archive_capability: ArchiveCapability::VerifiedArchiveWindow {
            archive_version: VersionId::new(format!("arch:{source_version}"))?,
            earliest_capture_ms: 500,
            latest_capture_ms: 900,
        },
        terms: CameraSourceTerms {
            automated_processing: SourceUsePermission::Permitted,
            clip_retention: SourceUsePermission::Permitted,
            redistribution: SourceUsePermission::Prohibited,
            max_retention_window_ms: Some(2_000),
            policy_reference: "Directory Terms v1".to_owned(),
            checked_at_ms: 900,
        },
        position: CameraHorizontalPosition::KnownWgs84 {
            latitude_deg: 33.4152,
            longitude_deg: -111.8315,
            horizontal_uncertainty_m: 3.0,
        },
        orientation: CameraViewingOrientation::Calibrated {
            calibration_version: VersionId::new(format!("cal:{source_version}"))?,
            azimuth_deg: 0.0,
            pitch_deg: Some(15.0),
            roll_deg: Some(0.0),
            horizontal_fov_deg: 60.0,
            vertical_fov_deg: Some(40.0),
            max_angular_error_mdeg: 100,
        },
        initial_health: SourceLifecycleHealth::Available,
    })
}

#[test]
fn cam01_cam02_directory_registration_intentional_publication_and_privacy_exclusions()
-> Result<(), Box<dyn std::error::Error>> {
    let mut registry = CameraDirectoryRegistry::new(default_limits()?);

    // 1. `NotEstablishedOrdinaryAccess` is metadata only and never blocks or degrades a camera (`CAM-01`).
    let reg = sample_registration("cam:ordinary-no-owner-pub", "srcver:1")?;
    let entry = registry.register_camera(reg)?;
    assert_eq!(
        entry.intentional_publication(),
        IntentionalPublicationStatus::NotEstablishedOrdinaryAccess
    );
    assert_eq!(
        entry.access_status(),
        CameraAccessStatus::OrdinarilyAccessible
    );
    assert_eq!(entry.current_health(), SourceLifecycleHealth::Available);
    assert_eq!(
        entry.directory_provenance_notice(),
        DIRECTORY_LISTING_PROVENANCE_NOTICE
    );

    // 2. Catalog-only camera without live stream, still refresh, archive, or processing permission is accepted (`CAM-02`).
    let mut catalog_only = sample_registration("cam:catalog-only", "srcver:2")?;
    catalog_only.stream_capability = StreamCapability::None;
    catalog_only.still_capability = StillRefreshCapability::None;
    catalog_only.archive_capability = ArchiveCapability::NoArchiveEstablished;
    catalog_only.terms.automated_processing = SourceUsePermission::Unknown;
    catalog_only.terms.clip_retention = SourceUsePermission::Prohibited;
    catalog_only.terms.max_retention_window_ms = None;
    catalog_only.position = CameraHorizontalPosition::ApproximateGazetteerCentroid {
        locality_label: "Downtown Mesa centroid".to_owned(),
        latitude_deg: 33.4150,
        longitude_deg: -111.8310,
        uncertainty_radius_m: 2_500.0,
    };
    catalog_only.orientation = CameraViewingOrientation::Unknown {
        reason: "listing omits compass bearing".to_owned(),
    };
    let catalog_entry = registry.register_camera(catalog_only)?;
    assert_eq!(
        catalog_entry.to_anomaly_source_profile().calibration,
        CameraCalibrationState::Uncalibrated
    );
    assert!(matches!(
        catalog_entry.omitted_retention_context_status(),
        ContextWindowStatus::OmittedByRetentionPolicy { .. }
    ));

    // 3. Home interiors (`HomeInterior`, `ViewIntoHomeInterior`) are unconditionally refused (`CAM-01`).
    for forbidden_view in [
        CameraViewKind::HomeInterior,
        CameraViewKind::ViewIntoHomeInterior,
    ] {
        let mut home = sample_registration("cam:home-rejected", "srcver:home")?;
        home.view_kind = forbidden_view;
        let err = registry.register_camera(home);
        assert!(matches!(
            err,
            Err(CameraDirectoryError::HomeInteriorExcluded { view_kind, .. }) if view_kind == forbidden_view
        ));
    }

    // 4. `ResidentialExteriorStreetView` requires declared privacy mask zones, and unmasked zones block frame ingestion (`CAM-02`).
    let mut res_no_masks = sample_registration("cam:res-unmasked", "srcver:3")?;
    res_no_masks.view_kind = CameraViewKind::ResidentialExteriorStreetView;
    assert!(matches!(
        registry.register_camera(res_no_masks.clone()),
        Err(CameraDirectoryError::ResidentialExteriorRequiresMaskZones { .. })
    ));

    let zone_id = OpaqueId::new("zone:window-01")?;
    res_no_masks.privacy_assessment = PrivacyViewAssessment::RequiresMaskZones {
        assessed_by: ActorId::new("actor:test-operator")?,
        assessed_at_ms: 960,
        zones: vec![PrivacyMaskZone {
            zone_id: zone_id.clone(),
            label: "Second-floor residential window".to_owned(),
            mask_applied: false,
        }],
    };
    let res_id = res_no_masks.camera_id.clone();
    let res_entry = registry.register_camera(res_no_masks)?;
    assert!(!res_entry.privacy_ready_for_acquisition());

    let unmasked_ingest = registry.ingest_frame(
        &res_id,
        CameraFrameSubmission {
            frame_version: VersionId::new("ver:res:f1")?,
            sha256_hex: hex64('1'),
            byte_len: 512,
            sequence_number: 1,
            declared_kind: FeedObservationKind::UpstreamCameraFeed,
            origin: FrameArtifactOrigin::OriginalRetainedFrame,
            source_capture_time_ms: Some(1_100),
            clock_uncertainty_ms: 10,
            ingestion_time_ms: 1_120,
            archive_version: None,
        },
    );
    assert!(matches!(
        unmasked_ingest,
        Err(CameraDirectoryError::UnmaskedPrivacyZone { zone_id: ref z, .. }) if z == &zone_id
    ));

    // 5. `ExcludedPrivateActivity` can be cataloged to record the exclusion, but refuses frame acquisition (`CAM-02`).
    let mut excluded = sample_registration("cam:private-yard", "srcver:4")?;
    excluded.privacy_assessment = PrivacyViewAssessment::ExcludedPrivateActivity {
        assessed_by: ActorId::new("actor:test-operator")?,
        assessed_at_ms: 970,
        reason: "Camera pans across enclosed private courtyard".to_owned(),
    };
    let excluded_id = excluded.camera_id.clone();
    registry.register_camera(excluded)?;
    let excluded_ingest = registry.ingest_frame(
        &excluded_id,
        CameraFrameSubmission {
            frame_version: VersionId::new("ver:excluded:f1")?,
            sha256_hex: hex64('2'),
            byte_len: 512,
            sequence_number: 1,
            declared_kind: FeedObservationKind::UpstreamCameraFeed,
            origin: FrameArtifactOrigin::OriginalRetainedFrame,
            source_capture_time_ms: Some(1_100),
            clock_uncertainty_ms: 10,
            ingestion_time_ms: 1_120,
            archive_version: None,
        },
    );
    assert!(matches!(
        excluded_ingest,
        Err(CameraDirectoryError::PrivateActivityViewExcluded { .. })
    ));

    Ok(())
}

#[test]
fn cam02_cam05_viewing_cone_geometry_and_movement_invalidation()
-> Result<(), Box<dyn std::error::Error>> {
    let mut registry = CameraDirectoryRegistry::new(default_limits()?);
    let cal_reg = sample_registration("cam:cone-cal", "srcver:cone:1")?;
    let cal_id = cal_reg.camera_id.clone();
    registry.register_camera(cal_reg)?;

    // Calibrated due-north camera (`azimuth_deg = 0.0`, `horizontal_fov_deg = 60.0`):
    // midpoint vertex (`idx = 1` of 3) lies due north of the apex with higher latitude and matching longitude.
    let cone = registry.compute_viewing_cone(&cal_id, 1_000.0, 3)?;
    assert!(cone.is_calibrated());
    assert_eq!(cone.arc_vertices_lat_lon_deg().len(), 3);
    let (north_lat, north_lon) = cone.arc_vertices_lat_lon_deg()[1];
    assert!(north_lat > cone.apex_latitude_deg());
    assert!((north_lon - cone.apex_longitude_deg()).abs() < 1e-9);

    // ReportedApproximate orientation produces an `IllustrativeReportedBearing` cone (`!is_calibrated()`).
    let mut approx_reg = sample_registration("cam:cone-approx", "srcver:cone:2")?;
    approx_reg.orientation = CameraViewingOrientation::ReportedApproximate {
        azimuth_deg: 90.0,
        horizontal_fov_deg: 40.0,
        bearing_uncertainty_deg: 12.0,
    };
    let approx_id = approx_reg.camera_id.clone();
    registry.register_camera(approx_reg)?;
    let approx_cone = registry.compute_viewing_cone(&approx_id, 1_000.0, 3)?;
    assert!(!approx_cone.is_calibrated());
    assert!(matches!(
        approx_cone.cone_class(),
        ViewingConeClass::IllustrativeReportedBearing {
            bearing_uncertainty_deg
        } if (*bearing_uncertainty_deg - 12.0).abs() < f64::EPSILON
    ));

    // Recording `CameraRepositioned` invalidates the calibrated orientation and causes viewing-cone
    // projection to fail until re-calibrated (`CAM-05`, `A09`).
    registry.record_health_transition(
        &cal_id,
        OpaqueId::new("trans:cone:repositioned")?,
        SourceLifecycleHealth::Degraded,
        Some(CameraHealthState::CameraRepositioned),
        2_000,
        "PTZ mount slewed 35 deg east during wind gust".to_owned(),
    )?;
    let cal_entry = registry.camera(&cal_id).ok_or("missing camera")?;
    assert!(matches!(
        cal_entry.to_anomaly_source_profile().calibration,
        CameraCalibrationState::InvalidatedByMovement {
            detected_at_ms: 2_000,
            ..
        }
    ));
    assert!(matches!(
        registry.compute_viewing_cone(&cal_id, 1_000.0, 3),
        Err(CameraDirectoryError::OrientationUnavailableForViewingCone { .. })
    ));

    // Invalid viewing-cone parameters are rejected before allocation.
    assert!(matches!(
        registry.compute_viewing_cone(&approx_id, f64::NAN, 3),
        Err(CameraDirectoryError::InvalidViewingConeRequest { .. })
    ));
    assert!(matches!(
        registry.compute_viewing_cone(&approx_id, MAX_VIEWING_CONE_RANGE_M + 1.0, 3),
        Err(CameraDirectoryError::InvalidViewingConeRequest { .. })
    ));
    assert!(matches!(
        registry.compute_viewing_cone(&approx_id, 500.0, 1),
        Err(CameraDirectoryError::InvalidViewingConeRequest { .. })
    ));
    assert!(matches!(
        registry.compute_viewing_cone(&approx_id, 500.0, MAX_VIEWING_CONE_ARC_VERTICES + 1),
        Err(CameraDirectoryError::InvalidViewingConeRequest { .. })
    ));

    Ok(())
}

#[test]
fn cam02_frame_classification_still_freshness_substitutes_and_archive_bounds()
-> Result<(), Box<dyn std::error::Error>> {
    let mut registry = CameraDirectoryRegistry::new(default_limits()?);
    let reg = sample_registration("cam:classify-01", "srcver:classify:1")?;
    let cam_id = reg.camera_id.clone();
    registry.register_camera(reg)?;

    // 1. Fresh periodic still (`stale_after_ms = 10_000`) is eligible as event evidence.
    let still_1 = registry.ingest_frame(
        &cam_id,
        CameraFrameSubmission {
            frame_version: VersionId::new("ver:still:1")?,
            sha256_hex: hex64('a'),
            byte_len: 1_024,
            sequence_number: 1,
            declared_kind: FeedObservationKind::PeriodicUpstreamStill,
            origin: FrameArtifactOrigin::OriginalRetainedFrame,
            source_capture_time_ms: Some(1_000),
            clock_uncertainty_ms: 50,
            ingestion_time_ms: 2_000,
            archive_version: None,
        },
    )?;
    assert_eq!(
        still_1.effective_kind(),
        FeedObservationKind::PeriodicUpstreamStill
    );
    assert!(still_1.eligible_as_event_evidence());
    assert!(still_1.retained_in_ring_buffer());
    assert!(matches!(
        still_1.freshness(),
        FrameFreshnessEvaluation::FreshPeriodicStill {
            stale_after_ms: 10_000,
            source_capture_time_ms: Some(1_000),
            ingestion_time_ms: 2_000,
            observed_capture_age_ms: Some(1_000),
            unchanged_hold_duration_ms: 0,
        }
    ));

    // 2. Unchanged digest held past `stale_after_ms` (at `ingestion_time_ms = 12_500`, hold = `10_500 > 10_000`)
    // is downgraded to `StaleStillHold`, cannot become evidence, is omitted from the ring buffer,
    // and marks camera health `Stale`.
    let still_2 = registry.ingest_frame(
        &cam_id,
        CameraFrameSubmission {
            frame_version: VersionId::new("ver:still:2")?,
            sha256_hex: hex64('a'),
            byte_len: 1_024,
            sequence_number: 2,
            declared_kind: FeedObservationKind::PeriodicUpstreamStill,
            origin: FrameArtifactOrigin::OriginalRetainedFrame,
            source_capture_time_ms: None,
            clock_uncertainty_ms: 50,
            ingestion_time_ms: 12_500,
            archive_version: None,
        },
    )?;
    assert_eq!(
        still_2.effective_kind(),
        FeedObservationKind::StaleStillHold
    );
    assert!(!still_2.eligible_as_event_evidence());
    assert!(!still_2.retained_in_ring_buffer());
    assert!(matches!(
        still_2.to_retained_frame_ref(),
        Err(
            CameraDirectoryError::NonEvidentiaryFrameCannotBecomeEvidence {
                effective_kind: FeedObservationKind::StaleStillHold,
                ..
            }
        )
    ));
    assert_eq!(
        registry.camera(&cam_id).ok_or("missing")?.current_health(),
        SourceLifecycleHealth::Stale
    );

    // 3. Non-evidentiary substitutes (`StillFallback`, `StreetViewFallback`, `SyntheticPlaceholder`)
    // are never buffered and never eligible as event evidence (`CAM-02`, `AGENTS.md` §2).
    for (idx, sub_kind) in [
        FeedObservationKind::StillFallback,
        FeedObservationKind::StreetViewFallback,
        FeedObservationKind::SyntheticPlaceholder,
    ]
    .into_iter()
    .enumerate()
    {
        let seq = 3 + idx as u64;
        let frame = registry.ingest_frame(
            &cam_id,
            CameraFrameSubmission {
                frame_version: VersionId::new(format!("ver:sub:{seq}"))?,
                sha256_hex: hex64('b'),
                byte_len: 512,
                sequence_number: seq,
                declared_kind: sub_kind,
                origin: FrameArtifactOrigin::OriginalRetainedFrame,
                source_capture_time_ms: None,
                clock_uncertainty_ms: 50,
                ingestion_time_ms: 12_600 + (idx as i64) * 10,
                archive_version: None,
            },
        )?;
        assert!(!frame.eligible_as_event_evidence());
        assert!(!frame.retained_in_ring_buffer());
    }

    // 4. Historical archive frame inside the verified window `[500, 900]` with matching `archive_version`
    // is eligible as retrospective evidence (`to_retained_frame_ref()` succeeds) without entering the live ring buffer,
    // and `CameraAnomalyMonitor::new` rejects `HistoricalArchiveFrame` as a live monitoring feed kind.
    let archive_ok = registry.ingest_frame(
        &cam_id,
        CameraFrameSubmission {
            frame_version: VersionId::new("ver:arch:700")?,
            sha256_hex: hex64('c'),
            byte_len: 1_024,
            sequence_number: 10,
            declared_kind: FeedObservationKind::HistoricalArchiveFrame,
            origin: FrameArtifactOrigin::OriginalRetainedFrame,
            source_capture_time_ms: Some(700),
            clock_uncertainty_ms: 50,
            ingestion_time_ms: 13_000,
            archive_version: Some(VersionId::new("arch:srcver:classify:1")?),
        },
    )?;
    assert!(archive_ok.eligible_as_event_evidence());
    assert!(!archive_ok.retained_in_ring_buffer());
    assert_eq!(archive_ok.to_retained_frame_ref()?.capture_time_ms, 700);

    // Archive frame outside `[500, 900]` is rejected.
    let archive_oob = registry.ingest_frame(
        &cam_id,
        CameraFrameSubmission {
            frame_version: VersionId::new("ver:arch:950")?,
            sha256_hex: hex64('d'),
            byte_len: 1_024,
            sequence_number: 11,
            declared_kind: FeedObservationKind::HistoricalArchiveFrame,
            origin: FrameArtifactOrigin::OriginalRetainedFrame,
            source_capture_time_ms: Some(950),
            clock_uncertainty_ms: 50,
            ingestion_time_ms: 13_100,
            archive_version: Some(VersionId::new("arch:srcver:classify:1")?),
        },
    );
    assert!(matches!(
        archive_oob,
        Err(CameraDirectoryError::ArchiveFrameOutsideVerifiedWindow { .. })
    ));

    // Generated/predictive frames and clock uncertainty > 24h are rejected (`AGENTS.md` §2).
    let gen_err = registry.ingest_frame(
        &cam_id,
        CameraFrameSubmission {
            frame_version: VersionId::new("ver:gen:1")?,
            sha256_hex: hex64('e'),
            byte_len: 1_024,
            sequence_number: 12,
            declared_kind: FeedObservationKind::UpstreamCameraFeed,
            origin: FrameArtifactOrigin::PredictiveOrGeneratedDerivative,
            source_capture_time_ms: Some(13_200),
            clock_uncertainty_ms: 10,
            ingestion_time_ms: 13_210,
            archive_version: None,
        },
    );
    assert!(matches!(
        gen_err,
        Err(CameraDirectoryError::GeneratedFrameForbiddenAsEvidence { .. })
    ));

    let clock_err = registry.ingest_frame(
        &cam_id,
        CameraFrameSubmission {
            frame_version: VersionId::new("ver:clock:1")?,
            sha256_hex: hex64('e'),
            byte_len: 1_024,
            sequence_number: 12,
            declared_kind: FeedObservationKind::UpstreamCameraFeed,
            origin: FrameArtifactOrigin::OriginalRetainedFrame,
            source_capture_time_ms: Some(13_200),
            clock_uncertainty_ms: MAX_CLOCK_UNCERTAINTY_MS + 1,
            ingestion_time_ms: 13_210,
            archive_version: None,
        },
    );
    assert!(matches!(
        clock_err,
        Err(CameraDirectoryError::ClockUncertaintyExceedsMax { .. })
    ));

    Ok(())
}

#[test]
fn cam01_a09_restricted_cutoff_blocks_acquisition_and_retries_until_explicit_reregistration()
-> Result<(), Box<dyn std::error::Error>> {
    let mut registry = CameraDirectoryRegistry::new(default_limits()?);
    let reg = sample_registration("cam:restricted-flow", "srcver:rest:1")?;
    let cam_id = reg.camera_id.clone();
    registry.register_camera(reg)?;

    // Transition to `UnavailableRestricted` / `SourceRestricted`.
    registry.record_health_transition(
        &cam_id,
        OpaqueId::new("trans:rest:1")?,
        SourceLifecycleHealth::UnavailableRestricted,
        Some(CameraHealthState::SourceRestricted),
        5_000,
        "HTTP 401 authentication required on upstream feed".to_owned(),
    )?;

    // Frame ingestion is immediately refused with `RESTRICTED_SOURCE_POLICY_NOTICE`.
    let ingest_err = registry.ingest_frame(
        &cam_id,
        CameraFrameSubmission {
            frame_version: VersionId::new("ver:rest:f1")?,
            sha256_hex: hex64('1'),
            byte_len: 512,
            sequence_number: 1,
            declared_kind: FeedObservationKind::UpstreamCameraFeed,
            origin: FrameArtifactOrigin::OriginalRetainedFrame,
            source_capture_time_ms: Some(5_100),
            clock_uncertainty_ms: 20,
            ingestion_time_ms: 5_110,
            archive_version: None,
        },
    );
    assert_eq!(
        ingest_err,
        Err(CameraDirectoryError::SourceRestricted {
            camera_id: cam_id.clone(),
            policy_notice: RESTRICTED_SOURCE_POLICY_NOTICE,
        })
    );

    // Retry health transitions (`Connecting`, `Available`, etc.) are refused; only explicit
    // re-registration with a new `source_version` can re-admit the camera.
    let retry_err = registry.record_health_transition(
        &cam_id,
        OpaqueId::new("trans:rest:retry")?,
        SourceLifecycleHealth::Available,
        None,
        5_200,
        "Automatic reconnect attempt".to_owned(),
    );
    assert_eq!(
        retry_err,
        Err(
            CameraDirectoryError::RestrictedSourceRequiresExplicitReregistration {
                camera_id: cam_id.clone()
            }
        )
    );

    // Reusing the old `source_version` is refused.
    let reuse_err = registry.reregister_ordinarily_accessible_version(
        &cam_id,
        OpaqueId::new("trans:rest:rereg-bad")?,
        VersionId::new("srcver:rest:1")?,
        "https://example.invalid/cameras/new-public-endpoint".to_owned(),
        6_000,
        "Re-registering with reused version".to_owned(),
    );
    assert!(matches!(
        reuse_err,
        Err(CameraDirectoryError::ReusedVersionId(_))
    ));

    // Explicit re-registration with a new `source_version` succeeds and preserves `prior_source_versions`.
    let updated = registry.reregister_ordinarily_accessible_version(
        &cam_id,
        OpaqueId::new("trans:rest:rereg-ok")?,
        VersionId::new("srcver:rest:2")?,
        "https://example.invalid/cameras/new-public-endpoint".to_owned(),
        6_000,
        "Operator configured new public directory feed URL".to_owned(),
    )?;
    assert_eq!(
        updated.access_status(),
        CameraAccessStatus::OrdinarilyAccessible
    );
    assert_eq!(updated.source_version(), &VersionId::new("srcver:rest:2")?);
    assert_eq!(
        updated.prior_source_versions(),
        &[VersionId::new("srcver:rest:1")?]
    );

    Ok(())
}

#[test]
fn cam04_a09_fifo_eviction_order_clip_manifest_and_anomaly_monitor_bridge()
-> Result<(), Box<dyn std::error::Error>> {
    // Buffer limit: max 3 buffered frames, max 3_072 bytes, `max_retention_window_ms = 2_000`.
    let limits = CameraDirectoryLimits::new(4, 4, 8, 3, 3_072, 8, 4, 8, 16_384)?;
    let mut registry = CameraDirectoryRegistry::new(limits);
    let reg = sample_registration("cam:ring-fifo", "srcver:ring:1")?;
    let cam_id = reg.camera_id.clone();
    registry.register_camera(reg)?;

    // Ingest 3 frames at the exact same `ingestion_time_ms = 10_000` with increasing `sequence_number` (1, 2, 3),
    // then a 4th frame at `ingestion_time_ms = 10_000, sequence_number = 4` to verify deterministic FIFO
    // eviction of `(10_000, seq 1, ver:f1)` by capacity, followed by a 5th frame at `12_500` (seq 7, dropping 5 & 6)
    // that expires all `10_000` frames by retention window (`age = 2_500 > 2_000`).
    for (ver, digit, seq) in [
        ("ver:f1", '1', 1_u64),
        ("ver:f2", '2', 2_u64),
        ("ver:f3", '3', 3_u64),
        ("ver:f4", '4', 4_u64),
    ] {
        registry.ingest_frame(
            &cam_id,
            CameraFrameSubmission {
                frame_version: VersionId::new(ver)?,
                sha256_hex: hex64(digit),
                byte_len: 1_024,
                sequence_number: seq,
                declared_kind: FeedObservationKind::UpstreamCameraFeed,
                origin: FrameArtifactOrigin::OriginalRetainedFrame,
                source_capture_time_ms: Some(10_000),
                clock_uncertainty_ms: 20,
                ingestion_time_ms: 10_000,
                archive_version: None,
            },
        )?;
    }

    let entry_after_cap = registry.camera(&cam_id).ok_or("missing")?;
    assert_eq!(entry_after_cap.buffered_frames().len(), 3);
    assert_eq!(
        entry_after_cap.buffered_frames()[0].frame_version(),
        &VersionId::new("ver:f2")?
    );
    assert_eq!(
        entry_after_cap
            .eviction_summary()
            .evicted_by_capacity_frames(),
        1
    );

    // Frame 5 at `12_500` ms (seq 7 -> discontinuity of 2 dropped frames) expires f2, f3, f4 by window,
    // and Frame 6 at `12_800` ms (seq 8) provides post-context.
    registry.ingest_frame(
        &cam_id,
        CameraFrameSubmission {
            frame_version: VersionId::new("ver:f5")?,
            sha256_hex: hex64('5'),
            byte_len: 1_024,
            sequence_number: 7,
            declared_kind: FeedObservationKind::UpstreamCameraFeed,
            origin: FrameArtifactOrigin::OriginalRetainedFrame,
            source_capture_time_ms: Some(12_500),
            clock_uncertainty_ms: 20,
            ingestion_time_ms: 12_520,
            archive_version: None,
        },
    )?;
    registry.ingest_frame(
        &cam_id,
        CameraFrameSubmission {
            frame_version: VersionId::new("ver:f6")?,
            sha256_hex: hex64('6'),
            byte_len: 1_024,
            sequence_number: 8,
            declared_kind: FeedObservationKind::UpstreamCameraFeed,
            origin: FrameArtifactOrigin::OriginalRetainedFrame,
            source_capture_time_ms: Some(12_800),
            clock_uncertainty_ms: 20,
            ingestion_time_ms: 12_820,
            archive_version: None,
        },
    )?;

    let manifest = registry
        .extract_retained_clip_manifest(
            &cam_id,
            ClipExtractionRequest {
                clip_id: OpaqueId::new("clip:ring:001")?,
                clip_version: VersionId::new("clipver:ring:001")?,
                source_session_id: OpaqueId::new("sess:ring:01")?,
                event_start_ms: 12_400,
                event_end_ms: 12_600,
                requested_pre_context_ms: 3_000,
                requested_post_context_ms: 500,
                extracted_at_ms: 13_000,
                trigger_explanation: "Simulated transient in ring buffer".to_owned(),
            },
        )?
        .clone();

    assert_eq!(manifest.dropped_frame_count_in_span(), 2);
    assert!(matches!(
        manifest.context_window(),
        ContextWindowStatus::PartialBufferExpired {
            expired_frame_count: 4,
            pre_frames,
            post_frames,
        } if pre_frames.is_empty() && post_frames.len() == 1
    ));

    // Bridge directly into `CameraAnomalyMonitor` (`AGENTS.md` §4).
    let entry = registry.camera(&cam_id).ok_or("missing")?;
    let mut monitor = CameraAnomalyMonitor::new(
        entry.to_anomaly_source_profile(),
        DetectorRunMetadata {
            detector_id: OpaqueId::new("det:bridge")?,
            detector_version: VersionId::new("detver:bridge:v1")?,
            method_description: "Bridge test detector".to_owned(),
            config_digest_sha256: hex64('9'),
        },
        vec![DetectionRegionConfig {
            region_id: OpaqueId::new("reg:sky")?,
            label: "Sky region".to_owned(),
            baseline: SceneBaselineKind::DaylightClear,
            score_threshold_bps: 6_000,
            min_persistence_frames: 1,
            min_persistence_ms: 100,
            deduplication_window_ms: 1_000,
            requires_privacy_mask: false,
            privacy_mask_applied: false,
        }],
        AnomalyLimits::new(4, 16, 16, 8, 8, 8, 8, 16_384)?,
    )?;

    monitor.record_step(ObservationStepInput::SceneTrigger(Box::new(
        SceneTriggerInput {
            observation_id: OpaqueId::new("obs:bridge:1")?,
            candidate_id: OpaqueId::new("cand:bridge:1")?,
            candidate_version: VersionId::new("candver:bridge:1")?,
            region_id: OpaqueId::new("reg:sky")?,
            category: AnomalyCategory::SkyTransient,
            baseline_at_trigger: SceneBaselineKind::DaylightClear,
            event_start_ms: manifest.event_start_ms(),
            event_end_ms: manifest.event_end_ms(),
            detection_emitted_ms: manifest.extracted_at_ms(),
            observed_persistence_frames: 2,
            raw_score_bps: 7_500,
            clock_uncertainty_ms: 20,
            trigger_explanation: manifest.trigger_explanation().to_owned(),
            primary_frame: manifest.primary_frame().clone(),
            context_window: manifest.context_window().clone(),
        },
    )))?;
    assert_eq!(monitor.candidates().len(), 1);

    // Also verify `CameraAnomalyMonitor::new` accepts `PeriodicUpstreamStill` and rejects `HistoricalArchiveFrame` and `StaleStillHold`.
    let mut still_profile = entry.to_anomaly_source_profile();
    still_profile.feed_kind = FeedObservationKind::PeriodicUpstreamStill;
    assert!(
        CameraAnomalyMonitor::new(
            still_profile.clone(),
            DetectorRunMetadata {
                detector_id: OpaqueId::new("det:bridge")?,
                detector_version: VersionId::new("detver:bridge:v1")?,
                method_description: "Bridge test detector".to_owned(),
                config_digest_sha256: hex64('9'),
            },
            vec![DetectionRegionConfig {
                region_id: OpaqueId::new("reg:sky")?,
                label: "Sky region".to_owned(),
                baseline: SceneBaselineKind::DaylightClear,
                score_threshold_bps: 6_000,
                min_persistence_frames: 1,
                min_persistence_ms: 100,
                deduplication_window_ms: 1_000,
                requires_privacy_mask: false,
                privacy_mask_applied: false,
            }],
            AnomalyLimits::new(4, 16, 16, 8, 8, 8, 8, 16_384)?,
        )
        .is_ok()
    );

    for non_live_kind in [
        FeedObservationKind::StaleStillHold,
        FeedObservationKind::HistoricalArchiveFrame,
    ] {
        let mut rejected_profile = still_profile.clone();
        rejected_profile.feed_kind = non_live_kind;
        let res = CameraAnomalyMonitor::new(
            rejected_profile,
            DetectorRunMetadata {
                detector_id: OpaqueId::new("det:bridge")?,
                detector_version: VersionId::new("detver:bridge:v1")?,
                method_description: "Bridge test detector".to_owned(),
                config_digest_sha256: hex64('9'),
            },
            vec![],
            AnomalyLimits::new(4, 16, 16, 8, 8, 8, 8, 16_384)?,
        );
        assert!(matches!(
            res,
            Err(AnomalyError::NonObservationFeedSubstitute { feed_kind, .. }) if feed_kind == non_live_kind
        ));
    }

    // Verify zero limit rejection.
    assert_eq!(
        CameraDirectoryLimits::new(0, 1, 1, 1, 1, 1, 1, 1, 1),
        Err(CameraDirectoryError::ZeroLimit(
            CameraDirectoryLimitKind::RegisteredCameras
        ))
    );

    Ok(())
}
