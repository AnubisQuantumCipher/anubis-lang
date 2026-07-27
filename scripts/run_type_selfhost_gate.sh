#!/usr/bin/env bash
# Phase-4 type-selfhost gate — the differential that pins the Anubis-authored TYPE engine against the
# Rust type checker. Sibling of run_effect_selfhost_gate.sh; same shadow → opt-in → default-flip arc:
# a permanent 0-disagreement invariant that must hold before the Anubis type pass can be promoted.
#
# Two engines, one program each:
#   (A) Rust  : `anubis check <f>`  — compiler/src/middle/mod.rs let-binding ANUBIS_TYPE_MISMATCH
#               (infer_expr_type_scoped + ty::assignable, default lane).
#   (B) Anubis: `anubis run selfhost/src/anubis_sh.anb -- types <f>` — the
#               self-hosted sh_check port (ty_assignable + sh_infer_type over the let-annotation site).
#
# PRIMARY ORACLE (0-disagreement invariant): the two independently-derived type-mismatch message sets
# must be IDENTICAL. Extraction is ANCHORED to the THREE phrases the SH engine now ports:
#   (1) let-init      "type mismatch: expected `E`, got `G`"                       (slice 1)
#   (2) argument      "type mismatch: argument N of `F` expects `E`, got `G`"      (slice 2)
#   (3) return        "function declared `-> R` but returns a value of type `A`"   (slice 2)
# These anchors DELIBERATELY EXCLUDE the sibling type-mismatch shapes NOT ported (assignment
# 'type mismatch on assign…', struct-field, operator/index), so the gate isolates exactly the ported
# surface. Rust emits (2) from two walkers (mod.rs:5407 effect-walk + :12258 semantics-walk) with the
# identical message — `sort -u` collapses the duplicate — and (2)/(3) each have a Rust-ONLY
# `check_mismatch_scoped` fallback (:12270/:11727) for Call/Index/FieldAccess operands the flat SH
# inferer returns "" for; those are the INCOMPLETE residual covered by the whole-corpus SH ⊆ Rust sweep
# (subset-tolerant), and are deliberately kept out of THIS curated fixture corpus so the exact-equality
# invariant holds here. Per-FILE set (SH diags carry no fn attribution yet).
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
SH=selfhost/src/anubis_sh.anb
CORPUS=tests/fixtures/types_selfhost

if [[ -n "${ANUBIS_BIN:-}" ]]; then
  BIN="$ANUBIS_BIN"
  [[ -x "$BIN" ]] || { echo "TYPE_SELFHOST_GATE: FAIL (ANUBIS_BIN=$BIN not executable)"; exit 127; }
else
  BIN=./target/release/anubis
  cargo build -q --release -p anubis
  [[ -x "$BIN" ]] || { echo "TYPE_SELFHOST_GATE: FAIL (no binary at $BIN)"; exit 127; }
fi

# Extract the ported type-mismatch message set — anchored to the three ported phrasings (let-init,
# argument, return) as sorted-unique whole messages so a per-file union cannot mask a per-site divergence
# and a duplicate emission from two Rust walkers collapses.
extract_types() {
  { grep -oE 'type mismatch: expected `[^`]+`, got `[^`]+`|type mismatch: argument [0-9]+ of `[^`]+` expects `[^`]+`, got `[^`]+`|function declared `-> [^`]+` but returns a value of type `[^`]+`' || true; } \
    | sort -u
}

agree=0; disagree=0; expect_ok=0; expect_bad=0; n=0
echo "== type-selfhost differential (Anubis-authored type engine  vs  Rust type checker) =="
for f in "$CORPUS"/*.anb; do
  [ -e "$f" ] || continue
  n=$((n+1))
  name=$(basename "$f")
  exp=$( { grep -m1 '// EXPECT:' "$f" || true; } | sed 's|.*EXPECT: *||' | tr -d '[:space:]')

  rust_ty=$( { "$BIN" check "$f" 2>&1 || true; } | extract_types | tr '\n' ',')

  set +e
  # `anubis_sh.anb` has no research{}/exploit{} blocks or @research/@exploit attrs
  # -> program_mode = Mode::Safe -> this `run` needs no --allow-research. Passing it
  # would now FAIL on host with ANUBIS_RESEARCH_HOST_FORBIDDEN (commit 5fb7b67 made
  # `run --allow-research` VZ-guest-only). See scripts/run_selfhost_gate.sh.
  anb_out=$("$BIN" run "$SH" -- types "$f" 2>&1)
  anb_rc=$?
  set -e
  anb_ty=$(printf '%s\n' "$anb_out" | extract_types | tr '\n' ',')

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
echo "TYPE_SELFHOST over $n fixtures: AGREE=$agree DISAGREE=$disagree | EXPECT ok=$expect_ok mismatch=$expect_bad"

if [ "$n" -eq 0 ]; then
  echo "TYPE_SELFHOST_GATE: FAIL (empty/missing corpus — hollow PASS forbidden)"
  exit 1
fi
if [ "$disagree" -gt 0 ] || [ "$expect_bad" -gt 0 ]; then
  echo "TYPE_SELFHOST_GATE: FAIL"
  exit 1
fi
echo "TYPE_SELFHOST_GATE: PASS (0 disagreements; Anubis-authored type engine == Rust type checker on the let/argument/return-position mismatch surface)"
exit 0
