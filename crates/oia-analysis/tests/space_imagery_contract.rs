//! Contract tests for space imagery catalog, scientific provenance, and mission stream kernel (`SPACE-01`–`SPACE-03`, `GEO-04`, `INTEL-03`, `A05`).

use oia_analysis::space_imagery::*;
use oia_time_geo::{GeoBoundingBoxWgs84, TimeWindow};

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
            uncovered_angular_area_fraction,
            uncovered_bands_count,
            ..
        } => {
            assert!(uncovered_angular_area_fraction > 0.0);
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
            uncovered_angular_area_fraction,
            uncovered_bands_count,
            ..
        } => {
            // Envelope area is 100, four 1x1 corners cover 4 units; uncovered fraction is 96%
            assert!((uncovered_angular_area_fraction - 0.96).abs() < 1e-4);
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

#[test]
fn dateline_crossing_mosaic_union_and_wrapped_footprint() -> Result<(), Box<dyn std::error::Error>>
{
    let limits = SpaceCatalogLimits::default();
    let mut catalog = SpaceImageryCatalog::new();
    let provider = SpaceDataProvider::new("DatelineTest", "Archive", true)?;

    // 1. Dateline query (0, 10, 170, -170) with one tile (0, 10, 170, 180)
    let query_dateline = GeoBoundingBoxWgs84::new(0.0, 10.0, 170.0, -170.0)?;
    let tile_east = GeoBoundingBoxWgs84::new(0.0, 10.0, 170.0, 180.0)?;

    let entry_east = SpaceObservationEntry {
        observation_id: "obs:dateline:east".into(),
        mission: "DatelineMission".into(),
        instrument: "DL-1".into(),
        target: SpaceObservationTarget::EarthObservation {
            footprint: tile_east,
            region_label: None,
        },
        spectral_regime: SpectralRegime::OpticalVisible,
        wavelength_range_nm: None,
        time_window: TimeWindow::new(1_700_000_000_000, 1_700_000_010_000)?,
        spatial_resolution: Some(SpatialResolution::GroundSampleDistanceMeters(20.0)),
        provider: provider.clone(),
        scientific_product: ScientificProductRecord::new(
            "p:dl:east",
            ScientificProductFormat::FitsPrimaryImage,
            CalibrationLevel::Level1BGeoreferenced,
            1.0,
            "v1",
        )?,
        previews: Vec::new(),
    };
    catalog.add_observation(entry_east, &limits)?;

    let report_dateline = catalog.evaluate_mosaic_coverage(&query_dateline, None, &limits)?;
    match report_dateline.assessment {
        MosaicCoverageAssessment::PartialCoverageWithGaps {
            uncovered_angular_area_fraction,
            uncovered_bands_count,
            ..
        } => {
            // Half the longitudinal span [170, -170] is covered ([170, 180]), half is uncovered ([-180, -170])
            assert!((uncovered_angular_area_fraction - 0.5).abs() < 1e-4);
            assert!(uncovered_bands_count > 0);
        }
        _ => return Err("expected PartialCoverageWithGaps for half-covered dateline query".into()),
    }

    // 2. Wrapped footprint (0, 10, 170, -170) fully containing ordinary query (0, 10, 175, 179)
    let mut catalog_wrapped = SpaceImageryCatalog::new();
    let query_ordinary = GeoBoundingBoxWgs84::new(0.0, 10.0, 175.0, 179.0)?;
    let footprint_wrapped = GeoBoundingBoxWgs84::new(0.0, 10.0, 170.0, -170.0)?;

    let entry_wrapped = SpaceObservationEntry {
        observation_id: "obs:dateline:wrapped".into(),
        mission: "DatelineMission".into(),
        instrument: "DL-1".into(),
        target: SpaceObservationTarget::EarthObservation {
            footprint: footprint_wrapped,
            region_label: None,
        },
        spectral_regime: SpectralRegime::OpticalVisible,
        wavelength_range_nm: None,
        time_window: TimeWindow::new(1_700_000_000_000, 1_700_000_010_000)?,
        spatial_resolution: Some(SpatialResolution::GroundSampleDistanceMeters(20.0)),
        provider,
        scientific_product: ScientificProductRecord::new(
            "p:dl:wrapped",
            ScientificProductFormat::FitsPrimaryImage,
            CalibrationLevel::Level1BGeoreferenced,
            1.0,
            "v1",
        )?,
        previews: Vec::new(),
    };
    catalog_wrapped.add_observation(entry_wrapped, &limits)?;

    let report_ordinary =
        catalog_wrapped.evaluate_mosaic_coverage(&query_ordinary, None, &limits)?;
    match report_ordinary.assessment {
        MosaicCoverageAssessment::CompleteVerifiedCoverage {
            observation_count,
            min_resolution_m,
            max_resolution_m,
        } => {
            assert_eq!(observation_count, 1);
            assert_eq!(min_resolution_m, Some(20.0));
            assert_eq!(max_resolution_m, Some(20.0));
        }
        _ => {
            return Err(
                "expected CompleteVerifiedCoverage for wrapped footprint covering query".into(),
            );
        }
    }

    Ok(())
}

#[test]
fn bookmark_timestamp_checked_arithmetic_overflow() -> Result<(), Box<dyn std::error::Error>> {
    let feed_live = SpaceMissionFeed::new(
        "feed:iss:live",
        "ISS",
        "ISS Live",
        StreamClassification::LiveCamera,
        MissionFeedStatus::NominalActive,
        1, // +1 ms clock offset
    )?;

    // display_utc_ms = i64::MAX with clock offset +1 must overflow checked addition
    let err_live = feed_live.create_bookmark(
        "bm:overflow:live",
        i64::MAX,
        "Overflowing timestamp",
        10,
        "None",
    );
    assert!(matches!(
        err_live,
        Err(SpaceImageryError::TimestampOverflow { .. })
    ));

    // DelayedTransmission with huge delay seconds overflowing integer conversion
    let feed_delayed = SpaceMissionFeed::new(
        "feed:probe:deep",
        "DeepSpace",
        "Deep Space Video",
        StreamClassification::DelayedTransmission {
            delay_seconds: 1e20,
            delay_reason: "Extreme interstellar transit".into(),
        },
        MissionFeedStatus::NominalActive,
        0,
    )?;

    let err_delayed = feed_delayed.create_bookmark(
        "bm:overflow:delayed",
        1_000_000_000,
        "Overflowing delay",
        10,
        "None",
    );
    assert!(matches!(
        err_delayed,
        Err(SpaceImageryError::TimestampOverflow { .. })
    ));

    Ok(())
}

#[test]
fn baseline_comparison_revalidates_geometry_inputs() -> Result<(), Box<dyn std::error::Error>> {
    let limits = SpaceCatalogLimits::default();
    let mut catalog = SpaceImageryCatalog::new();
    let env = GeoBoundingBoxWgs84::new(0.0, 1.0, 0.0, 1.0)?;
    let provider = SpaceDataProvider::new("Prov", "Archive", true)?;

    let entry = SpaceObservationEntry {
        observation_id: "obs:geom:test".into(),
        mission: "TestMission".into(),
        instrument: "TestInst".into(),
        target: SpaceObservationTarget::EarthObservation {
            footprint: env,
            region_label: None,
        },
        spectral_regime: SpectralRegime::OpticalVisible,
        wavelength_range_nm: None,
        time_window: TimeWindow::new(1_700_000_000_000, 1_700_000_010_000)?,
        spatial_resolution: Some(SpatialResolution::GroundSampleDistanceMeters(10.0)),
        provider,
        scientific_product: ScientificProductRecord::new(
            "p:geom:test",
            ScientificProductFormat::FitsPrimaryImage,
            CalibrationLevel::Level1BGeoreferenced,
            1.0,
            "v1",
        )?,
        previews: Vec::new(),
    };
    catalog.add_observation(entry, &limits)?;

    // Directly constructed invalid geometry (bypass constructor)
    let invalid_geom = ImageryCaptureGeometry {
        sun_elevation_deg: f64::NAN,
        sun_azimuth_deg: 0.0,
        off_nadir_angle_deg: 0.0,
        day_of_year: 0,
    };
    let valid_geom = ImageryCaptureGeometry::new(45.0, 180.0, 5.0, 100)?;

    let err_comp = catalog.compare_baselines(
        "obs:geom:test",
        "obs:geom:test",
        &invalid_geom,
        &valid_geom,
        &limits,
    );
    assert!(matches!(
        err_comp,
        Err(SpaceImageryError::InvalidCoordinates { .. })
    ));

    Ok(())
}

#[test]
fn disconnected_intersection_components_preserved() -> Result<(), Box<dyn std::error::Error>> {
    let limits = SpaceCatalogLimits::default();
    let mut catalog = SpaceImageryCatalog::new();
    let provider = SpaceDataProvider::new("Prov", "Archive", true)?;

    // b1 spans [-170, 170], b2 spans [160, -160] (antimeridian crossing)
    // Their intersection has two disconnected components: [160, 170] and [-170, -160]
    let b1 = GeoBoundingBoxWgs84::new(0.0, 10.0, -170.0, 170.0)?;
    let b2 = GeoBoundingBoxWgs84::new(0.0, 10.0, 160.0, -160.0)?;

    let obs1 = SpaceObservationEntry {
        observation_id: "obs:multi:1".into(),
        mission: "M1".into(),
        instrument: "I1".into(),
        target: SpaceObservationTarget::EarthObservation {
            footprint: b1,
            region_label: None,
        },
        spectral_regime: SpectralRegime::OpticalVisible,
        wavelength_range_nm: None,
        time_window: TimeWindow::new(1_700_000_000_000, 1_700_000_010_000)?,
        spatial_resolution: Some(SpatialResolution::GroundSampleDistanceMeters(10.0)),
        provider: provider.clone(),
        scientific_product: ScientificProductRecord::new(
            "p:multi:1",
            ScientificProductFormat::FitsPrimaryImage,
            CalibrationLevel::Level1BGeoreferenced,
            1.0,
            "v1",
        )?,
        previews: Vec::new(),
    };

    let obs2 = SpaceObservationEntry {
        observation_id: "obs:multi:2".into(),
        mission: "M2".into(),
        instrument: "I2".into(),
        target: SpaceObservationTarget::EarthObservation {
            footprint: b2,
            region_label: None,
        },
        spectral_regime: SpectralRegime::OpticalVisible,
        wavelength_range_nm: None,
        time_window: TimeWindow::new(1_700_000_000_000, 1_700_000_010_000)?,
        spatial_resolution: Some(SpatialResolution::GroundSampleDistanceMeters(10.0)),
        provider,
        scientific_product: ScientificProductRecord::new(
            "p:multi:2",
            ScientificProductFormat::FitsPrimaryImage,
            CalibrationLevel::Level1BGeoreferenced,
            1.0,
            "v1",
        )?,
        previews: Vec::new(),
    };

    catalog.add_observation(obs1, &limits)?;
    catalog.add_observation(obs2, &limits)?;

    let geom = ImageryCaptureGeometry::new(45.0, 180.0, 5.0, 100)?;
    let report = catalog.compare_baselines("obs:multi:1", "obs:multi:2", &geom, &geom, &limits)?;

    // Both disjoint components must be preserved in footprint_intersections
    assert_eq!(report.footprint_intersections.len(), 2);
    let has_pos = report.footprint_intersections.iter().any(|b| {
        (b.west_lon_deg() - 160.0).abs() < 1e-4 && (b.east_lon_deg() - 170.0).abs() < 1e-4
    });
    let has_neg = report.footprint_intersections.iter().any(|b| {
        (b.west_lon_deg() - (-170.0)).abs() < 1e-4 && (b.east_lon_deg() - (-160.0)).abs() < 1e-4
    });
    assert!(has_pos);
    assert!(has_neg);

    Ok(())
}

#[test]
fn delay_float_to_i64_exclusive_bound_overflow() -> Result<(), Box<dyn std::error::Error>> {
    // delay_seconds = (i64::MAX as f64) / 1000.0 yields 2^63 ms, which must be rejected
    let feed_delayed = SpaceMissionFeed::new(
        "feed:probe:max_bound",
        "MaxBoundProbe",
        "Max Bound Probe Video",
        StreamClassification::DelayedTransmission {
            delay_seconds: (i64::MAX as f64) / 1000.0,
            delay_reason: "Boundary delay".into(),
        },
        MissionFeedStatus::NominalActive,
        0,
    )?;

    let err =
        feed_delayed.create_bookmark("bm:bound:overflow", 0, "Boundary overflow test", 10, "None");
    assert!(matches!(
        err,
        Err(SpaceImageryError::TimestampOverflow { .. })
    ));

    Ok(())
}

#[test]
fn multi_source_composite_stitching_deblurs_repairs_clouds_and_defends_against_upsampling()
-> Result<(), Box<dyn std::error::Error>> {
    let limits = SpaceCatalogLimits::default();
    let envelope = GeoBoundingBoxWgs84::new(43.0, 43.2, 5.0, 5.2)?;

    // 1. National aerial orthophoto (0.15 m native) covering full 2x2 grid,
    //    with a statutory blur polygon in SW (row 0, col 0) and cumulus cloud in NE (row 1, col 1).
    let mut aerial = CompositeCandidatePatch {
        patch_id: "patch:ign:ortho_15cm".into(),
        source_id: "src:ign:2025".into(),
        platform: CompositeSourcePlatform::AerialOrthophoto,
        provider: SpaceDataProvider::new("IGN France", "Geoservices", true)?,
        mission_or_collection: "BD ORTHO".into(),
        instrument_or_sensor: "DMC-III".into(),
        footprint: envelope,
        time_window: TimeWindow::new(1_718_000_000_000, 1_718_003_600_000)?,
        native_gsd_m: Some(0.15),
        served_grid_spacing_m: 0.15,
        radiometric_class: SurfaceRadiometricClass::OpticalTrueColor,
        calibration_level: CalibrationLevel::Level3GriddedComposite,
        stacking_history: None,
        color_mapping: Some(ColorMapping::TrueColor),
        base_integrity: PatchIntegrityState::UneditedClearNative {
            cloud_or_haze_fraction: 0.01,
        },
        localized_defects: Vec::new(),
        registration: CrossViewRegistration::new(
            VerticalReference::OrthometricDem {
                dem_source: "RGE ALTI 1m".into(),
                vertical_rmse_m: 0.25,
            },
            CorrespondenceMethod::GroundControlPointsBundleAdjustment { gcp_count: 16 },
            0.30,
            4.0,
        )?,
    };
    aerial.localized_defects.push(LocalizedPatchDefect::new(
        "def:blur_sw",
        GeoBoundingBoxWgs84::new(43.0, 43.1, 5.0, 5.1)?,
        PatchIntegrityState::DeliberatelyBlurredOrRedacted {
            reason: "Statutory security redaction mask".into(),
            coarse_effective_gsd_m: Some(25.0),
        },
    )?);
    aerial.localized_defects.push(LocalizedPatchDefect::new(
        "def:cloud_ne",
        GeoBoundingBoxWgs84::new(43.1, 43.2, 5.1, 5.2)?,
        PatchIntegrityState::ObscuredByCloudShadowOrSmoke {
            obscuration_fraction: 0.90,
        },
    )?);

    // 2. Sentinel-2 (10.0 m native GSD) bicubic-upsampled to a 0.05 m grid spacing.
    //    Must be clamped to 10.0 m effective verifiable GSD and never outrank 0.50 m or 0.15 m native!
    let upsampled_s2 = CompositeCandidatePatch {
        patch_id: "patch:sentinel2:upsampled_5cm_grid".into(),
        source_id: "src:copernicus:s2".into(),
        platform: CompositeSourcePlatform::SatelliteEarthObservation,
        provider: SpaceDataProvider::new("ESA Copernicus", "CDSE", true)?,
        mission_or_collection: "Sentinel-2B".into(),
        instrument_or_sensor: "MSI".into(),
        footprint: envelope,
        time_window: TimeWindow::new(1_718_500_000_000, 1_718_500_030_000)?,
        native_gsd_m: Some(10.0),
        served_grid_spacing_m: 0.05,
        radiometric_class: SurfaceRadiometricClass::OpticalTrueColor,
        calibration_level: CalibrationLevel::Level2Geophysical,
        stacking_history: None,
        color_mapping: Some(ColorMapping::TrueColor),
        base_integrity: PatchIntegrityState::UneditedClearNative {
            cloud_or_haze_fraction: 0.0,
        },
        localized_defects: Vec::new(),
        registration: CrossViewRegistration::new(
            VerticalReference::OrthometricDem {
                dem_source: "GLO-30".into(),
                vertical_rmse_m: 3.0,
            },
            CorrespondenceMethod::OrbitalWcsAndDemOrthorectification,
            3.5,
            5.0,
        )?,
    };
    assert!(upsampled_s2.was_upsampled_beyond_native());

    // 3. Unredacted commercial open-data satellite scene (0.50 m native GSD) covering full envelope.
    let clean_sat = CompositeCandidatePatch {
        patch_id: "patch:wv3:open_50cm".into(),
        source_id: "src:maxar:wv3".into(),
        platform: CompositeSourcePlatform::SatelliteEarthObservation,
        provider: SpaceDataProvider::new("Maxar Open Data", "AWS STAC", true)?,
        mission_or_collection: "WorldView-3".into(),
        instrument_or_sensor: "WV110".into(),
        footprint: envelope,
        time_window: TimeWindow::new(1_719_000_000_000, 1_719_000_060_000)?,
        native_gsd_m: Some(0.50),
        served_grid_spacing_m: 0.50,
        radiometric_class: SurfaceRadiometricClass::OpticalPansharpenedTrueColor,
        calibration_level: CalibrationLevel::Level2Geophysical,
        stacking_history: None,
        color_mapping: Some(ColorMapping::TrueColor),
        base_integrity: PatchIntegrityState::UneditedClearNative {
            cloud_or_haze_fraction: 0.02,
        },
        localized_defects: Vec::new(),
        registration: CrossViewRegistration::new(
            VerticalReference::OrthometricDem {
                dem_source: "GLO-30".into(),
                vertical_rmse_m: 2.0,
            },
            CorrespondenceMethod::OrbitalWcsAndDemOrthorectification,
            1.5,
            10.0,
        )?,
    };

    // 4. Calibrated UAV orthomosaic (0.04 m native GSD) covering SE cell (row 0, col 1).
    let uav_se = CompositeCandidatePatch {
        patch_id: "patch:oam:uav_4cm_se".into(),
        source_id: "src:oam:uav".into(),
        platform: CompositeSourcePlatform::CalibratedUavOrDroneOrtho,
        provider: SpaceDataProvider::new("OpenAerialMap", "OAM", true)?,
        mission_or_collection: "OAM Survey".into(),
        instrument_or_sensor: "Zenmuse P1".into(),
        footprint: GeoBoundingBoxWgs84::new(43.0, 43.1, 5.1, 5.2)?,
        time_window: TimeWindow::new(1_719_500_000_000, 1_719_501_800_000)?,
        native_gsd_m: Some(0.04),
        served_grid_spacing_m: 0.04,
        radiometric_class: SurfaceRadiometricClass::OpticalTrueColor,
        calibration_level: CalibrationLevel::Level3GriddedComposite,
        stacking_history: None,
        color_mapping: Some(ColorMapping::TrueColor),
        base_integrity: PatchIntegrityState::UneditedClearNative {
            cloud_or_haze_fraction: 0.0,
        },
        localized_defects: Vec::new(),
        registration: CrossViewRegistration::new(
            VerticalReference::OrthometricDem {
                dem_source: "RTK DSM".into(),
                vertical_rmse_m: 0.05,
            },
            CorrespondenceMethod::GroundControlPointsBundleAdjustment { gcp_count: 10 },
            0.05,
            1.5,
        )?,
    };

    let req = CompositeStitchRequest {
        target_envelope: envelope,
        grid_rows: 2,
        grid_cols: 2,
        policy: CompositeStitchPolicy::default(),
        candidates: vec![aerial, upsampled_s2, clean_sat, uav_se],
    };

    let plan = plan_globe_composite(&req, &limits, None)?;
    assert_eq!(plan.method_id, COMPOSITE_PLAN_METHOD_ID);
    assert_eq!(plan.policy, CompositeStitchPolicy::default());
    assert_eq!(plan.clean_optical_cells_count, 4);
    assert_eq!(plan.radar_fallback_cells_count, 0);
    assert_eq!(plan.retained_degraded_cells_count, 0);
    assert_eq!(plan.uncovered_cells_count, 0);
    assert_eq!(plan.cells_filled_from_alternate_optical_source, 2);
    assert_eq!(plan.cells_filled_from_radar_over_degraded_source, 0);
    assert!(!plan.is_single_instant_observation);
    assert_eq!(plan.finest_verifiable_gsd_m, Some(0.04));
    assert_eq!(plan.coarsest_verifiable_gsd_m, Some(0.50));
    assert_eq!(plan.seams.len(), 4);

    // SW cell (0, 0): aerial was blurred (25.0 m coarse), so 0.50 m satellite wins and bypasses aerial
    let sw = plan.cells[0]
        .outcome
        .assignment()
        .ok_or("missing SW assignment")?;
    assert_eq!(sw.patch_id, "patch:wv3:open_50cm");
    assert_eq!(sw.effective_verifiable_gsd_m, Some(0.50));
    assert_eq!(sw.bypassed_degraded_patch_ids, vec!["patch:ign:ortho_15cm"]);
    assert!(plan.cells[0].rejected_or_bypassed.iter().any(|r| {
        r.patch_id == "patch:ign:ortho_15cm"
            && r.source_id == "src:ign:2025"
            && r.mission_or_collection == "BD ORTHO"
            && r.reasons.contains(
                &CandidateRejectionOrBypassReason::DeliberateBlurOrRedactionBypassed {
                    reason: "Statutory security redaction mask".into(),
                    coarse_effective_gsd_m: Some(25.0),
                },
            )
    }));

    // SE cell (0, 1): UAV (0.04 m) outranks unedited aerial (0.15 m) and satellite (0.50 m)
    let se = plan.cells[1]
        .outcome
        .assignment()
        .ok_or("missing SE assignment")?;
    assert_eq!(se.patch_id, "patch:oam:uav_4cm_se");
    assert_eq!(se.effective_verifiable_gsd_m, Some(0.04));
    assert!(se.bypassed_degraded_patch_ids.is_empty());

    // NW cell (1, 0): unedited aerial (0.15 m) wins over satellite (0.50 m) and upsampled S2 (10.0 m)
    let nw = plan.cells[2]
        .outcome
        .assignment()
        .ok_or("missing NW assignment")?;
    assert_eq!(nw.patch_id, "patch:ign:ortho_15cm");
    assert_eq!(nw.effective_verifiable_gsd_m, Some(0.15));

    // NE cell (1, 1): aerial was cloudy (0.90), so 0.50 m satellite wins and bypasses aerial
    let ne = plan.cells[3]
        .outcome
        .assignment()
        .ok_or("missing NE assignment")?;
    assert_eq!(ne.patch_id, "patch:wv3:open_50cm");
    assert_eq!(ne.bypassed_degraded_patch_ids, vec!["patch:ign:ortho_15cm"]);

    Ok(())
}

#[test]
fn composite_rejects_outreach_predictive_uncalibrated_and_isolates_sar_fallback()
-> Result<(), Box<dyn std::error::Error>> {
    let limits = SpaceCatalogLimits::default();
    let envelope = GeoBoundingBoxWgs84::new(10.0, 11.0, 20.0, 21.0)?;
    let provider = SpaceDataProvider::new("PublicArchive", "STAC", true)?;
    let valid_reg = CrossViewRegistration::new(
        VerticalReference::OrthometricDem {
            dem_source: "Copernicus DEM".into(),
            vertical_rmse_m: 1.5,
        },
        CorrespondenceMethod::OrbitalWcsAndDemOrthorectification,
        2.0,
        8.0,
    )?;

    // 1. Outreach aesthetic rendering -> rejected
    let outreach_patch = CompositeCandidatePatch {
        patch_id: "patch:outreach".into(),
        source_id: "src:outreach".into(),
        platform: CompositeSourcePlatform::SatelliteEarthObservation,
        provider: provider.clone(),
        mission_or_collection: "PressRelease".into(),
        instrument_or_sensor: "Cam".into(),
        footprint: envelope,
        time_window: TimeWindow::new(1_700_000_000_000, 1_700_000_010_000)?,
        native_gsd_m: Some(0.30),
        served_grid_spacing_m: 0.30,
        radiometric_class: SurfaceRadiometricClass::OpticalTrueColor,
        calibration_level: CalibrationLevel::Level3GriddedComposite,
        stacking_history: None,
        color_mapping: Some(ColorMapping::OutreachRendering {
            renderer_attribution: "Media Team".into(),
            enhancement_notes: "Tone curve".into(),
        }),
        base_integrity: PatchIntegrityState::UneditedClearNative {
            cloud_or_haze_fraction: 0.0,
        },
        localized_defects: Vec::new(),
        registration: valid_reg.clone(),
    };

    // 2. Predictive AI super-resolution / inpainted -> rejected
    let ai_inpainted_patch = CompositeCandidatePatch {
        patch_id: "patch:ai_inpaint".into(),
        source_id: "src:ai".into(),
        platform: CompositeSourcePlatform::SatelliteEarthObservation,
        provider: provider.clone(),
        mission_or_collection: "SynthSR".into(),
        instrument_or_sensor: "GAN".into(),
        footprint: envelope,
        time_window: TimeWindow::new(1_700_000_000_000, 1_700_000_010_000)?,
        native_gsd_m: Some(0.25),
        served_grid_spacing_m: 0.25,
        radiometric_class: SurfaceRadiometricClass::OpticalTrueColor,
        calibration_level: CalibrationLevel::Level4ModelAssimilated,
        stacking_history: None,
        color_mapping: Some(ColorMapping::TrueColor),
        base_integrity: PatchIntegrityState::PredictiveSuperResolutionOrInpainted {
            model_id: "diffusion-sr-v2".into(),
            base_native_gsd_m: 10.0,
        },
        localized_defects: Vec::new(),
        registration: valid_reg.clone(),
    };

    // 3. Uncalibrated street-level photo -> rejected
    let uncalibrated_ground = CompositeCandidatePatch {
        patch_id: "patch:ground_uncalibrated".into(),
        source_id: "src:street".into(),
        platform: CompositeSourcePlatform::CalibratedGroundOrStreetFrame,
        provider: provider.clone(),
        mission_or_collection: "CrowdsourcedStreet".into(),
        instrument_or_sensor: "ActionCam".into(),
        footprint: envelope,
        time_window: TimeWindow::new(1_700_000_000_000, 1_700_000_010_000)?,
        native_gsd_m: Some(0.02),
        served_grid_spacing_m: 0.02,
        radiometric_class: SurfaceRadiometricClass::OpticalTrueColor,
        calibration_level: CalibrationLevel::Level1BGeoreferenced,
        stacking_history: None,
        color_mapping: Some(ColorMapping::TrueColor),
        base_integrity: PatchIntegrityState::UneditedClearNative {
            cloud_or_haze_fraction: 0.0,
        },
        localized_defects: Vec::new(),
        registration: CrossViewRegistration::new(
            VerticalReference::UnrectifiedSensorPlane,
            CorrespondenceMethod::UncalibratedApproximatePlacement,
            50.0,
            75.0,
        )?,
    };

    // 4. Oblique ISS crew photo without DEM terrain rectification -> rejected
    let unrectified_iss = CompositeCandidatePatch {
        patch_id: "patch:iss_unrectified".into(),
        source_id: "src:iss:eol".into(),
        platform: CompositeSourcePlatform::SpacecraftHandheldOrCrewFrame,
        provider: provider.clone(),
        mission_or_collection: "ISS Crew Earth Observations".into(),
        instrument_or_sensor: "Nikon Z9".into(),
        footprint: envelope,
        time_window: TimeWindow::new(1_700_000_000_000, 1_700_000_010_000)?,
        native_gsd_m: Some(2.0),
        served_grid_spacing_m: 2.0,
        radiometric_class: SurfaceRadiometricClass::OpticalTrueColor,
        calibration_level: CalibrationLevel::Level1BGeoreferenced,
        stacking_history: None,
        color_mapping: Some(ColorMapping::TrueColor),
        base_integrity: PatchIntegrityState::UneditedClearNative {
            cloud_or_haze_fraction: 0.0,
        },
        localized_defects: Vec::new(),
        registration: CrossViewRegistration::new(
            VerticalReference::UnrectifiedSensorPlane,
            CorrespondenceMethod::CalibratedCameraPoseAndTerrainIntersection,
            5.0,
            25.0,
        )?,
    };

    // 5. Partial footprint covering only half the cell -> rejected
    let partial_patch = CompositeCandidatePatch {
        patch_id: "patch:partial_half".into(),
        source_id: "src:partial".into(),
        platform: CompositeSourcePlatform::SatelliteEarthObservation,
        provider: provider.clone(),
        mission_or_collection: "Landsat-9".into(),
        instrument_or_sensor: "OLI-2".into(),
        footprint: GeoBoundingBoxWgs84::new(10.0, 10.5, 20.0, 21.0)?,
        time_window: TimeWindow::new(1_700_000_000_000, 1_700_000_010_000)?,
        native_gsd_m: Some(15.0),
        served_grid_spacing_m: 15.0,
        radiometric_class: SurfaceRadiometricClass::OpticalTrueColor,
        calibration_level: CalibrationLevel::Level2Geophysical,
        stacking_history: None,
        color_mapping: Some(ColorMapping::TrueColor),
        base_integrity: PatchIntegrityState::UneditedClearNative {
            cloud_or_haze_fraction: 0.0,
        },
        localized_defects: Vec::new(),
        registration: valid_reg.clone(),
    };

    // 6. Sentinel-1 SAR patch covering full envelope
    let sar_patch = CompositeCandidatePatch {
        patch_id: "patch:sentinel1:sar_5m".into(),
        source_id: "src:copernicus:s1".into(),
        platform: CompositeSourcePlatform::SatelliteEarthObservation,
        provider,
        mission_or_collection: "Sentinel-1A".into(),
        instrument_or_sensor: "C-SAR".into(),
        footprint: envelope,
        time_window: TimeWindow::new(1_700_000_100_000, 1_700_000_120_000)?,
        native_gsd_m: Some(5.0),
        served_grid_spacing_m: 5.0,
        radiometric_class: SurfaceRadiometricClass::SyntheticApertureRadar,
        calibration_level: CalibrationLevel::Level1BGeoreferenced,
        stacking_history: None,
        color_mapping: Some(ColorMapping::SingleBandGreyscale {
            band_name: "C-Band VV Backscatter".into(),
        }),
        base_integrity: PatchIntegrityState::UneditedClearNative {
            cloud_or_haze_fraction: 0.0,
        },
        localized_defects: Vec::new(),
        registration: valid_reg,
    };

    // Policy A: default policy (SAR fallback disabled) -> cell must be UncoveredGap
    let req_no_sar = CompositeStitchRequest {
        target_envelope: envelope,
        grid_rows: 1,
        grid_cols: 1,
        policy: CompositeStitchPolicy::default(),
        candidates: vec![
            outreach_patch.clone(),
            ai_inpainted_patch.clone(),
            uncalibrated_ground.clone(),
            unrectified_iss.clone(),
            partial_patch.clone(),
            sar_patch.clone(),
        ],
    };
    let plan_no_sar = plan_globe_composite(&req_no_sar, &limits, None)?;
    assert_eq!(plan_no_sar.uncovered_cells_count, 1);
    assert!(!plan_no_sar.is_single_instant_observation);
    let has_receipt = |id: &str, expected: &CandidateRejectionOrBypassReason| {
        plan_no_sar.cells[0]
            .rejected_or_bypassed
            .iter()
            .any(|r| r.patch_id == id && r.reasons.contains(expected))
    };
    assert!(has_receipt(
        "patch:outreach",
        &CandidateRejectionOrBypassReason::OutreachOrSimulatedPresentation
    ));
    assert!(has_receipt(
        "patch:ai_inpaint",
        &CandidateRejectionOrBypassReason::PredictiveOrInpaintedContent {
            model_id: "diffusion-sr-v2".into(),
        }
    ));
    assert!(has_receipt(
        "patch:ground_uncalibrated",
        &CandidateRejectionOrBypassReason::UncalibratedSpatialCorrespondence
    ));
    assert!(has_receipt(
        "patch:iss_unrectified",
        &CandidateRejectionOrBypassReason::MissingTerrainOrthorectificationForObliqueView
    ));
    assert!(has_receipt(
        "patch:partial_half",
        &CandidateRejectionOrBypassReason::InsufficientCellFootprintCoverage
    ));
    assert!(has_receipt(
        "patch:sentinel1:sar_5m",
        &CandidateRejectionOrBypassReason::RadarModalityDisabledByPolicy
    ));

    // Policy B: enable SAR cloud-penetration fallback -> cell gets AssignedRadarFallback (never relabeled as optical)
    let policy_sar = CompositeStitchPolicy {
        allow_sar_cloud_penetration_fallback: true,
        ..Default::default()
    };
    let req_sar = CompositeStitchRequest {
        target_envelope: envelope,
        grid_rows: 1,
        grid_cols: 1,
        policy: policy_sar,
        candidates: req_no_sar.candidates,
    };
    let plan_sar = plan_globe_composite(&req_sar, &limits, None)?;
    assert_eq!(plan_sar.clean_optical_cells_count, 0);
    assert_eq!(plan_sar.radar_fallback_cells_count, 1);
    assert_eq!(plan_sar.cells_filled_from_alternate_optical_source, 0);
    match &plan_sar.cells[0].outcome {
        CompositeCellOutcome::AssignedRadarFallback(a) => {
            assert_eq!(a.patch_id, "patch:sentinel1:sar_5m");
            assert_eq!(
                a.assigned_modality,
                AssignedModality::RadarBackscatterFallback
            );
        }
        other => return Err(format!("expected AssignedRadarFallback, got {other:?}").into()),
    }

    Ok(())
}

#[test]
fn composite_antimeridian_grid_and_boundary_revalidation() -> Result<(), Box<dyn std::error::Error>>
{
    let limits = SpaceCatalogLimits::default();
    let dateline_env = GeoBoundingBoxWgs84::new(0.0, 6.0, 174.0, -174.0)?;
    let wrapped_footprint = GeoBoundingBoxWgs84::new(-1.0, 7.0, 170.0, -170.0)?;
    let provider = SpaceDataProvider::new("USGS/NASA", "Landsat", true)?;

    let obs = SpaceObservationEntry {
        observation_id: "obs:landsat9:dateline".into(),
        mission: "Landsat-9".into(),
        instrument: "OLI-2".into(),
        target: SpaceObservationTarget::EarthObservation {
            footprint: wrapped_footprint,
            region_label: Some("Pacific Dateline".into()),
        },
        spectral_regime: SpectralRegime::OpticalVisible,
        wavelength_range_nm: Some(WavelengthRangeNm::new(430.0, 670.0)?),
        time_window: TimeWindow::new(1_710_000_000_000, 1_710_000_030_000)?,
        spatial_resolution: Some(SpatialResolution::GroundSampleDistanceMeters(15.0)),
        provider,
        scientific_product: ScientificProductRecord::new(
            "l9:dl:l2",
            ScientificProductFormat::GeoTiffCalibratedRadiance,
            CalibrationLevel::Level2Geophysical,
            0.025,
            "C2_L2",
        )?,
        previews: Vec::new(),
    };

    let reg = CrossViewRegistration::new(
        VerticalReference::OrthometricDem {
            dem_source: "Copernicus GLO-30".into(),
            vertical_rmse_m: 2.0,
        },
        CorrespondenceMethod::OrbitalWcsAndDemOrthorectification,
        3.0,
        4.0,
    )?;

    let wrapped_patch = obs.to_composite_candidate_patch(
        "patch:l9:dateline_wrapped",
        CompositeSourcePlatform::SatelliteEarthObservation,
        15.0,
        SurfaceRadiometricClass::OpticalPansharpenedTrueColor,
        PatchIntegrityState::UneditedClearNative {
            cloud_or_haze_fraction: 0.02,
        },
        reg.clone(),
        &limits,
    )?;
    assert_eq!(wrapped_patch.color_mapping, None);

    // 1x3 grid across [174, -174]: cell (0, 1) straddles the antimeridian [178, -178]
    let req_dl = CompositeStitchRequest {
        target_envelope: dateline_env,
        grid_rows: 1,
        grid_cols: 3,
        policy: CompositeStitchPolicy::default(),
        candidates: vec![wrapped_patch.clone()],
    };
    let plan_dl = plan_globe_composite(&req_dl, &limits, None)?;
    assert_eq!(plan_dl.clean_optical_cells_count, 3);
    assert_eq!(plan_dl.uncovered_cells_count, 0);
    assert!(plan_dl.is_single_instant_observation);
    assert!(plan_dl.seams.is_empty());
    assert!(plan_dl.cells[1].bounds.west_lon_deg() > plan_dl.cells[1].bounds.east_lon_deg());

    // Boundary revalidation: localized defect with UneditedClearNative must be rejected even if constructed directly
    let mut invalid_defect_patch = wrapped_patch.clone();
    invalid_defect_patch
        .localized_defects
        .push(LocalizedPatchDefect {
            defect_id: "def:invalid_clean".into(),
            bounds: GeoBoundingBoxWgs84::new(0.0, 2.0, 175.0, 177.0)?,
            defect_state: PatchIntegrityState::UneditedClearNative {
                cloud_or_haze_fraction: 0.0,
            },
        });
    let mut bad_req = req_dl.clone();
    bad_req.candidates = vec![invalid_defect_patch];
    assert!(matches!(
        plan_globe_composite(&bad_req, &limits, None),
        Err(SpaceImageryError::InvalidPatchIntegrity { .. })
    ));

    // Boundary revalidation: seam timestamp delta overflow across extreme i64 timestamps
    let mut west_patch = wrapped_patch.clone();
    west_patch.patch_id = "patch:extreme_early".into();
    west_patch.footprint = GeoBoundingBoxWgs84::new(0.0, 6.0, 174.0, 180.0)?;
    west_patch.time_window = TimeWindow::new(i64::MIN, i64::MIN + 10)?;

    let mut east_patch = wrapped_patch;
    east_patch.patch_id = "patch:extreme_late".into();
    east_patch.footprint = GeoBoundingBoxWgs84::new(0.0, 6.0, -180.0, -174.0)?;
    east_patch.time_window = TimeWindow::new(i64::MAX - 10, i64::MAX)?;

    let overflow_req = CompositeStitchRequest {
        target_envelope: dateline_env,
        grid_rows: 1,
        grid_cols: 2,
        policy: CompositeStitchPolicy::default(),
        candidates: vec![west_patch, east_patch],
    };
    assert!(matches!(
        plan_globe_composite(&overflow_req, &limits, None),
        Err(SpaceImageryError::TimestampOverflow { .. })
    ));

    Ok(())
}

#[test]
fn overlapping_blur_and_cloud_preserve_both_reasons_and_coarsening_floor()
-> Result<(), Box<dyn std::error::Error>> {
    let limits = SpaceCatalogLimits::default();
    let envelope = GeoBoundingBoxWgs84::new(48.0, 48.1, 2.0, 2.1)?;
    let provider = SpaceDataProvider::new("AerialProvider", "OpenOrtho", true)?;
    let reg = CrossViewRegistration::new(
        VerticalReference::OrthometricDem {
            dem_source: "LiDAR 1m".into(),
            vertical_rmse_m: 0.2,
        },
        CorrespondenceMethod::GroundControlPointsBundleAdjustment { gcp_count: 12 },
        0.5,
        3.0,
    )?;

    // Candidate 1: 0.20 m native aerial patch with BOTH a 25.0 m redaction blur AND a 0.30 cloud defect on the cell.
    // The cloud defect has higher severity rank than blur, but MUST NOT erase the 25.0 m blur floor or receipt reason!
    let mut blurred_and_cloudy = CompositeCandidatePatch {
        patch_id: "patch:aerial_blurred_25m_and_cloudy".into(),
        source_id: "src:aerial:1".into(),
        platform: CompositeSourcePlatform::AerialOrthophoto,
        provider: provider.clone(),
        mission_or_collection: "NationalOrtho".into(),
        instrument_or_sensor: "Cam1".into(),
        footprint: envelope,
        time_window: TimeWindow::new(1_715_000_000_000, 1_715_000_010_000)?,
        native_gsd_m: Some(0.20),
        served_grid_spacing_m: 0.20,
        radiometric_class: SurfaceRadiometricClass::OpticalTrueColor,
        calibration_level: CalibrationLevel::Level2Geophysical,
        stacking_history: None,
        color_mapping: Some(ColorMapping::TrueColor),
        base_integrity: PatchIntegrityState::DeliberatelyBlurredOrRedacted {
            reason: "Statutory security blur".into(),
            coarse_effective_gsd_m: Some(25.0),
        },
        localized_defects: vec![LocalizedPatchDefect::new(
            "def:local_cloud",
            envelope,
            PatchIntegrityState::ObscuredByCloudShadowOrSmoke {
                obscuration_fraction: 0.30,
            },
        )?],
        registration: reg.clone(),
    };

    // Verify effective_verifiable_gsd_m preserves the 25.0 m blur floor even when local cloud is present
    let states = blurred_and_cloudy.applicable_integrity_states_for_cell(&envelope);
    assert_eq!(
        blurred_and_cloudy.effective_verifiable_gsd_m(&states),
        Some(25.0)
    );

    // Candidate 2: Honest 1.0 m cloudy satellite patch (0.30 cloud fraction, no blur).
    // In DegradedOpticalFallback, this 1.0 m patch MUST beat the 25.0 m blurred+cloudy patch!
    let honest_cloudy_1m = CompositeCandidatePatch {
        patch_id: "patch:sat_honest_1m_cloudy".into(),
        source_id: "src:sat:2".into(),
        platform: CompositeSourcePlatform::SatelliteEarthObservation,
        provider: provider.clone(),
        mission_or_collection: "Sat1m".into(),
        instrument_or_sensor: "Imager".into(),
        footprint: envelope,
        time_window: TimeWindow::new(1_715_000_100_000, 1_715_000_110_000)?,
        native_gsd_m: Some(1.0),
        served_grid_spacing_m: 1.0,
        radiometric_class: SurfaceRadiometricClass::OpticalTrueColor,
        calibration_level: CalibrationLevel::Level2Geophysical,
        stacking_history: None,
        color_mapping: Some(ColorMapping::TrueColor),
        base_integrity: PatchIntegrityState::ObscuredByCloudShadowOrSmoke {
            obscuration_fraction: 0.30,
        },
        localized_defects: Vec::new(),
        registration: CrossViewRegistration::new(
            VerticalReference::OrthometricDem {
                dem_source: "GLO-30".into(),
                vertical_rmse_m: 2.0,
            },
            CorrespondenceMethod::OrbitalWcsAndDemOrthorectification,
            1.5,
            5.0,
        )?,
    };

    let req = CompositeStitchRequest {
        target_envelope: envelope,
        grid_rows: 1,
        grid_cols: 1,
        policy: CompositeStitchPolicy::default(),
        candidates: vec![blurred_and_cloudy.clone(), honest_cloudy_1m],
    };
    let plan = plan_globe_composite(&req, &limits, None)?;
    let winner = plan.cells[0]
        .outcome
        .assignment()
        .ok_or("missing degraded winner")?;
    assert_eq!(winner.patch_id, "patch:sat_honest_1m_cloudy");
    assert_eq!(winner.effective_verifiable_gsd_m, Some(1.0));

    // Furthermore, the receipt for `patch:aerial_blurred_25m_and_cloudy` MUST record BOTH the blur reason AND the cloud reason!
    let receipt = plan.cells[0]
        .rejected_or_bypassed
        .iter()
        .find(|r| r.patch_id == "patch:aerial_blurred_25m_and_cloudy")
        .ok_or("missing receipt for blurred_and_cloudy")?;
    assert!(receipt.reasons.contains(
        &CandidateRejectionOrBypassReason::DeliberateBlurOrRedactionBypassed {
            reason: "Statutory security blur".into(),
            coarse_effective_gsd_m: Some(25.0),
        }
    ));
    assert!(receipt.reasons.contains(
        &CandidateRejectionOrBypassReason::CloudOrHazeObscurationBypassed {
            obscuration_fraction: Some(0.30),
        }
    ));

    // Also test UpsampledBeyondNative with unknown cloud cover (`None`): must NOT enter CleanOptical!
    blurred_and_cloudy.patch_id = "patch:upsampled_unknown_cloud".into();
    blurred_and_cloudy.native_gsd_m = Some(10.0);
    blurred_and_cloudy.served_grid_spacing_m = 1.0;
    blurred_and_cloudy.base_integrity = PatchIntegrityState::UpsampledBeyondNative {
        native_gsd_m: 10.0,
        served_grid_spacing_m: 1.0,
        measured_cloud_or_haze_fraction: None,
    };
    blurred_and_cloudy.localized_defects.clear();

    let req_upsampled = CompositeStitchRequest {
        target_envelope: envelope,
        grid_rows: 1,
        grid_cols: 1,
        policy: CompositeStitchPolicy {
            allow_degraded_fallback_when_no_clean_source: false,
            ..Default::default()
        },
        candidates: vec![blurred_and_cloudy],
    };
    let plan_upsampled = plan_globe_composite(&req_upsampled, &limits, None)?;
    assert_eq!(plan_upsampled.clean_optical_cells_count, 0);
    assert_eq!(plan_upsampled.uncovered_cells_count, 1);

    Ok(())
}

#[test]
fn spectral_modality_validation_and_oblique_crew_street_orthorectification()
-> Result<(), Box<dyn std::error::Error>> {
    let limits = SpaceCatalogLimits::default();
    let envelope = GeoBoundingBoxWgs84::new(35.0, 35.1, 139.0, 139.1)?;
    let provider = SpaceDataProvider::new("ESA/NASA", "Public", true)?;

    // B2: Sentinel-1 Microwave observation passed as OpticalTrueColor must be rejected by to_composite_candidate_patch
    let mut s1_entry = SpaceObservationEntry {
        observation_id: "obs:s1:microwave".into(),
        mission: "Sentinel-1A".into(),
        instrument: "C-SAR".into(),
        target: SpaceObservationTarget::EarthObservation {
            footprint: envelope,
            region_label: None,
        },
        spectral_regime: SpectralRegime::Microwave,
        wavelength_range_nm: None,
        time_window: TimeWindow::new(1_710_000_000_000, 1_710_000_010_000)?,
        spatial_resolution: Some(SpatialResolution::GroundSampleDistanceMeters(5.0)),
        provider: provider.clone(),
        scientific_product: ScientificProductRecord::new(
            "s1:grd",
            ScientificProductFormat::GeoTiffCalibratedRadiance,
            CalibrationLevel::Level1BGeoreferenced,
            1.0,
            "v1",
        )?,
        previews: Vec::new(),
    };
    let reg_orbital = CrossViewRegistration::new(
        VerticalReference::OrthometricDem {
            dem_source: "GLO-30".into(),
            vertical_rmse_m: 2.0,
        },
        CorrespondenceMethod::OrbitalWcsAndDemOrthorectification,
        2.0,
        10.0,
    )?;

    let bad_modality = s1_entry.to_composite_candidate_patch(
        "patch:s1:fake_optical",
        CompositeSourcePlatform::SatelliteEarthObservation,
        5.0,
        SurfaceRadiometricClass::OpticalTrueColor,
        PatchIntegrityState::UneditedClearNative {
            cloud_or_haze_fraction: 0.0,
        },
        reg_orbital.clone(),
        &limits,
    );
    assert!(matches!(
        bad_modality,
        Err(SpaceImageryError::InvalidRadiometricModality { .. })
    ));

    // B2: When an entry has both a TrueColor scientific preview and an OutreachRendering preview,
    // to_composite_candidate_patch must NOT take ColorMapping from the outreach preview!
    s1_entry.spectral_regime = SpectralRegime::OpticalVisible;
    s1_entry.previews = vec![
        ImagePreviewRecord::new(
            "prev:outreach",
            ColorMapping::OutreachRendering {
                renderer_attribution: "Press".into(),
                enhancement_notes: "Artistic".into(),
            },
            512,
            512,
            "image/jpeg",
            "https://example.org/outreach.jpg",
        )?,
        ImagePreviewRecord::new(
            "prev:science_rgb",
            ColorMapping::TrueColor,
            512,
            512,
            "image/tiff",
            "https://example.org/rgb.tif",
        )?,
    ];
    let valid_opt_patch = s1_entry.to_composite_candidate_patch(
        "patch:opt:science_preview",
        CompositeSourcePlatform::SatelliteEarthObservation,
        5.0,
        SurfaceRadiometricClass::OpticalTrueColor,
        PatchIntegrityState::UneditedClearNative {
            cloud_or_haze_fraction: 0.0,
        },
        reg_orbital,
        &limits,
    )?;
    assert_eq!(valid_opt_patch.color_mapping, Some(ColorMapping::TrueColor));

    // B3 Probe 1: ISS handheld frame at 30 deg off-nadir with EllipsoidWgs84 and claimed OrbitalWcsAndDemOrthorectification
    let iss_fake_rpc = CompositeCandidatePatch {
        patch_id: "patch:iss:fake_rpc".into(),
        source_id: "src:iss:1".into(),
        platform: CompositeSourcePlatform::SpacecraftHandheldOrCrewFrame,
        provider: provider.clone(),
        mission_or_collection: "ISS Crew".into(),
        instrument_or_sensor: "Nikon".into(),
        footprint: envelope,
        time_window: TimeWindow::new(1_710_000_000_000, 1_710_000_005_000)?,
        native_gsd_m: Some(3.0),
        served_grid_spacing_m: 3.0,
        radiometric_class: SurfaceRadiometricClass::OpticalTrueColor,
        calibration_level: CalibrationLevel::Level1BGeoreferenced,
        stacking_history: None,
        color_mapping: Some(ColorMapping::TrueColor),
        base_integrity: PatchIntegrityState::UneditedClearNative {
            cloud_or_haze_fraction: 0.0,
        },
        localized_defects: Vec::new(),
        registration: CrossViewRegistration::new(
            VerticalReference::EllipsoidWgs84,
            CorrespondenceMethod::OrbitalWcsAndDemOrthorectification,
            4.0,
            30.0,
        )?,
    };

    // B3 Probe 2: Street frame with caller-asserted 0 deg off-nadir but EllipsoidWgs84 (not OrthometricDem)
    let street_ellipsoid = CompositeCandidatePatch {
        patch_id: "patch:street:ellipsoid".into(),
        source_id: "src:street:1".into(),
        platform: CompositeSourcePlatform::CalibratedGroundOrStreetFrame,
        provider,
        mission_or_collection: "Mapillary".into(),
        instrument_or_sensor: "StreetCam".into(),
        footprint: envelope,
        time_window: TimeWindow::new(1_710_000_000_000, 1_710_000_005_000)?,
        native_gsd_m: Some(0.05),
        served_grid_spacing_m: 0.05,
        radiometric_class: SurfaceRadiometricClass::OpticalTrueColor,
        calibration_level: CalibrationLevel::Level1BGeoreferenced,
        stacking_history: None,
        color_mapping: Some(ColorMapping::TrueColor),
        base_integrity: PatchIntegrityState::UneditedClearNative {
            cloud_or_haze_fraction: 0.0,
        },
        localized_defects: Vec::new(),
        registration: CrossViewRegistration::new(
            VerticalReference::EllipsoidWgs84,
            CorrespondenceMethod::CalibratedCameraPoseAndTerrainIntersection,
            1.0,
            0.0,
        )?,
    };

    let req_b3 = CompositeStitchRequest {
        target_envelope: envelope,
        grid_rows: 1,
        grid_cols: 1,
        policy: CompositeStitchPolicy::default(),
        candidates: vec![iss_fake_rpc, street_ellipsoid],
    };
    let plan_b3 = plan_globe_composite(&req_b3, &limits, None)?;
    assert_eq!(plan_b3.clean_optical_cells_count, 0);
    assert_eq!(plan_b3.uncovered_cells_count, 1);
    assert!(plan_b3.cells[0].rejected_or_bypassed.iter().any(|r| {
        r.patch_id == "patch:iss:fake_rpc"
            && r.reasons.contains(
                &CandidateRejectionOrBypassReason::IncompatiblePlatformCorrespondence {
                    platform: CompositeSourcePlatform::SpacecraftHandheldOrCrewFrame,
                    correspondence_method: CorrespondenceMethod::OrbitalWcsAndDemOrthorectification,
                },
            )
    }));
    assert!(plan_b3.cells[0].rejected_or_bypassed.iter().any(|r| {
        r.patch_id == "patch:street:ellipsoid"
            && r.reasons.contains(
                &CandidateRejectionOrBypassReason::MissingTerrainOrthorectificationForObliqueView,
            )
    }));

    Ok(())
}

#[test]
fn single_instant_mosaic_history_total_order_permutations_and_receipt_budget()
-> Result<(), Box<dyn std::error::Error>> {
    let limits = SpaceCatalogLimits::default();
    let envelope = GeoBoundingBoxWgs84::new(0.0, 4.0, 10.0, 14.0)?;
    let provider = SpaceDataProvider::new("Planet/NICFI", "NICFI", true)?;
    let reg = CrossViewRegistration::new(
        VerticalReference::OrthometricDem {
            dem_source: "Copernicus DEM".into(),
            vertical_rmse_m: 2.0,
        },
        CorrespondenceMethod::OrbitalWcsAndDemOrthorectification,
        3.0,
        5.0,
    )?;

    // B4: A single 30-day monthly-mosaic patch over a 4x4 grid MUST report is_single_instant_observation = false
    let thirty_days_ms = 30 * 86_400 * 1_000_i64;
    let monthly_mosaic_patch = CompositeCandidatePatch {
        patch_id: "patch:nicfi:monthly_mosaic".into(),
        source_id: "src:nicfi:2025_06".into(),
        platform: CompositeSourcePlatform::SatelliteEarthObservation,
        provider: provider.clone(),
        mission_or_collection: "Planet NICFI Monthly".into(),
        instrument_or_sensor: "Dove".into(),
        footprint: envelope,
        time_window: TimeWindow::new(1_717_200_000_000, 1_717_200_000_000 + thirty_days_ms)?,
        native_gsd_m: Some(4.77),
        served_grid_spacing_m: 4.77,
        radiometric_class: SurfaceRadiometricClass::OpticalTrueColor,
        calibration_level: CalibrationLevel::Level2Geophysical,
        stacking_history: Some(StackingHistory::new(42, StackingMethod::MedianStack, 0.35)?),
        color_mapping: Some(ColorMapping::TrueColor),
        base_integrity: PatchIntegrityState::UneditedClearNative {
            cloud_or_haze_fraction: 0.0,
        },
        localized_defects: Vec::new(),
        registration: reg.clone(),
    };

    let req_mosaic = CompositeStitchRequest {
        target_envelope: envelope,
        grid_rows: 4,
        grid_cols: 4,
        policy: CompositeStitchPolicy::default(),
        candidates: vec![monthly_mosaic_patch],
    };
    let plan_mosaic = plan_globe_composite(&req_mosaic, &limits, None)?;
    assert_eq!(plan_mosaic.clean_optical_cells_count, 16);
    assert!(!plan_mosaic.is_single_instant_observation);

    // P1 #3: Three candidates with GSDs 6e-10 apart must produce the exact same winner across all 6 input permutations
    let short_window = TimeWindow::new(1_717_200_000_000, 1_717_200_001_000)?;
    let make_sub_nanometer_candidate = |id: &str, gsd: f64, rmse: f64| CompositeCandidatePatch {
        patch_id: id.into(),
        source_id: id.into(),
        platform: CompositeSourcePlatform::SatelliteEarthObservation,
        provider: provider.clone(),
        mission_or_collection: "PermTest".into(),
        instrument_or_sensor: "Sensor".into(),
        footprint: envelope,
        time_window: short_window,
        native_gsd_m: Some(gsd),
        served_grid_spacing_m: gsd,
        radiometric_class: SurfaceRadiometricClass::OpticalTrueColor,
        calibration_level: CalibrationLevel::Level2Geophysical,
        stacking_history: None,
        color_mapping: Some(ColorMapping::TrueColor),
        base_integrity: PatchIntegrityState::UneditedClearNative {
            cloud_or_haze_fraction: 0.0,
        },
        localized_defects: Vec::new(),
        registration: CrossViewRegistration {
            vertical_reference: reg.vertical_reference.clone(),
            correspondence_method: reg.correspondence_method.clone(),
            horizontal_rmse_m: rmse,
            off_nadir_angle_deg: 5.0,
        },
    };

    let c1 = make_sub_nanometer_candidate("patch:c1", 1.0, 5.0);
    let c2 = make_sub_nanometer_candidate("patch:c2", 1.0 + 6e-10, 2.0);
    let c3 = make_sub_nanometer_candidate("patch:c3", 1.0 + 1.2e-9, 1.0);

    let permutations = [
        vec![c1.clone(), c2.clone(), c3.clone()],
        vec![c1.clone(), c3.clone(), c2.clone()],
        vec![c2.clone(), c1.clone(), c3.clone()],
        vec![c2.clone(), c3.clone(), c1.clone()],
        vec![c3.clone(), c1.clone(), c2.clone()],
        vec![c3, c2, c1],
    ];
    for perm in permutations {
        let req_perm = CompositeStitchRequest {
            target_envelope: envelope,
            grid_rows: 1,
            grid_cols: 1,
            policy: CompositeStitchPolicy::default(),
            candidates: perm,
        };
        let plan_perm = plan_globe_composite(&req_perm, &limits, None)?;
        let winner = plan_perm.cells[0]
            .outcome
            .assignment()
            .ok_or("missing winner")?;
        assert_eq!(winner.patch_id, "patch:c1");
    }

    // Non-blocking #3: Receipt budget caps total stored receipts and tracks omitted count
    let small_receipt_limits = SpaceCatalogLimits {
        max_composite_rejected_receipts: 1,
        ..Default::default()
    };
    let req_budget = CompositeStitchRequest {
        target_envelope: envelope,
        grid_rows: 1,
        grid_cols: 1,
        policy: CompositeStitchPolicy::default(),
        candidates: vec![
            make_sub_nanometer_candidate("patch:b1", 1.0, 1.0),
            make_sub_nanometer_candidate("patch:b2", 2.0, 1.0),
            make_sub_nanometer_candidate("patch:b3", 3.0, 1.0),
        ],
    };
    let plan_budget = plan_globe_composite(&req_budget, &small_receipt_limits, None)?;
    assert_eq!(plan_budget.cells[0].rejected_or_bypassed.len(), 1);
    assert_eq!(plan_budget.cells[0].omitted_rejected_receipts_count, 1);
    assert_eq!(plan_budget.total_omitted_rejected_receipts_count, 1);

    Ok(())
}
