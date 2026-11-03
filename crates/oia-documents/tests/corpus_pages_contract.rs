use oia_documents::corpus::{
    CorpusError, CorpusInput, CorpusLimit, CorpusLimits, CorpusSession, DocumentOutcome,
    UnavailableReason, search_corpus,
};
use oia_documents::search::{SearchError, SearchLimits, SearchMode};
use oia_documents::{ExtractionLimits, StopReason, extract_utf8};
use std::error::Error;

// Critical contracts: resumption preserves input-slot/UTF-8 anchors and cannot
// erase unavailable or unextracted coverage on reaching the terminal page.
#[test]
fn pages_preserve_anchors_gaps_and_total_budget() -> Result<(), Box<dyn Error>> {
    let bytes = "Ã© x Ã©\nÃ©".as_bytes();
    let full = extract_utf8(bytes, ExtractionLimits::new(100, 10)?, || Ok(()))?;
    let partial = extract_utf8(bytes, ExtractionLimits::new(100, 1)?, || Ok(()))?;
    let empty = extract_utf8(b"", ExtractionLimits::new(1, 1)?, || Ok(()))?;
    let inputs = [
        CorpusInput::Text(&full),
        CorpusInput::Unavailable(UnavailableReason::Missing),
        CorpusInput::Text(&partial),
        CorpusInput::Text(&empty),
        CorpusInput::Text(&full),
    ];
    for mode in [SearchMode::ExactSubstring, SearchMode::WhitespaceToken] {
        let limits = CorpusLimits::new(5, 100, SearchLimits::new(10, 2)?)?;
        let mut session = CorpusSession::new(&inputs, "Ã©", limits, mode, 10, || Ok(()))?;
        let mut hits = Vec::new();
        let mut unavailable = false;
        let mut extraction_gap = false;
        let mut exhausted = false;
        for _ in 0..10 {
            let Some(page) = session.next_page(|| Ok(()))? else {
                break;
            };
            assert!(page.match_count() <= 2);
            assert!(!page.is_complete());
            for document in page.documents() {
                match document.outcome() {
                    DocumentOutcome::Searched(result) => {
                        extraction_gap |=
                            result.coverage().extraction().unprocessed_range().is_some();
                        for hit in result.matches() {
                            assert_eq!(
                                std::str::from_utf8(bytes)?.get(hit.byte_range()),
                                Some(hit.text())
                            );
                            hits.push((document.input_index(), hit.byte_range()));
                        }
                    }
                    DocumentOutcome::Unavailable(_) => unavailable = true,
                    DocumentOutcome::MatchLimitReached(_) => {
                        return Err("unexpected skipped input".into());
                    }
                }
            }
            exhausted = page.is_exhausted();
        }
        assert!(exhausted && unavailable && extraction_gap);
        let reference = search_corpus(
            &inputs,
            "Ã©",
            CorpusLimits::new(5, 100, SearchLimits::new(10, 100)?)?,
            mode,
            || Ok(()),
        )?;
        let expected: Vec<_> = reference
            .documents()
            .iter()
            .flat_map(|d| match d.outcome() {
                DocumentOutcome::Searched(r) => r
                    .matches()
                    .iter()
                    .map(|h| (d.input_index(), h.byte_range()))
                    .collect::<Vec<_>>(),
                _ => Vec::new(),
            })
            .collect();
        assert_eq!(hits, expected);
        assert!(session.next_page(|| Ok(()))?.is_none());
    }
    let limits = CorpusLimits::new(5, 100, SearchLimits::new(10, 1)?)?;
    let mut capped = CorpusSession::new(
        &inputs,
        "Ã©",
        limits,
        SearchMode::ExactSubstring,
        1,
        || Ok(()),
    )?;
    assert!(capped.next_page(|| Ok(()))?.is_some());
    assert_eq!(
        capped.next_page(|| Ok(())),
        Err(CorpusError::PageLimitReached { maximum: 1 })
    );
    assert_eq!(capped.pages_returned(), 1);
    assert!(matches!(
        CorpusSession::new(
            &inputs,
            "Ã©",
            limits,
            SearchMode::ExactSubstring,
            0,
            || Ok(())
        ),
        Err(CorpusError::ZeroLimit(CorpusLimit::Pages))
    ));
    let mut empty_session =
        CorpusSession::new(&[], "Ã©", limits, SearchMode::ExactSubstring, 1, || Ok(()))?;
    assert!(
        empty_session
            .next_page(|| Ok(()))?
            .is_some_and(|p| p.is_complete() && p.is_exhausted())
    );
    assert!(empty_session.next_page(|| Ok(()))?.is_none());
    Ok(())
}

// Failed pages must not consume slots, hits or page budget, including failure
// after visiting multiple documents and at the final publication boundary.
#[test]
fn stopped_pages_retry_without_losing_hits() -> Result<(), Box<dyn Error>> {
    let first = extract_utf8(b"x x", ExtractionLimits::new(100, 10)?, || Ok(()))?;
    let second = extract_utf8(b"x x", ExtractionLimits::new(100, 10)?, || Ok(()))?;
    let inputs = [
        CorpusInput::Text(&first),
        CorpusInput::Unavailable(UnavailableReason::OcrRequired),
        CorpusInput::Text(&second),
    ];
    let limits = CorpusLimits::new(3, 100, SearchLimits::new(10, 3)?)?;
    let make = || {
        CorpusSession::new(&inputs, "x", limits, SearchMode::ExactSubstring, 3, || {
            Ok(())
        })
    };
    for prefix_pages in 0..=1 {
        let mut baseline = make()?;
        for _ in 0..prefix_pages {
            baseline.next_page(|| Ok(()))?;
        }
        let mut checks = 0;
        let expected = baseline.next_page(|| {
            checks += 1;
            Ok(())
        })?;
        for stop_at in 1..=checks {
            for reason in [StopReason::Cancelled, StopReason::DeadlineExceeded] {
                let mut session = make()?;
                for _ in 0..prefix_pages {
                    session.next_page(|| Ok(()))?;
                }
                let mut calls = 0;
                assert_eq!(
                    session.next_page(|| {
                        calls += 1;
                        if calls == stop_at {
                            Err(reason)
                        } else {
                            Ok(())
                        }
                    }),
                    Err(CorpusError::Search(SearchError::Stopped(reason)))
                );
                assert_eq!(session.pages_returned(), prefix_pages);
                assert_eq!(session.next_page(|| Ok(()))?, expected);
            }
        }
    }
    // Admission rejects oversized selection/bytes and validates empty queries.
    assert!(matches!(
        CorpusSession::new(
            &inputs,
            "x",
            CorpusLimits::new(2, 100, SearchLimits::new(10, 3)?)?,
            SearchMode::ExactSubstring,
            3,
            || Ok(())
        ),
        Err(CorpusError::LimitExceeded {
            limit: CorpusLimit::Documents,
            ..
        })
    ));
    assert!(matches!(
        CorpusSession::new(
            &inputs,
            "x",
            CorpusLimits::new(3, 5, SearchLimits::new(10, 3)?)?,
            SearchMode::ExactSubstring,
            3,
            || Ok(())
        ),
        Err(CorpusError::LimitExceeded {
            limit: CorpusLimit::InputBytes,
            ..
        })
    ));
    assert!(matches!(
        CorpusSession::new(&[], "", limits, SearchMode::ExactSubstring, 3, || Ok(())),
        Err(CorpusError::Search(SearchError::EmptyQuery))
    ));
    Ok(())
}
