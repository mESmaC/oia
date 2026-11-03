# Historical analogue retrieval, conditional response sequences, and `A15` forecasting kernel

Implemented slice of [#45](https://github.com/mESmaC/oia/issues/45), supporting
`FORECAST-01`, `FORECAST-02`, `FORECAST-03`, and `FORECAST-04` in
[the specification](../spec.md), Sections 2.2, 3.1, and 6.4 of
[the architecture](../architecture.md), and deterministic scenario `A15` in
[delivery and verification](../delivery.md). The Rust library entry points live
in [`oia_forecast::analogues`](../../crates/oia-forecast/src/analogues.rs) and
compose with [`oia_forecast::issuance`](../../crates/oia-forecast/src/issuance.rs)
and [`oia_forecast::scoring`](../../crates/oia-forecast/src/scoring.rs).

## Contract

1. **Sourced event-response histories and point-in-time knowability (`FORECAST-01`, `FORECAST-04`):**
   - `PrecedentLibrary` stores up to `5,000` versioned `PrecedentEpisodeRecord`
     entries across government, institutional, industry, and community actors
     (`PrecedentSector`), recording initiating conditions, actor response steps
     (`ActorResponseStep` with separate `announced_intention` and observed
     `action_summary`), terminal outcomes (`PrecedentOutcomeDisposition`),
     elapsed durations, and cited `CutoffSourceInput` versions.
   - Chronological ordering (`TemporalSequenceRelation::ObservedSequenceOnly`)
     is kept distinct from assessed causation
     (`TemporalSequenceRelation::AssessedCausalLink`, which requires a non-empty
     rationale and cited evidence versions).
   - `evaluate_episode_against_target` evaluates what was knowable at a
     forecast's `information_cutoff_utc_ms` under `ProspectiveStrict` or
     `RetrospectiveReconstructed`:
     - Episodes whose initiating event or initiation evidence was not admissible
       at cutoff are classified as `EpisodePointInTimeStatus::InadmissibleAtCutoff`
       and rejected from the active comparison set.
     - Episodes that initiated at or before cutoff whose outcome either
       remained `CensoredOrIncompleteFollowUp` or resolved / became publicly
       available *after* `information_cutoff_utc_ms` are classified as
       `EpisodePointInTimeStatus::InitiatedButOutcomeCensoredAtCutoff`
       (`PrecedentRole::CensoredAtCutoff`), excluding their post-cutoff outcome
       and post-cutoff evidence sources so future outcomes cannot leak into
       point-in-time base rates or forecast sources.

2. **Multi-criteria retrieval, editable comparison sets, and episode independence (`FORECAST-02`, `A15`):**
   - Compares each precedent against a target `AnalogueProfile` across seven
     explicit dimensions (`EventScale`, `Geography`, `ActorSector`,
     `InstitutionalRegime`, `TensionLevel`, `EconomicCondition`, and
     `InformationQuality`), producing per-dimension similarity/difference notes
     (`DimensionAssessment`), institutional regime difference disclosures, and
     transfer limitation notes.
   - `AnalogueComparisonSet::apply_edit` records auditable `ComparisonSetEdit`
     entries (`IncludeEpisode`, `ExcludeEpisode`, `AnnotateTransferLimits`).
     Excluding a counterexample retains the removed record in `excluded_entries`
     with its rationale and increments `excluded_counterexample_count`.
   - `EpisodeDependencyKind` (`IndependentEpisode`, `SharedCrisisOrCampaign`,
     `PolicySpilloverOrRetaliation`, `SharedSourceLineageOnly`) assigns a
     shared `independence_cluster_key` to related episodes so coordinated waves
     or syndicated reports do not inflate `independent_cluster_count` or
     `independent_cluster_support_ratio_bps`.
   - `PrecedentOutcomeDisposition::DocumentedNonResponse` represents verified
     observation windows where no action was taken, distinct from
     `CensoredOrIncompleteFollowUp`.

3. **Conditional response sequences, uncalibrated frequency caveats, and transferability (`FORECAST-03`, `A15`):**
   - `HistoricalFrequencySummary` reports active episode counts, independent
     cluster counts, dependent episode counts, supportive responses, divergent
     counterexamples, documented non-responses, cutoff-censored counts, raw and
     cluster-deduplicated support ratios in basis points, and elapsed-time
     bounds (`ElapsedTimeRangeMs { min_elapsed_ms, median_elapsed_ms, max_elapsed_ms }`),
     always paired with `is_calibrated_probability: false` and
     `UNCALIBRATED_HISTORICAL_FREQUENCY_CAVEAT`.
   - `AnalogueComparisonSet::assess_transferability` flags
     `TransferabilityAssessment::SparseOrPoorlyTransferable` when independent
     resolved clusters fall below `min_independent_clusters`, censored follow-up
     reaches at least half of active episodes, institutional regimes or low
     information quality dominate the comparison set, or counterexamples have
     been excluded.
   - `ConditionalResponseBranch` validates sequential step numbering, step
     dependency rationales, and consistency between `chained_probability` and
     the product of step `conditional_probability` values within `1e-9`.
   - `MonitoringIndicator` records observable signals, target branches,
     `IndicatorDirection`, source coverage limits, and false-positive caveats.

4. **`ForecastLedger` integration and timing evaluation (`FORECAST-04`, `A15`):**
   - `AnalogueComparisonSet::issue_forecast` bridges a validated comparison set
     and `AnalogueForecastIssuanceSpec` into `ForecastLedger::issue_forecast`,
     freezing active episode `VersionId`s into `comparison_set` and merging
     cutoff-admissible precedent sources with `scenario_sources`. Quantitative
     issuance is refused when `PredictionJustification` is
     `QualitativeScenarioOnly` or when history is `SparseOrPoorlyTransferable`
     without an explicit `sparse_history_override_rationale`.
   - `AnalogueComparisonSet::evaluate_resolution_timing` compares an observed
     resolution timestamp against the precedent `ElapsedTimeRangeMs`
     (`signed_error_from_median_ms`, `abs_error_from_median_ms`,
     `within_precedent_min_max_band`, and `band_excursion_ms`) while keeping
     non-occurrences, unresolved outcomes, and invalidated outcomes distinct.

## Limitations and remaining `#45` scope

- **In-memory kernel only:** `PrecedentLibrary` and `AnalogueComparisonSet` do
  not persist to SQLite (`oia-store`, `#11`), ingest live news/event streams
  (`#20`, `#13`), wire into shared application services (`#14`), or provide
  workspace UI views (`#16`).
- **Open decision `D12`:** Selection of initial production forecasting targets,
  horizons, point-in-time historical datasets, and fitted calibration methods
  remains open in `#43` and `#45`.
- **No formal proof claim:** No Lean theorem or Rust-to-Lean correspondence is
  claimed for this module.

## Local verification

Run with pinned Rust `1.99.0`:

```text
cargo +1.99.0 run -p oia-forecast --example inspect-a15-historical-analogues --locked --offline
cargo +1.99.0 fmt --all -- --check
cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings
cargo +1.99.0 test --workspace --locked --offline
```
