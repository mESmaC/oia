use std::error::Error;

use oia_documents::search::{
    SearchError, SearchLimits, SearchMode, SearchSession, find_text, find_token,
};
use oia_documents::{ExtractionLimits, StopReason, extract_utf8};

// CASE-05/DOC-04: pagination must preserve anchors, ordering, non-overlap and
// missing extraction coverage, including UTF-8 and token boundary cases.
#[test]
fn pages_preserve_one_shot_hits_and_coverage() -> Result<(), Box<dyn Error>> {
    for input in ["", "éé éé\r\néééé\nlast éé", "none\n", "éé"] {
        for max_passages in [1, 10] {
            let extraction = extract_utf8(
                input.as_bytes(),
                ExtractionLimits::new(100, max_passages)?,
                || Ok(()),
            )?;
            for mode in [SearchMode::ExactSubstring, SearchMode::WhitespaceToken] {
                let all_limits = SearchLimits::new(10, 100)?;
                let expected = match mode {
                    SearchMode::ExactSubstring => {
                        find_text(&extraction, "éé", all_limits, || Ok(()))?
                    }
                    SearchMode::WhitespaceToken => {
                        find_token(&extraction, "éé", all_limits, || Ok(()))?
                    }
                };
                for size in [1, 2, 3] {
                    let mut session =
                        SearchSession::new(&extraction, "éé", SearchLimits::new(10, size)?, mode)?;
                    let mut hits = Vec::new();
                    let mut last_coverage = None;
                    let mut pages = 0;
                    while let Some(page) = session.next_page(|| Ok(()))? {
                        pages += 1;
                        assert!(pages <= input.len() + 1);
                        assert!(page.matches().len() <= size);
                        for hit in page.matches() {
                            assert_eq!(input.get(hit.byte_range()), Some(hit.text()));
                        }
                        hits.extend_from_slice(page.matches());
                        last_coverage = Some(page.coverage().clone());
                    }
                    assert_eq!(hits, expected.matches());
                    assert_eq!(last_coverage.as_ref(), Some(expected.coverage()));
                    assert!(session.next_page(|| Ok(()))?.is_none());
                }
            }
        }
    }
    Ok(())
}

// A failed page must neither publish hits nor skip them on retry, even if
// cancellation arrives at the final publication boundary.
#[test]
fn stopped_pages_do_not_advance_cursor() -> Result<(), Box<dyn Error>> {
    let extraction = extract_utf8(
        b"one oneish one\none",
        ExtractionLimits::new(100, 10)?,
        || Ok(()),
    )?;
    let limits = SearchLimits::new(10, 1)?;
    for mode in [SearchMode::ExactSubstring, SearchMode::WhitespaceToken] {
        let mut reference = SearchSession::new(&extraction, "one", limits, mode)?;
        reference.next_page(|| Ok(()))?;
        let mut calls = 0;
        let expected = reference.next_page(|| {
            calls += 1;
            Ok(())
        })?;
        for stop_at in 1..=calls {
            for reason in [StopReason::Cancelled, StopReason::DeadlineExceeded] {
                let mut session = SearchSession::new(&extraction, "one", limits, mode)?;
                session.next_page(|| Ok(()))?;
                let mut checked = 0;
                assert_eq!(
                    session.next_page(|| {
                        checked += 1;
                        if checked == stop_at {
                            Err(reason)
                        } else {
                            Ok(())
                        }
                    }),
                    Err(SearchError::Stopped(reason))
                );
                assert_eq!(session.next_page(|| Ok(()))?, expected);
            }
        }
    }
    assert!(matches!(
        SearchSession::new(&extraction, "", limits, SearchMode::ExactSubstring),
        Err(SearchError::EmptyQuery)
    ));
    assert!(matches!(
        SearchSession::new(&extraction, "one two", limits, SearchMode::WhitespaceToken),
        Err(SearchError::WhitespaceInToken)
    ));
    Ok(())
}
