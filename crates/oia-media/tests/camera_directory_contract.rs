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
fn cam01_cam02_directory_registration_permissions_and_privacy_exclusions()
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

    // 2. Catalog-only camera without live stream or still refresh is accepted (`CAM-02`),
    // and `to_anomaly_source_profile()` refuses it as `CatalogOnlyCameraHasNoLiveFeed` (`N7`)
    // rather than mislabeling it as `StillFallback`.
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
    let catalog_id = catalog_only.camera_id.clone();
    let catalog_entry = registry.register_camera(catalog_only)?;
    assert_eq!(
        catalog_entry.orientation().to_calibration_state(),
        CameraCalibrationState::Uncalibrated
    );
    assert_eq!(
        catalog_entry.to_anomaly_source_profile(),
        Err(CameraDirectoryError::CatalogOnlyCameraHasNoLiveFeed {
            camera_id: catalog_id.clone(),
        })
    );
    assert!(matches!(
        catalog_entry.omitted_retention_context_status(),
        ContextWindowStatus::OmittedByRetentionPolicy { .. }
    ));

    // `automated_processing != Permitted` refuses `ingest_frame` (`B6`), and `clip_retention != Permitted`
    // refuses `extract_retained_clip_manifest`.
    let auto_err = registry.ingest_frame(
        &catalog_id,
        CameraFrameSubmission {
            frame_version: VersionId::new("ver:cat:f1")?,
            sha256_hex: hex64('1'),
            byte_len: 512,
            sequence_number: 1,
            declared_kind: FeedObservationKind::StillFallback,
            origin: FrameArtifactOrigin::OriginalRetainedFrame,
            source_capture_time_ms: None,
            clock_uncertainty_ms: 10,
            ingestion_time_ms: 1_100,
            archive_version: None,
        },
    );
    assert_eq!(
        auto_err,
        Err(CameraDirectoryError::AutomatedProcessingNotPermitted {
            camera_id: catalog_id.clone(),
            status: SourceUsePermission::Unknown,
        })
    );

    let clip_perm_err = registry.extract_retained_clip_manifest(
        &catalog_id,
        ClipExtractionRequest {
            clip_id: OpaqueId::new("clip:cat:1")?,
            clip_version: VersionId::new("clipver:cat:1")?,
            source_session_id: OpaqueId::new("sess:cat:1")?,
            event_start_ms: 1_000,
            event_end_ms: 1_500,
            requested_pre_context_ms: 0,
            requested_post_context_ms: 0,
            extracted_at_ms: 1_600,
            trigger_explanation: "Disallowed clip retention".to_owned(),
        },
    );
    assert_eq!(
        clip_perm_err,
        Err(CameraDirectoryError::ClipRetentionNotPermitted {
            camera_id: catalog_id,
            status: SourceUsePermission::Prohibited,
        })
    );

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

    // 4. `ResidentialExteriorStreetView` requires declared privacy mask zones; unmasked zones block
    // both `ingest_frame` AND `to_anomaly_source_profile` (`B3`, `CAM-02`).
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
    assert_eq!(
        res_entry.to_anomaly_source_profile(),
        Err(CameraDirectoryError::UnmaskedPrivacyZone {
            camera_id: res_id.clone(),
            zone_id: zone_id.clone(),
        })
    );

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

    // 5. `ExcludedPrivateActivity` and `DisabledByPolicy` block `ingest_frame`,
    // `to_anomaly_source_profile()`, and `extract_retained_clip_manifest` (`B3`, `B6`, `CAM-02`).
    let mut excluded = sample_registration("cam:private-yard", "srcver:4")?;
    excluded.privacy_assessment = PrivacyViewAssessment::ExcludedPrivateActivity {
        assessed_by: ActorId::new("actor:test-operator")?,
        assessed_at_ms: 970,
        reason: "Camera pans across enclosed private courtyard".to_owned(),
    };
    let excluded_id = excluded.camera_id.clone();
    let excluded_entry = registry.register_camera(excluded)?;
    assert!(matches!(
        excluded_entry.to_anomaly_source_profile(),
        Err(CameraDirectoryError::PrivateActivityViewExcluded { .. })
    ));
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

    let mut disabled = sample_registration("cam:disabled-policy", "srcver:5")?;
    disabled.initial_health = SourceLifecycleHealth::DisabledByPolicy;
    let disabled_id = disabled.camera_id.clone();
    let disabled_entry = registry.register_camera(disabled)?;
    assert_eq!(
        disabled_entry.to_anomaly_source_profile(),
        Err(CameraDirectoryError::CameraDisabledByPolicy {
            camera_id: disabled_id.clone(),
        })
    );
    assert_eq!(
        registry.ingest_frame(
            &disabled_id,
            CameraFrameSubmission {
                frame_version: VersionId::new("ver:disabled:f1")?,
                sha256_hex: hex64('3'),
                byte_len: 512,
                sequence_number: 1,
                declared_kind: FeedObservationKind::UpstreamCameraFeed,
                origin: FrameArtifactOrigin::OriginalRetainedFrame,
                source_capture_time_ms: Some(1_100),
                clock_uncertainty_ms: 10,
                ingestion_time_ms: 1_120,
                archive_version: None,
            },
        ),
        Err(CameraDirectoryError::CameraDisabledByPolicy {
            camera_id: disabled_id.clone(),
        })
    );
    assert_eq!(
        registry.extract_retained_clip_manifest(
            &disabled_id,
            ClipExtractionRequest {
                clip_id: OpaqueId::new("clip:disabled:1")?,
                clip_version: VersionId::new("clipver:disabled:1")?,
                source_session_id: OpaqueId::new("sess:disabled:1")?,
                event_start_ms: 1_000,
                event_end_ms: 1_200,
                requested_pre_context_ms: 0,
                requested_post_context_ms: 0,
                extracted_at_ms: 1_300,
                trigger_explanation: "Disabled camera clip".to_owned(),
            },
        ),
        Err(CameraDirectoryError::CameraDisabledByPolicy {
            camera_id: disabled_id,
        })
    );

    Ok(())
}

#[test]
fn cam02_cam05_viewing_cone_geometry_movement_invalidation_and_recalibration()
-> Result<(), Box<dyn std::error::Error>> {
    let mut registry = CameraDirectoryRegistry::new(default_limits()?);
    let cal_reg = sample_registration("cam:cone-cal", "srcver:cone:1")?;
    let cal_id = cal_reg.camera_id.clone();
    registry.register_camera(cal_reg)?;

    // Calibrated due-north camera (`azimuth_deg = 0.0`, `horizontal_fov_deg = 60.0`):
    // midpoint vertex (`idx = 1` of 3) lies due north of the apex with higher latitude and matching longitude,
    // and `uncertainty_widened_horizontal_fov_deg` + `uncertainty_widened_arc_vertices_lat_lon_deg`
    // reflect both `max_angular_error_mdeg` and `asin(horizontal_uncertainty_m / range_m)`.
    let cone = registry.compute_viewing_cone(&cal_id, 1_000.0, 3)?;
    assert!(cone.is_calibrated());
    assert_eq!(cone.arc_vertices_lat_lon_deg().len(), 3);
    assert_eq!(cone.uncertainty_widened_arc_vertices_lat_lon_deg().len(), 3);
    let expected_widened_fov = 60.0 + 2.0 * (0.1 + (3.0_f64 / 1_000.0).asin().to_degrees());
    assert!((cone.uncertainty_widened_horizontal_fov_deg() - expected_widened_fov).abs() < 1e-9);
    assert_ne!(
        cone.arc_vertices_lat_lon_deg()[0],
        cone.uncertainty_widened_arc_vertices_lat_lon_deg()[0]
    );
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
    assert!(approx_cone.uncertainty_widened_horizontal_fov_deg() > 64.0);
    assert!(matches!(
        approx_cone.cone_class(),
        ViewingConeClass::IllustrativeReportedBearing {
            bearing_uncertainty_deg
        } if (*bearing_uncertainty_deg - 12.0).abs() < f64::EPSILON
    ));

    // Recording `CameraRepositioned` invalidates the calibrated orientation, preserves the old orientation
    // in `prior_orientations()`, and causes viewing-cone projection to fail until `update_camera_orientation`
    // supplies a new calibration version (`CAM-05`, `N9`).
    registry.record_health_transition(
        &cal_id,
        OpaqueId::new("trans:cone:repositioned")?,
        SourceLifecycleHealth::Degraded,
        Some(CameraHealthState::CameraRepositioned),
        2_000,
        "PTZ mount slewed 35 deg east during wind gust".to_owned(),
    )?;
    let cal_entry = registry.camera(&cal_id).ok_or("missing camera")?;
    assert_eq!(cal_entry.prior_orientations().len(), 1);
    assert!(matches!(
        cal_entry.to_anomaly_source_profile()?.calibration,
        CameraCalibrationState::InvalidatedByMovement {
            detected_at_ms: 2_000,
            ..
        }
    ));
    assert!(matches!(
        registry.compute_viewing_cone(&cal_id, 1_000.0, 3),
        Err(CameraDirectoryError::OrientationUnavailableForViewingCone { .. })
    ));

    // Re-calibrating with a new `calibration_version` restores viewing-cone projection while preserving
    // orientation history (`N9`).
    let recal_entry = registry.update_camera_orientation(
        &cal_id,
        CameraViewingOrientation::Calibrated {
            calibration_version: VersionId::new("cal:srcver:cone:1:v2")?,
            azimuth_deg: 35.0,
            pitch_deg: Some(15.0),
            roll_deg: Some(0.0),
            horizontal_fov_deg: 60.0,
            vertical_fov_deg: Some(40.0),
            max_angular_error_mdeg: 150,
        },
    )?;
    assert_eq!(recal_entry.prior_orientations().len(), 2);
    assert!(
        registry
            .compute_viewing_cone(&cal_id, 1_000.0, 3)?
            .is_calibrated()
    );

    // Polar apex (`latitude_deg = 90.0`) is refused as degenerate (`PolarApexUnsupportedForViewingCone`, `N9`).
    let mut pole_reg = sample_registration("cam:cone-pole", "srcver:cone:3")?;
    pole_reg.position = CameraHorizontalPosition::KnownWgs84 {
        latitude_deg: 90.0,
        longitude_deg: 0.0,
        horizontal_uncertainty_m: 5.0,
    };
    let pole_id = pole_reg.camera_id.clone();
    registry.register_camera(pole_reg)?;
    assert!(matches!(
        registry.compute_viewing_cone(&pole_id, 1_000.0, 3),
        Err(CameraDirectoryError::PolarApexUnsupportedForViewingCone { .. })
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
fn cam02_frame_classification_capture_time_honesty_still_freshness_and_archive_bounds()
-> Result<(), Box<dyn std::error::Error>> {
    let mut registry = CameraDirectoryRegistry::new(default_limits()?);
    let reg = sample_registration("cam:classify-01", "srcver:classify:1")?;
    let cam_id = reg.camera_id.clone();
    registry.register_camera(reg)?;

    // 1. Fresh periodic still with explicit `source_capture_time_ms` (`stale_after_ms = 10_000`)
    // is eligible as event evidence and preserves the exact `source_capture_time_ms` (`B2`).
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
    assert_eq!(still_1.to_retained_frame_ref()?.capture_time_ms, 1_000);
    assert!(matches!(
        still_1.freshness(),
        FrameFreshnessEvaluation::FreshPeriodicStill {
            stale_after_ms: 10_000,
            source_capture_time_ms: 1_000,
            ingestion_time_ms: 2_000,
            observed_capture_age_ms: 1_000,
            unchanged_hold_duration_ms: 0,
        }
    ));

    // 2. Periodic still WITHOUT `source_capture_time_ms` and a new digest is classified as
    // `PeriodicStillCaptureTimeUnknown` (`N2`) rather than unconditionally fresh, is NOT eligible as
    // event evidence, and `to_retained_frame_ref()` refuses to fabricate `capture_time_ms` from
    // `ingestion_time_ms` (`B2`: `MissingSourceCaptureTimeForEvidence`).
    let still_unknown_cap = registry.ingest_frame(
        &cam_id,
        CameraFrameSubmission {
            frame_version: VersionId::new("ver:still:unknown-cap")?,
            sha256_hex: hex64('9'),
            byte_len: 1_024,
            sequence_number: 2,
            declared_kind: FeedObservationKind::PeriodicUpstreamStill,
            origin: FrameArtifactOrigin::OriginalRetainedFrame,
            source_capture_time_ms: None,
            clock_uncertainty_ms: 50,
            ingestion_time_ms: 3_000,
            archive_version: None,
        },
    )?;
    assert!(matches!(
        still_unknown_cap.freshness(),
        FrameFreshnessEvaluation::PeriodicStillCaptureTimeUnknown {
            stale_after_ms: 10_000,
            ingestion_time_ms: 3_000,
            unchanged_hold_duration_ms: 0,
        }
    ));
    assert!(!still_unknown_cap.eligible_as_event_evidence());
    assert!(!still_unknown_cap.retained_in_ring_buffer());
    assert_eq!(
        still_unknown_cap.to_retained_frame_ref(),
        Err(CameraDirectoryError::MissingSourceCaptureTimeForEvidence {
            frame_version: VersionId::new("ver:still:unknown-cap")?,
        })
    );

    // 3. Unchanged digest held past `stale_after_ms` (at `ingestion_time_ms = 13_500`, hold = `10_500 > 10_000`)
    // is downgraded to `StaleStillHold`, cannot become evidence, is omitted from the ring buffer,
    // and marks camera health `Stale`.
    let still_stale = registry.ingest_frame(
        &cam_id,
        CameraFrameSubmission {
            frame_version: VersionId::new("ver:still:stale")?,
            sha256_hex: hex64('9'),
            byte_len: 1_024,
            sequence_number: 3,
            declared_kind: FeedObservationKind::PeriodicUpstreamStill,
            origin: FrameArtifactOrigin::OriginalRetainedFrame,
            source_capture_time_ms: None,
            clock_uncertainty_ms: 50,
            ingestion_time_ms: 13_500,
            archive_version: None,
        },
    )?;
    assert_eq!(
        still_stale.effective_kind(),
        FeedObservationKind::StaleStillHold
    );
    assert!(!still_stale.eligible_as_event_evidence());
    assert!(!still_stale.retained_in_ring_buffer());
    assert!(matches!(
        still_stale.to_retained_frame_ref(),
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

    // 4. Non-evidentiary substitutes (`StillFallback`, `StreetViewFallback`, `SyntheticPlaceholder`)
    // are never buffered, never eligible as event evidence, use their own substitute sequence space
    // (`N12`: `seq = 50..=52` does not mask a live sequence gap between `seq = 3` and `seq = 5`),
    // and never advance `last_live_capture_time_ms` (`N4`).
    for (idx, sub_kind) in [
        FeedObservationKind::StillFallback,
        FeedObservationKind::StreetViewFallback,
        FeedObservationKind::SyntheticPlaceholder,
    ]
    .into_iter()
    .enumerate()
    {
        let seq = 50 + idx as u64;
        let frame = registry.ingest_frame(
            &cam_id,
            CameraFrameSubmission {
                frame_version: VersionId::new(format!("ver:sub:{seq}"))?,
                sha256_hex: hex64('b'),
                byte_len: 512,
                sequence_number: seq,
                declared_kind: sub_kind,
                origin: FrameArtifactOrigin::OriginalRetainedFrame,
                source_capture_time_ms: Some(13_550),
                clock_uncertainty_ms: 50,
                ingestion_time_ms: 13_600 + (idx as i64) * 10,
                archive_version: None,
            },
        )?;
        assert!(!frame.eligible_as_event_evidence());
        assert!(!frame.retained_in_ring_buffer());
    }

    // Live frame at `sequence_number = 5` still records the 1-frame discontinuity after `sequence_number = 3`
    // despite the intervening substitutes at `50..=52` (`N12`).
    registry.ingest_frame(
        &cam_id,
        CameraFrameSubmission {
            frame_version: VersionId::new("ver:live:after-sub")?,
            sha256_hex: hex64('7'),
            byte_len: 1_024,
            sequence_number: 5,
            declared_kind: FeedObservationKind::UpstreamCameraFeed,
            origin: FrameArtifactOrigin::OriginalRetainedFrame,
            source_capture_time_ms: Some(13_700),
            clock_uncertainty_ms: 20,
            ingestion_time_ms: 13_720,
            archive_version: None,
        },
    )?;
    assert_eq!(
        registry
            .camera(&cam_id)
            .ok_or("missing")?
            .discontinuity_spans()
            .len(),
        1
    );

    // 5. Historical archive frame inside the verified window `[500, 900]` with matching `archive_version`
    // uses its own sequence space (`N12`: `sequence_number = 1` does not conflict with live `seq = 5`)
    // and is eligible as retrospective evidence (`to_retained_frame_ref()` succeeds) without entering the live ring buffer.
    let archive_ok = registry.ingest_frame(
        &cam_id,
        CameraFrameSubmission {
            frame_version: VersionId::new("ver:arch:700")?,
            sha256_hex: hex64('c'),
            byte_len: 1_024,
            sequence_number: 1,
            declared_kind: FeedObservationKind::HistoricalArchiveFrame,
            origin: FrameArtifactOrigin::OriginalRetainedFrame,
            source_capture_time_ms: Some(700),
            clock_uncertainty_ms: 50,
            ingestion_time_ms: 14_000,
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
            sequence_number: 2,
            declared_kind: FeedObservationKind::HistoricalArchiveFrame,
            origin: FrameArtifactOrigin::OriginalRetainedFrame,
            source_capture_time_ms: Some(950),
            clock_uncertainty_ms: 50,
            ingestion_time_ms: 14_100,
            archive_version: Some(VersionId::new("arch:srcver:classify:1")?),
        },
    );
    assert!(matches!(
        archive_oob,
        Err(CameraDirectoryError::ArchiveFrameOutsideVerifiedWindow { .. })
    ));

    // Live frame exceeding staleness bound (`capture_age_ms = 5_200 > max_retention_window_ms = 2_000`) is rejected (`N3`).
    let stale_live_err = registry.ingest_frame(
        &cam_id,
        CameraFrameSubmission {
            frame_version: VersionId::new("ver:live:stale")?,
            sha256_hex: hex64('e'),
            byte_len: 1_024,
            sequence_number: 7,
            declared_kind: FeedObservationKind::UpstreamCameraFeed,
            origin: FrameArtifactOrigin::OriginalRetainedFrame,
            source_capture_time_ms: Some(13_800),
            clock_uncertainty_ms: 10,
            ingestion_time_ms: 19_000,
            archive_version: None,
        },
    );
    assert!(matches!(
        stale_live_err,
        Err(CameraDirectoryError::LiveFrameExceedsStalenessBound { .. })
    ));

    // Generated/predictive frames and clock uncertainty > 24h are rejected (`AGENTS.md` §2).
    let gen_err = registry.ingest_frame(
        &cam_id,
        CameraFrameSubmission {
            frame_version: VersionId::new("ver:gen:1")?,
            sha256_hex: hex64('e'),
            byte_len: 1_024,
            sequence_number: 7,
            declared_kind: FeedObservationKind::UpstreamCameraFeed,
            origin: FrameArtifactOrigin::PredictiveOrGeneratedDerivative,
            source_capture_time_ms: Some(19_100),
            clock_uncertainty_ms: 10,
            ingestion_time_ms: 19_110,
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
            sequence_number: 7,
            declared_kind: FeedObservationKind::UpstreamCameraFeed,
            origin: FrameArtifactOrigin::OriginalRetainedFrame,
            source_capture_time_ms: Some(19_100),
            clock_uncertainty_ms: MAX_CLOCK_UNCERTAINTY_MS + 1,
            ingestion_time_ms: 19_110,
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
fn cam01_a09_restricted_cutoff_same_listing_reregistration_and_source_version_provenance()
-> Result<(), Box<dyn std::error::Error>> {
    let mut registry = CameraDirectoryRegistry::new(default_limits()?);
    let reg = sample_registration("cam:restricted-flow", "srcver:rest:1")?;
    let cam_id = reg.camera_id.clone();
    let original_listing = reg.listing_reference.clone();
    registry.register_camera(reg)?;

    // Ingest a frame under `srcver:rest:1` before restriction (`B5`).
    registry.ingest_frame(
        &cam_id,
        CameraFrameSubmission {
            frame_version: VersionId::new("ver:rest:pre-1")?,
            sha256_hex: hex64('0'),
            byte_len: 512,
            sequence_number: 10,
            declared_kind: FeedObservationKind::UpstreamCameraFeed,
            origin: FrameArtifactOrigin::OriginalRetainedFrame,
            source_capture_time_ms: Some(4_500),
            clock_uncertainty_ms: 20,
            ingestion_time_ms: 4_520,
            archive_version: None,
        },
    )?;

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
            sequence_number: 11,
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
        6_000,
        "Re-registering with reused version".to_owned(),
    );
    assert!(matches!(
        reuse_err,
        Err(CameraDirectoryError::ReusedVersionId(_))
    ));

    // Explicit re-registration on the SAME `listing_reference` (`B4`) with a new `source_version`
    // succeeds, preserves `prior_source_versions` and `restriction_intervals`, and resets live session
    // sequence state (`N12`).
    let updated = registry.reregister_ordinarily_accessible_version(
        &cam_id,
        OpaqueId::new("trans:rest:rereg-ok")?,
        VersionId::new("srcver:rest:2")?,
        6_000,
        "Original directory listing restored ordinary public access after maintenance".to_owned(),
    )?;
    assert_eq!(
        updated.access_status(),
        CameraAccessStatus::OrdinarilyAccessible
    );
    assert_eq!(updated.listing_reference(), original_listing);
    assert_eq!(updated.source_version(), &VersionId::new("srcver:rest:2")?);
    assert_eq!(
        updated.prior_source_versions(),
        &[VersionId::new("srcver:rest:1")?]
    );
    assert_eq!(updated.restriction_intervals(), &[(5_000, Some(6_000))]);

    // Ingest a frame in the new session (`srcver:rest:2`) starting at `sequence_number = 1` without
    // triggering a false discontinuity against the pre-restriction session (`seq = 10`).
    registry.ingest_frame(
        &cam_id,
        CameraFrameSubmission {
            frame_version: VersionId::new("ver:rest:post-1")?,
            sha256_hex: hex64('2'),
            byte_len: 512,
            sequence_number: 1,
            declared_kind: FeedObservationKind::UpstreamCameraFeed,
            origin: FrameArtifactOrigin::OriginalRetainedFrame,
            source_capture_time_ms: Some(6_100),
            clock_uncertainty_ms: 20,
            ingestion_time_ms: 6_120,
            archive_version: None,
        },
    )?;

    // Extracting a clip over `[4_400, 4_600)` with a post-context window (`requested_post_context_ms = 1_800`,
    // `post_end_ms = 6_400`) that spans the restriction interval `[5_000, 6_000]` and the `srcver:rest:2`
    // frame at `6_100`:
    // - Stamps the manifest with `srcver:rest:1` (`B5`).
    // - Records the excluded `srcver:rest:2` frame in `excluded_cross_version_frames()` (`B1'`).
    // - Records the restriction interval `(5_000, Some(6_000))` in `restriction_intervals_in_window()` (`B1'`).
    // - Reports `PartialBufferExpired` rather than `Complete` (`B1'`).
    let pre_manifest = registry.extract_retained_clip_manifest(
        &cam_id,
        ClipExtractionRequest {
            clip_id: OpaqueId::new("clip:rest:pre")?,
            clip_version: VersionId::new("clipver:rest:pre:1")?,
            source_session_id: OpaqueId::new("sess:rest:1")?,
            event_start_ms: 4_400,
            event_end_ms: 4_600,
            requested_pre_context_ms: 0,
            requested_post_context_ms: 1_800,
            extracted_at_ms: 6_500,
            trigger_explanation: "Pre-restriction event clip spanning restriction".to_owned(),
        },
    )?;
    assert_eq!(
        pre_manifest.source_version(),
        &VersionId::new("srcver:rest:1")?
    );
    assert_eq!(pre_manifest.excluded_cross_version_frames().len(), 1);
    assert_eq!(
        pre_manifest.restriction_intervals_in_window(),
        &[(5_000, Some(6_000))]
    );
    assert!(matches!(
        pre_manifest.context_window(),
        ContextWindowStatus::PartialBufferExpired { .. }
    ));

    Ok(())
}

#[test]
fn cam04_a09_fifo_eviction_context_completeness_and_anomaly_monitor_bridge()
-> Result<(), Box<dyn std::error::Error>> {
    // Buffer limit: max 3 buffered frames, max 3_072 bytes, `max_retention_window_ms = 2_000`.
    let limits = CameraDirectoryLimits::new(4, 4, 8, 3, 3_072, 8, 8, 8, 16_384)?;
    let mut registry = CameraDirectoryRegistry::new(limits);
    let reg = sample_registration("cam:ring-fifo", "srcver:ring:1")?;
    let cam_id = reg.camera_id.clone();
    registry.register_camera(reg)?;

    // Ingest 3 frames at `ingestion_time_ms = 10_000` with increasing `sequence_number` (1, 2, 3),
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

    // 1. Window `[12_400, 12_600)` with `requested_pre_context_ms = 3_000` (`pre_start_ms = 9_400`)
    // and `requested_post_context_ms = 500` (`post_end_ms = 13_100`), extracted at `13_200`:
    // - 3 expired by window (`f2, f3, f4`) + 1 evicted by capacity (`f1`) + 2 dropped frames (`seq 5, 6`)
    //   = 6 exact frame losses in `PartialBufferExpired.expired_frame_count`, with `empty_context_window_count == 1`
    //   kept separate (`B1`, `B7`, `N5`).
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
                extracted_at_ms: 13_200,
                trigger_explanation: "Simulated transient in ring buffer".to_owned(),
            },
        )?
        .clone();

    assert_eq!(manifest.expired_by_window_in_span(), 3);
    assert_eq!(manifest.evicted_by_capacity_in_span(), 1);
    assert!(!manifest.eviction_ledger_truncated());
    assert_eq!(manifest.dropped_frame_count_in_span(), 2);
    assert_eq!(manifest.empty_context_window_count(), 1);
    assert!(manifest.missing_pre_context());
    assert!(!manifest.missing_post_context());
    assert_eq!(manifest.post_context_frames().len(), 1);
    assert_eq!(manifest.post_context_frames()[0].byte_len(), 1_024);
    assert_eq!(manifest.post_context_frames()[0].sequence_number(), 8);
    assert!(matches!(
        manifest.context_window(),
        ContextWindowStatus::PartialBufferExpired {
            expired_frame_count: 6,
            pre_frames,
            post_frames,
        } if pre_frames.is_empty() && post_frames.len() == 1
    ));

    // 2. Ingest Frame 7 at `13_100` ms (seq 9, contiguous), then test `[12_700, 12_900)`:
    // - With `requested_pre_context_ms = 1_000` (`pre_start_ms = 11_700`, first frame at `12_500`,
    //   lead gap `800 ms > 2 * 250 ms = 500 ms`), `coverage_gaps_in_window` records the edge gap and
    //   marks the manifest `PartialBufferExpired` (`B1'`).
    // - With `pre = 200` and `post = 300`, when `extracted_at_ms = 13_150 < post_end_ms`,
    //   `missing_post_context` is true -> `PartialBufferExpired` (`B1`).
    // - When `extracted_at_ms = 13_300 >= post_end_ms` (and no drops/evictions/gaps in `[12_500, 13_200]`),
    //   `ContextWindowStatus::Complete` is returned (`B1`).
    registry.ingest_frame(
        &cam_id,
        CameraFrameSubmission {
            frame_version: VersionId::new("ver:f7")?,
            sha256_hex: hex64('7'),
            byte_len: 1_024,
            sequence_number: 9,
            declared_kind: FeedObservationKind::UpstreamCameraFeed,
            origin: FrameArtifactOrigin::OriginalRetainedFrame,
            source_capture_time_ms: Some(13_100),
            clock_uncertainty_ms: 20,
            ingestion_time_ms: 13_120,
            archive_version: None,
        },
    )?;

    let edge_gap_manifest = registry.extract_retained_clip_manifest(
        &cam_id,
        ClipExtractionRequest {
            clip_id: OpaqueId::new("clip:ring:edge-gap")?,
            clip_version: VersionId::new("clipver:ring:edge-gap")?,
            source_session_id: OpaqueId::new("sess:ring:01")?,
            event_start_ms: 12_700,
            event_end_ms: 12_900,
            requested_pre_context_ms: 1_000,
            requested_post_context_ms: 300,
            extracted_at_ms: 13_300,
            trigger_explanation: "Edge gap at start of pre window".to_owned(),
        },
    )?;
    assert_eq!(edge_gap_manifest.coverage_gaps_in_window().len(), 1);
    assert!(matches!(
        edge_gap_manifest.context_window(),
        ContextWindowStatus::PartialBufferExpired { .. }
    ));

    let early_manifest = registry.extract_retained_clip_manifest(
        &cam_id,
        ClipExtractionRequest {
            clip_id: OpaqueId::new("clip:ring:early")?,
            clip_version: VersionId::new("clipver:ring:early")?,
            source_session_id: OpaqueId::new("sess:ring:01")?,
            event_start_ms: 12_700,
            event_end_ms: 12_900,
            requested_pre_context_ms: 200,
            requested_post_context_ms: 300,
            extracted_at_ms: 13_150,
            trigger_explanation: "Extracted before post window elapsed".to_owned(),
        },
    )?;
    assert!(early_manifest.missing_post_context());
    assert_eq!(early_manifest.empty_context_window_count(), 1);
    assert!(matches!(
        early_manifest.context_window(),
        ContextWindowStatus::PartialBufferExpired {
            expired_frame_count: 1,
            ..
        }
    ));

    let complete_manifest = registry.extract_retained_clip_manifest(
        &cam_id,
        ClipExtractionRequest {
            clip_id: OpaqueId::new("clip:ring:complete")?,
            clip_version: VersionId::new("clipver:ring:complete")?,
            source_session_id: OpaqueId::new("sess:ring:01")?,
            event_start_ms: 12_700,
            event_end_ms: 12_900,
            requested_pre_context_ms: 200,
            requested_post_context_ms: 300,
            extracted_at_ms: 13_300,
            trigger_explanation: "Complete pre and post context with zero drops".to_owned(),
        },
    )?;
    assert!(matches!(
        complete_manifest.context_window(),
        ContextWindowStatus::Complete {
            pre_frames,
            post_frames,
        } if pre_frames.len() == 1 && post_frames.len() == 1
    ));

    // 3. Verify `eviction_ledger_truncated` (`B7`) when more than `max(4 * max_buffered_frames, 16)`
    // frames are evicted inside `[pre_start_ms, post_end_ms]`:
    for idx in 10..=30_u64 {
        let cap_ms = 13_100 + ((idx - 9) as i64) * 50;
        registry.ingest_frame(
            &cam_id,
            CameraFrameSubmission {
                frame_version: VersionId::new(format!("ver:burst:{idx}"))?,
                sha256_hex: hex64('8'),
                byte_len: 1_024,
                sequence_number: idx,
                declared_kind: FeedObservationKind::UpstreamCameraFeed,
                origin: FrameArtifactOrigin::OriginalRetainedFrame,
                source_capture_time_ms: Some(cap_ms),
                clock_uncertainty_ms: 20,
                ingestion_time_ms: cap_ms + 10,
                archive_version: None,
            },
        )?;
    }
    let truncated_manifest = registry.extract_retained_clip_manifest(
        &cam_id,
        ClipExtractionRequest {
            clip_id: OpaqueId::new("clip:ring:truncated")?,
            clip_version: VersionId::new("clipver:ring:truncated")?,
            source_session_id: OpaqueId::new("sess:ring:01")?,
            event_start_ms: 14_100,
            event_end_ms: 14_200,
            requested_pre_context_ms: 1_500,
            requested_post_context_ms: 0,
            extracted_at_ms: 14_300,
            trigger_explanation: "Eviction ledger rolled off older records in window".to_owned(),
        },
    )?;
    assert!(truncated_manifest.eviction_ledger_truncated());
    assert!(matches!(
        truncated_manifest.context_window(),
        ContextWindowStatus::PartialBufferExpired { .. }
    ));

    // 4. Verify `R1`: coverage-gap evaluation over `[pre_start_ms, post_end_ms]` even when
    // `requested_pre_context_ms == 0` or `requested_post_context_ms == 0`.
    let mut r1_registry = CameraDirectoryRegistry::new(CameraDirectoryLimits::new(
        4, 4, 8, 8, 16_384, 8, 8, 8, 16_384,
    )?);
    let r1_reg = sample_registration("cam:r1-zero-context", "srcver:r1:1")?;
    let r1_cam_id = r1_reg.camera_id.clone();
    r1_registry.register_camera(r1_reg)?;

    for (idx, cap_ms) in [(1_u64, 2_900_i64), (2_u64, 3_000_i64), (3_u64, 4_000_i64)] {
        r1_registry.ingest_frame(
            &r1_cam_id,
            CameraFrameSubmission {
                frame_version: VersionId::new(format!("ver:r1:{idx}"))?,
                sha256_hex: hex64('a'),
                byte_len: 512,
                sequence_number: idx,
                declared_kind: FeedObservationKind::UpstreamCameraFeed,
                origin: FrameArtifactOrigin::OriginalRetainedFrame,
                source_capture_time_ms: Some(cap_ms),
                clock_uncertainty_ms: 20,
                ingestion_time_ms: cap_ms + 10,
                archive_version: None,
            },
        )?;
    }

    // Probe R1a: `requested_pre_context_ms == 0 && requested_post_context_ms == 0` over `[2_900, 4_100)`
    // has a 1_000 ms mid-event stall (`3_000 -> 4_000 > 2 * 250 = 500 ms`) with contiguous sequence numbers;
    // must record `coverage_gaps_in_window` and report `PartialBufferExpired`.
    let r1_zero_ctx = r1_registry.extract_retained_clip_manifest(
        &r1_cam_id,
        ClipExtractionRequest {
            clip_id: OpaqueId::new("clip:r1:zero-ctx")?,
            clip_version: VersionId::new("clipver:r1:zero-ctx")?,
            source_session_id: OpaqueId::new("sess:r1:1")?,
            event_start_ms: 2_900,
            event_end_ms: 4_100,
            requested_pre_context_ms: 0,
            requested_post_context_ms: 0,
            extracted_at_ms: 4_200,
            trigger_explanation: "Zero-context clip with 1000 ms mid-event stall".to_owned(),
        },
    )?;
    assert_eq!(r1_zero_ctx.coverage_gaps_in_window().len(), 1);
    assert_eq!(r1_zero_ctx.coverage_gaps_in_window()[0].start_ms(), 3_000);
    assert_eq!(r1_zero_ctx.coverage_gaps_in_window()[0].end_ms(), 4_000);
    assert!(matches!(
        r1_zero_ctx.context_window(),
        ContextWindowStatus::PartialBufferExpired { .. }
    ));

    // Probe R1b: `requested_pre_context_ms == 0, requested_post_context_ms = 200` over `[3_050, 4_050)`
    // (with frame 4 at `4_100` in post-context) has a 950 ms lead gap from `event_start_ms = 3_050`
    // to first retained frame at `4_000`; must record the lead gap and report `PartialBufferExpired`.
    r1_registry.ingest_frame(
        &r1_cam_id,
        CameraFrameSubmission {
            frame_version: VersionId::new("ver:r1:4")?,
            sha256_hex: hex64('b'),
            byte_len: 512,
            sequence_number: 4,
            declared_kind: FeedObservationKind::UpstreamCameraFeed,
            origin: FrameArtifactOrigin::OriginalRetainedFrame,
            source_capture_time_ms: Some(4_100),
            clock_uncertainty_ms: 20,
            ingestion_time_ms: 4_110,
            archive_version: None,
        },
    )?;
    let r1_lead_gap = r1_registry.extract_retained_clip_manifest(
        &r1_cam_id,
        ClipExtractionRequest {
            clip_id: OpaqueId::new("clip:r1:lead-gap")?,
            clip_version: VersionId::new("clipver:r1:lead-gap")?,
            source_session_id: OpaqueId::new("sess:r1:1")?,
            event_start_ms: 3_050,
            event_end_ms: 4_050,
            requested_pre_context_ms: 0,
            requested_post_context_ms: 200,
            extracted_at_ms: 4_300,
            trigger_explanation: "Zero pre-context with 950 ms lead gap inside event window"
                .to_owned(),
        },
    )?;
    assert_eq!(r1_lead_gap.coverage_gaps_in_window().len(), 1);
    assert_eq!(r1_lead_gap.coverage_gaps_in_window()[0].start_ms(), 3_050);
    assert_eq!(r1_lead_gap.coverage_gaps_in_window()[0].end_ms(), 4_000);
    assert!(matches!(
        r1_lead_gap.context_window(),
        ContextWindowStatus::PartialBufferExpired { .. }
    ));

    // Verify `nominal_frame_interval_ms > MAX_CLOCK_UNCERTAINTY_MS` rejection and `prior_orientations` cap.
    let mut bad_cap_reg = sample_registration("cam:bad-interval", "srcver:bad-int:1")?;
    bad_cap_reg.stream_capability = StreamCapability::LiveStreamSupported {
        nominal_frame_interval_ms: u64::MAX,
    };
    assert!(matches!(
        r1_registry.register_camera(bad_cap_reg),
        Err(CameraDirectoryError::InvalidCapabilityConfiguration { .. })
    ));

    // Bridge directly into `CameraAnomalyMonitor` (`AGENTS.md` §4).
    let entry = registry.camera(&cam_id).ok_or("missing")?;
    let mut monitor = CameraAnomalyMonitor::new(
        entry.to_anomaly_source_profile()?,
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
    let mut still_profile = entry.to_anomaly_source_profile()?;
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
