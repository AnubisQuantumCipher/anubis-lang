#!/usr/bin/env bash
# Regression tests for the macOS/Tart host resource guard.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
GUARD="$ROOT/scripts/lib/host_resource_guard.sh"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/anubis-host-resource-guard.XXXXXX")"
TMP="$(cd "$TMP" && pwd -P)"
trap 'rm -rf "$TMP"' EXIT

pass=0
fail=0
record() {
  local name="$1" ok="$2" detail="$3"
  if [[ "$ok" == 1 ]]; then
    pass=$((pass + 1)); printf 'PASS %-30s %s\n' "$name" "$detail"
  else
    fail=$((fail + 1)); printf 'FAIL %-30s %s\n' "$name" "$detail"
  fi
}

if [[ ! -f "$GUARD" ]]; then
  echo "FAIL missing guard: $GUARD" >&2
  exit 1
fi
# shellcheck disable=SC1090
source "$GUARD"
PRODUCTION_RSYNC_FUNCTION="$(declare -f anubis_guard_rsync)"
PRODUCTION_SSH_FUNCTION="$(declare -f anubis_guard_ssh)"

for tuple in "8 12288" "4 8192" "1 4096"; do
  set -- $tuple
  if anubis_guard_validate_vm_limits "$1" "$2" >/dev/null 2>&1; then ok=1; else ok=0; fi
  record "limits_accept_${1}_${2}" "$ok" "cpu=$1 mem=$2"
done

for tuple in \
  "9 12288" "8 12289" "0 8192" "x 8192" "8 0" "8 12GiB" \
  "08 8192" "8 08192" \
  "999999999999999999999999999999999999 8192" \
  "8 999999999999999999999999999999999999"; do
  set -- $tuple
  if anubis_guard_validate_vm_limits "$1" "$2" >/dev/null 2>&1; then ok=0; else ok=1; fi
  record "limits_reject_${1}_${2}" "$ok" "cpu=$1 mem=$2"
done

vm_fixture='Mach Virtual Memory Statistics: (page size of 16384 bytes)
Pages free:                               524288.
Pages active:                             100000.
Pages speculative:                        65536.'
free_mib="$(printf '%s\n' "$vm_fixture" | anubis_guard_free_mib_from_vm_stat)"
[[ "$free_mib" == 9216 ]] && ok=1 || ok=0
record free_parser "$ok" "immediately_free_mib=$free_mib"

# Repository sync into disposable guests copies the live source plus exactly the selected pin,
# removes host-only evidence/worktree paths, and preserves the warm guest target cache. Recursive
# deletion of that 48-GiB cache filled host memory until the breaker stopped the guest. Exercise the
# shared seam with fake rsync/SSH so this regression stays fast and cannot allocate a real VM.
mkdir -p "$TMP/sync-fakebin"
mkdir -p "$TMP/source/vm/pins"
mkdir -p "$TMP/source/scripts/lib"
cp "$ROOT/scripts/lib/pin_manifest.py" "$TMP/source/scripts/lib/"
printf '%s\n' \
  '{' \
  '  "schema": "anubis.pin-manifest-policy.v2",' \
  '  "roots": ["scripts", "vm"],' \
  '  "files": [],' \
  '  "excluded_top_level_entries": {' \
  '    ".claude": {"kind": "directory", "reason": "agent state"},' \
  '    ".env": {"kind": "file", "reason": "secret environment"},' \
  '    ".git": {"kind": "file_or_directory", "reason": "Git metadata"},' \
  '    "implementer": {"kind": "directory", "reason": "historical output"},' \
  '    "out": {"kind": "directory", "reason": "generated output"},' \
  '    "scratchpad": {"kind": "directory", "reason": "scratch output"},' \
  '    "secrets": {"kind": "directory", "reason": "secret material"},' \
  '    "target": {"kind": "directory", "reason": "build cache"}' \
  '  },' \
  '  "excluded_exact_directories": ["scripts/nested/generated", "vm/exports", "vm/pins"],' \
  '  "excluded_directory_names": [],' \
  '  "excluded_directory_names_under": {}' \
  '}' >"$TMP/source/scripts/lib/pin_manifest_policy.json"
printf '#!/usr/bin/env bash\nexit 0\n' >"$TMP/source/vm/pins/pin-under-test"
chmod 0555 "$TMP/source/vm/pins/pin-under-test"
PIN_SHA="$(shasum -a 256 "$TMP/source/vm/pins/pin-under-test" | awk '{print $1}')"
PIN_SRC_TREE=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
PIN_NAME="anubis-${PIN_SHA:0:12}-src-${PIN_SRC_TREE:0:12}-release"
mv "$TMP/source/vm/pins/pin-under-test" "$TMP/source/vm/pins/$PIN_NAME"
printf 'vm/pins/%s\n' "$PIN_NAME" >"$TMP/source/vm/pins/CURRENT"
printf '%s\n' \
  'pin_schema: anubis.binary-pin.v2' \
  "pin: vm/pins/$PIN_NAME" \
  "sha256: $PIN_SHA" \
  'source: fresh-exact-head-archive' \
  'build_mode: cargo-build-locked-release-exact-head-archive-clean-target' \
  'head: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' \
  'head_tree: cccccccccccccccccccccccccccccccccccccccc' \
  'commit_bound: true' \
  'manifest_schema: anubis.pin-source-manifest.v2' \
  'policy_sha256: dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd' \
  "src_tree: $PIN_SRC_TREE" \
  'src_count: 1' \
  'src_list_sha256: eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee' \
  >"$TMP/source/vm/pins/$PIN_NAME.meta"
chmod 0444 "$TMP/source/vm/pins/$PIN_NAME.meta"
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'printf "RSH=%s\n" "${RSYNC_RSH:-}" >"$ANUBIS_FAKE_RSYNC_LOG"' \
  'printf "ARG=%s\n" "$@" >>"$ANUBIS_FAKE_RSYNC_LOG"' \
  'if [[ -n "${ANUBIS_FAKE_RSYNC_MUTATE_PIN:-}" ]]; then chmod u+w "$ANUBIS_FAKE_RSYNC_MUTATE_PIN"; printf "mutated during sync\n" >>"$ANUBIS_FAKE_RSYNC_MUTATE_PIN"; chmod a-w,a+x "$ANUBIS_FAKE_RSYNC_MUTATE_PIN"; fi' \
  >"$TMP/sync-fakebin/rsync"
chmod +x "$TMP/sync-fakebin/rsync"
anubis_guard_rsync() { "$TMP/sync-fakebin/rsync" "$@"; }
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'printf "SSH_ARG=%s\n" "$@" >>"$ANUBIS_FAKE_SSH_LOG"' \
  '[[ "${ANUBIS_FAKE_SSH_FAIL:-0}" == 1 ]] && exit 1' \
  'command="${!#}"' \
  'if [[ "$command" == *ANUBIS_GUEST_PIN_IDENTITY_V1* ]]; then' \
  '  receipt="$ANUBIS_FAKE_SSH_PIN_IDENTITY"' \
  '  if [[ -n "${ANUBIS_FAKE_SSH_CLOSING_PIN_IDENTITY:-}" ]]; then' \
  '    count=0; [[ -f "$ANUBIS_FAKE_SSH_IDENTITY_COUNT_FILE" ]] && count="$(<"$ANUBIS_FAKE_SSH_IDENTITY_COUNT_FILE")"' \
  '    count=$((count + 1)); printf "%s\n" "$count" >"$ANUBIS_FAKE_SSH_IDENTITY_COUNT_FILE"' \
  '    [[ "$count" -ge 2 ]] && receipt="$ANUBIS_FAKE_SSH_CLOSING_PIN_IDENTITY"' \
  '  fi' \
  '  printf "ANUBIS_GUEST_PIN_IDENTITY_V1\n%s\n" "$receipt"; exit 0' \
  'fi' \
  'if [[ "${ANUBIS_FAKE_SSH_EXEC_ZSH:-0}" == 1 ]]; then HOME="$ANUBIS_FAKE_SSH_HOME" /bin/zsh -c "$command"; exit $?; fi' \
  'if [[ "${ANUBIS_FAKE_SSH_EXEC_LOCAL:-0}" == 1 ]]; then HOME="$ANUBIS_FAKE_SSH_HOME" bash -c "$command"; exit $?; fi' \
  'exit 0' \
  >"$TMP/sync-fakebin/ssh"
chmod +x "$TMP/sync-fakebin/ssh"
anubis_guard_ssh() { "$TMP/sync-fakebin/ssh" "$@"; }
ANUBIS_FAKE_SSH_PIN_IDENTITY="$(anubis_guard_selected_pin_identity "$TMP/source")"
export ANUBIS_FAKE_SSH_PIN_IDENTITY
: >"$TMP/fake-rsync.log"
: >"$TMP/fake-ssh.log"
sync_ok=1
if declare -F anubis_guard_sync_tree >/dev/null 2>&1; then
  set +e
  PATH="$TMP/sync-fakebin:$PATH" \
  ANUBIS_FAKE_RSYNC_LOG="$TMP/fake-rsync.log" \
  ANUBIS_FAKE_SSH_LOG="$TMP/fake-ssh.log" \
    anubis_guard_sync_tree "ssh -i fake-key" "$TMP/source/" "admin@guest:anubis-lang/"
  sync_rc=$?
  set -e
  [[ $sync_rc -eq 0 ]] || sync_ok=0
else
  sync_rc=127
  sync_ok=0
fi
for excluded in '/vm/pins/*'; do
  grep -Fxq "ARG=--exclude=$excluded" "$TMP/fake-rsync.log" || sync_ok=0
done
while IFS= read -r excluded; do
  [[ -n "$excluded" ]] || continue
  if [[ "$excluded" == "/vm/pins/" ]]; then
    if grep -Fxq "ARG=--exclude=$excluded" "$TMP/fake-rsync.log"; then sync_ok=0; fi
    continue
  fi
  grep -Fxq "ARG=--exclude=$excluded" "$TMP/fake-rsync.log" || sync_ok=0
  if [[ "$excluded" == "/.git" || "$excluded" == "/target/" ]]; then
    if grep -Fq "${excluded#/}=policy__" "$TMP/fake-ssh.log"; then sync_ok=0; fi
    continue
  fi
  quarantine_rel="${excluded#/}"
  quarantine_rel="${quarantine_rel%/}"
  grep -Fq "$quarantine_rel=policy__" "$TMP/fake-ssh.log" || sync_ok=0
done < <(python3 "$TMP/source/scripts/lib/pin_manifest.py" \
  --root "$TMP/source" --print-rsync-excludes)
for included in /vm/pins/ /vm/pins/CURRENT "/vm/pins/$PIN_NAME" "/vm/pins/$PIN_NAME.meta"; do
  grep -Fxq "ARG=--include=$included" "$TMP/fake-rsync.log" || sync_ok=0
done
grep -Fxq 'RSH=/usr/bin/ssh -i fake-key' "$TMP/fake-rsync.log" || sync_ok=0
grep -Fxq 'ARG=--checksum' "$TMP/fake-rsync.log" || sync_ok=0
grep -Fxq 'ARG=--no-times' "$TMP/fake-rsync.log" || sync_ok=0
grep -Fxq 'ARG=--delete' "$TMP/fake-rsync.log" || sync_ok=0
if grep -Fxq 'ARG=--delete-excluded' "$TMP/fake-rsync.log"; then sync_ok=0; fi
grep -Fxq "ARG=$TMP/source/" "$TMP/fake-rsync.log" || sync_ok=0
grep -Fxq 'ARG=admin@guest:anubis-lang/' "$TMP/fake-rsync.log" || sync_ok=0
grep -Fq 'SSH_ARG=admin@guest' "$TMP/fake-ssh.log" || sync_ok=0
for driver in \
  scripts/vm/run-slice.sh \
  scripts/run_offensive_platform_gate.sh \
  scripts/run_poc_kit_gate.sh; do
  grep -Fq 'anubis_guard_sync_tree' "$ROOT/$driver" || sync_ok=0
  grep -Fq 'anubis_guard_start_runtime_watch' "$ROOT/$driver" || sync_ok=0
  grep -Fq 'anubis_guard_stop_runtime_watch' "$ROOT/$driver" || sync_ok=0
  grep -Fq 'anubis_guard_teardown_guest' "$ROOT/$driver" || sync_ok=0
  case "$driver" in
    scripts/vm/run-slice.sh)
      grep -Fq 'anubis_guard_start_runtime_watch $$ "$RUN"' "$ROOT/$driver" || sync_ok=0
      grep -Fq 'guest_source_manifest_before_battery.json' "$ROOT/$driver" || sync_ok=0
      grep -Fq 'guest_source_manifest_after_battery.json' "$ROOT/$driver" || sync_ok=0
      grep -Fq 'cmp -s "$HOST_EVIDENCE_DIR/guest_source_manifest_before_battery.json"' \
        "$ROOT/$driver" || sync_ok=0
      grep -Fq 'guest_pin_identity_before_battery.txt' "$ROOT/$driver" || sync_ok=0
      grep -Fq 'guest_pin_identity_after_battery.txt' "$ROOT/$driver" || sync_ok=0
      grep -Fq 'host_pin_identity_after.txt' "$ROOT/$driver" || sync_ok=0
      grep -Fq 'host CURRENT pin identity changed during VM run' "$ROOT/$driver" || sync_ok=0
      grep -Fq 'PIN_VERIFY_FLAG="--verify-release"' "$ROOT/$driver" || sync_ok=0
      ;;
    *)
      grep -Fq 'anubis_guard_start_runtime_watch $$ "$guest"' "$ROOT/$driver" || sync_ok=0 ;;
  esac
  if [[ "$driver" != scripts/vm/run-slice.sh ]]; then
    grep -Fq 'anubis_guard_require_torn_down "$teardown_final" || return 1' \
      "$ROOT/$driver" || sync_ok=0
  fi
done
record vm_sync_excludes_artifacts "$sync_ok" "rc=$sync_rc"

bad_destination_ok=1
for bad_destination in \
  'admin@guest:.' \
  'admin@guest:./anubis-lang/' \
  'admin@guest:anubis-lang/./nested' \
  'admin@guest:anubis-lang//nested' \
  'admin@guest:anubis-lang//' \
  ':anubis-lang/' \
  '-guest:anubis-lang/' \
  '@guest:anubis-lang/'; do
  : >"$TMP/fake-rsync.log"
  : >"$TMP/fake-ssh.log"
  set +e
  PATH="$TMP/sync-fakebin:$PATH" \
    ANUBIS_FAKE_RSYNC_LOG="$TMP/fake-rsync.log" \
    ANUBIS_FAKE_SSH_LOG="$TMP/fake-ssh.log" \
    anubis_guard_sync_tree "ssh -i fake-key" "$TMP/source/" "$bad_destination" \
      >/dev/null 2>&1
  bad_destination_rc=$?
  set -e
  if [[ "$bad_destination_rc" -ne 2 || -s "$TMP/fake-rsync.log" \
    || -s "$TMP/fake-ssh.log" ]]; then
    bad_destination_ok=0
  fi
done
record vm_sync_rejects_broad_destination "$bad_destination_ok" \
  "dot/noncanonical paths and unsafe hosts rejected before SSH/rsync"

mkdir -p "$TMP/host-tool-shims"
for shim in python3 rsync ssh; do
  printf '%s\n' '#!/usr/bin/env bash' \
    'printf "%s\n" "$0" >>"$ANUBIS_HOST_TOOL_SHIM_MARKER"' 'exit 96' \
    >"$TMP/host-tool-shims/$shim"
  chmod +x "$TMP/host-tool-shims/$shim"
done
: >"$TMP/host-tool-shim.marker"
set +e
(
  eval "$PRODUCTION_RSYNC_FUNCTION"
  PATH="$TMP/host-tool-shims:$PATH" \
    ANUBIS_HOST_TOOL_SHIM_MARKER="$TMP/host-tool-shim.marker" \
    anubis_guard_rsync --version >/dev/null
)
trusted_rsync_rc=$?
(
  eval "$PRODUCTION_SSH_FUNCTION"
  PATH="$TMP/host-tool-shims:$PATH" \
    ANUBIS_HOST_TOOL_SHIM_MARKER="$TMP/host-tool-shim.marker" \
    anubis_guard_ssh -V >/dev/null 2>&1
)
trusted_ssh_rc=$?
PATH="$TMP/host-tool-shims:$TMP/sync-fakebin:$PATH" \
  ANUBIS_HOST_TOOL_SHIM_MARKER="$TMP/host-tool-shim.marker" \
  ANUBIS_FAKE_RSYNC_LOG="$TMP/fake-rsync.log" \
  ANUBIS_FAKE_SSH_LOG="$TMP/fake-ssh.log" \
  anubis_guard_sync_tree "ssh -i fake-key" "$TMP/source/" "admin@guest:anubis-lang/" \
  >/dev/null 2>&1
trusted_python_rc=$?
set -e
if [[ "$trusted_rsync_rc" -eq 0 && "$trusted_ssh_rc" -eq 0 \
  && "$trusted_python_rc" -eq 0 \
  && ! -s "$TMP/host-tool-shim.marker" ]]; then
  ok=1
else
  ok=0
fi
record vm_sync_ignores_path_tool_shims "$ok" \
  "rsync_rc=$trusted_rsync_rc ssh_rc=$trusted_ssh_rc python_rc=$trusted_python_rc"

mkdir -p "$TMP/python-site-poison"
printf '%s\n' \
  'import os' \
  'from pathlib import Path' \
  'Path(os.environ["ANUBIS_PYTHON_SITE_MARKER"]).touch()' \
  >"$TMP/python-site-poison/sitecustomize.py"
rm -f "$TMP/python-sitecustomize.marker"
set +e
PYTHONPATH="$TMP/python-site-poison" \
  ANUBIS_PYTHON_SITE_MARKER="$TMP/python-sitecustomize.marker" \
  PATH="$TMP/sync-fakebin:$PATH" \
  ANUBIS_FAKE_RSYNC_LOG="$TMP/fake-rsync.log" \
  ANUBIS_FAKE_SSH_LOG="$TMP/fake-ssh.log" \
  anubis_guard_sync_tree "ssh -i fake-key" "$TMP/source/" "admin@guest:anubis-lang/" \
    >/dev/null 2>&1
isolated_python_rc=$?
set -e
if [[ "$isolated_python_rc" -eq 0 && ! -e "$TMP/python-sitecustomize.marker" ]]; then
  ok=1
else
  ok=0
fi
record vm_sync_ignores_python_sitecustomize "$ok" \
  "rc=$isolated_python_rc PYTHONPATH/user-site startup code excluded"

cp "$TMP/source/scripts/lib/pin_manifest.py" "$TMP/pin-manifest-tool.backup"
printf '%s\n' '#!/usr/bin/env python3' \
  'print("/.git")' 'print("/target/")' 'print("/vm/pins/")' 'print("/../outside/")' \
  >"$TMP/source/scripts/lib/pin_manifest.py"
set +e
PATH="$TMP/sync-fakebin:$PATH" ANUBIS_FAKE_RSYNC_LOG="$TMP/fake-rsync.log" \
  ANUBIS_FAKE_SSH_LOG="$TMP/fake-ssh.log" \
  anubis_guard_sync_tree "ssh -i fake-key" "$TMP/source/" "admin@guest:anubis-lang/" \
  >"$TMP/policy-traversal.out" 2>"$TMP/policy-traversal.err"
policy_traversal_rc=$?
set -e
mv "$TMP/pin-manifest-tool.backup" "$TMP/source/scripts/lib/pin_manifest.py"
if [[ "$policy_traversal_rc" -eq 2 ]] \
  && grep -q 'unsafe exported exclusion: /../outside/' "$TMP/policy-traversal.err"; then
  ok=1
else
  ok=0
fi
record vm_sync_rejects_policy_traversal "$ok" "rc=$policy_traversal_rc"

saved_fake_guest_pin_identity="$ANUBIS_FAKE_SSH_PIN_IDENTITY"
ANUBIS_FAKE_SSH_PIN_IDENTITY="$(printf '%s\n' "$saved_fake_guest_pin_identity" \
  | sed 's/^pin_sha256=.*/pin_sha256=0000000000000000000000000000000000000000000000000000000000000000/')"
export ANUBIS_FAKE_SSH_PIN_IDENTITY
set +e
PATH="$TMP/sync-fakebin:$PATH" ANUBIS_FAKE_RSYNC_LOG="$TMP/fake-rsync.log" \
  ANUBIS_FAKE_SSH_LOG="$TMP/fake-ssh.log" \
  anubis_guard_sync_tree "ssh -i fake-key" "$TMP/source/" "admin@guest:anubis-lang/" \
  >"$TMP/guest-pin-mismatch.out" 2>"$TMP/guest-pin-mismatch.err"
guest_pin_mismatch_rc=$?
set -e
ANUBIS_FAKE_SSH_PIN_IDENTITY="$saved_fake_guest_pin_identity"
export ANUBIS_FAKE_SSH_PIN_IDENTITY
if [[ "$guest_pin_mismatch_rc" -eq 2 ]] \
   && grep -q 'guest selected pin bytes do not match' "$TMP/guest-pin-mismatch.err"; then
  ok=1
else
  ok=0
fi
record vm_sync_rejects_guest_pin_mismatch "$ok" "rc=$guest_pin_mismatch_rc"

for changed_field in current_sha256 meta_sha256; do
  closing_identity="$(printf '%s\n' "$saved_fake_guest_pin_identity" \
    | sed "s/^${changed_field}=.*/${changed_field}=0000000000000000000000000000000000000000000000000000000000000000/")"
  : >"$TMP/fake-ssh-identity-count"
  set +e
  PATH="$TMP/sync-fakebin:$PATH" ANUBIS_FAKE_RSYNC_LOG="$TMP/fake-rsync.log" \
    ANUBIS_FAKE_SSH_LOG="$TMP/fake-ssh.log" \
    ANUBIS_FAKE_SSH_CLOSING_PIN_IDENTITY="$closing_identity" \
    ANUBIS_FAKE_SSH_IDENTITY_COUNT_FILE="$TMP/fake-ssh-identity-count" \
    anubis_guard_sync_tree "ssh -i fake-key" "$TMP/source/" "admin@guest:anubis-lang/" \
    >"$TMP/guest-${changed_field}-race.out" 2>"$TMP/guest-${changed_field}-race.err"
  guest_closing_race_rc=$?
  set -e
  if [[ "$guest_closing_race_rc" -eq 2 ]] \
    && grep -q 'guest selected pin, metadata, or CURRENT changed after sync' \
      "$TMP/guest-${changed_field}-race.err"; then
    ok=1
  else
    ok=0
  fi
  record "vm_sync_rejects_guest_${changed_field}_closing_race" "$ok" \
    "rc=$guest_closing_race_rc"
done

selected_pin="$TMP/source/vm/pins/$PIN_NAME"
selected_meta="$selected_pin.meta"

chmod u+w "$selected_pin"
set +e
PATH="$TMP/sync-fakebin:$PATH" ANUBIS_FAKE_RSYNC_LOG="$TMP/fake-rsync.log" \
  ANUBIS_FAKE_SSH_LOG="$TMP/fake-ssh.log" \
  anubis_guard_sync_tree "ssh -i fake-key" "$TMP/source/" "admin@guest:anubis-lang/" \
  >/dev/null 2>&1
writable_selected_pin_rc=$?
set -e
chmod 0555 "$selected_pin"
[[ "$writable_selected_pin_rc" -eq 2 ]] && ok=1 || ok=0
record vm_sync_rejects_writable_selected_pin "$ok" "rc=$writable_selected_pin_rc"

chmod 0444 "$selected_pin"
set +e
PATH="$TMP/sync-fakebin:$PATH" ANUBIS_FAKE_RSYNC_LOG="$TMP/fake-rsync.log" \
  ANUBIS_FAKE_SSH_LOG="$TMP/fake-ssh.log" \
  anubis_guard_sync_tree "ssh -i fake-key" "$TMP/source/" "admin@guest:anubis-lang/" \
  >/dev/null 2>&1
nonexec_selected_pin_rc=$?
set -e
chmod 0555 "$selected_pin"
[[ "$nonexec_selected_pin_rc" -eq 2 ]] && ok=1 || ok=0
record vm_sync_rejects_nonexec_selected_pin "$ok" "rc=$nonexec_selected_pin_rc"

chmod u+w "$selected_meta"
set +e
PATH="$TMP/sync-fakebin:$PATH" ANUBIS_FAKE_RSYNC_LOG="$TMP/fake-rsync.log" \
  ANUBIS_FAKE_SSH_LOG="$TMP/fake-ssh.log" \
  anubis_guard_sync_tree "ssh -i fake-key" "$TMP/source/" "admin@guest:anubis-lang/" \
  >/dev/null 2>&1
writable_selected_meta_rc=$?
set -e
chmod 0444 "$selected_meta"
[[ "$writable_selected_meta_rc" -eq 2 ]] && ok=1 || ok=0
record vm_sync_rejects_writable_selected_meta "$ok" "rc=$writable_selected_meta_rc"

cp -p "$selected_meta" "$TMP/selected-meta.backup"
chmod u+w "$selected_meta"
python3 - "$selected_meta" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
lines = path.read_text(encoding="utf-8").splitlines()
path.write_text(
    "\n".join("sha256: " + "0" * 64 if line.startswith("sha256:") else line for line in lines)
    + "\n",
    encoding="utf-8",
)
PY
chmod 0444 "$selected_meta"
set +e
PATH="$TMP/sync-fakebin:$PATH" ANUBIS_FAKE_RSYNC_LOG="$TMP/fake-rsync.log" \
  ANUBIS_FAKE_SSH_LOG="$TMP/fake-ssh.log" \
  anubis_guard_sync_tree "ssh -i fake-key" "$TMP/source/" "admin@guest:anubis-lang/" \
  >/dev/null 2>&1
inconsistent_selected_meta_rc=$?
set -e
chmod u+w "$selected_meta"
cp -p "$TMP/selected-meta.backup" "$selected_meta"
chmod 0444 "$selected_meta"
[[ "$inconsistent_selected_meta_rc" -eq 2 ]] && ok=1 || ok=0
record vm_sync_rejects_inconsistent_pin_metadata "$ok" "rc=$inconsistent_selected_meta_rc"

cp -p "$selected_pin" "$TMP/selected-pin.backup"
set +e
PATH="$TMP/sync-fakebin:$PATH" ANUBIS_FAKE_RSYNC_LOG="$TMP/fake-rsync.log" \
  ANUBIS_FAKE_SSH_LOG="$TMP/fake-ssh.log" \
  ANUBIS_FAKE_RSYNC_MUTATE_PIN="$selected_pin" \
  anubis_guard_sync_tree "ssh -i fake-key" "$TMP/source/" "admin@guest:anubis-lang/" \
  >/dev/null 2>&1
mutated_during_sync_rc=$?
set -e
chmod u+w "$selected_pin"
cp -p "$TMP/selected-pin.backup" "$selected_pin"
chmod 0555 "$selected_pin"
[[ "$mutated_during_sync_rc" -eq 2 ]] && ok=1 || ok=0
record vm_sync_rejects_pin_digest_change_during_sync "$ok" "rc=$mutated_during_sync_rc"

# The guest battery's acceptance list must be an exact inventory of every `run NAME ...`
# invocation in its remote body. Counting only failures is fail-open: deleting a required
# result from EXPECTED_GATES makes a never-run gate indistinguishable from PASS.
set +e
python3 - "$ROOT/scripts/vm/run-slice.sh" "$ROOT/scripts/lib/vm_battery_validate.py" <<'PY'
import re
import sys
from runpy import run_path

text = open(sys.argv[1]).read()
remote = text.split("<<'REMOTE'", 1)[1].split("\nREMOTE", 1)[0]
runs = re.findall(r"^run\s+([a-z0-9-]+)\s+", remote, re.M)
namespace = run_path(sys.argv[2])
expected = namespace.get("EXPECTED_GATES")
if not isinstance(expected, tuple) or not expected or not all(isinstance(x, str) and x for x in expected):
    raise SystemExit(f"validator EXPECTED_GATES is not a non-empty tuple[str, ...]: {expected!r}")
if len(expected) != 23 or len(expected) != len(set(expected)):
    raise SystemExit(f"validator EXPECTED_GATES must contain 23 unique names: {expected!r}")
if runs != list(expected):
    raise SystemExit(f"remote runs != expected gates: runs={runs!r} expected={expected!r}")
if len(runs) != len(set(runs)):
    raise SystemExit(f"duplicate remote gate names: {runs!r}")
if text.count("scripts/lib/vm_battery_validate.py") != 1:
    raise SystemExit("run-slice must invoke the strict validator exactly once")
for token in (
    "--log", "--protocol", "--out", "--expected-fixpoint", "--expected-jobs",
    "--expected-pin", "--expected-pin-sha256", "--expected-pin-meta-sha256",
):
    if token not in text:
        raise SystemExit(f"strict validator invocation missing {token}")
for token in (
    'PROTOCOL_LOG="$HOST_EVIDENCE_DIR/battery.protocol"',
    'PROTOCOL_TMP="$(mktemp "$HOME/.battery.protocol.XXXXXX")"',
    'exec 3>>"$PROTOCOL_TMP"',
    'exec 4<"$PROTOCOL_TMP"',
    'rm -f "$PROTOCOL_TMP"',
    '3>&- 4>&-',
    'cat <&4 || exit 125',
    'ANUBIS_VM_SEAL_FIXPOINT',
    'ANUBIS_VM_SELECTED_PIN',
    'ANUBIS_VM_LOG_SHA256',
    'remote_battery.stderr',
    'protocol_transport_exit_code.txt',
    '--protocol "$PROTOCOL_LOG"',
):
    if token not in text:
        raise SystemExit(f"wrapper-only VM protocol wiring missing {token}")
for forbidden in (
    'PROTOCOL="$HOME/battery.protocol"',
    'cat <&4 > "$PROTOCOL"',
    ':battery.protocol" "$PROTOCOL_LOG"',
    'PROTOCOL_SCP_RC=',
):
    if forbidden in text:
        raise SystemExit(f"child-writable VM protocol transport remains: {forbidden}")
PY
battery_inventory_rc=$?
set -e
[[ "$battery_inventory_rc" -eq 0 ]] && ok=1 || ok=0
record vm_battery_expected_gate_set_complete "$ok" "rc=$battery_inventory_rc"

if grep -Fq 'CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-6}" cargo build -q --release -p anubis' \
  "$ROOT/scripts/run_native_authoritative_gate.sh"; then ok=1; else ok=0; fi
record nested_native_build_inherits_job_cap "$ok" "native-authoritative does not override guest cap"

if grep -Fq 'local build_jobs="${ANUBIS_OFFENSIVE_GATE_BUILD_JOBS:-${ANUBIS_VM_BUILD_JOBS:-${CARGO_BUILD_JOBS:-6}}}"' \
  "$ROOT/scripts/run_offensive_platform_gate.sh" \
  && grep -Fq 'local rayon_threads="${ANUBIS_OFFENSIVE_GATE_RAYON_THREADS:-${ANUBIS_VM_BUILD_JOBS:-${RAYON_NUM_THREADS:-$build_jobs}}}"' \
    "$ROOT/scripts/run_offensive_platform_gate.sh"; then ok=1; else ok=0; fi
record offensive_guest_inherits_general_job_cap "$ok" "offensive-specific override remains available"

filter_src="$TMP/filter-src"
filter_dst="$TMP/filter-dst"
mkdir -p "$filter_src/nested/out" "$filter_src/nested/target" \
  "$filter_src/nested/.git/worktrees" "$filter_src/.git" \
  "$filter_dst/nested/out" "$filter_dst/nested/target" \
  "$filter_dst/nested/.git/worktrees" "$filter_dst/target" "$filter_dst/.git/worktrees"
printf 'current\n' >"$filter_src/nested/out/current"
printf 'current\n' >"$filter_src/nested/target/current"
printf 'stale\n' >"$filter_dst/nested/out/stale"
printf 'stale\n' >"$filter_dst/nested/target/stale"
printf 'current\n' >"$filter_src/nested/.git/worktrees/current"
printf 'stale\n' >"$filter_dst/nested/.git/worktrees/stale"
printf 'warm\n' >"$filter_dst/target/warm-cache"
printf 'root-metadata\n' >"$filter_dst/.git/worktrees/root-metadata"
printf 'source-parent\n' >"$filter_src/.git/source-parent"
rsync -a --delete --exclude=/target/ --exclude=/out/ --exclude='/.git/worktrees/***' \
  "$filter_src/" "$filter_dst/"
if [[ -f "$filter_dst/nested/out/current" && -f "$filter_dst/nested/target/current" \
  && -f "$filter_dst/nested/.git/worktrees/current" \
  && ! -e "$filter_dst/nested/out/stale" && ! -e "$filter_dst/nested/target/stale" \
  && ! -e "$filter_dst/nested/.git/worktrees/stale" \
  && -f "$filter_dst/target/warm-cache" && -f "$filter_dst/.git/worktrees/root-metadata" ]]; then
  ok=1
else
  ok=0
fi
record vm_sync_filters_are_root_anchored "$ok" "nested source synchronized; root target preserved"

# Exercise rsync's first-match filter semantics with the real platform binary.
# The selected pair must be copied, source-side archive siblings must be hidden,
# and a valid pre-existing guest archive binary must remain untouched.
real_pin_source="$TMP/real-pin-source"
real_pin_destination="$TMP/real-pin-destination"
mkdir -p "$real_pin_source/vm/pins" "$real_pin_destination/vm/pins"
cp "$TMP/source/vm/pins/CURRENT" "$real_pin_source/vm/pins/CURRENT"
cp "$TMP/source/vm/pins/$PIN_NAME" "$real_pin_source/vm/pins/$PIN_NAME"
cp "$TMP/source/vm/pins/$PIN_NAME.meta" "$real_pin_source/vm/pins/$PIN_NAME.meta"
printf 'source-archive-not-selected\n' \
  >"$real_pin_source/vm/pins/anubis-aaaaaaaaaaaa"
printf 'guest-archive-preserved\n' \
  >"$real_pin_destination/vm/pins/anubis-bbbbbbbbbbbb"
printf 'guest-archive-metadata-preserved\n' \
  >"$real_pin_destination/vm/pins/anubis-bbbbbbbbbbbb.meta"
chmod 0444 "$real_pin_destination/vm/pins/anubis-bbbbbbbbbbbb.meta"
real_policy_args=()
while IFS= read -r excluded; do
  [[ -n "$excluded" ]] || continue
  [[ "$excluded" == "/vm/pins/" ]] && continue
  real_policy_args+=(--exclude="$excluded")
done < <(/usr/bin/python3 "$TMP/source/scripts/lib/pin_manifest.py" \
  --root "$TMP/source" --print-rsync-excludes)
set +e
/usr/bin/rsync -a --delete "${real_policy_args[@]}" \
  --include=/vm/pins/ \
  --include=/vm/pins/CURRENT \
  --include="/vm/pins/$PIN_NAME" \
  --include="/vm/pins/$PIN_NAME.meta" \
  --exclude='/vm/pins/*' \
  "$real_pin_source/" "$real_pin_destination/"
real_pin_rsync_rc=$?
set -e
if [[ "$real_pin_rsync_rc" -eq 0 \
  && -f "$real_pin_destination/vm/pins/CURRENT" \
  && -f "$real_pin_destination/vm/pins/$PIN_NAME" \
  && -f "$real_pin_destination/vm/pins/$PIN_NAME.meta" \
  && -f "$real_pin_destination/vm/pins/anubis-bbbbbbbbbbbb" \
  && -f "$real_pin_destination/vm/pins/anubis-bbbbbbbbbbbb.meta" \
  && ! -e "$real_pin_destination/vm/pins/anubis-aaaaaaaaaaaa" ]]; then
  ok=1
else
  ok=0
fi
record vm_sync_real_rsync_selected_pin_filter "$ok" \
  "rc=$real_pin_rsync_rc selected pair copied; archive boundary held"

ln -s "$TMP/source" "$TMP/source-link"
set +e
PATH="$TMP/sync-fakebin:$PATH" ANUBIS_FAKE_RSYNC_LOG="$TMP/fake-rsync.log" \
  ANUBIS_FAKE_SSH_LOG="$TMP/fake-ssh.log" \
  anubis_guard_sync_tree "ssh -i fake-key" "$TMP/source-link/" "admin@guest:anubis-lang/" \
  >/dev/null 2>&1
symlink_source_rc=$?
set -e
[[ "$symlink_source_rc" -eq 2 ]] && ok=1 || ok=0
record vm_sync_rejects_symlink_source "$ok" "rc=$symlink_source_rc"

mkdir -p "$TMP/real-parent/nested-source"
ln -s "$TMP/real-parent" "$TMP/parent-link"
set +e
PATH="$TMP/sync-fakebin:$PATH" ANUBIS_FAKE_RSYNC_LOG="$TMP/fake-rsync.log" \
  ANUBIS_FAKE_SSH_LOG="$TMP/fake-ssh.log" \
  anubis_guard_sync_tree "ssh -i fake-key" "$TMP/parent-link/nested-source/" \
    "admin@guest:anubis-lang/" >/dev/null 2>&1
symlink_ancestor_rc=$?
set -e
[[ "$symlink_ancestor_rc" -eq 2 ]] && ok=1 || ok=0
record vm_sync_rejects_symlink_source_ancestor "$ok" "rc=$symlink_ancestor_rc"

rm "$TMP/source/vm/pins/CURRENT"
ln -s /tmp/outside-pin "$TMP/source/vm/pins/CURRENT"
set +e
PATH="$TMP/sync-fakebin:$PATH" ANUBIS_FAKE_RSYNC_LOG="$TMP/fake-rsync.log" \
  ANUBIS_FAKE_SSH_LOG="$TMP/fake-ssh.log" \
  anubis_guard_sync_tree "ssh -i fake-key" "$TMP/source/" "admin@guest:anubis-lang/" \
  >/dev/null 2>&1
symlink_pin_rc=$?
set -e
rm "$TMP/source/vm/pins/CURRENT"
printf 'vm/pins/%s\n' "$PIN_NAME" >"$TMP/source/vm/pins/CURRENT"
[[ "$symlink_pin_rc" -eq 2 ]] && ok=1 || ok=0
record vm_sync_rejects_symlink_pin_manifest "$ok" "rc=$symlink_pin_rc"

mv "$TMP/source/vm/pins/$PIN_NAME" "$TMP/source/vm/pins/$PIN_NAME.saved"
set +e
PATH="$TMP/sync-fakebin:$PATH" ANUBIS_FAKE_RSYNC_LOG="$TMP/fake-rsync.log" \
  ANUBIS_FAKE_SSH_LOG="$TMP/fake-ssh.log" \
  anubis_guard_sync_tree "ssh -i fake-key" "$TMP/source/" "admin@guest:anubis-lang/" \
  >/dev/null 2>&1
missing_pin_rc=$?
set -e
mv "$TMP/source/vm/pins/$PIN_NAME.saved" "$TMP/source/vm/pins/$PIN_NAME"
[[ "$missing_pin_rc" -eq 2 ]] && ok=1 || ok=0
record vm_sync_rejects_missing_selected_pin "$ok" "rc=$missing_pin_rc"

mv "$TMP/source/vm/pins" "$TMP/source/vm/pins-real"
ln -s pins-real "$TMP/source/vm/pins"
set +e
PATH="$TMP/sync-fakebin:$PATH" ANUBIS_FAKE_RSYNC_LOG="$TMP/fake-rsync.log" \
  ANUBIS_FAKE_SSH_LOG="$TMP/fake-ssh.log" \
  anubis_guard_sync_tree "ssh -i fake-key" "$TMP/source/" "admin@guest:anubis-lang/" \
  >/dev/null 2>&1
source_pin_ancestor_rc=$?
set -e
rm "$TMP/source/vm/pins"
mv "$TMP/source/vm/pins-real" "$TMP/source/vm/pins"
[[ "$source_pin_ancestor_rc" -eq 2 ]] && ok=1 || ok=0
record vm_sync_rejects_symlink_pin_ancestor "$ok" "rc=$source_pin_ancestor_rc"

set +e
PATH="$TMP/sync-fakebin:$PATH" ANUBIS_FAKE_RSYNC_LOG="$TMP/fake-rsync.log" \
  ANUBIS_FAKE_SSH_LOG="$TMP/fake-ssh.log" ANUBIS_FAKE_SSH_FAIL=1 \
  anubis_guard_sync_tree "ssh -i fake-key" "$TMP/source/" "admin@guest:anubis-lang/" \
  >/dev/null 2>&1
remote_symlink_rc=$?
set -e
[[ "$remote_symlink_rc" -eq 2 ]] && ok=1 || ok=0
record vm_sync_rejects_unverified_remote_tree "$ok" "rc=$remote_symlink_rc"

mkdir -p "$TMP/fake-home/real-parent/child"
ln -s "$TMP/fake-home/real-parent" "$TMP/fake-home/parent-link"
set +e
PATH="$TMP/sync-fakebin:$PATH" ANUBIS_FAKE_RSYNC_LOG="$TMP/fake-rsync.log" \
  ANUBIS_FAKE_SSH_LOG="$TMP/fake-ssh.log" ANUBIS_FAKE_SSH_EXEC_LOCAL=1 \
  ANUBIS_FAKE_SSH_HOME="$TMP/fake-home" \
  anubis_guard_sync_tree "ssh -i fake-key" "$TMP/source/" \
    "admin@guest:parent-link/child/" >/dev/null 2>&1
remote_ancestor_rc=$?
set -e
[[ "$remote_ancestor_rc" -eq 2 ]] && ok=1 || ok=0
record vm_sync_rejects_symlink_remote_ancestor "$ok" "rc=$remote_ancestor_rc"

quarantine_symlink_home="$TMP/quarantine-symlink-home"
mkdir -p "$quarantine_symlink_home/anubis-lang/scripts" \
  "$quarantine_symlink_home/outside-nested/generated"
printf 'outside-sentinel\n' >"$quarantine_symlink_home/outside-nested/generated/keep"
ln -s "$quarantine_symlink_home/outside-nested" \
  "$quarantine_symlink_home/anubis-lang/scripts/nested"
set +e
PATH="$TMP/sync-fakebin:$PATH" ANUBIS_FAKE_RSYNC_LOG="$TMP/fake-rsync.log" \
  ANUBIS_FAKE_SSH_LOG="$TMP/fake-ssh.log" ANUBIS_FAKE_SSH_EXEC_LOCAL=1 \
  ANUBIS_FAKE_SSH_HOME="$quarantine_symlink_home" \
  anubis_guard_sync_tree "ssh -i fake-key" "$TMP/source/" \
    "admin@guest:anubis-lang/" >"$TMP/quarantine-symlink.out" \
    2>"$TMP/quarantine-symlink.err"
quarantine_symlink_rc=$?
set -e
if [[ "$quarantine_symlink_rc" -eq 2 \
  && -f "$quarantine_symlink_home/outside-nested/generated/keep" ]] \
  && grep -q 'could not quarantine stale generated guest paths' \
    "$TMP/quarantine-symlink.err"; then
  ok=1
else
  ok=0
fi
record vm_sync_rejects_symlink_quarantine_ancestor "$ok" \
  "rc=$quarantine_symlink_rc outside sentinel preserved"

git_symlink_home="$TMP/git-symlink-home"
mkdir -p "$git_symlink_home/anubis-lang" "$git_symlink_home/outside-git/worktrees"
printf 'outside-git-sentinel\n' >"$git_symlink_home/outside-git/worktrees/keep"
ln -s "$git_symlink_home/outside-git" "$git_symlink_home/anubis-lang/.git"
set +e
PATH="$TMP/sync-fakebin:$PATH" ANUBIS_FAKE_RSYNC_LOG="$TMP/fake-rsync.log" \
  ANUBIS_FAKE_SSH_LOG="$TMP/fake-ssh.log" ANUBIS_FAKE_SSH_EXEC_LOCAL=1 \
  ANUBIS_FAKE_SSH_HOME="$git_symlink_home" \
  anubis_guard_sync_tree "ssh -i fake-key" "$TMP/source/" \
    "admin@guest:anubis-lang/" >"$TMP/git-symlink.out" 2>"$TMP/git-symlink.err"
git_symlink_rc=$?
set -e
if [[ "$git_symlink_rc" -eq 2 \
  && -f "$git_symlink_home/outside-git/worktrees/keep" ]] \
  && grep -q 'preserved .git or target path is unsafe' "$TMP/git-symlink.err"; then
  ok=1
else
  ok=0
fi
record vm_sync_rejects_symlink_preserved_git "$ok" \
  "rc=$git_symlink_rc outside sentinel preserved"

remote_pin_home="$TMP/remote-pin-symlink-home"
mkdir -p "$remote_pin_home/anubis-lang/vm" "$remote_pin_home/outside-pins"
ln -s "$remote_pin_home/outside-pins" "$remote_pin_home/anubis-lang/vm/pins"
set +e
PATH="$TMP/sync-fakebin:$PATH" ANUBIS_FAKE_RSYNC_LOG="$TMP/fake-rsync.log" \
  ANUBIS_FAKE_SSH_LOG="$TMP/fake-ssh.log" ANUBIS_FAKE_SSH_EXEC_ZSH=1 \
  ANUBIS_FAKE_SSH_HOME="$remote_pin_home" \
  anubis_guard_sync_tree "ssh -i fake-key" "$TMP/source/" \
    "admin@guest:anubis-lang/" >/dev/null 2>&1
remote_pin_symlink_rc=$?
set -e
[[ "$remote_pin_symlink_rc" -eq 2 ]] && ok=1 || ok=0
record vm_sync_rejects_symlink_remote_pin_dir "$ok" "rc=$remote_pin_symlink_rc"

zsh_home="$TMP/zsh-empty-pin-home"
mkdir -p "$zsh_home/anubis-lang/vm/pins"
set +e
PATH="$TMP/sync-fakebin:$PATH" ANUBIS_FAKE_RSYNC_LOG="$TMP/fake-rsync.log" \
  ANUBIS_FAKE_SSH_LOG="$TMP/fake-ssh.log" ANUBIS_FAKE_SSH_EXEC_ZSH=1 \
  ANUBIS_FAKE_SSH_HOME="$zsh_home" \
  anubis_guard_sync_tree "ssh -i fake-key" "$TMP/source/" \
    "admin@guest:anubis-lang/" >/dev/null 2>&1
zsh_empty_pin_rc=$?
set -e
[[ "$zsh_empty_pin_rc" -eq 0 && -d "$zsh_home/anubis-lang/vm/pins" ]] && ok=1 || ok=0
record vm_sync_accepts_empty_pin_archive_under_zsh "$ok" "rc=$zsh_empty_pin_rc"

cleanup_home="$TMP/cleanup-home"
cleanup_repo="$cleanup_home/anubis-lang"
mkdir -p \
  "$cleanup_repo/target/cache" \
  "$cleanup_repo/.git/worktrees/stale" \
  "$cleanup_repo/out/stale" \
  "$cleanup_repo/implementer/a_plus_audit_run/stale" \
  "$cleanup_repo/.claude/worktrees/stale" \
  "$cleanup_repo/secrets/stale" \
  "$cleanup_repo/vm/exports/stale" \
  "$cleanup_repo/vm/pins/junk-dir"
mkdir -p "$cleanup_home/outside-scratchpad"
printf 'outside\n' >"$cleanup_home/outside-scratchpad/keep"
ln -s "$cleanup_home/outside-scratchpad" "$cleanup_repo/scratchpad"
printf 'cache\n' >"$cleanup_repo/target/cache/keep"
printf 'git-metadata\n' >"$cleanup_repo/.git/keep"
printf 'secret-env\n' >"$cleanup_repo/.env"
printf 'archive\n' >"$cleanup_repo/vm/pins/anubis-bbbbbbbbbbbb"
printf 'old-meta\n' >"$cleanup_repo/vm/pins/anubis-bbbbbbbbbbbb.meta"
chmod 0444 "$cleanup_repo/vm/pins/anubis-bbbbbbbbbbbb.meta"
printf 'archive-with-writable-meta\n' >"$cleanup_repo/vm/pins/anubis-dddddddddddd"
printf 'writable-meta\n' >"$cleanup_repo/vm/pins/anubis-dddddddddddd.meta"
printf 'malformed\n' >"$cleanup_repo/vm/pins/anubis-not-a-pin"
printf 'hidden\n' >"$cleanup_repo/vm/pins/.hidden-junk"
printf 'junk\n' >"$cleanup_repo/vm/pins/junk-dir/payload"
ln -s "$cleanup_repo/target" "$cleanup_repo/vm/pins/anubis-cccccccccccc"
printf 'old-current\n' >"$cleanup_repo/vm/pins/CURRENT"
set +e
PATH="$TMP/sync-fakebin:$PATH" ANUBIS_FAKE_RSYNC_LOG="$TMP/fake-rsync.log" \
  ANUBIS_FAKE_SSH_LOG="$TMP/fake-ssh.log" ANUBIS_FAKE_SSH_EXEC_LOCAL=1 \
  ANUBIS_FAKE_SSH_HOME="$cleanup_home" \
  anubis_guard_sync_tree "ssh -i fake-key" "$TMP/source/" \
    "admin@guest:anubis-lang/" >/dev/null 2>&1
cleanup_rc=$?
set -e
shopt -s nullglob
cleanup_quarantines=("$cleanup_home"/.anubis-sync-quarantine.*)
policy_quarantines=("$cleanup_home"/.anubis-policy-quarantine.*)
shopt -u nullglob
cleanup_ok=1
[[ "$cleanup_rc" -eq 0 ]] || cleanup_ok=0
[[ -f "$cleanup_repo/target/cache/keep" ]] || cleanup_ok=0
[[ -f "$cleanup_repo/.git/keep" ]] || cleanup_ok=0
[[ -f "$cleanup_repo/vm/pins/anubis-bbbbbbbbbbbb" ]] || cleanup_ok=0
[[ -f "$cleanup_repo/vm/pins/anubis-bbbbbbbbbbbb.meta" ]] || cleanup_ok=0
[[ -f "$cleanup_repo/vm/pins/anubis-dddddddddddd" ]] || cleanup_ok=0
for removed in \
  .env out/stale implementer/a_plus_audit_run/stale .claude/worktrees/stale \
  secrets/stale vm/exports/stale scratchpad \
  vm/pins/anubis-dddddddddddd.meta vm/pins/anubis-not-a-pin \
  vm/pins/.hidden-junk vm/pins/anubis-cccccccccccc vm/pins/junk-dir vm/pins/CURRENT; do
  [[ ! -e "$cleanup_repo/$removed" ]] || cleanup_ok=0
done
[[ "${#cleanup_quarantines[@]}" -eq 1 ]] || cleanup_ok=0
[[ "${#policy_quarantines[@]}" -eq 1 ]] || cleanup_ok=0
if [[ "${#cleanup_quarantines[@]}" -eq 1 ]]; then
  [[ -f "${cleanup_quarantines[0]}/pin__anubis-not-a-pin" ]] || cleanup_ok=0
  [[ -f "${cleanup_quarantines[0]}/pin__anubis-dddddddddddd.meta" ]] || cleanup_ok=0
  [[ -f "${cleanup_quarantines[0]}/pin__.hidden-junk" ]] || cleanup_ok=0
  [[ -d "${cleanup_quarantines[0]}/pin__junk-dir" ]] || cleanup_ok=0
  [[ -L "${cleanup_quarantines[0]}/pin__anubis-cccccccccccc" ]] || cleanup_ok=0
fi
if [[ "${#policy_quarantines[@]}" -eq 1 ]]; then
  policy_payload_count="$(find "${policy_quarantines[0]}" -mindepth 1 -maxdepth 1 | wc -l | tr -d ' ')"
  policy_symlink_count="$(find "${policy_quarantines[0]}" -mindepth 1 -maxdepth 1 -type l | wc -l | tr -d ' ')"
  [[ "$policy_payload_count" -ge 7 ]] || cleanup_ok=0
  [[ "$policy_symlink_count" -ge 1 ]] || cleanup_ok=0
fi
[[ -f "$cleanup_home/outside-scratchpad/keep" ]] || cleanup_ok=0
record vm_sync_cleanup_preserves_explicit_guest_state "$cleanup_ok" \
  "rc=$cleanup_rc .git/target/pin archive preserved; policy exclusions quarantined"

set +e
(
  anubis_guard_tart_stop() { return 2; }
  anubis_guard_tart_delete() { return 0; }
  anubis_guard_guest_absent() { return 0; }
  anubis_guard_teardown_guest anubis-run-1 >/dev/null
)
already_stopped_rc=$?
(
  anubis_guard_tart_stop() { return 0; }
  anubis_guard_tart_delete() { return 0; }
  anubis_guard_guest_absent() { return 1; }
  anubis_guard_teardown_guest anubis-run-1 >/dev/null 2>&1
)
survived_rc=$?
set -e
[[ "$already_stopped_rc" -eq 0 ]] && ok=1 || ok=0
record teardown_accepts_pre_stopped_absent "$ok" "rc=$already_stopped_rc"
[[ "$survived_rc" -ne 0 ]] && ok=1 || ok=0
record teardown_rejects_surviving_guest "$ok" "rc=$survived_rc"

set +e
anubis_guard_require_torn_down torn_down >/dev/null 2>&1
torn_down_rc=$?
anubis_guard_require_torn_down teardown_failed >/dev/null 2>&1
teardown_failed_rc=$?
set -e
[[ "$torn_down_rc" -eq 0 && "$teardown_failed_rc" -ne 0 ]] && ok=1 || ok=0
record teardown_status_is_exact "$ok" \
  "torn_down_rc=$torn_down_rc teardown_failed_rc=$teardown_failed_rc"

for name in anubis-run-123 anubis-offensive-gate-456 anubis-poc-kit-gate-789 anubis-vz-ephemeral-42; do
  pid="$(anubis_guard_generated_owner_pid "$name" 2>/dev/null || true)"
  [[ -n "$pid" ]] && ok=1 || ok=0
  record "generated_${name}" "$ok" "owner_pid=${pid:-none}"
done
for name in \
  anubis-xcode anubis-warroom anubis-xcode-snapshot random-123 \
  anubis-run-1x anubis-run-1-extra anubis-run--1 anubis-run-01 \
  anubis-offensive-gate-2x anubis-poc-kit-gate-3-extra \
  anubis-vz-ephemeral-4x; do
  if anubis_guard_generated_owner_pid "$name" >/dev/null 2>&1; then ok=0; else ok=1; fi
  record "preserve_${name}" "$ok" "not auto-reapable"
done

TART_JSON='[
 {"Name":"anubis-xcode","Running":true,"State":"running"},
 {"Name":"anubis-warroom","Running":false,"State":"stopped"},
 {"Name":"anubis-run-999999","Running":true,"State":"running"},
 {"Name":"anubis-poc-kit-gate-888888","Running":false,"State":"stopped"}
]'

# Low immediate free memory must stop every running VM, including a named/base VM,
# because protecting WindowServer is a harder boundary than preserving a test run.
: >"$TMP/actions"
anubis_guard_read_vm_stat() { printf '%s\n' 'Mach Virtual Memory Statistics: (page size of 16384 bytes)' 'Pages free: 131072.' 'Pages speculative: 0.'; }
anubis_guard_read_pressure() { printf '1\n'; }
anubis_guard_read_tart_json() { printf '%s\n' "$TART_JSON"; }
anubis_guard_tart_stop() { printf 'stop:%s\n' "$1" >>"$TMP/actions"; }
anubis_guard_tart_delete() { printf 'delete:%s\n' "$1" >>"$TMP/actions"; }
anubis_guard_owner_alive() { return 0; }
set +e
ANUBIS_HOST_MIN_FREE_MIB=8192 anubis_guard_watch_once >/dev/null
watch_rc=$?
set -e
if [[ "$watch_rc" -eq 1 && "$(sort "$TMP/actions")" == $'stop:anubis-run-999999\nstop:anubis-xcode' ]]; then ok=1; else ok=0; fi
record low_memory_stops_running "$ok" "rc=$watch_rc actions=$(tr '\n' ',' <"$TMP/actions")"

set +e
( anubis_guard_watch_once() { return 1; }; anubis_guard_start_runtime_watch $$ ) >/dev/null 2>&1
watch_start_rc=$?
set -e
[[ "$watch_start_rc" -eq 1 ]] && ok=1 || ok=0
record runtime_watch_checks_start "$ok" "rc=$watch_start_rc"

if anubis_guard_guest_running anubis-run-999999 \
  && ! anubis_guard_guest_running missing-guest; then ok=1; else ok=0; fi
record guest_running_state_is_exact "$ok" "running twin accepted; missing twin rejected"

valid_tart_json="$TART_JSON"
set +e
TART_JSON='[{"Name":"expected","Running":"false"}]'
anubis_guard_guest_running expected >/dev/null 2>&1
string_running_rc=$?
TART_JSON='[{"Name":"expected","Running":1}]'
anubis_guard_guest_running expected >/dev/null 2>&1
integer_running_rc=$?
set -e
TART_JSON="$valid_tart_json"
[[ "$string_running_rc" -ne 0 && "$integer_running_rc" -ne 0 ]] && ok=1 || ok=0
record guest_running_rejects_nonboolean_types "$ok" \
  "string_rc=$string_running_rc integer_rc=$integer_running_rc"

set +e
( anubis_guard_watch_once() { return 0; }; \
  anubis_guard_guest_running() { return 1; }; \
  anubis_guard_start_runtime_watch $$ expected-guest ) >/dev/null 2>&1
missing_guest_start_rc=$?
set -e
[[ "$missing_guest_start_rc" -eq 1 ]] && ok=1 || ok=0
record runtime_watch_rejects_stopped_guest_at_start "$ok" "rc=$missing_guest_start_rc"

guest_watch_json="$TMP/guest-watch.json"
guest_watch_events="$TMP/guest-watch.events"
guest_watch_result="$TMP/guest-watch.result"
printf '[{"Name":"expected-guest","Running":true}]\n' >"$guest_watch_json"
: >"$guest_watch_events"
set +e
(
  ANUBIS_GUARD_INTERVAL_SECS=1
  anubis_guard_watch_once() { return 0; }
  anubis_guard_read_tart_json() { command cat "$guest_watch_json"; }
  ( sleep 5; printf 'after-sleep\n' >>"$guest_watch_events" ) &
  watched_owner=$!
  anubis_guard_start_runtime_watch "$watched_owner" expected-guest || exit 2
  printf '[{"Name":"expected-guest","Running":false}]\n' >"$guest_watch_json"
  wait "$watched_owner"
  watched_owner_rc=$?
  anubis_guard_stop_runtime_watch
  printf '%s\n' "$watched_owner_rc" >"$guest_watch_result"
) >/dev/null 2>&1
guest_watch_harness_rc=$?
set -e
if [[ -f "$guest_watch_result" ]]; then
  stopped_guest_watch_rc="$(<"$guest_watch_result")"
else
  stopped_guest_watch_rc=missing
fi
[[ "$guest_watch_harness_rc" -eq 0 && "$stopped_guest_watch_rc" =~ ^[0-9]+$ \
  && "$stopped_guest_watch_rc" -ne 0 \
  && ! -s "$guest_watch_events" ]] && ok=1 || ok=0
record runtime_watch_terminates_stopped_guest "$ok" \
  "harness_rc=$guest_watch_harness_rc owner_rc=$stopped_guest_watch_rc events=$(tr '\n' ',' <"$guest_watch_events")"

set +e
( anubis_guard_read_tart_json() { return 1; }; anubis_guard_watch_once ) >/dev/null 2>&1
tart_unavailable_rc=$?
set -e
[[ "$tart_unavailable_rc" -eq 1 ]] && ok=1 || ok=0
record runtime_watch_requires_tart "$ok" "rc=$tart_unavailable_rc"

# With healthy memory, only generated clones whose owner is gone are reaped;
# named VMs and generated clones with live owners are preserved.
: >"$TMP/actions"
anubis_guard_read_vm_stat() { printf '%s\n' 'Mach Virtual Memory Statistics: (page size of 16384 bytes)' 'Pages free: 1048576.' 'Pages speculative: 0.'; }
anubis_guard_owner_alive() { [[ "$1" == 777777 ]]; }
TART_JSON='[
 {"Name":"anubis-warroom","Running":true,"State":"running"},
 {"Name":"anubis-vz-ephemeral-777777","Running":true,"State":"running"},
 {"Name":"anubis-run-999999","Running":true,"State":"running"},
 {"Name":"anubis-poc-kit-gate-888888","Running":false,"State":"stopped"}
]'
anubis_guard_watch_once >/dev/null
expected=$'delete:anubis-poc-kit-gate-888888\ndelete:anubis-run-999999\nstop:anubis-run-999999'
if [[ "$(sort "$TMP/actions")" == "$expected" ]]; then ok=1; else ok=0; fi
record orphan_reaping_is_scoped "$ok" "actions=$(tr '\n' ',' <"$TMP/actions")"

set +e
(
  ANUBIS_GUARD_QUIET_OK=0
  anubis_guard_read_pressure() { printf '1\n'; }
  anubis_guard_read_vm_stat() {
    printf '%s\n' 'Mach Virtual Memory Statistics: (page size of 16384 bytes)' \
      'Pages free: 1048576.' 'Pages speculative: 0.'
  }
  anubis_guard_read_tart_json() {
    printf '%s\n' '[{"Name":"anubis-run-999991","Running":true}]'
  }
  anubis_guard_owner_alive() { return 1; }
  anubis_guard_tart_stop() { return 17; }
  anubis_guard_tart_delete() { return 0; }
  anubis_guard_watch_once
) >"$TMP/orphan-stop-failure.out" 2>"$TMP/orphan-stop-failure.err"
orphan_stop_failure_rc=$?
(
  ANUBIS_GUARD_QUIET_OK=0
  anubis_guard_read_pressure() { printf '1\n'; }
  anubis_guard_read_vm_stat() {
    printf '%s\n' 'Mach Virtual Memory Statistics: (page size of 16384 bytes)' \
      'Pages free: 1048576.' 'Pages speculative: 0.'
  }
  anubis_guard_read_tart_json() {
    printf '%s\n' '[{"Name":"anubis-run-999992","Running":false}]'
  }
  anubis_guard_owner_alive() { return 1; }
  anubis_guard_tart_stop() { return 0; }
  anubis_guard_tart_delete() { return 18; }
  anubis_guard_watch_once
) >"$TMP/orphan-delete-failure.out" 2>"$TMP/orphan-delete-failure.err"
orphan_delete_failure_rc=$?
set -e
if [[ "$orphan_stop_failure_rc" -ne 0 \
  && "$orphan_delete_failure_rc" -ne 0 ]] \
  && ! grep -q 'ANUBIS_HOST_GUARD_OK' "$TMP/orphan-stop-failure.out" \
  && ! grep -q 'ANUBIS_HOST_GUARD_OK' "$TMP/orphan-delete-failure.out" \
  && grep -q 'ANUBIS_HOST_GUARD_STOP_FAILED' "$TMP/orphan-stop-failure.err" \
  && grep -q 'ANUBIS_HOST_GUARD_DELETE_FAILED' "$TMP/orphan-delete-failure.err"; then
  ok=1
else
  ok=0
fi
record orphan_action_failures_fail_closed "$ok" \
  "stop_rc=$orphan_stop_failure_rc delete_rc=$orphan_delete_failure_rc"

# An explicitly kept disposable clone is exempt from orphan deletion. The
# low-memory breaker may still stop it, but inspection state is not discarded.
: >"$TMP/actions"
anubis_guard_is_kept() { [[ "$1" == anubis-run-999999 ]]; }
anubis_guard_watch_once >/dev/null
if [[ "$(sort "$TMP/actions")" == 'delete:anubis-poc-kit-gate-888888' ]]; then ok=1; else ok=0; fi
record explicit_keep_preserved "$ok" "actions=$(tr '\n' ',' <"$TMP/actions")"

# Admission accounts for the entire requested guest allocation plus the host's
# emergency reserve. A 12 GiB VM must not start from only 16 GiB free.
anubis_guard_read_pressure() { printf '1\n'; }
anubis_guard_read_vm_stat() { printf '%s\n' 'Mach Virtual Memory Statistics: (page size of 16384 bytes)' 'Pages free: 1048576.' 'Pages speculative: 0.'; }
if anubis_guard_preflight 8 12288 >/dev/null 2>&1; then ok=0; else ok=1; fi
record admission_reserves_guest_ram "$ok" "free=16384MiB guest=12288MiB reserve=8192MiB"
anubis_guard_read_vm_stat() { printf '%s\n' 'Mach Virtual Memory Statistics: (page size of 16384 bytes)' 'Pages free: 1572864.' 'Pages speculative: 0.'; }
if anubis_guard_preflight 8 12288 >/dev/null 2>&1; then ok=1; else ok=0; fi
record admission_accepts_headroom "$ok" "free=24576MiB guest=12288MiB reserve=8192MiB"

# Integration regression: the offensive gate deliberately calls run_in_guest
# under `set +e` so it can classify return codes. A rejected preflight must be
# explicitly returned; relying on errexit allowed clone/set/run to continue.
mkdir -p "$TMP/fakebin"
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'echo "$*" >>"$ANUBIS_FAKE_TART_LOG"' \
  'case "${1:-}" in' \
  '  --version) echo "2.32.1" ;;' \
  '  list) if [[ "$*" == *"--format json"* ]]; then echo "[]"; else echo "Source Name State"; echo "local anubis-xcode stopped"; fi ;;' \
  '  ip) echo "127.0.0.1" ;;' \
  'esac' >"$TMP/fakebin/tart"
printf '%s\n' '#!/usr/bin/env bash' 'exit 0' >"$TMP/fakebin/anubis"
chmod +x "$TMP/fakebin/tart" "$TMP/fakebin/anubis"
: >"$TMP/fake-key"
: >"$TMP/fake-tart.log"
set +e
PATH="$TMP/fakebin:$PATH" \
ANUBIS_GUARD_TART_BIN="$TMP/fakebin/tart" \
ANUBIS_FAKE_TART_LOG="$TMP/fake-tart.log" \
ANUBIS_VM_KEY="$TMP/fake-key" \
ANUBIS_BIN="$TMP/fakebin/anubis" \
ANUBIS_OFFENSIVE_GATE_BUILD_JOBS=1 \
ANUBIS_OFFENSIVE_GATE_RAYON_THREADS=1 \
ANUBIS_OFFENSIVE_GATE_VM_MEM=24576 \
  bash "$ROOT/scripts/run_offensive_platform_gate.sh" --out "$TMP/offensive-overcap" \
  >"$TMP/offensive-overcap.log" 2>&1 &
gate_pid=$!
terminated_naturally=0
for _ in $(seq 1 200); do
  if ! kill -0 "$gate_pid" >/dev/null 2>&1; then
    terminated_naturally=1
    break
  fi
  sleep 0.05
done
if [[ "$terminated_naturally" == 1 ]]; then
  wait "$gate_pid" >/dev/null 2>&1
  rc=$?
else
  kill "$gate_pid" >/dev/null 2>&1 || true
  wait "$gate_pid" >/dev/null 2>&1 || true
  rc=124
fi
set -e
if [[ "$terminated_naturally" == 1 && "$rc" -ne 0 ]] \
  && ! grep -q '^clone ' "$TMP/fake-tart.log" \
  && grep -q 'ANUBIS_HOST_GUARD_MEMORY_CEILING' "$TMP/offensive-overcap.log"; then ok=1; else ok=0; fi
record rejected_preflight_no_clone "$ok" \
  "natural=$terminated_naturally rc=$rc tart_actions=$(tr '\n' ',' <"$TMP/fake-tart.log") child=$(tr '\n' ';' <"$TMP/offensive-overcap.log")"

anubis_guard_launchctl_print() { printf 'state = running\n'; }
if anubis_guard_require_launchagent >/dev/null 2>&1; then ok=1; else ok=0; fi
record launchagent_running_required "$ok" "running state accepted"

anubis_guard_launchctl_print() { printf 'state = waiting\n'; }
if anubis_guard_require_launchagent >/dev/null 2>&1; then ok=0; else ok=1; fi
record launchagent_inactive_rejected "$ok" "waiting state rejected"

anubis_guard_read_tart_json() { printf '%s\n' '[{"Name":"anubis-run-4242","Running":false,"State":"stopped"}]'; }
if anubis_guard_guest_absent anubis-run-4242 >/dev/null 2>&1; then ok=0; else ok=1; fi
record teardown_survivor_rejected "$ok" "stopped-but-present guest is not torn down"
if anubis_guard_guest_absent anubis-run-4343 >/dev/null 2>&1; then ok=1; else ok=0; fi
record teardown_absence_accepted "$ok" "absent guest accepted"

printf 'HOST_RESOURCE_GUARD_SELFTEST: %s (pass=%d fail=%d)\n' \
  "$([[ "$fail" -eq 0 ]] && echo PASS || echo FAIL)" "$pass" "$fail"
[[ "$fail" -eq 0 ]]
