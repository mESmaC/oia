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
            osm_id: 900_000_000_101,
            latitude_deg: 37.6172,
            longitude_deg: -121.8931,
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
        },
        RawOsmNode {
            element_type: "node".to_owned(),
            osm_id: 900_000_000_102,
            latitude_deg: 37.6247,
            longitude_deg: -121.8904,
            coordinate_zoom: Some(12),
            surveillance_type: Some("camera;ALPR".to_owned()),
            manufacturer: Some("Flock Safety".to_owned()),
            brand: None,
            operator: Some("Aegis Harbor District Patrol (Synthetic)".to_owned()),
            camera_type: Some("fixed".to_owned()),
            surveillance_zone: Some("street".to_owned()),
            camera_direction: Some("360".to_owned()),
            direction: None,
            reference: Some("SYNTH-NM-002".to_owned()),
            check_date: None,
            survey_date: Some("2026-08-29".to_owned()),
            osm_timestamp: Some("2026-10-19T08:11:00Z".to_owned()),
            source: None,
            note: Some("SYNTHETIC FIXTURE - NOT OBSERVED".to_owned()),
            oia_synthetic: Some("true".to_owned()),
            synthetic: true,
        },
        RawOsmNode {
            element_type: "node".to_owned(),
            osm_id: 900_000_000_103,
            latitude_deg: 37.6138,
            longitude_deg: -121.9022,
            coordinate_zoom: Some(12),
            surveillance_type: Some("ALPR".to_owned()),
            manufacturer: Some("Flock Safety".to_owned()),
            brand: None,
            operator: Some("Kestrel Commons Commercial HOA (Synthetic)".to_owned()),
            camera_type: Some("fixed".to_owned()),
            surveillance_zone: Some("parking".to_owned()),
            camera_direction: Some("   ".to_owned()),
            direction: None,
            reference: Some("SYNTH-NM-003".to_owned()),
            check_date: None,
            survey_date: None,
            osm_timestamp: Some("2026-10-21T19:45:33Z".to_owned()),
            source: None,
            note: Some("SYNTHETIC FIXTURE - NOT OBSERVED".to_owned()),
            oia_synthetic: Some("true".to_owned()),
            synthetic: true,
        },
        RawOsmNode {
            element_type: "node".to_owned(),
            osm_id: 900_000_000_104,
            latitude_deg: 37.6212,
            longitude_deg: -121.8841,
            coordinate_zoom: Some(12),
            surveillance_type: Some("alpr".to_owned()),
            manufacturer: Some("Flock Safety".to_owned()),
            brand: None,
            operator: Some("New Meridian Municipal Transit Authority (Synthetic)".to_owned()),
            camera_type: Some("fixed".to_owned()),
            surveillance_zone: Some("traffic".to_owned()),
            camera_direction: None,
            direction: Some("265".to_owned()),
            reference: Some("SYNTH-NM-004".to_owned()),
            check_date: Some("2026-10-02".to_owned()),
            survey_date: None,
            osm_timestamp: Some("2026-10-22T03:10:05Z".to_owned()),
            source: None,
            note: Some("SYNTHETIC FIXTURE - NOT OBSERVED".to_owned()),
            oia_synthetic: Some("true".to_owned()),
            synthetic: true,
        },
        RawOsmNode {
            element_type: "node".to_owned(),
            osm_id: 900_000_000_105,
            latitude_deg: 37.6291,
            longitude_deg: -121.8878,
            coordinate_zoom: Some(12),
            surveillance_type: Some("ALPR".to_owned()),
            manufacturer: Some("Motorola Solutions / Vigilant".to_owned()),
            brand: None,
            operator: Some("North Spire Institute Safety (Synthetic)".to_owned()),
            camera_type: Some("fixed".to_owned()),
            surveillance_zone: Some("traffic".to_owned()),
            camera_direction: Some("95".to_owned()),
            direction: None,
            reference: Some("SYNTH-NM-005".to_owned()),
            check_date: Some("2026-07-19".to_owned()),
            survey_date: None,
            osm_timestamp: Some("2026-10-25T11:04:19Z".to_owned()),
            source: None,
            note: Some("SYNTHETIC FIXTURE - NOT OBSERVED".to_owned()),
            oia_synthetic: Some("true".to_owned()),
            synthetic: true,
        },
        RawOsmNode {
            element_type: "node".to_owned(),
            osm_id: 900_000_000_106,
            latitude_deg: 37.6104,
            longitude_deg: -121.8889,
            coordinate_zoom: Some(12),
            surveillance_type: Some("ALPR".to_owned()),
            manufacturer: Some("Genetec AutoVu".to_owned()),
            brand: None,
            operator: Some("Vanguard Plaza Parking Cooperative (Synthetic)".to_owned()),
            camera_type: Some("fixed".to_owned()),
            surveillance_zone: Some("parking".to_owned()),
            camera_direction: Some("45".to_owned()),
            direction: None,
            reference: Some("SYNTH-NM-006".to_owned()),
            check_date: None,
            survey_date: None,
            osm_timestamp: Some("2026-10-26T16:30:00Z".to_owned()),
            source: None,
            note: Some("SYNTHETIC FIXTURE - NOT OBSERVED".to_owned()),
            oia_synthetic: Some("true".to_owned()),
            synthetic: true,
        },
        RawOsmNode {
            element_type: "node".to_owned(),
            osm_id: 900_000_000_107,
            latitude_deg: 37.6189,
            longitude_deg: -121.9074,
            coordinate_zoom: Some(12),
            surveillance_type: Some("ALPR".to_owned()),
            manufacturer: None,
            brand: None,
            operator: None,
            camera_type: Some("fixed".to_owned()),
            surveillance_zone: Some("traffic".to_owned()),
            camera_direction: Some("125".to_owned()),
            direction: None,
            reference: Some("SYNTH-NM-007".to_owned()),
            check_date: None,
            survey_date: None,
            osm_timestamp: Some("2026-10-28T22:15:42Z".to_owned()),
            source: Some("synthetic_fixture".to_owned()),
            note: Some("SYNTHETIC FIXTURE - NOT OBSERVED".to_owned()),
            oia_synthetic: Some("true".to_owned()),
            synthetic: true,
        },
        RawOsmNode {
            element_type: "node".to_owned(),
            osm_id: 900_000_000_108,
            latitude_deg: 37.6264,
            longitude_deg: -121.8995,
            coordinate_zoom: Some(12),
            surveillance_type: Some("ALPR".to_owned()),
            manufacturer: Some("Flock Safety".to_owned()),
            brand: None,
            operator: Some("Solstice Ridge Civic Association (Synthetic)".to_owned()),
            camera_type: Some("fixed".to_owned()),
            surveillance_zone: Some("street".to_owned()),
            camera_direction: Some("NE".to_owned()),
            direction: None,
            reference: Some("SYNTH-NM-008".to_owned()),
            check_date: Some("2026-10-11".to_owned()),
            survey_date: None,
            osm_timestamp: Some("2026-10-29T09:05:12Z".to_owned()),
            source: None,
            note: Some("SYNTHETIC FIXTURE - NOT OBSERVED".to_owned()),
            oia_synthetic: Some("true".to_owned()),
            synthetic: true,
        },
        RawOsmNode {
            element_type: "node".to_owned(),
            osm_id: 900_000_000_199,
            latitude_deg: 37.6161,
            longitude_deg: -121.8922,
            coordinate_zoom: Some(12),
            surveillance_type: Some("camera".to_owned()),
            manufacturer: None,
            brand: None,
            operator: Some("New Meridian Regional Roadway Authority (Synthetic)".to_owned()),
            camera_type: None,
            surveillance_zone: None,
            camera_direction: Some("90".to_owned()),
            direction: None,
            reference: Some("SYNTH-NM-199".to_owned()),
            check_date: None,
            survey_date: None,
            osm_timestamp: Some("2026-10-30T12:00:00Z".to_owned()),
            source: None,
            note: Some("SYNTHETIC FIXTURE - NOT OBSERVED".to_owned()),
            oia_synthetic: Some("true".to_owned()),
            synthetic: true,
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
    let requested = match GeoBBox::new(37.605, -121.915, 37.635, -121.875) {
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

    println!(
        "origin=synthetic-fixture corridor=\"New Meridian Synthetic Corridor\" requirement=CAM-06 source=OpenStreetMap-ODbL-1.0"
    );
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
            "camera id={} badge={} synthetic={} class=\"{}\" operator={:?} ref={:?} pos=({:.4},{:.4}) wedge={} check_date={:?} osm_timestamp={:?} note={:?}",
            record.id,
            record.display_id(),
            record.synthetic,
            record.manufacturer_class.short_label(),
            record.operator,
            record.reference,
            record.latitude_deg,
            record.longitude_deg,
            wedge_summary,
            record.last_verified,
            record.osm_timestamp,
            record.note
        );
    }

    match snapshot.coverage_status {
        AlprCoverageStatus::Saturated { .. } => ExitCode::from(2),
        _ => ExitCode::SUCCESS,
    }
}
