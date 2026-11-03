//! Bounded comparison of accounts of one incident (#138; `INTEL-01`, `REASON-03`,
//! `FIELD-02`).
//!
//! Callers supply pinned source versions (witness statements, police reports,
//! news reports, revisions of one account) with analyst-coded detail assertions
//! that cite exact wording by byte range. The kernel reports shared, unique,
//! conflicting and uncoded aspects; a pairwise lexical overlap matrix; drift
//! between revisions of one account; and long exact shared word runs that suggest
//! copying or quotation.
//!
//! Two groupings are reported. The declared grouping merges accounts only through
//! declared derivations; its reported-group count is an upper bound on independent
//! reported sources, provided the declarations are correct. The candidate-adjusted
//! grouping also merges accounts through undismissed copy candidates, so a copied
//! account lowers the count instead of raising corroboration. A false candidate
//! can push that count below the true value, so it is not a bound; a reviewer can
//! dismiss a candidate. Simulated accounts, and reported accounts whose declared
//! lineage is only simulated, never count as reported support.
//!
//! Inputs and results are transient analysis: no I/O, persistence, extraction,
//! model inference or normalization of the source text. Overlap is lexical. It is
//! never agreement about events, a probability, or proof of independence.

pub use crate::multilingual::StopReason;
use oia_domain::text::is_format_control;
use oia_domain::witness::AccountOrigin;
use oia_domain::{ActorId, OpaqueId, VersionId};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;
use std::ops::Range;

/// Pin this identifier and the parameters in [`MethodRecord`] with any persisted result.
pub const METHOD_ID: &str = "word-shingle-jaccard+exact-word-run-v2";
/// How text is split into the words that both measures compare.
pub const TOKENIZATION: &str = "maximal runs of Unicode alphanumeric characters, compared after \
     char::to_lowercase; every other character separates words. No Unicode normalization is \
     applied, so composed and decomposed forms of the same text (NFC/NFD) do not match. Scripts \
     written without word separators (for example Han, Kana, Thai, Lao, Khmer, Myanmar) form \
     very long words; such sources are marked UnsegmentedText and their overlap and copy \
     results are unreliable.";
pub const OVERLAP_CAVEAT: &str = "Overlap measures shared wording in the captured text. It is not \
     agreement about what happened, not evidence of independence, and not a probability.";
pub const DECLARED_GROUPS_CAVEAT: &str = "The declared reported-group count is an upper bound on \
     independent reported sources only if the declared derivations are correct. Paraphrase, \
     shared briefings, interviews or a common outside source can make accounts dependent \
     without any declaration or shared wording.";
pub const CANDIDATE_GROUPS_CAVEAT: &str = "The candidate-adjusted count also merges accounts that \
     share a long exact word run. A candidate can be a legitimate quotation of a common public \
     text, so this count can fall below the true number of independent sources. It is not a \
     bound; review or dismiss candidates before relying on it.";
pub const SIMULATED_CAVEAT: &str = "Simulated accounts, and reported accounts whose declared \
     in-set lineage is only simulated, never count as reported support or as reported groups.";
pub const CODING_CAVEAT: &str = "Aspects and coded values are analyst coding over cited wording, \
     not observations or measurements. An uncoded aspect is not an observed absence.";

/// Hard bounds. Oversized input is refused, never truncated.
pub const MAX_SOURCES: usize = 32;
pub const MAX_TEXT_BYTES: usize = 262_144;
pub const MAX_TOTAL_TEXT_BYTES: usize = 2_097_152;
pub const MAX_ASSERTIONS_PER_SOURCE: usize = 256;
pub const MAX_DERIVATIONS_PER_SOURCE: usize = 32;
pub const MAX_DISMISSALS: usize = 256;
pub const MAX_LABEL_BYTES: usize = 256;
pub const MAX_DESCRIPTION_BYTES: usize = 4_096;
pub const SHINGLE_WORDS_RANGE: Range<usize> = 2..17;
pub const COPY_RUN_WORDS_RANGE: Range<usize> = 6..257;
/// Per cross-account pair; reaching either bound marks that pair's search truncated.
pub const MAX_COPY_CANDIDATES_PER_PAIR: usize = 64;
pub const MAX_COPY_PROBES_PER_PAIR: usize = 4_000_000;
/// A word longer than this many characters marks its source `UnsegmentedText`.
pub const UNSEGMENTED_WORD_CHARS: usize = 40;
const CONTROL_INTERVAL: usize = 1_024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceClass {
    Testimony,
    PoliceReport,
    NewsReport,
    Other,
}

/// Completeness of this captured representation. Empty complete text differs
/// from unavailable text; partial text is compared but marked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextCapture<'a> {
    Complete(&'a str),
    Partial { text: &'a str, reason: &'a str },
    Unavailable { reason: &'a str },
}

impl<'a> TextCapture<'a> {
    fn text(&self) -> Option<&'a str> {
        match *self {
            Self::Complete(text) | Self::Partial { text, .. } => Some(text),
            Self::Unavailable { .. } => None,
        }
    }

    fn coverage(&self) -> TextCoverage {
        match self {
            Self::Complete(_) => TextCoverage::Complete,
            Self::Partial { .. } => TextCoverage::Partial,
            Self::Unavailable { .. } => TextCoverage::Unavailable,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TextCoverage {
    Complete,
    Partial,
    Unavailable,
}

/// A coded value never replaces the cited wording. `StatedUnknown` means the
/// source says the detail is unknown; an aspect a source does not mention is
/// simply absent from its assertions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CodedValue<'a> {
    Reported(&'a str),
    StatedUnknown,
}

/// One analyst-coded detail citing a UTF-8 byte range of the source text that
/// contains non-whitespace characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetailAssertion<'a> {
    pub aspect: &'a str,
    pub value: CodedValue<'a>,
    pub wording: Range<usize>,
}

/// In-workflow exposure to candidate explanations before this version (FIELD-02).
/// `NotExposedInWorkflow` says nothing about exposure outside the workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplanationExposure {
    Unknown,
    NotExposedInWorkflow,
    AfterExplanations,
}

/// One pinned version of one account. Versions of the same account share
/// `account` and are ordered by the caller's `revision` ordinal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatementSource<'a> {
    pub account: OpaqueId,
    pub revision: u32,
    pub version: VersionId,
    pub class: SourceClass,
    pub origin: AccountOrigin,
    pub exposure: ExplanationExposure,
    /// Source reference and credit, without credentials or private tokens.
    pub attribution: &'a str,
    pub text: TextCapture<'a>,
    pub assertions: &'a [DetailAssertion<'a>],
    /// Accounts this version declares it draws on (a report quoting a witness).
    /// Accounts outside the compared set are retained but cannot join groups.
    pub derived_from: &'a [OpaqueId],
}

/// A reviewer's decision that one copy candidate is not evidence of dependence,
/// for example both accounts quote the same public appeal. The candidate stays
/// listed with this dismissal but no longer merges candidate-adjusted groups.
/// Candidates are identified by their version pair and byte ranges, in either order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateDismissal<'a> {
    pub a_version: VersionId,
    pub a_range: Range<usize>,
    pub b_version: VersionId,
    pub b_range: Range<usize>,
    pub reviewed_by: ActorId,
    pub reason: &'a str,
}

/// Which version represents each account in the aspect table. The default is
/// `Initial`, the account preserved before any explanations (FIELD-02).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Representative {
    Initial,
    Latest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComparisonParameters {
    pub shingle_words: usize,
    pub copy_run_words: usize,
    pub representative: Representative,
}

impl Default for ComparisonParameters {
    /// Five-word shingles and twelve-word runs: shorter exact runs are common
    /// phrasing ("a bright white light") rather than evidence of copying.
    fn default() -> Self {
        Self {
            shingle_words: 5,
            copy_run_words: 12,
            representative: Representative::Initial,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComparisonRequest<'a> {
    /// The caller's saved coding scheme and coder; assertions belong to it.
    pub coding_version: VersionId,
    pub coded_by: ActorId,
    pub parameters: ComparisonParameters,
    pub sources: &'a [StatementSource<'a>],
    pub dismissals: &'a [CandidateDismissal<'a>],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComparisonError {
    TooFewSources(usize),
    TooManySources(usize),
    TooManyDismissals(usize),
    InvalidParameter(&'static str),
    DuplicateVersion {
        source: usize,
    },
    DuplicateRevision {
        source: usize,
    },
    InconsistentAccount {
        source: usize,
        field: &'static str,
    },
    EmptyField {
        source: usize,
        field: &'static str,
    },
    FieldTooLong {
        source: usize,
        field: &'static str,
    },
    InvalidCharacter {
        source: usize,
        field: &'static str,
    },
    TextTooLarge {
        source: usize,
        bytes: usize,
    },
    TotalTextTooLarge {
        bytes: usize,
    },
    TooManyAssertions {
        source: usize,
        count: usize,
    },
    TooManyDerivations {
        source: usize,
        count: usize,
    },
    SelfDerivation {
        source: usize,
    },
    WordingWithoutText {
        source: usize,
        assertion: usize,
    },
    InvalidWording {
        source: usize,
        assertion: usize,
    },
    InvalidDismissal {
        dismissal: usize,
        field: &'static str,
    },
    Stopped(StopReason),
}

impl fmt::Display for ComparisonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooFewSources(n) => write!(f, "{n} source versions; at least 2 are required"),
            Self::TooManySources(n) => write!(f, "{n} source versions; limit is {MAX_SOURCES}"),
            Self::TooManyDismissals(n) => {
                write!(f, "{n} candidate dismissals; limit is {MAX_DISMISSALS}")
            }
            Self::InvalidParameter(name) => write!(f, "parameter {name} is outside its range"),
            Self::DuplicateVersion { source } => {
                write!(f, "source {source} repeats an earlier version id")
            }
            Self::DuplicateRevision { source } => {
                write!(
                    f,
                    "source {source} repeats a revision ordinal of its account"
                )
            }
            Self::InconsistentAccount { source, field } => {
                write!(f, "source {source} changes the account's {field}")
            }
            Self::EmptyField { source, field } => {
                write!(f, "source {source}: {field} must contain text")
            }
            Self::FieldTooLong { source, field } => {
                write!(f, "source {source}: {field} exceeds its byte limit")
            }
            Self::InvalidCharacter { source, field } => {
                write!(
                    f,
                    "source {source}: {field} contains a control or format character"
                )
            }
            Self::TextTooLarge { source, bytes } => {
                write!(
                    f,
                    "source {source} has {bytes} bytes; limit is {MAX_TEXT_BYTES}"
                )
            }
            Self::TotalTextTooLarge { bytes } => {
                write!(
                    f,
                    "{bytes} text bytes in total; limit is {MAX_TOTAL_TEXT_BYTES}"
                )
            }
            Self::TooManyAssertions { source, count } => write!(
                f,
                "source {source} has {count} assertions; limit is {MAX_ASSERTIONS_PER_SOURCE}"
            ),
            Self::TooManyDerivations { source, count } => write!(
                f,
                "source {source} declares {count} derivations; limit is {MAX_DERIVATIONS_PER_SOURCE}"
            ),
            Self::SelfDerivation { source } => {
                write!(
                    f,
                    "source {source} declares a derivation from its own account"
                )
            }
            Self::WordingWithoutText { source, assertion } => write!(
                f,
                "source {source} assertion {assertion} cites wording but the text is unavailable"
            ),
            Self::InvalidWording { source, assertion } => write!(
                f,
                "source {source} assertion {assertion} cites an empty, whitespace-only, \
                 out-of-range or non-character-boundary byte range"
            ),
            Self::InvalidDismissal { dismissal, field } => {
                write!(f, "candidate dismissal {dismissal}: invalid {field}")
            }
            Self::Stopped(reason) => write!(f, "statement comparison stopped: {reason:?}"),
        }
    }
}

impl std::error::Error for ComparisonError {}

/// Everything a reader needs to reproduce or question the figures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodRecord {
    pub id: &'static str,
    pub tokenization: &'static str,
    pub parameters: ComparisonParameters,
    pub coding_version: VersionId,
    pub coded_by: ActorId,
    pub overlap_caveat: &'static str,
    pub declared_groups_caveat: &'static str,
    pub candidate_groups_caveat: &'static str,
    pub simulated_caveat: &'static str,
    pub coding_caveat: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSummary<'a> {
    pub index: usize,
    pub account: &'a OpaqueId,
    pub version: &'a VersionId,
    pub revision: u32,
    pub class: SourceClass,
    pub origin: AccountOrigin,
    pub exposure: ExplanationExposure,
    pub attribution: &'a str,
    pub coverage: TextCoverage,
    pub word_count: usize,
    pub longest_word_chars: usize,
    pub assertion_count: usize,
    /// Whether this version represents its account in the aspect table.
    pub representative: bool,
    /// False for simulated accounts and for reported accounts whose declared
    /// in-set lineage is only simulated.
    pub counts_as_reported: bool,
    /// Indices into [`StatementComparison::declared`] and
    /// [`StatementComparison::candidate_adjusted`] groups.
    pub declared_group: usize,
    pub candidate_adjusted_group: usize,
}

/// Exact rational; no floating-point rounding enters the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ratio {
    pub numerator: u64,
    pub denominator: u64,
}

impl Ratio {
    /// Display convenience only.
    pub fn as_f64(self) -> f64 {
        self.numerator as f64 / self.denominator as f64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlapMeasure {
    /// Jaccard index of distinct word shingles: shared / union.
    ShingleJaccard(Ratio),
    /// At least one text has fewer words than one shingle; not measurable.
    TooShort,
    /// At least one text is unavailable.
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopySearch {
    /// Same-account revisions share wording by construction; see drift instead.
    SameAccount,
    Complete,
    /// A per-pair bound was reached; further shared runs may exist.
    Truncated,
    /// The search ran, but a text lacks word separators, so runs are unreliable.
    UnsegmentedText,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairOverlap {
    pub a: usize,
    pub b: usize,
    pub same_account: bool,
    /// The weaker coverage of the two texts.
    pub coverage: TextCoverage,
    pub overlap: OverlapMeasure,
    pub copy_search: CopySearch,
    pub copy_candidates: usize,
}

/// A maximal exact shared word run between different accounts: possible
/// copying or quotation, not established plagiarism and not corroboration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyCandidate<'a> {
    pub a: usize,
    pub b: usize,
    pub a_range: Range<usize>,
    pub b_range: Range<usize>,
    pub words: usize,
    pub a_text: &'a str,
    pub b_text: &'a str,
    /// Index into the request's dismissals when a reviewer dismissed it.
    pub dismissal: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyReason<'a> {
    DeclaredDerivation {
        source: usize,
        derived_from: &'a OpaqueId,
    },
    SharedWording {
        candidate: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndependenceGroup<'a> {
    pub accounts: Vec<&'a OpaqueId>,
    pub reasons: Vec<DependencyReason<'a>>,
    /// At least one member counts as reported support.
    pub counts_as_reported: bool,
    pub includes_simulated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grouping<'a> {
    pub groups: Vec<IndependenceGroup<'a>>,
    /// Groups with `counts_as_reported`.
    pub reported_group_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AspectReport<'a> {
    pub source: usize,
    pub value: CodedValue<'a>,
    pub wording: Range<usize>,
    pub wording_text: &'a str,
    pub origin: AccountOrigin,
    pub exposure: ExplanationExposure,
}

/// Support for one coded value. Group counts include only accounts that count
/// as reported support.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant<'a> {
    pub value: &'a str,
    pub sources: Vec<usize>,
    pub accounts: usize,
    pub reported_accounts: usize,
    pub simulated_accounts: usize,
    pub declared_groups: usize,
    pub candidate_adjusted_groups: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AspectStatus {
    /// One coded value, with reported support from at least two
    /// candidate-adjusted groups.
    Converging,
    /// One coded value from two or more reported accounts that fall in one
    /// candidate-adjusted group; see the variant's declared group count.
    RepeatedWithinOneGroup,
    /// One coded value from one reported account.
    Unique,
    /// One coded value with no reported support (simulated only, or lineage
    /// declared only from simulated accounts).
    NoReportedSupport,
    /// Two or more coded values. Variants stay separate; none is preferred.
    Conflicting,
    /// Every report states the detail is unknown.
    StatedUnknownOnly,
}

/// A representative version with no coded assertion for this aspect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Omission {
    pub source: usize,
    pub coverage: TextCoverage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AspectComparison<'a> {
    pub aspect: &'a str,
    pub status: AspectStatus,
    pub variants: Vec<Variant<'a>>,
    pub stated_unknown_by: Vec<usize>,
    pub reports: Vec<AspectReport<'a>>,
    pub not_coded_in: Vec<Omission>,
}

/// Coded values of one aspect before and after a revision; an empty side means
/// the aspect was added or removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AspectChange<'a> {
    pub aspect: &'a str,
    pub before: Vec<CodedValue<'a>>,
    pub after: Vec<CodedValue<'a>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriftStep<'a> {
    pub account: &'a OpaqueId,
    pub from: usize,
    pub to: usize,
    pub to_exposure: ExplanationExposure,
    /// `None` when either text is unavailable.
    pub text_identical: Option<bool>,
    pub overlap: OverlapMeasure,
    pub changes: Vec<AspectChange<'a>>,
    pub unchanged_aspects: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limitation {
    TextPartial {
        source: usize,
    },
    TextUnavailable {
        source: usize,
    },
    TooShortForShingles {
        source: usize,
    },
    UnsegmentedText {
        source: usize,
        longest_word_chars: usize,
    },
    CopySearchTruncated {
        a: usize,
        b: usize,
    },
    UnmatchedDismissal {
        dismissal: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatementComparison<'a> {
    pub method: MethodRecord,
    pub sources: Vec<SourceSummary<'a>>,
    pub account_count: usize,
    pub reported_account_count: usize,
    /// Declared derivations only. Its reported-group count is an upper bound
    /// under [`DECLARED_GROUPS_CAVEAT`].
    pub declared: Grouping<'a>,
    /// Declared derivations plus undismissed copy candidates. Not a bound
    /// ([`CANDIDATE_GROUPS_CAVEAT`]).
    pub candidate_adjusted: Grouping<'a>,
    pub pairs: Vec<PairOverlap>,
    pub copy_candidates: Vec<CopyCandidate<'a>>,
    pub aspects: Vec<AspectComparison<'a>>,
    pub drift: Vec<DriftStep<'a>>,
    pub limitations: Vec<Limitation>,
    pub contains_simulated: bool,
}

impl StatementComparison<'_> {
    /// A partial result is usable but must be presented with its limitations.
    pub fn is_partial(&self) -> bool {
        !self.limitations.is_empty()
    }
}

struct Word {
    norm: String,
    range: Range<usize>,
}

struct Prepared<'a> {
    text: Option<&'a str>,
    words: Vec<Word>,
    account: usize,
    longest_word_chars: usize,
    unsegmented: bool,
}

struct Control<F> {
    check: F,
    ticks: usize,
}

impl<F: FnMut() -> Result<(), StopReason>> Control<F> {
    fn now(&mut self) -> Result<(), ComparisonError> {
        (self.check)().map_err(ComparisonError::Stopped)
    }

    fn tick(&mut self) -> Result<(), ComparisonError> {
        self.ticks = self.ticks.wrapping_add(1);
        if self.ticks.is_multiple_of(CONTROL_INTERVAL) {
            self.now()?;
        }
        Ok(())
    }
}

enum FieldProblem {
    TooLong,
    Empty,
    Character,
}

fn check_text_field(value: &str, limit: usize, structural: bool) -> Result<(), FieldProblem> {
    if value.len() > limit {
        return Err(FieldProblem::TooLong);
    }
    if value.trim().is_empty() {
        return Err(FieldProblem::Empty);
    }
    if structural
        && value
            .chars()
            .any(|c| c.is_control() || is_format_control(c))
    {
        return Err(FieldProblem::Character);
    }
    Ok(())
}

fn validate_text_field(
    source: usize,
    field: &'static str,
    value: &str,
    limit: usize,
    structural: bool,
) -> Result<(), ComparisonError> {
    check_text_field(value, limit, structural).map_err(|problem| match problem {
        FieldProblem::TooLong => ComparisonError::FieldTooLong { source, field },
        FieldProblem::Empty => ComparisonError::EmptyField { source, field },
        FieldProblem::Character => ComparisonError::InvalidCharacter { source, field },
    })
}

fn validate(request: &ComparisonRequest<'_>) -> Result<(), ComparisonError> {
    let sources = request.sources;
    if sources.len() < 2 {
        return Err(ComparisonError::TooFewSources(sources.len()));
    }
    if sources.len() > MAX_SOURCES {
        return Err(ComparisonError::TooManySources(sources.len()));
    }
    if request.dismissals.len() > MAX_DISMISSALS {
        return Err(ComparisonError::TooManyDismissals(request.dismissals.len()));
    }
    let p = request.parameters;
    if !SHINGLE_WORDS_RANGE.contains(&p.shingle_words) {
        return Err(ComparisonError::InvalidParameter("shingle_words"));
    }
    if !COPY_RUN_WORDS_RANGE.contains(&p.copy_run_words) {
        return Err(ComparisonError::InvalidParameter("copy_run_words"));
    }
    let mut versions = HashSet::new();
    let mut revisions = HashSet::new();
    let mut accounts: HashMap<&OpaqueId, (SourceClass, AccountOrigin)> = HashMap::new();
    let mut total = 0usize;
    for (index, source) in sources.iter().enumerate() {
        if !versions.insert(&source.version) {
            return Err(ComparisonError::DuplicateVersion { source: index });
        }
        if !revisions.insert((&source.account, source.revision)) {
            return Err(ComparisonError::DuplicateRevision { source: index });
        }
        let (class, origin) = *accounts
            .entry(&source.account)
            .or_insert((source.class, source.origin));
        if class != source.class {
            return Err(ComparisonError::InconsistentAccount {
                source: index,
                field: "source class",
            });
        }
        if origin != source.origin {
            return Err(ComparisonError::InconsistentAccount {
                source: index,
                field: "origin",
            });
        }
        validate_text_field(
            index,
            "attribution",
            source.attribution,
            MAX_DESCRIPTION_BYTES,
            false,
        )?;
        match source.text {
            TextCapture::Partial { reason, .. } | TextCapture::Unavailable { reason } => {
                validate_text_field(
                    index,
                    "coverage reason",
                    reason,
                    MAX_DESCRIPTION_BYTES,
                    false,
                )?
            }
            TextCapture::Complete(_) => {}
        }
        let text = source.text.text();
        if let Some(text) = text {
            if text.len() > MAX_TEXT_BYTES {
                return Err(ComparisonError::TextTooLarge {
                    source: index,
                    bytes: text.len(),
                });
            }
            total += text.len();
        }
        if source.assertions.len() > MAX_ASSERTIONS_PER_SOURCE {
            return Err(ComparisonError::TooManyAssertions {
                source: index,
                count: source.assertions.len(),
            });
        }
        if source.derived_from.len() > MAX_DERIVATIONS_PER_SOURCE {
            return Err(ComparisonError::TooManyDerivations {
                source: index,
                count: source.derived_from.len(),
            });
        }
        if source.derived_from.contains(&source.account) {
            return Err(ComparisonError::SelfDerivation { source: index });
        }
        for (a, assertion) in source.assertions.iter().enumerate() {
            validate_text_field(index, "aspect", assertion.aspect, MAX_LABEL_BYTES, true)?;
            if let CodedValue::Reported(value) = assertion.value {
                validate_text_field(index, "coded value", value, MAX_LABEL_BYTES, true)?;
            }
            let Some(text) = text else {
                return Err(ComparisonError::WordingWithoutText {
                    source: index,
                    assertion: a,
                });
            };
            let cited = text.get(assertion.wording.clone());
            if !cited.is_some_and(|w| !w.trim().is_empty()) {
                return Err(ComparisonError::InvalidWording {
                    source: index,
                    assertion: a,
                });
            }
        }
    }
    if total > MAX_TOTAL_TEXT_BYTES {
        return Err(ComparisonError::TotalTextTooLarge { bytes: total });
    }
    for (dismissal, d) in request.dismissals.iter().enumerate() {
        check_text_field(d.reason, MAX_DESCRIPTION_BYTES, false).map_err(|_| {
            ComparisonError::InvalidDismissal {
                dismissal,
                field: "reason",
            }
        })?;
        if d.a_range.start >= d.a_range.end || d.b_range.start >= d.b_range.end {
            return Err(ComparisonError::InvalidDismissal {
                dismissal,
                field: "range",
            });
        }
    }
    Ok(())
}

/// Scripts conventionally written without spaces between words.
fn is_unsegmented_script(ch: char) -> bool {
    matches!(
        ch,
        '\u{0E00}'..='\u{0EFF}'
            | '\u{1000}'..='\u{109F}'
            | '\u{1780}'..='\u{17FF}'
            | '\u{3040}'..='\u{30FF}'
            | '\u{31F0}'..='\u{31FF}'
            | '\u{3400}'..='\u{4DBF}'
            | '\u{4E00}'..='\u{9FFF}'
            | '\u{F900}'..='\u{FAFF}'
            | '\u{20000}'..='\u{3134F}'
    )
}

struct Tokens {
    words: Vec<Word>,
    longest_word_chars: usize,
    unsegmented_script: bool,
}

fn tokenize<F: FnMut() -> Result<(), StopReason>>(
    text: &str,
    control: &mut Control<F>,
) -> Result<Tokens, ComparisonError> {
    let mut tokens = Tokens {
        words: Vec::new(),
        longest_word_chars: 0,
        unsegmented_script: false,
    };
    let mut start: Option<usize> = None;
    let mut chars = 0usize;
    for (offset, ch) in text.char_indices() {
        control.tick()?;
        if ch.is_alphanumeric() {
            tokens.unsegmented_script |= is_unsegmented_script(ch);
            chars += 1;
            start.get_or_insert(offset);
        } else if let Some(begin) = start.take() {
            tokens.words.push(word(text, begin..offset));
            tokens.longest_word_chars = tokens.longest_word_chars.max(chars);
            chars = 0;
        }
    }
    if let Some(begin) = start {
        tokens.words.push(word(text, begin..text.len()));
        tokens.longest_word_chars = tokens.longest_word_chars.max(chars);
    }
    Ok(tokens)
}

fn word(text: &str, range: Range<usize>) -> Word {
    let norm = text
        .get(range.clone())
        .unwrap_or_default()
        .chars()
        .flat_map(char::to_lowercase)
        .collect();
    Word { norm, range }
}

fn shingles<'w, F: FnMut() -> Result<(), StopReason>>(
    norms: &'w [&'w str],
    k: usize,
    control: &mut Control<F>,
) -> Result<HashSet<&'w [&'w str]>, ComparisonError> {
    let mut set = HashSet::new();
    if norms.len() >= k {
        for window in norms.windows(k) {
            control.tick()?;
            set.insert(window);
        }
    }
    Ok(set)
}

fn jaccard<F: FnMut() -> Result<(), StopReason>>(
    a: &HashSet<&[&str]>,
    b: &HashSet<&[&str]>,
    control: &mut Control<F>,
) -> Result<OverlapMeasure, ComparisonError> {
    if a.is_empty() || b.is_empty() {
        return Ok(OverlapMeasure::TooShort);
    }
    let (small, large) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    let mut shared = 0u64;
    for shingle in small {
        control.tick()?;
        if large.contains(shingle) {
            shared += 1;
        }
    }
    let union = (a.len() + b.len()) as u64 - shared;
    Ok(OverlapMeasure::ShingleJaccard(Ratio {
        numerator: shared,
        denominator: union,
    }))
}

/// `(start in a, start in b, words)` per run, and whether the search was truncated.
type SharedRuns = (Vec<(usize, usize, usize)>, bool);

/// Maximal exact runs of at least `m` words. A run starting where the previous
/// words also match is a suffix of a longer run and is skipped. Probes and
/// candidates are bounded per pair; the bool reports truncation.
fn shared_runs<F: FnMut() -> Result<(), StopReason>>(
    a: &[&str],
    b: &[&str],
    m: usize,
    control: &mut Control<F>,
) -> Result<SharedRuns, ComparisonError> {
    let mut runs = Vec::new();
    if a.len() < m || b.len() < m {
        return Ok((runs, false));
    }
    let mut index: HashMap<&[&str], Vec<usize>> = HashMap::new();
    for i in 0..=a.len() - m {
        control.tick()?;
        index.entry(&a[i..i + m]).or_default().push(i);
    }
    let mut probes = 0usize;
    for j in 0..=b.len() - m {
        control.tick()?;
        let Some(starts) = index.get(&b[j..j + m]) else {
            continue;
        };
        for &i in starts {
            probes += 1;
            if probes > MAX_COPY_PROBES_PER_PAIR {
                return Ok((runs, true));
            }
            control.tick()?;
            if i > 0 && j > 0 && a[i - 1] == b[j - 1] {
                continue;
            }
            let mut len = m;
            while i + len < a.len() && j + len < b.len() && a[i + len] == b[j + len] {
                len += 1;
                control.tick()?;
            }
            if runs.len() == MAX_COPY_CANDIDATES_PER_PAIR {
                return Ok((runs, true));
            }
            runs.push((i, j, len));
        }
    }
    Ok((runs, false))
}

struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
        }
    }

    fn find(&mut self, mut x: usize) -> usize {
        while let Some(&p) = self.parent.get(x) {
            if p == x {
                break;
            }
            let grand = self.parent.get(p).copied().unwrap_or(p);
            if let Some(slot) = self.parent.get_mut(x) {
                *slot = grand;
            }
            x = p;
        }
        x
    }

    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        let (low, high) = if ra <= rb { (ra, rb) } else { (rb, ra) };
        if let Some(slot) = self.parent.get_mut(high) {
            *slot = low;
        }
    }
}

/// Build groups in order of first account appearance; returns the grouping and
/// each account's group index.
fn build_grouping<'a>(
    mut sets: UnionFind,
    account_ids: &[&'a OpaqueId],
    counts: &[bool],
    simulated: &[bool],
    edges: &[(usize, DependencyReason<'a>)],
) -> (Grouping<'a>, Vec<usize>) {
    let mut group_of_root: BTreeMap<usize, usize> = BTreeMap::new();
    let mut groups: Vec<IndependenceGroup<'a>> = Vec::new();
    let mut account_group = vec![0usize; account_ids.len()];
    for (account, id) in account_ids.iter().enumerate() {
        let root = sets.find(account);
        let next = group_of_root.len();
        let g = *group_of_root.entry(root).or_insert(next);
        if g == groups.len() {
            groups.push(IndependenceGroup {
                accounts: Vec::new(),
                reasons: Vec::new(),
                counts_as_reported: false,
                includes_simulated: false,
            });
        }
        if let Some(group) = groups.get_mut(g) {
            group.accounts.push(id);
            group.counts_as_reported |= counts[account];
            group.includes_simulated |= simulated[account];
        }
        account_group[account] = g;
    }
    for (account, reason) in edges {
        if let Some(group) = groups.get_mut(account_group[*account]) {
            group.reasons.push(reason.clone());
        }
    }
    let reported_group_count = groups.iter().filter(|g| g.counts_as_reported).count();
    (
        Grouping {
            groups,
            reported_group_count,
        },
        account_group,
    )
}

fn weaker(a: TextCoverage, b: TextCoverage) -> TextCoverage {
    a.max(b)
}

/// Compare a validated, bounded set of pinned source versions. Work is bounded by
/// the input limits and per-pair copy-search budgets; control is checked before
/// and after work, per pair and drift step, and every 1,024 steps of tokenizing,
/// shingle construction, shingle comparison and copy search. A stop returns no
/// result. The caller must fence stale completions before persisting or
/// displaying a result.
pub fn compare_statements<'a>(
    request: &ComparisonRequest<'a>,
    check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<StatementComparison<'a>, ComparisonError> {
    let mut control = Control {
        check: check_control,
        ticks: 0,
    };
    control.now()?;
    validate(request)?;
    let params = request.parameters;
    let sources = request.sources;

    // Accounts in order of first appearance.
    let mut account_ids: Vec<&'a OpaqueId> = Vec::new();
    let mut account_of: HashMap<&'a OpaqueId, usize> = HashMap::new();
    for source in sources {
        account_of.entry(&source.account).or_insert_with(|| {
            account_ids.push(&source.account);
            account_ids.len() - 1
        });
    }
    let n_accounts = account_ids.len();

    let mut prepared = Vec::with_capacity(sources.len());
    let mut limitations = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        let text = source.text.text();
        let tokens = match text {
            Some(text) => tokenize(text, &mut control)?,
            None => Tokens {
                words: Vec::new(),
                longest_word_chars: 0,
                unsegmented_script: false,
            },
        };
        match source.text {
            TextCapture::Partial { .. } => {
                limitations.push(Limitation::TextPartial { source: index })
            }
            TextCapture::Unavailable { .. } => {
                limitations.push(Limitation::TextUnavailable { source: index })
            }
            TextCapture::Complete(_) => {}
        }
        let unsegmented =
            tokens.unsegmented_script || tokens.longest_word_chars > UNSEGMENTED_WORD_CHARS;
        if unsegmented {
            limitations.push(Limitation::UnsegmentedText {
                source: index,
                longest_word_chars: tokens.longest_word_chars,
            });
        }
        if text.is_some() && tokens.words.len() < params.shingle_words {
            limitations.push(Limitation::TooShortForShingles { source: index });
        }
        let account = account_of.get(&source.account).copied().unwrap_or_default();
        prepared.push(Prepared {
            text,
            words: tokens.words,
            account,
            longest_word_chars: tokens.longest_word_chars,
            unsegmented,
        });
    }
    let norms: Vec<Vec<&str>> = prepared
        .iter()
        .map(|p| p.words.iter().map(|w| w.norm.as_str()).collect())
        .collect();
    let mut shingle_sets: Vec<HashSet<&[&str]>> = Vec::with_capacity(norms.len());
    for n in &norms {
        shingle_sets.push(shingles(n, params.shingle_words, &mut control)?);
    }

    // Pairwise matrix and cross-account copy candidates.
    let mut pairs = Vec::new();
    let mut copy_candidates = Vec::new();
    for a in 0..sources.len() {
        for b in a + 1..sources.len() {
            control.now()?;
            let (pa, pb) = (&prepared[a], &prepared[b]);
            let same_account = pa.account == pb.account;
            let coverage = weaker(sources[a].text.coverage(), sources[b].text.coverage());
            let available = pa.text.is_some() && pb.text.is_some();
            let overlap = if available {
                jaccard(&shingle_sets[a], &shingle_sets[b], &mut control)?
            } else {
                OverlapMeasure::Unavailable
            };
            let (copy_search, found) = if same_account {
                (CopySearch::SameAccount, 0)
            } else if !available {
                (CopySearch::Unavailable, 0)
            } else {
                let (runs, truncated) =
                    shared_runs(&norms[a], &norms[b], params.copy_run_words, &mut control)?;
                let found = runs.len();
                for (i, j, len) in runs {
                    let a_range = pa.words[i].range.start..pa.words[i + len - 1].range.end;
                    let b_range = pb.words[j].range.start..pb.words[j + len - 1].range.end;
                    copy_candidates.push(CopyCandidate {
                        a,
                        b,
                        a_text: pa
                            .text
                            .and_then(|t| t.get(a_range.clone()))
                            .unwrap_or_default(),
                        b_text: pb
                            .text
                            .and_then(|t| t.get(b_range.clone()))
                            .unwrap_or_default(),
                        a_range,
                        b_range,
                        words: len,
                        dismissal: None,
                    });
                }
                if truncated {
                    limitations.push(Limitation::CopySearchTruncated { a, b });
                    (CopySearch::Truncated, found)
                } else if pa.unsegmented || pb.unsegmented {
                    (CopySearch::UnsegmentedText, found)
                } else {
                    (CopySearch::Complete, found)
                }
            };
            pairs.push(PairOverlap {
                a,
                b,
                same_account,
                coverage,
                overlap,
                copy_search,
                copy_candidates: found,
            });
        }
    }

    // Reviewer dismissals, matched by version pair and byte ranges in either order.
    for (index, d) in request.dismissals.iter().enumerate() {
        let matched = copy_candidates.iter_mut().find(|c| {
            let (va, vb) = (&sources[c.a].version, &sources[c.b].version);
            (va == &d.a_version
                && vb == &d.b_version
                && c.a_range == d.a_range
                && c.b_range == d.b_range)
                || (va == &d.b_version
                    && vb == &d.a_version
                    && c.a_range == d.b_range
                    && c.b_range == d.a_range)
        });
        match matched {
            Some(candidate) if candidate.dismissal.is_none() => candidate.dismissal = Some(index),
            _ => limitations.push(Limitation::UnmatchedDismissal { dismissal: index }),
        }
    }

    // Account origin and declared lineage.
    let mut simulated = vec![false; n_accounts];
    let mut derivations: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n_accounts];
    let mut declared_edges: Vec<(usize, DependencyReason<'a>)> = Vec::new();
    let mut declared_sets = UnionFind::new(n_accounts);
    for (index, source) in sources.iter().enumerate() {
        let account = prepared[index].account;
        simulated[account] = source.origin == AccountOrigin::Simulated;
        for derived in source.derived_from {
            if let Some(&other) = account_of.get(derived) {
                derivations[account].insert(other);
                declared_sets.union(account, other);
                declared_edges.push((
                    account,
                    DependencyReason::DeclaredDerivation {
                        source: index,
                        derived_from: derived,
                    },
                ));
            }
        }
    }
    // Least fixpoint: an account is anchored only in simulation if it is
    // simulated, or every in-set account it declares it draws on is.
    let mut anchored_only_simulated = simulated.clone();
    for _ in 0..n_accounts {
        let mut changed = false;
        for account in 0..n_accounts {
            if !anchored_only_simulated[account]
                && !derivations[account].is_empty()
                && derivations[account]
                    .iter()
                    .all(|&d| anchored_only_simulated[d])
            {
                anchored_only_simulated[account] = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let counts: Vec<bool> = anchored_only_simulated.iter().map(|&s| !s).collect();

    let mut candidate_sets = UnionFind::new(n_accounts);
    let mut candidate_edges = declared_edges.clone();
    for (account, derived) in derivations.iter().enumerate() {
        for &other in derived {
            candidate_sets.union(account, other);
        }
    }
    for (candidate, c) in copy_candidates.iter().enumerate() {
        if c.dismissal.is_none() {
            let (x, y) = (prepared[c.a].account, prepared[c.b].account);
            candidate_sets.union(x, y);
            candidate_edges.push((x, DependencyReason::SharedWording { candidate }));
        }
    }
    let (declared, declared_group) = build_grouping(
        declared_sets,
        &account_ids,
        &counts,
        &simulated,
        &declared_edges,
    );
    let (candidate_adjusted, candidate_group) = build_grouping(
        candidate_sets,
        &account_ids,
        &counts,
        &simulated,
        &candidate_edges,
    );

    // Representative version per account.
    let mut representative: Vec<Option<usize>> = vec![None; n_accounts];
    for (index, source) in sources.iter().enumerate() {
        let slot = &mut representative[prepared[index].account];
        let better = match *slot {
            None => true,
            Some(current) => match params.representative {
                Representative::Initial => source.revision < sources[current].revision,
                Representative::Latest => source.revision > sources[current].revision,
            },
        };
        if better {
            *slot = Some(index);
        }
    }
    let representatives: BTreeSet<usize> = representative.iter().flatten().copied().collect();

    // Aspect table over representative versions, sorted by aspect key.
    let mut by_aspect: BTreeMap<&'a str, Vec<AspectReport<'a>>> = BTreeMap::new();
    for &index in &representatives {
        let source = &sources[index];
        for assertion in source.assertions {
            control.tick()?;
            let wording_text = source
                .text
                .text()
                .and_then(|t| t.get(assertion.wording.clone()))
                .unwrap_or_default();
            by_aspect
                .entry(assertion.aspect)
                .or_default()
                .push(AspectReport {
                    source: index,
                    value: assertion.value,
                    wording: assertion.wording.clone(),
                    wording_text,
                    origin: source.origin,
                    exposure: source.exposure,
                });
        }
    }
    let mut aspects = Vec::with_capacity(by_aspect.len());
    for (aspect, reports) in by_aspect {
        let mut variant_sources: BTreeMap<&'a str, BTreeSet<usize>> = BTreeMap::new();
        let mut stated_unknown_by = BTreeSet::new();
        let mut coded_in = BTreeSet::new();
        for report in &reports {
            coded_in.insert(report.source);
            match report.value {
                CodedValue::Reported(value) => {
                    variant_sources
                        .entry(value)
                        .or_default()
                        .insert(report.source);
                }
                CodedValue::StatedUnknown => {
                    stated_unknown_by.insert(report.source);
                }
            }
        }
        let variants: Vec<Variant<'a>> = variant_sources
            .into_iter()
            .map(|(value, members)| {
                let accounts: BTreeSet<usize> =
                    members.iter().map(|&s| prepared[s].account).collect();
                let reported: Vec<usize> =
                    accounts.iter().copied().filter(|&a| counts[a]).collect();
                let distinct = |groups: &[usize]| {
                    reported
                        .iter()
                        .map(|&a| groups[a])
                        .collect::<BTreeSet<_>>()
                        .len()
                };
                Variant {
                    value,
                    sources: members.into_iter().collect(),
                    accounts: accounts.len(),
                    reported_accounts: reported.len(),
                    simulated_accounts: accounts.iter().filter(|&&a| simulated[a]).count(),
                    declared_groups: distinct(&declared_group),
                    candidate_adjusted_groups: distinct(&candidate_group),
                }
            })
            .collect();
        let status = match variants.as_slice() {
            [] => AspectStatus::StatedUnknownOnly,
            [only] if only.reported_accounts == 0 => AspectStatus::NoReportedSupport,
            [only] if only.candidate_adjusted_groups >= 2 => AspectStatus::Converging,
            [only] if only.reported_accounts >= 2 => AspectStatus::RepeatedWithinOneGroup,
            [_] => AspectStatus::Unique,
            _ => AspectStatus::Conflicting,
        };
        let not_coded_in = representatives
            .iter()
            .filter(|s| !coded_in.contains(s))
            .map(|&s| Omission {
                source: s,
                coverage: sources[s].text.coverage(),
            })
            .collect();
        aspects.push(AspectComparison {
            aspect,
            status,
            variants,
            stated_unknown_by: stated_unknown_by.into_iter().collect(),
            reports,
            not_coded_in,
        });
    }

    // Drift between consecutive revisions of each account.
    let mut drift = Vec::new();
    for (account, id) in account_ids.iter().enumerate() {
        let mut versions: Vec<usize> = (0..sources.len())
            .filter(|&s| prepared[s].account == account)
            .collect();
        versions.sort_by_key(|&s| sources[s].revision);
        for step in versions.windows(2) {
            let &[from, to] = step else { continue };
            control.now()?;
            let before = coded_values(&sources[from]);
            let after = coded_values(&sources[to]);
            let keys: BTreeSet<&'a str> = before.keys().chain(after.keys()).copied().collect();
            let mut changes = Vec::new();
            let mut unchanged = 0;
            for key in keys {
                let b = before.get(key).cloned().unwrap_or_default();
                let a = after.get(key).cloned().unwrap_or_default();
                if a == b {
                    unchanged += 1;
                } else {
                    changes.push(AspectChange {
                        aspect: key,
                        before: b.into_iter().collect(),
                        after: a.into_iter().collect(),
                    });
                }
            }
            let (low, high) = (from.min(to), from.max(to));
            let overlap = pairs
                .iter()
                .find(|p| p.a == low && p.b == high)
                .map_or(OverlapMeasure::Unavailable, |p| p.overlap);
            let text_identical = match (prepared[from].text, prepared[to].text) {
                (Some(x), Some(y)) => Some(x == y),
                _ => None,
            };
            drift.push(DriftStep {
                account: id,
                from,
                to,
                to_exposure: sources[to].exposure,
                text_identical,
                overlap,
                changes,
                unchanged_aspects: unchanged,
            });
        }
    }

    let summaries = sources
        .iter()
        .enumerate()
        .map(|(index, source)| {
            let account = prepared[index].account;
            SourceSummary {
                index,
                account: &source.account,
                version: &source.version,
                revision: source.revision,
                class: source.class,
                origin: source.origin,
                exposure: source.exposure,
                attribution: source.attribution,
                coverage: source.text.coverage(),
                word_count: prepared[index].words.len(),
                longest_word_chars: prepared[index].longest_word_chars,
                assertion_count: source.assertions.len(),
                representative: representatives.contains(&index),
                counts_as_reported: counts[account],
                declared_group: declared_group[account],
                candidate_adjusted_group: candidate_group[account],
            }
        })
        .collect();
    control.now()?;
    Ok(StatementComparison {
        method: MethodRecord {
            id: METHOD_ID,
            tokenization: TOKENIZATION,
            parameters: params,
            coding_version: request.coding_version.clone(),
            coded_by: request.coded_by.clone(),
            overlap_caveat: OVERLAP_CAVEAT,
            declared_groups_caveat: DECLARED_GROUPS_CAVEAT,
            candidate_groups_caveat: CANDIDATE_GROUPS_CAVEAT,
            simulated_caveat: SIMULATED_CAVEAT,
            coding_caveat: CODING_CAVEAT,
        },
        sources: summaries,
        account_count: n_accounts,
        reported_account_count: counts.iter().filter(|&&c| c).count(),
        declared,
        candidate_adjusted,
        pairs,
        copy_candidates,
        aspects,
        drift,
        limitations,
        contains_simulated: simulated.iter().any(|&s| s),
    })
}

fn coded_values<'a>(source: &StatementSource<'a>) -> BTreeMap<&'a str, BTreeSet<CodedValue<'a>>> {
    let mut map: BTreeMap<&'a str, BTreeSet<CodedValue<'a>>> = BTreeMap::new();
    for assertion in source.assertions {
        map.entry(assertion.aspect)
            .or_default()
            .insert(assertion.value);
    }
    map
}
