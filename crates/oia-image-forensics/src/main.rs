#![deny(clippy::indexing_slicing)]

use oia_image_forensics::{Config, analyze};
use std::collections::HashSet;
use std::env;
use std::error::Error;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "Usage: oia-image-forensics INPUT [OPTIONS]\n\nOffline PNG/JPEG diagnostic cues; never an authenticity or AI probability verdict.\nJSON report on stdout; diagnostics on stderr. Exit 0: configured profile complete,\n2: profile partial, 1: error/stopped. Unimplemented detectors remain explicit.\nJPEG reports are partial: entropy completeness and decoder recovery are unverified.\n\nOptions (defaults recorded in every report):\n  --residual-map PATH      Create a derived JPEG residual PNG; never overwrite\n  --jpeg-quality N         Recompression quality 1..100 (default 90)\n  --block-size N           Nonoverlapping grid size 4..64 (default 16)\n  --max-input-bytes N      Refuse larger input (default 33554432)\n  --max-pixels N           Refuse larger decoded raster (default 4000000)\n  --max-blocks N           Bound grid scan, reporting partial coverage (65536)\n  --max-candidates N       Bound repeat candidates, reporting omissions (100)\n  --deadline-ms N          Cooperative analysis deadline (default 30000)\n  --help                  Show usage\n";

fn argument_error(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

fn run() -> Result<ExitCode, Box<dyn Error>> {
    let mut arguments = env::args_os().skip(1);
    let Some(input_argument) = arguments.next() else {
        return Err(argument_error(USAGE).into());
    };
    if input_argument == "--help" {
        io::stdout().lock().write_all(USAGE.as_bytes())?;
        return Ok(ExitCode::SUCCESS);
    }
    let input_path = PathBuf::from(input_argument);
    let mut config = Config::default();
    let mut residual_path = None;
    let mut seen = HashSet::new();
    while let Some(option) = arguments.next() {
        let option = option
            .to_str()
            .ok_or_else(|| argument_error("options must be UTF-8"))?;
        if !seen.insert(option.to_owned()) {
            return Err(argument_error(format!("duplicate option: {option}")).into());
        }
        let value = arguments
            .next()
            .ok_or_else(|| argument_error(format!("missing value: {option}")))?;
        if option == "--residual-map" {
            residual_path = Some(PathBuf::from(value));
            config.emit_residual_map = true;
            continue;
        }
        let value = value
            .to_str()
            .ok_or_else(|| argument_error("numeric setting must be UTF-8"))?;
        match option {
            "--jpeg-quality" => config.jpeg_quality = value.parse()?,
            "--block-size" => config.block_size = value.parse()?,
            "--max-input-bytes" => config.max_input_bytes = value.parse()?,
            "--max-pixels" => config.max_pixels = value.parse()?,
            "--max-blocks" => config.max_blocks = value.parse()?,
            "--max-candidates" => config.max_candidates = value.parse()?,
            "--deadline-ms" => config.deadline_ms = value.parse()?,
            _ => return Err(argument_error(format!("unknown option: {option}")).into()),
        }
    }
    config.validate()?;
    if !fs::metadata(&input_path)?.is_file() {
        return Err(argument_error("input must be a regular file").into());
    }
    let input_file = File::open(&input_path)?;
    let information = input_file.metadata()?;
    if !information.is_file() {
        return Err(argument_error("opened input is not a regular file").into());
    }
    if information.len() > config.max_input_bytes as u64 {
        return Err(argument_error("input byte limit reached; input was not truncated").into());
    }
    let read_limit = config
        .max_input_bytes
        .checked_add(1)
        .ok_or_else(|| argument_error("input limit too large"))?;
    let mut input = Vec::new();
    input_file.take(read_limit as u64).read_to_end(&mut input)?;
    let analysis = analyze(&input, &config, || Ok(()))?;
    let report = serde_json::to_vec_pretty(&analysis.report)?;
    if let Some(path) = residual_path {
        let bytes = analysis.residual_png.as_ref().ok_or_else(|| {
            argument_error("a recompression residual map is available only for JPEG input")
        })?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        if let Err(error) = output.write_all(bytes).and_then(|()| output.sync_all()) {
            drop(output);
            if let Err(cleanup_error) = fs::remove_file(&path) {
                eprintln!(
                    "partial derivative retained at {}: {cleanup_error}",
                    path.display()
                );
            }
            return Err(error.into());
        }
    }
    let mut stdout = io::stdout().lock();
    stdout.write_all(&report)?;
    stdout.write_all(b"\n")?;
    Ok(if analysis.report.profile_is_partial() {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    })
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("oia-image-forensics: {error}");
            ExitCode::FAILURE
        }
    }
}
