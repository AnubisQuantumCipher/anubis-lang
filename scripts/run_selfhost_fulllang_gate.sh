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
# Fail-closed: set -e so bare cargo-build failure cannot fall through to PASS on unbuilt HEAD.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
OUT="${1:-out/selfhost_fulllang_gate}"
if [[ "$OUT" != /* ]]; then OUT="$ROOT/$OUT"; fi
rm -rf "$OUT"; mkdir -p "$OUT"

if [[ -n "${ANUBIS_BIN:-}" ]]; then
  BIN="$ANUBIS_BIN"
  if [[ ! -x "$BIN" ]]; then
    echo "SELFHOST_FULLLANG_GATE: FAIL (ANUBIS_BIN=$BIN not executable)"; exit 127
  fi
else
  BIN=./target/release/anubis
  if [[ ! -x "$BIN" ]]; then
    echo "== cargo build --release -p anubis (binary missing at $BIN) =="
    cargo build -q --release -p anubis
  fi
  if [[ ! -x "$BIN" ]]; then
    echo "SELFHOST_FULLLANG_GATE: FAIL (no anubis binary after build)"; exit 127
  fi
fi
{
  echo "instrument: $BIN"
  stat -f 'mtime=%Sm size=%z' -t '%Y-%m-%dT%H:%M:%S' "$BIN" 2>/dev/null || true
} | tee "$OUT/instrument.txt"
SELF=selfhost/src/anubis_sh.anb

TIMEOUT_BIN=""
if command -v timeout >/dev/null 2>&1; then TIMEOUT_BIN=timeout
elif command -v gtimeout >/dev/null 2>&1; then TIMEOUT_BIN=gtimeout
else
  echo "SELFHOST_FULLLANG_GATE: FAIL (neither timeout nor gtimeout on PATH — required so hangs are not silent)" >&2
  exit 127
fi

pass=0; fail=0; skip=0; timed_out=0
: >"$OUT/summary.txt"
note() { echo "  $1" | tee -a "$OUT/summary.txt"; }

# Build the self-hosted compiler binary ONCE (host emits stage1, rustc compiles it),
# then use that native binary to compile each example. This is both faster and a more
# faithful test: the actual self-hosted compiler compiling the full-language corpus.
# anubis_sh.anb has no research{}/exploit{} blocks or @research/@exploit attrs ->
# program_mode = Mode::Safe -> no --allow-research needed (confirmed empirically;
# see scripts/run_selfhost_gate.sh for the full mechanism note).
set +e
"$TIMEOUT_BIN" 3600 "$BIN" run "$SELF" -- compile "$SELF" -o "$OUT/shc.rs" >"$OUT/shc_emit.log" 2>&1
shc_emit_rc=$?
set -e
if [[ $shc_emit_rc -eq 124 || $shc_emit_rc -eq 137 ]]; then
  echo "SELFHOST_FULLLANG_GATE: FAIL (self-host emit timed out rc=$shc_emit_rc — not a PASS)"; exit 1
fi
if [[ $shc_emit_rc -ne 0 ]]; then
  echo "SELFHOST_FULLLANG_GATE: FAIL (host could not emit self-host compiler, rc=$shc_emit_rc)"; exit 1
fi
if ! rustc -O "$OUT/shc.rs" -o "$OUT/shc" 2>"$OUT/shc_rustc.err"; then
  echo "SELFHOST_FULLLANG_GATE: FAIL (could not build self-host compiler binary)"; exit 1
fi

shopt -s nullglob
files=(examples/*.anb)
shopt -u nullglob
if [[ ${#files[@]} -eq 0 ]]; then
  echo "SELFHOST_FULLLANG_GATE: FAIL (no examples/*.anb — empty corpus is not PASS)"; exit 1
fi

for f in "${files[@]}"; do
  b=$(basename "$f" .anb)
  set +e
  host_out=$("$TIMEOUT_BIN" 3600 "$BIN" run "$f" 2>/dev/null); host_rc=$?
  set -e
  # Timeout is NOT "host rejects" — bucket separately so load cannot manufacture PASS via skip.
  if [[ $host_rc -eq 124 || $host_rc -eq 137 ]]; then
    note "$b: TIMEOUT (host run budget exceeded rc=$host_rc)"
    timed_out=$((timed_out+1)); continue
  fi
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
  set +e
  sh_out=$("$TIMEOUT_BIN" 3600 "$OUT/${b}.bin" 2>/dev/null); sh_rc=$?
  set -e
  if [[ $sh_rc -eq 124 || $sh_rc -eq 137 ]]; then
    note "$b: TIMEOUT (selfhost binary budget exceeded rc=$sh_rc)"
    timed_out=$((timed_out+1)); continue
  fi
  if [[ "$host_out" == "$sh_out" && "$host_rc" == "$sh_rc" ]]; then
    note "$b: PASS (host==selfhost, rc=$host_rc)"; pass=$((pass+1))
  else
    note "$b: FAIL (mismatch)"
    { echo "host_rc=$host_rc sh_rc=$sh_rc"; echo "host:[$host_out]"; echo "self:[$sh_out]"; } >>"$OUT/${b}.mismatch"
    fail=$((fail+1))
  fi
done

{
  echo "fulllang_gate pass=$pass fail=$fail skip=$skip timed_out=$timed_out"
  echo "oracle: host \`anubis run\` vs self-host compile->rustc->run over examples/*.anb"
} | tee -a "$OUT/summary.txt"

if [[ "$fail" -gt 0 ]]; then
  echo "SELFHOST_FULLLANG_GATE: FAIL ($pass pass / $fail fail / $skip skip / $timed_out timeout)"
  exit 1
fi
if [[ "$timed_out" -gt 0 ]]; then
  echo "SELFHOST_FULLLANG_GATE: FAIL ($timed_out timeout(s) — not scored as skip; hollow PASS forbidden under load)"
  exit 2
fi
if [[ "$pass" -eq 0 ]]; then
  echo "SELFHOST_FULLLANG_GATE: FAIL (zero PASS cases; all skipped — hollow PASS forbidden)"
  exit 1
fi
echo "SELFHOST_FULLLANG_GATE: PASS ($pass pass / $skip skip)"
exit 0
