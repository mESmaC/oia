//! Explicit, versioned trading sessions and closures with honest unknown coverage.

use std::collections::BTreeSet;

use oia_domain::{OpaqueId, VersionId};

use crate::model::CalendarDate;
use crate::{MarketError, MarketLimits, validate_text, validate_timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionKind {
    Ordinary,
    EarlyClose,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradingSession {
    pub local_date: CalendarDate,
    pub open_at_utc_ms: i64,
    pub close_at_utc_ms: i64,
    pub kind: SessionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClosureKind {
    Holiday,
    Weekend,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradingClosure {
    pub local_date: CalendarDate,
    pub kind: ClosureKind,
    pub reason: String,
}

/// Constructor input for a caller-normalized calendar version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradingCalendarInput {
    pub venue: OpaqueId,
    pub version: VersionId,
    pub source_version: VersionId,
    pub timezone_identifier: String,
    pub coverage_start: CalendarDate,
    pub coverage_end: CalendarDate,
    pub sessions: Vec<TradingSession>,
    pub closures: Vec<TradingClosure>,
}

/// A validated calendar. UTC session bounds are supplied explicitly; this type
/// does not invent timezone or DST conversions from the timezone identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradingCalendarVersion {
    input: TradingCalendarInput,
}

impl TradingCalendarVersion {
    pub fn new(input: TradingCalendarInput, limits: MarketLimits) -> Result<Self, MarketError> {
        validate_text("calendar timezone_identifier", &input.timezone_identifier)?;
        if input.coverage_start > input.coverage_end {
            return Err(MarketError::InvalidCalendar(
                "coverage start must not follow inclusive coverage end",
            ));
        }
        let entry_count = input
            .sessions
            .len()
            .checked_add(input.closures.len())
            .ok_or(MarketError::ArithmeticOverflow)?;
        limits.check_len(entry_count)?;
        let mut dates = BTreeSet::new();
        for session in &input.sessions {
            validate_calendar_date(session.local_date, input.coverage_start, input.coverage_end)?;
            validate_timestamp("session open_at_utc_ms", session.open_at_utc_ms)?;
            validate_timestamp("session close_at_utc_ms", session.close_at_utc_ms)?;
            if session.open_at_utc_ms >= session.close_at_utc_ms {
                return Err(MarketError::InvalidCalendar(
                    "session open must precede close",
                ));
            }
            if !dates.insert(session.local_date) {
                return Err(MarketError::InvalidCalendar(
                    "duplicate session or closure local date",
                ));
            }
        }
        for closure in &input.closures {
            validate_calendar_date(closure.local_date, input.coverage_start, input.coverage_end)?;
            validate_text("calendar closure reason", &closure.reason)?;
            if !dates.insert(closure.local_date) {
                return Err(MarketError::InvalidCalendar(
                    "duplicate session or closure local date",
                ));
            }
        }
        Ok(Self { input })
    }

    #[must_use]
    pub fn venue(&self) -> &OpaqueId {
        &self.input.venue
    }
    #[must_use]
    pub fn version(&self) -> &VersionId {
        &self.input.version
    }
    #[must_use]
    pub fn source_version(&self) -> &VersionId {
        &self.input.source_version
    }
    #[must_use]
    pub fn timezone_identifier(&self) -> &str {
        &self.input.timezone_identifier
    }
    #[must_use]
    pub fn sessions(&self) -> &[TradingSession] {
        &self.input.sessions
    }
    #[must_use]
    pub fn closures(&self) -> &[TradingClosure] {
        &self.input.closures
    }

    /// Missing entries inside or outside declared coverage are `Unknown`, not
    /// guessed openings or observed closures.
    #[must_use]
    pub fn calendar_day(&self, date: CalendarDate) -> CalendarDay<'_> {
        if date < self.input.coverage_start || date > self.input.coverage_end {
            return CalendarDay::Unknown;
        }
        for session in &self.input.sessions {
            if session.local_date == date {
                return CalendarDay::Open(session);
            }
        }
        for closure in &self.input.closures {
            if closure.local_date == date {
                return CalendarDay::Closed(closure);
            }
        }
        CalendarDay::Unknown
    }

    /// Session membership is half-open: the exact close instant is not open.
    pub fn session_at(
        &self,
        local_date: CalendarDate,
        at_utc_ms: i64,
    ) -> Result<SessionAt<'_>, MarketError> {
        validate_timestamp("session query instant", at_utc_ms)?;
        Ok(match self.calendar_day(local_date) {
            CalendarDay::Open(session)
                if at_utc_ms >= session.open_at_utc_ms && at_utc_ms < session.close_at_utc_ms =>
            {
                SessionAt::Open(session)
            }
            CalendarDay::Unknown => SessionAt::Unknown,
            _ => SessionAt::Closed,
        })
    }
}

fn validate_calendar_date(
    date: CalendarDate,
    coverage_start: CalendarDate,
    coverage_end: CalendarDate,
) -> Result<(), MarketError> {
    if date < coverage_start || date > coverage_end {
        Err(MarketError::InvalidCalendar(
            "session or closure falls outside declared coverage",
        ))
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalendarDay<'a> {
    Open(&'a TradingSession),
    Closed(&'a TradingClosure),
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionAt<'a> {
    Open(&'a TradingSession),
    Closed,
    Unknown,
}
