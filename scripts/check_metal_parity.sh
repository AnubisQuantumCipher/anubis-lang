#!/usr/bin/env bash
set -euo pipefail

# Gate 11 CPU vs Metal-hybrid RISC0 parity checker.
# Usage:
#   rm -rf out/a_plus_gate11_parity
#   bash scripts/check_metal_parity.sh --require-metal --out out/a_plus_gate11_parity
#   jq . out/a_plus_gate11_parity/parity_report.json

REQUIRE_METAL=0
OUT_DIR="out/a_plus_gate11_parity"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --require-metal) REQUIRE_METAL=1; shift ;;
    --out) OUT_DIR="$2"; shift 2 ;;
    *) echo "unknown arg $1"; exit 1 ;;
  esac
done

mkdir -p "$OUT_DIR"
rm -rf "${OUT_DIR:?}"/*
REPORT="$OUT_DIR/parity_report.json"
LOG="$OUT_DIR/parity.log"

echo "=== Gate 11 Metal Parity Check ===" | tee "$LOG"
date | tee -a "$LOG"
echo "host: $(uname -a)" | tee -a "$LOG"
echo "require_metal=$REQUIRE_METAL" | tee -a "$LOG"
echo "reference: /Users/sicarii/Desktop/metal-hybrid-prover" | tee -a "$LOG"

FIXTURES=("metal_parity_hello" "metal_parity_arithmetic" "metal_parity_symbolic_safe")
OVERALL="PASS"

host_os=$(uname -s)
host_machine=$(uname -m)
apple_silicon=false
tier2=false
if [[ "$host_machine" == arm64 || "$host_machine" == aarch64 ]] && [[ "$host_os" == Darwin ]]; then
  apple_silicon=true
  # Best-effort Tier-2 probe (will be confirmed by actual lane_observed in logs)
  tier2=true
fi

if [[ "$REQUIRE_METAL" == "1" && "$apple_silicon" != "true" ]]; then
  echo "ERROR: --require-metal but not Apple Silicon" | tee -a "$LOG"
  exit 1
fi

echo '{' > "$REPORT"
echo '  "schema_version": "1.0",' >> "$REPORT"
echo '  "host": {' >> "$REPORT"
echo "    \"os\": \"$host_os\"," >> "$REPORT"
echo "    \"machine\": \"$host_machine\"," >> "$REPORT"
echo "    \"apple_silicon\": $apple_silicon," >> "$REPORT"
echo "    \"tier2_metal_available\": $tier2" >> "$REPORT"
echo '  },' >> "$REPORT"
echo '  "reference": {' >> "$REPORT"
echo '    "repo": "https://github.com/AnubisQuantumCipher/risc0-metal-hybrid",' >> "$REPORT"
echo '    "local_path": "/Users/sicarii/Desktop/metal-hybrid-prover",' >> "$REPORT"
echo '    "vendor_path": "/Users/sicarii/Desktop/metal-hybrid-prover/vendor/risc0-circuit-rv32im"' >> "$REPORT"
echo '  },' >> "$REPORT"
echo '  "fixtures": [' >> "$REPORT"

first=1
for f in "${FIXTURES[@]}"; do
  src="examples/${f}.anb"
  if [[ ! -f "$src" ]]; then
    echo "MISSING $src" | tee -a "$LOG"
    continue
  fi

  cpu_out="$OUT_DIR/${f}_cpu"
  metal_out="$OUT_DIR/${f}_metal"

  echo "=== Fixture: $f ===" | tee -a "$LOG"

  ANUBIS_BIN="${ANUBIS:-./target/release/anubis}"
  if [[ ! -x "$ANUBIS_BIN" ]]; then
    cargo build --release -p anubis 2>&1 | tee -a "$LOG"
  fi

  # CPU lane
  echo "Prove CPU (R0_DISABLE_METAL=1 --lane cpu)..." | tee -a "$LOG"
  env R0_DISABLE_METAL=1 "$ANUBIS_BIN" prove "$src" --backend risc0 --lane cpu --evidence --out "$cpu_out" 2>&1 | tee -a "$LOG" || true

  # Metal lane (unset disable; --lane metal-hybrid)
  echo "Prove Metal-hybrid (no R0_DISABLE --lane metal-hybrid)..." | tee -a "$LOG"
  env -u R0_DISABLE_METAL "$ANUBIS_BIN" prove "$src" --backend risc0 --lane metal-hybrid --evidence --out "$metal_out" 2>&1 | tee -a "$LOG" || true

  # Verify both (use our verify-receipt if present, else rely on bundle status)
  cpu_receipt="$cpu_out/backend/risc0/receipt.bin"
  cpu_id="$cpu_out/backend/risc0/image_id.txt"
  metal_receipt="$metal_out/backend/risc0/receipt.bin"
  metal_id="$metal_out/backend/risc0/image_id.txt"

  cpu_verify="skipped"
  metal_verify="skipped"
  if [[ -f "$cpu_receipt" && -f "$cpu_id" ]]; then
    "$ANUBIS_BIN" verify-receipt --receipt "$cpu_receipt" --image-id "$cpu_id" 2>&1 | tee -a "$LOG" || true
    cpu_verify="attempted"
  fi
  if [[ -f "$metal_receipt" && -f "$metal_id" ]]; then
    "$ANUBIS_BIN" verify-receipt --receipt "$metal_receipt" --image-id "$metal_id" 2>&1 | tee -a "$LOG" || true
    metal_verify="attempted"
  fi

  # Extract observed lanes from metadata or logs
  cpu_lane=$(jq -r '.metal_hybrid.lane_observed // .lane_observed // "unknown"' "$cpu_out/backend/risc0/risc0_metadata.json" 2>/dev/null || echo "unknown")
  metal_lane=$(jq -r '.metal_hybrid.lane_observed // .lane_observed // "unknown"' "$metal_out/backend/risc0/risc0_metadata.json" 2>/dev/null || echo "unknown")

  # If not present in meta, grep logs
  if [[ "$cpu_lane" == "unknown" ]]; then
    if grep -q "lane_observed=cpu\|lane=cpu" "$cpu_out/backend/risc0/receipt.verify.log" 2>/dev/null; then cpu_lane="cpu"; fi
  fi
  if [[ "$metal_lane" == "unknown" ]]; then
    if grep -q "lane_observed=metal-hybrid\|lane=metal-hybrid" "$metal_out/backend/risc0/receipt.verify.log" 2>/dev/null; then metal_lane="metal-hybrid"; fi
  fi

  cpu_id_val=$(cat "$cpu_id" 2>/dev/null | tr -d '\n' || echo "MISSING")
  metal_id_val=$(cat "$metal_id" 2>/dev/null | tr -d '\n' || echo "MISSING")

  cpu_receipt_sha=$(shasum -a 256 "$cpu_receipt" 2>/dev/null | awk '{print $1}' || echo "MISSING")
  metal_receipt_sha=$(shasum -a 256 "$metal_receipt" 2>/dev/null | awk '{print $1}' || echo "MISSING")

  cpu_meta_status=$(jq -r '.verify_status // "missing"' "$cpu_out/backend/risc0/risc0_metadata.json" 2>/dev/null || echo "missing")
  metal_meta_status=$(jq -r '.verify_status // "missing"' "$metal_out/backend/risc0/risc0_metadata.json" 2>/dev/null || echo "missing")

  image_match=false
  if [[ "$cpu_id_val" == "$metal_id_val" && "$cpu_id_val" != "MISSING" ]]; then image_match=true; fi

  # Journals come from program-derived guests (anb_main → env::commit). Both lanes prove the
  # same guest ELF (same ImageID) so journals must be bit-identical when both verify.
  # journal.bin is written by the real prove child after receipt deserialization — never hardcoded.
  cpu_journal_sha=$(shasum -a 256 "$cpu_out/backend/risc0/journal.bin" 2>/dev/null | awk '{print $1}' || echo "MISSING")
  metal_journal_sha=$(shasum -a 256 "$metal_out/backend/risc0/journal.bin" 2>/dev/null | awk '{print $1}' || echo "MISSING")
  # Honesty: cpu and metal outs must be distinct directories (no same-path compare).
  if [[ "$(cd "$cpu_out" 2>/dev/null && pwd -P)" == "$(cd "$metal_out" 2>/dev/null && pwd -P)" ]]; then
    echo "ERROR: cpu_out and metal_out resolve to the same path for $f" | tee -a "$LOG"
    OVERALL="FAIL"
  fi
  if [[ "$cpu_journal_sha" == "MISSING" || "$metal_journal_sha" == "MISSING" ]]; then
    echo "ERROR: missing extracted journal.bin for $f (CPU or Metal). Real journal extraction via verify-receipt is required; no hardcoded fallback allowed." | tee -a "$LOG"
    # Force FAIL for this fixture; do not fabricate match even if MISSING==MISSING
    journal_match=false
    output_match=false
    both_verify=false   # will be set below but we force FAIL path
  else
    journal_match=false
    output_match=false
    if $image_match && [[ "$cpu_meta_status" == "passed" && "$metal_meta_status" == "passed" ]] && [[ "$cpu_journal_sha" == "$metal_journal_sha" ]]; then
      journal_match=true
      output_match=true
    fi
  fi

  # Record for evidence
  echo "$cpu_journal_sha" > "$cpu_out/backend/risc0/journal.sha256" 2>/dev/null || true
  echo "$metal_journal_sha" > "$metal_out/backend/risc0/journal.sha256" 2>/dev/null || true

  both_verify=false
  if [[ "$cpu_meta_status" == "passed" && "$metal_meta_status" == "passed" ]]; then both_verify=true; fi

  # If journals missing, force no match and FAIL (override any both_verify)
  if [[ "$cpu_journal_sha" == "MISSING" || "$metal_journal_sha" == "MISSING" ]]; then
    journal_match=false
    output_match=false
    both_verify=false
  fi

  verdict="FAIL"
  if $image_match && $both_verify && [[ "$cpu_lane" == "cpu" ]] && [[ "$metal_lane" == "metal-hybrid" ]] && $journal_match; then
    verdict="PASS"
  elif $image_match && $both_verify && [[ "$cpu_lane" == "cpu" ]] && [[ "$metal_lane" == "cpu" ]]; then
    verdict="PARTIAL"  # metal not observed as metal
  fi

  if [[ "$verdict" != "PASS" ]]; then
    OVERALL="PARTIAL"
  fi

  # Final guard before emission: if journals missing, force false in the JSON
  if [[ "$cpu_journal_sha" == "MISSING" || "$metal_journal_sha" == "MISSING" ]]; then
    journal_match=false
    output_match=false
    both_verify=false
    verdict="FAIL"
  fi

  if [[ $first -eq 0 ]]; then echo ',' >> "$REPORT"; fi
  first=0

  cat >> "$REPORT" <<EOF
    {
      "name": "$f",
      "cpu": {
        "bundle": "$cpu_out",
        "lane_observed": "$cpu_lane",
        "receipt_verify": "$cpu_meta_status",
        "journal_sha256": "$cpu_journal_sha",
        "image_id": "$cpu_id_val"
      },
      "metal": {
        "bundle": "$metal_out",
        "lane_observed": "$metal_lane",
        "receipt_verify": "$metal_meta_status",
        "journal_sha256": "$metal_journal_sha",
        "image_id": "$metal_id_val"
      },
      "parity": {
        "image_id_match": $image_match,
        "journal_match": $journal_match,
        "output_match": $output_match,
        "both_receipts_verify": $both_verify
      },
      "verdict": "$verdict"
    }
EOF

  echo "Fixture $f verdict=$verdict cpu_lane=$cpu_lane metal_lane=$metal_lane" | tee -a "$LOG"
done

echo '  ],' >> "$REPORT"
echo "  \"overall_verdict\": \"$OVERALL\"" >> "$REPORT"
echo '}' >> "$REPORT"

echo "=== DONE ===" | tee -a "$LOG"
echo "Report: $REPORT"
cat "$REPORT" | tee -a "$LOG"

# Canonicalize via Rust single-source-of-truth sealer (Gate 11 subcommand).
# Layout under OUT_DIR uses distinct per-lane dirs: <fixture>_cpu vs <fixture>_metal.
# Passing the same OUT_DIR root for --cpu and --metal is intentional: the sealer
# resolves sibling *_cpu / *_metal paths (not the same bundle twice).
echo "Canonicalizing report via gate11-metal-parity subcommand..." | tee -a "$LOG"
SEAL_RC=0
./target/release/anubis gate11-metal-parity \
  --cpu "$OUT_DIR" --metal "$OUT_DIR" --out "$OUT_DIR" ${REQUIRE_METAL:+--require-metal} 2>&1 | tee -a "$LOG" || SEAL_RC=$?

# Re-read the canonical report for final verdict
if [[ -f "$REPORT" ]]; then
  if command -v jq >/dev/null; then
    OVERALL=$(jq -r '.overall_verdict // "UNKNOWN"' "$REPORT")
  fi
fi

echo "Final canonical overall_verdict=$OVERALL seal_rc=$SEAL_RC" | tee -a "$LOG"

# Honesty: sealer must not be ignored under --require-metal
if [[ "$REQUIRE_METAL" == "1" ]]; then
  if [[ "$OVERALL" != "PASS" || "$SEAL_RC" -ne 0 ]]; then
    echo "Metal parity FAIL under --require-metal (overall=$OVERALL seal_rc=$SEAL_RC)." | tee -a "$LOG"
    exit 1
  fi
fi

if [[ "$OVERALL" != "PASS" ]]; then
  exit 1
fi
exit 0
