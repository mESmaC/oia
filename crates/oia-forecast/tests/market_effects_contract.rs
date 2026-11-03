//! Focused contract tests for `oia_forecast::market_effects` (`#44`, `MARKET-03`..=`MARKET-05`, `A14`).

use oia_domain::{ActorId, OpaqueId, SchemaVersion, VersionId};
use oia_forecast::interval::PredictionInterval;
use oia_forecast::issuance::{
    CutoffAdmissibilityMode, CutoffSourceInput, ForecastLedger, IssuanceError, OutcomeDefinition,
    OutcomeResolutionRecord, ResolutionState, RevisionRationale,
};
use oia_forecast::market_effects::{
    AssumptionSensitivityPoint, CatalystDomain, CatalystInputRecord, ConditionalBranchResponse,
    ConditionalMarketScenarioSpec, ExpectationSurpriseInput, ExposureMechanismKind,
    ExposurePathwayCitation, MarketCohortEvaluationItem, MarketDirection, MarketEffectError,
    MarketEffectForecastBook, MarketEffectForecastDraft, MarketEffectPrediction,
    MarketForecastMode, MarketForecastModeSpec, MarketLeakageKind, MarketScalarResolutionDraft,
    MarketScalarResolutionState, NoveltyStatus, ORDER_EXECUTION_NOT_SUPPORTED_CAVEAT,
    PrecisionSupport, QualitativeDisposition, QualitativeMarketOutput,
    QuantitativeMarketPrediction, REAL_WORLD_EFFECT_NOT_INCREMENTAL_PRICE_CAVEAT,
    SIMULATED_STRATEGY_NOT_OBSERVED_RETURN_CAVEAT, ScenarioAssumptionParameter,
    SimulatedFrictionConfig,
};
use oia_forecast::scoring::{Probability, UnresolvedReason};

fn base_fixture() -> Result<
    (
        ForecastLedger,
        MarketEffectForecastBook,
        MarketEffectForecastDraft,
    ),
    Box<dyn std::error::Error>,
> {
    let ledger = ForecastLedger::new(64)?;
    let book = MarketEffectForecastBook::new(64)?;

    let cutoff_ms = 1_770_000_000_000_i64;
    let origin_ms = 1_770_000_060_000_i64;
    let issued_ms = 1_770_000_120_000_i64;
    let horizon_start_ms = 1_770_010_000_000_i64;
    let horizon_end_ms = 1_770_269_200_000_i64;

    let weather_src = VersionId::new("source:weather-gfs-v1")?;
    let filing_src = VersionId::new("source:filing-10k-v1")?;
    let consensus_src = VersionId::new("source:consensus-v1")?;

    let sources = vec![
        CutoffSourceInput {
            source_version: weather_src.clone(),
            event_utc_ms: Some(cutoff_ms - 7_200_000),
            public_available_at_utc_ms: Some(cutoff_ms - 3_600_000),
            ingested_at_utc_ms: Some(cutoff_ms - 1_800_000),
        },
        CutoffSourceInput {
            source_version: filing_src.clone(),
            event_utc_ms: Some(cutoff_ms - 86_400_000),
            public_available_at_utc_ms: Some(cutoff_ms - 43_200_000),
            ingested_at_utc_ms: Some(cutoff_ms - 21_600_000),
        },
        CutoffSourceInput {
            source_version: consensus_src.clone(),
            event_utc_ms: Some(cutoff_ms - 86_400_000),
            public_available_at_utc_ms: Some(cutoff_ms - 43_200_000),
            ingested_at_utc_ms: Some(cutoff_ms - 21_600_000),
        },
    ];

    let target_inst = OpaqueId::new("instrument:synthetic-semi-01")?;
    let catalyst_id = OpaqueId::new("catalyst:weather-freeze-v1")?;
    let param_id = OpaqueId::new("param:outage-hours")?;

    let scenario = ConditionalMarketScenarioSpec {
        scenario_id: OpaqueId::new("scenario:freeze-outage-v1")?,
        trigger_event_id: OpaqueId::new("event:grid-curtailment")?,
        trigger_proposition: "Grid curtailment occurs during freeze window.".to_owned(),
        trigger_occurrence_probability: Probability::new(0.60)?,
        response_given_event: ConditionalBranchResponse {
            branch_label: "Curtailment (E)".to_owned(),
            expected_direction: MarketDirection::Negative,
            conditional_outcome_probability: Probability::new(0.80)?,
            conditional_point_estimate: -160.0,
            conditional_interval: PredictionInterval::new(-250.0, -70.0, 0.80)?,
        },
        response_given_non_event: Some(ConditionalBranchResponse {
            branch_label: "No curtailment (not E)".to_owned(),
            expected_direction: MarketDirection::MutedOrAlreadyPriced,
            conditional_outcome_probability: Probability::new(0.20)?,
            conditional_point_estimate: -15.0,
            conditional_interval: PredictionInterval::new(-60.0, 30.0, 0.80)?,
        }),
    };

    let draft = MarketEffectForecastDraft {
        forecast_id: OpaqueId::new("forecast:semi-freeze-series")?,
        version_id: VersionId::new("market-vintage:v1")?,
        schema_version: SchemaVersion::V1,
        actor: ActorId::new("actor:analyst")?,
        issued_at_utc_ms: issued_ms,
        origin_utc_ms: origin_ms,
        information_cutoff_utc_ms: cutoff_ms,
        horizon_start_utc_ms: horizon_start_ms,
        horizon_end_utc_ms: horizon_end_ms,
        cutoff_mode: CutoffAdmissibilityMode::ProspectiveStrict,
        target_instrument_id: target_inst.clone(),
        instrument_selection_as_of_utc_ms: cutoff_ms - 86_400_000,
        baseline_version: VersionId::new("baseline:unconditional-v1")?,
        model_version: VersionId::new("model:scenario-impact-v1")?,
        model_knowledge_cutoff_utc_ms: Some(cutoff_ms - 86_400_000),
        mode: MarketForecastModeSpec::ConditionalScenario(scenario),
        catalysts: vec![CatalystInputRecord {
            catalyst_id: catalyst_id.clone(),
            source_version: weather_src.clone(),
            domain: CatalystDomain::WeatherForecastVintage {
                weather_model_id: OpaqueId::new("weather:gfs")?,
                model_run_origin_utc_ms: cutoff_ms - 7_200_000,
                valid_window_start_utc_ms: horizon_start_ms,
                valid_window_end_utc_ms: horizon_end_ms,
                is_observed_post_hoc_weather: false,
            },
            summary: "12Z ensemble weather forecast vintage".to_owned(),
            historical_event_utc_ms: cutoff_ms - 7_200_000,
            public_release_utc_ms: Some(cutoff_ms - 3_600_000),
            market_awareness_utc_ms: Some(cutoff_ms - 3_000_000),
            ingested_at_utc_ms: Some(cutoff_ms - 1_800_000),
            revision_vintage_public_release_utc_ms: None,
        }],
        exposure_pathways: vec![ExposurePathwayCitation {
            pathway_id: OpaqueId::new("pathway:fab-power")?,
            catalyst_id,
            subject_entity_or_facility_id: OpaqueId::new("facility:fab-1")?,
            target_instrument_id: target_inst,
            mechanism_kind: ExposureMechanismKind::WeatherOrPhysicalDisruption,
            mechanism_explanation: "Freeze risk threatens unhedged power supply at Fab 1.".to_owned(),
            expected_direction: MarketDirection::Negative,
            cited_evidence_versions: vec![weather_src.clone(), filing_src],
        }],
        novelty_status: NoveltyStatus::ScheduledReleaseWithConsensus,
        expectation_surprise: Some(ExpectationSurpriseInput {
            metric_unit: OpaqueId::new("unit:celsius_anomaly")?,
            prior_expectation_value: -1.5,
            expectation_source_version: consensus_src,
            realized_release_value: -6.0,
            realized_source_version: weather_src,
            stated_surprise: -4.5,
        }),
        real_world_vs_price_effect_rationale: "Short power dips are buffered by backup generation; multi-shift outage is required for incremental price move.".to_owned(),
        assumption_parameters: vec![ScenarioAssumptionParameter {
            parameter_id: param_id.clone(),
            label: "Curtailment duration".to_owned(),
            unit: OpaqueId::new("unit:hours")?,
            low_value: 4.0,
            base_value: 20.0,
            high_value: 40.0,
            rationale: "Historical freeze curtailment bounds.".to_owned(),
        }],
        sensitivity_points: vec![AssumptionSensitivityPoint {
            parameter_id: param_id,
            low_case_effect: 10.0,
            base_case_effect: -102.0,
            high_case_effect: -200.0,
            direction_flips_across_range: true,
            sensitivity_note: "Brief curtailment yields relief bounce (+10 bps); prolonged curtailment yields -200 bps.".to_owned(),
        }],
        precision_support: PrecisionSupport::SupportedByAvailableEvidence {
            rationale: "Facility power disclosures and ensemble spread support bounded CAR range.".to_owned(),
        },
        // P(M) = 0.60 * 0.80 + 0.40 * 0.20 = 0.56
        prediction: MarketEffectPrediction::Quantitative(QuantitativeMarketPrediction {
            effect_unit: OpaqueId::new("unit:car_bps")?,
            expected_direction: MarketDirection::Negative,
            point_estimate: -102.0,
            prediction_interval: PredictionInterval::new(-180.0, -30.0, 0.80)?,
            baseline_point_estimate: 0.0,
            baseline_interval: PredictionInterval::new(-100.0, 100.0, 0.80)?,
            outcome_definition: OutcomeDefinition {
                outcome_id: OpaqueId::new("outcome:semi-car-below-minus-100bps")?,
                proposition: "3-session CAR <= -100 bps".to_owned(),
                resolution_rules: "Resolve true iff 3-session CAR <= -100.0 bps".to_owned(),
            },
            forecast_probability: Probability::new(0.56)?,
            baseline_probability: Probability::new(0.20)?,
        }),
        sources,
        comparison_set: vec![VersionId::new("precedent:freeze-2021-v1")?],
        predecessor_version: None,
        revision_rationale: None,
    };

    Ok((ledger, book, draft))
}

/// Risk/Requirement (`MARKET-03`, `A14`): Conditional scenario forecasts must keep event
/// occurrence probability `P(E)` separate from conditional market response `R | E`, refuse
/// quantitative conditional forecasts that omit `R | ¬E` or conflate `P(E)` with `P(M)`, and
/// enforce cited exposure pathways.
#[test]
fn separates_event_probability_from_conditional_market_response_and_validates_exposure_citations()
-> Result<(), Box<dyn std::error::Error>> {
    let (mut ledger, mut book, base_draft) = base_fixture()?;

    // 1. Conflating P(E) = 0.60 with unconditional forecast_probability = 0.60 (instead of 0.56) is refused.
    let mut conflated = base_draft.clone();
    if let MarketEffectPrediction::Quantitative(ref mut quant) = conflated.prediction {
        quant.forecast_probability = Probability::new(0.60)?;
    }
    assert!(matches!(
        book.issue_market_effect_forecast(&mut ledger, conflated),
        Err(MarketEffectError::ConditionalScenarioContractViolation(_))
    ));

    // 2. Omitting the complementary branch response_given_non_event on a quantitative scenario is refused.
    let mut missing_comp = base_draft.clone();
    if let MarketForecastModeSpec::ConditionalScenario(ref mut scen) = missing_comp.mode {
        scen.response_given_non_event = None;
    }
    assert!(matches!(
        book.issue_market_effect_forecast(&mut ledger, missing_comp),
        Err(MarketEffectError::ConditionalScenarioContractViolation(_))
    ));

    // 3. Citing an exposure pathway evidence version absent from admissible sources is refused.
    let mut uncited_pathway = base_draft.clone();
    uncited_pathway.exposure_pathways[0]
        .cited_evidence_versions
        .push(VersionId::new("source:unregistered-v1")?);
    assert!(matches!(
        book.issue_market_effect_forecast(&mut ledger, uncited_pathway),
        Err(MarketEffectError::UncitedSourceVersion {
            context: "exposure_pathway.cited_evidence_versions",
            ..
        })
    ));

    // 4. Valid conditional scenario issues cleanly and preserves distinct P(E), P(M|E), P(M|¬E), and P(M).
    let issued = book.issue_market_effect_forecast(&mut ledger, base_draft)?;
    match issued.mode() {
        MarketForecastMode::ConditionalScenario(scen) => {
            assert_eq!(scen.trigger_occurrence_probability().value(), 0.60);
            assert_eq!(
                scen.response_given_event()
                    .conditional_outcome_probability
                    .value(),
                0.80
            );
            assert_eq!(
                scen.response_given_non_event()
                    .map(|b| b.conditional_outcome_probability.value()),
                Some(0.20)
            );
            assert!(
                (scen
                    .implied_unconditional_outcome_probability()
                    .map(|p| p.value())
                    .unwrap_or(0.0)
                    - 0.56)
                    .abs()
                    < 1e-12
            );
        }
        MarketForecastMode::CurrentCondition => {
            return Err("expected ConditionalScenario mode".into());
        }
    }
    assert_eq!(
        issued
            .expectation_and_novelty()
            .real_world_vs_incremental_price_caveat(),
        REAL_WORLD_EFFECT_NOT_INCREMENTAL_PRICE_CAVEAT
    );

    Ok(())
}

/// Risk/Requirement (`MARKET-04`, `MARKET-05`, `A14`): Detects and refuses point-in-time leakage
/// from post-hoc observed weather, revised economic releases, news published after an event but
/// after cutoff, market awareness after cutoff, post-cutoff instrument selection, and post-cutoff
/// model knowledge under `ProspectiveStrict`.
#[test]
fn refuses_point_in_time_leakage_across_weather_revisions_news_instruments_and_models()
-> Result<(), Box<dyn std::error::Error>> {
    let (mut ledger, mut book, base_draft) = base_fixture()?;
    let cutoff_ms = base_draft.information_cutoff_utc_ms;

    // 1. Post-hoc observed weather disguised as a weather forecast vintage.
    let mut obs_weather = base_draft.clone();
    obs_weather.catalysts[0].domain = CatalystDomain::WeatherForecastVintage {
        weather_model_id: OpaqueId::new("weather:station-obs")?,
        model_run_origin_utc_ms: cutoff_ms - 7_200_000,
        valid_window_start_utc_ms: base_draft.horizon_start_utc_ms,
        valid_window_end_utc_ms: base_draft.horizon_end_utc_ms,
        is_observed_post_hoc_weather: true,
    };
    assert!(matches!(
        book.issue_market_effect_forecast(&mut ledger, obs_weather),
        Err(MarketEffectError::LeakageDetected(
            MarketLeakageKind::ObservedWeatherUsedInsteadOfForecastVintage { .. }
        ))
    ));

    // 2. Revised economic release vintage published after information_cutoff_utc_ms.
    let mut revised_leak = base_draft.clone();
    revised_leak.catalysts[0].revision_vintage_public_release_utc_ms = Some(cutoff_ms + 60_000);
    assert!(matches!(
        book.issue_market_effect_forecast(&mut ledger, revised_leak),
        Err(MarketEffectError::LeakageDetected(
            MarketLeakageKind::RevisedReleaseVintageAfterCutoff { .. }
        ))
    ));

    // 3. Historical event occurred before cutoff, but public release (later news) occurred after cutoff.
    let mut late_news = base_draft.clone();
    late_news.catalysts[0].historical_event_utc_ms = cutoff_ms - 86_400_000;
    late_news.catalysts[0].public_release_utc_ms = Some(cutoff_ms + 1_000);
    late_news.catalysts[0].market_awareness_utc_ms = Some(cutoff_ms + 2_000);
    assert!(matches!(
        book.issue_market_effect_forecast(&mut ledger, late_news),
        Err(MarketEffectError::LeakageDetected(
            MarketLeakageKind::LaterNewsOrReleaseAfterEventCutoff { .. }
        ))
    ));

    // 4. Public release occurred before cutoff, but market awareness occurred after cutoff.
    let mut late_awareness = base_draft.clone();
    late_awareness.catalysts[0].public_release_utc_ms = Some(cutoff_ms - 1_000);
    late_awareness.catalysts[0].market_awareness_utc_ms = Some(cutoff_ms + 500);
    assert!(matches!(
        book.issue_market_effect_forecast(&mut ledger, late_awareness),
        Err(MarketEffectError::LeakageDetected(
            MarketLeakageKind::MarketAwarenessAfterCutoff { .. }
        ))
    ));

    // 5. Instrument selection / universe snapshot after cutoff.
    let mut survivorship_leak = base_draft.clone();
    survivorship_leak.instrument_selection_as_of_utc_ms = cutoff_ms + 1;
    assert!(matches!(
        book.issue_market_effect_forecast(&mut ledger, survivorship_leak),
        Err(MarketEffectError::LeakageDetected(
            MarketLeakageKind::InstrumentSelectionAfterCutoff { .. }
        ))
    ));

    // 6. Post-cutoff language-model knowledge under ProspectiveStrict.
    let mut model_leak = base_draft;
    model_leak.model_knowledge_cutoff_utc_ms = Some(cutoff_ms + 1);
    assert!(matches!(
        book.issue_market_effect_forecast(&mut ledger, model_leak),
        Err(MarketEffectError::Issuance(
            IssuanceError::ModelKnowledgeAfterStrictCutoff { .. }
        ))
    ));

    Ok(())
}

/// Risk/Requirement (`MARKET-04`): Enforces surprise consistency (`realized - prior`), refuses
/// non-muted quantitative shocks from redundant release restatements, validates sensitivity sign
/// flips, and requires `QualitativeOrInsufficient` outputs when precision is unsupported.
#[test]
fn enforces_surprise_novelty_sensitivity_and_qualitative_fallback_contracts()
-> Result<(), Box<dyn std::error::Error>> {
    let (mut ledger, mut book, base_draft) = base_fixture()?;

    // 1. Mismatched stated_surprise is refused.
    let mut bad_surprise = base_draft.clone();
    if let Some(ref mut surp) = bad_surprise.expectation_surprise {
        surp.stated_surprise = -1.0; // actual is -6.0 - (-1.5) = -4.5
    }
    assert!(matches!(
        book.issue_market_effect_forecast(&mut ledger, bad_surprise),
        Err(MarketEffectError::InvalidExpectationSurprise(_))
    ));

    // 2. Redundant restatement of a prior release claiming a non-muted quantitative price shock is refused.
    let mut redundant_shock = base_draft.clone();
    redundant_shock.novelty_status = NoveltyStatus::RedundantRestatementOfPriorRelease {
        original_release_version: VersionId::new("source:filing-10k-v1")?,
    };
    assert!(matches!(
        book.issue_market_effect_forecast(&mut ledger, redundant_shock),
        Err(MarketEffectError::RedundantReleaseClaimsIncrementalShock { .. })
    ));

    // 3. Incorrect direction_flips_across_range flag on sensitivity point is refused.
    let mut bad_sens = base_draft.clone();
    bad_sens.sensitivity_points[0].direction_flips_across_range = false; // +10.0 to -200.0 flips sign
    assert!(matches!(
        book.issue_market_effect_forecast(&mut ledger, bad_sens),
        Err(MarketEffectError::InvalidAssumptionOrSensitivity(_))
    ));

    // 4. UnsupportedPrecision paired with a Quantitative prediction is refused.
    let mut unsupported_quant = base_draft.clone();
    unsupported_quant.precision_support = PrecisionSupport::UnsupportedPrecision {
        reason: "No historical liquidity or exposure volume.".to_owned(),
    };
    assert!(matches!(
        book.issue_market_effect_forecast(&mut ledger, unsupported_quant),
        Err(MarketEffectError::PrecisionSupportMismatch(_))
    ));

    // 5. UnsupportedPrecision paired with QualitativeOrInsufficient succeeds.
    let mut valid_qual = base_draft;
    valid_qual.precision_support = PrecisionSupport::UnsupportedPrecision {
        reason: "No historical liquidity or exposure volume.".to_owned(),
    };
    valid_qual.prediction =
        MarketEffectPrediction::QualitativeOrInsufficient(QualitativeMarketOutput {
            disposition: QualitativeDisposition::DirectionalOnly {
                direction: MarketDirection::Negative,
            },
            explanation: "Directional headwind expected without supported numeric interval."
                .to_owned(),
            missing_or_limiting_evidence: "Missing facility revenue share.".to_owned(),
        });
    let issued_qual = book.issue_market_effect_forecast(&mut ledger, valid_qual)?;
    assert!(matches!(
        issued_qual.prediction(),
        MarketEffectPrediction::QualitativeOrInsufficient(_)
    ));

    Ok(())
}

/// Risk/Requirement (`MARKET-05`, `A14`): Preserves original and revised vintages, refuses cohort
/// evaluations that drop losing earlier vintages, records interval and directional misses alongside
/// unresolved/invalidated outcomes, and enforces positive friction and non-execution boundaries on
/// hypothetical strategy simulations.
#[test]
fn preserves_vintages_misses_and_enforces_simulation_friction_in_cohort_evaluation()
-> Result<(), Box<dyn std::error::Error>> {
    let (mut ledger, mut book, draft_v1) = base_fixture()?;
    let v1 = book
        .issue_market_effect_forecast(&mut ledger, draft_v1.clone())?
        .clone();

    // Issue revision v2 citing new admissible macro evidence.
    let macro_src = VersionId::new("source:macro-release-v1")?;
    let cutoff_v2 = draft_v1.information_cutoff_utc_ms + 600_000;
    let mut sources_v2 = draft_v1.sources.clone();
    sources_v2.push(CutoffSourceInput {
        source_version: macro_src.clone(),
        event_utc_ms: Some(draft_v1.information_cutoff_utc_ms - 86_400_000),
        public_available_at_utc_ms: Some(cutoff_v2 - 120_000),
        ingested_at_utc_ms: Some(cutoff_v2 - 60_000),
    });

    let mut draft_v2 = draft_v1.clone();
    draft_v2.version_id = VersionId::new("market-vintage:v2")?;
    draft_v2.information_cutoff_utc_ms = cutoff_v2;
    draft_v2.origin_utc_ms = cutoff_v2 + 30_000;
    draft_v2.issued_at_utc_ms = cutoff_v2 + 60_000;
    draft_v2.sources = sources_v2;
    draft_v2.sensitivity_points[0].base_case_effect = -210.0;
    if let MarketEffectPrediction::Quantitative(ref mut quant) = draft_v2.prediction {
        quant.point_estimate = -210.0;
        quant.prediction_interval = PredictionInterval::new(-260.0, -150.0, 0.80)?;
    }
    draft_v2.predecessor_version = Some(v1.version_id().clone());
    draft_v2.revision_rationale = Some(RevisionRationale {
        new_evidence_versions: vec![macro_src],
        summary: "Updated point estimate and interval after macro inventory release.".to_owned(),
    });
    let v2 = book
        .issue_market_effect_forecast(&mut ledger, draft_v2)?
        .clone();

    // Add a second series with an Unresolved (Censored) scalar outcome to verify unresolved accounting.
    let mut draft_unresolved = draft_v1.clone();
    draft_unresolved.forecast_id = OpaqueId::new("forecast:semi-unresolved-series")?;
    draft_unresolved.version_id = VersionId::new("market-vintage:unresolved-v1")?;
    if let MarketEffectPrediction::Quantitative(ref mut quant) = draft_unresolved.prediction {
        quant.outcome_definition = OutcomeDefinition {
            outcome_id: OpaqueId::new("outcome:semi-unresolved-outcome")?,
            proposition: "Secondary series CAR <= -100 bps".to_owned(),
            resolution_rules: "Resolve true iff CAR <= -100.0 bps".to_owned(),
        };
    }
    let v_unres = book
        .issue_market_effect_forecast(&mut ledger, draft_unresolved)?
        .clone();

    // Record binary and scalar outcomes: realized CAR = -205.0 bps (misses v1 interval [-180, -30], covered by v2 interval [-260, -150]).
    let bin_res_ver = VersionId::new("resolution:binary-v1")?;
    ledger.record_outcome_resolution(OutcomeResolutionRecord {
        resolution_id: OpaqueId::new("res-series:binary-1")?,
        version_id: bin_res_ver.clone(),
        outcome_id: OpaqueId::new("outcome:semi-car-below-minus-100bps")?,
        actor: ActorId::new("actor:eval")?,
        recorded_at_utc_ms: draft_v1.horizon_end_utc_ms + 60_000,
        evidence_versions: vec![VersionId::new("source:event-study-v1")?],
        state: ResolutionState::Resolved {
            occurred: true,
            observed_event_utc_ms: Some(draft_v1.horizon_end_utc_ms - 10_000),
        },
        supersedes_version: None,
        correction_rationale: None,
    })?;

    let bin_unres_ver = VersionId::new("resolution:binary-unres-v1")?;
    ledger.record_outcome_resolution(OutcomeResolutionRecord {
        resolution_id: OpaqueId::new("res-series:binary-2")?,
        version_id: bin_unres_ver.clone(),
        outcome_id: OpaqueId::new("outcome:semi-unresolved-outcome")?,
        actor: ActorId::new("actor:eval")?,
        recorded_at_utc_ms: draft_v1.horizon_end_utc_ms + 60_000,
        evidence_versions: vec![],
        state: ResolutionState::Unresolved(UnresolvedReason::Censored),
        supersedes_version: None,
        correction_rationale: None,
    })?;

    let scalar_res_ver = VersionId::new("resolution:scalar-v1")?;
    book.record_scalar_resolution(MarketScalarResolutionDraft {
        resolution_id: OpaqueId::new("res-series:scalar-1")?,
        version_id: scalar_res_ver.clone(),
        target_instrument_id: draft_v1.target_instrument_id.clone(),
        effect_unit: OpaqueId::new("unit:car_bps")?,
        horizon_start_utc_ms: draft_v1.horizon_start_utc_ms,
        horizon_end_utc_ms: draft_v1.horizon_end_utc_ms,
        actor: ActorId::new("actor:eval")?,
        recorded_at_utc_ms: draft_v1.horizon_end_utc_ms + 60_000,
        evidence_versions: vec![VersionId::new("source:event-study-v1")?],
        state: MarketScalarResolutionState::Resolved {
            realized_value: -205.0,
            observed_at_utc_ms: draft_v1.horizon_end_utc_ms - 10_000,
        },
    })?;

    let scalar_unres_ver = VersionId::new("resolution:scalar-unres-v1")?;
    book.record_scalar_resolution(MarketScalarResolutionDraft {
        resolution_id: OpaqueId::new("res-series:scalar-2")?,
        version_id: scalar_unres_ver.clone(),
        target_instrument_id: draft_v1.target_instrument_id,
        effect_unit: OpaqueId::new("unit:car_bps")?,
        horizon_start_utc_ms: draft_v1.horizon_start_utc_ms,
        horizon_end_utc_ms: draft_v1.horizon_end_utc_ms,
        actor: ActorId::new("actor:eval")?,
        recorded_at_utc_ms: draft_v1.horizon_end_utc_ms + 60_000,
        evidence_versions: vec![],
        state: MarketScalarResolutionState::Unresolved(UnresolvedReason::Censored),
    })?;

    // Evaluating v2 without v1 is refused so losing original vintages cannot be omitted.
    let omitted_v1 = book.evaluate_market_effect_cohort(
        &mut ledger,
        VersionId::new("eval:bad-v1")?,
        Some(VersionId::new("eval:bin-bad-v1")?),
        draft_v1.horizon_end_utc_ms + 120_000,
        &[MarketCohortEvaluationItem {
            market_vintage_version: v2.version_id().clone(),
            binary_resolution_version: Some(bin_res_ver.clone()),
            scalar_resolution_version: Some(scalar_res_ver.clone()),
        }],
        16,
    );
    assert!(matches!(
        omitted_v1,
        Err(MarketEffectError::IncompleteVintageSeriesInCohort { .. })
    ));

    // Full cohort evaluation preserves v1 miss, v2 coverage, and unresolved row.
    let eval = book.evaluate_market_effect_cohort(
        &mut ledger,
        VersionId::new("eval:good-v1")?,
        Some(VersionId::new("eval:bin-good-v1")?),
        draft_v1.horizon_end_utc_ms + 120_000,
        &[
            MarketCohortEvaluationItem {
                market_vintage_version: v1.version_id().clone(),
                binary_resolution_version: Some(bin_res_ver.clone()),
                scalar_resolution_version: Some(scalar_res_ver.clone()),
            },
            MarketCohortEvaluationItem {
                market_vintage_version: v2.version_id().clone(),
                binary_resolution_version: Some(bin_res_ver),
                scalar_resolution_version: Some(scalar_res_ver.clone()),
            },
            MarketCohortEvaluationItem {
                market_vintage_version: v_unres.version_id().clone(),
                binary_resolution_version: Some(bin_unres_ver),
                scalar_resolution_version: Some(scalar_unres_ver),
            },
        ],
        16,
    )?;
    assert_eq!(eval.quantitative_count(), 3);
    assert_eq!(eval.resolved_scalar_count(), 2);
    assert_eq!(eval.unresolved_scalar_count(), 1);
    assert_eq!(eval.forecast_interval_coverage_ratio(), (1, 2));
    assert_eq!(eval.baseline_interval_coverage_ratio(), (0, 2));
    assert_eq!(eval.misses().len(), 1);
    assert_eq!(eval.misses()[0].forecast_version, *v1.version_id());
    assert!((eval.misses()[0].interval_excursion - 25.0).abs() < 1e-12);
    assert!(eval.misses()[0].interval_missed);
    assert!(!eval.misses()[0].direction_missed);

    // Strategy simulation refuses zero friction and deducts round-trip friction on valid config.
    assert!(matches!(
        book.evaluate_simulated_strategy(
            VersionId::new("sim:zero-friction")?,
            v2.version_id(),
            &scalar_res_ver,
            SimulatedFrictionConfig {
                commission_bps_per_side: 0.0,
                half_spread_bps_per_side: 0.0,
                slippage_bps_per_side: 0.0,
            },
        ),
        Err(MarketEffectError::InvalidStrategySimulation(_))
    ));

    let sim = book.evaluate_simulated_strategy(
        VersionId::new("sim:valid-friction")?,
        v2.version_id(),
        &scalar_res_ver,
        SimulatedFrictionConfig {
            commission_bps_per_side: 2.0,
            half_spread_bps_per_side: 3.0,
            slippage_bps_per_side: 5.0,
        },
    )?;
    assert!((sim.gross_simulated_return_bps() - 205.0).abs() < 1e-12);
    assert!((sim.round_trip_friction_deducted_bps() - 20.0).abs() < 1e-12);
    assert!((sim.net_simulated_return_bps() - 185.0).abs() < 1e-12);
    assert!(!sim.is_observed_market_return());
    assert!(!sim.can_execute_orders());
    assert_eq!(
        sim.simulation_caveat(),
        SIMULATED_STRATEGY_NOT_OBSERVED_RETURN_CAVEAT
    );
    assert_eq!(
        sim.execution_boundary_caveat(),
        ORDER_EXECUTION_NOT_SUPPORTED_CAVEAT
    );

    Ok(())
}
