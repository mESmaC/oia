//! Validated opaque domain identifiers, immutable version IDs, and schema versions.
//!
//! Identifiers are independent of mutable names, URLs, ticker symbols, and database
//! row positions (`docs/architecture.md` Section 2.1).

use std::fmt;

/// Maximum byte length of an opaque identifier.
pub const MAX_ID_LEN: usize = 128;

/// Errors returned when validating an identifier or schema version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdError {
    /// Identifier was empty.
    Empty,
    /// Identifier exceeded [`MAX_ID_LEN`].
    TooLong(usize),
    /// Identifier contained a disallowed character (e.g., path separators, whitespace, or control chars).
    InvalidCharacter(char),
    /// Schema version was zero.
    ZeroSchemaVersion,
}

impl fmt::Display for IdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "identifier must not be empty"),
            Self::TooLong(len) => {
                write!(f, "identifier length {len} exceeds maximum {MAX_ID_LEN}")
            }
            Self::InvalidCharacter(ch) => {
                write!(f, "identifier contains disallowed character {ch:?}")
            }
            Self::ZeroSchemaVersion => write!(f, "schema version must be >= 1"),
        }
    }
}

impl std::error::Error for IdError {}

fn validate_id_str(raw: &str) -> Result<(), IdError> {
    if raw.is_empty() {
        return Err(IdError::Empty);
    }
    if raw.len() > MAX_ID_LEN {
        return Err(IdError::TooLong(raw.len()));
    }
    for ch in raw.chars() {
        if !(ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | ':' | '.')) {
            return Err(IdError::InvalidCharacter(ch));
        }
    }
    if raw == "." || raw == ".." || raw.contains("..") {
        return Err(IdError::InvalidCharacter('.'));
    }
    Ok(())
}

/// Stable opaque object identifier (`SourceId`, `InvestigationId`, `ClaimId`, etc.).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OpaqueId(String);

impl OpaqueId {
    /// Validates and constructs an [`OpaqueId`].
    pub fn new(raw: impl Into<String>) -> Result<Self, IdError> {
        let s = raw.into();
        validate_id_str(&s)?;
        Ok(Self(s))
    }

    /// Returns the identifier string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OpaqueId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Immutable version identifier (`VersionId`) that is never reused across revisions.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VersionId(String);

impl VersionId {
    /// Validates and constructs an immutable [`VersionId`].
    pub fn new(raw: impl Into<String>) -> Result<Self, IdError> {
        let s = raw.into();
        validate_id_str(&s)?;
        Ok(Self(s))
    }

    /// Returns the version identifier string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for VersionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Identifier of an actor or process that created a version or review.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ActorId(String);

impl ActorId {
    /// Validates and constructs an [`ActorId`].
    pub fn new(raw: impl Into<String>) -> Result<Self, IdError> {
        let s = raw.into();
        validate_id_str(&s)?;
        Ok(Self(s))
    }

    /// Returns the actor identifier string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ActorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Non-zero schema version number for persisted domain records and manifests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SchemaVersion(u32);

impl SchemaVersion {
    /// Current v1 domain schema version.
    pub const V1: Self = Self(1);

    /// Validates that `version >= 1`.
    pub const fn new(version: u32) -> Result<Self, IdError> {
        if version == 0 {
            Err(IdError::ZeroSchemaVersion)
        } else {
            Ok(Self(version))
        }
    }

    /// Returns the numeric schema version.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}
