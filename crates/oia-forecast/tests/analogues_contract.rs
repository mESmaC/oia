//! Focused contract tests for historical analogue retrieval, point-in-time precedent
//! knowability, episode cluster independence, conditional response branches, and `A15`
//! forecast lifecycle integration (`FORECAST-01`..=`FORECAST-04`, `#45`).

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
    OutcomeResolutionRecord, ResolutionState, RevisionRationale,
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
/// 4. Issuing an initial forecast (`v1`), revising it (`v2`) on new indicator evidence, and
///    resolving the outcome preserves both vintages and reports exact precedent timing error.
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
    })?;

    // Episodes 2a and 2b: Two supportive episodes in the SAME shared crisis cluster (elapsed = 6d and 8d).
    // Raw supportive count increases by 2, but independent supportive cluster count must increase by 1.
    let t2 = T0_MS + 15 * DAY_MS;
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-cluster-primary")?,
        version_id: VersionId::new("ep-cluster-primary:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:a")?,
        title: "Cluster primary supportive episode".to_owned(),
        initiating_condition: "Coordinated coastal corridor anomaly".to_owned(),
        initiating_event_utc_ms: t2,
        initiation_evidence_versions: vec![VersionId::new("src:ep2-init:v1")?],
        profile: base_profile.clone(),
        dependency: EpisodeDependencyKind::SharedCrisisOrCampaign {
            cluster_id: OpaqueId::new("cluster-2021-wave")?,
            dependency_note: "Primary sector in 2021 wave".to_owned(),
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
    })?;

    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-cluster-spillover")?,
        version_id: VersionId::new("ep-cluster-spillover:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:a")?,
        title: "Cluster spillover supportive episode".to_owned(),
        initiating_condition: "Adjacent sector spillover".to_owned(),
        initiating_event_utc_ms: t2 + DAY_MS,
        initiation_evidence_versions: vec![VersionId::new("src:ep2-init:v1")?],
        profile: base_profile.clone(),
        dependency: EpisodeDependencyKind::PolicySpilloverOrRetaliation {
            cluster_id: OpaqueId::new("cluster-2021-wave")?,
            predecessor_episode_id: OpaqueId::new("ep-cluster-primary")?,
            dependency_note: "Followed primary sector directive".to_owned(),
        },
        response_steps: Vec::new(),
        outcome: PrecedentOutcomeDisposition::TargetResponseOccurred {
            outcome_summary: "Mirrored NOTAM directive".to_owned(),
            resolved_at_utc_ms: t2 + 9 * DAY_MS,
            elapsed_from_initiation_ms: 8 * DAY_MS,
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
    })?;

    // Episode 3: Documented non-response counterexample.
    let t3 = T0_MS + 35 * DAY_MS;
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-doc-non-response")?,
        version_id: VersionId::new("ep-doc-non-response:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:a")?,
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
    })?;

    // Episode 4: Divergent response counterexample (under a different institutional regime, 6/7 match).
    let t4 = T0_MS + 60 * DAY_MS;
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-divergent")?,
        version_id: VersionId::new("ep-divergent:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:a")?,
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
    })?;

    // Episode 5: Initiated before cutoff (day 95) but outcome occurred at day 104 (after day 100 cutoff).
    // Must be classified as InitiatedButOutcomeCensoredAtCutoff at day 100!
    let t5 = T0_MS + 95 * DAY_MS;
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-post-cutoff-outcome")?,
        version_id: VersionId::new("ep-post-cutoff-outcome:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:a")?,
        title: "Episode resolving after forecast cutoff".to_owned(),
        initiating_condition: "Late-autumn corridor anomaly".to_owned(),
        initiating_event_utc_ms: t5,
        initiation_evidence_versions: vec![VersionId::new("src:ep5-init:v1")?],
        profile: base_profile.clone(),
        dependency: EpisodeDependencyKind::IndependentEpisode,
        response_steps: Vec::new(),
        outcome: PrecedentOutcomeDisposition::TargetResponseOccurred {
            outcome_summary: "Resolved on day 104 after day 100 cutoff".to_owned(),
            resolved_at_utc_ms: T0_MS + 104 * DAY_MS,
            elapsed_from_initiation_ms: 9 * DAY_MS,
            outcome_evidence_versions: vec![VersionId::new("src:ep5-late-out:v1")?],
        },
        sources: vec![
            source_at("src:ep5-init:v1", t5, t5 + 1_000, t5 + 2_000)?,
            source_at(
                "src:ep5-late-out:v1",
                T0_MS + 104 * DAY_MS,
                T0_MS + 104 * DAY_MS + 1_000,
                T0_MS + 104 * DAY_MS + 2_000,
            )?,
        ],
    })?;

    // Episode 6: Initiated after day 100 cutoff (day 110) -> must be rejected as InadmissibleAtCutoff.
    let t6 = T0_MS + 110 * DAY_MS;
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-future-inadmissible")?,
        version_id: VersionId::new("ep-future-inadmissible:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:a")?,
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
    })?;

    let cutoff_v1_ms = T0_MS + 100 * DAY_MS;
    let mut comp_set = lib.retrieve_comparison_set(AnalogueQueryCriteria {
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
    assert_eq!(comp_set.inadmissible_episode_ids.len(), 1);
    assert_eq!(
        comp_set.inadmissible_episode_ids[0].0,
        OpaqueId::new("ep-future-inadmissible")?
    );
    assert_eq!(comp_set.active_entries.len(), 6);

    // Verify Episode 5 was right-censored at cutoff_v1_ms and did NOT include `src:ep5-late-out:v1`.
    let ep5_entry = comp_set
        .active_entries
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
    // - 3 supportive episodes across 2 independent clusters (`ep-ind-support-1` and `cluster-2021-wave`)
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

    // Issue original forecast vintage (v1), then revise (v2), then resolve and evaluate.
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

    comp_set.issue_forecast(
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
    )?;

    // Revise forecast 2 days later when indicator triggers.
    let cutoff_v2_ms = cutoff_v1_ms + 2 * DAY_MS;
    comp_set.information_cutoff_utc_ms = cutoff_v2_ms;
    comp_set.issue_forecast(
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
                calibration_and_adjustment_rationale: "Step 1 docket indicator observed".to_owned(),
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
    )?;

    // Resolve event at scenario_init + 7 days (1 day slower than 6-day precedent median, inside [4d, 8d]).
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

    let eval = ledger.evaluate_cohort(
        VersionId::new("eval:a15:v1")?,
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

    // Excluding a counterexample retains it in `excluded_entries` and flags transferability.
    comp_set.apply_edit(
        &lib,
        ComparisonSetEdit {
            edit_version: VersionId::new("cs-a15:v2")?,
            actor: ActorId::new("analyst:1")?,
            edited_at_utc_ms: cutoff_v2_ms + 2_000,
            action: ComparisonEditAction::ExcludeEpisode {
                episode_id: OpaqueId::new("ep-divergent")?,
                rationale: "Statutory regime differed from regulatory authority".to_owned(),
            },
        },
    )?;
    let freq_after = comp_set.summarize_frequencies();
    assert_eq!(freq_after.excluded_episode_count, 1);
    assert_eq!(freq_after.excluded_counterexample_count, 1);
    assert_eq!(
        comp_set.assess_transferability(),
        TransferabilityAssessment::SparseOrPoorlyTransferable {
            reasons: vec![
                TransferLimitationReason::SparseIndependentPrecedents {
                    independent_resolved_clusters: 3,
                    minimum_recommended: 4,
                },
                TransferLimitationReason::ExcludedCounterexamplesPresent {
                    excluded_counterexample_count: 1,
                },
            ],
        }
    );

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
    })?;

    let t_future = T0_MS + 50 * DAY_MS;
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-future")?,
        version_id: VersionId::new("ep-future:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:a")?,
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
