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
#                  (over-grant) direction.
#   ANOMALY      — expected format lines missing (panic, format drift). Fail-closed: counts as FAIL.
#   SKIP         — parse/confine unverified (honest skip).
#
# The invariant: DISAGREE == 0 AND ANOMALY == 0 AND (OK+CONSERVATIVE+SKIP+DISAGREE+ANOMALY) > 0.
# Silent corpus shrink is forbidden: xargs failures and empty RAW are FAIL, not PASS.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
# Resolved in --one (must be pre-set by main or env) and again in main.
BIN="${ANUBIS_BIN:-./target/release/anubis}"; SH=selfhost/src/anubis_sh.anb

norm() { { tr ',' '\n' | sed 's/[[:space:]]//g' | grep -vE '^$|^\(none|noneproven|maximallyconfinable' || true; } | sort -u; }

# Per-file mode: re-invoked as `bash "$0" --one <file>` by xargs (robust across shells — no export -f).
if [ "${1:-}" = "--one" ]; then
  f="$2"
  rust_out=$( { "$BIN" vz confine "$f" 2>&1 || true; } )
  if printf '%s' "$rust_out" | grep -q "CONFINE_UNVERIFIED\|CONFINE_PARSE_FAILED"; then echo "SKIP $f"; exit 0; fi
  # `anubis_sh.anb` has no research{}/exploit{} blocks or @research/@exploit attrs
  # -> program_mode = Mode::Safe -> this `run` needs no --allow-research.
  anb_out=$( { "$BIN" run "$SH" -- capset "$f" 2>&1 || true; } )
  if printf '%s' "$anb_out" | grep -q "PARSE_ERROR"; then echo "SKIP $f"; exit 0; fi

  # Bracket greps: under set -euo pipefail an unmatched grep -m1 kills the child before any
  # OK/CONSERVATIVE/DISAGREE line is printed. Combined with main's historical `|| true` on xargs,
  # that was a silent fail-open (PASS over a shrunken corpus). Emit ANOMALY instead.
  set +e
  cap_line=$(printf '%s\n' "$rust_out" | grep -m1 -E "capabilities +:")
  bound_line=$(printf '%s\n' "$rust_out" | grep -m1 -E "effects_bounded +:")
  capset_line=$(printf '%s\n' "$anb_out" | grep -m1 CAPSET)
  set -e
  if [ -z "${cap_line:-}" ] || [ -z "${bound_line:-}" ]; then
    echo "ANOMALY $f missing-rust-confine-format"
    exit 0
  fi
  if [ -z "${capset_line:-}" ]; then
    echo "ANOMALY $f missing-sh-capset-format"
    exit 0
  fi

  rc=$(printf '%s\n' "$cap_line" | sed 's/.*: *//' | norm)
  rb=$(printf '%s\n' "$bound_line" | sed 's/.*: *//' | tr -d '[:space:]')
  ac=$(printf '%s\n' "$capset_line" | sed 's/.*caps=//; s/ *bounded=.*//' | norm)
  ab=$(printf '%s\n' "$capset_line" | sed 's/.*bounded=//' | tr -d '[:space:]')
  extra=$(comm -23 <(printf '%s\n' "$ac") <(printf '%s\n' "$rc") | grep -v '^$' || true)
  rcj=$(printf '%s' "$rc" | tr '\n' ','); acj=$(printf '%s' "$ac" | tr '\n' ',')
  if [ "$ac" = "$rc" ] && [ "$ab" = "$rb" ]; then echo "OK $f"
  elif [ -z "$extra" ] && ! { [ "$ab" = "true" ] && [ "$rb" = "false" ]; }; then echo "CONSERVATIVE $f rust={$rcj}/$rb anb={$acj}/$ab"
  else echo "DISAGREE $f rust={$rcj}/$rb anb={$acj}/$ab"; fi
  exit 0
fi

# Main mode: build once (only when ANUBIS_BIN unset), fan out per-file over the corpus.
if [[ -n "${ANUBIS_BIN:-}" ]]; then
  BIN="$ANUBIS_BIN"
  if [[ ! -x "$BIN" ]]; then
    echo "CAPSET_CORPUS_FAILCLOSED_GATE: FAIL (ANUBIS_BIN=$BIN not executable)" >&2
    exit 127
  fi
else
  BIN=./target/release/anubis
  if [[ ! -x "$BIN" ]]; then
    cargo build -q --release -p anubis
  fi
  if [[ ! -x "$BIN" ]]; then
    echo "CAPSET_CORPUS_FAILCLOSED_GATE: FAIL (no anubis binary at $BIN)" >&2
    exit 127
  fi
fi

RAW=$(mktemp)
ERR=$(mktemp)
LIST=$(mktemp)
trap 'rm -f "$RAW" "$ERR" "$LIST"' EXIT

# Count on-disk corpus first — scored must equal this (truncation / silent shrink).
find examples tests/fixtures selfhost/src -name '*.anb' | sort >"$LIST"
expected=$(wc -l <"$LIST" | tr -d ' ')
if [ "${expected:-0}" -eq 0 ]; then
  echo "CAPSET_CORPUS_FAILCLOSED_GATE: FAIL (empty corpus under examples|tests/fixtures|selfhost/src)" >&2
  exit 1
fi

set +e
xargs -P 10 -I{} bash "$0" --one {} <"$LIST" >"$RAW" 2>"$ERR"
xargs_rc=$?
set -e

ok=$(grep -c '^OK' "$RAW" || true)
cons=$(grep -c '^CONSERVATIVE' "$RAW" || true)
skip=$(grep -c '^SKIP' "$RAW" || true)
dis=$(grep -c '^DISAGREE' "$RAW" || true)
anom=$(grep -c '^ANOMALY' "$RAW" || true)
scored=$((ok + cons + skip + dis + anom))

grep '^DISAGREE' "$RAW" || true
grep '^ANOMALY' "$RAW" || true
[ "${STRICT:-0}" = 1 ] && grep '^CONSERVATIVE' "$RAW" || true
echo "CAPSET_CORPUS_FAILCLOSED: OK=$ok CONSERVATIVE=$cons SKIP=$skip DISAGREE=$dis ANOMALY=$anom scored=$scored expected=$expected xargs_rc=$xargs_rc"

if [ "$scored" -eq 0 ]; then
  echo "CAPSET_CORPUS_FAILCLOSED_GATE: FAIL (zero scored lines — empty corpus or all children died silently)" >&2
  if [ -s "$ERR" ]; then echo "--- xargs stderr ---"; cat "$ERR"; fi
  exit 1
fi
# Truncation / silent shrink (Seshat R8): every on-disk file must produce one classification line.
if [ "$scored" -lt "$expected" ]; then
  echo "CAPSET_CORPUS_FAILCLOSED_GATE: FAIL (TRUNCATED_RUN: scored=$scored < expected=$expected — mid-run death/skip shrinks denominator)" >&2
  if [ -s "$ERR" ]; then echo "--- xargs stderr ---"; head -50 "$ERR"; fi
  exit 1
fi
if [ "$scored" -gt "$expected" ]; then
  echo "CAPSET_CORPUS_FAILCLOSED_GATE: FAIL (corpus_count_surplus: scored=$scored > expected=$expected)" >&2
  exit 1
fi
# xargs exits 123 if any child nonzero. Our --one paths always exit 0 by design; nonzero means
# a real crash outside the classifier — fail closed rather than PASS on a partial RAW.
if [ "$xargs_rc" -ne 0 ]; then
  echo "CAPSET_CORPUS_FAILCLOSED_GATE: FAIL (xargs rc=$xargs_rc — at least one child died without a classification line)" >&2
  if [ -s "$ERR" ]; then echo "--- xargs stderr ---"; head -50 "$ERR"; fi
  exit 1
fi
if [ "$dis" -gt 0 ]; then
  echo "CAPSET_CORPUS_FAILCLOSED_GATE: FAIL ($dis over-grant(s) — self-hosted capset less restrictive than Rust)"
  exit 1
fi
if [ "$anom" -gt 0 ]; then
  echo "CAPSET_CORPUS_FAILCLOSED_GATE: FAIL ($anom format anomaly/ies — panics or drift would previously shrink the denominator silently)"
  exit 1
fi
# All-SKIP is hollow: no real OK/CONSERVATIVE comparison ran (broken BIN → every SKIP → green).
productive=$((ok + cons))
if [ "$productive" -eq 0 ]; then
  echo "CAPSET_CORPUS_FAILCLOSED_GATE: FAIL (zero productive OK+CONSERVATIVE — all SKIP/empty is hollow PASS)" >&2
  exit 1
fi
if [ "${STRICT:-0}" = 1 ] && [ "$cons" -gt 0 ]; then
  echo "CAPSET_CORPUS_FAILCLOSED_GATE: FAIL (STRICT: $cons CONSERVATIVE — a Rust builtin is unrecognized by sh_is_known_builtin)"
  exit 1
fi
echo "CAPSET_CORPUS_FAILCLOSED_GATE: PASS (0 over-grants; 0 anomalies; scored=$scored/$expected productive=$productive${cons:+; $cons conservative}${STRICT:+ / STRICT exact})"
exit 0
