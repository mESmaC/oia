use oia_media::frame_timing::{
    DeclaredDurationRelation, FrameInterval, FrameTimestamp, FrameTimingError, FrameTimingInput,
    MediaTimeBase, TimingSelectionCoverage, inspect_frame_timing,
};

const SOURCE_HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn input(frames: &[FrameTimestamp], total: Option<u64>) -> FrameTimingInput<'_> {
    FrameTimingInput {
        source_sha256: SOURCE_HASH,
        time_base: MediaTimeBase {
            seconds_numerator: u32::MAX,
            ticks_denominator: 90_000,
        },
        declared_total_frames: total,
        frames,
    }
}

fn frame(index: u64, ticks: Option<i64>, duration: Option<u64>) -> FrameTimestamp {
    FrameTimestamp {
        frame_index: index,
        presentation_ticks: ticks,
        declared_duration_ticks: duration,
    }
}

#[test]
fn exact_arithmetic_and_timing_discrepancies_preserve_source_metadata()
-> Result<(), Box<dyn std::error::Error>> {
    let frames = [
        frame(0, Some(i64::MIN), Some(u64::MAX)),
        frame(1, Some(i64::MAX), Some(0)),
        frame(2, Some(i64::MIN), None),
        frame(3, Some(i64::MIN), Some(3)),
        frame(4, Some(i64::MIN + 4), Some(5)),
        frame(5, Some(i64::MIN + 6), None),
        frame(6, Some(i64::MIN + 7), None),
    ];
    let report = inspect_frame_timing(input(&frames, Some(7)), 7, || false)?;
    assert_eq!(report.input, input(&frames, Some(7)));
    assert_eq!(
        report.selection_coverage,
        TimingSelectionCoverage::WholeDeclaredTimeline
    );
    let intervals: Vec<_> = report.pairs.iter().map(|pair| pair.interval).collect();
    assert_eq!(
        intervals,
        [
            FrameInterval::Forward {
                delta_ticks: u64::MAX,
                seconds_numerator: u128::from(u64::MAX) * u128::from(u32::MAX),
                seconds_denominator: 90_000,
                declared_duration_relation: DeclaredDurationRelation::Matches,
            },
            FrameInterval::BackwardTimestamp {
                delta_ticks: -i128::from(u64::MAX)
            },
            FrameInterval::DuplicateTimestamp,
            FrameInterval::Forward {
                delta_ticks: 4,
                seconds_numerator: 4 * u128::from(u32::MAX),
                seconds_denominator: 90_000,
                declared_duration_relation: DeclaredDurationRelation::Gap { ticks: 1 },
            },
            FrameInterval::Forward {
                delta_ticks: 2,
                seconds_numerator: 2 * u128::from(u32::MAX),
                seconds_denominator: 90_000,
                declared_duration_relation: DeclaredDurationRelation::Overlap { ticks: 3 },
            },
            FrameInterval::Forward {
                delta_ticks: 1,
                seconds_numerator: u128::from(u32::MAX),
                seconds_denominator: 90_000,
                declared_duration_relation: DeclaredDurationRelation::Unavailable,
            },
        ]
    );
    Ok(())
}

#[test]
fn missing_and_uninspected_frames_never_become_continuous_timing()
-> Result<(), Box<dyn std::error::Error>> {
    let frames = [
        frame(0, Some(0), None),
        frame(1, None, None),
        frame(4, Some(12), None),
    ];
    let report = inspect_frame_timing(input(&frames, Some(8)), 3, || false)?;
    assert_eq!(
        report.selection_coverage,
        TimingSelectionCoverage::SelectedFrames
    );
    assert_eq!(report.missing_timestamps, 1);
    assert_eq!(
        report
            .pairs
            .iter()
            .map(|pair| pair.interval)
            .collect::<Vec<_>>(),
        [
            FrameInterval::MissingTimestamp,
            FrameInterval::UninspectedFrames { omitted_frames: 2 },
        ]
    );
    assert_eq!(
        inspect_frame_timing(input(&frames, None), 3, || false)?.selection_coverage,
        TimingSelectionCoverage::UnknownTotal
    );
    assert_eq!(
        inspect_frame_timing(input(&[], Some(0)), 3, || false)?.selection_coverage,
        TimingSelectionCoverage::EmptySelection
    );
    Ok(())
}

#[test]
fn invalid_bounds_and_cancellation_publish_no_report() {
    let frames = [frame(0, Some(0), None), frame(1, Some(1), None)];
    assert_eq!(
        inspect_frame_timing(input(&frames, Some(2)), 1, || false),
        Err(FrameTimingError::FrameBudgetExceeded {
            supplied: 2,
            maximum: 1
        })
    );
    assert_eq!(
        inspect_frame_timing(input(&frames, Some(2)), 0, || false),
        Err(FrameTimingError::ZeroFrameBudget)
    );
    let mut invalid = input(&frames, Some(2));
    invalid.time_base.ticks_denominator = 0;
    assert_eq!(
        inspect_frame_timing(invalid, 2, || false),
        Err(FrameTimingError::InvalidTimeBase)
    );
    invalid = input(&frames, Some(2));
    invalid.time_base.seconds_numerator = 0;
    assert_eq!(
        inspect_frame_timing(invalid, 2, || false),
        Err(FrameTimingError::InvalidTimeBase)
    );
    invalid = input(&frames, Some(2));
    invalid.source_sha256 = "invalid";
    assert_eq!(
        inspect_frame_timing(invalid, 2, || false),
        Err(FrameTimingError::InvalidSourceHash)
    );
    let unordered = [frames[1], frames[0]];
    assert_eq!(
        inspect_frame_timing(input(&unordered, Some(2)), 2, || false),
        Err(FrameTimingError::FrameIndexNotIncreasing {
            previous: 1,
            current: 0
        })
    );
    assert_eq!(
        inspect_frame_timing(input(&frames, Some(1)), 2, || false),
        Err(FrameTimingError::FrameOutsideDeclaredTotal {
            frame_index: 1,
            total: 1
        })
    );
    for cancel_at in 1..=5 {
        let mut checks = 0;
        assert_eq!(
            inspect_frame_timing(input(&frames, Some(2)), 2, || {
                checks += 1;
                checks == cancel_at
            }),
            Err(FrameTimingError::Cancelled)
        );
    }
}
