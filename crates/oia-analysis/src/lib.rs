//! `oia-analysis` — Analysis boundary with local records-request drafting and
//! inspectable composed reasoning kernels (`REASON-01`..=`REASON-04`, `INTEL-04`).
//!
//! Service integration and durable persistence follow in `#14` and `#11`.

pub mod reasoning;
pub mod records_request;

/// Crate boundary marker for `oia-analysis` (`docs/architecture.md` §1.2, `#9`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalysisBoundary;
