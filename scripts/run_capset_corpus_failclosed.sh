#!/usr/bin/env bash
# Phase-4 slice-3 whole-corpus FAIL-CLOSED capset check (drift-check for is_builtin_name recognition;
# NOT in the VM battery — it runs `anubis vz confine` + the self-hosted `capset` over every
# SH-parseable file, which is slow). Complements the curated EXACT gate run_capset_selfhost_gate.sh.
#
# A confinement grant is SAFE as long as the self-hosted engine never grants MORE than Rust. For each
# check-passing SH-parseable file it classifies:
#   OK           — Anubis capset + bounded EXACTLY match Rust `anubis vz confine`.
#   CONSERVATIVE — Anubis grants a strict subset / is more restrictive (open where Rust is bounded).
#                  The SAFE direction — e.g. `open` on an effect-free builtin the SH engine does not
#                  yet recognize (proof/symbolic/cap/poc; is_builtin_name has ~116 names, SH has 24).
#   DISAGREE     — Anubis grants a cap Rust doesn't, OR is bounded where Rust is open. The real-bug
#                  (over-grant) direction. This is the ONLY failure.
# The invariant: DISAGREE == 0 (the self-hosted grant is never less restrictive than Rust's). When
# task #106 mirrors is_builtin_name, CONSERVATIVE should fall to 0 too and this can tighten to EXACT.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
BIN=./target/release/anubis; SH=selfhost/src/anubis_sh.anb

norm() { { tr ',' '\n' | sed 's/[[:space:]]//g' | grep -vE '^$|^\(none|noneproven|maximallyconfinable' || true; } | sort -u; }

# Per-file mode: re-invoked as `bash "$0" --one <file>` by xargs (robust across shells — no export -f).
if [ "${1:-}" = "--one" ]; then
  f="$2"
  rust_out=$( { "$BIN" vz confine "$f" 2>&1 || true; } )
  if printf '%s' "$rust_out" | grep -q "CONFINE_UNVERIFIED\|CONFINE_PARSE_FAILED"; then echo "SKIP $f"; exit 0; fi
  anb_out=$( { "$BIN" run "$SH" --allow-research -- capset "$f" 2>&1 || true; } )
  if printf '%s' "$anb_out" | grep -q "PARSE_ERROR"; then echo "SKIP $f"; exit 0; fi
  rc=$(printf '%s\n' "$rust_out" | grep -m1 -E "capabilities +:" | sed 's/.*: *//' | norm)
  rb=$(printf '%s\n' "$rust_out" | grep -m1 -E "effects_bounded +:" | sed 's/.*: *//' | tr -d '[:space:]')
  ac=$(printf '%s\n' "$anb_out" | grep -m1 CAPSET | sed 's/.*caps=//; s/ *bounded=.*//' | norm)
  ab=$(printf '%s\n' "$anb_out" | grep -m1 CAPSET | sed 's/.*bounded=//' | tr -d '[:space:]')
  extra=$(comm -23 <(printf '%s\n' "$ac") <(printf '%s\n' "$rc") | grep -v '^$' || true)
  rcj=$(printf '%s' "$rc" | tr '\n' ','); acj=$(printf '%s' "$ac" | tr '\n' ',')
  if [ "$ac" = "$rc" ] && [ "$ab" = "$rb" ]; then echo "OK $f"
  elif [ -z "$extra" ] && ! { [ "$ab" = "true" ] && [ "$rb" = "false" ]; }; then echo "CONSERVATIVE $f rust={$rcj}/$rb anb={$acj}/$ab"
  else echo "DISAGREE $f rust={$rcj}/$rb anb={$acj}/$ab"; fi
  exit 0
fi

# Main mode: build once, fan out per-file over the corpus.
cargo build -q --release -p anubis
RAW=$(mktemp)
find examples tests/fixtures selfhost/src -name '*.anb' | sort \
  | xargs -P 10 -I{} bash "$0" --one {} > "$RAW" 2>/dev/null || true

ok=$(grep -c '^OK' "$RAW" || true); cons=$(grep -c '^CONSERVATIVE' "$RAW" || true)
skip=$(grep -c '^SKIP' "$RAW" || true); dis=$(grep -c '^DISAGREE' "$RAW" || true)
grep '^DISAGREE' "$RAW" || true
echo "CAPSET_CORPUS_FAILCLOSED: OK=$ok CONSERVATIVE=$cons SKIP=$skip DISAGREE=$dis"
rm -f "$RAW"
if [ "$dis" -gt 0 ]; then
  echo "CAPSET_CORPUS_FAILCLOSED_GATE: FAIL ($dis over-grant(s) — self-hosted capset less restrictive than Rust)"; exit 1
fi
echo "CAPSET_CORPUS_FAILCLOSED_GATE: PASS (0 over-grants; self-hosted grant never less restrictive than Rust)"
exit 0
