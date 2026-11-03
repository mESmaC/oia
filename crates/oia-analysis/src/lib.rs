//! `oia-analysis` — Analysis boundary with local records-request drafting (`INTEL-04`),
//! multilingual research review (`INTEL-04`), two-view triangulation (`FIELD-03`), and
//! inspectable composed reasoning (`REASON-01`..=`REASON-04`) kernels.
//!
//! Service integration, durable persistence, and remaining P4 workflows follow in `#14`,
//! `#11`, and `#35`–`#37`.

pub mod multilingual;
pub mod reasoning;
pub mod records_request;
pub mod triangulation;

/// Crate boundary marker for `oia-analysis` (`docs/architecture.md` §1.2, `#9`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalysisBoundary;
