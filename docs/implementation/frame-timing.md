# Frame-timing metadata inspection

This standalone `oia-media::frame_timing` kernel implements a bounded subset of
MEDIA-02 / A10 under #30. It inspects caller-extracted presentation timestamps,
not video bytes. Container/codec demuxing, extraction, frame navigation, capture
clock alignment, persistence, desktop integration and full A10 remain open.
D09 detector/model selection is unchanged. No Lean or implementation-proof claim
is made.

## Contract and interpretation

`inspect_frame_timing` accepts an immutable source SHA-256 reference, a rational
media time base (seconds numerator / ticks denominator), optional declared frame
count, and immutable metadata in strictly increasing presentation-order frame
indices. The indices are zero-based; decode/packet order must not be substituted
for presentation order. Negative media timestamps are retained, not mapped to
UTC. The SHA-256 string is validated syntactically, not recomputed or authenticated.
Acquisition adapters remain responsible for linking the exact retained bytes,
artifact/version identity, workspace access, parser identity/settings and source
metadata. Do not pass a requested frame count as an observed declared total.

The report retains the entire original input and the time base. For consecutive
frame indices, it distinguishes missing, duplicate, backward and forward PTS.
Forward deltas use `i128` subtraction followed by exact `u64` ticks and a `u128`
seconds numerator, with no floating-point rounding, including the full signed
64-bit timestamp range. Fractions are exact and not necessarily reduced.

If the previous frame declares a duration, the forward interval is compared to
that duration: matching, gap or overlap, in ticks. A missing duration remains
unavailable; an explicitly supplied zero remains zero in the retained input.
These are metadata discrepancies, not proof of dropped frames, edits, clock
accuracy or authenticity. Variable frame timing is allowed; no nominal FPS,
constant-rate assumption or threshold is invented.

Skipped frame indices produce `UninspectedFrames`, never an interval or a
dropped-frame assertion. Their PTS remain inspectable in the retained input.
Missing PTS are never interpolated. Selection coverage is separate from timing
availability: `WholeDeclaredTimeline` means every declared frame index was supplied,
not that its timestamps exist or that the caller's declared count is accurate.
Other states distinguish selected frames, unknown total and an empty selection.
Every report carries the interpretation caveat.

## Bounds and failure behavior

The caller must supply a nonzero frame budget. Oversized selections are refused
rather than silently truncated; invalid hashes, zero time-base terms, duplicate
or unordered indices, and indices outside a declared total return typed errors.
Input validation precedes output allocation. Output capacity is reserved fallibly
for at most `supplied_frames - 1` pairs. Runtime and output memory are linear in
the supplied selection; source frames/hash are borrowed without copying.

The cancellation callback runs at entry, per input frame, per output pair and
immediately before returning. Callers may include deadline checks in it. An
interruption returns `Cancelled`, not a partial report or stale successful result.
This synchronous kernel does not provide the shared job scheduler, worker
isolation, late-publication authorization or adapter/parser memory controls.

## Local execution and verification

From the repository root, using pinned Rust 1.99.0:

```text
cargo run -p oia-media --example inspect-frame-timing --locked
cargo test -p oia-media --test frame_timing_contract --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

The example labels its input and illustrative hash as **simulated**. It exercises
negative timing, a missing timestamp, an uninspected frame and a declared-duration
gap at a 1/90,000-second media time base. It does not inspect a file or establish
video support. Three compact contract tests address exact extreme arithmetic and
source preservation, explicit incomplete coverage, and invalid bounds/cancellation.
Execution results are recorded in the PR; this command list is not passing evidence.
