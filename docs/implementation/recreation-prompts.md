# Illustrative recreation-prompt export and imported-recreation labeling

Implemented slice of [#31](https://github.com/mESmaC/oia/issues/31) and
[#140](https://github.com/mESmaC/oia/issues/140), supporting `MEDIA-04` and the
recreation-prompt subset of `A10` in [the product specification](../spec.md) and
[delivery plan](../delivery.md).
The Rust entry points live in [`crates/oia-media/src/recreation.rs`](../../crates/oia-media/src/recreation.rs):

- `oia_media::recreation::export_recreation_prompt`
- `oia_media::recreation::derive_unknown_negative_keywords`
- `oia_media::recreation::validate_imported_recreation`
- `ValidatedImportedRecreation::portable_metadata_text`

## Contract

`export_recreation_prompt` validates a `RecreationPromptInput` under explicit
`RecreationLimits` (including `max_source_versions_per_item` and
`max_negative_keywords`, both default `64`) and
an `AssetExportPolicy`, returning an encapsulated `ExportedRecreationPrompt`
with read-only getters and a deterministic, human-editable UTF-8 prompt document
(`portable_prompt_text()`):

1. **Evidence separation (`MEDIA-04`, `AGENTS.md` §2):**
   - `ReportedDetail` entries require between `1` and `max_source_versions_per_item` unique pinned source `VersionId`s.
   - `ExplicitAssumption` entries record an illustrative assumption and its rationale.
   - `ExplicitUnknown` entries record aspects that remain undetermined and their impact on the recreation.
   - `AccountVariant` entries preserve competing witness or source accounts separately with validated, symmetric `conflicts_with` links rather than merging or discarding disagreements. A single unconflicted variant is rejected (`SingleVariantRequiresSharedDetails`); each variant must contain at least one `ReportedDetail` (`EmptyVariantDetails`).
   - `NegativeKeyword` entries record explicit prompt exclusions and require a validated `NegativeKeywordOrigin`:
     - `Unknown { unknown_id }` (referencing a declared `ExplicitUnknown`, or a synthetic `context:<field>` / `context:apparent_scale` ID when the corresponding `SceneContext` field or `ApparentScale` leaves that aspect or physical size undetermined),
     - `ContradictedDetail { detail_id, contradicted_statement, contradicted_source_version, contradicting_source_versions }` (carrying the verbatim contradicted statement and its pinned source version, requiring between `1` and `max_source_versions_per_item` unique `contradicting_source_versions` that are pinned in the prompt's reported context/scale or reported details, refusing self-contradiction where `contradicted_source_version` appears in `contradicting_source_versions`, and refusing any `detail_id` present in `shared_reported_details` or any `conflicting_variants[*].reported_details`), or
     - `Variant { for_variant_id, excluded_variant_id }` (scoping a variant exclusion to a specific declared `for_variant_id` and verifying that `excluded_variant_id` is declared and listed in `for_variant_id`'s `conflicts_with`).
     Unattributed negative keywords (`origin: None`) and invalid origin references are refused (`MissingNegativeKeywordOrigin`, `UnknownNegativeOriginReference`, `ContradictedDetailInReportedDetails`, `UnpinnedContradictingSourceVersion`, `SelfContradictingSourceVersion`, `InvalidVariantConflict`).
   - `derive_unknown_negative_keywords(&SceneContext, &[ExplicitUnknown], RecreationLimits)` deterministically derives unknown-origin negative keywords (`"a specific value for <aspect>"`) from both `SceneContext` (`ContextField::Unknown`, `ApparentScale::Unknown`, and `ApparentScale::ReportedAngularOrRelative`'s undetermined physical size/slant range) and `ExplicitUnknown` items while enforcing `limits.max_negative_keywords()` and `limits.max_text_bytes()` before allocation and disambiguating ID collisions.
   - When `negative_keywords` is empty, `portable_prompt_text()` omits both the positive and negative keyword sections and `ValidatedImportedRecreation::portable_metadata_text()` omits `negative_keyword_count` and `negative_keyword=...` lines so `schema_version: 1` outputs without negative keywords remain byte-for-byte identical. When `negative_keywords` is non-empty, `portable_prompt_text()` renders `## Positive Prompt Keywords (Source-Pinned Details and Declared Assumptions)` (preserving `[prompt:title]`, source-pinned `SceneContext` and `ApparentScale` entries with neutral `origin=source_context` / `origin=source_angular_scale` / `origin=calibrated_scale` tags, deduplicated `ExplicitAssumption` entries, and `ReportedDetail` entries with `origin=source_detail` / `origin=variant_source_detail` and preserved `note="..."` qualifiers) and `## Negative Prompt Keywords (Evidence-Derived Exclusions)`. When `conflicting_variants` is non-empty, both sections render per-variant `### Variant` subsections so competing accounts and variant-specific exclusions are never flattened into a single list.
   - All identifier and text bytes are charged against `max_text_bytes` before allocation. Single-line text fields reject ASCII/Unicode control characters, line/paragraph separators (`U+0085`, `U+2028`, `U+2029`), structural bidi controls, and disallowed zero-width characters, while preserving `ZWNJ`/`ZWJ` (`U+200C`/`U+200D`) for multilingual text and emoji. Free-text values in `portable_prompt_text()` are quoted with `\` and `"` escaped so source strings cannot inject headings, forge tags, or alter manifest attributes. `U+200E`/`U+200F`, `U+FFF9..=U+FFFB`, `U+E0000..=U+E007F`, `U+180E`, and `U+3164` are rendered as visible `\u{...}` ASCII escapes rather than exported invisibly. Output length is checked incrementally against `max_output_bytes` on every write, including escape expansion.
2. **Scene, time, viewpoint, lighting, and apparent scale (`SceneContext`):**
   - Each context attribute is tagged as `Reported` (with pinned source versions), `Assumed` (referencing a declared `ExplicitAssumption`), or `Unknown` (preserving the reason without inventing a default time, lens, or altitude).
   - `ApparentScale` distinguishes `ReportedAngularOrRelative` (where physical size and slant range remain unknown), `CalibratedEstimate` (requiring a pinned `calibration_version` and explicit uncertainty bounds), and `Unknown`.
3. **Permitted reference asset manifest (`ReferenceAssetInput`):**
   - Reference assets require a validated 64-character lowercase hexadecimal SHA-256 digest string, pinned `source_version`, `media_type`, `role`, `redistribution_permitted`, and `usage_conditions`.
   - Under `AssetExportPolicy::RefuseUnpermitted`, any asset with `redistribution_permitted == false` fails the export with `RecreationError::UnpermittedAsset`.
   - Under `AssetExportPolicy::OmitWithNotice`, unpermitted assets are excluded from `permitted_assets()` and recorded in `omitted_assets()` as well as the rendered prompt's `Omitted Reference Assets` section so omissions are never hidden.
4. **Imported recreations (`validate_imported_recreation`):**
   - Accepts only a validated `&ExportedRecreationPrompt` (whose fields are private) and requires `AttemptedArtifactClass::IllustrativeRecreation`; attempting to register or promote a generated recreation as `ObservationEvidence`, `SourceAssertion`, or `CalibratedMeasurement` is rejected with `RecreationError::ForbiddenEvidencePromotion`.
   - Verifies that `prompt_id` and `prompt_version` match the exported prompt, requires `variant_id` to identify which declared variant was depicted whenever the prompt has conflicting variants (`MissingVariantSelection`), filters retained `negative_keywords()` to shared (`Unknown`, `ContradictedDetail`) and variant-matching (`Variant { for_variant_id == selected_variant_id, .. }`) exclusions, checks the 64-character lowercase hexadecimal SHA-256 digest format, preserves `generator_model` and `generator_settings`, requires an exact match with `VISIBLE_RECREATION_LABEL`, and returns an encapsulated `ValidatedImportedRecreation` whose `can_serve_as_observation_evidence()` and `can_establish_physical_measurement()` methods always return `false`.
   - Preserves `FIDELITY_AND_EXTERNAL_LABEL_CAVEAT`, stating that prompt wording alone neither proves physical fidelity nor guarantees external-generator metadata labeling.

### Imported recreation metadata export

`ValidatedImportedRecreation::portable_metadata_text(max_output_bytes)` returns a
plain UTF-8 sidecar beginning with `ILLUSTRATIVE RECREATION`. It retains artifact
ID/version, caller-declared digest, media type, prompt ID/version, selected variant,
and generator model/settings (plus `negative_keyword_count` and per-keyword
`negative_keyword=[...]` lines when the imported recreation has non-empty negative
keywords). An explicitly empty `variant_id` means the validated
prompt has no conflicting variants. It includes the fidelity caveat and explicit
statements that the recreation cannot serve as observation evidence or establish
physical measurements. Digest format validation does not verify image bytes.

All values use the prompt exporter's quote and Unicode escaping rules. The byte
budget includes escape expansion and every label; a zero budget is rejected and
overflow returns an error without a partial document. The caller chooses where
to save the returned text. This is a metadata sidecar, not a watermark or an image
exporter, and it cannot guarantee that a label accompanies externally shared
pixels. Distribute the pinned prompt separately to retain its source references,
assumptions, unknowns, and competing accounts. Report/bundle integration remains
open. Recreation sidecars/prompts and Markdown/HTML report prose share
`oia_domain::text::requires_visible_unicode_escape`: ZWNJ/ZWJ remain verbatim
for language and emoji shaping; other classified formatting characters use
visible Unicode escapes. Recreation single-line validation still rejects
structural bidi and line-break controls. Bundle structural validation remains
strict, including rejecting joiners in identifiers and validated metadata.
Original evidence bytes are unchanged.

## Limitations and remaining `#31` scope

- This module is an in-memory validation and rendering kernel; it checks the 64-character lowercase hex SHA-256 format of caller-supplied digest strings and the exact `VISIBLE_RECREATION_LABEL` metadata string, but does not read or hash image files, decode image pixels, verify visual watermarks inside rendered pixel buffers, execute external image-generation models, or persist records to `oia-store`.
- `AttemptedArtifactClass` is a local crate guard to be consolidated with the shared `oia-domain` origin-class vocabulary in `#10`.
- Predictive restoration, frame interpolation, denoising, deblurring, multi-frame reconstruction, and learned super-resolution (`MEDIA-03`) are not implemented in this slice.
- `D09` (restoration and detection model/corpus/threshold selection) remains open.
- The desktop workbench builds and saves a prompt from one session-only simulated account version through this kernel (see [desktop workbench](desktop-workbench.md), workflow 3) with `negative_keywords: Vec::new()`; wiring `derive_unknown_negative_keywords` into the desktop simulated-account export and statement-comparison/composer workflows (`#138`/`#139`) remains open on `#140`. Integration with the forensic media workbench (`#30`), portable bundle/report packaging (`#22`), and full end-to-end `A10` acceptance across decoded imagery remain open on `#31`.
- No Lean theorem or formal Rust-to-Lean correspondence claim is made for this slice.

## Local verification

Executed on Windows x86_64 MSVC with pinned `rustc 1.99.0` on 2026-11-02:

- `cargo +1.99.0 fmt --all -- --check`
- `cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings`
- `cargo +1.99.0 test --workspace --locked --offline`
- `cargo +1.99.0 run -p oia-media --example export-recreation-prompt --locked --offline`

Three targeted contract tests in [`crates/oia-media/tests/recreation_contract.rs`](../../crates/oia-media/tests/recreation_contract.rs) verify:
1. Persian, Devanagari, and emoji text preserve `ZWNJ`/`ZWJ`, while invisible-only fields containing each supported formatting/tag character are exported with visible `\u{...}` escapes rather than raw invisible characters.
2. Conflicting variants, assumptions, unknowns, angular scale, omitted unpermitted assets, mandatory variant selection on import, and exact `VISIBLE_RECREATION_LABEL` metadata remain separated and refuse promotion to observation or measurement evidence.
3. Unattributed reported details, per-item `source_versions` overflows, Unicode line-break/bidi/zero-width injection, quote-escape forging in manifest fields, asymmetric/empty variants, and budget-exceeding outputs are rejected without silent truncation or invented defaults.

### Metadata sidecar verification (2026-11-03)

Executed on Windows x86_64 MSVC with pinned `rustc 1.99.0`:

- `cargo test -p oia-media --locked --offline --test recreation_contract`: all three existing contract tests passed, with focused assertions added for sidecar label/lineage preservation, quoted settings, visible Unicode escapes, absent variant semantics, and exact/insufficient/zero output budgets.
- `cargo run -p oia-media --example export-recreation-prompt --locked --offline`: the synthetic example emitted the prompt followed by the labeled imported-recreation metadata sidecar.

These checks cover metadata export only; they do not establish image-byte integrity,
pixel watermarking, full A10 integration, or a formal proof claim.

The workspace baseline also passed for this change: `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets --locked --offline -- -D warnings`, and
`cargo test --workspace --locked --offline` (including the existing Rust cutoff
correspondence test). Documentation relative links/fences and `git diff --check`
passed. The Lean proof gate was not rerun; no covered algorithm changed.

### Shared Unicode display policy verification (2026-11-03)

On Windows x86_64 MSVC with pinned Rust 1.99.0, after rebasing onto `b13946b`,
workspace formatting, clippy (`--workspace --all-targets --locked --offline --
-D warnings`), and tests (`--workspace --locked --offline`) passed. The synthetic
`export-recreation-prompt` example also ran successfully. Existing report,
recreation, and bundle contract tests now cover multilingual joiner preservation,
visible soft-hyphen/formatting escapes, and continued strict bundle joiner refusal.
No new test cases were added. Relative documentation links/fences and diff checks
passed. An initial shared-cache clippy run reported a missing existing citation
module; the successful baseline used the task's isolated build cache. The optional
Cesium payload was absent, so Globe rendering was not exercised. No Lean proof
check was rerun; no covered algorithm or proof claim changed. These checks cover
the Unicode export slice, not full A10/A16 integration.

### Evidence-derived negative keywords verification (2026-11-03)

Executed on Windows x86_64 MSVC with pinned `rustc 1.99.0` for the `#140`
`oia-media::recreation` kernel slice after merging `main` (`#136` / `#143`):

- `cargo +1.99.0 fmt --all -- --check`
- `cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings`
- `cargo +1.99.0 test --workspace --locked --offline`
- `cargo +1.99.0 run -p oia-media --example export-recreation-prompt --locked --offline`

The three contract tests in [`crates/oia-media/tests/recreation_contract.rs`](../../crates/oia-media/tests/recreation_contract.rs)
verify byte-identical empty-negative output under `schema_version: 1`, per-variant
positive and negative prompt subsections, `Unknown`/`ContradictedDetail`/`Variant`
negative-keyword origins (including verbatim `contradicted_statement` and
`contradicted_source_version`), `derive_unknown_negative_keywords` across both
`SceneContext` and `ExplicitUnknown` items under `RecreationLimits`, variant-filtered
sidecar `negative_keyword_count` and `negative_keyword=[...]` metadata preservation,
and refusals for missing origins, undeclared unknown/variant references,
empty/unpinned/self-contradicting source lists, contradicted details present in
shared or variant `reported_details`, and `max_negative_keywords` overflows.
Desktop workbench `derive_unknown_negative_keywords` export wiring and
statement-comparison/composer wiring (`#138`/`#139`) remain open on `#140`. No
Lean proof check was rerun; no covered algorithm or formal proof claim changed.
