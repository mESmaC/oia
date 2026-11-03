use std::error::Error;

use oia_documents::citations::{
    Citation, CitationError, CitationLimits, CitationProblem, validate_citations,
};
use oia_documents::{ExtractionLimits, StopReason, extract_utf8};

// DOC-03/04: fabricated or relocated quotations cannot pass exact-byte checks;
// partial extraction and UTF-8 offsets cannot be repaired into valid citations.
#[test]
fn exact_anchors_and_unavailable_text_remain_distinct() -> Result<(), Box<dyn Error>> {
    let bytes = "é\r\nsource assertion\nmissing\n".as_bytes();
    let extraction = extract_utf8(bytes, ExtractionLimits::new(100, 2)?, || Ok(()))?;
    let limits = CitationLimits::new(10, 100)?;
    let valid = [Citation {
        byte_range: 0..10,
        quote: "é\r\nsource",
    }];
    validate_citations(&extraction, &valid, limits, || Ok(()))?;
    for (range, quote, problem) in [
        (1..3, "xx", CitationProblem::InvalidCharacterBoundary),
        (4..10, "SOURCE", CitationProblem::QuoteMismatch),
        (4..10, "short", CitationProblem::QuoteLengthMismatch),
        (21..28, "missing", CitationProblem::NotExtracted),
        (0..usize::MAX, "", CitationProblem::OutsideInput),
        (4..4, "", CitationProblem::EmptyOrReversedRange),
    ] {
        assert_eq!(
            validate_citations(
                &extraction,
                &[Citation {
                    byte_range: range,
                    quote
                }],
                limits,
                || Ok(())
            ),
            Err(CitationError::Invalid {
                citation_index: 0,
                problem
            })
        );
    }
    let fixture = include_bytes!("../../../fixtures/documents/a03/originals/report-v1.txt");
    let full = extract_utf8(fixture, ExtractionLimits::new(4096, 100)?, || Ok(()))?;
    validate_citations(
        &full,
        &[Citation {
            byte_range: 121..128,
            quote: "lights.",
        }],
        limits,
        || Ok(()),
    )?;
    assert_eq!(bytes, "é\r\nsource assertion\nmissing\n".as_bytes());
    Ok(())
}

// DOC-05: no usable partial validation after a late stop; repeated quotes count
// against total work, including comparisons longer than one control chunk.
#[test]
fn budgets_and_every_control_boundary_reject_the_batch() -> Result<(), Box<dyn Error>> {
    let text = "x".repeat(9000);
    let extraction = extract_utf8(
        text.as_bytes(),
        ExtractionLimits::new(10_000, 1)?,
        || Ok(()),
    )?;
    let citations = [
        Citation {
            byte_range: 0..9000,
            quote: &text,
        },
        Citation {
            byte_range: 0..9000,
            quote: &text,
        },
    ];
    assert_eq!(
        validate_citations(
            &extraction,
            &citations,
            CitationLimits::new(1, 18000)?,
            || Ok(())
        ),
        Err(CitationError::TooManyCitations)
    );
    assert_eq!(
        validate_citations(
            &extraction,
            &citations,
            CitationLimits::new(2, 17999)?,
            || Ok(())
        ),
        Err(CitationError::QuoteBudgetExceeded)
    );
    let limits = CitationLimits::new(2, 18000)?;
    let mut checks = 0;
    validate_citations(&extraction, &citations, limits, || {
        checks += 1;
        Ok(())
    })?;
    for stop_at in 1..=checks {
        let mut called = 0;
        assert_eq!(
            validate_citations(&extraction, &citations, limits, || {
                called += 1;
                if called == stop_at {
                    Err(StopReason::Cancelled)
                } else {
                    Ok(())
                }
            }),
            Err(CitationError::Stopped(StopReason::Cancelled))
        );
    }
    validate_citations(&extraction, &citations, limits, || Ok(()))?;
    validate_citations(&extraction, &[], limits, || Ok(()))?;
    Ok(())
}
