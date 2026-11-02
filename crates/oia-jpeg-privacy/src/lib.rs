//! Bounded JPEG marker/segment container privacy kernel.
//!
//! The profile walks the ITU-T T.81 (ISO/IEC 10918-1) interchange-format
//! container. It validates segment structure and table/frame/scan headers, and
//! preserves entropy-coded data byte for byte. It is not an image decoder.
//! See `docs/implementation/jpeg-privacy.md` for the profile and its limits.

use std::error::Error;
use std::fmt;
use std::ops::Range;

pub const PROFILE: &str = "oia-jpeg-container-huffman-v1";
const SOI: [u8; 2] = [0xff, 0xd8];
const CONTROL_BYTES: usize = 4096;
const EXIF_ID: &[u8] = b"Exif\x00\x00";
const XMP_ID: &[u8] = b"http://ns.adobe.com/xap/1.0/\x00";
const EXTENDED_XMP_ID: &[u8] = b"http://ns.adobe.com/xmp/extension/\x00";
const PHOTOSHOP_ID: &[u8] = b"Photoshop 3.0\x00";
const ICC_ID: &[u8] = b"ICC_PROFILE\x00";
const JFIF_ID: &[u8] = b"JFIF\x00";
const ADOBE_ID: &[u8] = b"Adobe";
/// XMP property names and namespace URIs that may carry rendering semantics.
/// An XMP packet containing any of them is not offered for removal.
const XMP_RENDERING_GUARDS: [&[u8]; 2] = [b"Orientation", b"hdr-gain-map"];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Limit {
    InputBytes,
    OutputBytes,
    Segments,
    MetadataBytes,
    Width,
    Height,
    Pixels,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub input_bytes: usize,
    pub output_bytes: usize,
    pub segments: usize,
    pub metadata_bytes: usize,
    pub width: u32,
    pub height: u32,
    pub pixels: u64,
}

impl Limits {
    fn validate(self) -> Result<(), JpegError> {
        for (value, limit) in [
            (self.input_bytes as u64, Limit::InputBytes),
            (self.output_bytes as u64, Limit::OutputBytes),
            (self.segments as u64, Limit::Segments),
            (self.metadata_bytes as u64, Limit::MetadataBytes),
            (u64::from(self.width), Limit::Width),
            (u64::from(self.height), Limit::Height),
            (self.pixels, Limit::Pixels),
        ] {
            if value == 0 {
                return Err(JpegError::ZeroLimit(limit));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopReason {
    Cancelled,
    DeadlineExceeded,
    Superseded,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JpegError {
    ZeroLimit(Limit),
    ResourceLimit(Limit),
    UnsupportedFormat,
    Malformed { offset: usize, reason: &'static str },
    UnsupportedProfile,
    RequiredForRendering(Target),
    Stopped(StopReason),
    AllocationFailed,
    VerificationFailed,
}

impl fmt::Display for JpegError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroLimit(limit) => write!(formatter, "{limit:?} limit must be nonzero"),
            Self::ResourceLimit(limit) => write!(formatter, "JPEG exceeds {limit:?} budget"),
            Self::UnsupportedFormat => write!(formatter, "input is not a JPEG container"),
            Self::Malformed { offset, reason } => {
                write!(
                    formatter,
                    "invalid JPEG structure at byte {offset}: {reason}"
                )
            }
            Self::UnsupportedProfile => write!(formatter, "JPEG has regions outside {PROFILE}"),
            Self::RequiredForRendering(target) => write!(
                formatter,
                "selected {target:?} segment carries rendering information; removal refused"
            ),
            Self::Stopped(reason) => write!(formatter, "JPEG operation stopped: {reason:?}"),
            Self::AllocationFailed => write!(formatter, "JPEG operation could not allocate memory"),
            Self::VerificationFailed => write!(formatter, "JPEG container verification failed"),
        }
    }
}

impl Error for JpegError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    Comment,
    Exif,
    Xmp,
    PhotoshopIrb,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Handling {
    Retained,
    Removable(Target),
    /// Recognized segment whose removal would change rendering; retained or refused.
    RequiredForRendering(Target),
    Unsupported,
}

/// Structural classification of a segment. It names the container, not its contents.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SegmentKind {
    StartOfImage,
    EndOfImage,
    Frame,
    HuffmanTables,
    QuantizationTables,
    RestartInterval,
    Scan,
    Comment,
    Jfif,
    Exif,
    Xmp,
    ExtendedXmp,
    PhotoshopIrb,
    IccProfile,
    Adobe,
    OtherApplication,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Segment {
    pub marker: u8,
    pub kind: SegmentKind,
    /// Leading fill bytes, marker, length, payload and (for a scan) entropy-coded data.
    pub wire_range: Range<usize>,
    /// Bytes after the two-byte length field; empty for standalone markers.
    pub payload_range: Range<usize>,
    /// Entropy-coded data following a scan header, including stuffing and restart markers.
    pub entropy_coded_range: Option<Range<usize>>,
    pub handling: Handling,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Coverage {
    CompleteContainerProfile,
    Partial,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Frame {
    /// SOFn marker byte (0xc0 to 0xcf, excluding DHT, JPG and DAC).
    pub marker: u8,
    pub precision: u8,
    pub width: u16,
    /// Zero when the number of lines is deferred to a DNL segment.
    pub height: u16,
    pub components: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Inspection {
    pub segments: Vec<Segment>,
    pub coverage: Coverage,
    pub frame: Frame,
    /// Frame process outside the profile (arithmetic, lossless, hierarchical or DNL height).
    pub unsupported_frame: bool,
    /// Bytes after EOI. They are opaque to this profile and make coverage Partial.
    pub trailing_range: Option<Range<usize>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Selection {
    pub comment: bool,
    pub exif: bool,
    pub xmp: bool,
    pub photoshop_irb: bool,
}

impl Selection {
    fn selects(self, target: Target) -> bool {
        match target {
            Target::Comment => self.comment,
            Target::Exif => self.exif,
            Target::Xmp => self.xmp,
            Target::PhotoshopIrb => self.photoshop_irb,
        }
    }

    fn removes(self, handling: Handling) -> bool {
        matches!(handling, Handling::Removable(target) if self.selects(target))
    }
}

pub struct Cleaned {
    bytes: Vec<u8>,
    inspection: Inspection,
    removed_segments: usize,
}

impl fmt::Debug for Cleaned {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Cleaned")
            .field("byte_length", &self.bytes.len())
            .field("inspection", &self.inspection)
            .field("removed_segments", &self.removed_segments)
            .finish()
    }
}

impl Cleaned {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn inspection(&self) -> &Inspection {
        &self.inspection
    }

    pub fn removed_segments(&self) -> usize {
        self.removed_segments
    }
}

fn malformed(offset: usize, reason: &'static str) -> JpegError {
    JpegError::Malformed { offset, reason }
}

fn bytes_at(input: &[u8], range: Range<usize>) -> Result<&[u8], JpegError> {
    let offset = range.start;
    input
        .get(range)
        .ok_or_else(|| malformed(offset, "truncated segment"))
}

fn byte(input: &[u8], offset: usize, segment: usize) -> Result<u8, JpegError> {
    input
        .get(offset)
        .copied()
        .ok_or_else(|| malformed(segment, "truncated segment"))
}

fn be16(input: &[u8], offset: usize, segment: usize) -> Result<u16, JpegError> {
    let end = offset
        .checked_add(2)
        .ok_or_else(|| malformed(segment, "offset overflow"))?;
    let bytes = input
        .get(offset..end)
        .ok_or_else(|| malformed(segment, "truncated segment"))?;
    let array = <[u8; 2]>::try_from(bytes).map_err(|_| malformed(segment, "invalid field"))?;
    Ok(u16::from_be_bytes(array))
}

fn stop(check_control: &mut impl FnMut() -> Result<(), StopReason>) -> Result<(), JpegError> {
    check_control().map_err(JpegError::Stopped)
}

/// Orientation as recorded in IFD0 of an Exif TIFF structure (Exif tag 0x0112).
enum Orientation {
    AbsentOrDefault,
    NonDefault,
    Uninterpretable,
}

/// Bounded read of the TIFF header and IFD0 that looks only for Orientation.
/// No other tag values are decoded; offsets are relative to the TIFF header,
/// which follows the six-byte `Exif\0\0` identifier code in APP1.
fn exif_orientation(
    tiff: &[u8],
    check_control: &mut impl FnMut() -> Result<(), StopReason>,
) -> Result<Orientation, JpegError> {
    let big_endian = match tiff.get(..4) {
        Some(b"MM\x00\x2a") => true,
        Some(b"II\x2a\x00") => false,
        _ => return Ok(Orientation::Uninterpretable),
    };
    let u16_at = |offset: usize| -> Option<u16> {
        let bytes = <[u8; 2]>::try_from(tiff.get(offset..offset.checked_add(2)?)?).ok()?;
        Some(if big_endian {
            u16::from_be_bytes(bytes)
        } else {
            u16::from_le_bytes(bytes)
        })
    };
    let u32_at = |offset: usize| -> Option<u32> {
        let bytes = <[u8; 4]>::try_from(tiff.get(offset..offset.checked_add(4)?)?).ok()?;
        Some(if big_endian {
            u32::from_be_bytes(bytes)
        } else {
            u32::from_le_bytes(bytes)
        })
    };
    let Some(ifd) = u32_at(4).and_then(|offset| usize::try_from(offset).ok()) else {
        return Ok(Orientation::Uninterpretable);
    };
    let Some(count) = u16_at(ifd).filter(|_| ifd >= 8) else {
        return Ok(Orientation::Uninterpretable);
    };
    // u16_at(ifd) succeeded, so ifd + 2 <= tiff.len() and cannot overflow.
    let entries = ifd + 2;
    let entries_fit = usize::from(count)
        .checked_mul(12)
        .and_then(|length| entries.checked_add(length))
        .is_some_and(|end| end <= tiff.len());
    if !entries_fit {
        return Ok(Orientation::Uninterpretable);
    }
    let mut orientation = None;
    for index in 0..usize::from(count) {
        if index % (CONTROL_BYTES / 12) == 0 {
            stop(check_control)?;
        }
        let entry = entries + index * 12;
        if u16_at(entry) != Some(0x0112) {
            continue;
        }
        if orientation.is_some() || u16_at(entry + 2) != Some(3) || u32_at(entry + 4) != Some(1) {
            return Ok(Orientation::Uninterpretable);
        }
        orientation = u16_at(entry + 8);
    }
    Ok(match orientation {
        None | Some(1) => Orientation::AbsentOrDefault,
        Some(2..=8) => Orientation::NonDefault,
        Some(_) => Orientation::Uninterpretable,
    })
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// Walks the Photoshop image-resource blocks of one APP13 segment.
/// Resources 0x0422/0x0423 (embedded Exif copies) and 0x0424 (embedded XMP)
/// could duplicate rendering fields, so they make the segment Unsupported,
/// as does any block that does not end inside this segment.
fn photoshop_handling(
    resources: &[u8],
    check_control: &mut impl FnMut() -> Result<(), StopReason>,
) -> Result<Handling, JpegError> {
    let mut position = 0usize;
    while position < resources.len() {
        stop(check_control)?;
        let parsed = (|| -> Option<(u16, usize)> {
            if resources.get(position..position.checked_add(4)?)? != b"8BIM" {
                return None;
            }
            let id_at = position.checked_add(4)?;
            let id =
                u16::from_be_bytes(<[u8; 2]>::try_from(resources.get(id_at..id_at + 2)?).ok()?);
            let name_at = id_at.checked_add(2)?;
            let name_length = usize::from(*resources.get(name_at)?);
            // Pascal string: length byte plus characters, padded to an even total.
            let name_total = (name_length + 1).next_multiple_of(2);
            let size_at = name_at.checked_add(name_total)?;
            let size = u32::from_be_bytes(
                <[u8; 4]>::try_from(resources.get(size_at..size_at.checked_add(4)?)?).ok()?,
            );
            let data_end = size_at
                .checked_add(4)?
                .checked_add(usize::try_from(size).ok()?)?;
            if data_end > resources.len() {
                return None;
            }
            // Resource data is padded to an even length; tolerate a missing final pad.
            Some((id, data_end.next_multiple_of(2).min(resources.len())))
        })();
        let Some((id, next)) = parsed else {
            return Ok(Handling::Unsupported);
        };
        if matches!(id, 0x0422..=0x0424) {
            return Ok(Handling::Unsupported);
        }
        position = next;
    }
    Ok(Handling::Removable(Target::PhotoshopIrb))
}

fn validate_quantization_tables(payload: &[u8], offset: usize) -> Result<(), JpegError> {
    let mut position = 0usize;
    if payload.is_empty() {
        return Err(malformed(offset, "empty quantization-table segment"));
    }
    while position < payload.len() {
        let header = byte(payload, position, offset)?;
        let (precision, destination) = (header >> 4, header & 0x0f);
        if precision > 1 || destination > 3 {
            return Err(malformed(offset, "invalid quantization-table header"));
        }
        let length = if precision == 0 { 64 } else { 128 };
        position = position
            .checked_add(1 + length)
            .filter(|end| *end <= payload.len())
            .ok_or_else(|| malformed(offset, "truncated quantization table"))?;
    }
    Ok(())
}

fn validate_huffman_tables(payload: &[u8], offset: usize) -> Result<(), JpegError> {
    let mut position = 0usize;
    if payload.is_empty() {
        return Err(malformed(offset, "empty Huffman-table segment"));
    }
    while position < payload.len() {
        let header = byte(payload, position, offset)?;
        if header >> 4 > 1 || header & 0x0f > 3 {
            return Err(malformed(offset, "invalid Huffman-table header"));
        }
        let counts_end = position
            .checked_add(17)
            .ok_or_else(|| malformed(offset, "offset overflow"))?;
        let counts = payload
            .get(position + 1..counts_end)
            .ok_or_else(|| malformed(offset, "truncated Huffman table"))?;
        let values: usize = counts.iter().map(|count| usize::from(*count)).sum();
        if values == 0 || values > 256 {
            return Err(malformed(offset, "invalid Huffman code count"));
        }
        position = counts_end
            .checked_add(values)
            .filter(|end| *end <= payload.len())
            .ok_or_else(|| malformed(offset, "truncated Huffman table"))?;
    }
    Ok(())
}

/// Parsed frame plus its component identifiers, used to validate scans.
struct FrameState {
    frame: Frame,
    component_ids: Vec<u8>,
    supported: bool,
}

fn parse_frame(
    marker: u8,
    payload: &[u8],
    offset: usize,
    limits: Limits,
) -> Result<FrameState, JpegError> {
    let precision = byte(payload, 0, offset)?;
    let height = be16(payload, 1, offset)?;
    let width = be16(payload, 3, offset)?;
    let components = byte(payload, 5, offset)?;
    let supported_process = matches!(marker, 0xc0..=0xc2);
    let expected = 6 + 3 * usize::from(components);
    if components == 0 || payload.len() != expected {
        return Err(malformed(offset, "invalid frame header length"));
    }
    let valid_precision = match marker {
        0xc0 => precision == 8,
        0xc1 | 0xc2 => matches!(precision, 8 | 12),
        _ => (2..=16).contains(&precision),
    };
    if !valid_precision {
        return Err(malformed(offset, "invalid sample precision"));
    }
    if width == 0 {
        return Err(malformed(offset, "invalid frame width"));
    }
    if marker == 0xc2 && components > 4 {
        return Err(malformed(
            offset,
            "progressive frame has more than four components",
        ));
    }
    if u32::from(width) > limits.width {
        return Err(JpegError::ResourceLimit(Limit::Width));
    }
    if u32::from(height) > limits.height {
        return Err(JpegError::ResourceLimit(Limit::Height));
    }
    if u64::from(width) * u64::from(height) > limits.pixels {
        return Err(JpegError::ResourceLimit(Limit::Pixels));
    }
    let mut component_ids = Vec::new();
    component_ids
        .try_reserve_exact(usize::from(components))
        .map_err(|_| JpegError::AllocationFailed)?;
    for index in 0..usize::from(components) {
        let base = 6 + 3 * index;
        let id = byte(payload, base, offset)?;
        let sampling = byte(payload, base + 1, offset)?;
        let table = byte(payload, base + 2, offset)?;
        if !(1..=4).contains(&(sampling >> 4)) || !(1..=4).contains(&(sampling & 0x0f)) || table > 3
        {
            return Err(malformed(offset, "invalid frame component"));
        }
        if component_ids.contains(&id) {
            return Err(malformed(offset, "duplicate frame component identifier"));
        }
        component_ids.push(id);
    }
    Ok(FrameState {
        frame: Frame {
            marker,
            precision,
            width,
            height,
            components,
        },
        component_ids,
        // A zero height defers the line count to DNL, which this profile does not cover.
        supported: supported_process && height != 0,
    })
}

fn validate_scan_header(
    payload: &[u8],
    offset: usize,
    frame: &FrameState,
) -> Result<(), JpegError> {
    let count = byte(payload, 0, offset)?;
    if !(1..=4).contains(&count) || payload.len() != 1 + 2 * usize::from(count) + 3 {
        return Err(malformed(offset, "invalid scan header length"));
    }
    let mut seen = [0u8; 4];
    for index in 0..usize::from(count) {
        let selector = byte(payload, 1 + 2 * index, offset)?;
        let tables = byte(payload, 2 + 2 * index, offset)?;
        if !frame.component_ids.contains(&selector)
            || seen.get(..index).is_some_and(|ids| ids.contains(&selector))
        {
            return Err(malformed(offset, "invalid scan component selector"));
        }
        if let Some(slot) = seen.get_mut(index) {
            *slot = selector;
        }
        if tables >> 4 > 3 || tables & 0x0f > 3 {
            return Err(malformed(offset, "invalid scan table selector"));
        }
    }
    let tail = 1 + 2 * usize::from(count);
    let start = byte(payload, tail, offset)?;
    let end = byte(payload, tail + 1, offset)?;
    let approximation = byte(payload, tail + 2, offset)?;
    let (high, low) = (approximation >> 4, approximation & 0x0f);
    let valid = match frame.frame.marker {
        0xc0 | 0xc1 => start == 0 && end == 63 && approximation == 0,
        0xc2 => {
            start <= end
                && end <= 63
                && ((start == 0) == (end == 0))
                && (start == 0 || count == 1)
                && high <= 13
                && low <= 13
        }
        _ => true,
    };
    if !valid {
        return Err(malformed(
            offset,
            "invalid scan spectral or approximation parameters",
        ));
    }
    Ok(())
}

/// Finds the end of entropy-coded data that starts at `start`. Stuffed zero
/// bytes and restart markers (optionally after fill bytes) stay inside it;
/// any other marker ends it. The returned offset is the first byte of that
/// marker's fill/marker sequence.
fn entropy_coded_end(
    input: &[u8],
    start: usize,
    check_control: &mut impl FnMut() -> Result<(), StopReason>,
) -> Result<usize, JpegError> {
    let mut position = start;
    let mut since_check = 0usize;
    loop {
        let rest = input
            .get(position..)
            .ok_or_else(|| malformed(position, "unterminated entropy-coded data"))?;
        let window = rest
            .len()
            .min(CONTROL_BYTES.saturating_sub(since_check).max(1));
        let found = rest
            .get(..window)
            .and_then(|block| block.iter().position(|b| *b == 0xff));
        let Some(relative) = found else {
            if window == rest.len() {
                return Err(malformed(start, "unterminated entropy-coded data"));
            }
            position += window;
            stop(check_control)?;
            since_check = 0;
            continue;
        };
        since_check += relative + 1;
        if since_check >= CONTROL_BYTES {
            stop(check_control)?;
            since_check = 0;
        }
        let marker_start = position + relative;
        let mut next = marker_start + 1;
        while input.get(next) == Some(&0xff) {
            next += 1;
            since_check += 1;
            if since_check >= CONTROL_BYTES {
                stop(check_control)?;
                since_check = 0;
            }
        }
        match input.get(next) {
            None => return Err(malformed(start, "unterminated entropy-coded data")),
            Some(0x00) if next == marker_start + 1 => position = next + 1,
            Some(0x00) => return Err(malformed(next, "stuffed byte after fill bytes")),
            Some(0xd0..=0xd7) => position = next + 1,
            Some(_) => return Ok(marker_start),
        }
    }
}

fn classify_application(
    marker: u8,
    payload: &[u8],
    check_control: &mut impl FnMut() -> Result<(), StopReason>,
) -> Result<(SegmentKind, Handling), JpegError> {
    Ok(match marker {
        0xe0 if payload.starts_with(JFIF_ID) => {
            // Identifier, version (2), units, X/Y density (2 each), thumbnail
            // width and height, then 3 bytes per thumbnail pixel. Only an
            // exactly sized header with no thumbnail is retained as inspected.
            // An embedded thumbnail is a preview that cannot be removed without
            // rewriting the segment, and a nonconforming header is opaque; both
            // are Unsupported rather than fatal to inspection.
            let conforming_without_thumbnail = payload.len() == 14
                && payload.get(7).is_some_and(|units| *units <= 2)
                && payload.get(12..14) == Some(&[0, 0]);
            let handling = if conforming_without_thumbnail {
                Handling::Retained
            } else {
                Handling::Unsupported
            };
            (SegmentKind::Jfif, handling)
        }
        0xe1 if payload.starts_with(EXIF_ID) => {
            let tiff = payload.get(EXIF_ID.len()..).unwrap_or_default();
            let handling = match exif_orientation(tiff, check_control)? {
                Orientation::AbsentOrDefault => Handling::Removable(Target::Exif),
                Orientation::NonDefault => Handling::RequiredForRendering(Target::Exif),
                Orientation::Uninterpretable => Handling::Unsupported,
            };
            (SegmentKind::Exif, handling)
        }
        0xe1 if payload.starts_with(XMP_ID) => {
            stop(check_control)?;
            let guarded = XMP_RENDERING_GUARDS
                .iter()
                .any(|needle| contains(payload, needle));
            let handling = if guarded {
                Handling::Unsupported
            } else {
                Handling::Removable(Target::Xmp)
            };
            (SegmentKind::Xmp, handling)
        }
        // Extended XMP portions can split a property name across segments,
        // so no single segment can be checked for rendering properties.
        0xe1 if payload.starts_with(EXTENDED_XMP_ID) => {
            (SegmentKind::ExtendedXmp, Handling::Unsupported)
        }
        0xe2 if payload.starts_with(ICC_ID) => (SegmentKind::IccProfile, Handling::Unsupported),
        0xed if payload.starts_with(PHOTOSHOP_ID) => {
            let resources = payload.get(PHOTOSHOP_ID.len()..).unwrap_or_default();
            (
                SegmentKind::PhotoshopIrb,
                photoshop_handling(resources, check_control)?,
            )
        }
        0xee if payload.starts_with(ADOBE_ID) => {
            // DCTEncode version, two flag words and the color transform (0 to 2).
            let handling = if payload.len() == 12 && payload.get(11).is_some_and(|t| *t <= 2) {
                Handling::Retained
            } else {
                Handling::Unsupported
            };
            (SegmentKind::Adobe, handling)
        }
        _ => (SegmentKind::OtherApplication, Handling::Unsupported),
    })
}

pub fn inspect(
    input: &[u8],
    limits: Limits,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<Inspection, JpegError> {
    stop(&mut check_control)?;
    limits.validate()?;
    if input.len() > limits.input_bytes {
        return Err(JpegError::ResourceLimit(Limit::InputBytes));
    }
    if input.get(..2) != Some(&SOI) {
        return Err(JpegError::UnsupportedFormat);
    }
    let mut segments = Vec::new();
    segments
        .try_reserve(1)
        .map_err(|_| JpegError::AllocationFailed)?;
    segments.push(Segment {
        marker: 0xd8,
        kind: SegmentKind::StartOfImage,
        wire_range: 0..2,
        payload_range: 2..2,
        entropy_coded_range: None,
        handling: Handling::Retained,
    });
    let mut offset = 2usize;
    let mut metadata_bytes = 0usize;
    let mut frame: Option<FrameState> = None;
    let mut scans = 0usize;
    let mut partial = false;
    let trailing_range;
    loop {
        stop(&mut check_control)?;
        if segments.len() >= limits.segments {
            return Err(JpegError::ResourceLimit(Limit::Segments));
        }
        let start = offset;
        if input.get(offset) != Some(&0xff) {
            return Err(if offset >= input.len() {
                malformed(offset, "missing EOI")
            } else {
                malformed(offset, "expected marker")
            });
        }
        // Any marker may be preceded by fill bytes (T.81 B.1.1.2).
        while input.get(offset) == Some(&0xff) {
            offset += 1;
            if (offset - start).is_multiple_of(CONTROL_BYTES) {
                stop(&mut check_control)?;
            }
        }
        let marker = byte(input, offset, start).map_err(|_| malformed(start, "missing EOI"))?;
        offset += 1;
        let standalone = |kind, handling| Segment {
            marker,
            kind,
            wire_range: start..offset,
            payload_range: offset..offset,
            entropy_coded_range: None,
            handling,
        };
        let segment = match marker {
            0x00 => return Err(malformed(start, "stuffed byte outside entropy-coded data")),
            0xd8 => return Err(malformed(start, "duplicate SOI")),
            0xd0..=0xd7 => {
                return Err(malformed(
                    start,
                    "restart marker outside entropy-coded data",
                ));
            }
            0x02..=0xbf => return Err(malformed(start, "reserved marker")),
            0xd9 => {
                if frame.is_none() || scans == 0 {
                    return Err(malformed(start, "EOI before any scan"));
                }
                trailing_range = (offset < input.len()).then_some(offset..input.len());
                segments
                    .try_reserve(1)
                    .map_err(|_| JpegError::AllocationFailed)?;
                segments.push(standalone(SegmentKind::EndOfImage, Handling::Retained));
                break;
            }
            0x01 => standalone(SegmentKind::Other, Handling::Unsupported),
            _ => {
                let length = usize::from(be16(input, offset, start)?);
                if length < 2 {
                    return Err(malformed(start, "segment length below two"));
                }
                let payload_start = offset + 2;
                let end = offset
                    .checked_add(length)
                    .ok_or_else(|| malformed(start, "offset overflow"))?;
                let payload = bytes_at(input, payload_start..end)?;
                let (kind, handling) = match marker {
                    0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf => {
                        if frame.is_some() {
                            return Err(malformed(start, "multiple frames"));
                        }
                        let state = parse_frame(marker, payload, start, limits)?;
                        let handling = if state.supported {
                            Handling::Retained
                        } else {
                            Handling::Unsupported
                        };
                        frame = Some(state);
                        (SegmentKind::Frame, handling)
                    }
                    0xc4 => {
                        validate_huffman_tables(payload, start)?;
                        (SegmentKind::HuffmanTables, Handling::Retained)
                    }
                    0xdb => {
                        validate_quantization_tables(payload, start)?;
                        (SegmentKind::QuantizationTables, Handling::Retained)
                    }
                    0xdd => {
                        if payload.len() != 2 {
                            return Err(malformed(start, "invalid restart-interval length"));
                        }
                        (SegmentKind::RestartInterval, Handling::Retained)
                    }
                    0xda => {
                        let state = frame
                            .as_ref()
                            .ok_or_else(|| malformed(start, "scan before frame"))?;
                        validate_scan_header(payload, start, state)?;
                        (SegmentKind::Scan, Handling::Retained)
                    }
                    0xfe => (SegmentKind::Comment, Handling::Removable(Target::Comment)),
                    0xe0..=0xef => classify_application(marker, payload, &mut check_control)?,
                    // JPG, DAC, DNL, DHP, EXP and JPGn extensions are outside the profile.
                    _ => (SegmentKind::Other, Handling::Unsupported),
                };
                if !matches!(
                    kind,
                    SegmentKind::Frame
                        | SegmentKind::HuffmanTables
                        | SegmentKind::QuantizationTables
                        | SegmentKind::RestartInterval
                        | SegmentKind::Scan
                ) {
                    metadata_bytes = metadata_bytes
                        .checked_add(payload.len())
                        .ok_or(JpegError::ResourceLimit(Limit::MetadataBytes))?;
                    if metadata_bytes > limits.metadata_bytes {
                        return Err(JpegError::ResourceLimit(Limit::MetadataBytes));
                    }
                }
                let (wire_end, entropy_coded_range) = if kind == SegmentKind::Scan {
                    scans += 1;
                    let data_end = entropy_coded_end(input, end, &mut check_control)?;
                    (data_end, Some(end..data_end))
                } else {
                    (end, None)
                };
                offset = wire_end;
                Segment {
                    marker,
                    kind,
                    wire_range: start..wire_end,
                    payload_range: payload_start..end,
                    entropy_coded_range,
                    handling,
                }
            }
        };
        if segment.handling == Handling::Unsupported {
            partial = true;
        }
        segments
            .try_reserve(1)
            .map_err(|_| JpegError::AllocationFailed)?;
        segments.push(segment);
    }
    stop(&mut check_control)?;
    let frame = frame.ok_or_else(|| malformed(offset, "missing frame"))?;
    if !frame.supported || trailing_range.is_some() {
        partial = true;
    }
    Ok(Inspection {
        segments,
        coverage: if partial {
            Coverage::Partial
        } else {
            Coverage::CompleteContainerProfile
        },
        frame: frame.frame,
        unsupported_frame: !frame.supported,
        trailing_range,
    })
}

pub fn clean(
    input: &[u8],
    selection: Selection,
    limits: Limits,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<Cleaned, JpegError> {
    let original = inspect(input, limits, &mut check_control)?;
    if original.coverage != Coverage::CompleteContainerProfile {
        return Err(JpegError::UnsupportedProfile);
    }
    for segment in &original.segments {
        if let Handling::RequiredForRendering(target) = segment.handling
            && selection.selects(target)
        {
            return Err(JpegError::RequiredForRendering(target));
        }
    }
    let mut output_size = 0usize;
    let mut removed_segments = 0;
    for segment in &original.segments {
        stop(&mut check_control)?;
        if selection.removes(segment.handling) {
            removed_segments += 1;
        } else {
            output_size = output_size
                .checked_add(segment.wire_range.len())
                .ok_or(JpegError::ResourceLimit(Limit::OutputBytes))?;
        }
    }
    if output_size > limits.output_bytes {
        return Err(JpegError::ResourceLimit(Limit::OutputBytes));
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(output_size)
        .map_err(|_| JpegError::AllocationFailed)?;
    for segment in &original.segments {
        if !selection.removes(segment.handling) {
            for block in bytes_at(input, segment.wire_range.clone())?.chunks(CONTROL_BYTES) {
                stop(&mut check_control)?;
                bytes.extend_from_slice(block);
            }
        }
    }
    // Reinspection parses the staged bytes afresh; it does not reuse the plan above.
    let inspection = inspect(&bytes, limits, &mut check_control)?;
    if inspection.coverage != Coverage::CompleteContainerProfile
        || inspection.frame != original.frame
        || inspection.segments.len() != original.segments.len() - removed_segments
    {
        return Err(JpegError::VerificationFailed);
    }
    let mut candidates = inspection.segments.iter();
    for before in &original.segments {
        stop(&mut check_control)?;
        if selection.removes(before.handling) {
            continue;
        }
        let after = candidates.next().ok_or(JpegError::VerificationFailed)?;
        if selection.removes(after.handling)
            || before.kind != after.kind
            || before.handling != after.handling
            || before.wire_range.len() != after.wire_range.len()
        {
            return Err(JpegError::VerificationFailed);
        }
        for (left, right) in bytes_at(input, before.wire_range.clone())?
            .chunks(CONTROL_BYTES)
            .zip(bytes_at(&bytes, after.wire_range.clone())?.chunks(CONTROL_BYTES))
        {
            stop(&mut check_control)?;
            if left != right {
                return Err(JpegError::VerificationFailed);
            }
        }
    }
    stop(&mut check_control)?;
    Ok(Cleaned {
        bytes,
        inspection,
        removed_segments,
    })
}
