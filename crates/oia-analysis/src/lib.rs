//! `oia-analysis` — Composed analysis, reasoning, and lineage boundary stub (`#9`).
//!
//! Full implementation follows in `#14` and P4 (`#35`–`#37`).

/// Crate boundary marker for `oia-analysis` (`docs/architecture.md` §1.2, `#9`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalysisBoundary;
