//! Forecast issuance, cutoff admissibility, historical analogue precedent retrieval,
//! market-effects scenario forecasting, and evaluation kernels for Open Intelligence Agency.
//!
//! Binary scoring (`scoring`), reliability (`reliability`), scalar intervals (`interval`),
//! immutable issuance/revision/resolution lifecycle (`issuance`), historical analogue
//! forecasting (`analogues`, `#45`), and market-effects forecasting (`market_effects`, `#44`)
//! are implemented; durable storage, calibration fitting, `D12` dataset selection, and
//! application service integration remain open in `#43`–`#45`.

pub mod analogues;
pub mod interval;
pub mod issuance;
pub mod market_effects;
pub mod reliability;
pub mod scoring;

/// Crate boundary marker for `oia-forecast` (`docs/architecture.md` §1.2, `#9`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForecastBoundary;
