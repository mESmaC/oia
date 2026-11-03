//! Bounded, deterministic difference measurements for decoded 8-bit luma frames.
//!
//! This is a diagnostic primitive for the `CAM-03` differencing subset. It does
//! not classify scene anomalies or camera faults, and its measurements are not
//! calibrated probabilities. Source admission, original-frame retention,
//! timestamps, health-fault separation, and review remain owned by
//! [`crate::camera_anomaly`].

use std::fmt;

/// Disclosure applying to every raw-luma difference measurement (`CAM-03`, `CAM-05`).
pub const RAW_LUMA_DIFF_CAVEAT: &str = "Raw luma change is a diagnostic measurement, not a scene anomaly, camera-health classification, or calibrated probability.";

/// A borrowed, already-decoded 8-bit single-channel frame with explicit row stride.
///
/// `samples.len()` must equal `row_stride_bytes * height`. Each row's first
/// `width` bytes are samples; all remaining stride bytes are padding and are not
/// compared. No color conversion, resizing, alignment, or subsampling occurs.
#[derive(Debug, Clone, Copy)]
pub struct RawLumaFrame<'a> {
    pub width: u32,
    pub height: u32,
    pub row_stride_bytes: usize,
    pub samples: &'a [u8],
}

/// Explicit resource and change-counting bounds for [`diff_raw_luma`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawLumaDiffConfig {
    /// Maximum number of visible pixels to compare. Must be at least one.
    pub max_pixels: u64,
    /// Minimum absolute sample delta counted as changed. Must be in `1..=255`.
    pub minimum_changed_delta: u8,
}

/// Exact integer measurements from one pair of raw-luma frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawLumaDiff {
    /// Number of visible pixels compared, excluding row padding.
    pub compared_pixels: u64,
    /// Pixels whose absolute delta is at least `minimum_changed_delta`.
    pub changed_pixels: u64,
    /// Exact numerator of the mean absolute sample delta.
    ///
    /// The exact mean is `total_absolute_delta / compared_pixels`; the exact
    /// changed fraction is `changed_pixels / compared_pixels`. No truncated
    /// ratio is stored.
    pub total_absolute_delta: u128,
    /// Largest absolute sample delta across all compared pixels.
    pub maximum_delta: u8,
    /// The exact change-counting setting used for this result.
    pub minimum_changed_delta: u8,
}

/// Identifies the input frame rejected by a validation error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameRole {
    Previous,
    Current,
}

impl fmt::Display for FrameRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Previous => f.write_str("previous"),
            Self::Current => f.write_str("current"),
        }
    }
}

/// Typed validation failures returned before any pixels are compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawLumaDiffError {
    EmptyDimensions {
        frame: FrameRole,
    },
    DimensionMismatch {
        previous_width: u32,
        previous_height: u32,
        current_width: u32,
        current_height: u32,
    },
    ZeroPixelBudget,
    PixelBudgetExceeded {
        requested_pixels: u64,
        max_pixels: u64,
    },
    ZeroMinimumChangedDelta,
    RowStrideTooSmall {
        frame: FrameRole,
        row_stride_bytes: usize,
        minimum_row_stride_bytes: usize,
    },
    FrameLayoutOverflow {
        frame: FrameRole,
    },
    BufferLengthMismatch {
        frame: FrameRole,
        expected_bytes: usize,
        actual_bytes: usize,
    },
}

impl fmt::Display for RawLumaDiffError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyDimensions { frame } => {
                write!(f, "{frame} frame width and height must both be nonzero")
            }
            Self::DimensionMismatch {
                previous_width,
                previous_height,
                current_width,
                current_height,
            } => write!(
                f,
                "frame dimensions differ: previous is {previous_width}x{previous_height}, current is {current_width}x{current_height}"
            ),
            Self::ZeroPixelBudget => f.write_str("max_pixels must be at least one"),
            Self::PixelBudgetExceeded {
                requested_pixels,
                max_pixels,
            } => write!(
                f,
                "frame contains {requested_pixels} pixels, exceeding max_pixels={max_pixels}"
            ),
            Self::ZeroMinimumChangedDelta => {
                f.write_str("minimum_changed_delta must be at least one")
            }
            Self::RowStrideTooSmall {
                frame,
                row_stride_bytes,
                minimum_row_stride_bytes,
            } => write!(
                f,
                "{frame} row stride {row_stride_bytes} is smaller than width {minimum_row_stride_bytes}"
            ),
            Self::FrameLayoutOverflow { frame } => {
                write!(f, "{frame} frame buffer layout exceeds addressable size")
            }
            Self::BufferLengthMismatch {
                frame,
                expected_bytes,
                actual_bytes,
            } => write!(
                f,
                "{frame} buffer has {actual_bytes} bytes; expected exactly {expected_bytes} from stride times height"
            ),
        }
    }
}

impl std::error::Error for RawLumaDiffError {}

fn validate_layout(frame: RawLumaFrame<'_>, role: FrameRole) -> Result<(), RawLumaDiffError> {
    let width = usize::try_from(frame.width)
        .map_err(|_| RawLumaDiffError::FrameLayoutOverflow { frame: role })?;
    let height = usize::try_from(frame.height)
        .map_err(|_| RawLumaDiffError::FrameLayoutOverflow { frame: role })?;
    if frame.row_stride_bytes < width {
        return Err(RawLumaDiffError::RowStrideTooSmall {
            frame: role,
            row_stride_bytes: frame.row_stride_bytes,
            minimum_row_stride_bytes: width,
        });
    }
    let expected_bytes = frame
        .row_stride_bytes
        .checked_mul(height)
        .ok_or(RawLumaDiffError::FrameLayoutOverflow { frame: role })?;
    if frame.samples.len() != expected_bytes {
        return Err(RawLumaDiffError::BufferLengthMismatch {
            frame: role,
            expected_bytes,
            actual_bytes: frame.samples.len(),
        });
    }
    Ok(())
}

/// Compares two equally sized raw-luma frames after validating both complete layouts.
///
/// Every visible pixel contributes to the mean and maximum. A pixel contributes
/// to `changed_pixels` when its absolute delta is greater than or equal to
/// `minimum_changed_delta`. Arithmetic uses wide integer accumulators; results
/// are deterministic and no allocation or implicit sampling is performed.
///
/// Zero changed pixels does not establish a frozen frame: a genuinely static
/// scene can be unchanged. A large or global difference does not establish an
/// exposure shift, camera movement, or scene anomaly. See [`RAW_LUMA_DIFF_CAVEAT`].
pub fn diff_raw_luma(
    previous: RawLumaFrame<'_>,
    current: RawLumaFrame<'_>,
    config: RawLumaDiffConfig,
) -> Result<RawLumaDiff, RawLumaDiffError> {
    if previous.width == 0 || previous.height == 0 {
        return Err(RawLumaDiffError::EmptyDimensions {
            frame: FrameRole::Previous,
        });
    }
    if current.width == 0 || current.height == 0 {
        return Err(RawLumaDiffError::EmptyDimensions {
            frame: FrameRole::Current,
        });
    }
    if previous.width != current.width || previous.height != current.height {
        return Err(RawLumaDiffError::DimensionMismatch {
            previous_width: previous.width,
            previous_height: previous.height,
            current_width: current.width,
            current_height: current.height,
        });
    }
    if config.max_pixels == 0 {
        return Err(RawLumaDiffError::ZeroPixelBudget);
    }
    if config.minimum_changed_delta == 0 {
        return Err(RawLumaDiffError::ZeroMinimumChangedDelta);
    }
    let compared_pixels = u64::from(previous.width) * u64::from(previous.height);
    if compared_pixels > config.max_pixels {
        return Err(RawLumaDiffError::PixelBudgetExceeded {
            requested_pixels: compared_pixels,
            max_pixels: config.max_pixels,
        });
    }
    validate_layout(previous, FrameRole::Previous)?;
    validate_layout(current, FrameRole::Current)?;

    let width =
        usize::try_from(previous.width).map_err(|_| RawLumaDiffError::FrameLayoutOverflow {
            frame: FrameRole::Previous,
        })?;
    let previous_rows = previous.samples.chunks_exact(previous.row_stride_bytes);
    let current_rows = current.samples.chunks_exact(current.row_stride_bytes);
    let mut changed_pixels = 0_u64;
    let mut total_absolute_delta = 0_u128;
    let mut maximum_delta = 0_u8;

    for (previous_row, current_row) in previous_rows.zip(current_rows) {
        for (previous_sample, current_sample) in previous_row
            .iter()
            .take(width)
            .zip(current_row.iter().take(width))
        {
            let delta = previous_sample.abs_diff(*current_sample);
            total_absolute_delta += u128::from(delta);
            maximum_delta = maximum_delta.max(delta);
            if delta >= config.minimum_changed_delta {
                changed_pixels += 1;
            }
        }
    }

    Ok(RawLumaDiff {
        compared_pixels,
        changed_pixels,
        total_absolute_delta,
        maximum_delta,
        minimum_changed_delta: config.minimum_changed_delta,
    })
}
