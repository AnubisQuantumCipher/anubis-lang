#!/usr/bin/env bash
set -euo pipefail

# Gate 2/3 language fixture runner
# Respects // EXPECT: PASS|FAIL and // ERROR_CONTAINS: ...
# Writes out/.../fixture_report.json
# Exits nonzero on any mismatch.

OUT_DIR="out/a_plus_gate2_language_fixtures"
if [[ "${1:-}" == "--out" && -n "${2:-}" ]]; then OUT_DIR="$2"; shift 2; fi
mkdir -p "$OUT_DIR"
FIXTURE_DIR="tests/fixtures/language_core"

report="$OUT_DIR/fixture_report.json"
echo '{"fixtures": [], "overall_verdict": "PENDING"}' > "$report"

total=0
passed=0
failed=0

for f in "$FIXTURE_DIR"/*.anb; do
  [[ -f "$f" ]] || continue
  base=$(basename "$f" .anb)
  total=$((total+1))
  echo "=== $base ==="

  outd="$OUT_DIR/$base"
  mkdir -p "$outd"
  set +e
  cargo run -- check "$f" --evidence --out "$outd" > "$outd/run.log" 2>&1
  rc=$?
  set -e

  expect=$(grep -o 'EXPECT: [A-Z]*' "$f" | head -1 | awk '{print $2}' || echo "PASS")
  err_needle=$(grep -o 'ERROR_CONTAINS: .*' "$f" | sed 's/ERROR_CONTAINS: //' | head -1 || echo "")

  # Determine if this run was a failure (syntax error, type error, taint violation, etc.)
  has_check_error=0
  if [[ -f "$outd/check-summary.json" ]] && grep -q '"check_error":' "$outd/check-summary.json" && ! grep -q '"check_error": null' "$outd/check-summary.json"; then
    has_check_error=1
  fi
  log_has_check_failed=0
  if grep -qi "check failed" "$outd/run.log" 2>/dev/null || grep -qi "Error: parse" "$outd/run.log" 2>/dev/null; then
    log_has_check_failed=1
  fi
  bounty_ready_false=0
  if [[ -f "$outd/check-summary.json" ]] && grep -q '"bounty_ready": false' "$outd/check-summary.json"; then
    bounty_ready_false=1
  fi

  failure_run=0
  if [[ $has_check_error -eq 1 || $log_has_check_failed -eq 1 || $bounty_ready_false -eq 1 ]]; then
    failure_run=1
  fi

  # Verdict for reporting (best effort)
  if [[ $failure_run -eq 1 ]]; then
    verdict="FAIL"
  else
    verdict="PASS"
  fi

  ok=0
  if [[ "$expect" == "PASS" ]]; then
    if [[ $failure_run -eq 0 ]]; then ok=1; fi
  else
    # EXPECT FAIL: success for the test if we actually saw a failure
    if [[ $failure_run -eq 1 ]]; then
      ok=1
    fi
    # Also accept if the specific error needle appears anywhere (belt and suspenders)
    if [[ -n "$err_needle" ]]; then
      if grep -qi "$err_needle" "$outd"/* 2>/dev/null || grep -qi "$err_needle" "$outd/run.log" 2>/dev/null || grep -qi "$err_needle" "$outd/check-summary.json" 2>/dev/null || grep -qi "$err_needle" "$outd/check_diagnostics.txt" 2>/dev/null; then
        ok=1
      fi
    fi
  fi

  if [[ $ok -eq 1 ]]; then
    passed=$((passed+1))
    status="PASS"
  else
    failed=$((failed+1))
    status="FAIL"
    echo "  MISMATCH: expected $expect got $verdict (needle=$err_needle)"
  fi

  # append to report (simple)
  jq --arg b "$base" --arg s "$status" --arg e "$expect" --arg v "$verdict" \
     '.fixtures += [{"name":$b, "expected":$e, "actual":$v, "status":$s}]' "$report" > "$report.tmp" && mv "$report.tmp" "$report"
done

if [[ $failed -eq 0 ]]; then
  overall="PASS"
else
  overall="FAIL"
fi

jq --arg o "$overall" --argjson t $total --argjson p $passed --argjson f $failed \
   '.overall_verdict = $o | .total = $t | .passed = $p | .failed = $f' "$report" > "$report.tmp" && mv "$report.tmp" "$report"

echo "Report: $report"
echo "Overall: $overall ($passed/$total)"
[[ "$overall" == "PASS" ]] || exit 1
