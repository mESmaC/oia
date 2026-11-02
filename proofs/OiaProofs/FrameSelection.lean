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

/-- Theorem 5 (Completeness): If `selectLatestValid` returns `none`, then no element
    in `candidates` satisfies `isValidFrame`. -/
theorem selectLatestValid_complete
    (windowStart windowEnd target maxGap : Int)
    (candidates : List Int)
    (hNone : selectLatestValid windowStart windowEnd target maxGap candidates = none)
    (c : Int)
    (hMem : c ∈ candidates) :
    isValidFrame windowStart windowEnd target maxGap c = false := by
  induction candidates with
  | nil =>
    cases hMem
  | cons head tail ih =>
    simp only [selectLatestValid] at hNone
    split at hNone
    · split at hNone
      · contradiction
      · split at hNone <;> contradiction
    · rename_i hHeadInvalid
      have hHeadFalse : isValidFrame windowStart windowEnd target maxGap head = false :=
        Bool.eq_false_iff.mpr hHeadInvalid
      cases hMem with
      | head _ => exact hHeadFalse
      | tail _ hTailMem => exact ih hNone hTailMem

/-- Theorem 6 (Optimality / Latest-Selection): If `selectLatestValid` returns `some t`,
    then for every valid candidate `c ∈ candidates`, `c <= t`. -/
theorem selectLatestValid_optimal
    (windowStart windowEnd target maxGap : Int)
    (candidates : List Int)
    (t : Int)
    (hSome : selectLatestValid windowStart windowEnd target maxGap candidates = some t)
    (c : Int)
    (hMem : c ∈ candidates)
    (hValid : isValidFrame windowStart windowEnd target maxGap c = true) :
    c <= t := by
  induction candidates generalizing t with
  | nil =>
    simp [selectLatestValid] at hSome
  | cons head tail ih =>
    simp only [selectLatestValid] at hSome
    split at hSome
    · rename_i hHeadValid
      split at hSome
      · rename_i hTailNone
        injection hSome with hEq
        subst hEq
        cases hMem with
        | head _ => exact Int.le_refl c
        | tail _ hTailMem =>
          have hFalse := selectLatestValid_complete windowStart windowEnd target maxGap tail hTailNone c hTailMem
          rw [hValid] at hFalse
          contradiction
      · rename_i b hTailSome
        split at hSome
        · rename_i hBGeHead
          injection hSome with hEq
          subst hEq
          have hHeadLeB : head <= b := of_decide_eq_true hBGeHead
          cases hMem with
          | head _ => exact hHeadLeB
          | tail _ hTailMem => exact ih b hTailSome hTailMem
        · rename_i hNotBGeHead
          injection hSome with hEq
          subst hEq
          have hBLeHead : b <= head := by
            have hLt : ¬(b >= head) := of_decide_eq_false (Bool.eq_false_iff.mpr hNotBGeHead)
            omega
          cases hMem with
          | head _ => exact Int.le_refl c
          | tail _ hTailMem =>
            have hCLeB : c <= b := ih b hTailSome hTailMem
            exact Int.le_trans hCLeB hBLeHead
    · rename_i hHeadInvalid
      cases hMem with
      | head _ =>
        exfalso
        exact hHeadInvalid hValid
      | tail _ hTailMem =>
        exact ih t hSome hTailMem

/-- Theorem 7 (Coverage-Gap Characterization): `selectFrameInWindow` returns `coverageGap`
    if and only if `candidates` is non-empty and every candidate fails `isValidFrame`. -/
theorem selectFrameInWindow_coverageGap_iff
    (windowStart windowEnd target maxGap : Int)
    (candidates : List Int) :
    selectFrameInWindow windowStart windowEnd target maxGap candidates =
      FrameSelectionOutcome.coverageGap ↔
      (candidates ≠ [] ∧ ∀ c ∈ candidates, isValidFrame windowStart windowEnd target maxGap c = false) := by
  constructor
  · intro hGap
    cases candidates with
    | nil =>
      simp [selectFrameInWindow] at hGap
    | cons head tail =>
      simp only [selectFrameInWindow] at hGap
      split at hGap
      · contradiction
      · rename_i hNone
        exact ⟨
          List.cons_ne_nil head tail,
          fun c hMem => selectLatestValid_complete windowStart windowEnd target maxGap (head :: tail) hNone c hMem
        ⟩
  · intro ⟨hNonEmpty, hAllInvalid⟩
    cases candidates with
    | nil => contradiction
    | cons head tail =>
      simp only [selectFrameInWindow]
      split
      · rename_i t hSome
        have hMem := selectLatestValid_mem windowStart windowEnd target maxGap (head :: tail) t hSome
        have hValid := selectLatestValid_sound windowStart windowEnd target maxGap (head :: tail) t hSome
        have hInvalid := hAllInvalid t hMem
        rw [hValid] at hInvalid
        contradiction
      · rfl

end OiaProofs
