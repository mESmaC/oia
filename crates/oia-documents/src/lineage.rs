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

/// Maximum UTF-8 bytes accepted for one caller-supplied external bibliographic reference.
pub const MAX_EXTERNAL_CITATION_REFERENCE_BYTES: usize = 4_096;

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
    InvalidTimestamp {
        record_version_id: String,
        field: &'static str,
        value: String,
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
    InvalidExternalCitation {
        citing_record_version_id: String,
        reason: &'static str,
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
            Self::InvalidTimestamp {
                record_version_id,
                field,
                value,
            } => write!(
                formatter,
                "invalid RFC 3339 timestamp on {record_version_id:?} field {field}: {value:?}"
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
            Self::InvalidExternalCitation {
                citing_record_version_id,
                reason,
            } => write!(
                formatter,
                "invalid external citation on {citing_record_version_id:?}: {reason}"
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

    pub fn is_version_chain_edge(self) -> bool {
        matches!(
            self,
            Self::Supersedes | Self::AlternateRedaction | Self::PreprintVersion
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
pub struct UnresolvedExternalCitation {
    pub citing_record_version_id: String,
    pub reference: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EarliestSourceCoverageQualification {
    pub scope: &'static str,
    pub omitted_reference_only_records: Vec<String>,
    pub unresolved_external_citations: Vec<UnresolvedExternalCitation>,
    pub detected_cycle_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EarliestKnownSourceView {
    pub resolution: EarliestSourceResolution,
    pub earliest_dated_root: Option<String>,
    pub undated_candidate_roots: Vec<String>,
    pub coverage_qualification: EarliestSourceCoverageQualification,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ClaimCorroborationInputs<'a> {
    pub supporting: &'a [String],
    pub contradicting: &'a [String],
    pub external_citations: &'a [UnresolvedExternalCitation],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorroborationSummary {
    pub supporting_record_versions: Vec<String>,
    pub exact_duplicate_group_count: usize,
    pub excluded_derivative_runs: Vec<String>,
    pub circular_only_record_versions: Vec<String>,
    pub primary_cycle_witness: Option<Vec<String>>,
    pub independent_supporting_roots: Vec<String>,
    pub independent_root_count: usize,
    pub contradicting_independent_roots: Vec<String>,
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

/// Parse an RFC 3339 timestamp (`YYYY-MM-DDTHH:MM:SS[.fff](Z|+HH:MM|-HH:MM)`)
/// into UTC nanoseconds from the Unix epoch, preserving original strings for display
/// while rejecting malformed dates, missing offsets, or out-of-range fields.
pub fn parse_rfc3339_utc_nanos(
    record_version_id: &str,
    field: &'static str,
    input: &str,
) -> Result<i128, LineageError> {
    let err = || LineageError::InvalidTimestamp {
        record_version_id: record_version_id.to_owned(),
        field,
        value: input.to_owned(),
    };
    let parse_digits_u32 = |s: &str| -> Result<u32, LineageError> {
        if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
            return Err(err());
        }
        s.parse().map_err(|_| err())
    };
    if input.len() < 20 {
        return Err(err());
    }
    let year_slice = input.get(0..4).ok_or_else(err)?;
    if !year_slice.bytes().all(|b| b.is_ascii_digit()) {
        return Err(err());
    }
    let year: i64 = year_slice.parse().map_err(|_| err())?;
    if input.get(4..5) != Some("-") {
        return Err(err());
    }
    let month = parse_digits_u32(input.get(5..7).ok_or_else(err)?)?;
    if input.get(7..8) != Some("-") {
        return Err(err());
    }
    let day = parse_digits_u32(input.get(8..10).ok_or_else(err)?)?;
    let sep = input.get(10..11).ok_or_else(err)?;
    if sep != "T" && sep != "t" {
        return Err(err());
    }
    let hour = parse_digits_u32(input.get(11..13).ok_or_else(err)?)?;
    if input.get(13..14) != Some(":") {
        return Err(err());
    }
    let minute = parse_digits_u32(input.get(14..16).ok_or_else(err)?)?;
    if input.get(16..17) != Some(":") {
        return Err(err());
    }
    let second = parse_digits_u32(input.get(17..19).ok_or_else(err)?)?;

    let max_day = days_in_month(year, month).ok_or_else(err)?;
    if day == 0 || day > max_day || hour > 23 || minute > 59 || second > 60 {
        return Err(err());
    }

    let tail = input.get(19..).ok_or_else(err)?;
    let (nanos, tz_tail) = if let Some(after_dot) = tail.strip_prefix('.') {
        let tz_pos = after_dot.find(['Z', 'z', '+', '-']).ok_or_else(err)?;
        let frac = after_dot.get(..tz_pos).ok_or_else(err)?;
        if frac.is_empty() || frac.len() > 9 || !frac.bytes().all(|b| b.is_ascii_digit()) {
            return Err(err());
        }
        let mut parsed_frac: i128 = frac.parse().map_err(|_| err())?;
        for _ in frac.len()..9 {
            parsed_frac *= 10;
        }
        (parsed_frac, after_dot.get(tz_pos..).ok_or_else(err)?)
    } else {
        (0i128, tail)
    };

    let offset_seconds: i64 = if tz_tail == "Z" || tz_tail == "z" {
        0
    } else {
        if tz_tail.len() != 6 {
            return Err(err());
        }
        let sign: i64 = match tz_tail.get(0..1) {
            Some("+") => 1,
            Some("-") => -1,
            _ => return Err(err()),
        };
        if tz_tail.get(3..4) != Some(":") {
            return Err(err());
        }
        let off_h = parse_digits_u32(tz_tail.get(1..3).ok_or_else(err)?)?;
        let off_m = parse_digits_u32(tz_tail.get(4..6).ok_or_else(err)?)?;
        if off_h > 23 || off_m > 59 {
            return Err(err());
        }
        sign * (i64::from(off_h) * 3600 + i64::from(off_m) * 60)
    };

    let days = days_from_civil(year, month, day);
    let base_local_seconds = days * 86_400 + i64::from(hour) * 3600 + i64::from(minute) * 60;
    let base_utc_seconds = base_local_seconds - offset_seconds;
    if second == 60 && base_utc_seconds.rem_euclid(86_400) != 86_340 {
        return Err(err());
    }
    let utc_seconds = base_utc_seconds + i64::from(second.min(59));
    let leap_adjust = if second == 60 { 999_999_999 } else { nanos };
    Ok(i128::from(utc_seconds) * 1_000_000_000 + leap_adjust)
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn days_in_month(year: i64, month: u32) -> Option<u32> {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Some(31),
        4 | 6 | 9 | 11 => Some(30),
        2 => Some(if is_leap_year(year) { 29 } else { 28 }),
        _ => None,
    }
}

// Howard Hinnant's civil-to-days algorithm (days relative to 1970-01-01).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let m = i64::from(month);
    let d = i64::from(day);
    let y = year - i64::from(m <= 2);
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn record_priority_key(
    record: &SourceRecordDescriptor,
) -> Result<(u8, i128, i128, String), LineageError> {
    let retrieval_nanos =
        parse_rfc3339_utc_nanos(&record.version_id, "retrieval_time", &record.retrieval_time)?;
    if let Some(pub_time) = record.publication_time.as_deref() {
        let pub_nanos = parse_rfc3339_utc_nanos(&record.version_id, "publication_time", pub_time)?;
        Ok((0, pub_nanos, retrieval_nanos, record.version_id.clone()))
    } else {
        Ok((1, i128::MAX, retrieval_nanos, record.version_id.clone()))
    }
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
        parse_rfc3339_utc_nanos(&record.version_id, "retrieval_time", &record.retrieval_time)?;
        if let Some(event_time) = record.event_time.as_deref() {
            parse_rfc3339_utc_nanos(&record.version_id, "event_time", event_time)?;
        }
        if let Some(pub_time) = record.publication_time.as_deref() {
            parse_rfc3339_utc_nanos(&record.version_id, "publication_time", pub_time)?;
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
        if record.predecessor_version_id.as_deref() == Some(record.version_id.as_str()) {
            return Err(LineageError::InvalidLifecycleTransition {
                record_version_id: record.version_id.clone(),
                reason: "record cannot name itself as predecessor_version_id",
            });
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
                let mut seen_pages = BTreeSet::new();
                for page in &record.pages {
                    if page.number == 0 || !seen_pages.insert(page.number) {
                        return Err(LineageError::InvalidRetentionState {
                            record_version_id: record.version_id.clone(),
                            reason: "page numbers must be positive and unique within a record",
                        });
                    }
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
/// Members are ordered deterministically by earliest verified `publication_time`
/// instant, then `retrieval_time` instant, then lexical `version_id` (§2.2).
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
    type MemberSortKey = (u8, i128, i128, String);
    let mut by_hash: BTreeMap<String, Vec<(MemberSortKey, String, String, String)>> =
        BTreeMap::new();
    for record in records_in_order {
        check_control().map_err(LineageError::Stopped)?;
        if record.retention != RetentionMode::Retained {
            continue;
        }
        let sort_key = record_priority_key(record)?;
        let mut seen_record_hashes = BTreeSet::new();
        for page in &record.pages {
            if let Some(art_id) = page.artifact_id.as_deref() {
                let artifact =
                    artifacts
                        .get(art_id)
                        .ok_or_else(|| LineageError::UnknownReference {
                            context: "artifact_id",
                            target_id: art_id.to_owned(),
                        })?;
                if !seen_record_hashes.insert(artifact.sha256.clone()) {
                    continue;
                }
                let entry = by_hash.entry(artifact.sha256.clone()).or_default();
                entry
                    .try_reserve(1)
                    .map_err(|_| LineageError::AllocationFailed)?;
                entry.push((
                    sort_key.clone(),
                    art_id.to_owned(),
                    record.version_id.clone(),
                    record.source_id.clone(),
                ));
            }
        }
    }

    let mut groups = Vec::new();
    for (sha256, mut members) in by_hash {
        check_control().map_err(LineageError::Stopped)?;
        if members.len() > 1 {
            members.sort_by(|a, b| a.0.cmp(&b.0));
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
            for (_, art_id, version_id, source_id) in members {
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
/// ensuring neither caller-declared `forbidden_cleartext_in_left` nor segment-derived
/// disclosed/newly-redacted substrings leak inside the corresponding redacted artifact
/// (`DOC-02`, `DOC-04`, `A03`).
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
                if !forbidden.is_empty() && text.contains(forbidden.as_str()) {
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
            let raw = payloads
                .get(art_id)
                .ok_or_else(|| LineageError::UnknownReference {
                    context: "right page artifact_id",
                    target_id: art_id.to_owned(),
                })?;
            std::str::from_utf8(raw).map_err(|_| LineageError::InvalidReleaseComparison {
                comparison_id: comparison.comparison_id.clone(),
                reason: "right artifact payload is not valid UTF-8",
            })?;
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
                let disclosed = right_slice.trim();
                let left_full = full_artifact_str(left_art_id, payloads)?;
                if !disclosed.is_empty() && left_full.contains(disclosed) {
                    return Err(LineageError::HiddenRedactionPayloadRejected {
                        comparison_id: comparison.comparison_id.clone(),
                        artifact_id: left_art_id.clone(),
                        forbidden_cleartext: disclosed.to_owned(),
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
                let newly_redacted = left_slice.trim();
                let right_full = full_artifact_str(right_art_id, payloads)?;
                if !newly_redacted.is_empty() && right_full.contains(newly_redacted) {
                    return Err(LineageError::HiddenRedactionPayloadRejected {
                        comparison_id: comparison.comparison_id.clone(),
                        artifact_id: right_art_id.clone(),
                        forbidden_cleartext: newly_redacted.to_owned(),
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

fn full_artifact_str<'a>(
    artifact_id: &str,
    payloads: &'a BTreeMap<String, Vec<u8>>,
) -> Result<&'a str, LineageError> {
    let raw = payloads
        .get(artifact_id)
        .ok_or_else(|| LineageError::UnknownReference {
            context: "artifact_id",
            target_id: artifact_id.to_owned(),
        })?;
    std::str::from_utf8(raw).map_err(|_| LineageError::InvalidByteAnchorRange {
        artifact_id: artifact_id.to_owned(),
        start: 0,
        end: raw.len(),
        byte_length: raw.len(),
    })
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

/// Run deterministic, iterative Tarjan strongly connected components decomposition over
/// `{Republishes, Quotes, Cites, Translates}` edges (`INTEL-01`, `A08`).
/// Uses an explicit heap-allocated call stack bounded by `limits.max_nodes` so deep
/// lineage chains never overflow the thread call stack.
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
    for root in &sorted_nodes {
        check_control().map_err(LineageError::Stopped)?;
        if !state.indices.contains_key(root) {
            strongconnect_iterative(root, &adj, &mut state, &mut check_control)?;
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
            let witness_path = build_closed_cycle_witness(first, &comp_set, &adj)?;
            detected_cycles
                .try_reserve(1)
                .map_err(|_| LineageError::AllocationFailed)?;
            detected_cycles.push(DetectedCycle {
                members: comp,
                witness_path,
                classification: CycleClassification::UnanchoredCycle,
            });
        }
    }

    // A cycle is anchored only when its ancestry can reach a terminal,
    // non-cyclic upstream root. An outbound edge into another rootless cycle
    // does not provide an anchor.
    let cyclic_nodes: BTreeSet<String> = detected_cycles
        .iter()
        .flat_map(|cycle| cycle.members.iter().cloned())
        .collect();
    let mut reverse_adj: BTreeMap<String, Vec<String>> = adj
        .keys()
        .map(|node_id| (node_id.clone(), Vec::new()))
        .collect();
    for (from, targets) in &adj {
        check_control().map_err(LineageError::Stopped)?;
        for target in targets {
            if let Some(predecessors) = reverse_adj.get_mut(target) {
                predecessors
                    .try_reserve(1)
                    .map_err(|_| LineageError::AllocationFailed)?;
                predecessors.push(from.clone());
            }
        }
    }
    let mut reaches_terminal_root = BTreeSet::new();
    let mut root_stack = Vec::new();
    for (node_id, targets) in &adj {
        check_control().map_err(LineageError::Stopped)?;
        if targets.is_empty() && !cyclic_nodes.contains(node_id) {
            reaches_terminal_root.insert(node_id.clone());
            root_stack
                .try_reserve(1)
                .map_err(|_| LineageError::AllocationFailed)?;
            root_stack.push(node_id.clone());
        }
    }
    while let Some(curr) = root_stack.pop() {
        check_control().map_err(LineageError::Stopped)?;
        if let Some(predecessors) = reverse_adj.get(&curr) {
            for predecessor in predecessors {
                if reaches_terminal_root.insert(predecessor.clone()) {
                    root_stack
                        .try_reserve(1)
                        .map_err(|_| LineageError::AllocationFailed)?;
                    root_stack.push(predecessor.clone());
                }
            }
        }
    }
    for cycle in &mut detected_cycles {
        check_control().map_err(LineageError::Stopped)?;
        if cycle
            .members
            .iter()
            .any(|member| reaches_terminal_root.contains(member))
        {
            cycle.classification = CycleClassification::AnchoredCycle;
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

struct TarjanFrame {
    node: String,
    neighbors: Vec<String>,
    next_idx: usize,
}

fn enter_tarjan_node(
    node: &str,
    adj: &BTreeMap<String, BTreeSet<String>>,
    state: &mut TarjanState,
) -> Result<TarjanFrame, LineageError> {
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
    let neighbors = adj
        .get(node)
        .map(|set| set.iter().cloned().collect())
        .unwrap_or_default();
    Ok(TarjanFrame {
        node: node.to_owned(),
        neighbors,
        next_idx: 0,
    })
}

fn strongconnect_iterative(
    start_node: &str,
    adj: &BTreeMap<String, BTreeSet<String>>,
    state: &mut TarjanState,
    check_control: &mut impl FnMut() -> Result<(), StopReason>,
) -> Result<(), LineageError> {
    let mut work_stack: Vec<TarjanFrame> = Vec::new();
    work_stack
        .try_reserve(1)
        .map_err(|_| LineageError::AllocationFailed)?;
    work_stack.push(enter_tarjan_node(start_node, adj, state)?);

    while let Some(frame) = work_stack.last_mut() {
        check_control().map_err(LineageError::Stopped)?;
        if let Some(neighbor) = frame.neighbors.get(frame.next_idx).cloned() {
            frame.next_idx += 1;
            if !state.indices.contains_key(&neighbor) {
                let child_frame = enter_tarjan_node(&neighbor, adj, state)?;
                work_stack
                    .try_reserve(1)
                    .map_err(|_| LineageError::AllocationFailed)?;
                work_stack.push(child_frame);
            } else if state.on_stack.contains(&neighbor) {
                let neighbor_idx = state.indices.get(&neighbor).copied().unwrap_or(0);
                if let Some(node_low) = state.lowlink.get_mut(&frame.node) {
                    *node_low = (*node_low).min(neighbor_idx);
                }
            }
        } else {
            let Some(finished) = work_stack.pop() else {
                break;
            };
            let node_low = state.lowlink.get(&finished.node).copied().unwrap_or(0);
            let node_idx = state.indices.get(&finished.node).copied().unwrap_or(0);
            if node_low == node_idx {
                let mut component = Vec::new();
                while let Some(w) = state.stack.pop() {
                    state.on_stack.remove(&w);
                    component
                        .try_reserve(1)
                        .map_err(|_| LineageError::AllocationFailed)?;
                    let done = w == finished.node;
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
            if let Some(parent) = work_stack.last()
                && let Some(parent_low) = state.lowlink.get_mut(&parent.node)
            {
                *parent_low = (*parent_low).min(node_low);
            }
        }
    }
    Ok(())
}

// Find a shortest deterministic closed cycle witness `start -> ... -> start`
// inside the strongly connected component using BFS.
fn build_closed_cycle_witness(
    start: &str,
    comp_set: &BTreeSet<String>,
    adj: &BTreeMap<String, BTreeSet<String>>,
) -> Result<Vec<String>, LineageError> {
    if adj.get(start).is_some_and(|outs| outs.contains(start)) {
        return Ok(vec![start.to_owned(), start.to_owned()]);
    }
    let mut parent_map: BTreeMap<String, String> = BTreeMap::new();
    let mut queue: Vec<String> = Vec::new();
    queue
        .try_reserve(comp_set.len())
        .map_err(|_| LineageError::AllocationFailed)?;
    let mut head = 0usize;

    if let Some(outs) = adj.get(start) {
        for nb in outs {
            if comp_set.contains(nb) && nb != start {
                parent_map.insert(nb.clone(), start.to_owned());
                queue
                    .try_reserve(1)
                    .map_err(|_| LineageError::AllocationFailed)?;
                queue.push(nb.clone());
            }
        }
    }

    let mut closing_pred: Option<String> = None;
    'bfs: while let Some(curr) = queue.get(head).cloned() {
        head += 1;
        if let Some(outs) = adj.get(&curr) {
            for nb in outs {
                if !comp_set.contains(nb) {
                    continue;
                }
                if nb == start {
                    closing_pred = Some(curr);
                    break 'bfs;
                }
                if !parent_map.contains_key(nb) {
                    parent_map.insert(nb.clone(), curr.clone());
                    queue
                        .try_reserve(1)
                        .map_err(|_| LineageError::AllocationFailed)?;
                    queue.push(nb.clone());
                }
            }
        }
    }

    let mut witness = Vec::new();
    if let Some(last) = closing_pred {
        let mut cur = last;
        witness
            .try_reserve(comp_set.len() + 1)
            .map_err(|_| LineageError::AllocationFailed)?;
        witness.push(start.to_owned());
        loop {
            witness
                .try_reserve(1)
                .map_err(|_| LineageError::AllocationFailed)?;
            witness.push(cur.clone());
            if cur == start {
                break;
            }
            let Some(prev) = parent_map.get(&cur) else {
                break;
            };
            cur = prev.clone();
        }
        witness.reverse();
    } else {
        witness.push(start.to_owned());
        witness.push(start.to_owned());
    }
    Ok(witness)
}

/// Evaluate positive-support corroboration for compatibility with callers that
/// predate stance-separated claims. Use [`evaluate_claim_corroboration`] to
/// supply contradicting records and unresolved external citations.
pub fn evaluate_corroboration(
    citing_inputs: &[String],
    records: &BTreeMap<String, SourceRecordDescriptor>,
    derivations: &BTreeMap<String, DerivationNode>,
    relations: &[RecordRelation],
    artifacts: &BTreeMap<String, ArtifactDescriptor>,
    limits: LineageLimits,
    check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<CorroborationSummary, LineageError> {
    evaluate_claim_corroboration(
        ClaimCorroborationInputs {
            supporting: citing_inputs,
            ..ClaimCorroborationInputs::default()
        },
        records,
        derivations,
        relations,
        artifacts,
        limits,
        check_control,
    )
}

/// Evaluate stance-separated corroboration independence and earliest-known-source
/// candidates (`INTEL-01`, `DOC-05`, `A08`). Supporting and contradicting roots
/// are reported separately and never combined into a truth score. External
/// citations are explicit bibliographic inputs, not text scraped from documents;
/// only citations attached to reached claim ancestry enter coverage reporting.
pub fn evaluate_claim_corroboration(
    inputs: ClaimCorroborationInputs<'_>,
    records: &BTreeMap<String, SourceRecordDescriptor>,
    derivations: &BTreeMap<String, DerivationNode>,
    relations: &[RecordRelation],
    artifacts: &BTreeMap<String, ArtifactDescriptor>,
    limits: LineageLimits,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<CorroborationSummary, LineageError> {
    let ClaimCorroborationInputs {
        supporting: supporting_inputs,
        contradicting: contradicting_inputs,
        external_citations,
    } = inputs;
    check_control().map_err(LineageError::Stopped)?;
    let input_count = supporting_inputs
        .len()
        .checked_add(contradicting_inputs.len());
    if input_count.is_none_or(|count| count > limits.max_nodes) {
        return Err(LineageError::LineageGraphBudgetExceeded {
            limit: LineageLimit::Nodes,
            actual: input_count.unwrap_or(usize::MAX),
            max: limits.max_nodes,
        });
    }
    if external_citations.len() > limits.max_edges {
        return Err(LineageError::LineageGraphBudgetExceeded {
            limit: LineageLimit::Edges,
            actual: external_citations.len(),
            max: limits.max_edges,
        });
    }
    for version_id in records.keys() {
        if derivations.contains_key(version_id) {
            return Err(LineageError::DisjointVersionNamespaceViolation {
                version_id: version_id.clone(),
            });
        }
    }

    let (adj, graph_report) = compute_lineage_sccs(records, relations, limits, &mut check_control)?;
    let mut cyclic_members = BTreeSet::new();
    let mut cycle_idx_by_member: BTreeMap<String, usize> = BTreeMap::new();
    for (idx, cycle) in graph_report.detected_cycles.iter().enumerate() {
        for member in &cycle.members {
            cyclic_members.insert(member.clone());
            cycle_idx_by_member.insert(member.clone(), idx);
        }
    }

    let mut supporting_set = BTreeSet::new();
    let mut contradicting_set = BTreeSet::new();
    let mut excluded_set = BTreeSet::new();
    for (stance, inputs) in [
        ("supporting", supporting_inputs),
        ("contradicting", contradicting_inputs),
    ] {
        for item_id in inputs {
            check_control().map_err(LineageError::Stopped)?;
            if derivations.contains_key(item_id) {
                excluded_set.insert(item_id.clone());
            } else if records.contains_key(item_id) {
                if stance == "supporting" {
                    supporting_set.insert(item_id.clone());
                } else {
                    contradicting_set.insert(item_id.clone());
                }
            } else {
                return Err(LineageError::UnknownReference {
                    context: if stance == "supporting" {
                        "corroboration citing_input"
                    } else {
                        "corroboration contradicting_input"
                    },
                    target_id: item_id.clone(),
                });
            }
        }
    }
    let supporting_record_versions: Vec<String> = supporting_set.into_iter().collect();
    let contradicting_record_versions: Vec<String> = contradicting_set.into_iter().collect();
    let excluded_derivative_runs: Vec<String> = excluded_set.into_iter().collect();

    let supporting_trace = trace_claim_ancestry(
        &supporting_record_versions,
        &adj,
        &cyclic_members,
        &cycle_idx_by_member,
        &mut check_control,
    )?;
    let contradicting_trace = trace_claim_ancestry(
        &contradicting_record_versions,
        &adj,
        &cyclic_members,
        &cycle_idx_by_member,
        &mut check_control,
    )?;
    let circular_only_record_versions = supporting_trace.circular_only_record_versions.clone();
    let primary_cycle_witness = circular_only_record_versions
        .first()
        .and_then(|member| {
            cycle_idx_by_member
                .get(member)
                .or_else(|| supporting_trace.cycle_idx_by_circular_input.get(member))
        })
        .and_then(|&idx| graph_report.detected_cycles.get(idx))
        .map(|cycle| cycle.witness_path.clone());

    let mut supporting_content_hashes = BTreeSet::new();
    for record_id in &supporting_record_versions {
        check_control().map_err(LineageError::Stopped)?;
        let record = records
            .get(record_id)
            .ok_or_else(|| LineageError::UnknownReference {
                context: "supporting record",
                target_id: record_id.clone(),
            })?;
        if record.retention != RetentionMode::Retained {
            continue;
        }
        for page in &record.pages {
            check_control().map_err(LineageError::Stopped)?;
            let Some(artifact_id) = page.artifact_id.as_deref() else {
                continue;
            };
            let artifact =
                artifacts
                    .get(artifact_id)
                    .ok_or_else(|| LineageError::UnknownReference {
                        context: "supporting record artifact",
                        target_id: artifact_id.to_owned(),
                    })?;
            supporting_content_hashes.insert(artifact.sha256.clone());
        }
    }
    let exact_duplicate_group_count = supporting_content_hashes.len();

    // Build version-chain equivalence classes over `{Supersedes, AlternateRedaction, PreprintVersion}`,
    // `predecessor_version_id`, and shared document `id`.
    let version_chain_rep = build_version_chain_representatives(records, relations)?;

    let independent_supporting_roots = collapse_independent_roots(
        &supporting_trace.upstream_roots,
        records,
        &version_chain_rep,
        artifacts,
        &mut check_control,
    )?;
    let contradicting_independent_roots = collapse_independent_roots(
        &contradicting_trace.upstream_roots,
        records,
        &version_chain_rep,
        artifacts,
        &mut check_control,
    )?;

    let mut dated_roots: Vec<(i128, String)> = Vec::new();
    let mut undated_candidate_roots: Vec<String> = Vec::new();
    for root_id in &independent_supporting_roots {
        let rec = records
            .get(root_id)
            .ok_or_else(|| LineageError::UnknownReference {
                context: "independent root record",
                target_id: root_id.clone(),
            })?;
        if let Some(pub_time) = rec.publication_time.as_deref() {
            let pub_nanos = parse_rfc3339_utc_nanos(root_id, "publication_time", pub_time)?;
            dated_roots
                .try_reserve(1)
                .map_err(|_| LineageError::AllocationFailed)?;
            dated_roots.push((pub_nanos, root_id.clone()));
        } else {
            undated_candidate_roots
                .try_reserve(1)
                .map_err(|_| LineageError::AllocationFailed)?;
            undated_candidate_roots.push(root_id.clone());
        }
    }
    dated_roots.sort();
    let earliest_dated_root = dated_roots.first().map(|(_, id)| id.clone());
    let has_earliest_tie = match (dated_roots.first(), dated_roots.get(1)) {
        (Some((first_ts, _)), Some((second_ts, _))) => first_ts == second_ts,
        _ => false,
    };
    let resolution = if independent_supporting_roots.is_empty() {
        EarliestSourceResolution::NoIndependentRoots
    } else if !undated_candidate_roots.is_empty() || has_earliest_tie {
        EarliestSourceResolution::AmbiguousCandidates
    } else {
        EarliestSourceResolution::SingleEarliestDated
    };

    let reached_records: BTreeSet<String> = supporting_trace
        .reached_records
        .union(&contradicting_trace.reached_records)
        .cloned()
        .collect();
    let encountered_cycles: BTreeSet<usize> = supporting_trace
        .encountered_cycles
        .union(&contradicting_trace.encountered_cycles)
        .copied()
        .collect();
    let mut omitted_reference_only_records = Vec::new();
    for record_id in &reached_records {
        check_control().map_err(LineageError::Stopped)?;
        let record = records
            .get(record_id)
            .ok_or_else(|| LineageError::UnknownReference {
                context: "reached coverage record",
                target_id: record_id.clone(),
            })?;
        if record.retention == RetentionMode::ReferenceOnly
            || matches!(
                record.availability,
                AvailabilityState::ReferenceOnly
                    | AvailabilityState::Restricted
                    | AvailabilityState::Missing
            )
        {
            omitted_reference_only_records
                .try_reserve(1)
                .map_err(|_| LineageError::AllocationFailed)?;
            omitted_reference_only_records.push(record_id.clone());
        }
    }

    let mut unresolved_external_citations = Vec::new();
    for citation in external_citations {
        check_control().map_err(LineageError::Stopped)?;
        if !records.contains_key(&citation.citing_record_version_id) {
            return Err(LineageError::UnknownReference {
                context: "external citation citing_record_version_id",
                target_id: citation.citing_record_version_id.clone(),
            });
        }
        if citation.reference.trim().is_empty() {
            return Err(LineageError::InvalidExternalCitation {
                citing_record_version_id: citation.citing_record_version_id.clone(),
                reason: "bibliographic reference must not be empty",
            });
        }
        if citation.reference.len() > MAX_EXTERNAL_CITATION_REFERENCE_BYTES {
            return Err(LineageError::InvalidExternalCitation {
                citing_record_version_id: citation.citing_record_version_id.clone(),
                reason: "bibliographic reference exceeds 4096 UTF-8 bytes",
            });
        }
        if reached_records.contains(&citation.citing_record_version_id) {
            unresolved_external_citations
                .try_reserve(1)
                .map_err(|_| LineageError::AllocationFailed)?;
            unresolved_external_citations.push(citation.clone());
        }
    }
    unresolved_external_citations.sort_by(|left, right| {
        left.citing_record_version_id
            .cmp(&right.citing_record_version_id)
            .then_with(|| left.reference.cmp(&right.reference))
    });
    unresolved_external_citations.dedup();

    let independent_root_count = independent_supporting_roots.len();
    check_control().map_err(LineageError::Stopped)?;
    Ok(CorroborationSummary {
        supporting_record_versions,
        exact_duplicate_group_count,
        excluded_derivative_runs,
        circular_only_record_versions,
        primary_cycle_witness,
        independent_supporting_roots,
        independent_root_count,
        contradicting_independent_roots,
        earliest_known_source_view: EarliestKnownSourceView {
            resolution,
            earliest_dated_root,
            undated_candidate_roots,
            coverage_qualification: EarliestSourceCoverageQualification {
                scope: "retained_and_cataloged_workspace_records",
                omitted_reference_only_records,
                unresolved_external_citations,
                detected_cycle_count: encountered_cycles.len(),
            },
        },
    })
}

#[derive(Default)]
struct ClaimAncestryTrace {
    upstream_roots: BTreeSet<String>,
    reached_records: BTreeSet<String>,
    encountered_cycles: BTreeSet<usize>,
    circular_only_record_versions: Vec<String>,
    cycle_idx_by_circular_input: BTreeMap<String, usize>,
}

fn trace_claim_ancestry(
    record_ids: &[String],
    adj: &BTreeMap<String, BTreeSet<String>>,
    cyclic_members: &BTreeSet<String>,
    cycle_idx_by_member: &BTreeMap<String, usize>,
    check_control: &mut impl FnMut() -> Result<(), StopReason>,
) -> Result<ClaimAncestryTrace, LineageError> {
    let mut trace = ClaimAncestryTrace::default();
    for rec_id in record_ids {
        check_control().map_err(LineageError::Stopped)?;
        let mut visited = BTreeSet::new();
        let mut stack = Vec::new();
        stack
            .try_reserve(1)
            .map_err(|_| LineageError::AllocationFailed)?;
        stack.push(rec_id.clone());
        let mut reached_roots = BTreeSet::new();
        let mut reached_cycle_indices = BTreeSet::new();

        while let Some(curr) = stack.pop() {
            check_control().map_err(LineageError::Stopped)?;
            if !visited.insert(curr.clone()) {
                continue;
            }
            trace.reached_records.insert(curr.clone());
            if cyclic_members.contains(&curr)
                && let Some(cycle_idx) = cycle_idx_by_member.get(&curr)
            {
                trace.encountered_cycles.insert(*cycle_idx);
                reached_cycle_indices.insert(*cycle_idx);
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
        if reached_roots.is_empty()
            && let Some(cycle_idx) = reached_cycle_indices.first()
        {
            trace
                .circular_only_record_versions
                .try_reserve(1)
                .map_err(|_| LineageError::AllocationFailed)?;
            trace.circular_only_record_versions.push(rec_id.clone());
            trace
                .cycle_idx_by_circular_input
                .insert(rec_id.clone(), *cycle_idx);
        }
        trace.upstream_roots.extend(reached_roots);
    }
    trace.circular_only_record_versions.sort();
    Ok(trace)
}

fn collapse_independent_roots(
    upstream_roots: &BTreeSet<String>,
    records: &BTreeMap<String, SourceRecordDescriptor>,
    version_chain_rep: &BTreeMap<String, String>,
    artifacts: &BTreeMap<String, ArtifactDescriptor>,
    check_control: &mut impl FnMut() -> Result<(), StopReason>,
) -> Result<Vec<String>, LineageError> {
    // Sort candidate upstream roots by earliest verified publication instant -> retrieval instant -> lexical version_id
    // before collapsing transitive equivalence classes over shared hash, version chain, and `source_id`.
    let mut prioritized_roots: Vec<((u8, i128, i128, String), String)> = Vec::new();
    prioritized_roots
        .try_reserve(upstream_roots.len())
        .map_err(|_| LineageError::AllocationFailed)?;
    for root_id in upstream_roots {
        check_control().map_err(LineageError::Stopped)?;
        let rec = records
            .get(root_id)
            .ok_or_else(|| LineageError::UnknownReference {
                context: "upstream root record",
                target_id: root_id.clone(),
            })?;
        if rec.publication_time.is_none() {
            let has_reason = rec
                .unknown_time_reason
                .as_deref()
                .is_some_and(|text| !text.trim().is_empty());
            if !has_reason {
                return Err(LineageError::MissingUnknownTimeReason {
                    record_version_id: root_id.clone(),
                });
            }
        }
        let key = record_priority_key(rec)?;
        prioritized_roots.push((key, root_id.clone()));
    }
    prioritized_roots.sort_by(|a, b| a.0.cmp(&b.0));

    let ordered_root_ids: Vec<String> = prioritized_roots.into_iter().map(|(_, id)| id).collect();
    let mut root_parent: BTreeMap<String, String> = ordered_root_ids
        .iter()
        .map(|id| (id.clone(), id.clone()))
        .collect();
    let mut first_by_hash: BTreeMap<String, String> = BTreeMap::new();
    let mut first_by_chain: BTreeMap<String, String> = BTreeMap::new();
    let mut first_by_source: BTreeMap<String, String> = BTreeMap::new();

    for root_id in &ordered_root_ids {
        check_control().map_err(LineageError::Stopped)?;
        let rec = records
            .get(root_id)
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
        let chain_key = version_chain_rep
            .get(root_id)
            .cloned()
            .unwrap_or_else(|| root_id.clone());
        let source_key = rec.source_id.clone();

        if let Some(prev) = first_by_hash.get(&hash_key).cloned() {
            union_str_rep(root_id, &prev, &mut root_parent);
        } else {
            first_by_hash.insert(hash_key, root_id.clone());
        }
        if let Some(prev) = first_by_chain.get(&chain_key).cloned() {
            union_str_rep(root_id, &prev, &mut root_parent);
        } else {
            first_by_chain.insert(chain_key, root_id.clone());
        }
        if let Some(prev) = first_by_source.get(&source_key).cloned() {
            union_str_rep(root_id, &prev, &mut root_parent);
        } else {
            first_by_source.insert(source_key, root_id.clone());
        }
    }

    let mut independent_roots = Vec::new();
    let mut emitted_classes = BTreeSet::new();
    for root_id in ordered_root_ids {
        check_control().map_err(LineageError::Stopped)?;
        let class_rep = find_str_rep(&root_id, &mut root_parent);
        if emitted_classes.insert(class_rep) {
            independent_roots
                .try_reserve(1)
                .map_err(|_| LineageError::AllocationFailed)?;
            independent_roots.push(root_id);
        }
    }
    independent_roots.sort();
    Ok(independent_roots)
}

fn find_str_rep(node: &str, parent: &mut BTreeMap<String, String>) -> String {
    let mut curr = node.to_owned();
    while let Some(next) = parent.get(&curr).cloned() {
        if next == curr {
            break;
        }
        if let Some(grand) = parent.get(&next).cloned() {
            parent.insert(curr.clone(), grand.clone());
            curr = grand;
        } else {
            break;
        }
    }
    curr
}

fn union_str_rep(a: &str, b: &str, parent: &mut BTreeMap<String, String>) {
    let ra = find_str_rep(a, parent);
    let rb = find_str_rep(b, parent);
    if ra != rb {
        if ra < rb {
            parent.insert(rb, ra);
        } else {
            parent.insert(ra, rb);
        }
    }
}

fn build_version_chain_representatives(
    records: &BTreeMap<String, SourceRecordDescriptor>,
    relations: &[RecordRelation],
) -> Result<BTreeMap<String, String>, LineageError> {
    let mut parent: BTreeMap<String, String> =
        records.keys().map(|k| (k.clone(), k.clone())).collect();

    let mut by_doc_id: BTreeMap<String, String> = BTreeMap::new();
    for (ver_id, rec) in records {
        if let Some(existing) = by_doc_id.get(&rec.id).cloned() {
            union_str_rep(ver_id, &existing, &mut parent);
        } else {
            by_doc_id.insert(rec.id.clone(), ver_id.clone());
        }
        if let Some(pred) = rec.predecessor_version_id.as_deref()
            && records.contains_key(pred)
        {
            union_str_rep(ver_id, pred, &mut parent);
        }
    }
    for rel in relations {
        if rel.kind.is_version_chain_edge()
            && records.contains_key(&rel.from)
            && records.contains_key(&rel.to)
        {
            union_str_rep(&rel.from, &rel.to, &mut parent);
        }
    }

    let keys: Vec<String> = parent.keys().cloned().collect();
    let mut resolved = BTreeMap::new();
    for key in keys {
        let rep = find_str_rep(&key, &mut parent);
        resolved.insert(key, rep);
    }
    Ok(resolved)
}

/// Validate that `derived_from` forms a strict DAG (`DerivationCycleDetected`) using an
/// explicit iterative DFS stack and propagate `needs_reassessment` across downstream
/// derivations without auto-retracting removed endpoints (`DOC-05`, `NEWS-05`, `A03`, `A06`).
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
    let derived_edge_count: usize = derivations_in_order
        .iter()
        .map(|d| d.derived_from.len())
        .sum();
    let total_edges = relations.len().saturating_add(derived_edge_count);
    if total_edges > limits.max_edges {
        return Err(LineageError::LineageGraphBudgetExceeded {
            limit: LineageLimit::Edges,
            actual: total_edges,
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

    // 1. Verify derived_from forms a strict DAG (iterative three-color DFS; no recursion).
    validate_dag_iterative(records, &derivations, &mut check_control)?;

    // 2. Identify invalidating transitions on SourceRecords (Supersedes, RetractedBy, and lifecycle_state).
    type TriggerEntry = (String, InvalidationReason, bool);
    let mut triggers: BTreeMap<String, Vec<TriggerEntry>> = BTreeMap::new();
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
                let list = triggers.entry(old_rec.version_id.clone()).or_default();
                list.try_reserve(1)
                    .map_err(|_| LineageError::AllocationFailed)?;
                list.push((
                    rel.from.clone(),
                    InvalidationReason::SourceCorrected,
                    old_rec.retention == RetentionMode::Retained,
                ));
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
                let list = triggers.entry(old_rec.version_id.clone()).or_default();
                list.try_reserve(1)
                    .map_err(|_| LineageError::AllocationFailed)?;
                list.push((
                    rel.from.clone(),
                    InvalidationReason::SourceRetracted,
                    old_rec.retention == RetentionMode::Retained,
                ));
            }
            _ => {}
        }
    }

    // 3. Verify removed endpoints are NEVER auto-marked retracted (DOC-05, NEWS-05, A06),
    // and ensure any record in Retracted or SupersededByCorrection state triggers reassessment
    // even when retracted in-place without a separate notice record (Contract §6.1(2)).
    let mut removed_not_auto_retracted_record_versions = Vec::new();
    for rec in records.values() {
        check_control().map_err(LineageError::Stopped)?;
        let has_retraction_trigger = triggers.get(&rec.version_id).is_some_and(|list| {
            list.iter()
                .any(|(_, reason, _)| *reason == InvalidationReason::SourceRetracted)
        });
        if rec.availability == AvailabilityState::Removed {
            if rec.lifecycle_state == SourceLifecycleState::Retracted || has_retraction_trigger {
                return Err(LineageError::RemovedEndpointAutoRetracted {
                    record_version_id: rec.version_id.clone(),
                });
            }
            removed_not_auto_retracted_record_versions
                .try_reserve(1)
                .map_err(|_| LineageError::AllocationFailed)?;
            removed_not_auto_retracted_record_versions.push(rec.version_id.clone());
        }
        match rec.lifecycle_state {
            SourceLifecycleState::Retracted => {
                if !triggers.contains_key(&rec.version_id) {
                    let list = triggers.entry(rec.version_id.clone()).or_default();
                    list.try_reserve(1)
                        .map_err(|_| LineageError::AllocationFailed)?;
                    list.push((
                        rec.version_id.clone(),
                        InvalidationReason::SourceRetracted,
                        rec.retention == RetentionMode::Retained,
                    ));
                }
            }
            SourceLifecycleState::SupersededByCorrection => {
                if !triggers.contains_key(&rec.version_id) {
                    return Err(LineageError::InvalidLifecycleTransition {
                        record_version_id: rec.version_id.clone(),
                        reason: "record with lifecycle_state SupersededByCorrection requires a Supersedes relation",
                    });
                }
            }
            SourceLifecycleState::Active => {}
        }
    }
    removed_not_auto_retracted_record_versions.sort();

    // 4. Walk downstream derivations transitively.
    let mut needs_reassessment = Vec::new();
    let mut unaffected_fresh_versions = Vec::new();
    for deriv in derivations_in_order {
        check_control().map_err(LineageError::Stopped)?;
        let mut found_triggers: BTreeMap<String, Vec<TriggerEntry>> = BTreeMap::new();
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
            if let Some(trig_list) = triggers.get(&curr) {
                found_triggers.insert(curr.clone(), trig_list.clone());
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
            for (trigger_id, trig_list) in found_triggers {
                for (replacement_id, reason, preserve_old_citations) in trig_list {
                    needs_reassessment
                        .try_reserve(1)
                        .map_err(|_| LineageError::AllocationFailed)?;
                    needs_reassessment.push(InvalidationNotice {
                        target_version_id: deriv.version_id.clone(),
                        trigger_record_version_id: trigger_id.clone(),
                        replacement_record_version_id: replacement_id,
                        reason,
                        preserve_old_citations,
                    });
                }
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

struct DagFrame {
    node_id: String,
    parents: Vec<String>,
    next_idx: usize,
}

fn validate_dag_iterative(
    records: &BTreeMap<String, SourceRecordDescriptor>,
    derivations: &BTreeMap<String, DerivationNode>,
    check_control: &mut impl FnMut() -> Result<(), StopReason>,
) -> Result<(), LineageError> {
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    let mut work_stack: Vec<DagFrame> = Vec::new();

    for start_id in derivations.keys() {
        check_control().map_err(LineageError::Stopped)?;
        if visited.contains(start_id) {
            continue;
        }
        let Some(start_node) = derivations.get(start_id) else {
            continue;
        };
        visiting.insert(start_id.clone());
        work_stack
            .try_reserve(1)
            .map_err(|_| LineageError::AllocationFailed)?;
        work_stack.push(DagFrame {
            node_id: start_id.clone(),
            parents: start_node.derived_from.clone(),
            next_idx: 0,
        });

        while let Some(frame) = work_stack.last_mut() {
            check_control().map_err(LineageError::Stopped)?;
            if let Some(parent_id) = frame.parents.get(frame.next_idx).cloned() {
                frame.next_idx += 1;
                if !records.contains_key(&parent_id) && !derivations.contains_key(&parent_id) {
                    return Err(LineageError::UnknownReference {
                        context: "derived_from target",
                        target_id: parent_id,
                    });
                }
                if visited.contains(&parent_id) {
                    continue;
                }
                if visiting.contains(&parent_id) {
                    return Err(LineageError::DerivationCycleDetected {
                        version_id: parent_id,
                    });
                }
                if let Some(parent_deriv) = derivations.get(&parent_id) {
                    visiting.insert(parent_id.clone());
                    work_stack
                        .try_reserve(1)
                        .map_err(|_| LineageError::AllocationFailed)?;
                    work_stack.push(DagFrame {
                        node_id: parent_id,
                        parents: parent_deriv.derived_from.clone(),
                        next_idx: 0,
                    });
                } else {
                    visited.insert(parent_id);
                }
            } else {
                let Some(done) = work_stack.pop() else {
                    break;
                };
                visiting.remove(&done.node_id);
                visited.insert(done.node_id);
            }
        }
    }
    Ok(())
}
