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
     while preserving distinct `version_id`s and `source_id`s.
   - Rejects duplicate record versions within a group, collapsed `source_id`s
     (`ExactDuplicateCollapsedSourceId`), and identical SHA-256 digests mapped
     to conflicting artifact IDs (`IdenticalSha256MultipleArtifacts`).
   - Excludes `reference_only` records from byte-duplicate groups (`validate_records`
     enforces that `reference_only` records carry an explicit `omission_reason`
     and zero retained page blobs).
2. **Version-bound UTF-8 byte citations and release/redaction comparison (`verify_utf8_byte_citation`, `verify_release_comparison`, `DOC-02`, `DOC-04`, `A03`):**
   - Verifies that every citation resolves to an artifact and page owned by the
     exact cited `record_version_id` (`CrossVersionCitationMismatch`), lands on
     valid UTF-8 character boundaries (`InvalidByteAnchorRange`), and matches
     the quoted substring byte-for-byte (`CitationQuoteMismatch`).
   - Verifies aligned release comparison segments (`UnchangedUnredacted`,
     `NewlyDisclosedInRight`, `NewlyRedactedInRight`, `DifferentlyRedacted`,
     `RevisedUnredacted`, and `CoverageGap`) across distinct document versions.
   - Rejects any redacted artifact that contains hidden cleartext declared in
     `forbidden_cleartext_in_left` (`HiddenRedactionPayloadRejected`).
3. **Lineage SCC cycle detection, earliest-known-source view, and corroboration independence (`compute_lineage_sccs`, `evaluate_corroboration`, `INTEL-01`, `DOC-05`, `A08`):**
   - Runs deterministic Tarjan strongly connected components decomposition over
     `{Republishes, Quotes, Cites, Translates}` edges within `max_nodes` and
     `max_edges` budgets.
   - Classifies each cycle as `UnanchoredCycle` (no outbound lineage edge
     leaving the cycle) or `AnchoredCycle` (cites an external upstream record)
     and extracts a deterministic cycle witness path.
   - Computes `CorroborationSummary`: separates `supporting_record_versions`
     from `excluded_derivative_runs` (`ExtractedClaim` / `AnalyticalInference`),
     identifies `circular_only_record_versions`, collapses transitive copies and
     shared-hash upstream roots into `independent_supporting_roots`, and builds
     `EarliestKnownSourceView`.
   - Preserves undated upstream roots (`publication_time: None` with mandatory
     `unknown_time_reason`) without inventing epoch/zero timestamps, reporting
     `EarliestSourceResolution::AmbiguousCandidates` whenever undated roots
     coexist with dated roots, alongside `omitted_reference_only_records` and
     `detected_cycle_count`.
4. **Acyclic derivation validation and invalidation propagation (`compute_invalidation_propagation`, `DOC-05`, `NEWS-05`, `A03`, `A06`):**
   - Verifies that `derived_from` links form a strict DAG, rejecting cycles with
     `DerivationCycleDetected`.
   - Propagates `InvalidationNotice` (`SourceCorrected` for `Supersedes`,
     `SourceRetracted` for `RetractedBy`) transitively to all downstream
     derivations while preserving prior versions and `preserve_old_citations`.
   - Enforces that records with `availability: Removed` are never auto-marked
     `lifecycle_state: Retracted` (`RemovedEndpointAutoRetracted`).

## Integration and remaining scope

This kernel implements the pure graph and validation logic from
[`docs/contracts/document-lineage-and-correction.md`](../contracts/document-lineage-and-correction.md)
inside the standalone `oia-documents` package. Still open on
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

Two targeted contract tests in `crates/oia-documents/tests/lineage_contract.rs`
verify positive `A03`/`A08` fixture expectations (`lineage_fixture_satisfies_a03_and_a08_contracts`)
and adversarial contract violations (`adversarial_lineage_violations_and_control_stops_are_rejected`:
shifted and mid-codepoint UTF-8 byte anchors, hidden cleartext inside redacted
artifacts, `derived_from` cycles, auto-retracted removed endpoints, anchored vs.
unanchored cycles, node-budget overflow, and cooperative cancellation at every
callback boundary).
