//! End-to-end synthetic demonstration of the `A15` historical-analogue forecasting workflow
//! (`FORECAST-01`..=`FORECAST-04`, `#45`).
//!
//! Exercises:
//! 1. Sourced historical precedent episodes across government/regulatory contexts, including
//!    comparable responses, a divergent response, a documented non-response, a shared-crisis
//!    dependent episode, an inherently censored episode, an episode whose outcome only resolved
//!    *after* the forecast cutoff (right-censored at cutoff), and a post-cutoff episode.
//! 2. Multi-dimension analogue retrieval, similarity/difference inspection, and auditable
//!    comparison-set edits (`IncludeEpisode`, `ExcludeEpisode`, `AnnotateTransferLimits`).
//! 3. Uncalibrated historical frequency summaries separating raw episode counts from
//!    independent precedent cluster counts, plus transferability assessment.
//! 4. Conditional multi-step response branches, monitoring indicators, initial forecast
//!    issuance into [`ForecastLedger`], linked revision after new indicator evidence arrives,
//!    outcome resolution, timing-error evaluation against the precedent elapsed-time band,
//!    and cohort evaluation preserving both the original and revised vintages.

use std::error::Error;

use oia_domain::{ActorId, OpaqueId, SchemaVersion, VersionId};
use oia_forecast::analogues::{
    ActorResponseStep, AnalogueComparisonSet, AnalogueForecastIssuanceSpec, AnalogueProfile,
    AnalogueQueryCriteria, ComparisonEditAction, ComparisonSetEdit, ConditionalResponseBranch,
    ConditionalResponseStep, EconomicCondition, EpisodeDependencyKind, EventScale,
    IndicatorDirection, InformationQuality, InstitutionalRegime, MonitoringIndicator,
    PrecedentEpisodeRecord, PrecedentLibrary, PrecedentOutcomeDisposition, PrecedentSector,
    PredictionJustification, TemporalSequenceRelation, TensionLevel,
};
use oia_forecast::issuance::{
    CutoffAdmissibilityMode, CutoffSourceInput, ForecastLedger, OutcomeDefinition,
    OutcomeResolutionRecord, ResolutionState, RevisionRationale,
};
use oia_forecast::scoring::Probability;

const DAY_MS: i64 = 86_400_000;
const BASE_T0_MS: i64 = 1_700_000_000_000;

fn main() -> Result<(), Box<dyn Error>> {
    println!("=== OIA A15 Historical Analogue Forecasting Demo (SYNTHETIC FIXTURE) ===");

    let mut library = build_synthetic_precedent_library()?;
    println!(
        "Registered {} precedent episodes in library.",
        library.len()
    );

    let cutoff_v1_ms = BASE_T0_MS + 100 * DAY_MS;
    let target_profile = AnalogueProfile {
        event_scale: EventScale::Regional,
        geography_region: "North Atlantic Corridor".to_owned(),
        actor_sector: PrecedentSector::Government,
        actor_profile: "National Civil Aviation & Airspace Authority".to_owned(),
        institutional_regime: InstitutionalRegime::RegulatoryAgency,
        tension_level: TensionLevel::Elevated,
        economic_condition: EconomicCondition::InflationaryOrSupplyStress,
        information_quality: InformationQuality::HighVerifiedPrimary,
    };

    let mut comparison_set: AnalogueComparisonSet =
        library.retrieve_comparison_set(AnalogueQueryCriteria {
            comparison_set_id: OpaqueId::new("comp-set-airspace-a15")?,
            initial_version_id: VersionId::new("comp-set-airspace-a15:v1")?,
            actor: ActorId::new("analyst:lead")?,
            created_at_utc_ms: cutoff_v1_ms,
            information_cutoff_utc_ms: cutoff_v1_ms,
            cutoff_mode: CutoffAdmissibilityMode::ProspectiveStrict,
            target_profile,
            min_matched_dimensions: 5,
            min_independent_clusters: 3,
        })?;

    println!(
        "Initial comparison set retrieved: {} active episodes, {} inadmissible at cutoff.",
        comparison_set.active_entries.len(),
        comparison_set.inadmissible_episode_ids.len()
    );

    // Apply an auditable comparison-set edit annotating institutional transfer limits.
    comparison_set.apply_edit(
        &library,
        ComparisonSetEdit {
            edit_version: VersionId::new("comp-set-airspace-a15:v2")?,
            actor: ActorId::new("analyst:lead")?,
            edited_at_utc_ms: cutoff_v1_ms + 1_000,
            action: ComparisonEditAction::AnnotateTransferLimits {
                episode_id: OpaqueId::new("ep-2021-corridor-directive")?,
                institutional_difference_note: Some(
                    "Operated under emergency executive coordination rather than ordinary statutory rulemaking."
                        .to_owned(),
                ),
                additional_limitations: vec![
                    "Higher radar coverage density in 2021 coastal sector.".to_owned(),
                ],
                rationale: "Document radar density difference before issuing initial forecast."
                    .to_owned(),
            },
        },
    )?;

    let summary = comparison_set.summarize_frequencies();
    let transferability = comparison_set.assess_transferability();
    println!(
        "Frequency summary: active={} (independent_clusters={}, dependent={}), resolved_knowable={}, supportive={}, divergent={}, documented_non_response={}, censored_at_cutoff={}",
        summary.active_episode_count,
        summary.independent_cluster_count,
        summary.dependent_episode_count,
        summary.resolved_knowable_episode_count,
        summary.supportive_target_response_count,
        summary.divergent_counterexample_count,
        summary.documented_non_response_count,
        summary.censored_at_cutoff_count,
    );
    println!(
        "Raw resolved support ratio (bps): {:?}, Independent cluster support ratio (bps): {:?}, Calibrated={}",
        summary.raw_resolved_support_ratio_bps,
        summary.independent_cluster_support_ratio_bps,
        summary.is_calibrated_probability
    );
    println!("Caveat: {}", summary.calibration_caveat);
    println!("Transferability: {transferability:?}");

    if let Some(range) = summary.target_elapsed_time_range {
        println!(
            "Precedent elapsed time range (ms): min={}, median={}, max={} (n={})",
            range.min_elapsed_ms,
            range.median_elapsed_ms,
            range.max_elapsed_ms,
            range.resolved_episode_count
        );
    }

    let mut ledger = ForecastLedger::new(100)?;
    let horizon_start_ms = cutoff_v1_ms + DAY_MS;
    let horizon_end_ms = cutoff_v1_ms + 14 * DAY_MS;
    let outcome_def = OutcomeDefinition {
        outcome_id: OpaqueId::new("outcome-airspace-notam-directive")?,
        proposition: "Civil aviation authority issues formal multi-sector sensor-corroboration NOTAM directive within 14 days."
            .to_owned(),
        resolution_rules: "Resolves true if an official public NOTAM directive is published in the authority register inside the target horizon."
            .to_owned(),
    };

    let branches = vec![
        ConditionalResponseBranch {
            branch_id: OpaqueId::new("branch-formal-directive")?,
            branch_label: "Interagency review followed by formal NOTAM directive".to_owned(),
            is_non_response_or_divergent: false,
            steps: vec![
                ConditionalResponseStep {
                    step_number: 1,
                    actor_sector: PrecedentSector::Government,
                    action_description: "Convene technical radar/optical working group".to_owned(),
                    conditional_probability: Probability::new(0.80)?,
                    dependency_rationale:
                        "Triggered by multi-sensor corridor reports under elevated tension."
                            .to_owned(),
                    expected_step_window_ms: Some((DAY_MS, 4 * DAY_MS)),
                },
                ConditionalResponseStep {
                    step_number: 2,
                    actor_sector: PrecedentSector::Government,
                    action_description: "Publish formal sensor-corroboration NOTAM directive"
                        .to_owned(),
                    conditional_probability: Probability::new(0.75)?,
                    dependency_rationale:
                        "Conditional on step 1 technical working group confirming cross-sector tracks."
                            .to_owned(),
                    expected_step_window_ms: Some((4 * DAY_MS, 10 * DAY_MS)),
                },
            ],
            chained_probability: Probability::new(0.60)?,
            expected_elapsed_range_ms: summary.target_elapsed_time_range,
            supporting_episode_versions: vec![
                VersionId::new("ep-2019-corridor:v1")?,
                VersionId::new("ep-2021-corridor-directive:v1")?,
            ],
        },
        ConditionalResponseBranch {
            branch_id: OpaqueId::new("branch-non-response")?,
            branch_label: "Internal logging only without public directive".to_owned(),
            is_non_response_or_divergent: true,
            steps: vec![ConditionalResponseStep {
                step_number: 1,
                actor_sector: PrecedentSector::Government,
                action_description: "Retain internal incident log with no public rulemaking"
                    .to_owned(),
                conditional_probability: Probability::new(0.40)?,
                dependency_rationale:
                    "Consistent with documented non-response counterexample when tracks do not repeat."
                        .to_owned(),
                expected_step_window_ms: Some((DAY_MS, 14 * DAY_MS)),
            }],
            chained_probability: Probability::new(0.40)?,
            expected_elapsed_range_ms: None,
            supporting_episode_versions: vec![VersionId::new("ep-2020-non-response:v1")?],
        },
    ];

    let indicators = vec![MonitoringIndicator {
        indicator_id: OpaqueId::new("ind-working-group-docket")?,
        observable_signal:
            "Publication of an interagency safety docket notice within 96 hours of initiation"
                .to_owned(),
        target_branch_id: OpaqueId::new("branch-formal-directive")?,
        direction: IndicatorDirection::ConfirmsBranch,
        source_coverage_limits: "Covers public civil aviation register only; classified military annexes are not observable."
            .to_owned(),
        false_positive_caveat:
            "Routine equipment-maintenance notices use similar docket prefixes and must be filtered."
                .to_owned(),
    }];

    // Issue Vintage 1 before resolving.
    let v1 = comparison_set.issue_forecast(
        &mut ledger,
        AnalogueForecastIssuanceSpec {
            forecast_id: OpaqueId::new("fc-airspace-a15")?,
            version_id: VersionId::new("fc-airspace-a15:v1")?,
            schema_version: SchemaVersion::new(1)?,
            actor: ActorId::new("analyst:lead")?,
            issued_at_utc_ms: cutoff_v1_ms + 3_600_000,
            origin_utc_ms: cutoff_v1_ms,
            horizon_start_utc_ms: horizon_start_ms,
            horizon_end_utc_ms: horizon_end_ms,
            target_id: OpaqueId::new("target:north-atlantic-authority")?,
            outcome_definition: outcome_def.clone(),
            baseline_version: VersionId::new("baseline:independent-cluster-rate:v1")?,
            baseline_probability: Probability::new(0.50)?,
            forecast_probability: Probability::new(0.60)?,
            model_version: VersionId::new("model:analogue-branch-v1")?,
            model_knowledge_cutoff_utc_ms: Some(cutoff_v1_ms),
            branches: branches.clone(),
            indicators: indicators.clone(),
            justification: PredictionJustification::JustifiedQuantitativeForecast {
                calibration_and_adjustment_rationale:
                    "Independent cluster support ratio is 50% (2/4 resolved clusters); upward adjustment to 0.60 reflects multi-sensor radar confirmation in current scenario."
                        .to_owned(),
                sparse_history_override_rationale: None,
            },
            scenario_sources: vec![CutoffSourceInput {
                source_version: VersionId::new("src:current-corridor-report:v1")?,
                event_utc_ms: Some(cutoff_v1_ms - 12 * 3_600_000),
                public_available_at_utc_ms: Some(cutoff_v1_ms - 6 * 3_600_000),
                ingested_at_utc_ms: Some(cutoff_v1_ms - 3_600_000),
            }],
            assumptions: vec![
                "Current civilian radar feed remains operational through the 14-day window."
                    .to_owned(),
            ],
            predecessor_version: None,
            revision_rationale: None,
        },
    )?;
    println!(
        "Issued original forecast vintage: {} (p={:.2}, comparison_set={} episodes)",
        v1.version_id,
        v1.forecast_probability.value(),
        v1.comparison_set.len()
    );

    // Issue Vintage 2 (revision) when the monitored indicator triggers 2 days later.
    let cutoff_v2_ms = cutoff_v1_ms + 2 * DAY_MS;
    comparison_set.information_cutoff_utc_ms = cutoff_v2_ms;
    let v2 = comparison_set.issue_forecast(
        &mut ledger,
        AnalogueForecastIssuanceSpec {
            forecast_id: OpaqueId::new("fc-airspace-a15")?,
            version_id: VersionId::new("fc-airspace-a15:v2")?,
            schema_version: SchemaVersion::new(1)?,
            actor: ActorId::new("analyst:lead")?,
            issued_at_utc_ms: cutoff_v2_ms + 3_600_000,
            origin_utc_ms: cutoff_v2_ms,
            horizon_start_utc_ms: horizon_start_ms,
            horizon_end_utc_ms: horizon_end_ms,
            target_id: OpaqueId::new("target:north-atlantic-authority")?,
            outcome_definition: outcome_def.clone(),
            baseline_version: VersionId::new("baseline:independent-cluster-rate:v1")?,
            baseline_probability: Probability::new(0.50)?,
            forecast_probability: Probability::new(0.78)?,
            model_version: VersionId::new("model:analogue-branch-v1")?,
            model_knowledge_cutoff_utc_ms: Some(cutoff_v2_ms),
            branches,
            indicators,
            justification: PredictionJustification::JustifiedQuantitativeForecast {
                calibration_and_adjustment_rationale:
                    "Step 1 indicator (interagency safety docket notice) confirmed at +48h; updated to conditional step-2 probability."
                        .to_owned(),
                sparse_history_override_rationale: None,
            },
            scenario_sources: vec![
                CutoffSourceInput {
                    source_version: VersionId::new("src:current-corridor-report:v1")?,
                    event_utc_ms: Some(cutoff_v1_ms - 12 * 3_600_000),
                    public_available_at_utc_ms: Some(cutoff_v1_ms - 6 * 3_600_000),
                    ingested_at_utc_ms: Some(cutoff_v1_ms - 3_600_000),
                },
                CutoffSourceInput {
                    source_version: VersionId::new("src:docket-notice-step1:v1")?,
                    event_utc_ms: Some(cutoff_v2_ms - 4 * 3_600_000),
                    public_available_at_utc_ms: Some(cutoff_v2_ms - 2 * 3_600_000),
                    ingested_at_utc_ms: Some(cutoff_v2_ms - 3_600_000),
                },
            ],
            assumptions: vec![
                "Working group docket notice confirmed Step 1 of branch-formal-directive."
                    .to_owned(),
            ],
            predecessor_version: Some(VersionId::new("fc-airspace-a15:v1")?),
            revision_rationale: Some(RevisionRationale {
                new_evidence_versions: vec![VersionId::new("src:docket-notice-step1:v1")?],
                summary: "Monitored indicator ind-working-group-docket triggered on schedule."
                    .to_owned(),
            }),
        },
    )?;
    println!(
        "Issued revised forecast vintage: {} (p={:.2}, vintage_number={})",
        v2.version_id,
        v2.forecast_probability.value(),
        v2.vintage_number
    );

    // Resolve outcome at +6 days from scenario initiation.
    let scenario_init_ms = cutoff_v1_ms - 12 * 3_600_000;
    let observed_event_ms = scenario_init_ms + 6 * DAY_MS;
    let resolution = ledger
        .record_outcome_resolution(OutcomeResolutionRecord {
            resolution_id: OpaqueId::new("res-airspace-a15")?,
            version_id: VersionId::new("res-airspace-a15:v1")?,
            outcome_id: outcome_def.outcome_id,
            actor: ActorId::new("analyst:reviewer")?,
            recorded_at_utc_ms: observed_event_ms + 3_600_000,
            evidence_versions: vec![VersionId::new("src:published-notam-directive:v1")?],
            state: ResolutionState::Resolved {
                occurred: true,
                observed_event_utc_ms: Some(observed_event_ms),
            },
            supersedes_version: None,
            correction_rationale: None,
        })?
        .clone();

    let timing_eval = comparison_set.evaluate_resolution_timing(scenario_init_ms, &resolution)?;
    println!("Timing evaluation against precedent band: {timing_eval:?}");

    let eval_run = ledger.evaluate_cohort(
        VersionId::new("eval:airspace-a15:v1")?,
        observed_event_ms + 7_200_000,
        &[
            (
                VersionId::new("fc-airspace-a15:v1")?,
                VersionId::new("res-airspace-a15:v1")?,
            ),
            (
                VersionId::new("fc-airspace-a15:v2")?,
                VersionId::new("res-airspace-a15:v1")?,
            ),
        ],
        10,
    )?;
    println!(
        "Cohort evaluation preserved {} vintages (resolved={}); mean Brier scores={:?}",
        eval_run.vintages.len(),
        eval_run.resolved_count,
        eval_run.mean_scores
    );

    let _ = &mut library;
    Ok(())
}

fn build_synthetic_precedent_library() -> Result<PrecedentLibrary, Box<dyn Error>> {
    let mut lib = PrecedentLibrary::new(50)?;
    let profile = AnalogueProfile {
        event_scale: EventScale::Regional,
        geography_region: "North Atlantic Corridor".to_owned(),
        actor_sector: PrecedentSector::Government,
        actor_profile: "National Civil Aviation & Airspace Authority".to_owned(),
        institutional_regime: InstitutionalRegime::RegulatoryAgency,
        tension_level: TensionLevel::Elevated,
        economic_condition: EconomicCondition::InflationaryOrSupplyStress,
        information_quality: InformationQuality::HighVerifiedPrimary,
    };

    // 1. Supportive precedent #1 (elapsed: 5 days).
    let t1 = BASE_T0_MS + 5 * DAY_MS;
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-2019-corridor")?,
        version_id: VersionId::new("ep-2019-corridor:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:hist")?,
        title: "2019 Corridor Multi-Radar Advisory".to_owned(),
        initiating_condition: "Primary and secondary radar tracks reported across two coastal FIR sectors."
            .to_owned(),
        initiating_event_utc_ms: t1,
        initiation_evidence_versions: vec![VersionId::new("src:2019-init:v1")?],
        profile: profile.clone(),
        dependency: EpisodeDependencyKind::IndependentEpisode,
        response_steps: vec![ActorResponseStep {
            step_id: OpaqueId::new("ep-2019-step-1")?,
            actor_id: ActorId::new("actor:aviation-authority")?,
            action_summary: "Convened joint radar review panel".to_owned(),
            announced_intention: Some("Assess transponder anomaly within 72 hours".to_owned()),
            action_utc_ms: t1 + 2 * DAY_MS,
            elapsed_from_initiation_ms: 2 * DAY_MS,
            sequence_relation: TemporalSequenceRelation::AssessedCausalLink {
                rationale: "Order explicitly cited the coastal FIR track reports.".to_owned(),
                supporting_evidence_versions: vec![VersionId::new("src:2019-init:v1")?],
            },
            evidence_versions: vec![VersionId::new("src:2019-init:v1")?],
        }],
        outcome: PrecedentOutcomeDisposition::TargetResponseOccurred {
            outcome_summary: "Issued formal multi-sector NOTAM directive.".to_owned(),
            resolved_at_utc_ms: t1 + 5 * DAY_MS,
            elapsed_from_initiation_ms: 5 * DAY_MS,
            outcome_evidence_versions: vec![VersionId::new("src:2019-out:v1")?],
        },
        sources: vec![
            CutoffSourceInput {
                source_version: VersionId::new("src:2019-init:v1")?,
                event_utc_ms: Some(t1),
                public_available_at_utc_ms: Some(t1 + 3_600_000),
                ingested_at_utc_ms: Some(t1 + 7_200_000),
            },
            CutoffSourceInput {
                source_version: VersionId::new("src:2019-out:v1")?,
                event_utc_ms: Some(t1 + 5 * DAY_MS),
                public_available_at_utc_ms: Some(t1 + 5 * DAY_MS + 3_600_000),
                ingested_at_utc_ms: Some(t1 + 5 * DAY_MS + 7_200_000),
            },
        ],
    })?;

    // 2. Supportive precedent #2 (elapsed: 7 days) + dependent spillover episode in the same cluster.
    let t2 = BASE_T0_MS + 20 * DAY_MS;
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-2021-corridor-directive")?,
        version_id: VersionId::new("ep-2021-corridor-directive:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:hist")?,
        title: "2021 Maritime Approach Sensor Directive".to_owned(),
        initiating_condition: "Repeated optical and primary-radar tracks near offshore airway."
            .to_owned(),
        initiating_event_utc_ms: t2,
        initiation_evidence_versions: vec![VersionId::new("src:2021-init:v1")?],
        profile: profile.clone(),
        dependency: EpisodeDependencyKind::SharedCrisisOrCampaign {
            cluster_id: OpaqueId::new("cluster-2021-maritime-wave")?,
            dependency_note: "Primary sector in the 2021 offshore wave.".to_owned(),
        },
        response_steps: Vec::new(),
        outcome: PrecedentOutcomeDisposition::TargetResponseOccurred {
            outcome_summary: "Issued coordinated NOTAM sensor directive.".to_owned(),
            resolved_at_utc_ms: t2 + 7 * DAY_MS,
            elapsed_from_initiation_ms: 7 * DAY_MS,
            outcome_evidence_versions: vec![VersionId::new("src:2021-out:v1")?],
        },
        sources: vec![
            CutoffSourceInput {
                source_version: VersionId::new("src:2021-init:v1")?,
                event_utc_ms: Some(t2),
                public_available_at_utc_ms: Some(t2 + 3_600_000),
                ingested_at_utc_ms: Some(t2 + 7_200_000),
            },
            CutoffSourceInput {
                source_version: VersionId::new("src:2021-out:v1")?,
                event_utc_ms: Some(t2 + 7 * DAY_MS),
                public_available_at_utc_ms: Some(t2 + 7 * DAY_MS + 3_600_000),
                ingested_at_utc_ms: Some(t2 + 7 * DAY_MS + 7_200_000),
            },
        ],
    })?;

    // Dependent spillover episode in `cluster-2021-maritime-wave` (elapsed: 9 days).
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-2021-adjacent-spillover")?,
        version_id: VersionId::new("ep-2021-adjacent-spillover:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:hist")?,
        title: "2021 Adjacent Sector Mirror Directive".to_owned(),
        initiating_condition: "Adjacent FIR adopted the same offshore reporting template."
            .to_owned(),
        initiating_event_utc_ms: t2 + DAY_MS,
        initiation_evidence_versions: vec![VersionId::new("src:2021-init:v1")?],
        profile: profile.clone(),
        dependency: EpisodeDependencyKind::PolicySpilloverOrRetaliation {
            cluster_id: OpaqueId::new("cluster-2021-maritime-wave")?,
            predecessor_episode_id: OpaqueId::new("ep-2021-corridor-directive")?,
            dependency_note: "Mirrored predecessor sector's directive during the same wave."
                .to_owned(),
        },
        response_steps: Vec::new(),
        outcome: PrecedentOutcomeDisposition::TargetResponseOccurred {
            outcome_summary: "Mirrored NOTAM directive in adjacent FIR.".to_owned(),
            resolved_at_utc_ms: t2 + 10 * DAY_MS,
            elapsed_from_initiation_ms: 9 * DAY_MS,
            outcome_evidence_versions: vec![VersionId::new("src:2021-out:v1")?],
        },
        sources: vec![
            CutoffSourceInput {
                source_version: VersionId::new("src:2021-init:v1")?,
                event_utc_ms: Some(t2),
                public_available_at_utc_ms: Some(t2 + 3_600_000),
                ingested_at_utc_ms: Some(t2 + 7_200_000),
            },
            CutoffSourceInput {
                source_version: VersionId::new("src:2021-out:v1")?,
                event_utc_ms: Some(t2 + 7 * DAY_MS),
                public_available_at_utc_ms: Some(t2 + 7 * DAY_MS + 3_600_000),
                ingested_at_utc_ms: Some(t2 + 7 * DAY_MS + 7_200_000),
            },
        ],
    })?;

    // 3. Documented non-response counterexample.
    let t3 = BASE_T0_MS + 40 * DAY_MS;
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-2020-non-response")?,
        version_id: VersionId::new("ep-2020-non-response:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:hist")?,
        title: "2020 Single-Night Corridor Sighting (No Directive)".to_owned(),
        initiating_condition: "Pilot visual reports logged without secondary radar return."
            .to_owned(),
        initiating_event_utc_ms: t3,
        initiation_evidence_versions: vec![VersionId::new("src:2020-init:v1")?],
        profile: profile.clone(),
        dependency: EpisodeDependencyKind::IndependentEpisode,
        response_steps: Vec::new(),
        outcome: PrecedentOutcomeDisposition::DocumentedNonResponse {
            observed_window_end_utc_ms: t3 + 14 * DAY_MS,
            observation_duration_ms: 14 * DAY_MS,
            non_response_rationale:
                "Full 14-day docket review confirmed no public NOTAM directive was issued."
                    .to_owned(),
            outcome_evidence_versions: vec![VersionId::new("src:2020-docket-audit:v1")?],
        },
        sources: vec![
            CutoffSourceInput {
                source_version: VersionId::new("src:2020-init:v1")?,
                event_utc_ms: Some(t3),
                public_available_at_utc_ms: Some(t3 + 3_600_000),
                ingested_at_utc_ms: Some(t3 + 7_200_000),
            },
            CutoffSourceInput {
                source_version: VersionId::new("src:2020-docket-audit:v1")?,
                event_utc_ms: Some(t3 + 14 * DAY_MS),
                public_available_at_utc_ms: Some(t3 + 14 * DAY_MS + 3_600_000),
                ingested_at_utc_ms: Some(t3 + 14 * DAY_MS + 7_200_000),
            },
        ],
    })?;

    // 4. Divergent response counterexample.
    let t4 = BASE_T0_MS + 65 * DAY_MS;
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-2022-divergent-referral")?,
        version_id: VersionId::new("ep-2022-divergent-referral:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:hist")?,
        title: "2022 Meteorological Balloon Referral".to_owned(),
        initiating_condition: "High-altitude slow-moving return in northern corridor.".to_owned(),
        initiating_event_utc_ms: t4,
        initiation_evidence_versions: vec![VersionId::new("src:2022-init:v1")?],
        profile: profile.clone(),
        dependency: EpisodeDependencyKind::IndependentEpisode,
        response_steps: Vec::new(),
        outcome: PrecedentOutcomeDisposition::DivergentResponseOccurred {
            divergent_action_summary:
                "Referred track to meteorological service without airspace NOTAM directive."
                    .to_owned(),
            resolved_at_utc_ms: t4 + 4 * DAY_MS,
            elapsed_from_initiation_ms: 4 * DAY_MS,
            outcome_evidence_versions: vec![VersionId::new("src:2022-out:v1")?],
        },
        sources: vec![
            CutoffSourceInput {
                source_version: VersionId::new("src:2022-init:v1")?,
                event_utc_ms: Some(t4),
                public_available_at_utc_ms: Some(t4 + 3_600_000),
                ingested_at_utc_ms: Some(t4 + 7_200_000),
            },
            CutoffSourceInput {
                source_version: VersionId::new("src:2022-out:v1")?,
                event_utc_ms: Some(t4 + 4 * DAY_MS),
                public_available_at_utc_ms: Some(t4 + 4 * DAY_MS + 3_600_000),
                ingested_at_utc_ms: Some(t4 + 4 * DAY_MS + 7_200_000),
            },
        ],
    })?;

    // 5. Episode that initiated before cutoff (day 95) but resolved AFTER cutoff_v1_ms (day 105).
    // At cutoff_v1_ms (day 100), this episode must be classified as CensoredAtCutoff!
    let t5 = BASE_T0_MS + 95 * DAY_MS;
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-2023-in-flight-at-cutoff")?,
        version_id: VersionId::new("ep-2023-in-flight-at-cutoff:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:hist")?,
        title: "2023 Late-Resolving Sector Review".to_owned(),
        initiating_condition: "Corridor anomaly under active review at day 100 cutoff.".to_owned(),
        initiating_event_utc_ms: t5,
        initiation_evidence_versions: vec![VersionId::new("src:2023-init:v1")?],
        profile,
        dependency: EpisodeDependencyKind::IndependentEpisode,
        response_steps: Vec::new(),
        outcome: PrecedentOutcomeDisposition::TargetResponseOccurred {
            outcome_summary: "Resolved on day 105 (after day 100 forecast cutoff).".to_owned(),
            resolved_at_utc_ms: BASE_T0_MS + 105 * DAY_MS,
            elapsed_from_initiation_ms: 10 * DAY_MS,
            outcome_evidence_versions: vec![VersionId::new("src:2023-post-cutoff-out:v1")?],
        },
        sources: vec![
            CutoffSourceInput {
                source_version: VersionId::new("src:2023-init:v1")?,
                event_utc_ms: Some(t5),
                public_available_at_utc_ms: Some(t5 + 3_600_000),
                ingested_at_utc_ms: Some(t5 + 7_200_000),
            },
            CutoffSourceInput {
                source_version: VersionId::new("src:2023-post-cutoff-out:v1")?,
                event_utc_ms: Some(BASE_T0_MS + 105 * DAY_MS),
                public_available_at_utc_ms: Some(BASE_T0_MS + 105 * DAY_MS + 3_600_000),
                ingested_at_utc_ms: Some(BASE_T0_MS + 105 * DAY_MS + 7_200_000),
            },
        ],
    })?;

    Ok(lib)
}
