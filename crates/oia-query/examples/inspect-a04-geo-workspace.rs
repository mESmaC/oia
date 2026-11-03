//! Read-only inspection example for the `#23` geographic workspace, reversible
//! clustering, discrete footprint mosaic, and cross-view navigation kernel (`A04`).

use std::collections::{BTreeMap, BTreeSet};
use std::process::ExitCode;

use oia_domain::witness::AccountOrigin;
use oia_domain::{ActorId, OpaqueId, VersionId};
use oia_query::geo_workspace::{
    ClockAlignmentStatus, ClusterMatchCriteria, EarliestKnownSourceQualification,
    EventClusterLedger, FootprintMosaic, GeoBoundingBoxWgs84, GeoCoverageEnvelope, GeoEvidenceItem,
    GeoEvidenceKind, GeoLocation, GeoModality, GeoPointWgs84, GeoWorkspaceError,
    GeoWorkspaceLimits, LocalViewFilter, ModalityCoverageRecord, ModalitySourceStatus, MosaicTile,
    MosaicTileGeometry, ReportedEventTime, ReportedTimePrecision, SourceLineageRelation,
    TileCalibrationState, TrackDataBasis, ViewingCone2D, WorkspaceSurface,
};

fn run_a04_inspection() -> Result<(), Box<dyn std::error::Error>> {
    let limits = GeoWorkspaceLimits::default();
    let max_text = limits.max_text_bytes;

    // 1. Dateline-crossing query envelope (178.5°E to -178.5°W) and multi-modality coverage (GEO-01, GEO-02, A04).
    let query_bounds = GeoBoundingBoxWgs84::new(18.0, 22.0, 178.5, -178.5)?;
    let mut required_modalities = BTreeSet::new();
    required_modalities.insert(GeoModality::SatelliteImagery);
    required_modalities.insert(GeoModality::PublicCameraRecords);
    required_modalities.insert(GeoModality::AircraftVesselActivity);
    required_modalities.insert(GeoModality::WitnessReports);

    let coverage_records = vec![
        ModalityCoverageRecord::new(
            GeoModality::SatelliteImagery,
            OpaqueId::new("source:sat-landsat-archive")?,
            VersionId::new("ver:sat-2026-11-01-a")?,
            ModalitySourceStatus::StaleSnapshot {
                age_ms: 14 * 3_600_000,
            },
            Some((1_762_000_000_000, 1_762_003_600_000)),
            Some(query_bounds),
            TrackDataBasis::NotApplicable,
            "30m multispectral; cloud-obscured eastern sector",
            max_text,
        )?,
        ModalityCoverageRecord::new(
            GeoModality::PublicCameraRecords,
            OpaqueId::new("source:cam-harbor-east")?,
            VersionId::new("ver:cam-harbor-east-01")?,
            ModalitySourceStatus::MissingHistory,
            None,
            None,
            TrackDataBasis::NotApplicable,
            "Live-only public directory feed; no retained archive for event window",
            max_text,
        )?,
        ModalityCoverageRecord::new(
            GeoModality::AircraftVesselActivity,
            OpaqueId::new("source:ais-pacific-corridor")?,
            VersionId::new("ver:ais-corridor-01")?,
            ModalitySourceStatus::PartialCoverage,
            Some((1_762_000_000_000, 1_762_003_600_000)),
            Some(query_bounds),
            TrackDataBasis::IncompleteTrackWithDropouts { dropout_windows: 2 },
            "Terrestrial AIS receiver range limit; 28-minute receiver dropout",
            max_text,
        )?,
        ModalityCoverageRecord::new(
            GeoModality::WitnessReports,
            OpaqueId::new("source:witness-intake-local")?,
            VersionId::new("ver:witness-batch-01")?,
            ModalitySourceStatus::Available,
            Some((1_761_999_000_000, 1_762_004_000_000)),
            Some(query_bounds),
            TrackDataBasis::NotApplicable,
            "Witness testimony with uncertain clock and gazetteer location",
            max_text,
        )?,
    ];

    let envelope = GeoCoverageEnvelope::new(
        OpaqueId::new("query:a04-dateline-corridor")?,
        query_bounds,
        1_762_000_000_000,
        1_762_003_600_000,
        required_modalities,
        coverage_records,
    )?;
    let conclusion = envelope.evaluate_scene_conclusion();

    // 2. Source-aware event clustering with distinct counts and reversible split/merge (GEO-03, A04).
    let mut ledger = EventClusterLedger::new(limits);
    let criteria = ClusterMatchCriteria::new(
        50_000.0,
        3_600_000,
        BTreeSet::from([String::from("apparent_shape")]),
    )?;

    let mut props_witness = BTreeMap::new();
    props_witness.insert(String::from("apparent_shape"), String::from("amber-orb"));

    let witness_time = ReportedEventTime::new(
        "approx 21:10–21:25 local ship time",
        "Etc/GMT-12",
        Some(720),
        ReportedTimePrecision::BoundedInterval {
            start_utc_ms: 1_762_000_200_000,
            end_utc_ms: 1_762_001_100_000,
            witness_uncertainty_note: Some(String::from(
                "Bridge logbook entry; wristwatch unverified against GPS",
            )),
        },
        None,
        max_text,
    )?;
    let witness_item = GeoEvidenceItem::new(
        OpaqueId::new("item:witness-01")?,
        VersionId::new("ver:item-witness-01")?,
        OpaqueId::new("source:witness-vessel-alpha")?,
        Some(String::from("sha256:1111aaaa")),
        GeoEvidenceKind::WitnessAccount {
            origin: AccountOrigin::Reported,
        },
        SourceLineageRelation::IndependentRoot,
        witness_time,
        GeoLocation::surveyed_point(GeoPointWgs84::new(19.85, 179.90)?, Some(850.0))?,
        props_witness,
        max_text,
    )?;
    ledger.register_item_with_initial_cluster(
        witness_item,
        OpaqueId::new("cluster:alpha")?,
        VersionId::new("ver:cluster-alpha-v1")?,
        criteria.clone(),
        "Initial primary witness account near 180th meridian",
    )?;

    // Historical / archival article with calendar-date-only time and earliest-known-source qualification (GEO-01).
    let earliest_qual = EarliestKnownSourceQualification::new(
        OpaqueId::new("source:maritime-digest-reprint")?,
        VersionId::new("ver:digest-2026-11-02")?,
        "2026-11-02",
        "Original harbour dispatch unlocated; known only via next-day maritime digest",
        max_text,
    )?;
    let article_time = ReportedEventTime::calendar_date_only(
        "1 November 2026 (date-only archival clipping)",
        "Etc/GMT+12",
        Some(-720),
        2026,
        11,
        1,
        Some(earliest_qual),
        max_text,
    )?;
    let mut props_article = BTreeMap::new();
    props_article.insert(
        String::from("apparent_shape"),
        String::from("elongated-streak"),
    );

    let article_root = GeoEvidenceItem::new(
        OpaqueId::new("item:article-root")?,
        VersionId::new("ver:item-article-root")?,
        OpaqueId::new("source:maritime-digest-reprint")?,
        Some(String::from("sha256:2222bbbb")),
        GeoEvidenceKind::PublishedArticle,
        SourceLineageRelation::IndependentRoot,
        article_time.clone(),
        GeoLocation::gazetteer_centroid(
            "Western Ridge Atoll (gazetteer centroid)",
            GeoPointWgs84::new(19.92, -179.92)?,
            12_000.0,
            max_text,
        )?,
        props_article.clone(),
        max_text,
    )?;
    ledger.register_item_with_initial_cluster(
        article_root,
        OpaqueId::new("cluster:beta")?,
        VersionId::new("ver:cluster-beta-v1")?,
        criteria.clone(),
        "Archival digest article on opposite side of antimeridian",
    )?;

    // Syndicated reprint of `item:article-root` (must increment `article_count` without inflating `independent_source_count`).
    let article_reprint = GeoEvidenceItem::new(
        OpaqueId::new("item:article-reprint")?,
        VersionId::new("ver:item-article-reprint")?,
        OpaqueId::new("source:syndicated-wire-copy")?,
        Some(String::from("sha256:2222bbbb")),
        GeoEvidenceKind::PublishedArticle,
        SourceLineageRelation::Republishes {
            upstream_item_id: OpaqueId::new("item:article-root")?,
        },
        article_time,
        GeoLocation::gazetteer_centroid(
            "Western Ridge Atoll (gazetteer centroid)",
            GeoPointWgs84::new(19.92, -179.92)?,
            12_000.0,
            max_text,
        )?,
        props_article,
        max_text,
    )?;
    ledger.register_item_with_initial_cluster(
        article_reprint,
        OpaqueId::new("cluster:gamma")?,
        VersionId::new("ver:cluster-gamma-v1")?,
        criteria.clone(),
        "Wire reprint of maritime digest article",
    )?;

    let actor = ActorId::new("actor:analyst-01")?;
    ledger.merge_clusters(
        &OpaqueId::new("cluster:alpha")?,
        &OpaqueId::new("cluster:beta")?,
        OpaqueId::new("cluster:merged-ab")?,
        VersionId::new("ver:cluster-merged-ab-v1")?,
        criteria.clone(),
        OpaqueId::new("decision:merge-01")?,
        actor.clone(),
        "Merge cross-dateline witness report and digest article within 25 km",
    )?;
    ledger.merge_clusters(
        &OpaqueId::new("cluster:merged-ab")?,
        &OpaqueId::new("cluster:gamma")?,
        OpaqueId::new("cluster:merged-all")?,
        VersionId::new("ver:cluster-merged-all-v1")?,
        criteria,
        OpaqueId::new("decision:merge-02")?,
        actor,
        "Attach syndicated wire reprint to unified event cluster",
    )?;

    let merged = ledger
        .clusters()
        .get(&OpaqueId::new("cluster:merged-all")?)
        .ok_or(GeoWorkspaceError::UnknownClusterId(OpaqueId::new(
            "cluster:merged-all",
        )?))?;

    // 3. Discrete footprint and viewing-cone mosaic (GEO-04).
    let mut mosaic = FootprintMosaic::new(limits);
    mosaic.add_tile(MosaicTile {
        tile_id: OpaqueId::new("tile:cam-jetty-north")?,
        source_id: OpaqueId::new("source:cam-jetty-north")?,
        artifact_version: VersionId::new("ver:tile-cam-01")?,
        status: ModalitySourceStatus::Available,
        geometry: MosaicTileGeometry::CameraViewingCone(ViewingCone2D::new(
            GeoPointWgs84::new(19.80, 179.88)?,
            25.0,
            70.0,
            50.0,
            35_000.0,
        )?),
        calibration: TileCalibrationState::VerifiedAtCapture {
            calibration_version: VersionId::new("ver:cal-cam-01")?,
            angular_uncertainty_deg: 0.4,
        },
        clock_alignment: ClockAlignmentStatus::Synchronized {
            reference_clock: String::from("NTP-stratum-2"),
            offset_ms: -120,
            uncertainty_ms: 45,
        },
    })?;
    mosaic.add_tile(MosaicTile {
        tile_id: OpaqueId::new("tile:cam-lighthouse")?,
        source_id: OpaqueId::new("source:cam-lighthouse")?,
        artifact_version: VersionId::new("ver:tile-cam-02")?,
        status: ModalitySourceStatus::Available,
        geometry: MosaicTileGeometry::CameraViewingCone(ViewingCone2D::new(
            GeoPointWgs84::new(19.95, -179.95)?,
            240.0,
            60.0,
            50.0,
            35_000.0,
        )?),
        calibration: TileCalibrationState::InvalidatedByCameraMovement {
            prior_calibration_version: VersionId::new("ver:cal-cam-02-old")?,
            movement_reason: String::from("Pan-tilt housing shifted after storm gust"),
        },
        clock_alignment: ClockAlignmentStatus::LostSyncReceiptTimeOnly {
            warning: String::from("NTP unreachable; frame receipt timestamps only"),
        },
    })?;

    let alignment = mosaic.evaluate_cross_view_alignment(
        &OpaqueId::new("tile:cam-jetty-north")?,
        &OpaqueId::new("tile:cam-lighthouse")?,
        "Candidate bearing intersection on luminous marine target",
    )?;

    // 4. Shared workspace context and route-back navigation (CASE-02, UX-02).
    let mut ctx = oia_query::geo_workspace::SharedWorkspaceContext::new(
        OpaqueId::new("case:pacific-dateline-a04")?,
        limits,
        WorkspaceSurface::Globe,
        query_bounds,
        1_762_000_000_000,
        1_762_003_600_000,
        "Initial globe overview of dateline corridor",
    )?;
    ctx.set_local_filter(
        WorkspaceSurface::Media,
        LocalViewFilter {
            area_override: None,
            time_window_override_utc_ms: Some((1_762_000_200_000, 1_762_001_100_000)),
            modality_filter: Some(BTreeSet::from([GeoModality::PublicCameraRecords])),
            include_simulated: false,
        },
    )?;
    ctx.coordinate_event_selection(
        WorkspaceSurface::Documents,
        query_bounds,
        1_762_000_000_000,
        1_762_003_600_000,
        Some(OpaqueId::new("cluster:merged-all")?),
        BTreeSet::from([
            OpaqueId::new("item:witness-01")?,
            OpaqueId::new("item:article-root")?,
        ]),
        "Inspect dossier and archival digest for merged cluster",
    )?;

    println!("=== OIA A04 Geographic Workspace & Reversible Clustering Inspection ===");
    println!(
        "Query envelope: {} (crosses antimeridian: {})",
        envelope.query_id,
        envelope.requested_bounds.crosses_antimeridian()
    );
    println!("Scene coverage conclusion: {conclusion:?}");
    println!(
        "Merged cluster {} ({}): articles={}, accounts={}, independent_sources={}, proposed_events={}, conflicts={}",
        merged.cluster_id,
        merged.version_id,
        merged.counts.article_count,
        merged.counts.account_count,
        merged.counts.independent_source_count,
        merged.counts.proposed_event_count,
        merged.conflicts.len()
    );
    println!("Mosaic presentation mode: {:?}", mosaic.presentation_mode());
    println!("Cross-view alignment status: {:?}", alignment.status);
    println!(
        "Workspace surface: {:?} (route_back depth: {}, divergent filter surfaces: {})",
        ctx.current().active_surface,
        ctx.route_back().len(),
        ctx.surfaces_with_divergent_local_filters().len()
    );

    Ok(())
}

fn main() -> ExitCode {
    match run_a04_inspection() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}
