//! Targeted contract checks for the renderer-independent shell model.
//!
//! Each test names the risk it guards. They run without a window.

use std::collections::BTreeMap;

use oia_shell_core::fixture;
use oia_shell_core::jobs::{Finish, JobKind, JobRegistry, JobState};
use oia_shell_core::persist::{self, PersistError, Persisted};
use oia_shell_core::shell::{EffectsQuality, Instrument, Selection, SettingChange, ShellState};

/// CASE-02 / UX-02: route back must restore instrument, selection, local
/// filters, clock, and timeline focus as one step, not just the instrument.
#[test]
fn route_back_restores_the_whole_previous_entry() {
    let mut shell = ShellState::new(fixture::initial_context());
    shell.set_local_filter(Instrument::Library, Some("ridge".to_owned()));
    let before = shell.clone();

    shell
        .switch_instrument(Instrument::Library)
        .unwrap_or_else(|e| panic!("library is available: {e}"));
    shell.select(
        Selection {
            object_id: "FIXTURE-ACC-A".to_owned(),
            version: 1,
        },
        fixture::object_focus("FIXTURE-ACC-A"),
    );
    shell.set_local_filter(Instrument::Library, None);
    shell.scrub_clock(-30);
    assert_eq!(shell.route_stack.len(), 2);

    shell
        .route_back()
        .unwrap_or_else(|e| panic!("selection entry: {e}"));
    assert_eq!(shell.instrument, Instrument::Library);
    assert_eq!(shell.context.selection, None);
    assert_eq!(
        shell
            .local_filters
            .get(&Instrument::Library)
            .map(String::as_str),
        Some("ridge")
    );
    assert_eq!(shell.context.clock_min, before.context.clock_min);

    shell
        .route_back()
        .unwrap_or_else(|e| panic!("instrument entry: {e}"));
    assert_eq!(shell.instrument, Instrument::Case);
    assert_eq!(shell.context, before.context);
    assert_eq!(shell.local_filters, before.local_filters);
    assert!(
        shell.route_back().is_err(),
        "empty stack is an error, not a silent no-op"
    );

    // Unavailable instruments are refused and leave no route entry.
    assert!(shell.switch_instrument(Instrument::Globe).is_err());
    assert!(shell.route_stack.is_empty());
}

/// CASE-04: a search that finishes after the query sequence has moved on is
/// superseded; its result is retained but never applied until the user opens it.
#[test]
fn late_search_result_is_superseded_not_applied() {
    let mut shell = ShellState::new(fixture::initial_context());
    let mut jobs = JobRegistry::new();

    let first_seq = shell.next_query_seq();
    let first = jobs.start(
        "Search · first",
        "this case",
        JobKind::Search {
            query: "ridge".to_owned(),
            scope: "this case".to_owned(),
            query_seq: first_seq,
        },
        JobState::Running,
        None,
        1,
    );
    let second_seq = shell.next_query_seq();
    let second = jobs.start(
        "Search · second",
        "this case",
        JobKind::Search {
            query: "light".to_owned(),
            scope: "this case".to_owned(),
            query_seq: second_seq,
        },
        JobState::Running,
        None,
        1,
    );

    let first_result = serde_json::json!({ "hits": 1 });
    let state = jobs
        .finish(
            &first,
            Finish::Succeeded {
                result: first_result.clone(),
            },
            shell.query_seq,
        )
        .unwrap_or_else(|e| panic!("finish first: {e}"));
    assert_eq!(state, JobState::Superseded);
    let job = jobs
        .get(&first)
        .unwrap_or_else(|| panic!("first job exists"));
    assert!(!job.applied);
    assert_eq!(
        job.result.as_ref(),
        Some(&first_result),
        "retained, not discarded"
    );

    let state = jobs
        .finish(
            &second,
            Finish::Succeeded {
                result: serde_json::json!({ "hits": 3 }),
            },
            shell.query_seq,
        )
        .unwrap_or_else(|e| panic!("finish second: {e}"));
    assert_eq!(state, JobState::Succeeded);
    assert!(jobs.get(&second).is_some_and(|j| j.applied));

    // Only an explicit user action applies the superseded result.
    let opened = jobs
        .mark_applied(&first)
        .unwrap_or_else(|e| panic!("open superseded: {e}"));
    assert_eq!(opened, first_result);
    assert!(jobs.get(&first).is_some_and(|j| j.applied));

    // A failed job has no result to open.
    let failed = jobs.start(
        "Acquire",
        "x",
        JobKind::FixtureAcquire {
            release: "r".to_owned(),
        },
        JobState::Running,
        None,
        2,
    );
    jobs.finish(
        &failed,
        Finish::Failed {
            error: "unavailable".to_owned(),
        },
        shell.query_seq,
    )
    .unwrap_or_else(|e| panic!("finish failed: {e}"));
    assert!(jobs.mark_applied(&failed).is_err());
    assert!(jobs.retry(&failed).is_ok());
    assert_eq!(jobs.get(&failed).map(|j| j.state), Some(JobState::Queued));
}

/// A01 slice: the shell context round-trips through the state file; jobs that
/// were active at shutdown come back interrupted; a malformed file is reported
/// rather than silently replaced by defaults.
#[test]
fn persisted_state_round_trips_and_reports_corruption() {
    let dir = std::env::temp_dir().join(format!(
        "oia-shell-spike-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("temp dir: {e}"));
    let path = dir.join("shell-state.json");

    assert_eq!(
        persist::load(&path),
        Ok(None),
        "missing file is not an error"
    );

    let mut shell = ShellState::new(fixture::initial_context());
    shell
        .switch_instrument(Instrument::Report)
        .unwrap_or_else(|e| panic!("report: {e}"));
    shell.select(
        Selection {
            object_id: "FIXTURE-ACC-C".to_owned(),
            version: 1,
        },
        None,
    );
    shell
        .apply_setting(SettingChange::ReducedTransparency(true))
        .unwrap_or_else(|e| panic!("setting: {e}"));
    let mut jobs = JobRegistry::new();
    jobs.start(
        "OCR",
        "doc",
        JobKind::FixtureOcr {
            document: "doc".to_owned(),
        },
        JobState::Running,
        None,
        1,
    );

    persist::save(&path, &Persisted::new(shell.clone(), jobs.clone()))
        .unwrap_or_else(|e| panic!("save: {e}"));
    let mut restored = persist::load(&path)
        .unwrap_or_else(|e| panic!("load: {e}"))
        .unwrap_or_else(|| panic!("state present"));
    assert_eq!(restored.shell, shell);
    assert_eq!(restored.jobs, jobs);
    assert_eq!(restored.jobs.mark_interrupted(), 1);
    assert_eq!(
        restored.jobs.ids_in_state(JobState::Interrupted).len(),
        1,
        "a job running at shutdown is never shown as succeeded"
    );

    std::fs::write(&path, b"{ not json").unwrap_or_else(|e| panic!("corrupt: {e}"));
    assert!(matches!(
        persist::load(&path),
        Err(PersistError::Malformed(_))
    ));
    let backup = persist::preserve_unreadable(&path).unwrap_or_else(|e| panic!("preserve: {e}"));
    persist::save(
        &path,
        &Persisted::new(
            ShellState::new(fixture::initial_context()),
            JobRegistry::new(),
        ),
    )
    .unwrap_or_else(|e| panic!("save defaults after preserve: {e}"));
    assert_eq!(
        std::fs::read(&backup).unwrap_or_else(|e| panic!("read backup: {e}")),
        b"{ not json",
        "unreadable state file is preserved aside when defaults are written"
    );
    assert!(
        persist::load(&path)
            .unwrap_or_else(|e| panic!("load fresh defaults: {e}"))
            .is_some()
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// UX-05: effect settings never touch the shared context or its values.
#[test]
fn settings_do_not_change_context() {
    let mut shell = ShellState::new(fixture::initial_context());
    let context = shell.context.clone();
    for change in [
        SettingChange::ReducedMotion(true),
        SettingChange::ReducedTransparency(true),
        SettingChange::Effects(EffectsQuality::Low),
        SettingChange::CleanEvidence(true),
        SettingChange::FocusedReading(true),
        SettingChange::Sound(true),
        SettingChange::ReadingTextPx(18),
    ] {
        shell
            .apply_setting(change)
            .unwrap_or_else(|e| panic!("setting: {e}"));
    }
    assert_eq!(shell.context, context);
    assert!(
        shell
            .apply_setting(SettingChange::ReadingTextPx(8))
            .is_err()
    );
    assert_eq!(shell.settings.reading_text_px, 18);
    assert_eq!(shell.local_filters, BTreeMap::new());
}
