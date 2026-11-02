//! Inspect supplied translations beside exact original passages (`INTEL-04`, #36).
//!
//! This is a transient, read-only comparison, not a translation service or persisted
//! evidence schema. Structural coverage and matching quotes do not establish accuracy.

use oia_domain::{ActorId, OpaqueId, VersionId};
use std::{fmt, ops::Range};

pub const MAX_SOURCE_BYTES: usize = 1_048_576;
pub const MAX_PASSAGE_BYTES: usize = 65_536;
pub const MAX_TRANSLATIONS: usize = 8;
pub const MAX_SEGMENTS: usize = 256;
pub const MAX_NOTE_BYTES: usize = 2_048;
/// Combined quotes, translated text, omission reasons and metadata, across alternatives.
pub const MAX_COMPARISON_BYTES: usize = 524_288;

/// The caller must resolve these references to authorized, immutable source bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextReference {
    pub source_id: OpaqueId,
    pub source_version: VersionId,
    pub extraction_version: VersionId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceText {
    pub reference: TextReference,
    /// Declared language label, not detected or validated as a language tag. None is unknown.
    pub language: Option<String>,
    /// Exact UTF-8 extraction; no trimming, newline conversion or Unicode normalization.
    pub text: String,
    /// Required disclosure of extraction coverage/limitations, including missing pages.
    pub extraction_notes: String,
}

/// Caller-supplied attribution. Neither identity nor a model's execution is verified here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranslationMethod {
    Human {
        translator: ActorId,
    },
    Model {
        provider: OpaqueId,
        model_version: VersionId,
        run_version: VersionId,
    },
}

/// An attributed review note, not a system endorsement or an authenticated review record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewNote {
    pub reviewer: ActorId,
    pub review_version: VersionId,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SegmentRendering {
    /// Supplied derivative text; never an additional observation or corroborating source.
    Translated {
        text: String,
    },
    Omitted {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranslationSegment {
    /// Half-open UTF-8 byte range in the full extraction, not relative to the selection.
    pub source_range: Range<usize>,
    /// Must match retained extraction bytes exactly, including whitespace and controls.
    pub original_quote: String,
    pub rendering: SegmentRendering,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranslationInput {
    pub version: VersionId,
    pub source: TextReference,
    /// Caller-declared target language; not a claim of language detection or quality.
    pub language: String,
    pub method: TranslationMethod,
    /// None means no review supplied. Notes cannot promote a derivative into an observation.
    pub review: Option<ReviewNote>,
    pub limitations: String,
    /// Ordered, gap-free partition of the selection, including explicit omitted segments.
    pub segments: Vec<TranslationSegment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComparisonInput {
    pub source: SourceText,
    pub selection: Range<usize>,
    /// Alternatives are retained in caller order, without choosing a winner.
    pub translations: Vec<TranslationInput>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    Cancelled,
    DeadlineExceeded,
    Superseded,
}

/// Errors contain field/index information, never source or translation content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComparisonError {
    EmptyField(&'static str),
    LimitExceeded(&'static str),
    InvalidSelection,
    SourceMismatch { translation: usize },
    VersionCollision { translation: usize },
    InvalidPartition { translation: usize, segment: usize },
    QuoteMismatch { translation: usize, segment: usize },
    Stopped(StopReason),
}

impl fmt::Display for ComparisonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyField(field) => write!(f, "{field} must contain text or entries"),
            Self::LimitExceeded(field) => write!(f, "{field} exceeds its byte or count limit"),
            Self::InvalidSelection => f.write_str("selection must be a nonempty UTF-8 byte range"),
            Self::SourceMismatch { translation } => {
                write!(
                    f,
                    "translation {translation} refers to different source/extraction versions"
                )
            }
            Self::VersionCollision { translation } => {
                write!(
                    f,
                    "translation {translation} reuses an input or translation version"
                )
            }
            Self::InvalidPartition {
                translation,
                segment,
            } => write!(
                f,
                "translation {translation}, segment {segment}: expected an ordered, complete UTF-8 partition"
            ),
            Self::QuoteMismatch {
                translation,
                segment,
            } => write!(
                f,
                "translation {translation}, segment {segment}: quote differs from pinned input bytes"
            ),
            Self::Stopped(reason) => write!(f, "comparison stopped: {reason:?}"),
        }
    }
}

impl std::error::Error for ComparisonError {}

/// Structural byte accounting only. Even zero omitted bytes does not prove completeness
/// of meaning, correct translation, or complete extraction of the original document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Coverage {
    pub selected_bytes: usize,
    pub translated_source_bytes: usize,
    pub explicitly_omitted_bytes: usize,
}

/// A validated comparison borrowing immutable caller input. No source bytes are copied.
#[derive(Debug)]
pub struct Comparison<'a> {
    input: &'a ComparisonInput,
}

impl<'a> Comparison<'a> {
    pub fn input(&self) -> &'a ComparisonInput {
        self.input
    }

    /// Inspect alternatives with structural coverage, preserving supplied order and notes.
    pub fn alternatives(&self) -> impl Iterator<Item = (&'a TranslationInput, Coverage)> + '_ {
        self.input.translations.iter().map(|translation| {
            let translated_source_bytes =
                translation
                    .segments
                    .iter()
                    .fold(0, |bytes, segment| match segment.rendering {
                        SegmentRendering::Translated { .. } => {
                            bytes + (segment.source_range.end - segment.source_range.start)
                        }
                        SegmentRendering::Omitted { .. } => bytes,
                    });
            let selected_bytes = self.input.selection.end - self.input.selection.start;
            (
                translation,
                Coverage {
                    selected_bytes,
                    translated_source_bytes,
                    explicitly_omitted_bytes: selected_bytes - translated_source_bytes,
                },
            )
        })
    }
}

fn text_budget(
    text: &str,
    field: &'static str,
    limit: usize,
    total: &mut usize,
) -> Result<(), ComparisonError> {
    if text.len() > limit {
        return Err(ComparisonError::LimitExceeded(field));
    }
    *total = total
        .checked_add(text.len())
        .filter(|sum| *sum <= MAX_COMPARISON_BYTES)
        .ok_or(ComparisonError::LimitExceeded("comparison text"))?;
    if text.trim().is_empty() {
        return Err(ComparisonError::EmptyField(field));
    }
    Ok(())
}

/// Validate quotes, version binding, resource limits and explicit translation coverage.
/// Control is checked on entry, before each alternative/segment and before returning.
/// There is no allocation, file/network access, translation execution or publication.
pub fn compare(
    input: &ComparisonInput,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<Comparison<'_>, ComparisonError> {
    check_control().map_err(ComparisonError::Stopped)?;
    if input.source.text.len() > MAX_SOURCE_BYTES {
        return Err(ComparisonError::LimitExceeded("source text"));
    }
    if input.selection.start >= input.selection.end
        || input.source.text.get(input.selection.clone()).is_none()
    {
        return Err(ComparisonError::InvalidSelection);
    }
    if input.selection.end - input.selection.start > MAX_PASSAGE_BYTES {
        return Err(ComparisonError::LimitExceeded("selected passage"));
    }
    if input.translations.is_empty() {
        return Err(ComparisonError::EmptyField("translations"));
    }
    if input.translations.len() > MAX_TRANSLATIONS {
        return Err(ComparisonError::LimitExceeded("translations"));
    }
    let mut text_bytes = 0;
    if let Some(language) = &input.source.language {
        text_budget(language, "source language", MAX_NOTE_BYTES, &mut text_bytes)?;
    }
    text_budget(
        &input.source.extraction_notes,
        "extraction notes",
        MAX_NOTE_BYTES,
        &mut text_bytes,
    )?;
    for (index, translation) in input.translations.iter().enumerate() {
        check_control().map_err(ComparisonError::Stopped)?;
        if translation.source != input.source.reference {
            return Err(ComparisonError::SourceMismatch { translation: index });
        }
        if translation.version == input.source.reference.source_version
            || translation.version == input.source.reference.extraction_version
            || input
                .translations
                .iter()
                .take(index)
                .any(|other| other.version == translation.version)
        {
            return Err(ComparisonError::VersionCollision { translation: index });
        }
        text_budget(
            &translation.language,
            "target language",
            MAX_NOTE_BYTES,
            &mut text_bytes,
        )?;
        text_budget(
            &translation.limitations,
            "translation limitations",
            MAX_NOTE_BYTES,
            &mut text_bytes,
        )?;
        if let Some(review) = &translation.review {
            text_budget(&review.note, "review note", MAX_NOTE_BYTES, &mut text_bytes)?;
        }
        if translation.segments.is_empty() {
            return Err(ComparisonError::EmptyField("segments"));
        }
        if translation.segments.len() > MAX_SEGMENTS {
            return Err(ComparisonError::LimitExceeded("segments"));
        }
        let mut next_byte = input.selection.start;
        for (segment_index, segment) in translation.segments.iter().enumerate() {
            check_control().map_err(ComparisonError::Stopped)?;
            let range = &segment.source_range;
            let partition_error = ComparisonError::InvalidPartition {
                translation: index,
                segment: segment_index,
            };
            if range.start != next_byte
                || range.start >= range.end
                || range.end > input.selection.end
            {
                return Err(partition_error);
            }
            let original = input
                .source
                .text
                .get(range.clone())
                .ok_or(partition_error)?;
            // Quotes may be whitespace-only. They are still retained exactly and counted.
            text_bytes = text_bytes
                .checked_add(segment.original_quote.len())
                .filter(|sum| *sum <= MAX_COMPARISON_BYTES)
                .ok_or(ComparisonError::LimitExceeded("comparison text"))?;
            if segment.original_quote.as_bytes() != original.as_bytes() {
                return Err(ComparisonError::QuoteMismatch {
                    translation: index,
                    segment: segment_index,
                });
            }
            match &segment.rendering {
                SegmentRendering::Translated { text } => {
                    text_budget(text, "translated text", MAX_PASSAGE_BYTES, &mut text_bytes)?;
                }
                SegmentRendering::Omitted { reason } => {
                    text_budget(reason, "omission reason", MAX_NOTE_BYTES, &mut text_bytes)?;
                }
            }
            next_byte = range.end;
        }
        if next_byte != input.selection.end {
            return Err(ComparisonError::InvalidPartition {
                translation: index,
                segment: translation.segments.len(),
            });
        }
    }
    check_control().map_err(ComparisonError::Stopped)?;
    Ok(Comparison { input })
}
