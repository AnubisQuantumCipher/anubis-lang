#!/usr/bin/env bash
# Parameterized proof gate: same program + different inputs → different journals;
# ImageID stable for same program; receipt verifies; input_sha256 recorded.
set -uo pipefail
REF="${ANUBIS_RISC0_METAL_REFERENCE:-/Users/sicarii/Desktop/metal-hybrid-prover}"
OUT="out/parameterized_proof"
while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --metal-reference) REF="$2"; shift 2 ;;
    *) shift ;;
  esac
done
REPO="$(cd "$(dirname "$0")/.." && pwd)"; cd "$REPO"
BIN="target/release/anubis"
[ -x "$BIN" ] || { echo "FAIL: cargo build --release -p anubis first"; exit 1; }
[ -d "$REF/vendor/risc0-circuit-rv32im" ] || { echo "FAIL: RISC0 reference missing at $REF"; exit 1; }
mkdir -p "$OUT"
PROG="examples/proof/proof_factorial_input.anb"
overall=PASS

prove_case() {
  local name="$1" json="$2" want="$3"
  local d="$OUT/$name"
  echo "── prove $name input=$json expect=$want ──"
  if ! "$BIN" prove "$PROG" --backend risc0 --lane cpu \
      --metal-reference "$REF" \
      --input-json "$json" \
      --evidence --out "$d" > "$OUT/$name.log" 2>&1; then
    echo "  FAIL: prove nonzero"; overall=FAIL; return
  fi
  local verdict
  verdict=$(python3 - "$d" "$want" <<'PY'
import json, struct, sys, pathlib
d=pathlib.Path(sys.argv[1]); want=int(sys.argv[2])
side=d/"backend"/"risc0"
try:
    m=json.load(open(side/"risc0_metadata.json"))
    jb=(side/"journal.bin").read_bytes()
    j=struct.unpack("<I", jb[:4])[0]
    gsrc=(side/"guest/src/main.rs").read_text()
    ok=(m.get("verify_status")=="passed" and m.get("fresh_receipt_generated") and
        not m.get("dev_mode") and not m.get("mock_prover") and
        m.get("guest_binding")=="anubis-program" and j==want and
        not m.get("image_id_is_placeholder") and
        m.get("parameterized") is True and
        m.get("input_sha256") and
        "proof_input_u32" in gsrc and "anubis_load_proof_inputs" in gsrc and
        "x * 6" not in gsrc)
    print(f"{'PASS' if ok else 'FAIL'} journal={j} verify={m.get('verify_status')} input_sha={str(m.get('input_sha256'))[:16]} imageid={str(m.get('image_id'))[:20]}")
except Exception as e:
    print(f"FAIL error={e}")
PY
)
  echo "  $verdict"
  case "$verdict" in PASS*) : ;; *) overall=FAIL ;; esac
}

prove_case "fact_n5" '{"n":5}' 120
prove_case "fact_n6" '{"n":6}' 720

# Same program → same ImageID; different inputs → different journals (already checked)
python3 - "$OUT" <<'PY'
import json, pathlib, sys
o=pathlib.Path(sys.argv[1])
try:
    a=json.load(open(o/"fact_n5/backend/risc0/risc0_metadata.json"))
    b=json.load(open(o/"fact_n6/backend/risc0/risc0_metadata.json"))
    same_id = a["image_id"]==b["image_id"]
    diff_in = a["input_sha256"]!=b["input_sha256"]
    print(f"same ImageID (program-bound): {'PASS' if same_id else 'FAIL'}")
    print(f"different input_sha256: {'PASS' if diff_in else 'FAIL'}")
    if not same_id or not diff_in:
        sys.exit(2)
except Exception as e:
    print(f"binding compare FAIL: {e}")
    sys.exit(2)
PY
[ $? -eq 0 ] || overall=FAIL

# File-based input path
echo "── prove fact_file n=5 via --input-file ──"
if "$BIN" prove "$PROG" --backend risc0 --lane cpu --metal-reference "$REF" \
    --input-file examples/proof/inputs/factorial_5.json \
    --out "$OUT/fact_file" > "$OUT/fact_file.log" 2>&1; then
  j=$(python3 -c 'import struct,pathlib; print(struct.unpack("<I", pathlib.Path("'"$OUT"'/fact_file/backend/risc0/journal.bin").read_bytes()[:4])[0])')
  if [ "$j" = "120" ]; then echo "  PASS journal=120"; else echo "  FAIL journal=$j"; overall=FAIL; fi
else
  echo "  FAIL prove file path"; overall=FAIL
fi

echo "{ \"overall_verdict\": \"$overall\" }" > "$OUT/report.json"
echo "Overall: $overall"
[ "$overall" = "PASS" ]
