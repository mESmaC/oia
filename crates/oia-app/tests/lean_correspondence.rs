//! Full workspace Rust-to-Lean correspondence fixture suite (`#15`, ADR 0004).
//!
//! Verifies that `oia_time_geo::select_frame_in_window`, `oia_forecast::is_admissible_at_cutoff`,
//! and `oia_analysis::compute_invalidated_nodes` produce identical outputs to the `#guard`
//! assertions in `proofs/OiaProofs/Fixtures.lean`.

use oia_analysis::compute_invalidated_nodes;
use oia_forecast::{CutoffMode, is_admissible_at_cutoff};
use oia_time_geo::{FrameSelectionOutcome, TimeGeoError, select_frame_in_window};

#[test]
fn lean_correspondence_all_targets_match_fixtures() -> Result<(), TimeGeoError> {
    assert_eq!(
        select_frame_in_window(&[], 1_000, 5_000, 3_000, 600)?,
        FrameSelectionOutcome::NoFramesAvailable
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

    assert!(!is_admissible_at_cutoff(
        CutoffMode::StrictProspective,
        1_000,
        None,
        500
    ));
    assert!(is_admissible_at_cutoff(
        CutoffMode::StrictProspective,
        1_000,
        Some(900),
        1_000
    ));
    assert!(!is_admissible_at_cutoff(
        CutoffMode::StrictProspective,
        1_000,
        Some(900),
        1_001
    ));
    assert!(is_admissible_at_cutoff(
        CutoffMode::RetrospectiveReconstruction,
        1_000,
        Some(900),
        1_500
    ));
    assert!(!is_admissible_at_cutoff(
        CutoffMode::RetrospectiveReconstruction,
        1_000,
        Some(1_001),
        500
    ));

    let edges = [(2, 1), (3, 2), (4, 1)];
    assert_eq!(compute_invalidated_nodes(&edges, 1), vec![1, 2, 3, 4]);

    Ok(())
}
