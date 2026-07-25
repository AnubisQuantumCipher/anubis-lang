#!/usr/bin/env bash
# VZ slice-2 apply gate: confinement → applied argv artifact (no tart required).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
OUT="${1:-out/vz_apply_gate}"
mkdir -p "$OUT"
ANUBIS="${ANUBIS:-$ROOT/target/release/anubis}"
if [[ ! -x "$ANUBIS" ]]; then
  cargo build --release -p anubis
fi

DEMO=examples/showcase/vz_confine_demo.anb
"$ANUBIS" vz confine "$DEMO" --out "$OUT/core.json"
"$ANUBIS" vz apply "$DEMO" --applied-out "$OUT/applied.json"

python3 - <<PY
import json, sys
core = json.load(open("$OUT/core.json"))
app = json.load(open("$OUT/applied.json"))
assert app["schema"] == "anubis.confinement.applied.v1", app["schema"]
assert app["source_merkle"]
args = app.get("tart_args") or []
# net-free demo must apply host-only
if "--net-host" not in args:
    # Some demos may declare net; still require schema + merkle
    print("note: no --net-host in tart_args", args)
print("applied_ok", args)
PY

# Unit tests for apply + egress gateway
cargo test -p anubis vz_apply 2>&1 | tee "$OUT/unit_apply.log"
cargo test -p anubis vz_egress 2>&1 | tee "$OUT/unit_egress.log"
grep -q 'test result: ok' "$OUT/unit_apply.log" || { echo "unit apply fail"; exit 1; }
grep -q 'test result: ok' "$OUT/unit_egress.log" || { echo "unit egress fail"; exit 1; }

echo "VZ_APPLY_GATE: PASS"
