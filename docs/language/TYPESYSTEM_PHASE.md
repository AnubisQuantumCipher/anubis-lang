# Type-System Phase — honest boundary & plan

This phase makes the Anubis type checker *real*: a genuine bidirectional inference engine over the
existing structured `Ty` foundation (`compiler/src/middle/ty.rs`), with captured generics, real
traits, and typed error propagation. It is **outward** work on the Rust middle-end, not deeper
self-host proof. Written before the work, per discipline — and corrected against the tree, because
the originating brief was partly stale.

## Honest boundary

**What this phase DOES claim**
- **Real bidirectional inference.** A `synth`/`check` core (union-find over `Ty::Var`) replaces the
  flat `infer_expr_type_scoped` (`middle/mod.rs:4247`), synthesizing `Call`/`Index`/`FieldAccess`
  results that today hard-return `None`, and doing genuine arm-join unification for `if`/`match`.
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
| Shadow-mode harness | **THIS SLICE** | `SemanticContext::emit` + `shadow`/`shadow_diags` + `scripts/run_shadow_diff.sh` |
| Bidirectional inference core | open | `Call`/`Index`/`FieldAccess` return `None` (`mod.rs`) |
| Captured generics | open | `skip_generic_params` discards (`frontend:2533`) |
| Real traits | open | no `collect_trait_env` sidecar yet |
| Typed `?` | partial | `ANUBIS_TRY_OUTSIDE_RESULT` exists; `ANUBIS_TRY_TYPE_MISMATCH` missing |

The brief's premise ("a float is aliased to u32"; "`Ty` is scaffolding the checker doesn't consume")
is stale: `Ty` is already load-bearing via `ty::assignable`, and floats are first-class.

## Invariants (green after every commit)

`cargo test -p anubis-compiler --lib` = 367 · `clippy --all-targets -D warnings` clean ·
turing-core 13/13 byte-exact · language-core 26/26 · security fixtures · PCA/evidence gate ·
selfhost 9/9 with the binary fixpoint recomputed to the same `c640badd` · dogfood 3/3 · DDC 34/34 ·
repro 6/6. **Frozen & untouched:** the `ty_parity` oracle (`ty.rs:675`), its `ref_*` block and
`VOCAB`. **AST string boundary:** no existing field's type changes; new info rides on new defaulted
fields or sidecar tables; no new key in `selfhost_schema::project_item` (would move the fixpoint).

## Shadow-mode method (not optional)

Every check that adds rejection power lands in **shadow mode** first: it emits through
`SemanticContext::emit(.., shadow_gated=true)`, so under `ANUBIS_SHADOW_TYPES=1` its would-be
rejections are logged (`ANUBIS_SHADOW: …` on stderr) but never enter the enforcing `diagnostics`
Err-gate. `scripts/run_shadow_diff.sh` runs the whole corpus and classifies each would-be rejection
as EXPECTED (fixture is `// EXPECT: FAIL` with a matching `// ERROR_CONTAINS`) or UNEXPECTED (a
currently-accepted program). A check is promoted to enforcing (`shadow_gated=false`, one-line flip)
**only when UNEXPECTED = 0**. Corpus baseline captured at harness time: **111 accept / 28 reject**.

## Next-phase seam (deliberately set up, not done here)

Keep the unification core small and dependency-light so porting `Ty` + unification + the checker into
Anubis is the next phase's mechanical job. The review's suggested opener for that phase — port the
`ty_parity` oracle and a few core predicates, prove the self-host still type-checks itself — belongs
at the front of the *port* phase, not this one.
