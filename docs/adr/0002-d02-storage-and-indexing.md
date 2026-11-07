# ADR 0002: Transactional Evidence Storage, Managed Blobs, and Indexing (D02)

- **Date:** 2026-11-02
- **Status:** Accepted ([project-owner decision on 2026-11-02](https://github.com/mESmaC/oia/pull/56#issuecomment-6568675966); runtime implementation tracked in `#11` and `#14`)
- **Decision ID:** D02
- **Issues:** #9, #11, #14
- **Requirements:** `CASE-03`, `CASE-05`, `REPRO-01`, `REPRO-02`

## Context

`D02` requires choosing OIA's local transactional persistence engine and indexing strategy for cases, versioned evidence records, permissions, and durable jobs alongside content-addressed managed blobs, schema migrations, reference-aware retention, tombstones, and backup/restore recovery.

## Decision

Per the project owner's decision on 2026-11-02 ([#56 comment 6568675966](https://github.com/mESmaC/oia/pull/56#issuecomment-6568675966) and [#9 comment 6568676127](https://github.com/mESmaC/oia/issues/9#issuecomment-6568676127)):

1. **Embedded SQLite (`rusqlite` in WAL Mode) + Content-Addressed Managed Blobs in P0 (`#11`):**
   - `oia-store` will use embedded SQLite via a pinned `rusqlite` dependency configured with write-ahead logging (`PRAGMA journal_mode = WAL`), `PRAGMA foreign_keys = ON`, and SQLite FTS5 / R-Tree indexes as needed for permission-aware text, temporal, and spatial queries, paired with content-addressed blob files on disk (`blobs/<first-2-hex>/<sha256-hex>`) and a transaction staging directory (`staging/`).
   - Required transactional and recovery invariants for `#11`:
     - **Staged blob durability and hash verification:** Incoming bytes are written to `staging/`, synced, verified by SHA-256 content hash (including verifying pre-existing destination blobs before deduplication), and promoted in order with the SQLite transaction commit so a failed or interrupted operation never leaves partial case state.
     - **Atomic batch commits:** Multi-record mutations (such as recording an annotation and updating its owning investigation) commit atomically within a single SQLite transaction.
     - **Tombstone ordering:** Explicit deletion persists the `TombstoneRecord` and `AvailabilityState::Removed` state in SQLite and flags dependent records `NeedsReassessment` *before* unlinking physical blob bytes when no live references remain.
     - **Invariant validation on open/reload:** Opening or restoring a store validates schema version compatibility, identifier constraints, `supersedes` lineage, and transitive non-observational origin-class rules (`AGENTS.md` §2 and §5).
     - **Durable job lease fencing:** Lease epoch increments and interrupted-job recovery propagate persistence errors rather than silently ignoring failed writes.

2. **Scope in This `#9` Bootstrap PR:**
   - In `#9`, `crates/oia-store` is a zero-dependency workspace boundary stub so the 15-crate workspace builds offline (`--locked --offline`). The `rusqlite` dependency, SQLite schema, managed blob store, and recovery test suite are delivered in the `#11` split PR.

## Alternatives Considered

- **Custom JSON snapshot + WAL file store (`store.json` + `store.wal`):** Evaluated during initial P0 prototyping; superseded by the project owner's decision to use embedded SQLite (`rusqlite` with WAL mode, FTS5, and R-Tree) from P0 rather than deferring SQLite to P1.
- **Unvalidated ad-hoc file writes without transactions or staging:** Rejected because interrupted imports or crashes would leave partial cases or unreferenced blobs, violating `A01`, `A17`, and the recovery gate.
- **External database server in P0:** Rejected because OIA is local-first and must operate offline without external server infrastructure.

## Validation

- In `#9`: verified that the `crates/oia-store` boundary stub compiles cleanly under workspace lints (`cargo clippy --workspace --all-targets --locked --offline -- -D warnings`).
- In `#11` (planned): deterministic SQLite + managed blob tests covering durable reopen, atomic batch commits, tombstone-before-unlink ordering, invariant validation on open, backup/restore, and lease-fence recovery.

### First runtime slice (2026-11-05)

[`oia-store::LedgerStore`](../../crates/oia-store/src/sqlite.rs) persists one canonical
evidence ledger in a SQLite WAL transaction. `rusqlite` is pinned to 0.41.1 with
bundled SQLite 3.54.1 (`libsqlite3-sys` 0.39.1), avoiding an unconfigured system
SQLite requirement. This dependency stays in the storage adapter. Schema 1 stores
the domain operation document as a bounded BLOB with a revision counter; it does
not yet create case/search/job tables or managed blobs. See the
[implementation and recovery evidence](../implementation/sqlite-ledger.md).
The remaining invariants above, including staging, tombstones, backup/restore and
lease fencing, remain required scope on #11/#14.
