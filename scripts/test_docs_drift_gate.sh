#!/usr/bin/env bash
# Microbench driver for scripts/run_docs_drift_gate.sh
# Proves every guard fires (FAIL on deliberate drift; PASS on truth/dated).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
if [[ $# -gt 0 ]]; then
  OUT="$1"
else
  OUT="$(mktemp -d "${TMPDIR:-/tmp}/anubis-docs-drift-test.XXXXXX")"
  trap 'rm -rf "$OUT"' EXIT
fi
mkdir -p "$OUT"

echo "=== docs drift gate self-test ==="
bash scripts/run_docs_drift_gate.sh --out "$OUT/selftest" --self-test
echo "selftest_rc=$?"

echo "=== green on current tree ==="
bash scripts/run_docs_drift_gate.sh --out "$OUT/green"
echo "green_rc=$?"

echo "=== output path with a quote remains data, not Python source ==="
QUOTED_OUT="$OUT/quoted-'path"
bash scripts/run_docs_drift_gate.sh --out "$QUOTED_OUT"
test -f "$QUOTED_OUT/docs_drift_report.json"

echo "=== stale output directory refusal ==="
mkdir -p "$OUT/nonempty"
printf 'stale\n' >"$OUT/nonempty/stale.txt"
set +e
bash scripts/run_docs_drift_gate.sh --out "$OUT/nonempty" >/dev/null 2>&1
nonempty_rc=$?
set -e
test "$nonempty_rc" -ne 0
test ! -f "$OUT/nonempty/docs_drift_report.json"

for poison in missing invalid; do
  echo "=== $poison scan JSON refusal ==="
  set +e
  ANUBIS_TEST_ONLY_DOCS_DRIFT_SCAN_POISON="$poison" \
    bash scripts/run_docs_drift_gate.sh --out "$OUT/scan-$poison" >/dev/null 2>&1
  poison_rc=$?
  set -e
  test "$poison_rc" -ne 0
  test ! -f "$OUT/scan-$poison/docs_drift_report.json"
done

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
if r.get("overall_verdict") != "FAIL":
    raise SystemExit(f"expected FAIL verdict: {r}")
if not isinstance(r.get("scan_failures"), int) or r["scan_failures"] < 1:
    raise SystemExit(f"expected nonzero scan failures: {r}")
print("deliberate_drift report OK", r["scan_failures"], "failures")
' "$OUT/red/docs_drift_report.json"

echo "=== missing canonical owned doc must FAIL in strict mode ==="
MISSING="$OUT/missing_owned"
rm -rf "$MISSING"
mkdir -p "$MISSING"
printf 'fixture root\n' >"$MISSING/AGENTS.md"
set +e
python3 scripts/lib/docs_drift_scan.py \
  "$MISSING" "$OUT/green/derived.json" --require-owned-files >"$OUT/missing_owned.json"
missing_rc=$?
set -e
if [[ "$missing_rc" -eq 0 ]]; then
  echo "FAIL: strict owned-doc scan accepted a sparse root" >&2
  exit 1
fi
python3 -c '
import json,sys
r=json.load(open(sys.argv[1]))
if not any(x.startswith("MISSING_OWNED_DOC ") for x in r.get("failures", [])):
    raise SystemExit(f"expected missing-owned-doc failure: {r}")
print("missing_owned report OK", r["scan_failures"], "failures")
' "$OUT/missing_owned.json"

echo "=== builtins LIVE (no cache file) ==="
python3 - <<'PY'
import shutil
import sys
import tempfile
from pathlib import Path
sys.path.insert(0, "scripts/lib")
from docs_drift_derive import derive_builtins
root = Path(".").resolve()
# A scratch root containing a deliberately false cache proves the derivation
# ignores cache files without renaming or otherwise mutating the repository.
with tempfile.TemporaryDirectory(prefix="anubis-docs-builtins-") as td:
    fixture = Path(td)
    source = fixture / "compiler/src/backends/run.rs"
    source.parent.mkdir(parents=True)
    shutil.copy2(root / "compiler/src/backends/run.rs", source)
    cache = fixture / "scratchpad/fleet_20260726/_builtin_names_union.txt"
    cache.parent.mkdir(parents=True)
    cache.write_text("deliberately-wrong-cache-entry\n")
    n, cmd = derive_builtins(fixture)
    if n != 213:
        raise SystemExit(f"expected 213 builtins, got {n}")
    if "LIVE" not in cmd or "_builtin_names_union" in cmd:
        raise SystemExit(f"expected live derivation command, got {cmd}")
    print("builtins_live_ok", n)
PY

echo "=== per-quantity FAIL needles (self-test already covers; re-assert log) ==="
grep -E 'SELFTEST PASS: fail_(security|language|stdlib|native|builtins|lean|doc_ok|modules)' \
  "$OUT/selftest/docs_drift_report.txt" | tee "$OUT/per_quantity_fails.txt"
test "$(wc -l < "$OUT/per_quantity_fails.txt" | tr -d ' ')" -ge 8

echo "TEST_DOCS_DRIFT_GATE: PASS"
exit 0
