# Binary forecast scoring

Implemented slice of [#43](https://github.com/mESmaC/oia/issues/43), supporting
FORECAST-04 and MARKET-05 in [the specification](../spec.md). The Rust library
entry point is `oia_forecast::scoring::evaluate_binary` in
[`scoring.rs`](../../crates/oia-forecast/src/scoring.rs).

## Contract

Supply a slice of `BinaryForecastRow` values and an explicit row limit. Each row
references a forecast version, a baseline version, their validated binary-event
probabilities, and either a resolution version with a Boolean outcome or an
explicit pending, censored, or disputed outcome. `Probability::new` refuses
non-finite values and values outside 0 through 1, without clamping.

The evaluator returns every row in input order with its version references.
For resolved rows, the forecast and baseline Brier scores are `(p - y)^2`, where
`y` is 1 for occurrence and 0 for non-occurrence. Lower scores are better. Means
use equal weight per resolved forecast vintage; unresolved rows remain visible
and counted but have no score. Empty or wholly unresolved cohorts have no mean.
For example, an original probability of 0 and a revision of 1 for an event that
occurred yield scores of 1 and 0 respectively; both remain in the result. A
baseline of 0.5 yields 0.25 for each. The two-vintage means are 0.5 and 0.25.
These are synthetic arithmetic examples, not observed predictive performance.

The caller limit must be between 1 and 100,000 rows. Excess rows and duplicate
forecast version IDs cause typed errors for the entire request. There is no
truncation, latest-version filtering, fitting, sampling, network access or
mutation. Computation uses bounded O(n log n) time and O(n) auxiliary memory.
Returned rows borrow the inputs; the caller owns retention of evaluation inputs
and results. This is not a durable forecast ledger.

## Integration limits and open decisions

The caller selects the cohort and supplies compatible target/horizon and baseline
records. This module cannot establish whether those records exist or whether
probabilities agree with them. Issuance authorization, original preservation,
point-in-time cutoffs, chronological evaluation, outcome independence, evidence
coverage and resolution corrections must be enforced by future shared services
and versioned records (#10, #12, #14 and #40). No unmerged code from those packages
is copied here. A correction should produce a new evaluation referencing its new
resolution version while retaining the previous evaluation.

Multiple vintages of one event can be dependent. Descriptive row means do not
establish independent-event performance, calibration, statistical significance,
or superiority to a fitted baseline. There is no baseline training, probability
calibration, timing-error evaluation, interval scoring, persistent issuance,
serialization, UI or market integration. D12 target/horizon/dataset and baseline
selection remains open. No Lean theorem or Rust correspondence claim is made.

## Local verification

Executed on Windows x86_64 MSVC with pinned Rust 1.99.0 on 2026-11-02:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`
- `cargo test --workspace --locked`

All passed. Two targeted tests address concrete risks: loss of unsuccessful
original vintages or unresolved coverage during evaluation, and numeric/budget/
duplicate inputs silently distorting scores. Fixed, exactly representable binary
fractions exercise known positive and negative outcomes. General `f64` sums have
ordinary rounding error; no cross-platform bitwise identity is claimed. This is
only numerical fixture coverage toward [A15](../delivery.md#a15-historical-analogue-forecasting);
the end-to-end A14/A15 acceptance scenarios have not been executed.

## Tracking handoff

Owner: Codex forecast-scoring session, branch `codex/forecast-scoring`, worktree
`C:/Users/Cameron/Desktop/int/oia-forecast-scoring`. The
[verified claim](https://github.com/mESmaC/oia/issues/43#issuecomment-6586165924)
covers only this scoring slice. Intended Project status is In Progress; Project
reads and updates failed because the GitHub GraphQL quota was exhausted. Reconcile
that status when access returns. Issue #43 remains open for the integration and
evaluation work above; this slice is delivered for review, not marked Done.
