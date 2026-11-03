//! Deterministic synthetic `A14` inspection example for `oia_forecast::market_effects` (`#44`).
//!
//! Demonstrates:
//! 1. Point-in-time conditional market-effect forecast issuance (`v1`) combining a weather
//!    forecast vintage and a declassified logistics notice with cited exposure pathways,
//!    explicit separation of trigger probability `P(E)` from conditional response `R | E`,
//!    adjustable assumption ranges, and sensitivity analysis.
//! 2. Linked revision vintage (`v2`) after an admissible macroeconomic release vintage
//!    arrives with an explicit consensus surprise calculation.
//! 3. Qualitative / insufficient-evidence forecast issuance when quantitative precision is
//!    unsupported (`MARKET-04`).
//! 4. Point-in-time leakage refusal for a revised economic release published after the cutoff,
//!    post-event news published after the cutoff, and post-hoc observed weather (`MARKET-05`, `A14`).
//! 5. Chronological holdout evaluation preserving the original vintage miss alongside the
//!    revised vintage, central prediction-interval coverage, Brier scores, and a
//!    friction-disclosed hypothetical strategy simulation (`MARKET-05`, `A14`).

use oia_domain::{ActorId, OpaqueId, SchemaVersion, VersionId};
use oia_forecast::interval::PredictionInterval;
use oia_forecast::issuance::{
    CutoffAdmissibilityMode, CutoffSourceInput, ForecastLedger, OutcomeDefinition,
    OutcomeResolutionRecord, ResolutionState, RevisionRationale,
};
use oia_forecast::market_effects::{
    AssumptionSensitivityPoint, CatalystDomain, CatalystInputRecord, ConditionalBranchResponse,
    ConditionalMarketScenarioSpec, ExpectationSurpriseInput, ExposureMechanismKind,
    ExposurePathwayCitation, MarketCohortEvaluationItem, MarketDirection, MarketEffectError,
    MarketEffectForecastBook, MarketEffectForecastDraft, MarketEffectPrediction,
    MarketForecastModeSpec, MarketLeakageKind, MarketScalarResolutionDraft,
    MarketScalarResolutionState, NoveltyStatus, PrecisionSupport, QualitativeDisposition,
    QualitativeMarketOutput, QuantitativeMarketPrediction, ScenarioAssumptionParameter,
    SimulatedFrictionConfig,
};
use oia_forecast::scoring::Probability;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut ledger = ForecastLedger::new(128)?;
    let mut book = MarketEffectForecastBook::new(128)?;

    let cutoff_v1_ms = 1_770_000_000_000_i64;
    let origin_v1_ms = 1_770_000_060_000_i64;
    let issued_v1_ms = 1_770_000_120_000_i64;
    let horizon_start_ms = 1_770_010_000_000_i64;
    let horizon_end_ms = 1_770_269_200_000_i64;

    let weather_src = VersionId::new("source:nws-gfs-20260201t12z-v1")?;
    let declass_src = VersionId::new("source:doe-declass-corridor-v1")?;
    let filing_src = VersionId::new("source:issuer-10k-facility-map-v1")?;

    let sources_v1 = vec![
        CutoffSourceInput {
            source_version: weather_src.clone(),
            event_utc_ms: Some(cutoff_v1_ms - 7_200_000),
            public_available_at_utc_ms: Some(cutoff_v1_ms - 3_600_000),
            ingested_at_utc_ms: Some(cutoff_v1_ms - 1_800_000),
        },
        CutoffSourceInput {
            source_version: declass_src.clone(),
            event_utc_ms: Some(cutoff_v1_ms - 86_400_000),
            public_available_at_utc_ms: Some(cutoff_v1_ms - 7_200_000),
            ingested_at_utc_ms: Some(cutoff_v1_ms - 3_600_000),
        },
        CutoffSourceInput {
            source_version: filing_src.clone(),
            event_utc_ms: Some(cutoff_v1_ms - 604_800_000),
            public_available_at_utc_ms: Some(cutoff_v1_ms - 500_000_000),
            ingested_at_utc_ms: Some(cutoff_v1_ms - 400_000_000),
        },
    ];

    let catalysts_v1 = vec![
        CatalystInputRecord {
            catalyst_id: OpaqueId::new("catalyst: synthetic-freeze-forecast-v1")
                .or_else(|_| OpaqueId::new("catalyst:synthetic-freeze-forecast-v1"))?,
            source_version: weather_src.clone(),
            domain: CatalystDomain::WeatherForecastVintage {
                weather_model_id: OpaqueId::new("weather:gfs-ensemble-0p25")?,
                model_run_origin_utc_ms: cutoff_v1_ms - 7_200_000,
                valid_window_start_utc_ms: horizon_start_ms,
                valid_window_end_utc_ms: horizon_end_ms,
                is_observed_post_hoc_weather: false,
            },
            summary: "[SYNTHETIC FIXTURE] 12Z ensemble weather forecast vintage projecting coastal corridor freeze risk".to_owned(),
            historical_event_utc_ms: cutoff_v1_ms - 7_200_000,
            public_release_utc_ms: Some(cutoff_v1_ms - 3_600_000),
            market_awareness_utc_ms: Some(cutoff_v1_ms - 3_000_000),
            ingested_at_utc_ms: Some(cutoff_v1_ms - 1_800_000),
            revision_vintage_public_release_utc_ms: None,
        },
        CatalystInputRecord {
            catalyst_id: OpaqueId::new("catalyst:synthetic-declass-notice-v1")?,
            source_version: declass_src.clone(),
            domain: CatalystDomain::DeclassificationOrPublicRelease {
                releasing_authority: "Synthetic Archival Declassification Board".to_owned(),
                original_record_utc_ms: Some(cutoff_v1_ms - 86_400_000),
            },
            summary: "[SYNTHETIC FIXTURE] Declassified corridor maintenance schedule affecting specialty gas transport".to_owned(),
            historical_event_utc_ms: cutoff_v1_ms - 86_400_000,
            public_release_utc_ms: Some(cutoff_v1_ms - 7_200_000),
            market_awareness_utc_ms: Some(cutoff_v1_ms - 5_400_000),
            ingested_at_utc_ms: Some(cutoff_v1_ms - 3_600_000),
            revision_vintage_public_release_utc_ms: None,
        },
    ];

    let target_inst = OpaqueId::new("instrument:synthetic-aero-semi-01")?;
    let unit_bps = OpaqueId::new("unit:car_bps")?;

    let pathways_v1 = vec![
        ExposurePathwayCitation {
            pathway_id: OpaqueId::new("pathway:weather-fab-utility")?,
            catalyst_id: OpaqueId::new("catalyst:synthetic-freeze-forecast-v1")?,
            subject_entity_or_facility_id: OpaqueId::new("facility:coastal-fab-south")?,
            target_instrument_id: target_inst.clone(),
            mechanism_kind: ExposureMechanismKind::WeatherOrPhysicalDisruption,
            mechanism_explanation: "Projected sub-freezing curtailment window reduces unhedged fab utility uptime over the 3-session window.".to_owned(),
            expected_direction: MarketDirection::Negative,
            cited_evidence_versions: vec![weather_src.clone(), filing_src.clone()],
        },
        ExposurePathwayCitation {
            pathway_id: OpaqueId::new("pathway:declass-specialty-gas")?,
            catalyst_id: OpaqueId::new("catalyst:synthetic-declass-notice-v1")?,
            subject_entity_or_facility_id: OpaqueId::new("corridor: rail-sector-4")
                .or_else(|_| OpaqueId::new("corridor:rail-sector-4"))?,
            target_instrument_id: target_inst.clone(),
            mechanism_kind: ExposureMechanismKind::SupplyChainInputOrLogistics,
            mechanism_explanation: "Declassified maintenance schedule tightens specialty gas buffer inventory during cold snaps.".to_owned(),
            expected_direction: MarketDirection::Negative,
            cited_evidence_versions: vec![declass_src.clone(), filing_src.clone()],
        },
    ];

    // Trigger event probability P(E) = 0.60, conditional P(M | E) = 0.75, P(M | not E) = 0.25
    // => unconditional P(M) = 0.60 * 0.75 + 0.40 * 0.25 = 0.55.
    let scenario_v1 = ConditionalMarketScenarioSpec {
        scenario_id: OpaqueId::new("scenario:freeze-curtailment-v1")?,
        trigger_event_id: OpaqueId::new("event:grid-curtailment-stage2")?,
        trigger_proposition: "Regional grid operator declares Stage-2 industrial curtailment inside the 72-hour window.".to_owned(),
        trigger_occurrence_probability: Probability::new(0.60)?,
        response_given_event: ConditionalBranchResponse {
            branch_label: "Curtailment occurs (E)".to_owned(),
            expected_direction: MarketDirection::Negative,
            conditional_outcome_probability: Probability::new(0.75)?,
            conditional_point_estimate: -140.0,
            conditional_interval: PredictionInterval::new(-240.0, -40.0, 0.80)?,
        },
        response_given_non_event: Some(ConditionalBranchResponse {
            branch_label: "No curtailment (not E)".to_owned(),
            expected_direction: MarketDirection::MutedOrAlreadyPriced,
            conditional_outcome_probability: Probability::new(0.25)?,
            conditional_point_estimate: -20.0,
            conditional_interval: PredictionInterval::new(-80.0, 40.0, 0.80)?,
        }),
    };

    let outcome_def = OutcomeDefinition {
        outcome_id: OpaqueId::new("outcome:aero-semi-car-below-minus-100bps")?,
        proposition: "Synthetic instrument cumulative abnormal return (CAR) over the 3-session window is <= -100 bps.".to_owned(),
        resolution_rules: "Resolve true iff 3-session market-model CAR <= -100.0 bps using point-in-time split-adjusted closes.".to_owned(),
    };

    let param_id = OpaqueId::new("param:curtailment-duration-hours")?;
    let draft_v1 = MarketEffectForecastDraft {
        forecast_id: OpaqueId::new("forecast:synthetic-aero-semi-freeze")?,
        version_id: VersionId::new("market-vintage:aero-semi-v1")?,
        schema_version: SchemaVersion::V1,
        actor: ActorId::new("actor:analyst-synthetic")?,
        issued_at_utc_ms: issued_v1_ms,
        origin_utc_ms: origin_v1_ms,
        information_cutoff_utc_ms: cutoff_v1_ms,
        horizon_start_utc_ms: horizon_start_ms,
        horizon_end_utc_ms: horizon_end_ms,
        cutoff_mode: CutoffAdmissibilityMode::ProspectiveStrict,
        target_instrument_id: target_inst.clone(),
        instrument_selection_as_of_utc_ms: cutoff_v1_ms - 86_400_000,
        baseline_version: VersionId::new("baseline:unconditional-historical-car-v1")?,
        model_version: VersionId::new("model:scenario-linear-impact-v1")?,
        model_knowledge_cutoff_utc_ms: Some(cutoff_v1_ms - 86_400_000),
        mode: MarketForecastModeSpec::ConditionalScenario(scenario_v1),
        catalysts: catalysts_v1.clone(),
        exposure_pathways: pathways_v1.clone(),
        novelty_status: NoveltyStatus::PartiallyAnticipated,
        expectation_surprise: None,
        real_world_vs_price_effect_rationale: "Partial hedges and existing inventory buffers absorb brief freeze events; incremental price impact requires multi-shift curtailment.".to_owned(),
        assumption_parameters: vec![ScenarioAssumptionParameter {
            parameter_id: param_id.clone(),
            label: "Curtailment duration".to_owned(),
            unit: OpaqueId::new("unit:hours")?,
            low_value: 6.0,
            base_value: 24.0,
            high_value: 48.0,
            rationale: "Historical grid emergency durations in comparable coastal freeze episodes.".to_owned(),
        }],
        sensitivity_points: vec![AssumptionSensitivityPoint {
            parameter_id: param_id.clone(),
            low_case_effect: 15.0,
            base_case_effect: -92.0,
            high_case_effect: -210.0,
            direction_flips_across_range: true,
            sensitivity_note: "A brief 6-hour curtailment is out-performed by relief rally (+15 bps), whereas 48-hour curtailment triggers -210 bps CAR.".to_owned(),
        }],
        precision_support: PrecisionSupport::SupportedByAvailableEvidence {
            rationale: "Supported by 10-K facility disclosure and ensemble weather spread.".to_owned(),
        },
        prediction: MarketEffectPrediction::Quantitative(QuantitativeMarketPrediction {
            effect_unit: unit_bps.clone(),
            expected_direction: MarketDirection::Negative,
            point_estimate: -92.0,
            prediction_interval: PredictionInterval::new(-150.0, -30.0, 0.80)?,
            baseline_point_estimate: 0.0,
            baseline_interval: PredictionInterval::new(-110.0, 110.0, 0.80)?,
            outcome_definition: outcome_def.clone(),
            forecast_probability: Probability::new(0.55)?,
            baseline_probability: Probability::new(0.20)?,
        }),
        sources: sources_v1.clone(),
        comparison_set: vec![VersionId::new("precedent:coastal-freeze-2021-v1")?],
        predecessor_version: None,
        revision_rationale: None,
    };

    let v1 = book
        .issue_market_effect_forecast(&mut ledger, draft_v1.clone())?
        .clone();

    // Issue linked revision v2 after an admissible preliminary macroeconomic inventory release arrives with consensus surprise.
    let cutoff_v2_ms = cutoff_v1_ms + 3_600_000;
    let macro_src = VersionId::new("source:macro-gas-inventory-prelim-v1")?;
    let consensus_src = VersionId::new("source:macro-consensus-survey-v1")?;
    let mut sources_v2 = sources_v1;
    sources_v2.push(CutoffSourceInput {
        source_version: macro_src.clone(),
        event_utc_ms: Some(cutoff_v1_ms - 86_400_000),
        public_available_at_utc_ms: Some(cutoff_v2_ms - 600_000),
        ingested_at_utc_ms: Some(cutoff_v2_ms - 300_000),
    });
    sources_v2.push(CutoffSourceInput {
        source_version: consensus_src.clone(),
        event_utc_ms: Some(cutoff_v1_ms - 172_800_000),
        public_available_at_utc_ms: Some(cutoff_v1_ms - 86_400_000),
        ingested_at_utc_ms: Some(cutoff_v1_ms - 80_000_000),
    });

    let mut catalysts_v2 = catalysts_v1;
    catalysts_v2.push(CatalystInputRecord {
        catalyst_id: OpaqueId::new("catalyst:macro-inventory-release-v1")?,
        source_version: macro_src.clone(),
        domain: CatalystDomain::MacroeconomicRelease {
            series_id: OpaqueId::new("series:industrial-gas-reserve-index")?,
            reference_period_label: "2026-W04".to_owned(),
            release_vintage_number: 1,
            supersedes_release_version: None,
        },
        summary: "[SYNTHETIC FIXTURE] Preliminary weekly industrial gas reserve index (-12.5% vs -4.0% consensus)".to_owned(),
        historical_event_utc_ms: cutoff_v1_ms - 86_400_000,
        public_release_utc_ms: Some(cutoff_v2_ms - 600_000),
        market_awareness_utc_ms: Some(cutoff_v2_ms - 540_000),
        ingested_at_utc_ms: Some(cutoff_v2_ms - 300_000),
        revision_vintage_public_release_utc_ms: None,
    });

    let mut pathways_v2 = pathways_v1;
    pathways_v2.push(ExposurePathwayCitation {
        pathway_id: OpaqueId::new("pathway:macro-gas-reserve-draw")?,
        catalyst_id: OpaqueId::new("catalyst:macro-inventory-release-v1")?,
        subject_entity_or_facility_id: OpaqueId::new("facility:coastal-fab-south")?,
        target_instrument_id: target_inst.clone(),
        mechanism_kind: ExposureMechanismKind::MacroRateOrCommodityChannel,
        mechanism_explanation:
            "Larger-than-expected regional gas reserve draw increases curtailment severity."
                .to_owned(),
        expected_direction: MarketDirection::Negative,
        cited_evidence_versions: vec![macro_src.clone(), consensus_src.clone()],
    });

    // Updated P(E) = 0.80, P(M | E) = 0.85, P(M | not E) = 0.30 => P(M) = 0.80*0.85 + 0.20*0.30 = 0.74.
    let scenario_v2 = ConditionalMarketScenarioSpec {
        scenario_id: OpaqueId::new("scenario:freeze-curtailment-v1")?,
        trigger_event_id: OpaqueId::new("event:grid-curtailment-stage2")?,
        trigger_proposition: "Regional grid operator declares Stage-2 industrial curtailment inside the 72-hour window.".to_owned(),
        trigger_occurrence_probability: Probability::new(0.80)?,
        response_given_event: ConditionalBranchResponse {
            branch_label: "Curtailment occurs (E)".to_owned(),
            expected_direction: MarketDirection::Negative,
            conditional_outcome_probability: Probability::new(0.85)?,
            conditional_point_estimate: -195.0,
            conditional_interval: PredictionInterval::new(-280.0, -110.0, 0.80)?,
        },
        response_given_non_event: Some(ConditionalBranchResponse {
            branch_label: "No curtailment (not E)".to_owned(),
            expected_direction: MarketDirection::Negative,
            conditional_outcome_probability: Probability::new(0.30)?,
            conditional_point_estimate: -45.0,
            conditional_interval: PredictionInterval::new(-110.0, 20.0, 0.80)?,
        }),
    };

    let draft_v2 = MarketEffectForecastDraft {
        version_id: VersionId::new("market-vintage:aero-semi-v2")?,
        issued_at_utc_ms: cutoff_v2_ms + 120_000,
        origin_utc_ms: cutoff_v2_ms + 60_000,
        information_cutoff_utc_ms: cutoff_v2_ms,
        mode: MarketForecastModeSpec::ConditionalScenario(scenario_v2),
        catalysts: catalysts_v2,
        exposure_pathways: pathways_v2,
        novelty_status: NoveltyStatus::ScheduledReleaseWithConsensus,
        expectation_surprise: Some(ExpectationSurpriseInput {
            metric_unit: OpaqueId::new("unit:pct_change")?,
            prior_expectation_value: -4.0,
            expectation_source_version: consensus_src,
            realized_release_value: -12.5,
            realized_source_version: macro_src.clone(),
            stated_surprise: -8.5,
        }),
        sensitivity_points: vec![AssumptionSensitivityPoint {
            parameter_id: param_id,
            low_case_effect: -40.0,
            base_case_effect: -165.0,
            high_case_effect: -260.0,
            direction_flips_across_range: false,
            sensitivity_note: "With depleted regional gas reserves, even a short curtailment produces a negative CAR.".to_owned(),
        }],
        prediction: MarketEffectPrediction::Quantitative(QuantitativeMarketPrediction {
            effect_unit: unit_bps.clone(),
            expected_direction: MarketDirection::Negative,
            point_estimate: -165.0,
            prediction_interval: PredictionInterval::new(-240.0, -90.0, 0.80)?,
            baseline_point_estimate: 0.0,
            baseline_interval: PredictionInterval::new(-110.0, 110.0, 0.80)?,
            outcome_definition: outcome_def.clone(),
            forecast_probability: Probability::new(0.74)?,
            baseline_probability: Probability::new(0.20)?,
        }),
        sources: sources_v2.clone(),
        predecessor_version: Some(v1.version_id().clone()),
        revision_rationale: Some(RevisionRationale {
            new_evidence_versions: vec![macro_src.clone()],
            summary: "Revised curtailment probability and conditional CAR downward after -8.5 pct-pt negative inventory release surprise.".to_owned(),
        }),
        ..draft_v1.clone()
    };

    let v2 = book
        .issue_market_effect_forecast(&mut ledger, draft_v2)?
        .clone();

    // Issue a QualitativeOrInsufficient forecast when precision is unsupported (MARKET-04).
    let mut qual_draft = draft_v1.clone();
    qual_draft.forecast_id = OpaqueId::new("forecast:synthetic-unverified-supplier")?;
    qual_draft.version_id = VersionId::new("market-vintage:supplier-qualitative-v1")?;
    qual_draft.mode = MarketForecastModeSpec::CurrentCondition;
    qual_draft.precision_support = PrecisionSupport::UnsupportedPrecision {
        reason: "Single uncorroborated trade notice without historical contract volume disclosure."
            .to_owned(),
    };
    qual_draft.prediction = MarketEffectPrediction::QualitativeOrInsufficient(QualitativeMarketOutput {
        disposition: QualitativeDisposition::VolatilityExpansionOnly,
        explanation: "Potential supply-chain bottleneck warrants monitoring for heightened spread/volatility, but numeric return ranges are unsupported.".to_owned(),
        missing_or_limiting_evidence: "Requires audited customer concentration or shipment volume records.".to_owned(),
    });
    let v_qual = book
        .issue_market_effect_forecast(&mut ledger, qual_draft)?
        .clone();

    // Verify point-in-time leakage rejection for a post-cutoff revised macroeconomic release (MARKET-05, A14).
    let mut leaked_draft = draft_v1;
    leaked_draft.forecast_id = OpaqueId::new("forecast:leaked-revision-attempt")?;
    leaked_draft.version_id = VersionId::new("market-vintage:leaked-v1")?;
    if let Some(first_cat) = leaked_draft.catalysts.first_mut() {
        first_cat.revision_vintage_public_release_utc_ms = Some(cutoff_v1_ms + 86_400_000);
    }
    let leak_res = book.issue_market_effect_forecast(&mut ledger, leaked_draft);
    let leak_refused = matches!(
        leak_res,
        Err(MarketEffectError::LeakageDetected(
            MarketLeakageKind::RevisedReleaseVintageAfterCutoff { .. }
        ))
    );

    // Record binary and scalar outcomes after horizon end: realized CAR = -175.0 bps (misses v1 interval [-150, -30], covered by v2 interval [-240, -90]).
    let bin_res_ver = VersionId::new("resolution:aero-semi-binary-v1")?;
    ledger.record_outcome_resolution(OutcomeResolutionRecord {
        resolution_id: OpaqueId::new("res-series:aero-semi-binary")?,
        version_id: bin_res_ver.clone(),
        outcome_id: outcome_def.outcome_id,
        actor: ActorId::new("actor:evaluator-synthetic")?,
        recorded_at_utc_ms: horizon_end_ms + 3_600_000,
        evidence_versions: vec![VersionId::new("source:market-event-study-run-v1")?],
        state: ResolutionState::Resolved {
            occurred: true,
            observed_event_utc_ms: Some(horizon_end_ms - 60_000),
        },
        supersedes_version: None,
        correction_rationale: None,
    })?;

    let scalar_res_ver = VersionId::new("resolution:aero-semi-scalar-v1")?;
    book.record_scalar_resolution(MarketScalarResolutionDraft {
        resolution_id: OpaqueId::new("res-series:aero-semi-scalar")?,
        version_id: scalar_res_ver.clone(),
        target_instrument_id: target_inst,
        effect_unit: unit_bps,
        horizon_start_utc_ms: horizon_start_ms,
        horizon_end_utc_ms: horizon_end_ms,
        actor: ActorId::new("actor:evaluator-synthetic")?,
        recorded_at_utc_ms: horizon_end_ms + 3_600_000,
        evidence_versions: vec![VersionId::new("source:market-event-study-run-v1")?],
        state: MarketScalarResolutionState::Resolved {
            realized_value: -175.0,
            observed_at_utc_ms: horizon_end_ms - 60_000,
        },
    })?;

    let eval = book
        .evaluate_market_effect_cohort(
            &mut ledger,
            VersionId::new("eval:market-effects-a14-v1")?,
            Some(VersionId::new("eval:binary-ledger-a14-v1")?),
            horizon_end_ms + 7_200_000,
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
                    market_vintage_version: v_qual.version_id().clone(),
                    binary_resolution_version: None,
                    scalar_resolution_version: None,
                },
            ],
            16,
        )?
        .clone();

    let sim = book.evaluate_simulated_strategy(
        VersionId::new("sim:aero-semi-v2-paper")?,
        v2.version_id(),
        &scalar_res_ver,
        SimulatedFrictionConfig {
            commission_bps_per_side: 1.5,
            half_spread_bps_per_side: 3.5,
            slippage_bps_per_side: 4.0,
        },
    )?;

    println!(
        "oia-forecast market_effects synthetic A14 check: quantitative_vintages={} qualitative_vintages={} resolved_scalar={} interval_coverage={}/{} preserved_misses={} revised_leakage_refused={} sim_gross_bps={:.1} sim_friction_bps={:.1} sim_net_bps={:.1} is_observed_return={} can_execute_orders={}",
        eval.quantitative_count(),
        eval.qualitative_or_insufficient_count(),
        eval.resolved_scalar_count(),
        eval.forecast_interval_coverage_ratio().0,
        eval.forecast_interval_coverage_ratio().1,
        eval.misses().len(),
        leak_refused,
        sim.gross_simulated_return_bps(),
        sim.round_trip_friction_deducted_bps(),
        sim.net_simulated_return_bps(),
        sim.is_observed_market_return(),
        sim.can_execute_orders(),
    );

    Ok(())
}
