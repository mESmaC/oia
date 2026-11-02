use oia_png_privacy::{
    Coverage, Handling, Limit, Limits, PngError, Selection, StopReason, clean, inspect,
};
use std::error::Error;

fn limits() -> Limits {
    Limits {
        input_bytes: 1_048_576,
        output_bytes: 1_048_576,
        chunk_bytes: 262_144,
        chunks: 128,
        metadata_bytes: 65_536,
        width: 4096,
        height: 4096,
        pixels: 16_777_216,
    }
}

fn fixture() -> Result<Vec<u8>, Box<dyn Error>> {
    decode(include_str!("fixtures/rgba.hex"))
}

fn decode(hex: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    hex.trim()
        .as_bytes()
        .chunks(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair)?;
            Ok(u8::from_str_radix(text, 16)?)
        })
        .collect()
}

fn all_metadata() -> Selection {
    Selection {
        text: true,
        modification_time: true,
    }
}

fn with_header(input: &[u8], header: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = input.get(..8).ok_or("signature")?.to_vec();
    bytes.extend_from_slice(&decode(header)?);
    bytes.extend_from_slice(input.get(33..).ok_or("body")?);
    Ok(bytes)
}

#[test]
fn selected_removal_preserves_every_retained_byte() -> Result<(), Box<dyn Error>> {
    let input = fixture()?;
    let snapshot = input.clone();
    let original = inspect(&input, limits(), || Ok(()))?;
    assert_eq!(original.coverage, Coverage::CompleteContainerProfile);
    assert_eq!((original.width, original.height), (1, 1));
    let cleaned = clean(&input, all_metadata(), limits(), || Ok(()))?;
    assert_eq!(cleaned.removed_chunks(), 3);
    assert_eq!(input, snapshot);
    let mut expected = input.get(..8).ok_or("signature")?.to_vec();
    for chunk in original
        .chunks
        .iter()
        .filter(|chunk| chunk.handling == Handling::Retained)
    {
        expected.extend_from_slice(
            input
                .get(chunk.wire_range.clone())
                .ok_or("retained region")?,
        );
    }
    assert_eq!(cleaned.bytes(), expected);
    assert_eq!(
        cleaned.inspection(),
        &inspect(cleaned.bytes(), limits(), || Ok(()))?
    );
    let text_only = clean(
        &input,
        Selection {
            text: true,
            modification_time: false,
        },
        limits(),
        || Ok(()),
    )?;
    assert_eq!(text_only.removed_chunks(), 2);
    assert!(
        text_only
            .inspection()
            .chunks
            .iter()
            .any(|chunk| chunk.kind == *b"tIME")
    );
    assert!(!format!("{text_only:?}").contains("Synthetic Alice"));
    let no_op = clean(
        &input,
        Selection {
            text: false,
            modification_time: false,
        },
        limits(),
        || Ok(()),
    )?;
    assert_eq!(no_op.bytes(), input);
    Ok(())
}

#[test]
fn opaque_and_animated_regions_cannot_yield_cleaned_output() -> Result<(), Box<dyn Error>> {
    let input = fixture()?;
    let interlaced = with_header(&input, "0000000d49484452000000010000000108060000016812f41f")?;
    let report = inspect(&interlaced, limits(), || Ok(()))?;
    assert!(report.unsupported_image_parameters);
    assert_eq!(report.coverage, Coverage::Partial);
    assert_eq!(
        clean(&interlaced, all_metadata(), limits(), || Ok(())).err(),
        Some(PngError::UnsupportedProfile)
    );
    for wire in [
        "00000006767041676f7061717565fa113941",
        "000000086163544c0000000100000000b42de9a0",
    ] {
        let mut opaque = input.get(..33).ok_or("header")?.to_vec();
        opaque.extend_from_slice(&decode(wire)?);
        opaque.extend_from_slice(input.get(33..).ok_or("body")?);
        let report = inspect(&opaque, limits(), || Ok(()))?;
        assert_eq!(report.coverage, Coverage::Partial);
        assert!(
            report
                .chunks
                .iter()
                .any(|chunk| chunk.handling == Handling::Unsupported)
        );
        assert_eq!(
            clean(&opaque, all_metadata(), limits(), || Ok(())).err(),
            Some(PngError::UnsupportedProfile)
        );
    }
    Ok(())
}

#[test]
fn truncation_checksums_order_and_trailing_bytes_fail_closed() -> Result<(), Box<dyn Error>> {
    let input = fixture()?;
    let invalid_depth = with_header(&input, "0000000d49484452000000010000000100060000002f658f48")?;
    assert!(matches!(
        inspect(&invalid_depth, limits(), || Ok(())),
        Err(PngError::Malformed {
            reason: "invalid image parameters",
            ..
        })
    ));
    for end in 0..input.len() {
        assert!(inspect(input.get(..end).ok_or("prefix")?, limits(), || Ok(())).is_err());
    }
    let mut corrupted = input.clone();
    *corrupted.get_mut(29).ok_or("header CRC")? ^= 1;
    assert!(matches!(
        inspect(&corrupted, limits(), || Ok(())),
        Err(PngError::Malformed { .. })
    ));
    let mut trailing = input.clone();
    trailing.push(0);
    assert!(inspect(&trailing, limits(), || Ok(())).is_err());
    let report = inspect(&input, limits(), || Ok(()))?;
    let mut nonconsecutive = input.get(..8).ok_or("signature")?.to_vec();
    let interloper = report
        .chunks
        .iter()
        .find(|chunk| chunk.kind == *b"tIME")
        .ok_or("time")?;
    let mut inserted = false;
    for chunk in &report.chunks {
        if chunk.kind == *b"tIME" {
            continue;
        }
        nonconsecutive.extend_from_slice(input.get(chunk.wire_range.clone()).ok_or("chunk")?);
        if chunk.kind == *b"IDAT" && !inserted {
            nonconsecutive.extend_from_slice(
                input
                    .get(interloper.wire_range.clone())
                    .ok_or("time wire")?,
            );
            inserted = true;
        }
    }
    assert!(matches!(
        inspect(&nonconsecutive, limits(), || Ok(())),
        Err(PngError::Malformed {
            reason: "IDAT chunks must be consecutive",
            ..
        })
    ));
    Ok(())
}

#[test]
fn resource_budgets_reject_before_publication() -> Result<(), Box<dyn Error>> {
    let input = fixture()?;
    let larger_dimensions =
        with_header(&input, "0000000d494844520000000200000002080600000072b60d24")?;
    for dimension in [Limit::Width, Limit::Height, Limit::Pixels] {
        let mut budget = limits();
        match dimension {
            Limit::Width => budget.width = 1,
            Limit::Height => budget.height = 1,
            Limit::Pixels => budget.pixels = 1,
            _ => return Err("unexpected fixture dimension".into()),
        }
        assert_eq!(
            inspect(&larger_dimensions, budget, || Ok(())).err(),
            Some(PngError::ResourceLimit(dimension))
        );
    }
    let mut budget = limits();
    budget.input_bytes = input.len() - 1;
    assert_eq!(
        inspect(&input, budget, || Ok(())).err(),
        Some(PngError::ResourceLimit(Limit::InputBytes))
    );
    budget = limits();
    budget.chunk_bytes = 12;
    assert_eq!(
        inspect(&input, budget, || Ok(())).err(),
        Some(PngError::ResourceLimit(Limit::ChunkBytes))
    );
    budget = limits();
    budget.chunks = 9;
    assert_eq!(
        inspect(&input, budget, || Ok(())).err(),
        Some(PngError::ResourceLimit(Limit::Chunks))
    );
    budget = limits();
    budget.metadata_bytes = 14;
    assert_eq!(
        inspect(&input, budget, || Ok(())).err(),
        Some(PngError::ResourceLimit(Limit::MetadataBytes))
    );
    budget = limits();
    budget.output_bytes = 8;
    assert_eq!(
        clean(&input, all_metadata(), budget, || Ok(())).err(),
        Some(PngError::ResourceLimit(Limit::OutputBytes))
    );
    budget = limits();
    budget.pixels = 0;
    assert_eq!(
        inspect(&input, budget, || Ok(())).err(),
        Some(PngError::ZeroLimit(Limit::Pixels))
    );
    Ok(())
}

#[test]
fn cancellation_and_deadlines_never_return_a_candidate() -> Result<(), Box<dyn Error>> {
    let input = fixture()?;
    let mut checkpoints = 0;
    clean(&input, all_metadata(), limits(), || {
        checkpoints += 1;
        Ok(())
    })?;
    for stop_at in 1..=checkpoints {
        let mut current = 0;
        let result = clean(&input, all_metadata(), limits(), || {
            current += 1;
            if current == stop_at {
                Err(StopReason::Cancelled)
            } else {
                Ok(())
            }
        });
        assert_eq!(result.err(), Some(PngError::Stopped(StopReason::Cancelled)));
    }
    for reason in [StopReason::DeadlineExceeded, StopReason::Superseded] {
        assert_eq!(
            clean(&input, all_metadata(), limits(), || Err(reason)).err(),
            Some(PngError::Stopped(reason))
        );
    }
    Ok(())
}
