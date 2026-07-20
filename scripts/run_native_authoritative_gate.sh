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
# Phase-7 fragment gate: a TRUE property built from an UNPROVEN op (bvashr) must FAIL CLOSED z3-free.
DANGER_FIX=tests/fixtures/native_authoritative/int_contract_danger_defers.anb
demo_fail=0

set +e
PATH=/nonexistent ANUBIS_NATIVE_AUTHORITATIVE=1 "$BIN" check "$PASS_FIX" >/dev/null 2>&1
[ $? -eq 0 ] || { echo "DEMO FAIL: native-authoritative could not prove $PASS_FIX without z3"; demo_fail=1; }
PATH=/nonexistent ANUBIS_NATIVE_AUTHORITATIVE=1 "$BIN" check "$FAIL_FIX" >/dev/null 2>&1
[ $? -ne 0 ] || { echo "DEMO FAIL: native-authoritative ACCEPTED the violating $FAIL_FIX without z3"; demo_fail=1; }
# Control: without the flag, a z3-less check of the SAME green fixture must fail — z3 was load-bearing.
PATH=/nonexistent "$BIN" check "$PASS_FIX" >/dev/null 2>&1
[ $? -ne 0 ] || { echo "DEMO FAIL: default mode passed WITHOUT z3 — control invalid (z3 not load-bearing?)"; demo_fail=1; }
# FRAGMENT-GATE SOUNDNESS: the danger fixture's property is TRUE (so it PASSES with z3), but its op is
# unproven, so z3-free + authoritative it must FAIL CLOSED (native declines, no z3 to defer to). This is
# what proves native's z3-free authority is bounded to the machine-checked fragment, not the full blaster.
"$BIN" check "$DANGER_FIX" >/dev/null 2>&1
[ $? -eq 0 ] || { echo "DEMO FAIL: danger fixture $DANGER_FIX should PASS with z3 present (property is true)"; demo_fail=1; }
PATH=/nonexistent ANUBIS_NATIVE_AUTHORITATIVE=1 "$BIN" check "$DANGER_FIX" >/dev/null 2>&1
[ $? -ne 0 ] || { echo "DEMO FAIL: danger fixture $DANGER_FIX proved z3-free on an UNPROVEN blast — the fragment gate did not fire"; demo_fail=1; }
set -e

# ---- Drift check: the Rust allow-list (fragment.rs PROVEN_OP_TAGS) must be backed by live Lean proofs,
# and must NOT admit any deferred op. Ties the authoritative fragment to formal/Anubis/BitBlast.lean so
# an op cannot ride as authoritative without a green *_correct/value-lemma theorem. ----
drift_fail=0
LEAN=formal/Anubis/BitBlast.lean
FRAG=solver/src/fragment.rs
# Every admitted op's backing theorem/lemma must exist in BitBlast.lean.
for thm in rippleCarry_spec ult_correct slt_correct ule_correct sle_correct eqBits_correct \
           andBits_correct orBits_correct xorBits_correct subBits_correct negBits_correct \
           mulConst_correct shlConst_correct barrelShl_correct shrConstL_correct barrelLshr_correct \
           bitsToNat_not bitsToNat_append_list bitsToNat_extract bitsToNat_append_replicate_false; do
  grep -q "\b$thm\b" "$LEAN" || { echo "DRIFT: fragment admits an op but its backing '$thm' is MISSING from $LEAN"; drift_fail=1; }
done
# No DEFERRED op name may appear in PROVEN_OP_TAGS (guards accidental admission of unproven wiring).
# (Xor left this list 2026-07-20 when xorBits_correct landed — the drift gate itself caught the move;
#  Sub/Neg left the same day with subBits/negBits_correct.)
TAGS=$(sed -n '/PROVEN_OP_TAGS/,/];/p' "$FRAG")
for deferred in Ashr SignExtend Udiv Urem Sdiv Srem Ite; do
  echo "$TAGS" | grep -qw "\"$deferred\"" && { echo "DRIFT: deferred op '$deferred' is listed in PROVEN_OP_TAGS (unproven wiring admitted)"; drift_fail=1; }
done

if [ "$mismatches" = 0 ] && [ "$disagreements" = 0 ] && [ "$demo_fail" = 0 ] && [ "$drift_fail" = 0 ]; then
  echo "NATIVE_AUTHORITATIVE_GATE: PASS (verdict-equivalent on $n files; native alone proves+rejects the int fixtures with z3 hidden; fragment gate fails closed on an unproven op; allow-list ↔ Lean proofs in sync)"
  exit 0
fi
echo "NATIVE_AUTHORITATIVE_GATE: FAIL"
exit 1
