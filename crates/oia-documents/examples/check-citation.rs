use std::{
    env,
    error::Error,
    fs::File,
    io::{self, Read, Write},
    process::ExitCode,
    time::{Duration, Instant},
};

use oia_documents::citations::{Citation, CitationLimits, validate_citations};
use oia_documents::{ExtractionLimits, StopReason, extract_utf8};

fn argument_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "usage: check-citation PATH START_BYTE END_BYTE QUOTE (decimal byte offsets)",
    )
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let path = args.next().ok_or_else(argument_error)?;
    let mut offset = || -> Result<usize, Box<dyn Error>> {
        Ok(args
            .next()
            .ok_or_else(argument_error)?
            .into_string()
            .map_err(|_| argument_error())?
            .parse()?)
    };
    let start = offset()?;
    let end = offset()?;
    let quote = args
        .next()
        .ok_or_else(argument_error)?
        .into_string()
        .map_err(|_| argument_error())?;
    if args.next().is_some() || quote.len() > 4096 {
        return Err(argument_error().into());
    }
    let mut input = Vec::new();
    File::open(path)?.take(1_048_577).read_to_end(&mut input)?;
    let started = Instant::now();
    let mut control = || {
        if started.elapsed() >= Duration::from_secs(5) {
            Err(StopReason::DeadlineExceeded)
        } else {
            Ok(())
        }
    };
    let extraction = extract_utf8(
        &input,
        ExtractionLimits::new(1_048_576, 10_000)?,
        &mut control,
    )?;
    validate_citations(
        &extraction,
        &[Citation {
            byte_range: start..end,
            quote: &quote,
        }],
        CitationLimits::new(1, 4096)?,
        &mut control,
    )?;
    writeln!(
        io::stdout().lock(),
        "Exact quote and byte range match the supplied extraction; source version, entailment and human review are unchecked."
    )?;
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "{error}");
            ExitCode::FAILURE
        }
    }
}
