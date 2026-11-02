use crate::{Config, ForensicError};
use image::{DynamicImage, ImageFormat, ImageReader, RgbImage, RgbaImage};
use std::io::Cursor;
use zune_core::{bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions};
use zune_jpeg::JpegDecoder;

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
        if u64::from(width) * u64::from(height) * 7 > config.decoder_allocation_bytes {
            return Err(ForensicError::Limit("JPEG output/conversion bytes"));
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
