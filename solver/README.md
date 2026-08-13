# anubis-solver — a native SMT decision procedure

**Purpose.** Anubis's contract checker proves `requires`/`ensures`/`assert` obligations with an SMT
solver. Until now that solver was **z3** — the single largest *trusted third party* in the whole
system. If z3 ever answered `unsat` (i.e. "your obligation is proven") incorrectly, Anubis would
certify a false contract. This crate exists to **remove that trust**: it is a from-scratch SMT
decision procedure for the bit-vector (integer) fragment, with **zero external solver dependency**
(std only). It is Phase 7 of the roadmap — *minimize the Trusted Computing Base*.

## What it decides

The integer lane: fixed-width bit-vectors (`QF_BV`), which is what dominates real contract obligations
(`x + 1 > x`, `abs(x) < 100`, the u32-mask range facts, comparisons, wrapping arithmetic). Plus two
theories that *lower to that same BV pipeline* — no new gates:

- the **float comparison** subset of `QF_FP`: a `Float64` is a `BitVec 64`, so `fp.lt/leq/gt/geq/eq`,
  the `isNaN/isInfinite/isZero` classifiers, and exact `fp.neg/abs` are lowered via the monotonic-key
  transform (`fp.rs`). Float **arithmetic** (`fp.add/mul/div/…`, which rounds) is declined.
- the **string equality** subset of `QF_S`: string-equality is equality logic, so each distinct string
  literal interns to a distinct constant and each `String` var becomes a free bit-vector, with `=` as
  BV equality (`parse.rs`, 32-bit ids). String **operations** (`str.len/++/contains/…`) are declined.

Arrays, float arithmetic, and non-equality string ops fall back to z3.

## Pipeline

```
SMT-LIB2 text ──parse──▶ bv::Formula ──bit-blast──▶ CNF ──SAT──▶ verdict
   parse.rs               bv.rs           blast.rs      sat.rs
```

- **`parse.rs`** — a deliberately *conservative* SMT-LIB2 reader for exactly the fragment
  `compiler/src/middle/mod.rs` emits. Any non-BV sort, unsupported op, or malformed input makes the
  whole parse return `None`. It never guesses.
- **`bv.rs`** — the QF_BV formula AST: bit-vector `Term`s and boolean `Pred`s, with a structural
  `Term::width` and an independent concrete evaluator (`eval`) used for the SAT-model replay.
- **`fp.rs`** — IEEE-754 `Float64 → QF_BV` lowering for the comparison subset: the monotonic
  ordering-key transform (`sign ? ~x : x^0x8000…0`) plus NaN/±0 special-casing, so unsigned BV `<` on
  the key equals IEEE `<`. Produces ordinary `bv` terms/preds — nothing new for the blaster to trust.
- **`blast.rs`** — a Tseitin bit-blaster. Each term becomes a `Vec<Lit>` (LSB first), each predicate a
  single `Lit`. Supported: `const`/`var`, `and`/`or`/`xor`/`not`, `neg`, ripple-carry `add`/`sub`,
  **constant-multiplier `mul`** (`x * c` = shift-and-add over c's set bits, mod 2^w), all eight
  signed+unsigned comparisons, `=`, `extract`/`concat`/`{zero,sign}_extend`, `ite`, and shifts by
  **any** amount — constant (direct wiring) or **variable** (a log-depth barrel shifter of `mux`es).
  Gate semantics match `run.rs` and `formal/Anubis/Encoding.lean`. Only **variable × variable** `mul`
  and `div`/`rem` are still declined (→ `None` → z3).
- **`sat.rs`** — a **CDCL** SAT engine (watched literals, 1-UIP clause learning, VSIDS, Luby restarts)
  bounded by a *conflict* budget; over budget → `Unknown` (decline). `Unsat` is only ever returned via a
  conflict at decision level 0 (a root refutation), and every Unsat carries a self-contained
  **RUP/LRAT certificate** (original DIMACS CNF + derived clauses ending in empty).
- **`lrat.rs`** — a pure, independent **RUP** checker (LRAT-shaped emission: clause additions only;
  **no deletion steps**, no RAT/DRAT). No CDCL, no watches, no solver state. `check_proof` accepts a
  certificate only if every clause is well-formed (DIMACS vars in range, no zero/complementary
  pairs), every step is reverse-unit-propagation w.r.t. the accumulated database, and the terminal
  step is the empty clause. Adversarial forgeries (truncated, SAT formulas with forged empty, non-RUP
  learned clauses, wrong variable numbering) are rejected.

## The one entry point

```rust
anubis_solver::native_check_sat(smt: &str) -> Option<bool>
```

- `Some(true)`  — **SAT**: a model of `assumptions ∧ ¬property` exists → the property is **not** proven
  (a counterexample). Same meaning as z3 answering `sat`. Model is replayed by independent `eval`.
- `Some(false)` — **UNSAT**: no model → the property is **proven**. Same as z3 `unsat`. Returned only
  when CDCL emits a certificate **and** `lrat::check_proof` accepts it. Missing/invalid cert → `None`.
- `None`        — the solver **declines**: out-of-fragment, undecided within budget, or Unsat cert
  failed verification → defer to z3.

**Soundness is structural.** A definite Unsat is returned *only* when the input parsed as pure QF_BV
(or a BV-lowered subset), every term bit-blasted with a supported gate, the SAT engine produced a
root refutation, **and** the independent RUP checker verified the certificate. SAT requires model
replay. Anything else is `None`.

## Rollout: shadow → authoritative default (flipped 2026-07-25)

- **Default (native-authoritative):** native *decides* every **proof-backed fragment** obligation
  (see `fragment.rs`); z3 is a fail-closed cross-check when present. With z3 **absent**, native alone
  carries that fragment — Unsat only with a **verified RUP certificate**. Gate:
  `scripts/run_native_authoritative_gate.sh`.
- **Opt out:** `ANUBIS_NATIVE_AUTHORITATIVE=0` restores z3-only authority (pre-flip).
- **`ANUBIS_NATIVE_SHADOW=1`:** still available for explicit shadow logging / disagreement hunts.
- **Division stays deferred by design. Var×var mul is ADMITTED to the fragment but usually
  DECLINES on cost** — and those are two different facts this README used to collapse into one.

  *Admission:* `MulVar` is in `PROVEN_OP_TAGS` (`solver/src/fragment.rs`), TIER-1 via
  `mulVar_correct` (`formal/Anubis/BitBlast.lean:691`). A var×var product is inside the Lean-proven
  blast family, so a native verdict on one **may ride as authoritative**.

  *Decision:* native still has to afford it. Measured on the commutativity miter `a*b == b*a` at
  **u8**, with z3 hidden so nothing could mask the result:

  | budget | vars | clauses | conflicts | outcome |
  |---|---:|---:|---:|---|
  | default (20k) | 22,334 | 74,730 | 20,001 | `declined_budget_or_cert` |
  | raised to 2M | 22,334 | 74,730 | 120,163 | `declined_cert_work` (7.4M) |

  So it blasts fine and then declines — first on the conflict budget, and with that lifted, on
  certificate work. In practice a var×var obligation **defers to z3**.

  Saying only "deferred by design" hid the admission; saying only "admitted" (which this README
  briefly did, before the numbers above were measured) implies native decides it, and it does not.
  Trust-boundary prose has to survive someone hiding z3 and re-running.

## How we know it's correct (and why it can't cause a false accept)

1. **Shadow, don't trust.** During rollout **z3 stays the authority by default** — the native answer is
   only *compared* under shadow/authoritative-with-z3, and any disagreement fails closed. In
   z3-absent authoritative mode, trust rests on proven blast + **verified Unsat certificate** (or
   SAT model replay), not on an uncheckable CDCL claim.
   - **SAT is self-replayed.** A native `sat` (counterexample) is returned only after the reconstructed
     model is re-verified by an *independent* concrete evaluator (`bv::Formula::eval`, sharing no code
     with the bit-blaster or SAT engine).
   - **UNSAT is certified.** A native `unsat` is returned only after `lrat::check_proof` accepts the
     CDCL-emitted RUP derivation ending in the empty clause.
2. **Differential vs z3.** `tests/differential.rs` runs thousands of random QF_BV formulas plus
   hand-crafted 64-bit edge cases (wrapping overflow, signed `MIN`, the u32 mask, extract, sign-extend)
   through both native and z3 and asserts they agree wherever native decides. Current: **2000 small +
   600 wide (16/24/32-bit) + 7 edge = 0 disagreements, 0 deferred** — with CDCL the wide 32-bit regime
   (what real `u32` obligations compile to) is fully *decided*, not deferred.
3. **Machine-checked core.** `formal/Anubis/BitBlast.lean` proves the ripple-carry adder — from which
   every `bvadd`, and via two's complement every `bvsub`/`bvneg`, is built — computes *true integer
   addition* (`rippleCarry_spec`), chaining to `Encoding.lean` (bvadd = `wrapping_add` = runtime); the
   unsigned comparator the blaster emits is exactly `<` (`ult_correct`: `ult a b = true ↔ ⟦a⟧ < ⟦b⟧`,
   via the subtractor's carry-out); and the **signed** comparator's flip-MSB-then-unsigned trick is
   exactly two's-complement signed `<` (`slt_correct`: `slt a b = true ↔ toIntW a < toIntW b`, via the
   offset-binary identity `flipMsb_val`). All depend only on the three standard Lean core axioms — no
   `sorry`/`admit`/`native_decide`.

**Certificate residual (closed):** CDCL Unsat emits RUP and is independently verified.
**Default flip (done 2026-07-25):** compiler defaults to native-authoritative; opt out with `=0`.
Division remains deferred forever-or-until-proven. Var×var mul is PROVEN (`mulVar_correct`) and
admitted to the fragment (2026-07-25), but declines on cost for miter shapes and defers to z3 in
practice — admission and decision are separate; see the measured table above.

## Status / next

- ✅ parser, bit-blaster, `native_check_sat`, differential + corpus shadow.
- ✅ adder **and** comparator (and the rest of the proven fragment) machine-checked in Lean.
- ✅ CDCL engine (watched literals, 1-UIP learning, VSIDS, Luby restarts).
- ✅ **RUP Unsat certificates** (LRAT-shaped, no deletions) + independent pure checker (`lrat.rs`);
  Unsat fail-closed without a verifying cert.
- ✅ **Default flip (2026-07-25):** native-authoritative by default; opt-out `ANUBIS_NATIVE_AUTHORITATIVE=0`.
- ⬜ division remains deferred by design; float arithmetic / non-eq string ops still z3.
- ✅ **var×var mul admitted to the fragment (2026-07-25):** TIER-1 via `mulVar_correct`. Native
  still declines the miter shape on cost and defers to z3 — gated by
  `tests/fixtures/language_core/var_mul_obligation_discharged.anb`.
