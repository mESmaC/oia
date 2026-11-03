//! Contract tests for reversible event clustering, distinct evidence counts,
//! `LineageRelationKind` reuse, mixed-stance source class accounting,
//! `CalendarDateOnly` non-midnight discipline, and non-destructive reassessment
//! (`GEO-01`, `GEO-03`, `A04`, `#23`).

use std::collections::{BTreeMap, BTreeSet};

use oia_domain::witness::AccountOrigin;
use oia_domain::{ActorId, OpaqueId, VersionId};
use oia_news_claims::{ClaimAssessmentState, LineageRelationKind};
use oia_query::geo_workspace::{
    ClusterConflict, ClusterLimitKind, ClusterLimits, ClusterMatchCriteria, DistinctEvidenceCounts,
    EarliestKnownSourceQualification, EventClusterLedger, EvidenceLineageLink, GeoClusteringError,
    GeoEvidenceItem, GeoEvidenceKind, LocationQualification, ReportedClusteringTime,
    TemporalComparisonOutcome,
};

/// Verifies `GEO-01` and `docs/architecture.md` §3.1: `CalendarDateOnly` never
/// converts a date-only historical record to a midnight UTC timestamp,
/// `minimum_whole_day_separation` uses the timezone-safe `-2` bound (`B7`),
/// and `LocationQualification` rejects invalid radii (`B6`).
#[test]
fn calendar_date_only_never_masquerades_as_midnight_utc() -> Result<(), Box<dyn std::error::Error>>
{
    let limits = ClusterLimits::default();
    let max_text = limits.max_text_bytes();

    assert!(matches!(
        LocationQualification::gazetteer_centroid("Invalid zero radius", 0.0, max_text),
        Err(GeoClusteringError::InvalidDistanceOrRadius { .. })
    ));
    assert!(matches!(
        LocationQualification::gazetteer_centroid("Invalid NaN radius", f64::NAN, max_text),
        Err(GeoClusteringError::InvalidDistanceOrRadius { .. })
    ));
    assert!(matches!(
        LocationQualification::surveyed(OpaqueId::new("loc:neg")?, Some(-5.0)),
        Err(GeoClusteringError::InvalidDistanceOrRadius { .. })
    ));
    let centroid =
        LocationQualification::gazetteer_centroid("Synthetic Basin Centroid", 15_000.0, max_text)?;
    assert!(centroid.is_approximate());

    let qual = EarliestKnownSourceQualification::new(
        OpaqueId::new("source:synth-archive-2024")?,
        VersionId::new("ver:archive-2024-v1")?,
        "2024-06-20",
        "Synthetic test fixture: archival clipping dated 5 days after reported synthetic observation",
        max_text,
    )?;
    let date_only = ReportedClusteringTime::calendar_date_only(
        "15 June 2024 (synthetic date-only record)",
        "Synthetic Local Time (date only)",
        2024,
        6,
        15,
        Some(qual),
        max_text,
    )?;
    assert!(date_only.is_calendar_date_only());
    assert!(date_only.earliest_known_source().is_some());

    // Comparing a CalendarDateOnly record with an ExactInstant on the same UTC date
    // must report `CalendarDateOnlyCannotResolveSubDayGap` rather than a fabricated
    // millisecond offset from 00:00:00Z.
    // 2024-06-15T20:30:00Z = 1_718_483_400_000 ms
    let evening_instant = ReportedClusteringTime::exact_instant(
        "2024-06-15T20:30:00Z",
        "UTC",
        1_718_483_400_000,
        None,
        max_text,
    )?;
    assert_eq!(
        date_only.compare_temporal_separation(&evening_instant),
        TemporalComparisonOutcome::CalendarDateOnlyCannotResolveSubDayGap {
            minimum_whole_day_separation: 0
        }
    );

    // B7 boundary check: Date-only 2024-06-15 compared with instant 2024-06-13T23:59:59Z
    // (`1_718_323_199_000` ms, 2 UTC days apart). Under UTC+14, local 2024-06-15 00:00 is
    // 2024-06-14T10:00Z (~10 hours after 2024-06-13T23:59:59Z), so the true separation
    // can be < 24 hours (0 whole days).
    let two_utc_days_earlier = ReportedClusteringTime::exact_instant(
        "2024-06-13T23:59:59Z",
        "UTC",
        1_718_323_199_000,
        None,
        max_text,
    )?;
    assert_eq!(
        date_only.compare_temporal_separation(&two_utc_days_earlier),
        TemporalComparisonOutcome::CalendarDateOnlyCannotResolveSubDayGap {
            minimum_whole_day_separation: 0
        }
    );

    // 3 UTC days earlier (`2024-06-12T23:59:59Z` = `1_718_236_799_000` ms) guarantees at least 1 whole day.
    let three_utc_days_earlier = ReportedClusteringTime::exact_instant(
        "2024-06-12T23:59:59Z",
        "UTC",
        1_718_236_799_000,
        None,
        max_text,
    )?;
    assert_eq!(
        date_only.compare_temporal_separation(&three_utc_days_earlier),
        TemporalComparisonOutcome::CalendarDateOnlyCannotResolveSubDayGap {
            minimum_whole_day_separation: 1
        }
    );

    // Two CalendarDateOnly records compare by whole calendar days only.
    let later_date = ReportedClusteringTime::calendar_date_only(
        "18 June 2024 (synthetic date-only record)",
        "Synthetic Local Time (date only)",
        2024,
        6,
        18,
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
/// - `AccountOrigin::Simulated` items are excluded from observed counts, and a reported
///   item quoting only a simulated item (including across a DAG diamond) surfaces
///   `LineageAnchoredOnlyToSimulatedEvidence`;
/// - Missing pairwise distances surface `SpatialSeparationNotEvaluated` (`B9`);
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
        1_793_566_800_000,
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
        BTreeMap::from([(String::from("formation"), String::from("triangle"))]),
        limits,
    )?;
    ledger.register_item_with_initial_cluster(
        article_1,
        OpaqueId::new("cluster:c2")?,
        VersionId::new("ver:c2-v1")?,
        criteria.clone(),
        "Initial newspaper cluster",
    )?;

    // 3. Quoting wire article (`Contradicted`, quotes `item:article-1` via `LineageRelationKind::Quotes`).
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
    // B9: Because no pairwise distances were registered in this test, `SpatialSeparationNotEvaluated`
    // must be explicitly surfaced rather than silently looking like spatial agreement.
    assert!(
        merged_all
            .conflicts()
            .iter()
            .any(|c| matches!(c, ClusterConflict::SpatialSeparationNotEvaluated { .. }))
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

/// Verifies `B1`–`B5`, `N1`, and non-blocking items:
/// - `B1`: Reverting a merge when a retired `cluster_id` was reused fails with
///   `RevertDestinationClusterInUse`.
/// - `B2`: `revise_item` never mutates `archived_cluster_versions` in place; reassessment reasons
///   live in `reassessment_by_cluster_version` and carry through merge, split, and revert.
/// - `B3`: Revising an upstream lineage item flags all active clusters whose members reach that
///   item transitively through lineage.
/// - `B4` & `B5`: Ledger boundary re-validates item/criteria limits against `self.limits` and
///   enforces `max_revisions_per_item` and `max_reassessment_reasons_per_cluster`.
/// - `N1`: Merging two clusters whose combined reassessment reasons exceed
///   `max_reassessment_reasons_per_cluster` fails with `BudgetExceeded` instead of truncating
///   silently, and `reassess_cluster` acknowledges/clears pending reasons under a new
///   `VersionId` so subsequent revisions are never permanently locked out.
#[test]
fn revert_collision_transitive_lineage_reassessment_and_ledger_budgets()
-> Result<(), Box<dyn std::error::Error>> {
    // `max_revisions_per_item = 2`, `max_reassessment_reasons_per_cluster = 3`
    let strict_limits = ClusterLimits::with_reassessment_limits(64, 8, 8, 8, 2, 2, 3)?;
    let permissive_limits = ClusterLimits::default();
    let mut ledger = EventClusterLedger::new(strict_limits);

    let t = ReportedClusteringTime::exact_instant(
        "2026-11-01T22:00:00Z",
        "UTC",
        1_793_570_400_000,
        None,
        strict_limits.max_text_bytes(),
    )?;
    let criteria = ClusterMatchCriteria::new(5_000.0, 300_000, 0, BTreeSet::new())?;

    // B4: Item constructed with `permissive_limits` (3 properties) must be rejected by
    // `ledger` whose `max_properties_per_item` is 2.
    let over_budget_item = GeoEvidenceItem::new(
        OpaqueId::new("item:over-budget")?,
        VersionId::new("ver:over-budget-v1")?,
        OpaqueId::new("source:synth-x")?,
        None,
        GeoEvidenceKind::PublishedArticle,
        ClaimAssessmentState::Supported,
        Vec::new(),
        t.clone(),
        LocationQualification::surveyed(OpaqueId::new("loc:1")?, None)?,
        BTreeMap::from([
            (String::from("k1"), String::from("v1")),
            (String::from("k2"), String::from("v2")),
            (String::from("k3"), String::from("v3")),
        ]),
        permissive_limits,
    )?;
    assert!(matches!(
        ledger.register_item_with_initial_cluster(
            over_budget_item,
            OpaqueId::new("cluster:over")?,
            VersionId::new("ver:over-c1")?,
            criteria.clone(),
            "Should reject over-budget item",
        ),
        Err(GeoClusteringError::BudgetExceeded {
            limit: ClusterLimitKind::PropertiesPerItem,
            ..
        })
    ));

    // Register X (in `c:x`), Y republishing X (in `c:y`), and W (in `c:w`).
    let item_x_v1 = GeoEvidenceItem::new(
        OpaqueId::new("item:x")?,
        VersionId::new("ver:x-v1")?,
        OpaqueId::new("source:src-x")?,
        None,
        GeoEvidenceKind::PublishedArticle,
        ClaimAssessmentState::Supported,
        Vec::new(),
        t.clone(),
        LocationQualification::surveyed(OpaqueId::new("loc:x")?, None)?,
        BTreeMap::new(),
        strict_limits,
    )?;
    ledger.register_item_with_initial_cluster(
        item_x_v1,
        OpaqueId::new("cluster:x")?,
        VersionId::new("ver:cx-v1")?,
        criteria.clone(),
        "Cluster X",
    )?;

    let item_y_v1 = GeoEvidenceItem::new(
        OpaqueId::new("item:y")?,
        VersionId::new("ver:y-v1")?,
        OpaqueId::new("source:src-y")?,
        None,
        GeoEvidenceKind::PublishedArticle,
        ClaimAssessmentState::Supported,
        vec![EvidenceLineageLink::new(
            LineageRelationKind::Republishes,
            OpaqueId::new("item:x")?,
        )],
        t.clone(),
        LocationQualification::surveyed(OpaqueId::new("loc:y")?, None)?,
        BTreeMap::new(),
        strict_limits,
    )?;
    ledger.register_item_with_initial_cluster(
        item_y_v1,
        OpaqueId::new("cluster:y")?,
        VersionId::new("ver:cy-v1")?,
        criteria.clone(),
        "Cluster Y republishing X",
    )?;

    let item_w_v1 = GeoEvidenceItem::new(
        OpaqueId::new("item:w")?,
        VersionId::new("ver:w-v1")?,
        OpaqueId::new("source:src-w")?,
        None,
        GeoEvidenceKind::PublishedArticle,
        ClaimAssessmentState::Supported,
        Vec::new(),
        t.clone(),
        LocationQualification::surveyed(OpaqueId::new("loc:w")?, None)?,
        BTreeMap::new(),
        strict_limits,
    )?;
    ledger.register_item_with_initial_cluster(
        item_w_v1,
        OpaqueId::new("cluster:w")?,
        VersionId::new("ver:cw-v1")?,
        criteria.clone(),
        "Cluster W",
    )?;

    let actor = ActorId::new("actor:reviewer")?;
    ledger.merge_clusters(
        &OpaqueId::new("cluster:y")?,
        &OpaqueId::new("cluster:w")?,
        OpaqueId::new("cluster:yw")?,
        VersionId::new("ver:cyw-v1")?,
        criteria.clone(),
        OpaqueId::new("decision:merge-yw")?,
        actor.clone(),
        "Merge Y and W",
    )?;

    // B3 & B2: Revise X's `source_id` to `source:src-w`.
    // Both `cluster:x` (direct member) and `cluster:yw` (reaches X via Y -> X lineage)
    // must be flagged for reassessment, while `archived_cluster_versions()[ver:cx-v1]`
    // remains completely unchanged (`needs_reassessment_at_issue() == false`).
    let item_x_v2 = GeoEvidenceItem::new(
        OpaqueId::new("item:x")?,
        VersionId::new("ver:x-v2")?,
        OpaqueId::new("source:src-w")?,
        None,
        GeoEvidenceKind::PublishedArticle,
        ClaimAssessmentState::Supported,
        Vec::new(),
        t.clone(),
        LocationQualification::surveyed(OpaqueId::new("loc:x")?, None)?,
        BTreeMap::new(),
        strict_limits,
    )?;
    let flagged = ledger.revise_item(
        &VersionId::new("ver:x-v1")?,
        item_x_v2,
        "Corrected upstream publisher attribution",
    )?;
    assert_eq!(
        flagged,
        vec![OpaqueId::new("cluster:x")?, OpaqueId::new("cluster:yw")?]
    );
    assert!(ledger.cluster_needs_reassessment(&OpaqueId::new("cluster:x")?));
    assert!(ledger.cluster_needs_reassessment(&OpaqueId::new("cluster:yw")?));
    assert!(
        !ledger.archived_cluster_versions()[&VersionId::new("ver:cx-v1")?]
            .needs_reassessment_at_issue()
    );

    // B5: Because `max_revisions_per_item` is 2 (`ver:x-v1` + `ver:x-v2`), a 3rd version
    // must be rejected with `BudgetExceeded { limit: ClusterLimitKind::RevisionsPerItem, .. }`.
    let item_x_v3 = GeoEvidenceItem::new(
        OpaqueId::new("item:x")?,
        VersionId::new("ver:x-v3")?,
        OpaqueId::new("source:src-w")?,
        None,
        GeoEvidenceKind::PublishedArticle,
        ClaimAssessmentState::Disputed,
        Vec::new(),
        t.clone(),
        LocationQualification::surveyed(OpaqueId::new("loc:x")?, None)?,
        BTreeMap::new(),
        strict_limits,
    )?;
    assert!(matches!(
        ledger.revise_item(
            &VersionId::new("ver:x-v2")?,
            item_x_v3,
            "Exceeds revision cap"
        ),
        Err(GeoClusteringError::BudgetExceeded {
            limit: ClusterLimitKind::RevisionsPerItem,
            ..
        })
    ));

    // N1: Reassess `cluster:yw` to acknowledge the upstream change and mint `ver:cyw-v2`
    // with `cluster_needs_reassessment == false` and updated `independent_source_count == 1`.
    let reassessed = ledger
        .reassess_cluster(
            &OpaqueId::new("cluster:yw")?,
            VersionId::new("ver:cyw-v2")?,
            criteria.clone(),
            OpaqueId::new("decision:reassess-yw")?,
            actor.clone(),
            "Reassessed Y+W after X publisher correction",
        )?
        .clone();
    assert!(!ledger.cluster_needs_reassessment(&OpaqueId::new("cluster:yw")?));
    assert_eq!(reassessed.counts().independent_source_count, 1);

    // Revert the reassessment to restore `ver:cyw-v1` (under `ver:cyw-restored`),
    // then test B1 collision detection on `cluster:y`.
    ledger.revert_decision(
        &OpaqueId::new("decision:reassess-yw")?,
        OpaqueId::new("decision:revert-reassess-yw")?,
        &[VersionId::new("ver:cyw-restored")?],
        actor.clone(),
        "Restore pre-reassessment state to test merge revert",
    )?;
    assert!(ledger.cluster_needs_reassessment(&OpaqueId::new("cluster:yw")?));

    // B1: Register a new item `item:new` reusing the retired `cluster:y` ID.
    // Attempting to revert `decision:merge-yw` must fail with `RevertDestinationClusterInUse`.
    let item_new = GeoEvidenceItem::new(
        OpaqueId::new("item:new")?,
        VersionId::new("ver:new-v1")?,
        OpaqueId::new("source:src-new")?,
        None,
        GeoEvidenceKind::PublishedArticle,
        ClaimAssessmentState::Supported,
        Vec::new(),
        t,
        LocationQualification::surveyed(OpaqueId::new("loc:new")?, None)?,
        BTreeMap::new(),
        strict_limits,
    )?;
    ledger.register_item_with_initial_cluster(
        item_new,
        OpaqueId::new("cluster:y")?,
        VersionId::new("ver:cy-reused-v1")?,
        criteria,
        "New item registered under retired cluster:y ID",
    )?;

    // Note: `decision:merge-yw` produced `ver:cyw-v1`, which was superseded by `ver:cyw-restored`.
    // Even if we merge two fresh clusters and reuse one of their IDs, `RevertDestinationClusterInUse`
    // protects the active cluster.
    ledger.merge_clusters(
        &OpaqueId::new("cluster:x")?,
        &OpaqueId::new("cluster:y")?,
        OpaqueId::new("cluster:xy")?,
        VersionId::new("ver:cxy-v1")?,
        ClusterMatchCriteria::new(5_000.0, 300_000, 0, BTreeSet::new())?,
        OpaqueId::new("decision:merge-xy")?,
        actor.clone(),
        "Merge X and reused Y",
    )?;
    let item_collide = GeoEvidenceItem::new(
        OpaqueId::new("item:collide")?,
        VersionId::new("ver:collide-v1")?,
        OpaqueId::new("source:src-collide")?,
        None,
        GeoEvidenceKind::PublishedArticle,
        ClaimAssessmentState::Supported,
        Vec::new(),
        ReportedClusteringTime::exact_instant(
            "2026-11-01T22:00:00Z",
            "UTC",
            1_793_570_400_000,
            None,
            strict_limits.max_text_bytes(),
        )?,
        LocationQualification::surveyed(OpaqueId::new("loc:collide")?, None)?,
        BTreeMap::new(),
        strict_limits,
    )?;
    ledger.register_item_with_initial_cluster(
        item_collide,
        OpaqueId::new("cluster:x")?,
        VersionId::new("ver:cx-reused-v1")?,
        ClusterMatchCriteria::new(5_000.0, 300_000, 0, BTreeSet::new())?,
        "Reuse cluster:x ID",
    )?;

    assert!(matches!(
        ledger.revert_decision(
            &OpaqueId::new("decision:merge-xy")?,
            OpaqueId::new("decision:revert-xy")?,
            &[
                VersionId::new("ver:cx-restored")?,
                VersionId::new("ver:cy-restored-2")?,
            ],
            actor,
            "Attempt revert while cluster:x is in use",
        ),
        Err(GeoClusteringError::RevertDestinationClusterInUse { .. })
    ));
    assert!(
        ledger.clusters()[&OpaqueId::new("cluster:x")?]
            .member_item_ids()
            .contains(&OpaqueId::new("item:collide")?)
    );
    Ok(())
}

/// Verifies `N1` (no silent truncation when combining reassessment reasons across merges)
/// and non-blocking item 4 (DAG diamond into a simulated item reports
/// `LineageAnchoredOnlyToSimulatedEvidence`, not `UnanchoredLineageCycle`).
#[test]
fn reason_cap_never_truncates_silently_and_simulated_diamond_is_not_cycle()
-> Result<(), Box<dyn std::error::Error>> {
    // Cap reassessment reasons per cluster at 3, revisions per item at 4.
    let limits = ClusterLimits::with_reassessment_limits(128, 16, 16, 16, 4, 4, 3)?;
    let mut ledger = EventClusterLedger::new(limits);
    let t = ReportedClusteringTime::exact_instant(
        "2026-11-01T22:00:00Z",
        "UTC",
        1_793_570_400_000,
        None,
        limits.max_text_bytes(),
    )?;
    let criteria = ClusterMatchCriteria::new(5_000.0, 300_000, 0, BTreeSet::new())?;

    // Build two clusters `c:1` and `c:2`, revise each member twice (2 reasons each).
    for (idx, cid, item_id) in [(1, "cluster:1", "item:1"), (2, "cluster:2", "item:2")] {
        let item_v1 = GeoEvidenceItem::new(
            OpaqueId::new(item_id)?,
            VersionId::new(format!("ver:{idx}-v1"))?,
            OpaqueId::new(format!("source:s{idx}"))?,
            None,
            GeoEvidenceKind::PublishedArticle,
            ClaimAssessmentState::Supported,
            Vec::new(),
            t.clone(),
            LocationQualification::surveyed(OpaqueId::new(format!("loc:{idx}"))?, None)?,
            BTreeMap::new(),
            limits,
        )?;
        ledger.register_item_with_initial_cluster(
            item_v1,
            OpaqueId::new(cid)?,
            VersionId::new(format!("ver:c{idx}-v1"))?,
            criteria.clone(),
            "Init cluster",
        )?;
        for rev in 2..=3 {
            let item_vn = GeoEvidenceItem::new(
                OpaqueId::new(item_id)?,
                VersionId::new(format!("ver:{idx}-v{rev}"))?,
                OpaqueId::new(format!("source:s{idx}"))?,
                None,
                GeoEvidenceKind::PublishedArticle,
                ClaimAssessmentState::Supported,
                Vec::new(),
                t.clone(),
                LocationQualification::surveyed(OpaqueId::new(format!("loc:{idx}"))?, None)?,
                BTreeMap::new(),
                limits,
            )?;
            ledger.revise_item(
                &VersionId::new(format!("ver:{idx}-v{}", rev - 1))?,
                item_vn,
                format!("Revision {rev} for item {idx}"),
            )?;
        }
    }

    // Merging `cluster:1` (2 reasons) and `cluster:2` (2 reasons) requires 4 reasons,
    // which exceeds `max_reassessment_reasons_per_cluster = 3` and must return `BudgetExceeded`
    // rather than silently dropping the 4th reason.
    let actor = ActorId::new("actor:reviewer")?;
    assert!(matches!(
        ledger.merge_clusters(
            &OpaqueId::new("cluster:1")?,
            &OpaqueId::new("cluster:2")?,
            OpaqueId::new("cluster:12")?,
            VersionId::new("ver:c12-v1")?,
            criteria.clone(),
            OpaqueId::new("decision:merge-12-fail")?,
            actor.clone(),
            "Should refuse silent reason truncation",
        ),
        Err(GeoClusteringError::BudgetExceeded {
            limit: ClusterLimitKind::ReassessmentReasonsPerCluster,
            actual: 4,
            max: 3,
        })
    ));

    // After reassessing `cluster:1` (acknowledging its 2 reasons), the merge succeeds cleanly.
    ledger.reassess_cluster(
        &OpaqueId::new("cluster:1")?,
        VersionId::new("ver:c1-v2")?,
        criteria.clone(),
        OpaqueId::new("decision:reassess-c1")?,
        actor.clone(),
        "Acknowledge cluster:1 revisions",
    )?;
    ledger.merge_clusters(
        &OpaqueId::new("cluster:1")?,
        &OpaqueId::new("cluster:2")?,
        OpaqueId::new("cluster:12")?,
        VersionId::new("ver:c12-v1")?,
        criteria.clone(),
        OpaqueId::new("decision:merge-12-ok")?,
        actor,
        "Merge after acknowledging cluster:1 reasons",
    )?;
    assert_eq!(
        ledger
            .cluster_reassessment_reasons(&OpaqueId::new("cluster:12")?)
            .len(),
        2
    );

    // Non-blocking item 4: Diamond D -> {B, C} -> simulated S must emit
    // `LineageAnchoredOnlyToSimulatedEvidence`, not `UnanchoredLineageCycle`.
    let sim_s = GeoEvidenceItem::new(
        OpaqueId::new("item:sim-s")?,
        VersionId::new("ver:sim-s-v1")?,
        OpaqueId::new("source:sim")?,
        None,
        GeoEvidenceKind::WitnessAccount {
            origin: AccountOrigin::Simulated,
        },
        ClaimAssessmentState::Supported,
        Vec::new(),
        t.clone(),
        LocationQualification::surveyed(OpaqueId::new("loc:s")?, None)?,
        BTreeMap::new(),
        limits,
    )?;
    ledger.register_item_with_initial_cluster(
        sim_s,
        OpaqueId::new("cluster:s")?,
        VersionId::new("ver:cs-v1")?,
        criteria.clone(),
        "Simulated root S",
    )?;
    for mid in ["item:b", "item:c"] {
        let m_item = GeoEvidenceItem::new(
            OpaqueId::new(mid)?,
            VersionId::new(format!("ver:{mid}-v1"))?,
            OpaqueId::new(format!("source:{mid}"))?,
            None,
            GeoEvidenceKind::PublishedArticle,
            ClaimAssessmentState::Supported,
            vec![EvidenceLineageLink::new(
                LineageRelationKind::Quotes,
                OpaqueId::new("item:sim-s")?,
            )],
            t.clone(),
            LocationQualification::surveyed(OpaqueId::new("loc:s")?, None)?,
            BTreeMap::new(),
            limits,
        )?;
        ledger.register_item_with_initial_cluster(
            m_item,
            OpaqueId::new(format!("cluster:{mid}"))?,
            VersionId::new(format!("ver:c-{mid}-v1"))?,
            criteria.clone(),
            "Intermediate quoting S",
        )?;
    }
    let item_d = GeoEvidenceItem::new(
        OpaqueId::new("item:d")?,
        VersionId::new("ver:d-v1")?,
        OpaqueId::new("source:d")?,
        None,
        GeoEvidenceKind::PublishedArticle,
        ClaimAssessmentState::Supported,
        vec![
            EvidenceLineageLink::new(LineageRelationKind::Quotes, OpaqueId::new("item:b")?),
            EvidenceLineageLink::new(LineageRelationKind::Quotes, OpaqueId::new("item:c")?),
        ],
        t,
        LocationQualification::surveyed(OpaqueId::new("loc:s")?, None)?,
        BTreeMap::new(),
        limits,
    )?;
    ledger.register_item_with_initial_cluster(
        item_d,
        OpaqueId::new("cluster:d")?,
        VersionId::new("ver:cd-v1")?,
        criteria,
        "Diamond tip D quoting B and C",
    )?;
    let d_conflicts = ledger.clusters()[&OpaqueId::new("cluster:d")?].conflicts();
    assert!(d_conflicts.iter().any(|c| matches!(
        c,
        ClusterConflict::LineageAnchoredOnlyToSimulatedEvidence { item_id }
            if item_id.as_str() == "item:d"
    )));
    assert!(
        !d_conflicts
            .iter()
            .any(|c| matches!(c, ClusterConflict::UnanchoredLineageCycle { .. }))
    );
    Ok(())
}
