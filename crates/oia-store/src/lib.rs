//! `oia-store` — Canonical evidence-ledger persistence in SQLite WAL mode (#11, ADR 0002).
//!
//! Atomic append-only saves, optimistic revisions, and validated reopen are implemented.
//! Managed blobs, retention, backup, application authorization and jobs remain unfinished.

mod sqlite;

pub use sqlite::{LedgerStore, StoreError, StoreLimits, StoredLedger};

/// Crate boundary marker for `oia-store` (`docs/architecture.md` §1.2, `#9`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreBoundary;
