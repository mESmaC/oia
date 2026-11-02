//! Shell context held by the application layer (design §4): instrument, shared
//! context, route stack, per-instrument local filters, and effect settings.
//!
//! Everything here is renderer-independent and has no window or IPC
//! dependency, so it can be exercised in plain tests. Settings change how the
//! presentation layer draws; they never touch fixture or domain values.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// Workspace instruments in rail order (design §2). `Ctrl+1..9` maps onto this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Instrument {
    Case,
    Library,
    News,
    Report,
    Export,
    Globe,
    Sky,
    Media,
    Jobs,
    Settings,
}

impl Instrument {
    /// Rail order.
    pub const ALL: [Instrument; 10] = [
        Instrument::Case,
        Instrument::Library,
        Instrument::News,
        Instrument::Report,
        Instrument::Export,
        Instrument::Globe,
        Instrument::Sky,
        Instrument::Media,
        Instrument::Jobs,
        Instrument::Settings,
    ];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Case => "Case",
            Self::Library => "Library",
            Self::News => "News",
            Self::Report => "Report",
            Self::Export => "Export",
            Self::Globe => "Globe",
            Self::Sky => "Sky",
            Self::Media => "Media",
            Self::Jobs => "Jobs",
            Self::Settings => "Settings",
        }
    }

    /// Text glyph placeholder (design §6 notes the icon set is an open item).
    #[must_use]
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Case => "◎",
            Self::Library => "▣",
            Self::News => "≋",
            Self::Report => "¶",
            Self::Export => "⇪",
            Self::Globe => "◍",
            Self::Sky => "✦",
            Self::Media => "▦",
            Self::Jobs => "⟳",
            Self::Settings => "⚙",
        }
    }

    /// Why an instrument cannot be opened in this build, if it cannot.
    /// Unavailable instruments stay listed with the reason (design §2).
    #[must_use]
    pub fn unavailable_reason(self) -> Option<&'static str> {
        match self {
            Self::Case
            | Self::Library
            | Self::Report
            | Self::Export
            | Self::Jobs
            | Self::Settings => None,
            Self::News | Self::Globe | Self::Sky | Self::Media => Some("Not in this build"),
        }
    }

    /// Maps a 1-based rail position to an instrument.
    #[must_use]
    pub fn from_rail_position(position: u8) -> Option<Instrument> {
        position
            .checked_sub(1)
            .and_then(|i| Self::ALL.get(usize::from(i)).copied())
    }
}

/// Rail entry as shown to the presentation layer.
#[derive(Debug, Clone, Serialize)]
pub struct InstrumentInfo {
    pub id: Instrument,
    pub label: &'static str,
    pub glyph: &'static str,
    pub available: bool,
    pub reason: Option<&'static str>,
}

impl InstrumentInfo {
    #[must_use]
    pub fn all() -> Vec<InstrumentInfo> {
        Instrument::ALL
            .iter()
            .map(|&id| InstrumentInfo {
                id,
                label: id.label(),
                glyph: id.glyph(),
                available: id.unavailable_reason().is_none(),
                reason: id.unavailable_reason(),
            })
            .collect()
    }
}

/// Timeline mode badge (design §6.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClockMode {
    Live,
    Historical,
    Replay,
    Simulated,
}

/// A local-date interval expressed in minutes after local midnight.
///
/// The spike does not convert between time references; it preserves the
/// reported local date, zone name, and reference label as entered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalInterval {
    pub date: String,
    pub start_min: u32,
    pub end_min: u32,
    pub zone: String,
    pub reference: String,
}

/// Approximate area summary (design §5.1: an unknown area stays unknown).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AreaSummary {
    pub label: String,
    pub approximate: bool,
    pub extent_km: Option<f64>,
}

/// Current object selection by id and version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    pub object_id: String,
    pub version: u32,
}

/// The shared context record (design §4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedContext {
    pub case_id: String,
    pub case_title: String,
    pub area: Option<AreaSummary>,
    pub interval: LocalInterval,
    pub clock_min: u32,
    pub clock_mode: ClockMode,
    pub selection: Option<Selection>,
    /// Interval the timeline highlights for the selection, in minutes.
    pub timeline_focus: Option<(u32, u32)>,
    /// A filter promoted from a local filter into the shared context.
    pub shared_filter: Option<String>,
}

/// One route-stack entry: enough to restore the previous view as one step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteEntry {
    pub instrument: Instrument,
    pub selection: Option<Selection>,
    pub local_filters: BTreeMap<Instrument, String>,
    pub clock_min: u32,
    pub timeline_focus: Option<(u32, u32)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectsQuality {
    Low,
    Standard,
    High,
}

/// Effect and layout settings (design §8.3). None of these reach data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub reduced_motion: bool,
    pub reduced_transparency: bool,
    pub effects: EffectsQuality,
    pub clean_evidence: bool,
    pub focused_reading: bool,
    pub sound: bool,
    pub reading_text_px: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            reduced_motion: false,
            reduced_transparency: false,
            effects: EffectsQuality::Standard,
            clean_evidence: false,
            focused_reading: false,
            sound: false,
            reading_text_px: 16,
        }
    }
}

/// Minimum and maximum reading text size in pixels (design §7.3).
pub const READING_TEXT_PX_RANGE: (u8, u8) = (13, 24);

/// A single setting change, typed so the presentation layer cannot invent keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "setting", content = "value", rename_all = "snake_case")]
pub enum SettingChange {
    ReducedMotion(bool),
    ReducedTransparency(bool),
    Effects(EffectsQuality),
    CleanEvidence(bool),
    FocusedReading(bool),
    Sound(bool),
    ReadingTextPx(u8),
}

/// Upper bound on retained route entries.
pub const ROUTE_STACK_LIMIT: usize = 200;

/// Persisted schema version of [`ShellState`].
pub const SHELL_SCHEMA: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShellState {
    pub schema: u32,
    pub instrument: Instrument,
    pub context: SharedContext,
    pub route_stack: Vec<RouteEntry>,
    pub local_filters: BTreeMap<Instrument, String>,
    pub settings: Settings,
    /// Monotonic query sequence; a search result applies only if its sequence
    /// is still current (CASE-04).
    pub query_seq: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellError {
    InstrumentUnavailable(Instrument),
    ClockOutsideInterval {
        requested: u32,
        start: u32,
        end: u32,
    },
    NothingToRestore,
    NoLocalFilter(Instrument),
    ReadingTextOutOfRange(u8),
}

impl fmt::Display for ShellError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InstrumentUnavailable(i) => {
                write!(f, "{} is not available in this build", i.label())
            }
            Self::ClockOutsideInterval {
                requested,
                start,
                end,
            } => write!(
                f,
                "clock {requested} min is outside the interval {start}..={end} min"
            ),
            Self::NothingToRestore => write!(f, "route stack is empty"),
            Self::NoLocalFilter(i) => write!(f, "{} has no local filter", i.label()),
            Self::ReadingTextOutOfRange(px) => write!(
                f,
                "reading text {px} px is outside {}..={} px",
                READING_TEXT_PX_RANGE.0, READING_TEXT_PX_RANGE.1
            ),
        }
    }
}

impl std::error::Error for ShellError {}

impl ShellState {
    #[must_use]
    pub fn new(context: SharedContext) -> Self {
        Self {
            schema: SHELL_SCHEMA,
            instrument: Instrument::Case,
            context,
            route_stack: Vec::new(),
            local_filters: BTreeMap::new(),
            settings: Settings::default(),
            query_seq: 0,
        }
    }

    fn push_route(&mut self) {
        if self.route_stack.len() >= ROUTE_STACK_LIMIT {
            self.route_stack.remove(0);
        }
        self.route_stack.push(RouteEntry {
            instrument: self.instrument,
            selection: self.context.selection.clone(),
            local_filters: self.local_filters.clone(),
            clock_min: self.context.clock_min,
            timeline_focus: self.context.timeline_focus,
        });
    }

    /// Switches instruments, keeping the shared context (design §4 rule 1).
    pub fn switch_instrument(&mut self, next: Instrument) -> Result<(), ShellError> {
        if let Some(_reason) = next.unavailable_reason() {
            return Err(ShellError::InstrumentUnavailable(next));
        }
        if next != self.instrument {
            self.push_route();
            self.instrument = next;
        }
        Ok(())
    }

    /// Coordinated selection (design §4 rule 2): one route entry, selection
    /// and timeline focus updated together.
    pub fn select(&mut self, selection: Selection, timeline_focus: Option<(u32, u32)>) {
        self.push_route();
        self.context.selection = Some(selection);
        self.context.timeline_focus = timeline_focus;
    }

    pub fn clear_selection(&mut self) {
        if self.context.selection.is_some() {
            self.push_route();
            self.context.selection = None;
            self.context.timeline_focus = None;
        }
    }

    /// Restores exactly the previous entry (design §4 rule 3).
    pub fn route_back(&mut self) -> Result<RouteEntry, ShellError> {
        let entry = self.route_stack.pop().ok_or(ShellError::NothingToRestore)?;
        self.instrument = entry.instrument;
        self.context.selection = entry.selection.clone();
        self.local_filters = entry.local_filters.clone();
        self.context.clock_min = entry.clock_min;
        self.context.timeline_focus = entry.timeline_focus;
        Ok(entry)
    }

    /// Sets or clears an instrument-local filter. Local filters never become
    /// shared context by themselves (design §4 rule 5).
    pub fn set_local_filter(&mut self, instrument: Instrument, filter: Option<String>) {
        match filter
            .map(|f| f.trim().to_owned())
            .filter(|f| !f.is_empty())
        {
            Some(f) => {
                self.local_filters.insert(instrument, f);
            }
            None => {
                self.local_filters.remove(&instrument);
            }
        }
    }

    /// Explicitly promotes a local filter into the shared context.
    pub fn promote_local_filter(&mut self, instrument: Instrument) -> Result<String, ShellError> {
        let filter = self
            .local_filters
            .remove(&instrument)
            .ok_or(ShellError::NoLocalFilter(instrument))?;
        self.context.shared_filter = Some(filter.clone());
        Ok(filter)
    }

    #[must_use]
    pub fn divergent_local_filters(&self) -> Vec<Instrument> {
        self.local_filters
            .iter()
            .filter(|(_, local)| self.context.shared_filter.as_deref() != Some(local.as_str()))
            .map(|(instrument, _)| *instrument)
            .collect()
    }

    /// Moves the clock by `delta_min`, clamped to the interval bounds.
    pub fn scrub_clock(&mut self, delta_min: i32) -> u32 {
        let start = i64::from(self.context.interval.start_min);
        let end = i64::from(self.context.interval.end_min);
        let next = (i64::from(self.context.clock_min) + i64::from(delta_min)).clamp(start, end);
        self.context.clock_min = u32::try_from(next).unwrap_or(self.context.interval.start_min);
        self.context.clock_min
    }

    pub fn set_clock(&mut self, clock_min: u32) -> Result<(), ShellError> {
        let (start, end) = (
            self.context.interval.start_min,
            self.context.interval.end_min,
        );
        if clock_min < start || clock_min > end {
            return Err(ShellError::ClockOutsideInterval {
                requested: clock_min,
                start,
                end,
            });
        }
        self.context.clock_min = clock_min;
        Ok(())
    }

    pub fn apply_setting(&mut self, change: SettingChange) -> Result<(), ShellError> {
        match change {
            SettingChange::ReducedMotion(v) => self.settings.reduced_motion = v,
            SettingChange::ReducedTransparency(v) => self.settings.reduced_transparency = v,
            SettingChange::Effects(v) => self.settings.effects = v,
            SettingChange::CleanEvidence(v) => self.settings.clean_evidence = v,
            SettingChange::FocusedReading(v) => self.settings.focused_reading = v,
            SettingChange::Sound(v) => self.settings.sound = v,
            SettingChange::ReadingTextPx(px) => {
                if px < READING_TEXT_PX_RANGE.0 || px > READING_TEXT_PX_RANGE.1 {
                    return Err(ShellError::ReadingTextOutOfRange(px));
                }
                self.settings.reading_text_px = px;
            }
        }
        Ok(())
    }

    /// Advances and returns the query sequence for a new search.
    pub fn next_query_seq(&mut self) -> u64 {
        self.query_seq = self.query_seq.wrapping_add(1);
        self.query_seq
    }
}
