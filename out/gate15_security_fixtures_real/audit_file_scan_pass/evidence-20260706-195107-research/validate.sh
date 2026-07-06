#!/usr/bin/env sh
set -eu
# Self-contained bundle validation (no 'anubis' CLI dependency to avoid arg parsing errors).
# Checks that all files listed in MANIFEST.sha256 still match their recorded hashes.
DIR=$(dirname "$0")
if [ ! -f "$DIR/MANIFEST.sha256" ]; then
  echo 'MISSING MANIFEST.sha256' >&2
  exit 1
fi
while read -r line; do
  [ -z "$line" ] && continue
  hash=$(echo "$line" | cut -d' ' -f1)
  file=$(echo "$line" | cut -d' ' -f2- | xargs)
  if [ -f "$DIR/$file" ]; then
    actual=$(shasum -a 256 "$DIR/$file" | cut -d' ' -f1)
    if [ "$actual" != "$hash" ]; then
      echo "TAMPER: $file hash mismatch" >&2
      exit 1
    fi
  else
    echo "MISSING: $file" >&2
    exit 1
  fi
done < "$DIR/MANIFEST.sha256"
echo 'validate.sh: OK'
