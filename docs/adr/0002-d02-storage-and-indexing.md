# ADR 0002: Transactional Evidence Storage, Managed Blobs, and Indexing (D02)

- **Date:** 2026-11-01
- **Status:** Accepted for P0 foundation; external SQL/vector accelerator backends remain replaceable
- **Decision ID:** D02
- **Issues:** #9, #11, #14
- **Requirements:** `CASE-03`, `CASE-05`, `REPRO-01`, `REPRO-02`

## Context

`D02` requires transactional local persistence for cases, versioned evidence records, permissions, and durable jobs alongside content-addressed managed blobs, schema migrations, reference-aware retention, tombstones, and backup/restore recovery.

## Decision

1. **Accepted for P0 (Transactional Local Store + Content-Addressed Blobs):**
   - `oia-store` implements a transactional local repository (`EvidenceStore`) backed by:
     - An atomic write-ahead journaled state store (`store.json` + `store.wal` + `schema_migrations.json`) that validates schema versions on open, applies forward migrations atomically after creating a pre-migration backup snapshot, and rejects unknown future schema versions with a typed `StoreError::UnsupportedSchemaVersion`.
     - A content-addressed blob directory (`blobs/sha256/<first-2-hex>/<sha256-hex>`) paired with a staging directory (`staging/`). Byte payloads are hashed with FIPS 180-4 SHA-256, written to `staging/` with size-cap enforcement, and promoted into `blobs/` only upon transaction commit. Rolled-back or interrupted transactions clean up staged blobs without leaving orphaned partial files or mutating existing cases.
     - Reference-aware retention across four explicit classes (`RoutineBuffer`, `TemporaryExtractionCache`, `PinnedCaseEvidence`, `IssuedReportSnapshot`) and explicit deletion via `TombstoneRecord` that removes underlying blob bytes while preserving permitted audit metadata, marking references `AvailabilityState::Removed`, and flagging dependent analyses/reports `NeedsReassessment`.
     - Deterministic inverted token, identifier, temporal-interval, and spatial-bounding-box indexes built in pure Rust with workspace/role permission filtering applied before returning hits or snippets.

2. **Open Proposal for Later Scale Gates (P1/P4):**
   - Once the P1/P4 10,000-document / 100,000-passage reference corpus benchmark is evaluated, an embedded SQLite (`rusqlite` with FTS5/R-Tree) and rebuildable vector-embedding index adapter can be plugged behind `oia-store`'s transactional store contract without altering `oia-domain` or `oia-app`.

## Alternatives Considered

- **Unvalidated ad-hoc file writes without staging or WAL:** Rejected because interrupted imports or crashes would leave partial cases or unreferenced blobs, violating `A01`, `A17`, and the recovery gate.
- **Mandating an external database server in P0:** Rejected because OIA is local-first and must operate offline without external infrastructure dependencies.

## Validation

- Verified via deterministic tests in `crates/oia-store` covering atomic source/artifact commits, simulated crash/interruption rollback and staged-blob cleanup, storage-budget exhaustion, schema migration backup and future-schema rejection, tombstone deletion without silent original pretension, and full backup/restore integrity verification.
