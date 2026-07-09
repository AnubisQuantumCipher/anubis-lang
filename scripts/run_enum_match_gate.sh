#!/usr/bin/env bash
# Enum + match language gate (parse, typecheck, execute, optional prove).
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
OUT="${1:-out/enum_match}"
if [[ "${1:-}" == "--out" && -n "${2:-}" ]]; then OUT="$2"; fi
REF="${ANUBIS_RISC0_METAL_REFERENCE:-/Users/sicarii/Desktop/metal-hybrid-prover}"
mkdir -p "$OUT"
BIN=target/release/anubis
[ -x "$BIN" ] || { echo "FAIL: build anubis release first"; exit 1; }
overall=PASS

echo "── check + run enum_status ──"
if ! "$BIN" check examples/enum_status.anb >"$OUT/check.log" 2>&1; then
  echo "  FAIL check"; overall=FAIL
else
  echo "  PASS check"
fi
if ! "$BIN" run examples/enum_status.anb --evidence --out "$OUT/run" >"$OUT/run.log" 2>&1; then
  echo "  FAIL run"; overall=FAIL
else
  out=$(cat "$OUT/run/stdout.txt" 2>/dev/null | tr -d ' \n')
  if [[ "$out" == "42" ]]; then echo "  PASS run stdout=42"; else echo "  FAIL stdout='$out'"; overall=FAIL; fi
fi

echo "── prove enum status tag=1 n=42 → code 42 ──"
if [[ -d "$REF/vendor/risc0-circuit-rv32im" ]]; then
  if ! "$BIN" prove examples/proof/proof_enum_status.anb --backend risc0 --lane cpu \
      --metal-reference "$REF" --input-json '{"n":42}' \
      --evidence --out "$OUT/prove" >"$OUT/prove.log" 2>&1; then
    echo "  FAIL prove"; overall=FAIL
  else
    python3 - "$OUT/prove" <<'PY' || overall=FAIL
import json, pathlib, sys
m=json.load(open(pathlib.Path(sys.argv[1])/"backend"/"risc0"/"risc0_metadata.json"))
jf=m.get("journal_fields") or {}
by={f["name"]:f["value_u32"] for f in jf.get("fields",[])}
ok=(m.get("verify_status")=="passed"
    and by.get("code")==42
    and by.get("ok_unit")==0
    and by.get("pending_unit")==1)
print(f"{'PASS' if ok else 'FAIL'} journal={by} verify={m.get('verify_status')}")
sys.exit(0 if ok else 1)
PY
  fi
else
  echo "  SKIP prove (no metal ref)"
fi

echo "{ \"overall_verdict\": \"$overall\" }" > "$OUT/report.json"
echo "Overall: $overall"
[ "$overall" = "PASS" ]
