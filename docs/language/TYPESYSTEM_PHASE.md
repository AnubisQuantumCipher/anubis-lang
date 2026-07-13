# Type-System Phase — honest boundary & plan

This phase makes the Anubis type checker *real*: a genuine bidirectional inference engine over the
existing structured `Ty` foundation (`compiler/src/middle/ty.rs`), with captured generics, real
traits, and typed error propagation. It is **outward** work on the Rust middle-end, not deeper
self-host proof. Written before the work, per discipline — and corrected against the tree, because
the originating brief was partly stale.

## Honest boundary

**What this phase DOES claim**
- **Real bidirectional inference — LANDED (this slice).** A self-contained `synth`/`check` core with a
  `Vec`-backed union-find over `Ty::Var` now lives in `middle/ty.rs` (`InferCtx`, `InferEnv`, `synth`,
  `unify`, `arm_join_conflict`, `check_mismatch`). It synthesizes `Call`/`Index`/`FieldAccess` — which
  the flat inference hard-returned `None` for — and does genuine arm-join unification for `if`/`match`,
  so `if true { "a" } else { 1 }` is now the conflict it is. The legacy flat `infer_expr_type_scoped`
  is deliberately retained UNCHANGED as the enforcing substrate for the pre-existing arg/return/let-init
  checks; the new synthesis reaches the verdict path only through `check_mismatch_scoped` +
  `arm_join_conflict_scoped`, every one of them entered in shadow mode first. Promoted to enforcing (the
  shadow diff reported UNEXPECTED=0): `ANUBIS_ARM_TYPE_CONFLICT` (if/match arm-join), and the
  `Call`/`Index`/`FieldAccess` `ANUBIS_TYPE_MISMATCH`/`ANUBIS_RETURN_TYPE_MISMATCH` that synthesis can
  now see. **The port of the unifier and checker into Anubis remains the next phase** — the core was
  kept dependency-light (plain maps, no closures/trait objects) precisely to make that port mechanical.
- **Captured generics.** Type parameters captured onto a **new defaulted** AST field (not discarded
  by `skip_generic_params`, `frontend:2533`); monomorphization is checker-only, codegen stays erased.
- **Real traits.** A read-only `collect_trait_env` sidecar (peer of `collect_trait_defaults`,
  `frontend:541`) carrying coherence/missing-method/bound checks; the backend-facing desugar stays
  byte-identical so the dogfood fixpoint does not move.
- **Typed `?`.** `?` checked against the enclosing function's declared return type (extends the
  existing static `ANUBIS_TRY_OUTSIDE_RESULT` at `mod.rs:788`).

**What this phase does NOT claim**
- **The checker stays Rust.** No Anubis-language port of `Ty`/unification/checker this phase — that is
  the *next* phase, set up by keeping the unification core deliberately portable. `anubis_sh.anb` and
  the frozen JSON projection are untouched.
- **The solver still declines floats.** Floats stay opaque in QF_BV/QF_ABV; richer *typing* of floats
  buys the solver no float reasoning. `ANUBIS_FLOAT_CONTRACT_UNMODELED` remains the honest diagnostic.
- **New checks are additive rejection power only.** Every new diagnostic is a *tightening*, gated
  through shadow mode until proven corpus-clean. No existing accept is silently turned into a reject;
  no existing reject is relaxed; no solver decline becomes a false proof.
- **Codegen is additive-only.** Types are erased at `run.rs` (`lower_program_*`); emitted signatures
  stay `fn(..: AnubisValue) -> AnubisValue`.

**Fail-closed, in two non-interchangeable directions**
- **Checker → accept on unknown.** `Any` / unification-var / generic-param / `Unknown` / undecidable
  resolves toward *accept* — a working dynamic program is never rejected. Unification *failure*
  degrades to `None` (no diagnostic), never to a spurious reject.
- **Solver → decline on unmodelable.** Anything the bit-vector layer cannot faithfully model stays out
  of the solver and is left to runtime — never a fabricated discharge.

## Corrected workstream state (verified against the tree, not the brief)

| WS | State | Evidence |
|----|-------|----------|
| Floats (first-class, u32-aliasing bug) | **DONE** (prior sessions) | `8e9d450`, `752f7dc`; `let x: u32 = 3.14` → `ANUBIS_TYPE_MISMATCH`; `ty::assignable` (`ty.rs:320`) load-bearing |
| Shadow-mode harness | **DONE** (prior slice) | `SemanticContext::emit` + `shadow`/`shadow_diags` + `scripts/run_shadow_diff.sh` |
| Bidirectional inference core | **DONE** (this slice) | `middle/ty.rs` union-find `synth`/`check`/`unify`; `Call`/`Index`/`FieldAccess` synthesized; arm-join enforcing |
| `emit` shadow-gating correctness | **FIXED** (this slice) | `emit` no longer routes shadow-gated diags to the Err-gate with shadow off (was a latent inversion in dead code) |
| Captured generics | open (after this slice) | `skip_generic_params` discards (`frontend:2533`) |
| Real traits | open (after this slice) | no `collect_trait_env` sidecar yet |
| Typed `?` | open (after this slice) | `ANUBIS_TRY_OUTSIDE_RESULT` exists; `ANUBIS_TRY_TYPE_MISMATCH` missing |

The brief's premise ("a float is aliased to u32"; "`Ty` is scaffolding the checker doesn't consume")
is stale: `Ty` is already load-bearing via `ty::assignable`, and floats are first-class.

## Invariants (green after every commit)

`cargo test -p anubis-compiler --lib` = 372 (367 + 5 new `ty` inference-core tests) ·
`clippy --all-targets -D warnings` clean · turing-core 13/13 byte-exact · language-core 33/33
(26 unchanged + 7 new inference-core fixtures) · security fixtures 17/17 · PCA/evidence gate ·
selfhost 9/9 with the binary fixpoint recomputed to the same `c640badd` · dogfood 3/3 · DDC 34/34 ·
repro 6/6. **Frozen & untouched:** the `ty_parity` oracle, its `ref_*` block and `VOCAB` (the new
inference-core tests sit before it, contiguous block intact). **AST string boundary:** no existing
field's type changes; `struct_fields` is a NEW sidecar table registered in pass 1 (not an AST field);
no new key in `selfhost_schema::project_item` (would move the fixpoint) — the seal reconfirms `c640badd`.

## Shadow-mode method (not optional)

Every check that adds rejection power lands in **shadow mode** first: it emits through
`SemanticContext::emit(.., shadow_gated=true)`, so under `ANUBIS_SHADOW_TYPES=1` its would-be
rejections are logged (`ANUBIS_SHADOW: …` on stderr) but never enter the enforcing `diagnostics`
Err-gate.

> **Ledger correction (surfaced by the inference-core slice).** The shadow harness's `emit` shipped
> in `bbf3117` with a latent inversion — `if shadow_gated && self.shadow { shadow_diags } else {
> diagnostics }` — which routed a shadow-gated diagnostic to the *enforcing* Err-gate whenever shadow
> was off, contradicting its own "verdict path bit-identical whether shadow on/off" contract. It was
> harmless only because nothing had called `emit` yet (dead code). It was corrected in **`cfba865`**
> to drop-when-off. **The harness contract is therefore only truly load-bearing from `cfba865`
> onward**; any "shadow-clean" claim made before that commit rested on an `emit` that mis-routed when
> shadow was off. This is a note for honesty, not a redo: no check had been wired through `emit`
> before the inference slice, so no earlier verdict was actually affected — but the *guarantee* the
> word "shadow-clean" implies did not hold until `cfba865`. `scripts/run_shadow_diff.sh` runs the whole corpus and classifies each would-be rejection
as EXPECTED (fixture is `// EXPECT: FAIL` with a matching `// ERROR_CONTAINS`) or UNEXPECTED (a
currently-accepted program). A check is promoted to enforcing (`shadow_gated=false`, one-line flip)
**only when UNEXPECTED = 0**. Corpus baseline captured at harness time: **111 accept / 28 reject**
(139 programs). This slice's diff: **total=0** shadow diagnostics — the inference core rejects zero
currently-accepted programs — verified verdict-neutral by a pre/post binary comparison over the now
**140-program** corpus (**112 accept / 28 reject** identical before and after; the 112-vs-111 is one
added fixture, not a regression). All three promoted checks were flipped only after that zero diff.

## Next-phase seam (deliberately set up, not done here)

Keep the unification core small and dependency-light so porting `Ty` + unification + the checker into
Anubis is the next phase's mechanical job. The review's suggested opener for that phase — port the
`ty_parity` oracle and a few core predicates, prove the self-host still type-checks itself — belongs
at the front of the *port* phase, not this one.
