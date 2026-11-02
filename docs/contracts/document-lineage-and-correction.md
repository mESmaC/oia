# Document Release Comparison, Source Lineage, and Correction Propagation Contract

Status: proposed domain and application-service contract for [issue #19](https://github.com/mESmaC/oia/issues/19). This document specifies data structures, invariants, failure behavior, and fixture verification rules for `DOC-02`, `DOC-04`, `DOC-05`, and `INTEL-01` across acceptance scenarios `A03` and `A08`. It does not claim that a Rust runtime workspace, persistence engine, or Lean proof has been executed in this slice.

## 1. Scope and requirement traceability

| Requirement / Scenario | Contract obligation in this specification |
| --- | --- |
| `DOC-02` | Deduplicate exact-byte payloads in storage while keeping separate `SourceRecord` identities, editions, releases, differently redacted copies, and preprint/arXiv versions with explicit relationships. |
| `DOC-04` | Support release/redaction comparisons and cross-document references where every citation resolves to the exact `SourceRecord` version and UTF-8 byte or page/region anchor used. |
| `DOC-05` | Track model/configuration versions, generation timestamps, human review states, source corrections, and `needs_reassessment` invalidation without mutating prior outputs; keep document text isolated as untrusted input. |
| `INTEL-01` | Trace source lineage across republication and quotation chains, detect copied accounts and circular references, resolve earliest-known-source candidates with coverage qualifications, and compute corroboration independence without inflating repeated accounts. |
| `A03` | Verify duplicate bytes under distinct sources, revised editions, differently redacted releases, preserved historical citations, and downstream invalidation after correction. |
| `A08` | Verify circular-citation detection, reversible relationship typing, and corroboration accounting over copied and cyclic accounts. |

## 2. Exact-byte duplicates versus distinct source records

Per [Architecture §2.1–2.2](../architecture.md#2-identity-versions-and-evidence), content hashes (`Artifact.sha256` and `byte_length`) identify retained bytes, not bibliographic or provenance identity.

### 2.1 Storage deduplication and identity separation

1. **Shared artifact, distinct records:** When two acquisitions yield identical bytes (`sha256` and `byte_length` match bitwise), they may reference the same content-addressed `Artifact` record, but each acquisition must create a distinct `SourceRecord` with its own opaque `id`, immutable `version_id`, `source_id`, `provider_version`, acquisition timestamps, and per-source reuse conditions (`access`, `automated_processing`, `retention`, `redistribution`).
2. **Exact-duplicate grouping (`ExactDuplicateGroup`):**
   - Groups `SourceRecord` versions that reference the same `Artifact.sha256` within the caller's authorized workspace/library scope.
   - Exposes `artifact_id`, `sha256`, `byte_length`, `member_record_versions` (ordered deterministically by earliest verified `publication_time`, then `retrieval_time`, then lexical `version_id`), and `recorded_relations` (`republishes` when directionality is evidenced, or `exact_byte_match_unconfirmed_direction` when two archives supply identical bytes without established priority).
3. **Workspace isolation:** Cross-workspace blob deduplication at the storage layer must never leak the existence, metadata, or annotations of a `SourceRecord` from an unauthorized workspace into an `ExactDuplicateGroup`.

## 3. Release, revision, and redaction comparison

Public disclosures, FOIA releases, and preprints frequently appear in multiple editions (`report-v1` vs. `report-v2`) or differently redacted releases (`release-a` vs. `release-b`).

### 3.1 Versioned release relation kinds

Every release relationship is a versioned record with its own `origin_class` (`acquired_source_material`, `analytical_inference`, or `user_testimony`) and `review_status` (`unreviewed`, `in_review`, `reviewed`, `challenged`, `superseded`):

- `supersedes`: `from` is a later edition, erratum, or corrected revision of `to`.
- `retracted_by`: `from` is an official retraction notice or retracted version superseding `to`.
- `alternate_redaction`: `from` and `to` represent distinct redaction releases of the same underlying document (for example, releases under different agency reviews or subsequent declassification cycles).
- `preprint_version`: `from` and `to` are sequential preprint or journal versions (such as `arXiv v1` and `arXiv v2`).

### 3.2 Redaction and edition comparison (`ReleaseRedactionComparison`)

Comparing two document versions (`left_record_version_id` and `right_record_version_id`) produces an inspectable diff bound to both exact versions:

- Each aligned segment records:
  - `page`: one-based page number,
  - `left_byte_range`: half-open UTF-8 byte interval `[start, end)` in `left` (or `null` if page is missing/omitted in `left`),
  - `right_byte_range`: half-open UTF-8 byte interval `[start, end)` in `right` (or `null` if page is missing/omitted in `right`),
  - `segment_kind`:
    - `unchanged_unredacted`: byte-identical unredacted text in both releases,
    - `newly_disclosed_in_right`: redacted marker/region in `left` replaced by unredacted text in `right`,
    - `newly_redacted_in_right`: unredacted text in `left` replaced by a redaction marker/region in `right` (e.g., conflicting agency withholdings),
    - `differently_redacted`: both `left` and `right` redact the passage, but with different exemption markers or span widths,
    - `revised_unredacted`: both `left` and `right` contain unredacted text that differs between editions,
    - `coverage_gap`: one or both pages are `unreadable`, `missing`, or `reference_only`.
- **No hidden text behind redactions (`DOC-02`, `A03`):** A redacted artifact contains only its released bytes. OIA must never store hidden cleartext inside a redacted artifact fixture or infer redacted text without citing the specific alternate `SourceRecord` version where that passage is unredacted.

## 4. Source-lineage graphs, circular citations, and earliest-known-source views

### 4.1 Lineage edge semantics

Lineage edges track how assertions and passages propagate across records:

- `republishes`: `from` copies or syndicates `to` in whole or substantial part.
- `quotes`: `from` quotes a specific passage of `to` (with optional `from_anchor` and `to_anchor` UTF-8 byte ranges).
- `cites`: `from` references `to` as a source for a claim or observation without verbatim quotation.
- `translates`: `from` translates a passage of `to` across languages (`INTEL-04`).

Note on edge direction: In OIA manifests and domain records (`fixtures/documents/a03/manifest.json`), `{"from": A, "to": B, "kind": "republishes" | "quotes" | "cites"}` means record `A` derives from, quotes, or cites record `B` (so `B` is the upstream source and `A` is the downstream consumer).

### 4.2 Cycle detection in lineage graphs (`INTEL-01`, `A08`)

Unlike analysis derivation graphs (which must be strictly acyclic), real-world document and news lineage graphs can contain cycles (for example, secondary summary `A` cites digest `B`, which cites article `C`, which cites `A`).

1. **Bounded SCC decomposition:** Lineage evaluation runs deterministic Tarjan strongly connected components (SCC) decomposition over `{republishes, quotes, cites, translates}` edges within configured graph budgets (`max_lineage_nodes`, `max_lineage_edges`).
2. **Cycle classification:**
   - An SCC with more than one record, or a single record with a self-edge, is marked `circular_reference_detected = true` with a deterministic cycle witness path `(v_0 -> v_1 -> ... -> v_0)`.
   - **Unanchored circular cluster (`unanchored_cycle`):** If an SCC has no outbound lineage edge to any upstream record outside the SCC, none of its members may be counted as an independent primary source; the entire SCC is reported as a mutually citing cycle requiring external corroboration.
   - **Anchored cycle (`anchored_cycle`):** If members of a cycle also cite an external upstream record `R` outside the cycle, lineage tracing resolves `R` as the upstream root while preserving the `circular_reference_detected` warning on the downstream loop.

### 4.3 Earliest-known-source view (`INTEL-01`, `GEO-01`)

For any claim, event, or document cluster, the earliest-known-source view traces upstream along `{republishes, quotes, cites, translates}` edges and reports:

1. **`upstream_roots`:** Terminal upstream `SourceRecord` versions (outside unanchored cycles) that do not republish, quote, or cite another known record for the proposition.
2. **Temporal ordering without invented timestamps (`Architecture §3.1`):**
   - Candidate roots are compared using explicit `event_time` and `publication_time` intervals/instants.
   - Records with `publication_time: null` (with mandatory `unknown_time_reason`) or calendar-only dates retain their uncertainty; unknown timestamps are never coerced to `0`, epoch, or midnight UTC to force a total order.
   - When temporal uncertainty or missing publication dates prevent a strict earliest winner among roots, the view returns `earliest_resolution: "ambiguous_candidates"` listing all non-dominated candidates rather than guessing a single winner.
3. **Mandatory `coverage_qualification`:** Every earliest-known-source result carries:
   - `scope`: `"retained_and_cataloged_workspace_records"`,
   - `omitted_reference_only_records`: IDs of known upstream records whose payloads are `reference_only`, `restricted`, or `missing`,
   - `unresolved_external_citations`: cited bibliographic references not yet ingested as `SourceRecord`s,
   - `detected_cycles`: all cycle witness paths encountered during upstream traversal.

## 5. Corroboration independence accounting

Per `AGENTS.md §2` and `Spec §2 rule 8`, multiple articles repeating a source or multiple model runs agreeing on a summary are not independent corroboration.

A `CorroborationSummary` for a proposition or claim must compute and expose:

| Field | Definition |
| --- | --- |
| `supporting_record_versions` | All `SourceRecord` versions linked as supporting the claim. |
| `exact_duplicate_group_count` | Count of distinct content hashes (`Artifact.sha256`) among retained supporting records; `reference_only` records are tracked separately. |
| `independent_supporting_roots` | Upstream lineage roots reached from supporting records after collapsing all `{republishes, quotes, cites, translates}` copy/citation chains, collapsing `{supersedes, alternate_redaction, preprint_version}` version chains and shared document/`source_id` identities (selecting the earliest verified publication instant, then retrieval instant, then lexical `version_id`), collapsing shared-hash duplicate groups, and excluding unanchored citation cycles. |
| `independent_root_count` | `len(independent_supporting_roots)`. Only records with distinct upstream sources, distinct document version chains, distinct content hashes, and non-circular lineage contribute to this count. |
| `circular_only_record_versions` | Supporting records whose upstream lineage terminates solely in a citation cycle (`unanchored_cycle`). |
| `contradicting_independent_roots` | Upstream lineage roots of records linked via `contradicts`. |
| `excluded_derivative_runs` | Inferences, AI summaries (`origin_class = analytical_inference`), and simulations, which are never counted as source corroboration. |

## 6. Correction, retraction, and removal propagation

### 6.1 Distinct source lifecycle states (`DOC-05`, `NEWS-05`, `A03`, `A06`)

1. **Source correction / revision (`supersedes`):** A new `SourceRecord` version replaces an earlier edition. The prior version remains stored, immutable, and addressable by its `version_id`; citations to the prior version continue to resolve to its exact UTF-8 byte range.
2. **Retraction (`retracted_by` or retraction state):** An explicit publisher retraction marks the prior version's lifecycle status as `retracted` while keeping its retained bytes and citations inspectable for audit.
3. **Removed or unavailable endpoint (`availability = removed | unavailable`):** When a subsequent fetch finds a deleted web page or offline endpoint without an explicit retraction notice, OIA records the availability change on the source health/check log. It must **never** automatically classify a removed page as `retracted` or falsified (`Architecture §2.4`).

### 6.2 Acyclic derivation invalidation (`needs_reassessment`)

1. **DAG invariant:** Derivation links (`derived_from`) connecting `SourceRecordVersion` $\to$ `ExtractedTextVersion` $\to$ `ExtractedClaimVersion` / `CitedSummaryVersion` / `AnalysisRunVersion` $\to$ `ReportVersion` must form a strict directed acyclic graph (DAG). Any attempt to insert a cycle in `derived_from` fails validation with `DerivationCycleDetected`.
2. **Propagation rule:** When a `SourceRecord` version is superseded by a correction (`supersedes`), retracted (`retracted_by`), or tombstoned under a retention/deletion policy:
   - Every transitive downstream node in the `derived_from` DAG that pinned the affected `version_id` transitions to `reassessment_state = "needs_reassessment"`.
   - Each affected downstream node records an immutable `InvalidationNotice` containing:
     - `trigger_record_id` and `trigger_version_id`,
     - `replacement_record_id` and `replacement_version_id` (when triggered by `supersedes` or `retracted_by`),
     - `reason` (`"source_corrected"`, `"source_retracted"`, or `"source_deleted_tombstone"`),
     - `citation_anchors_preserved`: `true` for corrections/retractions (where original bytes remain retained), `false` only when bytes are tombstoned.
   - Frozen `AnalysisRun`s, `CitedSummary` drafts, and issued `Report` versions are **never** overwritten or silently recomputed in place. Reassessment produces a new version linked via `supersedes`.

## 7. Failure behavior and resource bounds

Implementations of this contract must enforce explicit bounds before allocation or graph traversal (`AGENTS.md §5`):

- `InvalidByteAnchorRange`: UTF-8 byte anchor fails `0 <= start < end <= byte_length`, splits a multi-byte UTF-8 codepoint, or does not match the quoted text in the target `Artifact`.
- `CrossVersionCitationMismatch`: A citation references a `record_id` whose `record_version_id` or `artifact_id` does not belong to that exact version.
- `HiddenRedactionPayloadRejected`: A redacted artifact or diff claims hidden cleartext inside a redaction span without a separate unredacted `SourceRecord` version.
- `DerivationCycleDetected`: A `derived_from` relation introduces a cycle among analysis, summary, claim, or report versions.
- `LineageGraphBudgetExceeded`: Node or edge count exceeds configured traversal bounds (`max_lineage_nodes`, `max_lineage_edges`).
- `UntrustedInstructionRejected`: Source text or redaction labels attempting to invoke tools, network fetches, or permission changes are treated strictly as inert quoted strings (`DOC-05`).

## 8. Fixture verification in this repository

The companion fixture corpus in [`fixtures/documents/lineage/`](../../fixtures/documents/lineage/README.md) and the standard-library validator [`scripts/fixtures/verify_document_lineage.py`](../../scripts/fixtures/verify_document_lineage.py) exercise these exact invariants offline (and cross-check compatibility with [`fixtures/documents/a03/manifest.json`](../../fixtures/documents/a03/manifest.json)):

```text
python -I scripts/fixtures/verify_document_lineage.py --self-test
```

This verifies the fixture corpus and reference contract logic only; runtime Rust crates remain pending P0 (#10) and P1 (#17) integration.
