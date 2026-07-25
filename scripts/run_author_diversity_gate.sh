#!/usr/bin/env bash
# Author-diversity architecture lane (not TT-total).
# Compiles the independent table-driven scanner with a non-LLVM CC and checks
# it tokenizes a fixture corpus without crashing; residual same-human authorship
# is recorded in the summary (never claimed closed).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
OUT="${1:-out/author_diversity_gate}"
mkdir -p "$OUT"

pick_cc() {
  if [[ -n "${ANUBIS_DDC_CC:-}" ]]; then echo "$ANUBIS_DDC_CC"; return; fi
  for c in gcc-15 gcc-14 gcc-13 gcc-12 tcc; do
    if command -v "$c" >/dev/null 2>&1; then echo "$c"; return; fi
  done
  # On macOS, clang is LLVM — refuse for diversity unless forced.
  if [[ -n "${ANUBIS_AUTHOR_DIVERSITY_ALLOW_CLANG:-}" ]] && command -v clang >/dev/null; then
    echo clang; return
  fi
  echo ""
}
CC="$(pick_cc)"
if [[ -z "$CC" ]]; then
  echo "AUTHOR_DIVERSITY_GATE: FAIL (no non-LLVM CC; install gcc/tcc or set ANUBIS_DDC_CC)"
  exit 1
fi
if "$CC" --version 2>/dev/null | head -1 | grep -qi clang; then
  if [[ -z "${ANUBIS_AUTHOR_DIVERSITY_ALLOW_CLANG:-}" ]]; then
    echo "AUTHOR_DIVERSITY_GATE: FAIL (CC is clang/LLVM; need gcc/tcc for diversity)"
    exit 1
  fi
fi

SRC=selfhost/backend_independent/token_scan.c
BIN="$OUT/token_scan"
"$CC" -O2 -std=c11 -o "$BIN" "$SRC"
pass=0
fail=0
for f in \
  selfhost/src/anubis_sh.anb \
  examples/hello.anb \
  examples/hello_normal.anb \
  examples/showcase/vz_confine_demo.anb
do
  [[ -f "$f" ]] || continue
  if "$BIN" "$f" >"$OUT/$(basename "$f").tok" 2>"$OUT/$(basename "$f").err"; then
    if grep -q '^EOF$' "$OUT/$(basename "$f").tok"; then
      pass=$((pass + 1))
    else
      fail=$((fail + 1))
      echo "missing EOF: $f"
    fi
  else
    fail=$((fail + 1))
    echo "scan failed: $f"
  fi
done

{
  echo "cc=$CC"
  echo "pass=$pass fail=$fail"
  echo "architecture_lane=table_driven_token_scan"
  echo "tt_total=NOT_CLAIMED (same-human residual)"
} | tee "$OUT/summary.txt"

if [[ "$fail" -ne 0 || "$pass" -lt 2 ]]; then
  echo "AUTHOR_DIVERSITY_GATE: FAIL"
  exit 1
fi
echo "AUTHOR_DIVERSITY_GATE: PASS (architecture lane; same-human residual remains for TT-total)"
