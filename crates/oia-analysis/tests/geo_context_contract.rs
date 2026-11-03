//! Targeted contract tests for geographic context reconstruction (`GEO-02`, `GEO-05`, `INTEL-03`, `A04`).

use oia_analysis::geo_context::provisional::{
    Altitude, AltitudeBand, AltitudeReference, GeoBoundingBoxWgs84, GeoPointWgs84, GeoPosition,
    TimeWindow,
};
use oia_analysis::geo_context::{
    AircraftTrack, AircraftTrackPoint, CloudCoverage, CloudLayer, ContextLimits,
    ContextReconstructionInputs, GeoContextError, MaritimeTrackPoint, MaritimeVesselTrack,
    NavigationNotice, NavigationalStatus, NoticeEnvelopeOverlap, NoticeKind,
    NoticePositionIntersection, NoticeSpatialBoundary, ObservationSource, PositionSource,
    SceneTrafficAssessment, SensorCoverageMask, SoundingLevel, SpatioTemporalEnvelope, StopReason,
    UnobservedReason, VesselIdentity, VesselType, WeatherRecord, WeatherReportKind,
    reconstruct_geographic_context, spherical_slerp,
};
use oia_domain::{OpaqueId, VersionId};

/// Risk: Missing radar, ADS-B, or AIS sensor feeds reported as an empty scene (`GEO-05`, `A04`, B1′).
///
/// Verifies:
/// 1. Zero observed tracks with no registered sensors produces `InconclusiveUnobserved(NoSensorsCoveringRegion)`.
/// 2. Zero observed tracks with an offline or stale sensor produces `InconclusiveUnobserved(SensorFeedDownOrStale)`.
/// 3. Zero observed tracks with sensor whose feed stopped before the envelope window ends produces `SensorFeedDownOrStale`.
/// 4. Envelope without altitude band (unbounded column) against bounded sensor produces `IncompleteAltitudeCoverage`.
/// 5. Zero observed tracks with verified, fully active operational sensor masks produces `ObservedAbsenceWithinCoverage`.
#[test]
fn test_empty_scene_contract_prevents_unfounded_absence_claim()
-> Result<(), Box<dyn std::error::Error>> {
    let bbox = GeoBoundingBoxWgs84::new(10.0, 12.0, 20.0, 22.0)?;
    let time_window = TimeWindow::new(1_700_000_000_000, 1_700_003_600_000)?;
    let alt_band = AltitudeBand::new(
        Altitude::new(0.0, AltitudeReference::OrthometricMsl)?,
        Altitude::new(15_000.0, AltitudeReference::OrthometricMsl)?,
    )?;
    let envelope = SpatioTemporalEnvelope::new(bbox, Some(alt_band), time_window);
    let limits = ContextLimits::default();

    // Case 1: No sensors registered in the region -> InconclusiveUnobserved(NoSensorsCoveringRegion)
    let inputs_no_sensors = ContextReconstructionInputs {
        weather_records: &[],
        aircraft_tracks: &[],
        vessel_tracks: &[],
        notices: &[],
        sensor_masks: &[],
    };
    let res_no_sensors = reconstruct_geographic_context(
        &envelope,
        &inputs_no_sensors,
        &limits,
        1_700_000_600_000,
        None,
    )?;
    assert_eq!(
        res_no_sensors.traffic_assessment,
        SceneTrafficAssessment::InconclusiveUnobserved {
            reason: UnobservedReason::NoSensorsCoveringRegion,
        }
    );

    // Case 2: Sensor exists but is offline / not operational -> InconclusiveUnobserved(SensorFeedDownOrStale)
    let offline_sensor = SensorCoverageMask {
        sensor_id: OpaqueId::new("sensor:synth-radar-01")?,
        source_id: OpaqueId::new("synth:source:provider-01")?,
        coverage_bounds: GeoBoundingBoxWgs84::new(9.0, 13.0, 19.0, 23.0)?,
        altitude_band: Some(AltitudeBand::new(
            Altitude::new(0.0, AltitudeReference::OrthometricMsl)?,
            Altitude::new(20_000.0, AltitudeReference::OrthometricMsl)?,
        )?),
        operational_window: TimeWindow::new(1_699_000_000_000, 1_701_000_000_000)?,
        sensor_type: PositionSource::PrimaryRadar,
        is_operational: false, // Offline!
        last_feed_update_utc_ms: 1_700_004_000_000,
    };
    let masks_offline = [offline_sensor];
    let inputs_offline = ContextReconstructionInputs {
        weather_records: &[],
        aircraft_tracks: &[],
        vessel_tracks: &[],
        notices: &[],
        sensor_masks: &masks_offline,
    };
    let res_offline = reconstruct_geographic_context(
        &envelope,
        &inputs_offline,
        &limits,
        1_700_000_600_000,
        None,
    )?;
    assert_eq!(
        res_offline.traffic_assessment,
        SceneTrafficAssessment::InconclusiveUnobserved {
            reason: UnobservedReason::SensorFeedDownOrStale,
        }
    );

    // Case 3: Sensor feed stopped reporting BEFORE the envelope time window ended -> SensorFeedDownOrStale
    let premature_end_sensor = SensorCoverageMask {
        sensor_id: OpaqueId::new("sensor:synth-radar-02")?,
        source_id: OpaqueId::new("synth:source:provider-01")?,
        coverage_bounds: GeoBoundingBoxWgs84::new(9.0, 13.0, 19.0, 23.0)?,
        altitude_band: Some(AltitudeBand::new(
            Altitude::new(0.0, AltitudeReference::OrthometricMsl)?,
            Altitude::new(20_000.0, AltitudeReference::OrthometricMsl)?,
        )?),
        operational_window: TimeWindow::new(1_699_000_000_000, 1_701_000_000_000)?,
        sensor_type: PositionSource::PrimaryRadar,
        is_operational: true,
        // Feed stopped at t=1_700_002_000_000 (before envelope end 1_700_003_600_000)
        last_feed_update_utc_ms: 1_700_002_000_000,
    };
    let masks_premature = [premature_end_sensor];
    let inputs_premature = ContextReconstructionInputs {
        weather_records: &[],
        aircraft_tracks: &[],
        vessel_tracks: &[],
        notices: &[],
        sensor_masks: &masks_premature,
    };
    let res_premature = reconstruct_geographic_context(
        &envelope,
        &inputs_premature,
        &limits,
        1_700_000_600_000,
        None,
    )?;
    assert_eq!(
        res_premature.traffic_assessment,
        SceneTrafficAssessment::InconclusiveUnobserved {
            reason: UnobservedReason::SensorFeedDownOrStale,
        }
    );

    // Case 4: Envelope has NO altitude band (unbounded vertical column), but sensor is bounded -> IncompleteAltitudeCoverage
    let unbounded_envelope = SpatioTemporalEnvelope::new(bbox, None, time_window);
    let bounded_sensor = SensorCoverageMask {
        sensor_id: OpaqueId::new("sensor:synth-radar-03")?,
        source_id: OpaqueId::new("synth:source:provider-01")?,
        coverage_bounds: GeoBoundingBoxWgs84::new(9.0, 13.0, 19.0, 23.0)?,
        altitude_band: Some(AltitudeBand::new(
            Altitude::new(0.0, AltitudeReference::OrthometricMsl)?,
            Altitude::new(20_000.0, AltitudeReference::OrthometricMsl)?,
        )?),
        operational_window: TimeWindow::new(1_699_000_000_000, 1_701_000_000_000)?,
        sensor_type: PositionSource::PrimaryRadar,
        is_operational: true,
        last_feed_update_utc_ms: 1_700_004_000_000,
    };
    let masks_bounded = [bounded_sensor];
    let inputs_bounded = ContextReconstructionInputs {
        weather_records: &[],
        aircraft_tracks: &[],
        vessel_tracks: &[],
        notices: &[],
        sensor_masks: &masks_bounded,
    };
    let res_bounded = reconstruct_geographic_context(
        &unbounded_envelope,
        &inputs_bounded,
        &limits,
        1_700_000_600_000,
        None,
    )?;
    assert_eq!(
        res_bounded.traffic_assessment,
        SceneTrafficAssessment::InconclusiveUnobserved {
            reason: UnobservedReason::IncompleteAltitudeCoverage,
        }
    );

    // Case 5: Sensor is operational, covers the envelope completely, and reported through window end -> ObservedAbsenceWithinCoverage
    let operational_sensor = SensorCoverageMask {
        sensor_id: OpaqueId::new("sensor:synth-adsb-01")?,
        source_id: OpaqueId::new("synth:source:provider-01")?,
        coverage_bounds: GeoBoundingBoxWgs84::new(9.0, 13.0, 19.0, 23.0)?,
        altitude_band: Some(AltitudeBand::new(
            Altitude::new(0.0, AltitudeReference::OrthometricMsl)?,
            Altitude::new(20_000.0, AltitudeReference::OrthometricMsl)?,
        )?),
        operational_window: TimeWindow::new(1_699_000_000_000, 1_701_000_000_000)?,
        sensor_type: PositionSource::Adsb,
        is_operational: true,
        last_feed_update_utc_ms: 1_700_004_000_000, // Fresh past window end
    };
    let masks_operational = [operational_sensor];
    let inputs_covered = ContextReconstructionInputs {
        weather_records: &[],
        aircraft_tracks: &[],
        vessel_tracks: &[],
        notices: &[],
        sensor_masks: &masks_operational,
    };
    let res_covered = reconstruct_geographic_context(
        &envelope,
        &inputs_covered,
        &limits,
        1_700_000_600_000,
        None,
    )?;
    assert!(matches!(
        res_covered.traffic_assessment,
        SceneTrafficAssessment::ObservedAbsenceWithinCoverage {
            verified_covering_sensor_count: 1,
            ..
        }
    ));

    Ok(())
}

/// Risk: Interpolated flight states masquerading as direct observations, or linear interpolation
/// departing from great circles (B3′, B6′).
#[test]
fn test_observation_versus_interpolation_separation_and_uncertainty_growth()
-> Result<(), Box<dyn std::error::Error>> {
    let limits = ContextLimits {
        max_interpolation_gap_ms: 300_000,
        base_observation_uncertainty_m: 15.0,
        interpolation_growth_rate_mps: 2.0,
        ..ContextLimits::default()
    };

    // Unallocated ICAO test address (000001-003FFF reserved block per ICAO Annex 10)
    let p1 = AircraftTrackPoint {
        transmit_instant_utc_ms: Some(1_000_000),
        receiver_instant_utc_ms: Some(1_000_000),
        retrieval_instant_utc_ms: Some(1_000_100),
        target_interpolation_instant_utc_ms: None,
        position: GeoPointWgs84::new(10.0, 20.0)?,
        barometric_altitude: Some(Altitude::new(
            10_000.0,
            AltitudeReference::BarometricUnadjusted,
        )?),
        geometric_altitude: Some(Altitude::new(10_050.0, AltitudeReference::Wgs84Ellipsoid)?),
        ground_speed_mps: Some(250.0),
        track_angle_deg: Some(90.0),
        vertical_rate_mps: Some(0.0),
        squawk_octal: Some("1200".to_string()),
        observation_source: ObservationSource::DirectObservation,
        raw_source_hash: Some([0x01; 32]),
    };
    let p2 = AircraftTrackPoint {
        transmit_instant_utc_ms: Some(1_120_000), // 120 seconds later
        receiver_instant_utc_ms: Some(1_120_000),
        retrieval_instant_utc_ms: Some(1_120_100),
        target_interpolation_instant_utc_ms: None,
        position: GeoPointWgs84::new(10.0, 20.4)?,
        barometric_altitude: Some(Altitude::new(
            10_000.0,
            AltitudeReference::BarometricUnadjusted,
        )?),
        geometric_altitude: Some(Altitude::new(10_060.0, AltitudeReference::Wgs84Ellipsoid)?),
        ground_speed_mps: Some(250.0),
        track_angle_deg: Some(90.0),
        vertical_rate_mps: Some(0.0),
        squawk_octal: Some("1200".to_string()),
        observation_source: ObservationSource::DirectObservation,
        raw_source_hash: Some([0x02; 32]),
    };

    let track = AircraftTrack::new(
        "000001",
        Some("SIM123".to_string()),
        Some("SIM738".to_string()),
        PositionSource::Adsb,
        OpaqueId::new("synth:source:station-01")?,
        vec![p1.clone(), p2.clone()],
        &limits,
    )?;

    // 1. Exact match returns direct observation with observed timestamps
    let exact = track.interpolate_at(1_000_000, &limits)?;
    assert_eq!(
        exact.observation_source,
        ObservationSource::DirectObservation
    );
    assert_eq!(exact.receiver_instant_utc_ms, Some(1_000_000));
    assert_eq!(exact.target_interpolation_instant_utc_ms, None);

    // 2. Midpoint produces InterpolatedSegment with uncertainty growth and method ID
    let mid = track.interpolate_at(1_060_000, &limits)?;
    assert_eq!(mid.target_interpolation_instant_utc_ms, Some(1_060_000));
    assert_eq!(mid.receiver_instant_utc_ms, None); // B6': No fabricated receiver time!
    assert_eq!(mid.transmit_instant_utc_ms, None);
    assert_eq!(mid.retrieval_instant_utc_ms, None);

    match mid.observation_source {
        ObservationSource::InterpolatedSegment {
            elapsed_gap_ms,
            distance_gap_m,
            uncertainty_radius_m,
            method_id,
            base_uncertainty_m,
            growth_rate_mps,
        } => {
            assert_eq!(elapsed_gap_ms, 120_000);
            assert!(distance_gap_m > 30_000.0);
            assert_eq!(method_id, "bounded-great-circle-spherical-v1");
            assert_eq!(base_uncertainty_m, 15.0);
            assert_eq!(growth_rate_mps, 2.0);
            // Midpoint uncertainty: base 15m + 2.0 m/s * 120s = 255m
            assert!((uncertainty_radius_m - 255.0).abs() < 1e-6);
        }
        ObservationSource::DirectObservation => {
            return Err("midpoint must be an InterpolatedSegment".into());
        }
    }
    assert!((mid.position.latitude_deg() - 10.0).abs() < 1e-4);
    assert!((mid.position.longitude_deg() - 20.2).abs() < 1e-4);

    // 3. Excessive gap must be rejected
    let p_distant = AircraftTrackPoint {
        transmit_instant_utc_ms: Some(1_600_000), // 600s gap (exceeds 300s limit)
        receiver_instant_utc_ms: Some(1_600_000),
        retrieval_instant_utc_ms: Some(1_600_100),
        target_interpolation_instant_utc_ms: None,
        position: GeoPointWgs84::new(10.0, 21.0)?,
        barometric_altitude: Some(Altitude::new(
            10_000.0,
            AltitudeReference::BarometricUnadjusted,
        )?),
        geometric_altitude: None,
        ground_speed_mps: None,
        track_angle_deg: None,
        vertical_rate_mps: None,
        squawk_octal: None,
        observation_source: ObservationSource::DirectObservation,
        raw_source_hash: None,
    };
    let track_gap = AircraftTrack::new(
        "000001",
        None,
        None,
        PositionSource::Adsb,
        OpaqueId::new("synth:source:station-01")?,
        vec![p1, p_distant],
        &limits,
    )?;
    let gap_err = track_gap.interpolate_at(1_300_000, &limits);
    assert!(matches!(
        gap_err,
        Err(GeoContextError::InterpolationGapExceeded { .. })
    ));

    Ok(())
}

/// Verifies that spherical SLERP follows the exact great circle at high latitudes (B3′).
#[test]
fn test_spherical_slerp_great_circle_accuracy() -> Result<(), Box<dyn std::error::Error>> {
    let p1 = GeoPointWgs84::new(79.0, 0.0)?;
    let p2 = GeoPointWgs84::new(79.0, 20.0)?;

    let mid = spherical_slerp(p1, p2, 0.5)?;

    // Great circle bows northward towards the pole from latitude 79.0°
    assert!(mid.latitude_deg() > 79.0);
    assert!((mid.longitude_deg() - 10.0).abs() < 1e-5);

    // Distance from p1 to mid equals distance from mid to p2
    let d1 = p1.great_circle_distance_m(mid);
    let d2 = mid.great_circle_distance_m(p2);
    assert!((d1 - d2).abs() < 1.0); // within 1 metre symmetry!

    Ok(())
}

/// Risk: Maritime AIS vessel tracks failing validation or fabricating locations (B9, B1′).
#[test]
fn test_maritime_vessel_track_and_interpolation() -> Result<(), Box<dyn std::error::Error>> {
    let limits = ContextLimits::default();
    // Unallocated ITU MID 111 (SAR prefix under ITU-R M.585)
    // No commercial IMO number is assigned.
    let identity = VesselIdentity {
        mmsi: 111_987_654,
        vessel_name: Some("SYNTHETIC RESEARCH SHIP".to_string()),
        imo_number: None,
        callsign: Some("SIMV01".to_string()),
        vessel_type: VesselType::SearchAndRescue,
        nav_status: NavigationalStatus::UnderwayUsingEngine,
        draught_m: Some(8.5),
    };

    let p1 = MaritimeTrackPoint {
        transmit_instant_utc_ms: Some(10_000),
        receiver_instant_utc_ms: Some(10_050),
        retrieval_instant_utc_ms: Some(10_100),
        target_interpolation_instant_utc_ms: None,
        position: GeoPointWgs84::new(20.0, -150.0)?,
        speed_over_ground_mps: Some(10.0),
        course_over_ground_deg: Some(90.0),
        true_heading_deg: Some(90.0),
        observation_source: ObservationSource::DirectObservation,
    };
    let p2 = MaritimeTrackPoint {
        transmit_instant_utc_ms: Some(70_000), // 60 seconds later
        receiver_instant_utc_ms: Some(70_050),
        retrieval_instant_utc_ms: Some(70_100),
        target_interpolation_instant_utc_ms: None,
        position: GeoPointWgs84::new(20.0, -149.994)?,
        speed_over_ground_mps: Some(10.0),
        course_over_ground_deg: Some(90.0),
        true_heading_deg: Some(90.0),
        observation_source: ObservationSource::DirectObservation,
    };

    let track = MaritimeVesselTrack::new(
        identity.clone(),
        OpaqueId::new("synth:source:coastal-receiver")?,
        vec![p1, p2],
        &limits,
    )?;

    let interpolated = track.interpolate_at(40_000, &limits)?;
    assert!(matches!(
        interpolated.observation_source,
        ObservationSource::InterpolatedSegment { .. }
    ));
    assert_eq!(
        interpolated.target_interpolation_instant_utc_ms,
        Some(40_000)
    );
    assert_eq!(interpolated.receiver_instant_utc_ms, None);

    // Invalid MMSI check
    let bad_identity = VesselIdentity {
        mmsi: 12345, // Invalid MMSI (not 9 digits)
        vessel_name: None,
        imo_number: None,
        callsign: None,
        vessel_type: VesselType::Unknown,
        nav_status: NavigationalStatus::Unknown,
        draught_m: None,
    };
    let bad_mmsi = MaritimeVesselTrack::new(
        bad_identity,
        OpaqueId::new("synth:source:coastal-receiver")?,
        track.points().to_vec(),
        &limits,
    );
    assert!(matches!(bad_mmsi, Err(GeoContextError::InvalidMmsi(12345))));

    // Deliberately failing IMO check digit: for 1_000_000, 1*7 % 10 = 7 != 0
    let bad_imo_identity = VesselIdentity {
        imo_number: Some(1_000_000),
        ..identity
    };
    let bad_imo = MaritimeVesselTrack::new(
        bad_imo_identity,
        OpaqueId::new("synth:source:coastal-receiver")?,
        track.points().to_vec(),
        &limits,
    );
    assert!(matches!(
        bad_imo,
        Err(GeoContextError::InvalidImoNumber(1_000_000))
    ));

    Ok(())
}

/// Risk: Weather forecasts masquerading as observations, or corrupted weather data (`GEO-02`, `GEO-05`, B6′).
#[test]
fn test_weather_observation_versus_forecast_separation_and_soundings()
-> Result<(), Box<dyn std::error::Error>> {
    let station_pos = GeoPosition::new(
        GeoPointWgs84::new(10.5, 20.5)?, // Abstract synthetic coordinates
        Some(Altitude::new(4.0, AltitudeReference::OrthometricMsl)?),
    );

    // Valid observation
    let valid_obs = WeatherRecord {
        source_id: OpaqueId::new("synth:source:station-01")?,
        version_id: VersionId::new("v20231102t1800z")?,
        station_id: OpaqueId::new("station:synth-station-01")?,
        station_position: station_pos,
        kind: WeatherReportKind::DirectObservation,
        observation_instant_utc_ms: Some(1_700_000_000_000),
        forecast_issuance_utc_ms: None,
        retrieval_instant_utc_ms: 1_700_000_050_000,
        valid_window: TimeWindow::new(1_700_000_000_000, 1_700_003_600_000)?,
        model_id: None,
        temperature_c: Some(15.0),
        dew_point_c: Some(10.0),
        relative_humidity_pct: Some(72.0),
        wind_speed_mps: Some(5.0),
        wind_direction_deg: Some(180.0),
        wind_gust_mps: None,
        visibility_m: Some(10_000.0),
        visibility_restriction: None,
        pressure_altimeter_hpa: Some(1013.25),
        cloud_layers: vec![CloudLayer {
            coverage: CloudCoverage::Scattered,
            base_altitude: Altitude::new(1500.0, AltitudeReference::AboveGroundLevel)?,
            top_altitude: None,
        }],
        sounding_profile: vec![SoundingLevel {
            altitude: Altitude::new(3000.0, AltitudeReference::OrthometricMsl)?,
            temperature_c: 2.0,
            pressure_hpa: 700.0,
            wind_speed_mps: 15.0,
            wind_direction_deg: 210.0,
        }],
        raw_text_hash_sha256: [0x77; 32],
    };
    assert!(valid_obs.validate().is_ok());

    // Invalid observation: missing observation_instant_utc_ms
    let invalid_obs = WeatherRecord {
        observation_instant_utc_ms: None,
        ..valid_obs.clone()
    };
    assert!(invalid_obs.validate().is_err());

    // Invalid observation: observation timestamp outside valid window
    let invalid_obs_time = WeatherRecord {
        observation_instant_utc_ms: Some(1_699_000_000_000),
        ..valid_obs.clone()
    };
    assert!(invalid_obs_time.validate().is_err());

    // Invalid forecast: forecast issuance after valid window start
    let invalid_forecast = WeatherRecord {
        kind: WeatherReportKind::ForecastModel,
        observation_instant_utc_ms: None,
        forecast_issuance_utc_ms: Some(1_700_001_000_000), // After start (1_700_000_000_000)!
        model_id: Some(OpaqueId::new("model:synth-nwp")?),
        ..valid_obs
    };
    assert!(invalid_forecast.validate().is_err());

    Ok(())
}

/// Risk: Longitude wrapping across the 180° antimeridian causing incorrect out-of-bounds
/// classifications or distance calculation overflow (`A04`).
#[test]
fn test_dateline_crossing_spatial_containment_and_great_circle_distance()
-> Result<(), Box<dyn std::error::Error>> {
    let dateline_box = GeoBoundingBoxWgs84::new(-10.0, 10.0, 170.0, -170.0)?;
    assert!(dateline_box.crosses_antimeridian());

    // Inside dateline box: 175.0°E
    let pt_east = GeoPointWgs84::new(0.0, 175.0)?;
    assert!(dateline_box.contains_point(pt_east));

    // Inside dateline box: -175.0°W
    let pt_west = GeoPointWgs84::new(0.0, -175.0)?;
    assert!(dateline_box.contains_point(pt_west));

    // Outside dateline box: 0.0° (Prime Meridian)
    let pt_prime = GeoPointWgs84::new(0.0, 0.0)?;
    assert!(!dateline_box.contains_point(pt_prime));

    // Great circle distance across dateline: from 179° to -179° at equator is ~2 degrees = ~222.4 km
    let pt_a = GeoPointWgs84::new(0.0, 179.0)?;
    let pt_b = GeoPointWgs84::new(0.0, -179.0)?;
    let dist_m = pt_a.great_circle_distance_m(pt_b);
    let expected_approx_m = 2.0 * (std::f64::consts::PI / 180.0) * 6_371_000.0;
    assert!((dist_m - expected_approx_m).abs() < 1000.0);

    Ok(())
}

/// Risk: Notice near the dateline missed due to naive longitude clamp (B5′).
#[test]
fn test_navigation_notice_dateline_boundary_intersection() -> Result<(), Box<dyn std::error::Error>>
{
    // Envelope 170..179.5 (not wrapping)
    let envelope_box = GeoBoundingBoxWgs84::new(-5.0, 5.0, 170.0, 179.5)?;
    assert!(!envelope_box.crosses_antimeridian());

    // Circle centered at (0.0, -179.9), radius 100 km (distance to 179.5 is ~66.7 km)
    let boundary = NoticeSpatialBoundary::Circle {
        center: GeoPointWgs84::new(0.0, -179.9)?,
        radius_m: 100_000.0,
    };

    // Must intersect box across the dateline boundary!
    assert!(boundary.intersects_box(&envelope_box));

    let notice = NavigationNotice {
        notice_id: OpaqueId::new("notam:synth-boundary-test")?,
        issuing_authority: "Synthetic Airspace Agency".to_string(),
        notice_kind: NoticeKind::AirspaceRestriction,
        issuance_instant_utc_ms: 1_699_990_000_000,
        retrieval_instant_utc_ms: 1_700_000_000_000,
        effective_window: TimeWindow::new(1_700_000_000_000, 1_700_010_000_000)?,
        spatial_boundary: boundary,
        lower_altitude: None,
        upper_altitude: None,
        summary: "Boundary test notice".to_string(),
        raw_content_sha256: [0x42; 32],
    };

    let envelope = SpatioTemporalEnvelope::new(
        envelope_box,
        None,
        TimeWindow::new(1_700_000_000_000, 1_700_010_000_000)?,
    );
    assert_eq!(
        notice.overlaps_envelope(&envelope),
        NoticeEnvelopeOverlap::DefiniteOverlap
    );

    Ok(())
}

/// Risk: Direct observation with missing or incompatible altitude treated as absence (B1′).
#[test]
fn test_unresolved_altitude_observation_prevents_false_absence()
-> Result<(), Box<dyn std::error::Error>> {
    let limits = ContextLimits::default();
    let bbox = GeoBoundingBoxWgs84::new(10.0, 12.0, 20.0, 22.0)?;
    let time_window = TimeWindow::new(1_000_000, 1_200_000)?;
    let alt_band = AltitudeBand::new(
        Altitude::new(5_000.0, AltitudeReference::OrthometricMsl)?,
        Altitude::new(10_000.0, AltitudeReference::OrthometricMsl)?,
    )?;
    let envelope = SpatioTemporalEnvelope::new(bbox, Some(alt_band), time_window);

    // Aircraft direct observation inside 2D box and time window, but with NO altitude measurement:
    let p_no_alt = AircraftTrackPoint {
        transmit_instant_utc_ms: Some(1_100_000),
        receiver_instant_utc_ms: Some(1_100_000),
        retrieval_instant_utc_ms: Some(1_100_100),
        target_interpolation_instant_utc_ms: None,
        position: GeoPointWgs84::new(11.0, 21.0)?, // inside 2D box!
        barometric_altitude: None,
        geometric_altitude: None,
        ground_speed_mps: None,
        track_angle_deg: None,
        vertical_rate_mps: None,
        squawk_octal: None,
        observation_source: ObservationSource::DirectObservation,
        raw_source_hash: None,
    };
    let track = AircraftTrack::new(
        "000001",
        Some("SIM101".to_string()),
        None,
        PositionSource::Adsb,
        OpaqueId::new("synth:source:feeder")?,
        vec![p_no_alt],
        &limits,
    )?;

    // Register a sensor that fully covers 4D envelope
    let sensor = SensorCoverageMask {
        sensor_id: OpaqueId::new("sensor:synth-radar-01")?,
        source_id: OpaqueId::new("synth:source:provider-01")?,
        coverage_bounds: GeoBoundingBoxWgs84::new(9.0, 13.0, 19.0, 23.0)?,
        altitude_band: Some(AltitudeBand::new(
            Altitude::new(0.0, AltitudeReference::OrthometricMsl)?,
            Altitude::new(20_000.0, AltitudeReference::OrthometricMsl)?,
        )?),
        operational_window: TimeWindow::new(900_000, 1_300_000)?,
        sensor_type: PositionSource::PrimaryRadar,
        is_operational: true,
        last_feed_update_utc_ms: 1_250_000,
    };

    let aircraft = [track];
    let masks = [sensor];
    let inputs = ContextReconstructionInputs {
        weather_records: &[],
        aircraft_tracks: &aircraft,
        vessel_tracks: &[],
        notices: &[],
        sensor_masks: &masks,
    };

    let result = reconstruct_geographic_context(&envelope, &inputs, &limits, 1_100_000, None)?;

    // B1' contract: because an observation exists with unknown altitude inside the envelope box/time,
    // we MUST NOT declare ObservedAbsenceWithinCoverage! It must be InconclusiveUnobserved(IncompleteAltitudeCoverage)!
    assert_eq!(
        result.traffic_assessment,
        SceneTrafficAssessment::InconclusiveUnobserved {
            reason: UnobservedReason::IncompleteAltitudeCoverage,
        }
    );

    Ok(())
}

/// Risk: Infinite loops, unchecked resource exhaustion, or overflow panics (`AGENTS.md` §5, B8′).
#[test]
fn test_cancellation_resource_limits_and_overflow_safety() -> Result<(), Box<dyn std::error::Error>>
{
    // Test extreme time window: i64::MIN to i64::MAX does not panic on midpoint calculation
    let extreme_window = TimeWindow::new(0, i64::MAX)?;
    let envelope = SpatioTemporalEnvelope::new(
        GeoBoundingBoxWgs84::new(10.0, 20.0, 10.0, 20.0)?,
        None,
        extreme_window,
    );
    let limits = ContextLimits {
        max_tracks: 1,
        ..ContextLimits::default()
    };

    // 1. Cooperative cancellation
    let mut cancel_called = false;
    let mut cancel_fn = || {
        cancel_called = true;
        Err(StopReason::Cancelled)
    };
    let empty_inputs = ContextReconstructionInputs {
        weather_records: &[],
        aircraft_tracks: &[],
        vessel_tracks: &[],
        notices: &[],
        sensor_masks: &[],
    };
    let res = reconstruct_geographic_context(
        &envelope,
        &empty_inputs,
        &limits,
        150,
        Some(&mut cancel_fn),
    );
    assert_eq!(res, Err(GeoContextError::Stopped(StopReason::Cancelled)));
    assert!(cancel_called);

    // 2. Duplicate timestamp rejection within a track
    let p_dup1 = AircraftTrackPoint {
        transmit_instant_utc_ms: Some(100),
        receiver_instant_utc_ms: Some(100),
        retrieval_instant_utc_ms: Some(150),
        target_interpolation_instant_utc_ms: None,
        position: GeoPointWgs84::new(15.0, 15.0)?,
        barometric_altitude: None,
        geometric_altitude: None,
        ground_speed_mps: None,
        track_angle_deg: None,
        vertical_rate_mps: None,
        squawk_octal: None,
        observation_source: ObservationSource::DirectObservation,
        raw_source_hash: None,
    };
    let p_dup2 = p_dup1.clone();
    let dup_track_res = AircraftTrack::new(
        "000001",
        None,
        None,
        PositionSource::Adsb,
        OpaqueId::new("synth:source:station-01")?,
        vec![p_dup1, p_dup2],
        &limits,
    );
    assert!(matches!(
        dup_track_res,
        Err(GeoContextError::DuplicateTimestamp(100))
    ));

    // 3. Limits validation rejects NaN or negative growth
    let bad_limits = ContextLimits {
        interpolation_growth_rate_mps: -1.0,
        ..ContextLimits::default()
    };
    assert!(bad_limits.validate().is_err());

    Ok(())
}

/// N1 Contract: A vessel operating at sea level cannot be assigned 0m in an ellipsoidal datum (N1).
/// Against an ellipsoidal band, vertical overlap is unresolvable and must yield InconclusiveUnobserved.
#[test]
fn test_n1_vessel_surface_altitude_and_ellipsoid_datum_inconclusive()
-> Result<(), Box<dyn std::error::Error>> {
    let limits = ContextLimits::default();
    let bbox = GeoBoundingBoxWgs84::new(10.0, 12.0, 20.0, 22.0)?;
    let time_window = TimeWindow::new(1_000_000, 1_200_000)?;

    // Query envelope uses an ellipsoidal altitude band [10m, 1000m]
    let ellipsoid_band = AltitudeBand::new(
        Altitude::new(10.0, AltitudeReference::Wgs84Ellipsoid)?,
        Altitude::new(1000.0, AltitudeReference::Wgs84Ellipsoid)?,
    )?;
    let envelope = SpatioTemporalEnvelope::new(bbox, Some(ellipsoid_band), time_window);

    // Vessel inside the 2D bounding box and time window
    let identity = VesselIdentity {
        mmsi: 111_000_002,
        vessel_name: Some("SYNTHETIC PATROL".to_string()),
        imo_number: None,
        callsign: Some("SIMV02".to_string()),
        vessel_type: VesselType::SearchAndRescue,
        nav_status: NavigationalStatus::UnderwayUsingEngine,
        draught_m: Some(5.0),
    };
    let pt = MaritimeTrackPoint {
        transmit_instant_utc_ms: Some(1_100_000),
        receiver_instant_utc_ms: Some(1_100_050),
        retrieval_instant_utc_ms: Some(1_100_100),
        target_interpolation_instant_utc_ms: None,
        position: GeoPointWgs84::new(11.0, 21.0)?, // Inside 2D box
        speed_over_ground_mps: Some(8.0),
        course_over_ground_deg: Some(90.0),
        true_heading_deg: Some(90.0),
        observation_source: ObservationSource::DirectObservation,
    };
    let vessel_track = MaritimeVesselTrack::new(
        identity,
        OpaqueId::new("synth:source:ais-satellite")?,
        vec![pt],
        &limits,
    )?;

    // Fully active sensor mask covering the envelope horizontally and vertically
    let sensor = SensorCoverageMask {
        sensor_id: OpaqueId::new("sensor:synth-radar-01")?,
        source_id: OpaqueId::new("synth:source:provider-01")?,
        coverage_bounds: GeoBoundingBoxWgs84::new(9.0, 13.0, 19.0, 23.0)?,
        altitude_band: Some(AltitudeBand::new(
            Altitude::new(0.0, AltitudeReference::Wgs84Ellipsoid)?,
            Altitude::new(20_000.0, AltitudeReference::Wgs84Ellipsoid)?,
        )?),
        operational_window: TimeWindow::new(900_000, 1_300_000)?,
        sensor_type: PositionSource::PrimaryRadar,
        is_operational: true,
        last_feed_update_utc_ms: 1_250_000,
    };
    let masks = [sensor];

    let inputs = ContextReconstructionInputs {
        weather_records: &[],
        aircraft_tracks: &[],
        vessel_tracks: &[vessel_track],
        notices: &[],
        sensor_masks: &masks,
    };

    let result = reconstruct_geographic_context(&envelope, &inputs, &limits, 1_250_000, None)?;

    // Crucial N1 assertion: must NOT assert ObservedAbsenceWithinCoverage!
    // Because the vessel's vertical overlap against an ellipsoidal band is not comparable.
    assert_eq!(
        result.traffic_assessment,
        SceneTrafficAssessment::InconclusiveUnobserved {
            reason: UnobservedReason::IncompleteAltitudeCoverage,
        }
    );

    Ok(())
}

/// N2 Contract: Retrieval time must never be used as event time, and direct observations
/// must require a transmit or receiver time (N2).
#[test]
fn test_n2_retrieval_time_not_event_time_and_required_tx_rx()
-> Result<(), Box<dyn std::error::Error>> {
    // 1. Direct aircraft observation with only retrieval time is rejected
    let bad_pt = AircraftTrackPoint {
        transmit_instant_utc_ms: None,
        receiver_instant_utc_ms: None,
        retrieval_instant_utc_ms: Some(1_700_000_000_000),
        target_interpolation_instant_utc_ms: None,
        position: GeoPointWgs84::new(10.0, 20.0)?,
        barometric_altitude: None,
        geometric_altitude: None,
        ground_speed_mps: None,
        track_angle_deg: None,
        vertical_rate_mps: None,
        squawk_octal: None,
        observation_source: ObservationSource::DirectObservation,
        raw_source_hash: None,
    };
    assert!(matches!(
        bad_pt.validate(),
        Err(GeoContextError::InvalidTemporalOrdering(_))
    ));
    // effective_instant_utc_ms returns None (never retrieval time)
    assert_eq!(bad_pt.effective_instant_utc_ms(), None);

    // 2. Direct maritime observation with only retrieval time is rejected
    let bad_vessel_pt = MaritimeTrackPoint {
        transmit_instant_utc_ms: None,
        receiver_instant_utc_ms: None,
        retrieval_instant_utc_ms: Some(1_700_000_000_000),
        target_interpolation_instant_utc_ms: None,
        position: GeoPointWgs84::new(10.0, 20.0)?,
        speed_over_ground_mps: None,
        course_over_ground_deg: None,
        true_heading_deg: None,
        observation_source: ObservationSource::DirectObservation,
    };
    assert!(matches!(
        bad_vessel_pt.validate(),
        Err(GeoContextError::InvalidTemporalOrdering(_))
    ));
    assert_eq!(bad_vessel_pt.effective_instant_utc_ms(), None);

    Ok(())
}

/// N3 Contract: Notices with mixed vertical datums (e.g. SFC AGL to FL180 barometric)
/// must be reported as vertical overlap unknown rather than dropped silently (N3).
#[test]
fn test_n3_mixed_datum_notice_reported_as_vertical_overlap_unknown()
-> Result<(), Box<dyn std::error::Error>> {
    let limits = ContextLimits::default();
    let bbox = GeoBoundingBoxWgs84::new(10.0, 12.0, 20.0, 22.0)?;
    let time_window = TimeWindow::new(1_000_000, 1_200_000)?;

    let envelope_band = AltitudeBand::new(
        Altitude::new(1_000.0, AltitudeReference::OrthometricMsl)?,
        Altitude::new(3_000.0, AltitudeReference::OrthometricMsl)?,
    )?;
    let envelope = SpatioTemporalEnvelope::new(bbox, Some(envelope_band), time_window);

    // Mixed datum notice: SFC in AGL, top in BarometricUnadjusted (FL180 ~ 5500m)
    let mixed_notice = NavigationNotice {
        notice_id: OpaqueId::new("notice:notam-mixed-01")?,
        issuing_authority: "SIMULATED-CAA".to_string(),
        notice_kind: NoticeKind::AirspaceRestriction,
        issuance_instant_utc_ms: 950_000,
        retrieval_instant_utc_ms: 960_000,
        effective_window: TimeWindow::new(990_000, 1_300_000)?,
        spatial_boundary: NoticeSpatialBoundary::BoundingBox(GeoBoundingBoxWgs84::new(
            9.0, 13.0, 19.0, 23.0,
        )?),
        lower_altitude: Some(Altitude::new(0.0, AltitudeReference::AboveGroundLevel)?),
        upper_altitude: Some(Altitude::new(
            5500.0,
            AltitudeReference::BarometricUnadjusted,
        )?),
        summary: "Mixed datum airspace restriction".to_string(),
        raw_content_sha256: [0x55; 32],
    };
    mixed_notice.validate()?;

    // overlaps_envelope must classify as VerticalOverlapUnknown
    assert_eq!(
        mixed_notice.overlaps_envelope(&envelope),
        NoticeEnvelopeOverlap::VerticalOverlapUnknown
    );

    // intersects_position against MSL position must return VerticalOverlapUnknown
    let test_pos = GeoPosition::new(
        GeoPointWgs84::new(10.0, 20.0)?,
        Some(Altitude::new(2000.0, AltitudeReference::OrthometricMsl)?),
    );
    assert_eq!(
        mixed_notice.intersects_position(&test_pos, 1_000_000),
        NoticePositionIntersection::VerticalOverlapUnknown
    );

    let inputs = ContextReconstructionInputs {
        weather_records: &[],
        aircraft_tracks: &[],
        vessel_tracks: &[],
        notices: &[mixed_notice],
        sensor_masks: &[],
    };

    let result = reconstruct_geographic_context(&envelope, &inputs, &limits, 1_250_000, None)?;

    // Crucial N3 assertion: notice is counted in vertical_overlap_unknown_count and recorded in
    // notices_with_vertical_overlap_unknown, NOT dropped silently!
    assert_eq!(result.active_notices.len(), 0);
    assert_eq!(result.notices_with_vertical_overlap_unknown.len(), 1);
    assert_eq!(result.notice_summary.active_matching_notices_count, 0);
    assert_eq!(result.notice_summary.vertical_overlap_unknown_count, 1);

    // Validation rejects NaN radius and retrieval before issuance
    let bad_radius_notice = NavigationNotice {
        spatial_boundary: NoticeSpatialBoundary::Circle {
            center: GeoPointWgs84::new(10.0, 20.0)?,
            radius_m: f64::NAN,
        },
        ..result.notices_with_vertical_overlap_unknown[0].clone()
    };
    assert!(bad_radius_notice.validate().is_err());

    let bad_time_notice = NavigationNotice {
        issuance_instant_utc_ms: 1_000_000,
        retrieval_instant_utc_ms: 900_000, // before issuance!
        ..result.notices_with_vertical_overlap_unknown[0].clone()
    };
    assert!(bad_time_notice.validate().is_err());

    Ok(())
}
