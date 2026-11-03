import OiaProofs

open OiaProofs

def optionalTime : Option Int → String
  | none => "?"
  | some t => toString t

def fixtureLine (f : CutoffFixture) : String :=
  let mode := match f.mode with
    | .strictProspective => "strict"
    | .retrospectiveReconstruction => "retro"
  let sources := if f.sources.isEmpty then "-" else
    String.intercalate ";" (f.sources.map fun s =>
      s!"{optionalTime s.publicAvailability}/{optionalTime s.ingestedAt}")
  let result := match f.expected with
    | .rejected => "reject"
    | .admitted false => "accept"
    | .admitted true => "disclosed"
  String.intercalate "\t" [f.id, mode, toString f.cutoff, optionalTime f.knowledge, sources, result]

def main (args : List String) : IO UInt32 := do
  let path := "fixtures/cutoff.tsv"
  let text := "# Generated from kernel-checked OiaProofs.cutoffFixtures; see proofs/README.md.\n" ++
    String.intercalate "\n" (cutoffFixtures.map fixtureLine) ++ "\n"
  match args with
  | ["--write"] =>
    IO.FS.createDirAll "fixtures"
    IO.FS.writeFile path text
    return 0
  | ["--check"] =>
    if (← IO.FS.readFile path) != text then
      (← IO.getStderr).putStrLn "Cutoff fixture differs from Lean; regenerate with --write and review."
      return 1
    IO.println "Lean/Rust cutoff fixture bytes match."
    return 0
  | _ =>
    (← IO.getStderr).putStrLn "Usage: lake env lean --run ExportFixtures.lean --check|--write"
    return 1
