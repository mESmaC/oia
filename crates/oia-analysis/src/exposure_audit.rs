//! Personal public exposure and owned-infrastructure audit kernel (`PRIVACY-02`, `A18`, `#47`).
//!
//! ## Scope and status (`AGENTS.md` §2, §3, §4, §5)
//!
//! 1. **Explicit ownership and authorization scope (`DeclaredAuditScope`):**
//!    Personal exposure and owned-infrastructure assessments require a recorded
//!    [`OwnershipBasis`] (`SelfOwnedPersonalScope` or `AuthorizedAssessmentScope`) and
//!    declared domain or `(service_name, handle)` roots before registering, matching, or
//!    auditing records. `AuthorizedAssessmentScope` requires `authorizing_party`,
//!    `authorization_reference`, `authorized_scope_summary`, and
//!    `[valid_from_unix_ms, valid_until_unix_ms]` bounds; operations outside that window
//!    are rejected (`ExposureAuditError::AuthorizationExpiredOrNotYetValid`).
//!    Domains or `(service_name, handle)` pairs outside the declared roots are rejected
//!    (`DomainOutsideDeclaredScope` / `HandleOutsideDeclaredScope`) so a discovered candidate
//!    account or linked domain outside the scope cannot be fetched, expanded, or matched
//!    further without an explicit `revise_declared_scope` revision recording its basis and
//!    rationale.
//! 2. **Strict in-scope profile claiming and dynamic scope re-checking:**
//!    A [`PublicProfileRecord`] is accepted with `claimed_in_declared_scope: true` **only**
//!    when its `(service_name, handle)` pair matches a [`DeclaredAccountHandle`] in the active
//!    [`DeclaredAuditScope`]. Listing a declared domain in `listed_website_domain` is someone
//!    else's assertion and does **not** make a third-party profile in-scope; instead, a link
//!    to a declared domain is recorded as a candidate matching signal
//!    ([`MatchSignalKind::CrossLinkToDeclaredDomain`]). Unclaimed candidate profiles must cite
//!    an `anchor_primary_profile_id` that is currently in scope so arbitrary third-party
//!    profiles cannot be stored without a tie to an in-scope primary (`PRIVACY-02`).
//!    Furthermore, every gate (`propose_account_match`, `review_account_match`) re-checks the
//!    *current* `DeclaredAuditScope` rather than relying solely on a registration-time flag,
//!    and narrowing the scope via `revise_declared_scope` marks any previously registered
//!    profiles, account matches, or infrastructure entries that fell outside the narrowed
//!    scope with `outside_current_scope == true` and `needs_reassessment == true` while
//!    retaining their history.
//! 3. **Candidate account connections retain uncertainty (`AccountMatchCandidate`):**
//!    Proposed public-profile and account-connection matches must anchor `primary_profile_id`
//!    to a profile claimed and currently covered within `DeclaredAuditScope`, preventing
//!    transitive pivoting from an external candidate profile to third-party accounts.
//!    Candidates record their matching signals, contradicting signals, `SourceCitation`
//!    (`source_id`, `source_version`, and [`ExposureCitationAnchor`]), and an uncalibrated
//!    heuristic ranking signal (`uncalibrated_heuristic_score` in `[0.0, 1.0]`). Even a `1.0`
//!    score starts as [`MatchAssessmentState::ProposedCandidate`] and never auto-promotes to
//!    established identity. `UserRejectedDifferentEntity` retains the rejected candidate, its
//!    signals, and its append-only review history so a later reviewer can inspect why it was
//!    considered and rejected (`PRIVACY-02`, `A18`).
//! 4. **Reuse of `oia_domain` primitives and proposed access policy:**
//!    Reuses [`oia_domain::witness::AccountOrigin`] (`Reported` / `Simulated`),
//!    [`oia_domain::OpaqueId`], [`oia_domain::VersionId`], [`oia_domain::ActorId`],
//!    [`oia_domain::SchemaVersion`], and delegates role/surface/clearance checks to
//!    [`oia_domain::collaboration::evaluate_proposed_access_decision`] (`#38`, proposed default
//!    pending `D10`) rather than introducing a second access-policy table.
//! 5. **Dated owned-infrastructure history and distinct timestamps (`InfrastructureHistoryEntry`):**
//!    DNS records, TLS/certificate-transparency observations, and exposed contact findings
//!    retain four distinct timestamps (`observed_at`, `published_or_issued_at`,
//!    `retrieved_at`, and `audited_at`), pinned `SourceCitation`, and explicit
//!    [`SourceQueryCoverage`] (`Complete`, `PartialBudgetReached`, `SourceUnavailable`,
//!    `ObservedAbsenceInQueriedSource`) so missing source coverage is never conflated with
//!    absence of exposure.
//! 6. **Caller-supplied timestamps and in-memory analysis kernel boundaries:**
//!    Because `oia-analysis` is a pure deterministic kernel without wall-clock I/O, all
//!    timestamps (`declared_at_unix_ms`, `revised_at_unix_ms`, `requested_at_unix_ms`, and
//!    [`ObservationTimeEnvelope`] fields) are supplied by the caller; monotonicity is enforced
//!    across revisions and retrieval/audit ordering, while wall-clock attestation belongs to
//!    the outer service layer (`#13`, `#14`). Live network queries (`#13`), link-analysis graph
//!    service integration (`#36`), SQLite persistence (`#11`, `#14`), and workspace UI (`#16`)
//!    remain open on `#47`. No Lean theorem or formal Rust-to-Lean correspondence claim is made
//!    for this slice.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::ops::Range;

use oia_domain::collaboration::{
    AccessDecision, AccessDenialReason, AccessSurface, CaseRole, ClearanceGrant, SensitivityFlags,
    evaluate_proposed_access_decision,
};
use oia_domain::witness::AccountOrigin;
use oia_domain::{ActorId, OpaqueId, SchemaVersion, VersionId};

/// Mandatory notice attached to candidate account matches (`PRIVACY-02`, `A18`, `AGENTS.md` §2).
pub const ACCOUNT_MATCH_UNCERTAINTY_NOTICE: &str = "UNCALIBRATED HEURISTIC RANKING SIGNAL ONLY: Similarity scores are uncalibrated ranking signals and do not establish identity. Ambiguous matches remain unresolved unless explicitly reviewed.";

/// Mandatory notice attached to local exposure audit sessions (`PRIVACY-02`, `A18`).
pub const LOCAL_ASSESSMENT_PRIVACY_NOTICE: &str = "LOCAL-ONLY PERSONAL EXPOSURE AUDIT: Scope declarations, private notes, and unreviewed account-match hypotheses remain local and must not be transmitted when querying public sources.";

/// Default maximum UTF-8 bytes per text field, rationale, or evidence excerpt.
pub const DEFAULT_MAX_TEXT_BYTES: usize = 4_096;
/// Default maximum declared domains or `(service_name, handle)` roots in a single [`DeclaredAuditScope`].
pub const DEFAULT_MAX_SCOPE_TARGETS: usize = 64;
/// Default maximum scope revisions retained per [`PersonalExposureAudit`].
pub const DEFAULT_MAX_SCOPE_REVISIONS: usize = 32;
/// Default maximum public profile observations retained per [`PersonalExposureAudit`].
pub const DEFAULT_MAX_PROFILES: usize = 256;
/// Default maximum candidate account matches retained per [`PersonalExposureAudit`].
pub const DEFAULT_MAX_ACCOUNT_MATCHES: usize = 256;
/// Default maximum review history entries retained per [`AccountMatchCandidate`].
pub const DEFAULT_MAX_REVIEWS_PER_MATCH: usize = 64;
/// Default maximum owned-infrastructure history entries retained per [`PersonalExposureAudit`].
pub const DEFAULT_MAX_INFRASTRUCTURE_ENTRIES: usize = 512;
/// Default maximum source-coverage records retained per [`PersonalExposureAudit`].
pub const DEFAULT_MAX_COVERAGE_RECORDS: usize = 256;

/// Common multi-label public suffixes rejected as bare declared roots so a user cannot
/// declare a public suffix like `co.uk` and match unrelated third-party domains.
const DISALLOWED_PUBLIC_SUFFIX_ROOTS: &[&str] = &[
    "co.uk", "org.uk", "gov.uk", "ac.uk", "net.uk", "me.uk", "com.au", "net.au", "org.au",
    "edu.au", "gov.au", "co.nz", "org.nz", "net.nz", "co.jp", "ne.jp", "or.jp", "ac.jp", "go.jp",
    "com.br", "org.br", "net.br", "co.in", "org.in", "net.in", "gov.in", "ac.in", "co.za",
    "org.za", "net.za", "com.mx", "org.mx", "com.sg", "org.sg", "com.hk", "org.hk", "co.kr",
    "or.kr", "com.cn", "org.cn", "net.cn", "com.tw", "org.tw", "eu.org",
];

/// Which resource limit was zero or exceeded during an exposure audit operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditLimitKind {
    TextBytes,
    ScopeTargets,
    ScopeRevisions,
    Profiles,
    AccountMatches,
    ReviewsPerMatch,
    InfrastructureEntries,
    CoverageRecords,
}

impl fmt::Display for AuditLimitKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TextBytes => f.write_str("max_text_bytes"),
            Self::ScopeTargets => f.write_str("max_scope_targets"),
            Self::ScopeRevisions => f.write_str("max_scope_revisions"),
            Self::Profiles => f.write_str("max_profiles"),
            Self::AccountMatches => f.write_str("max_account_matches"),
            Self::ReviewsPerMatch => f.write_str("max_reviews_per_match"),
            Self::InfrastructureEntries => f.write_str("max_infrastructure_entries"),
            Self::CoverageRecords => f.write_str("max_coverage_records"),
        }
    }
}

/// Explicit resource bounds for a [`PersonalExposureAudit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExposureAuditLimits {
    max_text_bytes: usize,
    max_scope_targets: usize,
    max_scope_revisions: usize,
    max_profiles: usize,
    max_account_matches: usize,
    max_reviews_per_match: usize,
    max_infrastructure_entries: usize,
    max_coverage_records: usize,
}

impl ExposureAuditLimits {
    /// Validates that all resource limits are non-zero.
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        max_text_bytes: usize,
        max_scope_targets: usize,
        max_scope_revisions: usize,
        max_profiles: usize,
        max_account_matches: usize,
        max_reviews_per_match: usize,
        max_infrastructure_entries: usize,
        max_coverage_records: usize,
    ) -> Result<Self, ExposureAuditError> {
        if max_text_bytes == 0 {
            return Err(ExposureAuditError::ZeroLimit(AuditLimitKind::TextBytes));
        }
        if max_scope_targets == 0 {
            return Err(ExposureAuditError::ZeroLimit(AuditLimitKind::ScopeTargets));
        }
        if max_scope_revisions == 0 {
            return Err(ExposureAuditError::ZeroLimit(
                AuditLimitKind::ScopeRevisions,
            ));
        }
        if max_profiles == 0 {
            return Err(ExposureAuditError::ZeroLimit(AuditLimitKind::Profiles));
        }
        if max_account_matches == 0 {
            return Err(ExposureAuditError::ZeroLimit(
                AuditLimitKind::AccountMatches,
            ));
        }
        if max_reviews_per_match == 0 {
            return Err(ExposureAuditError::ZeroLimit(
                AuditLimitKind::ReviewsPerMatch,
            ));
        }
        if max_infrastructure_entries == 0 {
            return Err(ExposureAuditError::ZeroLimit(
                AuditLimitKind::InfrastructureEntries,
            ));
        }
        if max_coverage_records == 0 {
            return Err(ExposureAuditError::ZeroLimit(
                AuditLimitKind::CoverageRecords,
            ));
        }
        Ok(Self {
            max_text_bytes,
            max_scope_targets,
            max_scope_revisions,
            max_profiles,
            max_account_matches,
            max_reviews_per_match,
            max_infrastructure_entries,
            max_coverage_records,
        })
    }

    #[must_use]
    pub const fn max_text_bytes(self) -> usize {
        self.max_text_bytes
    }

    #[must_use]
    pub const fn max_scope_targets(self) -> usize {
        self.max_scope_targets
    }

    #[must_use]
    pub const fn max_scope_revisions(self) -> usize {
        self.max_scope_revisions
    }

    #[must_use]
    pub const fn max_profiles(self) -> usize {
        self.max_profiles
    }

    #[must_use]
    pub const fn max_account_matches(self) -> usize {
        self.max_account_matches
    }

    #[must_use]
    pub const fn max_reviews_per_match(self) -> usize {
        self.max_reviews_per_match
    }

    #[must_use]
    pub const fn max_infrastructure_entries(self) -> usize {
        self.max_infrastructure_entries
    }

    #[must_use]
    pub const fn max_coverage_records(self) -> usize {
        self.max_coverage_records
    }
}

impl Default for ExposureAuditLimits {
    fn default() -> Self {
        Self {
            max_text_bytes: DEFAULT_MAX_TEXT_BYTES,
            max_scope_targets: DEFAULT_MAX_SCOPE_TARGETS,
            max_scope_revisions: DEFAULT_MAX_SCOPE_REVISIONS,
            max_profiles: DEFAULT_MAX_PROFILES,
            max_account_matches: DEFAULT_MAX_ACCOUNT_MATCHES,
            max_reviews_per_match: DEFAULT_MAX_REVIEWS_PER_MATCH,
            max_infrastructure_entries: DEFAULT_MAX_INFRASTRUCTURE_ENTRIES,
            max_coverage_records: DEFAULT_MAX_COVERAGE_RECORDS,
        }
    }
}

/// Errors returned by personal exposure and owned-infrastructure audit operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExposureAuditError {
    ZeroLimit(AuditLimitKind),
    LimitExceeded {
        limit: AuditLimitKind,
        actual: usize,
        max: usize,
    },
    EmptyField(&'static str),
    FieldTooLong {
        field: &'static str,
        actual: usize,
        max: usize,
    },
    ControlCharacter(&'static str),
    EmptyDeclaredScope,
    InvalidDomainSyntax,
    PublicSuffixRootDisallowed {
        domain: String,
    },
    DuplicateScopeTarget,
    InvalidAuthorizationWindow,
    AuthorizationExpiredOrNotYetValid {
        action_unix_ms: i64,
        valid_from_unix_ms: i64,
        valid_until_unix_ms: i64,
    },
    UnauthorizedActor {
        expected: ActorId,
        actual: ActorId,
    },
    DomainOutsideDeclaredScope {
        candidate_domain: String,
    },
    HandleOutsideDeclaredScope,
    PrimaryProfileMustBeInDeclaredScope {
        profile_id: OpaqueId,
    },
    UnclaimedProfileRequiresInScopePrimaryAnchor,
    InvalidHeuristicScore,
    EmptyMatchingSignals,
    SameProfileAccountMatch,
    InvalidCitationAnchor(&'static str),
    InvalidCertificateFingerprint,
    InvalidCalendarDate,
    UnknownProfileId(OpaqueId),
    UnknownAccountMatchId(OpaqueId),
    DuplicateIdentifier {
        kind: &'static str,
        id: OpaqueId,
    },
    DuplicateVersion(VersionId),
    StaleExpectedVersion {
        expected: VersionId,
        actual: VersionId,
    },
    InvalidDecisionTransition(&'static str),
    InvalidTimestampOrder(&'static str),
    InvalidValidityInterval,
    AccessDenied(AccessDenialReason),
    PrivateAssessmentLeakBlocked(&'static str),
    AllocationFailed,
}

impl fmt::Display for ExposureAuditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroLimit(kind) => write!(f, "resource limit `{kind}` must be non-zero"),
            Self::LimitExceeded { limit, actual, max } => {
                write!(
                    f,
                    "resource limit `{limit}` exceeded: {actual} exceeds {max}"
                )
            }
            Self::EmptyField(field) => write!(f, "field `{field}` must not be empty"),
            Self::FieldTooLong { field, actual, max } => {
                write!(
                    f,
                    "field `{field}` byte length {actual} exceeds limit {max}"
                )
            }
            Self::ControlCharacter(field) => {
                write!(f, "field `{field}` contains disallowed control characters")
            }
            Self::EmptyDeclaredScope => {
                f.write_str("declared audit scope must include at least one domain or handle root")
            }
            Self::InvalidDomainSyntax => f.write_str(
                "domain must be a valid DNS hostname with at least one dot, a non-numeric TLD, and no protocol or IP literal",
            ),
            Self::PublicSuffixRootDisallowed { domain } => write!(
                f,
                "domain `{domain}` is a shared public suffix and cannot be declared as an owned root domain"
            ),
            Self::DuplicateScopeTarget => {
                f.write_str("declared audit scope contains a duplicate domain or (service_name, handle) target")
            }
            Self::InvalidAuthorizationWindow => f.write_str(
                "authorized assessment window valid_until_unix_ms must be >= valid_from_unix_ms",
            ),
            Self::AuthorizationExpiredOrNotYetValid {
                action_unix_ms,
                valid_from_unix_ms,
                valid_until_unix_ms,
            } => write!(
                f,
                "timestamp {action_unix_ms} is outside authorized assessment window [{valid_from_unix_ms}, {valid_until_unix_ms}]"
            ),
            Self::UnauthorizedActor { expected, actual } => write!(
                f,
                "actor `{actual}` is not the declared audit actor `{expected}`"
            ),
            Self::DomainOutsideDeclaredScope { candidate_domain } => write!(
                f,
                "domain `{candidate_domain}` is outside the declared audit scope; widening scope requires an explicit DeclaredAuditScope revision"
            ),
            Self::HandleOutsideDeclaredScope => f.write_str(
                "(service_name, handle) is outside the declared audit scope; out-of-scope candidate handles cannot be claimed, fetched, or expanded without an explicit DeclaredAuditScope revision",
            ),
            Self::PrimaryProfileMustBeInDeclaredScope { profile_id } => write!(
                f,
                "primary profile `{profile_id}` must be claimed and currently covered within the declared audit scope; pivoting from an unconfirmed or out-of-scope profile is refused"
            ),
            Self::UnclaimedProfileRequiresInScopePrimaryAnchor => f.write_str(
                "unclaimed candidate profile must reference an in-scope anchor_primary_profile_id to prevent storing unanchored third-party profiles"
            ),
            Self::InvalidHeuristicScore => f.write_str(
                "uncalibrated heuristic ranking score must be finite and within [0.0, 1.0]",
            ),
            Self::EmptyMatchingSignals => {
                f.write_str("account match candidate must cite at least one matching signal")
            }
            Self::SameProfileAccountMatch => {
                f.write_str("account match candidate must compare two distinct profile records")
            }
            Self::InvalidCitationAnchor(reason) => {
                write!(f, "invalid source citation anchor: {reason}")
            }
            Self::InvalidCertificateFingerprint => f.write_str(
                "certificate_sha256_fingerprint must be 64 ASCII hexadecimal characters",
            ),
            Self::InvalidCalendarDate => {
                f.write_str("DateOnly calendar_date must be a valid YYYY-MM-DD calendar date")
            }
            Self::UnknownProfileId(id) => write!(f, "unknown public profile record `{id}`"),
            Self::UnknownAccountMatchId(id) => {
                write!(f, "unknown account match candidate `{id}`")
            }
            Self::DuplicateIdentifier { kind, id } => {
                write!(f, "duplicate {kind} identifier `{id}`")
            }
            Self::DuplicateVersion(ver) => {
                write!(f, "version identifier `{ver}` was already used")
            }
            Self::StaleExpectedVersion { expected, actual } => write!(
                f,
                "expected version `{expected}`, but current version is `{actual}`"
            ),
            Self::InvalidDecisionTransition(reason) => {
                write!(f, "invalid account-match review transition: {reason}")
            }
            Self::InvalidTimestampOrder(reason) => {
                write!(f, "invalid timestamp relationship: {reason}")
            }
            Self::InvalidValidityInterval => {
                f.write_str("validity end timestamp must not precede validity start timestamp")
            }
            Self::AccessDenied(reason) => {
                write!(f, "proposed access policy denied operation: {reason}")
            }
            Self::PrivateAssessmentLeakBlocked(reason) => write!(
                f,
                "outbound source query blocked to preserve local assessment privacy: {reason}"
            ),
            Self::AllocationFailed => f.write_str("memory reservation failed"),
        }
    }
}

impl std::error::Error for ExposureAuditError {}

/// Owned citation anchor referencing an exact byte range, extracted passage, or structured record field (`docs/architecture.md` §2.1).
///
/// Note on relationship to `oia_export::CitationAnchor`: `oia-export` defines a borrowed
/// `CitationAnchor<'input>` over in-memory report slices (`ByteRange`, `Passage`, `Region`,
/// `TimestampRange`) and sits above `oia-analysis` in the workspace dependency graph.
/// `ExposureCitationAnchor` is the owned equivalent for retained audit state:
/// `Utf8ByteRange` maps to `CitationAnchor::ByteRange`, `ExtractedPassage` maps to
/// `CitationAnchor::Passage`, and `StructuredRecordField` addresses structured DNS/CT/WHOIS/profile
/// fields before report serialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExposureCitationAnchor {
    /// Half-open UTF-8 byte range `start..end` within the retained source artifact.
    Utf8ByteRange(Range<usize>),
    /// Anchored passage line and byte range within a versioned extraction.
    ExtractedPassage {
        extraction_version: VersionId,
        passage_line: usize,
        byte_range: Range<usize>,
    },
    /// Structured record locator (e.g. DNS RR index, CT log entry index, or profile field selector).
    StructuredRecordField {
        record_locator: String,
        field_name: String,
    },
}

impl ExposureCitationAnchor {
    fn validate(&self, limits: ExposureAuditLimits) -> Result<(), ExposureAuditError> {
        match self {
            Self::Utf8ByteRange(range) => {
                if range.is_empty() {
                    return Err(ExposureAuditError::InvalidCitationAnchor(
                        "UTF-8 byte range anchor must be non-empty (start < end)",
                    ));
                }
                Ok(())
            }
            Self::ExtractedPassage {
                passage_line,
                byte_range,
                ..
            } => {
                if *passage_line == 0 {
                    return Err(ExposureAuditError::InvalidCitationAnchor(
                        "passage_line is 1-indexed and must be >= 1",
                    ));
                }
                if byte_range.is_empty() {
                    return Err(ExposureAuditError::InvalidCitationAnchor(
                        "extracted passage byte_range must be non-empty (start < end)",
                    ));
                }
                Ok(())
            }
            Self::StructuredRecordField {
                record_locator,
                field_name,
            } => {
                validate_text("citation.record_locator", record_locator, limits)?;
                validate_text("citation.field_name", field_name, limits)?;
                Ok(())
            }
        }
    }
}

/// Pinned source record reference (`source_id`, `source_version`, and [`ExposureCitationAnchor`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceCitation {
    pub source_id: OpaqueId,
    pub source_version: VersionId,
    pub anchor: ExposureCitationAnchor,
}

impl SourceCitation {
    fn validate(&self, limits: ExposureAuditLimits) -> Result<(), ExposureAuditError> {
        self.anchor.validate(limits)
    }
}

/// Service-qualified account handle declared in [`DeclaredAuditScope`] (`PRIVACY-02`, `A18`).
///
/// Pairing `service_name` with `handle` ensures that declaring handle `fictional_a` on one
/// service does not automatically treat `fictional_a` on a different service as user-claimed.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeclaredAccountHandle {
    service_name: String,
    handle: String,
}

impl DeclaredAccountHandle {
    /// Creates a `(service_name, handle)` pair; normalization is applied when building [`DeclaredAuditScope`].
    #[must_use]
    pub fn new(service_name: impl Into<String>, handle: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
            handle: handle.into(),
        }
    }

    #[must_use]
    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    #[must_use]
    pub fn handle(&self) -> &str {
        &self.handle
    }

    fn normalize_and_validate(
        &self,
        limits: ExposureAuditLimits,
    ) -> Result<Self, ExposureAuditError> {
        let norm_service = normalize_service_name(&self.service_name, limits)?;
        let norm_handle = normalize_handle(&self.handle, limits)?;
        Ok(Self {
            service_name: norm_service,
            handle: norm_handle,
        })
    }
}

/// Recorded ownership or authorization basis for a personal exposure assessment (`PRIVACY-02`, `A18`).
#[derive(Clone, PartialEq, Eq)]
pub enum OwnershipBasis {
    /// The user is auditing their own personal public profiles and owned domains.
    SelfOwnedPersonalScope {
        owner_actor: ActorId,
        verification_note: String,
    },
    /// The user holds explicit, time-bounded authorization from the asset owner for a scoped assessment.
    AuthorizedAssessmentScope {
        auditor_actor: ActorId,
        authorizing_party: String,
        authorization_reference: String,
        authorized_scope_summary: String,
        valid_from_unix_ms: i64,
        valid_until_unix_ms: i64,
        verification_note: String,
    },
}

impl OwnershipBasis {
    /// Returns the actor authorized to perform the audit and record match reviews.
    #[must_use]
    pub fn actor(&self) -> &ActorId {
        match self {
            Self::SelfOwnedPersonalScope { owner_actor, .. } => owner_actor,
            Self::AuthorizedAssessmentScope { auditor_actor, .. } => auditor_actor,
        }
    }

    /// Returns the local verification note (redacted in `fmt::Debug`).
    #[must_use]
    pub fn verification_note(&self) -> &str {
        match self {
            Self::SelfOwnedPersonalScope {
                verification_note, ..
            }
            | Self::AuthorizedAssessmentScope {
                verification_note, ..
            } => verification_note,
        }
    }

    fn validate(&self, limits: ExposureAuditLimits) -> Result<(), ExposureAuditError> {
        match self {
            Self::SelfOwnedPersonalScope {
                verification_note, ..
            } => validate_text("verification_note", verification_note, limits),
            Self::AuthorizedAssessmentScope {
                authorizing_party,
                authorization_reference,
                authorized_scope_summary,
                valid_from_unix_ms,
                valid_until_unix_ms,
                verification_note,
                ..
            } => {
                validate_text("authorizing_party", authorizing_party, limits)?;
                validate_text("authorization_reference", authorization_reference, limits)?;
                validate_text("authorized_scope_summary", authorized_scope_summary, limits)?;
                validate_text("verification_note", verification_note, limits)?;
                if valid_until_unix_ms < valid_from_unix_ms {
                    return Err(ExposureAuditError::InvalidAuthorizationWindow);
                }
                Ok(())
            }
        }
    }

    fn check_timestamp_within_authorization(
        &self,
        action_unix_ms: i64,
    ) -> Result<(), ExposureAuditError> {
        if let Self::AuthorizedAssessmentScope {
            valid_from_unix_ms,
            valid_until_unix_ms,
            ..
        } = self
            && (action_unix_ms < *valid_from_unix_ms || action_unix_ms > *valid_until_unix_ms)
        {
            return Err(ExposureAuditError::AuthorizationExpiredOrNotYetValid {
                action_unix_ms,
                valid_from_unix_ms: *valid_from_unix_ms,
                valid_until_unix_ms: *valid_until_unix_ms,
            });
        }
        Ok(())
    }
}

impl fmt::Debug for OwnershipBasis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SelfOwnedPersonalScope { owner_actor, .. } => f
                .debug_struct("SelfOwnedPersonalScope")
                .field("owner_actor", owner_actor)
                .field("verification_note", &"<redacted-local-note>")
                .finish(),
            Self::AuthorizedAssessmentScope {
                auditor_actor,
                authorizing_party,
                authorization_reference,
                authorized_scope_summary,
                valid_from_unix_ms,
                valid_until_unix_ms,
                ..
            } => f
                .debug_struct("AuthorizedAssessmentScope")
                .field("auditor_actor", auditor_actor)
                .field("authorizing_party", authorizing_party)
                .field("authorization_reference", authorization_reference)
                .field("authorized_scope_summary", authorized_scope_summary)
                .field("valid_from_unix_ms", valid_from_unix_ms)
                .field("valid_until_unix_ms", valid_until_unix_ms)
                .field("verification_note", &"<redacted-local-note>")
                .finish(),
        }
    }
}

/// Inspectable declaration of the user's owned or authorized domains and `(service_name, handle)` roots (`PRIVACY-02`, `A18`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredAuditScope {
    scope_id: OpaqueId,
    scope_version: VersionId,
    predecessor_scope_version: Option<VersionId>,
    declared_at_unix_ms: i64,
    ownership_basis: OwnershipBasis,
    declared_root_domains: Vec<String>,
    declared_handles: Vec<DeclaredAccountHandle>,
    revision_rationale: Option<String>,
}

impl DeclaredAuditScope {
    /// Validates and constructs an initial [`DeclaredAuditScope`].
    pub fn new(
        scope_id: OpaqueId,
        scope_version: VersionId,
        declared_at_unix_ms: i64,
        ownership_basis: OwnershipBasis,
        declared_root_domains: Vec<String>,
        declared_handles: Vec<DeclaredAccountHandle>,
        limits: ExposureAuditLimits,
    ) -> Result<Self, ExposureAuditError> {
        Self::build(
            scope_id,
            scope_version,
            None,
            declared_at_unix_ms,
            ownership_basis,
            declared_root_domains,
            declared_handles,
            None,
            limits,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn build(
        scope_id: OpaqueId,
        scope_version: VersionId,
        predecessor_scope_version: Option<VersionId>,
        declared_at_unix_ms: i64,
        ownership_basis: OwnershipBasis,
        declared_root_domains: Vec<String>,
        declared_handles: Vec<DeclaredAccountHandle>,
        revision_rationale: Option<String>,
        limits: ExposureAuditLimits,
    ) -> Result<Self, ExposureAuditError> {
        ownership_basis.validate(limits)?;
        ownership_basis.check_timestamp_within_authorization(declared_at_unix_ms)?;

        if let Some(rationale) = &revision_rationale {
            validate_text("revision_rationale", rationale, limits)?;
        }

        let total_targets = declared_root_domains
            .len()
            .saturating_add(declared_handles.len());
        if total_targets == 0 {
            return Err(ExposureAuditError::EmptyDeclaredScope);
        }
        if total_targets > limits.max_scope_targets {
            return Err(ExposureAuditError::LimitExceeded {
                limit: AuditLimitKind::ScopeTargets,
                actual: total_targets,
                max: limits.max_scope_targets,
            });
        }

        let mut normalized_domains = Vec::new();
        normalized_domains
            .try_reserve(declared_root_domains.len())
            .map_err(|_| ExposureAuditError::AllocationFailed)?;
        let mut seen_domains = BTreeSet::new();
        for domain in declared_root_domains {
            let norm = normalize_and_validate_root_domain(&domain, limits)?;
            if !seen_domains.insert(norm.clone()) {
                return Err(ExposureAuditError::DuplicateScopeTarget);
            }
            normalized_domains.push(norm);
        }

        let mut normalized_handles = Vec::new();
        normalized_handles
            .try_reserve(declared_handles.len())
            .map_err(|_| ExposureAuditError::AllocationFailed)?;
        let mut seen_handles = BTreeSet::new();
        for pair in declared_handles {
            let norm_pair = pair.normalize_and_validate(limits)?;
            if !seen_handles.insert(norm_pair.clone()) {
                return Err(ExposureAuditError::DuplicateScopeTarget);
            }
            normalized_handles.push(norm_pair);
        }

        Ok(Self {
            scope_id,
            scope_version,
            predecessor_scope_version,
            declared_at_unix_ms,
            ownership_basis,
            declared_root_domains: normalized_domains,
            declared_handles: normalized_handles,
            revision_rationale,
        })
    }

    #[must_use]
    pub fn scope_id(&self) -> &OpaqueId {
        &self.scope_id
    }

    #[must_use]
    pub fn scope_version(&self) -> &VersionId {
        &self.scope_version
    }

    #[must_use]
    pub fn predecessor_scope_version(&self) -> Option<&VersionId> {
        self.predecessor_scope_version.as_ref()
    }

    #[must_use]
    pub const fn declared_at_unix_ms(&self) -> i64 {
        self.declared_at_unix_ms
    }

    #[must_use]
    pub fn ownership_basis(&self) -> &OwnershipBasis {
        &self.ownership_basis
    }

    #[must_use]
    pub fn declared_root_domains(&self) -> &[String] {
        &self.declared_root_domains
    }

    #[must_use]
    pub fn declared_handles(&self) -> &[DeclaredAccountHandle] {
        &self.declared_handles
    }

    #[must_use]
    pub fn revision_rationale(&self) -> Option<&str> {
        self.revision_rationale.as_deref()
    }

    /// Returns `true` if `candidate_domain` equals a declared root domain or is a subdomain of one
    /// on a strict dot boundary (`sub.example.com` matches `example.com`, whereas `badexample.com` does not).
    #[must_use]
    pub fn covers_domain(&self, candidate_domain: &str) -> bool {
        let Ok(norm) = normalize_domain_str(candidate_domain) else {
            return false;
        };
        self.declared_root_domains.iter().any(|root| {
            norm == *root
                || (norm.len() > root.len()
                    && norm.ends_with(root.as_str())
                    && norm.as_bytes().get(norm.len() - root.len() - 1) == Some(&b'.'))
        })
    }

    /// Returns `true` if `(service_name, handle)` matches a declared `(service_name, handle)` pair
    /// (case-insensitively after trimming).
    #[must_use]
    pub fn covers_handle(&self, service_name: &str, handle: &str) -> bool {
        let norm_service = service_name.trim().to_ascii_lowercase();
        let norm_handle = handle.trim().to_ascii_lowercase();
        self.declared_handles
            .iter()
            .any(|pair| pair.service_name == norm_service && pair.handle == norm_handle)
    }
}

/// Input for explicitly revising/widening or narrowing a [`DeclaredAuditScope`] with a recorded basis and rationale (`PRIVACY-02`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeRevisionInput {
    pub expected_scope_version: VersionId,
    pub new_scope_version: VersionId,
    pub revised_at_unix_ms: i64,
    pub ownership_basis: OwnershipBasis,
    pub declared_root_domains: Vec<String>,
    pub declared_handles: Vec<DeclaredAccountHandle>,
    pub revision_rationale: String,
}

/// Precision and timezone reference for a timestamp (`AGENTS.md` §2, `docs/architecture.md` §3.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditTimePrecision {
    /// Exact instant with known UTC timestamp.
    ExactInstant { original_tz_label: String },
    /// Calendar date only (`YYYY-MM-DD`); must not be presented as an exact midnight observation.
    DateOnly { calendar_date: String },
    /// Approximate instant with symmetric uncertainty in milliseconds.
    Approximate {
        uncertainty_ms: u64,
        original_tz_label: String,
    },
}

/// Dated timestamp keeping observation, publication, retrieval, and audit times distinct (`AGENTS.md` §2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditTimestamp {
    pub unix_ms: i64,
    pub precision: AuditTimePrecision,
}

impl AuditTimestamp {
    fn validate(
        &self,
        field: &'static str,
        limits: ExposureAuditLimits,
    ) -> Result<(), ExposureAuditError> {
        match &self.precision {
            AuditTimePrecision::ExactInstant { original_tz_label }
            | AuditTimePrecision::Approximate {
                original_tz_label, ..
            } => validate_text(field, original_tz_label, limits),
            AuditTimePrecision::DateOnly { calendar_date } => {
                validate_text(field, calendar_date, limits)?;
                validate_iso_calendar_date(calendar_date)
            }
        }
    }
}

/// Distinct timestamps for an infrastructure or profile observation (`AGENTS.md` §2, `docs/architecture.md` §3.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationTimeEnvelope {
    /// When the record was observed in the wild (if known). Certificate validity bounds
    /// (`not_before_unix_ms`, `not_after_unix_ms`) are stored separately on [`InfrastructureArtifactKind::CertificateHistory`].
    pub observed_at: Option<AuditTimestamp>,
    /// When the source published or issued the record (e.g. CT log entry or WHOIS/DNS snapshot time).
    pub published_or_issued_at: Option<AuditTimestamp>,
    /// When the record was retrieved from the source.
    pub retrieved_at: AuditTimestamp,
    /// When OIA recorded or evaluated the entry in the local audit.
    pub audited_at: AuditTimestamp,
}

impl ObservationTimeEnvelope {
    fn validate(&self, limits: ExposureAuditLimits) -> Result<(), ExposureAuditError> {
        if let Some(obs) = &self.observed_at {
            obs.validate("observed_at", limits)?;
        }
        if let Some(pub_at) = &self.published_or_issued_at {
            pub_at.validate("published_or_issued_at", limits)?;
        }
        self.retrieved_at.validate("retrieved_at", limits)?;
        self.audited_at.validate("audited_at", limits)?;
        if self.audited_at.unix_ms < self.retrieved_at.unix_ms {
            return Err(ExposureAuditError::InvalidTimestampOrder(
                "audited_at must not precede retrieved_at",
            ));
        }
        Ok(())
    }
}

/// Public profile observation input (`PRIVACY-02`, `A18`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicProfileInput {
    pub profile_id: OpaqueId,
    pub version_id: VersionId,
    pub origin: AccountOrigin,
    pub service_name: String,
    pub handle: String,
    pub display_name: Option<String>,
    pub bio_excerpt: Option<String>,
    pub listed_website_domain: Option<String>,
    pub exposed_contact_values: Vec<String>,
    /// If `true`, `(service_name, handle)` must match a [`DeclaredAccountHandle`] in `current_scope`.
    /// Listing an in-scope `listed_website_domain` alone does **not** make a profile claimed.
    pub claimed_in_declared_scope: bool,
    /// Required when `claimed_in_declared_scope == false`: ties an unclaimed candidate profile
    /// to a registered, currently in-scope primary profile so unrelated third-party profiles
    /// cannot be stored without an in-scope anchor (`PRIVACY-02`).
    pub anchor_primary_profile_id: Option<OpaqueId>,
    pub citation: SourceCitation,
    pub timestamps: ObservationTimeEnvelope,
}

/// Stored public profile record with provenance, scope status, and reassessment state (`PRIVACY-02`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicProfileRecord {
    profile_id: OpaqueId,
    version_id: VersionId,
    origin: AccountOrigin,
    service_name: String,
    handle: String,
    display_name: Option<String>,
    bio_excerpt: Option<String>,
    listed_website_domain: Option<String>,
    exposed_contact_values: Vec<String>,
    claimed_in_declared_scope: bool,
    anchor_primary_profile_id: Option<OpaqueId>,
    outside_current_scope: bool,
    citation: SourceCitation,
    timestamps: ObservationTimeEnvelope,
    needs_reassessment: bool,
    reassessment_reason: Option<String>,
}

impl PublicProfileRecord {
    #[must_use]
    pub fn profile_id(&self) -> &OpaqueId {
        &self.profile_id
    }

    #[must_use]
    pub fn version_id(&self) -> &VersionId {
        &self.version_id
    }

    #[must_use]
    pub const fn origin(&self) -> AccountOrigin {
        self.origin
    }

    #[must_use]
    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    #[must_use]
    pub fn handle(&self) -> &str {
        &self.handle
    }

    #[must_use]
    pub fn display_name(&self) -> Option<&str> {
        self.display_name.as_deref()
    }

    #[must_use]
    pub fn bio_excerpt(&self) -> Option<&str> {
        self.bio_excerpt.as_deref()
    }

    #[must_use]
    pub fn listed_website_domain(&self) -> Option<&str> {
        self.listed_website_domain.as_deref()
    }

    #[must_use]
    pub fn exposed_contact_values(&self) -> &[String] {
        &self.exposed_contact_values
    }

    #[must_use]
    pub const fn claimed_in_declared_scope(&self) -> bool {
        self.claimed_in_declared_scope
    }

    #[must_use]
    pub fn anchor_primary_profile_id(&self) -> Option<&OpaqueId> {
        self.anchor_primary_profile_id.as_ref()
    }

    /// Returns `true` if a subsequent `revise_declared_scope` narrowed the scope so this
    /// profile (or its anchor primary profile) is no longer covered by `current_scope`.
    #[must_use]
    pub const fn outside_current_scope(&self) -> bool {
        self.outside_current_scope
    }

    #[must_use]
    pub fn citation(&self) -> &SourceCitation {
        &self.citation
    }

    #[must_use]
    pub fn timestamps(&self) -> &ObservationTimeEnvelope {
        &self.timestamps
    }

    #[must_use]
    pub const fn needs_reassessment(&self) -> bool {
        self.needs_reassessment
    }

    #[must_use]
    pub fn reassessment_reason(&self) -> Option<&str> {
        self.reassessment_reason.as_deref()
    }
}

/// Kind of signal supporting a proposed account-connection candidate (`PRIVACY-02`, `A18`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchSignalKind {
    ExactHandleReuse,
    SimilarHandlePattern,
    DisplayNameSimilarity,
    CrossLinkToDeclaredDomain,
    MutualProfileLink,
    SharedExposedContactAddress,
    SharedPgpOrKeyFingerprint,
    BiographyPhraseOverlap,
}

/// Evidence signal cited in a candidate account-connection match (`PRIVACY-02`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountMatchSignal {
    pub kind: MatchSignalKind,
    pub description: String,
    pub citation: SourceCitation,
}

/// Epistemic assessment state of a candidate account-connection match (`PRIVACY-02`, `A18`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchAssessmentState {
    /// Initial proposed candidate match; never treated as established identity.
    ProposedCandidate,
    /// Reviewed and explicitly retained as ambiguous due to conflicting or insufficient evidence.
    AmbiguousRetained,
    /// Explicitly confirmed by the user as belonging to the declared scope/owner.
    UserConfirmedSameOwner,
    /// Explicitly rejected by the user as a different person or unrelated entity; retained in the audit trail.
    UserRejectedDifferentEntity,
}

impl MatchAssessmentState {
    /// Returns `true` only when the user has explicitly confirmed ownership (`UserConfirmedSameOwner`),
    /// and even then this represents a user-scoped audit confirmation rather than an automated proof of identity.
    #[must_use]
    pub const fn is_user_confirmed(self) -> bool {
        matches!(self, Self::UserConfirmedSameOwner)
    }

    /// Automated heuristic matching never establishes identity (`PRIVACY-02`, `A18`, `AGENTS.md` §2).
    #[must_use]
    pub const fn establishes_automated_identity(self) -> bool {
        false
    }
}

/// Input for recording an explicit user review transition on a candidate account match (`PRIVACY-02`, `A18`).
#[derive(Clone, PartialEq, Eq)]
pub struct AccountMatchReviewInput {
    pub match_id: OpaqueId,
    pub expected_version: VersionId,
    pub new_version: VersionId,
    pub reviewer: ActorId,
    pub reviewed_at: AuditTimestamp,
    pub new_state: MatchAssessmentState,
    pub private_rationale_note: String,
}

impl fmt::Debug for AccountMatchReviewInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AccountMatchReviewInput")
            .field("match_id", &self.match_id)
            .field("expected_version", &self.expected_version)
            .field("new_version", &self.new_version)
            .field("reviewer", &self.reviewer)
            .field("reviewed_at", &self.reviewed_at)
            .field("new_state", &self.new_state)
            .field("private_rationale_note", &"<redacted-local-note>")
            .finish()
    }
}

/// Immutable review history entry for a candidate account match (`PRIVACY-02`, `A18`).
#[derive(Clone, PartialEq, Eq)]
pub struct AccountMatchReviewEntry {
    review_version: VersionId,
    reviewer: ActorId,
    reviewed_at: AuditTimestamp,
    previous_state: MatchAssessmentState,
    new_state: MatchAssessmentState,
    private_rationale_note: String,
}

impl AccountMatchReviewEntry {
    #[must_use]
    pub fn review_version(&self) -> &VersionId {
        &self.review_version
    }

    #[must_use]
    pub fn reviewer(&self) -> &ActorId {
        &self.reviewer
    }

    #[must_use]
    pub fn reviewed_at(&self) -> &AuditTimestamp {
        &self.reviewed_at
    }

    #[must_use]
    pub const fn previous_state(&self) -> MatchAssessmentState {
        self.previous_state
    }

    #[must_use]
    pub const fn new_state(&self) -> MatchAssessmentState {
        self.new_state
    }

    #[must_use]
    pub fn private_rationale_note(&self) -> &str {
        &self.private_rationale_note
    }
}

impl fmt::Debug for AccountMatchReviewEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AccountMatchReviewEntry")
            .field("review_version", &self.review_version)
            .field("reviewer", &self.reviewer)
            .field("reviewed_at", &self.reviewed_at)
            .field("previous_state", &self.previous_state)
            .field("new_state", &self.new_state)
            .field("private_rationale_note", &"<redacted-local-note>")
            .finish()
    }
}

/// Input for proposing a candidate connection between an in-scope profile (`primary_profile_id`)
/// and another public profile (`candidate_profile_id`) (`PRIVACY-02`, `A18`).
#[derive(Debug, Clone, PartialEq)]
pub struct AccountMatchInput {
    pub match_id: OpaqueId,
    pub version_id: VersionId,
    /// Must reference a profile claimed and currently covered within `DeclaredAuditScope`.
    pub primary_profile_id: OpaqueId,
    pub candidate_profile_id: OpaqueId,
    /// Uncalibrated heuristic ranking score in `[0.0, 1.0]`. Never establishes identity.
    pub uncalibrated_heuristic_score: f64,
    pub matching_signals: Vec<AccountMatchSignal>,
    pub contradicting_signals: Vec<String>,
    pub ambiguity_explanation: String,
    pub proposed_at: AuditTimestamp,
}

/// Candidate account-connection match retaining uncertainty and review history (`PRIVACY-02`, `A18`).
#[derive(Debug, Clone, PartialEq)]
pub struct AccountMatchCandidate {
    match_id: OpaqueId,
    current_version: VersionId,
    primary_profile_id: OpaqueId,
    candidate_profile_id: OpaqueId,
    uncalibrated_heuristic_score: f64,
    matching_signals: Vec<AccountMatchSignal>,
    contradicting_signals: Vec<String>,
    ambiguity_explanation: String,
    uncertainty_notice: &'static str,
    assessment_state: MatchAssessmentState,
    proposed_at: AuditTimestamp,
    review_history: Vec<AccountMatchReviewEntry>,
    outside_current_scope: bool,
    needs_reassessment: bool,
    reassessment_reason: Option<String>,
}

impl AccountMatchCandidate {
    #[must_use]
    pub fn match_id(&self) -> &OpaqueId {
        &self.match_id
    }

    #[must_use]
    pub fn current_version(&self) -> &VersionId {
        &self.current_version
    }

    #[must_use]
    pub fn primary_profile_id(&self) -> &OpaqueId {
        &self.primary_profile_id
    }

    #[must_use]
    pub fn candidate_profile_id(&self) -> &OpaqueId {
        &self.candidate_profile_id
    }

    /// Uncalibrated heuristic ranking signal in `[0.0, 1.0]`. Never establishes identity.
    #[must_use]
    pub const fn uncalibrated_heuristic_score(&self) -> f64 {
        self.uncalibrated_heuristic_score
    }

    #[must_use]
    pub fn matching_signals(&self) -> &[AccountMatchSignal] {
        &self.matching_signals
    }

    #[must_use]
    pub fn contradicting_signals(&self) -> &[String] {
        &self.contradicting_signals
    }

    #[must_use]
    pub fn ambiguity_explanation(&self) -> &str {
        &self.ambiguity_explanation
    }

    #[must_use]
    pub const fn uncertainty_notice(&self) -> &'static str {
        self.uncertainty_notice
    }

    #[must_use]
    pub const fn assessment_state(&self) -> MatchAssessmentState {
        self.assessment_state
    }

    #[must_use]
    pub fn proposed_at(&self) -> &AuditTimestamp {
        &self.proposed_at
    }

    #[must_use]
    pub fn review_history(&self) -> &[AccountMatchReviewEntry] {
        &self.review_history
    }

    /// Returns `true` if a subsequent `revise_declared_scope` narrowed the scope so the
    /// primary profile is no longer covered by `current_scope`.
    #[must_use]
    pub const fn outside_current_scope(&self) -> bool {
        self.outside_current_scope
    }

    #[must_use]
    pub const fn needs_reassessment(&self) -> bool {
        self.needs_reassessment
    }

    #[must_use]
    pub fn reassessment_reason(&self) -> Option<&str> {
        self.reassessment_reason.as_deref()
    }
}

/// DNS record type observed for an owned domain (`PRIVACY-02`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsRecordType {
    A,
    Aaaa,
    Cname,
    Mx,
    Txt,
    Ns,
    Ptr,
    Srv,
    Caa,
}

/// Kind of exposed contact detail observed in public records (`PRIVACY-02`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExposedContactKind {
    EmailAddress,
    TelephoneNumber,
    PostalAddress,
    RegistrantName,
    AdministrativeContact,
    MessagingHandle,
}

/// Specific infrastructure or contact exposure payload for a declared domain (`PRIVACY-02`, `A18`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InfrastructureArtifactKind {
    /// Historical or current DNS record observation.
    DnsRecord {
        record_type: DnsRecordType,
        record_value: String,
        ttl_seconds: Option<u32>,
    },
    /// TLS certificate or Certificate Transparency (CT) log entry observation.
    CertificateHistory {
        certificate_sha256_fingerprint: String,
        serial_number: String,
        issuer_common_name: String,
        subject_common_name: String,
        subject_alt_names: Vec<String>,
        not_before_unix_ms: i64,
        not_after_unix_ms: i64,
    },
    /// Publicly exposed contact detail linked to the domain or its public records.
    ExposedContact {
        contact_kind: ExposedContactKind,
        exposed_value: String,
        context_location: String,
        remediation_note: String,
    },
}

/// Input for recording an owned-domain DNS, certificate, or contact history observation (`PRIVACY-02`, `A18`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfrastructureEntryInput {
    pub entry_id: OpaqueId,
    pub version_id: VersionId,
    pub origin: AccountOrigin,
    pub domain: String,
    pub artifact: InfrastructureArtifactKind,
    pub citation: SourceCitation,
    pub timestamps: ObservationTimeEnvelope,
}

/// Stored owned-domain DNS, certificate, or contact history entry (`PRIVACY-02`, `A18`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfrastructureHistoryEntry {
    entry_id: OpaqueId,
    version_id: VersionId,
    origin: AccountOrigin,
    domain: String,
    artifact: InfrastructureArtifactKind,
    citation: SourceCitation,
    timestamps: ObservationTimeEnvelope,
    outside_current_scope: bool,
    needs_reassessment: bool,
    reassessment_reason: Option<String>,
}

impl InfrastructureHistoryEntry {
    #[must_use]
    pub fn entry_id(&self) -> &OpaqueId {
        &self.entry_id
    }

    #[must_use]
    pub fn version_id(&self) -> &VersionId {
        &self.version_id
    }

    #[must_use]
    pub const fn origin(&self) -> AccountOrigin {
        self.origin
    }

    #[must_use]
    pub fn domain(&self) -> &str {
        &self.domain
    }

    #[must_use]
    pub fn artifact(&self) -> &InfrastructureArtifactKind {
        &self.artifact
    }

    #[must_use]
    pub fn citation(&self) -> &SourceCitation {
        &self.citation
    }

    #[must_use]
    pub fn timestamps(&self) -> &ObservationTimeEnvelope {
        &self.timestamps
    }

    /// Returns `true` if a subsequent `revise_declared_scope` narrowed the scope so this
    /// infrastructure entry's domain (or certificate SAN/CN) is no longer covered by `current_scope`.
    #[must_use]
    pub const fn outside_current_scope(&self) -> bool {
        self.outside_current_scope
    }

    #[must_use]
    pub const fn needs_reassessment(&self) -> bool {
        self.needs_reassessment
    }

    #[must_use]
    pub fn reassessment_reason(&self) -> Option<&str> {
        self.reassessment_reason.as_deref()
    }
}

/// Explicit source coverage state so missing or truncated queries are never mistaken for absence of exposure (`AGENTS.md` §2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceQueryCoverage {
    /// The queried source returned a complete response for the target within its documented scope.
    Complete,
    /// The query stopped at a configured pagination or byte budget (`partial`).
    PartialBudgetReached {
        returned_items: usize,
        omission_note: String,
    },
    /// The source was unreachable, rate-limited, or restricted (`unavailable`).
    SourceUnavailable { reason: String },
    /// The source responded cleanly and reported zero matching records (`empty` in that source only).
    ObservedAbsenceInQueriedSource { scope_limitation_note: String },
}

/// Recorded source-query coverage entry for an audit target (`AGENTS.md` §2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceCoverageRecord {
    pub coverage_id: OpaqueId,
    pub source_id: OpaqueId,
    pub source_version: VersionId,
    pub queried_target: OutboundQueryTarget,
    pub queried_at: AuditTimestamp,
    pub coverage: SourceQueryCoverage,
}

/// Target for a read-only public source lookup (`PRIVACY-02`, `A18`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutboundQueryTarget {
    DeclaredDomain(String),
    DeclaredHandle {
        service_name: String,
        handle: String,
    },
}

/// Proposed outbound source lookup request examined by [`PersonalExposureAudit::prepare_read_only_source_query`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundSourceQueryCandidate {
    pub source_id: OpaqueId,
    pub target: OutboundQueryTarget,
    pub requested_at_unix_ms: i64,
    /// Must be `false`: connecting a source must not publish private local analysis or notes.
    pub attaches_private_notes_or_hypotheses: bool,
    /// Must be `false`: a single lookup must not broadcast the user's full multi-target scope manifest.
    pub exports_full_scope_manifest: bool,
    /// Must be `false`: source queries are strictly read-only.
    pub mutates_remote_account_or_dns: bool,
}

/// Sanitized, read-only query descriptor permitted to be sent to a public source adapter (`PRIVACY-02`, `A18`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermittedSourceQuery {
    source_id: OpaqueId,
    normalized_service_name: Option<String>,
    normalized_target: String,
    is_domain_query: bool,
    privacy_notice: &'static str,
}

impl PermittedSourceQuery {
    #[must_use]
    pub fn source_id(&self) -> &OpaqueId {
        &self.source_id
    }

    #[must_use]
    pub fn normalized_service_name(&self) -> Option<&str> {
        self.normalized_service_name.as_deref()
    }

    #[must_use]
    pub fn normalized_target(&self) -> &str {
        &self.normalized_target
    }

    #[must_use]
    pub const fn is_domain_query(&self) -> bool {
        self.is_domain_query
    }

    #[must_use]
    pub const fn privacy_notice(&self) -> &'static str {
        self.privacy_notice
    }
}

/// Summary counts for a [`PersonalExposureAudit`] snapshot (`PRIVACY-02`, `A18`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExposureAuditSummary {
    pub declared_domain_count: usize,
    pub declared_handle_count: usize,
    pub scope_revision_count: usize,
    pub profile_count: usize,
    pub proposed_match_count: usize,
    pub ambiguous_match_count: usize,
    pub user_confirmed_match_count: usize,
    pub user_rejected_match_count: usize,
    pub dns_entry_count: usize,
    pub certificate_entry_count: usize,
    pub exposed_contact_entry_count: usize,
    pub items_outside_current_scope: usize,
    pub items_needing_reassessment: usize,
}

/// User-scoped personal public exposure and owned-infrastructure audit session (`PRIVACY-02`, `A18`, `#47`).
#[derive(Debug, Clone, PartialEq)]
pub struct PersonalExposureAudit {
    audit_id: OpaqueId,
    schema_version: SchemaVersion,
    current_scope: DeclaredAuditScope,
    prior_scopes: Vec<DeclaredAuditScope>,
    limits: ExposureAuditLimits,
    used_versions: BTreeSet<VersionId>,
    profiles: BTreeMap<OpaqueId, PublicProfileRecord>,
    account_matches: BTreeMap<OpaqueId, AccountMatchCandidate>,
    infrastructure_entries: BTreeMap<OpaqueId, InfrastructureHistoryEntry>,
    coverage_records: Vec<SourceCoverageRecord>,
}

impl PersonalExposureAudit {
    /// Creates a new [`PersonalExposureAudit`] anchored to an explicit [`DeclaredAuditScope`],
    /// revalidating `scope` against `limits`.
    pub fn new(
        audit_id: OpaqueId,
        schema_version: SchemaVersion,
        scope: DeclaredAuditScope,
        limits: ExposureAuditLimits,
    ) -> Result<Self, ExposureAuditError> {
        let revalidated_scope = DeclaredAuditScope::build(
            scope.scope_id,
            scope.scope_version,
            scope.predecessor_scope_version,
            scope.declared_at_unix_ms,
            scope.ownership_basis,
            scope.declared_root_domains,
            scope.declared_handles,
            scope.revision_rationale,
            limits,
        )?;
        let mut used_versions = BTreeSet::new();
        used_versions.insert(revalidated_scope.scope_version().clone());
        Ok(Self {
            audit_id,
            schema_version,
            current_scope: revalidated_scope,
            prior_scopes: Vec::new(),
            limits,
            used_versions,
            profiles: BTreeMap::new(),
            account_matches: BTreeMap::new(),
            infrastructure_entries: BTreeMap::new(),
            coverage_records: Vec::new(),
        })
    }

    #[must_use]
    pub fn audit_id(&self) -> &OpaqueId {
        &self.audit_id
    }

    #[must_use]
    pub const fn schema_version(&self) -> SchemaVersion {
        self.schema_version
    }

    #[must_use]
    pub fn scope(&self) -> &DeclaredAuditScope {
        &self.current_scope
    }

    /// Returns prior [`DeclaredAuditScope`] revisions in chronological order.
    #[must_use]
    pub fn prior_scopes(&self) -> &[DeclaredAuditScope] {
        &self.prior_scopes
    }

    #[must_use]
    pub const fn limits(&self) -> ExposureAuditLimits {
        self.limits
    }

    #[must_use]
    pub const fn privacy_notice(&self) -> &'static str {
        LOCAL_ASSESSMENT_PRIVACY_NOTICE
    }

    /// Returns `true` if `profile_id` refers to a registered profile that was claimed in scope
    /// and whose `(service_name, handle)` is *currently* covered by `self.current_scope`.
    #[must_use]
    pub fn is_profile_currently_in_scope(&self, profile_id: &OpaqueId) -> bool {
        self.profiles.get(profile_id).is_some_and(|profile| {
            profile.claimed_in_declared_scope
                && !profile.outside_current_scope
                && self
                    .current_scope
                    .covers_handle(&profile.service_name, &profile.handle)
        })
    }

    /// Evaluates whether accessing local exposure-audit state on `surface` is permitted under
    /// [`oia_domain::collaboration::evaluate_proposed_access_decision`] (`#38`, `D10` proposed default).
    ///
    /// Local exposure-audit state is treated as an unpublished draft with sensitive artifacts
    /// (`unpublished_draft: true, sensitive_artifact: true`), so `PublicPublication` is always
    /// denied and only cleared roles permitted on `surface` are allowed.
    pub fn evaluate_surface_access(
        &self,
        role: CaseRole,
        clearance: ClearanceGrant,
        surface: AccessSurface,
    ) -> Result<AccessDecision, ExposureAuditError> {
        let sensitivity = SensitivityFlags {
            unpublished_draft: true,
            sensitive_artifact: true,
            sensitive_location: false,
        };
        let decision = evaluate_proposed_access_decision(role, clearance, surface, sensitivity);
        match decision {
            AccessDecision::AllowFull | AccessDecision::AllowWithStrippedLocation => Ok(decision),
            AccessDecision::Deny(reason) => Err(ExposureAuditError::AccessDenied(reason)),
        }
    }

    /// Explicitly revises (widens or narrows) the [`DeclaredAuditScope`] with an ownership/authorization
    /// basis and a required non-empty rationale, preserving prior scope versions (`PRIVACY-02`).
    ///
    /// - The revising actor (`input.ownership_basis.actor()`) must match `current_scope.ownership_basis.actor()`.
    /// - If `current_scope` is an `AuthorizedAssessmentScope`, `revised_at_unix_ms` must fall within the
    ///   current authorization window (`AuthorizationExpiredOrNotYetValid`) before any revision or basis
    ///   transition is accepted.
    /// - When a revision narrows `declared_root_domains` or `declared_handles`, any existing profiles,
    ///   candidate account matches, or infrastructure entries that are no longer covered by `current_scope`
    ///   are retained with `outside_current_scope = true` and `needs_reassessment = true`.
    pub fn revise_declared_scope(
        &mut self,
        input: ScopeRevisionInput,
    ) -> Result<&DeclaredAuditScope, ExposureAuditError> {
        if self.prior_scopes.len() >= self.limits.max_scope_revisions {
            return Err(ExposureAuditError::LimitExceeded {
                limit: AuditLimitKind::ScopeRevisions,
                actual: self.prior_scopes.len().saturating_add(1),
                max: self.limits.max_scope_revisions,
            });
        }
        if self.current_scope.scope_version != input.expected_scope_version {
            return Err(ExposureAuditError::StaleExpectedVersion {
                expected: input.expected_scope_version,
                actual: self.current_scope.scope_version.clone(),
            });
        }
        if self.used_versions.contains(&input.new_scope_version) {
            return Err(ExposureAuditError::DuplicateVersion(
                input.new_scope_version,
            ));
        }
        if input.revised_at_unix_ms < self.current_scope.declared_at_unix_ms {
            return Err(ExposureAuditError::InvalidTimestampOrder(
                "revised_at_unix_ms must not precede current scope declared_at_unix_ms",
            ));
        }

        // Enforce that the revision happens within the active scope's authorization window
        // and by the same actor.
        self.current_scope
            .ownership_basis
            .check_timestamp_within_authorization(input.revised_at_unix_ms)?;
        if input.ownership_basis.actor() != self.current_scope.ownership_basis.actor() {
            return Err(ExposureAuditError::UnauthorizedActor {
                expected: self.current_scope.ownership_basis.actor().clone(),
                actual: input.ownership_basis.actor().clone(),
            });
        }

        let new_scope = DeclaredAuditScope::build(
            self.current_scope.scope_id.clone(),
            input.new_scope_version.clone(),
            Some(self.current_scope.scope_version.clone()),
            input.revised_at_unix_ms,
            input.ownership_basis,
            input.declared_root_domains,
            input.declared_handles,
            Some(input.revision_rationale),
            self.limits,
        )?;

        self.prior_scopes
            .try_reserve(1)
            .map_err(|_| ExposureAuditError::AllocationFailed)?;
        self.used_versions.insert(input.new_scope_version.clone());
        let old_scope = std::mem::replace(&mut self.current_scope, new_scope);
        self.prior_scopes.push(old_scope);

        self.reconcile_records_against_current_scope(&input.new_scope_version);
        Ok(&self.current_scope)
    }

    fn reconcile_records_against_current_scope(&mut self, scope_version: &VersionId) {
        let scope_note = format!(
            "record falls outside narrowed declared audit scope `{scope_version}`; retained for audit history"
        );

        let mut in_scope_primary_ids = BTreeSet::new();
        for profile in self.profiles.values_mut() {
            if profile.claimed_in_declared_scope {
                let covered = self
                    .current_scope
                    .covers_handle(&profile.service_name, &profile.handle);
                if covered {
                    profile.outside_current_scope = false;
                    in_scope_primary_ids.insert(profile.profile_id.clone());
                } else {
                    profile.outside_current_scope = true;
                    profile.needs_reassessment = true;
                    profile.reassessment_reason = Some(scope_note.clone());
                }
            }
        }

        for profile in self.profiles.values_mut() {
            if !profile.claimed_in_declared_scope {
                let anchor_covered = profile
                    .anchor_primary_profile_id
                    .as_ref()
                    .is_some_and(|anchor_id| in_scope_primary_ids.contains(anchor_id));
                if anchor_covered {
                    profile.outside_current_scope = false;
                } else {
                    profile.outside_current_scope = true;
                    profile.needs_reassessment = true;
                    profile.reassessment_reason = Some(scope_note.clone());
                }
            }
        }

        for candidate in self.account_matches.values_mut() {
            if in_scope_primary_ids.contains(&candidate.primary_profile_id) {
                candidate.outside_current_scope = false;
            } else {
                candidate.outside_current_scope = true;
                candidate.needs_reassessment = true;
                candidate.reassessment_reason = Some(scope_note.clone());
            }
        }

        for entry in self.infrastructure_entries.values_mut() {
            let domain_covered = self.current_scope.covers_domain(&entry.domain);
            let cert_covered = match &entry.artifact {
                InfrastructureArtifactKind::CertificateHistory {
                    subject_common_name,
                    subject_alt_names,
                    ..
                } => {
                    self.current_scope.covers_domain(subject_common_name)
                        && subject_alt_names
                            .iter()
                            .all(|san| self.current_scope.covers_domain(san))
                }
                InfrastructureArtifactKind::DnsRecord { .. }
                | InfrastructureArtifactKind::ExposedContact { .. } => true,
            };
            if domain_covered && cert_covered {
                entry.outside_current_scope = false;
            } else {
                entry.outside_current_scope = true;
                entry.needs_reassessment = true;
                entry.reassessment_reason = Some(scope_note.clone());
            }
        }
    }

    /// Registers a public profile observation (`PRIVACY-02`, `A18`).
    ///
    /// - If `claimed_in_declared_scope` is `true`, the profile's `(service_name, handle)` must
    ///   match a [`DeclaredAccountHandle`] in `current_scope`. A profile's `listed_website_domain`
    ///   alone never grants in-scope claimed status (`HandleOutsideDeclaredScope`).
    /// - If `claimed_in_declared_scope` is `false`, `anchor_primary_profile_id` must reference an
    ///   existing profile that is currently claimed and in scope (`UnclaimedProfileRequiresInScopePrimaryAnchor`
    ///   or `PrimaryProfileMustBeInDeclaredScope`), preventing storage of unanchored third-party profiles.
    pub fn register_public_profile(
        &mut self,
        input: PublicProfileInput,
    ) -> Result<&PublicProfileRecord, ExposureAuditError> {
        if self.profiles.len() >= self.limits.max_profiles {
            return Err(ExposureAuditError::LimitExceeded {
                limit: AuditLimitKind::Profiles,
                actual: self.profiles.len().saturating_add(1),
                max: self.limits.max_profiles,
            });
        }
        if self.profiles.contains_key(&input.profile_id) {
            return Err(ExposureAuditError::DuplicateIdentifier {
                kind: "profile_id",
                id: input.profile_id,
            });
        }
        if self.used_versions.contains(&input.version_id) {
            return Err(ExposureAuditError::DuplicateVersion(input.version_id));
        }

        self.current_scope
            .ownership_basis
            .check_timestamp_within_authorization(input.timestamps.audited_at.unix_ms)?;

        let normalized_service = normalize_service_name(&input.service_name, self.limits)?;
        let normalized_handle = normalize_handle(&input.handle, self.limits)?;
        input.citation.validate(self.limits)?;
        if let Some(display_name) = &input.display_name {
            validate_text("display_name", display_name, self.limits)?;
        }
        if let Some(bio) = &input.bio_excerpt {
            validate_text("bio_excerpt", bio, self.limits)?;
        }

        let normalized_domain = if let Some(domain) = &input.listed_website_domain {
            Some(normalize_and_validate_domain(domain, self.limits)?)
        } else {
            None
        };

        for contact in &input.exposed_contact_values {
            validate_text("exposed_contact_value", contact, self.limits)?;
        }
        input.timestamps.validate(self.limits)?;

        if input.claimed_in_declared_scope {
            if !self
                .current_scope
                .covers_handle(&normalized_service, &normalized_handle)
            {
                return Err(ExposureAuditError::HandleOutsideDeclaredScope);
            }
        } else {
            let anchor_id = input
                .anchor_primary_profile_id
                .as_ref()
                .ok_or(ExposureAuditError::UnclaimedProfileRequiresInScopePrimaryAnchor)?;
            if !self.profiles.contains_key(anchor_id) {
                return Err(ExposureAuditError::UnknownProfileId(anchor_id.clone()));
            }
            if !self.is_profile_currently_in_scope(anchor_id) {
                return Err(ExposureAuditError::PrimaryProfileMustBeInDeclaredScope {
                    profile_id: anchor_id.clone(),
                });
            }
        }

        self.used_versions.insert(input.version_id.clone());
        let profile_id = input.profile_id.clone();
        let record = PublicProfileRecord {
            profile_id: input.profile_id,
            version_id: input.version_id,
            origin: input.origin,
            service_name: normalized_service,
            handle: normalized_handle,
            display_name: input.display_name,
            bio_excerpt: input.bio_excerpt,
            listed_website_domain: normalized_domain,
            exposed_contact_values: input.exposed_contact_values,
            claimed_in_declared_scope: input.claimed_in_declared_scope,
            anchor_primary_profile_id: if input.claimed_in_declared_scope {
                None
            } else {
                input.anchor_primary_profile_id
            },
            outside_current_scope: false,
            citation: input.citation,
            timestamps: input.timestamps,
            needs_reassessment: false,
            reassessment_reason: None,
        };
        let entry = self.profiles.entry(profile_id).or_insert(record);
        Ok(entry)
    }

    /// Proposes a candidate connection between an in-scope profile (`primary_profile_id`) and another
    /// public profile (`candidate_profile_id`) (`PRIVACY-02`, `A18`).
    ///
    /// `primary_profile_id` must be both claimed and *currently* covered by `self.current_scope`
    /// (`is_profile_currently_in_scope`), preventing transitive expansion from an external candidate
    /// profile or a profile removed by a scope-narrowing revision.
    /// Regardless of `uncalibrated_heuristic_score`, the candidate starts in
    /// [`MatchAssessmentState::ProposedCandidate`] and never auto-promotes to established identity.
    pub fn propose_account_match(
        &mut self,
        input: AccountMatchInput,
    ) -> Result<&AccountMatchCandidate, ExposureAuditError> {
        if self.account_matches.len() >= self.limits.max_account_matches {
            return Err(ExposureAuditError::LimitExceeded {
                limit: AuditLimitKind::AccountMatches,
                actual: self.account_matches.len().saturating_add(1),
                max: self.limits.max_account_matches,
            });
        }
        if self.account_matches.contains_key(&input.match_id) {
            return Err(ExposureAuditError::DuplicateIdentifier {
                kind: "match_id",
                id: input.match_id,
            });
        }
        if self.used_versions.contains(&input.version_id) {
            return Err(ExposureAuditError::DuplicateVersion(input.version_id));
        }
        if input.primary_profile_id == input.candidate_profile_id {
            return Err(ExposureAuditError::SameProfileAccountMatch);
        }

        self.current_scope
            .ownership_basis
            .check_timestamp_within_authorization(input.proposed_at.unix_ms)?;

        let primary = self
            .profiles
            .get(&input.primary_profile_id)
            .ok_or_else(|| {
                ExposureAuditError::UnknownProfileId(input.primary_profile_id.clone())
            })?;
        let candidate = self
            .profiles
            .get(&input.candidate_profile_id)
            .ok_or_else(|| {
                ExposureAuditError::UnknownProfileId(input.candidate_profile_id.clone())
            })?;

        if !self.is_profile_currently_in_scope(&input.primary_profile_id) {
            return Err(ExposureAuditError::PrimaryProfileMustBeInDeclaredScope {
                profile_id: input.primary_profile_id,
            });
        }

        if !input.uncalibrated_heuristic_score.is_finite()
            || !(0.0..=1.0).contains(&input.uncalibrated_heuristic_score)
        {
            return Err(ExposureAuditError::InvalidHeuristicScore);
        }
        if input.matching_signals.is_empty() {
            return Err(ExposureAuditError::EmptyMatchingSignals);
        }

        for signal in &input.matching_signals {
            validate_text("signal.description", &signal.description, self.limits)?;
            signal.citation.validate(self.limits)?;
        }
        for contra in &input.contradicting_signals {
            validate_text("contradicting_signal", contra, self.limits)?;
        }
        validate_text(
            "ambiguity_explanation",
            &input.ambiguity_explanation,
            self.limits,
        )?;
        input.proposed_at.validate("proposed_at", self.limits)?;

        self.used_versions.insert(input.version_id.clone());
        let match_id = input.match_id.clone();
        let record = AccountMatchCandidate {
            match_id: input.match_id,
            current_version: input.version_id,
            primary_profile_id: input.primary_profile_id,
            candidate_profile_id: input.candidate_profile_id,
            uncalibrated_heuristic_score: input.uncalibrated_heuristic_score,
            matching_signals: input.matching_signals,
            contradicting_signals: input.contradicting_signals,
            ambiguity_explanation: input.ambiguity_explanation,
            uncertainty_notice: ACCOUNT_MATCH_UNCERTAINTY_NOTICE,
            assessment_state: MatchAssessmentState::ProposedCandidate,
            proposed_at: input.proposed_at,
            review_history: Vec::new(),
            outside_current_scope: false,
            needs_reassessment: primary.needs_reassessment || candidate.needs_reassessment,
            reassessment_reason: if primary.needs_reassessment || candidate.needs_reassessment {
                Some("referenced profile already flagged for reassessment".to_owned())
            } else {
                None
            },
        };
        let entry = self.account_matches.entry(match_id).or_insert(record);
        Ok(entry)
    }

    /// Records an explicit, reversible user review decision on a candidate account match (`PRIVACY-02`, `A18`).
    ///
    /// - Re-checks that `input.reviewer` matches `current_scope.ownership_basis.actor()` (`UnauthorizedActor`).
    /// - Re-checks that the candidate's `primary_profile_id` is still covered by `current_scope`
    ///   (`PrimaryProfileMustBeInDeclaredScope`).
    /// - Transitioning to [`MatchAssessmentState::UserRejectedDifferentEntity`] retains the candidate,
    ///   its signals, and its review history in `account_matches()` so later reviewers can inspect it.
    pub fn review_account_match(
        &mut self,
        input: AccountMatchReviewInput,
    ) -> Result<&AccountMatchCandidate, ExposureAuditError> {
        if matches!(input.new_state, MatchAssessmentState::ProposedCandidate) {
            return Err(ExposureAuditError::InvalidDecisionTransition(
                "review decision must transition to AmbiguousRetained, UserConfirmedSameOwner, or UserRejectedDifferentEntity",
            ));
        }
        validate_text(
            "private_rationale_note",
            &input.private_rationale_note,
            self.limits,
        )?;
        input.reviewed_at.validate("reviewed_at", self.limits)?;
        self.current_scope
            .ownership_basis
            .check_timestamp_within_authorization(input.reviewed_at.unix_ms)?;

        if &input.reviewer != self.current_scope.ownership_basis.actor() {
            return Err(ExposureAuditError::UnauthorizedActor {
                expected: self.current_scope.ownership_basis.actor().clone(),
                actual: input.reviewer,
            });
        }

        if self.used_versions.contains(&input.new_version) {
            return Err(ExposureAuditError::DuplicateVersion(input.new_version));
        }

        let primary_profile_id = self
            .account_matches
            .get(&input.match_id)
            .ok_or_else(|| ExposureAuditError::UnknownAccountMatchId(input.match_id.clone()))?
            .primary_profile_id
            .clone();

        if !self.is_profile_currently_in_scope(&primary_profile_id) {
            return Err(ExposureAuditError::PrimaryProfileMustBeInDeclaredScope {
                profile_id: primary_profile_id,
            });
        }

        let match_id = input.match_id;
        let candidate = self
            .account_matches
            .get_mut(&match_id)
            .ok_or_else(|| ExposureAuditError::UnknownAccountMatchId(match_id.clone()))?;

        if candidate.current_version != input.expected_version {
            return Err(ExposureAuditError::StaleExpectedVersion {
                expected: input.expected_version,
                actual: candidate.current_version.clone(),
            });
        }
        if candidate.review_history.len() >= self.limits.max_reviews_per_match {
            return Err(ExposureAuditError::LimitExceeded {
                limit: AuditLimitKind::ReviewsPerMatch,
                actual: candidate.review_history.len().saturating_add(1),
                max: self.limits.max_reviews_per_match,
            });
        }

        candidate
            .review_history
            .try_reserve(1)
            .map_err(|_| ExposureAuditError::AllocationFailed)?;

        let previous_state = candidate.assessment_state;
        candidate.review_history.push(AccountMatchReviewEntry {
            review_version: input.new_version.clone(),
            reviewer: input.reviewer,
            reviewed_at: input.reviewed_at,
            previous_state,
            new_state: input.new_state,
            private_rationale_note: input.private_rationale_note,
        });
        candidate.current_version = input.new_version.clone();
        candidate.assessment_state = input.new_state;
        candidate.needs_reassessment = false;
        candidate.reassessment_reason = None;
        self.used_versions.insert(input.new_version);

        Ok(&*candidate)
    }

    /// Records a dated DNS, certificate-transparency, or exposed-contact entry for a domain in the declared scope (`PRIVACY-02`, `A18`).
    pub fn record_infrastructure_entry(
        &mut self,
        input: InfrastructureEntryInput,
    ) -> Result<&InfrastructureHistoryEntry, ExposureAuditError> {
        if self.infrastructure_entries.len() >= self.limits.max_infrastructure_entries {
            return Err(ExposureAuditError::LimitExceeded {
                limit: AuditLimitKind::InfrastructureEntries,
                actual: self.infrastructure_entries.len().saturating_add(1),
                max: self.limits.max_infrastructure_entries,
            });
        }
        if self.infrastructure_entries.contains_key(&input.entry_id) {
            return Err(ExposureAuditError::DuplicateIdentifier {
                kind: "entry_id",
                id: input.entry_id,
            });
        }
        if self.used_versions.contains(&input.version_id) {
            return Err(ExposureAuditError::DuplicateVersion(input.version_id));
        }

        self.current_scope
            .ownership_basis
            .check_timestamp_within_authorization(input.timestamps.audited_at.unix_ms)?;

        let norm_domain = normalize_and_validate_domain(&input.domain, self.limits)?;
        if !self.current_scope.covers_domain(&norm_domain) {
            return Err(ExposureAuditError::DomainOutsideDeclaredScope {
                candidate_domain: norm_domain,
            });
        }

        input.citation.validate(self.limits)?;
        input.timestamps.validate(self.limits)?;

        let validated_artifact = match input.artifact {
            InfrastructureArtifactKind::DnsRecord {
                record_type,
                record_value,
                ttl_seconds,
            } => {
                validate_text("record_value", &record_value, self.limits)?;
                InfrastructureArtifactKind::DnsRecord {
                    record_type,
                    record_value,
                    ttl_seconds,
                }
            }
            InfrastructureArtifactKind::CertificateHistory {
                certificate_sha256_fingerprint,
                serial_number,
                issuer_common_name,
                subject_common_name,
                subject_alt_names,
                not_before_unix_ms,
                not_after_unix_ms,
            } => {
                let norm_fp = normalize_and_validate_sha256_hex(
                    &certificate_sha256_fingerprint,
                    self.limits,
                )?;
                validate_text("serial_number", &serial_number, self.limits)?;
                validate_text("issuer_common_name", &issuer_common_name, self.limits)?;
                let norm_cn = normalize_and_validate_domain(&subject_common_name, self.limits)?;
                if !self.current_scope.covers_domain(&norm_cn) {
                    return Err(ExposureAuditError::DomainOutsideDeclaredScope {
                        candidate_domain: norm_cn,
                    });
                }
                if not_after_unix_ms < not_before_unix_ms {
                    return Err(ExposureAuditError::InvalidValidityInterval);
                }
                let mut norm_sans = Vec::new();
                norm_sans
                    .try_reserve(subject_alt_names.len())
                    .map_err(|_| ExposureAuditError::AllocationFailed)?;
                for san in subject_alt_names {
                    let norm_san = normalize_and_validate_domain(&san, self.limits)?;
                    if !self.current_scope.covers_domain(&norm_san) {
                        return Err(ExposureAuditError::DomainOutsideDeclaredScope {
                            candidate_domain: norm_san,
                        });
                    }
                    norm_sans.push(norm_san);
                }
                InfrastructureArtifactKind::CertificateHistory {
                    certificate_sha256_fingerprint: norm_fp,
                    serial_number,
                    issuer_common_name,
                    subject_common_name: norm_cn,
                    subject_alt_names: norm_sans,
                    not_before_unix_ms,
                    not_after_unix_ms,
                }
            }
            InfrastructureArtifactKind::ExposedContact {
                contact_kind,
                exposed_value,
                context_location,
                remediation_note,
            } => {
                validate_text("exposed_value", &exposed_value, self.limits)?;
                validate_text("context_location", &context_location, self.limits)?;
                validate_text("remediation_note", &remediation_note, self.limits)?;
                InfrastructureArtifactKind::ExposedContact {
                    contact_kind,
                    exposed_value,
                    context_location,
                    remediation_note,
                }
            }
        };

        self.used_versions.insert(input.version_id.clone());
        let entry_id = input.entry_id.clone();
        let entry = InfrastructureHistoryEntry {
            entry_id: input.entry_id,
            version_id: input.version_id,
            origin: input.origin,
            domain: norm_domain,
            artifact: validated_artifact,
            citation: input.citation,
            timestamps: input.timestamps,
            outside_current_scope: false,
            needs_reassessment: false,
            reassessment_reason: None,
        };
        let stored = self.infrastructure_entries.entry(entry_id).or_insert(entry);
        Ok(stored)
    }

    /// Records the coverage outcome of querying a source for a declared domain or `(service_name, handle)` (`AGENTS.md` §2).
    pub fn record_source_coverage(
        &mut self,
        mut record: SourceCoverageRecord,
    ) -> Result<&SourceCoverageRecord, ExposureAuditError> {
        if self.coverage_records.len() >= self.limits.max_coverage_records {
            return Err(ExposureAuditError::LimitExceeded {
                limit: AuditLimitKind::CoverageRecords,
                actual: self.coverage_records.len().saturating_add(1),
                max: self.limits.max_coverage_records,
            });
        }
        if self
            .coverage_records
            .iter()
            .any(|existing| existing.coverage_id == record.coverage_id)
        {
            return Err(ExposureAuditError::DuplicateIdentifier {
                kind: "coverage_id",
                id: record.coverage_id,
            });
        }

        self.current_scope
            .ownership_basis
            .check_timestamp_within_authorization(record.queried_at.unix_ms)?;

        record.queried_at.validate("queried_at", self.limits)?;
        record.queried_target = match record.queried_target {
            OutboundQueryTarget::DeclaredDomain(domain) => {
                let norm = normalize_and_validate_domain(&domain, self.limits)?;
                if !self.current_scope.covers_domain(&norm) {
                    return Err(ExposureAuditError::DomainOutsideDeclaredScope {
                        candidate_domain: norm,
                    });
                }
                OutboundQueryTarget::DeclaredDomain(norm)
            }
            OutboundQueryTarget::DeclaredHandle {
                service_name,
                handle,
            } => {
                let norm_service = normalize_service_name(&service_name, self.limits)?;
                let norm_handle = normalize_handle(&handle, self.limits)?;
                if !self
                    .current_scope
                    .covers_handle(&norm_service, &norm_handle)
                {
                    return Err(ExposureAuditError::HandleOutsideDeclaredScope);
                }
                OutboundQueryTarget::DeclaredHandle {
                    service_name: norm_service,
                    handle: norm_handle,
                }
            }
        };

        match &record.coverage {
            SourceQueryCoverage::Complete => {}
            SourceQueryCoverage::PartialBudgetReached { omission_note, .. } => {
                validate_text("omission_note", omission_note, self.limits)?;
            }
            SourceQueryCoverage::SourceUnavailable { reason } => {
                validate_text("unavailable_reason", reason, self.limits)?;
            }
            SourceQueryCoverage::ObservedAbsenceInQueriedSource {
                scope_limitation_note,
            } => {
                validate_text("scope_limitation_note", scope_limitation_note, self.limits)?;
            }
        }

        self.coverage_records
            .try_reserve(1)
            .map_err(|_| ExposureAuditError::AllocationFailed)?;
        let idx = self.coverage_records.len();
        self.coverage_records.push(record);
        Ok(&self.coverage_records[idx])
    }

    /// Validates that an outbound source lookup is strictly read-only, within the active
    /// authorization window, scoped to a single declared target in `current_scope` (refusing
    /// expansion into out-of-scope candidate handles or domains), and does not leak private local
    /// analysis, scope manifests, or unreviewed match hypotheses (`PRIVACY-02`, `A18`).
    pub fn prepare_read_only_source_query(
        &self,
        candidate: OutboundSourceQueryCandidate,
    ) -> Result<PermittedSourceQuery, ExposureAuditError> {
        self.current_scope
            .ownership_basis
            .check_timestamp_within_authorization(candidate.requested_at_unix_ms)?;

        if candidate.attaches_private_notes_or_hypotheses {
            return Err(ExposureAuditError::PrivateAssessmentLeakBlocked(
                "private notes and unreviewed account-match hypotheses must never be sent to external sources",
            ));
        }
        if candidate.exports_full_scope_manifest {
            return Err(ExposureAuditError::PrivateAssessmentLeakBlocked(
                "full declared audit scope manifest must not be broadcast to external sources",
            ));
        }
        if candidate.mutates_remote_account_or_dns {
            return Err(ExposureAuditError::PrivateAssessmentLeakBlocked(
                "personal exposure audit source queries are strictly read-only",
            ));
        }

        match candidate.target {
            OutboundQueryTarget::DeclaredDomain(domain) => {
                let norm = normalize_and_validate_domain(&domain, self.limits)?;
                if !self.current_scope.covers_domain(&norm) {
                    return Err(ExposureAuditError::DomainOutsideDeclaredScope {
                        candidate_domain: norm,
                    });
                }
                Ok(PermittedSourceQuery {
                    source_id: candidate.source_id,
                    normalized_service_name: None,
                    normalized_target: norm,
                    is_domain_query: true,
                    privacy_notice: LOCAL_ASSESSMENT_PRIVACY_NOTICE,
                })
            }
            OutboundQueryTarget::DeclaredHandle {
                service_name,
                handle,
            } => {
                let norm_service = normalize_service_name(&service_name, self.limits)?;
                let norm_handle = normalize_handle(&handle, self.limits)?;
                if !self
                    .current_scope
                    .covers_handle(&norm_service, &norm_handle)
                {
                    return Err(ExposureAuditError::HandleOutsideDeclaredScope);
                }
                Ok(PermittedSourceQuery {
                    source_id: candidate.source_id,
                    normalized_service_name: Some(norm_service),
                    normalized_target: norm_handle,
                    is_domain_query: false,
                    privacy_notice: LOCAL_ASSESSMENT_PRIVACY_NOTICE,
                })
            }
        }
    }

    /// Flags profiles, candidate account matches, and infrastructure history entries that cite
    /// `(source_id, superseded_source_version)` as needing reassessment (`AGENTS.md` §2, `docs/architecture.md` §2.3).
    ///
    /// Returns the number of records flagged.
    pub fn flag_source_version_updated(
        &mut self,
        source_id: &OpaqueId,
        superseded_source_version: &VersionId,
        replacement_source_version: &VersionId,
        reason: &str,
    ) -> Result<usize, ExposureAuditError> {
        validate_text("reassessment_reason", reason, self.limits)?;
        if superseded_source_version == replacement_source_version {
            return Err(ExposureAuditError::DuplicateVersion(
                replacement_source_version.clone(),
            ));
        }

        let note = format!(
            "source `{source_id}` version `{superseded_source_version}` superseded by `{replacement_source_version}`: {reason}"
        );
        let mut flagged_profile_ids = BTreeSet::new();
        let mut count = 0usize;

        for profile in self.profiles.values_mut() {
            if &profile.citation.source_id == source_id
                && &profile.citation.source_version == superseded_source_version
            {
                profile.needs_reassessment = true;
                profile.reassessment_reason = Some(note.clone());
                flagged_profile_ids.insert(profile.profile_id.clone());
                count = count.saturating_add(1);
            }
        }

        for candidate in self.account_matches.values_mut() {
            let cites_source = candidate.matching_signals.iter().any(|sig| {
                &sig.citation.source_id == source_id
                    && &sig.citation.source_version == superseded_source_version
            });
            let references_flagged_profile = flagged_profile_ids
                .contains(&candidate.primary_profile_id)
                || flagged_profile_ids.contains(&candidate.candidate_profile_id);
            if cites_source || references_flagged_profile {
                candidate.needs_reassessment = true;
                candidate.reassessment_reason = Some(note.clone());
                count = count.saturating_add(1);
            }
        }

        for entry in self.infrastructure_entries.values_mut() {
            if &entry.citation.source_id == source_id
                && &entry.citation.source_version == superseded_source_version
            {
                entry.needs_reassessment = true;
                entry.reassessment_reason = Some(note.clone());
                count = count.saturating_add(1);
            }
        }

        Ok(count)
    }

    #[must_use]
    pub fn profiles(&self) -> &BTreeMap<OpaqueId, PublicProfileRecord> {
        &self.profiles
    }

    #[must_use]
    pub fn account_matches(&self) -> &BTreeMap<OpaqueId, AccountMatchCandidate> {
        &self.account_matches
    }

    #[must_use]
    pub fn infrastructure_entries(&self) -> &BTreeMap<OpaqueId, InfrastructureHistoryEntry> {
        &self.infrastructure_entries
    }

    #[must_use]
    pub fn coverage_records(&self) -> &[SourceCoverageRecord] {
        &self.coverage_records
    }

    /// Computes summary counts across the current audit state.
    #[must_use]
    pub fn summary(&self) -> ExposureAuditSummary {
        let mut proposed_match_count = 0usize;
        let mut ambiguous_match_count = 0usize;
        let mut user_confirmed_match_count = 0usize;
        let mut user_rejected_match_count = 0usize;
        let mut items_outside_current_scope = 0usize;
        let mut items_needing_reassessment = 0usize;

        for profile in self.profiles.values() {
            if profile.outside_current_scope {
                items_outside_current_scope = items_outside_current_scope.saturating_add(1);
            }
            if profile.needs_reassessment {
                items_needing_reassessment = items_needing_reassessment.saturating_add(1);
            }
        }

        for m in self.account_matches.values() {
            match m.assessment_state {
                MatchAssessmentState::ProposedCandidate => {
                    proposed_match_count = proposed_match_count.saturating_add(1);
                }
                MatchAssessmentState::AmbiguousRetained => {
                    ambiguous_match_count = ambiguous_match_count.saturating_add(1);
                }
                MatchAssessmentState::UserConfirmedSameOwner => {
                    user_confirmed_match_count = user_confirmed_match_count.saturating_add(1);
                }
                MatchAssessmentState::UserRejectedDifferentEntity => {
                    user_rejected_match_count = user_rejected_match_count.saturating_add(1);
                }
            }
            if m.outside_current_scope {
                items_outside_current_scope = items_outside_current_scope.saturating_add(1);
            }
            if m.needs_reassessment {
                items_needing_reassessment = items_needing_reassessment.saturating_add(1);
            }
        }

        let mut dns_entry_count = 0usize;
        let mut certificate_entry_count = 0usize;
        let mut exposed_contact_entry_count = 0usize;
        for entry in self.infrastructure_entries.values() {
            match entry.artifact {
                InfrastructureArtifactKind::DnsRecord { .. } => {
                    dns_entry_count = dns_entry_count.saturating_add(1);
                }
                InfrastructureArtifactKind::CertificateHistory { .. } => {
                    certificate_entry_count = certificate_entry_count.saturating_add(1);
                }
                InfrastructureArtifactKind::ExposedContact { .. } => {
                    exposed_contact_entry_count = exposed_contact_entry_count.saturating_add(1);
                }
            }
            if entry.outside_current_scope {
                items_outside_current_scope = items_outside_current_scope.saturating_add(1);
            }
            if entry.needs_reassessment {
                items_needing_reassessment = items_needing_reassessment.saturating_add(1);
            }
        }

        ExposureAuditSummary {
            declared_domain_count: self.current_scope.declared_root_domains.len(),
            declared_handle_count: self.current_scope.declared_handles.len(),
            scope_revision_count: self.prior_scopes.len(),
            profile_count: self.profiles.len(),
            proposed_match_count,
            ambiguous_match_count,
            user_confirmed_match_count,
            user_rejected_match_count,
            dns_entry_count,
            certificate_entry_count,
            exposed_contact_entry_count,
            items_outside_current_scope,
            items_needing_reassessment,
        }
    }
}

fn validate_text(
    field: &'static str,
    value: &str,
    limits: ExposureAuditLimits,
) -> Result<(), ExposureAuditError> {
    if value.len() > limits.max_text_bytes {
        return Err(ExposureAuditError::FieldTooLong {
            field,
            actual: value.len(),
            max: limits.max_text_bytes,
        });
    }
    if value.trim().is_empty() {
        return Err(ExposureAuditError::EmptyField(field));
    }
    if value
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\r' | '\n' | '\t'))
    {
        return Err(ExposureAuditError::ControlCharacter(field));
    }
    Ok(())
}

fn normalize_service_name(
    raw: &str,
    limits: ExposureAuditLimits,
) -> Result<String, ExposureAuditError> {
    validate_text("service_name", raw, limits)?;
    Ok(raw.trim().to_ascii_lowercase())
}

fn normalize_handle(raw: &str, limits: ExposureAuditLimits) -> Result<String, ExposureAuditError> {
    validate_text("handle", raw, limits)?;
    let trimmed = raw.trim().to_ascii_lowercase();
    if !trimmed.is_ascii()
        || trimmed
            .chars()
            .any(|c| c.is_ascii_whitespace() || c.is_ascii_control())
    {
        return Err(ExposureAuditError::ControlCharacter("handle"));
    }
    Ok(trimmed)
}

fn normalize_and_validate_root_domain(
    raw: &str,
    limits: ExposureAuditLimits,
) -> Result<String, ExposureAuditError> {
    let norm = normalize_and_validate_domain(raw, limits)?;
    if DISALLOWED_PUBLIC_SUFFIX_ROOTS.contains(&norm.as_str()) {
        return Err(ExposureAuditError::PublicSuffixRootDisallowed { domain: norm });
    }
    Ok(norm)
}

fn normalize_and_validate_domain(
    raw: &str,
    limits: ExposureAuditLimits,
) -> Result<String, ExposureAuditError> {
    validate_text("domain", raw, limits)?;
    normalize_domain_str(raw)
}

fn normalize_domain_str(raw: &str) -> Result<String, ExposureAuditError> {
    let trimmed = raw.trim().trim_end_matches('.').to_ascii_lowercase();
    if trimmed.is_empty() || trimmed.len() > 253 || !trimmed.contains('.') {
        return Err(ExposureAuditError::InvalidDomainSyntax);
    }
    let mut last_label = "";
    for label in trimmed.split('.') {
        if label.is_empty()
            || label.len() > 63
            || label.starts_with('-')
            || label.ends_with('-')
            || !label
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        {
            return Err(ExposureAuditError::InvalidDomainSyntax);
        }
        last_label = label;
    }
    // Reject IPv4 literals (e.g. `192.0.2.1`) and all-numeric TLDs per RFC 1123 / RFC 3696.
    if last_label.chars().all(|ch| ch.is_ascii_digit()) {
        return Err(ExposureAuditError::InvalidDomainSyntax);
    }
    Ok(trimmed)
}

fn normalize_and_validate_sha256_hex(
    raw: &str,
    limits: ExposureAuditLimits,
) -> Result<String, ExposureAuditError> {
    validate_text("certificate_sha256_fingerprint", raw, limits)?;
    let trimmed = raw.trim().to_ascii_lowercase();
    if trimmed.len() != 64 || !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ExposureAuditError::InvalidCertificateFingerprint);
    }
    Ok(trimmed)
}

fn validate_iso_calendar_date(raw: &str) -> Result<(), ExposureAuditError> {
    let bytes = raw.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return Err(ExposureAuditError::InvalidCalendarDate);
    }
    let year: u16 = raw[0..4]
        .parse()
        .map_err(|_| ExposureAuditError::InvalidCalendarDate)?;
    let month: u8 = raw[5..7]
        .parse()
        .map_err(|_| ExposureAuditError::InvalidCalendarDate)?;
    let day: u8 = raw[8..10]
        .parse()
        .map_err(|_| ExposureAuditError::InvalidCalendarDate)?;
    if year == 0 || !(1..=12).contains(&month) || day == 0 {
        return Err(ExposureAuditError::InvalidCalendarDate);
    }
    let is_leap = (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400);
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap => 29,
        2 => 28,
        _ => return Err(ExposureAuditError::InvalidCalendarDate),
    };
    if day > max_day {
        return Err(ExposureAuditError::InvalidCalendarDate);
    }
    Ok(())
}
