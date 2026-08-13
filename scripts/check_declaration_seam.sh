#!/usr/bin/env bash
# scripts/check_declaration_seam.sh
# Structural Class D seam check for Anubis middle-end declaration consultation.
#
# Exit 0 = HIGH register / place / enforce / summary-return surface present +
#          required witnesses on disk.
# Exit 1 = any HIGH consumer or witness is missing (fail closed).
#
# WARN lines do not fail: parity residuals still open (e.g. seed_taint_pattern D4,
# historical body_returns_secret LetPattern twin). They must appear as WARN when open so a
# skeptic can see them without treating them as green.
#
# Complements scripts/run_security_fixtures.sh — does not replace it.
# Does not compile or run anubis (no cargo lock).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
M=compiler/src/middle/mod.rs
fail=0

need() {
  # $1=label $2=rg pattern $3=file
  if ! rg -q "$2" "$3"; then
    echo "FAIL: missing $1  (/$2/ in $3)"
    fail=1
  else
    echo "ok: $1"
  fi
}

file_need() {
  if [ ! -f "$1" ]; then
    echo "FAIL: missing fixture $1"
    fail=1
  else
    echo "ok: fixture $(basename "$1")"
  fi
}

# $1=label $2=fn name $3=required substring in next 45 lines after fn
fn_body_has() {
  if rg -n "fn $2" -A45 "$M" | rg -q "$3"; then
    echo "ok: $1"
  else
    echo "FAIL: $1  (fn $2 body lacks /$3/)"
    fail=1
  fi
}

# Summary-return: from fn body_returns_X through ~320 lines must call
# seed_declared_pattern_binders with the given predicate name.
summary_return_has() {
  local label=$1 fn=$2 pred=$3
  if ! rg -n "fn $fn" -A320 "$M" | rg -q 'seed_declared_pattern_binders'; then
    echo "FAIL: $label missing seed_declared_pattern_binders"
    fail=1
    return
  fi
  if ! rg -n "fn $fn" -A320 "$M" | rg 'seed_declared_pattern_binders' -A6 | rg -q "$pred"; then
    echo "FAIL: $label seed_declared without $pred"
    fail=1
    return
  fi
  echo "ok: $label"
}

if [ ! -f "$M" ]; then
  echo "FAIL: $M not found (not at repo root?)"
  exit 1
fi

echo "=== 1. REGISTER (pass-1 maps) ==="
need "fn_ret_types"                      'fn_ret_types'                      "$M"
need "method_ret_types"                  'method_ret_types'                  "$M"
need "enum_payload_types"                'enum_payload_types'                "$M"
need "struct_fields insert"              'struct_fields\.insert'             "$M"
need "collect_declared_ret_qualified"    'fn collect_declared_ret_qualified' "$M"

echo "=== 2. PLACE CONSUME (R1 + D1–D3 + D5) ==="
need "place_struct_type"                 'fn place_struct_type'              "$M"
need "declared_field_type"               'fn declared_field_type'            "$M"
need "Call place arm"                    'Expr::Call \{ callee'              "$M"
need "method_ret consult"                'method_ret'                        "$M"
need "fn_alias"                          'fn_alias'                          "$M"
need "declared_field_type call sites"    'declared_field_type\('             "$M"

echo "=== 3. BINDER CONSUME — enforce + summary expr ==="
need "qualified_pattern_binders"         'fn qualified_pattern_binders'      "$M"
fn_body_has "seed_effect_pattern → D4"   seed_effect_pattern   'qualified_pattern_binders'
fn_body_has "seed_pattern → D4"          seed_pattern          'qualified_pattern_binders'

echo "=== 4. SUMMARY RETURN CONSUME (D4-S both lanes) ==="
need "seed_declared_pattern_binders"     'fn seed_declared_pattern_binders'  "$M"
need "collect_stmt_patterns"             'fn collect_stmt_patterns'          "$M"
summary_return_has "body_returns D4-S taint"  body_returns  'is_tainted'
summary_return_has "body_returns D4-S secret" body_returns  'is_secret'

echo "=== 5. PARITY PROBES (warn only — do not fail HIGH gate) ==="
if rg -n 'fn seed_pattern' -A40 "$M" | rg -q 'qualified_pattern_binders'; then
  echo "ok: seed_pattern consults declared payloads"
else
  echo "WARN: seed_pattern lacks qualified_pattern_binders (summary-expr parity residual)"
fi
if rg -n 'fn body_returns' -A220 "$M" | rg -q 'LetPattern'; then
  echo "ok: body_returns has LetPattern"
else
  echo "WARN: body_returns lacks LetPattern (H8 residual)"
fi

echo "=== 6. WITNESSES (direct + via-summary) ==="
file_need examples/security/declared_secret_return_print_rejects.anb
file_need examples/security/declared_secret_struct_field_print_rejects.anb
file_need examples/security/declared_secret_field_via_call_result_rejects.anb
file_need examples/security/declared_secret_field_via_method_result_rejects.anb
file_need examples/security/declared_secret_field_via_fn_alias_result_rejects.anb
file_need examples/security/declared_secret_enum_payload_rejects.anb
file_need examples/security/declared_secret_enum_struct_payload_rejects.anb
file_need examples/security/declared_public_enum_payload_accepts.anb
# Summary dimension — without these, enforce-only green is insufficient
file_need examples/security/declared_secret_enum_payload_via_summary_rejects.anb
file_need examples/security/declared_tainted_enum_payload_via_summary_rejects.anb

if [ "$fail" -ne 0 ]; then
  echo "DECLARATION_SEAM_CHECK: FAIL"
  exit 1
fi
echo "DECLARATION_SEAM_CHECK: PASS (HIGH register/place/enforce/summary-return surface + witnesses)"
exit 0
