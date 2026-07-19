#!/usr/bin/env bash
# Phase-7 z3-authoritative flip gate. Proves the native QF_BV solver can CARRY the integer lane:
#
#   1. VERDICT EQUIVALENCE — `anubis check` over the whole corpus, default mode vs
#      `ANUBIS_NATIVE_AUTHORITATIVE=1` (z3 present, so every native verdict is cross-checked and any
#      disagreement fails closed + prints ANUBIS_NATIVE_DISAGREE). Requires: identical per-file exit
#      codes AND zero disagreement lines.
#   2. TCB-DROP DEMO — with z3 REMOVED from PATH: the pure-int proving fixture still checks green and
#      the violating fixture is still rejected under the flag, while WITHOUT the flag the same
#      z3-less check fails (proving z3 really was load-bearing before, i.e. the flip is what drops it).
#
# Exit 0 = z3 is demonstrably droppable for the integer lane (equivalence + demo hold).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
BIN=./target/release/anubis

CARGO_BUILD_JOBS=6 cargo build -q --release -p anubis

command -v z3 >/dev/null || { echo "FATAL: z3 not on PATH — the equivalence half needs it"; exit 1; }
command -v timeout >/dev/null || { echo "FATAL: coreutils timeout missing"; exit 1; }

files=$(find examples tests/fixtures -name '*.anb' | sort)
n=0; mismatches=0; disagreements=0
DISAGREE_LOG="$(mktemp)"

for f in $files; do
  n=$((n+1))
  set +e
  timeout 60 "$BIN" check "$f" >/dev/null 2>&1
  base_rc=$?
  ANUBIS_NATIVE_AUTHORITATIVE=1 timeout 120 "$BIN" check "$f" >/dev/null 2>"$DISAGREE_LOG.cur"
  auth_rc=$?
  set -e
  if grep -q "ANUBIS_NATIVE_DISAGREE" "$DISAGREE_LOG.cur"; then
    disagreements=$((disagreements+1))
    { echo "== $f =="; grep "ANUBIS_NATIVE_DISAGREE" "$DISAGREE_LOG.cur"; } >> "$DISAGREE_LOG"
  fi
  if [ "$base_rc" != "$auth_rc" ]; then
    mismatches=$((mismatches+1))
    echo "VERDICT MISMATCH: $f (default rc=$base_rc, authoritative rc=$auth_rc)"
  fi
done
rm -f "$DISAGREE_LOG.cur"

echo "NATIVE_AUTHORITATIVE equivalence over $n files: mismatches=$mismatches disagreements=$disagreements"
if [ "$disagreements" -gt 0 ]; then cat "$DISAGREE_LOG"; fi
rm -f "$DISAGREE_LOG"

# ---- TCB-drop demo: z3 hidden from PATH ----
PASS_FIX=tests/fixtures/native_authoritative/int_contract_proves.anb
FAIL_FIX=tests/fixtures/native_authoritative/int_contract_violates.anb
demo_fail=0

set +e
PATH=/nonexistent ANUBIS_NATIVE_AUTHORITATIVE=1 "$BIN" check "$PASS_FIX" >/dev/null 2>&1
[ $? -eq 0 ] || { echo "DEMO FAIL: native-authoritative could not prove $PASS_FIX without z3"; demo_fail=1; }
PATH=/nonexistent ANUBIS_NATIVE_AUTHORITATIVE=1 "$BIN" check "$FAIL_FIX" >/dev/null 2>&1
[ $? -ne 0 ] || { echo "DEMO FAIL: native-authoritative ACCEPTED the violating $FAIL_FIX without z3"; demo_fail=1; }
# Control: without the flag, a z3-less check of the SAME green fixture must fail — z3 was load-bearing.
PATH=/nonexistent "$BIN" check "$PASS_FIX" >/dev/null 2>&1
[ $? -ne 0 ] || { echo "DEMO FAIL: default mode passed WITHOUT z3 — control invalid (z3 not load-bearing?)"; demo_fail=1; }
set -e

if [ "$mismatches" = 0 ] && [ "$disagreements" = 0 ] && [ "$demo_fail" = 0 ]; then
  echo "NATIVE_AUTHORITATIVE_GATE: PASS (verdict-equivalent on $n files; native alone proves+rejects the int fixtures with z3 hidden)"
  exit 0
fi
echo "NATIVE_AUTHORITATIVE_GATE: FAIL"
exit 1
