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
    /// Includes rationale and references so uncertainty survives export by the caller.
    pub fn preview(&self) -> Result<String, DraftError> {
        let input = &self.0;
        // All text and ID sizes are validated. This upper bound includes labels,
        // separators, and the maximum 128-byte identifier for every reference.
        let capacity = MAX_REQUEST_BYTES + 2 * MAX_CUSTODIAN_BYTES + MAX_REFERENCES * 132 + 1024;
        let mut output = String::new();
        output
            .try_reserve(capacity)
            .map_err(|_| DraftError::AllocationFailed)?;
        let assessment = match input.assessment {
            CustodianAssessment::Candidate => "candidate; responsibility unconfirmed",
            CustodianAssessment::AuthorConfirmed => "confirmed by draft author",
        };
        write!(output, "DRAFT — NOT SUBMITTED\nVersion: {}\nCustodian: {}\nAssessment: {}\n\nCustodian rationale:\n{}\n\nRequest wording:\n{}\n\nSource versions (research references):\n",
            input.version, input.custodian, assessment, input.custodian_rationale, input.request
        ).map_err(|_| DraftError::FormattingFailed)?;
        if input.source_versions.is_empty() {
            output.push_str("None supplied; provenance not established.\n");
        }
        for reference in &input.source_versions {
            writeln!(output, "- {reference}").map_err(|_| DraftError::FormattingFailed)?;
        }
        Ok(output)
    }
}
