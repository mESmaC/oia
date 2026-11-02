# Scalar forecast interval scoring

Implemented numerical slice of [#43](https://github.com/mESmaC/oia/issues/43)
for FORECAST-04 and MARKET-05 in [the specification](../spec.md). The entry point
is [`oia_forecast::interval::evaluate_intervals`](../../crates/oia-forecast/src/interval.rs).
It complements [binary scoring](forecast-scoring.md); the existing
[forecast lifecycle](forecast-issuance.md) still issues binary forecasts only.

## Inputs and preserved evidence

Each `IntervalForecastRow` supplies an immutable forecast version, a central
prediction interval, a pinned baseline version and interval, a declared unit ID,
and either a finite scalar value with its resolution version or an explicit
pending, censored, or disputed outcome. The unit applies to both intervals and
the scalar outcome. IDs refer to upstream definitions; the kernel does not
resolve them or convert units.

`PredictionInterval::new` validates finite ordered bounds, finite width, and
nominal coverage strictly between 0 and 1. An interval at coverage `c` represents
lower and upper quantiles at `(1-c)/2` and `1-(1-c)/2`. Equal bounds are allowed;
both endpoints count as covered. This is a scalar prediction interval, not a
confidence interval for an estimated model parameter.

The cohort and every matched baseline must declare exactly the same unit and
nominal coverage, including unresolved rows. Equality of coverage is exact, with
no tolerance, rounding, or substitution. Different units or levels require
separate evaluations. Upstream code must establish compatible targets, horizons,
unit definitions, provenance, source availability and authorization. A matching
unit ID alone does not establish these properties or make distinct targets
meaningfully comparable.

Every supplied row is retained in its original order. No latest-vintage
selection, sampling, fitting, imputation or automatic cohort selection occurs.
The kernel rejects duplicate forecast version IDs but permits distinct vintages
of one event. It cannot detect omitted historical vintages or validate that an
ID refers to a real issued forecast. Corrected resolutions require new input
rows and a new evaluation; a borrowed prior evaluation continues to identify its
original inputs and scores. Callers own retention; nothing is persisted here.

## Calculation and coverage

For lower bound `l`, upper bound `u`, outcome `y`, and `alpha = 1-c`, the central
interval score is:

```text
width = u - l
score = width                         when l <= y <= u
score = width + 2 * (l - y) / alpha    when y < l
score = width + 2 * (y - u) / alpha    when y > u
```

Lower scores are better. Width and score retain the input unit. Results expose
each forecast and baseline score, width and coverage flag, plus their separate
means and empirical coverage over resolved vintages. Each resolved vintage has
equal weight; losing originals are included when supplied. Unresolved rows have
no score, remain counted, and retain their reasons in the returned input rows.
Empty or wholly unresolved cohorts have no means. Empty input additionally has
no inferred unit or nominal coverage.

For a synthetic 50% interval `[2, 4]` and outcome `1`, the score is
`2 + 2 * 1 / 0.5 = 6`. A revised `[1, 3]` scores `2`. Baseline `[1, 5]` scores
`4` for both. The forecast mean is `4` and empirical coverage is `0.5`; baseline
mean is `4` and coverage is `1`. These two vintages share an outcome and are not
independent events. Agreement between nominal and empirical coverage on such a
fixture does not establish calibration or predictive skill.

Requests require an explicit row budget in `1..=100,000`. Excess rows, duplicate
versions, mixed units/levels, non-finite resolutions and unrepresentable scores
cause typed errors for the entire request, without partial publication. The
budget includes unresolved rows. Computation is O(n log n) time and O(n) auxiliary
memory. Nonnegative running means avoid overflowing a sum when individual scores
and their mean are representable. Ordinary `f64` rounding and underflow remain;
no cross-platform bitwise identity, arbitrary-precision arithmetic, or accuracy
beyond the supplied scalar values is claimed.

## Run and verification

The runnable example uses simulated inputs and labels them explicitly:

```text
cargo +1.99.0 run -p oia-forecast --example inspect-forecast-intervals --locked --offline
```

On Windows x86_64 MSVC with Rust 1.99.0, 2026-11-02, the example and two focused
[`interval_contract`](../../crates/oia-forecast/tests/interval_contract.rs)
tests passed, as did these workspace checks:

```text
cargo +1.99.0 fmt --all -- --check
cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings
cargo +1.99.0 test --workspace --locked --offline
```

No hosted CI was triggered. The tests address numerical correctness, preservation of losing
and unresolved vintages, pinned correction inputs, incompatible cohort inputs,
request limits, non-finite values, and representable versus overflowing scores.
Hand-calculated binary-fraction fixtures use exact comparisons. They exercise
numerical subsets of [A14](../delivery.md#a14-market-analysis-and-point-in-time-prediction)
and [A15](../delivery.md#a15-historical-analogue-forecasting), not the complete
acceptance scenarios.

## Remaining scope

Scalar issuance and resolution histories, chronological datasets, calibration
fitting, timing-error scoring, durable storage, application services, and full
A14/A15 acceptance remain open. The binary ledger does not yet accept these
scalar intervals or propagate reassessment to their evaluations. An invalidated
scalar forecast needs a future lifecycle contract; it must not be converted to a
resolved value. No shared P0 implementation was copied from unmerged branches.
D12 production targets, datasets, baselines and calibration choices remain open.
This change makes no Lean proof or Rust correspondence claim.
