//! Lowering must be LINEAR in the query. Float64 `=`, the `fp.*` comparisons and tests, Boolean `=`
//! and Boolean `ite` each mention an operand several times; the parser used to CLONE the operand
//! tree for every mention, so nesting them grew the lowered formula exponentially. An 8-deep nested
//! `fp.lt` (241 bytes, `fixtures/harness_fp_8.smt2`) exhausted memory and aborted the process; a 13-deep
//! nested Float64 `=` aborted after 31 s. The lowering now binds such an operand to a fresh variable
//! once (`share_term` / `share_pred`), so these queries are DECIDED — and must agree with z3.

use anubis_solver::bv::{Pred, Term};
use anubis_solver::*;
use std::io::Write;
use std::process::{Command, Stdio};

fn z3(smt: &str) -> Option<bool> {
    let mut child = Command::new("z3")
        .args(["-in", "-smt2"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    child.stdin.as_mut()?.write_all(smt.as_bytes()).ok()?;
    let out = child.wait_with_output().ok()?;
    match String::from_utf8_lossy(&out.stdout).lines().next()?.trim() {
        "sat" => Some(true),
        "unsat" => Some(false),
        _ => None,
    }
}

fn term_size(t: &Term) -> usize {
    1 + match t {
        Term::Var(..) | Term::Const(..) => 0,
        Term::Add(a, b)
        | Term::Sub(a, b)
        | Term::Mul(a, b)
        | Term::And(a, b)
        | Term::Or(a, b)
        | Term::Xor(a, b)
        | Term::Shl(a, b)
        | Term::Lshr(a, b)
        | Term::Ashr(a, b)
        | Term::Udiv(a, b)
        | Term::Urem(a, b)
        | Term::Sdiv(a, b)
        | Term::Srem(a, b)
        | Term::Concat(a, b) => term_size(a) + term_size(b),
        Term::Neg(a)
        | Term::Not(a)
        | Term::Extract(_, _, a)
        | Term::ZeroExtend(_, a)
        | Term::SignExtend(_, a) => term_size(a),
        Term::Ite(c, a, b) => pred_size(c) + term_size(a) + term_size(b),
    }
}

fn pred_size(p: &Pred) -> usize {
    1 + match p {
        Pred::Const(_) | Pred::BoolVar(_) => 0,
        Pred::Eq(a, b)
        | Pred::Ult(a, b)
        | Pred::Ule(a, b)
        | Pred::Ugt(a, b)
        | Pred::Uge(a, b)
        | Pred::Slt(a, b)
        | Pred::Sle(a, b)
        | Pred::Sgt(a, b)
        | Pred::Sge(a, b) => term_size(a) + term_size(b),
        Pred::Not(a) => pred_size(a),
        Pred::And(v) | Pred::Or(v) => v.iter().map(pred_size).sum(),
    }
}

/// Decided by the native solver, in agreement with z3, with a lowered formula at most `per_byte`
/// nodes per byte of query text (linear, whatever the nesting).
fn decides_linearly(query: &str, per_byte: usize) {
    let f = parse::parse_smt2(query).expect("well-sorted query must parse");
    let size: usize = f.asserts.iter().map(pred_size).sum();
    assert!(
        size <= per_byte * query.len(),
        "lowered formula has {size} nodes for {} bytes of query: not linear",
        query.len()
    );
    let native = native_check_sat_budget(query, 200_000);
    assert!(
        native.is_some(),
        "native declined a query sharing makes small: {query}"
    );
    if let Some(z) = z3(query) {
        assert_eq!(native, Some(z), "native disagrees with z3 on {query}");
    }
}

#[test]
fn retained_harness_crash_is_decided() {
    decides_linearly(include_str!("fixtures/harness_fp_8.smt2"), 64);
}

#[test]
fn nested_fp_comparisons_are_decided_linearly() {
    for op in ["fp.lt", "fp.leq", "fp.gt", "fp.geq", "fp.eq", "="] {
        let mut term = "x".to_owned();
        for _ in 0..24 {
            term = format!("(ite ({op} {term} y) x y)");
        }
        decides_linearly(
            &format!(
                "(declare-const x Float64)(declare-const y Float64)\
                 (assert ({op} {term} y))(check-sat)"
            ),
            64,
        );
    }
}

#[test]
fn nested_fp_tests_are_decided_linearly() {
    for op in ["fp.isNaN", "fp.isInfinite", "fp.isZero"] {
        let mut term = "x".to_owned();
        for _ in 0..24 {
            term = format!("(ite ({op} {term}) x y)");
        }
        decides_linearly(
            &format!(
                "(declare-const x Float64)(declare-const y Float64)\
                 (assert ({op} {term}))(check-sat)"
            ),
            64,
        );
    }
}

#[test]
fn nested_boolean_equivalence_and_ite_are_decided_linearly() {
    for ite in [false, true] {
        let mut pred = "p".to_owned();
        for _ in 0..24 {
            pred = if ite {
                format!("(ite {pred} q p)")
            } else {
                format!("(= {pred} q)")
            };
        }
        decides_linearly(
            &format!("(declare-const p Bool)(declare-const q Bool)(assert {pred})(check-sat)"),
            64,
        );
    }
}

#[test]
fn many_assertions_are_decided() {
    // 65,536 assertions, each trivial: linear work, decided.
    let query = format!("{}(check-sat)", "(assert true)".repeat(65_536));
    assert_eq!(native_check_sat_budget(&query, 200_000), Some(true));
}

#[test]
fn introduced_variables_never_reach_the_model() {
    // A SAT query whose lowering introduces shared variables: the model names only declared ones.
    let query = "(declare-const x Float64)(declare-const y Float64)\
                 (assert (= (ite (fp.lt x y) x y) y))(check-sat)";
    match native_check_sat_model(query) {
        Some(NativeVerdict::Sat(model)) => {
            assert!(!model.is_empty());
            for (name, _, _) in &model {
                assert!(
                    !parse::is_introduced(name),
                    "internal variable in model: {name:?}"
                );
                assert!(
                    name == "x" || name == "y",
                    "unexpected model variable {name:?}"
                );
            }
        }
        other => panic!("expected a SAT model, got {other:?}"),
    }
}

#[test]
fn ordinary_controls_keep_their_verdicts() {
    for (query, expected) in [
        ("(assert true)(check-sat)", true),
        ("(assert false)(check-sat)", false),
        ("(declare-const p Bool)(assert (= p true))(check-sat)", true),
        (
            "(declare-const x Float64)(assert (fp.lt x x))(check-sat)",
            false,
        ),
        (
            "(declare-const x (_ BitVec 8))(assert (= x #x01))(check-sat)",
            true,
        ),
    ] {
        assert!(
            parse::parse_smt2(query).is_some(),
            "control refused: {query}"
        );
        assert_eq!(
            native_check_sat_budget(query, 200_000),
            Some(expected),
            "{query}"
        );
    }
}

#[test]
fn shallow_fp_ite_still_parses() {
    let query = "(declare-const x Float64)(declare-const y Float64)\
        (assert (fp.lt (ite (fp.lt x y) x y) y))(check-sat)";
    assert!(parse::parse_smt2(query).is_some());
}

// Inputs from the collaborator's earlier budget draft (`lowering_budget.rs`, 2026-09-23), which
// asserted these DECLINE under a pre-lowering budget. With shared operands they are decided; the
// same inputs now assert the stronger property.

#[test]
fn recorded_nested_float_equality_inputs_are_decided() {
    for query in [
        include_str!("fixtures/lowering/x_nested_eq_16.smt2"),
        include_str!("fixtures/lowering/x_nested_eq_20.smt2"),
        include_str!("fixtures/lowering/x_nested_eq_24.smt2"),
    ] {
        decides_linearly(query, 64);
    }
}

#[test]
fn nested_boolean_expansion_with_constants_is_decided() {
    for form in ["eq", "ite"] {
        let mut term = "p".to_string();
        for _ in 0..24 {
            term = if form == "eq" {
                format!("(= {term} p)")
            } else {
                format!("(ite {term} p false)")
            };
        }
        decides_linearly(
            &format!("(declare-const p Bool)(assert {term})(check-sat)"),
            64,
        );
    }
}

#[test]
fn nested_float_helpers_under_qf_fp_are_decided() {
    for op in [
        "fp.lt",
        "fp.leq",
        "fp.gt",
        "fp.geq",
        "fp.eq",
        "fp.isNaN",
        "fp.isInfinite",
        "fp.isZero",
    ] {
        let mut term = "x".to_string();
        for _ in 0..16 {
            let pred = if op.starts_with("fp.is") {
                format!("({op} {term})")
            } else {
                format!("({op} {term} y)")
            };
            term = format!("(ite {pred} x y)");
        }
        decides_linearly(
            &format!(
                "(set-logic QF_FP)(declare-const x (_ FloatingPoint 11 53))\
                 (declare-const y (_ FloatingPoint 11 53))(assert (= {term} x))(check-sat)"
            ),
            64,
        );
    }
}

#[test]
fn ordinary_sat_and_unsat_are_preserved() {
    let prefix = "(set-logic QF_BV)(declare-const x (_ BitVec 8))(assert (= x #x01))";
    assert_eq!(
        native_check_sat_budget(&format!("{prefix}(check-sat)"), 1_000_000),
        Some(true)
    );
    assert_eq!(
        native_check_sat_budget(
            &format!("{prefix}(assert (= x #x02))(check-sat)"),
            1_000_000
        ),
        Some(false)
    );
}

#[test]
fn nested_boolean_ite_equality_parses_in_linear_time() {
    // Review round 2 (2026-09-23): `(= (ite X p q) r)` nested n deep took ~4x longer per two levels
    // (3.2 s at n = 22, over 120 s at n = 26): `=` tried the term reading, failed deep inside the
    // Boolean `ite`, and re-parsed the subtree. The reading is now chosen up front.
    let mut x = "p".to_string();
    for _ in 0..40 {
        x = format!("(= (ite {x} p q) r)");
    }
    let query = format!(
        "(declare-const p Bool)(declare-const q Bool)(declare-const r Bool)(assert {x})(check-sat)"
    );
    let start = std::time::Instant::now();
    decides_linearly(&query, 64);
    assert!(
        start.elapsed().as_secs() < 10,
        "n = 40 took {:?}",
        start.elapsed()
    );
}
