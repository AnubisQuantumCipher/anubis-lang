#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# build_signed_anubis.sh — build the `anubis` binary and code-sign it with the
# com.apple.security.virtualization entitlement so the NATIVE VZ backend
# (`anubis vz native-preflight` / future `native-boot`) can instantiate a guest.
#
# WHY: Apple's Virtualization.framework refuses to instantiate a VZVirtualMachine
# unless the process is signed with `com.apple.security.virtualization`. A plain
# `cargo build` leaves the binary UNSIGNED, so -[VZVirtualMachineConfiguration
# validateWithError:] fails with "...doesn't have the ... entitlement". This
# script does the build AND the codesign so the entitlement actually lands.
#
# The entitlement is NOT restricted: an AD-HOC signature (`--sign -`) is enough
# to run this on your own Mac — no Apple Developer portal, no provisioning
# profile. Pass --identity "Apple Development: you@example.com (TEAMID)" to sign
# with a real identity instead (needed only if you later notarize for others).
#
# USAGE:
#   scripts/build_signed_anubis.sh                 # release, ad-hoc signature
#   scripts/build_signed_anubis.sh --debug         # debug profile
#   scripts/build_signed_anubis.sh --identity "Apple Development: ... (TEAMID)"
#   scripts/build_signed_anubis.sh --jobs 4        # cap build parallelism (host safety)
#
# On success it prints the signed binary path and verifies the entitlement is on it.
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
ENTITLEMENTS="$REPO/vm/entitlements/anubis.entitlements"
PROFILE="release"
PROFILE_FLAG="--release"
IDENTITY="-"                 # "-" == ad-hoc; override with --identity
JOBS=""                      # empty == cargo default; cap on the host to protect WindowServer

while [ $# -gt 0 ]; do
  case "$1" in
    --debug)    PROFILE="debug"; PROFILE_FLAG="";;
    --release)  PROFILE="release"; PROFILE_FLAG="--release";;
    --identity) IDENTITY="${2:?--identity needs a value}"; shift;;
    --jobs)     JOBS="${2:?--jobs needs a value}"; shift;;
    *) echo "unknown arg: $1" >&2; exit 2;;
  esac
  shift
done

# ── preconditions ────────────────────────────────────────────────────────────
[ "$(uname -s)" = "Darwin" ] || { echo "FATAL: native VZ backend requires macOS" >&2; exit 1; }
[ "$(uname -m)" = "arm64" ]  || { echo "FATAL: native VZ backend requires Apple Silicon (arm64)" >&2; exit 1; }
[ -f "$ENTITLEMENTS" ]       || { echo "FATAL: missing $ENTITLEMENTS" >&2; exit 1; }
command -v codesign >/dev/null 2>&1 || { echo "FATAL: codesign not found" >&2; exit 1; }

# ── build ────────────────────────────────────────────────────────────────────
echo "[build] cargo build -p anubis ${PROFILE_FLAG} ${JOBS:+-j $JOBS}"
# shellcheck disable=SC2086
( cd "$REPO" && cargo build -p anubis $PROFILE_FLAG ${JOBS:+-j "$JOBS"} )

BIN="$REPO/target/$PROFILE/anubis"
[ -x "$BIN" ] || { echo "FATAL: expected binary not found at $BIN" >&2; exit 1; }

# ── sign ─────────────────────────────────────────────────────────────────────
if [ "$IDENTITY" = "-" ]; then
  echo "[sign] ad-hoc signature (--sign -) with the virtualization entitlement"
else
  echo "[sign] identity: $IDENTITY"
fi
codesign --force --sign "$IDENTITY" --entitlements "$ENTITLEMENTS" --options runtime "$BIN" 2>/dev/null \
  || codesign --force --sign "$IDENTITY" --entitlements "$ENTITLEMENTS" "$BIN"

# ── verify ───────────────────────────────────────────────────────────────────
echo "[verify] entitlement on the signed binary:"
if codesign -d --entitlements - "$BIN" 2>/dev/null | grep -q "com.apple.security.virtualization"; then
  echo "  ✓ com.apple.security.virtualization present"
else
  echo "  ✗ entitlement NOT present after signing" >&2
  exit 1
fi

echo
echo "[done] signed binary: $BIN"
echo "       prove the lane:  $BIN vz native-preflight <program.anb>"
