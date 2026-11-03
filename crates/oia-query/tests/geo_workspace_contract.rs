//! Contract tests for geographic query coverage, historical time/source
//! qualifications, reversible event clustering, footprint mosaics, and
//! shared workspace navigation (`GEO-01`..=`GEO-04`, `CASE-02`, `UX-02`, `A04`).

use std::collections::{BTreeMap, BTreeSet};

use oia_domain::witness::AccountOrigin;
use oia_domain::{ActorId, OpaqueId, VersionId};
use oia_query::geo_workspace::{
    ClockAlignmentStatus, ClusterConflict, ClusterMatchCriteria, CrossViewAlignmentStatus,
    DistinctEvidenceCounts, EarliestKnownSourceQualification, EventClusterLedger, FootprintMosaic,
    GeoBoundingBoxWgs84, GeoCoverageEnvelope, GeoEvidenceItem, GeoEvidenceKind, GeoLocation,
    GeoModality, GeoPointWgs84, GeoWorkspaceError, GeoWorkspaceLimits, LocalViewFilter,
    ModalityCoverageRecord, ModalitySourceStatus, MosaicPresentationMode, MosaicTile,
    MosaicTileGeometry, ReportedEventTime, ReportedTimePrecision, SceneCoverageConclusion,
    SharedWorkspaceContext, SourceLineageRelation, TileCalibrationState, TrackDataBasis,
    ViewingCone2D, WorkspaceSurface,
};

/// Verifies `GEO-01`, `GEO-02`, and `A04`: dateline-crossing bounding boxes,
/// calendar-date-only historical accounts (never midnight UTC), approximate
/// gazetteer centroids, and missing feeds reported as coverage gaps rather
/// than empty-scene conclusions.
#[test]
fn dateline_coverage_and_historical_time_contracts() -> Result<(), Box<dyn std::error::Error>> {
    let limits = GeoWorkspaceLimits::default();
    let max_text = limits.max_text_bytes;

    // Antimeridian-crossing box: 175°E to -175°W.
    let dateline_box = GeoBoundingBoxWgs84::new(-25.0, -15.0, 175.0, -175.0)?;
    assert!(dateline_box.crosses_antimeridian());
    assert!(dateline_box.contains_point(GeoPointWgs84::new(-20.0, 179.2)?));
    assert!(dateline_box.contains_point(GeoPointWgs84::new(-20.0, -178.4)?));
    assert!(!dateline_box.contains_point(GeoPointWgs84::new(-20.0, 160.0)?));

    // Gazetteer centroid requires positive uncertainty radius and stays labeled approximate.
    let centroid = GeoLocation::gazetteer_centroid(
        "Phoenix basin centroid",
        GeoPointWgs84::new(33.45, -112.07)?,
        15_000.0,
        max_text,
    )?;
    assert!(centroid.is_approximate());
    assert!(matches!(
        GeoLocation::gazetteer_centroid(
            "Invalid zero radius",
            GeoPointWgs84::new(33.45, -112.07)?,
            0.0,
            max_text
        ),
        Err(GeoWorkspaceError::InvalidDistanceOrRadius { .. })
    ));

    // Calendar-date-only historical account preserves date-only status and earliest-known-source qualification.
    let qual = EarliestKnownSourceQualification::new(
        OpaqueId::new("source:archive-1997")?,
        VersionId::new("ver:archive-1997-v1")?,
        "1997-03-18",
        "First published 5 days after reported sighting in regional newspaper",
        max_text,
    )?;
    let date_only = ReportedEventTime::calendar_date_only(
        "13 March 1997",
        "America/Phoenix",
        Some(-420),
        1997,
        3,
        13,
        Some(qual),
        max_text,
    )?;
    assert!(date_only.is_calendar_date_only());
    assert!(date_only.earliest_known_source().is_some());

    // Missing camera history + stale imagery + incomplete tracks must never yield an empty-scene conclusion.
    let envelope = GeoCoverageEnvelope::new(
        OpaqueId::new("query:a04-gap-check")?,
        dateline_box,
        1_762_000_000_000,
        1_762_003_600_000,
        BTreeSet::from([
            GeoModality::SatelliteImagery,
            GeoModality::PublicCameraRecords,
            GeoModality::AircraftVesselActivity,
        ]),
        vec![
            ModalityCoverageRecord::new(
                GeoModality::SatelliteImagery,
                OpaqueId::new("source:sat-01")?,
                VersionId::new("ver:sat-01")?,
                ModalitySourceStatus::StaleSnapshot { age_ms: 86_400_000 },
                Some((1_761_900_000_000, 1_761_903_600_000)),
                Some(dateline_box),
                TrackDataBasis::NotApplicable,
                "10m optical snapshot 24h stale",
                max_text,
            )?,
            ModalityCoverageRecord::new(
                GeoModality::PublicCameraRecords,
                OpaqueId::new("source:cam-01")?,
                VersionId::new("ver:cam-01")?,
                ModalitySourceStatus::MissingHistory,
                None,
                None,
                TrackDataBasis::NotApplicable,
                "No retained archive",
                max_text,
            )?,
            ModalityCoverageRecord::new(
                GeoModality::AircraftVesselActivity,
                OpaqueId::new("source:adsb-01")?,
                VersionId::new("ver:adsb-01")?,
                ModalitySourceStatus::Available,
                Some((1_762_000_000_000, 1_762_003_600_000)),
                Some(dateline_box),
                TrackDataBasis::ContainsDisplayInterpolation {
                    max_gap_ms: 120_000,
                },
                "ADS-B with 120s display interpolation across receiver gap",
                max_text,
            )?,
        ],
    )?;

    assert_eq!(
        envelope.evaluate_scene_conclusion(),
        SceneCoverageConclusion::CoverageGapCannotInferEmptyScene {
            missing_or_unavailable_modalities: vec![GeoModality::PublicCameraRecords],
            stale_or_partial_modalities: vec![GeoModality::SatelliteImagery],
            interpolated_or_incomplete_track_sources: vec![OpaqueId::new("source:adsb-01")?],
        }
    );
    Ok(())
}

/// Verifies `GEO-03` and `A04`: distinct article/account/independent-source/event
/// counts (collapsing reprints and excluding simulated items), conflict detection,
/// and reversible merge/split/revert decisions.
#[test]
fn reversible_clustering_and_distinct_counts_contract() -> Result<(), Box<dyn std::error::Error>> {
    let limits = GeoWorkspaceLimits::default();
    let max_text = limits.max_text_bytes;
    let mut ledger = EventClusterLedger::new(limits);

    let criteria = ClusterMatchCriteria::new(
        10_000.0,
        600_000,
        BTreeSet::from([String::from("formation")]),
    )?;

    let base_time = ReportedEventTime::new(
        "2026-11-01T21:00:00Z",
        "UTC",
        Some(0),
        ReportedTimePrecision::ExactInstant {
            utc_ms: 1_762_030_800_000,
        },
        None,
        max_text,
    )?;

    // 1. Primary witness account (independent source #1).
    let witness = GeoEvidenceItem::new(
        OpaqueId::new("item:witness-a")?,
        VersionId::new("ver:witness-a-1")?,
        OpaqueId::new("source:witness-a")?,
        Some(String::from("sha256:aaaa")),
        GeoEvidenceKind::WitnessAccount {
            origin: AccountOrigin::Reported,
        },
        SourceLineageRelation::IndependentRoot,
        base_time.clone(),
        GeoLocation::surveyed_point(GeoPointWgs84::new(33.4484, -112.0740)?, Some(50.0))?,
        BTreeMap::from([(String::from("formation"), String::from("triangle"))]),
        max_text,
    )?;
    ledger.register_item_with_initial_cluster(
        witness,
        OpaqueId::new("cluster:c1")?,
        VersionId::new("ver:c1-v1")?,
        criteria.clone(),
        "Initial witness cluster",
    )?;

    // 2. Original newspaper article (independent source #2).
    let article_1 = GeoEvidenceItem::new(
        OpaqueId::new("item:article-1")?,
        VersionId::new("ver:article-1-v1")?,
        OpaqueId::new("source:daily-herald")?,
        Some(String::from("sha256:bbbb")),
        GeoEvidenceKind::PublishedArticle,
        SourceLineageRelation::IndependentRoot,
        base_time.clone(),
        GeoLocation::surveyed_point(GeoPointWgs84::new(33.4500, -112.0700)?, Some(100.0))?,
        BTreeMap::from([(String::from("formation"), String::from("linear-row"))]),
        max_text,
    )?;
    ledger.register_item_with_initial_cluster(
        article_1,
        OpaqueId::new("cluster:c2")?,
        VersionId::new("ver:c2-v1")?,
        criteria.clone(),
        "Original newspaper article cluster",
    )?;

    // 3. Syndicated reprint of `item:article-1` + simulated item (neither may inflate independent_source_count).
    let article_2_reprint = GeoEvidenceItem::new(
        OpaqueId::new("item:article-2-reprint")?,
        VersionId::new("ver:article-2-v1")?,
        OpaqueId::new("source:wire-reprint")?,
        Some(String::from("sha256:cccc")),
        GeoEvidenceKind::PublishedArticle,
        SourceLineageRelation::Republishes {
            upstream_item_id: OpaqueId::new("item:article-1")?,
        },
        base_time.clone(),
        GeoLocation::surveyed_point(GeoPointWgs84::new(33.4500, -112.0700)?, Some(100.0))?,
        BTreeMap::from([(String::from("formation"), String::from("linear-row"))]),
        max_text,
    )?;
    ledger.register_item_with_initial_cluster(
        article_2_reprint,
        OpaqueId::new("cluster:c3")?,
        VersionId::new("ver:c3-v1")?,
        criteria.clone(),
        "Syndicated reprint cluster",
    )?;

    let sim_account = GeoEvidenceItem::new(
        OpaqueId::new("item:sim-account")?,
        VersionId::new("ver:sim-account-v1")?,
        OpaqueId::new("source:synthetic-generator")?,
        Some(String::from("sha256:dddd")),
        GeoEvidenceKind::WitnessAccount {
            origin: AccountOrigin::Simulated,
        },
        SourceLineageRelation::IndependentRoot,
        base_time,
        GeoLocation::surveyed_point(GeoPointWgs84::new(33.4490, -112.0720)?, None)?,
        BTreeMap::from([(String::from("formation"), String::from("triangle"))]),
        max_text,
    )?;
    ledger.register_item_with_initial_cluster(
        sim_account,
        OpaqueId::new("cluster:c4")?,
        VersionId::new("ver:c4-v1")?,
        criteria.clone(),
        "Simulated benchmark account",
    )?;

    let actor = ActorId::new("actor:reviewer-1")?;
    ledger.merge_clusters(
        &OpaqueId::new("cluster:c1")?,
        &OpaqueId::new("cluster:c2")?,
        OpaqueId::new("cluster:c12")?,
        VersionId::new("ver:c12-v1")?,
        criteria.clone(),
        OpaqueId::new("decision:m1")?,
        actor.clone(),
        "Merge witness and original article",
    )?;
    ledger.merge_clusters(
        &OpaqueId::new("cluster:c12")?,
        &OpaqueId::new("cluster:c3")?,
        OpaqueId::new("cluster:c123")?,
        VersionId::new("ver:c123-v1")?,
        criteria.clone(),
        OpaqueId::new("decision:m2")?,
        actor.clone(),
        "Include wire reprint",
    )?;
    let merged_all = ledger
        .merge_clusters(
            &OpaqueId::new("cluster:c123")?,
            &OpaqueId::new("cluster:c4")?,
            OpaqueId::new("cluster:c1234")?,
            VersionId::new("ver:c1234-v1")?,
            criteria.clone(),
            OpaqueId::new("decision:m3")?,
            actor.clone(),
            "Attach simulated control item to verify count isolation",
        )?
        .clone();

    // 2 articles, 1 real witness account, 2 independent sources (witness + root article), 1 proposed cluster, 1 simulated item.
    assert_eq!(
        merged_all.counts,
        DistinctEvidenceCounts {
            article_count: 2,
            account_count: 1,
            independent_source_count: 2,
            proposed_event_count: 1,
            simulated_item_count: 1,
        }
    );
    assert!(merged_all.conflicts.iter().any(|c| matches!(
        c,
        ClusterConflict::PropertyMismatch { property_key, .. } if property_key == "formation"
    )));
    assert!(merged_all.conflicts.iter().any(|c| matches!(
        c,
        ClusterConflict::SimulatedEvidencePresent { item_id }
            if item_id.as_str() == "item:sim-account"
    )));

    // Revert the last merge (`decision:m3`), then split `cluster:c123` and revert the split.
    ledger.revert_decision(
        &OpaqueId::new("decision:m3")?,
        OpaqueId::new("decision:rev-m3")?,
        &[
            VersionId::new("ver:c123-restored")?,
            VersionId::new("ver:c4-restored")?,
        ],
        actor.clone(),
        "Separate simulated item from observed cluster",
    )?;
    assert_eq!(ledger.clusters().len(), 2);

    ledger.split_cluster(
        &OpaqueId::new("cluster:c123")?,
        OpaqueId::new("cluster:witness-only")?,
        VersionId::new("ver:witness-only-v1")?,
        BTreeSet::from([OpaqueId::new("item:witness-a")?]),
        OpaqueId::new("cluster:articles-only")?,
        VersionId::new("ver:articles-only-v1")?,
        BTreeSet::from([
            OpaqueId::new("item:article-1")?,
            OpaqueId::new("item:article-2-reprint")?,
        ]),
        criteria,
        OpaqueId::new("decision:split-1")?,
        actor.clone(),
        "Split witness account from conflicting newspaper formation reports",
    )?;
    assert_eq!(ledger.clusters().len(), 3);
    assert_eq!(ledger.ledger_wide_counts().proposed_event_count, 3);

    ledger.revert_decision(
        &OpaqueId::new("decision:split-1")?,
        OpaqueId::new("decision:rev-split-1")?,
        &[VersionId::new("ver:c123-restored-2")?],
        actor,
        "Restore combined cluster after reviewing formation notes",
    )?;
    assert_eq!(ledger.clusters().len(), 2);
    assert!(
        ledger
            .clusters()
            .contains_key(&OpaqueId::new("cluster:c123")?)
    );
    Ok(())
}

/// Verifies `GEO-04`, `CASE-02`, and `UX-02`: discrete viewing-cone mosaic,
/// degraded alignment when camera calibration is invalidated or clock sync is
/// lost, local filter divergence disclosure, and route-back navigation.
#[test]
fn mosaic_alignment_and_shared_navigation_contract() -> Result<(), Box<dyn std::error::Error>> {
    let limits = GeoWorkspaceLimits::default();
    let mut mosaic = FootprintMosaic::new(limits);
    assert_eq!(
        mosaic.presentation_mode(),
        MosaicPresentationMode::DiscreteFootprintAndConeMosaicOnly
    );

    let cone_a = ViewingCone2D::new(
        GeoPointWgs84::new(35.0, -106.6)?,
        90.0,
        60.0,
        100.0,
        15_000.0,
    )?;
    let target_east = GeoPointWgs84::new(35.0, -106.5)?;
    let target_west = GeoPointWgs84::new(35.0, -106.7)?;
    assert!(cone_a.contains_point(target_east));
    assert!(!cone_a.contains_point(target_west));

    mosaic.add_tile(MosaicTile {
        tile_id: OpaqueId::new("tile:cam-a")?,
        source_id: OpaqueId::new("source:cam-a")?,
        artifact_version: VersionId::new("ver:cam-a-v1")?,
        status: ModalitySourceStatus::Available,
        geometry: MosaicTileGeometry::CameraViewingCone(cone_a),
        calibration: TileCalibrationState::VerifiedAtCapture {
            calibration_version: VersionId::new("ver:cal-a")?,
            angular_uncertainty_deg: 0.25,
        },
        clock_alignment: ClockAlignmentStatus::Synchronized {
            reference_clock: String::from("GPS-PPS"),
            offset_ms: 5,
            uncertainty_ms: 10,
        },
    })?;
    mosaic.add_tile(MosaicTile {
        tile_id: OpaqueId::new("tile:cam-b")?,
        source_id: OpaqueId::new("source:cam-b")?,
        artifact_version: VersionId::new("ver:cam-b-v1")?,
        status: ModalitySourceStatus::Available,
        geometry: MosaicTileGeometry::CameraViewingCone(ViewingCone2D::new(
            GeoPointWgs84::new(35.05, -106.5)?,
            180.0,
            60.0,
            100.0,
            15_000.0,
        )?),
        calibration: TileCalibrationState::InvalidatedByCameraMovement {
            prior_calibration_version: VersionId::new("ver:cal-b-old")?,
            movement_reason: String::from("Tripod bumped between calibration and capture"),
        },
        clock_alignment: ClockAlignmentStatus::LostSyncReceiptTimeOnly {
            warning: String::from("Camera battery reset clock; receipt time only"),
        },
    })?;

    assert_eq!(
        mosaic.tiles_covering_point(target_east),
        vec![OpaqueId::new("tile:cam-a")?, OpaqueId::new("tile:cam-b")?]
    );

    let alignment = mosaic.evaluate_cross_view_alignment(
        &OpaqueId::new("tile:cam-a")?,
        &OpaqueId::new("tile:cam-b")?,
        "Cross-bearing check on east-ridge light",
    )?;
    match alignment.status {
        CrossViewAlignmentStatus::DegradedAlignment { reasons } => {
            assert_eq!(reasons.len(), 2);
        }
        other => panic!("expected degraded alignment, got {other:?}"),
    }

    // Shared workspace navigation and local filter divergence (CASE-02, UX-02).
    let shared_box = GeoBoundingBoxWgs84::new(34.8, 35.2, -106.8, -106.3)?;
    let mut ctx = SharedWorkspaceContext::new(
        OpaqueId::new("case:sandia-ridge")?,
        limits,
        WorkspaceSurface::Globe,
        shared_box,
        1_762_000_000_000,
        1_762_003_600_000,
        "Initial globe context",
    )?;
    ctx.set_local_filter(
        WorkspaceSurface::Sky,
        LocalViewFilter {
            area_override: None,
            time_window_override_utc_ms: Some((1_762_001_000_000, 1_762_002_000_000)),
            modality_filter: None,
            include_simulated: false,
        },
    )?;
    assert_eq!(ctx.surfaces_with_divergent_local_filters().len(), 1);

    ctx.coordinate_event_selection(
        WorkspaceSurface::Report,
        shared_box,
        1_762_000_000_000,
        1_762_003_600_000,
        Some(OpaqueId::new("cluster:c123")?),
        BTreeSet::from([OpaqueId::new("item:witness-a")?]),
        "Jump to report view for cluster:c123",
    )?;
    assert_eq!(ctx.current().active_surface, WorkspaceSurface::Report);

    ctx.navigate_back()?;
    assert_eq!(ctx.current().active_surface, WorkspaceSurface::Globe);
    ctx.navigate_forward()?;
    assert_eq!(ctx.current().active_surface, WorkspaceSurface::Report);
    Ok(())
}
