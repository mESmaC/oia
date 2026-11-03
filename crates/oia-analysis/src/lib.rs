//! `oia-analysis` — Analysis boundary with local records-request drafting (`INTEL-04`),
//! multilingual research review (`INTEL-04`), two-view triangulation (`FIELD-03`), and
//! inspectable composed reasoning (`REASON-01`..=`REASON-04`), and dated text
//! snapshot comparison (`PRIVACY-03`) kernels.
//!
//! Service integration, durable persistence, and remaining P4 workflows follow in `#14`,
//! `#11`, `#35`–`#37`, and `#48`.

pub mod exposure_audit;
pub mod multilingual;
pub mod reasoning;
pub mod records_request;
pub mod snapshot_change;
pub mod triangulation;

/// Crate boundary marker for `oia-analysis` (`docs/architecture.md` §1.2, `#9`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalysisBoundary;
