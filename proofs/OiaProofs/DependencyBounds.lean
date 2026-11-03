import OiaProofs.DependencyInvalidation

namespace OiaProofs

inductive DependencyPath (edges : List (Nat × Nat)) (origin : Nat) : Nat → Nat → Prop where
  | zero : DependencyPath edges origin origin 0
  | step {upstream dependent hops : Nat}
      (preceding : DependencyPath edges origin upstream hops)
      (edge : (dependent, upstream) ∈ edges) :
      DependencyPath edges origin dependent (hops + 1)

theorem stepInvalidation_mem_iff (edges : List (Nat × Nat)) (flagged : List Nat)
    (node : Nat) :
    node ∈ stepInvalidation edges flagged ↔
      node ∈ flagged ∨ ∃ upstream ∈ flagged, (node, upstream) ∈ edges := by
  simp only [stepInvalidation, List.mem_append, List.mem_flatMap]
  constructor
  · rintro (retained | ⟨upstream, present, dependent⟩)
    · exact Or.inl retained
    · exact Or.inr ⟨upstream, present, edge_of_mem_directDependents edges node upstream dependent⟩
  · rintro (retained | ⟨upstream, present, edge⟩)
    · exact Or.inl retained
    · exact Or.inr ⟨upstream, present, mem_directDependents_of_edge edges node upstream edge⟩

theorem invalidateClosure_fuel_mono (edges : List (Nat × Nat)) (seeds : List Nat)
    (smaller larger node : Nat) (budget : smaller ≤ larger)
    (present : node ∈ invalidateClosure edges smaller seeds) :
    node ∈ invalidateClosure edges larger seeds := by
  induction budget with
  | refl => exact present
  | @step upper _ inductionHypothesis =>
    rw [invalidateClosure_succ]
    exact stepInvalidation_monotonic edges _ node inductionHypothesis

theorem DependencyPath.mem_closure {edges : List (Nat × Nat)} {origin node hops : Nat}
    (path : DependencyPath edges origin node hops) (seeds : List Nat)
    (seed : origin ∈ seeds) :
    node ∈ invalidateClosure edges hops seeds := by
  induction path with
  | zero => exact seed
  | @step upstream dependent steps _ edge inductionHypothesis =>
    rw [invalidateClosure_succ]
    exact stepInvalidation_includes_direct_dependent edges _ dependent upstream
      inductionHypothesis edge

theorem invalidateClosure_mem_iff_bounded_path (edges : List (Nat × Nat))
    (seeds : List Nat) (fuel node : Nat) :
    node ∈ invalidateClosure edges fuel seeds ↔
      ∃ origin ∈ seeds, ∃ hops ≤ fuel, DependencyPath edges origin node hops := by
  constructor
  · induction fuel generalizing node with
    | zero =>
      intro present
      exact ⟨node, present, 0, Nat.le_refl 0, DependencyPath.zero⟩
    | succ steps inductionHypothesis =>
      rw [invalidateClosure_succ, stepInvalidation_mem_iff]
      rintro (retained | ⟨upstream, present, edge⟩)
      · obtain ⟨origin, seed, hops, budget, path⟩ := inductionHypothesis node retained
        exact ⟨origin, seed, hops, Nat.le_trans budget (Nat.le_succ steps), path⟩
      · obtain ⟨origin, seed, hops, budget, path⟩ := inductionHypothesis upstream present
        exact ⟨origin, seed, hops + 1, Nat.succ_le_succ budget, DependencyPath.step path edge⟩
  · rintro ⟨origin, seed, hops, budget, path⟩
    exact invalidateClosure_fuel_mono edges seeds hops fuel node budget (path.mem_closure seeds seed)

theorem invalidateClosure_not_mem_iff_long_paths (edges : List (Nat × Nat))
    (seeds : List Nat) (fuel node : Nat) :
    node ∉ invalidateClosure edges fuel seeds ↔
      ∀ origin ∈ seeds, ∀ hops, DependencyPath edges origin node hops → fuel < hops := by
  constructor
  · intro absent origin seed hops path
    apply Nat.lt_of_not_ge
    intro budget
    exact absent ((invalidateClosure_mem_iff_bounded_path edges seeds fuel node).mpr
      ⟨origin, seed, hops, budget, path⟩)
  · intro allLong present
    obtain ⟨origin, seed, hops, budget, path⟩ :=
      (invalidateClosure_mem_iff_bounded_path edges seeds fuel node).mp present
    exact Nat.not_lt_of_ge budget (allLong origin seed hops path)

theorem invalidateClosure_mono (smallerEdges largerEdges : List (Nat × Nat))
    (smallerSeeds largerSeeds : List Nat) (fuel node : Nat)
    (edgesIncluded : ∀ edge ∈ smallerEdges, edge ∈ largerEdges)
    (seedsIncluded : ∀ seed ∈ smallerSeeds, seed ∈ largerSeeds)
    (present : node ∈ invalidateClosure smallerEdges fuel smallerSeeds) :
    node ∈ invalidateClosure largerEdges fuel largerSeeds := by
  induction fuel generalizing node with
  | zero => exact seedsIncluded node present
  | succ steps inductionHypothesis =>
    rw [invalidateClosure_succ, stepInvalidation_mem_iff] at present ⊢
    rcases present with retained | ⟨upstream, present, edge⟩
    · exact Or.inl (inductionHypothesis node retained)
    · exact Or.inr ⟨upstream, inductionHypothesis upstream present,
        edgesIncluded (node, upstream) edge⟩

theorem invalidateClosure_membership_congr (firstEdges secondEdges : List (Nat × Nat))
    (firstSeeds secondSeeds : List Nat) (fuel node : Nat)
    (sameEdges : ∀ edge, edge ∈ firstEdges ↔ edge ∈ secondEdges)
    (sameSeeds : ∀ seed, seed ∈ firstSeeds ↔ seed ∈ secondSeeds) :
    node ∈ invalidateClosure firstEdges fuel firstSeeds ↔
      node ∈ invalidateClosure secondEdges fuel secondSeeds := by
  constructor
  · exact invalidateClosure_mono firstEdges secondEdges firstSeeds secondSeeds fuel node
      (fun edge present => (sameEdges edge).mp present)
      (fun seed present => (sameSeeds seed).mp present)
  · exact invalidateClosure_mono secondEdges firstEdges secondSeeds firstSeeds fuel node
      (fun edge present => (sameEdges edge).mpr present)
      (fun seed present => (sameSeeds seed).mpr present)

theorem invalidateClosure_append_seeds (edges : List (Nat × Nat))
    (firstSeeds secondSeeds : List Nat) (fuel node : Nat) :
    node ∈ invalidateClosure edges fuel (firstSeeds ++ secondSeeds) ↔
      node ∈ invalidateClosure edges fuel firstSeeds ∨
        node ∈ invalidateClosure edges fuel secondSeeds := by
  simp only [invalidateClosure_mem_iff_bounded_path, List.mem_append]
  constructor
  · rintro ⟨origin, seed, hops, budget, path⟩
    rcases seed with first | second
    · exact Or.inl ⟨origin, first, hops, budget, path⟩
    · exact Or.inr ⟨origin, second, hops, budget, path⟩
  · rintro (⟨origin, seed, hops, budget, path⟩ | ⟨origin, seed, hops, budget, path⟩)
    · exact ⟨origin, Or.inl seed, hops, budget, path⟩
    · exact ⟨origin, Or.inr seed, hops, budget, path⟩

theorem stepInvalidation_closed_iff (edges : List (Nat × Nat)) (flagged : List Nat) :
    (∀ node ∈ stepInvalidation edges flagged, node ∈ flagged) ↔
      ∀ upstream ∈ flagged, ∀ dependent, (dependent, upstream) ∈ edges → dependent ∈ flagged := by
  constructor
  · intro closed upstream present dependent edge
    exact closed dependent
      (stepInvalidation_includes_direct_dependent edges flagged dependent upstream present edge)
  · intro closed node present
    rw [stepInvalidation_mem_iff] at present
    rcases present with retained | ⟨upstream, present, edge⟩
    · exact retained
    · exact closed upstream present node edge

theorem invalidateClosure_closed_mem_iff (edges : List (Nat × Nat)) (flagged : List Nat)
    (fuel node : Nat) (closed : ∀ next ∈ stepInvalidation edges flagged, next ∈ flagged) :
    node ∈ invalidateClosure edges fuel flagged ↔ node ∈ flagged := by
  constructor
  · induction fuel generalizing node with
    | zero => exact fun present => present
    | succ steps inductionHypothesis =>
      rw [invalidateClosure_succ, stepInvalidation_mem_iff]
      rintro (retained | ⟨upstream, present, edge⟩)
      · exact inductionHypothesis node retained
      · exact closed node (stepInvalidation_includes_direct_dependent edges flagged node upstream
          (inductionHypothesis upstream present) edge)
  · exact invalidateClosure_monotonic edges fuel flagged node

theorem invalidateClosure_fixed_point_complete (edges : List (Nat × Nat)) (seeds : List Nat)
    (fuel node : Nat)
    (closed : ∀ next ∈ stepInvalidation edges (invalidateClosure edges fuel seeds),
      next ∈ invalidateClosure edges fuel seeds) :
    node ∈ invalidateClosure edges fuel seeds ↔ IsReachable edges seeds node := by
  constructor
  · exact invalidateClosure_sound edges fuel seeds node
  · intro reachable
    induction reachable with
    | base seed => exact invalidateClosure_monotonic edges fuel seeds _ seed
    | @step upstream dependent _ edge inductionHypothesis =>
      exact closed dependent (stepInvalidation_includes_direct_dependent edges _ dependent upstream
        inductionHypothesis edge)

example : 2 ∉ invalidateClosure [(1, 0), (2, 1)] 1 [0] ∧
    2 ∈ invalidateClosure [(1, 0), (2, 1)] 2 [0] := by
  decide

example : invalidateClosure [(1, 0), (0, 1), (1, 0), (3, 3)] 0 [0, 0] = [0, 0] ∧
    1 ∈ invalidateClosure [(1, 0), (0, 1), (1, 0), (3, 3)] 3 [0, 0] ∧
    3 ∉ invalidateClosure [(1, 0), (0, 1), (1, 0), (3, 3)] 3 [0, 0] ∧
    invalidateClosure [(1, 0)] 3 [] = [] := by
  decide

end OiaProofs
