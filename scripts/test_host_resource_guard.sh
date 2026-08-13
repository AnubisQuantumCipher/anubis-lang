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

for tuple in "8 12288" "4 8192" "1 4096"; do
  set -- $tuple
  if anubis_guard_validate_vm_limits "$1" "$2" >/dev/null 2>&1; then ok=1; else ok=0; fi
  record "limits_accept_${1}_${2}" "$ok" "cpu=$1 mem=$2"
done

for tuple in "9 12288" "8 12289" "0 8192" "x 8192" "8 0" "8 12GiB"; do
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
PIN_NAME=anubis-aaaaaaaaaaaa
printf 'vm/pins/%s\n' "$PIN_NAME" >"$TMP/source/vm/pins/CURRENT"
printf 'binary\n' >"$TMP/source/vm/pins/$PIN_NAME"
printf 'meta\n' >"$TMP/source/vm/pins/$PIN_NAME.meta"
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'printf "RSH=%s\n" "${RSYNC_RSH:-}" >"$ANUBIS_FAKE_RSYNC_LOG"' \
  'printf "ARG=%s\n" "$@" >>"$ANUBIS_FAKE_RSYNC_LOG"' \
  >"$TMP/sync-fakebin/rsync"
chmod +x "$TMP/sync-fakebin/rsync"
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'printf "SSH_ARG=%s\n" "$@" >>"$ANUBIS_FAKE_SSH_LOG"' \
  '[[ "${ANUBIS_FAKE_SSH_FAIL:-0}" == 1 ]] && exit 1' \
  'if [[ "${ANUBIS_FAKE_SSH_EXEC_ZSH:-0}" == 1 ]]; then command="${!#}"; HOME="$ANUBIS_FAKE_SSH_HOME" /bin/zsh -c "$command"; exit $?; fi' \
  'if [[ "${ANUBIS_FAKE_SSH_EXEC_LOCAL:-0}" == 1 ]]; then command="${!#}"; HOME="$ANUBIS_FAKE_SSH_HOME" bash -c "$command"; exit $?; fi' \
  'exit 0' \
  >"$TMP/sync-fakebin/ssh"
chmod +x "$TMP/sync-fakebin/ssh"
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
for excluded in \
  /target/ /out/ /implementer/a_plus_audit_run/ /.claude/worktrees/ '/.git/worktrees/***' \
  /.hermes/ /adversary/ '/vm/pins/*' /vm/exports/ /scratchpad/ /.DS_Store; do
  grep -Fxq "ARG=--exclude=$excluded" "$TMP/fake-rsync.log" || sync_ok=0
done
for included in /vm/pins/ /vm/pins/CURRENT "/vm/pins/$PIN_NAME" "/vm/pins/$PIN_NAME.meta"; do
  grep -Fxq "ARG=--include=$included" "$TMP/fake-rsync.log" || sync_ok=0
done
grep -Fxq 'RSH=ssh -i fake-key' "$TMP/fake-rsync.log" || sync_ok=0
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
      grep -Fq 'anubis_guard_start_runtime_watch $$ "$RUN"' "$ROOT/$driver" || sync_ok=0 ;;
    *)
      grep -Fq 'anubis_guard_start_runtime_watch $$ "$guest"' "$ROOT/$driver" || sync_ok=0 ;;
  esac
  if [[ "$driver" != scripts/vm/run-slice.sh ]]; then
    grep -Fq 'anubis_guard_require_torn_down "$teardown_final" || return 1' \
      "$ROOT/$driver" || sync_ok=0
  fi
done
record vm_sync_excludes_artifacts "$sync_ok" "rc=$sync_rc"

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
  "$cleanup_repo/.hermes/stale" \
  "$cleanup_repo/adversary/stale" \
  "$cleanup_repo/vm/exports/stale" \
  "$cleanup_repo/vm/pins/junk-dir"
mkdir -p "$cleanup_home/outside-scratchpad"
printf 'outside\n' >"$cleanup_home/outside-scratchpad/keep"
ln -s "$cleanup_home/outside-scratchpad" "$cleanup_repo/scratchpad"
printf 'cache\n' >"$cleanup_repo/target/cache/keep"
printf 'archive\n' >"$cleanup_repo/vm/pins/anubis-bbbbbbbbbbbb"
printf 'old-meta\n' >"$cleanup_repo/vm/pins/anubis-bbbbbbbbbbbb.meta"
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
shopt -u nullglob
cleanup_ok=1
[[ "$cleanup_rc" -eq 0 ]] || cleanup_ok=0
[[ -f "$cleanup_repo/target/cache/keep" ]] || cleanup_ok=0
[[ -f "$cleanup_repo/vm/pins/anubis-bbbbbbbbbbbb" ]] || cleanup_ok=0
for removed in \
  .git/worktrees/stale out/stale implementer/a_plus_audit_run/stale .claude/worktrees/stale \
  .hermes/stale adversary/stale vm/exports/stale scratchpad \
  vm/pins/anubis-bbbbbbbbbbbb.meta vm/pins/anubis-not-a-pin \
  vm/pins/.hidden-junk vm/pins/anubis-cccccccccccc vm/pins/junk-dir vm/pins/CURRENT; do
  [[ ! -e "$cleanup_repo/$removed" ]] || cleanup_ok=0
done
[[ "${#cleanup_quarantines[@]}" -eq 1 ]] || cleanup_ok=0
if [[ "${#cleanup_quarantines[@]}" -eq 1 ]]; then
  [[ -d "${cleanup_quarantines[0]}/out" ]] || cleanup_ok=0
  [[ -d "${cleanup_quarantines[0]}/.git__worktrees" ]] || cleanup_ok=0
  [[ -f "${cleanup_quarantines[0]}/pin__anubis-not-a-pin" ]] || cleanup_ok=0
  [[ -f "${cleanup_quarantines[0]}/pin__.hidden-junk" ]] || cleanup_ok=0
  [[ -d "${cleanup_quarantines[0]}/pin__junk-dir" ]] || cleanup_ok=0
  [[ -L "${cleanup_quarantines[0]}/pin__anubis-cccccccccccc" ]] || cleanup_ok=0
  [[ -L "${cleanup_quarantines[0]}/scratchpad" ]] || cleanup_ok=0
fi
[[ -f "$cleanup_home/outside-scratchpad/keep" ]] || cleanup_ok=0
record vm_sync_cleanup_preserves_only_guest_cache "$cleanup_ok" \
  "rc=$cleanup_rc host-only trees quarantined"

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
for name in anubis-xcode anubis-warroom anubis-xcode-snapshot random-123; do
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
  '  list) echo "Source Name State"; echo "local anubis-xcode stopped" ;;' \
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
ANUBIS_OFFENSIVE_GATE_VM_MEM=24576 \
  bash "$ROOT/scripts/run_offensive_platform_gate.sh" --out "$TMP/offensive-overcap" \
  >"$TMP/offensive-overcap.log" 2>&1 &
gate_pid=$!
terminated_naturally=0
for _ in $(seq 1 40); do
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
record rejected_preflight_no_clone "$ok" "natural=$terminated_naturally rc=$rc tart_actions=$(tr '\n' ',' <"$TMP/fake-tart.log")"

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
