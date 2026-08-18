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

---

## Production-linked SecurityLabel slice (Completion Blueprint Phase 8, Slice 1)

Everything above is about the seven-link main chain. The **entire chain remains unmoved by this
slice** — Links 1, 2, and 5 of the TCB are unchanged; there is still no mechanized AST semantics, no
verified VC construction, and no round-trip proof for the SMT parser. What this slice does move is
a single **bounded, off-chain component** whose behavior is nevertheless load-bearing for security
downstream: the six methods on `compiler::middle::security_label::SecurityLabel` that the Phase 3
lattice separation exposes.

### The claim, stated tightly

> The finite, security-relevant behavior of the production Rust `SecurityLabel::{from_legacy_taint,
> from_legacy_secret, join, declassified_by, to_legacy_taint, to_legacy_secret}` methods agrees
> byte-for-byte with a Lean 4 model of the same operations over the complete declared abstraction,
> and any future divergence fails a reproducible gate.

That is **all** it claims. In particular, this slice does not prove:

- that `SecurityLabel` is the RIGHT abstraction for the wider taint / secret / declassify problem;
- that Anubis's checker calls these methods on the right operands at the right places (that path
  runs through Link 1's untouched Rust lowering, which stays TCB);
- that the whole Rust runtime honors the labels the checker computes;
- any property of any other module.

### How the link is closed

Two independent observers, one byte comparison:

- **Rust side.** `anubis_compiler::observe_security_label_correspondence` (public re-export from
  `compiler/src/middle/mod.rs`) walks the declared finite abstraction and, for every `(op, args)`
  tuple, calls the actual `SecurityLabel` method above and emits one canonical TSV row. It is
  driven from the integration test at
  `compiler/tests/security_label_correspondence_observer.rs`, which the correspondence gate runs
  via `cargo test -p anubis-compiler --test security_label_correspondence_observer` under a
  private `CARGO_TARGET_DIR`. There is no shadow reimplementation — the observer's outputs are
  exactly what the production methods return.

- **Lean side.** `Anubis.SecurityLabelObserver.main` (built as the `lean_exe`
  `security_label_observer` in `formal/lakefile.toml`) prints
  `Anubis.SecurityLabel.observationRows`, the SAME corpus whose length and no-duplicates are
  machine-locked by the theorems `observationRows_length` and `observationRows_nodup` in
  `formal/Anubis/SecurityLabel.lean`. `lake build` covers this file, so the emitter cannot drift
  from the theorems that prove things about it.

- **The gate.** `scripts/run_security_label_correspondence_gate.sh` runs both observers to
  independent per-PID paths, schema-validates each stream (exact row count, per-op count,
  four tab-separated fields, unique keys, no unknown op), and then `cmp`s the two files
  byte-for-byte. Its declared verdict is either
  `SECURITY_LABEL_CORRESPONDENCE_GATE: PASS (rows=83 rust_sha=<sha> lean_sha=<sha> out=<dir>)`
  or the corresponding `FAIL`. `--self-test` mutates temporary copies of the observations across
  nine defect classes (Rust row altered, Lean row altered, row deleted, row duplicated, input
  class omitted, `Unknown → Clean` adapter mutation, fake full-record commutativity claim,
  malformed row, unknown op) and refuses to pass unless the gate rejects every one.

### The finite abstraction and its completeness

None of the six operations reads a character of a `source` or `reason` string. `from_legacy_taint`
matches on `(bool, is_some)`; the `Labeled` branch echoes the `Option<String>` unchanged and the
`Unknown` branch emits a FIXED constant string. `from_legacy_secret` reads only a bool. `join`
matches on variant tags and combines `Option`s with `Option::or`, a presence-preserving operation.
`declassified_by` matches on `Labeled { source: Some(_) }` guarded by a bool. `to_legacy_taint`
echoes `source` through for `Labeled`, emits `Some("unknown-label")` for `Unknown`, and
`(false, None)` for `Clean`. `to_legacy_secret` reads only the variant tag.

A two-token source alphabet (`"s1"`, `"s2"`) and a two-token reason alphabet (`"r1"`, `"r2"`) —
plus `None` and the two production-constant strings (`"legacy-shape: taint_source without tainted"`
and `"unknown-label"`) — is therefore complete for the six operations. Two representative tokens
per position are the MINIMUM needed to expose join's left-biased `Option::or` fallback, which
`join_full_not_commutative` and `join_full_not_commutative_unknown` in the Lean module machine-lock
as a WITNESS that full-record commutativity is FALSE. A one-token corpus would silently accept
full commutativity as a theorem.

The exact corpus is 83 rows: 4 `from_legacy_taint` + 2 `from_legacy_secret` + 49 `join`
(`abstractLabels × abstractLabels`) + 14 `declassified_by` (`abstractLabels × Bool`) + 7
`to_legacy_taint` + 7 `to_legacy_secret`. The gate refuses any stream whose row count differs
from 83 or whose per-op count differs from `(4, 2, 49, 14, 7, 7)`.

### What this slice does NOT do to the TCB

All seven items in the TCB list above remain trusted, not proven. Adding a `pub fn` at the
compiler crate root that walks the abstraction did not change one line of the six methods it
observes; the observer is a witness harness for existing behavior, not a re-implementation. The
`SecurityLabel` type itself stays `pub(crate)` — external consumers only ever see a
`std::io::Write` sink and a row-count `usize`, not the type or its variants.

### Where a future slice could go

Not a plan, a definition — analogous to "What would move a row out of the TCB" above:

- **Extend the linked surface.** Bring the two `set_taint_label` / `set_secret_label` producers
  and the `sync_labels_from_legacy` shape check into the same abstraction, so the lattice
  transfer through a `ScopeBinding` (Phase 3 Slice 3) has the same production↔Lean link.
- **Link the walker.** The value-flow walkers that CALL these methods are the accept-biased
  surface the Completion Blueprint's Phase 3 named. A slice that closed the walker↔label
  correspondence would move the accept surface out of "we ran the tests" and into "we ran the
  tests AND the walker refines a Lean specification."
- **Toolchain surface.** `lake exe` and `cargo test` are still in the TCB. A future round could
  bind the observer binaries to a content-addressed pin the way `scripts/publish_pin.sh` binds
  `anubis` itself, so a rebuild between measurement and comparison cannot ride.

Until any of those lands, the correspondence claim above is exactly what the pair of observers
establishes — no wider, no narrower.
