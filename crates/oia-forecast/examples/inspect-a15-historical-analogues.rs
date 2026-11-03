//! End-to-end synthetic demonstration of the `A15` historical-analogue forecasting workflow
//! (`FORECAST-01`..=`FORECAST-04`, `#45`).
//!
//! Exercises:
//! 1. Sourced synthetic historical precedent episodes across government/regulatory contexts,
//!    including comparable responses, a divergent response, a documented non-response, a
//!    shared-crisis dependent episode, an episode whose outcome only resolved *after* the initial
//!    forecast cutoff (right-censored at cutoff), a cross-regional episode (`4/7` match) below the
//!    automatic retrieval threshold, and a post-cutoff episode rejected as inadmissible at cutoff.
//! 2. Multi-dimension analogue retrieval, similarity/difference inspection, and auditable
//!    comparison-set edits (`IncludeEpisode`, `ExcludeEpisode`, `AnnotateTransferLimits`, and
//!    `AdvanceCutoff` via `re_retrieve_at_cutoff`).
//! 3. Uncalibrated historical frequency summaries separating raw episode counts from
//!    independent precedent cluster counts (including mixed-outcome cluster accounting), plus
//!    transferability assessment.
//! 4. Conditional multi-step response branches, monitoring indicators, initial forecast
//!    issuance into [`ForecastLedger`], linked revision after re-retrieving the comparison set at
//!    the new cutoff when new indicator evidence arrives, outcome resolution, timing-error
//!    evaluation against the precedent elapsed-time band, cohort evaluation + reliability reporting,
//!    and precedent episode correction flagging dependent evaluations for reassessment.

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
        "Registered {} synthetic precedent episodes in library.",
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
        "Initial comparison set retrieved ({}): {} active episodes, {} inadmissible at cutoff.",
        comparison_set.current_version_id(),
        comparison_set.active_entries().len(),
        comparison_set.inadmissible_episode_ids().len()
    );

    // 1. Manually include a 4/7 cross-regional episode via `IncludeEpisode`, then exclude it via
    // `ExcludeEpisode` so the edit history exercises all edit actions and retains the audit trail.
    comparison_set.apply_edit(
        &library,
        ComparisonSetEdit {
            edit_version: VersionId::new("comp-set-airspace-a15:v2")?,
            actor: ActorId::new("analyst:lead")?,
            edited_at_utc_ms: cutoff_v1_ms + 500,
            action: ComparisonEditAction::IncludeEpisode {
                episode_id: OpaqueId::new("ep-synth-cross-region-low-match")?,
                rationale:
                    "Inspect cross-regional municipal port precedent before finalizing scope."
                        .to_owned(),
                transfer_limitations: vec![
                    "Different geography, sector, and statutory regime.".to_owned(),
                ],
            },
        },
    )?;
    comparison_set.apply_edit(
        &library,
        ComparisonSetEdit {
            edit_version: VersionId::new("comp-set-airspace-a15:v3")?,
            actor: ActorId::new("analyst:lead")?,
            edited_at_utc_ms: cutoff_v1_ms + 800,
            action: ComparisonEditAction::ExcludeEpisode {
                episode_id: OpaqueId::new("ep-synth-cross-region-low-match")?,
                rationale:
                    "Exclude municipal port precedent after review; keep in excluded audit trail."
                        .to_owned(),
            },
        },
    )?;

    // 2. Apply an auditable comparison-set edit annotating institutional transfer limits.
    comparison_set.apply_edit(
        &library,
        ComparisonSetEdit {
            edit_version: VersionId::new("comp-set-airspace-a15:v4")?,
            actor: ActorId::new("analyst:lead")?,
            edited_at_utc_ms: cutoff_v1_ms + 1_000,
            action: ComparisonEditAction::AnnotateTransferLimits {
                episode_id: OpaqueId::new("ep-synth-day20-corridor-directive")?,
                institutional_difference_note: Some(
                    "Operated under joint coastal coordination protocol in addition to ordinary rulemaking."
                        .to_owned(),
                ),
                additional_limitations: vec![
                    "Higher radar coverage density in day-20 synthetic coastal sector.".to_owned(),
                ],
                rationale: "Document radar density difference before issuing initial forecast."
                    .to_owned(),
            },
        },
    )?;

    let summary = comparison_set.summarize_frequencies();
    let transferability = comparison_set.assess_transferability();
    println!(
        "Frequency summary: active={} (independent_clusters={}, dependent={}, mixed_clusters={}), resolved_knowable={}, supportive={}, divergent={}, documented_non_response={}, censored_at_cutoff={}, excluded={}",
        summary.active_episode_count,
        summary.independent_cluster_count,
        summary.dependent_episode_count,
        summary.mixed_outcome_cluster_count,
        summary.resolved_knowable_episode_count,
        summary.supportive_target_response_count,
        summary.divergent_counterexample_count,
        summary.documented_non_response_count,
        summary.censored_at_cutoff_count,
        summary.excluded_episode_count,
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
                VersionId::new("ep-synth-day05-corridor:v1")?,
                VersionId::new("ep-synth-day20-corridor-directive:v1")?,
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
            supporting_episode_versions: vec![VersionId::new("ep-synth-day40-non-response:v1")?],
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
                sparse_history_override_rationale: Some(
                    "Excluded 4/7 cross-regional municipal port case leaves 4 matched North Atlantic aviation clusters."
                        .to_owned(),
                ),
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
        "Issued original forecast vintage: {} (p={:.2}, pinned comparison_set versions={})",
        v1.version_id,
        v1.forecast_probability.value(),
        v1.comparison_set.len()
    );

    // Issue Vintage 2 (revision) by re-retrieving the comparison set at `cutoff_v2_ms` (+2 days),
    // producing a new comparison-set version (`comp-set-airspace-a15:v5`) with an auditable `AdvanceCutoff` edit.
    let cutoff_v2_ms = cutoff_v1_ms + 2 * DAY_MS;
    let comparison_set_v2 = comparison_set.re_retrieve_at_cutoff(
        &library,
        VersionId::new("comp-set-airspace-a15:v5")?,
        ActorId::new("analyst:lead")?,
        cutoff_v2_ms,
        cutoff_v2_ms,
        "Advance comparison-set cutoff by 48h upon Step-1 safety docket trigger.".to_owned(),
    )?;
    let v2 = comparison_set_v2.issue_forecast(
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
                sparse_history_override_rationale: Some(
                    "Excluded 4/7 cross-regional municipal port case leaves 4 matched North Atlantic aviation clusters."
                        .to_owned(),
                ),
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
        "Issued revised forecast vintage: {} (p={:.2}, vintage_number={}, comparison_set_version={})",
        v2.version_id,
        v2.forecast_probability.value(),
        v2.vintage_number,
        comparison_set_v2.current_version_id()
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

    let timing_eval =
        comparison_set_v2.evaluate_resolution_timing(scenario_init_ms, &resolution)?;
    println!("Timing evaluation against precedent band: {timing_eval:?}");

    let eval_ver = VersionId::new("eval:airspace-a15:v1")?;
    let eval_run = ledger.evaluate_cohort(
        eval_ver.clone(),
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

    let rel_report = ledger.reliability_report(&eval_ver, 5, 10)?;
    println!(
        "Reliability report generated across {} bins for {} evaluated rows.",
        rel_report.forecast_bins.len(),
        rel_report.rows.len()
    );

    // Record a later precedent correction and verify that `eval:airspace-a15:v1` is flagged
    // for reassessment without mutating the issued vintages (`B3`).
    let mut corrected_ep1 = library
        .get(&OpaqueId::new("ep-synth-day05-corridor")?)
        .ok_or("missing ep-synth-day05-corridor")?
        .clone();
    corrected_ep1.version_id = VersionId::new("ep-synth-day05-corridor:v2")?;
    corrected_ep1.recorded_at_utc_ms = observed_event_ms + 10_800_000;
    corrected_ep1.supersedes_version = Some(VersionId::new("ep-synth-day05-corridor:v1")?);
    corrected_ep1.correction_rationale = Some(
        "Archival erratum updated elapsed time of synthetic day-05 precedent from 5d to 6d."
            .to_owned(),
    );
    let (_, affected_vintages) = library.record_episode_correction(corrected_ep1, &mut ledger)?;
    let updated_eval = ledger
        .evaluation(&eval_ver)
        .ok_or("missing evaluation after precedent correction")?;
    println!(
        "Precedent correction recorded: affected_vintages={affected_vintages:?}, evaluation needs_reassessment={}, reasons={:?}",
        updated_eval.needs_reassessment, updated_eval.reassessment_reasons
    );

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

    // 1. Supportive synthetic precedent #1 (elapsed: 5 days).
    let t1 = BASE_T0_MS + 5 * DAY_MS;
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-synth-day05-corridor")?,
        version_id: VersionId::new("ep-synth-day05-corridor:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:hist")?,
        recorded_at_utc_ms: t1 + 6 * DAY_MS,
        title: "Synthetic Day-05 Corridor Multi-Radar Advisory".to_owned(),
        initiating_condition:
            "Primary and secondary radar tracks reported across two synthetic coastal FIR sectors."
                .to_owned(),
        initiating_event_utc_ms: t1,
        initiation_evidence_versions: vec![VersionId::new("src:synth-d05-init:v1")?],
        profile: profile.clone(),
        dependency: EpisodeDependencyKind::IndependentEpisode,
        response_steps: vec![ActorResponseStep {
            step_id: OpaqueId::new("ep-synth-d05-step-1")?,
            actor_id: ActorId::new("actor:aviation-authority")?,
            action_summary: "Convened joint radar review panel".to_owned(),
            announced_intention: Some("Assess transponder anomaly within 72 hours".to_owned()),
            action_utc_ms: t1 + 2 * DAY_MS,
            elapsed_from_initiation_ms: 2 * DAY_MS,
            sequence_relation: TemporalSequenceRelation::AssessedCausalLink {
                rationale: "Order explicitly cited the coastal FIR track reports.".to_owned(),
                supporting_evidence_versions: vec![VersionId::new("src:synth-d05-init:v1")?],
            },
            evidence_versions: vec![VersionId::new("src:synth-d05-init:v1")?],
        }],
        outcome: PrecedentOutcomeDisposition::TargetResponseOccurred {
            outcome_summary: "Issued formal multi-sector NOTAM directive.".to_owned(),
            resolved_at_utc_ms: t1 + 5 * DAY_MS,
            elapsed_from_initiation_ms: 5 * DAY_MS,
            outcome_evidence_versions: vec![VersionId::new("src:synth-d05-out:v1")?],
        },
        sources: vec![
            CutoffSourceInput {
                source_version: VersionId::new("src:synth-d05-init:v1")?,
                event_utc_ms: Some(t1),
                public_available_at_utc_ms: Some(t1 + 3_600_000),
                ingested_at_utc_ms: Some(t1 + 7_200_000),
            },
            CutoffSourceInput {
                source_version: VersionId::new("src:synth-d05-out:v1")?,
                event_utc_ms: Some(t1 + 5 * DAY_MS),
                public_available_at_utc_ms: Some(t1 + 5 * DAY_MS + 3_600_000),
                ingested_at_utc_ms: Some(t1 + 5 * DAY_MS + 7_200_000),
            },
        ],
        supersedes_version: None,
        correction_rationale: None,
    })?;

    // 2. Supportive synthetic precedent #2 (elapsed: 7 days) + dependent spillover episode in the same cluster.
    let t2 = BASE_T0_MS + 20 * DAY_MS;
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-synth-day20-corridor-directive")?,
        version_id: VersionId::new("ep-synth-day20-corridor-directive:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:hist")?,
        recorded_at_utc_ms: t2 + 8 * DAY_MS,
        title: "Synthetic Day-20 Maritime Approach Sensor Directive".to_owned(),
        initiating_condition: "Repeated optical and primary-radar tracks near offshore airway."
            .to_owned(),
        initiating_event_utc_ms: t2,
        initiation_evidence_versions: vec![VersionId::new("src:synth-d20-init:v1")?],
        profile: profile.clone(),
        dependency: EpisodeDependencyKind::SharedCrisisOrCampaign {
            cluster_id: OpaqueId::new("cluster-synth-maritime-wave")?,
            dependency_note: "Primary sector in the synthetic day-20 offshore wave.".to_owned(),
        },
        response_steps: Vec::new(),
        outcome: PrecedentOutcomeDisposition::TargetResponseOccurred {
            outcome_summary: "Issued coordinated NOTAM sensor directive.".to_owned(),
            resolved_at_utc_ms: t2 + 7 * DAY_MS,
            elapsed_from_initiation_ms: 7 * DAY_MS,
            outcome_evidence_versions: vec![VersionId::new("src:synth-d20-out:v1")?],
        },
        sources: vec![
            CutoffSourceInput {
                source_version: VersionId::new("src:synth-d20-init:v1")?,
                event_utc_ms: Some(t2),
                public_available_at_utc_ms: Some(t2 + 3_600_000),
                ingested_at_utc_ms: Some(t2 + 7_200_000),
            },
            CutoffSourceInput {
                source_version: VersionId::new("src:synth-d20-out:v1")?,
                event_utc_ms: Some(t2 + 7 * DAY_MS),
                public_available_at_utc_ms: Some(t2 + 7 * DAY_MS + 3_600_000),
                ingested_at_utc_ms: Some(t2 + 7 * DAY_MS + 7_200_000),
            },
        ],
        supersedes_version: None,
        correction_rationale: None,
    })?;

    // Dependent spillover episode in `cluster-synth-maritime-wave` (elapsed: 9 days).
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-synth-day21-adjacent-spillover")?,
        version_id: VersionId::new("ep-synth-day21-adjacent-spillover:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:hist")?,
        recorded_at_utc_ms: t2 + 11 * DAY_MS,
        title: "Synthetic Day-21 Adjacent Sector Mirror Directive".to_owned(),
        initiating_condition: "Adjacent FIR adopted the same offshore reporting template."
            .to_owned(),
        initiating_event_utc_ms: t2 + DAY_MS,
        initiation_evidence_versions: vec![VersionId::new("src:synth-d20-init:v1")?],
        profile: profile.clone(),
        dependency: EpisodeDependencyKind::PolicySpilloverOrRetaliation {
            cluster_id: OpaqueId::new("cluster-synth-maritime-wave")?,
            predecessor_episode_id: OpaqueId::new("ep-synth-day20-corridor-directive")?,
            dependency_note: "Mirrored predecessor sector's directive during the same wave."
                .to_owned(),
        },
        response_steps: Vec::new(),
        outcome: PrecedentOutcomeDisposition::TargetResponseOccurred {
            outcome_summary: "Mirrored NOTAM directive in adjacent FIR.".to_owned(),
            resolved_at_utc_ms: t2 + 10 * DAY_MS,
            elapsed_from_initiation_ms: 9 * DAY_MS,
            outcome_evidence_versions: vec![VersionId::new("src:synth-d21-out:v1")?],
        },
        sources: vec![
            CutoffSourceInput {
                source_version: VersionId::new("src:synth-d20-init:v1")?,
                event_utc_ms: Some(t2),
                public_available_at_utc_ms: Some(t2 + 3_600_000),
                ingested_at_utc_ms: Some(t2 + 7_200_000),
            },
            CutoffSourceInput {
                source_version: VersionId::new("src:synth-d21-out:v1")?,
                event_utc_ms: Some(t2 + 10 * DAY_MS),
                public_available_at_utc_ms: Some(t2 + 10 * DAY_MS + 3_600_000),
                ingested_at_utc_ms: Some(t2 + 10 * DAY_MS + 7_200_000),
            },
        ],
        supersedes_version: None,
        correction_rationale: None,
    })?;

    // 3. Documented non-response counterexample.
    let t3 = BASE_T0_MS + 40 * DAY_MS;
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-synth-day40-non-response")?,
        version_id: VersionId::new("ep-synth-day40-non-response:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:hist")?,
        recorded_at_utc_ms: t3 + 15 * DAY_MS,
        title: "Synthetic Day-40 Single-Night Corridor Sighting (No Directive)".to_owned(),
        initiating_condition: "Pilot visual reports logged without secondary radar return."
            .to_owned(),
        initiating_event_utc_ms: t3,
        initiation_evidence_versions: vec![VersionId::new("src:synth-d40-init:v1")?],
        profile: profile.clone(),
        dependency: EpisodeDependencyKind::IndependentEpisode,
        response_steps: Vec::new(),
        outcome: PrecedentOutcomeDisposition::DocumentedNonResponse {
            observed_window_end_utc_ms: t3 + 14 * DAY_MS,
            observation_duration_ms: 14 * DAY_MS,
            non_response_rationale:
                "Full 14-day docket review confirmed no public NOTAM directive was issued."
                    .to_owned(),
            outcome_evidence_versions: vec![VersionId::new("src:synth-d40-docket-audit:v1")?],
        },
        sources: vec![
            CutoffSourceInput {
                source_version: VersionId::new("src:synth-d40-init:v1")?,
                event_utc_ms: Some(t3),
                public_available_at_utc_ms: Some(t3 + 3_600_000),
                ingested_at_utc_ms: Some(t3 + 7_200_000),
            },
            CutoffSourceInput {
                source_version: VersionId::new("src:synth-d40-docket-audit:v1")?,
                event_utc_ms: Some(t3 + 14 * DAY_MS),
                public_available_at_utc_ms: Some(t3 + 14 * DAY_MS + 3_600_000),
                ingested_at_utc_ms: Some(t3 + 14 * DAY_MS + 7_200_000),
            },
        ],
        supersedes_version: None,
        correction_rationale: None,
    })?;

    // 4. Divergent response counterexample.
    let t4 = BASE_T0_MS + 65 * DAY_MS;
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-synth-day65-divergent-referral")?,
        version_id: VersionId::new("ep-synth-day65-divergent-referral:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:hist")?,
        recorded_at_utc_ms: t4 + 5 * DAY_MS,
        title: "Synthetic Day-65 Meteorological Sounding Referral".to_owned(),
        initiating_condition: "High-altitude slow-moving return in northern corridor.".to_owned(),
        initiating_event_utc_ms: t4,
        initiation_evidence_versions: vec![VersionId::new("src:synth-d65-init:v1")?],
        profile: profile.clone(),
        dependency: EpisodeDependencyKind::IndependentEpisode,
        response_steps: Vec::new(),
        outcome: PrecedentOutcomeDisposition::DivergentResponseOccurred {
            divergent_action_summary:
                "Referred track to meteorological service without airspace NOTAM directive."
                    .to_owned(),
            resolved_at_utc_ms: t4 + 4 * DAY_MS,
            elapsed_from_initiation_ms: 4 * DAY_MS,
            outcome_evidence_versions: vec![VersionId::new("src:synth-d65-out:v1")?],
        },
        sources: vec![
            CutoffSourceInput {
                source_version: VersionId::new("src:synth-d65-init:v1")?,
                event_utc_ms: Some(t4),
                public_available_at_utc_ms: Some(t4 + 3_600_000),
                ingested_at_utc_ms: Some(t4 + 7_200_000),
            },
            CutoffSourceInput {
                source_version: VersionId::new("src:synth-d65-out:v1")?,
                event_utc_ms: Some(t4 + 4 * DAY_MS),
                public_available_at_utc_ms: Some(t4 + 4 * DAY_MS + 3_600_000),
                ingested_at_utc_ms: Some(t4 + 4 * DAY_MS + 7_200_000),
            },
        ],
        supersedes_version: None,
        correction_rationale: None,
    })?;

    // 5. Episode that initiated before cutoff (day 95) and whose initial record was curated at day 96
    // as CensoredOrIncompleteFollowUp (still in flight at the day-100 cutoff).
    let t5 = BASE_T0_MS + 95 * DAY_MS;
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-synth-day95-in-flight-at-cutoff")?,
        version_id: VersionId::new("ep-synth-day95-in-flight-at-cutoff:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:hist")?,
        recorded_at_utc_ms: t5 + DAY_MS,
        title: "Synthetic Day-95 In-Flight Sector Review".to_owned(),
        initiating_condition: "Corridor anomaly under active review at day 100 cutoff.".to_owned(),
        initiating_event_utc_ms: t5,
        initiation_evidence_versions: vec![VersionId::new("src:synth-d95-init:v1")?],
        profile: profile.clone(),
        dependency: EpisodeDependencyKind::IndependentEpisode,
        response_steps: Vec::new(),
        outcome: PrecedentOutcomeDisposition::CensoredOrIncompleteFollowUp {
            observed_until_utc_ms: t5 + DAY_MS,
            censor_reason: oia_forecast::scoring::UnresolvedReason::Pending,
            coverage_gap_note: "Follow-up window still open at day-100 cutoff.".to_owned(),
            partial_evidence_versions: vec![VersionId::new("src:synth-d95-init:v1")?],
        },
        sources: vec![CutoffSourceInput {
            source_version: VersionId::new("src:synth-d95-init:v1")?,
            event_utc_ms: Some(t5),
            public_available_at_utc_ms: Some(t5 + 3_600_000),
            ingested_at_utc_ms: Some(t5 + 7_200_000),
        }],
        supersedes_version: None,
        correction_rationale: None,
    })?;

    // 6. Cross-regional episode with 4/7 matching dimensions (below `min_matched_dimensions = 5`),
    // used to demonstrate manual `IncludeEpisode` and `ExcludeEpisode` edits.
    let t6 = BASE_T0_MS + 50 * DAY_MS;
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-synth-cross-region-low-match")?,
        version_id: VersionId::new("ep-synth-cross-region-low-match:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:hist")?,
        recorded_at_utc_ms: t6 + 4 * DAY_MS,
        title: "Synthetic Day-50 Pacific Port Advisory (4/7 Match)".to_owned(),
        initiating_condition: "Harbor surface sensor alert in a different region and regime."
            .to_owned(),
        initiating_event_utc_ms: t6,
        initiation_evidence_versions: vec![VersionId::new("src:synth-d50-init:v1")?],
        profile: AnalogueProfile {
            event_scale: EventScale::Regional,
            geography_region: "Pacific Basin".to_owned(),
            actor_sector: PrecedentSector::Institution,
            actor_profile: "Municipal Port Authority".to_owned(),
            institutional_regime: InstitutionalRegime::DemocraticStatutory,
            tension_level: TensionLevel::Elevated,
            economic_condition: EconomicCondition::InflationaryOrSupplyStress,
            information_quality: InformationQuality::HighVerifiedPrimary,
        },
        dependency: EpisodeDependencyKind::IndependentEpisode,
        response_steps: Vec::new(),
        outcome: PrecedentOutcomeDisposition::DivergentResponseOccurred {
            divergent_action_summary: "Issued local harbor notice only.".to_owned(),
            resolved_at_utc_ms: t6 + 3 * DAY_MS,
            elapsed_from_initiation_ms: 3 * DAY_MS,
            outcome_evidence_versions: vec![VersionId::new("src:synth-d50-out:v1")?],
        },
        sources: vec![
            CutoffSourceInput {
                source_version: VersionId::new("src:synth-d50-init:v1")?,
                event_utc_ms: Some(t6),
                public_available_at_utc_ms: Some(t6 + 3_600_000),
                ingested_at_utc_ms: Some(t6 + 7_200_000),
            },
            CutoffSourceInput {
                source_version: VersionId::new("src:synth-d50-out:v1")?,
                event_utc_ms: Some(t6 + 3 * DAY_MS),
                public_available_at_utc_ms: Some(t6 + 3 * DAY_MS + 3_600_000),
                ingested_at_utc_ms: Some(t6 + 3 * DAY_MS + 7_200_000),
            },
        ],
        supersedes_version: None,
        correction_rationale: None,
    })?;

    // 7. Post-cutoff episode (initiated at day 120, after both cutoff_v1_ms=100d and cutoff_v2_ms=102d),
    // which must be rejected as `InadmissibleAtCutoff`.
    let t7 = BASE_T0_MS + 120 * DAY_MS;
    lib.register_episode(PrecedentEpisodeRecord {
        episode_id: OpaqueId::new("ep-synth-day120-post-cutoff")?,
        version_id: VersionId::new("ep-synth-day120-post-cutoff:v1")?,
        schema_version: SchemaVersion::new(1)?,
        recorded_by: ActorId::new("curator:hist")?,
        recorded_at_utc_ms: t7 + 3 * DAY_MS,
        title: "Synthetic Day-120 Post-Cutoff Episode".to_owned(),
        initiating_condition: "Future corridor event occurring after forecast cutoffs.".to_owned(),
        initiating_event_utc_ms: t7,
        initiation_evidence_versions: vec![VersionId::new("src:synth-d120-init:v1")?],
        profile,
        dependency: EpisodeDependencyKind::IndependentEpisode,
        response_steps: Vec::new(),
        outcome: PrecedentOutcomeDisposition::TargetResponseOccurred {
            outcome_summary: "Resolved after cutoff.".to_owned(),
            resolved_at_utc_ms: t7 + 2 * DAY_MS,
            elapsed_from_initiation_ms: 2 * DAY_MS,
            outcome_evidence_versions: vec![VersionId::new("src:synth-d120-init:v1")?],
        },
        sources: vec![CutoffSourceInput {
            source_version: VersionId::new("src:synth-d120-init:v1")?,
            event_utc_ms: Some(t7),
            public_available_at_utc_ms: Some(t7 + 3_600_000),
            ingested_at_utc_ms: Some(t7 + 7_200_000),
        }],
        supersedes_version: None,
        correction_rationale: None,
    })?;

    Ok(lib)
}
