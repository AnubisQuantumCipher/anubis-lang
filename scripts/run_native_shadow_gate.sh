#!/usr/bin/env bash
# Phase-7 native-solver cross-check gate. Runs `anubis check` over the whole corpus with the native
# QF_BV solver shadowing z3 on every obligation, and requires ZERO native-vs-z3 disagreements. z3
# stays authoritative — this only measures how much of the real obligation stream the native solver
# already decides correctly. When AGREE+NATIVE_ONLY covers the integer lane and DISAGREE stays 0
# (with the Lean bit-blaster proof), the compiler can flip native-authoritative and drop z3.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
source "$ROOT/scripts/lib/gate_common.sh"
BIN="${ANUBIS_BIN:-./target/release/anubis}"
LOG="$(mktemp)"
: > "$LOG"

[[ -x "$BIN" ]] || { echo "NATIVE_SHADOW_GATE: FAIL (binary not executable: $BIN)"; exit 127; }
export ANUBIS_NATIVE_SHADOW=1
export ANUBIS_NATIVE_SHADOW_LOG="$LOG"

n=0
for f in $(find examples tests/fixtures -name '*.anb' | sort); do
  "$BIN" check "$f" >/dev/null 2>>"$ROOT/out/native_shadow.err" || true
  n=$((n+1))
done

agree=$(grep -c '^AGREE$' "$LOG" || true)
defer=$(grep -c '^DEFER$' "$LOG" || true)
native_only=$(grep -c '^NATIVE_ONLY$' "$LOG" || true)
disagree=$(grep -c '^DISAGREE$' "$LOG" || true)
total=$((agree + defer + native_only + disagree))

# Coverage ratchet (adversary R49): file corpus size ($n) must not silently shrink.
set +e
assert_floor "native_shadow_gate" "$n" "$ROOT/scripts/floors/native_shadow_gate.count_floor"
_floor_rc=$?
set -e
if [[ $_floor_rc -ne 0 ]]; then
  echo "FLOOR: FAIL ($n files; $GATE_FLOOR_ERROR)" >&2
  echo "NATIVE_SHADOW_GATE: FAIL (coverage floor)"
  exit 1
fi

echo "NATIVE_SHADOW over $n files, $total obligations:"
echo "  AGREE       = $agree   (native decided, matched z3)"
echo "  NATIVE_ONLY = $native_only   (native decided, z3 unknown/errored)"
echo "  DEFER       = $defer   (native declined → z3)"
echo "  DISAGREE    = $disagree"
rm -f "$LOG"

if ! require_nonempty_corpus "$n" "examples|tests/fixtures/**/*.anb"; then
  echo "NATIVE_SHADOW_GATE: FAIL (empty corpus)"
  exit 1
fi
if [ "$disagree" -gt 0 ]; then
  echo "NATIVE_SHADOW_GATE: FAIL ($disagree disagreements — see stderr ANUBIS_NATIVE_DISAGREE lines)"
  exit 1
fi
echo "NATIVE_SHADOW_GATE: PASS (0 disagreements; native decided $((agree + native_only))/$total)"
exit 0
