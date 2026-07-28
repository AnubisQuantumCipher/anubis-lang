#!/usr/bin/env bash
# ============================================================================
# anubis run — fail-closed meta-gate (GROK-PTAH)
# ============================================================================
# PASS of an instrumented bucket means that surface was *looked at* and is green.
# This gate refuses to equate that with "run is fail-closed as a whole".
#
# Buckets:
#   A  closed_corpus     — tests/fixtures/stdlib/*should_fail_closed.anb
#   B  permanent_controls— tests/fixtures/runtime/*.anb (top-level only)
#   C  graduated_open    — tests/fixtures/runtime/failclosed_open/*.anb
#                          (residuals that were red; must stay green after land)
#   D  enforcement       — tests/fixtures/runtime/failclosed_enforcement/*.anb
#   E  doc_ok            — tests/fixtures/stdlib/doc_ok/*.anb
#                          (IEEE leniency MUST stay PASS — proves nobody "fixed" inf/NaN)
#
# Inventory (required):
#   tests/fixtures/runtime/failclosed_inventory.json
#   - Lists OPEN residuals, DOC_OK exclusions, UNENUMERATED surfaces.
#   - Gate fails closed if the inventory file is missing or unreadable.
#   - overall_verdict never becomes PASS_RUNTIME_FAILCLOSED_WHOLE while any
#     open_named_residuals[].status is OPEN with severity BLOCKS_WHOLE_CLAIM,
#     or while unenumerated_surfaces is non-empty (honest: we have not finished
#     a full-builtin enumeration).
#
# overall_verdict:
#   FAIL                         — any instrumented bucket red/timeout-fail
#   PASS_INSTRUMENTED            — A–E all green; inventory present; WHOLE still false
#   PASS_RUNTIME_FAILCLOSED_WHOLE— only if inventory stamps allow (see stamp_rules);
#                                  currently unreachable without human FA seal +
#                                  clearing unenumerated list
#   TIMEOUT                      — only timeouts, no hard fails
#
# Usage:
#   bash scripts/run_run_failclosed_gate.sh
#   bash scripts/run_run_failclosed_gate.sh --out out/my_run --timeout 90
#   bash scripts/run_run_failclosed_gate.sh --self-test
# ============================================================================
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
source "$ROOT/scripts/lib/gate_common.sh"

STAMP="$(date +%Y%m%dT%H%M%S)_$$"
OUT_DIR="out/run_failclosed_gate/${STAMP}"
DEFAULT_TIMEOUT=120
SELF_TEST=0
INVENTORY="tests/fixtures/runtime/failclosed_inventory.json"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out) OUT_DIR="$2"; shift 2 ;;
    --timeout) DEFAULT_TIMEOUT="$2"; shift 2 ;;
    --self-test) SELF_TEST=1; shift ;;
    --inventory) INVENTORY="$2"; shift 2 ;;
    -h|--help)
      sed -n '2,50p' "$0"
      exit 0
      ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

mkdir -p "$OUT_DIR"
report="$OUT_DIR/run_failclosed_report.json"
runner="$ROOT/scripts/run_runtime_fixtures.sh"

if [[ -n "${ANUBIS_BIN:-}" ]]; then
  bin="$ANUBIS_BIN"
elif [[ -x "./target/release/anubis" ]]; then
  bin="./target/release/anubis"
elif [[ -x "./target/debug/anubis" ]]; then
  bin="./target/debug/anubis"
else
  echo "FATAL: no anubis binary (will not cargo build)" >&2
  exit 127
fi
[[ -x "$bin" ]] || { echo "FATAL: binary not executable: $bin" >&2; exit 127; }
export ANUBIS_BIN="$bin"
if [[ ! -f "$runner" ]]; then
  echo "FATAL: missing $runner" >&2
  exit 2
fi

bin_mtime="$(stat -f '%Sm' -t '%Y-%m-%dT%H:%M:%S' "$bin" 2>/dev/null || echo unknown)"

echo "run-failclosed meta-gate"
echo "  binary:    $bin mtime=$bin_mtime"
echo "  out:       $OUT_DIR"
echo "  inventory: $INVENTORY"
echo "  timeout:   ${DEFAULT_TIMEOUT}s per fixture"
echo

# --- self-test: instrument must go red ---
if [[ "$SELF_TEST" -eq 1 ]]; then
  echo "=== SELF-TEST (must go RED) ==="
  break_dir="scratchpad/fleet_20260726/grok_ptah_round4/instrument_break"
  if [[ ! -d "$break_dir" ]]; then
    # fallback: synthesize a break under out
    break_dir="$OUT_DIR/synth_break"
    mkdir -p "$break_dir"
    cat >"$break_dir/broken_expect_pass.anb" <<'EOF'
// EXPECT: PASS
// STDOUT_CONTAINS: ok
fn main() { print(1 / 0); }
EOF
  fi
  set +e
  bash "$runner" --dir "$break_dir" --glob 'broken_*.anb' --out "$OUT_DIR/self_test" --timeout 30 \
    >"$OUT_DIR/self_test.log" 2>&1
  st_rc=$?
  set -e
  st_verdict="$(jq -r '.overall_verdict // "FAIL"' "$OUT_DIR/self_test/runtime_fixture_report.json" 2>/dev/null || echo FAIL)"
  echo "  self_test: verdict=$st_verdict rc=$st_rc"
  if [[ "$st_verdict" == "FAIL" && "$st_rc" -eq 1 ]]; then
    echo "SELF-TEST: PASS (instrument correctly went red on deliberate break)"
    jq -n '{overall_verdict:"SELF_TEST_PASS",self_test:"instrument_goes_red"}' >"$report"
    exit 0
  fi
  echo "SELF-TEST: FAIL (instrument did not go red — gate is untrustworthy)" >&2
  jq -n '{overall_verdict:"SELF_TEST_FAIL"}' >"$report"
  exit 1
fi

# --- inventory required ---
if [[ ! -f "$INVENTORY" ]]; then
  echo "FATAL: missing inventory $INVENTORY — refuse clean bill without residual list" >&2
  exit 2
fi
if ! jq -e . "$INVENTORY" >/dev/null 2>&1; then
  echo "FATAL: inventory is not valid JSON: $INVENTORY" >&2
  exit 2
fi

blocks_open="$(jq '[.open_named_residuals[]? | select(.status=="OPEN" and .severity=="BLOCKS_WHOLE_CLAIM")] | length' "$INVENTORY")"
open_count="$(jq '[.open_named_residuals[]? | select(.status=="OPEN" or .status=="OPEN_SOFT")] | length' "$INVENTORY")"
unenum_count="$(jq '.unenumerated_surfaces | length' "$INVENTORY")"
doc_ok_count="$(jq '.doc_ok_excluded_from_failclosed_claim | length' "$INVENTORY")"

echo "inventory: blocks_whole_open=$blocks_open open_total=$open_count unenumerated=$unenum_count doc_ok_rows=$doc_ok_count"
echo

run_bucket() {
  local name="$1" dir="$2" glob="$3"
  local bdir="$OUT_DIR/buckets/$name"
  mkdir -p "$bdir"
  local log="$bdir/runner.log"
  set +e
  bash "$runner" --dir "$dir" --glob "$glob" --out "$bdir/run" --timeout "$DEFAULT_TIMEOUT" \
    >"$log" 2>&1
  local rc=$?
  set -e
  local verdict="FAIL" passed=0 total=0 failed=0 timed_out=0
  local j="$bdir/run/runtime_fixture_report.json"
  if [[ -f "$j" ]]; then
    verdict="$(jq -r '.overall_verdict // "FAIL"' "$j")"
    passed="$(jq -r '.passed // 0' "$j")"
    total="$(jq -r '.total // 0' "$j")"
    failed="$(jq -r '.failed // 0' "$j")"
    timed_out="$(jq -r '.timed_out // 0' "$j")"
  fi
  set +e
  finalize "$total" "$passed" "$failed" "$timed_out"
  local final_rc=$?
  set -e
  local status="FAIL"
  if [[ "$GATE_FINAL_STATUS" == "PASS" && "$verdict" == "PASS" && "$rc" -eq 0 ]]; then
    status="PASS"
  elif [[ "$GATE_FINAL_STATUS" == "INCOMPLETE" && ( "$verdict" == "PASS_WITH_TIMEOUTS" || "$verdict" == "TIMEOUT" || "$rc" -eq 2 ) ]]; then
    status="TIMEOUT"
  else
    status="FAIL"
  fi
  echo "  bucket[$name]: status=$status verdict=$verdict passed=$passed/$total failed=$failed timeouts=$timed_out rc=$rc finalize_rc=$final_rc"
  eval "BUCKET_${name}_STATUS='$status'"
  eval "BUCKET_${name}_PASSED='$passed'"
  eval "BUCKET_${name}_TOTAL='$total'"
  eval "BUCKET_${name}_FAILED='$failed'"
  eval "BUCKET_${name}_TIMEOUT='$timed_out'"
  eval "BUCKET_${name}_RC='$rc'"
}

echo "=== A closed_corpus (stdlib should_fail_closed) ==="
run_bucket closed_corpus "tests/fixtures/stdlib" '*should_fail_closed.anb'

echo "=== B permanent_controls ==="
run_bucket permanent_controls "tests/fixtures/runtime" '*.anb'

echo "=== C graduated residuals (failclosed_open — must stay green) ==="
run_bucket graduated_open "tests/fixtures/runtime/failclosed_open" '*.anb'

echo "=== D enforcement binding ==="
run_bucket enforcement "tests/fixtures/runtime/failclosed_enforcement" '*.anb'

echo "=== E doc_ok IEEE (must stay PASS — do not harden to panic) ==="
run_bucket doc_ok "tests/fixtures/stdlib/doc_ok" '*.anb'

# --- overall ---
any_timeout=0
any_fail=0
for b in closed_corpus permanent_controls graduated_open enforcement doc_ok; do
  eval "s=\$BUCKET_${b}_STATUS"
  if [[ "$s" == "TIMEOUT" ]]; then any_timeout=1; fi
  if [[ "$s" == "FAIL" ]]; then any_fail=1; fi
done

instrumented_ok=0
if [[ "$any_fail" -eq 0 && "$any_timeout" -eq 0 ]]; then
  instrumented_ok=1
fi

# Whole claim requires inventory seals
whole_ok=0
if [[ "$instrumented_ok" -eq 1 && "$blocks_open" -eq 0 && "$unenum_count" -eq 0 ]]; then
  whole_ok=1
fi

if [[ "$any_fail" -eq 1 ]]; then
  overall="FAIL"
elif [[ "$any_timeout" -eq 1 ]]; then
  overall="TIMEOUT"
elif [[ "$whole_ok" -eq 1 ]]; then
  overall="PASS_RUNTIME_FAILCLOSED_WHOLE"
else
  overall="PASS_INSTRUMENTED"
fi

# Human-readable blockers for WHOLE
blockers=()
if [[ "$blocks_open" -gt 0 ]]; then
  blockers+=("inventory has $blocks_open BLOCKS_WHOLE_CLAIM OPEN residual(s) (typically FA class)")
fi
if [[ "$unenum_count" -gt 0 ]]; then
  blockers+=("inventory lists $unenum_count unenumerated_surfaces — full run surface not systematically closed")
fi
if [[ "$open_count" -gt 0 ]]; then
  blockers+=("inventory has $open_count OPEN/OPEN_SOFT named residual(s)")
fi
blockers_json="$(printf '%s\n' "${blockers[@]+"${blockers[@]}"}" | jq -R . | jq -s .)"

jq -n \
  --arg overall "$overall" \
  --arg bin "$bin" \
  --arg mtime "$bin_mtime" \
  --arg out "$OUT_DIR" \
  --arg inv "$INVENTORY" \
  --argjson timeout "$DEFAULT_TIMEOUT" \
  --argjson blocks_open "$blocks_open" \
  --argjson open_count "$open_count" \
  --argjson unenum_count "$unenum_count" \
  --argjson doc_ok_count "$doc_ok_count" \
  --argjson instrumented_ok "$instrumented_ok" \
  --argjson whole_ok "$whole_ok" \
  --argjson blockers "$blockers_json" \
  --arg a_s "$BUCKET_closed_corpus_STATUS" --argjson a_p "$BUCKET_closed_corpus_PASSED" --argjson a_t "$BUCKET_closed_corpus_TOTAL" --argjson a_f "$BUCKET_closed_corpus_FAILED" \
  --arg b_s "$BUCKET_permanent_controls_STATUS" --argjson b_p "$BUCKET_permanent_controls_PASSED" --argjson b_t "$BUCKET_permanent_controls_TOTAL" --argjson b_f "$BUCKET_permanent_controls_FAILED" \
  --arg c_s "$BUCKET_graduated_open_STATUS" --argjson c_p "$BUCKET_graduated_open_PASSED" --argjson c_t "$BUCKET_graduated_open_TOTAL" --argjson c_f "$BUCKET_graduated_open_FAILED" \
  --arg d_s "$BUCKET_enforcement_STATUS" --argjson d_p "$BUCKET_enforcement_PASSED" --argjson d_t "$BUCKET_enforcement_TOTAL" --argjson d_f "$BUCKET_enforcement_FAILED" \
  --arg e_s "$BUCKET_doc_ok_STATUS" --argjson e_p "$BUCKET_doc_ok_PASSED" --argjson e_t "$BUCKET_doc_ok_TOTAL" --argjson e_f "$BUCKET_doc_ok_FAILED" \
  '{
     overall_verdict: $overall,
     meaning: (
       if $overall == "FAIL" then
         "An instrumented fail-closed or DOC_OK bucket is red. Do not claim run health."
       elif $overall == "PASS_INSTRUMENTED" then
         "Buckets A–E green: every surface we instrumented was looked at and is green. This is NOT fail-closed-as-a-whole while inventory still has BLOCKS_WHOLE_CLAIM OPEN and/or unenumerated_surfaces."
       elif $overall == "PASS_RUNTIME_FAILCLOSED_WHOLE" then
         "Instrumented surface green AND inventory has no BLOCKS_WHOLE_CLAIM OPEN and no unenumerated_surfaces. Still re-verify FA seal before public completion language."
       else "timeout" end
     ),
     claim_run_failclosed_as_a_whole: (if $overall == "PASS_RUNTIME_FAILCLOSED_WHOLE" then true else false end),
     claim_runtime_half_done: false,
     claim_runtime_half_done_note: "Never auto-set true by this gate. Operator stamp only after PASS_RUNTIME_FAILCLOSED_WHOLE + independent FA rehunt seal + DOC_OK policy acceptance.",
     inventory: {
       path: $inv,
       blocks_whole_open: $blocks_open,
       open_named_count: $open_count,
       unenumerated_count: $unenum_count,
       doc_ok_exclusion_rows: $doc_ok_count
     },
     whole_claim_blockers: $blockers,
     instrument: {path: $bin, mtime: $mtime},
     out_dir: $out,
     default_timeout_secs: $timeout,
     buckets: {
       closed_corpus: {status: $a_s, passed: $a_p, total: $a_t, failed: $a_f},
       permanent_controls: {status: $b_s, passed: $b_p, total: $b_t, failed: $b_f},
       graduated_open: {status: $c_s, passed: $c_p, total: $c_t, failed: $c_f},
       enforcement: {status: $d_s, passed: $d_p, total: $d_t, failed: $d_f},
       doc_ok: {status: $e_s, passed: $e_p, total: $e_t, failed: $e_f, role: "IEEE leniency must stay PASS"}
     }
   }' >"$report"

echo
echo "==================== SUMMARY ===================="
echo "overall_verdict:              $overall"
echo "claim_run_failclosed_as_a_whole: $([[ "$overall" == "PASS_RUNTIME_FAILCLOSED_WHOLE" ]] && echo true || echo false)"
echo "  A closed_corpus:      $BUCKET_closed_corpus_STATUS ($BUCKET_closed_corpus_PASSED/$BUCKET_closed_corpus_TOTAL)"
echo "  B permanent_controls: $BUCKET_permanent_controls_STATUS ($BUCKET_permanent_controls_PASSED/$BUCKET_permanent_controls_TOTAL)"
echo "  C graduated_open:     $BUCKET_graduated_open_STATUS ($BUCKET_graduated_open_PASSED/$BUCKET_graduated_open_TOTAL)"
echo "  D enforcement:        $BUCKET_enforcement_STATUS ($BUCKET_enforcement_PASSED/$BUCKET_enforcement_TOTAL)"
echo "  E doc_ok IEEE:        $BUCKET_doc_ok_STATUS ($BUCKET_doc_ok_PASSED/$BUCKET_doc_ok_TOTAL)"
echo "  inventory:            blocks_whole_open=$blocks_open open=$open_count unenumerated=$unenum_count"
if [[ ${#blockers[@]} -gt 0 ]]; then
  echo "  whole-claim blockers:"
  for b in "${blockers[@]}"; do echo "    - $b"; done
fi
echo "report: $report"
echo

# Coverage ratchet (adversary R49): open inventory total must not silently shrink.
# open_count is the enumerated open surface size printed in inventory.
set +e
assert_floor "run_failclosed_gate" "$open_count" "$ROOT/scripts/floors/run_failclosed_gate.count_floor"
_floor_rc=$?
set -e
if [[ $_floor_rc -ne 0 ]]; then
  echo "FLOOR: FAIL (open_count=$open_count; $GATE_FLOOR_ERROR)" >&2
  overall="FAIL"
fi

if [[ "$overall" == "FAIL" ]]; then
  echo "GATE: FAIL — instrumented surface is not all green"
  exit 1
fi
if [[ "$overall" == "TIMEOUT" ]]; then
  echo "GATE: TIMEOUT"
  exit 2
fi
if [[ "$overall" == "PASS_INSTRUMENTED" ]]; then
  echo "GATE: PASS_INSTRUMENTED — looked-at surfaces are green"
  echo "  STILL NOT fail-closed as a whole (see whole_claim_blockers in report)"
  # exit 0 so CI can require instrumented green without lying about WHOLE
  exit 0
fi
echo "GATE: PASS_RUNTIME_FAILCLOSED_WHOLE"
exit 0
