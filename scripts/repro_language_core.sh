#!/usr/bin/env bash
set -euo pipefail
# Reproducibility for ordinary language checks (Gate 12 partial)
# Runs same fixture twice, compares source hash + summary (isolates timestamps)

OUT_DIR="${1:-out/a_plus_gate2_repro}"
if [[ "${1:-}" == "--out" ]]; then OUT_DIR="$2"; fi
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
source "$ROOT/scripts/lib/gate_common.sh"
mkdir -p "$OUT_DIR"
FIXTURES="${ANUBIS_REPRO_LANGUAGE_CORPUS:-tests/fixtures/language_core}"
BIN="${ANUBIS_BIN:-./target/release/anubis}"
[[ -x "$BIN" ]] || { echo "REPRO_LANGUAGE_CORE: FAIL (binary not executable: $BIN)" >&2; exit 127; }

report="$OUT_DIR/repro_report.json"
echo '{"runs": [], "overall_verdict": "PENDING"}' > "$report"

passed=0; failed=0; total=0
shopt -s nullglob
fixtures=( "$FIXTURES"/*.anb )
shopt -u nullglob
if ! require_nonempty_corpus "${#fixtures[@]}" "$FIXTURES/*.anb"; then
  jq '.overall_verdict = "FAIL" | .total = 0 | .passed = 0 | .failed = 0' "$report" > "$report.tmp" && mv "$report.tmp" "$report"
  echo "REPRO_LANGUAGE_CORE: FAIL"
  exit 1
fi

for f in "${fixtures[@]}"; do
  total=$((total+1))
  base=$(basename "$f" .anb)
  echo "repro $base"
  d1="$OUT_DIR/run1_$base"; mkdir -p "$d1"
  d2="$OUT_DIR/run2_$base"; mkdir -p "$d2"
  "$BIN" check "$f" --evidence --out "$d1" > /dev/null 2>&1 || true
  sleep 1
  "$BIN" check "$f" --evidence --out "$d2" > /dev/null 2>&1 || true

  h1=$(sha256sum "$f" | awk '{print $1}')
  # Reproducibility = the tool's DETERMINISTIC semantic output is identical across two runs.
  # We extract only the timestamp-free fields (verdict + check_error) with jq and compare them.
  # Missing output from either run (tool failed to write a summary) is a FAIL, not a pass.
  s1=$(jq -S -c '{verdict, check_error}' "$d1/check-summary.json" 2>/dev/null || echo "MISSING_1")
  s2=$(jq -S -c '{verdict, check_error}' "$d2/check-summary.json" 2>/dev/null || echo "MISSING_2")

  match="true"; actual="PASS"
  if [[ "$s1" == "MISSING_1" || "$s2" == "MISSING_2" || "$s1" != "$s2" ]]; then
    match="false"; actual="FAIL"
  fi
  if score_fixture PASS "$actual"; then passed=$((passed+1)); else failed=$((failed+1)); fi

  jq --arg b "$base" --arg m "$match" --arg h "$h1" --arg s1 "$s1" --arg s2 "$s2" \
     '.runs += [{"fixture":$b, "source_hash":$h, "summary1":$s1, "summary2":$s2, "match":$m}]' "$report" > "$report.tmp" && mv "$report.tmp" "$report"
done

set +e
finalize "$total" "$passed" "$failed" 0
final_rc=$?
set -e
overall="$GATE_FINAL_STATUS"; [[ "$overall" == PASS ]] || overall=FAIL
jq --arg o "$overall" --argjson t "$total" --argjson p "$passed" --argjson f "$failed" \
  '.overall_verdict = $o | .total = $t | .passed = $p | .failed = $f' "$report" > "$report.tmp" && mv "$report.tmp" "$report"
cat "$report"
[[ "$overall" == "PASS" ]] || exit 1
