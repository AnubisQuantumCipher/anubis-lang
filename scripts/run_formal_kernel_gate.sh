#!/usr/bin/env bash
# formal_kernel gate — pure-Anubis SAT kernel + independent Python oracle.
# Fails closed: any anubis check/run failure or oracle mismatch is non-zero.
#
# Seshat R8: private OUT (no world-shared /tmp/formal_kernel_*.out race);
# honor ANUBIS_BIN exclusively under seal pin.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

OUT="${1:-${ANUBIS_FORMAL_KERNEL_OUT:-out/formal_kernel_gate}}"
if [[ "$OUT" != /* ]]; then OUT="$ROOT/$OUT"; fi
mkdir -p "$OUT"

if [[ -n "${ANUBIS_BIN:-}" ]]; then
  ANUBIS="$ANUBIS_BIN"
  if [[ ! -x "$ANUBIS" ]]; then
    echo "FORMAL_KERNEL_GATE: FAIL (ANUBIS_BIN=$ANUBIS not executable)"
    exit 127
  fi
else
  ANUBIS="${ANUBIS:-$ROOT/target/release/anubis}"
  if [[ ! -x "$ANUBIS" ]]; then
    echo "formal_kernel_gate: building release anubis..."
    cargo build --release -p anubis
  fi
fi
if [[ ! -x "$ANUBIS" ]]; then
  echo "FORMAL_KERNEL_GATE: FAIL (no executable anubis at $ANUBIS)"
  exit 127
fi

{
  echo "formal_kernel_instrument_v1"
  echo "ANUBIS=$ANUBIS"
  echo "out=$OUT"
  stat -f 'size=%z mtime=%Sm' -t '%Y-%m-%dT%H:%M:%S' "$ANUBIS" 2>/dev/null \
    || stat -c 'size=%s mtime=%y' "$ANUBIS" 2>/dev/null \
    || true
} | tee "$OUT/instrument.txt"

DIR=examples/programs/formal_kernel
echo "=== formal_kernel check/run ==="
"$ANUBIS" check "$DIR/formal_kernel.anb"
"$ANUBIS" run "$DIR/formal_kernel.anb" | tee "$OUT/formal_kernel_run.out" | tail -5
grep -q 'formal_kernel: ok' "$OUT/formal_kernel_run.out"
echo "=== formal_kernel_hard_tests check/run ==="
"$ANUBIS" check "$DIR/formal_kernel_hard_tests.anb"
"$ANUBIS" run "$DIR/formal_kernel_hard_tests.anb" | tee "$OUT/formal_kernel_hard.out" | tail -5
grep -q 'formal_kernel_hard_tests: ok' "$OUT/formal_kernel_hard.out"
echo "=== independent oracle (Python) ==="
python3 "$DIR/independent_oracle.py" | tee "$OUT/formal_kernel_oracle.out"
grep -q 'oracle_summary failed=0/12' "$OUT/formal_kernel_oracle.out"
echo "FORMAL_KERNEL_GATE: PASS"
