#!/usr/bin/env bash
# PCA gate — Proof-Carrying Artifact v0.
#
# Builds an evidence bundle carrying a claim block (pca.json), verifies it, and confirms that
# `anubis verify` fails CLOSED when the source or the claim block is tampered. The verdict is
# derived from observed exit codes, never defaulted, so a broken verifier fails the gate.
set -uo pipefail
cd "$(dirname "$0")/.."

OUT_DIR="out/pca_gate"
if [[ "${1:-}" == "--out" && -n "${2:-}" ]]; then OUT_DIR="$2"; fi
rm -rf "$OUT_DIR"; mkdir -p "$OUT_DIR"

BIN="./target/release/anubis"
[[ -x "$BIN" ]] || BIN="./target/debug/anubis"
[[ -x "$BIN" ]] || { echo "FAIL: no anubis binary — run 'cargo build' first"; exit 1; }

pass=0; total=0
step() { # step <got> <want> <name>
  total=$((total+1))
  if [[ "$1" == "$2" ]]; then pass=$((pass+1)); echo "PASS $3 (=$1)"
  else echo "FAIL $3 (got $1, want $2)"; fi
}

prog="$OUT_DIR/prog.anb"
printf 'fn main() { let x = 6 * 7; print(x); }\n' > "$prog"
"$BIN" check "$prog" --evidence --out "$OUT_DIR/b1" >/dev/null 2>&1
BND=$(find "$OUT_DIR/b1" -maxdepth 1 -type d -name 'evidence-*' | head -1)
[[ -n "$BND" ]] || { echo "FAIL: no evidence bundle produced"; exit 1; }

# 1. The bundle carries a claim block, and it names a PASS verdict for this program.
if [[ -f "$BND/pca.json" ]]; then step "yes" "yes" "claim_block_emitted"; else step "no" "yes" "claim_block_emitted"; fi
verdict=$(jq -r '.verdict' "$BND/pca.json" 2>/dev/null || echo "?")
step "$verdict" "PASS" "claim_block_verdict"

# 2. A clean bundle verifies (exit 0).
"$BIN" verify "$BND" >/dev/null 2>&1; step "$?" "0" "verify_clean_passes"

# 3. Tampering the source fails closed (exit 1).
cp -r "$BND" "$OUT_DIR/b_src"
printf 'fn main() { print(999); }\n' > "$OUT_DIR/b_src/source.anubis"
"$BIN" verify "$OUT_DIR/b_src" >/dev/null 2>&1; step "$?" "1" "tamper_source_fails_closed"

# 4. Tampering the claim block fails closed (exit 1) — a forged verdict does not survive verify.
cp -r "$BND" "$OUT_DIR/b_claim"
jq '.verdict = "FAIL"' "$BND/pca.json" > "$OUT_DIR/b_claim/pca.json"
"$BIN" verify "$OUT_DIR/b_claim" >/dev/null 2>&1; step "$?" "1" "tamper_claim_fails_closed"

echo "Report: $OUT_DIR"
if [[ $pass -eq $total ]]; then echo "Overall: PASS ($pass/$total)"; else echo "Overall: FAIL ($pass/$total)"; exit 1; fi
