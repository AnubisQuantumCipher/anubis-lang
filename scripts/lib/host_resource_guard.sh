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
  local pressure free_mib
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
  if (( free_mib < ANUBIS_HOST_START_MIN_FREE_MIB )); then
    echo "ANUBIS_HOST_GUARD_HEADROOM: free=${free_mib}MiB required=${ANUBIS_HOST_START_MIN_FREE_MIB}MiB; refusing to start VM" >&2
    return 1
  fi
  printf 'ANUBIS_HOST_GUARD_PREFLIGHT: PASS cpu=%s mem=%sMiB free=%sMiB pressure=%s\n' "$1" "$2" "$free_mib" "$pressure"
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
    printf 'ANUBIS_HOST_GUARD: tart unavailable free=%sMiB pressure=%s\n' "$free_mib" "$pressure"
    return 0
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

  if [[ "$emergency" == 0 && "$ANUBIS_GUARD_QUIET_OK" != 1 ]]; then
    printf 'ANUBIS_HOST_GUARD_OK: free=%sMiB pressure=%s\n' "$free_mib" "$pressure"
  fi
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
