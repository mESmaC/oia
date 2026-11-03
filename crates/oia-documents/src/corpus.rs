//! Bounded matching over a caller-selected, already-authorized input list.
//! Input indices identify slots, not source versions or independent evidence.
//! No I/O, source filtering, identity resolution or authorization occurs here.

use std::error::Error;
use std::fmt;

use crate::search::{SearchError, SearchLimits, SearchMode, SearchResult, find_text, find_token};
use crate::{Coverage, Extraction, StopReason};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CorpusLimit {
    Documents,
    InputBytes,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CorpusLimits {
    max_documents: usize,
    max_input_bytes: usize,
    search: SearchLimits,
}

impl CorpusLimits {
    /// `search.max_matches()` is shared across the entire input list.
    /// Bytes count each supplied extraction's original buffer, including its
    /// unextracted suffix. Duplicate buffers count separately, as does the work.
    pub fn new(
        max_documents: usize,
        max_input_bytes: usize,
        search: SearchLimits,
    ) -> Result<Self, CorpusError> {
        if max_documents == 0 {
            return Err(CorpusError::ZeroLimit(CorpusLimit::Documents));
        }
        if max_input_bytes == 0 {
            return Err(CorpusError::ZeroLimit(CorpusLimit::InputBytes));
        }
        Ok(Self {
            max_documents,
            max_input_bytes,
            search,
        })
    }

    pub fn max_documents(self) -> usize {
        self.max_documents
    }

    pub fn max_input_bytes(self) -> usize {
        self.max_input_bytes
    }

    pub fn search(self) -> SearchLimits {
        self.search
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CorpusError {
    ZeroLimit(CorpusLimit),
    LimitExceeded { limit: CorpusLimit, maximum: usize },
    AllocationFailed,
    Search(SearchError),
}

impl fmt::Display for CorpusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroLimit(limit) => write!(f, "corpus {limit:?} limit must be greater than zero"),
            Self::LimitExceeded { limit, maximum } => {
                write!(
                    f,
                    "corpus exceeds {limit:?} limit of {maximum}; no result returned"
                )
            }
            Self::AllocationFailed => write!(f, "unable to allocate corpus search metadata"),
            Self::Search(error) => write!(f, "{error}"),
        }
    }
}

impl Error for CorpusError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Search(error) => Some(error),
            _ => None,
        }
    }
}

impl From<SearchError> for CorpusError {
    fn from(error: SearchError) -> Self {
        Self::Search(error)
    }
}

/// Caller-declared reason why this in-scope input cannot be searched as text.
/// A missing input is never treated as an empty, successfully searched buffer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnavailableReason {
    OcrRequired,
    Unreadable,
    Missing,
    ReferenceOnly,
}

#[derive(Clone, Copy, Debug)]
pub enum CorpusInput<'selection, 'input> {
    Text(&'selection Extraction<'input>),
    Unavailable(UnavailableReason),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DocumentOutcome<'input> {
    /// May still contain search or extraction gaps; inspect its coverage.
    Searched(SearchResult<'input>),
    /// The shared hit limit left this input wholly unsearched. Its extraction
    /// coverage remains separate from the absence of search work.
    MatchLimitReached(Coverage),
    Unavailable(UnavailableReason),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentResult<'input> {
    input_index: usize,
    outcome: DocumentOutcome<'input>,
}

impl<'input> DocumentResult<'input> {
    /// Zero-based slot in the exact list supplied to `search_corpus`.
    pub fn input_index(&self) -> usize {
        self.input_index
    }

    pub fn outcome(&self) -> &DocumentOutcome<'input> {
        &self.outcome
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorpusResult<'input> {
    documents: Vec<DocumentResult<'input>>,
    match_count: usize,
    known_input_bytes: usize,
    limits: CorpusLimits,
    mode: SearchMode,
}

impl<'input> CorpusResult<'input> {
    /// One entry per input, in input order, including unavailable/skipped input.
    pub fn documents(&self) -> &[DocumentResult<'input>] {
        &self.documents
    }

    pub fn match_count(&self) -> usize {
        self.match_count
    }

    /// Sum of supplied original buffer lengths; unavailable inputs have unknown
    /// sizes and are not represented as zero-byte observed documents.
    pub fn known_input_bytes(&self) -> usize {
        self.known_input_bytes
    }

    pub fn limits(&self) -> CorpusLimits {
        self.limits
    }

    pub fn mode(&self) -> SearchMode {
        self.mode
    }

    /// Complete only for this supplied list and matching mode. An empty list
    /// is vacuously complete; no source-wide or real-world absence is inferred.
    pub fn is_complete(&self) -> bool {
        self.documents.iter().all(|document| {
            matches!(&document.outcome, DocumentOutcome::Searched(result) if result.coverage().is_complete())
        })
    }
}

/// Search in input order, then original byte order, sharing one hit limit.
/// Document/total-byte overflow rejects the request rather than silently
/// selecting a smaller corpus. Query validation also runs on an empty corpus.
/// Stops discard the entire result, including stops at the final boundary.
/// Control is cooperative: an individual substring scan is not preemptible.
pub fn search_corpus<'input>(
    inputs: &[CorpusInput<'_, 'input>],
    query: &str,
    limits: CorpusLimits,
    mode: SearchMode,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<CorpusResult<'input>, CorpusError> {
    check_control().map_err(SearchError::Stopped)?;
    crate::search::validate_query(query, limits.search, mode)?;
    if inputs.len() > limits.max_documents {
        return Err(CorpusError::LimitExceeded {
            limit: CorpusLimit::Documents,
            maximum: limits.max_documents,
        });
    }
    let mut known_input_bytes = 0_usize;
    for input in inputs {
        check_control().map_err(SearchError::Stopped)?;
        if let CorpusInput::Text(extraction) = input {
            known_input_bytes = known_input_bytes
                .checked_add(extraction.coverage().input_bytes())
                .filter(|total| *total <= limits.max_input_bytes)
                .ok_or(CorpusError::LimitExceeded {
                    limit: CorpusLimit::InputBytes,
                    maximum: limits.max_input_bytes,
                })?;
        }
    }
    let mut documents = Vec::new();
    documents
        .try_reserve_exact(inputs.len())
        .map_err(|_| CorpusError::AllocationFailed)?;
    let mut match_count = 0;
    for (input_index, input) in inputs.iter().enumerate() {
        check_control().map_err(SearchError::Stopped)?;
        let outcome = match input {
            CorpusInput::Unavailable(reason) => DocumentOutcome::Unavailable(*reason),
            CorpusInput::Text(extraction) => {
                let remaining = limits.search.max_matches() - match_count;
                if remaining == 0 && extraction.coverage().input_bytes() != 0 {
                    DocumentOutcome::MatchLimitReached(extraction.coverage().clone())
                } else {
                    // An empty buffer can still be completely searched after
                    // the hit limit, without scanning text or emitting a hit.
                    let per_document =
                        SearchLimits::new(limits.search.max_query_bytes(), remaining.max(1))?;
                    let result = match mode {
                        SearchMode::ExactSubstring => {
                            find_text(extraction, query, per_document, &mut check_control)?
                        }
                        SearchMode::WhitespaceToken => {
                            find_token(extraction, query, per_document, &mut check_control)?
                        }
                    };
                    match_count += result.matches().len();
                    DocumentOutcome::Searched(result)
                }
            }
        };
        documents.push(DocumentResult {
            input_index,
            outcome,
        });
    }
    check_control().map_err(SearchError::Stopped)?;
    Ok(CorpusResult {
        documents,
        match_count,
        known_input_bytes,
        limits,
        mode,
    })
}
