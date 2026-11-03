use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs::File;
use std::io::{self, Read, Write};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use oia_documents::corpus::{CorpusInput, CorpusLimits, DocumentOutcome, search_corpus};
use oia_documents::search::{SearchLimits, SearchMode};
use oia_documents::{ExtractionLimits, StopReason, extract_utf8};

const MAX_DOCUMENTS: usize = 100;

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn limit(argument: Option<OsString>) -> Result<usize, Box<dyn Error>> {
    Ok(argument
        .ok_or_else(|| invalid("missing required limit"))?
        .into_string()
        .map_err(|_| invalid("limits must be decimal integers"))?
        .parse()?)
}

fn run() -> Result<ExitCode, Box<dyn Error>> {
    let usage = "usage: find-corpus [--token | --] QUERY MAX_MATCHES MAX_TOTAL_BYTES MAX_PASSAGES_PER_FILE PATH...";
    let mut args = env::args_os().skip(1);
    let first = args.next().ok_or_else(|| invalid(usage))?;
    let mode = if first == "--token" {
        SearchMode::WhitespaceToken
    } else {
        SearchMode::ExactSubstring
    };
    let query = if first == "--token" || first == "--" {
        args.next().ok_or_else(|| invalid(usage))?
    } else {
        first
    }
    .into_string()
    .map_err(|_| invalid("query must be valid Unicode"))?;
    let search_limits = SearchLimits::new(4096, limit(args.next())?)?;
    let total_bytes = limit(args.next())?;
    let extraction_limits = ExtractionLimits::new(total_bytes, limit(args.next())?)?;
    let limits = CorpusLimits::new(MAX_DOCUMENTS, total_bytes, search_limits)?;
    // Validate even an invalid token/empty query before any file is opened.
    search_corpus(&[], &query, limits, mode, || Ok(()))?;
    let mut paths = Vec::new();
    paths.try_reserve_exact(MAX_DOCUMENTS)?;
    for path in args {
        if paths.len() == MAX_DOCUMENTS {
            return Err(invalid("at most 100 explicit paths are supported").into());
        }
        paths.push(path);
    }
    if paths.is_empty() {
        return Err(invalid(usage).into());
    }
    let started = Instant::now();
    let mut check_control = || {
        if started.elapsed() >= Duration::from_secs(5) {
            Err(StopReason::DeadlineExceeded)
        } else {
            Ok(())
        }
    };
    let mut buffers = Vec::new();
    buffers.try_reserve_exact(paths.len())?;
    let mut bytes_read = 0_usize;
    for path in &paths {
        check_control().map_err(|_| invalid("processing deadline exceeded"))?;
        let file = File::open(path)
            .map_err(|error| io::Error::new(error.kind(), format!("{path:?}: {error}")))?;
        if !file.metadata()?.is_file() {
            return Err(invalid("every path must refer to a regular local file").into());
        }
        // At most one byte beyond the remaining shared budget is read, solely
        // to detect overflow. Do not reserve the remaining corpus budget once
        // per file: many small files must not multiply retained capacity.
        let read_limit = (total_bytes - bytes_read)
            .checked_add(1)
            .ok_or_else(|| invalid("total byte limit is too large"))?;
        let mut reader = file.take(u64::try_from(read_limit)?);
        let mut input = Vec::new();
        let mut chunk = [0_u8; 8192];
        loop {
            check_control().map_err(|_| invalid("processing deadline exceeded"))?;
            let read = reader.read(&mut chunk)?;
            if read == 0 {
                break;
            }
            if read > total_bytes - bytes_read {
                return Err(
                    invalid("files exceed total byte limit; no search result returned").into(),
                );
            }
            let bytes = chunk
                .get(..read)
                .ok_or_else(|| invalid("invalid read length"))?;
            input.try_reserve(read)?;
            input.extend_from_slice(bytes);
            bytes_read += read;
        }
        buffers.push(input);
    }
    let mut extractions = Vec::new();
    extractions.try_reserve_exact(buffers.len())?;
    for (path, buffer) in paths.iter().zip(&buffers) {
        let extraction = extract_utf8(buffer, extraction_limits, &mut check_control)
            .map_err(|error| invalid(&format!("{path:?}: {error}")))?;
        extractions.push(extraction);
    }
    let mut inputs = Vec::new();
    inputs.try_reserve_exact(extractions.len())?;
    inputs.extend(extractions.iter().map(CorpusInput::Text));
    let result = search_corpus(&inputs, &query, limits, mode, &mut check_control)?;
    let mut output = io::stdout().lock();
    writeln!(
        output,
        "Local UTF-8 corpus search; supplied files only; paths are not source-version citations"
    )?;
    writeln!(
        output,
        "query={query:?}; matching={:?}; documents={}; matches={}; status={}",
        result.mode(),
        result.documents().len(),
        result.match_count(),
        if result.is_complete() {
            "complete"
        } else {
            "partial"
        }
    )?;
    writeln!(
        output,
        "limits: documents={MAX_DOCUMENTS}; total_input_bytes={total_bytes}; matches={}; query_bytes=4096; passages_per_file={}; cooperative_deadline_seconds=5",
        search_limits.max_matches(),
        extraction_limits.max_passages()
    )?;
    writeln!(
        output,
        "known_input_bytes={}; zero hits do not establish absence outside the searched text",
        result.known_input_bytes()
    )?;
    for document in result.documents() {
        let index = document.input_index();
        let path = paths
            .get(index)
            .ok_or_else(|| invalid("invalid input slot"))?;
        writeln!(output, "input={index} path={path:?}")?;
        match document.outcome() {
            DocumentOutcome::Searched(search) => {
                let coverage = search.coverage();
                writeln!(
                    output,
                    "  searched_bytes={}; extracted_bytes={}; input_bytes={}; match_budget_unsearched={:?}; unextracted={:?}",
                    coverage.searched_bytes(),
                    coverage.extraction().extracted_bytes(),
                    coverage.extraction().input_bytes(),
                    coverage.unsearched_extracted_range(),
                    coverage.extraction().unprocessed_range()
                )?;
                for hit in search.matches() {
                    writeln!(
                        output,
                        "  line={} bytes={:?} text={:?}",
                        hit.line_number(),
                        hit.byte_range(),
                        hit.text()
                    )?;
                }
            }
            DocumentOutcome::MatchLimitReached(coverage) => {
                writeln!(
                    output,
                    "  not searched: shared match limit; unsearched_extracted=0..{}; unextracted={:?}",
                    coverage.extracted_bytes(),
                    coverage.unprocessed_range()
                )?;
            }
            DocumentOutcome::Unavailable(reason) => {
                writeln!(output, "  unavailable={reason:?}")?;
            }
        }
    }
    Ok(if result.is_complete() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(2)
    })
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "find-corpus: {error}");
            ExitCode::FAILURE
        }
    }
}
