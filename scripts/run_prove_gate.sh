#!/usr/bin/env bash
# Prove gate — the ZK-receipt-binding thesis made executable.
#
# Proves that a Proof-Carrying Artifact carrying a REAL RISC Zero receipt:
#   - binds the receipt into the claim block (zk_present=true + ImageID + receipt/journal digests);
#   - verifies COLD — in a context that never ran the prover (a fresh dir with only the bundle, no
#     methods crate, no vendored circuit), the receipt is cryptographically re-verified against its
#     ImageID (re-derive, not re-trust);
#   - fails closed under tampering — a corrupted receipt or a swapped ImageID is rejected nonzero;
# and that a bundle WITHOUT a receipt honestly reports zk_present=false (no silent overclaim).
#
# Uses the committed real-receipt fixture tests/fixtures/zk_prove_bundle (a genuine receipt produced
# by the Metal prove path, see A0). Rebuilds the binary first so a stale release can't mask a
# regression. The verdict is derived from observed exit codes / values, never defaulted.
set -uo pipefail
cd "$(dirname "$0")/.."

OUT_DIR="out/prove_gate"
if [[ "${1:-}" == "--out" && -n "${2:-}" ]]; then OUT_DIR="$2"; fi
rm -rf "$OUT_DIR"; mkdir -p "$OUT_DIR"

FIXTURE="tests/fixtures/zk_prove_bundle"
[[ -f "$FIXTURE/backend/risc0/receipt.bin" ]] || { echo "FAIL: missing committed receipt fixture $FIXTURE"; exit 1; }

cargo build -p anubis --release >/dev/null 2>&1 || { echo "FAIL: cargo build -p anubis --release"; exit 1; }
BIN="$(pwd)/target/release/anubis"

pass=0; total=0
step() { # step <got> <want> <name>
  total=$((total+1))
  if [[ "$1" == "$2" ]]; then pass=$((pass+1)); echo "PASS $3 (=$1)"; else echo "FAIL $3 (got $1, want $2)"; fi
}

# --- 1. The fixture's claim block binds the receipt (A1). ---
step "$(jq -r '.zk_present'                      "$FIXTURE/pca.json")" true claim_zk_present
step "$(jq -r '(.zk_image_id|length)>0'          "$FIXTURE/pca.json")" true claim_has_image_id
step "$(jq -r '(.zk_receipt_sha256|length)==64'  "$FIXTURE/pca.json")" true claim_has_receipt_digest
step "$(jq -r '(.zk_journal_sha256|length)==64'  "$FIXTURE/pca.json")" true claim_has_journal_digest

# --- 2. COLD verify: copy the bundle to a fresh dir outside the repo (no prover state) and verify. ---
COLD="$(mktemp -d "${TMPDIR:-/tmp}/anubis-cold-XXXXXX")"
cp -R "$FIXTURE"/. "$COLD"/
COLD_OUT="$( (cd "${TMPDIR:-/tmp}" && "$BIN" verify "$COLD") 2>&1 )"
step "$?" 0 cold_verify_exit_zero
if grep -q "zk: receipt re-verified against ImageID" <<<"$COLD_OUT"; then step yes yes cold_zk_reverified; else step no yes cold_zk_reverified; fi
if grep -q "bundle valid: true" <<<"$COLD_OUT"; then step yes yes cold_bundle_valid; else step no yes cold_bundle_valid; fi

# --- 3. Tamper: corrupt the receipt bytes in the cold copy → verify must fail closed. ---
CORRUPT="$(mktemp -d "${TMPDIR:-/tmp}/anubis-corrupt-XXXXXX")"
cp -R "$FIXTURE"/. "$CORRUPT"/
# Flip bytes deep in the receipt (both the tree and flat copies are hash-bound, so tamper both).
python3 - "$CORRUPT" <<'PY'
import sys, os
d = sys.argv[1]
for rel in ["backend/risc0/receipt.bin", "risc0_receipt.bin"]:
    p = os.path.join(d, rel)
    if os.path.exists(p):
        b = bytearray(open(p, "rb").read())
        for i in (5000, 50000, 150000):
            if i < len(b): b[i] ^= 0xFF
        open(p, "wb").write(b)
PY
( (cd "${TMPDIR:-/tmp}" && "$BIN" verify "$CORRUPT") >/dev/null 2>&1 ); step "$?" 1 tampered_receipt_fails_closed

# --- 4. Tamper: swap the ImageID → verify must fail closed. ---
WRONGID="$(mktemp -d "${TMPDIR:-/tmp}/anubis-wrongid-XXXXXX")"
cp -R "$FIXTURE"/. "$WRONGID"/
printf '1 2 3 4 5 6 7 8\n' > "$WRONGID/backend/risc0/image_id.txt"
( (cd "${TMPDIR:-/tmp}" && "$BIN" verify "$WRONGID") >/dev/null 2>&1 ); step "$?" 1 wrong_image_id_fails_closed

# --- 5. A bundle WITHOUT a receipt honestly reports zk_present=false, and still verifies. ---
prog="$OUT_DIR/noreceipt.anb"
printf 'fn main() { let x = 6 * 7; print(x); }\n' > "$prog"
"$BIN" check "$prog" --evidence --out "$OUT_DIR/nb" >/dev/null 2>&1
NB=$(find "$OUT_DIR/nb" -maxdepth 1 -type d -name 'evidence-*' | head -1)
step "$(jq -r '.zk_present' "$NB/pca.json")" false no_receipt_zk_absent
( "$BIN" verify "$NB" >/dev/null 2>&1 ); step "$?" 0 no_receipt_still_verifies

rm -rf "$COLD" "$CORRUPT" "$WRONGID"
echo "Report: $OUT_DIR"
echo "Overall: $([[ $pass -eq $total ]] && echo PASS || echo FAIL) ($pass/$total)"
[[ $pass -eq $total ]] || exit 1
