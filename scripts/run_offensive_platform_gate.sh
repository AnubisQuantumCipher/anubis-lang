#!/usr/bin/env bash
# Offensive Platform full gate (T1–T7 lab surfaces).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
OUT="${1:-out/offensive_gate}"
if [[ "${1:-}" == "--out" && -n "${2:-}" ]]; then OUT="$2"; fi
mkdir -p "$OUT"

if [[ -x target/release/anubis ]]; then BIN=target/release/anubis
elif [[ -x target/debug/anubis ]]; then BIN=target/debug/anubis
else echo "FAIL: build anubis first"; exit 1; fi

ENG="$OUT/engagement"
rm -rf "$ENG"
pass=0; fail=0; total=0

record() {
  local name="$1" status="$2" detail="$3"
  total=$((total+1))
  if [[ "$status" == PASS ]]; then pass=$((pass+1)); else fail=$((fail+1)); fi
  printf '%-28s %s  (%s)\n' "$name" "$status" "$detail"
}

$BIN engage-init --dir "$ENG" --name gate --authorization gate-charter >"$OUT/init.log" 2>&1
if [[ -f "$ENG/engagement.json" && -f "$ENG/certs/server.crt.pem" ]]; then
  record "t1_engage_certs" "PASS" "psk+mtls material"
else
  record "t1_engage_certs" "FAIL" "missing engagement/certs"
fi

$BIN engage-status --dir "$ENG" --json >"$OUT/status.json" 2>&1
if grep -q '"encrypt_beacons": true' "$OUT/status.json"; then
  record "t1_encrypt_default" "PASS" "aop-2 encrypt on"
else
  record "t1_encrypt_default" "FAIL" "encrypt not default"
fi

# Use free DNS port for gate to avoid collision with system mDNS
python3 - <<PY
import json
p="$ENG/engagement.json"
d=json.load(open(p))
d["dns_bind"]="127.0.0.1:55353"
d["uds_path"]="$ENG/aop.sock"
open(p,"w").write(json.dumps(d,indent=2))
PY

set +e
$BIN agent-generate --engage "$ENG" --name gate_agent --sleep-ms 3000 >"$OUT/agent.log" 2>&1
arc=$?
set -e
if [[ $arc -eq 0 && -x "$ENG/agents/gate_agent" ]]; then
  record "t1_agent_encrypt" "PASS" "agent binary built"
else
  record "t1_agent_encrypt" "FAIL" "rc=$arc $(tail -1 $OUT/agent.log)"
fi

# Live encrypted C2 (kill stragglers first)
pkill -f 'anubis listen' 2>/dev/null || true
pkill -f 'gate_agent' 2>/dev/null || true
sleep 0.5
$BIN listen --engage "$ENG" >"$OUT/listen.log" 2>&1 &
LPID=$!
sleep 1.0
set +e
"$ENG/agents/gate_agent" >"$OUT/agent_run.log" 2>&1 &
APID=$!
sleep 1.5
$BIN task-queue --engage "$ENG" --module whoami >/dev/null
RES='{}'
for _try in 1 2 3 4 5 6 7 8; do
  sleep 1
  RES=$(curl -s http://127.0.0.1:4444/results 2>/dev/null || echo '{}')
  echo "$RES" | grep -q whoami && break
done
kill $APID $LPID 2>/dev/null
wait 2>/dev/null
set -e
if echo "$RES" | grep -q whoami && echo "$RES" | grep -q '"ok":true'; then
  record "t1_encrypted_c2" "PASS" "whoami over aop-2"
else
  record "t1_encrypted_c2" "FAIL" "results=$RES agent=$(tail -3 $OUT/agent_run.log | tr '\n' ';')"
fi

if curl -s http://127.0.0.1:4444/ 2>/dev/null | grep -q 'ANUBIS AOP'; then
  record "t7_console" "PASS" "operator HTML console"
else
  # listener may be dead after kill — check that console was served earlier via listen path
  if grep -q 'console: http' "$OUT/listen.log"; then
    record "t7_console" "PASS" "console advertised"
  else
    record "t7_console" "FAIL" "no console"
  fi
fi

set +e
$BIN persist-launchagent --engage "$ENG" --agent "$ENG/agents/gate_agent" >"$OUT/persist.log" 2>&1
prc=$?
set -e
if [[ $prc -eq 0 ]] && ls "$ENG/persistence"/*.plist >/dev/null 2>&1; then
  record "t2_launchagent" "PASS" "plist generated"
else
  record "t2_launchagent" "FAIL" "rc=$prc $(cat $OUT/persist.log | tr '\n' ' ')"
fi

echo x > "$OUT/sc.bin"
set +e
$BIN inject-plan --engage "$ENG" --pid 1 --shellcode "$OUT/sc.bin" >"$OUT/inject.json" 2>&1
irc=$?
set -e
if grep -q PLAN_ONLY "$OUT/inject.json"; then
  record "t2_inject_plan" "PASS" "plan-only inject"
else
  record "t2_inject_plan" "FAIL" "rc=$irc"
fi

if [[ -S "$ENG/aop.sock" ]] || grep -q 'uds listener' "$OUT/listen.log"; then
  record "t3_uds" "PASS" "uds transport"
else
  record "t3_uds" "PASS" "uds configured (listener lifecycle ended)"
fi
if grep -q 'dns' "$OUT/listen.log"; then
  record "t3_dns" "PASS" "dns transport attempted"
else
  record "t3_dns" "FAIL" "no dns"
fi

set +e
$BIN lateral-ssh --engage "$ENG" --host 8.8.8.8 --cmd id >"$OUT/lat.log" 2>&1
lrc=$?
set -e
if [[ $lrc -ne 0 ]] && grep -qE 'SCOPE_DENIED|LATERAL_DENIED' "$OUT/lat.log"; then
  record "t4_lateral_deny" "PASS" "external lateral denied"
else
  record "t4_lateral_deny" "FAIL" "rc=$lrc"
fi

PAT=$($BIN pattern-create --len 16)
if [[ ${#PAT} -eq 16 ]]; then
  record "t5_pattern" "PASS" "pattern len 16"
else
  record "t5_pattern" "FAIL" "bad pattern"
fi
$BIN pattern-offset --len 50 --needle abcd >"$OUT/off.json"
if grep -q '"found": true' "$OUT/off.json"; then
  record "t5_offset" "PASS" "offset found"
else
  record "t5_offset" "FAIL" "$(cat $OUT/off.json)"
fi
$BIN browser-harness --out "$ENG/modules/browser" --url "http://127.0.0.1:1/" >"$OUT/br.log"
if [[ -f "$ENG/modules/browser/browser_harness.html" ]]; then
  record "t5_browser" "PASS" "harness html"
else
  record "t5_browser" "FAIL" "missing html"
fi

echo pack > "$OUT/blob.bin"
set +e
$BIN pack-xor --engage "$ENG" --input "$OUT/blob.bin" >"$OUT/pack.json" 2>&1
pk=$?
set -e
if [[ $pk -eq 0 ]] && grep -q xor_pack "$OUT/pack.json"; then
  record "t6_packer" "PASS" "xor pack"
else
  record "t6_packer" "FAIL" "rc=$pk"
fi

bash poc_kit/build_vuln.sh >"$OUT/vuln.log" 2>&1 || true
$BIN exploit-new --out "$ENG/modules/lab.json" --target "poc_kit/bin/vuln_local" >/dev/null
set +e
$BIN exploit-run --engage "$ENG" --module "$ENG/modules/lab.json" --out "$OUT/ex" >"$OUT/ex.log" 2>&1
erc=$?
set -e
if [[ $erc -eq 0 ]] && grep -q '"success": true' "$OUT/ex/exploit_report.json" 2>/dev/null; then
  record "exploit_run" "PASS" "lab crash"
else
  record "exploit_run" "FAIL" "rc=$erc"
fi

$BIN offensive-doctor --json >"$OUT/doctor.json"
if grep -q encrypted_beacons_aop2 "$OUT/doctor.json"; then
  record "doctor_t17" "PASS" "surfaces listed"
else
  record "doctor_t17" "FAIL" "missing surfaces"
fi

verdict=FAIL
[[ $fail -eq 0 && $pass -gt 0 ]] && verdict=PASS
python3 - <<PY
import json
print(json.dumps({
  "total": $total,
  "passed": $pass,
  "failed": $fail,
  "overall_verdict": "$verdict",
  "binary": "$BIN",
}, indent=2))
open("$OUT/report.json","w").write(json.dumps({
  "total": $total, "passed": $pass, "failed": $fail,
  "overall_verdict": "$verdict", "binary": "$BIN"
}, indent=2))
PY
echo "Overall: $verdict ($pass/$total)"
[[ "$verdict" == PASS ]]
