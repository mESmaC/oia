//! Corpus search and structural citation anchors over held text snapshots.
//!
//! Composes the merged `oia-documents` corpus matcher and citation validator over the
//! session's immutable byte snapshots. Input slots are the order of the user's explicit
//! selection, never source or version identities; a hash identifies bytes only. No source
//! registry, authorization, ranking, persistence, OCR or model runs here.
use super::{
    Control, FileInfo, FileKind, MATCHES, PASSAGES, Passage, Result, Snapshot, TEXT_BYTES,
    ToolError, display_text, open_file, require_kind,
};
use oia_documents::{
    Extraction, ExtractionLimits,
    citations::{Citation, CitationError, CitationLimits, CitationProblem, validate_citations},
    corpus::{CorpusInput, CorpusLimits, DocumentOutcome, search_corpus as corpus_search},
    extract_utf8,
    search::{SearchLimits, SearchMode},
};
use serde::Serialize;
use std::path::PathBuf;

/// Files accepted in one explicitly selected corpus.
pub const CORPUS_FILES: usize = 20;
/// Original bytes shared across one corpus while it is read.
pub const CORPUS_BYTES: usize = 8 * 1024 * 1024;
/// Session-only citation records held at once.
pub const CITATIONS: usize = 100;
/// UTF-8 bytes in one quote.
pub const QUOTE_BYTES: usize = 4096;
/// UTF-8 bytes in one query, shared with the single-document search.
pub const QUERY_BYTES: usize = 512;

fn extract<'a>(snapshot: &'a Snapshot, control: &Control) -> Result<Extraction<'a>> {
    require_kind(snapshot, FileKind::Text)?;
    let limits =
        ExtractionLimits::new(TEXT_BYTES, PASSAGES).map_err(|e| ToolError::new("limits", e))?;
    extract_utf8(snapshot.bytes(), limits, || control.text_check())
        .map_err(|e| ToolError::new("extraction", e))
}

/// Reads every selected file in selection order. A count or byte overflow refuses the
/// whole selection before anything is retained; a failed read refuses it too.
pub fn open_corpus(paths: &[PathBuf], control: &Control) -> Result<Vec<Snapshot>> {
    control.check()?;
    if paths.is_empty() {
        return Err(ToolError::new("input", "Select at least one file."));
    }
    if paths.len() > CORPUS_FILES {
        return Err(ToolError::new(
            "input_limit",
            format!(
                "Select at most {CORPUS_FILES} files; {} were selected and none was loaded.",
                paths.len()
            ),
        ));
    }
    let mut snapshots = Vec::with_capacity(paths.len());
    let mut total = 0_usize;
    for path in paths {
        let snapshot = open_file(path, FileKind::Text, control)?;
        total = total
            .checked_add(snapshot.info.bytes)
            .filter(|t| *t <= CORPUS_BYTES)
            .ok_or_else(|| {
                ToolError::new(
                    "input_limit",
                    format!(
                        "The selection exceeds the {CORPUS_BYTES}-byte corpus limit; nothing was loaded."
                    ),
                )
            })?;
        snapshots.push(snapshot);
    }
    control.check()?;
    Ok(snapshots)
}

#[derive(Clone, Debug, Serialize)]
pub struct CorpusView {
    pub files: Vec<FileInfo>,
    pub total_bytes: usize,
    pub file_limit: usize,
    pub byte_limit: usize,
}

#[must_use]
pub fn corpus_view(snapshots: &[Snapshot]) -> CorpusView {
    CorpusView {
        files: snapshots.iter().map(|s| s.info.clone()).collect(),
        total_bytes: snapshots.iter().map(|s| s.info.bytes).sum(),
        file_limit: CORPUS_FILES,
        byte_limit: CORPUS_BYTES,
    }
}

/// One input slot's outcome. `outcome` is `searched`, `match_limit_reached` (the shared hit
/// limit left this file unsearched) or `unavailable`; the byte counts keep extraction and
/// search coverage separate so an unsearched file never reads as an empty one.
#[derive(Clone, Debug, Serialize)]
pub struct CorpusDocumentView {
    pub index: usize,
    pub file: FileInfo,
    pub outcome: &'static str,
    pub matches: Vec<Passage>,
    pub searched_bytes: usize,
    pub extracted_bytes: usize,
    pub input_bytes: usize,
    pub extraction_complete: bool,
    pub search_complete: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct CorpusSearchView {
    pub query: String,
    pub documents: Vec<CorpusDocumentView>,
    pub match_count: usize,
    pub known_input_bytes: usize,
    pub complete: bool,
    pub match_limit: usize,
}

/// Exact, case-sensitive matching over every held corpus snapshot in selection order with
/// one shared hit limit. Results carry original byte anchors; a stop discards everything.
pub fn search_corpus(
    snapshots: &[Snapshot],
    query: &str,
    control: &Control,
) -> Result<CorpusSearchView> {
    let extractions = snapshots
        .iter()
        .map(|s| extract(s, control))
        .collect::<Result<Vec<_>>>()?;
    let inputs: Vec<CorpusInput<'_, '_>> = extractions.iter().map(CorpusInput::Text).collect();
    let search =
        SearchLimits::new(QUERY_BYTES, MATCHES).map_err(|e| ToolError::new("limits", e))?;
    let limits = CorpusLimits::new(CORPUS_FILES, CORPUS_BYTES, search)
        .map_err(|e| ToolError::new("limits", e))?;
    let result = corpus_search(&inputs, query, limits, SearchMode::ExactSubstring, || {
        control.text_check()
    })
    .map_err(|e| ToolError::new("search", e))?;
    let mut documents = Vec::with_capacity(result.documents().len());
    for document in result.documents() {
        let file = snapshots
            .get(document.input_index())
            .map(|s| s.info.clone())
            .ok_or_else(|| ToolError::new("internal", "A result slot has no snapshot."))?;
        let view = match document.outcome() {
            DocumentOutcome::Searched(r) => {
                let coverage = r.coverage();
                CorpusDocumentView {
                    index: document.input_index(),
                    file,
                    outcome: "searched",
                    matches: r
                        .matches()
                        .iter()
                        .map(|m| Passage {
                            line: m.line_number(),
                            start: m.byte_range().start,
                            end: m.byte_range().end,
                            text: display_text(m.text()),
                        })
                        .collect(),
                    searched_bytes: coverage.searched_bytes(),
                    extracted_bytes: coverage.extraction().extracted_bytes(),
                    input_bytes: coverage.extraction().input_bytes(),
                    extraction_complete: coverage.extraction().unprocessed_range().is_none(),
                    search_complete: coverage.is_complete(),
                }
            }
            DocumentOutcome::MatchLimitReached(coverage) => CorpusDocumentView {
                index: document.input_index(),
                file,
                outcome: "match_limit_reached",
                matches: Vec::new(),
                searched_bytes: 0,
                extracted_bytes: coverage.extracted_bytes(),
                input_bytes: coverage.input_bytes(),
                extraction_complete: coverage.unprocessed_range().is_none(),
                search_complete: false,
            },
            DocumentOutcome::Unavailable(_) => CorpusDocumentView {
                input_bytes: file.bytes,
                index: document.input_index(),
                file,
                outcome: "unavailable",
                matches: Vec::new(),
                searched_bytes: 0,
                extracted_bytes: 0,
                extraction_complete: false,
                search_complete: false,
            },
        };
        documents.push(view);
    }
    control.check()?;
    Ok(CorpusSearchView {
        query: query.to_owned(),
        documents,
        match_count: result.match_count(),
        known_input_bytes: result.known_input_bytes(),
        complete: result.is_complete(),
        match_limit: MATCHES,
    })
}

/// A session-only citation record: a half-open byte range in a held snapshot and a quote,
/// checked structurally against the snapshot's extraction. `status` is `valid` or
/// `invalid`; an invalid record keeps the kernel's reason. The quote is escaped for display
/// like the reader; `quote_bytes` is the exact length. Nothing here is a source-version
/// citation, entailment, authenticity or review.
#[derive(Clone, Debug, Serialize)]
pub struct CitationView {
    pub file: FileInfo,
    pub start: usize,
    pub end: usize,
    pub line: Option<usize>,
    pub quote: String,
    pub quote_bytes: usize,
    pub origin: &'static str,
    pub status: &'static str,
    pub problem: Option<String>,
    pub check: &'static str,
}

const CHECK: &str = "Structural check of the retained bytes in this session: the quote equals the bytes at the half-open range, which lies in the extracted text on character boundaries. Not a source-version citation, entailment, authenticity or review.";

fn describe(problem: CitationProblem) -> String {
    match problem {
        CitationProblem::EmptyOrReversedRange => "The range is empty or reversed.",
        CitationProblem::OutsideInput => "The range ends beyond the file.",
        CitationProblem::NotExtracted => {
            "The range reaches bytes that were not extracted (passage limit)."
        }
        CitationProblem::QuoteLengthMismatch => {
            "The quote's UTF-8 length differs from the range length."
        }
        CitationProblem::InvalidCharacterBoundary => {
            "The range does not start and end on character boundaries."
        }
        CitationProblem::QuoteMismatch => "The quote differs from the bytes at the range.",
    }
    .to_owned()
}

fn validate(
    extraction: &Extraction<'_>,
    start: usize,
    end: usize,
    quote: &str,
    control: &Control,
) -> Result<Option<String>> {
    let limits = CitationLimits::new(1, QUOTE_BYTES).map_err(|e| ToolError::new("limits", e))?;
    let citation = Citation {
        byte_range: start..end,
        quote,
    };
    match validate_citations(extraction, &[citation], limits, || control.text_check()) {
        Ok(()) => Ok(None),
        Err(CitationError::Invalid { problem, .. }) => Ok(Some(describe(problem))),
        Err(e) => Err(ToolError::new("citation", e)),
    }
}

fn line_of(extraction: &Extraction<'_>, start: usize) -> Option<usize> {
    extraction
        .passages()
        .iter()
        .find(|p| p.byte_range().contains(&start))
        .map(|p| p.line_number())
}

fn record(
    snapshot: &Snapshot,
    extraction: &Extraction<'_>,
    start: usize,
    end: usize,
    quote: &str,
    origin: &'static str,
    problem: Option<String>,
) -> CitationView {
    CitationView {
        file: snapshot.info.clone(),
        start,
        end,
        line: line_of(extraction, start),
        quote: display_text(quote),
        quote_bytes: quote.len(),
        origin,
        status: if problem.is_none() {
            "valid"
        } else {
            "invalid"
        },
        problem,
        check: CHECK,
    }
}

/// Cites a range of the held snapshot. The quote is copied from the retained bytes, then
/// the kernel checks the anchor; stale or partial offsets from the page yield an invalid
/// record rather than a repaired one.
pub fn cite_selection(
    snapshot: &Snapshot,
    start: usize,
    end: usize,
    control: &Control,
) -> Result<CitationView> {
    let extraction = extract(snapshot, control)?;
    let Some(quote) = snapshot
        .bytes()
        .get(start..end)
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
    else {
        return Err(ToolError::new(
            "citation",
            "The selected range is not a whole UTF-8 span of the retained bytes.",
        ));
    };
    if quote.len() > QUOTE_BYTES {
        return Err(ToolError::new(
            "input_limit",
            format!("A quote is limited to {QUOTE_BYTES} UTF-8 bytes."),
        ));
    }
    let problem = validate(&extraction, start, end, quote, control)?;
    control.check()?;
    Ok(record(
        snapshot,
        &extraction,
        start,
        end,
        quote,
        "selection",
        problem,
    ))
}

/// Checks a user-supplied range and quote against the held snapshot. A rejected citation
/// is a result with its reason, not an operation failure; nothing is repaired or searched.
pub fn check_citation(
    snapshot: &Snapshot,
    start: usize,
    end: usize,
    quote: &str,
    control: &Control,
) -> Result<CitationView> {
    if quote.len() > QUOTE_BYTES {
        return Err(ToolError::new(
            "input_limit",
            format!("A quote is limited to {QUOTE_BYTES} UTF-8 bytes."),
        ));
    }
    let extraction = extract(snapshot, control)?;
    let problem = validate(&extraction, start, end, quote, control)?;
    control.check()?;
    Ok(record(
        snapshot,
        &extraction,
        start,
        end,
        quote,
        "supplied",
        problem,
    ))
}
