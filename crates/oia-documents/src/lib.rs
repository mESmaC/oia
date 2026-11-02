use std::error::Error;
use std::fmt;
use std::ops::Range;

pub mod lineage;
pub mod search;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Limit {
    InputBytes,
    Passages,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExtractionLimits {
    max_input_bytes: usize,
    max_passages: usize,
}

impl ExtractionLimits {
    pub fn new(max_input_bytes: usize, max_passages: usize) -> Result<Self, ExtractionError> {
        if max_input_bytes == 0 {
            return Err(ExtractionError::ZeroLimit(Limit::InputBytes));
        }
        if max_passages == 0 {
            return Err(ExtractionError::ZeroLimit(Limit::Passages));
        }
        Ok(Self {
            max_input_bytes,
            max_passages,
        })
    }

    pub fn max_input_bytes(self) -> usize {
        self.max_input_bytes
    }

    pub fn max_passages(self) -> usize {
        self.max_passages
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopReason {
    Cancelled,
    DeadlineExceeded,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExtractionError {
    ZeroLimit(Limit),
    InputTooLarge {
        actual_bytes: usize,
        limit_bytes: usize,
    },
    MalformedUtf8 {
        valid_up_to: usize,
        error_len: Option<usize>,
    },
    AllocationFailed,
    Stopped(StopReason),
}

impl fmt::Display for ExtractionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroLimit(limit) => {
                write!(formatter, "{limit:?} limit must be greater than zero")
            }
            Self::InputTooLarge {
                actual_bytes,
                limit_bytes,
            } => write!(
                formatter,
                "input has {actual_bytes} bytes; configured limit is {limit_bytes} bytes"
            ),
            Self::MalformedUtf8 {
                valid_up_to,
                error_len,
            } => write!(
                formatter,
                "malformed UTF-8 at byte {valid_up_to}; invalid sequence length: {error_len:?}"
            ),
            Self::AllocationFailed => write!(formatter, "unable to allocate passage metadata"),
            Self::Stopped(reason) => write!(formatter, "extraction stopped: {reason:?}"),
        }
    }
}

impl Error for ExtractionError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExtractionStatus {
    Complete,
    Empty,
    PassageLimitReached,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Coverage {
    status: ExtractionStatus,
    input_bytes: usize,
    extracted_bytes: usize,
}

impl Coverage {
    pub fn status(&self) -> ExtractionStatus {
        self.status
    }

    pub fn input_bytes(&self) -> usize {
        self.input_bytes
    }

    pub fn extracted_bytes(&self) -> usize {
        self.extracted_bytes
    }

    pub fn unprocessed_range(&self) -> Option<Range<usize>> {
        (self.extracted_bytes < self.input_bytes).then_some(self.extracted_bytes..self.input_bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Passage<'input> {
    byte_range: Range<usize>,
    line_number: usize,
    text: &'input str,
}

impl<'input> Passage<'input> {
    pub fn byte_range(&self) -> Range<usize> {
        self.byte_range.clone()
    }

    pub fn line_number(&self) -> usize {
        self.line_number
    }

    pub fn text(&self) -> &'input str {
        self.text
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Extraction<'input> {
    passages: Vec<Passage<'input>>,
    coverage: Coverage,
    limits: ExtractionLimits,
}

impl<'input> Extraction<'input> {
    pub fn passages(&self) -> &[Passage<'input>] {
        &self.passages
    }

    pub fn coverage(&self) -> &Coverage {
        &self.coverage
    }

    pub fn limits(&self) -> ExtractionLimits {
        self.limits
    }
}

pub fn extract_utf8(
    input: &[u8],
    limits: ExtractionLimits,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<Extraction<'_>, ExtractionError> {
    check_control().map_err(ExtractionError::Stopped)?;
    if input.len() > limits.max_input_bytes {
        return Err(ExtractionError::InputTooLarge {
            actual_bytes: input.len(),
            limit_bytes: limits.max_input_bytes,
        });
    }
    let text = std::str::from_utf8(input).map_err(|error| ExtractionError::MalformedUtf8 {
        valid_up_to: error.valid_up_to(),
        error_len: error.error_len(),
    })?;
    let mut passages = Vec::new();
    let mut extracted_bytes = 0;
    for line in text.split_inclusive('\n') {
        if passages.len() == limits.max_passages {
            break;
        }
        check_control().map_err(ExtractionError::Stopped)?;
        let end = extracted_bytes + line.len();
        passages
            .try_reserve(1)
            .map_err(|_| ExtractionError::AllocationFailed)?;
        passages.push(Passage {
            byte_range: extracted_bytes..end,
            line_number: passages.len() + 1,
            text: line,
        });
        extracted_bytes = end;
    }
    check_control().map_err(ExtractionError::Stopped)?;
    let status = if extracted_bytes < input.len() {
        ExtractionStatus::PassageLimitReached
    } else if input.is_empty() {
        ExtractionStatus::Empty
    } else {
        ExtractionStatus::Complete
    };
    Ok(Extraction {
        passages,
        coverage: Coverage {
            status,
            input_bytes: input.len(),
            extracted_bytes,
        },
        limits,
    })
}
