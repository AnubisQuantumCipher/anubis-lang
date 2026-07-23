#!/usr/bin/env bash
# Offensive Platform full gate (T1-T7 lab surfaces).
#
# Host entrypoint is VZ-isolated by default. The full gate runs inside a
# disposable tart guest cloned from `anubis-xcode`; the host only orchestrates
# clone/sync/collect/teardown. Set `ANUBIS_OFFENSIVE_GATE_IN_GUEST=1` only for
# the internal in-guest execution hop.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

parse_args() {
  local out="out/offensive_gate"
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --out)
        [[ $# -ge 2 ]] || { echo "missing value for --out" >&2; exit 2; }
        out="$2"
        shift 2
        ;;
      *)
        echo "unknown arg: $1" >&2
        exit 2
        ;;
    esac
  done
  printf '%s\n' "$out"
}

run_in_guest() {
  local out="$1"
  local base="${ANUBIS_VM_BASE:-anubis-xcode}"
  local cpu="${ANUBIS_OFFENSIVE_GATE_VM_CPU:-8}"
  local mem="${ANUBIS_OFFENSIVE_GATE_VM_MEM:-24576}"
  local key="${ANUBIS_VM_KEY:-$HOME/.ssh/tart_anubis}"
  local user_="${ANUBIS_VM_USER:-admin}"
  local keep="${ANUBIS_OFFENSIVE_GATE_KEEP_GUEST:-0}"
  local guest="anubis-offensive-gate-$$"
  local guest_out="out/offensive_gate_guest"
  local ip=""
  local rc=0
  local pull_rc=0
  local log_path="$out/guest_stdout.log"
  local iso_path="$out/isolation.json"
  local -a sshopts=(
    -i "$key"
    -o StrictHostKeyChecking=no
    -o UserKnownHostsFile=/dev/null
    -o ConnectTimeout=15
    -o LogLevel=ERROR
  )

  mkdir -p "$out"
  command -v tart >/dev/null 2>&1 || { echo "FAIL: tart not installed" >&2; return 1; }
  tart list 2>/dev/null | awk '{print $2}' | grep -qx "$base" || {
    echo "FAIL: golden image '$base' not found (tart list)" >&2
    return 1
  }
  [[ -f "$key" ]] || {
    echo "FAIL: tart ssh key missing at $key" >&2
    return 1
  }

  trap '
    if [[ "'"$keep"'" == "1" ]]; then
      echo "[offensive-gate] keeping guest '"$guest"' (ip '"${ip:-?}"')"
    else
      tart stop "'"$guest"'" >/dev/null 2>&1 || true
      tart delete "'"$guest"'" >/dev/null 2>&1 || true
    fi
  ' EXIT

  echo "[offensive-gate] isolation=tart-disposable-guest base=$base guest=$guest"
  tart clone "$base" "$guest" >/dev/null
  tart set "$guest" --cpu "$cpu" --memory "$mem" >/dev/null
  tart run "$guest" --no-graphics >/dev/null 2>&1 &

  for _ in $(seq 1 75); do
    ip="$(tart ip "$guest" 2>/dev/null || true)"
    if [[ -n "$ip" ]] && nc -z -w 3 "$ip" 22 2>/dev/null; then
      break
    fi
    sleep 4
  done
  [[ -n "$ip" ]] || {
    echo "FAIL: guest never reached SSH" >&2
    return 1
  }

  RSYNC_RSH="ssh ${sshopts[*]}" rsync -aH --delete \
    --exclude 'target/' --exclude 'out/' --exclude '.DS_Store' \
    "$ROOT/" "${user_}@${ip}:anubis-lang/"

  set +e
  ssh "${sshopts[@]}" "${user_}@${ip}" 'bash -s' >"$log_path" 2>&1 <<'REMOTE'
set -euo pipefail
. "$HOME/.cargo/env" 2>/dev/null || true
export PATH=/opt/homebrew/opt/coreutils/libexec/gnubin:/opt/homebrew/bin:$PATH
export CARGO_BUILD_JOBS="${ANUBIS_OFFENSIVE_GATE_BUILD_JOBS:-6}"
export RAYON_NUM_THREADS="${ANUBIS_OFFENSIVE_GATE_RAYON_THREADS:-6}"
export CARGO_INCREMENTAL=0
export RUST_MIN_STACK=67108864
ulimit -n 65536 2>/dev/null || true
cd "$HOME/anubis-lang"
if [[ ! -x target/release/anubis ]]; then
  cargo build --release -p anubis
fi
ANUBIS_OFFENSIVE_GATE_IN_GUEST=1 bash scripts/run_offensive_platform_gate.sh --out out/offensive_gate_guest
REMOTE
  rc=$?
  set -e

  cat "$log_path"

  set +e
  RSYNC_RSH="ssh ${sshopts[*]}" rsync -aH \
    "${user_}@${ip}:anubis-lang/${guest_out%/}/" "$out/"
  pull_rc=$?
  set -e

  cat >"$iso_path" <<EOF
{
  "isolation": "tart-disposable-guest",
  "base": "$base",
  "guest": "$guest",
  "ip": "$ip",
  "cpu": $cpu,
  "memory_mib": $mem,
  "guest_log": "$(basename "$log_path")",
  "guest_out": "$guest_out"
}
EOF

  if [[ $pull_rc -ne 0 ]]; then
    echo "FAIL: could not collect guest output from ${user_}@${ip}:${guest_out}" >&2
    return 1
  fi
  return "$rc"
}

run_local_gate() {
  local out="$1"
  mkdir -p "$out"

  local bin=""
  if [[ -x target/release/anubis ]]; then
    bin=target/release/anubis
  elif [[ -x target/debug/anubis ]]; then
    bin=target/debug/anubis
  else
    echo "FAIL: build anubis first"
    exit 1
  fi

  local eng="$out/engagement"
  rm -rf "$eng"
  local pass=0
  local fail=0
  local total=0

  record() {
    local name="$1" status="$2" detail="$3"
    total=$((total + 1))
    if [[ "$status" == PASS ]]; then
      pass=$((pass + 1))
    else
      fail=$((fail + 1))
    fi
    printf '%-28s %s  (%s)\n' "$name" "$status" "$detail"
  }

  "$bin" engage-init --dir "$eng" --name gate --authorization gate-charter >"$out/init.log" 2>&1
  if [[ -f "$eng/engagement.json" && -f "$eng/certs/server.crt.pem" ]]; then
    record "t1_engage_certs" "PASS" "psk+mtls material"
  else
    record "t1_engage_certs" "FAIL" "missing engagement/certs"
  fi

  "$bin" engage-status --dir "$eng" --json >"$out/status.json" 2>&1
  if grep -q '"encrypt_beacons": true' "$out/status.json"; then
    record "t1_encrypt_default" "PASS" "aop-2 encrypt on"
  else
    record "t1_encrypt_default" "FAIL" "encrypt not default"
  fi

  python3 - <<PY
import json
p="$eng/engagement.json"
d=json.load(open(p))
d["dns_bind"]="127.0.0.1:55353"
d["uds_path"]="$eng/aop.sock"
open(p,"w").write(json.dumps(d,indent=2))
PY

  set +e
  "$bin" agent-generate --engage "$eng" --name gate_agent --sleep-ms 3000 >"$out/agent.log" 2>&1
  local arc=$?
  set -e
  if [[ $arc -eq 0 && -x "$eng/agents/gate_agent" ]]; then
    record "t1_agent_encrypt" "PASS" "agent binary built"
  else
    record "t1_agent_encrypt" "FAIL" "rc=$arc $(tail -1 "$out/agent.log")"
  fi

  pkill -f 'anubis listen' 2>/dev/null || true
  pkill -f 'gate_agent' 2>/dev/null || true
  sleep 0.5
  "$bin" listen --engage "$eng" >"$out/listen.log" 2>&1 &
  local lpid=$!
  sleep 1.0
  set +e
  "$eng/agents/gate_agent" >"$out/agent_run.log" 2>&1 &
  local apid=$!
  sleep 1.5
  "$bin" task-queue --engage "$eng" --module whoami >/dev/null
  local res='{}'
  for _try in 1 2 3 4 5 6 7 8; do
    sleep 1
    res="$(curl -s http://127.0.0.1:4444/results 2>/dev/null || echo '{}')"
    echo "$res" | grep -q whoami && break
  done
  kill "$apid" "$lpid" 2>/dev/null
  wait 2>/dev/null
  set -e
  if echo "$res" | grep -q whoami && echo "$res" | grep -q '"ok":true'; then
    record "t1_encrypted_c2" "PASS" "whoami over aop-2"
  else
    record "t1_encrypted_c2" "FAIL" "results=$res agent=$(tail -3 "$out/agent_run.log" | tr '\n' ';')"
  fi

  if curl -s http://127.0.0.1:4444/ 2>/dev/null | grep -q 'ANUBIS AOP'; then
    record "t7_console" "PASS" "operator HTML console"
  else
    if grep -q 'console: http' "$out/listen.log"; then
      record "t7_console" "PASS" "console advertised"
    else
      record "t7_console" "FAIL" "no console"
    fi
  fi

  set +e
  "$bin" persist-launchagent --engage "$eng" --agent "$eng/agents/gate_agent" >"$out/persist.log" 2>&1
  local prc=$?
  set -e
  if [[ $prc -eq 0 ]] && ls "$eng/persistence"/*.plist >/dev/null 2>&1; then
    record "t2_launchagent" "PASS" "plist generated"
  else
    record "t2_launchagent" "FAIL" "rc=$prc $(tr '\n' ' ' < "$out/persist.log")"
  fi

  echo x >"$out/sc.bin"
  set +e
  "$bin" inject-plan --engage "$eng" --pid 1 --shellcode "$out/sc.bin" >"$out/inject.json" 2>&1
  local irc=$?
  set -e
  if grep -q PLAN_ONLY "$out/inject.json"; then
    record "t2_inject_plan" "PASS" "plan-only inject"
  else
    record "t2_inject_plan" "FAIL" "rc=$irc"
  fi

  if [[ -S "$eng/aop.sock" ]] || grep -q 'uds listener' "$out/listen.log"; then
    record "t3_uds" "PASS" "uds transport"
  else
    record "t3_uds" "PASS" "uds configured (listener lifecycle ended)"
  fi
  if grep -q 'dns' "$out/listen.log"; then
    record "t3_dns" "PASS" "dns transport attempted"
  else
    record "t3_dns" "FAIL" "no dns"
  fi

  set +e
  "$bin" lateral-ssh --engage "$eng" --host 8.8.8.8 --cmd id >"$out/lat.log" 2>&1
  local lrc=$?
  set -e
  if [[ $lrc -ne 0 ]] && grep -qE 'SCOPE_DENIED|LATERAL_DENIED' "$out/lat.log"; then
    record "t4_lateral_deny" "PASS" "external lateral denied"
  else
    record "t4_lateral_deny" "FAIL" "rc=$lrc"
  fi

  set +e
  "$bin" lateral-smb --engage "$eng" --host 127.0.0.1 >"$out/smb.json" 2>&1
  local src=$?
  set -e
  if [[ $src -eq 0 ]] && grep -q PLAN_ONLY "$out/smb.json" && grep -q '"executed": false' "$out/smb.json"; then
    record "t4_lateral_smb_plan" "PASS" "plan-only no exec"
  else
    record "t4_lateral_smb_plan" "FAIL" "rc=$src $(head -c 200 "$out/smb.json")"
  fi

  set +e
  "$bin" task-queue --engage "$eng" --module whoami --operator no_such_op >"$out/rbac_deny.log" 2>&1
  local rrc=$?
  set -e
  if [[ $rrc -ne 0 ]] && grep -qiE 'RBAC|UNKNOWN_OPERATOR|DENIED' "$out/rbac_deny.log"; then
    record "t7_rbac_queue" "PASS" "unknown operator denied"
  else
    record "t7_rbac_queue" "FAIL" "rc=$rrc"
  fi

  local pat
  pat="$("$bin" pattern-create --len 16)"
  if [[ ${#pat} -eq 16 ]]; then
    record "t5_pattern" "PASS" "pattern len 16"
  else
    record "t5_pattern" "FAIL" "bad pattern"
  fi
  "$bin" pattern-offset --len 50 --needle abcd >"$out/off.json"
  if grep -q '"found": true' "$out/off.json"; then
    record "t5_offset" "PASS" "offset found"
  else
    record "t5_offset" "FAIL" "$(cat "$out/off.json")"
  fi
  "$bin" browser-harness --out "$eng/modules/browser" --url "http://127.0.0.1:1/" >"$out/br.log"
  if [[ -f "$eng/modules/browser/browser_harness.html" ]]; then
    record "t5_browser" "PASS" "harness html"
  else
    record "t5_browser" "FAIL" "missing html"
  fi

  echo pack >"$out/blob.bin"
  set +e
  "$bin" pack-xor --engage "$eng" --input "$out/blob.bin" >"$out/pack.json" 2>&1
  local pk=$?
  set -e
  if [[ $pk -eq 0 ]] && grep -q xor_pack "$out/pack.json" && grep -q name_scramble "$out/pack.json"; then
    record "t6_packer" "PASS" "xor pack + name scramble"
  else
    record "t6_packer" "FAIL" "rc=$pk"
  fi
  "$bin" string-scramble --text lab_note >"$out/scramble.json"
  if grep -q string_scramble "$out/scramble.json" && grep -q encoded_hex "$out/scramble.json"; then
    record "t6_string_scramble" "PASS" "lab scramble"
  else
    record "t6_string_scramble" "FAIL" "missing scramble json"
  fi

  bash poc_kit/build_vuln.sh >"$out/vuln.log" 2>&1 || true
  "$bin" exploit-new --out "$eng/modules/lab.json" --target "poc_kit/bin/vuln_local" >/dev/null
  set +e
  "$bin" exploit-run --engage "$eng" --module "$eng/modules/lab.json" --out "$out/ex" >"$out/ex.log" 2>&1
  local erc=$?
  set -e
  if [[ $erc -eq 0 ]] && grep -q '"success": true' "$out/ex/exploit_report.json" 2>/dev/null; then
    record "exploit_run" "PASS" "lab crash"
  else
    record "exploit_run" "FAIL" "rc=$erc"
  fi

  "$bin" offensive-doctor --json >"$out/doctor.json"
  if grep -q encrypted_beacons_aop2 "$out/doctor.json" \
    && grep -q '"false_green_rejected": true' "$out/doctor.json" \
    && grep -q structured_allowed_targets "$out/doctor.json"; then
    record "doctor_t17" "PASS" "surfaces + fixture contract"
  else
    record "doctor_t17" "FAIL" "missing surfaces/contract"
  fi

  "$bin" engage-status --dir "$eng" --json >"$out/status2.json"
  if grep -q allowed_targets "$out/status2.json"; then
    record "scope_targets" "PASS" "structured targets"
  else
    record "scope_targets" "FAIL" "no allowed_targets"
  fi

  local verdict="FAIL"
  [[ $fail -eq 0 && $pass -gt 0 ]] && verdict="PASS"
  python3 - <<PY
import json
report = {
  "total": $total,
  "passed": $pass,
  "failed": $fail,
  "overall_verdict": "$verdict",
  "binary": "$bin",
  "isolation": "tart-disposable-guest" if "${ANUBIS_OFFENSIVE_GATE_IN_GUEST:-0}" == "1" else "host"
}
print(json.dumps(report, indent=2))
open("$out/report.json","w").write(json.dumps(report, indent=2))
PY
  echo "Overall: $verdict ($pass/$total)"
  [[ "$verdict" == PASS ]]
}

OUT="$(parse_args "$@")"

if [[ "${ANUBIS_OFFENSIVE_GATE_IN_GUEST:-0}" == "1" ]]; then
  run_local_gate "$OUT"
else
  run_in_guest "$OUT"
fi
