//! `oia-app` — Renderer-independent workspace shell configuration (`CASE-01`–`CASE-03`,
//! `UX-01`–`UX-05`) and localhost transport guard (`docs/architecture.md` §9) for `#9`.
//!
//! Durable jobs, command/query execution, and role-based case service wiring follow in `#14`.

use std::collections::BTreeMap;
use std::fmt;

use oia_domain::OpaqueId;

/// Errors returned by the `#9` workspace shell and localhost transport contracts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppError {
    /// Session token did not meet the minimum length requirement.
    WeakSessionToken,
    /// Provided session token did not match the expected token.
    UnauthenticatedTransport,
    /// Cross-origin request was rejected (`Origin` not in `allowed_origins`).
    CrossOriginRejected(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WeakSessionToken => {
                write!(f, "localhost session token must be at least 16 bytes")
            }
            Self::UnauthenticatedTransport => {
                write!(f, "unauthenticated localhost transport request")
            }
            Self::CrossOriginRejected(origin) => {
                write!(f, "rejected disallowed cross-origin header: {origin}")
            }
        }
    }
}

impl std::error::Error for AppError {}

/// Semantic accent token in the military-glass visual system (`UX-01`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticAccent {
    /// Verified / operational / observed state (`#2ea043` terminal green).
    TerminalGreen,
    /// Active selection / informational / analytical readout (`#58a6ff` cyan).
    InstrumentationCyan,
    /// Stale, partial, needs-reassessment, or caution state (`#d29922` amber).
    CautionAmber,
}

/// Non-color-dependent mode badge distinguishing `Live`, `Historical`, `Simulated`, and
/// `Generated` views (`UX-03`, `A02`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataModeBadge {
    /// Live stream or telemetry reception.
    Live,
    /// Point-in-time historical reconstruction.
    Historical,
    /// Controlled synthetic simulation fixture.
    Simulated,
    /// AI-drafted, predictive, or illustrative generated material.
    Generated,
}

impl DataModeBadge {
    /// Returns the non-color text badge and glyph required by `UX-03` and `A02`.
    #[must_use]
    pub const fn non_color_label(self) -> &'static str {
        match self {
            Self::Live => "[LIVE ●]",
            Self::Historical => "[HIST ◷]",
            Self::Simulated => "[SIM ◇]",
            Self::Generated => "[GEN ✦]",
        }
    }

    /// Returns the associated semantic accent.
    #[must_use]
    pub const fn accent(self) -> SemanticAccent {
        match self {
            Self::Live => SemanticAccent::TerminalGreen,
            Self::Historical => SemanticAccent::InstrumentationCyan,
            Self::Simulated | Self::Generated => SemanticAccent::CautionAmber,
        }
    }
}

/// Workspace reading layout mode (`UX-04`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadingMode {
    /// Full multi-pane workspace with canvas, timeline, and inspector.
    StandardWorkspace,
    /// Clean evidence view with chrome minimized over imagery/media.
    CleanEvidence,
    /// Focused document/report reading layout.
    FocusedReading,
}

/// Visual system and accessibility configuration (`UX-01`, `UX-04`, `UX-05`, `A02`).
///
/// Routine sound defaults to `false` (`UX-04`). Changing any visual/accessibility setting
/// never alters scientific calculations or persisted domain records (`UX-05`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualSystemConfig {
    /// Dark canvas hex token (`#090d12`).
    pub canvas_hex: &'static str,
    /// Smoked glass surface hex token (`#121a24`).
    pub glass_surface_hex: &'static str,
    /// Reduced motion preference (`UX-04`).
    pub reduced_motion: bool,
    /// Reduced glass transparency / blur preference (`UX-04`, `UX-05`).
    pub reduced_transparency: bool,
    /// Routine UI sound effects (`false` by default per `UX-04`).
    pub routine_sound_enabled: bool,
    /// Active reading/layout mode.
    pub reading_mode: ReadingMode,
    /// Visible keyboard focus ring enabled (`true` by default per `UX-04`).
    pub visible_keyboard_focus: bool,
}

impl Default for VisualSystemConfig {
    fn default() -> Self {
        Self {
            canvas_hex: "#090d12",
            glass_surface_hex: "#121a24",
            reduced_motion: false,
            reduced_transparency: false,
            routine_sound_enabled: false,
            reading_mode: ReadingMode::StandardWorkspace,
            visible_keyboard_focus: true,
        }
    }
}

/// Workspace views sharing context across an investigation (`CASE-02`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WorkspaceViewKind {
    /// 3D/2D geographic globe and event mosaic.
    Globe,
    /// Local sky and solar-system ephemeris view.
    Sky,
    /// Public document reader and citation inspector.
    Documents,
    /// Forensic image/video/audio workbench.
    Media,
    /// Reasoning, link-analysis, and lineage graphs.
    Graphs,
    /// Market charts and event studies.
    MarketCharts,
    /// Cited report editor and bundle exporter.
    Report,
}

/// Shared and per-view workspace navigation state (`CASE-02`, `UX-02`, `A01`, `A02`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceShellState {
    /// Visual and accessibility settings.
    pub visual: VisualSystemConfig,
    /// Active view.
    pub active_view: WorkspaceViewKind,
    /// Active data mode badge (`Live`, `Historical`, `Simulated`, `Generated`).
    pub mode_badge: DataModeBadge,
    /// Currently selected investigation/case ID.
    pub selected_case_id: Option<OpaqueId>,
    /// Shared geographic area filter across views.
    pub shared_area_filter: Option<String>,
    /// Shared time window `(start_ms, end_ms)` across views.
    pub shared_time_window_ms: Option<(i64, i64)>,
    /// Per-view local filter overrides (`CASE-02`: "Each view must expose its local filters when they differ from shared context").
    pub local_view_filters: BTreeMap<WorkspaceViewKind, String>,
    /// Navigation route history stack so selecting an event or switching views preserves the user's route back (`UX-02`).
    pub navigation_history: Vec<WorkspaceViewKind>,
    /// Currently focused keyboard element ID (`UX-04`, `A02`).
    pub focused_element_id: String,
}

impl Default for WorkspaceShellState {
    fn default() -> Self {
        Self {
            visual: VisualSystemConfig::default(),
            active_view: WorkspaceViewKind::Documents,
            mode_badge: DataModeBadge::Historical,
            selected_case_id: None,
            shared_area_filter: None,
            shared_time_window_ms: None,
            local_view_filters: BTreeMap::new(),
            navigation_history: Vec::new(),
            focused_element_id: "command_search_bar".to_owned(),
        }
    }
}

impl WorkspaceShellState {
    /// Switches to `next_view` while preserving `active_view` on the navigation history stack (`CASE-02`, `UX-02`).
    pub fn switch_view(&mut self, next_view: WorkspaceViewKind) {
        if self.active_view != next_view {
            self.navigation_history.push(self.active_view);
            self.active_view = next_view;
        }
    }

    /// Navigates back to the previous view on the history stack, if any (`UX-02`).
    pub fn navigate_back(&mut self) -> Option<WorkspaceViewKind> {
        let prev = self.navigation_history.pop()?;
        self.active_view = prev;
        Some(prev)
    }

    /// Returns `true` if `view` has an active local filter override that differs from the shared context (`CASE-02`).
    #[must_use]
    pub fn has_divergent_local_filter(&self, view: WorkspaceViewKind) -> bool {
        match (self.local_view_filters.get(&view), &self.shared_area_filter) {
            (Some(local), Some(shared)) => local != shared,
            (Some(_), None) => true,
            (None, _) => false,
        }
    }
}

/// Localhost transport authentication and origin guard (`docs/architecture.md` Section 9).
///
/// Loopback binding alone is not authentication; local UI/API requests must present a valid
/// session token and an allowed origin (or no cross-origin header).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalhostTransportGuard {
    expected_session_token: String,
    allowed_origins: Vec<String>,
}

impl LocalhostTransportGuard {
    /// Creates a localhost guard requiring `session_token` and restricting `Origin` to `allowed_origins`.
    pub fn new(
        session_token: impl Into<String>,
        allowed_origins: Vec<String>,
    ) -> Result<Self, AppError> {
        let tok = session_token.into();
        if tok.len() < 16 {
            return Err(AppError::WeakSessionToken);
        }
        Ok(Self {
            expected_session_token: tok,
            allowed_origins,
        })
    }

    /// Verifies `provided_token` and optional `origin_header`.
    pub fn verify_request(
        &self,
        provided_token: &str,
        origin_header: Option<&str>,
    ) -> Result<(), AppError> {
        if provided_token != self.expected_session_token {
            return Err(AppError::UnauthenticatedTransport);
        }
        if let Some(origin) = origin_header
            && !self.allowed_origins.iter().any(|allowed| allowed == origin)
        {
            return Err(AppError::CrossOriginRejected(origin.to_owned()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies `CASE-02` view route-back and local filter divergence, `UX-01`–`UX-05`
    /// visual/accessibility defaults and context isolation, and `LocalhostTransportGuard` (`#9`).
    #[test]
    fn workspace_shell_state_and_localhost_guard_contracts()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut shell = WorkspaceShellState::default();
        assert!(!shell.visual.routine_sound_enabled);
        assert!(shell.visual.visible_keyboard_focus);
        assert_eq!(DataModeBadge::Live.non_color_label(), "[LIVE ●]");
        assert_eq!(DataModeBadge::Historical.non_color_label(), "[HIST ◷]");
        assert_eq!(DataModeBadge::Simulated.non_color_label(), "[SIM ◇]");
        assert_eq!(DataModeBadge::Generated.non_color_label(), "[GEN ✦]");

        let case_id = OpaqueId::new("case_phoenix_1997")?;
        shell.selected_case_id = Some(case_id.clone());
        shell.shared_area_filter = Some("US-AZ".to_owned());
        shell.shared_time_window_ms = Some((858_297_600_000, 858_312_000_000));

        shell.switch_view(WorkspaceViewKind::Globe);
        shell.switch_view(WorkspaceViewKind::Sky);
        assert_eq!(shell.active_view, WorkspaceViewKind::Sky);
        assert_eq!(shell.selected_case_id.as_ref(), Some(&case_id));

        shell
            .local_view_filters
            .insert(WorkspaceViewKind::Sky, "US-NV".to_owned());
        assert!(shell.has_divergent_local_filter(WorkspaceViewKind::Sky));
        assert!(!shell.has_divergent_local_filter(WorkspaceViewKind::Globe));

        // Toggling reduced effects or reading modes leaves shared investigation context unchanged (UX-05).
        shell.visual.reduced_motion = true;
        shell.visual.reduced_transparency = true;
        shell.visual.reading_mode = ReadingMode::CleanEvidence;
        assert_eq!(shell.selected_case_id.as_ref(), Some(&case_id));
        assert_eq!(
            shell.shared_time_window_ms,
            Some((858_297_600_000, 858_312_000_000))
        );

        assert_eq!(shell.navigate_back(), Some(WorkspaceViewKind::Globe));
        assert_eq!(shell.navigate_back(), Some(WorkspaceViewKind::Documents));
        assert_eq!(shell.navigate_back(), None);

        // Localhost transport guard rejects weak tokens, wrong tokens, and untrusted origins.
        assert_eq!(
            LocalhostTransportGuard::new("short", Vec::new()),
            Err(AppError::WeakSessionToken)
        );
        let guard = LocalhostTransportGuard::new(
            "session_secret_token_012345",
            vec!["app://oia.local".to_owned()],
        )?;
        assert!(
            guard
                .verify_request("session_secret_token_012345", Some("app://oia.local"))
                .is_ok()
        );
        assert_eq!(
            guard.verify_request("wrong_token_0123456789", None),
            Err(AppError::UnauthenticatedTransport)
        );
        assert_eq!(
            guard.verify_request("session_secret_token_012345", Some("https://evil.example")),
            Err(AppError::CrossOriginRejected(
                "https://evil.example".to_owned()
            ))
        );
        Ok(())
    }
}
