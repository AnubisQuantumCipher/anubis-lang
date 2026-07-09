#!/usr/bin/env bash
# Turing-core gate: proves the Anubis language actually COMPUTES.
# Each fixture in tests/fixtures/turing_core/<name>.anb is executed with `anubis run`
# and its stdout is compared byte-for-byte against <name>.expected.
#
# HONESTY CONTRACT (no false-green):
#   - A fixture PASSES only if `anubis run` exits 0 AND stdout == expected exactly.
#   - A missing binary, missing .expected, nonzero exit, or any mismatch => FAIL.
#   - The verdict is derived from the comparison; it never defaults to PASS.
set -uo pipefail

OUT="out/turing_core"
while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    *) shift ;;
  esac
done

REPO="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO"
FIXDIR="tests/fixtures/turing_core"
mkdir -p "$OUT"

# Prefer the release binary (what release/A15 gates use); fall back to debug.
if [ -x target/release/anubis ]; then BIN="target/release/anubis"
elif [ -x target/debug/anubis ]; then BIN="target/debug/anubis"
else echo "FAIL: no anubis binary (build with cargo build --release -p anubis)"; exit 1; fi

pass=0; fail=0; total=0
report="$OUT/report.json"
echo "{" > "$report"
echo "  \"binary\": \"$BIN\"," >> "$report"
echo "  \"fixtures\": [" >> "$report"
first=1

for anb in "$FIXDIR"/*.anb; do
  name="$(basename "$anb" .anb)"
  exp="$FIXDIR/$name.expected"
  total=$((total+1))
  status="FAIL"; detail=""
  if [ ! -f "$exp" ]; then
    detail="missing .expected"
  else
    actual="$("$BIN" run "$anb" --out "$OUT/run_$name" 2>"$OUT/$name.stderr")"
    rc=$?
    expected="$(cat "$exp")"
    if [ $rc -ne 0 ]; then
      detail="run exit $rc: $(head -1 "$OUT/$name.stderr")"
    elif [ "$actual" = "$expected" ]; then
      status="PASS"; detail="stdout matches expected"
    else
      detail="stdout mismatch: got [$actual] want [$expected]"
    fi
  fi
  if [ "$status" = "PASS" ]; then pass=$((pass+1)); else fail=$((fail+1)); fi
  [ $first -eq 1 ] && first=0 || echo "," >> "$report"
  printf '    {"name": "%s", "status": "%s", "detail": %s}' \
    "$name" "$status" "$(python3 -c 'import json,sys; print(json.dumps(sys.argv[1]))' "$detail")" >> "$report"
  printf '%-24s %s  (%s)\n' "$name" "$status" "$detail"
done

verdict="FAIL"; [ $fail -eq 0 ] && [ $pass -gt 0 ] && verdict="PASS"
echo "" >> "$report"
echo "  ]," >> "$report"
echo "  \"total\": $total, \"passed\": $pass, \"failed\": $fail," >> "$report"
echo "  \"overall_verdict\": \"$verdict\"" >> "$report"
echo "}" >> "$report"

echo "Report: $report"
echo "Overall: $verdict ($pass/$total)"
[ "$verdict" = "PASS" ]
