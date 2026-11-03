//! Bounded, case-sensitive, non-overlapping substring matching within passages.
//! Offsets refer to the original buffer; the caller must bind source identities
//! and authorization before treating a match as a citation.

use std::error::Error;
use std::fmt;
use std::ops::Range;

use crate::{Coverage, Extraction, StopReason};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchLimit {
    QueryBytes,
    Matches,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SearchLimits {
    max_query_bytes: usize,
    max_matches: usize,
}

impl SearchLimits {
    pub fn new(max_query_bytes: usize, max_matches: usize) -> Result<Self, SearchError> {
        if max_query_bytes == 0 {
            return Err(SearchError::ZeroLimit(SearchLimit::QueryBytes));
        }
        if max_matches == 0 {
            return Err(SearchError::ZeroLimit(SearchLimit::Matches));
        }
        Ok(Self {
            max_query_bytes,
            max_matches,
        })
    }

    pub fn max_query_bytes(self) -> usize {
        self.max_query_bytes
    }

    pub fn max_matches(self) -> usize {
        self.max_matches
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SearchError {
    ZeroLimit(SearchLimit),
    EmptyQuery,
    QueryTooLarge {
        actual_bytes: usize,
        limit_bytes: usize,
    },
    MultilineQuery,
    WhitespaceInToken,
    InvalidCursor,
    AllocationFailed,
    Stopped(StopReason),
}

impl fmt::Display for SearchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroLimit(limit) => {
                write!(formatter, "{limit:?} limit must be greater than zero")
            }
            Self::EmptyQuery => write!(formatter, "search query must not be empty"),
            Self::QueryTooLarge {
                actual_bytes,
                limit_bytes,
            } => write!(
                formatter,
                "query has {actual_bytes} bytes; configured limit is {limit_bytes} bytes"
            ),
            Self::MultilineQuery => write!(
                formatter,
                "passage search does not support queries containing LF"
            ),
            Self::AllocationFailed => write!(formatter, "unable to allocate search match metadata"),
            Self::WhitespaceInToken => {
                write!(formatter, "token query must not contain Unicode whitespace")
            }
            Self::InvalidCursor => write!(formatter, "search cursor is not a valid text boundary"),
            Self::Stopped(reason) => write!(formatter, "search stopped: {reason:?}"),
        }
    }
}

impl Error for SearchError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextMatch<'input> {
    byte_range: Range<usize>,
    line_number: usize,
    text: &'input str,
}

impl<'input> TextMatch<'input> {
    pub fn byte_range(&self) -> Range<usize> {
        self.byte_range.clone()
    }

    pub fn line_number(&self) -> usize {
        self.line_number
    }

    pub fn text(&self) -> &'input str {
        self.text
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchCoverage {
    extraction: Coverage,
    searched_bytes: usize,
}

impl SearchCoverage {
    /// Original extraction coverage, including bytes unavailable to this search.
    pub fn extraction(&self) -> &Coverage {
        &self.extraction
    }

    /// Prefix searched under the non-overlapping, within-passage match rule.
    pub fn searched_bytes(&self) -> usize {
        self.searched_bytes
    }

    /// Extracted text not searched because the match budget stopped the scan.
    pub fn unsearched_extracted_range(&self) -> Option<Range<usize>> {
        (self.searched_bytes < self.extraction.extracted_bytes())
            .then_some(self.searched_bytes..self.extraction.extracted_bytes())
    }

    /// Complete only for the supplied UTF-8 buffer, never for a source/corpus.
    pub fn is_complete(&self) -> bool {
        self.searched_bytes == self.extraction.input_bytes()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchResult<'input> {
    matches: Vec<TextMatch<'input>>,
    coverage: SearchCoverage,
    limits: SearchLimits,
    mode: SearchMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchMode {
    ExactSubstring,
    /// A whole token bounded by Unicode whitespace or a passage edge.
    WhitespaceToken,
}

impl<'input> SearchResult<'input> {
    pub fn mode(&self) -> SearchMode {
        self.mode
    }

    pub fn matches(&self) -> &[TextMatch<'input>] {
        &self.matches
    }

    pub fn coverage(&self) -> &SearchCoverage {
        &self.coverage
    }

    pub fn limits(&self) -> SearchLimits {
        self.limits
    }
}

/// Search exact UTF-8 substrings in byte order, without case folding,
/// normalization, tokenization, or matches crossing LF passage boundaries.
/// Entry, each passage/match scan, and final return check cooperative control.
/// A single substring scan is bounded by the extraction's input byte budget,
/// but is not preemptible. No partial result is returned after a reported stop.
pub fn find_text<'input>(
    extraction: &Extraction<'input>,
    query: &str,
    limits: SearchLimits,
    check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<SearchResult<'input>, SearchError> {
    search(
        extraction,
        query,
        limits,
        SearchMode::ExactSubstring,
        check_control,
    )
}

/// Match a complete, case-sensitive Unicode-whitespace-delimited token.
/// Punctuation, BOMs, combining marks and zero-width characters remain literal
/// token content. No normalization, word segmentation or case folding occurs.
/// Empty queries and queries containing whitespace are rejected. Limits,
/// original byte anchors and cooperative control follow `find_text`.
pub fn find_token<'input>(
    extraction: &Extraction<'input>,
    query: &str,
    limits: SearchLimits,
    check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<SearchResult<'input>, SearchError> {
    search(
        extraction,
        query,
        limits,
        SearchMode::WhitespaceToken,
        check_control,
    )
}

fn search<'input>(
    extraction: &Extraction<'input>,
    query: &str,
    limits: SearchLimits,
    mode: SearchMode,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<SearchResult<'input>, SearchError> {
    check_control().map_err(SearchError::Stopped)?;
    validate_query(query, limits, mode)?;
    search_from(extraction, query, limits, mode, 0, check_control)
}

pub(crate) fn validate_query(
    query: &str,
    limits: SearchLimits,
    mode: SearchMode,
) -> Result<(), SearchError> {
    if query.is_empty() {
        return Err(SearchError::EmptyQuery);
    }
    if query.len() > limits.max_query_bytes {
        return Err(SearchError::QueryTooLarge {
            actual_bytes: query.len(),
            limit_bytes: limits.max_query_bytes,
        });
    }
    if query.contains('\n') {
        return Err(SearchError::MultilineQuery);
    }
    if mode == SearchMode::WhitespaceToken && query.chars().any(char::is_whitespace) {
        return Err(SearchError::WhitespaceInToken);
    }
    Ok(())
}

/// Resumable search over one immutable extraction and query. The cursor is
/// private and cannot be transferred to a different buffer, query or mode.
/// Limits apply per page; callers must separately bound total pages/work.
#[derive(Debug)]
pub struct SearchSession<'session, 'input> {
    extraction: &'session Extraction<'input>,
    query: &'session str,
    limits: SearchLimits,
    mode: SearchMode,
    searched_bytes: usize,
    exhausted: bool,
}

impl<'session, 'input> SearchSession<'session, 'input> {
    pub fn new(
        extraction: &'session Extraction<'input>,
        query: &'session str,
        limits: SearchLimits,
        mode: SearchMode,
    ) -> Result<Self, SearchError> {
        validate_query(query, limits, mode)?;
        Ok(Self {
            extraction,
            query,
            limits,
            mode,
            searched_bytes: 0,
            exhausted: false,
        })
    }

    /// Returns only this page's hits, with cumulative coverage since session
    /// creation. An empty terminal page is possible. `None` means extracted
    /// text is exhausted, not that extraction or source coverage is complete.
    /// Failure (including the final control check) leaves the cursor unchanged.
    pub fn next_page(
        &mut self,
        mut check_control: impl FnMut() -> Result<(), StopReason>,
    ) -> Result<Option<SearchResult<'input>>, SearchError> {
        check_control().map_err(SearchError::Stopped)?;
        if self.exhausted {
            return Ok(None);
        }
        let page = search_from(
            self.extraction,
            self.query,
            self.limits,
            self.mode,
            self.searched_bytes,
            check_control,
        )?;
        self.searched_bytes = page.coverage.searched_bytes;
        self.exhausted = self.searched_bytes == self.extraction.coverage().extracted_bytes();
        Ok(Some(page))
    }
}

// start_byte is internal, either zero or the end of a successfully returned
// match. Therefore it is a UTF-8 boundary in this immutable extraction.
pub(crate) fn search_from<'input>(
    extraction: &Extraction<'input>,
    query: &str,
    limits: SearchLimits,
    mode: SearchMode,
    start_byte: usize,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<SearchResult<'input>, SearchError> {
    let mut matches = Vec::new();
    let mut searched_bytes = start_byte;
    'passages: for passage in extraction.passages() {
        check_control().map_err(SearchError::Stopped)?;
        if passage.byte_range().end <= start_byte {
            continue;
        }
        let local_start = start_byte.saturating_sub(passage.byte_range().start);
        let Some(remainder) = passage.text().get(local_start..) else {
            // Unreachable through the private cursor; never use unchecked slicing.
            return Err(SearchError::InvalidCursor);
        };
        let mut occurrences = remainder.match_indices(query);
        loop {
            check_control().map_err(SearchError::Stopped)?;
            let Some((offset, text)) = occurrences.next() else {
                searched_bytes = passage.byte_range().end;
                break;
            };
            let offset = local_start + offset;
            if mode == SearchMode::WhitespaceToken {
                // match_indices guarantees UTF-8 boundaries within the passage.
                let before = passage
                    .text()
                    .get(..offset)
                    .and_then(|s| s.chars().next_back());
                let after = passage
                    .text()
                    .get(offset + text.len()..)
                    .and_then(|s| s.chars().next());
                if !before.is_none_or(char::is_whitespace) || !after.is_none_or(char::is_whitespace)
                {
                    continue;
                }
            }
            // These additions stay within the validated original buffer:
            // match_indices returns byte spans inside this exact passage.
            let start = passage.byte_range().start + offset;
            let end = start + text.len();
            matches
                .try_reserve(1)
                .map_err(|_| SearchError::AllocationFailed)?;
            matches.push(TextMatch {
                byte_range: start..end,
                line_number: passage.line_number(),
                text,
            });
            searched_bytes = end;
            if matches.len() == limits.max_matches {
                break 'passages;
            }
        }
    }
    check_control().map_err(SearchError::Stopped)?;
    Ok(SearchResult {
        matches,
        coverage: SearchCoverage {
            extraction: extraction.coverage().clone(),
            searched_bytes,
        },
        limits,
        mode,
    })
}
