# News Claim-Checking, Misleading-Coverage Review, and Publisher Summary Contract

- **Status:** Implementation design, contract specification, and deterministic validation rules for [issue #21](https://github.com/mESmaC/oia/issues/21), building on [ADR 0005 (`D03`)](../adr/0005-d03-seed-corpus-and-news-sources.md) and the [document lineage contract](document-lineage-and-correction.md).
- **Date:** 2026-11-02 UTC
- **Requirements:** [`NEWS-03`](../spec.md#44-regional-news-and-public-events), [`NEWS-04`](../spec.md#44-regional-news-and-public-events), [`NEWS-05`](../spec.md#44-regional-news-and-public-events)
- **Acceptance scenario:** [`A06: news lineage and claim assessment`](../delivery.md#a06-news-lineage-and-claim-assessment) (claim assessment, correction propagation, and publisher summary subset)
- **Companion kernel & fixture:** [`crates/oia-news-claims`](../../crates/oia-news-claims/Cargo.toml), [`docs/implementation/news-claim-checking.md`](../implementation/news-claim-checking.md), [`fixtures/news/a06/`](../../fixtures/news/a06/README.md)

The [architecture](../architecture.md) owns the shared domain records, origin classes (`§2.3`), claim/review/availability states (`§2.4`), storage (`§5`), and durable jobs (`§6.1`). This contract specifies passage-level factual and presentation evaluation, claim assessment criteria, correction/outcome invalidation, and publisher-level summaries; final shared workspace integration awaits the merged `oia-domain` and `oia-analysis` packages (`#10`, `#14`, `#18`, `#20`).

## 1. Scope and requirement traceability

| Requirement / Scenario | Contract obligation in this specification |
| --- | --- |
| `NEWS-03` | Evaluate specific claims and presentations for `factual_contradiction`, `unsupported_certainty`, `headline_caption_mismatch`, `distorted_quotation`, `misattributed_imagery`, `numerical_error`, `selective_comparison`, `materially_missing_context`, and `news_volume_equated_with_crime_rate`. Pin every assessment to the exact `SourceRecord` version and passage byte/codepoint span, contrary/supporting passage evidence, stated reasoning, and review status. |
| `NEWS-04` | Support six explicit assessment states (`supported`, `contradicted`, `unsupported_by_available_evidence`, `materially_misleading`, `disputed`, `unresolved`) kept strictly separate from `review_status` and `availability`. Distinguish quoting a claim from endorsing it, opinion from factual premises, and publication-time evidence from later knowledge. Never equate lack of corroboration or majority agreement with truth/falsity, and never auto-mark a deleted/removed page as retracted. |
| `NEWS-05` | Flag dependent source records, event candidates, claim assessments, and reports as `needs_reassessment` when corrections (`supersedes`), retractions (`retracted_by`), or case outcomes arrive, without overwriting prior issued versions. Expose sample size, evaluated versions, sampling method, time window, unresolved count, and correction history in publisher summaries, and enforce that news volume is never equated with incident frequency or crime rate. |
| `A06` | Verify distorted quotations, overstated headlines, numerical transcription errors, attributed allegations, publication-time vs. post-adjudication shifts, deleted pages (`HTTP 410 Gone` $\neq$ retracted), correction invalidation, and publisher summary methodology guards over the synthetic A06 corpus. |

## 2. Passage-level claim and presentation evaluation (`NEWS-03`)

### 2.1 Immutable passage anchoring

Every evaluated claim or presentation fault must reference an immutable `SourceRecord` version (`source_record_version_id`) and a verified passage anchor (`passage_id`):

1. **Dual UTF-8 and Unicode codepoint spans:** Each passage anchor carries half-open UTF-8 byte offsets `[byte_start, byte_end)` and half-open Unicode scalar value (codepoint) offsets `[char_start, char_end)` into the retained UTF-8 source payload (`raw_bytes`), plus the exact `quote` string.
2. **Multibyte safety:** Both offsets must satisfy `0 <= byte_start < byte_end <= byte_length` and `0 <= char_start < char_end <= char_length`, land on valid UTF-8 character boundaries, and slice to the exact `quote`. Multibyte passages (such as the Japanese VHF log entry `p-harbor-ja-1` in `sr-vesper-harbor-log@v1`) cause byte and character offsets to diverge on that passage and all subsequent passages; validators must verify both coordinate spaces independently.
3. **Optional headline/caption anchor:** When a presentation assessment involves a headline or media caption mismatch (`headline_caption_mismatch`), the claim record may additionally pin `headline_passage_id` (or `caption_passage_id`) within the same `source_record_version_id` so the headline/caption and body passage can be inspected side by side.

### 2.2 Fault taxonomy (`FaultCategory`)

Presentation and factual fault annotations use explicit machine-readable categories:

- `factual_contradiction`: A factual assertion is directly refuted by primary observational logs, instrumentation, or verified records (`sr-archipelago-chronicle-884@v1` claiming a corridor-wide shipping blockade when `sr-pelagic-notice-0041@v1` and `sr-vesper-harbor-log@v1` show lanes remained open).
- `unsupported_certainty`: Presenting an unverified, provisional, or speculative statement as established fact.
- `headline_caption_mismatch`: A headline or caption overstates, inverts, or contradicts the body text or underlying source record (`p-tribune-headline` declaring `"MARITIME LOCKDOWN HALTS SHIPPING ACROSS KESTREL CORRIDOR"` for a 35-minute local fairway hold).
- `distorted_quotation`: Attributing verbatim text inside quotation marks that alters, fabricates, or inverts the speaker's actual words in the upstream source (`p-tribune-distorted-quote` attributing `"harbor control confirmed a full corridor lockdown"` to Capt. Kealoha when the upstream log `p-harbor-kealoha-quote` and wire dispatch `p-wire-v1-kealoha` state `"harbor control held inbound pilotage for 35 minutes"` and `"no range vessels entered the harbor channel"`).
- `misattributed_imagery`: Pairing an image, video clip, or sensor graphic from a different time, place, or simulation with an event without disclosure.
- `numerical_error`: A concrete transcription, unit, or arithmetic error (`p-wire-v1-14vessels` misreading `"14-meter commercial survey launch"` as `"14 vessels"`, or miscomputing a percentage change from repeated syndication).
- `selective_comparison`: Comparing non-equivalent windows, denominators, or populations to create a false trend.
- `materially_missing_context`: Omitting decisive qualifying information present in the cited source (such as omitting that an obstruction charge was provisional or that an offshore exercise was $>35\text{ km}$ away).
- `news_volume_equated_with_crime_rate`: Treating the number of published or syndicated news articles as a measure of real-world incident frequency, crime rate, or community criminality (`p-tribune-crime-surge` claiming a `"350% maritime crime surge"` because seven articles were published on the same non-criminal equipment fault).

### 2.3 Quotation fidelity verification

When a downstream passage encloses an attributed quotation in double quotes (`"..."`) or single/CJK quotation marks and links via `quotes` or `republishes` to an upstream `SourceRecord` version:

1. **Exact substring check:** The quotation verifier extracts the quoted span and checks whether it appears verbatim (or after documented whitespace normalization) in the upstream record's passages.
2. **Distortion detection:** If a downstream record attributes a verbatim quote to a named speaker or upstream record, and the quoted substring is absent from the upstream record while conflicting with the speaker's recorded passage (`p-harbor-kealoha-quote`), the verifier flags a `distorted_quotation` fault and requires the upstream passage to be linked in `contrary_evidence`.

## 3. Claim assessment states, attribution, and lifecycle separation (`NEWS-04`)

### 3.1 Three orthogonal state dimensions

Per [Architecture §2.4](../architecture.md#24-claim-and-review-states), every claim assessment record maintains three separate state axes that must never be collapsed into a single boolean or badge:

1. **Assessment (`ClaimAssessmentState`):**
   - `supported`: Backed by independent primary evidence or verified records with no unresolved material contradiction.
   - `contradicted`: Refuted by verified primary evidence, physical constraints, or final adjudication/correction.
   - `unsupported_by_available_evidence`: The claim asserts a factual proposition for which current retained evidence provides no corroboration, without yet proving the proposition false.
   - `materially_misleading`: The passage uses distorted quotations, headline mismatch, numerical/unit conflation, selective comparison, or missing context that materially misrepresents the underlying evidence even if individual words overlap an event.
   - `disputed`: Credible sources or parties disagree on the proposition and available evidence does not yet decisively resolve the conflict.
   - `unresolved`: Available evidence is incomplete (for example, an uncorroborated witness vantage point or a removed page whose underlying observation is neither confirmed nor refuted).
2. **Review status (`ReviewStatus`):** `unreviewed`, `in_review`, `reviewed`, `challenged`, `superseded`.
3. **Source availability (`AvailabilityState`):** `available`, `reference_only`, `missing`, `restricted`, `removed`, `expired`, `corrupt`.

### 3.2 Mandatory epistemic invariants (`NEWS-04`)

Implementations and validators must enforce these invariants before persisting or publishing a claim assessment:

1. **Unsupported is not automatically contradicted:** A claim with zero supporting evidence and zero contrary primary evidence may be `unsupported_by_available_evidence` or `unresolved`, never automatically marked `contradicted`. Conversely, a `contradicted` or `materially_misleading` assessment **must** link at least one contrary passage anchor (`contrary_evidence` non-empty) and state the specific refuting criteria in `criteria_and_reasoning`.
2. **Supported requires evidence:** A `supported` assessment **must** link at least one verified passage anchor in `supporting_evidence`.
3. **Quoting versus endorsing (`quotes_claim_without_endorsing`):** Reporting that a speaker made an allegation (`claim-03-contractor-payload-allegation` in `sr-meridian-wire-884@v1`) must separate whether the speaker said it from whether the underlying proposition is true. An article that accurately attributes an unverified allegation without endorsing it is evaluated on the underlying proposition (`unsupported_by_available_evidence`) with `quotes_claim_without_endorsing: true`, rather than accusing the journalist of fabricating the quote.
4. **Opinion versus factual premise (`is_opinion_not_factual_premise`):** Value judgments and editorial opinions are distinguished from checkable factual premises; when an opinion rests on a false factual premise (such as a 350% crime surge derived from syndicated article counts), the factual premise is evaluated under `NEWS-03`/`NEWS-04`.
5. **Publication-time versus current knowledge (`assessment_at_publication` vs. `current_assessment`):** Every claim record preserves both the assessment justified by evidence available at publication time and the current assessment after later corrections, inspections, or adjudications. For example, in `claim-06-willful-fairway-obstruction`, Provisional Count 2 was `disputed` at publication on `2024-03-16` (`sr-vesper-port-safety-0412@v1`) and became `contradicted` on `2024-03-18` after the adjudication order (`sr-maritime-adj-panel-0412@v1`) dismissed the charge based on electrical bus logs.
6. **Removed pages are not auto-retracted (`A06`):** When a source page later returns HTTP `404` or `410 Gone` (`availability: removed`, such as `sr-sable-shore-notebook@v1`), its `retraction_status` remains `not_retracted` unless an explicit retraction notice is evidenced. Any record or claim that marks `retraction_status: retracted` solely because `availability == removed` fails validation (`RemovedPageAutoRetracted`).
7. **Corroboration independence (`INTEL-01`, `NEWS-01`):** Syndicated reprints (`republishes`) and derivative quotes (`quotes`) sharing the same `independent_lineage_root_id` must be collapsed when counting independent supporting sources for a claim. Multiple outlets repeating `sr-meridian-wire-884@v1` count as one upstream lineage root, never independent corroboration.

## 4. Correction propagation and publisher-level summaries (`NEWS-05`)

### 4.1 Correction, retraction, and outcome propagation

When a `SourceRecord` version is superseded by a corrected dispatch (`sr-meridian-wire-884@v2` superseding `sr-meridian-wire-884@v1`) or updated by an official case outcome (`sr-maritime-adj-panel-0412@v1` updating `sr-vesper-port-safety-0412@v1`):

1. **Deterministic dependency walk:** Every downstream `SourceRecord` that republished the uncorrected version (`sr-archipelago-chronicle-884@v1`, `sr-pelagic-digest-884@v1`), every `EventCandidate` derived from the superseded claim/charge (`cand-contractor-payload-allegation`, `cand-reef-scanner-charges`), every affected `Claim` version (`claim-06-willful-fairway-obstruction@v2`), and every dependent `Report` version (`report-vesper-harbor-brief@v1`) must be flagged `needs_reassessment: true`.
2. **Preservation of issued versions:** Flagging `needs_reassessment: true` records an immutable `DependencyInvalidation` entry (`prior_issued_version_preserved: true`) and never overwrites the prior `SourceRecord`, `Claim`, or `Report` version in place.
3. **Uncorrected downstream tracking:** If a downstream publisher (`src-archipelago-chronicle`) continues to display a syndicated story derived from `sr-meridian-wire-884@v1` after `v2` was issued, its publisher summary must list `sr-meridian-wire-884@v1` under `uncorrected_upstream_dependencies`.

### 4.2 Publisher-level summary contract (`PublisherSummary`)

A publisher summary aggregates claim and revision behavior for a single registered `source_id` over an explicit evaluation scope. To satisfy `NEWS-05` and `A06`, every `PublisherSummary` must include:

- `summary_id`, `source_id`, and `publisher_name`.
- `time_window`: Explicit `start_utc` and `end_utc` bounds (`start_utc <= end_utc`).
- `evaluated_source_record_versions`: Non-empty list of exact `SourceRecord` `version_id`s belonging to `source_id` included in the sample.
- `sample_size`: Exact count of `evaluated_source_record_versions` (`sample_size == len(evaluated_source_record_versions)`).
- `sampling_method`: Explicit description of how the sample was selected (for example, `"complete_case_corpus_within_window"`).
- `assessment_counts`: Per-state counts across the six `NEWS-04` assessment states for claims originating in `evaluated_source_record_versions`.
- `unresolved_assessment_count`: Explicit count of `unresolved` (plus any `disputed`) assessments so readers can see how much of the sample remains unsettled; must match `assessment_counts.unresolved`.
- `correction_history`: Chronological list of `(original_version_id, corrected_version_id, corrected_at, summary)` records for revisions issued by that publisher within the window.
- `uncorrected_upstream_dependencies`: Any upstream `SourceRecord` versions superseded by a correction that this publisher's evaluated records still depend on without a matching downstream correction.
- `volume_interpretation_guard`:
  - `article_count_in_window`: Total evaluated article versions in the window (`== sample_size`).
  - `distinct_underlying_incidents`: Count of distinct underlying event clusters or incidents represented.
  - `represents_incident_frequency_or_crime_rate`: **Must be `false`.** Any summary that sets this flag to `true` or omits `methodology_note` is rejected (`VolumeEquatedWithCrimeRate`).
  - `methodology_note`: Non-empty human-readable explanation stating how the sample was constructed and why publication volume does not measure incident frequency or crime rate.

## 5. Failure behavior and resource bounds

Implementations of this contract must validate sizes, counts, and invariants before allocation or graph traversal (`AGENTS.md §5`):

- `ZeroLimit`: Any configured budget in `AssessmentLimits` (`max_sources`, `max_records`, `max_passages_per_record`, `max_claims`, `max_evidence_links_per_claim`, `max_invalidations`, `max_publisher_summaries`, `max_blob_bytes`) is zero.
- `BudgetExceeded`: Input collection or blob byte length exceeds the caller's configured `AssessmentLimits`.
- `InvalidPassageAnchor`: A passage's `byte_start..byte_end` or `char_start..char_end` is out of bounds, splits a UTF-8 codepoint, or does not match `quote` in the retained blob bytes.
- `RemovedPageAutoRetracted`: A record with `availability == "removed"` is marked `retraction_status == "retracted"` without an explicit retraction record (`NEWS-04`, `A06`).
- `UnsupportedAutomaticallyFalse`: A claim without contrary evidence is marked `contradicted`, or a `contradicted`/`materially_misleading` claim omits `contrary_evidence` or `criteria_and_reasoning` (`NEWS-03`, `NEWS-04`).
- `SupportedWithoutEvidence`: A claim is marked `supported` with empty `supporting_evidence`.
- `UnpropagatedCorrection`: A `supersedes` edge or `DependencyInvalidation` fails to mark an affected downstream record, candidate, or claim `needs_reassessment: true`, or fails to preserve prior issued versions (`NEWS-05`).
- `PublisherSummarySampleMismatch`: A `PublisherSummary` omits `sampling_method`, mismatches `sample_size` vs. `evaluated_source_record_versions`, mismatches `assessment_counts` against evaluated claims for that publisher, or mismatches `unresolved_assessment_count`.
- `VolumeEquatedWithCrimeRate`: A `PublisherSummary` sets `represents_incident_frequency_or_crime_rate: true` or omits `methodology_note` (`NEWS-05`, `A06`).
- `AllocationFailed`: Fallible vector reservation (`try_reserve`) fails.
- `Stopped`: Cooperative cancellation, deadline expiry, or supersession callback fired.

## 6. Local verification

The standalone Rust kernel in [`crates/oia-news-claims`](../../crates/oia-news-claims/Cargo.toml) and the Python fixture validator in [`scripts/news/validate_a06.py`](../../scripts/news/validate_a06.py) enforce these rules offline against `fixtures/news/a06/`:

```text
cargo +1.99.0 fmt --manifest-path crates/oia-news-claims/Cargo.toml --all -- --check
cargo +1.99.0 clippy --manifest-path crates/oia-news-claims/Cargo.toml --workspace --all-targets --locked --offline -- -D warnings
cargo +1.99.0 test --manifest-path crates/oia-news-claims/Cargo.toml --workspace --locked --offline
cargo +1.99.0 run --manifest-path crates/oia-news-claims/Cargo.toml --example check-a06-claims --locked --offline
python -I scripts/news/validate_a06.py --self-test
```
