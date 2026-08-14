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
# The default `full` profile executes all 29 gates and passes only when EVERY named gate in
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
#   G23 Carrier totality (a new Expr variant breaks the build until classified)
#   G24 Promise coherence (the headline promise inherits the open-issues framing)
#   G25 Formal kernel (the demo verifies under DEFAULT Safe settings, no wrap bypass)
#   G26 Proof correspondence (the AST->…->runtime evidence map and TCB list stay true)
#   G27 Phase-metrics ledger faults (race/path/mode/truthfulness/self-contamination controls)
#   G28 Corpus/pin inventory binding (tracked inventory and immutable-pin poison controls)
#   G29 Host-resource contract (VZ admission, runtime guard, teardown and sync controls)
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
AUDIT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$AUDIT_ROOT"

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
source "$AUDIT_ROOT/scripts/lib/gate_common.sh"
gate_configure_audit_profile_environment "$PROFILE" "$AUDIT_ROOT"
mkdir -p "$OUT"
{
  printf 'profile=%s\n' "$PROFILE"
  printf 'risc0_skip_build_kernels=%s\n' "${RISC0_SKIP_BUILD_KERNELS:-unset}"
  printf 'anubis_skip_risc0_metal=%s\n' "${ANUBIS_SKIP_RISC0_METAL:-unset}"
  printf 'r0_disable_metal=%s\n' "${R0_DISABLE_METAL:-unset}"
  printf 'anubis_risc0_metal_reference=%s\n' "${ANUBIS_RISC0_METAL_REFERENCE:-unset}"
} >"$OUT/profile_environment.txt"

# An audit of a DIRTY TREE grades a state that never existed as a commit.
#
# Demonstrated 2026-07-28: a full run reported `FAIL G1_fmt` while `cargo fmt --check` was clean
# both before and after, because another agent saved a file mid-run. The verdict described a tree
# that existed for about four seconds. Worse, the same race silently STRIPS the code signature
# from `target/release/anubis` when G4 rebuilds it, so a VZ gate run alongside an audit fails with
# a missing-entitlement error that has nothing to do with the gate.
#
# Nothing in a 29-gate suite is worth reporting if the thing under test changed while it ran. This
# refuses rather than producing a verdict about a moving target. Override is explicit and is
# RECORDED IN THE REPORT, so a dirty run can never be mistaken later for a clean one.
# `|| true` is LOAD-BEARING: a clean tree makes `grep -v` match nothing and exit 1, and under
# `set -euo pipefail` that kills the script HERE — at the dirty check — before the suite prints a
# single line. The wrapper then reported a grade for a run that never happened.
#
# This is the third time this session that a checking tool died on a non-zero exit meaning "found
# nothing" rather than "failed": once in `publish_pin --verify`, once in `fixture_preflight.sh`,
# now here. In a grading tool a non-zero exit is DATA. `set -e` cannot tell the difference, so
# every such pipeline has to say so explicitly.
DIRTY="$(git status --porcelain 2>/dev/null | grep -vE '^\?\?' | head -20 || true)"
AUDIT_TREE_STATE="clean"
if [[ -n "$DIRTY" ]]; then
  if [[ "${ANUBIS_AUDIT_ALLOW_DIRTY:-0}" == "1" ]]; then
    AUDIT_TREE_STATE="dirty-override"
    echo "ANUBIS_AUDIT_DIRTY_TREE_OVERRIDE: grading a tree with uncommitted changes:" >&2
    printf '%s\n' "$DIRTY" >&2
  else
    echo "ANUBIS_AUDIT_DIRTY_TREE: refusing to grade a tree with uncommitted changes." >&2
    echo "  A verdict over a moving tree describes a state that was never committed, and a" >&2
    echo "  concurrent write makes gates fail for reasons that have nothing to do with the gate." >&2
    printf '%s\n' "$DIRTY" >&2
    echo "  Commit or stash, or set ANUBIS_AUDIT_ALLOW_DIRTY=1 (recorded in the report)." >&2
    exit 2
  fi
fi

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
{"timestamp":"$STAMP","profile":"$PROFILE","tree_state":"$AUDIT_TREE_STATE","pass":$pass,"fail":$fail,"skip":$skip,"external":$external,"total":$total,"verdict":"FAIL","gates":[$JOINED]}
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
    echo "Run the default full profile in the approved operator-run disposable Tart/VZ lane."
  } >"$OUT/g9_poc_kit.log"
  gate "G9_poc_kit" "EXTERNAL" "requires the approved operator-run disposable Tart/VZ lane; not executed by hosted profile"
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

# G23 — Phase 2: adding an `Expr` variant BREAKS THE BUILD until its carrier class is stated.
#
# The blueprint's Phase-2 criterion that no ordinary test can express. The false-accept class was
# never a list of bugs but a shape space that kept growing, and nothing forced the next person
# adding a binder or container form to write its consumer. `carrier.rs` matches every variant with
# no wildcard arm; this gate PLANTS one and proves rustc refuses it, then restores the tree.
if bash scripts/run_carrier_totality_gate.sh >"$OUT/g23_carrier.log" 2>&1; then
  gate "G23_carrier_totality" "PASS" "an unclassified Expr variant fails to compile"
else
  gate "G23_carrier_totality" "FAIL" "carrier totality not enforced (see g23_carrier.log)"
fi

# G24 — Phase 6: the headline promise must INHERIT the open-issues framing.
#
# `CLAIMS.md` already says "green means no KNOWN defects — not no defects". The gap was that the
# promise, restated in other docs, did not inherit it: a reader who meets the promise in HANDOFF.md
# and never reaches CLAIMS.md leaves with a stronger claim than this repo can discharge. Registering
# it here is the point — it caught a real drift in HANDOFF.md the first time it ran, and an
# unregistered gate only catches things while someone remembers to run it. The RED guard is part of
# G24, not a one-time development transcript: a neutered detector must fail the audit even when its
# live scan can still print PASS over the narrowed surface.
if bash scripts/run_promise_coherence_gate.sh --self-test >"$OUT/g24_promise_selftest.log" 2>&1 \
  && bash scripts/run_promise_coherence_gate.sh >"$OUT/g24_promise.log" 2>&1; then
  gate "G24_promise_coherence" "PASS" "RED guard passed; live restatements scoped; exclusions disclosed"
else
  gate "G24_promise_coherence" "FAIL" "self-test or live scan failed (see g24_promise_selftest.log and g24_promise.log)"
fi

# G25 — the formal-kernel demo, under DEFAULT Safe verification.
#
# This gate EXISTED and was registered nowhere: not in audit_unified, not in the VM battery, not in
# the seal checklist. So the 19/19 board and the 24/24 audit both certified a tree in which the
# formal-kernel lane was RED and nobody saw it — the demo failed `anubis check` with
# ANUBIS_WRAP_RISK on a guarded negation. An unregistered gate is a gate that runs only when someone
# remembers, which is the same as not having one.
#
# The gate itself refuses to run under ANUBIS_WRAP_SAFETY=0, so it cannot go green by disabling the
# check it exists to exercise.
if bash scripts/run_formal_kernel_gate.sh >"$OUT/g25_formal_kernel.log" 2>&1; then
  gate "G25_formal_kernel" "PASS" "formal-kernel demo verifies under default Safe settings"
else
  gate "G25_formal_kernel" "FAIL" "formal-kernel lane red (see g25_formal_kernel.log)"
fi

# G26 — the source-to-proof correspondence map must stay TRUE.
#
# `docs/PROOF_CORRESPONDENCE.md` states which links in AST -> VC -> SMT -> parser -> CNF ->
# certificate -> runtime carry evidence and which are TCB. A stale TCB list is worse than none: it
# reads as an assurance while describing a repo that no longer exists. This checks every cited Lean
# theorem and every cited path still resolves, and that the TCB section is non-empty — "nothing is
# trusted" must never be reachable by deleting a list.
if bash scripts/run_proof_correspondence_gate.sh --self-test >"$OUT/g26_proof_correspondence_selftest.log" 2>&1 \
  && bash scripts/run_proof_correspondence_gate.sh >"$OUT/g26_correspondence.log" 2>&1; then
  gate "G26_proof_correspondence" "PASS" "every cited theorem/path resolves; TCB enumerated"
else
  gate "G26_proof_correspondence" "FAIL" "proof/TCB correspondence drift or falsification control failure (see g26_proof_correspondence*.log)"
fi

# G27: phase-metrics ledger fault suite. Exit zero alone is insufficient: require exactly one
# nonzero-work terminal summary. A valid line plus a second contradictory/malformed summary is FAIL,
# not a parseable subset from which the contradiction can be discarded.
g27_rc=0
bash scripts/test_phase_metrics_ledger.sh >"$OUT/g27_phase_metrics_ledger.log" 2>&1 \
  || g27_rc=$?
g27_summary_count="$(grep -Ec '^PHASE_METRICS_LEDGER_TESTS:' \
  "$OUT/g27_phase_metrics_ledger.log" || true)"
g27_valid_summary_count="$(grep -Ec \
  '^PHASE_METRICS_LEDGER_TESTS: [1-9][0-9]* passed, 0 failed$' \
  "$OUT/g27_phase_metrics_ledger.log" || true)"
if [[ "$g27_rc" -eq 0 && "$g27_summary_count" -eq 1 && "$g27_valid_summary_count" -eq 1 ]]; then
  gate "G27_phase_metrics_ledger" "PASS" "fault suite reported nonzero tests and zero failures"
else
  gate "G27_phase_metrics_ledger" "FAIL" "fault suite failed or emitted missing/duplicate/malformed summary (see g27_phase_metrics_ledger.log)"
fi

# G28: native-corpus inventory and source-pin binding poison suite. This catches the split where
# native-authoritative grades untracked/on-disk examples that docs and the source pin do not bind.
g28_rc=0
bash scripts/test_corpus_inventory_binding.sh >"$OUT/g28_corpus_inventory_binding.log" 2>&1 \
  || g28_rc=$?
g28_summary_count="$(grep -Ec '^CORPUS_INVENTORY_BINDING:' \
  "$OUT/g28_corpus_inventory_binding.log" || true)"
g28_valid_summary_count="$(grep -Ec \
  '^CORPUS_INVENTORY_BINDING: [1-9][0-9]* passed, 0 failed$' \
  "$OUT/g28_corpus_inventory_binding.log" || true)"
if [[ "$g28_rc" -eq 0 && "$g28_summary_count" -eq 1 && "$g28_valid_summary_count" -eq 1 ]]; then
  gate "G28_corpus_inventory_binding" "PASS" "corpus and vendored divergences are source-manifest/pin bound"
else
  gate "G28_corpus_inventory_binding" "FAIL" "fault suite failed or emitted missing/duplicate/malformed summary (see g28_corpus_inventory_binding.log)"
fi

# G29: VM admission, teardown, battery-completeness, and nested build-cap contract. Require one
# nonzero-work summary so an empty/neutered shell test cannot certify this trust surface.
g29_rc=0
bash scripts/test_host_resource_guard.sh >"$OUT/g29_host_resource_contract.log" 2>&1 \
  || g29_rc=$?
g29_summary_count="$(grep -Ec '^HOST_RESOURCE_GUARD_SELFTEST:' \
  "$OUT/g29_host_resource_contract.log" || true)"
g29_valid_summary_count="$(grep -Ec \
  '^HOST_RESOURCE_GUARD_SELFTEST: PASS \(pass=[1-9][0-9]* fail=0\)$' \
  "$OUT/g29_host_resource_contract.log" || true)"
if [[ "$g29_rc" -eq 0 && "$g29_summary_count" -eq 1 && "$g29_valid_summary_count" -eq 1 ]]; then
  gate "G29_host_resource_contract" "PASS" "guard, teardown, gate inventory, and job-cap poison suite passed"
else
  gate "G29_host_resource_contract" "FAIL" "fault suite failed or emitted missing/duplicate/malformed summary (see g29_host_resource_contract.log)"
fi

# G30: Completion Blueprint Phase 3 label-site census.
#
# The blueprint's Phase 3 statement is to separate the security-label lattice from accept-biased
# type inference. Before any migration, every direct read/write of `ScopeBinding.info.tainted /
# .taint_source / .declassified / .secret` in `compiler/src/middle/mod.rs` is enumerated by
# `scripts/lib/phase3_label_census.py` and diffed against the hand-classified
# `docs/phase3/label_census.tsv`. A newly-appeared writer/reader function, a change in the
# (writes, reads) shape of an existing bucket, or an unclassified row fails the gate.
#
# This is the "unclassified constructors fail the gate" clause from the mission. The gate is
# INTENDED to bind Slices 2-5: as label sites migrate to the explicit lattice, the classified
# expectation shrinks, and any regression that resurrects a legacy site fails closed.
# The gate first runs its RED self-test (regression suite that catches a word-boundary drop,
# first-match undercount, and missing --update bootstrap in the tool itself), then the live
# comparison. Mirrors the G24/G26 self-test-before-live pattern; a broken census that PASSes the
# live scan by construction is a real failure mode this guard exists to catch.
if bash scripts/run_phase3_label_census.sh --self-test >"$OUT/g30_phase3_label_census_selftest.log" 2>&1 \
  && bash scripts/run_phase3_label_census.sh >"$OUT/g30_phase3_label_census.log" 2>&1; then
  gate "G30_phase3_label_census" "PASS" "self-test passed; label-site census matches classified expectation"
else
  gate "G30_phase3_label_census" "FAIL" "self-test or live scan failed (see g30_phase3_label_census_selftest.log and g30_phase3_label_census.log)"
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
EXPECTED_GATES="G1_fmt G2_clippy G3_test G4_build_release G5_language_fixtures G6_turing_core G7_pca G8_security_fixtures G9_poc_kit G10_prove G11_enum_match G12_for_in G13_lang_trio G14_offensive G15_dogfood_feel G16_docs_drift G17_stdlib_failclosed G18_native_authoritative G19_walker_completeness G20_gate_common_adoption G21_formal G22_fixture_preflight G23_carrier_totality G24_promise_coherence G25_formal_kernel G26_proof_correspondence G27_phase_metrics_ledger G28_corpus_inventory_binding G29_host_resource_contract G30_phase3_label_census"

MISSING_GATES=""
for g in $EXPECTED_GATES; do
  case " ${GATE_NAMES[*]} " in *" $g "*) ;; *) MISSING_GATES="$MISSING_GATES $g" ;; esac
done
set -- $EXPECTED_GATES
EXPECTED_GATE_COUNT=$#
DUPLICATE_GATES=""
EXTRA_GATES=""
for i in "${!GATE_NAMES[@]}"; do
  g="${GATE_NAMES[$i]}"
  case " $EXPECTED_GATES " in
    *" $g "*) ;;
    *) EXTRA_GATES="$EXTRA_GATES $g" ;;
  esac
  for j in "${!GATE_NAMES[@]}"; do
    if [[ "$j" -lt "$i" && "${GATE_NAMES[$j]}" == "$g" ]]; then
      DUPLICATE_GATES="$DUPLICATE_GATES $g"
      break
    fi
  done
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
  # G21_formal is NO LONGER tolerated as external. It was, because CI had no Lean toolchain — so
  # the board read green while the 162 theorems went unchecked, which is the exact shape of claim
  # this repo exists to refuse. CI now provisions elan + the pinned Lean from `formal/lean-toolchain`
  # (.github/workflows/ci.yml), so an absent toolchain is a configuration failure to fix, not a
  # result to accept. G9_poc_kit stays external: it needs a built vuln binary, not a toolchain.
  hosted) ALLOWED_EXTERNAL="G9_poc_kit" ;;
  *)      ALLOWED_EXTERNAL="" ;;
esac
UNEXPECTED_EXTERNAL=""
MISSING_EXPECTED_EXTERNAL=""
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
for g in $ALLOWED_EXTERNAL; do
  found_expected_external=0
  for i in "${!GATE_NAMES[@]}"; do
    if [[ "${GATE_NAMES[$i]}" == "$g" \
       && "${GATE_RESULTS[$i]}" == *'"status":"EXTERNAL"'* ]]; then
      found_expected_external=1
    fi
  done
  if [[ "$found_expected_external" != "1" ]]; then
    MISSING_EXPECTED_EXTERNAL="$MISSING_EXPECTED_EXTERNAL $g"
  fi
done
set -- $ALLOWED_EXTERNAL
EXPECTED_EXTERNAL_COUNT=$#
EXPECTED_PASS_COUNT=$((EXPECTED_GATE_COUNT - EXPECTED_EXTERNAL_COUNT))

VERDICT="FAIL"
if [[ -n "$MISSING_GATES" ]]; then
  echo "ANUBIS_AUDIT_INCOMPLETE: gate(s) produced no result:$MISSING_GATES" | tee -a "$LOG"
elif [[ -n "$DUPLICATE_GATES" || -n "$EXTRA_GATES" || "$total" -ne "$EXPECTED_GATE_COUNT" ]]; then
  echo "ANUBIS_AUDIT_ROSTER_INVALID: duplicate=[$DUPLICATE_GATES ] extra=[$EXTRA_GATES ] observed=$total expected=$EXPECTED_GATE_COUNT" | tee -a "$LOG"
elif [[ -n "$UNEXPECTED_EXTERNAL" ]]; then
  echo "ANUBIS_AUDIT_EXTERNAL_NOT_ALLOWED: gate(s) went EXTERNAL outside the $PROFILE profile's allowance:$UNEXPECTED_EXTERNAL" | tee -a "$LOG"
elif [[ -n "$MISSING_EXPECTED_EXTERNAL" || "$external" -ne "$EXPECTED_EXTERNAL_COUNT" ]]; then
  echo "ANUBIS_AUDIT_EXTERNAL_REQUIRED: expected exact EXTERNAL gate(s) [$ALLOWED_EXTERNAL] missing=[$MISSING_EXPECTED_EXTERNAL ] observed=$external expected=$EXPECTED_EXTERNAL_COUNT" | tee -a "$LOG"
elif [[ $fail -eq 0 && $skip -eq 0 ]]; then
  # `external` is declared, counted and printed — never folded into PASS silently.
  if [[ "$PROFILE" == "full" && $external -eq 0 && $pass -eq $EXPECTED_PASS_COUNT ]]; then
    VERDICT="PASS"
  elif [[ "$PROFILE" == "hosted" && $pass -eq $EXPECTED_PASS_COUNT ]]; then
    VERDICT="HOSTED_PASS"
  fi
fi

JOINED=$(IFS=,; echo "${GATE_RESULTS[*]}")
cat > "$REPORT" <<ENDJSON
{"timestamp":"$STAMP","profile":"$PROFILE","tree_state":"$AUDIT_TREE_STATE","pass":$pass,"fail":$fail,"skip":$skip,"external":$external,"total":$total,"verdict":"$VERDICT","gates":[$JOINED]}
ENDJSON

echo "Overall: $VERDICT ($pass/$total passed, $fail failed, $skip skipped, $external external)" | tee -a "$LOG"
echo "Report: $REPORT" | tee -a "$LOG"
echo "Log: $LOG" | tee -a "$LOG"

if [[ "$VERDICT" == "FAIL" ]]; then exit 1; fi
exit 0
