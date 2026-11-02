# ADR 0004: First Lean 4 Verification Target and Rust Correspondence Method (D13)

- **Date:** 2026-11-01
- **Status:** Accepted for P0
- **Decision ID:** D13
- **Issues:** #15
- **Requirements:** `REASON-01`, `GEO-02`, `FORECAST-04`

## Context

`D13` and issue #15 require selecting bounded, deterministic algorithmic properties for OIA's first Lean 4 formal specification and establishing an explicit, honest correspondence bridge to the Rust implementation without introducing a Lean runtime dependency into production binaries.

## Decision

1. **Selected Proof Targets (`proofs/OiaProofs/`):**
   - **Target 1 — Bounded Time-Window Source Frame Selection (`OiaProofs/FrameSelection.lean`):**
     Models selecting the latest available source frame at or before an investigation target instant `targetMs` within a maximum staleness gap `maxGapMs` and Half-Open query window `[windowStartMs, windowEndMs)`, adapted from the weather/imagery clock selection contract in God's Eye View (`src/layers/weather/clock.js`).
     - Proves window/gap soundness (`selected_frame_within_bounds`: any returned frame timestamp `t` satisfies `windowStartMs <= t`, `t < windowEndMs`, `t <= targetMs`, and `targetMs - t <= maxGapMs`).
     - Proves empty-catalog preservation and distinction between `NoFramesAvailable` (empty source catalog) and `CoverageGap` (frames exist but none satisfy the window and gap constraints).
     - Proves optimality (`selected_frame_is_latest_valid`: any other valid candidate frame in the input list has timestamp `<=` the selected frame's timestamp).
   - **Target 2 — Point-in-Time Forecast Cutoff Admissibility (`OiaProofs/CutoffAdmissibility.lean`):**
     Models strict prospective vs retrospective admissibility of evidence records relative to an information cutoff `cutoffMs`.
     - Proves strict cutoff soundness (`strict_admissible_implies_known_before_cutoff`: a record admitted in strict prospective mode must have a known public availability timestamp `availMs <= cutoffMs` AND an OIA ingestion timestamp `ingestedMs <= cutoffMs`; records with unknown availability are never admitted into strict prospective evaluations).
   - **Target 3 — Dependency Invalidation Monotonicity (`OiaProofs/DependencyInvalidation.lean`):**
     Models marking direct and transitive downstream dependents as `NeedsReassessment` when an upstream source/claim/calibration version is revised or retracted, while preserving original version immutability.

2. **Toolchain and Trusted Base:**
   - Pinned in `proofs/lean-toolchain` to `leanprover/lean4:v4.35.0` (matching the local `Lean 4.35.0` / `Lake 5.0.0` toolchain).
   - Zero `sorry`, zero `admit`, and zero project-specific `axiom` declarations.
   - Lean remains a development and verification tool only; no Lean runtime is linked into any Rust binary.

3. **Rust Correspondence Method:**
   - The Lean functions operate over integer millisecond timestamps (`Int`) and explicit discrete state enums.
   - The corresponding Rust functions (`oia_time_geo::select_frame_in_window`, `oia_forecast::is_admissible_at_cutoff`, and `oia_analysis::propagate_reassessment`) use exact `i64` millisecond timestamps and isomorphic state enums.
   - Correspondence is checked via deterministic cross-check fixtures (`crates/oia-time-geo/tests/lean_correspondence.rs` and `proofs/OiaProofs/Fixtures.lean`) that evaluate identical boundary cases (empty catalogs, exact boundary timestamps `t == windowStart`, `t == windowEnd`, `target - t == maxGap`, unsorted/duplicate timestamps, unknown availability, post-cutoff ingestion, and transitive dependency chains) in both `lake env lean` and `cargo test`.
   - Verification status in `oia-domain` / `oia-analysis` distinguishes `VerificationStatus::ModelPropertyProved` and `VerificationStatus::RustCorrespondenceTested` from `VerificationStatus::FormalImplementationProof` (which is not claimed by differential testing alone).

## Validation

- `lake build` in `proofs/` compiles all Lean modules with zero warnings and zero `sorry`/`axiom` placeholders.
- `cargo test --workspace --locked` executes the Rust-to-Lean correspondence test suite.
