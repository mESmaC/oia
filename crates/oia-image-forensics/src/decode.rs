use crate::{Config, ForensicError};
use flate2::{Decompress, FlushDecompress, Status};
use image::{DynamicImage, ImageFormat, ImageReader, RgbImage, RgbaImage};
use std::io::Cursor;
use zune_core::{bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions};
use zune_jpeg::JpegDecoder;

pub(crate) fn validate_png_stream(
    compressed: &[u8],
    pixels: u64,
    offset: usize,
    checkpoint: &mut impl FnMut() -> Result<(), ForensicError>,
) -> Result<(), ForensicError> {
    let mut decoder = Decompress::new(true);
    let mut buffer = [0_u8; 8192];
    loop {
        checkpoint()?;
        let previous_input = decoder.total_in();
        let previous_output = decoder.total_out();
        let consumed = usize::try_from(previous_input)
            .map_err(|_| ForensicError::Limit("PNG compressed bytes"))?;
        let remaining = compressed.get(consumed..).ok_or(ForensicError::Malformed {
            offset,
            reason: "PNG zlib input accounting",
        })?;
        let input_chunk = remaining.get(..buffer.len()).unwrap_or(remaining);
        let status = decoder
            .decompress(input_chunk, &mut buffer, FlushDecompress::None)
            .map_err(|_| ForensicError::Malformed {
                offset,
                reason: "invalid PNG zlib stream or Adler-32",
            })?;
        if decoder.total_out() > pixels.saturating_mul(5) {
            return Err(ForensicError::Limit("PNG inflated bytes"));
        }
        if status == Status::StreamEnd {
            if decoder.total_in() != compressed.len() as u64 {
                return Err(ForensicError::Malformed {
                    offset,
                    reason: "trailing bytes after PNG zlib stream",
                });
            }
            return Ok(());
        }
        if decoder.total_in() == previous_input && decoder.total_out() == previous_output {
            return Err(ForensicError::Malformed {
                offset,
                reason: "incomplete PNG zlib stream",
            });
        }
    }
}

fn dimensions(width: u32, height: u32, config: &Config) -> Result<(), ForensicError> {
    if width == 0 || height == 0 {
        return Err(ForensicError::Malformed {
            offset: 0,
            reason: "zero image dimension",
        });
    }
    if width > config.max_dimension || height > config.max_dimension {
        return Err(ForensicError::Limit("dimensions"));
    }
    if u64::from(width) * u64::from(height) > config.max_pixels {
        return Err(ForensicError::Limit("decoded pixels"));
    }
    Ok(())
}

pub(crate) fn raster(
    bytes: &[u8],
    format: ImageFormat,
    config: &Config,
) -> Result<RgbaImage, ForensicError> {
    if format == ImageFormat::Jpeg {
        let options = DecoderOptions::default()
            .set_strict_mode(true)
            .set_max_width(config.max_dimension as usize)
            .set_max_height(config.max_dimension as usize)
            .jpeg_set_max_scans(config.max_jpeg_scans)
            .jpeg_set_out_colorspace(ColorSpace::RGB);
        let mut decoder = JpegDecoder::new_with_options(ZCursor::new(bytes), options);
        decoder
            .decode_headers()
            .map_err(|error| ForensicError::Jpeg(error.to_string()))?;
        let information = decoder
            .info()
            .ok_or(ForensicError::Unsupported("JPEG header information"))?;
        let (width, height) = (u32::from(information.width), u32::from(information.height));
        dimensions(width, height, config)?;
        if information.components != 1 && information.components != 3 {
            return Err(ForensicError::Unsupported(
                "JPEG component count; CMYK/YCCK conversion is outside this profile",
            ));
        }
        let pixel_bytes = u64::from(width) * u64::from(height) * 7;
        let coefficient_bytes = u64::from(width).div_ceil(32)
            * 32
            * u64::from(height).div_ceil(32)
            * 32
            * u64::from(information.components)
            * 2;
        if pixel_bytes + coefficient_bytes > config.decoder_allocation_bytes {
            return Err(ForensicError::Limit(
                "JPEG output/conversion/coefficient bytes",
            ));
        }
        let decoded = decoder
            .decode()
            .map_err(|error| ForensicError::Jpeg(error.to_string()))?;
        let rgb = RgbImage::from_raw(width, height, decoded)
            .ok_or(ForensicError::Unsupported("JPEG output size"))?;
        return Ok(DynamicImage::ImageRgb8(rgb).into_rgba8());
    }
    let (width, height) = ImageReader::with_format(Cursor::new(bytes), format).into_dimensions()?;
    dimensions(width, height, config)?;
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(width);
    limits.max_image_height = Some(height);
    limits.max_alloc = Some(config.decoder_allocation_bytes);
    reader.limits(limits);
    Ok(reader.decode()?.into_rgba8())
}
