//! Read-only inspection of supplied translations. No translator is executed.

use oia_analysis::multilingual::{
    ComparisonInput, MAX_PASSAGE_BYTES, ReviewNote, SegmentRendering, SourceText, TextReference,
    TranslationInput, TranslationMethod, TranslationSegment, compare,
};
use oia_domain::{ActorId, OpaqueId, VersionId};
use std::{
    error::Error,
    ffi::OsString,
    fmt,
    fs::File,
    io::{self, Read, Write},
    path::Path,
    process::ExitCode,
};

// ASCII escapes make controls, bidi formatting, quotes and field boundaries inspectable.
struct Quoted<'a>(&'a str);

impl fmt::Display for Quoted<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\"{}\"", self.0.escape_default())
    }
}

fn synthetic() -> Result<ComparisonInput, Box<dyn Error>> {
    let first = "Vi dos luces sobre el café.\r\n";
    let second = "No pude estimar la distancia.";
    let reference = TextReference {
        source_id: OpaqueId::new("synthetic-account")?,
        source_version: VersionId::new("synthetic-source-v1")?,
        extraction_version: VersionId::new("synthetic-text-v1")?,
    };
    let source = SourceText {
        reference: reference.clone(),
        language: Some("es (declared)".into()),
        text: format!("{first}{second}"),
        extraction_notes: "Synthetic text, not an observed event or evaluated translation corpus."
            .into(),
    };
    let full = TranslationInput {
        version: VersionId::new("human-translation-v1")?,
        source: reference.clone(),
        language: "en (declared)".into(),
        method: TranslationMethod::Human {
            translator: ActorId::new("fixture-author")?,
        },
        review: Some(ReviewNote {
            reviewer: ActorId::new("fixture-reviewer")?,
            review_version: VersionId::new("fixture-review-v1")?,
            note: "Synthetic review: preserves the reported inability to estimate distance.".into(),
        }),
        limitations: "Supplied example; no independent linguistic evaluation.".into(),
        segments: vec![
            TranslationSegment {
                source_range: 0..first.len(),
                original_quote: first.into(),
                rendering: SegmentRendering::Translated {
                    text: "I saw two lights above the café.\r\n".into(),
                },
            },
            TranslationSegment {
                source_range: first.len()..source.text.len(),
                original_quote: second.into(),
                rendering: SegmentRendering::Translated {
                    text: "I could not estimate the distance.".into(),
                },
            },
        ],
    };
    let partial = TranslationInput {
        version: VersionId::new("simulated-model-translation-v1")?,
        source: reference,
        language: "en (declared)".into(),
        method: TranslationMethod::Model {
            provider: OpaqueId::new("synthetic-provider")?,
            model_version: VersionId::new("synthetic-model-v1")?,
            run_version: VersionId::new("synthetic-run-v1")?,
        },
        review: None,
        limitations:
            "Simulated model output; no model was run. Alternative wording may change meaning."
                .into(),
        segments: vec![
            TranslationSegment {
                source_range: 0..first.len(),
                original_quote: first.into(),
                rendering: SegmentRendering::Translated {
                    text: "I saw two lights over the coffee shop.".into(),
                },
            },
            TranslationSegment {
                source_range: first.len()..source.text.len(),
                original_quote: second.into(),
                rendering: SegmentRendering::Omitted {
                    reason: "Not translated in this supplied alternative.".into(),
                },
            },
        ],
    };
    Ok(ComparisonInput {
        selection: 0..source.text.len(),
        source,
        translations: vec![full, partial],
    })
}

fn read_text(path: &Path) -> Result<String, Box<dyn Error>> {
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(MAX_PASSAGE_BYTES + 1)?;
    File::open(path)?
        .take((MAX_PASSAGE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_PASSAGE_BYTES {
        return Err("file exceeds the 65,536-byte whole-passage example limit".into());
    }
    Ok(String::from_utf8(bytes)?)
}

fn argument(value: &OsString) -> Result<&str, Box<dyn Error>> {
    value
        .to_str()
        .ok_or_else(|| "metadata arguments must be UTF-8".into())
}

fn input_from_args(args: &[OsString]) -> Result<ComparisonInput, Box<dyn Error>> {
    match args {
        [] => synthetic(),
        [flag, original, translated, source_id, source_version, extraction_version,
            translation_version, language, translator] if flag == "--files" => {
            let text = read_text(Path::new(original))?;
            let translated_text = read_text(Path::new(translated))?;
            let reference = TextReference {
                source_id: OpaqueId::new(argument(source_id)?)?,
                source_version: VersionId::new(argument(source_version)?)?,
                extraction_version: VersionId::new(argument(extraction_version)?)?,
            };
            let translation = TranslationInput {
                version: VersionId::new(argument(translation_version)?)?,
                source: reference.clone(),
                language: argument(language)?.into(),
                method: TranslationMethod::Human { translator: ActorId::new(argument(translator)?)? },
                review: None,
                limitations: "Caller declares a human translation; attribution and accuracy unverified. Whole-file mapping may conceal semantic omissions.".into(),
                segments: vec![TranslationSegment {
                    source_range: 0..text.len(),
                    original_quote: text.clone(),
                    rendering: SegmentRendering::Translated { text: translated_text },
                }],
            };
            Ok(ComparisonInput {
                selection: 0..text.len(),
                source: SourceText {
                    reference,
                    language: None,
                    text,
                    extraction_notes: "Caller-supplied UTF-8 file; version binding and original-document coverage unverified.".into(),
                },
                translations: vec![translation],
            })
        }
        _ => Err("usage: inspect-translation [--files ORIGINAL TRANSLATION SOURCE_ID SOURCE_VERSION EXTRACTION_VERSION TRANSLATION_VERSION TARGET_LANGUAGE HUMAN_TRANSLATOR_ID]".into()),
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let input = input_from_args(&std::env::args_os().skip(1).collect::<Vec<_>>())?;
    let comparison = compare(&input, || Ok(()))?;
    let mut output = io::stdout().lock();
    writeln!(
        output,
        "SUPPLIED TRANSLATIONS — structural comparison, not accuracy verification"
    )?;
    writeln!(
        output,
        "Text fields use quoted ASCII escapes; original UTF-8 bytes remain unchanged."
    )?;
    writeln!(
        output,
        "Source: {} / {} / {}",
        input.source.reference.source_id,
        input.source.reference.source_version,
        input.source.reference.extraction_version
    )?;
    match &input.source.language {
        Some(language) => writeln!(output, "Declared source language: {}", Quoted(language))?,
        None => writeln!(output, "Declared source language: unknown")?,
    }
    writeln!(
        output,
        "Extraction limitations: {}",
        Quoted(&input.source.extraction_notes)
    )?;
    writeln!(
        output,
        "Selected byte range: {:?}; supplied extraction: {} bytes",
        input.selection,
        input.source.text.len()
    )?;
    for (translation, coverage) in comparison.alternatives() {
        writeln!(
            output,
            "\nAlternative: {}; declared target language: {}",
            translation.version,
            Quoted(&translation.language)
        )?;
        // Method fields are validated opaque identifiers, not arbitrary display text.
        writeln!(
            output,
            "Declared method (unverified): {:?}",
            translation.method
        )?;
        writeln!(
            output,
            "Coverage: {} source bytes mapped; {} explicitly omitted; {} selected",
            coverage.translated_source_bytes,
            coverage.explicitly_omitted_bytes,
            coverage.selected_bytes
        )?;
        writeln!(output, "Limitations: {}", Quoted(&translation.limitations))?;
        match &translation.review {
            Some(review) => writeln!(
                output,
                "Supplied review (unverified): {} / {}: {}",
                review.reviewer,
                review.review_version,
                Quoted(&review.note)
            )?,
            None => writeln!(output, "Review: unreviewed; no review supplied")?,
        }
        for segment in &translation.segments {
            writeln!(
                output,
                "  Original {:?}: {}",
                segment.source_range,
                Quoted(&segment.original_quote)
            )?;
            match &segment.rendering {
                SegmentRendering::Translated { text } => {
                    writeln!(output, "  Translation (derivative): {}", Quoted(text))?
                }
                SegmentRendering::Omitted { reason } => {
                    writeln!(output, "  OMITTED: {}", Quoted(reason))?
                }
            }
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "inspect-translation: {error}");
            ExitCode::FAILURE
        }
    }
}
