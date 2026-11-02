# ADR 0005: First Lean 4 Verification Target and Rust Correspondence Method (D13)

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
     Models selecting the latest available source frame at or before an investigation target instant `target` within a maximum staleness gap `maxGap` and half-open query window `[windowStart, windowEnd)`, adapted from the weather/imagery clock selection contract in God's Eye View (`src/layers/weather/clock.js`).
     - Proves empty-catalog preservation (`selectFrameInWindow_empty`: `candidates = []` always yields `FrameSelectionOutcome.noFramesAvailable`).
     - Proves window/gap soundness and provenance membership (`selectLatestValid_sound`, `selectLatestValid_mem`, `selectFrameInWindow_selected_sound`: any selected timestamp `t` satisfies `windowStart <= t`, `t < windowEnd`, `t <= target`, `target - t <= maxGap`, and `t ∈ candidates`).
     - Proves completeness and coverage-gap characterization (`selectLatestValid_complete`, `selectFrameInWindow_coverageGap_iff`: `coverageGap` is returned if and only if `candidates ≠ []` and every candidate fails `isValidFrame`).
     - Proves optimality (`selectLatestValid_optimal`: every valid candidate `c ∈ candidates` satisfies `c <= t`).
   - **Target 2 — Point-in-Time Forecast Cutoff Admissibility (`OiaProofs/CutoffAdmissibility.lean`):**
     Models strict prospective vs. retrospective admissibility of evidence records relative to an information cutoff `cutoffMs`.
     - Proves strict cutoff soundness (`strictProspective_requires_known_avail_and_ingest`, `strictProspective_rejects_unknown_avail`, `strictProspective_rejects_late_ingest`, `retrospective_rejects_post_cutoff_observation`: a record admitted in strict prospective mode must have a known public availability timestamp `avail <= cutoffMs` AND an OIA ingestion timestamp `ingestedMs <= cutoffMs`; records with unknown availability are never admitted into strict prospective evaluations).
   - **Target 3 — Dependency Invalidation Monotonicity and Closure (`OiaProofs/DependencyInvalidation.lean`):**
     Models single-step (`stepInvalidation`) and multi-step bounded transitive closure (`invalidateClosure`) for marking direct and transitive downstream dependents as `NeedsReassessment` when an upstream source/claim/calibration version is revised or retracted, while preserving original version immutability.
     - Proves single-step and multi-step monotonicity (`stepInvalidation_monotonic`, `invalidateClosure_monotonic`: reassessment flags are monotonic across propagation steps) and direct-edge closure (`stepInvalidation_includes_direct_dependent`: if `src ∈ flagged` and `(dep, src) ∈ edges`, then `dep ∈ stepInvalidation edges flagged`).

2. **Toolchain and Trusted Base:**
   - Pinned in `proofs/lean-toolchain` to `leanprover/lean4:v4.35.0` (matching the local `Lean 4.35.0` / `Lake 5.0.0` toolchain).
   - Zero `sorry`, zero `admit`, and zero project-specific `axiom` declarations.
   - Lean remains a development and verification tool only; no Lean runtime is linked into any Rust binary.

3. **Rust Correspondence Method and Preconditions:**
   - The Lean functions operate over unbounded integer millisecond timestamps (`Int`) and explicit discrete state enums.
   - The corresponding Rust functions (`oia_time_geo::select_frame_in_window`, `oia_forecast::is_admissible_at_cutoff`, `oia_analysis::compute_invalidated_nodes`, and `oia_domain::EvidenceGraph::propagate_reassessment`) use `i64` millisecond timestamps and isomorphic state enums.
   - **Explicit Domain Preconditions:** `oia_time_geo::select_frame_in_window` enforces two runtime preconditions before delegating to the pure selection logic:
     1. `window_start_ms < window_end_ms` (returning `Err(TimeGeoError::InvertedTimeWindow)` otherwise) and `max_gap_ms >= 0` (returning `Err(TimeGeoError::NegativeMaxGap)` otherwise).
     2. Checked integer subtraction `target_ms.checked_sub(t)` (returning `Err(TimeGeoError::TimestampOverflow)` if `target_ms - t` overflows `i64`).
     For all inputs satisfying `window_start_ms < window_end_ms`, `max_gap_ms >= 0`, and non-overflowing `i64` difference `target_ms - t`, `select_frame_in_window` is behaviorally equivalent to `OiaProofs.selectFrameInWindow`.
   - Correspondence is checked via deterministic cross-check fixtures (`crates/oia-time-geo/tests/lean_correspondence.rs`, `crates/oia-app/tests/lean_correspondence.rs`, and `proofs/OiaProofs/Fixtures.lean`) that evaluate identical boundary cases (empty catalogs, exact boundary timestamps `t == windowStart`, `t == windowEnd`, `target - t == maxGap`, unsorted/duplicate timestamps, unknown availability, post-cutoff ingestion, and 4-node transitive dependency chains) in both `lake build` (`#guard`) and `cargo test`.
   - Verification status in `oia-analysis` (`VerificationStatus`, `VerificationBadge`) distinguishes `VerificationStatus::ModelPropertyProved` and `VerificationStatus::RustCorrespondenceTested` from `VerificationStatus::FormalImplementationProof` (which is never claimed by differential testing alone).

## Validation

- `lake build` in `proofs/` compiles all Lean modules and `#guard` fixtures with zero warnings and zero `sorry`/`axiom` placeholders.
- `cargo test --workspace --locked` executes the Rust-to-Lean correspondence test suites in `oia-time-geo` and `oia-app`.
