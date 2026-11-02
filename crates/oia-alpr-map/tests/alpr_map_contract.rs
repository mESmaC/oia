use std::error::Error;

use oia_alpr_map::{
    AlprCoverageStatus, AlprLocalFilter, AlprManufacturerClass, AlprMapError, AlprPrecisionCache,
    AlprQueryPolicy, GeoBBox, GeometryClass, RawOsmNode, SYNTHETIC_OSM_ID_FLOOR, StopReason,
    alpr_credit_markup, alpr_detail_zoom, alpr_retry_delay_ms, build_overpass_query,
    classify_manufacturer, direction_wedge_polygon, evaluate_viewport_query,
    is_alpr_surveillance_type, normalize_alpr_node, normalize_direction_deg, validate_alpr_records,
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
        operator: Some("New Meridian Municipal Transit Authority (Synthetic)".to_owned()),
        camera_type: Some("fixed".to_owned()),
        surveillance_zone: Some("traffic".to_owned()),
        camera_direction: Some("180".to_owned()),
        direction: None,
        reference: Some("SYNTH-NM-001".to_owned()),
        check_date: Some("2026-09-14".to_owned()),
        survey_date: None,
        osm_timestamp: Some("2026-10-18T14:22:10Z".to_owned()),
        source: Some("synthetic_fixture".to_owned()),
        note: Some("SYNTHETIC FIXTURE - NOT OBSERVED".to_owned()),
        oia_synthetic: Some("true".to_owned()),
        synthetic: true,
    }
}

#[test]
fn surveillance_type_and_bearing_preserve_missing_values_and_provenance() -> TestResult {
    // Protects CAM-06 and AGENTS.md §2: semicolon-delimited surveillance:type
    // tags are matched case-insensitively, blank/cardinal bearings never become
    // 0 deg (due north), 360 wraps to 0, osm_timestamp is never promoted to
    // field verification (last_verified), and synthetic markers survive node
    // normalization.
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

    let mut raw = sample_node(900_000_000_103, 37.6138, -121.9022);
    raw.camera_direction = Some("   ".to_owned());
    raw.check_date = None;
    raw.survey_date = None;
    raw.osm_timestamp = Some("2026-10-21T19:45:33Z".to_owned());
    let record = normalize_alpr_node(&raw).ok_or("valid ALPR node")?;
    assert!(record.osm_id >= SYNTHETIC_OSM_ID_FLOOR);
    assert_eq!(record.id, "alpr:900000000103");
    assert_eq!(record.display_id(), "ALPR-0103");
    assert_eq!(record.direction_deg, None);
    assert!(record.synthetic);
    assert_eq!(
        record.note.as_deref(),
        Some("SYNTHETIC FIXTURE - NOT OBSERVED")
    );
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

    let with_bearing = normalize_alpr_node(&sample_node(900_000_000_101, 37.6172, -121.8931))
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
    // Protects CAM-06, GEO-02/GEO-04, and selective port parity with God's Eye View
    // `source.test.mjs`: each view picks the finest zoom in z12..=z9 fitting
    // <= 16 tiles, and coarser tiles never nudge a camera placed at z12.
    let policy = AlprQueryPolicy::default_city_scale();
    let neighborhood = GeoBBox::new(37.60, -121.91, 37.63, -121.88)?;
    let central_city = GeoBBox::new(37.52, -122.00, 37.71, -121.78)?;
    let metro = GeoBBox::new(37.15, -122.35, 37.95, -121.55)?;
    let region = GeoBBox::new(36.75, -122.70, 38.35, -121.10)?;
    let too_wide = GeoBBox::new(36.0, -123.5, 39.0, -120.5)?;

    assert_eq!(alpr_detail_zoom(neighborhood, policy), Some(12));
    assert_eq!(alpr_detail_zoom(central_city, policy), Some(12));
    assert_eq!(alpr_detail_zoom(metro, policy), Some(10));
    assert_eq!(alpr_detail_zoom(region, policy), Some(9));
    assert_eq!(alpr_detail_zoom(too_wide, policy), None);

    let mut cache = AlprPrecisionCache::new(2)?;
    let fine = normalize_alpr_node(&sample_node(900_000_000_101, 37.61721, -121.89311))
        .ok_or("fine record")?;
    let applied_fine = cache.apply(fine, 12);
    assert_eq!(applied_fine.latitude_deg, 37.61721);
    assert_eq!(applied_fine.longitude_deg, -121.89311);

    let mut coarse_raw = sample_node(900_000_000_101, 37.6175, -121.8935);
    coarse_raw.operator = Some("Updated Synthetic Operator".to_owned());
    let coarse = normalize_alpr_node(&coarse_raw).ok_or("coarse record")?;
    let kept = cache.apply(coarse, 9);
    assert_eq!(kept.latitude_deg, 37.61721);
    assert_eq!(kept.longitude_deg, -121.89311);
    assert_eq!(kept.coordinate_zoom, Some(12));
    assert_eq!(kept.operator.as_deref(), Some("Updated Synthetic Operator"));

    let r2 = normalize_alpr_node(&sample_node(900_000_000_102, 37.61, -121.89)).ok_or("r2")?;
    let r3 = normalize_alpr_node(&sample_node(900_000_000_103, 37.62, -121.90)).ok_or("r3")?;
    cache.apply(r2, 12);
    cache.apply(r3, 12);
    assert_eq!(cache.len(), 2);
    Ok(())
}

#[test]
fn viewport_query_separates_coverage_states_and_manufacturer_filters() -> TestResult {
    // Protects CASE-02, CAM-06, and GEO-02: local manufacturer filters expose
    // omitted counts rather than silently dropping non-Flock or untagged cameras,
    // saturation truncates deterministically by center distance, and NoCoverage /
    // ZoomRequired / Stopped remain distinct.
    let policy = AlprQueryPolicy::with_limits(3.0, 0.05, 9, 12, 16, 2)?;
    let new_meridian = GeoBBox::new(37.605, -121.915, 37.635, -121.875)?;
    let us_ca_extract = GeoBBox::new(17.0, -180.0, 84.0, -50.0)?;

    let mut flock_center = sample_node(900_000_000_101, 37.6200, -121.8950);
    flock_center.manufacturer = Some("Flock Safety".to_owned());

    let mut motorola_mid = sample_node(900_000_000_102, 37.6250, -121.8900);
    motorola_mid.manufacturer = Some("Motorola Solutions".to_owned());

    let mut unspecified_edge = sample_node(900_000_000_103, 37.6340, -121.8760);
    unspecified_edge.manufacturer = None;
    unspecified_edge.operator = None;

    let mut non_alpr = sample_node(900_000_000_199, 37.6200, -121.8950);
    non_alpr.surveillance_type = Some("camera".to_owned());

    let nodes = vec![unspecified_edge, motorola_mid, flock_center, non_alpr];

    let all_snapshot = evaluate_viewport_query(
        new_meridian,
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
        Some(900_000_000_101),
        "camera nearest viewport center is retained first"
    );

    let flock_only = evaluate_viewport_query(
        new_meridian,
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

    let wide = GeoBBox::new(35.0, -124.0, 39.0, -120.0)?;
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
        new_meridian,
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
        classify_manufacturer(
            None,
            None,
            Some(" New Meridian Transit Authority (Synthetic) ")
        ),
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

    let bbox = GeoBBox::new(37.6, -121.9, 37.7, -121.8)?;
    let query = build_overpass_query(bbox, 1500);
    assert!(query.contains("[\"surveillance:type\"~\"(^|;)\\\\s*ALPR\\\\s*(;|$)\",i]"));

    let rec =
        normalize_alpr_node(&sample_node(900_000_000_142, 37.65, -121.85)).ok_or("valid record")?;
    assert_eq!(validate_alpr_records(std::slice::from_ref(&rec)), Ok(()));
    assert_eq!(
        validate_alpr_records(&[rec.clone(), rec]),
        Err(AlprMapError::DuplicateRecordId(
            "alpr:900000000142".to_owned()
        ))
    );

    assert_eq!(alpr_retry_delay_ms(None), 30_000);
    assert_eq!(alpr_retry_delay_ms(Some(30_000)), 60_000);
    assert_eq!(alpr_retry_delay_ms(Some(240_000)), 240_000);
    Ok(())
}
