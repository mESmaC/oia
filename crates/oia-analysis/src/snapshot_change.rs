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
    TextTooLarge { actual_bytes: usize },
    ZeroMaxAge,
    ScopeMismatch,
    ReusedSnapshotVersion,
    ReversedRetrievalOrder,
    EvaluationBeforeRetrieval,
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
