#!/usr/bin/env bash
set -euo pipefail

# Gate 15 — Security Superpowers fixture runner
# Respects // EXPECT: PASS|FAIL and // ERROR_CONTAINS: ...
# Produces security_fixture_report.json with overall_verdict

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
  # Use built bin for speed/repeatability in Gate15 real runs; falls back to cargo run -p anubis if no bin
  if [[ -x "./target/debug/anubis" ]]; then
    ANUBIS_BIN="./target/debug/anubis"
  else
    ANUBIS_BIN="cargo run -p anubis --"
  fi
  $ANUBIS_BIN check "$f" --evidence --out "$outd" > "$outd/run.log" 2>&1
  rc=$?
  set -e

  expect=$(grep -o 'EXPECT: [A-Z]*' "$f" | head -1 | awk '{print $2}' || echo "PASS")
  err_needle=$(grep -o 'ERROR_CONTAINS: .*' "$f" | sed 's/ERROR_CONTAINS: //' | head -1 || echo "")

  actual="PASS"
  if [[ $rc -ne 0 ]]; then
    actual="FAIL"
  fi
  if [[ -f "$outd/check-summary.json" ]] && grep -q '"check_error":' "$outd/check-summary.json" && ! grep -q '"check_error": null' "$outd/check-summary.json"; then
    actual="FAIL"
  fi
  if [[ -n "$err_needle" ]] && ! grep -q "$err_needle" "$outd/run.log" 2>/dev/null; then
    actual="FAIL"
  fi

  if [[ "$actual" == "$expect" ]]; then
    passed=$((passed+1))
    status="PASS"
  else
    failed=$((failed+1))
    status="FAIL"
    echo "  MISMATCH: expected $expect got $actual"
  fi

  jq --arg name "$base" --arg status "$status" --arg expect "$expect" --arg actual "$actual" \
    '.fixtures += [{"name": $name, "status": $status, "expected": $expect, "actual": $actual}]' \
    "$report" > "$report.tmp" && mv "$report.tmp" "$report"
done

overall="PASS"
if [[ $failed -gt 0 ]]; then
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
