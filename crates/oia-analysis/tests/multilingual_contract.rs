use oia_analysis::multilingual::{
    ComparisonError, ComparisonInput, MAX_COMPARISON_BYTES, MAX_NOTE_BYTES, MAX_PASSAGE_BYTES,
    MAX_SEGMENTS, MAX_SOURCE_BYTES, MAX_TRANSLATIONS, ReviewNote, SegmentRendering, SourceText,
    StopReason, TextReference, TranslationInput, TranslationMethod, TranslationSegment, compare,
};
use oia_domain::{ActorId, OpaqueId, VersionId};
use std::error::Error;

fn fixture() -> Result<ComparisonInput, Box<dyn Error>> {
    let first = "  café e\u{301}\r\n";
    let second = "距離不明\u{202e}\0";
    let reference = TextReference {
        source_id: OpaqueId::new("source-a")?,
        source_version: VersionId::new("source-v2")?,
        extraction_version: VersionId::new("text-v3")?,
    };
    let source = SourceText {
        reference: reference.clone(),
        language: None,
        text: format!("prefix{first}{second}suffix"),
        extraction_notes: "Page two missing; selection is only part of supplied text.".into(),
    };
    let selection = "prefix".len().."prefix".len() + first.len() + second.len();
    let human = TranslationInput {
        version: VersionId::new("translation-v1")?,
        source: reference,
        language: "en".into(),
        method: TranslationMethod::Human {
            translator: ActorId::new("author")?,
        },
        review: Some(ReviewNote {
            reviewer: ActorId::new("reviewer")?,
            review_version: VersionId::new("review-v1")?,
            note: "Interpretation disputed; retain alternative.".into(),
        }),
        limitations: "Synthetic mixed-language text; accuracy unverified.".into(),
        segments: vec![
            TranslationSegment {
                source_range: selection.start..selection.start + first.len(),
                original_quote: first.into(),
                rendering: SegmentRendering::Translated {
                    text: "  coffee shop e\u{301}\r\n".into(),
                },
            },
            TranslationSegment {
                source_range: selection.start + first.len()..selection.end,
                original_quote: second.into(),
                rendering: SegmentRendering::Omitted {
                    reason: "Untranslated supplied text.".into(),
                },
            },
        ],
    };
    let mut model = human.clone();
    model.version = VersionId::new("translation-v2")?;
    model.method = TranslationMethod::Model {
        provider: OpaqueId::new("fixture-provider")?,
        model_version: VersionId::new("fixture-model-v1")?,
        run_version: VersionId::new("fixture-run-v1")?,
    };
    model.review = None;
    model.segments[1].rendering = SegmentRendering::Translated {
        text: "Distance unknown.".into(),
    };
    Ok(ComparisonInput {
        source,
        selection,
        translations: vec![human, model],
    })
}

/// INTEL-04/A08: exact source/derivative bytes, pinned versions, omissions and disagreement
/// must survive comparison. Bad anchors or incomplete partitions must never look complete.
#[test]
fn evidence_and_coverage_survive_comparison() -> Result<(), Box<dyn Error>> {
    let input = fixture()?;
    let original = input.clone();
    let comparison = compare(&input, || Ok(()))?;
    assert_eq!(comparison.input(), &original);
    assert!(std::ptr::eq(comparison.input(), &input));
    let alternatives: Vec<_> = comparison.alternatives().collect();
    assert_eq!(alternatives.len(), 2);
    let selected = input.selection.end - input.selection.start;
    assert_eq!(alternatives[0].1.selected_bytes, selected);
    assert_eq!(
        alternatives[0].1.explicitly_omitted_bytes,
        "距離不明\u{202e}\0".len()
    );
    assert_eq!(alternatives[1].1.translated_source_bytes, selected);
    assert_eq!(alternatives[1].1.explicitly_omitted_bytes, 0);
    assert_eq!(alternatives[0].0.review, input.translations[0].review);
    assert_eq!(alternatives[1].0.method, input.translations[1].method);
    for (alternative, _) in &alternatives {
        for segment in &alternative.segments {
            assert_eq!(
                input.source.text.get(segment.source_range.clone()),
                Some(segment.original_quote.as_str())
            );
        }
    }

    let mut invalid = input.clone();
    invalid.translations[0].source.extraction_version = VersionId::new("text-v4")?;
    assert_eq!(
        compare(&invalid, || Ok(())).map(|_| ()),
        Err(ComparisonError::SourceMismatch { translation: 0 })
    );
    let mut invalid = input.clone();
    invalid.translations[1].version = input.translations[0].version.clone();
    assert_eq!(
        compare(&invalid, || Ok(())).map(|_| ()),
        Err(ComparisonError::VersionCollision { translation: 1 })
    );
    let mut invalid = input.clone();
    invalid.translations[0].segments[0].original_quote = input.translations[0].segments[0]
        .original_quote
        .replace("\r\n", "\n");
    assert_eq!(
        compare(&invalid, || Ok(())).map(|_| ()),
        Err(ComparisonError::QuoteMismatch {
            translation: 0,
            segment: 0
        })
    );

    let start = input.selection.start;
    let boundary = input.translations[0].segments[0].source_range.end;
    // Interior UTF-8 boundary, hidden gap, overlap, reversed range and oversized endpoint.
    for range in [
        start..start + 6,
        start + 1..boundary,
        start..boundary + 3,
        boundary..start,
        start..usize::MAX,
    ] {
        let mut invalid = input.clone();
        invalid.translations[0].segments[0].source_range = range;
        assert!(matches!(
            compare(&invalid, || Ok(())),
            Err(ComparisonError::InvalidPartition { .. })
                | Err(ComparisonError::QuoteMismatch { .. })
        ));
    }
    let mut invalid = input.clone();
    invalid.translations[0].segments.pop();
    assert!(matches!(
        compare(&invalid, || Ok(())),
        Err(ComparisonError::InvalidPartition { .. })
    ));
    let mut invalid = input.clone();
    invalid.selection = start..start;
    assert!(matches!(
        compare(&invalid, || Ok(())),
        Err(ComparisonError::InvalidSelection)
    ));
    let mut invalid = input;
    invalid.translations[0].segments[1].rendering = SegmentRendering::Omitted {
        reason: " \n".into(),
    };
    assert_eq!(
        compare(&invalid, || Ok(())).map(|_| ()),
        Err(ComparisonError::EmptyField("omission reason"))
    );
    Ok(())
}

/// Bounded input and cancellation, including final publication boundary, are critical
/// when supplied text is adversarial or a worker has been cancelled/superseded.
#[test]
fn limits_and_stop_signals_fail_without_a_result() -> Result<(), Box<dyn Error>> {
    let input = fixture()?;
    let mut calls = 0;
    compare(&input, || {
        calls += 1;
        Ok(())
    })?;
    for (stop_at, reason) in [
        (1, StopReason::Cancelled),
        (4, StopReason::DeadlineExceeded),
        (calls, StopReason::Superseded),
    ] {
        let mut current = 0;
        let result = compare(&input, || {
            current += 1;
            if current == stop_at {
                Err(reason)
            } else {
                Ok(())
            }
        });
        assert_eq!(result.map(|_| ()), Err(ComparisonError::Stopped(reason)));
    }
    let mut invalid = input.clone();
    invalid.source.text = "a".repeat(MAX_SOURCE_BYTES + 1);
    assert_eq!(
        compare(&invalid, || Ok(())).map(|_| ()),
        Err(ComparisonError::LimitExceeded("source text"))
    );
    let mut invalid = input.clone();
    invalid.source.text = "a".repeat(MAX_PASSAGE_BYTES + 1);
    invalid.selection = 0..invalid.source.text.len();
    assert_eq!(
        compare(&invalid, || Ok(())).map(|_| ()),
        Err(ComparisonError::LimitExceeded("selected passage"))
    );
    let mut invalid = input.clone();
    invalid.translations = vec![input.translations[0].clone(); MAX_TRANSLATIONS + 1];
    assert_eq!(
        compare(&invalid, || Ok(())).map(|_| ()),
        Err(ComparisonError::LimitExceeded("translations"))
    );
    let mut invalid = input.clone();
    invalid.translations[0].segments =
        vec![input.translations[0].segments[0].clone(); MAX_SEGMENTS + 1];
    assert_eq!(
        compare(&invalid, || Ok(())).map(|_| ()),
        Err(ComparisonError::LimitExceeded("segments"))
    );
    let mut invalid = input.clone();
    invalid.translations[0].limitations = "é".repeat(MAX_NOTE_BYTES / 2 + 1);
    assert_eq!(
        compare(&invalid, || Ok(())).map(|_| ()),
        Err(ComparisonError::LimitExceeded("translation limitations"))
    );
    let mut invalid = input;
    invalid.source.text = "a".repeat(MAX_PASSAGE_BYTES);
    invalid.selection = 0..MAX_PASSAGE_BYTES;
    invalid.translations.truncate(1);
    invalid.translations[0].segments = vec![TranslationSegment {
        source_range: invalid.selection.clone(),
        original_quote: invalid.source.text.clone(),
        rendering: SegmentRendering::Translated {
            text: "b".repeat(MAX_PASSAGE_BYTES),
        },
    }];
    for index in 1..=MAX_COMPARISON_BYTES / (2 * MAX_PASSAGE_BYTES) {
        let mut alternative = invalid.translations[0].clone();
        alternative.version = VersionId::new(format!("large-alternative-{index}"))?;
        invalid.translations.push(alternative);
    }
    assert_eq!(
        compare(&invalid, || Ok(())).map(|_| ()),
        Err(ComparisonError::LimitExceeded("comparison text"))
    );
    Ok(())
}
