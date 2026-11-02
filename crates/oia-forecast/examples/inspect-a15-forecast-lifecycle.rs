//! Inspect an A14/A15 forecast issuance, linked revision, outcome resolution,
//! and resolution-correction reassessment lifecycle in `oia-forecast`.

use oia_domain::{ActorId, OpaqueId, SchemaVersion, VersionId};
use oia_forecast::issuance::{
    CutoffAdmissibilityMode, CutoffSourceInput, ForecastDraft, ForecastLedger, OutcomeDefinition,
    OutcomeResolutionRecord, ResolutionState, RevisionRationale,
};
use oia_forecast::scoring::{Probability, UnresolvedReason};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut ledger = ForecastLedger::new(64)?;
    let outcome_def = OutcomeDefinition {
        outcome_id: OpaqueId::new("outcome:a15-agency-briefing")?,
        proposition: "Agency publishes formal response briefing within 30-day window".to_string(),
        resolution_rules:
            "Official registry publication timestamp inside [horizon_start, horizon_end)"
                .to_string(),
    };

    // 1. Issue initial prospective forecast (vintage 1, p = 0.25, baseline = 0.50).
    let v1 = ledger.issue_forecast(ForecastDraft {
        forecast_id: OpaqueId::new("forecast:a15-briefing")?,
        version_id: VersionId::new("forecast:a15-briefing:v1")?,
        schema_version: SchemaVersion::V1,
        actor: ActorId::new("analyst:local")?,
        issued_at_utc_ms: 1_700_000_000_000,
        origin_utc_ms: 1_700_000_000_000,
        information_cutoff_utc_ms: 1_700_000_000_000,
        horizon_start_utc_ms: 1_700_000_000_000,
        horizon_end_utc_ms: 1_702_592_000_000,
        cutoff_mode: CutoffAdmissibilityMode::ProspectiveStrict,
        target_id: OpaqueId::new("actor:civil-aviation-board")?,
        outcome_definition: outcome_def.clone(),
        baseline_version: VersionId::new("baseline:base-rate:v1")?,
        baseline_probability: Probability::new(0.50)?,
        forecast_probability: Probability::new(0.25)?,
        model_version: VersionId::new("model:analogue-cohort:v1")?,
        model_knowledge_cutoff_utc_ms: Some(1_690_000_000_000),
        sources: vec![CutoffSourceInput {
            source_version: VersionId::new("source:initial-notice:v1")?,
            event_utc_ms: Some(1_699_900_000_000),
            public_available_at_utc_ms: Some(1_699_950_000_000),
            ingested_at_utc_ms: Some(1_699_960_000_000),
        }],
        comparison_set: vec![
            VersionId::new("analogue:1997-phoenix:v1")?,
            VersionId::new("analogue:2006-ohare-nonresponse:v1")?,
        ],
        assumptions: vec!["No statutory reporting mandate triggered".to_string()],
        predecessor_version: None,
        revision_rationale: None,
    })?;
    println!(
        "issued {} (vintage #{}) p={:.2}",
        v1.version_id,
        v1.vintage_number,
        v1.forecast_probability.value()
    );

    // 2. Issue linked revision after new committee hearing notice (vintage 2, p = 0.75).
    let v2 = ledger.issue_forecast(ForecastDraft {
        forecast_id: OpaqueId::new("forecast:a15-briefing")?,
        version_id: VersionId::new("forecast:a15-briefing:v2")?,
        schema_version: SchemaVersion::V1,
        actor: ActorId::new("analyst:local")?,
        issued_at_utc_ms: 1_700_500_000_000,
        origin_utc_ms: 1_700_500_000_000,
        information_cutoff_utc_ms: 1_700_500_000_000,
        horizon_start_utc_ms: 1_700_000_000_000,
        horizon_end_utc_ms: 1_702_592_000_000,
        cutoff_mode: CutoffAdmissibilityMode::ProspectiveStrict,
        target_id: OpaqueId::new("actor:civil-aviation-board")?,
        outcome_definition: outcome_def,
        baseline_version: VersionId::new("baseline:base-rate:v1")?,
        baseline_probability: Probability::new(0.50)?,
        forecast_probability: Probability::new(0.75)?,
        model_version: VersionId::new("model:analogue-cohort:v1")?,
        model_knowledge_cutoff_utc_ms: Some(1_690_000_000_000),
        sources: vec![
            CutoffSourceInput {
                source_version: VersionId::new("source:initial-notice:v1")?,
                event_utc_ms: Some(1_699_900_000_000),
                public_available_at_utc_ms: Some(1_699_950_000_000),
                ingested_at_utc_ms: Some(1_699_960_000_000),
            },
            CutoffSourceInput {
                source_version: VersionId::new("source:committee-docket:v1")?,
                event_utc_ms: Some(1_700_400_000_000),
                public_available_at_utc_ms: Some(1_700_450_000_000),
                ingested_at_utc_ms: Some(1_700_460_000_000),
            },
        ],
        comparison_set: vec![
            VersionId::new("analogue:1997-phoenix:v1")?,
            VersionId::new("analogue:2006-ohare-nonresponse:v1")?,
        ],
        assumptions: vec!["Oversight docket scheduled prior to window close".to_string()],
        predecessor_version: Some(VersionId::new("forecast:a15-briefing:v1")?),
        revision_rationale: Some(RevisionRationale {
            new_evidence_versions: vec![VersionId::new("source:committee-docket:v1")?],
            summary: "Oversight docket notice added; original v1 preserved for scoring".to_string(),
        }),
    })?;
    println!(
        "revised {} -> {} (vintage #{}) p={:.2}",
        v2.predecessor_version
            .as_ref()
            .map_or("-", VersionId::as_str),
        v2.version_id,
        v2.vintage_number,
        v2.forecast_probability.value()
    );

    // 3. Record initial outcome resolution (pending) and evaluate both vintages.
    ledger.record_outcome_resolution(OutcomeResolutionRecord {
        resolution_id: OpaqueId::new("resolution:a15-briefing")?,
        version_id: VersionId::new("resolution:a15-briefing:v1")?,
        outcome_id: OpaqueId::new("outcome:a15-agency-briefing")?,
        actor: ActorId::new("analyst:local")?,
        recorded_at_utc_ms: 1_701_000_000_000,
        evidence_versions: Vec::new(),
        state: ResolutionState::Unresolved(UnresolvedReason::Pending),
        supersedes_version: None,
        correction_rationale: None,
    })?;

    let eval_v1 = ledger.evaluate_cohort(
        VersionId::new("eval:a15-briefing:v1")?,
        1_701_000_100_000,
        &[
            (
                VersionId::new("forecast:a15-briefing:v1")?,
                VersionId::new("resolution:a15-briefing:v1")?,
            ),
            (
                VersionId::new("forecast:a15-briefing:v2")?,
                VersionId::new("resolution:a15-briefing:v1")?,
            ),
        ],
        10,
    )?;
    println!(
        "evaluation {} resolved={} unresolved={} mean_scores={:?}",
        eval_v1.evaluation_version,
        eval_v1.resolved_count,
        eval_v1.unresolved_count,
        eval_v1.mean_scores
    );

    // 4. Supersede resolution when event occurs within horizon; prior evaluation is flagged for reassessment.
    ledger.record_outcome_resolution(OutcomeResolutionRecord {
        resolution_id: OpaqueId::new("resolution:a15-briefing")?,
        version_id: VersionId::new("resolution:a15-briefing:v2")?,
        outcome_id: OpaqueId::new("outcome:a15-agency-briefing")?,
        actor: ActorId::new("analyst:local")?,
        recorded_at_utc_ms: 1_702_100_000_000,
        evidence_versions: vec![VersionId::new("source:published-briefing:v1")?],
        state: ResolutionState::Resolved {
            occurred: true,
            observed_event_utc_ms: Some(1_702_000_000_000),
        },
        supersedes_version: Some(VersionId::new("resolution:a15-briefing:v1")?),
        correction_rationale: Some(
            "Formal briefing published before window end; supersedes pending status".to_string(),
        ),
    })?;

    let prior_eval = ledger
        .evaluation(&VersionId::new("eval:a15-briefing:v1")?)
        .ok_or("missing eval:a15-briefing:v1")?;
    println!(
        "prior evaluation {} needs_reassessment={} reasons={:?}",
        prior_eval.evaluation_version,
        prior_eval.needs_reassessment,
        prior_eval.reassessment_reasons
    );

    let eval_v2 = ledger.evaluate_cohort(
        VersionId::new("eval:a15-briefing:v2")?,
        1_702_100_100_000,
        &[
            (
                VersionId::new("forecast:a15-briefing:v1")?,
                VersionId::new("resolution:a15-briefing:v2")?,
            ),
            (
                VersionId::new("forecast:a15-briefing:v2")?,
                VersionId::new("resolution:a15-briefing:v2")?,
            ),
        ],
        10,
    )?;
    for row in &eval_v2.vintages {
        println!(
            "vintage #{} ({}) brier={:?} timing={:?}",
            row.vintage_number, row.forecast_version, row.brier_scores, row.horizon_timing
        );
    }
    println!("Synthetic fixture reliability; descriptive vintage counts, not a calibration claim.");
    for version in ["eval:a15-briefing:v1", "eval:a15-briefing:v2"] {
        let report = ledger.reliability_report(&VersionId::new(version)?, 4, 10)?;
        println!(
            "reliability {} needs_reassessment={}",
            report.evaluation.evaluation_version, report.evaluation.needs_reassessment
        );
        for (label, bins) in [
            ("forecast", &report.forecast_bins),
            ("baseline", &report.baseline_bins),
        ] {
            for bin in bins {
                println!(
                    "{label} [{:.2}, {:.2}{} counts={:?} statistics={:?}",
                    bin.lower,
                    bin.upper,
                    if bin.upper_inclusive { "]" } else { ")" },
                    bin.counts,
                    bin.statistics
                );
            }
        }
    }
    Ok(())
}
