//! Transient witness capture (FIELD-02, #27). No persistence, inference or I/O.
//!
//! Reported fields preserve wording, not validated physical measurements. A host
//! must retain the initial capture durably before presenting explanations, and
//! supply case authorization, timestamps and the common evidence envelope.

use crate::{ActorId, OpaqueId, VersionId};
use std::fmt;

/// Maximum UTF-8 bytes per form field or revision rationale.
pub const MAX_FIELD_BYTES: usize = 8_192;
/// Maximum retained versions, including the initial account.
pub const MAX_VERSIONS: usize = 64;

/// Origin is fixed for the entire capture; simulated input cannot become testimony.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountOrigin {
    Reported,
    Simulated,
}

/// Exact caller-supplied wording. None means unreported, never zero or absence.
/// Time/location remain descriptions until shared time/geometry contracts land.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WitnessForm {
    pub narrative: String,
    pub reported_time: Option<String>,
    pub location: Option<String>,
    pub duration: Option<String>,
    pub direction: Option<String>,
    pub apparent_size: Option<String>,
    pub motion: Option<String>,
    pub sound: Option<String>,
    pub conditions: Option<String>,
}

/// An immutable captured version. Creation is controlled by WitnessCapture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountVersion {
    version: VersionId,
    actor: ActorId,
    form: WitnessForm,
    rationale: Option<String>,
    after_explanations: bool,
}

impl AccountVersion {
    pub fn version(&self) -> &VersionId {
        &self.version
    }

    pub fn actor(&self) -> &ActorId {
        &self.actor
    }

    pub fn form(&self) -> &WitnessForm {
        &self.form
    }

    /// None only for the initial account.
    pub fn rationale(&self) -> Option<&str> {
        self.rationale.as_deref()
    }

    /// Whether this version followed in-workflow exposure. False says nothing
    /// about exposure outside this capture; it does not establish independence.
    pub fn after_explanations(&self) -> bool {
        self.after_explanations
    }
}

/// In-memory, append-only capture. A successful transition is not a durable save.
#[derive(Debug, PartialEq, Eq)]
pub struct WitnessCapture {
    id: OpaqueId,
    origin: AccountOrigin,
    initial: AccountVersion,
    revisions: Vec<AccountVersion>,
    first_exposure_version: Option<VersionId>,
}

/// Validation/transition failures do not include private account contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureError {
    EmptyField(&'static str),
    FieldTooLong(&'static str),
    ControlCharacter(&'static str),
    StaleVersion,
    DuplicateVersion,
    VersionLimit,
    AllocationFailed,
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyField(name) => write!(f, "{name} must contain text or be unreported"),
            Self::FieldTooLong(name) => write!(f, "{name} exceeds the byte limit"),
            Self::ControlCharacter(name) => write!(f, "{name} contains unsupported controls"),
            Self::StaleVersion => f.write_str("capture changed; inspect the latest version"),
            Self::DuplicateVersion => f.write_str("version identifier was already used"),
            Self::VersionLimit => f.write_str("capture version limit reached"),
            Self::AllocationFailed => f.write_str("unable to reserve capture history"),
        }
    }
}

impl std::error::Error for CaptureError {}

fn validate_text(name: &'static str, value: &str) -> Result<(), CaptureError> {
    if value.len() > MAX_FIELD_BYTES {
        return Err(CaptureError::FieldTooLong(name));
    }
    if value.trim().is_empty() {
        return Err(CaptureError::EmptyField(name));
    }
    // Preserve LF, CR (including original CRLF) and tabs without normalization.
    if value
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\r' | '\n' | '\t'))
    {
        return Err(CaptureError::ControlCharacter(name));
    }
    Ok(())
}

impl WitnessForm {
    fn validate(&self) -> Result<(), CaptureError> {
        validate_text("narrative", &self.narrative)?;
        for (name, value) in [
            ("reported time", &self.reported_time),
            ("location", &self.location),
            ("duration", &self.duration),
            ("direction", &self.direction),
            ("apparent size", &self.apparent_size),
            ("motion", &self.motion),
            ("sound", &self.sound),
            ("conditions", &self.conditions),
        ] {
            if let Some(value) = value {
                validate_text(name, value)?;
            }
        }
        Ok(())
    }
}

impl WitnessCapture {
    /// Capture the initial wording before any explanation transition is possible.
    /// The caller must disclose any explanation exposure outside this workflow;
    /// this API cannot establish whether testimony was independently obtained.
    pub fn new(
        id: OpaqueId,
        origin: AccountOrigin,
        version: VersionId,
        actor: ActorId,
        form: WitnessForm,
    ) -> Result<Self, CaptureError> {
        form.validate()?;
        Ok(Self {
            id,
            origin,
            initial: AccountVersion {
                version,
                actor,
                form,
                rationale: None,
                after_explanations: false,
            },
            revisions: Vec::new(),
            first_exposure_version: None,
        })
    }

    pub fn id(&self) -> &OpaqueId {
        &self.id
    }

    pub fn origin(&self) -> AccountOrigin {
        self.origin
    }

    pub fn initial(&self) -> &AccountVersion {
        &self.initial
    }

    pub fn latest(&self) -> &AccountVersion {
        self.revisions.last().unwrap_or(&self.initial)
    }

    /// Revisions in append order; initial() is retained separately.
    pub fn revisions(&self) -> &[AccountVersion] {
        &self.revisions
    }

    /// Version current at the first in-workflow explanation exposure.
    pub fn first_exposure_version(&self) -> Option<&VersionId> {
        self.first_exposure_version.as_ref()
    }

    /// Record exposure immediately before the host shows candidate explanations.
    /// Repeated calls retain the first exposure; no transition can clear it.
    pub fn expose_explanations(&mut self, expected: &VersionId) -> Result<(), CaptureError> {
        if self.latest().version() != expected {
            return Err(CaptureError::StaleVersion);
        }
        if self.first_exposure_version.is_none() {
            self.first_exposure_version = Some(expected.clone());
        }
        Ok(())
    }

    /// Append exact revised wording. All validation occurs before history changes.
    /// Stale edits are rejected without deleting either the current or initial text.
    pub fn revise(
        &mut self,
        expected: &VersionId,
        version: VersionId,
        actor: ActorId,
        form: WitnessForm,
        rationale: String,
    ) -> Result<(), CaptureError> {
        if self.latest().version() != expected {
            return Err(CaptureError::StaleVersion);
        }
        if self.initial.version() == &version
            || self
                .revisions
                .iter()
                .any(|entry| entry.version() == &version)
        {
            return Err(CaptureError::DuplicateVersion);
        }
        if self.revisions.len() >= MAX_VERSIONS - 1 {
            return Err(CaptureError::VersionLimit);
        }
        form.validate()?;
        validate_text("revision rationale", &rationale)?;
        self.revisions
            .try_reserve(1)
            .map_err(|_| CaptureError::AllocationFailed)?;
        self.revisions.push(AccountVersion {
            version,
            actor,
            form,
            rationale: Some(rationale),
            after_explanations: self.first_exposure_version.is_some(),
        });
        Ok(())
    }
}
