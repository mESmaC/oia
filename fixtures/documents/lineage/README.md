# Synthetic document lineage, release-comparison, and correction-propagation corpus

This fixture corpus is development input for [issue #19](https://github.com/mESmaC/oia/issues/19),
the [document lineage and correction contract](../../../docs/contracts/document-lineage-and-correction.md),
requirements [`DOC-02`/`DOC-04`/`DOC-05`](../../../docs/spec.md#41-public-document-library) and
[`INTEL-01`](../../../docs/spec.md#45-reasoning-and-additional-intelligence-methods), and the
[`A03`](../../../docs/delivery.md#a03-cited-document-research) and
[`A08`](../../../docs/delivery.md#a08-additional-intelligence-methods) acceptance scenarios.

Every document, source identity, redaction span, citation cycle, and timestamp in this directory is
synthetic (`classification: "simulation"`). None is an observation, a real government disclosure, or
evidence for a historical event. All `.invalid` URLs are offline identifiers; no network fetch is
required or permitted.

## Contents and contracts exercised

| Input / Record set | Contract exercised |
| --- | --- |
| `dispatch-primary-v1` and `dispatch-mirror-v1` (`dispatch-root-v1.txt`) | Exact-byte duplicate grouping (`DOC-02`): identical `SHA-256` bytes are shared across two distinct `SourceRecord` versions (`archive-alpha` and `mirror-beta`) while preserving separate source identities and the `republishes` relation. |
| `dispatch-primary-v2` (`dispatch-root-v2-corrected.txt`) | Source correction (`DOC-05`, `A03`): supersedes `dispatch-primary-v1` with updated bytes (`054 deg` vs. `045 deg`), preserves version-bound UTF-8 byte-range citations to `dispatch-primary-v1`, and marks transitive downstream claims, summaries, and issued reports `needs_reassessment`. |
| `regional-digest-v1` (`syndicated-quote-v1.txt`) | Multi-hop quotation lineage (`INTEL-01`): quotes `dispatch-mirror-v1`, which republishes `dispatch-primary-v1`. Collapses to the single upstream root `dispatch-primary-v1` rather than inflating corroboration counts. |
| `harbor-radar-log-v1` (`independent-log-v1.txt`) | Undated independent primary record (`INTEL-01`, `Architecture §3.1`): provides a second independent corroboration root (`independent_root_count = 2`) with `publication_time: null` and explicit `unknown_time_reason`, forcing `earliest_known_source` resolution to report `ambiguous_candidates` rather than inventing a timestamp. |
| `restricted-deck-log-v1` (no retained payload) | `reference_only` retention omission (`DOC-01`, `INTEL-01`): carries no page bytes (`retention: "reference_only"`) and appears in `coverage_qualification.omitted_reference_only_records`. |
| `cycle-memo-a-v1`, `cycle-bulletin-b-v1`, `cycle-review-c-v1` | Circular citation detection (`INTEL-01`, `A08`): forms a 3-node citation cycle (`A -> B -> C -> A`) with no external upstream root (`unanchored_cycle`). Detected deterministically via Tarjan SCC decomposition and contributes `0` independent corroboration roots. |
| `foia-release-2024-v1`, `foia-release-2026-v1`, `foia-release-partner-v1` | Release and redaction comparison (`DOC-02`, `DOC-04`, `INTEL-04`): exercises `unchanged_unredacted`, `newly_disclosed_in_right`, `newly_redacted_in_right`, and `differently_redacted` UTF-8 byte ranges, and verifies that redacted artifacts contain zero hidden cleartext behind redaction markers. |
| `retracted-advisory-v1` and `retraction-notice-v2` | Explicit retraction propagation (`DOC-05`, `INTEL-01`): `retraction-notice-v2` retracts `retracted-advisory-v1` (`retracted_by`), keeping the retracted version inspectable while flagging dependent `claim-sensor-fault-v1` as `needs_reassessment` (`reason: "source_retracted"`). |
| `removed-endpoint-v1` (`removed-endpoint-v1.txt`) | Removed vs. retracted separation (`DOC-05`, `NEWS-05`, `A06`): records `availability: "removed"` without marking `lifecycle_state: "retracted"` or triggering retraction invalidation on `summary-foia-overview-v1`. |

All 13 original text files begin with `SYNTHETIC LINEAGE FIXTURE — NOT OBSERVED EVIDENCE\n`
(containing the 3-byte UTF-8 em-dash `—`, `0xE2 0x80 0x94`) so UTF-8 byte offsets differ from
character offsets. `.gitattributes` disables Git text conversion on `originals/**` and enforces LF on
`manifest.json`.

## Local verification

From the repository root, using Python 3.12 (standard library only):

```text
python -I scripts/fixtures/verify_document_lineage.py --self-test
```

The validator checks:
- `.gitattributes` rules and absence of CRLF in `manifest.json` and `originals/**`,
- bounded reads, path confinement under `originals/`, exact byte lengths, and `SHA-256` digests,
- exact-byte duplicate grouping and preservation of distinct `SourceRecord` identities,
- UTF-8 byte-range citation anchors across original, corrected, syndicated, independent, and retracted records,
- release/redaction diff segments and absence of hidden cleartext in redacted artifacts,
- Tarjan SCC cycle detection, exclusion of `analytical_inference` summaries from corroboration counts, collapse of `republishes`/`quotes` chains, and earliest-known-source coverage qualifications,
- strict DAG acyclicity on `derived_from` and transitive `needs_reassessment` propagation for corrections and retractions while keeping `availability: "removed"` distinct from `retracted`,
- read-only compatibility with [`fixtures/documents/a03/manifest.json`](../a03/manifest.json),
- five in-memory adversarial negative tests (`--self-test`).

## Scope and limitations

This corpus and validator establish the data contract and deterministic fixtures for #19. Runtime
Rust implementation (`oia-domain`, `oia-ingest`, `oia-analysis`, `oia-query`), persistent storage,
and full end-to-end `A03`/`A08` acceptance depend on #10 and #17 and are not claimed here.
