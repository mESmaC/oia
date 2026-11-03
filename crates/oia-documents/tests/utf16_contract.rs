use oia_documents::utf16::{ByteOrder, Utf16Error, Utf16Limits, extract_utf16};
use oia_documents::{ExtractionError, ExtractionStatus, StopReason};

fn encode(text: &str, order: ByteOrder) -> Vec<u8> {
    text.encode_utf16()
        .flat_map(|unit| match order {
            ByteOrder::LittleEndian => unit.to_le_bytes(),
            ByteOrder::BigEndian => unit.to_be_bytes(),
        })
        .collect()
}

#[test]
fn decoded_passages_reencode_to_exact_source_ranges() -> Result<(), Utf16Error> {
    let original = "\u{feff}caf\u{e9}\r\n\r\n\u{1f6f0}\u{1b}[31m\nfinal\0";
    for order in [ByteOrder::LittleEndian, ByteOrder::BigEndian] {
        let input = encode(original, order);
        let preserved = input.clone();
        let limits = Utf16Limits::new(input.len(), original.len(), 4)?;
        let result = extract_utf16(&input, order, limits, || Ok(()))?;
        assert_eq!(result.byte_order(), order);
        assert_eq!(result.limits(), limits);
        assert_eq!(result.passages().len(), 4);
        let mut source_end = 0;
        for (index, passage) in result.passages().iter().enumerate() {
            assert_eq!(passage.line_number(), index + 1);
            assert_eq!(passage.source_byte_range().start, source_end);
            let reencoded = encode(passage.text(), order);
            assert_eq!(
                input.get(passage.source_byte_range()),
                Some(reencoded.as_slice())
            );
            source_end = passage.source_byte_range().end;
        }
        let reconstructed: String = result.passages().iter().map(|part| part.text()).collect();
        assert_eq!(reconstructed, original);
        assert_eq!(source_end, input.len());
        assert_eq!(result.coverage().status(), ExtractionStatus::Complete);
        assert_eq!(result.coverage().extracted_bytes(), input.len());
        assert_eq!(result.coverage().unprocessed_range(), None);
        assert_eq!(input, preserved);
        let partial = extract_utf16(&input, order, Utf16Limits::new(128, 128, 1)?, || Ok(()))?;
        assert_eq!(
            partial.coverage().status(),
            ExtractionStatus::PassageLimitReached
        );
        assert_eq!(partial.coverage().input_bytes(), input.len());
        assert_eq!(partial.coverage().extracted_bytes(), 14);
        assert_eq!(
            partial.coverage().unprocessed_range(),
            Some(14..input.len())
        );
        assert_eq!(partial.passages().len(), 1);
    }
    Ok(())
}

#[test]
fn malformed_utf16_is_rejected_even_after_the_passage_limit() -> Result<(), Utf16Error> {
    let limits = Utf16Limits::new(128, 128, 1)?;
    for order in [ByteOrder::LittleEndian, ByteOrder::BigEndian] {
        for suffix in [vec![0xd800], vec![0xdc00], vec![0xd800, 0x0041]] {
            let mut input = encode("ok\n", order);
            input.extend(suffix.into_iter().flat_map(|unit: u16| match order {
                ByteOrder::LittleEndian => unit.to_le_bytes(),
                ByteOrder::BigEndian => unit.to_be_bytes(),
            }));
            assert!(matches!(
                extract_utf16(&input, order, limits, || Ok(())),
                Err(Utf16Error::UnpairedSurrogate { byte_offset: 6, .. })
            ));
        }
    }
    assert_eq!(
        extract_utf16(&[0x41], ByteOrder::LittleEndian, limits, || Ok(())),
        Err(Utf16Error::OddByteLength { input_bytes: 1 })
    );
    Ok(())
}

#[test]
fn budgets_and_stop_requests_fail_without_publishing_partial_success() -> Result<(), Utf16Error> {
    assert!(Utf16Limits::new(0, 1, 1).is_err());
    assert!(Utf16Limits::new(1, 1, 0).is_err());
    assert_eq!(
        Utf16Limits::new(1, 0, 1),
        Err(Utf16Error::ZeroDecodedByteLimit)
    );
    let order = ByteOrder::LittleEndian;
    let input = encode("a\n\u{800}", order);
    assert!(matches!(
        extract_utf16(&input, order, Utf16Limits::new(2, 128, 1)?, || Ok(())),
        Err(Utf16Error::Extraction(
            ExtractionError::InputTooLarge { .. }
        ))
    ));
    assert_eq!(
        extract_utf16(&input, order, Utf16Limits::new(128, 4, 1)?, || Ok(())),
        Err(Utf16Error::DecodedTextTooLarge { limit_bytes: 4 })
    );
    let limits = Utf16Limits::new(128, 128, 1)?;
    for stop_at in [1, 3, 5] {
        let mut calls = 0;
        let result = extract_utf16(&input, order, limits, || {
            calls += 1;
            if calls == stop_at {
                Err(StopReason::Cancelled)
            } else {
                Ok(())
            }
        });
        assert_eq!(
            result,
            Err(Utf16Error::Extraction(ExtractionError::Stopped(
                StopReason::Cancelled
            )))
        );
    }
    assert_eq!(
        extract_utf16(&[], order, limits, || Err(StopReason::DeadlineExceeded)),
        Err(Utf16Error::Extraction(ExtractionError::Stopped(
            StopReason::DeadlineExceeded
        )))
    );
    let empty = extract_utf16(&[], order, limits, || Ok(()))?;
    assert_eq!(empty.coverage().status(), ExtractionStatus::Empty);
    assert!(empty.passages().is_empty());
    let exact = extract_utf16(&encode("a\n", order), order, limits, || Ok(()))?;
    assert_eq!(exact.coverage().status(), ExtractionStatus::Complete);
    assert_eq!(exact.passages().len(), 1);
    Ok(())
}
