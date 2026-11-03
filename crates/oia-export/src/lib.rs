//! `oia-export` — Cited report export and reproducible bundle import/export (`#9`, `#22`).
//!
//! Implements the dependency-independent cited report projection, redaction/omission preview,
//! and inert UTF-8 Markdown and self-contained HTML rendering kernel (`ADR 0006` / `D05`,
//! `docs/contracts/report-export.md` §§1–4).

#![forbid(unsafe_code)]
#![deny(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

pub mod bundle;

use std::error::Error;
use std::fmt;

/// Crate boundary marker for `oia-export` (`docs/architecture.md` §1.2, `#9`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportBoundary;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Limit {
    Blocks,
    Citations,
    EvidenceEntries,
    TextBytes,
    OutputBytes,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReportLimits {
    max_blocks: usize,
    max_citations: usize,
    max_evidence_entries: usize,
    max_text_bytes: usize,
    max_output_bytes: usize,
}

impl ReportLimits {
    pub fn new(
        max_blocks: usize,
        max_citations: usize,
        max_evidence_entries: usize,
        max_text_bytes: usize,
        max_output_bytes: usize,
    ) -> Result<Self, ReportError> {
        if max_blocks == 0 {
            return Err(ReportError::ZeroLimit(Limit::Blocks));
        }
        if max_citations == 0 {
            return Err(ReportError::ZeroLimit(Limit::Citations));
        }
        if max_evidence_entries == 0 {
            return Err(ReportError::ZeroLimit(Limit::EvidenceEntries));
        }
        if max_text_bytes == 0 {
            return Err(ReportError::ZeroLimit(Limit::TextBytes));
        }
        if max_output_bytes == 0 {
            return Err(ReportError::ZeroLimit(Limit::OutputBytes));
        }
        Ok(Self {
            max_blocks,
            max_citations,
            max_evidence_entries,
            max_text_bytes,
            max_output_bytes,
        })
    }

    pub fn max_blocks(self) -> usize {
        self.max_blocks
    }

    pub fn max_citations(self) -> usize {
        self.max_citations
    }

    pub fn max_evidence_entries(self) -> usize {
        self.max_evidence_entries
    }

    pub fn max_text_bytes(self) -> usize {
        self.max_text_bytes
    }

    pub fn max_output_bytes(self) -> usize {
        self.max_output_bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopReason {
    Cancelled,
    DeadlineExceeded,
    Superseded,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReportError {
    ZeroLimit(Limit),
    LimitExceeded {
        limit: Limit,
        actual: usize,
        max: usize,
    },
    EmptyIdentifier {
        field: &'static str,
    },
    DuplicateIdentifier {
        kind: &'static str,
        id: String,
    },
    InvalidTimeDisplay {
        field: &'static str,
        reason: &'static str,
    },
    LocalPathDisclosed {
        field: &'static str,
    },
    UnsafeExternalUrl {
        entry_id: String,
        reason: &'static str,
    },
    DisclosurePolicyViolation {
        entry_id: String,
        reason: &'static str,
    },
    UnexplainedDanglingReference {
        statement_id: String,
        entry_id: String,
    },
    ExistenceSensitiveReferenceLeak {
        statement_id: String,
        entry_id: String,
    },
    UnadjustedRedactedAnchor {
        statement_id: String,
        entry_id: String,
    },
    InvalidCitationAnchor {
        statement_id: String,
        entry_id: String,
        reason: &'static str,
    },
    MalformedTargetUtf8 {
        entry_id: String,
        valid_up_to: usize,
    },
    SensitiveConclusionRequiresRedaction {
        statement_id: String,
        entry_id: String,
    },
    AllocationFailed,
    Stopped(StopReason),
}

impl fmt::Display for ReportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroLimit(limit) => {
                write!(formatter, "{limit:?} limit must be greater than zero")
            }
            Self::LimitExceeded { limit, actual, max } => write!(
                formatter,
                "{limit:?} budget exceeded: actual {actual} exceeds limit {max}"
            ),
            Self::EmptyIdentifier { field } => {
                write!(formatter, "required field `{field}` must not be empty")
            }
            Self::DuplicateIdentifier { kind, id } => {
                write!(formatter, "duplicate {kind} identifier `{id}`")
            }
            Self::InvalidTimeDisplay { field, reason } => {
                write!(formatter, "invalid time display in `{field}`: {reason}")
            }
            Self::LocalPathDisclosed { field } => {
                write!(formatter, "local filesystem path rejected in `{field}`")
            }
            Self::UnsafeExternalUrl { entry_id, reason } => write!(
                formatter,
                "unsafe external reference URL on `{entry_id}`: {reason}"
            ),
            Self::DisclosurePolicyViolation { entry_id, reason } => write!(
                formatter,
                "disclosure policy violation on `{entry_id}`: {reason}"
            ),
            Self::UnexplainedDanglingReference {
                statement_id,
                entry_id,
            } => write!(
                formatter,
                "statement `{statement_id}` references unknown evidence entry `{entry_id}`"
            ),
            Self::ExistenceSensitiveReferenceLeak {
                statement_id,
                entry_id,
            } => write!(
                formatter,
                "statement `{statement_id}` cannot cite omitted or existence-sensitive entry `{entry_id}`"
            ),
            Self::UnadjustedRedactedAnchor {
                statement_id,
                entry_id,
            } => write!(
                formatter,
                "statement `{statement_id}` applies an unadjusted original byte range to redacted derivative `{entry_id}`"
            ),
            Self::InvalidCitationAnchor {
                statement_id,
                entry_id,
                reason,
            } => write!(
                formatter,
                "invalid citation anchor from `{statement_id}` to `{entry_id}`: {reason}"
            ),
            Self::MalformedTargetUtf8 {
                entry_id,
                valid_up_to,
            } => write!(
                formatter,
                "retained target bytes for `{entry_id}` are not valid UTF-8 (valid up to byte {valid_up_to})"
            ),
            Self::SensitiveConclusionRequiresRedaction {
                statement_id,
                entry_id,
            } => write!(
                formatter,
                "statement `{statement_id}` depends on unexportable or redacted evidence `{entry_id}` and must be removed or redacted before export"
            ),
            Self::AllocationFailed => write!(formatter, "fallible memory allocation failed"),
            Self::Stopped(reason) => write!(formatter, "report operation stopped: {reason:?}"),
        }
    }
}

impl Error for ReportError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReportSection {
    QuestionAndScope,
    Coverage,
    Chronology,
    SourceLineage,
    ContextAndMedia,
    Hypotheses,
    SupportingEvidence,
    ConflictingEvidence,
    BoundedConclusions,
    OpenQuestions,
}

impl ReportSection {
    pub const ALL: [Self; 10] = [
        Self::QuestionAndScope,
        Self::Coverage,
        Self::Chronology,
        Self::SourceLineage,
        Self::ContextAndMedia,
        Self::Hypotheses,
        Self::SupportingEvidence,
        Self::ConflictingEvidence,
        Self::BoundedConclusions,
        Self::OpenQuestions,
    ];

    pub fn heading(self) -> &'static str {
        match self {
            Self::QuestionAndScope => "Question and Scope",
            Self::Coverage => "Coverage",
            Self::Chronology => "Chronology",
            Self::SourceLineage => "Source Lineage",
            Self::ContextAndMedia => "Relevant Context and Media",
            Self::Hypotheses => "Hypotheses",
            Self::SupportingEvidence => "Supporting Evidence",
            Self::ConflictingEvidence => "Conflicting Evidence",
            Self::BoundedConclusions => "Bounded Conclusions",
            Self::OpenQuestions => "Open Questions",
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Self::QuestionAndScope => "question-and-scope",
            Self::Coverage => "coverage",
            Self::Chronology => "chronology",
            Self::SourceLineage => "source-lineage",
            Self::ContextAndMedia => "context-and-media",
            Self::Hypotheses => "hypotheses",
            Self::SupportingEvidence => "supporting-evidence",
            Self::ConflictingEvidence => "conflicting-evidence",
            Self::BoundedConclusions => "bounded-conclusions",
            Self::OpenQuestions => "open-questions",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OriginClass {
    Observation,
    SourceAssertion,
    Assumption,
    Inference,
    Forecast,
    Generated,
    Simulated,
}

impl OriginClass {
    pub fn label(self) -> &'static str {
        match self {
            Self::Observation => "OBSERVATION",
            Self::SourceAssertion => "SOURCE ASSERTION",
            Self::Assumption => "ASSUMPTION",
            Self::Inference => "INFERENCE",
            Self::Forecast => "FORECAST",
            Self::Generated => "GENERATED",
            Self::Simulated => "SIMULATED",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewStatus {
    Unreviewed,
    InReview,
    Reviewed,
    Challenged,
    Superseded,
}

impl ReviewStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unreviewed => "UNREVIEWED",
            Self::InReview => "IN REVIEW",
            Self::Reviewed => "REVIEWED",
            Self::Challenged => "CHALLENGED",
            Self::Superseded => "SUPERSEDED",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimePrecision<'input> {
    ExactInstant,
    Minute,
    Hour,
    DateOnly,
    ApproximateInterval,
    Unknown { reason: &'input str },
}

impl<'input> TimePrecision<'input> {
    pub fn label(self) -> &'static str {
        match self {
            Self::ExactInstant => "exact instant",
            Self::Minute => "minute precision",
            Self::Hour => "hour precision",
            Self::DateOnly => "date-only",
            Self::ApproximateInterval => "approximate interval",
            Self::Unknown { .. } => "unknown time",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimeDisplay<'input> {
    pub text: &'input str,
    pub precision: TimePrecision<'input>,
    pub time_reference: &'input str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReassessmentStatus<'input> {
    Current,
    NeedsReassessment {
        flagging_record_version_id: &'input str,
        reason: &'input str,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetAvailability {
    Available,
    ReferenceOnly,
    Missing,
    Restricted,
    Removed,
    Unsupported,
}

impl TargetAvailability {
    pub fn label(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::ReferenceOnly => "reference-only",
            Self::Missing => "missing",
            Self::Restricted => "restricted",
            Self::Removed => "removed",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OmissionReason {
    NotSelected,
    RetentionProhibited,
    RedistributionProhibited,
    Missing,
    Removed,
    Restricted,
    Unsupported,
}

impl OmissionReason {
    pub fn label(self) -> &'static str {
        match self {
            Self::NotSelected => "not selected",
            Self::RetentionProhibited => "retention prohibited",
            Self::RedistributionProhibited => "redistribution prohibited",
            Self::Missing => "missing",
            Self::Removed => "removed",
            Self::Restricted => "restricted",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecipientDisclosure {
    FullTarget,
    MetadataOnly,
    GenericCoverageOnly,
    ExistenceSensitive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvidenceInventoryEntry<'input> {
    pub entry_id: &'input str,
    pub citation_label: &'input str,
    pub source_id: &'input str,
    pub source_record_id: &'input str,
    pub source_record_version_id: &'input str,
    pub artifact_id: Option<&'input str>,
    pub artifact_version_id: Option<&'input str>,
    pub availability: TargetAvailability,
    pub omission_reason: Option<OmissionReason>,
    pub disclosure: RecipientDisclosure,
    pub retained_utf8_bytes: Option<&'input [u8]>,
    pub external_reference_url: Option<&'input str>,
    pub redacted_from_entry_id: Option<&'input str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CitationAnchor<'input> {
    Utf8ByteRange {
        start: usize,
        end: usize,
        is_unadjusted_original_offset: bool,
    },
    ExtractedPassage {
        extraction_version_id: &'input str,
        passage_line: usize,
        byte_start: usize,
        byte_end: usize,
    },
    PageRegion {
        page_number: usize,
        region_label: &'input str,
    },
    UnsupportedAnchor {
        kind: &'input str,
        limitation_note: &'input str,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StatementCitation<'input> {
    pub entry_id: &'input str,
    pub anchor: CitationAnchor<'input>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReportBlock<'input> {
    pub block_id: &'input str,
    pub statement_id: &'input str,
    pub section: ReportSection,
    pub actor_attribution: &'input str,
    pub origin_class: OriginClass,
    pub review_status: ReviewStatus,
    pub chronology_time: Option<TimeDisplay<'input>>,
    pub uncertainty_note: Option<&'input str>,
    pub source_limitation_note: Option<&'input str>,
    pub text: &'input str,
    pub citations: &'input [StatementCitation<'input>],
    pub sensitive_dependency_entry_ids: &'input [&'input str],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReportSnapshot<'input> {
    pub report_id: &'input str,
    pub report_version_id: &'input str,
    pub case_id: &'input str,
    pub case_version_id: &'input str,
    pub title: &'input str,
    pub snapshot_created_at: TimeDisplay<'input>,
    pub reassessment_status: ReassessmentStatus<'input>,
    pub coverage_summary: &'input str,
    pub redaction_policy_id: &'input str,
    pub generic_omission_note: Option<&'input str>,
    pub evidence_entries: &'input [EvidenceInventoryEntry<'input>],
    pub blocks: &'input [ReportBlock<'input>],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AnchorValidationState<'input> {
    ValidatedUtf8Slice {
        start: usize,
        end: usize,
        quoted_text: &'input str,
    },
    UnvalidatedExtractedPassage {
        extraction_version_id: &'input str,
        passage_line: usize,
        byte_start: usize,
        byte_end: usize,
        availability: TargetAvailability,
    },
    UnvalidatedPageRegion {
        page_number: usize,
        region_label: &'input str,
        availability: TargetAvailability,
    },
    UnvalidatedTargetUnavailable {
        anchor_summary: String,
        availability: TargetAvailability,
        omission_reason: Option<OmissionReason>,
    },
    UnsupportedAnchorLimitation {
        kind: &'input str,
        limitation_note: &'input str,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedCitation<'input> {
    pub visible_entry_ordinal: usize,
    pub citation_label: &'input str,
    pub source_id: &'input str,
    pub source_record_id: &'input str,
    pub source_record_version_id: &'input str,
    pub artifact_id: Option<&'input str>,
    pub artifact_version_id: Option<&'input str>,
    pub availability: TargetAvailability,
    pub omission_reason: Option<OmissionReason>,
    pub is_redacted_derivative: bool,
    pub visible_redacted_from_entry_id: Option<&'input str>,
    pub validation: AnchorValidationState<'input>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedBlock<'input> {
    pub block_ordinal: usize,
    pub block_id: &'input str,
    pub statement_id: &'input str,
    pub section: ReportSection,
    pub actor_attribution: &'input str,
    pub origin_class: OriginClass,
    pub review_status: ReviewStatus,
    pub chronology_time: Option<TimeDisplay<'input>>,
    pub uncertainty_note: Option<&'input str>,
    pub source_limitation_note: Option<&'input str>,
    pub text: &'input str,
    pub citations: Vec<VerifiedCitation<'input>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VisibleEvidenceItem<'input> {
    pub ordinal: usize,
    pub entry_id: &'input str,
    pub citation_label: &'input str,
    pub source_id: &'input str,
    pub source_record_id: &'input str,
    pub source_record_version_id: &'input str,
    pub artifact_id: Option<&'input str>,
    pub artifact_version_id: Option<&'input str>,
    pub availability: TargetAvailability,
    pub omission_reason: Option<OmissionReason>,
    pub disclosure: RecipientDisclosure,
    pub external_reference_url: Option<&'input str>,
    pub is_redacted_derivative: bool,
    pub visible_redacted_from_entry_id: Option<&'input str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RedactionPreviewCounts {
    pub included_full_targets: usize,
    pub reference_only_targets: usize,
    pub omitted_visible_targets: usize,
    pub generic_coverage_omissions: usize,
    pub validated_citations: usize,
    pub unvalidated_citations: usize,
    pub unsupported_anchor_citations: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedReportProjection<'input> {
    report_id: &'input str,
    report_version_id: &'input str,
    case_id: &'input str,
    case_version_id: &'input str,
    title: &'input str,
    snapshot_created_at: TimeDisplay<'input>,
    reassessment_status: ReassessmentStatus<'input>,
    coverage_summary: &'input str,
    redaction_policy_id: &'input str,
    generic_omission_note: Option<&'input str>,
    visible_inventory: Vec<VisibleEvidenceItem<'input>>,
    blocks: Vec<VerifiedBlock<'input>>,
    counts: RedactionPreviewCounts,
    limits: ReportLimits,
}

impl<'input> VerifiedReportProjection<'input> {
    pub fn report_id(&self) -> &'input str {
        self.report_id
    }

    pub fn report_version_id(&self) -> &'input str {
        self.report_version_id
    }

    pub fn case_id(&self) -> &'input str {
        self.case_id
    }

    pub fn case_version_id(&self) -> &'input str {
        self.case_version_id
    }

    pub fn title(&self) -> &'input str {
        self.title
    }

    pub fn snapshot_created_at(&self) -> TimeDisplay<'input> {
        self.snapshot_created_at
    }

    pub fn reassessment_status(&self) -> ReassessmentStatus<'input> {
        self.reassessment_status
    }

    pub fn coverage_summary(&self) -> &'input str {
        self.coverage_summary
    }

    pub fn redaction_policy_id(&self) -> &'input str {
        self.redaction_policy_id
    }

    pub fn generic_omission_note(&self) -> Option<&'input str> {
        self.generic_omission_note
    }

    pub fn visible_inventory(&self) -> &[VisibleEvidenceItem<'input>] {
        &self.visible_inventory
    }

    pub fn blocks(&self) -> &[VerifiedBlock<'input>] {
        &self.blocks
    }

    pub fn counts(&self) -> RedactionPreviewCounts {
        self.counts
    }

    pub fn limits(&self) -> ReportLimits {
        self.limits
    }
}

pub fn preview_report<'input>(
    snapshot: &ReportSnapshot<'input>,
    limits: ReportLimits,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<VerifiedReportProjection<'input>, ReportError> {
    check_control().map_err(ReportError::Stopped)?;

    if snapshot.evidence_entries.len() > limits.max_evidence_entries {
        return Err(ReportError::LimitExceeded {
            limit: Limit::EvidenceEntries,
            actual: snapshot.evidence_entries.len(),
            max: limits.max_evidence_entries,
        });
    }
    if snapshot.blocks.len() > limits.max_blocks {
        return Err(ReportError::LimitExceeded {
            limit: Limit::Blocks,
            actual: snapshot.blocks.len(),
            max: limits.max_blocks,
        });
    }

    validate_non_empty(snapshot.report_id, "report_id")?;
    validate_non_empty(snapshot.report_version_id, "report_version_id")?;
    validate_non_empty(snapshot.case_id, "case_id")?;
    validate_non_empty(snapshot.case_version_id, "case_version_id")?;
    validate_non_empty(snapshot.title, "title")?;
    validate_non_empty(snapshot.coverage_summary, "coverage_summary")?;
    validate_non_empty(snapshot.redaction_policy_id, "redaction_policy_id")?;

    let mut cumulative_text_bytes = 0usize;
    add_text_budget(
        &mut cumulative_text_bytes,
        snapshot.report_id,
        "report_id",
        limits,
    )?;
    add_text_budget(
        &mut cumulative_text_bytes,
        snapshot.report_version_id,
        "report_version_id",
        limits,
    )?;
    add_text_budget(
        &mut cumulative_text_bytes,
        snapshot.case_id,
        "case_id",
        limits,
    )?;
    add_text_budget(
        &mut cumulative_text_bytes,
        snapshot.case_version_id,
        "case_version_id",
        limits,
    )?;
    add_text_budget(&mut cumulative_text_bytes, snapshot.title, "title", limits)?;
    add_text_budget(
        &mut cumulative_text_bytes,
        snapshot.coverage_summary,
        "coverage_summary",
        limits,
    )?;
    add_text_budget(
        &mut cumulative_text_bytes,
        snapshot.redaction_policy_id,
        "redaction_policy_id",
        limits,
    )?;
    if let Some(note) = snapshot.generic_omission_note {
        validate_non_empty(note, "generic_omission_note")?;
        add_text_budget(
            &mut cumulative_text_bytes,
            note,
            "generic_omission_note",
            limits,
        )?;
    }

    validate_time_display(
        snapshot.snapshot_created_at,
        "snapshot_created_at",
        &mut cumulative_text_bytes,
        limits,
    )?;

    if let ReassessmentStatus::NeedsReassessment {
        flagging_record_version_id,
        reason,
    } = snapshot.reassessment_status
    {
        validate_non_empty(
            flagging_record_version_id,
            "reassessment_status.flagging_record_version_id",
        )?;
        validate_non_empty(reason, "reassessment_status.reason")?;
        add_text_budget(
            &mut cumulative_text_bytes,
            flagging_record_version_id,
            "reassessment_status.flagging_record_version_id",
            limits,
        )?;
        add_text_budget(
            &mut cumulative_text_bytes,
            reason,
            "reassessment_status.reason",
            limits,
        )?;
    }

    let mut visible_inventory = Vec::new();
    visible_inventory
        .try_reserve(snapshot.evidence_entries.len())
        .map_err(|_| ReportError::AllocationFailed)?;

    let mut entry_ordinals: Vec<Option<usize>> = Vec::new();
    entry_ordinals
        .try_reserve(snapshot.evidence_entries.len())
        .map_err(|_| ReportError::AllocationFailed)?;

    let mut included_full_targets = 0usize;
    let mut reference_only_targets = 0usize;
    let mut omitted_visible_targets = 0usize;
    let mut generic_coverage_omissions = 0usize;

    for (idx, entry) in snapshot.evidence_entries.iter().enumerate() {
        check_control().map_err(ReportError::Stopped)?;
        validate_non_empty(entry.entry_id, "evidence_entries.entry_id")?;
        validate_non_empty(entry.citation_label, "evidence_entries.citation_label")?;
        validate_non_empty(entry.source_id, "evidence_entries.source_id")?;
        validate_non_empty(entry.source_record_id, "evidence_entries.source_record_id")?;
        validate_non_empty(
            entry.source_record_version_id,
            "evidence_entries.source_record_version_id",
        )?;

        add_text_budget(
            &mut cumulative_text_bytes,
            entry.entry_id,
            "evidence_entries.entry_id",
            limits,
        )?;
        add_text_budget(
            &mut cumulative_text_bytes,
            entry.citation_label,
            "evidence_entries.citation_label",
            limits,
        )?;
        add_text_budget(
            &mut cumulative_text_bytes,
            entry.source_id,
            "evidence_entries.source_id",
            limits,
        )?;
        add_text_budget(
            &mut cumulative_text_bytes,
            entry.source_record_id,
            "evidence_entries.source_record_id",
            limits,
        )?;
        add_text_budget(
            &mut cumulative_text_bytes,
            entry.source_record_version_id,
            "evidence_entries.source_record_version_id",
            limits,
        )?;

        if let Some(artifact_id) = entry.artifact_id {
            validate_non_empty(artifact_id, "evidence_entries.artifact_id")?;
            add_text_budget(
                &mut cumulative_text_bytes,
                artifact_id,
                "evidence_entries.artifact_id",
                limits,
            )?;
        }
        if let Some(artifact_version_id) = entry.artifact_version_id {
            validate_non_empty(artifact_version_id, "evidence_entries.artifact_version_id")?;
            add_text_budget(
                &mut cumulative_text_bytes,
                artifact_version_id,
                "evidence_entries.artifact_version_id",
                limits,
            )?;
        }

        let (is_redacted_derivative, visible_redacted_from_entry_id) = if let Some(redacted_from) =
            entry.redacted_from_entry_id
        {
            validate_non_empty(redacted_from, "evidence_entries.redacted_from_entry_id")?;
            add_text_budget(
                &mut cumulative_text_bytes,
                redacted_from,
                "evidence_entries.redacted_from_entry_id",
                limits,
            )?;
            if redacted_from == entry.entry_id {
                return Err(ReportError::DisclosurePolicyViolation {
                    entry_id: make_owned_str(entry.entry_id)?,
                    reason: "redacted derivative must have a distinct entry_id from its original",
                });
            }
            let Some(orig_entry) = snapshot
                .evidence_entries
                .iter()
                .find(|candidate| candidate.entry_id == redacted_from)
            else {
                return Err(ReportError::DisclosurePolicyViolation {
                    entry_id: make_owned_str(entry.entry_id)?,
                    reason: "redacted_from_entry_id must reference a declared evidence entry in the snapshot",
                });
            };
            if orig_entry.disclosure == RecipientDisclosure::FullTarget {
                return Err(ReportError::DisclosurePolicyViolation {
                    entry_id: make_owned_str(entry.entry_id)?,
                    reason: "excluded original entry must not be disclosed as FullTarget beside its redacted derivative",
                });
            }
            let visible_origin = if orig_entry.disclosure == RecipientDisclosure::MetadataOnly {
                Some(redacted_from)
            } else {
                None
            };
            (true, visible_origin)
        } else {
            (false, None)
        };

        if let Some(prior_slice) = snapshot.evidence_entries.get(..idx) {
            for prior in prior_slice {
                if prior.entry_id == entry.entry_id {
                    return Err(ReportError::DuplicateIdentifier {
                        kind: "evidence_entry",
                        id: make_owned_str(entry.entry_id)?,
                    });
                }
                if prior.citation_label == entry.citation_label {
                    return Err(ReportError::DuplicateIdentifier {
                        kind: "citation_label",
                        id: make_owned_str(entry.citation_label)?,
                    });
                }
            }
        }

        if let Some(url) = entry.external_reference_url {
            add_text_budget(
                &mut cumulative_text_bytes,
                url,
                "evidence_entries.external_reference_url",
                limits,
            )?;
            validate_external_url(entry.entry_id, url)?;
        }

        if let Some(bytes) = entry.retained_utf8_bytes {
            cumulative_text_bytes = cumulative_text_bytes.saturating_add(bytes.len());
            if cumulative_text_bytes > limits.max_text_bytes {
                return Err(ReportError::LimitExceeded {
                    limit: Limit::TextBytes,
                    actual: cumulative_text_bytes,
                    max: limits.max_text_bytes,
                });
            }
            std::str::from_utf8(bytes).map_err(|err| ReportError::MalformedTargetUtf8 {
                entry_id: make_owned_str(entry.entry_id).unwrap_or_default(),
                valid_up_to: err.valid_up_to(),
            })?;
        }

        match entry.disclosure {
            RecipientDisclosure::FullTarget => {
                if entry.availability != TargetAvailability::Available {
                    return Err(ReportError::DisclosurePolicyViolation {
                        entry_id: make_owned_str(entry.entry_id)?,
                        reason: "FullTarget disclosure requires Available target availability",
                    });
                }
                if entry.omission_reason.is_some() {
                    return Err(ReportError::DisclosurePolicyViolation {
                        entry_id: make_owned_str(entry.entry_id)?,
                        reason: "Available FullTarget entry must not declare an omission_reason",
                    });
                }
                if entry.retained_utf8_bytes.is_none() && entry.artifact_id.is_none() {
                    return Err(ReportError::DisclosurePolicyViolation {
                        entry_id: make_owned_str(entry.entry_id)?,
                        reason: "FullTarget entry must supply retained_utf8_bytes or an artifact_id",
                    });
                }
                included_full_targets = included_full_targets.saturating_add(1);
                let ordinal = visible_inventory.len().saturating_add(1);
                visible_inventory.push(VisibleEvidenceItem {
                    ordinal,
                    entry_id: entry.entry_id,
                    citation_label: entry.citation_label,
                    source_id: entry.source_id,
                    source_record_id: entry.source_record_id,
                    source_record_version_id: entry.source_record_version_id,
                    artifact_id: entry.artifact_id,
                    artifact_version_id: entry.artifact_version_id,
                    availability: entry.availability,
                    omission_reason: None,
                    disclosure: entry.disclosure,
                    external_reference_url: entry.external_reference_url,
                    is_redacted_derivative,
                    visible_redacted_from_entry_id,
                });
                entry_ordinals.push(Some(ordinal));
            }
            RecipientDisclosure::MetadataOnly => {
                if entry.retained_utf8_bytes.is_some() {
                    return Err(ReportError::DisclosurePolicyViolation {
                        entry_id: make_owned_str(entry.entry_id)?,
                        reason: "MetadataOnly entry must not bundle retained target bytes",
                    });
                }
                if entry.omission_reason.is_none() {
                    return Err(ReportError::DisclosurePolicyViolation {
                        entry_id: make_owned_str(entry.entry_id)?,
                        reason: "MetadataOnly entry must state an explicit omission_reason",
                    });
                }
                if entry.availability == TargetAvailability::ReferenceOnly {
                    reference_only_targets = reference_only_targets.saturating_add(1);
                } else {
                    omitted_visible_targets = omitted_visible_targets.saturating_add(1);
                }
                let ordinal = visible_inventory.len().saturating_add(1);
                visible_inventory.push(VisibleEvidenceItem {
                    ordinal,
                    entry_id: entry.entry_id,
                    citation_label: entry.citation_label,
                    source_id: entry.source_id,
                    source_record_id: entry.source_record_id,
                    source_record_version_id: entry.source_record_version_id,
                    artifact_id: entry.artifact_id,
                    artifact_version_id: entry.artifact_version_id,
                    availability: entry.availability,
                    omission_reason: entry.omission_reason,
                    disclosure: entry.disclosure,
                    external_reference_url: entry.external_reference_url,
                    is_redacted_derivative,
                    visible_redacted_from_entry_id,
                });
                entry_ordinals.push(Some(ordinal));
            }
            RecipientDisclosure::GenericCoverageOnly => {
                if entry.retained_utf8_bytes.is_some() || entry.external_reference_url.is_some() {
                    return Err(ReportError::DisclosurePolicyViolation {
                        entry_id: make_owned_str(entry.entry_id)?,
                        reason: "GenericCoverageOnly entry must not bundle bytes or external URLs",
                    });
                }
                generic_coverage_omissions = generic_coverage_omissions.saturating_add(1);
                entry_ordinals.push(None);
            }
            RecipientDisclosure::ExistenceSensitive => {
                if entry.retained_utf8_bytes.is_some() || entry.external_reference_url.is_some() {
                    return Err(ReportError::DisclosurePolicyViolation {
                        entry_id: make_owned_str(entry.entry_id)?,
                        reason: "ExistenceSensitive entry must not bundle bytes or external URLs",
                    });
                }
                entry_ordinals.push(None);
            }
        }
    }

    let mut verified_blocks = Vec::new();
    verified_blocks
        .try_reserve(snapshot.blocks.len())
        .map_err(|_| ReportError::AllocationFailed)?;

    let mut total_citations = 0usize;
    let mut validated_citations = 0usize;
    let mut unvalidated_citations = 0usize;
    let mut unsupported_anchor_citations = 0usize;

    for (block_idx, block) in snapshot.blocks.iter().enumerate() {
        check_control().map_err(ReportError::Stopped)?;
        validate_non_empty(block.block_id, "blocks.block_id")?;
        validate_non_empty(block.statement_id, "blocks.statement_id")?;
        validate_non_empty(block.actor_attribution, "blocks.actor_attribution")?;
        validate_non_empty(block.text, "blocks.text")?;

        add_text_budget(
            &mut cumulative_text_bytes,
            block.block_id,
            "blocks.block_id",
            limits,
        )?;
        add_text_budget(
            &mut cumulative_text_bytes,
            block.statement_id,
            "blocks.statement_id",
            limits,
        )?;
        add_text_budget(
            &mut cumulative_text_bytes,
            block.actor_attribution,
            "blocks.actor_attribution",
            limits,
        )?;
        add_text_budget(
            &mut cumulative_text_bytes,
            block.text,
            "blocks.text",
            limits,
        )?;

        if let Some(prior_blocks) = snapshot.blocks.get(..block_idx) {
            for prior in prior_blocks {
                if prior.block_id == block.block_id {
                    return Err(ReportError::DuplicateIdentifier {
                        kind: "block_id",
                        id: make_owned_str(block.block_id)?,
                    });
                }
                if prior.statement_id == block.statement_id {
                    return Err(ReportError::DuplicateIdentifier {
                        kind: "statement_id",
                        id: make_owned_str(block.statement_id)?,
                    });
                }
            }
        }

        if let Some(chronology_time) = block.chronology_time {
            validate_time_display(
                chronology_time,
                "blocks.chronology_time",
                &mut cumulative_text_bytes,
                limits,
            )?;
        } else if block.section == ReportSection::Chronology {
            return Err(ReportError::InvalidTimeDisplay {
                field: "blocks.chronology_time",
                reason: "chronology section block must declare an explicit TimeDisplay",
            });
        }

        if let Some(uncertainty) = block.uncertainty_note {
            validate_non_empty(uncertainty, "blocks.uncertainty_note")?;
            add_text_budget(
                &mut cumulative_text_bytes,
                uncertainty,
                "blocks.uncertainty_note",
                limits,
            )?;
        }
        if let Some(limitation) = block.source_limitation_note {
            validate_non_empty(limitation, "blocks.source_limitation_note")?;
            add_text_budget(
                &mut cumulative_text_bytes,
                limitation,
                "blocks.source_limitation_note",
                limits,
            )?;
        }

        for dep_id in block.sensitive_dependency_entry_ids {
            validate_non_empty(dep_id, "blocks.sensitive_dependency_entry_ids")?;
            let Some(dep_entry) = snapshot
                .evidence_entries
                .iter()
                .find(|entry| entry.entry_id == *dep_id)
            else {
                return Err(ReportError::UnexplainedDanglingReference {
                    statement_id: make_owned_str(block.statement_id)?,
                    entry_id: make_owned_str(dep_id)?,
                });
            };
            if dep_entry.disclosure != RecipientDisclosure::FullTarget
                || dep_entry.availability != TargetAvailability::Available
            {
                return Err(ReportError::SensitiveConclusionRequiresRedaction {
                    statement_id: make_owned_str(block.statement_id)?,
                    entry_id: make_owned_str(dep_id)?,
                });
            }
        }

        let prospective_citations = total_citations.saturating_add(block.citations.len());
        if prospective_citations > limits.max_citations {
            return Err(ReportError::LimitExceeded {
                limit: Limit::Citations,
                actual: total_citations
                    .saturating_add(1)
                    .max(limits.max_citations.saturating_add(1)),
                max: limits.max_citations,
            });
        }

        let mut block_citations = Vec::new();
        block_citations
            .try_reserve(block.citations.len())
            .map_err(|_| ReportError::AllocationFailed)?;

        for citation in block.citations {
            check_control().map_err(ReportError::Stopped)?;
            total_citations = total_citations.saturating_add(1);
            if total_citations > limits.max_citations {
                return Err(ReportError::LimitExceeded {
                    limit: Limit::Citations,
                    actual: total_citations,
                    max: limits.max_citations,
                });
            }

            let Some((entry_index, target_entry)) = snapshot
                .evidence_entries
                .iter()
                .enumerate()
                .find(|(_, entry)| entry.entry_id == citation.entry_id)
            else {
                return Err(ReportError::UnexplainedDanglingReference {
                    statement_id: make_owned_str(block.statement_id)?,
                    entry_id: make_owned_str(citation.entry_id)?,
                });
            };

            let Some(Some(visible_ordinal)) = entry_ordinals.get(entry_index).copied() else {
                return Err(ReportError::ExistenceSensitiveReferenceLeak {
                    statement_id: make_owned_str(block.statement_id)?,
                    entry_id: make_owned_str(citation.entry_id)?,
                });
            };

            let visible_item = visible_inventory
                .get(visible_ordinal.saturating_sub(1))
                .ok_or(ReportError::AllocationFailed)?;

            let validation = validate_citation_anchor(
                block.statement_id,
                target_entry,
                citation.anchor,
                &mut cumulative_text_bytes,
                limits,
            )?;

            match &validation {
                AnchorValidationState::ValidatedUtf8Slice { .. } => {
                    validated_citations = validated_citations.saturating_add(1);
                }
                AnchorValidationState::UnvalidatedExtractedPassage { .. }
                | AnchorValidationState::UnvalidatedPageRegion { .. }
                | AnchorValidationState::UnvalidatedTargetUnavailable { .. } => {
                    unvalidated_citations = unvalidated_citations.saturating_add(1);
                }
                AnchorValidationState::UnsupportedAnchorLimitation { .. } => {
                    unsupported_anchor_citations = unsupported_anchor_citations.saturating_add(1);
                }
            }

            block_citations.push(VerifiedCitation {
                visible_entry_ordinal: visible_ordinal,
                citation_label: target_entry.citation_label,
                source_id: target_entry.source_id,
                source_record_id: target_entry.source_record_id,
                source_record_version_id: target_entry.source_record_version_id,
                artifact_id: target_entry.artifact_id,
                artifact_version_id: target_entry.artifact_version_id,
                availability: target_entry.availability,
                omission_reason: target_entry.omission_reason,
                is_redacted_derivative: visible_item.is_redacted_derivative,
                visible_redacted_from_entry_id: visible_item.visible_redacted_from_entry_id,
                validation,
            });
        }

        verified_blocks.push(VerifiedBlock {
            block_ordinal: block_idx.saturating_add(1),
            block_id: block.block_id,
            statement_id: block.statement_id,
            section: block.section,
            actor_attribution: block.actor_attribution,
            origin_class: block.origin_class,
            review_status: block.review_status,
            chronology_time: block.chronology_time,
            uncertainty_note: block.uncertainty_note,
            source_limitation_note: block.source_limitation_note,
            text: block.text,
            citations: block_citations,
        });
    }

    check_control().map_err(ReportError::Stopped)?;

    Ok(VerifiedReportProjection {
        report_id: snapshot.report_id,
        report_version_id: snapshot.report_version_id,
        case_id: snapshot.case_id,
        case_version_id: snapshot.case_version_id,
        title: snapshot.title,
        snapshot_created_at: snapshot.snapshot_created_at,
        reassessment_status: snapshot.reassessment_status,
        coverage_summary: snapshot.coverage_summary,
        redaction_policy_id: snapshot.redaction_policy_id,
        generic_omission_note: snapshot.generic_omission_note,
        visible_inventory,
        blocks: verified_blocks,
        counts: RedactionPreviewCounts {
            included_full_targets,
            reference_only_targets,
            omitted_visible_targets,
            generic_coverage_omissions,
            validated_citations,
            unvalidated_citations,
            unsupported_anchor_citations,
        },
        limits,
    })
}

pub fn render_markdown(
    projection: &VerifiedReportProjection<'_>,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<String, ReportError> {
    check_control().map_err(ReportError::Stopped)?;
    let max_out = projection.limits.max_output_bytes;
    let mut out = String::new();

    push_str_bounded(&mut out, "# ", max_out)?;
    push_markdown_escaped(&mut out, projection.title, max_out)?;
    push_str_bounded(&mut out, "\n\n", max_out)?;

    push_str_bounded(&mut out, "- **Report ID / Version:** ", max_out)?;
    push_markdown_code_pair(
        &mut out,
        projection.report_id,
        "@",
        projection.report_version_id,
        max_out,
    )?;
    push_str_bounded(&mut out, "\n- **Case ID / Version:** ", max_out)?;
    push_markdown_code_pair(
        &mut out,
        projection.case_id,
        "@",
        projection.case_version_id,
        max_out,
    )?;
    push_str_bounded(&mut out, "\n- **Snapshot Created:** ", max_out)?;
    push_time_display_markdown(&mut out, projection.snapshot_created_at, max_out)?;
    push_str_bounded(&mut out, "\n- **Redaction Policy:** ", max_out)?;
    push_markdown_code_span(&mut out, projection.redaction_policy_id, max_out)?;
    push_str_bounded(&mut out, "\n- **Reassessment Status:** ", max_out)?;
    match projection.reassessment_status {
        ReassessmentStatus::Current => {
            push_str_bounded(&mut out, "`[REASSESSMENT: CURRENT]`\n", max_out)?;
        }
        ReassessmentStatus::NeedsReassessment {
            flagging_record_version_id,
            reason,
        } => {
            push_str_bounded(&mut out, "`[REASSESSMENT REQUIRED]` flagged by ", max_out)?;
            push_markdown_code_span(&mut out, flagging_record_version_id, max_out)?;
            push_str_bounded(&mut out, " — ", max_out)?;
            push_markdown_escaped(&mut out, reason, max_out)?;
            push_str_bounded(&mut out, "\n", max_out)?;
        }
    }
    push_str_bounded(&mut out, "- **Coverage Summary:** ", max_out)?;
    push_markdown_escaped(&mut out, projection.coverage_summary, max_out)?;
    push_str_bounded(&mut out, "\n", max_out)?;
    if projection.generic_omission_note.is_some()
        || projection.counts.generic_coverage_omissions > 0
    {
        push_str_bounded(&mut out, "- **Omission Coverage Notice:** ", max_out)?;
        if let Some(generic_note) = projection.generic_omission_note {
            push_markdown_escaped(&mut out, generic_note, max_out)?;
            if projection.counts.generic_coverage_omissions > 0 {
                push_str_bounded(&mut out, " (", max_out)?;
                push_usize_bounded(
                    &mut out,
                    projection.counts.generic_coverage_omissions,
                    max_out,
                )?;
                push_str_bounded(
                    &mut out,
                    " generic-coverage omitted record(s) under recipient policy)",
                    max_out,
                )?;
            }
        } else {
            push_usize_bounded(
                &mut out,
                projection.counts.generic_coverage_omissions,
                max_out,
            )?;
            push_str_bounded(
                &mut out,
                " additional record(s) omitted under recipient policy (generic coverage only).",
                max_out,
            )?;
        }
        push_str_bounded(&mut out, "\n", max_out)?;
    }
    push_str_bounded(&mut out, "\n", max_out)?;

    for section in ReportSection::ALL {
        check_control().map_err(ReportError::Stopped)?;
        push_str_bounded(&mut out, "## ", max_out)?;
        push_str_bounded(&mut out, section.heading(), max_out)?;
        push_str_bounded(&mut out, "\n\n", max_out)?;

        let mut section_has_blocks = false;
        for block in &projection.blocks {
            if block.section != section {
                continue;
            }
            check_control().map_err(ReportError::Stopped)?;
            section_has_blocks = true;

            push_str_bounded(&mut out, "- **[ORIGIN: ", max_out)?;
            push_str_bounded(&mut out, block.origin_class.label(), max_out)?;
            push_str_bounded(&mut out, "] [REVIEW: ", max_out)?;
            push_str_bounded(&mut out, block.review_status.label(), max_out)?;
            push_str_bounded(&mut out, "]** (", max_out)?;
            push_markdown_code_span(&mut out, block.statement_id, max_out)?;
            push_str_bounded(&mut out, ", actor: ", max_out)?;
            push_markdown_code_span(&mut out, block.actor_attribution, max_out)?;
            push_str_bounded(&mut out, ")", max_out)?;

            if let Some(chronology_time) = block.chronology_time {
                push_str_bounded(&mut out, " [TIME: ", max_out)?;
                push_time_display_markdown(&mut out, chronology_time, max_out)?;
                push_str_bounded(&mut out, "]", max_out)?;
            }

            push_str_bounded(&mut out, ": ", max_out)?;
            push_markdown_escaped(&mut out, block.text, max_out)?;
            push_str_bounded(&mut out, "\n", max_out)?;

            if let Some(uncertainty) = block.uncertainty_note {
                push_str_bounded(&mut out, "  - **[UNCERTAINTY]:** ", max_out)?;
                push_markdown_escaped(&mut out, uncertainty, max_out)?;
                push_str_bounded(&mut out, "\n", max_out)?;
            }
            if let Some(limitation) = block.source_limitation_note {
                push_str_bounded(&mut out, "  - **[SOURCE LIMITATION]:** ", max_out)?;
                push_markdown_escaped(&mut out, limitation, max_out)?;
                push_str_bounded(&mut out, "\n", max_out)?;
            }

            if block.citations.is_empty() {
                push_str_bounded(&mut out, "  - **[CITATIONS]:** `[UNCITED]`\n", max_out)?;
            } else {
                for citation in &block.citations {
                    check_control().map_err(ReportError::Stopped)?;
                    push_str_bounded(&mut out, "  - **[CITATION ", max_out)?;
                    push_markdown_escaped(&mut out, citation.citation_label, max_out)?;
                    push_str_bounded(&mut out, "]:** source ", max_out)?;
                    push_markdown_code_span(&mut out, citation.source_id, max_out)?;
                    push_str_bounded(&mut out, " record ", max_out)?;
                    push_markdown_code_pair(
                        &mut out,
                        citation.source_record_id,
                        "@",
                        citation.source_record_version_id,
                        max_out,
                    )?;
                    if let Some(artifact_id) = citation.artifact_id {
                        push_str_bounded(&mut out, " artifact ", max_out)?;
                        if let Some(artifact_ver) = citation.artifact_version_id {
                            push_markdown_code_pair(
                                &mut out,
                                artifact_id,
                                "@",
                                artifact_ver,
                                max_out,
                            )?;
                        } else {
                            push_markdown_code_span(&mut out, artifact_id, max_out)?;
                        }
                    }
                    push_str_bounded(&mut out, " (availability: `", max_out)?;
                    push_str_bounded(&mut out, citation.availability.label(), max_out)?;
                    push_str_bounded(&mut out, "`)", max_out)?;
                    if citation.is_redacted_derivative {
                        if let Some(redacted_from) = citation.visible_redacted_from_entry_id {
                            push_str_bounded(&mut out, " (redacted derivative of ", max_out)?;
                            push_markdown_code_span(&mut out, redacted_from, max_out)?;
                            push_str_bounded(&mut out, ")", max_out)?;
                        } else {
                            push_str_bounded(&mut out, " (redacted derivative)", max_out)?;
                        }
                    }
                    push_str_bounded(&mut out, " — ", max_out)?;
                    push_anchor_validation_markdown(&mut out, &citation.validation, max_out)?;
                    push_str_bounded(&mut out, "\n", max_out)?;
                }
            }
        }

        if !section_has_blocks {
            push_str_bounded(
                &mut out,
                "_No material selected for this section._\n",
                max_out,
            )?;
        }
        push_str_bounded(&mut out, "\n", max_out)?;
    }

    push_str_bounded(&mut out, "## Evidence Inventory\n\n", max_out)?;
    if projection.visible_inventory.is_empty() {
        push_str_bounded(
            &mut out,
            "_No exportable evidence entries in this projection._\n",
            max_out,
        )?;
    } else {
        for item in &projection.visible_inventory {
            check_control().map_err(ReportError::Stopped)?;
            push_str_bounded(&mut out, "- **[", max_out)?;
            push_markdown_escaped(&mut out, item.citation_label, max_out)?;
            push_str_bounded(&mut out, "]** entry ", max_out)?;
            push_markdown_code_span(&mut out, item.entry_id, max_out)?;
            push_str_bounded(&mut out, " | source ", max_out)?;
            push_markdown_code_span(&mut out, item.source_id, max_out)?;
            push_str_bounded(&mut out, " | record ", max_out)?;
            push_markdown_code_pair(
                &mut out,
                item.source_record_id,
                "@",
                item.source_record_version_id,
                max_out,
            )?;
            push_str_bounded(&mut out, " | availability: `", max_out)?;
            push_str_bounded(&mut out, item.availability.label(), max_out)?;
            push_str_bounded(&mut out, "`", max_out)?;
            if let Some(reason) = item.omission_reason {
                push_str_bounded(&mut out, " | omission reason: `", max_out)?;
                push_str_bounded(&mut out, reason.label(), max_out)?;
                push_str_bounded(&mut out, "`", max_out)?;
            }
            if item.is_redacted_derivative {
                if let Some(redacted_from) = item.visible_redacted_from_entry_id {
                    push_str_bounded(&mut out, " | redacted derivative of ", max_out)?;
                    push_markdown_code_span(&mut out, redacted_from, max_out)?;
                } else {
                    push_str_bounded(&mut out, " | redacted derivative", max_out)?;
                }
            }
            if let Some(url) = item.external_reference_url {
                push_str_bounded(
                    &mut out,
                    " | external reference (requires reader activation): ",
                    max_out,
                )?;
                push_markdown_code_span(&mut out, url, max_out)?;
            }
            push_str_bounded(&mut out, "\n", max_out)?;
        }
    }

    check_control().map_err(ReportError::Stopped)?;
    Ok(out)
}

pub fn render_html(
    projection: &VerifiedReportProjection<'_>,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<String, ReportError> {
    check_control().map_err(ReportError::Stopped)?;
    let max_out = projection.limits.max_output_bytes;
    let mut out = String::new();

    push_str_bounded(
        &mut out,
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; img-src data:; form-action 'none'; base-uri 'none'\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>",
        max_out,
    )?;
    push_html_escaped(&mut out, projection.title, max_out)?;
    push_str_bounded(
        &mut out,
        "</title>\n<style>\n:root { color-scheme: dark; --bg: #0b1016; --panel: #121a24; --border: #283648; --text: #e6edf5; --muted: #9fb0c3; --accent: #6cb6ff; --warn: #f0b429; }\nbody { margin: 0; padding: 2rem; background: var(--bg); color: var(--text); font-family: ui-monospace, SFMono-Regular, Consolas, monospace; line-height: 1.5; }\nmain { max-width: 68rem; margin: 0 auto; }\nheader, section { background: var(--panel); border: 1px solid var(--border); padding: 1.25rem; margin-bottom: 1.25rem; }\nh1, h2 { margin-top: 0; letter-spacing: 0.04em; }\n.badge { display: inline-block; border: 1px solid var(--border); padding: 0.1rem 0.4rem; margin-right: 0.35rem; font-weight: 700; font-size: 0.85rem; }\n.banner-warn { border-left: 4px solid var(--warn); padding-left: 0.75rem; margin: 0.75rem 0; }\n.qualifier, .citation-entry { margin-top: 0.35rem; color: var(--muted); font-size: 0.92rem; }\na { color: var(--accent); text-decoration: underline; }\na:focus-visible { outline: 3px solid var(--warn); outline-offset: 2px; }\n@media (prefers-reduced-motion: reduce), (prefers-reduced-transparency: reduce) { * { transition: none !important; animation: none !important; backdrop-filter: none !important; } }\n@media print { :root { color-scheme: light; } body { background: #ffffff; color: #000000; padding: 0; } header, section { background: #ffffff; border: 1px solid #000000; page-break-inside: avoid; } .badge { border: 1px solid #000000; color: #000000; } .qualifier, .citation-entry, a { color: #000000; } }\n</style>\n</head>\n<body>\n<main>\n<header>\n<h1>",
        max_out,
    )?;
    push_html_escaped(&mut out, projection.title, max_out)?;
    push_str_bounded(
        &mut out,
        "</h1>\n<ul>\n<li><strong>Report ID / Version:</strong> <code>",
        max_out,
    )?;
    push_html_escaped(&mut out, projection.report_id, max_out)?;
    push_str_bounded(&mut out, "@", max_out)?;
    push_html_escaped(&mut out, projection.report_version_id, max_out)?;
    push_str_bounded(
        &mut out,
        "</code></li>\n<li><strong>Case ID / Version:</strong> <code>",
        max_out,
    )?;
    push_html_escaped(&mut out, projection.case_id, max_out)?;
    push_str_bounded(&mut out, "@", max_out)?;
    push_html_escaped(&mut out, projection.case_version_id, max_out)?;
    push_str_bounded(
        &mut out,
        "</code></li>\n<li><strong>Snapshot Created:</strong> ",
        max_out,
    )?;
    push_time_display_html(&mut out, projection.snapshot_created_at, max_out)?;
    push_str_bounded(
        &mut out,
        "</li>\n<li><strong>Redaction Policy:</strong> <code>",
        max_out,
    )?;
    push_html_escaped(&mut out, projection.redaction_policy_id, max_out)?;
    push_str_bounded(
        &mut out,
        "</code></li>\n<li><strong>Reassessment Status:</strong> ",
        max_out,
    )?;
    match projection.reassessment_status {
        ReassessmentStatus::Current => {
            push_str_bounded(
                &mut out,
                "<span class=\"badge\">[REASSESSMENT: CURRENT]</span></li>\n",
                max_out,
            )?;
        }
        ReassessmentStatus::NeedsReassessment {
            flagging_record_version_id,
            reason,
        } => {
            push_str_bounded(
                &mut out,
                "<div class=\"banner-warn\"><span class=\"badge\">[REASSESSMENT REQUIRED]</span> Flagged by <code>",
                max_out,
            )?;
            push_html_escaped(&mut out, flagging_record_version_id, max_out)?;
            push_str_bounded(&mut out, "</code>: ", max_out)?;
            push_html_escaped(&mut out, reason, max_out)?;
            push_str_bounded(&mut out, "</div></li>\n", max_out)?;
        }
    }
    push_str_bounded(&mut out, "<li><strong>Coverage Summary:</strong> ", max_out)?;
    push_html_escaped(&mut out, projection.coverage_summary, max_out)?;
    push_str_bounded(&mut out, "</li>\n", max_out)?;
    if projection.generic_omission_note.is_some()
        || projection.counts.generic_coverage_omissions > 0
    {
        push_str_bounded(
            &mut out,
            "<li><strong>Omission Coverage Notice:</strong> ",
            max_out,
        )?;
        if let Some(generic_note) = projection.generic_omission_note {
            push_html_escaped(&mut out, generic_note, max_out)?;
            if projection.counts.generic_coverage_omissions > 0 {
                push_str_bounded(&mut out, " (", max_out)?;
                push_usize_bounded(
                    &mut out,
                    projection.counts.generic_coverage_omissions,
                    max_out,
                )?;
                push_str_bounded(
                    &mut out,
                    " generic-coverage omitted record(s) under recipient policy)",
                    max_out,
                )?;
            }
        } else {
            push_usize_bounded(
                &mut out,
                projection.counts.generic_coverage_omissions,
                max_out,
            )?;
            push_str_bounded(
                &mut out,
                " additional record(s) omitted under recipient policy (generic coverage only).",
                max_out,
            )?;
        }
        push_str_bounded(&mut out, "</li>\n", max_out)?;
    }
    push_str_bounded(&mut out, "</ul>\n</header>\n", max_out)?;

    for section in ReportSection::ALL {
        check_control().map_err(ReportError::Stopped)?;
        push_str_bounded(&mut out, "<section id=\"sec-", max_out)?;
        push_str_bounded(&mut out, section.slug(), max_out)?;
        push_str_bounded(&mut out, "\">\n<h2>", max_out)?;
        push_str_bounded(&mut out, section.heading(), max_out)?;
        push_str_bounded(&mut out, "</h2>\n", max_out)?;

        let mut section_has_blocks = false;
        for block in &projection.blocks {
            if block.section != section {
                continue;
            }
            check_control().map_err(ReportError::Stopped)?;
            section_has_blocks = true;

            push_str_bounded(&mut out, "<article id=\"stmt-", max_out)?;
            push_usize_bounded(&mut out, block.block_ordinal, max_out)?;
            push_str_bounded(&mut out, "\">\n<p><span class=\"badge\">[ORIGIN: ", max_out)?;
            push_str_bounded(&mut out, block.origin_class.label(), max_out)?;
            push_str_bounded(&mut out, "]</span><span class=\"badge\">[REVIEW: ", max_out)?;
            push_str_bounded(&mut out, block.review_status.label(), max_out)?;
            push_str_bounded(&mut out, "]</span> <code>", max_out)?;
            push_html_escaped(&mut out, block.statement_id, max_out)?;
            push_str_bounded(&mut out, "</code> (actor: <code>", max_out)?;
            push_html_escaped(&mut out, block.actor_attribution, max_out)?;
            push_str_bounded(&mut out, "</code>)", max_out)?;

            if let Some(chronology_time) = block.chronology_time {
                push_str_bounded(&mut out, " <span class=\"badge\">[TIME: ", max_out)?;
                push_time_display_html(&mut out, chronology_time, max_out)?;
                push_str_bounded(&mut out, "]</span>", max_out)?;
            }

            push_str_bounded(&mut out, ": ", max_out)?;
            push_html_escaped(&mut out, block.text, max_out)?;
            push_str_bounded(&mut out, "</p>\n", max_out)?;

            if let Some(uncertainty) = block.uncertainty_note {
                push_str_bounded(
                    &mut out,
                    "<div class=\"qualifier\"><strong>[UNCERTAINTY]:</strong> ",
                    max_out,
                )?;
                push_html_escaped(&mut out, uncertainty, max_out)?;
                push_str_bounded(&mut out, "</div>\n", max_out)?;
            }
            if let Some(limitation) = block.source_limitation_note {
                push_str_bounded(
                    &mut out,
                    "<div class=\"qualifier\"><strong>[SOURCE LIMITATION]:</strong> ",
                    max_out,
                )?;
                push_html_escaped(&mut out, limitation, max_out)?;
                push_str_bounded(&mut out, "</div>\n", max_out)?;
            }

            if block.citations.is_empty() {
                push_str_bounded(
                    &mut out,
                    "<div class=\"citation-entry\"><strong>[CITATIONS]:</strong> <code>[UNCITED]</code></div>\n",
                    max_out,
                )?;
            } else {
                for citation in &block.citations {
                    check_control().map_err(ReportError::Stopped)?;
                    push_str_bounded(
                        &mut out,
                        "<div class=\"citation-entry\"><a href=\"#cite-",
                        max_out,
                    )?;
                    push_usize_bounded(&mut out, citation.visible_entry_ordinal, max_out)?;
                    push_str_bounded(&mut out, "\">[CITATION ", max_out)?;
                    push_html_escaped(&mut out, citation.citation_label, max_out)?;
                    push_str_bounded(&mut out, "]</a> source <code>", max_out)?;
                    push_html_escaped(&mut out, citation.source_id, max_out)?;
                    push_str_bounded(&mut out, "</code> record <code>", max_out)?;
                    push_html_escaped(&mut out, citation.source_record_id, max_out)?;
                    push_str_bounded(&mut out, "@", max_out)?;
                    push_html_escaped(&mut out, citation.source_record_version_id, max_out)?;
                    push_str_bounded(&mut out, "</code>", max_out)?;
                    if let Some(artifact_id) = citation.artifact_id {
                        push_str_bounded(&mut out, " artifact <code>", max_out)?;
                        push_html_escaped(&mut out, artifact_id, max_out)?;
                        if let Some(artifact_ver) = citation.artifact_version_id {
                            push_str_bounded(&mut out, "@", max_out)?;
                            push_html_escaped(&mut out, artifact_ver, max_out)?;
                        }
                        push_str_bounded(&mut out, "</code>", max_out)?;
                    }
                    push_str_bounded(&mut out, " (availability: <code>", max_out)?;
                    push_str_bounded(&mut out, citation.availability.label(), max_out)?;
                    push_str_bounded(&mut out, "</code>)", max_out)?;
                    if citation.is_redacted_derivative {
                        if let Some(redacted_from) = citation.visible_redacted_from_entry_id {
                            push_str_bounded(&mut out, " (redacted derivative of <code>", max_out)?;
                            push_html_escaped(&mut out, redacted_from, max_out)?;
                            push_str_bounded(&mut out, "</code>)", max_out)?;
                        } else {
                            push_str_bounded(&mut out, " (redacted derivative)", max_out)?;
                        }
                    }
                    push_str_bounded(&mut out, " — ", max_out)?;
                    push_anchor_validation_html(&mut out, &citation.validation, max_out)?;
                    push_str_bounded(&mut out, "</div>\n", max_out)?;
                }
            }
            push_str_bounded(&mut out, "</article>\n", max_out)?;
        }

        if !section_has_blocks {
            push_str_bounded(
                &mut out,
                "<p class=\"EmptySection\"><em>No material selected for this section.</em></p>\n",
                max_out,
            )?;
        }
        push_str_bounded(&mut out, "</section>\n", max_out)?;
    }

    push_str_bounded(
        &mut out,
        "<section id=\"sec-evidence-inventory\">\n<h2>Evidence Inventory</h2>\n",
        max_out,
    )?;
    if projection.visible_inventory.is_empty() {
        push_str_bounded(
            &mut out,
            "<p><em>No exportable evidence entries in this projection.</em></p>\n",
            max_out,
        )?;
    } else {
        push_str_bounded(&mut out, "<ul>\n", max_out)?;
        for item in &projection.visible_inventory {
            check_control().map_err(ReportError::Stopped)?;
            push_str_bounded(&mut out, "<li id=\"cite-", max_out)?;
            push_usize_bounded(&mut out, item.ordinal, max_out)?;
            push_str_bounded(&mut out, "\"><strong>[", max_out)?;
            push_html_escaped(&mut out, item.citation_label, max_out)?;
            push_str_bounded(&mut out, "]</strong> entry <code>", max_out)?;
            push_html_escaped(&mut out, item.entry_id, max_out)?;
            push_str_bounded(&mut out, "</code> | source <code>", max_out)?;
            push_html_escaped(&mut out, item.source_id, max_out)?;
            push_str_bounded(&mut out, "</code> | record <code>", max_out)?;
            push_html_escaped(&mut out, item.source_record_id, max_out)?;
            push_str_bounded(&mut out, "@", max_out)?;
            push_html_escaped(&mut out, item.source_record_version_id, max_out)?;
            push_str_bounded(&mut out, "</code> | availability: <code>", max_out)?;
            push_str_bounded(&mut out, item.availability.label(), max_out)?;
            push_str_bounded(&mut out, "</code>", max_out)?;
            if let Some(reason) = item.omission_reason {
                push_str_bounded(&mut out, " | omission reason: <code>", max_out)?;
                push_str_bounded(&mut out, reason.label(), max_out)?;
                push_str_bounded(&mut out, "</code>", max_out)?;
            }
            if item.is_redacted_derivative {
                if let Some(redacted_from) = item.visible_redacted_from_entry_id {
                    push_str_bounded(&mut out, " | redacted derivative of <code>", max_out)?;
                    push_html_escaped(&mut out, redacted_from, max_out)?;
                    push_str_bounded(&mut out, "</code>", max_out)?;
                } else {
                    push_str_bounded(&mut out, " | redacted derivative", max_out)?;
                }
            }
            if let Some(url) = item.external_reference_url {
                push_str_bounded(&mut out, " | <a href=\"", max_out)?;
                push_html_escaped(&mut out, url, max_out)?;
                push_str_bounded(
                    &mut out,
                    "\" rel=\"noopener noreferrer external\">external reference (requires reader activation)</a>: <code>",
                    max_out,
                )?;
                push_html_escaped(&mut out, url, max_out)?;
                push_str_bounded(&mut out, "</code>", max_out)?;
            }
            push_str_bounded(&mut out, "</li>\n", max_out)?;
        }
        push_str_bounded(&mut out, "</ul>\n", max_out)?;
    }
    push_str_bounded(&mut out, "</section>\n</main>\n</body>\n</html>\n", max_out)?;

    check_control().map_err(ReportError::Stopped)?;
    Ok(out)
}

fn validate_non_empty(value: &str, field: &'static str) -> Result<(), ReportError> {
    if value.trim().is_empty() {
        return Err(ReportError::EmptyIdentifier { field });
    }
    if contains_local_filesystem_path(value) {
        return Err(ReportError::LocalPathDisclosed { field });
    }
    Ok(())
}

fn add_text_budget(
    cumulative: &mut usize,
    value: &str,
    field: &'static str,
    limits: ReportLimits,
) -> Result<(), ReportError> {
    if contains_local_filesystem_path(value) {
        return Err(ReportError::LocalPathDisclosed { field });
    }
    *cumulative = cumulative.saturating_add(value.len());
    if *cumulative > limits.max_text_bytes {
        return Err(ReportError::LimitExceeded {
            limit: Limit::TextBytes,
            actual: *cumulative,
            max: limits.max_text_bytes,
        });
    }
    Ok(())
}

fn validate_time_display(
    time: TimeDisplay<'_>,
    field: &'static str,
    cumulative: &mut usize,
    limits: ReportLimits,
) -> Result<(), ReportError> {
    validate_non_empty(time.time_reference, field)?;
    add_text_budget(cumulative, time.time_reference, field, limits)?;
    match time.precision {
        TimePrecision::Unknown { reason } => {
            if reason.trim().is_empty() {
                return Err(ReportError::InvalidTimeDisplay {
                    field,
                    reason: "Unknown time precision requires a non-empty reason",
                });
            }
            add_text_budget(cumulative, reason, field, limits)?;
            if !time.text.is_empty() {
                add_text_budget(cumulative, time.text, field, limits)?;
            }
        }
        TimePrecision::DateOnly => {
            if time.text.trim().is_empty() {
                return Err(ReportError::InvalidTimeDisplay {
                    field,
                    reason: "DateOnly time display text must not be empty",
                });
            }
            if time.text.contains('T') || time.text.contains(':') {
                return Err(ReportError::InvalidTimeDisplay {
                    field,
                    reason: "DateOnly source must not be expanded into a clock/midnight timestamp",
                });
            }
            add_text_budget(cumulative, time.text, field, limits)?;
        }
        TimePrecision::ExactInstant
        | TimePrecision::Minute
        | TimePrecision::Hour
        | TimePrecision::ApproximateInterval => {
            if time.text.trim().is_empty() {
                return Err(ReportError::InvalidTimeDisplay {
                    field,
                    reason: "time display text must not be empty",
                });
            }
            add_text_budget(cumulative, time.text, field, limits)?;
        }
    }
    Ok(())
}

fn contains_local_filesystem_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    if value.contains("file://")
        || value.contains("\\\\")
        || value.contains("/Users/")
        || value.contains("/home/")
        || value.contains("/var/folders/")
        || value.contains("/tmp/")
        || value.contains("\\Users\\")
        || value.contains("\\Windows\\")
    {
        return true;
    }
    let mut idx = 0usize;
    while idx.saturating_add(2) < bytes.len() {
        if let (Some(&first), Some(&second), Some(&third)) = (
            bytes.get(idx),
            bytes.get(idx.saturating_add(1)),
            bytes.get(idx.saturating_add(2)),
        ) {
            let prev_is_boundary = idx == 0
                || bytes
                    .get(idx.saturating_sub(1))
                    .is_none_or(|b| !b.is_ascii_alphanumeric());
            if prev_is_boundary
                && first.is_ascii_alphabetic()
                && second == b':'
                && (third == b'\\' || third == b'/')
            {
                return true;
            }
        }
        idx = idx.saturating_add(1);
    }
    false
}

fn validate_external_url(entry_id: &str, url: &str) -> Result<(), ReportError> {
    if url.trim().is_empty() || url.bytes().any(|b| b <= 0x20 || b == 0x7f) {
        return Err(ReportError::UnsafeExternalUrl {
            entry_id: make_owned_str(entry_id)?,
            reason: "external URL must not be empty or contain whitespace/control bytes",
        });
    }
    let rest = if let Some(stripped) = url.strip_prefix("https://") {
        stripped
    } else if let Some(stripped) = url.strip_prefix("http://") {
        stripped
    } else {
        return Err(ReportError::UnsafeExternalUrl {
            entry_id: make_owned_str(entry_id)?,
            reason: "external URL must use explicit http:// or https:// scheme",
        });
    };

    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = rest.get(..authority_end).unwrap_or("");
    if authority.is_empty() {
        return Err(ReportError::UnsafeExternalUrl {
            entry_id: make_owned_str(entry_id)?,
            reason: "external URL is missing host authority",
        });
    }
    if authority.contains('@') {
        return Err(ReportError::UnsafeExternalUrl {
            entry_id: make_owned_str(entry_id)?,
            reason: "credential-bearing userinfo in external URL is prohibited",
        });
    }

    let host = if authority.starts_with('[') {
        authority
    } else {
        authority.split(':').next().unwrap_or("")
    };
    if is_private_or_local_host(host) {
        return Err(ReportError::UnsafeExternalUrl {
            entry_id: make_owned_str(entry_id)?,
            reason: "private, loopback, or local-network endpoint is prohibited in exports",
        });
    }

    if let Some(query_start) = rest.find('?') {
        let query_and_frag = rest.get(query_start.saturating_add(1)..).unwrap_or("");
        let query = query_and_frag.split('#').next().unwrap_or("");
        for pair in query.split('&') {
            let key = pair.split('=').next().unwrap_or("");
            if key.eq_ignore_ascii_case("token")
                || key.eq_ignore_ascii_case("access_token")
                || key.eq_ignore_ascii_case("api_key")
                || key.eq_ignore_ascii_case("apikey")
                || key.eq_ignore_ascii_case("key")
                || key.eq_ignore_ascii_case("secret")
                || key.eq_ignore_ascii_case("password")
                || key.eq_ignore_ascii_case("sig")
                || key.eq_ignore_ascii_case("signature")
            {
                return Err(ReportError::UnsafeExternalUrl {
                    entry_id: make_owned_str(entry_id)?,
                    reason: "credential or signature query parameter in external URL is prohibited",
                });
            }
        }
    }

    Ok(())
}

fn is_private_or_local_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost")
        || host.starts_with("[::1]")
        || host.starts_with("[fe80:")
        || host.starts_with("[fc")
        || host.starts_with("[fd")
        || host.ends_with(".local")
        || host.ends_with(".internal")
        || host.ends_with(".localhost")
    {
        return true;
    }
    if host.starts_with("127.")
        || host.starts_with("10.")
        || host.starts_with("192.168.")
        || host.starts_with("169.254.")
        || host == "0.0.0.0"
    {
        return true;
    }
    if let Some(after_172) = host.strip_prefix("172.")
        && let Some(second_octet_str) = after_172.split('.').next()
        && let Ok(second_octet) = second_octet_str.parse::<u8>()
        && (16..=31).contains(&second_octet)
    {
        return true;
    }
    false
}

fn validate_citation_anchor<'input>(
    statement_id: &'input str,
    target_entry: &EvidenceInventoryEntry<'input>,
    anchor: CitationAnchor<'input>,
    cumulative_text_bytes: &mut usize,
    limits: ReportLimits,
) -> Result<AnchorValidationState<'input>, ReportError> {
    match anchor {
        CitationAnchor::Utf8ByteRange {
            start,
            end,
            is_unadjusted_original_offset,
        } => {
            if is_unadjusted_original_offset && target_entry.redacted_from_entry_id.is_some() {
                return Err(ReportError::UnadjustedRedactedAnchor {
                    statement_id: make_owned_str(statement_id)?,
                    entry_id: make_owned_str(target_entry.entry_id)?,
                });
            }
            if start >= end {
                return Err(ReportError::InvalidCitationAnchor {
                    statement_id: make_owned_str(statement_id)?,
                    entry_id: make_owned_str(target_entry.entry_id)?,
                    reason: "Utf8ByteRange requires start < end",
                });
            }
            if target_entry.availability == TargetAvailability::Available
                && target_entry.disclosure == RecipientDisclosure::FullTarget
            {
                let Some(bytes) = target_entry.retained_utf8_bytes else {
                    return Err(ReportError::InvalidCitationAnchor {
                        statement_id: make_owned_str(statement_id)?,
                        entry_id: make_owned_str(target_entry.entry_id)?,
                        reason: "Utf8ByteRange citation requires retained_utf8_bytes on Available FullTarget entry",
                    });
                };
                let text =
                    std::str::from_utf8(bytes).map_err(|err| ReportError::MalformedTargetUtf8 {
                        entry_id: make_owned_str(target_entry.entry_id).unwrap_or_default(),
                        valid_up_to: err.valid_up_to(),
                    })?;
                if end > text.len() {
                    return Err(ReportError::InvalidCitationAnchor {
                        statement_id: make_owned_str(statement_id)?,
                        entry_id: make_owned_str(target_entry.entry_id)?,
                        reason: "Utf8ByteRange end exceeds target byte length",
                    });
                }
                if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
                    return Err(ReportError::InvalidCitationAnchor {
                        statement_id: make_owned_str(statement_id)?,
                        entry_id: make_owned_str(target_entry.entry_id)?,
                        reason: "Utf8ByteRange bounds do not lie on UTF-8 character boundaries",
                    });
                }
                let Some(quoted_text) = text.get(start..end) else {
                    return Err(ReportError::InvalidCitationAnchor {
                        statement_id: make_owned_str(statement_id)?,
                        entry_id: make_owned_str(target_entry.entry_id)?,
                        reason: "Utf8ByteRange slice out of bounds",
                    });
                };
                Ok(AnchorValidationState::ValidatedUtf8Slice {
                    start,
                    end,
                    quoted_text,
                })
            } else {
                let mut summary = String::new();
                push_str_bounded(&mut summary, "UTF-8 byte range ", limits.max_output_bytes)?;
                push_usize_bounded(&mut summary, start, limits.max_output_bytes)?;
                push_str_bounded(&mut summary, "..", limits.max_output_bytes)?;
                push_usize_bounded(&mut summary, end, limits.max_output_bytes)?;
                Ok(AnchorValidationState::UnvalidatedTargetUnavailable {
                    anchor_summary: summary,
                    availability: target_entry.availability,
                    omission_reason: target_entry.omission_reason,
                })
            }
        }
        CitationAnchor::ExtractedPassage {
            extraction_version_id,
            passage_line,
            byte_start,
            byte_end,
        } => {
            validate_non_empty(
                extraction_version_id,
                "citation.anchor.extraction_version_id",
            )?;
            add_text_budget(
                cumulative_text_bytes,
                extraction_version_id,
                "citation.anchor.extraction_version_id",
                limits,
            )?;
            if passage_line == 0 || byte_start >= byte_end {
                return Err(ReportError::InvalidCitationAnchor {
                    statement_id: make_owned_str(statement_id)?,
                    entry_id: make_owned_str(target_entry.entry_id)?,
                    reason: "ExtractedPassage requires passage_line >= 1 and byte_start < byte_end",
                });
            }
            if target_entry.availability == TargetAvailability::Available
                && target_entry.disclosure == RecipientDisclosure::FullTarget
            {
                Ok(AnchorValidationState::UnvalidatedExtractedPassage {
                    extraction_version_id,
                    passage_line,
                    byte_start,
                    byte_end,
                    availability: target_entry.availability,
                })
            } else {
                let mut summary = String::new();
                push_str_bounded(&mut summary, "extraction ", limits.max_output_bytes)?;
                push_str_bounded(&mut summary, extraction_version_id, limits.max_output_bytes)?;
                push_str_bounded(&mut summary, " line ", limits.max_output_bytes)?;
                push_usize_bounded(&mut summary, passage_line, limits.max_output_bytes)?;
                push_str_bounded(&mut summary, " (", limits.max_output_bytes)?;
                push_usize_bounded(&mut summary, byte_start, limits.max_output_bytes)?;
                push_str_bounded(&mut summary, "..", limits.max_output_bytes)?;
                push_usize_bounded(&mut summary, byte_end, limits.max_output_bytes)?;
                push_str_bounded(&mut summary, ")", limits.max_output_bytes)?;
                Ok(AnchorValidationState::UnvalidatedTargetUnavailable {
                    anchor_summary: summary,
                    availability: target_entry.availability,
                    omission_reason: target_entry.omission_reason,
                })
            }
        }
        CitationAnchor::PageRegion {
            page_number,
            region_label,
        } => {
            if page_number == 0 || region_label.trim().is_empty() {
                return Err(ReportError::InvalidCitationAnchor {
                    statement_id: make_owned_str(statement_id)?,
                    entry_id: make_owned_str(target_entry.entry_id)?,
                    reason: "PageRegion requires page_number >= 1 and non-empty region_label",
                });
            }
            add_text_budget(
                cumulative_text_bytes,
                region_label,
                "citation.anchor.region_label",
                limits,
            )?;
            if target_entry.availability == TargetAvailability::Available
                && target_entry.disclosure == RecipientDisclosure::FullTarget
            {
                Ok(AnchorValidationState::UnvalidatedPageRegion {
                    page_number,
                    region_label,
                    availability: target_entry.availability,
                })
            } else {
                let mut summary = String::new();
                push_str_bounded(&mut summary, "page ", limits.max_output_bytes)?;
                push_usize_bounded(&mut summary, page_number, limits.max_output_bytes)?;
                push_str_bounded(&mut summary, " region ", limits.max_output_bytes)?;
                push_str_bounded(&mut summary, region_label, limits.max_output_bytes)?;
                Ok(AnchorValidationState::UnvalidatedTargetUnavailable {
                    anchor_summary: summary,
                    availability: target_entry.availability,
                    omission_reason: target_entry.omission_reason,
                })
            }
        }
        CitationAnchor::UnsupportedAnchor {
            kind,
            limitation_note,
        } => {
            if kind.trim().is_empty() || limitation_note.trim().is_empty() {
                return Err(ReportError::InvalidCitationAnchor {
                    statement_id: make_owned_str(statement_id)?,
                    entry_id: make_owned_str(target_entry.entry_id)?,
                    reason: "UnsupportedAnchor requires non-empty kind and limitation_note",
                });
            }
            add_text_budget(cumulative_text_bytes, kind, "citation.anchor.kind", limits)?;
            add_text_budget(
                cumulative_text_bytes,
                limitation_note,
                "citation.anchor.limitation_note",
                limits,
            )?;
            Ok(AnchorValidationState::UnsupportedAnchorLimitation {
                kind,
                limitation_note,
            })
        }
    }
}

fn push_time_display_markdown(
    out: &mut String,
    time: TimeDisplay<'_>,
    max_out: usize,
) -> Result<(), ReportError> {
    match time.precision {
        TimePrecision::Unknown { reason } => {
            push_str_bounded(out, "`unknown time` (", max_out)?;
            push_markdown_escaped(out, reason, max_out)?;
            push_str_bounded(out, ", ref: ", max_out)?;
            push_markdown_code_span(out, time.time_reference, max_out)?;
            push_str_bounded(out, ")", max_out)?;
        }
        _ => {
            push_markdown_code_span(out, time.text, max_out)?;
            push_str_bounded(out, " (", max_out)?;
            push_str_bounded(out, time.precision.label(), max_out)?;
            push_str_bounded(out, ", ref: ", max_out)?;
            push_markdown_code_span(out, time.time_reference, max_out)?;
            push_str_bounded(out, ")", max_out)?;
        }
    }
    Ok(())
}

fn push_time_display_html(
    out: &mut String,
    time: TimeDisplay<'_>,
    max_out: usize,
) -> Result<(), ReportError> {
    match time.precision {
        TimePrecision::Unknown { reason } => {
            push_str_bounded(out, "<code>unknown time</code> (", max_out)?;
            push_html_escaped(out, reason, max_out)?;
            push_str_bounded(out, ", ref: <code>", max_out)?;
            push_html_escaped(out, time.time_reference, max_out)?;
            push_str_bounded(out, "</code>)", max_out)?;
        }
        _ => {
            push_str_bounded(out, "<code>", max_out)?;
            push_html_escaped(out, time.text, max_out)?;
            push_str_bounded(out, "</code> (", max_out)?;
            push_str_bounded(out, time.precision.label(), max_out)?;
            push_str_bounded(out, ", ref: <code>", max_out)?;
            push_html_escaped(out, time.time_reference, max_out)?;
            push_str_bounded(out, "</code>)", max_out)?;
        }
    }
    Ok(())
}

fn push_anchor_validation_markdown(
    out: &mut String,
    validation: &AnchorValidationState<'_>,
    max_out: usize,
) -> Result<(), ReportError> {
    match validation {
        AnchorValidationState::ValidatedUtf8Slice {
            start,
            end,
            quoted_text,
        } => {
            push_str_bounded(out, "`[ANCHOR VALIDATED: UTF-8 bytes ", max_out)?;
            push_usize_bounded(out, *start, max_out)?;
            push_str_bounded(out, "..", max_out)?;
            push_usize_bounded(out, *end, max_out)?;
            push_str_bounded(out, "]` quote: \"", max_out)?;
            push_markdown_escaped(out, quoted_text, max_out)?;
            push_str_bounded(out, "\"", max_out)?;
        }
        AnchorValidationState::UnvalidatedExtractedPassage {
            extraction_version_id,
            passage_line,
            byte_start,
            byte_end,
            availability,
        } => {
            push_str_bounded(
                out,
                "**[ANCHOR UNVALIDATED (extracted passage):** extraction ",
                max_out,
            )?;
            push_markdown_code_span(out, extraction_version_id, max_out)?;
            push_str_bounded(out, " line ", max_out)?;
            push_usize_bounded(out, *passage_line, max_out)?;
            push_str_bounded(out, " bytes ", max_out)?;
            push_usize_bounded(out, *byte_start, max_out)?;
            push_str_bounded(out, "..", max_out)?;
            push_usize_bounded(out, *byte_end, max_out)?;
            push_str_bounded(out, ", target ", max_out)?;
            push_str_bounded(out, availability.label(), max_out)?;
            push_str_bounded(out, "**]**", max_out)?;
        }
        AnchorValidationState::UnvalidatedPageRegion {
            page_number,
            region_label,
            availability,
        } => {
            push_str_bounded(out, "**[ANCHOR UNVALIDATED (page region):** page ", max_out)?;
            push_usize_bounded(out, *page_number, max_out)?;
            push_str_bounded(out, " ", max_out)?;
            push_markdown_escaped(out, region_label, max_out)?;
            push_str_bounded(out, ", target ", max_out)?;
            push_str_bounded(out, availability.label(), max_out)?;
            push_str_bounded(out, "**]**", max_out)?;
        }
        AnchorValidationState::UnvalidatedTargetUnavailable {
            anchor_summary,
            availability,
            omission_reason,
        } => {
            push_str_bounded(out, "**[ANCHOR UNVALIDATED: target ", max_out)?;
            push_str_bounded(out, availability.label(), max_out)?;
            if let Some(reason) = omission_reason {
                push_str_bounded(out, " (", max_out)?;
                push_str_bounded(out, reason.label(), max_out)?;
                push_str_bounded(out, ")", max_out)?;
            }
            push_str_bounded(out, " — ", max_out)?;
            push_markdown_escaped(out, anchor_summary, max_out)?;
            push_str_bounded(out, "**]**", max_out)?;
        }
        AnchorValidationState::UnsupportedAnchorLimitation {
            kind,
            limitation_note,
        } => {
            push_str_bounded(out, "**[UNSUPPORTED ANCHOR LIMITATION: ", max_out)?;
            push_markdown_escaped(out, kind, max_out)?;
            push_str_bounded(out, "]** ", max_out)?;
            push_markdown_escaped(out, limitation_note, max_out)?;
        }
    }
    Ok(())
}

fn push_anchor_validation_html(
    out: &mut String,
    validation: &AnchorValidationState<'_>,
    max_out: usize,
) -> Result<(), ReportError> {
    match validation {
        AnchorValidationState::ValidatedUtf8Slice {
            start,
            end,
            quoted_text,
        } => {
            push_str_bounded(
                out,
                "<span class=\"badge\">[ANCHOR VALIDATED: UTF-8 bytes ",
                max_out,
            )?;
            push_usize_bounded(out, *start, max_out)?;
            push_str_bounded(out, "..", max_out)?;
            push_usize_bounded(out, *end, max_out)?;
            push_str_bounded(out, "]</span> quote: <q>", max_out)?;
            push_html_escaped(out, quoted_text, max_out)?;
            push_str_bounded(out, "</q>", max_out)?;
        }
        AnchorValidationState::UnvalidatedExtractedPassage {
            extraction_version_id,
            passage_line,
            byte_start,
            byte_end,
            availability,
        } => {
            push_str_bounded(
                out,
                "<span class=\"badge\">[ANCHOR UNVALIDATED (extracted passage): extraction <code>",
                max_out,
            )?;
            push_html_escaped(out, extraction_version_id, max_out)?;
            push_str_bounded(out, "</code> line ", max_out)?;
            push_usize_bounded(out, *passage_line, max_out)?;
            push_str_bounded(out, " bytes ", max_out)?;
            push_usize_bounded(out, *byte_start, max_out)?;
            push_str_bounded(out, "..", max_out)?;
            push_usize_bounded(out, *byte_end, max_out)?;
            push_str_bounded(out, ", target ", max_out)?;
            push_str_bounded(out, availability.label(), max_out)?;
            push_str_bounded(out, "]</span>", max_out)?;
        }
        AnchorValidationState::UnvalidatedPageRegion {
            page_number,
            region_label,
            availability,
        } => {
            push_str_bounded(
                out,
                "<span class=\"badge\">[ANCHOR UNVALIDATED (page region): page ",
                max_out,
            )?;
            push_usize_bounded(out, *page_number, max_out)?;
            push_str_bounded(out, " ", max_out)?;
            push_html_escaped(out, region_label, max_out)?;
            push_str_bounded(out, ", target ", max_out)?;
            push_str_bounded(out, availability.label(), max_out)?;
            push_str_bounded(out, "]</span>", max_out)?;
        }
        AnchorValidationState::UnvalidatedTargetUnavailable {
            anchor_summary,
            availability,
            omission_reason,
        } => {
            push_str_bounded(
                out,
                "<span class=\"badge\">[ANCHOR UNVALIDATED: target ",
                max_out,
            )?;
            push_str_bounded(out, availability.label(), max_out)?;
            if let Some(reason) = omission_reason {
                push_str_bounded(out, " (", max_out)?;
                push_str_bounded(out, reason.label(), max_out)?;
                push_str_bounded(out, ")", max_out)?;
            }
            push_str_bounded(out, " — ", max_out)?;
            push_html_escaped(out, anchor_summary, max_out)?;
            push_str_bounded(out, "]</span>", max_out)?;
        }
        AnchorValidationState::UnsupportedAnchorLimitation {
            kind,
            limitation_note,
        } => {
            push_str_bounded(
                out,
                "<span class=\"badge\">[UNSUPPORTED ANCHOR LIMITATION: ",
                max_out,
            )?;
            push_html_escaped(out, kind, max_out)?;
            push_str_bounded(out, "]</span> ", max_out)?;
            push_html_escaped(out, limitation_note, max_out)?;
        }
    }
    Ok(())
}

fn push_str_bounded(out: &mut String, text: &str, max_out: usize) -> Result<(), ReportError> {
    let new_len = out.len().saturating_add(text.len());
    if new_len > max_out {
        return Err(ReportError::LimitExceeded {
            limit: Limit::OutputBytes,
            actual: new_len,
            max: max_out,
        });
    }
    out.try_reserve(text.len())
        .map_err(|_| ReportError::AllocationFailed)?;
    out.push_str(text);
    Ok(())
}

fn push_usize_bounded(out: &mut String, value: usize, max_out: usize) -> Result<(), ReportError> {
    let mut digits = [0u8; 24];
    let mut n = value;
    let mut pos = digits.len();
    if n == 0 {
        pos = pos.saturating_sub(1);
        if let Some(slot) = digits.get_mut(pos) {
            *slot = b'0';
        }
    } else {
        while n > 0 {
            pos = pos.saturating_sub(1);
            let rem = (n % 10) as u8;
            if let Some(slot) = digits.get_mut(pos) {
                *slot = b'0'.saturating_add(rem);
            }
            n /= 10;
        }
    }
    let slice = digits.get(pos..).unwrap_or(&[]);
    let text = std::str::from_utf8(slice).unwrap_or("0");
    push_str_bounded(out, text, max_out)
}

fn longest_backtick_run(fragments: &[&str]) -> usize {
    let mut max_run = 0usize;
    let mut current_run = 0usize;
    for fragment in fragments {
        for ch in fragment.chars() {
            if ch == '`' {
                current_run = current_run.saturating_add(1);
                if current_run > max_run {
                    max_run = current_run;
                }
            } else {
                current_run = 0;
            }
        }
    }
    max_run
}

fn push_backticks_bounded(
    out: &mut String,
    count: usize,
    max_out: usize,
) -> Result<(), ReportError> {
    for _ in 0..count {
        push_str_bounded(out, "`", max_out)?;
    }
    Ok(())
}

fn push_code_span_raw_fragment(
    out: &mut String,
    input: &str,
    max_out: usize,
) -> Result<(), ReportError> {
    for ch in input.chars() {
        match ch {
            '\r' | '\n' => push_str_bounded(out, " ", max_out)?,
            c if oia_domain::text::requires_visible_unicode_escape(c) => {
                push_control_hex_escape(out, c as u32, max_out)?;
            }
            c => {
                let mut buf = [0u8; 4];
                let encoded = c.encode_utf8(&mut buf);
                push_str_bounded(out, encoded, max_out)?;
            }
        }
    }
    Ok(())
}

fn push_markdown_code_span(
    out: &mut String,
    input: &str,
    max_out: usize,
) -> Result<(), ReportError> {
    let fence_len = longest_backtick_run(&[input]).saturating_add(1);
    let pad = input.starts_with('`')
        || input.ends_with('`')
        || (input.starts_with(' ') && input.ends_with(' '));
    push_backticks_bounded(out, fence_len, max_out)?;
    if pad {
        push_str_bounded(out, " ", max_out)?;
    }
    push_code_span_raw_fragment(out, input, max_out)?;
    if pad {
        push_str_bounded(out, " ", max_out)?;
    }
    push_backticks_bounded(out, fence_len, max_out)
}

fn push_markdown_code_pair(
    out: &mut String,
    left: &str,
    sep: &str,
    right: &str,
    max_out: usize,
) -> Result<(), ReportError> {
    let fence_len = longest_backtick_run(&[left, sep, right]).saturating_add(1);
    let pad = left.starts_with('`')
        || right.ends_with('`')
        || (left.starts_with(' ') && right.ends_with(' '));
    push_backticks_bounded(out, fence_len, max_out)?;
    if pad {
        push_str_bounded(out, " ", max_out)?;
    }
    push_code_span_raw_fragment(out, left, max_out)?;
    push_code_span_raw_fragment(out, sep, max_out)?;
    push_code_span_raw_fragment(out, right, max_out)?;
    if pad {
        push_str_bounded(out, " ", max_out)?;
    }
    push_backticks_bounded(out, fence_len, max_out)
}

// Bundle validation keeps its structural-field policy, including joiners.
pub(crate) fn is_bidi_or_zero_width(ch: char) -> bool {
    oia_domain::text::is_format_control(ch)
}

fn push_markdown_escaped(out: &mut String, input: &str, max_out: usize) -> Result<(), ReportError> {
    for ch in input.chars() {
        match ch {
            '&' => push_str_bounded(out, "&amp;", max_out)?,
            '<' => push_str_bounded(out, "&lt;", max_out)?,
            '>' => push_str_bounded(out, "&gt;", max_out)?,
            '\\' => push_str_bounded(out, "\\\\", max_out)?,
            '`' => push_str_bounded(out, "\\`", max_out)?,
            '*' => push_str_bounded(out, "\\*", max_out)?,
            '_' => push_str_bounded(out, "\\_", max_out)?,
            '[' => push_str_bounded(out, "\\[", max_out)?,
            ']' => push_str_bounded(out, "\\]", max_out)?,
            '(' => push_str_bounded(out, "\\(", max_out)?,
            ')' => push_str_bounded(out, "\\)", max_out)?,
            '!' => push_str_bounded(out, "\\!", max_out)?,
            '#' => push_str_bounded(out, "\\#", max_out)?,
            '~' => push_str_bounded(out, "\\~", max_out)?,
            '|' => push_str_bounded(out, "\\|", max_out)?,
            ':' => push_str_bounded(out, "&#58;", max_out)?,
            '.' => push_str_bounded(out, "&#46;", max_out)?,
            '@' => push_str_bounded(out, "&#64;", max_out)?,
            '\r' | '\n' => push_str_bounded(out, " ", max_out)?,
            c if oia_domain::text::requires_visible_unicode_escape(c) => {
                push_control_hex_escape(out, c as u32, max_out)?;
            }
            c => {
                let mut buf = [0u8; 4];
                let encoded = c.encode_utf8(&mut buf);
                push_str_bounded(out, encoded, max_out)?;
            }
        }
    }
    Ok(())
}

fn push_html_escaped(out: &mut String, input: &str, max_out: usize) -> Result<(), ReportError> {
    for ch in input.chars() {
        match ch {
            '&' => push_str_bounded(out, "&amp;", max_out)?,
            '<' => push_str_bounded(out, "&lt;", max_out)?,
            '>' => push_str_bounded(out, "&gt;", max_out)?,
            '"' => push_str_bounded(out, "&quot;", max_out)?,
            '\'' => push_str_bounded(out, "&#39;", max_out)?,
            '\r' | '\n' => push_str_bounded(out, " ", max_out)?,
            c if oia_domain::text::requires_visible_unicode_escape(c) => {
                push_control_hex_escape(out, c as u32, max_out)?;
            }
            c => {
                let mut buf = [0u8; 4];
                let encoded = c.encode_utf8(&mut buf);
                push_str_bounded(out, encoded, max_out)?;
            }
        }
    }
    Ok(())
}

fn push_control_hex_escape(out: &mut String, code: u32, max_out: usize) -> Result<(), ReportError> {
    const HEX: [u8; 16] = *b"0123456789ABCDEF";
    let b3 = ((code >> 12) & 0xF) as usize;
    let b2 = ((code >> 8) & 0xF) as usize;
    let b1 = ((code >> 4) & 0xF) as usize;
    let b0 = (code & 0xF) as usize;
    if code > 0xFFFF {
        let b4 = ((code >> 16) & 0xF) as usize;
        let seq = [
            b'\\',
            b'u',
            b'{',
            *HEX.get(b4).unwrap_or(&b'0'),
            *HEX.get(b3).unwrap_or(&b'0'),
            *HEX.get(b2).unwrap_or(&b'0'),
            *HEX.get(b1).unwrap_or(&b'0'),
            *HEX.get(b0).unwrap_or(&b'0'),
            b'}',
        ];
        let text = std::str::from_utf8(&seq).unwrap_or("\\u{0000}");
        push_str_bounded(out, text, max_out)
    } else {
        let seq = [
            b'\\',
            b'u',
            b'{',
            *HEX.get(b3).unwrap_or(&b'0'),
            *HEX.get(b2).unwrap_or(&b'0'),
            *HEX.get(b1).unwrap_or(&b'0'),
            *HEX.get(b0).unwrap_or(&b'0'),
            b'}',
        ];
        let text = std::str::from_utf8(&seq).unwrap_or("\\u{0000}");
        push_str_bounded(out, text, max_out)
    }
}

fn make_owned_str(input: &str) -> Result<String, ReportError> {
    let mut owned = String::new();
    owned
        .try_reserve(input.len())
        .map_err(|_| ReportError::AllocationFailed)?;
    owned.push_str(input);
    Ok(owned)
}
