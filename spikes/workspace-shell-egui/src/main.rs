//! Rust-native comparison spike for ADR 0008: the same P1 shell subset as the
//! Tauri spike, drawn with egui/eframe over the shared `oia-shell-core` model.
//! Fixture data only. Same measurements, same machine, same method.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Frame, Key, Pos2, Rect, RichText, Sense, Shadow,
    Shape, Stroke, Vec2,
};
use oia_shell_core::fixture::{self, CaseGeometry, Km, ObjectDetail, SearchResult};
use oia_shell_core::jobs::{Finish, JobKind, JobRegistry, JobState, Progress};
use oia_shell_core::persist::{self, Persisted, RestoreReport};
use oia_shell_core::shell::{Instrument, InstrumentInfo, Selection, SettingChange, ShellState};

// Tokens from docs/design/workspace-ui-ux.md §7.1.
const TEXT: Color32 = Color32::from_rgb(0xe6, 0xed, 0xf3);
const TEXT_2: Color32 = Color32::from_rgba_premultiplied(0x7f, 0x83, 0x86, 0x8c);
const TEXT_3: Color32 = Color32::from_rgba_premultiplied(0x4a, 0x4c, 0x4e, 0x52);
const CYAN: Color32 = Color32::from_rgb(0x45, 0xd1, 0xff);
const GREEN: Color32 = Color32::from_rgb(0x5f, 0xe0, 0x8a);
const AMBER: Color32 = Color32::from_rgb(0xf5, 0x9e, 0x2e);
const RED: Color32 = Color32::from_rgb(0xff, 0x6b, 0x6b);
const VIOLET: Color32 = Color32::from_rgb(0xb5, 0x9c, 0xff);
const LINE: Color32 = Color32::from_rgba_premultiplied(20, 20, 20, 20);

const SEARCH_LATENCY: Duration = Duration::from_millis(1200);
const OCR_PAGE_INTERVAL: Duration = Duration::from_millis(2500);
const ACQUIRE_FAIL_DELAY: Duration = Duration::from_millis(1600);

fn main() -> eframe::Result<()> {
    let start = Instant::now();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("OIA workspace shell spike, egui (fixture data, nothing live)")
            .with_inner_size([1400.0, 840.0])
            .with_min_inner_size([960.0, 620.0]),
        ..Default::default()
    };
    eframe::run_native(
        "OIA shell spike (egui)",
        options,
        Box::new(move |_cc| Ok(Box::new(App::new(start)))),
    )
}

struct Timer {
    job_id: String,
    due: Instant,
}

struct App {
    start: Instant,
    first_frame_ms: Option<u64>,
    shell: ShellState,
    jobs: JobRegistry,
    store_path: Option<PathBuf>,
    restore: RestoreReport,
    persist_error: Option<String>,
    geometry: CaseGeometry,
    object: Option<ObjectDetail>,
    search: Option<SearchResult>,
    pending_searches: Vec<(String, mpsc::Receiver<SearchResult>)>,
    timers: Vec<Timer>,
    query: String,
    notices: VecDeque<String>,
    inspector_tab: usize,
    focus_command: bool,
    opaque: bool,
}

impl App {
    fn new(start: Instant) -> Self {
        let store_path = std::env::var_os("APPDATA").map(|d| {
            PathBuf::from(d)
                .join("org.oia.shell-spike-egui")
                .join("shell-state.json")
        });
        let (shell, jobs, restore) = match store_path.as_deref().map(persist::load) {
            Some(Ok(Some(mut p))) => {
                let interrupted = p.jobs.mark_interrupted();
                let saved_at_unix = p.saved_at_unix;
                (
                    p.shell,
                    p.jobs,
                    RestoreReport::Restored {
                        saved_at_unix,
                        interrupted_jobs: interrupted,
                    },
                )
            }
            Some(Ok(None)) | None => {
                let (s, j) = fresh_state();
                (s, j, RestoreReport::Fresh)
            }
            Some(Err(e)) => {
                let (s, j) = fresh_state();
                (
                    s,
                    j,
                    RestoreReport::Unreadable {
                        reason: e.to_string(),
                    },
                )
            }
        };
        let mut notices = VecDeque::new();
        match &restore {
            RestoreReport::Restored {
                interrupted_jobs, ..
            } => notices.push_back(format!(
                "Previous context restored. {interrupted_jobs} job(s) were running at shutdown and are marked interrupted."
            )),
            RestoreReport::Unreadable { reason } => notices.push_back(format!(
                "Previous context could not be read: {reason}. Defaults are in effect."
            )),
            RestoreReport::Fresh => {}
        }
        let mut app = Self {
            start,
            first_frame_ms: None,
            opaque: shell.settings.reduced_transparency,
            shell,
            jobs,
            store_path,
            restore,
            persist_error: None,
            geometry: fixture::case_geometry(),
            object: None,
            search: None,
            pending_searches: Vec::new(),
            timers: Vec::new(),
            query: String::new(),
            notices,
            inspector_tab: 0,
            focus_command: false,
        };
        app.load_selection();
        let now = Instant::now();
        for job in app.jobs.jobs() {
            if job.state == JobState::Running && matches!(job.kind, JobKind::FixtureOcr { .. }) {
                app.timers.push(Timer {
                    job_id: job.id.clone(),
                    due: now + OCR_PAGE_INTERVAL,
                });
            }
        }
        app.persist();
        app
    }

    fn persist(&mut self) {
        let Some(path) = self.store_path.clone() else {
            return;
        };
        self.persist_error = persist::save(
            &path,
            &Persisted::new(self.shell.clone(), self.jobs.clone()),
        )
        .err()
        .map(|e| e.to_string());
    }

    fn load_selection(&mut self) {
        self.object = self
            .shell
            .context
            .selection
            .as_ref()
            .and_then(|s| fixture::object(&s.object_id));
    }

    fn select(&mut self, id: &str) {
        if let Some(version) = fixture::object_version(id) {
            self.shell.select(
                Selection {
                    object_id: id.to_owned(),
                    version,
                },
                fixture::object_focus(id),
            );
            self.inspector_tab = 0;
            self.load_selection();
            self.persist();
        }
    }

    fn switch(&mut self, instrument: Instrument) {
        match self.shell.switch_instrument(instrument) {
            Ok(()) => self.persist(),
            Err(e) => self.notices.push_back(e.to_string()),
        }
    }

    fn route_back(&mut self) {
        if self.shell.route_back().is_ok() {
            self.load_selection();
            self.persist();
        }
    }

    fn setting(&mut self, change: SettingChange) {
        if let Err(e) = self.shell.apply_setting(change) {
            self.notices.push_back(e.to_string());
        }
        self.opaque = self.shell.settings.reduced_transparency;
        self.persist();
    }

    fn start_search(&mut self) {
        let q = self.query.trim().to_owned();
        if q.is_empty() || q.chars().count() > fixture::MAX_QUERY_CHARS {
            return;
        }
        let seq = self.shell.next_query_seq();
        let id = self.jobs.start(
            &format!("Search · \"{q}\""),
            "this case",
            JobKind::Search {
                query: q.clone(),
                scope: "this case".to_owned(),
                query_seq: seq,
            },
            JobState::Running,
            None,
            1,
        );
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            std::thread::sleep(SEARCH_LATENCY);
            let _ = tx.send(fixture::search(&q, "this case"));
        });
        self.pending_searches.push((id, rx));
        self.search = None;
        if self.shell.instrument != Instrument::Library {
            self.switch(Instrument::Library);
        }
        self.persist();
    }

    fn poll_workers(&mut self) {
        let now = Instant::now();
        let mut changed = false;
        let mut finished: Vec<(String, SearchResult)> = Vec::new();
        self.pending_searches
            .retain(|(id, rx)| match rx.try_recv() {
                Ok(result) => {
                    finished.push((id.clone(), result));
                    false
                }
                Err(mpsc::TryRecvError::Empty) => true,
                Err(mpsc::TryRecvError::Disconnected) => false,
            });
        for (id, result) in finished {
            let value = serde_json::to_value(&result).unwrap_or(serde_json::Value::Null);
            let seq = self.shell.query_seq;
            match self
                .jobs
                .finish(&id, Finish::Succeeded { result: value }, seq)
            {
                Ok(JobState::Succeeded) => self.search = Some(result),
                Ok(JobState::Superseded) => self.notices.push_back(format!(
                    "A result for a previous query finished ({id}). It was not applied."
                )),
                _ => {}
            }
            changed = true;
        }
        let due: Vec<String> = self
            .timers
            .iter()
            .filter(|t| t.due <= now)
            .map(|t| t.job_id.clone())
            .collect();
        self.timers.retain(|t| t.due > now);
        for id in due {
            let Some(job) = self.jobs.get(&id) else {
                continue;
            };
            match job.kind.clone() {
                JobKind::FixtureOcr { .. } if job.state == JobState::Running => {
                    let (done, total) = job
                        .progress
                        .as_ref()
                        .map(|p| (p.done, p.total))
                        .unwrap_or((0, 9));
                    if done + 1 >= total {
                        let seq = self.shell.query_seq;
                        let _ = self.jobs.finish(
                            &id,
                            Finish::PartiallySucceeded {
                                result: serde_json::json!({"pages_readable": 7, "pages_unreadable": [4, 8], "fixture": true}),
                                extent: "OCR complete for 7 of 9 pages; pages 4 and 8 unreadable (fixture)".to_owned(),
                            },
                            seq,
                        );
                    } else {
                        let _ = self.jobs.report_progress(
                            &id,
                            Progress {
                                done: done + 1,
                                total,
                                unit: "pages".to_owned(),
                            },
                        );
                        self.timers.push(Timer {
                            job_id: id,
                            due: now + OCR_PAGE_INTERVAL,
                        });
                    }
                    changed = true;
                }
                JobKind::FixtureAcquire { .. } if job.state.is_active() => {
                    let (attempt, max) = (job.attempt, job.max_attempts);
                    let _ = self.jobs.set_running(&id);
                    let seq = self.shell.query_seq;
                    let _ = self.jobs.finish(
                        &id,
                        Finish::Failed {
                            error: format!(
                                "unavailable · 503 · attempt {attempt}/{max} (fixture provider)"
                            ),
                        },
                        seq,
                    );
                    changed = true;
                }
                _ => {}
            }
        }
        if changed {
            self.persist();
        }
    }

    fn glass(&self, strong: bool) -> Frame {
        let fill = match (self.opaque, strong) {
            (true, false) => Color32::from_rgb(0x0a, 0x0d, 0x11),
            (true, true) => Color32::from_rgb(0x0c, 0x10, 0x14),
            (false, false) => Color32::from_rgba_unmultiplied(6, 9, 12, 168),
            (false, true) => Color32::from_rgba_unmultiplied(8, 12, 16, 204),
        };
        let shadow = if self.shell.settings.effects == oia_shell_core::shell::EffectsQuality::Low {
            Shadow::NONE
        } else {
            Shadow {
                offset: [0, 10],
                blur: 30,
                spread: 0,
                color: Color32::from_black_alpha(128),
            }
        };
        Frame::NONE
            .fill(fill)
            .stroke(Stroke::new(1.0, LINE))
            .corner_radius(CornerRadius::same(14))
            .inner_margin(12.0)
            .shadow(shadow)
    }

    /// Key chords are read from the per-event modifier state, not the
    /// end-of-frame state: a modifier released within the same frame as the
    /// key (as automation and fast typists do) must still count.
    fn handle_keys(&mut self, ctx: &egui::Context) {
        let chords: Vec<(Key, egui::Modifiers)> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => Some((*key, *modifiers)),
                    _ => None,
                })
                .collect()
        });
        let text_focused = ctx.memory(|m| m.focused().is_some());
        const RAIL_KEYS: [Key; 10] = [
            Key::Num1,
            Key::Num2,
            Key::Num3,
            Key::Num4,
            Key::Num5,
            Key::Num6,
            Key::Num7,
            Key::Num8,
            Key::Num9,
            Key::Num0,
        ];
        for (key, m) in chords {
            let ctrl_only = m.ctrl && !m.shift && !m.alt;
            if ctrl_only && let Some(i) = RAIL_KEYS.iter().position(|k| *k == key) {
                if let Some(inst) = Instrument::from_rail_position(u8::try_from(i + 1).unwrap_or(1))
                {
                    self.switch(inst);
                }
                continue;
            }
            match key {
                Key::K if ctrl_only => self.focus_command = true,
                Key::E if m.ctrl && m.shift => {
                    let v = !self.shell.settings.clean_evidence;
                    self.setting(SettingChange::CleanEvidence(v));
                }
                Key::F if m.ctrl && m.shift => {
                    let v = !self.shell.settings.focused_reading;
                    self.setting(SettingChange::FocusedReading(v));
                }
                Key::ArrowLeft if m.alt => self.route_back(),
                Key::ArrowLeft if !text_focused => {
                    self.shell.scrub_clock(if m.shift { -50 } else { -5 });
                    self.persist();
                }
                Key::ArrowRight if !text_focused => {
                    self.shell.scrub_clock(if m.shift { 50 } else { 5 });
                    self.persist();
                }
                Key::Escape => {
                    self.notices.pop_front();
                }
                _ => {}
            }
        }
    }
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

fn hhmm(min: u32) -> String {
    format!("{:02}:{:02}", min / 60, min % 60)
}

fn k(text: &str) -> RichText {
    RichText::new(text.to_uppercase())
        .monospace()
        .size(11.0)
        .color(TEXT_2)
}

fn v(text: &str) -> RichText {
    RichText::new(text).monospace().size(12.0).color(TEXT)
}

fn badge(ui: &mut egui::Ui, glyph: &str, label: &str, color: Color32) {
    Frame::NONE
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.5)))
        .corner_radius(CornerRadius::same(99))
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                ui.label(RichText::new(glyph).monospace().size(11.0).color(color));
                ui.label(k(label));
            });
        });
}

impl eframe::App for App {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = &root.ctx().clone();
        if self.first_frame_ms.is_none() {
            let ms = u64::try_from(self.start.elapsed().as_millis()).unwrap_or(u64::MAX);
            self.first_frame_ms = Some(ms);
            let profile = if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            };
            eprintln!("oia-shell-spike-egui: first frame after {ms} ms ({profile})");
            if let Some(dir) = self.store_path.as_ref().and_then(|p| p.parent()) {
                use std::io::Write;
                let _ = std::fs::create_dir_all(dir);
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(dir.join("startup-log.jsonl"))
                {
                    let _ = writeln!(
                        f,
                        "{{\"ms_since_process_start\":{ms},\"profile\":\"{profile}\",\"renderer\":\"egui\"}}"
                    );
                }
            }
        }
        self.poll_workers();
        self.handle_keys(ctx);
        if !self.pending_searches.is_empty() || !self.timers.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(200));
        }

        let screen = ctx.content_rect();
        let focused = self.shell.settings.focused_reading;
        let clean = self.shell.settings.clean_evidence;

        // C. Evidence canvas, full bleed.
        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(Color32::BLACK))
            .show(root, |ui| {
                if self.shell.instrument == Instrument::Case {
                    let free = Rect::from_min_max(
                        Pos2::new(if focused { 24.0 } else { 100.0 }, 100.0),
                        Pos2::new(
                            screen.right() - 384.0,
                            screen.bottom() - if focused { 110.0 } else { 186.0 },
                        ),
                    );
                    let mut clicked: Option<String> = None;
                    draw_geometry(ui, &self.geometry, free, &self.shell, clean, &mut clicked);
                    if let Some(id) = clicked {
                        self.select(&id);
                    }
                } else {
                    let rect = Rect::from_min_max(
                        Pos2::new(if focused { 24.0 } else { 450.0 }, 100.0),
                        Pos2::new(screen.right() - 384.0, screen.bottom() - 150.0),
                    );
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                    self.draw_pane(&mut child);
                }
            });

        // Fixture strip.
        egui::Area::new(egui::Id::unique("fixture-strip"))
            .fixed_pos(Pos2::new(screen.center().x - 190.0, 0.0))
            .show(ctx, |ui| {
                Frame::NONE
                    .fill(Color32::from_black_alpha(180))
                    .stroke(Stroke::new(1.0, VIOLET.gamma_multiply(0.6)))
                    .inner_margin(egui::Margin::symmetric(14, 4))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(
                                "FIXTURE · SYNTHETIC CASE · NO LIVE SOURCES · SPIKE BUILD · EGUI",
                            )
                            .monospace()
                            .size(10.0)
                            .color(VIOLET),
                        );
                    });
            });

        if !focused {
            // Identity, case card, rail.
            egui::Area::new(egui::Id::unique("ident"))
                .fixed_pos(Pos2::new(24.0, 22.0))
                .show(ctx, |ui| {
                    ui.set_width(330.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("◉ OIA").size(14.0).strong().color(TEXT));
                        ui.label(k("local · private"));
                    });
                    let home = fixture::case_home();
                    self.glass(false).show(ui, |ui| {
                        ui.set_width(306.0);
                        ui.set_max_width(306.0);
                        ui.label(k(&format!("Case · {}", home.title)));
                        ui.label(RichText::new(&home.question).size(14.0).color(TEXT));
                        ui.horizontal_wrapped(|ui| {
                            for row in &home.inventory {
                                badge(
                                    ui,
                                    row.origin.glyph,
                                    &format!("{} {}", row.count, row.origin.label),
                                    if row.origin.tone == "warn" {
                                        AMBER
                                    } else {
                                        TEXT_2
                                    },
                                );
                            }
                        });
                    });
                    ui.add_space(10.0);
                    let mut switch_to: Option<Instrument> = None;
                    self.glass(false).show(ui, |ui| {
                        ui.set_width(24.0);
                        for info in InstrumentInfo::all() {
                            let active = info.id == self.shell.instrument;
                            let color = if active {
                                CYAN
                            } else if info.available {
                                TEXT_2
                            } else {
                                TEXT_3
                            };
                            let text = RichText::new(info.glyph)
                                .monospace()
                                .size(14.0)
                                .color(color);
                            let resp = ui
                                .add(egui::Button::new(text).frame(active).fill(if active {
                                    CYAN.gamma_multiply(0.16)
                                } else {
                                    Color32::TRANSPARENT
                                }))
                                .on_hover_text(match info.reason {
                                    Some(r) => format!("{} · {r}", info.label),
                                    None => info.label.to_owned(),
                                });
                            if resp.clicked() {
                                switch_to = Some(info.id);
                            }
                        }
                    });
                    if let Some(i) = switch_to {
                        self.switch(i);
                    }
                });
        }

        // G. Context strip.
        egui::Area::new(egui::Id::unique("context"))
            .fixed_pos(Pos2::new(378.0, 22.0))
            .show(ctx, |ui| {
                let ctx_ = self.shell.context.clone();
                let mut back = false;
                self.glass(false)
                    .corner_radius(CornerRadius::same(99))
                    .inner_margin(egui::Margin::symmetric(12, 6))
                    .show(ui, |ui| {
                        ui.set_width(screen.width() - 378.0 - 360.0 - 36.0);
                        ui.horizontal(|ui| {
                            ui.label(k("case"));
                            ui.label(v(&ctx_.case_title.chars().take(14).collect::<String>()));
                            ui.label(k("›"));
                            ui.label(k("area"));
                            ui.label(v(&ctx_
                                .area
                                .as_ref()
                                .map(|a| format!("≈ {} km", a.extent_km.unwrap_or(0.0)))
                                .unwrap_or_else(|| "— unknown".to_owned())));
                            ui.label(k("›"));
                            ui.label(k("interval"));
                            ui.label(v(&format!(
                                "{}–{}",
                                hhmm(ctx_.interval.start_min),
                                hhmm(ctx_.interval.end_min)
                            )));
                            ui.label(k("›"));
                            ui.label(k("selected"));
                            ui.label(v(ctx_
                                .selection
                                .as_ref()
                                .map(|s| s.object_id.as_str())
                                .unwrap_or("none")));
                            if self.shell.settings.clean_evidence {
                                badge(ui, "◌", "clean evidence", CYAN);
                            }
                            if self.shell.settings.focused_reading {
                                badge(ui, "▭", "focused reading", CYAN);
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add_enabled(
                                            !self.shell.route_stack.is_empty(),
                                            egui::Button::new(k(&format!(
                                                "← back · {}",
                                                self.shell.route_stack.len()
                                            ))),
                                        )
                                        .clicked()
                                    {
                                        back = true;
                                    }
                                },
                            );
                        });
                    });
                if back {
                    self.route_back();
                }
            });

        // Clock.
        egui::Area::new(egui::Id::unique("clock"))
            .fixed_pos(Pos2::new(screen.right() - 360.0, 22.0))
            .show(ctx, |ui| {
                let c = &self.shell.context;
                self.glass(false).show(ui, |ui| {
                    ui.set_width(312.0);
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(k("mode"));
                            badge(ui, "▮", &format!("{:?}", c.clock_mode), TEXT_2);
                        });
                        ui.add_space(18.0);
                        ui.vertical(|ui| {
                            ui.label(k("investigation clock"));
                            ui.label(
                                RichText::new(hhmm(c.clock_min))
                                    .monospace()
                                    .size(20.0)
                                    .color(TEXT),
                            );
                        });
                    });
                    ui.label(k("reference"));
                    ui.label(v(&format!(
                        "{} · {} · {}",
                        c.interval.date, c.interval.zone, c.interval.reference
                    )));
                });
            });

        // D. Inspector.
        egui::Area::new(egui::Id::unique("inspector")).fixed_pos(Pos2::new(screen.right() - 360.0, 128.0)).show(ctx, |ui| {
            let mut select: Option<String> = None;
            self.glass(false).show(ui, |ui| {
                ui.set_width(312.0);
                ui.set_min_height(screen.height() - 128.0 - 150.0 - 24.0);
                ui.horizontal(|ui| {
                    for (i, name) in ["Selected", "Source", "Claims", "Notes", "Lineage"].iter().enumerate() {
                        if ui.selectable_label(self.inspector_tab == i, k(name)).clicked() {
                            self.inspector_tab = i;
                        }
                    }
                });
                ui.separator();
                match (&self.object, self.inspector_tab) {
                    (None, _) => { ui.label(RichText::new("Nothing selected. Select an account, source, or claim on the canvas or in a list.").size(12.0).color(TEXT_2)); }
                    (Some(o), 0) => draw_selected(ui, o, &mut select),
                    (Some(o), 1) => { if let ObjectDetail::Source { .. } = o { draw_selected(ui, o, &mut select) } else if ui.button(v("Open FIXTURE-DOC-0007 · v2")).clicked() { select = Some(fixture::DOC_ID.to_owned()); } }
                    (Some(_), 2) => { for id in ["FIXTURE-CLM-0001", "FIXTURE-CLM-0002"] { if ui.button(v(id)).clicked() { select = Some(id.to_owned()); } } }
                    (Some(_), 3) => { ui.label(k("your notes")); ui.label(RichText::new("No notes. Notes are separate records linked to the source version.").size(12.0).color(TEXT_2)); }
                    (Some(o), _) => { ui.label(k("lineage")); if let ObjectDetail::Source { lineage, .. } = o { for l in lineage { ui.label(v(l)); } } else { ui.label(RichText::new("Lineage is shown for sources.").size(12.0).color(TEXT_2)); } }
                }
            });
            if let Some(id) = select {
                self.select(&id);
            }
        });

        if !focused {
            // E. Shared timeline.
            egui::Area::new(egui::Id::unique("timeline"))
                .fixed_pos(Pos2::new(212.0, screen.bottom() - 86.0 - 92.0))
                .show(ctx, |ui| {
                    self.glass(false).show(ui, |ui| {
                        ui.set_width(screen.width() - 212.0 - 384.0 - 24.0);
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(k("shared timeline"));
                                ui.label(v(&format!(
                                    "{} → {}",
                                    hhmm(self.shell.context.interval.start_min),
                                    hhmm(self.shell.context.interval.end_min)
                                )));
                                ui.label(k(&format!(
                                    "{} · ← → scrub",
                                    self.shell.context.interval.date
                                )));
                            });
                            ui.add_space(18.0);
                            let (rect, _) = ui.allocate_exact_size(
                                Vec2::new(ui.available_width(), 60.0),
                                Sense::hover(),
                            );
                            draw_timeline(ui.painter(), rect, &self.shell);
                        });
                    });
                });

            // Readout.
            egui::Area::new(egui::Id::unique("readout"))
                .fixed_pos(Pos2::new(24.0, screen.bottom() - 22.0 - 150.0))
                .show(ctx, |ui| {
                    self.glass(false).show(ui, |ui| {
                        ui.set_width(146.0);
                        let rows = [
                            (
                                "view",
                                format!("{:?}", self.shell.instrument).to_lowercase(),
                            ),
                            (
                                "basemap",
                                if self.shell.instrument == Instrument::Case {
                                    self.geometry.basemap.to_owned()
                                } else {
                                    "—".to_owned()
                                },
                            ),
                            (
                                "selected",
                                self.shell
                                    .context
                                    .selection
                                    .as_ref()
                                    .map(|s| s.object_id.replace("FIXTURE-", ""))
                                    .unwrap_or_else(|| "none".to_owned()),
                            ),
                            (
                                "overlays",
                                if clean {
                                    "off".to_owned()
                                } else {
                                    "on".to_owned()
                                },
                            ),
                            ("clock", hhmm(self.shell.context.clock_min)),
                            (
                                "start",
                                format!(
                                    "{} ms · {}",
                                    self.first_frame_ms.unwrap_or(0),
                                    if cfg!(debug_assertions) {
                                        "debug"
                                    } else {
                                        "release"
                                    }
                                ),
                            ),
                        ];
                        for (key, val) in rows {
                            ui.horizontal(|ui| {
                                ui.label(k(key));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            RichText::new(val).monospace().size(11.0).color(TEXT),
                                        );
                                    },
                                );
                            });
                        }
                        if let Some(e) = &self.persist_error {
                            ui.label(
                                RichText::new(format!("persist failed: {e}"))
                                    .size(10.0)
                                    .color(RED),
                            );
                        }
                    });
                });
        }

        // A. Command dock.
        egui::Area::new(egui::Id::unique("dock"))
            .fixed_pos(Pos2::new(
                screen.center().x - 380.0,
                screen.bottom() - 22.0 - 44.0,
            ))
            .show(ctx, |ui| {
                let mut submit = false;
                self.glass(true)
                    .corner_radius(CornerRadius::same(99))
                    .inner_margin(egui::Margin::symmetric(14, 8))
                    .show(ui, |ui| {
                        ui.set_width(760.0);
                        ui.horizontal(|ui| {
                            badge(ui, "⌕", "scope · this case", CYAN);
                            let resp = ui.add(
                                egui::TextEdit::singleline(&mut self.query)
                                    .id(egui::Id::unique("command"))
                                    .hint_text("Search, or id:  near:  t:  in:")
                                    .desired_width(380.0)
                                    .frame(Frame::NONE),
                            );
                            if self.focus_command {
                                resp.request_focus();
                                if let Some(mut st) = egui::TextEdit::load_state(ui.ctx(), resp.id)
                                {
                                    let end = egui::text::CCursor::new(self.query.chars().count());
                                    st.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                                        egui::text::CCursor::new(0),
                                        end,
                                    )));
                                    st.store(ui.ctx(), resp.id);
                                }
                                self.focus_command = false;
                            }
                            if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                                submit = true;
                            }
                            ui.label(k("ctrl k"));
                            if ui
                                .button(RichText::new("Save · private").size(12.0))
                                .on_hover_text(
                                    "Save is private. Nothing in this spike is published.",
                                )
                                .clicked()
                            {
                                self.notices.push_back(
                                    "Save is private and keeps this case local.".to_owned(),
                                );
                            }
                        });
                    });
                if submit {
                    self.start_search();
                }
            });

        // F. Job tray.
        egui::Area::new(egui::Id::unique("jobs"))
            .fixed_pos(Pos2::new(
                screen.right() - 360.0,
                screen.bottom() - 22.0 - 110.0,
            ))
            .show(ctx, |ui| {
                let mut action: Option<(&'static str, String)> = None;
                self.glass(false).show(ui, |ui| {
                    ui.set_width(312.0);
                    ui.set_max_height(86.0);
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for job in self.jobs.jobs().iter().rev().take(4) {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(&job.name).monospace().size(11.0).color(TEXT),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if job.state.is_active() && ui.button(k("cancel")).clicked()
                                        {
                                            action = Some(("cancel", job.id.clone()));
                                        }
                                        if job.state.can_retry()
                                            && ui
                                                .add_enabled(
                                                    job.attempt < job.max_attempts,
                                                    egui::Button::new(k("retry")),
                                                )
                                                .clicked()
                                        {
                                            action = Some(("retry", job.id.clone()));
                                        }
                                        if job.state.has_result() && ui.button(k("open")).clicked()
                                        {
                                            action = Some(("open", job.id.clone()));
                                        }
                                    },
                                );
                            });
                            if job.state == JobState::Running {
                                let (rect, _) = ui.allocate_exact_size(
                                    Vec2::new(ui.available_width(), 3.0),
                                    Sense::hover(),
                                );
                                ui.painter()
                                    .rect_filled(rect, 2.0, Color32::from_white_alpha(20));
                                if let Some(p) = &job.progress {
                                    let w = rect.width() * p.done as f32 / p.total.max(1) as f32;
                                    ui.painter().rect_filled(
                                        Rect::from_min_size(rect.min, Vec2::new(w, 3.0)),
                                        2.0,
                                        CYAN,
                                    );
                                }
                            }
                            let mut parts = vec![format!("{:?}", job.state).to_lowercase()];
                            match (&job.progress, job.state) {
                                (Some(p), JobState::Running) => {
                                    parts.push(format!("{} / {} {}", p.done, p.total, p.unit))
                                }
                                (None, JobState::Running) => {
                                    parts.push("progress not reported".to_owned())
                                }
                                _ => {}
                            }
                            if let Some(e) = &job.extent {
                                parts.push(e.clone());
                            }
                            if let Some(e) = &job.error {
                                parts.push(e.clone());
                            }
                            if job.state == JobState::Superseded {
                                parts.push(if job.applied {
                                    "opened by user".to_owned()
                                } else {
                                    "not applied".to_owned()
                                });
                            }
                            parts.push("fixture".to_owned());
                            let color = match job.state {
                                JobState::Failed => RED,
                                JobState::Interrupted
                                | JobState::PartiallySucceeded
                                | JobState::Superseded => AMBER,
                                _ => TEXT_2,
                            };
                            ui.label(
                                RichText::new(parts.join(" · "))
                                    .monospace()
                                    .size(10.0)
                                    .color(color),
                            );
                        }
                    });
                });
                match action {
                    Some(("cancel", id)) => {
                        let _ = self.jobs.cancel(&id);
                        self.persist();
                    }
                    Some(("retry", id)) => {
                        if self.jobs.retry(&id).is_ok() {
                            let kind = self.jobs.get(&id).map(|j| j.kind.clone());
                            match kind {
                                Some(JobKind::FixtureAcquire { .. }) => self.timers.push(Timer {
                                    job_id: id,
                                    due: Instant::now() + ACQUIRE_FAIL_DELAY,
                                }),
                                Some(JobKind::FixtureOcr { .. }) => {
                                    let _ = self.jobs.set_running(&id);
                                    self.timers.push(Timer {
                                        job_id: id,
                                        due: Instant::now() + OCR_PAGE_INTERVAL,
                                    });
                                }
                                _ => {}
                            }
                            self.persist();
                        }
                    }
                    Some((_, id)) => {
                        if let Ok(value) = self.jobs.mark_applied(&id) {
                            if let Ok(result) = serde_json::from_value::<SearchResult>(value) {
                                self.search = Some(result);
                                if self.shell.instrument != Instrument::Library {
                                    self.switch(Instrument::Library);
                                }
                            }
                            self.persist();
                        }
                    }
                    None => {}
                }
            });

        // Notices.
        if let Some(text) = self.notices.front().cloned() {
            egui::Area::new(egui::Id::unique("notice"))
                .fixed_pos(Pos2::new(
                    screen.center().x - 280.0,
                    screen.bottom() - 190.0,
                ))
                .show(ctx, |ui| {
                    let mut dismiss = false;
                    self.glass(true).show(ui, |ui| {
                        ui.set_width(536.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(text).size(12.0).color(TEXT));
                            if ui.button("✕").clicked() {
                                dismiss = true;
                            }
                        });
                    });
                    if dismiss {
                        self.notices.pop_front();
                    }
                });
        }
    }
}

impl App {
    fn draw_pane(&mut self, ui: &mut egui::Ui) {
        match self.shell.instrument {
            Instrument::Library => {
                ui.label(RichText::new("Library").size(20.0).color(TEXT));
                for (id, _) in &self.pending_searches {
                    ui.label(
                        RichText::new(format!("Searching… ({id}; progress not reported)"))
                            .monospace()
                            .size(11.0)
                            .color(TEXT_2),
                    );
                }
                let mut select: Option<String> = None;
                if let Some(r) = &self.search {
                    ui.horizontal(|ui| {
                        badge(ui, "⌕", &format!("scope · {}", r.scope), CYAN);
                        badge(ui, "=", &format!("mode · {}", r.mode), TEXT_2);
                        badge(
                            ui,
                            "#",
                            &format!("{} results", r.hits.len()),
                            if r.outcome == "empty" { AMBER } else { TEXT_2 },
                        );
                        badge(ui, "¶", "page 1 of 1", TEXT_2);
                    });
                    ui.label(
                        RichText::new(&r.coverage)
                            .monospace()
                            .size(11.0)
                            .color(TEXT_2),
                    );
                    if r.outcome == "empty" {
                        ui.label(RichText::new(format!("No passages contain \"{}\". This is an empty result, not a failed search.", r.query)).size(12.0).color(TEXT_2));
                    }
                    for hit in &r.hits {
                        if ui
                            .add(
                                egui::Button::new(RichText::new(&hit.text).size(13.0).color(TEXT))
                                    .frame(false)
                                    .wrap(),
                            )
                            .clicked()
                        {
                            select = Some(hit.doc_id.clone());
                        }
                        ui.label(v(&format!(
                            "{} · v{} · {}",
                            hit.doc_id, hit.version, hit.anchor
                        )));
                    }
                }
                ui.label(k("sources · 1 of 1"));
                for d in fixture::documents() {
                    if ui
                        .add(
                            egui::Button::new(RichText::new(&d.title).size(13.0).color(TEXT))
                                .frame(false),
                        )
                        .clicked()
                    {
                        select = Some(d.id.clone());
                    }
                    ui.horizontal_wrapped(|ui| {
                        badge(ui, d.origin.glyph, &d.origin.label, TEXT_2);
                        badge(ui, d.availability.glyph, &d.availability.label, GREEN);
                        ui.label(k(&format!(
                            "published {} · {}",
                            d.publication, d.extraction
                        )));
                    });
                }
                if let Some(id) = select {
                    self.select(&id);
                }
            }
            Instrument::Report => {
                let r = fixture::report_draft();
                ui.label(
                    RichText::new(format!("Report · revision {}", r.revision))
                        .size(20.0)
                        .color(TEXT),
                );
                ui.horizontal(|ui| {
                    badge(
                        ui,
                        "¶",
                        &format!("{} statements", r.statements.len()),
                        TEXT_2,
                    );
                    badge(ui, "!", &format!("{} uncited", r.uncited), AMBER);
                });
                for s in &r.statements {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(if s.citation.is_some() { "¹" } else { "!" })
                                .monospace()
                                .color(if s.citation.is_some() { CYAN } else { AMBER }),
                        );
                        ui.label(RichText::new(&s.text).size(14.0).color(TEXT));
                        if s.origin == "generated" {
                            badge(
                                ui,
                                "✦",
                                if s.reviewed {
                                    "generated · reviewed"
                                } else {
                                    "generated, unreviewed"
                                },
                                VIOLET,
                            );
                        }
                        if let Some(c) = &s.citation {
                            ui.label(v(c));
                        } else {
                            badge(ui, "!", "uncited", AMBER);
                        }
                    });
                }
            }
            Instrument::Export => {
                let x = fixture::export_preview();
                ui.label(RichText::new("Export preview").size(20.0).color(TEXT));
                ui.label(RichText::new(x.note).monospace().size(11.0).color(TEXT_2));
                for f in &x.formats {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(f.name).size(13.0).color(TEXT));
                        badge(ui, "○", f.status, TEXT_2);
                        ui.label(k(f.note));
                    });
                }
                ui.add_enabled(false, egui::Button::new("Write file"));
            }
            Instrument::Jobs => {
                ui.label(RichText::new("Jobs").size(20.0).color(TEXT));
                for j in self.jobs.jobs() {
                    ui.label(v(&format!(
                        "{} · {:?} · attempt {}/{} · {}",
                        j.name, j.state, j.attempt, j.max_attempts, j.id
                    )));
                }
            }
            Instrument::Settings => {
                ui.label(RichText::new("Settings").size(20.0).color(TEXT));
                ui.label(RichText::new("Each setting changes how the workspace is drawn. None of them changes data, selection, counts, or analysis results.").monospace().size(11.0).color(TEXT_2));
                let s = self.shell.settings.clone();
                let mut changes = Vec::new();
                let mut rm = s.reduced_motion;
                if ui.checkbox(&mut rm, "Reduced motion").changed() {
                    changes.push(SettingChange::ReducedMotion(rm));
                }
                let mut rt = s.reduced_transparency;
                if ui
                    .checkbox(&mut rt, "Reduced transparency (opaque surfaces)")
                    .changed()
                {
                    changes.push(SettingChange::ReducedTransparency(rt));
                }
                let mut ce = s.clean_evidence;
                if ui
                    .checkbox(&mut ce, "Clean evidence (Ctrl+Shift+E)")
                    .changed()
                {
                    changes.push(SettingChange::CleanEvidence(ce));
                }
                let mut fr = s.focused_reading;
                if ui
                    .checkbox(&mut fr, "Focused reading (Ctrl+Shift+F)")
                    .changed()
                {
                    changes.push(SettingChange::FocusedReading(fr));
                }
                let mut snd = s.sound;
                if ui
                    .checkbox(&mut snd, "Sound (off by default; nothing plays)")
                    .changed()
                {
                    changes.push(SettingChange::Sound(snd));
                }
                for c in changes {
                    self.setting(c);
                }
                ui.label(k(&format!(
                    "State file: {}",
                    self.store_path
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "— not persisted".to_owned())
                )));
                ui.label(k(&format!("Restore: {:?}", self.restore)));
            }
            _ => {}
        }
    }
}

fn draw_selected(ui: &mut egui::Ui, o: &ObjectDetail, select: &mut Option<String>) {
    match o {
        ObjectDetail::Account {
            label,
            origin,
            anchor,
            assessment,
            review,
            conflicts,
            source_record,
            bearing,
            times,
            ..
        } => {
            ui.horizontal(|ui| {
                ui.label(k("object"));
                ui.label(v(label));
            });
            ui.horizontal(|ui| {
                ui.label(k("origin"));
                badge(ui, origin.glyph, &origin.label, TEXT_2);
            });
            ui.horizontal(|ui| {
                ui.label(k("anchor"));
                if ui.button(v(anchor)).clicked() {
                    *select = Some(fixture::DOC_ID.to_owned());
                }
            });
            ui.horizontal(|ui| {
                ui.label(k("assessment"));
                badge(ui, assessment.glyph, &assessment.label, TEXT_2);
                badge(ui, review.glyph, &review.label, TEXT_2);
            });
            ui.horizontal(|ui| {
                ui.label(k("conflicts"));
                if conflicts.is_empty() {
                    ui.label(v("none"));
                }
                for c in conflicts {
                    badge(ui, c.glyph, &c.label, RED);
                }
            });
            ui.label(k("reported direction of travel"));
            let (rect, _) = ui.allocate_exact_size(Vec2::new(280.0, 110.0), Sense::hover());
            let p = ui.painter();
            let c = Pos2::new(rect.left() + 55.0, rect.center().y);
            p.circle_stroke(c, 44.0, Stroke::new(1.0, Color32::from_white_alpha(30)));
            p.circle_stroke(c, 28.0, Stroke::new(1.0, Color32::from_white_alpha(20)));
            p.text(
                Pos2::new(c.x, c.y - 52.0),
                Align2::CENTER_CENTER,
                "N",
                FontId::monospace(9.0),
                TEXT_2,
            );
            match bearing {
                Some(b) => {
                    let rad = |d: f64| ((d - 90.0).to_radians()) as f32;
                    let arc =
                        |d: f64| Pos2::new(c.x + 42.0 * rad(d).cos(), c.y + 42.0 * rad(d).sin());
                    let mut pts = vec![c];
                    let mut d = b.degrees - b.plus_minus_degrees;
                    while d <= b.degrees + b.plus_minus_degrees {
                        pts.push(arc(d));
                        d += 2.0;
                    }
                    p.add(Shape::convex_polygon(
                        pts,
                        CYAN.gamma_multiply(0.18),
                        Stroke::new(1.0, CYAN),
                    ));
                    p.line_segment([c, arc(b.degrees)], Stroke::new(1.5, CYAN));
                    p.text(
                        Pos2::new(rect.left() + 120.0, c.y - 6.0),
                        Align2::LEFT_CENTER,
                        b.text,
                        FontId::monospace(10.0),
                        TEXT,
                    );
                    p.text(
                        Pos2::new(rect.left() + 120.0, c.y + 10.0),
                        Align2::LEFT_CENTER,
                        format!("{}° ± {}° (reported)", b.degrees, b.plus_minus_degrees),
                        FontId::monospace(9.0),
                        TEXT_2,
                    );
                }
                None => {
                    p.extend(Shape::dashed_line(
                        &circle_points(c, 36.0),
                        Stroke::new(1.0, AMBER),
                        2.0,
                        5.0,
                    ));
                    p.text(
                        Pos2::new(rect.left() + 120.0, c.y - 6.0),
                        Align2::LEFT_CENTER,
                        "no bearing reported",
                        FontId::monospace(10.0),
                        TEXT,
                    );
                    p.text(
                        Pos2::new(rect.left() + 120.0, c.y + 10.0),
                        Align2::LEFT_CENTER,
                        "absence drawn as dashed ring",
                        FontId::monospace(9.0),
                        TEXT_2,
                    );
                }
            }
            ui.label(k("coverage"));
            ui.horizontal(|ui| {
                for (name, cov) in [
                    ("event", times.event),
                    ("received", times.received),
                    ("position", times.position),
                ] {
                    ui.vertical(|ui| {
                        ui.label(k(name));
                        let (r, _) = ui.allocate_exact_size(Vec2::new(60.0, 5.0), Sense::hover());
                        match cov {
                            fixture::Coverage::Approximate => {
                                ui.painter().rect_filled(r, 2.0, CYAN);
                            }
                            fixture::Coverage::Recorded => {
                                ui.painter().rect_filled(r, 2.0, GREEN);
                            }
                            fixture::Coverage::RegionOnly => {
                                ui.painter().rect_stroke(
                                    r,
                                    2.0,
                                    Stroke::new(1.0, AMBER),
                                    egui::StrokeKind::Inside,
                                );
                            }
                            fixture::Coverage::NotRecorded => {
                                ui.painter().rect_stroke(
                                    r,
                                    2.0,
                                    Stroke::new(1.0, Color32::from_white_alpha(60)),
                                    egui::StrokeKind::Inside,
                                );
                            }
                        }
                        ui.label(
                            RichText::new(format!("{cov:?}"))
                                .monospace()
                                .size(9.0)
                                .color(TEXT_2),
                        );
                    });
                }
            });
            ui.label(k("source record"));
            for row in source_record {
                ui.horizontal_wrapped(|ui| {
                    ui.label(k(row.key));
                    ui.label(if row.mono {
                        v(&row.value)
                    } else {
                        RichText::new(&row.value).size(12.0).color(TEXT)
                    });
                });
            }
        }
        ObjectDetail::Source {
            title,
            origin,
            availability,
            health,
            source_record,
            pages,
            unreadable_pages,
            ..
        } => {
            ui.label(RichText::new(title).size(12.0).color(TEXT));
            ui.horizontal(|ui| {
                badge(ui, origin.glyph, &origin.label, TEXT_2);
                badge(ui, availability.glyph, &availability.label, GREEN);
                badge(ui, health.glyph, &health.label, GREEN);
            });
            ui.label(k(&format!(
                "extraction coverage · {} of {} pages",
                pages - u32::try_from(unreadable_pages.len()).unwrap_or(0),
                pages
            )));
            ui.horizontal(|ui| {
                for p in 1..=*pages {
                    let bad = unreadable_pages.contains(&p);
                    let (r, _) = ui.allocate_exact_size(Vec2::new(22.0, 12.0), Sense::hover());
                    if bad {
                        ui.painter().rect_stroke(
                            r,
                            2.0,
                            Stroke::new(1.0, AMBER),
                            egui::StrokeKind::Inside,
                        );
                    } else {
                        ui.painter().rect_filled(r, 2.0, GREEN);
                    }
                }
            });
            ui.label(k("source record"));
            for row in source_record {
                ui.horizontal_wrapped(|ui| {
                    ui.label(k(row.key));
                    ui.label(if row.mono {
                        v(&row.value)
                    } else {
                        RichText::new(&row.value).size(12.0).color(TEXT)
                    });
                });
            }
        }
        ObjectDetail::Hypothesis {
            id,
            version,
            text,
            origin,
            supporting,
            contradicting,
            discriminating,
        } => {
            ui.label(v(&format!("{id} · v{version}")));
            badge(ui, origin.glyph, &origin.label, AMBER);
            ui.label(RichText::new(text).size(14.0).color(TEXT));
            ui.label(k(&format!("supporting · {}", supporting.len())));
            for s in supporting {
                ui.label(v(s));
            }
            ui.label(k(&format!("contradicting · {}", contradicting.len())));
            for s in contradicting {
                ui.label(v(s));
            }
            ui.label(k("evidence that would discriminate"));
            ui.label(RichText::new(discriminating).size(12.0).color(TEXT));
        }
        ObjectDetail::Claim {
            id,
            version,
            proposition,
            claimant,
            anchor,
            assessment,
            review,
            ..
        } => {
            ui.label(RichText::new(proposition).size(14.0).color(TEXT));
            ui.label(v(&format!("{claimant} · {anchor} · {id} v{version}")));
            ui.horizontal(|ui| {
                badge(ui, assessment.glyph, &assessment.label, TEXT_2);
                badge(ui, review.glyph, &review.label, TEXT_2);
            });
            ui.label(k("no probability is shown for a claim"));
        }
    }
}

fn circle_points(c: Pos2, r: f32) -> Vec<Pos2> {
    (0..=64)
        .map(|i| {
            let a = i as f32 / 64.0 * std::f32::consts::TAU;
            Pos2::new(c.x + r * a.cos(), c.y + r * a.sin())
        })
        .collect()
}

fn draw_timeline(p: &egui::Painter, rect: Rect, shell: &ShellState) {
    let (a, b) = (
        shell.context.interval.start_min,
        shell.context.interval.end_min,
    );
    let x = |m: f32| rect.left() + (m - a as f32) / (b - a) as f32 * rect.width();
    let y = rect.top() + 36.0;
    p.line_segment(
        [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
        Stroke::new(1.0, Color32::from_white_alpha(50)),
    );
    let mut m = a;
    while m <= b {
        p.line_segment(
            [
                Pos2::new(x(m as f32), y - 4.0),
                Pos2::new(x(m as f32), y + 4.0),
            ],
            Stroke::new(1.0, Color32::from_white_alpha(50)),
        );
        p.text(
            Pos2::new(x(m as f32).min(rect.right() - 36.0) + 2.0, y + 16.0),
            Align2::LEFT_CENTER,
            hhmm(m),
            FontId::monospace(10.0),
            TEXT_2,
        );
        m += 60;
    }
    let mut absent = Vec::new();
    for mk in fixture::timeline_marks() {
        match mk.range_min {
            Some((m0, m1)) => {
                let r = Rect::from_min_max(
                    Pos2::new(x(m0 as f32), y - 6.0),
                    Pos2::new(x(m1 as f32), y + 6.0),
                );
                p.rect(
                    r,
                    3.0,
                    CYAN.gamma_multiply(0.18),
                    Stroke::new(1.0, CYAN.gamma_multiply(0.6)),
                    egui::StrokeKind::Inside,
                );
                p.text(
                    Pos2::new(r.center().x, y - 16.0),
                    Align2::CENTER_CENTER,
                    format!(
                        "{} {} · {} {}",
                        mk.glyph,
                        mk.label,
                        mk.kind.to_uppercase(),
                        mk.note.replace("event ", "").to_uppercase()
                    ),
                    FontId::monospace(10.0),
                    CYAN,
                );
            }
            None => absent.push(mk.label),
        }
    }
    if !absent.is_empty() {
        p.text(
            Pos2::new(rect.right(), y - 16.0),
            Align2::RIGHT_CENTER,
            format!("{} · NO EVENT TIME", absent.join(", ")),
            FontId::monospace(10.0),
            TEXT_3,
        );
    }
    let cx = x(shell.context.clock_min as f32);
    p.line_segment(
        [Pos2::new(cx, rect.top() + 6.0), Pos2::new(cx, y + 10.0)],
        Stroke::new(1.5, CYAN),
    );
}

fn draw_geometry(
    ui: &mut egui::Ui,
    g: &CaseGeometry,
    free: Rect,
    shell: &ShellState,
    clean: bool,
    clicked: &mut Option<String>,
) {
    let k_px = (free.width().min(free.height()) * 0.46) / g.region_radius_km as f32;
    let c = free.center();
    let p = |km: &Km| Pos2::new(c.x + km.east as f32 * k_px, c.y - km.north as f32 * k_px);
    let painter = ui.painter().clone();
    let sel = shell
        .context
        .selection
        .as_ref()
        .map(|s| s.object_id.as_str());

    if !clean {
        for r in &g.ring_km {
            let edge = (r - g.region_radius_km).abs() < 1e-9;
            let pts = circle_points(c, *r as f32 * k_px);
            if edge {
                painter.extend(Shape::dashed_line(
                    &pts,
                    Stroke::new(1.0, CYAN.gamma_multiply(0.3)),
                    3.0,
                    6.0,
                ));
            } else {
                painter.circle_stroke(
                    c,
                    *r as f32 * k_px,
                    Stroke::new(1.0, CYAN.gamma_multiply(0.14)),
                );
            }
            painter.text(
                Pos2::new(c.x - *r as f32 * k_px - 6.0, c.y - 8.0),
                Align2::RIGHT_CENTER,
                format!("{r} KM"),
                FontId::monospace(10.0),
                TEXT_3,
            );
        }
        painter.text(
            Pos2::new(
                c.x + g.region_radius_km as f32 * k_px * 0.75,
                c.y + g.region_radius_km as f32 * k_px * 0.78,
            ),
            Align2::LEFT_CENTER,
            format!("≈ {} KM · EDGE OF REPORTED REGION", g.region_radius_km),
            FontId::monospace(10.0),
            TEXT_3,
        );
        for (pos, label) in [
            (Pos2::new(c.x + 14.0, free.top() + 12.0), "N"),
            (Pos2::new(c.x + 14.0, free.bottom() - 6.0), "S"),
            (Pos2::new(free.left() + 54.0, c.y - 10.0), "W"),
            (Pos2::new(free.right() - 54.0, c.y - 10.0), "E"),
        ] {
            painter.text(
                pos,
                Align2::CENTER_CENTER,
                label,
                FontId::monospace(11.0),
                TEXT_2,
            );
        }
        painter.text(
            Pos2::new(free.left() + 12.0, free.bottom() - 10.0),
            Align2::LEFT_CENTER,
            format!(
                "REPORTED GEOMETRY · BASEMAP {} · {}",
                g.basemap.to_uppercase(),
                g.scale_note.to_uppercase()
            ),
            FontId::monospace(10.0),
            TEXT_3,
        );
    }

    let ridge: Vec<Pos2> = g.ridge.iter().map(p).collect();
    painter.extend(Shape::dashed_line(
        &ridge,
        Stroke::new(1.5, Color32::from_white_alpha(120)),
        6.0,
        5.0,
    ));
    if !clean && let Some(end) = ridge.last() {
        painter.text(
            Pos2::new(end.x + 8.0, end.y - 8.0),
            Align2::LEFT_CENTER,
            g.ridge_label.to_uppercase(),
            FontId::monospace(10.0),
            TEXT_2,
        );
    }
    for r in &g.regions {
        let pts: Vec<Pos2> = r.polygon.iter().map(p).collect();
        let selected = sel == Some(r.account_id.as_str());
        // egui has no pattern fill: a translucent amber fill stands in for the hatch.
        painter.add(Shape::convex_polygon(
            pts.clone(),
            AMBER.gamma_multiply(0.08),
            Stroke::NONE,
        ));
        let mut closed = pts.clone();
        if let Some(first) = pts.first() {
            closed.push(*first);
        }
        painter.extend(Shape::dashed_line(
            &closed,
            Stroke::new(if selected { 1.5 } else { 1.0 }, AMBER.gamma_multiply(0.6)),
            4.0,
            4.0,
        ));
        let bounds = Rect::from_points(&pts);
        if ui
            .interact(
                bounds,
                egui::Id::unique(("region", &r.account_id)),
                Sense::click(),
            )
            .clicked()
        {
            *clicked = Some(r.account_id.clone());
        }
        if !clean {
            painter.text(
                Pos2::new(bounds.right() - 160.0, bounds.bottom() + 16.0),
                Align2::LEFT_CENTER,
                r.label.to_uppercase(),
                FontId::monospace(10.0),
                AMBER,
            );
            painter.text(
                Pos2::new(bounds.right() - 160.0, bounds.bottom() + 30.0),
                Align2::LEFT_CENTER,
                r.note.to_uppercase(),
                FontId::monospace(10.0),
                TEXT_3,
            );
        }
    }
    for w in &g.wedges {
        painter.add(Shape::convex_polygon(
            vec![p(&w.apex), p(&w.left), p(&w.right)],
            CYAN.gamma_multiply(0.06),
            Stroke::new(1.0, CYAN.gamma_multiply(0.35)),
        ));
    }
    for t in &g.tracks {
        let (a, b) = (p(&t.from), p(&t.to));
        let selected = sel == Some(t.account_id.as_str());
        painter.extend(Shape::dashed_line(
            &[a, b],
            Stroke::new(if selected { 2.0 } else { 1.5 }, CYAN),
            10.0,
            6.0,
        ));
        painter.arrow(
            b - (b - a).normalized() * 12.0,
            (b - a).normalized() * 12.0,
            Stroke::new(1.5, CYAN),
        );
        if ui
            .interact(
                Rect::from_two_pos(a, b).expand(10.0),
                egui::Id::unique(("track", &t.account_id)),
                Sense::click(),
            )
            .clicked()
        {
            *clicked = Some(t.account_id.clone());
        }
        if !clean {
            painter.text(
                Pos2::new(a.x - 12.0, a.y - 16.0),
                Align2::LEFT_CENTER,
                t.label.to_uppercase(),
                FontId::monospace(10.0),
                CYAN,
            );
            painter.text(
                Pos2::new(a.x - 12.0, a.y - 3.0),
                Align2::LEFT_CENTER,
                t.note.to_uppercase(),
                FontId::monospace(10.0),
                TEXT_3,
            );
        }
    }
    let mut unplotted = Vec::new();
    for o in &g.observers {
        match &o.position {
            Some(pos) => {
                let q = p(pos);
                painter.circle_filled(q, 5.0, CYAN);
                painter.circle_stroke(q, 11.0, Stroke::new(1.0, CYAN));
                if ui
                    .interact(
                        Rect::from_center_size(q, Vec2::splat(28.0)),
                        egui::Id::unique(("observer", &o.account_id)),
                        Sense::click(),
                    )
                    .clicked()
                {
                    *clicked = Some(o.account_id.clone());
                }
                if !clean {
                    painter.text(
                        Pos2::new(q.x + 16.0, q.y - 7.0),
                        Align2::LEFT_CENTER,
                        o.label,
                        FontId::monospace(11.0),
                        TEXT,
                    );
                    painter.text(
                        Pos2::new(q.x + 16.0, q.y + 7.0),
                        Align2::LEFT_CENTER,
                        o.position_note.to_uppercase(),
                        FontId::monospace(10.0),
                        TEXT_2,
                    );
                }
            }
            None => unplotted.push(o),
        }
    }
    painter.line_segment(
        [Pos2::new(c.x - 8.0, c.y), Pos2::new(c.x + 8.0, c.y)],
        Stroke::new(1.0, CYAN),
    );
    painter.line_segment(
        [Pos2::new(c.x, c.y - 8.0), Pos2::new(c.x, c.y + 8.0)],
        Stroke::new(1.0, CYAN),
    );
    // Unknown positions are listed outside the geometry, never placed at an invented point (design §7.6).
    if !unplotted.is_empty() {
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(
            Pos2::new(free.left() + 278.0, free.top()),
            Vec2::new(360.0, 40.0),
        )));
        child.horizontal(|ui| {
            ui.label(k("not plotted"));
            for o in unplotted {
                if ui
                    .button(k(&format!("{} · {}", o.label, o.position_note)))
                    .clicked()
                {
                    *clicked = Some(o.account_id.clone());
                }
            }
        });
    }
    if let (Some(id), false) = (sel, clean) {
        let mut pts = Vec::new();
        for t in &g.tracks {
            if t.account_id == id {
                pts.push(p(&t.from));
                pts.push(p(&t.to));
            }
        }
        for r in &g.regions {
            if r.account_id == id {
                pts.extend(r.polygon.iter().map(p));
            }
        }
        for o in &g.observers {
            if o.account_id == id
                && let Some(pos) = &o.position
            {
                pts.push(p(pos));
            }
        }
        if !pts.is_empty() {
            let b = Rect::from_points(&pts).expand(20.0);
            let s = Stroke::new(1.2, CYAN);
            for (corner, dx, dy) in [
                (b.left_top(), 14.0, 14.0),
                (b.right_top(), -14.0, 14.0),
                (b.left_bottom(), 14.0, -14.0),
                (b.right_bottom(), -14.0, -14.0),
            ] {
                painter.line_segment([corner, Pos2::new(corner.x + dx, corner.y)], s);
                painter.line_segment([corner, Pos2::new(corner.x, corner.y + dy)], s);
            }
        }
    }
}
