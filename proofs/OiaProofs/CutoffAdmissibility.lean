/-
  OiaProofs.CutoffAdmissibility
  Formal specification and soundness proofs for point-in-time forecast cutoff admissibility.
  Corresponds to `oia_forecast::is_admissible_at_cutoff` in `crates/oia-forecast/src/lib.rs`
  (ADR 0004, Issue #15, Requirements MARKET-04, MARKET-05, FORECAST-04).
-/

namespace OiaProofs

/-- Evaluation cutoff mode matching `oia_forecast::CutoffMode`. -/
inductive CutoffMode where
  | strictProspective : CutoffMode
  | retrospectiveReconstruction : CutoffMode
  deriving DecidableEq, Repr

/-- Admissibility predicate matching `oia_forecast::is_admissible_at_cutoff`. -/
def isAdmissibleAtCutoff
    (mode : CutoffMode)
    (cutoffMs : Int)
    (publicAvailabilityMs : Option Int)
    (ingestedAtMs : Int) : Bool :=
  match publicAvailabilityMs with
  | none => false
  | some availMs =>
    match mode with
    | CutoffMode.strictProspective =>
      decide (availMs <= cutoffMs) && decide (ingestedAtMs <= cutoffMs)
    | CutoffMode.retrospectiveReconstruction =>
      decide (availMs <= cutoffMs)

/-- Theorem 1: A record with unknown public availability (`none`) is never admissible
    under any cutoff mode (`docs/architecture.md` Section 3.1). -/
theorem unknown_availability_never_admissible
    (mode : CutoffMode) (cutoffMs ingestedAtMs : Int) :
    isAdmissibleAtCutoff mode cutoffMs none ingestedAtMs = false := by
  rfl

/-- Theorem 2: In `strictProspective` mode, admissibility implies both public availability
    and OIA ingestion occurred at or before `cutoffMs` (no future leakage). -/
theorem strict_admissible_no_leakage
    (cutoffMs availMs ingestedAtMs : Int)
    (h : isAdmissibleAtCutoff CutoffMode.strictProspective cutoffMs (some availMs) ingestedAtMs = true) :
    availMs <= cutoffMs ∧ ingestedAtMs <= cutoffMs := by
  simp [isAdmissibleAtCutoff] at h
  exact h

/-- Theorem 3: Strict prospective admissibility is stronger than (implies) retrospective
    reconstruction admissibility. -/
theorem strict_implies_retrospective
    (cutoffMs : Int) (publicAvail : Option Int) (ingestedAtMs : Int)
    (h : isAdmissibleAtCutoff CutoffMode.strictProspective cutoffMs publicAvail ingestedAtMs = true) :
    isAdmissibleAtCutoff CutoffMode.retrospectiveReconstruction cutoffMs publicAvail ingestedAtMs = true := by
  cases publicAvail with
  | none =>
    simp [isAdmissibleAtCutoff] at h
  | some availMs =>
    simp [isAdmissibleAtCutoff] at h ⊢
    exact h.left

end OiaProofs
