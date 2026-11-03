//! Deterministic synthetic demonstration of camera-anomaly candidate packaging,
//! fault separation, deduplication with version progression, append-only human
//! review history, cross-camera association validation, and held-out operational
//! benchmark evaluation (`CAM-01`–`CAM-05`, `A09`).

use oia_domain::{ActorId, OpaqueId, VersionId};
use oia_media::camera_anomaly::{
    AnomalyCategory, AnomalyError, AnomalyLimits, AnomalyReviewDisposition, AnomalyReviewInput,
    CameraAccessStatus, CameraAnomalyMonitor, CameraCalibrationState, CameraHealthState,
    CameraSourceProfile, CameraViewKind, ContextWindowStatus, DetectionRegionConfig,
    DetectorRunMetadata, FeedObservationKind, FrameArtifactOrigin, HealthEventInput,
    LabeledSceneEvent, ObservationStepInput, RetainedFrameRef, SceneBaselineKind,
    SceneTriggerInput, SourceUsePermission, validate_cross_camera_association,
};

fn build_demo_frame(
    version: &str,
    digit: char,
    capture_time_ms: i64,
    ingestion_time_ms: i64,
) -> Result<RetainedFrameRef, AnomalyError> {
    Ok(RetainedFrameRef {
        frame_version: VersionId::new(version).map_err(AnomalyError::InvalidId)?,
        sha256_hex: digit.to_string().repeat(64),
        capture_time_ms,
        ingestion_time_ms,
        origin: FrameArtifactOrigin::OriginalRetainedFrame,
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let limits = AnomalyLimits::new(8, 64, 32, 8, 8, 16, 16, 16_384)?;

    let north_source = CameraSourceProfile {
        camera_id: OpaqueId::new("cam:mesa-north-sky-01")?,
        upstream_feed_id: OpaqueId::new("feed:mesa-observatory-north")?,
        provider_attribution: "Mesa Public Sky Observatory Directory (simulated fixture)"
            .to_owned(),
        listing_reference: "https://example.invalid/cameras/mesa-north-sky-01".to_owned(),
        access_status: CameraAccessStatus::OrdinarilyAccessible,
        view_kind: CameraViewKind::SkyOrObservatory,
        feed_kind: FeedObservationKind::UpstreamCameraFeed,
        automated_processing: SourceUsePermission::Permitted,
        clip_retention: SourceUsePermission::Permitted,
        redistribution: SourceUsePermission::Permitted,
        max_retention_window_ms: Some(30_000),
        calibration: CameraCalibrationState::Valid {
            calibration_version: VersionId::new("cal:mesa-north:v1")?,
            max_angular_error_mdeg: 120,
        },
    };

    let detector = DetectorRunMetadata {
        detector_id: OpaqueId::new("det:temporal-diff-sky")?,
        detector_version: VersionId::new("detver:temporal-diff-sky:v1")?,
        method_description:
            "Adaptive background differencing with persistence gate (simulated fixture)".to_owned(),
        config_digest_sha256: "a".repeat(64),
    };

    let sky_region = DetectionRegionConfig {
        region_id: OpaqueId::new("reg:western-horizon")?,
        label: "Western Horizon 15-65 deg elevation (rooftop masked)".to_owned(),
        baseline: SceneBaselineKind::NighttimeLowLight,
        score_threshold_bps: 7_000,
        min_persistence_frames: 3,
        min_persistence_ms: 400,
        deduplication_window_ms: 5_000,
        requires_privacy_mask: true,
        privacy_mask_applied: true,
    };

    let mut north_monitor = CameraAnomalyMonitor::new(
        north_source,
        detector.clone(),
        vec![sky_region.clone()],
        limits,
    )?;

    // 1. First sky transient candidate at t = 10_000..10_800 ms.
    north_monitor.record_step(ObservationStepInput::SceneTrigger(Box::new(
        SceneTriggerInput {
            observation_id: OpaqueId::new("obs:north:001")?,
            candidate_id: OpaqueId::new("cand:north:001")?,
            candidate_version: VersionId::new("ver:cand:north:001:v1")?,
            region_id: sky_region.region_id.clone(),
            category: AnomalyCategory::SkyTransient,
            baseline_at_trigger: SceneBaselineKind::NighttimeLowLight,
            event_start_ms: 10_000,
            event_end_ms: 10_800,
            detection_emitted_ms: 10_950,
            observed_persistence_frames: 5,
            raw_score_bps: 8_450,
            clock_uncertainty_ms: 40,
            trigger_explanation:
                "Localized luminous streak moving 4.2 deg/s across western horizon mask".to_owned(),
            primary_frame: build_demo_frame("ver:frame:north:10400", '1', 10_400, 10_450)?,
            context_window: ContextWindowStatus::Complete {
                pre_frames: vec![build_demo_frame(
                    "ver:frame:north:09500",
                    '2',
                    9_500,
                    9_550,
                )?],
                post_frames: vec![build_demo_frame(
                    "ver:frame:north:11500",
                    '3',
                    11_500,
                    11_550,
                )?],
            },
        },
    )))?;

    // 2. Overlapping/follow-up trigger at 10_500..13_600 ms deduplicates into cand:north:001
    // and advances its version to ver:cand:north:001:v2 while retaining all frames.
    north_monitor.record_step(ObservationStepInput::SceneTrigger(Box::new(
        SceneTriggerInput {
            observation_id: OpaqueId::new("obs:north:002")?,
            candidate_id: OpaqueId::new("cand:north:001-dup")?,
            candidate_version: VersionId::new("ver:cand:north:001:v2")?,
            region_id: sky_region.region_id.clone(),
            category: AnomalyCategory::SkyTransient,
            baseline_at_trigger: SceneBaselineKind::NighttimeLowLight,
            event_start_ms: 10_500,
            event_end_ms: 13_600,
            detection_emitted_ms: 13_720,
            observed_persistence_frames: 4,
            raw_score_bps: 8_900,
            clock_uncertainty_ms: 45,
            trigger_explanation: "Continued western horizon luminous track segment".to_owned(),
            primary_frame: build_demo_frame("ver:frame:north:13200", '4', 13_200, 13_250)?,
            context_window: ContextWindowStatus::PartialBufferExpired {
                pre_frames: vec![],
                post_frames: vec![],
                expired_frame_count: 1,
            },
        },
    )))?;

    // 3. Camera frozen-frame fault at 600_000..900_000 ms (5 minutes of coverage loss, not a scene alert).
    north_monitor.record_step(ObservationStepInput::HealthEvent(HealthEventInput {
        observation_id: OpaqueId::new("obs:north:fault-001")?,
        health_state: CameraHealthState::FrozenFrame,
        start_ms: 600_000,
        end_ms: 900_000,
        detail: "Upstream encoder repeated identical frame digest for 300 seconds".to_owned(),
    }))?;

    // 4. Unmatched false alert at 1_200_000..1_200_600 ms (headlight reflection on dome).
    north_monitor.record_step(ObservationStepInput::SceneTrigger(Box::new(
        SceneTriggerInput {
            observation_id: OpaqueId::new("obs:north:003")?,
            candidate_id: OpaqueId::new("cand:north:002")?,
            candidate_version: VersionId::new("ver:cand:north:002:v1")?,
            region_id: sky_region.region_id.clone(),
            category: AnomalyCategory::SkyTransient,
            baseline_at_trigger: SceneBaselineKind::NighttimeLowLight,
            event_start_ms: 1_200_000,
            event_end_ms: 1_200_600,
            detection_emitted_ms: 1_200_750,
            observed_persistence_frames: 3,
            raw_score_bps: 7_250,
            clock_uncertainty_ms: 40,
            trigger_explanation: "Dome glare flash near lower-left boundary".to_owned(),
            primary_frame: build_demo_frame("ver:frame:north:1200300", '5', 1_200_300, 1_200_340)?,
            context_window: ContextWindowStatus::OmittedByRetentionPolicy {
                reason: "Buffer rotation occurred before manual pin".to_owned(),
            },
        },
    )))?;

    // Append two human reviews to cand:north:001 citing frames from both merged trigger segments.
    north_monitor.review_candidate(AnomalyReviewInput {
        candidate_id: OpaqueId::new("cand:north:001")?,
        expected_current_version: VersionId::new("ver:cand:north:001:v2")?,
        new_review_version: VersionId::new("ver:cand:north:001:rev1")?,
        reviewer: ActorId::new("actor:analyst-01")?,
        reviewed_at_ms: 20_000,
        disposition: AnomalyReviewDisposition::InsufficientEvidence,
        rationale: "Single-camera angular track; awaiting second independent viewpoint".to_owned(),
        cited_frame_versions: vec![VersionId::new("ver:frame:north:10400")?],
    })?;

    north_monitor.review_candidate(AnomalyReviewInput {
        candidate_id: OpaqueId::new("cand:north:001")?,
        expected_current_version: VersionId::new("ver:cand:north:001:rev1")?,
        new_review_version: VersionId::new("ver:cand:north:001:rev2")?,
        reviewer: ActorId::new("actor:reviewer-02")?,
        reviewed_at_ms: 25_000,
        disposition: AnomalyReviewDisposition::RetainedForInvestigation,
        rationale: "Temporally aligned with independent South Ridge camera candidate; retained for geometric check (timing coincidence alone does not establish identity)".to_owned(),
        cited_frame_versions: vec![
            VersionId::new("ver:frame:north:10400")?,
            VersionId::new("ver:frame:north:13200")?,
        ],
    })?;

    // Set up independent second camera to demonstrate cross-camera association eligibility.
    let south_source = CameraSourceProfile {
        camera_id: OpaqueId::new("cam:south-ridge-sky-02")?,
        upstream_feed_id: OpaqueId::new("feed:south-ridge-observatory")?,
        provider_attribution: "South Ridge Amateur Sky Watch (simulated fixture)".to_owned(),
        listing_reference: "https://example.invalid/cameras/south-ridge-sky-02".to_owned(),
        access_status: CameraAccessStatus::OrdinarilyAccessible,
        view_kind: CameraViewKind::SkyOrObservatory,
        feed_kind: FeedObservationKind::UpstreamCameraFeed,
        automated_processing: SourceUsePermission::Permitted,
        clip_retention: SourceUsePermission::Permitted,
        redistribution: SourceUsePermission::Prohibited,
        max_retention_window_ms: Some(30_000),
        calibration: CameraCalibrationState::Valid {
            calibration_version: VersionId::new("cal:south-ridge:v1")?,
            max_angular_error_mdeg: 150,
        },
    };
    let mut south_monitor =
        CameraAnomalyMonitor::new(south_source, detector, vec![sky_region.clone()], limits)?;
    south_monitor.record_step(ObservationStepInput::SceneTrigger(Box::new(
        SceneTriggerInput {
            observation_id: OpaqueId::new("obs:south:001")?,
            candidate_id: OpaqueId::new("cand:south:001")?,
            candidate_version: VersionId::new("ver:cand:south:001:v1")?,
            region_id: sky_region.region_id.clone(),
            category: AnomalyCategory::SkyTransient,
            baseline_at_trigger: SceneBaselineKind::NighttimeLowLight,
            event_start_ms: 10_050,
            event_end_ms: 13_550,
            detection_emitted_ms: 10_980,
            observed_persistence_frames: 8,
            raw_score_bps: 8_700,
            clock_uncertainty_ms: 55,
            trigger_explanation: "Simultaneous northwest azimuth luminous streak".to_owned(),
            primary_frame: build_demo_frame("ver:frame:south:10500", '6', 10_500, 10_560)?,
            context_window: ContextWindowStatus::Complete {
                pre_frames: vec![build_demo_frame(
                    "ver:frame:south:10000",
                    '7',
                    10_000,
                    10_040,
                )?],
                post_frames: vec![],
            },
        },
    )))?;

    let association = validate_cross_camera_association(
        &north_monitor,
        &OpaqueId::new("cand:north:001")?,
        &south_monitor,
        &OpaqueId::new("cand:south:001")?,
        true,
        250,
        "Overlapping NTP-synchronized capture intervals from independent observatories (simulated fixture)".to_owned(),
        limits,
    )?;

    let labeled_truth = vec![LabeledSceneEvent {
        truth_id: OpaqueId::new("truth:meteor-bolide-01")?,
        region_id: sky_region.region_id,
        category: AnomalyCategory::SkyTransient,
        start_ms: 10_000,
        end_ms: 13_600,
    }];
    let eval = north_monitor.evaluate_held_out(
        OpaqueId::new("corpus:a09-simulated-sky")?,
        VersionId::new("ver:corpus:a09-simulated-sky:v1")?,
        0,
        3_600_000,
        &labeled_truth,
    )?;

    println!(
        "[SIMULATED FIXTURE] Camera: {} | Corpus: {}@{}",
        eval.camera_id, eval.evaluation_corpus_id, eval.evaluation_corpus_version
    );
    println!(
        "[SIMULATED FIXTURE] Coverage: {} bps (active {} ms, missing {} ms, health events overlapping window: {})",
        eval.coverage_bps,
        eval.active_coverage_ms,
        eval.missing_coverage_ms,
        eval.health_events_overlapping_window
    );
    println!(
        "[SIMULATED FIXTURE] Candidates: {} total (true positive: {}, false alerts: {}, deduplicated repeats in window: {})",
        eval.total_candidate_alerts,
        eval.true_positive_alerts,
        eval.false_alert_count,
        eval.deduplicated_repeat_triggers_in_window
    );
    println!(
        "[SIMULATED FIXTURE] Held-out recall (active coverage): {:?} bps | False alerts/camera-hour (milli): {} | Mean latency: {:?} ms",
        eval.event_recall_bps,
        eval.false_alerts_per_camera_hour_milli,
        eval.mean_detection_latency_ms
    );
    if let Some(first) = north_monitor.candidates().first() {
        println!(
            "[SIMULATED FIXTURE] Candidate {} segments: {}, reviews retained: {} (current disposition: {:?}, calibrated probability: {})",
            first.candidate_id(),
            first.trigger_segments().len(),
            first.reviews().len(),
            first.current_disposition(),
            first.is_score_calibrated_probability()
        );
    }
    println!(
        "[SIMULATED FIXTURE] Cross-camera association {} <-> {} eligible for triangulation: {}",
        association.first_camera_id(),
        association.second_camera_id(),
        association.eligible_for_triangulation()
    );

    Ok(())
}
