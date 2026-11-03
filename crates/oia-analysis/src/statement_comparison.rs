//! Bounded comparison of accounts of one incident (#138; `INTEL-01`, `REASON-03`,
//! `FIELD-02`).
//!
//! Callers supply pinned source versions (witness statements, police reports,
//! news reports, revisions of one account) with analyst-coded detail assertions
//! that cite exact wording by byte range. The kernel reports shared, unique,
//! conflicting and uncoded aspects; a pairwise lexical overlap matrix; drift
//! between revisions of one account; and long exact shared word runs that suggest
//! copying or quotation. Shared wording merges accounts into one independence
//! group, so a copied account lowers the independent-source count instead of
//! raising corroboration.
//!
//! Inputs and results are transient analysis: no I/O, persistence, extraction,
//! model inference or normalization of the source text. Overlap is lexical. It is
//! never agreement about events, a probability, or proof of independence.

use oia_domain::text::is_format_control;
use oia_domain::witness::AccountOrigin;
use oia_domain::{ActorId, OpaqueId, VersionId};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;
use std::ops::Range;

/// Pin this identifier and the parameters in [`MethodRecord`] with any persisted result.
pub const METHOD_ID: &str = "word-shingle-jaccard+exact-word-run-v1";
/// How text is split into the words that both measures compare.
pub const TOKENIZATION: &str = "maximal runs of Unicode alphanumeric characters, compared after \
     char::to_lowercase; every other character separates words";
pub const OVERLAP_CAVEAT: &str = "Overlap measures shared wording in the captured text. It is not \
     agreement about what happened, not evidence of independence, and not a probability.";
pub const INDEPENDENCE_CAVEAT: &str = "Independence groups are an upper bound. Paraphrase, shared \
     briefings, interviews or a common outside source can make accounts dependent without \
     shared wording.";
pub const CODING_CAVEAT: &str = "Aspects and coded values are analyst coding over cited wording, \
     not observations or measurements. An uncoded aspect is not an observed absence.";

/// Hard bounds. Oversized input is refused, never truncated.
pub const MAX_SOURCES: usize = 32;
pub const MAX_TEXT_BYTES: usize = 262_144;
pub const MAX_TOTAL_TEXT_BYTES: usize = 2_097_152;
pub const MAX_ASSERTIONS_PER_SOURCE: usize = 256;
pub const MAX_DERIVATIONS_PER_SOURCE: usize = 32;
pub const MAX_LABEL_BYTES: usize = 256;
pub const MAX_DESCRIPTION_BYTES: usize = 4_096;
pub const SHINGLE_WORDS_RANGE: Range<usize> = 2..17;
pub const COPY_RUN_WORDS_RANGE: Range<usize> = 6..257;
/// Per cross-account pair; reaching either bound marks that pair's search truncated.
pub const MAX_COPY_CANDIDATES_PER_PAIR: usize = 64;
pub const MAX_COPY_PROBES_PER_PAIR: usize = 4_000_000;
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

/// One analyst-coded detail citing a non-empty UTF-8 byte range of the source text.
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

/// Which version represents each account in the aspect table.
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
            representative: Representative::Latest,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    Cancelled,
    DeadlineExceeded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComparisonError {
    TooFewSources(usize),
    TooManySources(usize),
    InvalidParameter(&'static str),
    DuplicateVersion { source: usize },
    DuplicateRevision { source: usize },
    InconsistentAccount { source: usize, field: &'static str },
    EmptyField { source: usize, field: &'static str },
    FieldTooLong { source: usize, field: &'static str },
    InvalidCharacter { source: usize, field: &'static str },
    TextTooLarge { source: usize, bytes: usize },
    TotalTextTooLarge { bytes: usize },
    TooManyAssertions { source: usize, count: usize },
    TooManyDerivations { source: usize, count: usize },
    SelfDerivation { source: usize },
    WordingWithoutText { source: usize, assertion: usize },
    InvalidWording { source: usize, assertion: usize },
    Stopped(StopReason),
}

impl fmt::Display for ComparisonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooFewSources(n) => write!(f, "{n} source versions; at least 2 are required"),
            Self::TooManySources(n) => write!(f, "{n} source versions; limit is {MAX_SOURCES}"),
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
                "source {source} assertion {assertion} cites an empty, out-of-range or \
                 non-character-boundary byte range"
            ),
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
    pub independence_caveat: &'static str,
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
    pub assertion_count: usize,
    /// Whether this version represents its account in the aspect table.
    pub representative: bool,
    /// Index into [`StatementComparison::groups`].
    pub independence_group: usize,
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
    /// Neither text has enough words for one shingle.
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AspectReport<'a> {
    pub source: usize,
    pub value: CodedValue<'a>,
    pub wording: Range<usize>,
    pub wording_text: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant<'a> {
    pub value: &'a str,
    pub sources: Vec<usize>,
    pub accounts: usize,
    pub independence_groups: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AspectStatus {
    /// One coded value, reported from at least two independence groups.
    Converging,
    /// One coded value, repeated only by accounts in one dependent group.
    RepeatedWithinOneGroup,
    /// One coded value from one account.
    Unique,
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
    TextPartial { source: usize },
    TextUnavailable { source: usize },
    TooShortForShingles { source: usize },
    CopySearchTruncated { a: usize, b: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatementComparison<'a> {
    pub method: MethodRecord,
    pub sources: Vec<SourceSummary<'a>>,
    pub account_count: usize,
    /// Equals `groups.len()`; an upper bound (see [`INDEPENDENCE_CAVEAT`]).
    pub independence_group_upper_bound: usize,
    pub groups: Vec<IndependenceGroup<'a>>,
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

fn validate_text_field(
    source: usize,
    field: &'static str,
    value: &str,
    limit: usize,
    structural: bool,
) -> Result<(), ComparisonError> {
    if value.len() > limit {
        return Err(ComparisonError::FieldTooLong { source, field });
    }
    if value.trim().is_empty() {
        return Err(ComparisonError::EmptyField { source, field });
    }
    if structural
        && value
            .chars()
            .any(|c| c.is_control() || is_format_control(c))
    {
        return Err(ComparisonError::InvalidCharacter { source, field });
    }
    Ok(())
}

fn validate(request: &ComparisonRequest<'_>) -> Result<(), ComparisonError> {
    let sources = request.sources;
    if sources.len() < 2 {
        return Err(ComparisonError::TooFewSources(sources.len()));
    }
    if sources.len() > MAX_SOURCES {
        return Err(ComparisonError::TooManySources(sources.len()));
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
            let w = &assertion.wording;
            if w.start >= w.end
                || w.end > text.len()
                || !text.is_char_boundary(w.start)
                || !text.is_char_boundary(w.end)
            {
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
    Ok(())
}

fn tokenize<F: FnMut() -> Result<(), StopReason>>(
    text: &str,
    control: &mut Control<F>,
) -> Result<Vec<Word>, ComparisonError> {
    let mut words = Vec::new();
    let mut start: Option<usize> = None;
    for (offset, ch) in text.char_indices() {
        control.tick()?;
        match (ch.is_alphanumeric(), start) {
            (true, None) => start = Some(offset),
            (false, Some(begin)) => {
                words.push(word(text, begin..offset));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(begin) = start {
        words.push(word(text, begin..text.len()));
    }
    Ok(words)
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

fn shingles<'w>(norms: &'w [&'w str], k: usize) -> HashSet<&'w [&'w str]> {
    if norms.len() < k {
        return HashSet::new();
    }
    norms.windows(k).collect()
}

fn jaccard<F: FnMut() -> Result<(), StopReason>>(
    a: &HashSet<&[&str]>,
    b: &HashSet<&[&str]>,
    control: &mut Control<F>,
) -> Result<OverlapMeasure, ComparisonError> {
    let (small, large) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    let mut shared = 0u64;
    for shingle in small {
        control.tick()?;
        if large.contains(shingle) {
            shared += 1;
        }
    }
    let union = (a.len() + b.len()) as u64 - shared;
    Ok(if union == 0 {
        OverlapMeasure::TooShort
    } else {
        OverlapMeasure::ShingleJaccard(Ratio {
            numerator: shared,
            denominator: union,
        })
    })
}

/// Maximal exact runs of at least `m` words. A run starting where the previous
/// words also match is a suffix of a longer run and is skipped. Probes and
/// candidates are bounded per pair; the bool reports truncation.
/// `(start in a, start in b, words)` per run, and whether the search was truncated.
type SharedRuns = (Vec<(usize, usize, usize)>, bool);

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

struct Groups {
    parent: Vec<usize>,
}

impl Groups {
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

fn weaker(a: TextCoverage, b: TextCoverage) -> TextCoverage {
    a.max(b)
}

/// Compare a validated, bounded set of pinned source versions. Work is bounded by
/// the input limits and per-pair copy-search budgets; control is checked before
/// and after work and every 1,024 steps. A stop returns no result. The caller
/// must fence stale completions before persisting or displaying a result.
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

    let mut prepared = Vec::with_capacity(sources.len());
    let mut limitations = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        let text = source.text.text();
        let words = match text {
            Some(text) => tokenize(text, &mut control)?,
            None => Vec::new(),
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
        if text.is_some() && words.len() < params.shingle_words {
            limitations.push(Limitation::TooShortForShingles { source: index });
        }
        let account = account_of.get(&source.account).copied().unwrap_or_default();
        prepared.push(Prepared {
            text,
            words,
            account,
        });
    }
    let norms: Vec<Vec<&str>> = prepared
        .iter()
        .map(|p| p.words.iter().map(|w| w.norm.as_str()).collect())
        .collect();
    let shingle_sets: Vec<HashSet<&[&str]>> = norms
        .iter()
        .map(|n| shingles(n, params.shingle_words))
        .collect();

    // Pairwise matrix and cross-account copy candidates.
    let mut groups = Groups {
        parent: (0..account_ids.len()).collect(),
    };
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
                    });
                }
                if truncated {
                    limitations.push(Limitation::CopySearchTruncated { a, b });
                    (CopySearch::Truncated, found)
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

    // Independence: declared derivations and shared wording merge accounts.
    let mut edges: Vec<(usize, DependencyReason<'a>)> = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        for derived in source.derived_from {
            if let Some(&other) = account_of.get(derived) {
                groups.union(prepared[index].account, other);
                edges.push((
                    prepared[index].account,
                    DependencyReason::DeclaredDerivation {
                        source: index,
                        derived_from: derived,
                    },
                ));
            }
        }
    }
    for (candidate, c) in copy_candidates.iter().enumerate() {
        groups.union(prepared[c.a].account, prepared[c.b].account);
        edges.push((
            prepared[c.a].account,
            DependencyReason::SharedWording { candidate },
        ));
    }
    let mut group_of_root: BTreeMap<usize, usize> = BTreeMap::new();
    let mut group_list: Vec<IndependenceGroup<'a>> = Vec::new();
    let mut account_group = vec![0usize; account_ids.len()];
    for (account, id) in account_ids.iter().enumerate() {
        let root = groups.find(account);
        let next = group_of_root.len();
        let g = *group_of_root.entry(root).or_insert(next);
        if g == group_list.len() {
            group_list.push(IndependenceGroup {
                accounts: Vec::new(),
                reasons: Vec::new(),
            });
        }
        if let Some(group) = group_list.get_mut(g) {
            group.accounts.push(*id);
        }
        account_group[account] = g;
    }
    for (account, reason) in edges {
        if let Some(group) = group_list.get_mut(account_group[account]) {
            group.reasons.push(reason);
        }
    }

    // Representative version per account.
    let mut representative: Vec<Option<usize>> = vec![None; account_ids.len()];
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
                let independence_groups = accounts
                    .iter()
                    .map(|&acc| account_group[acc])
                    .collect::<BTreeSet<_>>()
                    .len();
                Variant {
                    value,
                    sources: members.into_iter().collect(),
                    accounts: accounts.len(),
                    independence_groups,
                }
            })
            .collect();
        let status = match variants.as_slice() {
            [] => AspectStatus::StatedUnknownOnly,
            [only] if only.accounts == 1 => AspectStatus::Unique,
            [only] if only.independence_groups >= 2 => AspectStatus::Converging,
            [_] => AspectStatus::RepeatedWithinOneGroup,
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
        .map(|(index, source)| SourceSummary {
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
            assertion_count: source.assertions.len(),
            representative: representatives.contains(&index),
            independence_group: account_group[prepared[index].account],
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
            independence_caveat: INDEPENDENCE_CAVEAT,
            coding_caveat: CODING_CAVEAT,
        },
        sources: summaries,
        account_count: account_ids.len(),
        independence_group_upper_bound: group_list.len(),
        groups: group_list,
        pairs,
        copy_candidates,
        aspects,
        drift,
        limitations,
        contains_simulated: sources.iter().any(|s| s.origin == AccountOrigin::Simulated),
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
