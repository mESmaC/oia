//! Contract tests for space imagery catalog, scientific provenance, and mission stream kernel (`SPACE-01`–`SPACE-03`, `GEO-04`, `INTEL-03`, `A05`).

use oia_analysis::geo_context::provisional::{GeoBoundingBoxWgs84, TimeWindow};
use oia_analysis::space_imagery::*;

#[test]
fn scientific_provenance_and_outreach_rendering_integrity() -> Result<(), Box<dyn std::error::Error>>
{
    let limits = SpaceCatalogLimits::default();
    let mut catalog = SpaceImageryCatalog::new();

    // 1. Scientific product with FITS format, Level-1B georeferenced calibration, and stacking history
    let mut sci_product = ScientificProductRecord::new(
        "jwst:carina:nircam:f444w_sci",
        ScientificProductFormat::FitsPrimaryImage,
        CalibrationLevel::Level1BGeoreferenced,
        1850.5,
        "calwebb_1.8.2",
    )?;

    let stack_hist = StackingHistory::new(8, StackingMethod::MedianStack, 0.04)?;
    sci_product.stacking_history = Some(stack_hist);
    sci_product.publication_reference = Some("doi:10.17909/T9-STScI-JWST-CARINA".into());

    // 2. Previews: one false-color composite, one outreach aesthetic rendering
    let false_color_preview = ImagePreviewRecord::new(
        "prev:carina:nircam:rgb_composite",
        ColorMapping::FalseColor {
            red_channel_source: "F444W".into(),
            green_channel_source: "F200W".into(),
            blue_channel_source: "F090W".into(),
        },
        2048,
        2048,
        "image/png",
        "https://archive.stsci.edu/previews/jwst/carina_composite.png",
    )?;

    assert!(!false_color_preview.color_mapping.is_outreach_or_simulated());

    let outreach_preview = ImagePreviewRecord::new(
        "prev:carina:nircam:outreach_press",
        ColorMapping::OutreachRendering {
            renderer_attribution: "STScI Outreach Team (J. DePasquale)".into(),
            enhancement_notes: "Non-linear tone stretch and saturation boost for public release"
                .into(),
        },
        3840,
        2160,
        "image/jpeg",
        "https://webbtelescope.org/contents/media/images/2022/031/01G77PKBBM0J5K3",
    )?;

    // Critical contract check: outreach rendering must be explicitly flagged
    assert!(outreach_preview.color_mapping.is_outreach_or_simulated());
    assert_eq!(
        outreach_preview.color_mapping.classification_code(),
        "OUTREACH_RENDERING"
    );

    let provider = SpaceDataProvider::new("STScI MAST", "JWST Science Operations", true)?;
    assert!(provider.is_candidate_catalog_entry);

    let target = SpaceObservationTarget::Celestial {
        target_name: "Carina Nebula NGC 3372".into(),
        ra_deg: 161.26,
        dec_deg: -59.87,
        fov_radius_deg: 0.25,
    };

    let obs_entry = SpaceObservationEntry {
        observation_id: "obs:jwst:carina_2022_01".into(),
        mission: "JWST".into(),
        instrument: "NIRCam".into(),
        target,
        spectral_regime: SpectralRegime::NearInfrared,
        wavelength_range_nm: Some(WavelengthRangeNm::new(600.0, 5000.0)?),
        time_window: TimeWindow::new(1_657_670_400_000, 1_657_672_250_000)?,
        spatial_resolution: Some(SpatialResolution::AngularResolutionArcsec(0.065)),
        provider,
        scientific_product: sci_product,
        previews: vec![false_color_preview, outreach_preview],
    };

    assert!(obs_entry.has_outreach_or_simulated_preview());
    catalog.add_observation(obs_entry, &limits)?;

    // Query 1: Unfiltered query returns the observation
    let q_all = SpaceObservationQuery {
        mission: Some("JWST".into()),
        ..Default::default()
    };
    let res = catalog.query(&q_all, &limits, None)?;
    assert_eq!(res.len(), 1);

    // Query 2: Exclude outreach renderings filters it out because outreach preview is attached
    let q_strict = SpaceObservationQuery {
        mission: Some("JWST".into()),
        exclude_outreach_renderings: true,
        ..Default::default()
    };
    let res_strict = catalog.query(&q_strict, &limits, None)?;
    assert_eq!(res_strict.len(), 0);

    Ok(())
}

#[test]
fn mosaic_coverage_and_absence_without_seamless_illusion() -> Result<(), Box<dyn std::error::Error>>
{
    let limits = SpaceCatalogLimits::default();
    let mut catalog = SpaceImageryCatalog::new();

    // Query envelope over a 1-degree square in the Pacific
    let envelope = GeoBoundingBoxWgs84::new(10.0, 11.0, 140.0, 141.0)?;

    // Case A: Empty catalog -> NoCoverageAvailable
    let report_empty = catalog.evaluate_mosaic_coverage(&envelope, None, &limits)?;
    assert_eq!(
        report_empty.assessment,
        MosaicCoverageAssessment::NoCoverageAvailable
    );

    // Case B: Single partial footprint covering only the SW quadrant
    let partial_footprint = GeoBoundingBoxWgs84::new(9.9, 10.5, 139.9, 140.5)?;
    let provider = SpaceDataProvider::new("USGS/NASA", "Landsat Ground Network", true)?;

    let entry_partial = SpaceObservationEntry {
        observation_id: "obs:landsat9:sw_quadrant".into(),
        mission: "Landsat-9".into(),
        instrument: "OLI-2".into(),
        target: SpaceObservationTarget::EarthObservation {
            footprint: partial_footprint,
            region_label: Some("Western Pacific Tropical".into()),
        },
        spectral_regime: SpectralRegime::OpticalVisible,
        wavelength_range_nm: Some(WavelengthRangeNm::new(430.0, 670.0)?),
        time_window: TimeWindow::new(1_700_000_000_000, 1_700_000_030_000)?,
        spatial_resolution: Some(SpatialResolution::GroundSampleDistanceMeters(30.0)),
        provider: provider.clone(),
        scientific_product: ScientificProductRecord::new(
            "l9:sw:l1t",
            ScientificProductFormat::GeoTiffCalibratedRadiance,
            CalibrationLevel::Level1BGeoreferenced,
            0.025,
            "Collection-2_v02",
        )?,
        previews: Vec::new(),
    };
    catalog.add_observation(entry_partial, &limits)?;

    let report_partial = catalog.evaluate_mosaic_coverage(&envelope, None, &limits)?;

    // Critical contract check: Partial coverage MUST NOT claim complete or seamless coverage
    match report_partial.assessment {
        MosaicCoverageAssessment::PartialCoverageWithGaps { gap_count, .. } => {
            assert!(gap_count > 0);
        }
        _ => return Err("expected PartialCoverageWithGaps".into()),
    }

    // Case C: Add a second observation covering the full envelope
    let full_footprint = GeoBoundingBoxWgs84::new(9.5, 11.5, 139.5, 141.5)?;
    let entry_full = SpaceObservationEntry {
        observation_id: "obs:sentinel2:full_swath".into(),
        mission: "Sentinel-2".into(),
        instrument: "MSI".into(),
        target: SpaceObservationTarget::EarthObservation {
            footprint: full_footprint,
            region_label: Some("Western Pacific Full Swath".into()),
        },
        spectral_regime: SpectralRegime::OpticalVisible,
        wavelength_range_nm: Some(WavelengthRangeNm::new(400.0, 700.0)?),
        time_window: TimeWindow::new(1_700_000_100_000, 1_700_000_130_000)?,
        spatial_resolution: Some(SpatialResolution::GroundSampleDistanceMeters(10.0)),
        provider,
        scientific_product: ScientificProductRecord::new(
            "s2:full:l2a",
            ScientificProductFormat::GeoTiffCalibratedRadiance,
            CalibrationLevel::Level2Geophysical,
            0.015,
            "S2-PDGS-02.14",
        )?,
        previews: Vec::new(),
    };
    catalog.add_observation(entry_full, &limits)?;

    let report_full = catalog.evaluate_mosaic_coverage(&envelope, None, &limits)?;
    match report_full.assessment {
        MosaicCoverageAssessment::CompleteVerifiedCoverage {
            observation_count,
            min_resolution_m,
            max_resolution_m,
        } => {
            assert_eq!(observation_count, 2);
            assert_eq!(min_resolution_m, 10.0);
            assert_eq!(max_resolution_m, 30.0);
        }
        _ => return Err("expected CompleteVerifiedCoverage".into()),
    }

    Ok(())
}

#[test]
fn mission_stream_classification_and_geometry_change_detection()
-> Result<(), Box<dyn std::error::Error>> {
    let limits = SpaceCatalogLimits::default();
    let mut catalog = SpaceImageryCatalog::new();

    // 1. Mission feeds: verify stream classifications and bookmark latency tracking
    let mut iss_feed = SpaceMissionFeed::new(
        "feed:iss:ehdc_live",
        "International Space Station",
        "ISS External High Definition Live Views",
        StreamClassification::LiveCamera,
        MissionFeedStatus::NominalActive,
        -150, // 150 ms clock lead
    )?;

    assert!(iss_feed.classification.is_live());
    assert_eq!(
        iss_feed.classification.classification_token(),
        "LIVE_CAMERA"
    );

    let sim_feed = SpaceMissionFeed::new(
        "feed:artemis:trajectory_sim",
        "Artemis II",
        "Translunar Trajectory Orbital Simulation",
        StreamClassification::SimulationOrRecreation {
            engine: "NASA DOUG Flight Visualization Engine".into(),
        },
        MissionFeedStatus::NominalActive,
        0,
    )?;

    assert!(!sim_feed.classification.is_live());
    assert_eq!(
        sim_feed.classification.classification_token(),
        "SIMULATION_OR_RECREATION"
    );

    let bookmark = MissionFeedBookmark::new(
        "bm:iss:los_event_01",
        1_700_100_000_000,
        &iss_feed.classification,
        "Tracking Ku-band handover with momentary signal fade",
        250,
        "Ku-band satellite relay transition; video held on buffer frame",
    )?;
    iss_feed.add_bookmark(bookmark, &limits)?;

    catalog.add_feed(iss_feed, &limits)?;
    catalog.add_feed(sim_feed, &limits)?;

    // 2. Geometry-aware baseline change detection
    let shared_footprint = GeoBoundingBoxWgs84::new(35.0, 35.5, -115.5, -115.0)?;
    let provider = SpaceDataProvider::new("NASA CMR", "LP DAAC", true)?;

    let base_entry = SpaceObservationEntry {
        observation_id: "obs:landsat:summer".into(),
        mission: "Landsat-8".into(),
        instrument: "OLI".into(),
        target: SpaceObservationTarget::EarthObservation {
            footprint: shared_footprint,
            region_label: Some("Mojave Study Region".into()),
        },
        spectral_regime: SpectralRegime::OpticalVisible,
        wavelength_range_nm: Some(WavelengthRangeNm::new(450.0, 680.0)?),
        time_window: TimeWindow::new(1_688_169_600_000, 1_688_169_630_000)?, // July 1
        spatial_resolution: Some(SpatialResolution::GroundSampleDistanceMeters(30.0)),
        provider: provider.clone(),
        scientific_product: ScientificProductRecord::new(
            "l8:base:sci",
            ScientificProductFormat::GeoTiffCalibratedRadiance,
            CalibrationLevel::Level2Geophysical,
            0.02,
            "C2_L2",
        )?,
        previews: Vec::new(),
    };

    let comp_entry = SpaceObservationEntry {
        observation_id: "obs:landsat:winter".into(),
        mission: "Landsat-9".into(),
        instrument: "OLI-2".into(),
        target: SpaceObservationTarget::EarthObservation {
            footprint: shared_footprint,
            region_label: Some("Mojave Study Region".into()),
        },
        spectral_regime: SpectralRegime::OpticalVisible,
        wavelength_range_nm: Some(WavelengthRangeNm::new(450.0, 680.0)?),
        time_window: TimeWindow::new(1_704_067_200_000, 1_704_067_230_000)?, // Jan 1
        spatial_resolution: Some(SpatialResolution::GroundSampleDistanceMeters(30.0)),
        provider,
        scientific_product: ScientificProductRecord::new(
            "l9:comp:sci",
            ScientificProductFormat::GeoTiffCalibratedRadiance,
            CalibrationLevel::Level2Geophysical,
            0.02,
            "C2_L2",
        )?,
        previews: Vec::new(),
    };

    catalog.add_observation(base_entry, &limits)?;
    catalog.add_observation(comp_entry, &limits)?;

    let geom_summer = ImageryCaptureGeometry::new(68.5, 125.0, 2.5, 182)?;
    let geom_winter = ImageryCaptureGeometry::new(28.0, 155.0, 3.0, 1)?;

    let change_report = catalog.compare_baselines(
        "obs:landsat:summer",
        "obs:landsat:winter",
        &geom_summer,
        &geom_winter,
        &limits,
    )?;

    assert_eq!(change_report.seasonal_day_gap, 181);
    assert!((change_report.sun_elevation_diff_deg - 40.5).abs() < 1e-4);

    // Critical contract check: significant sun angle discrepancy must flag illumination artifact
    assert_eq!(change_report.candidates.len(), 1);
    let cand = match change_report.candidates.first() {
        Some(c) => c,
        None => return Err("expected change candidate".into()),
    };
    assert!(!cand.is_calibrated_probability); // NEVER calibrated probability without held-out eval
    assert!(
        cand.limitations
            .contains(&GeometricLimitation::SignificantSunElevationDifference)
    );
    assert!(
        cand.limitations
            .contains(&GeometricLimitation::SeasonalFoliageOrSnowDiscrepancy)
    );
    assert_eq!(
        cand.review_status,
        ChangeReviewStatus::LikelyIlluminationArtifact
    );

    Ok(())
}
