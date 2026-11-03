# Reproducible investigation bundle validation, staged SHA-256 verification, and analysis replay kernel

Status: implemented as the in-memory bundle validation, staged integrity verification, local namespace mapping, and analysis replay kernel in [`crates/oia-export/src/bundle.rs`](../../crates/oia-export/src/bundle.rs); not yet wired into filesystem handle-relative directory staging, `oia-store` transactional commit/recovery, or workspace application services. Work package: [#39](https://github.com/mESmaC/oia/issues/39) (also covering the in-memory bundle manifest, path, container, integrity, and namespace-mapping rules of [#22](https://github.com/mESmaC/oia/issues/22)). Requirements: [REPRO-01, REPRO-02, REPRO-03](../spec.md#72-reproducible-packages). Design and contract references: [ADR 0006 (D05)](../adr/0006-d05-report-and-bundle-formats.md) and [report export contract](../contracts/report-export.md) sections 5–6. Acceptance reference: [A17](../delivery.md#a17-portable-evidence-and-replay), covering the in-memory bundle verification, namespace mapping, inert handling, and deterministic/tolerance-based analysis replay subset.

## Scope and boundaries

`crates/oia-export/src/bundle.rs` is a standard-library-only Rust 2024 module (`unsafe_code = "forbid"`, `unwrap_used = "deny"`, `expect_used = "deny"`, `indexing_slicing = "deny"`). It validates caller-staged portable directory bundle manifests and payload bytes (`validate_and_stage_bundle`) and evaluates explicit analysis replay requests against a validated bundle (`replay_analysis`). No unmerged prerequisite code is copied.

What this kernel **does not** do:

- It does not perform OS-level handle-relative/no-follow directory traversal, write staging directories to disk, or execute atomic same-filesystem directory renames (`docs/contracts/report-export.md` §6 filesystem I/O and `oia-store` transactional publication remain open on `#22` and `#39`). `StagedPayloadFile::entry_kind` and `rechecked_sha256_hex` are caller-asserted metadata from the caller's staging layer (`#22`).
- It does not parse wire JSON directly; callers decode the bounded manifest into `BundleImportInput` and supply staged byte slices (`StagedPayloadFile`) after reading regular files into a private staging area.
- It does not execute analysis engines, load AI model weights, or invoke solver crates directly; `replay_analysis` checks capability/source/asset/calibration/model/license prerequisites on the pinned bundle and compares caller-supplied candidate outputs (`CandidateReplayOutput::Bytes` or `CandidateReplayOutput::NumericalSeries`) against the pinned `ReplayContract`.
- SHA-256 equality (`pub(crate)` `sha256_bytes`, `sha256_hex`) establishes byte integrity of staged payloads and bitwise replay outputs only; it does not establish source truth, authenticity, or cryptographic authorship (`ADR 0006`, `AGENTS.md` §2). Once workspace `sha2` lands on `main`, `bundle.rs` can delegate to `sha2::Sha256` directly.
- No performance benchmark, live hardware/provider integration, or Lean theorem / Rust-to-Lean correspondence is claimed.

## Run locally

From the repository root using Rust 1.99.0:

```text
cargo +1.99.0 run -p oia-export --example inspect-a17-bundle --locked --offline
cargo +1.99.0 run -p oia-export --example inspect-a17-bundle --locked --offline -- --summary 64
```

The read-only `inspect-a17-bundle` example accepts `[--summary] [MAX_TOTAL_BYTES]`. It borrows the canonical synthetic [`fixtures/documents/a03/originals/`](../../fixtures/documents/a03/README.md) UTF-8 artifacts (`report-v1.txt`, `report-v2.txt`, `redacted-a.txt`) in place, constructs a synthetic `A17` directory bundle manifest (labeled `[SYNTHETIC FIXTURE]` in output) with original and verbatim mirror records sharing asset bytes, corrected versions, restricted external references, omissions, calibration records, forecast vintages, and three analysis runs (deterministic bitwise, numerical tolerance, and stochastic model-backed), runs `validate_and_stage_bundle` and `replay_analysis`, and prints the resulting namespace mappings and replay outcomes. Passing a tight byte ceiling (for example `--summary 64`) demonstrates deterministic `LimitExceeded` rejection before full staging.

## Kernel contract

### 1. Explicit budgets and cooperative stop control (`REPRO-02`)

- `BundleLimits::new(...)` enforces non-zero finite ceilings across all ten dimensions (`Capabilities`, `Entries`, `Records`, `References`, `Omissions`, `ReproductionItems`, `FileBytes`, `TotalBytes`, `TextBytes`, `ReplaySamples`) and rejects zero in any dimension (`BundleError::ZeroLimit`).
- `validate_and_stage_bundle` and `replay_analysis` enforce counts, per-file byte limits, cumulative staged byte limits, and metadata/identifier UTF-8 byte limits before allocating (`Vec::try_reserve`) or hashing.
- Both entry points accept a synchronous callback `FnMut() -> Result<(), StopReason>` (`Cancelled`, `DeadlineExceeded`, `Superseded`) checked at entry, across every inventory loop, per replay sample, and before returning.

### 2. Portable bundle layout, path safety, and staged SHA-256 integrity (`REPRO-01`, `REPRO-02`, `A17`)

- Validates format identity (`oia-investigation-bundle` v1) and checks `CapabilityDeclaration` entries against `SupportedCapabilities`. Unsupported `Required` capabilities fail import (`BundleError::UnsupportedRequiredCapability`). Unsupported `Optional` `(capability_id, version)` pairs are recorded in `ValidatedBundle::omitted_optional_capabilities`; records depending on them are either retained as `OptionalRecordHandling::RetainedInertOpaqueBytes` (when `allow_inert_optional_records` is `true`) or omitted cleanly (with references targeting them rewritten to `ValidatedReferenceTarget::PermittedUnavailable`), never marked executed or replayed.
- Enforces role-specific relative `/`-separated paths (`records/`, `assets/`, `reports/`, `reproduction/`). Path segments are restricted to safe ASCII `[A-Za-z0-9._-]` (1..=255 bytes per segment), rejecting empty, `.` or `..` traversal components, leading/trailing slashes, backslashes, drive letters/colons, non-ASCII characters (preventing Unicode case-folding collisions), Windows-invalid characters (`< > : " | ? *`), trailing dots, Windows reserved device names (`CON`, `PRN`, `AUX`, `NUL`, `CONIN$`, `CONOUT$`, `COM1`–`COM9`, `LPT1`–`LPT9`, and superscript variants), and case-insensitive colliding paths (`BundleError::InvalidRelativePath`, `BundleError::CaseInsensitivePathCollision`).
- Rejects non-regular staged filesystem entries (`Symlink`, `Junction`, `ReparsePoint`, `DeviceNode`, `FifoOrSocket`) (`BundleError::UnsafeFilesystemEntry`) and unlisted staged payloads (`BundleError::UnlistedStagedPayload`).
- Rejects archive and compressed containers by extension (`.zip`, `.gz`, `.tgz`, `.tar`, `.7z`, `.xz`, `.bz2`, `.zst`) or magic header (`PK\x03\x04`, `\x1F\x8B`, Zstandard, XZ, 7z, BZIP2, POSIX `ustar`) on `records/`, `reports/`, and `reproduction/` entries without unpacking (`BundleError::ArchiveContainerRejected`), while permitting opaque hashed evidence artifacts (such as `.docx` or `.zip` evidence files) under `assets/`.
- Computes FIPS 180-4 SHA-256 over staged payload bytes (`sha256_bytes`, `sha256_hex`), verifies `assets/<sha256>` filename-to-digest correspondence (`BundleError::AssetFilenameHashMismatch`), verifies exact byte lengths (`BundleError::ByteLengthMismatch`) and digests (`BundleError::IntegrityHashMismatch`—never downgrading corrupt included bytes to missing), and checks optional second-pass digests to detect mid-staging input mutation (`BundleError::InputChangedDuringStaging`).

### 3. Local workspace namespace mapping, duplicate-byte separation, and inert handling (`REPRO-01`, `REPRO-02`, `A17`)

- Refuses to overwrite an existing case when `destination_case_id` is present in `existing_case_ids` (`BundleError::ExistingCaseOverwriteRefused`) and rejects any `(record_id, version_id)` that appears in both `records` and `omissions` (`BundleError::DuplicateIdentifier { kind: "record_and_omission", .. }`).
- Maps every `(origin_namespace, record_id, version_id)` deterministically and injectively into `local_namespace_prefix` using length-prefixed encoding (`format_local_record_id` -> `"{prefix}/rec/{len}:{origin_namespace}/{len}:{record_id}"`, `format_local_version_id` -> `"{prefix}/ver/{len}:{origin_namespace}/{len}:{record_id}/{len}:{version_id}"`), preventing delimiter or multi-namespace collisions while preserving `origin_namespace`, `origin_record_id`, `origin_version_id`, `origin_class`, `review_status`, `recorded_time`, and `uncertainty_note` on `MappedBundleRecord`.
- Retains validated package `identity`, `use_conditions`, `references`, `omissions`, `queries`, `groupings`, `calibrations`, `forecasts`, and `analyses` on `ValidatedBundle` so downstream `oia-store` persistence has all validated domain artifacts.
- Distinct records that reference the same content-addressed asset under `assets/` (such as an original report and a verbatim mirror republication) retain separate origin and local record/version identities while sharing the verified asset SHA-256 digest.
- Source actors retain origin attribution with `LocalAuthorityGrant::None` so imported packages never grant permissions in the destination workspace.
- Bundled readable reports under `reports/` are tagged `ImportedReportHandling::UntrustedRequiresRegenerationOrInertViewer`. Reproduction specifications and external reference URLs are checked against local filesystem paths, loopback/private-network targets (`127.0.0.1`, `0.0.0.0`, `::1`, `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `169.254.0.0/16`), credential parameters, and executable/device/network hooks (`BundleError::ForbiddenBundleSideEffect`).

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
  - `ReplayContract::NumericalTolerance` requires `0.0 <= max_abs_diff` and `0.0 <= max_rel_diff < 1.0`, verifies that candidate and expected `f64` series have matching sample counts (reporting `DivergedBeyondTolerance` with `expected_sample_count` and `actual_sample_count` on length mismatch), and verifies that every sample satisfies **both** `abs_diff <= max_abs_diff` and `rel_diff <= max_rel_diff` (`ReproducedWithinTolerance` vs. `DivergedBeyondTolerance`).

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

Executed locally on 2026-11-02 (America/New_York), Windows x86_64/MSVC, Rust/Cargo 1.99.0, Python 3.12, at repository base `5d2fb26`:

- `cargo +1.99.0 fmt --all -- --check`: passed.
- `cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings`: passed with zero warnings across workspace crates, examples, and integration tests.
- `cargo +1.99.0 test --workspace --locked --offline`: passed across the workspace, including the NIST FIPS 180-4 SHA-256 unit test (`sha256_matches_nist_fips_180_4_vectors`) and 4 contract tests in `crates/oia-export/tests/bundle_contract.rs`.
- `cargo +1.99.0 run -p oia-export --example inspect-a17-bundle --locked --offline`: verified bundle staging, duplicate-byte record separation, injective local namespace mapping, inert report handling, bitwise/tolerance/non-replayable replay checks, and deterministic `TotalBytes` budget rejection (`--summary 64`); confirmed original `fixtures/documents/a03/` SHA-256 digests remain unchanged via `python -I scripts/fixtures/verify_a03.py`.
