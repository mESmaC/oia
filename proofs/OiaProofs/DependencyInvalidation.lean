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

/-- Multi-step bounded transitive closure of dependency reassessment propagation (`fuel` steps). -/
def invalidateClosure (edges : List (Nat × Nat)) (fuel : Nat) (flagged : List Nat) : List Nat :=
  match fuel with
  | 0 => flagged
  | k + 1 => invalidateClosure edges k (stepInvalidation edges flagged)

/-- Theorem 2 (Multi-step Monotonicity): `invalidateClosure` preserves all initially flagged
    nodes across any number of propagation steps. -/
theorem invalidateClosure_monotonic
    (edges : List (Nat × Nat)) (fuel : Nat) (flagged : List Nat) (n : Nat)
    (h : n ∈ flagged) :
    n ∈ invalidateClosure edges fuel flagged := by
  induction fuel generalizing flagged with
  | zero =>
    exact h
  | succ k ih =>
    simp only [invalidateClosure]
    exact ih (stepInvalidation edges flagged) (stepInvalidation_monotonic edges flagged n h)

/-- Helper lemma: if `(dep, src)` is in `edges`, then `dep` is in `directDependents edges src`. -/
theorem mem_directDependents_of_edge
    (edges : List (Nat × Nat)) (dep src : Nat)
    (hEdge : (dep, src) ∈ edges) :
    dep ∈ directDependents edges src := by
  simp only [directDependents, List.mem_map, List.mem_filter, isDirectDependentEdge]
  exact ⟨(dep, src), ⟨hEdge, decide_eq_true rfl⟩, rfl⟩

/-- Theorem 3 (One-step Direct-Edge Closure): If `src ∈ flagged` and `(dep, src) ∈ edges`,
    then `dep ∈ stepInvalidation edges flagged`. Consequently, at any fixed point where
    `∀ x ∈ stepInvalidation edges flagged, x ∈ flagged`, the flagged set is closed under `edges`. -/
theorem stepInvalidation_includes_direct_dependent
    (edges : List (Nat × Nat)) (flagged : List Nat) (dep src : Nat)
    (hSrc : src ∈ flagged)
    (hEdge : (dep, src) ∈ edges) :
    dep ∈ stepInvalidation edges flagged := by
  simp only [stepInvalidation]
  apply List.mem_append_right
  rw [List.mem_flatMap]
  exact ⟨src, hSrc, mem_directDependents_of_edge edges dep src hEdge⟩

end OiaProofs
