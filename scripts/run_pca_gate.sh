#!/usr/bin/env bash
# PCA gate — Proof-Carrying Artifact schema v2.
#
# Builds a claim-carrying evidence bundle and proves `anubis verify` is:
#   - re-deriving + tamper-fail-closed: tampering the source OR the claim block is rejected;
#   - PORTABLE: it needs no Metal / prove path — verify passes even with the Metal reference set to
#     a wrong path and Metal disabled (only `prove`, not `verify`, needs Metal);
#   - HONEST: the v2 claim states tier="checked" and zk_present=false, carries no independent
#     taint-clean theorem, rejects retired/unknown claim fields, and never downgrades to hash-only.
#
# Rebuilds the binary first so a stale release can't mask a regression. The verdict is derived from
# observed exit codes / values, never defaulted.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
source "$ROOT/scripts/lib/gate_common.sh"
source "$ROOT/scripts/lib/pca_gate_harness.sh"

OUT_DIR="out/pca_gate"
if [[ "${1:-}" == "--out" && -n "${2:-}" ]]; then OUT_DIR="$2"; fi
if ! OUT_DIR="$(python3 - "$ROOT" "$OUT_DIR" <<'PY'
from pathlib import Path
import sys
root = Path(sys.argv[1]).resolve(strict=True)
base = (root / "out").resolve(strict=True)
candidate = Path(sys.argv[2])
if not candidate.is_absolute():
    candidate = root / candidate
candidate = candidate.resolve(strict=False)
try:
    candidate.relative_to(base)
except ValueError:
    raise SystemExit(1)
if candidate == base:
    raise SystemExit(1)
print(candidate)
PY
)"; then
  echo "PCA_GATE_SETUP_ERROR: --out must resolve below $ROOT/out" >&2
  exit 2
fi
if [[ -L "$OUT_DIR" ]]; then
  echo "PCA_GATE_SETUP_ERROR: output directory must not be a symlink" >&2
  exit 2
fi
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

if [[ -n "${ANUBIS_BIN:-}" ]]; then
  BIN="$ANUBIS_BIN"
  printf 'using supplied pinned binary: %s\n' "$BIN" >"$OUT_DIR/build.log"
else
  cargo build -p anubis --release >"$OUT_DIR/build.log" 2>&1
  build_rc=$?
  if [[ $build_rc -ne 0 ]]; then
    echo "PCA_GATE_SETUP_ERROR: cargo build -p anubis --release exited $build_rc" >&2
    exit 1
  fi
  BIN="$ROOT/target/release/anubis"
fi
if [[ ! -f "$BIN" || -L "$BIN" || ! -x "$BIN" ]]; then
  echo "PCA_GATE_SETUP_ERROR: selected binary must be regular executable: $BIN" >&2
  exit 1
fi
shasum -a 256 "$BIN" > "$OUT_DIR/anubis.sha256"

pass=0; total=0
step() { # step <got> <want> <name>
  total=$((total+1))
  if [[ "$1" == "$2" ]]; then pass=$((pass+1)); echo "PASS $3 (=$1)"; else echo "FAIL $3 (got $1, want $2)"; fi
}

prog="$OUT_DIR/prog.anb"
printf 'fn main() { let x = 6 * 7; print(x); }\n' > "$prog"
if ! BND="$(pca_generate_evidence_bundle "$BIN" "$prog" "$OUT_DIR/b1")"; then
  exit 1
fi
printf '%s\n' "$BND" > "$OUT_DIR/evidence_bundle.path"

# 1. Claim block present, with an honest verdict/tier and no silent ZK claim.
if [[ -f "$BND/pca.json" ]]; then step yes yes claim_block_emitted; else step no yes claim_block_emitted; fi
step "$(jq -r '.verdict'    "$BND/pca.json")" PASS    claim_verdict_pass
step "$(jq -r '.tier'       "$BND/pca.json")" checked claim_tier_checked
step "$(jq -r '.zk_present' "$BND/pca.json")" false   claim_zk_absent
step "$(jq -r '.pca_version' "$BND/pca.json")" 2      claim_schema_v2
step "$(jq 'has("taint_clean")' "$BND/pca.json")" false no_retired_taint_claim

rehash_bundle() {
  python3 "$ROOT/scripts/lib/bundle_manifest.py" rehash --bundle "$1"
}

# A rehashed PCA-v2 object with the retired claim must fail semantic parsing, not disappear as an
# ignored JSON extension.
cp -R "$BND" "$OUT_DIR/b_v2_retired" || { echo "PCA_GATE_SETUP_ERROR: copy v2 poison" >&2; exit 1; }
jq '.taint_clean = true' "$BND/pca.json" > "$OUT_DIR/b_v2_retired/pca.json" \
  || { echo "PCA_GATE_SETUP_ERROR: create v2 poison" >&2; exit 1; }
rehash_bundle "$OUT_DIR/b_v2_retired" || exit 1
"$BIN" verify "$OUT_DIR/b_v2_retired" >/dev/null 2>&1
step "$?" 1 rehashed_v2_retired_field_fails

# Version rollback is independently rejected even without the retired-field poison.
cp -R "$BND" "$OUT_DIR/b_v1_only" || { echo "PCA_GATE_SETUP_ERROR: copy v1-only poison" >&2; exit 1; }
jq '.pca_version = 1' "$BND/pca.json" > "$OUT_DIR/b_v1_only/pca.json" \
  || { echo "PCA_GATE_SETUP_ERROR: create v1-only poison" >&2; exit 1; }
rehash_bundle "$OUT_DIR/b_v1_only" || exit 1
"$BIN" verify "$OUT_DIR/b_v1_only" >/dev/null 2>&1
step "$?" 1 rehashed_v1_only_claim_fails

# An arbitrary unknown PCA-v2 field is rejected fail-closed independently of `taint_clean`.
cp -R "$BND" "$OUT_DIR/b_v2_unknown" || { echo "PCA_GATE_SETUP_ERROR: copy v2-unknown poison" >&2; exit 1; }
jq '.unknown_phase1_probe = true' "$BND/pca.json" > "$OUT_DIR/b_v2_unknown/pca.json" \
  || { echo "PCA_GATE_SETUP_ERROR: create v2-unknown poison" >&2; exit 1; }
rehash_bundle "$OUT_DIR/b_v2_unknown" || exit 1
"$BIN" verify "$OUT_DIR/b_v2_unknown" >/dev/null 2>&1
step "$?" 1 rehashed_v2_unknown_field_fails

# Removing the semantic object may leave an internally consistent integrity envelope, but it must
# not make `anubis verify` report PCA success.
cp -R "$BND" "$OUT_DIR/b_missing_pca" || { echo "PCA_GATE_SETUP_ERROR: copy missing-PCA poison" >&2; exit 1; }
rm "$OUT_DIR/b_missing_pca/pca.json" || { echo "PCA_GATE_SETUP_ERROR: remove PCA claim" >&2; exit 1; }
rehash_bundle "$OUT_DIR/b_missing_pca" || exit 1
"$BIN" verify "$OUT_DIR/b_missing_pca" >/dev/null 2>&1
step "$?" 1 rehashed_missing_pca_fails

# 2. A clean bundle verifies.
"$BIN" verify "$BND" >/dev/null 2>&1; step "$?" 0 verify_clean_passes

# 3. PORTABLE / COLD: verify with a wrong Metal reference and Metal disabled still passes — verify
#    re-derives the claim with no prove path, so it does not depend on the Desktop Metal tree.
env ANUBIS_RISC0_METAL_REFERENCE=/nonexistent/metal R0_DISABLE_METAL=1 "$BIN" verify "$BND" >/dev/null 2>&1
step "$?" 0 verify_cold_no_metal

# 4. Tampering the source fails closed.
cp -R "$BND" "$OUT_DIR/b_src" || { echo "PCA_GATE_SETUP_ERROR: copy source poison" >&2; exit 1; }
printf 'fn main() { print(999); }\n' > "$OUT_DIR/b_src/source.anubis" \
  || { echo "PCA_GATE_SETUP_ERROR: write source poison" >&2; exit 1; }
"$BIN" verify "$OUT_DIR/b_src" >/dev/null 2>&1; step "$?" 1 tamper_source_fails_closed

# 5. Tampering the claim block fails closed — a forged verdict does not survive re-derivation.
cp -R "$BND" "$OUT_DIR/b_claim" || { echo "PCA_GATE_SETUP_ERROR: copy claim poison" >&2; exit 1; }
jq '.verdict = "FAIL"' "$BND/pca.json" > "$OUT_DIR/b_claim/pca.json" \
  || { echo "PCA_GATE_SETUP_ERROR: create claim poison" >&2; exit 1; }
"$BIN" verify "$OUT_DIR/b_claim" >/dev/null 2>&1; step "$?" 1 tamper_claim_fails_closed

# 6. Ed25519 signing: keygen, sign, and attributable verify.
"$BIN" keygen --out "$OUT_DIR/keys" >/dev/null 2>&1
keygen_rc=$?
if [[ $keygen_rc -ne 0 || ! -s "$OUT_DIR/keys/verifying.key" || -L "$OUT_DIR/keys/verifying.key" \
   || ! -s "$OUT_DIR/keys/signing.key" || -L "$OUT_DIR/keys/signing.key" ]]; then
  echo "PCA_GATE_SETUP_ERROR: keygen failed or emitted invalid key files (rc=$keygen_rc)" >&2
  exit 1
fi
VK="$(cat "$OUT_DIR/keys/verifying.key")"
cp -R "$BND" "$OUT_DIR/b_signed" || { echo "PCA_GATE_SETUP_ERROR: copy signed bundle" >&2; exit 1; }
"$BIN" sign "$OUT_DIR/b_signed" --key "$OUT_DIR/keys/signing.key" >/dev/null 2>&1
sign_rc=$?
if [[ $sign_rc -ne 0 ]]; then
  echo "PCA_GATE_SETUP_ERROR: signing command exited $sign_rc" >&2
  exit 1
fi
[[ -f "$OUT_DIR/b_signed/pca.sig" ]] && step yes yes signature_written || step no yes signature_written
"$BIN" verify "$OUT_DIR/b_signed" >/dev/null 2>&1; step "$?" 0 verify_signed_passes
"$BIN" verify "$OUT_DIR/b_signed" --pubkey "$VK" >/dev/null 2>&1; step "$?" 0 verify_pubkey_match
"$BIN" verify "$OUT_DIR/b_signed" --pubkey deadbeef >/dev/null 2>&1; step "$?" 1 verify_pubkey_mismatch_fails

# 7. Tampering a SIGNED claim invalidates the signature -> fail closed.
cp -R "$OUT_DIR/b_signed" "$OUT_DIR/b_signed_tampered" \
  || { echo "PCA_GATE_SETUP_ERROR: copy signed poison" >&2; exit 1; }
jq '.verdict = "FAIL"' "$OUT_DIR/b_signed/pca.json" > "$OUT_DIR/b_signed_tampered/pca.json" \
  || { echo "PCA_GATE_SETUP_ERROR: create signed poison" >&2; exit 1; }
"$BIN" verify "$OUT_DIR/b_signed_tampered" >/dev/null 2>&1; step "$?" 1 tamper_signed_claim_fails

echo "Report: $OUT_DIR"
# Coverage ratchet (adversary R49): case total must not silently shrink.
set +e
assert_floor "pca_gate" "$total" "$ROOT/scripts/floors/pca_gate.count_floor"
_floor_rc=$?
set -e
if [[ $_floor_rc -ne 0 ]]; then
  echo "FLOOR: FAIL ($total cases; $GATE_FLOOR_ERROR)" >&2
  echo "Overall: FAIL ($pass/$total) coverage floor"
  exit 1
fi
if [[ $pass -eq $total ]]; then echo "Overall: PASS ($pass/$total)"; else echo "Overall: FAIL ($pass/$total)"; exit 1; fi
