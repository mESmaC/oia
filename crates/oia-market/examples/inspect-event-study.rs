//! Synthetic daily event study. All instruments, prices, calendars, and events are
//! generated in memory; output is not observed market evidence or a provider result.

use std::error::Error;

use oia_domain::{OpaqueId, VersionId};
use oia_market::MarketLimits;
use oia_market::calendar::{
    ClosureKind, SessionKind, TradingCalendarInput, TradingCalendarVersion, TradingClosure,
    TradingSession,
};
use oia_market::event_study::{
    BenchmarkModel, CarOutcome, CloseStatus, EstimationOutcome, EventStudyRequest,
    MarketAwarenessAnchor, PriceSeries, SessionWindow, run_event_study,
};
use oia_market::model::{
    CalendarDate, CorporateActionRecord, CutoffMode, FixedDecimal, Frequency, PriceAdjustmentBasis,
    QuoteDelay, QuoteRecord, RecordAvailability, Unit,
};

const JAN_1_2024_UTC_MS: i64 = 1_704_067_200_000;
const DAY_MS: i64 = 86_400_000;
const OPEN_MS: i64 = 52_200_000;
const CLOSE_MS: i64 = 75_600_000;
const SESSION_DAYS: [u8; 11] = [2, 3, 4, 5, 8, 9, 10, 11, 12, 16, 17];

fn day(day: u8) -> i64 {
    JAN_1_2024_UTC_MS + (i64::from(day) - 1) * DAY_MS
}

fn closes(
    instrument: &OpaqueId,
    venue: &OpaqueId,
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
            venue: venue.clone(),
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

fn main() -> Result<(), Box<dyn Error>> {
    let venue = OpaqueId::new("venue:synthetic")?;
    let source = OpaqueId::new("source:synthetic-eod")?;
    let unit = Unit::new("USD")?;
    let subject_id = OpaqueId::new("instrument:synthetic-subject")?;
    let benchmark_id = OpaqueId::new("instrument:synthetic-index")?;

    let mut sessions = Vec::new();
    for session_day in SESSION_DAYS {
        sessions.push(TradingSession {
            local_date: CalendarDate::new(2024, 1, session_day)?,
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
            local_date: CalendarDate::new(2024, 1, closure_day)?,
            kind,
            reason: "synthetic closure".into(),
        });
    }
    let calendar = TradingCalendarVersion::new(
        TradingCalendarInput {
            venue: venue.clone(),
            version: VersionId::new("synthetic-calendar-v1")?,
            source_version: VersionId::new("synthetic-calendar-source-v1")?,
            timezone_identifier: "America/New_York".into(),
            coverage_start: CalendarDate::new(2024, 1, 2)?,
            coverage_end: CalendarDate::new(2024, 1, 17)?,
            sessions,
            closures,
        },
        MarketLimits::new(64)?,
    )?;

    let subject_quotes = closes(
        &subject_id,
        &venue,
        &source,
        &unit,
        "subject",
        &[
            10_000, 10_100, 10_080, 10_200, 10_150, 10_220, 10_300, 10_350, 11_000, 5_600, 5_550,
        ],
    )?;
    let benchmark_quotes = closes(
        &benchmark_id,
        &venue,
        &source,
        &unit,
        "index",
        &[
            400_000, 402_000, 401_000, 404_000, 403_000, 405_000, 406_000, 407_000, 408_000,
            410_000, 409_000,
        ],
    )?;
    let split = [CorporateActionRecord {
        instrument_id: subject_id.clone(),
        action_id: OpaqueId::new("action:synthetic-split")?,
        version: VersionId::new("synthetic-split-v1")?,
        source_version: VersionId::new("synthetic-split-source-v1")?,
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
    let anchor = MarketAwarenessAnchor {
        event_id: OpaqueId::new("event:synthetic")?,
        awareness_record_version: VersionId::new("synthetic-disclosure-v1")?,
        awareness_source_version: VersionId::new("synthetic-disclosure-source-v1")?,
        awareness: RecordAvailability {
            public_available_at_utc_ms: Some(day(11) + 79_200_000),
            ingested_at_utc_ms: Some(day(11) + 79_500_000),
        },
        event_occurred_at_utc_ms: Some(day(11) + 64_800_000),
    };
    let mode = CutoffMode::ProspectiveStrict;
    let result = run_event_study(
        EventStudyRequest {
            calendar: &calendar,
            anchor: &anchor,
            subject: PriceSeries {
                instrument_id: &subject_id,
                venue: &venue,
                source_id: &source,
                unit: &unit,
                quotes: &subject_quotes,
                actions: &split,
            },
            benchmark: Some(PriceSeries {
                instrument_id: &benchmark_id,
                venue: &venue,
                source_id: &source,
                unit: &unit,
                quotes: &benchmark_quotes,
                actions: &[],
            }),
            model: BenchmarkModel::MarketModelOls,
            basis: PriceAdjustmentBasis::SplitAdjusted,
            estimation_window: SessionWindow::new(-7, -2)?,
            event_window: SessionWindow::new(-1, 2)?,
            min_estimation_returns: 5,
            cutoff_utc_ms: day(18),
            mode: &mode,
        },
        MarketLimits::new(256)?,
        || Ok(()),
    )?;

    println!("SYNTHETIC event study (not observed market evidence)");
    println!(
        "event {} | awareness record {} public at {} ms | occurred at {:?} ms",
        result.event_id(),
        result.awareness_record_version(),
        result.market_aware_at_utc_ms(),
        result.event_occurred_at_utc_ms()
    );
    println!(
        "day 0 {} | calendar {} | cutoff {} ms {:?}",
        result.day_zero_date(),
        result.calendar_version(),
        result.cutoff_utc_ms(),
        result.mode()
    );
    println!(
        "methods: returns {} | model {} | aggregation {} | standardized {} | prices {}",
        result.return_method_id(),
        result.model_method_id(),
        result.aggregation_method_id(),
        result.standardized_car_method_id(),
        result.subject().adjustment_method_id
    );
    println!(
        "windows: estimation {}..={} (min {} returns), event {}..={}",
        result.estimation_window().start_offset(),
        result.estimation_window().end_offset(),
        result.min_estimation_returns(),
        result.event_window().start_offset(),
        result.event_window().end_offset()
    );
    for close in &result.subject().closes {
        match &close.status {
            CloseStatus::Present {
                quote_version,
                adjusted_price,
                applied_actions,
                ..
            } => println!(
                "  close {:>3} {} {} adjusted {} actions {:?}",
                close.offset,
                close.local_date,
                quote_version,
                adjusted_price,
                applied_actions
                    .iter()
                    .map(VersionId::as_str)
                    .collect::<Vec<_>>()
            ),
            CloseStatus::Missing(reason) => {
                println!(
                    "  close {:>3} {} missing {reason:?}",
                    close.offset, close.local_date
                );
            }
        }
    }
    match result.estimation() {
        EstimationOutcome::Fitted {
            returns_used,
            parameters,
            residual_std_dev,
            degrees_of_freedom,
        } => println!(
            "estimation: {returns_used} returns, {parameters:?}, residual sd {residual_std_dev:.6} (df {degrees_of_freedom})"
        ),
        other => println!("estimation: {other:?}"),
    }
    for event_day in result.event_days() {
        println!(
            "  event {:>2} {} return {:?} expected {:?} abnormal {:?}",
            event_day.offset,
            event_day.local_date,
            event_day.subject_return.map(|value| value.to_string()),
            event_day.expected_return,
            event_day.abnormal_return
        );
    }
    match result.car() {
        CarOutcome::Complete { car, standardized } => {
            println!("CAR {car:.6} standardized {standardized:?}");
        }
        other => println!("CAR unavailable: {other:?}"),
    }
    println!(
        "quote exclusions: {:?}",
        result
            .subject()
            .quote_exclusions
            .iter()
            .map(|excluded| (excluded.version.as_str(), excluded.reason))
            .collect::<Vec<_>>()
    );
    println!(
        "qualifications {:?} | interpretation {:?}",
        result.qualifications(),
        result.interpretation()
    );
    Ok(())
}
