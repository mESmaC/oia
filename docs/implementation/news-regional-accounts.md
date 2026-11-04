# News regional-account kernel (`NEWS-02`)

- **Status:** Implemented as a standalone kernel in [`oia-news-claims`](../../crates/oia-news-claims/src/regional.rs) for one slice of [issue #20](https://github.com/mESmaC/oia/issues/20). The issue stays open.
- **Date:** 2026-11-04 UTC
- **Requirements:** [`NEWS-02`](../spec.md#44-news-regional-intelligence-and-claim-assessment); the separate-count part of [`GEO-03`](../spec.md#42-geographic-events-and-uap-investigations)
- **Acceptance scenario:** the regional-account subset of [`A06`](../delivery.md#a06-news-lineage-and-claim-assessment). This is not full end-to-end `A06` or `A19`.
- **Builds on:** [ADR 0005 (`D03`)](../adr/0005-d03-seed-corpus-and-news-sources.md), the [`A06` fixture](../../fixtures/news/a06/README.md), and the [claim-checking contract](../contracts/news-claim-checking.md) (`verify_corpus`)

## What it does

`regional::build_regional_account` takes a news corpus, runs the existing `verify_corpus` checks on it, and then builds an account of reported developments for one region and one UTC time window.

1. **Epistemic categories stay separate.** Each event candidate gets one `EpistemicCategory`: `Plan`, `Observation`, `AttributedStatement`, `Allegation`, `Arrest`, `Charge`, or `Judgment`. It also gets a `DevelopmentKind`: military movement, exercise, deployment, criminal activity, public-safety incident, transport disruption, or other. Categories are never merged or upgraded, and category counts are kept per event and per region.
2. **Movement does not establish purpose.** A `StatedPurposeInput` (intent, mission, or destination) is accepted only on a plan, attributed statement, allegation, or judgment, and only when it cites a passage in that candidate's own source record. The output `AttributedPurpose` names who stated it: the speaker, or the issuing source for an unattributed plan. It keeps the entry's own category, so an alleged mission is still an allegation. An observation that reports movement and carries a purpose is rejected with `MovementPromotedToPurpose`. Any other category carrying a purpose is rejected with `PurposeNotAttributable`. Attributed statements and allegations without a speaker are rejected with `MissingAttribution`. The corpus-level `movement_establishes_intent_or_mission` guard in `verify_corpus` still applies.
3. **Grouping is an explicit input.** The caller assigns every candidate to a `ProposedEventInput` and names the `grouping_decision_id` it comes from, for example a cluster revision. The kernel does not cluster automatically and does not re-implement the reversible ledger in [`oia-query::geo_workspace`](../../crates/oia-query/src/geo_workspace.rs). Events may list extra member versions that have no typed candidate, such as syndicated reprints or corrected versions.
4. **Counts are separate and are not rates.** Each `EventSourceCounts` reports articles (distinct `record_id`), article versions, syndicated-reprint versions, superseded versions, entries, independent sources (distinct `independent_lineage_root_id`), and per-category entries. The region reports distinct articles and independent sources across included events, plus the number of events touching each development kind. No field divides a count by time or presents article volume as how often incidents happen or a crime rate.
5. **Legal process keeps every stage.** Arrests, charges, and judgments must name a `legal_matter_id`. No other category may name one. For each matter, stages are ordered arrest → charge → judgment, with a stable chronological order inside each stage. A later judgment never removes an earlier arrest or a superseded charge. `latest_reported_stage` and `judgment_reported` describe what was reported. They are not findings made by OIA.
6. **Conflicts and uncertainty survive grouping.** `EventConflictInput` entries must reference member versions, and a correction-resolved conflict must name the correcting versions. The kernel returns them unchanged. Location basis (polygon, point with radius, gazetteer centroid, not reported) and a finite positive uncertainty are validated per entry. Each event shows whether any gazetteer-centroid approximation is present, plus its tightest and widest uncertainty. Superseded records, `needs_reassessment` flags, and non-`Reviewed` statuses stay visible on entries and events.
7. **Window coverage is explicit.** Reported times are strict `YYYY-MM-DDTHH:MM:SSZ` strings, and the original strings are kept. Field ranges are checked, and leap seconds are rejected. An event is included if its region matches and at least one entry overlaps the window or has no reported time. An included event is shown whole, and each entry is marked `ReportedBounds`, `UncertaintyOnly` (overlaps only after widening by the stated uncertainty), `Outside`, or `TimeNotReported`. `RegionalCoverage` counts events outside the region or window, uncertainty-only and unreported-time entries, entries needing reassessment, and entries not yet reviewed.
8. **Bounded and cancellable.** `RegionalLimits` sets non-zero budgets for events, candidates, members per event, conflicts per event, passages per candidate, and text bytes. Vectors grow with fallible reservation. The caller's control callback runs before corpus verification, inside it, and once per event and candidate. If it stops the run, the kernel returns `Stopped` with no partial account. Ordering is deterministic: entries by reported start (unreported last), then category, then candidate id. Events by earliest start, then event id.

## Not implemented

This slice does not include live news or feed acquisition (#13, #17), translation, automatic or proximity-based clustering, persistence, application services, UI, or integrated `A06`/`A19` acceptance. Region ids are caller-declared strings; the kernel checks no geometry and has no region or gazetteer source. Development kinds, legal matter ids, and stated purposes are analyst typing supplied as input. The kernel does not extract them from text.

## Fixture example

[`examples/build-a06-regional-account.rs`](../../crates/oia-news-claims/examples/build-a06-regional-account.rs) types the seven synthetic `A06` candidates and groups them by the fixture's `cluster-rev-2` split decision. It then checks the account against `fixtures/news/a06/manifest.json`. The harbor event must give 7 articles, 8 versions, and 3 independent sources. The CX-9 event must give 1/1/1. The region must give 2 proposed events, 8 distinct articles, and 4 independent sources. Any mismatch makes the example exit with code 1. The region id, development kinds, legal matter id, and purposes in the example are declared for the example only. The fixture does not contain them.

## Targeted tests

[`tests/regional_contract.rs`](../../crates/oia-news-claims/tests/regional_contract.rs) adds three tests over a small synthetic corpus:

- A syndicated reprint adds an article and a version but no independent source. Legal stages given out of order come back as arrest → charge → judgment, and the superseded charge is still present.
- An observation that reports movement cannot carry a mission. The same purpose on an attributed statement is accepted and attributed to its speaker. An attributed statement without a speaker is rejected.
- Window coverage shows an uncertainty-only entry, an entry with no reported time, and an event excluded as outside the window.

A unit test in `regional.rs` checks the UTC parser against known Unix instants and rejects an invalid leap day, month, hour, leap second, and non-`Z` offset.

## Local verification evidence

Run on 2026-11-04 UTC on Windows x86_64/MSVC with `rustc 1.99.0 (b940084d7 2026-09-28)`, rebased on `main` @ `753fd2c`:

```text
cargo +1.99.0 fmt --all -- --check                                                  # exit 0
cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings    # exit 0
cargo +1.99.0 test --workspace --locked --offline                                   # exit 0, 94 test targets, 0 failed
cargo +1.99.0 run -p oia-news-claims --example build-a06-regional-account --locked --offline  # status=ok fixture_counts_match=true
cargo +1.99.0 run -p oia-news-claims --example check-a06-claims --locked --offline           # status=ok (unchanged)
```

The `A06` fixture, its Python validator, and the Lean proofs were not changed, so `scripts/news/validate_a06.py` and the proof gate were not re-run for this slice.
