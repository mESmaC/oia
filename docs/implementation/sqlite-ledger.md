# SQLite evidence-ledger persistence

This implements a bounded slice of [#11](https://github.com/mESmaC/oia/issues/11)
using [ADR 0002](../adr/0002-d02-storage-and-indexing.md)'s accepted SQLite choice.
It supports [CASE-03](../spec.md#31-investigation-workspace) and the persistent
history/original-preservation subset of [A01](../delivery.md#a01-persistent-investigation-and-asynchronous-work).
It does not complete A01, A17 or #11.

## API and behavior

`oia_store::LedgerStore` holds one canonical
[`EvidenceLedger`](evidence-ledger.md) per local database file. Callers load a
`StoredLedger`, append through the domain API, serialize with `to_json`, and call
`append_json(snapshot.revision, &json)`. There is no second evidence vocabulary.

- New databases start at revision zero with an empty ledger. `open_existing`
  refuses missing or uninitialized stores; it never creates an empty investigation.
- Saves replay the complete candidate under configured domain limits. A SQLite
  `BEGIN IMMEDIATE` transaction reads the current revision/history, checks the
  caller's revision, and updates the snapshot and revision together. SQLite commits
  with `synchronous=FULL` in WAL mode. Lock waits are limited to two seconds.
- Every committed operation must remain an identical prefix: entry order, all
  record fields, predecessor/rationale/labels and all relation fields are checked.
  Removing records or changing text under an existing version ID is refused even
  when the candidate is independently valid. New corrections append versions.
- Another writer's committed revision produces `Conflict`; callers must reload
  and reconcile their intended append. No automatic merging occurs. An equivalent
  ledger is a no-op and retains both revision and existing bytes.
- A successful append retains the supplied JSON's exact UTF-8 bytes. On each
  open/load/save, strict domain replay revalidates identities, version links,
  origins, unknowns and relations, and recomputes reassessment flags and labels.
  No failure returns a partial ledger or silently replaces invalid stored content.
- Store identity (`application_id = 0x4f49414c`) and store schema (`user_version = 1`)
  are distinct from the domain JSON format/schema. Foreign nonempty databases and
  unknown versions are refused before journal-mode or schema changes. Initialization
  is transactional. No previous store schema exists and no upgrade is implemented.
- `StoreError::Database` retains SQLite's typed error and error code, including
  lock contention, disk exhaustion, read-only access and I/O failures. Validation,
  history conflicts, compatibility and byte budgets have separate error variants.

The adapter uses `rusqlite = 0.41.1` with its `bundled` and `limits` features;
`libsqlite3-sys = 0.39.1` builds SQLite 3.54.1. This keeps the database engine out
of `oia-domain` and removes a requirement for an arbitrary system SQLite version.
Use a local filesystem supporting SQLite WAL locking. The database is local
application state, not an accepted bundle/import format; do not open untrusted
SQLite files as research documents. No source content chooses its path or SQL.

## Limits and remaining scope

`StoreLimits` defaults to 8 MiB of JSON and the domain's `LedgerLimits::default()`.
The configurable JSON ceiling is 64 MiB, matching the domain codec. Input length
is checked before parsing; SQLite's length limit is configured to the selected
budget plus 4 KiB of row overhead. A lower budget can refuse a previously valid
store; it does not truncate evidence. Very small limits can also prevent schema
initialization. Inspect the configured budget rather than retrying with silent
truncation. The example prints its budget on every successful operation.

This is a whole-ledger snapshot adapter, not a query index. Parsing/replay is
synchronous and uses memory proportional to the JSON and graph; it inherits the
domain codec's double parsing and propagation costs. #10 tracks the independent
propagation-work budget follow-up. There is no cancellation or interactive
latency guarantee. The two-second lock timeout does not bound domain replay.
WAL size can exceed a snapshot during writes or while readers hold old snapshots;
the journal-size setting limits retained WAL after reset, not peak disk use.

There are no managed originals/blobs, source/artifact commits, retention classes,
tombstones, garbage collection, backup/restore, destructive schema migrations,
case/query indexes, encryption, application authorization, UI integration or durable
jobs yet. Stored access-policy references are preserved data, not enforcement;
authorized application services remain #14. A database can contain records from
multiple scopes and must only be given to an already-authorized local caller.
Copying a live `.db` alone is not a supported backup. No content hash establishes
source truth, and this slice makes no authenticity or formal-proof claim.

## Runnable local path

The existing correction fixture is synthetic. These commands append it and then
reopen it in a second process; the parent directory must already exist:

```text
cargo run -p oia-store --example ledger-store --locked -- append target/ledger-demo.db 0 fixtures/domain/evidence-ledger-correction.json
cargo run -p oia-store --example ledger-store --locked -- inspect target/ledger-demo.db
```

`inspect` writes the canonical domain JSON to stdout and the revision/entry count
to stderr. `append` bounds file reads and validates JSON before opening/creating
the store. It requires the expected revision explicitly. Exit zero means success;
errors exit nonzero. It performs no network acquisition or device operations.

## Verification

Focused critical-contract assertions are in
[`sqlite/tests.rs`](../../crates/oia-store/src/sqlite/tests.rs):

1. The correction fixture survives close/reopen byte-for-byte, including unknowns,
   original wording and downstream flags. Two independent connections demonstrate
   stale-save refusal. Altered/truncated history, malformed and oversized JSON are
   rejected without changing committed content; equivalent saves do not advance.
2. A child process spills an uncommitted SQLite update to WAL and exits without
   running destructors. Reopen retains the last committed ledger. A per-connection
   SQLite page ceiling then forces `SQLITE_FULL` on the actual adapter save path;
   both immediate reload and a second reopen retain the committed revision/bytes.
   This is deterministic process interruption and simulated allocation exhaustion,
   not a physical power-loss or full-device experiment.
3. Missing inspection targets, unrelated databases, future store schemas, smaller
   configured budgets and malformed stored payloads are refused. Unknown schemas
   and foreign databases retain their original bytes; malformed payloads are not
   repaired or discarded.

Local validation (2026-11-05): Windows x86_64, pinned Rust 1.99.0, base `main` at
`f039c66`:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
  After adding the final append/reopen assertions, the changed crate's all-target
  Clippy check passed again.
- `cargo test --workspace --locked`: 230 passed, zero failed, including the three
  storage tests above. The child interruption process is part of its parent test.
- `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run.ps1 proofs`:
  Lean 4.35.0 gate passed, 290 declarations audited, cutoff fixture bytes matched.
  No proof or covered algorithm changed.
- The two example commands above passed in separate processes; reopened canonical
  JSON matched the existing correction fixture exactly after checkout-line-ending
  normalization. Relative documentation targets, code fences and diff whitespace
  were checked.

The desktop build reported the already-documented absent gitignored Cesium vendor
directory. No Globe/UI path was exercised. No hosted workflows, hardware,
production deployment or Rust–Lean correspondence claim is involved.
