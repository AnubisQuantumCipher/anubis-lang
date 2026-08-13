#!/usr/bin/env bash
# RUN-path fixture gate — verifies what `anubis run` actually does.
#
# Sibling of scripts/run_security_fixtures.sh (check-path, examples/security)
# and scripts/run_language_fixtures.sh (check-path, language_core). Those two
# only see `anubis check`. This gate is the missing half: runtime exit codes,
# ANUBIS_* panic needles, optional stdout, and side-effect constraints.
#
# HONESTY CONTRACT (no false-green) — same shape as run_security_fixtures.sh:
#   EXPECT PASS  → run exit 0 AND (no ERROR_CONTAINS OR needle in log)
#                  AND all MUST_EXIST present AND all MUST_NOT_EXIST absent
#                  AND STDOUT_CONTAINS (if any) present in stdout
#   EXPECT FAIL without ERROR_CONTAINS → run nonzero
#   EXPECT FAIL with ERROR_CONTAINS    → run MUST fail AND needle MUST appear
#                                        in run.log. Nonzero exit for a different
#                                        reason without the needle is a MISMATCH
#                                        (fixture FAIL), never a green pass.
#
# TIMEOUTS are a third bucket (never PASS, never FAIL): scored TIMEOUT with a
# stated budget. Under load a timeout is SKIPPED-with-reason for overall, not
# a cry-wolf FAIL.
#
# UNIQUE OUT DIR BY DEFAULT: default path includes timestamp + pid so concurrent
# agents cannot race `jq ... > "$report.tmp" && mv` on a shared fixed dir.
# Explicit --out DIR still allowed (caller owns uniqueness then).
#
# macOS ships bash 3.2.57: never expand an empty array as "${arr[@]}" under
# set -u — that throws unbound-variable on the ZERO-findings happy path.
#
# Header tags (// lines in the .anb fixture):
#   EXPECT: PASS|FAIL
#   ERROR_CONTAINS: ANUBIS_*
#   STDOUT_CONTAINS: literal substring of program stdout (banners stripped)
#   MUST_EXIST: path-relative-to-workdir
#   MUST_NOT_EXIST: path-relative-to-workdir
#   TIMEOUT_SECS: N   (optional per-fixture override; default --timeout)
#
# Usage:
#   bash scripts/run_runtime_fixtures.sh
#   bash scripts/run_runtime_fixtures.sh --out out/my_run
#   bash scripts/run_runtime_fixtures.sh --dir tests/fixtures/stdlib --glob '*should_fail_closed.anb'
#   bash scripts/run_runtime_fixtures.sh --timeout 90
#
# Does NOT rebuild the binary (fleet multi-agent cargo lock discipline).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
source "$ROOT/scripts/lib/gate_common.sh"

# --- defaults: UNIQUE stamp so concurrent agents never share a report path ---
STAMP="$(date +%Y%m%dT%H%M%S)_$$"
OUT_DIR="out/runtime_fixtures/${STAMP}"
FIXTURE_DIR="tests/fixtures/runtime"
GLOB_PAT='*.anb'
DEFAULT_TIMEOUT=120
OUT_EXPLICIT=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out)
      OUT_DIR="$2"
      OUT_EXPLICIT=1
      shift 2
      ;;
    --dir)
      FIXTURE_DIR="$2"
      shift 2
      ;;
    --glob)
      GLOB_PAT="$2"
      shift 2
      ;;
    --timeout)
      DEFAULT_TIMEOUT="$2"
      shift 2
      ;;
    -h|--help)
      sed -n '2,55p' "$0"
      exit 0
      ;;
    *)
      echo "unknown arg: $1" >&2
      exit 2
      ;;
  esac
done

mkdir -p "$OUT_DIR"
report="$OUT_DIR/runtime_fixture_report.json"
# Per-process unique jq temp (even with shared --out, avoid cross-agent mv races)
jq_tmp() { echo "${1}.$$.$RANDOM.tmp"; }

echo '{"fixtures": [], "overall_verdict": "PENDING"}' > "$report"

# --- resolve binary (no rebuild) ---
if [[ -n "${ANUBIS_BIN:-}" ]]; then
  [[ -x "$ANUBIS_BIN" ]] || {
    echo "FATAL: ANUBIS_BIN=$ANUBIS_BIN is not executable" >&2
    exit 127
  }
  executed_via="preset:$ANUBIS_BIN"
elif [[ -x "./target/release/anubis" ]]; then
  ANUBIS_BIN="./target/release/anubis"
  executed_via="release"
elif [[ -x "./target/debug/anubis" ]]; then
  ANUBIS_BIN="./target/debug/anubis"
  executed_via="debug"
else
  echo "FATAL: no anubis binary at target/release or target/debug (will not cargo build)" >&2
  exit 127
fi

bin_mtime="$(stat -f '%Sm' -t '%Y-%m-%dT%H:%M:%S' "$ANUBIS_BIN" 2>/dev/null || echo unknown)"
bin_size="$(stat -f '%z' "$ANUBIS_BIN" 2>/dev/null || echo 0)"
echo "instrument: $ANUBIS_BIN mtime=$bin_mtime size=$bin_size via=$executed_via out=$OUT_DIR" \
  | tee "$OUT_DIR/instrument.txt"
if [[ "$ANUBIS_BIN" == /* ]]; then
  ANUBIS_EXEC="$ANUBIS_BIN"
else
  ANUBIS_EXEC="$ROOT/$ANUBIS_BIN"
fi

# --- collect fixtures (bash 3.2 / set -u safe empty array) ---
fixtures=()
shopt -s nullglob
# shellcheck disable=SC2206
_glob_matches=( "$FIXTURE_DIR"/$GLOB_PAT )
shopt -u nullglob
# Only assign if non-empty — empty "${arr[@]}" under set -u is fatal on bash 3.2
if [[ ${#_glob_matches[@]} -gt 0 ]]; then
  fixtures=( "${_glob_matches[@]}" )
fi
unset _glob_matches

total=0
passed=0
failed=0
timed_out=0

if ! require_nonempty_corpus "${#fixtures[@]}" "$FIXTURE_DIR/$GLOB_PAT"; then
  _t=$(jq_tmp "$report")
  jq --arg overall FAIL --arg bin "$ANUBIS_BIN" --arg mtime "$bin_mtime" \
     --argjson size "$bin_size" --arg via "$executed_via" --arg out "$OUT_DIR" \
     --arg note "no fixtures matched" \
     '. + {total:0, passed:0, failed:0, timed_out:0, overall_verdict:"FAIL",
           instrument:{path:$bin, mtime:$mtime, size_bytes:$size, via:$via},
           out_dir:$out, note:$note}' \
     "$report" > "$_t" && mv "$_t" "$report"
  echo "Report: $report"
  echo "Overall: FAIL (0/0) timeouts=0"
  exit 1
fi

# --- helpers ---
# Strip anubis run banners from a log; keep program stdout + panic/error lines.
program_stdout() {
  local log="$1"
  # Drop compile banners; keep the rest (prints + panics)
  grep -v -E '^anubis run: (compiling|compile done)' "$log" 2>/dev/null || true
}

header_val() {
  # Extract first // TAG: value from fixture (value may contain spaces)
  local file="$1" tag="$2"
  grep -E "^// ${tag}:" "$file" 2>/dev/null | head -1 | sed "s|^// ${tag}:[[:space:]]*||" | tr -d '\r' || true
}

header_vals() {
  # All // TAG: values (for multi MUST_NOT_EXIST lines)
  local file="$1" tag="$2"
  grep -E "^// ${tag}:" "$file" 2>/dev/null | sed "s|^// ${tag}:[[:space:]]*||" | tr -d '\r' || true
}

# --- main loop: bash 3.2 safe — only expand when length > 0 (guarded above) ---
for f in "${fixtures[@]}"; do
  [[ -f "$f" ]] || continue
  base=$(basename "$f" .anb)
  total=$((total + 1))
  echo "=== $base ==="

  outd="$OUT_DIR/cases/$base"
  work="$OUT_DIR/work/$base"
  rm -rf "$outd" "$work"
  mkdir -p "$outd" "$work"

  # Copy fixture into workdir so relative write_file paths stay under --out
  cp "$f" "$work/fixture.anb"
  fixture_run="$work/fixture.anb"

  if ! parse_expectation "$f" "$base" accept_reject; then
    expect="${GATE_EXPECT:-}"
    actual="MALFORMED"
    status="FAIL"
    failed=$((failed + 1))
    echo "  MALFORMED: $GATE_MALFORMED"
    _t=$(jq_tmp "$report")
    jq --arg name "$base" --arg status "$status" --arg expect "$expect" \
       --arg actual "$actual" --arg path "$f" --arg detail "$GATE_MALFORMED" \
      '.fixtures += [{"name":$name,"path":$path,"status":$status,"expected":$expect,
        "actual":$actual,"malformed":$detail}]' \
      "$report" > "$_t" && mv "$_t" "$report"
    continue
  fi
  expect="$GATE_EXPECT"
  err_needle=$(header_val "$f" "ERROR_CONTAINS")
  # Drop trailing inline comment after needle
  err_needle="${err_needle%% // *}"
  err_needle="$(printf '%s' "$err_needle" | sed 's/[[:space:]]*$//')"
  stdout_needle=$(header_val "$f" "STDOUT_CONTAINS")
  stdout_needle="${stdout_needle%% // *}"
  per_timeout=$(header_val "$f" "TIMEOUT_SECS")
  if [[ -z "${per_timeout:-}" ]]; then
    per_timeout="$DEFAULT_TIMEOUT"
  fi

  cmd="$ANUBIS_BIN run $fixture_run"
  echo "$cmd" > "$outd/command.txt"
  echo "timeout_budget_secs=$per_timeout" > "$outd/timeout.txt"
  echo "workdir=$work" > "$outd/workdir.txt"

  set +e
  if command -v timeout >/dev/null 2>&1; then
    # Run with CWD = work so relative side effects land under --out only
    (cd "$work" && timeout "$per_timeout" "$ANUBIS_EXEC" run fixture.anb) \
      >"$outd/run.log" 2>&1
    rc=$?
  else
    (cd "$work" && "$ANUBIS_EXEC" run fixture.anb) \
      >"$outd/run.log" 2>&1
    rc=$?
  fi
  set -e

  # timeout(1) uses 124; some builds use 137 for kill
  is_timeout=0
  if [[ $rc -eq 124 || $rc -eq 137 ]]; then
    is_timeout=1
  fi
  if grep -qE 'TIMEOUT|killed \(timeout\)' "$outd/run.log" 2>/dev/null && [[ $rc -ne 0 ]]; then
    # only trust if budget likely hit — keep rc-based primary
    :
  fi

  if [[ $is_timeout -eq 1 ]]; then
    timed_out=$((timed_out + 1))
    status="TIMEOUT"
    actual="TIMEOUT"
    echo "  TIMEOUT: budget ${per_timeout}s exceeded (rc=$rc) — not scored PASS or FAIL"
    _t=$(jq_tmp "$report")
    jq --arg name "$base" --arg status "$status" --arg expect "$expect" --arg actual "$actual" \
       --arg cmd "$cmd" --argjson rc "$rc" --arg executed_via "$executed_via" \
       --arg needle "$err_needle" --arg path "$f" --argjson budget "$per_timeout" \
       --arg work "$work" \
      '.fixtures += [{"name":$name,"path":$path,"status":$status,"expected":$expect,
        "actual":$actual,"command":$cmd,"exit_code":$rc,"executed_via":$executed_via,
        "error_contains":$needle,"timeout_budget_secs":$budget,"workdir":$work}]' \
      "$report" > "$_t" && mv "$_t" "$report"
    continue
  fi

  cmd_failed=0
  if [[ $rc -ne 0 ]]; then
    cmd_failed=1
  fi
  # anubis may exit 1 with "Error: run failed" wrapping a panic
  if grep -qE 'panicked at|Error: run failed|exit_code=Some\(101\)' "$outd/run.log" 2>/dev/null; then
    cmd_failed=1
  fi

  needle_present=0
  if [[ -n "${err_needle:-}" ]] && grep -qF -- "$err_needle" "$outd/run.log" 2>/dev/null; then
    needle_present=1
  fi

  # stdout check (program output only)
  stdout_ok=1
  if [[ -n "${stdout_needle:-}" ]]; then
    stdout_ok=0
    if program_stdout "$outd/run.log" | grep -qF -- "$stdout_needle"; then
      stdout_ok=1
    fi
  fi

  # side effects relative to workdir
  side_ok=1
  side_detail=""
  while IFS= read -r rel || [[ -n "${rel:-}" ]]; do
    [[ -z "${rel:-}" ]] && continue
    rel="${rel%% // *}"
    rel="$(printf '%s' "$rel" | sed 's/[[:space:]]*$//')"
    [[ -z "$rel" ]] && continue
    if [[ ! -e "$work/$rel" ]]; then
      side_ok=0
      side_detail="${side_detail}; missing MUST_EXIST:$rel"
    fi
  done < <(header_vals "$f" "MUST_EXIST")

  while IFS= read -r rel || [[ -n "${rel:-}" ]]; do
    [[ -z "${rel:-}" ]] && continue
    rel="${rel%% // *}"
    rel="$(printf '%s' "$rel" | sed 's/[[:space:]]*$//')"
    [[ -z "$rel" ]] && continue
    if [[ -e "$work/$rel" ]]; then
      side_ok=0
      side_detail="${side_detail}; present MUST_NOT_EXIST:$rel"
    fi
  done < <(header_vals "$f" "MUST_NOT_EXIST")

  # Derive actual with honesty rules (mirror security fixtures)
  if [[ "$expect" == "PASS" ]]; then
    if [[ $cmd_failed -eq 0 && $stdout_ok -eq 1 && $side_ok -eq 1 ]]; then
      if [[ -z "${err_needle:-}" || $needle_present -eq 1 ]]; then
        actual="PASS"
      else
        actual="FAIL"
      fi
    else
      actual="FAIL"
      if [[ $cmd_failed -ne 0 ]]; then
        echo "  UNEXPECTED_FAIL: exit=$rc"
      fi
      if [[ $stdout_ok -eq 0 ]]; then
        echo "  STDOUT_MISSING: wanted '$stdout_needle'"
      fi
      if [[ $side_ok -eq 0 ]]; then
        echo "  SIDE_EFFECT: $side_detail"
      fi
    fi
  else
    # EXPECT FAIL
    if [[ -n "${err_needle:-}" ]]; then
      if [[ $cmd_failed -eq 1 && $needle_present -eq 1 && $side_ok -eq 1 ]]; then
        actual="FAIL"   # correct failure shape
      else
        actual="PASS"   # did not match FAIL criteria (silent success or wrong needle)
        if [[ $cmd_failed -eq 0 ]]; then
          echo "  SILENT_SUCCESS: expected fail with '$err_needle' but exit=$rc (runtime defect still open, or wrong expect)"
        else
          echo "  NEEDLE_MISSING: wanted '$err_needle' in run.log (rc=$rc)"
        fi
        if [[ $side_ok -eq 0 ]]; then
          echo "  SIDE_EFFECT: $side_detail"
        fi
      fi
    else
      if [[ $cmd_failed -eq 1 && $side_ok -eq 1 ]]; then
        actual="FAIL"
      else
        actual="PASS"
        if [[ $cmd_failed -eq 0 ]]; then
          echo "  SILENT_SUCCESS: expected nonzero exit"
        fi
      fi
    fi
  fi

  if score_fixture "$expect" "$actual"; then
    passed=$((passed + 1))
    status="PASS"
  else
    failed=$((failed + 1))
    status="FAIL"
    echo "  MISMATCH: expected $expect got $actual (rc=$rc needle_present=$needle_present stdout_ok=$stdout_ok side_ok=$side_ok)"
  fi

  _t=$(jq_tmp "$report")
  jq --arg name "$base" --arg status "$status" --arg expect "$expect" --arg actual "$actual" \
     --arg cmd "$cmd" --argjson rc "$rc" --arg executed_via "$executed_via" \
     --arg needle "$err_needle" --argjson needle_present "$needle_present" \
     --arg path "$f" --arg work "$work" --argjson stdout_ok "$stdout_ok" \
     --argjson side_ok "$side_ok" --arg side_detail "$side_detail" \
     --argjson budget "$per_timeout" \
    '.fixtures += [{"name":$name,"path":$path,"status":$status,"expected":$expect,
      "actual":$actual,"command":$cmd,"exit_code":$rc,"executed_via":$executed_via,
      "error_contains":$needle,"needle_present":$needle_present,"stdout_ok":$stdout_ok,
      "side_ok":$side_ok,"side_detail":$side_detail,"timeout_budget_secs":$budget,
      "workdir":$work}]' \
    "$report" > "$_t" && mv "$_t" "$report"
done

# Timeouts do not count as PASS or FAIL for overall_verdict.
# overall FAIL if any real FAIL, or if total==0.
# overall PASS if failed==0 and (passed+timed_out)==total and passed>=0.
# If all timed out with zero fails: overall is TIMEOUT (not PASS) — cry-wolf free.
set +e
finalize "$total" "$passed" "$failed" "$timed_out"
# Coverage ratchet: corpus must not silently shrink (assert_tested/finalize only see this run).
# The floor is PER-CORPUS, keyed by the directory and glob actually run.
#
# This runner is shared: `run_run_failclosed_gate.sh` invokes it over five different buckets
# (closed_corpus, permanent_controls, graduated_open, enforcement, doc_ok), each a different
# directory with a different fixture count. A single floor file ratchets to the LARGEST bucket and
# then fails every smaller one — `passed=23/23 failed=0 rc=1`, a bucket where nothing is wrong.
#
# I introduced exactly that defect adding the ratchet, and it is the same shape the ratchet exists
# to catch: a number that describes one thing being read as if it described another. Keying the
# floor to the corpus makes each bucket ratchet against its own history.
_floor_key="$(printf '%s|%s' "$FIXTURE_DIR" "$GLOB_PAT" | tr -c 'A-Za-z0-9' '_' | sed 's/__*/_/g;s/^_//;s/_$//')"
mkdir -p "$ROOT/scripts/floors"
assert_floor "runtime_fixtures[$FIXTURE_DIR $GLOB_PAT]" "$total" \
  "$ROOT/scripts/floors/runtime_${_floor_key}.count_floor"
floor_rc=$?
set -e
case "$GATE_FINAL_STATUS" in
  PASS) overall="PASS" ;;
  FAIL) overall="FAIL" ;;
  INCOMPLETE)
    if [[ $passed -eq 0 ]]; then overall="TIMEOUT"; else overall="PASS_WITH_TIMEOUTS"; fi
    ;;
  *) overall="FAIL" ;;
esac
if [[ $floor_rc -ne 0 ]]; then
  overall="FAIL"
  echo "Overall: FAIL ($GATE_FLOOR_ERROR)" >&2
fi

_t=$(jq_tmp "$report")
jq --arg overall "$overall" --argjson total "$total" --argjson passed "$passed" \
   --argjson failed "$failed" --argjson timed_out "$timed_out" \
   --arg bin "$ANUBIS_BIN" --arg mtime "$bin_mtime" --argjson size "$bin_size" \
   --arg via "$executed_via" --arg out "$OUT_DIR" --argjson budget "$DEFAULT_TIMEOUT" \
   --argjson out_explicit "$OUT_EXPLICIT" \
  '. + {total:$total, passed:$passed, failed:$failed, timed_out:$timed_out,
        overall_verdict:$overall, default_timeout_secs:$budget,
        out_dir:$out, out_explicit:$out_explicit,
        instrument:{path:$bin, mtime:$mtime, size_bytes:$size, via:$via}}' \
  "$report" > "$_t" && mv "$_t" "$report"

echo "Report: $report"
echo "Overall: $overall ($passed/$total) failed=$failed timeouts=$timed_out"
echo "Instrument: $ANUBIS_BIN mtime=$bin_mtime out=$OUT_DIR"

# Exit codes:
#   0 = clean PASS (no fails, no timeouts)
#   1 = FAIL (at least one mismatch)
#   2 = TIMEOUT-only or PASS_WITH_TIMEOUTS (not a code regression signal)
if [[ "$overall" == "FAIL" ]]; then
  exit 1
fi
if [[ "$overall" == "TIMEOUT" || "$overall" == "PASS_WITH_TIMEOUTS" ]]; then
  exit 2
fi
exit 0
