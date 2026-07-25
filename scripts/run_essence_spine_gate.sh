#!/usr/bin/env bash
# Essence spine — the invariant Anubis exists to mint re-checkable truth.
# Fails closed if any load-bearing pillar is red.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
OUT="${1:-out/essence_spine_gate}"
mkdir -p "$OUT"
ANUBIS="${ANUBIS:-$ROOT/target/release/anubis}"
if [[ ! -x "$ANUBIS" ]]; then
  cargo build --release -p anubis 2>&1 | tail -5
fi

pass=0
fail=0
note() { echo "  $1" | tee -a "$OUT/summary.txt"; }
ok() { pass=$((pass+1)); note "PASS  $1"; }
ko() { fail=$((fail+1)); note "FAIL  $1"; }

: >"$OUT/summary.txt"
note "Anubis essence spine — fail-closed pillars"
note "thesis: claims become re-checkable artifacts; green check never certifies what run violates"
note ""

# 1. Implicit-flow identity (confidentiality assignment pattern)
if ! "$ANUBIS" check examples/security/implicit_flow_rejects.anb >/tmp/ess_if_rej.txt 2>&1; then
  if grep -q 'ANUBIS_IMPLICIT_FLOW' /tmp/ess_if_rej.txt; then
    ok "implicit_flow_rejects (compile error)"
  else
    ko "implicit_flow_rejects (wrong error)"; cat /tmp/ess_if_rej.txt | tail -5
  fi
else
  ko "implicit_flow_rejects (should FAIL)"
fi
if "$ANUBIS" check examples/security/implicit_flow_secret_local_accepts.anb >/tmp/ess_if_ok.txt 2>&1; then
  ok "implicit_flow_secret_local_accepts"
else
  ko "implicit_flow_secret_local_accepts"; tail -5 /tmp/ess_if_ok.txt
fi

# 2. Flagship showcases
if "$ANUBIS" check examples/showcase/nexus/nexus_cognitive_kernel.anb >/tmp/ess_nx.txt 2>&1; then
  ok "NEXUS check"
else
  ko "NEXUS check"; tail -8 /tmp/ess_nx.txt
fi
if "$ANUBIS" check --verified examples/showcase/anubis_vault/vault.anb >/tmp/ess_v.txt 2>&1; then
  ok "Vault check --verified"
else
  ko "Vault check --verified"; tail -8 /tmp/ess_v.txt
fi

# 3. Counterexample mint (essence: value, not shrug)
if ! "$ANUBIS" check examples/showcase/ring_buffer_underflow.anb >/tmp/ess_rb.txt 2>&1; then
  if grep -qi 'counterexample\|ANUBIS_ASSERTION_UNPROVEN\|unproven' /tmp/ess_rb.txt; then
    ok "ring_buffer counterexample (disproof)"
  else
    ko "ring_buffer (expected disproof)"; tail -8 /tmp/ess_rb.txt
  fi
else
  ko "ring_buffer (should disprove)"
fi

# 4. Confinement from proof
if "$ANUBIS" vz confine examples/showcase/vz_confine_demo.anb --out "$OUT/confine.json" >/tmp/ess_vz.txt 2>&1; then
  ok "vz confine"
else
  ko "vz confine"; tail -8 /tmp/ess_vz.txt
fi

# 5–6. TCB spine (optional skip for quick local loops: ESSENCE_SPINE_FAST=1)
if [[ "${ESSENCE_SPINE_FAST:-0}" == "1" ]]; then
  note "SKIP  native_authoritative_gate (ESSENCE_SPINE_FAST=1)"
  note "SKIP  formal_gate (ESSENCE_SPINE_FAST=1)"
else
  if bash scripts/run_native_authoritative_gate.sh >"$OUT/native_auth.log" 2>&1; then
    ok "native_authoritative_gate"
  else
    ko "native_authoritative_gate"; tail -15 "$OUT/native_auth.log"
  fi
  if bash scripts/run_formal_gate.sh >"$OUT/formal.log" 2>&1; then
    ok "formal_gate"
  else
    ko "formal_gate"; tail -10 "$OUT/formal.log"
  fi
fi

note ""
note "pass=$pass fail=$fail"
if [[ "$fail" -ne 0 ]]; then
  echo "ESSENCE_SPINE_GATE: FAIL ($fail pillars red)"
  exit 1
fi
echo "ESSENCE_SPINE_GATE: PASS ($pass pillars green)"
exit 0
