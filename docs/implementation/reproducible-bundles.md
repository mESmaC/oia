# Reproducible investigation bundle validation, staged SHA-256 verification, and analysis replay kernel

Status: implemented as the in-memory bundle validation, staged integrity verification, local namespace mapping, and analysis replay kernel in [`crates/oia-export/src/bundle.rs`](../../crates/oia-export/src/bundle.rs); not yet wired into filesystem handle-relative directory staging, `oia-store` transactional commit/recovery, or workspace application services. Work package: [#39](https://github.com/mESmaC/oia/issues/39) (also covering the in-memory bundle manifest, path, container, integrity, and namespace-mapping rules of [#22](https://github.com/mESmaC/oia/issues/22)). Requirements: [REPRO-01, REPRO-02, REPRO-03](../spec.md#72-reproducible-packages). Design and contract references: [ADR 0006 (D05)](../adr/0006-d05-report-and-bundle-formats.md) and [report export contract](../contracts/report-export.md) sections 5–6. Acceptance reference: [A17](../delivery.md#a17-portable-evidence-and-replay), covering the in-memory bundle verification, namespace mapping, inert handling, and deterministic/tolerance-based analysis replay subset.

## Scope and boundaries

`crates/oia-export/src/bundle.rs` is a standard-library-only Rust 2024 module (`unsafe_code = "forbid"`, `unwrap_used = "deny"`, `expect_used = "deny"`, `indexing_slicing = "deny"`). It validates caller-staged portable directory bundle manifests and payload bytes (`validate_and_stage_bundle`) and evaluates explicit analysis replay requests against a validated bundle (`replay_analysis`). No unmerged prerequisite code is copied.

What this kernel **does not** do:

- It does not perform OS-level handle-relative/no-follow directory traversal, write staging directories to disk, or execute atomic same-filesystem directory renames (`docs/contracts/report-export.md` §6 filesystem I/O and `oia-store` transactional publication remain open on `#22` and `#39`).
- It does not parse wire JSON directly; callers decode the bounded manifest into `BundleImportInput` and supply staged byte slices (`StagedPayloadFile`) after reading regular files into a private staging area.
- It does not execute analysis engines, load AI model weights, or invoke solver crates directly; `replay_analysis` checks capability/source/asset/calibration/model/license prerequisites on the pinned bundle and compares caller-supplied candidate outputs (`CandidateReplayOutput::Bytes` or `CandidateReplayOutput::NumericalSeries`) against the pinned `ReplayContract`.
- SHA-256 equality (`sha256_bytes`, `sha256_hex`) establishes byte integrity of staged payloads and bitwise replay outputs only; it does not establish source truth, authenticity, or cryptographic authorship (`ADR 0006`, `AGENTS.md` §2).
- No performance benchmark, live hardware/provider integration, or Lean theorem / Rust-to-Lean correspondence is claimed.

## Run locally

From the repository root using Rust 1.99.0:

```text
cargo +1.99.0 run -p oia-export --example inspect-a17-bundle --locked --offline
cargo +1.99.0 run -p oia-export --example inspect-a17-bundle --locked --offline -- --summary 64
```

The read-only `inspect-a17-bundle` example accepts `[--summary] [MAX_TOTAL_BYTES]`. It borrows the canonical synthetic [`fixtures/documents/a03/originals/`](../../fixtures/documents/a03/README.md) UTF-8 artifacts (`report-v1.txt`, `report-v2.txt`, `redacted-a.txt`) in place, constructs a synthetic `A17` directory bundle manifest with original and verbatim mirror records sharing asset bytes, corrected versions, restricted external references, omissions, calibration records, forecast vintages, and three analysis runs (deterministic bitwise, numerical tolerance, and stochastic model-backed), runs `validate_and_stage_bundle` and `replay_analysis`, and prints the resulting namespace mappings and replay outcomes. Passing a tight byte ceiling (for example `--summary 64`) demonstrates deterministic `LimitExceeded` rejection before full staging.

## Kernel contract

### 1. Explicit budgets and cooperative stop control (`REPRO-02`)

- `BundleLimits::new(...)` enforces non-zero finite ceilings across all ten dimensions (`Capabilities`, `Entries`, `Records`, `References`, `Omissions`, `ReproductionItems`, `FileBytes`, `TotalBytes`, `TextBytes`, `ReplaySamples`) and rejects zero in any dimension (`BundleError::ZeroLimit`).
- `validate_and_stage_bundle` and `replay_analysis` enforce counts, per-file byte limits, cumulative staged byte limits, and metadata/identifier UTF-8 byte limits before allocating (`Vec::try_reserve`) or hashing.
- Both entry points accept a synchronous callback `FnMut() -> Result<(), StopReason>` (`Cancelled`, `DeadlineExceeded`, `Superseded`) checked at entry, across every inventory loop, per replay sample, and before returning.

### 2. Portable bundle layout, path safety, and staged SHA-256 integrity (`REPRO-01`, `REPRO-02`, `A17`)

- Validates format identity (`oia-investigation-bundle` v1) and checks `CapabilityDeclaration` entries against `SupportedCapabilities`. Unsupported `Required` capabilities fail import (`BundleError::UnsupportedRequiredCapability`). Unsupported `Optional` capabilities are recorded in `ValidatedBundle::omitted_optional_capabilities`, and records depending on them are marked `OptionalRecordHandling::RetainedInertOpaqueBytes` (when permitted) or `OptionalRecordHandling::OmittedWithNotice`, never marked executed or replayed.
- Enforces role-specific relative `/`-separated paths (`records/`, `assets/`, `reports/`, `reproduction/`). Rejects empty, `.` or `..` traversal components, leading/trailing slashes, backslashes, drive letters/colons, control/bidi/invisible characters, leading/trailing spaces or trailing dots on components, Windows reserved device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9` with or without extensions), and case-insensitive colliding paths (`BundleError::InvalidRelativePath`, `BundleError::CaseInsensitivePathCollision`).
- Rejects non-regular staged filesystem entries (`Symlink`, `Junction`, `ReparsePoint`, `DeviceNode`, `FifoOrSocket`) (`BundleError::UnsafeFilesystemEntry`) and unlisted staged payloads (`BundleError::UnlistedStagedPayload`).
- Rejects archive and compressed containers by extension (`.zip`, `.gz`, `.tgz`, `.tar`, `.7z`, `.xz`, `.bz2`, `.zst`) or magic header (`PK\x03\x04`, `\x1F\x8B`, Zstandard, XZ, 7z, BZIP2, POSIX `ustar`) without unpacking (`BundleError::ArchiveContainerRejected`).
- Computes FIPS 180-4 SHA-256 over staged payload bytes (`sha256_bytes`, `sha256_hex`), verifies `assets/<sha256>` filename-to-digest correspondence (`BundleError::AssetFilenameHashMismatch`), verifies exact byte lengths (`BundleError::ByteLengthMismatch`) and digests (`BundleError::IntegrityHashMismatch`—never downgrading corrupt included bytes to missing), and checks optional second-pass digests to detect mid-staging input mutation (`BundleError::InputChangedDuringStaging`).

### 3. Local workspace namespace mapping, duplicate-byte separation, and inert handling (`REPRO-01`, `REPRO-02`, `A17`)

- Refuses to overwrite an existing case when `destination_case_id` is present in `existing_case_ids` (`BundleError::ExistingCaseOverwriteRefused`).
- Maps every `(record_id, version_id)` deterministically into `local_namespace_prefix` (`<prefix>/rec/<record_id>`, `<prefix>/ver/<version_id>`) while preserving `origin_namespace`, `origin_record_id`, `origin_version_id`, `origin_class`, `review_status`, `recorded_time` precision, and `uncertainty_note`.
- Distinct records that reference the same content-addressed asset under `assets/` (such as an original report and a verbatim mirror republication) retain separate origin and local record/version identities while sharing the verified asset SHA-256 digest.
- Source actors retain origin attribution with `LocalAuthorityGrant::None` so imported packages never grant permissions in the destination workspace.
- Bundled readable reports under `reports/` are tagged `ImportedReportHandling::UntrustedRequiresRegenerationOrInertViewer`. Reproduction specifications and external reference URLs are checked against local filesystem paths, loopback/private-network targets, credential parameters, and executable/device/network hooks (`BundleError::ForbiddenBundleSideEffect`).

### 4. Deterministic and tolerance-based analysis replay (`REPRO-03`, `A17`)

- `replay_analysis` is an explicit, separate verification step that never runs during `validate_and_stage_bundle`.
- Before comparing outputs, `replay_analysis` checks and reports explicit `AnalysisReplayOutcome::NonReplayable` variants (never silently substituting inputs, models, or tolerances) for:
  - `UnsupportedCapability { capability_id, version }`
  - `UnavailableSource { record_id, version_id, reason }` (including omitted or restricted input records)
  - `UnavailableAsset { asset_path }`
  - `InvalidatedCalibration { calibration_id, version_id, reason }` (such as a displaced camera)
  - `MissingOrMismatchedModel { model_id, required_version, available_version }`
  - `ChangedOrIncompatibleLicense { method_id, detail }`
  - `StochasticOutput { method_id, reason }`
- When replayable:
  - `ReplayContract::DeterministicBitwise` verifies candidate output byte length and SHA-256 digest against the pinned expected digest (`ReproducedBitwise` vs. `DivergedBitwise`).
  - `ReplayContract::NumericalTolerance` verifies finite candidate and expected `f64` series of matching length against justified `max_abs_diff` and `max_rel_diff` bounds (`ReproducedWithinTolerance` vs. `DivergedBeyondTolerance` with the first failing index and observed residuals).

## Integration still required on #39 and #22

Keep `#39` and `#22` open. Completing them requires merging `#11`, `#16`, `#35`, `#37`, and `#38` and implementing:

- Handle-relative/no-follow filesystem staging, atomic same-filesystem directory publication, and `oia-store` transactional case commit/recovery;
- Wire JSON serialization/deserialization bound to the finalized shared domain schemas;
- Live replay dispatch into composed reasoning (`#35`) and multi-view calibration/reconstruction solvers (`#37`);
- Full end-to-end `A17` acceptance verification across persisted workspace storage and UI workflows.

## Local verification

```text
cargo +1.99.0 fmt --all -- --check
cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings
cargo +1.99.0 test --workspace --locked --offline
cargo +1.99.0 run -p oia-export --example inspect-a17-bundle --locked --offline
python -I scripts/fixtures/verify_a03.py
```

### Executed evidence

Executed locally on 2026-11-02 (America/New_York), Windows x86_64/MSVC, Rust/Cargo 1.99.0, Python 3.12, at repository base `527416f`:

- `cargo +1.99.0 fmt --all -- --check`: passed.
- `cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings`: passed with zero warnings across workspace crates, examples, and integration tests.
- `cargo +1.99.0 test --workspace --locked --offline`: passed across the workspace, including 4 targeted contract tests in `crates/oia-export/tests/bundle_contract.rs` (`validates_bundle_hashes_maps_local_namespace_and_keeps_duplicate_byte_records_distinct`, `rejects_traversal_windows_devices_symlinks_archives_corrupt_hashes_and_side_effects`, `verifies_bitwise_and_tolerance_replay_and_reports_non_replayable_reasons_honestly`, `enforces_bundle_resource_budgets_case_overwrite_guard_and_cooperative_stops`).
- `cargo +1.99.0 run -p oia-export --example inspect-a17-bundle --locked --offline`: verified bundle staging, duplicate-byte record separation, local namespace mapping, inert report handling, bitwise/tolerance/non-replayable replay checks, and deterministic `TotalBytes` budget rejection (`--summary 64`); confirmed original `fixtures/documents/a03/` SHA-256 digests remain unchanged via `python -I scripts/fixtures/verify_a03.py`.
