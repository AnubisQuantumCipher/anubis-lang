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
# Reject fixtures: each must FAIL with ANUBIS_IMPLICIT_FLOW (stmt/value/for/guard coverage).
for if_rej in \
  examples/security/implicit_flow_rejects.anb \
  examples/security/implicit_flow_while_rejects.anb \
  examples/security/implicit_flow_match_rejects.anb \
  examples/security/implicit_flow_iflet_rejects.anb \
  examples/security/implicit_flow_for_rejects.anb \
  examples/security/implicit_flow_value_if_rejects.anb \
  examples/security/implicit_flow_value_match_rejects.anb \
  examples/security/implicit_flow_match_guard_rejects.anb \
  examples/security/implicit_flow_return_rejects.anb \
  examples/security/implicit_flow_return_tail_rejects.anb
do
  base="$(basename "$if_rej" .anb)"
  if ! "$ANUBIS" check "$if_rej" >"$OUT/${base}.txt" 2>&1; then
    if grep -q 'ANUBIS_IMPLICIT_FLOW' "$OUT/${base}.txt"; then
      ok "${base} (compile error)"
    else
      ko "${base} (wrong error)"; tail -5 "$OUT/${base}.txt"
    fi
  else
    ko "${base} (should FAIL)"
  fi
done
for if_ok in \
  examples/security/implicit_flow_secret_local_accepts.anb \
  examples/security/implicit_flow_for_secret_local_accepts.anb \
  examples/security/implicit_flow_return_secret_accepts.anb
do
  base="$(basename "$if_ok" .anb)"
  if "$ANUBIS" check "$if_ok" >"$OUT/${base}.txt" 2>&1; then
    ok "$base"
  else
    ko "$base"; tail -5 "$OUT/${base}.txt"
  fi
done

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

# 4. Confinement from proof + check→confine→run vertical
if "$ANUBIS" vz confine examples/showcase/vz_confine_demo.anb --out "$OUT/confine.json" >/tmp/ess_vz.txt 2>&1; then
  ok "vz confine"
else
  ko "vz confine"; tail -8 /tmp/ess_vz.txt
fi
if bash scripts/run_check_confine_run_gate.sh "$OUT/ccr" >/tmp/ess_ccr.txt 2>&1; then
  ok "check_confine_run vertical"
else
  ko "check_confine_run vertical"; tail -15 /tmp/ess_ccr.txt
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
