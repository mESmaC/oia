use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs::File;
use std::io::{self, Read, Write};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use oia_documents::{ExtractionLimits, ExtractionStatus, StopReason, extract_utf8};

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
    let path = arguments.next().ok_or_else(|| {
        invalid_argument("usage: inspect-text PATH [MAX_INPUT_BYTES] [MAX_PASSAGES]")
    })?;
    let limits = ExtractionLimits::new(
        parse_limit(arguments.next(), 1_048_576)?,
        parse_limit(arguments.next(), 10_000)?,
    )?;
    if arguments.next().is_some() {
        return Err(invalid_argument("too many arguments").into());
    }
    let read_limit = limits
        .max_input_bytes()
        .checked_add(1)
        .ok_or_else(|| invalid_argument("byte limit is too large for bounded file inspection"))?;
    let file = File::open(&path)?;
    if !file.metadata()?.is_file() {
        return Err(invalid_argument("inspection requires a regular local file").into());
    }
    let mut input = Vec::new();
    file.take(u64::try_from(read_limit)?)
        .read_to_end(&mut input)?;
    if input.len() > limits.max_input_bytes() {
        return Err(invalid_argument(
            "input exceeds the byte limit; only a bounded prefix was read and no extraction returned",
        )
        .into());
    }
    let started = Instant::now();
    let extraction = extract_utf8(&input, limits, || {
        if started.elapsed() >= Duration::from_secs(5) {
            Err(StopReason::DeadlineExceeded)
        } else {
            Ok(())
        }
    })?;
    let (status, exit_code) = match extraction.coverage().status() {
        ExtractionStatus::Complete => ("complete", ExitCode::SUCCESS),
        ExtractionStatus::Empty => ("empty", ExitCode::SUCCESS),
        ExtractionStatus::PassageLimitReached => {
            ("partial: passage limit reached", ExitCode::from(2))
        }
    };
    let mut output = io::stdout().lock();
    writeln!(
        output,
        "UTF-8 text inspection only; no file-type detection, PDF parsing, or OCR"
    )?;
    writeln!(output, "input={path:?}; status={status}")?;
    writeln!(
        output,
        "limits: input_bytes={}; passages={}; extraction_deadline_seconds=5",
        limits.max_input_bytes(),
        limits.max_passages()
    )?;
    writeln!(
        output,
        "coverage: extracted_bytes={}; input_bytes={}; unprocessed={:?}",
        extraction.coverage().extracted_bytes(),
        extraction.coverage().input_bytes(),
        extraction.coverage().unprocessed_range()
    )?;
    for passage in extraction.passages() {
        writeln!(
            output,
            "line={} bytes={:?} text={:?}",
            passage.line_number(),
            passage.byte_range(),
            passage.text()
        )?;
    }
    Ok(exit_code)
}

fn main() -> ExitCode {
    match run() {
        Ok(exit_code) => exit_code,
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "inspect-text: {error}");
            ExitCode::FAILURE
        }
    }
}
