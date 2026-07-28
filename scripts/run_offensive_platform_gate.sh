#!/usr/bin/env bash
# Offensive Platform full gate (T1-T7 lab surfaces).
#
# Host entrypoint is VZ-isolated by default. The full gate runs inside a
# disposable tart guest cloned from `anubis-xcode`; the host only orchestrates
# clone/sync/collect/teardown. Set `ANUBIS_OFFENSIVE_GATE_IN_GUEST=1` only for
# the internal in-guest execution hop.
#
# Acceptance (fail-closed):
#   - Full guest battery PASS requires isolation=tart-disposable-guest and 34/34.
#   - Host isolation witness PASS requires isolation=host-isolation-witness and 5/5.
#   - Missing tart/image/key/binary does NOT auto-promote to witness unless
#     ANUBIS_OFFENSIVE_FORCE_ISOLATION_WITNESS=1 is set explicitly (hosted CI).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
# shellcheck source=lib/gate_evidence.sh
GATE_EVIDENCE_ROOT="$ROOT"
# shellcheck disable=SC1091
source "$ROOT/scripts/lib/gate_evidence.sh"
source "$ROOT/scripts/lib/gate_common.sh"

OFFENSIVE_EXPECTED_GUEST_TOTAL=34
OFFENSIVE_EXPECTED_WITNESS_TOTAL=5

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

resolve_anubis_bin() {
  if [[ -n "${ANUBIS_BIN:-}" ]]; then
    if [[ ! -x "$ANUBIS_BIN" ]]; then
      echo "FAIL: ANUBIS_BIN is not executable: $ANUBIS_BIN" >&2
      return 1
    fi
    printf '%s\n' "$ANUBIS_BIN"
  elif [[ -x target/release/anubis ]]; then
    printf '%s\n' target/release/anubis
  elif [[ -x target/debug/anubis ]]; then
    printf '%s\n' target/debug/anubis
  else
    echo "FAIL: build anubis first or set ANUBIS_BIN" >&2
    return 1
  fi
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
  local host_bin=""
  local binary_sha=""
  local guest_sha_line=""
  local guest_sha=""
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
  local teardown_file="$out/teardown_status.txt"
  echo "not_started" >"$teardown_file"

  # Prerequisites for disposable-guest full battery. Missing tools/images are NOT
  # a green full-suite claim — caller must NOT fall back to the 5/5 witness unless
  # ANUBIS_OFFENSIVE_FORCE_ISOLATION_WITNESS=1 (hosted CI only).
  if ! command -v tart >/dev/null 2>&1; then
    echo "PREREQ_MISSING: tart not installed" >&2
    echo "prereq_missing" >"$teardown_file"
    return 2
  fi
  if ! tart list 2>/dev/null | awk '{print $2}' | grep -qx "$base"; then
    echo "PREREQ_MISSING: golden image '$base' not found (tart list)" >&2
    echo "prereq_missing" >"$teardown_file"
    return 2
  fi
  if [[ ! -f "$key" ]]; then
    echo "PREREQ_MISSING: tart ssh key missing at $key" >&2
    echo "prereq_missing" >"$teardown_file"
    return 2
  fi
  if ! host_bin="$(resolve_anubis_bin)"; then
    echo "prereq_missing" >"$teardown_file"
    return 2
  fi
  binary_sha="$(gate_sha256_file "$host_bin")"

  # Guest name + out dir captured for EXIT trap (locals out of scope after return).
  OFFENSIVE_GATE_GUEST="$guest"
  OFFENSIVE_GATE_KEEP="$keep"
  OFFENSIVE_GATE_TEARDOWN_FILE="$teardown_file"
  cleanup_offensive_guest() {
    local g="${OFFENSIVE_GATE_GUEST:-}"
    local k="${OFFENSIVE_GATE_KEEP:-0}"
    local tf="${OFFENSIVE_GATE_TEARDOWN_FILE:-}"
    if [[ -z "$g" ]]; then
      [[ -n "$tf" ]] && echo "no_guest" >"$tf"
      return 0
    fi
    if [[ "$k" == "1" ]]; then
      echo "[offensive-gate] keeping guest $g"
      [[ -n "$tf" ]] && echo "kept" >"$tf"
    else
      local stop_rc=0 del_rc=0
      tart stop "$g" >/dev/null 2>&1 || stop_rc=$?
      tart delete "$g" >/dev/null 2>&1 || del_rc=$?
      if [[ $stop_rc -eq 0 && $del_rc -eq 0 ]]; then
        [[ -n "$tf" ]] && echo "torn_down" >"$tf"
      else
        [[ -n "$tf" ]] && echo "teardown_failed" >"$tf"
      fi
    fi
    OFFENSIVE_GATE_GUEST=""
  }
  trap cleanup_offensive_guest EXIT

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

  RSYNC_RSH="ssh ${sshopts[*]}" rsync -aH --delete --no-devices --no-specials \
    --exclude 'target/' --exclude 'out/' --exclude 'implementer/a_plus_audit_run/' \
    --exclude '.DS_Store' \
    "$ROOT/" "${user_}@${ip}:anubis-lang/"
  ssh "${sshopts[@]}" "${user_}@${ip}" 'mkdir -p "$HOME/anubis-lang/target/release"'
  RSYNC_RSH="ssh ${sshopts[*]}" rsync -a \
    "$host_bin" "${user_}@${ip}:anubis-lang/target/release/anubis"
  guest_sha_line="$(
    ssh "${sshopts[@]}" "${user_}@${ip}" \
      'shasum -a 256 "$HOME/anubis-lang/target/release/anubis"'
  )"
  guest_sha="${guest_sha_line%% *}"
  if [[ "$guest_sha" != "$binary_sha" ]]; then
    echo "FAIL: synced binary hash mismatch host=$binary_sha guest=$guest_sha" >&2
    return 1
  fi

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
# The fresh host G4 binary is synced into this disposable guest and hash-checked before this hop.
# Compilation is host-safe; all offensive execution below remains guest-only.
test -x target/release/anubis
export ANUBIS_VZ_GUEST=1
export ANUBIS_OFFENSIVE_GATE_IN_GUEST=1
export ANUBIS_ISOLATION=tart-disposable-guest
export ANUBIS_BIN=target/release/anubis
# Never inherit host force-witness into the guest battery.
unset ANUBIS_OFFENSIVE_FORCE_ISOLATION_WITNESS || true
touch "$HOME/.anubis-vz-guest" 2>/dev/null || true
ANUBIS_OFFENSIVE_GATE_IN_GUEST=1 bash scripts/run_offensive_platform_gate.sh --out out/offensive_gate_guest
REMOTE
  rc=$?
  set -e

  cat "$log_path"

  set +e
  RSYNC_RSH="ssh ${sshopts[*]}" rsync -aH --no-devices --no-specials \
    "${user_}@${ip}:anubis-lang/${guest_out%/}/" "$out/"
  pull_rc=$?
  set -e

  # isolation.json written now; teardown_status finalized after explicit cleanup below.
  python3 - "$iso_path" "$base" "$guest" "$ip" "$cpu" "$mem" "$log_path" "$guest_out" "$binary_sha" "$ROOT" <<'PY'
import json, sys, subprocess, os
iso_path, base, guest, ip, cpu, mem, log_path, guest_out, binary_sha, root = sys.argv[1:11]

def git(*args):
    try:
        return subprocess.check_output(["git", "-C", root, *args], text=True, stderr=subprocess.DEVNULL).strip()
    except Exception:
        return ""

data = {
    "isolation": "tart-disposable-guest",
    "mode": "tart-disposable-guest",
    "base": base,
    "guest": guest,
    "ip": ip,
    "cpu": int(cpu),
    "memory_mib": int(mem),
    "guest_log": os.path.basename(log_path),
    "guest_out": guest_out,
    "binary_transport": "host-built-arm64-hash-verified",
    "binary_sha256": binary_sha,
    "git_head": git("rev-parse", "HEAD"),
    "git_tree": git("rev-parse", "HEAD^{tree}"),
    "git_dirty": bool(git("status", "--porcelain")),
    "teardown_status": "pending_exit_trap",
}
with open(iso_path, "w") as f:
    json.dump(data, f, indent=2)
    f.write("\n")
PY

  if [[ $pull_rc -ne 0 ]]; then
    echo "FAIL: could not collect guest output from ${user_}@${ip}:${guest_out}" >&2
    cleanup_offensive_guest
    trap - EXIT
    return 1
  fi
  # Tear down before return so report/isolation capture final teardown_status.
  cleanup_offensive_guest
  trap - EXIT
  local teardown_final
  teardown_final="$(cat "$teardown_file" 2>/dev/null || echo unknown)"
  if [[ -f "$iso_path" ]]; then
    python3 - "$iso_path" "$teardown_final" <<'PY'
import json, sys
iso_path, status = sys.argv[1:3]
d = json.load(open(iso_path))
d["teardown_status"] = status
with open(iso_path, "w") as f:
    json.dump(d, f, indent=2)
    f.write("\n")
PY
  fi
  if [[ -f "$out/report.json" ]]; then
    gate_augment_report_json "$out/report.json" "$host_bin" "$teardown_final" "tart-disposable-guest" >/dev/null
  fi
  # Guest battery must prove 34/34 tart-disposable-guest in pulled report.
  if [[ $rc -eq 0 ]]; then
    if ! gate_validate_offensive_report "$out/report.json" "tart-disposable-guest" "$OFFENSIVE_EXPECTED_GUEST_TOTAL"; then
      return 1
    fi
  fi
  return "$rc"
}

run_local_gate() {
  local out="$1"
  mkdir -p "$out"

  # Guest marker: offensive execution is forbidden on bare host.
  # When this function runs under ANUBIS_OFFENSIVE_GATE_IN_GUEST=1 (tart guest
  # hop or explicit lab), export the full isolation contract.
  if [[ "${ANUBIS_OFFENSIVE_GATE_IN_GUEST:-0}" == "1" ]]; then
    export ANUBIS_VZ_GUEST=1
    export ANUBIS_ISOLATION="${ANUBIS_ISOLATION:-tart-disposable-guest}"
    touch "${HOME}/.anubis-vz-guest" 2>/dev/null || true
  fi

  local bin=""
  bin="$(resolve_anubis_bin)" || return 1

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
  # Intentional field edits require content_hash re-seal (fail-closed integrity).
  "$bin" engage-rehash --dir "$eng" >/dev/null

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

  # Live inject under double authorization (pid 0 = lab victim loader).
  python3 - <<PY
import json
p="$eng/engagement.json"
d=json.load(open(p))
d["allow_live_inject"]=True
open(p,"w").write(json.dumps(d,indent=2))
PY
  "$bin" engage-rehash --dir "$eng" >/dev/null
  set +e
  "$bin" inject-plan --engage "$eng" --pid 0 --shellcode "$out/sc.bin" --allow-research-inject >"$out/inject_live.json" 2>&1
  local ilrc=$?
  set -e
  if [[ $ilrc -eq 0 ]] && grep -q EXECUTED "$out/inject_live.json" && grep -q '"executed": true' "$out/inject_live.json"; then
    record "t2_inject_live_double_auth" "PASS" "live under double auth"
  else
    record "t2_inject_live_double_auth" "FAIL" "rc=$ilrc $(head -c 160 "$out/inject_live.json" | tr '\n' ' ')"
  fi
  # restore default inject gate
  python3 - <<PY
import json
p="$eng/engagement.json"
d=json.load(open(p))
d["allow_live_inject"]=False
open(p,"w").write(json.dumps(d,indent=2))
PY
  "$bin" engage-rehash --dir "$eng" >/dev/null

  # Both branches used to record PASS, so this check was structurally incapable of failing and the
  # headline `34/34` verified at most 33 things. Its own else-branch detail string admitted the
  # listener lifecycle had already ended — i.e. it was a SKIP, booked as a pass. The adjacent
  # t3_dns check is the control: it records FAIL in its else branch, so the shape was known-good
  # ten lines away.
  if [[ -S "$eng/aop.sock" ]] || grep -q 'uds listener' "$out/listen.log"; then
    record "t3_uds" "PASS" "uds transport"
  else
    record "t3_uds" "FAIL" "no uds socket at $eng/aop.sock and no 'uds listener' in listen.log"
  fi
  if grep -q 'dns' "$out/listen.log"; then
    record "t3_dns" "PASS" "dns transport attempted"
  else
    record "t3_dns" "FAIL" "no dns"
  fi

  # DNS/DoH codec (HTTP remains default; DoH is an HTTP path on the same listener).
  # Restart is not required — the prior listen cycle already exercised multi-transport.
  # Probe codec via a fresh short-lived listener if port free; otherwise mark from doctor.
  set +e
  "$bin" engage-init --dir "$out/eng_doh" --name dohgate --authorization gate-doh >"$out/doh_init.log" 2>&1
  python3 - <<PY
import json
p="$out/eng_doh/engagement.json"
d=json.load(open(p))
d["c2_bind"]="127.0.0.1:14446"
d["dns_bind"]="127.0.0.1:55354"
d["uds_path"]="$out/eng_doh/aop.sock"
open(p,"w").write(json.dumps(d,indent=2))
PY
  "$bin" engage-rehash --dir "$out/eng_doh" >/dev/null
  pkill -f 'anubis listen' 2>/dev/null || true
  sleep 0.3
  "$bin" listen --engage "$out/eng_doh" >"$out/doh_listen.log" 2>&1 &
  local dpid=$!
  sleep 1.0
  local doh_res
  doh_res="$(curl -s -X POST http://127.0.0.1:14446/doh -H 'Content-Type: application/json' -d '{"qname":"0.1.p.x.aop.c2"}' 2>/dev/null || echo '{}')"
  local health_doh
  health_doh="$(curl -s http://127.0.0.1:14446/health 2>/dev/null || echo '{}')"
  kill "$dpid" 2>/dev/null
  wait 2>/dev/null
  set -e
  if echo "$doh_res" | grep -q '"ok":true' && echo "$health_doh" | grep -q aop-dns-v1; then
    record "t3_dns_doh_codec" "PASS" "DoH + dns codec"
  else
    record "t3_dns_doh_codec" "FAIL" "doh=$doh_res health=$health_doh"
  fi

  # Multi-operator token auth
  set +e
  "$bin" operator-token-issue --engage "$eng" --operator operator --json >"$out/tok.json" 2>&1
  local trc=$?
  set -e
  if [[ $trc -eq 0 ]] && grep -q token "$out/tok.json"; then
    local tok
    tok="$(python3 -c 'import json;print(json.load(open("'"$out"'/tok.json"))["token"])')"
    set +e
    "$bin" task-queue --engage "$eng" --module whoami --operator operator >"$out/tok_deny.log" 2>&1
    local td=$?
    "$bin" task-queue --engage "$eng" --module whoami --operator operator --token "$tok" >"$out/tok_ok.log" 2>&1
    local to=$?
    set -e
    if [[ $td -ne 0 ]] && grep -qi TOKEN "$out/tok_deny.log" && [[ $to -eq 0 ]]; then
      record "t7_operator_token_auth" "PASS" "deny without / allow with token"
    else
      record "t7_operator_token_auth" "FAIL" "deny_rc=$td ok_rc=$to"
    fi
  else
    record "t7_operator_token_auth" "FAIL" "issue rc=$trc"
  fi

  # mTLS opt-in handshake (HTTP remains default path above)
  set +e
  python3 - <<PY
import json
p="$out/eng_doh/engagement.json"
d=json.load(open(p))
d["c2_bind"]="127.0.0.1:14447"
open(p,"w").write(json.dumps(d,indent=2))
PY
  "$bin" engage-rehash --dir "$out/eng_doh" >/dev/null
  pkill -f 'anubis listen' 2>/dev/null || true
  sleep 0.3
  "$bin" listen --engage "$out/eng_doh" --mtls >"$out/mtls_listen.log" 2>&1 &
  local mpid=$!
  sleep 1.2
  python3 - <<PY >"$out/mtls.json" 2>"$out/mtls.err"
import json, ssl, urllib.request
ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
ctx.check_hostname = False
ctx.load_verify_locations("$out/eng_doh/certs/ca.crt.pem")
ctx.load_cert_chain("$out/eng_doh/certs/client.crt.pem", "$out/eng_doh/certs/client.key.pem")
r = urllib.request.urlopen("https://127.0.0.1:14447/health", context=ctx, timeout=5)
body = r.read().decode()
print(json.dumps({"mtls_ok": "ok" in body and "true" in body}))
PY
  local mrc=$?
  kill "$mpid" 2>/dev/null
  wait 2>/dev/null
  set -e
  if [[ $mrc -eq 0 ]] && grep -q '"mtls_ok": true' "$out/mtls.json"; then
    record "t1_mtls_rustls" "PASS" "rustls mTLS handshake"
  else
    record "t1_mtls_rustls" "FAIL" "rc=$mrc $(head -c 120 "$out/mtls.err" | tr '\n' ' ')"
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

  # ── T9 elite control plane ──
  set +e
  "$bin" attck-catalog --json >"$out/attck.json" 2>&1
  local ac=$?
  set -e
  if [[ $ac -eq 0 ]] && grep -q aop-attck-v1 "$out/attck.json" && grep -q T1071 "$out/attck.json"; then
    record "t9_attck_catalog" "PASS" "kill-chain catalog"
  else
    record "t9_attck_catalog" "FAIL" "rc=$ac"
  fi

  set +e
  "$bin" opsec-score --engage "$eng" --json >"$out/opsec.json" 2>&1
  local oc=$?
  set -e
  if [[ $oc -eq 0 ]] && grep -q aop-opsec-v1 "$out/opsec.json" && grep -q grade "$out/opsec.json"; then
    record "t9_opsec_score" "PASS" "opsec scored"
  else
    record "t9_opsec_score" "FAIL" "rc=$oc"
  fi

  set +e
  "$bin" malleable-init --engage "$eng" --name gate_profile >"$out/mall.log" 2>&1
  local mc=$?
  set -e
  if [[ $mc -eq 0 ]] && ls "$eng/profiles"/*.json >/dev/null 2>&1; then
    record "t9_malleable" "PASS" "profile written"
  else
    record "t9_malleable" "FAIL" "rc=$mc"
  fi

  set +e
  "$bin" campaign-init --engage "$eng" >"$out/camp.log" 2>&1
  local cc=$?
  set -e
  if [[ $cc -eq 0 && -f "$eng/campaigns/full_spectrum.json" && -f "$eng/campaigns/full_spectrum.md" ]]; then
    record "t9_campaign" "PASS" "playbook json+md"
  else
    record "t9_campaign" "FAIL" "rc=$cc"
  fi

  set +e
  "$bin" phish-plan --engage "$eng" --theme password_reset >"$out/phish.json" 2>&1
  local pc=$?
  set -e
  if [[ $pc -eq 0 ]] && grep -q PLAN_ONLY "$out/phish.json" && grep -q '"executed": false' "$out/phish.json"; then
    record "t9_phish_plan" "PASS" "plan-only never sends"
  else
    record "t9_phish_plan" "FAIL" "rc=$pc"
  fi

  set +e
  "$bin" lolbas-catalog --json >"$out/lolbas.json" 2>&1
  local lc=$?
  set -e
  if [[ $lc -eq 0 ]] && grep -q PLAN_ONLY "$out/lolbas.json" && grep -q T1218 "$out/lolbas.json"; then
    record "t9_lolbas" "PASS" "catalog plan-only"
  else
    record "t9_lolbas" "FAIL" "rc=$lc"
  fi

  set +e
  "$bin" purple-report --engage "$eng" --out "$eng/loot/purple" --json >"$out/purple.json" 2>&1
  local prc=$?
  set -e
  if [[ $prc -eq 0 ]] && grep -q aop-purple-v1 "$out/purple.json" && [[ -f "$eng/loot/purple/purple_report.md" ]]; then
    record "t9_purple_report" "PASS" "coverage + gaps"
  else
    record "t9_purple_report" "FAIL" "rc=$prc"
  fi

  set +e
  "$bin" recon-hostinfo --engage "$eng" >"$out/recon_hi.json" 2>&1
  local rh=$?
  set -e
  if [[ $rh -eq 0 ]] && grep -q aop-recon-v1 "$out/recon_hi.json"; then
    record "t9_recon_hostinfo" "PASS" "scope facts"
  else
    record "t9_recon_hostinfo" "FAIL" "rc=$rh"
  fi

  set +e
  "$bin" recon-scan --engage "$eng" --host 127.0.0.1 --ports 22,80 >"$out/recon_scan.json" 2>&1
  local rs=$?
  set -e
  if [[ $rs -eq 0 ]] && grep -q open_ports "$out/recon_scan.json"; then
    record "t9_recon_scan" "PASS" "scoped scan"
  else
    # Host path of this gate hop may still be VZ-marked when IN_GUEST=1
    if [[ $rs -ne 0 ]] && grep -q OFFENSIVE_HOST_FORBIDDEN "$out/recon_scan.json" 2>/dev/null; then
      record "t9_recon_scan" "FAIL" "host forbid unexpected under guest markers"
    else
      record "t9_recon_scan" "FAIL" "rc=$rs $(head -c 120 "$out/recon_scan.json" | tr '\n' ' ')"
    fi
  fi

  set +e
  "$bin" offensive-doctor --json >"$out/doctor_t9.json" 2>&1
  set -e
  if grep -q attck_kill_chain_catalog "$out/doctor_t9.json" \
    && grep -q purple_team_report "$out/doctor_t9.json" \
    && grep -q malleable_c2_profile "$out/doctor_t9.json"; then
    record "t9_doctor_surfaces" "PASS" "T9 surfaces present (LAB_REAL/PLAN_ONLY)"
  else
    record "t9_doctor_surfaces" "FAIL" "missing T9 surfaces"
  fi

  local isolation="tart-disposable-guest"
  if [[ "${ANUBIS_OFFENSIVE_GATE_IN_GUEST:-0}" != "1" ]]; then
    # run_local_gate is only for the in-guest hop; bare-host misuse must not claim 34/34.
    isolation="host-misuse"
  fi
  local verdict="FAIL"
  if [[ "$isolation" == "tart-disposable-guest" \
    && $fail -eq 0 \
    && $pass -eq $OFFENSIVE_EXPECTED_GUEST_TOTAL \
    && $total -eq $OFFENSIVE_EXPECTED_GUEST_TOTAL ]]; then
    verdict="PASS"
  fi
  python3 - <<PY
import json, subprocess, os, hashlib
out = r"""$out"""
bin_path = r"""$bin"""
root = r"""$ROOT"""

def git(*args):
    try:
        return subprocess.check_output(["git", "-C", root, *args], text=True, stderr=subprocess.DEVNULL).strip()
    except Exception:
        return ""

def sha256(path):
    try:
        h = hashlib.sha256()
        with open(path, "rb") as f:
            for chunk in iter(lambda: f.read(1024 * 1024), b""):
                h.update(chunk)
        return h.hexdigest()
    except Exception:
        return ""

report = {
  "total": $total,
  "passed": $pass,
  "failed": $fail,
  "overall_verdict": "$verdict",
  "binary": bin_path,
  "binary_sha256": sha256(bin_path) if bin_path else "",
  "isolation": "$isolation",
  "mode": "$isolation",
  "expected_total": $OFFENSIVE_EXPECTED_GUEST_TOTAL,
  "git_head": git("rev-parse", "HEAD"),
  "git_tree": git("rev-parse", "HEAD^{tree}"),
  "git_dirty": bool(git("status", "--porcelain")),
  "teardown_status": "in_guest_n_a",
}
print(json.dumps(report, indent=2))
open(os.path.join(out, "report.json"), "w").write(json.dumps(report, indent=2) + "\n")
PY
  # Coverage ratchet (adversary R49): guest battery total must not silently shrink.
  # Separate floor from host-witness (5) so the two modes do not collide.
  set +e
  assert_floor "offensive_platform_gate_guest" "$total" "$ROOT/scripts/floors/offensive_platform_gate_guest.count_floor"
  _floor_rc=$?
  set -e
  if [[ $_floor_rc -ne 0 ]]; then
    echo "FLOOR: FAIL ($total cases; $GATE_FLOOR_ERROR)" >&2
    verdict=FAIL
  fi
  echo "Overall: $verdict ($pass/$total) isolation=$isolation expected=$OFFENSIVE_EXPECTED_GUEST_TOTAL"
  [[ "$verdict" == PASS ]]
}

# Host isolation witness — the only honest G14 surface on tart-less machines
# (stock GitHub Actions macos-latest has no tart + no golden image).
# Proves AOP fail-closed on bare host WITHOUT running red-team payloads on host.
# This is NOT a substitute for the full tart disposable-guest battery (34/34);
# it is the isolation contract that CI can actually re-derive off-desk.
run_host_isolation_witness() {
  local out="$1"
  mkdir -p "$out"

  local bin=""
  bin="$(resolve_anubis_bin)" || return 1

  # Absolute: no guest markers on host witness path.
  rm -f "${HOME}/.anubis-vz-guest" 2>/dev/null || true
  unset ANUBIS_VZ_GUEST ANUBIS_OFFENSIVE_GATE_IN_GUEST ANUBIS_ISOLATION || true

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

  # Scaffolding only (no red-team execution): engage workspace for forbidden surfaces.
  if "$bin" engage-init --dir "$eng" --name iso-witness --authorization gate-charter >"$out/init.log" 2>&1 \
    && [[ -f "$eng/engagement.json" ]]; then
    record "iso_engage_init" "PASS" "workspace scaffold"
  else
    record "iso_engage_init" "FAIL" "engage-init $(tail -1 "$out/init.log" 2>/dev/null || true)"
  fi

  # Each offensive execution surface must refuse on bare host.
  local surface cmd rc log
  for surface in task-queue recon-scan lateral-smb; do
    log="$out/forbid_${surface//-/_}.log"
    set +e
    case "$surface" in
      task-queue)
        "$bin" task-queue --engage "$eng" --module whoami --operator operator >"$log" 2>&1
        rc=$?
        ;;
      recon-scan)
        "$bin" recon-scan --engage "$eng" --host 127.0.0.1 --ports 22 >"$log" 2>&1
        rc=$?
        ;;
      lateral-smb)
        "$bin" lateral-smb --engage "$eng" --host 127.0.0.1 >"$log" 2>&1
        rc=$?
        ;;
    esac
    set -e
    if [[ $rc -ne 0 ]] && grep -q 'ANUBIS_OFFENSIVE_HOST_FORBIDDEN' "$log"; then
      record "iso_forbid_${surface//-/_}" "PASS" "host-forbidden"
    else
      record "iso_forbid_${surface//-/_}" "FAIL" "rc=$rc (expected HOST_FORBIDDEN)"
    fi
  done

  # Guest marker must not remain after host witness (fail-open hygiene).
  if [[ ! -f "${HOME}/.anubis-vz-guest" ]]; then
    record "iso_no_stale_guest_marker" "PASS" "host clean"
  else
    record "iso_no_stale_guest_marker" "FAIL" "stale $HOME/.anubis-vz-guest"
    rm -f "${HOME}/.anubis-vz-guest" || true
  fi

  local isolation="host-isolation-witness"
  local verdict="FAIL"
  # Hosted CI requires exactly 5/5 witness — not "any pass>0".
  if [[ $fail -eq 0 \
    && $pass -eq $OFFENSIVE_EXPECTED_WITNESS_TOTAL \
    && $total -eq $OFFENSIVE_EXPECTED_WITNESS_TOTAL ]]; then
    verdict="PASS"
  fi
  python3 - <<PY
import json, subprocess, os, hashlib
out = r"""$out"""
bin_path = r"""$bin"""
root = r"""$ROOT"""

def git(*args):
    try:
        return subprocess.check_output(["git", "-C", root, *args], text=True, stderr=subprocess.DEVNULL).strip()
    except Exception:
        return ""

def sha256(path):
    try:
        h = hashlib.sha256()
        with open(path, "rb") as f:
            for chunk in iter(lambda: f.read(1024 * 1024), b""):
                h.update(chunk)
        return h.hexdigest()
    except Exception:
        return ""

report = {
  "total": $total,
  "passed": $pass,
  "failed": $fail,
  "overall_verdict": "$verdict",
  "binary": bin_path,
  "binary_sha256": sha256(bin_path) if bin_path else "",
  "isolation": "$isolation",
  "mode": "$isolation",
  "expected_total": $OFFENSIVE_EXPECTED_WITNESS_TOTAL,
  "git_head": git("rev-parse", "HEAD"),
  "git_tree": git("rev-parse", "HEAD^{tree}"),
  "git_dirty": bool(git("status", "--porcelain")),
  "teardown_status": "host_witness_n_a",
  "note": "Full tart disposable-guest battery requires tart+golden image; CI proves host fail-closed only (exactly 5/5)."
}
print(json.dumps(report, indent=2))
open(os.path.join(out, "report.json"), "w").write(json.dumps(report, indent=2) + "\n")
PY
  # Coverage ratchet (adversary R49): host-witness total must not silently shrink.
  set +e
  assert_floor "offensive_platform_gate_witness" "$total" "$ROOT/scripts/floors/offensive_platform_gate_witness.count_floor"
  _floor_rc=$?
  set -e
  if [[ $_floor_rc -ne 0 ]]; then
    echo "FLOOR: FAIL ($total cases; $GATE_FLOOR_ERROR)" >&2
    verdict=FAIL
  fi
  echo "Overall: $verdict ($pass/$total) isolation=$isolation expected=$OFFENSIVE_EXPECTED_WITNESS_TOTAL"
  echo "G14_MODE: host-isolation-witness (exactly ${OFFENSIVE_EXPECTED_WITNESS_TOTAL}/${OFFENSIVE_EXPECTED_WITNESS_TOTAL} required)"
  [[ "$verdict" == PASS ]]
}

OUT="$(parse_args "$@")"

# Host hygiene: a leftover `$HOME/.anubis-vz-guest` on the *host* (e.g. from an
# accidental IN_GUEST=1 local run) makes `in_vz_guest()` true and fail-opens AOP
# on bare metal. Only guest hops may create that marker; host entrypoint strips it.
if [[ "${ANUBIS_OFFENSIVE_GATE_IN_GUEST:-0}" != "1" ]]; then
  if [[ -f "${HOME}/.anubis-vz-guest" ]]; then
    echo "[offensive-gate] removing stale host guest marker ${HOME}/.anubis-vz-guest (isolation fail-open)" >&2
    rm -f "${HOME}/.anubis-vz-guest"
  fi
  unset ANUBIS_VZ_GUEST ANUBIS_OFFENSIVE_GATE_IN_GUEST || true
  # Keep ANUBIS_ISOLATION only if it does not claim guest membership on host.
  if [[ -n "${ANUBIS_ISOLATION:-}" ]]; then
    case "${ANUBIS_ISOLATION}" in
      *tart*|*vz*|*virtualization*)
        echo "[offensive-gate] clearing host ANUBIS_ISOLATION=${ANUBIS_ISOLATION} (guest-claiming)" >&2
        unset ANUBIS_ISOLATION
        ;;
    esac
  fi
fi

if [[ "${ANUBIS_OFFENSIVE_GATE_IN_GUEST:-0}" == "1" ]]; then
  run_local_gate "$OUT"
elif [[ "${ANUBIS_OFFENSIVE_FORCE_ISOLATION_WITNESS:-0}" == "1" ]]; then
  echo "[offensive-gate] ANUBIS_OFFENSIVE_FORCE_ISOLATION_WITNESS=1 — host isolation witness only (exactly ${OFFENSIVE_EXPECTED_WITNESS_TOTAL}/${OFFENSIVE_EXPECTED_WITNESS_TOTAL})"
  run_host_isolation_witness "$OUT"
  # Fail-closed re-check of report shape for hosted consumers.
  gate_validate_offensive_report "$OUT/report.json" "host-isolation-witness" "$OFFENSIVE_EXPECTED_WITNESS_TOTAL"
else
  # Full / default path: guest battery only. Do NOT inherit ambient force-witness
  # and do NOT soft-downgrade prereq miss to the 5/5 host witness.
  unset ANUBIS_OFFENSIVE_FORCE_ISOLATION_WITNESS || true
  set +e
  run_in_guest "$OUT"
  guest_rc=$?
  set -e
  # Finalize isolation.json teardown_status after EXIT trap would have run...
  # Trap still fires on exit; refresh isolation.json from teardown file if present.
  if [[ -f "$OUT/teardown_status.txt" && -f "$OUT/isolation.json" ]]; then
    python3 - "$OUT/isolation.json" "$OUT/teardown_status.txt" <<'PY'
import json, sys
iso_path, tf = sys.argv[1:3]
try:
    d = json.load(open(iso_path))
except Exception:
    raise SystemExit(0)
try:
    d["teardown_status"] = open(tf).read().strip() or "unknown"
except Exception:
    d["teardown_status"] = "unknown"
with open(iso_path, "w") as f:
    json.dump(d, f, indent=2)
    f.write("\n")
PY
  fi
  if [[ $guest_rc -eq 0 ]]; then
    # Double-check 34/34 tart-disposable-guest before claiming green.
    gate_validate_offensive_report "$OUT/report.json" "tart-disposable-guest" "$OFFENSIVE_EXPECTED_GUEST_TOTAL"
    exit 0
  elif [[ $guest_rc -eq 2 ]]; then
    echo "FAIL: tart guest prereqs missing — full G14 requires disposable guest battery (${OFFENSIVE_EXPECTED_GUEST_TOTAL}/${OFFENSIVE_EXPECTED_GUEST_TOTAL}); host witness is hosted-only (set ANUBIS_OFFENSIVE_FORCE_ISOLATION_WITNESS=1)" >&2
    exit 2
  else
    # Guest launched but battery failed — do not paper over with isolation witness.
    exit "$guest_rc"
  fi
fi
