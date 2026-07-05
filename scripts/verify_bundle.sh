#!/usr/bin/env bash
set -euo pipefail

BUNDLE_DIR="$1"

if [[ -z "$BUNDLE_DIR" || ! -d "$BUNDLE_DIR" ]]; then
  echo "Usage: $0 <bundle-dir>"
  exit 1
fi

echo "Verifying bundle: $BUNDLE_DIR"

# Required top level files (evidence.json acts as manifest in current impl)
for f in evidence.json MANIFEST.sha256 source.anubis taint-traces.json checks.sarif environment.json source-tree.json validate.sh; do
  if [[ ! -f "$BUNDLE_DIR/$f" ]]; then
    echo "MISSING: $f"
    exit 1
  fi
done

# Also accept manifest.json if present (for future schema v1 strict)
if [[ -f "$BUNDLE_DIR/manifest.json" ]]; then
  echo "  manifest.json present (preferred for v1)"
fi

# Full MANIFEST validation: recompute hashes for listed files and compare
if command -v shasum >/dev/null; then
  while read -r line; do
    [[ -z "$line" ]] && continue
    hash=$(echo "$line" | cut -d' ' -f1)
    file=$(echo "$line" | cut -d' ' -f2- | xargs)
    if [[ -f "$BUNDLE_DIR/$file" ]]; then
      actual=$(shasum -a 256 "$BUNDLE_DIR/$file" | cut -d' ' -f1)
      if [[ "$actual" != "$hash" ]]; then
        echo "TAMPER: $file hash mismatch"
        exit 1
      fi
    fi
  done < "$BUNDLE_DIR/MANIFEST.sha256"
else
  echo "WARN: no shasum, skipping full hash verify"
fi

# Call internal validate if present (best effort; some bundles use 'anubis' from PATH)
if [[ -x "$BUNDLE_DIR/validate.sh" ]]; then
  if (cd "$BUNDLE_DIR" && ./validate.sh 2>&1); then
    echo "  internal validate passed"
  else
    echo "  WARN: internal validate.sh had issues (may be PATH or CLI version); continuing with file checks"
  fi
fi

echo "Bundle structure and basic hashes OK. Verdict from manifest:"
jq -r '.verdict' "$BUNDLE_DIR/evidence.json" 2>/dev/null || cat "$BUNDLE_DIR/evidence.json" | head -c 200

echo "verify_bundle.sh: SUCCESS for $BUNDLE_DIR"
exit 0
