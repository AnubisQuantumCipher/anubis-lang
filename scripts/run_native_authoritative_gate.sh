#!/usr/bin/env bash
# Phase-7 native SMT authority gate (post default-flip 2026-07-25).
#
#   0. RUP CERT SUITE — every Unsat from CDCL carries a checkable certificate; adversarial
#      forgeries are rejected (`cargo test -p anubis-solver lrat` + sat Unsat emission).
#   1. VERDICT EQUIVALENCE — default mode vs explicit `ANUBIS_NATIVE_AUTHORITATIVE=1` over the
#      corpus (z3 present ⇒ native verdicts are cross-checked; disagreement fails closed).
#      Requires: identical per-file exit codes AND zero ANUBIS_NATIVE_DISAGREE lines.
#   2. TCB-DROP DEMO — with z3 REMOVED from PATH:
#        - default (native-authoritative) proves the good int fixture and rejects the bad one;
#        - opt-out `ANUBIS_NATIVE_AUTHORITATIVE=0` makes the same good fixture FAIL (z3 still
#          load-bearing when native is disabled);
#        - danger op (unproven blast) fails closed z3-free on the default path.
#      Unsat requires a verified RUP certificate (fail-closed if missing/invalid).
#
# Exit 0 = native default is safe for the proven integer fragment (cert + equivalence + demo).
# Opt out of default: ANUBIS_NATIVE_AUTHORITATIVE=0.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
source "$ROOT/scripts/lib/gate_common.sh"
# Seshat: honor seal pin. Never rebuild under ANUBIS_BIN (stale/wrong binary must stay wrong).
if [[ -n "${ANUBIS_BIN:-}" ]]; then
  BIN="$ANUBIS_BIN"
  [[ -x "$BIN" ]] || { echo "NATIVE_AUTHORITATIVE_GATE: FAIL (ANUBIS_BIN=$BIN not executable)"; exit 127; }
else
  BIN=./target/release/anubis
  CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-6}" cargo build -q --release -p anubis
  [[ -x "$BIN" ]] || { echo "NATIVE_AUTHORITATIVE_GATE: FAIL (no binary at $BIN)"; exit 127; }
fi

# ---- Certificate path (RUP/LRAT emit + independent checker) ----
cert_fail=0
if ! cargo test -q -p anubis-solver lrat -- --test-threads=4; then
  echo "FATAL: anubis-solver lrat certificate tests failed"
  cert_fail=1
fi
if ! cargo test -q -p anubis-solver sat:: -- --test-threads=4; then
  echo "FATAL: anubis-solver sat tests failed (Unsat cert emission)"
  cert_fail=1
fi
if [ "$cert_fail" -ne 0 ]; then
  echo "NATIVE_AUTHORITATIVE_GATE: FAIL (certificate suite)"
  exit 1
fi
echo "NATIVE_AUTHORITATIVE cert suite: PASS (lrat + sat Unsat certificates)"
echo "NATIVE_AUTHORITATIVE: Unsat requires verified RUP cert (lrat::check_proof); fragment gate required; default=ON (opt-out ANUBIS_NATIVE_AUTHORITATIVE=0)"

command -v z3 >/dev/null || { echo "FATAL: z3 not on PATH — the equivalence half needs it"; exit 1; }
command -v timeout >/dev/null || { echo "FATAL: coreutils timeout missing"; exit 1; }

INVENTORY_ERR="$(mktemp)"
if ! files="$(python3 scripts/lib/native_corpus_inventory.py 2>"$INVENTORY_ERR")"; then
  cat "$INVENTORY_ERR"
  rm -f "$INVENTORY_ERR"
  echo "NATIVE_AUTHORITATIVE_GATE: FAIL (native corpus is not a stable source-manifest-bound set)"
  exit 1
fi
rm -f "$INVENTORY_ERR"
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
# Hollow PASS guard (Seshat R8): zero files compared is not equivalence.
if ! require_nonempty_corpus "$n" "examples|tests/fixtures/**/*.anb"; then
  echo "NATIVE_AUTHORITATIVE_GATE: FAIL (zero files compared - hollow PASS forbidden)"
  exit 1
fi

# ---- TCB-drop demo: z3 hidden from PATH ----
PASS_FIX=tests/fixtures/native_authoritative/int_contract_proves.anb
FAIL_FIX=tests/fixtures/native_authoritative/int_contract_violates.anb
# Phase-7 fragment gate: a TRUE property built from an UNPROVEN op (bvashr) must FAIL CLOSED z3-free.
DANGER_FIX=tests/fixtures/native_authoritative/int_contract_danger_defers.anb
demo_fail=0

set +e
# Default path is native-authoritative (no env required).
PATH=/nonexistent "$BIN" check "$PASS_FIX" >/dev/null 2>&1
demo_rc=$?
[ "$demo_rc" -eq 0 ] || { echo "DEMO FAIL: default native-authoritative could not prove $PASS_FIX without z3"; demo_fail=1; }
PATH=/nonexistent "$BIN" check "$FAIL_FIX" >/dev/null 2>&1
demo_rc=$?
[ "$demo_rc" -ne 0 ] || { echo "DEMO FAIL: default native-authoritative ACCEPTED the violating $FAIL_FIX without z3"; demo_fail=1; }
# Explicit =1 must match default.
PATH=/nonexistent ANUBIS_NATIVE_AUTHORITATIVE=1 "$BIN" check "$PASS_FIX" >/dev/null 2>&1
demo_rc=$?
[ "$demo_rc" -eq 0 ] || { echo "DEMO FAIL: ANUBIS_NATIVE_AUTHORITATIVE=1 could not prove $PASS_FIX without z3"; demo_fail=1; }
# Control: opt-out restores z3 dependence — z3-less check of the green fixture must FAIL.
PATH=/nonexistent ANUBIS_NATIVE_AUTHORITATIVE=0 "$BIN" check "$PASS_FIX" >/dev/null 2>&1
demo_rc=$?
[ "$demo_rc" -ne 0 ] || { echo "DEMO FAIL: opt-out ANUBIS_NATIVE_AUTHORITATIVE=0 still passed WITHOUT z3 — opt-out broken"; demo_fail=1; }
# FRAGMENT-GATE SOUNDNESS: the danger fixture's property is TRUE (so it PASSES with z3), but its op is
# unproven, so z3-free + authoritative it must FAIL CLOSED (native declines, no z3 to defer to).
"$BIN" check "$DANGER_FIX" >/dev/null 2>&1
demo_rc=$?
[ "$demo_rc" -eq 0 ] || { echo "DEMO FAIL: danger fixture $DANGER_FIX should PASS with z3 present (property is true)"; demo_fail=1; }
PATH=/nonexistent "$BIN" check "$DANGER_FIX" >/dev/null 2>&1
demo_rc=$?
[ "$demo_rc" -ne 0 ] || { echo "DEMO FAIL: danger fixture $DANGER_FIX proved z3-free on an UNPROVEN blast — the fragment gate did not fire"; demo_fail=1; }
set -e

# ---- Drift check: the Rust allow-list (fragment.rs PROVEN_OP_TAGS) must be backed by live Lean proofs,
# and must NOT admit any deferred op. Ties the authoritative fragment to formal/Anubis/BitBlast.lean so
# an op cannot ride as authoritative without a green *_correct/value-lemma theorem. ----
drift_fail=0
LEAN=formal/Anubis/BitBlast.lean
FRAG=solver/src/fragment.rs
# Every admitted op's backing theorem/lemma must exist in BitBlast.lean.
for thm in rippleCarry_spec ult_correct slt_correct ule_correct sle_correct eqBits_correct \
           andBits_correct orBits_correct xorBits_correct subBits_correct negBits_correct iteBits_correct \
           mulConst_correct mulVar_correct shlConst_correct barrelShl_correct shrConstL_correct barrelLshr_correct \
           bitsToNat_not bitsToNat_append_list bitsToNat_extract bitsToNat_append_replicate_false; do
  grep -q "\b$thm\b" "$LEAN" || { echo "DRIFT: fragment admits an op but its backing '$thm' is MISSING from $LEAN"; drift_fail=1; }
done
# No DEFERRED op name may appear in PROVEN_OP_TAGS (guards accidental admission of unproven wiring).
# (Xor left this list 2026-07-20 when xorBits_correct landed — the drift gate itself caught the move;
#  Sub/Neg left the same day with subBits/negBits_correct.)
TAGS=$(sed -n '/PROVEN_OP_TAGS/,/];/p' "$FRAG")
for deferred in Ashr SignExtend Udiv Urem Sdiv Srem; do
  echo "$TAGS" | grep -qw "\"$deferred\"" && { echo "DRIFT: deferred op '$deferred' is listed in PROVEN_OP_TAGS (unproven wiring admitted)"; drift_fail=1; }
done

# Coverage ratchet on the verdict-equivalence corpus. "0 mismatches over $n files" is only as
# strong as $n, and nothing else notices if $n falls.
assert_floor "native_authoritative" "$n" "$ROOT/.gate_floors/native_authoritative.floor"
_floor_rc=$?
if [ $_floor_rc -ne 0 ]; then
  echo "NATIVE_AUTHORITATIVE_GATE: FAIL ($GATE_FLOOR_ERROR)" >&2
  exit 1
fi

if [ "$mismatches" = 0 ] && [ "$disagreements" = 0 ] && [ "$demo_fail" = 0 ] && [ "$drift_fail" = 0 ]; then
  echo "NATIVE_AUTHORITATIVE_GATE: PASS (cert suite + fragment/TCB-drop demo + verdict-equivalent on $n files, mismatches=0 disagreements=0; default native z3-hidden proves/rejects; opt-out=0 restores z3 dependence; danger op fails closed; allow-list ↔ Lean; DEFAULT=native-authoritative)"
  exit 0
fi
echo "NATIVE_AUTHORITATIVE_GATE: FAIL"
exit 1
