//! `oia-media` — Original/derivative media separation, forensic vs predictive transform contracts,
//! camera calibration records, and retained clip manifests (`MEDIA-01`–`MEDIA-04`, `FIELD-03`, `A10`, `A13`).

use std::fmt;

use oia_domain::{ContentHash, OpaqueId, OriginClass, VersionId};
use oia_time_geo::GeoPoint;

/// Classification of media transformations keeping conventional forensic processing distinct
/// from predictive super-resolution/inpainting (`MEDIA-01`–`MEDIA-03`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaTransformKind {
    /// Conventional deterministic operation (crop, contrast/histogram stretch, lens undistort, frame extraction).
    ConventionalForensic {
        /// Operation name.
        operation: String,
        /// Exact parameters serialized.
        parameters: String,
    },
    /// Predictive restoration (learned super-resolution, temporal interpolation, denoising/inpainting).
    ///
    /// Predictive detail must remain labeled and cannot be used as an independent observation or physical measurement (`MEDIA-03`).
    PredictiveEnhancement {
        /// Model name and version.
        model_version: String,
        /// Mandatory user-facing warning label.
        disclosure_label: String,
    },
}

impl MediaTransformKind {
    /// Returns the mandatory [`OriginClass`] for derivatives produced by this transform.
    #[must_use]
    pub const fn required_origin_class(&self) -> OriginClass {
        match self {
            Self::ConventionalForensic { .. } => OriginClass::ConventionalDerivative,
            Self::PredictiveEnhancement { .. } => OriginClass::PredictiveDerivative,
        }
    }

    /// Validates that predictive transforms include a non-empty disclosure label.
    pub fn validate(&self) -> Result<(), MediaError> {
        match self {
            Self::ConventionalForensic { operation, .. } => {
                if operation.trim().is_empty() {
                    return Err(MediaError::EmptyOperationName);
                }
                Ok(())
            }
            Self::PredictiveEnhancement {
                model_version,
                disclosure_label,
            } => {
                if model_version.trim().is_empty() || disclosure_label.trim().is_empty() {
                    return Err(MediaError::MissingPredictiveDisclosure);
                }
                Ok(())
            }
        }
    }
}

/// Camera optical and orientation calibration record (`FIELD-03`, `A13`).
#[derive(Debug, Clone, PartialEq)]
pub struct CameraCalibration {
    /// Calibration version ID.
    pub calibration_version: VersionId,
    /// Camera or device ID.
    pub camera_id: OpaqueId,
    /// Calibrated camera position.
    pub position: GeoPoint,
    /// Boresight azimuth in degrees `[0.0, 360.0)`.
    pub azimuth_deg: f64,
    /// Boresight elevation in degrees `[-90.0, 90.0]`.
    pub elevation_deg: f64,
    /// Horizontal field of view in degrees `(0.0, 180.0]`.
    pub horizontal_fov_deg: f64,
    /// RMS angular residual in degrees (`>= 0.0`).
    pub rms_angular_error_deg: f64,
    /// Reference stars/landmarks used.
    pub reference_basis: String,
    /// Whether physical camera movement or PTZ repositioning has invalidated this calibration (`A13`).
    pub invalidated_by_movement: bool,
}

impl CameraCalibration {
    /// Validates angular ranges and ensures an invalidated calibration cannot be used for triangulation (`A13`).
    pub fn validate_for_measurement(&self) -> Result<(), MediaError> {
        if self.invalidated_by_movement {
            return Err(MediaError::CalibrationInvalidatedByMovement(
                self.calibration_version.clone(),
            ));
        }
        if !self.azimuth_deg.is_finite()
            || !self.elevation_deg.is_finite()
            || !self.horizontal_fov_deg.is_finite()
            || self.horizontal_fov_deg <= 0.0
            || self.horizontal_fov_deg > 180.0
            || !self.rms_angular_error_deg.is_finite()
            || self.rms_angular_error_deg < 0.0
        {
            return Err(MediaError::InvalidCalibrationGeometry);
        }
        Ok(())
    }
}

/// Retained video/audio evidence clip manifest (`docs/architecture.md` Section 8, `CAM-04`, `DEVICE-03`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceClipManifest {
    /// Clip identifier.
    pub clip_id: OpaqueId,
    /// SHA-256 content hash of the unaltered original media bytes.
    pub original_blob_hash: ContentHash,
    /// First frame/sample UTC timestamp (ms).
    pub start_utc_ms: i64,
    /// Last frame/sample UTC timestamp (ms).
    pub end_utc_ms: i64,
    /// Count of retained frames/packets.
    pub frame_count: u64,
    /// Count of dropped or missing frames exposed for analysis (never hidden by smooth playback).
    pub dropped_frame_count: u64,
    /// Source session ID.
    pub source_session_id: OpaqueId,
}

/// Typed errors for `oia-media`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaError {
    /// Transform operation name was empty.
    EmptyOperationName,
    /// Predictive enhancement lacked model version or visible disclosure label.
    MissingPredictiveDisclosure,
    /// Attempted to use a camera calibration after camera movement invalidated it (`A13`).
    CalibrationInvalidatedByMovement(VersionId),
    /// Invalid angular parameters in camera calibration.
    InvalidCalibrationGeometry,
}

impl fmt::Display for MediaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyOperationName => {
                write!(f, "media transform operation name must not be empty")
            }
            Self::MissingPredictiveDisclosure => write!(
                f,
                "predictive enhancement requires both model_version and disclosure_label"
            ),
            Self::CalibrationInvalidatedByMovement(v) => {
                write!(
                    f,
                    "camera calibration '{v}' was invalidated by camera movement"
                )
            }
            Self::InvalidCalibrationGeometry => write!(f, "invalid camera calibration geometry"),
        }
    }
}

impl std::error::Error for MediaError {}
