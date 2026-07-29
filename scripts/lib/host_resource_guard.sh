#!/usr/bin/env bash
# Fail-closed host protection for Anubis Tart/VZ workloads on macOS.
# Safe to source from gate scripts or execute as: host_resource_guard.sh once|watch.

ANUBIS_GUARD_MAX_CPU=8
ANUBIS_GUARD_MAX_MEM_MIB=12288
ANUBIS_HOST_MIN_FREE_MIB=8192
ANUBIS_HOST_START_MIN_FREE_MIB=12288
ANUBIS_GUARD_INTERVAL_SECS=5
ANUBIS_GUARD_TART_BIN="${ANUBIS_GUARD_TART_BIN:-$(command -v tart 2>/dev/null || true)}"
ANUBIS_GUARD_KEEP_DIR="${ANUBIS_GUARD_KEEP_DIR:-$HOME/Library/Application Support/Anubis/kept-vms}"
ANUBIS_GUARD_QUIET_OK="${ANUBIS_GUARD_QUIET_OK:-0}"

anubis_guard_mark_kept() {
  [[ $# -eq 1 && "$1" != */* ]] || return 2
  mkdir -p "$ANUBIS_GUARD_KEEP_DIR"
  : >"$ANUBIS_GUARD_KEEP_DIR/$1"
}
anubis_guard_is_kept() {
  [[ $# -eq 1 && "$1" != */* && -f "$ANUBIS_GUARD_KEEP_DIR/$1" ]]
}
anubis_guard_clear_kept() {
  [[ $# -eq 1 && "$1" != */* ]] || return 2
  rm -f "$ANUBIS_GUARD_KEEP_DIR/$1"
}

anubis_guard_validate_vm_limits() {
  if [[ $# -ne 2 ]]; then
    echo "ANUBIS_HOST_GUARD_INVALID: expected CPU and memory MiB" >&2
    return 2
  fi
  local cpu="$1" mem="$2"
  if [[ ! "$cpu" =~ ^[1-9][0-9]*$ || ! "$mem" =~ ^[1-9][0-9]*$ ]]; then
    echo "ANUBIS_HOST_GUARD_INVALID: CPU and memory must be positive integers (cpu=$cpu mem=$mem)" >&2
    return 2
  fi
  if (( cpu > ANUBIS_GUARD_MAX_CPU )); then
    echo "ANUBIS_HOST_GUARD_CPU_CEILING: requested=$cpu max=$ANUBIS_GUARD_MAX_CPU" >&2
    return 1
  fi
  if (( mem > ANUBIS_GUARD_MAX_MEM_MIB )); then
    echo "ANUBIS_HOST_GUARD_MEMORY_CEILING: requested=${mem}MiB max=${ANUBIS_GUARD_MAX_MEM_MIB}MiB" >&2
    return 1
  fi
}

# Copy the live repository into a disposable guest without traversing host-local
# artifact forests. These paths are not build/gate inputs: they are shared build
# outputs, agent worktrees, archived binary pins/exports, or scratch evidence.
# Keeping this seam here makes the full slice and offensive gate use one policy;
# a new launcher cannot silently reintroduce the measured 36-GiB cache blowout.
anubis_guard_sync_tree() {
  if [[ $# -ne 3 || -z "$1" || -z "$2" || -z "$3" ]]; then
    echo "ANUBIS_HOST_GUARD_INVALID: sync_tree requires RSYNC_RSH, source, and destination" >&2
    return 2
  fi
  local rsh="$1" source="$2" destination="$3" source_root remote_host remote_path remote_rel
  local -a rsh_argv pin_manifests
  source_root="${source%/}"
  if [[ "$source_root" != /* ]]; then source_root="$PWD/$source_root"; fi
  local source_component="$source_root"
  while [[ "$source_component" != / ]]; do
    if [[ -L "$source_component" ]]; then
      echo "ANUBIS_HOST_GUARD_SYNC_SOURCE: symlinked source component: $source_component" >&2
      return 2
    fi
    source_component="${source_component%/*}"
    [[ -n "$source_component" ]] || source_component=/
  done
  if [[ ! -d "$source_root" ]]; then
    echo "ANUBIS_HOST_GUARD_SYNC_SOURCE: source must be a real directory: $source" >&2
    return 2
  fi
  shopt -s nullglob
  pin_manifests=("$source_root"/vm/pins/CURRENT "$source_root"/vm/pins/*.meta)
  shopt -u nullglob
  local manifest
  for manifest in "${pin_manifests[@]}"; do
    if [[ -L "$manifest" ]]; then
      echo "ANUBIS_HOST_GUARD_SYNC_SYMLINK: refusing pin manifest symlink: $manifest" >&2
      return 2
    fi
  done
  if [[ "$destination" != *:* ]]; then
    echo "ANUBIS_HOST_GUARD_SYNC_DESTINATION: expected host:path destination" >&2
    return 2
  fi
  remote_host="${destination%%:*}"
  remote_path="${destination#*:}"
  case "$remote_path" in
    '~/'*) remote_rel="${remote_path#\~/}" ;;
    /*) echo "ANUBIS_HOST_GUARD_SYNC_DESTINATION: absolute remote path denied" >&2; return 2 ;;
    *) remote_rel="$remote_path" ;;
  esac
  remote_rel="${remote_rel%/}"
  if [[ ! "$remote_rel" =~ ^[A-Za-z0-9._/-]+$ \
    || "$remote_rel" == .. || "$remote_rel" == ../* \
    || "$remote_rel" == */../* || "$remote_rel" == */.. ]]; then
    echo "ANUBIS_HOST_GUARD_SYNC_DESTINATION: unsafe remote path" >&2
    return 2
  fi
  read -r -a rsh_argv <<<"$rsh"
  if (( ${#rsh_argv[@]} == 0 )); then
    echo "ANUBIS_HOST_GUARD_SYNC_SHELL: empty remote shell" >&2
    return 2
  fi
  if ! "${rsh_argv[@]}" "$remote_host" \
    "set -u; test ! -L \"\$HOME\" || exit 42; d=\"\$HOME\"; rel=\"$remote_rel\"; while test -n \"\$rel\"; do case \"\$rel\" in */*) part=\"\${rel%%/*}\"; rel=\"\${rel#*/}\" ;; *) part=\"\$rel\"; rel= ;; esac; test -n \"\$part\" || exit 42; d=\"\$d/\$part\"; test -d \"\$d\" || exit 42; test ! -L \"\$d\" || exit 42; done"; then
    echo "ANUBIS_HOST_GUARD_SYNC_DESTINATION: remote tree is missing or symlinked" >&2
    return 2
  fi
  RSYNC_RSH="$rsh" rsync -aH --delete --delete-excluded --no-devices --no-specials \
    --exclude=target/ \
    --exclude=out/ \
    --exclude=implementer/a_plus_audit_run/ \
    --exclude=.claude/worktrees/ \
    --include=vm/pins/ \
    --include='vm/pins/*.meta' \
    --include=vm/pins/CURRENT \
    --exclude='vm/pins/*' \
    --exclude=vm/exports/ \
    --exclude=scratchpad/ \
    --exclude=.DS_Store \
    -- "$source" "$destination"
}

# Parse vm_stat from stdin and report immediately unused memory. Inactive/cache
# pages are deliberately excluded: the prior WindowServer watchdog event occurred
# with memoryPressure=false, so pressure/reclaimability alone is not a safe signal.
anubis_guard_free_mib_from_vm_stat() {
  awk '
    /page size of [0-9]+ bytes/ {
      line=$0; sub(/^.*page size of /, "", line); sub(/ bytes.*$/, "", line); page=line+0
    }
    /^Pages free:/ { value=$3; gsub(/\./, "", value); free=value+0 }
    /^Pages speculative:/ { value=$3; gsub(/\./, "", value); speculative=value+0 }
    END {
      if (page <= 0) exit 2
      printf "%d\n", int(((free + speculative) * page) / 1048576)
    }
  '
}

anubis_guard_generated_owner_pid() {
  if [[ $# -ne 1 ]]; then return 2; fi
  case "$1" in
    anubis-run-[0-9]*|anubis-offensive-gate-[0-9]*|anubis-poc-kit-gate-[0-9]*|anubis-vz-ephemeral-[0-9]*)
      local pid="${1##*-}"
      [[ "$pid" =~ ^[1-9][0-9]*$ ]] || return 1
      printf '%s\n' "$pid"
      ;;
    *) return 1 ;;
  esac
}

anubis_guard_read_vm_stat() { /usr/bin/vm_stat; }
anubis_guard_read_pressure() { /usr/sbin/sysctl -n kern.memorystatus_vm_pressure_level; }
anubis_guard_read_tart_json() {
  [[ -n "$ANUBIS_GUARD_TART_BIN" ]] || return 1
  "$ANUBIS_GUARD_TART_BIN" list --source local --format json
}
anubis_guard_tart_stop() {
  "$ANUBIS_GUARD_TART_BIN" stop "$1" --timeout 5 >/dev/null 2>&1
}
anubis_guard_tart_delete() {
  "$ANUBIS_GUARD_TART_BIN" delete "$1" >/dev/null 2>&1
}
anubis_guard_owner_alive() { kill -0 "$1" >/dev/null 2>&1; }

anubis_guard_json_rows() {
  /usr/bin/python3 -c '
import json, sys
for vm in json.load(sys.stdin):
    name = vm.get("Name")
    if isinstance(name, str):
        print(f"{name}\t{1 if vm.get(chr(82)+chr(117)+chr(110)+chr(110)+chr(105)+chr(110)+chr(103)) else 0}")
'
}

anubis_guard_preflight() {
  if [[ $# -ne 2 ]]; then
    echo "ANUBIS_HOST_GUARD_INVALID: preflight requires CPU and memory MiB" >&2
    return 2
  fi
  anubis_guard_validate_vm_limits "$1" "$2" || return
  local pressure free_mib required_mib
  pressure="$(anubis_guard_read_pressure 2>/dev/null)" || {
    echo "ANUBIS_HOST_GUARD_UNREADABLE: cannot read macOS memory pressure" >&2
    return 1
  }
  free_mib="$(anubis_guard_read_vm_stat | anubis_guard_free_mib_from_vm_stat)" || {
    echo "ANUBIS_HOST_GUARD_UNREADABLE: cannot calculate immediately free memory" >&2
    return 1
  }
  if [[ "$pressure" != 1 ]]; then
    echo "ANUBIS_HOST_GUARD_PRESSURE: level=$pressure; refusing to start VM" >&2
    return 1
  fi
  required_mib=$(( $2 + ANUBIS_HOST_MIN_FREE_MIB ))
  if (( required_mib < ANUBIS_HOST_START_MIN_FREE_MIB )); then
    required_mib=$ANUBIS_HOST_START_MIN_FREE_MIB
  fi
  if (( free_mib < required_mib )); then
    echo "ANUBIS_HOST_GUARD_HEADROOM: free=${free_mib}MiB required=${required_mib}MiB (guest=$2MiB + host_reserve=${ANUBIS_HOST_MIN_FREE_MIB}MiB); refusing to start VM" >&2
    return 1
  fi
  printf 'ANUBIS_HOST_GUARD_PREFLIGHT: PASS cpu=%s mem=%sMiB free=%sMiB required=%sMiB pressure=%s\n' "$1" "$2" "$free_mib" "$required_mib" "$pressure"
}

anubis_guard_launchctl_print() {
  launchctl print "gui/${UID}/com.anubis.host-resource-guard"
}

anubis_guard_require_launchagent() {
  local status
  status="$(anubis_guard_launchctl_print 2>/dev/null)" || {
    echo "ANUBIS_HOST_GUARD_LAUNCHAGENT_MISSING: com.anubis.host-resource-guard is not loaded" >&2
    return 1
  }
  if ! grep -Eq 'state[[:space:]]*=[[:space:]]*running' <<<"$status"; then
    echo "ANUBIS_HOST_GUARD_LAUNCHAGENT_INACTIVE: com.anubis.host-resource-guard is not running" >&2
    return 1
  fi
  printf 'ANUBIS_HOST_GUARD_LAUNCHAGENT: PASS\n'
}

anubis_guard_guest_absent() {
  if [[ $# -ne 1 || -z "$1" ]]; then
    echo "ANUBIS_HOST_GUARD_INVALID: guest_absent requires a guest name" >&2
    return 2
  fi
  local json rows name running
  json="$(anubis_guard_read_tart_json 2>/dev/null)" || {
    echo "ANUBIS_HOST_GUARD_UNAVAILABLE: cannot verify guest teardown" >&2
    return 1
  }
  rows="$(printf '%s\n' "$json" | anubis_guard_json_rows)" || return 1
  while IFS=$'\t' read -r name running; do
    if [[ "$name" == "$1" ]]; then
      echo "ANUBIS_HOST_GUARD_GUEST_SURVIVED: $1" >&2
      return 1
    fi
  done <<<"$rows"
}

anubis_guard_start_caffeinate() {
  local parent_pid="${1:-$$}"
  /usr/bin/caffeinate -dimsu -w "$parent_pid" >/dev/null 2>&1 &
  ANUBIS_GUARD_CAFFEINATE_PID=$!
  export ANUBIS_GUARD_CAFFEINATE_PID
}

anubis_guard_watch_once() {
  local pressure free_mib json rows emergency=0 name running owner
  pressure="$(anubis_guard_read_pressure 2>/dev/null)" || pressure=unknown
  free_mib="$(anubis_guard_read_vm_stat | anubis_guard_free_mib_from_vm_stat 2>/dev/null)" || free_mib=0
  json="$(anubis_guard_read_tart_json 2>/dev/null)" || {
    printf 'ANUBIS_HOST_GUARD_UNAVAILABLE: tart inventory unreadable free=%sMiB pressure=%s\n' "$free_mib" "$pressure" >&2
    return 1
  }
  rows="$(printf '%s\n' "$json" | anubis_guard_json_rows)" || {
    echo "ANUBIS_HOST_GUARD: invalid tart JSON; no destructive action taken" >&2
    return 1
  }

  if [[ "$pressure" != 1 ]] || (( free_mib < ANUBIS_HOST_MIN_FREE_MIB )); then
    emergency=1
    while IFS=$'\t' read -r name running; do
      [[ -n "$name" && "$running" == 1 ]] || continue
      printf 'ANUBIS_HOST_GUARD_EMERGENCY: stopping VM=%s free=%sMiB pressure=%s threshold=%sMiB\n' \
        "$name" "$free_mib" "$pressure" "$ANUBIS_HOST_MIN_FREE_MIB"
      anubis_guard_tart_stop "$name" || \
        printf 'ANUBIS_HOST_GUARD_STOP_FAILED: VM=%s\n' "$name" >&2
    done <<<"$rows"
  fi

  # Generated clones encode their creator PID. If that owner vanished, the clone
  # is an orphan. Named/base/snapshot VMs never match and are never auto-deleted.
  while IFS=$'\t' read -r name running; do
    [[ -n "$name" ]] || continue
    owner="$(anubis_guard_generated_owner_pid "$name" 2>/dev/null)" || continue
    anubis_guard_is_kept "$name" && continue
    anubis_guard_owner_alive "$owner" && continue
    if [[ "$running" == 1 && "$emergency" == 0 ]]; then
      printf 'ANUBIS_HOST_GUARD_ORPHAN: stopping VM=%s owner_pid=%s\n' "$name" "$owner"
      anubis_guard_tart_stop "$name" || {
        printf 'ANUBIS_HOST_GUARD_STOP_FAILED: orphan VM=%s\n' "$name" >&2
        continue
      }
    fi
    printf 'ANUBIS_HOST_GUARD_ORPHAN: deleting VM=%s owner_pid=%s\n' "$name" "$owner"
    anubis_guard_tart_delete "$name" || \
      printf 'ANUBIS_HOST_GUARD_DELETE_FAILED: VM=%s\n' "$name" >&2
  done <<<"$rows"

  if [[ "$emergency" != 0 ]]; then
    return 1
  fi

  if [[ "$emergency" == 0 && "$ANUBIS_GUARD_QUIET_OK" != 1 ]]; then
    printf 'ANUBIS_HOST_GUARD_OK: free=%sMiB pressure=%s\n' "$free_mib" "$pressure"
  fi
}

anubis_guard_start_runtime_watch() {
  local parent_pid="${1:-$$}"
  if [[ ! "$parent_pid" =~ ^[1-9][0-9]*$ ]] || ! kill -0 "$parent_pid" 2>/dev/null; then
    echo "ANUBIS_HOST_GUARD_INVALID: runtime watch requires a live parent PID" >&2
    return 2
  fi
  anubis_guard_watch_once || {
    echo "ANUBIS_HOST_GUARD_RUNTIME_START_FAILED: initial watch check failed" >&2
    return 1
  }
  (
    while kill -0 "$parent_pid" 2>/dev/null; do
      sleep "$ANUBIS_GUARD_INTERVAL_SECS"
      if ! anubis_guard_watch_once; then
        echo "ANUBIS_HOST_GUARD_RUNTIME_TRIPPED: terminating owner_pid=$parent_pid" >&2
        kill -TERM "$parent_pid" 2>/dev/null || true
        exit 1
      fi
    done
  ) &
  ANUBIS_GUARD_RUNTIME_WATCH_PID=$!
  export ANUBIS_GUARD_RUNTIME_WATCH_PID
  kill -0 "$ANUBIS_GUARD_RUNTIME_WATCH_PID" 2>/dev/null || {
    echo "ANUBIS_HOST_GUARD_RUNTIME_START_FAILED: watcher exited during startup" >&2
    return 1
  }
}

anubis_guard_stop_runtime_watch() {
  local watch_pid="${ANUBIS_GUARD_RUNTIME_WATCH_PID:-}"
  [[ -n "$watch_pid" ]] || return 0
  kill "$watch_pid" 2>/dev/null || true
  wait "$watch_pid" 2>/dev/null || true
  ANUBIS_GUARD_RUNTIME_WATCH_PID=""
  export ANUBIS_GUARD_RUNTIME_WATCH_PID
}

anubis_guard_watch() {
  printf 'ANUBIS_HOST_GUARD_STARTED: interval=%ss min_free=%sMiB max_cpu=%s max_mem=%sMiB\n' \
    "$ANUBIS_GUARD_INTERVAL_SECS" "$ANUBIS_HOST_MIN_FREE_MIB" "$ANUBIS_GUARD_MAX_CPU" "$ANUBIS_GUARD_MAX_MEM_MIB"
  while :; do
    anubis_guard_watch_once || true
    sleep "$ANUBIS_GUARD_INTERVAL_SECS"
  done
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  set -u
  case "${1:-once}" in
    once) anubis_guard_watch_once ;;
    watch) anubis_guard_watch ;;
    preflight) anubis_guard_preflight "${2:-}" "${3:-}" ;;
    *) echo "usage: $0 [once|watch|preflight CPU MEM_MIB]" >&2; exit 2 ;;
  esac
fi
