#!/usr/bin/env bash
# Proof-binding gate: proves that `anubis prove --backend risc0` proves the ACTUAL
# input program (not a fixed circuit). It compiles the Anubis program into the RISC0
# guest, derives the ImageID from that guest's ELF, generates a real receipt, and
# checks that the committed journal equals the program's real result.
#
# HONESTY CONTRACT: a program whose expected result is R passes only if
#   verify_status == "passed" AND fresh_receipt_generated AND !dev_mode AND !mock_prover
#   AND guest_binding == "anubis-program" AND journal(u32 LE) == R.
# Anything else FAILs. Requires the metal-hybrid-prover reference for the patched circuit.
# set -e so missing python / unhandled failures cannot leave overall=PASS hollow.
set -euo pipefail

REF="${ANUBIS_RISC0_METAL_REFERENCE:-/Users/sicarii/Desktop/metal-hybrid-prover}"
OUT="out/proof_binding"
while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --metal-reference) REF="$2"; shift 2 ;;
    *) shift ;;
  esac
done
REPO="$(cd "$(dirname "$0")/.." && pwd)"; cd "$REPO"
BIN="${ANUBIS_BIN:-target/release/anubis}"
[ -x "$BIN" ] || { echo "FAIL: no release binary (cargo build --release -p anubis)"; exit 1; }
[ -d "$REF/vendor/risc0-circuit-rv32im" ] || { echo "FAIL: RISC0 reference not at $REF"; exit 1; }
command -v python3 >/dev/null 2>&1 || { echo "FAIL: python3 required for journal/ImageID oracles"; exit 127; }
mkdir -p "$OUT"

# name:expected — small computations so proving stays fast
CASES="proof_factorial:120 proof_fib:55"
overall=PASS
for case in $CASES; do
  name="${case%%:*}"; want="${case##*:}"; d="$OUT/$name"
  echo "── proving $name (expect journal=$want) ──"
  if ! "$BIN" prove "examples/$name.anb" --backend risc0 --lane cpu \
        --metal-reference "$REF" --out "$d" > "$OUT/$name.log" 2>&1; then
    echo "  FAIL: prove exited nonzero"; overall=FAIL; continue
  fi
  verdict=$(python3 - "$d" "$want" <<'PY'
import json, struct, sys, pathlib
d=pathlib.Path(sys.argv[1])/"backend"/"risc0"; want=int(sys.argv[2])
try:
    m=json.load(open(d/"risc0_metadata.json"))
    jb=(d/"journal.bin").read_bytes()
    j=struct.unpack("<I", jb[:4])[0]
    ok=(m.get("verify_status")=="passed" and m.get("fresh_receipt_generated") and
        not m.get("dev_mode") and not m.get("mock_prover") and
        m.get("guest_binding")=="anubis-program" and j==want and
        not m.get("image_id_is_placeholder"))
    print(f"{'PASS' if ok else 'FAIL'} journal={j} verify={m.get('verify_status')} binding={m.get('guest_binding')} imageid={m.get('image_id','')[:22]}")
except Exception as e:
    print(f"FAIL error={e}")
PY
)
  echo "  $verdict"
  case "$verdict" in PASS*) : ;; *) overall=FAIL ;; esac
done

# The two programs must produce DIFFERENT ImageIDs (proof is program-bound, not fixed).
# Previously this python block's exit status and PASS/FAIL text were discarded — a same-ImageID
# result still printed Overall: PASS (Seshat T2, hollow evidence).
id_verdict=$(python3 - "$OUT" <<'PY'
import json, sys, pathlib
o=pathlib.Path(sys.argv[1])
try:
    a=json.load(open(o/"proof_factorial/backend/risc0/risc0_metadata.json"))["image_id"]
    b=json.load(open(o/"proof_fib/backend/risc0/risc0_metadata.json"))["image_id"]
    print(f"{'PASS' if a!=b else 'FAIL'} distinct ImageIDs (binding): a={a[:22]} b={b[:22]}")
except Exception as e:
    print(f"FAIL distinct ImageIDs: error={e}")
PY
)
echo "  $id_verdict"
case "$id_verdict" in
  PASS*) : ;;
  *) overall=FAIL ;;
esac

echo "{ \"overall_verdict\": \"$overall\" }" > "$OUT/report.json"
echo "Overall: $overall"
[ "$overall" = "PASS" ]
