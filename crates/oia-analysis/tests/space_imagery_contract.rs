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
        MosaicCoverageAssessment::PartialCoverageWithGaps {
            uncovered_area_fraction,
            uncovered_bands_count,
            ..
        } => {
            assert!(uncovered_area_fraction > 0.0);
            assert!(uncovered_bands_count > 0);
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
            assert_eq!(min_resolution_m, Some(10.0));
            assert_eq!(max_resolution_m, Some(30.0));
        }
        _ => return Err("expected CompleteVerifiedCoverage".into()),
    }

    // Case D (B2 regression test): Four corner tiles on envelope [0, 10] x [0, 10] MUST NOT
    // report CompleteVerifiedCoverage when center is uncovered
    let mut corner_catalog = SpaceImageryCatalog::new();
    let corner_env = GeoBoundingBoxWgs84::new(0.0, 10.0, 0.0, 10.0)?;
    let corner_boxes = [
        GeoBoundingBoxWgs84::new(0.0, 1.0, 0.0, 1.0)?,
        GeoBoundingBoxWgs84::new(0.0, 1.0, 9.0, 10.0)?,
        GeoBoundingBoxWgs84::new(9.0, 10.0, 0.0, 1.0)?,
        GeoBoundingBoxWgs84::new(9.0, 10.0, 9.0, 10.0)?,
    ];
    let corner_provider = SpaceDataProvider::new("CornerTest", "Archive", true)?;
    for (idx, cb) in corner_boxes.iter().enumerate() {
        let entry = SpaceObservationEntry {
            observation_id: format!("obs:corner:{idx}"),
            mission: "TestMission".into(),
            instrument: "TestInst".into(),
            target: SpaceObservationTarget::EarthObservation {
                footprint: *cb,
                region_label: None,
            },
            spectral_regime: SpectralRegime::OpticalVisible,
            wavelength_range_nm: None,
            time_window: TimeWindow::new(1_700_000_000_000, 1_700_000_010_000)?,
            spatial_resolution: Some(SpatialResolution::GroundSampleDistanceMeters(15.0)),
            provider: corner_provider.clone(),
            scientific_product: ScientificProductRecord::new(
                format!("prod:corner:{idx}"),
                ScientificProductFormat::RawCcdArray,
                CalibrationLevel::Level0RawTelemetry,
                1.0,
                "v1",
            )?,
            previews: Vec::new(),
        };
        corner_catalog.add_observation(entry, &limits)?;
    }

    let corner_report = corner_catalog.evaluate_mosaic_coverage(&corner_env, None, &limits)?;
    match corner_report.assessment {
        MosaicCoverageAssessment::PartialCoverageWithGaps {
            uncovered_area_fraction,
            uncovered_bands_count,
            ..
        } => {
            // Envelope area is 100, four 1x1 corners cover 4 units; uncovered fraction is 96%
            assert!((uncovered_area_fraction - 0.96).abs() < 1e-4);
            assert!(uncovered_bands_count > 0);
        }
        MosaicCoverageAssessment::CompleteVerifiedCoverage { .. } => {
            return Err(
                "four disconnected corners must NEVER produce CompleteVerifiedCoverage".into(),
            );
        }
        MosaicCoverageAssessment::NoCoverageAvailable => {
            return Err("expected PartialCoverageWithGaps, got NoCoverageAvailable".into());
        }
    }

    Ok(())
}

#[test]
fn resolution_accounting_preserves_unavailable_data() -> Result<(), Box<dyn std::error::Error>> {
    let limits = SpaceCatalogLimits::default();
    let mut catalog = SpaceImageryCatalog::new();
    let env = GeoBoundingBoxWgs84::new(20.0, 21.0, 30.0, 31.0)?;

    // Observation with spatial_resolution = None
    let entry_no_res = SpaceObservationEntry {
        observation_id: "obs:no_res".into(),
        mission: "NoResSat".into(),
        instrument: "ExperimentalSensor".into(),
        target: SpaceObservationTarget::EarthObservation {
            footprint: env,
            region_label: None,
        },
        spectral_regime: SpectralRegime::Radio,
        wavelength_range_nm: None,
        time_window: TimeWindow::new(1_700_000_000_000, 1_700_000_010_000)?,
        spatial_resolution: None,
        provider: SpaceDataProvider::new("ProviderA", "ServiceA", true)?,
        scientific_product: ScientificProductRecord::new(
            "p:no_res",
            ScientificProductFormat::NetCdf4,
            CalibrationLevel::Level1ARadiometric,
            5.0,
            "v1",
        )?,
        previews: Vec::new(),
    };
    catalog.add_observation(entry_no_res, &limits)?;

    let report = catalog.evaluate_mosaic_coverage(&env, None, &limits)?;
    // Footprint MUST have None for ground_sample_distance_m (not 30.0)
    let fp = match report.footprints.first() {
        Some(f) => f,
        None => return Err("expected footprint".into()),
    };
    assert_eq!(fp.ground_sample_distance_m, None);

    match report.assessment {
        MosaicCoverageAssessment::CompleteVerifiedCoverage {
            min_resolution_m,
            max_resolution_m,
            ..
        } => {
            // Must be None when no resolutions exist
            assert_eq!(min_resolution_m, None);
            assert_eq!(max_resolution_m, None);
        }
        _ => return Err("expected CompleteVerifiedCoverage".into()),
    }

    Ok(())
}

#[test]
fn nested_metadata_revalidation_and_collection_limits_at_admission()
-> Result<(), Box<dyn std::error::Error>> {
    let limits = SpaceCatalogLimits {
        max_stack_inputs: 4,
        max_previews_per_observation: 2,
        max_bookmarks_per_feed: 2,
        ..Default::default()
    };
    let mut catalog = SpaceImageryCatalog::new();

    // 1. Direct construction bypass attempt with NaN/negative wavelength range
    let mut invalid_entry = SpaceObservationEntry {
        observation_id: "obs:invalid_wave".into(),
        mission: "Test".into(),
        instrument: "Test".into(),
        target: SpaceObservationTarget::Celestial {
            target_name: "Vega".into(),
            ra_deg: 279.23,
            dec_deg: 38.78,
            fov_radius_deg: 1.0,
        },
        spectral_regime: SpectralRegime::OpticalVisible,
        wavelength_range_nm: Some(WavelengthRangeNm {
            min_nm: f64::NAN,
            max_nm: -1.0,
        }),
        time_window: TimeWindow::new(1_700_000_000_000, 1_700_000_010_000)?,
        spatial_resolution: None,
        provider: SpaceDataProvider::new("Prov", "Archive", true)?,
        scientific_product: ScientificProductRecord::new(
            "p:invalid",
            ScientificProductFormat::FitsPrimaryImage,
            CalibrationLevel::Level0RawTelemetry,
            1.0,
            "v1",
        )?,
        previews: Vec::new(),
    };

    assert!(
        catalog
            .add_observation(invalid_entry.clone(), &limits)
            .is_err()
    );
    assert_eq!(catalog.observations().len(), 0);

    // 2. Direct construction bypass attempt with stack exceeding limit or NaN residual
    invalid_entry.wavelength_range_nm = None;
    invalid_entry.scientific_product.stacking_history = Some(StackingHistory {
        input_frame_count: 100, // exceeds max_stack_inputs (4)
        method: StackingMethod::MeanWithOutlierRejection,
        alignment_residual_error: f64::NAN,
    });
    assert!(
        catalog
            .add_observation(invalid_entry.clone(), &limits)
            .is_err()
    );
    assert_eq!(catalog.observations().len(), 0);

    // 3. Excessive previews collection exceeding max_previews_per_observation (2)
    invalid_entry.scientific_product.stacking_history = None;
    invalid_entry.previews = vec![
        ImagePreviewRecord::new("p1", ColorMapping::TrueColor, 100, 100, "image/png", "u1")?,
        ImagePreviewRecord::new("p2", ColorMapping::TrueColor, 100, 100, "image/png", "u2")?,
        ImagePreviewRecord::new("p3", ColorMapping::TrueColor, 100, 100, "image/png", "u3")?,
    ];
    assert!(catalog.add_observation(invalid_entry, &limits).is_err());
    assert_eq!(catalog.observations().len(), 0);

    // 4. Feed admission revalidation with prefilled bookmarks exceeding limits
    let mut feed = SpaceMissionFeed::new(
        "feed:flood",
        "Mission",
        "Title",
        StreamClassification::LiveCamera,
        MissionFeedStatus::NominalActive,
        0,
    )?;
    let bm1 = feed.create_bookmark("bm1", 1_700_000_000_000, "d1", 10, "lim")?;
    let bm2 = feed.create_bookmark("bm2", 1_700_000_001_000, "d2", 10, "lim")?;
    let bm3 = feed.create_bookmark("bm3", 1_700_000_002_000, "d3", 10, "lim")?;
    feed.bookmarks = vec![bm1, bm2, bm3]; // 3 bookmarks, max is 2
    assert!(catalog.add_feed(feed, &limits).is_err());
    assert_eq!(catalog.feeds().len(), 0);

    Ok(())
}

#[test]
fn mission_stream_classification_and_bookmark_timing_integrity()
-> Result<(), Box<dyn std::error::Error>> {
    let limits = SpaceCatalogLimits::default();
    let mut catalog = SpaceImageryCatalog::new();

    let iss_feed = SpaceMissionFeed::new(
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

    let mut delayed_feed_10 = SpaceMissionFeed::new(
        "feed:mars:relay_10s",
        "Mars Reconnaissance Orbiter",
        "Mars Relay 10s Delay",
        StreamClassification::DelayedTransmission {
            delay_seconds: 10.0,
            delay_reason: "Near orbital pass buffer".into(),
        },
        MissionFeedStatus::NominalActive,
        0,
    )?;

    let delayed_feed_3600 = SpaceMissionFeed::new(
        "feed:mars:relay_3600s",
        "Mars Reconnaissance Orbiter",
        "Mars Relay 3600s Delay",
        StreamClassification::DelayedTransmission {
            delay_seconds: 3600.0,
            delay_reason: "Deep space speed-of-light propagation".into(),
        },
        MissionFeedStatus::NominalActive,
        0,
    )?;

    // B5 regression: Bookmarks from 10s and 3600s delay streams must NOT be equal
    let bm_10 = delayed_feed_10.create_bookmark(
        "bm:mars:event",
        1_700_000_000_000,
        "Entry descent landing cue",
        50,
        "Buffered relay",
    )?;
    let bm_3600 = delayed_feed_3600.create_bookmark(
        "bm:mars:event",
        1_700_000_000_000,
        "Entry descent landing cue",
        50,
        "Buffered relay",
    )?;

    assert_ne!(bm_10, bm_3600);
    assert_eq!(bm_10.estimated_event_utc_ms, 1_700_000_000_000 - 10_000);
    assert_eq!(
        bm_3600.estimated_event_utc_ms,
        1_700_000_000_000 - 3_600_000
    );

    // B5 association check: feed rejects bookmark constructed for another feed
    assert!(delayed_feed_10.add_bookmark(bm_3600, &limits).is_err());
    delayed_feed_10.add_bookmark(bm_10, &limits)?;

    catalog.add_feed(delayed_feed_10, &limits)?;
    catalog.add_feed(iss_feed, &limits)?;
    assert_eq!(catalog.feeds().len(), 2);

    Ok(())
}

#[test]
fn geometry_aware_baseline_comparison_never_fabricates_detections()
-> Result<(), Box<dyn std::error::Error>> {
    let limits = SpaceCatalogLimits::default();
    let mut catalog = SpaceImageryCatalog::new();

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

    // Critical B1 contract check: Without a raster detector, candidates MUST BE EMPTY
    // and status MUST BE RasterDetectorNotPerformed (never fabricated 0.72 score!)
    assert_eq!(
        change_report.analysis_status,
        ChangeAnalysisStatus::RasterDetectorNotPerformed
    );
    assert!(change_report.candidates.is_empty());

    // Confounding limitations are correctly identified from geometry
    assert!(
        change_report
            .limitations
            .contains(&GeometricLimitation::SignificantSunElevationDifference)
    );
    assert!(
        change_report
            .limitations
            .contains(&GeometricLimitation::SeasonalFoliageOrSnowDiscrepancy)
    );

    // Probe B1 self-comparison check: identical geometry yields 0 candidates and no limitations
    let self_report = catalog.compare_baselines(
        "obs:landsat:summer",
        "obs:landsat:summer",
        &geom_summer,
        &geom_summer,
        &limits,
    )?;
    assert_eq!(
        self_report.analysis_status,
        ChangeAnalysisStatus::RasterDetectorNotPerformed
    );
    assert!(self_report.candidates.is_empty());
    assert!(self_report.limitations.is_empty());
    assert_eq!(self_report.sun_elevation_diff_deg, 0.0);
    assert_eq!(self_report.off_nadir_diff_deg, 0.0);
    assert_eq!(self_report.seasonal_day_gap, 0);

    Ok(())
}
