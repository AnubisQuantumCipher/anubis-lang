#!/usr/bin/env bash
# Language power trio gate: maps, struct-like enum variants, if-expressions.
# Fail-closed. Expected stdout values are fixed.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
OUT="${1:-out/lang_trio}"
if [[ "${1:-}" == "--out" && -n "${2:-}" ]]; then OUT="$2"; fi
REF="${ANUBIS_RISC0_METAL_REFERENCE:-/Users/sicarii/Desktop/metal-hybrid-prover}"
mkdir -p "$OUT"
if [[ -x target/release/anubis ]]; then BIN=target/release/anubis
elif [[ -x target/debug/anubis ]]; then BIN=target/debug/anubis
else echo "FAIL: build anubis first"; exit 1; fi
overall=PASS

run_expect() {
  local name="$1" src="$2" expect="$3"
  echo "── check + run $name (expect $expect) ──"
  if ! "$BIN" check "$src" >"$OUT/${name}_check.log" 2>&1; then
    echo "  FAIL check"; overall=FAIL; return
  fi
  echo "  PASS check"
  if ! "$BIN" run "$src" --evidence --out "$OUT/${name}_run" >"$OUT/${name}_run.log" 2>&1; then
    echo "  FAIL run"; overall=FAIL; return
  fi
  local out
  out=$(tr -d ' \n' <"$OUT/${name}_run/stdout.txt" 2>/dev/null || true)
  if [[ "$out" == "$expect" ]]; then
    echo "  PASS run stdout=$expect"
  else
    echo "  FAIL stdout='$out' expected='$expect'"
    overall=FAIL
  fi
}

run_expect if_expr examples/if_expr.anb 7
run_expect map_dict examples/map_dict.anb 6
run_expect enum_struct_variant examples/enum_struct_variant.anb 99
run_expect lang_power_trio examples/lang_power_trio.anb 42

echo "── prove proof_lang_trio (secret in range) ──"
if [[ -d "$REF/vendor/risc0-circuit-rv32im" ]]; then
  if ! "$BIN" prove examples/proof/proof_lang_trio.anb --backend risc0 --lane cpu \
      --metal-reference "$REF" \
      --input-json '{"secret":7,"lo":1,"hi":10}' \
      --evidence --out "$OUT/prove" >"$OUT/prove.log" 2>&1; then
    echo "  FAIL prove"; overall=FAIL
  else
    python3 - "$OUT/prove" <<'PY' || overall=FAIL
import json, pathlib, sys
root = pathlib.Path(sys.argv[1])
meta = root / "backend" / "risc0" / "risc0_metadata.json"
if not meta.exists():
    # alternate layout
    candidates = list(root.rglob("risc0_metadata.json"))
    if not candidates:
        print("FAIL no risc0_metadata.json"); sys.exit(1)
    meta = candidates[0]
m = json.load(open(meta))
jf = m.get("journal_fields") or {}
by = {f["name"]: f["value_u32"] for f in jf.get("fields", [])}
ok = (
    m.get("verify_status") == "passed"
    and by.get("code") == 7
    and by.get("ok") == 1
)
print(f"{'PASS' if ok else 'FAIL'} journal={by} verify={m.get('verify_status')}")
sys.exit(0 if ok else 1)
PY
  fi
else
  echo "  SKIP prove (no metal ref at $REF)"
fi

echo "{ \"overall_verdict\": \"$overall\" }" > "$OUT/report.json"
echo "Overall: $overall"
[[ "$overall" = "PASS" ]]
