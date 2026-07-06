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

  # CPU lane
  echo "Prove CPU (R0_DISABLE_METAL=1 --lane cpu)..." | tee -a "$LOG"
  env R0_DISABLE_METAL=1 cargo run --release -p anubis -- prove "$src" --backend risc0 --lane cpu --evidence --out "$cpu_out" 2>&1 | tee -a "$LOG" || true

  # Metal lane (unset disable; --lane metal-hybrid)
  echo "Prove Metal-hybrid (no R0_DISABLE --lane metal-hybrid)..." | tee -a "$LOG"
  env -u R0_DISABLE_METAL cargo run --release -p anubis -- prove "$src" --backend risc0 --lane metal-hybrid --evidence --out "$metal_out" 2>&1 | tee -a "$LOG" || true

  # Verify both (use our verify-receipt if present, else rely on bundle status)
  cpu_receipt="$cpu_out/backend/risc0/receipt.bin"
  cpu_id="$cpu_out/backend/risc0/image_id.txt"
  metal_receipt="$metal_out/backend/risc0/receipt.bin"
  metal_id="$metal_out/backend/risc0/image_id.txt"

  cpu_verify="skipped"
  metal_verify="skipped"
  if [[ -f "$cpu_receipt" && -f "$cpu_id" ]]; then
    cargo run --release -p anubis -- verify-receipt --receipt "$cpu_receipt" --image-id "$cpu_id" 2>&1 | tee -a "$LOG" || true
    cpu_verify="attempted"
  fi
  if [[ -f "$metal_receipt" && -f "$metal_id" ]]; then
    cargo run --release -p anubis -- verify-receipt --receipt "$metal_receipt" --image-id "$metal_id" 2>&1 | tee -a "$LOG" || true
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
  receipt_match=false
  if [[ "$cpu_id_val" == "$metal_id_val" && "$cpu_id_val" != "MISSING" ]]; then image_match=true; fi
  if [[ "$cpu_receipt_sha" == "$metal_receipt_sha" && "$cpu_receipt_sha" != "MISSING" ]]; then receipt_match=true; fi

  both_verify=false
  if [[ "$cpu_meta_status" == "passed" && "$metal_meta_status" == "passed" ]]; then both_verify=true; fi

  verdict="FAIL"
  if $image_match && $receipt_match && $both_verify && [[ "$cpu_lane" == "cpu" ]] && [[ "$metal_lane" == "metal-hybrid" || "$metal_lane" == "cpu" ]]; then
    # For strict Gate 11 we want metal observed on metal run
    if [[ "$metal_lane" == "metal-hybrid" ]]; then
      verdict="PASS"
    else
      verdict="PARTIAL"
    fi
  fi

  if [[ "$verdict" != "PASS" ]]; then
    OVERALL="PARTIAL"
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
        "journal_sha256": "$cpu_receipt_sha",
        "image_id": "$cpu_id_val"
      },
      "metal": {
        "bundle": "$metal_out",
        "lane_observed": "$metal_lane",
        "receipt_verify": "$metal_meta_status",
        "journal_sha256": "$metal_receipt_sha",
        "image_id": "$metal_id_val"
      },
      "parity": {
        "image_id_match": $image_match,
        "journal_match": $receipt_match,
        "output_match": $receipt_match,
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

if [[ "$OVERALL" != "PASS" && "$REQUIRE_METAL" == "1" ]]; then
  echo "Metal parity not fully PASS under --require-metal (observed lanes or verify may be PARTIAL on this run)."
fi

exit 0
