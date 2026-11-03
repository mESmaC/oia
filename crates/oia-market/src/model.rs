//! Validated market values, immutable record envelopes, and cutoff semantics.
//!
//! **Provisional pending #12:** `CalendarDate`, `DateInterval`, `Unit`, and all
//! `*_utc_ms` fields are local kernel primitives until the shared time/geometry
//! contract lands. Exact instants are signed Unix-epoch UTC milliseconds with
//! millisecond resolution and no leap-second representation. Callers must truncate
//! finer tick timestamps and retain the original precision on the source version.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use oia_domain::{OpaqueId, VersionId};

use crate::{MarketError, MarketLimits, StopReason, validate_text, validate_timestamp};

/// **Provisional pending #12.** A civil calendar date, never an implicitly observed UTC midnight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CalendarDate {
    year: u16,
    month: u8,
    day: u8,
}

impl CalendarDate {
    pub fn new(year: u16, month: u8, day: u8) -> Result<Self, MarketError> {
        if !(1..=9_999).contains(&year) {
            return Err(MarketError::InvalidDate("year must be in 1..=9999"));
        }
        let leap =
            year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
        let days = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap => 29,
            2 => 28,
            _ => return Err(MarketError::InvalidDate("month must be in 1..=12")),
        };
        if day == 0 || day > days {
            return Err(MarketError::InvalidDate("day is outside the month"));
        }
        Ok(Self { year, month, day })
    }

    #[must_use]
    pub fn year(self) -> u16 {
        self.year
    }
    #[must_use]
    pub fn month(self) -> u8 {
        self.month
    }
    #[must_use]
    pub fn day(self) -> u8 {
        self.day
    }
}

impl fmt::Display for CalendarDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// **Provisional pending #12.** Half-open calendar-date interval `[start, end_exclusive)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateInterval {
    pub start: CalendarDate,
    pub end_exclusive: CalendarDate,
}

impl DateInterval {
    pub fn new(start: CalendarDate, end_exclusive: CalendarDate) -> Result<Self, MarketError> {
        if start >= end_exclusive {
            return Err(MarketError::InvalidInterval(
                "calendar start must precede exclusive end",
            ));
        }
        Ok(Self {
            start,
            end_exclusive,
        })
    }
}

/// **Provisional pending #12.** Explicit currency, asset, percentage, or other unit;
/// no implicit FX conversion.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Unit(String);

impl Unit {
    pub fn new(raw: impl Into<String>) -> Result<Self, MarketError> {
        let value = raw.into();
        validate_text("unit", &value)?;
        if value.len() > 32
            || !value
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '/' | '.' | '%'))
        {
            return Err(MarketError::InvalidText { field: "unit" });
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Unit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Exact decimal `coefficient * 10^-scale`; binary floating point is not used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedDecimal {
    coefficient: i128,
    scale: u32,
}

impl FixedDecimal {
    pub fn new(coefficient: i128, scale: u32) -> Result<Self, MarketError> {
        if scale > 18 {
            return Err(MarketError::InvalidDecimalScale(scale));
        }
        if coefficient == i128::MIN {
            return Err(MarketError::UnsupportedCoefficient);
        }
        Ok(Self { coefficient, scale })
    }

    #[must_use]
    pub fn coefficient(self) -> i128 {
        self.coefficient
    }
    #[must_use]
    pub fn scale(self) -> u32 {
        self.scale
    }
}

/// Checked exact rational used for adjustment output without undocumented rounding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExactRational {
    numerator: i128,
    denominator: i128,
}

impl ExactRational {
    pub fn new(numerator: i128, denominator: i128) -> Result<Self, MarketError> {
        if numerator == i128::MIN || denominator == i128::MIN {
            return Err(MarketError::UnsupportedCoefficient);
        }
        if denominator == 0 {
            return Err(MarketError::InvalidRatio);
        }
        if numerator == 0 {
            return Ok(Self {
                numerator: 0,
                denominator: 1,
            });
        }
        let (numerator, denominator) = if denominator < 0 {
            (
                numerator
                    .checked_neg()
                    .ok_or(MarketError::ArithmeticOverflow)?,
                denominator
                    .checked_neg()
                    .ok_or(MarketError::ArithmeticOverflow)?,
            )
        } else {
            (numerator, denominator)
        };
        let mut a = numerator.unsigned_abs();
        let mut b = denominator.unsigned_abs();
        while b != 0 {
            let remainder = a % b;
            a = b;
            b = remainder;
        }
        let divisor = i128::try_from(a).map_err(|_| MarketError::ArithmeticOverflow)?;
        Ok(Self {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        })
    }

    pub fn from_decimal(value: FixedDecimal) -> Result<Self, MarketError> {
        let denominator = 10_i128
            .checked_pow(value.scale)
            .ok_or(MarketError::ArithmeticOverflow)?;
        Self::new(value.coefficient, denominator)
    }

    /// Multiplies by `numerator / denominator` with checked intermediate arithmetic.
    pub fn checked_multiply(self, numerator: i128, denominator: i128) -> Result<Self, MarketError> {
        if numerator <= 0 || denominator <= 0 {
            return Err(MarketError::InvalidRatio);
        }
        let next_numerator = self
            .numerator
            .checked_mul(numerator)
            .ok_or(MarketError::ArithmeticOverflow)?;
        let next_denominator = self
            .denominator
            .checked_mul(denominator)
            .ok_or(MarketError::ArithmeticOverflow)?;
        Self::new(next_numerator, next_denominator)
    }

    #[must_use]
    pub fn numerator(self) -> i128 {
        self.numerator
    }
    #[must_use]
    pub fn denominator(self) -> i128 {
        self.denominator
    }
}

impl fmt::Display for ExactRational {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.numerator, self.denominator)
    }
}

/// Public availability and OIA knowledge are distinct exact instants, represented
/// with the module's provisional Unix-epoch UTC millisecond convention pending #12.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordAvailability {
    pub public_available_at_utc_ms: Option<i64>,
    pub ingested_at_utc_ms: Option<i64>,
}

pub(crate) fn validate_availability(availability: RecordAvailability) -> Result<(), MarketError> {
    if let Some(value) = availability.public_available_at_utc_ms {
        validate_timestamp("public_available_at_utc_ms", value)?;
    }
    if let Some(value) = availability.ingested_at_utc_ms {
        validate_timestamp("ingested_at_utc_ms", value)?;
    }
    Ok(())
}

/// Strict prospective selection or explicitly limited retrospective reconstruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CutoffMode {
    ProspectiveStrict,
    RetrospectiveReconstructed { limitation_note: String },
}

pub(crate) fn validate_cutoff_mode(mode: &CutoffMode) -> Result<(), MarketError> {
    if let CutoffMode::RetrospectiveReconstructed { limitation_note } = mode {
        validate_text("retrospective limitation_note", limitation_note)?;
    }
    Ok(())
}

/// Why a supplied immutable version was not used at a cutoff or adjustment boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExclusionReason {
    ObservationAfterCutoff,
    PublicAvailabilityUnknown,
    PublicAvailabilityAfterCutoff,
    IngestionUnknownStrict,
    IngestedAfterStrictCutoff,
    SupersededByAdmissibleRevision,
    EffectiveAfterCutoff,
    NotEffectiveForQuote,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExcludedVersion {
    pub version: VersionId,
    pub reason: ExclusionReason,
}

pub(crate) fn availability_exclusion(
    availability: RecordAvailability,
    cutoff_utc_ms: i64,
    mode: &CutoffMode,
) -> Option<ExclusionReason> {
    let Some(public) = availability.public_available_at_utc_ms else {
        return Some(ExclusionReason::PublicAvailabilityUnknown);
    };
    if public > cutoff_utc_ms {
        return Some(ExclusionReason::PublicAvailabilityAfterCutoff);
    }
    if matches!(mode, CutoffMode::ProspectiveStrict) {
        let Some(ingested) = availability.ingested_at_utc_ms else {
            return Some(ExclusionReason::IngestionUnknownStrict);
        };
        if ingested > cutoff_utc_ms {
            return Some(ExclusionReason::IngestedAfterStrictCutoff);
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstrumentClass {
    Equity,
    Index,
    Fund,
    BondOrYield,
    Commodity,
    Currency,
    DigitalAsset,
}

/// Mutable provider/venue symbol with a half-open civil-date validity interval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListingAlias {
    pub symbol: String,
    pub valid_from: CalendarDate,
    pub valid_to_exclusive: Option<CalendarDate>,
    pub source_version: VersionId,
}

/// Stable identity survives symbol renames; venue and unit remain explicit. This
/// slice models one venue per instrument; a listing transfer needs a distinct ID
/// and an explicit versioned relationship rather than mutation of this record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instrument {
    pub id: OpaqueId,
    pub version: VersionId,
    pub class: InstrumentClass,
    pub venue: OpaqueId,
    pub quote_unit: Unit,
    pub aliases: Vec<ListingAlias>,
}

fn validate_symbol(symbol: &str) -> Result<(), MarketError> {
    validate_text("listing symbol", symbol)?;
    if symbol.trim() != symbol || symbol.chars().any(char::is_control) {
        return Err(MarketError::InvalidInstrument(
            "listing symbols must not have surrounding whitespace or control characters",
        ));
    }
    Ok(())
}

/// Resolves a historical symbol without treating it as the instrument's identity.
/// The complete bounded catalog is validated before a match is returned.
pub fn resolve_instrument<'a>(
    instruments: &'a [Instrument],
    venue: &OpaqueId,
    symbol: &str,
    date: CalendarDate,
    limits: MarketLimits,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<&'a Instrument, MarketError> {
    check_control().map_err(MarketError::Stopped)?;
    limits.check_len(instruments.len())?;
    validate_symbol(symbol)?;
    let mut instrument_ids = BTreeSet::new();
    let mut versions = BTreeSet::new();
    let mut alias_groups: BTreeMap<(OpaqueId, String), Vec<ListingAlias>> = BTreeMap::new();
    let mut matched = None;
    let mut match_count = 0_usize;
    let mut alias_count = 0_usize;
    for instrument in instruments {
        check_control().map_err(MarketError::Stopped)?;
        if !instrument_ids.insert(&instrument.id) {
            return Err(MarketError::InvalidInstrument("duplicate instrument ID"));
        }
        if !versions.insert(&instrument.version) {
            return Err(MarketError::DuplicateVersion(instrument.version.clone()));
        }
        alias_count = alias_count
            .checked_add(instrument.aliases.len())
            .ok_or(MarketError::ArithmeticOverflow)?;
        limits.check_len(alias_count)?;
        for alias in &instrument.aliases {
            validate_symbol(&alias.symbol)?;
            if let Some(end) = alias.valid_to_exclusive
                && alias.valid_from >= end
            {
                return Err(MarketError::InvalidInstrument(
                    "alias start must precede its exclusive end",
                ));
            }
            alias_groups
                .entry((instrument.venue.clone(), alias.symbol.clone()))
                .or_default()
                .push(alias.clone());
            let active =
                alias.valid_from <= date && alias.valid_to_exclusive.is_none_or(|end| date < end);
            if active && &instrument.venue == venue && alias.symbol == symbol {
                matched = Some(instrument);
                match_count += 1;
            }
        }
    }
    for aliases in alias_groups.values_mut() {
        check_control().map_err(MarketError::Stopped)?;
        aliases.sort_by_key(|alias| (alias.valid_from, alias.valid_to_exclusive));
        let mut has_previous = false;
        let mut unbounded = false;
        let mut furthest_end = None;
        for alias in aliases.iter() {
            if has_previous && (unbounded || furthest_end.is_some_and(|end| alias.valid_from < end))
            {
                return Err(MarketError::AmbiguousAlias);
            }
            has_previous = true;
            match alias.valid_to_exclusive {
                None => {
                    unbounded = true;
                    furthest_end = None;
                }
                Some(end) if !unbounded => {
                    furthest_end = Some(furthest_end.map_or(end, |previous| previous.max(end)));
                }
                Some(_) => {}
            }
        }
    }
    check_control().map_err(MarketError::Stopped)?;
    match match_count {
        0 => Err(MarketError::AliasNotFound),
        1 => matched.ok_or(MarketError::AliasNotFound),
        _ => Err(MarketError::AmbiguousAlias),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Frequency {
    Tick,
    Secondly,
    Minutely,
    Hourly,
    Daily,
    Weekly,
    Monthly,
    Unknown,
}

/// Declared feed delay is metadata, never a substitute for availability evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuoteDelay {
    RealTime,
    Delayed { declared_delay_ms: Option<u64> },
    EndOfDay,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuoteRecord {
    pub instrument_id: OpaqueId,
    pub venue: OpaqueId,
    pub source_id: OpaqueId,
    pub version: VersionId,
    pub source_version: VersionId,
    pub observed_at_utc_ms: i64,
    pub availability: RecordAvailability,
    pub price: FixedDecimal,
    pub unit: Unit,
    pub frequency: Frequency,
    pub delay: QuoteDelay,
}

/// Split-only corporate-action revision. `action_id` is stable across corrections;
/// other action kinds are outside this kernel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorporateActionRecord {
    pub instrument_id: OpaqueId,
    pub action_id: OpaqueId,
    pub version: VersionId,
    pub source_version: VersionId,
    pub revision: u32,
    pub predecessor_version: Option<VersionId>,
    pub effective_at_utc_ms: i64,
    pub availability: RecordAvailability,
    pub split_numerator: i128,
    pub split_denominator: i128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceAdjustmentBasis {
    Unadjusted,
    SplitAdjusted,
    /// Explicitly unsupported; never silently replaced by split-only adjustment.
    TotalReturn,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleasePayload {
    Numeric {
        value: FixedDecimal,
        unit: Unit,
    },
    Disclosure {
        document_version: VersionId,
        linked_entities: Vec<OpaqueId>,
    },
}

/// Immutable economic-release revision or linked disclosure/news vintage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseVintage {
    pub series_id: OpaqueId,
    pub source_id: OpaqueId,
    pub version: VersionId,
    pub source_version: VersionId,
    pub revision: u32,
    pub predecessor_version: Option<VersionId>,
    pub reporting_period: DateInterval,
    pub event_at_utc_ms: Option<i64>,
    pub availability: RecordAvailability,
    pub payload: ReleasePayload,
}
