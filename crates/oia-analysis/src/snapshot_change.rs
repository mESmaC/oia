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

pub use crate::multilingual::StopReason;

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
/// Hard ceiling on exact quadratic LCS cells (`sub_before_lines * sub_after_lines`)
/// for an individual residual sub-window.
///
/// Sub-windows exceeding this cell budget first partition around unique common line
/// anchors (Patience LIS in `O((n + m) log u)` time) and, if no unique common lines
/// exist, attempt bounded Myers alignment up to [`MAX_MYERS_EDIT_DISTANCE`] before
/// coalescing only that unanchored residual sub-window.
pub const MAX_LCS_CELLS: usize = 262_144;
/// Maximum edit distance (`removed_lines + inserted_lines`) explored by bounded Myers
/// alignment on an unanchored sub-window whose cell product exceeds [`MAX_LCS_CELLS`].
pub const MAX_MYERS_EDIT_DISTANCE: usize = 256;
/// Strict ceiling on cumulative sub-quadratic partitioning comparisons (merge sort,
/// unique-line scan, merge, and LIS binary search probes) and Myers diagonal/snake steps
/// across a single `explain_changes` invocation before remaining oversized sub-windows
/// coalesce directly with [`CoalescedReason::AlignmentStepBudgetExceeded`].
pub const MAX_ALIGNMENT_STEPS: usize = 1_048_576;
/// Pin this method alongside rule and capture versions when persisting line-level explanations.
pub const EXPLANATION_METHOD_ID: &str = "exact-utf8-line-segments-v2";
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

/// Reason an unanchored sub-window exceeding [`MAX_LCS_CELLS`] was coalesced into a
/// single replacement hunk (`AGENTS.md` §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoalescedReason {
    /// The sub-window had no unique common lines and its edit distance exceeded
    /// [`MAX_MYERS_EDIT_DISTANCE`].
    NoUniqueAnchorsOverEditBudget,
    /// Cumulative sub-quadratic partitioning and Myers steps reached the strict
    /// [`MAX_ALIGNMENT_STEPS`] ceiling before this sub-window could be aligned.
    AlignmentStepBudgetExceeded,
}

/// Alignment provenance for an individual [`ChangeHunk`] (`AGENTS.md` §2, §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HunkAlignment {
    /// Globally minimal edit-script alignment on the full non-identical window (via
    /// prefix/suffix trimming, direct [`MAX_LCS_CELLS`] DP, or direct
    /// [`MAX_MYERS_EDIT_DISTANCE`] Myers without unique-anchor partitioning).
    ExactMinimal,
    /// Exact alignment within a sub-window produced by Patience unique-line anchor
    /// partitioning. Unique anchors preserve human-meaningful landmarks across large
    /// files, so duplicate lines moved across a pinned unique anchor are reported as
    /// removed and re-inserted rather than globally minimal.
    UniqueAnchorPartitioned,
    /// Quality-reducing coalesced replacement hunk for an unanchored sub-window that
    /// exceeded [`MAX_LCS_CELLS`] and could not be aligned within explicit budgets.
    /// May contain unaligned unchanged lines inside its byte/line range.
    Coalesced(CoalescedReason),
}

/// A contiguous, non-overlapping line-level change hunk anchored to exact UTF-8 byte ranges
/// and 1-indexed line numbers in both captures.
///
/// Exact line endings (`\r\n` vs `\n`) are included in the byte ranges so changes in
/// line-ending conventions or trailing newlines remain inspectable and reconstructable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeHunk {
    kind: ChangeHunkKind,
    alignment: HunkAlignment,
    before_bytes: Range<usize>,
    after_bytes: Range<usize>,
    before_lines: Option<std::ops::RangeInclusive<usize>>,
    after_lines: Option<std::ops::RangeInclusive<usize>>,
}

impl ChangeHunk {
    pub fn kind(&self) -> ChangeHunkKind {
        self.kind
    }

    /// Alignment provenance indicating whether this hunk came from a globally minimal
    /// alignment, a unique-anchor partitioned sub-window, or a coalesced fallback window.
    pub fn alignment(&self) -> HunkAlignment {
        self.alignment
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

/// Line-level change counts for `ChangeExplanation::Changed`.
///
/// Separates globally minimal line counts, unique-anchor partitioned line counts, and the
/// quality-reducing coalesced sub-window fallback so neither anchor-partitioned duplicate
/// moves nor unaligned high-churn sub-windows masquerade as globally minimal edit counts
/// (`AGENTS.md` §2, §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineChangeCounts {
    /// Globally minimal line counts measured via leading/trailing line trimming,
    /// direct [`MAX_LCS_CELLS`] DP, or direct [`MAX_MYERS_EDIT_DISTANCE`] Myers
    /// without unique-anchor partitioning. Every hunk has
    /// [`HunkAlignment::ExactMinimal`].
    Exact {
        unchanged_lines: usize,
        inserted_lines: usize,
        removed_lines: usize,
    },
    /// Line counts measured after Patience unique-line anchor partitioning, where
    /// every residual sub-window was aligned without coalescing (via trimming,
    /// [`MAX_LCS_CELLS`] DP, or [`MAX_MYERS_EDIT_DISTANCE`] Myers).
    ///
    /// Every hunk contains only inserted, removed, or modified lines relative to the
    /// pinned unique-anchor partition ([`HunkAlignment::UniqueAnchorPartitioned`]),
    /// while duplicate lines moved across a pinned unique anchor are counted as
    /// removed and re-inserted rather than globally minimal (`AGENTS.md` §3).
    AnchorPartitioned {
        matched_unchanged_lines: usize,
        inserted_lines: usize,
        removed_lines: usize,
    },
    /// Quality-reducing fallback used when at least one unanchored sub-window exceeds
    /// [`MAX_LCS_CELLS`] and is coalesced with [`HunkAlignment::Coalesced`].
    ///
    /// `trimmed_unchanged_lines` records all verified unchanged lines (from prefix/suffix
    /// trimming, unique anchors, and aligned sub-windows). `coalesced_before_window_lines`
    /// and `coalesced_after_window_lines` bound total non-matched baseline and current
    /// lines across emitted hunks, which may include unaligned unchanged lines inside
    /// any coalesced sub-window. These window sizes are upper bounds on removed and
    /// inserted lines, not measured change counts.
    CoalescedWindowUpperBound {
        trimmed_unchanged_lines: usize,
        coalesced_before_window_lines: usize,
        coalesced_after_window_lines: usize,
    },
}

/// Reason a line/segment-level change explanation could not be completed within
/// `ChangeExplanationLimits` during notification projection (`PRIVACY-03`, `A18`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplanationUnavailableReason {
    /// At least one complete capture exceeded `limits.max_lines_per_capture()`.
    LineLimitExceeded {
        actual_lines: usize,
        max_lines: usize,
    },
    /// The number of distinct line/segment change hunks exceeded `limits.max_hunks()`.
    HunkLimitExceeded { max_hunks: usize },
}

/// Structured line/segment-level explanation of snapshot differences (`PRIVACY-03`, `A18`).
///
/// Keeps `Changed`, `Unchanged`, `Inconclusive`, and `Unavailable` strictly distinct so
/// partial, unavailable, or budget-exceeded captures never masquerade as deletions or
/// unchanged resources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeExplanation {
    Changed {
        method_id: &'static str,
        before_line_count: usize,
        after_line_count: usize,
        /// Globally minimal line counts (`LineChangeCounts::Exact`), unique-anchor
        /// partitioned counts (`LineChangeCounts::AnchorPartitioned`), or explicit
        /// window upper bounds (`LineChangeCounts::CoalescedWindowUpperBound`)
        /// when an unanchored high-churn sub-window is coalesced.
        line_counts: LineChangeCounts,
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
    /// Line/segment decomposition exceeded `ChangeExplanationLimits` during notification
    /// projection. The notification's `RuleDecision`, capture coverage, freshness, and
    /// exact top-level `TextComparison` byte replacement span remain intact.
    Unavailable {
        before_line_count: usize,
        after_line_count: usize,
        reason: ExplanationUnavailableReason,
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
    fn from_capture(
        capture: Capture<'_>,
        check_control: &mut impl FnMut() -> Result<(), StopReason>,
    ) -> Result<Self, ChangeError> {
        check_control().map_err(ChangeError::Stopped)?;
        match capture {
            Capture::Complete(text) => {
                let mut line_count = 0usize;
                for (idx, _) in text.split_inclusive('\n').enumerate() {
                    if idx.is_multiple_of(CONTROL_INTERVAL) {
                        check_control().map_err(ChangeError::Stopped)?;
                    }
                    line_count += 1;
                }
                Ok(Self::Complete {
                    byte_len: text.len(),
                    line_count,
                })
            }
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
    hash: u64,
    byte_range: Range<usize>,
    line_number: usize,
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3_u64);
    }
    hash
}

fn lines_equal(left: &LineSlice<'_>, right: &LineSlice<'_>) -> bool {
    left.hash == right.hash && left.text.len() == right.text.len() && left.text == right.text
}

fn cmp_lines(left: &LineSlice<'_>, right: &LineSlice<'_>) -> std::cmp::Ordering {
    left.hash
        .cmp(&right.hash)
        .then_with(|| left.text.len().cmp(&right.text.len()))
        .then_with(|| left.text.cmp(right.text))
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
    let mut count = 0usize;
    for (idx, _) in text.split_inclusive('\n').enumerate() {
        if idx.is_multiple_of(CONTROL_INTERVAL) {
            check_control().map_err(ChangeError::Stopped)?;
        }
        count += 1;
    }
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
        if idx.is_multiple_of(CONTROL_INTERVAL) {
            check_control().map_err(ChangeError::Stopped)?;
        }
        let end = offset + line.len();
        lines.push(LineSlice {
            text: line,
            hash: fnv1a_64(line.as_bytes()),
            byte_range: offset..end,
            line_number: idx + 1,
        });
        offset = end;
    }
    Ok(lines)
}

fn boundary_byte(lines: &[LineSlice<'_>], index: usize) -> usize {
    if let Some(line) = lines.get(index) {
        line.byte_range.start
    } else if index > 0
        && let Some(prev) = lines.get(index - 1)
    {
        prev.byte_range.end
    } else {
        0
    }
}

fn build_hunk(
    alignment: HunkAlignment,
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
            alignment,
            before_bytes: before_insertion_byte..before_insertion_byte,
            after_bytes: af.byte_range.start..al.byte_range.end,
            before_lines: None,
            after_lines: Some(af.line_number..=al.line_number),
        }),
        (Some(bf), Some(bl), None, None) => Some(ChangeHunk {
            kind: ChangeHunkKind::Removed,
            alignment,
            before_bytes: bf.byte_range.start..bl.byte_range.end,
            after_bytes: after_insertion_byte..after_insertion_byte,
            before_lines: Some(bf.line_number..=bl.line_number),
            after_lines: None,
        }),
        (Some(bf), Some(bl), Some(af), Some(al)) => Some(ChangeHunk {
            kind: ChangeHunkKind::Modified,
            alignment,
            before_bytes: bf.byte_range.start..bl.byte_range.end,
            after_bytes: af.byte_range.start..al.byte_range.end,
            before_lines: Some(bf.line_number..=bl.line_number),
            after_lines: Some(af.line_number..=al.line_number),
        }),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_subwindow_hunk(
    alignment: HunkAlignment,
    left_lines: &[LineSlice<'_>],
    l_start: usize,
    l_end: usize,
    right_lines: &[LineSlice<'_>],
    r_start: usize,
    r_end: usize,
    max_hunks: usize,
    hunks: &mut Vec<ChangeHunk>,
) -> Result<(), ChangeError> {
    let b_slice = left_lines.get(l_start..l_end).unwrap_or(&[]);
    let a_slice = right_lines.get(r_start..r_end).unwrap_or(&[]);
    let b_ins = boundary_byte(left_lines, l_start);
    let a_ins = boundary_byte(right_lines, r_start);
    if let Some(hunk) = build_hunk(alignment, b_slice, b_ins, a_slice, a_ins) {
        if hunks.len() == max_hunks {
            return Err(ChangeError::HunkLimitExceeded { max_hunks });
        }
        hunks
            .try_reserve(1)
            .map_err(|_| ChangeError::AllocationFailed)?;
        hunks.push(hunk);
    }
    Ok(())
}

/// Iterative bottom-up merge sort that charges 1 step per element comparison against
/// `alignment_steps`, checks `check_control` every `CONTROL_INTERVAL` steps, and returns
/// `Ok(false)` immediately if `alignment_steps` reaches [`MAX_ALIGNMENT_STEPS`].
fn merge_sort_by<T: Copy>(
    items: &mut [T],
    alignment_steps: &mut usize,
    check_control: &mut impl FnMut() -> Result<(), StopReason>,
    mut cmp: impl FnMut(T, T) -> std::cmp::Ordering,
) -> Result<bool, ChangeError> {
    let len = items.len();
    let Some(&first) = items.first() else {
        return Ok(true);
    };
    if len <= 1 {
        return Ok(true);
    }
    let mut scratch = Vec::new();
    scratch
        .try_reserve_exact(len)
        .map_err(|_| ChangeError::AllocationFailed)?;
    scratch.resize(len, first);

    let mut width = 1usize;
    while width < len {
        let mut left = 0usize;
        while left < len {
            let mid = left.saturating_add(width).min(len);
            let right = left.saturating_add(width.saturating_mul(2)).min(len);
            let mut i = left;
            let mut j = mid;
            for out in scratch.get_mut(left..right).unwrap_or(&mut []) {
                if i < mid && j < right {
                    if alignment_steps.is_multiple_of(CONTROL_INTERVAL) {
                        check_control().map_err(ChangeError::Stopped)?;
                    }
                    if *alignment_steps >= MAX_ALIGNMENT_STEPS {
                        return Ok(false);
                    }
                    *alignment_steps += 1;
                    let (Some(&a), Some(&b)) = (items.get(i), items.get(j)) else {
                        break;
                    };
                    if cmp(a, b) != std::cmp::Ordering::Greater {
                        *out = a;
                        i += 1;
                    } else {
                        *out = b;
                        j += 1;
                    }
                } else if i < mid {
                    if let Some(&a) = items.get(i) {
                        *out = a;
                    }
                    i += 1;
                } else if let Some(&b) = items.get(j) {
                    *out = b;
                    j += 1;
                }
            }
            left = right;
        }
        items.copy_from_slice(&scratch);
        width = width.saturating_mul(2);
    }
    Ok(true)
}

fn unique_sorted_indices(
    sub: &[LineSlice<'_>],
    alignment_steps: &mut usize,
    check_control: &mut impl FnMut() -> Result<(), StopReason>,
) -> Result<Option<Vec<usize>>, ChangeError> {
    let len = sub.len();
    let mut order = Vec::new();
    order
        .try_reserve_exact(len)
        .map_err(|_| ChangeError::AllocationFailed)?;
    for idx in 0..len {
        if idx.is_multiple_of(CONTROL_INTERVAL) {
            check_control().map_err(ChangeError::Stopped)?;
        }
        order.push(idx);
    }
    if !merge_sort_by(&mut order, alignment_steps, check_control, |a, b| {
        match (sub.get(a), sub.get(b)) {
            (Some(la), Some(lb)) => cmp_lines(la, lb).then_with(|| a.cmp(&b)),
            _ => a.cmp(&b),
        }
    })? {
        return Ok(None);
    }
    let mut unique = Vec::new();
    unique
        .try_reserve_exact(len)
        .map_err(|_| ChangeError::AllocationFailed)?;
    for pos in 0..len {
        if alignment_steps.is_multiple_of(CONTROL_INTERVAL) {
            check_control().map_err(ChangeError::Stopped)?;
        }
        if *alignment_steps >= MAX_ALIGNMENT_STEPS {
            return Ok(None);
        }
        *alignment_steps += 1;
        let Some(&idx) = order.get(pos) else {
            continue;
        };
        let Some(cur) = sub.get(idx) else {
            continue;
        };
        let dup_prev = pos > 0
            && order
                .get(pos - 1)
                .and_then(|&p| sub.get(p))
                .is_some_and(|prev| lines_equal(prev, cur));
        let dup_next = order
            .get(pos + 1)
            .and_then(|&n| sub.get(n))
            .is_some_and(|next| lines_equal(cur, next));
        if !dup_prev && !dup_next {
            unique.push(idx);
        }
    }
    Ok(Some(unique))
}

/// Find the longest increasing subsequence of lines that appear exactly once in `left_sub`
/// and `right_sub` (`O((n + m) log(n + m))` time, deterministic tie-breaking).
/// Returns `Ok(None)` if [`MAX_ALIGNMENT_STEPS`] is reached during sorting, merging, or LIS.
fn find_unique_anchor_lis(
    left_sub: &[LineSlice<'_>],
    right_sub: &[LineSlice<'_>],
    alignment_steps: &mut usize,
    check_control: &mut impl FnMut() -> Result<(), StopReason>,
) -> Result<Option<Vec<(usize, usize)>>, ChangeError> {
    let Some(left_unique) = unique_sorted_indices(left_sub, alignment_steps, check_control)? else {
        return Ok(None);
    };
    let Some(right_unique) = unique_sorted_indices(right_sub, alignment_steps, check_control)?
    else {
        return Ok(None);
    };
    if left_unique.is_empty() || right_unique.is_empty() {
        return Ok(Some(Vec::new()));
    }

    let mut pairs = Vec::new();
    pairs
        .try_reserve_exact(left_unique.len().min(right_unique.len()))
        .map_err(|_| ChangeError::AllocationFailed)?;
    let mut li = 0usize;
    let mut ri = 0usize;
    while li < left_unique.len() && ri < right_unique.len() {
        if alignment_steps.is_multiple_of(CONTROL_INTERVAL) {
            check_control().map_err(ChangeError::Stopped)?;
        }
        if *alignment_steps >= MAX_ALIGNMENT_STEPS {
            return Ok(None);
        }
        *alignment_steps += 1;
        let Some(&l_idx) = left_unique.get(li) else {
            break;
        };
        let Some(&r_idx) = right_unique.get(ri) else {
            break;
        };
        let (Some(l_line), Some(r_line)) = (left_sub.get(l_idx), right_sub.get(r_idx)) else {
            break;
        };
        match cmp_lines(l_line, r_line) {
            std::cmp::Ordering::Less => li += 1,
            std::cmp::Ordering::Greater => ri += 1,
            std::cmp::Ordering::Equal => {
                pairs.push((l_idx, r_idx));
                li += 1;
                ri += 1;
            }
        }
    }
    if pairs.is_empty() {
        return Ok(Some(Vec::new()));
    }

    if !merge_sort_by(
        &mut pairs,
        alignment_steps,
        check_control,
        |(la, _), (lb, _)| la.cmp(&lb),
    )? {
        return Ok(None);
    }

    let pair_count = pairs.len();
    let mut tails_pair_idx = Vec::new();
    tails_pair_idx
        .try_reserve_exact(pair_count)
        .map_err(|_| ChangeError::AllocationFailed)?;
    let mut prev_pair_idx = Vec::new();
    prev_pair_idx
        .try_reserve_exact(pair_count)
        .map_err(|_| ChangeError::AllocationFailed)?;
    prev_pair_idx.resize(pair_count, None);

    for (pair_i, &(_, r_idx)) in pairs.iter().enumerate() {
        if alignment_steps.is_multiple_of(CONTROL_INTERVAL) {
            check_control().map_err(ChangeError::Stopped)?;
        }
        if *alignment_steps >= MAX_ALIGNMENT_STEPS {
            return Ok(None);
        }
        *alignment_steps += 1;
        let mut lo = 0usize;
        let mut hi = tails_pair_idx.len();
        while lo < hi {
            if alignment_steps.is_multiple_of(CONTROL_INTERVAL) {
                check_control().map_err(ChangeError::Stopped)?;
            }
            if *alignment_steps >= MAX_ALIGNMENT_STEPS {
                return Ok(None);
            }
            *alignment_steps += 1;
            let mid = lo + (hi - lo) / 2;
            let is_less = tails_pair_idx
                .get(mid)
                .and_then(|&tail_i| pairs.get(tail_i))
                .is_some_and(|&(_, tail_r)| tail_r < r_idx);
            if is_less {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        let pos = lo;
        if pos > 0
            && let Some(slot) = prev_pair_idx.get_mut(pair_i)
        {
            *slot = tails_pair_idx.get(pos - 1).copied();
        }
        if pos == tails_pair_idx.len() {
            tails_pair_idx.push(pair_i);
        } else if let Some(slot) = tails_pair_idx.get_mut(pos) {
            *slot = pair_i;
        }
    }

    let mut anchors = Vec::new();
    anchors
        .try_reserve_exact(tails_pair_idx.len())
        .map_err(|_| ChangeError::AllocationFailed)?;
    let mut cur = tails_pair_idx.last().copied();
    while let Some(pair_i) = cur {
        if let Some(&pair) = pairs.get(pair_i) {
            anchors.push(pair);
        }
        cur = prev_pair_idx.get(pair_i).copied().flatten();
    }
    anchors.reverse();
    Ok(Some(anchors))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MyersOutcome {
    Aligned,
    OverEditDistanceBudget,
    AlignmentStepBudgetExceeded,
}

/// Bounded Myers `O((n + m) D)` alignment for unanchored sub-windows where all unchanged
/// lines are duplicates (`D <= MAX_MYERS_EDIT_DISTANCE`).
#[allow(clippy::too_many_arguments)]
fn try_bounded_myers(
    alignment: HunkAlignment,
    left_lines: &[LineSlice<'_>],
    l_start: usize,
    l_end: usize,
    right_lines: &[LineSlice<'_>],
    r_start: usize,
    r_end: usize,
    max_hunks: usize,
    hunks: &mut Vec<ChangeHunk>,
    matched_unchanged_lines: &mut usize,
    exact_inserted_lines: &mut usize,
    exact_removed_lines: &mut usize,
    alignment_steps: &mut usize,
    check_control: &mut impl FnMut() -> Result<(), StopReason>,
) -> Result<MyersOutcome, ChangeError> {
    let left_sub = left_lines.get(l_start..l_end).unwrap_or(&[]);
    let right_sub = right_lines.get(r_start..r_end).unwrap_or(&[]);
    let n = left_sub.len();
    let m = right_sub.len();
    let diff_bound = n.abs_diff(m);
    let max_d = MAX_MYERS_EDIT_DISTANCE.min(n.saturating_add(m));
    if diff_bound > max_d {
        return Ok(MyersOutcome::OverEditDistanceBudget);
    }

    let k_offset = max_d.checked_add(1).ok_or(ChangeError::AllocationFailed)?;
    let stride = max_d
        .checked_mul(2)
        .and_then(|v| v.checked_add(3))
        .ok_or(ChangeError::AllocationFailed)?;
    let trace_len = k_offset
        .checked_mul(stride)
        .ok_or(ChangeError::AllocationFailed)?;
    let mut v = Vec::new();
    v.try_reserve_exact(stride)
        .map_err(|_| ChangeError::AllocationFailed)?;
    v.resize(stride, 0usize);
    let mut trace = Vec::new();
    trace
        .try_reserve_exact(trace_len)
        .map_err(|_| ChangeError::AllocationFailed)?;
    trace.resize(trace_len, 0usize);

    let mut reached_d = None;

    'outer: for d in 0..=max_d {
        let idx_min = k_offset
            .checked_sub(d)
            .ok_or(ChangeError::AllocationFailed)?;
        let idx_max = k_offset
            .checked_add(d)
            .ok_or(ChangeError::AllocationFailed)?;
        let mut idx = idx_min;
        while idx <= idx_max {
            if alignment_steps.is_multiple_of(CONTROL_INTERVAL) {
                check_control().map_err(ChangeError::Stopped)?;
            }
            if *alignment_steps >= MAX_ALIGNMENT_STEPS {
                return Ok(MyersOutcome::AlignmentStepBudgetExceeded);
            }
            *alignment_steps += 1;
            let mut x = if d == 0 {
                0usize
            } else {
                let down_idx = idx.checked_add(1).ok_or(ChangeError::AllocationFailed)?;
                let right_idx = idx.checked_sub(1).ok_or(ChangeError::AllocationFailed)?;
                let down = v.get(down_idx).copied().unwrap_or(0);
                let right = v.get(right_idx).copied().unwrap_or(0);
                if idx == idx_min || (idx != idx_max && right < down) {
                    down
                } else {
                    right.saturating_add(1)
                }
            };
            let mut y = x
                .checked_add(k_offset)
                .and_then(|val| val.checked_sub(idx))
                .ok_or(ChangeError::AllocationFailed)?;
            while x < n && y < m {
                if alignment_steps.is_multiple_of(CONTROL_INTERVAL) {
                    check_control().map_err(ChangeError::Stopped)?;
                }
                if *alignment_steps >= MAX_ALIGNMENT_STEPS {
                    return Ok(MyersOutcome::AlignmentStepBudgetExceeded);
                }
                *alignment_steps += 1;
                let (Some(ll), Some(rl)) = (left_sub.get(x), right_sub.get(y)) else {
                    break;
                };
                if !lines_equal(ll, rl) {
                    break;
                }
                x += 1;
                y += 1;
            }
            if let Some(slot) = v.get_mut(idx) {
                *slot = x;
            }
            let row_start = d.checked_mul(stride).ok_or(ChangeError::AllocationFailed)?;
            let row_end = row_start
                .checked_add(stride)
                .ok_or(ChangeError::AllocationFailed)?;
            if x >= n && y >= m {
                if let Some(row) = trace.get_mut(row_start..row_end) {
                    row.copy_from_slice(&v);
                }
                reached_d = Some(d);
                break 'outer;
            }
            idx = idx.checked_add(2).ok_or(ChangeError::AllocationFailed)?;
        }
        let row_start = d.checked_mul(stride).ok_or(ChangeError::AllocationFailed)?;
        let row_end = row_start
            .checked_add(stride)
            .ok_or(ChangeError::AllocationFailed)?;
        if let Some(row) = trace.get_mut(row_start..row_end) {
            row.copy_from_slice(&v);
        }
    }

    let Some(final_d) = reached_d else {
        return Ok(MyersOutcome::OverEditDistanceBudget);
    };

    let mut snakes = Vec::new();
    snakes
        .try_reserve_exact(final_d.saturating_add(1))
        .map_err(|_| ChangeError::AllocationFailed)?;
    let mut x = n;
    let mut y = m;
    for d in (1..=final_d).rev() {
        let idx = x
            .checked_add(k_offset)
            .and_then(|val| val.checked_sub(y))
            .ok_or(ChangeError::AllocationFailed)?;
        let idx_min = k_offset
            .checked_sub(d)
            .ok_or(ChangeError::AllocationFailed)?;
        let idx_max = k_offset
            .checked_add(d)
            .ok_or(ChangeError::AllocationFailed)?;
        let prev_row_start = (d - 1)
            .checked_mul(stride)
            .ok_or(ChangeError::AllocationFailed)?;
        let prev_row_end = prev_row_start
            .checked_add(stride)
            .ok_or(ChangeError::AllocationFailed)?;
        let prev_row = trace.get(prev_row_start..prev_row_end).unwrap_or(&[]);
        let down_idx = idx.checked_add(1).ok_or(ChangeError::AllocationFailed)?;
        let right_idx = idx.checked_sub(1).ok_or(ChangeError::AllocationFailed)?;
        let down = prev_row.get(down_idx).copied().unwrap_or(0);
        let right = prev_row.get(right_idx).copied().unwrap_or(0);
        let (prev_idx, x_start) = if idx == idx_min || (idx != idx_max && right < down) {
            (down_idx, down)
        } else {
            (right_idx, right.saturating_add(1))
        };
        let y_start = x_start
            .checked_add(k_offset)
            .and_then(|val| val.checked_sub(idx))
            .ok_or(ChangeError::AllocationFailed)?;
        if x > x_start {
            snakes.push((x_start, y_start, x - x_start));
        }
        x = prev_row.get(prev_idx).copied().unwrap_or(0);
        y = x
            .checked_add(k_offset)
            .and_then(|val| val.checked_sub(prev_idx))
            .ok_or(ChangeError::AllocationFailed)?;
    }
    if x > 0 {
        snakes.push((0, 0, x));
    }
    snakes.reverse();

    let mut cur_i = 0usize;
    let mut cur_j = 0usize;
    for (x_start, y_start, len) in snakes {
        if cur_i < x_start || cur_j < y_start {
            emit_subwindow_hunk(
                alignment,
                left_lines,
                l_start + cur_i,
                l_start + x_start,
                right_lines,
                r_start + cur_j,
                r_start + y_start,
                max_hunks,
                hunks,
            )?;
            *exact_removed_lines += x_start - cur_i;
            *exact_inserted_lines += y_start - cur_j;
        }
        *matched_unchanged_lines += len;
        cur_i = x_start + len;
        cur_j = y_start + len;
    }
    if cur_i < n || cur_j < m {
        emit_subwindow_hunk(
            alignment,
            left_lines,
            l_start + cur_i,
            l_end,
            right_lines,
            r_start + cur_j,
            r_end,
            max_hunks,
            hunks,
        )?;
        *exact_removed_lines += n - cur_i;
        *exact_inserted_lines += m - cur_j;
    }
    Ok(MyersOutcome::Aligned)
}

#[allow(clippy::too_many_arguments)]
fn align_exact_lcs_subwindow(
    alignment: HunkAlignment,
    left_lines: &[LineSlice<'_>],
    l_start: usize,
    l_end: usize,
    right_lines: &[LineSlice<'_>],
    r_start: usize,
    r_end: usize,
    max_hunks: usize,
    hunks: &mut Vec<ChangeHunk>,
    matched_unchanged_lines: &mut usize,
    exact_inserted_lines: &mut usize,
    exact_removed_lines: &mut usize,
    check_control: &mut impl FnMut() -> Result<(), StopReason>,
) -> Result<(), ChangeError> {
    let left_mid = left_lines.get(l_start..l_end).unwrap_or(&[]);
    let right_mid = right_lines.get(r_start..r_end).unwrap_or(&[]);
    let n = left_mid.len();
    let m = right_mid.len();
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
            let matched = match (left_mid.get(i), right_mid.get(j)) {
                (Some(ll), Some(rl)) => lines_equal(ll, rl),
                _ => false,
            };
            let val = if matched {
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
    let mut backtrack_steps = 0usize;

    while i < n || j < m {
        if backtrack_steps.is_multiple_of(CONTROL_INTERVAL) {
            check_control().map_err(ChangeError::Stopped)?;
        }
        backtrack_steps += 1;
        let matched = match (left_mid.get(i), right_mid.get(j)) {
            (Some(ll), Some(rl)) => lines_equal(ll, rl),
            _ => false,
        };

        if matched {
            if run_start_i < i || run_start_j < j {
                emit_subwindow_hunk(
                    alignment,
                    left_lines,
                    l_start + run_start_i,
                    l_start + i,
                    right_lines,
                    r_start + run_start_j,
                    r_start + j,
                    max_hunks,
                    hunks,
                )?;
                *exact_removed_lines += i - run_start_i;
                *exact_inserted_lines += j - run_start_j;
            }
            *matched_unchanged_lines += 1;
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
        emit_subwindow_hunk(
            alignment,
            left_lines,
            l_start + run_start_i,
            l_end,
            right_lines,
            r_start + run_start_j,
            r_end,
            max_hunks,
            hunks,
        )?;
        *exact_removed_lines += n - run_start_i;
        *exact_inserted_lines += m - run_start_j;
    }
    Ok(())
}

impl<'a> ChangeEvaluation<'a> {
    /// Compute a bounded line/segment-level explanation of the evaluated snapshot pair.
    ///
    /// - Preserves exact UTF-8 byte offsets and line endings (`\r\n` vs `\n`).
    /// - Keeps `Changed`, `Unchanged`, and `Inconclusive` distinct.
    /// - Enforces `limits.max_lines_per_capture()` and `limits.max_hunks()`, returning a
    ///   typed `ChangeError` rather than silently truncating hunks.
    /// - Partitions oversized windows (`sub_n * sub_m > MAX_LCS_CELLS`) around Patience
    ///   unique common line anchors (`O((n + m) log u)`) and bounded Myers alignment
    ///   (`D <= MAX_MYERS_EDIT_DISTANCE`), reporting `LineChangeCounts::AnchorPartitioned`
    ///   and `HunkAlignment::UniqueAnchorPartitioned` when unique-anchor partitioning is
    ///   used, or coalescing only an unanchored high-churn sub-window with
    ///   `HunkAlignment::Coalesced` and `LineChangeCounts::CoalescedWindowUpperBound`.
    pub fn explain_changes(
        &self,
        limits: ChangeExplanationLimits,
        mut check_control: impl FnMut() -> Result<(), StopReason>,
    ) -> Result<ChangeExplanation, ChangeError> {
        check_control().map_err(ChangeError::Stopped)?;
        let (left_text, right_text) = match (self.before.0.capture, self.after.0.capture) {
            (Capture::Complete(left), Capture::Complete(right)) => (left, right),
            _ => {
                let baseline_coverage = CaptureCoverageSummary::from_capture(
                    self.before.0.capture,
                    &mut check_control,
                )?;
                let current_coverage =
                    CaptureCoverageSummary::from_capture(self.after.0.capture, &mut check_control)?;
                check_control().map_err(ChangeError::Stopped)?;
                return Ok(ChangeExplanation::Inconclusive {
                    baseline_coverage,
                    current_coverage,
                });
            }
        };

        if self.comparison == TextComparison::Unchanged {
            let mut line_count = 0usize;
            for (idx, _) in left_text.split_inclusive('\n').enumerate() {
                if idx.is_multiple_of(CONTROL_INTERVAL) {
                    check_control().map_err(ChangeError::Stopped)?;
                }
                line_count += 1;
            }
            check_control().map_err(ChangeError::Stopped)?;
            return Ok(ChangeExplanation::Unchanged {
                method_id: EXPLANATION_METHOD_ID,
                line_count,
            });
        }

        let left_lines = split_lines(left_text, limits.max_lines_per_capture, &mut check_control)?;
        let right_lines =
            split_lines(right_text, limits.max_lines_per_capture, &mut check_control)?;

        let mut hunks = Vec::new();
        let mut matched_unchanged_lines = 0usize;
        let mut exact_inserted_lines = 0usize;
        let mut exact_removed_lines = 0usize;
        let mut coalesced_before_lines = 0usize;
        let mut coalesced_after_lines = 0usize;
        let mut used_anchor_partition = false;
        let mut any_coalesced = false;
        let mut alignment_steps = 0usize;
        let mut trim_steps = 0usize;

        let mut work_stack = Vec::new();
        work_stack
            .try_reserve_exact(64)
            .map_err(|_| ChangeError::AllocationFailed)?;
        work_stack.push((0usize, left_lines.len(), 0usize, right_lines.len(), false));

        while let Some((mut l_start, mut l_end, mut r_start, mut r_end, from_anchor_partition)) =
            work_stack.pop()
        {
            check_control().map_err(ChangeError::Stopped)?;

            // Trim identical leading lines in this sub-window.
            while l_start < l_end && r_start < r_end {
                if trim_steps.is_multiple_of(CONTROL_INTERVAL) {
                    check_control().map_err(ChangeError::Stopped)?;
                }
                trim_steps += 1;
                let (Some(ll), Some(rl)) = (left_lines.get(l_start), right_lines.get(r_start))
                else {
                    break;
                };
                if !lines_equal(ll, rl) {
                    break;
                }
                l_start += 1;
                r_start += 1;
                matched_unchanged_lines += 1;
            }

            // Trim identical trailing lines in this sub-window without overlapping the prefix.
            while l_start < l_end && r_start < r_end {
                if trim_steps.is_multiple_of(CONTROL_INTERVAL) {
                    check_control().map_err(ChangeError::Stopped)?;
                }
                trim_steps += 1;
                let (Some(ll), Some(rl)) = (left_lines.get(l_end - 1), right_lines.get(r_end - 1))
                else {
                    break;
                };
                if !lines_equal(ll, rl) {
                    break;
                }
                l_end -= 1;
                r_end -= 1;
                matched_unchanged_lines += 1;
            }

            let n = l_end - l_start;
            let m = r_end - r_start;
            if n == 0 && m == 0 {
                continue;
            }
            let exact_alignment = if from_anchor_partition {
                HunkAlignment::UniqueAnchorPartitioned
            } else {
                HunkAlignment::ExactMinimal
            };
            if n == 0 || m == 0 {
                emit_subwindow_hunk(
                    exact_alignment,
                    &left_lines,
                    l_start,
                    l_end,
                    &right_lines,
                    r_start,
                    r_end,
                    limits.max_hunks,
                    &mut hunks,
                )?;
                exact_removed_lines += n;
                exact_inserted_lines += m;
                continue;
            }

            let cells = n.saturating_mul(m);
            if cells <= MAX_LCS_CELLS {
                align_exact_lcs_subwindow(
                    exact_alignment,
                    &left_lines,
                    l_start,
                    l_end,
                    &right_lines,
                    r_start,
                    r_end,
                    limits.max_hunks,
                    &mut hunks,
                    &mut matched_unchanged_lines,
                    &mut exact_inserted_lines,
                    &mut exact_removed_lines,
                    &mut check_control,
                )?;
                continue;
            }

            let coalesced_reason = if alignment_steps >= MAX_ALIGNMENT_STEPS {
                CoalescedReason::AlignmentStepBudgetExceeded
            } else {
                let left_sub = left_lines.get(l_start..l_end).unwrap_or(&[]);
                let right_sub = right_lines.get(r_start..r_end).unwrap_or(&[]);
                match find_unique_anchor_lis(
                    left_sub,
                    right_sub,
                    &mut alignment_steps,
                    &mut check_control,
                )? {
                    None => CoalescedReason::AlignmentStepBudgetExceeded,
                    Some(anchors) if !anchors.is_empty() => {
                        used_anchor_partition = true;
                        matched_unchanged_lines += anchors.len();
                        work_stack
                            .try_reserve(anchors.len() + 1)
                            .map_err(|_| ChangeError::AllocationFailed)?;
                        if let Some(&(last_li, last_rj)) = anchors.last() {
                            let sub_l_start = l_start + last_li + 1;
                            let sub_r_start = r_start + last_rj + 1;
                            if sub_l_start < l_end || sub_r_start < r_end {
                                work_stack.push((sub_l_start, l_end, sub_r_start, r_end, true));
                            }
                        }
                        for pair_idx in (0..anchors.len().saturating_sub(1)).rev() {
                            if let (Some(&(li_a, rj_a)), Some(&(li_b, rj_b))) =
                                (anchors.get(pair_idx), anchors.get(pair_idx + 1))
                            {
                                let sub_l_start = l_start + li_a + 1;
                                let sub_l_end = l_start + li_b;
                                let sub_r_start = r_start + rj_a + 1;
                                let sub_r_end = r_start + rj_b;
                                if sub_l_start < sub_l_end || sub_r_start < sub_r_end {
                                    work_stack.push((
                                        sub_l_start,
                                        sub_l_end,
                                        sub_r_start,
                                        sub_r_end,
                                        true,
                                    ));
                                }
                            }
                        }
                        if let Some(&(first_li, first_rj)) = anchors.first() {
                            let sub_l_end = l_start + first_li;
                            let sub_r_end = r_start + first_rj;
                            if l_start < sub_l_end || r_start < sub_r_end {
                                work_stack.push((l_start, sub_l_end, r_start, sub_r_end, true));
                            }
                        }
                        continue;
                    }
                    Some(_) => match try_bounded_myers(
                        exact_alignment,
                        &left_lines,
                        l_start,
                        l_end,
                        &right_lines,
                        r_start,
                        r_end,
                        limits.max_hunks,
                        &mut hunks,
                        &mut matched_unchanged_lines,
                        &mut exact_inserted_lines,
                        &mut exact_removed_lines,
                        &mut alignment_steps,
                        &mut check_control,
                    )? {
                        MyersOutcome::Aligned => continue,
                        MyersOutcome::AlignmentStepBudgetExceeded => {
                            CoalescedReason::AlignmentStepBudgetExceeded
                        }
                        MyersOutcome::OverEditDistanceBudget => {
                            CoalescedReason::NoUniqueAnchorsOverEditBudget
                        }
                    },
                }
            };

            emit_subwindow_hunk(
                HunkAlignment::Coalesced(coalesced_reason),
                &left_lines,
                l_start,
                l_end,
                &right_lines,
                r_start,
                r_end,
                limits.max_hunks,
                &mut hunks,
            )?;
            any_coalesced = true;
            coalesced_before_lines += n;
            coalesced_after_lines += m;
        }

        let line_counts = if any_coalesced {
            LineChangeCounts::CoalescedWindowUpperBound {
                trimmed_unchanged_lines: matched_unchanged_lines,
                coalesced_before_window_lines: exact_removed_lines + coalesced_before_lines,
                coalesced_after_window_lines: exact_inserted_lines + coalesced_after_lines,
            }
        } else if used_anchor_partition {
            LineChangeCounts::AnchorPartitioned {
                matched_unchanged_lines,
                inserted_lines: exact_inserted_lines,
                removed_lines: exact_removed_lines,
            }
        } else {
            LineChangeCounts::Exact {
                unchanged_lines: matched_unchanged_lines,
                inserted_lines: exact_inserted_lines,
                removed_lines: exact_removed_lines,
            }
        };

        check_control().map_err(ChangeError::Stopped)?;
        Ok(ChangeExplanation::Changed {
            method_id: EXPLANATION_METHOD_ID,
            before_line_count: left_lines.len(),
            after_line_count: right_lines.len(),
            line_counts,
            hunks,
        })
    }

    /// Project an explained notification record that exposes source coverage, freshness,
    /// attribution, the saved rule that triggered it, and the line-level change explanation
    /// (`PRIVACY-03`, `A18`).
    ///
    /// When line/segment decomposition exceeds `limits.max_lines_per_capture()` or
    /// `limits.max_hunks()`, the notification still succeeds with
    /// `ChangeExplanation::Unavailable` rather than failing outright, keeping the
    /// `RuleDecision`, capture coverage, freshness, and top-level `TextComparison`
    /// replacement span inspectable.
    pub fn explain_notification(
        &'a self,
        limits: ChangeExplanationLimits,
        mut check_control: impl FnMut() -> Result<(), StopReason>,
    ) -> Result<ExplainedNotification<'a>, ChangeError> {
        let baseline_coverage =
            CaptureCoverageSummary::from_capture(self.before.0.capture, &mut check_control)?;
        let current_coverage =
            CaptureCoverageSummary::from_capture(self.after.0.capture, &mut check_control)?;
        let before_line_count = match &baseline_coverage {
            CaptureCoverageSummary::Complete { line_count, .. } => *line_count,
            _ => 0,
        };
        let after_line_count = match &current_coverage {
            CaptureCoverageSummary::Complete { line_count, .. } => *line_count,
            _ => 0,
        };
        let explanation = match self.explain_changes(limits, &mut check_control) {
            Ok(explanation) => explanation,
            Err(ChangeError::LineLimitExceeded {
                actual_lines,
                max_lines,
            }) => {
                check_control().map_err(ChangeError::Stopped)?;
                ChangeExplanation::Unavailable {
                    before_line_count,
                    after_line_count,
                    reason: ExplanationUnavailableReason::LineLimitExceeded {
                        actual_lines,
                        max_lines,
                    },
                }
            }
            Err(ChangeError::HunkLimitExceeded { max_hunks }) => {
                check_control().map_err(ChangeError::Stopped)?;
                ChangeExplanation::Unavailable {
                    before_line_count,
                    after_line_count,
                    reason: ExplanationUnavailableReason::HunkLimitExceeded { max_hunks },
                }
            }
            Err(err) => return Err(err),
        };
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
