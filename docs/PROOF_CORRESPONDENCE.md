# Source-to-proof correspondence, and what is still in the TCB

**What this document is for.** Anubis ships 199 machine-checked Lean theorems. Those theorems are
about **models** — a bit-blasting relation, an effect lattice, a non-interference property. They are
not, and do not claim to be, proofs that the production Rust *implements* those models. Between an
Anubis source file and a verified refutation there are seven links, and only some of them carry
evidence.

This page names every link and states its **actual** evidence. Where a link has none, it says so and
puts the component in the TCB list at the bottom. `scripts/run_proof_correspondence_gate.sh` fails if
a row here cites a Lean theorem or a gate that no longer exists, so the map cannot quietly rot into
fiction.

Counts here are re-derived by the gate; do not hand-edit them.

---

## The chain

```
Anubis AST → VC construction → SMT text → native parser → bit-blaster/CNF → certificate → runtime
```

| # | Link | Evidence | Status |
|---|---|---|---|
| 1 | **AST → VC construction** | none | **TCB** |
| 2 | **VC → SMT text** | none directly; the emitted text is exercised end-to-end by every fixture gate | **TCB** |
| 3 | **SMT text → native parser** | `run_native_shadow_gate.sh` (whole corpus, native vs z3, 0 disagreements required) — an independent oracle on the *same input text* | differential only |
| 4 | **parser → bit-blaster / CNF** | `formal/Anubis/BitBlast.lean` (44 theorems, incl. `mulVar_correct`, `mulConst_correct`, `shlConst`, `barrelShl`) + `solver/src/fragment.rs` `PROVEN_OP_TAGS`, a TOTAL match so an unproven `Term` variant fails to compile rather than riding as authoritative | **model proven**, Rust↔model unproven |
| 5 | **CNF → certificate** | `lrat::check_proof` in-process, and — new — the published DIMACS + DRAT re-checked by `scripts/verify_proof_bundle.py`, an independent RUP checker in another language sharing no code with the solver | **independently re-checkable** |
| 6 | **certificate → verdict** | fail-closed by construction: no `Unsat` leaves the solver without an accepted certificate (`solver/src/lib.rs`); `run_native_authoritative_gate.sh` requires verdict-equivalence over the whole corpus | gated |
| 7 | **verdict → runtime behavior** | `run_check_run_parity_gate.sh` asserts, for every corpus program, that the `check` verdict equals `run`'s native-execution preflight verdict — without executing user code | gated |

### What each status means

- **TCB** — no evidence links this step to a proof. A defect here produces a wrong VC that everything
  downstream then faithfully proves. This is the honest weak point.
- **differential only** — two independent implementations agree on a corpus. That is real evidence and
  it is not a proof: both can be wrong the same way, and a corpus is not all inputs.
- **model proven** — a Lean theorem establishes the property *of the model*. Whether the Rust matches
  the model is exactly what is unproven.
- **independently re-checkable** — a third party can re-derive the result with neither the Anubis
  binary nor its solver.

---

## The TCB, enumerated

Everything below is trusted, not proven. A defect in any of it can produce a green `anubis check` on
a program that violates its contracts.

1. **Rust lowering (AST → VC).** The construction of verification conditions from the AST. Link 1.
2. **SMT emission.** `expr_to_smt` and the obligation printers. A mis-encoded operator yields a
   well-formed query about the wrong thing. Link 2.
3. **Modelability decisions.** `is_int_modelable`, `solver_int_vars`, the float/string/array
   modelable predicates. A value wrongly judged *unmodelable* silently drops its obligation —
   `lit_neg` carried an unchecked negation for exactly this reason until its domain was stated.
4. **Fragment admission.** `fragment::is_proven_authoritative` decides which terms may ride as
   authoritative. Bounded by a total match (a new `Term` variant breaks the build), but the mapping
   from a tag to *the Lean theorem that justifies it* is by review, not by machine.
5. **The SMT parser.** `solver/src/parse.rs`. It is the boundary between text and formula; a parser
   that reads a query differently than z3 does breaks the differential's meaning. Link 3.
6. **Runtime semantics.** The interpreter and native lowering — that executing a program actually
   behaves as the checked model says. Link 7 is gated on the *verdict*, not on execution semantics.
7. **The Lean toolchain and `lake build` itself**, plus this repo's own gate scripts. Bound to the
   pin (`scripts/publish_pin.sh` covers `formal/**` and the three gate scripts) so a change to them
   invalidates the pin — bound, still trusted.

## What would move a row out of the TCB

Not a plan, a definition — so a future claim can be checked against it:

- **Link 1/2** needs a mechanized semantics for the Anubis AST and a proof that VC construction is
  sound with respect to it. This is the deepest item and is not started.
- **Link 3** would need the parser verified against the same grammar the emitter targets, or a
  round-trip proof (`emit ∘ parse = id` on the modeled fragment).
- **Link 4** would need extraction, or a proof that the Rust blaster refines the Lean relation.

Until then: **`anubis check` PASS means no *known* way to violate the stated contracts, effects,
capabilities or information-flow — and everything it could not decide, it refused.** That sentence is
exactly as strong as the weakest link above, which is link 1.
