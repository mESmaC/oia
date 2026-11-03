use std::fmt;

pub const FRAME_TIMING_CAVEAT: &str = "Presentation timestamps and declared durations are caller-supplied media metadata, not capture times, proof of dropped frames, or an authenticity verdict. No missing timing is interpolated.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaTimeBase {
    pub seconds_numerator: u32,
    pub ticks_denominator: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameTimestamp {
    pub frame_index: u64,
    pub presentation_ticks: Option<i64>,
    pub declared_duration_ticks: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameTimingInput<'a> {
    pub source_sha256: &'a str,
    pub time_base: MediaTimeBase,
    pub declared_total_frames: Option<u64>,
    pub frames: &'a [FrameTimestamp],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimingSelectionCoverage {
    EmptySelection,
    WholeDeclaredTimeline,
    SelectedFrames,
    UnknownTotal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclaredDurationRelation {
    Unavailable,
    Matches,
    Gap { ticks: u64 },
    Overlap { ticks: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameInterval {
    UninspectedFrames {
        omitted_frames: u64,
    },
    MissingTimestamp,
    DuplicateTimestamp,
    BackwardTimestamp {
        delta_ticks: i128,
    },
    Forward {
        delta_ticks: u64,
        seconds_numerator: u128,
        seconds_denominator: u32,
        declared_duration_relation: DeclaredDurationRelation,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FramePairTiming {
    pub previous_frame_index: u64,
    pub current_frame_index: u64,
    pub interval: FrameInterval,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameTimingReport<'a> {
    pub input: FrameTimingInput<'a>,
    pub selection_coverage: TimingSelectionCoverage,
    pub missing_timestamps: usize,
    pub pairs: Vec<FramePairTiming>,
    pub caveat: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameTimingError {
    InvalidSourceHash,
    InvalidTimeBase,
    ZeroFrameBudget,
    FrameBudgetExceeded { supplied: usize, maximum: usize },
    FrameIndexNotIncreasing { previous: u64, current: u64 },
    FrameOutsideDeclaredTotal { frame_index: u64, total: u64 },
    AllocationFailed,
    Cancelled,
}

impl fmt::Display for FrameTimingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSourceHash => {
                formatter.write_str("source_sha256 must contain 64 lowercase hexadecimal digits")
            }
            Self::InvalidTimeBase => {
                formatter.write_str("both media time-base terms must be nonzero")
            }
            Self::ZeroFrameBudget => formatter.write_str("max_frames must be nonzero"),
            Self::FrameBudgetExceeded { supplied, maximum } => {
                write!(formatter, "{supplied} frames exceed max_frames={maximum}")
            }
            Self::FrameIndexNotIncreasing { previous, current } => write!(
                formatter,
                "presentation-order frame indices must increase: {previous} then {current}"
            ),
            Self::FrameOutsideDeclaredTotal { frame_index, total } => write!(
                formatter,
                "frame index {frame_index} is outside declared total {total}"
            ),
            Self::AllocationFailed => formatter.write_str("unable to allocate frame-timing report"),
            Self::Cancelled => formatter.write_str("frame-timing inspection cancelled"),
        }
    }
}

impl std::error::Error for FrameTimingError {}

pub fn inspect_frame_timing<'a>(
    input: FrameTimingInput<'a>,
    max_frames: usize,
    mut cancelled: impl FnMut() -> bool,
) -> Result<FrameTimingReport<'a>, FrameTimingError> {
    if cancelled() {
        return Err(FrameTimingError::Cancelled);
    }
    if !crate::is_valid_lowercase_sha256_hex(input.source_sha256) {
        return Err(FrameTimingError::InvalidSourceHash);
    }
    if input.time_base.seconds_numerator == 0 || input.time_base.ticks_denominator == 0 {
        return Err(FrameTimingError::InvalidTimeBase);
    }
    if max_frames == 0 {
        return Err(FrameTimingError::ZeroFrameBudget);
    }
    if input.frames.len() > max_frames {
        return Err(FrameTimingError::FrameBudgetExceeded {
            supplied: input.frames.len(),
            maximum: max_frames,
        });
    }

    let mut previous_frame: Option<FrameTimestamp> = None;
    let mut missing_timestamps = 0;
    for frame in input.frames {
        if cancelled() {
            return Err(FrameTimingError::Cancelled);
        }
        if let Some(total) = input.declared_total_frames
            && frame.frame_index >= total
        {
            return Err(FrameTimingError::FrameOutsideDeclaredTotal {
                frame_index: frame.frame_index,
                total,
            });
        }
        if let Some(previous) = previous_frame
            && frame.frame_index <= previous.frame_index
        {
            return Err(FrameTimingError::FrameIndexNotIncreasing {
                previous: previous.frame_index,
                current: frame.frame_index,
            });
        }
        missing_timestamps += usize::from(frame.presentation_ticks.is_none());
        previous_frame = Some(*frame);
    }

    let selection_coverage = if input.frames.is_empty() {
        TimingSelectionCoverage::EmptySelection
    } else {
        match input.declared_total_frames {
            Some(total) if u64::try_from(input.frames.len()).ok() == Some(total) => {
                TimingSelectionCoverage::WholeDeclaredTimeline
            }
            Some(_) => TimingSelectionCoverage::SelectedFrames,
            None => TimingSelectionCoverage::UnknownTotal,
        }
    };
    let mut pairs = Vec::new();
    pairs
        .try_reserve_exact(input.frames.len().saturating_sub(1))
        .map_err(|_| FrameTimingError::AllocationFailed)?;
    for window in input.frames.windows(2) {
        if cancelled() {
            return Err(FrameTimingError::Cancelled);
        }
        if let [previous, current] = window {
            pairs.push(FramePairTiming {
                previous_frame_index: previous.frame_index,
                current_frame_index: current.frame_index,
                interval: inspect_interval(*previous, *current, input.time_base),
            });
        }
    }
    if cancelled() {
        return Err(FrameTimingError::Cancelled);
    }
    Ok(FrameTimingReport {
        input,
        selection_coverage,
        missing_timestamps,
        pairs,
        caveat: FRAME_TIMING_CAVEAT,
    })
}

fn inspect_interval(
    previous: FrameTimestamp,
    current: FrameTimestamp,
    time_base: MediaTimeBase,
) -> FrameInterval {
    let index_step = current.frame_index - previous.frame_index;
    if index_step > 1 {
        return FrameInterval::UninspectedFrames {
            omitted_frames: index_step - 1,
        };
    }
    let (Some(previous_ticks), Some(current_ticks)) =
        (previous.presentation_ticks, current.presentation_ticks)
    else {
        return FrameInterval::MissingTimestamp;
    };
    let delta_ticks = i128::from(current_ticks) - i128::from(previous_ticks);
    if delta_ticks == 0 {
        return FrameInterval::DuplicateTimestamp;
    }
    let Ok(delta_ticks) = u64::try_from(delta_ticks) else {
        return FrameInterval::BackwardTimestamp { delta_ticks };
    };
    let declared_duration_relation = match previous.declared_duration_ticks {
        None => DeclaredDurationRelation::Unavailable,
        Some(duration) if delta_ticks == duration => DeclaredDurationRelation::Matches,
        Some(duration) if delta_ticks > duration => DeclaredDurationRelation::Gap {
            ticks: delta_ticks - duration,
        },
        Some(duration) => DeclaredDurationRelation::Overlap {
            ticks: duration - delta_ticks,
        },
    };
    FrameInterval::Forward {
        delta_ticks,
        seconds_numerator: u128::from(delta_ticks) * u128::from(time_base.seconds_numerator),
        seconds_denominator: time_base.ticks_denominator,
        declared_duration_relation,
    }
}
