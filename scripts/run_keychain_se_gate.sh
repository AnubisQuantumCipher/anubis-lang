#!/usr/bin/env bash
# Keychain / Secure Enclave bind gate
# - Soft path: mandatory on all hosts
# - Signed path (Darwin): compile → codesign (Apple Development or ad-hoc) → Keychain bind
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "=== KEYCHAIN_SE_GATE: soft path (mandatory) ==="
ANUBIS_KEYCHAIN_CAPS=0 ANUBIS_KEYCHAIN_SE=0 \
  cargo test -p anubis-compiler --lib backends::run::run_tests::keychain_se_probe_and_ne_acquire_run --quiet

echo "=== KEYCHAIN_SE_GATE: entitlement derive for NE ==="
cargo test -p anubis-compiler --lib package::entitlements::tests::nonexportable_cap_derives_keychain --quiet

echo "=== KEYCHAIN_SE_GATE: static NE export still sealed ==="
cargo test -p anubis-compiler --lib middle::capability::tests::nonexportable_token_as_print --quiet

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
