#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
source "$ROOT/scripts/lib/gate_common.sh"

# Gate 15 — Security Superpowers fixture runner
# Respects // EXPECT: PASS|FAIL and // ERROR_CONTAINS: ...
# Produces security_fixture_report.json with overall_verdict
#
# HONESTY CONTRACT (no false-green):
#   EXPECT PASS  → command exit 0 AND (no ERROR_CONTAINS OR needle present)
#   EXPECT FAIL without ERROR_CONTAINS → command nonzero (or check_error set)
#   EXPECT FAIL with ERROR_CONTAINS    → command must fail AND needle MUST appear
#                                        in run.log. A failure for a different
#                                        reason without the needle is a MISMATCH
#                                        (fixture FAIL), never a green pass.

OUT_DIR="out/gate15_security_fixtures"
if [[ "${1:-}" == "--out" && -n "${2:-}" ]]; then
  OUT_DIR="$2"
  shift 2
fi

mkdir -p "$OUT_DIR"
SECURITY_DIR="examples/security"

# Pin the instrument ONCE for the whole run (not per-fixture). A mid-run rebuild must not
# silently change which binary the remaining fixtures grade.
# An externally-provided ANUBIS_BIN wins over auto-detection.
if [[ -n "${ANUBIS_BIN:-}" ]]; then
  [[ -x "$ANUBIS_BIN" ]] || { echo "FATAL: ANUBIS_BIN=$ANUBIS_BIN is not executable" >&2; exit 127; }
  executed_via="preset"
elif [[ -x "./target/release/anubis" ]]; then
  ANUBIS_BIN="./target/release/anubis"
  executed_via="release"
elif [[ -x "./target/debug/anubis" ]]; then
  ANUBIS_BIN="./target/debug/anubis"
  executed_via="debug"
else
  ANUBIS_BIN="cargo run -p anubis --"
  executed_via="cargo"
fi
bin_mtime="n/a"; bin_size="n/a"
if [[ -x "$ANUBIS_BIN" ]]; then
  bin_mtime="$(stat -f '%Sm' -t '%Y-%m-%dT%H:%M:%S' "$ANUBIS_BIN" 2>/dev/null || echo unknown)"
  bin_size="$(stat -f '%z' "$ANUBIS_BIN" 2>/dev/null || echo 0)"
fi
echo "instrument: $ANUBIS_BIN via=$executed_via mtime=$bin_mtime size=$bin_size out=$OUT_DIR" \
  | tee "$OUT_DIR/instrument.txt"

report="$OUT_DIR/security_fixture_report.json"
# Per-process report temp so concurrent agents sharing a (misconfigured) OUT do not race on
# the same report.tmp path.
REPORT_TMP="$report.tmp.$$"
echo '{"fixtures": [], "overall_verdict": "PENDING"}' > "$report"

total=0
passed=0
failed=0

shopt -s nullglob
fixtures=( "$SECURITY_DIR"/*.anb )
shopt -u nullglob
if ! require_nonempty_corpus "${#fixtures[@]}" "$SECURITY_DIR/*.anb"; then
  jq '. + {total:0, passed:0, failed:0, overall_verdict:"FAIL", note:"no fixtures matched"}' \
    "$report" > "$REPORT_TMP" && mv "$REPORT_TMP" "$report"
  echo "Overall: FAIL (0/0)"
  exit 1
fi

for f in "${fixtures[@]}"; do
  [[ -f "$f" ]] || continue
  base=$(basename "$f" .anb)
  total=$((total+1))
  echo "=== $base ==="

  outd="$OUT_DIR/$base"
  mkdir -p "$outd"

  if ! parse_expectation "$f" "$base" accept_reject; then
    echo "  MALFORMED: $GATE_MALFORMED"
    echo "$GATE_MALFORMED" > "$outd/malformed.txt"
    failed=$((failed+1))
    jq --arg name "$base" --arg reason "$GATE_MALFORMED" --arg evidence_path "$outd" \
      '.fixtures += [{"name": $name, "status": "FAIL", "expected": "MALFORMED", "actual": "MALFORMED", "malformed_reason": $reason, "evidence_path": $evidence_path}]' \
      "$report" > "$REPORT_TMP" && mv "$REPORT_TMP" "$report"
    continue
  fi
  expect="$GATE_EXPECT"

  set +e
  cmd="$ANUBIS_BIN check $f --evidence --out $outd"
  echo "$cmd" > "$outd/command.txt"
  $ANUBIS_BIN check "$f" --evidence --out "$outd" > "$outd/run.log" 2>&1
  rc=$?
  set -e

  err_needle=$(grep -o 'ERROR_CONTAINS: .*' "$f" | sed 's/ERROR_CONTAINS: //' | head -1 || true)
  err_needle="${err_needle//$'\r'/}"

  # An `ERROR_CONTAINS:` on an `EXPECT: PASS` fixture is meaningless BY DEFINITION — a passing
  # program emits no error to contain — and the gate used to treat it as a real requirement, so
  # the fixture failed for its comment rather than its behaviour.
  #
  # That happened: a fixture rewritten to fix a tautology carried the annotation
  # `ERROR_CONTAINS: (none — this is a PASS fixture)`. `check` exited 0, the gate hunted for that
  # literal string in the log, did not find it, and reported FAIL. The corpus went 317 -> 316 and
  # the diagnosis pointed at the compiler, which was working correctly.
  #
  # A malformed fixture must be reported as MALFORMED, not as a failing test. Reading a FAIL and
  # hunting the compiler is exactly the cost this catches.
  if [[ "$expect" == "PASS" && -n "$err_needle" ]]; then
    echo "  MALFORMED: EXPECT: PASS fixture declares ERROR_CONTAINS ('$err_needle') — a passing program has no error to contain"
    printf 'EXPECT: PASS with ERROR_CONTAINS: %s\n' "$err_needle" > "$outd/malformed.txt"
    failed=$((failed+1))
    jq --arg name "$base" --arg reason "EXPECT: PASS fixture declares ERROR_CONTAINS" --arg evidence_path "$outd" \
      '.fixtures += [{"name": $name, "status": "FAIL", "expected": "MALFORMED", "actual": "MALFORMED", "malformed_reason": $reason, "evidence_path": $evidence_path}]' \
      "$report" > "$REPORT_TMP" && mv "$REPORT_TMP" "$report"
    continue
  fi

  cmd_failed=0
  if [[ $rc -ne 0 ]]; then
    cmd_failed=1
  fi
  if [[ -f "$outd/check-summary.json" ]] && grep -q '"check_error":' "$outd/check-summary.json" \
      && ! grep -q '"check_error": null' "$outd/check-summary.json"; then
    cmd_failed=1
  fi

  needle_present=0
  if [[ -n "$err_needle" ]] && grep -qF -- "$err_needle" "$outd/run.log" 2>/dev/null; then
    needle_present=1
  fi

  # Derive actual outcome with honesty rules (never treat wrong-failure as EXPECT FAIL pass).
  if [[ "$expect" == "PASS" ]]; then
    if [[ $cmd_failed -eq 0 ]]; then
      if [[ -z "$err_needle" || $needle_present -eq 1 ]]; then
        actual="PASS"
      else
        # Unexpected: PASS expected but required needle missing
        actual="FAIL"
      fi
    else
      actual="FAIL"
    fi
  else
    # EXPECT FAIL
    if [[ -n "$err_needle" ]]; then
      if [[ $cmd_failed -eq 1 && $needle_present -eq 1 ]]; then
        actual="FAIL"   # correct failure shape
      else
        # Command passed, or failed without the required needle → mismatch later
        # Represent as PASS so actual != expect when expect=FAIL without needle.
        if [[ $cmd_failed -eq 0 ]]; then
          actual="PASS"
        else
          # Failed but wrong reason — treat as wrong-pass signal for comparison:
          # use a distinct actual that will not equal FAIL only if... wait.
          # We need actual != FAIL when needle missing so status becomes FAIL.
          # Set actual to something other than FAIL: use "PASS" meaning "did not match FAIL criteria"
          actual="PASS"
          echo "  NEEDLE_MISSING: wanted '$err_needle' in run.log (rc=$rc)"
        fi
      fi
    else
      if [[ $cmd_failed -eq 1 ]]; then
        actual="FAIL"
      else
        actual="PASS"
      fi
    fi
  fi

  if score_fixture "$expect" "$actual"; then
    passed=$((passed+1))
    status="PASS"
  else
    failed=$((failed+1))
    status="FAIL"
    echo "  MISMATCH: expected $expect got $actual (rc=$rc needle_present=$needle_present)"
  fi

  evidence_path="$outd"
  jq --arg name "$base" --arg status "$status" --arg expect "$expect" --arg actual "$actual" \
     --arg cmd "$cmd" --argjson rc "$rc" --arg executed_via "$executed_via" --arg evidence_path "$evidence_path" \
     --arg needle "$err_needle" --argjson needle_present "$needle_present" \
    '.fixtures += [{"name": $name, "status": $status, "expected": $expect, "actual": $actual, "command": $cmd, "exit_code": $rc, "executed_via": $executed_via, "evidence_path": $evidence_path, "error_contains": $needle, "needle_present": $needle_present}]' \
    "$report" > "$REPORT_TMP" && mv "$REPORT_TMP" "$report"
done

set +e
finalize "$total" "$passed" "$failed" 0
final_rc=$?
# Coverage ratchet. `finalize` proves every fixture that RAN passed; it cannot notice that fewer
# fixtures ran than last time. A corpus that shrinks reports a smaller, greener number.
assert_floor "security_fixtures" "$total" "$ROOT/examples/security/.fixture_count_floor"
floor_rc=$?
set -e
overall="$GATE_FINAL_STATUS"
[[ "$overall" == "PASS" ]] || overall="FAIL"
if [[ $floor_rc -ne 0 ]]; then
  overall="FAIL"
  echo "Overall: FAIL ($GATE_FLOOR_ERROR)" >&2
fi

jq --arg overall "$overall" --argjson total "$total" --argjson passed "$passed" --argjson failed "$failed" \
  '. + {total: $total, passed: $passed, failed: $failed, overall_verdict: $overall}' \
  "$report" > "$REPORT_TMP" && mv "$REPORT_TMP" "$report"

echo "Report: $report"
echo "Overall: $overall ($passed/$total)"
if [[ "$overall" != "PASS" ]]; then
  exit 1
fi
