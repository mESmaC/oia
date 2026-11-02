//! `oia-analysis` — Versioned analysis definitions, deduction/extrapolation/correlation/abstraction
//! contracts, verification badges, and dependency invalidation (`REASON-01`–`REASON-04`, `A07`).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

use oia_domain::{
    ActorId, DomainError, ObjectEnvelope, OpaqueId, OriginClass, ReassessmentState, VersionId,
};

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
    /// Validates envelope, method specification, and origin-class derivation rules (`A07`).
    pub fn validate(&self) -> Result<(), AnalysisError> {
        self.envelope.validate().map_err(AnalysisError::Domain)?;
        self.method.validate()?;
        if self.input_versions.is_empty() {
            return Err(AnalysisError::NoInputVersionsPinned);
        }
        OriginClass::validate_derivation(&self.input_origin_classes, self.envelope.origin_class)
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
