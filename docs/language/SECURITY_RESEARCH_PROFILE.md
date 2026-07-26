# Security Research Profile (design + Phase 3 HIR)

**Status:** design + runtime spine + **Phase 3 typed HIR stubs** (not full language surface)  
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

Typed in `compiler/src/middle/research_profile.rs` as `ResearchProfile`.

## Typed objects (HIR)

| Type | Module | Status |
|------|--------|--------|
| `ResearchProfile` | `middle/research_profile` | LAB_REAL (typed + unit tests) |
| `SecurityEffect` | same | LAB_REAL (normalized effect IR) |
| `EngagementRef` | same | LAB_REAL (identity + allow-lists) |
| `ScopedPath` / `ScopedHost` | same | LAB_REAL (constructor-gated scope) |
| `Authorization` | same | LAB_REAL (digest bound to engagement) |
| `Evidence<T>` / `TrustLabel` | same | LAB_REAL (LAB_REAL / PLAN_ONLY labels) |
| `GuestRun` | same | LAB_REAL (plan for run-capability mint) |
| Language syntax for above | parser | **NOT_IMPLEMENTED** (permanent residual) |
| Checker exposes `TypedIR.proven_effects` | `typecheck` | LAB_REAL (effect IR; not full HIR syntax) |

Raw strings/paths/URLs/PIDs must not silently become scoped targets — `ScopedPath::bind` / `ScopedHost::bind` fail closed on out-of-scope inputs (`ANUBIS_RESEARCH_SCOPE`).

## Effects (shared IR)

Canonical research effects (`SecurityEffect`):

`net.connect`, `net.listen`, `fs.read`, `fs.write`, `process.spawn`, `process.inspect`, `debug.attach`, `vm.execute`, `secret.use`, `evidence.emit`, `human.approve`.

Legacy Safe-mode tags fold in: `shell` → `process.spawn`, `net.send` → `net.connect`.

Crash/research-class effects (`process.spawn|inspect`, `debug.attach`, `vm.execute`, `net.listen`) set `requires_run_capability` on `GuestRun`.

## Execution pipeline (target)

```
source → AST → typed HIR → effect IR → contracts/SMT
  → verified plan → VZ confinement → binary
  → guest-bound run capability → disposable guest
  → signed/MAC evidence bundle
```

## What exists now (honest)

| Piece | Location | Classification |
|-------|----------|----------------|
| Fail-closed contracts on check/build/run | compiler + CLI | PARTIAL→REAL (trust spine) |
| Research mode aggregation | middle | LAB_REAL |
| Engagement + scope + encrypt listener | AOP | LAB_REAL (P0 closed) |
| Receipt HMAC chain | `receipts.rs` | LAB_REAL_HMAC |
| Run capability mint/validate | `run_capability.rs` | LAB_REAL |
| **VZ host always mints + stages cap** on `vz exploit` / `vz fuzz` / research-class `vz exec` | `vz.rs` | LAB_REAL |
| Guest enforces cap when `ANUBIS_VZ_ENFORCE_RUN_CAP=1` | `isolation.rs` | LAB_REAL |
| Phase 3 HIR types (profiles, scope, effects, GuestRun) | `research_profile.rs` | LAB_REAL (typed IR) |
| **Shared `ProvenEffectSet` IR** (checker → confine + entitlements + VZ run cap) | `research_profile` + `confinement` + `vz.rs` | LAB_REAL |
| **`TypedIR.proven_effects` on typecheck path** | `middle::typecheck` | LAB_REAL (same fixpoint) |
| Full Security Research **syntax** (parser surface for Engagement/Scope/…) | language | **NOT_IMPLEMENTED** (permanent residual) |
| **Independent portable evidence verifier** | `anubis evidence-verify` | LAB_REAL (host offline; multi-artifact) |
| **Domain packs** (PoC/fuzz/crypto/bounty/emulation) | `anubis research-pack` | LAB_REAL catalog + scaffold; per-cap honesty |
| Receipt / run-cap authenticity | HMAC | **LAB_REAL_HMAC** — not Ed25519 PKI |
| Coverage-guided / AFL fuzz | tools | **NOT_IMPLEMENTED** (permanent residual) |
| NIST CAVP / FIPS | crypto pack | **NOT_IMPLEMENTED** (permanent residual) |
| Caldera-scale emulation farm | emulation pack | **NOT_IMPLEMENTED** (permanent residual) |

## Design slices 1–5

**All done** (runtime spine + IR + packs + verify). Not a full research language surface.

1. ~~Wire `run_capability` mint into `anubis vz exploit|fuzz` host orchestrator~~ **done** (also research-class `vz exec`).  
2. ~~HIR types for Engagement/Scope + constructors that check engagement~~ **done** (typed stubs; no parser).  
3. ~~Effect IR shared between checker and VZ confine~~ **done**: `ProvenEffectSet` from checker fixpoint; confinement emits `research_effects`; VZ mint from `.anb` uses same IR (`net.send`→`net.connect`, `shell`→`process.spawn`).  
4. ~~Independent portable evidence verifier CLI~~ **done**: `anubis evidence-verify <path> [--json] [--pubkey] [--run-cap-key] [--strict]` — PCA, engagement hash, receipt HMAC, run-cap MAC, confinement re-derive; host-side, no VZ.  
5. ~~Domain packs (PoC/fuzz/crypto/bounty/emulation)~~ **done**: `anubis research-pack list|show|scaffold|validate` — per-capability LAB_REAL / PLAN_ONLY / NOT_IMPLEMENTED; effect allow-list validate against `ProvenEffectSet`.

## Permanent residuals (explicit non-claims)

- Full Anubis **parser/syntax** for research HIR types  
- NIST **CAVP** / FIPS certification  
- Coverage-guided / **AFL-class** fuzz engine  
- **Caldera-scale** adversary emulation farm  
- Upgrading receipt/run-cap **HMAC → Ed25519 PKI** attestation  
- Softnet DNS rebind HARD / Keychain-SE hardware bind (platform residuals)

## Non-goals

- Stealth/evasion as maturity metric  
- Arbitrary remote exploitation  
- Host crash PoC as primary evidence  
- Beating Caldera at scale  

## Honesty rule

No documentation may call a PLAN_ONLY, lab MAC, or helper a production-REAL capability without a positive control, hostile negative control, and reproducible artifact. HMAC paths remain **LAB_REAL_HMAC**, never Ed25519 REAL without positive + hostile controls.
