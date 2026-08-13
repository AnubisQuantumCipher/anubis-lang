#!/usr/bin/env bash
# Keychain / Secure Enclave bind gate
# - Soft path: mandatory on all hosts
# - Signed path (Darwin): compile → codesign (Apple Development or ad-hoc) → Keychain bind
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# Run ONE named test and prove it actually ran.
#
# libtest name filters are SUBSTRING matches, and a filter matching zero tests EXITS 0
# ("0 passed; 0 failed; N filtered out"). Two of the three filters below were PREFIXES of the real
# test names — `nonexportable_cap_derives_keychain` vs the actual
# `nonexportable_cap_derives_keychain_and_se_keys`, and `nonexportable_token_as_print` vs
# `nonexportable_token_as_print_arg_is_export`. They passed by substring luck. Rename either test's
# prefix and this gate would run NOTHING and report success, on the lane that guards Secure Enclave
# key export.
#
# `--exact` plus an assertion on the passed count closes both halves: the wrong name now fails
# loudly instead of silently matching nothing.
run_one() {
  local desc="$1" name="$2"; shift 2
  echo "=== KEYCHAIN_SE_GATE: $desc ==="
  local out
  out="$(env "$@" cargo test -p anubis-compiler --lib "$name" -- --exact 2>&1)"
  local rc=$?
  printf '%s\n' "$out" | grep -E '^test result:' || true
  if [[ $rc -ne 0 ]]; then
    echo "KEYCHAIN_SE_GATE: FAIL ($name exited $rc)" >&2
    return 1
  fi
  if ! printf '%s\n' "$out" | grep -qE '^test result: ok\. 1 passed'; then
    echo "KEYCHAIN_SE_GATE: FAIL ($name matched no test — a rename would have gone unnoticed)" >&2
    return 1
  fi
}

run_one "soft path (mandatory)" \
  backends::run::run_tests::keychain_se_probe_and_ne_acquire_run \
  ANUBIS_KEYCHAIN_CAPS=0 ANUBIS_KEYCHAIN_SE=0

run_one "entitlement derive for NE" \
  package::entitlements::tests::nonexportable_cap_derives_keychain_and_se_keys

run_one "static NE export still sealed" \
  middle::capability::tests::nonexportable_token_as_print_arg_is_export

if [ "$(uname -s)" = "Darwin" ]; then
  echo "=== KEYCHAIN_SE_GATE: signed compile→codesign→Keychain bind ==="
  if security find-identity -v -p codesigning 2>/dev/null | grep -q "Apple Development"; then
    echo "  identity:"
    security find-identity -v -p codesigning 2>/dev/null | grep "Apple Development" | head -1
  else
    echo "  identity: ad-hoc fallback"
  fi
  cargo test -p anubis-compiler --lib backends::run::run_tests::keychain_se_signed_run_binds_keychain --quiet
fi

echo
echo "KEYCHAIN_SE_GATE: PASS"
echo "  soft path: green"
echo "  Darwin signed path: green (kc: under Development identity; se: when SE available)"
echo "  residual: App Store / notarized distribution packaging; restricted SE entitlement + provisioning UX"
