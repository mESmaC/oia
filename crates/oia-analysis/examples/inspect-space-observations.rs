//! Runnable inspection example for space imagery catalog, scientific provenance, and mission stream classification (`SPACE-01`–`SPACE-03`, `A05`).

use oia_analysis::geo_context::provisional::{GeoBoundingBoxWgs84, TimeWindow};
use oia_analysis::space_imagery::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== OIA Space Imagery and Scientific Observation Catalog (v0.1, #26) ===\n");

    let limits = SpaceCatalogLimits::default();
    let mut catalog = SpaceImageryCatalog::new();

    // 1. Earth Observation: Landsat-9 OLI-2
    let l9_bbox = GeoBoundingBoxWgs84::new(34.8, 35.8, -116.2, -115.0)?;
    let l9_provider = SpaceDataProvider::new("USGS/NASA", "Landsat Ground Network", true)?;
    let l9_sci = ScientificProductRecord::new(
        "l9:mojave:oli2:c2_l2",
        ScientificProductFormat::GeoTiffCalibratedRadiance,
        CalibrationLevel::Level2Geophysical,
        0.032,
        "Collection-2_v02",
    )?;
    let l9_true_preview = ImagePreviewRecord::new(
        "l9:mojave:preview:true_color",
        ColorMapping::TrueColor,
        1024,
        1024,
        "image/png",
        "https://landsatlook.usgs.gov/data/browse/2024/07/01/LC09_L2SP_039035_20240701_02_T1.png",
    )?;

    let l9_entry = SpaceObservationEntry {
        observation_id: "obs:landsat9:mojave_2024_07".into(),
        mission: "Landsat-9".into(),
        instrument: "OLI-2".into(),
        target: SpaceObservationTarget::EarthObservation {
            footprint: l9_bbox,
            region_label: Some("Mojave Desert Study Area".into()),
        },
        spectral_regime: SpectralRegime::OpticalVisible,
        wavelength_range_nm: Some(WavelengthRangeNm::new(430.0, 670.0)?),
        time_window: TimeWindow::new(1_719_811_200_000, 1_719_811_230_000)?,
        spatial_resolution: Some(SpatialResolution::GroundSampleDistanceMeters(30.0)),
        provider: l9_provider,
        scientific_product: l9_sci,
        previews: vec![l9_true_preview],
    };
    catalog.add_observation(l9_entry, &limits)?;

    // 2. Space Telescope: JWST NIRCam Carina Deep Observation
    let jwst_provider = SpaceDataProvider::new("STScI MAST", "JWST Science Operations", true)?;
    let mut jwst_sci = ScientificProductRecord::new(
        "jwst:carina:nircam:f444w_sci",
        ScientificProductFormat::FitsPrimaryImage,
        CalibrationLevel::Level1BGeoreferenced,
        1850.5,
        "calwebb_1.8.2",
    )?;
    jwst_sci.stacking_history = Some(StackingHistory::new(8, StackingMethod::MedianStack, 0.04)?);
    jwst_sci.publication_reference = Some("doi:10.17909/T9-STScI-JWST-CARINA".into());

    let jwst_outreach_preview = ImagePreviewRecord::new(
        "jwst:carina:preview:outreach",
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

    let jwst_entry = SpaceObservationEntry {
        observation_id: "obs:jwst:carina_2022".into(),
        mission: "JWST".into(),
        instrument: "NIRCam".into(),
        target: SpaceObservationTarget::Celestial {
            target_name: "Carina Nebula NGC 3372".into(),
            ra_deg: 161.26,
            dec_deg: -59.87,
            fov_radius_deg: 0.25,
        },
        spectral_regime: SpectralRegime::NearInfrared,
        wavelength_range_nm: Some(WavelengthRangeNm::new(600.0, 5000.0)?),
        time_window: TimeWindow::new(1_657_670_400_000, 1_657_672_250_000)?,
        spatial_resolution: Some(SpatialResolution::AngularResolutionArcsec(0.065)),
        provider: jwst_provider,
        scientific_product: jwst_sci,
        previews: vec![jwst_outreach_preview],
    };
    catalog.add_observation(jwst_entry, &limits)?;

    // 3. Solar Observation: SDO AIA Coronal Loop
    let sdo_provider = SpaceDataProvider::new("NASA GSFC", "SDO Science Data Center", true)?;
    let sdo_sci = ScientificProductRecord::new(
        "sdo:aia:171:level1",
        ScientificProductFormat::FitsPrimaryImage,
        CalibrationLevel::Level1ARadiometric,
        2.0,
        "aia_prep_2024",
    )?;
    let sdo_preview = ImagePreviewRecord::new(
        "sdo:aia:171:preview",
        ColorMapping::SingleBandGreyscale {
            band_name: "Fe IX 17.1 nm".into(),
        },
        4096,
        4096,
        "image/png",
        "https://sdo.gsfc.nasa.gov/assets/img/browse/2024/07/01/20240701_000000_4096_0171.jpg",
    )?;

    let sdo_entry = SpaceObservationEntry {
        observation_id: "obs:sdo:aia_171_2024_07".into(),
        mission: "SDO".into(),
        instrument: "AIA".into(),
        target: SpaceObservationTarget::Solar {
            feature_label: "Active Region AR 3720".into(),
            active_region_no: Some(3720),
        },
        spectral_regime: SpectralRegime::ExtremeUltraviolet,
        wavelength_range_nm: Some(WavelengthRangeNm::new(17.1, 17.1)?),
        time_window: TimeWindow::new(1_719_811_200_000, 1_719_811_202_000)?,
        spatial_resolution: Some(SpatialResolution::AngularResolutionArcsec(0.6)),
        provider: sdo_provider,
        scientific_product: sdo_sci,
        previews: vec![sdo_preview],
    };
    catalog.add_observation(sdo_entry, &limits)?;

    println!(
        "Registered {} space observation entries in catalog.",
        catalog.observations().len()
    );
    for obs in catalog.observations() {
        println!(
            " - [{}] Mission: {} | Instrument: {} | Regime: {} | Cal: {}",
            obs.observation_id,
            obs.mission,
            obs.instrument,
            obs.spectral_regime,
            obs.scientific_product.calibration_level
        );
        for prev in &obs.previews {
            println!(
                "    Preview [{}]: {} (Outreach/Sim: {})",
                prev.preview_id,
                prev.color_mapping.classification_code(),
                prev.color_mapping.is_outreach_or_simulated()
            );
        }
    }

    // 4. Mission Feeds Demonstration (SPACE-03)
    println!("\n=== Mission Feeds and Stream Classification (SPACE-03) ===");
    let mut iss_feed = SpaceMissionFeed::new(
        "feed:iss:live",
        "International Space Station",
        "ISS External High Definition Video",
        StreamClassification::LiveCamera,
        MissionFeedStatus::NominalActive,
        -150,
    )?;
    let bookmark = MissionFeedBookmark::new(
        "bm:iss:01",
        1_719_811_215_000,
        &iss_feed.classification,
        "Pacific terminator crossing",
        100,
        "Normal orbital sunset; camera auto-exposure active",
    )?;
    iss_feed.add_bookmark(bookmark, &limits)?;

    let sim_feed = SpaceMissionFeed::new(
        "feed:iss:trajectory_model",
        "International Space Station",
        "ISS Ephemeris Orbital Visualization",
        StreamClassification::SimulationOrRecreation {
            engine: "Orbitron SGP4 Propagator Model".into(),
        },
        MissionFeedStatus::NominalActive,
        0,
    )?;

    println!(
        " - Feed [{}]: '{}' -> Classification: {} (Live: {})",
        iss_feed.feed_id,
        iss_feed.feed_title,
        iss_feed.classification.classification_token(),
        iss_feed.classification.is_live()
    );
    println!(
        " - Feed [{}]: '{}' -> Classification: {} (Live: {})",
        sim_feed.feed_id,
        sim_feed.feed_title,
        sim_feed.classification.classification_token(),
        sim_feed.classification.is_live()
    );

    // 5. Mosaic Coverage Evaluation (GEO-04)
    println!("\n=== Mosaic Coverage Evaluation (GEO-04) ===");
    let query_box = GeoBoundingBoxWgs84::new(34.0, 36.0, -117.0, -114.0)?;
    let mosaic_report = catalog.evaluate_mosaic_coverage(&query_box, None, &limits)?;
    println!(
        "Query envelope: lat [{}, {}], lon [{}, {}]",
        query_box.south_lat_deg(),
        query_box.north_lat_deg(),
        query_box.west_lon_deg(),
        query_box.east_lon_deg()
    );
    println!("Assessment: {:?}", mosaic_report.assessment);
    println!(
        "Contributing footprints: {}",
        mosaic_report.footprints.len()
    );

    println!("\n=== Verification Completed Cleanly ===");
    Ok(())
}
