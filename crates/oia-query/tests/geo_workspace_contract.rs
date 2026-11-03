//! Contract tests for reversible event clustering, distinct evidence counts,
//! `LineageRelationKind` reuse, mixed-stance source class accounting,
//! `CalendarDateOnly` non-midnight discipline, and non-destructive reassessment
//! (`GEO-01`, `GEO-03`, `A04`, `#23`).

use std::collections::{BTreeMap, BTreeSet};

use oia_domain::witness::AccountOrigin;
use oia_domain::{ActorId, OpaqueId, VersionId};
use oia_news_claims::{ClaimAssessmentState, LineageRelationKind};
use oia_query::geo_workspace::{
    ClusterConflict, ClusterLimits, ClusterMatchCriteria, DistinctEvidenceCounts,
    EarliestKnownSourceQualification, EventClusterLedger, EvidenceLineageLink, GeoClusteringError,
    GeoEvidenceItem, GeoEvidenceKind, LocationQualification, ReportedClusteringTime,
    TemporalComparisonOutcome,
};

/// Verifies `GEO-01` and `docs/architecture.md` §3.1: `CalendarDateOnly` never
/// converts a date-only historical record to a midnight UTC timestamp, and
/// `ApproximateGazetteerCentroid` requires a strictly positive radius.
#[test]
fn calendar_date_only_never_masquerades_as_midnight_utc() -> Result<(), Box<dyn std::error::Error>>
{
    let limits = ClusterLimits::default();
    let max_text = limits.max_text_bytes();

    assert!(matches!(
        LocationQualification::gazetteer_centroid("Invalid zero radius", 0.0, max_text),
        Err(GeoClusteringError::InvalidDistanceOrRadius { .. })
    ));
    let centroid =
        LocationQualification::gazetteer_centroid("Synthetic Basin Centroid", 15_000.0, max_text)?;
    assert!(centroid.is_approximate());

    let qual = EarliestKnownSourceQualification::new(
        OpaqueId::new("source:synth-archive-1997")?,
        VersionId::new("ver:archive-1997-v1")?,
        "1997-03-18",
        "First published 5 days after reported sighting in regional digest",
        max_text,
    )?;
    let date_only = ReportedClusteringTime::calendar_date_only(
        "13 March 1997",
        "America/Phoenix (date only)",
        1997,
        3,
        13,
        Some(qual),
        max_text,
    )?;
    assert!(date_only.is_calendar_date_only());
    assert!(date_only.earliest_known_source().is_some());

    // Comparing a CalendarDateOnly record with an ExactInstant on the same UTC date
    // must report `CalendarDateOnlyCannotResolveSubDayGap` rather than a fabricated
    // millisecond offset from 00:00:00Z.
    let evening_instant = ReportedClusteringTime::exact_instant(
        "1997-03-13T20:30:00Z",
        "UTC",
        858_285_000_000,
        None,
        max_text,
    )?;
    assert_eq!(
        date_only.compare_temporal_separation(&evening_instant),
        TemporalComparisonOutcome::CalendarDateOnlyCannotResolveSubDayGap {
            minimum_whole_day_separation: 0
        }
    );

    // Two CalendarDateOnly records compare by whole calendar days only.
    let later_date = ReportedClusteringTime::calendar_date_only(
        "16 March 1997",
        "America/Phoenix (date only)",
        1997,
        3,
        16,
        None,
        max_text,
    )?;
    assert_eq!(
        date_only.compare_temporal_separation(&later_date),
        TemporalComparisonOutcome::CalendarDateSeparationDays(3)
    );
    Ok(())
}

/// Verifies `GEO-03`, `A04`, and `AGENTS.md` §2:
/// - `LineageRelationKind` (`Republishes`, `Quotes`, `Supersedes`) collapses syndicated
///   reprints and quotes into their upstream independent source class;
/// - Mixed-stance source classes (`Supported` + `Contradicted`) are counted in
///   `mixed_stance_independent_source_count` and never inflate
///   `supportive_independent_source_count`;
/// - `AccountOrigin::Simulated` items are excluded from observed counts;
/// - `merge_clusters`, `split_cluster`, and `revert_decision` are append-only,
///   retain all historical cluster versions, and never delete decision records.
#[test]
fn reversible_clustering_lineage_and_mixed_stance_counts() -> Result<(), Box<dyn std::error::Error>>
{
    let limits = ClusterLimits::default();
    let max_text = limits.max_text_bytes();
    let mut ledger = EventClusterLedger::new(limits);

    let criteria = ClusterMatchCriteria::new(
        10_000.0,
        600_000,
        0,
        BTreeSet::from([String::from("formation")]),
    )?;

    let base_time = ReportedClusteringTime::exact_instant(
        "2026-11-01T21:00:00Z",
        "UTC",
        1_762_030_800_000,
        None,
        max_text,
    )?;

    // 1. Primary witness account (`Supported`, independent source class #1).
    let witness = GeoEvidenceItem::new(
        OpaqueId::new("item:witness-a")?,
        VersionId::new("ver:witness-a-1")?,
        OpaqueId::new("source:synth-witness-a")?,
        Some(String::from("sha256:aaaa")),
        GeoEvidenceKind::WitnessAccount {
            origin: AccountOrigin::Reported,
        },
        ClaimAssessmentState::Supported,
        Vec::new(),
        base_time.clone(),
        LocationQualification::surveyed(OpaqueId::new("loc:surveyed-a")?, Some(50.0))?,
        BTreeMap::from([(String::from("formation"), String::from("triangle"))]),
        limits,
    )?;
    ledger.register_item_with_initial_cluster(
        witness,
        OpaqueId::new("cluster:c1")?,
        VersionId::new("ver:c1-v1")?,
        criteria.clone(),
        "Initial witness cluster",
    )?;

    // 2. Original newspaper article (`Supported`, root of source class #2).
    let article_1 = GeoEvidenceItem::new(
        OpaqueId::new("item:article-1")?,
        VersionId::new("ver:article-1-v1")?,
        OpaqueId::new("source:synth-daily-herald")?,
        Some(String::from("sha256:bbbb")),
        GeoEvidenceKind::PublishedArticle,
        ClaimAssessmentState::Supported,
        Vec::new(),
        base_time.clone(),
        LocationQualification::surveyed(OpaqueId::new("loc:surveyed-b")?, Some(100.0))?,
        BTreeMap::from([(String::from("formation"), String::from("linear-row"))]),
        limits,
    )?;
    ledger.register_item_with_initial_cluster(
        article_1,
        OpaqueId::new("cluster:c2")?,
        VersionId::new("ver:c2-v1")?,
        criteria.clone(),
        "Original newspaper article cluster",
    )?;

    // 3. Wire article quoting `item:article-1` (`LineageRelationKind::Quotes`) but assessing
    // the claim as `Contradicted`. Must collapse into source class #2 and mark class #2 as
    // mixed-stance (`mixed_stance_independent_source_count == 1`), NOT supportive.
    let article_2_quote = GeoEvidenceItem::new(
        OpaqueId::new("item:article-2-quote")?,
        VersionId::new("ver:article-2-v1")?,
        OpaqueId::new("source:synth-wire-service")?,
        Some(String::from("sha256:cccc")),
        GeoEvidenceKind::PublishedArticle,
        ClaimAssessmentState::Contradicted,
        vec![EvidenceLineageLink::new(
            LineageRelationKind::Quotes,
            OpaqueId::new("item:article-1")?,
        )],
        base_time.clone(),
        LocationQualification::surveyed(OpaqueId::new("loc:surveyed-b")?, Some(100.0))?,
        BTreeMap::from([(String::from("formation"), String::from("linear-row"))]),
        limits,
    )?;
    ledger.register_item_with_initial_cluster(
        article_2_quote,
        OpaqueId::new("cluster:c3")?,
        VersionId::new("ver:c3-v1")?,
        criteria.clone(),
        "Wire article quoting Herald with contradictory assessment",
    )?;

    // 4. Simulated witness account (`AccountOrigin::Simulated`) — excluded from observed counts.
    let sim_account = GeoEvidenceItem::new(
        OpaqueId::new("item:sim-account")?,
        VersionId::new("ver:sim-account-v1")?,
        OpaqueId::new("source:synth-generator")?,
        Some(String::from("sha256:dddd")),
        GeoEvidenceKind::WitnessAccount {
            origin: AccountOrigin::Simulated,
        },
        ClaimAssessmentState::Supported,
        Vec::new(),
        base_time,
        LocationQualification::surveyed(OpaqueId::new("loc:surveyed-c")?, None)?,
        BTreeMap::from([(String::from("formation"), String::from("triangle"))]),
        limits,
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
        "Include quoting wire article",
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

    assert_eq!(
        merged_all.counts(),
        DistinctEvidenceCounts {
            article_count: 2,
            account_count: 1,
            sensor_observation_count: 0,
            independent_source_count: 2,
            supportive_independent_source_count: 1,
            contradictory_independent_source_count: 0,
            mixed_stance_independent_source_count: 1,
            proposed_event_count: 1,
            simulated_item_count: 1,
        }
    );
    assert!(
        merged_all
            .conflicts()
            .iter()
            .any(|c| matches!(c, ClusterConflict::MixedStanceSourceClass { .. }))
    );

    // Revert `decision:m3`, split `cluster:c123`, and revert the split.
    // All prior decisions and archived cluster versions remain intact.
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
    assert_eq!(ledger.decision_history().len(), 4);

    ledger.split_cluster(
        &OpaqueId::new("cluster:c123")?,
        OpaqueId::new("cluster:witness-only")?,
        VersionId::new("ver:witness-only-v1")?,
        BTreeSet::from([OpaqueId::new("item:witness-a")?]),
        OpaqueId::new("cluster:articles-only")?,
        VersionId::new("ver:articles-only-v1")?,
        BTreeSet::from([
            OpaqueId::new("item:article-1")?,
            OpaqueId::new("item:article-2-quote")?,
        ]),
        criteria,
        OpaqueId::new("decision:split-1")?,
        actor.clone(),
        "Split witness account from newspaper formation reports",
    )?;
    assert_eq!(ledger.clusters().len(), 3);

    ledger.revert_decision(
        &OpaqueId::new("decision:split-1")?,
        OpaqueId::new("decision:rev-split-1")?,
        &[VersionId::new("ver:c123-restored-2")?],
        actor,
        "Restore combined cluster after reviewing formation notes",
    )?;
    assert_eq!(ledger.clusters().len(), 2);
    assert_eq!(ledger.decision_history().len(), 6);
    assert!(
        ledger
            .archived_cluster_versions()
            .contains_key(&VersionId::new("ver:c1234-v1")?)
    );
    Ok(())
}

/// Verifies `AGENTS.md` §4: revising a clustered evidence item preserves its
/// prior version and flags affected clusters with `needs_reassessment = true`
/// without mutating their pinned item versions in place.
#[test]
fn item_revision_flags_clusters_for_reassessment_without_in_place_mutation()
-> Result<(), Box<dyn std::error::Error>> {
    let limits = ClusterLimits::default();
    let max_text = limits.max_text_bytes();
    let mut ledger = EventClusterLedger::new(limits);

    let criteria = ClusterMatchCriteria::new(5_000.0, 300_000, 0, BTreeSet::new())?;
    let t = ReportedClusteringTime::exact_instant(
        "2026-11-01T22:00:00Z",
        "UTC",
        1_762_034_400_000,
        None,
        max_text,
    )?;
    let item_v1 = GeoEvidenceItem::new(
        OpaqueId::new("item:radar-track-1")?,
        VersionId::new("ver:radar-track-1-v1")?,
        OpaqueId::new("source:synth-radar-west")?,
        Some(String::from("sha256:1010")),
        GeoEvidenceKind::SensorObservation {
            origin: AccountOrigin::Reported,
        },
        ClaimAssessmentState::Supported,
        Vec::new(),
        t.clone(),
        LocationQualification::surveyed(OpaqueId::new("loc:radar-site-1")?, Some(25.0))?,
        BTreeMap::new(),
        limits,
    )?;
    ledger.register_item_with_initial_cluster(
        item_v1,
        OpaqueId::new("cluster:radar-c1")?,
        VersionId::new("ver:radar-c1-v1")?,
        criteria,
        "Initial radar observation cluster",
    )?;

    let item_v2 = GeoEvidenceItem::new(
        OpaqueId::new("item:radar-track-1")?,
        VersionId::new("ver:radar-track-1-v2")?,
        OpaqueId::new("source:synth-radar-west")?,
        Some(String::from("sha256:2020")),
        GeoEvidenceKind::SensorObservation {
            origin: AccountOrigin::Reported,
        },
        ClaimAssessmentState::Disputed,
        Vec::new(),
        t,
        LocationQualification::surveyed(OpaqueId::new("loc:radar-site-1")?, Some(25.0))?,
        BTreeMap::new(),
        limits,
    )?;

    let flagged = ledger.revise_item(
        &VersionId::new("ver:radar-track-1-v1")?,
        item_v2,
        "Clutter filter calibration corrected range gate",
    )?;
    assert_eq!(flagged, vec![OpaqueId::new("cluster:radar-c1")?]);

    let cluster = &ledger.clusters()[&OpaqueId::new("cluster:radar-c1")?];
    assert!(cluster.needs_reassessment());
    assert_eq!(
        cluster.pinned_item_versions()[&OpaqueId::new("item:radar-track-1")?],
        VersionId::new("ver:radar-track-1-v1")?
    );
    assert_eq!(
        ledger.item_version_history()[&OpaqueId::new("item:radar-track-1")?].len(),
        2
    );
    Ok(())
}
