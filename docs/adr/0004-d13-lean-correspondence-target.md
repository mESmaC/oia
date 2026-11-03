# ADR 0004: First Lean targets and Rust correspondence (D13)

- Date: 2026-11-02
- Status: Implemented for the scoped #15 delivery
- Issues: #15; future runtime bridges #10, #12, #14
- Requirements: REASON-01, MARKET-04, MARKET-05, FORECAST-04; model groundwork for GEO-02

## Decision

Ship a pinned Lean 4.35.0 package with frame-selection, dependency-propagation,
and forecast-cutoff model proofs. Establish the first Rust correspondence
fixture against the **merged forecast issuance kernel**, rather than requiring
unmerged P0 APIs. Keep the other two targets labeled model-only. This satisfies
the deterministic target and first fixture scope of #15 without importing the
unmerged #10/#12/#14 implementations or claiming their full acceptance scenarios.

Strict forecast acceptance proves known public availability, ingestion, and
model-knowledge times at or before cutoff for every source. Retrospective
acceptance permits unknown/later ingestion and retains the actual Rust model
knowledge disclosure semantics. Empty sources are rejected. The exact theorem
inventory, projection, source commit/blob, assumptions, numeric domain and
commands are in [the proof manifest](../../proofs/README.md).

Lean and Rust execute separately. Kernel-checked expectations generate a
committed shared fixture, and the local gate detects drift before Rust tests
consume it. Differential fixture agreement is evidence, **not** a formal proof
of the Rust implementation. All production behavior remains in Rust; Cargo
builds and application startup have no Lean dependency.

The full local check requires Lean. It rejects missing tools, admitted goals,
unapproved axiom dependencies, unused project axioms, unimported proof modules,
and mismatched fixture bytes. Only Lean's standard `propext`, `Quot.sound`, and
`Classical.choice` logical axioms are allowed. No hosted CI is enabled.

## Why this target

The historical #15 branch modeled a required ingestion integer and referenced
Rust functions that did not survive the #56 split onto `main`. The merged
`ForecastLedger` uses optional ingestion and separately enforces model-knowledge
cutoffs. Updating the model to that delivered contract gives a useful, reviewable
bridge immediately. Waiting for all foundation work would leave every proof
unavailable on `main`; copying its unmerged implementations would cross ownership
and dependency boundaries. Formal Rust translation/refinement remains future work.

## Historical correspondence corrections

The old frame-selector correspondence claim is explicitly withdrawn. Its
`window_start_ms <= window_end_ms` precondition was too weak: the historical Rust
implementation rejects equal endpoints, while the Lean half-open window model
returns `coverageGap` for `[1000]` in `[1000,1000)` (or `noFramesAvailable` for an
empty catalog). A future bridge must require **strictly increasing endpoints**
or model that validation error explicitly.

The historical Rust selector also used saturating `i64` subtraction. For
`target = i64::MAX`, candidate `i64::MIN`, and `maxGap = i64::MAX`, saturation can
admit a candidate whose exact mathematical age exceeds the gap. Any future
bridge must fix or explicitly restrict that arithmetic. Kernel-checked examples
retain both counterexamples in `OiaProofs/Fixtures.lean`; no Rust frame claim is
made here. The cutoff projection only compares bounded integers and has neither
problem.

Dependency proofs concern reachability and flag preservation over a list model.
They do not establish stored record immutability, production traversal resource
bounds, or completeness for an arbitrary fixed fuel. Each reachable node is
proved to appear for some sufficient fuel.

## Delivery evidence and limits

The [proof manifest](../../proofs/README.md) pins the unchanged Rust source and
documents separate Lean and Rust reproduction commands. The PR records the
immutable model/test commit and actual local check results. Integration with
the future frame selector (#12) and evidence graph (#10/#14) remains tracked
there; completing #15 does not complete those packages or full A07/A14/A15.
