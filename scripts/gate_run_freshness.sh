#!/usr/bin/env bash
set -uo pipefail
# gate_run_freshness.sh — validate this seal's exact pre-freshness gate roster.
#
# The ledger is a run receipt under SEAL_OUT. It is never tracked source and is
# never reused to bootstrap another run, so every row must equal the final Git
# HEAD exactly; there is no cross-commit age allowance. A seal promotes
# `.working` to `.validated` only after the exact final validator succeeds.
#
# Required environment:
#   ANUBIS_GATE_RUN_LEDGER  absolute path to SEAL_OUT/gate_run_ledger.working
#   ANUBIS_GATE_RUN_PROFILE core|full
#   ANUBIS_SEAL_OUT         absolute private seal-output directory
#
# Usage:
#   scripts/gate_run_freshness.sh --stamp NAME
#   scripts/gate_run_freshness.sh
#   scripts/gate_run_freshness.sh --self-test
#
# exit 0 PASS, 1 malformed/incomplete/stale, 2 unconfigured

export PATH="/usr/bin:/bin:/usr/sbin:/sbin"
script_path="${BASH_SOURCE[0]}"
case "$script_path" in
  */*) script_dir="${script_path%/*}" ;;
  *) script_dir=. ;;
esac
ROOT="$(cd "$script_dir/.." && pwd -P)"; cd "$ROOT"

fail() { echo "GATE_RUN_FRESHNESS_ERROR: $*" >&2; return 1; }

resolve_trusted_git() {
  # A PATH-prepended shim can otherwise return an old HEAD while changing the
  # repository as a side effect. Resolve Git only from the OS default command
  # path, then require a real absolute executable instead of a symlink.
  local candidate
  candidate="$(PATH=/usr/bin:/bin:/usr/sbin:/sbin command -v git 2>/dev/null || true)"
  if [[ "$candidate" != /* || ! -f "$candidate" || -L "$candidate" || ! -x "$candidate" ]]; then
    fail "trusted system Git executable is unavailable"
    return 2
  fi
  GIT_BIN="$candidate"
  return 0
}

sanitize_git_environment() {
  # `git -C` and a trusted executable still honor repository-redirection
  # variables. None are legitimate for a seal rooted at this checked-out tree.
  unset GIT_DIR GIT_WORK_TREE GIT_COMMON_DIR GIT_INDEX_FILE
  unset GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_NAMESPACE
  unset GIT_REPLACE_REF_BASE GIT_CONFIG_PARAMETERS GIT_CONFIG_COUNT
  unset GIT_CEILING_DIRECTORIES GIT_DISCOVERY_ACROSS_FILESYSTEM GIT_EXEC_PATH
  export GIT_CONFIG_GLOBAL=/dev/null
  export GIT_CONFIG_SYSTEM=/dev/null
  export GIT_CONFIG_NOSYSTEM=1
  export GIT_ATTR_NOSYSTEM=1
  export GIT_NO_REPLACE_OBJECTS=1
  export GIT_OPTIONAL_LOCKS=0
}

load_context() {
  LEDGER="${ANUBIS_GATE_RUN_LEDGER:-}"
  PROFILE="${ANUBIS_GATE_RUN_PROFILE:-}"
  SEAL_OUT_CONTEXT="${ANUBIS_SEAL_OUT:-}"
  if [[ -z "$LEDGER" || -z "$PROFILE" || -z "$SEAL_OUT_CONTEXT" ]]; then
    echo "GATE_RUN_FRESHNESS: FAIL (unconfigured explicit seal-output ledger)" >&2
    return 2
  fi
  case "$PROFILE" in core|full) ;; *) fail "invalid profile: $PROFILE"; return 2 ;; esac
  if [[ "$LEDGER" != /* || "$SEAL_OUT_CONTEXT" != /* ]]; then
    fail "ledger and seal output must be absolute paths"
    return 2
  fi
  case "$LEDGER" in
    "${SEAL_OUT_CONTEXT%/}"/*) ;;
    *) fail "ledger must be inside ANUBIS_SEAL_OUT"; return 2 ;;
  esac
  ledger_parent="$(dirname "$LEDGER")"
  if [[ ! -d "$ledger_parent" || -L "$ledger_parent" ]]; then
    fail "ledger parent must be a real directory: $ledger_parent"
    return 2
  fi
  return 0
}

load_roster() {
  local roster_output roster_rc gate
  roster_output="$(/usr/bin/python3 scripts/lib/seal_verdict_validate.py \
    --profile "$PROFILE" --print-roster 2>&1)"
  roster_rc=$?
  if [[ $roster_rc -ne 0 ]]; then
    fail "could not derive exact seal roster: $roster_output"
    return 1
  fi
  GATES=()
  while IFS= read -r gate; do
    [[ -z "$gate" || "$gate" == "gate_run_freshness" ]] && continue
    GATES+=("$gate")
  done <<<"$roster_output"
  if [[ ${#GATES[@]} -eq 0 ]]; then
    fail "exact pre-freshness roster is empty"
    return 1
  fi
  return 0
}

gate_expected() {
  local wanted="$1" gate
  for gate in "${GATES[@]}"; do
    [[ "$gate" == "$wanted" ]] && return 0
  done
  return 1
}

validate_ledger() {
  local completeness="$1" line_number=0 name sha timestamp extra
  local first_sha="" duplicate=0 malformed=0 unknown=0
  local -a seen=()
  [[ -f "$LEDGER" && ! -L "$LEDGER" ]] || {
    fail "ledger missing or not a regular non-symlink file: $LEDGER"
    return 1
  }
  while IFS=' ' read -r name sha timestamp extra; do
    line_number=$((line_number + 1))
    if [[ -z "$name" || -z "$sha" || -z "$timestamp" || -n "${extra:-}" \
       || ! "$sha" =~ ^[0-9a-f]{40}$ || ! "$timestamp" =~ ^[0-9]+$ ]]; then
      echo "  FAIL: malformed ledger row $line_number" >&2
      malformed=1
      continue
    fi
    if ! gate_expected "$name"; then
      echo "  FAIL: unexpected ledger gate $name" >&2
      unknown=1
    fi
    local prior
    for prior in "${seen[@]+"${seen[@]}"}"; do
      [[ "$prior" == "$name" ]] && duplicate=1
    done
    seen+=("$name")
    if [[ -z "$first_sha" ]]; then
      first_sha="$sha"
    elif [[ "$sha" != "$first_sha" ]]; then
      echo "  FAIL: mixed commit epochs in ledger ($first_sha and $sha)" >&2
      malformed=1
    fi
  done < "$LEDGER"
  if [[ $line_number -eq 0 || $duplicate -ne 0 || $malformed -ne 0 || $unknown -ne 0 ]]; then
    [[ $duplicate -ne 0 ]] && echo "  FAIL: duplicate ledger gate" >&2
    return 1
  fi
  LEDGER_SHA="$first_sha"
  if [[ "$completeness" == "complete" ]]; then
    local gate found count=0
    for gate in "${GATES[@]}"; do
      found=0
      for name in "${seen[@]}"; do [[ "$name" == "$gate" ]] && found=1; done
      if [[ $found -ne 1 ]]; then
        echo "  FAIL: required gate missing from ledger: $gate" >&2
        return 1
      fi
      count=$((count + 1))
    done
    if [[ ${#seen[@]} -ne $count ]]; then
      echo "  FAIL: ledger row count ${#seen[@]} does not equal expected $count" >&2
      return 1
    fi
  fi
  return 0
}

stamp_gate() {
  local name="$1" head now tmp
  if ! gate_expected "$name"; then
    fail "cannot stamp gate outside exact pre-freshness roster: $name"
    return 1
  fi
  head="$("$GIT_BIN" rev-parse HEAD 2>/dev/null)"
  if [[ ! "$head" =~ ^[0-9a-f]{40}$ ]]; then
    fail "Git HEAD unavailable for stamp"
    return 1
  fi
  if [[ -e "$LEDGER" || -L "$LEDGER" ]]; then
    validate_ledger partial || return 1
    if [[ "$LEDGER_SHA" != "$head" ]]; then
      fail "existing ledger epoch $LEDGER_SHA does not match HEAD $head"
      return 1
    fi
    if grep -Eq "^${name}[[:space:]]" "$LEDGER"; then
      fail "duplicate stamp refused for gate $name"
      return 1
    fi
  fi
  now="$(date -u +%s)"
  tmp="$(mktemp "$(dirname "$LEDGER")/.gate-run-ledger.XXXXXX")" || return 1
  if [[ -f "$LEDGER" ]]; then cat "$LEDGER" > "$tmp"; fi
  printf '%s %s %s\n' "$name" "$head" "$now" >> "$tmp"
  LC_ALL=C sort "$tmp" -o "$tmp"
  chmod 0444 "$tmp"
  if [[ -f "$LEDGER" ]]; then chmod u+w "$LEDGER" 2>/dev/null || true; fi
  mv "$tmp" "$LEDGER"
  echo "stamped $name @ $head ($now)"
}

self_test() {
  local td before after rc gate
  td="$(mktemp -d "${TMPDIR:-/tmp}/anubis-gate-freshness.XXXXXX")" || return 1
  before="$("$GIT_BIN" status --porcelain=v1 --untracked-files=all 2>/dev/null)"
  ANUBIS_GATE_RUN_LEDGER="$td/missing.working" \
    ANUBIS_GATE_RUN_PROFILE=core ANUBIS_SEAL_OUT="$td" \
    bash "$0" >"$td/missing.out" 2>"$td/missing.err"
  rc=$?
  if [[ $rc -eq 0 ]]; then rm -rf "$td"; fail "self-test missing ledger was accepted"; return 1; fi
  local roster
  roster="$(python3 scripts/lib/seal_verdict_validate.py --profile core --print-roster)" || {
    rm -rf "$td"; return 1;
  }
  while IFS= read -r gate; do
    [[ "$gate" == "gate_run_freshness" ]] && continue
    ANUBIS_GATE_RUN_LEDGER="$td/gate_run_ledger.working" \
      ANUBIS_GATE_RUN_PROFILE=core ANUBIS_SEAL_OUT="$td" \
      bash "$0" --stamp "$gate" >"$td/stamp.out" 2>"$td/stamp.err"
    rc=$?
    if [[ $rc -ne 0 ]]; then rm -rf "$td"; fail "self-test stamp failed for $gate"; return 1; fi
  done <<<"$roster"
  ANUBIS_GATE_RUN_LEDGER="$td/gate_run_ledger.working" \
    ANUBIS_GATE_RUN_PROFILE=core ANUBIS_SEAL_OUT="$td" \
    bash "$0" >"$td/complete.out" 2>"$td/complete.err"
  rc=$?
  if [[ $rc -ne 0 ]]; then rm -rf "$td"; fail "self-test complete ledger failed"; return 1; fi
  chmod u+w "$td/gate_run_ledger.working"
  head -1 "$td/gate_run_ledger.working" >> "$td/gate_run_ledger.working"
  chmod 0444 "$td/gate_run_ledger.working"
  ANUBIS_GATE_RUN_LEDGER="$td/gate_run_ledger.working" \
    ANUBIS_GATE_RUN_PROFILE=core ANUBIS_SEAL_OUT="$td" \
    bash "$0" >"$td/duplicate.out" 2>"$td/duplicate.err"
  rc=$?
  if [[ $rc -eq 0 ]]; then rm -rf "$td"; fail "self-test duplicate ledger row was accepted"; return 1; fi
  after="$("$GIT_BIN" status --porcelain=v1 --untracked-files=all 2>/dev/null)"
  rm -rf "$td"
  if [[ "$before" != "$after" ]]; then fail "self-test mutated repository status"; return 1; fi
  echo "GATE_RUN_FRESHNESS SELFTEST: PASS"
  return 0
}

resolve_trusted_git || exit $?
sanitize_git_environment

if [[ "${1:-}" == "--self-test" ]]; then
  self_test
  exit $?
fi

load_context || exit $?
load_roster || exit 1

if [[ "${1:-}" == "--stamp" ]]; then
  [[ $# -eq 2 && -n "$2" ]] || { echo "usage: $0 --stamp GATE_NAME" >&2; exit 64; }
  stamp_gate "$2"
  exit $?
fi
if [[ $# -ne 0 ]]; then echo "usage: $0 [--stamp GATE_NAME|--self-test]" >&2; exit 64; fi

echo "GATE_RUN_FRESHNESS"
validate_ledger complete || { echo "GATE_RUN_FRESHNESS: FAIL"; exit 1; }
if ! "$GIT_BIN" cat-file -e "${LEDGER_SHA}^{commit}" 2>/dev/null; then
  echo "  FAIL: ledger epoch $LEDGER_SHA is not in this repository"
  echo "GATE_RUN_FRESHNESS: FAIL"
  exit 1
fi
HEAD_SHA="$("$GIT_BIN" rev-parse HEAD 2>/dev/null || true)"
if [[ ! "$HEAD_SHA" =~ ^[0-9a-f]{40}$ ]]; then
  echo "  FAIL: current Git HEAD is unavailable or malformed"
  echo "GATE_RUN_FRESHNESS: FAIL"
  exit 1
fi
if [[ "$LEDGER_SHA" != "$HEAD_SHA" ]]; then
  echo "  FAIL: run-local ledger epoch $LEDGER_SHA does not equal current HEAD $HEAD_SHA"
  echo "GATE_RUN_FRESHNESS: FAIL"
  exit 1
fi
echo "  ok: exact pre-freshness roster ${#GATES[@]}/${#GATES[@]} at one commit epoch"
echo "  ok: run-local ledger epoch exactly equals current HEAD $HEAD_SHA"
echo "GATE_RUN_FRESHNESS: PASS"
exit 0
