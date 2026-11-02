# ADR 0003: Project Licensing, Third-Party Notices, and Local Verification Policy (D14)

- **Date:** 2026-11-02
- **Status:** Accepted for repository code license and third-party notices ([project-owner decision on 2026-11-02](https://github.com/mESmaC/oia/pull/56#issuecomment-6568675966)); release installer/signing packaging deferred to P6 `#49`
- **Decision ID:** D14
- **Issues:** #9, #49
- **Requirements:** Shared Research Rule 9 (Reuse conditions), `REPRO-01`

## Context

`D14` requires choosing OIA's source-code license, establishing the third-party notice and attribution process (for selective ports from God's Eye View and third-party reference fixtures/datasets), and defining the local build and verification workflow.

## Decision

Per the project owner's decision on 2026-11-02 ([#56 comment 6568675966](https://github.com/mESmaC/oia/pull/56#issuecomment-6568675966) and [#9 comment 6568676127](https://github.com/mESmaC/oia/issues/9#issuecomment-6568676127)):

1. **Dual `MIT OR Apache-2.0` Repository Code License:**
   - Original source code in Open Intelligence Agency (OIA) is dual-licensed under the MIT License ([`LICENSE-MIT`](../../LICENSE-MIT)) and the Apache License, Version 2.0 ([`LICENSE-APACHE`](../../LICENSE-APACHE)), with `license = "MIT OR Apache-2.0"` recorded in the root `[workspace.package]` manifest (`Cargo.toml`).
   - **Third-Party Code Attribution:** Selective ports and design references from God's Eye View (`https://github.com/bilawalsidhu/gods-eye-view`, inspected at commit `0d1a68160e532ba65dc34b48ea205de6de4028fc`) retain Bilawal Sidhu's MIT copyright notice in [`THIRD_PARTY_NOTICES.md`](../../THIRD_PARTY_NOTICES.md) and in the documentation of porting modules.
   - **Exclusion of Third-Party Datasets, Reference Fixtures, and Feeds:** Neither OIA's `MIT OR Apache-2.0` code license nor God's Eye View's MIT code license covers bundled third-party datasets, reference fixtures (such as the NASA/JPL Horizons ephemeris reference responses in `fixtures/sky/`), external documents, media feeds, or models. Each third-party fixture or source carries its own terms and attribution in `THIRD_PARTY_NOTICES.md` and its fixture/source manifest.
   - **Local-First Verification:** Per `AGENTS.md` Section 8, baseline verification (`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, and `cargo test --workspace --locked`, plus `lake build` in `proofs/` once `#15` lands) runs locally against pinned toolchains (`rust-toolchain.toml` at `1.99.0`). Hosted CI/CD workflows are not enabled unless explicitly requested by the project owner.

2. **Open Proposal for P6 (`#49`):**
   - Cross-platform binary installer formats, code-signing certificates, and update manifest distribution channels remain deferred to `#49`.

## Alternatives Considered

- **Single MIT-only or proprietary license:** Superseded by the project owner's selection of the Rust-ecosystem standard `MIT OR Apache-2.0` dual license.
- **Applying the repository code license to bundled datasets/fixtures:** Rejected because third-party datasets and reference responses (e.g., JPL Horizons outputs or OpenStreetMap ODbL extracts) are governed by their upstream terms.

## Validation

- Verified presence of [`LICENSE-MIT`](../../LICENSE-MIT), [`LICENSE-APACHE`](../../LICENSE-APACHE), [`THIRD_PARTY_NOTICES.md`](../../THIRD_PARTY_NOTICES.md), and `license = "MIT OR Apache-2.0"` in `Cargo.toml`, with `cargo test --workspace --locked --offline` passing across all 15 workspace crates.
