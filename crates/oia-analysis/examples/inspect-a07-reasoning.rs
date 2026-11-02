//! Synthetic inspection example for `A07: inspectable reasoning` (`REASON-01`..=`REASON-04`, `#35`).
//!
//! Composes abstraction, correlation, deduction, and extrapolation over a fixed synthetic
//! fixture, registers competing hypotheses with discriminating evidence leads, and then
//! revises a premise and reverses an entity match to demonstrate non-destructive
//! downstream reassessment propagation.

use oia_analysis::reasoning::{
    AbstractionField, AbstractionFieldState, AbstractionRelationType, AbstractionRelationship,
    AbstractionSpec, BaselineRateSpec, CompetingHypothesisDraft, CorrelationPair, CorrelationSpec,
    DecisionState, DeductionSpec, DerivationStep, DiscriminatingEvidenceLead, EntityMatchCandidate,
    EvidenceOriginClass, ExtrapolationSpec, MeasuredQuantity, PremiseDraft, ReasoningError,
    ReasoningGraph, ReasoningLimits, ReasoningStepDraft, ReasoningStepKind,
    ReasoningVerificationTier, SourceDependencyStatus, StepInputRef,
};
use oia_domain::{ActorId, OpaqueId, SchemaVersion, VersionId};

#[allow(clippy::too_many_lines)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== OIA A07 Inspectable Reasoning Synthetic Fixture (#35) ===");
    let limits = ReasoningLimits::new(64, 64, 32, 32, 16_384)?;
    let mut graph = ReasoningGraph::new(limits);
    let analyst = ActorId::new("actor:analyst-synthetic-01")?;

    // 1. Register initial premises (observation, witness testimony, and document record).
    let p_radar_v1 = graph.register_premise(PremiseDraft {
        premise_id: OpaqueId::new("premise:radar-track-01")?,
        version_id: VersionId::new("ver:premise-radar-v1")?,
        predecessor_version: None,
        schema_version: SchemaVersion::V1,
        actor: analyst.clone(),
        origin_class: EvidenceOriginClass::NormalizedObservation,
        source_version: VersionId::new("ver:source-radar-log-2026-01")?,
        citation_anchor: "sweep=14..22".to_owned(),
        verbatim_excerpt: "Primary return bearing 042 deg, groundspeed 18.5 m/s over [0s, 60s]"
            .to_owned(),
        proposition: "Primary radar recorded a track moving NE at 18.5 m/s over 60 seconds."
            .to_owned(),
        quantity: Some(MeasuredQuantity {
            value: 18.5,
            unit: "m/s".to_owned(),
            uncertainty: Some(0.8),
            domain_min: 0.0,
            domain_max: 120.0,
        }),
        uncertainty_note: "Range gate ±150 m; altitude channel uncalibrated.".to_owned(),
        revision_rationale: None,
    })?;

    let p_witness_v1 = graph.register_premise(PremiseDraft {
        premise_id: OpaqueId::new("premise:witness-report-01")?,
        version_id: VersionId::new("ver:premise-witness-v1")?,
        predecessor_version: None,
        schema_version: SchemaVersion::V1,
        actor: analyst.clone(),
        origin_class: EvidenceOriginClass::UserTestimony,
        source_version: VersionId::new("ver:source-witness-form-09")?,
        citation_anchor: "field=narrative#L1-L4".to_owned(),
        verbatim_excerpt:
            "Saw a steady amber light heading northeast near the ridge; heard no engine noise."
                .to_owned(),
        proposition:
            "Observer reported an amber light heading northeast with no audible engine sound."
                .to_owned(),
        quantity: None,
        uncertainty_note: "Handheld compass estimate ±10 deg; acoustic masking by wind possible."
            .to_owned(),
        revision_rationale: None,
    })?;

    let p_notam_v1 = graph.register_premise(PremiseDraft {
        premise_id: OpaqueId::new("premise:notam-bulletin-01")?,
        version_id: VersionId::new("ver:premise-notam-v1")?,
        predecessor_version: None,
        schema_version: SchemaVersion::V1,
        actor: analyst.clone(),
        origin_class: EvidenceOriginClass::AcquiredSourceMaterial,
        source_version: VersionId::new("ver:source-notam-archive-04")?,
        citation_anchor: "notam=04-112".to_owned(),
        verbatim_excerpt:
            "High-altitude radiosonde release scheduled for 02:00Z from Station North Ridge."
                .to_owned(),
        proposition: "Station North Ridge scheduled a radiosonde release within the window."
            .to_owned(),
        quantity: None,
        uncertainty_note: "Scheduled release window ±15 minutes; actual launch log pending."
            .to_owned(),
        revision_rationale: None,
    })?;

    // 2. Step 1: Abstraction (REASON-03) — normalize reports while preserving verbatim text,
    //    explicit unknowns, conflicts, omissions, and reversible entity/relationship decisions.
    let candidate_id = OpaqueId::new("match:entity-ridge-object-01")?;
    let step_abs_v1 = graph.add_step(ReasoningStepDraft {
        step_id: OpaqueId::new("step:abstraction-01")?,
        version_id: VersionId::new("ver:step-abstraction-v1")?,
        predecessor_version: None,
        schema_version: SchemaVersion::V1,
        actor: analyst.clone(),
        inputs: vec![
            StepInputRef {
                node_version: p_radar_v1.clone(),
                declared_origin_class: EvidenceOriginClass::NormalizedObservation,
            },
            StepInputRef {
                node_version: p_witness_v1.clone(),
                declared_origin_class: EvidenceOriginClass::UserTestimony,
            },
            StepInputRef {
                node_version: p_notam_v1.clone(),
                declared_origin_class: EvidenceOriginClass::AcquiredSourceMaterial,
            },
        ],
        kind: ReasoningStepKind::Abstraction(AbstractionSpec {
            event_schema: "oia.aerial.event.v1".to_owned(),
            taxonomy_version: VersionId::new("ver:taxonomy-aerial-v1")?,
            fields: vec![
                AbstractionField {
                    field_name: "heading_sector".to_owned(),
                    state: AbstractionFieldState::Normalized {
                        normalized_value: "NE (042 deg)".to_owned(),
                        verbatim_wording: "bearing 042 deg / heading northeast".to_owned(),
                        source_version: VersionId::new("ver:source-radar-log-2026-01")?,
                    },
                },
                AbstractionField {
                    field_name: "altitude_msl".to_owned(),
                    state: AbstractionFieldState::ExplicitUnknown {
                        reason: "Radar height channel uncalibrated and single-witness elevation angle lacks range.".to_owned(),
                        verbatim_context: "altitude channel uncalibrated".to_owned(),
                        source_version: VersionId::new("ver:source-radar-log-2026-01")?,
                    },
                },
                AbstractionField {
                    field_name: "propulsion_signature".to_owned(),
                    state: AbstractionFieldState::Conflicting {
                        primary_value: "inaudible_from_ridge".to_owned(),
                        primary_verbatim: "heard no engine noise".to_owned(),
                        primary_source_version: VersionId::new("ver:source-witness-form-09")?,
                        competing_value: "uninstrumented_acoustic_channel".to_owned(),
                        competing_verbatim: "Primary return bearing 042 deg".to_owned(),
                        competing_source_version: VersionId::new("ver:source-radar-log-2026-01")?,
                        conflict_note: "Witness reports silence; radar log has no acoustic sensor."
                            .to_owned(),
                    },
                },
            ],
            entity_matches: vec![EntityMatchCandidate {
                candidate_id: candidate_id.clone(),
                mention_verbatim: "steady amber light heading northeast".to_owned(),
                source_version: VersionId::new("ver:source-witness-form-09")?,
                target_entity_id: OpaqueId::new("entity:radiosonde-payload-nr04")?,
                taxonomy_concept: "instrument_balloon_candidate".to_owned(),
                decision: DecisionState::Proposed {
                    rationale: "Bearing and time window overlap scheduled Station North Ridge release."
                        .to_owned(),
                },
            }],
            relationships: vec![AbstractionRelationship {
                relationship_id: OpaqueId::new("rel:co-occurrence-01")?,
                from_entity_id: OpaqueId::new("entity:radar-track-01")?,
                to_entity_id: OpaqueId::new("entity:radiosonde-payload-nr04")?,
                relation_type: AbstractionRelationType::PossibleSameEvent,
                verbatim_basis: "02:00Z window and NE drift".to_owned(),
                source_version: VersionId::new("ver:source-notam-archive-04")?,
                decision: DecisionState::Confirmed {
                    actor: analyst.clone(),
                    rationale: "Spatiotemporal overlap merits candidate link without asserting identity."
                        .to_owned(),
                },
            }],
            explicit_omissions: vec![
                "Secondary transponder interrogation log unavailable for sector 04.".to_owned(),
            ],
        }),
        revision_rationale: None,
    })?;

    // 3. Step 2: Correlation (REASON-02) — match radar track, witness report, and abstraction output.
    let step_corr_v1 = graph.add_step(ReasoningStepDraft {
        step_id: OpaqueId::new("step:correlation-01")?,
        version_id: VersionId::new("ver:step-correlation-v1")?,
        predecessor_version: None,
        schema_version: SchemaVersion::V1,
        actor: analyst.clone(),
        inputs: vec![
            StepInputRef {
                node_version: p_radar_v1.clone(),
                declared_origin_class: EvidenceOriginClass::NormalizedObservation,
            },
            StepInputRef {
                node_version: p_witness_v1.clone(),
                declared_origin_class: EvidenceOriginClass::UserTestimony,
            },
            StepInputRef {
                node_version: step_abs_v1.clone(),
                declared_origin_class: EvidenceOriginClass::AnalyticalInference,
            },
        ],
        kind: ReasoningStepKind::Correlation(CorrelationSpec {
            temporal_tolerance_seconds: 120.0,
            spatial_tolerance_km: 5.0,
            attribute_matching_rule: "Azimuth difference <= 15 deg and timestamp delta <= 120 s"
                .to_owned(),
            evaluated_pair_count: 18,
            matched_pairs: vec![CorrelationPair {
                left_version: p_radar_v1.clone(),
                right_version: p_witness_v1.clone(),
                observed_temporal_delta_seconds: 35.0,
                observed_spatial_delta_km: 1.8,
                source_dependency: SourceDependencyStatus::IndependentSources,
            }],
            coverage_fraction: 0.75,
            coverage_limitations: "Southern quadrant occluded by terrain above 12 deg.".to_owned(),
            baseline_rate: BaselineRateSpec::EmpiricalRate {
                rate_per_window: 0.08,
                basis_description:
                    "Historical nightly track-witness coincidence rate in sector 04.".to_owned(),
            },
            tested_hypotheses_count: 6,
            multiple_comparison_method: "Holm-Bonferroni across 6 candidate azimuth-time windows"
                .to_owned(),
            correlation_summary:
                "Radar track and witness sighting coincide within 35 s and 1.8 km.".to_owned(),
        }),
        revision_rationale: None,
    })?;

    // 4. Step 3: Deduction (REASON-01) — consumes correlation inference and NOTAM premise.
    let step_ded_v1 = graph.add_step(ReasoningStepDraft {
        step_id: OpaqueId::new("step:deduction-01")?,
        version_id: VersionId::new("ver:step-deduction-v1")?,
        predecessor_version: None,
        schema_version: SchemaVersion::V1,
        actor: analyst.clone(),
        inputs: vec![
            StepInputRef {
                node_version: step_corr_v1.clone(),
                declared_origin_class: EvidenceOriginClass::AnalyticalInference,
            },
            StepInputRef {
                node_version: p_notam_v1.clone(),
                declared_origin_class: EvidenceOriginClass::AcquiredSourceMaterial,
            },
        ],
        kind: ReasoningStepKind::Deduction(DeductionSpec {
            rule_system: "oia.OrderedBoundedKinematics.v1".to_owned(),
            explicit_assumptions: vec![
                "Prevailing 850 hPa wind vector is 040 deg at 17–20 m/s.".to_owned(),
                "Radiosonde launch occurred within the scheduled NOTAM window.".to_owned(),
            ],
            derivation_steps: vec![
                DerivationStep {
                    step_number: 1,
                    rule_name: "kinematic_wind_consistency".to_owned(),
                    cited_input_versions: vec![step_corr_v1.clone()],
                    required_assumptions: vec![
                        "Prevailing 850 hPa wind vector is 040 deg at 17–20 m/s.".to_owned(),
                    ],
                    inferred_statement:
                        "Observed 18.5 m/s groundspeed is consistent with passive wind drift."
                            .to_owned(),
                },
                DerivationStep {
                    step_number: 2,
                    rule_name: "window_compatibility_elimination".to_owned(),
                    cited_input_versions: vec![step_corr_v1.clone(), p_notam_v1.clone()],
                    required_assumptions: vec![
                        "Radiosonde launch occurred within the scheduled NOTAM window.".to_owned(),
                    ],
                    inferred_statement:
                        "Passive balloon drift remains a viable conditional explanation."
                            .to_owned(),
                },
            ],
            conditional_conclusion:
                "If the 850 hPa wind profile and NOTAM launch schedule hold, the track is consistent with a passive radiosonde."
                    .to_owned(),
            verification_tier: ReasoningVerificationTier::HumanReviewed {
                reviewer: analyst.clone(),
                review_note: "Reviewed conditional kinematic bounds; launch confirmation still required."
                    .to_owned(),
            },
            contradictions_noted: vec![
                "Witness reported steady amber light whereas standard radiosonde carries no steady beacon."
                    .to_owned(),
            ],
        }),
        revision_rationale: None,
    })?;

    // 5. Step 4: Extrapolation (REASON-02) — project downrange position over [60s, 180s].
    let step_ext_v1 = graph.add_step(ReasoningStepDraft {
        step_id: OpaqueId::new("step:extrapolation-01")?,
        version_id: VersionId::new("ver:step-extrapolation-v1")?,
        predecessor_version: None,
        schema_version: SchemaVersion::V1,
        actor: analyst.clone(),
        inputs: vec![
            StepInputRef {
                node_version: p_radar_v1.clone(),
                declared_origin_class: EvidenceOriginClass::NormalizedObservation,
            },
            StepInputRef {
                node_version: step_ded_v1.clone(),
                declared_origin_class: EvidenceOriginClass::AnalyticalInference,
            },
        ],
        kind: ReasoningStepKind::Extrapolation(ExtrapolationSpec {
            domain_variable: "elapsed_time".to_owned(),
            domain_unit: "s".to_owned(),
            observed_domain_min: 0.0,
            observed_domain_max: 60.0,
            observed_sample_count: 9,
            projection_horizon_start: 60.0,
            projection_horizon_end: 180.0,
            value_unit: "m/s".to_owned(),
            projected_value: 18.5,
            uncertainty_lower: 15.2,
            uncertainty_upper: 21.8,
            model_name: "constant_velocity_wind_shear_envelope".to_owned(),
            model_version: VersionId::new("ver:model-kinematic-envelope-v1")?,
            uncertainty_method: "Analytical shear-cone propagation (±0.025 m/s^2)".to_owned(),
            sensitivity_summary: "Highly sensitive to vertical wind shear above 2,000 m AGL."
                .to_owned(),
            explicit_assumptions: vec![
                "No powered acceleration maneuver occurs during [60s, 180s].".to_owned(),
            ],
        }),
        revision_rationale: None,
    })?;

    // Verify that attempting to feed an inference as a NormalizedObservation is rejected.
    let bad_promotion = graph.add_step(ReasoningStepDraft {
        step_id: OpaqueId::new("step:illegal-promotion")?,
        version_id: VersionId::new("ver:step-illegal-v1")?,
        predecessor_version: None,
        schema_version: SchemaVersion::V1,
        actor: analyst.clone(),
        inputs: vec![StepInputRef {
            node_version: step_ded_v1.clone(),
            declared_origin_class: EvidenceOriginClass::NormalizedObservation,
        }],
        kind: ReasoningStepKind::Deduction(DeductionSpec {
            rule_system: "test".to_owned(),
            explicit_assumptions: vec![],
            derivation_steps: vec![DerivationStep {
                step_number: 1,
                rule_name: "noop".to_owned(),
                cited_input_versions: vec![step_ded_v1.clone()],
                required_assumptions: vec![],
                inferred_statement: "noop".to_owned(),
            }],
            conditional_conclusion: "noop".to_owned(),
            verification_tier: ReasoningVerificationTier::AiProposed {
                model_id: "model:test".to_owned(),
                prompt_template_version: VersionId::new("ver:prompt-v1")?,
            },
            contradictions_noted: vec![],
        }),
        revision_rationale: None,
    });
    assert!(matches!(
        bad_promotion,
        Err(ReasoningError::InputOriginClassMismatch { .. })
    ));

    // 6. Register two competing hypotheses (REASON-04, A07).
    let hyp_balloon = graph.register_hypothesis(CompetingHypothesisDraft {
        hypothesis_id: OpaqueId::new("hyp:radiosonde-drift")?,
        version_id: VersionId::new("ver:hyp-radiosonde-v1")?,
        schema_version: SchemaVersion::V1,
        actor: analyst.clone(),
        title: "H1: Scheduled radiosonde drift with lantern attachment".to_owned(),
        explanation: "Track speed and bearing match the 02:00Z North Ridge radiosonde profile."
            .to_owned(),
        supporting_node_versions: vec![
            p_notam_v1.clone(),
            step_ded_v1.clone(),
            step_ext_v1.clone(),
        ],
        contradicting_node_versions: vec![p_witness_v1.clone()],
        explicit_assumptions: vec!["Launch occurred on schedule at 02:00Z.".to_owned()],
        discriminating_leads: vec![DiscriminatingEvidenceLead {
            lead_id: OpaqueId::new("lead:station-launch-log")?,
            description: "Obtain Station North Ridge actual balloon release log and telemetry."
                .to_owned(),
            expected_if_hypothesis_holds:
                "Telemetry confirms 02:00Z release and GPS track along 042 deg.".to_owned(),
            expected_if_alternative_holds:
                "Launch was scrubbed or followed a different wind layer.".to_owned(),
        }],
    })?;

    let hyp_uav = graph.register_hypothesis(CompetingHypothesisDraft {
        hypothesis_id: OpaqueId::new("hyp:low-acoustic-uav")?,
        version_id: VersionId::new("ver:hyp-uav-v1")?,
        schema_version: SchemaVersion::V1,
        actor: analyst.clone(),
        title: "H2: Small electric rotary/fixed-wing UAV".to_owned(),
        explanation: "Steady amber navigation LED and low acoustic signature beyond 1.5 km."
            .to_owned(),
        supporting_node_versions: vec![p_witness_v1.clone(), step_corr_v1.clone()],
        contradicting_node_versions: vec![p_notam_v1.clone()],
        explicit_assumptions: vec![
            "Electric propulsion masked by ridge wind at 1.8 km slant range.".to_owned(),
        ],
        discriminating_leads: vec![DiscriminatingEvidenceLead {
            lead_id: OpaqueId::new("lead:remote-id-receiver")?,
            description: "Inspect sector 04 Remote ID and RF spectrum captures.".to_owned(),
            expected_if_hypothesis_holds: "2.4/5.8 GHz control link or Remote ID beacon present."
                .to_owned(),
            expected_if_alternative_holds: "403 MHz radiosonde telemetry bursts only.".to_owned(),
        }],
    })?;

    println!(
        "Registered 3 premises, 4 composed reasoning steps, and {} competing hypotheses.",
        graph.hypotheses().count()
    );
    let ext_rec = graph
        .step(&step_ext_v1)
        .ok_or("missing extrapolation step")?;
    println!(
        "Extrapolation step {} output_class={} (can_serve_as_observation={}), contains_non_observational_inputs={}",
        ext_rec.version_id(),
        ext_rec.output_origin_class(),
        ext_rec.can_serve_as_observation(),
        ext_rec.contains_non_observational_inputs()
    );
    println!("Mandatory notice: {}", ext_rec.mandatory_notice());

    // 7. Modify a premise (radar speed corrected after range-gate recalibration) and reverse
    //    the proposed entity match in the abstraction step. Verify downstream reassessment!
    let p_radar_v2 = graph.revise_premise(PremiseDraft {
        premise_id: OpaqueId::new("premise:radar-track-01")?,
        version_id: VersionId::new("ver:premise-radar-v2")?,
        predecessor_version: Some(p_radar_v1.clone()),
        schema_version: SchemaVersion::V1,
        actor: analyst.clone(),
        origin_class: EvidenceOriginClass::NormalizedObservation,
        source_version: VersionId::new("ver:source-radar-log-2026-02")?,
        citation_anchor: "sweep=14..22-recalibrated".to_owned(),
        verbatim_excerpt:
            "Recalibrated return bearing 042 deg, groundspeed 29.0 m/s over [0s, 60s]".to_owned(),
        proposition: "Recalibrated radar track shows 29.0 m/s groundspeed, exceeding wind drift."
            .to_owned(),
        quantity: Some(MeasuredQuantity {
            value: 29.0,
            unit: "m/s".to_owned(),
            uncertainty: Some(0.6),
            domain_min: 0.0,
            domain_max: 120.0,
        }),
        uncertainty_note: "Recalibrated range gate ±75 m.".to_owned(),
        revision_rationale: Some("Applied radar clock/range-gate calibration table v2.".to_owned()),
    })?;

    let step_abs_v2 = graph.update_entity_match_decision(
        &step_abs_v1,
        &candidate_id,
        VersionId::new("ver:step-abstraction-v2")?,
        analyst,
        DecisionState::Reversed {
            actor: ActorId::new("actor:reviewer-synthetic-02")?,
            reversal_rationale:
                "Station North Ridge confirmed 02:00Z radiosonde release was scrubbed due to icing."
                    .to_owned(),
        },
    )?;

    println!(
        "Revised premise {} -> {} and reversed entity match in {} -> {}.",
        p_radar_v1, p_radar_v2, step_abs_v1, step_abs_v2
    );

    for step_ver in [&step_abs_v1, &step_corr_v1, &step_ded_v1, &step_ext_v1] {
        let step = graph.step(step_ver).ok_or("missing step")?;
        println!(
            "  Step {} (family={}) needs_reassessment={} (triggers={})",
            step.version_id(),
            step.kind().family_name(),
            step.needs_reassessment(),
            step.reassessment_triggers().len()
        );
    }

    for hyp_ver in [&hyp_balloon, &hyp_uav] {
        let hyp = graph.hypothesis(hyp_ver).ok_or("missing hypothesis")?;
        println!(
            "  Hypothesis {} ('{}') needs_reassessment={} (triggers={})",
            hyp.version_id(),
            hyp.title(),
            hyp.needs_reassessment(),
            hyp.reassessment_triggers().len()
        );
    }

    Ok(())
}
