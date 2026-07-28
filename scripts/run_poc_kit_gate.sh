#!/usr/bin/env bash
# Bounty-grade PoC kit gate: packing + real local crash PoC + process fuzz.
# Host entry is an orchestrator only: the crash-capable battery runs inside a disposable Tart/VZ
# clone of anubis-xcode. Missing VZ prerequisites fail closed; there is no host fallback.
# Fail-closed: missing target / no crash / no unique fuzz crash / no VZ => FAIL.
# Network negative requires ANUBIS_POC_NETWORK_FORBIDDEN (not any nonzero exit).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
# shellcheck source=lib/gate_evidence.sh
GATE_EVIDENCE_ROOT="$ROOT"
# shellcheck disable=SC1091
source "$ROOT/scripts/lib/gate_evidence.sh"
source "$ROOT/scripts/lib/gate_common.sh"
OUT="${1:-out/poc_kit}"
if [[ "${1:-}" == "--out" && -n "${2:-}" ]]; then
  OUT="$2"
fi
mkdir -p "$OUT"

run_in_disposable_guest() {
  local out="$1"
  local base="${ANUBIS_VM_BASE:-anubis-xcode}"
  local key="${ANUBIS_VM_KEY:-$HOME/.ssh/tart_anubis}"
  local user_="${ANUBIS_VM_USER:-admin}"
  local guest="anubis-poc-kit-gate-$$"
  local guest_out="out/poc_kit_guest"
  local host_bin="$ROOT/target/release/anubis"
  local binary_sha=""
  local guest_sha_line=""
  local guest_sha=""
  local ip=""
  local rc=0
  local pull_rc=0
  local -a sshopts=(
    -i "$key"
    -o StrictHostKeyChecking=no
    -o UserKnownHostsFile=/dev/null
    -o ConnectTimeout=15
    -o LogLevel=ERROR
  )

  bash tools/grok-safety-check.sh "$out"
  rm -rf "$out"
  mkdir -p "$out"

  command -v tart >/dev/null 2>&1 || {
    echo "ANUBIS_POC_KIT_VZ_REQUIRED: tart is not installed" >&2
    return 1
  }
  tart list 2>/dev/null | awk '{print $2}' | grep -qx "$base" || {
    echo "ANUBIS_POC_KIT_VZ_REQUIRED: golden image '$base' is missing" >&2
    return 1
  }
  [[ -f "$key" ]] || {
    echo "ANUBIS_POC_KIT_VZ_REQUIRED: SSH key is missing at $key" >&2
    return 1
  }
  [[ -x "$host_bin" ]] || {
    echo "ANUBIS_POC_KIT_HOST_BINARY_REQUIRED: build target/release/anubis first" >&2
    return 1
  }
  binary_sha="$(gate_sha256_file "$host_bin")"
  local teardown_file="$out/teardown_status.txt"
  echo "not_started" >"$teardown_file"

  # Guest name must be captured for EXIT trap: locals are out of scope when the
  # trap fires after the function returns (set -u → "guest: unbound variable").
  POC_KIT_GUEST="$guest"
  POC_KIT_TEARDOWN_FILE="$teardown_file"
  cleanup_poc_guest() {
    local g="${POC_KIT_GUEST:-}"
    local tf="${POC_KIT_TEARDOWN_FILE:-}"
    if [[ -z "$g" ]]; then
      [[ -n "$tf" ]] && echo "no_guest" >"$tf"
      return 0
    fi
    local stop_rc=0 del_rc=0
    tart stop "$g" >/dev/null 2>&1 || stop_rc=$?
    tart delete "$g" >/dev/null 2>&1 || del_rc=$?
    if [[ $stop_rc -eq 0 && $del_rc -eq 0 ]]; then
      [[ -n "$tf" ]] && echo "torn_down" >"$tf"
    else
      [[ -n "$tf" ]] && echo "teardown_failed" >"$tf"
    fi
    POC_KIT_GUEST=""
  }
  trap cleanup_poc_guest EXIT

  echo "[poc-kit] isolation=tart-disposable-guest base=$base guest=$guest"
  tart clone "$base" "$guest" >/dev/null
  tart set "$guest" --cpu 4 --memory 8192 >/dev/null
  tart run "$guest" --no-graphics >/dev/null 2>&1 &

  for _ in $(seq 1 75); do
    ip="$(tart ip "$guest" 2>/dev/null || true)"
    if [[ -n "$ip" ]] && nc -z -w 3 "$ip" 22 2>/dev/null; then
      break
    fi
    sleep 4
  done
  [[ -n "$ip" ]] || {
    echo "ANUBIS_POC_KIT_VZ_REQUIRED: guest never reached SSH" >&2
    return 1
  }

  # --delete: guest tree matches host working tree (safe: disposable clone, unique name).
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
  [[ "$guest_sha" == "$binary_sha" ]] || {
    echo "ANUBIS_POC_KIT_BINARY_MISMATCH: host=$binary_sha guest=$guest_sha" >&2
    return 1
  }

  set +e
  ssh "${sshopts[@]}" "${user_}@${ip}" 'bash -s' >"$out/guest_stdout.log" 2>&1 <<'REMOTE'
set -euo pipefail
. "$HOME/.cargo/env" 2>/dev/null || true
export PATH=/opt/homebrew/opt/coreutils/libexec/gnubin:/opt/homebrew/bin:$PATH
export CARGO_BUILD_JOBS="${ANUBIS_POC_KIT_BUILD_JOBS:-4}"
export CARGO_INCREMENTAL=0
export RUST_MIN_STACK=67108864
cd "$HOME/anubis-lang"
# The host G4 binary is synced into this disposable guest and hash-checked before this hop.
# Compilation is host-safe; all research/crash-capable execution below remains guest-only.
test -x target/release/anubis
export ANUBIS_VZ_GUEST=1
export ANUBIS_OFFENSIVE_GATE_IN_GUEST=1
export ANUBIS_POC_KIT_IN_GUEST=1
export ANUBIS_ISOLATION=tart-disposable-guest
touch "$HOME/.anubis-vz-guest" 2>/dev/null || true
bash scripts/run_poc_kit_gate.sh --out out/poc_kit_guest
REMOTE
  rc=$?
  set -e

  cat "$out/guest_stdout.log"
  set +e
  RSYNC_RSH="ssh ${sshopts[*]}" rsync -aH --no-devices --no-specials \
    "${user_}@${ip}:anubis-lang/${guest_out%/}/" "$out/"
  pull_rc=$?
  set -e

  cleanup_poc_guest
  trap - EXIT
  local teardown_final
  teardown_final="$(cat "$teardown_file" 2>/dev/null || echo unknown)"

  python3 - "$out/isolation.json" "$base" "$guest" "$ip" "$guest_out" "$binary_sha" "$ROOT" "$teardown_final" <<'PY'
import json, sys, subprocess, os
iso_path, base, guest, ip, guest_out, binary_sha, root, teardown = sys.argv[1:9]

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
    "guest_out": guest_out,
    "binary_transport": "host-built-arm64-hash-verified",
    "binary_sha256": binary_sha,
    "git_head": git("rev-parse", "HEAD"),
    "git_tree": git("rev-parse", "HEAD^{tree}"),
    "git_dirty": bool(git("status", "--porcelain")),
    "teardown_status": teardown,
    "rsync_delete": True,
}
with open(iso_path, "w") as f:
    json.dump(data, f, indent=2)
    f.write("\n")
PY

  if [[ -f "$out/report.json" ]]; then
    gate_augment_report_json "$out/report.json" "$host_bin" "$teardown_final" "tart-disposable-guest" >/dev/null
  fi

  if [[ $pull_rc -ne 0 ]]; then
    echo "FAIL: could not collect PoC-kit evidence from disposable guest" >&2
    return 1
  fi
  return "$rc"
}

if [[ "${ANUBIS_POC_KIT_IN_GUEST:-0}" != "1" ]]; then
  run_in_disposable_guest "$OUT"
  exit $?
fi

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
fixtures_jsonl="$OUT/fixtures.jsonl"
: >"$fixtures_jsonl"

record() {
  local name="$1" status="$2" detail="$3"
  total=$((total+1))
  if [[ "$status" == "PASS" ]]; then pass=$((pass+1)); else fail=$((fail+1)); fi
  printf '%-28s %s  (%s)\n' "$name" "$status" "$detail"
  python3 -c 'import json,sys; print(json.dumps({"name":sys.argv[1],"status":sys.argv[2],"detail":sys.argv[3]}))' \
    "$name" "$status" "$detail" >>"$fixtures_jsonl"
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

# 4) network target rejected — must emit ANUBIS_POC_NETWORK_FORBIDDEN (not any nonzero).
set +e
"$BIN" fuzz --target "https://evil.example/bin" --runs 1 --out "$OUT/fuzz_net" >"$OUT/fuzz_net.log" 2>&1
nrc=$?
set -e
if [[ $nrc -ne 0 ]] && grep -q 'ANUBIS_POC_NETWORK_FORBIDDEN' "$OUT/fuzz_net.log" 2>/dev/null; then
  record "network_forbidden" "PASS" "ANUBIS_POC_NETWORK_FORBIDDEN"
else
  record "network_forbidden" "FAIL" "exit=$nrc missing ANUBIS_POC_NETWORK_FORBIDDEN"
fi

verdict="FAIL"
[[ $fail -eq 0 && $pass -gt 0 ]] && verdict="PASS"
# Isolation honesty: the crash-capable local branch is reachable only through the wrapper above.
iso_label="tart-disposable-guest"
python3 - "$report" "$fixtures_jsonl" "$BIN" "$ROOT" "$total" "$pass" "$fail" "$verdict" "$iso_label" <<'PY'
import json, sys, subprocess, os, hashlib
report_path, fixtures_path, bin_path, root, total, passed, failed, verdict, iso = sys.argv[1:10]

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

fixtures = []
if os.path.isfile(fixtures_path):
    with open(fixtures_path) as f:
        for line in f:
            line = line.strip()
            if line:
                fixtures.append(json.loads(line))

data = {
    "binary": bin_path,
    "binary_sha256": sha256(bin_path),
    "fixtures": fixtures,
    "total": int(total),
    "passed": int(passed),
    "failed": int(failed),
    "isolation": iso,
    "mode": iso,
    "execution_boundary": "mandatory disposable anubis-xcode guest; no host fallback",
    "overall_verdict": verdict,
    "git_head": git("rev-parse", "HEAD"),
    "git_tree": git("rev-parse", "HEAD^{tree}"),
    "git_dirty": bool(git("status", "--porcelain")),
    "teardown_status": "in_guest_n_a",
}
with open(report_path, "w") as f:
    json.dump(data, f, indent=2)
    f.write("\n")
print(json.dumps(data, indent=2))
PY

echo "Report: $report"
# Coverage ratchet (adversary R49): case total must not silently shrink.
set +e
assert_floor "poc_kit_gate" "$total" "$ROOT/scripts/floors/poc_kit_gate.count_floor"
_floor_rc=$?
set -e
if [[ $_floor_rc -ne 0 ]]; then
  echo "FLOOR: FAIL ($total cases; $GATE_FLOOR_ERROR)" >&2
  verdict=FAIL
fi
echo "Overall: $verdict ($pass/$total)"
[[ "$verdict" == "PASS" ]]
