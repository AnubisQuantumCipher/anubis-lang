#!/usr/bin/env bash
set -euo pipefail
# Reproducibility for ordinary language checks (Gate 12 partial)
# Runs same fixture twice, compares source hash + summary (isolates timestamps)

OUT_DIR="${1:-out/a_plus_gate2_repro}"
if [[ "${1:-}" == "--out" ]]; then OUT_DIR="$2"; fi
mkdir -p "$OUT_DIR"
FIXTURES="tests/fixtures/language_core"

report="$OUT_DIR/repro_report.json"
echo '{"runs": [], "overall_verdict": "PENDING"}' > "$report"

ok=1
for f in "$FIXTURES"/*.anb; do
  base=$(basename "$f" .anb)
  echo "repro $base"
  d1="$OUT_DIR/run1_$base"; mkdir -p "$d1"
  d2="$OUT_DIR/run2_$base"; mkdir -p "$d2"
  cargo run -- check "$f" --evidence --out "$d1" > /dev/null 2>&1 || true
  sleep 1
  cargo run -- check "$f" --evidence --out "$d2" > /dev/null 2>&1 || true

  h1=$(sha256sum "$f" | awk '{print $1}')
  # Reproducibility = the tool's DETERMINISTIC semantic output is identical across two runs.
  # We extract only the timestamp-free fields (verdict + check_error) with jq and compare them.
  # Missing output from either run (tool failed to write a summary) is a FAIL, not a pass.
  s1=$(jq -S -c '{verdict, check_error}' "$d1/check-summary.json" 2>/dev/null || echo "MISSING_1")
  s2=$(jq -S -c '{verdict, check_error}' "$d2/check-summary.json" 2>/dev/null || echo "MISSING_2")

  match="true"
  if [[ "$s1" == "MISSING_1" || "$s2" == "MISSING_2" || "$s1" != "$s2" ]]; then
    match="false"; ok=0
  fi

  jq --arg b "$base" --arg m "$match" --arg h "$h1" --arg s1 "$s1" --arg s2 "$s2" \
     '.runs += [{"fixture":$b, "source_hash":$h, "summary1":$s1, "summary2":$s2, "match":$m}]' "$report" > "$report.tmp" && mv "$report.tmp" "$report"
done

if [[ $ok -eq 1 ]]; then
  overall="PASS"
else
  overall="FAIL"
fi
jq --arg o "$overall" '.overall_verdict = $o' "$report" > "$report.tmp" && mv "$report.tmp" "$report"
cat "$report"
[[ "$overall" == "PASS" ]] || exit 1
