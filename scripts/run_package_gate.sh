#!/usr/bin/env bash
# Phase-6: package manager + proof-carrying dependencies gate.
# Fail-closed: unit tests + CLI lock/verify/check/run with signed path dep.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
source "$ROOT/scripts/lib/gate_common.sh"
OUT="${1:-out/package_gate}"
mkdir -p "$OUT"

pass=0
fail=0
detail=()
note() { detail+=("$1"); }

BIN=./target/release/anubis
cargo build -q --release -p anubis

# 1) Unit: phase6 + package modules
if cargo test -p anubis-compiler --lib phase6_ -- --test-threads=4 >"$OUT/unit_phase6.log" 2>&1 \
  && cargo test -p anubis-compiler --lib package:: -- --test-threads=4 >>"$OUT/unit_phase6.log" 2>&1; then
  pass=$((pass+1))
  note "unit_phase6: PASS"
else
  fail=$((fail+1))
  note "unit_phase6: FAIL (see $OUT/unit_phase6.log)"
fi

# 2) End-to-end path dep: keygen → evidence → sign → trust → lock → verify → check → run
FIX="$OUT/fixture"
rm -rf "$FIX"
mkdir -p "$FIX/math_lib/src" "$FIX/app" "$FIX/keys"
# Isolate ~/.anubis trust/cache, but keep rustup/cargo visible to native build/run.
REAL_HOME="${HOME:-/Users/sicarii}"
export HOME="$FIX/home"
mkdir -p "$HOME/.anubis/trust"
ln -sfn "$REAL_HOME/.rustup" "$HOME/.rustup"
ln -sfn "$REAL_HOME/.cargo" "$HOME/.cargo"
export RUSTUP_HOME="${RUSTUP_HOME:-$REAL_HOME/.rustup}"
export CARGO_HOME="${CARGO_HOME:-$REAL_HOME/.cargo}"
export PATH="$REAL_HOME/.cargo/bin:$PATH"

cat >"$FIX/math_lib/Anubis.toml" <<'EOF'
[package]
name = "math"
version = "1.0.0"
EOF
cat >"$FIX/math_lib/src/lib.anb" <<'EOF'
pub fn add(a, b) { return a + b; }
EOF
cat >"$FIX/app/Anubis.toml" <<'EOF'
[package]
name = "app"
version = "0.1.0"

[dependencies]
math = { path = "../math_lib" }
EOF
cat >"$FIX/app/main.anb" <<'EOF'
import math;
fn main() { print(math::add(2, 3)); }
EOF

"$BIN" keygen --out "$FIX/keys" >"$OUT/keygen.log" 2>&1
PK=$(grep -E 'public key:' "$OUT/keygen.log" | awk '{print $NF}')
if [[ -z "$PK" ]]; then
  PK=$(cat "$FIX/keys/verifying.key" | tr -d '[:space:]')
fi
"$BIN" build "$FIX/math_lib/src/lib.anb" --evidence --out "$FIX/math_lib/out" >"$OUT/math_build.log" 2>&1
EV=$(find "$FIX/math_lib/out" -type d -name 'evidence-*' | head -1)
if [[ -z "$EV" || ! -d "$EV" ]]; then
  fail=$((fail+1))
  note "math_evidence: FAIL (no evidence dir)"
else
  "$BIN" sign "$EV" --key "$FIX/keys/signing.key" >"$OUT/sign.log" 2>&1
  rm -rf "$FIX/math_lib/evidence"
  cp -R "$EV" "$FIX/math_lib/evidence"
  "$BIN" trust add-signer "$PK" --name math-fixture >"$OUT/trust_add.log" 2>&1
  pass=$((pass+1))
  note "sign_and_trust: PASS"
fi

set +e
"$BIN" package lock --root "$FIX/app" >"$OUT/lock.log" 2>&1
lock_rc=$?
set -e
if [[ $lock_rc -eq 0 ]] && [[ -f "$FIX/app/Anubis.lock" ]]; then
  pass=$((pass+1))
  note "package_lock: PASS"
else
  fail=$((fail+1))
  note "package_lock: FAIL (see $OUT/lock.log)"
fi

set +e
"$BIN" package verify --root "$FIX/app" >"$OUT/verify.log" 2>&1
vrc=$?
set -e
if [[ $vrc -eq 0 ]]; then
  pass=$((pass+1))
  note "package_verify: PASS"
else
  fail=$((fail+1))
  note "package_verify: FAIL (see $OUT/verify.log)"
fi

# Tamper detection: mutate dep after lock → cache/hash mismatch
echo 'pub fn add(a, b) { return 0; }' >"$FIX/math_lib/src/lib.anb"
set +e
"$BIN" package verify --root "$FIX/app" >"$OUT/tamper.log" 2>&1
trc=$?
set -e
if [[ $trc -ne 0 ]] && grep -q 'ANUBIS_CACHE_HASH_MISMATCH' "$OUT/tamper.log"; then
  pass=$((pass+1))
  note "tamper_hash: PASS"
else
  fail=$((fail+1))
  note "tamper_hash: FAIL (see $OUT/tamper.log)"
fi
# Restore original body so lock content hash matches again
echo 'pub fn add(a, b) { return a + b; }' >"$FIX/math_lib/src/lib.anb"

set +e
"$BIN" check "$FIX/app/main.anb" >"$OUT/check.log" 2>&1
crc=$?
set -e
if [[ $crc -eq 0 ]]; then
  pass=$((pass+1))
  note "check_with_deps: PASS"
else
  fail=$((fail+1))
  note "check_with_deps: FAIL (see $OUT/check.log)"
fi

set +e
"$BIN" run "$FIX/app/main.anb" --out "$FIX/run_out" >"$OUT/run.log" 2>&1
rrc=$?
set -e
if [[ $rrc -eq 0 ]] && grep -q '5' "$OUT/run.log"; then
  pass=$((pass+1))
  note "run_with_deps: PASS"
else
  fail=$((fail+1))
  note "run_with_deps: FAIL (see $OUT/run.log)"
fi

# 3) Untrusted signer fail-closed (fresh identity, no trust add)
FIX2="$OUT/fixture_untrusted"
rm -rf "$FIX2"
mkdir -p "$FIX2/math_lib/src" "$FIX2/app" "$FIX2/keys" "$FIX2/home/.anubis/trust"
export HOME="$FIX2/home"
ln -sfn "$REAL_HOME/.rustup" "$HOME/.rustup"
ln -sfn "$REAL_HOME/.cargo" "$HOME/.cargo"
cat >"$FIX2/math_lib/Anubis.toml" <<'EOF'
[package]
name = "math"
version = "1.0.0"
EOF
echo 'pub fn add(a, b) { return a + b; }' >"$FIX2/math_lib/src/lib.anb"
cat >"$FIX2/app/Anubis.toml" <<'EOF'
[package]
name = "app"
version = "0.1.0"
[dependencies]
math = { path = "../math_lib" }
EOF
echo 'import math; fn main() { print(1); }' >"$FIX2/app/main.anb"
"$BIN" keygen --out "$FIX2/keys" >"$OUT/keygen2.log" 2>&1
"$BIN" build "$FIX2/math_lib/src/lib.anb" --evidence --out "$FIX2/math_lib/out" >"$OUT/build2.log" 2>&1
EV2=$(find "$FIX2/math_lib/out" -type d -name 'evidence-*' | head -1)
"$BIN" sign "$EV2" --key "$FIX2/keys/signing.key" >"$OUT/sign2.log" 2>&1
cp -R "$EV2" "$FIX2/math_lib/evidence"
set +e
"$BIN" package lock --root "$FIX2/app" >"$OUT/untrusted.log" 2>&1
urc=$?
set -e
if [[ $urc -ne 0 ]] && grep -q 'ANUBIS_DEP_UNTRUSTED_SIGNER' "$OUT/untrusted.log"; then
  pass=$((pass+1))
  note "untrusted_signer: PASS"
else
  fail=$((fail+1))
  note "untrusted_signer: FAIL (see $OUT/untrusted.log)"
fi

# 4) CLI help
if "$BIN" package --help >"$OUT/pkg_help.log" 2>&1 \
  && "$BIN" trust --help >"$OUT/trust_help.log" 2>&1 \
  && grep -q lock "$OUT/pkg_help.log"; then
  pass=$((pass+1))
  note "cli_help: PASS"
else
  fail=$((fail+1))
  note "cli_help: FAIL"
fi

{
  echo "package_gate pass=$pass fail=$fail"
  for d in "${detail[@]}"; do echo "  $d"; done
} | tee "$OUT/summary.txt"

# Coverage ratchet (adversary R49) — outside | tee so fail+= is not lost in a subshell.
_cases=$((pass + fail))
set +e
assert_floor "package_gate" "$_cases" "$ROOT/scripts/floors/package_gate.count_floor"
_floor_rc=$?
set -e
if [[ $_floor_rc -ne 0 ]]; then
  echo "FLOOR: FAIL ($_cases cases; $GATE_FLOOR_ERROR)" >&2
  fail=$((fail + 1))
fi

if [[ "$fail" -gt 0 ]]; then
  exit 1
fi
echo "PACKAGE_GATE: PASS"
exit 0
