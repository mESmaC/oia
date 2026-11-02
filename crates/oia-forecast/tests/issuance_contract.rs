//! Targeted contract tests for immutable forecast issuance, strict vs. retrospective
//! point-in-time cutoffs, revision preservation, independent outcome resolutions,
//! and reassessment flagging (`FORECAST-04`, `MARKET-04`, `MARKET-05`, `A14`, `A15`).

use oia_domain::{ActorId, OpaqueId, SchemaVersion, VersionId};
use oia_forecast::issuance::{
    CutoffAdmissibilityMode, CutoffSourceInput, ForecastDraft, ForecastLedger, HorizonTiming,
    IssuanceError, OutcomeDefinition, OutcomeResolutionRecord, ReassessmentReason, ResolutionState,
    RevisionRationale,
};
use oia_forecast::scoring::{BrierScores, Probability, UnresolvedReason};

fn sample_outcome(id: &str) -> Result<OutcomeDefinition, Box<dyn std::error::Error>> {
    Ok(OutcomeDefinition {
        outcome_id: OpaqueId::new(id)?,
        proposition: "Defined event occurs within target horizon".to_string(),
        resolution_rules: "Official public release timestamp in [horizon_start, horizon_end)"
            .to_string(),
    })
}

fn sample_draft(
    forecast_id: &str,
    version_id: &str,
    probability: f64,
) -> Result<ForecastDraft, Box<dyn std::error::Error>> {
    Ok(ForecastDraft {
        forecast_id: OpaqueId::new(forecast_id)?,
        version_id: VersionId::new(version_id)?,
        schema_version: SchemaVersion::V1,
        actor: ActorId::new("analyst:test")?,
        issued_at_utc_ms: 1_000,
        origin_utc_ms: 1_000,
        information_cutoff_utc_ms: 1_000,
        horizon_start_utc_ms: 1_000,
        horizon_end_utc_ms: 5_000,
        cutoff_mode: CutoffAdmissibilityMode::ProspectiveStrict,
        target_id: OpaqueId::new("target:series-a")?,
        outcome_definition: sample_outcome("outcome:series-a")?,
        baseline_version: VersionId::new("baseline:v1")?,
        baseline_probability: Probability::new(0.5)?,
        forecast_probability: Probability::new(probability)?,
        model_version: VersionId::new("model:v1")?,
        model_knowledge_cutoff_utc_ms: Some(900),
        sources: vec![CutoffSourceInput {
            source_version: VersionId::new("source:release:v1")?,
            event_utc_ms: Some(800),
            public_available_at_utc_ms: Some(900),
            ingested_at_utc_ms: Some(950),
        }],
        comparison_set: vec![VersionId::new("analogue:ep1:v1")?],
        assumptions: vec!["Scheduled release window holds".to_string()],
        predecessor_version: None,
        revision_rationale: None,
    })
}

// Risk addressed (FORECAST-04, MARKET-05, A15):
// Revisions must never overwrite or hide an unsuccessful initial forecast vintage,
// cannot mutate frozen outcome/horizon definitions, and later outcome resolution
// corrections must flag prior evaluations for reassessment without mutating their
// historical scores. Also verifies distinct Resolved, Unresolved(Censored), and
// Invalidated outcome states plus horizon timing calculation.
#[test]
fn preserves_original_vintages_and_flags_evaluations_when_resolution_is_corrected()
-> Result<(), Box<dyn std::error::Error>> {
    let mut ledger = ForecastLedger::new(32)?;

    // Issue original forecast (p = 0.0) and a linked revision (p = 1.0) citing new evidence.
    ledger.issue_forecast(sample_draft("forecast:a15", "forecast:a15:v1", 0.0)?)?;

    // Attempting to mutate the frozen horizon or omit new evidence in a revision is rejected.
    let mut bad_horizon = sample_draft("forecast:a15", "forecast:a15:bad-horizon", 0.75)?;
    bad_horizon.issued_at_utc_ms = 2_000;
    bad_horizon.origin_utc_ms = 2_000;
    bad_horizon.information_cutoff_utc_ms = 2_000;
    bad_horizon.horizon_end_utc_ms = 6_000;
    bad_horizon.predecessor_version = Some(VersionId::new("forecast:a15:v1")?);
    bad_horizon.revision_rationale = Some(RevisionRationale {
        new_evidence_versions: vec![VersionId::new("source:release:v1")?],
        summary: "Attempted horizon shift".to_string(),
    });
    assert_eq!(
        ledger.issue_forecast(bad_horizon),
        Err(IssuanceError::RevisionMutatedFrozenContract {
            forecast_id: OpaqueId::new("forecast:a15")?,
            field: "horizon",
        })
    );

    let mut rev = sample_draft("forecast:a15", "forecast:a15:v2", 1.0)?;
    rev.issued_at_utc_ms = 2_000;
    rev.origin_utc_ms = 2_000;
    rev.information_cutoff_utc_ms = 2_000;
    rev.sources.push(CutoffSourceInput {
        source_version: VersionId::new("source:update:v1")?,
        event_utc_ms: Some(1_500),
        public_available_at_utc_ms: Some(1_800),
        ingested_at_utc_ms: Some(1_900),
    });
    rev.predecessor_version = Some(VersionId::new("forecast:a15:v1")?);
    rev.revision_rationale = Some(RevisionRationale {
        new_evidence_versions: vec![VersionId::new("source:update:v1")?],
        summary: "Updated filing confirms agency schedule".to_string(),
    });
    ledger.issue_forecast(rev)?;

    // Also issue separate censored and invalidated series to verify distinct cohort accounting.
    let mut censored_draft = sample_draft("forecast:censored", "forecast:censored:v1", 0.5)?;
    censored_draft.outcome_definition = sample_outcome("outcome:censored")?;
    ledger.issue_forecast(censored_draft)?;

    let mut invalid_draft = sample_draft("forecast:invalidated", "forecast:invalidated:v1", 0.75)?;
    invalid_draft.outcome_definition = sample_outcome("outcome:invalidated")?;
    ledger.issue_forecast(invalid_draft)?;

    // Record initial resolution for outcome:series-a (occurred = false) and evaluate.
    ledger.record_outcome_resolution(OutcomeResolutionRecord {
        resolution_id: OpaqueId::new("res:series-a")?,
        version_id: VersionId::new("res:series-a:v1")?,
        outcome_id: OpaqueId::new("outcome:series-a")?,
        actor: ActorId::new("analyst:test")?,
        recorded_at_utc_ms: 5_500,
        evidence_versions: vec![VersionId::new("source:initial-close:v1")?],
        state: ResolutionState::Resolved {
            occurred: false,
            observed_event_utc_ms: None,
        },
        supersedes_version: None,
        correction_rationale: None,
    })?;
    ledger.record_outcome_resolution(OutcomeResolutionRecord {
        resolution_id: OpaqueId::new("res:censored")?,
        version_id: VersionId::new("res:censored:v1")?,
        outcome_id: OpaqueId::new("outcome:censored")?,
        actor: ActorId::new("analyst:test")?,
        recorded_at_utc_ms: 5_500,
        evidence_versions: Vec::new(),
        state: ResolutionState::Unresolved(UnresolvedReason::Censored),
        supersedes_version: None,
        correction_rationale: None,
    })?;
    ledger.record_outcome_resolution(OutcomeResolutionRecord {
        resolution_id: OpaqueId::new("res:invalidated")?,
        version_id: VersionId::new("res:invalidated:v1")?,
        outcome_id: OpaqueId::new("outcome:invalidated")?,
        actor: ActorId::new("analyst:test")?,
        recorded_at_utc_ms: 5_500,
        evidence_versions: vec![VersionId::new("source:precondition-void:v1")?],
        state: ResolutionState::Invalidated {
            reason: "Target instrument halted and delisted prior to window".to_string(),
        },
        supersedes_version: None,
        correction_rationale: None,
    })?;

    let eval1 = ledger
        .evaluate_cohort(
            VersionId::new("eval:run:v1")?,
            5_600,
            &[
                (
                    VersionId::new("forecast:a15:v1")?,
                    VersionId::new("res:series-a:v1")?,
                ),
                (
                    VersionId::new("forecast:a15:v2")?,
                    VersionId::new("res:series-a:v1")?,
                ),
                (
                    VersionId::new("forecast:censored:v1")?,
                    VersionId::new("res:censored:v1")?,
                ),
                (
                    VersionId::new("forecast:invalidated:v1")?,
                    VersionId::new("res:invalidated:v1")?,
                ),
            ],
            10,
        )?
        .clone();

    assert_eq!(
        (
            eval1.resolved_count,
            eval1.unresolved_count,
            eval1.invalidated_count
        ),
        (2, 1, 1)
    );
    assert!(!eval1.needs_reassessment);
    assert_eq!(
        eval1.mean_scores,
        Some(BrierScores {
            forecast: 0.5,
            baseline: 0.25,
        })
    );

    // Supersede res:series-a:v1 with a corrected resolution where the event occurred late at 5_250.
    ledger.record_outcome_resolution(OutcomeResolutionRecord {
        resolution_id: OpaqueId::new("res:series-a")?,
        version_id: VersionId::new("res:series-a:v2")?,
        outcome_id: OpaqueId::new("outcome:series-a")?,
        actor: ActorId::new("analyst:test")?,
        recorded_at_utc_ms: 6_000,
        evidence_versions: vec![VersionId::new("source:corrected-release:v1")?],
        state: ResolutionState::Resolved {
            occurred: true,
            observed_event_utc_ms: Some(5_250),
        },
        supersedes_version: Some(VersionId::new("res:series-a:v1")?),
        correction_rationale: Some(
            "Late registry publication confirmed event occurrence 250ms after window end"
                .to_string(),
        ),
    })?;

    // Historical eval:run:v1 retains its original scores but is now flagged for reassessment.
    let eval1_after = ledger
        .evaluation(&VersionId::new("eval:run:v1")?)
        .ok_or("missing eval:run:v1")?;
    assert!(eval1_after.needs_reassessment);
    assert_eq!(
        eval1_after.reassessment_reasons,
        vec![ReassessmentReason::OutcomeResolutionSuperseded {
            outcome_id: OpaqueId::new("outcome:series-a")?,
            previous_resolution_version: VersionId::new("res:series-a:v1")?,
            new_resolution_version: VersionId::new("res:series-a:v2")?,
        }]
    );
    assert_eq!(
        eval1_after.vintages[0].brier_scores,
        eval1.vintages[0].brier_scores
    );

    // New evaluation run against res:series-a:v2 scores both original (1.0) and revised (0.0) vintages
    // and reports the 250ms late horizon timing error.
    let eval2 = ledger.evaluate_cohort(
        VersionId::new("eval:run:v2")?,
        6_100,
        &[
            (
                VersionId::new("forecast:a15:v1")?,
                VersionId::new("res:series-a:v2")?,
            ),
            (
                VersionId::new("forecast:a15:v2")?,
                VersionId::new("res:series-a:v2")?,
            ),
        ],
        10,
    )?;
    assert!(!eval2.needs_reassessment);
    assert_eq!(
        eval2.vintages[0].brier_scores,
        Some(BrierScores {
            forecast: 1.0,
            baseline: 0.25,
        })
    );
    assert_eq!(
        eval2.vintages[1].brier_scores,
        Some(BrierScores {
            forecast: 0.0,
            baseline: 0.25,
        })
    );
    assert_eq!(
        eval2.vintages[0].horizon_timing,
        Some(HorizonTiming::Late { late_by_ms: 250 })
    );
    Ok(())
}

// Risk addressed (MARKET-04, MARKET-05, A14, docs/architecture.md §§3.1, 6.4):
// Point-in-time cutoff validation must prevent leakage from revised series, later news,
// unknown availability, post-cutoff OIA ingestion in ProspectiveStrict mode, and
// post-cutoff language-model knowledge, while allowing explicitly labeled
// RetrospectiveReconstructed forecasts when public availability precedes the cutoff.
#[test]
fn enforces_strict_and_retrospective_cutoff_admissibility_against_leakage()
-> Result<(), Box<dyn std::error::Error>> {
    let mut ledger = ForecastLedger::new(16)?;

    // 1. Unknown public availability is rejected in both strict and retrospective modes.
    let mut unknown_pub = sample_draft("forecast:leak1", "forecast:leak1:v1", 0.6)?;
    unknown_pub.sources[0].public_available_at_utc_ms = None;
    assert_eq!(
        ledger.issue_forecast(unknown_pub),
        Err(IssuanceError::SourcePublicAvailabilityAfterCutoff {
            source_version: VersionId::new("source:release:v1")?,
            public_available_at_utc_ms: None,
            cutoff_utc_ms: 1_000,
        })
    );

    // 2. Event occurred before cutoff (800), publicly available at 900, but ingested into OIA
    // after cutoff (1_200): rejected under ProspectiveStrict.
    let mut late_ingest = sample_draft("forecast:leak2", "forecast:leak2:v1", 0.6)?;
    late_ingest.sources[0].ingested_at_utc_ms = Some(1_200);
    assert_eq!(
        ledger.issue_forecast(late_ingest.clone()),
        Err(IssuanceError::SourceIngestedAfterStrictCutoff {
            source_version: VersionId::new("source:release:v1")?,
            ingested_at_utc_ms: Some(1_200),
            cutoff_utc_ms: 1_000,
        })
    );

    // 3. Model knowledge cutoff after forecast cutoff is rejected under ProspectiveStrict.
    let mut post_cutoff_model = sample_draft("forecast:leak3", "forecast:leak3:v1", 0.6)?;
    post_cutoff_model.model_knowledge_cutoff_utc_ms = Some(1_500);
    assert_eq!(
        ledger.issue_forecast(post_cutoff_model),
        Err(IssuanceError::ModelKnowledgeAfterStrictCutoff {
            model_version: VersionId::new("model:v1")?,
            model_knowledge_cutoff_utc_ms: 1_500,
            cutoff_utc_ms: 1_000,
        })
    );

    // 4. In RetrospectiveReconstructed mode with a non-empty limitation note, externally
    // documented public availability <= cutoff (900 <= 1_000) is admitted even though OIA
    // ingested the archive at 1_200, and post-cutoff model knowledge is visibly disclosed.
    late_ingest.cutoff_mode = CutoffAdmissibilityMode::RetrospectiveReconstructed {
        limitation_note:
            "Historical archive ingested after cutoff; filtered by exchange release timestamp"
                .to_string(),
    };
    late_ingest.model_knowledge_cutoff_utc_ms = Some(1_500);
    let issued = ledger.issue_forecast(late_ingest)?;
    assert!(issued.post_cutoff_model_knowledge_disclosed);
    Ok(())
}
