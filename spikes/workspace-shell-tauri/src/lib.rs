//! OIA workspace shell spike: a Tauri 2 window over a Rust-held shell context.
//!
//! Evaluation artefact for the renderer part of decision D01 (see
//! `docs/adr/0008-d01-workspace-renderer.md`). Fixture data only. The pure
//! modules (`shell`, `jobs`, `persist`, `fixture`) come from `oia-shell-core`
//! and have no window dependency; `commands` is the only Tauri-aware module.

pub mod commands;
pub use oia_shell_core::{fixture, jobs, persist, shell};

use std::sync::OnceLock;
use std::time::Instant;

/// Process start, for the startup measurement reported by `ui_ready`.
pub static PROCESS_START: OnceLock<Instant> = OnceLock::new();

/// Builds and runs the window. Exits with status 1 on a runtime error.
pub fn run() {
    let _ = PROCESS_START.set(Instant::now());
    let result = tauri::Builder::default()
        .setup(|app| commands::setup(app.handle().clone()))
        .invoke_handler(tauri::generate_handler![
            commands::shell_state,
            commands::case_home,
            commands::case_geometry,
            commands::object_detail,
            commands::documents,
            commands::report_draft,
            commands::export_preview,
            commands::timeline_marks,
            commands::list_jobs,
            commands::switch_instrument,
            commands::select_object,
            commands::clear_selection,
            commands::route_back,
            commands::set_local_filter,
            commands::promote_local_filter,
            commands::scrub_clock,
            commands::set_clock,
            commands::set_setting,
            commands::start_search,
            commands::cancel_job,
            commands::retry_job,
            commands::open_job_result,
            commands::ui_ready,
        ])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("oia-shell-spike: {e}");
        std::process::exit(1);
    }
}
