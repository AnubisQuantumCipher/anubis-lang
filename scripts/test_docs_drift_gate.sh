#!/usr/bin/env bash
# Microbench driver for scripts/run_docs_drift_gate.sh
# Proves every guard fires (FAIL on deliberate drift; PASS on truth/dated).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
OUT="${1:-out/docs_drift_test}"
mkdir -p "$OUT"

echo "=== docs drift gate self-test ==="
bash scripts/run_docs_drift_gate.sh --out "$OUT/selftest" --self-test
echo "selftest_rc=$?"

echo "=== green on current tree ==="
bash scripts/run_docs_drift_gate.sh --out "$OUT/green"
echo "green_rc=$?"

echo "=== deliberate undated drift must FAIL ==="
DRIFT="$OUT/deliberate_drift"
rm -rf "$DRIFT"
mkdir -p "$DRIFT"
# Wrong security stamp only
cat >"$DRIFT/AGENTS.md" <<'EOF'
## Current state
security **1/1** · language **244/244**
EOF
set +e
bash scripts/run_docs_drift_gate.sh --out "$OUT/red" --scan-root "$DRIFT"
red_rc=$?
set -e
echo "red_rc=$red_rc"
if [[ "$red_rc" -eq 0 ]]; then
  echo "FAIL: deliberate drift exited 0" >&2
  exit 1
fi
# Ensure report JSON says FAIL
python3 -c '
import json,sys
r=json.load(open(sys.argv[1]))
assert r["overall_verdict"]=="FAIL", r
assert r["scan_failures"]>=1
print("deliberate_drift report OK", r["scan_failures"], "failures")
' "$OUT/red/docs_drift_report.json"

echo "=== builtins LIVE (no cache file) ==="
python3 - <<'PY'
import sys
from pathlib import Path
sys.path.insert(0, "scripts/lib")
from docs_drift_derive import derive_builtins
root = Path(".").resolve()
# rename cache if present
uf = root / "scratchpad/fleet_20260726/_builtin_names_union.txt"
bak = uf.with_suffix(".txt.bak_test")
moved = False
if uf.exists():
    uf.rename(bak)
    moved = True
try:
    n, cmd = derive_builtins(root)
    assert n == 213, n
    assert "LIVE" in cmd and "_builtin_names_union" not in cmd, cmd
    print("builtins_live_ok", n)
finally:
    if moved and bak.exists():
        bak.rename(uf)
PY

echo "=== per-quantity FAIL needles (self-test already covers; re-assert log) ==="
grep -E 'SELFTEST PASS: fail_(security|language|stdlib|native|builtins|lean|doc_ok|modules)' \
  "$OUT/selftest/docs_drift_report.txt" | tee "$OUT/per_quantity_fails.txt"
test "$(wc -l < "$OUT/per_quantity_fails.txt" | tr -d ' ')" -ge 8

echo "TEST_DOCS_DRIFT_GATE: PASS"
exit 0
