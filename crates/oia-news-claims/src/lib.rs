pub mod regional;
pub mod review;

use std::error::Error;
use std::fmt;
use std::ops::Range;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LimitKind {
    Sources,
    SourceRecords,
    PassagesPerRecord,
    LineageEdges,
    Claims,
    EvidenceLinksPerClaim,
    Invalidations,
    PublisherSummaries,
    BlobBytes,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AssessmentLimits {
    max_sources: usize,
    max_source_records: usize,
    max_passages_per_record: usize,
    max_lineage_edges: usize,
    max_claims: usize,
    max_evidence_links_per_claim: usize,
    max_invalidations: usize,
    max_publisher_summaries: usize,
    max_blob_bytes: usize,
}

impl AssessmentLimits {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        max_sources: usize,
        max_source_records: usize,
        max_passages_per_record: usize,
        max_lineage_edges: usize,
        max_claims: usize,
        max_evidence_links_per_claim: usize,
        max_invalidations: usize,
        max_publisher_summaries: usize,
        max_blob_bytes: usize,
    ) -> Result<Self, ClaimCheckError> {
        if max_sources == 0 {
            return Err(ClaimCheckError::ZeroLimit(LimitKind::Sources));
        }
        if max_source_records == 0 {
            return Err(ClaimCheckError::ZeroLimit(LimitKind::SourceRecords));
        }
        if max_passages_per_record == 0 {
            return Err(ClaimCheckError::ZeroLimit(LimitKind::PassagesPerRecord));
        }
        if max_lineage_edges == 0 {
            return Err(ClaimCheckError::ZeroLimit(LimitKind::LineageEdges));
        }
        if max_claims == 0 {
            return Err(ClaimCheckError::ZeroLimit(LimitKind::Claims));
        }
        if max_evidence_links_per_claim == 0 {
            return Err(ClaimCheckError::ZeroLimit(LimitKind::EvidenceLinksPerClaim));
        }
        if max_invalidations == 0 {
            return Err(ClaimCheckError::ZeroLimit(LimitKind::Invalidations));
        }
        if max_publisher_summaries == 0 {
            return Err(ClaimCheckError::ZeroLimit(LimitKind::PublisherSummaries));
        }
        if max_blob_bytes == 0 {
            return Err(ClaimCheckError::ZeroLimit(LimitKind::BlobBytes));
        }
        Ok(Self {
            max_sources,
            max_source_records,
            max_passages_per_record,
            max_lineage_edges,
            max_claims,
            max_evidence_links_per_claim,
            max_invalidations,
            max_publisher_summaries,
            max_blob_bytes,
        })
    }

    pub fn max_sources(self) -> usize {
        self.max_sources
    }

    pub fn max_source_records(self) -> usize {
        self.max_source_records
    }

    pub fn max_passages_per_record(self) -> usize {
        self.max_passages_per_record
    }

    pub fn max_lineage_edges(self) -> usize {
        self.max_lineage_edges
    }

    pub fn max_claims(self) -> usize {
        self.max_claims
    }

    pub fn max_evidence_links_per_claim(self) -> usize {
        self.max_evidence_links_per_claim
    }

    pub fn max_invalidations(self) -> usize {
        self.max_invalidations
    }

    pub fn max_publisher_summaries(self) -> usize {
        self.max_publisher_summaries
    }

    pub fn max_blob_bytes(self) -> usize {
        self.max_blob_bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopReason {
    Cancelled,
    DeadlineExceeded,
    Superseded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClaimAssessmentState {
    Supported,
    Contradicted,
    UnsupportedByAvailableEvidence,
    MateriallyMisleading,
    Disputed,
    Unresolved,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewStatus {
    Unreviewed,
    InReview,
    Reviewed,
    Challenged,
    Superseded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AvailabilityState {
    Available,
    ReferenceOnly,
    Missing,
    Restricted,
    Removed,
    Expired,
    Corrupt,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetractionStatus {
    NotRetracted,
    CorrectedByLaterVersion,
    CorrectionIssued,
    Retracted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FaultCategory {
    FactualContradiction,
    UnsupportedCertainty,
    HeadlineCaptionMismatch,
    DistortedQuotation,
    MisattributedImagery,
    NumericalError,
    SelectiveComparison,
    MateriallyMissingContext,
    NewsVolumeEquatedWithCrimeRate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DependentObjectKind {
    SourceRecord,
    EventCandidate,
    Claim,
    Report,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineageRelationKind {
    Quotes,
    Republishes,
    Supersedes,
    Supports,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LineageEdgeInput<'a> {
    pub edge_id: &'a str,
    pub relation_kind: LineageRelationKind,
    pub from_version_id: &'a str,
    pub to_version_id: &'a str,
    pub preserves_independent_corroboration: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PassageAnchor<'a> {
    pub passage_id: &'a str,
    pub language: &'a str,
    pub byte_range: Range<usize>,
    pub char_range: Range<usize>,
    pub quote: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceRecordInput<'a> {
    pub record_id: &'a str,
    pub version_id: &'a str,
    pub source_id: &'a str,
    pub predecessor_version_id: Option<&'a str>,
    pub superseded_by_version_id: Option<&'a str>,
    pub availability: AvailabilityState,
    pub retraction_status: RetractionStatus,
    pub review_status: ReviewStatus,
    pub needs_reassessment: bool,
    pub independent_lineage_root_id: &'a str,
    pub is_syndicated_reprint: bool,
    pub raw_bytes: &'a [u8],
    pub declared_sha256_hex: &'a str,
    pub expected_char_length: usize,
    pub passages: &'a [PassageAnchor<'a>],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PassageRef<'a> {
    pub source_record_version_id: &'a str,
    pub passage_id: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimInput<'a> {
    pub claim_id: &'a str,
    pub version_id: &'a str,
    pub source_record_version_id: &'a str,
    pub passage_id: &'a str,
    pub headline_passage_id: Option<&'a str>,
    pub proposition: &'a str,
    pub claimant: &'a str,
    pub quotes_claim_without_endorsing: bool,
    pub is_opinion_not_factual_premise: bool,
    pub assessment_at_publication: ClaimAssessmentState,
    pub current_assessment: ClaimAssessmentState,
    pub review_status: ReviewStatus,
    pub availability: AvailabilityState,
    pub needs_reassessment: bool,
    pub fault_categories: &'a [FaultCategory],
    pub supporting_evidence: &'a [PassageRef<'a>],
    pub contrary_evidence: &'a [PassageRef<'a>],
    pub criteria_and_reasoning: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventCandidateInput<'a> {
    pub candidate_id: &'a str,
    pub source_record_version_id: &'a str,
    pub review_status: ReviewStatus,
    pub needs_reassessment: bool,
    pub movement_establishes_intent_or_mission: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AffectedObjectInput<'a> {
    pub object_id: &'a str,
    pub object_kind: DependentObjectKind,
    pub status_set_needs_reassessment: bool,
    pub prior_issued_version_preserved: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DependencyInvalidationInput<'a> {
    pub invalidation_id: &'a str,
    pub trigger_version_id: &'a str,
    pub trigger_reason: &'a str,
    pub affected_objects: &'a [AffectedObjectInput<'a>],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AssessmentCounts {
    pub supported: usize,
    pub contradicted: usize,
    pub unsupported_by_available_evidence: usize,
    pub materially_misleading: usize,
    pub disputed: usize,
    pub unresolved: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorrectionHistoryEntry<'a> {
    pub original_version_id: &'a str,
    pub corrected_version_id: &'a str,
    pub corrected_at: &'a str,
    pub summary: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublisherSummaryInput<'a> {
    pub summary_id: &'a str,
    pub source_id: &'a str,
    pub publisher_name: &'a str,
    pub window_start_utc: &'a str,
    pub window_end_utc: &'a str,
    pub sample_size: usize,
    pub evaluated_source_record_versions: &'a [&'a str],
    pub sampling_method: &'a str,
    pub assessment_counts: AssessmentCounts,
    pub unresolved_assessment_count: usize,
    pub correction_history: &'a [CorrectionHistoryEntry<'a>],
    pub uncorrected_upstream_dependencies: &'a [&'a str],
    pub article_count_in_window: usize,
    pub distinct_underlying_incidents: usize,
    pub represents_incident_frequency_or_crime_rate: bool,
    pub methodology_note: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorpusAssessmentInput<'a> {
    pub source_ids: &'a [&'a str],
    pub records: &'a [SourceRecordInput<'a>],
    pub lineage_edges: &'a [LineageEdgeInput<'a>],
    pub candidates: &'a [EventCandidateInput<'a>],
    pub claims: &'a [ClaimInput<'a>],
    pub invalidations: &'a [DependencyInvalidationInput<'a>],
    pub publisher_summaries: &'a [PublisherSummaryInput<'a>],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuotationFidelityStatus {
    NoCheckedQuotationFound,
    ExactUpstreamSubstringMatch,
    DistortedQuoteDetectedAndFlagged,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedClaimReport<'a> {
    pub claim_id: &'a str,
    pub version_id: &'a str,
    pub source_record_version_id: &'a str,
    pub passage_id: &'a str,
    pub assessment_at_publication: ClaimAssessmentState,
    pub current_assessment: ClaimAssessmentState,
    pub review_status: ReviewStatus,
    pub availability: AvailabilityState,
    pub quotes_claim_without_endorsing: bool,
    pub independent_supporting_root_count: usize,
    pub independent_contrary_root_count: usize,
    pub quotation_status: QuotationFidelityStatus,
    pub temporal_assessment_shifted: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorpusVerificationReport<'a> {
    verified_claims: Vec<VerifiedClaimReport<'a>>,
    validated_source_count: usize,
    validated_record_count: usize,
    validated_passage_count: usize,
    validated_lineage_edge_count: usize,
    validated_invalidation_count: usize,
    validated_publisher_summary_count: usize,
    limits: AssessmentLimits,
}

impl<'a> CorpusVerificationReport<'a> {
    pub fn verified_claims(&self) -> &[VerifiedClaimReport<'a>] {
        &self.verified_claims
    }

    pub fn validated_source_count(&self) -> usize {
        self.validated_source_count
    }

    pub fn validated_record_count(&self) -> usize {
        self.validated_record_count
    }

    pub fn validated_passage_count(&self) -> usize {
        self.validated_passage_count
    }

    pub fn validated_lineage_edge_count(&self) -> usize {
        self.validated_lineage_edge_count
    }

    pub fn validated_invalidation_count(&self) -> usize {
        self.validated_invalidation_count
    }

    pub fn validated_publisher_summary_count(&self) -> usize {
        self.validated_publisher_summary_count
    }

    pub fn limits(&self) -> AssessmentLimits {
        self.limits
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClaimCheckError {
    ZeroLimit(LimitKind),
    BudgetExceeded {
        kind: LimitKind,
        actual: usize,
        limit: usize,
    },
    DuplicateIdentifier {
        kind: &'static str,
        id: String,
    },
    UnknownReference {
        context: String,
        target: String,
    },
    MalformedUtf8 {
        record_version_id: String,
        valid_up_to: usize,
    },
    CharLengthMismatch {
        record_version_id: String,
        actual_chars: usize,
        expected_chars: usize,
    },
    InvalidSha256Format {
        record_version_id: String,
    },
    InvalidPassageAnchor {
        record_version_id: String,
        passage_id: String,
        reason: &'static str,
    },
    InvalidLineageEdge {
        edge_id: String,
        reason: &'static str,
    },
    RemovedPageAutoRetracted {
        record_version_id: String,
    },
    MovementPromotedToIntent {
        candidate_id: String,
    },
    EmptyClaimReasoning {
        claim_id: String,
    },
    SupportedWithoutEvidence {
        claim_id: String,
    },
    UnsupportedAutomaticallyFalse {
        claim_id: String,
        reason: &'static str,
    },
    HeadlineMismatchMissingHeadlineAnchor {
        claim_id: String,
    },
    DistortedQuoteNotFlagged {
        claim_id: String,
        quoted_text: String,
    },
    ClaimAvailabilityMismatch {
        claim_id: String,
        claim_availability: AvailabilityState,
        record_availability: AvailabilityState,
    },
    UnpropagatedCorrection {
        invalidation_id: String,
        object_id: String,
        reason: &'static str,
    },
    PublisherSummarySampleMismatch {
        summary_id: String,
        reason: &'static str,
    },
    VolumeEquatedWithCrimeRate {
        summary_id: String,
    },
    AllocationFailed,
    Stopped(StopReason),
}

impl fmt::Display for ClaimCheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroLimit(kind) => write!(f, "{kind:?} limit must be greater than zero"),
            Self::BudgetExceeded {
                kind,
                actual,
                limit,
            } => write!(f, "{kind:?} count/size {actual} exceeds limit {limit}"),
            Self::DuplicateIdentifier { kind, id } => {
                write!(f, "duplicate {kind} identifier: {id}")
            }
            Self::UnknownReference { context, target } => {
                write!(f, "{context} references unknown target {target}")
            }
            Self::MalformedUtf8 {
                record_version_id,
                valid_up_to,
            } => write!(
                f,
                "record {record_version_id} has malformed UTF-8 at byte {valid_up_to}"
            ),
            Self::CharLengthMismatch {
                record_version_id,
                actual_chars,
                expected_chars,
            } => write!(
                f,
                "record {record_version_id} decoded {actual_chars} chars, expected {expected_chars}"
            ),
            Self::InvalidSha256Format { record_version_id } => write!(
                f,
                "record {record_version_id} has invalid SHA-256 hex digest format"
            ),
            Self::InvalidPassageAnchor {
                record_version_id,
                passage_id,
                reason,
            } => write!(
                f,
                "invalid passage anchor {record_version_id}:{passage_id}: {reason}"
            ),
            Self::InvalidLineageEdge { edge_id, reason } => {
                write!(f, "invalid lineage edge {edge_id}: {reason}")
            }
            Self::RemovedPageAutoRetracted { record_version_id } => write!(
                f,
                "removed record {record_version_id} must not be auto-marked retracted (NEWS-04 / A06)"
            ),
            Self::MovementPromotedToIntent { candidate_id } => write!(
                f,
                "candidate {candidate_id} promotes movement to intent or mission (NEWS-02)"
            ),
            Self::EmptyClaimReasoning { claim_id } => write!(
                f,
                "claim {claim_id} must state explicit criteria and reasoning (NEWS-03 / NEWS-04)"
            ),
            Self::SupportedWithoutEvidence { claim_id } => write!(
                f,
                "supported claim {claim_id} must cite at least one supporting passage (NEWS-04)"
            ),
            Self::UnsupportedAutomaticallyFalse { claim_id, reason } => write!(
                f,
                "claim {claim_id} violates NEWS-04 falsity/contradiction rule: {reason}"
            ),
            Self::HeadlineMismatchMissingHeadlineAnchor { claim_id } => write!(
                f,
                "claim {claim_id} flags headline_caption_mismatch without a valid headline_passage_id"
            ),
            Self::DistortedQuoteNotFlagged {
                claim_id,
                quoted_text,
            } => write!(
                f,
                "claim {claim_id} contains unverified verbatim quote \"{quoted_text}\" not present in upstream evidence without flagging distorted_quotation"
            ),
            Self::ClaimAvailabilityMismatch {
                claim_id,
                claim_availability,
                record_availability,
            } => write!(
                f,
                "claim {claim_id} availability {claim_availability:?} does not match source record availability {record_availability:?}"
            ),
            Self::UnpropagatedCorrection {
                invalidation_id,
                object_id,
                reason,
            } => write!(
                f,
                "invalidation {invalidation_id} on {object_id} violates NEWS-05: {reason}"
            ),
            Self::PublisherSummarySampleMismatch { summary_id, reason } => write!(
                f,
                "publisher summary {summary_id} violates NEWS-05 sample disclosure: {reason}"
            ),
            Self::VolumeEquatedWithCrimeRate { summary_id } => write!(
                f,
                "publisher summary {summary_id} violates NEWS-05 volume guard: news volume must not be equated with incident frequency or crime rate"
            ),
            Self::AllocationFailed => write!(f, "memory allocation failed"),
            Self::Stopped(reason) => write!(f, "claim verification stopped: {reason:?}"),
        }
    }
}

impl Error for ClaimCheckError {}

fn is_valid_lower_hex_sha256(hex: &str) -> bool {
    hex.len() == 64
        && hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Validates strict 20-byte RFC 3339 UTC timestamp format `YYYY-MM-DDTHH:MM:SSZ`.
fn is_valid_rfc3339_utc_second(ts: &str) -> bool {
    let bytes = ts.as_bytes();
    if bytes.len() != 20 {
        return false;
    }
    let digit_indices = [0usize, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18];
    for idx in digit_indices {
        if !bytes.get(idx).is_some_and(u8::is_ascii_digit) {
            return false;
        }
    }
    bytes.get(4) == Some(&b'-')
        && bytes.get(7) == Some(&b'-')
        && bytes.get(10) == Some(&b'T')
        && bytes.get(13) == Some(&b':')
        && bytes.get(16) == Some(&b':')
        && bytes.get(19) == Some(&b'Z')
}

fn find_record<'a, 'b>(
    records: &'b [SourceRecordInput<'a>],
    version_id: &str,
) -> Option<&'b SourceRecordInput<'a>> {
    records.iter().find(|r| r.version_id == version_id)
}

fn find_passage<'a, 'b>(
    record: &'b SourceRecordInput<'a>,
    passage_id: &str,
) -> Option<&'b PassageAnchor<'a>> {
    record.passages.iter().find(|p| p.passage_id == passage_id)
}

/// Extracts ASCII double-quoted verbatim spans (`"..."`) of at least 12 bytes from a passage quote.
fn extract_double_quoted_spans(text: &str) -> Result<Vec<&str>, ClaimCheckError> {
    let mut spans = Vec::new();
    let mut rest = text;
    while let Some(open_idx) = rest.find('"') {
        let after_open = rest.get(open_idx + 1..).unwrap_or("");
        let Some(close_idx) = after_open.find('"') else {
            break;
        };
        if let Some(quoted) = after_open.get(..close_idx)
            && quoted.len() >= 12
        {
            spans
                .try_reserve(1)
                .map_err(|_| ClaimCheckError::AllocationFailed)?;
            spans.push(quoted);
        }
        rest = after_open.get(close_idx + 1..).unwrap_or("");
    }
    Ok(spans)
}

/// Collects all `SourceRecord` versions reachable strictly upstream from `start_version_id`
/// by traversing outbound `Quotes` and `Republishes` lineage edges (`from -> to`).
/// Does not include `start_version_id` itself, downstream reprints, or parallel siblings.
fn collect_upstream_quote_or_republish_versions<'a>(
    start_version_id: &'a str,
    lineage_edges: &[LineageEdgeInput<'a>],
) -> Result<Vec<&'a str>, ClaimCheckError> {
    let mut reachable: Vec<&'a str> = Vec::new();
    let mut frontier: Vec<&'a str> = Vec::new();
    frontier
        .try_reserve(1)
        .map_err(|_| ClaimCheckError::AllocationFailed)?;
    frontier.push(start_version_id);

    let mut cursor = 0usize;
    while let Some(&current) = frontier.get(cursor) {
        cursor += 1;
        for edge in lineage_edges {
            if edge.from_version_id == current
                && matches!(
                    edge.relation_kind,
                    LineageRelationKind::Quotes | LineageRelationKind::Republishes
                )
                && edge.to_version_id != start_version_id
                && !reachable.contains(&edge.to_version_id)
            {
                reachable
                    .try_reserve(1)
                    .map_err(|_| ClaimCheckError::AllocationFailed)?;
                reachable.push(edge.to_version_id);
                frontier
                    .try_reserve(1)
                    .map_err(|_| ClaimCheckError::AllocationFailed)?;
                frontier.push(edge.to_version_id);
            }
        }
    }
    Ok(reachable)
}

/// Checks consistency of a corpus of source records, lineage edges, event candidates,
/// passage-level claim assessments, dependency invalidations, and publisher summaries
/// under `NEWS-03`, `NEWS-04`, `NEWS-05`, and `A06`.
pub fn verify_corpus<'a>(
    input: &CorpusAssessmentInput<'a>,
    limits: AssessmentLimits,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<CorpusVerificationReport<'a>, ClaimCheckError> {
    check_control().map_err(ClaimCheckError::Stopped)?;

    if input.source_ids.len() > limits.max_sources {
        return Err(ClaimCheckError::BudgetExceeded {
            kind: LimitKind::Sources,
            actual: input.source_ids.len(),
            limit: limits.max_sources,
        });
    }
    if input.records.len() > limits.max_source_records {
        return Err(ClaimCheckError::BudgetExceeded {
            kind: LimitKind::SourceRecords,
            actual: input.records.len(),
            limit: limits.max_source_records,
        });
    }
    if input.lineage_edges.len() > limits.max_lineage_edges {
        return Err(ClaimCheckError::BudgetExceeded {
            kind: LimitKind::LineageEdges,
            actual: input.lineage_edges.len(),
            limit: limits.max_lineage_edges,
        });
    }
    if input.claims.len() > limits.max_claims {
        return Err(ClaimCheckError::BudgetExceeded {
            kind: LimitKind::Claims,
            actual: input.claims.len(),
            limit: limits.max_claims,
        });
    }
    if input.invalidations.len() > limits.max_invalidations {
        return Err(ClaimCheckError::BudgetExceeded {
            kind: LimitKind::Invalidations,
            actual: input.invalidations.len(),
            limit: limits.max_invalidations,
        });
    }
    if input.publisher_summaries.len() > limits.max_publisher_summaries {
        return Err(ClaimCheckError::BudgetExceeded {
            kind: LimitKind::PublisherSummaries,
            actual: input.publisher_summaries.len(),
            limit: limits.max_publisher_summaries,
        });
    }

    // 1. Validate unique source IDs
    for (idx, sid) in input.source_ids.iter().enumerate() {
        check_control().map_err(ClaimCheckError::Stopped)?;
        if sid.is_empty()
            || input
                .source_ids
                .get(..idx)
                .is_some_and(|prior| prior.contains(sid))
        {
            return Err(ClaimCheckError::DuplicateIdentifier {
                kind: "source_id",
                id: (*sid).to_owned(),
            });
        }
    }

    // 2. Validate SourceRecords and passage anchors (NEWS-03, NEWS-04, A06)
    let mut validated_passage_count = 0usize;
    for (idx, rec) in input.records.iter().enumerate() {
        check_control().map_err(ClaimCheckError::Stopped)?;
        if rec.version_id.is_empty()
            || input
                .records
                .get(..idx)
                .is_some_and(|prior| prior.iter().any(|r| r.version_id == rec.version_id))
        {
            return Err(ClaimCheckError::DuplicateIdentifier {
                kind: "record_version_id",
                id: rec.version_id.to_owned(),
            });
        }
        if !input.source_ids.contains(&rec.source_id) {
            return Err(ClaimCheckError::UnknownReference {
                context: format!("record {}", rec.version_id),
                target: rec.source_id.to_owned(),
            });
        }
        if rec.raw_bytes.len() > limits.max_blob_bytes {
            return Err(ClaimCheckError::BudgetExceeded {
                kind: LimitKind::BlobBytes,
                actual: rec.raw_bytes.len(),
                limit: limits.max_blob_bytes,
            });
        }
        if rec.passages.len() > limits.max_passages_per_record {
            return Err(ClaimCheckError::BudgetExceeded {
                kind: LimitKind::PassagesPerRecord,
                actual: rec.passages.len(),
                limit: limits.max_passages_per_record,
            });
        }
        if !is_valid_lower_hex_sha256(rec.declared_sha256_hex) {
            return Err(ClaimCheckError::InvalidSha256Format {
                record_version_id: rec.version_id.to_owned(),
            });
        }
        // NEWS-04 / A06: A deleted/removed page must never be auto-marked retracted
        if rec.availability == AvailabilityState::Removed
            && rec.retraction_status == RetractionStatus::Retracted
        {
            return Err(ClaimCheckError::RemovedPageAutoRetracted {
                record_version_id: rec.version_id.to_owned(),
            });
        }

        let decoded =
            std::str::from_utf8(rec.raw_bytes).map_err(|err| ClaimCheckError::MalformedUtf8 {
                record_version_id: rec.version_id.to_owned(),
                valid_up_to: err.valid_up_to(),
            })?;
        let actual_chars = decoded.chars().count();
        if actual_chars != rec.expected_char_length {
            return Err(ClaimCheckError::CharLengthMismatch {
                record_version_id: rec.version_id.to_owned(),
                actual_chars,
                expected_chars: rec.expected_char_length,
            });
        }

        for (p_idx, passage) in rec.passages.iter().enumerate() {
            check_control().map_err(ClaimCheckError::Stopped)?;
            if passage.passage_id.is_empty()
                || rec
                    .passages
                    .get(..p_idx)
                    .is_some_and(|prior| prior.iter().any(|p| p.passage_id == passage.passage_id))
            {
                return Err(ClaimCheckError::DuplicateIdentifier {
                    kind: "passage_id",
                    id: format!("{}:{}", rec.version_id, passage.passage_id),
                });
            }
            if passage.byte_range.is_empty() || passage.byte_range.end > rec.raw_bytes.len() {
                return Err(ClaimCheckError::InvalidPassageAnchor {
                    record_version_id: rec.version_id.to_owned(),
                    passage_id: passage.passage_id.to_owned(),
                    reason: "byte_range is empty or exceeds raw_bytes length",
                });
            }
            let Some(byte_slice_str) = decoded.get(passage.byte_range.clone()) else {
                return Err(ClaimCheckError::InvalidPassageAnchor {
                    record_version_id: rec.version_id.to_owned(),
                    passage_id: passage.passage_id.to_owned(),
                    reason: "byte_range splits a multi-byte UTF-8 codepoint",
                });
            };
            if byte_slice_str != passage.quote {
                return Err(ClaimCheckError::InvalidPassageAnchor {
                    record_version_id: rec.version_id.to_owned(),
                    passage_id: passage.passage_id.to_owned(),
                    reason: "byte_range slice does not match passage quote",
                });
            }
            if passage.char_range.is_empty() || passage.char_range.end > actual_chars {
                return Err(ClaimCheckError::InvalidPassageAnchor {
                    record_version_id: rec.version_id.to_owned(),
                    passage_id: passage.passage_id.to_owned(),
                    reason: "char_range is empty or exceeds decoded character count",
                });
            }
            let char_span_len = passage.char_range.end - passage.char_range.start;
            let char_slice_matches = decoded
                .chars()
                .skip(passage.char_range.start)
                .take(char_span_len)
                .eq(passage.quote.chars());
            if !char_slice_matches || passage.quote.chars().count() != char_span_len {
                return Err(ClaimCheckError::InvalidPassageAnchor {
                    record_version_id: rec.version_id.to_owned(),
                    passage_id: passage.passage_id.to_owned(),
                    reason: "char_range slice does not match passage quote",
                });
            }
            validated_passage_count += 1;
        }
    }

    // 3. Validate LineageEdges (NEWS-01, INTEL-01)
    for (idx, edge) in input.lineage_edges.iter().enumerate() {
        check_control().map_err(ClaimCheckError::Stopped)?;
        if edge.edge_id.is_empty()
            || input
                .lineage_edges
                .get(..idx)
                .is_some_and(|prior| prior.iter().any(|e| e.edge_id == edge.edge_id))
        {
            return Err(ClaimCheckError::DuplicateIdentifier {
                kind: "edge_id",
                id: edge.edge_id.to_owned(),
            });
        }
        if find_record(input.records, edge.from_version_id).is_none() {
            return Err(ClaimCheckError::UnknownReference {
                context: format!("lineage edge {}", edge.edge_id),
                target: edge.from_version_id.to_owned(),
            });
        }
        if find_record(input.records, edge.to_version_id).is_none() {
            return Err(ClaimCheckError::UnknownReference {
                context: format!("lineage edge {}", edge.edge_id),
                target: edge.to_version_id.to_owned(),
            });
        }
        if edge.from_version_id == edge.to_version_id {
            return Err(ClaimCheckError::InvalidLineageEdge {
                edge_id: edge.edge_id.to_owned(),
                reason: "self-referential lineage edge is not permitted",
            });
        }
        if matches!(
            edge.relation_kind,
            LineageRelationKind::Quotes
                | LineageRelationKind::Republishes
                | LineageRelationKind::Supersedes
        ) && edge.preserves_independent_corroboration
        {
            return Err(ClaimCheckError::InvalidLineageEdge {
                edge_id: edge.edge_id.to_owned(),
                reason: "quotes, republishes, and supersedes edges must not count as independent corroboration",
            });
        }
    }

    // 4. Validate EventCandidates (NEWS-02 guard)
    for cand in input.candidates {
        check_control().map_err(ClaimCheckError::Stopped)?;
        if cand.movement_establishes_intent_or_mission {
            return Err(ClaimCheckError::MovementPromotedToIntent {
                candidate_id: cand.candidate_id.to_owned(),
            });
        }
        if find_record(input.records, cand.source_record_version_id).is_none() {
            return Err(ClaimCheckError::UnknownReference {
                context: format!("candidate {}", cand.candidate_id),
                target: cand.source_record_version_id.to_owned(),
            });
        }
    }

    // 5. Validate Claims and Assessments (NEWS-03, NEWS-04)
    let mut verified_claims = Vec::new();
    verified_claims
        .try_reserve(input.claims.len())
        .map_err(|_| ClaimCheckError::AllocationFailed)?;

    for (idx, claim) in input.claims.iter().enumerate() {
        check_control().map_err(ClaimCheckError::Stopped)?;
        if claim.claim_id.is_empty()
            || input
                .claims
                .get(..idx)
                .is_some_and(|prior| prior.iter().any(|c| c.claim_id == claim.claim_id))
        {
            return Err(ClaimCheckError::DuplicateIdentifier {
                kind: "claim_id",
                id: claim.claim_id.to_owned(),
            });
        }
        if claim.supporting_evidence.len() > limits.max_evidence_links_per_claim {
            return Err(ClaimCheckError::BudgetExceeded {
                kind: LimitKind::EvidenceLinksPerClaim,
                actual: claim.supporting_evidence.len(),
                limit: limits.max_evidence_links_per_claim,
            });
        }
        if claim.contrary_evidence.len() > limits.max_evidence_links_per_claim {
            return Err(ClaimCheckError::BudgetExceeded {
                kind: LimitKind::EvidenceLinksPerClaim,
                actual: claim.contrary_evidence.len(),
                limit: limits.max_evidence_links_per_claim,
            });
        }
        if claim.criteria_and_reasoning.trim().is_empty() || claim.proposition.trim().is_empty() {
            return Err(ClaimCheckError::EmptyClaimReasoning {
                claim_id: claim.claim_id.to_owned(),
            });
        }

        let Some(source_rec) = find_record(input.records, claim.source_record_version_id) else {
            return Err(ClaimCheckError::UnknownReference {
                context: format!("claim {}", claim.claim_id),
                target: claim.source_record_version_id.to_owned(),
            });
        };
        let Some(claim_passage) = find_passage(source_rec, claim.passage_id) else {
            return Err(ClaimCheckError::UnknownReference {
                context: format!(
                    "claim {} in {}",
                    claim.claim_id, claim.source_record_version_id
                ),
                target: claim.passage_id.to_owned(),
            });
        };

        if claim.availability != source_rec.availability {
            return Err(ClaimCheckError::ClaimAvailabilityMismatch {
                claim_id: claim.claim_id.to_owned(),
                claim_availability: claim.availability,
                record_availability: source_rec.availability,
            });
        }

        if let Some(hl_pid) = claim.headline_passage_id {
            if find_passage(source_rec, hl_pid).is_none() {
                return Err(ClaimCheckError::UnknownReference {
                    context: format!("claim {} headline_passage_id", claim.claim_id),
                    target: hl_pid.to_owned(),
                });
            }
        } else if claim
            .fault_categories
            .contains(&FaultCategory::HeadlineCaptionMismatch)
        {
            return Err(ClaimCheckError::HeadlineMismatchMissingHeadlineAnchor {
                claim_id: claim.claim_id.to_owned(),
            });
        }

        // Validate supporting and contrary evidence anchors and count independent lineage roots
        let mut supporting_roots: Vec<&str> = Vec::new();
        for ev in claim.supporting_evidence {
            let Some(ev_rec) = find_record(input.records, ev.source_record_version_id) else {
                return Err(ClaimCheckError::UnknownReference {
                    context: format!("claim {} supporting_evidence", claim.claim_id),
                    target: ev.source_record_version_id.to_owned(),
                });
            };
            if find_passage(ev_rec, ev.passage_id).is_none() {
                return Err(ClaimCheckError::UnknownReference {
                    context: format!(
                        "claim {} supporting_evidence in {}",
                        claim.claim_id, ev.source_record_version_id
                    ),
                    target: ev.passage_id.to_owned(),
                });
            }
            if !supporting_roots.contains(&ev_rec.independent_lineage_root_id) {
                supporting_roots
                    .try_reserve(1)
                    .map_err(|_| ClaimCheckError::AllocationFailed)?;
                supporting_roots.push(ev_rec.independent_lineage_root_id);
            }
        }

        let mut contrary_roots: Vec<&str> = Vec::new();
        let mut contrary_passages: Vec<&PassageAnchor<'a>> = Vec::new();
        for ev in claim.contrary_evidence {
            let Some(ev_rec) = find_record(input.records, ev.source_record_version_id) else {
                return Err(ClaimCheckError::UnknownReference {
                    context: format!("claim {} contrary_evidence", claim.claim_id),
                    target: ev.source_record_version_id.to_owned(),
                });
            };
            let Some(ev_passage) = find_passage(ev_rec, ev.passage_id) else {
                return Err(ClaimCheckError::UnknownReference {
                    context: format!(
                        "claim {} contrary_evidence in {}",
                        claim.claim_id, ev.source_record_version_id
                    ),
                    target: ev.passage_id.to_owned(),
                });
            };
            if !contrary_roots.contains(&ev_rec.independent_lineage_root_id) {
                contrary_roots
                    .try_reserve(1)
                    .map_err(|_| ClaimCheckError::AllocationFailed)?;
                contrary_roots.push(ev_rec.independent_lineage_root_id);
            }
            contrary_passages
                .try_reserve(1)
                .map_err(|_| ClaimCheckError::AllocationFailed)?;
            contrary_passages.push(ev_passage);
        }

        // NEWS-04: Supported requires supporting evidence; Contradicted / MateriallyMisleading require contrary evidence;
        // UnsupportedByAvailableEvidence is never automatically Contradicted without contrary evidence.
        if (claim.assessment_at_publication == ClaimAssessmentState::Supported
            || claim.current_assessment == ClaimAssessmentState::Supported)
            && claim.supporting_evidence.is_empty()
        {
            return Err(ClaimCheckError::SupportedWithoutEvidence {
                claim_id: claim.claim_id.to_owned(),
            });
        }

        if (claim.assessment_at_publication == ClaimAssessmentState::Contradicted
            || claim.current_assessment == ClaimAssessmentState::Contradicted
            || claim.assessment_at_publication == ClaimAssessmentState::MateriallyMisleading
            || claim.current_assessment == ClaimAssessmentState::MateriallyMisleading)
            && claim.contrary_evidence.is_empty()
        {
            return Err(ClaimCheckError::UnsupportedAutomaticallyFalse {
                claim_id: claim.claim_id.to_owned(),
                reason: "contradicted or materially_misleading assessment requires contrary passage evidence; lack of corroboration alone does not establish falsity",
            });
        }

        if (claim.assessment_at_publication == ClaimAssessmentState::MateriallyMisleading
            || claim.current_assessment == ClaimAssessmentState::MateriallyMisleading)
            && claim.fault_categories.is_empty()
        {
            return Err(ClaimCheckError::UnsupportedAutomaticallyFalse {
                claim_id: claim.claim_id.to_owned(),
                reason: "materially_misleading assessment must declare at least one NEWS-03 fault_category",
            });
        }

        // Quotation fidelity verification (NEWS-03): check only against upstream records
        // reachable via {Quotes, Republishes} edges, never against sibling or downstream reprints.
        let quoted_spans = extract_double_quoted_spans(claim_passage.quote)?;
        let quotation_status = if quoted_spans.is_empty() {
            QuotationFidelityStatus::NoCheckedQuotationFound
        } else {
            let upstream_versions = collect_upstream_quote_or_republish_versions(
                source_rec.version_id,
                input.lineage_edges,
            )?;
            let mut all_present_upstream = true;
            for span in &quoted_spans {
                let present_in_upstream = upstream_versions.iter().any(|up_vid| {
                    find_record(input.records, up_vid).is_some_and(|up_rec| {
                        up_rec.passages.iter().any(|p| p.quote.contains(span))
                    })
                });
                if !present_in_upstream {
                    all_present_upstream = false;
                    if !claim
                        .fault_categories
                        .contains(&FaultCategory::DistortedQuotation)
                        || contrary_passages.is_empty()
                    {
                        return Err(ClaimCheckError::DistortedQuoteNotFlagged {
                            claim_id: claim.claim_id.to_owned(),
                            quoted_text: (*span).to_owned(),
                        });
                    }
                }
            }
            if all_present_upstream {
                QuotationFidelityStatus::ExactUpstreamSubstringMatch
            } else {
                QuotationFidelityStatus::DistortedQuoteDetectedAndFlagged
            }
        };

        verified_claims.push(VerifiedClaimReport {
            claim_id: claim.claim_id,
            version_id: claim.version_id,
            source_record_version_id: claim.source_record_version_id,
            passage_id: claim.passage_id,
            assessment_at_publication: claim.assessment_at_publication,
            current_assessment: claim.current_assessment,
            review_status: claim.review_status,
            availability: claim.availability,
            quotes_claim_without_endorsing: claim.quotes_claim_without_endorsing,
            independent_supporting_root_count: supporting_roots.len(),
            independent_contrary_root_count: contrary_roots.len(),
            quotation_status,
            temporal_assessment_shifted: claim.assessment_at_publication
                != claim.current_assessment,
        });
    }

    // 6. Validate DependencyInvalidations and correction propagation (NEWS-05)
    for inv in input.invalidations {
        check_control().map_err(ClaimCheckError::Stopped)?;
        if find_record(input.records, inv.trigger_version_id).is_none() {
            return Err(ClaimCheckError::UnknownReference {
                context: format!("invalidation {}", inv.invalidation_id),
                target: inv.trigger_version_id.to_owned(),
            });
        }
        if inv.affected_objects.is_empty() {
            return Err(ClaimCheckError::UnpropagatedCorrection {
                invalidation_id: inv.invalidation_id.to_owned(),
                object_id: String::new(),
                reason: "invalidation must list at least one affected object",
            });
        }
        for obj in inv.affected_objects {
            if !obj.status_set_needs_reassessment || !obj.prior_issued_version_preserved {
                return Err(ClaimCheckError::UnpropagatedCorrection {
                    invalidation_id: inv.invalidation_id.to_owned(),
                    object_id: obj.object_id.to_owned(),
                    reason: "affected object must set needs_reassessment=true and preserve prior issued version",
                });
            }
            match obj.object_kind {
                DependentObjectKind::SourceRecord => {
                    let Some(rec) = find_record(input.records, obj.object_id) else {
                        return Err(ClaimCheckError::UnknownReference {
                            context: format!("invalidation {}", inv.invalidation_id),
                            target: obj.object_id.to_owned(),
                        });
                    };
                    if !rec.needs_reassessment {
                        return Err(ClaimCheckError::UnpropagatedCorrection {
                            invalidation_id: inv.invalidation_id.to_owned(),
                            object_id: obj.object_id.to_owned(),
                            reason: "dependent SourceRecord is not marked needs_reassessment",
                        });
                    }
                }
                DependentObjectKind::EventCandidate => {
                    let Some(cand) = input
                        .candidates
                        .iter()
                        .find(|c| c.candidate_id == obj.object_id)
                    else {
                        return Err(ClaimCheckError::UnknownReference {
                            context: format!("invalidation {}", inv.invalidation_id),
                            target: obj.object_id.to_owned(),
                        });
                    };
                    if !cand.needs_reassessment {
                        return Err(ClaimCheckError::UnpropagatedCorrection {
                            invalidation_id: inv.invalidation_id.to_owned(),
                            object_id: obj.object_id.to_owned(),
                            reason: "dependent EventCandidate is not marked needs_reassessment",
                        });
                    }
                }
                DependentObjectKind::Claim => {
                    let Some(clm) = input.claims.iter().find(|c| c.version_id == obj.object_id)
                    else {
                        return Err(ClaimCheckError::UnknownReference {
                            context: format!("invalidation {}", inv.invalidation_id),
                            target: obj.object_id.to_owned(),
                        });
                    };
                    if !clm.needs_reassessment {
                        return Err(ClaimCheckError::UnpropagatedCorrection {
                            invalidation_id: inv.invalidation_id.to_owned(),
                            object_id: obj.object_id.to_owned(),
                            reason: "dependent Claim version is not marked needs_reassessment",
                        });
                    }
                }
                DependentObjectKind::Report => {}
            }
        }
    }

    // 7. Validate PublisherSummaries and volume guards (NEWS-05, A06)
    for ps in input.publisher_summaries {
        check_control().map_err(ClaimCheckError::Stopped)?;
        if !input.source_ids.contains(&ps.source_id) {
            return Err(ClaimCheckError::UnknownReference {
                context: format!("publisher summary {}", ps.summary_id),
                target: ps.source_id.to_owned(),
            });
        }
        if ps.represents_incident_frequency_or_crime_rate || ps.methodology_note.trim().is_empty() {
            return Err(ClaimCheckError::VolumeEquatedWithCrimeRate {
                summary_id: ps.summary_id.to_owned(),
            });
        }
        if ps.sampling_method.trim().is_empty()
            || !is_valid_rfc3339_utc_second(ps.window_start_utc)
            || !is_valid_rfc3339_utc_second(ps.window_end_utc)
            || ps.window_start_utc > ps.window_end_utc
        {
            return Err(ClaimCheckError::PublisherSummarySampleMismatch {
                summary_id: ps.summary_id.to_owned(),
                reason: "missing sampling_method or invalid RFC 3339 UTC time_window bounds",
            });
        }
        let requires_full_window_sample =
            ps.sampling_method == "complete_case_corpus_within_window";
        if ps.sample_size == 0
            || ps.sample_size != ps.evaluated_source_record_versions.len()
            || ps.sample_size > ps.article_count_in_window
            || (requires_full_window_sample && ps.sample_size != ps.article_count_in_window)
            || ps.distinct_underlying_incidents == 0
        {
            return Err(ClaimCheckError::PublisherSummarySampleMismatch {
                summary_id: ps.summary_id.to_owned(),
                reason: "sample_size must be non-zero, match evaluated_source_record_versions, and satisfy sampling_method/article_count_in_window bounds",
            });
        }

        let mut expected_uncorrected_upstream: Vec<&str> = Vec::new();
        for vid in ps.evaluated_source_record_versions {
            let Some(rec) = find_record(input.records, vid) else {
                return Err(ClaimCheckError::UnknownReference {
                    context: format!("publisher summary {}", ps.summary_id),
                    target: (*vid).to_owned(),
                });
            };
            if rec.source_id != ps.source_id {
                return Err(ClaimCheckError::PublisherSummarySampleMismatch {
                    summary_id: ps.summary_id.to_owned(),
                    reason: "evaluated_source_record_version belongs to a different source_id",
                });
            }
            if rec.superseded_by_version_id.is_none() {
                for edge in input.lineage_edges {
                    if edge.from_version_id == rec.version_id
                        && matches!(
                            edge.relation_kind,
                            LineageRelationKind::Quotes | LineageRelationKind::Republishes
                        )
                        && let Some(up_rec) = find_record(input.records, edge.to_version_id)
                        && up_rec.superseded_by_version_id.is_some()
                        && !expected_uncorrected_upstream.contains(&up_rec.version_id)
                    {
                        expected_uncorrected_upstream
                            .try_reserve(1)
                            .map_err(|_| ClaimCheckError::AllocationFailed)?;
                        expected_uncorrected_upstream.push(up_rec.version_id);
                    }
                }
            }
        }

        if ps.uncorrected_upstream_dependencies.len() != expected_uncorrected_upstream.len()
            || !expected_uncorrected_upstream
                .iter()
                .all(|req| ps.uncorrected_upstream_dependencies.contains(req))
        {
            return Err(ClaimCheckError::PublisherSummarySampleMismatch {
                summary_id: ps.summary_id.to_owned(),
                reason: "uncorrected_upstream_dependencies does not match superseded upstream records referenced by this publisher",
            });
        }

        // Cross-check assessment_counts against claims in evaluated_source_record_versions
        let mut computed = AssessmentCounts {
            supported: 0,
            contradicted: 0,
            unsupported_by_available_evidence: 0,
            materially_misleading: 0,
            disputed: 0,
            unresolved: 0,
        };
        for clm in input.claims {
            if ps
                .evaluated_source_record_versions
                .contains(&clm.source_record_version_id)
            {
                match clm.current_assessment {
                    ClaimAssessmentState::Supported => computed.supported += 1,
                    ClaimAssessmentState::Contradicted => computed.contradicted += 1,
                    ClaimAssessmentState::UnsupportedByAvailableEvidence => {
                        computed.unsupported_by_available_evidence += 1;
                    }
                    ClaimAssessmentState::MateriallyMisleading => {
                        computed.materially_misleading += 1;
                    }
                    ClaimAssessmentState::Disputed => computed.disputed += 1,
                    ClaimAssessmentState::Unresolved => computed.unresolved += 1,
                }
            }
        }
        if computed != ps.assessment_counts
            || ps.unresolved_assessment_count != computed.unresolved + computed.disputed
        {
            return Err(ClaimCheckError::PublisherSummarySampleMismatch {
                summary_id: ps.summary_id.to_owned(),
                reason: "assessment_counts or unresolved_assessment_count does not match evaluated claims for publisher",
            });
        }

        for corr in ps.correction_history {
            let Some(orig_rec) = find_record(input.records, corr.original_version_id) else {
                return Err(ClaimCheckError::UnknownReference {
                    context: format!("publisher summary {} correction_history", ps.summary_id),
                    target: corr.original_version_id.to_owned(),
                });
            };
            let Some(corr_rec) = find_record(input.records, corr.corrected_version_id) else {
                return Err(ClaimCheckError::UnknownReference {
                    context: format!("publisher summary {} correction_history", ps.summary_id),
                    target: corr.corrected_version_id.to_owned(),
                });
            };
            if orig_rec.superseded_by_version_id != Some(corr.corrected_version_id)
                || corr_rec.predecessor_version_id != Some(corr.original_version_id)
                || !is_valid_rfc3339_utc_second(corr.corrected_at)
                || corr.summary.trim().is_empty()
            {
                return Err(ClaimCheckError::PublisherSummarySampleMismatch {
                    summary_id: ps.summary_id.to_owned(),
                    reason: "correction_history entry does not match SourceRecord supersession links or RFC 3339 UTC timestamp",
                });
            }
        }
    }

    check_control().map_err(ClaimCheckError::Stopped)?;

    Ok(CorpusVerificationReport {
        verified_claims,
        validated_source_count: input.source_ids.len(),
        validated_record_count: input.records.len(),
        validated_passage_count,
        validated_lineage_edge_count: input.lineage_edges.len(),
        validated_invalidation_count: input.invalidations.len(),
        validated_publisher_summary_count: input.publisher_summaries.len(),
        limits,
    })
}
