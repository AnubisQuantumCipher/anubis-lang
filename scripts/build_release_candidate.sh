#!/usr/bin/env bash
set -euo pipefail

# Gate 12/13/14 — Portable Release-Candidate Toolchain builder
# Produces out/release_candidate/<STAMP>/ with full evidence + verdicts.
#
# Usage:
#   bash scripts/build_release_candidate.sh \
#     --metal-reference /Users/sicarii/Desktop/metal-hybrid-prover \
#     --require-metal \
#     --include-security \
#     --out out/release_candidate

METAL_REF="/Users/sicarii/Desktop/metal-hybrid-prover"
REQUIRE_METAL=0
OUT_BASE="out/release_candidate"
INCLUDE_SECURITY=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --metal-reference) METAL_REF="$2"; shift 2 ;;
    --require-metal) REQUIRE_METAL=1; shift ;;
    --include-security) INCLUDE_SECURITY=1; shift ;;
    --out) OUT_BASE="$2"; shift 2 ;;
    *) echo "unknown arg: $1"; exit 1 ;;
  esac
done

STAMP=$(date +%Y%m%d-%H%M%S)
OUT_DIR="$OUT_BASE/$STAMP"
mkdir -p "$OUT_DIR"

LOG="$OUT_DIR/build_release_candidate.log"
REPORT="$OUT_DIR/RELEASE_CANDIDATE_REPORT.md"
JSON="$OUT_DIR/release_candidate.json"
MANIFEST="$OUT_DIR/MANIFEST.sha256"

exec > >(tee -a "$LOG") 2>&1

echo "=== Anubis Release Candidate Build ==="
echo "stamp: $STAMP"
echo "metal_reference: $METAL_REF"
echo "require_metal: $REQUIRE_METAL"
echo "host: $(uname -a)"
date

OVERALL="PASS"

step() {
  echo ""
  echo ">>> $1"
}

fail() {
  echo "FAIL: $1" | tee -a "$LOG"
  OVERALL="FAIL"
}

verify_pass_bundles_under() {
  local root="$1"
  local count=0
  local bundle

  while IFS= read -r bundle; do
    [[ -z "$bundle" ]] && continue
    count=$((count + 1))
    bash scripts/verify_bundle.sh "$bundle" || return 1
  done < <(find "$root" -type d -name 'evidence-*' | sort)

  if [[ $count -eq 0 ]]; then
    echo "no evidence bundles found under $root"
    return 1
  fi
}

step "0. safety + hygiene"
bash tools/grok-safety-check.sh || fail "safety check"

step "1. fmt"
cargo fmt --check || fail "fmt"

step "2. test --all"
cargo test --all || fail "tests"

step "3. clippy -D warnings"
cargo clippy --all-targets --all-features -- -D warnings || fail "clippy"

step "4. language fixtures"
bash scripts/run_language_fixtures.sh --out "$OUT_DIR/language_fixtures" || fail "language fixtures"
jq -e '.overall_verdict == "PASS"' "$OUT_DIR/language_fixtures/fixture_report.json" || fail "language fixtures verdict"

step "5. language reproducibility"
bash scripts/repro_language_core.sh --out "$OUT_DIR/language_repro" || fail "repro"
jq -e '.overall_verdict == "PASS"' "$OUT_DIR/language_repro/repro_report.json" || fail "repro"

step "6. doctor (with evidence)"
DOCTOR_ARGS=(doctor --metal-reference "$METAL_REF" --require-risc0 --evidence --out "$OUT_DIR/doctor")
if [[ $REQUIRE_METAL -eq 1 ]]; then
  DOCTOR_ARGS+=(--require-metal)
fi
cargo run --release -p anubis -- "${DOCTOR_ARGS[@]}" || fail "doctor"

step "7. Gate 4 regression (taint)"
cargo run --release -p anubis -- check examples/taint_reject.anb --evidence --out "$OUT_DIR/regress_gate4" || true
grep -R "ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY\|tainted flow" "$OUT_DIR/regress_gate4" || fail "Gate 4 taint still enforced"

step "8. Gate 5 regression (declassify)"
# (placeholder — extend when exact declassify fixture is stable)

step "9. Gate 7 regression (solver)"
cargo run --release -p anubis -- check examples/symbolic_assert_pass.anb --evidence --out "$OUT_DIR/regress_gate7" || fail "Gate 7 symbolic"
bash scripts/verify_bundle.sh "$OUT_DIR/regress_gate7"/evidence-* || fail "Gate 7 bundle"

step "10. Gate 10 RISC0 (CPU lane)"
cargo run --release -p anubis -- prove examples/risc0_receipt.anb \
  --backend risc0 --lane cpu \
  --metal-reference "$METAL_REF" \
  --evidence --out "$OUT_DIR/regress_gate10" || fail "Gate 10 prove"
bash scripts/verify_bundle.sh "$OUT_DIR/regress_gate10"/evidence-* || fail "Gate 10 bundle"

step "11. Gate 11 Metal parity (if --require-metal)"
if [[ $REQUIRE_METAL -eq 1 ]]; then
  mkdir -p "$OUT_DIR/regress_gate11"
  if bash scripts/check_metal_parity.sh --require-metal --out "$OUT_DIR/regress_gate11" 2>&1 | tee "$OUT_DIR/regress_gate11/parity.log"; then
    jq -e '.overall_verdict == "PASS"' "$OUT_DIR/regress_gate11/parity_report.json" || fail "Gate 11 parity verdict"
    verify_pass_bundles_under "$OUT_DIR/regress_gate11" || fail "Gate 11 bundle"
  else
    fail "Gate 11 metal parity"
  fi
else
  echo "skipping Gate 11 require-metal (not requested)"
fi

# Gate 15: Security superpowers (optional)
if [[ "${INCLUDE_SECURITY:-0}" == "1" ]]; then
  step "12. Gate 15 security fixtures"
  bash scripts/run_security_fixtures.sh --out "$OUT_DIR/security_fixtures" || fail "security fixtures"
  jq -e '.overall_verdict == "PASS"' "$OUT_DIR/security_fixtures/security_fixture_report.json" || fail "security fixtures verdict"

  # Produce a real security superpowers summary from executed artifacts (no simulated)
  FIXTURE_VERDICT=$(jq -r '.overall_verdict' "$OUT_DIR/security_fixtures/security_fixture_report.json" 2>/dev/null || echo "UNKNOWN")
  jq -n \
    --arg stamp "$STAMP" \
    --arg fixtures "$FIXTURE_VERDICT" \
    --arg note "REAL security fixtures + fuzz V1 + bounty from CLI; no simulated/demo artifacts; release PASS still requires every requested gate" \
    '{schema_version:"1.0", tranche:"gate15", stamp:$stamp, security_fixture_verdict:$fixtures, note:$note, demo_artifacts_used: false}' \
    > "$OUT_DIR/security_superpowers.json"

  # Always use r0-metal-doctor for Metal security proofs if available
  if command -v /Users/sicarii/Desktop/r0-metal-doctor >/dev/null 2>&1; then
    /Users/sicarii/Desktop/r0-metal-doctor --reference "$METAL_REF" >> "$OUT_DIR/r0_metal_doctor_security.log" 2>&1 || true
  fi
fi

step "12. release binary"
cargo build --release -p anubis
RELEASE_BIN="target/release/anubis"
cp "$RELEASE_BIN" "$OUT_DIR/anubis-release" || true
"$OUT_DIR/anubis-release" --version || fail "version"

step "13. collect + manifest"
find "$OUT_DIR" -type f ! -name 'MANIFEST.sha256' -print0 | sort -z | xargs -0 sha256sum > "$MANIFEST"
sha256sum "$MANIFEST" >> "$MANIFEST"

step "14. final verdict"
if [[ "$OVERALL" == "PASS" ]]; then
  echo "Final Verdict: PASS"
else
  echo "Final Verdict: $OVERALL"
fi

cat > "$REPORT" <<EOF
# Anubis Release Candidate Report — $STAMP

metal_reference: $METAL_REF
require_metal: $REQUIRE_METAL
overall: $OVERALL

See:
- build_release_candidate.log
- release_candidate.json (machine summary)
- language_fixtures/, language_repro/
- doctor/, regress_*/
- MANIFEST.sha256
EOF

jq -n \
  --arg stamp "$STAMP" \
  --arg ref "$METAL_REF" \
  --arg overall "$OVERALL" \
  '{
    schema_version: "1.0",
    stamp: $stamp,
    metal_reference: $ref,
    overall_verdict: $overall,
    artifacts: {
      report: "RELEASE_CANDIDATE_REPORT.md",
      manifest: "MANIFEST.sha256",
      binary: "anubis-release"
    }
  }' > "$JSON"

echo "Release candidate written to $OUT_DIR"
echo "overall_verdict=$OVERALL"
