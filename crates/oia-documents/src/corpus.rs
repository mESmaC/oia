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
    Pages,
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
    PageLimitReached { maximum: usize },
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
            Self::PageLimitReached { maximum } => write!(
                f,
                "corpus page limit {maximum} reached; search remains unfinished"
            ),
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

/// One page contains only inputs visited during this call. A resumed input's
/// search coverage is cumulative, while its hits belong only to this page.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorpusPage<'input> {
    documents: Vec<DocumentResult<'input>>,
    match_count: usize,
    next_input_index: usize,
    next_byte: usize,
    exhausted: bool,
    complete: bool,
}

impl<'input> CorpusPage<'input> {
    pub fn documents(&self) -> &[DocumentResult<'input>] {
        &self.documents
    }
    pub fn match_count(&self) -> usize {
        self.match_count
    }
    /// All later slots remain unvisited. This slot may have a searched prefix.
    /// Equals the input count when the session is exhausted.
    pub fn next_input_index(&self) -> usize {
        self.next_input_index
    }
    pub fn next_byte(&self) -> usize {
        self.next_byte
    }
    /// All available extracted text was visited, even if extraction has gaps.
    pub fn is_exhausted(&self) -> bool {
        self.exhausted
    }
    /// Cumulative: exhausted with no unavailable inputs or extraction gaps.
    pub fn is_complete(&self) -> bool {
        self.complete
    }
}

/// An in-memory cursor bound to immutable borrowed selection, text and query.
/// Admission bounds the entire selection. Hit limits apply per page and the
/// positive page cap bounds total successful calls. No cursor serialization,
/// source identity, ranking, I/O or authorization is implemented.
#[derive(Debug)]
pub struct CorpusSession<'session, 'input> {
    inputs: &'session [CorpusInput<'session, 'input>],
    query: &'session str,
    limits: CorpusLimits,
    mode: SearchMode,
    max_pages: usize,
    pages_returned: usize,
    next_input_index: usize,
    next_byte: usize,
    has_gaps: bool,
    exhausted: bool,
}

impl<'session, 'input> CorpusSession<'session, 'input> {
    pub fn new(
        inputs: &'session [CorpusInput<'session, 'input>],
        query: &'session str,
        limits: CorpusLimits,
        mode: SearchMode,
        max_pages: usize,
        mut check_control: impl FnMut() -> Result<(), StopReason>,
    ) -> Result<Self, CorpusError> {
        check_control().map_err(SearchError::Stopped)?;
        crate::search::validate_query(query, limits.search, mode)?;
        if max_pages == 0 {
            return Err(CorpusError::ZeroLimit(CorpusLimit::Pages));
        }
        if inputs.len() > limits.max_documents {
            return Err(CorpusError::LimitExceeded {
                limit: CorpusLimit::Documents,
                maximum: limits.max_documents,
            });
        }
        let mut total = 0_usize;
        for input in inputs {
            check_control().map_err(SearchError::Stopped)?;
            if let CorpusInput::Text(extraction) = input {
                total = total
                    .checked_add(extraction.coverage().input_bytes())
                    .filter(|sum| *sum <= limits.max_input_bytes)
                    .ok_or(CorpusError::LimitExceeded {
                        limit: CorpusLimit::InputBytes,
                        maximum: limits.max_input_bytes,
                    })?;
            }
        }
        check_control().map_err(SearchError::Stopped)?;
        Ok(Self {
            inputs,
            query,
            limits,
            mode,
            max_pages,
            pages_returned: 0,
            next_input_index: 0,
            next_byte: 0,
            has_gaps: false,
            exhausted: false,
        })
    }

    pub fn pages_returned(&self) -> usize {
        self.pages_returned
    }

    /// Failure leaves every cursor field and the successful-page count intact.
    /// None means exhausted available text; the last page retains any gaps.
    /// A full page may require an additional empty page to establish exhaustion.
    pub fn next_page(
        &mut self,
        mut check_control: impl FnMut() -> Result<(), StopReason>,
    ) -> Result<Option<CorpusPage<'input>>, CorpusError> {
        check_control().map_err(SearchError::Stopped)?;
        if self.exhausted {
            return Ok(None);
        }
        if self.pages_returned == self.max_pages {
            return Err(CorpusError::PageLimitReached {
                maximum: self.max_pages,
            });
        }
        let mut index = self.next_input_index;
        let mut offset = self.next_byte;
        let mut has_gaps = self.has_gaps;
        let mut documents = Vec::new();
        let mut match_count = 0;
        while let Some(input) = self.inputs.get(index) {
            check_control().map_err(SearchError::Stopped)?;
            if match_count == self.limits.search.max_matches() {
                break;
            }
            let input_index = index;
            let outcome = match input {
                CorpusInput::Unavailable(reason) => {
                    has_gaps = true;
                    index += 1;
                    DocumentOutcome::Unavailable(*reason)
                }
                CorpusInput::Text(extraction) => {
                    let search_limits = SearchLimits::new(
                        self.limits.search.max_query_bytes(),
                        self.limits.search.max_matches() - match_count,
                    )?;
                    let result = crate::search::search_from(
                        extraction,
                        self.query,
                        search_limits,
                        self.mode,
                        offset,
                        &mut check_control,
                    )?;
                    offset = result.coverage().searched_bytes();
                    match_count += result.matches().len();
                    has_gaps |= extraction.coverage().unprocessed_range().is_some();
                    if offset == extraction.coverage().extracted_bytes() {
                        index += 1;
                        offset = 0;
                    }
                    DocumentOutcome::Searched(result)
                }
            };
            documents
                .try_reserve(1)
                .map_err(|_| CorpusError::AllocationFailed)?;
            documents.push(DocumentResult {
                input_index,
                outcome,
            });
        }
        let exhausted = index == self.inputs.len();
        check_control().map_err(SearchError::Stopped)?;
        let page = CorpusPage {
            documents,
            match_count,
            next_input_index: index,
            next_byte: offset,
            exhausted,
            complete: exhausted && !has_gaps,
        };
        self.next_input_index = index;
        self.next_byte = offset;
        self.has_gaps = has_gaps;
        self.exhausted = exhausted;
        self.pages_returned += 1;
        Ok(Some(page))
    }
}
