#!/usr/bin/env bash
# VZ slice-2 apply gate: confinement → applied argv artifact (no tart required).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
source "$ROOT/scripts/lib/gate_common.sh"
if [[ $# -gt 0 ]]; then
  OUT="$1"
  mkdir -p "$OUT"
else
  mkdir -p "$ROOT/out"
  OUT="$(mktemp -d "$ROOT/out/vz_apply_gate.XXXXXX")"
fi
OUT_LOCK="$OUT/.anubis-vz-apply.lock"
if ! mkdir "$OUT_LOCK" 2>/dev/null; then
  echo "VZ_APPLY_GATE: FAIL (output directory is already in use: $OUT)" >&2
  exit 2
fi
trap 'rmdir "$OUT_LOCK" 2>/dev/null || true' EXIT
if ! assert_clean_output_dir "$OUT" ".anubis-vz-apply.lock" "VZ apply gate"; then
  echo "VZ_APPLY_GATE: FAIL ($GATE_OUTPUT_DIR_ERROR)" >&2
  exit 2
fi
echo "vz_apply_out=$OUT"
ANUBIS="${ANUBIS_BIN:-${ANUBIS:-}}"
if [[ -z "$ANUBIS" ]]; then
  if ! scripts/publish_pin.sh --verify >"$OUT/pin_verify.log" 2>&1; then
    cat "$OUT/pin_verify.log" >&2
    echo "VZ_APPLY_GATE: FAIL (published pin does not match the live source tree)" >&2
    exit 127
  fi
  ANUBIS="$(scripts/publish_pin.sh --current)" || {
    echo "VZ_APPLY_GATE: FAIL (no ANUBIS_BIN and no published pin)" >&2
    exit 127
  }
fi
[[ -x "$ANUBIS" ]] || { echo "VZ_APPLY_GATE: FAIL ($ANUBIS is not executable)" >&2; exit 127; }
if ! ANUBIS_SHA="$(shasum -a 256 "$ANUBIS" | awk 'NR == 1 { print $1 }')" \
  || [[ ! "$ANUBIS_SHA" =~ ^[0-9a-f]{64}$ ]]; then
  echo "VZ_APPLY_GATE: FAIL (could not hash ANUBIS_BIN=$ANUBIS)" >&2
  exit 127
fi
echo "anubis_bin=$ANUBIS sha256=$ANUBIS_SHA"

DEMO=examples/showcase/vz_confine_demo.anb
python3 scripts/test_vz_apply_validator.py | tee "$OUT/validator_unit.log"
"$ANUBIS" vz confine "$DEMO" --out "$OUT/core.json"
"$ANUBIS" vz apply "$DEMO" --applied-out "$OUT/applied.json"

python3 scripts/lib/vz_apply_validate.py "$OUT/core.json" "$OUT/applied.json"

# Fail-closed: engagement mount on net-free / mount:none program must be denied.
rm -f "$OUT/should_fail.json"
if "$ANUBIS" vz apply "$DEMO" --dir 'ws:/tmp/ws' --applied-out "$OUT/should_fail.json" 2>"$OUT/mount_deny.err"; then
  echo "expected ANUBIS_APPLY_MOUNT_DENIED for mount:none + --dir" >&2
  exit 1
fi
grep -q 'ANUBIS_APPLY_MOUNT_DENIED' "$OUT/mount_deny.err" || {
  echo "mount deny missing ANUBIS_APPLY_MOUNT_DENIED:" >&2
  cat "$OUT/mount_deny.err" >&2
  exit 1
}
[[ ! -e "$OUT/should_fail.json" ]] || {
  echo "denied mount request emitted should_fail.json" >&2
  exit 1
}
echo "mount_deny_ok"

# Fail-closed: net-free + --allow-host must be denied (network dual of mounts).
rm -f "$OUT/net_should_fail.json"
if "$ANUBIS" vz apply "$DEMO" --allow-host 127.0.0.1 --applied-out "$OUT/net_should_fail.json" 2>"$OUT/net_deny.err"; then
  echo "expected ANUBIS_APPLY_NET_DENIED for host-only + --allow-host" >&2
  exit 1
fi
grep -q 'ANUBIS_APPLY_NET_DENIED' "$OUT/net_deny.err" || {
  echo "net deny missing ANUBIS_APPLY_NET_DENIED:" >&2
  cat "$OUT/net_deny.err" >&2
  exit 1
}
[[ ! -e "$OUT/net_should_fail.json" ]] || {
  echo "denied network request emitted net_should_fail.json" >&2
  exit 1
}
echo "net_deny_ok"

# Unit tests for apply + egress gateway
CARGO_TERM_COLOR=never
export CARGO_TERM_COLOR
cargo test -p anubis vz_apply 2>&1 | tee "$OUT/unit_apply.log"
cargo test -p anubis vz_egress 2>&1 | tee "$OUT/unit_egress.log"
assert_rust_tests_exercised "$OUT/unit_apply.log" "vz_apply" || {
  echo "unit apply fail: $GATE_RUST_TESTS_ERROR" >&2
  exit 1
}
echo "unit_apply_tests=$GATE_RUST_TESTS_PASSED"
assert_rust_tests_exercised "$OUT/unit_egress.log" "vz_egress" || {
  echo "unit egress fail: $GATE_RUST_TESTS_ERROR" >&2
  exit 1
}
echo "unit_egress_tests=$GATE_RUST_TESTS_PASSED"

echo "VZ_APPLY_GATE: PASS"
