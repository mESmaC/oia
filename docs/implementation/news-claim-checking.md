# Bounded news claim-checking, misleading-coverage review, and publisher summary kernel

- **Implemented:** The dependency-independent claim-assessment, quotation-fidelity, correction-propagation, and publisher-summary kernel and contract slice of [issue #21](https://github.com/mESmaC/oia/issues/21), supporting [`NEWS-03`](../spec.md#44-regional-news-and-public-events), [`NEWS-04`](../spec.md#44-regional-news-and-public-events), [`NEWS-05`](../spec.md#44-regional-news-and-public-events), and the claim-assessment subset of [`A06`](../delivery.md#a06-news-lineage-and-claim-assessment).
- **Not implemented:** Shared application-service integration (`oia-domain`, `oia-analysis`, `oia-app`), database persistence, UI claim-review workflows, and full end-to-end `A06` acceptance. Those remain open on [#21](https://github.com/mESmaC/oia/issues/21) and depend on `#10`, `#14`, `#16`, `#18`, and `#20`. No formal Lean proof or measured performance target is claimed.

The full domain contract is specified in [`docs/contracts/news-claim-checking.md`](../contracts/news-claim-checking.md).

## Run locally

From the repository root with Rust 1.99.0 installed:

```text
cargo +1.99.0 run --manifest-path crates/oia-news-claims/Cargo.toml --example check-a06-claims --locked --offline
```

This read-only example validates the merged synthetic [`fixtures/news/a06/`](../../fixtures/news/a06/README.md) corpus against `NEWS-03`, `NEWS-04`, `NEWS-05`, and `A06`, printing per-claim assessment states, independent supporting/contrary lineage-root counts, quotation fidelity status, and temporal assessment shifts.

## Kernel boundary and contract

`oia_news_claims::verify_corpus(input, limits, check_control)` is a pure, standard-library-only Rust kernel (`edition = "2024"`, `rust-version = "1.99"`, `unsafe_code = "forbid"`, `unwrap_used = "deny"`, `expect_used = "deny"`, `indexing_slicing = "deny"`). It:

- Borrows immutable UTF-8 source blobs, passage anchors, event candidates, claim assessments, dependency invalidations, and publisher summaries supplied by the caller.
- Enforces explicit, non-zero `AssessmentLimits` (`max_sources`, `max_source_records`, `max_passages_per_record`, `max_claims`, `max_evidence_links_per_claim`, `max_invalidations`, `max_publisher_summaries`, `max_blob_bytes`) before work or allocation, using fallible `try_reserve` for vector growth.
- Checks the cooperative cancellation/deadline/supersession callback (`StopReason::Cancelled`, `DeadlineExceeded`, `Superseded`) at entry, across every source, record, passage, candidate, claim, invalidation, and publisher summary, and immediately before returning.
- Performs no file I/O, network requests, model calls, or mutable state publication.

### Verified invariants

1. **Passage byte and Unicode codepoint anchors (`NEWS-03`):** Every passage anchor is checked against its immutable UTF-8 source payload in both half-open byte offsets (`byte_range`) and half-open Unicode scalar value offsets (`char_range`), including multibyte divergence on the bilingual Japanese/English harbor log (`sr-vesper-harbor-log@v1`, `p-harbor-ja-1`).
2. **Quotation fidelity and fault categories (`NEWS-03`):** Double-quoted verbatim attributions (`"..."`) in downstream passages are checked against upstream passages sharing the same `independent_lineage_root_id`. Fabricated or altered verbatim quotes (such as `"harbor control confirmed a full corridor lockdown"` in `claim-04-tribune-blockade-and-distorted-quote`) must be flagged with `FaultCategory::DistortedQuotation` and linked to contrary upstream evidence (`p-harbor-kealoha-quote`), or verification fails with `ClaimCheckError::DistortedQuoteNotFlagged`. Claims with `FaultCategory::HeadlineCaptionMismatch` must pin a valid `headline_passage_id`.
3. **Orthogonal assessment, review, and availability states (`NEWS-04`):**
   - Keeps `ClaimAssessmentState` (`Supported`, `Contradicted`, `UnsupportedByAvailableEvidence`, `MateriallyMisleading`, `Disputed`, `Unresolved`), `ReviewStatus`, and `AvailabilityState` distinct.
   - Rejects `Supported` claims that lack supporting passage anchors (`SupportedWithoutEvidence`).
   - Rejects `Contradicted` or `MateriallyMisleading` claims that lack contrary passage anchors (`UnsupportedAutomaticallyFalse`), ensuring lack of corroboration alone never establishes falsity.
   - Preserves `quotes_claim_without_endorsing` (`claim-03-contractor-payload-allegation`) and publication-time vs. post-adjudication assessment shifts (`claim-06-willful-fairway-obstruction`: `Disputed` $\to$ `Contradicted`).
   - Rejects any `SourceRecord` where `availability == AvailabilityState::Removed` (`HTTP 410 Gone`) is auto-marked `RetractionStatus::Retracted` (`RemovedPageAutoRetracted`).
   - Collapses syndicated reprints and derivative quotes sharing an `independent_lineage_root_id` when counting `independent_supporting_root_count` and `independent_contrary_root_count`.
4. **Correction propagation and publisher summaries (`NEWS-05`):**
   - Verifies that every `DependencyInvalidation` triggered by a source correction (`sr-meridian-wire-884@v2`) or adjudication outcome (`sr-maritime-adj-panel-0412@v1`) sets `needs_reassessment = true` on all affected `SourceRecord`, `EventCandidate`, `Claim`, and `Report` records while preserving prior issued versions (`prior_issued_version_preserved = true`).
   - Verifies that every `PublisherSummaryInput` declares a valid `time_window`, non-empty `sampling_method`, exact `sample_size` matching `evaluated_source_record_versions` and `article_count_in_window`, consistent `assessment_counts` and `unresolved_assessment_count` across the publisher's evaluated records, valid `correction_history` supersession links, and `represents_incident_frequency_or_crime_rate == false` with a non-empty `methodology_note`.

## Local verification evidence

Executed on 2026-11-02 (America/New_York), Windows x86_64/MSVC, Rust/Cargo 1.99.0, Python 3.12, at base revision `3a9d5d3`:

```text
cargo +1.99.0 fmt --manifest-path crates/oia-news-claims/Cargo.toml --all -- --check
cargo +1.99.0 clippy --manifest-path crates/oia-news-claims/Cargo.toml --workspace --all-targets --locked --offline -- -D warnings
cargo +1.99.0 test --manifest-path crates/oia-news-claims/Cargo.toml --workspace --locked --offline
cargo +1.99.0 run --manifest-path crates/oia-news-claims/Cargo.toml --example check-a06-claims --locked --offline
python -I scripts/news/validate_a06.py --self-test
```

The four targeted integration tests in [`crates/oia-news-claims/tests/claim_contract.rs`](../../crates/oia-news-claims/tests/claim_contract.rs) cover:

1. `verifies_a06_claims_distorted_quotes_and_independent_roots`: Full verification of the 8 sources, 9 record versions, 21 passages, 7 event candidates, 7 claims, 2 invalidations, and 3 publisher summaries in `A06`, checking lineage-root deduplication, unendorsed allegation attribution, distorted quote detection, temporal assessment shift, and removed-page handling.
2. `rejects_removed_page_auto_retracted_and_unfounded_contradiction`: Mutation checks confirming rejection of auto-retracted removed pages (`RemovedPageAutoRetracted`), marking an uncorroborated claim `Contradicted` without contrary evidence (`UnsupportedAutomaticallyFalse`), and omitting `DistortedQuotation` on a fabricated quote (`DistortedQuoteNotFlagged`).
3. `rejects_unpropagated_corrections_and_publisher_volume_or_sample_violations`: Mutation checks confirming rejection of unpropagated `needs_reassessment` flags (`UnpropagatedCorrection`), equating article volume with crime rate (`VolumeEquatedWithCrimeRate`), and mismatched publisher sample sizes (`PublisherSummarySampleMismatch`).
4. `enforces_multibyte_anchors_budgets_and_cooperative_stop`: Mutation checks confirming rejection of a split UTF-8 codepoint on the Japanese passage `p-harbor-ja-1` (`InvalidPassageAnchor`), zero-limit configuration (`ZeroLimit`), claim budget overflow (`BudgetExceeded`), and cancellation at every callback point (`Stopped(StopReason::Cancelled)`).
