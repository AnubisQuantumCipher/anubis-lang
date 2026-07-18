#!/usr/bin/env bash
# run_formal_gate.sh — machine-check the Anubis mechanized-soundness formalization (Phase 5).
# Every theorem in formal/ must compile under Lean 4 core (no Mathlib). Fails closed: a `sorry`,
# an `admit`, an added `axiom`, or `native_decide` (an unaudited kernel-reduction trust hole) turns
# the gate red. This is the third-party-trust-elimination gate — what a consumer would otherwise
# take on faith is a machine-checked Lean proof.
set -euo pipefail
export PATH="$HOME/.elan/bin:$PATH"
cd "$(dirname "$0")/../formal"
echo "== lean toolchain =="; lean --version
echo "== lake build (checks every proof) =="
lake build

# No-stopgap scan. A real `sorry`/`admit` makes Lean emit a warning but STILL exits 0, so `lake build`
# succeeding is not sufficient — scan the source. Strip Lean comments FIRST (both `--` line comments
# and `/- ... -/` / `/-- ... -/` block comments) so a PROSE mention of these words in a doc comment
# (e.g. a file documenting that it is "stopgap-free") does not trip the gate. Only genuine CODE tokens
# remain after stripping.
STRIPPED="$(cat Anubis.lean Anubis/*.lean | perl -0777 -pe 's{/-.*?-/}{}gs; s{--[^\n]*}{}g')"
if printf '%s' "$STRIPPED" | grep -qE '\bsorry\b|\badmit\b|\bnative_decide\b|(^|[^A-Za-z_])axiom[[:space:]]'; then
  echo "FORMAL_GATE: FAIL — a sorry/admit/axiom/native_decide is present in proof CODE (not just a comment):"
  printf '%s' "$STRIPPED" | grep -nE '\bsorry\b|\badmit\b|\bnative_decide\b|(^|[^A-Za-z_])axiom[[:space:]]' | head
  exit 1
fi
echo "FORMAL_GATE: PASS (all theorems machine-checked; no sorry/admit/axiom/native_decide in code)"
