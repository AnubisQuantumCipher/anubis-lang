//! Proof-backed fragment gate for the native-authoritative lane.
//!
//! When the compiler runs the native solver as the AUTHORITY (`ANUBIS_NATIVE_AUTHORITATIVE=1`, and in
//! particular the z3-absent window where no cross-check fires), a native `Unsat` is a PROOF — and a
//! proof is only as sound as the bit-blast that produced it. This module restricts the authoritative
//! lane to the sub-language whose bit-blast wiring is MACHINE-CHECKED in `formal/Anubis/BitBlast.lean`.
//! If an obligation touches any operation whose wiring is not proven, [`is_proven_authoritative`]
//! returns `false` and the caller declines (`None` → defer to z3).
//!
//! Deferring is ALWAYS sound: the gate only ever SHRINKS what native decides — it never changes a
//! verdict native would have returned, it only turns some `Some` into `None`. So wiring it in front of
//! the blaster cannot introduce a false accept; it can only remove a native decision that rested on an
//! unproven blast.
//!
//! The walk is TOTAL and CONSERVATIVE:
//! * an explicit per-constructor match with **no wildcard arm**, so a newly added `Term`/`Pred` variant
//!   fails to COMPILE here rather than silently riding as authoritative (fail-closed against drift);
//! * every child of every allowed constructor is recursed into, so a danger op nested inside an allowed
//!   one — `bvslt (bvashr x k) y`, an unproven op in a shift AMOUNT or a non-const MULTIPLIER, an `Eq`
//!   buried in an `And` vector — forces a decline.
//!
//! Admission tiers:
//! * **TIER-1 (proven wiring — a `*_correct` theorem in BitBlast.lean):** `Add` (rippleCarry_spec),
//!   constant `Mul` (mulConst_correct), `Shl`/`Lshr` const+barrel (shlConst/shrConstL/barrelShl/
//!   barrelLshr_correct), `Not` (bitsToNat_not), `Concat` (bitsToNat_append_list), `Extract`
//!   (bitsToNat_extract), `ZeroExtend` (bitsToNat_append_replicate_false), all eight comparators via
//!   `ult`/`slt`/`ule`/`sle_correct` (`>`/`≥` are these on swapped operands), equality `Eq` via
//!   `eqBits_correct` (backed by `bitsToNat_inj`), bitwise `And`/`Or`/`Xor` via
//!   `andBits`/`orBits`/`xorBits_correct` (the `bitsToNat_testBit` bridge + core `Nat.testBit_*`),
//!   `Sub`/`Neg` via `subBits`/`negBits_correct` (the two's-complement subtractor —
//!   `rippleCarry_spec` + the complement identity, the same circuit `ult` rests on), and `Ite` via
//!   `iteBits_correct` (a common-selector per-bit mux IS the list-level if-then-else; the mux gate's
//!   four clauses are the TIER-0 Tseitin family).
//! * **TIER-0 (trusted propositional base):** the SAT literals and the Tseitin clauses for `And`/`Or`/
//!   `Not` OVER PREDICATES. These carry no `_correct` theorem because they ARE the base every proven
//!   gate is built on — the proven adder's own internal and/or/xor gates rely on the identical Tseitin
//!   translation, and the CDCL engine consumes the same clauses. Admitting them adds no trust beyond
//!   what TIER-1 already requires.
//! * **DEFERRED (unproven wiring → z3):** `Ashr`, `SignExtend`, `Udiv`/`Urem`/`Sdiv`/`Srem`.
//!   Variable×variable `Mul` is TIER-1 via `mulVar_correct` (schoolbook array = const-mul family).
//!   (NOTE: historical encoder `bvurem` on shift amounts was rewritten to proven extract/zext.)

use crate::bv::{Formula, Pred, Term};

/// True iff every assertion in `f` lies entirely within the machine-checked native-authoritative
/// fragment. A `false` result means the caller must decline (`None`) and defer to z3.
pub fn is_proven_authoritative(f: &Formula) -> bool {
    f.asserts.iter().all(pred_ok)
}

/// The set of operation tags admitted as native-authoritative, as stable strings. This is the
/// machine-readable mirror of the match arms below — the drift gate
/// (`scripts/run_native_authoritative_gate.sh`) checks it against the set of ops carrying a live
/// `*_correct`/value-lemma marker in `BitBlast.lean`, so an op cannot be admitted here without a green
/// proof, nor a proof silently dropped while its op stays admitted.
pub const PROVEN_OP_TAGS: &[&str] = &[
    // TIER-1 term wiring
    "Add",
    "MulConst",
    "MulVar",
    "Shl",
    "Lshr",
    "Not",
    "Concat",
    "Extract",
    "ZeroExtend",
    "And",
    "Or",
    "Xor",
    "Sub",
    "Neg",
    "Ite",
    // TIER-1 comparators (all eight) + equality
    "Ult",
    "Ule",
    "Ugt",
    "Uge",
    "Slt",
    "Sle",
    "Sgt",
    "Sge",
    "Eq",
    // TIER-0 propositional base
    "PredConst",
    "BoolVar",
    "PredNot",
    "PredAnd",
    "PredOr",
];

fn pred_ok(p: &Pred) -> bool {
    match p {
        // TIER-0 propositional base (trusted Tseitin core, shared with every proven gate).
        Pred::Const(_) | Pred::BoolVar(_) => true,
        Pred::Not(q) => pred_ok(q),
        Pred::And(qs) | Pred::Or(qs) => qs.iter().all(pred_ok),
        // TIER-1 comparators — all eight proven (ult/slt/ule/sle_correct; gt/ge are operand swaps).
        // Recurse into BOTH operands: a danger op may hide in either side.
        Pred::Ult(a, b)
        | Pred::Ule(a, b)
        | Pred::Ugt(a, b)
        | Pred::Uge(a, b)
        | Pred::Slt(a, b)
        | Pred::Sle(a, b)
        | Pred::Sgt(a, b)
        | Pred::Sge(a, b)
        // Equality: eqBits_correct (bitsToNat_inj) proves eq_bits decides value equality. Recurse both.
        | Pred::Eq(a, b) => term_ok(a) && term_ok(b),
    }
}

fn term_ok(t: &Term) -> bool {
    match t {
        // Leaves.
        Term::Const(_, _) | Term::Var(_, _) => true,
        // TIER-1 proven arithmetic — recurse into EVERY child. Sub/Neg: subBits/negBits_correct
        // (the two's-complement subtractor is the same circuit the proven ult rests on).
        Term::Add(a, b) | Term::Sub(a, b) => term_ok(a) && term_ok(b),
        Term::Neg(a) => term_ok(a),
        // Multiply: const path (mulConst_correct) or schoolbook var×var (mulVar_correct).
        Term::Mul(a, b) => term_ok(a) && term_ok(b),
        // Shifts: proven for constant (shlConst/shrConstL) AND variable (barrel) amounts. BOTH the
        // shifted value AND the amount must be proven — a danger op frequently hides in the amount
        // (e.g. the runtime wraps it as `bvurem r 64`, which is DEFERRED, so real shifts defer here).
        Term::Shl(a, b) | Term::Lshr(a, b) => term_ok(a) && term_ok(b),
        // Bitwise: andBits/orBits/xorBits_correct (the bitsToNat_testBit bridge). Recurse both.
        Term::And(a, b) | Term::Or(a, b) | Term::Xor(a, b) => term_ok(a) && term_ok(b),
        // Conditional select: iteBits_correct. The condition is a PRED child — recurse pred_ok into
        // it (walker completeness: a danger op may hide in the selector as well as either branch).
        Term::Ite(p, a, b) => pred_ok(p) && term_ok(a) && term_ok(b),
        // Structural — proven value lemmas; recurse the inner term(s).
        Term::Not(a) => term_ok(a),
        Term::Concat(a, b) => term_ok(a) && term_ok(b),
        Term::Extract(_, _, a) => term_ok(a),
        Term::ZeroExtend(_, a) => term_ok(a),
        // DEFERRED — unproven wiring; native declines and z3 decides. Listed explicitly (no wildcard)
        // so adding a new `Term` variant is a compile error here, not a silent authoritative admission.
        Term::Ashr(_, _)
        | Term::SignExtend(_, _)
        | Term::Udiv(_, _)
        | Term::Urem(_, _)
        | Term::Sdiv(_, _)
        | Term::Srem(_, _) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_smt2;

    fn gate(smt: &str) -> bool {
        is_proven_authoritative(&parse_smt2(smt).expect("parse"))
    }

    // ---- Positive: obligations built ONLY from proven-fragment ops are admitted. ----

    #[test]
    fn admits_add_and_comparators() {
        // The shape of a real uN body obligation: range assumptions (bvsge/bvsle) + bvadd + ensures.
        let smt = "(declare-const x (_ BitVec 64))(declare-const y (_ BitVec 64))\
                   (assert (bvsle x (_ bv1000 64)))(assert (bvsge x (_ bv0 64)))\
                   (assert (not (bvsge (bvadd x y) x)))(check-sat)";
        assert!(gate(smt), "add + comparators must be authoritative");
    }

    #[test]
    fn admits_const_mul_shift_structural() {
        let smt = "(declare-const x (_ BitVec 64))\
                   (assert (bvult (bvmul x (_ bv4 64)) (bvshl x (_ bv3 64))))\
                   (assert (bvule ((_ extract 31 0) x) ((_ zero_extend 0) x)))(check-sat)";
        assert!(
            gate(smt),
            "const-mul, const-shift, extract, zero_extend must be authoritative"
        );
    }

    #[test]
    fn admits_propositional_structure() {
        let smt = "(declare-const x (_ BitVec 64))\
                   (assert (and (bvslt x (_ bv5 64)) (not (bvsgt x (_ bv0 64)))))(check-sat)";
        assert!(
            gate(smt),
            "And/Or/Not over proven comparators must be authoritative"
        );
    }

    // ---- Negative: a danger op ANYWHERE forces a decline. This is the soundness core. ----

    #[test]
    fn admits_equality() {
        // Eq is proof-backed (eqBits_correct / bitsToNat_inj), so ground + symbolic equality over
        // proven operands stays native-authoritative (the string-equality lane depends on this).
        assert!(gate("(declare-const x (_ BitVec 64))(assert (= (bvadd x (_ bv1 64)) (_ bv3 64)))(check-sat)"));
        assert!(gate("(assert (= (_ bv5 64) (_ bv5 64)))(check-sat)"));
    }

    #[test]
    fn admits_bitwise() {
        // Bitwise is proof-backed (andBits/orBits/xorBits_correct via the bitsToNat_testBit bridge).
        // The u32 literal-arg call-site coercion lowers to bvand, so this keeps concrete-call
        // obligations native-authoritative z3-free.
        assert!(gate(
            "(declare-const x (_ BitVec 64))(assert (bvsle (bvand x (_ bv7 64)) x))(check-sat)"
        ));
        assert!(gate(
            "(declare-const x (_ BitVec 64))(assert (bvule x (bvor x (_ bv7 64))))(check-sat)"
        ));
        assert!(gate(
            "(declare-const x (_ BitVec 64))(assert (= (bvxor x x) (_ bv0 64)))(check-sat)"
        ));
    }

    #[test]
    fn admits_sub_and_neg() {
        // Sub/Neg are proof-backed (subBits/negBits_correct — the two's-complement subtractor).
        assert!(gate(
            "(declare-const x (_ BitVec 64))(assert (bvsle (bvsub x (_ bv1 64)) x))(check-sat)"
        ));
        assert!(gate(
            "(declare-const x (_ BitVec 64))(assert (= (bvadd (bvneg x) x) (_ bv0 64)))(check-sat)"
        ));
    }

    #[test]
    fn admits_ite() {
        // Ite is proof-backed (iteBits_correct) — the abs/min/max patterns the encoder emits.
        assert!(gate(
            "(declare-const x (_ BitVec 64))\
             (assert (bvsge (ite (bvslt x (_ bv0 64)) (bvneg x) x) x))(check-sat)"
        ));
        assert!(gate(
            "(declare-const x (_ BitVec 64))\
             (assert (bvsge (ite (bvsle x (_ bv0 64)) (_ bv0 64) x) (_ bv0 64)))(check-sat)"
        ));
    }

    #[test]
    fn declines_danger_in_any_ite_child() {
        // Walker completeness for the THREE Ite children: selector (a Pred!), then, else.
        for smt in [
            // danger in the SELECTOR predicate
            "(declare-const x (_ BitVec 64))\
             (assert (bvsge (ite (bvslt (bvashr x (_ bv1 64)) (_ bv0 64)) x x) x))(check-sat)",
            // danger in the THEN branch
            "(declare-const x (_ BitVec 64))\
             (assert (bvsge (ite (bvslt x (_ bv0 64)) (bvsdiv x (_ bv2 64)) x) x))(check-sat)",
            // danger in the ELSE branch
            "(declare-const x (_ BitVec 64))\
             (assert (bvsge (ite (bvslt x (_ bv0 64)) x (bvurem x (_ bv2 64))) x))(check-sat)",
        ] {
            assert!(
                !gate(smt),
                "danger op in an Ite child must force decline: {smt}"
            );
        }
    }

    #[test]
    fn declines_top_level_danger_ops() {
        for smt in [
            // Ashr (arithmetic right shift — NOT proven)
            "(declare-const x (_ BitVec 64))(assert (bvsge (bvashr x (_ bv1 64)) (_ bv0 64)))(check-sat)",
            // sign_extend
            "(declare-const x (_ BitVec 32))(assert (bvsge ((_ sign_extend 32) x) (_ bv0 64)))(check-sat)",
            // division / remainder
            "(declare-const x (_ BitVec 64))(assert (bvsle (bvsdiv x (_ bv2 64)) x))(check-sat)",
            "(declare-const x (_ BitVec 64))(assert (bvsle (bvurem x (_ bv2 64)) x))(check-sat)",
        ] {
            assert!(!gate(smt), "danger op must force decline: {smt}");
        }
    }

    // ---- The load-bearing case: a danger op NESTED inside an allowed constructor (walker
    // completeness — the recurring Anubis "N walkers, one missed child" defect class). ----

    #[test]
    fn declines_danger_nested_in_allowed() {
        for smt in [
            // danger in a comparator operand
            "(declare-const x (_ BitVec 64))(declare-const y (_ BitVec 64))\
             (assert (bvslt (bvashr x (_ bv1 64)) y))(check-sat)",
            // danger in a shift AMOUNT
            "(declare-const x (_ BitVec 64))(declare-const y (_ BitVec 64))\
             (assert (bvult (bvshl x (bvashr y (_ bv1 64))) x))(check-sat)",
            // danger in the NON-CONST multiplier operand
            "(declare-const x (_ BitVec 64))(declare-const y (_ BitVec 64))\
             (assert (bvult (bvmul (_ bv2 64) (bvashr x y)) x))(check-sat)",
            // danger inside an Extract inner term
            "(declare-const x (_ BitVec 64))\
             (assert (bvult ((_ extract 31 0) (bvsdiv x (_ bv2 64))) x))(check-sat)",
            // danger inside a Concat inner term
            "(declare-const x (_ BitVec 32))(declare-const y (_ BitVec 32))\
             (assert (bvult (concat (bvashr x y) y) (_ bv0 64)))(check-sat)",
            // danger (Srem) buried in an Eq operand inside an And vector, after a proven conjunct
            "(declare-const x (_ BitVec 64))(declare-const y (_ BitVec 64))\
             (assert (and (bvslt x y) (= y (bvsrem x (_ bv3 64)))))(check-sat)",
            // danger inside a Not-wrapped predicate (Ashr under the comparator under Not)
            "(declare-const x (_ BitVec 64))\
             (assert (not (bvsle (bvashr x (_ bv1 64)) x)))(check-sat)",
        ] {
            assert!(
                !gate(smt),
                "danger op nested in an allowed constructor must force decline: {smt}"
            );
        }
    }

    // var×var mul is admitted when both operands are proven (mulVar_correct); danger still declines.
    #[test]
    fn admits_var_times_var_mul() {
        let smt = "(declare-const x (_ BitVec 64))(declare-const y (_ BitVec 64))\
                   (assert (bvult (bvmul x y) x))(check-sat)";
        assert!(
            gate(smt),
            "variable×variable multiply is schoolbook-proven authoritative"
        );
    }

    #[test]
    fn declines_var_mul_with_danger_operand() {
        let smt = "(declare-const x (_ BitVec 64))(declare-const y (_ BitVec 64))\
                   (assert (bvult (bvmul x (bvsdiv y (_ bv2 64))) x))(check-sat)";
        assert!(!gate(smt), "mul of a danger (sdiv) operand must decline");
    }
}
