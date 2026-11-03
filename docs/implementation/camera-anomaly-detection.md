# Camera anomaly candidate, health classification, review, and operational benchmark kernel

Implemented slice of [#29](https://github.com/mESmaC/oia/issues/29), supporting
`CAM-01`–`CAM-05` and the deterministic validation/evaluation subset of `A09` in
[the product specification](../spec.md) and [delivery plan](../delivery.md).
The Rust entry points live in [`crates/oia-media/src/camera_anomaly.rs`](../../crates/oia-media/src/camera_anomaly.rs):

- `oia_media::camera_anomaly::CameraAnomalyMonitor`
- `oia_media::camera_anomaly::validate_cross_camera_association`

## Contract

1. **Camera source admission and privacy boundaries (`CAM-01`, `CAM-02`, `A09`):**
   - `CameraAnomalyMonitor::new` rejects views classified as `CameraViewKind::HomeInterior` or `CameraViewKind::ViewIntoHomeInterior` (`AnomalyError::HomeInteriorExcluded`), sources where `automated_processing` is `SourceUsePermission::Prohibited` or `SourceUsePermission::Unknown` (`AnomalyError::AutomatedProcessingNotPermitted`), and sources that are `CameraAccessStatus::RestrictedOrUnavailable` (`AnomalyError::SourceRestricted` with `RESTRICTED_SOURCE_POLICY_NOTICE` stating that OIA records unavailability and never probes hidden endpoints, guesses credentials, or seeks alternate access).
   - `CameraViewKind::ResidentialExteriorStreetView` and any region with `requires_privacy_mask == true` require `privacy_mask_applied == true` (`AnomalyError::PrivacyMaskRequiredForRegion`) so ordinary street views with residential building exteriors are supported only when private windows/areas are masked (`CAM-01`, `CAM-02`).
   - Source profiles separately record `automated_processing`, `clip_retention`, and `redistribution` permissions using `SourceUsePermission { Permitted, Prohibited, Unknown }`.
   - Non-live substitutes (`FeedObservationKind::StillFallback`, `StreetViewFallback`, `SyntheticPlaceholder`) are rejected as scene-anomaly event triggers (`AnomalyError::NonObservationFeedSubstitute`) so fallbacks cannot masquerade as live observation evidence (`AGENTS.md` §2).
2. **Separation of camera faults from scene events (`CAM-03`, `A09`):**
   - `ObservationStepInput::HealthEvent` records feed faults and coverage interruptions (`FrozenFrame`, `ExposureShift`, `CameraRepositioned`, `RepeatedRebroadcast`, `TimingDrift`, `CoverageGap`, `SourceRestricted`) over half-open intervals `[start_ms, end_ms)` in `health_events()` without emitting candidate scene alerts.
   - Any `SceneTrigger` whose uncertainty-widened interval `[event_start_ms - clock_uncertainty_ms, event_end_ms + clock_uncertainty_ms)` overlaps an active fault interval on the same camera is rejected with `AnomalyError::CameraFaultBlocksSceneTrigger`.
   - Late-arriving faults retroactively mark any overlapping prior candidate with `quarantined_by_fault = Some(health_state)` and mark any existing human reviews `needs_reassessment = true` so late fault detection does not leave false alerts unflagged.
   - Recording `CameraHealthState::CameraRepositioned` automatically transitions a `CameraCalibrationState::Valid` camera to `CameraCalibrationState::InvalidatedByMovement`, preserving the prior calibration `VersionId` and detection timestamp (`CAM-05`, `A09`).
   - Recording `CameraHealthState::SourceRestricted` transitions the source to `RestrictedOrUnavailable`, refuses all subsequent triggers without attempting alternate access (`CAM-01`, `A09`), and treats coverage as unavailable from `start_ms` onward.
3. **Candidate packaging, original context preservation, and versioned deduplication (`CAM-04`, `AGENTS.md` §2):**
   - Every candidate scene event requires a primary `RetainedFrameRef` with `FrameArtifactOrigin::OriginalRetainedFrame` and a validated 64-character lowercase hexadecimal SHA-256 digest (shared via `crate::is_valid_lowercase_sha256_hex`). Attempting to supply `FrameArtifactOrigin::PredictiveOrGeneratedDerivative` is rejected with `AnomalyError::GeneratedFrameForbiddenAsEvidence`.
   - Pre/post context (`ContextWindowStatus`) distinguishes `Complete` (requiring at least one retained pre or post frame), `PartialBufferExpired` (requiring `expired_frame_count >= 1`), and `OmittedByRetentionPolicy` (with an explicit reason), and enforces `clip_retention == SourceUsePermission::Permitted` and `max_retention_window_ms`.
   - Overlapping or nearby triggers in the same `region_id` and `AnomalyCategory` whose gap `max(0, event_start_ms - cand.event_end_ms)` is within `deduplication_window_ms` merge into the active candidate (`TriggerOutcome::DeduplicatedIntoExisting`). Every merge appends a full `RetainedTriggerSegment` (preserving the merged trigger's `observation_id`, `segment_version`, primary/context frames, timestamps, and explanation), advances `current_version` to the new trigger's `candidate_version`, and sets `needs_reassessment = true` on the candidate and any prior reviews (`AGENTS.md` §2).
   - Raw detector scores (`peak_raw_score_bps`) are paired with `UNCALIBRATED_SCORE_CAVEAT` (`is_score_calibrated_probability() == false`) and `CANDIDATE_EVENT_CAVEAT` stating that an anomaly is a reason to review rather than an identification of a UAP, crime, or emergency.
4. **Append-only human review and cross-camera association (`CAM-05`):**
   - `review_candidate` validates `expected_current_version`, monotonic `reviewed_at_ms`, and non-reused `VersionId`s; verifies that every cited frame version belongs to the candidate's retained trigger segments (primary or context frames across all merged segments); and appends an immutable `AnomalyReviewEntry` (`ArtifactOrFault`, `LikelyOrdinaryExplanation`, `InsufficientEvidence`, `RetainedForInvestigation`) while clearing the candidate-level `needs_reassessment` flag.
   - `validate_cross_camera_association` requires distinct cameras, independent `upstream_feed_id`s (rejecting shared or rebroadcast feeds), non-quarantined candidates, no movement-invalidated calibration, and `temporal_skew_ms + combined_clock_uncertainty_ms <= max_allowed_skew_ms`. When `require_triangulation == false`, uncalibrated cameras may be associated with `TriangulationReadiness::UncalibratedAssociationOnly` (`eligible_for_triangulation() == false`); when `require_triangulation == true`, uncalibrated cameras are rejected with `AnomalyError::CalibrationRequiredForTriangulation`.
5. **Held-out operational benchmark evaluation (`CAM-03`–`CAM-05`, `A09`, `AGENTS.md` §8):**
   - `evaluate_held_out` records the pinned `evaluation_corpus_id`, `evaluation_corpus_version`, `[eval_start_ms, eval_end_ms)`, `detector_config_digest_sha256`, and `region_configs`.
   - Computes `missing_coverage_ms` from merged health/fault intervals (including open-ended unavailability after `SourceRestricted`), `active_coverage_ms`, and `coverage_bps`.
   - Separates supplied ground-truth events into `truth_events_outside_window_count`, `truth_events_in_coverage_gap_count`, and `observable_labeled_event_count`, computing `event_recall_bps` over observable active-coverage truth events so coverage gaps are not conflated with detector misses (`AGENTS.md` §§2, 8).
   - Reports window-scoped `total_candidate_alerts`, `quarantined_fault_candidates_in_window`, `true_positive_alerts`, `false_alert_count`, `false_alerts_per_camera_hour_milli` (normalized by active observed camera-hours), `early_detection_count`, `min/mean/max_detection_latency_ms`, `deduplicated_repeat_triggers_in_window`, and `health_events_overlapping_window`.

## Limitations and remaining `#29` scope

- This module is an in-memory validation, state-transition, and operational evaluation kernel; it does not decode video streams, run pixel-level background subtraction / optical-flow / neural inference, poll live camera directories (`#28`, `#13`), persist ring buffers or records to `oia-store` (`#11`), schedule background worker jobs (`#14`), or render the workspace UI (`#16`).
- `D09` (selection of concrete detection/restoration models, evaluation corpora, acceptance thresholds, and hardware budgets) remains open.
- No Lean theorem or formal Rust-to-Lean correspondence claim is made for this slice.

## Local verification

Executed on Windows x86_64 MSVC with pinned `rustc 1.99.0` on 2026-11-02:

- `cargo +1.99.0 fmt --all -- --check`
- `cargo +1.99.0 clippy -p oia-media --all-targets --locked --offline -- -D warnings`
- `cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings`
- `cargo +1.99.0 test --workspace --locked --offline`
- `cargo +1.99.0 run -p oia-media --example evaluate-camera-anomalies --locked --offline`

Two targeted contract tests in [`crates/oia-media/tests/camera_anomaly_contract.rs`](../../crates/oia-media/tests/camera_anomaly_contract.rs) verify:
1. Overlapping and adjacent trigger deduplication (`B2`), retained trigger segment evidence and version advancement with review `needs_reassessment` flagging (`B3`), active-coverage recall separating truth events outside the evaluation window or inside coverage gaps (`B4`), and window-scoped deduplication and health-event counters (`B6`).
2. Refusal of home-interior cameras (`HomeInterior`, `ViewIntoHomeInterior`), unmasked residential exterior street views, restricted sources and permanent post-restriction coverage loss (`B5`), unknown/prohibited automated-processing permissions, non-observation feed substitutes, predictive/generated derivative frames, triggers overlapping active camera faults, shared-upstream-feed associations, movement-invalidated calibrations, and uncalibrated association versus triangulation readiness.
