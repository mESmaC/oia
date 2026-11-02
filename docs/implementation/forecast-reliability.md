# Forecast probability reliability reports

Implemented slice of [#43](https://github.com/mESmaC/oia/issues/43) for
`FORECAST-04` and `MARKET-05`, covering the descriptive calibration-reporting
portion of [A15](../delivery.md#a15-historical-analogue-forecasting).

[`ForecastLedger::reliability_report`](../../crates/oia-forecast/src/reliability.rs)
reads an existing immutable evaluation by version. It returns separate forecast
and baseline reliability tables, the exact evaluation reference, and every
vintage in its original order with its assigned bin indices. It uses the merged
[issuance and resolution lifecycle](forecast-issuance.md); no unmerged domain,
storage, or time code is copied.

## Contract and method

- The caller explicitly chooses 1–100 equal-width bins and a row budget in
  1–100,000. The ledger currently imposes its tighter cumulative limits on stored
  evaluations. Oversized requests fail as a whole before report allocation;
  allocation failures are typed errors. Work is bounded by the row count times
  the bin count, with storage proportional to rows plus bins. There is no I/O.
- Edges are `i / bin_count` in binary64 arithmetic. Bins are `[lower, upper)`,
  with the final bin including 1. Assignment compares probabilities directly to
  the displayed edges. It does not multiply a probability and round an index,
  which could disagree with a displayed floating-point boundary.
- Each bin counts resolved, pending, censored, disputed, and invalidated vintages
  separately. A probability is assigned even when its outcome is unscored.
  Unresolved and invalidated outcomes never become negative observations.
- For resolved rows in a bin, mean probability is `sum(p) / n`; observed
  frequency is `count(occurred) / n`. Both use the same denominator and equal
  vintage weights. Forecast and baseline probabilities are binned independently.
  Empty bins and bins containing only unscored outcomes have `statistics: None`.
- Every original and revised vintage remains present. Resolution corrections
  preserve the old frequencies and expose `needs_reassessment` and its reasons
  through the source evaluation. A new evaluation may use the corrected
  resolution; requesting a report never substitutes it for the pinned version.
- Each row retains forecast, baseline, and outcome-resolution version references,
  the original Brier scores, timing, and model-knowledge disclosure through its
  borrowed vintage. Reports cannot outlive their ledger. Re-read the report after
  a ledger mutation to obtain its current reassessment state.

These are descriptive statistics of the selected cohort, not a fitted calibrator
or evidence that probabilities are calibrated. Revisions may concern the same
event; vintage counts are not independent-event counts. Cohort selection,
unresolved follow-up, small samples, and bin choice can materially change the
apparent relationship. No significance tests, confidence intervals, or automatic
calibrated/uncalibrated label are generated. Probability sums use binary64
arithmetic; numerical fixture assertions allow a `1e-15` absolute tolerance where
addition rounds. There is no Lean proof or correspondence claim.

## Local execution and verification

The existing lifecycle example now prints both the superseded pending evaluation
and its resolved successor, including coverage, all bins, and reassessment status:

```text
cargo run -p oia-forecast --example inspect-a15-forecast-lifecycle --locked --offline
cargo test -p oia-forecast --test reliability_contract --locked --offline
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo test --workspace --locked --offline
```

The two targeted contract tests protect numerical bin boundaries and matched
denominators, explicit missing-outcome coverage, failed-original preservation,
correction provenance, and whole-request budget refusal. Inputs are synthetic;
these tests do not measure forecasting skill or complete A14/A15 integration.

All five commands above passed locally on 2026-11-02 with Rust 1.99.0
(`b940084d7`), Windows x86_64 MSVC, on this change based on `e1d171c`.
The focused suite contains two passing tests. The example executed against
synthetic inputs; no hosted CI, live provider, or production model was used.

## Remaining scope

Issue #43 remains open for D12 production targets, datasets, baseline selection
and calibration methods; calibrator fitting and held-out evaluation; interval
scoring; durable persistence and shared services; and full A14/A15 acceptance.
This reporting API makes no production source/model choice and adds no dependency.
