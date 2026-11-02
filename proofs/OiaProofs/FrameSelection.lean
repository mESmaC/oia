/-
  OiaProofs.FrameSelection
  Formal specification and soundness proofs for bounded time-window source frame selection.
  Corresponds to `oia_time_geo::select_frame_in_window` in `crates/oia-time-geo/src/lib.rs`
  (ADR 0004, Issue #15).
-/

namespace OiaProofs

/-- Predicate checking whether a candidate timestamp `t` (in UTC ms) falls within the
    half-open query window `[windowStart, windowEnd)`, is not after `target`, and is
    within `maxGap` of `target`. -/
def isValidFrame (windowStart windowEnd target maxGap t : Int) : Bool :=
  (decide (windowStart <= t) && decide (t < windowEnd)) &&
  (decide (t <= target) && decide (target - t <= maxGap))

/-- Folds over `candidates` to select the maximum timestamp satisfying `isValidFrame`. -/
def selectLatestValid (windowStart windowEnd target maxGap : Int) (candidates : List Int) : Option Int :=
  match candidates with
  | [] => none
  | t :: rest =>
    let tailBest := selectLatestValid windowStart windowEnd target maxGap rest
    if isValidFrame windowStart windowEnd target maxGap t then
      match tailBest with
      | none => some t
      | some b => if decide (b >= t) then some b else some t
    else
      tailBest

/-- Discrete outcome of bounded time-window frame selection, distinguishing an empty
    source catalog from a coverage gap. -/
inductive FrameSelectionOutcome where
  | noFramesAvailable : FrameSelectionOutcome
  | coverageGap : FrameSelectionOutcome
  | selected (t : Int) : FrameSelectionOutcome
  deriving DecidableEq, Repr

/-- Top-level frame selection specification corresponding to `oia_time_geo::select_frame_in_window`. -/
def selectFrameInWindow
    (windowStart windowEnd target maxGap : Int)
    (candidates : List Int) : FrameSelectionOutcome :=
  match candidates with
  | [] => FrameSelectionOutcome.noFramesAvailable
  | _ :: _ =>
    match selectLatestValid windowStart windowEnd target maxGap candidates with
    | some t => FrameSelectionOutcome.selected t
    | none => FrameSelectionOutcome.coverageGap

/-- Theorem 1: An empty candidate catalog always yields `noFramesAvailable` (never `coverageGap`
    or a fabricated timestamp). -/
theorem selectFrameInWindow_empty (windowStart windowEnd target maxGap : Int) :
    selectFrameInWindow windowStart windowEnd target maxGap [] =
      FrameSelectionOutcome.noFramesAvailable := by
  rfl

/-- Theorem 2 (Soundness): Any timestamp `t` returned by `selectLatestValid` satisfies
    `isValidFrame windowStart windowEnd target maxGap t = true`. -/
theorem selectLatestValid_sound
    (windowStart windowEnd target maxGap : Int)
    (candidates : List Int)
    (t : Int)
    (h : selectLatestValid windowStart windowEnd target maxGap candidates = some t) :
    isValidFrame windowStart windowEnd target maxGap t = true := by
  induction candidates generalizing t with
  | nil =>
    simp [selectLatestValid] at h
  | cons head tail ih =>
    simp only [selectLatestValid] at h
    split at h
    · rename_i hHeadValid
      split at h
      · injection h with hEq
        subst hEq
        exact hHeadValid
      · rename_i b hTail
        split at h
        · injection h with hEq
          subst hEq
          exact ih b hTail
        · injection h with hEq
          subst hEq
          exact hHeadValid
    · exact ih t h

/-- Theorem 3 (Provenance membership): Any timestamp `t` returned by `selectLatestValid`
    is an element of the input `candidates` list (never invented or interpolated). -/
theorem selectLatestValid_mem
    (windowStart windowEnd target maxGap : Int)
    (candidates : List Int)
    (t : Int)
    (h : selectLatestValid windowStart windowEnd target maxGap candidates = some t) :
    t ∈ candidates := by
  induction candidates generalizing t with
  | nil =>
    simp [selectLatestValid] at h
  | cons head tail ih =>
    simp only [selectLatestValid] at h
    split at h
    · split at h
      · injection h with hEq
        subst hEq
        exact List.Mem.head tail
      · rename_i b hTail
        split at h
        · injection h with hEq
          subst hEq
          exact List.Mem.tail head (ih b hTail)
        · injection h with hEq
          subst hEq
          exact List.Mem.head tail
    · exact List.Mem.tail head (ih t h)

/-- Theorem 4 (End-to-end outcome soundness): Whenever `selectFrameInWindow` returns
    `FrameSelectionOutcome.selected t`, `t` satisfies all window and staleness bounds
    and belongs to `candidates`. -/
theorem selectFrameInWindow_selected_sound
    (windowStart windowEnd target maxGap : Int)
    (candidates : List Int)
    (t : Int)
    (h : selectFrameInWindow windowStart windowEnd target maxGap candidates =
          FrameSelectionOutcome.selected t) :
    isValidFrame windowStart windowEnd target maxGap t = true ∧ t ∈ candidates := by
  cases candidates with
  | nil =>
    simp [selectFrameInWindow] at h
  | cons head tail =>
    simp only [selectFrameInWindow] at h
    split at h
    · rename_i sel hSel
      injection h with hEq
      subst hEq
      exact ⟨
        selectLatestValid_sound windowStart windowEnd target maxGap (head :: tail) sel hSel,
        selectLatestValid_mem windowStart windowEnd target maxGap (head :: tail) sel hSel
      ⟩
    · contradiction

end OiaProofs
