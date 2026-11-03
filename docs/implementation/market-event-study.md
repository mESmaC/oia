# Daily market event studies

This implements the dependency-independent event-study calculation slice of
[#42](https://github.com/mESmaC/oia/issues/42). It covers the event-study part of
`MARKET-02`: inspectable benchmarks, windows, return definitions, and sample
choices. It also covers the benchmark-calculation and strict-cutoff subset of
[A14](../delivery.md#a14-market-analysis-and-point-in-time-prediction). The
standard-library-only `oia_market::event_study` module builds on the merged
[point-in-time normalization kernel](market-point-in-time.md). It has no network,
storage, UI, or forecast dependency. The
[synthetic example](../../crates/oia-market/examples/inspect-event-study.rs) is not
observed market evidence or a provider integration.

**Provisional pending #12:** dates and `*_utc_ms` instants use the crate's local
Unix-epoch UTC millisecond conventions.

## Anchor and session offsets

`MarketAwarenessAnchor` cites the record whose documented public availability
defines market awareness: the event ID, the record and source versions, and the
`RecordAvailability`. The anchor goes through the same cutoff admissibility as quotes.
Unknown public availability, publication after the cutoff, or, in
`ProspectiveStrict` mode, unknown or late OIA ingestion rejects the study with
`AnchorExcluded`. The event's historical occurrence time is retained for
inspection and never places day 0. Public availability is the awareness proxy in
this slice; a separately evidenced market-awareness time needs its own contract.

Day 0 is the first listed session whose close is strictly after awareness.
Awareness inside a session makes that session day 0 and adds
`AwarenessWithinDayZeroSession`, because its return includes earlier trading.
Awareness at or after a close moves day 0 to the next session. Offsets count
listed trading sessions in the pinned `TradingCalendarVersion`, so weekends and
holidays are not sessions. Every civil date between consecutive sessions in the
study span must be a recorded closure. A date with no entry could be an unlisted
session that would shift every offset, so it returns
`CalendarCoverageUnknown(date)` instead of being guessed. The checked span always
includes days −1 and 0. A span that runs past the calendar's listed sessions returns
`StudySpanBeyondCalendar`. Session local dates must follow UTC session order.

`SessionWindow` is an inclusive offset range bounded by `±10,000` sessions. The
estimation window must end before the event window starts. The event window may
include negative offsets for pre-awareness drift.

## Closes, adjustment, and returns

Each `PriceSeries` is one exact instrument/venue/source/unit scope. The whole
series is validated through `select_quote_as_of`: scope, timestamps, duplicate
versions, and tie rules. Corporate actions are validated before closes are chosen.
Only `Frequency::Daily` quotes are accepted; intraday studies need a separate
contract. A quote belongs to a session when observed within `[open, close]`.
The close is inclusive because closing prints occur at the close instant. A quote
on a boundary shared by two adjacent sessions is rejected as ambiguous. Quotes
outside every study session are counted and not used.

Per session, the latest admissible quote is selected with the existing
`select_quote_as_of` rules at the single analysis cutoff. Exclusions are retained
per version. A session with no daily quote, or with only inadmissible quotes, is
`Missing`. It is never interpolated, held forward, or replaced by another session.
Selected closes are put on the requested basis with `adjust_quote`.

- `SplitAdjusted` expresses every close on the post-split share basis, so a split
  inside the window does not appear as a return. In the fixture, the 2-for-1 split
  on Jan 16 yields an exact `1/55` return, not −49%.
- `Unadjusted` is rejected when an admissible split takes effect between the first
  and last present close, because returns across it would be spurious.
- `TotalReturn` remains unsupported.

An action that is excluded for availability but takes effect inside the span is
listed in `inadmissible_actions_within_span` and qualified as
`InadmissibleActionWithinSpan`. Returns across it may reflect a share-basis change
that was not public at the cutoff.

Returns use `simple-close-to-close-exact-rational-v1`: `P_t / P_{t−1} − 1`
between consecutive listed sessions, computed exactly as `ExactRational`. A return
whose bounding close is missing is absent and records each missing close as a
`ReturnGap`. No multi-session return is substituted. Simple returns require
strictly positive prices, so negative-price instruments and yield levels are
rejected.

## Benchmarks and statistics

Aggregation uses `f64-two-pass-session-order-v1`: exact returns are converted to
IEEE-754 doubles (numerator and denominator rounded separately, then divided), and
means and sums of squares are computed in two passes in offset order. Exact
rational aggregation overflows checked `i128` within a few terms, so it is not
offered. Every aggregate is checked for finiteness (`NonFiniteResult`).

| Model | Method ID | Expected return | Residual dispersion |
| --- | --- | --- | --- |
| `MeanAdjusted` | `mean-adjusted-f64-v1` | Mean estimation-window subject return | Sample SD, `n − 1` |
| `MarketAdjusted` | `market-adjusted-f64-v1` | Same-session benchmark return | SD of `R − R_m` about its mean, `n − 1` |
| `MarketModelOls` | `market-model-ols-f64-v1` | `α + β·R_m` from OLS | Residual SD, `n − 2` |

Benchmark models require a distinct benchmark instrument. A supplied benchmark is
rejected for `MeanAdjusted` rather than silently ignored. The subject and any
benchmark must share the calendar's venue; otherwise the request returns
`ScopeMismatch` before any calculation. Benchmark closes are therefore aligned to
the same session bounds. Cross-calendar synchronization, and a separate venue ID
for an index computed on the subject's calendar, need an explicit mapping contract. An estimation observation requires every return the model uses.
The request declares `min_estimation_returns`. It must leave at least one residual
degree of freedom and cannot exceed the estimation window. Fewer usable returns
yield `EstimationOutcome::Insufficient` with the available and required counts.
Zero benchmark variance yields `DegenerateBenchmarkVariance`.

Abnormal returns are actual minus expected returns for each event-window session.
`CarOutcome` is one of three outcomes:

- `Complete`: the sum of event-window abnormal returns, with the standardized
  value `CAR / (σ·√L)` under `car-over-sigma-sqrt-l-iid-v1`. That statistic
  assumes i.i.d. abnormal returns and ignores market-model estimation error. It
  is not a calibrated probability or p-value. When `σ = 0`, the standardized
  value is absent and `ZeroResidualDispersion` is added.
- `MissingEventReturns`: lists the offsets without an abnormal return. No CAR is
  reported for a partial window.
- `NotEstimated`: the model was not fitted.

Every result reports `Interpretation::StatisticalAssociationOnly`. An abnormal
return around an anchored information instant does not establish that the event
caused it, and real-world effects need not produce incremental price effects.

## Provenance in results

`EventStudyResult` retains the event, awareness record and source versions,
awareness and occurrence instants, day-0 date, calendar and calendar-source
versions, model, basis, windows, minimum sample, cutoff, and full cutoff mode. It
also retains the method IDs for returns, model, aggregation, standardization, and
price adjustment. For each series, it keeps every span session's close status,
including the selected quote and source version, the exact adjusted price, and
the applied action versions. It also keeps per-version quote exclusions,
quote-independent action exclusions, and the count of quotes outside the span.
Estimation-window returns, including gaps, are returned alongside event-window
returns, expected returns, and abnormal returns.

## Bounds and cancellation

All supplied quotes and actions, across both series, count against one
`MarketLimits` budget, as does the number of span sessions. Oversized inputs are
rejected. Result vectors use fallible reservations, and offsets and indexes use
checked arithmetic. The cooperative cancellation or deadline callback is checked
on entry, per quote and session, inside the reused selection and adjustment calls,
and before returning. A stop returns no result. The function performs no I/O.

## Local use

```text
cargo +1.99.0 run -p oia-market --example inspect-event-study --locked --offline
```

The example builds only synthetic in-memory data: a two-week calendar with
weekends and a holiday, a subject with a 2-for-1 split, an index benchmark, and an
event that occurs during one session but is published after its close. It prints
day 0, the method IDs, every split-adjusted close with applied actions, the
fitted market model, exact event-window returns, abnormal returns, CAR, the
standardized CAR, exclusions, qualifications, and the interpretation.

## Integration and remaining scope

This is a transient calculation kernel. Charts, watchlists, alerts, event
overlays, exposure graphs with dated citations, historical comparisons, cited
briefs, multi-event aggregation, persistence, application-service authorization,
coverage/freshness envelopes, UI, and integrated A14 acceptance remain open on #42,
pending #20, #36, #16, and the P0 runtime packages. These also need separate
contracts:

- log or total returns, dividends, and FX conversion
- intraday and cross-calendar studies
- prediction-error-corrected or non-parametric test statistics
- other market-awareness evidence

D11 and D12 remain open. No provider access, historical coverage, source
authenticity, forecast skill, or causal effect is claimed. No unmerged
prerequisite code is copied, and no Lean proof or Rust-to-Lean correspondence
claim applies.

## Verification

The [two focused contract tests](../../crates/oia-market/tests/event_study_contract.rs)
compare the market-model, mean-adjusted, and market-adjusted results with
reference values computed independently using exact `fractions.Fraction` returns
and IEEE-754 double aggregation in the same order. The tolerance is `1e-12` for
returns and CAR, and `1e-9` for standardized CAR. They also check:

- exact returns across a split and a holiday
- rejection of an unadjusted basis across a split
- day 0 after a post-close publication and qualification for intraday awareness
- a post-cutoff close kept as an explicit gap with a partial CAR
- an insufficient estimation sample after a removed close
- the not-yet-public split qualification
- anchor exclusion after the cutoff
- unknown calendar coverage, a span beyond the calendar, rejection of a calendar
  or benchmark on another venue, rejection of an unused benchmark, and cancellation

The fixtures are synthetic and do not establish market-data truth.

On 2026-11-03 UTC, local Windows x86_64 MSVC verification with pinned Rust 1.99.0
passed:

- `cargo +1.99.0 fmt --all -- --check`
- `cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings`
- `cargo +1.99.0 test -p oia-market --locked --offline`: the two new tests and
  the three existing tests
- the example command above

The example placed day 0 on 2024-01-12 for awareness after the Jan 11 close. It
reported the Jan 16 return as exactly `1/55` and produced a CAR of `0.061116`
over offsets −1..=2. Other operating systems and live providers were not
exercised. No hosted CI jobs were added or triggered. The delivery PR records the
workspace test result and the final checked commit.
