# Security Research Profile (design)

**Status:** design + partial runtime spine (not full language surface)  
**Date:** 2026-07-26  
**Identity:** proof-carrying security research language — not a generic C2 framework.

## Goal

A researcher writes an authorized PoC / fuzz / emulation / crypto / bounty program in Anubis. The compiler proves authorization, scope, effects, isolation, and evidence obligations. Execution is only allowed inside a disposable Tart/Apple Virtualization guest with a guest-bound run capability. Results are hash- and MAC-bound evidence.

## Profiles

| Profile | Default | Isolation | Notes |
|---------|---------|-----------|-------|
| `safe` | **yes** | host OK | no net/process mutation/FFI |
| `research` | opt-in | **mandatory VZ** | PoC, crash, fuzz, debug |
| `emulation` | opt-in | **mandatory VZ** | ATT&CK-aligned defense validation |
| `crypto_research` | opt-in | VZ for leakage/fuzz; host for pure math | never overclaim CAVP |
| `bounty` | opt-in | scope-bound only | no arbitrary RCE |

## Typed objects (target HIR)

`Authorization`, `Engagement`, `Scope<T>` / `Scoped<T>`, `Technique`, `DetectionExpectation`, `Secret<T>`, `Declassified<T,R>`, `Finding`, `Evidence<T>`, `Verified<T>`, `Unverified<T>`, `GuestRun<T>`.

Raw strings/paths/URLs/PIDs must not silently become scoped targets.

## Effects (shared IR)

`net.connect`, `net.listen`, `fs.read`, `fs.write`, `process.spawn`, `process.inspect`, `debug.attach`, `vm.execute`, `secret.use`, `evidence.emit`, `human.approve`.

Checker, runtime, and VZ confinement must consume the **same** normalized effect representation.

## Execution pipeline (target)

```
source → AST → typed HIR → effect IR → contracts/SMT
  → verified plan → VZ confinement → binary
  → guest-bound run capability → disposable guest
  → signed/MAC evidence bundle
```

## What exists now (LAB_REAL / PARTIAL)

| Piece | Location | Classification |
|-------|----------|----------------|
| Fail-closed contracts on check/build/run | compiler + CLI | PARTIAL→REAL (trust spine commit) |
| Research mode aggregation | middle | LAB_REAL |
| Engagement + scope + encrypt listener | AOP | LAB_REAL (P0 closed) |
| Receipt HMAC chain | `receipts.rs` | LAB_REAL_HMAC |
| Run capability mint/validate | `run_capability.rs` | LAB_REAL (optional via env) |
| Tart isolation | `vz.rs` + isolation | LAB_REAL |
| Full Security Research syntax | language | NOT_IMPLEMENTED |

## Non-goals

- Stealth/evasion as maturity metric  
- Arbitrary remote exploitation  
- Host crash PoC as primary evidence  
- Beating Caldera at scale  

## Next implementation slices

1. Wire `run_capability` mint into `anubis vz exploit|fuzz` host orchestrator (always mint; guest always validates).  
2. HIR types for Engagement/Scope + constructors that check engagement.  
3. Effect IR shared between checker and VZ confine.  
4. Independent portable evidence verifier CLI.  
5. Domain packs (PoC/fuzz/crypto/bounty/emulation) with honest classifications.

## Honesty rule

No documentation may call a PLAN_ONLY, lab MAC, or helper a production-REAL capability without a positive control, hostile negative control, and reproducible artifact.
