//! Read-only inspection example for the `#23` reversible event clustering and
//! distinct evidence counts kernel (`GEO-03`, `A04`).

use std::collections::{BTreeMap, BTreeSet};
use std::process::ExitCode;

use oia_domain::witness::AccountOrigin;
use oia_domain::{ActorId, OpaqueId, VersionId};
use oia_news_claims::{ClaimAssessmentState, LineageRelationKind};
use oia_query::geo_workspace::{
    ClusterLimits, ClusterMatchCriteria, EarliestKnownSourceQualification, EventClusterLedger,
    EvidenceLineageLink, GeoClusteringError, GeoEvidenceItem, GeoEvidenceKind,
    LocationQualification, ReportedClusteringTime,
};

fn run_a04_clustering_inspection() -> Result<(), Box<dyn std::error::Error>> {
    let limits = ClusterLimits::default();
    let max_text = limits.max_text_bytes();
    let mut ledger = EventClusterLedger::new(limits);

    let criteria = ClusterMatchCriteria::new(
        25_000.0,
        1_800_000,
        0,
        BTreeSet::from([String::from("apparent_shape")]),
    )?;

    // 1. Primary witness account on 1 November 2026:
    //    UTC interval 2026-11-01 09:10:00Z .. 09:25:00Z (`1_761_988_200_000` .. `1_761_989_100_000`),
    //    which corresponds to 21:10–21:25 local ship time in `Etc/GMT-12` (UTC+12).
    let witness_time = ReportedClusteringTime::bounded_interval(
        "approx 21:10-21:25 local ship time (1 Nov 2026)",
        "Etc/GMT-12 (UTC+12)",
        1_761_988_200_000,
        1_761_989_100_000,
        Some(String::from(
            "Bridge logbook entry; wristwatch unverified against GPS",
        )),
        None,
        max_text,
    )?;
    let witness_item = GeoEvidenceItem::new(
        OpaqueId::new("item:witness-01")?,
        VersionId::new("ver:item-witness-01-v1")?,
        OpaqueId::new("source:synth-witness-vessel-alpha")?,
        Some(String::from("sha256:1111aaaa")),
        GeoEvidenceKind::WitnessAccount {
            origin: AccountOrigin::Reported,
        },
        ClaimAssessmentState::Supported,
        Vec::new(),
        witness_time,
        LocationQualification::surveyed(OpaqueId::new("loc:surveyed-bridge-fix-01")?, Some(850.0))?,
        BTreeMap::from([(String::from("apparent_shape"), String::from("amber-orb"))]),
        limits,
    )?;
    ledger.register_item_with_initial_cluster(
        witness_item,
        OpaqueId::new("cluster:alpha")?,
        VersionId::new("ver:cluster-alpha-v1")?,
        criteria.clone(),
        "Initial primary witness account",
    )?;

    // 2. Historical archival article with calendar-date-only time (never midnight UTC) and earliest-known-source qualification.
    let earliest_qual = EarliestKnownSourceQualification::new(
        OpaqueId::new("source:synth-maritime-digest")?,
        VersionId::new("ver:digest-2026-11-02")?,
        "2026-11-02",
        "Original harbour dispatch unlocated; known only via next-day maritime digest",
        max_text,
    )?;
    let article_time = ReportedClusteringTime::calendar_date_only(
        "1 November 2026 (date-only archival clipping)",
        "Etc/GMT-12 (UTC+12)",
        2026,
        11,
        1,
        Some(earliest_qual),
        max_text,
    )?;
    let article_root = GeoEvidenceItem::new(
        OpaqueId::new("item:article-root")?,
        VersionId::new("ver:item-article-root-v1")?,
        OpaqueId::new("source:synth-maritime-digest")?,
        Some(String::from("sha256:2222bbbb")),
        GeoEvidenceKind::PublishedArticle,
        ClaimAssessmentState::Supported,
        Vec::new(),
        article_time.clone(),
        LocationQualification::gazetteer_centroid(
            "Synthetic Atoll Gazetteer Centroid",
            12_000.0,
            max_text,
        )?,
        BTreeMap::from([(
            String::from("apparent_shape"),
            String::from("elongated-streak"),
        )]),
        limits,
    )?;
    ledger.register_item_with_initial_cluster(
        article_root,
        OpaqueId::new("cluster:beta")?,
        VersionId::new("ver:cluster-beta-v1")?,
        criteria.clone(),
        "Archival digest article",
    )?;

    // 3. Syndicated reprint of `item:article-root` with conflicting assessment (`Contradicted`)
    // to verify that `LineageRelationKind::Republishes` collapses into the same source class
    // and marks the class as `MixedStanceSourceClass` rather than inflating `supportive_independent_source_count`.
    let article_reprint = GeoEvidenceItem::new(
        OpaqueId::new("item:article-reprint")?,
        VersionId::new("ver:item-article-reprint-v1")?,
        OpaqueId::new("source:synth-syndicated-wire")?,
        Some(String::from("sha256:2222bbbb")),
        GeoEvidenceKind::PublishedArticle,
        ClaimAssessmentState::Contradicted,
        vec![EvidenceLineageLink::new(
            LineageRelationKind::Republishes,
            OpaqueId::new("item:article-root")?,
        )],
        article_time,
        LocationQualification::gazetteer_centroid(
            "Synthetic Atoll Gazetteer Centroid",
            12_000.0,
            max_text,
        )?,
        BTreeMap::from([(
            String::from("apparent_shape"),
            String::from("elongated-streak"),
        )]),
        limits,
    )?;
    ledger.register_item_with_initial_cluster(
        article_reprint,
        OpaqueId::new("cluster:gamma")?,
        VersionId::new("ver:cluster-gamma-v1")?,
        criteria.clone(),
        "Syndicated wire reprint with contradictory editorial stance",
    )?;

    ledger.record_pairwise_distance_m(
        &OpaqueId::new("item:witness-01")?,
        &OpaqueId::new("item:article-root")?,
        14_200.0,
    )?;
    ledger.record_pairwise_distance_m(
        &OpaqueId::new("item:witness-01")?,
        &OpaqueId::new("item:article-reprint")?,
        14_200.0,
    )?;
    ledger.record_pairwise_distance_m(
        &OpaqueId::new("item:article-root")?,
        &OpaqueId::new("item:article-reprint")?,
        0.0,
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
        "Merge witness report and digest article within 25 km threshold",
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
        .ok_or(GeoClusteringError::UnknownClusterId(OpaqueId::new(
            "cluster:merged-all",
        )?))?;
    let counts = merged.counts();

    println!("=== OIA #23 Reversible Event Clustering & Distinct Counts Inspection ===");
    println!(
        "Merged cluster {} ({}): articles={}, accounts={}, sensors={}, independent_sources={} (supportive={}, contradictory={}, mixed={}), proposed_events={}, simulated={}, conflicts={}",
        merged.cluster_id(),
        merged.version_id(),
        counts.article_count,
        counts.account_count,
        counts.sensor_observation_count,
        counts.independent_source_count,
        counts.supportive_independent_source_count,
        counts.contradictory_independent_source_count,
        counts.mixed_stance_independent_source_count,
        counts.proposed_event_count,
        counts.simulated_item_count,
        merged.conflicts().len()
    );
    println!(
        "Archived cluster snapshots retained: {}, decision history entries: {}",
        ledger.archived_cluster_versions().len(),
        ledger.decision_history().len()
    );

    Ok(())
}

fn main() -> ExitCode {
    match run_a04_clustering_inspection() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}
