//! Synthetic contract checks for leakage, exact arithmetic, and honest coverage.

use std::error::Error;

use oia_domain::{OpaqueId, VersionId};
use oia_market::calendar::{
    CalendarDay, ClosureKind, SessionAt, SessionKind, TradingCalendarInput, TradingCalendarVersion,
    TradingClosure, TradingSession,
};
use oia_market::model::{
    CalendarDate, CorporateActionRecord, CutoffMode, DateInterval, ExactRational, ExclusionReason,
    FixedDecimal, Frequency, Instrument, InstrumentClass, ListingAlias, PriceAdjustmentBasis,
    QuoteDelay, QuoteRecord, RecordAvailability, ReleasePayload, ReleaseVintage, Unit,
    resolve_instrument,
};
use oia_market::point_in_time::{
    QuoteQuery, SPLIT_ADJUSTED_METHOD_ID, adjust_quote, select_quote_as_of, select_release_vintage,
};
use oia_market::{MarketError, MarketLimits, StopReason};

fn id(value: &str) -> Result<OpaqueId, Box<dyn Error>> {
    Ok(OpaqueId::new(value)?)
}

fn version(value: &str) -> Result<VersionId, Box<dyn Error>> {
    Ok(VersionId::new(value)?)
}

fn date(year: u16, month: u8, day: u8) -> Result<CalendarDate, Box<dyn Error>> {
    Ok(CalendarDate::new(year, month, day)?)
}

fn available(public: i64, ingested: i64) -> RecordAvailability {
    RecordAvailability {
        public_available_at_utc_ms: Some(public),
        ingested_at_utc_ms: Some(ingested),
    }
}

fn instrument() -> Result<Instrument, Box<dyn Error>> {
    Ok(Instrument {
        id: id("instrument:stable")?,
        version: version("instrument-v1")?,
        class: InstrumentClass::Equity,
        venue: id("venue:test")?,
        quote_unit: Unit::new("USD")?,
        aliases: vec![
            ListingAlias {
                symbol: "OLD".into(),
                valid_from: date(2024, 1, 1)?,
                valid_to_exclusive: Some(date(2024, 2, 1)?),
                source_version: version("listing-old")?,
            },
            ListingAlias {
                symbol: "NEW".into(),
                valid_from: date(2024, 2, 1)?,
                valid_to_exclusive: None,
                source_version: version("listing-new")?,
            },
        ],
    })
}

fn make_quote(observed: i64) -> Result<QuoteRecord, Box<dyn Error>> {
    Ok(QuoteRecord {
        instrument_id: id("instrument:stable")?,
        venue: id("venue:test")?,
        source_id: id("source:test")?,
        version: version("quote-v1")?,
        source_version: version("quote-source-v1")?,
        observed_at_utc_ms: observed,
        availability: available(observed + 100, observed + 200),
        price: FixedDecimal::new(10_000, 2)?,
        unit: Unit::new("USD")?,
        frequency: Frequency::Minutely,
        delay: QuoteDelay::Delayed {
            declared_delay_ms: Some(100),
        },
    })
}

fn query<'a>(
    instrument: &'a Instrument,
    source: &'a OpaqueId,
    unit: &'a Unit,
    cutoff: i64,
    mode: &'a CutoffMode,
) -> Result<QuoteQuery<'a>, Box<dyn Error>> {
    Ok(QuoteQuery {
        instrument_id: &instrument.id,
        venue: &instrument.venue,
        source_id: source,
        unit,
        cutoff_utc_ms: cutoff,
        mode,
    })
}

#[test]
fn identity_calendar_and_delayed_quote_cutoffs() -> Result<(), Box<dyn Error>> {
    assert!(CalendarDate::new(2024, 2, 29).is_ok());
    assert!(CalendarDate::new(2023, 2, 29).is_err());
    let instrument = instrument()?;
    let limits = MarketLimits::new(16)?;
    for (symbol, day) in [("OLD", 15), ("NEW", 1)] {
        let date = if symbol == "OLD" {
            date(2024, 1, day)?
        } else {
            date(2024, 2, day)?
        };
        let resolved = resolve_instrument(
            std::slice::from_ref(&instrument),
            &instrument.venue,
            symbol,
            date,
            limits,
            || Ok(()),
        )?;
        assert_eq!(resolved.id, instrument.id);
    }
    assert_eq!(
        resolve_instrument(
            std::slice::from_ref(&instrument),
            &instrument.venue,
            "OLD",
            date(2024, 2, 1)?,
            limits,
            || Ok(()),
        ),
        Err(MarketError::AliasNotFound)
    );
    let mut ambiguous = instrument.clone();
    ambiguous.id = id("instrument:other")?;
    ambiguous.version = version("instrument-other-v1")?;
    assert!(matches!(
        resolve_instrument(
            &[instrument.clone(), ambiguous],
            &instrument.venue,
            "OLD",
            date(2024, 1, 15)?,
            limits,
            || Ok(()),
        ),
        Err(MarketError::InvalidInstrument(_))
    ));

    let jan15 = date(2024, 1, 15)?;
    let jan16 = date(2024, 1, 16)?;
    let jan17 = date(2024, 1, 17)?;
    let jan18 = date(2024, 1, 18)?;
    let calendar = TradingCalendarVersion::new(
        TradingCalendarInput {
            venue: instrument.venue.clone(),
            version: version("calendar-v1")?,
            source_version: version("calendar-source-v1")?,
            timezone_identifier: "America/New_York".into(),
            coverage_start: jan15,
            coverage_end: jan18,
            sessions: vec![
                TradingSession {
                    local_date: jan16,
                    open_at_utc_ms: 1_000,
                    close_at_utc_ms: 2_000,
                    kind: SessionKind::Ordinary,
                },
                TradingSession {
                    local_date: jan17,
                    open_at_utc_ms: 3_000,
                    close_at_utc_ms: 4_000,
                    kind: SessionKind::EarlyClose,
                },
            ],
            closures: vec![TradingClosure {
                local_date: jan15,
                kind: ClosureKind::Holiday,
                reason: "Synthetic holiday".into(),
            }],
        },
        limits,
    )?;
    assert!(matches!(
        calendar.calendar_day(jan15),
        CalendarDay::Closed(closure) if closure.kind == ClosureKind::Holiday
    ));
    assert!(matches!(
        calendar.calendar_day(jan17),
        CalendarDay::Open(session) if session.kind == SessionKind::EarlyClose
    ));
    assert_eq!(calendar.calendar_day(jan18), CalendarDay::Unknown);
    assert_eq!(
        calendar.session_at(jan17, 3_999)?,
        SessionAt::Open(&calendar.sessions()[1])
    );
    assert_eq!(calendar.session_at(jan17, 4_000)?, SessionAt::Closed);
    assert_eq!(calendar.session_at(jan18, 4_000)?, SessionAt::Unknown);

    let quote = make_quote(1_000)?;
    let source = id("source:test")?;
    let unit = Unit::new("USD")?;
    let strict = CutoffMode::ProspectiveStrict;
    let early = select_quote_as_of(
        std::slice::from_ref(&quote),
        query(&instrument, &source, &unit, 1_050, &strict)?,
        limits,
        || Ok(()),
    )?;
    assert!(early.selected().is_none());
    assert_eq!(early.cutoff_utc_ms(), 1_050);
    assert_eq!(early.eligible_count(), 0);
    assert_eq!(
        early.exclusions()[0].reason,
        ExclusionReason::PublicAvailabilityAfterCutoff
    );
    let late_ingestion = select_quote_as_of(
        std::slice::from_ref(&quote),
        query(&instrument, &source, &unit, 1_150, &strict)?,
        limits,
        || Ok(()),
    )?;
    assert_eq!(
        late_ingestion.exclusions()[0].reason,
        ExclusionReason::IngestedAfterStrictCutoff
    );
    let retrospective = CutoffMode::RetrospectiveReconstructed {
        limitation_note: "Externally documented availability; later OIA ingestion.".into(),
    };
    let reconstructed = select_quote_as_of(
        std::slice::from_ref(&quote),
        query(&instrument, &source, &unit, 1_150, &retrospective)?,
        limits,
        || Ok(()),
    )?;
    assert_eq!(reconstructed.selected(), Some(&quote));
    assert_eq!(reconstructed.mode(), &retrospective);
    let complete = select_quote_as_of(
        std::slice::from_ref(&quote),
        query(&instrument, &source, &unit, 1_200, &strict)?,
        limits,
        || Ok(()),
    )?;
    assert_eq!(complete.selected(), Some(&quote));
    assert_eq!(complete.eligible_count(), 1);
    let empty = select_quote_as_of(
        &[],
        query(&instrument, &source, &unit, 1_200, &strict)?,
        limits,
        || Ok(()),
    )?;
    assert_eq!(empty.supplied_count(), 0);
    assert!(empty.selected().is_none());
    assert!(empty.exclusions().is_empty());
    let mut unknown = quote.clone();
    unknown.version = version("quote-unknown-availability")?;
    unknown.availability.public_available_at_utc_ms = None;
    let unknown_inputs = [unknown];
    let excluded = select_quote_as_of(
        &unknown_inputs,
        query(&instrument, &source, &unit, 2_000, &strict)?,
        limits,
        || Ok(()),
    )?;
    assert_eq!(
        excluded.exclusions()[0].reason,
        ExclusionReason::PublicAvailabilityUnknown
    );
    let mut impossible_quote = quote.clone();
    impossible_quote.version = version("quote-invalid-publication")?;
    impossible_quote.availability.public_available_at_utc_ms = Some(900);
    assert!(matches!(
        select_quote_as_of(
            &[impossible_quote],
            query(&instrument, &source, &unit, 2_000, &strict)?,
            limits,
            || Ok(()),
        ),
        Err(MarketError::InvalidQuote(_))
    ));
    let euro = Unit::new("EUR")?;
    assert!(matches!(
        select_quote_as_of(
            std::slice::from_ref(&quote),
            query(&instrument, &source, &euro, 2_000, &strict)?,
            limits,
            || Ok(()),
        ),
        Err(MarketError::ScopeMismatch("quote unit"))
    ));
    Ok(())
}

#[test]
fn release_vintages_exclude_late_or_unknown_information() -> Result<(), Box<dyn Error>> {
    let limits = MarketLimits::new(16)?;
    let series = id("series:economic")?;
    let period = DateInterval::new(date(2024, 1, 1)?, date(2024, 2, 1)?)?;
    let v1 = ReleaseVintage {
        series_id: series.clone(),
        source_id: id("source:economic")?,
        version: version("release-v1")?,
        source_version: version("release-source-v1")?,
        revision: 1,
        predecessor_version: None,
        reporting_period: period,
        event_at_utc_ms: Some(10_000),
        availability: available(11_000, 11_000),
        payload: ReleasePayload::Numeric {
            value: FixedDecimal::new(30, 1)?,
            unit: Unit::new("%")?,
        },
    };
    let v2 = ReleaseVintage {
        version: version("release-v2")?,
        source_version: version("release-source-v2")?,
        revision: 2,
        predecessor_version: Some(version("release-v1")?),
        availability: available(12_000, 12_000),
        payload: ReleasePayload::Numeric {
            value: FixedDecimal::new(31, 1)?,
            unit: Unit::new("%")?,
        },
        ..v1.clone()
    };
    let releases = vec![v2.clone(), v1.clone()];
    let strict = CutoffMode::ProspectiveStrict;
    let before_revision =
        select_release_vintage(&releases, &series, 11_500, &strict, limits, || Ok(()))?;
    assert_eq!(before_revision.selected(), Some(&v1));
    assert_eq!(before_revision.cutoff_utc_ms(), 11_500);
    assert_eq!(
        before_revision.exclusions()[0].reason,
        ExclusionReason::PublicAvailabilityAfterCutoff
    );
    let after_revision =
        select_release_vintage(&releases, &series, 12_500, &strict, limits, || Ok(()))?;
    assert_eq!(after_revision.selected(), Some(&v2));
    assert_eq!(after_revision.eligible_count(), 2);

    let mut unknown_v2 = v2.clone();
    unknown_v2.availability.public_available_at_utc_ms = None;
    let unknown_inputs = [unknown_v2, v1.clone()];
    let unknown =
        select_release_vintage(&unknown_inputs, &series, 12_500, &strict, limits, || Ok(()))?;
    assert_eq!(unknown.selected(), Some(&v1));
    assert_eq!(
        unknown.exclusions()[0].reason,
        ExclusionReason::PublicAvailabilityUnknown
    );

    let mut late_ingested_v1 = v1.clone();
    late_ingested_v1.availability.ingested_at_utc_ms = Some(13_000);
    let late_inputs = vec![v2.clone(), late_ingested_v1.clone()];
    let prospective =
        select_release_vintage(&late_inputs, &series, 11_500, &strict, limits, || Ok(()))?;
    assert!(prospective.selected().is_none());
    assert_eq!(
        prospective.exclusions()[1].reason,
        ExclusionReason::IngestedAfterStrictCutoff
    );
    let retrospective = CutoffMode::RetrospectiveReconstructed {
        limitation_note: "Historical public timestamps are documented; OIA ingestion was later."
            .into(),
    };
    let reconstructed = select_release_vintage(
        &late_inputs,
        &series,
        11_500,
        &retrospective,
        limits,
        || Ok(()),
    )?;
    assert_eq!(reconstructed.selected(), Some(&late_ingested_v1));
    assert_eq!(reconstructed.mode(), &retrospective);

    let late_news = ReleaseVintage {
        series_id: id("series:news")?,
        source_id: id("source:news")?,
        version: version("news-v1")?,
        source_version: version("news-source-v1")?,
        revision: 1,
        predecessor_version: None,
        reporting_period: period,
        event_at_utc_ms: Some(10_000),
        availability: available(20_000, 20_000),
        payload: ReleasePayload::Disclosure {
            document_version: version("news-document-v1")?,
            linked_entities: vec![id("instrument:stable")?],
        },
    };
    let news = select_release_vintage(
        std::slice::from_ref(&late_news),
        &late_news.series_id,
        11_500,
        &strict,
        limits,
        || Ok(()),
    )?;
    assert!(news.selected().is_none());
    assert_eq!(
        news.exclusions()[0].reason,
        ExclusionReason::PublicAvailabilityAfterCutoff
    );
    let mut scheduled_news = late_news.clone();
    scheduled_news.event_at_utc_ms = Some(20_000);
    scheduled_news.availability = available(10_000, 10_000);
    let scheduled = select_release_vintage(
        std::slice::from_ref(&scheduled_news),
        &scheduled_news.series_id,
        11_500,
        &strict,
        limits,
        || Ok(()),
    )?;
    assert_eq!(scheduled.selected(), Some(&scheduled_news));

    let mut duplicate_revision = v2.clone();
    duplicate_revision.revision = 1;
    assert!(matches!(
        select_release_vintage(
            &[v1.clone(), duplicate_revision],
            &series,
            12_500,
            &strict,
            limits,
            || Ok(()),
        ),
        Err(MarketError::DuplicateRevision(1))
    ));
    let mut wrong_predecessor = v2.clone();
    wrong_predecessor.predecessor_version = Some(version("not-release-v1")?);
    assert!(matches!(
        select_release_vintage(
            &[v1.clone(), wrong_predecessor],
            &series,
            12_500,
            &strict,
            limits,
            || Ok(()),
        ),
        Err(MarketError::InvalidRelease(_))
    ));
    let mut wrong_unit = v2.clone();
    wrong_unit.payload = ReleasePayload::Numeric {
        value: FixedDecimal::new(31, 1)?,
        unit: Unit::new("USD")?,
    };
    assert!(matches!(
        select_release_vintage(
            &[v1.clone(), wrong_unit],
            &series,
            12_500,
            &strict,
            limits,
            || Ok(()),
        ),
        Err(MarketError::UnitMismatch)
    ));
    let mut wrong_period = v2.clone();
    wrong_period.reporting_period = DateInterval::new(date(2024, 2, 1)?, date(2024, 3, 1)?)?;
    assert!(matches!(
        select_release_vintage(
            &[v1.clone(), wrong_period],
            &series,
            12_500,
            &strict,
            limits,
            || Ok(()),
        ),
        Err(MarketError::InvalidRelease(_))
    ));
    let mut wrong_source = v2.clone();
    wrong_source.source_id = id("source:different")?;
    assert!(matches!(
        select_release_vintage(
            &[v1.clone(), wrong_source],
            &series,
            12_500,
            &strict,
            limits,
            || Ok(()),
        ),
        Err(MarketError::InvalidRelease(_))
    ));
    let mut backward_v1 = v1.clone();
    backward_v1.availability.public_available_at_utc_ms = Some(13_000);
    assert!(matches!(
        select_release_vintage(&[backward_v1, v2], &series, 14_000, &strict, limits, || Ok(
            ()
        ),),
        Err(MarketError::InvalidRelease(_))
    ));
    Ok(())
}

fn action(
    name: &str,
    effective: i64,
    numerator: i128,
    denominator: i128,
) -> Result<CorporateActionRecord, Box<dyn Error>> {
    Ok(CorporateActionRecord {
        instrument_id: id("instrument:stable")?,
        version: version(name)?,
        source_version: version(&format!("{name}-source"))?,
        effective_at_utc_ms: effective,
        availability: available(effective, effective),
        split_numerator: numerator,
        split_denominator: denominator,
    })
}

#[test]
fn split_adjustment_is_exact_and_failure_paths_return_no_result() -> Result<(), Box<dyn Error>> {
    let limits = MarketLimits::new(16)?;
    let strict = CutoffMode::ProspectiveStrict;
    let quote = make_quote(10_000)?;
    let split = action("split-2-for-1", 15_000, 2, 1)?;
    let future = action("split-future", 30_000, 2, 1)?;
    let prior = action("split-prior", 5_000, 2, 1)?;
    let mut unknown = action("split-unknown", 16_000, 2, 1)?;
    unknown.availability.public_available_at_utc_ms = None;
    let adjusted = adjust_quote(
        &quote,
        &[
            future.clone(),
            unknown.clone(),
            split.clone(),
            prior.clone(),
        ],
        PriceAdjustmentBasis::SplitAdjusted,
        20_000,
        &strict,
        limits,
        || Ok(()),
    )?;
    assert_eq!(adjusted.adjusted_price(), ExactRational::new(50, 1)?);
    assert_eq!(adjusted.original(), &quote);
    assert_eq!(adjusted.original().price, FixedDecimal::new(10_000, 2)?);
    assert_eq!(adjusted.method_id(), SPLIT_ADJUSTED_METHOD_ID);
    assert_eq!(adjusted.cutoff_utc_ms(), 20_000);
    assert_eq!(adjusted.mode(), &strict);
    assert_eq!(adjusted.applied_actions(), &[version("split-2-for-1")?]);
    assert_eq!(adjusted.excluded_actions().len(), 3);
    let future_version = version("split-future")?;
    assert!(adjusted.excluded_actions().iter().any(|entry| {
        entry.version == future_version && entry.reason == ExclusionReason::EffectiveAfterCutoff
    }));
    assert!(
        adjusted
            .excluded_actions()
            .iter()
            .any(|entry| { entry.reason == ExclusionReason::PublicAvailabilityUnknown })
    );
    assert!(
        adjusted
            .excluded_actions()
            .iter()
            .any(|entry| { entry.reason == ExclusionReason::NotEffectiveForQuote })
    );

    let before_split = adjust_quote(
        &quote,
        std::slice::from_ref(&split),
        PriceAdjustmentBasis::SplitAdjusted,
        14_000,
        &strict,
        limits,
        || Ok(()),
    )?;
    assert_eq!(before_split.adjusted_price(), ExactRational::new(100, 1)?);
    assert!(before_split.applied_actions().is_empty());
    assert_eq!(
        before_split.excluded_actions()[0].reason,
        ExclusionReason::EffectiveAfterCutoff
    );

    let post_split_quote = make_quote(16_000)?;
    let post_split = adjust_quote(
        &post_split_quote,
        std::slice::from_ref(&split),
        PriceAdjustmentBasis::SplitAdjusted,
        20_000,
        &strict,
        limits,
        || Ok(()),
    )?;
    assert_eq!(post_split.adjusted_price(), ExactRational::new(100, 1)?);
    assert!(post_split.applied_actions().is_empty());
    assert_eq!(
        post_split.excluded_actions()[0].reason,
        ExclusionReason::NotEffectiveForQuote
    );

    assert!(matches!(
        adjust_quote(
            &quote,
            &[],
            PriceAdjustmentBasis::TotalReturn,
            20_000,
            &strict,
            limits,
            || Ok(()),
        ),
        Err(MarketError::UnsupportedAdjustment)
    ));
    let mut duplicate_split = split.clone();
    duplicate_split.version = version("split-duplicate-version")?;
    duplicate_split.source_version = version("split-duplicate-source")?;
    assert!(matches!(
        adjust_quote(
            &quote,
            &[split.clone(), duplicate_split],
            PriceAdjustmentBasis::SplitAdjusted,
            20_000,
            &strict,
            limits,
            || Ok(()),
        ),
        Err(MarketError::InvalidAction(_))
    ));
    let invalid = action("split-invalid", 15_000, 0, 1)?;
    assert!(matches!(
        adjust_quote(
            &quote,
            &[invalid],
            PriceAdjustmentBasis::SplitAdjusted,
            20_000,
            &strict,
            limits,
            || Ok(()),
        ),
        Err(MarketError::InvalidAction(_))
    ));
    let mut overflowing_quote = quote.clone();
    overflowing_quote.price = FixedDecimal::new(i128::MAX, 0)?;
    let reverse_split = action("reverse-split", 15_000, 1, 2)?;
    assert!(matches!(
        adjust_quote(
            &overflowing_quote,
            &[reverse_split],
            PriceAdjustmentBasis::SplitAdjusted,
            20_000,
            &strict,
            limits,
            || Ok(()),
        ),
        Err(MarketError::ArithmeticOverflow)
    ));
    assert!(matches!(
        adjust_quote(
            &quote,
            &[split.clone(), future],
            PriceAdjustmentBasis::SplitAdjusted,
            20_000,
            &strict,
            MarketLimits::new(1)?,
            || Ok(()),
        ),
        Err(MarketError::LimitExceeded { .. })
    ));
    assert!(matches!(
        adjust_quote(
            &quote,
            &[split.clone(), prior.clone()],
            PriceAdjustmentBasis::Unadjusted,
            20_000,
            &strict,
            MarketLimits::new(1)?,
            || Ok(()),
        ),
        Err(MarketError::LimitExceeded { .. })
    ));
    let retrospective = CutoffMode::RetrospectiveReconstructed {
        limitation_note: "Later OIA ingestion is disclosed for this reconstruction.".into(),
    };
    let retrospective_adjustment = adjust_quote(
        &quote,
        &[],
        PriceAdjustmentBasis::Unadjusted,
        10_150,
        &retrospective,
        limits,
        || Ok(()),
    )?;
    assert_eq!(retrospective_adjustment.mode(), &retrospective);
    assert_eq!(retrospective_adjustment.cutoff_utc_ms(), 10_150);
    assert_eq!(MarketLimits::new(0), Err(MarketError::ZeroLimit));
    assert_eq!(
        FixedDecimal::new(1, 19),
        Err(MarketError::InvalidDecimalScale(19))
    );
    assert_eq!(ExactRational::new(4, 8)?, ExactRational::new(1, 2)?);

    let instrument = instrument()?;
    let source = id("source:test")?;
    let unit = Unit::new("USD")?;
    assert!(matches!(
        select_quote_as_of(
            std::slice::from_ref(&quote),
            query(&instrument, &source, &unit, 20_000, &strict)?,
            limits,
            || Err(StopReason::Cancelled),
        ),
        Err(MarketError::Stopped(StopReason::Cancelled))
    ));
    let mut calls = 0;
    assert!(matches!(
        adjust_quote(
            &quote,
            &[],
            PriceAdjustmentBasis::Unadjusted,
            20_000,
            &strict,
            limits,
            || {
                calls += 1;
                if calls == 2 {
                    Err(StopReason::DeadlineExceeded)
                } else {
                    Ok(())
                }
            },
        ),
        Err(MarketError::Stopped(StopReason::DeadlineExceeded))
    ));
    Ok(())
}
