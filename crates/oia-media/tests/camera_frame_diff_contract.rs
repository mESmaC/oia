//! Focused numerical and malformed-input checks for raw-luma differencing.
//!
//! Simulated buffers establish arithmetic, padding exclusion, and resource-bound
//! behavior only; they do not establish anomaly-detection quality or A09 results.

use oia_media::camera_frame_diff::{
    FrameRole, RawLumaDiff, RawLumaDiffConfig, RawLumaDiffError, RawLumaFrame, diff_raw_luma,
};

fn config(max_pixels: u64, minimum_changed_delta: u8) -> RawLumaDiffConfig {
    RawLumaDiffConfig {
        max_pixels,
        minimum_changed_delta,
    }
}

#[test]
fn computes_exact_integer_deltas_and_excludes_row_padding() {
    let previous = RawLumaFrame {
        width: 3,
        height: 2,
        row_stride_bytes: 5,
        samples: &[10, 20, 30, 200, 201, 10, 40, 60, 202, 203],
    };
    let current = RawLumaFrame {
        samples: &[10, 22, 35, 100, 101, 50, 40, 10, 102, 103],
        ..previous
    };

    // Visible deltas are 0, 2, 5, 40, 0, and 50. The large padding deltas
    // are excluded. Sum = 97; changed pixels at threshold 3 = 5, 40, and 50.
    assert_eq!(
        diff_raw_luma(previous, current, config(6, 3)),
        Ok(RawLumaDiff {
            compared_pixels: 6,
            changed_pixels: 3,
            total_absolute_delta: 97,
            maximum_delta: 50,
            minimum_changed_delta: 3,
        })
    );
    assert_eq!(
        diff_raw_luma(previous, previous, config(6, 1)),
        Ok(RawLumaDiff {
            compared_pixels: 6,
            changed_pixels: 0,
            total_absolute_delta: 0,
            maximum_delta: 0,
            minimum_changed_delta: 1,
        })
    );
}

#[test]
fn refuses_invalid_dimensions_buffers_and_budgets() {
    let previous = RawLumaFrame {
        width: 3,
        height: 2,
        row_stride_bytes: 5,
        samples: &[10, 20, 30, 200, 201, 10, 40, 60, 202, 203],
    };
    let current = RawLumaFrame {
        samples: &[10, 22, 35, 100, 101, 50, 40, 10, 102, 103],
        ..previous
    };

    assert_eq!(
        diff_raw_luma(
            RawLumaFrame {
                width: 0,
                ..previous
            },
            current,
            config(6, 1),
        ),
        Err(RawLumaDiffError::EmptyDimensions {
            frame: FrameRole::Previous,
        })
    );
    assert_eq!(
        diff_raw_luma(
            previous,
            RawLumaFrame {
                width: 2,
                ..current
            },
            config(6, 1),
        ),
        Err(RawLumaDiffError::DimensionMismatch {
            previous_width: 3,
            previous_height: 2,
            current_width: 2,
            current_height: 2,
        })
    );
    assert_eq!(
        diff_raw_luma(previous, current, config(0, 1)),
        Err(RawLumaDiffError::ZeroPixelBudget)
    );
    assert_eq!(
        diff_raw_luma(previous, current, config(6, 0)),
        Err(RawLumaDiffError::ZeroMinimumChangedDelta)
    );
    assert_eq!(
        diff_raw_luma(previous, current, config(5, 1)),
        Err(RawLumaDiffError::PixelBudgetExceeded {
            requested_pixels: 6,
            max_pixels: 5,
        })
    );

    assert_eq!(
        diff_raw_luma(
            RawLumaFrame {
                row_stride_bytes: 2,
                ..previous
            },
            current,
            config(6, 1),
        ),
        Err(RawLumaDiffError::RowStrideTooSmall {
            frame: FrameRole::Previous,
            row_stride_bytes: 2,
            minimum_row_stride_bytes: 3,
        })
    );
    assert_eq!(
        diff_raw_luma(
            RawLumaFrame {
                samples: &[0; 9],
                ..previous
            },
            current,
            config(6, 1),
        ),
        Err(RawLumaDiffError::BufferLengthMismatch {
            frame: FrameRole::Previous,
            expected_bytes: 10,
            actual_bytes: 9,
        })
    );
    assert_eq!(
        diff_raw_luma(
            previous,
            RawLumaFrame {
                samples: &[0; 9],
                ..current
            },
            config(6, 1),
        ),
        Err(RawLumaDiffError::BufferLengthMismatch {
            frame: FrameRole::Current,
            expected_bytes: 10,
            actual_bytes: 9,
        })
    );

    let overflowing = RawLumaFrame {
        width: 1,
        height: 2,
        row_stride_bytes: usize::MAX,
        samples: &[],
    };
    let small = RawLumaFrame {
        width: 1,
        height: 2,
        row_stride_bytes: 1,
        samples: &[0, 0],
    };
    assert_eq!(
        diff_raw_luma(overflowing, small, config(2, 1)),
        Err(RawLumaDiffError::FrameLayoutOverflow {
            frame: FrameRole::Previous,
        })
    );
}
