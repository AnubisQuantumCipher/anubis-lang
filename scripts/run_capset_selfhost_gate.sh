#!/usr/bin/env bash
# Phase-4 slice-3 capset-selfhost gate — pins the Anubis-authored WHOLE-PROGRAM capability set against
# the Rust engine's, the same 0-disagreement way run_effect_selfhost_gate.sh does for the per-fn check.
#
# Two engines, one program each:
#   (A) Rust  : `anubis vz confine <f>` — derives the confinement manifest from
#               effects.rs::program_capability_set (union of transitive rows' caps + declared fold +
#               OR of open). It REFUSES (ANUBIS_CONFINE_UNVERIFIED) for a program that does not pass
#               `anubis check` — confinement is only meaningful as a consequence of a passing check —
#               so those files are correctly SKIPPED (no ground truth to compare).
#   (B) Anubis: `anubis run selfhost/src/anubis_sh.anb -- capset <f>` — the
#               self-hosted eff_program_capset (same union, over the SH AST).
#
# ORACLE: the sorted capability SET and the `effects_bounded` (= !open) bit must be IDENTICAL. The
# whole-program capset is the natural granularity (it IS the confinement grant), so a set comparison
# is exact here — there is no per-function decomposition to mask (unlike the per-fn undeclared check).
# Secondary: both must match the fixture's `// EXPECT-CAPSET:` marker where present.
#
# This curated corpus uses only builtins the SH effect engine recognizes, so EXACT equality holds.
# Over the WHOLE SH-parseable corpus the invariant is the weaker but security-correct FAIL-CLOSED one
# (verified out-of-band): the self-hosted grant is never LESS restrictive than Rust's (never
# over-grants) — it is only more conservative (`open`→unbounded) on effect-free builtins outside SH's
# recognition (proof/symbolic/cap/poc). Closing that to whole-corpus EXACT is a completeness follow-up
# (mirror backends/run.rs::is_builtin_name).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
source "$ROOT/scripts/lib/gate_common.sh"
BIN="${ANUBIS_BIN:-./target/release/anubis}"
SH=selfhost/src/anubis_sh.anb
CORPUS="${ANUBIS_CAPSET_SELFHOST_CORPUS:-tests/fixtures/capset_selfhost}"

if [[ ! -x "$BIN" ]]; then
  echo "CAPSET_SELFHOST_GATE: FAIL (binary not executable: $BIN)"
  exit 127
fi

# ── Builtin-recognition drift-check ──────────────────────────────────────────────────────────────
# The self-hosted effect engine's `sh_is_known_builtin` MIRRORS backends/run.rs::is_builtin_name so an
# effect-free builtin never spuriously opens the row (which would over-restrict the derived
# confinement). The math/string/crypto surface (emit_builtin_call) is stable; the VOLATILE part — the
# analysis/proof/poc/cap builtins — lives in the greppable `matches!` sub-predicates. Assert every
# name in those (is_proof_input_builtin / is_poc_kit_builtin / is_non_run_builtin + the is_builtin_name
# hardcoded list) is present in sh_is_known_builtin, so a NEW such builtin added to Rust is caught here.
RUN=compiler/src/backends/run.rs
extract_fn_names() {
  awk -v fn="fn $1" 'index($0, fn){d=1} d{print} d&&/^}/{exit}' "$RUN" | grep -oE '"[a-z_][a-z0-9_]*"' | tr -d '"'
}
drift_missing=0
for cand in $( { extract_fn_names is_proof_input_builtin; extract_fn_names is_poc_kit_builtin; extract_fn_names is_non_run_builtin; extract_fn_names is_builtin_name; } | sort -u ); do
  grep -q "name == \"$cand\"" "$SH" || { echo "  DRIFT: run.rs builtin \`$cand\` is NOT in sh_is_known_builtin (anubis_sh.anb)"; drift_missing=$((drift_missing+1)); }
done
if [ "$drift_missing" -gt 0 ]; then
  echo "CAPSET_SELFHOST_GATE: FAIL ($drift_missing builtin-registry drift — mirror the new name into sh_is_known_builtin)"; exit 1
fi
echo "builtin-registry drift-check: PASS (all volatile run.rs builtins mirrored in sh_is_known_builtin)"

# Normalize a caps string (Rust human list "a, b" | Anubis "a,b," | "(none proven…)") -> sorted set.
# Normalize a capability set for comparison, and apply the SAME grant projection to BOTH sides.
#
# `vz confine`'s `capabilities` line is `capabilities_present`, a GRANT projection in which
# `fs.write` deliberately implies `fs.read` — a read-write mount posture needs both. The
# Anubis-authored engine reports the RAW proven set. Comparing a projection against a raw set made
# `c05_open_param_call` read rust={fs.read,fs.write} vs anb={fs.write} and fail the whole gate, with
# NEITHER engine wrong: the program performs no read, the fixture header expects `fs.write`, and the
# oracle was the thing comparing unlike quantities. Diagnosed by GROK-HORUS.
#
# Projecting both sides keeps the comparison honest without teaching either engine a fact it should
# not hold: the Rust fixpoint stays write-only and the SH engine stays raw. Comparing raw sets on
# both sides is NOT an option here — `research_effects` uses a different vocabulary
# (`net.connect`/`process.spawn` vs `net.send`/`shell`), so it is not the same quantity either.
norm_caps() {
  local caps
  caps=$( { tr ',' '\n' | sed 's/[[:space:]]//g' \
    | grep -vE '^$|^\(none|noneproven|maximallyconfinable' || true; } | sort -u )
  # `|| true` on EVERY grep: an unmatched grep under `set -euo pipefail` kills the gate mid-corpus
  # and prints a partial, green-looking run — the hazard this script already warns about at the
  # bracket-greps below, which I reintroduced here once and caught because c04 (the empty-capset
  # fixture) truncated the output.
  if printf '%s\n' "$caps" | grep -qx 'fs.write' 2>/dev/null; then
    caps=$(printf '%s\n%s\n' "$caps" 'fs.read')
  fi
  printf '%s\n' "$caps" | { grep -vE '^$' || true; } | sort -u | tr '\n' ','
}

agree=0; disagree=0; skip=0; anomaly=0; anomaly_rows=0; expect_ok=0; expect_bad=0; n=0
echo "== capset-selfhost differential (Anubis-authored capability set  vs  Rust vz confine) =="
for f in "$CORPUS"/*.anb; do
  [ -e "$f" ] || continue
  n=$((n+1)); name=$(basename "$f")

  rust_out=$( { "$BIN" vz confine "$f" 2>&1 || true; } )
  if printf '%s' "$rust_out" | grep -q "CONFINE_UNVERIFIED\|CONFINE_PARSE_FAILED"; then
    skip=$((skip+1)); printf "  %-38s SKIP (check-failed — vz confine refused)\n" "$name"; continue
  fi
  rust_caps_line=$(printf '%s\n' "$rust_out" | grep -m1 -E "capabilities +:" || true)
  rust_bnd_line=$(printf '%s\n' "$rust_out" | grep -m1 -E "effects_bounded +:" || true)

  # `anubis_sh.anb` has no research{}/exploit{} blocks or @research/@exploit attrs
  # -> program_mode = Mode::Safe -> this `run` needs no --allow-research. Passing it
  # would now FAIL on host with ANUBIS_RESEARCH_HOST_FORBIDDEN (commit 5fb7b67 made
  # `run --allow-research` VZ-guest-only). See scripts/run_selfhost_gate.sh.
  anb_out=$( { "$BIN" run "$SH" -- capset "$f" 2>&1 || true; } )
  anb_capset_line=$(printf '%s\n' "$anb_out" | grep -m1 "CAPSET" || true)

  row_anomaly=0
  if [[ -z "$rust_caps_line" ]]; then
    echo "  $name ANOMALY (missing Rust capabilities line)"
    anomaly=$((anomaly+1)); row_anomaly=1
  fi
  if [[ -z "$rust_bnd_line" ]]; then
    echo "  $name ANOMALY (missing Rust effects_bounded line)"
    anomaly=$((anomaly+1)); row_anomaly=1
  fi
  if [[ -z "$anb_capset_line" || "$anb_capset_line" != *"caps="* ]]; then
    echo "  $name ANOMALY (missing Anubis CAPSET caps field)"
    anomaly=$((anomaly+1)); row_anomaly=1
  fi
  if [[ -z "$anb_capset_line" || "$anb_capset_line" != *"bounded="* ]]; then
    echo "  $name ANOMALY (missing Anubis CAPSET bounded field)"
    anomaly=$((anomaly+1)); row_anomaly=1
  fi
  if [[ $row_anomaly -ne 0 ]]; then
    anomaly_rows=$((anomaly_rows+1))
    continue
  fi

  rust_caps=$(printf '%s\n' "$rust_caps_line" | sed 's/.*: *//' | norm_caps)
  rust_bnd=$(printf '%s\n' "$rust_bnd_line" | sed 's/.*: *//' | tr -d '[:space:]')
  anb_caps=$(printf '%s\n' "$anb_capset_line" | sed 's/.*caps=//; s/ *bounded=.*//' | norm_caps)
  anb_bnd=$(printf '%s\n' "$anb_capset_line" | sed 's/.*bounded=//' | tr -d '[:space:]')

  if [ "$rust_caps" = "$anb_caps" ] && [ "$rust_bnd" = "$anb_bnd" ]; then
    agree=$((agree+1)); verdict="AGREE"
  else
    disagree=$((disagree+1)); verdict="*** DISAGREE ***"
  fi

  # secondary: EXPECT-CAPSET marker (caps set + bounded)
  em=""
  if grep -q "EXPECT-CAPSET:" "$f"; then
    exp_caps=$( { grep -m1 "EXPECT-CAPSET:" "$f" || true; } | sed 's|.*EXPECT-CAPSET: *||; s/ *bounded=.*//' | norm_caps)
    exp_bnd=$( { grep -m1 "EXPECT-CAPSET:" "$f" || true; } | sed 's|.*bounded=||' | tr -d '[:space:]')
    if [ "$exp_caps" = "$anb_caps" ] && [ "$exp_bnd" = "$anb_bnd" ]; then expect_ok=$((expect_ok+1)); em="marker-ok"; else expect_bad=$((expect_bad+1)); em="MARKER-MISMATCH(exp {$exp_caps}/$exp_bnd)"; fi
  fi

  printf "  %-38s rust={%s}/%s anb={%s}/%s  %s  %s\n" "$name" "$rust_caps" "$rust_bnd" "$anb_caps" "$anb_bnd" "$verdict" "$em"
done

echo ""
echo "CAPSET_SELFHOST over $n fixtures: AGREE=$agree DISAGREE=$disagree SKIP=$skip ANOMALY=$anomaly | marker ok=$expect_ok bad=$expect_bad"
set +e
finalize "$n" "$((agree + skip))" "$((disagree + anomaly_rows))" 0

# Coverage ratchet. `finalize` proves the fixtures that RAN agreed; it cannot notice that fewer
# ran than last time. A self-host comparison over a shrinking corpus reports "0 disagreements"
# with perfect confidence about less and less.
assert_floor "capset_selfhost" "$n" "$ROOT/.gate_floors/capset_selfhost.floor"
_floor_rc=$?
if [[ $_floor_rc -ne 0 ]]; then
  echo "CAPSET_SELFHOST_GATE: FAIL ($GATE_FLOOR_ERROR)" >&2
  exit 1
fi
final_rc=$?
set -e
if [[ "$final_rc" -ne 0 ]]; then
  echo "CAPSET_SELFHOST_GATE: FAIL"; exit 1
fi
if [[ "$agree" -eq 0 ]]; then
  echo "CAPSET_SELFHOST_GATE: FAIL (zero productive comparisons; all SKIP/ANOMALY is hollow)"; exit 1
fi
if [ "$expect_bad" -gt 0 ] || [ "$anomaly" -gt 0 ]; then
  echo "CAPSET_SELFHOST_GATE: FAIL"; exit 1
fi
echo "CAPSET_SELFHOST_GATE: PASS (0 disagreements; Anubis-authored capability set == Rust program_capability_set)"
exit 0
