//! Shared bounded public-source collection (`#13`, `#166`).
//! Durable ingestion, scheduled jobs and the remaining capability matrix stay on #13/#14.

pub mod collection;
mod parsing;
pub mod transport;

/// Crate boundary marker for `oia-ingest` (`docs/architecture.md` §1.2, `#9`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IngestBoundary;
