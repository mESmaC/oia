//! Bounded, provider-neutral point-in-time market data kernels.
//!
//! This crate normalizes caller-supplied, already-authorized records. It does not
//! acquire market data, select production providers, persist evidence, or resolve D11.

pub mod calendar;
pub mod model;
pub mod point_in_time;

use std::fmt;

use oia_domain::VersionId;

use crate::model::ExclusionReason;

/// Hard upper bound for any caller-configured record budget.
pub const MAX_RECORDS: usize = 10_000;
/// Hard bound for symbols, timezone names, reasons, and limitation notes.
pub const MAX_TEXT_BYTES: usize = 4_096;
/// Minimum accepted exact instant (`0001-01-01T00:00:00.000Z`).
pub const MIN_VALID_UTC_MS: i64 = -62_135_596_800_000;
/// Maximum accepted exact instant (`9999-12-31T23:59:59.999Z`).
pub const MAX_VALID_UTC_MS: i64 = 253_402_300_799_999;

/// Cooperative cancellation or deadline outcome. A stopped operation returns no result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    Cancelled,
    DeadlineExceeded,
}

/// Per-operation top-level record budget. Nested disclosure links have a separate
/// per-record bound, as documented by `ReleaseVintage` selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarketLimits {
    max_records: usize,
}

impl MarketLimits {
    pub fn new(max_records: usize) -> Result<Self, MarketError> {
        if max_records == 0 {
            return Err(MarketError::ZeroLimit);
        }
        if max_records > MAX_RECORDS {
            return Err(MarketError::LimitExceeded {
                actual: max_records,
                maximum: MAX_RECORDS,
            });
        }
        Ok(Self { max_records })
    }

    #[must_use]
    pub fn max_records(self) -> usize {
        self.max_records
    }

    pub(crate) fn check_len(self, actual: usize) -> Result<(), MarketError> {
        if actual > self.max_records {
            Err(MarketError::LimitExceeded {
                actual,
                maximum: self.max_records,
            })
        } else {
            Ok(())
        }
    }
}

/// Typed failures; no invalid or excluded record is silently substituted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarketError {
    ZeroLimit,
    LimitExceeded {
        actual: usize,
        maximum: usize,
    },
    AllocationFailed,
    InvalidText {
        field: &'static str,
    },
    TextTooLong {
        field: &'static str,
        actual: usize,
    },
    TimestampOutOfRange {
        field: &'static str,
    },
    InvalidDate(&'static str),
    InvalidInterval(&'static str),
    InvalidInstrument(&'static str),
    AliasNotFound,
    AmbiguousAlias,
    InvalidCalendar(&'static str),
    InvalidQuote(&'static str),
    InvalidAction(&'static str),
    InvalidRelease(&'static str),
    InvalidDecimalScale(u32),
    UnsupportedCoefficient,
    InvalidRatio,
    ArithmeticOverflow,
    DuplicateVersion(VersionId),
    DuplicateRevision(u32),
    ScopeMismatch(&'static str),
    UnitMismatch,
    UnsupportedAdjustment,
    QuoteExcluded {
        version: VersionId,
        reason: ExclusionReason,
    },
    Stopped(StopReason),
}

impl fmt::Display for MarketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroLimit => f.write_str("market record limit must be greater than zero"),
            Self::LimitExceeded { actual, maximum } => {
                write!(f, "market input has {actual} records; limit is {maximum}")
            }
            Self::AllocationFailed => f.write_str("unable to allocate market result metadata"),
            Self::InvalidText { field } => write!(f, "{field} must contain non-whitespace text"),
            Self::TextTooLong { field, actual } => write!(
                f,
                "{field} has {actual} UTF-8 bytes; limit is {MAX_TEXT_BYTES}"
            ),
            Self::TimestampOutOfRange { field } => write!(f, "{field} is outside the supported UTC range"),
            Self::InvalidDate(message) => write!(f, "invalid calendar date: {message}"),
            Self::InvalidInterval(message) => write!(f, "invalid interval: {message}"),
            Self::InvalidInstrument(message) => write!(f, "invalid instrument: {message}"),
            Self::AliasNotFound => f.write_str("no listing alias matches the venue, symbol, and date"),
            Self::AmbiguousAlias => f.write_str("more than one instrument matches the listing alias"),
            Self::InvalidCalendar(message) => write!(f, "invalid trading calendar: {message}"),
            Self::InvalidQuote(message) => write!(f, "invalid quote: {message}"),
            Self::InvalidAction(message) => write!(f, "invalid corporate action: {message}"),
            Self::InvalidRelease(message) => write!(f, "invalid release vintage: {message}"),
            Self::InvalidDecimalScale(scale) => write!(f, "decimal scale {scale} exceeds 18"),
            Self::UnsupportedCoefficient => f.write_str("the minimum signed integer is outside the exact arithmetic domain"),
            Self::InvalidRatio => f.write_str("ratio must have nonzero, positive supported terms"),
            Self::ArithmeticOverflow => f.write_str("exact market arithmetic overflowed"),
            Self::DuplicateVersion(version) => write!(f, "duplicate immutable version {version}"),
            Self::DuplicateRevision(revision) => write!(f, "duplicate release revision {revision}"),
            Self::ScopeMismatch(field) => write!(f, "market query scope mismatch: {field}"),
            Self::UnitMismatch => f.write_str("market records use inconsistent units"),
            Self::UnsupportedAdjustment => f.write_str("total-return adjustment is not implemented; use unadjusted or split-only adjustment"),
            Self::QuoteExcluded { version, reason } => {
                write!(f, "quote {version} is inadmissible at the cutoff: {reason:?}")
            }
            Self::Stopped(reason) => write!(f, "market operation stopped: {reason:?}"),
        }
    }
}

impl std::error::Error for MarketError {}

pub(crate) fn validate_text(field: &'static str, text: &str) -> Result<(), MarketError> {
    if text.len() > MAX_TEXT_BYTES {
        return Err(MarketError::TextTooLong {
            field,
            actual: text.len(),
        });
    }
    if text.trim().is_empty() {
        return Err(MarketError::InvalidText { field });
    }
    Ok(())
}

pub(crate) fn validate_timestamp(field: &'static str, value: i64) -> Result<(), MarketError> {
    if (MIN_VALID_UTC_MS..=MAX_VALID_UTC_MS).contains(&value) {
        Ok(())
    } else {
        Err(MarketError::TimestampOutOfRange { field })
    }
}
