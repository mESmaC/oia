//! OIA local CLI entrypoint (`oia`).

use oia_app::{DataModeBadge, WorkspaceShellState, compute_sha256_hex};
use oia_time_geo::{FrameSelectionOutcome, select_frame_in_window};

fn main() {
    let shell = WorkspaceShellState::default();
    let sample_hash = compute_sha256_hex(b"OIA-P0-EVIDENCE-FOUNDATION");
    let frame_check = select_frame_in_window(&[1_000, 2_000, 3_000], 500, 4_000, 2_500, 1_000);
    let selected_ms = match frame_check {
        Ok(FrameSelectionOutcome::Selected(ms)) => ms,
        _ => -1,
    };
    println!(
        "OIA v0.1.0 | mode={} | canvas={} | sound_default={} | frame_select_ms={} | self_check_sha256={}",
        DataModeBadge::Historical.non_color_label(),
        shell.visual.canvas_hex,
        shell.visual.routine_sound_enabled,
        selected_ms,
        &sample_hash[..16]
    );
}
