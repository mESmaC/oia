//! FORECAST-04 / MARKET-05: numerical boundaries, missing coverage and preserved vintages.

use oia_domain::{ActorId, OpaqueId, SchemaVersion, VersionId};
use oia_forecast::issuance::{
    CutoffAdmissibilityMode, CutoffSourceInput, ForecastDraft, ForecastLedger, OutcomeDefinition,
    OutcomeResolutionRecord, ResolutionState, RevisionRationale,
};
use oia_forecast::reliability::{OutcomeCounts, ReliabilityError, ReliabilityStatistics};
use oia_forecast::scoring::{Probability, UnresolvedReason};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn draft(index: usize, probability: f64) -> Result<ForecastDraft, Box<dyn std::error::Error>> {
    Ok(ForecastDraft {
        forecast_id: OpaqueId::new(format!("forecast:{index}"))?,
        version_id: VersionId::new(format!("forecast:{index}:v1"))?,
        schema_version: SchemaVersion::V1,
        actor: ActorId::new("analyst:fixture")?,
        issued_at_utc_ms: 1_000,
        origin_utc_ms: 1_000,
        information_cutoff_utc_ms: 1_000,
        horizon_start_utc_ms: 1_000,
        horizon_end_utc_ms: 5_000,
        cutoff_mode: CutoffAdmissibilityMode::ProspectiveStrict,
        target_id: OpaqueId::new("target:fixture")?,
        outcome_definition: OutcomeDefinition {
            outcome_id: OpaqueId::new(format!("outcome:{index}"))?,
            proposition: "Synthetic event in the target window".into(),
            resolution_rules: "Synthetic recorded timestamp inside [1000, 5000)".into(),
        },
        baseline_version: VersionId::new("baseline:v1")?,
        baseline_probability: Probability::new(0.5)?,
        forecast_probability: Probability::new(probability)?,
        model_version: VersionId::new("model:v1")?,
        model_knowledge_cutoff_utc_ms: Some(900),
        sources: vec![CutoffSourceInput {
            source_version: VersionId::new("source:v1")?,
            event_utc_ms: Some(800),
            public_available_at_utc_ms: Some(900),
            ingested_at_utc_ms: Some(950),
        }],
        comparison_set: vec![],
        assumptions: vec!["Synthetic fixture only".into()],
        predecessor_version: None,
        revision_rationale: None,
    })
}

fn resolution(
    index: usize,
    state: ResolutionState,
) -> Result<OutcomeResolutionRecord, Box<dyn std::error::Error>> {
    Ok(OutcomeResolutionRecord {
        resolution_id: OpaqueId::new(format!("resolution:{index}"))?,
        version_id: VersionId::new(format!("resolution:{index}:v1"))?,
        outcome_id: OpaqueId::new(format!("outcome:{index}"))?,
        actor: ActorId::new("analyst:fixture")?,
        recorded_at_utc_ms: 6_000,
        evidence_versions: vec![VersionId::new("evidence:v1")?],
        state,
        supersedes_version: None,
        correction_rationale: None,
    })
}

fn resolved(occurred: bool) -> ResolutionState {
    ResolutionState::Resolved {
        occurred,
        observed_event_utc_ms: occurred.then_some(4_000),
    }
}

// Critical numerical contract: endpoints and exact/interior bin boundaries;
// all unresolved reasons remain visible and never enter observed frequencies.
#[test]
fn bins_preserve_coverage_and_use_matching_resolved_denominators() -> TestResult {
    let mut ledger = ForecastLedger::new(32)?;
    let inputs = [
        (0.0, resolved(true)),
        (0.25, resolved(false)),
        (0.5_f64.next_down(), resolved(true)),
        (0.5, resolved(false)),
        (0.75, ResolutionState::Unresolved(UnresolvedReason::Pending)),
        (1.0, resolved(true)),
        (0.5, ResolutionState::Unresolved(UnresolvedReason::Censored)),
        (0.5, ResolutionState::Unresolved(UnresolvedReason::Disputed)),
        (
            1.0,
            ResolutionState::Invalidated {
                reason: "Synthetic precondition failed".into(),
            },
        ),
    ];
    let mut cohort = Vec::new();
    for (index, (probability, state)) in inputs.into_iter().enumerate() {
        let forecast = draft(index, probability)?;
        let outcome = resolution(index, state)?;
        cohort.push((forecast.version_id.clone(), outcome.version_id.clone()));
        ledger.issue_forecast(forecast)?;
        ledger.record_outcome_resolution(outcome)?;
    }
    let version = VersionId::new("evaluation:v1")?;
    ledger.evaluate_cohort(version.clone(), 7_000, &cohort, 32)?;
    let report = ledger.reliability_report(&version, 4, 9)?;
    assert_eq!(
        report
            .rows
            .iter()
            .map(|r| r.forecast_bin)
            .collect::<Vec<_>>(),
        [0, 1, 1, 2, 3, 3, 2, 2, 3]
    );
    assert!(report.rows.iter().all(|r| r.baseline_bin == 2));
    for (row, original) in report.rows.iter().zip(&report.evaluation.vintages) {
        assert!(std::ptr::eq(row.vintage, original));
    }
    let forecast = report.forecast_bins.get(1).ok_or("missing bin")?;
    let stats = forecast.statistics.ok_or("missing statistics")?;
    assert!((stats.mean_probability - 0.375).abs() < 1e-15);
    assert_eq!(stats.observed_frequency, 0.5);
    let baseline = report.baseline_bins.get(2).ok_or("missing bin")?;
    assert_eq!(
        baseline.counts,
        OutcomeCounts {
            resolved: 5,
            pending: 1,
            censored: 1,
            disputed: 1,
            invalidated: 1
        }
    );
    assert_eq!(baseline.counts.total(), 9);
    assert_eq!(
        baseline.statistics,
        Some(ReliabilityStatistics {
            mean_probability: 0.5,
            observed_frequency: 0.6
        })
    );
    assert_eq!(
        report
            .baseline_bins
            .first()
            .ok_or("missing bin")?
            .statistics,
        None
    );

    // Whole-report refusal: caller budgets cannot silently truncate coverage.
    assert_eq!(
        ledger.reliability_report(&version, 4, 8),
        Err(ReliabilityError::RowLimitExceeded {
            supplied: 9,
            limit: 8
        })
    );
    for bins in [0, 101, usize::MAX] {
        assert_eq!(
            ledger.reliability_report(&version, bins, 9),
            Err(ReliabilityError::InvalidBinCount)
        );
    }
    for limit in [0, 100_001, usize::MAX] {
        assert_eq!(
            ledger.reliability_report(&version, 4, limit),
            Err(ReliabilityError::InvalidRowLimit)
        );
    }
    let missing = VersionId::new("missing")?;
    assert_eq!(
        ledger.reliability_report(&missing, 4, 9),
        Err(ReliabilityError::UnknownEvaluation(missing))
    );
    Ok(())
}

// Evidence contract: a successful revision cannot erase the losing original;
// corrections mark the old report stale while preserving its numerical values.
#[test]
fn revisions_and_corrections_remain_inspectable() -> TestResult {
    let mut ledger = ForecastLedger::new(16)?;
    let original = draft(0, 0.0)?;
    let mut revision = original.clone();
    revision.version_id = VersionId::new("forecast:0:v2")?;
    revision.forecast_probability = Probability::new(1.0)?;
    revision.predecessor_version = Some(original.version_id.clone());
    revision.issued_at_utc_ms = 2_000;
    revision.origin_utc_ms = 2_000;
    revision.information_cutoff_utc_ms = 2_000;
    revision.sources.push(CutoffSourceInput {
        source_version: VersionId::new("source:v2")?,
        event_utc_ms: Some(1_500),
        public_available_at_utc_ms: Some(1_600),
        ingested_at_utc_ms: Some(1_700),
    });
    revision.revision_rationale = Some(RevisionRationale {
        new_evidence_versions: vec![VersionId::new("source:v2")?],
        summary: "Synthetic new evidence".into(),
    });
    let mut outcome = resolution(0, resolved(true))?;
    let cohort = [
        (original.version_id.clone(), outcome.version_id.clone()),
        (revision.version_id.clone(), outcome.version_id.clone()),
    ];
    ledger.issue_forecast(original)?;
    ledger.issue_forecast(revision)?;
    ledger.record_outcome_resolution(outcome.clone())?;
    let old = VersionId::new("evaluation:old")?;
    ledger.evaluate_cohort(old.clone(), 7_000, &cohort, 2)?;
    assert_eq!(
        ledger
            .reliability_report(&old, 1, 2)?
            .forecast_bins
            .first()
            .ok_or("missing bin")?
            .statistics,
        Some(ReliabilityStatistics {
            mean_probability: 0.5,
            observed_frequency: 1.0
        })
    );

    outcome.supersedes_version = Some(outcome.version_id.clone());
    outcome.version_id = VersionId::new("resolution:0:v2")?;
    outcome.recorded_at_utc_ms = 8_000;
    outcome.state = ResolutionState::Unresolved(UnresolvedReason::Disputed);
    outcome.correction_rationale = Some("Fixture timestamp disputed".into());
    ledger.record_outcome_resolution(outcome.clone())?;
    let corrected: Vec<_> = cohort
        .iter()
        .map(|(f, _)| (f.clone(), outcome.version_id.clone()))
        .collect();
    let new = VersionId::new("evaluation:new")?;
    ledger.evaluate_cohort(new.clone(), 9_000, &corrected, 2)?;
    let prior = ledger.reliability_report(&old, 1, 2)?;
    assert!(prior.evaluation.needs_reassessment);
    assert!(!prior.evaluation.reassessment_reasons.is_empty());
    assert_eq!(prior.rows.len(), 2);
    assert_eq!(
        prior
            .forecast_bins
            .first()
            .ok_or("missing bin")?
            .statistics
            .ok_or("missing statistics")?
            .observed_frequency,
        1.0
    );
    let current = ledger.reliability_report(&new, 1, 2)?;
    assert!(!current.evaluation.needs_reassessment);
    assert_eq!(
        current
            .forecast_bins
            .first()
            .ok_or("missing bin")?
            .counts
            .disputed,
        2
    );
    assert!(
        current
            .forecast_bins
            .iter()
            .chain(&current.baseline_bins)
            .all(|b| b.statistics.is_none())
    );

    let empty = VersionId::new("evaluation:empty")?;
    ledger.evaluate_cohort(empty.clone(), 9_000, &[], 1)?;
    let empty_report = ledger.reliability_report(&empty, 100, 1)?;
    assert!(empty_report.rows.is_empty());
    assert!(
        empty_report
            .forecast_bins
            .iter()
            .chain(&empty_report.baseline_bins)
            .all(|b| b.statistics.is_none() && b.counts.total() == 0)
    );
    Ok(())
}
