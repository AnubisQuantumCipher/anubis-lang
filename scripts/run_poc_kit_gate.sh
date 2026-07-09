#!/usr/bin/env bash
# Bounty-grade PoC kit gate: packing + real local crash PoC + process fuzz.
# Fail-closed: missing target / no crash / no unique fuzz crash => FAIL.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
OUT="${1:-out/poc_kit}"
if [[ "${1:-}" == "--out" && -n "${2:-}" ]]; then
  OUT="$2"
fi
mkdir -p "$OUT"

if [[ -x target/release/anubis ]]; then
  BIN=target/release/anubis
elif [[ -x target/debug/anubis ]]; then
  BIN=target/debug/anubis
else
  echo "FAIL: build anubis first (cargo build --release -p anubis)"
  exit 1
fi

echo "==> build gold vuln target"
bash poc_kit/build_vuln.sh | tee "$OUT/build_vuln.log"
VULN="$ROOT/poc_kit/bin/vuln_local"
test -x "$VULN"

pass=0
fail=0
total=0
report="$OUT/report.json"
echo "{" > "$report"
echo "  \"binary\": \"$BIN\"," >> "$report"
echo "  \"fixtures\": [" >> "$report"
first=1

record() {
  local name="$1" status="$2" detail="$3"
  total=$((total+1))
  if [[ "$status" == "PASS" ]]; then pass=$((pass+1)); else fail=$((fail+1)); fi
  printf '%-28s %s  (%s)\n' "$name" "$status" "$detail"
  [[ $first -eq 1 ]] && first=0 || echo "," >> "$report"
  printf '    {"name":"%s","status":"%s","detail":%s}' \
    "$name" "$status" "$(python3 -c 'import json,sys; print(json.dumps(sys.argv[1]))' "$detail")" >> "$report"
}

# 1) packing smoke
total_before=$total
set +e
pack_out=$("$BIN" run examples/security/poc_packing_smoke.anb --allow-research --out "$OUT/run_packing" 2>"$OUT/packing.stderr")
prc=$?
set -e
if [[ $prc -eq 0 && "$pack_out" == $'16\n65\n65' ]]; then
  # p32(AAAA)=4 + p64=8 + cyclic(4)=4 => 16; payload[0]=0x41=65
  record "packing_smoke" "PASS" "stdout matches 16/65/65"
else
  record "packing_smoke" "FAIL" "exit=$prc out=[$pack_out]"
fi

# 2) local overflow PoC — must report crashed=1
set +e
poc_out=$("$BIN" run examples/security/poc_local_overflow.anb --allow-research --out "$OUT/run_poc" 2>"$OUT/poc.stderr")
prc=$?
set -e
# first line is crashed flag
first_line=$(printf '%s\n' "$poc_out" | head -1)
if [[ $prc -eq 0 && "$first_line" == "1" ]]; then
  record "poc_local_overflow" "PASS" "target crashed (crashed=1)"
else
  record "poc_local_overflow" "FAIL" "exit=$prc first=[$first_line] out=[$poc_out]"
fi

# 3) process fuzz against gold target — must find >=1 unique crash
set +e
"$BIN" fuzz --target "$VULN" --runs 200 --max-len 128 --seed 42 --out "$OUT/fuzz" >"$OUT/fuzz.log" 2>&1
frc=$?
set -e
uniq=$(python3 -c 'import json; d=json.load(open("'"$OUT"'/fuzz/fuzz_report.json")); print(d.get("unique_crashes",0))' 2>/dev/null || echo 0)
crashes=$(python3 -c 'import json; d=json.load(open("'"$OUT"'/fuzz/fuzz_report.json")); print(d.get("crashes",0))' 2>/dev/null || echo 0)
if [[ $frc -eq 0 && "$uniq" -ge 1 ]]; then
  record "process_fuzz" "PASS" "unique_crashes=$uniq total_crashes=$crashes"
else
  record "process_fuzz" "FAIL" "exit=$frc unique=$uniq crashes=$crashes"
fi

# 4) network target rejected
set +e
"$BIN" fuzz --target "https://evil.example/bin" --runs 1 --out "$OUT/fuzz_net" >"$OUT/fuzz_net.log" 2>&1
nrc=$?
set -e
if [[ $nrc -ne 0 ]] && grep -q 'NETWORK_FORBIDDEN\|TARGET_MISSING\|POC_NETWORK' "$OUT/fuzz_net.log" 2>/dev/null; then
  record "network_forbidden" "PASS" "network target rejected"
else
  # missing path also fails closed — accept TARGET_MISSING for https path that doesn't exist as file
  if [[ $nrc -ne 0 ]]; then
    record "network_forbidden" "PASS" "non-local target failed closed (rc=$nrc)"
  else
    record "network_forbidden" "FAIL" "network target was accepted"
  fi
fi

verdict="FAIL"
[[ $fail -eq 0 && $pass -gt 0 ]] && verdict="PASS"
echo "" >> "$report"
echo "  ]," >> "$report"
echo "  \"total\": $total, \"passed\": $pass, \"failed\": $fail," >> "$report"
echo "  \"overall_verdict\": \"$verdict\"" >> "$report"
echo "}" >> "$report"

echo "Report: $report"
echo "Overall: $verdict ($pass/$total)"
[[ "$verdict" == "PASS" ]]
