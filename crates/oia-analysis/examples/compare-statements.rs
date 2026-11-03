//! Read-only lexical comparison of local UTF-8 text files (#138).
//!
//! Usage: `compare-statements <class>:<account>[@<revision>]=<path> ...`
//! where class is `testimony`, `police`, `news` or `other`. Files are labeled
//! `Reported` origin with unknown exposure and no coded assertions, so the aspect
//! table is empty; the overlap matrix, copy candidates, independence groups and
//! revision text overlap are printed. Each read stops one byte past the
//! kernel's per-text limit so oversized files are refused, not truncated.
//! Copied-text previews are escaped and capped at 240 characters for display
//! only; comparison uses the whole bounded text. Nothing is written.
use oia_analysis::statement_comparison::{
    ComparisonParameters, ComparisonRequest, CopySearch, ExplanationExposure, MAX_TEXT_BYTES,
    OverlapMeasure, SourceClass, StatementSource, TextCapture, compare_statements,
};
use oia_domain::witness::AccountOrigin;
use oia_domain::{ActorId, OpaqueId, VersionId};
use std::error::Error;
use std::io::Read;
use std::time::Instant;

struct Spec {
    class: SourceClass,
    account: String,
    revision: u32,
    path: String,
}

fn parse(arg: &str) -> Result<Spec, Box<dyn Error>> {
    let (head, path) = arg
        .split_once('=')
        .ok_or("expected <class>:<account>[@rev]=<path>")?;
    let (class, account) = head.split_once(':').ok_or("expected <class>:<account>")?;
    let class = match class {
        "testimony" => SourceClass::Testimony,
        "police" => SourceClass::PoliceReport,
        "news" => SourceClass::NewsReport,
        "other" => SourceClass::Other,
        _ => return Err(format!("unknown class {class}").into()),
    };
    let (account, revision) = match account.split_once('@') {
        Some((account, rev)) => (account, rev.parse()?),
        None => (account, 1),
    };
    Ok(Spec {
        class,
        account: account.to_owned(),
        revision,
        path: path.to_owned(),
    })
}

fn read_bounded(path: &str) -> Result<String, Box<dyn Error>> {
    let mut text = String::new();
    std::fs::File::open(path)?
        .take(MAX_TEXT_BYTES as u64 + 1)
        .read_to_string(&mut text)?;
    Ok(text)
}

fn preview(text: &str) -> String {
    let shown: String = text.chars().take(240).collect();
    let cut = if shown.len() < text.len() {
        " [preview truncated]"
    } else {
        ""
    };
    format!("{shown:?}{cut}")
}

fn measure(m: OverlapMeasure) -> String {
    match m {
        OverlapMeasure::ShingleJaccard(r) => {
            format!("{}/{} ({:.3})", r.numerator, r.denominator, r.as_f64())
        }
        OverlapMeasure::TooShort => "too short".to_owned(),
        OverlapMeasure::Unavailable => "unavailable".to_owned(),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let specs = std::env::args()
        .skip(1)
        .map(|a| parse(&a))
        .collect::<Result<Vec<_>, _>>()?;
    let texts = specs
        .iter()
        .map(|s| read_bounded(&s.path))
        .collect::<Result<Vec<_>, _>>()?;
    let sources = specs
        .iter()
        .zip(&texts)
        .map(|(s, text)| -> Result<StatementSource<'_>, Box<dyn Error>> {
            Ok(StatementSource {
                account: OpaqueId::new(s.account.as_str())?,
                revision: s.revision,
                version: VersionId::new(format!("{}-r{}", s.account, s.revision))?,
                class: s.class,
                origin: AccountOrigin::Reported,
                exposure: ExplanationExposure::Unknown,
                attribution: s.path.as_str(),
                text: TextCapture::Complete(text),
                assertions: &[],
                derived_from: &[],
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let request = ComparisonRequest {
        coding_version: VersionId::new("uncoded")?,
        coded_by: ActorId::new("example")?,
        parameters: ComparisonParameters::default(),
        sources: &sources,
        dismissals: &[],
    };
    let started = Instant::now();
    let result = compare_statements(&request, || Ok(()))?;
    let elapsed = started.elapsed();

    let m = &result.method;
    println!(
        "method {} (shingle {} words, copy run >= {} words)",
        m.id, m.parameters.shingle_words, m.parameters.copy_run_words
    );
    println!("tokenization: {}", m.tokenization);
    for caveat in [
        m.overlap_caveat,
        m.declared_groups_caveat,
        m.candidate_groups_caveat,
    ] {
        println!("{caveat}");
    }
    for s in &result.sources {
        println!(
            "[{}] {} r{} {:?} {} words (longest {} chars), declared group {}, \
             candidate-adjusted group {}",
            s.index,
            s.account.as_str(),
            s.revision,
            s.class,
            s.word_count,
            s.longest_word_chars,
            s.declared_group,
            s.candidate_adjusted_group
        );
    }
    println!("pairs:");
    for p in &result.pairs {
        let copy = match p.copy_search {
            CopySearch::SameAccount => "same account".to_owned(),
            CopySearch::Complete => format!("{} copy candidates", p.copy_candidates),
            CopySearch::Truncated => format!("{}+ copy candidates (truncated)", p.copy_candidates),
            CopySearch::UnsegmentedText => {
                format!(
                    "{} copy candidates (unsegmented text, unreliable)",
                    p.copy_candidates
                )
            }
            CopySearch::Unavailable => "copy search unavailable".to_owned(),
        };
        println!("  {}-{}: {}; {}", p.a, p.b, measure(p.overlap), copy);
    }
    for c in &result.copy_candidates {
        println!(
            "possible copying [{}] {:?} <-> [{}] {:?}, {} words: {}",
            c.a,
            c.a_range,
            c.b,
            c.b_range,
            c.words,
            preview(c.a_text)
        );
    }
    println!(
        "{} accounts; reported groups: {} declared (upper bound if declarations hold), \
         {} candidate-adjusted (not a bound); limitations: {:?}",
        result.account_count,
        result.declared.reported_group_count,
        result.candidate_adjusted.reported_group_count,
        result.limitations
    );
    eprintln!("compared in {elapsed:?}");
    Ok(())
}
