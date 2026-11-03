//! Structural quote validation only; callers bind authorized source versions.
//! Success does not establish entailment, authenticity, or human review.

use std::{error::Error, fmt, ops::Range};

use crate::{Extraction, StopReason};

/// Passive, untrusted citation data. Byte offsets are half-open and zero-based.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Citation<'quote> {
    pub byte_range: Range<usize>,
    pub quote: &'quote str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CitationLimits {
    max_citations: usize,
    max_quote_bytes: usize,
}

impl CitationLimits {
    /// Quote bytes are cumulative, including repeated/overlapping citations.
    pub fn new(max_citations: usize, max_quote_bytes: usize) -> Result<Self, CitationError> {
        if max_citations == 0 || max_quote_bytes == 0 {
            return Err(CitationError::ZeroLimit);
        }
        Ok(Self {
            max_citations,
            max_quote_bytes,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CitationProblem {
    EmptyOrReversedRange,
    OutsideInput,
    NotExtracted,
    QuoteLengthMismatch,
    InvalidCharacterBoundary,
    QuoteMismatch,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CitationError {
    ZeroLimit,
    TooManyCitations,
    QuoteBudgetExceeded,
    Invalid {
        citation_index: usize,
        problem: CitationProblem,
    },
    Stopped(StopReason),
}

impl fmt::Display for CitationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroLimit => write!(f, "citation limits must be greater than zero"),
            Self::TooManyCitations => write!(f, "citation count exceeds configured limit"),
            Self::QuoteBudgetExceeded => write!(f, "total quote bytes exceed configured limit"),
            Self::Invalid {
                citation_index,
                problem,
            } => {
                write!(
                    f,
                    "citation {citation_index} failed structural validation: {problem:?}"
                )
            }
            Self::Stopped(reason) => write!(f, "citation validation stopped: {reason:?}"),
        }
    }
}

impl Error for CitationError {}

/// Validate every citation against the extracted prefix without copying text.
/// Quotes may span passages. No normalization, anchor repair, or source lookup
/// occurs. An empty batch succeeds but establishes nothing about a draft.
/// Work is bounded by citation/quote budgets and extraction limits. Control is
/// checked during preflight, per passage, per 4096-byte comparison, and before
/// success. A failed/stopped batch grants no partial validation result.
pub fn validate_citations(
    extraction: &Extraction<'_>,
    citations: &[Citation<'_>],
    limits: CitationLimits,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<(), CitationError> {
    check_control().map_err(CitationError::Stopped)?;
    if citations.len() > limits.max_citations {
        return Err(CitationError::TooManyCitations);
    }
    let mut quote_bytes = 0usize;
    for citation in citations {
        check_control().map_err(CitationError::Stopped)?;
        quote_bytes = quote_bytes
            .checked_add(citation.quote.len())
            .filter(|total| *total <= limits.max_quote_bytes)
            .ok_or(CitationError::QuoteBudgetExceeded)?;
    }
    for (citation_index, citation) in citations.iter().enumerate() {
        check_control().map_err(CitationError::Stopped)?;
        let invalid = |problem| CitationError::Invalid {
            citation_index,
            problem,
        };
        let range = &citation.byte_range;
        if range.start >= range.end {
            return Err(invalid(CitationProblem::EmptyOrReversedRange));
        }
        if range.end > extraction.coverage().input_bytes() {
            return Err(invalid(CitationProblem::OutsideInput));
        }
        if range.end > extraction.coverage().extracted_bytes() {
            return Err(invalid(CitationProblem::NotExtracted));
        }
        if range.end - range.start != citation.quote.len() {
            return Err(invalid(CitationProblem::QuoteLengthMismatch));
        }
        let mut compared = 0;
        for passage in extraction.passages() {
            check_control().map_err(CitationError::Stopped)?;
            let passage_range = passage.byte_range();
            if passage_range.start >= range.end {
                break;
            }
            let start = range.start.max(passage_range.start);
            let end = range.end.min(passage_range.end);
            if start >= end {
                continue;
            }
            let text = passage
                .text()
                .get(start - passage_range.start..end - passage_range.start)
                .ok_or_else(|| invalid(CitationProblem::InvalidCharacterBoundary))?;
            for chunk in text.as_bytes().chunks(4096) {
                check_control().map_err(CitationError::Stopped)?;
                let quote_chunk = citation
                    .quote
                    .as_bytes()
                    .get(compared..compared + chunk.len());
                if quote_chunk != Some(chunk) {
                    return Err(invalid(CitationProblem::QuoteMismatch));
                }
                compared += chunk.len();
            }
        }
        if compared != citation.quote.len() {
            return Err(invalid(CitationProblem::NotExtracted));
        }
    }
    check_control().map_err(CitationError::Stopped)
}
