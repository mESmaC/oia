//! Bounded, exact UTF-8 snapshot comparison (`PRIVACY-03`, #48).
//!
//! These borrowed inputs and results are transient analysis, not a source registry,
//! evidence store or notification transport. Callers supply authorized, versioned
//! captures. Persistence must use the common domain envelope when #10/#11 land.

use oia_domain::{OpaqueId, VersionId};
use std::fmt;
use std::ops::Range;
use std::time::{Duration, SystemTime};

/// Hard bound per capture; oversized text is rejected, never truncated.
pub const MAX_TEXT_BYTES: usize = 1_048_576;
/// Hard bound for attribution and partial-capture explanation fields.
pub const MAX_DESCRIPTION_BYTES: usize = 4_096;
/// Pin this method alongside rule and capture versions when persisting results.
pub const METHOD_ID: &str = "exact-utf8-prefix-suffix-v1";
const CONTROL_INTERVAL: usize = 1_024;

/// Compare only captures of the same resource in the same workspace and source.
/// Identity is caller-established; matching these IDs does not grant access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotScope {
    pub workspace: OpaqueId,
    pub source: OpaqueId,
    pub resource: OpaqueId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnavailableReason {
    Missing,
    Restricted,
    RateLimited,
    AcquisitionFailed,
    NotRetained,
    Malformed,
}

/// Completeness concerns this exact captured representation, not source coverage
/// outside it. An empty complete capture differs from an unavailable capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capture<'text> {
    Complete(&'text str),
    Partial {
        text: &'text str,
        reason: &'text str,
    },
    Unavailable(UnavailableReason),
}

/// Caller-supplied provenance. Retrieval is not publication, revision or event time.
/// Exact retrieval instants only; uncertain/unknown clocks need the shared time
/// contract before they can be used by this rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotInput<'text> {
    pub scope: SnapshotScope,
    pub version: VersionId,
    /// Source reference and credit, supplied without credentials or private tokens.
    pub attribution: &'text str,
    pub retrieved_at: SystemTime,
    pub capture: Capture<'text>,
}

/// Validated immutable view of caller-owned UTF-8 bytes. No normalization occurs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot<'text>(SnapshotInput<'text>);

/// A pinned rule: any exact text change, with a maximum current retrieval age.
/// The version identifies the caller's saved rule; construction does not save it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeRule {
    scope: SnapshotScope,
    version: VersionId,
    max_retrieval_age: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    Cancelled,
    DeadlineExceeded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeError {
    EmptyDescription(&'static str),
    DescriptionTooLong(&'static str),
    TextTooLarge {
        actual_bytes: usize,
    },
    ZeroMaxAge,
    ScopeMismatch,
    ReusedSnapshotVersion,
    ReversedRetrievalOrder,
    EvaluationBeforeRetrieval,
    InvalidExplanationLimits(&'static str),
    LineLimitExceeded {
        actual_lines: usize,
        max_lines: usize,
    },
    HunkLimitExceeded {
        max_hunks: usize,
    },
    AllocationFailed,
    Stopped(StopReason),
}

impl fmt::Display for ChangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyDescription(field) => write!(f, "{field} must contain text"),
            Self::DescriptionTooLong(field) => {
                write!(f, "{field} exceeds {MAX_DESCRIPTION_BYTES} UTF-8 bytes")
            }
            Self::TextTooLarge { actual_bytes } => {
                write!(
                    f,
                    "capture has {actual_bytes} bytes; limit is {MAX_TEXT_BYTES}"
                )
            }
            Self::ZeroMaxAge => f.write_str("maximum retrieval age must be positive"),
            Self::ScopeMismatch => {
                f.write_str("rule and captures must share workspace/source/resource")
            }
            Self::ReusedSnapshotVersion => {
                f.write_str("captures must have distinct immutable versions")
            }
            Self::ReversedRetrievalOrder => {
                f.write_str("current capture predates the baseline retrieval")
            }
            Self::EvaluationBeforeRetrieval => {
                f.write_str("evaluation predates the current retrieval")
            }
            Self::InvalidExplanationLimits(field) => {
                write!(f, "invalid explanation limit for {field}")
            }
            Self::LineLimitExceeded {
                actual_lines,
                max_lines,
            } => write!(
                f,
                "capture has {actual_lines} lines; explanation limit is {max_lines}"
            ),
            Self::HunkLimitExceeded { max_hunks } => {
                write!(f, "change explanation exceeds limit of {max_hunks} hunks")
            }
            Self::AllocationFailed => f.write_str("unable to allocate change explanation buffer"),
            Self::Stopped(reason) => write!(f, "snapshot comparison stopped: {reason:?}"),
        }
    }
}

impl std::error::Error for ChangeError {}

fn validate_description(field: &'static str, text: &str) -> Result<(), ChangeError> {
    if text.len() > MAX_DESCRIPTION_BYTES {
        return Err(ChangeError::DescriptionTooLong(field));
    }
    if text.trim().is_empty() {
        return Err(ChangeError::EmptyDescription(field));
    }
    Ok(())
}

impl<'text> Snapshot<'text> {
    pub fn new(input: SnapshotInput<'text>) -> Result<Self, ChangeError> {
        validate_description("attribution", input.attribution)?;
        let text = match input.capture {
            Capture::Complete(text) => Some(text),
            Capture::Partial { text, reason } => {
                validate_description("partial capture reason", reason)?;
                Some(text)
            }
            Capture::Unavailable(_) => None,
        };
        if let Some(text) = text
            && text.len() > MAX_TEXT_BYTES
        {
            return Err(ChangeError::TextTooLarge {
                actual_bytes: text.len(),
            });
        }
        Ok(Self(input))
    }

    pub fn input(&self) -> &SnapshotInput<'text> {
        &self.0
    }
}

impl ChangeRule {
    pub fn new(
        scope: SnapshotScope,
        version: VersionId,
        max_retrieval_age: Duration,
    ) -> Result<Self, ChangeError> {
        if max_retrieval_age.is_zero() {
            return Err(ChangeError::ZeroMaxAge);
        }
        Ok(Self {
            scope,
            version,
            max_retrieval_age,
        })
    }

    pub fn scope(&self) -> &SnapshotScope {
        &self.scope
    }

    pub fn version(&self) -> &VersionId {
        &self.version
    }

    pub fn max_retrieval_age(&self) -> Duration {
        self.max_retrieval_age
    }
}

/// A single replacement span after removing the common character prefix/suffix.
/// This can include unchanged text between edits; it is not a minimal edit script.
/// Ranges refer to the original UTF-8 bytes, including whitespace and newlines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextComparison {
    Changed {
        before: Range<usize>,
        after: Range<usize>,
    },
    Unchanged,
    Inconclusive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetrievalFreshness {
    WithinRuleAge,
    Stale,
}

/// An analytical rule decision, never a delivered notification or safety verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleDecision {
    ChangeDetected,
    NoTextChange,
    IncompleteCaptures,
    StaleCurrentCapture,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeEvaluation<'a> {
    rule: &'a ChangeRule,
    before: &'a Snapshot<'a>,
    after: &'a Snapshot<'a>,
    evaluated_at: SystemTime,
    baseline_retrieval_age: Duration,
    current_retrieval_age: Duration,
    comparison: TextComparison,
}

impl<'a> ChangeEvaluation<'a> {
    pub fn rule(&self) -> &ChangeRule {
        self.rule
    }
    pub fn before(&self) -> &Snapshot<'a> {
        self.before
    }
    pub fn after(&self) -> &Snapshot<'a> {
        self.after
    }
    pub fn evaluated_at(&self) -> SystemTime {
        self.evaluated_at
    }
    pub fn current_retrieval_age(&self) -> Duration {
        self.current_retrieval_age
    }
    pub fn baseline_retrieval_age(&self) -> Duration {
        self.baseline_retrieval_age
    }
    pub fn comparison(&self) -> &TextComparison {
        &self.comparison
    }

    /// Inclusive age threshold; recency of retrieval does not establish recency
    /// of publication or completeness of the real-world source.
    pub fn retrieval_freshness(&self) -> RetrievalFreshness {
        if self.current_retrieval_age <= self.rule.max_retrieval_age {
            RetrievalFreshness::WithinRuleAge
        } else {
            RetrievalFreshness::Stale
        }
    }

    /// Apply the same age threshold to the historical baseline for inspection.
    /// An old baseline is intentional in change tracking and does not suppress a
    /// rule match against a fresh current capture.
    pub fn baseline_retrieval_freshness(&self) -> RetrievalFreshness {
        if self.baseline_retrieval_age <= self.rule.max_retrieval_age {
            RetrievalFreshness::WithinRuleAge
        } else {
            RetrievalFreshness::Stale
        }
    }

    pub fn decision(&self) -> RuleDecision {
        if self.comparison == TextComparison::Inconclusive {
            RuleDecision::IncompleteCaptures
        } else if self.retrieval_freshness() == RetrievalFreshness::Stale {
            RuleDecision::StaleCurrentCapture
        } else if self.comparison == TextComparison::Unchanged {
            RuleDecision::NoTextChange
        } else {
            RuleDecision::ChangeDetected
        }
    }
}

/// Evaluate without allocation, I/O or mutation. Work is linear in bounded text
/// size; control is checked before/after work and every 1,024 characters scanned.
/// A stop returns no result. The caller must also fence stale job completions
/// before persisting/publishing this result through the application service.
pub fn evaluate<'a>(
    rule: &'a ChangeRule,
    before: &'a Snapshot<'a>,
    after: &'a Snapshot<'a>,
    evaluated_at: SystemTime,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<ChangeEvaluation<'a>, ChangeError> {
    check_control().map_err(ChangeError::Stopped)?;
    if before.0.scope != rule.scope || after.0.scope != rule.scope {
        return Err(ChangeError::ScopeMismatch);
    }
    if before.0.version == after.0.version {
        return Err(ChangeError::ReusedSnapshotVersion);
    }
    if before.0.retrieved_at > after.0.retrieved_at {
        return Err(ChangeError::ReversedRetrievalOrder);
    }
    let current_retrieval_age = evaluated_at
        .duration_since(after.0.retrieved_at)
        .map_err(|_| ChangeError::EvaluationBeforeRetrieval)?;
    let baseline_retrieval_age = evaluated_at
        .duration_since(before.0.retrieved_at)
        .map_err(|_| ChangeError::EvaluationBeforeRetrieval)?;
    let comparison = match (before.0.capture, after.0.capture) {
        (Capture::Complete(left), Capture::Complete(right)) => {
            compare_text(left, right, &mut check_control)?
        }
        _ => TextComparison::Inconclusive,
    };
    check_control().map_err(ChangeError::Stopped)?;
    Ok(ChangeEvaluation {
        rule,
        before,
        after,
        evaluated_at,
        baseline_retrieval_age,
        current_retrieval_age,
        comparison,
    })
}

fn compare_text(
    left: &str,
    right: &str,
    check_control: &mut impl FnMut() -> Result<(), StopReason>,
) -> Result<TextComparison, ChangeError> {
    let mut prefix = 0;
    for (index, (a, b)) in left.chars().zip(right.chars()).enumerate() {
        if index % CONTROL_INTERVAL == 0 {
            check_control().map_err(ChangeError::Stopped)?;
        }
        if a != b {
            break;
        }
        prefix += a.len_utf8();
    }
    if prefix == left.len() && prefix == right.len() {
        return Ok(TextComparison::Unchanged);
    }
    let mut suffix = 0;
    for (index, (a, b)) in left.chars().rev().zip(right.chars().rev()).enumerate() {
        if index % CONTROL_INTERVAL == 0 {
            check_control().map_err(ChangeError::Stopped)?;
        }
        // Stop at the prefix boundary so insertion/deletion spans cannot overlap.
        if left.len() - suffix <= prefix || right.len() - suffix <= prefix || a != b {
            break;
        }
        suffix += a.len_utf8();
    }
    Ok(TextComparison::Changed {
        before: prefix..left.len() - suffix,
        after: prefix..right.len() - suffix,
    })
}

/// Hard ceiling on lines per capture for line-level change explanation.
pub const MAX_EXPLANATION_LINES: usize = 16_384;
/// Hard ceiling on emitted line/segment change hunks.
pub const MAX_EXPLANATION_HUNKS: usize = 1_024;
/// Hard ceiling on exact quadratic LCS cells (`changed_before_lines * changed_after_lines`)
/// before falling back to a single bounded replacement hunk for the middle window.
pub const MAX_LCS_CELLS: usize = 262_144;
/// Pin this method alongside rule and capture versions when persisting line-level explanations.
pub const EXPLANATION_METHOD_ID: &str = "exact-utf8-line-segments-v1";
/// Criterion identifier for `ChangeRule`.
pub const RULE_CRITERION_ID: &str = "any-exact-utf8-text-change-v1";

/// Explicit caller budgets for line-level change explanation (`PRIVACY-03`, `#48`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChangeExplanationLimits {
    max_lines_per_capture: usize,
    max_hunks: usize,
}

impl ChangeExplanationLimits {
    pub fn new(max_lines_per_capture: usize, max_hunks: usize) -> Result<Self, ChangeError> {
        if max_lines_per_capture == 0 || max_lines_per_capture > MAX_EXPLANATION_LINES {
            return Err(ChangeError::InvalidExplanationLimits(
                "max_lines_per_capture",
            ));
        }
        if max_hunks == 0 || max_hunks > MAX_EXPLANATION_HUNKS {
            return Err(ChangeError::InvalidExplanationLimits("max_hunks"));
        }
        Ok(Self {
            max_lines_per_capture,
            max_hunks,
        })
    }

    pub fn default_limits() -> Self {
        Self {
            max_lines_per_capture: 4_096,
            max_hunks: 256,
        }
    }

    pub fn max_lines_per_capture(self) -> usize {
        self.max_lines_per_capture
    }

    pub fn max_hunks(self) -> usize {
        self.max_hunks
    }
}

/// Classification of an individual line/segment change hunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeHunkKind {
    /// New lines added in the current capture (`before_bytes` is empty at the insertion point).
    Inserted,
    /// Existing lines removed from the baseline capture (`after_bytes` is empty at the deletion point).
    Removed,
    /// Existing lines replaced or modified between the baseline and current captures.
    Modified,
}

/// A contiguous, non-overlapping line-level change hunk anchored to exact UTF-8 byte ranges
/// and 1-indexed line numbers in both captures.
///
/// Exact line endings (`\r\n` vs `\n`) are included in the byte ranges so changes in
/// line-ending conventions or trailing newlines remain inspectable and reconstructable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeHunk {
    kind: ChangeHunkKind,
    before_bytes: Range<usize>,
    after_bytes: Range<usize>,
    before_lines: Option<std::ops::RangeInclusive<usize>>,
    after_lines: Option<std::ops::RangeInclusive<usize>>,
}

impl ChangeHunk {
    pub fn kind(&self) -> ChangeHunkKind {
        self.kind
    }

    /// Half-open UTF-8 byte range in the baseline capture. Empty (`p..p`) for `Inserted`.
    pub fn before_bytes(&self) -> Range<usize> {
        self.before_bytes.clone()
    }

    /// Half-open UTF-8 byte range in the current capture. Empty (`q..q`) for `Removed`.
    pub fn after_bytes(&self) -> Range<usize> {
        self.after_bytes.clone()
    }

    /// 1-indexed inclusive line range in the baseline capture (`None` for pure insertions).
    pub fn before_lines(&self) -> Option<std::ops::RangeInclusive<usize>> {
        self.before_lines.clone()
    }

    /// 1-indexed inclusive line range in the current capture (`None` for pure removals).
    pub fn after_lines(&self) -> Option<std::ops::RangeInclusive<usize>> {
        self.after_lines.clone()
    }
}

/// Structured line/segment-level explanation of snapshot differences (`PRIVACY-03`, `A18`).
///
/// Keeps `Changed`, `Unchanged`, and `Inconclusive` strictly distinct so partial or
/// unavailable captures never masquerade as deletions or unchanged resources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeExplanation {
    Changed {
        method_id: &'static str,
        before_line_count: usize,
        after_line_count: usize,
        unchanged_line_count: usize,
        inserted_line_count: usize,
        removed_line_count: usize,
        /// `true` when `changed_before_lines * changed_after_lines > MAX_LCS_CELLS`
        /// caused the interior line window to coalesce into one bounded `Modified` hunk
        /// rather than allocating a quadratic alignment matrix.
        coalesced_window_exceeded_lcs_budget: bool,
        hunks: Vec<ChangeHunk>,
    },
    Unchanged {
        method_id: &'static str,
        line_count: usize,
    },
    Inconclusive {
        baseline_coverage: CaptureCoverageSummary,
        current_coverage: CaptureCoverageSummary,
    },
}

/// Explicit capture coverage summary without copying capture text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureCoverageSummary {
    Complete {
        byte_len: usize,
        line_count: usize,
    },
    Partial {
        captured_byte_len: usize,
        reason: String,
    },
    Unavailable(UnavailableReason),
}

impl CaptureCoverageSummary {
    fn from_capture(capture: Capture<'_>) -> Result<Self, ChangeError> {
        match capture {
            Capture::Complete(text) => Ok(Self::Complete {
                byte_len: text.len(),
                line_count: text.split_inclusive('\n').count(),
            }),
            Capture::Partial { text, reason } => {
                let mut owned_reason = String::new();
                owned_reason
                    .try_reserve_exact(reason.len())
                    .map_err(|_| ChangeError::AllocationFailed)?;
                owned_reason.push_str(reason);
                Ok(Self::Partial {
                    captured_byte_len: text.len(),
                    reason: owned_reason,
                })
            }
            Capture::Unavailable(reason) => Ok(Self::Unavailable(reason)),
        }
    }
}

/// Inspectable notification/summary envelope binding the saved rule, source attribution,
/// baseline/current capture coverage, retrieval freshness, and line-level change explanation
/// (`PRIVACY-03`, `A18`).
///
/// This is an analytical projection, not a delivered alert or safety conclusion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplainedNotification<'a> {
    rule_version: &'a VersionId,
    rule_criterion: &'static str,
    scope: &'a SnapshotScope,
    max_retrieval_age: Duration,
    before_version: &'a VersionId,
    before_attribution: &'a str,
    before_retrieved_at: SystemTime,
    baseline_retrieval_age: Duration,
    baseline_retrieval_freshness: RetrievalFreshness,
    baseline_coverage: CaptureCoverageSummary,
    after_version: &'a VersionId,
    after_attribution: &'a str,
    after_retrieved_at: SystemTime,
    current_retrieval_age: Duration,
    current_retrieval_freshness: RetrievalFreshness,
    current_coverage: CaptureCoverageSummary,
    evaluated_at: SystemTime,
    decision: RuleDecision,
    replacement_span: &'a TextComparison,
    explanation: ChangeExplanation,
}

impl<'a> ExplainedNotification<'a> {
    pub fn rule_version(&self) -> &'a VersionId {
        self.rule_version
    }
    pub fn rule_criterion(&self) -> &'static str {
        self.rule_criterion
    }
    pub fn scope(&self) -> &'a SnapshotScope {
        self.scope
    }
    pub fn max_retrieval_age(&self) -> Duration {
        self.max_retrieval_age
    }
    pub fn before_version(&self) -> &'a VersionId {
        self.before_version
    }
    pub fn before_attribution(&self) -> &'a str {
        self.before_attribution
    }
    pub fn before_retrieved_at(&self) -> SystemTime {
        self.before_retrieved_at
    }
    pub fn baseline_retrieval_age(&self) -> Duration {
        self.baseline_retrieval_age
    }
    pub fn baseline_retrieval_freshness(&self) -> RetrievalFreshness {
        self.baseline_retrieval_freshness
    }
    pub fn baseline_coverage(&self) -> &CaptureCoverageSummary {
        &self.baseline_coverage
    }
    pub fn after_version(&self) -> &'a VersionId {
        self.after_version
    }
    pub fn after_attribution(&self) -> &'a str {
        self.after_attribution
    }
    pub fn after_retrieved_at(&self) -> SystemTime {
        self.after_retrieved_at
    }
    pub fn current_retrieval_age(&self) -> Duration {
        self.current_retrieval_age
    }
    pub fn current_retrieval_freshness(&self) -> RetrievalFreshness {
        self.current_retrieval_freshness
    }
    pub fn current_coverage(&self) -> &CaptureCoverageSummary {
        &self.current_coverage
    }
    pub fn evaluated_at(&self) -> SystemTime {
        self.evaluated_at
    }
    pub fn decision(&self) -> RuleDecision {
        self.decision
    }
    pub fn replacement_span(&self) -> &'a TextComparison {
        self.replacement_span
    }
    pub fn explanation(&self) -> &ChangeExplanation {
        &self.explanation
    }
}

#[derive(Clone)]
struct LineSlice<'a> {
    text: &'a str,
    byte_range: Range<usize>,
    line_number: usize,
}

fn split_lines<'a>(
    text: &'a str,
    max_lines: usize,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<Vec<LineSlice<'a>>, ChangeError> {
    check_control().map_err(ChangeError::Stopped)?;
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let count = text.split_inclusive('\n').count();
    if count > max_lines {
        return Err(ChangeError::LineLimitExceeded {
            actual_lines: count,
            max_lines,
        });
    }
    let mut lines = Vec::new();
    lines
        .try_reserve_exact(count)
        .map_err(|_| ChangeError::AllocationFailed)?;
    let mut offset = 0usize;
    for (idx, line) in text.split_inclusive('\n').enumerate() {
        if idx % CONTROL_INTERVAL == 0 {
            check_control().map_err(ChangeError::Stopped)?;
        }
        let end = offset + line.len();
        lines.push(LineSlice {
            text: line,
            byte_range: offset..end,
            line_number: idx + 1,
        });
        offset = end;
    }
    Ok(lines)
}

fn build_hunk(
    before_window: &[LineSlice<'_>],
    before_insertion_byte: usize,
    after_window: &[LineSlice<'_>],
    after_insertion_byte: usize,
) -> Option<ChangeHunk> {
    match (
        before_window.first(),
        before_window.last(),
        after_window.first(),
        after_window.last(),
    ) {
        (None, None, None, None) => None,
        (None, None, Some(af), Some(al)) => Some(ChangeHunk {
            kind: ChangeHunkKind::Inserted,
            before_bytes: before_insertion_byte..before_insertion_byte,
            after_bytes: af.byte_range.start..al.byte_range.end,
            before_lines: None,
            after_lines: Some(af.line_number..=al.line_number),
        }),
        (Some(bf), Some(bl), None, None) => Some(ChangeHunk {
            kind: ChangeHunkKind::Removed,
            before_bytes: bf.byte_range.start..bl.byte_range.end,
            after_bytes: after_insertion_byte..after_insertion_byte,
            before_lines: Some(bf.line_number..=bl.line_number),
            after_lines: None,
        }),
        (Some(bf), Some(bl), Some(af), Some(al)) => Some(ChangeHunk {
            kind: ChangeHunkKind::Modified,
            before_bytes: bf.byte_range.start..bl.byte_range.end,
            after_bytes: af.byte_range.start..al.byte_range.end,
            before_lines: Some(bf.line_number..=bl.line_number),
            after_lines: Some(af.line_number..=al.line_number),
        }),
        _ => None,
    }
}

impl<'a> ChangeEvaluation<'a> {
    /// Compute a bounded line/segment-level explanation of the evaluated snapshot pair.
    ///
    /// - Preserves exact UTF-8 byte offsets and line endings (`\r\n` vs `\n`).
    /// - Keeps `Changed`, `Unchanged`, and `Inconclusive` distinct.
    /// - Enforces `limits.max_lines_per_capture()` and `limits.max_hunks()`, returning a
    ///   typed `ChangeError` rather than silently truncating hunks.
    pub fn explain_changes(
        &self,
        limits: ChangeExplanationLimits,
        mut check_control: impl FnMut() -> Result<(), StopReason>,
    ) -> Result<ChangeExplanation, ChangeError> {
        check_control().map_err(ChangeError::Stopped)?;
        let (left_text, right_text) = match (self.before.0.capture, self.after.0.capture) {
            (Capture::Complete(left), Capture::Complete(right)) => (left, right),
            _ => {
                let baseline_coverage =
                    CaptureCoverageSummary::from_capture(self.before.0.capture)?;
                let current_coverage = CaptureCoverageSummary::from_capture(self.after.0.capture)?;
                check_control().map_err(ChangeError::Stopped)?;
                return Ok(ChangeExplanation::Inconclusive {
                    baseline_coverage,
                    current_coverage,
                });
            }
        };

        let left_lines = split_lines(left_text, limits.max_lines_per_capture, &mut check_control)?;
        let right_lines =
            split_lines(right_text, limits.max_lines_per_capture, &mut check_control)?;

        if self.comparison == TextComparison::Unchanged {
            check_control().map_err(ChangeError::Stopped)?;
            return Ok(ChangeExplanation::Unchanged {
                method_id: EXPLANATION_METHOD_ID,
                line_count: left_lines.len(),
            });
        }

        // Trim identical leading lines.
        let mut prefix_lines = 0usize;
        for (idx, (l, r)) in left_lines.iter().zip(right_lines.iter()).enumerate() {
            if idx % CONTROL_INTERVAL == 0 {
                check_control().map_err(ChangeError::Stopped)?;
            }
            if l.text != r.text {
                break;
            }
            prefix_lines += 1;
        }

        // Trim identical trailing lines without overlapping the prefix.
        let mut suffix_lines = 0usize;
        for (idx, (l, r)) in left_lines
            .iter()
            .rev()
            .zip(right_lines.iter().rev())
            .enumerate()
        {
            if idx % CONTROL_INTERVAL == 0 {
                check_control().map_err(ChangeError::Stopped)?;
            }
            if left_lines.len() - suffix_lines <= prefix_lines
                || right_lines.len() - suffix_lines <= prefix_lines
                || l.text != r.text
            {
                break;
            }
            suffix_lines += 1;
        }

        let left_mid = left_lines
            .get(prefix_lines..left_lines.len() - suffix_lines)
            .unwrap_or(&[]);
        let right_mid = right_lines
            .get(prefix_lines..right_lines.len() - suffix_lines)
            .unwrap_or(&[]);

        let before_anchor_start = left_mid
            .first()
            .map(|l| l.byte_range.start)
            .or_else(|| {
                left_lines
                    .get(prefix_lines.wrapping_sub(1))
                    .map(|l| l.byte_range.end)
            })
            .unwrap_or(0);
        let after_anchor_start = right_mid
            .first()
            .map(|l| l.byte_range.start)
            .or_else(|| {
                right_lines
                    .get(prefix_lines.wrapping_sub(1))
                    .map(|l| l.byte_range.end)
            })
            .unwrap_or(0);

        let n = left_mid.len();
        let m = right_mid.len();
        let cells = n.saturating_mul(m);

        let mut hunks = Vec::new();
        let mut unchanged_mid_lines = 0usize;
        let mut inserted_line_count = 0usize;
        let mut removed_line_count = 0usize;
        let mut coalesced = false;

        if n == 0 || m == 0 || cells > MAX_LCS_CELLS {
            if cells > MAX_LCS_CELLS {
                coalesced = true;
            }
            if let Some(hunk) =
                build_hunk(left_mid, before_anchor_start, right_mid, after_anchor_start)
            {
                hunks
                    .try_reserve_exact(1)
                    .map_err(|_| ChangeError::AllocationFailed)?;
                hunks.push(hunk);
                removed_line_count = n;
                inserted_line_count = m;
            }
        } else {
            let cols = m + 1;
            let total_dp = (n + 1)
                .checked_mul(cols)
                .ok_or(ChangeError::AllocationFailed)?;
            let mut dp = Vec::new();
            dp.try_reserve_exact(total_dp)
                .map_err(|_| ChangeError::AllocationFailed)?;
            dp.resize(total_dp, 0u32);

            let mut step = 0usize;
            for i in (0..n).rev() {
                for j in (0..m).rev() {
                    if step.is_multiple_of(CONTROL_INTERVAL) {
                        check_control().map_err(ChangeError::Stopped)?;
                    }
                    step += 1;
                    let left_line = left_mid.get(i).map(|l| l.text).unwrap_or("");
                    let right_line = right_mid.get(j).map(|l| l.text).unwrap_or("");
                    let val = if left_line == right_line {
                        let diag = dp.get((i + 1) * cols + (j + 1)).copied().unwrap_or(0);
                        diag.saturating_add(1)
                    } else {
                        let down = dp.get((i + 1) * cols + j).copied().unwrap_or(0);
                        let right = dp.get(i * cols + (j + 1)).copied().unwrap_or(0);
                        down.max(right)
                    };
                    if let Some(slot) = dp.get_mut(i * cols + j) {
                        *slot = val;
                    }
                }
            }

            let mut i = 0usize;
            let mut j = 0usize;
            let mut run_start_i = 0usize;
            let mut run_start_j = 0usize;

            while i < n || j < m {
                if (i + j).is_multiple_of(CONTROL_INTERVAL) {
                    check_control().map_err(ChangeError::Stopped)?;
                }
                let matched = if i < n && j < m {
                    left_mid.get(i).map(|l| l.text) == right_mid.get(j).map(|r| r.text)
                } else {
                    false
                };

                if matched {
                    if run_start_i < i || run_start_j < j {
                        let b_slice = left_mid.get(run_start_i..i).unwrap_or(&[]);
                        let a_slice = right_mid.get(run_start_j..j).unwrap_or(&[]);
                        let b_ins = left_mid
                            .get(run_start_i)
                            .map(|l| l.byte_range.start)
                            .unwrap_or(before_anchor_start);
                        let a_ins = right_mid
                            .get(run_start_j)
                            .map(|l| l.byte_range.start)
                            .unwrap_or(after_anchor_start);
                        if let Some(hunk) = build_hunk(b_slice, b_ins, a_slice, a_ins) {
                            if hunks.len() == limits.max_hunks {
                                return Err(ChangeError::HunkLimitExceeded {
                                    max_hunks: limits.max_hunks,
                                });
                            }
                            hunks
                                .try_reserve(1)
                                .map_err(|_| ChangeError::AllocationFailed)?;
                            hunks.push(hunk);
                            removed_line_count += b_slice.len();
                            inserted_line_count += a_slice.len();
                        }
                    }
                    unchanged_mid_lines += 1;
                    i += 1;
                    j += 1;
                    run_start_i = i;
                    run_start_j = j;
                } else if i < n
                    && (j == m
                        || dp.get((i + 1) * cols + j).copied().unwrap_or(0)
                            >= dp.get(i * cols + (j + 1)).copied().unwrap_or(0))
                {
                    i += 1;
                } else {
                    j += 1;
                }
            }

            if run_start_i < n || run_start_j < m {
                let b_slice = left_mid.get(run_start_i..n).unwrap_or(&[]);
                let a_slice = right_mid.get(run_start_j..m).unwrap_or(&[]);
                let b_ins = left_mid
                    .get(run_start_i)
                    .map(|l| l.byte_range.start)
                    .or_else(|| {
                        left_mid
                            .get(run_start_i.wrapping_sub(1))
                            .map(|l| l.byte_range.end)
                    })
                    .unwrap_or(before_anchor_start);
                let a_ins = right_mid
                    .get(run_start_j)
                    .map(|l| l.byte_range.start)
                    .or_else(|| {
                        right_mid
                            .get(run_start_j.wrapping_sub(1))
                            .map(|l| l.byte_range.end)
                    })
                    .unwrap_or(after_anchor_start);
                if let Some(hunk) = build_hunk(b_slice, b_ins, a_slice, a_ins) {
                    if hunks.len() == limits.max_hunks {
                        return Err(ChangeError::HunkLimitExceeded {
                            max_hunks: limits.max_hunks,
                        });
                    }
                    hunks
                        .try_reserve(1)
                        .map_err(|_| ChangeError::AllocationFailed)?;
                    hunks.push(hunk);
                    removed_line_count += b_slice.len();
                    inserted_line_count += a_slice.len();
                }
            }
        }

        check_control().map_err(ChangeError::Stopped)?;
        Ok(ChangeExplanation::Changed {
            method_id: EXPLANATION_METHOD_ID,
            before_line_count: left_lines.len(),
            after_line_count: right_lines.len(),
            unchanged_line_count: prefix_lines + suffix_lines + unchanged_mid_lines,
            inserted_line_count,
            removed_line_count,
            coalesced_window_exceeded_lcs_budget: coalesced,
            hunks,
        })
    }

    /// Project an explained notification record that exposes source coverage, freshness,
    /// attribution, the saved rule that triggered it, and the line-level change explanation
    /// (`PRIVACY-03`, `A18`).
    pub fn explain_notification(
        &'a self,
        limits: ChangeExplanationLimits,
        mut check_control: impl FnMut() -> Result<(), StopReason>,
    ) -> Result<ExplainedNotification<'a>, ChangeError> {
        let baseline_coverage = CaptureCoverageSummary::from_capture(self.before.0.capture)?;
        let current_coverage = CaptureCoverageSummary::from_capture(self.after.0.capture)?;
        let explanation = self.explain_changes(limits, &mut check_control)?;
        Ok(ExplainedNotification {
            rule_version: &self.rule.version,
            rule_criterion: RULE_CRITERION_ID,
            scope: &self.rule.scope,
            max_retrieval_age: self.rule.max_retrieval_age,
            before_version: &self.before.0.version,
            before_attribution: self.before.0.attribution,
            before_retrieved_at: self.before.0.retrieved_at,
            baseline_retrieval_age: self.baseline_retrieval_age,
            baseline_retrieval_freshness: self.baseline_retrieval_freshness(),
            baseline_coverage,
            after_version: &self.after.0.version,
            after_attribution: self.after.0.attribution,
            after_retrieved_at: self.after.0.retrieved_at,
            current_retrieval_age: self.current_retrieval_age,
            current_retrieval_freshness: self.retrieval_freshness(),
            current_coverage,
            evaluated_at: self.evaluated_at,
            decision: self.decision(),
            replacement_span: &self.comparison,
            explanation,
        })
    }
}
