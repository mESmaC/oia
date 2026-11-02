//! `oia-models` — Model gateway, prompt sanitation, and grounding boundary stub (`#9`).
//!
//! Full implementation follows in `#14` and `#18`.

/// Crate boundary marker for `oia-models` (`docs/architecture.md` §1.2, `#9`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelsBoundary;
