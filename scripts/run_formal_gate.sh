#!/usr/bin/env bash
# run_formal_gate.sh — machine-check the Anubis mechanized-soundness formalization (Phase 5).
# Every theorem in formal/ must compile under Lean 4 core (no Mathlib). Fails closed: a `sorry`,
# an axiom, or a broken proof turns lake build red. This is the third-party-trust-elimination
# gate — what a consumer would otherwise have to take on faith is a machine-checked Lean proof.
set -euo pipefail
export PATH="$HOME/.elan/bin:$PATH"
cd "$(dirname "$0")/../formal"
echo "== lean toolchain =="; lean --version
echo "== lake build (checks every proof) =="
lake build
# Guard against vacuous proofs: no `sorry` / `admit` / added `axiom` may leak in.
if grep -rnE '\bsorry\b|\badmit\b|^axiom ' Anubis/ Anubis.lean 2>/dev/null | grep -v '^\s*--'; then
  echo "FORMAL_GATE: FAIL (a sorry/admit/axiom is present — the proof is not complete)"; exit 1
fi
echo "FORMAL_GATE: PASS (all theorems machine-checked, no sorry/admit/axiom)"
