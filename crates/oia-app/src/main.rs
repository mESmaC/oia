//! OIA local workspace bootstrap CLI (`oia`, `#9`).

use oia_app::{DataModeBadge, WorkspaceShellState};

fn main() {
    let shell = WorkspaceShellState::default();
    println!(
        "OIA v0.1.0 | view={:?} | mode={} | canvas={} | sound_default={}",
        shell.active_view,
        DataModeBadge::Historical.non_color_label(),
        shell.visual.canvas_hex,
        shell.visual.routine_sound_enabled,
    );
}
