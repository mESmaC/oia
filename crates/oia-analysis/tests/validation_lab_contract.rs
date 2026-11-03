//! Targeted contract tests for `oia_analysis::validation_lab` (`#40`, `FIELD-04`, `CAM-05`,
//! `MEDIA-02`, `DEVICE-04`, `docs/delivery.md` §4, and deterministic subsets of `A09`, `A10`,
//! `A12`, `A13`).

#![deny(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use oia_analysis::validation_lab::{
    BackpressureAndCoverageInput, BenchmarkCacheState, BenchmarkEnvironmentManifest,
    CalibrationReceipt, CancellationAndRecoveryInput, DatasetManifestInput, DatasetRecord,
    DatasetRecordOrigin, DatasetSplit, GroundTruthLabel, HeldOutEvaluationInput,
    HeldOutItemEvaluation, HeldOutItemOutcome, HeldOutSkipReason,
    MAX_DEGENERATE_CROSSING_ANGLE_MRAD, OperationalBenchmarkInput, PerturbationSpec,
    ReferenceTargetProfile, SIMULATION_LABEL, ScoreCalibrationStatus, StopReason, SystemDecision,
    TargetComparisonOutcome, TelemetryAndEditResponsiveness, ValidationDomain, ValidationLabError,
    ValidationLabLimits, VisualQualityParityInput, evaluate_held_out_run,
    evaluate_operational_benchmark, validate_dataset_manifest,
};
use oia_domain::{IdError, OpaqueId, SchemaVersion, VersionId};

fn sample_manifest() -> Result<DatasetManifestInput, IdError> {
    Ok(DatasetManifestInput {
        dataset_id: OpaqueId::new("dataset:lab-contract")?,
        dataset_version: VersionId::new("ver:dataset-contract-v1")?,
        schema_version: SchemaVersion::V1,
        description: String::from("Validation lab contract fixture"),
        records: vec![
            DatasetRecord {
                record_id: OpaqueId::new("rec:train-real-01")?,
                record_version: VersionId::new("ver:train-real-01")?,
                split: DatasetSplit::Train,
                domain: ValidationDomain::CameraAnomalyAndHealth,
                payload_sha256: "10".repeat(32),
                observed_duration_ms: 1_800_000,
                expected_label: GroundTruthLabel::PositiveTargetPresent,
                origin: DatasetRecordOrigin::LabeledRealFixture {
                    source_id: OpaqueId::new("src:cam-a")?,
                    source_version: VersionId::new("ver:src-cam-a-01")?,
                    calibration_version: Some(VersionId::new("ver:cal-a-01")?),
                    capture_time_uncertainty_ms: 20,
                    ground_truth_basis: String::from("Radar-confirmed aircraft pass"),
                },
                promoted_to_observation_evidence: true,
            },
            DatasetRecord {
                record_id: OpaqueId::new("rec:cal-real-01")?,
                record_version: VersionId::new("ver:cal-real-01")?,
                split: DatasetSplit::CalibrationOrTuning,
                domain: ValidationDomain::CameraAnomalyAndHealth,
                payload_sha256: "20".repeat(32),
                observed_duration_ms: 1_800_000,
                expected_label: GroundTruthLabel::NegativeOrdinaryOrFault,
                origin: DatasetRecordOrigin::LabeledRealFixture {
                    source_id: OpaqueId::new("src:cam-a")?,
                    source_version: VersionId::new("ver:src-cam-a-02")?,
                    calibration_version: Some(VersionId::new("ver:cal-a-01")?),
                    capture_time_uncertainty_ms: 20,
                    ground_truth_basis: String::from("Clear sky calibration interval"),
                },
                promoted_to_observation_evidence: true,
            },
            DatasetRecord {
                record_id: OpaqueId::new("rec:heldout-real-pos")?,
                record_version: VersionId::new("ver:heldout-real-pos")?,
                split: DatasetSplit::HeldOutEvaluation,
                domain: ValidationDomain::MediaForensicsAndGeometry,
                payload_sha256: "30".repeat(32),
                observed_duration_ms: 3_600_000,
                expected_label: GroundTruthLabel::PositiveTargetPresent,
                origin: DatasetRecordOrigin::LabeledRealFixture {
                    source_id: OpaqueId::new("src:cam-b")?,
                    source_version: VersionId::new("ver:src-cam-b-01")?,
                    calibration_version: Some(VersionId::new("ver:cal-b-01")?),
                    capture_time_uncertainty_ms: 30,
                    ground_truth_basis: String::from("Calibrated two-view sounding balloon"),
                },
                promoted_to_observation_evidence: true,
            },
            DatasetRecord {
                record_id: OpaqueId::new("rec:heldout-real-neg")?,
                record_version: VersionId::new("ver:heldout-real-neg")?,
                split: DatasetSplit::HeldOutEvaluation,
                domain: ValidationDomain::CameraAnomalyAndHealth,
                payload_sha256: "40".repeat(32),
                observed_duration_ms: 3_600_000,
                expected_label: GroundTruthLabel::NegativeOrdinaryOrFault,
                origin: DatasetRecordOrigin::LabeledRealFixture {
                    source_id: OpaqueId::new("src:cam-b")?,
                    source_version: VersionId::new("ver:src-cam-b-02")?,
                    calibration_version: None,
                    capture_time_uncertainty_ms: 50,
                    ground_truth_basis: String::from("Sudden exposure shift fault sequence"),
                },
                promoted_to_observation_evidence: true,
            },
            DatasetRecord {
                record_id: OpaqueId::new("rec:heldout-sim-perturbed")?,
                record_version: VersionId::new("ver:heldout-sim-perturbed")?,
                split: DatasetSplit::HeldOutEvaluation,
                domain: ValidationDomain::UavVideoAndTelemetry,
                payload_sha256: "50".repeat(32),
                observed_duration_ms: 1_800_000,
                expected_label: GroundTruthLabel::PositiveTargetPresent,
                origin: DatasetRecordOrigin::ControlledSimulation {
                    generator_id: OpaqueId::new("gen:perturb-v1")?,
                    generator_version: VersionId::new("ver:gen-perturb-v1")?,
                    base_real_fixture_version: Some(VersionId::new("ver:heldout-real-pos")?),
                    seed: 7,
                    perturbations: vec![
                        PerturbationSpec::OpticalGlare {
                            bloom_intensity_bps: 1_200,
                            affected_area_bps: 800,
                        },
                        PerturbationSpec::RollingShutterReadout {
                            line_readout_us: 28,
                            angular_rate_mrad_per_s: -95,
                        },
                        PerturbationSpec::LensAndPerspectiveDistortion {
                            radial_k1_milli: -120,
                            radial_k2_milli: 15,
                            viewing_obliquity_deg_milli: 32_000,
                        },
                        PerturbationSpec::LossyCompression {
                            codec_label: String::from("jpeg-baseline"),
                            quality_step: 75,
                            recompression_generations: 2,
                        },
                        PerturbationSpec::FrameAndTelemetryDrop {
                            dropped_frame_count: 14,
                            disconnected_duration_ms: 450,
                        },
                    ],
                    visible_simulation_label: String::from(SIMULATION_LABEL),
                    difference_from_observation: String::from(
                        "Synthetic glare, rolling shutter, radial lens distortion, 2x JPEG Q75 recompression, and 14 dropped frames",
                    ),
                },
                promoted_to_observation_evidence: false,
            },
            DatasetRecord {
                record_id: OpaqueId::new("rec:heldout-sim-degenerate")?,
                record_version: VersionId::new("ver:heldout-sim-degenerate")?,
                split: DatasetSplit::HeldOutEvaluation,
                domain: ValidationDomain::FieldworkAndTriangulation,
                payload_sha256: "60".repeat(32),
                observed_duration_ms: 1_800_000,
                expected_label: GroundTruthLabel::DegenerateRequiresInsufficientEvidence,
                origin: DatasetRecordOrigin::ControlledSimulation {
                    generator_id: OpaqueId::new("gen:geometry-v1")?,
                    generator_version: VersionId::new("ver:gen-geometry-v1")?,
                    base_real_fixture_version: None,
                    seed: 11,
                    perturbations: vec![PerturbationSpec::DegenerateSightlineGeometry {
                        crossing_angle_mrad: 3,
                    }],
                    visible_simulation_label: String::from(SIMULATION_LABEL),
                    difference_from_observation: String::from(
                        "Near-parallel sightlines (3 mrad) where range cannot be uniquely resolved",
                    ),
                },
                promoted_to_observation_evidence: false,
            },
        ],
    })
}

/// Risk/requirement addressed: verifies that labeled real fixtures and controlled simulations
/// remain strictly separated in dataset manifests and held-out metrics (`FIELD-04`, `MEDIA-02`,
/// `DEVICE-04`, `A10`, `A13`), prevents cross-split payload/source/derivation leakage, and
/// rejects cherry-picked held-out subsets (`#40` completion criterion 1, `A09`).
#[test]
fn validation_lab_separates_real_fixtures_and_simulations_and_prevents_cherry_picking()
-> Result<(), Box<dyn std::error::Error>> {
    let limits = ValidationLabLimits::default();
    let manifest_input = sample_manifest()?;
    let manifest = validate_dataset_manifest(manifest_input.clone(), &limits, || Ok(()))?;

    assert_eq!(
        manifest.held_out_composition().labeled_real_fixture_count,
        2
    );
    assert_eq!(
        manifest.held_out_composition().controlled_simulation_count,
        2
    );
    for rec in manifest.records() {
        if rec.is_simulation() {
            assert!(!rec.can_serve_as_observation_evidence());
        } else {
            assert!(rec.can_serve_as_observation_evidence());
        }
    }

    // Omitting `rec:heldout-real-neg` silently from evaluation must fail as cherry-picking.
    let cherry_picked = HeldOutEvaluationInput {
        evaluation_run_id: OpaqueId::new("eval:cherry-picked")?,
        evaluation_run_version: VersionId::new("ver:eval-cherry-picked")?,
        detector_or_solver_id: OpaqueId::new("solver:test")?,
        detector_or_solver_version: VersionId::new("ver:solver-test")?,
        claims_calibrated_probabilities: false,
        calibration_receipt: None,
        items: vec![HeldOutItemEvaluation {
            record_id: OpaqueId::new("rec:heldout-real-pos")?,
            record_version: VersionId::new("ver:heldout-real-pos")?,
            outcome: HeldOutItemOutcome::Evaluated {
                decision: SystemDecision::PositiveCandidateOrReconstruction,
                raw_score_bps: Some(9_000),
                processing_latency_ms: 25,
                used_predictive_or_generated_detail_as_measurement: false,
                output_retained_simulation_label: false,
            },
        }],
    };
    assert_eq!(
        Err(ValidationLabError::HeldOutRecordSilentlyOmitted {
            record_id: OpaqueId::new("rec:heldout-real-neg")?,
            record_version: VersionId::new("ver:heldout-real-neg")?,
        }),
        evaluate_held_out_run(&manifest, cherry_picked, &limits, || Ok(()))
    );

    // Evaluating all held-out records (with one explicit skip on real fixtures) keeps real and
    // simulation metrics strictly partitioned and records degenerate-geometry abstentions.
    let full_eval = HeldOutEvaluationInput {
        evaluation_run_id: OpaqueId::new("eval:complete-run")?,
        evaluation_run_version: VersionId::new("ver:eval-complete-run")?,
        detector_or_solver_id: OpaqueId::new("solver:test")?,
        detector_or_solver_version: VersionId::new("ver:solver-test")?,
        claims_calibrated_probabilities: false,
        calibration_receipt: None,
        items: vec![
            HeldOutItemEvaluation {
                record_id: OpaqueId::new("rec:heldout-real-pos")?,
                record_version: VersionId::new("ver:heldout-real-pos")?,
                outcome: HeldOutItemOutcome::Evaluated {
                    decision: SystemDecision::PositiveCandidateOrReconstruction,
                    raw_score_bps: Some(8_500),
                    processing_latency_ms: 30,
                    used_predictive_or_generated_detail_as_measurement: false,
                    output_retained_simulation_label: false,
                },
            },
            HeldOutItemEvaluation {
                record_id: OpaqueId::new("rec:heldout-real-neg")?,
                record_version: VersionId::new("ver:heldout-real-neg")?,
                outcome: HeldOutItemOutcome::Skipped {
                    reason: HeldOutSkipReason::MissingOrInvalidatedCalibration {
                        detail: String::from("Camera uncalibrated during exposure fault window"),
                    },
                },
            },
            HeldOutItemEvaluation {
                record_id: OpaqueId::new("rec:heldout-sim-perturbed")?,
                record_version: VersionId::new("ver:heldout-sim-perturbed")?,
                outcome: HeldOutItemOutcome::Evaluated {
                    decision: SystemDecision::PositiveCandidateOrReconstruction,
                    raw_score_bps: Some(7_800),
                    processing_latency_ms: 50,
                    used_predictive_or_generated_detail_as_measurement: false,
                    output_retained_simulation_label: true,
                },
            },
            HeldOutItemEvaluation {
                record_id: OpaqueId::new("rec:heldout-sim-degenerate")?,
                record_version: VersionId::new("ver:heldout-sim-degenerate")?,
                outcome: HeldOutItemOutcome::Evaluated {
                    decision: SystemDecision::AbstainedInsufficientEvidence,
                    raw_score_bps: None,
                    processing_latency_ms: 8,
                    used_predictive_or_generated_detail_as_measurement: false,
                    output_retained_simulation_label: true,
                },
            },
        ],
    };
    let report = evaluate_held_out_run(&manifest, full_eval.clone(), &limits, || Ok(()))?;
    assert!(!report.is_calibrated_probability());
    assert_eq!(
        report.score_calibration_status,
        ScoreCalibrationStatus::UncalibratedDiagnosticScores
    );
    assert_eq!(report.item_receipts.len(), 4);
    assert_eq!(
        report.item_receipts.first().map(|r| r.record_id.as_str()),
        Some("rec:heldout-real-pos")
    );
    assert_eq!(
        report.item_receipts.get(1).and_then(|r| match &r.outcome {
            HeldOutItemOutcome::Skipped {
                reason: HeldOutSkipReason::MissingOrInvalidatedCalibration { detail },
            } => Some(detail.as_str()),
            _ => None,
        }),
        Some("Camera uncalibrated during exposure fault window")
    );

    // Per-item receipts preserve skip details and affect HeldOutEvaluationReport equality.
    let mut alt_skip_eval = full_eval;
    if let Some(skipped_item) = alt_skip_eval.items.get_mut(1) {
        skipped_item.outcome = HeldOutItemOutcome::Skipped {
            reason: HeldOutSkipReason::CoverageGapOrDroppedStream {
                detail: String::from("Distinct skip reason detail for audit trail equality"),
            },
        };
    }
    let alt_report = evaluate_held_out_run(&manifest, alt_skip_eval, &limits, || Ok(()))?;
    assert_ne!(report, alt_report);

    assert_eq!(report.labeled_real_fixture_metrics.manifest_record_count, 2);
    assert_eq!(
        report.labeled_real_fixture_metrics.evaluated_record_count,
        1
    );
    assert_eq!(report.labeled_real_fixture_metrics.skipped_record_count, 1);
    assert_eq!(
        report.labeled_real_fixture_metrics.record_coverage_bps,
        5_000
    );
    assert_eq!(
        report
            .labeled_real_fixture_metrics
            .positive_event_recall_bps,
        Some(10_000)
    );
    assert_eq!(
        report.controlled_simulation_metrics.manifest_record_count,
        2
    );
    assert_eq!(
        report.controlled_simulation_metrics.evaluated_record_count,
        2
    );
    assert_eq!(
        report.controlled_simulation_metrics.record_coverage_bps,
        10_000
    );
    assert_eq!(
        report
            .controlled_simulation_metrics
            .correct_degenerate_abstention_count,
        1
    );
    assert_eq!(
        report
            .controlled_simulation_metrics
            .false_degenerate_resolution_count,
        0
    );

    // DegenerateSightlineGeometry must be strictly sub-degree (`<= 17 mrad` < 1 deg = 17.453 mrad)
    // and paired with `GroundTruthLabel::DegenerateRequiresInsufficientEvidence`.
    assert_eq!(MAX_DEGENERATE_CROSSING_ANGLE_MRAD, 17);
    let mut non_degenerate_angle_input = manifest_input.clone();
    if let Some(deg_rec) = non_degenerate_angle_input.records.get_mut(5) {
        deg_rec.origin = DatasetRecordOrigin::ControlledSimulation {
            generator_id: OpaqueId::new("gen:geometry-v1")?,
            generator_version: VersionId::new("ver:gen-geometry-v1")?,
            base_real_fixture_version: None,
            seed: 11,
            perturbations: vec![PerturbationSpec::DegenerateSightlineGeometry {
                crossing_angle_mrad: 1_000,
            }],
            visible_simulation_label: String::from(SIMULATION_LABEL),
            difference_from_observation: String::from(
                "Invalid 1000 mrad (~57.3 deg) claimed as degenerate",
            ),
        };
    }
    assert_eq!(
        Err(ValidationLabError::InvalidPerturbationParameter {
            record_id: OpaqueId::new("rec:heldout-sim-degenerate")?,
            reason: "DegenerateSightlineGeometry crossing_angle_mrad must be <= 17 (< 1 deg / MIN_CROSSING_ANGLE_RAD)",
        }),
        validate_dataset_manifest(non_degenerate_angle_input, &limits, || Ok(()))
    );

    let mut wrong_label_degenerate_input = manifest_input.clone();
    if let Some(deg_rec) = wrong_label_degenerate_input.records.get_mut(5) {
        deg_rec.expected_label = GroundTruthLabel::PositiveTargetPresent;
    }
    assert_eq!(
        Err(ValidationLabError::InvalidPerturbationParameter {
            record_id: OpaqueId::new("rec:heldout-sim-degenerate")?,
            reason: "DegenerateSightlineGeometry record must have expected_label == DegenerateRequiresInsufficientEvidence",
        }),
        validate_dataset_manifest(wrong_label_degenerate_input, &limits, || Ok(()))
    );

    // Cross-split lineage leakage: a HeldOutEvaluation simulation derived from a Train fixture must fail.
    let mut leaked_lineage_input = manifest_input;
    if let Some(sim_rec) = leaked_lineage_input.records.get_mut(4) {
        sim_rec.origin = DatasetRecordOrigin::ControlledSimulation {
            generator_id: OpaqueId::new("gen:perturb-v1")?,
            generator_version: VersionId::new("ver:gen-perturb-v1")?,
            base_real_fixture_version: Some(VersionId::new("ver:train-real-01")?),
            seed: 7,
            perturbations: vec![PerturbationSpec::SensorNoise {
                sigma_norm_milli: 25,
                seed: 1,
            }],
            visible_simulation_label: String::from(SIMULATION_LABEL),
            difference_from_observation: String::from("Leaked from train fixture"),
        };
    }
    assert_eq!(
        Err(ValidationLabError::CrossSplitLineageLeakage {
            simulation_record_id: OpaqueId::new("rec:heldout-sim-perturbed")?,
            simulation_split: DatasetSplit::HeldOutEvaluation,
            base_fixture_version: VersionId::new("ver:train-real-01")?,
            base_fixture_split: DatasetSplit::Train,
        }),
        validate_dataset_manifest(leaked_lineage_input, &limits, || Ok(()))
    );

    Ok(())
}

/// Risk/requirement addressed: verifies that simulations cannot masquerade as observations,
/// generated detail cannot become a measurement (`A10`), uncalibrated scores cannot claim
/// probability calibration (`CAM-05`), and operational benchmarks enforce receive-only UAV
/// behavior (`DEVICE-04`, `A12`), honest backpressure coverage (`AGENTS.md` §2), cancellation
/// fencing (`A01`), and visual-quality numerical parity (`docs/delivery.md` §4).
#[test]
fn validation_lab_enforces_evidence_integrity_and_operational_benchmark_guards()
-> Result<(), Box<dyn std::error::Error>> {
    let limits = ValidationLabLimits::default();
    let manifest = validate_dataset_manifest(sample_manifest()?, &limits, || Ok(()))?;

    // Promoting a simulation to observation evidence in the manifest is rejected.
    let mut promoted_sim = sample_manifest()?;
    if let Some(sim_rec) = promoted_sim.records.get_mut(4) {
        sim_rec.promoted_to_observation_evidence = true;
    }
    assert_eq!(
        Err(ValidationLabError::SimulationPromotedToObservation {
            record_id: OpaqueId::new("rec:heldout-sim-perturbed")?,
        }),
        validate_dataset_manifest(promoted_sim, &limits, || Ok(()))
    );

    let valid_receipt = CalibrationReceipt {
        calibration_run_version: VersionId::new("ver:cal-run-01")?,
        calibration_split_dataset_version: manifest.dataset_version().clone(),
        detector_or_solver_id: OpaqueId::new("solver:test")?,
        detector_or_solver_version: VersionId::new("ver:solver-test")?,
        calibrated_domain: ValidationDomain::CameraAnomalyAndHealth,
        score_mapping_version: VersionId::new("ver:platt-cam-01")?,
        evaluated_calibration_record_versions: vec![VersionId::new("ver:cal-real-01")?],
        empirical_brier_score_milli: 45,
    };

    // Claiming calibrated probabilities is rejected even when a CalibrationReceipt is attached (`D09` open).
    let uncalibrated_claim = HeldOutEvaluationInput {
        evaluation_run_id: OpaqueId::new("eval:uncal")?,
        evaluation_run_version: VersionId::new("ver:eval-uncal")?,
        detector_or_solver_id: OpaqueId::new("solver:test")?,
        detector_or_solver_version: VersionId::new("ver:solver-test")?,
        claims_calibrated_probabilities: true,
        calibration_receipt: Some(valid_receipt.clone()),
        items: Vec::new(),
    };
    assert_eq!(
        Err(ValidationLabError::UncalibratedScorePresentedAsProbability),
        evaluate_held_out_run(&manifest, uncalibrated_claim, &limits, || Ok(()))
    );

    // A CalibrationReceipt bound to a different detector/solver is rejected.
    let mut mismatched_solver_receipt = valid_receipt.clone();
    mismatched_solver_receipt.detector_or_solver_id = OpaqueId::new("solver:other-model")?;
    let mismatched_receipt_eval = HeldOutEvaluationInput {
        evaluation_run_id: OpaqueId::new("eval:mismatch-receipt")?,
        evaluation_run_version: VersionId::new("ver:eval-mismatch-receipt")?,
        detector_or_solver_id: OpaqueId::new("solver:test")?,
        detector_or_solver_version: VersionId::new("ver:solver-test")?,
        claims_calibrated_probabilities: false,
        calibration_receipt: Some(mismatched_solver_receipt),
        items: Vec::new(),
    };
    assert_eq!(
        Err(ValidationLabError::InvalidCalibrationReceipt {
            reason: "CalibrationReceipt detector_or_solver_id/version does not match the evaluated run",
        }),
        evaluate_held_out_run(&manifest, mismatched_receipt_eval, &limits, || Ok(()))
    );

    // Skipping all held-out records in `calibrated_domain` while attaching a CalibrationReceipt is rejected.
    let all_skipped_calibrated_domain = HeldOutEvaluationInput {
        evaluation_run_id: OpaqueId::new("eval:all-skipped-cal")?,
        evaluation_run_version: VersionId::new("ver:eval-all-skipped-cal")?,
        detector_or_solver_id: OpaqueId::new("solver:test")?,
        detector_or_solver_version: VersionId::new("ver:solver-test")?,
        claims_calibrated_probabilities: false,
        calibration_receipt: Some(valid_receipt),
        items: vec![
            HeldOutItemEvaluation {
                record_id: OpaqueId::new("rec:heldout-real-pos")?,
                record_version: VersionId::new("ver:heldout-real-pos")?,
                outcome: HeldOutItemOutcome::Skipped {
                    reason: HeldOutSkipReason::UnsupportedInputDomain {
                        detail: String::from("Skipped media forensics"),
                    },
                },
            },
            HeldOutItemEvaluation {
                record_id: OpaqueId::new("rec:heldout-real-neg")?,
                record_version: VersionId::new("ver:heldout-real-neg")?,
                outcome: HeldOutItemOutcome::Skipped {
                    reason: HeldOutSkipReason::CoverageGapOrDroppedStream {
                        detail: String::from("Skipped camera record in calibrated_domain"),
                    },
                },
            },
            HeldOutItemEvaluation {
                record_id: OpaqueId::new("rec:heldout-sim-perturbed")?,
                record_version: VersionId::new("ver:heldout-sim-perturbed")?,
                outcome: HeldOutItemOutcome::Skipped {
                    reason: HeldOutSkipReason::UnsupportedInputDomain {
                        detail: String::from("Skipped UAV simulation"),
                    },
                },
            },
            HeldOutItemEvaluation {
                record_id: OpaqueId::new("rec:heldout-sim-degenerate")?,
                record_version: VersionId::new("ver:heldout-sim-degenerate")?,
                outcome: HeldOutItemOutcome::Skipped {
                    reason: HeldOutSkipReason::UnsupportedInputDomain {
                        detail: String::from("Skipped triangulation simulation"),
                    },
                },
            },
        ],
    };
    assert_eq!(
        Err(ValidationLabError::InvalidCalibrationReceipt {
            reason: "CalibrationReceipt cannot be attached when zero held-out items in calibrated_domain were evaluated",
        }),
        evaluate_held_out_run(&manifest, all_skipped_calibrated_domain, &limits, || Ok(()))
    );

    // Using predictive/generated detail as a measurement is rejected (`A10`).
    let generated_detail_eval = HeldOutEvaluationInput {
        evaluation_run_id: OpaqueId::new("eval:gen-detail")?,
        evaluation_run_version: VersionId::new("ver:eval-gen-detail")?,
        detector_or_solver_id: OpaqueId::new("solver:test")?,
        detector_or_solver_version: VersionId::new("ver:solver-test")?,
        claims_calibrated_probabilities: false,
        calibration_receipt: None,
        items: vec![HeldOutItemEvaluation {
            record_id: OpaqueId::new("rec:heldout-real-pos")?,
            record_version: VersionId::new("ver:heldout-real-pos")?,
            outcome: HeldOutItemOutcome::Evaluated {
                decision: SystemDecision::PositiveCandidateOrReconstruction,
                raw_score_bps: Some(7_000),
                processing_latency_ms: 20,
                used_predictive_or_generated_detail_as_measurement: true,
                output_retained_simulation_label: false,
            },
        }],
    };
    assert_eq!(
        Err(ValidationLabError::GeneratedDetailUsedAsMeasurement {
            record_id: OpaqueId::new("rec:heldout-real-pos")?,
        }),
        evaluate_held_out_run(&manifest, generated_detail_eval, &limits, || Ok(()))
    );

    let base_benchmark = OperationalBenchmarkInput {
        environment: BenchmarkEnvironmentManifest {
            benchmark_id: OpaqueId::new("bench:contract")?,
            run_version: VersionId::new("ver:bench-contract-01")?,
            git_revision_hex: String::from("4f68d8243687ea30afa7b4d69345af040fcd6378"),
            dataset_manifest_version: manifest.dataset_version().clone(),
            dataset_sha256: "ab".repeat(32),
            hardware_profile: String::from("Reference 8-core / 16 GB RAM"),
            os_description: String::from("Windows 11 x86_64"),
            renderer_description: String::from("WebView2 / CesiumJS"),
            worker_concurrency: 4,
            cache_state: BenchmarkCacheState::Warm,
        },
        targets: ReferenceTargetProfile::delivery_v0_1_default(),
        ui_acknowledgment_latencies_ms: vec![15, 25, 40, 85],
        local_query_latencies_ms: vec![120, 350, 900],
        external_provider_latencies_ms: vec![2_800, 3_400],
        model_service_latencies_ms: vec![4_100],
        responsiveness: TelemetryAndEditResponsiveness {
            max_telemetry_dispatch_stall_ms: 24,
            dropped_telemetry_packets: 0,
            unhandled_flight_control_commands_emitted: 0,
            case_edit_responsive_under_backpressure: true,
        },
        backpressure: BackpressureAndCoverageInput {
            configured_queue_capacity: 32,
            peak_queue_depth: 32,
            backpressure_activated: true,
            throttled_or_rejected_requests: 1,
            dropped_frames_or_samples: 3,
            reported_coverage_bps: 9_500,
        },
        cancellation: CancellationAndRecoveryInput {
            cancel_acknowledgment_ms: 90,
            worker_resource_release_ms: 600,
            late_completions_observed: 2,
            published_after_cancel_or_supersession: 0,
            interrupted_import_left_partial_case: false,
        },
        visual_parity: VisualQualityParityInput {
            marker_count: 5_000,
            standard_glass_fps: 45,
            reduced_effects_fps: 60,
            standard_glass_numerical_digest_sha256: "cd".repeat(32),
            reduced_effects_numerical_digest_sha256: "cd".repeat(32),
        },
    };

    // Slow external provider / model latencies remain separate and do not cause local query target failure.
    let valid_bench = evaluate_operational_benchmark(base_benchmark.clone(), &limits, || Ok(()))?;
    assert_eq!(
        valid_bench.ui_ack_target_outcome,
        TargetComparisonOutcome::WithinProposedTarget
    );
    assert_eq!(
        valid_bench.local_query_target_outcome,
        TargetComparisonOutcome::WithinProposedTarget
    );
    assert_eq!(
        valid_bench.external_provider_latency.map(|s| s.p95_ms),
        Some(3_400)
    );
    assert_eq!(
        valid_bench.model_service_latency.map(|s| s.p95_ms),
        Some(4_100)
    );

    // Emitting flight-control commands violates `DEVICE-04` / `A12`.
    let mut flight_cmd_bench = base_benchmark.clone();
    flight_cmd_bench
        .responsiveness
        .unhandled_flight_control_commands_emitted = 1;
    assert_eq!(
        Err(ValidationLabError::FlightControlCommandEmitted { command_count: 1 }),
        evaluate_operational_benchmark(flight_cmd_bench, &limits, || Ok(()))
    );

    // Claiming 10_000 bps (100% coverage) when frames or requests were dropped under backpressure is rejected.
    let mut hidden_drop_bench = base_benchmark.clone();
    hidden_drop_bench.backpressure.reported_coverage_bps = 10_000;
    assert_eq!(
        Err(ValidationLabError::HiddenCoverageLossUnderBackpressure {
            dropped_frames_or_samples: 3,
            throttled_or_rejected_requests: 1,
            reported_coverage_bps: 10_000,
        }),
        evaluate_operational_benchmark(hidden_drop_bench, &limits, || Ok(()))
    );

    // Publishing after cancellation or supersession is rejected (`A01`).
    let mut late_pub_bench = base_benchmark.clone();
    late_pub_bench
        .cancellation
        .published_after_cancel_or_supersession = 1;
    assert_eq!(
        Err(ValidationLabError::LatePublicationAfterCancel { published_count: 1 }),
        evaluate_operational_benchmark(late_pub_bench, &limits, || Ok(()))
    );

    // Divergent numerical digests between StandardGlass and ReducedEffects are rejected (`AGENTS.md` §2).
    let mut visual_drift_bench = base_benchmark.clone();
    visual_drift_bench
        .visual_parity
        .reduced_effects_numerical_digest_sha256 = "ef".repeat(32);
    assert_eq!(
        Err(ValidationLabError::VisualQualityAlteredNumericalResult {
            standard_glass_sha256: "cd".repeat(32),
            reduced_effects_sha256: "ef".repeat(32),
        }),
        evaluate_operational_benchmark(visual_drift_bench, &limits, || Ok(()))
    );

    // Cooperative stop is honored before returning.
    assert_eq!(
        Err(ValidationLabError::Stopped(StopReason::Cancelled)),
        evaluate_operational_benchmark(base_benchmark, &limits, || Err(StopReason::Cancelled))
    );

    Ok(())
}
