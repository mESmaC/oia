# Market-effects forecasting from news, weather, releases, and conditional scenarios

Implemented slice of [#44](https://github.com/mESmaC/oia/issues/44), supporting
`MARKET-03`, `MARKET-04`, and `MARKET-05` in [the specification](../spec.md),
Sections 2.2, 3.1, and 6.4 of [the architecture](../architecture.md), and the
market-prediction subset of scenario `A14` in
[delivery and verification](../delivery.md). The Rust library entry points live
in [`oia_forecast::market_effects`](../../crates/oia-forecast/src/market_effects.rs)
and compose with [`oia_forecast::issuance`](../../crates/oia-forecast/src/issuance.rs),
[`oia_forecast::interval`](../../crates/oia-forecast/src/interval.rs), and
[`oia_forecast::scoring`](../../crates/oia-forecast/src/scoring.rs).

## Contract

1. **Catalyst provenance, cited exposure pathways, and conditional vs. current-condition separation (`MARKET-03`, `A14`):**
   - `CatalystInputRecord` supports five point-in-time catalyst domains
     (`CatalystDomain`): `NewsCoverage`, `WeatherForecastVintage` (requiring
     `is_observed_post_hoc_weather == false` and
     `model_run_origin_utc_ms <= min(public_release_utc_ms, information_cutoff_utc_ms)`),
     `DeclassificationOrPublicRelease`, `CorporateOrRegulatoryDisclosure`, and
     `MacroeconomicRelease`.
   - Every market-effect forecast must cite at least one
     `ExposurePathwayCitation` connecting a declared `catalyst_id` to
     `target_instrument_id` via an explicit `ExposureMechanismKind`,
     `expected_direction` (`MarketDirection`), and non-empty
     `cited_evidence_versions` verified against the forecast's cutoff-admissible
     `sources`.
   - `MarketForecastMode` distinguishes unconditional `CurrentCondition`
     forecasts from `ConditionalScenario` forecasts. `ConditionalMarketScenario`
     keeps the trigger-event occurrence probability `P(E)`
     (`trigger_occurrence_probability`) strictly separate from the conditional
     market response given the event `R | E` (`response_given_event`) and the
     optional complementary branch `R | ¬E` (`response_given_non_event`). When a
     conditional scenario issues a quantitative binary market-outcome forecast,
     it requires `response_given_non_event` and verifies that the unconditional
     `forecast_probability` matches
     $P(E) P(M \mid E) + (1 - P(E)) P(M \mid \neg E)$ within `1e-9` rather than
     conflating `P(E)` with `P(M)`.

2. **Three-timestamp awareness model, novelty/surprise, adjustable assumptions, sensitivity, and qualitative fallback (`MARKET-04`, `A14`):**
   - Every `CatalystInputRecord` enforces
     `historical_event_utc_ms <= public_release_utc_ms <= market_awareness_utc_ms <= information_cutoff_utc_ms`,
     rejecting unknown (`None`) or post-cutoff public release and market
     awareness timestamps even when the historical event occurred before the
     cutoff.
   - `ExpectationAndNoveltyAssessment` records `NoveltyStatus`
     (`NovelUnanticipated`, `PartiallyAnticipated`,
     `ScheduledReleaseWithConsensus`, and
     `RedundantRestatementOfPriorRelease { original_release_version }`), validates
     optional `ExpectationSurpriseInput` (`stated_surprise == realized_release_value - prior_expectation_value`
     within `1e-9`), refuses non-muted quantitative incremental shocks from
     redundant restatements (`MarketEffectError::RedundantReleaseClaimsIncrementalShock`),
     and pairs every assessment with
     `REAL_WORLD_EFFECT_NOT_INCREMENTAL_PRICE_CAVEAT`.
   - `ScenarioAssumptionParameter` and `AssumptionSensitivityPoint` enforce
     ordered finite parameter bounds (`low_value <= base_value <= high_value`
     with `low_value < high_value`), alignment of `base_case_effect` with the
     quantitative `point_estimate`, and exact sign-flip detection
     (`direction_flips_across_range`).
   - `PrecisionSupport::UnsupportedPrecision` requires
     `MarketEffectPrediction::QualitativeOrInsufficient` (`DirectionalOnly`,
     `VolatilityExpansionOnly`, or `InsufficientEvidence`); supplying a
     quantitative prediction when precision is unsupported is refused with
     `MarketEffectError::PrecisionSupportMismatch`.

3. **Leakage detection, immutable vintage/interval/baseline evaluation, and strategy simulation guardrails (`MARKET-05`, `A14`):**
   - `MarketEffectForecastBook::issue_market_effect_forecast` detects and refuses:
     - Post-hoc weather observations (`MarketLeakageKind::ObservedWeatherUsedInsteadOfForecastVintage`)
       and post-cutoff weather model runs (`MarketLeakageKind::WeatherModelRunAfterCutoffOrRelease`).
     - Revised economic/release vintages published after `information_cutoff_utc_ms`
       (`MarketLeakageKind::RevisedReleaseVintageAfterCutoff`).
     - Later news or public releases after an earlier historical event
       (`MarketLeakageKind::LaterNewsOrReleaseAfterEventCutoff`) and post-cutoff
       market awareness (`MarketLeakageKind::MarketAwarenessAfterCutoff`).
     - Post-cutoff instrument/universe selection
       (`MarketLeakageKind::InstrumentSelectionAfterCutoff` when
       `instrument_selection_as_of_utc_ms > information_cutoff_utc_ms`).
     - Post-cutoff or unknown language-model knowledge under `ProspectiveStrict`
       (`IssuanceError::ModelKnowledgeAfterStrictCutoff`).
   - Quantitative vintages bridge directly into `ForecastLedger` (`#43`) and
     `evaluate_intervals` (`#43`), including across mixed
     `QualitativeOrInsufficient <-> Quantitative` series revisions (`Qual -> Quant`
     initializes the binary ledger chain while preserving market-series lineage,
     and `Quant -> Qual -> Quant` bridges to the most recent prior quantitative
     vintage with accumulated new evidence versions and frozen quantitative
     series fields).
   - `evaluate_market_effect_cohort` validates scalar interval scoring and finite
     point-error bounds before committing binary `ForecastLedger` evaluations so
     failed evaluations never leave orphaned state, requires all earlier vintages
     (`1..vintage_number`) of any included series to be present in the cohort,
     validates scalar resolution target, series `effect_unit`, horizon, and
     chronology across both quantitative and qualitative vintages, retains
     resolution version and evidence lineage on `EvaluatedMarketVintageSummary`,
     maintains a single canonical scalar resolution history across alias
     `resolution_id`s for the same `(target, unit, horizon)`, and flags existing
     evaluations with `MarketReassessmentReason` when a scalar or paired binary
     resolution in the owning `ForecastLedger` is superseded without mutating
     cached historical scores (cloning `ForecastLedger`,
     `MarketEffectForecastBook`, or `MarketEffectEvaluationRecord` creates an
     isolated value snapshot so mutations in a forked clone never cross-contaminate
     the original book or ledger unless explicitly synchronized via
     `sync_ledger_reassessments` / `evaluation_with_ledger`).
   - `evaluate_simulated_strategy` enforces a return-expressible `effect_unit`
     via `SimulatedReturnBasis` (`BasisPoints`, `LogBasisPoints`,
     `PercentagePoints`, `LogPercentagePoints`, `DecimalReturn`,
     `LogDecimalReturn`), refuses non-return/level/volatility units, converts
     arithmetic and logarithmic returns (`expm1(r_log) * 10_000`) into arithmetic
     basis points before deducting strictly positive round-trip transaction
     frictions (`commission_bps_per_side`, `half_spread_bps_per_side`,
     `slippage_bps_per_side`), and guarantees
     `is_observed_market_return() == false` and `can_execute_orders() == false`.

## Limitations and remaining `#44` scope

- **In-memory kernel only:** `MarketEffectForecastBook` does not persist records
  to SQLite (`oia-store`, `#11`), ingest live news/weather/market providers
  (`#13`, `#20`, `#24`, `#41`), wire into shared application services (`#14`),
  or render UI charts/panels (`#16`, `#42`).
- **Open decision `D12`:** Selection of production forecasting targets, horizons,
  baselines, point-in-time datasets, and fitted calibration methods remains open
  in `#43`–`#45`.
- **No formal proof claim:** No Lean theorem or Rust-to-Lean correspondence is
  claimed for this module.

## Local verification

Run with pinned Rust `1.99.0`:

```text
cargo +1.99.0 run -p oia-forecast --example inspect-a14-market-effects --locked --offline
cargo +1.99.0 fmt --all -- --check
cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings
cargo +1.99.0 test --workspace --locked --offline
```
