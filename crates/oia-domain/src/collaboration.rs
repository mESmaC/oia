//! Shared-case roles, sensitive-artifact access isolation, scoped reviews,
//! distinct save/export/share/publish transitions, and offline field synchronization
//! with explicit conflict preservation (`COLLAB-01`, `COLLAB-03`, `FIELD-04`, `A13`, `A16`, `#38`).
//!
//! This module is a deterministic, in-memory domain kernel with zero I/O or external
//! dependencies. Persistent relational storage (`#11`), background job scheduling (`#14`),
//! portable bundle file writing (`#22`), and hosted transport/identity (`D10`) remain
//! outside this module.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::{ActorId, OpaqueId, SchemaVersion, VersionId};

/// Mandatory caveat attached to every scoped review (`COLLAB-01`).
pub const MANDATORY_REVIEW_SCOPE_CAVEAT: &str = "A review records its author's assessment within the declared scope and cited evidence versions; it does not establish universal truth.";

/// Mandatory label preserved on simulated field captures across search, reports, and exports (`FIELD-04`, `A13`).
pub const VISIBLE_SIMULATION_LABEL: &str =
    "SIMULATED FIELD FIXTURE — CONTROLLED SIMULATION (NOT OBSERVED EVIDENCE)";

/// Default maximum UTF-8 bytes per narrative, rationale, or summary field.
pub const DEFAULT_MAX_TEXT_BYTES: usize = 8_192;
/// Default maximum collaborators per shared case.
pub const DEFAULT_MAX_COLLABORATORS: usize = 128;
/// Default maximum registered case items (artifacts/records) per case.
pub const DEFAULT_MAX_ITEMS: usize = 1_024;
/// Default maximum versions retained per item.
pub const DEFAULT_MAX_VERSIONS_PER_ITEM: usize = 128;
/// Default maximum scoped reviews, competing interpretations, or assignments per case.
pub const DEFAULT_MAX_REVIEWS_AND_NOTES: usize = 512;
/// Default maximum issued or shared report snapshots per case.
pub const DEFAULT_MAX_REPORT_SNAPSHOTS: usize = 128;

/// Which resource limit was zero or exceeded during a collaboration or sync operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollaborationLimitKind {
    TextBytes,
    Collaborators,
    Items,
    VersionsPerItem,
    ReviewsAndNotes,
    ReportSnapshots,
}

impl fmt::Display for CollaborationLimitKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TextBytes => f.write_str("max_text_bytes"),
            Self::Collaborators => f.write_str("max_collaborators"),
            Self::Items => f.write_str("max_items"),
            Self::VersionsPerItem => f.write_str("max_versions_per_item"),
            Self::ReviewsAndNotes => f.write_str("max_reviews_and_notes"),
            Self::ReportSnapshots => f.write_str("max_report_snapshots"),
        }
    }
}

/// Explicit resource bounds for a [`SharedCaseWorkspace`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollaborationLimits {
    max_text_bytes: usize,
    max_collaborators: usize,
    max_items: usize,
    max_versions_per_item: usize,
    max_reviews_and_notes: usize,
    max_report_snapshots: usize,
}

impl CollaborationLimits {
    /// Validates that all collaboration and offline-sync limits are non-zero.
    pub fn new(
        max_text_bytes: usize,
        max_collaborators: usize,
        max_items: usize,
        max_versions_per_item: usize,
        max_reviews_and_notes: usize,
        max_report_snapshots: usize,
    ) -> Result<Self, CollaborationError> {
        if max_text_bytes == 0 {
            return Err(CollaborationError::ZeroLimit(
                CollaborationLimitKind::TextBytes,
            ));
        }
        if max_collaborators == 0 {
            return Err(CollaborationError::ZeroLimit(
                CollaborationLimitKind::Collaborators,
            ));
        }
        if max_items == 0 {
            return Err(CollaborationError::ZeroLimit(CollaborationLimitKind::Items));
        }
        if max_versions_per_item == 0 {
            return Err(CollaborationError::ZeroLimit(
                CollaborationLimitKind::VersionsPerItem,
            ));
        }
        if max_reviews_and_notes == 0 {
            return Err(CollaborationError::ZeroLimit(
                CollaborationLimitKind::ReviewsAndNotes,
            ));
        }
        if max_report_snapshots == 0 {
            return Err(CollaborationError::ZeroLimit(
                CollaborationLimitKind::ReportSnapshots,
            ));
        }
        Ok(Self {
            max_text_bytes,
            max_collaborators,
            max_items,
            max_versions_per_item,
            max_reviews_and_notes,
            max_report_snapshots,
        })
    }

    #[must_use]
    pub const fn max_text_bytes(&self) -> usize {
        self.max_text_bytes
    }

    #[must_use]
    pub const fn max_collaborators(&self) -> usize {
        self.max_collaborators
    }

    #[must_use]
    pub const fn max_items(&self) -> usize {
        self.max_items
    }

    #[must_use]
    pub const fn max_versions_per_item(&self) -> usize {
        self.max_versions_per_item
    }

    #[must_use]
    pub const fn max_reviews_and_notes(&self) -> usize {
        self.max_reviews_and_notes
    }

    #[must_use]
    pub const fn max_report_snapshots(&self) -> usize {
        self.max_report_snapshots
    }
}

impl Default for CollaborationLimits {
    fn default() -> Self {
        Self {
            max_text_bytes: DEFAULT_MAX_TEXT_BYTES,
            max_collaborators: DEFAULT_MAX_COLLABORATORS,
            max_items: DEFAULT_MAX_ITEMS,
            max_versions_per_item: DEFAULT_MAX_VERSIONS_PER_ITEM,
            max_reviews_and_notes: DEFAULT_MAX_REVIEWS_AND_NOTES,
            max_report_snapshots: DEFAULT_MAX_REPORT_SNAPSHOTS,
        }
    }
}

/// Role within a shared case (`COLLAB-01`, `docs/architecture.md` §9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CaseRole {
    /// Controls sharing, roles, clearance grants, publication, and deletion.
    Owner,
    /// Adds and edits case material, prepares/synchronizes field captures, saves drafts, and exports privately.
    Editor,
    /// Submits scoped assessments, reviews, and competing interpretations; inspects permitted content.
    Reviewer,
    /// Read-only inspection of permitted case material.
    Viewer,
}

impl CaseRole {
    /// Whether this role can manage collaborator roles, clearances, or public publication.
    #[must_use]
    pub const fn can_manage_case_and_publish(self) -> bool {
        matches!(self, Self::Owner)
    }

    /// Whether this role can add or edit evidence, synchronize offline captures, and save report drafts.
    #[must_use]
    pub const fn can_edit_evidence(self) -> bool {
        matches!(self, Self::Owner | Self::Editor)
    }

    /// Whether this role can submit scoped reviews and competing interpretations.
    #[must_use]
    pub const fn can_submit_review(self) -> bool {
        matches!(self, Self::Owner | Self::Editor | Self::Reviewer)
    }
}

impl fmt::Display for CaseRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Owner => f.write_str("owner"),
            Self::Editor => f.write_str("editor"),
            Self::Reviewer => f.write_str("reviewer"),
            Self::Viewer => f.write_str("viewer"),
        }
    }
}

/// Independent clearance flags for unpublished drafts, sensitive artifacts, and sensitive locations (`COLLAB-01`, `A16`).
///
/// Per `docs/architecture.md` §9, permission to see a case title or hold a role does not
/// automatically grant access to sensitive artifacts or sensitive locations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClearanceGrant {
    pub allow_unpublished_drafts: bool,
    pub allow_sensitive_artifacts: bool,
    pub allow_sensitive_locations: bool,
}

impl ClearanceGrant {
    /// Clearance for standard published/collaborator material only (no sensitive artifacts, sensitive locations, or drafts).
    #[must_use]
    pub const fn standard_only() -> Self {
        Self {
            allow_unpublished_drafts: false,
            allow_sensitive_artifacts: false,
            allow_sensitive_locations: false,
        }
    }

    /// Full clearance across unpublished drafts, sensitive artifacts, and sensitive locations.
    #[must_use]
    pub const fn full() -> Self {
        Self {
            allow_unpublished_drafts: true,
            allow_sensitive_artifacts: true,
            allow_sensitive_locations: true,
        }
    }

    /// Checks whether this clearance covers the given [`SensitivityFlags`].
    #[must_use]
    pub const fn permits(self, flags: SensitivityFlags) -> bool {
        if flags.unpublished_draft && !self.allow_unpublished_drafts {
            return false;
        }
        if flags.sensitive_artifact && !self.allow_sensitive_artifacts {
            return false;
        }
        if flags.sensitive_location && !self.allow_sensitive_locations {
            return false;
        }
        true
    }
}

/// Sensitivity attributes attached to an artifact, observation, or report section (`COLLAB-01`, `A16`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SensitivityFlags {
    pub unpublished_draft: bool,
    pub sensitive_artifact: bool,
    pub sensitive_location: bool,
}

impl SensitivityFlags {
    /// Standard shared case material with no special sensitivity restrictions.
    #[must_use]
    pub const fn standard() -> Self {
        Self {
            unpublished_draft: false,
            sensitive_artifact: false,
            sensitive_location: false,
        }
    }
}

/// Access surfaces across which authorization and redaction must be consistently enforced (`COLLAB-01`, `A16`, `AGENTS.md` §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AccessSurface {
    Search,
    BlobRead,
    Subscription,
    ModelInput,
    ReportGeneration,
    DirectArtifactRequest,
    OfflinePreparation,
    Export,
}

impl fmt::Display for AccessSurface {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Search => f.write_str("search"),
            Self::BlobRead => f.write_str("blob_read"),
            Self::Subscription => f.write_str("subscription"),
            Self::ModelInput => f.write_str("model_input"),
            Self::ReportGeneration => f.write_str("report_generation"),
            Self::DirectArtifactRequest => f.write_str("direct_artifact_request"),
            Self::OfflinePreparation => f.write_str("offline_preparation"),
            Self::Export => f.write_str("export"),
        }
    }
}

/// Registered collaborator membership in a [`SharedCaseWorkspace`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollaboratorMembership {
    actor: ActorId,
    role: CaseRole,
    clearance: ClearanceGrant,
}

impl CollaboratorMembership {
    #[must_use]
    pub fn actor(&self) -> &ActorId {
        &self.actor
    }

    #[must_use]
    pub const fn role(&self) -> CaseRole {
        self.role
    }

    #[must_use]
    pub const fn clearance(&self) -> ClearanceGrant {
        self.clearance
    }
}

/// Fixed origin of a case item or field capture (`FIELD-04`, `A13`, `AGENTS.md` §2).
///
/// Simulations remain permanently labeled across search, reports, and evaluation and cannot
/// be promoted to observed field evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceOriginKind {
    /// Observed or reported field/source material.
    ObservedOrReported,
    /// Controlled simulation fixture with explicit noise/blur/geometry/clock-drift parameters (`FIELD-04`).
    LabeledSimulation { simulation_parameters: String },
}

impl EvidenceOriginKind {
    /// Whether this item is a simulation that must carry [`VISIBLE_SIMULATION_LABEL`].
    #[must_use]
    pub const fn is_simulation(&self) -> bool {
        matches!(self, Self::LabeledSimulation { .. })
    }

    /// Mandatory visible label if this item is a simulation (`A13`, `FIELD-04`).
    #[must_use]
    pub const fn required_visible_label(&self) -> Option<&'static str> {
        match self {
            Self::ObservedOrReported => None,
            Self::LabeledSimulation { .. } => Some(VISIBLE_SIMULATION_LABEL),
        }
    }
}

/// Device clock quality recorded with an offline or live capture (`FIELD-04`, `docs/architecture.md` §3.1, §9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceClockQuality {
    /// Synchronized to UTC with a bounded uncertainty in milliseconds.
    SynchronizedUtc { uncertainty_ms: u64 },
    /// Local device clock with estimated offset and uncertainty in milliseconds.
    DriftingLocalClock {
        estimated_offset_ms: i64,
        uncertainty_ms: u64,
    },
    /// Unreferenced device clock where absolute UTC offset is unknown.
    UnreferencedLocalClock { uncertainty_reason: String },
}

/// Freshness state of a prepared offline reference item at the time of field use (`FIELD-04`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfflineReferenceFreshness {
    /// Prepared reference was used directly while fresh (`capture_ms <= expires_at_unix_ms`).
    FreshWithinWindow,
    /// Prepared reference was cached past its expiration window (`FIELD-04`: mark cached/expired data).
    CachedExpired { expired_by_ms: u64 },
    /// Device clock was unreferenced so expiration against UTC could not be verified.
    ClockUncertainCannotVerifyExpiry,
}

/// Immutable version of a case item retained in the workspace's version graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseItemVersion {
    version: VersionId,
    author: ActorId,
    parent_versions: Vec<VersionId>,
    summary_text: String,
    sensitive_location_detail: Option<String>,
    content_sha256: String,
    origin: EvidenceOriginKind,
    clock_quality: DeviceClockQuality,
    reference_freshness: Option<OfflineReferenceFreshness>,
}

impl CaseItemVersion {
    #[must_use]
    pub fn version(&self) -> &VersionId {
        &self.version
    }

    #[must_use]
    pub fn author(&self) -> &ActorId {
        &self.author
    }

    #[must_use]
    pub fn parent_versions(&self) -> &[VersionId] {
        &self.parent_versions
    }

    #[must_use]
    pub fn summary_text(&self) -> &str {
        &self.summary_text
    }

    #[must_use]
    pub fn sensitive_location_detail(&self) -> Option<&str> {
        self.sensitive_location_detail.as_deref()
    }

    #[must_use]
    pub fn content_sha256(&self) -> &str {
        &self.content_sha256
    }

    #[must_use]
    pub fn origin(&self) -> &EvidenceOriginKind {
        &self.origin
    }

    #[must_use]
    pub fn clock_quality(&self) -> &DeviceClockQuality {
        &self.clock_quality
    }

    #[must_use]
    pub const fn reference_freshness(&self) -> Option<OfflineReferenceFreshness> {
        self.reference_freshness
    }
}

/// Input for registering or updating a case item version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseItemInput {
    pub item_id: OpaqueId,
    pub version: VersionId,
    pub sensitivity: SensitivityFlags,
    pub summary_text: String,
    pub sensitive_location_detail: Option<String>,
    pub content_sha256: String,
    pub origin: EvidenceOriginKind,
    pub clock_quality: DeviceClockQuality,
}

/// Sanitized projection of a case item after permission and redaction filtering (`AGENTS.md` §5, `docs/architecture.md` §9).
///
/// When a caller has access to an item's general content but lacks `allow_sensitive_locations`,
/// `sensitive_location_detail` is physically stripped (`None`) and `redacted_sensitive_location`
/// is set to `true` rather than merely hiding a UI field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermittedItemView {
    item_id: OpaqueId,
    version: VersionId,
    summary_text: String,
    sensitive_location_detail: Option<String>,
    redacted_sensitive_location: bool,
    content_sha256: String,
    origin: EvidenceOriginKind,
    visible_simulation_label: Option<&'static str>,
    clock_quality: DeviceClockQuality,
    reference_freshness: Option<OfflineReferenceFreshness>,
}

impl PermittedItemView {
    #[must_use]
    pub fn item_id(&self) -> &OpaqueId {
        &self.item_id
    }

    #[must_use]
    pub fn version(&self) -> &VersionId {
        &self.version
    }

    #[must_use]
    pub fn summary_text(&self) -> &str {
        &self.summary_text
    }

    #[must_use]
    pub fn sensitive_location_detail(&self) -> Option<&str> {
        self.sensitive_location_detail.as_deref()
    }

    #[must_use]
    pub const fn redacted_sensitive_location(&self) -> bool {
        self.redacted_sensitive_location
    }

    #[must_use]
    pub fn content_sha256(&self) -> &str {
        &self.content_sha256
    }

    #[must_use]
    pub fn origin(&self) -> &EvidenceOriginKind {
        &self.origin
    }

    #[must_use]
    pub const fn visible_simulation_label(&self) -> Option<&'static str> {
        self.visible_simulation_label
    }

    #[must_use]
    pub fn clock_quality(&self) -> &DeviceClockQuality {
        &self.clock_quality
    }

    #[must_use]
    pub const fn reference_freshness(&self) -> Option<OfflineReferenceFreshness> {
        self.reference_freshness
    }
}

/// Reason an item or field was redacted or omitted during surface filtering or export preview (`COLLAB-03`, `A16`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedactionReason {
    UnpublishedDraftRestricted,
    SensitiveArtifactRestricted,
    SensitiveLocationStripped,
}

impl fmt::Display for RedactionReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnpublishedDraftRestricted => f.write_str("unpublished_draft_restricted"),
            Self::SensitiveArtifactRestricted => f.write_str("sensitive_artifact_restricted"),
            Self::SensitiveLocationStripped => f.write_str("sensitive_location_stripped"),
        }
    }
}

/// Notice of an omitted or redacted item produced during access filtering or export preview (`COLLAB-03`, `A16`).
///
/// Contains no secret payload, summary text, or sensitive coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedactionNotice {
    pub item_id: OpaqueId,
    pub version: VersionId,
    pub surface: AccessSurface,
    pub reason: RedactionReason,
}

/// Result of filtering case items for a specific actor and [`AccessSurface`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceQueryEnvelope {
    actor: ActorId,
    surface: AccessSurface,
    permitted_items: Vec<PermittedItemView>,
    redaction_notices: Vec<RedactionNotice>,
}

impl SurfaceQueryEnvelope {
    #[must_use]
    pub fn actor(&self) -> &ActorId {
        &self.actor
    }

    #[must_use]
    pub const fn surface(&self) -> AccessSurface {
        self.surface
    }

    #[must_use]
    pub fn permitted_items(&self) -> &[PermittedItemView] {
        &self.permitted_items
    }

    #[must_use]
    pub fn redaction_notices(&self) -> &[RedactionNotice] {
        &self.redaction_notices
    }
}

/// Explicit scope of a reviewer's assessment (`COLLAB-01`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewScope {
    /// Applies to the cited target version as a whole.
    WholeTargetVersion,
    /// Applies only to a specific UTF-8 byte range `[start_byte, end_byte)` within the target.
    PassageByteRange { start_byte: usize, end_byte: usize },
    /// Applies only to a bounded time window `[start_unix_ms, end_unix_ms)`.
    TemporalWindow {
        start_unix_ms: i64,
        end_unix_ms: i64,
    },
    /// Applies only to calibration, clock alignment, or observation geometry assumptions.
    CalibrationAndGeometryOnly,
    /// Applies only to analytical methodology, not underlying source authenticity.
    MethodologyOnly,
}

/// Reviewer disposition within the declared [`ReviewScope`] (`COLLAB-01`, `docs/architecture.md` §2.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewDisposition {
    SupportedWithinScope,
    ChallengedWithCounterevidence,
    UnresolvedNeedsCollection,
    InsufficientEvidence,
}

/// Input for recording an author- and scope-attributed review (`COLLAB-01`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedReviewInput {
    pub review_id: OpaqueId,
    pub review_version: VersionId,
    pub target_item_id: OpaqueId,
    pub target_version: VersionId,
    pub scope: ReviewScope,
    pub disposition: ReviewDisposition,
    pub rationale: String,
    pub cited_evidence_versions: Vec<VersionId>,
    pub supersedes_review_version: Option<VersionId>,
}

/// Immutable, author- and scope-attributed review record (`COLLAB-01`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedReviewRecord {
    review_id: OpaqueId,
    review_version: VersionId,
    author: ActorId,
    author_role: CaseRole,
    target_item_id: OpaqueId,
    target_version: VersionId,
    scope: ReviewScope,
    disposition: ReviewDisposition,
    rationale: String,
    cited_evidence_versions: Vec<VersionId>,
    supersedes_review_version: Option<VersionId>,
}

impl ScopedReviewRecord {
    #[must_use]
    pub fn review_id(&self) -> &OpaqueId {
        &self.review_id
    }

    #[must_use]
    pub fn review_version(&self) -> &VersionId {
        &self.review_version
    }

    #[must_use]
    pub fn author(&self) -> &ActorId {
        &self.author
    }

    #[must_use]
    pub const fn author_role(&self) -> CaseRole {
        self.author_role
    }

    #[must_use]
    pub fn target_item_id(&self) -> &OpaqueId {
        &self.target_item_id
    }

    #[must_use]
    pub fn target_version(&self) -> &VersionId {
        &self.target_version
    }

    #[must_use]
    pub fn scope(&self) -> &ReviewScope {
        &self.scope
    }

    #[must_use]
    pub const fn disposition(&self) -> ReviewDisposition {
        self.disposition
    }

    #[must_use]
    pub fn rationale(&self) -> &str {
        &self.rationale
    }

    #[must_use]
    pub fn cited_evidence_versions(&self) -> &[VersionId] {
        &self.cited_evidence_versions
    }

    #[must_use]
    pub fn supersedes_review_version(&self) -> Option<&VersionId> {
        self.supersedes_review_version.as_ref()
    }

    /// Per `COLLAB-01`, a review records its author and scope rather than implying universal truth.
    #[must_use]
    pub const fn implies_universal_truth(&self) -> bool {
        false
    }

    #[must_use]
    pub const fn scope_caveat(&self) -> &'static str {
        MANDATORY_REVIEW_SCOPE_CAVEAT
    }
}

/// Competing analyst interpretation retained alongside alternative hypotheses (`COLLAB-01`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompetingInterpretation {
    interpretation_id: OpaqueId,
    version: VersionId,
    author: ActorId,
    target_item_id: OpaqueId,
    hypothesis: String,
    supporting_versions: Vec<VersionId>,
    contradicting_versions: Vec<VersionId>,
    explicit_assumptions: String,
}

impl CompetingInterpretation {
    #[must_use]
    pub fn interpretation_id(&self) -> &OpaqueId {
        &self.interpretation_id
    }

    #[must_use]
    pub fn version(&self) -> &VersionId {
        &self.version
    }

    #[must_use]
    pub fn author(&self) -> &ActorId {
        &self.author
    }

    #[must_use]
    pub fn target_item_id(&self) -> &OpaqueId {
        &self.target_item_id
    }

    #[must_use]
    pub fn hypothesis(&self) -> &str {
        &self.hypothesis
    }

    #[must_use]
    pub fn supporting_versions(&self) -> &[VersionId] {
        &self.supporting_versions
    }

    #[must_use]
    pub fn contradicting_versions(&self) -> &[VersionId] {
        &self.contradicting_versions
    }

    #[must_use]
    pub fn explicit_assumptions(&self) -> &str {
        &self.explicit_assumptions
    }
}

/// Status of a collaborator task or review assignment (`COLLAB-01`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignmentStatus {
    Open,
    CompletedWithReview,
}

/// Assignment record linking an assignee, target version, and scope (`COLLAB-01`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewAssignment {
    assignment_id: OpaqueId,
    assigned_by: ActorId,
    assignee: ActorId,
    target_item_id: OpaqueId,
    target_version: VersionId,
    instructions: String,
    status: AssignmentStatus,
}

impl ReviewAssignment {
    #[must_use]
    pub fn assignment_id(&self) -> &OpaqueId {
        &self.assignment_id
    }

    #[must_use]
    pub fn assigned_by(&self) -> &ActorId {
        &self.assigned_by
    }

    #[must_use]
    pub fn assignee(&self) -> &ActorId {
        &self.assignee
    }

    #[must_use]
    pub fn target_item_id(&self) -> &OpaqueId {
        &self.target_item_id
    }

    #[must_use]
    pub fn target_version(&self) -> &VersionId {
        &self.target_version
    }

    #[must_use]
    pub fn instructions(&self) -> &str {
        &self.instructions
    }

    #[must_use]
    pub const fn status(&self) -> AssignmentStatus {
        self.status
    }
}

/// Offline preparation package for field research (`FIELD-04`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedOfflinePackage {
    package_id: OpaqueId,
    case_id: OpaqueId,
    prepared_for: ActorId,
    prepared_at_unix_ms: i64,
    expires_at_unix_ms: i64,
    included_items: Vec<PermittedItemView>,
    omitted_notices: Vec<RedactionNotice>,
}

impl PreparedOfflinePackage {
    #[must_use]
    pub fn package_id(&self) -> &OpaqueId {
        &self.package_id
    }

    #[must_use]
    pub fn case_id(&self) -> &OpaqueId {
        &self.case_id
    }

    #[must_use]
    pub fn prepared_for(&self) -> &ActorId {
        &self.prepared_for
    }

    #[must_use]
    pub const fn prepared_at_unix_ms(&self) -> i64 {
        self.prepared_at_unix_ms
    }

    #[must_use]
    pub const fn expires_at_unix_ms(&self) -> i64 {
        self.expires_at_unix_ms
    }

    #[must_use]
    pub fn included_items(&self) -> &[PermittedItemView] {
        &self.included_items
    }

    #[must_use]
    pub fn omitted_notices(&self) -> &[RedactionNotice] {
        &self.omitted_notices
    }

    /// Evaluates whether the prepared offline package is fresh, cached/expired, or clock-uncertain
    /// at the time of field capture (`FIELD-04`).
    #[must_use]
    pub fn evaluate_freshness(
        &self,
        observed_device_unix_ms: Option<i64>,
        clock_quality: &DeviceClockQuality,
    ) -> OfflineReferenceFreshness {
        if matches!(
            clock_quality,
            DeviceClockQuality::UnreferencedLocalClock { .. }
        ) {
            return OfflineReferenceFreshness::ClockUncertainCannotVerifyExpiry;
        }
        let Some(now_ms) = observed_device_unix_ms else {
            return OfflineReferenceFreshness::ClockUncertainCannotVerifyExpiry;
        };
        if now_ms <= self.expires_at_unix_ms {
            OfflineReferenceFreshness::FreshWithinWindow
        } else {
            let expired_by_ms = now_ms.saturating_sub(self.expires_at_unix_ms) as u64;
            OfflineReferenceFreshness::CachedExpired { expired_by_ms }
        }
    }
}

/// Offline field observation or offline edit submitted for synchronization (`FIELD-04`, `A13`, `A16`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfflineFieldCapture {
    pub capture_id: OpaqueId,
    pub target_item_id: OpaqueId,
    pub base_version: Option<VersionId>,
    pub new_version: VersionId,
    pub sensitivity: SensitivityFlags,
    pub summary_text: String,
    pub sensitive_location_detail: Option<String>,
    pub content_sha256: String,
    pub origin: EvidenceOriginKind,
    pub clock_quality: DeviceClockQuality,
    pub observed_device_unix_ms: Option<i64>,
    pub prepared_package_id: Option<OpaqueId>,
}

/// Resolution status of a synchronization conflict (`FIELD-04`, `A13`, `A16`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictResolutionState {
    /// Both conflicting versions are retained; an explicit merge resolution is required.
    Unresolved,
    /// Resolved by an explicit new version linking both conflicting parent versions.
    Resolved {
        resolution_version: VersionId,
        resolved_by: ActorId,
        rationale: String,
    },
}

/// Explicit record of a concurrent/offline edit conflict (`COLLAB-01`, `FIELD-04`, `A13`, `A16`).
///
/// Neither the accepted in-case version nor the incoming offline version is erased.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncConflictRecord {
    conflict_id: OpaqueId,
    target_item_id: OpaqueId,
    expected_base_version: Option<VersionId>,
    current_head_version: VersionId,
    incoming_offline_version: VersionId,
    incoming_author: ActorId,
    resolution: ConflictResolutionState,
}

impl SyncConflictRecord {
    #[must_use]
    pub fn conflict_id(&self) -> &OpaqueId {
        &self.conflict_id
    }

    #[must_use]
    pub fn target_item_id(&self) -> &OpaqueId {
        &self.target_item_id
    }

    #[must_use]
    pub fn expected_base_version(&self) -> Option<&VersionId> {
        self.expected_base_version.as_ref()
    }

    #[must_use]
    pub fn current_head_version(&self) -> &VersionId {
        &self.current_head_version
    }

    #[must_use]
    pub fn incoming_offline_version(&self) -> &VersionId {
        &self.incoming_offline_version
    }

    #[must_use]
    pub fn incoming_author(&self) -> &ActorId {
        &self.incoming_author
    }

    #[must_use]
    pub fn resolution(&self) -> &ConflictResolutionState {
        &self.resolution
    }
}

/// Outcome of synchronizing an [`OfflineFieldCapture`] into a [`SharedCaseWorkspace`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncOutcome {
    /// Target was created or cleanly advanced from the expected `base_version`.
    AppliedCleanly {
        item_id: OpaqueId,
        new_head_version: VersionId,
        reference_freshness: Option<OfflineReferenceFreshness>,
    },
    /// Base version diverged, but content SHA-256 and origin were identical (`docs/architecture.md` §9).
    DeduplicatedExactBytes {
        item_id: OpaqueId,
        retained_head_version: VersionId,
        recorded_duplicate_version: VersionId,
    },
    /// Base version diverged with different content; both versions are preserved in history and
    /// a [`SyncConflictRecord`] is opened without overwriting the current head (`A13`, `A16`).
    ConflictPreserved {
        conflict_id: OpaqueId,
        item_id: OpaqueId,
        preserved_head_version: VersionId,
        preserved_offline_version: VersionId,
    },
}

/// Distinct delivery/sharing mode for a report or case snapshot (`COLLAB-03`, `A16`, `AGENTS.md` §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportDeliveryMode {
    /// Local draft save only; never exports, shares, or publishes.
    SavedDraftOnly,
    /// Private export for the actor; retains labels/citations and previews redactions without publishing.
    PrivateExportOnly,
    /// Shared with case collaborators according to target clearance; not publicly published.
    SharedWithCollaborators,
    /// Publicly issued snapshot (`Owner` only); immutable and flagged on downstream source corrections.
    PublishedSnapshot,
}

/// Notice recorded on an issued/shared report when a cited source or item version is superseded (`COLLAB-03`, `A16`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyChangeFlag {
    pub superseded_version: VersionId,
    pub replacement_version: VersionId,
    pub reason: String,
}

/// Immutable record of a saved draft, private export, collaborator share, or published report (`COLLAB-03`, `A16`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportSnapshotRecord {
    report_id: OpaqueId,
    report_version: VersionId,
    actor: ActorId,
    mode: ReportDeliveryMode,
    title: String,
    included_items: Vec<PermittedItemView>,
    redaction_preview: Vec<RedactionNotice>,
    cited_versions: Vec<VersionId>,
    needs_reassessment: bool,
    dependency_flags: Vec<DependencyChangeFlag>,
}

impl ReportSnapshotRecord {
    #[must_use]
    pub fn report_id(&self) -> &OpaqueId {
        &self.report_id
    }

    #[must_use]
    pub fn report_version(&self) -> &VersionId {
        &self.report_version
    }

    #[must_use]
    pub fn actor(&self) -> &ActorId {
        &self.actor
    }

    #[must_use]
    pub const fn mode(&self) -> ReportDeliveryMode {
        self.mode
    }

    /// Saving, exporting privately, and sharing with collaborators never mark a report as publicly published (`COLLAB-03`).
    #[must_use]
    pub const fn is_publicly_published(&self) -> bool {
        matches!(self.mode, ReportDeliveryMode::PublishedSnapshot)
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub fn included_items(&self) -> &[PermittedItemView] {
        &self.included_items
    }

    #[must_use]
    pub fn redaction_preview(&self) -> &[RedactionNotice] {
        &self.redaction_preview
    }

    #[must_use]
    pub fn cited_versions(&self) -> &[VersionId] {
        &self.cited_versions
    }

    #[must_use]
    pub const fn needs_reassessment(&self) -> bool {
        self.needs_reassessment
    }

    #[must_use]
    pub fn dependency_flags(&self) -> &[DependencyChangeFlag] {
        &self.dependency_flags
    }
}

/// Typed errors for shared-case authorization, review attribution, report delivery, and offline sync.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollaborationError {
    ZeroLimit(CollaborationLimitKind),
    LimitExceeded {
        kind: CollaborationLimitKind,
        limit: usize,
    },
    EmptyTextField(&'static str),
    ControlCharacterInText(&'static str),
    InvalidSha256Hex,
    InvalidTimeWindow {
        start_ms: i64,
        end_ms: i64,
    },
    InvalidByteSpan {
        start_byte: usize,
        end_byte: usize,
    },
    UnknownActor(ActorId),
    UnauthorizedRole {
        actor: ActorId,
        role: CaseRole,
        action: &'static str,
    },
    AccessDenied {
        actor: ActorId,
        item_id: OpaqueId,
        surface: AccessSurface,
        reason: RedactionReason,
    },
    CannotRemoveLastOwner,
    UnknownItem(OpaqueId),
    UnknownItemVersion {
        item_id: OpaqueId,
        version: VersionId,
    },
    DuplicateVersion(VersionId),
    DuplicateReviewId(OpaqueId),
    DuplicateReportVersion(VersionId),
    EmptyCitedVersions,
    InvalidSupersessionTarget(VersionId),
    OriginPromotionForbidden {
        item_id: OpaqueId,
    },
    UnknownPreparedPackage(OpaqueId),
    PreparedPackageActorMismatch {
        package_id: OpaqueId,
        expected: ActorId,
        actual: ActorId,
    },
    UnknownConflict(OpaqueId),
    ConflictAlreadyResolved(OpaqueId),
    ConflictParentMismatch {
        conflict_id: OpaqueId,
    },
}

impl fmt::Display for CollaborationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroLimit(kind) => write!(f, "collaboration limit {kind} must be >= 1"),
            Self::LimitExceeded { kind, limit } => {
                write!(f, "collaboration limit {kind} ({limit}) exceeded")
            }
            Self::EmptyTextField(name) => write!(f, "{name} must not be empty or whitespace"),
            Self::ControlCharacterInText(name) => {
                write!(f, "{name} contains disallowed control characters")
            }
            Self::InvalidSha256Hex => {
                f.write_str("content_sha256 must be 64 lowercase ASCII hexadecimal characters")
            }
            Self::InvalidTimeWindow { start_ms, end_ms } => {
                write!(
                    f,
                    "invalid time window [{start_ms}, {end_ms}): start must be < end"
                )
            }
            Self::InvalidByteSpan {
                start_byte,
                end_byte,
            } => {
                write!(
                    f,
                    "invalid passage byte span [{start_byte}, {end_byte}): start must be < end"
                )
            }
            Self::UnknownActor(actor) => {
                write!(f, "actor {actor} is not a collaborator on this case")
            }
            Self::UnauthorizedRole {
                actor,
                role,
                action,
            } => write!(
                f,
                "actor {actor} with role {role} is not authorized for {action}"
            ),
            Self::AccessDenied {
                actor,
                item_id,
                surface,
                reason,
            } => write!(
                f,
                "access denied for actor {actor} on item {item_id} via {surface}: {reason}"
            ),
            Self::CannotRemoveLastOwner => {
                f.write_str("shared case must retain at least one owner")
            }
            Self::UnknownItem(id) => write!(f, "case item {id} does not exist"),
            Self::UnknownItemVersion { item_id, version } => {
                write!(f, "version {version} does not exist on item {item_id}")
            }
            Self::DuplicateVersion(v) => write!(f, "version identifier {v} was already used"),
            Self::DuplicateReviewId(id) => {
                write!(f, "review or record identifier {id} already exists")
            }
            Self::DuplicateReportVersion(v) => write!(f, "report version {v} was already used"),
            Self::EmptyCitedVersions => {
                f.write_str("reviews and reports must cite at least one version")
            }
            Self::InvalidSupersessionTarget(v) => {
                write!(
                    f,
                    "superseded review version {v} does not exist on the target item"
                )
            }
            Self::OriginPromotionForbidden { item_id } => write!(
                f,
                "item {item_id} has a fixed origin class; simulated fixtures cannot become observed evidence"
            ),
            Self::UnknownPreparedPackage(id) => {
                write!(f, "prepared offline package {id} does not exist")
            }
            Self::PreparedPackageActorMismatch {
                package_id,
                expected,
                actual,
            } => write!(
                f,
                "prepared offline package {package_id} belongs to {expected}, not {actual}"
            ),
            Self::UnknownConflict(id) => write!(f, "sync conflict {id} does not exist"),
            Self::ConflictAlreadyResolved(id) => {
                write!(f, "sync conflict {id} has already been resolved")
            }
            Self::ConflictParentMismatch { conflict_id } => write!(
                f,
                "resolution for conflict {conflict_id} must reference both conflicting versions"
            ),
        }
    }
}

impl std::error::Error for CollaborationError {}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TrackedCaseItem {
    item_id: OpaqueId,
    sensitivity: SensitivityFlags,
    head_version: VersionId,
    versions: Vec<CaseItemVersion>,
}

fn validate_text(
    name: &'static str,
    value: &str,
    max_bytes: usize,
) -> Result<(), CollaborationError> {
    if value.len() > max_bytes {
        return Err(CollaborationError::LimitExceeded {
            kind: CollaborationLimitKind::TextBytes,
            limit: max_bytes,
        });
    }
    if value.trim().is_empty() {
        return Err(CollaborationError::EmptyTextField(name));
    }
    if value
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\r' | '\n' | '\t'))
    {
        return Err(CollaborationError::ControlCharacterInText(name));
    }
    Ok(())
}

fn validate_sha256_hex(digest: &str) -> Result<(), CollaborationError> {
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
    {
        return Err(CollaborationError::InvalidSha256Hex);
    }
    Ok(())
}

fn validate_origin_and_clock(
    origin: &EvidenceOriginKind,
    clock: &DeviceClockQuality,
    max_bytes: usize,
) -> Result<(), CollaborationError> {
    if let EvidenceOriginKind::LabeledSimulation {
        simulation_parameters,
    } = origin
    {
        validate_text("simulation_parameters", simulation_parameters, max_bytes)?;
    }
    if let DeviceClockQuality::UnreferencedLocalClock { uncertainty_reason } = clock {
        validate_text("uncertainty_reason", uncertainty_reason, max_bytes)?;
    }
    Ok(())
}

/// In-memory shared case workspace enforcing role capabilities, independent sensitivity
/// clearances, scoped review history, distinct save/export/share/publish actions, and
/// offline field synchronization with explicit conflict preservation (`#38`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedCaseWorkspace {
    case_id: OpaqueId,
    schema_version: SchemaVersion,
    limits: CollaborationLimits,
    collaborators: BTreeMap<ActorId, CollaboratorMembership>,
    items: BTreeMap<OpaqueId, TrackedCaseItem>,
    used_versions: BTreeSet<VersionId>,
    reviews: Vec<ScopedReviewRecord>,
    interpretations: Vec<CompetingInterpretation>,
    assignments: Vec<ReviewAssignment>,
    prepared_packages: BTreeMap<OpaqueId, PreparedOfflinePackage>,
    conflicts: BTreeMap<OpaqueId, SyncConflictRecord>,
    report_snapshots: Vec<ReportSnapshotRecord>,
}

impl SharedCaseWorkspace {
    /// Creates a new shared case workspace with an initial [`CaseRole::Owner`].
    pub fn new(
        case_id: OpaqueId,
        schema_version: SchemaVersion,
        initial_owner: ActorId,
        owner_clearance: ClearanceGrant,
        limits: CollaborationLimits,
    ) -> Self {
        let mut collaborators = BTreeMap::new();
        collaborators.insert(
            initial_owner.clone(),
            CollaboratorMembership {
                actor: initial_owner,
                role: CaseRole::Owner,
                clearance: owner_clearance,
            },
        );
        Self {
            case_id,
            schema_version,
            limits,
            collaborators,
            items: BTreeMap::new(),
            used_versions: BTreeSet::new(),
            reviews: Vec::new(),
            interpretations: Vec::new(),
            assignments: Vec::new(),
            prepared_packages: BTreeMap::new(),
            conflicts: BTreeMap::new(),
            report_snapshots: Vec::new(),
        }
    }

    #[must_use]
    pub fn case_id(&self) -> &OpaqueId {
        &self.case_id
    }

    #[must_use]
    pub const fn schema_version(&self) -> SchemaVersion {
        self.schema_version
    }

    #[must_use]
    pub const fn limits(&self) -> CollaborationLimits {
        self.limits
    }

    #[must_use]
    pub fn collaborator(&self, actor: &ActorId) -> Option<&CollaboratorMembership> {
        self.collaborators.get(actor)
    }

    #[must_use]
    pub fn reviews(&self) -> &[ScopedReviewRecord] {
        &self.reviews
    }

    #[must_use]
    pub fn competing_interpretations(&self) -> &[CompetingInterpretation] {
        &self.interpretations
    }

    #[must_use]
    pub fn assignments(&self) -> &[ReviewAssignment] {
        &self.assignments
    }

    #[must_use]
    pub fn conflicts(&self) -> &BTreeMap<OpaqueId, SyncConflictRecord> {
        &self.conflicts
    }

    #[must_use]
    pub fn report_snapshots(&self) -> &[ReportSnapshotRecord] {
        &self.report_snapshots
    }

    /// Returns all retained versions for an item if the requesting actor is authorized to inspect it.
    pub fn inspect_item_history(
        &self,
        actor: &ActorId,
        item_id: &OpaqueId,
    ) -> Result<&[CaseItemVersion], CollaborationError> {
        let membership = self.require_member(actor)?;
        let item = self
            .items
            .get(item_id)
            .ok_or_else(|| CollaborationError::UnknownItem(item_id.clone()))?;
        if let Some(reason) =
            Self::blocking_sensitivity_reason(membership.clearance, item.sensitivity)
        {
            return Err(CollaborationError::AccessDenied {
                actor: actor.clone(),
                item_id: item_id.clone(),
                surface: AccessSurface::DirectArtifactRequest,
                reason,
            });
        }
        Ok(&item.versions)
    }

    /// Adds or updates a collaborator's role and clearance (`CaseRole::Owner` required).
    pub fn set_collaborator_role(
        &mut self,
        acting_owner: &ActorId,
        target_actor: ActorId,
        role: CaseRole,
        clearance: ClearanceGrant,
    ) -> Result<(), CollaborationError> {
        let caller = self.require_member(acting_owner)?;
        if !caller.role.can_manage_case_and_publish() {
            return Err(CollaborationError::UnauthorizedRole {
                actor: acting_owner.clone(),
                role: caller.role,
                action: "set_collaborator_role",
            });
        }
        if !self.collaborators.contains_key(&target_actor)
            && self.collaborators.len() >= self.limits.max_collaborators
        {
            return Err(CollaborationError::LimitExceeded {
                kind: CollaborationLimitKind::Collaborators,
                limit: self.limits.max_collaborators,
            });
        }
        if let Some(existing) = self.collaborators.get(&target_actor)
            && existing.role == CaseRole::Owner
            && role != CaseRole::Owner
        {
            let owner_count = self
                .collaborators
                .values()
                .filter(|m| m.role == CaseRole::Owner)
                .count();
            if owner_count <= 1 {
                return Err(CollaborationError::CannotRemoveLastOwner);
            }
        }
        self.collaborators.insert(
            target_actor.clone(),
            CollaboratorMembership {
                actor: target_actor,
                role,
                clearance,
            },
        );
        Ok(())
    }

    /// Registers a new case item with its initial version (`Owner` or `Editor` required).
    pub fn add_case_item(
        &mut self,
        actor: &ActorId,
        input: CaseItemInput,
    ) -> Result<(), CollaborationError> {
        let membership = self.require_member(actor)?;
        if !membership.role.can_edit_evidence() {
            return Err(CollaborationError::UnauthorizedRole {
                actor: actor.clone(),
                role: membership.role,
                action: "add_case_item",
            });
        }
        if self.items.contains_key(&input.item_id) {
            return Err(CollaborationError::DuplicateReviewId(input.item_id));
        }
        if self.items.len() >= self.limits.max_items {
            return Err(CollaborationError::LimitExceeded {
                kind: CollaborationLimitKind::Items,
                limit: self.limits.max_items,
            });
        }
        if self.used_versions.contains(&input.version) {
            return Err(CollaborationError::DuplicateVersion(input.version));
        }
        validate_text(
            "summary_text",
            &input.summary_text,
            self.limits.max_text_bytes,
        )?;
        if let Some(loc) = &input.sensitive_location_detail {
            validate_text("sensitive_location_detail", loc, self.limits.max_text_bytes)?;
        }
        validate_sha256_hex(&input.content_sha256)?;
        validate_origin_and_clock(
            &input.origin,
            &input.clock_quality,
            self.limits.max_text_bytes,
        )?;

        let head_version = input.version.clone();
        let initial_version = CaseItemVersion {
            version: input.version.clone(),
            author: actor.clone(),
            parent_versions: Vec::new(),
            summary_text: input.summary_text,
            sensitive_location_detail: input.sensitive_location_detail,
            content_sha256: input.content_sha256,
            origin: input.origin,
            clock_quality: input.clock_quality,
            reference_freshness: None,
        };
        self.used_versions.insert(input.version);
        self.items.insert(
            input.item_id.clone(),
            TrackedCaseItem {
                item_id: input.item_id,
                sensitivity: input.sensitivity,
                head_version,
                versions: vec![initial_version],
            },
        );
        Ok(())
    }

    /// Requests a single item on a specific [`AccessSurface`], enforcing both role and independent
    /// sensitivity clearance (`COLLAB-01`, `A16`, `AGENTS.md` §5).
    pub fn request_item_on_surface(
        &self,
        actor: &ActorId,
        item_id: &OpaqueId,
        surface: AccessSurface,
    ) -> Result<PermittedItemView, CollaborationError> {
        let membership = self.require_member(actor)?;
        let item = self
            .items
            .get(item_id)
            .ok_or_else(|| CollaborationError::UnknownItem(item_id.clone()))?;
        self.project_item_for_clearance(actor, membership.clearance, item, surface)
            .map_err(|notice| CollaborationError::AccessDenied {
                actor: actor.clone(),
                item_id: notice.item_id,
                surface,
                reason: notice.reason,
            })
    }

    /// Filters all items in the case for `actor` on `surface` (`Search`, `BlobRead`, `Subscription`,
    /// `ModelInput`, `ReportGeneration`, etc.), stripping unpermitted sensitive items and sensitive
    /// location fields while recording explicit [`RedactionNotice`]s (`A16`, `AGENTS.md` §5).
    pub fn query_surface(
        &self,
        actor: &ActorId,
        surface: AccessSurface,
    ) -> Result<SurfaceQueryEnvelope, CollaborationError> {
        let membership = self.require_member(actor)?;
        let (permitted_items, redaction_notices) =
            self.filter_items_by_clearance(actor, membership.clearance, surface, None)?;
        Ok(SurfaceQueryEnvelope {
            actor: actor.clone(),
            surface,
            permitted_items,
            redaction_notices,
        })
    }

    /// Submits an author- and scope-attributed review without overwriting prior reviews (`COLLAB-01`).
    pub fn submit_scoped_review(
        &mut self,
        actor: &ActorId,
        input: ScopedReviewInput,
    ) -> Result<ScopedReviewRecord, CollaborationError> {
        let membership = self.require_member(actor)?;
        if !membership.role.can_submit_review() {
            return Err(CollaborationError::UnauthorizedRole {
                actor: actor.clone(),
                role: membership.role,
                action: "submit_scoped_review",
            });
        }
        if self.reviews.len() >= self.limits.max_reviews_and_notes {
            return Err(CollaborationError::LimitExceeded {
                kind: CollaborationLimitKind::ReviewsAndNotes,
                limit: self.limits.max_reviews_and_notes,
            });
        }
        if self.used_versions.contains(&input.review_version) {
            return Err(CollaborationError::DuplicateVersion(input.review_version));
        }
        // Actor must have access to the target item to review it.
        let _ = self.request_item_on_surface(
            actor,
            &input.target_item_id,
            AccessSurface::DirectArtifactRequest,
        )?;
        let item = self
            .items
            .get(&input.target_item_id)
            .ok_or_else(|| CollaborationError::UnknownItem(input.target_item_id.clone()))?;
        let target_ver = item
            .versions
            .iter()
            .find(|v| v.version == input.target_version)
            .ok_or_else(|| CollaborationError::UnknownItemVersion {
                item_id: input.target_item_id.clone(),
                version: input.target_version.clone(),
            })?;

        match &input.scope {
            ReviewScope::PassageByteRange {
                start_byte,
                end_byte,
            } => {
                if start_byte >= end_byte || *end_byte > target_ver.summary_text.len() {
                    return Err(CollaborationError::InvalidByteSpan {
                        start_byte: *start_byte,
                        end_byte: *end_byte,
                    });
                }
            }
            ReviewScope::TemporalWindow {
                start_unix_ms,
                end_unix_ms,
            } => {
                if start_unix_ms >= end_unix_ms {
                    return Err(CollaborationError::InvalidTimeWindow {
                        start_ms: *start_unix_ms,
                        end_ms: *end_unix_ms,
                    });
                }
            }
            ReviewScope::WholeTargetVersion
            | ReviewScope::CalibrationAndGeometryOnly
            | ReviewScope::MethodologyOnly => {}
        }

        validate_text("rationale", &input.rationale, self.limits.max_text_bytes)?;
        if input.cited_evidence_versions.is_empty() {
            return Err(CollaborationError::EmptyCitedVersions);
        }
        if let Some(superseded) = &input.supersedes_review_version
            && !self.reviews.iter().any(|r| {
                &r.review_version == superseded && r.target_item_id == input.target_item_id
            })
        {
            return Err(CollaborationError::InvalidSupersessionTarget(
                superseded.clone(),
            ));
        }

        let record = ScopedReviewRecord {
            review_id: input.review_id,
            review_version: input.review_version.clone(),
            author: actor.clone(),
            author_role: membership.role,
            target_item_id: input.target_item_id.clone(),
            target_version: input.target_version.clone(),
            scope: input.scope,
            disposition: input.disposition,
            rationale: input.rationale,
            cited_evidence_versions: input.cited_evidence_versions,
            supersedes_review_version: input.supersedes_review_version,
        };
        self.used_versions.insert(input.review_version);
        self.reviews.push(record.clone());

        for assignment in &mut self.assignments {
            if &assignment.assignee == actor
                && assignment.target_item_id == input.target_item_id
                && assignment.target_version == input.target_version
            {
                assignment.status = AssignmentStatus::CompletedWithReview;
            }
        }

        Ok(record)
    }

    /// Records a competing analyst interpretation alongside existing interpretations (`COLLAB-01`).
    #[allow(clippy::too_many_arguments)]
    pub fn add_competing_interpretation(
        &mut self,
        actor: &ActorId,
        interpretation_id: OpaqueId,
        version: VersionId,
        target_item_id: OpaqueId,
        hypothesis: String,
        supporting_versions: Vec<VersionId>,
        contradicting_versions: Vec<VersionId>,
        explicit_assumptions: String,
    ) -> Result<CompetingInterpretation, CollaborationError> {
        let membership = self.require_member(actor)?;
        if !membership.role.can_submit_review() {
            return Err(CollaborationError::UnauthorizedRole {
                actor: actor.clone(),
                role: membership.role,
                action: "add_competing_interpretation",
            });
        }
        if self.interpretations.len() >= self.limits.max_reviews_and_notes {
            return Err(CollaborationError::LimitExceeded {
                kind: CollaborationLimitKind::ReviewsAndNotes,
                limit: self.limits.max_reviews_and_notes,
            });
        }
        if self.used_versions.contains(&version) {
            return Err(CollaborationError::DuplicateVersion(version));
        }
        let _ = self.request_item_on_surface(
            actor,
            &target_item_id,
            AccessSurface::DirectArtifactRequest,
        )?;
        validate_text("hypothesis", &hypothesis, self.limits.max_text_bytes)?;
        validate_text(
            "explicit_assumptions",
            &explicit_assumptions,
            self.limits.max_text_bytes,
        )?;
        if supporting_versions.is_empty() {
            return Err(CollaborationError::EmptyCitedVersions);
        }

        let record = CompetingInterpretation {
            interpretation_id,
            version: version.clone(),
            author: actor.clone(),
            target_item_id,
            hypothesis,
            supporting_versions,
            contradicting_versions,
            explicit_assumptions,
        };
        self.used_versions.insert(version);
        self.interpretations.push(record.clone());
        Ok(record)
    }

    /// Assigns a review task on a specific item version to a collaborator (`Owner` or `Editor` required).
    pub fn assign_review(
        &mut self,
        assigner: &ActorId,
        assignment_id: OpaqueId,
        assignee: ActorId,
        target_item_id: OpaqueId,
        target_version: VersionId,
        instructions: String,
    ) -> Result<ReviewAssignment, CollaborationError> {
        let caller = self.require_member(assigner)?;
        if !caller.role.can_edit_evidence() {
            return Err(CollaborationError::UnauthorizedRole {
                actor: assigner.clone(),
                role: caller.role,
                action: "assign_review",
            });
        }
        let target_member = self.require_member(&assignee)?;
        if !target_member.role.can_submit_review() {
            return Err(CollaborationError::UnauthorizedRole {
                actor: assignee,
                role: target_member.role,
                action: "receive_review_assignment",
            });
        }
        if self.assignments.len() >= self.limits.max_reviews_and_notes {
            return Err(CollaborationError::LimitExceeded {
                kind: CollaborationLimitKind::ReviewsAndNotes,
                limit: self.limits.max_reviews_and_notes,
            });
        }
        if self
            .assignments
            .iter()
            .any(|a| a.assignment_id == assignment_id)
        {
            return Err(CollaborationError::DuplicateReviewId(assignment_id));
        }
        let item = self
            .items
            .get(&target_item_id)
            .ok_or_else(|| CollaborationError::UnknownItem(target_item_id.clone()))?;
        if !item.versions.iter().any(|v| v.version == target_version) {
            return Err(CollaborationError::UnknownItemVersion {
                item_id: target_item_id,
                version: target_version,
            });
        }
        validate_text("instructions", &instructions, self.limits.max_text_bytes)?;

        let record = ReviewAssignment {
            assignment_id,
            assigned_by: assigner.clone(),
            assignee,
            target_item_id,
            target_version,
            instructions,
            status: AssignmentStatus::Open,
        };
        self.assignments.push(record.clone());
        Ok(record)
    }

    /// Prepares a bounded offline field package filtered by the actor's clearance (`FIELD-04`).
    pub fn prepare_offline_package(
        &mut self,
        actor: &ActorId,
        package_id: OpaqueId,
        prepared_at_unix_ms: i64,
        expires_at_unix_ms: i64,
        selected_items: Option<&[OpaqueId]>,
    ) -> Result<PreparedOfflinePackage, CollaborationError> {
        let membership = self.require_member(actor)?;
        if prepared_at_unix_ms >= expires_at_unix_ms {
            return Err(CollaborationError::InvalidTimeWindow {
                start_ms: prepared_at_unix_ms,
                end_ms: expires_at_unix_ms,
            });
        }
        if self.prepared_packages.contains_key(&package_id) {
            return Err(CollaborationError::DuplicateReviewId(package_id));
        }
        let (included_items, omitted_notices) = self.filter_items_by_clearance(
            actor,
            membership.clearance,
            AccessSurface::OfflinePreparation,
            selected_items,
        )?;
        let pkg = PreparedOfflinePackage {
            package_id: package_id.clone(),
            case_id: self.case_id.clone(),
            prepared_for: actor.clone(),
            prepared_at_unix_ms,
            expires_at_unix_ms,
            included_items,
            omitted_notices,
        };
        self.prepared_packages.insert(package_id, pkg.clone());
        Ok(pkg)
    }

    /// Synchronizes an offline field capture or offline edit into the workspace (`FIELD-04`, `A13`, `A16`).
    ///
    /// - Preserves actor, base version, local device clock uncertainty, cached/expired reference state,
    ///   and fixed `ObservedOrReported` vs `LabeledSimulation` origin.
    /// - Never overwrites concurrent edits or auto-merges differing semantic text: if `base_version`
    ///   does not match the item's current head version (and the edit is not an exact-byte duplicate),
    ///   both versions are retained in the item's version history and a [`SyncConflictRecord`] is opened.
    pub fn synchronize_offline_capture(
        &mut self,
        actor: &ActorId,
        capture: OfflineFieldCapture,
    ) -> Result<SyncOutcome, CollaborationError> {
        let membership = self.require_member(actor)?;
        if !membership.role.can_edit_evidence() {
            return Err(CollaborationError::UnauthorizedRole {
                actor: actor.clone(),
                role: membership.role,
                action: "synchronize_offline_capture",
            });
        }
        if self.used_versions.contains(&capture.new_version) {
            return Err(CollaborationError::DuplicateVersion(capture.new_version));
        }
        validate_text(
            "summary_text",
            &capture.summary_text,
            self.limits.max_text_bytes,
        )?;
        if let Some(loc) = &capture.sensitive_location_detail {
            validate_text("sensitive_location_detail", loc, self.limits.max_text_bytes)?;
        }
        validate_sha256_hex(&capture.content_sha256)?;
        validate_origin_and_clock(
            &capture.origin,
            &capture.clock_quality,
            self.limits.max_text_bytes,
        )?;

        let reference_freshness = if let Some(pkg_id) = &capture.prepared_package_id {
            let pkg = self
                .prepared_packages
                .get(pkg_id)
                .ok_or_else(|| CollaborationError::UnknownPreparedPackage(pkg_id.clone()))?;
            if &pkg.prepared_for != actor {
                return Err(CollaborationError::PreparedPackageActorMismatch {
                    package_id: pkg_id.clone(),
                    expected: pkg.prepared_for.clone(),
                    actual: actor.clone(),
                });
            }
            Some(pkg.evaluate_freshness(capture.observed_device_unix_ms, &capture.clock_quality))
        } else {
            None
        };

        if !self.items.contains_key(&capture.target_item_id) {
            if let Some(missing_base) = capture.base_version {
                return Err(CollaborationError::UnknownItemVersion {
                    item_id: capture.target_item_id,
                    version: missing_base,
                });
            }
            if self.items.len() >= self.limits.max_items {
                return Err(CollaborationError::LimitExceeded {
                    kind: CollaborationLimitKind::Items,
                    limit: self.limits.max_items,
                });
            }
            let head = capture.new_version.clone();
            let ver = CaseItemVersion {
                version: capture.new_version.clone(),
                author: actor.clone(),
                parent_versions: Vec::new(),
                summary_text: capture.summary_text,
                sensitive_location_detail: capture.sensitive_location_detail,
                content_sha256: capture.content_sha256,
                origin: capture.origin,
                clock_quality: capture.clock_quality,
                reference_freshness,
            };
            self.used_versions.insert(capture.new_version);
            self.items.insert(
                capture.target_item_id.clone(),
                TrackedCaseItem {
                    item_id: capture.target_item_id.clone(),
                    sensitivity: capture.sensitivity,
                    head_version: head.clone(),
                    versions: vec![ver],
                },
            );
            return Ok(SyncOutcome::AppliedCleanly {
                item_id: capture.target_item_id,
                new_head_version: head,
                reference_freshness,
            });
        }

        let item = self
            .items
            .get_mut(&capture.target_item_id)
            .ok_or_else(|| CollaborationError::UnknownItem(capture.target_item_id.clone()))?;
        if item.versions.len() >= self.limits.max_versions_per_item {
            return Err(CollaborationError::LimitExceeded {
                kind: CollaborationLimitKind::VersionsPerItem,
                limit: self.limits.max_versions_per_item,
            });
        }

        // Fixed origin class: a simulated item cannot become observed evidence, nor vice versa.
        if let Some(first) = item.versions.first()
            && first.origin.is_simulation() != capture.origin.is_simulation()
        {
            return Err(CollaborationError::OriginPromotionForbidden {
                item_id: capture.target_item_id,
            });
        }

        if let Some(base) = &capture.base_version
            && !item.versions.iter().any(|v| &v.version == base)
        {
            return Err(CollaborationError::UnknownItemVersion {
                item_id: capture.target_item_id,
                version: base.clone(),
            });
        }

        let current_head = item.head_version.clone();
        let parent_versions = capture.base_version.clone().into_iter().collect::<Vec<_>>();
        let incoming_ver = CaseItemVersion {
            version: capture.new_version.clone(),
            author: actor.clone(),
            parent_versions,
            summary_text: capture.summary_text,
            sensitive_location_detail: capture.sensitive_location_detail,
            content_sha256: capture.content_sha256,
            origin: capture.origin,
            clock_quality: capture.clock_quality,
            reference_freshness,
        };

        if capture.base_version.as_ref() == Some(&current_head) {
            item.head_version = capture.new_version.clone();
            item.sensitivity = SensitivityFlags {
                unpublished_draft: item.sensitivity.unpublished_draft
                    || capture.sensitivity.unpublished_draft,
                sensitive_artifact: item.sensitivity.sensitive_artifact
                    || capture.sensitivity.sensitive_artifact,
                sensitive_location: item.sensitivity.sensitive_location
                    || capture.sensitivity.sensitive_location,
            };
            item.versions.push(incoming_ver);
            self.used_versions.insert(capture.new_version.clone());
            return Ok(SyncOutcome::AppliedCleanly {
                item_id: capture.target_item_id,
                new_head_version: capture.new_version,
                reference_freshness,
            });
        }

        // Concurrent edit detected: check exact-byte equality first (`docs/architecture.md` §9).
        let head_record = item
            .versions
            .iter()
            .find(|v| v.version == current_head)
            .ok_or_else(|| CollaborationError::UnknownItemVersion {
                item_id: capture.target_item_id.clone(),
                version: current_head.clone(),
            })?;
        if head_record.content_sha256 == incoming_ver.content_sha256
            && head_record.summary_text == incoming_ver.summary_text
            && head_record.sensitive_location_detail == incoming_ver.sensitive_location_detail
            && head_record.origin == incoming_ver.origin
        {
            item.versions.push(incoming_ver);
            self.used_versions.insert(capture.new_version.clone());
            return Ok(SyncOutcome::DeduplicatedExactBytes {
                item_id: capture.target_item_id,
                retained_head_version: current_head,
                recorded_duplicate_version: capture.new_version,
            });
        }

        // Preserve both versions in history and record an explicit conflict (`COLLAB-01`, `FIELD-04`, `A13`, `A16`).
        if self.conflicts.contains_key(&capture.capture_id) {
            return Err(CollaborationError::DuplicateReviewId(capture.capture_id));
        }
        item.sensitivity = SensitivityFlags {
            unpublished_draft: item.sensitivity.unpublished_draft
                || capture.sensitivity.unpublished_draft,
            sensitive_artifact: item.sensitivity.sensitive_artifact
                || capture.sensitivity.sensitive_artifact,
            sensitive_location: item.sensitivity.sensitive_location
                || capture.sensitivity.sensitive_location,
        };
        item.versions.push(incoming_ver);
        self.used_versions.insert(capture.new_version.clone());

        let conflict = SyncConflictRecord {
            conflict_id: capture.capture_id.clone(),
            target_item_id: capture.target_item_id.clone(),
            expected_base_version: capture.base_version,
            current_head_version: current_head.clone(),
            incoming_offline_version: capture.new_version.clone(),
            incoming_author: actor.clone(),
            resolution: ConflictResolutionState::Unresolved,
        };
        self.conflicts.insert(capture.capture_id.clone(), conflict);

        Ok(SyncOutcome::ConflictPreserved {
            conflict_id: capture.capture_id,
            item_id: capture.target_item_id,
            preserved_head_version: current_head,
            preserved_offline_version: capture.new_version,
        })
    }

    /// Resolves an open [`SyncConflictRecord`] by appending a new merged version that explicitly
    /// links both conflicting parent versions without erasing either (`FIELD-04`, `A13`, `A16`).
    #[allow(clippy::too_many_arguments)]
    pub fn resolve_sync_conflict(
        &mut self,
        actor: &ActorId,
        conflict_id: &OpaqueId,
        resolution_version: VersionId,
        parent_versions: (VersionId, VersionId),
        merged_summary_text: String,
        merged_sensitive_location: Option<String>,
        merged_content_sha256: String,
        clock_quality: DeviceClockQuality,
        rationale: String,
    ) -> Result<(), CollaborationError> {
        let membership = self.require_member(actor)?;
        if !membership.role.can_edit_evidence() {
            return Err(CollaborationError::UnauthorizedRole {
                actor: actor.clone(),
                role: membership.role,
                action: "resolve_sync_conflict",
            });
        }
        if self.used_versions.contains(&resolution_version) {
            return Err(CollaborationError::DuplicateVersion(resolution_version));
        }
        validate_text(
            "merged_summary_text",
            &merged_summary_text,
            self.limits.max_text_bytes,
        )?;
        if let Some(loc) = &merged_sensitive_location {
            validate_text("merged_sensitive_location", loc, self.limits.max_text_bytes)?;
        }
        validate_text("rationale", &rationale, self.limits.max_text_bytes)?;
        validate_sha256_hex(&merged_content_sha256)?;

        let conflict = self
            .conflicts
            .get(conflict_id)
            .ok_or_else(|| CollaborationError::UnknownConflict(conflict_id.clone()))?;
        if matches!(
            conflict.resolution,
            ConflictResolutionState::Resolved { .. }
        ) {
            return Err(CollaborationError::ConflictAlreadyResolved(
                conflict_id.clone(),
            ));
        }
        let matches_parents = (parent_versions.0 == conflict.current_head_version
            && parent_versions.1 == conflict.incoming_offline_version)
            || (parent_versions.1 == conflict.current_head_version
                && parent_versions.0 == conflict.incoming_offline_version);
        if !matches_parents {
            return Err(CollaborationError::ConflictParentMismatch {
                conflict_id: conflict_id.clone(),
            });
        }

        let target_id = conflict.target_item_id.clone();
        let item = self
            .items
            .get_mut(&target_id)
            .ok_or_else(|| CollaborationError::UnknownItem(target_id.clone()))?;
        if item.versions.len() >= self.limits.max_versions_per_item {
            return Err(CollaborationError::LimitExceeded {
                kind: CollaborationLimitKind::VersionsPerItem,
                limit: self.limits.max_versions_per_item,
            });
        }
        let origin = item
            .versions
            .first()
            .map(|v| v.origin.clone())
            .unwrap_or(EvidenceOriginKind::ObservedOrReported);
        validate_origin_and_clock(&origin, &clock_quality, self.limits.max_text_bytes)?;

        let merged_record = CaseItemVersion {
            version: resolution_version.clone(),
            author: actor.clone(),
            parent_versions: vec![parent_versions.0, parent_versions.1],
            summary_text: merged_summary_text,
            sensitive_location_detail: merged_sensitive_location,
            content_sha256: merged_content_sha256,
            origin,
            clock_quality,
            reference_freshness: None,
        };
        item.head_version = resolution_version.clone();
        item.versions.push(merged_record);
        self.used_versions.insert(resolution_version.clone());

        if let Some(conflict_mut) = self.conflicts.get_mut(conflict_id) {
            conflict_mut.resolution = ConflictResolutionState::Resolved {
                resolution_version,
                resolved_by: actor.clone(),
                rationale,
            };
        }
        Ok(())
    }

    /// Saves a report draft locally without exporting, sharing, or publishing (`COLLAB-03`, `A16`).
    pub fn save_report_draft(
        &mut self,
        actor: &ActorId,
        report_id: OpaqueId,
        report_version: VersionId,
        title: String,
        selected_items: &[OpaqueId],
    ) -> Result<ReportSnapshotRecord, CollaborationError> {
        let membership = self.require_member(actor)?;
        if !membership.role.can_edit_evidence() {
            return Err(CollaborationError::UnauthorizedRole {
                actor: actor.clone(),
                role: membership.role,
                action: "save_report_draft",
            });
        }
        self.record_report_transition(
            actor,
            membership.clearance,
            report_id,
            report_version,
            ReportDeliveryMode::SavedDraftOnly,
            title,
            selected_items,
        )
    }

    /// Exports a report privately for the actor with a redaction preview and preserved labels,
    /// without sharing with other collaborators or publishing (`COLLAB-03`, `A16`).
    pub fn export_report_privately(
        &mut self,
        actor: &ActorId,
        report_id: OpaqueId,
        report_version: VersionId,
        title: String,
        selected_items: &[OpaqueId],
    ) -> Result<ReportSnapshotRecord, CollaborationError> {
        let membership = self.require_member(actor)?;
        if !membership.role.can_submit_review() {
            return Err(CollaborationError::UnauthorizedRole {
                actor: actor.clone(),
                role: membership.role,
                action: "export_report_privately",
            });
        }
        self.record_report_transition(
            actor,
            membership.clearance,
            report_id,
            report_version,
            ReportDeliveryMode::PrivateExportOnly,
            title,
            selected_items,
        )
    }

    /// Shares a report with case collaborators filtered to `recipient_clearance`, distinct from
    /// public publication (`COLLAB-03`, `A16`).
    pub fn share_report_with_collaborators(
        &mut self,
        actor: &ActorId,
        report_id: OpaqueId,
        report_version: VersionId,
        title: String,
        selected_items: &[OpaqueId],
        recipient_clearance: ClearanceGrant,
    ) -> Result<ReportSnapshotRecord, CollaborationError> {
        let membership = self.require_member(actor)?;
        if !membership.role.can_edit_evidence() {
            return Err(CollaborationError::UnauthorizedRole {
                actor: actor.clone(),
                role: membership.role,
                action: "share_report_with_collaborators",
            });
        }
        let effective_clearance = ClearanceGrant {
            allow_unpublished_drafts: membership.clearance.allow_unpublished_drafts
                && recipient_clearance.allow_unpublished_drafts,
            allow_sensitive_artifacts: membership.clearance.allow_sensitive_artifacts
                && recipient_clearance.allow_sensitive_artifacts,
            allow_sensitive_locations: membership.clearance.allow_sensitive_locations
                && recipient_clearance.allow_sensitive_locations,
        };
        self.record_report_transition(
            actor,
            effective_clearance,
            report_id,
            report_version,
            ReportDeliveryMode::SharedWithCollaborators,
            title,
            selected_items,
        )
    }

    /// Publishes an immutable report snapshot (`CaseRole::Owner` required), enforcing public
    /// redaction (`ClearanceGrant::standard_only()`) so unpublished drafts, sensitive artifacts,
    /// and sensitive locations are never leaked (`COLLAB-03`, `A16`).
    pub fn publish_report_snapshot(
        &mut self,
        actor: &ActorId,
        report_id: OpaqueId,
        report_version: VersionId,
        title: String,
        selected_items: &[OpaqueId],
    ) -> Result<ReportSnapshotRecord, CollaborationError> {
        let membership = self.require_member(actor)?;
        if !membership.role.can_manage_case_and_publish() {
            return Err(CollaborationError::UnauthorizedRole {
                actor: actor.clone(),
                role: membership.role,
                action: "publish_report_snapshot",
            });
        }
        self.record_report_transition(
            actor,
            ClearanceGrant::standard_only(),
            report_id,
            report_version,
            ReportDeliveryMode::PublishedSnapshot,
            title,
            selected_items,
        )
    }

    /// Flags all existing report snapshots that cite `superseded_version` for reassessment
    /// without overwriting or mutating their previously issued contents or version identifiers
    /// (`COLLAB-03`, `A16`, `docs/architecture.md` §2.3).
    pub fn flag_dependent_reports_on_correction(
        &mut self,
        actor: &ActorId,
        superseded_version: VersionId,
        replacement_version: VersionId,
        reason: String,
    ) -> Result<usize, CollaborationError> {
        let membership = self.require_member(actor)?;
        if !membership.role.can_edit_evidence() {
            return Err(CollaborationError::UnauthorizedRole {
                actor: actor.clone(),
                role: membership.role,
                action: "flag_dependent_reports_on_correction",
            });
        }
        validate_text("reason", &reason, self.limits.max_text_bytes)?;
        let mut flagged_count = 0usize;
        for snapshot in &mut self.report_snapshots {
            if snapshot.cited_versions.contains(&superseded_version) {
                snapshot.needs_reassessment = true;
                snapshot.dependency_flags.push(DependencyChangeFlag {
                    superseded_version: superseded_version.clone(),
                    replacement_version: replacement_version.clone(),
                    reason: reason.clone(),
                });
                flagged_count += 1;
            }
        }
        Ok(flagged_count)
    }

    fn require_member(
        &self,
        actor: &ActorId,
    ) -> Result<&CollaboratorMembership, CollaborationError> {
        self.collaborators
            .get(actor)
            .ok_or_else(|| CollaborationError::UnknownActor(actor.clone()))
    }

    const fn blocking_sensitivity_reason(
        clearance: ClearanceGrant,
        sensitivity: SensitivityFlags,
    ) -> Option<RedactionReason> {
        if sensitivity.unpublished_draft && !clearance.allow_unpublished_drafts {
            return Some(RedactionReason::UnpublishedDraftRestricted);
        }
        if sensitivity.sensitive_artifact && !clearance.allow_sensitive_artifacts {
            return Some(RedactionReason::SensitiveArtifactRestricted);
        }
        None
    }

    fn project_item_for_clearance(
        &self,
        _actor: &ActorId,
        clearance: ClearanceGrant,
        item: &TrackedCaseItem,
        surface: AccessSurface,
    ) -> Result<PermittedItemView, RedactionNotice> {
        if let Some(reason) = Self::blocking_sensitivity_reason(clearance, item.sensitivity) {
            return Err(RedactionNotice {
                item_id: item.item_id.clone(),
                version: item.head_version.clone(),
                surface,
                reason,
            });
        }
        let head = item
            .versions
            .iter()
            .find(|v| v.version == item.head_version)
            .or_else(|| item.versions.last());
        let Some(head) = head else {
            return Err(RedactionNotice {
                item_id: item.item_id.clone(),
                version: item.head_version.clone(),
                surface,
                reason: RedactionReason::UnpublishedDraftRestricted,
            });
        };

        let strip_location = (item.sensitivity.sensitive_location
            || head.sensitive_location_detail.is_some())
            && !clearance.allow_sensitive_locations;
        let sensitive_location_detail = if strip_location {
            None
        } else {
            head.sensitive_location_detail.clone()
        };
        let redacted_sensitive_location =
            strip_location && head.sensitive_location_detail.is_some();

        Ok(PermittedItemView {
            item_id: item.item_id.clone(),
            version: head.version.clone(),
            summary_text: head.summary_text.clone(),
            sensitive_location_detail,
            redacted_sensitive_location,
            content_sha256: head.content_sha256.clone(),
            visible_simulation_label: head.origin.required_visible_label(),
            origin: head.origin.clone(),
            clock_quality: head.clock_quality.clone(),
            reference_freshness: head.reference_freshness,
        })
    }

    fn filter_items_by_clearance(
        &self,
        actor: &ActorId,
        clearance: ClearanceGrant,
        surface: AccessSurface,
        selected_ids: Option<&[OpaqueId]>,
    ) -> Result<(Vec<PermittedItemView>, Vec<RedactionNotice>), CollaborationError> {
        let mut permitted = Vec::new();
        let mut notices = Vec::new();

        if let Some(ids) = selected_ids {
            if ids.is_empty() {
                return Err(CollaborationError::EmptyCitedVersions);
            }
            for id in ids {
                let item = self
                    .items
                    .get(id)
                    .ok_or_else(|| CollaborationError::UnknownItem(id.clone()))?;
                match self.project_item_for_clearance(actor, clearance, item, surface) {
                    Ok(view) => {
                        if view.redacted_sensitive_location {
                            notices.push(RedactionNotice {
                                item_id: view.item_id.clone(),
                                version: view.version.clone(),
                                surface,
                                reason: RedactionReason::SensitiveLocationStripped,
                            });
                        }
                        permitted.push(view);
                    }
                    Err(notice) => notices.push(notice),
                }
            }
        } else {
            for item in self.items.values() {
                match self.project_item_for_clearance(actor, clearance, item, surface) {
                    Ok(view) => {
                        if view.redacted_sensitive_location {
                            notices.push(RedactionNotice {
                                item_id: view.item_id.clone(),
                                version: view.version.clone(),
                                surface,
                                reason: RedactionReason::SensitiveLocationStripped,
                            });
                        }
                        permitted.push(view);
                    }
                    Err(notice) => notices.push(notice),
                }
            }
        }

        Ok((permitted, notices))
    }

    #[allow(clippy::too_many_arguments)]
    fn record_report_transition(
        &mut self,
        actor: &ActorId,
        effective_clearance: ClearanceGrant,
        report_id: OpaqueId,
        report_version: VersionId,
        mode: ReportDeliveryMode,
        title: String,
        selected_items: &[OpaqueId],
    ) -> Result<ReportSnapshotRecord, CollaborationError> {
        if self.report_snapshots.len() >= self.limits.max_report_snapshots {
            return Err(CollaborationError::LimitExceeded {
                kind: CollaborationLimitKind::ReportSnapshots,
                limit: self.limits.max_report_snapshots,
            });
        }
        if self.used_versions.contains(&report_version) {
            return Err(CollaborationError::DuplicateReportVersion(report_version));
        }
        validate_text("title", &title, self.limits.max_text_bytes)?;
        let surface = match mode {
            ReportDeliveryMode::SavedDraftOnly => AccessSurface::ReportGeneration,
            ReportDeliveryMode::PrivateExportOnly
            | ReportDeliveryMode::SharedWithCollaborators
            | ReportDeliveryMode::PublishedSnapshot => AccessSurface::Export,
        };
        let (included_items, redaction_preview) = self.filter_items_by_clearance(
            actor,
            effective_clearance,
            surface,
            Some(selected_items),
        )?;
        let cited_versions = included_items
            .iter()
            .map(|item| item.version.clone())
            .collect::<Vec<_>>();
        if cited_versions.is_empty() {
            return Err(CollaborationError::EmptyCitedVersions);
        }

        let snapshot = ReportSnapshotRecord {
            report_id,
            report_version: report_version.clone(),
            actor: actor.clone(),
            mode,
            title,
            included_items,
            redaction_preview,
            cited_versions,
            needs_reassessment: false,
            dependency_flags: Vec::new(),
        };
        self.used_versions.insert(report_version);
        self.report_snapshots.push(snapshot.clone());
        Ok(snapshot)
    }
}
