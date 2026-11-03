//! Example demonstrating geographic context reconstruction (`GEO-02`, `GEO-05`, `INTEL-03`, `A04`).
//!
//! Demonstrates:
//! 1. Weather observation vs. forecast distinction and sounding profiles.
//! 2. Direct aircraft track observations vs. bounded spherical great-circle interpolated states with uncertainty growth.
//! 3. Maritime vessel tracks and active navigation notices with dateline-aware spatial intersection.
//! 4. Empty-scene prevention: absent tracks produce `InconclusiveUnobserved` when sensors are missing, offline, or stale.
//! 5. Dateline-crossing spatial query envelope (crossing 180° antimeridian).

use oia_analysis::geo_context::provisional::{
    Altitude, AltitudeBand, AltitudeReference, GeoBoundingBoxWgs84, GeoPointWgs84, GeoPosition,
    TimeWindow,
};
use oia_analysis::geo_context::{
    AircraftTrack, AircraftTrackPoint, CloudCoverage, CloudLayer, ContextLimits,
    ContextReconstructionInputs, METHOD_ID, MaritimeTrackPoint, MaritimeVesselTrack,
    NavigationNotice, NavigationalStatus, NoticeKind, NoticeSpatialBoundary, ObservationSource,
    PositionSource, SceneTrafficAssessment, SensorCoverageMask, SoundingLevel,
    SpatioTemporalEnvelope, VesselIdentity, VesselType, WeatherRecord, WeatherReportKind,
    reconstruct_geographic_context,
};
use oia_domain::{OpaqueId, VersionId};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== OIA Geographic Context Reconstruction Example ===");
    println!("Method: {METHOD_ID}\n");

    let limits = ContextLimits::default();

    // 1. Spatio-temporal query envelope crossing the 180° antimeridian (A04)
    // Spans from 175.0°E to -175.0°W across the dateline
    let envelope_box = GeoBoundingBoxWgs84::new(20.0, 30.0, 175.0, -175.0)?;
    let time_window = TimeWindow::new(1_700_000_000_000, 1_700_003_600_000)?; // 1 hour window
    let envelope_alt_band = AltitudeBand::new(
        Altitude::new(0.0, AltitudeReference::OrthometricMsl)?,
        Altitude::new(15_000.0, AltitudeReference::OrthometricMsl)?,
    )?;
    let envelope = SpatioTemporalEnvelope::new(envelope_box, Some(envelope_alt_band), time_window);

    println!(
        "Query envelope: lat [{:.1}, {:.1}], lon [{:.1}, {:.1}] (crosses antimeridian: {}), time [{} to {}]",
        envelope.spatial_bounds.south_lat_deg(),
        envelope.spatial_bounds.north_lat_deg(),
        envelope.spatial_bounds.west_lon_deg(),
        envelope.spatial_bounds.east_lon_deg(),
        envelope.spatial_bounds.crosses_antimeridian(),
        envelope.time_window.start_utc_ms(),
        envelope.time_window.end_utc_ms(),
    );

    // 2. Weather records: 1 direct observation with sounding, 1 forecast model
    let weather_station_pos = GeoPosition::new(
        GeoPointWgs84::new(25.0, 178.0)?,
        Some(Altitude::new(15.0, AltitudeReference::OrthometricMsl)?),
    );

    let observation = WeatherRecord {
        source_id: OpaqueId::new("synth:source:weather-station-01")?,
        version_id: VersionId::new("v2023-11-14t2000z")?,
        station_id: OpaqueId::new("station:synth-pacific-01")?,
        station_position: weather_station_pos,
        kind: WeatherReportKind::DirectObservation,
        observation_instant_utc_ms: Some(1_700_001_000_000),
        forecast_issuance_utc_ms: None,
        retrieval_instant_utc_ms: 1_700_001_050_000,
        valid_window: TimeWindow::new(1_700_000_000_000, 1_700_003_600_000)?,
        model_id: None,
        temperature_c: Some(24.5),
        dew_point_c: Some(19.0),
        relative_humidity_pct: Some(71.5),
        wind_speed_mps: Some(8.2),
        wind_direction_deg: Some(60.0),
        wind_gust_mps: Some(11.5),
        visibility_m: Some(10_000.0),
        visibility_restriction: None,
        pressure_altimeter_hpa: Some(1014.2),
        cloud_layers: vec![CloudLayer {
            coverage: CloudCoverage::Few,
            base_altitude: Altitude::new(750.0, AltitudeReference::AboveGroundLevel)?,
            top_altitude: None,
        }],
        sounding_profile: vec![
            SoundingLevel {
                altitude: Altitude::new(1000.0, AltitudeReference::OrthometricMsl)?,
                temperature_c: 18.0,
                pressure_hpa: 900.0,
                wind_speed_mps: 12.0,
                wind_direction_deg: 65.0,
            },
            SoundingLevel {
                altitude: Altitude::new(5000.0, AltitudeReference::OrthometricMsl)?,
                temperature_c: -5.0,
                pressure_hpa: 550.0,
                wind_speed_mps: 25.0,
                wind_direction_deg: 75.0,
            },
        ],
        raw_text_hash_sha256: [0xaa; 32],
    };

    let forecast = WeatherRecord {
        source_id: OpaqueId::new("synth:source:forecast-model-01")?,
        version_id: VersionId::new("run-20231114-12z")?,
        station_id: OpaqueId::new("grid:25n-178w")?,
        station_position: GeoPosition::new(
            GeoPointWgs84::new(25.0, -178.0)?,
            Some(Altitude::new(0.0, AltitudeReference::OrthometricMsl)?),
        ),
        kind: WeatherReportKind::ForecastModel,
        observation_instant_utc_ms: None,
        forecast_issuance_utc_ms: Some(1_699_980_000_000),
        retrieval_instant_utc_ms: 1_699_990_000_000,
        valid_window: TimeWindow::new(1_700_000_000_000, 1_700_007_200_000)?,
        model_id: Some(OpaqueId::new("model:synth-global-v1")?),
        temperature_c: Some(23.8),
        dew_point_c: Some(18.5),
        relative_humidity_pct: Some(72.0),
        wind_speed_mps: Some(9.0),
        wind_direction_deg: Some(65.0),
        wind_gust_mps: None,
        visibility_m: Some(12_000.0),
        visibility_restriction: None,
        pressure_altimeter_hpa: Some(1013.8),
        cloud_layers: vec![],
        sounding_profile: vec![],
        raw_text_hash_sha256: [0xbb; 32],
    };

    // 3. Aircraft Track with direct observations and an interpolated state
    // Unallocated ICAO test address (000000-003FFF reserved block per ICAO Annex 10)
    let track_point_1 = AircraftTrackPoint {
        transmit_instant_utc_ms: Some(1_700_000_500_000),
        receiver_instant_utc_ms: Some(1_700_000_500_050),
        retrieval_instant_utc_ms: Some(1_700_000_500_100),
        target_interpolation_instant_utc_ms: None,
        position: GeoPointWgs84::new(24.5, 178.5)?,
        barometric_altitude: Some(Altitude::new(
            10_668.0,
            AltitudeReference::BarometricUnadjusted,
        )?),
        geometric_altitude: Some(Altitude::new(10_720.0, AltitudeReference::OrthometricMsl)?),
        ground_speed_mps: Some(240.0),
        track_angle_deg: Some(85.0),
        vertical_rate_mps: Some(0.0),
        squawk_octal: Some("7712".to_string()),
        observation_source: ObservationSource::DirectObservation,
        raw_source_hash: Some([0x11; 32]),
    };

    let track_point_2 = AircraftTrackPoint {
        transmit_instant_utc_ms: Some(1_700_000_620_000), // 120 seconds later
        receiver_instant_utc_ms: Some(1_700_000_620_050),
        retrieval_instant_utc_ms: Some(1_700_000_620_100),
        target_interpolation_instant_utc_ms: None,
        position: GeoPointWgs84::new(24.6, 178.8)?,
        barometric_altitude: Some(Altitude::new(
            10_668.0,
            AltitudeReference::BarometricUnadjusted,
        )?),
        geometric_altitude: Some(Altitude::new(10_725.0, AltitudeReference::OrthometricMsl)?),
        ground_speed_mps: Some(242.0),
        track_angle_deg: Some(86.0),
        vertical_rate_mps: Some(0.5),
        squawk_octal: Some("7712".to_string()),
        observation_source: ObservationSource::DirectObservation,
        raw_source_hash: Some([0x22; 32]),
    };

    let flight_track = AircraftTrack::new(
        "000001", // Unallocated ICAO test address
        Some("SIM101".to_string()),
        Some("SIM77W".to_string()),
        PositionSource::Adsb,
        OpaqueId::new("synth:source:feeder-01")?,
        vec![track_point_1, track_point_2],
        &limits,
    )?;

    // Interpolate midpoint
    let target_midpoint = 1_700_000_560_000;
    let interpolated = flight_track.interpolate_at(target_midpoint, &limits)?;
    println!("Aircraft track interpolation at t={target_midpoint}:");
    println!(
        "  Position: ({:.4}, {:.4})",
        interpolated.position.latitude_deg(),
        interpolated.position.longitude_deg()
    );
    if let ObservationSource::InterpolatedSegment {
        elapsed_gap_ms,
        distance_gap_m,
        uncertainty_radius_m,
        method_id,
        ..
    } = interpolated.observation_source
    {
        println!(
            "  Interpolated state (method: {method_id}): gap={elapsed_gap_ms} ms, dist={distance_gap_m:.1} m, uncertainty_radius={uncertainty_radius_m:.2} m"
        );
    }

    // 4. Maritime vessel track
    // Synthetic test vessel: ITU-R Recommendation M.585 SAR aircraft prefix 111
    // No commercial IMO number is assigned.
    let vessel_identity = VesselIdentity {
        mmsi: 111_000_001,
        vessel_name: Some("SYNTHETIC RESEARCH VESSEL".to_string()),
        imo_number: None,
        callsign: Some("SIMV01".to_string()),
        vessel_type: VesselType::Cargo,
        nav_status: NavigationalStatus::UnderwayUsingEngine,
        draught_m: Some(12.5),
    };
    let vessel_track = MaritimeVesselTrack::new(
        vessel_identity,
        OpaqueId::new("synth:source:ais-satellite-01")?,
        vec![
            MaritimeTrackPoint {
                transmit_instant_utc_ms: Some(1_700_000_000_000),
                receiver_instant_utc_ms: Some(1_700_000_000_050),
                retrieval_instant_utc_ms: Some(1_700_000_000_100),
                target_interpolation_instant_utc_ms: None,
                position: GeoPointWgs84::new(22.0, 177.0)?,
                speed_over_ground_mps: Some(9.5),
                course_over_ground_deg: Some(45.0),
                true_heading_deg: Some(44.0),
                observation_source: ObservationSource::DirectObservation,
            },
            MaritimeTrackPoint {
                transmit_instant_utc_ms: Some(1_700_001_800_000),
                receiver_instant_utc_ms: Some(1_700_001_800_050),
                retrieval_instant_utc_ms: Some(1_700_001_800_100),
                target_interpolation_instant_utc_ms: None,
                position: GeoPointWgs84::new(22.1, 177.2)?,
                speed_over_ground_mps: Some(9.6),
                course_over_ground_deg: Some(46.0),
                true_heading_deg: Some(45.0),
                observation_source: ObservationSource::DirectObservation,
            },
        ],
        &limits,
    )?;

    // 5. Navigation Notice: Rocket Launch Corridor
    let notice = NavigationNotice {
        notice_id: OpaqueId::new("notam:synth-launch-corridor-01")?,
        issuing_authority: "Synthetic Aviation & Space Authority".to_string(),
        notice_kind: NoticeKind::RocketLaunchCorridor,
        issuance_instant_utc_ms: 1_699_990_000_000,
        retrieval_instant_utc_ms: 1_700_000_000_000,
        effective_window: TimeWindow::new(1_700_000_000_000, 1_700_007_200_000)?,
        spatial_boundary: NoticeSpatialBoundary::Circle {
            center: GeoPointWgs84::new(24.0, 179.0)?,
            radius_m: 150_000.0,
        },
        lower_altitude: Some(Altitude::new(0.0, AltitudeReference::OrthometricMsl)?),
        upper_altitude: Some(Altitude::new(50_000.0, AltitudeReference::OrthometricMsl)?),
        summary: "Hazard area active for orbital launch ascent corridor".to_string(),
        raw_content_sha256: [0x55; 32],
    };

    // 6. Reconstruction with sensor coverage
    let sensor_mask = SensorCoverageMask {
        sensor_id: OpaqueId::new("sensor:synth-pacific-net")?,
        source_id: OpaqueId::new("synth:source:oceanic-radar-01")?,
        coverage_bounds: GeoBoundingBoxWgs84::new(15.0, 35.0, 170.0, -170.0)?,
        altitude_band: Some(AltitudeBand::new(
            Altitude::new(0.0, AltitudeReference::OrthometricMsl)?,
            Altitude::new(20_000.0, AltitudeReference::OrthometricMsl)?,
        )?),
        operational_window: TimeWindow::new(1_699_900_000_000, 1_701_000_000_000)?,
        sensor_type: PositionSource::Adsb,
        is_operational: true,
        last_feed_update_utc_ms: 1_700_004_000_000, // Covers through envelope end (1_700_003_600_000)
    };

    let weather_slice = [observation, forecast];
    let aircraft_slice = [flight_track];
    let vessel_slice = [vessel_track];
    let notice_slice = [notice];
    let mask_slice = [sensor_mask];

    let inputs = ContextReconstructionInputs {
        weather_records: &weather_slice,
        aircraft_tracks: &aircraft_slice,
        vessel_tracks: &vessel_slice,
        notices: &notice_slice,
        sensor_masks: &mask_slice,
    };

    let result =
        reconstruct_geographic_context(&envelope, &inputs, &limits, 1_700_005_000_000, None)?;

    println!("\nReconstruction Summary:");
    println!(
        "  Weather matching records: {}",
        result.weather_summary.matching_records_count
    );
    println!(
        "  Weather observations: {}",
        result.weather_summary.direct_observations_count
    );
    println!(
        "  Weather forecast models: {}",
        result.weather_summary.forecast_models_count
    );
    println!(
        "  Upper-air soundings: {}",
        result.weather_summary.upper_air_soundings_count
    );
    println!(
        "  Aircraft tracks matching: {}",
        result.aircraft_tracks.len()
    );
    println!(
        "  Maritime vessels matching: {}",
        result.vessel_tracks.len()
    );
    println!("  Active notices: {}", result.active_notices.len());
    println!("  Traffic assessment: {:?}", result.traffic_assessment);

    // 7. Verify the Empty-Scene contract: if 0 tracks and NO sensors, must return InconclusiveUnobserved
    let empty_inputs = ContextReconstructionInputs {
        weather_records: &[],
        aircraft_tracks: &[],
        vessel_tracks: &[],
        notices: &[],
        sensor_masks: &[], // No sensors!
    };
    let empty_result =
        reconstruct_geographic_context(&envelope, &empty_inputs, &limits, 1_700_005_000_000, None)?;
    println!("\nEmpty-Scene Check (zero tracks, zero sensors):");
    match empty_result.traffic_assessment {
        SceneTrafficAssessment::InconclusiveUnobserved { reason } => {
            println!(
                "  Correctly assessed as INCONCLUSIVE ({reason:?}) rather than claiming an empty scene."
            );
        }
        other => {
            return Err(format!(
                "Empty scene contract violation: expected InconclusiveUnobserved, got {other:?}"
            )
            .into());
        }
    }

    println!("\nDone.");
    Ok(())
}
