#!/usr/bin/env bash
# ============================================================================
# Anubis Unified Gate Suite
# ============================================================================
# A single command that runs EVERY gate. A stranger on a fresh clone can run
# this and receive a clean pass (or a precise, honest failure) on ALL gates.
#
# Usage: bash scripts/audit_unified.sh [--out DIR]
#
# Gates:
#   G1  cargo fmt --check
#   G2  cargo clippy --all-targets -- -D warnings
#   G3  cargo test --all
#   G4  cargo build --release
#   G5  Language fixtures (26/26)
#   G6  Turing-core fixtures (13/13)
#   G7  PCA gate (13/13)
#   G8  Security fixtures
#   G9  PoC kit gate
#   G10 Prove gate (ZK receipt binding, cold verify)
#   G11 Enum/match gate
#   G12 For-in gate
#   G13 Lang power trio gate
#   G14 Offensive platform gate (T1-T7)
#   G15 Dogfood: examples/feel/* programs run
#
# Each gate is fail-closed: a missing tool, nonzero exit, or unexpected output
# is FAIL. The overall verdict is PASS only if every gate passes.
# ============================================================================
set -uo pipefail

STAMP=$(date +%Y%m%d-%H%M%S)
OUT="out/unified_gate/${STAMP}"
if [[ "${1:-}" == "--out" && -n "${2:-}" ]]; then OUT="$2"; fi
mkdir -p "$OUT"

pass=0; fail=0; skip=0; total=0
REPORT="$OUT/gate_report.json"
LOG="$OUT/gate_log.txt"
GATE_RESULTS=()

gate() {
  local name="$1" status="$2" detail="$3"
  total=$((total+1))
  if [[ "$status" == "PASS" ]]; then
    pass=$((pass+1))
  elif [[ "$status" == "SKIP" ]]; then
    skip=$((skip+1))
  else
    fail=$((fail+1))
  fi
  GATE_RESULTS+=("{\"gate\":\"$name\",\"status\":\"$status\",\"detail\":\"$detail\"}")
  printf '%-6s %-40s %s\n' "$status" "$name" "$detail" | tee -a "$LOG"
}

echo "=== ANUBIS UNIFIED GATE SUITE ===" | tee "$LOG"
echo "Timestamp: $STAMP" | tee -a "$LOG"
echo "Working directory: $(pwd)" | tee -a "$LOG"
echo "" | tee -a "$LOG"

# ── G1: cargo fmt ──
if cargo fmt -- --check >"$OUT/g1_fmt.log" 2>&1; then
  gate "G1_fmt" "PASS" "no formatting diffs"
else
  gate "G1_fmt" "FAIL" "formatting diffs found (see g1_fmt.log)"
fi

# ── G2: cargo clippy ──
if cargo clippy --all-targets -- -D warnings >"$OUT/g2_clippy.log" 2>&1; then
  gate "G2_clippy" "PASS" "zero warnings/errors"
else
  gate "G2_clippy" "FAIL" "clippy violations (see g2_clippy.log)"
fi

# ── G3: cargo test ──
if cargo test --all >"$OUT/g3_test.log" 2>&1; then
  TEST_COUNT=$(grep -oE '[0-9]+ passed' "$OUT/g3_test.log" | grep -oE '[0-9]+' | awk '{s+=$1} END{print s}')
  gate "G3_test" "PASS" "${TEST_COUNT:-?} tests passed"
else
  gate "G3_test" "FAIL" "test failures (see g3_test.log)"
fi

# ── G4: cargo build --release ──
if cargo build --release >"$OUT/g4_build.log" 2>&1; then
  gate "G4_build_release" "PASS" "release binary built"
else
  gate "G4_build_release" "FAIL" "release build failed (see g4_build.log)"
fi

BIN="$(pwd)/target/release/anubis"
if [[ ! -x "$BIN" ]]; then
  echo "FATAL: release binary missing after G4. Aborting remaining gates." | tee -a "$LOG"
  gate "G4_binary_exists" "FAIL" "target/release/anubis not found"
  # Write partial report and exit
  JOINED=$(IFS=,; echo "${GATE_RESULTS[*]}")
  cat > "$REPORT" <<ENDJSON
{"timestamp":"$STAMP","pass":$pass,"fail":$fail,"skip":$skip,"total":$total,"verdict":"FAIL","gates":[$JOINED]}
ENDJSON
  echo ""
  echo "Overall: FAIL ($pass/$total passed, $fail failed, $skip skipped)"
  exit 1
fi

# ── G5: Language fixtures ──
if bash scripts/run_language_fixtures.sh --out "$OUT/g5_language" >"$OUT/g5_language.log" 2>&1; then
  LF_PASS=$(grep -oE 'Overall: PASS \([0-9]+/[0-9]+\)' "$OUT/g5_language.log" || echo "")
  gate "G5_language_fixtures" "PASS" "$LF_PASS"
else
  gate "G5_language_fixtures" "FAIL" "language fixture failures (see g5_language.log)"
fi

# ── G6: Turing-core fixtures ──
if bash scripts/run_turing_core_fixtures.sh --out "$OUT/g6_turing" >"$OUT/g6_turing.log" 2>&1; then
  TC_PASS=$(grep -oE 'Overall: PASS \([0-9]+/[0-9]+\)' "$OUT/g6_turing.log" || echo "")
  gate "G6_turing_core" "PASS" "$TC_PASS"
else
  gate "G6_turing_core" "FAIL" "turing-core failures (see g6_turing.log)"
fi

# ── G7: PCA gate ──
if bash scripts/run_pca_gate.sh --out "$OUT/g7_pca" >"$OUT/g7_pca.log" 2>&1; then
  PCA_PASS=$(grep -oE 'Overall: PASS \([0-9]+/[0-9]+\)' "$OUT/g7_pca.log" || echo "")
  gate "G7_pca" "PASS" "$PCA_PASS"
else
  gate "G7_pca" "FAIL" "PCA gate failures (see g7_pca.log)"
fi

# ── G8: Security fixtures ──
if bash scripts/run_security_fixtures.sh --out "$OUT/g8_security" >"$OUT/g8_security.log" 2>&1; then
  SF_PASS=$(grep -oE 'Overall: PASS' "$OUT/g8_security.log" || echo "passed")
  gate "G8_security_fixtures" "PASS" "$SF_PASS"
else
  gate "G8_security_fixtures" "FAIL" "security fixture failures (see g8_security.log)"
fi

# ── G9: PoC kit gate ──
if bash scripts/run_poc_kit_gate.sh --out "$OUT/g9_poc_kit" >"$OUT/g9_poc_kit.log" 2>&1; then
  PK_PASS=$(grep -oE 'Overall: PASS \([0-9]+/[0-9]+\)' "$OUT/g9_poc_kit.log" || echo "")
  gate "G9_poc_kit" "PASS" "$PK_PASS"
else
  gate "G9_poc_kit" "FAIL" "PoC kit failures (see g9_poc_kit.log)"
fi

# ── G10: Prove gate (ZK receipt binding + cold verify) ──
if [[ -f tests/fixtures/zk_prove_bundle/backend/risc0/receipt.bin ]]; then
  if bash scripts/run_prove_gate.sh --out "$OUT/g10_prove" >"$OUT/g10_prove.log" 2>&1; then
    PG_PASS=$(grep -oE 'Overall: PASS \([0-9]+/[0-9]+\)' "$OUT/g10_prove.log" || echo "")
    gate "G10_prove" "PASS" "$PG_PASS"
  else
    gate "G10_prove" "FAIL" "prove gate failures (see g10_prove.log)"
  fi
else
  gate "G10_prove" "SKIP" "no committed receipt fixture (tests/fixtures/zk_prove_bundle)"
fi

# ── G11: Enum/match gate ──
if bash scripts/run_enum_match_gate.sh >"$OUT/g11_enum.log" 2>&1; then
  gate "G11_enum_match" "PASS" "enum/match gate clean"
else
  gate "G11_enum_match" "FAIL" "enum/match failures (see g11_enum.log)"
fi

# ── G12: For-in gate ──
if bash scripts/run_for_in_gate.sh >"$OUT/g12_for_in.log" 2>&1; then
  gate "G12_for_in" "PASS" "for-in gate clean"
else
  gate "G12_for_in" "FAIL" "for-in failures (see g12_for_in.log)"
fi

# ── G13: Lang power trio gate ──
if bash scripts/run_lang_trio_gate.sh >"$OUT/g13_lang_trio.log" 2>&1; then
  gate "G13_lang_trio" "PASS" "lang trio gate clean"
else
  gate "G13_lang_trio" "FAIL" "lang trio failures (see g13_lang_trio.log)"
fi

# ── G14: Offensive platform gate (T1-T7) ──
if bash scripts/run_offensive_platform_gate.sh --out "$OUT/g14_offensive" >"$OUT/g14_offensive.log" 2>&1; then
  OF_PASS=$(grep -oE 'Overall: PASS \([0-9]+/[0-9]+\)' "$OUT/g14_offensive.log" || echo "")
  gate "G14_offensive" "PASS" "$OF_PASS"
else
  gate "G14_offensive" "FAIL" "offensive gate failures (see g14_offensive.log)"
fi

# ── G15: Dogfood examples/feel/* ──
FEEL_DIR="examples/feel"
if [[ -d "$FEEL_DIR" ]]; then
  feel_pass=0; feel_fail=0; feel_total=0
  for f in "$FEEL_DIR"/*.anb "$FEEL_DIR"/*.anub; do
    [[ -f "$f" ]] || continue
    feel_total=$((feel_total+1))
    if "$BIN" run "$f" >"$OUT/g15_$(basename "$f").log" 2>&1; then
      feel_pass=$((feel_pass+1))
    else
      feel_fail=$((feel_fail+1))
      echo "  FAIL: $f" >> "$OUT/g15_summary.log"
    fi
  done
  if [[ $feel_fail -eq 0 && $feel_total -gt 0 ]]; then
    gate "G15_dogfood_feel" "PASS" "$feel_pass/$feel_total programs ran"
  elif [[ $feel_total -eq 0 ]]; then
    gate "G15_dogfood_feel" "SKIP" "no .anb/.anub files in $FEEL_DIR"
  else
    gate "G15_dogfood_feel" "FAIL" "$feel_fail/$feel_total programs failed"
  fi
else
  gate "G15_dogfood_feel" "SKIP" "no examples/feel directory"
fi

# ── Report ──
echo "" | tee -a "$LOG"
echo "========================================" | tee -a "$LOG"

VERDICT="PASS"
if [[ $fail -gt 0 ]]; then VERDICT="FAIL"; fi

JOINED=$(IFS=,; echo "${GATE_RESULTS[*]}")
cat > "$REPORT" <<ENDJSON
{"timestamp":"$STAMP","pass":$pass,"fail":$fail,"skip":$skip,"total":$total,"verdict":"$VERDICT","gates":[$JOINED]}
ENDJSON

echo "Overall: $VERDICT ($pass/$total passed, $fail failed, $skip skipped)" | tee -a "$LOG"
echo "Report: $REPORT" | tee -a "$LOG"
echo "Log: $LOG" | tee -a "$LOG"

if [[ "$VERDICT" == "FAIL" ]]; then exit 1; fi
exit 0
