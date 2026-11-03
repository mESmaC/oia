use std::error::Error;
use std::fmt;
use std::ops::Range;

use crate::{Coverage, ExtractionError, ExtractionLimits, ExtractionStatus, StopReason};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ByteOrder {
    LittleEndian,
    BigEndian,
}

impl ByteOrder {
    fn code_unit(self, bytes: [u8; 2]) -> u16 {
        match self {
            Self::LittleEndian => u16::from_le_bytes(bytes),
            Self::BigEndian => u16::from_be_bytes(bytes),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ByteOrderMark {
    Absent,
    Preserved,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Utf16Limits {
    extraction: ExtractionLimits,
    max_decoded_bytes: usize,
}

impl Utf16Limits {
    pub fn new(
        max_input_bytes: usize,
        max_decoded_bytes: usize,
        max_passages: usize,
    ) -> Result<Self, Utf16Error> {
        let extraction =
            ExtractionLimits::new(max_input_bytes, max_passages).map_err(Utf16Error::Extraction)?;
        if max_decoded_bytes == 0 {
            return Err(Utf16Error::ZeroDecodedByteLimit);
        }
        Ok(Self {
            extraction,
            max_decoded_bytes,
        })
    }

    pub fn max_input_bytes(self) -> usize {
        self.extraction.max_input_bytes()
    }

    pub fn max_decoded_bytes(self) -> usize {
        self.max_decoded_bytes
    }

    pub fn max_passages(self) -> usize {
        self.extraction.max_passages()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Utf16Error {
    Extraction(ExtractionError),
    ZeroDecodedByteLimit,
    ConflictingByteOrderMark { declared: ByteOrder },
    OddByteLength { input_bytes: usize },
    UnpairedSurrogate { byte_offset: usize, code_unit: u16 },
    DecodedTextTooLarge { limit_bytes: usize },
}

impl fmt::Display for Utf16Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Extraction(error) => error.fmt(formatter),
            Self::ZeroDecodedByteLimit => write!(formatter, "decoded byte limit must be positive"),
            Self::ConflictingByteOrderMark { declared } => write!(
                formatter,
                "initial UTF-16 byte-order mark contradicts declared {declared:?} encoding"
            ),
            Self::OddByteLength { input_bytes } => {
                write!(
                    formatter,
                    "UTF-16 input has an odd byte length: {input_bytes}"
                )
            }
            Self::UnpairedSurrogate {
                byte_offset,
                code_unit,
            } => write!(
                formatter,
                "unpaired UTF-16 surrogate {code_unit:#06x} at source byte {byte_offset}"
            ),
            Self::DecodedTextTooLarge { limit_bytes } => {
                write!(formatter, "decoded UTF-8 exceeds {limit_bytes} bytes")
            }
        }
    }
}

impl Error for Utf16Error {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Extraction(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Utf16Passage {
    source_byte_range: Range<usize>,
    line_number: usize,
    text: String,
}

impl Utf16Passage {
    pub fn source_byte_range(&self) -> Range<usize> {
        self.source_byte_range.clone()
    }

    pub fn line_number(&self) -> usize {
        self.line_number
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Utf16Extraction {
    passages: Vec<Utf16Passage>,
    coverage: Coverage,
    limits: Utf16Limits,
    byte_order: ByteOrder,
    byte_order_mark: ByteOrderMark,
}

impl Utf16Extraction {
    pub fn passages(&self) -> &[Utf16Passage] {
        &self.passages
    }

    pub fn coverage(&self) -> &Coverage {
        &self.coverage
    }

    pub fn limits(&self) -> Utf16Limits {
        self.limits
    }

    pub fn byte_order(&self) -> ByteOrder {
        self.byte_order
    }

    pub fn byte_order_mark(&self) -> ByteOrderMark {
        self.byte_order_mark
    }
}

pub fn extract_utf16(
    input: &[u8],
    byte_order: ByteOrder,
    limits: Utf16Limits,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<Utf16Extraction, Utf16Error> {
    let stopped = |reason| Utf16Error::Extraction(ExtractionError::Stopped(reason));
    check_control().map_err(stopped)?;
    if input.len() > limits.max_input_bytes() {
        return Err(Utf16Error::Extraction(ExtractionError::InputTooLarge {
            actual_bytes: input.len(),
            limit_bytes: limits.max_input_bytes(),
        }));
    }
    let (pairs, trailing) = input.as_chunks::<2>();
    if !trailing.is_empty() {
        return Err(Utf16Error::OddByteLength {
            input_bytes: input.len(),
        });
    }
    let byte_order_mark = match pairs.first().map(|pair| byte_order.code_unit(*pair)) {
        Some(0xfffe) => {
            return Err(Utf16Error::ConflictingByteOrderMark {
                declared: byte_order,
            });
        }
        Some(0xfeff) => ByteOrderMark::Preserved,
        _ => ByteOrderMark::Absent,
    };
    let mut units = pairs.iter().enumerate();
    let mut passages = Vec::new();
    let mut current_text = String::new();
    let mut extracted_bytes = 0;
    let mut decoded_bytes = 0_usize;
    while let Some((unit_index, pair)) = units.next() {
        check_control().map_err(stopped)?;
        let byte_offset = unit_index * 2;
        let code_unit = byte_order.code_unit(*pair);
        let mut source_end = byte_offset + 2;
        let scalar = match code_unit {
            0xd800..=0xdbff => {
                let Some((next_index, next_pair)) = units.next() else {
                    return Err(Utf16Error::UnpairedSurrogate {
                        byte_offset,
                        code_unit,
                    });
                };
                let low = byte_order.code_unit(*next_pair);
                if !(0xdc00..=0xdfff).contains(&low) {
                    return Err(Utf16Error::UnpairedSurrogate {
                        byte_offset,
                        code_unit,
                    });
                }
                source_end = next_index * 2 + 2;
                0x10000 + ((u32::from(code_unit) - 0xd800) << 10) + u32::from(low) - 0xdc00
            }
            0xdc00..=0xdfff => {
                return Err(Utf16Error::UnpairedSurrogate {
                    byte_offset,
                    code_unit,
                });
            }
            _ => u32::from(code_unit),
        };
        let character = char::from_u32(scalar).ok_or(Utf16Error::UnpairedSurrogate {
            byte_offset,
            code_unit,
        })?;
        decoded_bytes = decoded_bytes
            .checked_add(character.len_utf8())
            .filter(|total| *total <= limits.max_decoded_bytes())
            .ok_or(Utf16Error::DecodedTextTooLarge {
                limit_bytes: limits.max_decoded_bytes(),
            })?;
        if passages.len() < limits.max_passages() {
            current_text
                .try_reserve(character.len_utf8())
                .map_err(|_| Utf16Error::Extraction(ExtractionError::AllocationFailed))?;
            current_text.push(character);
            if character == '\n' {
                push_passage(
                    &mut passages,
                    &mut current_text,
                    extracted_bytes..source_end,
                )?;
                extracted_bytes = source_end;
            }
        }
    }
    if !current_text.is_empty() {
        push_passage(
            &mut passages,
            &mut current_text,
            extracted_bytes..input.len(),
        )?;
        extracted_bytes = input.len();
    }
    check_control().map_err(stopped)?;
    let status = if extracted_bytes < input.len() {
        ExtractionStatus::PassageLimitReached
    } else if input.is_empty() {
        ExtractionStatus::Empty
    } else {
        ExtractionStatus::Complete
    };
    Ok(Utf16Extraction {
        passages,
        coverage: Coverage {
            status,
            input_bytes: input.len(),
            extracted_bytes,
        },
        limits,
        byte_order,
        byte_order_mark,
    })
}

fn push_passage(
    passages: &mut Vec<Utf16Passage>,
    text: &mut String,
    source_byte_range: Range<usize>,
) -> Result<(), Utf16Error> {
    passages
        .try_reserve(1)
        .map_err(|_| Utf16Error::Extraction(ExtractionError::AllocationFailed))?;
    passages.push(Utf16Passage {
        source_byte_range,
        line_number: passages.len() + 1,
        text: std::mem::take(text),
    });
    Ok(())
}
