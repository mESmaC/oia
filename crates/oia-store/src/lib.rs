//! `oia-store` — Transactional evidence storage, managed blobs, and retention boundary stub (`#9`).
//!
//! Embedded SQLite (`rusqlite` in WAL mode) and managed blob implementation follows in `#11`
//! per `ADR 0002` (`D02`).

/// Crate boundary marker for `oia-store` (`docs/architecture.md` §1.2, `#9`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreBoundary;
