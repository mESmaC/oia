/-
  OiaProofs.Fixtures
  Compile-time verified correspondence fixtures shared with
  `crates/oia-time-geo/tests/lean_correspondence.rs`.
-/

import OiaProofs.FrameSelection
import OiaProofs.CutoffAdmissibility
import OiaProofs.DependencyInvalidation

namespace OiaProofs

-- Fixture 1: Empty catalog returns `noFramesAvailable`.
#guard selectFrameInWindow 1000 5000 3000 600 [] == FrameSelectionOutcome.noFramesAvailable

-- Fixture 2: Candidates exist, but all are either before `target - maxGap` (2400) or after `target` (3000).
#guard selectFrameInWindow 1000 5000 3000 600 [1500, 2399, 3001, 4500] == FrameSelectionOutcome.coverageGap

-- Fixture 3: Exact boundary inclusion (`t == windowStart`, `t == target - maxGap`, `t == target`)
-- and exact upper-bound exclusion (`t == windowEnd`).
#guard selectFrameInWindow 1000 3000 3000 2000 [1000, 3000] == FrameSelectionOutcome.selected 1000
#guard selectFrameInWindow 1000 5000 3000 600 [2400, 2950, 2700, 3000, 3001] == FrameSelectionOutcome.selected 3000

-- Fixture 4: Cutoff admissibility vectors.
#guard isAdmissibleAtCutoff CutoffMode.strictProspective 1000 none 500 == false
#guard isAdmissibleAtCutoff CutoffMode.strictProspective 1000 (some 900) 1000 == true
#guard isAdmissibleAtCutoff CutoffMode.strictProspective 1000 (some 900) 1001 == false
#guard isAdmissibleAtCutoff CutoffMode.retrospectiveReconstruction 1000 (some 900) 1500 == true
#guard isAdmissibleAtCutoff CutoffMode.retrospectiveReconstruction 1000 (some 1001) 500 == false

-- Fixture 5: Dependency invalidation single step and multi-step transitive closure.
#guard stepInvalidation [(2, 1), (3, 2), (4, 1)] [1] == [1, 2, 4]
#guard (let closed := invalidateClosure [(2, 1), (3, 2), (4, 1)] 3 [1];
        closed.contains 1 && closed.contains 2 && closed.contains 3 && closed.contains 4) == true

end OiaProofs
