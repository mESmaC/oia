/-
  Projection of ForecastLedger::issue_forecast / validate_cutoff_and_sources.
  Preconditions excluded here: valid bounded timestamps, unique source versions,
  valid event times, nonempty retrospective limitation, and other draft validation.
  See README.md and ADR 0004 for the immutable Rust revision and correspondence gap.
-/

namespace OiaProofs

inductive CutoffMode where
  | strictProspective
  | retrospectiveReconstruction
  deriving DecidableEq, Repr

structure SourceTimes where
  publicAvailability : Option Int
  ingestedAt : Option Int
  deriving DecidableEq, Repr

/-- Unknown time is never evidence of availability at the cutoff. -/
def knownAtOrBefore (cutoff : Int) (time : Option Int) : Bool :=
  match time with
  | none => false
  | some t => decide (t ≤ cutoff)

def sourceAdmissible (mode : CutoffMode) (cutoff : Int) (source : SourceTimes) : Bool :=
  knownAtOrBefore cutoff source.publicAvailability &&
    match mode with
    | .strictProspective => knownAtOrBefore cutoff source.ingestedAt
    | .retrospectiveReconstruction => true

def modelAdmissible (mode : CutoffMode) (cutoff : Int) (knowledge : Option Int) : Bool :=
  match mode with
  | .strictProspective => knownAtOrBefore cutoff knowledge
  | .retrospectiveReconstruction => true

/-- Empty evidence is refused; every cited source must pass, not just one. -/
def inputsAdmissible (mode : CutoffMode) (cutoff : Int) (knowledge : Option Int)
    (sources : List SourceTimes) : Bool :=
  !sources.isEmpty && modelAdmissible mode cutoff knowledge &&
    sources.all (sourceAdmissible mode cutoff)

inductive CutoffDecision where
  | rejected
  | admitted (modelKnowledgeDisclosed : Bool)
  deriving DecidableEq, Repr

/-- Projection of success/rejection and the retained model-knowledge disclosure flag. -/
def cutoffDecision (mode : CutoffMode) (cutoff : Int) (knowledge : Option Int)
    (sources : List SourceTimes) : CutoffDecision :=
  if inputsAdmissible mode cutoff knowledge sources then
    .admitted (!knownAtOrBefore cutoff knowledge)
  else .rejected

theorem knownAtOrBefore_iff (cutoff : Int) (time : Option Int) :
    knownAtOrBefore cutoff time = true ↔ ∃ t, time = some t ∧ t ≤ cutoff := by
  cases time <;> simp [knownAtOrBefore]

theorem unknown_availability_never_admissible
    (mode : CutoffMode) (cutoff : Int) (ingested : Option Int) :
    sourceAdmissible mode cutoff ⟨none, ingested⟩ = false := by
  simp [sourceAdmissible, knownAtOrBefore]

/-- Exact characterization, including unknown ingestion (missing from the old model). -/
theorem strict_source_iff (cutoff : Int) (source : SourceTimes) :
    sourceAdmissible .strictProspective cutoff source = true ↔
      (∃ p, source.publicAvailability = some p ∧ p ≤ cutoff) ∧
      (∃ i, source.ingestedAt = some i ∧ i ≤ cutoff) := by
  simp [sourceAdmissible, knownAtOrBefore_iff]

theorem empty_sources_rejected (mode : CutoffMode) (cutoff : Int) (knowledge : Option Int) :
    cutoffDecision mode cutoff knowledge [] = .rejected := by
  simp [cutoffDecision, inputsAdmissible]

/-- No admitted source or model in strict mode has an unknown or future cutoff time. -/
theorem strict_inputs_no_leakage (cutoff : Int) (knowledge : Option Int)
    (sources : List SourceTimes)
    (h : inputsAdmissible .strictProspective cutoff knowledge sources = true) :
    sources ≠ [] ∧ (∃ k, knowledge = some k ∧ k ≤ cutoff) ∧
      ∀ s ∈ sources,
        (∃ p, s.publicAvailability = some p ∧ p ≤ cutoff) ∧
        (∃ i, s.ingestedAt = some i ∧ i ≤ cutoff) := by
  simp [inputsAdmissible, modelAdmissible, List.all_eq_true] at h
  exact ⟨h.1.1, (knownAtOrBefore_iff _ _).mp h.1.2,
    fun s hs => (strict_source_iff _ _).mp (h.2 s hs)⟩

theorem strict_implies_retrospective (cutoff : Int) (knowledge : Option Int)
    (sources : List SourceTimes)
    (h : inputsAdmissible .strictProspective cutoff knowledge sources = true) :
    inputsAdmissible .retrospectiveReconstruction cutoff knowledge sources = true := by
  obtain ⟨hne, _, hs⟩ := strict_inputs_no_leakage cutoff knowledge sources h
  simp only [inputsAdmissible, modelAdmissible, Bool.and_true, Bool.and_eq_true,
    Bool.not_eq_true', List.isEmpty_eq_false_iff, List.all_eq_true]
  exact ⟨hne, fun s hmem => by
    simpa [sourceAdmissible, knownAtOrBefore_iff] using (hs s hmem).1⟩

/-- A successful strict decision never needs the retrospective knowledge disclaimer. -/
theorem strict_admitted_without_disclosure (cutoff : Int) (knowledge : Option Int)
    (sources : List SourceTimes) (disclosed : Bool)
    (h : cutoffDecision .strictProspective cutoff knowledge sources = .admitted disclosed) :
    disclosed = false := by
  unfold cutoffDecision at h
  split at h
  · rename_i hadmit
    have hk := (strict_inputs_no_leakage cutoff knowledge sources hadmit).2.1
    have hknown := (knownAtOrBefore_iff cutoff knowledge).mpr hk
    simp [hknown] at h
    exact h
  · contradiction

/-- Admitting unknown or future model knowledge always retains the disclosure flag. -/
theorem admitted_disclosure_exact (mode : CutoffMode) (cutoff : Int)
    (knowledge : Option Int) (sources : List SourceTimes) (disclosed : Bool)
    (h : cutoffDecision mode cutoff knowledge sources = .admitted disclosed) :
    disclosed = true ↔ ¬ ∃ k, knowledge = some k ∧ k ≤ cutoff := by
  unfold cutoffDecision at h
  split at h
  · injection h with heq
    rw [← heq]
    simp [← knownAtOrBefore_iff]
  · contradiction

end OiaProofs
