# Bounded document release comparison, source lineage, and correction propagation kernel

Status: implemented in the temporary standalone `oia-documents` validation
package; not integrated into application services or persistent storage. This
is the Rust validation and graph-evaluation kernel slice of
[#19](https://github.com/mESmaC/oia/issues/19), supporting
[DOC-02, DOC-04, and DOC-05](../spec.md#41-public-document-library) and
[INTEL-01](../spec.md#7-additional-intelligence-methods). Verification covers
the document-lineage, release-comparison, corroboration-independence, and
invalidation-propagation subset of [A03](../delivery.md#a03-cited-document-research)
and [A08](../delivery.md#a08-additional-intelligence-methods) defined in
[`docs/contracts/document-lineage-and-correction.md`](../contracts/document-lineage-and-correction.md).

## Run locally

From the repository root with Rust 1.99.0 installed:

```text
cargo +1.99.0 run --manifest-path crates/oia-documents/Cargo.toml --example inspect-lineage --locked --offline -- fixtures/documents/lineage
```

Arguments: `[FIXTURE_ROOT] [MAX_NODES] [MAX_EDGES] [MAX_SEGMENTS]`. Defaults:
`fixtures/documents/lineage`, `200` nodes, `500` edges, and `100` comparison
segments. All budgets must be strictly positive (`LineageLimits::new`). The
command reads each synthetic original with a bounded reader (`65,536` bytes
plus one sentinel byte), verifies exact byte lengths and contract invariants,
and leaves the fixture files untouched.

## Kernel contract

`oia_documents::lineage` provides pure, standard-library-only validation and
graph algorithms with explicit `LineageLimits`, fallible allocations
(`Vec::try_reserve`), `unsafe_code = "forbid"`, and `unwrap_used` /
`expect_used` / `indexing_slicing` denied:

1. **Exact-byte duplicate grouping (`compute_exact_duplicate_groups`, `DOC-02`, `A03`):**
   - Groups retained `SourceRecordDescriptor`s by `ArtifactDescriptor.sha256`
     while preserving distinct `version_id`s and `source_id`s, ordering members
     deterministically by earliest verified UTC `publication_time` instant, then
     UTC `retrieval_time` instant, then lexical `version_id` (§2.2).
   - Rejects duplicate record versions within a group, collapsed `source_id`s
     (`ExactDuplicateCollapsedSourceId`), and identical SHA-256 digests mapped
     to conflicting artifact IDs (`IdenticalSha256MultipleArtifacts`).
   - Excludes `reference_only` records from byte-duplicate groups (`validate_records`
     enforces that `reference_only` records carry an explicit `omission_reason`,
     zero retained page blobs, unique positive page numbers, and valid RFC 3339
     timestamps).
2. **Version-bound UTF-8 byte citations and release/redaction comparison (`verify_utf8_byte_citation`, `verify_release_comparison`, `DOC-02`, `DOC-04`, `A03`):**
   - Verifies that every citation resolves to an artifact and page owned by the
     exact cited `record_version_id` (`CrossVersionCitationMismatch`), lands on
     valid UTF-8 character boundaries (`InvalidByteAnchorRange`), and matches
     the quoted substring byte-for-byte (`CitationQuoteMismatch`).
   - Verifies aligned release comparison segments (`UnchangedUnredacted`,
     `NewlyDisclosedInRight`, `NewlyRedactedInRight`, `DifferentlyRedacted`,
     `RevisedUnredacted`, and `CoverageGap`) across distinct document versions.
   - Rejects hidden cleartext inside redacted artifacts (`HiddenRedactionPayloadRejected`),
     checking both caller-declared `forbidden_cleartext_in_left` and
     segment-derived cleartext (`NewlyDisclosedInRight` text must not appear in
     the left page artifact; `NewlyRedactedInRight` text must not appear in the
     right page artifact). Note: the segment-derived substring scan operates on
     each segment's corresponding page artifact; multi-page cross-page scans or
     partial-occurrence exceptions where the same token legitimately remains
     unredacted elsewhere on the page are outside this kernel's scope.
3. **Iterative lineage SCC cycle detection, earliest-known-source view, and corroboration independence (`compute_lineage_sccs`, `evaluate_corroboration`, `INTEL-01`, `DOC-05`, `A08`):**
   - Runs deterministic, iterative Tarjan strongly connected components
     decomposition over `{Republishes, Quotes, Cites, Translates}` edges using
     an explicit heap-allocated work stack bounded by `max_nodes` and `max_edges`
     (no unbounded recursion).
   - Classifies each cycle as `UnanchoredCycle` (cannot reach a terminal,
     non-cyclic upstream root) or `AnchoredCycle` (can reach such a root, including
     through another cycle) and extracts a shortest deterministic closed cycle witness path
     (`v_0 -> ... -> v_0`) via BFS.
   - Computes `CorroborationSummary`: deduplicates claim inputs, separates
     `supporting_record_versions` from `excluded_derivative_runs`
     (`ExtractedClaim` / `AnalyticalInference`), identifies
     `circular_only_record_versions`, and builds transitive union-find
     equivalence classes across version chains
     (`{Supersedes, AlternateRedaction, PreprintVersion}`,
     `predecessor_version_id`, and shared document `id`), shared `source_id`, and
     shared-hash duplicate groups, emitting one earliest representative per
     equivalence class (`independent_supporting_roots`) ordered by earliest
     verified UTC `publication_time` instant, then UTC `retrieval_time` instant,
     then lexical `version_id`.
   - `evaluate_claim_corroboration` accepts `ClaimCorroborationInputs` and applies
     the same traversal and collapse to contradicting inputs, emitting
     `contradicting_independent_roots` separately;
     the two stances are never combined into a net truth score. Derivative runs
     supplied for either stance enter only `excluded_derivative_runs`.
     `evaluate_corroboration` remains as a positive-support compatibility wrapper.
   - Reports `exact_duplicate_group_count` as the number of distinct retained
     content hashes across direct supporting records, including singleton hashes;
     reference-only records are excluded. This is distinct from
     `compute_exact_duplicate_groups`, which lists only hashes shared by multiple
     records.
   - Scopes coverage to ancestry reached from supporting or contradicting inputs:
     unrelated restricted/reference-only/missing records and unrelated cycles do
     not appear. `unresolved_external_citations` are bounded, deterministic,
     caller-supplied `(citing_record_version_id, reference)` pairs filtered to
     reached records. Each reference is limited to 4,096 UTF-8 bytes before cloning,
     in addition to the citation-count bound; the kernel does not infer or scrape
     citations from prose.
   - Parses RFC 3339 timestamps (`parse_rfc3339_utc_nanos`) requiring strict
     ASCII digit fields and explicit timezone offsets into UTC instants while
     preserving original strings for display; preserves undated upstream roots
     (`publication_time: None` with mandatory `unknown_time_reason`) without
     inventing epoch/zero timestamps, reporting
     `EarliestSourceResolution::AmbiguousCandidates` whenever undated roots
     coexist with dated roots or when two independent roots tie for the earliest
     publication instant.
4. **Iterative acyclic derivation validation and invalidation propagation (`compute_invalidation_propagation`, `DOC-05`, `NEWS-05`, `A03`, `A06`):**
   - Verifies that `derived_from` links form a strict DAG using an iterative
     three-color DFS stack (no recursion), rejecting cycles with
     `DerivationCycleDetected` and counting `derived_from` links against
     `max_edges`.
   - Propagates `InvalidationNotice` (`SourceCorrected` for `Supersedes`,
     `SourceRetracted` for `RetractedBy` or in-place `lifecycle_state: Retracted`)
     transitively to all downstream derivations while preserving prior versions
     and `preserve_old_citations`.
   - Enforces that records with `availability: Removed` are never marked
     `lifecycle_state: Retracted` or targeted by a `RetractedBy` notice
     (`RemovedEndpointAutoRetracted`), while still permitting a removed old
     edition to be superseded by a `Supersedes` correction.

## Integration and remaining scope

This kernel implements the pure graph and validation logic from
[`docs/contracts/document-lineage-and-correction.md`](../contracts/document-lineage-and-correction.md)
inside the standalone `oia-documents` package. `CorroborationSummary` covers
positive and contradicting roots, retained supporting content-hash counts,
derivative exclusion, cycle handling, and claim-scoped coverage including
unresolved external citations. It does not infer claim stances or bibliographic
references from document prose. Still open on
[#19](https://github.com/mESmaC/oia/issues/19):

- Integration with P0 domain envelopes (`oia-domain`), persistent storage
  (`oia-store`), ingestion (`oia-ingest`), analysis (`oia-analysis`), and query
  services (`oia-query`) once `#10` and runtime `#17` merge.
- Workspace UI views for release comparisons and lineage inspection.
- End-to-end `A03` and `A08` integration acceptance checks.

## Local verification

```text
cargo +1.99.0 fmt --manifest-path crates/oia-documents/Cargo.toml --all -- --check
cargo +1.99.0 clippy --manifest-path crates/oia-documents/Cargo.toml --workspace --all-targets --locked --offline -- -D warnings
cargo +1.99.0 test --manifest-path crates/oia-documents/Cargo.toml --workspace --locked --offline
cargo +1.99.0 run --manifest-path crates/oia-documents/Cargo.toml --example inspect-lineage --locked --offline -- fixtures/documents/lineage
python -I scripts/fixtures/verify_document_lineage.py --self-test
```

Three targeted contract tests in `crates/oia-documents/tests/lineage_contract.rs`
verify positive `A03`/`A08` fixture expectations
(`lineage_fixture_satisfies_a03_and_a08_contracts`), claim-scoped stance and
coverage (`claim_stance_and_coverage_are_scoped_to_reached_ancestry`), and
adversarial contract violations
(`adversarial_lineage_violations_and_control_stops_are_rejected`: shifted and
mid-codepoint UTF-8 byte anchors, hidden cleartext inside redacted artifacts,
`derived_from` cycles, auto-retracted removed endpoints, anchored vs. unanchored
cycles, rootless cycle-to-cycle chains, downstream circular-only witnesses,
node-budget overflow, and cooperative cancellation at every callback boundary).
The stance test addresses derivative-evidence promotion and unrelated-record
metadata leakage; neither test is an integrated A08 acceptance run.
