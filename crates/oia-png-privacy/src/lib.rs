use std::error::Error;
use std::fmt;
use std::ops::Range;

pub const PROFILE: &str = "oia-png-container-rgba8-v2";
const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
const CONTROL_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Limit {
    InputBytes,
    OutputBytes,
    ChunkBytes,
    Chunks,
    MetadataBytes,
    Width,
    Height,
    Pixels,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub input_bytes: usize,
    pub output_bytes: usize,
    pub chunk_bytes: usize,
    pub chunks: usize,
    pub metadata_bytes: usize,
    pub width: u32,
    pub height: u32,
    pub pixels: u64,
}

impl Limits {
    fn validate(self) -> Result<(), PngError> {
        for (value, limit) in [
            (self.input_bytes as u64, Limit::InputBytes),
            (self.output_bytes as u64, Limit::OutputBytes),
            (self.chunk_bytes as u64, Limit::ChunkBytes),
            (self.chunks as u64, Limit::Chunks),
            (self.metadata_bytes as u64, Limit::MetadataBytes),
            (u64::from(self.width), Limit::Width),
            (u64::from(self.height), Limit::Height),
            (self.pixels, Limit::Pixels),
        ] {
            if value == 0 {
                return Err(PngError::ZeroLimit(limit));
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
pub enum PngError {
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

impl fmt::Display for PngError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroLimit(limit) => write!(formatter, "{limit:?} limit must be nonzero"),
            Self::ResourceLimit(limit) => write!(formatter, "PNG exceeds {limit:?} budget"),
            Self::UnsupportedFormat => write!(formatter, "input is not a PNG container"),
            Self::Malformed { offset, reason } => {
                write!(
                    formatter,
                    "invalid PNG structure at byte {offset}: {reason}"
                )
            }
            Self::UnsupportedProfile => write!(formatter, "PNG has regions outside {PROFILE}"),
            Self::RequiredForRendering(target) => write!(
                formatter,
                "selected {target:?} container carries rendering information; removal refused"
            ),
            Self::Stopped(reason) => write!(formatter, "PNG operation stopped: {reason:?}"),
            Self::AllocationFailed => write!(formatter, "PNG operation could not allocate memory"),
            Self::VerificationFailed => write!(formatter, "PNG container verification failed"),
        }
    }
}

impl Error for PngError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    Text,
    ModificationTime,
    Exif,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Handling {
    Retained,
    Removable(Target),
    /// Recognized container whose removal would change rendering; retained or refused.
    RequiredForRendering(Target),
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Chunk {
    pub kind: [u8; 4],
    pub wire_range: Range<usize>,
    pub payload_range: Range<usize>,
    pub handling: Handling,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Coverage {
    CompleteContainerProfile,
    Partial,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Inspection {
    pub chunks: Vec<Chunk>,
    pub coverage: Coverage,
    pub unsupported_image_parameters: bool,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Selection {
    pub text: bool,
    pub modification_time: bool,
    pub exif: bool,
}

impl Selection {
    fn selects(self, target: Target) -> bool {
        match target {
            Target::Text => self.text,
            Target::ModificationTime => self.modification_time,
            Target::Exif => self.exif,
        }
    }

    fn removes(self, handling: Handling) -> bool {
        matches!(handling, Handling::Removable(target) if self.selects(target))
    }
}

pub struct Cleaned {
    bytes: Vec<u8>,
    inspection: Inspection,
    removed_chunks: usize,
}

impl fmt::Debug for Cleaned {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Cleaned")
            .field("byte_length", &self.bytes.len())
            .field("inspection", &self.inspection)
            .field("removed_chunks", &self.removed_chunks)
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

    pub fn removed_chunks(&self) -> usize {
        self.removed_chunks
    }
}

fn malformed(offset: usize, reason: &'static str) -> PngError {
    PngError::Malformed { offset, reason }
}

fn bytes_at(input: &[u8], range: Range<usize>) -> Result<&[u8], PngError> {
    let offset = range.start;
    input
        .get(range)
        .ok_or_else(|| malformed(offset, "truncated chunk"))
}

fn word(input: &[u8], offset: usize) -> Result<u32, PngError> {
    let end = offset
        .checked_add(4)
        .ok_or_else(|| malformed(offset, "offset overflow"))?;
    let bytes = bytes_at(input, offset..end)?;
    let array = <[u8; 4]>::try_from(bytes).map_err(|_| malformed(offset, "invalid word"))?;
    Ok(u32::from_be_bytes(array))
}

fn crc32(
    input: &[u8],
    check_control: &mut impl FnMut() -> Result<(), StopReason>,
) -> Result<u32, PngError> {
    let mut crc = u32::MAX;
    for block in input.chunks(CONTROL_BYTES) {
        check_control().map_err(PngError::Stopped)?;
        for byte in block {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
            }
        }
    }
    Ok(!crc)
}

fn validate_text(
    payload: &[u8],
    offset: usize,
    check_control: &mut impl FnMut() -> Result<(), StopReason>,
) -> Result<(), PngError> {
    let separator = payload
        .iter()
        .take(80)
        .position(|byte| *byte == 0)
        .ok_or_else(|| malformed(offset, "missing or oversized text keyword"))?;
    let keyword = bytes_at(payload, 0..separator)?;
    if keyword.is_empty()
        || keyword.first() == Some(&b' ')
        || keyword.last() == Some(&b' ')
        || keyword.windows(2).any(|pair| pair == b"  ")
        || !keyword
            .iter()
            .all(|byte| matches!(byte, 32..=126 | 161..=255))
    {
        return Err(malformed(offset, "invalid text keyword"));
    }
    for block in bytes_at(payload, separator + 1..payload.len())?.chunks(CONTROL_BYTES) {
        check_control().map_err(PngError::Stopped)?;
        if block.contains(&0) {
            return Err(malformed(offset, "null byte in text value"));
        }
    }
    Ok(())
}

/// Orientation as recorded in IFD0 of an eXIf TIFF structure (Exif tag 0x0112).
enum Orientation {
    AbsentOrDefault,
    NonDefault,
    Uninterpretable,
}

/// Bounded read of the eXIf TIFF header and IFD0 that looks only for Orientation.
/// No other tag values are decoded; offsets are relative to the payload start
/// (PNG Third Edition 11.3.4.5: no APP1 marker or "Exif" identifier code).
fn exif_orientation(
    payload: &[u8],
    check_control: &mut impl FnMut() -> Result<(), StopReason>,
) -> Result<Orientation, PngError> {
    let big_endian = match payload.get(..4) {
        Some(b"MM\x00\x2a") => true,
        Some(b"II\x2a\x00") => false,
        _ => return Ok(Orientation::Uninterpretable),
    };
    let u16_at = |offset: usize| -> Option<u16> {
        let bytes = <[u8; 2]>::try_from(payload.get(offset..offset.checked_add(2)?)?).ok()?;
        Some(if big_endian {
            u16::from_be_bytes(bytes)
        } else {
            u16::from_le_bytes(bytes)
        })
    };
    let u32_at = |offset: usize| -> Option<u32> {
        let bytes = <[u8; 4]>::try_from(payload.get(offset..offset.checked_add(4)?)?).ok()?;
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
    // u16_at(ifd) succeeded, so ifd + 2 <= payload.len() and cannot overflow.
    let entries = ifd + 2;
    let entries_fit = usize::from(count)
        .checked_mul(12)
        .and_then(|length| entries.checked_add(length))
        .is_some_and(|end| end <= payload.len());
    if !entries_fit {
        return Ok(Orientation::Uninterpretable);
    }
    let mut orientation = None;
    for index in 0..usize::from(count) {
        if index % (CONTROL_BYTES / 12) == 0 {
            check_control().map_err(PngError::Stopped)?;
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

pub fn inspect(
    input: &[u8],
    limits: Limits,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<Inspection, PngError> {
    check_control().map_err(PngError::Stopped)?;
    limits.validate()?;
    if input.len() > limits.input_bytes {
        return Err(PngError::ResourceLimit(Limit::InputBytes));
    }
    if input.get(..SIGNATURE.len()) != Some(SIGNATURE) {
        return Err(PngError::UnsupportedFormat);
    }
    let mut result = Inspection {
        chunks: Vec::new(),
        coverage: Coverage::CompleteContainerProfile,
        unsupported_image_parameters: false,
        width: 0,
        height: 0,
    };
    let mut offset = SIGNATURE.len();
    let mut metadata_bytes = 0usize;
    let mut data_started = false;
    let mut data_ended = false;
    let mut data_bytes = 0usize;
    let mut singletons = Vec::new();
    loop {
        check_control().map_err(PngError::Stopped)?;
        if result.chunks.len() >= limits.chunks {
            return Err(PngError::ResourceLimit(Limit::Chunks));
        }
        let length = word(input, offset)?;
        if length > 0x7fff_ffff {
            return Err(malformed(offset, "chunk length exceeds PNG range"));
        }
        let length =
            usize::try_from(length).map_err(|_| PngError::ResourceLimit(Limit::ChunkBytes))?;
        if length > limits.chunk_bytes {
            return Err(PngError::ResourceLimit(Limit::ChunkBytes));
        }
        let kind_start = offset
            .checked_add(4)
            .ok_or_else(|| malformed(offset, "offset overflow"))?;
        let payload_start = offset
            .checked_add(8)
            .ok_or_else(|| malformed(offset, "offset overflow"))?;
        let payload_end = payload_start
            .checked_add(length)
            .ok_or_else(|| malformed(offset, "offset overflow"))?;
        let end = payload_end
            .checked_add(4)
            .ok_or_else(|| malformed(offset, "offset overflow"))?;
        let kind = <[u8; 4]>::try_from(bytes_at(input, kind_start..payload_start)?)
            .map_err(|_| malformed(offset, "invalid chunk type"))?;
        if !kind.iter().all(u8::is_ascii_alphabetic)
            || kind.get(2).is_none_or(u8::is_ascii_lowercase)
        {
            return Err(malformed(offset, "invalid chunk type or reserved bit"));
        }
        if !matches!(&kind, b"IDAT" | b"IEND" | b"IHDR") {
            metadata_bytes = metadata_bytes
                .checked_add(length)
                .ok_or(PngError::ResourceLimit(Limit::MetadataBytes))?;
            if metadata_bytes > limits.metadata_bytes {
                return Err(PngError::ResourceLimit(Limit::MetadataBytes));
            }
        }
        let payload = bytes_at(input, payload_start..payload_end)?;
        let expected_crc = word(input, payload_end)?;
        if crc32(
            bytes_at(input, kind_start..payload_end)?,
            &mut check_control,
        )? != expected_crc
        {
            return Err(malformed(offset, "chunk checksum mismatch"));
        }
        if result.chunks.is_empty() && kind != *b"IHDR" {
            return Err(malformed(offset, "IHDR must be first"));
        }
        if matches!(
            &kind,
            b"IHDR" | b"tIME" | b"gAMA" | b"sRGB" | b"pHYs" | b"eXIf"
        ) {
            if singletons.contains(&kind) {
                return Err(malformed(offset, "duplicate singleton chunk"));
            }
            singletons
                .try_reserve(1)
                .map_err(|_| PngError::AllocationFailed)?;
            singletons.push(kind);
        }
        if data_started && kind != *b"IDAT" {
            data_ended = true;
        }
        let handling = match &kind {
            b"IHDR" => {
                if length != 13 {
                    return Err(malformed(offset, "IHDR must contain 13 bytes"));
                }
                result.width = word(payload, 0)?;
                result.height = word(payload, 4)?;
                if result.width == 0
                    || result.height == 0
                    || result.width > 0x7fff_ffff
                    || result.height > 0x7fff_ffff
                {
                    return Err(malformed(offset, "invalid image dimensions"));
                }
                if result.width > limits.width {
                    return Err(PngError::ResourceLimit(Limit::Width));
                }
                if result.height > limits.height {
                    return Err(PngError::ResourceLimit(Limit::Height));
                }
                if u64::from(result.width) * u64::from(result.height) > limits.pixels {
                    return Err(PngError::ResourceLimit(Limit::Pixels));
                }
                let parameters = <[u8; 5]>::try_from(bytes_at(payload, 8..13)?)
                    .map_err(|_| malformed(offset, "invalid image parameters"))?;
                let [depth, color, compression, filter, interlace] = parameters;
                if !matches!(
                    (color, depth),
                    (0, 1 | 2 | 4 | 8 | 16) | (2 | 4 | 6, 8 | 16) | (3, 1 | 2 | 4 | 8)
                ) || compression != 0
                    || filter != 0
                    || interlace > 1
                {
                    return Err(malformed(offset, "invalid image parameters"));
                }
                result.unsupported_image_parameters = parameters != [8, 6, 0, 0, 0];
                Handling::Retained
            }
            b"IDAT" => {
                if data_ended {
                    return Err(malformed(offset, "IDAT chunks must be consecutive"));
                }
                data_started = true;
                data_bytes = data_bytes
                    .checked_add(length)
                    .ok_or_else(|| malformed(offset, "data length overflow"))?;
                Handling::Retained
            }
            b"IEND" => {
                if length != 0 || !data_started || data_bytes == 0 || end != input.len() {
                    return Err(malformed(
                        offset,
                        "invalid IEND, missing image data, or trailing bytes",
                    ));
                }
                Handling::Retained
            }
            b"tEXt" => {
                validate_text(payload, offset, &mut check_control)?;
                Handling::Removable(Target::Text)
            }
            b"tIME" => {
                if length != 7 {
                    return Err(malformed(offset, "invalid modification-time length"));
                }
                let [year_high, year_low, month, day, hour, minute, second] =
                    <[u8; 7]>::try_from(payload)
                        .map_err(|_| malformed(offset, "invalid modification time"))?;
                let year = u16::from_be_bytes([year_high, year_low]);
                let leap_year = year.is_multiple_of(4)
                    && (!year.is_multiple_of(100) || year.is_multiple_of(400));
                let days = match month {
                    4 | 6 | 9 | 11 => 30,
                    2 if leap_year => 29,
                    2 => 28,
                    1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
                    _ => 0,
                };
                if day == 0 || day > days || hour > 23 || minute > 59 || second > 60 {
                    return Err(malformed(offset, "invalid modification time"));
                }
                Handling::Removable(Target::ModificationTime)
            }
            b"eXIf" => {
                if data_started {
                    return Err(malformed(offset, "eXIf follows image data"));
                }
                match exif_orientation(payload, &mut check_control)? {
                    Orientation::AbsentOrDefault => Handling::Removable(Target::Exif),
                    Orientation::NonDefault => Handling::RequiredForRendering(Target::Exif),
                    Orientation::Uninterpretable => Handling::Unsupported,
                }
            }
            b"gAMA" | b"sRGB" | b"pHYs" => {
                if data_started {
                    return Err(malformed(offset, "rendering chunk follows image data"));
                }
                let valid = match &kind {
                    b"gAMA" => length == 4 && word(payload, 0)? != 0,
                    b"sRGB" => length == 1 && payload.first().is_some_and(|intent| *intent <= 3),
                    b"pHYs" => length == 9 && payload.get(8).is_some_and(|unit| *unit <= 1),
                    _ => false,
                };
                if !valid {
                    return Err(malformed(offset, "invalid rendering chunk"));
                }
                Handling::Retained
            }
            _ => Handling::Unsupported,
        };
        if handling == Handling::Unsupported || result.unsupported_image_parameters {
            result.coverage = Coverage::Partial;
        }
        result
            .chunks
            .try_reserve(1)
            .map_err(|_| PngError::AllocationFailed)?;
        result.chunks.push(Chunk {
            kind,
            wire_range: offset..end,
            payload_range: payload_start..payload_end,
            handling,
        });
        offset = end;
        if kind == *b"IEND" {
            break;
        }
    }
    check_control().map_err(PngError::Stopped)?;
    Ok(result)
}

pub fn clean(
    input: &[u8],
    selection: Selection,
    limits: Limits,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<Cleaned, PngError> {
    let original = inspect(input, limits, &mut check_control)?;
    if original.coverage != Coverage::CompleteContainerProfile {
        return Err(PngError::UnsupportedProfile);
    }
    for chunk in &original.chunks {
        if let Handling::RequiredForRendering(target) = chunk.handling
            && selection.selects(target)
        {
            return Err(PngError::RequiredForRendering(target));
        }
    }
    let mut output_size = SIGNATURE.len();
    let mut removed_chunks = 0;
    for chunk in &original.chunks {
        check_control().map_err(PngError::Stopped)?;
        if selection.removes(chunk.handling) {
            removed_chunks += 1;
        } else {
            output_size = output_size
                .checked_add(chunk.wire_range.len())
                .ok_or(PngError::ResourceLimit(Limit::OutputBytes))?;
        }
    }
    if output_size > limits.output_bytes {
        return Err(PngError::ResourceLimit(Limit::OutputBytes));
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(output_size)
        .map_err(|_| PngError::AllocationFailed)?;
    bytes.extend_from_slice(SIGNATURE);
    for chunk in &original.chunks {
        if !selection.removes(chunk.handling) {
            for block in bytes_at(input, chunk.wire_range.clone())?.chunks(CONTROL_BYTES) {
                check_control().map_err(PngError::Stopped)?;
                bytes.extend_from_slice(block);
            }
        }
    }
    let inspection = inspect(&bytes, limits, &mut check_control)?;
    if inspection.coverage != Coverage::CompleteContainerProfile
        || inspection.chunks.len() != original.chunks.len() - removed_chunks
    {
        return Err(PngError::VerificationFailed);
    }
    let mut candidates = inspection.chunks.iter();
    for before in &original.chunks {
        check_control().map_err(PngError::Stopped)?;
        if selection.removes(before.handling) {
            continue;
        }
        let after = candidates.next().ok_or(PngError::VerificationFailed)?;
        if selection.removes(after.handling) || before.wire_range.len() != after.wire_range.len() {
            return Err(PngError::VerificationFailed);
        }
        for (left, right) in bytes_at(input, before.wire_range.clone())?
            .chunks(CONTROL_BYTES)
            .zip(bytes_at(&bytes, after.wire_range.clone())?.chunks(CONTROL_BYTES))
        {
            check_control().map_err(PngError::Stopped)?;
            if left != right {
                return Err(PngError::VerificationFailed);
            }
        }
    }
    check_control().map_err(PngError::Stopped)?;
    Ok(Cleaned {
        bytes,
        inspection,
        removed_chunks,
    })
}
