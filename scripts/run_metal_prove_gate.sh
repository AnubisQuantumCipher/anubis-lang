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

# Honor seal pin: ANUBIS_BIN exclusive — never rebuild under a pin (Seshat R8).
if [[ -n "${ANUBIS_BIN:-}" ]]; then
  ANUBIS="$ANUBIS_BIN"
  if [[ ! -x "$ANUBIS" ]]; then
    echo "METAL_PROVE_GATE: FAIL (ANUBIS_BIN=$ANUBIS not executable)"
    exit 127
  fi
else
  ANUBIS="${ANUBIS:-$ROOT/target/release/anubis}"
  if [[ ! -x "$ANUBIS" ]]; then
    cargo build --release -p anubis 2>&1 | tail -5
  fi
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
  set +e
  bash scripts/check_metal_parity.sh --require-metal --out "$OUT/parity" 2>&1 | tee "$OUT/parity.log"
  pc=${PIPESTATUS[0]}
  set -e
  # Prefer overall_verdict=PASS. A "witness" of metal-hybrid in residual JSON is only
  # accepted when the child gate itself exited 0 — otherwise a failing/crashing
  # check_metal_parity.sh plus a stale or partial report could print PASS while
  # the evidence is hollow (Seshat T2, 2026-07-26).
  if [[ "$pc" -ne 0 ]]; then
    echo "METAL_PROVE_GATE: FAIL (check_metal_parity.sh exited $pc — witness path forbidden on nonzero child)"
    echo "fail_child_$pc" >"$OUT/status.txt"
    exit 1
  fi
  if grep -q '"overall_verdict": "PASS"' "$OUT/parity/parity_report.json" 2>/dev/null; then
    echo "METAL_PROVE_GATE: PASS"
    echo pass >"$OUT/status.txt"
    exit 0
  fi
  # Default STRICT when requiring metal: PARTIAL + witness line is hollow for a prove seal
  # (Seshat R8). Opt-in ANUBIS_METAL_GATE_ALLOW_WITNESS=1 restores the soft witness path.
  if grep -q 'lane_observed.: .metal-hybrid' "$OUT/parity/parity_report.json" 2>/dev/null \
    && grep -q '"receipt_verify": "passed"' "$OUT/parity/parity_report.json" 2>/dev/null; then
    if [[ "${ANUBIS_METAL_GATE_ALLOW_WITNESS:-0}" == "1" && "${ANUBIS_METAL_GATE_STRICT:-1}" != "1" ]]; then
      echo "METAL_PROVE_GATE: PASS (metal-hybrid witnessed with verified receipt; child rc=0; overall may be PARTIAL; ALLOW_WITNESS=1)"
      echo "pass_witness" >"$OUT/status.txt"
      exit 0
    fi
    echo "METAL_PROVE_GATE: FAIL (overall_verdict is not PASS — witness-only path is hollow; set ANUBIS_METAL_GATE_ALLOW_WITNESS=1 to opt in)"
    echo "fail_witness_only" >"$OUT/status.txt"
    exit 1
  fi
  echo "METAL_PROVE_GATE: FAIL (require-metal did not observe verifying metal-hybrid)"
  exit 1
fi

# Skip is explicit opt-in for a caller that is not making a Metal claim (for example hosted GHA).
if [[ "${ANUBIS_METAL_GATE_ALLOW_SKIP:-0}" == "1" ]]; then
  echo "METAL_PROVE_GATE: SKIPPED (set ANUBIS_REQUIRE_METAL=1 on a Metal-ready AS runner to enforce)"
  echo "skipped" >"$OUT/status.txt"
  exit 0
fi
echo "METAL_PROVE_GATE: FAIL (Metal not required and skip disabled)"
exit 1
