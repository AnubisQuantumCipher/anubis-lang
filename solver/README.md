# anubis-solver — a native SMT decision procedure

**Purpose.** Anubis's contract checker proves `requires`/`ensures`/`assert` obligations with an SMT
solver. Until now that solver was **z3** — the single largest *trusted third party* in the whole
system. If z3 ever answered `unsat` (i.e. "your obligation is proven") incorrectly, Anubis would
certify a false contract. This crate exists to **remove that trust**: it is a from-scratch SMT
decision procedure for the bit-vector (integer) fragment, with **zero external solver dependency**
(std only). It is Phase 7 of the roadmap — *minimize the Trusted Computing Base*.

## What it decides

The integer lane: fixed-width bit-vectors (`QF_BV`), which is what dominates real contract obligations
(`x + 1 > x`, `abs(x) < 100`, the u32-mask range facts, comparisons, wrapping arithmetic). Plus the
**float comparison** subset of `QF_FP`: a `Float64` is a `BitVec 64`, so `fp.lt/leq/gt/geq/eq` and the
`isNaN/isInfinite/isZero` classifiers are *lowered* to bit-vector formulas (`fp.rs`, the monotonic-key
transform) and decided by the same BV pipeline — no rounding, no new gates. Float **arithmetic**
(`fp.add/mul/div/…`, which rounds), strings, and arrays are declined and fall back to z3.

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
  conflict at decision level 0 (a root refutation), so a "proof" is sound by construction.

## The one entry point

```rust
anubis_solver::native_check_sat(smt: &str) -> Option<bool>
```

- `Some(true)`  — **SAT**: a model of `assumptions ∧ ¬property` exists → the property is **not** proven
  (a counterexample). Same meaning as z3 answering `sat`.
- `Some(false)` — **UNSAT**: no model → the property is **proven**. Same as z3 `unsat`.
- `None`        — the solver **declines**: out-of-fragment, or undecided within budget → defer to z3.

**Soundness is structural.** A definite verdict is returned *only* when the input parsed as pure
QF_BV, every term bit-blasted with a supported gate, and the SAT engine actually decided the CNF.
Anything else is `None`. So this can sit in front of z3 without ever changing a verdict z3 would not
also give — *provided the bit-blaster is correct*, which is what the validation below establishes.

## Rollout: shadow → opt-in authoritative → default flip

There are three compiler modes, each a strictly bolder step, all gated:

- **Default (shadow off):** z3 decides everything; the stock pipeline, unchanged.
- **`ANUBIS_NATIVE_SHADOW=1`:** native runs *alongside* z3 on every obligation (now including the
  primary proof stream), z3 stays authoritative, disagreements fail `scripts/run_native_shadow_gate.sh`.
  Current: **243/293 real obligations decided by native, 0 disagreements** (the 50 deferrals are the
  non-BV float/string/array obligations).
- **`ANUBIS_NATIVE_AUTHORITATIVE=1` (opt-in):** native *decides* every QF_BV obligation it can, and z3
  is consulted only as a fail-closed cross-check while present. With z3 **absent**, native alone
  carries the integer lane — the actual TCB drop. `scripts/run_native_authoritative_gate.sh` proves
  this is safe: **verdict-equivalent to z3 over the whole corpus (326 files, 0 mismatches, 0
  disagreements)**, and native alone (z3 hidden) proves the passing int fixture and rejects the
  violating one, while the default mode without z3 fails (z3 was load-bearing). The default flip
  follows after soak.

## How we know it's correct (and why it can't cause a false accept)

1. **Shadow, don't trust.** During rollout **z3 stays the authority** — the native answer is only
   *compared*, and any disagreement fails the cross-check gate. In authoritative mode native's verdict
   is used but every one is still cross-checked against z3 while it is present (a disagreement fails
   *closed* — reject). A native bug is *caught*, never silently *trusted*.
   - **SAT is self-replayed.** A native `sat` (counterexample) is returned only after the reconstructed
     model is re-verified by an *independent* concrete evaluator (`bv::Formula::eval`, sharing no code
     with the bit-blaster or SAT engine) — the native equivalent of the z3 counterexample replay.
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

Only once the cross-check gate is sustained at zero disagreements **and** the bit-blaster is fully
mechanized does the compiler flip to native-authoritative, dropping z3 from the trusted base for the
integer lane.

## Status / next

- ✅ parser, bit-blaster, `native_check_sat`, differential + corpus shadow.
- ✅ adder **and** comparator machine-checked in Lean (`rippleCarry_spec`, `ult_correct`).
- ✅ CDCL engine (watched literals, 1-UIP learning, VSIDS, Luby restarts) — wide 32-bit formulas
  decide in-budget (600/600, 0 deferred).
- ⬜ native lanes for floats / strings / arrays; the z3-authoritative flip.
