use std::error::Error;

use oia_alpr_map::{
    AlprCoverageStatus, AlprLocalFilter, AlprManufacturerClass, AlprMapError, AlprPrecisionCache,
    AlprQueryPolicy, GeoBBox, GeometryClass, RawOsmNode, StopReason, alpr_credit_markup,
    alpr_detail_zoom, alpr_retry_delay_ms, build_overpass_query, classify_manufacturer,
    direction_wedge_polygon, evaluate_viewport_query, is_alpr_surveillance_type,
    normalize_alpr_node, normalize_direction_deg, validate_alpr_records,
};

type TestResult = Result<(), Box<dyn Error>>;

fn sample_node(osm_id: i64, lat: f64, lon: f64) -> RawOsmNode {
    RawOsmNode {
        element_type: "node".to_owned(),
        osm_id,
        latitude_deg: lat,
        longitude_deg: lon,
        coordinate_zoom: Some(12),
        surveillance_type: Some("ALPR".to_owned()),
        manufacturer: Some("Flock Safety".to_owned()),
        brand: None,
        operator: Some("Austin Police Department".to_owned()),
        camera_type: Some("fixed".to_owned()),
        surveillance_zone: Some("traffic".to_owned()),
        camera_direction: Some("180".to_owned()),
        direction: None,
        reference: Some("APD-01".to_owned()),
        check_date: Some("2026-09-14".to_owned()),
        survey_date: None,
        osm_timestamp: Some("2026-10-18T14:22:10Z".to_owned()),
        source: Some("survey".to_owned()),
    }
}

#[test]
fn surveillance_type_and_bearing_preserve_missing_values_and_provenance() -> TestResult {
    // Protects CAM-02 and AGENTS.md §2: semicolon-delimited surveillance:type
    // tags are matched case-insensitively, blank/cardinal bearings never become
    // 0 deg (due north), 360 wraps to 0, and osm_timestamp is never promoted to
    // field verification (last_verified).
    assert!(is_alpr_surveillance_type("ALPR"));
    assert!(is_alpr_surveillance_type("camera;ALPR"));
    assert!(is_alpr_surveillance_type(" traffic ; alpr ; pole "));
    assert!(!is_alpr_surveillance_type("camera"));
    assert!(!is_alpr_surveillance_type(""));

    assert_eq!(normalize_direction_deg(None), None);
    assert_eq!(normalize_direction_deg(Some("")), None);
    assert_eq!(normalize_direction_deg(Some("   ")), None);
    assert_eq!(normalize_direction_deg(Some("N")), None);
    assert_eq!(normalize_direction_deg(Some("NE")), None);
    assert_eq!(normalize_direction_deg(Some("-1")), None);
    assert_eq!(normalize_direction_deg(Some("361")), None);
    assert_eq!(normalize_direction_deg(Some("0")), Some(0.0));
    assert_eq!(normalize_direction_deg(Some("360")), Some(0.0));
    assert_eq!(normalize_direction_deg(Some(" 185.5 ")), Some(185.5));

    let mut raw = sample_node(91_010_423, 30.2638, -97.7522);
    raw.camera_direction = Some("   ".to_owned());
    raw.check_date = None;
    raw.survey_date = None;
    raw.osm_timestamp = Some("2026-10-21T19:45:33Z".to_owned());
    let record = normalize_alpr_node(&raw).ok_or("valid ALPR node")?;
    assert_eq!(record.id, "alpr:91010423");
    assert_eq!(record.display_id(), "ALPR-0423");
    assert_eq!(record.direction_deg, None);
    assert_eq!(
        record.last_verified, None,
        "OSM edit timestamp must not become a field verification date"
    );
    assert_eq!(
        record.osm_timestamp.as_deref(),
        Some("2026-10-21T19:45:33Z")
    );
    assert_eq!(
        direction_wedge_polygon(&record, AlprQueryPolicy::default_city_scale()),
        None
    );

    let with_bearing = normalize_alpr_node(&sample_node(91_010_421, 30.2672, -97.7431))
        .ok_or("valid ALPR node with bearing")?;
    let wedge = direction_wedge_polygon(&with_bearing, AlprQueryPolicy::default_city_scale())
        .ok_or("wedge for numeric bearing")?;
    assert_eq!(
        wedge.geometry_class,
        GeometryClass::IllustrativeBearingWedge
    );
    assert!(wedge.left_lat_deg < wedge.apex_lat_deg);
    assert!(wedge.right_lat_deg < wedge.apex_lat_deg);
    assert!(wedge.left_lon_deg > wedge.right_lon_deg);
    Ok(())
}

#[test]
fn detail_zoom_and_precision_cache_preserve_finer_coordinates() -> TestResult {
    // Protects GEO-02/GEO-04 and selective port parity with God's Eye View
    // `source.test.mjs`: each view picks the finest zoom in z12..=z9 fitting
    // <= 16 tiles, and coarser tiles never nudge a camera placed at z12.
    let policy = AlprQueryPolicy::default_city_scale();
    let neighborhood = GeoBBox::new(30.29, -97.78, 30.31, -97.75)?;
    let central_city = GeoBBox::new(30.17, -97.85, 30.36, -97.63)?;
    let metro = GeoBBox::new(29.7, -98.25, 30.7, -97.25)?;
    let region = GeoBBox::new(29.2, -98.75, 31.2, -96.75)?;
    let too_wide = GeoBBox::new(29.0, -99.0, 32.0, -96.0)?;

    assert_eq!(alpr_detail_zoom(neighborhood, policy), Some(12));
    assert_eq!(alpr_detail_zoom(central_city, policy), Some(12));
    assert_eq!(alpr_detail_zoom(metro, policy), Some(10));
    assert_eq!(alpr_detail_zoom(region, policy), Some(9));
    assert_eq!(alpr_detail_zoom(too_wide, policy), None);

    let mut cache = AlprPrecisionCache::new(2)?;
    let fine = normalize_alpr_node(&sample_node(1, 30.30001, -97.76001)).ok_or("fine record")?;
    let applied_fine = cache.apply(fine, 12);
    assert_eq!(applied_fine.latitude_deg, 30.30001);
    assert_eq!(applied_fine.longitude_deg, -97.76001);

    let mut coarse_raw = sample_node(1, 30.3001, -97.7602);
    coarse_raw.operator = Some("Updated Operator".to_owned());
    let coarse = normalize_alpr_node(&coarse_raw).ok_or("coarse record")?;
    let kept = cache.apply(coarse, 9);
    assert_eq!(kept.latitude_deg, 30.30001);
    assert_eq!(kept.longitude_deg, -97.76001);
    assert_eq!(kept.coordinate_zoom, Some(12));
    assert_eq!(kept.operator.as_deref(), Some("Updated Operator"));

    let r2 = normalize_alpr_node(&sample_node(2, 30.1, -97.1)).ok_or("r2")?;
    let r3 = normalize_alpr_node(&sample_node(3, 30.2, -97.2)).ok_or("r3")?;
    cache.apply(r2, 12);
    cache.apply(r3, 12);
    assert_eq!(cache.len(), 2);
    Ok(())
}

#[test]
fn viewport_query_separates_coverage_states_and_manufacturer_filters() -> TestResult {
    // Protects CASE-02, CAM-01/02, and GEO-02: local manufacturer filters expose
    // omitted counts rather than silently dropping non-Flock or untagged cameras,
    // saturation truncates deterministically by center distance, and NoCoverage /
    // ZoomRequired / Stopped remain distinct.
    let policy = AlprQueryPolicy::with_limits(3.0, 0.05, 9, 12, 16, 2)?;
    let austin = GeoBBox::new(30.255, -97.765, 30.285, -97.725)?;
    let us_ca_extract = GeoBBox::new(17.0, -180.0, 84.0, -50.0)?;

    let mut flock_center = sample_node(101, 30.2700, -97.7450);
    flock_center.manufacturer = Some("Flock Safety".to_owned());

    let mut motorola_mid = sample_node(102, 30.2750, -97.7400);
    motorola_mid.manufacturer = Some("Motorola Solutions".to_owned());

    let mut unspecified_edge = sample_node(103, 30.2840, -97.7260);
    unspecified_edge.manufacturer = None;
    unspecified_edge.operator = None;

    let mut non_alpr = sample_node(104, 30.2700, -97.7450);
    non_alpr.surveillance_type = Some("camera".to_owned());

    let nodes = vec![unspecified_edge, motorola_mid, flock_center, non_alpr];

    let all_snapshot = evaluate_viewport_query(
        austin,
        Some(us_ca_extract),
        &nodes,
        AlprLocalFilter::AllMappedAlpr,
        policy,
        || None,
    )?;
    assert_eq!(all_snapshot.breakdown.total_in_viewport, 3);
    assert_eq!(all_snapshot.breakdown.flock_safety, 1);
    assert_eq!(all_snapshot.breakdown.other_named, 1);
    assert_eq!(all_snapshot.breakdown.unspecified, 1);
    assert_eq!(
        all_snapshot.coverage_status,
        AlprCoverageStatus::Saturated {
            total_matching: 3,
            retained: 2,
            omitted: 1,
        }
    );
    assert_eq!(all_snapshot.records.len(), 2);
    assert_eq!(
        all_snapshot.records.first().map(|r| r.osm_id),
        Some(101),
        "camera nearest viewport center is retained first"
    );

    let flock_only = evaluate_viewport_query(
        austin,
        Some(us_ca_extract),
        &nodes,
        AlprLocalFilter::FlockSafetyOnly,
        policy,
        || None,
    )?;
    assert_eq!(flock_only.coverage_status, AlprCoverageStatus::Complete);
    assert_eq!(flock_only.records.len(), 1);
    assert_eq!(flock_only.local_filter_omitted, 2);

    let europe = GeoBBox::new(51.0, 0.0, 51.1, 0.1)?;
    let no_cov = evaluate_viewport_query(
        europe,
        Some(us_ca_extract),
        &nodes,
        AlprLocalFilter::AllMappedAlpr,
        policy,
        || None,
    )?;
    assert_eq!(no_cov.coverage_status, AlprCoverageStatus::NoCoverage);

    let wide = GeoBBox::new(28.0, -100.0, 32.0, -96.0)?;
    let zoom_req = evaluate_viewport_query(
        wide,
        Some(us_ca_extract),
        &nodes,
        AlprLocalFilter::AllMappedAlpr,
        policy,
        || None,
    )?;
    assert_eq!(zoom_req.coverage_status, AlprCoverageStatus::ZoomRequired);

    let stopped = evaluate_viewport_query(
        austin,
        Some(us_ca_extract),
        &nodes,
        AlprLocalFilter::AllMappedAlpr,
        policy,
        || Some(StopReason::Cancelled),
    );
    assert_eq!(stopped, Err(AlprMapError::Stopped(StopReason::Cancelled)));
    Ok(())
}

#[test]
fn attribution_query_validation_and_backoff_enforce_safety_contracts() -> TestResult {
    assert_eq!(
        classify_manufacturer(None, None, Some(" Austin Police ")),
        (None, AlprManufacturerClass::Unspecified)
    );
    assert_eq!(
        alpr_credit_markup("", "https://www.openstreetmap.org/copyright"),
        Ok(None)
    );
    assert!(alpr_credit_markup("OSM", "javascript:alert(1)").is_err());
    assert!(alpr_credit_markup("OSM", "https://user:pass@example.org/").is_err());

    let html = alpr_credit_markup("<img onerror=alert(1)>", "https://example.org/?a=1&b=2")?
        .ok_or("non-empty markup")?;
    assert!(html.contains("&lt;img onerror=alert(1)&gt;"));
    assert!(html.contains("?a=1&amp;b=2"));
    assert!(!html.contains("<img"));

    let bbox = GeoBBox::new(30.0, -98.0, 30.1, -97.9)?;
    let query = build_overpass_query(bbox, 1500);
    assert!(query.contains("[\"surveillance:type\"~\"(^|;)\\\\s*ALPR\\\\s*(;|$)\",i]"));

    let rec = normalize_alpr_node(&sample_node(42, 30.05, -97.95)).ok_or("valid record")?;
    assert_eq!(validate_alpr_records(std::slice::from_ref(&rec)), Ok(()));
    assert_eq!(
        validate_alpr_records(&[rec.clone(), rec]),
        Err(AlprMapError::DuplicateRecordId("alpr:42".to_owned()))
    );

    assert_eq!(alpr_retry_delay_ms(None), 30_000);
    assert_eq!(alpr_retry_delay_ms(Some(30_000)), 60_000);
    assert_eq!(alpr_retry_delay_ms(Some(240_000)), 240_000);
    Ok(())
}
