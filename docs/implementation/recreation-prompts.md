# Illustrative recreation-prompt export and imported-recreation labeling

Implemented slice of [#31](https://github.com/mESmaC/oia/issues/31), supporting
`MEDIA-04` and the recreation-prompt subset of `A10` in
[the product specification](../spec.md) and [delivery plan](../delivery.md).
The Rust entry points live in [`crates/oia-media/src/recreation.rs`](../../crates/oia-media/src/recreation.rs):

- `oia_media::recreation::export_recreation_prompt`
- `oia_media::recreation::validate_imported_recreation`

## Contract

`export_recreation_prompt` validates a `RecreationPromptInput` under explicit
`RecreationLimits` and an `AssetExportPolicy`, returning an
`ExportedRecreationPrompt` with a deterministic, human-editable UTF-8 prompt
document (`portable_prompt_text`):

1. **Evidence separation (`MEDIA-04`, `AGENTS.md` §2):**
   - `ReportedDetail` entries require at least one pinned source `VersionId`.
   - `ExplicitAssumption` entries record an illustrative assumption and its rationale.
   - `ExplicitUnknown` entries record aspects that remain undetermined and their impact on the recreation.
   - `AccountVariant` entries preserve competing witness or source accounts separately with explicit `conflicts_with` links rather than merging or discarding disagreements.
2. **Scene, time, viewpoint, lighting, and apparent scale (`SceneContext`):**
   - Each context attribute is tagged as `Reported` (with pinned source versions), `Assumed` (referencing a declared `ExplicitAssumption`), or `Unknown` (preserving the reason without inventing a default time, lens, or altitude).
   - `ApparentScale` distinguishes `ReportedAngularOrRelative` (where physical size and slant range remain unknown), `CalibratedEstimate` (requiring a pinned `calibration_version` and explicit uncertainty bounds), and `Unknown`.
3. **Permitted reference asset manifest (`ReferenceAssetInput`):**
   - Reference assets require a validated 64-character lowercase hexadecimal SHA-256 digest, pinned `source_version`, `media_type`, `role`, `redistribution_permitted`, and `usage_conditions`.
   - Under `AssetExportPolicy::RefuseUnpermitted`, any asset with `redistribution_permitted == false` fails the export with `RecreationError::UnpermittedAsset`.
   - Under `AssetExportPolicy::OmitWithNotice`, unpermitted assets are excluded from `permitted_assets` and recorded in `omitted_assets` as well as the rendered prompt's `Omitted Reference Assets` section so omissions are never hidden.
4. **Imported recreations (`validate_imported_recreation`):**
   - Requires `AttemptedArtifactClass::IllustrativeRecreation`; attempting to register or promote a generated recreation as `ObservationEvidence`, `SourceAssertion`, or `CalibratedMeasurement` is rejected with `RecreationError::ForbiddenEvidencePromotion`.
   - Verifies that `prompt_id`, `prompt_version`, and optional `variant_id` match the exported prompt, validates the artifact's SHA-256 digest, preserves `generator_model` and `generator_settings`, requires `VISIBLE_RECREATION_LABEL`, and sets `can_serve_as_observation_evidence = false` and `can_establish_physical_measurement = false`.
   - Preserves `FIDELITY_AND_EXTERNAL_LABEL_CAVEAT`, stating that prompt wording alone neither proves physical fidelity nor guarantees external-generator metadata labeling.

## Limitations and remaining `#31` scope

- This module is an in-memory validation and rendering kernel; it does not read or write image files, execute external image-generation models, or persist records to `oia-store`.
- Predictive restoration, frame interpolation, denoising, deblurring, multi-frame reconstruction, and learned super-resolution (`MEDIA-03`) are not implemented in this slice.
- `D09` (restoration and detection model/corpus/threshold selection) remains open.
- Integration with the forensic media workbench (`#30`), portable bundle/report packaging (`#22`), and full end-to-end `A10` acceptance across decoded imagery remain open on `#31`.
- No Lean theorem or formal Rust-to-Lean correspondence claim is made for this slice.

## Local verification

Executed on Windows x86_64 MSVC with pinned `rustc 1.99.0` on 2026-11-02:

- `cargo +1.99.0 fmt --all -- --check`
- `cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings`
- `cargo +1.99.0 test --workspace --locked --offline`
- `cargo +1.99.0 run -p oia-media --example export-recreation-prompt --locked --offline`

Two targeted contract tests in [`crates/oia-media/tests/recreation_contract.rs`](../../crates/oia-media/tests/recreation_contract.rs) verify:
1. Conflicting variants, assumptions, unknowns, angular scale, omitted unpermitted assets, and imported-recreation labeling remain separated and refuse promotion to observation or measurement evidence.
2. Unattributed reported details, undeclared assumption references, and budget-exceeding outputs are rejected without silent truncation or invented defaults.
