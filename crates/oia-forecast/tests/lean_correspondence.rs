//! Tests the cutoff projection against kernel-checked Lean fixture expectations.
//! Risk: unknown/future inputs or omitted disclosure diverge from the proved model.
//! This is finite correspondence evidence, not a formal proof of the Rust program.

use oia_domain::{ActorId, OpaqueId, SchemaVersion, VersionId};
use oia_forecast::issuance::{
    CutoffAdmissibilityMode, CutoffSourceInput, ForecastDraft, ForecastLedger, IssuanceError,
    OutcomeDefinition,
};
use oia_forecast::scoring::Probability;

type TestResult<T> = Result<T, Box<dyn std::error::Error>>;

fn optional_time(text: &str) -> TestResult<Option<i64>> {
    if text == "?" {
        Ok(None)
    } else {
        Ok(Some(text.parse()?))
    }
}

#[test]
fn ledger_cutoffs_match_kernel_checked_lean_fixtures() -> TestResult<()> {
    let fixtures = include_str!("../../../proofs/fixtures/cutoff.tsv");
    let mut checked = 0;
    for line in fixtures.lines().filter(|line| !line.starts_with('#')) {
        let fields: Vec<_> = line.split('\t').collect();
        let [id, mode, cutoff, knowledge, source_text, expected] = fields.as_slice() else {
            return Err(format!("invalid Lean fixture row: {line}").into());
        };
        let cutoff: i64 = cutoff.parse()?;
        let cutoff_mode = match *mode {
            "strict" => CutoffAdmissibilityMode::ProspectiveStrict,
            "retro" => CutoffAdmissibilityMode::RetrospectiveReconstructed {
                limitation_note: "Synthetic reconstruction; ingestion may be later or unknown."
                    .into(),
            },
            other => return Err(format!("unknown fixture mode: {other}").into()),
        };
        let mut sources = Vec::new();
        if *source_text != "-" {
            for (index, pair) in source_text.split(';').enumerate() {
                let (available, ingested) = pair.split_once('/').ok_or("invalid source pair")?;
                sources.push(CutoffSourceInput {
                    source_version: VersionId::new(format!("source:{index}"))?,
                    // No event constraint is part of this cutoff projection.
                    event_utc_ms: None,
                    public_available_at_utc_ms: optional_time(available)?,
                    ingested_at_utc_ms: optional_time(ingested)?,
                });
            }
        }
        // All non-cutoff validation preconditions are fixed and valid. Use a fresh
        // ledger per row so history/capacity cannot masquerade as a cutoff rejection.
        let draft = ForecastDraft {
            forecast_id: OpaqueId::new("forecast:lean")?,
            version_id: VersionId::new("forecast:lean:v1")?,
            schema_version: SchemaVersion::V1,
            actor: ActorId::new("actor:lean-fixture")?,
            issued_at_utc_ms: cutoff,
            origin_utc_ms: cutoff,
            information_cutoff_utc_ms: cutoff,
            horizon_start_utc_ms: cutoff,
            horizon_end_utc_ms: cutoff.checked_add(1).ok_or("fixture horizon overflow")?,
            cutoff_mode,
            target_id: OpaqueId::new("target:synthetic")?,
            outcome_definition: OutcomeDefinition {
                outcome_id: OpaqueId::new("outcome:synthetic")?,
                proposition: "Synthetic event occurs in the declared horizon".into(),
                resolution_rules: "Synthetic fixture only; no observed evidence".into(),
            },
            baseline_version: VersionId::new("baseline:v1")?,
            baseline_probability: Probability::new(0.5)?,
            forecast_probability: Probability::new(0.5)?,
            model_version: VersionId::new("model:v1")?,
            model_knowledge_cutoff_utc_ms: optional_time(knowledge)?,
            sources,
            comparison_set: Vec::new(),
            assumptions: vec!["All non-cutoff draft preconditions hold".into()],
            predecessor_version: None,
            revision_rationale: None,
        };
        let mut ledger = ForecastLedger::new(1)?;
        let actual = match ledger.issue_forecast(draft) {
            Ok(record) if record.post_cutoff_model_knowledge_disclosed => "disclosed",
            Ok(_) => "accept",
            Err(
                IssuanceError::SourcePublicAvailabilityAfterCutoff { .. }
                | IssuanceError::SourceIngestedAfterStrictCutoff { .. }
                | IssuanceError::ModelKnowledgeAfterStrictCutoff { .. },
            ) => "reject",
            Err(IssuanceError::InvalidSourceList(_)) if *source_text == "-" => "reject",
            Err(other) => {
                return Err(format!("{id}: unexpected non-cutoff failure: {other}").into());
            }
        };
        assert_eq!(actual, *expected, "Lean/Rust cutoff mismatch for {id}");
        checked += 1;
    }
    assert!(checked > 0, "Lean correspondence fixture must not be empty");
    Ok(())
}
