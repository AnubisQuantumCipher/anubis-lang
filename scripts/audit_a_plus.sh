#!/usr/bin/env bash
set -euo pipefail
STAMP=$(date +%Y%m%d-%H%M%S)
OUT="implementer/a_plus_audit_run/${STAMP}"
mkdir -p "$OUT"
echo "=== A+ AUDIT RUN $STAMP ===" | tee "$OUT/GATING_EVIDENCE.log"
echo "pwd: $(pwd)" | tee -a "$OUT/GATING_EVIDENCE.log"
bash tools/grok-safety-check.sh || exit 1

echo "GATE 1 clean build..." | tee -a "$OUT/GATING_EVIDENCE.log"
cargo fmt --check | tee -a "$OUT/GATING_EVIDENCE.log"
cargo test --all 2>&1 | tail -20 | tee -a "$OUT/GATING_EVIDENCE.log"
cargo clippy --all-targets -- -D warnings 2>&1 | tail -5 | tee -a "$OUT/GATING_EVIDENCE.log"
cargo build --release 2>&1 | tail -3 | tee -a "$OUT/GATING_EVIDENCE.log"

# TODO: add remaining gates as implemented (taint rejection smoke, evidence validate, examples, repro, etc.)
echo "A+ audit skeleton complete. Full gates added in later phases." | tee -a "$OUT/GATING_EVIDENCE.log"
echo "$STAMP" > "$OUT/RUN_STAMP.txt"
ls -l "$OUT"
