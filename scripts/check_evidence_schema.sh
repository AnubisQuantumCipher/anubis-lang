#!/usr/bin/env bash
set -euo pipefail

BUNDLE_DIR="$1"
SCHEMA_DIR="schemas"

if [[ -z "$BUNDLE_DIR" || ! -d "$BUNDLE_DIR" ]]; then
  echo "Usage: $0 <bundle-dir>"
  exit 1
fi

echo "Schema checking bundle layout in: $BUNDLE_DIR"

# Core layout per plan
required_top=( "evidence.json" "MANIFEST.sha256" "source.anubis" "taint-traces.json" "checks.sarif" "environment.json" )
for f in "${required_top[@]}"; do
  if [[ ! -f "$BUNDLE_DIR/$f" ]]; then
    echo "SCHEMA FAIL: missing $f"
    exit 1
  fi
done

# Optional but recommended for full
for f in hir.json mir.json solver.json source-tree.json; do
  if [[ -f "$BUNDLE_DIR/$f" ]]; then
    echo "  present: $f"
  fi
done

# Check for analysis/ or reports/ subdirs if present in some bundles
if [[ -d "$BUNDLE_DIR/analysis" || -f "$BUNDLE_DIR/taint-traces.json" ]]; then
  echo "  taint analysis present"
fi

if command -v jq >/dev/null 2>&1 && [[ -f "$SCHEMA_DIR/evidence_bundle.schema.json" ]]; then
  # Basic structural check with jq (not full jsonschema, but validates required keys)
  if jq -e 'has("timestamp") and has("verdict") and has("checks")' "$BUNDLE_DIR/evidence.json" >/dev/null; then
    echo "  evidence.json has required top keys per schema"
  else
    echo "SCHEMA FAIL: evidence.json missing keys"
    exit 1
  fi
else
  echo "  (jq or schema not available for deep validation; file presence passed)"
fi

echo "check_evidence_schema.sh: PASS for layout on $BUNDLE_DIR"
exit 0
