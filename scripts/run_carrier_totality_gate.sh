#!/usr/bin/env bash
set -uo pipefail
# Prove that adding an `Expr` variant BREAKS THE BUILD until its carrier class is stated.
#
# Phase 2 of the completion blueprint asks for exactly one thing that no test can express:
#
#     "Adding a new binder/carrier variant BREAKS THE BUILD until its consumer is written"
#
# The false-accept class closed this session was never a list of bugs — it was a shape space that
# kept growing, and nothing forced the next person adding a binder or container form to write its
# consumer. `compiler/src/middle/carrier.rs` matches every `Expr` variant with NO WILDCARD ARM, so
# rustc refuses to compile an unclassified construct.
#
# A guard nobody has watched fail is a guard taken on faith. This gate PLANTS a variant, confirms
# the build goes red with E0004 naming `carrier.rs`, and restores the file — the same RED-before-
# GREEN discipline every other guard in this repo carries.
#
# The plant goes into a SCRATCH COPY path only via backup+restore with a trap, because this repo
# has already lost in-progress work once to a backup-restore window (docs/COMMIT_5259227_CORRECTION.md)
# and a gate's own self-test is the last place that should cause it a second time.
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"

AST="compiler/src/frontend/mod.rs"
GUARD="compiler/src/middle/carrier.rs"

[[ -f "$AST" ]]   || { echo "CARRIER_TOTALITY_GATE: FAIL (missing $AST)"; exit 1; }
[[ -f "$GUARD" ]] || { echo "CARRIER_TOTALITY_GATE: FAIL (missing $GUARD — the totality guard is gone)"; exit 1; }

# A wildcard arm in the guard would silently defeat the whole mechanism while still compiling.
if grep -qE '^\s*_\s*=>' "$GUARD"; then
  echo "CARRIER_TOTALITY_GATE: FAIL (a wildcard arm in $GUARD defeats variant totality)"
  exit 1
fi

BACKUP="$(mktemp -t anubis_ast_backup).rs"
restore() { [[ -f "$BACKUP" ]] && cp "$BACKUP" "$AST"; rm -f "$BACKUP"; }
trap restore EXIT
cp "$AST" "$BACKUP"

python3 - "$AST" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
i = s.index('pub enum Expr {')
old = '    Other(String),\n}'
j = s.index(old, i)
new = ('    Other(String),\n'
       '    AnubisCarrierProbe { inner: Box<Expr> },\n}')
open(p, 'w').write(s[:j] + new + s[j + len(old):])
print("planted: Expr::AnubisCarrierProbe")
PY

set +e
OUT="$(cargo build --release -p anubis-compiler 2>&1)"
set -e
restore
trap - EXIT

if ! grep -q 'E0004' <<<"$OUT"; then
  echo "CARRIER_TOTALITY_GATE: FAIL (an unclassified Expr variant COMPILED — totality is not enforced)"
  exit 1
fi
if ! grep -q 'carrier.rs' <<<"$OUT"; then
  echo "CARRIER_TOTALITY_GATE: FAIL (build broke, but not in $GUARD — the carrier guard did not catch it)"
  exit 1
fi

# And it must build clean again, or the gate has left the tree broken.
if ! cargo build --release -p anubis-compiler >/dev/null 2>&1; then
  echo "CARRIER_TOTALITY_GATE: FAIL (tree does not build after restore)"
  exit 1
fi

echo "CARRIER_TOTALITY_GATE: PASS (an unclassified Expr variant fails to compile in carrier.rs; tree restored and green)"
