#!/usr/bin/env bash
# Named journal fields gate: proof_commit_u32 → journal_fields in metadata.
set -uo pipefail
REF="${ANUBIS_RISC0_METAL_REFERENCE:-/Users/sicarii/Desktop/metal-hybrid-prover}"
OUT="out/named_journal"
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

echo "── named prove a=3 b=4 expect sum=7 product=12 ──"
if ! "$BIN" prove examples/proof/proof_named_fields.anb --backend risc0 --lane cpu \
    --metal-reference "$REF" \
    --input-json '{"a":3,"b":4}' \
    --evidence --out "$OUT/named_3_4" > "$OUT/named_3_4.log" 2>&1; then
  echo "  FAIL: prove nonzero"; overall=FAIL
else
  python3 - "$OUT/named_3_4" <<'PY' || overall=FAIL
import json, struct, sys, pathlib
d = pathlib.Path(sys.argv[1])
side = d / "backend" / "risc0"
m = json.load(open(side / "risc0_metadata.json"))
jb = (side / "journal.bin").read_bytes()
gsrc = (side / "guest/src/main.rs").read_text()
dec = json.load(open(side / "journal_decoded.json"))
fields = list(struct.unpack("<II", jb))
jf = m.get("journal_fields") or {}
by_name = {f["name"]: f["value_u32"] for f in jf.get("fields", [])}
ok = (
    fields == [7, 12]
    and m.get("verify_status") == "passed"
    and m.get("guest_binding") == "anubis-program"
    and jf.get("named") is True
    and by_name.get("sum") == 7
    and by_name.get("product") == 12
    and dec.get("named") is True
    and "anubis_proof_commit_u32" in gsrc
    and "proof_commit_u32" in gsrc
)
print(f"{'PASS' if ok else 'FAIL'} fields={fields} named={by_name} verify={m.get('verify_status')}")
sys.exit(0 if ok else 1)
PY
fi

# Unnamed list multi-field still gets synthetic field_0, field_1
echo "── unnamed list multi-field still decodes (synthetic names) ──"
if ! "$BIN" prove examples/proof/proof_multi_field.anb --backend risc0 --lane cpu \
    --metal-reference "$REF" \
    --input-json '{"a":3,"b":4}' \
    --out "$OUT/unnamed" > "$OUT/unnamed.log" 2>&1; then
  echo "  FAIL: multi prove"; overall=FAIL
else
  python3 - "$OUT/unnamed" <<'PY' || overall=FAIL
import json, pathlib, sys
m=json.load(open(pathlib.Path(sys.argv[1])/"backend"/"risc0"/"risc0_metadata.json"))
jf=m.get("journal_fields") or {}
names=[f["name"] for f in jf.get("fields",[])]
vals=[f["value_u32"] for f in jf.get("fields",[])]
ok = vals==[7,12] and names==["field_0","field_1"] and jf.get("named") is False
print(f"{'PASS' if ok else 'FAIL'} names={names} vals={vals}")
sys.exit(0 if ok else 1)
PY
fi

# Scalar still single field_0
echo "── scalar factorial n=5 → field_0=120 ──"
if ! "$BIN" prove examples/proof/proof_factorial_input.anb --backend risc0 --lane cpu \
    --metal-reference "$REF" \
    --input-json '{"n":5}' \
    --out "$OUT/scalar" > "$OUT/scalar.log" 2>&1; then
  echo "  FAIL: scalar"; overall=FAIL
else
  python3 - "$OUT/scalar" <<'PY' || overall=FAIL
import json, pathlib, sys
m=json.load(open(pathlib.Path(sys.argv[1])/"backend"/"risc0"/"risc0_metadata.json"))
jf=m.get("journal_fields") or {}
ok = jf.get("field_count")==1 and jf["fields"][0]["value_u32"]==120
print(f"{'PASS' if ok else 'FAIL'} jf={jf}")
sys.exit(0 if ok else 1)
PY
fi

echo "{ \"overall_verdict\": \"$overall\" }" > "$OUT/report.json"
echo "Overall: $overall"
[ "$overall" = "PASS" ]
