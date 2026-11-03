# Bounded news claim-checking, misleading-coverage review, and publisher summary kernel

- **Implemented:** The dependency-independent **consistency-checking** kernel and contract slice of [issue #21](https://github.com/mESmaC/oia/issues/21) for caller-supplied claim assessments, evidence links, quotation fidelity, correction propagation, and publisher summaries, plus a bounded in-memory append-only claim review ledger, supporting [`NEWS-03`](../spec.md#44-regional-news-and-public-events), [`NEWS-04`](../spec.md#44-regional-news-and-public-events), [`NEWS-05`](../spec.md#44-regional-news-and-public-events), and the claim-assessment/review-history subsets of [`A06`](../delivery.md#a06-news-lineage-and-claim-assessment).
- **Not implemented:** Automated claim extraction or truth evaluation, shared application-service integration (`oia-domain`, `oia-analysis`, `oia-app`), review persistence, reviewer authentication/authorization, durable reassessment resolution, UI claim-review workflows, and full end-to-end `A06` acceptance. Those remain open on [#21](https://github.com/mESmaC/oia/issues/21) and depend on `#10`, `#14`, `#16`, `#18`, and `#20`. The ledger borrows caller-owned data and is dropped without external effect; it does not publish projections into application state. Because `oia-news-claims` is standard-library-only, it validates the 64-digit lowercase hex format of `declared_sha256_hex` and exact UTF-8 byte/char lengths while [`scripts/news/validate_a06.py`](../../scripts/news/validate_a06.py) verifies the cryptographic SHA-256 digest of each fixture blob. No formal Lean proof or measured performance target is claimed.

The full domain contract is specified in [`docs/contracts/news-claim-checking.md`](../contracts/news-claim-checking.md).

## Run locally

From the repository root with Rust 1.99.0 installed:

```text
cargo +1.99.0 run -p oia-news-claims --example check-a06-claims --locked --offline
cargo +1.99.0 test -p oia-news-claims --test review_contract --locked --offline
```

This read-only example validates the merged synthetic [`fixtures/news/a06/`](../../fixtures/news/a06/README.md) corpus against `NEWS-03`, `NEWS-04`, `NEWS-05`, and `A06`, printing per-claim assessment states, independent supporting/contrary lineage-root counts, quotation fidelity status, and temporal assessment shifts.

## Kernel boundary and contract

`oia_news_claims::verify_corpus(input, limits, check_control)` is a pure, standard-library-only Rust consistency checker (`edition = "2024"`, `rust-version = "1.99"`, `unsafe_code = "forbid"`, `unwrap_used = "deny"`, `expect_used = "deny"`, `indexing_slicing = "deny"`). It:

- Borrows immutable UTF-8 source blobs, passage anchors, lineage edges, event candidates, caller-supplied claim assessments, dependency invalidations, and publisher summaries.
- Enforces explicit, non-zero `AssessmentLimits` (`max_sources`, `max_source_records`, `max_passages_per_record`, `max_lineage_edges`, `max_claims`, `max_evidence_links_per_claim`, `max_invalidations`, `max_publisher_summaries`, `max_blob_bytes`) before work or allocation, using fallible `try_reserve` for vector growth.
- Checks the cooperative cancellation/deadline/supersession callback (`StopReason::Cancelled`, `DeadlineExceeded`, `Superseded`) at entry, across every source, record, passage, lineage edge, candidate, claim, invalidation, and publisher summary, and immediately before returning.
- Performs no file I/O, network requests, model calls, or mutable state publication.

### Verified invariants

1. **Passage byte and Unicode codepoint anchors (`NEWS-03`):** Every passage anchor is checked against its immutable UTF-8 source payload in both half-open byte offsets (`byte_range`) and half-open Unicode scalar value offsets (`char_range`), including multibyte divergence on the bilingual Japanese/English harbor log (`sr-vesper-harbor-log@v1`, `p-harbor-ja-1`).
2. **Upstream-only quotation fidelity and fault categories (`NEWS-03`):** ASCII double-quoted verbatim attributions (`"..."`) of at least 12 bytes in downstream passages (checked without whitespace normalization; other quotation styles or shorter spans return `QuotationFidelityStatus::NoCheckedQuotationFound`) are checked exclusively against upstream records reachable via outbound `{LineageRelationKind::Quotes, LineageRelationKind::Republishes}` edges (`from_version_id -> to_version_id`). Sibling or downstream syndicated reprints sharing an `independent_lineage_root_id` cannot vouch for a fabricated quote. Fabricated or altered verbatim quotes (such as `"harbor control confirmed a full corridor lockdown"` in `claim-04-tribune-blockade-and-distorted-quote`) must be flagged with `FaultCategory::DistortedQuotation` and linked to contrary upstream evidence (`p-harbor-kealoha-quote`), or verification fails with `ClaimCheckError::DistortedQuoteNotFlagged`. Claims with `FaultCategory::HeadlineCaptionMismatch` must pin a valid `headline_passage_id`.
3. **Orthogonal assessment, review, and availability states (`NEWS-04`):**
   - Keeps `ClaimAssessmentState` (`Supported`, `Contradicted`, `UnsupportedByAvailableEvidence`, `MateriallyMisleading`, `Disputed`, `Unresolved`), `ReviewStatus`, and `AvailabilityState` distinct.
   - Rejects `Supported` claims that lack supporting passage anchors (`SupportedWithoutEvidence`).
   - Rejects `Contradicted` or `MateriallyMisleading` claims that lack contrary passage anchors (`UnsupportedAutomaticallyFalse`), ensuring lack of corroboration alone never establishes falsity.
   - Preserves `quotes_claim_without_endorsing` (`claim-03-contractor-payload-allegation`) and publication-time vs. post-adjudication assessment shifts (`claim-06-willful-fairway-obstruction`: `Disputed` $\to$ `Contradicted`).
   - Rejects any `SourceRecord` where `availability == AvailabilityState::Removed` (`HTTP 410 Gone`) is auto-marked `RetractionStatus::Retracted` (`RemovedPageAutoRetracted`).
   - Collapses syndicated reprints and derivative quotes sharing an `independent_lineage_root_id` when counting `independent_supporting_root_count` and `independent_contrary_root_count`.
4. **Correction propagation and publisher summaries (`NEWS-05`):**
   - Verifies that every `DependencyInvalidation` triggered by a source correction (`sr-meridian-wire-884@v2`) or adjudication outcome (`sr-maritime-adj-panel-0412@v1`) sets `needs_reassessment = true` on all affected `SourceRecord`, `EventCandidate`, `Claim` (matched strictly by `version_id`), and `Report` records while preserving prior issued versions (`prior_issued_version_preserved = true`).
   - Verifies that every `PublisherSummaryInput` declares a valid RFC 3339 UTC `time_window` (`window_start_utc <= window_end_utc`), non-empty `sampling_method`, `sample_size == evaluated_source_record_versions.len()` with `1 <= sample_size <= article_count_in_window` (and `sample_size == article_count_in_window` when `sampling_method == "complete_case_corpus_within_window"`), consistent `assessment_counts` and `unresolved_assessment_count == assessment_counts.unresolved + assessment_counts.disputed` across the publisher's evaluated records, valid `correction_history` supersession links, complete `uncorrected_upstream_dependencies` for any active evaluated record referencing a superseded upstream record via `{Quotes, Republishes}`, and `represents_incident_frequency_or_crime_rate == false` with a non-empty `methodology_note`.
5. **Append-only review history (`NEWS-03`, `NEWS-04`, `A06` review-history subset):**
   - `review::ClaimReviewLedger::new` borrows one exact claim version and its source records only after running the existing `verify_corpus` validation. It does not duplicate the corpus verifier.
   - `append` validates the entire borrowed `ReviewEntry` before one fallible history reservation and push. It enforces exact target version, unique review IDs, optional non-self/non-duplicate supersession of an existing entry, non-regressing caller-supplied signed Unix-second timestamps, non-empty reviewer/scope/rationale, validated passage evidence, assessment-evidence requirements, the target claim's headline anchor for `HeadlineCaptionMismatch`, and non-zero `ReviewLimits` for history length, combined evidence links, fault categories, and review text/identifier bytes.
   - `history` retains every entry in append order. `projection` selects only the latest assessment and review status while preserving publication-time assessment, availability, and sticky reassessment state. Earlier entries and borrowed claim/source values are never mutated. `Reviewed` and `Supported` remain independent caller values.
   - The implementation performs no wall-clock reads, storage, networking, reviewer authentication/authorization, truth inference, or UI/application publication.

## Local verification evidence

Executed on 2026-11-02 (America/New_York), Windows x86_64/MSVC, Rust/Cargo 1.99.0, Python 3.12, at base revision `05b56e6`:

```text
cargo +1.99.0 fmt --all -- --check
cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings
cargo +1.99.0 test --workspace --locked --offline
cargo +1.99.0 run -p oia-news-claims --example check-a06-claims --locked --offline
python -I scripts/news/validate_a06.py --self-test
```

The four targeted integration tests in [`crates/oia-news-claims/tests/claim_contract.rs`](../../crates/oia-news-claims/tests/claim_contract.rs) cover:

1. `verifies_a06_claims_distorted_quotes_and_independent_roots`: Full verification of the 8 sources, 9 record versions, 21 passages, 6 lineage edges, 7 event candidates, 7 claims, 2 invalidations, and 3 publisher summaries in `A06`, checking lineage-root deduplication, unendorsed allegation attribution, distorted quote detection, temporal assessment shift, and removed-page handling.
2. `rejects_removed_page_auto_retracted_and_unfounded_contradiction`: Mutation checks confirming rejection of auto-retracted removed pages (`RemovedPageAutoRetracted`), marking an uncorroborated claim `Contradicted` without contrary evidence (`UnsupportedAutomaticallyFalse`), omitting `DistortedQuotation` on a fabricated quote (`DistortedQuoteNotFlagged`), and ensuring a sibling reprint (`sr-pelagic-digest-884@v1`) sharing `independent_lineage_root_id` cannot vouch for Tribune's fabricated quote when not upstream via `{Quotes, Republishes}`.
3. `rejects_unpropagated_corrections_and_publisher_volume_or_sample_violations`: Mutation checks confirming rejection of unpropagated `needs_reassessment` flags (`UnpropagatedCorrection`), equating article volume with crime rate (`VolumeEquatedWithCrimeRate`), mismatched publisher sample sizes (`PublisherSummarySampleMismatch`), and omitting superseded upstream dependencies from `uncorrected_upstream_dependencies` (`PublisherSummarySampleMismatch`).
4. `enforces_multibyte_anchors_budgets_and_cooperative_stop`: Mutation checks confirming rejection of a split UTF-8 codepoint on the Japanese passage `p-harbor-ja-1` (`InvalidPassageAnchor`), zero-limit configuration (`ZeroLimit`), claim budget overflow (`BudgetExceeded`), and cancellation at every callback point (`Stopped(StopReason::Cancelled)`).

## Review-ledger focused checks

Run from the repository root with Rust 1.99.0:

```text
cargo +1.99.0 fmt --all -- --check
cargo +1.99.0 clippy -p oia-news-claims --all-targets --locked --offline -- -D warnings
cargo +1.99.0 test -p oia-news-claims --locked --offline
```

The two synthetic-data integration tests in [`crates/oia-news-claims/tests/review_contract.rs`](../../crates/oia-news-claims/tests/review_contract.rs) cover the successful challenge-to-reviewed history/projection path and transactional rejection of stale targets, duplicate IDs, invalid supersession, timestamp regression, empty required text, unknown or malformed passage evidence, missing assessment evidence or headline anchors, and review/evidence/text budget overflow. These tests do not establish persistence, application integration, or full `A06`.

### Review-ledger slice verification record

Executed on 2026-11-03 (America/New_York), Windows x86_64/MSVC, Rust/Cargo 1.99.0, on the review-ledger working tree based at `433e253`:

```text
cargo +1.99.0 test -p oia-news-claims --test review_contract --locked --offline
cargo +1.99.0 run -p oia-news-claims --example check-a06-claims --locked --offline
python -I scripts/news/validate_a06.py --self-test
cargo +1.99.0 fmt --all -- --check
cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings
cargo +1.99.0 test --workspace --locked --offline
git diff --check
```

All commands passed. The focused run passed both new review contract tests; the workspace run also passed the existing four claim contract tests. The app build reported that the gitignored CesiumJS vendor directory was absent, so this verification did not exercise the Globe renderer. No Lean gate was run because this slice does not change a proof-covered algorithm or make a proof/correspondence claim.
