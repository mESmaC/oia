//! Synthetic A14 data-condition demonstration; not observed market evidence.

use std::error::Error;

use oia_domain::{OpaqueId, VersionId};
use oia_market::MarketLimits;
use oia_market::calendar::{
    ClosureKind, SessionAt, SessionKind, TradingCalendarInput, TradingCalendarVersion,
    TradingClosure, TradingSession,
};
use oia_market::model::{
    CalendarDate, CorporateActionRecord, CutoffMode, DateInterval, FixedDecimal, Frequency,
    Instrument, InstrumentClass, ListingAlias, PriceAdjustmentBasis, QuoteDelay, QuoteRecord,
    RecordAvailability, ReleasePayload, ReleaseVintage, Unit, resolve_instrument,
};
use oia_market::point_in_time::{
    QuoteQuery, adjust_quote, select_quote_as_of, select_release_vintage,
};

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

fn main() -> Result<(), Box<dyn Error>> {
    println!("SYNTHETIC A14 FIXTURE - NOT OBSERVED MARKET EVIDENCE");
    let limits = MarketLimits::new(32)?;
    let instrument_id = id("instrument:example")?;
    let venue = id("venue:example")?;
    let source = id("source:synthetic")?;
    let usd = Unit::new("USD")?;
    let instrument = Instrument {
        id: instrument_id.clone(),
        version: version("instrument-v1")?,
        class: InstrumentClass::Equity,
        venue: venue.clone(),
        quote_unit: usd.clone(),
        aliases: vec![
            ListingAlias {
                symbol: "OLD".into(),
                valid_from: date(2024, 1, 1)?,
                valid_to_exclusive: Some(date(2024, 2, 1)?),
                source_version: version("listing-old-v1")?,
            },
            ListingAlias {
                symbol: "NEW".into(),
                valid_from: date(2024, 2, 1)?,
                valid_to_exclusive: None,
                source_version: version("listing-new-v1")?,
            },
        ],
    };
    let before_rename = resolve_instrument(
        std::slice::from_ref(&instrument),
        &venue,
        "OLD",
        date(2024, 1, 15)?,
        limits,
        || Ok(()),
    )?;
    let after_rename = resolve_instrument(
        std::slice::from_ref(&instrument),
        &venue,
        "NEW",
        date(2024, 2, 1)?,
        limits,
        || Ok(()),
    )?;
    println!(
        "rename: OLD and NEW resolve to same stable ID = {}",
        before_rename.id == after_rename.id
    );

    let jan15 = date(2024, 1, 15)?;
    let jan16 = date(2024, 1, 16)?;
    let jan17 = date(2024, 1, 17)?;
    let t0 = 1_705_415_400_000_i64;
    let calendar = TradingCalendarVersion::new(
        TradingCalendarInput {
            venue: venue.clone(),
            version: version("calendar-v1")?,
            source_version: version("calendar-source-v1")?,
            timezone_identifier: "America/New_York".into(),
            coverage_start: jan15,
            coverage_end: jan17,
            sessions: vec![
                TradingSession {
                    local_date: jan16,
                    open_at_utc_ms: t0,
                    close_at_utc_ms: t0 + 23_400_000,
                    kind: SessionKind::Ordinary,
                },
                TradingSession {
                    local_date: jan17,
                    open_at_utc_ms: t0 + 86_400_000,
                    close_at_utc_ms: t0 + 99_000_000,
                    kind: SessionKind::EarlyClose,
                },
            ],
            closures: vec![TradingClosure {
                local_date: jan15,
                kind: ClosureKind::Holiday,
                reason: "Synthetic holiday closure".into(),
            }],
        },
        limits,
    )?;
    println!("holiday: {:?}", calendar.calendar_day(jan15));
    println!(
        "exact early close is open = {}",
        matches!(
            calendar.session_at(jan17, t0 + 99_000_000)?,
            SessionAt::Open(_)
        )
    );
    println!(
        "missing calendar date: {:?}",
        calendar.calendar_day(date(2024, 1, 18)?)
    );

    let quote = QuoteRecord {
        instrument_id: instrument_id.clone(),
        venue: venue.clone(),
        source_id: source.clone(),
        version: version("quote-normalized-v1")?,
        source_version: version("quote-source-v1")?,
        observed_at_utc_ms: t0,
        availability: available(t0 + 900_000, t0 + 901_000),
        price: FixedDecimal::new(10_000, 2)?,
        unit: usd.clone(),
        frequency: Frequency::Minutely,
        delay: QuoteDelay::Delayed {
            declared_delay_ms: Some(900_000),
        },
    };
    let before_publication = select_quote_as_of(
        std::slice::from_ref(&quote),
        QuoteQuery {
            instrument_id: &instrument_id,
            venue: &venue,
            source_id: &source,
            unit: &usd,
            cutoff_utc_ms: t0 + 1_000,
            mode: &CutoffMode::ProspectiveStrict,
        },
        limits,
        || Ok(()),
    )?;
    println!(
        "delayed quote before public availability: selected={:?}, exclusions={:?}",
        before_publication.selected().map(|q| &q.version),
        before_publication.exclusions()
    );
    let after = t0 + 2_000_000;
    let after_publication = select_quote_as_of(
        std::slice::from_ref(&quote),
        QuoteQuery {
            instrument_id: &instrument_id,
            venue: &venue,
            source_id: &source,
            unit: &usd,
            cutoff_utc_ms: after,
            mode: &CutoffMode::ProspectiveStrict,
        },
        limits,
        || Ok(()),
    )?;
    println!(
        "delayed quote after availability: selected={:?}",
        after_publication.selected().map(|q| &q.version)
    );

    let split = CorporateActionRecord {
        instrument_id: instrument_id.clone(),
        version: version("split-v1")?,
        source_version: version("split-source-v1")?,
        effective_at_utc_ms: t0 + 1_000_000,
        availability: available(t0 + 1_000_001, t0 + 1_000_002),
        split_numerator: 2,
        split_denominator: 1,
    };
    let future_split = CorporateActionRecord {
        version: version("split-future-v1")?,
        source_version: version("split-future-source-v1")?,
        effective_at_utc_ms: after + 1_000_000,
        availability: available(after + 1_000_001, after + 1_000_002),
        ..split.clone()
    };
    let adjusted = adjust_quote(
        &quote,
        &[split, future_split],
        PriceAdjustmentBasis::SplitAdjusted,
        after,
        &CutoffMode::ProspectiveStrict,
        limits,
        || Ok(()),
    )?;
    println!(
        "split adjustment: original={:?}, adjusted={}, method={}, applied={:?}, excluded={:?}",
        adjusted.original().version,
        adjusted.adjusted_price(),
        adjusted.method_id(),
        adjusted.applied_actions(),
        adjusted.excluded_actions()
    );

    let reporting_period = DateInterval::new(date(2024, 1, 1)?, date(2024, 2, 1)?)?;
    let release_v1 = ReleaseVintage {
        series_id: id("series:synthetic-cpi")?,
        source_id: source.clone(),
        version: version("release-v1")?,
        source_version: version("release-source-v1")?,
        revision: 1,
        predecessor_version: None,
        reporting_period,
        event_at_utc_ms: Some(after + 100),
        availability: available(after + 1_000, after + 1_001),
        payload: ReleasePayload::Numeric {
            value: FixedDecimal::new(30, 1)?,
            unit: Unit::new("%")?,
        },
    };
    let release_v2 = ReleaseVintage {
        version: version("release-v2")?,
        source_version: version("release-source-v2")?,
        revision: 2,
        predecessor_version: Some(version("release-v1")?),
        availability: available(after + 2_000, after + 2_001),
        payload: ReleasePayload::Numeric {
            value: FixedDecimal::new(31, 1)?,
            unit: Unit::new("%")?,
        },
        ..release_v1.clone()
    };
    let releases = vec![release_v2, release_v1];
    let series = id("series:synthetic-cpi")?;
    for cutoff in [after + 1_500, after + 3_000] {
        let selected = select_release_vintage(
            &releases,
            &series,
            cutoff,
            &CutoffMode::ProspectiveStrict,
            limits,
            || Ok(()),
        )?;
        println!(
            "release cutoff {cutoff}: selected={:?}, exclusions={:?}",
            selected.selected().map(|v| &v.version),
            selected.exclusions()
        );
    }

    let late_news = ReleaseVintage {
        series_id: id("series:late-news")?,
        source_id: source.clone(),
        version: version("news-normalized-v1")?,
        source_version: version("news-source-v1")?,
        revision: 1,
        predecessor_version: None,
        reporting_period,
        event_at_utc_ms: Some(after + 100),
        availability: available(after + 5_000, after + 5_001),
        payload: ReleasePayload::Disclosure {
            document_version: version("news-document-v1")?,
            linked_entities: vec![instrument_id],
        },
    };
    let news = select_release_vintage(
        std::slice::from_ref(&late_news),
        &late_news.series_id,
        after + 1_500,
        &CutoffMode::ProspectiveStrict,
        limits,
        || Ok(()),
    )?;
    println!(
        "post-event news published after cutoff: selected={:?}, exclusions={:?}",
        news.selected().map(|v| &v.version),
        news.exclusions()
    );
    println!("Synthetic inputs were not written or sent to a provider.");
    Ok(())
}
