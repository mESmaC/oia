# ADR 0002: Transactional Evidence Storage, Managed Blobs, and Indexing (D02)

- **Date:** 2026-11-01
- **Status:** Proposed (awaiting project-owner confirmation on embedded SQLite vs. journaled snapshot store)
- **Decision ID:** D02
- **Issues:** #9, #11, #14
- **Requirements:** `CASE-03`, `CASE-05`, `REPRO-01`, `REPRO-02`

## Context

`D02` requires transactional local persistence for cases, versioned evidence records, permissions, and durable jobs alongside content-addressed managed blobs, schema migrations, reference-aware retention, tombstones, and backup/restore recovery.

## Decision

1. **Implemented P0 Foundation (Transactional Local Store + Content-Addressed Blobs):**
   - `oia-store` implements a transactional local repository (`EvidenceStore`) backed by:
     - An atomic write-ahead journaled state store (`store.json` + `store.wal` + `schema_migrations.json`) with `File::sync_all()` durability on WAL append and temporary-file snapshot promotion (`store.json.tmp` -> `store.json`), schema version validation on open, pre-migration backup snapshots, and rejection of unknown future schema versions with `StoreError::UnsupportedSchemaVersion`.
     - A content-addressed blob directory (`blobs/sha256/<first-2-hex>/<sha256-hex>`) paired with a staging directory (`staging/`). Byte payloads are hashed with FIPS 180-4 SHA-256, written to `staging/` with size-cap enforcement and `sync_all()`, verified on content-addressed deduplication against pre-existing blob files, and promoted into `blobs/` only after dry-running graph insertion and persisting the WAL commit. Rolled-back or interrupted transactions clean up staged blobs without leaving orphaned partial files or mutating existing cases.
     - Reference-aware retention across four explicit classes (`RoutineBuffer`, `TemporaryExtractionCache`, `PinnedCaseEvidence`, `IssuedReportSnapshot`) and explicit deletion via `TombstoneRecord` that removes underlying blob bytes from both the active blob store and pre-migration backups while preserving permitted audit metadata, marking references `AvailabilityState::Removed`, and flagging dependent analyses/reports `NeedsReassessment`.
     - Deterministic inverted token, identifier, temporal-interval, and spatial-bounding-box indexes built in pure Rust with workspace/role permission filtering applied before returning hits or snippets.

2. **Open Owner Decision (`D02`): Embedded SQLite (`rusqlite` + FTS5/R-Tree) vs. Journaled Snapshot Store:**
   - Before `D02` is marked closed in `docs/delivery.md`, the project owner should confirm whether to adopt the pure-Rust journaled snapshot store (`store.json` + `store.wal`) as the permanent P0/P1 store or introduce an embedded SQLite backend (`rusqlite` with WAL mode, FTS5, and R-Tree) before the P1/P4 10,000-document / 100,000-passage scale benchmark.

| Criterion | Journaled Snapshot Store (`store.json` + `store.wal`) | Embedded SQLite (`rusqlite` + FTS5/R-Tree) |
| --- | --- | --- |
| Build & C toolchain footprint | Pure Rust (`std` + `sha2`), zero C compiler dependency | Requires bundled C SQLite (`libsqlite3-sys`) or system SQLite |
| Crash durability & ACID semantics | Explicit `sync_all()` on WAL + atomic snapshot rename; full snapshot rewrite per commit (`O(N)` in store size) | Page-level WAL with incremental `O(delta)` commits and battle-tested crash recovery |
| Full-text, temporal & spatial indexing | Rebuilt in-memory from `EvidenceGraph` on `open()` | Persistent on-disk B-Tree, FTS5, and R-Tree indexes without full reload |
| Scale ceiling | Well-suited for P0 unit/integration suites and small cases (< 5,000 objects) | Required for P1/P4 10,000-document / 100,000-passage interactive latency budgets |

## Alternatives Considered

- **Unvalidated ad-hoc file writes without staging or WAL:** Rejected because interrupted imports or crashes would leave partial cases or unreferenced blobs, violating `A01`, `A17`, and the recovery gate.
- **Mandating an external database server in P0:** Rejected because OIA is local-first and must operate offline without external infrastructure dependencies.

## Validation

- Verified via deterministic tests in `crates/oia-store` covering durable reopen across fresh `EvidenceStore::open` instances, atomic source/artifact commits, simulated crash/interruption rollback and staged-blob cleanup, corrupted pre-existing blob detection, storage-budget exhaustion, schema migration backup and future-schema rejection, single-ref and multi-ref tombstone deletion without silent original pretension, and full backup/restore integrity verification.
