//! `oia-analysis` — Analysis boundary with local records-request drafting (`INTEL-04`),
//! multilingual research review (`INTEL-04`), two-view triangulation (`FIELD-03`),
//! inspectable composed reasoning (`REASON-01`..=`REASON-04`), dated text
//! snapshot comparison (`PRIVACY-03`), and geographic context reconstruction
//! (`GEO-02`, `GEO-05`, `INTEL-03`, `#24`) kernels.
//!
//! Service integration, durable persistence, and remaining P2/P4 workflows follow in `#14`,
//! `#11`, `#24`, `#35`–`#37`, and `#48`.

pub mod geo_context;
pub mod multilingual;
pub mod reasoning;
pub mod records_request;
pub mod snapshot_change;
pub mod triangulation;

/// Crate boundary marker for `oia-analysis` (`docs/architecture.md` §1.2, `#9`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalysisBoundary;
