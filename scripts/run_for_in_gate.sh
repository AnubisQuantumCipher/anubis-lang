#!/usr/bin/env bash
# for-in collection iteration gate (range regression + list sum + prove).
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
OUT="${1:-out/for_in}"
if [[ "${1:-}" == "--out" && -n "${2:-}" ]]; then OUT="$2"; fi
REF="${ANUBIS_RISC0_METAL_REFERENCE:-/Users/sicarii/Desktop/metal-hybrid-prover}"
mkdir -p "$OUT"
BIN=target/release/anubis
[ -x "$BIN" ] || { echo "FAIL: build release anubis"; exit 1; }
overall=PASS

echo "── for-in list run ──"
got=$("$BIN" run examples/for_in_list.anb 2>/dev/null | tr -d ' \n')
if [[ "$got" == "60" ]]; then echo "  PASS stdout=60"; else echo "  FAIL got=$got"; overall=FAIL; fi

echo "── for-range regression ──"
got=$("$BIN" run tests/fixtures/turing_core/for_range_sum.anb 2>/dev/null | tr -d ' \n')
if [[ "$got" == "5050" ]]; then echo "  PASS range 5050"; else echo "  FAIL got=$got"; overall=FAIL; fi

echo "── turing fixture for_in_list ──"
got=$("$BIN" run tests/fixtures/turing_core/for_in_list.anb 2>/dev/null | tr -d ' \n')
if [[ "$got" == "15" ]]; then echo "  PASS fixture 15"; else echo "  FAIL got=$got"; overall=FAIL; fi

echo "── prove for-in sum a+b+c ──"
if [[ -d "$REF/vendor/risc0-circuit-rv32im" ]]; then
  if ! "$BIN" prove examples/proof/proof_for_in_sum.anb --backend risc0 --lane cpu \
      --metal-reference "$REF" --input-json '{"a":10,"b":20,"c":30}' \
      --evidence --out "$OUT/prove" >"$OUT/prove.log" 2>&1; then
    echo "  FAIL prove"; overall=FAIL
  else
    python3 - "$OUT/prove" <<'PY' || overall=FAIL
import json, pathlib, sys
m=json.load(open(pathlib.Path(sys.argv[1])/"backend"/"risc0"/"risc0_metadata.json"))
by={f["name"]:f["value_u32"] for f in (m.get("journal_fields") or {}).get("fields",[])}
ok=m.get("verify_status")=="passed" and by.get("sum")==60
print(f"{'PASS' if ok else 'FAIL'} sum={by.get('sum')} verify={m.get('verify_status')}")
sys.exit(0 if ok else 1)
PY
  fi
else
  echo "  SKIP prove"
fi

echo "{ \"overall_verdict\": \"$overall\" }" > "$OUT/report.json"
echo "Overall: $overall"
[ "$overall" = "PASS" ]
