#!/usr/bin/env bash
# ============================================================================
# A+ acceptance audit — stable entry point for the FULL sealed gate suite.
#
# This is a thin, honest alias: it runs the repo safety check, then delegates to
# scripts/audit_unified.sh — the canonical runner that executes every gate
# (G1-G15: fmt, clippy, tests, build, language/turing/PCA/security/poc-kit/
# prove/enum/for-in/lang-trio/offensive fixtures, dogfood) and emits an honest
# PASS/FAIL/SKIP verdict + JSON report, exiting non-zero if any gate fails.
#
# History: this script used to be a 4-gate skeleton that ended in a bare
# "add the rest later" TODO while the acceptance docs advertised it as the full
# sealed suite. That stub is retired — the remaining gates live in
# audit_unified.sh and this front door now runs them, so the documented promise
# ("run audit_a_plus.sh -> full sealed gate suite") is true rather than aspirational.
#
# Usage: bash scripts/audit_a_plus.sh [--out DIR]
# ============================================================================
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# Fail closed before running any gate if we are outside the repo tree.
bash tools/grok-safety-check.sh

# Delegate to the canonical runner; its exit code becomes ours (fail-closed).
exec bash "$ROOT/scripts/audit_unified.sh" "$@"
