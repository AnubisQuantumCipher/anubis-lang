#!/usr/bin/env bash
# Resource controls for ordinary Linux CI only. This is not a VM/isolation seal.
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
out="$repo/out/linux-native"
mkdir -p "$out"
unit="anubis-linux-native-${GITHUB_RUN_ID:?}-${GITHUB_RUN_ATTEMPT:?}-${RUNNER_ARCH:?}"
work="${RUNNER_TEMP:?}/$unit"
owned=false
launch_rc=not-started

finish() {
  shell_rc=$?
  trap - EXIT INT TERM
  set +e
  state="$(systemctl show "$unit.service" --property=LoadState --value 2>>"$out/teardown.stderr.log")"
  query_rc=$?
  cleanup=false
  stop_rc=not-needed
  # Cancellation does not automatically stop an independent transient service.
  # Only clean the exact unit admitted as absent before our launch.
  if [[ "$owned" == true && ( "$query_rc" != 0 || "$state" != not-found ) ]]; then
    cleanup=true
    timeout 30s sudo systemctl stop "$unit.service" >>"$out/teardown.stdout.log" 2>>"$out/teardown.stderr.log"
    stop_rc=$?
    if [[ "$stop_rc" != 0 ]]; then
      timeout 15s sudo systemctl kill --kill-whom=all --signal=KILL "$unit.service" \
        >>"$out/teardown.stdout.log" 2>>"$out/teardown.stderr.log"
    fi
    state="$(systemctl show "$unit.service" --property=LoadState --value 2>>"$out/teardown.stderr.log")"
    query_rc=$?
  fi
  python3 - "$out/launcher.json" "$shell_rc" "$launch_rc" "$unit" "$state" "$query_rc" "$cleanup" "$stop_rc" <<'PY'
import json, pathlib, sys
pathlib.Path(sys.argv[1]).write_text(json.dumps({
    "exit_code": int(sys.argv[2]), "launch_exit_code": sys.argv[3],
    "unit": sys.argv[4] + ".service", "load_state_after": sys.argv[5],
    "query_exit_code": int(sys.argv[6]), "cleanup_required": sys.argv[7] == "true",
    "stop_exit_code": sys.argv[8],
    "unit_removed": sys.argv[5] == "not-found" and sys.argv[6] == "0",
}, indent=2) + "\n")
if sys.argv[2] != "0" or sys.argv[3] != "0" or sys.argv[5] != "not-found" or sys.argv[6] != "0" or sys.argv[7] != "false":
    receipt_path = pathlib.Path(sys.argv[1]).with_name("receipt.json")
    if receipt_path.is_file():
        receipt = json.loads(receipt_path.read_text())
        receipt.update(verdict="FAIL", error="launcher or teardown failed; inspect launcher.json and logs")
        receipt_path.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
PY
  record_rc=$?
  [[ ! -f "$out/launcher.stdout.log" ]] || cat "$out/launcher.stdout.log"
  [[ ! -f "$out/launcher.stderr.log" ]] || cat "$out/launcher.stderr.log" >&2
  if [[ "$shell_rc" != 0 || "$launch_rc" != 0 || "$record_rc" != 0 || "$query_rc" != 0 \
        || "$state" != not-found || "$cleanup" != false ]]; then
    exit 1
  fi
  python3 "$repo/scripts/ci/linux_native.py" finalize --out="$out" --sha="$GITHUB_SHA" --arch="$1" \
    >"$out/validation.stdout.log" 2>"$out/validation.stderr.log"
  exit $?
}
expected_arch="${1:?expected native architecture}"
trap 'finish "$expected_arch"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
[[ ! -e "$work" ]] || { echo 'owned work directory is not fresh' >&2; exit 1; }
mkdir -p "$work"
initial="$(systemctl show "$unit.service" --property=LoadState --value 2>"$out/admission.stderr.log")"
[[ "$initial" == not-found ]] || { echo 'refusing a pre-existing service' >&2; exit 1; }
owned=true
set +e
sudo systemd-run --wait --pipe --collect --service-type=exec \
  --unit="$unit" --uid="$(id -u)" --gid="$(id -g)" \
  --property="WorkingDirectory=$repo" --property=MemoryMax=6000000000 \
  --property=MemorySwapMax=0 --property=CPUQuota=200% --property=TasksMax=512 \
  --property=RuntimeMaxSec=5400 --property=TimeoutStopSec=15 \
  --property=KillMode=control-group --property=LimitCORE=0 \
  --setenv="PATH=$PATH" --setenv="HOME=$HOME" --setenv="CARGO_HOME=${CARGO_HOME:-$HOME/.cargo}" \
  --setenv="RUSTUP_HOME=${RUSTUP_HOME:-$HOME/.rustup}" \
  --setenv="ImageOS=${ImageOS:-unknown}" --setenv="ImageVersion=${ImageVersion:-unknown}" \
  python3 "$repo/scripts/ci/linux_native.py" run \
  --arch="$expected_arch" --sha="${GITHUB_SHA:?}" \
  --unit="$unit.service" --work="$work" --out="$out" \
  >"$out/launcher.stdout.log" 2>"$out/launcher.stderr.log"
launch_rc=$?
exit "$launch_rc"
