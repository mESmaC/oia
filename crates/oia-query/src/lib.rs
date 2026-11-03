//! `oia-query` — Permission-filtered search and reversible event clustering with
//! lineage-adjusted distinct evidence counts (`#9`, `#23`).
//!
//! Full service and persistence integration follows in `#14`, `#18`, and `#23`.

pub mod geo_workspace;

/// Crate boundary marker for `oia-query` (`docs/architecture.md` §1.2, `#9`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryBoundary;
