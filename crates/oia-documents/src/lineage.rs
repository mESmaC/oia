//! Bounded document release comparison, source-lineage graph evaluation,
//! corroboration independence accounting, and correction/retraction propagation.
//!
//! This module implements the pure validation and graph-evaluation kernel for
//! `DOC-02`, `DOC-04`, `DOC-05`, and `INTEL-01` across scenarios `A03` and `A08`,
//! following `docs/contracts/document-lineage-and-correction.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::ops::Range;

use crate::StopReason;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineageLimit {
    Nodes,
    Edges,
    Segments,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LineageLimits {
    max_nodes: usize,
    max_edges: usize,
    max_segments: usize,
}

impl LineageLimits {
    pub fn new(
        max_nodes: usize,
        max_edges: usize,
        max_segments: usize,
    ) -> Result<Self, LineageError> {
        if max_nodes == 0 {
            return Err(LineageError::ZeroLimit(LineageLimit::Nodes));
        }
        if max_edges == 0 {
            return Err(LineageError::ZeroLimit(LineageLimit::Edges));
        }
        if max_segments == 0 {
            return Err(LineageError::ZeroLimit(LineageLimit::Segments));
        }
        Ok(Self {
            max_nodes,
            max_edges,
            max_segments,
        })
    }

    pub fn max_nodes(self) -> usize {
        self.max_nodes
    }

    pub fn max_edges(self) -> usize {
        self.max_edges
    }

    pub fn max_segments(self) -> usize {
        self.max_segments
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LineageError {
    ZeroLimit(LineageLimit),
    LineageGraphBudgetExceeded {
        limit: LineageLimit,
        actual: usize,
        max: usize,
    },
    DuplicateIdentifier {
        kind: &'static str,
        id: String,
    },
    DisjointVersionNamespaceViolation {
        version_id: String,
    },
    UnknownReference {
        context: &'static str,
        target_id: String,
    },
    MissingUnknownTimeReason {
        record_version_id: String,
    },
    InvalidRetentionState {
        record_version_id: String,
        reason: &'static str,
    },
    IdenticalSha256MultipleArtifacts {
        sha256: String,
    },
    ExactDuplicateCollapsedSourceId {
        sha256: String,
        source_id: String,
    },
    CrossVersionCitationMismatch {
        record_version_id: String,
        artifact_id: String,
        page: usize,
    },
    InvalidByteAnchorRange {
        artifact_id: String,
        start: usize,
        end: usize,
        byte_length: usize,
    },
    CitationQuoteMismatch {
        record_version_id: String,
        artifact_id: String,
    },
    HiddenRedactionPayloadRejected {
        comparison_id: String,
        artifact_id: String,
        forbidden_cleartext: String,
    },
    InvalidReleaseComparison {
        comparison_id: String,
        reason: &'static str,
    },
    DerivationCycleDetected {
        version_id: String,
    },
    InvalidLifecycleTransition {
        record_version_id: String,
        reason: &'static str,
    },
    RemovedEndpointAutoRetracted {
        record_version_id: String,
    },
    AllocationFailed,
    Stopped(StopReason),
}

impl fmt::Display for LineageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroLimit(limit) => {
                write!(formatter, "{limit:?} limit must be greater than zero")
            }
            Self::LineageGraphBudgetExceeded { limit, actual, max } => write!(
                formatter,
                "{limit:?} count {actual} exceeds configured limit {max}"
            ),
            Self::DuplicateIdentifier { kind, id } => {
                write!(formatter, "duplicate {kind} identifier: {id:?}")
            }
            Self::DisjointVersionNamespaceViolation { version_id } => write!(
                formatter,
                "version_id {version_id:?} appears in both records and derivations"
            ),
            Self::UnknownReference { context, target_id } => {
                write!(formatter, "unknown {context} reference: {target_id:?}")
            }
            Self::MissingUnknownTimeReason { record_version_id } => write!(
                formatter,
                "record {record_version_id:?} with null timestamp must provide unknown_time_reason"
            ),
            Self::InvalidRetentionState {
                record_version_id,
                reason,
            } => write!(
                formatter,
                "invalid retention state on {record_version_id:?}: {reason}"
            ),
            Self::IdenticalSha256MultipleArtifacts { sha256 } => write!(
                formatter,
                "identical sha256 {sha256:?} mapped to multiple artifact IDs"
            ),
            Self::ExactDuplicateCollapsedSourceId { sha256, source_id } => write!(
                formatter,
                "exact-byte duplicate group for {sha256:?} collapsed source_id {source_id:?}"
            ),
            Self::CrossVersionCitationMismatch {
                record_version_id,
                artifact_id,
                page,
            } => write!(
                formatter,
                "citation to {record_version_id:?} does not own artifact {artifact_id:?} on page {page}"
            ),
            Self::InvalidByteAnchorRange {
                artifact_id,
                start,
                end,
                byte_length,
            } => write!(
                formatter,
                "invalid UTF-8 byte anchor {start}..{end} for artifact {artifact_id:?} ({byte_length} bytes)"
            ),
            Self::CitationQuoteMismatch {
                record_version_id,
                artifact_id,
            } => write!(
                formatter,
                "quoted text does not match exact byte slice in {artifact_id:?} for {record_version_id:?}"
            ),
            Self::HiddenRedactionPayloadRejected {
                comparison_id,
                artifact_id,
                forbidden_cleartext,
            } => write!(
                formatter,
                "hidden cleartext {forbidden_cleartext:?} present in redacted artifact {artifact_id:?} ({comparison_id:?})"
            ),
            Self::InvalidReleaseComparison {
                comparison_id,
                reason,
            } => write!(
                formatter,
                "invalid release comparison {comparison_id:?}: {reason}"
            ),
            Self::DerivationCycleDetected { version_id } => {
                write!(
                    formatter,
                    "cycle detected in derived_from DAG at {version_id:?}"
                )
            }
            Self::InvalidLifecycleTransition {
                record_version_id,
                reason,
            } => write!(
                formatter,
                "invalid lifecycle transition on {record_version_id:?}: {reason}"
            ),
            Self::RemovedEndpointAutoRetracted { record_version_id } => write!(
                formatter,
                "removed record {record_version_id:?} must not be auto-marked retracted"
            ),
            Self::AllocationFailed => write!(formatter, "unable to allocate lineage metadata"),
            Self::Stopped(reason) => write!(formatter, "lineage operation stopped: {reason:?}"),
        }
    }
}

impl Error for LineageError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactDescriptor {
    pub id: String,
    pub sha256: String,
    pub byte_length: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetentionMode {
    Retained,
    ReferenceOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AvailabilityState {
    Available,
    ReferenceOnly,
    Missing,
    Restricted,
    Removed,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceLifecycleState {
    Active,
    SupersededByCorrection,
    Retracted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageAttachment {
    pub number: usize,
    pub artifact_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceRecordDescriptor {
    pub id: String,
    pub version_id: String,
    pub source_id: String,
    pub availability: AvailabilityState,
    pub lifecycle_state: SourceLifecycleState,
    pub predecessor_version_id: Option<String>,
    pub event_time: Option<String>,
    pub publication_time: Option<String>,
    pub retrieval_time: String,
    pub unknown_time_reason: Option<String>,
    pub retention: RetentionMode,
    pub omission_reason: Option<String>,
    pub pages: Vec<PageAttachment>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelationKind {
    Republishes,
    Quotes,
    Cites,
    Translates,
    Supersedes,
    RetractedBy,
    AlternateRedaction,
    PreprintVersion,
}

impl RelationKind {
    pub fn is_lineage_edge(self) -> bool {
        matches!(
            self,
            Self::Republishes | Self::Quotes | Self::Cites | Self::Translates
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordRelation {
    pub from: String,
    pub to: String,
    pub kind: RelationKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DerivationOriginClass {
    ExtractedClaim,
    AnalyticalInference,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DerivationNode {
    pub version_id: String,
    pub origin_class: DerivationOriginClass,
    pub derived_from: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactDuplicateGroup {
    pub artifact_id: String,
    pub sha256: String,
    pub member_record_versions: Vec<String>,
    pub distinct_source_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Utf8ByteCitation {
    pub record_version_id: String,
    pub artifact_id: String,
    pub page: usize,
    pub byte_range: Range<usize>,
    pub quote: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComparisonSegmentKind {
    UnchangedUnredacted,
    NewlyDisclosedInRight,
    NewlyRedactedInRight,
    DifferentlyRedacted,
    RevisedUnredacted,
    CoverageGap,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseComparisonSegment {
    pub page: usize,
    pub segment_kind: ComparisonSegmentKind,
    pub left_byte_range: Option<Range<usize>>,
    pub right_byte_range: Option<Range<usize>>,
    pub left_text: Option<String>,
    pub right_text: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseRedactionComparison {
    pub comparison_id: String,
    pub left_record_version_id: String,
    pub right_record_version_id: String,
    pub forbidden_cleartext_in_left: Vec<String>,
    pub segments: Vec<ReleaseComparisonSegment>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CycleClassification {
    UnanchoredCycle,
    AnchoredCycle,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DetectedCycle {
    pub members: Vec<String>,
    pub witness_path: Vec<String>,
    pub classification: CycleClassification,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LineageGraphReport {
    pub detected_cycles: Vec<DetectedCycle>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EarliestSourceResolution {
    SingleEarliestDated,
    AmbiguousCandidates,
    NoIndependentRoots,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EarliestSourceCoverageQualification {
    pub scope: &'static str,
    pub omitted_reference_only_records: Vec<String>,
    pub detected_cycle_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EarliestKnownSourceView {
    pub resolution: EarliestSourceResolution,
    pub earliest_dated_root: Option<String>,
    pub undated_candidate_roots: Vec<String>,
    pub coverage_qualification: EarliestSourceCoverageQualification,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorroborationSummary {
    pub supporting_record_versions: Vec<String>,
    pub excluded_derivative_runs: Vec<String>,
    pub circular_only_record_versions: Vec<String>,
    pub primary_cycle_witness: Option<Vec<String>>,
    pub independent_supporting_roots: Vec<String>,
    pub independent_root_count: usize,
    pub earliest_known_source_view: EarliestKnownSourceView,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidationReason {
    SourceCorrected,
    SourceRetracted,
}

impl InvalidationReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SourceCorrected => "source_corrected",
            Self::SourceRetracted => "source_retracted",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidationNotice {
    pub target_version_id: String,
    pub trigger_record_version_id: String,
    pub replacement_record_version_id: String,
    pub reason: InvalidationReason,
    pub preserve_old_citations: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidationPropagationReport {
    pub needs_reassessment: Vec<InvalidationNotice>,
    pub unaffected_fresh_versions: Vec<String>,
    pub removed_not_auto_retracted_record_versions: Vec<String>,
}

/// Validate `SourceRecordDescriptor` invariants (`DOC-02`, `Architecture §2.4`, `§3.1`).
pub fn validate_records(
    records: &[SourceRecordDescriptor],
    artifacts: &BTreeMap<String, ArtifactDescriptor>,
    limits: LineageLimits,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<BTreeMap<String, SourceRecordDescriptor>, LineageError> {
    check_control().map_err(LineageError::Stopped)?;
    if records.len() > limits.max_nodes {
        return Err(LineageError::LineageGraphBudgetExceeded {
            limit: LineageLimit::Nodes,
            actual: records.len(),
            max: limits.max_nodes,
        });
    }
    let mut indexed = BTreeMap::new();
    for record in records {
        check_control().map_err(LineageError::Stopped)?;
        if indexed.contains_key(&record.version_id) {
            return Err(LineageError::DuplicateIdentifier {
                kind: "record version_id",
                id: record.version_id.clone(),
            });
        }
        if record.event_time.is_none() || record.publication_time.is_none() {
            let valid_reason = record
                .unknown_time_reason
                .as_deref()
                .is_some_and(|text| !text.trim().is_empty());
            if !valid_reason {
                return Err(LineageError::MissingUnknownTimeReason {
                    record_version_id: record.version_id.clone(),
                });
            }
        }
        match record.retention {
            RetentionMode::ReferenceOnly => {
                if !record.pages.is_empty() {
                    return Err(LineageError::InvalidRetentionState {
                        record_version_id: record.version_id.clone(),
                        reason: "reference_only record must not retain page artifacts",
                    });
                }
                let has_omission = record
                    .omission_reason
                    .as_deref()
                    .is_some_and(|text| !text.trim().is_empty());
                if !has_omission {
                    return Err(LineageError::InvalidRetentionState {
                        record_version_id: record.version_id.clone(),
                        reason: "reference_only record must state omission_reason",
                    });
                }
            }
            RetentionMode::Retained => {
                if record.pages.is_empty() || record.omission_reason.is_some() {
                    return Err(LineageError::InvalidRetentionState {
                        record_version_id: record.version_id.clone(),
                        reason: "retained record must attach pages and have no omission_reason",
                    });
                }
                for page in &record.pages {
                    let Some(art_id) = page.artifact_id.as_deref() else {
                        return Err(LineageError::InvalidRetentionState {
                            record_version_id: record.version_id.clone(),
                            reason: "retained page must reference an artifact_id",
                        });
                    };
                    if !artifacts.contains_key(art_id) {
                        return Err(LineageError::UnknownReference {
                            context: "page artifact_id",
                            target_id: art_id.to_owned(),
                        });
                    }
                }
            }
        }
        indexed.insert(record.version_id.clone(), record.clone());
    }
    for record in indexed.values() {
        if let Some(pred) = record.predecessor_version_id.as_deref()
            && !indexed.contains_key(pred)
        {
            return Err(LineageError::UnknownReference {
                context: "predecessor_version_id",
                target_id: pred.to_owned(),
            });
        }
    }
    check_control().map_err(LineageError::Stopped)?;
    Ok(indexed)
}

/// Compute `ExactDuplicateGroup`s by content SHA-256 while preserving distinct
/// `SourceRecord` versions and `source_id`s (`DOC-02`, `A03`).
pub fn compute_exact_duplicate_groups(
    records_in_order: &[SourceRecordDescriptor],
    artifacts: &BTreeMap<String, ArtifactDescriptor>,
    limits: LineageLimits,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<Vec<ExactDuplicateGroup>, LineageError> {
    check_control().map_err(LineageError::Stopped)?;
    if records_in_order.len() > limits.max_nodes {
        return Err(LineageError::LineageGraphBudgetExceeded {
            limit: LineageLimit::Nodes,
            actual: records_in_order.len(),
            max: limits.max_nodes,
        });
    }
    let mut by_hash: BTreeMap<String, Vec<(String, String, String)>> = BTreeMap::new();
    for record in records_in_order {
        check_control().map_err(LineageError::Stopped)?;
        if record.retention != RetentionMode::Retained {
            continue;
        }
        for page in &record.pages {
            if let Some(art_id) = page.artifact_id.as_deref() {
                let artifact =
                    artifacts
                        .get(art_id)
                        .ok_or_else(|| LineageError::UnknownReference {
                            context: "artifact_id",
                            target_id: art_id.to_owned(),
                        })?;
                let entry = by_hash.entry(artifact.sha256.clone()).or_default();
                entry
                    .try_reserve(1)
                    .map_err(|_| LineageError::AllocationFailed)?;
                entry.push((
                    art_id.to_owned(),
                    record.version_id.clone(),
                    record.source_id.clone(),
                ));
            }
        }
    }

    let mut groups = Vec::new();
    for (sha256, members) in by_hash {
        check_control().map_err(LineageError::Stopped)?;
        if members.len() > 1 {
            let mut art_ids = BTreeSet::new();
            let mut seen_versions = BTreeSet::new();
            let mut seen_sources = BTreeSet::new();
            let mut member_record_versions = Vec::new();
            let mut distinct_source_ids = Vec::new();
            member_record_versions
                .try_reserve(members.len())
                .map_err(|_| LineageError::AllocationFailed)?;
            distinct_source_ids
                .try_reserve(members.len())
                .map_err(|_| LineageError::AllocationFailed)?;
            for (art_id, version_id, source_id) in members {
                art_ids.insert(art_id);
                if !seen_versions.insert(version_id.clone()) {
                    return Err(LineageError::DuplicateIdentifier {
                        kind: "exact-duplicate member version_id",
                        id: version_id,
                    });
                }
                if !seen_sources.insert(source_id.clone()) {
                    return Err(LineageError::ExactDuplicateCollapsedSourceId {
                        sha256,
                        source_id,
                    });
                }
                member_record_versions.push(version_id);
                distinct_source_ids.push(source_id);
            }
            if art_ids.len() != 1 {
                return Err(LineageError::IdenticalSha256MultipleArtifacts { sha256 });
            }
            let Some(artifact_id) = art_ids.into_iter().next() else {
                return Err(LineageError::IdenticalSha256MultipleArtifacts { sha256 });
            };
            groups
                .try_reserve(1)
                .map_err(|_| LineageError::AllocationFailed)?;
            groups.push(ExactDuplicateGroup {
                artifact_id,
                sha256,
                member_record_versions,
                distinct_source_ids,
            });
        }
    }
    check_control().map_err(LineageError::Stopped)?;
    Ok(groups)
}

/// Verify that a `Utf8ByteCitation` resolves to the exact `SourceRecord` version,
/// page artifact, and UTF-8 byte boundary (`DOC-04`, `A03`).
pub fn verify_utf8_byte_citation(
    citation: &Utf8ByteCitation,
    records: &BTreeMap<String, SourceRecordDescriptor>,
    payloads: &BTreeMap<String, Vec<u8>>,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<(), LineageError> {
    check_control().map_err(LineageError::Stopped)?;
    let record =
        records
            .get(&citation.record_version_id)
            .ok_or_else(|| LineageError::UnknownReference {
                context: "citation record_version_id",
                target_id: citation.record_version_id.clone(),
            })?;
    let owns_page = record.pages.iter().any(|page| {
        page.number == citation.page
            && page.artifact_id.as_deref() == Some(citation.artifact_id.as_str())
    });
    if !owns_page {
        return Err(LineageError::CrossVersionCitationMismatch {
            record_version_id: citation.record_version_id.clone(),
            artifact_id: citation.artifact_id.clone(),
            page: citation.page,
        });
    }
    let raw =
        payloads
            .get(&citation.artifact_id)
            .ok_or_else(|| LineageError::UnknownReference {
                context: "citation artifact_id",
                target_id: citation.artifact_id.clone(),
            })?;
    let start = citation.byte_range.start;
    let end = citation.byte_range.end;
    if start >= end || end > raw.len() {
        return Err(LineageError::InvalidByteAnchorRange {
            artifact_id: citation.artifact_id.clone(),
            start,
            end,
            byte_length: raw.len(),
        });
    }
    let full_text = std::str::from_utf8(raw).map_err(|_| LineageError::InvalidByteAnchorRange {
        artifact_id: citation.artifact_id.clone(),
        start,
        end,
        byte_length: raw.len(),
    })?;
    let Some(slice) = full_text.get(start..end) else {
        return Err(LineageError::InvalidByteAnchorRange {
            artifact_id: citation.artifact_id.clone(),
            start,
            end,
            byte_length: raw.len(),
        });
    };
    if slice != citation.quote {
        return Err(LineageError::CitationQuoteMismatch {
            record_version_id: citation.record_version_id.clone(),
            artifact_id: citation.artifact_id.clone(),
        });
    }
    check_control().map_err(LineageError::Stopped)?;
    Ok(())
}

/// Verify a `ReleaseRedactionComparison` between two distinct document versions,
/// ensuring no hidden cleartext is stored inside redacted artifacts (`DOC-02`, `DOC-04`, `A03`).
pub fn verify_release_comparison(
    comparison: &ReleaseRedactionComparison,
    records: &BTreeMap<String, SourceRecordDescriptor>,
    payloads: &BTreeMap<String, Vec<u8>>,
    limits: LineageLimits,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<(), LineageError> {
    check_control().map_err(LineageError::Stopped)?;
    if comparison.segments.is_empty() {
        return Err(LineageError::InvalidReleaseComparison {
            comparison_id: comparison.comparison_id.clone(),
            reason: "release comparison must contain at least one segment",
        });
    }
    if comparison.segments.len() > limits.max_segments {
        return Err(LineageError::LineageGraphBudgetExceeded {
            limit: LineageLimit::Segments,
            actual: comparison.segments.len(),
            max: limits.max_segments,
        });
    }
    if comparison.left_record_version_id == comparison.right_record_version_id {
        return Err(LineageError::InvalidReleaseComparison {
            comparison_id: comparison.comparison_id.clone(),
            reason: "left and right record versions must be distinct",
        });
    }
    let left_record = records
        .get(&comparison.left_record_version_id)
        .ok_or_else(|| LineageError::UnknownReference {
            context: "left_record_version_id",
            target_id: comparison.left_record_version_id.clone(),
        })?;
    let right_record = records
        .get(&comparison.right_record_version_id)
        .ok_or_else(|| LineageError::UnknownReference {
            context: "right_record_version_id",
            target_id: comparison.right_record_version_id.clone(),
        })?;

    let mut left_pages = BTreeMap::new();
    for page in &left_record.pages {
        if let Some(art_id) = page.artifact_id.as_deref() {
            left_pages.insert(page.number, art_id.to_owned());
            let raw = payloads
                .get(art_id)
                .ok_or_else(|| LineageError::UnknownReference {
                    context: "left page artifact_id",
                    target_id: art_id.to_owned(),
                })?;
            let text =
                std::str::from_utf8(raw).map_err(|_| LineageError::InvalidReleaseComparison {
                    comparison_id: comparison.comparison_id.clone(),
                    reason: "left artifact payload is not valid UTF-8",
                })?;
            for forbidden in &comparison.forbidden_cleartext_in_left {
                if text.contains(forbidden.as_str()) {
                    return Err(LineageError::HiddenRedactionPayloadRejected {
                        comparison_id: comparison.comparison_id.clone(),
                        artifact_id: art_id.to_owned(),
                        forbidden_cleartext: forbidden.clone(),
                    });
                }
            }
        }
    }

    let mut right_pages = BTreeMap::new();
    for page in &right_record.pages {
        if let Some(art_id) = page.artifact_id.as_deref() {
            right_pages.insert(page.number, art_id.to_owned());
        }
    }

    for segment in &comparison.segments {
        check_control().map_err(LineageError::Stopped)?;
        if segment.segment_kind == ComparisonSegmentKind::CoverageGap {
            if segment.left_byte_range.is_some() && segment.right_byte_range.is_some() {
                return Err(LineageError::InvalidReleaseComparison {
                    comparison_id: comparison.comparison_id.clone(),
                    reason: "coverage_gap segment must omit at least one side's byte range",
                });
            }
            continue;
        }
        let left_art_id = left_pages.get(&segment.page).ok_or_else(|| {
            LineageError::InvalidReleaseComparison {
                comparison_id: comparison.comparison_id.clone(),
                reason: "segment page missing on left record",
            }
        })?;
        let right_art_id = right_pages.get(&segment.page).ok_or_else(|| {
            LineageError::InvalidReleaseComparison {
                comparison_id: comparison.comparison_id.clone(),
                reason: "segment page missing on right record",
            }
        })?;
        let left_range = segment.left_byte_range.as_ref().ok_or_else(|| {
            LineageError::InvalidReleaseComparison {
                comparison_id: comparison.comparison_id.clone(),
                reason: "missing left_byte_range on non-gap segment",
            }
        })?;
        let right_range = segment.right_byte_range.as_ref().ok_or_else(|| {
            LineageError::InvalidReleaseComparison {
                comparison_id: comparison.comparison_id.clone(),
                reason: "missing right_byte_range on non-gap segment",
            }
        })?;
        let left_slice = extract_verified_utf8_slice(left_art_id, left_range, payloads)?;
        let right_slice = extract_verified_utf8_slice(right_art_id, right_range, payloads)?;
        if segment.left_text.as_deref() != Some(left_slice)
            || segment.right_text.as_deref() != Some(right_slice)
        {
            return Err(LineageError::InvalidReleaseComparison {
                comparison_id: comparison.comparison_id.clone(),
                reason: "segment text does not match exact UTF-8 byte range",
            });
        }
        match segment.segment_kind {
            ComparisonSegmentKind::UnchangedUnredacted => {
                if left_slice != right_slice {
                    return Err(LineageError::InvalidReleaseComparison {
                        comparison_id: comparison.comparison_id.clone(),
                        reason: "unchanged_unredacted segment text must match byte-for-byte",
                    });
                }
            }
            ComparisonSegmentKind::NewlyDisclosedInRight => {
                if !has_redaction_marker(left_slice) || has_redaction_marker(right_slice) {
                    return Err(LineageError::InvalidReleaseComparison {
                        comparison_id: comparison.comparison_id.clone(),
                        reason: "newly_disclosed_in_right must transition from redaction marker to disclosed text",
                    });
                }
            }
            ComparisonSegmentKind::NewlyRedactedInRight => {
                if has_redaction_marker(left_slice) || !has_redaction_marker(right_slice) {
                    return Err(LineageError::InvalidReleaseComparison {
                        comparison_id: comparison.comparison_id.clone(),
                        reason: "newly_redacted_in_right must transition from disclosed text to redaction marker",
                    });
                }
            }
            ComparisonSegmentKind::DifferentlyRedacted => {
                if left_slice == right_slice
                    || !has_redaction_marker(left_slice)
                    || !has_redaction_marker(right_slice)
                {
                    return Err(LineageError::InvalidReleaseComparison {
                        comparison_id: comparison.comparison_id.clone(),
                        reason: "differently_redacted segments must both contain distinct redaction markers",
                    });
                }
            }
            ComparisonSegmentKind::RevisedUnredacted => {
                if left_slice == right_slice
                    || has_redaction_marker(left_slice)
                    || has_redaction_marker(right_slice)
                {
                    return Err(LineageError::InvalidReleaseComparison {
                        comparison_id: comparison.comparison_id.clone(),
                        reason: "revised_unredacted segments must contain distinct unredacted text",
                    });
                }
            }
            ComparisonSegmentKind::CoverageGap => {}
        }
    }
    check_control().map_err(LineageError::Stopped)?;
    Ok(())
}

fn has_redaction_marker(text: &str) -> bool {
    text.contains("REDACTED") || text.contains("WITHHELD")
}

fn extract_verified_utf8_slice<'a>(
    artifact_id: &str,
    range: &Range<usize>,
    payloads: &'a BTreeMap<String, Vec<u8>>,
) -> Result<&'a str, LineageError> {
    let raw = payloads
        .get(artifact_id)
        .ok_or_else(|| LineageError::UnknownReference {
            context: "artifact_id",
            target_id: artifact_id.to_owned(),
        })?;
    if range.start >= range.end || range.end > raw.len() {
        return Err(LineageError::InvalidByteAnchorRange {
            artifact_id: artifact_id.to_owned(),
            start: range.start,
            end: range.end,
            byte_length: raw.len(),
        });
    }
    let full_text = std::str::from_utf8(raw).map_err(|_| LineageError::InvalidByteAnchorRange {
        artifact_id: artifact_id.to_owned(),
        start: range.start,
        end: range.end,
        byte_length: raw.len(),
    })?;
    full_text
        .get(range.start..range.end)
        .ok_or_else(|| LineageError::InvalidByteAnchorRange {
            artifact_id: artifact_id.to_owned(),
            start: range.start,
            end: range.end,
            byte_length: raw.len(),
        })
}

/// Run deterministic Tarjan strongly connected components decomposition over
/// `{Republishes, Quotes, Cites, Translates}` edges (`INTEL-01`, `A08`).
pub fn compute_lineage_sccs(
    records: &BTreeMap<String, SourceRecordDescriptor>,
    relations: &[RecordRelation],
    limits: LineageLimits,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<(BTreeMap<String, BTreeSet<String>>, LineageGraphReport), LineageError> {
    check_control().map_err(LineageError::Stopped)?;
    if records.len() > limits.max_nodes {
        return Err(LineageError::LineageGraphBudgetExceeded {
            limit: LineageLimit::Nodes,
            actual: records.len(),
            max: limits.max_nodes,
        });
    }
    if relations.len() > limits.max_edges {
        return Err(LineageError::LineageGraphBudgetExceeded {
            limit: LineageLimit::Edges,
            actual: relations.len(),
            max: limits.max_edges,
        });
    }

    let mut adj: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for node_id in records.keys() {
        adj.insert(node_id.clone(), BTreeSet::new());
    }
    for rel in relations {
        check_control().map_err(LineageError::Stopped)?;
        if rel.kind.is_lineage_edge() {
            if !adj.contains_key(&rel.from) {
                return Err(LineageError::UnknownReference {
                    context: "lineage relation from",
                    target_id: rel.from.clone(),
                });
            }
            if !adj.contains_key(&rel.to) {
                return Err(LineageError::UnknownReference {
                    context: "lineage relation to",
                    target_id: rel.to.clone(),
                });
            }
            if let Some(neighbors) = adj.get_mut(&rel.from) {
                neighbors.insert(rel.to.clone());
            }
        }
    }

    let mut state = TarjanState::default();
    let sorted_nodes: Vec<String> = adj.keys().cloned().collect();
    for node in &sorted_nodes {
        check_control().map_err(LineageError::Stopped)?;
        if !state.indices.contains_key(node) {
            strongconnect(node, &adj, &mut state, &mut check_control)?;
        }
    }

    let mut detected_cycles = Vec::new();
    for comp in state.sccs {
        check_control().map_err(LineageError::Stopped)?;
        let Some(first) = comp.first() else {
            continue;
        };
        let has_self_edge =
            comp.len() == 1 && adj.get(first).is_some_and(|outs| outs.contains(first));
        if comp.len() > 1 || has_self_edge {
            let comp_set: BTreeSet<String> = comp.iter().cloned().collect();
            let witness_path = build_cycle_witness(first, &comp_set, &adj)?;
            let has_external_outbound = comp.iter().any(|member| {
                adj.get(member).is_some_and(|outs| {
                    outs.iter()
                        .any(|target| !comp_set.contains(target.as_str()))
                })
            });
            let classification = if has_external_outbound {
                CycleClassification::AnchoredCycle
            } else {
                CycleClassification::UnanchoredCycle
            };
            detected_cycles
                .try_reserve(1)
                .map_err(|_| LineageError::AllocationFailed)?;
            detected_cycles.push(DetectedCycle {
                members: comp,
                witness_path,
                classification,
            });
        }
    }

    check_control().map_err(LineageError::Stopped)?;
    Ok((adj, LineageGraphReport { detected_cycles }))
}

#[derive(Default)]
struct TarjanState {
    index: usize,
    indices: BTreeMap<String, usize>,
    lowlink: BTreeMap<String, usize>,
    stack: Vec<String>,
    on_stack: BTreeSet<String>,
    sccs: Vec<Vec<String>>,
}

fn strongconnect(
    node: &str,
    adj: &BTreeMap<String, BTreeSet<String>>,
    state: &mut TarjanState,
    check_control: &mut impl FnMut() -> Result<(), StopReason>,
) -> Result<(), LineageError> {
    check_control().map_err(LineageError::Stopped)?;
    let current_index = state.index;
    state.index += 1;
    state.indices.insert(node.to_owned(), current_index);
    state.lowlink.insert(node.to_owned(), current_index);
    state
        .stack
        .try_reserve(1)
        .map_err(|_| LineageError::AllocationFailed)?;
    state.stack.push(node.to_owned());
    state.on_stack.insert(node.to_owned());

    if let Some(neighbors) = adj.get(node) {
        for neighbor in neighbors {
            if !state.indices.contains_key(neighbor) {
                strongconnect(neighbor, adj, state, check_control)?;
                let neighbor_low = state
                    .lowlink
                    .get(neighbor)
                    .copied()
                    .unwrap_or(current_index);
                if let Some(node_low) = state.lowlink.get_mut(node) {
                    *node_low = (*node_low).min(neighbor_low);
                }
            } else if state.on_stack.contains(neighbor) {
                let neighbor_idx = state
                    .indices
                    .get(neighbor)
                    .copied()
                    .unwrap_or(current_index);
                if let Some(node_low) = state.lowlink.get_mut(node) {
                    *node_low = (*node_low).min(neighbor_idx);
                }
            }
        }
    }

    let is_root = state.lowlink.get(node) == state.indices.get(node);
    if is_root {
        let mut component = Vec::new();
        while let Some(w) = state.stack.pop() {
            state.on_stack.remove(&w);
            component
                .try_reserve(1)
                .map_err(|_| LineageError::AllocationFailed)?;
            let done = w == node;
            component.push(w);
            if done {
                break;
            }
        }
        component.sort();
        state
            .sccs
            .try_reserve(1)
            .map_err(|_| LineageError::AllocationFailed)?;
        state.sccs.push(component);
    }
    Ok(())
}

fn build_cycle_witness(
    start: &str,
    comp_set: &BTreeSet<String>,
    adj: &BTreeMap<String, BTreeSet<String>>,
) -> Result<Vec<String>, LineageError> {
    let mut witness = Vec::new();
    witness
        .try_reserve(comp_set.len() + 1)
        .map_err(|_| LineageError::AllocationFailed)?;
    witness.push(start.to_owned());
    let mut curr = start.to_owned();
    let mut visited = BTreeSet::new();
    visited.insert(start.to_owned());

    while let Some(outs) = adj.get(&curr) {
        let candidates: Vec<&String> = outs.iter().filter(|nb| comp_set.contains(*nb)).collect();
        let Some(first_candidate) = candidates.first() else {
            break;
        };
        if candidates.iter().any(|nb| nb.as_str() == start) {
            witness
                .try_reserve(1)
                .map_err(|_| LineageError::AllocationFailed)?;
            witness.push(start.to_owned());
            break;
        }
        let chosen = candidates
            .iter()
            .find(|nb| !visited.contains(nb.as_str()))
            .copied()
            .unwrap_or(*first_candidate);
        witness
            .try_reserve(1)
            .map_err(|_| LineageError::AllocationFailed)?;
        witness.push(chosen.clone());
        if !visited.insert(chosen.clone()) {
            break;
        }
        curr = chosen.clone();
    }
    Ok(witness)
}

/// Evaluate corroboration independence and earliest-known-source candidates for a
/// set of citing inputs (`INTEL-01`, `DOC-05`, `A08`).
pub fn evaluate_corroboration(
    citing_inputs: &[String],
    records: &BTreeMap<String, SourceRecordDescriptor>,
    derivations: &BTreeMap<String, DerivationNode>,
    relations: &[RecordRelation],
    artifacts: &BTreeMap<String, ArtifactDescriptor>,
    limits: LineageLimits,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<CorroborationSummary, LineageError> {
    check_control().map_err(LineageError::Stopped)?;
    for version_id in records.keys() {
        if derivations.contains_key(version_id) {
            return Err(LineageError::DisjointVersionNamespaceViolation {
                version_id: version_id.clone(),
            });
        }
    }

    let (adj, graph_report) = compute_lineage_sccs(records, relations, limits, &mut check_control)?;
    let mut cyclic_members = BTreeSet::new();
    let mut cycle_witness_by_member: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for cycle in &graph_report.detected_cycles {
        for member in &cycle.members {
            cyclic_members.insert(member.clone());
            cycle_witness_by_member.insert(member.clone(), cycle.witness_path.clone());
        }
    }

    let mut supporting_record_versions = Vec::new();
    let mut excluded_derivative_runs = Vec::new();
    for item_id in citing_inputs {
        check_control().map_err(LineageError::Stopped)?;
        if derivations.contains_key(item_id) {
            excluded_derivative_runs
                .try_reserve(1)
                .map_err(|_| LineageError::AllocationFailed)?;
            excluded_derivative_runs.push(item_id.clone());
        } else if records.contains_key(item_id) {
            supporting_record_versions
                .try_reserve(1)
                .map_err(|_| LineageError::AllocationFailed)?;
            supporting_record_versions.push(item_id.clone());
        } else {
            return Err(LineageError::UnknownReference {
                context: "corroboration citing_input",
                target_id: item_id.clone(),
            });
        }
    }
    supporting_record_versions.sort();
    excluded_derivative_runs.sort();

    let mut circular_only_record_versions = Vec::new();
    let mut upstream_roots = BTreeSet::new();
    for rec_id in &supporting_record_versions {
        check_control().map_err(LineageError::Stopped)?;
        let mut visited = BTreeSet::new();
        let mut stack = Vec::new();
        stack
            .try_reserve(1)
            .map_err(|_| LineageError::AllocationFailed)?;
        stack.push(rec_id.clone());
        let mut reached_roots = BTreeSet::new();
        let mut hit_cycle = false;

        while let Some(curr) = stack.pop() {
            check_control().map_err(LineageError::Stopped)?;
            if !visited.insert(curr.clone()) {
                continue;
            }
            if cyclic_members.contains(&curr) {
                hit_cycle = true;
            }
            let outs = adj.get(&curr);
            let has_outs = outs.is_some_and(|set| !set.is_empty());
            if !has_outs {
                if !cyclic_members.contains(&curr) {
                    reached_roots.insert(curr);
                }
            } else if let Some(neighbors) = outs {
                for nxt in neighbors {
                    if !visited.contains(nxt) {
                        stack
                            .try_reserve(1)
                            .map_err(|_| LineageError::AllocationFailed)?;
                        stack.push(nxt.clone());
                    }
                }
            }
        }
        if reached_roots.is_empty() && hit_cycle {
            circular_only_record_versions
                .try_reserve(1)
                .map_err(|_| LineageError::AllocationFailed)?;
            circular_only_record_versions.push(rec_id.clone());
        }
        upstream_roots.extend(reached_roots);
    }
    circular_only_record_versions.sort();
    let primary_cycle_witness = circular_only_record_versions
        .first()
        .and_then(|member| cycle_witness_by_member.get(member).cloned());

    // Collapse exact-byte duplicates among upstream roots by SHA-256 signature.
    let mut independent_supporting_roots = Vec::new();
    let mut seen_hashes = BTreeSet::new();
    for root_id in upstream_roots {
        check_control().map_err(LineageError::Stopped)?;
        let rec = records
            .get(&root_id)
            .ok_or_else(|| LineageError::UnknownReference {
                context: "upstream root record",
                target_id: root_id.clone(),
            })?;
        let mut page_hashes = Vec::new();
        for page in &rec.pages {
            if let Some(art_id) = page.artifact_id.as_deref() {
                let art = artifacts
                    .get(art_id)
                    .ok_or_else(|| LineageError::UnknownReference {
                        context: "upstream root artifact",
                        target_id: art_id.to_owned(),
                    })?;
                page_hashes
                    .try_reserve(1)
                    .map_err(|_| LineageError::AllocationFailed)?;
                page_hashes.push(art.sha256.as_str());
            }
        }
        let hash_key = if page_hashes.is_empty() {
            format!("no-bytes:{root_id}")
        } else {
            page_hashes.join("|")
        };
        if seen_hashes.insert(hash_key) {
            independent_supporting_roots
                .try_reserve(1)
                .map_err(|_| LineageError::AllocationFailed)?;
            independent_supporting_roots.push(root_id);
        }
    }

    let mut dated_roots: Vec<(String, String)> = Vec::new();
    let mut undated_candidate_roots: Vec<String> = Vec::new();
    for root_id in &independent_supporting_roots {
        let rec = records
            .get(root_id)
            .ok_or_else(|| LineageError::UnknownReference {
                context: "independent root record",
                target_id: root_id.clone(),
            })?;
        if let Some(pub_time) = rec.publication_time.as_ref() {
            dated_roots
                .try_reserve(1)
                .map_err(|_| LineageError::AllocationFailed)?;
            dated_roots.push((pub_time.clone(), root_id.clone()));
        } else {
            let has_reason = rec
                .unknown_time_reason
                .as_deref()
                .is_some_and(|text| !text.trim().is_empty());
            if !has_reason {
                return Err(LineageError::MissingUnknownTimeReason {
                    record_version_id: root_id.clone(),
                });
            }
            undated_candidate_roots
                .try_reserve(1)
                .map_err(|_| LineageError::AllocationFailed)?;
            undated_candidate_roots.push(root_id.clone());
        }
    }
    dated_roots.sort();
    let earliest_dated_root = dated_roots.first().map(|(_, id)| id.clone());
    let resolution = if independent_supporting_roots.is_empty() {
        EarliestSourceResolution::NoIndependentRoots
    } else if !undated_candidate_roots.is_empty() {
        EarliestSourceResolution::AmbiguousCandidates
    } else {
        EarliestSourceResolution::SingleEarliestDated
    };

    let mut omitted_reference_only_records: Vec<String> = records
        .values()
        .filter(|rec| rec.retention == RetentionMode::ReferenceOnly)
        .map(|rec| rec.version_id.clone())
        .collect();
    omitted_reference_only_records.sort();

    let independent_root_count = independent_supporting_roots.len();
    check_control().map_err(LineageError::Stopped)?;
    Ok(CorroborationSummary {
        supporting_record_versions,
        excluded_derivative_runs,
        circular_only_record_versions,
        primary_cycle_witness,
        independent_supporting_roots,
        independent_root_count,
        earliest_known_source_view: EarliestKnownSourceView {
            resolution,
            earliest_dated_root,
            undated_candidate_roots,
            coverage_qualification: EarliestSourceCoverageQualification {
                scope: "retained_and_cataloged_workspace_records",
                omitted_reference_only_records,
                detected_cycle_count: graph_report.detected_cycles.len(),
            },
        },
    })
}

/// Validate that `derived_from` forms a strict DAG (`DerivationCycleDetected`) and
/// propagate `needs_reassessment` across downstream derivations without auto-retracting
/// removed endpoints (`DOC-05`, `NEWS-05`, `A03`, `A06`).
pub fn compute_invalidation_propagation(
    records: &BTreeMap<String, SourceRecordDescriptor>,
    derivations_in_order: &[DerivationNode],
    relations: &[RecordRelation],
    limits: LineageLimits,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<InvalidationPropagationReport, LineageError> {
    check_control().map_err(LineageError::Stopped)?;
    let total_nodes = records.len().saturating_add(derivations_in_order.len());
    if total_nodes > limits.max_nodes {
        return Err(LineageError::LineageGraphBudgetExceeded {
            limit: LineageLimit::Nodes,
            actual: total_nodes,
            max: limits.max_nodes,
        });
    }
    if relations.len() > limits.max_edges {
        return Err(LineageError::LineageGraphBudgetExceeded {
            limit: LineageLimit::Edges,
            actual: relations.len(),
            max: limits.max_edges,
        });
    }

    let mut derivations = BTreeMap::new();
    for deriv in derivations_in_order {
        check_control().map_err(LineageError::Stopped)?;
        if records.contains_key(&deriv.version_id) {
            return Err(LineageError::DisjointVersionNamespaceViolation {
                version_id: deriv.version_id.clone(),
            });
        }
        if derivations.contains_key(&deriv.version_id) {
            return Err(LineageError::DuplicateIdentifier {
                kind: "derivation version_id",
                id: deriv.version_id.clone(),
            });
        }
        if deriv.derived_from.is_empty() {
            return Err(LineageError::UnknownReference {
                context: "empty derived_from on derivation",
                target_id: deriv.version_id.clone(),
            });
        }
        derivations.insert(deriv.version_id.clone(), deriv.clone());
    }

    // 1. Verify derived_from forms a strict DAG (no cycles allowed in derivations).
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    for deriv_id in derivations.keys() {
        check_dag_node(
            deriv_id,
            records,
            &derivations,
            &mut visiting,
            &mut visited,
            &mut check_control,
        )?;
    }

    // 2. Identify invalidating transitions on SourceRecords (Supersedes and RetractedBy).
    let mut triggers: BTreeMap<String, (String, InvalidationReason, bool)> = BTreeMap::new();
    for rel in relations {
        check_control().map_err(LineageError::Stopped)?;
        match rel.kind {
            RelationKind::Supersedes => {
                let old_rec =
                    records
                        .get(&rel.to)
                        .ok_or_else(|| LineageError::UnknownReference {
                            context: "supersedes target",
                            target_id: rel.to.clone(),
                        })?;
                if !records.contains_key(&rel.from) {
                    return Err(LineageError::UnknownReference {
                        context: "supersedes replacement",
                        target_id: rel.from.clone(),
                    });
                }
                if old_rec.lifecycle_state != SourceLifecycleState::SupersededByCorrection {
                    return Err(LineageError::InvalidLifecycleTransition {
                        record_version_id: old_rec.version_id.clone(),
                        reason: "superseded record must have lifecycle_state SupersededByCorrection",
                    });
                }
                triggers.insert(
                    old_rec.version_id.clone(),
                    (
                        rel.from.clone(),
                        InvalidationReason::SourceCorrected,
                        old_rec.retention == RetentionMode::Retained,
                    ),
                );
            }
            RelationKind::RetractedBy => {
                let old_rec =
                    records
                        .get(&rel.to)
                        .ok_or_else(|| LineageError::UnknownReference {
                            context: "retracted_by target",
                            target_id: rel.to.clone(),
                        })?;
                if !records.contains_key(&rel.from) {
                    return Err(LineageError::UnknownReference {
                        context: "retracted_by notice",
                        target_id: rel.from.clone(),
                    });
                }
                if old_rec.lifecycle_state != SourceLifecycleState::Retracted {
                    return Err(LineageError::InvalidLifecycleTransition {
                        record_version_id: old_rec.version_id.clone(),
                        reason: "retracted record must have lifecycle_state Retracted",
                    });
                }
                triggers.insert(
                    old_rec.version_id.clone(),
                    (
                        rel.from.clone(),
                        InvalidationReason::SourceRetracted,
                        old_rec.retention == RetentionMode::Retained,
                    ),
                );
            }
            _ => {}
        }
    }

    // 3. Verify removed endpoints are NEVER auto-marked retracted (DOC-05, NEWS-05, A06).
    let mut removed_not_auto_retracted_record_versions = Vec::new();
    for rec in records.values() {
        check_control().map_err(LineageError::Stopped)?;
        if rec.availability == AvailabilityState::Removed {
            if rec.lifecycle_state == SourceLifecycleState::Retracted
                || triggers.contains_key(&rec.version_id)
            {
                return Err(LineageError::RemovedEndpointAutoRetracted {
                    record_version_id: rec.version_id.clone(),
                });
            }
            removed_not_auto_retracted_record_versions
                .try_reserve(1)
                .map_err(|_| LineageError::AllocationFailed)?;
            removed_not_auto_retracted_record_versions.push(rec.version_id.clone());
        }
    }
    removed_not_auto_retracted_record_versions.sort();

    // 4. Walk downstream derivations transitively.
    let mut needs_reassessment = Vec::new();
    let mut unaffected_fresh_versions = Vec::new();
    for deriv in derivations_in_order {
        check_control().map_err(LineageError::Stopped)?;
        let mut found_triggers = BTreeMap::new();
        let mut stack = Vec::new();
        stack
            .try_reserve(1)
            .map_err(|_| LineageError::AllocationFailed)?;
        stack.push(deriv.version_id.clone());
        let mut seen = BTreeSet::new();

        while let Some(curr) = stack.pop() {
            check_control().map_err(LineageError::Stopped)?;
            if !seen.insert(curr.clone()) {
                continue;
            }
            if let Some(trig) = triggers.get(&curr) {
                found_triggers.insert(curr.clone(), trig.clone());
            }
            if let Some(node) = derivations.get(&curr) {
                for parent in &node.derived_from {
                    stack
                        .try_reserve(1)
                        .map_err(|_| LineageError::AllocationFailed)?;
                    stack.push(parent.clone());
                }
            }
        }

        if found_triggers.is_empty() {
            unaffected_fresh_versions
                .try_reserve(1)
                .map_err(|_| LineageError::AllocationFailed)?;
            unaffected_fresh_versions.push(deriv.version_id.clone());
        } else {
            for (trigger_id, (replacement_id, reason, preserve_old_citations)) in found_triggers {
                needs_reassessment
                    .try_reserve(1)
                    .map_err(|_| LineageError::AllocationFailed)?;
                needs_reassessment.push(InvalidationNotice {
                    target_version_id: deriv.version_id.clone(),
                    trigger_record_version_id: trigger_id,
                    replacement_record_version_id: replacement_id,
                    reason,
                    preserve_old_citations,
                });
            }
        }
    }
    unaffected_fresh_versions.sort();
    check_control().map_err(LineageError::Stopped)?;
    Ok(InvalidationPropagationReport {
        needs_reassessment,
        unaffected_fresh_versions,
        removed_not_auto_retracted_record_versions,
    })
}

fn check_dag_node(
    node_id: &str,
    records: &BTreeMap<String, SourceRecordDescriptor>,
    derivations: &BTreeMap<String, DerivationNode>,
    visiting: &mut BTreeSet<String>,
    visited: &mut BTreeSet<String>,
    check_control: &mut impl FnMut() -> Result<(), StopReason>,
) -> Result<(), LineageError> {
    check_control().map_err(LineageError::Stopped)?;
    if visited.contains(node_id) {
        return Ok(());
    }
    if !visiting.insert(node_id.to_owned()) {
        return Err(LineageError::DerivationCycleDetected {
            version_id: node_id.to_owned(),
        });
    }
    if let Some(deriv) = derivations.get(node_id) {
        for parent_id in &deriv.derived_from {
            if !records.contains_key(parent_id) && !derivations.contains_key(parent_id) {
                return Err(LineageError::UnknownReference {
                    context: "derived_from target",
                    target_id: parent_id.clone(),
                });
            }
            check_dag_node(
                parent_id,
                records,
                derivations,
                visiting,
                visited,
                check_control,
            )?;
        }
    }
    visiting.remove(node_id);
    visited.insert(node_id.to_owned());
    Ok(())
}
