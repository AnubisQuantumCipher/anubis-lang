#!/usr/bin/env bash
# Keychain / Secure Enclave bind gate for non-exportable capabilities.
# Soft path is mandatory green. Keychain/SE bind is best-effort on macOS.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "KEYCHAIN_SE_GATE: soft path (ANUBIS_KEYCHAIN_CAPS=0)"
ANUBIS_KEYCHAIN_CAPS=0 ANUBIS_KEYCHAIN_SE=0 \
  cargo test -p anubis-compiler --lib backends::run::run_tests::keychain_se_probe_and_ne_acquire_run --quiet

echo "KEYCHAIN_SE_GATE: entitlement derive for NE"
cargo test -p anubis-compiler --lib package::entitlements::tests::nonexportable_cap_derives_keychain --quiet

echo "KEYCHAIN_SE_GATE: static NE export still sealed"
cargo test -p anubis-compiler --lib middle::capability::tests::nonexportable_token_as_print --quiet

echo "KEYCHAIN_SE_GATE: PASS (soft + entitlements + static NE)"
echo "NOTE: live Keychain/SE item create is host-dependent; soft fallback is the CI contract."
echo "      Production SE isolation still requires codesign + access groups (apple_enforced_claim=false until signed)."
