#!/usr/bin/env bash
# Phase 8 — dogfood gate. Fail-closed. Proves the self-host compiler's OWN source is
# written in idiomatic Anubis (enums + match + if-expressions) in *load-bearing*
# positions — not cosmetically. Three layers; the third (ablation) is what neither
# Zig nor Dafny ships.
#
#   G1 structural : the compiler's own AST contains the load-bearing enums/match/if-expr
#   G2 semantic   : (delegated) run_selfhost_gate.sh fixpoint + run_selfhost_fulllang_gate.sh
#   G3 ablation   : neuter a match arm -> the self-build must break (removal breaks output)
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
OUT="${1:-out/selfhost_dogfood_gate}"
if [[ "$OUT" != /* ]]; then OUT="$ROOT/$OUT"; fi
rm -rf "$OUT"; mkdir -p "$OUT"
BIN=./target/release/anubis
SELF=selfhost/src/anubis_sh.anb
cargo build -q --release -p anubis
pass=0; fail=0
note() { echo "  $1" | tee -a "$OUT/summary.txt"; }
: >"$OUT/summary.txt"

# Build a fast BOOT compiler once (host emits stage1, rustc compiles it). BOOT compiles
# the (possibly ablated) source in ~1s each thereafter.
echo "== building BOOT compiler =="
# anubis_sh.anb has no research{}/exploit{} blocks or @research/@exploit attrs
# -> program_mode = Mode::Safe -> --allow-research is not needed (confirmed
# empirically; see scripts/run_selfhost_gate.sh for the full mechanism note).
"$BIN" run "$SELF" -- compile "$SELF" -o "$OUT/boot.rs" >"$OUT/boot_emit.log" 2>&1
if ! rustc -O "$OUT/boot.rs" -o "$OUT/boot" 2>"$OUT/boot_rustc.err"; then
  echo "SELFHOST_DOGFOOD_GATE: FAIL (could not build BOOT)"; exit 1
fi

# ---- G1 structural (AST-based) ----
echo "== G1 structural (enums + match + if-expr in load-bearing functions) =="
"$OUT/boot" parse "$SELF" >"$OUT/self_ast.json" 2>"$OUT/self_ast.err"
if python3 scripts/dogfood_ast_check.py "$OUT/self_ast.json" | tee "$OUT/g1.log"; then
  pass=$((pass+1)); note "G1_structural: PASS"
else
  fail=$((fail+1)); note "G1_structural: FAIL"; cat "$OUT/g1.log" >>"$OUT/summary.txt"
fi

# ---- G3 ablation (neuter a load-bearing arm -> build must break) ----
# Probe program + its true (host) output. Uses variables and let-bindings.
PROBE=examples/for_in_list.anb
HOST_OUT=$("$BIN" run "$PROBE" 2>/dev/null)
echo "== G3 ablation (host $PROBE = [$HOST_OUT]) =="
ablate_breaks() {
  local target="$1"
  python3 scripts/dogfood_ablate.py "$SELF" "$OUT/ablated_${target}.anb" "$target" >>"$OUT/summary.txt" 2>&1 || { echo "ablate-pattern-missing"; return 2; }
  # BOOT compiles the ablated compiler -> ablated stage1
  if ! "$OUT/boot" compile "$OUT/ablated_${target}.anb" -o "$OUT/abl_${target}_s1.rs" >"$OUT/abl_${target}.log" 2>&1; then
    echo "broke-at-selfcompile"; return 0
  fi
  if ! rustc -O "$OUT/abl_${target}_s1.rs" -o "$OUT/abl_${target}_s1" 2>"$OUT/abl_${target}_rustc.err"; then
    echo "broke-at-rustc"; return 0
  fi
  # ablated compiler compiles the probe -> run it
  if ! "$OUT/abl_${target}_s1" compile "$PROBE" -o "$OUT/abl_${target}_probe.rs" 2>/dev/null; then
    echo "broke-at-probe-compile"; return 0
  fi
  if ! rustc -O "$OUT/abl_${target}_probe.rs" -o "$OUT/abl_${target}_probe" 2>/dev/null; then
    echo "broke-at-probe-rustc"; return 0
  fi
  local abl_out
  abl_out=$("$OUT/abl_${target}_probe" 2>/dev/null)
  if [[ "$abl_out" == "$HOST_OUT" ]]; then
    echo "NOT-BROKEN(out=[$abl_out])"; return 1   # ablation had no effect -> cosmetic -> FAIL
  fi
  echo "broke-output([$abl_out]!=[$HOST_OUT])"; return 0
}
for target in var let; do
  res=$(ablate_breaks "$target"); rc=$?
  if [[ $rc -eq 0 ]]; then
    pass=$((pass+1)); note "G3_ablation_${target}: PASS (load-bearing; $res)"
  else
    fail=$((fail+1)); note "G3_ablation_${target}: FAIL ($res — construct not load-bearing?)"
  fi
done

note "G2_semantic: DELEGATED -> run_selfhost_gate.sh (fixpoint+binary) + run_selfhost_fulllang_gate.sh"
echo "dogfood_gate pass=$pass fail=$fail" | tee -a "$OUT/summary.txt"
if [[ "$fail" -gt 0 ]]; then
  echo "SELFHOST_DOGFOOD_GATE: FAIL ($pass pass / $fail fail)"; exit 1
fi
echo "SELFHOST_DOGFOOD_GATE: PASS ($pass/$pass)"
exit 0
