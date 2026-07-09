#!/usr/bin/env bash
# Multi-field journal gate: return [sum, product] commits two u32s; scalar path still works.
set -uo pipefail
REF="${ANUBIS_RISC0_METAL_REFERENCE:-/Users/sicarii/Desktop/metal-hybrid-prover}"
OUT="out/multi_field_journal"
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
overall=PASS

echo "── multi-field prove a=3 b=4 expect journal [7, 12] ──"
if ! "$BIN" prove examples/proof/proof_multi_field.anb --backend risc0 --lane cpu \
    --metal-reference "$REF" \
    --input-json '{"a":3,"b":4}' \
    --evidence --out "$OUT/multi_3_4" > "$OUT/multi_3_4.log" 2>&1; then
  echo "  FAIL: prove nonzero"; overall=FAIL
else
  python3 - "$OUT/multi_3_4" <<'PY' || overall=FAIL
import json, struct, sys, pathlib
d = pathlib.Path(sys.argv[1])
side = d / "backend" / "risc0"
m = json.load(open(side / "risc0_metadata.json"))
jb = (side / "journal.bin").read_bytes()
gsrc = (side / "guest/src/main.rs").read_text()
assert len(jb) == 8, f"expected 8-byte journal, got {len(jb)}"
fields = list(struct.unpack("<II", jb))
ok = (
    fields == [7, 12]
    and m.get("verify_status") == "passed"
    and m.get("fresh_receipt_generated")
    and not m.get("dev_mode")
    and not m.get("mock_prover")
    and m.get("guest_binding") == "anubis-program"
    and m.get("parameterized") is True
    and "anubis_commit_journal" in gsrc
    and "x * 6" not in gsrc
)
print(f"{'PASS' if ok else 'FAIL'} fields={fields} verify={m.get('verify_status')} image={str(m.get('image_id'))[:24]}")
sys.exit(0 if ok else 1)
PY
fi

# Different inputs → different multi-field journals; same ImageID
echo "── multi-field prove a=5 b=6 expect [11, 30] ──"
if ! "$BIN" prove examples/proof/proof_multi_field.anb --backend risc0 --lane cpu \
    --metal-reference "$REF" \
    --input-json '{"a":5,"b":6}' \
    --evidence --out "$OUT/multi_5_6" > "$OUT/multi_5_6.log" 2>&1; then
  echo "  FAIL: prove nonzero"; overall=FAIL
else
  python3 - "$OUT" <<'PY' || overall=FAIL
import json, struct, pathlib, sys
o = pathlib.Path(sys.argv[1])
a = json.load(open(o / "multi_3_4/backend/risc0/risc0_metadata.json"))
b = json.load(open(o / "multi_5_6/backend/risc0/risc0_metadata.json"))
ja = struct.unpack("<II", (o / "multi_3_4/backend/risc0/journal.bin").read_bytes())
jb = struct.unpack("<II", (o / "multi_5_6/backend/risc0/journal.bin").read_bytes())
same_id = a["image_id"] == b["image_id"]
diff_j = ja != jb
ok = same_id and diff_j and list(jb) == [11, 30] and b.get("verify_status") == "passed"
print(f"{'PASS' if ok else 'FAIL'} same_ImageID={same_id} journals {list(ja)} vs {list(jb)}")
sys.exit(0 if ok else 1)
PY
fi

# Scalar path still single u32 (regression: factorial n=5 → 120)
echo "── scalar regression factorial n=5 → 120 ──"
if ! "$BIN" prove examples/proof/proof_factorial_input.anb --backend risc0 --lane cpu \
    --metal-reference "$REF" \
    --input-json '{"n":5}' \
    --out "$OUT/scalar_fact" > "$OUT/scalar_fact.log" 2>&1; then
  echo "  FAIL: scalar prove"; overall=FAIL
else
  j=$(python3 -c 'import struct,pathlib; print(struct.unpack("<I", pathlib.Path("'"$OUT"'/scalar_fact/backend/risc0/journal.bin").read_bytes()[:4])[0])')
  if [ "$j" = "120" ]; then echo "  PASS journal=120"; else echo "  FAIL journal=$j"; overall=FAIL; fi
fi

echo "{ \"overall_verdict\": \"$overall\" }" > "$OUT/report.json"
echo "Overall: $overall"
[ "$overall" = "PASS" ]
