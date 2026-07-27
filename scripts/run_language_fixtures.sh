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
REPORT_TMP="$report.tmp.$$"
echo '{"fixtures": [], "overall_verdict": "PENDING"}' > "$report"

# Pin instrument once for the whole run. Default remains `cargo run --` (DEBUG) so historical
# numbers do not shift; seal runs must set ANUBIS_BIN to the same release snapshot as security.
if [[ -n "${ANUBIS_BIN:-}" ]]; then
  LANG_CMD=("$ANUBIS_BIN")
  executed_via="preset:$ANUBIS_BIN"
else
  LANG_CMD=(cargo run --)
  executed_via="cargo-run-debug-default"
fi
echo "instrument: via=$executed_via out=$OUT_DIR (security grades release by default; this gate defaults DEBUG)" \
  | tee "$OUT_DIR/instrument.txt"

total=0
passed=0
failed=0

for f in "$FIXTURE_DIR"/*.anb; do
  [[ -f "$f" ]] || continue
  base=$(basename "$f" .anb)
  total=$((total+1))
  echo "=== $base ==="

  outd="$OUT_DIR/$base"
  # Start from a clean per-fixture dir: `check --evidence` writes a fresh timestamped
  # `evidence-*/` each run, and the FAIL-detection below globs `evidence-*`, so a stale dir from
  # a prior run would produce a false failure. Removing it first makes the runner deterministic.
  rm -rf "$outd"
  mkdir -p "$outd"
  set +e
  # NOTE: the default is `cargo run --`, i.e. the DEBUG binary — while `run_security_fixtures.sh`
  # grades `./target/release/anubis`. The two headline gates therefore measure DIFFERENT builds of
  # the compiler (debug additionally panics on integer overflow), and this one takes the cargo build
  # lock once per fixture, so it cannot run alongside a build. The default is left unchanged here so
  # no historical number shifts silently; set ANUBIS_BIN to pin a specific binary instead.
  "${LANG_CMD[@]}" check "$f" --evidence --out "$outd" > "$outd/run.log" 2>&1
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
  if [[ $has_check_error -eq 1 || $log_has_check_failed -eq 1 ]]; then
    failure_run=1
  fi
  # Also detect solver/assert FAIL in evidence for symbolic cases
  if grep -q '"status": "FAIL"' "$outd"/evidence-*/solver.json 2>/dev/null || grep -q '"status": "FAIL"' "$outd"/evidence-*/evidence.json 2>/dev/null || grep -q '"status": "FAIL"' "$outd"/evidence-*/checks.sarif 2>/dev/null; then
    failure_run=1
  fi

  # Verdict is DERIVED from the tool's own recorded verdict PLUS a positive success signal.
  # It never defaults to PASS: a run with no positive confirmation is UNKNOWN (which fails a PASS
  # expectation). This closes the false-green where a no-op checker scored PASS on rc==0 alone.
  summary_verdict="$(grep -oE '"verdict": *"(PASS|FAIL)"' "$outd/check-summary.json" 2>/dev/null | grep -oE 'PASS|FAIL' | head -1 || echo "")"
  if [[ $failure_run -eq 1 || "$summary_verdict" == "FAIL" ]]; then
    verdict="FAIL"
  elif [[ "$summary_verdict" == "PASS" ]] && grep -q "check passed (no policy violations)" "$outd/run.log" 2>/dev/null; then
    verdict="PASS"
  else
    verdict="UNKNOWN"
  fi

  needle_ok=1
  if [[ -n "$err_needle" ]]; then
    needle_ok=0
    if grep -qi "$err_needle" "$outd"/* 2>/dev/null || grep -qi "$err_needle" "$outd/run.log" 2>/dev/null || grep -qi "$err_needle" "$outd/check-summary.json" 2>/dev/null || grep -qi "$err_needle" "$outd/check_diagnostics.txt" 2>/dev/null; then
      needle_ok=1
    fi
  fi

  ok=0
  if [[ "$expect" == "PASS" ]]; then
    # PASS means the command completed and no parser/type/taint/solver evidence failed.
    if [[ $rc -eq 0 && $failure_run -eq 0 && "$verdict" == "PASS" ]]; then
      ok=1
    fi
  else
    # FAIL means a real failure was observed; if a diagnostic needle is present, it must match too.
    if [[ $needle_ok -eq 1 && ( $rc -ne 0 || $failure_run -eq 1 || "$verdict" == "FAIL" ) ]]; then
      ok=1
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
     '.fixtures += [{"name":$b, "expected":$e, "actual":$v, "status":$s}]' "$report" > "$REPORT_TMP" && mv "$REPORT_TMP" "$report"
done

if [[ $failed -eq 0 ]]; then
  overall="PASS"
else
  overall="FAIL"
fi

jq --arg o "$overall" --argjson t $total --argjson p $passed --argjson f $failed \
   '.overall_verdict = $o | .total = $t | .passed = $p | .failed = $f' "$report" > "$REPORT_TMP" && mv "$REPORT_TMP" "$report"

echo "Report: $report"
echo "Overall: $overall ($passed/$total)"
[[ "$overall" == "PASS" ]] || exit 1
