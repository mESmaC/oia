//! Focused contract tests for historical analogue retrieval, point-in-time precedent
//! knowability, episode cluster independence, conditional response branches, precedent
//! supersession/reassessment, and `A15` forecast lifecycle integration (`FORECAST-01`..=`FORECAST-04`, `#45`).

use std::error::Error;

use oia_domain::{ActorId, OpaqueId, SchemaVersion, VersionId};
use oia_forecast::analogues::{
    ActorResponseStep, AnalogueError, AnalogueForecastIssuanceSpec, AnalogueProfile,
    AnalogueQueryCriteria, AnalogueTimingEvaluation, ComparisonEditAction, ComparisonSetEdit,
    ConditionalResponseBranch, ConditionalResponseStep, EconomicCondition, ElapsedTimeRangeMs,
    EpisodeDependencyKind, EpisodePointInTimeStatus, EventScale, IndicatorDirection,
    InformationQuality, InstitutionalRegime, MonitoringIndicator, PrecedentEpisodeRecord,
    PrecedentLibrary, PrecedentOutcomeDisposition, PrecedentRole, PrecedentSector,
    PredictionJustification, TemporalSequenceRelation, TensionLevel, TransferLimitationReason,
    TransferabilityAssessment, UNCALIBRATED_HISTORICAL_FREQUENCY_CAVEAT,
};
use oia_forecast::issuance::{
    CutoffAdmissibilityMode, CutoffSourceInput, ForecastLedger, OutcomeDefinition,
    OutcomeResolutionRecord, ReassessmentReason, ResolutionState, RevisionRationale,
};
use oia_forecast::scoring::Probability;

const DAY_MS: i64 = 86_400_000;
const T0_MS: i64 = 1_700_000_000_000;

fn sample_profile(regime: InstitutionalRegime) -> AnalogueProfile {
    AnalogueProfile {
        event_scale: EventScale::Regional,
        geography_region: "North Atlantic".to_owned(),
        actor_sector: PrecedentSector::Government,
        actor_profile: "Civil Aviation Authority".to_owned(),
        institutional_regime: regime,
        tension_level: TensionLevel::Elevated,
        economic_condition: EconomicCondition::InflationaryOrSupplyStress,
        information_quality: InformationQuality::HighVerifiedPrimary,
    }
}

fn source_at(
    ver: &str,
    event_ms: i64,
    pub_ms: i64,
    ing_ms: i64,
) -> Result<CutoffSourceInput, Box<dyn Error>> {
    Ok(CutoffSourceInput {
        source_version: VersionId::new(ver)?,
        event_utc_ms: Some(event_ms),
        public_available_at_utc_ms: Some(pub_ms),
        ingested_at_utc_ms: Some(ing_ms),
    })
}

/// Risk / Requirements: `FORECAST-01`..=`FORECAST-04`, `A15`.
///
/// Establishes that:
/// 1. Precedent outcomes occurring after `information_cutoff_utc_ms` are right-censored at cutoff
///    (`CensoredAtCutoff`) and excluded from resolved support counts and forecast draft sources,
///    preventing lookahead leakage.
/// 2. Dependent episodes sharing a crisis cluster or source lineage do not inflate
///    `independent_cluster_count` or `independent_cluster_support_ratio_bps`.
/// 3. Documented non-response, divergent counterexamples, and censored follow-up remain distinct,
///    and excluding a counterexample preserves an audit record and flags transferability.
/// 4. Issuing an initial forecast (`v1`), revising it (`v2`) via `re_retrieve_at_cutoff` on new
///    indicator evidence, and resolving the outcome preserves both vintages, pins the comparison-set
///    `VersionId`, and reports exact precedent timing error.
#[test]
fn a15_precedent_knowability_independence_and_lifecycle_contract() -> Result<(), Box<dyn Error>> {
    let mut lib = PrecedentLibrary::new(32)?;
    let base_profile = sample_profile(InstitutionalRegime::RegulatoryAgency);

    // Episode 1: Independent supportive precedent (elapsed = 4 days).
    let t1 = T0_MS + 5 * DAY_MS;
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-ind-support-1")?,
        version_id: VersionId::new("ep-ind-support-1:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:a")?,
        recorded_at_utc_ms: t1 + 5 * DAY_MS,
        title: "Independent supportive episode 1".to_owned(),
        initiating_condition: "Multi-sector primary radar return".to_owned(),
        initiating_event_utc_ms: t1,
        initiation_evidence_versions: vec![VersionId::new("src:ep1-init:v1")?],
        profile: base_profile.clone(),
        dependency: EpisodeDependencyKind::IndependentEpisode,
        response_steps: vec![ActorResponseStep {
            step_id: OpaqueId::new("ep1-step1")?,
            actor_id: ActorId::new("actor:caa")?,
            action_summary: "Opened technical review docket".to_owned(),
            announced_intention: Some("Publish interim notice within 96h".to_owned()),
            action_utc_ms: t1 + DAY_MS,
            elapsed_from_initiation_ms: DAY_MS,
            sequence_relation: TemporalSequenceRelation::ObservedSequenceOnly,
            evidence_versions: vec![VersionId::new("src:ep1-init:v1")?],
        }],
        outcome: PrecedentOutcomeDisposition::TargetResponseOccurred {
            outcome_summary: "Issued NOTAM sensor directive".to_owned(),
            resolved_at_utc_ms: t1 + 4 * DAY_MS,
            elapsed_from_initiation_ms: 4 * DAY_MS,
            outcome_evidence_versions: vec![VersionId::new("src:ep1-out:v1")?],
        },
        sources: vec![
            source_at("src:ep1-init:v1", t1, t1 + 1_000, t1 + 2_000)?,
            source_at(
                "src:ep1-out:v1",
                t1 + 4 * DAY_MS,
                t1 + 4 * DAY_MS + 1_000,
                t1 + 4 * DAY_MS + 2_000,
            )?,
        ],
        supersedes_version: None,
        correction_rationale: None,
    })?;

    // Episodes 2a and 2b: Two supportive episodes in the SAME shared crisis cluster (elapsed = 6d and 8d).
    // Raw supportive count increases by 2, but independent supportive cluster count must increase by 1.
    let t2 = T0_MS + 15 * DAY_MS;
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-cluster-primary")?,
        version_id: VersionId::new("ep-cluster-primary:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:a")?,
        recorded_at_utc_ms: t2 + 7 * DAY_MS,
        title: "Cluster primary supportive episode".to_owned(),
        initiating_condition: "Coordinated coastal corridor anomaly".to_owned(),
        initiating_event_utc_ms: t2,
        initiation_evidence_versions: vec![VersionId::new("src:ep2-init:v1")?],
        profile: base_profile.clone(),
        dependency: EpisodeDependencyKind::SharedCrisisOrCampaign {
            cluster_id: OpaqueId::new("cluster-synth-wave")?,
            dependency_note: "Primary sector in synthetic wave".to_owned(),
        },
        response_steps: Vec::new(),
        outcome: PrecedentOutcomeDisposition::TargetResponseOccurred {
            outcome_summary: "Issued NOTAM directive in primary sector".to_owned(),
            resolved_at_utc_ms: t2 + 6 * DAY_MS,
            elapsed_from_initiation_ms: 6 * DAY_MS,
            outcome_evidence_versions: vec![VersionId::new("src:ep2-out:v1")?],
        },
        sources: vec![
            source_at("src:ep2-init:v1", t2, t2 + 1_000, t2 + 2_000)?,
            source_at(
                "src:ep2-out:v1",
                t2 + 6 * DAY_MS,
                t2 + 6 * DAY_MS + 1_000,
                t2 + 6 * DAY_MS + 2_000,
            )?,
        ],
        supersedes_version: None,
        correction_rationale: None,
    })?;

    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-cluster-spillover")?,
        version_id: VersionId::new("ep-cluster-spillover:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:a")?,
        recorded_at_utc_ms: t2 + 10 * DAY_MS,
        title: "Cluster spillover supportive episode".to_owned(),
        initiating_condition: "Adjacent sector spillover".to_owned(),
        initiating_event_utc_ms: t2 + DAY_MS,
        initiation_evidence_versions: vec![VersionId::new("src:ep2-init:v1")?],
        profile: base_profile.clone(),
        dependency: EpisodeDependencyKind::PolicySpilloverOrRetaliation {
            cluster_id: OpaqueId::new("cluster-synth-wave")?,
            predecessor_episode_id: OpaqueId::new("ep-cluster-primary")?,
            dependency_note: "Followed primary sector directive".to_owned(),
        },
        response_steps: Vec::new(),
        outcome: PrecedentOutcomeDisposition::TargetResponseOccurred {
            outcome_summary: "Mirrored NOTAM directive".to_owned(),
            resolved_at_utc_ms: t2 + 9 * DAY_MS,
            elapsed_from_initiation_ms: 8 * DAY_MS,
            outcome_evidence_versions: vec![VersionId::new("src:ep2-spill-out:v1")?],
        },
        sources: vec![
            source_at("src:ep2-init:v1", t2, t2 + 1_000, t2 + 2_000)?,
            source_at(
                "src:ep2-spill-out:v1",
                t2 + 9 * DAY_MS,
                t2 + 9 * DAY_MS + 1_000,
                t2 + 9 * DAY_MS + 2_000,
            )?,
        ],
        supersedes_version: None,
        correction_rationale: None,
    })?;

    // Episode 3: Documented non-response counterexample.
    let t3 = T0_MS + 35 * DAY_MS;
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-doc-non-response")?,
        version_id: VersionId::new("ep-doc-non-response:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:a")?,
        recorded_at_utc_ms: t3 + 15 * DAY_MS,
        title: "Documented non-response episode".to_owned(),
        initiating_condition: "Unconfirmed single-sensor track".to_owned(),
        initiating_event_utc_ms: t3,
        initiation_evidence_versions: vec![VersionId::new("src:ep3-init:v1")?],
        profile: base_profile.clone(),
        dependency: EpisodeDependencyKind::IndependentEpisode,
        response_steps: Vec::new(),
        outcome: PrecedentOutcomeDisposition::DocumentedNonResponse {
            observed_window_end_utc_ms: t3 + 14 * DAY_MS,
            observation_duration_ms: 14 * DAY_MS,
            non_response_rationale: "14-day docket audit confirmed zero directives issued"
                .to_owned(),
            outcome_evidence_versions: vec![VersionId::new("src:ep3-audit:v1")?],
        },
        sources: vec![
            source_at("src:ep3-init:v1", t3, t3 + 1_000, t3 + 2_000)?,
            source_at(
                "src:ep3-audit:v1",
                t3 + 14 * DAY_MS,
                t3 + 14 * DAY_MS + 1_000,
                t3 + 14 * DAY_MS + 2_000,
            )?,
        ],
        supersedes_version: None,
        correction_rationale: None,
    })?;

    // Episode 4: Divergent response counterexample (under a different institutional regime, 6/7 match).
    let t4 = T0_MS + 60 * DAY_MS;
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-divergent")?,
        version_id: VersionId::new("ep-divergent:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:a")?,
        recorded_at_utc_ms: t4 + 4 * DAY_MS,
        title: "Divergent response episode".to_owned(),
        initiating_condition: "Weather-balloon track referral".to_owned(),
        initiating_event_utc_ms: t4,
        initiation_evidence_versions: vec![VersionId::new("src:ep4-init:v1")?],
        profile: sample_profile(InstitutionalRegime::DemocraticStatutory),
        dependency: EpisodeDependencyKind::IndependentEpisode,
        response_steps: Vec::new(),
        outcome: PrecedentOutcomeDisposition::DivergentResponseOccurred {
            divergent_action_summary: "Referred to meteorological bureau without airspace NOTAM"
                .to_owned(),
            resolved_at_utc_ms: t4 + 3 * DAY_MS,
            elapsed_from_initiation_ms: 3 * DAY_MS,
            outcome_evidence_versions: vec![VersionId::new("src:ep4-out:v1")?],
        },
        sources: vec![
            source_at("src:ep4-init:v1", t4, t4 + 1_000, t4 + 2_000)?,
            source_at(
                "src:ep4-out:v1",
                t4 + 3 * DAY_MS,
                t4 + 3 * DAY_MS + 1_000,
                t4 + 3 * DAY_MS + 2_000,
            )?,
        ],
        supersedes_version: None,
        correction_rationale: None,
    })?;

    // Episode 5: Initiated before cutoff (day 95), resolved at day 98, and record curated at day 99
    // (before day-100 cutoff), BUT its outcome source `src:ep5-late-out:v1` only became publicly
    // available and ingested at day 104 (after day-100 cutoff).
    // At day 100 (`cutoff_v1_ms`), this episode must be classified as `InitiatedButOutcomeCensoredAtCutoff`!
    let t5 = T0_MS + 95 * DAY_MS;
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-post-cutoff-outcome")?,
        version_id: VersionId::new("ep-post-cutoff-outcome:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:a")?,
        recorded_at_utc_ms: t5 + 4 * DAY_MS,
        title: "Episode whose outcome source published after forecast cutoff".to_owned(),
        initiating_condition: "Late-autumn corridor anomaly".to_owned(),
        initiating_event_utc_ms: t5,
        initiation_evidence_versions: vec![VersionId::new("src:ep5-init:v1")?],
        profile: base_profile.clone(),
        dependency: EpisodeDependencyKind::IndependentEpisode,
        response_steps: Vec::new(),
        outcome: PrecedentOutcomeDisposition::TargetResponseOccurred {
            outcome_summary: "Resolved on day 98, published on day 104 after day 100 cutoff"
                .to_owned(),
            resolved_at_utc_ms: t5 + 3 * DAY_MS,
            elapsed_from_initiation_ms: 3 * DAY_MS,
            outcome_evidence_versions: vec![VersionId::new("src:ep5-late-out:v1")?],
        },
        sources: vec![
            source_at("src:ep5-init:v1", t5, t5 + 1_000, t5 + 2_000)?,
            source_at(
                "src:ep5-late-out:v1",
                t5 + 3 * DAY_MS,
                T0_MS + 104 * DAY_MS + 1_000,
                T0_MS + 104 * DAY_MS + 2_000,
            )?,
        ],
        supersedes_version: None,
        correction_rationale: None,
    })?;

    // Episode 6: Initiated after day 100 cutoff (day 110) -> must be rejected as InadmissibleAtCutoff.
    let t6 = T0_MS + 110 * DAY_MS;
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-future-inadmissible")?,
        version_id: VersionId::new("ep-future-inadmissible:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:a")?,
        recorded_at_utc_ms: t6 + 3 * DAY_MS,
        title: "Future episode after cutoff".to_owned(),
        initiating_condition: "Future event".to_owned(),
        initiating_event_utc_ms: t6,
        initiation_evidence_versions: vec![VersionId::new("src:ep6-init:v1")?],
        profile: base_profile.clone(),
        dependency: EpisodeDependencyKind::IndependentEpisode,
        response_steps: Vec::new(),
        outcome: PrecedentOutcomeDisposition::TargetResponseOccurred {
            outcome_summary: "Future outcome".to_owned(),
            resolved_at_utc_ms: t6 + 2 * DAY_MS,
            elapsed_from_initiation_ms: 2 * DAY_MS,
            outcome_evidence_versions: vec![VersionId::new("src:ep6-init:v1")?],
        },
        sources: vec![source_at("src:ep6-init:v1", t6, t6 + 1_000, t6 + 2_000)?],
        supersedes_version: None,
        correction_rationale: None,
    })?;

    let cutoff_v1_ms = T0_MS + 100 * DAY_MS;
    let comp_set = lib.retrieve_comparison_set(AnalogueQueryCriteria {
        comparison_set_id: OpaqueId::new("cs-a15")?,
        initial_version_id: VersionId::new("cs-a15:v1")?,
        actor: ActorId::new("analyst:1")?,
        created_at_utc_ms: cutoff_v1_ms,
        information_cutoff_utc_ms: cutoff_v1_ms,
        cutoff_mode: CutoffAdmissibilityMode::ProspectiveStrict,
        target_profile: base_profile,
        min_matched_dimensions: 6,
        min_independent_clusters: 4,
    })?;

    // Episode 6 was rejected at cutoff; Episodes 1, 2a, 2b, 3, 4, 5 are active (6 episodes).
    assert_eq!(comp_set.inadmissible_episode_ids().len(), 1);
    assert_eq!(
        comp_set.inadmissible_episode_ids()[0].0,
        OpaqueId::new("ep-future-inadmissible")?
    );
    assert_eq!(comp_set.active_entries().len(), 6);

    // Verify Episode 5 was right-censored at cutoff_v1_ms and did NOT include `src:ep5-late-out:v1`.
    let ep5_entry = comp_set
        .active_entries()
        .iter()
        .find(|e| e.episode_id.as_str() == "ep-post-cutoff-outcome")
        .ok_or("missing ep-post-cutoff-outcome")?;
    assert_eq!(ep5_entry.role, PrecedentRole::CensoredAtCutoff);
    assert!(matches!(
        ep5_entry.point_in_time_status,
        EpisodePointInTimeStatus::InitiatedButOutcomeCensoredAtCutoff { .. }
    ));
    assert_eq!(ep5_entry.cutoff_admissible_sources.len(), 1);
    assert_eq!(
        ep5_entry.cutoff_admissible_sources[0].source_version,
        VersionId::new("src:ep5-init:v1")?
    );

    // Check frequency summary before edits:
    // - 3 supportive episodes across 2 independent clusters (`ep-ind-support-1` and `cluster-synth-wave`)
    // - 1 divergent counterexample + 1 documented non-response counterexample (2 clusters)
    // - 1 censored at cutoff (`ep-post-cutoff-outcome`)
    // Raw resolved ratio: 3 / 5 = 6000 bps.
    // Independent cluster resolved ratio: 2 / 4 = 5000 bps.
    let freq = comp_set.summarize_frequencies();
    assert_eq!(freq.active_episode_count, 6);
    assert_eq!(freq.independent_cluster_count, 5);
    assert_eq!(freq.dependent_episode_count, 1);
    assert_eq!(freq.supportive_target_response_count, 3);
    assert_eq!(freq.divergent_counterexample_count, 1);
    assert_eq!(freq.documented_non_response_count, 1);
    assert_eq!(freq.total_counterexample_count, 2);
    assert_eq!(freq.censored_at_cutoff_count, 1);
    assert_eq!(freq.mixed_outcome_cluster_count, 0);
    assert_eq!(freq.raw_resolved_support_ratio_bps, Some(6_000));
    assert_eq!(freq.independent_cluster_support_ratio_bps, Some(5_000));
    assert!(!freq.is_calibrated_probability);
    assert_eq!(
        freq.calibration_caveat,
        UNCALIBRATED_HISTORICAL_FREQUENCY_CAVEAT
    );
    assert_eq!(
        freq.target_elapsed_time_range,
        Some(ElapsedTimeRangeMs {
            resolved_episode_count: 3,
            min_elapsed_ms: 4 * DAY_MS,
            median_elapsed_ms: 6 * DAY_MS,
            max_elapsed_ms: 8 * DAY_MS,
        })
    );
    assert_eq!(
        comp_set.assess_transferability(),
        TransferabilityAssessment::AdequatePrecedentBasis
    );

    // Issue original forecast vintage (v1), then revise (v2) via `re_retrieve_at_cutoff`, then resolve and evaluate.
    let mut ledger = ForecastLedger::new(32)?;
    let horizon_start_ms = cutoff_v1_ms + DAY_MS;
    let horizon_end_ms = cutoff_v1_ms + 14 * DAY_MS;
    let outcome_def = OutcomeDefinition {
        outcome_id: OpaqueId::new("out-a15")?,
        proposition: "Formal NOTAM directive issued within 14 days".to_owned(),
        resolution_rules: "Published in official register within horizon".to_owned(),
    };
    let branches = vec![ConditionalResponseBranch {
        branch_id: OpaqueId::new("b-main")?,
        branch_label: "Working group then NOTAM".to_owned(),
        is_non_response_or_divergent: false,
        steps: vec![
            ConditionalResponseStep {
                step_number: 1,
                actor_sector: PrecedentSector::Government,
                action_description: "Convene working group".to_owned(),
                conditional_probability: Probability::new(0.80)?,
                dependency_rationale: "Initial radar trigger".to_owned(),
                expected_step_window_ms: Some((DAY_MS, 3 * DAY_MS)),
            },
            ConditionalResponseStep {
                step_number: 2,
                actor_sector: PrecedentSector::Government,
                action_description: "Issue directive".to_owned(),
                conditional_probability: Probability::new(0.75)?,
                dependency_rationale: "Depends on working group confirmation".to_owned(),
                expected_step_window_ms: Some((3 * DAY_MS, 10 * DAY_MS)),
            },
        ],
        chained_probability: Probability::new(0.60)?,
        expected_elapsed_range_ms: freq.target_elapsed_time_range,
        supporting_episode_versions: vec![VersionId::new("ep-ind-support-1:v1")?],
    }];
    let indicators = vec![MonitoringIndicator {
        indicator_id: OpaqueId::new("ind-1")?,
        observable_signal: "Docket notice published".to_owned(),
        target_branch_id: OpaqueId::new("b-main")?,
        direction: IndicatorDirection::ConfirmsBranch,
        source_coverage_limits: "Public docket only".to_owned(),
        false_positive_caveat: "Maintenance notices share prefix".to_owned(),
    }];

    let v1_rec = comp_set
        .issue_forecast(
            &mut ledger,
            AnalogueForecastIssuanceSpec {
                forecast_id: OpaqueId::new("fc-a15")?,
                version_id: VersionId::new("fc-a15:v1")?,
                schema_version: SchemaVersion::new(1)?,
                actor: ActorId::new("analyst:1")?,
                issued_at_utc_ms: cutoff_v1_ms + 1_000,
                origin_utc_ms: cutoff_v1_ms,
                horizon_start_utc_ms: horizon_start_ms,
                horizon_end_utc_ms: horizon_end_ms,
                target_id: OpaqueId::new("target:caa")?,
                outcome_definition: outcome_def.clone(),
                baseline_version: VersionId::new("baseline:cluster:v1")?,
                baseline_probability: Probability::new(0.50)?,
                forecast_probability: Probability::new(0.60)?,
                model_version: VersionId::new("model:analogue:v1")?,
                model_knowledge_cutoff_utc_ms: Some(cutoff_v1_ms),
                branches: branches.clone(),
                indicators: indicators.clone(),
                justification: PredictionJustification::JustifiedQuantitativeForecast {
                    calibration_and_adjustment_rationale: "Cluster base rate 0.50 adjusted to 0.60"
                        .to_owned(),
                    sparse_history_override_rationale: None,
                },
                scenario_sources: vec![source_at(
                    "src:scen-init:v1",
                    cutoff_v1_ms - DAY_MS,
                    cutoff_v1_ms - 10_000,
                    cutoff_v1_ms - 5_000,
                )?],
                assumptions: vec!["Sensor feeds remain online".to_owned()],
                predecessor_version: None,
                revision_rationale: None,
            },
        )?
        .clone();
    // Pinned comparison_set begins with the structured comparison-set VersionId (`cs-a15:v1`).
    assert_eq!(v1_rec.comparison_set[0], VersionId::new("cs-a15:v1")?);

    // Revise forecast 5 days later (`cutoff_v2_ms = day 105`), after `src:ep5-late-out:v1` became
    // publicly available at day 104! Re-retrieval at `cutoff_v2_ms` transitions Episode 5 from
    // `CensoredAtCutoff` to `SupportiveAnalogue` and records `AdvanceCutoff` in `edit_history`.
    let cutoff_v2_ms = cutoff_v1_ms + 5 * DAY_MS;
    let mut comp_set_v2 = comp_set.re_retrieve_at_cutoff(
        &lib,
        VersionId::new("cs-a15:v2")?,
        ActorId::new("analyst:1")?,
        cutoff_v2_ms,
        cutoff_v2_ms,
        "Re-retrieve comparison set at day-105 cutoff for vintage 2 revision".to_owned(),
    )?;
    assert_eq!(
        comp_set_v2.current_version_id(),
        &VersionId::new("cs-a15:v2")?
    );
    assert_eq!(comp_set_v2.edit_history().len(), 1);
    let ep5_v2 = comp_set_v2
        .active_entries()
        .iter()
        .find(|e| e.episode_id.as_str() == "ep-post-cutoff-outcome")
        .ok_or("missing ep-post-cutoff-outcome at v2")?;
    assert_eq!(ep5_v2.role, PrecedentRole::SupportiveAnalogue);

    let v2_rec = comp_set_v2
        .issue_forecast(
            &mut ledger,
            AnalogueForecastIssuanceSpec {
                forecast_id: OpaqueId::new("fc-a15")?,
                version_id: VersionId::new("fc-a15:v2")?,
                schema_version: SchemaVersion::new(1)?,
                actor: ActorId::new("analyst:1")?,
                issued_at_utc_ms: cutoff_v2_ms + 1_000,
                origin_utc_ms: cutoff_v2_ms,
                horizon_start_utc_ms: horizon_start_ms,
                horizon_end_utc_ms: horizon_end_ms,
                target_id: OpaqueId::new("target:caa")?,
                outcome_definition: outcome_def.clone(),
                baseline_version: VersionId::new("baseline:cluster:v1")?,
                baseline_probability: Probability::new(0.50)?,
                forecast_probability: Probability::new(0.75)?,
                model_version: VersionId::new("model:analogue:v1")?,
                model_knowledge_cutoff_utc_ms: Some(cutoff_v2_ms),
                branches,
                indicators,
                justification: PredictionJustification::JustifiedQuantitativeForecast {
                    calibration_and_adjustment_rationale: "Step 1 docket indicator observed"
                        .to_owned(),
                    sparse_history_override_rationale: None,
                },
                scenario_sources: vec![
                    source_at(
                        "src:scen-init:v1",
                        cutoff_v1_ms - DAY_MS,
                        cutoff_v1_ms - 10_000,
                        cutoff_v1_ms - 5_000,
                    )?,
                    source_at(
                        "src:scen-ind1:v1",
                        cutoff_v2_ms - 5_000,
                        cutoff_v2_ms - 2_000,
                        cutoff_v2_ms - 1_000,
                    )?,
                ],
                assumptions: vec!["Working group convened".to_owned()],
                predecessor_version: Some(VersionId::new("fc-a15:v1")?),
                revision_rationale: Some(RevisionRationale {
                    new_evidence_versions: vec![VersionId::new("src:scen-ind1:v1")?],
                    summary: "Indicator ind-1 confirmed step 1".to_owned(),
                }),
            },
        )?
        .clone();
    assert_eq!(v2_rec.comparison_set[0], VersionId::new("cs-a15:v2")?);
    // Original vintage v1 remains byte-identical after v2 revision.
    assert_eq!(ledger.vintage(&VersionId::new("fc-a15:v1")?), Some(&v1_rec));

    // Resolve event at scenario_init + 7 days (against original `comp_set` where elapsed range is [4d, 6d, 8d]).
    let scenario_init_ms = cutoff_v1_ms - DAY_MS;
    let observed_ms = scenario_init_ms + 7 * DAY_MS;
    let res = ledger
        .record_outcome_resolution(OutcomeResolutionRecord {
            resolution_id: OpaqueId::new("res-a15")?,
            version_id: VersionId::new("res-a15:v1")?,
            outcome_id: outcome_def.outcome_id,
            actor: ActorId::new("analyst:2")?,
            recorded_at_utc_ms: observed_ms + 1_000,
            evidence_versions: vec![VersionId::new("src:res-notam:v1")?],
            state: ResolutionState::Resolved {
                occurred: true,
                observed_event_utc_ms: Some(observed_ms),
            },
            supersedes_version: None,
            correction_rationale: None,
        })?
        .clone();

    let timing = comp_set.evaluate_resolution_timing(scenario_init_ms, &res)?;
    assert_eq!(
        timing,
        AnalogueTimingEvaluation::ResolvedOccurrence {
            observed_event_utc_ms: observed_ms,
            observed_elapsed_ms: 7 * DAY_MS,
            precedent_range: ElapsedTimeRangeMs {
                resolved_episode_count: 3,
                min_elapsed_ms: 4 * DAY_MS,
                median_elapsed_ms: 6 * DAY_MS,
                max_elapsed_ms: 8 * DAY_MS,
            },
            signed_error_from_median_ms: DAY_MS,
            abs_error_from_median_ms: DAY_MS,
            within_precedent_min_max_band: true,
            band_excursion_ms: 0,
        }
    );

    let eval_ver = VersionId::new("eval:a15:v1")?;
    let eval = ledger.evaluate_cohort(
        eval_ver.clone(),
        observed_ms + 2_000,
        &[
            (VersionId::new("fc-a15:v1")?, VersionId::new("res-a15:v1")?),
            (VersionId::new("fc-a15:v2")?, VersionId::new("res-a15:v1")?),
        ],
        10,
    )?;
    assert_eq!(eval.vintages.len(), 2);
    assert_eq!(eval.resolved_count, 2);
    assert_eq!(eval.vintages[0].vintage_number, 1);
    assert_eq!(eval.vintages[1].vintage_number, 2);
    assert!(!eval.needs_reassessment);

    let rel = ledger.reliability_report(&eval_ver, 5, 10)?;
    assert_eq!(rel.rows.len(), 2);

    // Excluding a counterexample retains it in `excluded_entries` and flags transferability.
    comp_set_v2.apply_edit(
        &lib,
        ComparisonSetEdit {
            edit_version: VersionId::new("cs-a15:v3")?,
            actor: ActorId::new("analyst:1")?,
            edited_at_utc_ms: cutoff_v2_ms + 2_000,
            action: ComparisonEditAction::ExcludeEpisode {
                episode_id: OpaqueId::new("ep-divergent")?,
                rationale: "Statutory regime differed from regulatory authority".to_owned(),
            },
        },
    )?;
    let freq_after = comp_set_v2.summarize_frequencies();
    assert_eq!(freq_after.excluded_episode_count, 1);
    assert_eq!(freq_after.excluded_counterexample_count, 1);
    assert_eq!(
        comp_set_v2.assess_transferability(),
        TransferabilityAssessment::SparseOrPoorlyTransferable {
            reasons: vec![TransferLimitationReason::ExcludedCounterexamplesPresent {
                excluded_counterexample_count: 1,
            }],
        }
    );

    Ok(())
}

/// Risk / Requirements: `B2`, `B3`, Non-blocking 1, 2, 4 (`FORECAST-02`, `FORECAST-04`, `AGENTS.md` §§2, 4).
///
/// Establishes that:
/// 1. (`B2`) A shared crisis cluster with mixed outcomes (1 supportive + 2 divergent) alongside an
///    independent divergent episode is counted in `mixed_outcome_cluster_count` (`1`), kept out of
///    `independent_supportive_cluster_count` (`0`), and weighted by its within-cluster support
///    fraction (`(1/3) / 2 = 1666 bps`), so deduplication never inflates support above the raw
///    ratio (`2500 bps`).
/// 2. (`B3`) Recording a superseding correction to a precedent episode (`record_episode_correction`)
///    preserves the original episode version in `PrecedentLibrary::episode_history` and flags
///    dependent evaluations in `ForecastLedger` with `ReassessmentReason::PrecedentEpisodeSuperseded`
///    without mutating issued vintages or historical scores.
/// 3. (Non-blocking 1) Under `ProspectiveStrict`, an episode whose `recorded_at_utc_ms` is after
///    `information_cutoff_utc_ms` is rejected as `InadmissibleAtCutoff` even if its sources were
///    available before cutoff.
/// 4. (Non-blocking 2) Two episodes declared as `IndependentEpisode` that cite the exact same
///    outcome source version are merged into a single effective cluster (`independent_cluster_count = 1`).
/// 5. (Non-blocking 4) Conflicting source metadata timestamps for the same `source_version` are
///    rejected with `AnalogueError::ConflictingSourceMetadata`.
#[test]
fn mixed_clusters_precedent_corrections_strict_recorded_at_and_shared_sources_contract()
-> Result<(), Box<dyn Error>> {
    let mut lib = PrecedentLibrary::new(32)?;
    let profile = sample_profile(InstitutionalRegime::RegulatoryAgency);
    let t1 = T0_MS + 10 * DAY_MS;

    // Cluster X: 1 supportive episode + 2 divergent episodes.
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-mix-sup")?,
        version_id: VersionId::new("ep-mix-sup:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:a")?,
        recorded_at_utc_ms: t1 + 5 * DAY_MS,
        title: "Mixed cluster supportive member".to_owned(),
        initiating_condition: "Cluster X trigger".to_owned(),
        initiating_event_utc_ms: t1,
        initiation_evidence_versions: vec![VersionId::new("src:mix-init:v1")?],
        profile: profile.clone(),
        dependency: EpisodeDependencyKind::SharedCrisisOrCampaign {
            cluster_id: OpaqueId::new("cluster-x")?,
            dependency_note: "Cluster X member 1".to_owned(),
        },
        response_steps: Vec::new(),
        outcome: PrecedentOutcomeDisposition::TargetResponseOccurred {
            outcome_summary: "Issued directive".to_owned(),
            resolved_at_utc_ms: t1 + 2 * DAY_MS,
            elapsed_from_initiation_ms: 2 * DAY_MS,
            outcome_evidence_versions: vec![VersionId::new("src:mix-sup-out:v1")?],
        },
        sources: vec![
            source_at("src:mix-init:v1", t1, t1 + 1_000, t1 + 2_000)?,
            source_at(
                "src:mix-sup-out:v1",
                t1 + 2 * DAY_MS,
                t1 + 2 * DAY_MS + 1_000,
                t1 + 2 * DAY_MS + 2_000,
            )?,
        ],
        supersedes_version: None,
        correction_rationale: None,
    })?;

    for (idx, ep_id, out_src) in [
        (1, "ep-mix-div-1", "src:mix-div1-out:v1"),
        (2, "ep-mix-div-2", "src:mix-div2-out:v1"),
    ] {
        lib.register_episode(PrecedentEpisodeRecord {
            episode_id: OpaqueId::new(ep_id)?,
            version_id: VersionId::new(format!("{ep_id}:v1"))?,
            schema_version: SchemaVersion::new(1)?,
            recorded_by: ActorId::new("curator:a")?,
            recorded_at_utc_ms: t1 + (5 + idx) * DAY_MS,
            title: format!("Mixed cluster divergent member {idx}"),
            initiating_condition: "Cluster X trigger".to_owned(),
            initiating_event_utc_ms: t1,
            initiation_evidence_versions: vec![VersionId::new("src:mix-init:v1")?],
            profile: profile.clone(),
            dependency: EpisodeDependencyKind::SharedCrisisOrCampaign {
                cluster_id: OpaqueId::new("cluster-x")?,
                dependency_note: "Cluster X divergent member".to_owned(),
            },
            response_steps: Vec::new(),
            outcome: PrecedentOutcomeDisposition::DivergentResponseOccurred {
                divergent_action_summary: "Divergent response".to_owned(),
                resolved_at_utc_ms: t1 + 3 * DAY_MS,
                elapsed_from_initiation_ms: 3 * DAY_MS,
                outcome_evidence_versions: vec![VersionId::new(out_src)?],
            },
            sources: vec![
                source_at("src:mix-init:v1", t1, t1 + 1_000, t1 + 2_000)?,
                source_at(
                    out_src,
                    t1 + 3 * DAY_MS,
                    t1 + 3 * DAY_MS + 1_000,
                    t1 + 3 * DAY_MS + 2_000,
                )?,
            ],
            supersedes_version: None,
            correction_rationale: None,
        })?;
    }

    // Two episodes declared as `IndependentEpisode` that cite the SAME outcome source `src:shared-div-out:v1` (Non-blocking 2).
    for ep_id in ["ep-ind-div-a", "ep-ind-div-b"] {
        lib.register_episode(PrecedentEpisodeRecord {
            episode_id: OpaqueId::new(ep_id)?,
            version_id: VersionId::new(format!("{ep_id}:v1"))?,
            schema_version: SchemaVersion::new(1)?,
            recorded_by: ActorId::new("curator:a")?,
            recorded_at_utc_ms: t1 + 8 * DAY_MS,
            title: format!("Independent episode sharing outcome source ({ep_id})"),
            initiating_condition: "Independent trigger".to_owned(),
            initiating_event_utc_ms: t1,
            initiation_evidence_versions: vec![VersionId::new("src:mix-init:v1")?],
            profile: profile.clone(),
            dependency: EpisodeDependencyKind::IndependentEpisode,
            response_steps: Vec::new(),
            outcome: PrecedentOutcomeDisposition::DivergentResponseOccurred {
                divergent_action_summary: "Divergent action reported in shared source".to_owned(),
                resolved_at_utc_ms: t1 + 4 * DAY_MS,
                elapsed_from_initiation_ms: 4 * DAY_MS,
                outcome_evidence_versions: vec![VersionId::new("src:shared-div-out:v1")?],
            },
            sources: vec![
                source_at("src:mix-init:v1", t1, t1 + 1_000, t1 + 2_000)?,
                source_at(
                    "src:shared-div-out:v1",
                    t1 + 4 * DAY_MS,
                    t1 + 4 * DAY_MS + 1_000,
                    t1 + 4 * DAY_MS + 2_000,
                )?,
            ],
            supersedes_version: None,
            correction_rationale: None,
        })?;
    }

    // Episode curated AFTER the day-30 cutoff (`recorded_at_utc_ms = day 35`), even though its sources
    // were published at day 12 (Non-blocking 1).
    let cutoff_ms = T0_MS + 30 * DAY_MS;
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-hindsight-curated")?,
        version_id: VersionId::new("ep-hindsight-curated:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:a")?,
        recorded_at_utc_ms: cutoff_ms + 5 * DAY_MS,
        title: "Hindsight-curated episode after strict cutoff".to_owned(),
        initiating_condition: "Historical event curated with post-cutoff hindsight".to_owned(),
        initiating_event_utc_ms: t1,
        initiation_evidence_versions: vec![VersionId::new("src:mix-init:v1")?],
        profile: profile.clone(),
        dependency: EpisodeDependencyKind::IndependentEpisode,
        response_steps: Vec::new(),
        outcome: PrecedentOutcomeDisposition::TargetResponseOccurred {
            outcome_summary: "Issued directive".to_owned(),
            resolved_at_utc_ms: t1 + 2 * DAY_MS,
            elapsed_from_initiation_ms: 2 * DAY_MS,
            outcome_evidence_versions: vec![VersionId::new("src:mix-sup-out:v1")?],
        },
        sources: vec![
            source_at("src:mix-init:v1", t1, t1 + 1_000, t1 + 2_000)?,
            source_at(
                "src:mix-sup-out:v1",
                t1 + 2 * DAY_MS,
                t1 + 2 * DAY_MS + 1_000,
                t1 + 2 * DAY_MS + 2_000,
            )?,
        ],
        supersedes_version: None,
        correction_rationale: None,
    })?;

    // Conflicting source metadata (`src:mix-init:v1` with different `public_available_at_utc_ms`) is rejected (Non-blocking 4).
    let conflict_err = lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-conflict-src")?,
        version_id: VersionId::new("ep-conflict-src:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:a")?,
        recorded_at_utc_ms: t1 + 5 * DAY_MS,
        title: "Conflicting source metadata episode".to_owned(),
        initiating_condition: "Trigger".to_owned(),
        initiating_event_utc_ms: t1,
        initiation_evidence_versions: vec![VersionId::new("src:mix-init:v1")?],
        profile: profile.clone(),
        dependency: EpisodeDependencyKind::IndependentEpisode,
        response_steps: Vec::new(),
        outcome: PrecedentOutcomeDisposition::TargetResponseOccurred {
            outcome_summary: "Directive".to_owned(),
            resolved_at_utc_ms: t1 + DAY_MS,
            elapsed_from_initiation_ms: DAY_MS,
            outcome_evidence_versions: vec![VersionId::new("src:mix-init:v1")?],
        },
        sources: vec![source_at("src:mix-init:v1", t1, t1 + 9_999, t1 + 9_999)?],
        supersedes_version: None,
        correction_rationale: None,
    });
    assert!(matches!(
        conflict_err,
        Err(AnalogueError::ConflictingSourceMetadata { .. })
    ));

    let cs = lib.retrieve_comparison_set(AnalogueQueryCriteria {
        comparison_set_id: OpaqueId::new("cs-mix")?,
        initial_version_id: VersionId::new("cs-mix:v1")?,
        actor: ActorId::new("analyst:1")?,
        created_at_utc_ms: cutoff_ms,
        information_cutoff_utc_ms: cutoff_ms,
        cutoff_mode: CutoffAdmissibilityMode::ProspectiveStrict,
        target_profile: profile,
        min_matched_dimensions: 7,
        min_independent_clusters: 1,
    })?;

    // `ep-hindsight-curated` was rejected at strict cutoff because `recorded_at_utc_ms > cutoff_ms`.
    assert_eq!(cs.inadmissible_episode_ids().len(), 1);
    assert_eq!(
        cs.inadmissible_episode_ids()[0].0,
        OpaqueId::new("ep-hindsight-curated")?
    );

    // 5 active episodes:
    // - 3 in `cluster-x` (1 supportive + 2 divergent -> 1 mixed-outcome cluster with weight 1/3 = 3333 bps)
    // - 2 declared independent episodes sharing `src:shared-div-out:v1`, merged into 1 counterexample cluster (weight 0)
    // Total independent resolved clusters = 2 (`cluster-x` and `shared-outcome-source:src:shared-div-out:v1`).
    // Raw support ratio = 1 / 5 = 2000 bps.
    // Cluster-deduplicated support ratio = 3333 / 2 = 1666 bps (strictly <= raw ratio, never inflated!).
    let freq = cs.summarize_frequencies();
    assert_eq!(freq.active_episode_count, 5);
    assert_eq!(freq.independent_cluster_count, 2);
    assert_eq!(freq.independent_resolved_cluster_count, 2);
    assert_eq!(freq.independent_supportive_cluster_count, 0);
    assert_eq!(freq.independent_counterexample_cluster_count, 1);
    assert_eq!(freq.mixed_outcome_cluster_count, 1);
    assert_eq!(freq.raw_resolved_support_ratio_bps, Some(2_000));
    assert_eq!(freq.independent_cluster_support_ratio_bps, Some(1_666));

    // Issue a forecast backed by `cs`, evaluate it, and then record a precedent correction to `ep-mix-sup`.
    let mut ledger = ForecastLedger::new(16)?;
    let outcome_def = OutcomeDefinition {
        outcome_id: OpaqueId::new("out-mix")?,
        proposition: "Directive issued".to_owned(),
        resolution_rules: "Official register".to_owned(),
    };
    cs.issue_forecast(
        &mut ledger,
        AnalogueForecastIssuanceSpec {
            forecast_id: OpaqueId::new("fc-mix")?,
            version_id: VersionId::new("fc-mix:v1")?,
            schema_version: SchemaVersion::new(1)?,
            actor: ActorId::new("analyst:1")?,
            issued_at_utc_ms: cutoff_ms + 1_000,
            origin_utc_ms: cutoff_ms,
            horizon_start_utc_ms: cutoff_ms + DAY_MS,
            horizon_end_utc_ms: cutoff_ms + 10 * DAY_MS,
            target_id: OpaqueId::new("target:caa")?,
            outcome_definition: outcome_def.clone(),
            baseline_version: VersionId::new("base:mix:v1")?,
            baseline_probability: Probability::new(0.20)?,
            forecast_probability: Probability::new(0.25)?,
            model_version: VersionId::new("model:mix:v1")?,
            model_knowledge_cutoff_utc_ms: Some(cutoff_ms),
            branches: vec![ConditionalResponseBranch {
                branch_id: OpaqueId::new("b-mix")?,
                branch_label: "Single-step branch".to_owned(),
                is_non_response_or_divergent: false,
                steps: vec![ConditionalResponseStep {
                    step_number: 1,
                    actor_sector: PrecedentSector::Government,
                    action_description: "Issue directive".to_owned(),
                    conditional_probability: Probability::new(0.25)?,
                    dependency_rationale: "Trigger".to_owned(),
                    expected_step_window_ms: None,
                }],
                chained_probability: Probability::new(0.25)?,
                expected_elapsed_range_ms: None,
                supporting_episode_versions: vec![VersionId::new("ep-mix-sup:v1")?],
            }],
            indicators: vec![MonitoringIndicator {
                indicator_id: OpaqueId::new("ind-mix")?,
                observable_signal: "Docket notice".to_owned(),
                target_branch_id: OpaqueId::new("b-mix")?,
                direction: IndicatorDirection::ConfirmsBranch,
                source_coverage_limits: "Public register".to_owned(),
                false_positive_caveat: "Routine notices".to_owned(),
            }],
            justification: PredictionJustification::JustifiedQuantitativeForecast {
                calibration_and_adjustment_rationale: "Weighted cluster support 16.66%".to_owned(),
                sparse_history_override_rationale: None,
            },
            scenario_sources: Vec::new(),
            assumptions: Vec::new(),
            predecessor_version: None,
            revision_rationale: None,
        },
    )?;

    let obs_ms = cutoff_ms + 3 * DAY_MS;
    ledger.record_outcome_resolution(OutcomeResolutionRecord {
        resolution_id: OpaqueId::new("res-mix")?,
        version_id: VersionId::new("res-mix:v1")?,
        outcome_id: outcome_def.outcome_id,
        actor: ActorId::new("analyst:2")?,
        recorded_at_utc_ms: obs_ms + 1_000,
        evidence_versions: vec![VersionId::new("src:res-mix:v1")?],
        state: ResolutionState::Resolved {
            occurred: true,
            observed_event_utc_ms: Some(obs_ms),
        },
        supersedes_version: None,
        correction_rationale: None,
    })?;

    let eval_ver = VersionId::new("eval:mix:v1")?;
    let eval_before = ledger
        .evaluate_cohort(
            eval_ver.clone(),
            obs_ms + 2_000,
            &[(VersionId::new("fc-mix:v1")?, VersionId::new("res-mix:v1")?)],
            10,
        )?
        .clone();
    assert!(!eval_before.needs_reassessment);

    // Supersede `ep-mix-sup:v1` with `ep-mix-sup:v2` after an archival correction reveals it was a divergent response (`B3`).
    let mut corrected = lib
        .get(&OpaqueId::new("ep-mix-sup")?)
        .ok_or("missing ep-mix-sup")?
        .clone();
    corrected.version_id = VersionId::new("ep-mix-sup:v2")?;
    corrected.recorded_at_utc_ms = obs_ms + 5_000;
    corrected.outcome = PrecedentOutcomeDisposition::DivergentResponseOccurred {
        divergent_action_summary: "Archival correction: referral only, no NOTAM directive"
            .to_owned(),
        resolved_at_utc_ms: t1 + 2 * DAY_MS,
        elapsed_from_initiation_ms: 2 * DAY_MS,
        outcome_evidence_versions: vec![VersionId::new("src:mix-sup-out:v1")?],
    };
    corrected.supersedes_version = Some(VersionId::new("ep-mix-sup:v1")?);
    corrected.correction_rationale =
        Some("Corrected outcome disposition from archival docket erratum".to_owned());

    let (_, affected_vintages) = lib.record_episode_correction(corrected, &mut ledger)?;
    assert_eq!(affected_vintages, vec![VersionId::new("fc-mix:v1")?]);
    assert_eq!(lib.episode_history(&OpaqueId::new("ep-mix-sup")?).len(), 2);

    let eval_after = ledger.evaluation(&eval_ver).ok_or("missing eval:mix:v1")?;
    assert!(eval_after.needs_reassessment);
    assert_eq!(
        eval_after.reassessment_reasons,
        vec![ReassessmentReason::PrecedentEpisodeSuperseded {
            episode_id: OpaqueId::new("ep-mix-sup")?,
            previous_episode_version: VersionId::new("ep-mix-sup:v1")?,
            new_episode_version: VersionId::new("ep-mix-sup:v2")?,
        }]
    );
    // Historical scores on `eval:mix:v1` are untouched.
    assert_eq!(eval_after.mean_scores, eval_before.mean_scores);

    Ok(())
}

/// Risk / Requirements: `FORECAST-01`, `FORECAST-03`, `FORECAST-04`, `AGENTS.md` §5.
///
/// Verifies that malformed chronology, inadmissible manual inclusions, inconsistent multi-step
/// branch probabilities, and unjustified quantitative forecasts over sparse history are rejected.
#[test]
fn rejects_invalid_chronology_unjustified_sparse_issuance_and_inconsistent_branch_chains()
-> Result<(), Box<dyn Error>> {
    let mut lib = PrecedentLibrary::new(16)?;
    let profile = sample_profile(InstitutionalRegime::RegulatoryAgency);
    let t1 = T0_MS + 5 * DAY_MS;

    // 1. Mismatched elapsed_from_initiation_ms is rejected.
    let bad_chronology = lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-bad-elapsed")?,
        version_id: VersionId::new("ep-bad-elapsed:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:a")?,
        recorded_at_utc_ms: t1 + 4 * DAY_MS,
        title: "Mismatched elapsed time".to_owned(),
        initiating_condition: "Radar anomaly".to_owned(),
        initiating_event_utc_ms: t1,
        initiation_evidence_versions: vec![VersionId::new("src:1:v1")?],
        profile: profile.clone(),
        dependency: EpisodeDependencyKind::IndependentEpisode,
        response_steps: Vec::new(),
        outcome: PrecedentOutcomeDisposition::TargetResponseOccurred {
            outcome_summary: "Directive".to_owned(),
            resolved_at_utc_ms: t1 + 3 * DAY_MS,
            elapsed_from_initiation_ms: 2 * DAY_MS, // wrong!
            outcome_evidence_versions: vec![VersionId::new("src:1:v1")?],
        },
        sources: vec![source_at("src:1:v1", t1, t1 + 1_000, t1 + 2_000)?],
        supersedes_version: None,
        correction_rationale: None,
    });
    assert!(matches!(
        bad_chronology,
        Err(AnalogueError::InvalidEpisodeChronology { .. })
    ));

    // Register one valid episode and one post-cutoff episode.
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-valid-1")?,
        version_id: VersionId::new("ep-valid-1:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:a")?,
        recorded_at_utc_ms: t1 + 4 * DAY_MS,
        title: "Single valid episode".to_owned(),
        initiating_condition: "Radar anomaly".to_owned(),
        initiating_event_utc_ms: t1,
        initiation_evidence_versions: vec![VersionId::new("src:1:v1")?],
        profile: profile.clone(),
        dependency: EpisodeDependencyKind::IndependentEpisode,
        response_steps: Vec::new(),
        outcome: PrecedentOutcomeDisposition::TargetResponseOccurred {
            outcome_summary: "Directive".to_owned(),
            resolved_at_utc_ms: t1 + 3 * DAY_MS,
            elapsed_from_initiation_ms: 3 * DAY_MS,
            outcome_evidence_versions: vec![VersionId::new("src:1:v1")?],
        },
        sources: vec![source_at("src:1:v1", t1, t1 + 1_000, t1 + 2_000)?],
        supersedes_version: None,
        correction_rationale: None,
    })?;

    let t_future = T0_MS + 50 * DAY_MS;
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-future")?,
        version_id: VersionId::new("ep-future:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:a")?,
        recorded_at_utc_ms: t_future + 2 * DAY_MS,
        title: "Post-cutoff episode".to_owned(),
        initiating_condition: "Future anomaly".to_owned(),
        initiating_event_utc_ms: t_future,
        initiation_evidence_versions: vec![VersionId::new("src:future:v1")?],
        profile: profile.clone(),
        dependency: EpisodeDependencyKind::IndependentEpisode,
        response_steps: Vec::new(),
        outcome: PrecedentOutcomeDisposition::TargetResponseOccurred {
            outcome_summary: "Future directive".to_owned(),
            resolved_at_utc_ms: t_future + DAY_MS,
            elapsed_from_initiation_ms: DAY_MS,
            outcome_evidence_versions: vec![VersionId::new("src:future:v1")?],
        },
        sources: vec![source_at(
            "src:future:v1",
            t_future,
            t_future + 1_000,
            t_future + 2_000,
        )?],
        supersedes_version: None,
        correction_rationale: None,
    })?;

    let cutoff_ms = T0_MS + 20 * DAY_MS;
    let mut comp_set = lib.retrieve_comparison_set(AnalogueQueryCriteria {
        comparison_set_id: OpaqueId::new("cs-sparse")?,
        initial_version_id: VersionId::new("cs-sparse:v1")?,
        actor: ActorId::new("analyst:1")?,
        created_at_utc_ms: cutoff_ms,
        information_cutoff_utc_ms: cutoff_ms,
        cutoff_mode: CutoffAdmissibilityMode::ProspectiveStrict,
        target_profile: profile,
        min_matched_dimensions: 5,
        min_independent_clusters: 3, // requires 3, only 1 available -> SparseOrPoorlyTransferable
    })?;

    // 2. Manually including a post-cutoff episode via ComparisonSetEdit is rejected.
    let include_future = comp_set.apply_edit(
        &lib,
        ComparisonSetEdit {
            edit_version: VersionId::new("cs-sparse:v2")?,
            actor: ActorId::new("analyst:1")?,
            edited_at_utc_ms: cutoff_ms + 1_000,
            action: ComparisonEditAction::IncludeEpisode {
                episode_id: OpaqueId::new("ep-future")?,
                rationale: "Attempt to include post-cutoff episode".to_owned(),
                transfer_limitations: Vec::new(),
            },
        },
    );
    assert!(matches!(
        include_future,
        Err(AnalogueError::EpisodeInadmissibleAtCutoff { .. })
    ));

    let mut ledger = ForecastLedger::new(16)?;
    let outcome_def = OutcomeDefinition {
        outcome_id: OpaqueId::new("out-sparse")?,
        proposition: "Directive issued".to_owned(),
        resolution_rules: "Official register".to_owned(),
    };
    let indicator = MonitoringIndicator {
        indicator_id: OpaqueId::new("ind-sparse")?,
        observable_signal: "Docket opened".to_owned(),
        target_branch_id: OpaqueId::new("b-sparse")?,
        direction: IndicatorDirection::ConfirmsBranch,
        source_coverage_limits: "Public register".to_owned(),
        false_positive_caveat: "Routine notices".to_owned(),
    };

    // 3. Inconsistent chained_probability (0.8 * 0.5 != 0.55) is rejected.
    let bad_branch = ConditionalResponseBranch {
        branch_id: OpaqueId::new("b-sparse")?,
        branch_label: "Two-step branch".to_owned(),
        is_non_response_or_divergent: false,
        steps: vec![
            ConditionalResponseStep {
                step_number: 1,
                actor_sector: PrecedentSector::Government,
                action_description: "Step 1".to_owned(),
                conditional_probability: Probability::new(0.80)?,
                dependency_rationale: "Trigger".to_owned(),
                expected_step_window_ms: None,
            },
            ConditionalResponseStep {
                step_number: 2,
                actor_sector: PrecedentSector::Government,
                action_description: "Step 2".to_owned(),
                conditional_probability: Probability::new(0.50)?,
                dependency_rationale: "Given step 1".to_owned(),
                expected_step_window_ms: None,
            },
        ],
        chained_probability: Probability::new(0.55)?,
        expected_elapsed_range_ms: None,
        supporting_episode_versions: vec![VersionId::new("ep-valid-1:v1")?],
    };

    let bad_chain_res = comp_set.issue_forecast(
        &mut ledger,
        AnalogueForecastIssuanceSpec {
            forecast_id: OpaqueId::new("fc-sparse")?,
            version_id: VersionId::new("fc-sparse:v1")?,
            schema_version: SchemaVersion::new(1)?,
            actor: ActorId::new("analyst:1")?,
            issued_at_utc_ms: cutoff_ms + 1_000,
            origin_utc_ms: cutoff_ms,
            horizon_start_utc_ms: cutoff_ms + DAY_MS,
            horizon_end_utc_ms: cutoff_ms + 10 * DAY_MS,
            target_id: OpaqueId::new("target:caa")?,
            outcome_definition: outcome_def.clone(),
            baseline_version: VersionId::new("base:v1")?,
            baseline_probability: Probability::new(0.50)?,
            forecast_probability: Probability::new(0.40)?,
            model_version: VersionId::new("model:v1")?,
            model_knowledge_cutoff_utc_ms: Some(cutoff_ms),
            branches: vec![bad_branch],
            indicators: vec![indicator.clone()],
            justification: PredictionJustification::JustifiedQuantitativeForecast {
                calibration_and_adjustment_rationale: "Adjusted from single case".to_owned(),
                sparse_history_override_rationale: Some("Explicit override".to_owned()),
            },
            scenario_sources: Vec::new(),
            assumptions: Vec::new(),
            predecessor_version: None,
            revision_rationale: None,
        },
    );
    assert!(matches!(
        bad_chain_res,
        Err(AnalogueError::InconsistentChainedProbability { .. })
    ));

    // 4. Quantitative forecast on SparseOrPoorlyTransferable history without override rationale is rejected.
    let valid_branch = ConditionalResponseBranch {
        branch_id: OpaqueId::new("b-sparse")?,
        branch_label: "Two-step branch".to_owned(),
        is_non_response_or_divergent: false,
        steps: vec![ConditionalResponseStep {
            step_number: 1,
            actor_sector: PrecedentSector::Government,
            action_description: "Step 1".to_owned(),
            conditional_probability: Probability::new(0.40)?,
            dependency_rationale: "Trigger".to_owned(),
            expected_step_window_ms: None,
        }],
        chained_probability: Probability::new(0.40)?,
        expected_elapsed_range_ms: None,
        supporting_episode_versions: vec![VersionId::new("ep-valid-1:v1")?],
    };

    let missing_override_res = comp_set.issue_forecast(
        &mut ledger,
        AnalogueForecastIssuanceSpec {
            forecast_id: OpaqueId::new("fc-sparse")?,
            version_id: VersionId::new("fc-sparse:v1")?,
            schema_version: SchemaVersion::new(1)?,
            actor: ActorId::new("analyst:1")?,
            issued_at_utc_ms: cutoff_ms + 1_000,
            origin_utc_ms: cutoff_ms,
            horizon_start_utc_ms: cutoff_ms + DAY_MS,
            horizon_end_utc_ms: cutoff_ms + 10 * DAY_MS,
            target_id: OpaqueId::new("target:caa")?,
            outcome_definition: outcome_def,
            baseline_version: VersionId::new("base:v1")?,
            baseline_probability: Probability::new(0.50)?,
            forecast_probability: Probability::new(0.40)?,
            model_version: VersionId::new("model:v1")?,
            model_knowledge_cutoff_utc_ms: Some(cutoff_ms),
            branches: vec![valid_branch],
            indicators: vec![indicator],
            justification: PredictionJustification::JustifiedQuantitativeForecast {
                calibration_and_adjustment_rationale: "Adjusted from single case".to_owned(),
                sparse_history_override_rationale: None, // missing override!
            },
            scenario_sources: Vec::new(),
            assumptions: Vec::new(),
            predecessor_version: None,
            revision_rationale: None,
        },
    );
    assert!(matches!(
        missing_override_res,
        Err(AnalogueError::UnjustifiedQuantitativeForecast { .. })
    ));

    Ok(())
}

/// Risk / Requirements: `FORECAST-02`, `FORECAST-03`, `FORECAST-04`, `A15`, `P1`, `P2`, `N2`.
///
/// Verifies:
/// 1. `N2` (transitive & mixed-kind shared-source union-find): When episode A cites source X,
///    episode B cites sources X and Y, and episode C (`SharedCrisisOrCampaign`) cites source Y,
///    all three active episodes collapse into a single connected evidence component (`independent_cluster_count == 1`).
/// 2. `P2` (initial comparison-set version reservation): After editing `cs:v1` -> `cs:v2`,
///    attempting to reuse `cs:v1` in `apply_edit` or `re_retrieve_at_cutoff` is rejected with
///    `AnalogueError::DuplicateIdentifier`, and regressing `edited_at_utc_ms` is rejected.
/// 3. `P1` (issued scenario snapshot preservation): `ForecastVintageRecord::analogue_scenario`
///    retains the exact `ConditionalResponseBranch`es, `MonitoringIndicator`s, `TransferabilityAssessment`,
///    and `PredictionJustification` frozen at issuance so distinct conditional scenario models
///    produce distinct `ForecastVintageRecord`s.
#[test]
fn transitive_shared_source_union_initial_version_reservation_and_scenario_snapshot_contract()
-> Result<(), Box<dyn Error>> {
    let mut lib = PrecedentLibrary::new(16)?;
    let profile = sample_profile(InstitutionalRegime::RegulatoryAgency);
    let t1 = T0_MS + 2 * DAY_MS;
    let cutoff_ms = T0_MS + 10 * DAY_MS;

    let src_x = source_at("src:chain-x:v1", t1, t1 + 1_000, t1 + 2_000)?;
    let src_y = source_at("src:chain-y:v1", t1, t1 + 1_000, t1 + 2_000)?;

    // Episode A (IndependentEpisode): cites X
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-chain-a")?,
        version_id: VersionId::new("ep-chain-a:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:a")?,
        recorded_at_utc_ms: t1 + 3 * DAY_MS,
        title: "Chain episode A citing X".to_owned(),
        initiating_condition: "Anomaly A".to_owned(),
        initiating_event_utc_ms: t1,
        initiation_evidence_versions: vec![VersionId::new("src:chain-x:v1")?],
        profile: profile.clone(),
        dependency: EpisodeDependencyKind::IndependentEpisode,
        response_steps: Vec::new(),
        outcome: PrecedentOutcomeDisposition::TargetResponseOccurred {
            outcome_summary: "Directive A".to_owned(),
            resolved_at_utc_ms: t1 + DAY_MS,
            elapsed_from_initiation_ms: DAY_MS,
            outcome_evidence_versions: vec![VersionId::new("src:chain-x:v1")?],
        },
        sources: vec![src_x.clone()],
        supersedes_version: None,
        correction_rationale: None,
    })?;

    // Episode B (IndependentEpisode): cites X and Y
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-chain-b")?,
        version_id: VersionId::new("ep-chain-b:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:a")?,
        recorded_at_utc_ms: t1 + 3 * DAY_MS,
        title: "Chain episode B citing X and Y".to_owned(),
        initiating_condition: "Anomaly B".to_owned(),
        initiating_event_utc_ms: t1,
        initiation_evidence_versions: vec![VersionId::new("src:chain-x:v1")?],
        profile: profile.clone(),
        dependency: EpisodeDependencyKind::IndependentEpisode,
        response_steps: Vec::new(),
        outcome: PrecedentOutcomeDisposition::TargetResponseOccurred {
            outcome_summary: "Directive B".to_owned(),
            resolved_at_utc_ms: t1 + 2 * DAY_MS,
            elapsed_from_initiation_ms: 2 * DAY_MS,
            outcome_evidence_versions: vec![
                VersionId::new("src:chain-x:v1")?,
                VersionId::new("src:chain-y:v1")?,
            ],
        },
        sources: vec![src_x, src_y.clone()],
        supersedes_version: None,
        correction_rationale: None,
    })?;

    // Episode C (SharedCrisisOrCampaign): cites Y (tests mixed-kind + transitive chain A-B-C)
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-chain-c")?,
        version_id: VersionId::new("ep-chain-c:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:a")?,
        recorded_at_utc_ms: t1 + 3 * DAY_MS,
        title: "Chain episode C in campaign citing Y".to_owned(),
        initiating_condition: "Anomaly C".to_owned(),
        initiating_event_utc_ms: t1,
        initiation_evidence_versions: vec![VersionId::new("src:chain-y:v1")?],
        profile: profile.clone(),
        dependency: EpisodeDependencyKind::SharedCrisisOrCampaign {
            cluster_id: OpaqueId::new("cluster-campaign-c")?,
            dependency_note: "Coordinated regional campaign".to_owned(),
        },
        response_steps: Vec::new(),
        outcome: PrecedentOutcomeDisposition::TargetResponseOccurred {
            outcome_summary: "Directive C".to_owned(),
            resolved_at_utc_ms: t1 + 3 * DAY_MS,
            elapsed_from_initiation_ms: 3 * DAY_MS,
            outcome_evidence_versions: vec![VersionId::new("src:chain-y:v1")?],
        },
        sources: vec![src_y],
        supersedes_version: None,
        correction_rationale: None,
    })?;

    let mut comp_set = lib.retrieve_comparison_set(AnalogueQueryCriteria {
        comparison_set_id: OpaqueId::new("cs-chain")?,
        initial_version_id: VersionId::new("cs-chain:v1")?,
        actor: ActorId::new("analyst:1")?,
        created_at_utc_ms: cutoff_ms,
        information_cutoff_utc_ms: cutoff_ms,
        cutoff_mode: CutoffAdmissibilityMode::ProspectiveStrict,
        target_profile: profile,
        min_matched_dimensions: 5,
        min_independent_clusters: 2,
    })?;

    // N2: All 3 episodes form a single connected component via X and Y.
    let freq = comp_set.summarize_frequencies();
    assert_eq!(freq.active_episode_count, 3);
    assert_eq!(freq.independent_cluster_count, 1);
    assert_eq!(freq.independent_resolved_cluster_count, 1);
    assert!(matches!(
        comp_set.assess_transferability(),
        TransferabilityAssessment::SparseOrPoorlyTransferable { .. }
    ));

    // P2: Exclude ep-chain-c as cs-chain:v2, then attempt to reuse cs-chain:v1 in re_retrieve_at_cutoff and apply_edit.
    comp_set.apply_edit(
        &lib,
        ComparisonSetEdit {
            edit_version: VersionId::new("cs-chain:v2")?,
            actor: ActorId::new("analyst:1")?,
            edited_at_utc_ms: cutoff_ms + 1_000,
            action: ComparisonEditAction::ExcludeEpisode {
                episode_id: OpaqueId::new("ep-chain-c")?,
                rationale: "Testing initial version reservation".to_owned(),
            },
        },
    )?;
    assert_eq!(
        comp_set.initial_version_id(),
        &VersionId::new("cs-chain:v1")?
    );
    assert_eq!(
        comp_set.current_version_id(),
        &VersionId::new("cs-chain:v2")?
    );

    let reuse_initial_in_re_retrieve = comp_set.re_retrieve_at_cutoff(
        &lib,
        VersionId::new("cs-chain:v1")?,
        ActorId::new("analyst:1")?,
        cutoff_ms + 2_000,
        cutoff_ms + 1_500,
        "Attempt to reuse initial_version_id".to_owned(),
    );
    assert!(matches!(
        reuse_initial_in_re_retrieve,
        Err(AnalogueError::DuplicateIdentifier {
            kind: "edit_version",
            ..
        })
    ));

    let reuse_initial_in_edit = comp_set.apply_edit(
        &lib,
        ComparisonSetEdit {
            edit_version: VersionId::new("cs-chain:v1")?,
            actor: ActorId::new("analyst:1")?,
            edited_at_utc_ms: cutoff_ms + 2_000,
            action: ComparisonEditAction::IncludeEpisode {
                episode_id: OpaqueId::new("ep-chain-c")?,
                rationale: "Attempt to reuse initial_version_id in apply_edit".to_owned(),
                transfer_limitations: Vec::new(),
            },
        },
    );
    assert!(matches!(
        reuse_initial_in_edit,
        Err(AnalogueError::DuplicateIdentifier {
            kind: "edit_version",
            ..
        })
    ));

    // Non-monotonic edit timestamp is also rejected.
    let regressed_edit_ts = comp_set.apply_edit(
        &lib,
        ComparisonSetEdit {
            edit_version: VersionId::new("cs-chain:v3")?,
            actor: ActorId::new("analyst:1")?,
            edited_at_utc_ms: cutoff_ms + 500, // earlier than cutoff_ms + 1_000!
            action: ComparisonEditAction::IncludeEpisode {
                episode_id: OpaqueId::new("ep-chain-c")?,
                rationale: "Regressed edit timestamp".to_owned(),
                transfer_limitations: Vec::new(),
            },
        },
    );
    assert!(matches!(
        regressed_edit_ts,
        Err(AnalogueError::InvalidQueryCriteria(_))
    ));

    // P1: Issue two specs differing only in branch probability / indicator signal into separate ledgers
    // and verify that `ForecastVintageRecord::analogue_scenario` preserves the full scenario snapshot.
    let make_spec =
        |step_prob: f64, signal: &str| -> Result<AnalogueForecastIssuanceSpec, Box<dyn Error>> {
            Ok(AnalogueForecastIssuanceSpec {
                forecast_id: OpaqueId::new("fc-snap")?,
                version_id: VersionId::new("fc-snap:v1")?,
                schema_version: SchemaVersion::new(1)?,
                actor: ActorId::new("analyst:1")?,
                issued_at_utc_ms: cutoff_ms + 2_000,
                origin_utc_ms: cutoff_ms,
                horizon_start_utc_ms: cutoff_ms + DAY_MS,
                horizon_end_utc_ms: cutoff_ms + 10 * DAY_MS,
                target_id: OpaqueId::new("target:caa")?,
                outcome_definition: OutcomeDefinition {
                    outcome_id: OpaqueId::new("out-snap")?,
                    proposition: "Directive issued".to_owned(),
                    resolution_rules: "Official register".to_owned(),
                },
                baseline_version: VersionId::new("base:snap:v1")?,
                baseline_probability: Probability::new(0.50)?,
                forecast_probability: Probability::new(0.60)?,
                model_version: VersionId::new("model:snap:v1")?,
                model_knowledge_cutoff_utc_ms: Some(cutoff_ms),
                branches: vec![ConditionalResponseBranch {
                    branch_id: OpaqueId::new("b-snap")?,
                    branch_label: "Primary branch".to_owned(),
                    is_non_response_or_divergent: false,
                    steps: vec![ConditionalResponseStep {
                        step_number: 1,
                        actor_sector: PrecedentSector::Government,
                        action_description: "Issue directive".to_owned(),
                        conditional_probability: Probability::new(step_prob)?,
                        dependency_rationale: "Trigger".to_owned(),
                        expected_step_window_ms: Some((DAY_MS, 3 * DAY_MS)),
                    }],
                    chained_probability: Probability::new(step_prob)?,
                    expected_elapsed_range_ms: None,
                    supporting_episode_versions: vec![VersionId::new("ep-chain-a:v1")?],
                }],
                indicators: vec![MonitoringIndicator {
                    indicator_id: OpaqueId::new("ind-snap")?,
                    observable_signal: signal.to_owned(),
                    target_branch_id: OpaqueId::new("b-snap")?,
                    direction: IndicatorDirection::ConfirmsBranch,
                    source_coverage_limits: "Public register".to_owned(),
                    false_positive_caveat: "Routine notices".to_owned(),
                }],
                justification: PredictionJustification::JustifiedQuantitativeForecast {
                    calibration_and_adjustment_rationale: "Calibrated from connected cluster"
                        .to_owned(),
                    sparse_history_override_rationale: Some(
                        "Explicit sparse-cluster override".to_owned(),
                    ),
                },
                scenario_sources: Vec::new(),
                assumptions: Vec::new(),
                predecessor_version: None,
                revision_rationale: None,
            })
        };

    let mut ledger_a = ForecastLedger::new(16)?;
    let mut ledger_b = ForecastLedger::new(16)?;
    let rec_a = comp_set.issue_forecast(&mut ledger_a, make_spec(0.50, "Signal Alpha")?)?;
    let rec_b = comp_set.issue_forecast(&mut ledger_b, make_spec(0.90, "Signal Beta")?)?;

    assert!(rec_a.analogue_scenario.is_some());
    assert_ne!(rec_a, rec_b);
    let snap_a = rec_a
        .analogue_scenario
        .as_ref()
        .ok_or("missing analogue_scenario")?;
    assert_eq!(
        snap_a.comparison_set_version,
        VersionId::new("cs-chain:v2")?
    );
    assert_eq!(
        snap_a.branches[0].chained_probability,
        Probability::new(0.50)?
    );
    assert_eq!(snap_a.indicators[0].observable_signal, "Signal Alpha");

    Ok(())
}
