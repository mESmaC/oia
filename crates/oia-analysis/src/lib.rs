//! `oia-analysis` — Analysis boundary with local records-request drafting (`INTEL-04`),
//! multilingual research review (`INTEL-04`), two-view triangulation (`FIELD-03`),
//! inspectable composed reasoning (`REASON-01`..=`REASON-04`), dated text
//! snapshot comparison (`PRIVACY-03`), personal exposure audit (`INTEL-01`),
//! geographic context reconstruction (`GEO-02`, `GEO-05`, `INTEL-03`, `#24`), and
//! satellite imagery / space observation catalog (`SPACE-01`..=`SPACE-03`, `A05`, `#26`),
//! statement comparison with copied-text and drift detection (`INTEL-01`, `REASON-03`,
//! `FIELD-02`, `#138`), and
//! validation/simulation lab with operational benchmarks (`FIELD-04`, `CAM-05`, `MEDIA-02`,
//! `DEVICE-04`, `#40`) kernels.
//!
//! Service integration, durable persistence, and remaining P2/P4 workflows follow in `#14`,
//! `#11`, `#24`, `#26`, `#35`–`#37`, `#40`, and `#48`.

pub mod exposure_audit;
pub mod geo_context;
pub mod multilingual;
pub mod reasoning;
pub mod records_request;
pub mod snapshot_change;
pub mod space_imagery;
pub mod statement_comparison;
pub mod triangulation;
pub mod validation_lab;

/// Crate boundary marker for `oia-analysis` (`docs/architecture.md` §1.2, `#9`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalysisBoundary;
