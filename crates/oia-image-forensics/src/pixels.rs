use crate::{
    BlockMatch, BlockPosition, CappedWriter, Config, Coverage, DerivedArtifact, ForensicError,
    Method, PROFILE, RepeatedBlocks, ResidualSummary, digest,
};
use image::codecs::{jpeg::JpegEncoder, png::PngEncoder};
use image::{DynamicImage, ExtendedColorType, ImageEncoder, ImageFormat, RgbaImage};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Write;

fn increment(histogram: &mut [u64], value: u8) -> Result<(), ForensicError> {
    let count = histogram
        .get_mut(usize::from(value))
        .ok_or(ForensicError::Unsupported("histogram invariant"))?;
    *count += 1;
    Ok(())
}

pub(crate) fn histograms(
    raster: &RgbaImage,
    checkpoint: &mut impl FnMut() -> Result<(), ForensicError>,
) -> Result<[Vec<u64>; 3], ForensicError> {
    let mut histograms = [vec![0; 256], vec![0; 256], vec![0; 256]];
    for row in raster.rows() {
        checkpoint()?;
        for pixel in row {
            for (histogram, value) in histograms.iter_mut().zip(pixel.0.iter().take(3)) {
                increment(histogram, *value)?;
            }
        }
    }
    Ok(histograms)
}

fn block(
    raster: &RgbaImage,
    left: u32,
    top: u32,
    size: u32,
) -> Result<(Vec<u8>, bool, u8), ForensicError> {
    let mut bytes = Vec::new();
    let mut opaque = true;
    let mut minima = [255_u8; 3];
    let mut maxima = [0_u8; 3];
    for row in top..top + size {
        for column in left..left + size {
            let pixel = raster
                .get_pixel_checked(column, row)
                .ok_or(ForensicError::Unsupported("block outside raster"))?;
            let [red, green, blue, alpha] = pixel.0;
            opaque &= alpha == 255;
            for ((minimum, maximum), value) in
                minima.iter_mut().zip(&mut maxima).zip([red, green, blue])
            {
                *minimum = (*minimum).min(value);
                *maximum = (*maximum).max(value);
            }
            bytes.extend_from_slice(&pixel.0);
        }
    }
    let range = minima
        .iter()
        .zip(maxima)
        .map(|(minimum, maximum)| maximum - minimum)
        .max()
        .unwrap_or(0);
    Ok((bytes, opaque, range))
}

pub(crate) fn repeated_blocks(
    raster: &RgbaImage,
    config: &Config,
    checkpoint: &mut impl FnMut() -> Result<(), ForensicError>,
) -> Result<Method<RepeatedBlocks>, ForensicError> {
    let columns = raster.width() / config.block_size;
    let rows = raster.height() / config.block_size;
    let mut result = RepeatedBlocks {
        total_grid_blocks: u64::from(columns) * u64::from(rows),
        examined_grid_blocks: 0,
        eligible_blocks: 0,
        excluded_low_range_blocks: 0,
        excluded_nonopaque_blocks: 0,
        uncovered_right_pixels: raster.width() % config.block_size,
        uncovered_bottom_pixels: raster.height() % config.block_size,
        candidates: Vec::new(),
        stopped_at: None,
    };
    let mut seen: HashMap<[u8; 32], (u32, u32)> = HashMap::new();
    'scan: for row in 0..rows {
        for column in 0..columns {
            checkpoint()?;
            if result.examined_grid_blocks == config.max_blocks {
                result.stopped_at = Some("block_limit");
                break 'scan;
            }
            result.examined_grid_blocks += 1;
            let left = column * config.block_size;
            let top = row * config.block_size;
            let (bytes, opaque, range) = block(raster, left, top, config.block_size)?;
            if !opaque {
                result.excluded_nonopaque_blocks += 1;
                continue;
            }
            if range < config.minimum_channel_range {
                result.excluded_low_range_blocks += 1;
                continue;
            }
            result.eligible_blocks += 1;
            let hash: [u8; 32] = Sha256::digest(&bytes).into();
            if let Some((first_x, first_y)) = seen.get(&hash).copied() {
                let (first_bytes, _, _) = block(raster, first_x, first_y, config.block_size)?;
                if first_bytes == bytes {
                    if result.candidates.len() == config.max_candidates {
                        result.stopped_at = Some("candidate_limit");
                        break 'scan;
                    }
                    result.candidates.push(BlockMatch {
                        first: BlockPosition {
                            x: first_x,
                            y: first_y,
                        },
                        repeated: BlockPosition { x: left, y: top },
                    });
                }
            } else {
                seen.insert(hash, (left, top));
            }
        }
    }
    Ok(Method {
        coverage: if result.total_grid_blocks == 0 {
            Coverage::NotApplicable
        } else if result.stopped_at.is_some() {
            Coverage::Partial
        } else {
            Coverage::Complete
        },
        scope: "row-major, nonoverlapping grid-aligned opaque RGBA blocks; each repeat paired with its first representative",
        result: Some(result),
        limitations: vec![
            "Repeated patterns, screenshots, layouts and genuine scenes can yield identical blocks; candidates do not establish copy-move editing.",
            "Misses shifted, rotated, scaled, recompressed, blended or otherwise changed copies; no full-image authenticity coverage.",
            "Partial edge strips, nonopaque blocks and low-range blocks are excluded explicitly; range threshold is a heuristic, not an evaluated classifier.",
        ],
    })
}

pub(crate) fn recompression(
    raster: &RgbaImage,
    format: ImageFormat,
    source_sha256: &str,
    config: &Config,
    checkpoint: &mut impl FnMut() -> Result<(), ForensicError>,
) -> Result<(Method<ResidualSummary>, Option<Vec<u8>>), ForensicError> {
    let limitations = vec![
        "JPEG residuals depend on encoder, quality, texture and prior saves; bright regions do not prove edits or AI generation.",
        "Statistics use unamplified RGB differences (0..255), not probabilities. Optional map is a derived visualization with clipped gain.",
        "PNG is not tested by JPEG recompression: converting it would introduce a new lossy history.",
    ];
    if format != ImageFormat::Jpeg {
        return Ok((
            Method {
                coverage: Coverage::NotApplicable,
                scope: "JPEG decode -> image JPEG encode at recorded quality -> decode -> absolute RGB difference",
                result: None,
                limitations,
            },
            None,
        ));
    }
    let mut rgb = Vec::new();
    for row in raster.rows() {
        checkpoint()?;
        for pixel in row {
            rgb.extend(pixel.0.iter().take(3));
        }
    }
    let mut encoded = CappedWriter {
        bytes: Vec::new(),
        limit: config.max_artifact_bytes,
    };
    JpegEncoder::new_with_quality(&mut encoded, config.jpeg_quality).encode(
        &rgb,
        raster.width(),
        raster.height(),
        ExtendedColorType::Rgb8,
    )?;
    checkpoint()?;
    let recompressed = DynamicImage::ImageRgba8(crate::decode::raster(
        &encoded.bytes,
        ImageFormat::Jpeg,
        config,
    )?)
    .into_rgb8();
    checkpoint()?;
    let mut histogram = vec![0_u64; 256];
    let mut total_difference = 0_u64;
    let mut maximum_difference = 0_u8;
    let mut visualization = Vec::new();
    for (position, (original, derived)) in rgb.iter().zip(recompressed.as_raw()).enumerate() {
        if position % 4096 == 0 {
            checkpoint()?;
        }
        let difference = original.abs_diff(*derived);
        increment(&mut histogram, difference)?;
        total_difference += u64::from(difference);
        maximum_difference = maximum_difference.max(difference);
        if config.emit_residual_map {
            visualization.push(difference.saturating_mul(config.residual_gain));
        }
    }
    let residual_png = if config.emit_residual_map {
        let mut output = CappedWriter {
            bytes: Vec::new(),
            limit: config.max_artifact_bytes,
        };
        PngEncoder::new(&mut output).write_image(
            &visualization,
            raster.width(),
            raster.height(),
            ExtendedColorType::Rgb8,
        )?;
        checkpoint()?;
        Some(label_map(&output.bytes, source_sha256, config)?)
    } else {
        None
    };
    let artifact = residual_png.as_ref().map(|bytes| DerivedArtifact {
        role: "derived_jpeg_recompression_residual_not_observation",
        media_type: "image/png",
        sha256: digest(bytes),
        byte_length: bytes.len(),
    });
    Ok((
        Method {
            coverage: Coverage::Complete,
            scope: "JPEG decode -> image JPEG encode at recorded quality -> decode -> absolute RGB difference",
            result: Some(ResidualSummary {
                mean_absolute_channel_difference: total_difference as f64 / rgb.len() as f64,
                maximum_channel_difference: maximum_difference,
                channel_difference_histogram: histogram,
                artifact,
            }),
            limitations,
        },
        residual_png,
    ))
}

fn label_map(png: &[u8], source_sha256: &str, config: &Config) -> Result<Vec<u8>, ForensicError> {
    let end = png
        .len()
        .checked_sub(12)
        .ok_or(ForensicError::Unsupported("encoded PNG length"))?;
    let mut output = CappedWriter {
        bytes: Vec::new(),
        limit: config.max_artifact_bytes,
    };
    let settings = format!(
        "quality={},gain={},implementation={}",
        config.jpeg_quality,
        config.residual_gain,
        env!("CARGO_PKG_VERSION")
    );
    let fields = [
        (
            "oia:role",
            "derived JPEG recompression residual; not an observation or authenticity verdict",
        ),
        ("oia:source_sha256", source_sha256),
        ("oia:profile", PROFILE),
        (
            "oia:input_validation",
            "JPEG entropy completeness unverified; decoder recovery possible",
        ),
        ("oia:settings", settings.as_str()),
    ];
    let write_error = |_| ForensicError::Limit("encoded artifact bytes");
    output
        .write_all(
            png.get(..end)
                .ok_or(ForensicError::Unsupported("encoded PNG body"))?,
        )
        .map_err(write_error)?;
    for (key, value) in fields {
        let mut payload = key.as_bytes().to_vec();
        payload.push(0);
        payload.extend_from_slice(value.as_bytes());
        let length = u32::try_from(payload.len())
            .map_err(|_| ForensicError::Limit("artifact metadata bytes"))?;
        let mut checksum = crc32fast::Hasher::new();
        checksum.update(b"tEXt");
        checksum.update(&payload);
        output
            .write_all(&length.to_be_bytes())
            .map_err(write_error)?;
        output.write_all(b"tEXt").map_err(write_error)?;
        output.write_all(&payload).map_err(write_error)?;
        output
            .write_all(&checksum.finalize().to_be_bytes())
            .map_err(write_error)?;
    }
    output
        .write_all(
            png.get(end..)
                .ok_or(ForensicError::Unsupported("encoded PNG end"))?,
        )
        .map_err(write_error)?;
    Ok(output.bytes)
}
