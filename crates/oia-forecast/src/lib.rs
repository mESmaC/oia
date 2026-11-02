//! Forecast evaluation kernels for Open Intelligence Agency.
//!
//! Binary scoring is implemented; issuance, point-in-time validation, persistence,
//! calibration and application integration remain open in `#43`.

pub mod scoring;

/// Crate boundary marker for `oia-forecast` (`docs/architecture.md` §1.2, `#9`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForecastBoundary;
