#!/usr/bin/env bash
# ============================================================================
# Anubis Unified Gate Suite
# ============================================================================
# A single command that runs EVERY gate. A stranger on a fresh clone can run
# this and receive a clean pass (or a precise, honest failure) on ALL gates.
#
# Usage:
#   bash scripts/audit_unified.sh [--out DIR]
#   bash scripts/audit_unified.sh --profile hosted [--out DIR]
#
# The default `full` profile executes all 22 gates and passes only when EVERY named gate in
# EXPECTED_GATES reported, with zero failures, skips, or external gates. The `hosted` profile exists for
# stock GitHub macOS runners, which cannot provide nested Apple virtualization
# or a Tart golden image. It runs every host-verifiable gate, marks G9
# `EXTERNAL`, pins G14 to its non-executing host isolation witness, and emits
# `HOSTED_PASS` rather than pretending to be a full seal.
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
#   G16 Docs drift (published stamps match a re-measurement)
#   G17 Stdlib fail-closed
#   G18 Native-authoritative fragment bound
#   G19 Walker completeness (no `..` discards a field code can hide in)
#   G20 gate_common adoption
#   G21 Formal (Lean; EXTERNAL without elan/lake)
#   G22 Fixture preflight self-test (an ACCEPT that cannot reject is not a finding)
#
# G16-G22 publish numbers the board cites and were, for most of their life, never run by CI.
# They are listed here so the gap between "a gate exists" and "a gate runs" stays visible.
#
# Each gate is fail-closed: a missing tool, nonzero exit, or unexpected output
# is FAIL. The overall verdict is PASS only if every gate passes.
# ============================================================================
# set -e: a bare cargo/tool crash must not fall through to a green Overall (Seshat R8).
# Child gates are already wrapped in `if bash …`; unguarded failures abort before verdict.
set -euo pipefail

STAMP=$(date +%Y%m%d-%H%M%S)
OUT="out/unified_gate/${STAMP}"
PROFILE="full"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --out)
      if [[ -z "${2:-}" ]]; then
        echo "ANUBIS_AUDIT_ARGUMENT: --out requires a directory" >&2
        exit 2
      fi
      OUT="$2"
      shift 2
      ;;
    --profile)
      if [[ -z "${2:-}" ]]; then
        echo "ANUBIS_AUDIT_ARGUMENT: --profile requires full or hosted" >&2
        exit 2
      fi
      PROFILE="$2"
      shift 2
      ;;
    *)
      echo "ANUBIS_AUDIT_ARGUMENT: unknown argument '$1'" >&2
      exit 2
      ;;
  esac
done
if [[ "$PROFILE" != "full" && "$PROFILE" != "hosted" ]]; then
  echo "ANUBIS_AUDIT_ARGUMENT: unsupported profile '$PROFILE' (expected full or hosted)" >&2
  exit 2
fi
mkdir -p "$OUT"

pass=0; fail=0; skip=0; external=0; total=0
REPORT="$OUT/gate_report.json"
LOG="$OUT/gate_log.txt"
GATE_RESULTS=()
GATE_NAMES=()

gate() {
  local name="$1" status="$2" detail="$3"
  total=$((total+1))
  if [[ "$status" == "PASS" ]]; then
    pass=$((pass+1))
  elif [[ "$status" == "SKIP" ]]; then
    skip=$((skip+1))
  elif [[ "$status" == "EXTERNAL" ]]; then
    external=$((external+1))
  else
    fail=$((fail+1))
  fi
  GATE_RESULTS+=("{\"gate\":\"$name\",\"status\":\"$status\",\"detail\":\"$detail\"}")
  GATE_NAMES+=("$name")
  printf '%-6s %-40s %s\n' "$status" "$name" "$detail" | tee -a "$LOG"
}

echo "=== ANUBIS UNIFIED GATE SUITE ===" | tee "$LOG"
echo "Timestamp: $STAMP" | tee -a "$LOG"
echo "Profile: $PROFILE" | tee -a "$LOG"
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
# Large clap CLI (AOP T1–T9) needs a bigger thread stack for Cli::try_parse unit tests.
export RUST_MIN_STACK="${RUST_MIN_STACK:-16777216}"
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
# EXPORT it, so every downstream gate grades the binary G4 just built.
#
# Without this, each gate re-derived its own instrument: the security gate found release on disk,
# the language gate defaulted to `cargo run --` (debug), and CI published the two resulting numbers
# side by side as if they described one build. They described two. A unified audit that builds a
# binary and then lets its gates pick their own is not unified — it is two audits sharing a report.
export ANUBIS_BIN="$BIN"
if [[ ! -x "$BIN" ]]; then
  echo "FATAL: release binary missing after G4. Aborting remaining gates." | tee -a "$LOG"
  gate "G4_binary_exists" "FAIL" "target/release/anubis not found"
  # Write partial report and exit
  JOINED=$(IFS=,; echo "${GATE_RESULTS[*]}")
  cat > "$REPORT" <<ENDJSON
{"timestamp":"$STAMP","profile":"$PROFILE","pass":$pass,"fail":$fail,"skip":$skip,"external":$external,"total":$total,"verdict":"FAIL","gates":[$JOINED]}
ENDJSON
  echo ""
  echo "Overall: FAIL ($pass/$total passed, $fail failed, $skip skipped, $external external)"
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
if [[ "$PROFILE" == "hosted" ]]; then
  {
    echo "ANUBIS_POC_KIT_EXTERNAL_VZ_REQUIRED"
    echo "Stock hosted macOS runners cannot supply nested Apple virtualization,"
    echo "the canonical anubis-xcode Tart image, or the operator SSH key."
    echo "Run the default full profile on the dedicated Tart/VZ runner."
  } >"$OUT/g9_poc_kit.log"
  gate "G9_poc_kit" "EXTERNAL" "requires dedicated Tart/VZ runner; not executed by hosted profile"
else
  if bash scripts/run_poc_kit_gate.sh --out "$OUT/g9_poc_kit" >"$OUT/g9_poc_kit.log" 2>&1; then
    PK_PASS=$(grep -oE 'Overall: PASS \([0-9]+/[0-9]+\)' "$OUT/g9_poc_kit.log" || echo "")
    gate "G9_poc_kit" "PASS" "$PK_PASS"
  else
    gate "G9_poc_kit" "FAIL" "PoC kit failures (see g9_poc_kit.log)"
  fi
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
# Always pass a private --out so concurrent audit runs do not race fixed out/enum_match.
if bash scripts/run_enum_match_gate.sh "$OUT/g11_enum" >"$OUT/g11_enum.log" 2>&1; then
  gate "G11_enum_match" "PASS" "enum/match gate clean"
else
  gate "G11_enum_match" "FAIL" "enum/match failures (see g11_enum.log)"
fi

# ── G12: For-in gate ──
if bash scripts/run_for_in_gate.sh "$OUT/g12_for_in" >"$OUT/g12_for_in.log" 2>&1; then
  gate "G12_for_in" "PASS" "for-in gate clean"
else
  gate "G12_for_in" "FAIL" "for-in failures (see g12_for_in.log)"
fi

# ── G13: Lang power trio gate ──
if bash scripts/run_lang_trio_gate.sh "$OUT/g13_lang_trio" >"$OUT/g13_lang_trio.log" 2>&1; then
  gate "G13_lang_trio" "PASS" "lang trio gate clean"
else
  gate "G13_lang_trio" "FAIL" "lang trio failures (see g13_lang_trio.log)"
fi

# ── G14: Offensive platform gate (T1-T7) ──
# Hosted: explicit 5/5 host-isolation-witness only.
# Full: disposable guest battery only — never inherit/force the 5/5 witness.
# shellcheck source=lib/gate_evidence.sh
# shellcheck disable=SC1091
GATE_EVIDENCE_ROOT="$(pwd)"
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/gate_evidence.sh"
G14_OUT="$OUT/g14_offensive"
G14_LOG="$OUT/g14_offensive.log"
G14_REPORT="$G14_OUT/report.json"
if [[ "$PROFILE" == "hosted" ]]; then
  if (
      export ANUBIS_OFFENSIVE_FORCE_ISOLATION_WITNESS=1
      bash scripts/run_offensive_platform_gate.sh --out "$G14_OUT"
    ) >"$G14_LOG" 2>&1 \
    && gate_validate_offensive_report "$G14_REPORT" "host-isolation-witness" 5 >>"$G14_LOG" 2>&1; then
    OF_PASS=$(grep -oE 'Overall: PASS \([0-9]+/[0-9]+\)' "$G14_LOG" || echo "Overall: PASS (5/5)")
    gate "G14_offensive" "PASS" "$OF_PASS host-isolation-witness exactly 5/5"
  else
    gate "G14_offensive" "FAIL" "hosted witness requires isolation=host-isolation-witness and exactly 5/5; full 34-check battery requires VZ (see g14_offensive.log)"
  fi
else
  # Strip ambient force-witness so full seal cannot soft-downgrade to 5/5.
  if (
      unset ANUBIS_OFFENSIVE_FORCE_ISOLATION_WITNESS || true
      bash scripts/run_offensive_platform_gate.sh --out "$G14_OUT"
    ) >"$G14_LOG" 2>&1 \
    && gate_validate_offensive_report "$G14_REPORT" "tart-disposable-guest" 34 >>"$G14_LOG" 2>&1; then
    OF_PASS=$(grep -oE 'Overall: PASS \([0-9]+/[0-9]+\)' "$G14_LOG" || echo "Overall: PASS (34/34)")
    gate "G14_offensive" "PASS" "$OF_PASS tart-disposable-guest exactly 34/34"
  else
    gate "G14_offensive" "FAIL" "full G14 requires isolation=tart-disposable-guest and exactly 34/34 (see g14_offensive.log)"
  fi
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

# ── G16-G21: gates whose numbers the board publishes but CI never ran ──
#
# Until 2026-07-28 none of these ran on push. The 162 Lean theorems, the 104/104 fail-closed stdlib
# matrix, the 882-file native-authoritative corpus, the docs-drift stamps and walker totality were
# ALL locally-run assertions: a broken proof, an introduced `sorry`, or a drifted stamp shipped
# green. A number that CI does not check is a number nobody is checking.

if bash scripts/run_docs_drift_gate.sh --out "$OUT/g16_docs_drift" >"$OUT/g16_docs_drift.log" 2>&1; then
  gate "G16_docs_drift" "PASS" "stamps re-derived, no drift"
else
  gate "G16_docs_drift" "FAIL" "documentation drifted from measured inventory (see g16_docs_drift.log)"
fi

if bash scripts/run_stdlib_failclosed_gate.sh --out "$OUT/g17_stdlib_fc" >"$OUT/g17_stdlib_fc.log" 2>&1; then
  gate "G17_stdlib_failclosed" "PASS" "stdlib fail-closed matrix green"
else
  gate "G17_stdlib_failclosed" "FAIL" "stdlib fail-closed regression (see g17_stdlib_fc.log)"
fi

if bash scripts/run_native_authoritative_gate.sh >"$OUT/g18_native_auth.log" 2>&1; then
  gate "G18_native_authoritative" "PASS" "native solver agrees with reference across the corpus"
else
  gate "G18_native_authoritative" "FAIL" "native/reference mismatch (see g18_native_auth.log)"
fi

if bash scripts/run_walker_completeness_gate.sh >"$OUT/g19_walker.log" 2>&1; then
  gate "G19_walker_completeness" "PASS" "registered walkers bind every code-holding field"
else
  gate "G19_walker_completeness" "FAIL" "a walker discards a field code can hide in (see g19_walker.log)"
fi

if bash scripts/check_gate_common_adoption.sh >"$OUT/g20_gate_common.log" 2>&1; then
  gate "G20_gate_common_adoption" "PASS" "no unknown fixture scorer bypasses the shared guards"
else
  gate "G20_gate_common_adoption" "FAIL" "a scorer neither uses gate_common nor has an exception (see g20_gate_common.log)"
fi

# Lean needs the elan/lake toolchain. Absent it, this is EXTERNAL — declared and counted, never a
# silent pass. `command -v` before running, so a missing toolchain cannot masquerade as a proof.
if command -v lake >/dev/null 2>&1 || [[ -x "$HOME/.elan/bin/lake" ]]; then
  [[ -x "$HOME/.elan/bin/lake" ]] && export PATH="$HOME/.elan/bin:$PATH"
  if bash scripts/run_formal_gate.sh >"$OUT/g21_formal.log" 2>&1; then
    gate "G21_formal" "PASS" "every theorem machine-checked; no sorry/admit/free axiom"
  else
    gate "G21_formal" "FAIL" "formal gate red (see g21_formal.log)"
  fi
else
  gate "G21_formal" "EXTERNAL" "lake/elan not installed on this runner; Lean proofs NOT checked here"
fi

# G22 — the harness that decides whether an ACCEPT can be a finding at all.
#
# Four instruments failed SILENTLY in a single session: a `$?` read after a command substitution
# that reported a working fix as inert, three stale-binary scorings, and a fixture preflight that
# aborted before printing its own verdict. Every one was caught by accident.
#
# `fixture_preflight.sh --self-test` plants the `w06b` fixture verbatim — a witness that declared
# the very capability it was written to catch, and was therefore carried as an open defect for four
# rounds — and proves the harness calls it MALFORMED rather than grading it. The gate exists
# because a preflight nobody has watched fail is a preflight taken on faith, which is the exact
# thing it was built to stop doing to fixtures.
if bash scripts/fixture_preflight.sh --self-test >"$OUT/g22_preflight.log" 2>&1; then
  gate "G22_fixture_preflight" "PASS" "preflight reaches a defined verdict and catches malformed witnesses"
else
  gate "G22_fixture_preflight" "FAIL" "fixture preflight self-test red (see g22_preflight.log)"
fi

# ── Report ──
echo "" | tee -a "$LOG"
echo "========================================" | tee -a "$LOG"

# Verdict by NAMED gate, not by arithmetic.
#
# This used to be `pass -eq 15 && ... && total -eq 15`. The exact-count check had the right
# instinct — a gate cannot silently vanish — but it made the gate list unextendable without
# editing magic numbers in two places, which is why six gates the board publishes were never
# added. Naming them keeps the property AND says which one is missing instead of just that the
# arithmetic no longer works.
EXPECTED_GATES="G1_fmt G2_clippy G3_test G4_build_release G5_language_fixtures G6_turing_core G7_pca G8_security_fixtures G9_poc_kit G10_prove G11_enum_match G12_for_in G13_lang_trio G14_offensive G15_dogfood_feel G16_docs_drift G17_stdlib_failclosed G18_native_authoritative G19_walker_completeness G20_gate_common_adoption G21_formal G22_fixture_preflight"

MISSING_GATES=""
for g in $EXPECTED_GATES; do
  case " ${GATE_NAMES[*]} " in *" $g "*) ;; *) MISSING_GATES="$MISSING_GATES $g" ;; esac
done

# Which gates are ALLOWED to be EXTERNAL, per profile — by NAME.
#
# The count-based form (`pass -eq 14 && external -eq 1`) was replaced by the named-gate check
# above, and in the swap the hosted profile quietly lost a real property: it pinned that EXACTLY
# ONE gate could be external, so a second gate degrading to EXTERNAL turned the seal red. Under a
# bare `external >= 0` any number of gates could go external and hosted would still say
# HOSTED_PASS — which is the "declared, never silently folded in" rule failing in the direction
# it exists to prevent.
#
# Naming them keeps the property AND says which gate broke the rule, which the arithmetic never
# could. `full` allows none: a full seal that cannot run a gate is not a full seal.
case "$PROFILE" in
  hosted) ALLOWED_EXTERNAL="G9_poc_kit G21_formal" ;;
  *)      ALLOWED_EXTERNAL="" ;;
esac
UNEXPECTED_EXTERNAL=""
for i in "${!GATE_NAMES[@]}"; do
  case "${GATE_RESULTS[$i]}" in
    *'"status":"EXTERNAL"'*)
      g="${GATE_NAMES[$i]}"
      case " $ALLOWED_EXTERNAL " in
        *" $g "*) ;;
        *) UNEXPECTED_EXTERNAL="$UNEXPECTED_EXTERNAL $g" ;;
      esac
      ;;
  esac
done

VERDICT="FAIL"
if [[ -n "$MISSING_GATES" ]]; then
  echo "ANUBIS_AUDIT_INCOMPLETE: gate(s) produced no result:$MISSING_GATES" | tee -a "$LOG"
elif [[ -n "$UNEXPECTED_EXTERNAL" ]]; then
  echo "ANUBIS_AUDIT_EXTERNAL_NOT_ALLOWED: gate(s) went EXTERNAL outside the $PROFILE profile's allowance:$UNEXPECTED_EXTERNAL" | tee -a "$LOG"
elif [[ $fail -eq 0 && $skip -eq 0 ]]; then
  # `external` is declared, counted and printed — never folded into PASS silently.
  if [[ "$PROFILE" == "full" && $external -eq 0 ]]; then
    VERDICT="PASS"
  elif [[ "$PROFILE" == "hosted" ]]; then
    VERDICT="HOSTED_PASS"
  fi
fi

JOINED=$(IFS=,; echo "${GATE_RESULTS[*]}")
cat > "$REPORT" <<ENDJSON
{"timestamp":"$STAMP","profile":"$PROFILE","pass":$pass,"fail":$fail,"skip":$skip,"external":$external,"total":$total,"verdict":"$VERDICT","gates":[$JOINED]}
ENDJSON

echo "Overall: $VERDICT ($pass/$total passed, $fail failed, $skip skipped, $external external)" | tee -a "$LOG"
echo "Report: $REPORT" | tee -a "$LOG"
echo "Log: $LOG" | tee -a "$LOG"

if [[ "$VERDICT" == "FAIL" ]]; then exit 1; fi
exit 0
