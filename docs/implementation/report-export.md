# Cited report projection, redaction preview, and inert Markdown/HTML rendering kernel

Status: implemented as the in-memory report validation and rendering kernel in [`crates/oia-export/`](../../crates/oia-export/Cargo.toml); not yet wired into full case-bundle staging or workspace application services. Work package: [#22](https://github.com/mESmaC/oia/issues/22). Requirements: [COLLAB-02, COLLAB-03](../spec.md#71-cases-and-reports), [REPRO-02](../spec.md#72-reproducible-packages) (inert report handling subset), and [UX-05](../spec.md#32-visual-identity). Design and contract references: [ADR 0006 (D05)](../adr/0006-d05-report-and-bundle-formats.md) and [report export contract](../contracts/report-export.md) sections 1–4. Acceptance references: [A16](../delivery.md#a16-report-collaboration-and-sharing) and [A17](../delivery.md#a17-portable-evidence-and-replay), covering only the in-memory report projection, redaction/omission preview, citation validation, and inert Markdown/HTML rendering subset.

## Scope and boundaries

`crates/oia-export/` is a standard-library-only Rust workspace crate (`edition.workspace = true`, `rust-version.workspace = true`, `unsafe_code = "forbid"`, `unwrap_used = "deny"`, `expect_used = "deny"`, `indexing_slicing = "deny"`). It validates caller-supplied, version-pinned report snapshots (`preview_report`) and renders the resulting `VerifiedReportProjection` into UTF-8 Markdown (`render_markdown`) and self-contained UTF-8 HTML (`render_html`). No unmerged prerequisite code is copied.

What this kernel **does not** do:

- It does not read or write user files, stage directory bundles, or perform atomic filesystem moves (`docs/contracts/report-export.md` §§5–6 remain open on #22).
- It does not authenticate users, manage workspace roles, or hold storage revision leases; callers must authorize the workspace projection before calling `preview_report` and recheck the commit lease before publishing output.
- It does not automatically discover sensitive material, redact raw text, or assess truth; it checks the caller-supplied snapshot, citation anchors, disclosure scopes, and dependency links for consistency and fail-closed redaction rules.
- Certain checks rely on caller-supplied metadata flags: `UnadjustedRedactedAnchor` fires when `is_unadjusted_original_offset: true` is passed on a redacted derivative's `Utf8ByteRange`, and `SensitiveConclusionRequiresRedaction` covers entries listed in `sensitive_dependency_entry_ids` (a conclusion block that omits citations and omits `sensitive_dependency_entry_ids` renders as `[UNCITED]`).
- Local path and external URL checks reject common local path forms and common loopback, private-network, `.local` / `.internal`, and credential/token query forms rather than enforcing a full network parser or allowlist.
- No performance benchmark, live provider integration, or Lean theorem / Rust-to-Lean correspondence is claimed.

## Run locally

From the repository root using Rust 1.99.0:

```text
cargo +1.99.0 run -p oia-export --example render-a03-report --locked --offline -- --summary
cargo +1.99.0 run -p oia-export --example render-a03-report --locked --offline -- --markdown
cargo +1.99.0 run -p oia-export --example render-a03-report --locked --offline -- --html
```

The read-only `render-a03-report` example accepts `[--summary | --markdown | --html] [MAX_OUTPUT_BYTES]`. It borrows the canonical synthetic [`fixtures/documents/a03/originals/`](../../fixtures/documents/a03/README.md) UTF-8 artifacts in place, constructs a synthetic version-pinned report snapshot with original, mirror republication, corrected, conflicting, archive redacted release A, reference-only, adversarial, and existence-sensitive entries, runs `preview_report`, and writes the requested summary or rendered document to standard output. Passing a tight output byte ceiling (for example `--markdown 128`) demonstrates deterministic `LimitExceeded` failure before full output generation.

## Kernel contract

### 1. Explicit budgets and cooperative stop control

- `ReportLimits::new(max_blocks, max_citations, max_evidence_entries, max_text_bytes, max_output_bytes)` rejects zero in any dimension (`ReportError::ZeroLimit`).
- `preview_report` enforces `max_evidence_entries`, `max_blocks`, `max_citations`, and `max_text_bytes` across metadata, identifiers, retained UTF-8 target bytes, statement text, notes, and citation metadata before allocating or rendering.
- `render_markdown` and `render_html` enforce `max_output_bytes` on every append (`push_str_bounded`) and reserve capacity fallibly (`String::try_reserve`, `Vec::try_reserve`).
- All three entry points accept a synchronous callback `FnMut() -> Result<(), StopReason>` (`Cancelled`, `DeadlineExceeded`, `Superseded`) checked at entry, per evidence entry, per block, per citation, per report section, and immediately before returning.

### 2. Version-pinned report projection and exact citation validation (`COLLAB-02`)

- Every projection renders all ten `COLLAB-02` sections (`ReportSection::ALL`): `QuestionAndScope`, `Coverage`, `Chronology`, `SourceLineage`, `ContextAndMedia`, `Hypotheses`, `SupportingEvidence`, `ConflictingEvidence`, `BoundedConclusions`, and `OpenQuestions`. Sections with no blocks emit `_No material selected for this section._` (Markdown) / `<p class="EmptySection"><em>No material selected for this section.</em></p>` (HTML) and never fabricate findings.
- Statements retain stable `block_id`, `statement_id`, `actor_attribution`, `origin_class` (`Observation`, `SourceAssertion`, `Assumption`, `Inference`, `Forecast`, `Generated`, `Simulated`), `review_status` (`Unreviewed`, `InReview`, `Reviewed`, `Challenged`, `Superseded`), optional `chronology_time`, `uncertainty_note`, and `source_limitation_note`.
- Different evidence entries with identical bytes (such as `report-original` and `report-republication` in `A03`) retain distinct `entry_id`, `citation_label`, `source_id`, and `source_record_version_id` values.
- Citation anchors (`CitationAnchor`) are checked against the pinned target entry while keeping original and extraction coordinate spaces separate:
  - `Utf8ByteRange { start, end, is_unadjusted_original_offset }`: when the target entry is `Available` with `FullTarget` disclosure, the kernel validates `retained_utf8_bytes` as UTF-8, verifies `start < end <= len` and UTF-8 scalar boundaries (`is_char_boundary`), and borrows the exact slice from the original buffer (`ValidatedUtf8Slice`) without recalculating offsets against normalized text.
  - `ExtractedPassage { extraction_version_id, passage_line, byte_start, byte_end }`: because an extraction offset is not an offset into the original artifact bytes (`docs/contracts/report-export.md` §2) and `EvidenceInventoryEntry` does not carry separate extraction buffers, structural bounds (`passage_line >= 1`, `byte_start < byte_end`, non-empty `extraction_version_id`) are checked and the anchor is recorded as `UnvalidatedExtractedPassage` (`[ANCHOR UNVALIDATED (extracted passage): ...]`) rather than certified against original bytes.
  - When a cited entry has `MetadataOnly` disclosure (`ReferenceOnly`, `Missing`, `Restricted`, `Removed`, or `Unsupported`), the anchor is recorded as `UnvalidatedTargetUnavailable` with its availability and omission reason rather than certified as validated.
  - `PageRegion` records an explicit `UnvalidatedPageRegion` state; `UnsupportedAnchor` requires non-empty `kind` and `limitation_note` and renders `[UNSUPPORTED ANCHOR LIMITATION: ...]`.

### 3. Redaction preview and recipient disclosure isolation (`COLLAB-03`)

- Each `EvidenceInventoryEntry` declares `RecipientDisclosure`:
  - `FullTarget`: requires `TargetAvailability::Available`, no `omission_reason`, and either `retained_utf8_bytes` or `artifact_id`.
  - `MetadataOnly`: requires `retained_utf8_bytes.is_none()` and an explicit `OmissionReason` (`NotSelected`, `RetentionProhibited`, `RedistributionProhibited`, `Missing`, `Removed`, `Restricted`, `Unsupported`).
  - `GenericCoverageOnly`: omitted from `visible_inventory` and cannot bundle bytes or external URLs; counted in `generic_coverage_omissions`. Whenever `generic_coverage_omissions > 0` (even if `generic_omission_note` is `None`), both Markdown and HTML emit an explicit **Omission Coverage Notice** header entry so generic omissions never disappear silently.
  - `ExistenceSensitive`: completely omitted from `visible_inventory`, rendered outputs, and `Debug` representations of `VerifiedReportProjection`. Any statement citing an `ExistenceSensitive` or `GenericCoverageOnly` entry is rejected (`ExistenceSensitiveReferenceLeak`).
- Redacted derivatives (`redacted_from_entry_id: Some(orig_id)`):
  - Must reference a declared entry in the snapshot with a distinct `entry_id`.
  - If the original entry has `FullTarget` disclosure (whether via `retained_utf8_bytes` or `artifact_id`), `preview_report` fails (`DisclosurePolicyViolation`: an excluded original entry cannot be disclosed as `FullTarget` alongside its redacted derivative).
  - If the original entry is `ExistenceSensitive` or `GenericCoverageOnly`, the derivative's `is_redacted_derivative` flag is preserved while `visible_redacted_from_entry_id` is suppressed (`None`) so the sensitive original's identifier never leaks into the projection or rendered report.
  - Applying an unadjusted original byte range (`is_unadjusted_original_offset: true`) to a redacted derivative fails with `UnadjustedRedactedAnchor`.
- Sensitive conclusion guard: if any block lists a `sensitive_dependency_entry_ids` target that is not `Available` + `FullTarget`, `preview_report` fails with `SensitiveConclusionRequiresRedaction` rather than laundering the conclusion into an unsupported statement.
- Common local filesystem path forms (`C:\...`, `/Users/...`, `/home/...`, `\\...`, `file://`) in metadata or text fail with `LocalPathDisclosed`. External reference URLs must use explicit `http://` or `https://` schemes, contain no credentials/userinfo (`@`) or common secret/token query parameters, and reject common loopback, private-network, or `.local` / `.internal` host forms (`UnsafeExternalUrl`).

### 4. Legible, inert Markdown and self-contained HTML (`UX-05`, `REPRO-02`)

- Both renderers consume the same `VerifiedReportProjection` and emit explicit plain-text labels for `[ORIGIN: ...]`, `[REVIEW: ...]`, `[REASSESSMENT: CURRENT]` / `[REASSESSMENT REQUIRED]`, `[UNCERTAINTY]`, `[SOURCE LIMITATION]`, target availability, and citation validation states so neither color nor transparency is the sole indicator.
- Identifier and code-span values in Markdown are emitted through CommonMark code spans (`push_markdown_code_span` / `push_markdown_code_pair`) with raw content (normalizing only newlines and escaping control/bidi/zero-width characters) and a backtick fence longer than any backtick run in the content, ensuring identifiers containing `_` or punctuation match HTML `<code>` output identically.
- Chronology blocks require an explicit `TimeDisplay`; `TimePrecision::DateOnly` rejects strings containing `'T'` or `':'` (`InvalidTimeDisplay`) so date-only accounts are never expanded into midnight timestamps.
- `render_markdown` escapes `&`, `<`, `>`, `\`, `` ` ``, `*`, `_`, `[`, `]`, `(`, `)`, `!`, `#`, `~`, `|`, `:`, `.`, `@`, newlines, control characters, and Unicode bidi/zero-width formatting characters in untrusted text and quoted passages so quoted evidence cannot inject raw HTML, remote images, links, GFM autolinks (`https://...`, `www....`), or structural headings.
- `render_html` produces a single UTF-8 HTML5 document with a restrictive `Content-Security-Policy` meta tag (`default-src 'none'; style-src 'unsafe-inline'; img-src data:; form-action 'none'; base-uri 'none'`), renderer-owned inline styles, keyboard `:focus-visible` outlines, `@media (prefers-reduced-motion: reduce)` / `(prefers-reduced-transparency: reduce)`, `@media print` high-contrast rules, visible `<code>` URL text alongside external reference links for print legibility, exporter-owned opaque `#cite-N` and `#stmt-N` fragment IDs, and strict HTML + bidi/control escaping across all text and attributes.

## Integration still required on #22

Keep #22 open and **In Progress**. Completing #22 requires merging #11, #16, #18, and #19 and implementing:

- Shared application service authorization, revision leases, and distinct Save / Export Privately / Share / Publish commands;
- Finalized wire schema and bounded, staged directory bundle export and atomic import (`docs/contracts/report-export.md` §§5–6), including symlink/reparse/traversal rejection and SHA-256 staging verification;
- Workspace UI report-drafting and statement-to-evidence navigation;
- Full end-to-end A16 and A17 acceptance verification.

## Local verification

```text
cargo +1.99.0 fmt --all -- --check
cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings
cargo +1.99.0 test --workspace --locked --offline
cargo +1.99.0 run -p oia-export --example render-a03-report --locked --offline -- --summary
python -I scripts/fixtures/verify_a03.py
```

### Executed evidence

Executed locally on 2026-11-02 (America/New_York), Windows x86_64/MSVC, Rust/Cargo 1.99.0, Python 3.12, at repository base `9594277`:

- `cargo +1.99.0 fmt --all -- --check`: passed.
- `cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings`: passed with zero warnings across workspace crates, example, and integration tests.
- `cargo +1.99.0 test --workspace --locked --offline`: passed across the workspace, including 4 targeted contract tests in `oia-export` (`preserves_a03_versions_labels_anchors_and_distinct_duplicate_byte_citations`, `enforces_redaction_preview_existence_sensitive_isolation_and_conclusion_guards`, `neutralizes_hostile_markup_and_rejects_local_paths_and_unsafe_urls`, `enforces_resource_budgets_date_only_precision_and_cooperative_stops`).
- `cargo +1.99.0 run -p oia-export --example render-a03-report --locked --offline`: verified `--summary`, `--markdown`, `--html`, and output-budget failure modes; confirmed original `fixtures/documents/a03/` SHA-256 digests remain unchanged via `python -I scripts/fixtures/verify_a03.py`.
