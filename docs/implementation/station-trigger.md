# Station sensor threshold triggers

Implemented dependency-independent slice of [#34](https://github.com/mESmaC/oia/issues/34)
and FIELD-01: `oia_devices::station_trigger` evaluates a caller-authorized numeric
sensor stream in constant memory. It has no device, file, network, notification,
publication or persistence effects. It is an above-threshold trigger, not an
anomaly detector, physical event identification or hardware support claim.

Each evaluator pins one rule version, device ID, signal definition version and
origin class. The signal definition must specify units and normalization; no
unit conversion is performed. Samples from another device, signal version or
origin are refused. Simulations remain labeled and cannot advance an observation
rule. The caller owns source authorization, sample-version uniqueness, source
bytes, clock alignment, freshness and device-health admission. Do not submit
stale held readings as new samples or use generated detail as observations.

A rule requires finite release < trigger thresholds, 1..=65536 consecutive
samples, a positive maximum gap and positive post-window. Pre-window may be zero.
Temporal budgets are unsigned 32-bit milliseconds (at most about 49.7 days each).
The count measures discrete qualifying samples, not continuous duration. Equality
with the trigger threshold qualifies; equality with release rearms. Readings
between release and trigger reset pending persistence but do not rearm a latched
rule. Missing readings reset pending persistence and return `Missing`. Gaps
strictly greater than the configured maximum reset pending persistence and are
reported separately by `gap_before`. Missing readings and gaps never rearm an
already-fired rule; a release reading is required to avoid repeated captures.
Recreate the evaluator explicitly to begin a new run; state is not durable.

Sample UTC epoch-millisecond times must strictly increase. Duplicate or regressing
times are errors; replay uses a fresh evaluator. Ordering and gap decisions use
nominal aligned times, not a claim that uncertainty intervals cannot overlap.
Unknown capture time is unsupported and must be handled before submission. None
is missing, never zero; non-finite numeric values are rejected. All errors leave
state unchanged, including overflow while forming a final capture request.

A fired decision retains the rule, first qualifying sample ID/time, and full
confirming sample. It requests a half-open context window covering all qualifying
samples' uncertainty intervals plus pre/post context. It does not retain the
intermediate readings or substitute for a complete audit log; integrations must
retain the immutable input stream and rule to reproduce the decision. There is
no unbounded event queue or sample buffer. Process returned decisions before
discarding them. No detector score or calibrated probability is produced.

Pass the requested window to the existing [capture planner](station-capture.md)
with an event version and authorized retained inventory after the post-window
has elapsed. The trigger makes no availability assertion: the planner reports
actual referenced coverage, and storage must recheck/pin permitted bytes. A
synthetic trigger must remain labeled on the event created by the caller; the
planner's event ID alone does not encode origin.

## Execution and verification

```text
cargo run -p oia-devices --example station-trigger --locked --offline
cargo test -p oia-devices --test station_trigger --locked --offline
```

The example uses manufactured Celsius readings, including a missing sample. Two
critical-contract fixtures check missing/gap continuity, hysteresis and duplicate
capture suppression, input/origin refusal, transactional errors, and handoff to
the planner without fabricated coverage. They establish simulated Rust behavior,
not a physical sensor benchmark or formal proof.

D07/D08 hardware/source/retention decisions, live adapters, rolling buffers,
durable jobs/storage, permission enforcement, saved history/search, UI and full
A13 remain open on #34 and its dependencies. This implementation neither chooses
providers nor copies unmerged prerequisites. No Lean-covered algorithm changes.

Local verification on 2026-11-03, Windows x86_64 MSVC, Rust 1.99.0, rebased on
`main` at `b13946b`: workspace fmt, all-target Clippy with `-D warnings`, and all
workspace tests passed using locked/offline dependencies. The simulated example
also passed. Staged whitespace, relative link and code-fence checks passed.
`powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run.ps1 proofs`
was attempted but failed because `lake` is missing on PATH. The desktop build
warned that this isolated worktree lacks the optional Cesium vendor assets; no
Globe rendering or live hardware validation is claimed.
