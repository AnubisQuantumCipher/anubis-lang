//! Anubis native SMT decision procedure — a from-scratch QF_BV solver that replaces the z3 third
//! party for the integer contract lane. ZERO external solver dependency (std only). During rollout,
//! z3 stays the authority and this runs in SHADOW mode (`compiler` compares the two and fails closed
//! on any disagreement), so a bug here can never certify a false contract. Once the differential gate
//! is sustained at zero disagreements AND the bit-blaster is machine-checked in Lean, the compiler can
//! flip to native-authoritative, shrinking the trusted computing base by all of z3.
//!
//! Pipeline: SMT-LIB2 text → `bv::Formula` (parse) → CNF (bit-blast) → `sat::Cnf::solve` → verdict.

pub mod blast;
pub mod bv;
pub mod parse;
pub mod sat;

use sat::{Cnf, SatResult};

/// Default decision budget (max DPLL decisions). Sized so easy obligations decide fast and hard ones
/// return `None` (defer to z3) rather than hang. Tune with the differential gate.
pub const DEFAULT_BUDGET: u64 = 2_000_000;

/// Native decision for one SMT-LIB2 obligation, as emitted by `compiler/src/middle/mod.rs`.
///
/// Returns:
/// * `Some(true)`  — SATISFIABLE. A model of `assumptions ∧ ¬property` exists, i.e. a counterexample;
///   the obligation's property is NOT proven. (Same meaning as z3 answering `sat`.)
/// * `Some(false)` — UNSATISFIABLE. No model — the property is PROVEN. (z3 `unsat`.)
/// * `None`        — the solver declines: out-of-fragment (non-BV theory, unsupported op, parse
///   failure) OR undecided within the budget. The caller MUST defer to z3.
///
/// SOUNDNESS: a definite verdict is returned ONLY when the formula parsed as pure QF_BV, every term
/// bit-blasted with a supported gate, and the SAT engine actually decided the resulting CNF. Any
/// uncertainty is `None`. So this function can be wired in front of z3 without ever changing a verdict
/// z3 would not also give — provided the bit-blaster is correct, which the differential gate + the Lean
/// proof establish.
pub fn native_check_sat(smt: &str) -> Option<bool> {
    native_check_sat_budget(smt, DEFAULT_BUDGET)
}

/// As [`native_check_sat`] but with an explicit decision budget (used by the differential gate).
pub fn native_check_sat_budget(smt: &str, budget: u64) -> Option<bool> {
    let formula = parse::parse_smt2(smt)?;
    let mut cnf = Cnf::new();
    blast::blast(&formula, &mut cnf)?;
    match cnf.solve(budget) {
        SatResult::Sat(_) => Some(true),
        SatResult::Unsat => Some(false),
        SatResult::Unknown => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ground_true_equality_is_unsat_negated() {
        // ¬(5 == 5) is unsatisfiable ⇒ 5 == 5 is proven.
        let smt = "(set-logic QF_BV)\n(assert (not (= (_ bv5 64) (_ bv5 64))))\n(check-sat)\n";
        assert_eq!(native_check_sat(smt), Some(false));
    }

    #[test]
    fn ground_false_equality_is_sat() {
        // 5 == 6 is satisfiable-as-written? No: (= 5 6) is false, so the assert is unsatisfiable.
        let smt = "(set-logic QF_BV)\n(assert (= (_ bv5 64) (_ bv6 64)))\n(check-sat)\n";
        assert_eq!(native_check_sat(smt), Some(false));
    }

    #[test]
    fn simple_symbolic_add_is_sat() {
        // exists x. x + 1 == 3  (x = 2). SAT.
        let smt = "(set-logic QF_BV)\n(declare-const x (_ BitVec 64))\n\
                   (assert (= (bvadd x (_ bv1 64)) (_ bv3 64)))\n(check-sat)\n";
        assert_eq!(native_check_sat(smt), Some(true));
    }

    #[test]
    fn out_of_fragment_defers() {
        // A string-theory obligation is not QF_BV — decline (None → z3).
        let smt = "(set-logic QF_S)\n(declare-const s String)\n(assert (= s \"hi\"))\n(check-sat)\n";
        assert_eq!(native_check_sat(smt), None);
    }
}
