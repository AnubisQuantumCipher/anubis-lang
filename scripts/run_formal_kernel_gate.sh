#!/usr/bin/env bash
# formal_kernel gate — pure-Anubis SAT kernel + independent Python oracle.
# Fails closed: any anubis check/run failure or oracle mismatch is non-zero.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
ANUBIS="${ANUBIS:-$ROOT/target/release/anubis}"
if [[ ! -x "$ANUBIS" ]]; then
  echo "formal_kernel_gate: building release anubis..."
  cargo build --release -p anubis
fi
DIR=examples/programs/formal_kernel
echo "=== formal_kernel check/run ==="
"$ANUBIS" check "$DIR/formal_kernel.anb"
"$ANUBIS" run "$DIR/formal_kernel.anb" | tee /tmp/formal_kernel_run.out | tail -5
grep -q 'formal_kernel: ok' /tmp/formal_kernel_run.out
echo "=== formal_kernel_hard_tests check/run ==="
"$ANUBIS" check "$DIR/formal_kernel_hard_tests.anb"
"$ANUBIS" run "$DIR/formal_kernel_hard_tests.anb" | tee /tmp/formal_kernel_hard.out | tail -5
grep -q 'formal_kernel_hard_tests: ok' /tmp/formal_kernel_hard.out
echo "=== independent oracle (Python) ==="
python3 "$DIR/independent_oracle.py" | tee /tmp/formal_kernel_oracle.out
grep -q 'oracle_summary failed=0/12' /tmp/formal_kernel_oracle.out
echo "FORMAL_KERNEL_GATE: PASS"
