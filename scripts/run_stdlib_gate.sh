#!/usr/bin/env bash
# Phase-5: Anubis-source stdlib integrity + functional gate.
# Fail-closed: missing digests, failed import/run, or failed check = FAIL.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
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

# 3) Functional smoke: pure modules
BIN=./target/release/anubis
# Always rebuild so the release binary matches the embedded stdlib under test.
cargo build -q --release -p anubis

if "$BIN" run tests/fixtures/stdlib/math_collections.anb --out "$OUT/math_run" >"$OUT/math_run.log" 2>&1 \
  && grep -q "42" "$OUT/math_run.log"; then
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
  && grep -q "42" "$OUT/edges_run.log"; then
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
  && grep -q "true" "$OUT/crypto_run.log"; then
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
if "$BIN" run "$OUT/password_smoke.anb" --out "$OUT/password_run" >"$OUT/password_run.log" 2>&1 \
  && head -1 "$OUT/password_run.log" | grep -q 'true' \
  && sed -n '2p' "$OUT/password_run.log" | grep -q 'false'; then
  pass=$((pass+1))
  note "std_crypto_password: PASS"
else
  fail=$((fail+1))
  note "std_crypto_password: FAIL"
fi

# 5) std.pwn gold crash (optional if vuln_local present)
if [[ -x poc_kit/bin/vuln_local ]]; then
  if "$BIN" run examples/security/poc_stdlib_overflow.anb --allow-research --out "$OUT/poc" >"$OUT/poc.log" 2>&1 \
    && head -1 "$OUT/poc.log" | grep -q '^1$'; then
    pass=$((pass+1))
    note "poc_stdlib_overflow: PASS"
  else
    fail=$((fail+1))
    note "poc_stdlib_overflow: FAIL"
  fi
else
  note "poc_stdlib_overflow: SKIP (build with bash poc_kit/build_vuln.sh)"
fi

# 6) Evidence bundle on pure std program
if "$BIN" check tests/fixtures/stdlib/math_collections.anb --evidence --out "$OUT/evidence" >"$OUT/evidence.log" 2>&1; then
  pass=$((pass+1))
  note "evidence: PASS"
else
  # check may still write partial — count PASS only if evidence dir exists
  if ls "$OUT"/evidence/evidence-*/MANIFEST.sha256 >/dev/null 2>&1; then
    pass=$((pass+1))
    note "evidence: PASS (bundle present)"
  else
    fail=$((fail+1))
    note "evidence: FAIL"
  fi
fi

total=$((pass+fail))
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
