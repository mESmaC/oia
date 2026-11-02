//! `oia-app` — Shared application service boundary, command/query separation, optimistic
//! concurrency, role-based workspace isolation, bounded event subscriptions with resynchronization,
//! localhost transport authentication, and `UX-01`–`UX-05` military-glass workspace shell state
//! (`#9`, `#14`, `C11`, `C12`, `A01`, `A02`, `A16`).

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::path::Path;

use oia_domain::{
    ActorId, Annotation, AvailabilityState, ContentHash, DomainError, DomainObject,
    DomainObjectType, Investigation, ObjectEnvelope, OpaqueId, OriginClass, ProvenanceRecord,
    ReassessmentState, Review, ReviewStatus, SchemaVersion, VersionId, WorkspaceVisibility,
};
use oia_query::{AuthorizationContext, SearchRequest, SearchResponse, execute_scoped_search};
use oia_store::{EvidenceStore, StoreBudget, StoreError};

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

/// Domain event emitted on state-changing commands (`docs/architecture.md` Section 7).
///
/// Carries only IDs/versions and minimal context (never unbounded media payloads or secrets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainEvent {
    /// Monotonically increasing sequence number.
    pub seq: u64,
    /// Owning case ID, if scoped to a case (used to permission-filter subscribers).
    pub case_id: Option<OpaqueId>,
    /// Whether the event pertains to a sensitive artifact.
    pub sensitive: bool,
    /// Event kind tag (`"case_created"`, `"annotation_added"`, `"review_submitted"`, `"dependency_stale"`).
    pub kind: String,
    /// Target version ID.
    pub version_id: VersionId,
}

/// Bounded subscriber queue with lag detection and resynchronization notice (`docs/architecture.md` Section 7).
#[derive(Debug, Clone)]
pub struct EventSubscription {
    auth: AuthorizationContext,
    max_buffered: usize,
    queue: VecDeque<DomainEvent>,
    dropped_oldest_seq: Option<u64>,
}

impl EventSubscription {
    /// Creates a bounded subscription for `auth` with capacity `max_buffered`.
    #[must_use]
    pub fn new(auth: AuthorizationContext, max_buffered: usize) -> Self {
        Self {
            auth,
            max_buffered: max_buffered.max(1),
            queue: VecDeque::new(),
            dropped_oldest_seq: None,
        }
    }

    fn push_if_permitted(&mut self, event: &DomainEvent) {
        let permitted = match &event.case_id {
            None => true,
            Some(cid) => {
                let vis = WorkspaceVisibility::PrivateCase {
                    case_id: cid.clone(),
                    sensitive: event.sensitive,
                };
                self.auth.can_read(&vis)
            }
        };
        if !permitted {
            return;
        }
        if self.queue.len() >= self.max_buffered
            && let Some(evicted) = self.queue.pop_front()
        {
            self.dropped_oldest_seq.get_or_insert(evicted.seq);
        }
        self.queue.push_back(event.clone());
    }

    /// Drains buffered events and returns `(events, resync_required_from_seq)`.
    pub fn drain_events(&mut self) -> (Vec<DomainEvent>, Option<u64>) {
        let resync = self.dropped_oldest_seq.take();
        let events = self.queue.drain(..).collect();
        (events, resync)
    }
}

/// State-changing commands accepted by [`OiaApplicationService`] (`docs/architecture.md` Section 7).
///
/// Distinct from read-only queries; every command carries a `request_id` for idempotency.
#[derive(Debug, Clone)]
pub enum AppCommand {
    /// Create a private investigation (`CASE-01`: saving never publishes).
    CreateInvestigation {
        /// Case ID.
        case_id: OpaqueId,
        /// Initial version ID.
        version_id: VersionId,
        /// Case title.
        title: String,
        /// Guiding question.
        question: String,
        /// Optional geographic area description.
        geographic_scope: Option<String>,
        /// Optional temporal interval description.
        temporal_scope: Option<String>,
        /// Topics.
        topics: Vec<String>,
        /// Timestamp (UTC ms).
        now_ms: i64,
    },
    /// Attach an annotation to `target_version_id` without mutating the target (`CASE-03`).
    AddAnnotation {
        /// Case ID.
        case_id: OpaqueId,
        /// Annotation ID.
        annotation_id: OpaqueId,
        /// Annotation version ID.
        version_id: VersionId,
        /// Target evidence version ID.
        target_version_id: VersionId,
        /// Expected latest version of the investigation (optimistic concurrency check).
        expected_case_version: VersionId,
        /// New investigation version ID linking the annotation.
        new_case_version: VersionId,
        /// Annotation text.
        body: String,
        /// Timestamp (UTC ms).
        now_ms: i64,
    },
    /// Submit a review assessment (`COLLAB-01`, `A16`).
    SubmitReview {
        /// Case ID.
        case_id: OpaqueId,
        /// Review ID.
        review_id: OpaqueId,
        /// Review version ID.
        version_id: VersionId,
        /// Target object version ID being reviewed.
        target_version_id: VersionId,
        /// Resulting review status (`Reviewed` or `Challenged`).
        resulting_status: ReviewStatus,
        /// Rationale.
        rationale: String,
        /// Cited evidence versions.
        cited_evidence_versions: Vec<VersionId>,
        /// Timestamp (UTC ms).
        now_ms: i64,
    },
}

impl AppCommand {
    const fn case_id(&self) -> &OpaqueId {
        match self {
            Self::CreateInvestigation { case_id, .. }
            | Self::AddAnnotation { case_id, .. }
            | Self::SubmitReview { case_id, .. } => case_id,
        }
    }

    fn command_fingerprint(&self) -> String {
        match self {
            Self::CreateInvestigation {
                case_id,
                version_id,
                title,
                question,
                geographic_scope,
                temporal_scope,
                topics,
                ..
            } => format!(
                "create_case:{case_id}:{version_id}:{title:?}:{question:?}:{geographic_scope:?}:{temporal_scope:?}:{topics:?}"
            ),
            Self::AddAnnotation {
                case_id,
                annotation_id,
                version_id,
                target_version_id,
                expected_case_version,
                new_case_version,
                body,
                ..
            } => format!(
                "add_ann:{case_id}:{annotation_id}:{version_id}:{target_version_id}:{expected_case_version}:{new_case_version}:{body:?}"
            ),
            Self::SubmitReview {
                case_id,
                review_id,
                version_id,
                target_version_id,
                resulting_status,
                rationale,
                cited_evidence_versions,
                ..
            } => {
                let cited: Vec<&str> = cited_evidence_versions
                    .iter()
                    .map(VersionId::as_str)
                    .collect();
                format!(
                    "submit_rev:{case_id}:{review_id}:{version_id}:{target_version_id}:{}:{rationale:?}:{cited:?}",
                    resulting_status.as_str()
                )
            }
        }
    }
}

/// Shared application service coordinating `EvidenceStore`, authorization, idempotency,
/// optimistic concurrency, and event subscriptions (`#14`, `C11`, `C12`).
#[derive(Debug)]
pub struct OiaApplicationService {
    store: EvidenceStore,
    shell: WorkspaceShellState,
    completed_requests: BTreeMap<(ActorId, OpaqueId), (String, VersionId)>,
    next_event_seq: u64,
    subscriptions: Vec<EventSubscription>,
}

impl OiaApplicationService {
    /// Opens an [`OiaApplicationService`] backed by an [`EvidenceStore`] at `root_dir`.
    pub fn open(root_dir: impl AsRef<Path>, budget: StoreBudget) -> Result<Self, AppError> {
        let store = EvidenceStore::open(root_dir, 2, budget).map_err(AppError::Store)?;
        Ok(Self {
            store,
            shell: WorkspaceShellState::default(),
            completed_requests: BTreeMap::new(),
            next_event_seq: 1,
            subscriptions: Vec::new(),
        })
    }

    /// Returns a shared reference to the workspace shell state.
    #[must_use]
    pub const fn shell(&self) -> &WorkspaceShellState {
        &self.shell
    }

    /// Returns a mutable reference to the workspace shell state.
    pub fn shell_mut(&mut self) -> &mut WorkspaceShellState {
        &mut self.shell
    }

    /// Returns a shared reference to the underlying [`EvidenceStore`].
    #[must_use]
    pub const fn store(&self) -> &EvidenceStore {
        &self.store
    }

    /// Returns a mutable reference to the underlying [`EvidenceStore`].
    pub fn store_mut(&mut self) -> &mut EvidenceStore {
        &mut self.store
    }

    /// Registers a bounded event subscription for `auth` (`#14`, `A16`).
    pub fn subscribe(&mut self, auth: AuthorizationContext, max_buffered: usize) -> usize {
        let idx = self.subscriptions.len();
        self.subscriptions
            .push(EventSubscription::new(auth, max_buffered));
        idx
    }

    /// Drains buffered events for subscriber `sub_idx`.
    pub fn drain_subscription(
        &mut self,
        sub_idx: usize,
    ) -> Result<(Vec<DomainEvent>, Option<u64>), AppError> {
        let sub = self
            .subscriptions
            .get_mut(sub_idx)
            .ok_or(AppError::SubscriptionNotFound(sub_idx))?;
        Ok(sub.drain_events())
    }

    fn emit_event(
        &mut self,
        case_id: Option<OpaqueId>,
        sensitive: bool,
        kind: &str,
        version_id: VersionId,
    ) {
        let ev = DomainEvent {
            seq: self.next_event_seq,
            case_id,
            sensitive,
            kind: kind.to_owned(),
            version_id,
        };
        self.next_event_seq += 1;
        for sub in &mut self.subscriptions {
            sub.push_if_permitted(&ev);
        }
    }

    // -------------------------------------------------------------------------
    // Read-Only Queries (never mutate state)
    // -------------------------------------------------------------------------

    /// Read-only query for a specific object version, enforcing role/sensitivity access (`#14`, `A16`).
    pub fn query_version<'a>(
        &'a self,
        auth: &AuthorizationContext,
        version_id: &VersionId,
    ) -> Result<&'a DomainObject, AppError> {
        let obj = self
            .store
            .graph()
            .get_version(version_id)
            .ok_or_else(|| AppError::Store(StoreError::VersionNotFound(version_id.clone())))?;
        if !auth.can_read(&obj.envelope().visibility) {
            return Err(AppError::UnauthorizedAccess);
        }
        Ok(obj)
    }

    /// Read-only query for an artifact's verified blob bytes, enforcing role/sensitivity access (`#14`, `A16`).
    pub fn query_artifact_blob(
        &self,
        auth: &AuthorizationContext,
        artifact_version_id: &VersionId,
    ) -> Result<Vec<u8>, AppError> {
        let obj = self.query_version(auth, artifact_version_id)?;
        let DomainObject::Artifact(art) = obj else {
            return Err(AppError::Store(StoreError::NotAnArtifactVersion(
                artifact_version_id.clone(),
            )));
        };
        self.store
            .read_verified_blob(&art.content_hash)
            .map_err(AppError::Store)
    }

    /// Read-only scoped search query (`CASE-05`, `A01`, `A16`).
    #[must_use]
    pub fn query_search(&self, auth: &AuthorizationContext, req: &SearchRequest) -> SearchResponse {
        execute_scoped_search(&self.store, auth, req)
    }

    // -------------------------------------------------------------------------
    // State-Changing Commands (idempotent, authorized, and version-checked)
    // -------------------------------------------------------------------------

    /// Executes a state-changing [`AppCommand`] with actor-scoped `request_id` idempotency,
    /// pre-replay role authorization, target sensitivity inheritance, and optimistic concurrency
    /// checking (`#14`, `C11`, `C12`, `A01`, `A16`).
    pub fn execute_command(
        &mut self,
        request_id: OpaqueId,
        auth: &AuthorizationContext,
        command: AppCommand,
    ) -> Result<VersionId, AppError> {
        // 1. Authenticate actor and authorize role on the target case BEFORE checking replay table (`C11`).
        let actor = auth.actor_id.clone().ok_or(AppError::UnauthorizedAccess)?;
        let case_id = command.case_id();
        let role = auth
            .role_in_case(case_id)
            .ok_or(AppError::UnauthorizedAccess)?;
        match &command {
            AppCommand::CreateInvestigation { .. } | AppCommand::AddAnnotation { .. } => {
                if !role.can_edit_case() {
                    return Err(AppError::UnauthorizedAccess);
                }
            }
            AppCommand::SubmitReview { .. } => {
                if !role.can_submit_review() {
                    return Err(AppError::UnauthorizedAccess);
                }
            }
        }

        // 2. Check actor-scoped idempotent replay table (`(ActorId, request_id) -> (fingerprint, VersionId)`, `C11`).
        let fingerprint = command.command_fingerprint();
        let replay_key = (actor.clone(), request_id.clone());
        if let Some((prev_fp, existing_ver)) = self.completed_requests.get(&replay_key) {
            if prev_fp != &fingerprint {
                return Err(AppError::ConflictingRequestIdReplay(request_id));
            }
            return Ok(existing_ver.clone());
        }

        let committed_version = match command {
            AppCommand::CreateInvestigation {
                case_id,
                version_id,
                title,
                question,
                geographic_scope,
                temporal_scope,
                topics,
                now_ms,
            } => {
                // Reject overwriting an existing investigation with `CreateInvestigation` (`C12`).
                if self.store.graph().get_latest(&case_id).is_some() {
                    return Err(AppError::DuplicateObjectId(case_id));
                }
                let inv = Investigation {
                    envelope: ObjectEnvelope {
                        id: case_id.clone(),
                        version_id: version_id.clone(),
                        schema_version: SchemaVersion::V1,
                        object_type: DomainObjectType::Investigation,
                        created_at_ms: now_ms,
                        created_by: actor,
                        visibility: WorkspaceVisibility::PrivateCase {
                            case_id: case_id.clone(),
                            sensitive: false,
                        },
                        supersedes: None,
                        provenance: ProvenanceRecord {
                            source_version_ids: Vec::new(),
                            declared_origin_note: Some("Created by analyst".to_owned()),
                            processing_history: Vec::new(),
                        },
                        origin_class: OriginClass::AnalyticalInference,
                        review_status: ReviewStatus::Unreviewed,
                        availability: AvailabilityState::Available,
                        reassessment: ReassessmentState::Current,
                    },
                    title,
                    question,
                    geographic_scope,
                    temporal_scope,
                    topics,
                    evidence_versions: Vec::new(),
                    saved_query_versions: Vec::new(),
                    annotation_versions: Vec::new(),
                    hypothesis_versions: Vec::new(),
                    lead_versions: Vec::new(),
                    is_published: false,
                };
                self.store
                    .put_object(DomainObject::Investigation(inv), None)
                    .map_err(AppError::Store)?;
                self.shell.selected_case_id = Some(case_id.clone());
                self.emit_event(Some(case_id), false, "case_created", version_id.clone());
                version_id
            }

            AppCommand::AddAnnotation {
                case_id,
                annotation_id,
                version_id,
                target_version_id,
                expected_case_version,
                new_case_version,
                body,
                now_ms,
            } => {
                // Verify caller has read permission on `target_version_id` and target belongs to `case_id`
                // or is `LibraryPublic`, and inherit target sensitivity (`C12`).
                let target_sensitive = {
                    let target_obj = self.query_version(auth, &target_version_id)?;
                    if let Some(target_cid) = target_obj.envelope().visibility.case_id()
                        && target_cid != &case_id
                    {
                        return Err(AppError::UnauthorizedAccess);
                    }
                    target_obj.envelope().visibility.is_sensitive()
                };
                if self.store.graph().get_latest(&annotation_id).is_some() {
                    return Err(AppError::DuplicateObjectId(annotation_id));
                }

                // Optimistic concurrency check on the owning investigation (`#14`, `A16`).
                let latest_case = self.store.graph().get_latest(&case_id).ok_or_else(|| {
                    AppError::Store(StoreError::VersionNotFound(expected_case_version.clone()))
                })?;
                if latest_case.envelope().version_id != expected_case_version {
                    return Err(AppError::OptimisticConcurrencyConflict {
                        expected: expected_case_version,
                        actual: latest_case.envelope().version_id.clone(),
                    });
                }
                let DomainObject::Investigation(mut case_clone) = latest_case.clone() else {
                    return Err(AppError::UnauthorizedAccess);
                };

                let ann = Annotation {
                    envelope: ObjectEnvelope {
                        id: annotation_id,
                        version_id: version_id.clone(),
                        schema_version: SchemaVersion::V1,
                        object_type: DomainObjectType::Annotation,
                        created_at_ms: now_ms,
                        created_by: actor.clone(),
                        visibility: WorkspaceVisibility::PrivateCase {
                            case_id: case_id.clone(),
                            sensitive: target_sensitive,
                        },
                        supersedes: None,
                        provenance: ProvenanceRecord {
                            source_version_ids: vec![target_version_id.clone()],
                            declared_origin_note: Some("User annotation".to_owned()),
                            processing_history: Vec::new(),
                        },
                        origin_class: OriginClass::AnalyticalInference,
                        review_status: ReviewStatus::Unreviewed,
                        availability: AvailabilityState::Available,
                        reassessment: ReassessmentState::Current,
                    },
                    target_version_id,
                    body,
                };

                case_clone.envelope.version_id = new_case_version;
                case_clone.envelope.supersedes = Some(expected_case_version);
                case_clone.envelope.created_at_ms = now_ms;
                case_clone.envelope.created_by = actor;
                case_clone.annotation_versions.push(version_id.clone());

                // Atomically validate and commit both the `Annotation` and the updated `Investigation`
                // in a single batch so a failure in `case_clone` never leaves an orphaned annotation (`A2`, `C11`).
                self.store
                    .put_objects_batch(
                        vec![
                            (DomainObject::Annotation(ann), None),
                            (
                                DomainObject::Investigation(case_clone),
                                Some("Added annotation to investigation".to_owned()),
                            ),
                        ],
                        Vec::new(),
                    )
                    .map_err(AppError::Store)?;

                self.emit_event(
                    Some(case_id),
                    target_sensitive,
                    "annotation_added",
                    version_id.clone(),
                );
                version_id
            }

            AppCommand::SubmitReview {
                case_id,
                review_id,
                version_id,
                target_version_id,
                resulting_status,
                rationale,
                cited_evidence_versions,
                now_ms,
            } => {
                // Verify caller has read permission on `target_version_id` and every cited evidence version,
                // verify case membership, and inherit target sensitivity (`C12`).
                let target_sensitive = {
                    let target_obj = self.query_version(auth, &target_version_id)?;
                    if let Some(target_cid) = target_obj.envelope().visibility.case_id()
                        && target_cid != &case_id
                    {
                        return Err(AppError::UnauthorizedAccess);
                    }
                    let mut sens = target_obj.envelope().visibility.is_sensitive();
                    for cited in &cited_evidence_versions {
                        let cited_obj = self.query_version(auth, cited)?;
                        if let Some(cited_cid) = cited_obj.envelope().visibility.case_id()
                            && cited_cid != &case_id
                        {
                            return Err(AppError::UnauthorizedAccess);
                        }
                        sens |= cited_obj.envelope().visibility.is_sensitive();
                    }
                    sens
                };
                if self.store.graph().get_latest(&review_id).is_some() {
                    return Err(AppError::DuplicateObjectId(review_id));
                }

                let rev = Review {
                    envelope: ObjectEnvelope {
                        id: review_id,
                        version_id: version_id.clone(),
                        schema_version: SchemaVersion::V1,
                        object_type: DomainObjectType::Review,
                        created_at_ms: now_ms,
                        created_by: actor,
                        visibility: WorkspaceVisibility::PrivateCase {
                            case_id: case_id.clone(),
                            sensitive: target_sensitive,
                        },
                        supersedes: None,
                        provenance: ProvenanceRecord {
                            source_version_ids: vec![target_version_id.clone()],
                            declared_origin_note: Some("Human review submission".to_owned()),
                            processing_history: Vec::new(),
                        },
                        origin_class: OriginClass::AnalyticalInference,
                        review_status: resulting_status,
                        availability: AvailabilityState::Available,
                        reassessment: ReassessmentState::Current,
                    },
                    target_version_id,
                    resulting_status,
                    rationale,
                    cited_evidence_versions,
                };
                self.store
                    .put_object(DomainObject::Review(rev), None)
                    .map_err(AppError::Store)?;
                self.emit_event(
                    Some(case_id),
                    target_sensitive,
                    "review_submitted",
                    version_id.clone(),
                );
                version_id
            }
        };

        self.completed_requests
            .insert(replay_key, (fingerprint, committed_version.clone()));
        Ok(committed_version)
    }
}

/// Typed errors for `oia-app`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppError {
    /// Domain validation error.
    Domain(DomainError),
    /// Store persistence error.
    Store(StoreError),
    /// Caller lacks permission for the requested case, role action, or sensitive artifact (`A16`).
    UnauthorizedAccess,
    /// Concurrent edit detected: `expected_version` did not match the latest accepted version (`#14`, `A16`).
    OptimisticConcurrencyConflict {
        /// Expected base version ID.
        expected: VersionId,
        /// Actual current version ID.
        actual: VersionId,
    },
    /// Same `request_id` was replayed by the same actor for a different command payload (`C11`).
    ConflictingRequestIdReplay(OpaqueId),
    /// Attempted to create a new case, annotation, or review with an `OpaqueId` that already exists (`C12`).
    DuplicateObjectId(OpaqueId),
    /// Localhost session token was shorter than 16 characters.
    WeakSessionToken,
    /// Localhost transport request lacked a valid session token.
    UnauthenticatedTransport,
    /// Localhost transport request came from an untrusted cross-origin header.
    CrossOriginRejected(String),
    /// Subscription index was not found.
    SubscriptionNotFound(usize),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(e) => write!(f, "application domain error: {e}"),
            Self::Store(e) => write!(f, "application store error: {e}"),
            Self::UnauthorizedAccess => {
                write!(
                    f,
                    "unauthorized access to case, command, or sensitive artifact"
                )
            }
            Self::OptimisticConcurrencyConflict { expected, actual } => write!(
                f,
                "optimistic concurrency conflict: expected version '{expected}', actual is '{actual}'"
            ),
            Self::ConflictingRequestIdReplay(req) => write!(
                f,
                "request_id '{req}' was replayed with a conflicting command payload"
            ),
            Self::DuplicateObjectId(id) => write!(
                f,
                "object ID '{id}' already exists and cannot be overwritten without supersession"
            ),
            Self::WeakSessionToken => {
                write!(f, "localhost session token must be at least 16 characters")
            }
            Self::UnauthenticatedTransport => {
                write!(f, "unauthenticated localhost transport request rejected")
            }
            Self::CrossOriginRejected(origin) => {
                write!(f, "rejected cross-origin localhost request from '{origin}'")
            }
            Self::SubscriptionNotFound(idx) => write!(f, "subscription {idx} not found"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<DomainError> for AppError {
    fn from(value: DomainError) -> Self {
        Self::Domain(value)
    }
}

impl From<StoreError> for AppError {
    fn from(value: StoreError) -> Self {
        Self::Store(value)
    }
}

/// Helper that verifies `ContentHash` re-export for CLI self-check.
#[must_use]
pub fn compute_sha256_hex(bytes: &[u8]) -> String {
    ContentHash::compute(bytes).to_hex()
}

#[cfg(test)]
mod tests {
    use super::*;
    use oia_domain::{Artifact, RetainedContent, SourceRecord};
    use oia_query::WorkspaceRole;
    use oia_store::RetentionClass;

    /// Verifies A01, A02, A16, C11, and C12 across `oia-app`:
    /// - Pre-replay authorization (`Outsider` cannot replay `Owner`'s `request_id`), actor-scoped
    ///   command fingerprinting (`ConflictingRequestIdReplay`).
    /// - Sensitivity inheritance on `AddAnnotation` / `SubmitReview` so `Viewer` never receives
    ///   events or read access to annotations/reviews on sensitive records (`C12`).
    /// - Optimistic concurrency and duplicate-case protection.
    #[test]
    fn workspace_shell_concurrency_and_role_isolation() -> Result<(), Box<dyn std::error::Error>> {
        let mut dir = std::env::temp_dir();
        dir.push(format!("oia_app_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let mut app = OiaApplicationService::open(&dir, StoreBudget::default())?;

        // 1. Localhost transport guard rejects weak tokens, bad tokens, and cross-origin requests.
        let guard = LocalhostTransportGuard::new(
            "oia_local_session_token_987654",
            vec!["http://127.0.0.1:4173".to_owned()],
        )?;
        assert!(
            guard
                .verify_request(
                    "oia_local_session_token_987654",
                    Some("http://127.0.0.1:4173")
                )
                .is_ok()
        );
        assert_eq!(
            guard.verify_request("wrong_token", None),
            Err(AppError::UnauthenticatedTransport)
        );
        assert_eq!(
            guard.verify_request(
                "oia_local_session_token_987654",
                Some("https://evil.example")
            ),
            Err(AppError::CrossOriginRejected(
                "https://evil.example".to_owned()
            ))
        );

        // 2. UX-01..UX-05: routine sound off by default, non-color mode badges, view switching & back route.
        assert!(!app.shell().visual.routine_sound_enabled);
        assert_ne!(
            DataModeBadge::Live.non_color_label(),
            DataModeBadge::Historical.non_color_label()
        );
        app.shell_mut().shared_area_filter = Some("Arizona".to_owned());
        app.shell_mut()
            .local_view_filters
            .insert(WorkspaceViewKind::Globe, "Phoenix Metro".to_owned());
        assert!(
            app.shell()
                .has_divergent_local_filter(WorkspaceViewKind::Globe)
        );
        app.shell_mut().switch_view(WorkspaceViewKind::Globe);
        app.shell_mut().switch_view(WorkspaceViewKind::Sky);
        assert_eq!(
            app.shell_mut().navigate_back(),
            Some(WorkspaceViewKind::Globe)
        );

        // 3. Role permissions: Owner vs Viewer vs Outsider on sensitive artifacts, subscriptions, and replay (C11, C12, A16).
        let case_id = OpaqueId::new("case_shared")?;
        let mut owner_auth = AuthorizationContext::new(ActorId::new("user_owner")?);
        owner_auth.grant_case_role(case_id.clone(), WorkspaceRole::Owner);

        let mut viewer_auth = AuthorizationContext::new(ActorId::new("user_viewer")?);
        viewer_auth.grant_case_role(case_id.clone(), WorkspaceRole::Viewer);

        let mut outsider_auth = AuthorizationContext::new(ActorId::new("user_outsider")?);
        outsider_auth.grant_case_role(OpaqueId::new("other_case")?, WorkspaceRole::Owner);

        let owner_sub = app.subscribe(owner_auth.clone(), 4);
        let viewer_sub = app.subscribe(viewer_auth.clone(), 4);
        let outsider_sub = app.subscribe(outsider_auth.clone(), 4);

        let case_v1 = VersionId::new("ver_case_shared_1")?;
        app.execute_command(
            OpaqueId::new("cmd_create_case")?,
            &owner_auth,
            AppCommand::CreateInvestigation {
                case_id: case_id.clone(),
                version_id: case_v1.clone(),
                title: "Shared Aerial Anomaly Case".to_owned(),
                question: "Were two sensors correlated?".to_owned(),
                geographic_scope: Some("33.4484N, 112.0740W".to_owned()),
                temporal_scope: Some("2026-11-01T04:00Z..05:00Z".to_owned()),
                topics: vec!["uap".to_owned(), "radar".to_owned()],
                now_ms: 1_700_000_000_000,
            },
        )?;

        // Outsider or Viewer attempting to replay `cmd_create_case` is rejected by pre-replay auth (`C11`)!
        assert_eq!(
            app.execute_command(
                OpaqueId::new("cmd_create_case")?,
                &outsider_auth,
                AppCommand::CreateInvestigation {
                    case_id: case_id.clone(),
                    version_id: case_v1.clone(),
                    title: "Ignored".to_owned(),
                    question: "Ignored".to_owned(),
                    geographic_scope: None,
                    temporal_scope: None,
                    topics: Vec::new(),
                    now_ms: 1_700_000_000_100,
                },
            ),
            Err(AppError::UnauthorizedAccess)
        );

        // Exact idempotent retry by the same authorized actor for the identical command payload succeeds.
        let idempotent_ver = app.execute_command(
            OpaqueId::new("cmd_create_case")?,
            &owner_auth,
            AppCommand::CreateInvestigation {
                case_id: case_id.clone(),
                version_id: case_v1.clone(),
                title: "Shared Aerial Anomaly Case".to_owned(),
                question: "Were two sensors correlated?".to_owned(),
                geographic_scope: Some("33.4484N, 112.0740W".to_owned()),
                temporal_scope: Some("2026-11-01T04:00Z..05:00Z".to_owned()),
                topics: vec!["uap".to_owned(), "radar".to_owned()],
                now_ms: 1_700_000_000_100,
            },
        )?;
        assert_eq!(idempotent_ver, case_v1);

        // Replaying the same request_id with a different title/body/version returns ConflictingRequestIdReplay (`C11`).
        assert_eq!(
            app.execute_command(
                OpaqueId::new("cmd_create_case")?,
                &owner_auth,
                AppCommand::CreateInvestigation {
                    case_id: case_id.clone(),
                    version_id: case_v1.clone(),
                    title: "Different Title".to_owned(),
                    question: "Were two sensors correlated?".to_owned(),
                    geographic_scope: Some("33.4484N, 112.0740W".to_owned()),
                    temporal_scope: Some("2026-11-01T04:00Z..05:00Z".to_owned()),
                    topics: vec!["uap".to_owned(), "radar".to_owned()],
                    now_ms: 1_700_000_000_150,
                },
            ),
            Err(AppError::ConflictingRequestIdReplay(OpaqueId::new(
                "cmd_create_case"
            )?))
        );
        assert_eq!(
            app.execute_command(
                OpaqueId::new("cmd_create_case")?,
                &owner_auth,
                AppCommand::SubmitReview {
                    case_id: case_id.clone(),
                    review_id: OpaqueId::new("rev_conflict")?,
                    version_id: VersionId::new("ver_rev_conflict")?,
                    target_version_id: case_v1.clone(),
                    resulting_status: ReviewStatus::Reviewed,
                    rationale: "Different command".to_owned(),
                    cited_evidence_versions: Vec::new(),
                    now_ms: 1_700_000_000_200,
                },
            ),
            Err(AppError::ConflictingRequestIdReplay(OpaqueId::new(
                "cmd_create_case"
            )?))
        );

        // Commit a sensitive source record + artifact in `case_shared`.
        let sensitive_bytes = b"CLASSIFIED_WITNESS_IDENTITY_RECORD";
        let sensitive_hash = ContentHash::compute(sensitive_bytes);
        let sr_sens_ver = VersionId::new("ver_sr_sens_1")?;
        let art_sens_ver = VersionId::new("ver_art_sens_1")?;
        let sens_vis = WorkspaceVisibility::PrivateCase {
            case_id: case_id.clone(),
            sensitive: true,
        };
        app.store_mut().commit_source_and_artifact(
            SourceRecord {
                envelope: ObjectEnvelope {
                    id: OpaqueId::new("sr_sens")?,
                    version_id: sr_sens_ver.clone(),
                    schema_version: SchemaVersion::V1,
                    object_type: DomainObjectType::SourceRecord,
                    created_at_ms: 1_700_000_001_000,
                    created_by: ActorId::new("user_owner")?,
                    visibility: sens_vis.clone(),
                    supersedes: None,
                    provenance: ProvenanceRecord {
                        source_version_ids: Vec::new(),
                        declared_origin_note: Some("Protected witness intake".to_owned()),
                        processing_history: Vec::new(),
                    },
                    origin_class: OriginClass::UserTestimony,
                    review_status: ReviewStatus::Unreviewed,
                    availability: AvailabilityState::Available,
                    reassessment: ReassessmentState::Current,
                },
                source_id: OpaqueId::new("src_witness")?,
                upstream_version: "v1".to_owned(),
                acquisition_attempt_id: OpaqueId::new("acq_sens")?,
                source_locator: "local://witness/1".to_owned(),
                source_event_time_ms: None,
                public_availability_time_ms: None,
                retrieved_at_ms: 1_700_000_001_000,
                content: RetainedContent::RetainedBlob {
                    content_hash: sensitive_hash.clone(),
                    byte_len: sensitive_bytes.len() as u64,
                    media_type: "text/plain".to_owned(),
                },
            },
            Artifact {
                envelope: ObjectEnvelope {
                    id: OpaqueId::new("art_sens")?,
                    version_id: art_sens_ver.clone(),
                    schema_version: SchemaVersion::V1,
                    object_type: DomainObjectType::Artifact,
                    created_at_ms: 1_700_000_001_000,
                    created_by: ActorId::new("user_owner")?,
                    visibility: sens_vis,
                    supersedes: None,
                    provenance: ProvenanceRecord {
                        source_version_ids: vec![sr_sens_ver.clone()],
                        declared_origin_note: None,
                        processing_history: Vec::new(),
                    },
                    origin_class: OriginClass::UserTestimony,
                    review_status: ReviewStatus::Unreviewed,
                    availability: AvailabilityState::Available,
                    reassessment: ReassessmentState::Current,
                },
                content_hash: sensitive_hash,
                media_type: "text/plain".to_owned(),
                byte_len: sensitive_bytes.len() as u64,
                derived_from_artifact: None,
                quality_effect_disclosure: None,
            },
            sensitive_bytes,
            RetentionClass::PinnedCaseEvidence,
        )?;

        // Owner can read sensitive blob; Viewer and Outsider are denied.
        assert!(app.query_artifact_blob(&owner_auth, &art_sens_ver).is_ok());
        assert_eq!(
            app.query_artifact_blob(&viewer_auth, &art_sens_ver),
            Err(AppError::UnauthorizedAccess)
        );
        assert_eq!(
            app.query_artifact_blob(&outsider_auth, &case_v1),
            Err(AppError::UnauthorizedAccess)
        );

        // Owner adds annotation on sensitive target `sr_sens_ver`; annotation inherits `sensitive: true` (`C12`)!
        let case_v2 = VersionId::new("ver_case_shared_2")?;
        let ann_v1 = VersionId::new("ver_ann_1")?;
        app.execute_command(
            OpaqueId::new("cmd_ann_1")?,
            &owner_auth,
            AppCommand::AddAnnotation {
                case_id: case_id.clone(),
                annotation_id: OpaqueId::new("ann_1")?,
                version_id: ann_v1.clone(),
                target_version_id: sr_sens_ver.clone(),
                expected_case_version: case_v1.clone(),
                new_case_version: case_v2.clone(),
                body: "Checked witness clock offset".to_owned(),
                now_ms: 1_700_000_002_000,
            },
        )?;
        // Viewer cannot read the annotation on the sensitive record (`C12`)!
        assert_eq!(
            app.query_version(&viewer_auth, &ann_v1),
            Err(AppError::UnauthorizedAccess)
        );

        let stale_edit = app.execute_command(
            OpaqueId::new("cmd_ann_stale")?,
            &owner_auth,
            AppCommand::AddAnnotation {
                case_id: case_id.clone(),
                annotation_id: OpaqueId::new("ann_2")?,
                version_id: VersionId::new("ver_ann_2")?,
                target_version_id: sr_sens_ver,
                expected_case_version: case_v1, // stale base version!
                new_case_version: VersionId::new("ver_case_shared_3")?,
                body: "Concurrent conflicting edit".to_owned(),
                now_ms: 1_700_000_003_000,
            },
        );
        assert!(matches!(
            stale_edit,
            Err(AppError::OptimisticConcurrencyConflict { .. })
        ));

        // Outsider received 0 events; Viewer received only the non-sensitive `case_created` event (1 event);
        // Owner received both `case_created` and sensitive `annotation_added` (2 events) (`C12`).
        let (outsider_events, _) = app.drain_subscription(outsider_sub)?;
        assert!(outsider_events.is_empty());
        let (viewer_events, _) = app.drain_subscription(viewer_sub)?;
        assert_eq!(viewer_events.len(), 1);
        let (owner_events, _) = app.drain_subscription(owner_sub)?;
        assert_eq!(owner_events.len(), 2);

        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }
}
