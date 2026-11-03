//! `oia-media` — Media forensics, camera anomaly review, and illustrative recreation-prompt kernels (`#9`, `#29`, `#31`).
//!
//! - [`camera_anomaly`]: camera-health vs. scene-anomaly separation, deduplicated candidate packaging,
//!   append-only review history, cross-camera association checks, and operational held-out evaluation (`CAM-01`–`CAM-05`, `A09`).
//! - [`camera_frame_diff`]: bounded diagnostic measurements between decoded 8-bit luma frames
//!   (`CAM-03`); does not classify anomalies or camera faults.
//! - [`recreation`]: portable recreation-prompt export and imported-recreation labeling (`MEDIA-04`, `A10`).
//!
//! The separate `oia-image-forensics` codec worker implements bounded offline still-image
//! diagnostics; its integration here and the full forensic workbench (`#30`) remain open,
//! as does predictive restoration (`MEDIA-03` in `#31`).

#![forbid(unsafe_code)]
#![deny(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

pub mod camera_anomaly;
pub mod camera_frame_diff;
pub mod frame_timing;
pub mod recreation;

/// Crate boundary marker for `oia-media` (`docs/architecture.md` §1.2, `#9`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaBoundary;

pub(crate) fn is_valid_lowercase_sha256_hex(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
