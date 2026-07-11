#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE="$ROOT/examples/industry/lifeline_resilience_optimizer.anb"
OUT="${1:-$ROOT/out/lifeline_resilience/gate}"
BIN="$ROOT/target/release/anubis"

cd "$ROOT"
bash tools/grok-safety-check.sh
cargo build --release -p anubis

mkdir -p "$OUT/check" "$OUT/run" "$OUT/replay"

"$BIN" check "$SOURCE" \
  --emit ast,hir,mir \
  --evidence \
  --out "$OUT/check" \
  >"$OUT/check.log" 2>&1

BUNDLE="$(find "$OUT/check" -maxdepth 1 -type d -name 'evidence-*-safe' | sort | tail -1)"
if [[ -z "$BUNDLE" ]]; then
  echo "LIFELINE gate: check evidence bundle missing" >&2
  exit 2
fi

"$BIN" verify "$BUNDLE" >"$OUT/verify.log" 2>&1

"$BIN" run "$SOURCE" \
  --evidence \
  --json \
  --out "$OUT/run" \
  >"$OUT/run.json" 2>"$OUT/run.stderr"

"$BIN" run "$SOURCE" \
  --evidence \
  --json \
  --out "$OUT/replay" \
  >"$OUT/replay.json" 2>"$OUT/replay.stderr"

(cd "$OUT/run" && shasum -a 256 -c MANIFEST.sha256) >"$OUT/run-manifest.log"
(cd "$OUT/replay" && shasum -a 256 -c MANIFEST.sha256) >"$OUT/replay-manifest.log"
cmp "$OUT/run/stdout.txt" "$OUT/replay/stdout.txt"

rg -q '^VALIDATION model_ok=1 ' "$OUT/run/stdout.txt"
rg -q '^SEARCH method=exact_bounded_enumeration candidates=1024 feasible=134$' "$OUT/run/stdout.txt"
rg -q '^PLAN mask=611 ' "$OUT/run/stdout.txt"
rg -q '^AUDIT recompute_match=1 deterministic=1 reverse_global_optimum=1 improves_baseline=1$' "$OUT/run/stdout.txt"
rg -q '^NEGATIVE invalid_model_rejected=1 over_budget_tamper_rejected=1$' "$OUT/run/stdout.txt"
rg -q '^VERDICT=PLAN_CERTIFIED$' "$OUT/run/stdout.txt"
rg -q '^SUMMARY ok=1 ' "$OUT/run/stdout.txt"
[[ "$(tail -1 "$OUT/run/stdout.txt")" == "0" ]]

# Hostile evidence check: a copied bundle with a modified source snapshot must fail.
TAMPER="$OUT/tampered-bundle"
rm -rf "$TAMPER"
cp -R "$BUNDLE" "$TAMPER"
printf '\n// deliberate LIFELINE gate tamper\n' >>"$TAMPER/source.anubis"
set +e
"$BIN" verify "$TAMPER" >"$OUT/tamper-verify.log" 2>&1
TAMPER_RC=$?
set -e
if [[ "$TAMPER_RC" -eq 0 ]]; then
  echo "LIFELINE gate: tampered bundle incorrectly verified" >&2
  exit 3
fi

SOURCE_SHA="$(shasum -a 256 "$SOURCE" | awk '{print $1}')"
STDOUT_SHA="$(shasum -a 256 "$OUT/run/stdout.txt" | awk '{print $1}')"

{
  echo "LIFELINE_GATE=PASS"
  echo "source=$SOURCE"
  echo "source_sha256=$SOURCE_SHA"
  echo "bundle=$BUNDLE"
  echo "candidate_portfolios=1024"
  echo "feasible_portfolios=134"
  echo "selected_mask=611"
  echo "deterministic_replay=PASS"
  echo "tamper_rejection=PASS"
  echo "stdout_sha256=$STDOUT_SHA"
} | tee "$OUT/GATE_SUMMARY.txt"
