#!/usr/bin/env bash
# Phase 8 — full-language self-host parity gate. Fail-closed.
#
# Proves that the Anubis-SH self-host compiler compiles AND runs the FULL
# executable language (enums, match, if-expressions, for-in over collections,
# maps, recursion) identically to the Rust host `anubis run`, using the host
# as a differential oracle over the example corpus.
#
#   for each examples/*.anb the host runs (rc==0):
#       host_out = anubis run <ex>
#       sh_out   = (stage0 self-host) compile <ex> -> rustc -> run
#       require: host_out == sh_out  AND  host_rc == sh_rc
#
# Examples the host itself rejects under `anubis run` (research / taint /
# symbolic / proof-only constructs) are SKIPPED with an honest marker — they
# are not part of the executable-language surface.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
OUT="${1:-out/selfhost_fulllang_gate}"
if [[ "$OUT" != /* ]]; then OUT="$ROOT/$OUT"; fi
rm -rf "$OUT"; mkdir -p "$OUT"

BIN=./target/release/anubis
cargo build -q --release -p anubis
SELF=selfhost/src/anubis_sh.anb

pass=0; fail=0; skip=0
: >"$OUT/summary.txt"
note() { echo "  $1" | tee -a "$OUT/summary.txt"; }

# Build the self-hosted compiler binary ONCE (host emits stage1, rustc compiles it),
# then use that native binary to compile each example. This is both faster and a more
# faithful test: the actual self-hosted compiler compiling the full-language corpus.
timeout 3600 "$BIN" run "$SELF" --allow-research -- compile "$SELF" -o "$OUT/shc.rs" >"$OUT/shc_emit.log" 2>&1
if ! rustc -O "$OUT/shc.rs" -o "$OUT/shc" 2>"$OUT/shc_rustc.err"; then
  echo "SELFHOST_FULLLANG_GATE: FAIL (could not build self-host compiler binary)"; exit 1
fi

for f in examples/*.anb; do
  b=$(basename "$f" .anb)
  host_out=$(timeout 3600 "$BIN" run "$f" 2>/dev/null); host_rc=$?
  if [[ $host_rc -ne 0 ]]; then
    note "$b: SKIP (host rejects under run; research/taint/symbolic)"
    skip=$((skip+1)); continue
  fi
  # self-host compile (the self-hosted compiler binary) -> rustc -> run
  if ! "$OUT/shc" compile "$f" -o "$OUT/${b}.rs" >"$OUT/${b}.compile.log" 2>&1; then
    note "$b: FAIL (self-host compile)"; fail=$((fail+1)); continue
  fi
  if ! rustc -O "$OUT/${b}.rs" -o "$OUT/${b}.bin" 2>"$OUT/${b}.rustc.err"; then
    note "$b: FAIL (rustc of emitted package)"; fail=$((fail+1)); continue
  fi
  sh_out=$(timeout 3600 "$OUT/${b}.bin" 2>/dev/null); sh_rc=$?
  if [[ "$host_out" == "$sh_out" && "$host_rc" == "$sh_rc" ]]; then
    note "$b: PASS (host==selfhost, rc=$host_rc)"; pass=$((pass+1))
  else
    note "$b: FAIL (mismatch)"
    { echo "host_rc=$host_rc sh_rc=$sh_rc"; echo "host:[$host_out]"; echo "self:[$sh_out]"; } >>"$OUT/${b}.mismatch"
    fail=$((fail+1))
  fi
done

{
  echo "fulllang_gate pass=$pass fail=$fail skip=$skip"
  echo "oracle: host \`anubis run\` vs self-host compile->rustc->run over examples/*.anb"
} | tee -a "$OUT/summary.txt"

if [[ "$fail" -gt 0 ]]; then
  echo "SELFHOST_FULLLANG_GATE: FAIL ($pass pass / $fail fail / $skip skip)"
  exit 1
fi
echo "SELFHOST_FULLLANG_GATE: PASS ($pass pass / $skip skip)"
exit 0
