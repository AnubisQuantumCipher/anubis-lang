#!/usr/bin/env bash
# Phase-4 taint-selfhost gate — the differential that pins the Anubis-authored TAINT engine against the
# Rust taint engine. Sibling of run_effect_selfhost_gate.sh; a permanent 0-disagreement invariant.
#
# Two engines, one program each:
#   (A) Rust  : `anubis check <f>`  — compiler/src/middle/mod.rs sink-gate ANUBIS_TAINTED_SINK_WITHOUT_
#               DECLASSIFY (:5493) + the INTERPROC param→sink consult ANUBIS_INTERPROC_SINK (:5746/:5770),
#               default Safe lane.
#   (B) Anubis: `anubis run selfhost/src/anubis_sh.anb -- taint <f>` — the self-hosted
#               taint_check port: intraprocedural source→sink (tnt_source + sh_is_sink, slice 1) PLUS the
#               interprocedural param→sink + return-taint summary fixpoint + call-site consult (slice 2).
#
# PRIMARY ORACLE (0-disagreement invariant): the two independently-derived diagnostic-message sets must
# be IDENTICAL. Extraction (extract_taint) anchors on the TWO ported phrases: the intraproc
# "tainted flow from `S` to sink `Y`" AND the interproc "tainted flow from `S` into parameter N of `F`,
# which reaches a sink without declassify". Per-FILE set.
#
# SECONDARY ORACLE: the Anubis `taint` pass accept/reject (exit code) must match the fixture's
# `// EXPECT:` marker.
#
# SCOPE (honest boundary): the SH taint engine is a strict UNDER-approximation of Rust's — a general
# (non-io-source) call, composite (List/Map/EnumInit), closure, and the closure/container/method
# INTERPROC granularity all yield "" (deferred) — so SH ⊆ Rust holds by construction (no spurious). The
# interproc summary tracks param-flow through Var/Binary/Unary/Index/FieldAccess + simple lets/assigns +
# transitive direct calls (a fixpoint); closure/container interproc is the tolerated INCOMPLETE residual.
# This gate pins the SH subset (exact on the curated fixtures; whole-corpus SH ⊆ Rust is a separate sweep).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
SH=selfhost/src/anubis_sh.anb
CORPUS=tests/fixtures/taint_selfhost

if [[ -n "${ANUBIS_BIN:-}" ]]; then
  BIN="$ANUBIS_BIN"
  [[ -x "$BIN" ]] || { echo "TAINT_SELFHOST_GATE: FAIL (ANUBIS_BIN=$BIN not executable)"; exit 127; }
else
  BIN=./target/release/anubis
  cargo build -q --release -p anubis
  [[ -x "$BIN" ]] || { echo "TAINT_SELFHOST_GATE: FAIL (no binary at $BIN)"; exit 127; }
fi

# Extract the let-mismatch (expected, got) PAIR set — anchored to the unique
# "type mismatch: expected `E`, got `G`" phrasing — as sorted-unique `E:G` lines so a per-file union
# cannot mask a per-site divergence.
extract_taint() {
  # Two ported phrases: intraprocedural (source -> sink in one fn) AND interprocedural (source -> a
  # callee param that reaches a sink; ANUBIS_INTERPROC_SINK, mod.rs:5770). Greedy `.*` for the source
  # label so a nested-backtick source (`io source `input``) is captured whole.
  { grep -oE 'tainted flow from `.*` to sink `[^`]+`|tainted flow from `.*` into parameter [0-9]+ of `[^`]+`, which reaches a sink without declassify' || true; } | sort -u
}

agree=0; disagree=0; expect_ok=0; expect_bad=0; n=0
echo "== taint-selfhost differential (Anubis-authored taint engine  vs  Rust taint engine) =="
for f in "$CORPUS"/*.anb; do
  [ -e "$f" ] || continue
  n=$((n+1))
  name=$(basename "$f")
  exp=$( { grep -m1 '// EXPECT:' "$f" || true; } | sed 's|.*EXPECT: *||' | tr -d '[:space:]')

  rust_ty=$( { "$BIN" check "$f" 2>&1 || true; } | extract_taint | tr '\n' ',')

  set +e
  # `anubis_sh.anb` has no research{}/exploit{} blocks or @research/@exploit attrs
  # -> program_mode = Mode::Safe -> this `run` needs no --allow-research. Passing it
  # would now FAIL on host with ANUBIS_RESEARCH_HOST_FORBIDDEN (commit 5fb7b67 made
  # `run --allow-research` VZ-guest-only). See scripts/run_selfhost_gate.sh.
  anb_out=$("$BIN" run "$SH" -- taint "$f" 2>&1)
  anb_rc=$?
  set -e
  anb_ty=$(printf '%s\n' "$anb_out" | extract_taint | tr '\n' ',')

  if [ "$rust_ty" = "$anb_ty" ]; then
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

  printf "  %-46s exp=%-4s rust={%s} anb={%s}  %s  %s\n" "$name" "$exp" "$rust_ty" "$anb_ty" "$verdict" "$em"
done

echo ""
echo "TAINT_SELFHOST over $n fixtures: AGREE=$agree DISAGREE=$disagree | EXPECT ok=$expect_ok mismatch=$expect_bad"

if [ "$n" -eq 0 ]; then
  echo "TAINT_SELFHOST_GATE: FAIL (empty/missing corpus — hollow PASS forbidden)"
  exit 1
fi
if [ "$disagree" -gt 0 ] || [ "$expect_bad" -gt 0 ]; then
  echo "TAINT_SELFHOST_GATE: FAIL"
  exit 1
fi
echo "TAINT_SELFHOST_GATE: PASS (0 disagreements; Anubis-authored taint engine == Rust taint engine on the intra- + interprocedural source→sink surface)"
exit 0
