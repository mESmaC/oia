//! Common domain envelope, evidence classes, claim/review states, and principal objects.
//!
//! Enforces OIA's central contract (`AGENTS.md` Section 2):
//! - Original retained bytes are never overwritten by extraction or analysis.
//! - Observations, source assertions, assumptions, inferences, forecasts, and generated
//!   material remain distinct (`OriginClass`). Feeding an inference or predictive
//!   derivative into another step cannot promote it to an observation.
//! - Missing numeric measurements remain `RecordedValue::Missing` and are never
//!   converted to `0.0` or plausible defaults.
//! - Claim assessment, human review status, and source availability remain orthogonal.

use std::collections::BTreeMap;
use std::fmt;

use crate::ids::{ActorId, IdError, OpaqueId, SchemaVersion, VersionId};
use crate::json::JsonError;
use crate::sha256::{ContentHash, HashParseError};

/// Classification of domain object types stored in the evidence repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DomainObjectType {
    /// Registered upstream source provider or catalog (`Source`).
    Source,
    /// Specific acquired source record version (`SourceRecord`).
    SourceRecord,
    /// Content-addressed original or derivative blob (`Artifact`).
    Artifact,
    /// Bibliographic document with versioned text extraction and anchors (`Document`).
    Document,
    /// Physical or reported observation with explicit measurement/missing state (`Observation`).
    Observation,
    /// Grouped event with reversible cluster history (`Event`).
    Event,
    /// Asserted proposition with assessment and review state (`Claim`).
    Claim,
    /// Investigation / case workspace (`Investigation`).
    Investigation,
    /// Pinned analysis execution record (`AnalysisRun`).
    AnalysisRun,
    /// Immutable forecast vintage (`Forecast`).
    Forecast,
    /// Registered local hardware device (`Device`).
    Device,
    /// Cited investigation report (`Report`).
    Report,
    /// Human or structured review record (`Review`).
    Review,
    /// Saved search/query definition (`SavedQuery`).
    SavedQuery,
    /// User annotation attached to a record without mutating the target (`Annotation`).
    Annotation,
    /// Competing explanation or hypothesis (`Hypothesis`).
    Hypothesis,
    /// Open research lead / collection plan item (`ResearchLead`).
    ResearchLead,
}

impl DomainObjectType {
    /// Stable machine-readable tag for serialization.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::SourceRecord => "source_record",
            Self::Artifact => "artifact",
            Self::Document => "document",
            Self::Observation => "observation",
            Self::Event => "event",
            Self::Claim => "claim",
            Self::Investigation => "investigation",
            Self::AnalysisRun => "analysis_run",
            Self::Forecast => "forecast",
            Self::Device => "device",
            Self::Report => "report",
            Self::Review => "review",
            Self::SavedQuery => "saved_query",
            Self::Annotation => "annotation",
            Self::Hypothesis => "hypothesis",
            Self::ResearchLead => "research_lead",
        }
    }

    /// Parses a machine-readable tag into a [`DomainObjectType`].
    pub fn from_tag(tag: &str) -> Result<Self, DomainError> {
        match tag {
            "source" => Ok(Self::Source),
            "source_record" => Ok(Self::SourceRecord),
            "artifact" => Ok(Self::Artifact),
            "document" => Ok(Self::Document),
            "observation" => Ok(Self::Observation),
            "event" => Ok(Self::Event),
            "claim" => Ok(Self::Claim),
            "investigation" => Ok(Self::Investigation),
            "analysis_run" => Ok(Self::AnalysisRun),
            "forecast" => Ok(Self::Forecast),
            "device" => Ok(Self::Device),
            "report" => Ok(Self::Report),
            "review" => Ok(Self::Review),
            "saved_query" => Ok(Self::SavedQuery),
            "annotation" => Ok(Self::Annotation),
            "hypothesis" => Ok(Self::Hypothesis),
            "research_lead" => Ok(Self::ResearchLead),
            other => Err(DomainError::InvalidEnumTag {
                field: "object_type",
                value: other.to_owned(),
            }),
        }
    }
}

/// Explicit evidence origin class (`docs/architecture.md` Section 2.3).
///
/// Downstream operations must not promote an inference, forecast, predictive derivative,
/// simulation, or illustrative recreation into an observation or acquired source material.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OriginClass {
    /// Original acquired source material (document, feed frame, archive record).
    AcquiredSourceMaterial,
    /// Direct user or witness testimony.
    UserTestimony,
    /// Normalized physical or instrument observation.
    NormalizedObservation,
    /// Extracted claim or assertion from a source document/article.
    ExtractedClaim,
    /// Analytical inference derived from premises/evidence.
    AnalyticalInference,
    /// Issued forecast or conditional scenario projection.
    Forecast,
    /// Conventional deterministic derivative (crop, OCR text, contrast stretch, lens rectification).
    ConventionalDerivative,
    /// Predictive derivative (learned super-resolution, inpainting, temporal interpolation).
    PredictiveDerivative,
    /// Controlled synthetic simulation fixture.
    Simulation,
    /// Illustrative recreation prompt or generated illustration.
    IllustrativeRecreation,
}

impl OriginClass {
    /// Returns `true` if this class represents generative, predictive, inferred, or simulated material
    /// that must never be promoted into `AcquiredSourceMaterial` or `NormalizedObservation`.
    #[must_use]
    pub const fn is_non_observational(self) -> bool {
        matches!(
            self,
            Self::AnalyticalInference
                | Self::Forecast
                | Self::PredictiveDerivative
                | Self::Simulation
                | Self::IllustrativeRecreation
        )
    }

    /// Validates whether an output with `target` origin class may be produced from `inputs`.
    ///
    /// Feeding an inference, forecast, predictive derivative, simulation, or recreation into
    /// a downstream step cannot promote the result to `AcquiredSourceMaterial` or `NormalizedObservation`.
    pub fn validate_derivation(inputs: &[Self], target: Self) -> Result<(), DomainError> {
        let has_non_obs = inputs.iter().copied().any(Self::is_non_observational);
        if has_non_obs
            && matches!(
                target,
                Self::AcquiredSourceMaterial | Self::NormalizedObservation | Self::UserTestimony
            )
        {
            return Err(DomainError::IllegalOriginPromotion { target });
        }
        Ok(())
    }

    /// Machine-readable tag.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AcquiredSourceMaterial => "acquired_source_material",
            Self::UserTestimony => "user_testimony",
            Self::NormalizedObservation => "normalized_observation",
            Self::ExtractedClaim => "extracted_claim",
            Self::AnalyticalInference => "analytical_inference",
            Self::Forecast => "forecast",
            Self::ConventionalDerivative => "conventional_derivative",
            Self::PredictiveDerivative => "predictive_derivative",
            Self::Simulation => "simulation",
            Self::IllustrativeRecreation => "illustrative_recreation",
        }
    }

    /// Parses a machine-readable tag.
    pub fn from_tag(tag: &str) -> Result<Self, DomainError> {
        match tag {
            "acquired_source_material" => Ok(Self::AcquiredSourceMaterial),
            "user_testimony" => Ok(Self::UserTestimony),
            "normalized_observation" => Ok(Self::NormalizedObservation),
            "extracted_claim" => Ok(Self::ExtractedClaim),
            "analytical_inference" => Ok(Self::AnalyticalInference),
            "forecast" => Ok(Self::Forecast),
            "conventional_derivative" => Ok(Self::ConventionalDerivative),
            "predictive_derivative" => Ok(Self::PredictiveDerivative),
            "simulation" => Ok(Self::Simulation),
            "illustrative_recreation" => Ok(Self::IllustrativeRecreation),
            other => Err(DomainError::InvalidEnumTag {
                field: "origin_class",
                value: other.to_owned(),
            }),
        }
    }
}

/// Evidentiary assessment of a [`Claim`] (`docs/architecture.md` Section 2.4).
///
/// Separate from [`ReviewStatus`] and [`AvailabilityState`]. An unsupported claim is
/// not necessarily false; a reviewed claim is not necessarily supported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ClaimAssessment {
    /// Supported by cited independent evidence.
    Supported,
    /// Contradicted by cited evidence.
    Contradicted,
    /// Available evidence is insufficient to establish or refute the proposition.
    UnsupportedByAvailableEvidence,
    /// Contains selective comparisons, headline/caption mismatch, or missing material context.
    MateriallyMisleading,
    /// Credible sources conflict without a resolved conclusion.
    Disputed,
    /// Assessment has not yet been determined.
    Unresolved,
}

impl ClaimAssessment {
    /// Machine-readable tag.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::Contradicted => "contradicted",
            Self::UnsupportedByAvailableEvidence => "unsupported_by_available_evidence",
            Self::MateriallyMisleading => "materially_misleading",
            Self::Disputed => "disputed",
            Self::Unresolved => "unresolved",
        }
    }

    /// Parses a machine-readable tag.
    pub fn from_tag(tag: &str) -> Result<Self, DomainError> {
        match tag {
            "supported" => Ok(Self::Supported),
            "contradicted" => Ok(Self::Contradicted),
            "unsupported_by_available_evidence" => Ok(Self::UnsupportedByAvailableEvidence),
            "materially_misleading" => Ok(Self::MateriallyMisleading),
            "disputed" => Ok(Self::Disputed),
            "unresolved" => Ok(Self::Unresolved),
            other => Err(DomainError::InvalidEnumTag {
                field: "claim_assessment",
                value: other.to_owned(),
            }),
        }
    }
}

/// Human and workflow review state (`docs/architecture.md` Section 2.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReviewStatus {
    /// Not yet reviewed by a human researcher.
    Unreviewed,
    /// Currently undergoing review.
    InReview,
    /// Reviewed by a named actor; see linked `Review` record for scope and assessment.
    Reviewed,
    /// Challenged by a reviewer with documented counter-evidence or methodological objection.
    Challenged,
    /// Superseded by a newer version.
    Superseded,
}

impl ReviewStatus {
    /// Machine-readable tag.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unreviewed => "unreviewed",
            Self::InReview => "in_review",
            Self::Reviewed => "reviewed",
            Self::Challenged => "challenged",
            Self::Superseded => "superseded",
        }
    }

    /// Parses a machine-readable tag.
    pub fn from_tag(tag: &str) -> Result<Self, DomainError> {
        match tag {
            "unreviewed" => Ok(Self::Unreviewed),
            "in_review" => Ok(Self::InReview),
            "reviewed" => Ok(Self::Reviewed),
            "challenged" => Ok(Self::Challenged),
            "superseded" => Ok(Self::Superseded),
            other => Err(DomainError::InvalidEnumTag {
                field: "review_status",
                value: other.to_owned(),
            }),
        }
    }
}

/// Availability state of underlying bytes or upstream source material (`docs/architecture.md` Section 2.4).
///
/// A `Removed` upstream page or deleted local blob does not automatically mark a claim retracted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AvailabilityState {
    /// Retained bytes and metadata are available locally.
    Available,
    /// Metadata and external citation reference are retained; bytes are not stored locally.
    ReferenceOnly,
    /// Expected bytes are missing.
    Missing,
    /// Access is restricted by source policy or workspace role.
    Restricted,
    /// Removed upstream or tombstoned locally; metadata remains for auditability.
    Removed,
    /// Retention window expired.
    Expired,
    /// Integrity check (SHA-256) failed.
    Corrupt,
}

impl AvailabilityState {
    /// Machine-readable tag.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::ReferenceOnly => "reference_only",
            Self::Missing => "missing",
            Self::Restricted => "restricted",
            Self::Removed => "removed",
            Self::Expired => "expired",
            Self::Corrupt => "corrupt",
        }
    }

    /// Parses a machine-readable tag.
    pub fn from_tag(tag: &str) -> Result<Self, DomainError> {
        match tag {
            "available" => Ok(Self::Available),
            "reference_only" => Ok(Self::ReferenceOnly),
            "missing" => Ok(Self::Missing),
            "restricted" => Ok(Self::Restricted),
            "removed" => Ok(Self::Removed),
            "expired" => Ok(Self::Expired),
            "corrupt" => Ok(Self::Corrupt),
            other => Err(DomainError::InvalidEnumTag {
                field: "availability_state",
                value: other.to_owned(),
            }),
        }
    }
}

/// Reassessment state triggered when an upstream source, claim, or calibration changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReassessmentState {
    /// All pinned upstream dependencies match their current versions.
    Current,
    /// One or more upstream dependencies were revised, corrected, retracted, or deleted.
    NeedsReassessment {
        /// Version IDs of upstream dependencies that triggered invalidation.
        upstream_versions: Vec<VersionId>,
        /// Reasons recorded during dependency walk.
        reasons: Vec<String>,
        /// Timestamp (UTC ms) when flagged.
        flagged_at_ms: i64,
    },
}

/// Workspace and case visibility / access policy association (`CASE-01`, `COLLAB-01`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceVisibility {
    /// Private to the owning investigation/case (default; saving never publishes).
    PrivateCase {
        /// Owning investigation ID.
        case_id: OpaqueId,
        /// Whether this artifact contains sensitive location/identity material requiring elevated role access.
        sensitive: bool,
    },
    /// Shared within a collaborative case under an explicit access policy ID.
    SharedCase {
        /// Owning investigation ID.
        case_id: OpaqueId,
        /// Access policy ID.
        policy_id: OpaqueId,
        /// Whether this artifact is restricted to owner/editor roles even within the shared case.
        sensitive: bool,
    },
    /// Explicitly published or library-scoped reference material.
    LibraryPublic,
}

impl WorkspaceVisibility {
    /// Returns the owning case ID if scoped to a case.
    #[must_use]
    pub fn case_id(&self) -> Option<&OpaqueId> {
        match self {
            Self::PrivateCase { case_id, .. } | Self::SharedCase { case_id, .. } => Some(case_id),
            Self::LibraryPublic => None,
        }
    }

    /// Returns `true` if marked sensitive.
    #[must_use]
    pub const fn is_sensitive(&self) -> bool {
        match self {
            Self::PrivateCase { sensitive, .. } | Self::SharedCase { sensitive, .. } => *sensitive,
            Self::LibraryPublic => false,
        }
    }
}

/// Provenance link carried by every persisted [`ObjectEnvelope`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvenanceRecord {
    /// Upstream `SourceRecord` or input `VersionId`s from which this object was derived.
    pub source_version_ids: Vec<VersionId>,
    /// Optional explicit statement when an object is a manual user assumption or proposal
    /// rather than derived from acquired source records.
    pub declared_origin_note: Option<String>,
    /// Ordered processing or transformation steps applied to produce this version.
    pub processing_history: Vec<String>,
}

impl ProvenanceRecord {
    /// Validates that substantive objects either cite at least one source version or explicitly
    /// declare their manual/assumed origin (`Shared Research Rule 1`).
    pub fn validate(&self) -> Result<(), DomainError> {
        let has_sources = !self.source_version_ids.is_empty();
        let has_note = self
            .declared_origin_note
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty());
        if !has_sources && !has_note {
            return Err(DomainError::MissingProvenance);
        }
        Ok(())
    }
}

/// Common envelope carried by every persisted domain object (`docs/architecture.md` Section 2.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectEnvelope {
    /// Stable semantic identifier (`id`).
    pub id: OpaqueId,
    /// Immutable version identifier (`version_id`); never reused across edits.
    pub version_id: VersionId,
    /// Schema version (`schema_version`).
    pub schema_version: SchemaVersion,
    /// Domain object classification.
    pub object_type: DomainObjectType,
    /// Creation timestamp (UTC milliseconds).
    pub created_at_ms: i64,
    /// Creating actor or service process.
    pub created_by: ActorId,
    /// Workspace/case ownership and sensitivity scope.
    pub visibility: WorkspaceVisibility,
    /// Previous version superseded by this revision, if any.
    pub supersedes: Option<VersionId>,
    /// Provenance links and processing history.
    pub provenance: ProvenanceRecord,
    /// Evidence origin class.
    pub origin_class: OriginClass,
    /// Human/workflow review status.
    pub review_status: ReviewStatus,
    /// Underlying availability state.
    pub availability: AvailabilityState,
    /// Downstream reassessment status.
    pub reassessment: ReassessmentState,
}

impl ObjectEnvelope {
    /// Validates envelope invariants (provenance presence and self-supersession prevention).
    pub fn validate(&self) -> Result<(), DomainError> {
        self.provenance.validate()?;
        if self.supersedes.as_ref() == Some(&self.version_id) {
            return Err(DomainError::SelfSupersession(self.version_id.clone()));
        }
        Ok(())
    }
}

/// Per-source reuse conditions (`Shared Research Rule 9`, `CAM-01`, `CAM-02`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReuseConditions {
    /// How the source is ordinarily accessed (e.g., `"public_directory_embed"`, `"open_archive_api"`).
    pub ordinary_access_method: String,
    /// Whether automated processing/analysis is permitted under the source's conditions.
    pub automated_processing_allowed: bool,
    /// Whether local byte retention is permitted (`true`) or reference-only (`false`).
    pub retention_allowed: bool,
    /// Whether redistribution in exported bundles is permitted (`true`) or reference-only (`false`).
    pub redistribution_allowed: bool,
    /// Required attribution notice.
    pub attribution: String,
}

/// Health state of a registered [`Source`] (`docs/architecture.md` Section 4.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceHealthState {
    /// Registered but missing required configuration or credentials.
    Unconfigured,
    /// Establishing initial connection.
    Connecting,
    /// Operational and responding within budget.
    Available,
    /// Operational with partial failures or elevated latency.
    Degraded,
    /// Data age exceeds expected cadence/freshness threshold.
    Stale,
    /// Upstream endpoint unreachable or restricted.
    Unavailable,
    /// Explicitly disabled by user or policy.
    Disabled,
}

/// Registered upstream data source (`Source`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// Common object envelope (`DomainObjectType::Source`).
    pub envelope: ObjectEnvelope,
    /// Provider or directory name.
    pub provider_name: String,
    /// Capability tags (`catalog_search`, `historical_query`, `live_polling`, `streaming`, `media_retrieval`).
    pub capabilities: Vec<String>,
    /// Separate conditions for access, automated processing, retention, and redistribution.
    pub reuse_conditions: ReuseConditions,
    /// Opaque credential reference (never a raw secret or token).
    pub credential_ref: Option<OpaqueId>,
    /// Current operational health state (distinct from data observations).
    pub health: SourceHealthState,
}

/// Reason why original bytes were omitted from local storage or a bundle export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OmissionReason {
    /// Source reuse conditions prohibit local byte retention.
    RetentionProhibitedByPolicy,
    /// Source reuse conditions prohibit bundle redistribution; reference retained.
    RedistributionProhibitedByPolicy,
    /// Artifact contains sensitive material redacted for the target export scope.
    RedactedSensitiveContent,
    /// Upstream acquisition failed or returned reference metadata only.
    UpstreamReferenceOnly,
}

/// Retained blob reference or explicit omission reason on a [`SourceRecord`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetainedContent {
    /// Original bytes retained in content-addressed storage.
    RetainedBlob {
        /// SHA-256 content hash of the original bytes.
        content_hash: ContentHash,
        /// Exact byte length.
        byte_len: u64,
        /// IANA media type.
        media_type: String,
    },
    /// Original bytes intentionally omitted with an explicit reason.
    Omitted {
        /// Reason bytes are not stored.
        reason: OmissionReason,
        /// Optional upstream digest if known.
        known_upstream_hash: Option<ContentHash>,
    },
}

/// A specific acquired version from a registered [`Source`] (`SourceRecord`).
///
/// Two publishers distributing identical bytes remain distinct `SourceRecord` objects
/// even when their [`RetainedContent::RetainedBlob`] shares the same [`ContentHash`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRecord {
    /// Common object envelope (`DomainObjectType::SourceRecord`).
    pub envelope: ObjectEnvelope,
    /// Parent `Source` ID.
    pub source_id: OpaqueId,
    /// Upstream provider record identifier/version string.
    pub upstream_version: String,
    /// Acquisition attempt identifier.
    pub acquisition_attempt_id: OpaqueId,
    /// Canonical source URL or archive locator.
    pub source_locator: String,
    /// Event or capture timestamp (UTC ms) if reported by the source.
    pub source_event_time_ms: Option<i64>,
    /// First public availability or publication timestamp (UTC ms) if known.
    pub public_availability_time_ms: Option<i64>,
    /// Retrieval timestamp (UTC ms) when OIA acquired the payload.
    pub retrieved_at_ms: i64,
    /// Retained blob reference or explicit omission reason.
    pub content: RetainedContent,
}

/// Content-addressed original or derivative artifact (`Artifact`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    /// Common object envelope (`DomainObjectType::Artifact`).
    pub envelope: ObjectEnvelope,
    /// Content hash of the artifact bytes.
    pub content_hash: ContentHash,
    /// Media type (e.g., `"application/pdf"`, `"image/png"`, `"text/plain"`).
    pub media_type: String,
    /// Exact byte length.
    pub byte_len: u64,
    /// If this is a derivative, the exact original `Artifact` version ID it was derived from.
    /// Original artifacts have `derived_from_artifact: None`.
    pub derived_from_artifact: Option<VersionId>,
    /// If produced by a quality-reducing or predictive transform, records its disclosed effect.
    pub quality_effect_disclosure: Option<String>,
}

/// Extraction completeness for a [`CitationAnchor`] (`DOC-02`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractionCoverage {
    /// Passage was cleanly extracted.
    Complete,
    /// Passage was partially extracted (e.g., degraded scan / partial OCR).
    Partial {
        /// Description of unreadable or degraded region.
        limitation: String,
    },
    /// Page or region could not be read at all.
    Unreadable {
        /// Explanation (e.g., heavy redaction or corrupt scan page).
        reason: String,
    },
}

/// Exact page/byte citation anchor within a specific [`Document`] version (`DOC-02`, `DOC-04`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationAnchor {
    /// Stable anchor ID within the document version.
    pub anchor_id: OpaqueId,
    /// Pin to the exact `Document` version ID.
    pub document_version_id: VersionId,
    /// 1-based page number, if paginated.
    pub page_number: Option<u32>,
    /// Start byte offset within the extracted passage text.
    pub byte_start: usize,
    /// End byte offset (exclusive) within the extracted passage text.
    pub byte_end: usize,
    /// SHA-256 hash of the anchored text span (when readable).
    pub span_hash: Option<ContentHash>,
    /// Extraction status for this page/region.
    pub coverage: ExtractionCoverage,
}

impl CitationAnchor {
    /// Validates that `byte_start <= byte_end`.
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.byte_start > self.byte_end {
            return Err(DomainError::InvalidAnchorSpan {
                start: self.byte_start,
                end: self.byte_end,
            });
        }
        Ok(())
    }
}

/// Edition or release relationship for a [`Document`] (`DOC-02`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentReleaseKind {
    /// Initial known release from this source.
    OriginalRelease,
    /// Revised edition or updated preprint version (e.g., arXiv v2).
    Revision {
        /// Previous document version ID.
        previous_document_version: VersionId,
    },
    /// Differently redacted FOIA/declassified release of the same underlying record.
    DifferentlyRedactedRelease {
        /// Related release document version ID.
        related_document_version: VersionId,
        /// Summary of redaction difference.
        redaction_note: String,
    },
    /// Separate publisher distributing identical or syndicated text.
    SyndicatedOrRepublished {
        /// Upstream source record version if identified.
        upstream_source_version: Option<VersionId>,
    },
}

/// Public document record with bibliographic metadata and anchored passages (`Document`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// Common object envelope (`DomainObjectType::Document`).
    pub envelope: ObjectEnvelope,
    /// Backing `SourceRecord` version ID.
    pub source_record_version: VersionId,
    /// Original retained `Artifact` version ID, if bytes are retained.
    pub original_artifact_version: Option<VersionId>,
    /// Document title.
    pub title: String,
    /// Authors or issuing office.
    pub authors: Vec<String>,
    /// Publisher or archive name.
    pub publisher: String,
    /// Language code (BCP-47, e.g., `"en"`).
    pub language: String,
    /// Release classification (`OriginalRelease`, `Revision`, `DifferentlyRedactedRelease`, etc.).
    pub release_kind: DocumentReleaseKind,
    /// Extracted text passages keyed by [`CitationAnchor::anchor_id`].
    pub passages: BTreeMap<OpaqueId, String>,
    /// Anchors defining page/span boundaries and unreadable/partial page flags.
    pub anchors: Vec<CitationAnchor>,
    /// Total page count of the source document, if applicable.
    pub total_pages: Option<u32>,
}

impl Document {
    /// Validates all anchors and verifies that untrusted document text is stored strictly as data.
    pub fn validate(&self) -> Result<(), DomainError> {
        self.envelope.validate()?;
        for anchor in &self.anchors {
            anchor.validate()?;
            if anchor.document_version_id != self.envelope.version_id {
                return Err(DomainError::MismatchedAnchorDocumentVersion {
                    anchor_id: anchor.anchor_id.clone(),
                    expected: self.envelope.version_id.clone(),
                    actual: anchor.document_version_id.clone(),
                });
            }
        }
        Ok(())
    }
}

/// Recorded measurement or explicit absence on an [`Observation`] (`AGENTS.md` Section 2).
///
/// Missing fields are NEVER converted to `0.0`, default altitudes, or interpolated values.
#[derive(Debug, Clone, PartialEq)]
pub enum RecordedValue {
    /// Directly measured physical quantity with explicit unit and optional symmetric uncertainty.
    Measured {
        /// Finite measured value.
        value: f64,
        /// Explicit physical unit (e.g., `"m"`, `"deg"`, `"hPa"`, `"Hz"`).
        unit: String,
        /// 1-sigma or bounded uncertainty in the same unit, if known.
        uncertainty: Option<f64>,
    },
    /// Derived or estimated value with stated method and uncertainty.
    Estimated {
        /// Finite estimated value.
        value: f64,
        /// Explicit unit.
        unit: String,
        /// Estimation method (e.g., `"witness_angular_estimate"`).
        method: String,
        /// Uncertainty bound in the same unit.
        uncertainty: Option<f64>,
    },
    /// Value was not observed or was missing from the source payload.
    Missing {
        /// Reason the measurement is absent (e.g., `"altitude_omitted_by_transponder"`).
        reason: String,
    },
}

impl RecordedValue {
    /// Validates that any numeric values and uncertainties are finite (`!is_nan()` and `!is_infinite()`)
    /// and that units are non-empty.
    pub fn validate(&self) -> Result<(), DomainError> {
        match self {
            Self::Measured {
                value,
                unit,
                uncertainty,
            }
            | Self::Estimated {
                value,
                unit,
                uncertainty,
                ..
            } => {
                if !value.is_finite() {
                    return Err(DomainError::NonFiniteNumericValue("observation.value"));
                }
                if unit.trim().is_empty() {
                    return Err(DomainError::EmptyUnit);
                }
                if let Some(u) = uncertainty
                    && (!u.is_finite() || *u < 0.0)
                {
                    return Err(DomainError::InvalidUncertainty);
                }
                Ok(())
            }
            Self::Missing { reason } => {
                if reason.trim().is_empty() {
                    return Err(DomainError::MissingReasonRequired);
                }
                Ok(())
            }
        }
    }
}

/// Physical or reported observation (`Observation`).
#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    /// Common object envelope (`DomainObjectType::Observation`).
    pub envelope: ObjectEnvelope,
    /// Property name (e.g., `"azimuth_deg"`, `"barometric_altitude"`, `"apparent_magnitude"`).
    pub property: String,
    /// Measured, estimated, or explicitly missing value.
    pub recorded_value: RecordedValue,
    /// Observer or instrument identifier.
    pub observer_or_instrument: String,
    /// Capture or event timestamp (UTC ms), if known (`None` when time is unknown).
    pub observed_at_ms: Option<i64>,
    /// Preserved original time text from witness/source (e.g., `"approx 21:15 local"`).
    pub original_time_text: String,
}

impl Observation {
    /// Validates envelope and recorded value invariants.
    pub fn validate(&self) -> Result<(), DomainError> {
        self.envelope.validate()?;
        self.recorded_value.validate()
    }
}

/// Nature of a [`Claim`], keeping quoting a speaker distinct from endorsing truth (`NEWS-04`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimNature {
    /// Direct factual proposition asserted about the world.
    FactualProposition,
    /// Record that a named speaker/organization made a statement (separate from whether the statement is true).
    AttributedStatement {
        /// Speaker or issuing entity.
        speaker: String,
    },
    /// Hypothesis or analytical proposal under evaluation.
    AnalyticalProposal,
}

/// Asserted proposition with citation anchor, assessment, and review state (`Claim`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    /// Common object envelope (`DomainObjectType::Claim`).
    pub envelope: ObjectEnvelope,
    /// Exact proposition text.
    pub proposition: String,
    /// Claimant or source entity.
    pub claimant: String,
    /// Whether this is a factual proposition, attributed statement, or analytical proposal.
    pub nature: ClaimNature,
    /// Optional citation anchor in a source [`Document`] version.
    pub supporting_anchor: Option<CitationAnchor>,
    /// Evidentiary assessment (`Supported`, `Contradicted`, `UnsupportedByAvailableEvidence`, etc.).
    pub assessment: ClaimAssessment,
    /// Explicit criteria and reasoning supporting `assessment`.
    pub assessment_rationale: String,
}

/// Reversible entity-resolution or event-clustering decision (`REASON-03`, `GEO-03`, `A08`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReversibleClusterDecision {
    /// Decision identifier.
    pub decision_id: OpaqueId,
    /// Member record version IDs grouped or linked by this decision.
    pub member_versions: Vec<VersionId>,
    /// Explicit matching criteria and tolerances used.
    pub matching_criteria: String,
    /// Documented conflicts or disagreements preserved across members.
    pub preserved_conflicts: Vec<String>,
    /// Whether this grouping decision is currently active (`true`) or reverted (`false`).
    pub active: bool,
    /// Actor who recorded or reverted the decision.
    pub decided_by: ActorId,
}

/// Grouped event with separate article/account/independent-source counts (`Event`, `GEO-03`, `REASON-03`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// Common object envelope (`DomainObjectType::Event`).
    pub envelope: ObjectEnvelope,
    /// Proposed event classification (e.g., `"uap_sighting_cluster"`, `"regional_transport_disruption"`).
    pub event_kind: String,
    /// Summary title.
    pub title: String,
    /// Count of distinct news/document articles mentioning the event.
    pub article_count: usize,
    /// Count of witness or reporter accounts.
    pub account_count: usize,
    /// Count of traced independent source roots (excluding syndicated copies).
    pub independent_source_count: usize,
    /// Reversible split/merge and entity-match decisions.
    pub cluster_history: Vec<ReversibleClusterDecision>,
}

/// User annotation attached to a target version without mutating the target (`CASE-03`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Annotation {
    /// Common object envelope (`DomainObjectType::Annotation`).
    pub envelope: ObjectEnvelope,
    /// Target object version ID being annotated.
    pub target_version_id: VersionId,
    /// Annotation note text.
    pub body: String,
}

/// Saved query definition (`CASE-03`, `CASE-05`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedQuery {
    /// Common object envelope (`DomainObjectType::SavedQuery`).
    pub envelope: ObjectEnvelope,
    /// Human-readable query name.
    pub name: String,
    /// Exact query expression or text filter.
    pub query_text: String,
    /// Scoped investigation ID, if restricted to a single case.
    pub case_scope: Option<OpaqueId>,
    /// Topic tags included in filter.
    pub topic_filters: Vec<String>,
}

/// Competing hypothesis tracked within an [`Investigation`] (`CASE-03`, `REASON-04`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hypothesis {
    /// Common object envelope (`DomainObjectType::Hypothesis`).
    pub envelope: ObjectEnvelope,
    /// Hypothesis statement.
    pub statement: String,
    /// Supporting evidence version IDs.
    pub supporting_versions: Vec<VersionId>,
    /// Contradicting or challenging evidence version IDs.
    pub contradicting_versions: Vec<VersionId>,
    /// Observable evidence that would discriminate between this hypothesis and alternatives.
    pub discriminating_observables: Vec<String>,
}

/// Open research lead or collection plan item (`CASE-03`, `INTEL-01`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchLead {
    /// Common object envelope (`DomainObjectType::ResearchLead`).
    pub envelope: ObjectEnvelope,
    /// Open research question.
    pub question: String,
    /// Candidate sources to check.
    pub candidate_sources: Vec<String>,
    /// Current status (`"open"`, `"in_progress"`, `"resolved"`).
    pub status: String,
}

/// Investigation / case workspace (`Investigation`, `CASE-01`–`CASE-03`).
///
/// Saving an investigation never publishes it; `published` defaults to `false`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Investigation {
    /// Common object envelope (`DomainObjectType::Investigation`).
    pub envelope: ObjectEnvelope,
    /// Case title.
    pub title: String,
    /// Guiding investigation question.
    pub question: String,
    /// Optional approximate or bounded geographic area description.
    pub geographic_scope: Option<String>,
    /// Optional time interval summary (preserving approximate/unknown bounds).
    pub temporal_scope: Option<String>,
    /// Topic tags.
    pub topics: Vec<String>,
    /// Pinned evidence version IDs (sources, documents, observations, claims, analyses, reports).
    pub evidence_versions: Vec<VersionId>,
    /// Saved query version IDs.
    pub saved_query_versions: Vec<VersionId>,
    /// Annotation version IDs (stored separately so source records remain immutable).
    pub annotation_versions: Vec<VersionId>,
    /// Competing hypothesis version IDs.
    pub hypothesis_versions: Vec<VersionId>,
    /// Research lead version IDs.
    pub lead_versions: Vec<VersionId>,
    /// Explicit publication flag (`false` when saved privately; requires explicit publish command).
    pub is_published: bool,
}

/// Publication and export state of a [`Report`] (`COLLAB-03`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportPublicationState {
    /// Editable private draft inside the workspace.
    PrivateDraft,
    /// Submitted for collaborator review.
    InReview,
    /// Exported privately to a local file/bundle without public publication.
    PrivateExported,
    /// Explicitly published by an authorized owner.
    Published,
}

/// A statement block inside a [`Report`] linked to exact evidence versions and citation anchors (`COLLAB-02`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportBlock {
    /// Block identifier.
    pub block_id: OpaqueId,
    /// Statement text.
    pub statement: String,
    /// Exact evidence version IDs cited by this statement.
    pub cited_versions: Vec<VersionId>,
    /// Exact document citation anchors supporting this statement.
    pub citation_anchors: Vec<CitationAnchor>,
    /// Whether this block was drafted by an AI model and requires/received human review.
    pub ai_generated: bool,
    /// Review status of this specific statement block.
    pub review_status: ReviewStatus,
    /// Whether this block contains sensitive details that should be redacted in restricted exports.
    pub redact_in_restricted_export: bool,
}

/// Cited investigation report (`Report`, `COLLAB-02`, `COLLAB-03`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// Common object envelope (`DomainObjectType::Report`).
    pub envelope: ObjectEnvelope,
    /// Owning investigation ID.
    pub investigation_id: OpaqueId,
    /// Report title.
    pub title: String,
    /// Ordered statement blocks with citation anchors.
    pub blocks: Vec<ReportBlock>,
    /// Open questions and stated coverage limitations.
    pub limitations_and_open_questions: Vec<String>,
    /// Draft / private-export / published state (saving or exporting privately never sets `Published`).
    pub publication_state: ReportPublicationState,
}

/// Structured review record (`Review`, `COLLAB-01`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Review {
    /// Common object envelope (`DomainObjectType::Review`).
    pub envelope: ObjectEnvelope,
    /// Target object version ID being reviewed.
    pub target_version_id: VersionId,
    /// Reviewer assessment (`Reviewed` or `Challenged`).
    pub resulting_status: ReviewStatus,
    /// Scope and rationale of the review (never implies universal truth).
    pub rationale: String,
    /// Evidence versions cited by the reviewer.
    pub cited_evidence_versions: Vec<VersionId>,
}

/// Tagged union of persisted principal domain objects.
#[derive(Debug, Clone, PartialEq)]
pub enum DomainObject {
    /// Registered source.
    Source(Source),
    /// Acquired source record version.
    SourceRecord(SourceRecord),
    /// Content-addressed artifact.
    Artifact(Artifact),
    /// Document with anchored passages.
    Document(Document),
    /// Physical or reported observation.
    Observation(Observation),
    /// Grouped event.
    Event(Event),
    /// Asserted claim.
    Claim(Claim),
    /// Investigation workspace.
    Investigation(Investigation),
    /// Cited report.
    Report(Report),
    /// Review record.
    Review(Review),
    /// User annotation.
    Annotation(Annotation),
    /// Saved query.
    SavedQuery(SavedQuery),
    /// Competing hypothesis.
    Hypothesis(Hypothesis),
    /// Research lead.
    ResearchLead(ResearchLead),
}

impl DomainObject {
    /// Returns a shared reference to the object's [`ObjectEnvelope`].
    #[must_use]
    pub const fn envelope(&self) -> &ObjectEnvelope {
        match self {
            Self::Source(x) => &x.envelope,
            Self::SourceRecord(x) => &x.envelope,
            Self::Artifact(x) => &x.envelope,
            Self::Document(x) => &x.envelope,
            Self::Observation(x) => &x.envelope,
            Self::Event(x) => &x.envelope,
            Self::Claim(x) => &x.envelope,
            Self::Investigation(x) => &x.envelope,
            Self::Report(x) => &x.envelope,
            Self::Review(x) => &x.envelope,
            Self::Annotation(x) => &x.envelope,
            Self::SavedQuery(x) => &x.envelope,
            Self::Hypothesis(x) => &x.envelope,
            Self::ResearchLead(x) => &x.envelope,
        }
    }

    /// Returns a mutable reference to the object's [`ObjectEnvelope`].
    pub fn envelope_mut(&mut self) -> &mut ObjectEnvelope {
        match self {
            Self::Source(x) => &mut x.envelope,
            Self::SourceRecord(x) => &mut x.envelope,
            Self::Artifact(x) => &mut x.envelope,
            Self::Document(x) => &mut x.envelope,
            Self::Observation(x) => &mut x.envelope,
            Self::Event(x) => &mut x.envelope,
            Self::Claim(x) => &mut x.envelope,
            Self::Investigation(x) => &mut x.envelope,
            Self::Report(x) => &mut x.envelope,
            Self::Review(x) => &mut x.envelope,
            Self::Annotation(x) => &mut x.envelope,
            Self::SavedQuery(x) => &mut x.envelope,
            Self::Hypothesis(x) => &mut x.envelope,
            Self::ResearchLead(x) => &mut x.envelope,
        }
    }

    /// Validates domain invariants for this object.
    pub fn validate(&self) -> Result<(), DomainError> {
        match self {
            Self::Document(doc) => doc.validate(),
            Self::Observation(obs) => obs.validate(),
            other => other.envelope().validate(),
        }
    }
}

/// Typed domain errors across `oia-domain`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    /// Invalid identifier or schema version.
    Id(IdError),
    /// Invalid content hash hex string.
    Hash(HashParseError),
    /// JSON codec or schema error.
    Json(JsonError),
    /// Unrecognized enum tag in serialized data.
    InvalidEnumTag {
        /// Field name.
        field: &'static str,
        /// Unrecognized tag string.
        value: String,
    },
    /// Substantive record lacked both source version references and a declared manual origin note.
    MissingProvenance,
    /// Object version attempted to supersede itself.
    SelfSupersession(VersionId),
    /// Attempted to promote an inference, forecast, predictive derivative, simulation, or recreation
    /// into an observation or acquired source record.
    IllegalOriginPromotion {
        /// Disallowed target origin class.
        target: OriginClass,
    },
    /// Citation anchor had `byte_start > byte_end`.
    InvalidAnchorSpan {
        /// Start byte offset.
        start: usize,
        /// End byte offset.
        end: usize,
    },
    /// Anchor referenced a different document version than its owning document.
    MismatchedAnchorDocumentVersion {
        /// Anchor ID.
        anchor_id: OpaqueId,
        /// Owning document version.
        expected: VersionId,
        /// Anchor's declared document version.
        actual: VersionId,
    },
    /// Non-finite floating-point value rejected.
    NonFiniteNumericValue(&'static str),
    /// Measured or estimated quantity had an empty unit string.
    EmptyUnit,
    /// Uncertainty value was negative or non-finite.
    InvalidUncertainty,
    /// `RecordedValue::Missing` was constructed with an empty reason.
    MissingReasonRequired,
    /// Analysis derivation graph would contain a cycle (`DerivedFrom` must be a DAG).
    CyclicDerivation {
        /// Source version ID.
        from: VersionId,
        /// Target version ID.
        to: VersionId,
    },
    /// Attempted to interpret a `CoOccursWith` edge as organizational affiliation or causation.
    IllegalCoOccurrencePromotion,
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Id(e) => write!(f, "domain ID error: {e}"),
            Self::Hash(e) => write!(f, "content hash error: {e}"),
            Self::Json(e) => write!(f, "JSON error: {e}"),
            Self::InvalidEnumTag { field, value } => {
                write!(f, "invalid tag '{value}' for field '{field}'")
            }
            Self::MissingProvenance => write!(
                f,
                "record must cite at least one source version or state a declared origin note"
            ),
            Self::SelfSupersession(v) => write!(f, "version '{v}' cannot supersede itself"),
            Self::IllegalOriginPromotion { target } => write!(
                f,
                "cannot promote non-observational input to origin class '{}'",
                target.as_str()
            ),
            Self::InvalidAnchorSpan { start, end } => {
                write!(f, "invalid citation anchor span [{start}, {end})")
            }
            Self::MismatchedAnchorDocumentVersion {
                anchor_id,
                expected,
                actual,
            } => write!(
                f,
                "anchor '{anchor_id}' references document version '{actual}', expected '{expected}'"
            ),
            Self::NonFiniteNumericValue(field) => {
                write!(f, "non-finite numeric value in '{field}'")
            }
            Self::EmptyUnit => write!(f, "measurement unit must not be empty"),
            Self::InvalidUncertainty => write!(f, "uncertainty must be finite and non-negative"),
            Self::MissingReasonRequired => {
                write!(f, "missing measurement must provide a non-empty reason")
            }
            Self::CyclicDerivation { from, to } => write!(
                f,
                "adding derivation edge '{from}' -> '{to}' would create a cycle"
            ),
            Self::IllegalCoOccurrencePromotion => write!(
                f,
                "co-occurrence relation cannot be promoted to affiliation or causation"
            ),
        }
    }
}

impl std::error::Error for DomainError {}

impl From<IdError> for DomainError {
    fn from(value: IdError) -> Self {
        Self::Id(value)
    }
}

impl From<HashParseError> for DomainError {
    fn from(value: HashParseError) -> Self {
        Self::Hash(value)
    }
}

impl From<JsonError> for DomainError {
    fn from(value: JsonError) -> Self {
        Self::Json(value)
    }
}
