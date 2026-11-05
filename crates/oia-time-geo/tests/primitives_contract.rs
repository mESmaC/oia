use oia_domain::evidence::UnknownReason;
use oia_time_geo::{
    Altitude, AltitudeBand, AltitudeReference, CalendarDate, ClockAlignment, ClockSyncState,
    DstAmbiguity, EARTH_RADIUS_METERS, GeoBoundingBoxWgs84, GeoPointWgs84, GeoPosition,
    LeapSecondTable, LocalTimeContext, TT_MINUS_TAI_MS, TimeGeoError, TimeRepresentation,
    TimeWindow, parse_rfc3339_utc_nanos, parse_rfc3339_utc_seconds,
};

/// A04: seam aliases, world coverage and poles must not cause false missing coverage.
#[test]
fn geographic_extent_boundaries() -> Result<(), TimeGeoError> {
    let world = GeoBoundingBoxWgs84::new(-90.0, 90.0, -180.0, 180.0)?;
    let wrapped = GeoBoundingBoxWgs84::new(-10.0, 10.0, 170.0, -170.0)?;
    let east = GeoBoundingBoxWgs84::new(-10.0, 10.0, 170.0, 180.0)?;
    let west = GeoBoundingBoxWgs84::new(-10.0, 10.0, -180.0, -170.0)?;
    let seam = GeoBoundingBoxWgs84::new(-10.0, 10.0, 180.0, -180.0)?;
    assert!(world.contains_box(&wrapped));
    assert!(!wrapped.contains_box(&world));
    assert!(east.contains_point(GeoPointWgs84::new(0.0, -180.0)?));
    assert!(west.contains_point(GeoPointWgs84::new(0.0, 180.0)?));
    assert!(east.contains_box(&seam) && west.contains_box(&seam));
    assert!(east.overlaps_box(&west) && west.overlaps_box(&east));
    assert!(!east.overlaps_interior(&west, 0.0));
    assert!(!seam.contains_point(GeoPointWgs84::new(0.0, 0.0)?));
    assert!(wrapped.overlaps_interior(&wrapped, 15.0));
    assert!(!wrapped.overlaps_interior(&wrapped, 20.0));

    // B1 / GEO-05: shared sensor/envelope edges stay covered, but the next
    // representable value outside the edge must not gain coverage by rounding.
    for (outer_west, inner_west, shared_east) in [(-33.3, -33.0, 44.9), (170.1, 171.2, -44.9)] {
        let outer = GeoBoundingBoxWgs84::new(-1.0, 1.0, outer_west, shared_east)?;
        let inner = GeoBoundingBoxWgs84::new(-1.0, 1.0, inner_west, shared_east)?;
        let outside = GeoBoundingBoxWgs84::new(-1.0, 1.0, inner_west, shared_east.next_up())?;
        assert!(outer.contains_box(&inner));
        assert!(outer.contains_box_with_tolerance(&inner, 0.0));
        assert!(!outer.contains_box(&outside));
        assert!(!outer.contains_box_with_tolerance(&outside, 0.0));
        assert!(!outer.contains_point(GeoPointWgs84::new(0.0, shared_east.next_up())?));
        let touching = GeoBoundingBoxWgs84::new(-1.0, 1.0, outer_west - 1.0, outer_west)?;
        assert!(outer.overlaps_box(&touching));
        assert!(!outer.overlaps_interior(&touching, 0.0));
    }

    let cap = GeoBoundingBoxWgs84::new(85.0, 90.0, 10.0, 20.0)?;
    let pole = GeoBoundingBoxWgs84::new(90.0, 90.0, -60.0, -50.0)?;
    assert!(cap.contains_box(&pole) && cap.overlaps_box(&pole));
    assert!(!cap.overlaps_interior(&pole, 0.0));
    assert!(cap.contains_point(GeoPointWgs84::new(90.0, 160.0)?));
    assert!(!cap.contains_point(GeoPointWgs84::new(89.0, 160.0)?));
    let p = GeoPointWgs84::new(-90.0, -160.0)?;
    assert_eq!(
        p.great_circle_distance_m(GeoPointWgs84::new(-90.0, 20.0)?),
        0.0
    );
    assert_eq!(p.longitude_deg(), -160.0); // Retain supplied coordinates.

    let near_seam = GeoBoundingBoxWgs84::new(-1.0, 1.0, 179.5, 179.9)?;
    assert!(!west.contains_box(&near_seam));
    assert!(west.contains_box_with_tolerance(&near_seam, 0.5));
    for invalid in [-1.0, f64::NAN, f64::INFINITY] {
        assert!(!world.contains_box_with_tolerance(&wrapped, invalid));
        assert!(!world.overlaps_interior(&wrapped, invalid));
    }
    // Analytic spherical quarter-circle reference; tolerance is numerical, not physical accuracy.
    let origin = GeoPointWgs84::new(0.0, 0.0)?;
    let quarter = origin.great_circle_distance_m(GeoPointWgs84::new(0.0, 90.0)?);
    assert!((quarter - EARTH_RADIUS_METERS * std::f64::consts::FRAC_PI_2).abs() < 1e-8);
    assert!(GeoPointWgs84::new(f64::NAN, 0.0).is_err());
    assert!(GeoPointWgs84::new(0.0, 181.0).is_err());
    assert!(GeoBoundingBoxWgs84::new(10.0, -10.0, 0.0, 1.0).is_err());
    Ok(())
}

/// Missing height stays missing; reference categories cannot silently convert a measurement.
#[test]
fn altitude_reference_and_missing_value_contract() -> Result<(), TimeGeoError> {
    let point = GeoPointWgs84::new(1.0, 2.0)?;
    assert_eq!(GeoPosition::new(point, None).altitude(), None);
    let below = Altitude::new(-30.0, AltitudeReference::OrthometricMsl)?;
    let sea = Altitude::new(0.0, AltitudeReference::OrthometricMsl)?;
    let band = AltitudeBand::new(below, sea)?;
    assert!(band.contains_altitude(&below)?);
    assert!(band.contains_altitude(&sea)?);
    assert!(
        band.contains_altitude(&Altitude::new(0.0, AltitudeReference::Wgs84Ellipsoid)?)
            .is_err()
    );
    for reference in [
        AltitudeReference::Unknown,
        AltitudeReference::RelativeToTakeoff,
    ] {
        let unbound = Altitude::new(10.0, reference)?;
        assert!(unbound.compare_in_same_reference(&unbound).is_err());
    }
    assert!(AltitudeBand::new(sea, below).is_err());
    assert!(Altitude::new(f64::INFINITY, AltitudeReference::BarometricUnadjusted).is_err());
    Ok(())
}

/// Timeline boundaries remain half-open even at the i64 endpoints; no subtraction overflows.
#[test]
fn exact_utc_interval_boundaries() -> Result<(), TimeGeoError> {
    let all = TimeWindow::new(i64::MIN, i64::MAX)?;
    let left = TimeWindow::new(i64::MIN, 0)?;
    let right = TimeWindow::new(0, i64::MAX)?;
    assert!(all.contains(i64::MIN) && !all.contains(i64::MAX));
    assert!(!left.contains(0) && right.contains(0));
    assert!(!left.overlaps(&right));
    assert!(all.contains_window(&left) && all.contains_window(&right));
    assert!(TimeWindow::new(0, 0).is_err());
    assert!(TimeWindow::new(1, 0).is_err());
    Ok(())
}

/// GEO-01 / SPACE-05 / DEVICE-04: calendar/RFC 3339 boundaries, date-only non-promotion,
/// unresolved DST fold ambiguity, IERS leap-second conversions/expiry, and unsynchronized clock refusal.
#[test]
fn temporal_representations_leap_seconds_and_clock_alignment_contract() -> Result<(), TimeGeoError>
{
    // 1. Civil calendar round-trip and RFC 3339 boundary validation.
    let epoch = CalendarDate::parse_yyyy_mm_dd("1970-01-01")?;
    assert_eq!(epoch.to_epoch_days(), 0);
    assert_eq!(CalendarDate::from_epoch_days(0)?, epoch);
    let leap = CalendarDate::parse_yyyy_mm_dd("2024-02-29")?;
    assert_eq!(CalendarDate::from_epoch_days(leap.to_epoch_days())?, leap);
    assert!(CalendarDate::parse_yyyy_mm_dd("2026-02-29").is_err());
    assert_eq!(parse_rfc3339_utc_seconds("1970-01-01T00:00:00Z")?, 0);
    assert!(parse_rfc3339_utc_seconds("2016-12-31T23:59:60Z").is_err());
    // End-of-UTC-day leap second is valid in nanosecond parser, mid-day `:60` is rejected.
    assert_eq!(
        parse_rfc3339_utc_nanos("2016-12-31T23:59:60Z")?,
        1_483_228_799_999_999_999
    );
    assert_eq!(
        parse_rfc3339_utc_nanos("2017-01-01T01:59:60+02:00")?,
        1_483_228_799_999_999_999
    );
    assert!(parse_rfc3339_utc_nanos("2016-12-31T12:00:60Z").is_err());

    // 2. Date-only and unknown times never silently promote to midnight UTC; unresolved DST fold
    //    returns None when only one candidate falls inside the query window.
    let window = TimeWindow::new(1_000, 5_000)?;
    let date_only = TimeRepresentation::CalendarDate {
        date: CalendarDate::new(1947, 6, 24)?,
        original_text: "June 24, 1947".to_owned(),
    };
    date_only.validate()?;
    assert_eq!(date_only.overlaps_utc_window(&window), None);

    let unknown = TimeRepresentation::UnknownTime {
        reason: UnknownReason::NotReported,
        original_text: Some("sometime that winter".to_owned()),
    };
    unknown.validate()?;
    assert_eq!(unknown.overlaps_utc_window(&window), None);

    let fold = TimeRepresentation::ExactInstant {
        utc_ms: 2_000,
        local: Some(LocalTimeContext {
            zone_name: "America/New_York".to_owned(),
            utc_offset_minutes: None,
            ambiguity: DstAmbiguity::AmbiguousFoldUnresolved {
                earlier_utc_ms: 2_000,
                later_utc_ms: 8_000,
            },
            original_text: "01:30 local".to_owned(),
        }),
    };
    fold.validate()?;
    assert_eq!(fold.overlaps_utc_window(&window), None);
    assert_eq!(
        fold.overlaps_utc_window(&TimeWindow::new(1_000, 10_000)?),
        Some(true)
    );
    assert!(
        LocalTimeContext {
            zone_name: "UTC+15".to_owned(),
            utc_offset_minutes: Some(841),
            ambiguity: DstAmbiguity::Unambiguous,
            original_text: "bad offset".to_owned(),
        }
        .validate()
        .is_err()
    );

    // 3. Complete 28-step IERS leap-second table: pre-1972 refusal, historical steps, and expiry refusal.
    let table = LeapSecondTable::iers_standard();
    assert!(matches!(
        table.tai_minus_utc_seconds(63_071_999_999),
        Err(TimeGeoError::PreLeapSecondEpoch(_))
    ));
    assert_eq!(table.tai_minus_utc_seconds(63_072_000_000)?, 10); // 1972-01-01T00:00:00Z
    assert_eq!(table.tai_minus_utc_seconds(858_297_600_000)?, 30); // 1997-03-14T00:00:00Z
    assert_eq!(table.tai_minus_utc_seconds(1_100_390_400_000)?, 32); // 2004-11-14T00:00:00Z
    let utc_2026_ms = 1_793_577_600_000_i64;
    let tai_ms = table.utc_to_tai_ms(utc_2026_ms)?;
    let tt_ms = table.utc_to_tt_ms(utc_2026_ms)?;
    assert_eq!(tai_ms - utc_2026_ms, 37_000);
    assert_eq!(tt_ms - tai_ms, TT_MINUS_TAI_MS);
    assert!(matches!(
        table.tai_minus_utc_seconds(table.expires_at_utc_ms),
        Err(TimeGeoError::LeapTableExpired { .. })
    ));

    // 4. Clock alignment: unsynchronized refuses receipt-time substitution; drift inflates uncertainty.
    let valid_window = TimeWindow::new(10_000, 110_000)?;
    let unsynced = ClockAlignment {
        reference_clock: "GNSS_PPS".to_owned(),
        method: "1PPS".to_owned(),
        offset_ms: 500,
        drift_ppm: 25,
        uncertainty_ms: 10,
        validity_window: valid_window,
        sync_state: ClockSyncState::Unsynchronized,
    };
    assert_eq!(
        unsynced.align_device_time(20_000),
        Err(TimeGeoError::UnsynchronizedClock)
    );

    let degraded = ClockAlignment {
        sync_state: ClockSyncState::DegradedDrift,
        ..unsynced
    };
    let aligned = degraded.align_device_time(89_500)?; // aligned_utc_ms = 90_000, elapsed = 80_000 ms
    assert_eq!(aligned.aligned_utc_ms, 90_000);
    // 80_000 ms * 25 ppm = 2 ms drift penalty -> 10 + 2 = 12 ms effective uncertainty.
    assert_eq!(aligned.effective_uncertainty_ms, 12);
    assert!(matches!(
        degraded.align_device_time(200_000),
        Err(TimeGeoError::ClockAlignmentOutOfWindow { .. })
    ));

    Ok(())
}
