//! Read-only local comparison. Supplied timestamps describe retrieval of the
//! captured text; file modification times are never substituted for provenance.
use oia_analysis::snapshot_change::{
    Capture, ChangeRule, MAX_TEXT_BYTES, METHOD_ID, RuleDecision, Snapshot, SnapshotInput,
    SnapshotScope, StopReason, TextComparison, evaluate,
};
use oia_domain::{OpaqueId, VersionId};
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn seconds(argument: OsString) -> Result<u64, Box<dyn Error>> {
    Ok(argument
        .into_string()
        .map_err(|_| invalid("time must be Unicode decimal seconds"))?
        .parse()?)
}

fn instant(seconds: u64) -> Result<SystemTime, io::Error> {
    UNIX_EPOCH
        .checked_add(Duration::from_secs(seconds))
        .ok_or_else(|| invalid("timestamp is outside this platform's SystemTime range"))
}

fn read_text(path: &Path) -> Result<String, Box<dyn Error>> {
    let file = File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(invalid("capture must be a regular local file").into());
    }
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(MAX_TEXT_BYTES + 1)?;
    file.take(u64::try_from(MAX_TEXT_BYTES + 1)?)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_TEXT_BYTES {
        return Err(invalid("capture exceeds 1,048,576 bytes; comparison refused").into());
    }
    Ok(String::from_utf8(bytes)?)
}

// Display derivative only: ASCII escapes keep terminal and bidi controls inert.
fn excerpt(output: &mut impl Write, field: &str, text: &str) -> io::Result<()> {
    const MAX_CHARS: usize = 240;
    write!(output, "{field}=\"")?;
    let mut chars = text.chars();
    for character in chars.by_ref().take(MAX_CHARS) {
        write!(output, "{}", character.escape_default())?;
    }
    writeln!(output, "\"; preview_truncated={}", chars.next().is_some())
}

fn run() -> Result<ExitCode, Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let usage = "usage: compare-snapshots BEFORE AFTER SOURCE_REFERENCE BEFORE_RETRIEVED_SECONDS AFTER_RETRIEVED_SECONDS [MAX_AGE_SECONDS]";
    let before_path = args.next().ok_or_else(|| invalid(usage))?;
    let after_path = args.next().ok_or_else(|| invalid(usage))?;
    let attribution = args
        .next()
        .ok_or_else(|| invalid(usage))?
        .into_string()
        .map_err(|_| invalid("source reference must be Unicode"))?;
    let before_time = instant(seconds(args.next().ok_or_else(|| invalid(usage))?)?)?;
    let after_time = instant(seconds(args.next().ok_or_else(|| invalid(usage))?)?)?;
    let max_age = args.next().map(seconds).transpose()?.unwrap_or(86_400);
    if args.next().is_some() {
        return Err(invalid(usage).into());
    }
    let scope = SnapshotScope {
        workspace: OpaqueId::new("local-comparison")?,
        source: OpaqueId::new("caller-supplied-source")?,
        resource: OpaqueId::new("caller-supplied-resource")?,
    };
    let rule = ChangeRule::new(
        scope.clone(),
        VersionId::new("local-rule-v1")?,
        Duration::from_secs(max_age),
    )?;
    let before_input = SnapshotInput {
        scope: scope.clone(),
        version: VersionId::new("local-before-v1")?,
        attribution: &attribution,
        retrieved_at: before_time,
        capture: Capture::Complete(""),
    };
    let after_input = SnapshotInput {
        scope,
        version: VersionId::new("local-after-v1")?,
        attribution: &attribution,
        retrieved_at: after_time,
        capture: Capture::Complete(""),
    };
    // Validate attribution, scope and timestamps before opening the input files.
    let before_empty = Snapshot::new(before_input.clone())?;
    let after_empty = Snapshot::new(after_input.clone())?;
    evaluate(
        &rule,
        &before_empty,
        &after_empty,
        SystemTime::now(),
        || Ok(()),
    )?;
    let before_text = read_text(Path::new(&before_path))?;
    let after_text = read_text(Path::new(&after_path))?;
    let before = Snapshot::new(SnapshotInput {
        capture: Capture::Complete(&before_text),
        ..before_input
    })?;
    let after = Snapshot::new(SnapshotInput {
        capture: Capture::Complete(&after_text),
        ..after_input
    })?;
    let started = Instant::now();
    let result = evaluate(&rule, &before, &after, SystemTime::now(), || {
        if started.elapsed() >= Duration::from_secs(5) {
            Err(StopReason::DeadlineExceeded)
        } else {
            Ok(())
        }
    })?;
    let mut output = io::stdout().lock();
    writeln!(
        output,
        "Local UTF-8 comparison; files asserted complete by caller; no network or notification delivery"
    )?;
    writeln!(
        output,
        "IDs are transient example labels, not persisted evidence versions. No content freshness or safety claim."
    )?;
    excerpt(&mut output, "source_attribution", &attribution)?;
    writeln!(
        output,
        "method={METHOD_ID}; max_capture_bytes={MAX_TEXT_BYTES}; comparison_deadline_seconds=5"
    )?;
    writeln!(
        output,
        "rule={}; criterion=any exact text change; max_retrieval_age_seconds={max_age}",
        rule.version()
    )?;
    writeln!(
        output,
        "before={}; retrieved_unix_seconds={}; bytes={}; coverage=complete",
        before.input().version,
        before_time.duration_since(UNIX_EPOCH)?.as_secs(),
        before_text.len()
    )?;
    writeln!(
        output,
        "after={}; retrieved_unix_seconds={}; bytes={}; coverage=complete",
        after.input().version,
        after_time.duration_since(UNIX_EPOCH)?.as_secs(),
        after_text.len()
    )?;
    writeln!(
        output,
        "baseline_retrieval_age_seconds={}; baseline_retrieval_freshness={:?}; historical baseline age does not suppress a fresh current comparison",
        result.baseline_retrieval_age().as_secs(),
        result.baseline_retrieval_freshness()
    )?;
    writeln!(
        output,
        "evaluated_unix_seconds={}; current_retrieval_age_seconds={}; retrieval_freshness={:?}; decision={:?}",
        result.evaluated_at().duration_since(UNIX_EPOCH)?.as_secs(),
        result.current_retrieval_age().as_secs(),
        result.retrieval_freshness(),
        result.decision()
    )?;
    writeln!(
        output,
        "comparison={:?}; replacement spans may include unchanged text between edits",
        result.comparison()
    )?;
    if let TextComparison::Changed { before, after } = result.comparison() {
        excerpt(
            &mut output,
            "before_span",
            before_text
                .get(before.clone())
                .ok_or_else(|| invalid("invalid before span"))?,
        )?;
        excerpt(
            &mut output,
            "after_span",
            after_text
                .get(after.clone())
                .ok_or_else(|| invalid("invalid after span"))?,
        )?;
    }
    Ok(match result.decision() {
        RuleDecision::ChangeDetected | RuleDecision::NoTextChange => ExitCode::SUCCESS,
        RuleDecision::IncompleteCaptures | RuleDecision::StaleCurrentCapture => ExitCode::from(2),
    })
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("compare-snapshots: {error}");
            ExitCode::FAILURE
        }
    }
}
