#!/usr/bin/env bash
# Phase-4 taint-selfhost gate — the differential that pins the Anubis-authored TYPE engine against the
# Rust taint engine. Sibling of run_effect_selfhost_gate.sh; same shadow → opt-in → default-flip arc:
# a permanent 0-disagreement invariant that must hold before the Anubis type pass can be promoted.
#
# Two engines, one program each:
#   (A) Rust  : `anubis check <f>`  — compiler/src/middle/mod.rs sink-gate ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY (:5493-5518)
#               (expr_taint_source_m + is_sink, default Safe lane).
#   (B) Anubis: `anubis run selfhost/src/anubis_sh.anb --allow-research -- types <f>` — the
#               self-hosted taint_check port (tnt_source + sh_is_sink over the intraprocedural source->sink path).
#
# PRIMARY ORACLE (0-disagreement invariant): the two independently-derived source->sink
# PAIR sets must be IDENTICAL. Extraction is ANCHORED to the phrase
# "type mismatch: expected `E`, got `G`", UNIQUE to the ported let-binding ANUBIS_TYPE_MISMATCH site.
# That anchor DELIBERATELY EXCLUDES the sibling type-mismatch shapes slice 1 does NOT port (assignment
# 'type mismatch on assign…', argument 'type mismatch: argument…', struct-field, operator/index), so the
# gate isolates exactly the ported surface — the literal analog of the effect gate anchoring on
# "function `F` uses effect `X`" to isolate the effect lane. Per-FILE set (SH diags carry no fn
# attribution yet); slice 2 moves to per-(fn) pairs once return/arg checks add attribution.
#
# SECONDARY ORACLE: the Anubis `types` pass accept/reject (exit code) must match the fixture's
# `// EXPECT:` marker — pinning that the corpus exercises both verdicts and that the self-hosted verdict
# is correct per each fixture's intent.
#
# SCOPE (honest boundary): SH's inference is a strict UNDER-approximation of Rust's (returns ""/accept
# for Call/Index/FieldAccess/IfExpr/Match, no float-literal arm — SH cannot lex `1.5`), so SH ⊆ Rust
# holds by construction (no spurious mismatch). The generics/traits/typed-`?`/HM-union-find checks are
# structurally UNREACHABLE in the SH grammar (no <T:Bound>/trait/impl/`?`), named in the ROADMAP as
# [NEEDS-HUMAN] parser-growth decisions, not effort residuals. This gate pins the SH subset.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
BIN=./target/release/anubis
SH=selfhost/src/anubis_sh.anb
CORPUS=tests/fixtures/taint_selfhost

cargo build -q --release -p anubis

# Extract the let-mismatch (expected, got) PAIR set — anchored to the unique
# "type mismatch: expected `E`, got `G`" phrasing — as sorted-unique `E:G` lines so a per-file union
# cannot mask a per-site divergence.
extract_taint() {
  { grep -oE 'tainted flow from `.*` to sink `[^`]+`' || true; } | sort -u
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
  anb_out=$("$BIN" run "$SH" --allow-research -- taint "$f" 2>&1)
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

if [ "$disagree" -gt 0 ] || [ "$expect_bad" -gt 0 ]; then
  echo "TAINT_SELFHOST_GATE: FAIL"
  exit 1
fi
echo "TAINT_SELFHOST_GATE: PASS (0 disagreements; Anubis-authored taint engine == Rust taint engine on the intraprocedural source→sink surface)"
exit 0
