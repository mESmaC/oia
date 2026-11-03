import Lean
import OiaProofs

/- Audit every declaration in the imported project modules, including helpers,
   private declarations, unused axioms, and declarations outside our namespace.
   Lean's standard logical axioms are the explicit trusted base; sorryAx,
   native_decide's axiom and project axioms are not permitted. -/
open Lean in
run_cmd do
  let env ← getEnv
  -- Every source module must participate in the root's import graph. Otherwise
  -- a new proof file could compile separately without being audited here.
  for path in (← System.FilePath.walkDir "OiaProofs") do
    if path.extension == some "lean" then
      let moduleName := (path.withExtension "").components.foldl Name.str .anonymous
      unless env.header.moduleNames.contains moduleName do
        throwError "Project module is not imported by OiaProofs.lean: {moduleName}"
  let allowed := #[``propext, ``Classical.choice, ``Quot.sound]
  let mut checked : Nat := 0
  for (name, info) in env.constants.toList do
    let some idx := env.getModuleIdxFor? name | continue
    let some moduleName := env.header.moduleNames[idx.toNat]? | continue
    unless (`OiaProofs).isPrefixOf moduleName do continue
    if info.isAxiom then
      throwError "Project axiom is forbidden: {name}"
    for axiomName in (← collectAxioms name) do
      unless allowed.contains axiomName do
        throwError "Unapproved axiom dependency: {name} uses {axiomName}"
    checked := checked + 1
  if checked == 0 then throwError "Proof audit found no project declarations"
  logInfo m!"Audited {checked} project declarations; only standard Lean logical axioms permitted."
