use std::fmt;

use oia_domain::evidence::UnknownReason;

use crate::TimeGeoError;

/// Nonempty half-open interval of Unix-epoch UTC milliseconds `[start, end)`.
///
/// This timeline convention does not represent leap seconds, uncertain instants,
/// calendar dates or source-clock alignment. No date/time-zone conversion is implied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeWindow {
    start_utc_ms: i64,
    end_utc_ms: i64,
}

impl TimeWindow {
    pub fn new(start_utc_ms: i64, end_utc_ms: i64) -> Result<Self, TimeGeoError> {
        let tw = Self {
            start_utc_ms,
            end_utc_ms,
        };
        tw.validate()?;
        Ok(tw)
    }

    pub fn validate(&self) -> Result<(), TimeGeoError> {
        if self.start_utc_ms >= self.end_utc_ms {
            return Err(TimeGeoError::InvalidTimeWindow {
                start_utc_ms: self.start_utc_ms,
                end_utc_ms: self.end_utc_ms,
            });
        }
        Ok(())
    }

    #[inline]
    #[must_use]
    pub fn start_utc_ms(&self) -> i64 {
        self.start_utc_ms
    }

    #[inline]
    #[must_use]
    pub fn end_utc_ms(&self) -> i64 {
        self.end_utc_ms
    }

    #[inline]
    #[must_use]
    pub fn contains(&self, instant_utc_ms: i64) -> bool {
        instant_utc_ms >= self.start_utc_ms && instant_utc_ms < self.end_utc_ms
    }

    #[inline]
    #[must_use]
    pub fn overlaps(&self, other: &TimeWindow) -> bool {
        self.start_utc_ms < other.end_utc_ms && other.start_utc_ms < self.end_utc_ms
    }

    #[inline]
    #[must_use]
    pub fn contains_window(&self, other: &TimeWindow) -> bool {
        self.start_utc_ms <= other.start_utc_ms && self.end_utc_ms >= other.end_utc_ms
    }
}

/// Whether `year` is a proleptic Gregorian leap year.
#[inline]
#[must_use]
pub const fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Days in `month` (`1..=12`) of proleptic Gregorian `year`, or `None` if `month` is outside `1..=12`.
#[inline]
#[must_use]
pub const fn days_in_month(year: i64, month: u8) -> Option<u8> {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Some(31),
        4 | 6 | 9 | 11 => Some(30),
        2 => Some(if is_leap_year(year) { 29 } else { 28 }),
        _ => None,
    }
}

/// Howard Hinnant's `days_from_civil`: whole days relative to `1970-01-01` for a valid
/// proleptic Gregorian date `(year, month, day)`.
#[inline]
#[must_use]
pub const fn days_from_civil(year: i64, month: u8, day: u8) -> i64 {
    let m = month as i64;
    let d = day as i64;
    let y = if m <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Howard Hinnant's `civil_from_days`: proleptic Gregorian `(year, month, day)` for
/// `days` relative to `1970-01-01`.
#[inline]
#[must_use]
pub const fn civil_from_days(days: i64) -> (i64, u8, u8) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    (year, month as u8, day as u8)
}

/// A validated proleptic Gregorian calendar date (`-9999..=9999`) without a time of day.
///
/// Date-only records are never implicitly converted to `00:00:00Z` midnight
/// (`docs/architecture.md` §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CalendarDate {
    year: i32,
    month: u8,
    day: u8,
}

impl CalendarDate {
    /// Minimum supported proleptic Gregorian year.
    pub const MIN_YEAR: i32 = -9999;
    /// Maximum supported proleptic Gregorian year.
    pub const MAX_YEAR: i32 = 9999;

    /// Validates `year` (`-9999..=9999`), `month` (`1..=12`), and `day` (`1..=days_in_month`).
    pub const fn new(year: i32, month: u8, day: u8) -> Result<Self, TimeGeoError> {
        if year < Self::MIN_YEAR || year > Self::MAX_YEAR {
            return Err(TimeGeoError::InvalidCalendarDate { year, month, day });
        }
        let Some(max_day) = days_in_month(year as i64, month) else {
            return Err(TimeGeoError::InvalidCalendarDate { year, month, day });
        };
        if day == 0 || day > max_day {
            return Err(TimeGeoError::InvalidCalendarDate { year, month, day });
        }
        Ok(Self { year, month, day })
    }

    /// Parses a strict 10-byte `YYYY-MM-DD` date (`0000..=9999`), validating month and leap-year days.
    pub fn parse_yyyy_mm_dd(text: &str) -> Result<Self, TimeGeoError> {
        let bytes = text.as_bytes();
        let invalid = || TimeGeoError::InvalidCalendarDate {
            year: 0,
            month: 0,
            day: 0,
        };
        if bytes.len() != 10 || bytes.get(4) != Some(&b'-') || bytes.get(7) != Some(&b'-') {
            return Err(invalid());
        }
        let year: i32 =
            parse_ascii_digits(text.get(0..4).ok_or_else(invalid)?).ok_or_else(invalid)?;
        let month: u8 =
            parse_ascii_digits(text.get(5..7).ok_or_else(invalid)?).ok_or_else(invalid)?;
        let day: u8 =
            parse_ascii_digits(text.get(8..10).ok_or_else(invalid)?).ok_or_else(invalid)?;
        Self::new(year, month, day)
    }

    /// Converts whole days relative to `1970-01-01` into a `CalendarDate` if within `-9999..=9999`.
    pub fn from_epoch_days(days: i64) -> Result<Self, TimeGeoError> {
        let (year_i64, month, day) = civil_from_days(days);
        let year = i32::try_from(year_i64).map_err(|_| TimeGeoError::InvalidCalendarDate {
            year: if year_i64 < 0 { i32::MIN } else { i32::MAX },
            month,
            day,
        })?;
        Self::new(year, month, day)
    }

    /// Whole days relative to `1970-01-01`.
    #[inline]
    #[must_use]
    pub const fn to_epoch_days(self) -> i64 {
        days_from_civil(self.year as i64, self.month, self.day)
    }

    #[inline]
    #[must_use]
    pub const fn year(self) -> i32 {
        self.year
    }

    #[inline]
    #[must_use]
    pub const fn month(self) -> u8 {
        self.month
    }

    #[inline]
    #[must_use]
    pub const fn day(self) -> u8 {
        self.day
    }
}

impl fmt::Display for CalendarDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.year < 0 {
            write!(
                f,
                "-{:04}-{:02}-{:02}",
                self.year.unsigned_abs(),
                self.month,
                self.day
            )
        } else {
            write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
        }
    }
}

fn parse_ascii_digits<T: std::str::FromStr>(slice: &str) -> Option<T> {
    if slice.is_empty() || !slice.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    slice.parse::<T>().ok()
}

/// Checks whether `ts` has the exact 20-byte ASCII shape `YYYY-MM-DDTHH:MM:SSZ`
/// (without checking calendar month/day or hour/minute/second bounds).
///
/// Use [`parse_rfc3339_utc_seconds`] when full calendar and range validation is required.
#[must_use]
pub fn has_rfc3339_utc_second_shape(ts: &str) -> bool {
    let bytes = ts.as_bytes();
    if bytes.len() != 20 {
        return false;
    }
    let digit_indices = [0usize, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18];
    for idx in digit_indices {
        if !bytes.get(idx).is_some_and(u8::is_ascii_digit) {
            return false;
        }
    }
    bytes.get(4) == Some(&b'-')
        && bytes.get(7) == Some(&b'-')
        && bytes.get(10) == Some(&b'T')
        && bytes.get(13) == Some(&b':')
        && bytes.get(16) == Some(&b':')
        && bytes.get(19) == Some(&b'Z')
}

/// Parses a strict 20-byte `YYYY-MM-DDTHH:MM:SSZ` UTC timestamp into Unix epoch seconds,
/// rejecting out-of-range calendar dates, hours (`> 23`), minutes (`> 59`), and seconds (`> 59`,
/// including `:60` leap seconds).
pub fn parse_rfc3339_utc_seconds(ts: &str) -> Result<i64, TimeGeoError> {
    let err = || TimeGeoError::InvalidRfc3339Timestamp {
        value: ts.to_owned(),
    };
    if !has_rfc3339_utc_second_shape(ts) {
        return Err(err());
    }
    let year: i32 = parse_ascii_digits(ts.get(0..4).ok_or_else(err)?).ok_or_else(err)?;
    let month: u8 = parse_ascii_digits(ts.get(5..7).ok_or_else(err)?).ok_or_else(err)?;
    let day: u8 = parse_ascii_digits(ts.get(8..10).ok_or_else(err)?).ok_or_else(err)?;
    let hour: i64 = parse_ascii_digits(ts.get(11..13).ok_or_else(err)?).ok_or_else(err)?;
    let minute: i64 = parse_ascii_digits(ts.get(14..16).ok_or_else(err)?).ok_or_else(err)?;
    let second: i64 = parse_ascii_digits(ts.get(17..19).ok_or_else(err)?).ok_or_else(err)?;
    let date = CalendarDate::new(year, month, day).map_err(|_| err())?;
    if hour > 23 || minute > 59 || second > 59 {
        return Err(err());
    }
    Ok(date.to_epoch_days() * 86_400 + hour * 3_600 + minute * 60 + second)
}

/// Parses an RFC 3339 timestamp (`YYYY-MM-DD[T|t]HH:MM:SS[.f{1..9}][Z|z|[+-]HH:MM]`) into
/// Unix-epoch UTC nanoseconds (`i128`).
///
/// Validates calendar month/day and leap years, fractional nanoseconds (1 to 9 digits),
/// timezone offsets (`hour <= 23`, `minute <= 59`), and leap seconds (`second == 60` is permitted
/// only when the UTC time-of-day is `23:59:60Z`, folding to `:59.999999999` on the POSIX-style
/// nanosecond axis).
pub fn parse_rfc3339_utc_nanos(input: &str) -> Result<i128, TimeGeoError> {
    let err = || TimeGeoError::InvalidRfc3339Timestamp {
        value: input.to_owned(),
    };
    if input.len() < 20 {
        return Err(err());
    }
    let year: i32 = parse_ascii_digits(input.get(0..4).ok_or_else(err)?).ok_or_else(err)?;
    if input.get(4..5) != Some("-") {
        return Err(err());
    }
    let month: u8 = parse_ascii_digits(input.get(5..7).ok_or_else(err)?).ok_or_else(err)?;
    if input.get(7..8) != Some("-") {
        return Err(err());
    }
    let day: u8 = parse_ascii_digits(input.get(8..10).ok_or_else(err)?).ok_or_else(err)?;
    let sep = input.get(10..11).ok_or_else(err)?;
    if sep != "T" && sep != "t" {
        return Err(err());
    }
    let hour: u32 = parse_ascii_digits(input.get(11..13).ok_or_else(err)?).ok_or_else(err)?;
    if input.get(13..14) != Some(":") {
        return Err(err());
    }
    let minute: u32 = parse_ascii_digits(input.get(14..16).ok_or_else(err)?).ok_or_else(err)?;
    if input.get(16..17) != Some(":") {
        return Err(err());
    }
    let second: u32 = parse_ascii_digits(input.get(17..19).ok_or_else(err)?).ok_or_else(err)?;

    let date = CalendarDate::new(year, month, day).map_err(|_| err())?;
    if hour > 23 || minute > 59 || second > 60 {
        return Err(err());
    }

    let tail = input.get(19..).ok_or_else(err)?;
    let (nanos, tz_tail) = if let Some(after_dot) = tail.strip_prefix('.') {
        let tz_pos = after_dot.find(['Z', 'z', '+', '-']).ok_or_else(err)?;
        let frac = after_dot.get(..tz_pos).ok_or_else(err)?;
        if frac.is_empty() || frac.len() > 9 || !frac.bytes().all(|b| b.is_ascii_digit()) {
            return Err(err());
        }
        let mut parsed_frac: i128 = frac.parse().map_err(|_| err())?;
        for _ in frac.len()..9 {
            parsed_frac *= 10;
        }
        (parsed_frac, after_dot.get(tz_pos..).ok_or_else(err)?)
    } else {
        (0i128, tail)
    };

    let offset_seconds: i64 = if tz_tail == "Z" || tz_tail == "z" {
        0
    } else {
        if tz_tail.len() != 6 {
            return Err(err());
        }
        let sign: i64 = match tz_tail.get(0..1) {
            Some("+") => 1,
            Some("-") => -1,
            _ => return Err(err()),
        };
        if tz_tail.get(3..4) != Some(":") {
            return Err(err());
        }
        let off_h: u32 = parse_ascii_digits(tz_tail.get(1..3).ok_or_else(err)?).ok_or_else(err)?;
        let off_m: u32 = parse_ascii_digits(tz_tail.get(4..6).ok_or_else(err)?).ok_or_else(err)?;
        if off_h > 23 || off_m > 59 {
            return Err(err());
        }
        sign * (i64::from(off_h) * 3600 + i64::from(off_m) * 60)
    };

    let days = date.to_epoch_days();
    let base_local_seconds = days * 86_400 + i64::from(hour) * 3600 + i64::from(minute) * 60;
    let base_utc_seconds = base_local_seconds - offset_seconds;
    if second == 60 && base_utc_seconds.rem_euclid(86_400) != 86_340 {
        return Err(err());
    }
    let utc_seconds = base_utc_seconds + i64::from(second.min(59));
    let leap_adjust = if second == 60 { 999_999_999 } else { nanos };
    Ok(i128::from(utc_seconds) * 1_000_000_000 + leap_adjust)
}

/// Daylight-saving or local wall-clock ambiguity status (`docs/architecture.md` §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DstAmbiguity {
    /// Local wall-clock time maps uniquely to a single UTC instant.
    Unambiguous,
    /// Fall-back transition: earlier (pre-transition / DST) occurrence selected.
    FoldEarlierOccurrence,
    /// Fall-back transition: later (post-transition / standard) occurrence selected.
    FoldLaterOccurrence,
    /// Fall-back transition reported without disambiguating which occurrence (`earlier_utc_ms < later_utc_ms`).
    AmbiguousFoldUnresolved {
        earlier_utc_ms: i64,
        later_utc_ms: i64,
    },
    /// Spring-forward gap: reported local wall-clock time did not exist locally.
    SpringForwardGap,
}

impl DstAmbiguity {
    pub fn validate(&self) -> Result<(), TimeGeoError> {
        if let Self::AmbiguousFoldUnresolved {
            earlier_utc_ms,
            later_utc_ms,
        } = self
            && earlier_utc_ms >= later_utc_ms
        {
            return Err(TimeGeoError::InvalidTimeWindow {
                start_utc_ms: *earlier_utc_ms,
                end_utc_ms: *later_utc_ms,
            });
        }
        Ok(())
    }
}

/// Preserved local timezone context alongside normalized UTC comparison timestamps (`GEO-01`, `docs/architecture.md` §3.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalTimeContext {
    /// IANA timezone identifier or declared offset label (e.g., `"America/Phoenix"`, `"UTC+09:00"`).
    pub zone_name: String,
    /// Fixed UTC offset in minutes (`-840..=+840`), if known at the instant.
    pub utc_offset_minutes: Option<i16>,
    /// DST fold/gap ambiguity status.
    pub ambiguity: DstAmbiguity,
    /// Original textual representation from the source or witness.
    pub original_text: String,
}

impl LocalTimeContext {
    /// Minimum valid UTC offset in minutes (`-14:00`).
    pub const MIN_UTC_OFFSET_MINUTES: i16 = -840;
    /// Maximum valid UTC offset in minutes (`+14:00`).
    pub const MAX_UTC_OFFSET_MINUTES: i16 = 840;

    /// Validates non-empty zone/text and offset bounds (`-840..=+840` minutes).
    pub fn validate(&self) -> Result<(), TimeGeoError> {
        if self.zone_name.trim().is_empty() {
            return Err(TimeGeoError::EmptyField("zone_name"));
        }
        if self.original_text.trim().is_empty() {
            return Err(TimeGeoError::EmptyField("original_text"));
        }
        if let Some(off) = self.utc_offset_minutes
            && !(Self::MIN_UTC_OFFSET_MINUTES..=Self::MAX_UTC_OFFSET_MINUTES).contains(&off)
        {
            return Err(TimeGeoError::InvalidUtcOffsetMinutes(off));
        }
        self.ambiguity.validate()
    }
}

/// Time representation preserving exact instants, half-open intervals, date-only records,
/// approximate windows, and unknown times (`GEO-01`, `docs/architecture.md` §3.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimeRepresentation {
    /// Exact timestamp in UTC milliseconds, with optional preserved local timezone context.
    ExactInstant {
        utc_ms: i64,
        local: Option<LocalTimeContext>,
    },
    /// Half-open interval `[start_utc_ms, end_utc_ms)` (`start < end`), with optional local context.
    BoundedInterval {
        window: TimeWindow,
        local: Option<LocalTimeContext>,
    },
    /// Calendar date without a known time of day.
    ///
    /// Never implicitly converted to `00:00:00Z` midnight (`docs/architecture.md` §3.1).
    CalendarDate {
        date: CalendarDate,
        original_text: String,
    },
    /// Approximate time window with inclusive `[earliest_utc_ms, latest_utc_ms]` bounds and original phrasing.
    ApproximateTime {
        earliest_utc_ms: i64,
        latest_utc_ms: i64,
        original_text: String,
    },
    /// Unknown time with a canonical [`UnknownReason`] and optional verbatim source phrasing.
    UnknownTime {
        reason: UnknownReason,
        original_text: Option<String>,
    },
}

impl TimeRepresentation {
    /// Validates structural invariants (local context, approximate bounds, non-empty original text).
    pub fn validate(&self) -> Result<(), TimeGeoError> {
        match self {
            Self::ExactInstant { local, .. } => {
                if let Some(ctx) = local {
                    ctx.validate()?;
                }
                Ok(())
            }
            Self::BoundedInterval { window, local } => {
                window.validate()?;
                if let Some(ctx) = local {
                    ctx.validate()?;
                }
                Ok(())
            }
            Self::CalendarDate {
                date,
                original_text,
            } => {
                let _ = CalendarDate::new(date.year(), date.month(), date.day())?;
                if original_text.trim().is_empty() {
                    return Err(TimeGeoError::EmptyField("original_text"));
                }
                Ok(())
            }
            Self::ApproximateTime {
                earliest_utc_ms,
                latest_utc_ms,
                original_text,
            } => {
                if earliest_utc_ms > latest_utc_ms {
                    return Err(TimeGeoError::InvalidApproximateBounds {
                        earliest_utc_ms: *earliest_utc_ms,
                        latest_utc_ms: *latest_utc_ms,
                    });
                }
                if original_text.trim().is_empty() {
                    return Err(TimeGeoError::EmptyField("original_text"));
                }
                Ok(())
            }
            Self::UnknownTime { original_text, .. } => {
                if let Some(text) = original_text
                    && text.trim().is_empty()
                {
                    return Err(TimeGeoError::EmptyField("original_text"));
                }
                Ok(())
            }
        }
    }

    /// Evaluates whether this time representation overlaps the half-open query window `[query.start, query.end)`.
    ///
    /// Returns `None` for [`Self::CalendarDate`] and [`Self::UnknownTime`] because date-only or
    /// unknown times cannot be resolved against a sub-day UTC window without fabricating a
    /// midnight timestamp or assuming an offset (`docs/architecture.md` §3.1).
    /// Also returns `None` for [`Self::ExactInstant`] when its local context carries an
    /// unresolved DST fold ([`DstAmbiguity::AmbiguousFoldUnresolved`]) and one candidate falls
    /// inside `query` while the other falls outside.
    #[must_use]
    pub fn overlaps_utc_window(&self, query: &TimeWindow) -> Option<bool> {
        match self {
            Self::ExactInstant { utc_ms, local } => {
                if let Some(LocalTimeContext {
                    ambiguity:
                        DstAmbiguity::AmbiguousFoldUnresolved {
                            earlier_utc_ms,
                            later_utc_ms,
                        },
                    ..
                }) = local
                {
                    let in_earlier = query.contains(*earlier_utc_ms);
                    let in_later = query.contains(*later_utc_ms);
                    if in_earlier == in_later {
                        Some(in_earlier)
                    } else {
                        None
                    }
                } else {
                    Some(query.contains(*utc_ms))
                }
            }
            Self::BoundedInterval { window, .. } => Some(window.overlaps(query)),
            Self::ApproximateTime {
                earliest_utc_ms,
                latest_utc_ms,
                ..
            } => Some(
                *earliest_utc_ms < query.end_utc_ms() && *latest_utc_ms >= query.start_utc_ms(),
            ),
            Self::CalendarDate { .. } | Self::UnknownTime { .. } => None,
        }
    }
}

/// Scientific time scale (`SPACE-05`, `docs/architecture.md` §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TimeScale {
    /// Coordinated Universal Time.
    Utc,
    /// International Atomic Time (`TAI = UTC + delta_AT`).
    Tai,
    /// Terrestrial Time (`TT = TAI + 32.184 s`).
    Tt,
    /// Barycentric Dynamical Time (periodic relativistic offset from TT).
    Tdb,
}

/// Exact fixed offset between TAI and TT in milliseconds (`32.184 s = 32,184 ms`).
pub const TT_MINUS_TAI_MS: i64 = 32_184;

/// Versioned leap-second table for `UTC <-> TAI <-> TT` conversions (`SPACE-05`, `docs/architecture.md` §3.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeapSecondTable {
    /// Table version identifier (e.g., `"IERS-Bulletin-C-71"`).
    pub version: String,
    /// Expiry timestamp (UTC ms) beyond which `TAI - UTC` is not yet established by this bulletin.
    pub expires_at_utc_ms: i64,
    /// Sorted list of `(effective_utc_ms, tai_minus_utc_seconds)`.
    entries: Vec<(i64, i32)>,
}

impl LeapSecondTable {
    /// Returns the complete IERS leap-second table containing all 28 historical step introductions
    /// from `1972-01-01T00:00:00Z` (`10 s`) through `2017-01-01T00:00:00Z` (`37 s`) with the
    /// IERS Bulletin C 71 validity horizon (`2026-12-28T00:00:00Z`).
    #[must_use]
    pub fn iers_standard() -> Self {
        Self {
            version: "IERS-Bulletin-C-71".to_owned(),
            expires_at_utc_ms: 1_798_416_000_000, // 2026-12-28T00:00:00Z
            entries: vec![
                (63_072_000_000, 10),    // 1972-01-01T00:00:00Z
                (78_796_800_000, 11),    // 1972-07-01T00:00:00Z
                (94_694_400_000, 12),    // 1973-01-01T00:00:00Z
                (126_230_400_000, 13),   // 1974-01-01T00:00:00Z
                (157_766_400_000, 14),   // 1975-01-01T00:00:00Z
                (189_302_400_000, 15),   // 1976-01-01T00:00:00Z
                (220_924_800_000, 16),   // 1977-01-01T00:00:00Z
                (252_460_800_000, 17),   // 1978-01-01T00:00:00Z
                (283_996_800_000, 18),   // 1979-01-01T00:00:00Z
                (315_532_800_000, 19),   // 1980-01-01T00:00:00Z
                (362_793_600_000, 20),   // 1981-07-01T00:00:00Z
                (394_329_600_000, 21),   // 1982-07-01T00:00:00Z
                (425_865_600_000, 22),   // 1983-07-01T00:00:00Z
                (489_024_000_000, 23),   // 1985-07-01T00:00:00Z
                (567_993_600_000, 24),   // 1988-01-01T00:00:00Z
                (631_152_000_000, 25),   // 1990-01-01T00:00:00Z
                (662_688_000_000, 26),   // 1991-01-01T00:00:00Z
                (709_948_800_000, 27),   // 1992-07-01T00:00:00Z
                (741_484_800_000, 28),   // 1993-07-01T00:00:00Z
                (773_020_800_000, 29),   // 1994-07-01T00:00:00Z
                (820_454_400_000, 30),   // 1996-01-01T00:00:00Z
                (867_715_200_000, 31),   // 1997-07-01T00:00:00Z
                (915_148_800_000, 32),   // 1999-01-01T00:00:00Z
                (1_136_073_600_000, 33), // 2006-01-01T00:00:00Z
                (1_230_768_000_000, 34), // 2009-01-01T00:00:00Z
                (1_341_100_800_000, 35), // 2012-07-01T00:00:00Z
                (1_435_708_800_000, 36), // 2015-07-01T00:00:00Z
                (1_483_228_800_000, 37), // 2017-01-01T00:00:00Z
            ],
        }
    }

    /// Looks up `TAI - UTC` in whole seconds for `utc_ms` (`1972-01-01 <= utc_ms < expires_at_utc_ms`).
    pub fn tai_minus_utc_seconds(&self, utc_ms: i64) -> Result<i32, TimeGeoError> {
        if utc_ms >= self.expires_at_utc_ms {
            return Err(TimeGeoError::LeapTableExpired {
                utc_ms,
                expires_at_utc_ms: self.expires_at_utc_ms,
            });
        }
        let mut selected = None;
        for &(effective_ms, delta_s) in &self.entries {
            if utc_ms >= effective_ms {
                selected = Some(delta_s);
            } else {
                break;
            }
        }
        selected.ok_or(TimeGeoError::PreLeapSecondEpoch(utc_ms))
    }

    /// Converts a `UTC` millisecond timestamp into `TAI` milliseconds using checked arithmetic.
    pub fn utc_to_tai_ms(&self, utc_ms: i64) -> Result<i64, TimeGeoError> {
        let delta_s = i64::from(self.tai_minus_utc_seconds(utc_ms)?);
        let delta_ms = delta_s
            .checked_mul(1_000)
            .ok_or(TimeGeoError::TimestampArithmeticOverflow)?;
        utc_ms
            .checked_add(delta_ms)
            .ok_or(TimeGeoError::TimestampArithmeticOverflow)
    }

    /// Converts a `UTC` millisecond timestamp into Terrestrial Time (`TT`) milliseconds (`TAI + 32,184 ms`).
    pub fn utc_to_tt_ms(&self, utc_ms: i64) -> Result<i64, TimeGeoError> {
        let tai_ms = self.utc_to_tai_ms(utc_ms)?;
        tai_ms
            .checked_add(TT_MINUS_TAI_MS)
            .ok_or(TimeGeoError::TimestampArithmeticOverflow)
    }
}

/// Clock synchronization state for a device or stream (`DEVICE-04`, `docs/architecture.md` §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockSyncState {
    /// Device clock offset and uncertainty are bounded within the validity interval.
    Synchronized,
    /// Clock drift exceeded nominal threshold; alignment carries elevated uncertainty.
    DegradedDrift,
    /// Clock synchronization lost; receipt timestamps must NOT be substituted as capture times.
    Unsynchronized,
}

/// Aligned UTC timestamp and its effective uncertainty from a [`ClockAlignment`] record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlignedCaptureTime {
    /// Estimated capture instant in UTC milliseconds (`device_clock_ms + offset_ms`).
    pub aligned_utc_ms: i64,
    /// Effective symmetric alignment uncertainty in milliseconds (`base + drift_contribution`).
    pub effective_uncertainty_ms: u32,
    /// Synchronization state at the time of alignment (`Synchronized` or `DegradedDrift`).
    pub sync_state: ClockSyncState,
}

/// Clock alignment record between a source/device clock and a reference clock (`DEVICE-04`, `docs/architecture.md` §3.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClockAlignment {
    /// Reference clock identifier (e.g., `"NTP_POOL_UTC"`, `"GNSS_PPS"`).
    pub reference_clock: String,
    /// Alignment method description (e.g., `"GNSS 1PPS discipline"`, `"RTCP sender report"`).
    pub method: String,
    /// Estimated offset in milliseconds (`utc_ms = device_clock_ms + offset_ms`) at `valid_from_utc_ms`.
    pub offset_ms: i64,
    /// Estimated clock drift rate in parts-per-million (`ppm`).
    pub drift_ppm: i32,
    /// Base symmetric alignment uncertainty in milliseconds (`>= 0`).
    pub uncertainty_ms: u32,
    /// Validity window `[valid_from_utc_ms, valid_until_utc_ms)` in UTC milliseconds.
    pub validity_window: TimeWindow,
    /// Synchronization state.
    pub sync_state: ClockSyncState,
}

impl ClockAlignment {
    /// Validates non-empty `reference_clock`, non-empty `method`, and `validity_window`.
    pub fn validate(&self) -> Result<(), TimeGeoError> {
        if self.reference_clock.trim().is_empty() {
            return Err(TimeGeoError::EmptyField("reference_clock"));
        }
        if self.method.trim().is_empty() {
            return Err(TimeGeoError::EmptyField("method"));
        }
        self.validity_window.validate()
    }

    /// Aligns a raw device timestamp (`device_clock_ms`) to UTC if synchronization is valid.
    ///
    /// - Rejects `ClockSyncState::Unsynchronized` with [`TimeGeoError::UnsynchronizedClock`]
    ///   so receipt time is never substituted for capture time.
    /// - Rejects aligned timestamps outside `validity_window` with
    ///   [`TimeGeoError::ClockAlignmentOutOfWindow`].
    /// - Adds linear drift uncertainty `ceil(|drift_ppm| * elapsed_ms / 1_000_000)` to `uncertainty_ms`.
    pub fn align_device_time(
        &self,
        device_clock_ms: i64,
    ) -> Result<AlignedCaptureTime, TimeGeoError> {
        self.validate()?;
        if self.sync_state == ClockSyncState::Unsynchronized {
            return Err(TimeGeoError::UnsynchronizedClock);
        }
        let aligned_utc_ms = device_clock_ms
            .checked_add(self.offset_ms)
            .ok_or(TimeGeoError::TimestampArithmeticOverflow)?;
        if !self.validity_window.contains(aligned_utc_ms) {
            return Err(TimeGeoError::ClockAlignmentOutOfWindow {
                aligned_ms: aligned_utc_ms,
            });
        }
        let elapsed_ms = aligned_utc_ms.abs_diff(self.validity_window.start_utc_ms());
        let drift_ppm_abs = u128::from(self.drift_ppm.unsigned_abs());
        let drift_penalty_ms = (u128::from(elapsed_ms) * drift_ppm_abs).div_ceil(1_000_000);
        let drift_u32 = u32::try_from(drift_penalty_ms).unwrap_or(u32::MAX);
        let effective_uncertainty_ms = self.uncertainty_ms.saturating_add(drift_u32);
        Ok(AlignedCaptureTime {
            aligned_utc_ms,
            effective_uncertainty_ms,
            sync_state: self.sync_state,
        })
    }
}
