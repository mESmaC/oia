//! `oia-analysis` — Analysis boundary with a local records-request drafting kernel.
//!
//! Service integration and composed reasoning follow in `#14` and P4 (`#35`–`#37`).

pub mod multilingual;
pub mod records_request;
pub mod triangulation;

/// Crate boundary marker for `oia-analysis` (`docs/architecture.md` §1.2, `#9`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalysisBoundary;
