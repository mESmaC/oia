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
     - An atomic write-ahead journaled state store (`store.json` + `store.wal`) with `File::sync_all()` durability on WAL write (`store.wal`) and temporary-file snapshot promotion (`store.json.tmp` -> `store.json`), schema version validation on open (`EvidenceGraph::restore_and_validate`), WAL replay validation on startup (replaying valid `store.wal` snapshots and discarding torn/corrupt `store.wal` files without clobbering `store.json`), pre-migration backup snapshots (`backups/pre_migration_v<from>_to_v<to>`), and rejection of unknown future schema versions with `StoreError::UnsupportedFutureSchema`.
     - A content-addressed blob directory (`blobs/<first-2-hex>/<sha256-hex>`) paired with a staging directory (`staging/`). Byte payloads are hashed with the pure-Rust FIPS 180-4 SHA-256 implementation (`oia_domain::sha256::sha256_hex`), written to `staging/` with size-cap and storage-budget enforcement (`StoreError::BlobExceedsMaxSize`, `StoreError::StorageBudgetExceeded`) and `sync_all()`, verified on content-addressed deduplication against pre-existing blob files (`StoreError::CorruptedExistingBlob`), and promoted into `blobs/` only after dry-running graph insertion (`EvidenceGraph::insert_batch_validated`). Rolled-back or interrupted transactions clean up staged blobs without leaving orphaned partial files or mutating existing cases.
     - Reference-aware retention across four explicit classes (`RetentionClass::RoutineBuffer`, `RetentionClass::TemporaryExtractionCache`, `RetentionClass::PinnedCaseEvidence`, `RetentionClass::IssuedReportSnapshot`) and explicit deletion via `TombstoneRecord` that persists tombstone state before unlinking underlying blob bytes (and sweeps tombstoned blobs on startup), marks references `AvailabilityState::Removed`, and flags dependent analyses/reports `NeedsReassessment`.
     - `oia-query` (`QueryIndex`) executes permission-filtered full-text substring, object-type, origin-class, temporal-interval overlap, and geographic bounding-box searches over `EvidenceStore` with access checks applied before snippet generation (`QueryError::UnauthorizedCaseAccess`).

2. **Open Owner Decision (`D02`): Embedded SQLite (`rusqlite` + FTS5/R-Tree) vs. Journaled Snapshot Store:**
   - Before `D02` is marked closed in `docs/delivery.md`, the project owner should confirm whether to adopt the pure-Rust journaled snapshot store (`store.json` + `store.wal`) as the permanent P0/P1 store or introduce an embedded SQLite backend (`rusqlite` with WAL mode, FTS5, and R-Tree) before the P1/P4 10,000-document / 100,000-passage scale benchmark.

| Criterion | Journaled Snapshot Store (`store.json` + `store.wal`) | Embedded SQLite (`rusqlite` + FTS5/R-Tree) |
| --- | --- | --- |
| Build & C toolchain footprint | Pure Rust (`std` + `oia_domain::sha256`), zero C compiler dependency | Requires bundled C SQLite (`libsqlite3-sys`) or system SQLite |
| Crash durability & ACID semantics | Explicit `sync_all()` on `store.wal` + atomic snapshot rename; full snapshot rewrite per commit (`O(N)` in store size) | Page-level WAL with incremental `O(delta)` commits and battle-tested crash recovery |
| Full-text, temporal & spatial querying | In-memory permission-filtered scan over `EvidenceGraph` loaded on `open()` | Persistent on-disk B-Tree, FTS5, and R-Tree indexes without full reload |
| Scale ceiling | Well-suited for P0 unit/integration suites and small cases (< 5,000 objects) | Required for P1/P4 10,000-document / 100,000-passage interactive latency budgets |

## Alternatives Considered

- **Unvalidated ad-hoc file writes without staging or WAL:** Rejected because interrupted imports or crashes would leave partial cases or unreferenced blobs, violating `A01`, `A17`, and the recovery gate.
- **Mandating an external database server in P0:** Rejected because OIA is local-first and must operate offline without external infrastructure dependencies.

## Validation

- Verified via deterministic tests in `crates/oia-store` covering durable reopen across fresh `EvidenceStore::open` instances, valid and torn WAL recovery, tampered `store.json` rejection, atomic source/artifact commits, simulated crash/interruption rollback and staged-blob cleanup, corrupted pre-existing blob detection, storage-budget exhaustion, schema migration backup and future-schema rejection, single-ref and multi-ref tombstone deletion without silent original pretension, and full backup/restore integrity verification.
