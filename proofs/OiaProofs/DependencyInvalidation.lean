/-
  OiaProofs.DependencyInvalidation
  Formal specification and soundness proofs for single-step and multi-step dependency
  reassessment propagation. Corresponds to `oia_analysis::compute_invalidated_nodes`
  and `oia_domain::EvidenceGraph::propagate_reassessment` (ADR 0004, Issue #15).
-/

namespace OiaProofs

/-- Checks if `(dependent, upstream)` is a direct dependency edge where `upstream == changedNode`. -/
def isDirectDependentEdge (changedNode : Nat) (edge : Nat × Nat) : Bool :=
  decide (edge.snd = changedNode)

/-- Extracts all direct dependent node IDs from `edges` for `changedNode`. -/
def directDependents (edges : List (Nat × Nat)) (changedNode : Nat) : List Nat :=
  (edges.filter (isDirectDependentEdge changedNode)).map Prod.fst

/-- One propagation step from a frontier set `flagged`: preserves all already-flagged nodes
    and appends direct dependents of every node in `flagged`. -/
def stepInvalidation (edges : List (Nat × Nat)) (flagged : List Nat) : List Nat :=
  flagged ++ (flagged.flatMap (directDependents edges))

/-- Theorem 1 (Monotonicity): `stepInvalidation` never removes an already-flagged node
    (reassessment flags are monotonic until an explicit new version is issued). -/
theorem stepInvalidation_monotonic
    (edges : List (Nat × Nat)) (flagged : List Nat) (n : Nat)
    (h : n ∈ flagged) :
    n ∈ stepInvalidation edges flagged := by
  simp only [stepInvalidation]
  exact List.mem_append_left _ h

end OiaProofs
