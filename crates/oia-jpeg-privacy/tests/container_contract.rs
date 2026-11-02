use oia_jpeg_privacy::{
    Coverage, Handling, JpegError, Limit, Limits, SegmentKind, Selection, StopReason, Target,
    clean, inspect,
};
use std::error::Error;

/// Offset just after the 18-byte JFIF APP0 that follows SOI in both fixtures.
const AFTER_JFIF: usize = 20;

fn limits() -> Limits {
    Limits {
        input_bytes: 1_048_576,
        output_bytes: 1_048_576,
        segments: 128,
        metadata_bytes: 65_536,
        width: 4096,
        height: 4096,
        pixels: 16_777_216,
    }
}

fn decode(hex: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    hex.trim()
        .as_bytes()
        .chunks(2)
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}

fn baseline() -> Result<Vec<u8>, Box<dyn Error>> {
    decode(include_str!("fixtures/baseline.hex"))
}

fn progressive() -> Result<Vec<u8>, Box<dyn Error>> {
    decode(include_str!("fixtures/progressive.hex"))
}

fn all_metadata() -> Selection {
    Selection {
        comment: true,
        exif: true,
        xmp: true,
        photoshop_irb: true,
    }
}

fn segment(marker: u8, payload: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
    let length = u16::try_from(payload.len() + 2)?;
    let mut wire = vec![0xff, marker];
    wire.extend_from_slice(&length.to_be_bytes());
    wire.extend_from_slice(payload);
    Ok(wire)
}

fn with_segment(input: &[u8], wire: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = input.get(..AFTER_JFIF).ok_or("prefix")?.to_vec();
    bytes.extend_from_slice(wire);
    bytes.extend_from_slice(input.get(AFTER_JFIF..).ok_or("suffix")?);
    Ok(bytes)
}

/// Little-endian Exif APP1 whose IFD0 holds only Orientation = `value`.
fn exif_with_orientation(value: u8) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut payload = b"Exif\0\0II*\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0".to_vec();
    payload.extend_from_slice(&[value, 0, 0, 0, 0, 0, 0, 0]);
    segment(0xe1, &payload)
}

fn refused(input: &[u8]) -> Result<(), Box<dyn Error>> {
    assert_eq!(
        inspect(input, limits(), || Ok(()))?.coverage,
        Coverage::Partial
    );
    assert_eq!(
        clean(input, all_metadata(), limits(), || Ok(())).err(),
        Some(JpegError::UnsupportedProfile)
    );
    Ok(())
}

#[test]
fn selected_removal_preserves_every_retained_byte() -> Result<(), Box<dyn Error>> {
    let input = baseline()?;
    let snapshot = input.clone();
    let original = inspect(&input, limits(), || Ok(()))?;
    assert_eq!(original.coverage, Coverage::CompleteContainerProfile);
    assert_eq!((original.frame.width, original.frame.height), (16, 16));
    let removable: Vec<_> = original
        .segments
        .iter()
        .filter_map(|segment| match segment.handling {
            Handling::Removable(target) => Some(target),
            _ => None,
        })
        .collect();
    assert_eq!(
        removable,
        [
            Target::Exif,
            Target::Xmp,
            Target::PhotoshopIrb,
            Target::Comment
        ]
    );
    let cleaned = clean(&input, all_metadata(), limits(), || Ok(()))?;
    assert_eq!(cleaned.removed_segments(), 4);
    assert_eq!(input, snapshot);
    let mut expected = Vec::new();
    for segment in original
        .segments
        .iter()
        .filter(|segment| segment.handling == Handling::Retained)
    {
        expected.extend_from_slice(input.get(segment.wire_range.clone()).ok_or("region")?);
    }
    assert_eq!(cleaned.bytes(), expected);
    assert_eq!(
        cleaned.inspection(),
        &inspect(cleaned.bytes(), limits(), || Ok(()))?
    );
    assert!(!format!("{cleaned:?}").contains("Synthetic"));
    let comment_only = Selection {
        comment: true,
        exif: false,
        xmp: false,
        photoshop_irb: false,
    };
    let partial = clean(&input, comment_only, limits(), || Ok(()))?;
    assert_eq!(partial.removed_segments(), 1);
    assert!(
        partial
            .inspection()
            .segments
            .iter()
            .any(|segment| segment.kind == SegmentKind::Exif)
    );
    let none = Selection {
        comment: false,
        ..comment_only
    };
    assert_eq!(clean(&input, none, limits(), || Ok(()))?.bytes(), input);
    // A progressive frame with ten scans keeps every scan's entropy-coded bytes.
    let multi_scan = progressive()?;
    let report = inspect(&multi_scan, limits(), || Ok(()))?;
    assert_eq!(report.coverage, Coverage::CompleteContainerProfile);
    let scans = report
        .segments
        .iter()
        .filter(|segment| segment.entropy_coded_range.is_some())
        .count();
    assert_eq!(scans, 10);
    let cleaned = clean(&multi_scan, all_metadata(), limits(), || Ok(()))?;
    assert_eq!(cleaned.removed_segments(), 1);
    Ok(())
}

// Contract section 3: an Exif segment is removed only when doing so cannot drop
// orientation; otherwise it is retained, or the selected cleanup is refused.
#[test]
fn exif_removal_never_drops_orientation() -> Result<(), Box<dyn Error>> {
    let rotated = with_segment(&progressive()?, &exif_with_orientation(6)?)?;
    let report = inspect(&rotated, limits(), || Ok(()))?;
    assert_eq!(report.coverage, Coverage::CompleteContainerProfile);
    assert!(
        report
            .segments
            .iter()
            .any(|segment| segment.handling == Handling::RequiredForRendering(Target::Exif))
    );
    assert_eq!(
        clean(&rotated, all_metadata(), limits(), || Ok(())).err(),
        Some(JpegError::RequiredForRendering(Target::Exif))
    );
    let keep_exif = Selection {
        exif: false,
        ..all_metadata()
    };
    let kept = clean(&rotated, keep_exif, limits(), || Ok(()))?;
    assert!(
        kept.inspection()
            .segments
            .iter()
            .any(|segment| segment.kind == SegmentKind::Exif)
    );
    let default = with_segment(&progressive()?, &exif_with_orientation(1)?)?;
    assert_eq!(
        clean(&default, all_metadata(), limits(), || Ok(()))?.bytes(),
        clean(&progressive()?, all_metadata(), limits(), || Ok(()))?.bytes()
    );
    // Out-of-range orientation and a non-TIFF payload are opaque, not removable.
    refused(&with_segment(&progressive()?, &exif_with_orientation(9)?)?)?;
    refused(&with_segment(
        &progressive()?,
        &segment(0xe1, b"Exif\0\0not a TIFF header")?,
    )?)?;
    Ok(())
}

#[test]
fn opaque_regions_cannot_yield_cleaned_output() -> Result<(), Box<dyn Error>> {
    let input = progressive()?;
    let mut xmp_orientation = b"http://ns.adobe.com/xap/1.0/\0".to_vec();
    xmp_orientation.extend_from_slice(b"<x tiff:Orientation=\"6\"/>");
    let mut embedded_exif = b"Photoshop 3.0\08BIM\x04\x22\0\0".to_vec();
    embedded_exif.extend_from_slice(&[0, 0, 0, 2, 0, 0]);
    for wire in [
        segment(0xe2, b"ICC_PROFILE\0\x01\x01opaque")?,
        segment(0xe1, &xmp_orientation)?,
        segment(0xe1, b"http://ns.adobe.com/xmp/extension/\0part")?,
        segment(0xed, &embedded_exif)?,
        segment(0xe5, b"vendor")?,
        segment(0xe0, b"JFXX\0\x10")?,
    ] {
        refused(&with_segment(&input, &wire)?)?;
    }
    // A JFIF thumbnail (1x1, three bytes) is a preview the profile cannot remove.
    let mut thumbnail = input.clone();
    thumbnail.splice(
        2..20,
        segment(0xe0, b"JFIF\0\x01\x01\0\0\x01\0\x01\x01\x01\x07\x08\x09")?,
    );
    refused(&thumbnail)?;
    let mut trailing = input.clone();
    trailing.extend_from_slice(b"appended data");
    let report = inspect(&trailing, limits(), || Ok(()))?;
    assert_eq!(report.trailing_range, Some(input.len()..trailing.len()));
    refused(&trailing)?;
    // Same frame header under the arithmetic-coding SOF9 marker.
    let mut arithmetic = input.clone();
    let frame = inspect(&input, limits(), || Ok(()))?
        .segments
        .iter()
        .find(|segment| segment.kind == SegmentKind::Frame)
        .ok_or("frame")?
        .wire_range
        .start;
    *arithmetic.get_mut(frame + 1).ok_or("marker")? = 0xc9;
    assert!(inspect(&arithmetic, limits(), || Ok(()))?.unsupported_frame);
    refused(&arithmetic)?;
    Ok(())
}

#[test]
fn truncation_and_structure_errors_fail_closed() -> Result<(), Box<dyn Error>> {
    let input = baseline()?;
    for end in 0..input.len() {
        assert!(inspect(input.get(..end).ok_or("prefix")?, limits(), || Ok(())).is_err());
    }
    let restart = with_segment(&input, &[0xff, 0xd0])?;
    assert!(matches!(
        inspect(&restart, limits(), || Ok(())),
        Err(JpegError::Malformed {
            reason: "restart marker outside entropy-coded data",
            ..
        })
    ));
    let early_scan = with_segment(&input, &segment(0xda, b"\x01\x01\0\0\x3f\0")?)?;
    assert!(matches!(
        inspect(&early_scan, limits(), || Ok(())),
        Err(JpegError::Malformed {
            reason: "scan before frame",
            ..
        })
    ));
    let mut long_length = input.clone();
    *long_length.get_mut(5).ok_or("length")? = 0xff;
    assert!(inspect(&long_length, limits(), || Ok(())).is_err());
    assert_eq!(
        inspect(b"\x89PNG", limits(), || Ok(())).err(),
        Some(JpegError::UnsupportedFormat)
    );
    Ok(())
}

#[test]
fn resource_budgets_reject_before_publication() -> Result<(), Box<dyn Error>> {
    let input = baseline()?;
    let mut budget = limits();
    budget.width = 15;
    assert_eq!(
        inspect(&input, budget, || Ok(())).err(),
        Some(JpegError::ResourceLimit(Limit::Width))
    );
    budget = limits();
    budget.height = 15;
    assert_eq!(
        inspect(&input, budget, || Ok(())).err(),
        Some(JpegError::ResourceLimit(Limit::Height))
    );
    budget = limits();
    budget.pixels = 255;
    assert_eq!(
        inspect(&input, budget, || Ok(())).err(),
        Some(JpegError::ResourceLimit(Limit::Pixels))
    );
    budget = limits();
    budget.input_bytes = input.len() - 1;
    assert_eq!(
        inspect(&input, budget, || Ok(())).err(),
        Some(JpegError::ResourceLimit(Limit::InputBytes))
    );
    budget = limits();
    budget.segments = 5;
    assert_eq!(
        inspect(&input, budget, || Ok(())).err(),
        Some(JpegError::ResourceLimit(Limit::Segments))
    );
    budget = limits();
    budget.metadata_bytes = 100;
    assert_eq!(
        inspect(&input, budget, || Ok(())).err(),
        Some(JpegError::ResourceLimit(Limit::MetadataBytes))
    );
    budget = limits();
    budget.output_bytes = 650;
    assert_eq!(
        clean(&input, all_metadata(), budget, || Ok(())).err(),
        Some(JpegError::ResourceLimit(Limit::OutputBytes))
    );
    budget = limits();
    budget.segments = 0;
    assert_eq!(
        inspect(&input, budget, || Ok(())).err(),
        Some(JpegError::ZeroLimit(Limit::Segments))
    );
    Ok(())
}

#[test]
fn cancellation_and_deadlines_never_return_a_candidate() -> Result<(), Box<dyn Error>> {
    for input in [baseline()?, progressive()?] {
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
            assert_eq!(
                result.err(),
                Some(JpegError::Stopped(StopReason::Cancelled))
            );
        }
        for reason in [StopReason::DeadlineExceeded, StopReason::Superseded] {
            assert_eq!(
                clean(&input, all_metadata(), limits(), || Err(reason)).err(),
                Some(JpegError::Stopped(reason))
            );
        }
    }
    Ok(())
}
