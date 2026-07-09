#!/usr/bin/env bash
set -euo pipefail

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

report="$OUT_DIR/security_fixture_report.json"
echo '{"fixtures": [], "overall_verdict": "PENDING"}' > "$report"

total=0
passed=0
failed=0

for f in "$SECURITY_DIR"/*.anb; do
  [[ -f "$f" ]] || continue
  base=$(basename "$f" .anb)
  total=$((total+1))
  echo "=== $base ==="

  outd="$OUT_DIR/$base"
  mkdir -p "$outd"
  set +e
  if [[ -x "./target/release/anubis" ]]; then
    ANUBIS_BIN="./target/release/anubis"
    executed_via="release"
  elif [[ -x "./target/debug/anubis" ]]; then
    ANUBIS_BIN="./target/debug/anubis"
    executed_via="debug"
  else
    ANUBIS_BIN="cargo run -p anubis --"
    executed_via="cargo"
  fi
  cmd="$ANUBIS_BIN check $f --evidence --out $outd"
  echo "$cmd" > "$outd/command.txt"
  $ANUBIS_BIN check "$f" --evidence --out "$outd" > "$outd/run.log" 2>&1
  rc=$?
  set -e

  expect=$(grep -oE 'EXPECT: (PASS|FAIL)' "$f" | head -1 | awk '{print $2}' || true)
  if [[ -z "$expect" ]]; then
    expect="PASS"
  fi
  err_needle=$(grep -o 'ERROR_CONTAINS: .*' "$f" | sed 's/ERROR_CONTAINS: //' | head -1 || true)
  err_needle="${err_needle//$'\r'/}"

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

  if [[ "$actual" == "$expect" ]]; then
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
    "$report" > "$report.tmp" && mv "$report.tmp" "$report"
done

overall="PASS"
if [[ $failed -gt 0 ]]; then
  overall="FAIL"
fi
if [[ $total -eq 0 ]]; then
  overall="FAIL"
fi

jq --arg overall "$overall" --argjson total "$total" --argjson passed "$passed" --argjson failed "$failed" \
  '. + {total: $total, passed: $passed, failed: $failed, overall_verdict: $overall}' \
  "$report" > "$report.tmp" && mv "$report.tmp" "$report"

echo "Report: $report"
echo "Overall: $overall ($passed/$total)"
if [[ "$overall" != "PASS" ]]; then
  exit 1
fi
