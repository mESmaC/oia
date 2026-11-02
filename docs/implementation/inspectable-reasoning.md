# Inspectable reasoning and composed analysis graph

Implemented slice of [#35](https://github.com/mESmaC/oia/issues/35), supporting
`REASON-01`, `REASON-02`, `REASON-03`, and `REASON-04` in [the product specification](../spec.md),
Sections 2.3 and 6.2 of [the architecture](../architecture.md), and the
dependency-independent subset of acceptance scenario [A07](../delivery.md).
The Rust library entry points live in
[`oia_analysis::reasoning`](../../crates/oia-analysis/src/reasoning.rs).

## Contract

`ReasoningGraph` enforces a resource-bounded (`ReasoningLimits`), acyclic
in-memory reasoning graph across premises, composed reasoning steps, and
competing hypotheses:

1. **Versioned Premises (`ReasoningGraph::register_premise`, `revise_premise`):**
   - Each `PremiseRecord` pins `premise_id`, `version_id`, optional
     `predecessor_version`, `vintage_number`, `schema_version`, `actor`,
     `origin_class` (`EvidenceOriginClass`), `source_version`,
     `citation_anchor`, `verbatim_excerpt`, `proposition`, optional
     `MeasuredQuantity` (`value`, `unit`, `uncertainty`, `domain_min`,
     `domain_max`), and `uncertainty_note`.
   - Rejects non-finite numeric values, negative uncertainty, and values outside
     `[domain_min, domain_max]`.

2. **Abstraction (`REASON-03`, `ReasoningStepKind::Abstraction`):**
   - Normalizes reports into versioned event schemas and taxonomy concepts while
     preserving verbatim wording (`AbstractionFieldState::Normalized`), explicit
     unknowns (`AbstractionFieldState::ExplicitUnknown`), and conflicting
     source accounts (`AbstractionFieldState::Conflicting`).
   - Records explicit source/sensor omissions (`explicit_omissions`).
   - Keeps entity-match candidates (`EntityMatchCandidate`) and typed
     relationships (`AbstractionRelationship` with `AbstractionRelationType`)
     explicit and reversible via `DecisionState` (`Proposed`, `Confirmed`,
     `Reversed`).
   - Reversing or updating an entity match (`update_entity_match_decision`) or
     relationship (`update_relationship_decision`) requires a non-empty
     rationale, creates a new immutable step vintage, preserves prior vintages,
     and flags downstream dependents `needs_reassessment = true`.

3. **Correlation (`REASON-02`, `ReasoningStepKind::Correlation`):**
   - Requires positive finite `temporal_tolerance_seconds` and
     `spatial_tolerance_km`, explicit `attribute_matching_rule`,
     `evaluated_pair_count >= matched_pairs.len()`, `coverage_fraction` in
     `[0.0, 1.0]`, `coverage_limitations`, `BaselineRateSpec` (`EmpiricalRate`
     or `Unavailable { reason }`), `tested_hypotheses_count >= matched_pairs.len()`,
     and `multiple_comparison_method`.
   - Every `CorrelationPair` must reference declared step inputs, satisfy the
     declared temporal and spatial tolerances, and record
     `SourceDependencyStatus` (`IndependentSources`, `SharedUpstreamSource`, or
     `UnverifiedIndependence`).
   - Attaches `CORRELATION_NON_CAUSATION_NOTICE` so correlation never silently
     asserts causation or independent corroboration.

4. **Deduction (`REASON-01`, `ReasoningStepKind::Deduction`):**
   - Requires a named `rule_system`, explicit assumptions, sequential
     `DerivationStep` records citing declared step input versions, a
     `conditional_conclusion`, and explicit contradictions noted.
   - Keeps `ReasoningVerificationTier` variants distinct:
     - `AiProposed { model_id, prompt_template_version }`
     - `HumanReviewed { reviewer, review_note }`
     - `AssertedUnverifiedLeanRef { theorem_id, lean_toolchain, model_revision, assumptions_summary }`
       (records a caller-asserted theorem/model reference without claiming runtime
       Lean verification or formal Rust implementation correspondence; rejects
       placeholder `sorry`/`admit` theorem identifiers, and
       `establishes_lean_proof_or_correspondence()` returns `false`).
   - Attaches `DEDUCTION_CONDITIONALITY_NOTICE` stating that validity is
     conditional on the stated premises and assumptions.

5. **Extrapolation (`REASON-02`, `ReasoningStepKind::Extrapolation`):**
   - Requires finite `observed_domain_min < observed_domain_max`,
     `observed_sample_count >= 2`, projection horizon
     `observed_domain_max <= projection_horizon_start < projection_horizon_end`,
     uncertainty bounds bracketing `projected_value`, pinned `model_name` and
     `model_version`, `uncertainty_method`, `sensitivity_summary`, and
     non-empty `explicit_assumptions`.
   - Validates unit and domain compatibility against quantitative premise inputs
     (`ReasoningError::UnitMismatch` and `UnsupportedDomainValue`).
   - Attaches `EXTRAPOLATION_PROJECTION_NOTICE`.

6. **Composed Dependencies, Origin Preservation & Competing Hypotheses (`REASON-04`, `A07`):**
   - Every `ReasoningStepRecord` outputs `EvidenceOriginClass::AnalyticalInference`
     (`can_serve_as_observation() == false`).
   - Downstream steps must declare each upstream input's actual
     `EvidenceOriginClass` (`ReasoningError::InputOriginClassMismatch`), so
     feeding an inference into another step never promotes it to
     `NormalizedObservation`.
   - `CompetingHypothesisRecord` retains supporting and contradicting node
     versions (rejecting overlap), explicit assumptions, and
     `DiscriminatingEvidenceLead` entries side by side without forcing a single
     answer.
   - Revising a premise or upstream step preserves all prior versions and
     transitively marks dependent steps and hypotheses `needs_reassessment = true`
     with a structured `ReassessmentTrigger` audit trail.

## Limitations and remaining `#35` scope

- **In-memory kernel only:** `ReasoningGraph` does not persist records to SQLite
  (`oia-store`, `#11`) or schedule background worker jobs (`oia-app`, `#14`).
- **Asserted, unverified Lean reference tier:** `ReasoningVerificationTier::AssertedUnverifiedLeanRef`
  records a caller-asserted theorem identifier, toolchain, and model revision;
  because `proofs/` (`#15`) is not yet on `main` and a Lean model proof does not
  automatically establish Rust implementation correspondence (`AGENTS.md` §3),
  this kernel makes no formal proof claim (`establishes_lean_proof_or_correspondence() == false`).
- **UI workbench:** Visual graph inspection and interactive hypothesis comparison
  in the workspace UI follow in `#16`.

## Local verification

Run with pinned Rust `1.99.0`:

```text
cargo +1.99.0 run -p oia-analysis --example inspect-a07-reasoning --locked --offline
cargo +1.99.0 fmt --all -- --check
cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings
cargo +1.99.0 test --workspace --locked --offline
```
