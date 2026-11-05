# Lean model proofs and Rust correspondence

This package checks selected mathematical properties with Lean **4.35.0**.
It is a local development tool, with no Cargo build hook, production subprocess,
Lean runtime linkage, Mathlib dependency, or hosted workflow. See
[ADR 0004](../docs/adr/0004-d13-lean-correspondence-target.md) for D13 and the
[architecture boundary](../docs/architecture.md#11-rust-and-lean-verification-boundary).

## Claims

| Model | Proved properties | Rust connection |
| --- | --- | --- |
| [FrameSelection](OiaProofs/FrameSelection.lean) | Selected timestamp belongs to the input, satisfies the half-open window and staleness bounds, and is latest among valid candidates. Empty catalogs and coverage gaps remain distinct; gap characterization is complete. | **Model only.** The historical Rust selector is not on `main`; #12 owns the future bridge. |
| [CutoffAdmissibility](OiaProofs/CutoffAdmissibility.lean) | Strict acceptance requires known public availability, ingestion, and model knowledge at or before cutoff for **every** source. Empty source lists fail. Strict acceptance implies retrospective acceptance; successful strict results never carry the retrospective model disclaimer, and admitted unknown/future model knowledge always does. | **Rust behavior tested against the model** on the 24 cases below; not a formal implementation proof. |
| [DependencyInvalidation](OiaProofs/DependencyInvalidation.lean) and [DependencyBounds](OiaProofs/DependencyBounds.lean) | Propagation preserves flags and marks only reachable nodes. Membership at a given fuel is exactly reachability along a seed-origin path of at most that many edges. Membership is monotone in fuel, seeds, and edges; a membership fixed point is complete. | **Model only.** The historical graph APIs are not on `main`. `oia_domain::evidence::EvidenceLedger` has a similar walk, but no correspondence is established; #10/#14 own the future bridge. Finite arbitrary fuel is not claimed to compute a complete transitive closure. |

The theorem statements in those files are the specifications. Frame timestamps
are unbounded integers; validity uses exact subtraction. Dependency nodes are
natural numbers and edges are `(dependent, upstream)` pairs. Seeds are included,
duplicates are permitted, and the model does not prove version immutability,
storage behavior, termination of a production traversal, or a resource bound.
Neither timestamps alone nor membership in a list establish source authenticity.

## Bounded dependency coverage

The model-only follow-up in #142 adds `DependencyPath edges origin node hops`,
an inductive path specification independent of the list-propagation function.
Zero edges reach the origin; every path extension follows a stored
`(dependent, upstream)` edge from upstream to dependent and increases the edge
count by one. Node IDs and hop counts are unbounded natural numbers. Edges and
seeds are finite lists; cycles, self-loops, repeated nodes, duplicate edges, and
duplicate seeds are allowed. No acyclicity assumption is required.

The kernel checks these results in `OiaProofs/DependencyBounds.lean`:

| Theorem | Guarantee |
| --- | --- |
| `stepInvalidation_mem_iff` | One round retains a flagged node or follows exactly one edge from a flagged upstream. |
| `DependencyPath.mem_closure` | A seed-origin path of exactly `hops` edges is included after `hops` rounds. |
| `invalidateClosure_mem_iff_bounded_path` | A node is included after `fuel` rounds iff a seed-origin path exists with at most `fuel` edges. |
| `invalidateClosure_not_mem_iff_long_paths` | A node is excluded at this budget iff every seed-origin path to it is longer than the budget, including the vacuous case of no path. Exclusion alone does not establish unreachability. |
| `invalidateClosure_fuel_mono` | Increasing fuel cannot remove membership. |
| `invalidateClosure_mono` | Adding seeds or edges cannot remove membership at the same fuel. |
| `invalidateClosure_membership_congr` | Equal seed/edge membership gives equal result membership; reordering and duplication do not affect the flagged set. |
| `invalidateClosure_append_seeds` | Propagation from combined seed lists has the union of their separately propagated memberships. |
| `stepInvalidation_closed_iff` | A round adds no new membership iff the flagged set is closed under direct dependency edges. |
| `invalidateClosure_closed_mem_iff` | A closed flagged set retains exactly its membership for every further fuel budget. |
| `invalidateClosure_fixed_point_complete` | If a bounded result is closed under one further round, its membership is exactly all nodes reachable from the original seeds. |

The fixed-point hypotheses concern **membership**, not equality of lists. The
existing algorithm retains duplicates, so list length can continue to grow after
membership stabilizes. These results do not give a universal sufficient fuel
bound, bound allocations or execution time, prove a production traversal, or
establish Rust correspondence. They support inspectable conditional reasoning
under REASON-01 and ADR 0004, not complete A07/A16 integration. Two compact
kernel-checked examples retain the concrete insufficient-fuel counterexample
and exercise zero fuel, a cycle, duplicate edges/seeds, a disconnected self-loop,
and empty seeds. See the [validation record](DEPENDENCY-BOUNDS-VALIDATION.md)
for the checked model revision, commands, and remaining limitations.

## Reproduce

Install [elan](https://github.com/leanprover/elan), then install the pinned release:

```text
elan toolchain install leanprover/lean4:v4.35.0
```

From the repository root:

```text
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run.ps1 proofs
cargo test -p oia-forecast --test lean_correspondence --locked
```

On POSIX shells/Git Bash use `scripts/run.sh proofs`. `scripts/run.ps1 check`
or `scripts/run.sh check` requires the proof gate plus Rust formatting, Clippy,
and workspace tests. Missing Lake, missing proof files, or a failed proof gate
return nonzero. `fmt`, `lint`, and `test` remain explicit Rust-only commands.
The first toolchain install may need network access; Lean has no package downloads.

The proof-only gate executes these independently of Rust:

```text
cd proofs
lake build
lake env lean Audit.lean
lake env lean --run ExportFixtures.lean --check
```

`lake build` checks the models and the default `ProofAudit` target. The explicit
audit rerun also catches new `.lean` files missing from the root import graph
when Lake's existing targets are cached. Import every model module through
`OiaProofs.lean`. [Audit.lean](Audit.lean) traverses every declaration in those
modules (including private helpers and names outside the `OiaProofs` namespace),
rejects project axiom declarations even if unused, and examines transitive axiom
dependencies. The only permitted logical axioms are `propext`, `Quot.sound`, and
`Classical.choice` from Lean. `sorryAx`, project axioms, and `native_decide`'s
additional trust mechanism are rejected. Warnings are also errors in the package.
This is a guard against accidental unsupported claims, not a sandbox for hostile
Lean metaprograms or changes to the audit itself.

## Cutoff correspondence contract

**Pinned Rust input:** commit
`899ab55a4adcd5306581881c2e960b3a9db9a252`,
[`crates/oia-forecast/src/issuance.rs`](../crates/oia-forecast/src/issuance.rs),
Git blob `d3580d7358e57a957a4d63925d0e6be41e81b084`.
The tested path is `ForecastLedger::issue_forecast`, through its private
`validate_cutoff_and_sources` function. Production Rust is unchanged here.
The [local validation record](VALIDATION.md) identifies the exact model/test
commit checked and the positive and negative gate results.

The projection maps `ProspectiveStrict` to `strictProspective`,
`RetrospectiveReconstructed` to `retrospectiveReconstruction`, `Option<i64>`
timestamps to `Option Int`, and unique source versions to their timing pairs.
The observed output is cutoff rejection or success with the retained
`post_cutoff_model_knowledge_disclosed` flag. Exact error ordering is outside the
model. The Rust fixture fails if another class of error causes a rejection.

The model assumes all other Rust preconditions hold: known timestamps lie in
`[-62135596800000, 253402300799999]`; source versions are unique; the collection
is within 1,024 entries; retrospective limitation text is valid and nonempty;
any event time is within range and at or before public availability; identifiers,
probabilities, horizons, version history and ledger capacity are valid.
An empty source list is modeled explicitly as rejection. The fixture uses a
fresh ledger, distinct source versions, no event time, and a one-millisecond
horizon. It includes negative and minimum timestamps and a cutoff one millisecond
below the maximum so its horizon remains valid. No floating-point algorithm is
covered; cutoff arithmetic consists only of exact comparisons, so no overflow
assumption is needed within that Rust range.

[Fixtures.lean](OiaProofs/Fixtures.lean) contains the single authoritative set of
24 synthetic cutoff cases. `cutoffFixtures_correct` uses kernel-checked `decide`
to establish every expected result. [ExportFixtures.lean](ExportFixtures.lean)
serializes those inputs and expected results to [fixtures/cutoff.tsv](fixtures/cutoff.tsv).
The gate compares the exported bytes to the committed file, and the
[Rust integration test](../crates/oia-forecast/tests/lean_correspondence.rs)
consumes that same file. Cases exercise equality, future/unknown public,
ingestion and model times, retrospective disclosure, empty lists and a failing
second source. The frame/graph examples are model-only checks.

After an intentional model/fixture change, regenerate and review the diff:

```text
cd proofs
lake build
lake env lean --run ExportFixtures.lean --write
```

Then run both gates. Tests supply finite evidence; no translation, refinement
proof, compiler verification, or formal Rust-to-Lean equivalence is claimed.
This establishes a narrow temporal part of A14/A15 and inspectable assumptions
for REASON-01, not full A07/A14/A15 integration, empirical forecast quality,
source truth, calibrated probabilities, or validity of timestamp metadata.

## Origin and remaining work

Frame selection and the original dependency proofs are retained from OIA commit
`aba70b203e68786c073491989cfdbf3c3fc24f05`, paths
`proofs/OiaProofs/{FrameSelection,DependencyInvalidation}.lean`, authored for #15
before the #56 bootstrap split. The dependency completeness theorem and the
current cutoff model/bridge are added here. No unmerged Rust prerequisites are
copied. Historical claims about those missing Rust APIs are withdrawn.

Track the eventual frame/dependency runtime bridges on #12 and #10/#14 with #15
as context. Each needs its own immutable Rust revision, numeric/precondition
review, shared fixture and verification record before a correspondence label.
The additional bounded-coverage and stability proofs are tracked separately in
#142; they do not implement those runtime bridges or change the original model.
