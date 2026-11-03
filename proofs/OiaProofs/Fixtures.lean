import OiaProofs.FrameSelection
import OiaProofs.CutoffAdmissibility
import OiaProofs.DependencyInvalidation

namespace OiaProofs

-- Model-only boundary checks; these do not claim a Rust frame/graph bridge.
example : selectFrameInWindow 1000 5000 3000 600 [] = .noFramesAvailable := by decide
example : selectFrameInWindow 1000 5000 3000 600 [1500, 2399, 3001, 4500] =
    .coverageGap := by decide
example : selectFrameInWindow 1000 3000 3000 2000 [1000, 3000] = .selected 1000 := by decide
example : selectFrameInWindow 1000 5000 3000 600 [2400, 2950, 2700, 3000, 3000, 3001] =
    .selected 3000 := by decide
-- Equal endpoints describe an empty mathematical window. Historical Rust rejected it.
example : selectFrameInWindow 1000 1000 1000 0 [1000] = .coverageGap := by decide
-- Exact integer subtraction must not silently become saturating i64 subtraction.
example : selectFrameInWindow (-9223372036854775808) 9223372036854775807
    9223372036854775807 9223372036854775807 [-9223372036854775808] = .coverageGap := by decide
example : stepInvalidation [(2, 1), (3, 2), (4, 1)] [1] = [1, 2, 4] := by decide
example : (invalidateClosure [(2, 1), (3, 2), (4, 1)] 3 [1]).eraseDups =
    [1, 2, 4, 3] := by decide

structure CutoffFixture where
  id : String
  mode : CutoffMode
  cutoff : Int
  knowledge : Option Int
  sources : List SourceTimes
  expected : CutoffDecision
  deriving DecidableEq, Repr

open CutoffMode CutoffDecision in
def cutoffFixtures : List CutoffFixture := [
  ⟨"strict-before", strictProspective, 1000, some 900, [⟨some 900, some 950⟩], admitted false⟩,
  ⟨"strict-equality", strictProspective, 1000, some 1000, [⟨some 1000, some 1000⟩], admitted false⟩,
  ⟨"strict-unknown-public", strictProspective, 1000, some 900, [⟨none, some 950⟩], rejected⟩,
  ⟨"strict-future-public", strictProspective, 1000, some 900, [⟨some 1001, some 950⟩], rejected⟩,
  ⟨"strict-unknown-ingestion", strictProspective, 1000, some 900, [⟨some 900, none⟩], rejected⟩,
  ⟨"strict-future-ingestion", strictProspective, 1000, some 900, [⟨some 900, some 1001⟩], rejected⟩,
  ⟨"strict-unknown-model", strictProspective, 1000, none, [⟨some 900, some 950⟩], rejected⟩,
  ⟨"strict-future-model", strictProspective, 1000, some 1001, [⟨some 900, some 950⟩], rejected⟩,
  ⟨"strict-empty", strictProspective, 1000, some 900, [], rejected⟩,
  ⟨"strict-bad-second", strictProspective, 1000, some 900,
    [⟨some 900, some 950⟩, ⟨some 900, none⟩], rejected⟩,
  ⟨"strict-two-valid", strictProspective, 1000, some 900,
    [⟨some 900, some 950⟩, ⟨some 1000, some 1000⟩], admitted false⟩,
  ⟨"retro-before", retrospectiveReconstruction, 1000, some 900,
    [⟨some 900, some 950⟩], admitted false⟩,
  ⟨"retro-equality", retrospectiveReconstruction, 1000, some 1000,
    [⟨some 1000, some 1000⟩], admitted false⟩,
  ⟨"retro-unknown-public", retrospectiveReconstruction, 1000, none,
    [⟨none, some 950⟩], rejected⟩,
  ⟨"retro-future-public", retrospectiveReconstruction, 1000, some 900,
    [⟨some 1001, none⟩], rejected⟩,
  ⟨"retro-unknown-ingestion", retrospectiveReconstruction, 1000, some 900,
    [⟨some 900, none⟩], admitted false⟩,
  ⟨"retro-future-ingestion", retrospectiveReconstruction, 1000, some 900,
    [⟨some 900, some 1001⟩], admitted false⟩,
  ⟨"retro-unknown-model", retrospectiveReconstruction, 1000, none,
    [⟨some 900, none⟩], admitted true⟩,
  ⟨"retro-future-model", retrospectiveReconstruction, 1000, some 1001,
    [⟨some 900, some 1001⟩], admitted true⟩,
  ⟨"retro-empty", retrospectiveReconstruction, 1000, none, [], rejected⟩,
  ⟨"retro-bad-second", retrospectiveReconstruction, 1000, some 900,
    [⟨some 900, none⟩, ⟨none, none⟩], rejected⟩,
  ⟨"strict-negative-time", strictProspective, -1000, some (-1001),
    [⟨some (-1001), some (-1000)⟩], admitted false⟩,
  ⟨"strict-min-time", strictProspective, -62135596800000, some (-62135596800000),
    [⟨some (-62135596800000), some (-62135596800000)⟩], admitted false⟩,
  ⟨"strict-near-max-time", strictProspective, 253402300799998, some 253402300799998,
    [⟨some 253402300799998, some 253402300799998⟩], admitted false⟩]

/-- Expected results are checked by the kernel, not trusted exporter output. -/
theorem cutoffFixtures_correct : ∀ f ∈ cutoffFixtures,
    cutoffDecision f.mode f.cutoff f.knowledge f.sources = f.expected := by
  decide

end OiaProofs
