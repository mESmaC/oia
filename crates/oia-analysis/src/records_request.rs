//! Local records-request drafting (`INTEL-04`, #36). No submission or I/O.
//!
//! This is a transient draft, not a persisted domain record. Application integration
//! must supply the common envelope, access checks and immutable revision storage.

use oia_domain::VersionId;
use std::fmt::{self, Write};

/// Maximum UTF-8 bytes in the request wording.
pub const MAX_REQUEST_BYTES: usize = 32_768;
/// Maximum UTF-8 bytes in each custodian field.
pub const MAX_CUSTODIAN_BYTES: usize = 2_048;
/// Maximum pinned source versions attached to a draft.
pub const MAX_REFERENCES: usize = 64;
/// Upper bound on UTF-8 preview length, including field escapes and line quoting.
pub const MAX_PREVIEW_BYTES: usize =
    4 * (MAX_REQUEST_BYTES + 2 * MAX_CUSTODIAN_BYTES) + MAX_REFERENCES * 132 + 1024;

/// Whether the proposed recipient has been established as the records custodian.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustodianAssessment {
    /// A research lead; responsibility has not been established.
    Candidate,
    /// The author has identified evidence for responsibility; not a system guarantee.
    AuthorConfirmed,
}

/// Caller-owned fields. Validation preserves their exact wording and order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftInput {
    pub version: VersionId,
    pub custodian: String,
    pub assessment: CustodianAssessment,
    /// Explanation of the assessment, including uncertainty or supporting evidence.
    pub custodian_rationale: String,
    pub request: String,
    /// Exact source versions used to prepare this wording (may be empty).
    pub source_versions: Vec<VersionId>,
}

/// A validated in-memory draft; construction and preview cannot submit it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordsRequestDraft(DraftInput);

/// Stable validation failures; error messages never echo request content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftError {
    EmptyField(&'static str),
    FieldTooLong(&'static str),
    ControlCharacter(&'static str),
    TooManyReferences,
    DuplicateReference,
    AllocationFailed,
    FormattingFailed,
}

impl fmt::Display for DraftError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyField(field) => write!(f, "{field} must contain text"),
            Self::FieldTooLong(field) => write!(f, "{field} exceeds its byte limit"),
            Self::ControlCharacter(field) => {
                write!(f, "{field} contains unsupported control characters")
            }
            Self::TooManyReferences => f.write_str("too many source versions"),
            Self::DuplicateReference => f.write_str("source versions must be unique"),
            Self::AllocationFailed => f.write_str("unable to allocate draft preview"),
            Self::FormattingFailed => f.write_str("unable to format draft preview"),
        }
    }
}

impl std::error::Error for DraftError {}

fn validate_text(
    field: &'static str,
    text: &str,
    limit: usize,
    multiline: bool,
) -> Result<(), DraftError> {
    if text.len() > limit {
        return Err(DraftError::FieldTooLong(field));
    }
    if text.trim().is_empty() {
        return Err(DraftError::EmptyField(field));
    }
    if text
        .chars()
        .any(|ch| ch.is_control() && !(multiline && matches!(ch, '\n' | '\t')))
    {
        return Err(DraftError::ControlCharacter(field));
    }
    Ok(())
}

fn write_preview_field(output: &mut String, text: &str) -> Result<(), DraftError> {
    output.push_str("> ");
    for character in text.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\n> "),
            '\t' => output.push_str("\\t"),
            '\u{061C}'
            | '\u{200B}'..='\u{200F}'
            | '\u{2028}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
            | '\u{FEFF}' => write!(output, "\\u{{{:04X}}}", character as u32)
                .map_err(|_| DraftError::FormattingFailed)?,
            _ => output.push(character),
        }
    }
    output.push('\n');
    Ok(())
}

impl RecordsRequestDraft {
    /// Validate bounded text and unique pinned references without side effects.
    /// LF and tab are accepted in multiline fields; other control characters,
    /// including CR, are rejected rather than silently normalizing original text.
    pub fn new(input: DraftInput) -> Result<Self, DraftError> {
        validate_text("custodian", &input.custodian, MAX_CUSTODIAN_BYTES, false)?;
        validate_text(
            "custodian rationale",
            &input.custodian_rationale,
            MAX_CUSTODIAN_BYTES,
            true,
        )?;
        validate_text("request", &input.request, MAX_REQUEST_BYTES, true)?;
        if input.source_versions.len() > MAX_REFERENCES {
            return Err(DraftError::TooManyReferences);
        }
        for (index, reference) in input.source_versions.iter().enumerate() {
            if input
                .source_versions
                .iter()
                .take(index)
                .any(|earlier| earlier == reference)
            {
                return Err(DraftError::DuplicateReference);
            }
        }
        Ok(Self(input))
    }

    /// Inspect exact validated wording and provenance. Revisions create new drafts.
    pub fn input(&self) -> &DraftInput {
        &self.0
    }

    /// Render an explicitly labeled UTF-8 plain-text preview, not an HTTP/email body.
    /// Quotes every field line and escapes backslashes, tabs and display controls.
    /// Exact wording remains available through input(); this is a display derivative.
    pub fn preview(&self) -> Result<String, DraftError> {
        let input = &self.0;
        let mut output = String::new();
        output
            .try_reserve(MAX_PREVIEW_BYTES)
            .map_err(|_| DraftError::AllocationFailed)?;
        let assessment = match input.assessment {
            CustodianAssessment::Candidate => "candidate; responsibility unconfirmed",
            CustodianAssessment::AuthorConfirmed => "confirmed by draft author",
        };
        write!(output, "DRAFT — NOT SUBMITTED\nVersion: {}\nAssessment: {}\nPreview encoding: field lines start with > ; backslashes, tabs and display controls are escaped.\n\nCustodian:\n",
            input.version, assessment
        ).map_err(|_| DraftError::FormattingFailed)?;
        write_preview_field(&mut output, &input.custodian)?;
        output.push_str("\nCustodian rationale:\n");
        write_preview_field(&mut output, &input.custodian_rationale)?;
        output.push_str("\nRequest wording:\n");
        write_preview_field(&mut output, &input.request)?;
        output.push_str("\nSource versions (research references):\n");
        if input.source_versions.is_empty() {
            output.push_str("None supplied; provenance not established.\n");
        }
        for reference in &input.source_versions {
            writeln!(output, "- {reference}").map_err(|_| DraftError::FormattingFailed)?;
        }
        Ok(output)
    }
}
