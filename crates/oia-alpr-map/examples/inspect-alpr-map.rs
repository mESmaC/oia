use std::env;
use std::process::ExitCode;

use oia_alpr_map::{
    AlprCoverageStatus, AlprLocalFilter, AlprQueryPolicy, GeoBBox, RawOsmNode,
    direction_wedge_polygon, evaluate_viewport_query,
};

fn fixture_nodes() -> Vec<RawOsmNode> {
    vec![
        RawOsmNode {
            element_type: "node".to_owned(),
            osm_id: 91_010_421,
            latitude_deg: 30.2672,
            longitude_deg: -97.7431,
            coordinate_zoom: Some(12),
            surveillance_type: Some("ALPR".to_owned()),
            manufacturer: Some("Flock Safety".to_owned()),
            brand: None,
            operator: Some("Austin Police Department".to_owned()),
            camera_type: Some("fixed".to_owned()),
            surveillance_zone: Some("traffic".to_owned()),
            camera_direction: Some("180".to_owned()),
            direction: None,
            reference: Some("APD-CONGRESS-01".to_owned()),
            check_date: Some("2026-09-14".to_owned()),
            survey_date: None,
            osm_timestamp: Some("2026-10-18T14:22:10Z".to_owned()),
            source: Some("field survey".to_owned()),
        },
        RawOsmNode {
            element_type: "node".to_owned(),
            osm_id: 91_010_422,
            latitude_deg: 30.2747,
            longitude_deg: -97.7404,
            coordinate_zoom: Some(12),
            surveillance_type: Some("camera;ALPR".to_owned()),
            manufacturer: Some("Flock Safety".to_owned()),
            brand: None,
            operator: Some("Texas DPS - Capitol Complex".to_owned()),
            camera_type: Some("fixed".to_owned()),
            surveillance_zone: Some("street".to_owned()),
            camera_direction: Some("360".to_owned()),
            direction: None,
            reference: Some("DPS-CAP-N04".to_owned()),
            check_date: None,
            survey_date: Some("2026-08-29".to_owned()),
            osm_timestamp: Some("2026-10-19T08:11:00Z".to_owned()),
            source: None,
        },
        RawOsmNode {
            element_type: "node".to_owned(),
            osm_id: 91_010_423,
            latitude_deg: 30.2638,
            longitude_deg: -97.7522,
            coordinate_zoom: Some(12),
            surveillance_type: Some("ALPR".to_owned()),
            manufacturer: Some("Flock Safety".to_owned()),
            brand: None,
            operator: Some("Barton Springs Commercial HOA".to_owned()),
            camera_type: Some("fixed".to_owned()),
            surveillance_zone: Some("parking".to_owned()),
            camera_direction: Some("   ".to_owned()),
            direction: None,
            reference: Some("HOA-BS-02".to_owned()),
            check_date: None,
            survey_date: None,
            osm_timestamp: Some("2026-10-21T19:45:33Z".to_owned()),
            source: None,
        },
        RawOsmNode {
            element_type: "node".to_owned(),
            osm_id: 91_010_424,
            latitude_deg: 30.2712,
            longitude_deg: -97.7341,
            coordinate_zoom: Some(12),
            surveillance_type: Some("alpr".to_owned()),
            manufacturer: Some("Flock Safety".to_owned()),
            brand: None,
            operator: Some("Austin Police Department".to_owned()),
            camera_type: Some("fixed".to_owned()),
            surveillance_zone: Some("traffic".to_owned()),
            camera_direction: None,
            direction: Some("265".to_owned()),
            reference: Some("APD-I35-FR07".to_owned()),
            check_date: Some("2026-10-02".to_owned()),
            survey_date: None,
            osm_timestamp: Some("2026-10-22T03:10:05Z".to_owned()),
            source: None,
        },
        RawOsmNode {
            element_type: "node".to_owned(),
            osm_id: 91_010_425,
            latitude_deg: 30.2791,
            longitude_deg: -97.7378,
            coordinate_zoom: Some(12),
            surveillance_type: Some("ALPR".to_owned()),
            manufacturer: Some("Motorola Solutions / Vigilant".to_owned()),
            brand: None,
            operator: Some("UT Austin Campus Safety".to_owned()),
            camera_type: Some("fixed".to_owned()),
            surveillance_zone: Some("traffic".to_owned()),
            camera_direction: Some("95".to_owned()),
            direction: None,
            reference: Some("UTPD-MLK-12".to_owned()),
            check_date: Some("2026-07-19".to_owned()),
            survey_date: None,
            osm_timestamp: Some("2026-10-25T11:04:19Z".to_owned()),
            source: None,
        },
        RawOsmNode {
            element_type: "node".to_owned(),
            osm_id: 91_010_426,
            latitude_deg: 30.2604,
            longitude_deg: -97.7389,
            coordinate_zoom: Some(12),
            surveillance_type: Some("ALPR".to_owned()),
            manufacturer: Some("Genetec AutoVu".to_owned()),
            brand: None,
            operator: Some("Riverside Garage Management".to_owned()),
            camera_type: Some("fixed".to_owned()),
            surveillance_zone: Some("parking".to_owned()),
            camera_direction: Some("45".to_owned()),
            direction: None,
            reference: None,
            check_date: None,
            survey_date: None,
            osm_timestamp: Some("2026-10-26T16:30:00Z".to_owned()),
            source: None,
        },
        RawOsmNode {
            element_type: "node".to_owned(),
            osm_id: 91_010_427,
            latitude_deg: 30.2689,
            longitude_deg: -97.7574,
            coordinate_zoom: Some(12),
            surveillance_type: Some("ALPR".to_owned()),
            manufacturer: None,
            brand: None,
            operator: None,
            camera_type: Some("fixed".to_owned()),
            surveillance_zone: Some("traffic".to_owned()),
            camera_direction: Some("125".to_owned()),
            direction: None,
            reference: None,
            check_date: None,
            survey_date: None,
            osm_timestamp: Some("2026-10-28T22:15:42Z".to_owned()),
            source: Some("aerial_and_street_level".to_owned()),
        },
        RawOsmNode {
            element_type: "node".to_owned(),
            osm_id: 91_010_428,
            latitude_deg: 30.2764,
            longitude_deg: -97.7495,
            coordinate_zoom: Some(12),
            surveillance_type: Some("ALPR".to_owned()),
            manufacturer: Some("Flock Safety".to_owned()),
            brand: None,
            operator: Some("Shoal Creek Neighborhood Association".to_owned()),
            camera_type: Some("fixed".to_owned()),
            surveillance_zone: Some("street".to_owned()),
            camera_direction: Some("NE".to_owned()),
            direction: None,
            reference: None,
            check_date: Some("2026-10-11".to_owned()),
            survey_date: None,
            osm_timestamp: Some("2026-10-29T09:05:12Z".to_owned()),
            source: None,
        },
        RawOsmNode {
            element_type: "node".to_owned(),
            osm_id: 91_010_499,
            latitude_deg: 30.2661,
            longitude_deg: -97.7422,
            coordinate_zoom: Some(12),
            surveillance_type: Some("camera".to_owned()),
            manufacturer: None,
            brand: None,
            operator: Some("TxDOT".to_owned()),
            camera_type: None,
            surveillance_zone: None,
            camera_direction: Some("90".to_owned()),
            direction: None,
            reference: None,
            check_date: None,
            survey_date: None,
            osm_timestamp: Some("2026-10-30T12:00:00Z".to_owned()),
            source: None,
        },
    ]
}

fn parse_filter(arg: Option<&str>) -> Result<AlprLocalFilter, &'static str> {
    match arg.unwrap_or("all") {
        "all" => Ok(AlprLocalFilter::AllMappedAlpr),
        "flock" => Ok(AlprLocalFilter::FlockSafetyOnly),
        "other" => Ok(AlprLocalFilter::OtherNamedOnly),
        "unspecified" => Ok(AlprLocalFilter::UnspecifiedOnly),
        "bearing" => Ok(AlprLocalFilter::WithBearingOnly),
        _ => Err("filter must be one of: all, flock, other, unspecified, bearing"),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let filter = match parse_filter(args.first().map(String::as_str)) {
        Ok(filter) => filter,
        Err(message) => {
            eprintln!("inspect-alpr-map: {message}");
            return ExitCode::from(1);
        }
    };
    let query_limit = match args.get(1) {
        Some(raw) => match raw.parse::<usize>() {
            Ok(limit) => limit,
            Err(_) => {
                eprintln!("inspect-alpr-map: QUERY_LIMIT must be a positive integer");
                return ExitCode::from(1);
            }
        },
        None => 1_500,
    };

    let policy = match AlprQueryPolicy::with_limits(3.0, 0.05, 9, 12, 16, query_limit) {
        Ok(policy) => policy,
        Err(err) => {
            eprintln!("inspect-alpr-map: {err}");
            return ExitCode::from(1);
        }
    };
    let requested = match GeoBBox::new(30.255, -97.765, 30.285, -97.725) {
        Ok(bbox) => bbox,
        Err(err) => {
            eprintln!("inspect-alpr-map: {err}");
            return ExitCode::from(1);
        }
    };
    let extract_bounds = GeoBBox::new(17.0, -180.0, 84.0, -50.0).ok();
    let nodes = fixture_nodes();

    let snapshot =
        match evaluate_viewport_query(requested, extract_bounds, &nodes, filter, policy, || None) {
            Ok(snapshot) => snapshot,
            Err(err) => {
                eprintln!("inspect-alpr-map: {err}");
                return ExitCode::from(1);
            }
        };

    println!("origin=synthetic-fixture source=OpenStreetMap-ODbL-1.0");
    println!(
        "viewport={:.4},{:.4}..{:.4},{:.4} snapped={:.2},{:.2}..{:.2},{:.2} detail_zoom={:?}",
        snapshot.requested_bbox.south(),
        snapshot.requested_bbox.west(),
        snapshot.requested_bbox.north(),
        snapshot.requested_bbox.east(),
        snapshot.snapped_bbox.south(),
        snapshot.snapped_bbox.west(),
        snapshot.snapped_bbox.north(),
        snapshot.snapped_bbox.east(),
        snapshot.detail_zoom
    );
    println!(
        "coverage={:?} active_filter=\"{}\" local_filter_omitted={}",
        snapshot.coverage_status,
        snapshot.active_filter.chip_label(),
        snapshot.local_filter_omitted
    );
    println!(
        "viewport_breakdown total_alpr={} flock_safety={} other_named={} unspecified={} with_bearing={} field_verified={}",
        snapshot.breakdown.total_in_viewport,
        snapshot.breakdown.flock_safety,
        snapshot.breakdown.other_named,
        snapshot.breakdown.unspecified,
        snapshot.breakdown.with_bearing,
        snapshot.breakdown.verified_date_present
    );

    for record in &snapshot.records {
        let wedge_summary = match direction_wedge_polygon(record, policy) {
            Some(wedge) => format!(
                "illustrative-wedge({:.0}deg, {:.0}m, +/-{:.0}deg)",
                wedge.bearing_deg, wedge.radius_m, wedge.half_angle_deg
            ),
            None => "no-bearing-recorded".to_owned(),
        };
        println!(
            "camera id={} badge={} class=\"{}\" operator={:?} pos=({:.4},{:.4}) wedge={} check_date={:?} osm_timestamp={:?}",
            record.id,
            record.display_id(),
            record.manufacturer_class.short_label(),
            record.operator,
            record.latitude_deg,
            record.longitude_deg,
            wedge_summary,
            record.last_verified,
            record.osm_timestamp
        );
    }

    match snapshot.coverage_status {
        AlprCoverageStatus::Saturated { .. } => ExitCode::from(2),
        _ => ExitCode::SUCCESS,
    }
}
