# ADR 0003: Project Licensing, Third-Party Notices, and Local Verification Policy (D14)

- **Date:** 2026-11-01
- **Status:** Proposed (awaiting project-owner confirmation of repository license; release installer/signing packaging deferred to P6 #49)
- **Decision ID:** D14
- **Issues:** #9, #49
- **Requirements:** Shared Research Rule 9 (Reuse conditions), `REPRO-01`

## Context

`D14` requires choosing OIA's source-code license, establishing the third-party notice and attribution process (especially for selective ports from God's Eye View and upstream data sources), and defining the build/verification workflow.

## Decision

1. **Proposed Repository License (Awaiting Owner Confirmation) & Accepted Attribution/Verification Rules:**
   - **Repository Code License (Pending Owner Confirmation):** MIT License (or dual `MIT OR Apache-2.0`) is proposed for OIA original source code. Per `docs/delivery.md` (`D14`), no root `LICENSE` file is committed to `main` until the project owner explicitly confirms the license choice.
   - **Third-Party Code Attribution:** Selective ports from God's Eye View (`https://github.com/bilawalsidhu/gods-eye-view`, inspected at commit `0d1a68160e532ba65dc34b48ea205de6de4028fc`) retain Bilawal Sidhu's MIT copyright notice in `THIRD_PARTY_NOTICES.md` and in the doc headers of porting modules.
   - **Data and Asset Separation:** Neither OIA's repository license nor God's Eye View's MIT license covers external datasets, feeds, documents, models, or 3D/media assets. Every `Source` registration in `oia-domain`/`oia-ingest` and every `BundleManifest` in `oia-export` carries explicit `ReuseConditions` (`ordinary_access_method`, `automated_processing`, `retention_policy`, `redistribution_allowed`, and `required_attribution`).
   - **Local-First Verification:** Per `AGENTS.md` Section 8, baseline verification (`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked`, and `lake build` in `proofs/`) runs locally against pinned toolchains (`rust-toolchain.toml` at `1.99.0` and `proofs/lean-toolchain` at `leanprover/lean4:v4.35.0`). Hosted CI/CD workflows are intentionally not enabled by default to avoid hosted runner costs unless explicitly requested.

2. **Open Proposal for P6 (#49):**
   - Cross-platform binary installer formats, code-signing certificates, and update manifest distribution channels remain deferred to #49.

## Validation

- `THIRD_PARTY_NOTICES.md` is present at the repository root, and `oia-export` bundle manifests enforce per-asset reuse and omission reporting (`OmissionReason::RedistributionProhibitedByPolicy`). Once the project owner confirms the repository license under `D14`, the root `LICENSE` file and `Cargo.toml` `license` field will be added.
