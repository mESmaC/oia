//! `oia-analysis` — Versioned analysis definitions, deduction/extrapolation/correlation/abstraction
//! contracts, verification badges, and dependency invalidation (`REASON-01`–`REASON-04`, `A07`).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

use oia_domain::{
    ActorId, DomainError, EvidenceGraph, ObjectEnvelope, OpaqueId, OriginClass, ReassessmentState,
    VersionId,
};

/// Explicit verification claim classification (`AGENTS.md` Section 3, `ADR 0004`, `E17`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationStatus {
    /// Unverified or draft claim.
    Unverified,
    /// Mathematical/algorithmic property proved on a Lean 4 model (`proofs/`).
    ModelPropertyProved,
    /// Lean 4 model property proved AND deterministic Rust correspondence fixtures executed.
    RustCorrespondenceTested,
    /// Empirically evaluated against held-out or independent reference data.
    EmpiricallyEvaluated,
}

/// Explicit verification badge distinguishing AI-proposed, human-reviewed, and Lean-checked
/// reasoning (`REASON-01`, `AGENTS.md` Section 3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationBadge {
    /// Unreviewed draft proposed by an AI model.
    AiProposed {
        /// Model and prompt version identifier.
        model_id: String,
    },
    /// Inspected and reviewed by a human analyst (conditional on premises).
    HumanReviewed {
        /// Reviewing actor ID.
        reviewer: ActorId,
    },
    /// Covered by a proved Lean 4 model theorem with tested Rust correspondence.
    ///
    /// Note: tests connecting Rust to a Lean model establish correspondence evidence, not
    /// automatically a formal proof of the compiled Rust machine code (`AGENTS.md` Section 3).
    LeanModelProved {
        /// Fully qualified Lean theorem name.
        theorem_name: String,
        /// Pinned Lean toolchain identifier.
        lean_toolchain: String,
        /// Stated assumptions of the model.
        assumptions: Vec<String>,
        /// Whether Rust-to-Lean correspondence fixtures have been executed.
        rust_correspondence_tested: bool,
    },
}

impl VerificationBadge {
    /// Returns the [`VerificationStatus`] level represented by this badge.
    #[must_use]
    pub const fn verification_status(&self) -> VerificationStatus {
        match self {
            Self::AiProposed { .. } | Self::HumanReviewed { .. } => VerificationStatus::Unverified,
            Self::LeanModelProved {
                rust_correspondence_tested: true,
                ..
            } => VerificationStatus::RustCorrespondenceTested,
            Self::LeanModelProved {
                rust_correspondence_tested: false,
                ..
            } => VerificationStatus::ModelPropertyProved,
        }
    }
}

/// Reasoning or analytical method family (`REASON-01`–`REASON-04`).
#[derive(Debug, Clone, PartialEq)]
pub enum AnalysisMethodSpec {
    /// Conditional logical or mathematical derivation from explicit premises and rules (`REASON-01`).
    Deduction {
        /// Explicit premises (version IDs or stated assumptions).
        premises: Vec<String>,
        /// Inference rule applied.
        rule: String,
    },
    /// Projection beyond an observed domain with explicit horizon and uncertainty method (`REASON-02`).
    Extrapolation {
        /// Description of observed input domain.
        observed_domain: String,
        /// Projection horizon with units.
        projection_horizon: String,
        /// Model name and uncertainty estimation method.
        uncertainty_method: String,
    },
    /// Statistical or spatio-temporal association that never silently asserts causation (`REASON-02`).
    Correlation {
        /// Matching tolerance with explicit units.
        matching_tolerance: String,
        /// Number of candidate comparisons tested (for multiple-comparison disclosure).
        comparisons_tested: u32,
        /// Explicit reminder that correlation does not establish causation.
        causation_established: bool,
    },
    /// Normalization of reports into editable event structures and reversible entity links (`REASON-03`).
    Abstraction {
        /// Taxonomy or schema version used.
        taxonomy_version: String,
        /// Count of preserved conflicts or unknown fields.
        preserved_unknown_count: usize,
    },
}

impl AnalysisMethodSpec {
    /// Validates method preconditions (e.g., `Correlation` cannot claim `causation_established = true`
    /// without independent causal evidence).
    pub fn validate(&self) -> Result<(), AnalysisError> {
        match self {
            Self::Deduction { premises, rule } => {
                if premises.is_empty() || rule.trim().is_empty() {
                    return Err(AnalysisError::IncompleteDeductionSpec);
                }
                Ok(())
            }
            Self::Extrapolation {
                observed_domain,
                projection_horizon,
                uncertainty_method,
            } => {
                if observed_domain.trim().is_empty()
                    || projection_horizon.trim().is_empty()
                    || uncertainty_method.trim().is_empty()
                {
                    return Err(AnalysisError::IncompleteExtrapolationSpec);
                }
                Ok(())
            }
            Self::Correlation {
                matching_tolerance,
                comparisons_tested,
                causation_established,
            } => {
                if *causation_established {
                    return Err(AnalysisError::CorrelationCannotAssertCausation);
                }
                if matching_tolerance.trim().is_empty() || *comparisons_tested == 0 {
                    return Err(AnalysisError::IncompleteCorrelationSpec);
                }
                Ok(())
            }
            Self::Abstraction {
                taxonomy_version, ..
            } => {
                if taxonomy_version.trim().is_empty() {
                    return Err(AnalysisError::IncompleteAbstractionSpec);
                }
                Ok(())
            }
        }
    }
}

/// Pinned execution record of an analysis step (`AnalysisRun`, `docs/architecture.md` Section 6.2).
#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisRun {
    /// Common object envelope (`DomainObjectType::AnalysisRun`).
    pub envelope: ObjectEnvelope,
    /// Analysis definition identifier and version.
    pub definition_id: OpaqueId,
    /// Method specification (`Deduction`, `Extrapolation`, `Correlation`, `Abstraction`).
    pub method: AnalysisMethodSpec,
    /// Exact pinned input version IDs.
    pub input_versions: Vec<VersionId>,
    /// Origin classes of all inputs (used to prevent promoting inferences/predictive derivatives into observations).
    pub input_origin_classes: Vec<OriginClass>,
    /// Optional pinned camera/sensor calibration version ID.
    pub calibration_version: Option<VersionId>,
    /// Named parameters with explicit units (e.g., `"window_seconds" -> "300 s"`).
    pub parameters_with_units: BTreeMap<String, String>,
    /// Optional deterministic random seed.
    pub random_seed: Option<u64>,
    /// Verification level of this run.
    pub verification: VerificationBadge,
    /// Output summary and reproducibility notes.
    pub output_summary: String,
}

impl AnalysisRun {
    /// Validates envelope, method specification, and origin-class derivation rules (`A07`, `B9`).
    ///
    /// Rejects runs where `input_origin_classes.len() != input_versions.len()` so an empty
    /// caller-supplied `input_origin_classes` slice can never bypass origin-promotion guards.
    pub fn validate(&self) -> Result<(), AnalysisError> {
        self.envelope.validate().map_err(AnalysisError::Domain)?;
        self.method.validate()?;
        if self.input_versions.is_empty() {
            return Err(AnalysisError::NoInputVersionsPinned);
        }
        if self.input_origin_classes.len() != self.input_versions.len() {
            return Err(AnalysisError::InputOriginClassCountMismatch {
                versions_len: self.input_versions.len(),
                classes_len: self.input_origin_classes.len(),
            });
        }
        OriginClass::validate_derivation(&self.input_origin_classes, self.envelope.origin_class)
            .map_err(AnalysisError::Domain)?;
        Ok(())
    }

    /// Validates this run against the actual [`EvidenceGraph`], deriving each input's [`OriginClass`]
    /// and transitive non-observational taint directly from the stored `input_versions` (`B9`).
    pub fn validate_against_graph(&self, graph: &EvidenceGraph) -> Result<(), AnalysisError> {
        self.validate()?;
        let mut graph_classes = Vec::with_capacity(self.input_versions.len());
        for (idx, ver) in self.input_versions.iter().enumerate() {
            let obj = graph.get_version(ver).ok_or_else(|| {
                AnalysisError::Domain(DomainError::UnknownInputVersion(ver.clone()))
            })?;
            let actual_class = obj.envelope().origin_class;
            if self.input_origin_classes.get(idx) != Some(&actual_class) {
                return Err(AnalysisError::InputOriginClassMismatch {
                    version_id: ver.clone(),
                    declared: self.input_origin_classes[idx],
                    actual: actual_class,
                });
            }
            graph_classes.push(actual_class);
            if graph.has_non_observational_ancestor(ver) {
                graph_classes.push(OriginClass::AnalyticalInference);
            }
        }
        OriginClass::validate_derivation(&graph_classes, self.envelope.origin_class)
            .map_err(AnalysisError::Domain)?;
        Ok(())
    }

    /// Marks this run as needing reassessment due to an upstream premise, source, or calibration change.
    pub fn flag_for_reassessment(
        &mut self,
        changed_input_version: VersionId,
        reason: impl Into<String>,
        flagged_at_ms: i64,
    ) {
        let reason_str = reason.into();
        match &mut self.envelope.reassessment {
            ReassessmentState::Current => {
                self.envelope.reassessment = ReassessmentState::NeedsReassessment {
                    upstream_versions: vec![changed_input_version],
                    reasons: vec![reason_str],
                    flagged_at_ms,
                };
            }
            ReassessmentState::NeedsReassessment {
                upstream_versions,
                reasons,
                flagged_at_ms: ts,
            } => {
                if !upstream_versions.contains(&changed_input_version) {
                    upstream_versions.push(changed_input_version);
                }
                if !reasons.contains(&reason_str) {
                    reasons.push(reason_str);
                }
                *ts = (*ts).max(flagged_at_ms);
            }
        }
    }
}

/// Pure integer-node transitive dependency invalidation algorithm corresponding to
/// `proofs/OiaProofs/DependencyInvalidation.lean`.
///
/// Given a list of directed dependency pairs `(dependent_node, upstream_node)` where
/// `dependent_node` relies on `upstream_node`, returns the sorted, deduplicated list of all
/// nodes transitively invalidated when `revised_root` changes (including `revised_root` itself).
#[must_use]
pub fn compute_invalidated_nodes(edges: &[(u32, u32)], revised_root: u32) -> Vec<u32> {
    let mut visited = BTreeSet::new();
    let mut queue = VecDeque::new();
    visited.insert(revised_root);
    queue.push_back(revised_root);

    while let Some(curr) = queue.pop_front() {
        for &(dependent, upstream) in edges {
            if upstream == curr && visited.insert(dependent) {
                queue.push_back(dependent);
            }
        }
    }

    visited.into_iter().collect()
}

/// Typed errors for `oia-analysis`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalysisError {
    /// Underlying domain validation error.
    Domain(DomainError),
    /// Analysis run did not pin any input versions.
    NoInputVersionsPinned,
    /// `input_origin_classes.len()` did not match `input_versions.len()` (`B9`).
    InputOriginClassCountMismatch {
        /// Number of pinned input versions.
        versions_len: usize,
        /// Number of declared input origin classes.
        classes_len: usize,
    },
    /// Declared input origin class did not match the stored object's origin class in the graph (`B9`).
    InputOriginClassMismatch {
        /// Input version ID.
        version_id: VersionId,
        /// Caller-declared origin class.
        declared: OriginClass,
        /// Actual origin class in the graph.
        actual: OriginClass,
    },
    /// Deduction lacked premises or an explicit rule.
    IncompleteDeductionSpec,
    /// Extrapolation lacked observed domain, horizon, or uncertainty method.
    IncompleteExtrapolationSpec,
    /// Correlation attempted to assert causation (`REASON-02`).
    CorrelationCannotAssertCausation,
    /// Correlation lacked tolerance or comparison count.
    IncompleteCorrelationSpec,
    /// Abstraction lacked taxonomy version.
    IncompleteAbstractionSpec,
}

impl fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(e) => write!(f, "analysis domain error: {e}"),
            Self::NoInputVersionsPinned => {
                write!(f, "analysis run must pin at least one input version")
            }
            Self::InputOriginClassCountMismatch {
                versions_len,
                classes_len,
            } => write!(
                f,
                "analysis run pinned {versions_len} input versions but declared {classes_len} origin classes"
            ),
            Self::InputOriginClassMismatch {
                version_id,
                declared,
                actual,
            } => write!(
                f,
                "input version '{version_id}' has origin class '{}', not declared '{}'",
                actual.as_str(),
                declared.as_str()
            ),
            Self::IncompleteDeductionSpec => {
                write!(
                    f,
                    "deduction requires non-empty premises and inference rule"
                )
            }
            Self::IncompleteExtrapolationSpec => write!(
                f,
                "extrapolation requires observed domain, projection horizon, and uncertainty method"
            ),
            Self::CorrelationCannotAssertCausation => {
                write!(
                    f,
                    "correlation analysis must not assert causation (REASON-02)"
                )
            }
            Self::IncompleteCorrelationSpec => {
                write!(
                    f,
                    "correlation requires matching tolerance and comparisons_tested >= 1"
                )
            }
            Self::IncompleteAbstractionSpec => {
                write!(f, "abstraction requires a non-empty taxonomy version")
            }
        }
    }
}

impl std::error::Error for AnalysisError {}

#[cfg(test)]
mod tests {
    use super::*;
    use oia_domain::{
        AvailabilityState, Claim, ClaimAssessment, ClaimNature, DomainObject, DomainObjectType,
        Hypothesis, ProvenanceRecord, ReviewStatus, SchemaVersion, WorkspaceVisibility,
    };

    #[test]
    fn validate_against_graph_derives_input_classes_and_transitive_taint_from_graph()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut graph = EvidenceGraph::new();
        let actor = ActorId::new("analyst")?;
        let case_id = OpaqueId::new("case_b9")?;

        // 1. Insert a non-observational root (`Hypothesis` with `AnalyticalInference`).
        let hyp_ver = VersionId::new("ver_hyp_root")?;
        let hyp = Hypothesis {
            envelope: ObjectEnvelope {
                id: OpaqueId::new("hyp_root")?,
                version_id: hyp_ver.clone(),
                schema_version: SchemaVersion::V1,
                object_type: DomainObjectType::Hypothesis,
                created_at_ms: 1_700_000_000_000,
                created_by: actor.clone(),
                visibility: WorkspaceVisibility::PrivateCase {
                    case_id: case_id.clone(),
                    sensitive: false,
                },
                supersedes: None,
                provenance: ProvenanceRecord {
                    source_version_ids: Vec::new(),
                    declared_origin_note: Some("Analyst hypothesis".to_owned()),
                    processing_history: Vec::new(),
                },
                origin_class: OriginClass::AnalyticalInference,
                review_status: ReviewStatus::Unreviewed,
                availability: AvailabilityState::Available,
                reassessment: ReassessmentState::Current,
            },
            statement: "Relay timing anomaly".to_owned(),
            supporting_versions: Vec::new(),
            contradicting_versions: Vec::new(),
            discriminating_observables: vec!["Packet gap > 100ms".to_owned()],
        };
        graph.insert_object(DomainObject::Hypothesis(hyp), None)?;

        // 2. Insert a two-hop `AnalyticalInference` claim derived from `ver_hyp_root`.
        let claim_ver = VersionId::new("ver_claim_summary")?;
        let claim = Claim {
            envelope: ObjectEnvelope {
                id: OpaqueId::new("claim_summary")?,
                version_id: claim_ver.clone(),
                schema_version: SchemaVersion::V1,
                object_type: DomainObjectType::Claim,
                created_at_ms: 1_700_000_001_000,
                created_by: actor.clone(),
                visibility: WorkspaceVisibility::PrivateCase {
                    case_id,
                    sensitive: false,
                },
                supersedes: None,
                provenance: ProvenanceRecord {
                    source_version_ids: vec![hyp_ver.clone()],
                    declared_origin_note: None,
                    processing_history: Vec::new(),
                },
                origin_class: OriginClass::AnalyticalInference,
                review_status: ReviewStatus::Unreviewed,
                availability: AvailabilityState::Available,
                reassessment: ReassessmentState::Current,
            },
            proposition: "Intermediate analytical summary".to_owned(),
            claimant: "analyst".to_owned(),
            nature: ClaimNature::AnalyticalProposal,
            supporting_anchor: None,
            assessment: ClaimAssessment::Unresolved,
            assessment_rationale: "Summary of hypothesis".to_owned(),
        };
        graph.insert_object(DomainObject::Claim(claim), None)?;

        // 3. Spoofed `input_origin_classes` claiming `ver_hyp_root` is `AcquiredSourceMaterial` is rejected.
        let make_run = |inputs: Vec<VersionId>,
                        declared_classes: Vec<OriginClass>,
                        output_class: OriginClass|
         -> Result<AnalysisRun, DomainError> {
            Ok(AnalysisRun {
                envelope: ObjectEnvelope {
                    id: OpaqueId::new("run_test")?,
                    version_id: VersionId::new("ver_run_test")?,
                    schema_version: SchemaVersion::V1,
                    object_type: DomainObjectType::AnalysisRun,
                    created_at_ms: 1_700_000_002_000,
                    created_by: actor.clone(),
                    visibility: WorkspaceVisibility::LibraryPublic,
                    supersedes: None,
                    provenance: ProvenanceRecord {
                        source_version_ids: inputs.clone(),
                        declared_origin_note: None,
                        processing_history: Vec::new(),
                    },
                    origin_class: output_class,
                    review_status: ReviewStatus::Unreviewed,
                    availability: AvailabilityState::Available,
                    reassessment: ReassessmentState::Current,
                },
                definition_id: OpaqueId::new("def_1")?,
                method: AnalysisMethodSpec::Deduction {
                    premises: vec!["P1".to_owned()],
                    rule: "Modus ponens".to_owned(),
                },
                input_versions: inputs,
                input_origin_classes: declared_classes,
                calibration_version: None,
                parameters_with_units: BTreeMap::new(),
                random_seed: Some(7),
                verification: VerificationBadge::HumanReviewed {
                    reviewer: actor.clone(),
                },
                output_summary: "Checked".to_owned(),
            })
        };

        let spoofed_run = make_run(
            vec![hyp_ver],
            vec![OriginClass::AcquiredSourceMaterial],
            OriginClass::NormalizedObservation,
        )?;
        assert!(matches!(
            spoofed_run.validate_against_graph(&graph),
            Err(AnalysisError::InputOriginClassMismatch {
                declared: OriginClass::AcquiredSourceMaterial,
                actual: OriginClass::AnalyticalInference,
                ..
            })
        ));

        // 4. Attempting to promote `ver_claim_summary` (`AnalyticalInference`) to `NormalizedObservation`
        //    is rejected by `validate_against_graph`.
        let transitive_promotion = make_run(
            vec![claim_ver.clone()],
            vec![OriginClass::AnalyticalInference],
            OriginClass::NormalizedObservation,
        )?;
        assert!(matches!(
            transitive_promotion.validate_against_graph(&graph),
            Err(AnalysisError::Domain(DomainError::IllegalOriginPromotion {
                target: OriginClass::NormalizedObservation
            }))
        ));

        // 5. Valid `AnalyticalInference` output succeeds against the graph.
        let valid_run = make_run(
            vec![claim_ver],
            vec![OriginClass::AnalyticalInference],
            OriginClass::AnalyticalInference,
        )?;
        valid_run.validate_against_graph(&graph)?;
        Ok(())
    }
}
