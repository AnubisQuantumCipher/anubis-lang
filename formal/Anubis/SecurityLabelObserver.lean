/-
  Anubis — Lean 4 observer for the SecurityLabel finite abstraction (Phase 8, Slice 1).

  Prints one canonical TSV row per (op, args) tuple to the file path given on the
  command line. The rows come from `Anubis.SecurityLabel.observationRows`, evaluated
  by the Lean kernel over the same abstract corpus the mechanized module defines
  and proves theorems about. No file input; no dependency on any Rust artifact.

  Correspondence gate: `scripts/run_security_label_correspondence_gate.sh` runs this
  and the Rust observer independently, then byte-compares their outputs. A divergence
  is a real, mechanically detected Rust↔Lean disagreement over the declared abstraction.

  Usage:
    lake env lean --run formal/Anubis/SecurityLabelObserver.lean <output.tsv>

  or as a lake_exe:
    lake exe security_label_observer <output.tsv>
-/
import Anubis.SecurityLabel

namespace Anubis.SecurityLabelObserver

open Anubis.SecurityLabel

/-- Format the full corpus as one file body: rows joined by `\n`, terminated by `\n`.
    This exact byte layout matches the Rust observer's `writeln!` per row. -/
def renderCorpus : String :=
  String.intercalate "\n" observationRows ++ "\n"

def usage : String :=
  "usage: lake exe security_label_observer <output.tsv>"

def main (args : List String) : IO UInt32 := do
  match args with
  | [path] =>
    IO.FS.writeFile path renderCorpus
    pure 0
  | _ =>
    IO.eprintln usage
    pure 2

end Anubis.SecurityLabelObserver

/-- Lean 4 script entry point. `lake exe` wires this automatically for a `lean_exe`
    target; `lake env lean --run` also picks it up. -/
def main (args : List String) : IO UInt32 :=
  Anubis.SecurityLabelObserver.main args
