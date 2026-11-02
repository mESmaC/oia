# Immutable forecast issuance, cutoff admissibility, and resolution lifecycle

Implemented slice of [#43](https://github.com/mESmaC/oia/issues/43), supporting
`FORECAST-04`, `MARKET-04`, and `MARKET-05` in [the specification](../spec.md)
and Sections 2.2, 3.1, and 6.4 of [the architecture](../architecture.md). The
Rust library entry points live in [`oia_forecast::issuance`](../../crates/oia-forecast/src/issuance.rs)
and compose with [`oia_forecast::scoring::evaluate_binary`](../../crates/oia-forecast/src/scoring.rs).

## Contract

`ForecastLedger` enforces a bounded (`1..=10,000` records per family and total
evaluated vintage summaries), in-memory lifecycle across three immutable
versioned record types:

1. **`ForecastVintageRecord` (`ForecastLedger::issue_forecast`):**
   - Freezes `forecast_id`, `version_id`, `schema_version`, `actor`,
     `issued_at_utc_ms`, `origin_utc_ms`, `information_cutoff_utc_ms`, half-open
     target horizon `[horizon_start_utc_ms, horizon_end_utc_ms)`, `target_id`,
     `OutcomeDefinition` (`outcome_id`, `proposition`, `resolution_rules`),
     `baseline_version`, `baseline_probability`, `forecast_probability`,
     `model_version`, `sources`, `comparison_set`, and `assumptions`.
   - Enforces valid UTC millisecond epoch bounds
     (`0001-01-01T00:00:00.000Z`..=`9999-12-31T23:59:59.999Z`),
     `information_cutoff_utc_ms <= origin_utc_ms <= issued_at_utc_ms < horizon_end_utc_ms`,
     and `horizon_start_utc_ms < horizon_end_utc_ms`.
   - Binds each `outcome_id` to its first registered `OutcomeDefinition`,
     `target_id`, and `[horizon_start_utc_ms, horizon_end_utc_ms)` across the
     ledger so another forecast series cannot redefine the same `outcome_id`
     with a conflicting proposition, target, or horizon.
   - Enforces point-in-time cutoff admissibility (`CutoffAdmissibilityMode`):
     - `ProspectiveStrict` requires every cited source version to have both a
       known `public_available_at_utc_ms <= information_cutoff_utc_ms` and a
       known OIA `ingested_at_utc_ms <= information_cutoff_utc_ms`, and requires
       a known `model_knowledge_cutoff_utc_ms <= information_cutoff_utc_ms`.
     - `RetrospectiveReconstructed { limitation_note }` requires a non-empty
       `limitation_note` and known `public_available_at_utc_ms <= information_cutoff_utc_ms`,
       allowing historical records ingested into OIA after the cutoff while
       recording `post_cutoff_model_knowledge_disclosed = true` when a model's
       knowledge cutoff is unknown (`None`) or exceeds `information_cutoff_utc_ms`.
       Unknown public availability (`None`) is rejected in both modes.
   - Enforces linked revision preservation: a revision must reference the latest
     existing vintage of `forecast_id` via `predecessor_version`, preserve the
     frozen `target_id`, `outcome_definition`, `[horizon_start_utc_ms, horizon_end_utc_ms)`,
     `baseline_version`, and `baseline_probability`, never downgrade
     `ProspectiveStrict` to `RetrospectiveReconstructed`, advance timestamps
     monotonically, and supply a `RevisionRationale` whose
     `new_evidence_versions` are present in the revision's admissible `sources`
     and were not already cited in the predecessor vintage. Original vintages
     remain intact and queryable.

2. **`OutcomeResolutionRecord` (`ForecastLedger::record_outcome_resolution`):**
   - Requires `outcome_id` to be registered by an issued forecast and keeps
     `ResolutionState::Resolved { occurred, observed_event_utc_ms }`,
     `ResolutionState::Unresolved(Pending | Censored | Disputed)`, and
     `ResolutionState::Invalidated { reason }` distinct.
   - Enforces horizon-consistent resolution states:
     - `occurred: true` requires `observed_event_utc_ms: Some(t)` inside the
       outcome's frozen horizon `[horizon_start_utc_ms, horizon_end_utc_ms)`.
     - `occurred: false` requires `recorded_at_utc_ms >= horizon_end_utc_ms`
       (an open window must remain `Unresolved(Pending)` until it closes) and
       allows `observed_event_utc_ms` to be `None` (never occurred) or `Some(t)`
       outside `[horizon_start_utc_ms, horizon_end_utc_ms)` (an early or late
       event outside the target window).
   - Subsequent corrections to an outcome resolution require matching
     `resolution_id`, `supersedes_version`, and `correction_rationale`.
     Recording a superseding resolution preserves all prior resolution versions
     and transitively marks any existing `EvaluationRunRecord` that pinned any
     earlier resolution version of that outcome with `needs_reassessment = true`
     and a `ReassessmentReason::OutcomeResolutionSuperseded` entry without
     editing historical evaluation scores in place.

3. **`EvaluationRunRecord` (`ForecastLedger::evaluate_cohort`):**
   - Evaluates a caller-selected cohort of `(forecast_version, resolution_version)`
     pairs. While the caller selects which forecast series enter the cohort, any
     included revision requires all earlier vintages (`1..vintage_number`) of
     that series to be included in the cohort so unsuccessful originals cannot
     be omitted.
   - Verifies outcome-definition alignment, that each resolution does not
     predate forecast issuance, that `evaluated_at_utc_ms` does not predate any
     input forecast or resolution, and that no observed event occurred at or
     before a forecast vintage's `information_cutoff_utc_ms`, `origin_utc_ms`,
     or `issued_at_utc_ms`.
   - Delegates `Resolved` and `Unresolved` vintages to
     `oia_forecast::scoring::evaluate_binary` while accounting separately for
     `Invalidated` vintages (`invalidated_count`) so invalidated preconditions
     never masquerade as `occurred: false` or silently disappear.
   - Computes `HorizonTiming` (`WithinHorizon` for `occurred: true`, and
     `Early { early_by_ms }` or `Late { late_by_ms }` for `occurred: false` with
     an out-of-window `observed_event_utc_ms`).

## Limitations and remaining `#43` scope

- **In-memory kernel only:** `ForecastLedger` does not persist records to
  SQLite (`oia-store`) or wire into shared application commands/events
  (`oia-app`).
- **Caller-supplied timestamps and baseline probabilities:** The kernel validates
  timestamp ordering and cutoff admissibility against the supplied
  `CutoffSourceInput` metadata; it does not fetch upstream source records from a
  storage engine or compute fitted base-rate/calibration curves.
- **Open decisions and acceptance:** `D12` (selection of initial production
  targets/horizons, baselines, point-in-time datasets, calibration, and
  resolution methods), interval/quantile scoring, and end-to-end `A14`/`A15`
  acceptance across live or stored market and analogue corpora remain open in
  `#43`.
- **No formal proof claim:** No Lean theorem or Rust-to-Lean correspondence is
  claimed for this module.

## Local verification

Run with pinned Rust `1.99.0`:

```text
cargo +1.99.0 run -p oia-forecast --example inspect-a15-forecast-lifecycle --locked --offline
cargo +1.99.0 fmt --all -- --check
cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings
cargo +1.99.0 test --workspace --locked --offline
```
