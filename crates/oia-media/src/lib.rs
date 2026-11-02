//! `oia-media` — Media forensics, enhancement, and illustrative recreation-prompt kernels (`#9`, `#31`).
//!
//! Portable recreation-prompt export and imported-recreation labeling (`MEDIA-04`) live in
//! [`recreation`]; forensic workbench inspection (`#30`) and predictive restoration (`MEDIA-03`
//! in `#31`) remain open.

#![forbid(unsafe_code)]
#![deny(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

pub mod recreation;

/// Crate boundary marker for `oia-media` (`docs/architecture.md` §1.2, `#9`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaBoundary;
