use image::codecs::{jpeg::JpegEncoder, png::PngEncoder};
use image::{ExtendedColorType, ImageEncoder, ImageFormat, RgbImage};
use oia_image_forensics::{Config, Coverage, ForensicError, StopReason, analyze};
use sha2::{Digest, Sha256};
use std::error::Error;

fn texture(width: u32) -> RgbImage {
    RgbImage::from_fn(width, 16, |pixel_x, pixel_y| {
        image::Rgb([
            ((pixel_x % 16) * 15) as u8,
            (pixel_y * 15) as u8,
            ((pixel_x % 16 + pixel_y) * 7) as u8,
        ])
    })
}

fn encode(raster: &RgbImage, format: ImageFormat) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = Vec::new();
    match format {
        ImageFormat::Png => PngEncoder::new(&mut bytes).write_image(
            raster.as_raw(),
            raster.width(),
            raster.height(),
            ExtendedColorType::Rgb8,
        )?,
        ImageFormat::Jpeg => JpegEncoder::new_with_quality(&mut bytes, 87).encode(
            raster.as_raw(),
            raster.width(),
            raster.height(),
            ExtendedColorType::Rgb8,
        )?,
        _ => return Err("fixture format".into()),
    }
    Ok(bytes)
}

fn add_png_chunk(input: &[u8], kind: &[u8; 4], payload: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = input.get(..33).ok_or("fixture header")?.to_vec();
    bytes.extend_from_slice(&u32::try_from(payload.len())?.to_be_bytes());
    bytes.extend_from_slice(kind);
    bytes.extend_from_slice(payload);
    let mut checksum = crc32fast::Hasher::new();
    checksum.update(kind);
    checksum.update(payload);
    bytes.extend_from_slice(&checksum.finalize().to_be_bytes());
    bytes.extend_from_slice(input.get(33..).ok_or("fixture body")?);
    Ok(bytes)
}

#[test]
fn metadata_and_duplicate_cues_preserve_evidence_without_promoting_assertions()
-> Result<(), Box<dyn Error>> {
    let original = encode(&texture(32), ImageFormat::Png)?;
    let declared = add_png_chunk(
        &original,
        b"tEXt",
        b"parameters\0private prompt\nSteps: 20, Sampler: Euler, Seed: 42",
    )?;
    let retained = declared.clone();
    let report = analyze(&declared, &Config::default(), || Ok(()))?.report;
    assert_eq!(declared, retained);
    assert_eq!(report.assessment, "inconclusive");
    assert_eq!(
        report.learned_ai_detection.coverage,
        Coverage::NotImplemented
    );
    assert_eq!(
        report.source.sha256,
        format!("{:x}", Sha256::digest(&declared))
    );
    let metadata = report.metadata.result.as_ref().ok_or("metadata result")?;
    assert_eq!(metadata.cues.len(), 1);
    let cue = metadata.cues.first().ok_or("generation cue")?;
    let payload = declared
        .get(cue.byte_offset..cue.byte_offset + cue.byte_length)
        .ok_or("cue anchor")?;
    assert_eq!(cue.payload_sha256, format!("{:x}", Sha256::digest(payload)));
    let repeats = report
        .repeated_blocks
        .result
        .as_ref()
        .ok_or("repeat result")?;
    assert_eq!(repeats.candidates.len(), 1);
    assert_eq!(repeats.candidates.first().ok_or("repeat")?.repeated.x, 16);
    let without_metadata = analyze(&original, &Config::default(), || Ok(()))?.report;
    assert_eq!(without_metadata.assessment, "inconclusive");
    assert!(
        without_metadata
            .metadata
            .result
            .ok_or("metadata result")?
            .cues
            .is_empty()
    );
    assert_ne!(without_metadata.source.sha256, report.source.sha256);
    assert_eq!(
        without_metadata.rgb_histograms.result,
        report.rgb_histograms.result
    );
    assert_eq!(
        report
            .rgb_histograms
            .result
            .as_ref()
            .ok_or("histogram result")?
            .first()
            .ok_or("histogram")?
            .iter()
            .sum::<u64>(),
        512
    );
    assert!(!serde_json::to_string(&report)?.contains("private prompt"));
    Ok(())
}

#[test]
fn jpeg_residual_matches_unamplified_reference_and_separate_visualization()
-> Result<(), Box<dyn Error>> {
    let input = encode(&texture(32), ImageFormat::Jpeg)?;
    let config = Config {
        emit_residual_map: true,
        ..Config::default()
    };
    let analysis = analyze(&input, &config, || Ok(()))?;
    assert_eq!(
        analysis.report.raster_validation.coverage,
        Coverage::Partial
    );
    assert!(analysis.report.profile_is_partial());
    for (coverage, limitations) in [
        (
            analysis.report.rgb_histograms.coverage,
            &analysis.report.rgb_histograms.limitations,
        ),
        (
            analysis.report.repeated_blocks.coverage,
            &analysis.report.repeated_blocks.limitations,
        ),
        (
            analysis.report.jpeg_recompression.coverage,
            &analysis.report.jpeg_recompression.limitations,
        ),
    ] {
        assert_eq!(coverage, Coverage::Partial);
        assert!(
            limitations.contains(
                &analysis
                    .report
                    .raster_validation
                    .result
                    .ok_or("raster reason")?
            )
        );
    }
    let original = image::load_from_memory_with_format(&input, ImageFormat::Jpeg)?.to_rgb8();
    let mut encoded = Vec::new();
    JpegEncoder::new_with_quality(&mut encoded, config.jpeg_quality).encode(
        original.as_raw(),
        original.width(),
        original.height(),
        ExtendedColorType::Rgb8,
    )?;
    let derived = image::load_from_memory_with_format(&encoded, ImageFormat::Jpeg)?.to_rgb8();
    let differences: Vec<u8> = original
        .as_raw()
        .iter()
        .zip(derived.as_raw())
        .map(|(first, second)| first.abs_diff(*second))
        .collect();
    let sum: u64 = differences.iter().map(|value| u64::from(*value)).sum();
    let result = analysis
        .report
        .jpeg_recompression
        .result
        .ok_or("JPEG result")?;
    assert_eq!(
        result.mean_absolute_channel_difference,
        sum as f64 / differences.len() as f64
    );
    assert_eq!(
        result.channel_difference_histogram.iter().sum::<u64>(),
        differences.len() as u64
    );
    let png = analysis.residual_png.ok_or("residual image")?;
    assert_eq!(
        result.artifact.ok_or("artifact")?.sha256,
        format!("{:x}", Sha256::digest(&png))
    );
    let visualization = image::load_from_memory_with_format(&png, ImageFormat::Png)?.to_rgb8();
    assert_eq!(
        visualization.as_raw(),
        &differences
            .iter()
            .map(|value| value.saturating_mul(config.residual_gain))
            .collect::<Vec<_>>()
    );
    let mut with_xmp = input.get(..2).ok_or("JPEG header")?.to_vec();
    let payload = b"http://ns.adobe.com/xap/1.0/\0<x>http://cv.iptc.org/newscodes/digitalsourcetype/trainedAlgorithmicMedia</x>";
    with_xmp.extend_from_slice(&[0xff, 0xe1]);
    with_xmp.extend_from_slice(&u16::try_from(payload.len() + 2)?.to_be_bytes());
    with_xmp.extend_from_slice(payload);
    with_xmp.extend_from_slice(input.get(2..).ok_or("JPEG body")?);
    let asserted = analyze(&with_xmp, &Config::default(), || Ok(()))?.report;
    assert_eq!(
        asserted
            .metadata
            .result
            .ok_or("metadata")?
            .cues
            .first()
            .ok_or("XMP cue")?
            .kind,
        "synthetic_source_type_signature"
    );
    assert_eq!(asserted.assessment, "inconclusive");
    Ok(())
}

#[test]
fn scan_budgets_and_uninspected_regions_remain_visible() -> Result<(), Box<dyn Error>> {
    let input = encode(&texture(48), ImageFormat::Png)?;
    let block_limited = Config {
        max_blocks: 1,
        ..Config::default()
    };
    let partial = analyze(&input, &block_limited, || Ok(()))?.report;
    assert!(partial.profile_is_partial());
    let blocks = partial.repeated_blocks.result.ok_or("blocks")?;
    assert_eq!(blocks.examined_grid_blocks, 1);
    assert_eq!(blocks.total_grid_blocks, 3);
    assert_eq!(blocks.stopped_at, Some("block_limit"));
    let candidate_limited = Config {
        max_candidates: 1,
        ..Config::default()
    };
    let report = analyze(&input, &candidate_limited, || Ok(()))?.report;
    assert_eq!(report.repeated_blocks.coverage, Coverage::Partial);
    assert_eq!(
        report.repeated_blocks.result.ok_or("blocks")?.stopped_at,
        Some("candidate_limit")
    );
    let compressed = add_png_chunk(
        &input,
        b"zTXt",
        b"parameters\0\0\x78\x9c\x03\x00\x00\x00\x00\x01",
    )?;
    let report = analyze(&compressed, &Config::default(), || Ok(()))?.report;
    assert_eq!(report.metadata.coverage, Coverage::Partial);
    assert!(
        !report
            .metadata
            .result
            .ok_or("metadata")?
            .uninspected
            .is_empty()
    );
    let plain = encode(
        &RgbImage::from_pixel(32, 16, image::Rgb([255, 0, 0])),
        ImageFormat::Png,
    )?;
    let report = analyze(&plain, &Config::default(), || Ok(()))?.report;
    let blocks = report.repeated_blocks.result.ok_or("blocks")?;
    assert!(blocks.candidates.is_empty());
    assert_eq!(blocks.excluded_low_range_blocks, 2);
    Ok(())
}

#[test]
fn malformed_oversized_animated_and_stale_inputs_cannot_publish_reports()
-> Result<(), Box<dyn Error>> {
    let input = encode(&texture(32), ImageFormat::Png)?;
    let oversized = Config {
        max_input_bytes: input.len() - 1,
        ..Config::default()
    };
    assert!(matches!(
        analyze(&input, &oversized, || Ok(())),
        Err(ForensicError::Limit("input bytes"))
    ));
    let pixel_limited = Config {
        max_pixels: 1,
        ..Config::default()
    };
    assert!(matches!(
        analyze(&input, &pixel_limited, || Ok(())),
        Err(ForensicError::Limit("decoded pixels"))
    ));
    let mut corrupt = input.clone();
    *corrupt.last_mut().ok_or("fixture")? ^= 1;
    assert!(matches!(
        analyze(&corrupt, &Config::default(), || Ok(())),
        Err(ForensicError::Malformed { .. })
    ));
    let mut trailing = input.clone();
    trailing.push(0);
    assert!(matches!(
        analyze(&trailing, &Config::default(), || Ok(())),
        Err(ForensicError::Malformed { .. })
    ));
    let animated = add_png_chunk(&input, b"acTL", &[0, 0, 0, 1, 0, 0, 0, 0])?;
    assert!(matches!(
        analyze(&animated, &Config::default(), || Ok(())),
        Err(ForensicError::Unsupported(_))
    ));
    let mut checkpoint_count = 0;
    analyze(&input, &Config::default(), || {
        checkpoint_count += 1;
        Ok(())
    })?;
    let mut replay_count = 0;
    let stopped = analyze(&input, &Config::default(), || {
        replay_count += 1;
        if replay_count == checkpoint_count {
            Err(StopReason::Superseded)
        } else {
            Ok(())
        }
    });
    assert!(matches!(
        stopped,
        Err(ForensicError::Stopped(StopReason::Superseded))
    ));
    assert_eq!(replay_count, checkpoint_count);
    let jpeg = encode(&texture(32), ImageFormat::Jpeg)?;
    let allocation_limited = Config {
        decoder_allocation_bytes: 32 * 16 * 7,
        ..Config::default()
    };
    assert!(matches!(
        analyze(&jpeg, &allocation_limited, || Ok(())),
        Err(ForensicError::Limit(
            "JPEG output/conversion/coefficient bytes"
        ))
    ));
    let artifact_limited = Config {
        max_artifact_bytes: 1,
        ..Config::default()
    };
    assert!(analyze(&jpeg, &artifact_limited, || Ok(())).is_err());
    Ok(())
}
