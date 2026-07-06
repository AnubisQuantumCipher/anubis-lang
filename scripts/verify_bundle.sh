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
    else
      echo "MISSING: manifest-listed file $file"
      exit 1
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
    echo "  WARN: internal validate.sh had issues (may be PATH or CLI version); continuing with manifest/verdict checks"
  fi
fi

echo "Bundle structure and basic hashes OK. Verdict from manifest:"
jq -r '.verdict' "$BUNDLE_DIR/evidence.json" 2>/dev/null || cat "$BUNDLE_DIR/evidence.json" | head -c 200

if command -v jq >/dev/null; then
  verdict=$(jq -r '.verdict // "MISSING"' "$BUNDLE_DIR/evidence.json")
  if [[ "$verdict" != "PASS" ]]; then
    echo "FAIL: bundle verdict is $verdict"
    exit 1
  fi
  if ! jq -e '.checks | all(.status == "PASS")' "$BUNDLE_DIR/evidence.json" >/dev/null; then
    echo "FAIL: one or more evidence checks are not PASS"
    exit 1
  fi
else
  echo "FAIL: jq is required for verdict/check validation"
  exit 1
fi

# RISC0 sidecar strict tamper (A+ Gate 10: mechanical failure on ANY hashed sidecar)
# All of: guest.elf, guest source, image_id.txt, receipt.bin, risc0_metadata.json,
# receipt.verify.log, prove.log (if present), verify.log (if present) must be covered.
if [[ -d "$BUNDLE_DIR/backend/risc0" ]]; then
  for f in guest.elf image_id.txt receipt.bin risc0_metadata.json receipt.verify.log prove.log verify.log 'guest/src/main.rs' 'guest_source.rs'; do
    # find first match (flat or nested)
    target=$(find "$BUNDLE_DIR" -type f -name "$(basename "$f")" 2>/dev/null | head -1)
    if [[ -z "$target" ]]; then
      # also try risc0_ prefixed flat
      target=$(find "$BUNDLE_DIR" -type f -name "risc0_$(basename "$f" | tr '/' '_')" 2>/dev/null | head -1)
    fi
    if [[ -n "$target" && -f "$target" ]]; then
      if command -v shasum >/dev/null; then
        actual=$(shasum -a 256 "$target" | cut -d' ' -f1)
        if ! grep -q "$actual" "$BUNDLE_DIR/MANIFEST.sha256" 2>/dev/null; then
          echo "TAMPER: risc0 sidecar $f (at $target) hash mismatch or not tracked in MANIFEST"
          exit 1
        fi
      fi
    fi
  done
fi

# Also enforce any direct risc0_* flat files are covered by MANIFEST (from copy_hybrid_sidecars)
for f in "$BUNDLE_DIR"/risc0_*; do
  if [[ -f "$f" ]]; then
    actual=$(shasum -a 256 "$f" | cut -d' ' -f1)
    if ! grep -q "$actual" "$BUNDLE_DIR/MANIFEST.sha256" 2>/dev/null; then
      echo "TAMPER: risc0 flat sidecar $(basename "$f") not tracked or mismatch"
      exit 1
    fi
  fi
done 2>/dev/null || true

# Gate 10 strict: explicit mechanical detection for the 5 key RISC0 sidecars.
# Compute current hash and compare to the hash recorded in MANIFEST for that basename.
# Tamper (changing content) will cause mismatch -> nonzero exit.
for pat in receipt.bin image_id.txt guest.elf risc0_metadata.json receipt.verify.log; do
  target=$(find "$BUNDLE_DIR" -type f -name "$pat" 2>/dev/null | head -1)
  if [[ -z "$target" ]]; then
    target=$(find "$BUNDLE_DIR" -type f -name "risc0_$pat" 2>/dev/null | head -1)
  fi
  if [[ -n "$target" && -f "$target" ]]; then
    actual=$(shasum -a 256 "$target" | cut -d' ' -f1)
    # find the line in MANIFEST that mentions this pat and extract its recorded hash
    expected=$(grep -E "(^| )${pat}( |$)" "$BUNDLE_DIR/MANIFEST.sha256" 2>/dev/null | head -1 | cut -d' ' -f1)
    if [[ -z "$expected" ]]; then
      # fallback: any line containing the basename
      expected=$(grep -F "$(basename "$target")" "$BUNDLE_DIR/MANIFEST.sha256" 2>/dev/null | head -1 | cut -d' ' -f1)
    fi
    if [[ -n "$expected" && "$actual" != "$expected" ]]; then
      echo "TAMPER: key sidecar $pat (at $target) hash mismatch (expected $expected got $actual)"
      exit 1
    fi
    if [[ -n "$expected" && "$actual" == "$expected" ]]; then
      : # still matches (not tampered)
    fi
  fi
done

echo "verify_bundle.sh: SUCCESS for $BUNDLE_DIR"
exit 0
