//! Synthetic event-study checks against independently computed reference values.
//!
//! Reference values were computed outside Rust with exact `fractions.Fraction`
//! returns and IEEE-754 double aggregation in the same documented order; the
//! 1e-12 tolerance covers last-bit differences in rational-to-double conversion.
//! Risks covered: benchmark arithmetic, split/holiday handling, awareness anchoring,
//! strict cutoffs, explicit gaps, unknown calendar coverage, and cancellation.

use std::error::Error;

use oia_domain::{OpaqueId, VersionId};
use oia_market::calendar::{
    ClosureKind, SessionKind, TradingCalendarInput, TradingCalendarVersion, TradingClosure,
    TradingSession,
};
use oia_market::event_study::{
    BenchmarkModel, CarOutcome, CloseStatus, EstimationOutcome, EventStudyRequest,
    MarketAwarenessAnchor, MissingClose, ModelParameters, PriceSeries, Qualification, SeriesRole,
    SessionWindow, run_event_study,
};
use oia_market::model::{
    CalendarDate, CorporateActionRecord, CutoffMode, ExactRational, ExclusionReason, FixedDecimal,
    Frequency, PriceAdjustmentBasis, QuoteDelay, QuoteRecord, RecordAvailability, Unit,
};
use oia_market::{MarketError, MarketLimits, StopReason};

type TestResult = Result<(), Box<dyn Error>>;

const JAN_1_2024_UTC_MS: i64 = 1_704_067_200_000;
const DAY_MS: i64 = 86_400_000;
const OPEN_MS: i64 = 52_200_000; // 14:30Z
const CLOSE_MS: i64 = 75_600_000; // 21:00Z
const SESSION_DAYS: [u8; 13] = [2, 3, 4, 5, 8, 9, 10, 11, 12, 16, 17, 18, 19];
const SUBJECT_CENTS: [i128; 11] = [
    10_000, 10_100, 10_080, 10_200, 10_150, 10_220, 10_300, 10_350, 11_000, 5_600, 5_550,
];
const BENCHMARK_CENTS: [i128; 11] = [
    400_000, 402_000, 401_000, 404_000, 403_000, 405_000, 406_000, 407_000, 408_000, 410_000,
    409_000,
];

fn day(day: u8) -> i64 {
    JAN_1_2024_UTC_MS + (i64::from(day) - 1) * DAY_MS
}

fn date(day: u8) -> Result<CalendarDate, Box<dyn Error>> {
    Ok(CalendarDate::new(2024, 1, day)?)
}

struct Fixture {
    venue: OpaqueId,
    source: OpaqueId,
    unit: Unit,
    subject_id: OpaqueId,
    benchmark_id: OpaqueId,
    calendar: TradingCalendarVersion,
    anchor: MarketAwarenessAnchor,
    subject_quotes: Vec<QuoteRecord>,
    benchmark_quotes: Vec<QuoteRecord>,
    split: Vec<CorporateActionRecord>,
    strict: CutoffMode,
}

fn calendar_input(venue: &OpaqueId) -> Result<TradingCalendarInput, Box<dyn Error>> {
    let mut sessions = Vec::new();
    for session_day in SESSION_DAYS {
        sessions.push(TradingSession {
            local_date: date(session_day)?,
            open_at_utc_ms: day(session_day) + OPEN_MS,
            close_at_utc_ms: day(session_day) + CLOSE_MS,
            kind: SessionKind::Ordinary,
        });
    }
    let mut closures = Vec::new();
    for (closure_day, kind) in [
        (6, ClosureKind::Weekend),
        (7, ClosureKind::Weekend),
        (13, ClosureKind::Weekend),
        (14, ClosureKind::Weekend),
        (15, ClosureKind::Holiday),
    ] {
        closures.push(TradingClosure {
            local_date: date(closure_day)?,
            kind,
            reason: "synthetic closure".into(),
        });
    }
    Ok(TradingCalendarInput {
        venue: venue.clone(),
        version: VersionId::new("calendar-v1")?,
        source_version: VersionId::new("calendar-source-v1")?,
        timezone_identifier: "America/New_York".into(),
        coverage_start: date(2)?,
        coverage_end: date(19)?,
        sessions,
        closures,
    })
}

fn closes(
    instrument: &OpaqueId,
    fixture_venue: &OpaqueId,
    source: &OpaqueId,
    unit: &Unit,
    prefix: &str,
    cents: &[i128],
) -> Result<Vec<QuoteRecord>, Box<dyn Error>> {
    let mut quotes = Vec::new();
    for (session_day, price) in SESSION_DAYS.iter().zip(cents) {
        let close = day(*session_day) + CLOSE_MS;
        quotes.push(QuoteRecord {
            instrument_id: instrument.clone(),
            venue: fixture_venue.clone(),
            source_id: source.clone(),
            version: VersionId::new(format!("{prefix}-close-{session_day}"))?,
            source_version: VersionId::new(format!("{prefix}-source-{session_day}"))?,
            observed_at_utc_ms: close,
            availability: RecordAvailability {
                public_available_at_utc_ms: Some(close + 900_000),
                ingested_at_utc_ms: Some(close + 1_200_000),
            },
            price: FixedDecimal::new(*price, 2)?,
            unit: unit.clone(),
            frequency: Frequency::Daily,
            delay: QuoteDelay::EndOfDay,
        });
    }
    Ok(quotes)
}

fn build_fixture() -> Result<Fixture, Box<dyn Error>> {
    let venue = OpaqueId::new("venue:synthetic")?;
    let source = OpaqueId::new("source:synthetic-eod")?;
    let unit = Unit::new("USD")?;
    let subject_id = OpaqueId::new("instrument:subject")?;
    let benchmark_id = OpaqueId::new("instrument:benchmark")?;
    let calendar = TradingCalendarVersion::new(calendar_input(&venue)?, MarketLimits::new(64)?)?;
    let subject_quotes = closes(
        &subject_id,
        &venue,
        &source,
        &unit,
        "subject",
        &SUBJECT_CENTS,
    )?;
    let benchmark_quotes = closes(
        &benchmark_id,
        &venue,
        &source,
        &unit,
        "benchmark",
        &BENCHMARK_CENTS,
    )?;
    let split = vec![CorporateActionRecord {
        instrument_id: subject_id.clone(),
        action_id: OpaqueId::new("action:split-2-for-1")?,
        version: VersionId::new("split-v1")?,
        source_version: VersionId::new("split-source-v1")?,
        revision: 1,
        predecessor_version: None,
        effective_at_utc_ms: day(16) + OPEN_MS,
        availability: RecordAvailability {
            public_available_at_utc_ms: Some(day(10)),
            ingested_at_utc_ms: Some(day(10)),
        },
        split_numerator: 2,
        split_denominator: 1,
    }];
    // The event occurs during the Jan 11 session but becomes public after its close.
    let anchor = MarketAwarenessAnchor {
        event_id: OpaqueId::new("event:synthetic")?,
        awareness_record_version: VersionId::new("disclosure-v1")?,
        awareness_source_version: VersionId::new("disclosure-source-v1")?,
        awareness: RecordAvailability {
            public_available_at_utc_ms: Some(day(11) + 79_200_000),
            ingested_at_utc_ms: Some(day(11) + 79_500_000),
        },
        event_occurred_at_utc_ms: Some(day(11) + 64_800_000),
    };
    Ok(Fixture {
        venue,
        source,
        unit,
        subject_id,
        benchmark_id,
        calendar,
        anchor,
        subject_quotes,
        benchmark_quotes,
        split,
        strict: CutoffMode::ProspectiveStrict,
    })
}

fn request<'a>(
    fixture: &'a Fixture,
    model: BenchmarkModel,
    cutoff_utc_ms: i64,
) -> Result<EventStudyRequest<'a>, Box<dyn Error>> {
    let benchmark = model.requires_benchmark().then_some(PriceSeries {
        instrument_id: &fixture.benchmark_id,
        venue: &fixture.venue,
        source_id: &fixture.source,
        unit: &fixture.unit,
        quotes: &fixture.benchmark_quotes,
        actions: &[],
    });
    Ok(EventStudyRequest {
        calendar: &fixture.calendar,
        anchor: &fixture.anchor,
        subject: PriceSeries {
            instrument_id: &fixture.subject_id,
            venue: &fixture.venue,
            source_id: &fixture.source,
            unit: &fixture.unit,
            quotes: &fixture.subject_quotes,
            actions: &fixture.split,
        },
        benchmark,
        model,
        basis: PriceAdjustmentBasis::SplitAdjusted,
        estimation_window: SessionWindow::new(-7, -2)?,
        event_window: SessionWindow::new(-1, 2)?,
        min_estimation_returns: 5,
        cutoff_utc_ms,
        mode: &fixture.strict,
    })
}

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 1e-12,
        "actual {actual:e} differs from reference {expected:e}"
    );
}

fn complete_car(car: &CarOutcome) -> Result<(f64, Option<f64>), Box<dyn Error>> {
    match car {
        CarOutcome::Complete { car, standardized } => Ok((*car, *standardized)),
        other => Err(format!("expected a complete CAR, got {other:?}").into()),
    }
}

#[test]
fn benchmark_models_match_reference_fixture() -> TestResult {
    let fixture = build_fixture()?;
    let limits = MarketLimits::new(256)?;
    let cutoff = day(19);
    let result = run_event_study(
        request(&fixture, BenchmarkModel::MarketModelOls, cutoff)?,
        limits,
        || Ok(()),
    )?;

    // Awareness after the Jan 11 close places day 0 on Jan 12, not the occurrence date.
    assert_eq!(result.day_zero_date(), date(12)?);
    assert!(result.qualifications().is_empty());
    assert_eq!(result.model_method_id(), "market-model-ols-f64-v1");
    let offsets_and_dates: Vec<(i32, CalendarDate)> = result
        .event_days()
        .iter()
        .map(|event_day| (event_day.offset, event_day.local_date))
        .collect();
    // The weekend and the Jan 15 holiday are not sessions.
    assert_eq!(
        offsets_and_dates,
        vec![
            (-1, date(11)?),
            (0, date(12)?),
            (1, date(16)?),
            (2, date(17)?)
        ]
    );
    // Exact returns: the 2-for-1 split on Jan 16 is not a -49% return.
    assert_eq!(
        result.event_days().get(2).and_then(|d| d.subject_return),
        Some(ExactRational::new(1, 55)?)
    );
    assert_eq!(
        result.event_days().get(1).and_then(|d| d.subject_return),
        Some(ExactRational::new(13, 207)?)
    );
    let Some(CloseStatus::Present {
        applied_actions, ..
    }) = result.subject().closes.first().map(|close| &close.status)
    else {
        return Err("first subject close is missing".into());
    };
    assert_eq!(applied_actions, &vec![VersionId::new("split-v1")?]);

    let EstimationOutcome::Fitted {
        returns_used,
        parameters: ModelParameters::MarketModelOls { alpha, beta },
        residual_std_dev,
        degrees_of_freedom,
    } = result.estimation()
    else {
        return Err(format!("unexpected estimation {:?}", result.estimation()).into());
    };
    assert_eq!((returns_used, degrees_of_freedom), (6, 4));
    assert_close(alpha, 0.001_064_616_985_605_417_6);
    assert_close(beta, 1.562_449_060_301_089_4);
    assert_close(residual_std_dev, 0.002_192_750_899_825_315);
    let reference_ar = [
        -5.864_475_381_557_165e-5,
        0.057_898_374_201_443_385,
        0.009_458_137_175_128_991,
        -0.006_182_337_047_588_823,
    ];
    for (event_day, expected) in result.event_days().iter().zip(reference_ar) {
        assert_close(event_day.abnormal_return.ok_or("missing AR")?, expected);
    }
    let (car, standardized) = complete_car(result.car())?;
    assert_close(car, 0.061_115_529_575_167_984);
    assert!(
        (standardized.ok_or("missing standardized CAR")? - 13.935_812_221_086_476).abs() < 1e-9
    );

    for (model, reference_car, reference_std) in [
        (
            BenchmarkModel::MeanAdjusted,
            0.057_078_252_402_774_955,
            4.198_470_957_782_177_5,
        ),
        (
            BenchmarkModel::MarketAdjusted,
            0.069_526_555_014_170_95,
            11.377_519_772_903_64,
        ),
    ] {
        let result = run_event_study(request(&fixture, model, cutoff)?, limits, || Ok(()))?;
        let (car, standardized) = complete_car(result.car())?;
        assert_close(car, reference_car);
        assert!((standardized.ok_or("missing standardized CAR")? - reference_std).abs() < 1e-9);
    }

    // An unadjusted basis across an admissible split is rejected, not reported.
    let mut unadjusted = request(&fixture, BenchmarkModel::MarketModelOls, cutoff)?;
    unadjusted.basis = PriceAdjustmentBasis::Unadjusted;
    assert!(matches!(
        run_event_study(unadjusted, limits, || Ok(())),
        Err(MarketError::InvalidEventStudy(_))
    ));
    Ok(())
}

#[test]
fn gaps_cutoffs_and_coverage_stay_explicit() -> TestResult {
    let fixture = build_fixture()?;
    let limits = MarketLimits::new(256)?;

    // The Jan 17 close is observed before the cutoff but published after it.
    let cutoff = day(17) + CLOSE_MS + 600_000;
    let result = run_event_study(
        request(&fixture, BenchmarkModel::MarketModelOls, cutoff)?,
        limits,
        || Ok(()),
    )?;
    assert_eq!(
        result.car(),
        &CarOutcome::MissingEventReturns { offsets: vec![2] }
    );
    assert_eq!(
        result.subject().closes.last().map(|close| &close.status),
        Some(&CloseStatus::Missing(MissingClose::NoAdmissibleQuote))
    );
    assert!(result.subject().quote_exclusions.iter().any(|excluded| {
        excluded.version.as_str() == "subject-close-17"
            && excluded.reason == ExclusionReason::PublicAvailabilityAfterCutoff
    }));
    assert!(
        result
            .event_days()
            .iter()
            .take(3)
            .all(|event_day| event_day.abnormal_return.is_some())
    );

    // Removing the Jan 5 close removes two estimation returns; nothing is interpolated.
    let mut sparse = build_fixture()?;
    sparse
        .subject_quotes
        .retain(|quote| quote.version.as_str() != "subject-close-5");
    let result = run_event_study(
        request(&sparse, BenchmarkModel::MarketModelOls, day(19))?,
        limits,
        || Ok(()),
    )?;
    assert_eq!(
        result.estimation(),
        EstimationOutcome::Insufficient {
            returns_available: 4,
            returns_required: 5,
        }
    );
    assert_eq!(result.car(), &CarOutcome::NotEstimated);

    // A split inside the span that is not yet public stays visible as a qualification.
    let mut late_split = build_fixture()?;
    for action in &mut late_split.split {
        action.availability.public_available_at_utc_ms = Some(day(20));
    }
    let result = run_event_study(
        request(&late_split, BenchmarkModel::MarketModelOls, day(19))?,
        limits,
        || Ok(()),
    )?;
    assert_eq!(
        result.qualifications(),
        &[Qualification::InadmissibleActionWithinSpan(
            SeriesRole::Subject
        )]
    );
    assert_eq!(
        result.subject().inadmissible_actions_within_span,
        vec![VersionId::new("split-v1")?]
    );

    // Awareness inside the Jan 11 session makes Jan 11 day 0 and is qualified.
    let mut intraday = build_fixture()?;
    intraday.anchor.awareness = RecordAvailability {
        public_available_at_utc_ms: Some(day(11) + 64_800_000),
        ingested_at_utc_ms: Some(day(11) + 65_000_000),
    };
    // Day 0 moves earlier, so the original estimation span now precedes the calendar.
    let mut intraday_request = request(&intraday, BenchmarkModel::MeanAdjusted, day(19))?;
    assert_eq!(
        run_event_study(intraday_request, limits, || Ok(())),
        Err(MarketError::StudySpanBeyondCalendar)
    );
    intraday_request.estimation_window = SessionWindow::new(-6, -2)?;
    intraday_request.min_estimation_returns = 4;
    let result = run_event_study(intraday_request, limits, || Ok(()))?;
    assert_eq!(result.day_zero_date(), date(11)?);
    assert_eq!(
        result.qualifications(),
        &[Qualification::AwarenessWithinDayZeroSession]
    );

    // An awareness record published after the cutoff cannot anchor a study.
    assert_eq!(
        run_event_study(
            request(&fixture, BenchmarkModel::MeanAdjusted, day(11) + 70_000_000)?,
            limits,
            || Ok(()),
        ),
        Err(MarketError::AnchorExcluded(
            ExclusionReason::PublicAvailabilityAfterCutoff
        ))
    );

    // An unrecorded date inside the span could be an unlisted session.
    let mut unknown = build_fixture()?;
    let mut input = calendar_input(&unknown.venue)?;
    input
        .closures
        .retain(|closure| closure.kind != ClosureKind::Holiday);
    unknown.calendar = TradingCalendarVersion::new(input, MarketLimits::new(64)?)?;
    assert_eq!(
        run_event_study(
            request(&unknown, BenchmarkModel::MeanAdjusted, day(19))?,
            limits,
            || Ok(()),
        ),
        Err(MarketError::CalendarCoverageUnknown(date(15)?))
    );

    // A calendar for another venue cannot define this series' sessions.
    let mut foreign = build_fixture()?;
    let foreign_venue = OpaqueId::new("venue:unrelated")?;
    foreign.calendar =
        TradingCalendarVersion::new(calendar_input(&foreign_venue)?, MarketLimits::new(64)?)?;
    assert!(matches!(
        run_event_study(
            request(&foreign, BenchmarkModel::MarketModelOls, day(19))?,
            limits,
            || Ok(()),
        ),
        Err(MarketError::ScopeMismatch(_))
    ));
    let mut benchmark_elsewhere = request(&fixture, BenchmarkModel::MarketModelOls, day(19))?;
    if let Some(benchmark) = benchmark_elsewhere.benchmark.as_mut() {
        benchmark.venue = &foreign_venue;
    }
    assert!(matches!(
        run_event_study(benchmark_elsewhere, limits, || Ok(())),
        Err(MarketError::ScopeMismatch(_))
    ));

    // A supplied benchmark is not silently ignored, and cancellation returns no result.
    let mut ignored = request(&fixture, BenchmarkModel::MarketAdjusted, day(19))?;
    ignored.model = BenchmarkModel::MeanAdjusted;
    assert!(matches!(
        run_event_study(ignored, limits, || Ok(())),
        Err(MarketError::InvalidEventStudy(_))
    ));
    let mut calls = 0_usize;
    assert_eq!(
        run_event_study(
            request(&fixture, BenchmarkModel::MarketModelOls, day(19))?,
            limits,
            || {
                calls += 1;
                if calls > 40 {
                    Err(StopReason::Cancelled)
                } else {
                    Ok(())
                }
            },
        ),
        Err(MarketError::Stopped(StopReason::Cancelled))
    );
    Ok(())
}
