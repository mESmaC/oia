use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs::File;
use std::io::{self, Read, Write};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use oia_documents::search::{SearchLimits, SearchMode, SearchSession};
use oia_documents::{ExtractionLimits, StopReason, extract_utf8};

fn invalid_argument(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn parse_limit(argument: Option<OsString>, default: usize) -> Result<usize, Box<dyn Error>> {
    match argument {
        Some(argument) => Ok(argument
            .into_string()
            .map_err(|_| invalid_argument("limits must be decimal integers"))?
            .parse()?),
        None => Ok(default),
    }
}

fn run() -> Result<ExitCode, Box<dyn Error>> {
    let mut arguments = env::args_os().skip(1);
    let usage = "usage: find-text-pages [--token | --] PATH QUERY [PAGE_SIZE] [MAX_PAGES] [MAX_INPUT_BYTES] [MAX_PASSAGES]";
    let first = arguments.next().ok_or_else(|| invalid_argument(usage))?;
    let token_mode = first == "--token";
    let path = if token_mode || first == "--" {
        arguments.next().ok_or_else(|| invalid_argument(usage))?
    } else {
        first
    };
    let query = arguments
        .next()
        .ok_or_else(|| invalid_argument(usage))?
        .into_string()
        .map_err(|_| invalid_argument("query must be valid Unicode"))?;
    let search_limits = SearchLimits::new(4096, parse_limit(arguments.next(), 100)?)?;
    let max_pages = parse_limit(arguments.next(), 10)?;
    if max_pages == 0 {
        return Err(invalid_argument("page budget must be positive").into());
    }
    let mode = if token_mode {
        SearchMode::WhitespaceToken
    } else {
        SearchMode::ExactSubstring
    };
    let extraction_limits = ExtractionLimits::new(
        parse_limit(arguments.next(), 1_048_576)?,
        parse_limit(arguments.next(), 10_000)?,
    )?;
    if arguments.next().is_some() {
        return Err(invalid_argument("too many arguments").into());
    }
    // Validate the query before opening or reading a file.
    let empty = extract_utf8(b"", extraction_limits, || Ok(()))?;
    SearchSession::new(&empty, &query, search_limits, mode)?;
    let read_limit = extraction_limits
        .max_input_bytes()
        .checked_add(1)
        .ok_or_else(|| invalid_argument("byte limit is too large for bounded file search"))?;
    let file = File::open(&path)?;
    if !file.metadata()?.is_file() {
        return Err(invalid_argument("search requires a regular local file").into());
    }
    let mut input = Vec::new();
    input.try_reserve_exact(read_limit)?;
    file.take(u64::try_from(read_limit)?)
        .read_to_end(&mut input)?;
    if input.len() > extraction_limits.max_input_bytes() {
        return Err(
            invalid_argument("input exceeds the byte limit; no search result returned").into(),
        );
    }
    let started = Instant::now();
    let mut check_control = || {
        if started.elapsed() >= Duration::from_secs(5) {
            Err(StopReason::DeadlineExceeded)
        } else {
            Ok(())
        }
    };
    let extraction = extract_utf8(&input, extraction_limits, &mut check_control)?;
    let mut session = SearchSession::new(&extraction, &query, search_limits, mode)?;
    let mut complete = false;
    let mut output = io::stdout().lock();
    writeln!(
        output,
        "page_size={}; max_pages={max_pages}; coverage is cumulative across pages",
        search_limits.max_matches()
    )?;
    for page_number in 1..=max_pages {
        let Some(result) = session.next_page(&mut check_control)? else {
            break;
        };
        writeln!(output, "page={page_number}")?;
        let coverage = result.coverage();
        let status = if coverage.is_complete() {
            "complete"
        } else {
            "partial"
        };
        writeln!(
            output,
            "Local UTF-8 passage search; exact, case-sensitive, non-overlapping; no OCR or semantic retrieval"
        )?;
        writeln!(
            output,
            "matching={}",
            match result.mode() {
                SearchMode::ExactSubstring => "substring",
                SearchMode::WhitespaceToken =>
                    "whole Unicode-whitespace token; punctuation is literal",
            }
        )?;
        writeln!(
            output,
            "input={path:?}; query={query:?}; status={status}; matches={}",
            result.matches().len()
        )?;
        writeln!(
            output,
            "limits: input_bytes={}; passages={}; query_bytes={}; matches={}; processing_deadline_seconds=5",
            extraction_limits.max_input_bytes(),
            extraction_limits.max_passages(),
            search_limits.max_query_bytes(),
            search_limits.max_matches()
        )?;
        writeln!(
            output,
            "coverage: searched_bytes={}; extracted_bytes={}; input_bytes={}; match_budget_unsearched={:?}; unextracted={:?}",
            coverage.searched_bytes(),
            coverage.extraction().extracted_bytes(),
            coverage.extraction().input_bytes(),
            coverage.unsearched_extracted_range(),
            coverage.extraction().unprocessed_range()
        )?;
        for hit in result.matches() {
            writeln!(
                output,
                "line={} bytes={:?} text={:?}",
                hit.line_number(),
                hit.byte_range(),
                hit.text()
            )?;
        }
        complete = coverage.is_complete();
        if coverage.unsearched_extracted_range().is_none() {
            break;
        }
    }
    writeln!(
        output,
        "session_status={}",
        if complete {
            "complete"
        } else {
            "partial; inspect unsearched and unextracted ranges above"
        }
    )?;
    Ok(if complete {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(2)
    })
}

fn main() -> ExitCode {
    match run() {
        Ok(exit_code) => exit_code,
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "find-text-pages: {error}");
            ExitCode::FAILURE
        }
    }
}
