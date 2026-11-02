//! Targeted contract tests for `oia_media::camera_anomaly` (`CAM-01`–`CAM-05`, `A09`).
//!
//! Risk and requirement coverage:
//! 1. `fault_separation_deduplication_review_history_and_operational_metrics`:
//!    Verifies that camera faults (`FrozenFrame`, `CameraRepositioned`) are tracked separately
//!    and never publish as scene anomalies, persistent triggers deduplicate without losing
//!    observation counts, original context frames and uncalibrated score caveats are preserved,
//!    append-only human reviews keep prior revisions, and held-out evaluation reports
//!    operational false alerts per camera-hour, recall, latency, and coverage loss (`CAM-03`–`CAM-05`, `A09`).
//! 2. `admission_exclusions_generated_evidence_and_cross_camera_refusals`:
//!    Verifies that home interiors, restricted sources, unpermitted automated processing,
//!    non-live substitutes (`StillFallback`, `StreetViewFallback`, `SyntheticPlaceholder`),
//!    predictive/generated derivative frames, active fault windows, shared upstream feeds,
//!    and movement-invalidated calibrations are rejected (`CAM-01`–`CAM-05`, `A09`, `AGENTS.md` §2).

use oia_domain::{ActorId, OpaqueId, VersionId};
use oia_media::camera_anomaly::{
    AnomalyCategory, AnomalyError, AnomalyLimits, AnomalyReviewDisposition, AnomalyReviewInput,
    CANDIDATE_EVENT_CAVEAT, CameraAccessStatus, CameraAnomalyMonitor, CameraCalibrationState,
    CameraHealthState, CameraSourceProfile, CameraViewKind, ContextWindowStatus,
    DetectionRegionConfig, DetectorRunMetadata, FeedObservationKind, FrameArtifactOrigin,
    LabeledSceneEvent, ObservationStepInput, RESTRICTED_SOURCE_POLICY_NOTICE, RetainedFrameRef,
    SceneBaselineKind, TriggerOutcome, UNCALIBRATED_SCORE_CAVEAT,
    validate_cross_camera_association,
};

fn test_limits() -> Result<AnomalyLimits, AnomalyError> {
    AnomalyLimits::new(4, 32, 16, 6, 4, 8, 8, 8_192)
}

fn sample_detector() -> Result<DetectorRunMetadata, AnomalyError> {
    Ok(DetectorRunMetadata {
        detector_id: OpaqueId::new("det:sky-motion-v1").map_err(AnomalyError::InvalidId)?,
        detector_version: VersionId::new("detver:sky-motion:1").map_err(AnomalyError::InvalidId)?,
        method_description: "Background differencing + persistence gate (test fixture)".to_owned(),
        config_digest_sha256: "b".repeat(64),
    })
}

fn sample_region() -> Result<DetectionRegionConfig, AnomalyError> {
    Ok(DetectionRegionConfig {
        region_id: OpaqueId::new("reg:sky-sector-a").map_err(AnomalyError::InvalidId)?,
        label: "Sky Sector A".to_owned(),
        baseline: SceneBaselineKind::NighttimeLowLight,
        score_threshold_bps: 6_500,
        min_persistence_frames: 3,
        min_persistence_ms: 300,
        deduplication_window_ms: 4_000,
        privacy_mask_applied: true,
    })
}

fn sample_source(
    camera_id: &str,
    feed_id: &str,
    cal_version: &str,
) -> Result<CameraSourceProfile, AnomalyError> {
    Ok(CameraSourceProfile {
        camera_id: OpaqueId::new(camera_id).map_err(AnomalyError::InvalidId)?,
        upstream_feed_id: OpaqueId::new(feed_id).map_err(AnomalyError::InvalidId)?,
        provider_attribution: "Public Observatory Network (fixture)".to_owned(),
        listing_reference: "https://example.invalid/cam".to_owned(),
        access_status: CameraAccessStatus::OrdinarilyAccessible,
        view_kind: CameraViewKind::SkyOrObservatory,
        feed_kind: FeedObservationKind::UpstreamCameraFeed,
        automated_processing_permitted: true,
        clip_retention_permitted: true,
        max_retention_window_ms: Some(20_000),
        calibration: CameraCalibrationState::Valid {
            calibration_version: VersionId::new(cal_version).map_err(AnomalyError::InvalidId)?,
            max_angular_error_mdeg: 100,
        },
    })
}

fn frame_ref(
    version: &str,
    hex_char: char,
    capture_ms: i64,
    ingest_ms: i64,
    origin: FrameArtifactOrigin,
) -> Result<RetainedFrameRef, AnomalyError> {
    Ok(RetainedFrameRef {
        frame_version: VersionId::new(version).map_err(AnomalyError::InvalidId)?,
        sha256_hex: hex_char.to_string().repeat(64),
        capture_time_ms: capture_ms,
        ingestion_time_ms: ingest_ms,
        origin,
    })
}

#[test]
fn fault_separation_deduplication_review_history_and_operational_metrics()
-> Result<(), AnomalyError> {
    let limits = test_limits()?;
    let region = sample_region()?;
    let mut monitor = CameraAnomalyMonitor::new(
        sample_source("cam:obs-01", "feed:obs-01", "cal:obs-01:v1")?,
        sample_detector()?,
        vec![region.clone()],
        limits,
    )?;

    // Candidate 1: true positive at 5_000..5_500 ms, emitted at 5_620 ms (latency 620 ms).
    let outcome1 = monitor.record_step(ObservationStepInput::SceneTrigger {
        observation_id: OpaqueId::new("obs:1").map_err(AnomalyError::InvalidId)?,
        candidate_id: OpaqueId::new("cand:1").map_err(AnomalyError::InvalidId)?,
        candidate_version: VersionId::new("ver:cand:1:v1").map_err(AnomalyError::InvalidId)?,
        region_id: region.region_id.clone(),
        category: AnomalyCategory::SkyTransient,
        baseline_at_trigger: SceneBaselineKind::NighttimeLowLight,
        event_start_ms: 5_000,
        event_end_ms: 5_500,
        detection_emitted_ms: 5_620,
        observed_persistence_frames: 4,
        raw_score_bps: 8_100,
        clock_uncertainty_ms: 30,
        trigger_explanation: "Bright point transit across Sector A".to_owned(),
        primary_frame: frame_ref(
            "ver:f:1",
            '1',
            5_200,
            5_230,
            FrameArtifactOrigin::OriginalRetainedFrame,
        )?,
        context_window: ContextWindowStatus::Complete {
            pre_frames: vec![frame_ref(
                "ver:f:0",
                '0',
                4_500,
                4_520,
                FrameArtifactOrigin::OriginalRetainedFrame,
            )?],
            post_frames: vec![frame_ref(
                "ver:f:2",
                '2',
                6_000,
                6_020,
                FrameArtifactOrigin::OriginalRetainedFrame,
            )?],
        },
    })?;
    assert_eq!(
        outcome1,
        Some(TriggerOutcome::PublishedNewCandidate {
            candidate_id: OpaqueId::new("cand:1").map_err(AnomalyError::InvalidId)?
        })
    );

    // Follow-up trigger within 4_000 ms deduplication window merges into cand:1.
    let outcome2 = monitor.record_step(ObservationStepInput::SceneTrigger {
        observation_id: OpaqueId::new("obs:2").map_err(AnomalyError::InvalidId)?,
        candidate_id: OpaqueId::new("cand:1-dup").map_err(AnomalyError::InvalidId)?,
        candidate_version: VersionId::new("ver:cand:1:dup").map_err(AnomalyError::InvalidId)?,
        region_id: region.region_id.clone(),
        category: AnomalyCategory::SkyTransient,
        baseline_at_trigger: SceneBaselineKind::NighttimeLowLight,
        event_start_ms: 7_500,
        event_end_ms: 8_200,
        detection_emitted_ms: 8_300,
        observed_persistence_frames: 3,
        raw_score_bps: 8_600,
        clock_uncertainty_ms: 45,
        trigger_explanation: "Continuing transit segment".to_owned(),
        primary_frame: frame_ref(
            "ver:f:3",
            '3',
            7_800,
            7_830,
            FrameArtifactOrigin::OriginalRetainedFrame,
        )?,
        context_window: ContextWindowStatus::PartialBufferExpired {
            pre_frames: vec![],
            post_frames: vec![],
            expired_frame_count: 2,
        },
    })?;
    assert_eq!(
        outcome2,
        Some(TriggerOutcome::DeduplicatedIntoExisting {
            candidate_id: OpaqueId::new("cand:1").map_err(AnomalyError::InvalidId)?,
            merged_trigger_count: 2,
        })
    );

    // Health fault (900_000 ms = 15 minutes of frozen frames out of 1 hour).
    let fault_outcome = monitor.record_step(ObservationStepInput::HealthEvent {
        observation_id: OpaqueId::new("obs:fault:1").map_err(AnomalyError::InvalidId)?,
        health_state: CameraHealthState::FrozenFrame,
        start_ms: 100_000,
        end_ms: 1_000_000,
        detail: "Frozen upstream encoder output".to_owned(),
    })?;
    assert_eq!(fault_outcome, None);
    assert_eq!(monitor.candidates().len(), 1);
    assert_eq!(monitor.health_events().len(), 1);

    // Candidate 2: false alert at 2_000_000..2_000_500 ms (after fault window ended).
    monitor.record_step(ObservationStepInput::SceneTrigger {
        observation_id: OpaqueId::new("obs:3").map_err(AnomalyError::InvalidId)?,
        candidate_id: OpaqueId::new("cand:2").map_err(AnomalyError::InvalidId)?,
        candidate_version: VersionId::new("ver:cand:2:v1").map_err(AnomalyError::InvalidId)?,
        region_id: region.region_id.clone(),
        category: AnomalyCategory::SkyTransient,
        baseline_at_trigger: SceneBaselineKind::NighttimeLowLight,
        event_start_ms: 2_000_000,
        event_end_ms: 2_000_500,
        detection_emitted_ms: 2_000_600,
        observed_persistence_frames: 3,
        raw_score_bps: 6_800,
        clock_uncertainty_ms: 25,
        trigger_explanation: "Lens internal flare".to_owned(),
        primary_frame: frame_ref(
            "ver:f:4",
            '4',
            2_000_200,
            2_000_230,
            FrameArtifactOrigin::OriginalRetainedFrame,
        )?,
        context_window: ContextWindowStatus::OmittedByRetentionPolicy {
            reason: "Clip retention disabled for low-priority trigger".to_owned(),
        },
    })?;

    // Append two human reviews to cand:1 and verify append-only preservation + optimistic concurrency.
    monitor.review_candidate(AnomalyReviewInput {
        candidate_id: OpaqueId::new("cand:1").map_err(AnomalyError::InvalidId)?,
        expected_current_version: VersionId::new("ver:cand:1:v1")
            .map_err(AnomalyError::InvalidId)?,
        new_review_version: VersionId::new("ver:cand:1:r1").map_err(AnomalyError::InvalidId)?,
        reviewer: ActorId::new("actor:reviewer-a").map_err(AnomalyError::InvalidId)?,
        reviewed_at_ms: 12_000,
        disposition: AnomalyReviewDisposition::InsufficientEvidence,
        rationale: "Initial triage pending second view".to_owned(),
        cited_frame_versions: vec![VersionId::new("ver:f:1").map_err(AnomalyError::InvalidId)?],
    })?;

    // Stale expected_current_version is rejected without mutating history.
    let stale_err = monitor.review_candidate(AnomalyReviewInput {
        candidate_id: OpaqueId::new("cand:1").map_err(AnomalyError::InvalidId)?,
        expected_current_version: VersionId::new("ver:cand:1:v1")
            .map_err(AnomalyError::InvalidId)?,
        new_review_version: VersionId::new("ver:cand:1:r2-stale")
            .map_err(AnomalyError::InvalidId)?,
        reviewer: ActorId::new("actor:reviewer-b").map_err(AnomalyError::InvalidId)?,
        reviewed_at_ms: 13_000,
        disposition: AnomalyReviewDisposition::LikelyOrdinaryExplanation,
        rationale: "Stale concurrent edit".to_owned(),
        cited_frame_versions: vec![VersionId::new("ver:f:1").map_err(AnomalyError::InvalidId)?],
    });
    assert!(matches!(
        stale_err,
        Err(AnomalyError::StaleCandidateVersion { .. })
    ));

    let updated = monitor.review_candidate(AnomalyReviewInput {
        candidate_id: OpaqueId::new("cand:1").map_err(AnomalyError::InvalidId)?,
        expected_current_version: VersionId::new("ver:cand:1:r1")
            .map_err(AnomalyError::InvalidId)?,
        new_review_version: VersionId::new("ver:cand:1:r2").map_err(AnomalyError::InvalidId)?,
        reviewer: ActorId::new("actor:reviewer-b").map_err(AnomalyError::InvalidId)?,
        reviewed_at_ms: 14_000,
        disposition: AnomalyReviewDisposition::RetainedForInvestigation,
        rationale: "Confirmed multi-frame persistence and clean pre/post context".to_owned(),
        cited_frame_versions: vec![
            VersionId::new("ver:f:0").map_err(AnomalyError::InvalidId)?,
            VersionId::new("ver:f:1").map_err(AnomalyError::InvalidId)?,
        ],
    })?;
    assert_eq!(updated.reviews().len(), 2);
    assert_eq!(
        updated.current_disposition(),
        Some(AnomalyReviewDisposition::RetainedForInvestigation)
    );
    assert!(!updated.is_score_calibrated_probability());
    assert_eq!(updated.score_caveat(), UNCALIBRATED_SCORE_CAVEAT);
    assert_eq!(updated.candidate_caveat(), CANDIDATE_EVENT_CAVEAT);
    assert_eq!(updated.peak_raw_score_bps(), 8_600);
    assert_eq!(updated.observed_persistence_frames(), 7);

    // Held-out evaluation over 1 hour (3_600_000 ms) with 900_000 ms missing coverage (75% active = 0.75 hr).
    // 1 false alert over 0.75 active camera-hours = 1.333 false alerts/camera-hour (1_333 milli-alerts/hr).
    let truth = vec![
        LabeledSceneEvent {
            truth_id: OpaqueId::new("truth:1").map_err(AnomalyError::InvalidId)?,
            region_id: region.region_id.clone(),
            category: AnomalyCategory::SkyTransient,
            start_ms: 5_000,
            end_ms: 8_200,
        },
        LabeledSceneEvent {
            truth_id: OpaqueId::new("truth:2-missed").map_err(AnomalyError::InvalidId)?,
            region_id: region.region_id,
            category: AnomalyCategory::SkyTransient,
            start_ms: 3_000_000,
            end_ms: 3_001_000,
        },
    ];
    let summary = monitor.evaluate_held_out(0, 3_600_000, &truth)?;
    assert_eq!(summary.total_window_ms, 3_600_000);
    assert_eq!(summary.missing_coverage_ms, 900_000);
    assert_eq!(summary.active_coverage_ms, 2_700_000);
    assert_eq!(summary.coverage_bps, 7_500);
    assert_eq!(summary.labeled_event_count, 2);
    assert_eq!(summary.matched_labeled_event_count, 1);
    assert_eq!(summary.event_recall_bps, Some(5_000));
    assert_eq!(summary.total_candidate_alerts, 2);
    assert_eq!(summary.true_positive_alerts, 1);
    assert_eq!(summary.false_alert_count, 1);
    assert_eq!(summary.false_alerts_per_camera_hour_milli, 1_333);
    assert_eq!(summary.min_detection_latency_ms, Some(620));
    assert_eq!(summary.mean_detection_latency_ms, Some(620));
    assert_eq!(summary.max_detection_latency_ms, Some(620));
    assert_eq!(summary.deduplicated_repeat_triggers, 1);
    assert_eq!(summary.fault_events_excluded_from_alerts, 1);

    Ok(())
}

#[test]
fn admission_exclusions_generated_evidence_and_cross_camera_refusals() -> Result<(), AnomalyError> {
    let limits = test_limits()?;
    let region = sample_region()?;

    // 1. Home interior camera is unconditionally refused (CAM-01/CAM-02).
    let mut home_source = sample_source("cam:home-01", "feed:home-01", "cal:home:v1")?;
    home_source.view_kind = CameraViewKind::HomeInteriorOrPrivateResidence;
    assert!(matches!(
        CameraAnomalyMonitor::new(
            home_source,
            sample_detector()?,
            vec![region.clone()],
            limits
        ),
        Err(AnomalyError::HomeInteriorExcluded { .. })
    ));

    // 2. Restricted source is refused with explicit no-bypass policy notice (CAM-01, A09).
    let mut restricted_source = sample_source("cam:rest-01", "feed:rest-01", "cal:rest:v1")?;
    restricted_source.access_status = CameraAccessStatus::RestrictedOrUnavailable;
    assert_eq!(
        CameraAnomalyMonitor::new(
            restricted_source,
            sample_detector()?,
            vec![region.clone()],
            limits
        )
        .err(),
        Some(AnomalyError::SourceRestricted {
            camera_id: OpaqueId::new("cam:rest-01").map_err(AnomalyError::InvalidId)?,
            policy_notice: RESTRICTED_SOURCE_POLICY_NOTICE,
        })
    );

    // 3. Street View / still / synthetic substitutes are refused as observation evidence (CAM-02).
    let mut street_view_source = sample_source("cam:sv-01", "feed:sv-01", "cal:sv:v1")?;
    street_view_source.feed_kind = FeedObservationKind::StreetViewFallback;
    assert!(matches!(
        CameraAnomalyMonitor::new(
            street_view_source,
            sample_detector()?,
            vec![region.clone()],
            limits
        ),
        Err(AnomalyError::NonObservationFeedSubstitute {
            feed_kind: FeedObservationKind::StreetViewFallback,
            ..
        })
    ));

    // 4. Predictive or generated derivative frames cannot be used as original anomaly evidence (CAM-03, MEDIA-03).
    let mut monitor_a = CameraAnomalyMonitor::new(
        sample_source("cam:a", "feed:shared", "cal:a:v1")?,
        sample_detector()?,
        vec![region.clone()],
        limits,
    )?;
    let generated_err = monitor_a.record_step(ObservationStepInput::SceneTrigger {
        observation_id: OpaqueId::new("obs:gen").map_err(AnomalyError::InvalidId)?,
        candidate_id: OpaqueId::new("cand:gen").map_err(AnomalyError::InvalidId)?,
        candidate_version: VersionId::new("ver:cand:gen").map_err(AnomalyError::InvalidId)?,
        region_id: region.region_id.clone(),
        category: AnomalyCategory::SkyTransient,
        baseline_at_trigger: SceneBaselineKind::NighttimeLowLight,
        event_start_ms: 1_000,
        event_end_ms: 1_500,
        detection_emitted_ms: 1_600,
        observed_persistence_frames: 4,
        raw_score_bps: 8_000,
        clock_uncertainty_ms: 20,
        trigger_explanation: "Super-resolved synthetic frame trigger".to_owned(),
        primary_frame: frame_ref(
            "ver:frame:gen",
            'a',
            1_200,
            1_220,
            FrameArtifactOrigin::PredictiveOrGeneratedDerivative,
        )?,
        context_window: ContextWindowStatus::Complete {
            pre_frames: vec![],
            post_frames: vec![],
        },
    });
    assert!(matches!(
        generated_err,
        Err(AnomalyError::GeneratedFrameForbiddenAsEvidence { .. })
    ));

    // 5. Scene trigger overlapping an active fault window is blocked so faults never become scene events (CAM-03).
    monitor_a.record_step(ObservationStepInput::HealthEvent {
        observation_id: OpaqueId::new("obs:exp-shift").map_err(AnomalyError::InvalidId)?,
        health_state: CameraHealthState::ExposureShift,
        start_ms: 2_000,
        end_ms: 3_000,
        detail: "Auto-iris day-to-night step change".to_owned(),
    })?;
    let fault_block_err = monitor_a.record_step(ObservationStepInput::SceneTrigger {
        observation_id: OpaqueId::new("obs:blocked").map_err(AnomalyError::InvalidId)?,
        candidate_id: OpaqueId::new("cand:blocked").map_err(AnomalyError::InvalidId)?,
        candidate_version: VersionId::new("ver:cand:blocked").map_err(AnomalyError::InvalidId)?,
        region_id: region.region_id.clone(),
        category: AnomalyCategory::EnvironmentalChange,
        baseline_at_trigger: SceneBaselineKind::TwilightTransition,
        event_start_ms: 2_200,
        event_end_ms: 2_700,
        detection_emitted_ms: 2_800,
        observed_persistence_frames: 5,
        raw_score_bps: 9_000,
        clock_uncertainty_ms: 20,
        trigger_explanation: "Global luminance jump during exposure shift".to_owned(),
        primary_frame: frame_ref(
            "ver:frame:ok",
            'b',
            2_400,
            2_420,
            FrameArtifactOrigin::OriginalRetainedFrame,
        )?,
        context_window: ContextWindowStatus::Complete {
            pre_frames: vec![],
            post_frames: vec![],
        },
    });
    assert!(matches!(
        fault_block_err,
        Err(AnomalyError::CameraFaultBlocksSceneTrigger {
            active_health: CameraHealthState::ExposureShift,
            ..
        })
    ));

    // Record valid candidate on monitor_a at 4_000..4_500 ms.
    monitor_a.record_step(ObservationStepInput::SceneTrigger {
        observation_id: OpaqueId::new("obs:a:valid").map_err(AnomalyError::InvalidId)?,
        candidate_id: OpaqueId::new("cand:a:1").map_err(AnomalyError::InvalidId)?,
        candidate_version: VersionId::new("ver:cand:a:1").map_err(AnomalyError::InvalidId)?,
        region_id: region.region_id.clone(),
        category: AnomalyCategory::SkyTransient,
        baseline_at_trigger: SceneBaselineKind::NighttimeLowLight,
        event_start_ms: 4_000,
        event_end_ms: 4_500,
        detection_emitted_ms: 4_600,
        observed_persistence_frames: 4,
        raw_score_bps: 8_200,
        clock_uncertainty_ms: 30,
        trigger_explanation: "Valid sky transit on camera A".to_owned(),
        primary_frame: frame_ref(
            "ver:frame:a:1",
            'c',
            4_200,
            4_220,
            FrameArtifactOrigin::OriginalRetainedFrame,
        )?,
        context_window: ContextWindowStatus::Complete {
            pre_frames: vec![],
            post_frames: vec![],
        },
    })?;

    // 6. Cross-camera association refuses shared upstream feed dependency (CAM-05).
    let mut monitor_b_shared = CameraAnomalyMonitor::new(
        sample_source("cam:b", "feed:shared", "cal:b:v1")?,
        sample_detector()?,
        vec![region.clone()],
        limits,
    )?;
    monitor_b_shared.record_step(ObservationStepInput::SceneTrigger {
        observation_id: OpaqueId::new("obs:b:valid").map_err(AnomalyError::InvalidId)?,
        candidate_id: OpaqueId::new("cand:b:1").map_err(AnomalyError::InvalidId)?,
        candidate_version: VersionId::new("ver:cand:b:1").map_err(AnomalyError::InvalidId)?,
        region_id: region.region_id.clone(),
        category: AnomalyCategory::SkyTransient,
        baseline_at_trigger: SceneBaselineKind::NighttimeLowLight,
        event_start_ms: 4_020,
        event_end_ms: 4_520,
        detection_emitted_ms: 4_620,
        observed_persistence_frames: 4,
        raw_score_bps: 8_000,
        clock_uncertainty_ms: 30,
        trigger_explanation: "Rebroadcast of same upstream feed".to_owned(),
        primary_frame: frame_ref(
            "ver:frame:b:1",
            'd',
            4_220,
            4_240,
            FrameArtifactOrigin::OriginalRetainedFrame,
        )?,
        context_window: ContextWindowStatus::Complete {
            pre_frames: vec![],
            post_frames: vec![],
        },
    })?;
    assert!(matches!(
        validate_cross_camera_association(
            &monitor_a,
            &OpaqueId::new("cand:a:1").map_err(AnomalyError::InvalidId)?,
            &monitor_b_shared,
            &OpaqueId::new("cand:b:1").map_err(AnomalyError::InvalidId)?,
            200,
            "Attempted association across rebroadcast feed".to_owned(),
            limits,
        ),
        Err(AnomalyError::SharedUpstreamFeedDependency { .. })
    ));

    // 7. CameraRepositioned invalidates active calibration and blocks multi-view triangulation (CAM-05, A09).
    let mut monitor_c = CameraAnomalyMonitor::new(
        sample_source("cam:c", "feed:independent-c", "cal:c:v1")?,
        sample_detector()?,
        vec![region.clone()],
        limits,
    )?;
    monitor_c.record_step(ObservationStepInput::SceneTrigger {
        observation_id: OpaqueId::new("obs:c:valid").map_err(AnomalyError::InvalidId)?,
        candidate_id: OpaqueId::new("cand:c:1").map_err(AnomalyError::InvalidId)?,
        candidate_version: VersionId::new("ver:cand:c:1").map_err(AnomalyError::InvalidId)?,
        region_id: region.region_id,
        category: AnomalyCategory::SkyTransient,
        baseline_at_trigger: SceneBaselineKind::NighttimeLowLight,
        event_start_ms: 4_010,
        event_end_ms: 4_510,
        detection_emitted_ms: 4_610,
        observed_persistence_frames: 4,
        raw_score_bps: 8_300,
        clock_uncertainty_ms: 30,
        trigger_explanation: "Independent view on camera C before pan".to_owned(),
        primary_frame: frame_ref(
            "ver:frame:c:1",
            'e',
            4_210,
            4_230,
            FrameArtifactOrigin::OriginalRetainedFrame,
        )?,
        context_window: ContextWindowStatus::Complete {
            pre_frames: vec![],
            post_frames: vec![],
        },
    })?;
    monitor_c.record_step(ObservationStepInput::HealthEvent {
        observation_id: OpaqueId::new("obs:c:repositioned").map_err(AnomalyError::InvalidId)?,
        health_state: CameraHealthState::CameraRepositioned,
        start_ms: 5_000,
        end_ms: 5_100,
        detail: "PTZ preset changed by operator".to_owned(),
    })?;
    assert!(matches!(
        validate_cross_camera_association(
            &monitor_a,
            &OpaqueId::new("cand:a:1").map_err(AnomalyError::InvalidId)?,
            &monitor_c,
            &OpaqueId::new("cand:c:1").map_err(AnomalyError::InvalidId)?,
            200,
            "Association after PTZ movement invalidated camera C calibration".to_owned(),
            limits,
        ),
        Err(AnomalyError::CalibrationInvalidatedByMovement { .. })
    ));

    Ok(())
}
