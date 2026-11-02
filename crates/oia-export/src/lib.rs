//! `oia-export` — Cited report export and reproducible bundle import/export boundary stub (`#9`).
//!
//! Full implementation follows in `#14` and `#22` per `ADR 0006` (`D05`).

/// Crate boundary marker for `oia-export` (`docs/architecture.md` §1.2, `#9`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportBoundary;
