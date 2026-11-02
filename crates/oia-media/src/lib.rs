//! `oia-media` — Media forensics, camera anomaly review, and illustrative recreation-prompt kernels (`#9`, `#29`, `#31`).
//!
//! - [`camera_anomaly`]: camera-health vs. scene-anomaly separation, deduplicated candidate packaging,
//!   append-only review history, cross-camera association checks, and operational held-out evaluation (`CAM-01`–`CAM-05`, `A09`).
//! - [`recreation`]: portable recreation-prompt export and imported-recreation labeling (`MEDIA-04`, `A10`).
//!
//! Forensic workbench inspection (`#30`) and predictive restoration (`MEDIA-03` in `#31`) remain open.

#![forbid(unsafe_code)]
#![deny(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

pub mod camera_anomaly;
pub mod recreation;

/// Crate boundary marker for `oia-media` (`docs/architecture.md` §1.2, `#9`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaBoundary;
