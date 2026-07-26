#!/usr/bin/env bash
# Phase-4 slice-1 effect-selfhost gate — the differential that pins the Anubis-authored effect engine
# against the Rust effect engine. It is to the semantic-engine port what run_native_shadow_gate.sh is
# to the native SMT solver: a permanent 0-disagreement invariant that must hold before the Anubis pass
# can ever be promoted to authoritative (the shadow → opt-in → default-flip arc).
#
# Two engines, one program each:
#   (A) Rust  : `anubis check <f>`  — compiler/src/middle/effects.rs compute_fn_effect_rows fixpoint
#               + the mod.rs ANUBIS_UNDECLARED_EFFECT enforcing check (default lane).
#   (B) Anubis: `anubis run selfhost/src/anubis_sh.anb -- effects <f>` — the
#               self-hosted eff_check port (eff_compute_rows + row ⊆ declared).
#
# PRIMARY ORACLE (0-disagreement invariant): the two independently-derived undeclared-effect
# (FUNCTION, cap) PAIR sets must be IDENTICAL. Extraction is ANCHORED to the phrase
# "function `F` uses effect `X`", which is UNIQUE to the ANUBIS_UNDECLARED_EFFECT diagnostic — it
# isolates the effect lane from the SEPARATE Safe-mode gate (ANUBIS_EFFECT_FORBIDDEN_IN_MODE, worded
# "... forbidden without `uses(shell)`", slice 3) and the taint lane, neither of which uses that
# phrase. Comparing PER-FUNCTION pairs (not just the union of caps) is deliberate: a coarse cap-set
# union masks per-function divergence (e.g. an engine that over-propagates a callee's name-collision
# cap to its caller shares the same union but flags a different function). Both engines get the
# IDENTICAL declared input, so equal pair-sets ⟺ identical per-function reject decisions.
#
# SECONDARY ORACLE: the Anubis engine is a PURE effect-lane pass, so its accept/reject (exit code)
# must match the fixture's `// EXPECT:` marker — pinning that the corpus actually exercises both
# verdicts and that the self-hosted verdict is correct per each fixture's intent.
#
# SCOPE (honest boundary): the SH parser has no Lambda/CallExpr variant, so the Rust
# higher_order_closure_args lambda-descent and the CallExpr→open arm are structurally unreachable in
# the SH subset — growing the parser + porting those arms is slice 2. This gate pins the SH subset.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
BIN=./target/release/anubis
SH=selfhost/src/anubis_sh.anb
CORPUS=tests/fixtures/effects_selfhost

cargo build -q --release -p anubis

# Extract the undeclared-effect (function, cap) PAIR set from a diagnostic stream — anchored to the
# unique "function `F` uses effect `X`" phrasing (isolates from the Safe-mode + taint lanes), emitted
# as sorted-unique `F:X` lines so per-function divergence cannot be masked by a cap-set union.
extract_caps() {
  { grep -oE 'function `[^`]+` uses effect `[^`]+`' || true; } \
    | sed -E 's/function `([^`]+)` uses effect `([^`]+)`/\1:\2/' | sort -u
}

agree=0; disagree=0; expect_ok=0; expect_bad=0; n=0
echo "== effect-selfhost differential (Anubis-authored effect engine  vs  Rust effect engine) =="
for f in "$CORPUS"/*.anb; do
  [ -e "$f" ] || continue
  n=$((n+1))
  name=$(basename "$f")
  exp=$( { grep -m1 '// EXPECT:' "$f" || true; } | sed 's|.*EXPECT: *||' | tr -d '[:space:]')

  rust_caps=$( { "$BIN" check "$f" 2>&1 || true; } | extract_caps | tr '\n' ',')

  set +e
  # `anubis_sh.anb` has no research{}/exploit{} blocks or @research/@exploit attrs
  # -> program_mode = Mode::Safe -> this `run` needs no --allow-research. Passing it
  # would now FAIL on host with ANUBIS_RESEARCH_HOST_FORBIDDEN (commit 5fb7b67 made
  # `run --allow-research` VZ-guest-only). See scripts/run_selfhost_gate.sh.
  anb_out=$("$BIN" run "$SH" -- effects "$f" 2>&1)
  anb_rc=$?
  set -e
  anb_caps=$(printf '%s\n' "$anb_out" | extract_caps | tr '\n' ',')

  if [ "$rust_caps" = "$anb_caps" ]; then
    agree=$((agree+1)); verdict="AGREE"
  else
    disagree=$((disagree+1)); verdict="*** DISAGREE ***"
  fi

  anb_verdict="PASS"; [ "$anb_rc" -ne 0 ] && anb_verdict="FAIL"
  if [ "$anb_verdict" = "$exp" ]; then
    expect_ok=$((expect_ok+1)); em="ok"
  else
    expect_bad=$((expect_bad+1)); em="EXPECT-MISMATCH($anb_verdict!=$exp)"
  fi

  printf "  %-42s exp=%-4s rust={%s} anb={%s}  %s  %s\n" "$name" "$exp" "$rust_caps" "$anb_caps" "$verdict" "$em"
done

echo ""
echo "EFFECT_SELFHOST over $n fixtures: AGREE=$agree DISAGREE=$disagree | EXPECT ok=$expect_ok mismatch=$expect_bad"

if [ "$disagree" -gt 0 ] || [ "$expect_bad" -gt 0 ]; then
  echo "EFFECT_SELFHOST_GATE: FAIL"
  exit 1
fi
echo "EFFECT_SELFHOST_GATE: PASS (0 disagreements; Anubis-authored effect engine == Rust effect engine)"
exit 0
