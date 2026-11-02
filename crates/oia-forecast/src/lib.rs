//! Forecast issuance, cutoff admissibility, and evaluation kernels for Open Intelligence Agency.
//!
//! Binary scoring (`scoring`) and immutable issuance/revision/resolution lifecycle (`issuance`)
//! are implemented; durable storage, calibration fitting, `D12` dataset selection, and application
//! service integration remain open in `#43`.

pub mod issuance;
pub mod reliability;
pub mod scoring;

/// Crate boundary marker for `oia-forecast` (`docs/architecture.md` §1.2, `#9`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForecastBoundary;
