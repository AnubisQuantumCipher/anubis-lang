#!/usr/bin/env bash
# Stdlib fail-closed RUNTIME fixture gate.
#
# The defects this gate covers live in the embedded runtime (compiler/src/backends/run.rs
# ANUBIS_CORE_RUNTIME_RS). They are invisible to `anubis check` — every fixture exits 0 under
# check today. This runner uses `anubis run` and the same honesty contract as
# scripts/run_security_fixtures.sh:
#
#   EXPECT PASS  → run exit 0 AND (no ERROR_CONTAINS OR needle present in log)
#   EXPECT FAIL without ERROR_CONTAINS → run nonzero
#   EXPECT FAIL with ERROR_CONTAINS    → run MUST fail AND needle MUST appear in log.
#                                        Exit nonzero for a different reason without the
#                                        needle is a MISMATCH (fixture FAIL), never green.
#
# Sealed state (2026-07-27): EXPECT FAIL fixtures panic with ANUBIS_* codes under `anubis run`.
# Gate is GREEN when 32/32 match ERROR_CONTAINS. Do not weaken fixtures to PASS.
#
# Usage:
#   bash scripts/run_stdlib_failclosed_gate.sh
#   bash scripts/run_stdlib_failclosed_gate.sh --out out/my_run
#   bash scripts/run_stdlib_failclosed_gate.sh --dir tests/fixtures/stdlib
#   bash scripts/run_stdlib_failclosed_gate.sh --glob '*should_fail_closed.anb'
#
# Does NOT rebuild the binary (fleet multi-agent cargo lock discipline). Uses
# ./target/release/anubis if present, else ./target/debug/anubis.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

OUT_DIR="out/stdlib_failclosed_gate"
FIXTURE_DIR="tests/fixtures/stdlib"
GLOB_PAT='*should_fail_closed.anb'

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out) OUT_DIR="$2"; shift 2 ;;
    --dir) FIXTURE_DIR="$2"; shift 2 ;;
    --glob) GLOB_PAT="$2"; shift 2 ;;
    -h|--help)
      sed -n '2,30p' "$0"
      exit 0
      ;;
    *)
      echo "unknown arg: $1" >&2
      exit 2
      ;;
  esac
done

mkdir -p "$OUT_DIR"
report="$OUT_DIR/stdlib_failclosed_report.json"
REPORT_TMP="$report.tmp.$$"
echo '{"fixtures": [], "overall_verdict": "PENDING"}' > "$report"

if [[ -n "${ANUBIS_BIN:-}" ]]; then
  executed_via="preset"
elif [[ -x "./target/release/anubis" ]]; then
  ANUBIS_BIN="./target/release/anubis"
  executed_via="release"
elif [[ -x "./target/debug/anubis" ]]; then
  ANUBIS_BIN="./target/debug/anubis"
  executed_via="debug"
else
  echo "FATAL: no anubis binary at target/release/anubis or target/debug/anubis (will not cargo build — fleet lock)" >&2
  exit 127
fi

# Record the instrument: binary identity so a stale-binary false-green is detectable.
bin_mtime="$(stat -f '%Sm' -t '%Y-%m-%dT%H:%M:%S' "$ANUBIS_BIN" 2>/dev/null || stat -c '%y' "$ANUBIS_BIN" 2>/dev/null || echo unknown)"
bin_size="$(stat -f '%z' "$ANUBIS_BIN" 2>/dev/null || stat -c '%s' "$ANUBIS_BIN" 2>/dev/null || echo 0)"
echo "instrument: $ANUBIS_BIN mtime=$bin_mtime size=$bin_size via=$executed_via" | tee "$OUT_DIR/instrument.txt"

total=0
passed=0
failed=0
timed_out=0

shopt -s nullglob
fixtures=( "$FIXTURE_DIR"/$GLOB_PAT )
shopt -u nullglob

if [[ ${#fixtures[@]} -eq 0 ]]; then
  echo "FATAL: no fixtures match $FIXTURE_DIR/$GLOB_PAT" >&2
  jq --arg overall FAIL --argjson total 0 --argjson passed 0 --argjson failed 0 \
     --arg bin "$ANUBIS_BIN" --arg mtime "$bin_mtime" \
     '. + {total:0, passed:0, failed:0, overall_verdict:"FAIL", instrument:{path:$bin, mtime:$mtime}, note:"no fixtures matched"}' \
     "$report" > "$REPORT_TMP" && mv "$REPORT_TMP" "$report"
  exit 1
fi

for f in "${fixtures[@]}"; do
  [[ -f "$f" ]] || continue
  base=$(basename "$f" .anb)
  total=$((total + 1))
  echo "=== $base ==="

  outd="$OUT_DIR/$base"
  rm -rf "$outd"
  mkdir -p "$outd"

  expect=$(grep -oE 'EXPECT: (PASS|FAIL)' "$f" | head -1 | awk '{print $2}' || true)
  if [[ -z "$expect" ]]; then
    expect="PASS"
  fi
  err_needle=$(grep -o 'ERROR_CONTAINS: .*' "$f" | sed 's/ERROR_CONTAINS: //' | head -1 || true)
  err_needle="${err_needle//$'\r'/}"
  # Strip trailing comments after needle if any (keep first token-ish phrase)
  err_needle="${err_needle%% // *}"
  err_needle="$(echo -n "$err_needle" | sed 's/[[:space:]]*$//')"

  cmd="$ANUBIS_BIN run $f"
  echo "$cmd" > "$outd/command.txt"
  set +e
  # Bound compile+run so a hang cannot freeze the gate (fleet: instrument must terminate).
  if command -v timeout >/dev/null 2>&1; then
    timeout 120 $ANUBIS_BIN run "$f" >"$outd/run.log" 2>&1
    rc=$?
  else
    $ANUBIS_BIN run "$f" >"$outd/run.log" 2>&1
    rc=$?
  fi
  set -e

  # Timeout is its own bucket — never score as EXPECT FAIL PASS (Seshat / runtime_fixtures pattern).
  is_timeout=0
  if [[ $rc -eq 124 || $rc -eq 137 ]]; then
    is_timeout=1
  fi

  cmd_failed=0
  if [[ $rc -ne 0 && $is_timeout -eq 0 ]]; then
    cmd_failed=1
  fi
  # Rust panic / anubis run wrapper also counts as failure even if a wrapper remaps exit.
  if [[ $is_timeout -eq 0 ]] && grep -qE 'ANUBIS_[A-Z0-9_]+|panicked at|Error: run failed' "$outd/run.log" 2>/dev/null; then
    if [[ $rc -ne 0 ]] || grep -qE 'panicked at|Error: run failed|exit_code=Some\(101\)' "$outd/run.log" 2>/dev/null; then
      cmd_failed=1
    fi
  fi

  needle_present=0
  if [[ -n "$err_needle" ]] && grep -qF -- "$err_needle" "$outd/run.log" 2>/dev/null; then
    needle_present=1
  fi

  if [[ $is_timeout -eq 1 ]]; then
    actual="TIMEOUT"
    echo "  TIMEOUT: budget exceeded (rc=$rc) — not scored PASS or FAIL"
    timed_out=$((timed_out + 1))
    status="TIMEOUT"
  else
    # Honesty: never treat wrong-failure as EXPECT FAIL pass.
    if [[ "$expect" == "PASS" ]]; then
      if [[ $cmd_failed -eq 0 ]]; then
        if [[ -z "$err_needle" || $needle_present -eq 1 ]]; then
          actual="PASS"
        else
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
          actual="PASS"
          if [[ $cmd_failed -eq 0 ]]; then
            echo "  SILENT_SUCCESS: expected fail with '$err_needle' but exit=$rc (wrong-value factory still open)"
          else
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
      passed=$((passed + 1))
      status="PASS"
    else
      failed=$((failed + 1))
      status="FAIL"
      echo "  MISMATCH: expected $expect got $actual (rc=$rc needle_present=$needle_present)"
    fi
  fi

  # Prefer python for JSON append if jq missing; try jq first.
  if command -v jq >/dev/null 2>&1; then
    jq --arg name "$base" --arg status "$status" --arg expect "$expect" --arg actual "$actual" \
       --arg cmd "$cmd" --argjson rc "$rc" --arg executed_via "$executed_via" \
       --arg needle "$err_needle" --argjson needle_present "$needle_present" \
       --arg path "$f" \
      '.fixtures += [{"name":$name,"path":$path,"status":$status,"expected":$expect,"actual":$actual,"command":$cmd,"exit_code":$rc,"executed_via":$executed_via,"error_contains":$needle,"needle_present":$needle_present}]' \
      "$report" > "$REPORT_TMP" && mv "$REPORT_TMP" "$report"
  else
    # Minimal fallback: append a line to a side TSV; JSON stays skeleton.
    printf '%s\t%s\t%s\t%s\t%s\n' "$base" "$status" "$expect" "$actual" "$rc" >> "$OUT_DIR/fixtures.tsv"
  fi
done

overall="PASS"
if [[ $failed -gt 0 || $total -eq 0 ]]; then
  overall="FAIL"
elif [[ $timed_out -gt 0 ]]; then
  overall="FAIL"
fi

if command -v jq >/dev/null 2>&1; then
  jq --arg overall "$overall" --argjson total "$total" --argjson passed "$passed" --argjson failed "$failed" \
     --argjson timed_out "$timed_out" \
     --arg bin "$ANUBIS_BIN" --arg mtime "$bin_mtime" --argjson size "$bin_size" --arg via "$executed_via" \
    '. + {total:$total, passed:$passed, failed:$failed, timed_out:$timed_out, overall_verdict:$overall, instrument:{path:$bin, mtime:$mtime, size_bytes:$size, via:$via}}' \
    "$report" > "$REPORT_TMP" && mv "$REPORT_TMP" "$report"
fi

echo "Report: $report"
echo "Overall: $overall ($passed/$total) timed_out=$timed_out"
echo "Instrument: $ANUBIS_BIN mtime=$bin_mtime"

if [[ "$overall" != "PASS" ]]; then
  if [[ $timed_out -gt 0 && $failed -eq 0 ]]; then
    exit 2
  fi
  exit 1
fi
exit 0
