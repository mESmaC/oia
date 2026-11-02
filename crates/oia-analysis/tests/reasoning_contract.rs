//! Focused contract tests for `oia_analysis::reasoning` (`REASON-01`..=`REASON-04`, `A07`, `#35`).
//!
//! Risk and requirements covered:
//! 1. `REASON-01`..=`REASON-04` / `A07` composed pipeline: abstraction, correlation, deduction,
//!    and extrapolation preserve verbatim wording, explicit unknowns, conflicts, tolerances,
//!    baseline rates, verification tiers (`AiProposed`, `HumanReviewed`,
//!    `AssertedUnverifiedLeanRef`), and non-observational inference origin classes
//!    (`AnalyticalInference` never becomes `NormalizedObservation`).
//! 2. `REASON-04` / `A07` non-destructive reassessment propagation: modifying an upstream
//!    premise or reversing an abstraction entity/relationship decision preserves the original
//!    vintages and transitively flags dependent steps and competing hypotheses
//!    `needs_reassessment = true`.
//! 3. `REASON-02` / `A07` domain/unit and malformed-input guards: incompatible units,
//!    out-of-domain values, invalid horizons/tolerances, support/contradiction overlap, and
//!    bidi/control characters are rejected before corrupting graph state.

use oia_analysis::reasoning::{
    AbstractionField, AbstractionFieldState, AbstractionRelationType, AbstractionRelationship,
    AbstractionSpec, BaselineRateSpec, CORRELATION_NON_CAUSATION_NOTICE, CompetingHypothesisDraft,
    CorrelationPair, CorrelationSpec, DEDUCTION_CONDITIONALITY_NOTICE, DecisionState,
    DeductionSpec, DerivationStep, DiscriminatingEvidenceLead, EXTRAPOLATION_PROJECTION_NOTICE,
    EntityMatchCandidate, EvidenceOriginClass, ExtrapolationSpec, MeasuredQuantity, PremiseDraft,
    ReasoningError, ReasoningGraph, ReasoningLimitKind, ReasoningLimits, ReasoningStepDraft,
    ReasoningStepKind, ReasoningVerificationTier, ReassessmentTrigger, SourceDependencyStatus,
    StepInputRef,
};
use oia_domain::{ActorId, OpaqueId, SchemaVersion, VersionId};

#[test]
#[allow(clippy::too_many_lines)]
fn composed_reasoning_preserves_origins_and_propagates_reassessment()
-> Result<(), Box<dyn std::error::Error>> {
    let limits = ReasoningLimits::new(32, 32, 16, 16, 8_192)?;
    let mut graph = ReasoningGraph::new(limits);
    let actor = ActorId::new("actor:analyst-01")?;

    let p1_v1 = graph.register_premise(PremiseDraft {
        premise_id: OpaqueId::new("premise:sensor-speed")?,
        version_id: VersionId::new("ver:p1-v1")?,
        predecessor_version: None,
        schema_version: SchemaVersion::V1,
        actor: actor.clone(),
        origin_class: EvidenceOriginClass::NormalizedObservation,
        source_version: VersionId::new("ver:src-radar-1")?,
        citation_anchor: "frames=10..20".to_owned(),
        verbatim_excerpt: "Groundspeed 20.0 m/s over [0s, 30s]".to_owned(),
        proposition: "Object moved at 20.0 m/s over [0s, 30s].".to_owned(),
        quantity: Some(MeasuredQuantity {
            value: 20.0,
            unit: "m/s".to_owned(),
            uncertainty: Some(0.5),
            domain_min: 0.0,
            domain_max: 100.0,
        }),
        uncertainty_note: "±0.5 m/s Doppler uncertainty".to_owned(),
        revision_rationale: None,
    })?;

    let p2_v1 = graph.register_premise(PremiseDraft {
        premise_id: OpaqueId::new("premise:witness-log")?,
        version_id: VersionId::new("ver:p2-v1")?,
        predecessor_version: None,
        schema_version: SchemaVersion::V1,
        actor: actor.clone(),
        origin_class: EvidenceOriginClass::UserTestimony,
        source_version: VersionId::new("ver:src-witness-1")?,
        citation_anchor: "line=4".to_owned(),
        verbatim_excerpt: "Silent white light drifting east".to_owned(),
        proposition: "Witness saw a silent white light drifting east.".to_owned(),
        quantity: None,
        uncertainty_note: "Visual azimuth estimate ±12 deg".to_owned(),
        revision_rationale: None,
    })?;

    let candidate_id = OpaqueId::new("cand:entity-balloon")?;
    let rel_id = OpaqueId::new("rel:co-occur-1")?;
    let s_abs_v1 = graph.add_step(ReasoningStepDraft {
        step_id: OpaqueId::new("step:abs")?,
        version_id: VersionId::new("ver:s-abs-v1")?,
        predecessor_version: None,
        schema_version: SchemaVersion::V1,
        actor: actor.clone(),
        inputs: vec![
            StepInputRef {
                node_version: p1_v1.clone(),
                declared_origin_class: EvidenceOriginClass::NormalizedObservation,
            },
            StepInputRef {
                node_version: p2_v1.clone(),
                declared_origin_class: EvidenceOriginClass::UserTestimony,
            },
        ],
        kind: ReasoningStepKind::Abstraction(AbstractionSpec {
            event_schema: "oia.event.v1".to_owned(),
            taxonomy_version: VersionId::new("ver:tax-v1")?,
            fields: vec![
                AbstractionField {
                    field_name: "direction".to_owned(),
                    state: AbstractionFieldState::Normalized {
                        normalized_value: "East".to_owned(),
                        verbatim_wording: "drifting east".to_owned(),
                        source_version: VersionId::new("ver:src-witness-1")?,
                    },
                },
                AbstractionField {
                    field_name: "altitude".to_owned(),
                    state: AbstractionFieldState::ExplicitUnknown {
                        reason: "No elevation sensor active".to_owned(),
                        verbatim_context: "Groundspeed 20.0 m/s".to_owned(),
                        source_version: VersionId::new("ver:src-radar-1")?,
                    },
                },
                AbstractionField {
                    field_name: "lighting".to_owned(),
                    state: AbstractionFieldState::Conflicting {
                        primary_value: "white_steady".to_owned(),
                        primary_verbatim: "Silent white light".to_owned(),
                        primary_source_version: VersionId::new("ver:src-witness-1")?,
                        competing_value: "unlit_primary_target".to_owned(),
                        competing_verbatim: "Groundspeed 20.0 m/s".to_owned(),
                        competing_source_version: VersionId::new("ver:src-radar-1")?,
                        conflict_note: "Visual witness vs radar-only track".to_owned(),
                    },
                },
            ],
            entity_matches: vec![EntityMatchCandidate {
                candidate_id: candidate_id.clone(),
                mention_verbatim: "Silent white light".to_owned(),
                source_version: VersionId::new("ver:src-witness-1")?,
                target_entity_id: OpaqueId::new("entity:wx-balloon-01")?,
                taxonomy_concept: "weather_balloon".to_owned(),
                decision: DecisionState::Proposed {
                    rationale: "Drifting with prevailing wind".to_owned(),
                },
            }],
            relationships: vec![AbstractionRelationship {
                relationship_id: rel_id.clone(),
                from_entity_id: OpaqueId::new("entity:track-01")?,
                to_entity_id: OpaqueId::new("entity:wx-balloon-01")?,
                relation_type: AbstractionRelationType::ObservedCoOccurrence,
                verbatim_basis: "Same sector and window".to_owned(),
                source_version: VersionId::new("ver:src-radar-1")?,
                decision: DecisionState::Confirmed {
                    actor: actor.clone(),
                    rationale: "Co-located in sector".to_owned(),
                },
            }],
            explicit_omissions: vec!["No ADS-B receiver coverage below 500 m AGL".to_owned()],
        }),
        revision_rationale: None,
    })?;

    let abs_rec = graph.step(&s_abs_v1).ok_or("missing abs step")?;
    assert_eq!(
        abs_rec.output_origin_class(),
        EvidenceOriginClass::AnalyticalInference
    );
    assert!(!abs_rec.can_serve_as_observation());
    assert!(!abs_rec.contains_non_observational_inputs());

    // Feeding `s_abs_v1` into Correlation must declare `AnalyticalInference`, not `NormalizedObservation`.
    let promoted_err = graph.add_step(ReasoningStepDraft {
        step_id: OpaqueId::new("step:corr-bad")?,
        version_id: VersionId::new("ver:s-corr-bad")?,
        predecessor_version: None,
        schema_version: SchemaVersion::V1,
        actor: actor.clone(),
        inputs: vec![StepInputRef {
            node_version: s_abs_v1.clone(),
            declared_origin_class: EvidenceOriginClass::NormalizedObservation,
        }],
        kind: ReasoningStepKind::Correlation(CorrelationSpec {
            temporal_tolerance_seconds: 60.0,
            spatial_tolerance_km: 2.0,
            attribute_matching_rule: "azimuth <= 10 deg".to_owned(),
            evaluated_pair_count: 1,
            matched_pairs: vec![CorrelationPair {
                left_version: s_abs_v1.clone(),
                right_version: s_abs_v1.clone(),
                observed_temporal_delta_seconds: 0.0,
                observed_spatial_delta_km: 0.0,
                source_dependency: SourceDependencyStatus::IndependentSources,
            }],
            coverage_fraction: 0.8,
            coverage_limitations: "Partial sky coverage".to_owned(),
            baseline_rate: BaselineRateSpec::Unavailable {
                reason: "No historical archive".to_owned(),
            },
            tested_hypotheses_count: 1,
            multiple_comparison_method: "Single test".to_owned(),
            correlation_summary: "Self test".to_owned(),
        }),
        revision_rationale: None,
    });
    assert_eq!(
        promoted_err,
        Err(ReasoningError::InputOriginClassMismatch {
            node_version: s_abs_v1.clone(),
            declared: EvidenceOriginClass::NormalizedObservation,
            actual: EvidenceOriginClass::AnalyticalInference,
        })
    );

    let s_corr_v1 = graph.add_step(ReasoningStepDraft {
        step_id: OpaqueId::new("step:corr")?,
        version_id: VersionId::new("ver:s-corr-v1")?,
        predecessor_version: None,
        schema_version: SchemaVersion::V1,
        actor: actor.clone(),
        inputs: vec![
            StepInputRef {
                node_version: p1_v1.clone(),
                declared_origin_class: EvidenceOriginClass::NormalizedObservation,
            },
            StepInputRef {
                node_version: p2_v1.clone(),
                declared_origin_class: EvidenceOriginClass::UserTestimony,
            },
            StepInputRef {
                node_version: s_abs_v1.clone(),
                declared_origin_class: EvidenceOriginClass::AnalyticalInference,
            },
        ],
        kind: ReasoningStepKind::Correlation(CorrelationSpec {
            temporal_tolerance_seconds: 60.0,
            spatial_tolerance_km: 2.5,
            attribute_matching_rule: "azimuth <= 10 deg".to_owned(),
            evaluated_pair_count: 12,
            matched_pairs: vec![CorrelationPair {
                left_version: p1_v1.clone(),
                right_version: p2_v1.clone(),
                observed_temporal_delta_seconds: 18.0,
                observed_spatial_delta_km: 1.1,
                source_dependency: SourceDependencyStatus::SharedUpstreamSource {
                    common_source_version: VersionId::new("ver:src-dispatch-hub")?,
                },
            }],
            coverage_fraction: 0.85,
            coverage_limitations: "15% packet loss on secondary receiver".to_owned(),
            baseline_rate: BaselineRateSpec::EmpiricalRate {
                rate_per_window: 0.05,
                basis_description: "30-day sector baseline".to_owned(),
            },
            tested_hypotheses_count: 4,
            multiple_comparison_method: "Bonferroni (m=4)".to_owned(),
            correlation_summary: "Spatiotemporal match with shared dispatch origin noted"
                .to_owned(),
        }),
        revision_rationale: None,
    })?;

    let corr_rec = graph.step(&s_corr_v1).ok_or("missing corr step")?;
    assert!(corr_rec.contains_non_observational_inputs());
    assert_eq!(
        corr_rec.mandatory_notice(),
        CORRELATION_NON_CAUSATION_NOTICE
    );

    let s_ded_v1 = graph.add_step(ReasoningStepDraft {
        step_id: OpaqueId::new("step:ded")?,
        version_id: VersionId::new("ver:s-ded-v1")?,
        predecessor_version: None,
        schema_version: SchemaVersion::V1,
        actor: actor.clone(),
        inputs: vec![StepInputRef {
            node_version: s_corr_v1.clone(),
            declared_origin_class: EvidenceOriginClass::AnalyticalInference,
        }],
        kind: ReasoningStepKind::Deduction(DeductionSpec {
            rule_system: "oia.interval.bounds.v1".to_owned(),
            explicit_assumptions: vec!["Constant wind field during 30s interval".to_owned()],
            derivation_steps: vec![DerivationStep {
                step_number: 1,
                rule_name: "wind_drift_bound".to_owned(),
                cited_input_versions: vec![s_corr_v1.clone()],
                required_assumptions: vec!["Constant wind field during 30s interval".to_owned()],
                inferred_statement: "Speed lies within passive drift envelope".to_owned(),
            }],
            conditional_conclusion:
                "Conditional on constant wind, object speed is consistent with passive drift"
                    .to_owned(),
            verification_tier: ReasoningVerificationTier::AssertedUnverifiedLeanRef {
                theorem_id: "AssertedModelRef.Kinematics.passive_drift_bound".to_owned(),
                lean_toolchain: "leanprover/lean4:v4.35.0".to_owned(),
                model_revision: "unverified-fixture-ref".to_owned(),
                assumptions_summary: "Caller-asserted model reference; not verified at runtime"
                    .to_owned(),
            },
            contradictions_noted: vec![
                "Shared dispatch origin prevents independent corroboration".to_owned(),
            ],
        }),
        revision_rationale: None,
    })?;

    let ded_rec = graph.step(&s_ded_v1).ok_or("missing ded step")?;
    assert_eq!(ded_rec.mandatory_notice(), DEDUCTION_CONDITIONALITY_NOTICE);
    let ReasoningStepKind::Deduction(ded_spec) = ded_rec.kind() else {
        panic!("expected deduction step");
    };
    assert_eq!(
        ded_spec.verification_tier.tier_label(),
        "asserted_unverified_lean_ref"
    );
    assert!(
        !ded_spec
            .verification_tier
            .establishes_lean_proof_or_correspondence()
    );

    let s_ext_v1 = graph.add_step(ReasoningStepDraft {
        step_id: OpaqueId::new("step:ext")?,
        version_id: VersionId::new("ver:s-ext-v1")?,
        predecessor_version: None,
        schema_version: SchemaVersion::V1,
        actor: actor.clone(),
        inputs: vec![
            StepInputRef {
                node_version: p1_v1.clone(),
                declared_origin_class: EvidenceOriginClass::NormalizedObservation,
            },
            StepInputRef {
                node_version: s_ded_v1.clone(),
                declared_origin_class: EvidenceOriginClass::AnalyticalInference,
            },
        ],
        kind: ReasoningStepKind::Extrapolation(ExtrapolationSpec {
            domain_variable: "time".to_owned(),
            domain_unit: "s".to_owned(),
            observed_domain_min: 0.0,
            observed_domain_max: 30.0,
            observed_sample_count: 6,
            projection_horizon_start: 30.0,
            projection_horizon_end: 90.0,
            value_unit: "m/s".to_owned(),
            projected_value: 20.0,
            uncertainty_lower: 17.5,
            uncertainty_upper: 22.5,
            model_name: "linear_drift_v1".to_owned(),
            model_version: VersionId::new("ver:model-drift-1")?,
            uncertainty_method: "Analytical variance propagation".to_owned(),
            sensitivity_summary: "Sensitive to gust shear > 2 m/s".to_owned(),
            explicit_assumptions: vec!["No propulsion impulse in [30s, 90s]".to_owned()],
        }),
        revision_rationale: None,
    })?;

    let ext_rec = graph.step(&s_ext_v1).ok_or("missing ext step")?;
    assert_eq!(ext_rec.mandatory_notice(), EXTRAPOLATION_PROJECTION_NOTICE);
    assert!(!ext_rec.needs_reassessment());

    // Register two competing hypotheses (REASON-04, A07).
    let h1 = graph.register_hypothesis(CompetingHypothesisDraft {
        hypothesis_id: OpaqueId::new("hyp:passive-balloon")?,
        version_id: VersionId::new("ver:h1-v1")?,
        schema_version: SchemaVersion::V1,
        actor: actor.clone(),
        title: "Passive weather balloon".to_owned(),
        explanation: "Drifting east at 20 m/s with prevailing wind".to_owned(),
        supporting_node_versions: vec![p1_v1.clone(), s_ded_v1.clone(), s_ext_v1.clone()],
        contradicting_node_versions: vec![p2_v1.clone()],
        explicit_assumptions: vec!["Balloon launched upwind".to_owned()],
        discriminating_leads: vec![DiscriminatingEvidenceLead {
            lead_id: OpaqueId::new("lead:radiosonde-freq")?,
            description: "Check 403 MHz telemetry".to_owned(),
            expected_if_hypothesis_holds: "403 MHz burst present".to_owned(),
            expected_if_alternative_holds: "No 403 MHz burst".to_owned(),
        }],
    })?;

    let h2 = graph.register_hypothesis(CompetingHypothesisDraft {
        hypothesis_id: OpaqueId::new("hyp:powered-drone")?,
        version_id: VersionId::new("ver:h2-v1")?,
        schema_version: SchemaVersion::V1,
        actor: actor.clone(),
        title: "Small powered UAV".to_owned(),
        explanation: "Steady white navigation light on eastbound corridor".to_owned(),
        supporting_node_versions: vec![p2_v1.clone(), s_corr_v1.clone()],
        contradicting_node_versions: vec![s_ded_v1.clone()],
        explicit_assumptions: vec!["Acoustic signature masked by distance".to_owned()],
        discriminating_leads: vec![DiscriminatingEvidenceLead {
            lead_id: OpaqueId::new("lead:remote-id")?,
            description: "Check Remote ID broadcast".to_owned(),
            expected_if_hypothesis_holds: "2.4 GHz beacon".to_owned(),
            expected_if_alternative_holds: "No Remote ID beacon".to_owned(),
        }],
    })?;

    // Revise premise p1_v1 -> p1_v2 and verify non-destructive transitive reassessment!
    let p1_v2 = graph.revise_premise(PremiseDraft {
        premise_id: OpaqueId::new("premise:sensor-speed")?,
        version_id: VersionId::new("ver:p1-v2")?,
        predecessor_version: Some(p1_v1.clone()),
        schema_version: SchemaVersion::V1,
        actor: actor.clone(),
        origin_class: EvidenceOriginClass::NormalizedObservation,
        source_version: VersionId::new("ver:src-radar-2")?,
        citation_anchor: "frames=10..20-corrected".to_owned(),
        verbatim_excerpt: "Corrected groundspeed 34.0 m/s over [0s, 30s]".to_owned(),
        proposition: "Object moved at 34.0 m/s after cosine-angle correction.".to_owned(),
        quantity: Some(MeasuredQuantity {
            value: 34.0,
            unit: "m/s".to_owned(),
            uncertainty: Some(0.5),
            domain_min: 0.0,
            domain_max: 100.0,
        }),
        uncertainty_note: "Corrected for aspect angle".to_owned(),
        revision_rationale: Some("Applied aspect-angle correction to Doppler track".to_owned()),
    })?;

    // Original premise p1_v1 is preserved unchanged.
    assert_eq!(
        graph
            .premise(&p1_v1)
            .ok_or("p1_v1")?
            .quantity()
            .ok_or("qty")?
            .value,
        20.0
    );
    assert_eq!(
        graph
            .premise(&p1_v2)
            .ok_or("p1_v2")?
            .quantity()
            .ok_or("qty")?
            .value,
        34.0
    );

    // Every downstream step (s_abs_v1, s_corr_v1, s_ded_v1, s_ext_v1) and both hypotheses are flagged!
    for ver in [&s_abs_v1, &s_corr_v1, &s_ded_v1, &s_ext_v1] {
        let step = graph.step(ver).ok_or("step")?;
        assert!(step.needs_reassessment());
        assert!(
            step.reassessment_triggers()
                .contains(&ReassessmentTrigger::PremiseRevised {
                    premise_id: OpaqueId::new("premise:sensor-speed")?,
                    old_version: p1_v1.clone(),
                    new_version: p1_v2.clone(),
                    rationale: "Applied aspect-angle correction to Doppler track".to_owned(),
                })
        );
    }
    assert!(graph.hypothesis(&h1).ok_or("h1")?.needs_reassessment());
    assert!(graph.hypothesis(&h2).ok_or("h2")?.needs_reassessment());

    // Reverse the entity match decision and relationship decision in s_abs_v1 -> s_abs_v2 -> s_abs_v3.
    let s_abs_v2 = graph.update_entity_match_decision(
        &s_abs_v1,
        &candidate_id,
        VersionId::new("ver:s-abs-v2")?,
        actor.clone(),
        DecisionState::Reversed {
            actor: actor.clone(),
            reversal_rationale: "34.0 m/s exceeds balloon drift envelope".to_owned(),
        },
    )?;
    let s_abs_v3 = graph.update_relationship_decision(
        &s_abs_v2,
        &rel_id,
        VersionId::new("ver:s-abs-v3")?,
        actor,
        DecisionState::Reversed {
            actor: ActorId::new("actor:reviewer-02")?,
            reversal_rationale: "Co-occurrence invalidated after speed correction".to_owned(),
        },
    )?;

    assert_eq!(
        graph
            .step_versions(&OpaqueId::new("step:abs")?)
            .ok_or("history")?
            .len(),
        3
    );
    assert!(
        graph
            .step(&s_corr_v1)
            .ok_or("corr")?
            .reassessment_triggers()
            .contains(&ReassessmentTrigger::AbstractionDecisionUpdated {
                step_id: OpaqueId::new("step:abs")?,
                old_version: s_abs_v1,
                new_version: s_abs_v2,
                target_id: candidate_id,
                rationale: "34.0 m/s exceeds balloon drift envelope".to_owned(),
            })
    );
    let ReasoningStepKind::Abstraction(abs_v3_spec) = graph.step(&s_abs_v3).ok_or("abs_v3")?.kind()
    else {
        panic!("expected abstraction step");
    };
    assert!(matches!(
        abs_v3_spec.relationships[0].decision,
        DecisionState::Reversed { .. }
    ));

    Ok(())
}

#[test]
fn rejects_unsupported_units_domains_horizons_and_malformed_inputs()
-> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        ReasoningLimits::new(0, 10, 10, 10, 1024),
        Err(ReasoningError::ZeroLimit(ReasoningLimitKind::Premises))
    );

    let limits = ReasoningLimits::new(16, 16, 8, 8, 4_096)?;
    let mut graph = ReasoningGraph::new(limits);
    let actor = ActorId::new("actor:validator")?;

    // Out-of-domain premise quantity is rejected.
    let bad_premise = graph.register_premise(PremiseDraft {
        premise_id: OpaqueId::new("premise:bad-domain")?,
        version_id: VersionId::new("ver:p-bad")?,
        predecessor_version: None,
        schema_version: SchemaVersion::V1,
        actor: actor.clone(),
        origin_class: EvidenceOriginClass::NormalizedObservation,
        source_version: VersionId::new("ver:src-1")?,
        citation_anchor: "row=1".to_owned(),
        verbatim_excerpt: "150 m/s".to_owned(),
        proposition: "Out of sensor range".to_owned(),
        quantity: Some(MeasuredQuantity {
            value: 150.0,
            unit: "m/s".to_owned(),
            uncertainty: None,
            domain_min: 0.0,
            domain_max: 100.0,
        }),
        uncertainty_note: "Bounded to 100 m/s".to_owned(),
        revision_rationale: None,
    });
    assert!(matches!(
        bad_premise,
        Err(ReasoningError::UnsupportedDomainValue { .. })
    ));

    // Register a valid premise in "knots" and verify Extrapolation rejects a unit mismatch ("m/s").
    let p_knots = graph.register_premise(PremiseDraft {
        premise_id: OpaqueId::new("premise:knots")?,
        version_id: VersionId::new("ver:p-knots")?,
        predecessor_version: None,
        schema_version: SchemaVersion::V1,
        actor: actor.clone(),
        origin_class: EvidenceOriginClass::NormalizedObservation,
        source_version: VersionId::new("ver:src-1")?,
        citation_anchor: "row=2".to_owned(),
        verbatim_excerpt: "40 knots".to_owned(),
        proposition: "40 knots speed".to_owned(),
        quantity: Some(MeasuredQuantity {
            value: 40.0,
            unit: "knots".to_owned(),
            uncertainty: Some(1.0),
            domain_min: 0.0,
            domain_max: 80.0,
        }),
        uncertainty_note: "±1 knot".to_owned(),
        revision_rationale: None,
    })?;

    let unit_err = graph.add_step(ReasoningStepDraft {
        step_id: OpaqueId::new("step:ext-unit-mismatch")?,
        version_id: VersionId::new("ver:ext-unit-mismatch")?,
        predecessor_version: None,
        schema_version: SchemaVersion::V1,
        actor: actor.clone(),
        inputs: vec![StepInputRef {
            node_version: p_knots.clone(),
            declared_origin_class: EvidenceOriginClass::NormalizedObservation,
        }],
        kind: ReasoningStepKind::Extrapolation(ExtrapolationSpec {
            domain_variable: "time".to_owned(),
            domain_unit: "s".to_owned(),
            observed_domain_min: 0.0,
            observed_domain_max: 60.0,
            observed_sample_count: 5,
            projection_horizon_start: 60.0,
            projection_horizon_end: 120.0,
            value_unit: "m/s".to_owned(),
            projected_value: 20.0,
            uncertainty_lower: 18.0,
            uncertainty_upper: 22.0,
            model_name: "linear".to_owned(),
            model_version: VersionId::new("ver:m1")?,
            uncertainty_method: "bounds".to_owned(),
            sensitivity_summary: "linear".to_owned(),
            explicit_assumptions: vec!["steady".to_owned()],
        }),
        revision_rationale: None,
    });
    assert_eq!(
        unit_err,
        Err(ReasoningError::UnitMismatch {
            expected_unit: "m/s".to_owned(),
            actual_unit: "knots".to_owned(),
        })
    );

    // Reject bidi override control characters and Lean `sorry` theorem identifiers.
    let sorry_err = graph.add_step(ReasoningStepDraft {
        step_id: OpaqueId::new("step:ded-sorry")?,
        version_id: VersionId::new("ver:ded-sorry")?,
        predecessor_version: None,
        schema_version: SchemaVersion::V1,
        actor: actor.clone(),
        inputs: vec![StepInputRef {
            node_version: p_knots.clone(),
            declared_origin_class: EvidenceOriginClass::NormalizedObservation,
        }],
        kind: ReasoningStepKind::Deduction(DeductionSpec {
            rule_system: "rules".to_owned(),
            explicit_assumptions: vec![],
            derivation_steps: vec![DerivationStep {
                step_number: 1,
                rule_name: "rule1".to_owned(),
                cited_input_versions: vec![p_knots.clone()],
                required_assumptions: vec![],
                inferred_statement: "derived".to_owned(),
            }],
            conditional_conclusion: "conclusion".to_owned(),
            verification_tier: ReasoningVerificationTier::AssertedUnverifiedLeanRef {
                theorem_id: "AssertedModelRef.sorry_placeholder".to_owned(),
                lean_toolchain: "leanprover/lean4:v4.35.0".to_owned(),
                model_revision: "unverified-fixture-ref".to_owned(),
                assumptions_summary: "none".to_owned(),
            },
            contradictions_noted: vec![],
        }),
        revision_rationale: None,
    });
    assert_eq!(
        sorry_err,
        Err(ReasoningError::InvalidVerificationMetadata("theorem_id"))
    );

    let overlap_err = graph.register_hypothesis(CompetingHypothesisDraft {
        hypothesis_id: OpaqueId::new("hyp:overlap")?,
        version_id: VersionId::new("ver:hyp-overlap")?,
        schema_version: SchemaVersion::V1,
        actor,
        title: "Overlapping hypothesis".to_owned(),
        explanation: "Cites same node as supporting and contradicting".to_owned(),
        supporting_node_versions: vec![p_knots.clone()],
        contradicting_node_versions: vec![p_knots.clone()],
        explicit_assumptions: vec![],
        discriminating_leads: vec![DiscriminatingEvidenceLead {
            lead_id: OpaqueId::new("lead:1")?,
            description: "Check log".to_owned(),
            expected_if_hypothesis_holds: "A".to_owned(),
            expected_if_alternative_holds: "B".to_owned(),
        }],
    });
    assert_eq!(
        overlap_err,
        Err(ReasoningError::HypothesisSupportContradictionOverlap(
            p_knots
        ))
    );

    Ok(())
}
