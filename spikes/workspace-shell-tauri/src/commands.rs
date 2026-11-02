//! Tauri command layer: the only place that knows about the window runtime.
//!
//! Queries return data; commands mutate the Rust-held shell state and return
//! the new shell view. Every response is wrapped in an envelope carrying the
//! transport schema, the fixture flag, an outcome, and a coverage line.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::fixture::{
    self, CaseGeometry, CaseHome, DocumentRow, ExportPreview, ObjectDetail, ReportDraft, TimeMark,
};
use crate::jobs::{Finish, Job, JobKind, JobRegistry, JobState, Progress};
use crate::persist::{self, Persisted, RestoreReport};
use crate::shell::{Instrument, InstrumentInfo, Selection, SettingChange, ShellState};

/// Transport schema identifier carried on every envelope.
pub const SCHEMA: &str = "oia-shell-spike/1";

/// Event emitted whenever a job changes outside a command the UI issued.
pub const JOBS_CHANGED: &str = "jobs-changed";

const STATE_FILE: &str = "shell-state.json";
const STARTUP_LOG: &str = "startup-log.jsonl";

/// Fixture worker latencies. These simulate waiting, nothing else, and are
/// labelled as fixture behaviour in the tray.
const SEARCH_LATENCY: Duration = Duration::from_millis(1200);
const OCR_PAGE_INTERVAL: Duration = Duration::from_millis(2500);
const ACQUIRE_QUEUE_DELAY: Duration = Duration::from_millis(400);
const ACQUIRE_FAIL_DELAY: Duration = Duration::from_millis(1200);

pub struct Inner {
    pub shell: ShellState,
    pub jobs: JobRegistry,
    pub store_path: Option<PathBuf>,
    pub restore: RestoreReport,
    pub last_persist_error: Option<String>,
}

impl Inner {
    /// Persists after a mutation. A failure is recorded and shown, not hidden.
    pub fn persist(&mut self) {
        let Some(path) = self.store_path.clone() else {
            return;
        };
        match persist::save(
            &path,
            &Persisted::new(self.shell.clone(), self.jobs.clone()),
        ) {
            Ok(()) => self.last_persist_error = None,
            Err(e) => self.last_persist_error = Some(e.to_string()),
        }
    }
}

pub struct AppState {
    inner: Mutex<Inner>,
}

impl AppState {
    #[must_use]
    pub fn new(inner: Inner) -> Self {
        Self {
            inner: Mutex::new(inner),
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, Inner>, String> {
        self.inner
            .lock()
            .map_err(|_| "shell state lock poisoned".to_owned())
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Success,
    Partial,
    Empty,
    Unavailable,
}

#[derive(Debug, Clone, Serialize)]
pub struct Envelope<T: Serialize> {
    pub schema: &'static str,
    pub fixture: bool,
    pub outcome: Outcome,
    pub coverage: Option<String>,
    pub data: T,
}

fn ok<T: Serialize>(data: T) -> Envelope<T> {
    Envelope {
        schema: SCHEMA,
        fixture: true,
        outcome: Outcome::Success,
        coverage: None,
        data,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ShellView {
    pub shell: ShellState,
    pub instruments: Vec<InstrumentInfo>,
    pub divergent_local_filters: Vec<Instrument>,
    pub restore: RestoreReport,
    pub persist_error: Option<String>,
    pub store_path: Option<String>,
}

fn view(inner: &Inner) -> ShellView {
    ShellView {
        shell: inner.shell.clone(),
        instruments: InstrumentInfo::all(),
        divergent_local_filters: inner.shell.divergent_local_filters(),
        restore: inner.restore.clone(),
        persist_error: inner.last_persist_error.clone(),
        store_path: inner.store_path.as_ref().map(|p| p.display().to_string()),
    }
}

fn mutate<F>(state: &State<'_, AppState>, f: F) -> Result<Envelope<ShellView>, String>
where
    F: FnOnce(&mut Inner) -> Result<(), String>,
{
    let mut inner = state.lock()?;
    f(&mut inner)?;
    inner.persist();
    Ok(ok(view(&inner)))
}

// ── Queries ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn shell_state(state: State<'_, AppState>) -> Result<Envelope<ShellView>, String> {
    let inner = state.lock()?;
    Ok(ok(view(&inner)))
}

#[tauri::command]
pub fn case_home() -> Envelope<CaseHome> {
    ok(fixture::case_home())
}

#[tauri::command]
pub fn case_geometry() -> Envelope<CaseGeometry> {
    let mut env = ok(fixture::case_geometry());
    env.coverage = Some("Reported geometry only. No basemap, imagery, or terrain.".to_owned());
    env
}

#[tauri::command]
pub fn object_detail(object_id: String) -> Result<Envelope<ObjectDetail>, String> {
    fixture::object(&object_id)
        .map(ok)
        .ok_or_else(|| format!("unknown object {object_id}"))
}

#[tauri::command]
pub fn documents() -> Envelope<Vec<DocumentRow>> {
    ok(fixture::documents())
}

#[tauri::command]
pub fn report_draft() -> Envelope<ReportDraft> {
    ok(fixture::report_draft())
}

#[tauri::command]
pub fn export_preview() -> Envelope<ExportPreview> {
    let mut env = ok(fixture::export_preview());
    env.outcome = Outcome::Unavailable;
    env.coverage = Some("No exporter is implemented in this spike.".to_owned());
    env
}

#[tauri::command]
pub fn timeline_marks() -> Envelope<Vec<TimeMark>> {
    ok(fixture::timeline_marks())
}

#[tauri::command]
pub fn list_jobs(state: State<'_, AppState>) -> Result<Envelope<Vec<Job>>, String> {
    let inner = state.lock()?;
    Ok(ok(inner.jobs.jobs().to_vec()))
}

// ── Shell commands ─────────────────────────────────────────────────────────

#[tauri::command]
pub fn switch_instrument(
    state: State<'_, AppState>,
    instrument: Instrument,
) -> Result<Envelope<ShellView>, String> {
    mutate(&state, |inner| {
        inner
            .shell
            .switch_instrument(instrument)
            .map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn select_object(
    state: State<'_, AppState>,
    object_id: String,
    version: u32,
) -> Result<Envelope<ShellView>, String> {
    mutate(&state, |inner| {
        let current = fixture::object_version(&object_id)
            .ok_or_else(|| format!("unknown object {object_id}"))?;
        if current != version {
            return Err(format!(
                "object {object_id} is at version {current}, not {version}"
            ));
        }
        let focus = fixture::object_focus(&object_id);
        inner.shell.select(Selection { object_id, version }, focus);
        Ok(())
    })
}

#[tauri::command]
pub fn clear_selection(state: State<'_, AppState>) -> Result<Envelope<ShellView>, String> {
    mutate(&state, |inner| {
        inner.shell.clear_selection();
        Ok(())
    })
}

#[tauri::command]
pub fn route_back(state: State<'_, AppState>) -> Result<Envelope<ShellView>, String> {
    mutate(&state, |inner| {
        inner
            .shell
            .route_back()
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn set_local_filter(
    state: State<'_, AppState>,
    instrument: Instrument,
    filter: Option<String>,
) -> Result<Envelope<ShellView>, String> {
    mutate(&state, |inner| {
        inner.shell.set_local_filter(instrument, filter);
        Ok(())
    })
}

#[tauri::command]
pub fn promote_local_filter(
    state: State<'_, AppState>,
    instrument: Instrument,
) -> Result<Envelope<ShellView>, String> {
    mutate(&state, |inner| {
        inner
            .shell
            .promote_local_filter(instrument)
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn scrub_clock(
    state: State<'_, AppState>,
    delta_min: i32,
) -> Result<Envelope<ShellView>, String> {
    mutate(&state, |inner| {
        inner.shell.scrub_clock(delta_min);
        Ok(())
    })
}

#[tauri::command]
pub fn set_clock(
    state: State<'_, AppState>,
    clock_min: u32,
) -> Result<Envelope<ShellView>, String> {
    mutate(&state, |inner| {
        inner.shell.set_clock(clock_min).map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn set_setting(
    state: State<'_, AppState>,
    change: SettingChange,
) -> Result<Envelope<ShellView>, String> {
    mutate(&state, |inner| {
        inner.shell.apply_setting(change).map_err(|e| e.to_string())
    })
}

// ── Jobs ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct SearchStarted {
    pub job_id: String,
    pub query_seq: u64,
}

#[tauri::command]
pub fn start_search(
    app: AppHandle,
    state: State<'_, AppState>,
    query: String,
    scope: String,
) -> Result<Envelope<SearchStarted>, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err("query is empty".to_owned());
    }
    if trimmed.chars().count() > fixture::MAX_QUERY_CHARS {
        return Err(format!(
            "query is longer than {} characters",
            fixture::MAX_QUERY_CHARS
        ));
    }
    let (job_id, query_seq) = {
        let mut inner = state.lock()?;
        let query_seq = inner.shell.next_query_seq();
        let job_id = inner.jobs.start(
            &format!("Search · \"{trimmed}\""),
            &scope,
            JobKind::Search {
                query: trimmed.to_owned(),
                scope: scope.clone(),
                query_seq,
            },
            JobState::Running,
            None,
            1,
        );
        inner.persist();
        (job_id, query_seq)
    };
    spawn_worker(app, job_id.clone());
    Ok(ok(SearchStarted { job_id, query_seq }))
}

#[tauri::command]
pub fn cancel_job(
    state: State<'_, AppState>,
    job_id: String,
) -> Result<Envelope<Vec<Job>>, String> {
    let mut inner = state.lock()?;
    inner.jobs.cancel(&job_id).map_err(|e| e.to_string())?;
    inner.persist();
    Ok(ok(inner.jobs.jobs().to_vec()))
}

#[tauri::command]
pub fn retry_job(
    app: AppHandle,
    state: State<'_, AppState>,
    job_id: String,
) -> Result<Envelope<Vec<Job>>, String> {
    let jobs = {
        let mut inner = state.lock()?;
        inner.jobs.retry(&job_id).map_err(|e| e.to_string())?;
        inner.persist();
        inner.jobs.jobs().to_vec()
    };
    spawn_worker(app, job_id);
    Ok(ok(jobs))
}

#[tauri::command]
pub fn open_job_result(
    state: State<'_, AppState>,
    job_id: String,
) -> Result<Envelope<serde_json::Value>, String> {
    let mut inner = state.lock()?;
    let result = inner
        .jobs
        .mark_applied(&job_id)
        .map_err(|e| e.to_string())?;
    inner.persist();
    Ok(ok(result))
}

// ── Startup measurement ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct StartupReport {
    /// Milliseconds from process start to the UI's first ready call.
    pub ms_since_process_start: u64,
    pub profile: &'static str,
}

#[tauri::command]
pub fn ui_ready(app: AppHandle) -> Envelope<StartupReport> {
    let ms = crate::PROCESS_START
        .get()
        .map(|start| u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0);
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let report = StartupReport {
        ms_since_process_start: ms,
        profile,
    };
    if let Err(e) = append_startup_log(&app, &report) {
        eprintln!("oia-shell-spike: startup log not written: {e}");
    }
    eprintln!("oia-shell-spike: ui ready after {ms} ms ({profile})");
    ok(report)
}

/// Appends one JSON line per launch to the startup log in the app data dir.
fn append_startup_log(app: &AppHandle, report: &StartupReport) -> Result<(), String> {
    use std::io::Write;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let line = serde_json::to_string(report).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(STARTUP_LOG))
        .map_err(|e| e.to_string())?;
    writeln!(file, "{line}").map_err(|e| e.to_string())
}

// ── Setup and fixture workers ──────────────────────────────────────────────

/// Resolves the state file, restores or reports, registers state, and starts
/// the fixture workers for initial jobs.
pub fn setup(app: AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = app.path().app_data_dir()?;
    let store_path = data_dir.join(STATE_FILE);

    let (shell, jobs, restore, can_persist_initial) = match persist::load(&store_path) {
        Ok(Some(mut persisted)) => {
            let interrupted = persisted.jobs.mark_interrupted();
            let saved_at_unix = persisted.saved_at_unix;
            (
                persisted.shell,
                persisted.jobs,
                RestoreReport::Restored {
                    saved_at_unix,
                    interrupted_jobs: interrupted,
                },
                true,
            )
        }
        Ok(None) => {
            let (shell, jobs) = fresh_state();
            (shell, jobs, RestoreReport::Fresh, true)
        }
        Err(e) => {
            let preserved_path = persist::preserve_unreadable(&store_path)
                .ok()
                .map(|p| p.display().to_string());
            let can_save = preserved_path.is_some();
            let (shell, jobs) = fresh_state();
            (
                shell,
                jobs,
                RestoreReport::Unreadable {
                    reason: e.to_string(),
                    preserved_path,
                },
                can_save,
            )
        }
    };

    let mut inner = Inner {
        shell,
        jobs,
        store_path: Some(store_path),
        restore,
        last_persist_error: None,
    };
    if can_persist_initial {
        inner.persist();
    }
    let active: Vec<String> = inner
        .jobs
        .jobs()
        .iter()
        .filter(|j| j.state.is_active())
        .map(|j| j.id.clone())
        .collect();
    app.manage(AppState::new(inner));
    for id in active {
        spawn_worker(app.clone(), id);
    }
    Ok(())
}

fn fresh_state() -> (ShellState, JobRegistry) {
    let shell = ShellState::new(fixture::initial_context());
    let mut jobs = JobRegistry::new();
    jobs.start(
        "OCR · FIXTURE-DOC-0007",
        fixture::DOC_ID,
        JobKind::FixtureOcr {
            document: fixture::DOC_ID.to_owned(),
        },
        JobState::Running,
        Some(Progress {
            done: 7,
            total: 9,
            unit: "pages".to_owned(),
        }),
        3,
    );
    let acquire = jobs.start(
        "Acquire · release-2",
        "FIXTURE-REL-0002",
        JobKind::FixtureAcquire {
            release: "release-2".to_owned(),
        },
        JobState::Running,
        None,
        3,
    );
    // The fixture provider is unavailable; the second attempt has already failed.
    let _ = jobs.finish(
        &acquire,
        Finish::Failed {
            error: "unavailable · 503 · attempt 1/3 (fixture provider)".to_owned(),
        },
        0,
    );
    let _ = jobs.retry(&acquire);
    let _ = jobs.set_running(&acquire);
    let _ = jobs.finish(
        &acquire,
        Finish::Failed {
            error: "unavailable · 503 · attempt 2/3 (fixture provider)".to_owned(),
        },
        0,
    );
    (shell, jobs)
}

fn with_state<F>(app: &AppHandle, f: F) -> bool
where
    F: FnOnce(&mut Inner) -> bool,
{
    let Some(state) = app.try_state::<AppState>() else {
        return false;
    };
    let Ok(mut inner) = state.lock() else {
        return false;
    };
    let changed = f(&mut inner);
    if changed {
        inner.persist();
    }
    changed
}

fn emit_jobs_changed(app: &AppHandle) {
    if let Err(e) = app.emit(JOBS_CHANGED, ()) {
        eprintln!("oia-shell-spike: event emit failed: {e}");
    }
}

/// Drives one fixture job to completion on its own thread, according to its
/// kind. Workers only ever report progress or finish; they never touch the
/// shell context.
fn spawn_worker(app: AppHandle, job_id: String) {
    let kind = app.try_state::<AppState>().and_then(|s| {
        s.lock()
            .ok()
            .and_then(|inner| inner.jobs.get(&job_id).map(|j| j.kind.clone()))
    });
    let Some(kind) = kind else {
        return;
    };
    thread::Builder::new()
        .name(format!("fixture-worker-{job_id}"))
        .spawn(move || match kind {
            JobKind::Search { query, scope, .. } => {
                thread::sleep(SEARCH_LATENCY);
                let result = fixture::search(&query, &scope);
                let value = serde_json::to_value(&result).unwrap_or(serde_json::Value::Null);
                with_state(&app, |inner| {
                    let seq = inner.shell.query_seq;
                    inner
                        .jobs
                        .finish(&job_id, Finish::Succeeded { result: value }, seq)
                        .is_ok()
                });
                emit_jobs_changed(&app);
            }
            JobKind::FixtureOcr { .. } => {
                if with_state(&app, |inner| inner.jobs.set_running(&job_id).is_ok()) {
                    emit_jobs_changed(&app);
                }
                loop {
                    thread::sleep(OCR_PAGE_INTERVAL);
                    let mut finished = false;
                    let changed = with_state(&app, |inner| {
                        let Some(job) = inner.jobs.get(&job_id) else {
                            finished = true;
                            return false;
                        };
                        if job.state != JobState::Running {
                            finished = true;
                            return false;
                        }
                        let (done, total) = job
                            .progress
                            .as_ref()
                            .map(|p| (p.done, p.total))
                            .unwrap_or((0, 9));
                        if done + 1 >= total {
                            finished = true;
                            let seq = inner.shell.query_seq;
                            inner
                                .jobs
                                .finish(
                                    &job_id,
                                    Finish::PartiallySucceeded {
                                        result: serde_json::json!({
                                            "document": fixture::DOC_ID,
                                            "version": fixture::DOC_VERSION,
                                            "pages_readable": 7,
                                            "pages_unreadable": [4, 8],
                                            "fixture": true
                                        }),
                                        extent: "OCR complete for 7 of 9 pages; pages 4 and 8 unreadable (fixture)".to_owned(),
                                    },
                                    seq,
                                )
                                .is_ok()
                        } else {
                            inner
                                .jobs
                                .report_progress(
                                    &job_id,
                                    Progress {
                                        done: done + 1,
                                        total,
                                        unit: "pages".to_owned(),
                                    },
                                )
                                .is_ok()
                        }
                    });
                    if changed {
                        emit_jobs_changed(&app);
                    }
                    if finished {
                        break;
                    }
                }
            }
            JobKind::FixtureAcquire { .. } => {
                thread::sleep(ACQUIRE_QUEUE_DELAY);
                if with_state(&app, |inner| inner.jobs.set_running(&job_id).is_ok()) {
                    emit_jobs_changed(&app);
                }
                thread::sleep(ACQUIRE_FAIL_DELAY);
                let changed = with_state(&app, |inner| {
                    let attempt = inner.jobs.get(&job_id).map(|j| (j.attempt, j.max_attempts));
                    let Some((attempt, max)) = attempt else {
                        return false;
                    };
                    let seq = inner.shell.query_seq;
                    inner
                        .jobs
                        .finish(
                            &job_id,
                            Finish::Failed {
                                error: format!(
                                    "unavailable · 503 · attempt {attempt}/{max} (fixture provider)"
                                ),
                            },
                            seq,
                        )
                        .is_ok()
                });
                if changed {
                    emit_jobs_changed(&app);
                }
            }
        })
        .map(|_| ())
        .unwrap_or_else(|e| eprintln!("oia-shell-spike: worker spawn failed: {e}"));
}
