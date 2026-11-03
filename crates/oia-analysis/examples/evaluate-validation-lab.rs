//! Deterministic example exercising the validation and simulation lab kernel (`#40`,
//! `FIELD-04`, `CAM-05`, `MEDIA-02`, `DEVICE-04`, `docs/delivery.md` §4).
//!
//! Run from the repository root:
//! ```text
//! cargo +1.99.0 run -p oia-analysis --example evaluate-validation-lab --locked --offline
//! ```

#![deny(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use std::error::Error;

use oia_analysis::validation_lab::{
    BackpressureAndCoverageInput, BenchmarkCacheState, BenchmarkEnvironmentManifest,
    CalibrationReceipt, CancellationAndRecoveryInput, DatasetManifestInput, DatasetRecord,
    DatasetRecordOrigin, DatasetSplit, GroundTruthLabel, HeldOutEvaluationInput,
    HeldOutItemEvaluation, HeldOutItemOutcome, HeldOutSkipReason, OperationalBenchmarkInput,
    PerturbationSpec, ReferenceTargetProfile, SIMULATION_LABEL, SystemDecision,
    TelemetryAndEditResponsiveness, ValidationDomain, ValidationLabLimits,
    VisualQualityParityInput, evaluate_held_out_run, evaluate_operational_benchmark,
    validate_dataset_manifest,
};
use oia_domain::{OpaqueId, SchemaVersion, VersionId};

fn main() -> Result<(), Box<dyn Error>> {
    let limits = ValidationLabLimits::default();

    let manifest_input = DatasetManifestInput {
        dataset_id: OpaqueId::new("dataset:oia-validation-lab-v1")?,
        dataset_version: VersionId::new("ver:dataset-v1-20261103")?,
        schema_version: SchemaVersion::V1,
        description: String::from(
            "[SYNTHETIC FIXTURE] Multi-domain validation lab manifest separating labeled reference fixtures from controlled perturbations",
        ),
        records: vec![
            DatasetRecord {
                record_id: OpaqueId::new("rec:train-real-cam-01")?,
                record_version: VersionId::new("ver:train-real-cam-01")?,
                split: DatasetSplit::Train,
                domain: ValidationDomain::CameraAnomalyAndHealth,
                payload_sha256: "11".repeat(32),
                observed_duration_ms: 1_800_000,
                expected_label: GroundTruthLabel::PositiveTargetPresent,
                origin: DatasetRecordOrigin::LabeledRealFixture {
                    source_id: OpaqueId::new("src:ridge-cam-north")?,
                    source_version: VersionId::new("ver:src-ridge-cam-01")?,
                    calibration_version: Some(VersionId::new("ver:cal-ridge-01")?),
                    capture_time_uncertainty_ms: 25,
                    ground_truth_basis: String::from("Surveyed calibration beacon transit"),
                },
                promoted_to_observation_evidence: true,
            },
            DatasetRecord {
                record_id: OpaqueId::new("rec:cal-real-cam-01")?,
                record_version: VersionId::new("ver:cal-real-cam-01")?,
                split: DatasetSplit::CalibrationOrTuning,
                domain: ValidationDomain::CameraAnomalyAndHealth,
                payload_sha256: "22".repeat(32),
                observed_duration_ms: 1_800_000,
                expected_label: GroundTruthLabel::NegativeOrdinaryOrFault,
                origin: DatasetRecordOrigin::LabeledRealFixture {
                    source_id: OpaqueId::new("src:ridge-cam-north")?,
                    source_version: VersionId::new("ver:src-ridge-cam-02")?,
                    calibration_version: Some(VersionId::new("ver:cal-ridge-01")?),
                    capture_time_uncertainty_ms: 25,
                    ground_truth_basis: String::from("Verified clear-sky calibration interval"),
                },
                promoted_to_observation_evidence: true,
            },
            DatasetRecord {
                record_id: OpaqueId::new("rec:heldout-real-cam-pos")?,
                record_version: VersionId::new("ver:heldout-real-cam-pos")?,
                split: DatasetSplit::HeldOutEvaluation,
                domain: ValidationDomain::CameraAnomalyAndHealth,
                payload_sha256: "33".repeat(32),
                observed_duration_ms: 3_600_000,
                expected_label: GroundTruthLabel::PositiveTargetPresent,
                origin: DatasetRecordOrigin::LabeledRealFixture {
                    source_id: OpaqueId::new("src:valley-cam-east")?,
                    source_version: VersionId::new("ver:src-valley-cam-01")?,
                    calibration_version: Some(VersionId::new("ver:cal-valley-01")?),
                    capture_time_uncertainty_ms: 40,
                    ground_truth_basis: String::from(
                        "Logged weather-balloon ascent with independent radar track",
                    ),
                },
                promoted_to_observation_evidence: true,
            },
            DatasetRecord {
                record_id: OpaqueId::new("rec:heldout-real-uav-neg")?,
                record_version: VersionId::new("ver:heldout-real-uav-neg")?,
                split: DatasetSplit::HeldOutEvaluation,
                domain: ValidationDomain::UavVideoAndTelemetry,
                payload_sha256: "44".repeat(32),
                observed_duration_ms: 3_600_000,
                expected_label: GroundTruthLabel::NegativeOrdinaryOrFault,
                origin: DatasetRecordOrigin::LabeledRealFixture {
                    source_id: OpaqueId::new("src:uav-sortie-04")?,
                    source_version: VersionId::new("ver:src-uav-sortie-04")?,
                    calibration_version: Some(VersionId::new("ver:cal-uav-04")?),
                    capture_time_uncertainty_ms: 15,
                    ground_truth_basis: String::from("Benign gimbal sweep with no target present"),
                },
                promoted_to_observation_evidence: true,
            },
            DatasetRecord {
                record_id: OpaqueId::new("rec:heldout-sim-perturbed-uav")?,
                record_version: VersionId::new("ver:heldout-sim-perturbed-uav")?,
                split: DatasetSplit::HeldOutEvaluation,
                domain: ValidationDomain::UavVideoAndTelemetry,
                payload_sha256: "55".repeat(32),
                observed_duration_ms: 3_600_000,
                expected_label: GroundTruthLabel::PositiveTargetPresent,
                origin: DatasetRecordOrigin::ControlledSimulation {
                    generator_id: OpaqueId::new("gen:oia-perturbation-synth-v1")?,
                    generator_version: VersionId::new("ver:gen-synth-v1")?,
                    base_real_fixture_version: Some(VersionId::new("ver:heldout-real-cam-pos")?),
                    seed: 42,
                    perturbations: vec![
                        PerturbationSpec::SensorNoise {
                            sigma_norm_milli: 35,
                            seed: 101,
                        },
                        PerturbationSpec::MotionOrDefocusBlur {
                            kernel_radius_px_milli: 2_500,
                            motion_angle_deg_milli: 45_000,
                        },
                        PerturbationSpec::CameraMotionAndParallax {
                            vibration_rms_mrad: 12,
                            gimbal_rate_mrad_per_s: 150,
                            parallax_baseline_mm: 45,
                            exposure_shift_ev_milli: -500,
                        },
                        PerturbationSpec::ClockOffsetAndDrift {
                            initial_offset_ms: -120,
                            drift_ppm: 18,
                            stated_uncertainty_ms: 60,
                        },
                    ],
                    visible_simulation_label: String::from(SIMULATION_LABEL),
                    difference_from_observation: String::from(
                        "Injected synthetic Gaussian sensor noise, 2.5px motion blur, gimbal vibration, and -120ms/+18ppm clock drift over base fixture",
                    ),
                },
                promoted_to_observation_evidence: false,
            },
            DatasetRecord {
                record_id: OpaqueId::new("rec:heldout-sim-degenerate-triangulation")?,
                record_version: VersionId::new("ver:heldout-sim-degenerate-triangulation")?,
                split: DatasetSplit::HeldOutEvaluation,
                domain: ValidationDomain::FieldworkAndTriangulation,
                payload_sha256: "66".repeat(32),
                observed_duration_ms: 600_000,
                expected_label: GroundTruthLabel::DegenerateRequiresInsufficientEvidence,
                origin: DatasetRecordOrigin::ControlledSimulation {
                    generator_id: OpaqueId::new("gen:oia-geometry-synth-v1")?,
                    generator_version: VersionId::new("ver:gen-geometry-v1")?,
                    base_real_fixture_version: None,
                    seed: 99,
                    perturbations: vec![PerturbationSpec::DegenerateSightlineGeometry {
                        crossing_angle_mrad: 4,
                    }],
                    visible_simulation_label: String::from(SIMULATION_LABEL),
                    difference_from_observation: String::from(
                        "Synthetic two-observer geometry with 4 mrad (~0.23 deg) nearly parallel rays that cannot uniquely resolve range",
                    ),
                },
                promoted_to_observation_evidence: false,
            },
        ],
    };

    let validated_manifest = validate_dataset_manifest(manifest_input, &limits, || Ok(()))?;

    let held_out_report = evaluate_held_out_run(
        &validated_manifest,
        HeldOutEvaluationInput {
            evaluation_run_id: OpaqueId::new("eval:heldout-run-01")?,
            evaluation_run_version: VersionId::new("ver:heldout-run-01")?,
            detector_or_solver_id: OpaqueId::new("solver:oia-combined-v1")?,
            detector_or_solver_version: VersionId::new("ver:solver-v1")?,
            claims_calibrated_probabilities: false,
            calibration_receipt: Some(CalibrationReceipt {
                calibration_run_version: VersionId::new("ver:cal-run-01")?,
                calibration_split_dataset_version: VersionId::new("ver:dataset-v1-20261103")?,
                detector_or_solver_id: OpaqueId::new("solver:oia-combined-v1")?,
                detector_or_solver_version: VersionId::new("ver:solver-v1")?,
                calibrated_domain: ValidationDomain::CameraAnomalyAndHealth,
                score_mapping_version: VersionId::new("ver:score-map-cam-v1")?,
                evaluated_calibration_record_versions: vec![VersionId::new("ver:cal-real-cam-01")?],
                empirical_brier_score_milli: 48,
            }),
            items: vec![
                HeldOutItemEvaluation {
                    record_id: OpaqueId::new("rec:heldout-real-cam-pos")?,
                    record_version: VersionId::new("ver:heldout-real-cam-pos")?,
                    outcome: HeldOutItemOutcome::Evaluated {
                        decision: SystemDecision::PositiveCandidateOrReconstruction,
                        raw_score_bps: Some(8_900),
                        processing_latency_ms: 42,
                        used_predictive_or_generated_detail_as_measurement: false,
                        output_retained_simulation_label: false,
                    },
                },
                HeldOutItemEvaluation {
                    record_id: OpaqueId::new("rec:heldout-real-uav-neg")?,
                    record_version: VersionId::new("ver:heldout-real-uav-neg")?,
                    outcome: HeldOutItemOutcome::Skipped {
                        reason: HeldOutSkipReason::CoverageGapOrDroppedStream {
                            detail: String::from(
                                "Simulated telemetry link drop covered the evaluation window",
                            ),
                        },
                    },
                },
                HeldOutItemEvaluation {
                    record_id: OpaqueId::new("rec:heldout-sim-perturbed-uav")?,
                    record_version: VersionId::new("ver:heldout-sim-perturbed-uav")?,
                    outcome: HeldOutItemOutcome::Evaluated {
                        decision: SystemDecision::PositiveCandidateOrReconstruction,
                        raw_score_bps: None,
                        processing_latency_ms: 68,
                        used_predictive_or_generated_detail_as_measurement: false,
                        output_retained_simulation_label: true,
                    },
                },
                HeldOutItemEvaluation {
                    record_id: OpaqueId::new("rec:heldout-sim-degenerate-triangulation")?,
                    record_version: VersionId::new("ver:heldout-sim-degenerate-triangulation")?,
                    outcome: HeldOutItemOutcome::Evaluated {
                        decision: SystemDecision::AbstainedInsufficientEvidence,
                        raw_score_bps: None,
                        processing_latency_ms: 5,
                        used_predictive_or_generated_detail_as_measurement: false,
                        output_retained_simulation_label: true,
                    },
                },
            ],
        },
        &limits,
        || Ok(()),
    )?;

    let benchmark_report = evaluate_operational_benchmark(
        OperationalBenchmarkInput {
            environment: BenchmarkEnvironmentManifest {
                benchmark_id: OpaqueId::new("bench:synthetic-lab-profile-01")?,
                run_version: VersionId::new("ver:bench-run-01")?,
                git_revision_hex: String::from("4f68d8243687ea30afa7b4d69345af040fcd6378"),
                dataset_manifest_version: validated_manifest.dataset_version().clone(),
                dataset_sha256: "aa".repeat(32),
                hardware_profile: String::from(
                    "[SYNTHETIC FIXTURE] 8-core reference profile, 16 GB RAM, NVMe SSD",
                ),
                os_description: String::from("Windows 11 x86_64 MSVC"),
                renderer_description: String::from("Tauri 2 WebView2 / CesiumJS offline fixture"),
                worker_concurrency: 4,
                cache_state: BenchmarkCacheState::Warm,
            },
            targets: ReferenceTargetProfile::delivery_v0_1_default(),
            ui_acknowledgment_latencies_ms: vec![12, 18, 22, 31, 45, 62, 78],
            local_query_latencies_ms: vec![180, 240, 310, 490, 820],
            external_provider_latencies_ms: vec![450, 920, 1_450],
            model_service_latencies_ms: vec![1_100, 1_850],
            responsiveness: TelemetryAndEditResponsiveness {
                max_telemetry_dispatch_stall_ms: 19,
                dropped_telemetry_packets: 0,
                unhandled_flight_control_commands_emitted: 0,
                case_edit_responsive_under_backpressure: true,
            },
            backpressure: BackpressureAndCoverageInput {
                configured_queue_capacity: 64,
                peak_queue_depth: 64,
                backpressure_activated: true,
                throttled_or_rejected_requests: 2,
                dropped_frames_or_samples: 5,
                reported_coverage_bps: 9_650,
            },
            cancellation: CancellationAndRecoveryInput {
                cancel_acknowledgment_ms: 85,
                worker_resource_release_ms: 420,
                late_completions_observed: 1,
                published_after_cancel_or_supersession: 0,
                interrupted_import_left_partial_case: false,
            },
            visual_parity: VisualQualityParityInput {
                marker_count: 5_000,
                standard_glass_fps: 58,
                reduced_effects_fps: 60,
                standard_glass_numerical_digest_sha256: "bb".repeat(32),
                reduced_effects_numerical_digest_sha256: "bb".repeat(32),
            },
        },
        &limits,
        || Ok(()),
    )?;

    println!(
        "Validated dataset `{}` ({}): train={}/{} real/sim, cal={}/{} real/sim, held_out={}/{} real/sim",
        validated_manifest.dataset_id().as_str(),
        validated_manifest.dataset_version().as_str(),
        validated_manifest
            .train_composition()
            .labeled_real_fixture_count,
        validated_manifest
            .train_composition()
            .controlled_simulation_count,
        validated_manifest
            .calibration_composition()
            .labeled_real_fixture_count,
        validated_manifest
            .calibration_composition()
            .controlled_simulation_count,
        validated_manifest
            .held_out_composition()
            .labeled_real_fixture_count,
        validated_manifest
            .held_out_composition()
            .controlled_simulation_count,
    );
    println!(
        "Held-out real fixtures: evaluated={}/{}, coverage={} bps, recall={:?} bps, false_alerts_per_hr_milli={:?}",
        held_out_report
            .labeled_real_fixture_metrics
            .evaluated_record_count,
        held_out_report
            .labeled_real_fixture_metrics
            .manifest_record_count,
        held_out_report
            .labeled_real_fixture_metrics
            .record_coverage_bps,
        held_out_report
            .labeled_real_fixture_metrics
            .positive_event_recall_bps,
        held_out_report
            .labeled_real_fixture_metrics
            .false_alerts_per_hour_milli,
    );
    println!(
        "Held-out simulations ({}): evaluated={}/{}, coverage={} bps, recall={:?} bps, degenerate_abstentions={}, calibrated_prob={}, receipts={}",
        held_out_report.simulation_label,
        held_out_report
            .controlled_simulation_metrics
            .evaluated_record_count,
        held_out_report
            .controlled_simulation_metrics
            .manifest_record_count,
        held_out_report
            .controlled_simulation_metrics
            .record_coverage_bps,
        held_out_report
            .controlled_simulation_metrics
            .positive_event_recall_bps,
        held_out_report
            .controlled_simulation_metrics
            .correct_degenerate_abstention_count,
        held_out_report.is_calibrated_probability(),
        held_out_report.item_receipts.len(),
    );
    println!(
        "Operational benchmark `{}`: ui_p95={}ms ({:?}), query_p95={}ms ({:?}), provider_p95={:?}ms, model_p95={:?}ms, cancel_ack={}ms ({:?}), coverage={} bps",
        benchmark_report.environment.benchmark_id.as_str(),
        benchmark_report.ui_acknowledgment_latency.p95_ms,
        benchmark_report.ui_ack_target_outcome,
        benchmark_report.local_query_latency.p95_ms,
        benchmark_report.local_query_target_outcome,
        benchmark_report.external_provider_latency.map(|s| s.p95_ms),
        benchmark_report.model_service_latency.map(|s| s.p95_ms),
        benchmark_report.cancellation.cancel_acknowledgment_ms,
        benchmark_report.cancel_ack_target_outcome,
        benchmark_report.backpressure.reported_coverage_bps,
    );

    Ok(())
}
