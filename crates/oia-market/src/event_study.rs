//! Daily event studies over point-in-time closing quotes.
//!
//! Day 0 is anchored to market awareness, which is the documented public
//! availability of a cited record. The event's historical time is not used. Session
//! offsets come from a pinned [`TradingCalendarVersion`]. Closing quotes are chosen
//! with the same admissibility rules as [`select_quote_as_of`] at one analysis
//! cutoff and put on one basis with [`adjust_quote`]. Per-session simple returns
//! remain exact rationals. Benchmark fitting and abnormal-return aggregation use
//! the documented `f64` method [`AGGREGATION_METHOD_ID`], because sums of exact
//! rationals overflow the checked `i128` domain within a few terms.
//!
//! Results are statistical associations between an anchored information instant
//! and returns; they never establish that the event caused a price change.
//!
//! **Provisional pending #12:** dates and `*_utc_ms` instants use the crate's local
//! conventions documented in `crate::model`.

use std::collections::BTreeSet;

use oia_domain::{OpaqueId, VersionId};

use crate::calendar::{TradingCalendarVersion, TradingSession};
use crate::model::{
    CalendarDate, CorporateActionRecord, CutoffMode, ExactRational, ExcludedVersion,
    ExclusionReason, Frequency, PriceAdjustmentBasis, QuoteRecord, RecordAvailability, Unit,
    availability_exclusion, validate_availability, validate_cutoff_mode,
};
use crate::point_in_time::{QuoteQuery, adjust_quote, select_quote_as_of, validate_action};
use crate::{MarketError, MarketLimits, StopReason, validate_timestamp};

/// Close-to-close simple return `P_t / P_{t-1} - 1` over consecutive listed sessions.
pub const SIMPLE_RETURN_METHOD_ID: &str = "simple-close-to-close-exact-rational-v1";
/// Exact returns converted to `f64` and aggregated with two-pass sums in offset order.
pub const AGGREGATION_METHOD_ID: &str = "f64-two-pass-session-order-v1";
/// `CAR / (sigma * sqrt(L))` under i.i.d. abnormal returns; ignores estimation error.
pub const STANDARDIZED_CAR_METHOD_ID: &str = "car-over-sigma-sqrt-l-iid-v1";
/// Largest accepted absolute session offset for either window.
pub const MAX_ABS_OFFSET: i32 = 10_000;

/// How expected returns are formed. Each model is named in results by its method ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchmarkModel {
    /// Expected return is the subject's mean estimation-window return.
    MeanAdjusted,
    /// Expected return is the same-session benchmark return.
    MarketAdjusted,
    /// Expected return is `alpha + beta * benchmark` fitted by OLS on the estimation window.
    MarketModelOls,
}

impl BenchmarkModel {
    #[must_use]
    pub fn method_id(self) -> &'static str {
        match self {
            Self::MeanAdjusted => "mean-adjusted-f64-v1",
            Self::MarketAdjusted => "market-adjusted-f64-v1",
            Self::MarketModelOls => "market-model-ols-f64-v1",
        }
    }

    #[must_use]
    pub fn requires_benchmark(self) -> bool {
        !matches!(self, Self::MeanAdjusted)
    }

    /// The smallest estimation sample with at least one residual degree of freedom.
    #[must_use]
    pub fn minimum_estimation_returns(self) -> usize {
        match self {
            Self::MeanAdjusted | Self::MarketAdjusted => 2,
            Self::MarketModelOls => 3,
        }
    }

    fn fitted_parameter_count(self) -> usize {
        match self {
            Self::MeanAdjusted | Self::MarketAdjusted => 1,
            Self::MarketModelOls => 2,
        }
    }
}

/// Inclusive range of trading-session offsets relative to day 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionWindow {
    start_offset: i32,
    end_offset: i32,
}

impl SessionWindow {
    pub fn new(start_offset: i32, end_offset: i32) -> Result<Self, MarketError> {
        if start_offset > end_offset {
            return Err(MarketError::InvalidEventStudy(
                "window start offset must not follow its inclusive end",
            ));
        }
        if start_offset.unsigned_abs() > MAX_ABS_OFFSET.unsigned_abs()
            || end_offset.unsigned_abs() > MAX_ABS_OFFSET.unsigned_abs()
        {
            return Err(MarketError::InvalidEventStudy(
                "window offset exceeds the supported session range",
            ));
        }
        Ok(Self {
            start_offset,
            end_offset,
        })
    }

    #[must_use]
    pub fn start_offset(self) -> i32 {
        self.start_offset
    }
    #[must_use]
    pub fn end_offset(self) -> i32 {
        self.end_offset
    }

    /// Number of sessions, and therefore returns, in the window.
    #[must_use]
    pub fn session_count(self) -> usize {
        usize::try_from(i64::from(self.end_offset) - i64::from(self.start_offset) + 1).unwrap_or(0)
    }

    fn offsets(self) -> std::ops::RangeInclusive<i32> {
        self.start_offset..=self.end_offset
    }
}

/// The cited record whose documented public availability defines market awareness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketAwarenessAnchor {
    pub event_id: OpaqueId,
    pub awareness_record_version: VersionId,
    pub awareness_source_version: VersionId,
    pub awareness: RecordAvailability,
    /// Historical occurrence time, retained for inspection; never used to place day 0.
    pub event_occurred_at_utc_ms: Option<i64>,
}

/// One exact instrument/venue/source/unit scope of daily closing quotes.
#[derive(Debug, Clone, Copy)]
pub struct PriceSeries<'a> {
    pub instrument_id: &'a OpaqueId,
    pub venue: &'a OpaqueId,
    pub source_id: &'a OpaqueId,
    pub unit: &'a Unit,
    pub quotes: &'a [QuoteRecord],
    pub actions: &'a [CorporateActionRecord],
}

#[derive(Debug, Clone, Copy)]
pub struct EventStudyRequest<'a> {
    pub calendar: &'a TradingCalendarVersion,
    pub anchor: &'a MarketAwarenessAnchor,
    pub subject: PriceSeries<'a>,
    /// Required by market-adjusted and market-model studies, rejected otherwise.
    pub benchmark: Option<PriceSeries<'a>>,
    pub model: BenchmarkModel,
    pub basis: PriceAdjustmentBasis,
    pub estimation_window: SessionWindow,
    pub event_window: SessionWindow,
    pub min_estimation_returns: usize,
    pub cutoff_utc_ms: i64,
    pub mode: &'a CutoffMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeriesRole {
    Subject,
    Benchmark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingClose {
    /// No daily quote in this scope was observed within the session bounds.
    NoDailyQuoteInSession,
    /// Quotes exist for the session, but none is admissible at the cutoff.
    NoAdmissibleQuote,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloseStatus {
    Present {
        quote_version: VersionId,
        source_version: VersionId,
        observed_at_utc_ms: i64,
        adjusted_price: ExactRational,
        applied_actions: Vec<VersionId>,
    },
    /// Never interpolated, held forward, or replaced by another session's close.
    Missing(MissingClose),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionClose {
    pub offset: i32,
    pub local_date: CalendarDate,
    pub close_at_utc_ms: i64,
    pub status: CloseStatus,
}

/// Selected closes and every exclusion for one price series across the study span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeriesCloses {
    pub role: SeriesRole,
    pub instrument_id: OpaqueId,
    pub adjustment_method_id: &'static str,
    pub closes: Vec<SessionClose>,
    pub quote_exclusions: Vec<ExcludedVersion>,
    /// Quote-independent action exclusions: availability, supersession, or post-cutoff effect.
    pub action_exclusions: Vec<ExcludedVersion>,
    /// Supplied quotes observed outside every study session; they are not used.
    pub quotes_outside_span: usize,
    /// Availability-excluded actions whose effective instant falls between the first
    /// and last present close.
    pub inadmissible_actions_within_span: Vec<VersionId>,
}

/// A return is missing because a bounding close is missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReturnGap {
    pub role: SeriesRole,
    pub missing_close_offset: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionReturn {
    pub offset: i32,
    pub local_date: CalendarDate,
    pub subject: Option<ExactRational>,
    pub benchmark: Option<ExactRational>,
    pub gaps: Vec<ReturnGap>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ModelParameters {
    MeanAdjusted {
        mean_return: f64,
    },
    /// The mean is reported for inspection; market-adjusted expected returns do not use it.
    MarketAdjusted {
        mean_abnormal_return: f64,
    },
    MarketModelOls {
        alpha: f64,
        beta: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EstimationOutcome {
    Fitted {
        returns_used: usize,
        parameters: ModelParameters,
        residual_std_dev: f64,
        degrees_of_freedom: usize,
    },
    /// Fewer usable estimation returns than the request's declared minimum.
    Insufficient {
        returns_available: usize,
        returns_required: usize,
    },
    /// Benchmark returns have zero estimation-window variance; beta is undefined.
    DegenerateBenchmarkVariance { returns_used: usize },
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventDay {
    pub offset: i32,
    pub local_date: CalendarDate,
    pub subject_return: Option<ExactRational>,
    pub benchmark_return: Option<ExactRational>,
    pub expected_return: Option<f64>,
    pub abnormal_return: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CarOutcome {
    /// `standardized` is absent when residual dispersion is zero.
    Complete { car: f64, standardized: Option<f64> },
    /// At least one event-window abnormal return is missing; no CAR is reported.
    MissingEventReturns { offsets: Vec<i32> },
    /// The benchmark model was not fitted, so no abnormal return exists.
    NotEstimated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Qualification {
    /// Awareness fell inside the day-0 session, so the day-0 return includes earlier trading.
    AwarenessWithinDayZeroSession,
    /// Residual dispersion is zero; no standardized CAR is reported.
    ZeroResidualDispersion,
    /// A corporate action excluded for availability takes effect inside the span, so
    /// returns across it may reflect an unadjusted share-basis change.
    InadmissibleActionWithinSpan(SeriesRole),
}

/// Event-study output never asserts that the event caused the measured returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interpretation {
    StatisticalAssociationOnly,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventStudyResult {
    event_id: OpaqueId,
    awareness_record_version: VersionId,
    awareness_source_version: VersionId,
    market_aware_at_utc_ms: i64,
    event_occurred_at_utc_ms: Option<i64>,
    day_zero_date: CalendarDate,
    calendar_version: VersionId,
    calendar_source_version: VersionId,
    model: BenchmarkModel,
    basis: PriceAdjustmentBasis,
    estimation_window: SessionWindow,
    event_window: SessionWindow,
    min_estimation_returns: usize,
    cutoff_utc_ms: i64,
    mode: CutoffMode,
    subject: SeriesCloses,
    benchmark: Option<SeriesCloses>,
    estimation_returns: Vec<SessionReturn>,
    estimation: EstimationOutcome,
    event_days: Vec<EventDay>,
    car: CarOutcome,
    qualifications: Vec<Qualification>,
}

impl EventStudyResult {
    #[must_use]
    pub fn event_id(&self) -> &OpaqueId {
        &self.event_id
    }
    #[must_use]
    pub fn awareness_record_version(&self) -> &VersionId {
        &self.awareness_record_version
    }
    #[must_use]
    pub fn awareness_source_version(&self) -> &VersionId {
        &self.awareness_source_version
    }
    #[must_use]
    pub fn market_aware_at_utc_ms(&self) -> i64 {
        self.market_aware_at_utc_ms
    }
    #[must_use]
    pub fn event_occurred_at_utc_ms(&self) -> Option<i64> {
        self.event_occurred_at_utc_ms
    }
    #[must_use]
    pub fn day_zero_date(&self) -> CalendarDate {
        self.day_zero_date
    }
    #[must_use]
    pub fn calendar_version(&self) -> &VersionId {
        &self.calendar_version
    }
    #[must_use]
    pub fn calendar_source_version(&self) -> &VersionId {
        &self.calendar_source_version
    }
    #[must_use]
    pub fn model(&self) -> BenchmarkModel {
        self.model
    }
    #[must_use]
    pub fn model_method_id(&self) -> &'static str {
        self.model.method_id()
    }
    #[must_use]
    pub fn return_method_id(&self) -> &'static str {
        SIMPLE_RETURN_METHOD_ID
    }
    #[must_use]
    pub fn aggregation_method_id(&self) -> &'static str {
        AGGREGATION_METHOD_ID
    }
    #[must_use]
    pub fn standardized_car_method_id(&self) -> &'static str {
        STANDARDIZED_CAR_METHOD_ID
    }
    #[must_use]
    pub fn basis(&self) -> PriceAdjustmentBasis {
        self.basis
    }
    #[must_use]
    pub fn estimation_window(&self) -> SessionWindow {
        self.estimation_window
    }
    #[must_use]
    pub fn event_window(&self) -> SessionWindow {
        self.event_window
    }
    #[must_use]
    pub fn min_estimation_returns(&self) -> usize {
        self.min_estimation_returns
    }
    #[must_use]
    pub fn cutoff_utc_ms(&self) -> i64 {
        self.cutoff_utc_ms
    }
    #[must_use]
    pub fn mode(&self) -> &CutoffMode {
        &self.mode
    }
    #[must_use]
    pub fn subject(&self) -> &SeriesCloses {
        &self.subject
    }
    #[must_use]
    pub fn benchmark(&self) -> Option<&SeriesCloses> {
        self.benchmark.as_ref()
    }
    #[must_use]
    pub fn estimation_returns(&self) -> &[SessionReturn] {
        &self.estimation_returns
    }
    #[must_use]
    pub fn estimation(&self) -> EstimationOutcome {
        self.estimation
    }
    #[must_use]
    pub fn event_days(&self) -> &[EventDay] {
        &self.event_days
    }
    #[must_use]
    pub fn car(&self) -> &CarOutcome {
        &self.car
    }
    #[must_use]
    pub fn qualifications(&self) -> &[Qualification] {
        &self.qualifications
    }
    #[must_use]
    pub fn interpretation(&self) -> Interpretation {
        Interpretation::StatisticalAssociationOnly
    }
}

/// Runs one bounded daily event study. A stop returns no result. Inputs are validated
/// completely; gaps and inadmissible data remain visible instead of being filled.
pub fn run_event_study(
    request: EventStudyRequest<'_>,
    limits: MarketLimits,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<EventStudyResult, MarketError> {
    check_control().map_err(MarketError::Stopped)?;
    validate_request(&request, limits)?;
    let aware = validate_anchor(request.anchor, request.cutoff_utc_ms, request.mode)?;

    let mut sessions: Vec<&TradingSession> = Vec::new();
    sessions
        .try_reserve_exact(request.calendar.sessions().len())
        .map_err(|_| MarketError::AllocationFailed)?;
    sessions.extend(request.calendar.sessions());
    sessions.sort_by_key(|session| (session.open_at_utc_ms, session.close_at_utc_ms));
    if sessions
        .windows(2)
        .any(|pair| matches!(pair, [left, right] if left.local_date >= right.local_date))
    {
        return Err(MarketError::InvalidCalendar(
            "session local dates are not in UTC session order",
        ));
    }

    // Day 0 is the first listed session whose close follows awareness.
    let day_zero = sessions.partition_point(|session| session.close_at_utc_ms <= aware);
    let low = i64::from(request.estimation_window.start_offset())
        .checked_sub(1)
        .ok_or(MarketError::ArithmeticOverflow)?
        .min(-1);
    let high = i64::from(request.event_window.end_offset()).max(0);
    let day_zero = i64::try_from(day_zero).map_err(|_| MarketError::ArithmeticOverflow)?;
    let first =
        usize::try_from(day_zero + low).map_err(|_| MarketError::StudySpanBeyondCalendar)?;
    let last =
        usize::try_from(day_zero + high).map_err(|_| MarketError::StudySpanBeyondCalendar)?;
    let span = sessions
        .get(first..=last)
        .ok_or(MarketError::StudySpanBeyondCalendar)?;
    limits.check_len(span.len())?;
    check_span_coverage(request.calendar, span, &mut check_control)?;
    let low = i32::try_from(low).map_err(|_| MarketError::ArithmeticOverflow)?;
    let day_zero_session = span
        .get(usize::try_from(-low).map_err(|_| MarketError::ArithmeticOverflow)?)
        .ok_or(MarketError::StudySpanBeyondCalendar)?;

    let subject = collect_closes(
        request.subject,
        SeriesRole::Subject,
        span,
        low,
        &request,
        limits,
        &mut check_control,
    )?;
    let benchmark = match request.benchmark {
        Some(series) => Some(collect_closes(
            series,
            SeriesRole::Benchmark,
            span,
            low,
            &request,
            limits,
            &mut check_control,
        )?),
        None => None,
    };

    let mut estimation_returns = Vec::new();
    estimation_returns
        .try_reserve_exact(request.estimation_window.session_count())
        .map_err(|_| MarketError::AllocationFailed)?;
    for offset in request.estimation_window.offsets() {
        check_control().map_err(MarketError::Stopped)?;
        estimation_returns.push(session_return(&subject, benchmark.as_ref(), offset, low)?);
    }
    let estimation = fit_model(
        request.model,
        &estimation_returns,
        request.min_estimation_returns,
    )?;

    let mut event_days = Vec::new();
    event_days
        .try_reserve_exact(request.event_window.session_count())
        .map_err(|_| MarketError::AllocationFailed)?;
    let mut missing_offsets = Vec::new();
    let mut car_sum = 0.0_f64;
    for offset in request.event_window.offsets() {
        check_control().map_err(MarketError::Stopped)?;
        let observed = session_return(&subject, benchmark.as_ref(), offset, low)?;
        let expected = expected_return(estimation, &observed)?;
        let abnormal = match (observed.subject, expected) {
            (Some(actual), Some(expected)) => Some(finite(to_f64(actual) - expected)?),
            _ => None,
        };
        match abnormal {
            Some(value) => car_sum = finite(car_sum + value)?,
            None => {
                missing_offsets
                    .try_reserve(1)
                    .map_err(|_| MarketError::AllocationFailed)?;
                missing_offsets.push(offset);
            }
        }
        event_days.push(EventDay {
            offset,
            local_date: observed.local_date,
            subject_return: observed.subject,
            benchmark_return: observed.benchmark,
            expected_return: expected,
            abnormal_return: abnormal,
        });
    }

    let mut qualifications = Vec::new();
    for series in std::iter::once(&subject).chain(benchmark.as_ref()) {
        if !series.inadmissible_actions_within_span.is_empty() {
            qualifications.push(Qualification::InadmissibleActionWithinSpan(series.role));
        }
    }
    if aware >= day_zero_session.open_at_utc_ms {
        qualifications.push(Qualification::AwarenessWithinDayZeroSession);
    }
    let car = match estimation {
        EstimationOutcome::Fitted {
            residual_std_dev, ..
        } => {
            if missing_offsets.is_empty() {
                let standardized = if residual_std_dev > 0.0 {
                    let count = request.event_window.session_count() as f64;
                    Some(finite(car_sum / (residual_std_dev * count.sqrt()))?)
                } else {
                    qualifications.push(Qualification::ZeroResidualDispersion);
                    None
                };
                CarOutcome::Complete {
                    car: car_sum,
                    standardized,
                }
            } else {
                CarOutcome::MissingEventReturns {
                    offsets: missing_offsets,
                }
            }
        }
        EstimationOutcome::Insufficient { .. }
        | EstimationOutcome::DegenerateBenchmarkVariance { .. } => CarOutcome::NotEstimated,
    };

    check_control().map_err(MarketError::Stopped)?;
    Ok(EventStudyResult {
        event_id: request.anchor.event_id.clone(),
        awareness_record_version: request.anchor.awareness_record_version.clone(),
        awareness_source_version: request.anchor.awareness_source_version.clone(),
        market_aware_at_utc_ms: aware,
        event_occurred_at_utc_ms: request.anchor.event_occurred_at_utc_ms,
        day_zero_date: day_zero_session.local_date,
        calendar_version: request.calendar.version().clone(),
        calendar_source_version: request.calendar.source_version().clone(),
        model: request.model,
        basis: request.basis,
        estimation_window: request.estimation_window,
        event_window: request.event_window,
        min_estimation_returns: request.min_estimation_returns,
        cutoff_utc_ms: request.cutoff_utc_ms,
        mode: request.mode.clone(),
        subject,
        benchmark,
        estimation_returns,
        estimation,
        event_days,
        car,
        qualifications,
    })
}

fn validate_request(
    request: &EventStudyRequest<'_>,
    limits: MarketLimits,
) -> Result<(), MarketError> {
    validate_timestamp("event-study cutoff_utc_ms", request.cutoff_utc_ms)?;
    validate_cutoff_mode(request.mode)?;
    if request.basis == PriceAdjustmentBasis::TotalReturn {
        return Err(MarketError::UnsupportedAdjustment);
    }
    match (request.model.requires_benchmark(), request.benchmark) {
        (true, None) => {
            return Err(MarketError::InvalidEventStudy(
                "the benchmark model requires a benchmark series",
            ));
        }
        (false, Some(_)) => {
            return Err(MarketError::InvalidEventStudy(
                "a benchmark series was supplied to a model that does not use it",
            ));
        }
        (true, Some(benchmark)) if benchmark.instrument_id == request.subject.instrument_id => {
            return Err(MarketError::InvalidEventStudy(
                "the benchmark must be a different instrument from the subject",
            ));
        }
        _ => {}
    }
    if request.estimation_window.end_offset() >= request.event_window.start_offset() {
        return Err(MarketError::InvalidEventStudy(
            "the estimation window must end before the event window starts",
        ));
    }
    if request.min_estimation_returns < request.model.minimum_estimation_returns() {
        return Err(MarketError::InvalidEventStudy(
            "the minimum estimation sample leaves no residual degree of freedom",
        ));
    }
    if request.min_estimation_returns > request.estimation_window.session_count() {
        return Err(MarketError::InvalidEventStudy(
            "the minimum estimation sample exceeds the estimation window",
        ));
    }
    let mut total = 0_usize;
    for series in std::iter::once(request.subject).chain(request.benchmark) {
        total = total
            .checked_add(series.quotes.len())
            .and_then(|sum| sum.checked_add(series.actions.len()))
            .ok_or(MarketError::ArithmeticOverflow)?;
    }
    limits.check_len(total)
}

fn validate_anchor(
    anchor: &MarketAwarenessAnchor,
    cutoff_utc_ms: i64,
    mode: &CutoffMode,
) -> Result<i64, MarketError> {
    validate_availability(anchor.awareness)?;
    if let Some(occurred) = anchor.event_occurred_at_utc_ms {
        validate_timestamp("event_occurred_at_utc_ms", occurred)?;
    }
    if let Some(reason) = availability_exclusion(anchor.awareness, cutoff_utc_ms, mode) {
        return Err(MarketError::AnchorExcluded(reason));
    }
    anchor
        .awareness
        .public_available_at_utc_ms
        .ok_or(MarketError::AnchorExcluded(
            ExclusionReason::PublicAvailabilityUnknown,
        ))
}

/// Every civil date between consecutive span sessions must be a recorded closure;
/// an unknown date could be an unlisted session that would shift every offset.
fn check_span_coverage(
    calendar: &TradingCalendarVersion,
    span: &[&TradingSession],
    check_control: &mut impl FnMut() -> Result<(), StopReason>,
) -> Result<(), MarketError> {
    let closures: BTreeSet<CalendarDate> = calendar
        .closures()
        .iter()
        .map(|closure| closure.local_date)
        .collect();
    for pair in span.windows(2) {
        check_control().map_err(MarketError::Stopped)?;
        let [left, right] = pair else { continue };
        let mut date = next_date(left.local_date)?;
        while date < right.local_date {
            if !closures.contains(&date) {
                return Err(MarketError::CalendarCoverageUnknown(date));
            }
            date = next_date(date)?;
        }
    }
    Ok(())
}

fn next_date(date: CalendarDate) -> Result<CalendarDate, MarketError> {
    if let Some(day) = date.day().checked_add(1)
        && let Ok(next) = CalendarDate::new(date.year(), date.month(), day)
    {
        return Ok(next);
    }
    if date.month() < 12 {
        return CalendarDate::new(date.year(), date.month() + 1, 1);
    }
    let year = date
        .year()
        .checked_add(1)
        .ok_or(MarketError::InvalidDate("date successor exceeds year 9999"))?;
    CalendarDate::new(year, 1, 1)
}

fn collect_closes(
    series: PriceSeries<'_>,
    role: SeriesRole,
    span: &[&TradingSession],
    low: i32,
    request: &EventStudyRequest<'_>,
    limits: MarketLimits,
    check_control: &mut impl FnMut() -> Result<(), StopReason>,
) -> Result<SeriesCloses, MarketError> {
    let query = QuoteQuery {
        instrument_id: series.instrument_id,
        venue: series.venue,
        source_id: series.source_id,
        unit: series.unit,
        cutoff_utc_ms: request.cutoff_utc_ms,
        mode: request.mode,
    };
    // Validates scope, timestamps, duplicate versions, and tie rules for the whole series.
    select_quote_as_of(series.quotes, query, limits, &mut *check_control)?;
    for action in series.actions {
        validate_action(action)?;
        if action.instrument_id != *series.instrument_id {
            return Err(MarketError::ScopeMismatch("action instrument_id"));
        }
    }

    let mut buckets: Vec<Vec<QuoteRecord>> = Vec::new();
    buckets
        .try_reserve_exact(span.len())
        .map_err(|_| MarketError::AllocationFailed)?;
    buckets.resize_with(span.len(), Vec::new);
    let mut quotes_outside_span = 0_usize;
    for quote in series.quotes {
        check_control().map_err(MarketError::Stopped)?;
        if quote.frequency != Frequency::Daily {
            return Err(MarketError::InvalidEventStudy(
                "event studies use daily closing quotes only",
            ));
        }
        let observed = quote.observed_at_utc_ms;
        let index = span.partition_point(|session| session.close_at_utc_ms < observed);
        let containing = span
            .get(index)
            .is_some_and(|session| session.open_at_utc_ms <= observed);
        if !containing {
            quotes_outside_span += 1;
            continue;
        }
        if span
            .get(index + 1)
            .is_some_and(|session| session.open_at_utc_ms <= observed)
        {
            return Err(MarketError::InvalidEventStudy(
                "a quote instant lies on a boundary shared by two sessions",
            ));
        }
        let bucket = buckets
            .get_mut(index)
            .ok_or(MarketError::StudySpanBeyondCalendar)?;
        bucket
            .try_reserve(1)
            .map_err(|_| MarketError::AllocationFailed)?;
        bucket.push(quote.clone());
    }

    let mut closes = Vec::new();
    closes
        .try_reserve_exact(span.len())
        .map_err(|_| MarketError::AllocationFailed)?;
    let mut quote_exclusions = Vec::new();
    let mut first_present: Option<QuoteRecord> = None;
    let mut last_present_observed = None;
    let mut adjustment_method_id = None;
    for (index, (session, bucket)) in span.iter().zip(&buckets).enumerate() {
        check_control().map_err(MarketError::Stopped)?;
        let offset = low
            .checked_add(i32::try_from(index).map_err(|_| MarketError::ArithmeticOverflow)?)
            .ok_or(MarketError::ArithmeticOverflow)?;
        let status = if bucket.is_empty() {
            CloseStatus::Missing(MissingClose::NoDailyQuoteInSession)
        } else {
            let selection = select_quote_as_of(bucket, query, limits, &mut *check_control)?;
            quote_exclusions
                .try_reserve(selection.exclusions().len())
                .map_err(|_| MarketError::AllocationFailed)?;
            quote_exclusions.extend_from_slice(selection.exclusions());
            match selection.selected() {
                None => CloseStatus::Missing(MissingClose::NoAdmissibleQuote),
                Some(quote) => {
                    let adjusted = adjust_quote(
                        quote,
                        series.actions,
                        request.basis,
                        request.cutoff_utc_ms,
                        request.mode,
                        limits,
                        &mut *check_control,
                    )?;
                    if adjustment_method_id.is_none() {
                        adjustment_method_id = Some(adjusted_method(request.basis));
                    }
                    if first_present.is_none() {
                        first_present = Some(quote.clone());
                    }
                    last_present_observed = Some(quote.observed_at_utc_ms);
                    CloseStatus::Present {
                        quote_version: quote.version.clone(),
                        source_version: quote.source_version.clone(),
                        observed_at_utc_ms: quote.observed_at_utc_ms,
                        adjusted_price: adjusted.adjusted_price(),
                        applied_actions: adjusted.applied_actions().to_vec(),
                    }
                }
            }
        };
        closes.push(SessionClose {
            offset,
            local_date: session.local_date,
            close_at_utc_ms: session.close_at_utc_ms,
            status,
        });
    }

    // A split-adjusted probe on the earliest close yields the quote-independent
    // action exclusions and detects splits that an unadjusted basis would turn
    // into spurious returns.
    let mut action_exclusions = Vec::new();
    let mut inadmissible_actions_within_span = Vec::new();
    if let (Some(first), Some(last_observed)) = (first_present, last_present_observed) {
        let probe = adjust_quote(
            &first,
            series.actions,
            PriceAdjustmentBasis::SplitAdjusted,
            request.cutoff_utc_ms,
            request.mode,
            limits,
            &mut *check_control,
        )?;
        action_exclusions
            .try_reserve(probe.excluded_actions().len())
            .map_err(|_| MarketError::AllocationFailed)?;
        action_exclusions.extend(
            probe
                .excluded_actions()
                .iter()
                .filter(|excluded| excluded.reason != ExclusionReason::NotEffectiveForQuote)
                .cloned(),
        );
        for excluded in probe.excluded_actions() {
            let availability_reason = matches!(
                excluded.reason,
                ExclusionReason::PublicAvailabilityUnknown
                    | ExclusionReason::PublicAvailabilityAfterCutoff
                    | ExclusionReason::IngestionUnknownStrict
                    | ExclusionReason::IngestedAfterStrictCutoff
            );
            let within_span = series.actions.iter().any(|action| {
                action.version == excluded.version
                    && action.effective_at_utc_ms > first.observed_at_utc_ms
                    && action.effective_at_utc_ms <= last_observed
            });
            if availability_reason && within_span {
                inadmissible_actions_within_span
                    .try_reserve(1)
                    .map_err(|_| MarketError::AllocationFailed)?;
                inadmissible_actions_within_span.push(excluded.version.clone());
            }
        }
        if request.basis == PriceAdjustmentBasis::Unadjusted {
            let split_inside_span = probe.applied_actions().iter().any(|applied| {
                series.actions.iter().any(|action| {
                    &action.version == applied && action.effective_at_utc_ms <= last_observed
                })
            });
            if split_inside_span {
                return Err(MarketError::InvalidEventStudy(
                    "an admissible split falls inside the study span; unadjusted returns would be spurious",
                ));
            }
        }
    }

    Ok(SeriesCloses {
        role,
        instrument_id: series.instrument_id.clone(),
        adjustment_method_id: adjustment_method_id.unwrap_or(adjusted_method(request.basis)),
        closes,
        quote_exclusions,
        action_exclusions,
        quotes_outside_span,
        inadmissible_actions_within_span,
    })
}

fn adjusted_method(basis: PriceAdjustmentBasis) -> &'static str {
    match basis {
        PriceAdjustmentBasis::SplitAdjusted => crate::point_in_time::SPLIT_ADJUSTED_METHOD_ID,
        PriceAdjustmentBasis::Unadjusted | PriceAdjustmentBasis::TotalReturn => {
            crate::point_in_time::UNADJUSTED_METHOD_ID
        }
    }
}

fn present_price(
    closes: &SeriesCloses,
    offset: i32,
    low: i32,
) -> Result<Option<ExactRational>, MarketError> {
    let index = usize::try_from(
        offset
            .checked_sub(low)
            .ok_or(MarketError::ArithmeticOverflow)?,
    )
    .map_err(|_| MarketError::StudySpanBeyondCalendar)?;
    let close = closes
        .closes
        .get(index)
        .ok_or(MarketError::StudySpanBeyondCalendar)?;
    Ok(match close.status {
        CloseStatus::Present { adjusted_price, .. } => Some(adjusted_price),
        CloseStatus::Missing(_) => None,
    })
}

fn series_return(
    closes: &SeriesCloses,
    offset: i32,
    low: i32,
    gaps: &mut Vec<ReturnGap>,
) -> Result<Option<ExactRational>, MarketError> {
    let previous_offset = offset
        .checked_sub(1)
        .ok_or(MarketError::ArithmeticOverflow)?;
    let previous = present_price(closes, previous_offset, low)?;
    let current = present_price(closes, offset, low)?;
    for (price, missing_offset) in [(previous, previous_offset), (current, offset)] {
        if price.is_none() {
            gaps.try_reserve(1)
                .map_err(|_| MarketError::AllocationFailed)?;
            gaps.push(ReturnGap {
                role: closes.role,
                missing_close_offset: missing_offset,
            });
        }
    }
    match (previous, current) {
        (Some(previous), Some(current)) => simple_return(previous, current).map(Some),
        _ => Ok(None),
    }
}

fn session_return(
    subject: &SeriesCloses,
    benchmark: Option<&SeriesCloses>,
    offset: i32,
    low: i32,
) -> Result<SessionReturn, MarketError> {
    let index = usize::try_from(
        offset
            .checked_sub(low)
            .ok_or(MarketError::ArithmeticOverflow)?,
    )
    .map_err(|_| MarketError::StudySpanBeyondCalendar)?;
    let local_date = subject
        .closes
        .get(index)
        .ok_or(MarketError::StudySpanBeyondCalendar)?
        .local_date;
    let mut gaps = Vec::new();
    let subject_return = series_return(subject, offset, low, &mut gaps)?;
    let benchmark_return = match benchmark {
        Some(closes) => series_return(closes, offset, low, &mut gaps)?,
        None => None,
    };
    Ok(SessionReturn {
        offset,
        local_date,
        subject: subject_return,
        benchmark: benchmark_return,
        gaps,
    })
}

/// Exact `current / previous - 1`; simple returns require strictly positive prices.
fn simple_return(
    previous: ExactRational,
    current: ExactRational,
) -> Result<ExactRational, MarketError> {
    if previous.numerator() <= 0 || current.numerator() <= 0 {
        return Err(MarketError::InvalidEventStudy(
            "simple returns require strictly positive prices",
        ));
    }
    let gross_numerator = current
        .numerator()
        .checked_mul(previous.denominator())
        .ok_or(MarketError::ArithmeticOverflow)?;
    let denominator = current
        .denominator()
        .checked_mul(previous.numerator())
        .ok_or(MarketError::ArithmeticOverflow)?;
    ExactRational::new(
        gross_numerator
            .checked_sub(denominator)
            .ok_or(MarketError::ArithmeticOverflow)?,
        denominator,
    )
}

fn to_f64(value: ExactRational) -> f64 {
    value.numerator() as f64 / value.denominator() as f64
}

fn finite(value: f64) -> Result<f64, MarketError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(MarketError::NonFiniteResult)
    }
}

fn fit_model(
    model: BenchmarkModel,
    returns: &[SessionReturn],
    min_required: usize,
) -> Result<EstimationOutcome, MarketError> {
    let mut pairs: Vec<(f64, f64)> = Vec::new();
    pairs
        .try_reserve_exact(returns.len())
        .map_err(|_| MarketError::AllocationFailed)?;
    for session in returns {
        let Some(subject) = session.subject else {
            continue;
        };
        let benchmark = match (model.requires_benchmark(), session.benchmark) {
            (false, _) => 0.0,
            (true, Some(benchmark)) => to_f64(benchmark),
            (true, None) => continue,
        };
        pairs.push((benchmark, to_f64(subject)));
    }
    let count = pairs.len();
    if count < min_required {
        return Ok(EstimationOutcome::Insufficient {
            returns_available: count,
            returns_required: min_required,
        });
    }
    let n = count as f64;
    let degrees_of_freedom = count
        .checked_sub(model.fitted_parameter_count())
        .filter(|degrees| *degrees > 0)
        .ok_or(MarketError::InvalidEventStudy(
            "the estimation sample leaves no residual degree of freedom",
        ))?;
    let (parameters, residual_sum_squares) = match model {
        BenchmarkModel::MeanAdjusted => {
            let mean = finite(pairs.iter().map(|(_, y)| y).sum::<f64>() / n)?;
            let ss = pairs.iter().map(|(_, y)| (y - mean).powi(2)).sum::<f64>();
            (ModelParameters::MeanAdjusted { mean_return: mean }, ss)
        }
        BenchmarkModel::MarketAdjusted => {
            let mean = finite(pairs.iter().map(|(x, y)| y - x).sum::<f64>() / n)?;
            let ss = pairs
                .iter()
                .map(|(x, y)| (y - x - mean).powi(2))
                .sum::<f64>();
            (
                ModelParameters::MarketAdjusted {
                    mean_abnormal_return: mean,
                },
                ss,
            )
        }
        BenchmarkModel::MarketModelOls => {
            let mean_x = finite(pairs.iter().map(|(x, _)| x).sum::<f64>() / n)?;
            let mean_y = finite(pairs.iter().map(|(_, y)| y).sum::<f64>() / n)?;
            let sxx = finite(pairs.iter().map(|(x, _)| (x - mean_x).powi(2)).sum::<f64>())?;
            if sxx == 0.0 {
                return Ok(EstimationOutcome::DegenerateBenchmarkVariance {
                    returns_used: count,
                });
            }
            let sxy = pairs
                .iter()
                .map(|(x, y)| (x - mean_x) * (y - mean_y))
                .sum::<f64>();
            let beta = finite(sxy / sxx)?;
            let alpha = finite(mean_y - beta * mean_x)?;
            let ss = pairs
                .iter()
                .map(|(x, y)| (y - alpha - beta * x).powi(2))
                .sum::<f64>();
            (ModelParameters::MarketModelOls { alpha, beta }, ss)
        }
    };
    let residual_std_dev =
        finite((finite(residual_sum_squares)? / degrees_of_freedom as f64).sqrt())?;
    Ok(EstimationOutcome::Fitted {
        returns_used: count,
        parameters,
        residual_std_dev,
        degrees_of_freedom,
    })
}

fn expected_return(
    estimation: EstimationOutcome,
    observed: &SessionReturn,
) -> Result<Option<f64>, MarketError> {
    let EstimationOutcome::Fitted { parameters, .. } = estimation else {
        return Ok(None);
    };
    let value = match (parameters, observed.benchmark) {
        (ModelParameters::MeanAdjusted { mean_return }, _) => Some(mean_return),
        (ModelParameters::MarketAdjusted { .. }, Some(benchmark)) => Some(to_f64(benchmark)),
        (ModelParameters::MarketModelOls { alpha, beta }, Some(benchmark)) => {
            Some(alpha + beta * to_f64(benchmark))
        }
        (_, None) => None,
    };
    value.map(finite).transpose()
}
