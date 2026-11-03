//! Targeted contract tests for geographic context reconstruction (`GEO-02`, `GEO-05`, `INTEL-03`, `A04`).

use oia_analysis::geo_context::{
    AircraftTrack, AircraftTrackPoint, Altitude, AltitudeBand, AltitudeReference, CloudCoverage,
    CloudLayer, ContextLimits, ContextReconstructionInputs, GeoBoundingBoxWgs84, GeoContextError,
    GeoPointWgs84, GeoPosition, MaritimeTrackPoint, MaritimeVesselTrack, NavigationNotice,
    NavigationalStatus, NoticeKind, NoticeSpatialBoundary, ObservationSource, PositionSource,
    SceneTrafficAssessment, SensorCoverageMask, SoundingLevel, SpatioTemporalEnvelope, StopReason,
    TimeWindow, UnobservedReason, VesselIdentity, VesselType, WeatherRecord, WeatherReportKind,
    reconstruct_geographic_context,
};
use oia_domain::{OpaqueId, VersionId};

/// Risk: Missing radar, ADS-B, or AIS sensor feeds reported as an empty scene (`GEO-05`, `A04`).
///
/// Verifies:
/// 1. Zero observed tracks with no registered sensors produces `InconclusiveUnobserved(NoSensorsCoveringRegion)`.
/// 2. Zero observed tracks with an offline or stale sensor produces `InconclusiveUnobserved(SensorFeedDownOrStale)`.
/// 3. Zero observed tracks with verified, fully active operational sensor masks produces `ObservedAbsenceWithinCoverage`.
#[test]
fn test_empty_scene_contract_prevents_unfounded_absence_claim()
-> Result<(), Box<dyn std::error::Error>> {
    let bbox = GeoBoundingBoxWgs84::new(30.0, 32.0, -115.0, -113.0)?;
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
        source_id: OpaqueId::new("source:synth-faa")?,
        coverage_bounds: GeoBoundingBoxWgs84::new(29.0, 33.0, -116.0, -112.0)?,
        altitude_band: Some(AltitudeBand::new(
            Altitude::new(0.0, AltitudeReference::OrthometricMsl)?,
            Altitude::new(20_000.0, AltitudeReference::OrthometricMsl)?,
        )?),
        operational_window: TimeWindow::new(1_699_000_000_000, 1_701_000_000_000)?,
        sensor_type: PositionSource::PrimaryRadar,
        is_operational: false, // Offline!
        last_feed_update_utc_ms: 1_700_000_600_000,
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

    // Case 3: Sensor is operational and covers the envelope completely -> ObservedAbsenceWithinCoverage
    let operational_sensor = SensorCoverageMask {
        sensor_id: OpaqueId::new("sensor:synth-adsb-01")?,
        source_id: OpaqueId::new("source:synth-flight-net")?,
        coverage_bounds: GeoBoundingBoxWgs84::new(29.0, 33.0, -116.0, -112.0)?,
        altitude_band: Some(AltitudeBand::new(
            Altitude::new(0.0, AltitudeReference::OrthometricMsl)?,
            Altitude::new(20_000.0, AltitudeReference::OrthometricMsl)?,
        )?),
        operational_window: TimeWindow::new(1_699_000_000_000, 1_701_000_000_000)?,
        sensor_type: PositionSource::Adsb,
        is_operational: true,
        last_feed_update_utc_ms: 1_700_000_550_000, // Fresh (50s old)
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

/// Risk: Interpolated flight states masquerading as direct observations, or interpolating
/// across massive signal dropouts fabricating aircraft tracks (`GEO-05`, `A04`).
#[test]
fn test_observation_versus_interpolation_separation_and_uncertainty_growth()
-> Result<(), Box<dyn std::error::Error>> {
    let limits = ContextLimits {
        max_interpolation_gap_ms: 300_000,
        base_observation_uncertainty_m: 15.0,
        interpolation_growth_rate_mps: 2.0,
        ..ContextLimits::default()
    };

    let p1 = AircraftTrackPoint {
        transmit_instant_utc_ms: 1_000_000,
        receiver_instant_utc_ms: 1_000_000,
        retrieval_instant_utc_ms: 1_000_100,
        position: GeoPointWgs84::new(35.0, -110.0)?,
        barometric_altitude: Some(Altitude::new(
            10_000.0,
            AltitudeReference::BarometricUnadjusted,
        )?),
        geometric_altitude: Some(Altitude::new(10_050.0, AltitudeReference::Wgs84Ellipsoid)?),
        ground_speed_mps: Some(250.0),
        track_angle_deg: Some(90.0),
        vertical_rate_mps: Some(0.0),
        squawk_octal: Some(0o1200),
        observation_source: ObservationSource::DirectObservation,
        raw_source_hash: Some([0x01; 32]),
    };
    let p2 = AircraftTrackPoint {
        transmit_instant_utc_ms: 1_120_000, // 120 seconds later
        receiver_instant_utc_ms: 1_120_000,
        retrieval_instant_utc_ms: 1_120_100,
        position: GeoPointWgs84::new(35.0, -109.6)?,
        barometric_altitude: Some(Altitude::new(
            10_000.0,
            AltitudeReference::BarometricUnadjusted,
        )?),
        geometric_altitude: Some(Altitude::new(10_060.0, AltitudeReference::Wgs84Ellipsoid)?),
        ground_speed_mps: Some(250.0),
        track_angle_deg: Some(90.0),
        vertical_rate_mps: Some(0.0),
        squawk_octal: Some(0o1200),
        observation_source: ObservationSource::DirectObservation,
        raw_source_hash: Some([0x02; 32]),
    };

    let track = AircraftTrack::new(
        "A1B2C3",
        Some("SIM123".to_string()),
        Some("SIM738".to_string()),
        PositionSource::Adsb,
        OpaqueId::new("adsb:synth-ground-station-1")?,
        vec![p1.clone(), p2.clone()],
        &limits,
    )?;

    // 1. Exact match returns direct observation
    let exact = track.interpolate_at(1_000_000, &limits)?;
    assert_eq!(
        exact.observation_source,
        ObservationSource::DirectObservation
    );

    // 2. Midpoint produces InterpolatedSegment with uncertainty growth and method ID
    let mid = track.interpolate_at(1_060_000, &limits)?;
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
            // Midpoint has maximal interpolation uncertainty: base (15m) + 2.0 m/s * 120s = 255m
            assert!(uncertainty_radius_m > 200.0);
        }
        ObservationSource::DirectObservation => {
            return Err("midpoint must be an InterpolatedSegment".into());
        }
    }
    assert!((mid.position.latitude_deg() - 35.0).abs() < 1e-5);
    assert!((mid.position.longitude_deg() - (-109.8)).abs() < 1e-3);

    // 3. Excessive gap must be rejected
    let p_distant = AircraftTrackPoint {
        transmit_instant_utc_ms: 1_600_000, // 600s gap (exceeds 300s limit)
        receiver_instant_utc_ms: 1_600_050,
        retrieval_instant_utc_ms: 1_600_100,
        position: GeoPointWgs84::new(35.0, -108.0)?,
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
        "A1B2C3",
        None,
        None,
        PositionSource::Adsb,
        OpaqueId::new("adsb:synth-ground-station-1")?,
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

/// Risk: Maritime AIS vessel tracks failing validation or fabricating locations across dropouts (`GEO-05`, `INTEL-03`).
#[test]
fn test_maritime_vessel_track_and_interpolation() -> Result<(), Box<dyn std::error::Error>> {
    let limits = ContextLimits::default();
    let identity = VesselIdentity {
        mmsi: 366_987_654,
        vessel_name: Some("SYNTHETIC RESEARCH SHIP".to_string()),
        imo_number: Some(9876543),
        callsign: Some("KABC".to_string()),
        vessel_type: VesselType::SearchAndRescue,
        nav_status: NavigationalStatus::UnderwayUsingEngine,
        draught_m: Some(8.5),
    };

    let p1 = MaritimeTrackPoint {
        transmit_instant_utc_ms: 10_000,
        receiver_instant_utc_ms: 10_050,
        position: GeoPointWgs84::new(20.0, -150.0)?,
        speed_over_ground_mps: Some(10.0),
        course_over_ground_deg: Some(90.0),
        true_heading_deg: Some(90.0),
        observation_source: ObservationSource::DirectObservation,
    };
    let p2 = MaritimeTrackPoint {
        transmit_instant_utc_ms: 70_000, // 60 seconds later
        receiver_instant_utc_ms: 70_050,
        position: GeoPointWgs84::new(20.0, -149.994)?,
        speed_over_ground_mps: Some(10.0),
        course_over_ground_deg: Some(90.0),
        true_heading_deg: Some(90.0),
        observation_source: ObservationSource::DirectObservation,
    };

    let track = MaritimeVesselTrack::new(
        identity.clone(),
        OpaqueId::new("ais:synth-coastal-receiver")?,
        vec![p1, p2],
        &limits,
    )?;

    let interpolated = track.interpolate_at(40_000, &limits)?;
    assert!(matches!(
        interpolated.observation_source,
        ObservationSource::InterpolatedSegment { .. }
    ));
    assert!((interpolated.position.latitude_deg() - 20.0).abs() < 1e-5);

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
        OpaqueId::new("ais:synth-coastal-receiver")?,
        track.points.clone(),
        &limits,
    );
    assert!(matches!(bad_mmsi, Err(GeoContextError::InvalidMmsi(12345))));

    // Invalid IMO check digit (9876542 instead of 9876543)
    let bad_imo_identity = VesselIdentity {
        imo_number: Some(9876542),
        ..identity
    };
    let bad_imo = MaritimeVesselTrack::new(
        bad_imo_identity,
        OpaqueId::new("ais:synth-coastal-receiver")?,
        track.points,
        &limits,
    );
    assert!(matches!(
        bad_imo,
        Err(GeoContextError::InvalidImoNumber(9876542))
    ));

    Ok(())
}

/// Risk: Weather forecasts masquerading as observations, or corrupted weather data (`GEO-02`, `GEO-05`).
#[test]
fn test_weather_observation_versus_forecast_separation_and_soundings()
-> Result<(), Box<dyn std::error::Error>> {
    let station_pos = GeoPosition::new(
        GeoPointWgs84::new(40.6413, -73.7781)?,
        Some(Altitude::new(4.0, AltitudeReference::OrthometricMsl)?),
    );

    // Valid observation
    let valid_obs = WeatherRecord {
        source_id: OpaqueId::new("weather:synth:station-01")?,
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
        observation_instant_utc_ms: Some(1_699_000_000_000), // Far before valid_window start
        ..valid_obs.clone()
    };
    assert!(invalid_obs_time.validate().is_err());

    // Invalid forecast: missing forecast_issuance_utc_ms or model_id
    let invalid_forecast = WeatherRecord {
        kind: WeatherReportKind::ForecastModel,
        observation_instant_utc_ms: None,
        forecast_issuance_utc_ms: None,
        model_id: None,
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
    // Spans across dateline from 170.0°E to -170.0°W (20° longitude span across 180°)
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

    // Outside dateline box: 150.0°E
    let pt_far_east = GeoPointWgs84::new(0.0, 150.0)?;
    assert!(!dateline_box.contains_point(pt_far_east));

    // Great circle distance across dateline: from 179° to -179° at equator is ~2 degrees = ~222.4 km
    let pt_a = GeoPointWgs84::new(0.0, 179.0)?;
    let pt_b = GeoPointWgs84::new(0.0, -179.0)?;
    let dist_m = pt_a.great_circle_distance_m(pt_b);
    let expected_approx_m = 2.0 * (std::f64::consts::PI / 180.0) * 6_371_000.0;
    assert!((dist_m - expected_approx_m).abs() < 1000.0);

    Ok(())
}

/// Risk: Airspace restrictions or hazardous launch corridors fail to intersect position (`INTEL-03`).
#[test]
fn test_navigation_notice_and_launch_corridor_intersection()
-> Result<(), Box<dyn std::error::Error>> {
    let center = GeoPointWgs84::new(28.5721, -80.6480)?;
    let notice = NavigationNotice {
        notice_id: OpaqueId::new("notam:synth-launch-hazard-01")?,
        issuing_authority: "Synthetic Aviation Authority".to_string(),
        notice_kind: NoticeKind::RocketLaunchCorridor,
        issuance_instant_utc_ms: 1_699_990_000_000,
        retrieval_instant_utc_ms: 1_700_000_000_000,
        effective_window: TimeWindow::new(1_700_000_000_000, 1_700_010_000_000)?,
        spatial_boundary: NoticeSpatialBoundary::Circle {
            center,
            radius_m: 50_000.0, // 50 km radius
        },
        lower_altitude: Some(Altitude::new(0.0, AltitudeReference::OrthometricMsl)?),
        upper_altitude: Some(Altitude::new(30_000.0, AltitudeReference::OrthometricMsl)?),
        summary: "Hazard area active for commercial launch".to_string(),
        raw_content_sha256: [0x42; 32],
    };

    // Position inside the hazard area: 20 km east of launch site at 10,000m altitude
    let pos_inside = GeoPosition::new(
        GeoPointWgs84::new(28.5721, -80.4400)?,
        Some(Altitude::new(10_000.0, AltitudeReference::OrthometricMsl)?),
    );
    assert!(notice.intersects_position(&pos_inside, 1_700_005_000_000)?);

    // Position outside altitude band: at 35,000m altitude (above upper limit)
    let pos_high = GeoPosition::new(
        GeoPointWgs84::new(28.5721, -80.4400)?,
        Some(Altitude::new(35_000.0, AltitudeReference::OrthometricMsl)?),
    );
    assert!(!notice.intersects_position(&pos_high, 1_700_005_000_000)?);

    // Time outside notice window
    assert!(!notice.intersects_position(&pos_inside, 1_700_020_000_000)?);

    Ok(())
}

/// Risk: Cross-datum altitude comparison produces erroneous collision or containment claims (B4).
#[test]
fn test_vertical_datum_separation_and_incompatible_datums() -> Result<(), Box<dyn std::error::Error>>
{
    let msl_alt = Altitude::new(10_000.0, AltitudeReference::OrthometricMsl)?;
    let wgs84_alt = Altitude::new(10_000.0, AltitudeReference::Wgs84Ellipsoid)?;
    let baro_alt = Altitude::new(10_000.0, AltitudeReference::BarometricUnadjusted)?;

    // Direct comparison between different datums must fail closed with IncompatibleAltitudeReferences
    assert!(matches!(
        msl_alt.compare_in_same_reference(&wgs84_alt),
        Err(GeoContextError::IncompatibleAltitudeReferences { .. })
    ));
    assert!(matches!(
        msl_alt.compare_in_same_reference(&baro_alt),
        Err(GeoContextError::IncompatibleAltitudeReferences { .. })
    ));

    // Notice intersection with position in a different datum must fail closed
    let notice = NavigationNotice {
        notice_id: OpaqueId::new("notam:synth-restriction-01")?,
        issuing_authority: "Synthetic Aviation Authority".to_string(),
        notice_kind: NoticeKind::AirspaceRestriction,
        issuance_instant_utc_ms: 1_699_990_000_000,
        retrieval_instant_utc_ms: 1_700_000_000_000,
        effective_window: TimeWindow::new(1_700_000_000_000, 1_700_010_000_000)?,
        spatial_boundary: NoticeSpatialBoundary::Circle {
            center: GeoPointWgs84::new(30.0, -100.0)?,
            radius_m: 50_000.0,
        },
        lower_altitude: Some(Altitude::new(0.0, AltitudeReference::OrthometricMsl)?),
        upper_altitude: Some(Altitude::new(20_000.0, AltitudeReference::OrthometricMsl)?),
        summary: "Restricted area".to_string(),
        raw_content_sha256: [0x33; 32],
    };

    let pos_incompatible_datum = GeoPosition::new(
        GeoPointWgs84::new(30.0, -100.0)?,
        Some(Altitude::new(10_000.0, AltitudeReference::Wgs84Ellipsoid)?),
    );

    assert!(matches!(
        notice.intersects_position(&pos_incompatible_datum, 1_700_005_000_000),
        Err(GeoContextError::IncompatibleAltitudeReferences { .. })
    ));

    Ok(())
}

/// Risk: Interpolated flight states masquerading as direct observations in traffic counts (B2).
#[test]
fn test_derived_interpolated_states_do_not_count_as_observed_traffic()
-> Result<(), Box<dyn std::error::Error>> {
    let limits = ContextLimits::default();

    // Query envelope: lat [30.0, 31.0], lon [-110.0, -109.0]
    let bbox = GeoBoundingBoxWgs84::new(30.0, 31.0, -110.0, -109.0)?;
    let time_window = TimeWindow::new(1_000_000, 1_200_000)?;
    let envelope = SpatioTemporalEnvelope::new(bbox, None, time_window);

    // Track has an interpolated point inside the envelope: (30.5, -109.5) at t=1_060_000,
    // but its observation_source is InterpolatedSegment (not a DirectObservation).
    let p_interp = AircraftTrackPoint {
        transmit_instant_utc_ms: 1_060_000,
        receiver_instant_utc_ms: 1_060_000,
        retrieval_instant_utc_ms: 1_060_100,
        position: GeoPointWgs84::new(30.5, -109.5)?, // inside envelope!
        barometric_altitude: None,
        geometric_altitude: None,
        ground_speed_mps: None,
        track_angle_deg: None,
        vertical_rate_mps: None,
        squawk_octal: None,
        observation_source: ObservationSource::InterpolatedSegment {
            method_id: "bounded-great-circle-spherical-v1",
            elapsed_gap_ms: 120_000,
            distance_gap_m: 30_000.0,
            uncertainty_radius_m: 255.0,
            base_uncertainty_m: 15.0,
            growth_rate_mps: 2.0,
        },
        raw_source_hash: None,
    };

    let track = AircraftTrack::new(
        "A00001",
        Some("SIM101".to_string()),
        None,
        PositionSource::Adsb,
        OpaqueId::new("adsb:synth-feeder")?,
        vec![p_interp],
        &limits,
    )?;

    // Register a fully operational sensor covering the envelope
    let sensor = SensorCoverageMask {
        sensor_id: OpaqueId::new("sensor:synth-radar-01")?,
        source_id: OpaqueId::new("source:synth-faa")?,
        coverage_bounds: GeoBoundingBoxWgs84::new(29.0, 32.0, -113.0, -106.0)?,
        altitude_band: None,
        operational_window: TimeWindow::new(900_000, 1_300_000)?,
        sensor_type: PositionSource::PrimaryRadar,
        is_operational: true,
        last_feed_update_utc_ms: 1_100_000,
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

    // B2 contract: track is matched in aircraft_tracks (since its bounding box overlaps),
    // but observed_aircraft_count must be 0 because 0 points fall inside envelope!
    // And since 0 points fall inside and sensor is fully covering, assessment is ObservedAbsenceWithinCoverage!
    assert_eq!(result.aircraft_tracks.len(), 1);
    match result.traffic_assessment {
        SceneTrafficAssessment::ObservedAbsenceWithinCoverage {
            verified_covering_sensor_count,
            ..
        } => {
            assert_eq!(verified_covering_sensor_count, 1);
        }
        other => {
            return Err(format!(
                "expected ObservedAbsenceWithinCoverage when all observations are outside envelope, got {other:?}"
            ).into());
        }
    }

    Ok(())
}

/// Risk: Infinite loops, unchecked resource exhaustion, or ignored cooperative cancellation (`AGENTS.md` §5).
#[test]
fn test_cancellation_and_resource_limits() -> Result<(), Box<dyn std::error::Error>> {
    let envelope = SpatioTemporalEnvelope::new(
        GeoBoundingBoxWgs84::new(10.0, 20.0, 10.0, 20.0)?,
        None,
        TimeWindow::new(100, 200)?,
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

    // 2. Limit exceeded: 2 tracks provided when limit is 1
    let pt = AircraftTrackPoint {
        transmit_instant_utc_ms: 150,
        receiver_instant_utc_ms: 150,
        retrieval_instant_utc_ms: 150,
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
    let track1 = AircraftTrack::new(
        "A00001",
        None,
        None,
        PositionSource::Adsb,
        OpaqueId::new("src:1")?,
        vec![pt.clone()],
        &limits,
    )?;
    let track2 = AircraftTrack::new(
        "A00002",
        None,
        None,
        PositionSource::Adsb,
        OpaqueId::new("src:2")?,
        vec![pt],
        &limits,
    )?;

    let aircraft = [track1, track2];
    let inputs_exceeded = ContextReconstructionInputs {
        weather_records: &[],
        aircraft_tracks: &aircraft,
        vessel_tracks: &[],
        notices: &[],
        sensor_masks: &[],
    };
    let res_limit = reconstruct_geographic_context(&envelope, &inputs_exceeded, &limits, 150, None);
    assert!(matches!(
        res_limit,
        Err(GeoContextError::LimitExceeded { .. })
    ));

    Ok(())
}
