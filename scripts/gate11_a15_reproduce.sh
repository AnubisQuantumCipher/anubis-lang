#!/usr/bin/env bash
set -euo pipefail

# Single-source A15 reproduction for Gate 11.
# Runs the full parity checker (distinct CPU vs Metal-hybrid proves) then the sealer.
# Exits nonzero on failure — never || true on the sealer.

STAMP=${1:-$(date +%Y%m%d-%H%M%S)}
A15_DIR="implementer/a_plus_audit_run/${STAMP}/gate11_metal_parity"
OUT_DIR="out/a15_gate11_parity"
REF="${ANUBIS_RISC0_METAL_REFERENCE:-/Users/sicarii/Desktop/metal-hybrid-prover}"

mkdir -p "$A15_DIR"

echo "=== Gate 11 A15 reproduce start $(date) ===" | tee "$A15_DIR/GATING_EVIDENCE.log"
echo "branch=$(git branch --show-current 2>/dev/null || true)" | tee -a "$A15_DIR/GATING_EVIDENCE.log"
if [[ -x tools/grok-safety-check.sh ]]; then
  bash tools/grok-safety-check.sh 2>&1 | tee -a "$A15_DIR/GATING_EVIDENCE.log" || true
fi

# Full parity: distinct *_cpu / *_metal directories under OUT_DIR
rm -rf "$OUT_DIR"
# Prefer release binary for prove performance
if [[ ! -x target/release/anubis ]]; then
  cargo build --release -p anubis 2>&1 | tee -a "$A15_DIR/GATING_EVIDENCE.log"
fi

echo "Running check_metal_parity.sh --require-metal --out $OUT_DIR" | tee -a "$A15_DIR/GATING_EVIDENCE.log"
set +e
bash scripts/check_metal_parity.sh --require-metal --out "$OUT_DIR" 2>&1 | tee "$A15_DIR/command.log" | tee -a "$A15_DIR/GATING_EVIDENCE.log"
PARITY_RC=${PIPESTATUS[0]}
set -e

# Copy artifacts into A15 dir
cp -R "$OUT_DIR"/* "$A15_DIR/" 2>/dev/null || true
if [[ -f "$OUT_DIR/parity_report.json" ]]; then
  cp -f "$OUT_DIR/parity_report.json" "$A15_DIR/"
fi

# Distinct-path honesty check: every fixture must have different cpu/metal bundle paths
python3 - <<'PY' | tee -a "$A15_DIR/GATING_EVIDENCE.log"
import json, pathlib, sys
p = pathlib.Path("out/a15_gate11_parity/parity_report.json")
if not p.exists():
    print("DISTINCT_PATHS: FAIL missing parity_report.json")
    sys.exit(2)
rep = json.load(open(p))
ok = True
for f in rep.get("fixtures", []):
    c = f.get("cpu", {}).get("bundle", "")
    m = f.get("metal", {}).get("bundle", "")
    if not c or not m or pathlib.Path(c).resolve() == pathlib.Path(m).resolve():
        print(f"DISTINCT_PATHS: FAIL fixture={f.get('name')} cpu={c} metal={m}")
        ok = False
    else:
        print(f"DISTINCT_PATHS: ok {f.get('name')}: {c} vs {m}")
        print(f"  lanes cpu={f.get('cpu',{}).get('lane_observed')} metal={f.get('metal',{}).get('lane_observed')} verdict={f.get('verdict')}")
print("overall_verdict", rep.get("overall_verdict"))
sys.exit(0 if ok else 2)
PY
DIST_RC=$?

echo "=== A15 Gate 11 reproduce complete $(date) parity_rc=$PARITY_RC distinct_rc=$DIST_RC ===" | tee -a "$A15_DIR/GATING_EVIDENCE.log"
ls -l "$A15_DIR/" | tee -a "$A15_DIR/GATING_EVIDENCE.log"

if [[ "$PARITY_RC" -ne 0 || "$DIST_RC" -ne 0 ]]; then
  echo "Gate 11 A15 reproduce FAILED" | tee -a "$A15_DIR/GATING_EVIDENCE.log"
  exit 1
fi
exit 0
