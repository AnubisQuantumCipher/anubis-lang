# Phase 4 completion report — 2026-08-15

**Verdict: COMPLETE PENDING EXTERNAL SEAL.** Completion Blueprint Phase 4
(§56) — "close or explicitly publish the residual soundness surface" — is
satisfied in the sense the blueprint defines: every open soundness residual
is either **closed** with a RED→GREEN receipt or **explicitly published** as a
named, dated entry in `docs/CLAIMS.md`. One residual was closed this phase
(CLAIMS item 21 row 6); the rest are published open. Criterion 5 (canonical
VZ self-host seal) is **EXTERNAL / pending**, sharing the Phase-3 guest
toolchain limitation — recorded honestly, not inherited as a PASS.

Base commit: `origin/main` after PR #35 (`0b0889da`).

## 1. Header

```text
Phase:              4 — close or explicitly publish the residual soundness surface
Base commit:        0b0889da8cd49843d6f1f1a8fb6cf24b1e5c5387 (origin/main after PR #35)
Closure PR:         #35 — compiler/middle: close item-21 row-6 place-assignment write-carrier
Candidate binary:   9f0e46ed970d188f8be93cf6aac0ab091ff7f7f148ac94f63498aab151eff45b
rustc (host):       rustc 1.97.0-nightly
Z3 (host):          Z3 version 4.15.4 - 64 bit
```

## 2. What "close or explicitly publish" means here

The blueprint gives two acceptable outcomes per residual. This phase applied
both:

- **CLOSED (with receipt):** CLAIMS item 21 **row 6** — the place-assignment
  write-carrier for function identity — closed for the `let`-bound read shape
  on both security lanes.
- **EXPLICITLY PUBLISHED (named, dated):** every other open soundness residual
  remains in `docs/CLAIMS.md` with its status. This report indexes them
  (§4) but does not maintain a second inventory.

## 3. The closure — CLAIMS item 21 row 6

RED (pre-fix binary `601b0ef2…`): `struct Box { f: u64 } fn key() ->
secret<i64> { return 42; } fn main() { let b = Box { f: 0 }; b.f = key; let g
= b.f; print(g()); }` → `check` rc=0 (ACCEPT), `run` printed the secret.

Root cause: `expr_source`'s `Expr::Call` sink/egress consumer resolves the
callee's returned-label through the single-candidate `fn_alias_of`, and
`fn_alias_of_d` had no `FieldAccess`/`Index` arm — the write side populated
`field_fn_identities` but the consumer never read it (row 6's
"producer-writes / consumer-ignores" disease).

Fix (PR #35, three additive parts):
1. `fn_alias_of_d` `FieldAccess`/`Index` arm resolving through
   `fn_identities_at_path_expr`, fail-closed prefer-dangerous.
2. Dynamic-index write MONOTONE widen (any `*`-segment path unions into every
   existing entry) across `field_fn_identities`/`field_closures` and
   `field_builtin_gate_tags`.
3. `expr_source` `Expr::Call` full multi-candidate per-lane check (closes the
   mixed secret+taint carrier into a taint-only sink surfaced in review).

GREEN (candidate `9f0e46ed…`): struct-field / array-element / map-key /
dynamic-index carriers all REJECT on both lanes; literal-construction and
direct controls still REJECT; clean twins ACCEPT.

Evidence:

| gate | result |
|---|---|
| security fixtures | **337/337** (7 new: 5 RED witnesses + 2 clean guards + mixed + egress-builtin guards) |
| language fixtures | **259/259** |
| stdlib fail-closed | **104/104** |
| walker completeness | PASS |
| docs drift | PASS, 0 drift |
| phase metrics | OK |
| native authoritative | **937 files, 0 mismatches, 0 disagreements** |
| cargo test --release | **1245 passed, 0 failed** (17 suites) |
| corpus verdict-diff vs pre-fix | 929 files, **0 flips, 0 timeouts** |
| manual hostile matrix | **59/59** |
| adversarial soundness hunt | 3 surfaces, **54 probes, 0 false accepts, 0 over-rejections** |

## 4. Explicitly published open residuals (index into `docs/CLAIMS.md`)

| residual | CLAIMS | status |
|---|---|---|
| REG-002 full in-process UNSAT-cert replay (z3-only) | item 6 | CONDITIONALLY MITIGATED (opt-in `ANUBIS_REQUIRE_NATIVE_PROOFS=1`); full replay named, not implemented |
| item 21 rows 1/2 — contract `requires` carrier / local-alias defeat | item 21 rows 1/2 | OPEN |
| item 21 row 3 — `obj.f()` direct method-call stored-closure carrier | item 21 row 3 + row-6 note | OPEN (pre-existing `Expr::CallExpr` path; confirmed on both pre/post-fix binaries) |
| item 21 rows 8/9/10 — unannotated array-literal / formal / return element-type precision | item 21 rows 8/9/10 | OPEN (annotated closed; unannotated needs element-type inference) |
| cross-module four-walker-family → 1 | Phase 2 waiver | WAIVED to Phase 4 scope; `walker families = 4`, non-increasing |
| Keychain/Secure Enclave, Softnet DNS-rebind, Metal-CI, TT-total/author-diversity, non-pow2 div/rem | item 4 | OPEN, permanent (OS/hardware/second-impl dependent) |

None of these is silently absorbed, weakened, or re-labelled. Each is a named,
dated `docs/CLAIMS.md` entry with a reproduction or boundary.

## 5. Exit criteria

| # | Criterion | Verdict |
|---|---|---|
| 1 | Every residual CLOSED-with-receipt or EXPLICITLY PUBLISHED | **PASS** — §3 closure + §4 register |
| 2 | Any closure is a bounded, CI-green, hunted, 0-flip slice | **PASS** — PR #35 |
| 3 | No residual silently absorbed / weakened / re-labelled | **PASS** — §4 |
| 4 | This receipt maps surface + closure + open register | **PASS** — this document |
| 5 | Canonical seal under admission rules; external lanes reported exactly | **EXTERNAL** — `PHASE_3_VM_SEAL_ATTEMPT_2026-08-15.md`; guest base image lacks `cargo`/`lean`; no host substitution |

Net: **4 PASS + 1 EXTERNAL (honestly declared).**

## 6. Adversarial soundness hunt

3 surfaces (`HuntWriteCarrierField` 25 probes, `HuntWriteCarrierIndex` 14,
`HuntWriteCarrierOverReject` 15) against the fixed binary, discriminator-verified.
**0 genuine false accepts, 0 over-rejections.** The single OVER_REJECT is an
intentional fail-closed over-approximation (a loop-written symbolic index
marks every slot). One CodeRabbit CRITICAL finding was verified NOT reachable
(field-stored builtins carry no identity name; sink-ness is tracked by the
independent `field_builtin_gate_tags` lane; every variant rejects on both
binaries) and locked with a guard fixture.

## 7. Verified vs believed vs skipped vs unknown

- **Verified:** the row-6 closure (RED→GREEN on the real binary), all gates
  above, the 929-file verdict-diff, the hostile matrix, the hunt.
- **Believed:** that the published-open residuals behave as their CLAIMS
  entries describe (each has a prior reproduction; not all re-run this phase).
- **Skipped / EXTERNAL:** the VZ self-host seal (guest toolchain missing).
- **Unknown:** whether the still-open item-21 mechanisms (rows 1/2/3/8/9/10)
  share a single deeper cause; not investigated this phase.

## 8. What the phase got wrong / bounded honestly

- The first row-6 fix (single-name arm) left a mixed secret+taint carrier
  sub-case open; a CodeRabbit review caught it and it was closed with the
  multi-candidate per-lane check before merge. Recorded because the first
  version would have shipped a narrower version of the same disease.
- Phase 4 does not claim the security surface is now total. Item 21 is
  **partially** retired (row 6 only). Green means no KNOWN defects.

## 9. Operator approval

Per the mandatory phase stop, this report requests operator sign-off to open
Completion Phase 5. The VZ seal (criterion 5) remains a real pending action,
not a waiver.

---

`STOPPED — awaiting operator direction for Completion Phase 5`
