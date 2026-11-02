# ADR 0003: Project Licensing, Third-Party Notices, and Local Verification Policy (D14)

- **Date:** 2026-11-01
- **Status:** Accepted for P0; release installer/signing packaging deferred to P6 (#49)
- **Decision ID:** D14
- **Issues:** #9, #49
- **Requirements:** Shared Research Rule 9 (Reuse conditions), `REPRO-01`

## Context

`D14` requires choosing OIA's source-code license, establishing the third-party notice and attribution process (especially for selective ports from God's Eye View and upstream data sources), and defining the build/verification workflow.

## Decision

1. **Accepted for P0:**
   - **Repository Code License:** OIA original source code is licensed under the MIT License (`LICENSE`).
   - **Third-Party Code Attribution:** Selective ports from God's Eye View (`https://github.com/bilawalsidhu/gods-eye-view`, inspected at commit `0d1a68160e532ba65dc34b48ea205de6de4028fc`) retain Bilawal Sidhu's MIT copyright notice in `THIRD_PARTY_NOTICES.md` and in the doc headers of porting modules.
   - **Data and Asset Separation:** Neither OIA's MIT license nor God's Eye View's MIT license covers external datasets, feeds, documents, models, or 3D/media assets. Every `Source` registration in `oia-domain`/`oia-ingest` and every `BundleManifest` in `oia-export` carries explicit `ReuseConditions` (`ordinary_access_method`, `automated_processing`, `retention_policy`, `redistribution_policy`, and `attribution`).
   - **Local-First Verification:** Per `AGENTS.md` Section 8, baseline verification (`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked`, and `lake build` in `proofs/`) runs locally against pinned toolchains (`rust-toolchain.toml` at `1.99.0` and `proofs/lean-toolchain` at `leanprover/lean4:v4.35.0`). Hosted CI/CD workflows are intentionally not enabled by default to avoid hosted runner costs unless explicitly requested.

2. **Open Proposal for P6 (#49):**
   - Cross-platform binary installer formats, code-signing certificates, and update manifest distribution channels remain deferred to #49.

## Validation

- `LICENSE` and `THIRD_PARTY_NOTICES.md` are present at the repository root, and `oia-export` bundle manifests enforce per-asset reuse and omission reporting.
