#!/usr/bin/env bash
# Anubis end-of-arc SEAL CHECKLIST — executable, fail-closed, third-party runnable.
#
# One command that:
#   1. Resolves the current published immutable pin once (unless --bin / SEAL_BIN is supplied)
#   2. Pins that exact binary (copy + ANUBIS_BIN) for every subsequent gate
#   3. Runs every seal-trustworthy gate with PRIVATE --out dirs, in order
#   4. Records binary identity (path, mtime, size, sha256) in the seal root
#   5. Scores each gate by its DECLARED VERDICT LINE only — never by grepping
#      the log body for FAIL (fixture rows contain exp=FAIL and would false-alarm)
#   6. CORPUS COMPLETENESS: for fixture gates, reported fixture count must equal
#      on-disk corpus size — catches truncated mid-run (unmatched-grep under
#      pipefail) that still prints a green final line. Lesson: c04 capset case.
#   7. Runs KNOWN-FAILING gates honestly when any remain (expect declared FAIL)
#   8. REFUSES overall PASS if any instrument precondition is unmet
#
# Usage:
#   bash scripts/run_seal_checklist.sh [--out DIR] [--bin PATH] [--profile core|full]
#
# Profiles:
#   core (default) — seal decision spine (full suite green 2026-07-27):
#       security, language, runtime, check/run parity, stdlib fail-closed,
#       run fail-closed meta (PASS_INSTRUMENTED; refuses WHOLE while inventory open),
#       capset registry parity, formal, native-authoritative, selfhost,
#       taint selfhost, capset selfhost
#   full — core + type/effect selfhost + dogfood + fulllang
#       (+ capset corpus if ANUBIS_SEAL_CAPSET_CORPUS=1)
#       (+ DDC if ANUBIS_SEAL_DDC=1)
#
# Explicitly NOT required green (special host / hollow-skip risks):
#   metal prove, proof_binding, offensive platform (VZ).
#
# Exit codes:
#   0   SEAL_PASS   — all required gates PASS; known-failing still FAIL as documented
#   1   SEAL_FAIL   — a required gate failed, or a known-failing gate unexpectedly PASSed
#   2   SEAL_REFUSED — instrument precondition unmet (no false PASS possible)
# 127   SEAL_SETUP
#
# Authority: grok_seshat_instruments.md + rounds 2–8.
set -euo pipefail

# When re-exec'd from a private snapshot under /tmp, $0 no longer points at the repo.
# Prefer the pinned repo root from the first invocation.
if [[ -n "${ANUBIS_SEAL_ROOT:-}" && -d "${ANUBIS_SEAL_ROOT}/scripts" ]]; then
  ROOT="$ANUBIS_SEAL_ROOT"
else
  ROOT="$(cd "$(dirname "$0")/.." && pwd)"
fi
cd "$ROOT"

# Bash re-reads the script file from disk between commands. Concurrent agents editing
# this file mid-seal produce garbage next lines (`--: command not found`, `le: command
# not found`) with no seal_verdict.json (Seshat R8). Snapshot + re-exec from a private
# immutable copy before any long gate work. Keep ANUBIS_SEAL_ROOT so the snap still
# resolves the real repo.
if [[ "${ANUBIS_SEAL_REEXEC:-}" != "1" ]]; then
  _seal_snap_dir="${TMPDIR:-/tmp}/anubis_seal_script_$$"
  mkdir -p "$_seal_snap_dir"
  cp -p "$ROOT/scripts/run_seal_checklist.sh" "$_seal_snap_dir/run_seal_checklist.sh"
  chmod +x "$_seal_snap_dir/run_seal_checklist.sh"
  export ANUBIS_SEAL_REEXEC=1
  export ANUBIS_SEAL_ROOT="$ROOT"
  export ANUBIS_SEAL_SCRIPT_SNAP="$_seal_snap_dir/run_seal_checklist.sh"
  exec bash "$_seal_snap_dir/run_seal_checklist.sh" "$@"
fi

# Any unexpected command failure must not leave a partial tree looking green.
trap 'rc=$?; if [[ $rc -ne 0 && $rc -ne 2 && $rc -ne 127 ]]; then
  echo "SEAL_REFUSED: unexpected exit rc=$rc at line $LINENO (no false PASS)" >&2
  if [[ -n "${SEAL_OUT:-}" && -d "${SEAL_OUT:-}" ]]; then
    echo "SEAL_REFUSED: unexpected exit rc=$rc line=$LINENO" >>"${SEAL_OUT}/seal_summary.txt" 2>/dev/null || true
    printf "%s\n" "{\"status\":\"SEAL_REFUSED\",\"detail\":\"unexpected_exit_rc=${rc}_line=${LINENO}\"}" \
      >"${SEAL_OUT}/seal_verdict.json" 2>/dev/null || true
  fi
fi' ERR

STAMP="$(date +%Y%m%dT%H%M%S)_$$"
SEAL_OUT=""
SEAL_BIN_ARG=""
PROFILE="core"

usage() {
  sed -n '2,45p' "$0" | sed 's/^# \{0,1\}//'
  exit 2
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out) SEAL_OUT="$2"; shift 2 ;;
    --bin) SEAL_BIN_ARG="$2"; shift 2 ;;
    --profile) PROFILE="$2"; shift 2 ;;
    -h|--help) usage ;;
    *) echo "unknown arg: $1" >&2; usage ;;
  esac
done

case "$PROFILE" in
  core|full) ;;
  *) echo "SEAL_REFUSED: unknown --profile $PROFILE (want core|full)" >&2; exit 2 ;;
esac

if [[ -z "$SEAL_OUT" ]]; then
  SEAL_OUT="$ROOT/out/seal_${STAMP}"
fi
if [[ "$SEAL_OUT" != /* ]]; then
  SEAL_OUT="$ROOT/$SEAL_OUT"
fi
mkdir -p "$SEAL_OUT/gates" "$SEAL_OUT/logs"
SUMMARY="$SEAL_OUT/seal_summary.txt"
VERDICT_JSON="$SEAL_OUT/seal_verdict.json"
INSTRUMENT="$SEAL_OUT/instrument.txt"
KNOWN_FAILING_MD="$SEAL_OUT/known_failing.md"
: >"$SUMMARY"

log() { echo "$*" | tee -a "$SUMMARY"; }

# ── KNOWN-FAILING registry ───────────────────────────────────────────────────
# These gates are RUN under the pin and scored. Expected outcome is declared FAIL.
# Unexpected PASS flips the seal to SEAL_FAIL (status changed without list update).
# Format per entry (pipe-separated):
#   name|pass_re|fail_re|reason|command...
# Command is the remainder after the 4th field — stored separately below.

write_verdict() {
  local status="$1" detail="$2"
  if ! python3 - "$VERDICT_JSON" "$status" "$detail" "$SEAL_OUT" "$PROFILE" <<'PY'
import json, sys, pathlib
path, status, detail, seal_out, profile = sys.argv[1:6]
inst = {}
ip = pathlib.Path(seal_out) / "instrument.txt"
if ip.is_file():
    inst["raw"] = ip.read_text()
gates = []
gdir = pathlib.Path(seal_out) / "gates"
if gdir.is_dir():
    for p in sorted(gdir.glob("*.status")):
        row = {"name": p.stem, "status": p.read_text().strip()}
        for suf, key in (
            (".verdict_line", "declared_verdict_line"),
            (".score_reason", "score_reason"),
            (".known_fail_reason", "known_fail_reason"),
        ):
            f = gdir / (p.stem + suf)
            if f.is_file():
                row[key] = f.read_text().strip()
        gates.append(row)
kf = pathlib.Path(seal_out) / "known_failing.md"
m = {
    "gate": "seal_checklist",
    "status": status,
    "detail": detail,
    "profile": profile,
    "seal_out": seal_out,
    "instrument": inst,
    "gates": gates,
    "scoring_rule": "declared_verdict_line_only_never_body_grep_FAIL",
    "known_failing_manifest": kf.read_text() if kf.is_file() else None,
}
pathlib.Path(path).write_text(json.dumps(m, indent=2) + "\n")
PY
  then
    printf '{"status":"%s","detail":"%s","profile":"%s"}\n' "$status" "$detail" "$PROFILE" >"$VERDICT_JSON"
  fi
}

refuse() {
  log "SEAL_REFUSED: $*"
  write_verdict "SEAL_REFUSED" "$*"
  exit 2
}
die_setup() {
  log "SEAL_SETUP: $*"
  write_verdict "SEAL_SETUP" "$*"
  exit 127
}

# ── Preconditions ────────────────────────────────────────────────────────────
command -v python3 >/dev/null 2>&1 || die_setup "python3 required for seal verdict JSON"
if ! command -v shasum >/dev/null 2>&1 && ! command -v sha256sum >/dev/null 2>&1; then
  die_setup "shasum or sha256sum required for continuous pin verification"
fi
if ! command -v timeout >/dev/null 2>&1 && ! command -v gtimeout >/dev/null 2>&1; then
  die_setup "neither timeout nor gtimeout on PATH (parity/runtime/native gates require it)"
fi

REQUIRED_GATE_SCRIPTS=(
  scripts/run_security_fixtures.sh
  scripts/run_language_fixtures.sh
  scripts/run_runtime_fixtures.sh
  scripts/run_check_run_parity_gate.sh
  scripts/run_stdlib_failclosed_gate.sh
  scripts/run_run_failclosed_gate.sh
  scripts/check_capset_registry_parity.sh
  scripts/check_gate_common_adoption.sh
  scripts/run_formal_gate.sh
  scripts/run_native_authoritative_gate.sh
  scripts/run_selfhost_gate.sh
  scripts/run_taint_selfhost_gate.sh
  scripts/run_capset_selfhost_gate.sh
)
for required_script in "${REQUIRED_GATE_SCRIPTS[@]}"; do
  [[ -f "$required_script" ]] || refuse "required gate script missing: $required_script"
done

log "==== ANUBIS SEAL CHECKLIST ===="
log "profile=$PROFILE"
log "seal_out=$SEAL_OUT"
log "cwd=$ROOT"
log "host=$(uname -s) $(uname -m)"
log "date=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
log "scoring=declared_verdict_line_only (never body-grep FAIL — exp=FAIL columns are noise)"

# ── Resolve once / pin ───────────────────────────────────────────────────────
SNAP="$SEAL_OUT/anubis.snap"
SOURCE_PIN=""
if [[ -n "$SEAL_BIN_ARG" ]]; then
  [[ -x "$SEAL_BIN_ARG" ]] || refuse "SEAL_BIN / --bin not executable: $SEAL_BIN_ARG"
  cp -p "$SEAL_BIN_ARG" "$SNAP"
  SOURCE_PIN="$SEAL_BIN_ARG"
  log "using pre-pinned --bin=$SEAL_BIN_ARG"
elif [[ -n "${SEAL_BIN:-}" ]]; then
  [[ -x "$SEAL_BIN" ]] || refuse "SEAL_BIN env not executable: $SEAL_BIN"
  cp -p "$SEAL_BIN" "$SNAP"
  SOURCE_PIN="$SEAL_BIN"
  log "using pre-pinned SEAL_BIN=$SEAL_BIN"
else
  [[ -x scripts/publish_pin.sh ]] || refuse "scripts/publish_pin.sh missing or not executable"
  set +e
  SOURCE_PIN="$(scripts/publish_pin.sh --current 2>"$SEAL_OUT/logs/publish_pin_current.err")"
  pin_rc=$?
  set -e
  [[ $pin_rc -eq 0 ]] || refuse "could not resolve published pin (rc=$pin_rc; see logs/publish_pin_current.err)"
  [[ -x "$SOURCE_PIN" ]] || refuse "published pin not executable: $SOURCE_PIN"
  cp -p "$SOURCE_PIN" "$SNAP"
  log "using published current pin=$SOURCE_PIN"
fi
chmod +x "$SNAP" 2>/dev/null || true
chmod a-w "$SNAP" 2>/dev/null || refuse "could not make snapshot read-only: $SNAP"
[[ -x "$SNAP" ]] || refuse "snapshot binary not executable: $SNAP"

BIN_MTIME="$(stat -f '%Sm' -t '%Y-%m-%dT%H:%M:%S' "$SNAP" 2>/dev/null || stat -c '%y' "$SNAP" 2>/dev/null || echo unknown)"
BIN_SIZE="$(stat -f '%z' "$SNAP" 2>/dev/null || stat -c '%s' "$SNAP" 2>/dev/null || echo 0)"
if command -v shasum >/dev/null 2>&1; then
  SHA_TOOL="$(command -v shasum)"
  SHA_MODE="shasum"
  BIN_SHA="$(shasum -a 256 "$SNAP" | awk '{print $1}')"
elif command -v sha256sum >/dev/null 2>&1; then
  SHA_TOOL="$(command -v sha256sum)"
  SHA_MODE="sha256sum"
  BIN_SHA="$(sha256sum "$SNAP" | awk '{print $1}')"
else
  refuse "no SHA-256 implementation available after precondition"
fi

PIN_WRAPPER="$SEAL_OUT/anubis.pin"
PIN_TRACE="$SEAL_OUT/pin_invocations.log"
: >"$PIN_TRACE"
{
  # Some gates intentionally hide PATH to prove external solvers are not load-bearing.
  # The launcher must remain executable in that environment.
  echo '#!/bin/bash'
  echo 'set -euo pipefail'
  printf 'actual=%q\n' "$SNAP"
  printf 'expected_sha=%q\n' "$BIN_SHA"
  printf 'trace=%q\n' "$PIN_TRACE"
  printf 'hash_tool=%q\n' "$SHA_TOOL"
  printf 'hash_mode=%q\n' "$SHA_MODE"
  echo 'if [[ "$hash_mode" == "shasum" ]]; then'
  echo '  read -r current_sha _ < <("$hash_tool" -a 256 "$actual")'
  echo 'else'
  echo '  read -r current_sha _ < <("$hash_tool" "$actual")'
  echo 'fi'
  echo 'if [[ "$current_sha" != "$expected_sha" ]]; then'
  echo '  echo "SEAL_PIN_HASH_MISMATCH: expected=$expected_sha actual=$current_sha path=$actual" >&2'
  echo '  exit 125'
  echo 'fi'
  echo 'active_gate="${ANUBIS_SEAL_ACTIVE_GATE:-unattributed}"'
  echo 'printf "%s|%s|%s\n" "$active_gate" "$current_sha" "$actual" >>"$trace"'
  echo 'exec "$actual" "$@"'
} >"$PIN_WRAPPER"
chmod +x "$PIN_WRAPPER"

export ANUBIS_BIN="$PIN_WRAPPER"
export ANUBIS="$PIN_WRAPPER"

{
  echo "seal_instrument_v1"
  echo "path=$SNAP"
  echo "source_pin=$SOURCE_PIN"
  echo "mtime=$BIN_MTIME"
  echo "size_bytes=$BIN_SIZE"
  echo "sha256=$BIN_SHA"
  echo "launcher=$PIN_WRAPPER"
  echo "invocation_trace=$PIN_TRACE"
  echo "ANUBIS_BIN=$ANUBIS_BIN"
  echo "profile=$PROFILE"
} | tee "$INSTRUMENT" | tee -a "$SUMMARY"

if [[ "$BIN_SIZE" =~ ^[0-9]+$ ]] && [[ "$BIN_SIZE" -lt 1000000 ]]; then
  refuse "snapshot binary too small ($BIN_SIZE bytes) — not a real anubis release binary"
fi

# ── Declared-verdict scoring ─────────────────────────────────────────────────
# NEVER: grep -i FAIL "$log"  — fixture tables emit exp=FAIL and cause false alarms.
# ONLY: last line matching the gate's declared tag (PASS_RE / FAIL_RE).

gate_pass=0
gate_fail=0
gate_skip=0
gate_known_fail=0
gate_known_unexpected_pass=0
declare -a GATE_ROWS=()

extract_declared() {
  local logf="$1" re="$2"
  grep -E "$re" "$logf" 2>/dev/null | tail -1 || true
}

# classify_verdict LOG PASS_RE FAIL_RE → globals: _v_status _v_line _v_reason
classify_verdict() {
  local logf="$1" pass_re="$2" fail_re="$3"
  _v_status=""
  _v_line=""
  _v_reason=""

  local pass_line fail_line
  pass_line="$(extract_declared "$logf" "$pass_re")"
  fail_line="$(extract_declared "$logf" "$fail_re")"

  local pass_ln=0 fail_ln=0
  if [[ -n "$pass_line" ]]; then
    pass_ln="$(grep -nE "$pass_re" "$logf" 2>/dev/null | tail -1 | cut -d: -f1 || echo 0)"
  fi
  if [[ -n "$fail_line" ]]; then
    fail_ln="$(grep -nE "$fail_re" "$logf" 2>/dev/null | tail -1 | cut -d: -f1 || echo 0)"
  fi
  pass_ln="${pass_ln:-0}"
  fail_ln="${fail_ln:-0}"

  if [[ -n "$fail_line" && "$fail_ln" -ge "$pass_ln" ]]; then
    _v_status="FAIL"
    _v_line="$fail_line"
    _v_reason="declared_FAIL_line"
    return 0
  fi
  if [[ -n "$pass_line" ]]; then
    _v_status="PASS"
    _v_line="$pass_line"
    _v_reason="declared_PASS_line"
    return 0
  fi
  if [[ -n "$fail_line" ]]; then
    _v_status="FAIL"
    _v_line="$fail_line"
    _v_reason="declared_FAIL_line"
    return 0
  fi
  _v_status="FAIL"
  _v_line=""
  _v_reason="no_declared_verdict_line"
}

_instrument_guards() {
  if [[ "${ANUBIS_BIN:-}" != "$PIN_WRAPPER" ]]; then
    refuse "ANUBIS_BIN drifted from seal launcher (now=${ANUBIS_BIN:-unset})"
  fi
  if [[ ! -x "$ANUBIS_BIN" || ! -x "$SNAP" ]]; then
    refuse "seal launcher or snapshot not executable mid-seal"
  fi
  local now_size now_sha
  now_size="$(stat -f '%z' "$SNAP" 2>/dev/null || stat -c '%s' "$SNAP" 2>/dev/null || echo 0)"
  if [[ "$now_size" != "$BIN_SIZE" ]]; then
    refuse "snapshot size changed mid-seal (was $BIN_SIZE now $now_size)"
  fi
  if command -v shasum >/dev/null 2>&1; then
    now_sha="$(shasum -a 256 "$SNAP" | awk '{print $1}')"
  else
    now_sha="$(sha256sum "$SNAP" | awk '{print $1}')"
  fi
  if [[ "$now_sha" != "$BIN_SHA" ]]; then
    refuse "snapshot sha256 changed mid-seal (was $BIN_SHA now $now_sha)"
  fi
}

# run_gate NAME PASS_RE FAIL_RE [--] command...
# Required-green gate: must declare PASS (and rc consistent).
# Leading `--` after FAIL_RE is optional (avoid bare `--` as a top-level command if
# a line-continuation is ever broken by concurrent edit).
run_gate() {
  local name="$1" pass_re="$2" fail_re="$3"
  shift 3
  local pin_required=1
  if [[ "${1:-}" == "--no-pin-use" ]]; then pin_required=0; shift; fi
  while [[ "${1:-}" == "--" ]]; do shift; done
  if [[ $# -eq 0 ]]; then
    log "GATE $name: FAIL (run_gate invoked with no command)"
    echo "FAIL" >"$SEAL_OUT/gates/${name}.status"
    gate_fail=$((gate_fail + 1))
    GATE_ROWS+=("$name|FAIL|no_command|rc=n/a")
    return 0
  fi

  local gdir="$SEAL_OUT/gates/$name"
  local glog="$SEAL_OUT/logs/${name}.log"
  mkdir -p "$gdir"
  log ""
  log "---- GATE $name (required PASS) ----"
  log "cmd: $*"
  log "pass_re: $pass_re"
  log "fail_re: $fail_re"

  _instrument_guards

  local pin_before pin_after
  pin_before="$(grep -c -F "${name}|" "$PIN_TRACE" 2>/dev/null || true)"

  set +e
  env ANUBIS_BIN="$PIN_WRAPPER" ANUBIS="$PIN_WRAPPER" \
    ANUBIS_SEAL_ACTIVE_GATE="$name" "$@" >"$glog" 2>&1
  local rc=$?
  set -e

  _instrument_guards
  pin_after="$(grep -c -F "${name}|" "$PIN_TRACE" 2>/dev/null || true)"

  classify_verdict "$glog" "$pass_re" "$fail_re"
  printf '%s\n' "${_v_line:-}" >"$SEAL_OUT/gates/${name}.verdict_line"
  printf '%s\n' "$_v_reason" >"$SEAL_OUT/gates/${name}.score_reason"

  local final="$_v_status"
  if [[ "$_v_reason" == "no_declared_verdict_line" ]]; then
    final="FAIL"
    log "GATE $name: FAIL (no declared verdict line — unscorable; rc=$rc)"
    tail -15 "$glog" | sed 's/^/  | /' | tee -a "$SUMMARY" || true
  elif [[ "$_v_status" == "PASS" && $rc -ne 0 ]]; then
    final="FAIL"
    log "GATE $name: FAIL (declared PASS but rc=$rc — inconsistent)"
    log "  declared: $_v_line"
  elif [[ "$_v_status" == "FAIL" && $rc -eq 0 ]]; then
    final="FAIL"
    log "GATE $name: FAIL (declared FAIL but rc=0 — inconsistent hollow exit)"
    log "  declared: $_v_line"
  elif [[ "$final" == "PASS" ]]; then
    log "GATE $name: PASS"
    log "  declared: $_v_line"
  else
    log "GATE $name: FAIL"
    log "  declared: ${_v_line:-(none)}"
    tail -25 "$glog" | sed 's/^/  | /' | tee -a "$SUMMARY" || true
  fi

  if [[ "$final" == "PASS" && "$pin_required" -eq 1 && "$pin_after" -le "$pin_before" ]]; then
    final="FAIL"
    log "GATE $name: FAIL (declared PASS without invoking the traced pinned binary)"
    printf '%s\n' "no_traced_pin_invocation" >"$SEAL_OUT/gates/${name}.score_reason"
  fi

  if [[ -f "$gdir/instrument.txt" && ! -s "$gdir/instrument.txt" ]]; then
    final="FAIL"
    log "GATE $name: FAIL (empty instrument.txt under private out)"
  elif [[ -s "$gdir/instrument.txt" ]] && ! grep -qF "$PIN_WRAPPER" "$gdir/instrument.txt"; then
    final="FAIL"
    log "GATE $name: FAIL (private instrument.txt does not name traced pin launcher)"
  fi

  echo "$final" >"$SEAL_OUT/gates/${name}.status"
  if [[ "$final" == "PASS" ]]; then
    gate_pass=$((gate_pass + 1))
    GATE_ROWS+=("$name|PASS|$_v_reason|rc=$rc")
    # Suite-freshness ledger: stamp each green gate at HEAD so gate_run_freshness
    # can fail closed if the suite is not run end-to-end for >N commits.
    if [[ -x "$ROOT/scripts/gate_run_freshness.sh" ]]; then
      bash "$ROOT/scripts/gate_run_freshness.sh" --stamp "$name" >/dev/null 2>&1 || true
    fi
  else
    gate_fail=$((gate_fail + 1))
    GATE_ROWS+=("$name|FAIL|$_v_reason|rc=$rc")
  fi
  return 0
}

# force_gate_fail NAME REASON — demote a prior PASS (or reinforce FAIL) after postcheck
force_gate_fail() {
  local name="$1" reason="$2"
  local prev
  prev="$(cat "$SEAL_OUT/gates/${name}.status" 2>/dev/null || echo FAIL)"
  if [[ "$prev" == "PASS" ]]; then
    gate_pass=$((gate_pass - 1))
    gate_fail=$((gate_fail + 1))
  elif [[ "$prev" != "FAIL" && "$prev" != "KNOWN_FAIL" && "$prev" != "UNEXPECTED_PASS" ]]; then
    gate_fail=$((gate_fail + 1))
  fi
  echo "FAIL" >"$SEAL_OUT/gates/${name}.status"
  echo "POSTCHECK: $reason" >"$SEAL_OUT/gates/${name}.verdict_line"
  printf '%s\n' "$reason" >"$SEAL_OUT/gates/${name}.score_reason"
  GATE_ROWS+=("$name|FAIL|$reason|postcheck")
  log "POSTCHECK $name: FAIL ($reason)"
}

# count_corpus DIR GLOB → number of matching files (absolute or repo-relative DIR)
count_corpus() {
  local dir="$1" glob="$2"
  local abs="$dir"
  [[ "$dir" != /* ]] && abs="$ROOT/$dir"
  # nullglob: zero matches → empty array (not a literal "*.anb" path)
  shopt -s nullglob
  # shellcheck disable=SC2206,SC2086
  local matches=( "$abs"/$glob )
  shopt -u nullglob
  echo "${#matches[@]}"
}

# postcheck_corpus_complete NAME GLOG CORPUS_DIR GLOB MODE
# MODE:
#   overall_slash  — parse last ^Overall: ... (passed/total) → use total
#   over_fixtures  — parse last " over N fixtures" → use N
#
# Truncation detector (Seshat R7 / c04 lesson): if a gate dies mid-corpus under
# set -euo pipefail (unmatched grep) it may still leave a green-looking partial
# summary, or process fewer files than the on-disk corpus. Require reported == expected.
postcheck_corpus_complete() {
  local name="$1" glog="$2" corpus_dir="$3" glob_pat="$4" mode="$5"
  local expected reported=0
  expected="$(count_corpus "$corpus_dir" "$glob_pat")"
  if [[ "$expected" -eq 0 ]]; then
    force_gate_fail "$name" "corpus_empty_or_unreadable:$corpus_dir/$glob_pat"
    return 0
  fi

  case "$mode" in
    overall_slash)
      # Overall: PASS (219/219)  or  Overall: FAIL (3/219) timed_out=...
      reported="$(grep -E '^Overall: (PASS|FAIL)\b' "$glog" 2>/dev/null | tail -1 \
        | sed -n 's/.*(\([0-9][0-9]*\)\/\([0-9][0-9]*\)).*/\2/p' || true)"
      ;;
    over_fixtures)
      # CAPSET_SELFHOST over 5 fixtures: ...
      reported="$(grep -E ' over [0-9]+ fixtures' "$glog" 2>/dev/null | tail -1 \
        | sed -n 's/.* over \([0-9][0-9]*\) fixtures.*/\1/p' || true)"
      ;;
    *)
      force_gate_fail "$name" "unknown_corpus_postcheck_mode:$mode"
      return 0
      ;;
  esac

  if [[ -z "${reported:-}" ]]; then
    # Declared PASS without a countable summary is untrustworthy for fixture gates
    if [[ "$(cat "$SEAL_OUT/gates/${name}.status" 2>/dev/null || echo FAIL)" == "PASS" ]]; then
      force_gate_fail "$name" "no_fixture_count_line_in_log_corpus_expected=$expected"
    else
      log "POSTCHECK $name: skip count (gate already FAIL; expected_corpus=$expected)"
    fi
    return 0
  fi

  printf 'expected=%s reported=%s mode=%s corpus=%s/%s\n' \
    "$expected" "$reported" "$mode" "$corpus_dir" "$glob_pat" \
    >"$SEAL_OUT/gates/${name}.corpus_check"

  if [[ "$reported" -lt "$expected" ]]; then
    force_gate_fail "$name" "TRUNCATED_RUN: reported=$reported < corpus=$expected ($corpus_dir/$glob_pat) — mid-run death/skip hazard"
    return 0
  fi
  if [[ "$reported" -gt "$expected" ]]; then
    # Unexpected surplus — still fail closed (wrong dir or double-count)
    force_gate_fail "$name" "corpus_count_surplus: reported=$reported > expected=$expected"
    return 0
  fi
  log "POSTCHECK $name: corpus complete ($reported/$expected $corpus_dir/$glob_pat)"
  return 0
}

# run_known_failing NAME PASS_RE FAIL_RE REASON -- command...
# Expects declared FAIL. Records KNOWN_FAIL (honest). Unexpected PASS → seal failure.
run_known_failing() {
  local name="$1" pass_re="$2" fail_re="$3" reason="$4"
  shift 4
  if [[ "${1:-}" == "--" ]]; then shift; fi

  local gdir="$SEAL_OUT/gates/$name"
  local glog="$SEAL_OUT/logs/${name}.log"
  mkdir -p "$gdir"
  log ""
  log "---- GATE $name (KNOWN-FAILING — expect declared FAIL) ----"
  log "reason: $reason"
  log "cmd: $*"
  printf '%s\n' "$reason" >"$SEAL_OUT/gates/${name}.known_fail_reason"

  _instrument_guards

  set +e
  env ANUBIS_BIN="$PIN_WRAPPER" ANUBIS="$PIN_WRAPPER" \
    ANUBIS_SEAL_ACTIVE_GATE="$name" "$@" >"$glog" 2>&1
  local rc=$?
  set -e
  _instrument_guards

  classify_verdict "$glog" "$pass_re" "$fail_re"
  printf '%s\n' "${_v_line:-}" >"$SEAL_OUT/gates/${name}.verdict_line"
  printf '%s\n' "$_v_reason" >"$SEAL_OUT/gates/${name}.score_reason"

  if [[ "$_v_status" == "FAIL" && "$_v_reason" == "declared_FAIL_line" ]]; then
    # Expected: still red for the documented reason
    if [[ $rc -eq 0 ]]; then
      log "GATE $name: FAIL (declared FAIL but rc=0 — hollow exit; not acceptable even for known-failing)"
      echo "FAIL" >"$SEAL_OUT/gates/${name}.status"
      gate_fail=$((gate_fail + 1))
      GATE_ROWS+=("$name|FAIL|known_fail_hollow_rc0|rc=$rc")
    else
      log "GATE $name: KNOWN_FAIL (declared FAIL as expected)"
      log "  declared: $_v_line"
      log "  reason: $reason"
      echo "KNOWN_FAIL" >"$SEAL_OUT/gates/${name}.status"
      gate_known_fail=$((gate_known_fail + 1))
      GATE_ROWS+=("$name|KNOWN_FAIL|$_v_reason|rc=$rc")
    fi
  elif [[ "$_v_status" == "PASS" ]]; then
    log "GATE $name: UNEXPECTED_PASS (was listed KNOWN-FAILING — update the list or investigate)"
    log "  declared: $_v_line"
    log "  prior reason was: $reason"
    echo "UNEXPECTED_PASS" >"$SEAL_OUT/gates/${name}.status"
    gate_known_unexpected_pass=$((gate_known_unexpected_pass + 1))
    gate_fail=$((gate_fail + 1))
    GATE_ROWS+=("$name|UNEXPECTED_PASS|known_failing_flipped_green|rc=$rc")
  else
    log "GATE $name: FAIL (known-failing unscorable or wrong shape; rc=$rc)"
    log "  declared: ${_v_line:-(none)} reason=$_v_reason"
    tail -20 "$glog" | sed 's/^/  | /' | tee -a "$SUMMARY" || true
    echo "FAIL" >"$SEAL_OUT/gates/${name}.status"
    gate_fail=$((gate_fail + 1))
    GATE_ROWS+=("$name|FAIL|known_failing_unscorable|rc=$rc")
  fi
  return 0
}

skip_gate() {
  local name="$1" reason="$2"
  log ""
  log "---- GATE $name ----"
  log "GATE $name: SKIP ($reason)"
  echo "SKIP" >"$SEAL_OUT/gates/${name}.status"
  echo "skip:$reason" >"$SEAL_OUT/gates/${name}.verdict_line"
  gate_skip=$((gate_skip + 1))
  GATE_ROWS+=("$name|SKIP|$reason")
}

# Emit known-failing manifest (empty when the suite is fully green — still present for auditors)
{
  echo "# Seal known-failing gates"
  echo ""
  echo "These gates are **executed** under the pinned binary and expected to print a"
  echo "**declared FAIL** line. They are not hidden. A flip to PASS fails the seal."
  echo ""
  echo "| Gate | Script | Expected | Reason |"
  echo "|------|--------|----------|--------|"
  echo ""
  echo "_None as of 2026-07-27 (capset self-host promoted to required PASS after c05 legacy-CAPS fix + 5/5 green)._"
  echo ""
  echo "## Retired known-failing (history)"
  echo ""
  echo "| Gate | Was | Why retired |"
  echo "|------|-----|-------------|"
  echo "| capset_selfhost | FAIL on c05 (Rust listed fs.read with fs.write via legacy_capabilities_present) | Fixed; now required PASS + corpus completeness 5/5 |"
  echo ""
} | tee "$KNOWN_FAILING_MD" | tee -a "$SUMMARY" >/dev/null
log "known_failing_manifest: $KNOWN_FAILING_MD (no active known-failing entries)"

# ── CORE profile — required PASS ─────────────────────────────────────────────
run_gate security \
  '^Overall: PASS\b' \
  '^Overall: FAIL\b' \
  -- bash scripts/run_security_fixtures.sh --out "$SEAL_OUT/gates/security"
postcheck_corpus_complete security \
  "$SEAL_OUT/logs/security.log" "examples/security" "*.anb" overall_slash

run_gate language \
  '^Overall: PASS\b' \
  '^Overall: FAIL\b' \
  -- bash scripts/run_language_fixtures.sh --out "$SEAL_OUT/gates/language"
postcheck_corpus_complete language \
  "$SEAL_OUT/logs/language.log" "tests/fixtures/language_core" "*.anb" overall_slash

run_gate runtime \
  '^Overall: PASS\b' \
  '^Overall: FAIL\b' \
  -- bash scripts/run_runtime_fixtures.sh --out "$SEAL_OUT/gates/runtime"
postcheck_corpus_complete runtime \
  "$SEAL_OUT/logs/runtime.log" "tests/fixtures/runtime" "*.anb" overall_slash

# Match PASS or PASS_WITH_KNOWN_NON_RUN (avoid $| in the pattern — confusable under concurrent edits).
run_gate check_run_parity \
  '^GATE: PASS(_WITH_KNOWN_NON_RUN)?( |$)' \
  '^GATE: FAIL\b' \
  bash scripts/run_check_run_parity_gate.sh --out "$SEAL_OUT/gates/check_run_parity"

if [[ -f "$SEAL_OUT/gates/check_run_parity/report.json" ]]; then
  compared="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("compared",0))' \
    "$SEAL_OUT/gates/check_run_parity/report.json" 2>/dev/null || echo 0)"
  if [[ "${compared:-0}" -eq 0 ]]; then
    log "POSTCHECK check_run_parity: compared=0 — hollow; forcing FAIL"
    prev="$(cat "$SEAL_OUT/gates/check_run_parity.status" 2>/dev/null || echo FAIL)"
    if [[ "$prev" == "PASS" ]]; then
      gate_pass=$((gate_pass - 1))
      gate_fail=$((gate_fail + 1))
    fi
    echo "FAIL" >"$SEAL_OUT/gates/check_run_parity.status"
    echo "POSTCHECK: compared=0" >"$SEAL_OUT/gates/check_run_parity.verdict_line"
    GATE_ROWS+=("check_run_parity|FAIL|compared=0_postcheck|rc=n/a")
  else
    log "POSTCHECK check_run_parity: compared=$compared"
  fi
fi

run_gate stdlib_failclosed \
  '^Overall: PASS\b' \
  '^Overall: FAIL\b' \
  -- bash scripts/run_stdlib_failclosed_gate.sh --out "$SEAL_OUT/gates/stdlib_failclosed"
postcheck_corpus_complete stdlib_failclosed \
  "$SEAL_OUT/logs/stdlib_failclosed.log" "tests/fixtures/stdlib" "*should_fail_closed.anb" overall_slash

# Run-path fail-closed meta-gate (GROK-PTAH): buckets A–E + inventory stamp.
# Seal accepts PASS_INSTRUMENTED (looked-at surfaces green) OR WHOLE.
# Does NOT treat WHOLE as required — FA/unenumerated residuals stay honest.
# Declared line is the score source; claim_run_failclosed_as_a_whole stays false
# in the gate report while inventory blockers remain.
run_gate run_failclosed \
  '^GATE: PASS_(INSTRUMENTED|RUNTIME_FAILCLOSED_WHOLE)\b' \
  '^GATE: FAIL\b' \
  -- bash scripts/run_run_failclosed_gate.sh --out "$SEAL_OUT/gates/run_failclosed" --timeout 90

run_gate capset_registry_parity \
  '^CAPSET_REGISTRY_PARITY_GATE: PASS\b' \
  '^CAPSET_REGISTRY_PARITY_GATE: FAIL\b' \
  --no-pin-use -- bash scripts/check_capset_registry_parity.sh

run_gate gate_common_adoption \
  '^GATE_COMMON_ADOPTION_GATE: PASS\b' \
  '^GATE_COMMON_ADOPTION_GATE: FAIL\b' \
  --no-pin-use -- bash scripts/check_gate_common_adoption.sh

# Docs drift: re-derive live inventory numbers; fail undated stamps that disagree.
# Self-test proves every guard fires (FAIL on drift, PASS on truth/dated).
# --no-pin-use: inventory-only; does not execute anubis.
run_gate docs_drift \
  '^DOCS_DRIFT_GATE: PASS\b' \
  '^DOCS_DRIFT_GATE: FAIL\b' \
  --no-pin-use -- bash scripts/run_docs_drift_gate.sh \
    --out "$SEAL_OUT/gates/docs_drift" \
    --self-test


# NOTE (R47): instrument_hygiene + gate_run_freshness run AFTER all required
# content gates (see below, before final instrument re-check). Placing them
# here deadlocked first-seal bootstrap: freshness required selfhost/taint/
# capset_selfhost stamps that had not been produced yet in the same run.

if command -v lake >/dev/null 2>&1 || [[ -x "$HOME/.elan/bin/lake" ]]; then
  if ! command -v lake >/dev/null 2>&1 && [[ -x "$HOME/.elan/bin/lake" ]]; then
    export PATH="$HOME/.elan/bin:$PATH"
  fi
  run_gate formal \
    '^FORMAL_GATE: PASS\b' \
    '^FORMAL_GATE: FAIL\b' \
    --no-pin-use -- bash scripts/run_formal_gate.sh
else
  refuse "formal prerequisite missing: lake not on PATH"
fi

if command -v z3 >/dev/null 2>&1; then
  run_gate native_authoritative \
    '^NATIVE_AUTHORITATIVE_GATE: PASS\b' \
    '^NATIVE_AUTHORITATIVE_GATE: FAIL\b' \
    -- bash scripts/run_native_authoritative_gate.sh
else
  refuse "native-authoritative prerequisite missing: z3 not on PATH"
fi

run_gate selfhost \
  '^SELFHOST_GATE: PASS\b' \
  '^SELFHOST_GATE: FAIL\b' \
  -- bash scripts/run_selfhost_gate.sh "$SEAL_OUT/gates/selfhost"

run_gate taint_selfhost \
  '^TAINT_SELFHOST_GATE: PASS\b' \
  '^TAINT_SELFHOST_GATE: FAIL\b' \
  -- bash scripts/run_taint_selfhost_gate.sh
postcheck_corpus_complete taint_selfhost \
  "$SEAL_OUT/logs/taint_selfhost.log" "tests/fixtures/taint_selfhost" "*.anb" over_fixtures

# Capset self-host: promoted from known-failing → required PASS (5/5 green)
run_gate capset_selfhost \
  '^CAPSET_SELFHOST_GATE: PASS\b' \
  '^CAPSET_SELFHOST_GATE: FAIL\b' \
  -- bash scripts/run_capset_selfhost_gate.sh
postcheck_corpus_complete capset_selfhost \
  "$SEAL_OUT/logs/capset_selfhost.log" "tests/fixtures/capset_selfhost" "*.anb" over_fixtures

# ── FULL profile ─────────────────────────────────────────────────────────────
if [[ "$PROFILE" == "full" ]]; then
  run_gate type_selfhost \
    '^TYPE_SELFHOST_GATE: PASS\b' \
    '^TYPE_SELFHOST_GATE: FAIL\b' \
    -- bash scripts/run_type_selfhost_gate.sh
  postcheck_corpus_complete type_selfhost \
    "$SEAL_OUT/logs/type_selfhost.log" "tests/fixtures/types_selfhost" "*.anb" over_fixtures

  run_gate effect_selfhost \
    '^EFFECT_SELFHOST_GATE: PASS\b' \
    '^EFFECT_SELFHOST_GATE: FAIL\b' \
    -- bash scripts/run_effect_selfhost_gate.sh
  postcheck_corpus_complete effect_selfhost \
    "$SEAL_OUT/logs/effect_selfhost.log" "tests/fixtures/effects_selfhost" "*.anb" over_fixtures

  run_gate selfhost_dogfood \
    '^SELFHOST_DOGFOOD_GATE: PASS\b' \
    '^SELFHOST_DOGFOOD_GATE: FAIL\b' \
    -- bash scripts/run_selfhost_dogfood_gate.sh "$SEAL_OUT/gates/selfhost_dogfood"

  run_gate selfhost_fulllang \
    '^SELFHOST_FULLLANG_GATE: PASS\b' \
    '^SELFHOST_FULLLANG_GATE: FAIL\b' \
    -- bash scripts/run_selfhost_fulllang_gate.sh "$SEAL_OUT/gates/selfhost_fulllang"

  if [[ "${ANUBIS_SEAL_CAPSET_CORPUS:-0}" == "1" ]]; then
    run_gate capset_corpus \
      '^CAPSET_CORPUS_FAILCLOSED_GATE: PASS\b' \
      '^CAPSET_CORPUS_FAILCLOSED_GATE: FAIL\b' \
      -- bash scripts/run_capset_corpus_failclosed.sh
  else
    log "EXCLUDED capset_corpus: set ANUBIS_SEAL_CAPSET_CORPUS=1 to add this optional full-profile gate"
  fi

  if [[ "${ANUBIS_SEAL_DDC:-0}" == "1" ]]; then
    run_gate selfhost_ddc \
      '^SELFHOST_DDC_GATE: PASS\b' \
      '^SELFHOST_DDC_GATE: FAIL\b' \
      -- bash scripts/run_selfhost_ddc_gate.sh "$SEAL_OUT/gates/selfhost_ddc"
  else
    log "EXCLUDED selfhost_ddc: set ANUBIS_SEAL_DDC=1 to add this optional full-profile gate"
  fi
fi

# Instrument hygiene + suite freshness (adversary R46/R47): AFTER all required
# content gates have stamped the ledger, so a first seal bootstraps in one run.
# --no-pin-use: meta-checks over scripts/ledgers, not anubis execution.
run_gate instrument_hygiene \
  '^INSTRUMENT_HYGIENE: PASS\b' \
  '^INSTRUMENT_HYGIENE: FAIL\b' \
  --no-pin-use -- bash scripts/instrument_hygiene.sh

run_gate gate_run_freshness \
  '^GATE_RUN_FRESHNESS: PASS\b' \
  '^GATE_RUN_FRESHNESS: FAIL\b' \
  --no-pin-use -- bash scripts/gate_run_freshness.sh

# ── Final instrument re-check ────────────────────────────────────────────────
if [[ ! -f "$INSTRUMENT" || ! -s "$INSTRUMENT" ]]; then
  refuse "instrument.txt missing or empty at end of seal"
fi
if ! grep -q "sha256=$BIN_SHA" "$INSTRUMENT" 2>/dev/null && [[ "$BIN_SHA" != "unavailable" ]]; then
  refuse "instrument.txt sha256 mismatch at seal end"
fi
end_size="$(stat -f '%z' "$SNAP" 2>/dev/null || stat -c '%s' "$SNAP" 2>/dev/null || echo 0)"
if [[ "$end_size" != "$BIN_SIZE" ]]; then
  refuse "snapshot mutated during seal"
fi
if command -v shasum >/dev/null 2>&1; then
  end_sha="$(shasum -a 256 "$SNAP" | awk '{print $1}')"
else
  end_sha="$(sha256sum "$SNAP" | awk '{print $1}')"
fi
if [[ "$end_sha" != "$BIN_SHA" ]]; then
  refuse "snapshot sha256 mutated during seal (was $BIN_SHA now $end_sha)"
fi

# ── Verdict ──────────────────────────────────────────────────────────────────
log ""
log "==== SEAL SUMMARY ===="
log "gates_pass=$gate_pass gates_fail=$gate_fail gates_skip=$gate_skip known_fail=$gate_known_fail unexpected_pass=$gate_known_unexpected_pass"
log "instrument: $SNAP mtime=$BIN_MTIME size=$BIN_SIZE sha256=$BIN_SHA"
log "scoring_rule=declared_verdict_line_only (body FAIL/exp=FAIL ignored)"
log "corpus_completeness=reported_fixture_count_must_equal_on_disk_corpus (truncation → FAIL)"
log "known_failing_policy=run_and_expect_declared_FAIL when any listed (UNEXPECTED_PASS fails seal)"
for row in "${GATE_ROWS[@]+"${GATE_ROWS[@]}"}"; do
  log "  $row"
done

if [[ "$gate_pass" -eq 0 ]]; then
  refuse "zero required gates PASSed — hollow seal forbidden"
fi
if [[ "$gate_skip" -gt 0 ]]; then
  refuse "required gate skipped ($gate_skip SKIP status rows)"
fi
if [[ "$gate_fail" -gt 0 || "$gate_known_unexpected_pass" -gt 0 ]]; then
  log "SEAL_FAIL: fail=$gate_fail unexpected_known_pass=$gate_known_unexpected_pass"
  write_verdict "SEAL_FAIL" "fail=$gate_fail pass=$gate_pass skip=$gate_skip known_fail=$gate_known_fail unexpected_pass=$gate_known_unexpected_pass"
  exit 1
fi

log "SEAL_PASS: required gates green under pin; corpus completeness OK; known-failing empty or still red as documented"
write_verdict "SEAL_PASS" "pass=$gate_pass skip=$gate_skip known_fail=$gate_known_fail pinned=$SNAP sha256=$BIN_SHA"
echo ""
echo "SEAL_PASS"
echo "report: $VERDICT_JSON"
echo "instrument: $INSTRUMENT"
echo "known_failing: $KNOWN_FAILING_MD"
exit 0
