# Point-in-time market data normalization

This implements the dependency-independent normalization and selection slice of
[#41](https://github.com/mESmaC/oia/issues/41), `MARKET-01`, and the data-cutoff
portions of `MARKET-04`/`MARKET-05` and
[A14](../delivery.md#a14-market-analysis-and-point-in-time-prediction). The
standard-library-only [`oia-market`](../../crates/oia-market/) crate uses existing
`oia-domain` identifiers. It has no network, storage, UI, or forecast-lifecycle
dependency. The [synthetic example](../../crates/oia-market/examples/inspect-a14-point-in-time.rs)
is not observed market evidence or a provider integration.

## Identity, dates, and calendars

`Instrument` carries a stable `OpaqueId`, immutable instrument version, class,
venue, explicit quote unit, and dated `ListingAlias` records. Classes cover
equities, indices, funds, bonds/yields, commodities, currencies, and digital
assets. Alias validity is half-open by civil date and source-versioned.
`resolve_instrument` resolves the symbol effective on a requested date at one
venue; a rename therefore retains the same instrument ID. It validates the whole
bounded input before returning a match, rejecting duplicate instrument/version
IDs, malformed or overlapping venue/symbol intervals, and ambiguous matches.

`CalendarDate` validates civil dates, including leap years, without converting a
date-only value into an observed UTC midnight. `DateInterval` is half-open.
`TradingCalendarVersion` pins venue, immutable and source versions, the original
timezone identifier, inclusive civil-date coverage, explicit UTC session bounds,
and closure reasons. Ordinary and early-close sessions are distinct, as are
holiday, weekend, and other closures. Session membership is `[open, close)`, so
the exact close is not open. Missing entries inside or outside coverage return
`Unknown`; they are not guessed to be open or observed closed. The kernel consumes
already-normalized UTC bounds and does not embed a timezone/DST database. It does
not apply exchange-session rules to off-hours or 24/7 quotes automatically.

## Exact values and quote cutoffs

`FixedDecimal` stores a checked signed coefficient and scale through 18 decimal
places. `ExactRational` preserves adjusted values without binary floating point
or undocumented rounding. Unsupported extrema and arithmetic overflow are errors.
`Unit` remains attached to prices and numeric releases; no implicit currency
conversion occurs.

`QuoteRecord` preserves stable instrument, venue, source, normalized and exact
source versions, observation instant, availability, exact price/unit, frequency,
and declared delay. Real-time, delayed, end-of-day, and unknown delay states are
metadata only: the implementation never derives public availability by adding a
delay. A known public-availability instant cannot precede the quote's exact
observation instant.

`RecordAvailability` separates first documented public availability from OIA
ingestion. `ProspectiveStrict` requires both to be known and at or before the
cutoff. `RetrospectiveReconstructed` requires a nonempty limitation note, still
requires known public availability at or before the cutoff, and permits later or
unknown OIA ingestion. Equality with the cutoff qualifies. Unknown public
availability never qualifies in either mode.

`select_quote_as_of` requires one exact instrument/venue/source/unit scope and
observation at or before the cutoff. It returns the selected immutable quote,
supplied/eligible counts, the cutoff instant, the complete cutoff mode, and
per-version exclusion reasons. Empty supplied input is distinguishable from input whose records were
all excluded. Selection is deterministic by observation time, public availability,
then lexical normalized version ID. Mixed scopes and duplicate versions reject
the request rather than silently merging providers or units.

## Release vintages and disclosures

`ReleaseVintage` links a stable series and source to normalized/source versions,
a nonzero consecutive revision sequence, predecessor version, reporting period,
optional historical event instant, availability, and payload. Numeric payloads
carry exact values and units. Disclosure/news payloads carry an exact document
version and bounded linked instrument/organization IDs; document bytes and
lineage remain owned by the document/evidence layers.

A revision series must retain source identity, reporting period, payload kind,
and numeric unit. Predecessors must form a consecutive explicit chain, and known
public availability cannot move backward across revisions. `select_release_vintage`
chooses the highest revision admissible at the cutoff rather than the newest
record currently supplied. The result retains the cutoff, mode, counts, and
exclusions. Earlier vintages remain selectable and inspectable when a later
revision is not yet public or fails prospective ingestion rules.

Historical event time never substitutes for public availability. News published
after its event and after the cutoff is excluded. A legitimately published
scheduled future announcement may instead have `event_at_utc_ms` after its
publication or the analysis cutoff; event time alone is not a leakage verdict.
The pinned source version and publication evidence determine admissibility.

## Split-only corporate-action adjustment

`adjust_quote` supports `Unadjusted` and `SplitAdjusted`; `TotalReturn` returns an
explicit unsupported error. Split ratios are positive exact integers. Only actions
for the same stable instrument that are effective after the quote observation,
effective at or before the cutoff, and admissible under the same cutoff mode are
applied. A 2-for-1 split multiplies a pre-effective price by `1/2`; an observation
at or after the effective instant is not adjusted again. Future, post-cutoff,
unknown-availability, and non-effective actions remain visible as exclusions.
Multiple split records at one instrument/effective instant are rejected as
ambiguous instead of being multiplied repeatedly.

Adjustment methods are pinned as `unadjusted-exact-rational-v1` and
`split-adjusted-exact-rational-v1`. Results preserve the original quote/version,
exact adjusted rational, basis, method, cutoff, complete mode/limitation note,
and applied action versions in chronological/version order. Original inputs are
not mutated. Dividend/total-return adjustment, FX conversion, and simultaneous
split identity/revision handling require separate contracts.

## Bounds and cancellation

`MarketLimits` accepts a nonzero per-operation record budget up to 10,000. Every
supplied record counts, including excluded records and unused action inputs.
Aliases and total calendar entries are bounded. Disclosure links are bounded per
record. Text metadata is limited to 4,096 UTF-8 bytes and units to 32 bytes.
Oversized inputs are rejected, never truncated. Result vectors use fallible
reservations; arithmetic and timestamp/date ranges are checked before use.

Alias resolution, quote/release selection, and adjustment accept a cooperative
cancellation/deadline callback and check it before work, per record/validation
phase, and before returning a result. A stop returns no result. Calendar
construction and lookups are bounded but do not run a callback. The application
service must still fence cancelled or superseded completions before persistence
or publication. These pure functions perform no I/O and implement no job queue.

## Local use

```text
cargo +1.99.0 run -p oia-market --example inspect-a14-point-in-time --locked --offline
```

The example constructs only synthetic in-memory data. It demonstrates stable
identity across a rename, a holiday and early close, delayed-quote exclusion and
selection, exact split adjustment, revised-release selection, and exclusion of
post-event news published after the cutoff. Output includes normalized versions,
exclusion reasons, adjustment provenance, and cutoff mode where applicable. It
reads/writes no files and contacts no provider.

## Integration and remaining scope

This is a transient normalization kernel, not a selected production market data
source. D11 instrument/provider/calendar/history/license choices and D12 forecast
dataset/method choices remain open. No provider access, historical coverage,
source authenticity, forecast skill, or full A14 acceptance is claimed.

Issue #41 remains open for licensed provider/adapters and acquisition, shared
#12 time primitives and #13 ingestion, document integration from #17, immutable
storage, application-service authorization, source coverage/freshness envelopes,
and integrated acceptance. Provider calendar generation, corporate-action
revision identity beyond the explicit ambiguity rejection, total-return methods,
and FX inputs need separately validated contracts. Charts, event studies,
exposure citations, forecast issuance/scoring, baselines, intervals, and UI belong
to later packages. No unmerged prerequisite code is copied, and no Lean proof or
Rust-to-Lean correspondence claim applies.

## Verification

The [three focused contract tests](../../crates/oia-market/tests/market_contract.rs)
address stable identity and calendar boundaries; delayed/unknown/late cutoff
exclusion; retrospective disclosure; revision-chain integrity; future-event news
semantics; original/version preservation; exact split arithmetic and future
leakage; malformed input, ambiguity, overflow, budget exhaustion, and entry/final
cancellation. They use synthetic values and do not establish provider access or
market-data truth.

On 2026-11-03 UTC, local Windows x86_64 MSVC verification with pinned Rust 1.99.0
passed `cargo +1.99.0 test -p oia-market --locked --offline` (three tests),
`cargo +1.99.0 clippy -p oia-market --all-targets --locked --offline -- -D warnings`,
and the synthetic example command above. The example resolved OLD/NEW to one ID,
returned `Unknown` for missing calendar coverage, excluded the delayed quote
before publication, adjusted `100/1` to `50/1` for the synthetic 2-for-1 split,
selected release v1 before v2 publication and v2 afterward, and excluded late
news. Other operating systems and live providers were not exercised.

The workspace baseline also passed at the same revision before delivery:
`cargo +1.99.0 fmt --all -- --check`,
`cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings`,
and `cargo +1.99.0 test --workspace --locked --offline`. No hosted CI jobs were
added or triggered. The delivery PR records the final checked commit.
