#!/usr/bin/env sh
# Self-contained evidence validation. No 'anubis' binary is used, by design:
# a bundle you can only check with the tool that produced it is not evidence.
set -eu
DIR=$(dirname "$0")

# ---- pick a SHA-256 tool (Linux first, then macOS) --------------------------
if command -v sha256sum >/dev/null 2>&1; then
  sha256() { sha256sum "$1" | cut -d' ' -f1; }
elif command -v shasum >/dev/null 2>&1; then
  sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }
else
  echo 'validate.sh: no sha256sum or shasum on PATH' >&2
  exit 2
fi

# ---- 1. integrity: nothing in the bundle was edited --------------------------
if [ ! -f "$DIR/MANIFEST.sha256" ]; then
  echo 'MISSING MANIFEST.sha256' >&2
  exit 1
fi
while read -r line; do
  [ -z "$line" ] && continue
  hash=$(echo "$line" | cut -d' ' -f1)
  file=$(echo "$line" | cut -d' ' -f2- | xargs)
  if [ -f "$DIR/$file" ]; then
    actual=$(sha256 "$DIR/$file")
    if [ "$actual" != "$hash" ]; then
      echo "TAMPER: $file hash mismatch" >&2
      exit 1
    fi
  else
    echo "MISSING: $file" >&2
    exit 1
  fi
done < "$DIR/MANIFEST.sha256"
echo 'integrity: OK (every file matches MANIFEST.sha256)'

# ---- 2. proofs: replay every refutation with an external checker -------------
PROOF_DIR="$DIR/analysis/proofs"
if [ ! -d "$PROOF_DIR" ]; then
  echo 'proofs: none exported in this bundle'
  echo 'validate.sh: OK (integrity only)'
  exit 0
fi

total=0
for cnf in "$PROOF_DIR"/obligation_*.cnf; do
  [ -e "$cnf" ] || break
  total=$((total + 1))
done
if [ "$total" -eq 0 ]; then
  echo 'proofs: none exported in this bundle'
  echo 'validate.sh: OK (integrity only)'
  exit 0
fi

if command -v drat-trim >/dev/null 2>&1; then
  CHECKER=drat-trim
elif command -v cake_lpr >/dev/null 2>&1; then
  CHECKER=cake_lpr
else
  echo "proofs: $total refutation(s) present but NOT REPLAYED: no drat-trim or cake_lpr on PATH" >&2
  echo 'validate.sh: INCOMPLETE - integrity checked, proofs unchecked' >&2
  echo '  install a DRAT checker and re-run; unchecked is not verified' >&2
  exit 3
fi

checked=0
for cnf in "$PROOF_DIR"/obligation_*.cnf; do
  [ -e "$cnf" ] || break
  drat="${cnf%.cnf}.drat"
  name=$(basename "$cnf" .cnf)
  if [ ! -f "$drat" ]; then
    echo "PROOF MISSING: $name has a formula but no refutation" >&2
    exit 1
  fi
  # The checker's EXIT CODE is the verdict, not its stdout: drat-trim prefixes
  # its "s VERIFIED" line with a carriage return, so matching on text silently
  # fails. 0 means the refutation re-derived the empty clause; nonzero means it
  # did not, which is exactly what a forged, empty, or satisfiable input gives.
  if "$CHECKER" "$cnf" "$drat" >/dev/null 2>&1; then
    checked=$((checked + 1))
  else
    echo "PROOF FAILED: $name did not replay under $CHECKER" >&2
    exit 1
  fi
done
echo "proofs: $checked/$total refutation(s) replayed and VERIFIED by $CHECKER"

# What this does and does not establish, stated in the artifact itself.
cat <<'NOTE'
validate.sh: OK
  established: every bundled file is unedited, and every exported refutation
               re-derives the empty clause under an external checker.
  NOT established: that each CNF is the faithful encoding of its .smt2, or that
               each .smt2 is the faithful obligation for the source. That link
               is still the compiler's word. See analysis/proofs.json.
NOTE
