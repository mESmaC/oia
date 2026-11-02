# Immutable forecast issuance, cutoff admissibility, and resolution lifecycle

Implemented slice of [#43](https://github.com/mESmaC/oia/issues/43), supporting
`FORECAST-04`, `MARKET-04`, and `MARKET-05` in [the specification](../spec.md)
and Sections 2.2, 3.1, and 6.4 of [the architecture](../architecture.md). The
Rust library entry points live in [`oia_forecast::issuance`](../../crates/oia-forecast/src/issuance.rs)
and compose with [`oia_forecast::scoring::evaluate_binary`](../../crates/oia-forecast/src/scoring.rs).

## Contract

`ForecastLedger` enforces a bounded (`1..=10,000` records per family), in-memory
lifecycle across three immutable versioned record types:

1. **`ForecastVintageRecord` (`ForecastLedger::issue_forecast`):**
   - Freezes `forecast_id`, `version_id`, `schema_version`, `actor`,
     `issued_at_utc_ms`, `origin_utc_ms`, `information_cutoff_utc_ms`, half-open
     target horizon `[horizon_start_utc_ms, horizon_end_utc_ms)`, `target_id`,
     `OutcomeDefinition` (`outcome_id`, `proposition`, `resolution_rules`),
     `baseline_version`, `baseline_probability`, `forecast_probability`,
     `model_version`, `sources`, `comparison_set`, and `assumptions`.
   - Enforces `information_cutoff_utc_ms <= origin_utc_ms <= issued_at_utc_ms`,
     `horizon_start_utc_ms < horizon_end_utc_ms`, and
     `origin_utc_ms < horizon_end_utc_ms`.
   - Enforces point-in-time cutoff admissibility (`CutoffAdmissibilityMode`):
     - `ProspectiveStrict` requires every cited source version to have both a
       known `public_available_at_utc_ms <= information_cutoff_utc_ms` and a
       known OIA `ingested_at_utc_ms <= information_cutoff_utc_ms`, and rejects
       any model with `model_knowledge_cutoff_utc_ms > information_cutoff_utc_ms`.
     - `RetrospectiveReconstructed { limitation_note }` requires a non-empty
       `limitation_note` and known `public_available_at_utc_ms <= information_cutoff_utc_ms`,
       allowing historical records ingested into OIA after the cutoff while
       recording `post_cutoff_model_knowledge_disclosed = true` when a model's
       knowledge cutoff exceeds `information_cutoff_utc_ms`. Unknown public
       availability (`None`) is rejected in both modes.
   - Enforces linked revision preservation: a revision must reference the latest
     existing vintage of `forecast_id` via `predecessor_version`, preserve the
     frozen `target_id`, `outcome_definition`, and `[horizon_start_utc_ms, horizon_end_utc_ms)`,
     advance timestamps monotonically, and supply a `RevisionRationale` whose
     `new_evidence_versions` are present in the revision's admissible `sources`.
     Original vintages remain intact and queryable.

2. **`OutcomeResolutionRecord` (`ForecastLedger::record_outcome_resolution`):**
   - Keeps `ResolutionState::Resolved { occurred, observed_event_utc_ms }`,
     `ResolutionState::Unresolved(Pending | Censored | Disputed)`, and
     `ResolutionState::Invalidated { reason }` distinct.
   - Requires supporting `evidence_versions` for `Resolved`, `Disputed`, and
     `Invalidated` states, and forbids attaching an `observed_event_utc_ms`
     instant to a confirmed non-occurrence (`occurred: false`).
   - Subsequent corrections to an outcome resolution require `supersedes_version`
     and `correction_rationale`. Recording a superseding resolution preserves
     all prior resolution versions and marks any existing `EvaluationRunRecord`
     that pinned the superseded resolution version with
     `needs_reassessment = true` and a `ReassessmentReason::OutcomeResolutionSuperseded`
     entry without editing historical evaluation scores in place.

3. **`EvaluationRunRecord` (`ForecastLedger::evaluate_cohort`):**
   - Evaluates an explicit cohort of `(forecast_version, resolution_version)`
     pairs, verifying outcome-definition alignment and that each resolution does
     not predate forecast issuance.
   - Delegates `Resolved` and `Unresolved` vintages to
     `oia_forecast::scoring::evaluate_binary` while accounting separately for
     `Invalidated` vintages (`invalidated_count`) so invalidated preconditions
     never masquerade as `occurred: false` or silently disappear.
   - Computes `HorizonTiming` (`WithinHorizon`, `Early { early_by_ms }`,
     `Late { late_by_ms }`) for resolved occurrences with an observed event
     timestamp.

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
