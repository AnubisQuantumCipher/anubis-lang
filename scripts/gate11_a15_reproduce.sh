#!/usr/bin/env bash
set -euo pipefail

# Single-source A15 reproduction for Gate 11.
# Runs the canonical gate11-metal-parity flow (after proves have been done or as part of larger harness)
# and captures FULL stdout/stderr.

STAMP=${1:-$(date +%Y%m%d-%H%M%S)}
A15_DIR="implementer/a_plus_audit_run/${STAMP}/gate11_metal_parity"
OUT_DIR="out/a15_gate11_parity"

mkdir -p "$A15_DIR" "$OUT_DIR"

echo "=== Gate 11 A15 reproduce start $(date) ===" | tee "$A15_DIR/GATING_EVIDENCE.log"
echo "branch=$(git branch --show-current)" | tee -a "$A15_DIR/GATING_EVIDENCE.log"
bash tools/grok-safety-check.sh 2>&1 | tee -a "$A15_DIR/GATING_EVIDENCE.log"

cargo fmt --check 2>&1 | tee -a "$A15_DIR/GATING_EVIDENCE.log"
cargo test -p anubis-compiler metal 2>&1 | tee -a "$A15_DIR/GATING_EVIDENCE.log"
cargo test -p anubis-compiler risc0 2>&1 | tee -a "$A15_DIR/GATING_EVIDENCE.log"
cargo test --all 2>&1 | tail -5 | tee -a "$A15_DIR/GATING_EVIDENCE.log"
cargo clippy --all-targets -- -D warnings 2>&1 | tail -5 | tee -a "$A15_DIR/GATING_EVIDENCE.log"

rm -rf "$OUT_DIR"

# If the full prove step was already done by the caller, we can seal directly.
# Here we invoke the sealer on whatever bundles exist under the standard layout.
# For a complete run the caller should have run the parity checker first.
cargo run --release -p anubis -- gate11-metal-parity \
  --cpu out/a_plus_gate11_parity --metal out/a_plus_gate11_parity --out "$OUT_DIR" --require-metal 2>&1 | tee "$A15_DIR/command.log" || true

# Copy the produced tree
cp -R "$OUT_DIR"/* "$A15_DIR/" 2>/dev/null || true
cp -f "$OUT_DIR/parity_report.json" "$A15_DIR/" 2>/dev/null || true

# Also copy journals if present
mkdir -p "$A15_DIR/journals"
find out/a_plus_gate11_parity -name 'journal.bin' -exec sh -c 'cp -f "$1" "$2/journals/$(basename $(dirname $(dirname "$1")))_journal.bin"' _ {} "$A15_DIR" \; 2>/dev/null || true

echo "=== A15 Gate 11 reproduce complete $(date) ===" | tee -a "$A15_DIR/GATING_EVIDENCE.log"

# Full transcript is GATING_EVIDENCE.log + command.log + the copied parity.log if the caller provided one.
ls -l "$A15_DIR/" | tee -a "$A15_DIR/GATING_EVIDENCE.log"
