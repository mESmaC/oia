use oia_media::frame_timing::{
    FrameTimestamp, FrameTimingInput, MediaTimeBase, inspect_frame_timing,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let frames = [
        FrameTimestamp {
            frame_index: 0,
            presentation_ticks: Some(-3003),
            declared_duration_ticks: Some(3003),
        },
        FrameTimestamp {
            frame_index: 1,
            presentation_ticks: Some(0),
            declared_duration_ticks: Some(3003),
        },
        FrameTimestamp {
            frame_index: 2,
            presentation_ticks: None,
            declared_duration_ticks: None,
        },
        FrameTimestamp {
            frame_index: 4,
            presentation_ticks: Some(9009),
            declared_duration_ticks: Some(3003),
        },
        FrameTimestamp {
            frame_index: 5,
            presentation_ticks: Some(15015),
            declared_duration_ticks: None,
        },
    ];
    let report = inspect_frame_timing(
        FrameTimingInput {
            source_sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            time_base: MediaTimeBase {
                seconds_numerator: 1,
                ticks_denominator: 90_000,
            },
            declared_total_frames: Some(6),
            frames: &frames,
        },
        100,
        || false,
    )?;
    println!("SIMULATED metadata and illustrative source hash; no media file was inspected.");
    println!("{report:#?}");
    Ok(())
}
