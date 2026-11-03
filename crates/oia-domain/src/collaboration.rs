//! Proposed shared-case access decision policy and offline field conflict-synchronization
//! kernel (`COLLAB-01`, `COLLAB-03`, `FIELD-04`, `A13`, `A16`, `#38`).
//!
//! ## Scope and status (`AGENTS.md` §3, §4)
//!
//! 1. **Proposed capability and sensitivity table (`D10` open):**
//!    The role, clearance, and surface rules in [`PROPOSED_ROLE_SURFACE_TABLE`] and
//!    [`evaluate_proposed_access_decision`] are a **proposed default** accompanying
//!    `docs/architecture.md` §9. Decision `D10` (`docs/delivery.md` — hosted transport/identity,
//!    capability matrix, merge rules, and sensitive-data defaults) remains open and is not
//!    settled by this module.
//! 2. **Pure decision function (not wired into runtime services yet):**
//!    [`evaluate_proposed_access_decision`] is a stateless policy function (`(role, clearance,
//!    surface, sensitivity) -> AccessDecision`). **Nothing in the application calls it yet:**
//!    search, blob reads, subscriptions, model input, offline preparation, and report export
//!    are not access-controlled until `#14` (application services), `#22` (`oia-export`), and
//!    `#16` (workspace UI) wire this policy into their execution paths.
//! 3. **Offline field synchronization and conflict kernel (`FIELD-04`, `A13`, `A16`):**
//!    [`OfflineSyncItem`] tracks versioned offline captures for a single case item, reusing
//!    [`crate::witness::AccountOrigin`] (`Reported` vs `Simulated`), retaining separate device
//!    capture, host receipt, and sync timestamps with their clock uncertainties, evaluating
//!    offline package expiry conservatively across clock offset and uncertainty bounds, and
//!    preserving concurrent edits as explicit conflicts without overwriting or dropping newer
//!    head versions. Report snapshot rendering, redaction disclosures, and reassessment walks
//!    remain in `oia-export` (`#22`) and `oia-documents` (`#19`).

use std::collections::BTreeMap;
use std::fmt;

use crate::witness::AccountOrigin;
use crate::{ActorId, OpaqueId, SchemaVersion, VersionId};

/// Mandatory label preserved on simulated field captures across search, reports, and evaluation (`FIELD-04`, `A13`).
pub const VISIBLE_SIMULATION_LABEL: &str =
    "SIMULATED FIELD FIXTURE — CONTROLLED SIMULATION (NOT OBSERVED EVIDENCE)";

/// Notice that the capability matrix in this module is a proposed default pending `D10`.
pub const PROPOSED_D10_POLICY_NOTICE: &str = "PROPOSED DEFAULT POLICY (D10 OPEN): Role, surface, clearance, and merge rules are proposed defaults and are not yet wired into application services (#14).";

/// Default maximum UTF-8 bytes per summary, location detail, or rationale string.
pub const DEFAULT_MAX_TEXT_BYTES: usize = 8_192;
/// Default maximum retained versions per [`OfflineSyncItem`].
pub const DEFAULT_MAX_VERSIONS_PER_ITEM: usize = 128;
/// Default maximum prepared offline packages retained per [`OfflineSyncItem`].
pub const DEFAULT_MAX_PREPARED_PACKAGES: usize = 64;
/// Default maximum conflict records retained per [`OfflineSyncItem`].
pub const DEFAULT_MAX_CONFLICTS: usize = 64;

/// Which resource limit was zero or exceeded during an offline sync operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncLimitKind {
    TextBytes,
    VersionsPerItem,
    PreparedPackages,
    Conflicts,
}

impl fmt::Display for SyncLimitKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TextBytes => f.write_str("max_text_bytes"),
            Self::VersionsPerItem => f.write_str("max_versions_per_item"),
            Self::PreparedPackages => f.write_str("max_prepared_packages"),
            Self::Conflicts => f.write_str("max_conflicts"),
        }
    }
}

/// Explicit resource bounds for an [`OfflineSyncItem`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncLimits {
    max_text_bytes: usize,
    max_versions_per_item: usize,
    max_prepared_packages: usize,
    max_conflicts: usize,
}

impl SyncLimits {
    /// Validates that all limits are non-zero.
    pub const fn new(
        max_text_bytes: usize,
        max_versions_per_item: usize,
        max_prepared_packages: usize,
        max_conflicts: usize,
    ) -> Result<Self, SyncError> {
        if max_text_bytes == 0 {
            return Err(SyncError::ZeroLimit(SyncLimitKind::TextBytes));
        }
        if max_versions_per_item == 0 {
            return Err(SyncError::ZeroLimit(SyncLimitKind::VersionsPerItem));
        }
        if max_prepared_packages == 0 {
            return Err(SyncError::ZeroLimit(SyncLimitKind::PreparedPackages));
        }
        if max_conflicts == 0 {
            return Err(SyncError::ZeroLimit(SyncLimitKind::Conflicts));
        }
        Ok(Self {
            max_text_bytes,
            max_versions_per_item,
            max_prepared_packages,
            max_conflicts,
        })
    }

    #[must_use]
    pub const fn max_text_bytes(self) -> usize {
        self.max_text_bytes
    }

    #[must_use]
    pub const fn max_versions_per_item(self) -> usize {
        self.max_versions_per_item
    }

    #[must_use]
    pub const fn max_prepared_packages(self) -> usize {
        self.max_prepared_packages
    }

    #[must_use]
    pub const fn max_conflicts(self) -> usize {
        self.max_conflicts
    }
}

impl Default for SyncLimits {
    fn default() -> Self {
        Self {
            max_text_bytes: DEFAULT_MAX_TEXT_BYTES,
            max_versions_per_item: DEFAULT_MAX_VERSIONS_PER_ITEM,
            max_prepared_packages: DEFAULT_MAX_PREPARED_PACKAGES,
            max_conflicts: DEFAULT_MAX_CONFLICTS,
        }
    }
}

/// Proposed collaborator role within a shared case (`COLLAB-01`, `docs/architecture.md` §9, `D10` open).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CaseRole {
    /// Controls sharing, collaborator roles, publication, and deletion (`docs/architecture.md` §9).
    Owner,
    /// Adds and edits case material, prepares/synchronizes offline captures, and saves drafts.
    Editor,
    /// Submits assessments and reviews; inspects permitted case content.
    Reviewer,
    /// Read-only inspection of permitted case material on passive read surfaces.
    Viewer,
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

/// Access surfaces across which [`evaluate_proposed_access_decision`] evaluates `(role, clearance, surface, sensitivity)`
/// (`COLLAB-01`, `COLLAB-03`, `FIELD-04`, `A16`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AccessSurface {
    /// Full-text, spatial, or temporal search queries (`Owner`, `Editor`, `Reviewer`, `Viewer`).
    Search,
    /// Content-addressed blob reads (`Owner`, `Editor`, `Reviewer`, `Viewer`).
    BlobRead,
    /// Event stream subscriptions (`Owner`, `Editor`, `Reviewer`, `Viewer`).
    Subscription,
    /// Direct item or version inspection (`Owner`, `Editor`, `Reviewer`, `Viewer`).
    DirectArtifactRequest,
    /// Submitting a scoped review or assessment (`Owner`, `Editor`, `Reviewer`).
    ReviewSubmission,
    /// Exporting a private working copy (`Owner`, `Editor`, `Reviewer`).
    PrivateExport,
    /// Assembling context for local or remote model inference (`Owner`, `Editor`).
    ModelInput,
    /// Saving a local report draft without sharing or publishing (`Owner`, `Editor`).
    ReportDraftSave,
    /// Preparing a bounded package for offline field use (`Owner`, `Editor`).
    OfflinePreparation,
    /// Adding/editing case material, synchronizing offline captures, or resolving conflicts (`Owner`, `Editor`).
    EvidenceMutation,
    /// Sharing a case or report with collaborators (`Owner` only; `docs/architecture.md` §9).
    CollaboratorShare,
    /// Issuing a public report snapshot (`Owner` only; forces standard public clearance).
    PublicPublication,
}

impl fmt::Display for AccessSurface {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Search => f.write_str("search"),
            Self::BlobRead => f.write_str("blob_read"),
            Self::Subscription => f.write_str("subscription"),
            Self::DirectArtifactRequest => f.write_str("direct_artifact_request"),
            Self::ReviewSubmission => f.write_str("review_submission"),
            Self::PrivateExport => f.write_str("private_export"),
            Self::ModelInput => f.write_str("model_input"),
            Self::ReportDraftSave => f.write_str("report_draft_save"),
            Self::OfflinePreparation => f.write_str("offline_preparation"),
            Self::EvidenceMutation => f.write_str("evidence_mutation"),
            Self::CollaboratorShare => f.write_str("collaborator_share"),
            Self::PublicPublication => f.write_str("public_publication"),
        }
    }
}

/// Single proposed `(CaseRole, AccessSurface)` capability table (pending `D10`).
///
/// Centralizes every role-by-surface rule in one table so that changing the proposed
/// capability matrix requires editing only this definition.
pub const PROPOSED_ROLE_SURFACE_TABLE: &[(CaseRole, AccessSurface, bool)] = &[
    // Owner: all surfaces permitted by role (clearance still checked independently).
    (CaseRole::Owner, AccessSurface::Search, true),
    (CaseRole::Owner, AccessSurface::BlobRead, true),
    (CaseRole::Owner, AccessSurface::Subscription, true),
    (CaseRole::Owner, AccessSurface::DirectArtifactRequest, true),
    (CaseRole::Owner, AccessSurface::ReviewSubmission, true),
    (CaseRole::Owner, AccessSurface::PrivateExport, true),
    (CaseRole::Owner, AccessSurface::ModelInput, true),
    (CaseRole::Owner, AccessSurface::ReportDraftSave, true),
    (CaseRole::Owner, AccessSurface::OfflinePreparation, true),
    (CaseRole::Owner, AccessSurface::EvidenceMutation, true),
    (CaseRole::Owner, AccessSurface::CollaboratorShare, true),
    (CaseRole::Owner, AccessSurface::PublicPublication, true),
    // Editor: read, review, private export, model input, draft save, offline prep, and mutation;
    // cannot share with collaborators or publish publicly (`docs/architecture.md` §9).
    (CaseRole::Editor, AccessSurface::Search, true),
    (CaseRole::Editor, AccessSurface::BlobRead, true),
    (CaseRole::Editor, AccessSurface::Subscription, true),
    (CaseRole::Editor, AccessSurface::DirectArtifactRequest, true),
    (CaseRole::Editor, AccessSurface::ReviewSubmission, true),
    (CaseRole::Editor, AccessSurface::PrivateExport, true),
    (CaseRole::Editor, AccessSurface::ModelInput, true),
    (CaseRole::Editor, AccessSurface::ReportDraftSave, true),
    (CaseRole::Editor, AccessSurface::OfflinePreparation, true),
    (CaseRole::Editor, AccessSurface::EvidenceMutation, true),
    (CaseRole::Editor, AccessSurface::CollaboratorShare, false),
    (CaseRole::Editor, AccessSurface::PublicPublication, false),
    // Reviewer: passive read surfaces, review submission, and private export only.
    (CaseRole::Reviewer, AccessSurface::Search, true),
    (CaseRole::Reviewer, AccessSurface::BlobRead, true),
    (CaseRole::Reviewer, AccessSurface::Subscription, true),
    (
        CaseRole::Reviewer,
        AccessSurface::DirectArtifactRequest,
        true,
    ),
    (CaseRole::Reviewer, AccessSurface::ReviewSubmission, true),
    (CaseRole::Reviewer, AccessSurface::PrivateExport, true),
    (CaseRole::Reviewer, AccessSurface::ModelInput, false),
    (CaseRole::Reviewer, AccessSurface::ReportDraftSave, false),
    (CaseRole::Reviewer, AccessSurface::OfflinePreparation, false),
    (CaseRole::Reviewer, AccessSurface::EvidenceMutation, false),
    (CaseRole::Reviewer, AccessSurface::CollaboratorShare, false),
    (CaseRole::Reviewer, AccessSurface::PublicPublication, false),
    // Viewer: passive read inspection only (`Search`, `BlobRead`, `Subscription`, `DirectArtifactRequest`).
    (CaseRole::Viewer, AccessSurface::Search, true),
    (CaseRole::Viewer, AccessSurface::BlobRead, true),
    (CaseRole::Viewer, AccessSurface::Subscription, true),
    (CaseRole::Viewer, AccessSurface::DirectArtifactRequest, true),
    (CaseRole::Viewer, AccessSurface::ReviewSubmission, false),
    (CaseRole::Viewer, AccessSurface::PrivateExport, false),
    (CaseRole::Viewer, AccessSurface::ModelInput, false),
    (CaseRole::Viewer, AccessSurface::ReportDraftSave, false),
    (CaseRole::Viewer, AccessSurface::OfflinePreparation, false),
    (CaseRole::Viewer, AccessSurface::EvidenceMutation, false),
    (CaseRole::Viewer, AccessSurface::CollaboratorShare, false),
    (CaseRole::Viewer, AccessSurface::PublicPublication, false),
];

/// Looks up `(role, surface)` in [`PROPOSED_ROLE_SURFACE_TABLE`].
#[must_use]
pub const fn proposed_role_permits_surface(role: CaseRole, surface: AccessSurface) -> bool {
    let mut i = 0;
    while i < PROPOSED_ROLE_SURFACE_TABLE.len() {
        let (r, s, allowed) = PROPOSED_ROLE_SURFACE_TABLE[i];
        if r as u8 == role as u8 && s as u8 == surface as u8 {
            return allowed;
        }
        i += 1;
    }
    false
}

/// Independent sensitivity clearance flags (`COLLAB-01`, `A16`, `docs/architecture.md` §9, `D10` open).
///
/// Permission to see a case or hold a [`CaseRole`] (including [`CaseRole::Owner`]) is
/// independent of clearance for unpublished drafts, sensitive artifacts, or sensitive locations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClearanceGrant {
    pub allow_unpublished_drafts: bool,
    pub allow_sensitive_artifacts: bool,
    pub allow_sensitive_locations: bool,
}

impl ClearanceGrant {
    /// Clearance for non-sensitive, published/shared material only.
    #[must_use]
    pub const fn standard_only() -> Self {
        Self {
            allow_unpublished_drafts: false,
            allow_sensitive_artifacts: false,
            allow_sensitive_locations: false,
        }
    }

    /// Clearance for all three sensitivity dimensions.
    #[must_use]
    pub const fn full() -> Self {
        Self {
            allow_unpublished_drafts: true,
            allow_sensitive_artifacts: true,
            allow_sensitive_locations: true,
        }
    }

    /// Computes the intersection (least privilege) of two clearances.
    #[must_use]
    pub const fn intersect(self, other: Self) -> Self {
        Self {
            allow_unpublished_drafts: self.allow_unpublished_drafts
                && other.allow_unpublished_drafts,
            allow_sensitive_artifacts: self.allow_sensitive_artifacts
                && other.allow_sensitive_artifacts,
            allow_sensitive_locations: self.allow_sensitive_locations
                && other.allow_sensitive_locations,
        }
    }
}

/// Sensitivity markings on a case item (`COLLAB-01`, `A16`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SensitivityFlags {
    pub unpublished_draft: bool,
    pub sensitive_artifact: bool,
    pub sensitive_location: bool,
}

impl SensitivityFlags {
    /// Standard item with no draft, sensitive-artifact, or sensitive-location restriction.
    #[must_use]
    pub const fn standard() -> Self {
        Self {
            unpublished_draft: false,
            sensitive_artifact: false,
            sensitive_location: false,
        }
    }

    /// Monotonic union of two sensitivity markings so a stricter flag is never lost on update or deduplication.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self {
            unpublished_draft: self.unpublished_draft || other.unpublished_draft,
            sensitive_artifact: self.sensitive_artifact || other.sensitive_artifact,
            sensitive_location: self.sensitive_location || other.sensitive_location,
        }
    }
}

/// Reason [`evaluate_proposed_access_decision`] denied access to an item on a surface.
///
/// Carries no item identifier, version identifier, or secret content so denial reasons
/// cannot leak existence-sensitive metadata (`docs/contracts/report-export.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessDenialReason {
    RoleNotPermittedForSurface {
        role: CaseRole,
        surface: AccessSurface,
    },
    UnpublishedDraftRestricted,
    SensitiveArtifactRestricted,
}

impl fmt::Display for AccessDenialReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RoleNotPermittedForSurface { role, surface } => {
                write!(f, "role {role} is not permitted on surface {surface}")
            }
            Self::UnpublishedDraftRestricted => f.write_str("unpublished draft is restricted"),
            Self::SensitiveArtifactRestricted => f.write_str("sensitive artifact is restricted"),
        }
    }
}

/// Output of the pure proposed access policy [`evaluate_proposed_access_decision`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessDecision {
    /// Role and clearance permit full access, including any location detail.
    AllowFull,
    /// Role and clearance permit the item's general content, but `sensitive_location_detail`
    /// must be physically stripped before returning or exporting (`AGENTS.md` §5).
    AllowWithStrippedLocation,
    /// Access to the item on the requested surface is denied.
    Deny(AccessDenialReason),
}

/// Pure, stateless proposed access decision function (`COLLAB-01`, `COLLAB-03`, `A16`, `D10` open).
///
/// Evaluates `(role, clearance, surface, sensitivity)` in one place:
/// 1. Consults [`PROPOSED_ROLE_SURFACE_TABLE`] for `(role, surface)`.
/// 2. On [`AccessSurface::PublicPublication`], enforces [`ClearanceGrant::standard_only()`]
///    regardless of the caller's personal clearance so public snapshots never include
///    unpublished drafts, sensitive artifacts, or sensitive locations.
/// 3. Denies access when `unpublished_draft` or `sensitive_artifact` exceeds the effective clearance.
/// 4. Returns [`AccessDecision::AllowWithStrippedLocation`] when `sensitive_location` is set
///    and `allow_sensitive_locations` is `false`.
#[must_use]
pub const fn evaluate_proposed_access_decision(
    role: CaseRole,
    clearance: ClearanceGrant,
    surface: AccessSurface,
    sensitivity: SensitivityFlags,
) -> AccessDecision {
    if !proposed_role_permits_surface(role, surface) {
        return AccessDecision::Deny(AccessDenialReason::RoleNotPermittedForSurface {
            role,
            surface,
        });
    }
    let effective_clearance = match surface {
        AccessSurface::PublicPublication => ClearanceGrant::standard_only(),
        _ => clearance,
    };
    if sensitivity.unpublished_draft && !effective_clearance.allow_unpublished_drafts {
        return AccessDecision::Deny(AccessDenialReason::UnpublishedDraftRestricted);
    }
    if sensitivity.sensitive_artifact && !effective_clearance.allow_sensitive_artifacts {
        return AccessDecision::Deny(AccessDenialReason::SensitiveArtifactRestricted);
    }
    if sensitivity.sensitive_location && !effective_clearance.allow_sensitive_locations {
        return AccessDecision::AllowWithStrippedLocation;
    }
    AccessDecision::AllowFull
}

/// Clock quality associated with a recorded timestamp (`FIELD-04`, `AGENTS.md` §2, `docs/architecture.md` §3.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceClockQuality {
    /// Clock synchronized to UTC with a symmetric uncertainty bound `±uncertainty_ms`.
    SynchronizedUtc { uncertainty_ms: u64 },
    /// Drifting local clock with an estimated offset (`estimated_offset_ms`) and uncertainty (`uncertainty_ms`).
    DriftingLocalClock {
        estimated_offset_ms: i64,
        uncertainty_ms: u64,
    },
    /// Unreferenced local or monotonic clock whose offset from UTC is unknown.
    UnreferencedLocalClock { uncertainty_reason: String },
}

/// Recorded timestamp paired with its explicit [`DeviceClockQuality`] (`AGENTS.md` §2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimestampWithClock {
    pub recorded_unix_ms: i64,
    pub clock_quality: DeviceClockQuality,
}

impl TimestampWithClock {
    /// Computes the conservative UTC interval `[earliest_utc_ms, latest_utc_ms]` that spans both
    /// raw and offset-adjusted readings plus `±uncertainty_ms`, or `None` if the clock is unreferenced.
    #[must_use]
    pub fn conservative_utc_bounds(&self) -> Option<(i64, i64)> {
        match &self.clock_quality {
            DeviceClockQuality::SynchronizedUtc { uncertainty_ms } => {
                let unc = i64::try_from(*uncertainty_ms).unwrap_or(i64::MAX);
                Some((
                    self.recorded_unix_ms.saturating_sub(unc),
                    self.recorded_unix_ms.saturating_add(unc),
                ))
            }
            DeviceClockQuality::DriftingLocalClock {
                estimated_offset_ms,
                uncertainty_ms,
            } => {
                let unc = i64::try_from(*uncertainty_ms).unwrap_or(i64::MAX);
                let plus = self.recorded_unix_ms.saturating_add(*estimated_offset_ms);
                let minus = self.recorded_unix_ms.saturating_sub(*estimated_offset_ms);
                let low_center = self.recorded_unix_ms.min(plus).min(minus);
                let high_center = self.recorded_unix_ms.max(plus).max(minus);
                Some((
                    low_center.saturating_sub(unc),
                    high_center.saturating_add(unc),
                ))
            }
            DeviceClockQuality::UnreferencedLocalClock { .. } => None,
        }
    }
}

/// Freshness state of a prepared offline reference window when evaluated against a capture clock (`FIELD-04`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfflineReferenceFreshness {
    /// Every point in the capture clock's conservative UTC uncertainty interval falls within `[prepared_at_ms, expires_at_ms]`.
    FreshWithinWindow,
    /// Even at the earliest bound of the capture clock's uncertainty interval, the package was expired by at least `min_expired_by_ms`.
    CachedExpired { min_expired_by_ms: u64 },
    /// The capture clock is unreferenced, or its offset/uncertainty interval straddles the package validity boundary.
    ExpiryUncertain {
        earliest_possible_utc_ms: Option<i64>,
        latest_possible_utc_ms: Option<i64>,
    },
}

/// Prepared offline reference window metadata (`FIELD-04`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedOfflineWindow {
    package_id: OpaqueId,
    prepared_for: ActorId,
    base_version_at_prep: VersionId,
    prepared_at_unix_ms: i64,
    expires_at_unix_ms: i64,
}

impl PreparedOfflineWindow {
    #[must_use]
    pub fn package_id(&self) -> &OpaqueId {
        &self.package_id
    }

    #[must_use]
    pub fn prepared_for(&self) -> &ActorId {
        &self.prepared_for
    }

    #[must_use]
    pub fn base_version_at_prep(&self) -> &VersionId {
        &self.base_version_at_prep
    }

    #[must_use]
    pub const fn prepared_at_unix_ms(&self) -> i64 {
        self.prepared_at_unix_ms
    }

    #[must_use]
    pub const fn expires_at_unix_ms(&self) -> i64 {
        self.expires_at_unix_ms
    }

    /// Evaluates whether this offline reference window was fresh, definitely expired, or
    /// expiry-uncertain at `capture_time`, accounting for both clock offset and uncertainty (`FIELD-04`).
    #[must_use]
    pub fn evaluate_freshness(
        &self,
        capture_time: &TimestampWithClock,
    ) -> OfflineReferenceFreshness {
        let Some((earliest_utc_ms, latest_utc_ms)) = capture_time.conservative_utc_bounds() else {
            return OfflineReferenceFreshness::ExpiryUncertain {
                earliest_possible_utc_ms: None,
                latest_possible_utc_ms: None,
            };
        };
        if earliest_utc_ms > self.expires_at_unix_ms {
            let min_expired_by_ms = earliest_utc_ms.saturating_sub(self.expires_at_unix_ms) as u64;
            return OfflineReferenceFreshness::CachedExpired { min_expired_by_ms };
        }
        if earliest_utc_ms >= self.prepared_at_unix_ms && latest_utc_ms <= self.expires_at_unix_ms {
            return OfflineReferenceFreshness::FreshWithinWindow;
        }
        OfflineReferenceFreshness::ExpiryUncertain {
            earliest_possible_utc_ms: Some(earliest_utc_ms),
            latest_possible_utc_ms: Some(latest_utc_ms),
        }
    }
}

/// Sanitized, clearance-filtered projection of an item version (`AGENTS.md` §5).
///
/// Constructed only after passing [`evaluate_proposed_access_decision`]; if the decision is
/// [`AccessDecision::AllowWithStrippedLocation`], `sensitive_location_detail` is `None` and
/// `location_stripped` is `true`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermittedVersionView {
    version: VersionId,
    author: ActorId,
    parent_versions: Vec<VersionId>,
    summary_text: String,
    sensitive_location_detail: Option<String>,
    location_stripped: bool,
    declared_content_sha256: String,
    origin: AccountOrigin,
    simulation_parameters: Option<String>,
    visible_simulation_label: Option<&'static str>,
    capture_time: TimestampWithClock,
    receipt_time: TimestampWithClock,
    sync_time: TimestampWithClock,
    reference_freshness: Option<OfflineReferenceFreshness>,
}

impl PermittedVersionView {
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
    pub const fn location_stripped(&self) -> bool {
        self.location_stripped
    }

    #[must_use]
    pub fn declared_content_sha256(&self) -> &str {
        &self.declared_content_sha256
    }

    #[must_use]
    pub const fn origin(&self) -> AccountOrigin {
        self.origin
    }

    #[must_use]
    pub fn simulation_parameters(&self) -> Option<&str> {
        self.simulation_parameters.as_deref()
    }

    #[must_use]
    pub const fn visible_simulation_label(&self) -> Option<&'static str> {
        self.visible_simulation_label
    }

    #[must_use]
    pub fn capture_time(&self) -> &TimestampWithClock {
        &self.capture_time
    }

    #[must_use]
    pub fn receipt_time(&self) -> &TimestampWithClock {
        &self.receipt_time
    }

    #[must_use]
    pub fn sync_time(&self) -> &TimestampWithClock {
        &self.sync_time
    }

    #[must_use]
    pub const fn reference_freshness(&self) -> Option<OfflineReferenceFreshness> {
        self.reference_freshness
    }
}

/// Input for creating an [`OfflineSyncItem`] with its initial version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitialSyncItemInput {
    pub item_id: OpaqueId,
    pub schema_version: SchemaVersion,
    pub initial_version: VersionId,
    pub author: ActorId,
    pub origin: AccountOrigin,
    pub simulation_parameters: Option<String>,
    pub sensitivity: SensitivityFlags,
    pub summary_text: String,
    pub sensitive_location_detail: Option<String>,
    pub declared_content_sha256: String,
    pub capture_time: TimestampWithClock,
    pub receipt_time: TimestampWithClock,
    pub sync_time: TimestampWithClock,
}

/// Offline field capture or edit submitted for synchronization (`FIELD-04`, `A13`, `A16`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfflineFieldCapture {
    pub capture_id: OpaqueId,
    pub expected_base_version: VersionId,
    pub new_version: VersionId,
    pub author: ActorId,
    pub origin: AccountOrigin,
    pub simulation_parameters: Option<String>,
    pub sensitivity: SensitivityFlags,
    pub summary_text: String,
    pub sensitive_location_detail: Option<String>,
    pub declared_content_sha256: String,
    pub capture_time: TimestampWithClock,
    pub receipt_time: TimestampWithClock,
    pub sync_time: TimestampWithClock,
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncConflictRecord {
    conflict_id: OpaqueId,
    expected_base_version: VersionId,
    head_version_at_conflict: VersionId,
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
    pub fn expected_base_version(&self) -> &VersionId {
        &self.expected_base_version
    }

    #[must_use]
    pub fn head_version_at_conflict(&self) -> &VersionId {
        &self.head_version_at_conflict
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

/// Outcome of synchronizing an [`OfflineFieldCapture`] into an [`OfflineSyncItem`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncOutcome {
    /// `expected_base_version` matched the current head; head advanced cleanly.
    AppliedCleanly {
        new_head_version: VersionId,
        reference_freshness: Option<OfflineReferenceFreshness>,
    },
    /// Base version diverged, but caller-declared SHA-256, text, location detail, and origin
    /// matched the current head (`docs/architecture.md` §9). Sensitivity flags were unioned.
    DeduplicatedByDeclaredDigest {
        retained_head_version: VersionId,
        recorded_duplicate_version: VersionId,
    },
    /// Base version diverged with different content; both versions are preserved in history and
    /// a [`SyncConflictRecord`] is opened without overwriting the current head (`A13`, `A16`).
    ConflictPreserved {
        conflict_id: OpaqueId,
        preserved_head_version: VersionId,
        preserved_offline_version: VersionId,
    },
}

/// Typed errors for proposed access decisions and offline field synchronization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncError {
    ZeroLimit(SyncLimitKind),
    LimitExceeded {
        kind: SyncLimitKind,
        limit: usize,
    },
    AccessDenied(AccessDenialReason),
    EmptyTextField(&'static str),
    ControlCharacterInText(&'static str),
    InvalidSha256Hex,
    InvalidTimeWindow {
        start_ms: i64,
        end_ms: i64,
    },
    MissingSimulationParameters,
    UnexpectedSimulationParameters,
    OriginMismatch {
        expected: AccountOrigin,
        actual: AccountOrigin,
    },
    UnknownBaseVersion(VersionId),
    DuplicateVersion(VersionId),
    DuplicatePackageId(OpaqueId),
    UnknownPreparedPackage(OpaqueId),
    PreparedPackageActorMismatch {
        package_id: OpaqueId,
        expected: ActorId,
        actual: ActorId,
    },
    DuplicateConflictId(OpaqueId),
    UnknownConflict(OpaqueId),
    ConflictAlreadyResolved(OpaqueId),
    ConflictParentMismatch {
        conflict_id: OpaqueId,
    },
    StaleConflictHeadMoved {
        conflict_id: OpaqueId,
        conflict_head: VersionId,
        current_head: VersionId,
    },
}

impl fmt::Display for SyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroLimit(kind) => write!(f, "sync limit {kind} must be >= 1"),
            Self::LimitExceeded { kind, limit } => {
                write!(f, "sync limit {kind} ({limit}) exceeded")
            }
            Self::AccessDenied(reason) => write!(f, "access denied: {reason}"),
            Self::EmptyTextField(name) => write!(f, "{name} must not be empty or whitespace"),
            Self::ControlCharacterInText(name) => {
                write!(f, "{name} contains disallowed control characters")
            }
            Self::InvalidSha256Hex => f.write_str(
                "declared_content_sha256 must be 64 lowercase ASCII hexadecimal characters",
            ),
            Self::InvalidTimeWindow { start_ms, end_ms } => {
                write!(
                    f,
                    "invalid time window [{start_ms}, {end_ms}): start must be < end"
                )
            }
            Self::MissingSimulationParameters => f.write_str(
                "simulated captures must declare non-empty simulation_parameters (FIELD-04)",
            ),
            Self::UnexpectedSimulationParameters => {
                f.write_str("reported captures must not carry simulation_parameters")
            }
            Self::OriginMismatch { expected, actual } => write!(
                f,
                "origin class is fixed as {expected:?} and cannot change to {actual:?}"
            ),
            Self::UnknownBaseVersion(v) => write!(f, "base version {v} does not exist in history"),
            Self::DuplicateVersion(v) => write!(f, "version identifier {v} was already used"),
            Self::DuplicatePackageId(id) => {
                write!(f, "prepared package identifier {id} already exists")
            }
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
            Self::DuplicateConflictId(id) => write!(f, "conflict identifier {id} already exists"),
            Self::UnknownConflict(id) => write!(f, "sync conflict {id} does not exist"),
            Self::ConflictAlreadyResolved(id) => {
                write!(f, "sync conflict {id} has already been resolved")
            }
            Self::ConflictParentMismatch { conflict_id } => write!(
                f,
                "resolution for conflict {conflict_id} must reference both conflicting versions"
            ),
            Self::StaleConflictHeadMoved {
                conflict_id,
                conflict_head,
                current_head,
            } => write!(
                f,
                "cannot resolve conflict {conflict_id} recorded at head {conflict_head} because current head moved to {current_head}"
            ),
        }
    }
}

impl std::error::Error for SyncError {}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredItemVersion {
    version: VersionId,
    author: ActorId,
    parent_versions: Vec<VersionId>,
    summary_text: String,
    sensitive_location_detail: Option<String>,
    declared_content_sha256: String,
    simulation_parameters: Option<String>,
    capture_time: TimestampWithClock,
    receipt_time: TimestampWithClock,
    sync_time: TimestampWithClock,
    reference_freshness: Option<OfflineReferenceFreshness>,
}

fn validate_text(name: &'static str, value: &str, max_bytes: usize) -> Result<(), SyncError> {
    if value.len() > max_bytes {
        return Err(SyncError::LimitExceeded {
            kind: SyncLimitKind::TextBytes,
            limit: max_bytes,
        });
    }
    if value.trim().is_empty() {
        return Err(SyncError::EmptyTextField(name));
    }
    if value
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\r' | '\n' | '\t'))
    {
        return Err(SyncError::ControlCharacterInText(name));
    }
    Ok(())
}

fn validate_sha256_hex(digest: &str) -> Result<(), SyncError> {
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
    {
        return Err(SyncError::InvalidSha256Hex);
    }
    Ok(())
}

fn validate_clock(clock: &TimestampWithClock, max_bytes: usize) -> Result<(), SyncError> {
    if let DeviceClockQuality::UnreferencedLocalClock { uncertainty_reason } = &clock.clock_quality
    {
        validate_text("uncertainty_reason", uncertainty_reason, max_bytes)?;
    }
    Ok(())
}

fn validate_origin_params(
    origin: AccountOrigin,
    simulation_parameters: Option<&String>,
    max_bytes: usize,
) -> Result<(), SyncError> {
    match (origin, simulation_parameters) {
        (AccountOrigin::Simulated, Some(params)) => {
            validate_text("simulation_parameters", params, max_bytes)
        }
        (AccountOrigin::Simulated, None) => Err(SyncError::MissingSimulationParameters),
        (AccountOrigin::Reported, Some(_)) => Err(SyncError::UnexpectedSimulationParameters),
        (AccountOrigin::Reported, None) => Ok(()),
    }
}

/// Version-aware offline field synchronization log for a single case item (`FIELD-04`, `A13`, `A16`).
///
/// All read and write access goes through [`evaluate_proposed_access_decision`] so raw
/// `sensitive_location_detail` is never exposed without `allow_sensitive_locations`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfflineSyncItem {
    item_id: OpaqueId,
    schema_version: SchemaVersion,
    origin: AccountOrigin,
    sensitivity: SensitivityFlags,
    limits: SyncLimits,
    head_version: VersionId,
    versions: Vec<StoredItemVersion>,
    prepared_packages: BTreeMap<OpaqueId, PreparedOfflineWindow>,
    conflicts: BTreeMap<OpaqueId, SyncConflictRecord>,
}

impl OfflineSyncItem {
    /// Creates a versioned item for offline/field synchronization after checking
    /// [`AccessSurface::EvidenceMutation`] via [`evaluate_proposed_access_decision`].
    pub fn new(
        creator_role: CaseRole,
        creator_clearance: ClearanceGrant,
        input: InitialSyncItemInput,
        limits: SyncLimits,
    ) -> Result<Self, SyncError> {
        if let AccessDecision::Deny(reason) = evaluate_proposed_access_decision(
            creator_role,
            creator_clearance,
            AccessSurface::EvidenceMutation,
            input.sensitivity,
        ) {
            return Err(SyncError::AccessDenied(reason));
        }
        validate_text("summary_text", &input.summary_text, limits.max_text_bytes)?;
        if let Some(loc) = &input.sensitive_location_detail {
            validate_text("sensitive_location_detail", loc, limits.max_text_bytes)?;
        }
        validate_sha256_hex(&input.declared_content_sha256)?;
        validate_origin_params(
            input.origin,
            input.simulation_parameters.as_ref(),
            limits.max_text_bytes,
        )?;
        validate_clock(&input.capture_time, limits.max_text_bytes)?;
        validate_clock(&input.receipt_time, limits.max_text_bytes)?;
        validate_clock(&input.sync_time, limits.max_text_bytes)?;

        let sensitivity = SensitivityFlags {
            sensitive_location: input.sensitivity.sensitive_location
                || input.sensitive_location_detail.is_some(),
            ..input.sensitivity
        };
        let head_version = input.initial_version.clone();
        let initial = StoredItemVersion {
            version: input.initial_version,
            author: input.author,
            parent_versions: Vec::new(),
            summary_text: input.summary_text,
            sensitive_location_detail: input.sensitive_location_detail,
            declared_content_sha256: input.declared_content_sha256,
            simulation_parameters: input.simulation_parameters,
            capture_time: input.capture_time,
            receipt_time: input.receipt_time,
            sync_time: input.sync_time,
            reference_freshness: None,
        };
        Ok(Self {
            item_id: input.item_id,
            schema_version: input.schema_version,
            origin: input.origin,
            sensitivity,
            limits,
            head_version,
            versions: vec![initial],
            prepared_packages: BTreeMap::new(),
            conflicts: BTreeMap::new(),
        })
    }

    #[must_use]
    pub fn item_id(&self) -> &OpaqueId {
        &self.item_id
    }

    #[must_use]
    pub const fn schema_version(&self) -> SchemaVersion {
        self.schema_version
    }

    #[must_use]
    pub const fn origin(&self) -> AccountOrigin {
        self.origin
    }

    #[must_use]
    pub const fn sensitivity(&self) -> SensitivityFlags {
        self.sensitivity
    }

    #[must_use]
    pub const fn limits(&self) -> SyncLimits {
        self.limits
    }

    /// Reads the current head version on `surface`, enforcing [`evaluate_proposed_access_decision`]
    /// and stripping `sensitive_location_detail` when required.
    pub fn read_head_on_surface(
        &self,
        role: CaseRole,
        clearance: ClearanceGrant,
        surface: AccessSurface,
    ) -> Result<PermittedVersionView, SyncError> {
        let decision =
            evaluate_proposed_access_decision(role, clearance, surface, self.sensitivity);
        let strip_location = match decision {
            AccessDecision::AllowFull => false,
            AccessDecision::AllowWithStrippedLocation => true,
            AccessDecision::Deny(reason) => return Err(SyncError::AccessDenied(reason)),
        };
        let head = self
            .versions
            .iter()
            .find(|v| v.version == self.head_version)
            .ok_or_else(|| SyncError::UnknownBaseVersion(self.head_version.clone()))?;
        Ok(self.project_version(head, strip_location))
    }

    /// Reads all retained versions on `surface`, enforcing [`evaluate_proposed_access_decision`]
    /// and stripping `sensitive_location_detail` on every historical version when required.
    pub fn inspect_history_on_surface(
        &self,
        role: CaseRole,
        clearance: ClearanceGrant,
        surface: AccessSurface,
    ) -> Result<Vec<PermittedVersionView>, SyncError> {
        let decision =
            evaluate_proposed_access_decision(role, clearance, surface, self.sensitivity);
        let strip_location = match decision {
            AccessDecision::AllowFull => false,
            AccessDecision::AllowWithStrippedLocation => true,
            AccessDecision::Deny(reason) => return Err(SyncError::AccessDenied(reason)),
        };
        Ok(self
            .versions
            .iter()
            .map(|v| self.project_version(v, strip_location))
            .collect())
    }

    /// Inspects conflict records after verifying access on `surface`.
    pub fn inspect_conflicts_on_surface(
        &self,
        role: CaseRole,
        clearance: ClearanceGrant,
        surface: AccessSurface,
    ) -> Result<&BTreeMap<OpaqueId, SyncConflictRecord>, SyncError> {
        if let AccessDecision::Deny(reason) =
            evaluate_proposed_access_decision(role, clearance, surface, self.sensitivity)
        {
            return Err(SyncError::AccessDenied(reason));
        }
        Ok(&self.conflicts)
    }

    /// Prepares a bounded offline reference window (`FIELD-04`), enforcing both
    /// [`AccessSurface::OfflinePreparation`] via [`evaluate_proposed_access_decision`]
    /// and `limits.max_prepared_packages`.
    pub fn prepare_offline_window(
        &mut self,
        role: CaseRole,
        clearance: ClearanceGrant,
        actor: ActorId,
        package_id: OpaqueId,
        prepared_at_unix_ms: i64,
        expires_at_unix_ms: i64,
    ) -> Result<PreparedOfflineWindow, SyncError> {
        if let AccessDecision::Deny(reason) = evaluate_proposed_access_decision(
            role,
            clearance,
            AccessSurface::OfflinePreparation,
            self.sensitivity,
        ) {
            return Err(SyncError::AccessDenied(reason));
        }
        if prepared_at_unix_ms >= expires_at_unix_ms {
            return Err(SyncError::InvalidTimeWindow {
                start_ms: prepared_at_unix_ms,
                end_ms: expires_at_unix_ms,
            });
        }
        if self.prepared_packages.contains_key(&package_id) {
            return Err(SyncError::DuplicatePackageId(package_id));
        }
        if self.prepared_packages.len() >= self.limits.max_prepared_packages {
            return Err(SyncError::LimitExceeded {
                kind: SyncLimitKind::PreparedPackages,
                limit: self.limits.max_prepared_packages,
            });
        }
        let window = PreparedOfflineWindow {
            package_id: package_id.clone(),
            prepared_for: actor,
            base_version_at_prep: self.head_version.clone(),
            prepared_at_unix_ms,
            expires_at_unix_ms,
        };
        self.prepared_packages.insert(package_id, window.clone());
        Ok(window)
    }

    /// Synchronizes an offline field capture (`FIELD-04`, `A13`, `A16`).
    ///
    /// - Enforces [`AccessSurface::EvidenceMutation`] via [`evaluate_proposed_access_decision`].
    /// - Refuses origin changes (`Reported` vs `Simulated`).
    /// - Retains separate `capture_time`, `receipt_time`, and `sync_time` with their clock qualities.
    /// - Evaluates offline package freshness conservatively using clock offset and uncertainty.
    /// - Unions [`SensitivityFlags`] even on exact declared-digest deduplication so stricter
    ///   sensitivity flags are never dropped.
    /// - Opens a [`SyncConflictRecord`] and preserves both versions when `expected_base_version`
    ///   diverges from `head_version`.
    pub fn synchronize_offline_capture(
        &mut self,
        role: CaseRole,
        clearance: ClearanceGrant,
        capture: OfflineFieldCapture,
    ) -> Result<SyncOutcome, SyncError> {
        let combined_sensitivity = self.sensitivity.union(SensitivityFlags {
            sensitive_location: capture.sensitivity.sensitive_location
                || capture.sensitive_location_detail.is_some(),
            ..capture.sensitivity
        });
        if let AccessDecision::Deny(reason) = evaluate_proposed_access_decision(
            role,
            clearance,
            AccessSurface::EvidenceMutation,
            combined_sensitivity,
        ) {
            return Err(SyncError::AccessDenied(reason));
        }
        if capture.origin != self.origin {
            return Err(SyncError::OriginMismatch {
                expected: self.origin,
                actual: capture.origin,
            });
        }
        if self
            .versions
            .iter()
            .any(|v| v.version == capture.new_version)
        {
            return Err(SyncError::DuplicateVersion(capture.new_version));
        }
        if !self
            .versions
            .iter()
            .any(|v| v.version == capture.expected_base_version)
        {
            return Err(SyncError::UnknownBaseVersion(capture.expected_base_version));
        }
        if self.versions.len() >= self.limits.max_versions_per_item {
            return Err(SyncError::LimitExceeded {
                kind: SyncLimitKind::VersionsPerItem,
                limit: self.limits.max_versions_per_item,
            });
        }
        validate_text(
            "summary_text",
            &capture.summary_text,
            self.limits.max_text_bytes,
        )?;
        if let Some(loc) = &capture.sensitive_location_detail {
            validate_text("sensitive_location_detail", loc, self.limits.max_text_bytes)?;
        }
        validate_sha256_hex(&capture.declared_content_sha256)?;
        validate_origin_params(
            capture.origin,
            capture.simulation_parameters.as_ref(),
            self.limits.max_text_bytes,
        )?;
        validate_clock(&capture.capture_time, self.limits.max_text_bytes)?;
        validate_clock(&capture.receipt_time, self.limits.max_text_bytes)?;
        validate_clock(&capture.sync_time, self.limits.max_text_bytes)?;

        let reference_freshness = if let Some(pkg_id) = &capture.prepared_package_id {
            let pkg = self
                .prepared_packages
                .get(pkg_id)
                .ok_or_else(|| SyncError::UnknownPreparedPackage(pkg_id.clone()))?;
            if pkg.prepared_for != capture.author {
                return Err(SyncError::PreparedPackageActorMismatch {
                    package_id: pkg_id.clone(),
                    expected: pkg.prepared_for.clone(),
                    actual: capture.author,
                });
            }
            Some(pkg.evaluate_freshness(&capture.capture_time))
        } else {
            None
        };

        let current_head = self.head_version.clone();
        let incoming_stored = StoredItemVersion {
            version: capture.new_version.clone(),
            author: capture.author.clone(),
            parent_versions: vec![capture.expected_base_version.clone()],
            summary_text: capture.summary_text,
            sensitive_location_detail: capture.sensitive_location_detail,
            declared_content_sha256: capture.declared_content_sha256,
            simulation_parameters: capture.simulation_parameters,
            capture_time: capture.capture_time,
            receipt_time: capture.receipt_time,
            sync_time: capture.sync_time,
            reference_freshness,
        };

        if capture.expected_base_version == current_head {
            self.sensitivity = combined_sensitivity;
            self.head_version = capture.new_version.clone();
            self.versions.push(incoming_stored);
            return Ok(SyncOutcome::AppliedCleanly {
                new_head_version: capture.new_version,
                reference_freshness,
            });
        }

        let head_record = self
            .versions
            .iter()
            .find(|v| v.version == current_head)
            .ok_or_else(|| SyncError::UnknownBaseVersion(current_head.clone()))?;

        if head_record.declared_content_sha256 == incoming_stored.declared_content_sha256
            && head_record.summary_text == incoming_stored.summary_text
            && head_record.sensitive_location_detail == incoming_stored.sensitive_location_detail
            && head_record.simulation_parameters == incoming_stored.simulation_parameters
        {
            // Never drop a stricter sensitivity flag on deduplication.
            self.sensitivity = combined_sensitivity;
            self.versions.push(incoming_stored);
            return Ok(SyncOutcome::DeduplicatedByDeclaredDigest {
                retained_head_version: current_head,
                recorded_duplicate_version: capture.new_version,
            });
        }

        if self.conflicts.contains_key(&capture.capture_id) {
            return Err(SyncError::DuplicateConflictId(capture.capture_id));
        }
        if self.conflicts.len() >= self.limits.max_conflicts {
            return Err(SyncError::LimitExceeded {
                kind: SyncLimitKind::Conflicts,
                limit: self.limits.max_conflicts,
            });
        }

        self.sensitivity = combined_sensitivity;
        self.versions.push(incoming_stored);
        self.conflicts.insert(
            capture.capture_id.clone(),
            SyncConflictRecord {
                conflict_id: capture.capture_id.clone(),
                expected_base_version: capture.expected_base_version,
                head_version_at_conflict: current_head.clone(),
                incoming_offline_version: capture.new_version.clone(),
                incoming_author: capture.author,
                resolution: ConflictResolutionState::Unresolved,
            },
        );

        Ok(SyncOutcome::ConflictPreserved {
            conflict_id: capture.capture_id,
            preserved_head_version: current_head,
            preserved_offline_version: capture.new_version,
        })
    }

    /// Resolves an open [`SyncConflictRecord`] by appending a merged version linking both
    /// conflicting parent versions (`FIELD-04`, `A13`, `A16`).
    ///
    /// Refuses resolution with [`SyncError::StaleConflictHeadMoved`] if `self.head_version`
    /// has moved since `conflict.head_version_at_conflict`, preventing a later edit from
    /// dropping out of the head's lineage.
    #[allow(clippy::too_many_arguments)]
    pub fn resolve_sync_conflict(
        &mut self,
        role: CaseRole,
        clearance: ClearanceGrant,
        resolver: ActorId,
        conflict_id: &OpaqueId,
        resolution_version: VersionId,
        parent_versions: (VersionId, VersionId),
        merged_summary_text: String,
        merged_sensitive_location: Option<String>,
        merged_declared_sha256: String,
        simulation_parameters: Option<String>,
        capture_time: TimestampWithClock,
        receipt_time: TimestampWithClock,
        sync_time: TimestampWithClock,
        rationale: String,
    ) -> Result<(), SyncError> {
        let combined_sensitivity = self.sensitivity.union(SensitivityFlags {
            sensitive_location: self.sensitivity.sensitive_location
                || merged_sensitive_location.is_some(),
            ..self.sensitivity
        });
        if let AccessDecision::Deny(reason) = evaluate_proposed_access_decision(
            role,
            clearance,
            AccessSurface::EvidenceMutation,
            combined_sensitivity,
        ) {
            return Err(SyncError::AccessDenied(reason));
        }
        if self
            .versions
            .iter()
            .any(|v| v.version == resolution_version)
        {
            return Err(SyncError::DuplicateVersion(resolution_version));
        }
        if self.versions.len() >= self.limits.max_versions_per_item {
            return Err(SyncError::LimitExceeded {
                kind: SyncLimitKind::VersionsPerItem,
                limit: self.limits.max_versions_per_item,
            });
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
        validate_sha256_hex(&merged_declared_sha256)?;
        validate_origin_params(
            self.origin,
            simulation_parameters.as_ref(),
            self.limits.max_text_bytes,
        )?;
        validate_clock(&capture_time, self.limits.max_text_bytes)?;
        validate_clock(&receipt_time, self.limits.max_text_bytes)?;
        validate_clock(&sync_time, self.limits.max_text_bytes)?;

        let conflict = self
            .conflicts
            .get(conflict_id)
            .ok_or_else(|| SyncError::UnknownConflict(conflict_id.clone()))?;
        if matches!(
            conflict.resolution,
            ConflictResolutionState::Resolved { .. }
        ) {
            return Err(SyncError::ConflictAlreadyResolved(conflict_id.clone()));
        }
        if conflict.head_version_at_conflict != self.head_version {
            return Err(SyncError::StaleConflictHeadMoved {
                conflict_id: conflict_id.clone(),
                conflict_head: conflict.head_version_at_conflict.clone(),
                current_head: self.head_version.clone(),
            });
        }
        let matches_parents = (parent_versions.0 == conflict.head_version_at_conflict
            && parent_versions.1 == conflict.incoming_offline_version)
            || (parent_versions.1 == conflict.head_version_at_conflict
                && parent_versions.0 == conflict.incoming_offline_version);
        if !matches_parents {
            return Err(SyncError::ConflictParentMismatch {
                conflict_id: conflict_id.clone(),
            });
        }

        self.sensitivity = combined_sensitivity;
        self.head_version = resolution_version.clone();
        self.versions.push(StoredItemVersion {
            version: resolution_version.clone(),
            author: resolver.clone(),
            parent_versions: vec![parent_versions.0, parent_versions.1],
            summary_text: merged_summary_text,
            sensitive_location_detail: merged_sensitive_location,
            declared_content_sha256: merged_declared_sha256,
            simulation_parameters,
            capture_time,
            receipt_time,
            sync_time,
            reference_freshness: None,
        });

        if let Some(conflict_mut) = self.conflicts.get_mut(conflict_id) {
            conflict_mut.resolution = ConflictResolutionState::Resolved {
                resolution_version,
                resolved_by: resolver,
                rationale,
            };
        }
        Ok(())
    }

    fn project_version(
        &self,
        stored: &StoredItemVersion,
        strip_location: bool,
    ) -> PermittedVersionView {
        let sensitive_location_detail = if strip_location {
            None
        } else {
            stored.sensitive_location_detail.clone()
        };
        let location_stripped = strip_location && stored.sensitive_location_detail.is_some();
        let visible_simulation_label = match self.origin {
            AccountOrigin::Reported => None,
            AccountOrigin::Simulated => Some(VISIBLE_SIMULATION_LABEL),
        };
        PermittedVersionView {
            version: stored.version.clone(),
            author: stored.author.clone(),
            parent_versions: stored.parent_versions.clone(),
            summary_text: stored.summary_text.clone(),
            sensitive_location_detail,
            location_stripped,
            declared_content_sha256: stored.declared_content_sha256.clone(),
            origin: self.origin,
            simulation_parameters: stored.simulation_parameters.clone(),
            visible_simulation_label,
            capture_time: stored.capture_time.clone(),
            receipt_time: stored.receipt_time.clone(),
            sync_time: stored.sync_time.clone(),
            reference_freshness: stored.reference_freshness,
        }
    }
}
