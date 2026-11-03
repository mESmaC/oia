# Station event capture planning

Implemented slice of [#34](https://github.com/mESmaC/oia/issues/34), FIELD-01 and
DEVICE-01: `oia_devices::station::plan_capture` produces an immutable in-memory
manifest over a caller-authorized inventory. It neither connects devices nor
copies, pins, publishes, or verifies the existence of artifact bytes. This is a
capture **plan**, not a completed evidence capture or durable retention promise.

Each device retains its source version, current health, and every supplied
artifact version with its origin, clock context, expiry, and selection status.
Disconnected devices can contribute previously retained observations. Expiry is
exclusive; a reference expiring at plan creation is marked expired. The snapshot
does not prevent later eviction: a storage transaction must recheck availability
and pin permitted bytes before reporting a completed capture.

The caller supplies half-open UTC epoch-millisecond intervals and a versioned
alignment reference. This module performs no clock fitting, timezone conversion,
or drift correction; those remain with #12 and the adapters. Unknown capture time
is explicit. Nonzero uncertainty expands the possible-overlap interval using
checked arithmetic; these references remain uncertain and do not fill recorded
coverage. The overlap getter exposes only the clipped nominal interval, not the
expanded uncertainty bounds. This conservative rule may leave gaps even when
some real capture overlap is likely.

Recorded coverage is the sorted union of unexpired, overlapping observations with
zero declared clock uncertainty. It is relative to the supplied inventory and its
alignment declarations, not a measurement of continuous device uptime, truth,
authenticity, or complete station history. Simulations retain their label and
never fill recorded evidence gaps. Gaps mean missing retained coverage, not
observed absence of an event. Discrete samples must not be presented by callers
as continuous intervals; the current API is for interval-bearing artifacts.

The default budget is 32 devices and 4096 total observations, configurable up to
256 and 65536 respectively. Counts are checked before allocating planning sets
or outputs. Overflow refuses the whole plan instead of hiding omitted devices or
observations. IDs use bounded shared domain types. Duplicate device identities,
reused artifact versions across the station, and overflowing clock ranges are
typed errors. Device/artifact ordering is normalized; input inventories remain
unchanged. An empty inventory yields an explicitly empty device list; it makes
no coverage assertion about unregistered devices.

## Execution and scope

Run the labeled synthetic example:

```text
cargo run -p oia-devices --example station-capture --locked --offline
```

It prints one disconnected simulated camera, its selected simulated artifact,
empty recorded coverage, and the entire request as an evidence gap. No real
hardware or acquired data is used. Two focused contract tests in
`crates/oia-devices/tests/station_capture.rs` address overstated coverage, expiry
boundaries, input preservation, deterministic ordering, duplicate identities,
budget rejection, and clock arithmetic overflow.

D07/D08 source, hardware, and retention-policy selection remain open. Live
adapters, event triggers, rolling buffers, durable storage/jobs, searchable
history, UI, authorization enforcement, and transactional byte retention require
the dependencies listed on #34. The planner accepts only an already authorized
inventory; it is not an access-control service. This slice does not complete #34
or full A13, and makes no Lean proof or hardware-support claim.

## Local verification

On 2026-11-03, Windows x86_64 MSVC, pinned `rustc 1.99.0`, based on
`main` at `4ab8590`, these checks passed:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`
- `cargo test --workspace --locked --offline` (including both station contracts)
- The synthetic example command above; its entire event window remained an
  evidence gap despite a selected simulated clip.
- Staged diff whitespace check and inspection of all five changed files.

`powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run.ps1 proofs`
was attempted and failed because `lake` was not available on PATH. No Lean files
or covered algorithms changed. No live hardware, storage pinning, UI, or complete
A13 acceptance was tested.
