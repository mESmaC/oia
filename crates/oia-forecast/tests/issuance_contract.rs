//! Targeted contract tests for immutable forecast issuance, strict vs. retrospective
//! point-in-time cutoffs, revision preservation, independent outcome resolutions,
//! horizon-consistent scoring, and reassessment flagging (`FORECAST-04`, `MARKET-04`,
//! `MARKET-05`, `A14`, `A15`).

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
// cannot mutate frozen outcome/horizon/baseline/cutoff-mode contracts or recycle
// predecessor sources as "new evidence", and later outcome resolution corrections
// must transitively flag prior evaluations for reassessment without mutating their
// historical scores. Also verifies distinct Resolved, Unresolved(Censored), and
// Invalidated outcome states plus horizon-consistent timing calculation.
#[test]
fn preserves_original_vintages_and_flags_evaluations_when_resolution_is_corrected()
-> Result<(), Box<dyn std::error::Error>> {
    let mut ledger = ForecastLedger::new(32)?;

    // Issue original forecast (p = 0.0) and a linked revision (p = 1.0) citing genuinely new evidence.
    ledger.issue_forecast(sample_draft("forecast:a15", "forecast:a15:v1", 0.0)?)?;

    // Attempting to mutate the frozen horizon or recycle predecessor evidence in a revision is rejected.
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

    let mut recycled_ev = sample_draft("forecast:a15", "forecast:a15:recycled", 0.75)?;
    recycled_ev.issued_at_utc_ms = 2_000;
    recycled_ev.origin_utc_ms = 2_000;
    recycled_ev.information_cutoff_utc_ms = 2_000;
    recycled_ev.predecessor_version = Some(VersionId::new("forecast:a15:v1")?);
    recycled_ev.revision_rationale = Some(RevisionRationale {
        new_evidence_versions: vec![VersionId::new("source:release:v1")?],
        summary: "Recycled predecessor evidence".to_string(),
    });
    assert_eq!(
        ledger.issue_forecast(recycled_ev),
        Err(IssuanceError::RevisionEvidenceNotNew {
            forecast_id: OpaqueId::new("forecast:a15")?,
            evidence_version: VersionId::new("source:release:v1")?,
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

    // Record initial resolution for outcome:series-a (late event at 5_250 outside [1_000, 5_000) -> occurred = false).
    ledger.record_outcome_resolution(OutcomeResolutionRecord {
        resolution_id: OpaqueId::new("res:series-a")?,
        version_id: VersionId::new("res:series-a:v1")?,
        outcome_id: OpaqueId::new("outcome:series-a")?,
        actor: ActorId::new("analyst:test")?,
        recorded_at_utc_ms: 5_500,
        evidence_versions: vec![VersionId::new("source:initial-close:v1")?],
        state: ResolutionState::Resolved {
            occurred: false,
            observed_event_utc_ms: Some(5_250),
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

    // Omitting the original vintage v1 while evaluating v2 is rejected.
    assert_eq!(
        ledger.evaluate_cohort(
            VersionId::new("eval:incomplete:v1")?,
            5_600,
            &[(
                VersionId::new("forecast:a15:v2")?,
                VersionId::new("res:series-a:v1")?,
            )],
            10,
        ),
        Err(IssuanceError::IncompleteForecastSeriesInCohort {
            forecast_id: OpaqueId::new("forecast:a15")?,
            missing_vintage: VersionId::new("forecast:a15:v1")?,
        })
    );

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
        eval1.vintages[0].horizon_timing,
        Some(HorizonTiming::Late { late_by_ms: 250 })
    );
    assert_eq!(
        eval1.mean_scores,
        Some(BrierScores {
            forecast: 0.5,
            baseline: 0.25,
        })
    );

    // Supersede res:series-a:v1 -> v2 (within-horizon occurrence at 3_500) -> v3 (corrected timestamp 3_400).
    ledger.record_outcome_resolution(OutcomeResolutionRecord {
        resolution_id: OpaqueId::new("res:series-a")?,
        version_id: VersionId::new("res:series-a:v2")?,
        outcome_id: OpaqueId::new("outcome:series-a")?,
        actor: ActorId::new("analyst:test")?,
        recorded_at_utc_ms: 6_000,
        evidence_versions: vec![VersionId::new("source:corrected-release:v1")?],
        state: ResolutionState::Resolved {
            occurred: true,
            observed_event_utc_ms: Some(3_500),
        },
        supersedes_version: Some(VersionId::new("res:series-a:v1")?),
        correction_rationale: Some(
            "Archive audit confirmed publication occurred inside target window at 3_500"
                .to_string(),
        ),
    })?;
    ledger.record_outcome_resolution(OutcomeResolutionRecord {
        resolution_id: OpaqueId::new("res:series-a")?,
        version_id: VersionId::new("res:series-a:v3")?,
        outcome_id: OpaqueId::new("outcome:series-a")?,
        actor: ActorId::new("analyst:test")?,
        recorded_at_utc_ms: 6_100,
        evidence_versions: vec![VersionId::new("source:corrected-release:v2")?],
        state: ResolutionState::Resolved {
            occurred: true,
            observed_event_utc_ms: Some(3_400),
        },
        supersedes_version: Some(VersionId::new("res:series-a:v2")?),
        correction_rationale: Some("Exact exchange timestamp refined to 3_400".to_string()),
    })?;

    // Historical eval:run:v1 retains its original scores but records transitive reassessment reasons.
    let eval1_after = ledger
        .evaluation(&VersionId::new("eval:run:v1")?)
        .ok_or("missing eval:run:v1")?;
    assert!(eval1_after.needs_reassessment);
    assert_eq!(
        eval1_after.reassessment_reasons,
        vec![
            ReassessmentReason::OutcomeResolutionSuperseded {
                outcome_id: OpaqueId::new("outcome:series-a")?,
                previous_resolution_version: VersionId::new("res:series-a:v1")?,
                new_resolution_version: VersionId::new("res:series-a:v2")?,
            },
            ReassessmentReason::OutcomeResolutionSuperseded {
                outcome_id: OpaqueId::new("outcome:series-a")?,
                previous_resolution_version: VersionId::new("res:series-a:v1")?,
                new_resolution_version: VersionId::new("res:series-a:v3")?,
            },
        ]
    );
    assert_eq!(
        eval1_after.vintages[0].brier_scores,
        eval1.vintages[0].brier_scores
    );

    // New evaluation run against res:series-a:v3 scores both original (1.0) and revised (0.0) vintages.
    let eval2 = ledger.evaluate_cohort(
        VersionId::new("eval:run:v2")?,
        6_200,
        &[
            (
                VersionId::new("forecast:a15:v1")?,
                VersionId::new("res:series-a:v3")?,
            ),
            (
                VersionId::new("forecast:a15:v2")?,
                VersionId::new("res:series-a:v3")?,
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
        Some(HorizonTiming::WithinHorizon)
    );
    Ok(())
}

// Risk addressed (MARKET-04, MARKET-05, A14, B1-B4, docs/architecture.md §§3.1, 6.4):
// Enforces point-in-time cutoff validation against post-cutoff sources, unknown or
// post-cutoff model knowledge, hindsight issuance after horizon close, events observed
// at/before a vintage's cutoff/issuance, occurrences outside the target horizon,
// cross-series outcome_id redefinition, and premature non-occurrence resolution.
#[test]
fn enforces_strict_cutoff_horizon_and_hindsight_guards() -> Result<(), Box<dyn std::error::Error>> {
    let mut ledger = ForecastLedger::new(32)?;

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

    // 3. Unknown or post-cutoff model knowledge is rejected under ProspectiveStrict.
    let mut unknown_model_kc = sample_draft("forecast:leak3", "forecast:leak3:v1", 0.6)?;
    unknown_model_kc.model_knowledge_cutoff_utc_ms = None;
    assert_eq!(
        ledger.issue_forecast(unknown_model_kc),
        Err(IssuanceError::ModelKnowledgeAfterStrictCutoff {
            model_version: VersionId::new("model:v1")?,
            model_knowledge_cutoff_utc_ms: None,
            cutoff_utc_ms: 1_000,
        })
    );

    // 4. B1: Issuing a forecast at or after horizon_end_utc_ms is rejected.
    let mut issued_after_close =
        sample_draft("forecast:late-issue", "forecast:late-issue:v1", 0.6)?;
    issued_after_close.issued_at_utc_ms = 5_000;
    assert_eq!(
        ledger.issue_forecast(issued_after_close),
        Err(IssuanceError::InvalidTimestampOrder(
            "issued_at_utc_ms must precede horizon_end_utc_ms",
        ))
    );

    // 5. In RetrospectiveReconstructed mode with a non-empty limitation note, externally
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

    // 6. B3: Reusing outcome_id "outcome:series-a" with a different horizon or proposition is rejected.
    let mut conflicting_series = sample_draft("forecast:conflict", "forecast:conflict:v1", 0.4)?;
    conflicting_series.horizon_end_utc_ms = 9_000;
    assert_eq!(
        ledger.issue_forecast(conflicting_series),
        Err(IssuanceError::ConflictingOutcomeContract {
            outcome_id: OpaqueId::new("outcome:series-a")?,
            field: "horizon",
        })
    );

    // 7. Issue a strict series and revision at t = 3_000; verify B1 (event at 2_000 before revision cutoff),
    // B2 (occurred: true outside horizon), and B4 (occurred: false before horizon_end).
    let mut strict_v1 = sample_draft("forecast:strict", "forecast:strict:v1", 0.2)?;
    strict_v1.outcome_definition = sample_outcome("outcome:strict")?;
    ledger.issue_forecast(strict_v1)?;

    let mut strict_v2 = sample_draft("forecast:strict", "forecast:strict:v2", 0.9)?;
    strict_v2.outcome_definition = sample_outcome("outcome:strict")?;
    strict_v2.issued_at_utc_ms = 3_000;
    strict_v2.origin_utc_ms = 3_000;
    strict_v2.information_cutoff_utc_ms = 3_000;
    strict_v2.sources.push(CutoffSourceInput {
        source_version: VersionId::new("source:mid-window:v1")?,
        event_utc_ms: Some(2_050),
        public_available_at_utc_ms: Some(2_100),
        ingested_at_utc_ms: Some(2_150),
    });
    strict_v2.predecessor_version = Some(VersionId::new("forecast:strict:v1")?);
    strict_v2.revision_rationale = Some(RevisionRationale {
        new_evidence_versions: vec![VersionId::new("source:mid-window:v1")?],
        summary: "Mid-window update".to_string(),
    });
    ledger.issue_forecast(strict_v2)?;

    // B2: occurred: true with timestamp 6_000 outside [1_000, 5_000) is rejected.
    assert_eq!(
        ledger.record_outcome_resolution(OutcomeResolutionRecord {
            resolution_id: OpaqueId::new("res:strict")?,
            version_id: VersionId::new("res:strict:bad-late-hit")?,
            outcome_id: OpaqueId::new("outcome:strict")?,
            actor: ActorId::new("analyst:test")?,
            recorded_at_utc_ms: 6_500,
            evidence_versions: vec![VersionId::new("source:late:v1")?],
            state: ResolutionState::Resolved {
                occurred: true,
                observed_event_utc_ms: Some(6_000),
            },
            supersedes_version: None,
            correction_rationale: None,
        }),
        Err(IssuanceError::ResolutionOutsideTargetHorizon {
            outcome_id: OpaqueId::new("outcome:strict")?,
            occurred: true,
            observed_event_utc_ms: 6_000,
            horizon_start_utc_ms: 1_000,
            horizon_end_utc_ms: 5_000,
        })
    );

    // B4: occurred: false recorded at 4_000 before horizon_end (5_000) is rejected.
    assert_eq!(
        ledger.record_outcome_resolution(OutcomeResolutionRecord {
            resolution_id: OpaqueId::new("res:strict")?,
            version_id: VersionId::new("res:strict:premature-false")?,
            outcome_id: OpaqueId::new("outcome:strict")?,
            actor: ActorId::new("analyst:test")?,
            recorded_at_utc_ms: 4_000,
            evidence_versions: vec![VersionId::new("source:early-close:v1")?],
            state: ResolutionState::Resolved {
                occurred: false,
                observed_event_utc_ms: None,
            },
            supersedes_version: None,
            correction_rationale: None,
        }),
        Err(IssuanceError::NonOccurrenceResolvedBeforeHorizonEnd {
            outcome_id: OpaqueId::new("outcome:strict")?,
            recorded_at_utc_ms: 4_000,
            horizon_end_utc_ms: 5_000,
        })
    );

    // B1: Event observed at 2_000 (inside [1_000, 5_000), but before strict_v2's cutoff 3_000)
    // is refused when attempting to score strict_v2.
    ledger.record_outcome_resolution(OutcomeResolutionRecord {
        resolution_id: OpaqueId::new("res:strict")?,
        version_id: VersionId::new("res:strict:v1")?,
        outcome_id: OpaqueId::new("outcome:strict")?,
        actor: ActorId::new("analyst:test")?,
        recorded_at_utc_ms: 3_500,
        evidence_versions: vec![VersionId::new("source:event-2000:v1")?],
        state: ResolutionState::Resolved {
            occurred: true,
            observed_event_utc_ms: Some(2_000),
        },
        supersedes_version: None,
        correction_rationale: None,
    })?;
    assert_eq!(
        ledger.evaluate_cohort(
            VersionId::new("eval:hindsight:v1")?,
            3_600,
            &[
                (
                    VersionId::new("forecast:strict:v1")?,
                    VersionId::new("res:strict:v1")?,
                ),
                (
                    VersionId::new("forecast:strict:v2")?,
                    VersionId::new("res:strict:v1")?,
                ),
            ],
            10,
        ),
        Err(IssuanceError::EventObservedAtOrBeforeForecastIssuance {
            forecast_version: VersionId::new("forecast:strict:v2")?,
            information_cutoff_utc_ms: 3_000,
            issued_at_utc_ms: 3_000,
            observed_event_utc_ms: 2_000,
        })
    );

    Ok(())
}
