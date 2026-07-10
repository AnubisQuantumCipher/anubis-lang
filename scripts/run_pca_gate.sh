#!/usr/bin/env bash
# PCA gate — Proof-Carrying Artifact v0.1.
#
# Builds a claim-carrying evidence bundle and proves `anubis verify` is:
#   - re-deriving + tamper-fail-closed: tampering the source OR the claim block is rejected;
#   - PORTABLE: it needs no Metal / prove path — verify passes even with the Metal reference set to
#     a wrong path and Metal disabled (only `prove`, not `verify`, needs Metal);
#   - HONEST: the v0 claim states tier="checked" and zk_present=false — no silent overclaim.
#
# Rebuilds the binary first so a stale release can't mask a regression. The verdict is derived from
# observed exit codes / values, never defaulted.
set -uo pipefail
cd "$(dirname "$0")/.."

OUT_DIR="out/pca_gate"
if [[ "${1:-}" == "--out" && -n "${2:-}" ]]; then OUT_DIR="$2"; fi
rm -rf "$OUT_DIR"; mkdir -p "$OUT_DIR"

# Pinned, fresh binary — avoids the stale-release trap.
cargo build -p anubis --release >/dev/null 2>&1 || { echo "FAIL: cargo build -p anubis --release"; exit 1; }
BIN="./target/release/anubis"

pass=0; total=0
step() { # step <got> <want> <name>
  total=$((total+1))
  if [[ "$1" == "$2" ]]; then pass=$((pass+1)); echo "PASS $3 (=$1)"; else echo "FAIL $3 (got $1, want $2)"; fi
}

prog="$OUT_DIR/prog.anb"
printf 'fn main() { let x = 6 * 7; print(x); }\n' > "$prog"
"$BIN" check "$prog" --evidence --out "$OUT_DIR/b1" >/dev/null 2>&1
BND=$(find "$OUT_DIR/b1" -maxdepth 1 -type d -name 'evidence-*' | head -1)
[[ -n "$BND" ]] || { echo "FAIL: no evidence bundle produced"; exit 1; }

# 1. Claim block present, with an honest verdict/tier and no silent ZK claim.
if [[ -f "$BND/pca.json" ]]; then step yes yes claim_block_emitted; else step no yes claim_block_emitted; fi
step "$(jq -r '.verdict'    "$BND/pca.json")" PASS    claim_verdict_pass
step "$(jq -r '.tier'       "$BND/pca.json")" checked claim_tier_checked
step "$(jq -r '.zk_present' "$BND/pca.json")" false   claim_zk_absent

# 2. A clean bundle verifies.
"$BIN" verify "$BND" >/dev/null 2>&1; step "$?" 0 verify_clean_passes

# 3. PORTABLE / COLD: verify with a wrong Metal reference and Metal disabled still passes — verify
#    re-derives the claim with no prove path, so it does not depend on the Desktop Metal tree.
env ANUBIS_RISC0_METAL_REFERENCE=/nonexistent/metal R0_DISABLE_METAL=1 "$BIN" verify "$BND" >/dev/null 2>&1
step "$?" 0 verify_cold_no_metal

# 4. Tampering the source fails closed.
cp -r "$BND" "$OUT_DIR/b_src"
printf 'fn main() { print(999); }\n' > "$OUT_DIR/b_src/source.anubis"
"$BIN" verify "$OUT_DIR/b_src" >/dev/null 2>&1; step "$?" 1 tamper_source_fails_closed

# 5. Tampering the claim block fails closed — a forged verdict does not survive re-derivation.
cp -r "$BND" "$OUT_DIR/b_claim"
jq '.verdict = "FAIL"' "$BND/pca.json" > "$OUT_DIR/b_claim/pca.json"
"$BIN" verify "$OUT_DIR/b_claim" >/dev/null 2>&1; step "$?" 1 tamper_claim_fails_closed

echo "Report: $OUT_DIR"
if [[ $pass -eq $total ]]; then echo "Overall: PASS ($pass/$total)"; else echo "Overall: FAIL ($pass/$total)"; exit 1; fi
