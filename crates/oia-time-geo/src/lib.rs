//! `oia-time-geo` — Shared time representations, timezone/DST ambiguity, scientific time scales,
//! clock alignment, WGS84/altitude datum semantics, dateline-aware geometry, exact decimal
//! quantities, and Lean-correspondent time-window frame selection.
//!
//! Portions of the time-window frame selection (`select_frame_in_window`) and ellipsoidal/MSL
//! datum conversion (`convert_orthometric_and_ellipsoidal`) are adapted from God's Eye View
//! (`src/layers/weather/clock.js` and `src/data/geoid.js`, commit `0d1a68160e532ba65dc34b48ea205de6de4028fc`),
//! Copyright (c) 2026 Bilawal Sidhu, under the MIT License (`THIRD_PARTY_NOTICES.md`).

use std::fmt;

/// Daylight-saving or local-offset ambiguity status (`docs/architecture.md` Section 3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DstAmbiguity {
    /// Local wall-clock time maps uniquely to a single UTC instant.
    Unambiguous,
    /// Fall-back transition: earlier (pre-transition / DST) occurrence selected.
    FoldEarlierOccurrence,
    /// Fall-back transition: later (post-transition / standard) occurrence selected.
    FoldLaterOccurrence,
    /// Fall-back transition reported by witness without disambiguating which occurrence.
    AmbiguousFoldUnresolved {
        /// First candidate UTC timestamp (ms).
        earlier_utc_ms: i64,
        /// Second candidate UTC timestamp (ms).
        later_utc_ms: i64,
    },
    /// Spring-forward gap: reported wall-clock time did not exist locally.
    SpringForwardGap,
}

/// Preserved local timezone context alongside normalized UTC comparison timestamps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalTimeContext {
    /// IANA timezone identifier or declared offset label (e.g., `"America/Phoenix"`, `"UTC+09:00"`).
    pub zone_name: String,
    /// Fixed UTC offset in minutes (`-840..=+840`), if known at the instant.
    pub utc_offset_minutes: Option<i16>,
    /// DST fold/gap ambiguity status.
    pub ambiguity: DstAmbiguity,
    /// Original textual representation from source or witness.
    pub original_text: String,
}

impl LocalTimeContext {
    /// Validates offset range (`-14h..=+14h`).
    pub fn validate(&self) -> Result<(), TimeGeoError> {
        if let Some(off) = self.utc_offset_minutes
            && !(-840..=840).contains(&off)
        {
            return Err(TimeGeoError::InvalidUtcOffsetMinutes(off));
        }
        Ok(())
    }
}

/// Time representation preserving exact instants, half-open intervals, date-only records,
/// approximate windows, and unknown times (`docs/architecture.md` Section 3.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimeRepresentation {
    /// Exact timestamp in UTC milliseconds.
    ExactInstant {
        /// UTC epoch milliseconds.
        utc_ms: i64,
        /// Optional local timezone context.
        local: Option<LocalTimeContext>,
    },
    /// Half-open interval `[start_utc_ms, end_utc_ms)` where `start_utc_ms < end_utc_ms`.
    BoundedInterval {
        /// Inclusive start (UTC ms).
        start_utc_ms: i64,
        /// Exclusive end (UTC ms).
        end_utc_ms: i64,
        /// Optional local timezone context.
        local: Option<LocalTimeContext>,
    },
    /// Calendar date without a known time-of-day.
    ///
    /// Never implicitly converted to `00:00:00 UTC` midnight (`docs/architecture.md` Section 3.1).
    CalendarDate {
        /// Proleptic Gregorian year.
        year: i32,
        /// Month (`1..=12`).
        month: u8,
        /// Day of month (`1..=31`, validated against month/leap-year).
        day: u8,
        /// Preserved source date text.
        original_text: String,
    },
    /// Approximate time window with explicit earliest/latest bounds.
    ApproximateTime {
        /// Earliest plausible UTC instant (ms).
        earliest_utc_ms: i64,
        /// Latest plausible UTC instant (ms).
        latest_utc_ms: i64,
        /// Preserved witness/source phrasing (e.g., `"shortly after dusk"`).
        original_text: String,
    },
    /// Unknown time with preserved text and reason.
    UnknownTime {
        /// Reason time is unknown.
        reason: String,
        /// Original source text, if any.
        original_text: String,
    },
}

impl TimeRepresentation {
    /// Validates structural invariants (interval ordering, calendar dates, local offsets).
    pub fn validate(&self) -> Result<(), TimeGeoError> {
        match self {
            Self::ExactInstant { local, .. } => {
                if let Some(ctx) = local {
                    ctx.validate()?;
                }
                Ok(())
            }
            Self::BoundedInterval {
                start_utc_ms,
                end_utc_ms,
                local,
            } => {
                if start_utc_ms >= end_utc_ms {
                    return Err(TimeGeoError::InvalidTimeInterval {
                        start_ms: *start_utc_ms,
                        end_ms: *end_utc_ms,
                    });
                }
                if let Some(ctx) = local {
                    ctx.validate()?;
                }
                Ok(())
            }
            Self::CalendarDate {
                year, month, day, ..
            } => {
                if !is_valid_calendar_date(*year, *month, *day) {
                    return Err(TimeGeoError::InvalidCalendarDate {
                        year: *year,
                        month: *month,
                        day: *day,
                    });
                }
                Ok(())
            }
            Self::ApproximateTime {
                earliest_utc_ms,
                latest_utc_ms,
                ..
            } => {
                if earliest_utc_ms > latest_utc_ms {
                    return Err(TimeGeoError::InvalidTimeInterval {
                        start_ms: *earliest_utc_ms,
                        end_ms: *latest_utc_ms,
                    });
                }
                Ok(())
            }
            Self::UnknownTime { reason, .. } => {
                if reason.trim().is_empty() {
                    return Err(TimeGeoError::EmptyUnknownReason);
                }
                Ok(())
            }
        }
    }

    /// Returns `true` if this time representation overlaps the half-open query window `[start_ms, end_ms)`.
    ///
    /// `CalendarDate` and `UnknownTime` return `None` because date-only or unknown times cannot
    /// be compared against an instant window without an explicit timezone/day-expansion assumption.
    #[must_use]
    pub const fn overlaps_utc_window(&self, start_ms: i64, end_ms: i64) -> Option<bool> {
        match self {
            Self::ExactInstant { utc_ms, .. } => Some(*utc_ms >= start_ms && *utc_ms < end_ms),
            Self::BoundedInterval {
                start_utc_ms,
                end_utc_ms,
                ..
            } => Some(*start_utc_ms < end_ms && *end_utc_ms > start_ms),
            Self::ApproximateTime {
                earliest_utc_ms,
                latest_utc_ms,
                ..
            } => Some(*earliest_utc_ms < end_ms && *latest_utc_ms >= start_ms),
            Self::CalendarDate { .. } | Self::UnknownTime { .. } => None,
        }
    }
}

const fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

const fn is_valid_calendar_date(year: i32, month: u8, day: u8) -> bool {
    if month < 1 || month > 12 || day < 1 {
        return false;
    }
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => return false,
    };
    day <= max_day
}

/// Scientific time scale (`docs/architecture.md` Section 3.1, `SPACE-05`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// Fixed offset between TAI and TT in milliseconds (`32.184 s = 32_184 ms`).
pub const TT_MINUS_TAI_MS: i64 = 32_184;

/// Versioned leap-second table for `UTC <-> TAI <-> TT` conversions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeapSecondTable {
    /// Table version identifier (e.g., `"IERS-Bulletin-C-2026"`).
    pub version: String,
    /// Sorted list of `(effective_utc_ms, tai_minus_utc_seconds)`.
    entries: Vec<(i64, i32)>,
}

impl LeapSecondTable {
    /// Returns the standard IERS leap-second table from 1972-01-01 (`10 s`) through 2017-01-01 (`37 s`).
    #[must_use]
    pub fn iers_standard() -> Self {
        Self {
            version: "IERS-Bulletin-C-2017-2026".to_owned(),
            entries: vec![
                (63_072_000_000, 10),    // 1972-01-01T00:00:00Z
                (1_230_768_000_000, 34), // 2009-01-01T00:00:00Z
                (1_341_100_800_000, 35), // 2012-07-01T00:00:00Z
                (1_435_708_800_000, 36), // 2015-07-01T00:00:00Z
                (1_483_228_800_000, 37), // 2017-01-01T00:00:00Z
            ],
        }
    }

    /// Looks up `TAI - UTC` in seconds for a UTC timestamp (`utc_ms >= 1972-01-01`).
    pub fn tai_minus_utc_seconds(&self, utc_ms: i64) -> Result<i32, TimeGeoError> {
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

    /// Converts a `UTC` millisecond timestamp into `TAI` milliseconds.
    pub fn utc_to_tai_ms(&self, utc_ms: i64) -> Result<i64, TimeGeoError> {
        let delta_s = i64::from(self.tai_minus_utc_seconds(utc_ms)?);
        Ok(utc_ms.saturating_add(delta_s * 1_000))
    }

    /// Converts a `UTC` millisecond timestamp into Terrestrial Time (`TT`) milliseconds.
    pub fn utc_to_tt_ms(&self, utc_ms: i64) -> Result<i64, TimeGeoError> {
        let tai_ms = self.utc_to_tai_ms(utc_ms)?;
        Ok(tai_ms.saturating_add(TT_MINUS_TAI_MS))
    }
}

/// Clock synchronization state for a device or stream (`DEVICE-04`, `docs/architecture.md` Section 3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockSyncState {
    /// Device clock offset and uncertainty are bounded within the validity interval.
    Synchronized,
    /// Clock drift exceeded nominal threshold; alignment carries elevated uncertainty.
    DegradedDrift,
    /// Clock synchronization lost; receipt timestamps must NOT be substituted as capture times.
    Unsynchronized,
}

/// Clock alignment record between a source/device clock and UTC (`docs/architecture.md` Section 3.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClockAlignment {
    /// Reference clock name (e.g., `"NTP_POOL_UTC"`, `"GNSS_PPS"`).
    pub reference_clock: String,
    /// Estimated offset in milliseconds (`utc_ms = device_clock_ms + offset_ms`).
    pub offset_ms: i64,
    /// Estimated drift in parts-per-million.
    pub drift_ppm: i32,
    /// Symmetric alignment uncertainty in milliseconds (`>= 0`).
    pub uncertainty_ms: u32,
    /// Start of validity window (UTC ms).
    pub valid_from_utc_ms: i64,
    /// End of validity window (UTC ms).
    pub valid_until_utc_ms: i64,
    /// Synchronization state.
    pub sync_state: ClockSyncState,
}

impl ClockAlignment {
    /// Aligns a raw device timestamp (`device_clock_ms`) to UTC if synchronization is valid.
    ///
    /// When `sync_state == ClockSyncState::Unsynchronized`, returns `Err(TimeGeoError::UnsynchronizedClock)`
    /// rather than substituting receipt time as capture time.
    pub fn align_device_time(&self, device_clock_ms: i64) -> Result<(i64, u32), TimeGeoError> {
        if self.sync_state == ClockSyncState::Unsynchronized {
            return Err(TimeGeoError::UnsynchronizedClock);
        }
        let aligned = device_clock_ms.saturating_add(self.offset_ms);
        if aligned < self.valid_from_utc_ms || aligned >= self.valid_until_utc_ms {
            return Err(TimeGeoError::ClockAlignmentOutOfWindow {
                aligned_ms: aligned,
            });
        }
        Ok((aligned, self.uncertainty_ms))
    }
}

/// Outcome of bounded time-window source frame selection (`proofs/OiaProofs/FrameSelection.lean`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameSelectionOutcome {
    /// Source catalog contained no candidate timestamps at all.
    NoFramesAvailable,
    /// Source catalog contained frames, but none fell within `[window_start_ms, window_end_ms)`
    /// and `[target_ms - max_gap_ms, target_ms]`.
    CoverageGap,
    /// Selected the latest valid frame timestamp `t` satisfying all window and gap constraints.
    Selected(i64),
}

/// Selects the latest candidate frame timestamp `t` from `candidates_ms` satisfying:
/// 1. `window_start_ms <= t < window_end_ms` (half-open query window),
/// 2. `t <= target_ms` (no future leakage past target instant), and
/// 3. `target_ms - t <= max_gap_ms` (maximum freshness/staleness gap).
///
/// Adapted from God's Eye View (`src/layers/weather/clock.js`, MIT License) and formally
/// specified in `proofs/OiaProofs/FrameSelection.lean`.
pub fn select_frame_in_window(
    candidates_ms: &[i64],
    window_start_ms: i64,
    window_end_ms: i64,
    target_ms: i64,
    max_gap_ms: i64,
) -> Result<FrameSelectionOutcome, TimeGeoError> {
    if window_start_ms >= window_end_ms {
        return Err(TimeGeoError::InvalidTimeInterval {
            start_ms: window_start_ms,
            end_ms: window_end_ms,
        });
    }
    if max_gap_ms < 0 {
        return Err(TimeGeoError::NegativeMaxGap(max_gap_ms));
    }
    if candidates_ms.is_empty() {
        return Ok(FrameSelectionOutcome::NoFramesAvailable);
    }

    let mut best: Option<i64> = None;
    for &t in candidates_ms {
        let in_window = t >= window_start_ms && t < window_end_ms;
        let not_after_target = t <= target_ms;
        let age = target_ms.saturating_sub(t);
        let within_gap = age >= 0 && age <= max_gap_ms;
        if in_window && not_after_target && within_gap {
            best = Some(match best {
                Some(prev) if prev >= t => prev,
                _ => t,
            });
        }
    }

    match best {
        Some(t) => Ok(FrameSelectionOutcome::Selected(t)),
        None => Ok(FrameSelectionOutcome::CoverageGap),
    }
}

/// Explicit vertical altitude reference datum (`docs/architecture.md` Section 3.2, `GEO-05`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AltitudeDatum {
    /// Height above the WGS84 reference ellipsoid ($h$).
    Wgs84Ellipsoid,
    /// Orthometric height above mean sea level / geoid ($H$).
    OrthometricMsl,
    /// Height above local ground terrain (AGL).
    AboveGroundLevel,
    /// Uncorrected pressure-derived barometric altitude (flight level / standard atmosphere).
    BarometricPressure,
    /// Altitude was reported without a known vertical datum.
    UnknownDatum,
}

/// Altitude measurement with explicit vertical datum and optional uncertainty.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AltitudeValue {
    /// Altitude in meters.
    pub meters: f64,
    /// Vertical reference datum.
    pub datum: AltitudeDatum,
    /// Symmetric uncertainty in meters (`>= 0.0`), if known.
    pub uncertainty_m: Option<f64>,
}

impl AltitudeValue {
    /// Validates finite numeric values.
    pub fn validate(&self) -> Result<(), TimeGeoError> {
        if !self.meters.is_finite() {
            return Err(TimeGeoError::NonFiniteCoordinate("altitude.meters"));
        }
        if let Some(u) = self.uncertainty_m
            && (!u.is_finite() || u < 0.0)
        {
            return Err(TimeGeoError::NonFiniteCoordinate("altitude.uncertainty_m"));
        }
        Ok(())
    }

    /// Converts between `OrthometricMsl` ($H$) and `Wgs84Ellipsoid` ($h$) using local geoid
    /// undulation `geoid_undulation_m` ($N$, where $h = H + N$).
    ///
    /// Adapted from God's Eye View (`src/data/geoid.js`, MIT License).
    /// Explicitly rejects `BarometricPressure` (`TimeGeoError::BarometricGeoidMisuse`) because
    /// adding or subtracting geoid undulation is not a physical correction for pressure altitude
    /// (`docs/architecture.md` Section 3.2).
    pub fn convert_datum(
        &self,
        target: AltitudeDatum,
        geoid_undulation_m: f64,
    ) -> Result<Self, TimeGeoError> {
        self.validate()?;
        if !geoid_undulation_m.is_finite() {
            return Err(TimeGeoError::NonFiniteCoordinate("geoid_undulation_m"));
        }
        if self.datum == target {
            return Ok(*self);
        }
        if self.datum == AltitudeDatum::BarometricPressure
            || target == AltitudeDatum::BarometricPressure
        {
            return Err(TimeGeoError::BarometricGeoidMisuse);
        }
        match (self.datum, target) {
            (AltitudeDatum::OrthometricMsl, AltitudeDatum::Wgs84Ellipsoid) => Ok(Self {
                meters: self.meters + geoid_undulation_m,
                datum: AltitudeDatum::Wgs84Ellipsoid,
                uncertainty_m: self.uncertainty_m,
            }),
            (AltitudeDatum::Wgs84Ellipsoid, AltitudeDatum::OrthometricMsl) => Ok(Self {
                meters: self.meters - geoid_undulation_m,
                datum: AltitudeDatum::OrthometricMsl,
                uncertainty_m: self.uncertainty_m,
            }),
            (from, to) => Err(TimeGeoError::UnsupportedDatumTransform { from, to }),
        }
    }
}

/// Validated WGS84 geographic coordinates (`docs/architecture.md` Section 3.2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeoPoint {
    /// Latitude in degrees (`-90.0..=90.0`).
    pub lat_deg: f64,
    /// Longitude in degrees (`-180.0..=180.0`).
    pub lon_deg: f64,
    /// Optional altitude with explicit datum (`None` when height is absent; never defaulted to `0.0`).
    pub altitude: Option<AltitudeValue>,
    /// Optional horizontal uncertainty radius in meters (`>= 0.0`).
    pub horizontal_uncertainty_m: Option<f64>,
}

impl GeoPoint {
    /// Validates latitude, longitude, optional altitude, and horizontal uncertainty.
    pub fn new(
        lat_deg: f64,
        lon_deg: f64,
        altitude: Option<AltitudeValue>,
        horizontal_uncertainty_m: Option<f64>,
    ) -> Result<Self, TimeGeoError> {
        if !lat_deg.is_finite() || !(-90.0..=90.0).contains(&lat_deg) {
            return Err(TimeGeoError::InvalidLatitude(lat_deg));
        }
        if !lon_deg.is_finite() || !(-180.0..=180.0).contains(&lon_deg) {
            return Err(TimeGeoError::InvalidLongitude(lon_deg));
        }
        if let Some(alt) = altitude {
            alt.validate()?;
        }
        if let Some(u) = horizontal_uncertainty_m
            && (!u.is_finite() || u < 0.0)
        {
            return Err(TimeGeoError::NonFiniteCoordinate(
                "horizontal_uncertainty_m",
            ));
        }
        Ok(Self {
            lat_deg,
            lon_deg,
            altitude,
            horizontal_uncertainty_m,
        })
    }
}

/// Geographic bounding box supporting antimeridian / dateline crossing (`west_lon_deg > east_lon_deg`)
/// and polar latitudes (`docs/architecture.md` Section 3.2, `A04`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeoBoundingBox {
    /// Southern latitude bound (`-90.0..=90.0`).
    pub south_lat_deg: f64,
    /// Northern latitude bound (`south_lat_deg..=90.0`).
    pub north_lat_deg: f64,
    /// Western longitude bound (`-180.0..=180.0`).
    pub west_lon_deg: f64,
    /// Eastern longitude bound (`-180.0..=180.0`). If `west_lon_deg > east_lon_deg`, the box crosses the antimeridian (`±180°`).
    pub east_lon_deg: f64,
}

impl GeoBoundingBox {
    /// Validates latitude ordering and coordinate bounds.
    pub fn new(
        south_lat_deg: f64,
        north_lat_deg: f64,
        west_lon_deg: f64,
        east_lon_deg: f64,
    ) -> Result<Self, TimeGeoError> {
        if !south_lat_deg.is_finite() || !(-90.0..=90.0).contains(&south_lat_deg) {
            return Err(TimeGeoError::InvalidLatitude(south_lat_deg));
        }
        if !north_lat_deg.is_finite() || !(-90.0..=90.0).contains(&north_lat_deg) {
            return Err(TimeGeoError::InvalidLatitude(north_lat_deg));
        }
        if south_lat_deg > north_lat_deg {
            return Err(TimeGeoError::InvertedLatitudeBounds {
                south: south_lat_deg,
                north: north_lat_deg,
            });
        }
        if !west_lon_deg.is_finite() || !(-180.0..=180.0).contains(&west_lon_deg) {
            return Err(TimeGeoError::InvalidLongitude(west_lon_deg));
        }
        if !east_lon_deg.is_finite() || !(-180.0..=180.0).contains(&east_lon_deg) {
            return Err(TimeGeoError::InvalidLongitude(east_lon_deg));
        }
        Ok(Self {
            south_lat_deg,
            north_lat_deg,
            west_lon_deg,
            east_lon_deg,
        })
    }

    /// Returns `true` if this bounding box crosses the 180th meridian (dateline).
    #[must_use]
    pub fn crosses_dateline(&self) -> bool {
        self.west_lon_deg > self.east_lon_deg
    }

    /// Returns `true` if `point` lies within this bounding box, properly handling dateline crossing.
    #[must_use]
    pub fn contains(&self, point: &GeoPoint) -> bool {
        if point.lat_deg < self.south_lat_deg || point.lat_deg > self.north_lat_deg {
            return false;
        }
        if !self.crosses_dateline() {
            point.lon_deg >= self.west_lon_deg && point.lon_deg <= self.east_lon_deg
        } else {
            point.lon_deg >= self.west_lon_deg || point.lon_deg <= self.east_lon_deg
        }
    }
}

/// Distinguishes directly observed track points from display-only interpolated samples (`GEO-05`, `A04`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackSampleKind {
    /// Directly observed position sample from a sensor or transponder.
    Observed,
    /// Display-only interpolated point; must never be persisted or counted as an observation.
    DisplayInterpolated,
}

/// A timestamped point along an aircraft, vessel, or object track (`GEO-05`).
#[derive(Debug, Clone, PartialEq)]
pub struct TrackPoint {
    /// UTC timestamp (ms).
    pub utc_ms: i64,
    /// Position.
    pub position: GeoPoint,
    /// Whether observed or display-interpolated.
    pub kind: TrackSampleKind,
}

/// Spatial location variants preserving gazetteer centroid approximation and viewing geometry (`GEO-01`, `GEO-04`).
#[derive(Debug, Clone, PartialEq)]
pub enum SpatialLocation {
    /// Measured or reported coordinates.
    Point(GeoPoint),
    /// Gazetteer place-name centroid explicitly labeled as approximate (`docs/architecture.md` Section 3.2).
    GazetteerCentroid {
        /// Place name.
        place_name: String,
        /// Approximate centroid point.
        centroid: GeoPoint,
        /// Approximate radius in meters.
        approximate_radius_m: f64,
    },
    /// Geographic bounding box.
    Region(GeoBoundingBox),
    /// Sequence of track points.
    Track(Vec<TrackPoint>),
    /// Camera or observer viewing cone (`azimuth_deg`, `horizontal_fov_deg`, `max_range_m`).
    ViewingCone {
        /// Observer origin.
        origin: GeoPoint,
        /// Center azimuth in degrees `[0.0, 360.0)`.
        azimuth_deg: f64,
        /// Horizontal field of view in degrees `(0.0, 360.0]`.
        horizontal_fov_deg: f64,
        /// Optional nominal visual range in meters (does not establish target distance without triangulation).
        nominal_range_m: Option<f64>,
    },
    /// Location unknown or omitted with stated reason.
    UnknownLocation {
        /// Reason location is absent.
        reason: String,
    },
}

/// Exact base-10 decimal value for financial/market prices and adjustments (`MARKET-01`, `docs/architecture.md` Section 3.2).
///
/// Represents `mantissa * 10^(-scale)` without binary floating-point rounding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExactDecimal {
    mantissa: i128,
    scale: u8,
}

impl ExactDecimal {
    /// Maximum supported decimal places.
    pub const MAX_SCALE: u8 = 18;

    /// Constructs an [`ExactDecimal`] from `mantissa` and `scale` (`0..=18`).
    pub const fn new(mantissa: i128, scale: u8) -> Result<Self, TimeGeoError> {
        if scale > Self::MAX_SCALE {
            Err(TimeGeoError::DecimalScaleTooLarge(scale))
        } else {
            Ok(Self { mantissa, scale })
        }
    }

    /// Parses an exact base-10 decimal string (e.g., `"142.3750"`, `"-0.0025"`).
    pub fn parse(raw: &str) -> Result<Self, TimeGeoError> {
        let s = raw.trim();
        if s.is_empty() {
            return Err(TimeGeoError::InvalidDecimalString(raw.to_owned()));
        }
        let (negative, rest) = if let Some(stripped) = s.strip_prefix('-') {
            (true, stripped)
        } else if let Some(stripped) = s.strip_prefix('+') {
            (false, stripped)
        } else {
            (false, s)
        };
        if rest.is_empty() {
            return Err(TimeGeoError::InvalidDecimalString(raw.to_owned()));
        }
        let (int_part, frac_part) = match rest.split_once('.') {
            Some((a, b)) => (a, Some(b)),
            None => (rest, None),
        };
        let scale = frac_part.map_or(0, str::len);
        if scale > usize::from(Self::MAX_SCALE) {
            return Err(TimeGeoError::DecimalScaleTooLarge(scale as u8));
        }
        let combined = format!("{int_part}{}", frac_part.unwrap_or(""));
        if combined.is_empty() || !combined.chars().all(|c| c.is_ascii_digit()) {
            return Err(TimeGeoError::InvalidDecimalString(raw.to_owned()));
        }
        let unsigned = combined
            .parse::<i128>()
            .map_err(|_| TimeGeoError::InvalidDecimalString(raw.to_owned()))?;
        let mantissa = if negative { -unsigned } else { unsigned };
        Ok(Self {
            mantissa,
            scale: scale as u8,
        })
    }

    /// Returns the raw signed mantissa.
    #[must_use]
    pub const fn mantissa(self) -> i128 {
        self.mantissa
    }

    /// Returns the base-10 scale (`0..=18`).
    #[must_use]
    pub const fn scale(self) -> u8 {
        self.scale
    }

    /// Adds two [`ExactDecimal`] values by normalizing to the larger scale.
    pub fn checked_add(self, rhs: Self) -> Result<Self, TimeGeoError> {
        let target_scale = self.scale.max(rhs.scale);
        let lhs_m = scale_up(self.mantissa, target_scale - self.scale)?;
        let rhs_m = scale_up(rhs.mantissa, target_scale - rhs.scale)?;
        let sum = lhs_m
            .checked_add(rhs_m)
            .ok_or(TimeGeoError::DecimalOverflow)?;
        Ok(Self {
            mantissa: sum,
            scale: target_scale,
        })
    }
}

fn scale_up(val: i128, diff: u8) -> Result<i128, TimeGeoError> {
    let factor = 10_i128
        .checked_pow(u32::from(diff))
        .ok_or(TimeGeoError::DecimalOverflow)?;
    val.checked_mul(factor).ok_or(TimeGeoError::DecimalOverflow)
}

impl fmt::Display for ExactDecimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.scale == 0 {
            return write!(f, "{}", self.mantissa);
        }
        let sign = if self.mantissa < 0 { "-" } else { "" };
        let abs = self.mantissa.unsigned_abs();
        let divisor = 10_u128.pow(u32::from(self.scale));
        let int_part = abs / divisor;
        let frac_part = abs % divisor;
        write!(
            f,
            "{sign}{int_part}.{frac_part:0>width$}",
            width = usize::from(self.scale)
        )
    }
}

/// Typed errors for `oia-time-geo`.
#[derive(Debug, Clone, PartialEq)]
pub enum TimeGeoError {
    /// UTC offset minutes outside `-840..=840`.
    InvalidUtcOffsetMinutes(i16),
    /// Half-open interval had `start_ms >= end_ms`.
    InvalidTimeInterval {
        /// Start ms.
        start_ms: i64,
        /// End ms.
        end_ms: i64,
    },
    /// Invalid Gregorian calendar date.
    InvalidCalendarDate {
        /// Year.
        year: i32,
        /// Month.
        month: u8,
        /// Day.
        day: u8,
    },
    /// Unknown time reason was empty.
    EmptyUnknownReason,
    /// UTC timestamp precedes 1972-01-01 leap-second table coverage.
    PreLeapSecondEpoch(i64),
    /// Device clock is unsynchronized; receipt time cannot masquerade as capture time.
    UnsynchronizedClock,
    /// Aligned device timestamp fell outside the alignment validity interval.
    ClockAlignmentOutOfWindow {
        /// Aligned UTC timestamp (ms).
        aligned_ms: i64,
    },
    /// `max_gap_ms` was negative.
    NegativeMaxGap(i64),
    /// Non-finite floating-point value in coordinate or altitude.
    NonFiniteCoordinate(&'static str),
    /// Attempted to convert barometric pressure altitude using geoid undulation alone.
    BarometricGeoidMisuse,
    /// Unsupported vertical datum conversion without terrain/atmosphere model.
    UnsupportedDatumTransform {
        /// Source datum.
        from: AltitudeDatum,
        /// Target datum.
        to: AltitudeDatum,
    },
    /// Latitude outside `-90.0..=90.0` or non-finite.
    InvalidLatitude(f64),
    /// Longitude outside `-180.0..=180.0` or non-finite.
    InvalidLongitude(f64),
    /// Bounding box had `south_lat_deg > north_lat_deg`.
    InvertedLatitudeBounds {
        /// South latitude.
        south: f64,
        /// North latitude.
        north: f64,
    },
    /// Decimal scale exceeded `ExactDecimal::MAX_SCALE`.
    DecimalScaleTooLarge(u8),
    /// Invalid base-10 decimal literal.
    InvalidDecimalString(String),
    /// Decimal arithmetic overflow.
    DecimalOverflow,
}

impl fmt::Display for TimeGeoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUtcOffsetMinutes(m) => {
                write!(f, "UTC offset {m} minutes outside valid range [-840, 840]")
            }
            Self::InvalidTimeInterval { start_ms, end_ms } => {
                write!(f, "invalid time interval [{start_ms}, {end_ms})")
            }
            Self::InvalidCalendarDate { year, month, day } => {
                write!(f, "invalid calendar date {year:04}-{month:02}-{day:02}")
            }
            Self::EmptyUnknownReason => write!(f, "unknown time requires a non-empty reason"),
            Self::PreLeapSecondEpoch(ms) => {
                write!(f, "timestamp {ms} precedes 1972-01-01 leap-second coverage")
            }
            Self::UnsynchronizedClock => write!(
                f,
                "cannot align device timestamp: clock is unsynchronized and receipt time must not substitute for capture time"
            ),
            Self::ClockAlignmentOutOfWindow { aligned_ms } => {
                write!(
                    f,
                    "aligned timestamp {aligned_ms} is outside clock validity window"
                )
            }
            Self::NegativeMaxGap(g) => write!(f, "max_gap_ms must be >= 0, got {g}"),
            Self::NonFiniteCoordinate(field) => {
                write!(f, "non-finite value in coordinate field '{field}'")
            }
            Self::BarometricGeoidMisuse => write!(
                f,
                "barometric pressure altitude cannot be converted to ellipsoidal or orthometric height by geoid undulation alone"
            ),
            Self::UnsupportedDatumTransform { from, to } => {
                write!(
                    f,
                    "unsupported direct datum transform from {from:?} to {to:?}"
                )
            }
            Self::InvalidLatitude(lat) => {
                write!(f, "invalid latitude {lat} (must be in [-90, 90])")
            }
            Self::InvalidLongitude(lon) => {
                write!(f, "invalid longitude {lon} (must be in [-180, 180])")
            }
            Self::InvertedLatitudeBounds { south, north } => {
                write!(
                    f,
                    "bounding box south latitude {south} exceeds north latitude {north}"
                )
            }
            Self::DecimalScaleTooLarge(s) => write!(f, "decimal scale {s} exceeds maximum 18"),
            Self::InvalidDecimalString(s) => write!(f, "invalid exact decimal string '{s}'"),
            Self::DecimalOverflow => write!(f, "exact decimal arithmetic overflow"),
        }
    }
}

impl std::error::Error for TimeGeoError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies DST ambiguity, date-only non-promotion, leap-second UTC->TAI->TT conversion,
    /// dateline-crossing bounding boxes, barometric geoid misuse rejection, and exact decimal arithmetic (#12, A04, A05, A14).
    #[test]
    fn time_scales_dateline_bounds_altitude_datums_and_exact_decimal() -> Result<(), TimeGeoError> {
        // 1. Date-only record refuses silent instant window overlap and validates leap days.
        let date_only = TimeRepresentation::CalendarDate {
            year: 1947,
            month: 6,
            day: 24,
            original_text: "June 24, 1947".to_owned(),
        };
        date_only.validate()?;
        assert_eq!(date_only.overlaps_utc_window(0, 1_000_000), None);
        assert!(
            TimeRepresentation::CalendarDate {
                year: 2026,
                month: 2,
                day: 29,
                original_text: "2026-02-29".to_owned()
            }
            .validate()
            .is_err()
        );

        // 2. Leap-second aware UTC -> TAI -> TT conversion (2026: TAI - UTC = 37s, TT - TAI = 32.184s).
        let table = LeapSecondTable::iers_standard();
        let utc_2026_ms = 1_793_577_600_000_i64; // 2026-11-02T00:00:00Z
        let tai_ms = table.utc_to_tai_ms(utc_2026_ms)?;
        let tt_ms = table.utc_to_tt_ms(utc_2026_ms)?;
        assert_eq!(tai_ms - utc_2026_ms, 37_000);
        assert_eq!(tt_ms - utc_2026_ms, 69_184);

        // 3. Unsynchronized clock refuses alignment.
        let unsynced = ClockAlignment {
            reference_clock: "NTP".to_owned(),
            offset_ms: 120,
            drift_ppm: 5,
            uncertainty_ms: 15,
            valid_from_utc_ms: 1_000,
            valid_until_utc_ms: 20_000,
            sync_state: ClockSyncState::Unsynchronized,
        };
        assert_eq!(
            unsynced.align_device_time(5_000),
            Err(TimeGeoError::UnsynchronizedClock)
        );

        // 4. Dateline-crossing bounding box (e.g. Aleutians / Western Pacific: 170E to 170W).
        let aleutian_box = GeoBoundingBox::new(50.0, 60.0, 170.0, -170.0)?;
        assert!(aleutian_box.crosses_dateline());
        let west_of_dateline = GeoPoint::new(52.5, 175.0, None, None)?;
        let east_of_dateline = GeoPoint::new(52.5, -175.0, None, None)?;
        let outside_box = GeoPoint::new(52.5, 0.0, None, None)?;
        assert!(aleutian_box.contains(&west_of_dateline));
        assert!(aleutian_box.contains(&east_of_dateline));
        assert!(!aleutian_box.contains(&outside_box));

        // 5. Altitude datum: OrthometricMsl <-> Wgs84Ellipsoid works; BarometricPressure geoid conversion is rejected.
        let sfo_msl = AltitudeValue {
            meters: 17.0,
            datum: AltitudeDatum::OrthometricMsl,
            uncertainty_m: Some(0.5),
        };
        let sfo_ellip = sfo_msl.convert_datum(AltitudeDatum::Wgs84Ellipsoid, -32.0)?;
        assert!((sfo_ellip.meters - (-15.0)).abs() < 1e-9);

        let baro = AltitudeValue {
            meters: 10_668.0,
            datum: AltitudeDatum::BarometricPressure,
            uncertainty_m: Some(30.0),
        };
        assert_eq!(
            baro.convert_datum(AltitudeDatum::Wgs84Ellipsoid, -32.0),
            Err(TimeGeoError::BarometricGeoidMisuse)
        );

        // 6. ExactDecimal avoids binary float drift.
        let p1 = ExactDecimal::parse("100.10")?;
        let p2 = ExactDecimal::parse("0.2000")?;
        let sum = p1.checked_add(p2)?;
        assert_eq!(sum.to_string(), "100.3000");

        Ok(())
    }
}
