use crate::{Config, ForensicError, MetadataAnalysis, metadata};
use image::ImageFormat;
use std::ops::Range;

pub(crate) struct Parsed {
    pub format: ImageFormat,
    pub decode_bytes: Vec<u8>,
    pub metadata: MetadataAnalysis,
}

fn region(input: &[u8], range: Range<usize>) -> Result<&[u8], ForensicError> {
    input.get(range.clone()).ok_or(ForensicError::Malformed {
        offset: range.start,
        reason: "inspector range outside source",
    })
}

pub(crate) fn inspect(
    input: &[u8],
    config: &Config,
    checkpoint: &mut impl FnMut() -> Result<(), ForensicError>,
) -> Result<Parsed, ForensicError> {
    if input.starts_with(b"\x89PNG\r\n\x1a\n") {
        png(input, config, checkpoint)
    } else if input.starts_with(b"\xff\xd8") {
        jpeg(input, config, checkpoint)
    } else {
        Err(ForensicError::Unsupported(
            "only static PNG and JPEG are supported",
        ))
    }
}

fn png(
    input: &[u8],
    config: &Config,
    checkpoint: &mut impl FnMut() -> Result<(), ForensicError>,
) -> Result<Parsed, ForensicError> {
    let limits = oia_png_privacy::Limits {
        input_bytes: config.max_input_bytes,
        output_bytes: config.max_input_bytes,
        chunk_bytes: config.max_input_bytes,
        chunks: config.max_container_items,
        metadata_bytes: config.max_metadata_bytes,
        width: config.max_dimension,
        height: config.max_dimension,
        pixels: config.max_pixels,
    };
    let mut stopped = None;
    let inspection = oia_png_privacy::inspect(input, limits, || {
        checkpoint().map_err(|error| {
            stopped = Some(error);
            oia_png_privacy::StopReason::Cancelled
        })
    });
    if let Some(error) = stopped {
        return Err(error);
    }
    let inspection = inspection.map_err(|error| match error {
        oia_png_privacy::PngError::Malformed { offset, reason } => {
            ForensicError::Malformed { offset, reason }
        }
        oia_png_privacy::PngError::ResourceLimit(oia_png_privacy::Limit::Pixels) => {
            ForensicError::Limit("decoded pixels")
        }
        other => ForensicError::PngContainer(other),
    })?;
    let mut parsed = Parsed {
        format: ImageFormat::Png,
        decode_bytes: b"\x89PNG\r\n\x1a\n".to_vec(),
        metadata: MetadataAnalysis::default(),
    };
    let mut compressed = Vec::new();
    let mut data_offset = 0;
    let mut seen_data = false;
    let mut seen_palette = false;
    let mut seen_transparency = false;
    for chunk in inspection.chunks {
        checkpoint()?;
        let payload = region(input, chunk.payload_range.clone())?;
        let kind = chunk.kind.as_slice();
        if kind == b"IHDR" && payload.get(8).is_some_and(|depth| *depth > 8) {
            return Err(ForensicError::Unsupported("PNG above 8-bit sample depth"));
        }
        if matches!(kind, b"acTL" | b"fcTL" | b"fdAT") {
            return Err(ForensicError::Unsupported(
                "animated PNG; no silent first-frame analysis",
            ));
        }
        if kind == b"PLTE" || kind == b"tRNS" {
            let duplicate = if kind == b"PLTE" {
                std::mem::replace(&mut seen_palette, true)
            } else {
                std::mem::replace(&mut seen_transparency, true)
            };
            if seen_data || duplicate {
                return Err(ForensicError::Malformed {
                    offset: chunk.wire_range.start,
                    reason: "late or duplicate palette/transparency",
                });
            }
        }
        if kind == b"IDAT" {
            if !seen_data {
                data_offset = chunk.payload_range.start;
            }
            compressed.extend_from_slice(payload);
            seen_data = true;
        }
        if matches!(kind, b"IHDR" | b"PLTE" | b"tRNS" | b"IDAT" | b"IEND") {
            parsed
                .decode_bytes
                .extend_from_slice(region(input, chunk.wire_range)?);
        } else {
            if kind.first().is_some_and(u8::is_ascii_uppercase) {
                return Err(ForensicError::Unsupported("unknown critical PNG chunk"));
            }
            metadata::png_field(
                &mut parsed.metadata,
                kind,
                payload,
                chunk.payload_range.start,
            )?;
        }
    }
    crate::decode::validate_png_stream(
        &compressed,
        u64::from(inspection.width) * u64::from(inspection.height),
        data_offset,
        checkpoint,
    )?;
    Ok(parsed)
}

fn jpeg(
    input: &[u8],
    config: &Config,
    checkpoint: &mut impl FnMut() -> Result<(), ForensicError>,
) -> Result<Parsed, ForensicError> {
    let limits = oia_jpeg_privacy::Limits {
        input_bytes: config.max_input_bytes,
        output_bytes: config.max_input_bytes,
        segments: config.max_container_items,
        metadata_bytes: config.max_metadata_bytes,
        width: config.max_dimension,
        height: config.max_dimension,
        pixels: config.max_pixels,
    };
    let mut stopped = None;
    let inspection = oia_jpeg_privacy::inspect(input, limits, || {
        checkpoint().map_err(|error| {
            stopped = Some(error);
            oia_jpeg_privacy::StopReason::Cancelled
        })
    });
    if let Some(error) = stopped {
        return Err(error);
    }
    let inspection = inspection.map_err(|error| match error {
        oia_jpeg_privacy::JpegError::Malformed { offset, reason } => {
            ForensicError::Malformed { offset, reason }
        }
        oia_jpeg_privacy::JpegError::ResourceLimit(oia_jpeg_privacy::Limit::Pixels) => {
            ForensicError::Limit("decoded pixels")
        }
        other => ForensicError::JpegContainer(other),
    })?;
    if inspection.unsupported_frame {
        return Err(ForensicError::Unsupported(
            "JPEG frame outside supported Huffman profile",
        ));
    }
    if let Some(trailing) = inspection.trailing_range {
        return Err(ForensicError::Malformed {
            offset: trailing.start,
            reason: "trailing JPEG bytes",
        });
    }
    if inspection.frame.marker == 0xc0 {
        let blocks = u64::from(inspection.frame.width).div_ceil(32)
            * u64::from(inspection.frame.height).div_ceil(32)
            * u64::from(inspection.frame.components);
        let entropy_bytes: u64 = inspection
            .segments
            .iter()
            .filter_map(|segment| segment.entropy_coded_range.as_ref())
            .map(|range| range.len() as u64)
            .sum();
        if entropy_bytes < blocks.div_ceil(4) {
            return Err(ForensicError::Malformed {
                offset: 0,
                reason: "baseline JPEG entropy below two-bit-per-block minimum",
            });
        }
    }
    let mut parsed = Parsed {
        format: ImageFormat::Jpeg,
        decode_bytes: Vec::new(),
        metadata: MetadataAnalysis::default(),
    };
    let mut scans = 0;
    for segment in inspection.segments {
        checkpoint()?;
        if segment.marker == 0xda {
            scans += 1;
            if scans > config.max_jpeg_scans {
                return Err(ForensicError::Limit("JPEG scans"));
            }
        }
        let payload = region(input, segment.payload_range.clone())?;
        if (0xe0..=0xef).contains(&segment.marker) || segment.marker == 0xfe {
            metadata::jpeg_field(
                &mut parsed.metadata,
                segment.marker,
                payload,
                segment.payload_range.start,
            );
        }
        if !(0xe1..=0xed).contains(&segment.marker)
            && segment.marker != 0xef
            && segment.marker != 0xfe
        {
            parsed
                .decode_bytes
                .extend_from_slice(region(input, segment.wire_range)?);
        }
    }
    Ok(parsed)
}
