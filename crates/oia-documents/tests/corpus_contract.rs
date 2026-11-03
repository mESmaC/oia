use std::error::Error;

use oia_documents::corpus::{
    CorpusError, CorpusInput, CorpusLimit, CorpusLimits, DocumentOutcome, UnavailableReason,
    search_corpus,
};
use oia_documents::search::{SearchError, SearchLimits, SearchMode};
use oia_documents::{ExtractionLimits, StopReason, extract_utf8};

// A03: identical sources retain separate slots/anchors, while missing input,
// extraction gaps and the global hit cap must remain visible.
#[test]
fn corpus_keeps_input_attribution_and_all_coverage_gaps() -> Result<(), Box<dyn Error>> {
    let original = include_bytes!("../../../fixtures/documents/a03/originals/report-v1.txt");
    let text = std::str::from_utf8(original)?;
    let extraction = extract_utf8(original, ExtractionLimits::new(4096, 100)?, || Ok(()))?;
    let partial = extract_utf8(original, ExtractionLimits::new(4096, 1)?, || Ok(()))?;
    let empty = extract_utf8(b"", ExtractionLimits::new(1, 1)?, || Ok(()))?;
    let inputs = [
        CorpusInput::Text(&extraction),
        CorpusInput::Text(&extraction),
        CorpusInput::Unavailable(UnavailableReason::OcrRequired),
        CorpusInput::Unavailable(UnavailableReason::Missing),
        CorpusInput::Text(&partial),
        CorpusInput::Text(&empty),
    ];
    let limits = CorpusLimits::new(6, original.len() * 3, SearchLimits::new(100, 100)?)?;
    let result = search_corpus(
        &inputs,
        "two lights",
        limits,
        SearchMode::ExactSubstring,
        || Ok(()),
    )?;
    assert!(!result.is_complete());
    assert_eq!(result.known_input_bytes(), original.len() * 3);
    assert_eq!(result.documents().len(), inputs.len());
    let mut anchored_inputs = Vec::new();
    for (index, document) in result.documents().iter().enumerate() {
        assert_eq!(document.input_index(), index);
        if let DocumentOutcome::Searched(search) = document.outcome() {
            for hit in search.matches() {
                assert_eq!(text.get(hit.byte_range()), Some(hit.text()));
                assert_eq!(hit.text(), "two lights");
                anchored_inputs.push(index);
            }
        }
    }
    assert_eq!(anchored_inputs, [0, 1]);
    assert_eq!(result.match_count(), 2);
    assert!(matches!(
        result.documents().get(2).map(|d| d.outcome()),
        Some(DocumentOutcome::Unavailable(UnavailableReason::OcrRequired))
    ));
    assert!(matches!(
        result.documents().get(3).map(|d| d.outcome()),
        Some(DocumentOutcome::Unavailable(UnavailableReason::Missing))
    ));
    assert!(matches!(result.documents().get(4).map(|d| d.outcome()),
        Some(DocumentOutcome::Searched(search)) if search.matches().is_empty()
            && search.coverage().unsearched_extracted_range().is_none()
            && search.coverage().extraction().unprocessed_range().is_some()));
    let capped = search_corpus(
        &inputs,
        "two lights",
        CorpusLimits::new(6, original.len() * 3, SearchLimits::new(100, 1)?)?,
        SearchMode::ExactSubstring,
        || Ok(()),
    )?;
    assert_eq!(capped.match_count(), 1);
    assert!(!capped.is_complete());
    assert!(matches!(capped.documents().first().map(|d| d.outcome()),
        Some(DocumentOutcome::Searched(search)) if search.coverage().unsearched_extracted_range().is_some()));
    assert!(matches!(capped.documents().get(1).map(|d| d.outcome()),
        Some(DocumentOutcome::MatchLimitReached(coverage)) if coverage.unprocessed_range().is_none()));
    assert!(matches!(capped.documents().get(4).map(|d| d.outcome()),
        Some(DocumentOutcome::MatchLimitReached(coverage)) if coverage.unprocessed_range().is_some()));
    assert_eq!(capped.documents().get(3), result.documents().get(3));
    assert!(matches!(capped.documents().last().map(|d| d.outcome()),
        Some(DocumentOutcome::Searched(search)) if search.coverage().is_complete()));
    Ok(())
}

// CASE-05/DOC-05: total-work limits fail closed; stopped results never escape
// after earlier documents succeeded, including at final publication.
#[test]
fn corpus_bounds_and_stops_apply_to_the_whole_request() -> Result<(), Box<dyn Error>> {
    let original = "éé ééx\néé";
    let extraction = extract_utf8(original.as_bytes(), ExtractionLimits::new(100, 10)?, || {
        Ok(())
    })?;
    let inputs = [
        CorpusInput::Text(&extraction),
        CorpusInput::Text(&extraction),
    ];
    let search = SearchLimits::new(10, 100)?;
    let limits = CorpusLimits::new(2, original.len() * 2, search)?;
    for (limit, maximum, limits) in [
        (
            CorpusLimit::Documents,
            1,
            CorpusLimits::new(1, 100, search)?,
        ),
        (
            CorpusLimit::InputBytes,
            original.len(),
            CorpusLimits::new(2, original.len(), search)?,
        ),
    ] {
        assert_eq!(
            search_corpus(&inputs, "éé", limits, SearchMode::WhitespaceToken, || Ok(
                ()
            )),
            Err(CorpusError::LimitExceeded { limit, maximum })
        );
    }
    assert_eq!(
        search_corpus(&[], "", limits, SearchMode::ExactSubstring, || Ok(())),
        Err(CorpusError::Search(SearchError::EmptyQuery))
    );
    assert_eq!(
        search_corpus(
            &[],
            "two words",
            limits,
            SearchMode::WhitespaceToken,
            || Ok(())
        ),
        Err(CorpusError::Search(SearchError::WhitespaceInToken))
    );
    assert!(
        search_corpus(&[], "éé", limits, SearchMode::WhitespaceToken, || Ok(()))?.is_complete()
    );
    let mut calls = 0;
    let result = search_corpus(&inputs, "éé", limits, SearchMode::WhitespaceToken, || {
        calls += 1;
        Ok(())
    })?;
    assert_eq!(result.match_count(), 4);
    assert!(result.is_complete());
    for stop_at in 1..=calls {
        for reason in [StopReason::Cancelled, StopReason::DeadlineExceeded] {
            let mut checked = 0;
            assert_eq!(
                search_corpus(&inputs, "éé", limits, SearchMode::WhitespaceToken, || {
                    checked += 1;
                    if checked == stop_at {
                        Err(reason)
                    } else {
                        Ok(())
                    }
                }),
                Err(CorpusError::Search(SearchError::Stopped(reason)))
            );
        }
    }
    Ok(())
}
