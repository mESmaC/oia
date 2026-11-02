//! `oia-ingest` — Source registry and bounded ingestion contract boundary stub (`#9`).
//!
//! Full implementation follows in `#13`.

/// Crate boundary marker for `oia-ingest` (`docs/architecture.md` §1.2, `#9`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IngestBoundary;
