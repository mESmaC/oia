#![deny(clippy::indexing_slicing)]

mod container;
mod decode;
mod metadata;
mod pixels;

use image::ImageFormat;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fmt;
use std::io::{self, Write};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const PROFILE: &str = "oia-still-image-cues-v1";
pub const DECODER: &str = "image 0.25.10 / png 0.18.1 (PNG); flate2 1.1.10 / zlib-rs 0.6.8 (PNG zlib validation); zune-jpeg 0.5.15 (JPEG, strict option enabled but recovery not excluded)";
pub(crate) const JPEG_VALIDATION: &str =
    "JPEG entropy completeness unverified; decoder recovery possible";

#[derive(Clone, Debug, Serialize)]
pub struct Config {
    pub max_input_bytes: usize,
    pub max_pixels: u64,
    pub max_dimension: u32,
    pub decoder_allocation_bytes: u64,
    pub max_container_items: usize,
    pub max_jpeg_scans: usize,
    pub max_metadata_bytes: usize,
    pub max_artifact_bytes: usize,
    pub max_blocks: u64,
    pub max_candidates: usize,
    pub deadline_ms: u64,
    pub block_size: u32,
    pub minimum_channel_range: u8,
    pub jpeg_quality: u8,
    pub residual_gain: u8,
    pub emit_residual_map: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_input_bytes: 32 * 1024 * 1024,
            max_pixels: 4_000_000,
            max_dimension: 8192,
            decoder_allocation_bytes: 128 * 1024 * 1024,
            max_container_items: 8192,
            max_jpeg_scans: 64,
            max_metadata_bytes: 1024 * 1024,
            max_artifact_bytes: 32 * 1024 * 1024,
            max_blocks: 65_536,
            max_candidates: 100,
            deadline_ms: 30_000,
            block_size: 16,
            minimum_channel_range: 16,
            jpeg_quality: 90,
            residual_gain: 8,
            emit_residual_map: false,
        }
    }
}

impl Config {
    pub fn validate(&self) -> Result<(), ForensicError> {
        if self.max_input_bytes == 0
            || self.max_pixels == 0
            || self.max_dimension == 0
            || self.decoder_allocation_bytes == 0
            || self.max_container_items == 0
            || self.max_jpeg_scans == 0
            || self.max_metadata_bytes == 0
            || self.max_artifact_bytes == 0
            || self.max_blocks == 0
            || self.max_candidates == 0
            || self.deadline_ms == 0
            || !(4..=64).contains(&self.block_size)
            || self.minimum_channel_range == 0
            || !(1..=100).contains(&self.jpeg_quality)
            || !(1..=32).contains(&self.residual_gain)
        {
            return Err(ForensicError::InvalidConfig);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Coverage {
    Complete,
    Partial,
    NotApplicable,
    NotImplemented,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    Cancelled,
    DeadlineExceeded,
    Superseded,
}

#[derive(Debug)]
pub enum ForensicError {
    InvalidConfig,
    Limit(&'static str),
    Unsupported(&'static str),
    Malformed { offset: usize, reason: &'static str },
    Codec(image::ImageError),
    Jpeg(String),
    PngContainer(oia_png_privacy::PngError),
    JpegContainer(oia_jpeg_privacy::JpegError),
    Stopped(StopReason),
}

impl fmt::Display for ForensicError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig => formatter.write_str("invalid or zero analysis setting"),
            Self::Limit(limit) => write!(formatter, "resource limit reached: {limit}"),
            Self::Unsupported(reason) => write!(formatter, "unsupported input: {reason}"),
            Self::Malformed { offset, reason } => {
                write!(formatter, "malformed input at byte {offset}: {reason}")
            }
            Self::Codec(error) => write!(formatter, "image codec rejected input: {error}"),
            Self::Jpeg(error) => write!(formatter, "JPEG decoder rejected input: {error}"),
            Self::PngContainer(error) => write!(formatter, "PNG inspection failed: {error}"),
            Self::JpegContainer(error) => write!(formatter, "JPEG inspection failed: {error}"),
            Self::Stopped(reason) => write!(formatter, "analysis stopped: {reason:?}"),
        }
    }
}

impl std::error::Error for ForensicError {}

impl From<image::ImageError> for ForensicError {
    fn from(error: image::ImageError) -> Self {
        Self::Codec(error)
    }
}

#[derive(Debug, Serialize)]
pub struct SourceIdentity {
    pub sha256: String,
    pub byte_length: usize,
    pub format: &'static str,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Serialize)]
pub struct Cue {
    pub kind: &'static str,
    pub container: &'static str,
    pub byte_offset: usize,
    pub byte_length: usize,
    pub payload_sha256: String,
    pub interpretation: &'static str,
}

#[derive(Debug, Serialize)]
pub struct UninspectedContainer {
    pub container: String,
    pub byte_offset: usize,
    pub byte_length: usize,
    pub reason: &'static str,
}

#[derive(Debug, Default, Serialize)]
pub struct MetadataAnalysis {
    pub inspected_text_fields: u64,
    pub cues: Vec<Cue>,
    pub uninspected: Vec<UninspectedContainer>,
}

#[derive(Debug, Serialize)]
pub struct Method<T> {
    pub coverage: Coverage,
    pub scope: &'static str,
    pub result: Option<T>,
    pub limitations: Vec<&'static str>,
}

impl<T> Method<T> {
    fn with_raster_validation(mut self, validation: &Method<&'static str>) -> Self {
        if validation.coverage != Coverage::Complete
            && matches!(self.coverage, Coverage::Complete | Coverage::Partial)
        {
            self.coverage = Coverage::Partial;
            self.limitations.extend(validation.result);
        }
        self
    }
}

#[derive(Debug, Serialize)]
pub struct BlockPosition {
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Serialize)]
pub struct BlockMatch {
    pub first: BlockPosition,
    pub repeated: BlockPosition,
}

#[derive(Debug, Serialize)]
pub struct RepeatedBlocks {
    pub total_grid_blocks: u64,
    pub examined_grid_blocks: u64,
    pub eligible_blocks: u64,
    pub excluded_low_range_blocks: u64,
    pub excluded_nonopaque_blocks: u64,
    pub uncovered_right_pixels: u32,
    pub uncovered_bottom_pixels: u32,
    pub candidates: Vec<BlockMatch>,
    pub stopped_at: Option<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct DerivedArtifact {
    pub role: &'static str,
    pub media_type: &'static str,
    pub sha256: String,
    pub byte_length: usize,
}

#[derive(Debug, Serialize)]
pub struct ResidualSummary {
    pub mean_absolute_channel_difference: f64,
    pub maximum_channel_difference: u8,
    pub channel_difference_histogram: Vec<u64>,
    pub artifact: Option<DerivedArtifact>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub analysis_started_unix_ms: Option<u128>,
    pub analysis_duration_ms: u128,
    pub profile: &'static str,
    pub implementation_version: &'static str,
    pub build_revision: Option<&'static str>,
    pub decoder: &'static str,
    pub source: SourceIdentity,
    pub config: Config,
    pub assessment: &'static str,
    pub review_status: &'static str,
    pub sample_domain: &'static str,
    pub raster_validation: Method<&'static str>,
    pub rgb_histograms: Method<[Vec<u64>; 3]>,
    pub metadata: Method<MetadataAnalysis>,
    pub repeated_blocks: Method<RepeatedBlocks>,
    pub jpeg_recompression: Method<ResidualSummary>,
    pub learned_ai_detection: Method<()>,
    pub credential_and_watermark_verification: Method<()>,
    pub limitations: Vec<&'static str>,
}

#[derive(Debug)]
pub struct Analysis {
    pub report: Report,
    pub residual_png: Option<Vec<u8>>,
}

impl Report {
    pub fn profile_is_partial(&self) -> bool {
        self.raster_validation.coverage == Coverage::Partial
            || self.rgb_histograms.coverage == Coverage::Partial
            || self.metadata.coverage == Coverage::Partial
            || self.repeated_blocks.coverage == Coverage::Partial
            || self.jpeg_recompression.coverage == Coverage::Partial
    }
}

pub fn analyze(
    input: &[u8],
    config: &Config,
    mut control: impl FnMut() -> Result<(), StopReason>,
) -> Result<Analysis, ForensicError> {
    config.validate()?;
    if input.len() > config.max_input_bytes {
        return Err(ForensicError::Limit("input bytes"));
    }
    let started = Instant::now();
    let analysis_started_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|elapsed| elapsed.as_millis());
    let mut checkpoint = || {
        control().map_err(ForensicError::Stopped)?;
        if started.elapsed() >= Duration::from_millis(config.deadline_ms) {
            return Err(ForensicError::Stopped(StopReason::DeadlineExceeded));
        }
        Ok(())
    };
    checkpoint()?;
    let mut source_hasher = Sha256::new();
    for chunk in input.chunks(4096) {
        checkpoint()?;
        source_hasher.update(chunk);
    }
    let source_sha256 = format!("{:x}", source_hasher.finalize());
    let parsed = container::inspect(input, config, &mut checkpoint)?;
    let raster = decode::raster(&parsed.decode_bytes, parsed.format, config)?;
    let (width, height) = raster.dimensions();
    checkpoint()?;
    let raster_validation = Method {
        coverage: if parsed.format == ImageFormat::Jpeg {
            Coverage::Partial
        } else {
            Coverage::Complete
        },
        scope: "container, PNG zlib integrity and decoder acceptance checks, not source authenticity",
        result: Some(if parsed.format == ImageFormat::Jpeg {
            JPEG_VALIDATION
        } else {
            "PNG container, bounded complete zlib stream/Adler-32 and limited raster decoder checks passed"
        }),
        limitations: if parsed.format == ImageFormat::Jpeg {
            vec![
                "These profile checks are not a proof of full format conformance or authenticity.",
                "JPEG decoder can zero-fill premature entropy even with its strict option. All JPEG results are partial decoder-output diagnostics, never verified source samples.",
            ]
        } else {
            vec!["These profile checks are not a proof of full format conformance or authenticity."]
        },
    };
    let rgb_histograms = Method {
        coverage: Coverage::Complete,
        scope: "256-bin RGB channel counts over decoder output, including color beneath transparency",
        result: Some(pixels::histograms(&raster, &mut checkpoint)?),
        limitations: vec!["Not color-managed display values or an authenticity test."],
    }
    .with_raster_validation(&raster_validation);
    let repeated_blocks = pixels::repeated_blocks(&raster, config, &mut checkpoint)?
        .with_raster_validation(&raster_validation);
    let (jpeg_recompression, residual_png) = pixels::recompression(
        &raster,
        parsed.format,
        &source_sha256,
        config,
        &mut checkpoint,
    )?;
    let jpeg_recompression = jpeg_recompression.with_raster_validation(&raster_validation);
    let metadata = Method {
        coverage: if parsed.metadata.uninspected.is_empty() {
            Coverage::Complete
        } else {
            Coverage::Partial
        },
        scope: "PNG tEXt/uncompressed iTXt and JPEG COM/standard XMP declarations only",
        result: Some(parsed.metadata),
        limitations: vec![
            "Declarations are untrusted source assertions: editable, forgeable and removable.",
            "No cue does not establish camera origin, authenticity, or absence of editing/AI.",
            "Only listed signatures are recognized; metadata text and prompts are not exported.",
        ],
    };
    let analysis = Analysis {
        report: Report {
            schema_version: 2,
            analysis_started_unix_ms,
            analysis_duration_ms: started.elapsed().as_millis(),
            profile: PROFILE,
            implementation_version: env!("CARGO_PKG_VERSION"),
            build_revision: option_env!("OIA_BUILD_REVISION"),
            decoder: DECODER,
            source: SourceIdentity {
                sha256: source_sha256,
                byte_length: input.len(),
                format: if parsed.format == ImageFormat::Png {
                    "png"
                } else {
                    "jpeg"
                },
                width,
                height,
            },
            config: config.clone(),
            assessment: "inconclusive",
            review_status: "unreviewed",
            sample_domain: "decoder output, not observations; encoded raster coordinates; 8-bit RGBA including RGB under transparency; EXIF orientation and color profiles not applied",
            raster_validation,
            rgb_histograms,
            metadata,
            repeated_blocks,
            jpeg_recompression,
            learned_ai_detection: Method {
                coverage: Coverage::NotImplemented,
                scope: "learned AI-generation/edit classification",
                result: None,
                limitations: vec![
                    "No model or calibrated probability; D09 evaluation remains open.",
                ],
            },
            credential_and_watermark_verification: Method {
                coverage: Coverage::NotImplemented,
                scope: "C2PA signatures, trust lists and generator watermarks",
                result: None,
                limitations: vec!["Container presence or absence is not credential verification."],
            },
            limitations: vec![
                "Diagnostic cues are not authenticity, AI-origin, or tampering verdicts.",
                "Byte hashes establish integrity, not source truth or authenticity.",
                "Codec calls are bounded by input/dimensions but only cooperatively cancellable between calls; decoder allocation limits are best effort.",
                "No detector accuracy, false-positive rate, formal proof, or implementation correspondence has been established.",
            ],
        },
        residual_png,
    };
    checkpoint()?;
    Ok(analysis)
}

pub(crate) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(crate) struct CappedWriter {
    pub bytes: Vec<u8>,
    pub limit: usize,
}

impl Write for CappedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(io::Error::other("encoded artifact byte limit reached"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
