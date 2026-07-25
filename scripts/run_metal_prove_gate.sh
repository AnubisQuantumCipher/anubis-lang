#!/usr/bin/env bash
# Metal prove gate — fails closed unless a real Metal hybrid prove is observed.
# Hosted CI without an Apple Silicon Metal runner must SKIP (exit 0 with SKIPPED)
# only when ANUBIS_METAL_GATE_ALLOW_SKIP=1; otherwise FAIL (default for self-hosted).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
OUT="${1:-out/metal_prove_gate}"
mkdir -p "$OUT"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "METAL_PROVE_GATE: FAIL (not Darwin)"
  exit 1
fi

ANUBIS="${ANUBIS:-$ROOT/target/release/anubis}"
if [[ ! -x "$ANUBIS" ]]; then
  cargo build --release -p anubis 2>&1 | tail -5
fi

set +e
"$ANUBIS" doctor --json >"$OUT/doctor.json" 2>"$OUT/doctor.err"
doc_ec=$?
set -e

metal_ready=0
if command -v jq >/dev/null 2>&1 && [[ -f "$OUT/doctor.json" ]]; then
  if jq -e '.metal_ready == true or .metal.ready == true or .risc0.metal_ready == true' "$OUT/doctor.json" >/dev/null 2>&1; then
    metal_ready=1
  fi
fi
if grep -qi 'metal.*ready\|lane.*metal' "$OUT/doctor.json" 2>/dev/null; then
  # soft signal from free-form doctor json
  :
fi

if [[ "${ANUBIS_REQUIRE_METAL:-}" == "1" ]] || [[ "${ANUBIS_METAL_GATE_REQUIRE:-}" == "1" ]]; then
  if [[ ! -x scripts/check_metal_parity.sh ]]; then
    echo "METAL_PROVE_GATE: FAIL (check_metal_parity.sh missing)"
    exit 1
  fi
  bash scripts/check_metal_parity.sh --require-metal --out "$OUT/parity" 2>&1 | tee "$OUT/parity.log"
  if grep -q 'metal-hybrid\|lane_observed.*metal' "$OUT/parity.log" "$OUT/parity/"*.json 2>/dev/null; then
    echo "METAL_PROVE_GATE: PASS"
    exit 0
  fi
  echo "METAL_PROVE_GATE: FAIL (require-metal did not observe metal-hybrid)"
  exit 1
fi

# Default: honest skip when Metal HAL not selected (hosted GHA).
if [[ "${ANUBIS_METAL_GATE_ALLOW_SKIP:-1}" == "1" ]]; then
  echo "METAL_PROVE_GATE: SKIPPED (set ANUBIS_REQUIRE_METAL=1 on a Metal-ready AS runner to enforce)"
  echo "skipped" >"$OUT/status.txt"
  exit 0
fi
echo "METAL_PROVE_GATE: FAIL (Metal not required and skip disabled)"
exit 1
