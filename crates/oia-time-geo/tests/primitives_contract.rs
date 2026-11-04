use oia_time_geo::{
    Altitude, AltitudeBand, AltitudeReference, EARTH_RADIUS_METERS, GeoBoundingBoxWgs84,
    GeoPointWgs84, GeoPosition, TimeGeoError, TimeWindow,
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
