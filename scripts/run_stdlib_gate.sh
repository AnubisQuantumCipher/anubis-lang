#!/usr/bin/env bash
# Phase-5: Anubis-source stdlib integrity + functional gate.
# Fail-closed: missing digests, failed import/run, or failed check = FAIL.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
source "$ROOT/scripts/lib/gate_common.sh"
OUT="${1:-out/stdlib_gate}"
mkdir -p "$OUT"

pass=0
fail=0
detail=()

note() { detail+=("$1"); }

# 1) Content lock: live embedded digests == checked-in MANIFEST
LIVE=$(cargo test -p anubis-compiler --lib embedded_sources_match_checked_in_manifest -- --exact 2>&1 | tee "$OUT/manifest.log" || true)
if grep -q "test result: ok" "$OUT/manifest.log"; then
  pass=$((pass+1))
  note "manifest_lock: PASS"
else
  fail=$((fail+1))
  note "manifest_lock: FAIL (see $OUT/manifest.log)"
fi

# 2) Unit tests: stdlib registry + phase5 integration
if cargo test -p anubis-compiler --lib phase5_ -- --test-threads=4 >"$OUT/unit.log" 2>&1 \
  && cargo test -p anubis-compiler --lib stdlib:: -- --test-threads=4 >>"$OUT/unit.log" 2>&1; then
  pass=$((pass+1))
  note "unit_phase5: PASS"
else
  fail=$((fail+1))
  note "unit_phase5: FAIL (see $OUT/unit.log)"
fi

# 3) Functional smoke: pure modules. A seal or VM lane may bind an immutable compiler via
# ANUBIS_BIN; in that case never replace the named instrument with a mutable rebuild. Standalone
# invocations retain the historical build-first behavior.
if [[ -n "${ANUBIS_BIN:-}" ]]; then
  BIN="$ANUBIS_BIN"
  [[ -x "$BIN" ]] || { echo "STDLIB_GATE: FAIL (ANUBIS_BIN=$BIN not executable)"; exit 127; }
else
  BIN=./target/release/anubis
  cargo build -q --release -p anubis
fi

# Prefer pure stdout lines (ignore "anubis run: compiling…" banners).
stdout_lines() { grep -E '^(true|false|-?[0-9]+)$' "$1" 2>/dev/null || true; }

if "$BIN" run tests/fixtures/stdlib/math_collections.anb --out "$OUT/math_run" >"$OUT/math_run.log" 2>&1 \
  && stdout_lines "$OUT/math_run.log" | grep -qx '42'; then
  pass=$((pass+1))
  note "math_collections_run: PASS"
else
  fail=$((fail+1))
  note "math_collections_run: FAIL"
fi

# 4) I/O leak check must FAIL closed
if "$BIN" check tests/fixtures/stdlib/io_leak.anb >"$OUT/io_leak.log" 2>&1; then
  fail=$((fail+1))
  note "io_leak: FAIL (expected check failure)"
else
  if grep -q "ANUBIS_INTERPROC_SINK\|TAINTED" "$OUT/io_leak.log"; then
    pass=$((pass+1))
    note "io_leak: PASS (rejected)"
  else
    fail=$((fail+1))
    note "io_leak: FAIL (wrong error)"
  fi
fi

# 4b) Capability inheritance: write/shell without uses must FAIL closed
if "$BIN" check tests/fixtures/stdlib/effect_write_forbidden.anb >"$OUT/effect_write.log" 2>&1; then
  fail=$((fail+1))
  note "effect_write_forbidden: FAIL (expected reject)"
else
  if grep -q "ANUBIS_EFFECT_FORBIDDEN_IN_MODE\|file_write" "$OUT/effect_write.log"; then
    pass=$((pass+1))
    note "effect_write_forbidden: PASS (rejected)"
  else
    fail=$((fail+1))
    note "effect_write_forbidden: FAIL (wrong error)"
  fi
fi
if "$BIN" check tests/fixtures/stdlib/effect_shell_forbidden.anb >"$OUT/effect_shell.log" 2>&1; then
  fail=$((fail+1))
  note "effect_shell_forbidden: FAIL (expected reject)"
else
  if grep -q "ANUBIS_EFFECT_FORBIDDEN_IN_MODE\|shell" "$OUT/effect_shell.log"; then
    pass=$((pass+1))
    note "effect_shell_forbidden: PASS (rejected)"
  else
    fail=$((fail+1))
    note "effect_shell_forbidden: FAIL (wrong error)"
  fi
fi

# 4c) Broad export smoke
if "$BIN" run tests/fixtures/stdlib/edges_all_modules.anb --out "$OUT/edges_run" >"$OUT/edges_run.log" 2>&1 \
  && stdout_lines "$OUT/edges_run.log" | grep -qx '42'; then
  pass=$((pass+1))
  note "edges_all_modules: PASS"
else
  fail=$((fail+1))
  note "edges_all_modules: FAIL"
fi

# 4d) std.crypto RWC surface (HMAC verify + AEAD + password KDF KATs)
cat > "$OUT/crypto_smoke.anb" <<'ANB'
import std.crypto;
fn main() {
  print(crypto::mac_verify("k", "m", crypto::mac_hmac_sha256("k", "m")));
  let key = crypto::aead_keygen();
  let n = crypto::aead_nonce();
  let ct = crypto::aead_encrypt(key, n, "aad", "hi");
  print(len(crypto::aead_decrypt(key, n, "aad", ct)));
  // PBKDF2 RFC 6070 c=1 → 32 bytes; Argon2id KAT size + self-eq
  print(len(crypto::kdf_pbkdf2_hmac_sha256("password", "salt", 1, 32)));
  let h = crypto::kdf_argon2id("password", "somesalt", 32, 3, 1, 32);
  print(crypto::secret_eq(h, crypto::kdf_argon2id("password", "somesalt", 32, 3, 1, 32)));
}
ANB
if "$BIN" run "$OUT/crypto_smoke.anb" --out "$OUT/crypto_run" >"$OUT/crypto_run.log" 2>&1 \
  && stdout_lines "$OUT/crypto_run.log" | grep -qx 'true'; then
  pass=$((pass+1))
  note "std_crypto: PASS"
else
  fail=$((fail+1))
  note "std_crypto: FAIL"
fi

# 4e) password_hash / password_verify (Argon2id production defaults — real, not deferred)
cat > "$OUT/password_smoke.anb" <<'ANB'
import std.crypto;
fn main() {
  let s = crypto::password_hash("phase5-lock");
  print(crypto::password_verify("phase5-lock", s));
  print(crypto::password_verify("nope", s));
}
ANB
# Capture pure program stdout lines only (banners break naive head -1).
"$BIN" run "$OUT/password_smoke.anb" --out "$OUT/password_run" >"$OUT/password_run.log" 2>&1 || true
_pw0=$(stdout_lines "$OUT/password_run.log" | sed -n '1p')
_pw1=$(stdout_lines "$OUT/password_run.log" | sed -n '2p')
if [[ "$_pw0" == "true" && "$_pw1" == "false" ]]; then
  pass=$((pass+1))
  note "std_crypto_password: PASS"
else
  fail=$((fail+1))
  note "std_crypto_password: FAIL (got: '${_pw0:-}' '${_pw1:-}'; see $OUT/password_run.log)"
fi

# 5) std.pwn gold crash (optional if vuln_local present)
if [[ -x poc_kit/bin/vuln_local ]]; then
  # PoC prints crash flag on first pure-numeric line (`1`); isolation banners must not count.
  # The crash PoC runs in a DISPOSABLE GUEST, never on the host. `run --allow-research` on the host
  # is refused by the runtime itself (ANUBIS_RESEARCH_HOST_FORBIDDEN) — this gate was still calling
  # it, so the step failed for the right reason and the gate read it as a defect in the PoC.
  #
  # Inside the VM battery there is no nested virtualization, so the isolated lane is unavailable.
  # That is recorded as an explicit SKIP with its reason rather than counted: a crash PoC that never
  # ran isolated is not evidence, and calling a host run "isolated" would be fabrication.
  if [[ -z "${ANUBIS_IN_VM_GUEST:-}" ]] && command -v tart >/dev/null 2>&1; then
    # `vz exploit` requires --engage so the crash op is SEALED into the receipt chain. That is the
    # point of the flag: without it the guest is discarded and nobody can prove what happened, so
    # the gate mints a real engagement rather than reaching for a bypass.
    "$BIN" engage-init --dir "$OUT/engagement" --name stdlib-gate \
      --authorization local-lab-charter >"$OUT/engage_init.log" 2>&1 || true
    _poc_cmd=("$BIN" vz exploit examples/security/poc_stdlib_overflow.anb --allow-research \
      --base anubis-xcode --engage "$OUT/engagement")
  else
    _poc_cmd=()
  fi
  if [[ ${#_poc_cmd[@]} -eq 0 ]]; then
    note "poc_stdlib_overflow: SKIP (no disposable-guest lane here: nested virtualization unavailable in the VM battery; host --allow-research is forbidden and is NOT a substitute)"
  elif "${_poc_cmd[@]}" >"$OUT/poc.log" 2>&1 \
    && stdout_lines "$OUT/poc.log" | head -1 | grep -qx '1' \
    && grep -qE 'crashed:[[:space:]]*1|verdict:[[:space:]]*IMPACT' "$OUT/poc.log"; then
    pass=$((pass+1))
    note "poc_stdlib_overflow: PASS"
  else
    fail=$((fail+1))
    note "poc_stdlib_overflow: FAIL (see $OUT/poc.log)"
  fi
else
  note "poc_stdlib_overflow: SKIP (build with bash poc_kit/build_vuln.sh)"
fi

# 6) Evidence bundle on pure std program (must check-clean + emit MANIFEST)
if "$BIN" check tests/fixtures/stdlib/math_collections.anb --evidence --out "$OUT/evidence" >"$OUT/evidence.log" 2>&1 \
  && ls "$OUT"/evidence/evidence-*/MANIFEST.sha256 >/dev/null 2>&1; then
  pass=$((pass+1))
  note "evidence: PASS"
else
  fail=$((fail+1))
  note "evidence: FAIL (check+manifest required; see $OUT/evidence.log)"
fi

total=$((pass+fail))

# Coverage ratchet (adversary R49): case total must not silently shrink.
set +e
# The floor is keyed by whether the DISPOSABLE-GUEST LANE exists here, because the case count
# legitimately differs between environments and one floor cannot describe both.
#
# On the host the crash PoC runs in a throwaway VM (11 cases). Inside the VM battery there is no
# nested virtualization, so that lane is structurally unavailable and SKIPs (10 cases) — and a single
# shared floor read that as `coverage fell: stdlib_gate=10, floor is 11`, failing a guest run where
# every case that COULD run passed. Same shape as the shared runtime-fixture floor fixed earlier:
# a number describing one environment consulted as if it described another.
#
# Keyed, both still ratchet: losing a case in either environment is still caught, and the SKIP keeps
# its explicit reason in the report so an unavailable lane can never read as a pass.
if [[ -z "${ANUBIS_IN_VM_GUEST:-}" ]] && command -v tart >/dev/null 2>&1; then
  _floor_env="host_with_guest_lane"
else
  _floor_env="no_guest_lane"
fi
assert_floor "stdlib_gate[$_floor_env]" "$total" "$ROOT/scripts/floors/stdlib_gate.$_floor_env.count_floor"
_floor_rc=$?
set -e
if [[ $_floor_rc -ne 0 ]]; then
  echo "FLOOR: FAIL ($total cases; $GATE_FLOOR_ERROR)" >&2
  fail=$((fail + 1))
  total=$((pass+fail))
fi

if [[ "$fail" -eq 0 ]]; then
  verdict=PASS
else
  verdict=FAIL
fi

{
  echo "{"
  echo "  \"overall_verdict\": \"$verdict\","
  echo "  \"pass\": $pass,"
  echo "  \"fail\": $fail,"
  echo "  \"details\": ["
  i=0
  for d in "${detail[@]}"; do
    i=$((i+1))
    esc=${d//\"/\\\"}
    if [[ $i -lt ${#detail[@]} ]]; then
      echo "    \"$esc\","
    else
      echo "    \"$esc\""
    fi
  done
  echo "  ]"
  echo "}"
} | tee "$OUT/report.json"

echo "stdlib gate: $verdict ($pass pass, $fail fail) → $OUT/report.json"
[[ "$verdict" == "PASS" ]]
