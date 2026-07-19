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
    native_check_sat_model_budget(smt, budget).map(|v| matches!(v, NativeVerdict::Sat(_)))
}

/// A definite native verdict, carrying the reconstructed model on SAT.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeVerdict {
    /// No model exists — the obligation's property is PROVEN.
    Unsat,
    /// A model exists — `(name, value, width)` for every declared bit-vector variable, LSB-verified.
    /// The model has ALREADY been re-checked by the independent `bv::Formula::eval` replay: this
    /// variant is returned only when the assignment concretely satisfies every assertion.
    Sat(Vec<(String, u128, u32)>),
}

/// As [`native_check_sat`], additionally reconstructing the SMT-level model on SAT.
///
/// The model is read out of the CDCL assignment via the bit-blaster's variable→literal map, then
/// REPLAYED through the independent concrete evaluator (`bv::Formula::eval` — a straight-line
/// interpreter sharing no code with the bit-blaster or the SAT engine). If the replay does not
/// re-satisfy the formula — which would mean a solver defect — this returns `None` (defer), so a
/// broken model can never be presented as a counterexample. UNSAT needs no model.
pub fn native_check_sat_model(smt: &str) -> Option<NativeVerdict> {
    native_check_sat_model_budget(smt, DEFAULT_BUDGET)
}

/// As [`native_check_sat_model`] with an explicit budget.
pub fn native_check_sat_model_budget(smt: &str, budget: u64) -> Option<NativeVerdict> {
    let formula = parse::parse_smt2(smt)?;
    let mut cnf = Cnf::new();
    let map = blast::blast_with_map(&formula, &mut cnf)?;
    match cnf.solve(budget) {
        SatResult::Unsat => Some(NativeVerdict::Unsat),
        SatResult::Unknown => None,
        SatResult::Sat(assign) => {
            // Read each declared bit-vector's value out of the assignment (LSB first). A variable
            // absent from the map or a literal beyond the assignment is unconstrained — 0 is a model.
            let read_lit = |l: &sat::Lit| -> bool {
                let v = assign.get(l.var()).copied().unwrap_or(false);
                if l.is_neg() {
                    !v
                } else {
                    v
                }
            };
            let mut model = Vec::new();
            let mut env = std::collections::HashMap::new();
            for (name, w) in &formula.bv_vars {
                let value = match map.bv.get(name) {
                    Some(bits) => bits
                        .iter()
                        .enumerate()
                        .fold(0u128, |acc, (i, l)| acc | ((read_lit(l) as u128) << i)),
                    None => 0,
                };
                env.insert(name.clone(), value);
                model.push((name.clone(), value, *w));
            }
            let mut bool_env = std::collections::HashMap::new();
            for name in &formula.bool_vars {
                bool_env.insert(
                    name.clone(),
                    map.bools.get(name).map(&read_lit).unwrap_or(false),
                );
            }
            // The native replay: the model must concretely satisfy every assertion under the
            // independent evaluator, or we refuse to certify the SAT (defer to z3).
            if formula.eval(&env, &bool_env) != Some(true) {
                return None;
            }
            Some(NativeVerdict::Sat(model))
        }
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

    #[test]
    fn sat_model_is_concrete_and_replayed() {
        // exists x. x + 1 == 3 — the ONLY model is x = 2; the extracted model must say so.
        let smt = "(set-logic QF_BV)\n(declare-const x (_ BitVec 64))\n\
                   (assert (= (bvadd x (_ bv1 64)) (_ bv3 64)))\n(check-sat)\n(get-model)\n";
        match native_check_sat_model(smt) {
            Some(NativeVerdict::Sat(model)) => {
                assert_eq!(model, vec![("x".to_string(), 2u128, 64u32)]);
            }
            other => panic!("expected Sat with x=2, got {:?}", other),
        }
        // And a proven obligation yields Unsat with no model needed.
        let proven = "(set-logic QF_BV)\n(declare-const x (_ BitVec 64))\n\
                      (assert (not (bvule x x)))\n(check-sat)\n";
        assert_eq!(native_check_sat_model(proven), Some(NativeVerdict::Unsat));
    }
}
