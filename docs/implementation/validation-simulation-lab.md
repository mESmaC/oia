# Validation and simulation lab, controlled perturbation manifest, and operational benchmark kernel

Implemented dependency-independent slice of [#40](https://github.com/mESmaC/oia/issues/40), supporting `FIELD-04`, `CAM-05`, `MEDIA-02`, `DEVICE-04`, [delivery plan §4](../delivery.md#4-proposed-performance-and-resource-targets), and the deterministic fixture/simulation/benchmark evaluation subsets of `A09`, `A10`, `A12`, and `A13` in [the product specification](../spec.md) and [delivery plan](../delivery.md).

The Rust entry points live in [`crates/oia-analysis/src/validation_lab.rs`](../../crates/oia-analysis/src/validation_lab.rs):

- `oia_analysis::validation_lab::validate_dataset_manifest`
- `oia_analysis::validation_lab::evaluate_held_out_run`
- `oia_analysis::validation_lab::evaluate_operational_benchmark`

## Contract

1. **Strict separation of labeled real fixtures and controlled simulations (`FIELD-04`, `MEDIA-02`, `DEVICE-04`, `A10`, `A13`, `AGENTS.md` §2):**
   - `DatasetRecordOrigin` separates `LabeledRealFixture` (`can_serve_as_observation_evidence() == true`, `is_simulation() == false`) from `ControlledSimulation` (`can_serve_as_observation_evidence() == false`, `is_simulation() == true`).
   - Every `ControlledSimulation` record must carry the exact [`SIMULATION_LABEL`] (`"CONTROLLED SIMULATION — SYNTHETIC OR PERTURBED MATERIAL (NOT OBSERVATION EVIDENCE)"`), a non-empty `difference_from_observation` explanation, and at least one validated `PerturbationSpec` (`SensorNoise`, `MotionOrDefocusBlur`, `OpticalGlare`, `RollingShutterReadout`, `LensAndPerspectiveDistortion`, `LossyCompression`, `CameraMotionAndParallax`, `ClockOffsetAndDrift`, `FrameAndTelemetryDrop`, or `DegenerateSightlineGeometry`).
   - `PerturbationSpec::DegenerateSightlineGeometry` enforces `1 <= crossing_angle_mrad <= MAX_DEGENERATE_CROSSING_ANGLE_MRAD` (`17 mrad` < $\pi / 180\text{ rad} \approx 17.453\text{ mrad} = 1^\circ$, matching `oia_analysis::triangulation::MIN_CROSSING_ANGLE_RAD`) and requires the record's `expected_label` to be `GroundTruthLabel::DegenerateRequiresInsufficientEvidence`.
   - Any attempt to mark a `ControlledSimulation` record with `promoted_to_observation_evidence = true` is rejected with `ValidationLabError::SimulationPromotedToObservation`.
2. **Dataset provenance and split-leakage prevention on caller-supplied manifests (`FIELD-04`, `CAM-05`, `A09`):**
   - `validate_dataset_manifest` enforces unique `record_id` and `record_version` identifiers across `DatasetSplit::Train`, `DatasetSplit::CalibrationOrTuning`, and `DatasetSplit::HeldOutEvaluation`, and validates caller-supplied lowercase hexadecimal SHA-256 digests (`payload_sha256`).
   - Rejects cross-split payload SHA-256 reuse (`ValidationLabError::CrossSplitPayloadLeakage`), cross-split `source_version` reuse (`ValidationLabError::CrossSplitSourceVersionLeakage`), and cross-split simulation derivation where a simulation in one split derives from a `base_real_fixture_version` in another split (`ValidationLabError::CrossSplitLineageLeakage`).
   - Tracks `SplitComposition` separately for `labeled_real_fixture_count` and `controlled_simulation_count` in each split.
3. **Held-out evaluation without cherry-picking, per-item receipt retention, score calibration gating, and generated-detail guards (`#40`, `CAM-05`, `A09`, `A10`, `A13`):**
   - `evaluate_held_out_run` requires every `HeldOutEvaluation` record in the validated manifest to be accounted for as either `HeldOutItemOutcome::Evaluated` or `HeldOutItemOutcome::Skipped` with a typed `HeldOutSkipReason` (`UnsupportedInputDomain`, `MissingOrInvalidatedCalibration`, `CoverageGapOrDroppedStream`, `BudgetOrDeadlineStopped`). Silently omitting any held-out record fails with `ValidationLabError::HeldOutRecordSilentlyOmitted`.
   - Retains `item_receipts: Vec<HeldOutItemReceipt>` in deterministic manifest order on `HeldOutEvaluationReport` so every record's `SystemDecision`, `raw_score_bps`, `processing_latency_ms`, and `HeldOutSkipReason` detail string remain inspectable and participate in report equality.
   - Computes `PartitionMetrics` (`record_coverage_bps`, `true_positive_count`, `false_positive_count`, `false_negative_count`, `true_negative_count`, `correct_degenerate_abstention_count`, `false_degenerate_resolution_count`, `positive_event_recall_bps`, `false_alerts_per_hour_milli`, `mean_processing_latency_ms`, `p95_processing_latency_ms`) separately for `labeled_real_fixture_metrics` and `controlled_simulation_metrics` so synthetic simulations never inflate or dilute real-fixture metrics.
   - Because `D09` empirical calibration fitting and verification are not performed by this kernel, `claims_calibrated_probabilities = true` is always rejected with `ValidationLabError::UncalibratedScorePresentedAsProbability`, and `HeldOutEvaluationReport::is_calibrated_probability()` is always `false`. When a `CalibrationReceipt` is supplied (`ScoreCalibrationStatus::CallerDeclaredReceiptUnverified`), it must match the evaluated `detector_or_solver_id` and `detector_or_solver_version`, cover every `CalibrationOrTuning` record in `calibrated_domain` in the manifest, match all scored held-out items' domain, and have at least one evaluated held-out record in `calibrated_domain`.
   - Rejects `used_predictive_or_generated_detail_as_measurement = true` (`ValidationLabError::GeneratedDetailUsedAsMeasurement`) and rejects simulation outputs that drop their visible simulation label (`ValidationLabError::SimulationLabelDroppedInOutput`).
4. **Operational, backpressure, cancellation, and visual-parity benchmark receipt evaluation (`DEVICE-04`, `A01`, `A12`, `docs/delivery.md` §4):**
   - `evaluate_operational_benchmark` validates caller-supplied benchmark receipts and requires a pinned `BenchmarkEnvironmentManifest` (`git_revision_hex`, `dataset_sha256`, hardware/OS/renderer descriptions, `worker_concurrency`, and `BenchmarkCacheState`).
   - Summarizes `ui_acknowledgment_latency` and `local_query_latency` separately from `external_provider_latency` and `model_service_latency` (`docs/delivery.md` §4).
   - Rejects emitted UAV flight-control commands (`ValidationLabError::FlightControlCommandEmitted`, `DEVICE-04`, `A12`), rejects claims of `10_000` bps full coverage when frames/samples or requests were dropped under backpressure (`ValidationLabError::HiddenCoverageLossUnderBackpressure`, `AGENTS.md` §2), rejects late publications after cancellation or supersession (`ValidationLabError::LatePublicationAfterCancel`, `A01`), rejects partial case corruption after an interrupted import (`ValidationLabError::InterruptedImportCorruptedCase`), and requires identical SHA-256 numerical digests across `StandardGlass` and `ReducedEffects` visual modes (`ValidationLabError::VisualQualityAlteredNumericalResult`, `AGENTS.md` §2, `docs/delivery.md` §4).
   - Pairs all `TargetComparisonOutcome` comparisons with [`TARGET_VS_MEASURED_CAVEAT`] so design targets from `docs/delivery.md` §4 are never conflated with unexecuted production claims (`AGENTS.md` §3).

## Limitations and remaining `#40` scope

- This module is an in-memory validation, split-auditing, and benchmark-receipt evaluation kernel (`#40`); it validates caller-supplied digest strings, split constraints, and receipts rather than hashing payload files on disk, verifying physical ground-truth authenticity, synthesizing pixel rasters, executing live hardware/camera/drone benchmarks, persisting lab runs to `oia-store` (`#11`), scheduling background jobs (`#14`), or exposing a validation-lab UI panel in `oia-app` (`#16`).
- `D09` (selection of concrete detection/restoration models, evaluation corpora, empirical probability calibration verification, acceptance thresholds, and hardware budgets) remains open.
- No Lean theorem or formal Rust-to-Lean correspondence claim is made for this slice.

## Local verification

Executed on Windows x86_64 MSVC with pinned `rustc 1.99.0` on 2026-11-03:

```text
cargo +1.99.0 fmt --all -- --check
cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings
cargo +1.99.0 test --workspace --locked --offline
cargo +1.99.0 run -p oia-analysis --example evaluate-validation-lab --locked --offline
```

Two targeted contract tests in [`crates/oia-analysis/tests/validation_lab_contract.rs`](../../crates/oia-analysis/tests/validation_lab_contract.rs) verify:
1. Separation of labeled real fixtures and controlled simulations, partitioned real vs. simulation held-out metrics, per-item `HeldOutItemReceipt` retention and skip-detail equality, sub-degree `DegenerateSightlineGeometry` (`<= 17 mrad` with `DegenerateRequiresInsufficientEvidence`), degenerate-geometry abstention accounting, anti-cherry-picking rejection (`HeldOutRecordSilentlyOmitted`), and cross-split lineage leakage rejection (`CrossSplitLineageLeakage`).
2. Refusal of simulation-to-observation promotion (`SimulationPromotedToObservation`), uncalibrated probability claims even when a `CalibrationReceipt` is supplied (`UncalibratedScorePresentedAsProbability`), model/domain-mismatched or all-skipped `CalibrationReceipt` rejection (`InvalidCalibrationReceipt`), generated detail used as a measurement (`GeneratedDetailUsedAsMeasurement`), emitted flight-control commands (`FlightControlCommandEmitted`), hidden coverage loss under backpressure (`HiddenCoverageLossUnderBackpressure`), late publication after cancellation (`LatePublicationAfterCancel`), visual-quality numerical drift (`VisualQualityAlteredNumericalResult`), and cooperative cancellation (`Stopped(StopReason::Cancelled)`).
