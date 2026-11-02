use oia_documents::{
    ExtractionError, ExtractionLimits, ExtractionStatus, Limit, StopReason, extract_utf8,
};

#[test]
fn passages_preserve_original_bytes_and_exact_anchors() -> Result<(), ExtractionError> {
    let original = "\u{feff}caf\u{e9}\r\n\r\nrun tool\u{1b}[31m\nfinal\u{c}";
    let input = original.as_bytes().to_vec();
    let extraction = extract_utf8(&input, ExtractionLimits::new(128, 8)?, || Ok(()))?;
    let expected = [
        (0..10, 1, "\u{feff}caf\u{e9}\r\n"),
        (10..12, 2, "\r\n"),
        (12..26, 3, "run tool\u{1b}[31m\n"),
        (26..32, 4, "final\u{c}"),
    ];
    assert_eq!(extraction.passages().len(), expected.len());
    for (passage, (range, line_number, text)) in extraction.passages().iter().zip(expected) {
        assert_eq!(passage.byte_range(), range);
        assert_eq!(passage.line_number(), line_number);
        assert_eq!(passage.text(), text);
        assert_eq!(input.get(passage.byte_range()), Some(text.as_bytes()));
        assert_eq!(passage.text().as_ptr(), text_pointer(&input, range.start));
    }
    let reconstructed: String = extraction
        .passages()
        .iter()
        .map(|passage| passage.text())
        .collect();
    assert_eq!(reconstructed, original);
    assert_eq!(input, original.as_bytes());
    assert_eq!(extraction.coverage().status(), ExtractionStatus::Complete);
    assert_eq!(extraction.coverage().input_bytes(), 32);
    assert_eq!(extraction.coverage().extracted_bytes(), 32);
    assert_eq!(extraction.coverage().unprocessed_range(), None);
    Ok(())
}

fn text_pointer(input: &[u8], offset: usize) -> *const u8 {
    input.as_ptr().wrapping_add(offset)
}

#[test]
fn coverage_and_failures_do_not_masquerade_as_complete_text() -> Result<(), ExtractionError> {
    let input = b"one\n\ntwo";
    let limits = ExtractionLimits::new(input.len(), 2)?;
    let partial = extract_utf8(input, limits, || Ok(()))?;
    assert_eq!(
        partial.coverage().status(),
        ExtractionStatus::PassageLimitReached
    );
    assert_eq!(partial.coverage().extracted_bytes(), 5);
    assert_eq!(partial.coverage().unprocessed_range(), Some(5..8));
    assert_eq!(partial.limits(), limits);
    let complete = extract_utf8(input, ExtractionLimits::new(input.len(), 3)?, || Ok(()))?;
    assert_eq!(complete.coverage().status(), ExtractionStatus::Complete);
    let empty = extract_utf8(b"", limits, || Ok(()))?;
    assert_eq!(empty.coverage().status(), ExtractionStatus::Empty);
    assert!(empty.passages().is_empty());
    assert_eq!(empty.coverage().unprocessed_range(), None);
    assert_eq!(
        ExtractionLimits::new(0, 1),
        Err(ExtractionError::ZeroLimit(Limit::InputBytes))
    );
    assert_eq!(
        ExtractionLimits::new(1, 0),
        Err(ExtractionError::ZeroLimit(Limit::Passages))
    );
    assert_eq!(
        extract_utf8(input, ExtractionLimits::new(3, 1)?, || Ok(())),
        Err(ExtractionError::InputTooLarge {
            actual_bytes: 8,
            limit_bytes: 3
        })
    );
    assert_eq!(
        extract_utf8(
            &[b'a', b'\n', 0xff],
            ExtractionLimits::new(3, 1)?,
            || Ok(())
        ),
        Err(ExtractionError::MalformedUtf8 {
            valid_up_to: 2,
            error_len: Some(1)
        })
    );
    assert_eq!(
        extract_utf8(&[0xe2, 0x82], limits, || Ok(())),
        Err(ExtractionError::MalformedUtf8 {
            valid_up_to: 0,
            error_len: None
        })
    );
    Ok(())
}

#[test]
fn stopped_work_cannot_return_even_a_late_complete_or_partial_result() -> Result<(), ExtractionError>
{
    for reason in [StopReason::Cancelled, StopReason::DeadlineExceeded] {
        for stop_at in 1..=4 {
            let mut checks = 0;
            let outcome = extract_utf8(b"one\ntwo", ExtractionLimits::new(7, 2)?, || {
                checks += 1;
                if checks == stop_at {
                    Err(reason)
                } else {
                    Ok(())
                }
            });
            assert_eq!(outcome, Err(ExtractionError::Stopped(reason)));
        }
        let mut checks = 0;
        let partial = extract_utf8(b"one\ntwo", ExtractionLimits::new(7, 1)?, || {
            checks += 1;
            if checks == 3 { Err(reason) } else { Ok(()) }
        });
        assert_eq!(partial, Err(ExtractionError::Stopped(reason)));
    }
    Ok(())
}
