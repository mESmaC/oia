# Bounded dependency model verification

Executed 2026-11-03 (America/New_York) for #142, Codex session
`lean-dependency-bounds-20261103`. This supplements the original #15
[verification record](VALIDATION.md); it does not replace its historical results.

## Checked inputs

- Model and root-import revision: **`e8812243a7bb455358b960bbbf8ce91472bbb228`**,
  based on `main` **`433e253eccc678f6ae2afeb3e0c0123dac40e84e`**.
- New `OiaProofs/DependencyBounds.lean` Git blob:
  `751af74b41e95fd053a37b9d38fa94ea395e82be`.
- Unchanged `OiaProofs/DependencyInvalidation.lean` Git blob:
  `d4f7942c32b802b60ce3569fdda36662347ad3eb`.
- Root `OiaProofs.lean` Git blob:
  `f31a4aecba5c6b67e5807997d583863206102f0c`.
- Windows build 10.0.26200, `x86_64-pc-windows-msvc`; rustc 1.99.0
  (`b940084d7`, 2026-09-28).
- Lean 4.35.0, `x86_64-w64-windows-gnu`, commit
  `eb6cafb7290e8bd5f2937522d8d804f58f02bafc`; Lake 5.0.0.
- Existing cutoff fixture, Rust source, toolchain pins, Cargo dependencies,
  scripts, and axiom-audit implementation are unchanged.

## Executed checks

| Command/check | Actual result |
| --- | --- |
| From `proofs/`: `lake build`, `lake env lean Audit.lean`, `lake env lean --run ExportFixtures.lean --check` | Exit 0 for each. Build completed 10 jobs; audit covered 290 declarations and permitted only standard Lean logical axioms; cutoff fixture bytes matched. |
| `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run.ps1 proofs` | Exit 0, including a rerun after removing the deliberate audit mutation. |
| Git Bash: `bash scripts/run.sh proofs` | Exit 0 with the same audit count and fixture match. |
| `cargo +1.99.0 test -p oia-forecast --test lean_correspondence --locked --offline` | Exit 0; `ledger_cutoffs_match_kernel_checked_lean_fixtures` passed, comparing all 24 existing cases. This is the existing cutoff bridge, not a new dependency-graph bridge. |
| New theorem and boundary checks | 11 named theorems and two compact `decide` examples checked by Lean. No admitted goals, custom axioms, `native_decide`, or additional dependencies. |
| Documentation and staged diff | Relative local links, theorem names, REASON-01/A07/A16 references, Markdown fences, and staged `git diff --check` reviewed, including new files. |

## Audit participation check

Temporarily inserted the unused declaration
`axiom dependencyBoundsAuditSentinel : True` inside the new model module and ran
the PowerShell proof gate. It exited **1**, specifically reporting
`Project axiom is forbidden: OiaProofs.dependencyBoundsAuditSentinel`.
This establishes that the new module participates in the existing transitive
declaration audit, not merely that it compiles independently. Removal ran in a
`finally` block; SHA-256 equality confirmed restoration of the original source
bytes. The complete positive proof gate passed again after restoration. No
sentinel declaration or change to audit policy is retained.

## Claim boundaries

**Proved (model):** `invalidateClosure_mem_iff_bounded_path` gives an exact
edge-counted reachability specification for every finite fuel budget.
`invalidateClosure_not_mem_iff_long_paths` distinguishes exclusion at that budget
from unreachability. Supporting theorems establish monotonicity, seed-union and
membership invariance, and completeness conditional on a membership fixed point.
The [manifest](README.md#bounded-dependency-coverage) lists each theorem and its
assumptions. Natural-number nodes and hop counts are unbounded; arbitrary finite
edge/seed lists can contain cycles, self-loops, and duplicates.

**Not established:** a universal sufficient fuel bound, resource/latency limits,
bounded-memory traversal, frozen-record immutability, source authenticity, or
formal correspondence to Rust. In particular, membership stabilization does not
imply list equality or bounded growth of the duplicate-preserving model.

There is no production Rust change or new dependency-graph correspondence
fixture. The existing cutoff correspondence remains finite test evidence only.
No full workspace fmt/Clippy/test run, Linux/macOS execution, hosted CI, runtime
acceptance scenario, or complete A07/A16 integration is claimed for this
model-only follow-up. The #10/#14 graph bridge and #12 frame bridge remain with
their existing owners.
