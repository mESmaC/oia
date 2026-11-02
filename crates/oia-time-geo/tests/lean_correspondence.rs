//! Rust-to-Lean correspondence fixture suite for `oia-time-geo` (`#15`, ADR 0004).
//!
//! Executes the exact same frame-selection boundary fixture vectors checked by `#guard` in
//! `proofs/OiaProofs/Fixtures.lean` against `oia_time_geo::select_frame_in_window`.

use oia_time_geo::{FrameSelectionOutcome, TimeGeoError, select_frame_in_window};

#[test]
fn lean_frame_selection_correspondence_fixtures() -> Result<(), TimeGeoError> {
    // Fixture 1: Empty catalog returns `NoFramesAvailable` (matches `OiaProofs/Fixtures.lean`).
    assert_eq!(
        select_frame_in_window(&[], 1_000, 5_000, 3_000, 600)?,
        FrameSelectionOutcome::NoFramesAvailable
    );

    // Fixture 2: Candidates exist, but all are outside `[target - maxGap, target]` -> `CoverageGap`.
    assert_eq!(
        select_frame_in_window(&[1_500, 2_399, 3_001, 4_500], 1_000, 5_000, 3_000, 600)?,
        FrameSelectionOutcome::CoverageGap
    );

    // Fixture 3: Half-open window `[1000, 3000)` includes `1000` and excludes `3000 == windowEnd`;
    // unsorted candidates with duplicates select maximum valid frame `3000` when `windowEnd = 5000`.
    assert_eq!(
        select_frame_in_window(&[1_000, 3_000], 1_000, 3_000, 3_000, 2_000)?,
        FrameSelectionOutcome::Selected(1_000)
    );
    assert_eq!(
        select_frame_in_window(
            &[2_400, 2_950, 2_700, 3_000, 3_001],
            1_000,
            5_000,
            3_000,
            600
        )?,
        FrameSelectionOutcome::Selected(3_000)
    );

    Ok(())
}
