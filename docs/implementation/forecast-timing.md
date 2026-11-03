# Forecast event timing evaluation

Implemented numerical slice of [#43](https://github.com/mESmaC/oia/issues/43),
FORECAST-04 and [A15](../delivery.md#a15-historical-analogue-forecasting):
[`evaluate_timing`](../../crates/oia-forecast/src/timing.rs) compares point-event
predictions with a matched baseline and a version-pinned exact event time.

Inputs use normalized Unix milliseconds (`i64`, including pre-epoch values).
The caller must establish compatible event definitions/windows, exact source
precision, common time reference, cutoff admissibility, access and version
existence. Date-only, approximate, unknown or disputed times must not be turned
into exact instants to obtain a score. This kernel performs no calendar,
timezone, leap-second or scientific time-scale conversion and adds no competing
shared time primitive. D12 production targets and baselines remain undecided.

For each occurred event, signed error is `prediction - observation`; negative
means early and positive means late. Absolute error is its magnitude. Both
forecast and baseline errors are retained. Widening to `i128` before subtraction
supports the entire `i64` timestamp domain without saturation; maximum absolute
error is `2^64 - 1` milliseconds. Equal-weight means use exact integer sums before
one `f64` conversion and division. Row errors are exact; means can round. The
100,000-row ceiling keeps all sums within `i128`/`u128`.

Every supplied vintage remains in original order with its forecast, baseline,
and outcome version references. No latest-vintage selection occurs. Losing
originals and their revisions are separate rows and may be dependent. Pending,
censored, disputed, known non-occurrence and invalidated rows have no timing
score. Counts distinguish occurred, known non-occurrence, unresolved and
invalidated outcomes; the individual rows preserve unresolved reasons. Empty
and wholly unscored cohorts have no means. Non-occurrence is not an unknown
outcome or a zero error. Timing means condition on occurrence and must be read
alongside this coverage, never as overall event prediction accuracy.

Corrected resolutions require new inputs and a new evaluation. Prior evaluations
borrow the original rows; the caller owns persistence and retention. Inputs
must include the intended full cohort; this evaluator cannot detect omitted
failures, dependent events, or leakage. Occurrence scoring remains the separate
[binary evaluator](forecast-scoring.md). No survival analysis, interval-censored
timing, lifecycle integration, calibration or predictive skill is claimed.

Budgets must lie in `1..=100,000` and include unscored rows. Excess rows and
duplicate forecast versions reject the entire request through typed
`ScoringError` values. There is no truncation or partial publication. Runtime
is O(n log n), with O(n) auxiliary memory and no network, file or device effects.

## Local verification

The runnable example labels its inputs as simulated:

```text
cargo +1.99.0 run -p oia-forecast --example inspect-forecast-timing --locked --offline
```

It includes a losing original (4,000 ms early), a revision (1,000 ms late), a
baseline (2,000 ms late), censored follow-up and known non-occurrence. Forecast
mean signed error is -1,500 ms and mean absolute error is 2,500 ms. These are
hand-derived fixture values, not measured forecast performance.

Two targeted [contract tests](../../crates/oia-forecast/tests/timing_contract.rs)
address evidence/version preservation, unknown versus absent outcomes, exact
full-domain subtraction, correction retention, and request-bound enforcement.
The PR records executed checks and environment. Full A14/A15, durable storage,
shared application services, scalar issuance and scientific time conversion
remain open. No Lean theorem or implementation correspondence claim is added.
