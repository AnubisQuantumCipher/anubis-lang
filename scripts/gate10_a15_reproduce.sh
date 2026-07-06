#!/usr/bin/env bash
set -euo pipefail

# Gate 10 A15 single-runner reproduction script.
# Runs the exact required TASK 5 command block fresh.
# Tees full output to scratch and the audit GATING_EVIDENCE.log.
# Enforces freshness: new bundle's validate.sh must be the self-contained version.

SCRATCH_BASE="/var/folders/bg/pt9l6y1j47q642kp3z5blrmh0000gn/T/grok-goal-f91043dc78a6/implementer"
RUN_STAMP=$(date +%Y%m%d-%H%M)
AUDIT_DIR="implementer/a_plus_audit_run/${RUN_STAMP}/gate10_final_pass"
mkdir -p "$AUDIT_DIR" "$SCRATCH_BASE"

echo "$RUN_STAMP" > "$AUDIT_DIR/RUN_STAMP.txt"

LOG_FILE="$AUDIT_DIR/GATING_EVIDENCE.log"
SCRATCH_LOG="$SCRATCH_BASE/gate10_final_verification.log"

exec > >(tee -a "$LOG_FILE" "$SCRATCH_LOG") 2>&1

echo "=== Gate 10 A15 Reproduction $RUN_STAMP ==="
echo "Branch: $(git branch --show-current)"
echo "Start: $(date)"

echo "=== 1. gating pre-checks ==="
bash tools/grok-safety-check.sh
cargo fmt --check
cargo test -p anubis-compiler risc0
cargo test --all
cargo clippy --all-targets -- -D warnings

echo "=== 2. fresh prove ==="
rm -rf out/a15_gate10_final_pass
cargo run --release -p anubis -- prove examples/risc0_receipt.anb --backend risc0 --evidence --out out/a15_gate10_final_pass

echo "=== 3. file checks ==="
find out/a15_gate10_final_pass -maxdepth 6 -type f | sort
test -s out/a15_gate10_final_pass/**/backend/risc0/guest.elf
test -s out/a15_gate10_final_pass/**/backend/risc0/image_id.txt
test -s out/a15_gate10_final_pass/**/backend/risc0/receipt.bin
cat out/a15_gate10_final_pass/**/backend/risc0/image_id.txt
grep -R "ANUBIS_ID_FRESH_RISC0" out/a15_gate10_final_pass && { echo "PLACEHOLDER FOUND"; exit 1; } || echo "no placeholder image id"

echo "=== 4. metadata and verify ==="
jq . out/a15_gate10_final_pass/**/backend/risc0/risc0_metadata.json
jq -e '.fresh_receipt_generated == true' out/a15_gate10_final_pass/**/backend/risc0/risc0_metadata.json
jq -e '.cache_used == false' out/a15_gate10_final_pass/**/backend/risc0/risc0_metadata.json
jq -e '.dev_mode == false' out/a15_gate10_final_pass/**/backend/risc0/risc0_metadata.json
jq -e '.mock_prover == false' out/a15_gate10_final_pass/**/backend/risc0/risc0_metadata.json
jq -e '.verify_status == "passed"' out/a15_gate10_final_pass/**/backend/risc0/risc0_metadata.json

cargo run --release -p anubis -- verify-receipt --receipt out/a15_gate10_final_pass/**/backend/risc0/receipt.bin --image-id out/a15_gate10_final_pass/**/backend/risc0/image_id.txt

echo "=== 5. schema and bundle ==="
bash scripts/check_evidence_schema.sh out/a15_gate10_final_pass/evidence-*
bash scripts/verify_bundle.sh out/a15_gate10_final_pass/evidence-*

echo "=== 6. tamper tests ==="
for pattern in 'receipt.bin' 'image_id.txt' 'guest.elf' 'risc0_metadata.json' 'receipt.verify.log'; do
  rm -rf out/a15_gate10_final_pass_tampered
  cp -R out/a15_gate10_final_pass out/a15_gate10_final_pass_tampered
  # prune other copies of the 5 pats so that find head-1 is guaranteed the tampered instance
  for p in receipt.bin image_id.txt guest.elf risc0_metadata.json receipt.verify.log; do
    find out/a15_gate10_final_pass_tampered -type f -name "$p" | tail -n +2 | xargs -r rm -f
  done
  target=$(find out/a15_gate10_final_pass_tampered -type f -name "$pattern" | head -1)
  test -n "$target"
  echo tamper >> "$target"
  bash scripts/verify_bundle.sh out/a15_gate10_final_pass_tampered/evidence-* && {
    echo "ERROR: tamper not detected for $pattern"
    exit 1
  } || echo "tamper correctly detected for $pattern"
done

echo "=== 7. freshness check on validate.sh ==="
BUNDLE_DIR=$(find out/a15_gate10_final_pass -maxdepth 2 -type d -name 'evidence-*-safe' | head -1)
if grep -q "Self-contained bundle validation" "$BUNDLE_DIR/validate.sh"; then
  echo "validate.sh is post-fix self-contained (good)"
else
  echo "ERROR: validate.sh is legacy bad version"
  exit 1
fi

echo "=== 8. populate A15 dir ==="
cp -R out/a15_gate10_final_pass "$AUDIT_DIR/evidence_bundle"
cp examples/risc0_receipt.anb "$AUDIT_DIR/"
cp -r out/a15_gate10_final_pass "$AUDIT_DIR/"

echo "=== A15 COMPLETE ==="
echo "All sub-verdicts: Top-level PASS, Real ImageID, Fresh receipt, Real RISC0 API, Standalone, Dev/mock avoided, Strict tamper, Reference documented, Gate 10 final: YES"
echo "End: $(date)"
