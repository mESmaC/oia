use super::{Control, FileInfo, FileKind, IMAGE_BYTES, Result, Snapshot, ToolError, require_kind};
use oia_jpeg_privacy as jpeg;
use oia_png_privacy as png;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize)]
pub struct MetadataRow {
    pub kind: String,
    pub start: usize,
    pub end: usize,
    pub handling: &'static str,
    pub target: Option<&'static str>,
}
#[derive(Clone, Debug, Serialize)]
pub struct PrivacyView {
    pub file: FileInfo,
    pub profile: &'static str,
    pub width: u32,
    pub height: u32,
    pub complete: bool,
    pub rows: Vec<MetadataRow>,
    pub trailing_bytes: usize,
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    #[serde(default)]
    pub text: bool,
    #[serde(default)]
    pub modification_time: bool,
    #[serde(default)]
    pub exif: bool,
    #[serde(default)]
    pub comment: bool,
    #[serde(default)]
    pub xmp: bool,
    #[serde(default)]
    pub photoshop_irb: bool,
}
#[derive(Debug)]
pub struct CleanedImage {
    pub bytes: Vec<u8>,
    pub removed: usize,
}

fn png_limits() -> png::Limits {
    png::Limits {
        input_bytes: IMAGE_BYTES,
        output_bytes: IMAGE_BYTES,
        chunk_bytes: IMAGE_BYTES,
        chunks: 4096,
        metadata_bytes: 2 * 1024 * 1024,
        width: 32_768,
        height: 32_768,
        pixels: 100_000_000,
    }
}
fn jpeg_limits() -> jpeg::Limits {
    jpeg::Limits {
        input_bytes: IMAGE_BYTES,
        output_bytes: IMAGE_BYTES,
        segments: 4096,
        metadata_bytes: 2 * 1024 * 1024,
        width: 32_768,
        height: 32_768,
        pixels: 100_000_000,
    }
}
fn png_check(c: &Control) -> std::result::Result<(), png::StopReason> {
    c.check().map_err(|e| {
        if e.code == "cancelled" {
            png::StopReason::Cancelled
        } else {
            png::StopReason::DeadlineExceeded
        }
    })
}
fn jpeg_check(c: &Control) -> std::result::Result<(), jpeg::StopReason> {
    c.check().map_err(|e| {
        if e.code == "cancelled" {
            jpeg::StopReason::Cancelled
        } else {
            jpeg::StopReason::DeadlineExceeded
        }
    })
}
fn png_target(t: png::Target) -> &'static str {
    match t {
        png::Target::Text => "text",
        png::Target::ModificationTime => "modification_time",
        png::Target::Exif => "exif",
    }
}
fn jpeg_target(t: jpeg::Target) -> &'static str {
    match t {
        jpeg::Target::Comment => "comment",
        jpeg::Target::Exif => "exif",
        jpeg::Target::Xmp => "xmp",
        jpeg::Target::PhotoshopIrb => "photoshop_irb",
    }
}

pub fn inspect_image(snapshot: &Snapshot, control: &Control) -> Result<PrivacyView> {
    require_kind(snapshot, FileKind::Image)?;
    control.check()?;
    let bytes = snapshot.bytes();
    let view = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        let inspection = png::inspect(bytes, png_limits(), || png_check(control))
            .map_err(|e| ToolError::new("png_inspection", e))?;
        PrivacyView {
            file: snapshot.info.clone(),
            profile: png::PROFILE,
            width: inspection.width,
            height: inspection.height,
            complete: inspection.coverage == png::Coverage::CompleteContainerProfile,
            trailing_bytes: 0,
            rows: inspection
                .chunks
                .iter()
                .map(|c| {
                    let (handling, target) = match c.handling {
                        png::Handling::Retained => ("retained", None),
                        png::Handling::Removable(t) => ("removable", Some(png_target(t))),
                        png::Handling::RequiredForRendering(t) => {
                            ("required_for_rendering", Some(png_target(t)))
                        }
                        png::Handling::Unsupported => ("unsupported", None),
                    };
                    MetadataRow {
                        kind: String::from_utf8_lossy(&c.kind).into_owned(),
                        start: c.wire_range.start,
                        end: c.wire_range.end,
                        handling,
                        target,
                    }
                })
                .collect(),
        }
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        let inspection = jpeg::inspect(bytes, jpeg_limits(), || jpeg_check(control))
            .map_err(|e| ToolError::new("jpeg_inspection", e))?;
        PrivacyView {
            file: snapshot.info.clone(),
            profile: jpeg::PROFILE,
            width: u32::from(inspection.frame.width),
            height: u32::from(inspection.frame.height),
            complete: inspection.coverage == jpeg::Coverage::CompleteContainerProfile,
            trailing_bytes: inspection.trailing_range.as_ref().map_or(0, |r| r.len()),
            rows: inspection
                .segments
                .iter()
                .map(|s| {
                    let (handling, target) = match s.handling {
                        jpeg::Handling::Retained => ("retained", None),
                        jpeg::Handling::Removable(t) => ("removable", Some(jpeg_target(t))),
                        jpeg::Handling::RequiredForRendering(t) => {
                            ("required_for_rendering", Some(jpeg_target(t)))
                        }
                        jpeg::Handling::Unsupported => ("unsupported", None),
                    };
                    MetadataRow {
                        kind: format!("{:?}", s.kind),
                        start: s.wire_range.start,
                        end: s.wire_range.end,
                        handling,
                        target,
                    }
                })
                .collect(),
        }
    } else {
        return Err(ToolError::new(
            "unsupported_format",
            "This tool supports PNG and JPEG containers only.",
        ));
    };
    control.check()?;
    Ok(view)
}

pub fn clean_image(
    snapshot: &Snapshot,
    selection: &Selection,
    control: &Control,
) -> Result<CleanedImage> {
    let view = inspect_image(snapshot, control)?;
    let chosen = |target: &str| match target {
        "text" => selection.text,
        "modification_time" => selection.modification_time,
        "exif" => selection.exif,
        "comment" => selection.comment,
        "xmp" => selection.xmp,
        "photoshop_irb" => selection.photoshop_irb,
        _ => false,
    };
    if !view.complete {
        return Err(ToolError::new(
            "partial_profile",
            "Cleanup is unavailable: this file contains regions outside the supported profile.",
        ));
    }
    if !view
        .rows
        .iter()
        .any(|r| r.handling == "removable" && r.target.is_some_and(chosen))
    {
        return Err(ToolError::new(
            "empty_selection",
            "Select at least one removable metadata category.",
        ));
    }
    let result = if snapshot.bytes().starts_with(b"\x89PNG") {
        let cleaned = png::clean(
            snapshot.bytes(),
            png::Selection {
                text: selection.text,
                modification_time: selection.modification_time,
                exif: selection.exif,
            },
            png_limits(),
            || png_check(control),
        )
        .map_err(|e| ToolError::new("png_cleanup", e))?;
        CleanedImage {
            bytes: cleaned.bytes().to_vec(),
            removed: cleaned.removed_chunks(),
        }
    } else {
        let cleaned = jpeg::clean(
            snapshot.bytes(),
            jpeg::Selection {
                comment: selection.comment,
                exif: selection.exif,
                xmp: selection.xmp,
                photoshop_irb: selection.photoshop_irb,
            },
            jpeg_limits(),
            || jpeg_check(control),
        )
        .map_err(|e| ToolError::new("jpeg_cleanup", e))?;
        CleanedImage {
            bytes: cleaned.bytes().to_vec(),
            removed: cleaned.removed_segments(),
        }
    };
    control.check()?;
    Ok(result)
}
