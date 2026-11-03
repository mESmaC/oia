# Local verification record

Executed 2026-11-02 (America/New_York) for #15, Codex session `lean-proofs-15`.

- Model, gate, fixture and Rust test revision: **`de363b7fea04c933a15848e07d3a17cef6b3f88d`**.
- Base: `e8e6b9c` (rebased onto current `main` before the final full check).
- Unchanged Rust cutoff implementation: commit `899ab55a4adcd5306581881c2e960b3a9db9a252`,
  `crates/oia-forecast/src/issuance.rs`, blob `d3580d7358e57a957a4d63925d0e6be41e81b084`.
- Windows build 10.0.26200, `x86_64-pc-windows-msvc`, rustc 1.99.0
  (`b940084d7eb6a299eb4bfeb8e34901bc051e7ac4`).
- Lean 4.35.0, `x86_64-w64-windows-gnu`, commit
  `eb6cafb7290e8bd5f2937522d8d804f58f02bafc`; Lake 5.0.0.

## Positive checks

| Command/check | Actual result |
| --- | --- |
| `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run.ps1 check` | Exit 0 after rebase: Lean gate, `cargo +1.99.0 fmt --all -- --check`, workspace/all-target Clippy with `--locked --offline -- -D warnings`, and `cargo +1.99.0 test --workspace --locked --offline`. 93 Rust tests passed; 0 failed or ignored. |
| `cargo test -p oia-forecast --test lean_correspondence --locked` | Exit 0; one focused test compares all 24 Lean fixture cases against the real ledger path. Also passed within the final workspace run. |
| `scripts/run.ps1 proofs` and Git Bash `scripts/run.sh proofs` | Both exit 0 after restoring negative mutations. `lake build` completed 9 jobs; audit covered 263 project declarations; fixture bytes matched. |
| Lean kernel | 26 named theorems across the three model modules and fixture theorem, plus model-only boundary examples. No admitted/custom-axiom dependencies. |
| Documentation and diff inspection | Relative links/anchors and fences checked in the proof manifest and ADR; requirement IDs checked against `docs/spec.md`; staged and branch `git diff --check` passed. |
| Git attributes | `proofs/fixtures/cutoff.tsv` has `eol=lf`, preserving exporter bytes on Windows checkout. |

## Gate failure checks (L1/L2)

Temporary edits were made only in this task's worktree and restored byte-for-byte
in `finally` blocks. Each check asserted both a nonzero exit and its expected
diagnostic; an unrelated compilation failure did not count as a pass.

| Deliberate condition | Command | Result |
| --- | --- | --- |
| Remove directories containing `lake.exe` from the child process PATH | PowerShell `scripts/run.ps1 check` | Exit 1: Lake required, with elan installation command. |
| Same missing-Lake environment | Git Bash `scripts/run.sh check` | Exit 1 with the same actionable requirement. |
| Append unused `axiom gateUnusedAxiom : True` outside the project namespace to `OiaProofs.lean` | PowerShell `scripts/run.ps1 proofs` | Exit 1: project axiom forbidden. |
| Append `theorem gateAdmitted : True := by sorry` with `warningAsError` locally disabled | Same proof gate | Exit 1: the transitive axiom audit finds `sorryAx` even with the warning suppressed. |
| Append `theorem gateNativeDecision : 1 = 1 := by native_decide` | Same proof gate | Exit 1: unapproved axiom dependency. |
| Add `OiaProofs/GateUnimported.lean` without importing it, while existing build targets are cached | Same proof gate | Exit 1: project module not imported by `OiaProofs.lean`. |
| Change the first accepted TSV expectation to rejected | Same proof gate | Exit 1: cutoff fixture differs from Lean. |

L3 is resolved by withdrawing the historical frame correspondence claim,
documenting strict window preconditions and saturating arithmetic, and retaining
both mathematical counterexamples in `Fixtures.lean`. L4 is resolved by the
immutable model/test and Rust source identifiers above. The historical proof
source attribution is in [README.md](README.md#origin-and-remaining-work).

The negative checks preceded the final rebase; the proof/gate/test contents are
unchanged by that rebase. The full positive gate and all Rust checks passed again
at the revision above. This record is added in a subsequent documentation commit
to avoid a self-referential commit hash.

No Linux/macOS run, hosted CI, empirical forecast evaluation, complete A07/A14/A15
integration, or formal Rust implementation proof is claimed. Frame selection
and dependency propagation are model-only until their respective runtime bridges
are delivered in #12 and #10/#14. Production code and Cargo dependencies are
unchanged by #15.
