//! Independent **RUP** Unsat certificate checker (LRAT-shaped emission, no deletions).
//!
//! # Algorithm
//!
//! A clause `C` is **RUP** (reverse unit propagation) with respect to a clause database `D` iff
//! unit-propagating under the assumptions that **falsify every literal of `C`** reaches a conflict
//! in `D`. Intuition: `D ∧ ¬C` is unit-unsatisfiable, so `C` is logically implied by `D`.
//!
//! [`check_proof`] verifies a self-contained certificate:
//! 1. Malformed certificates (zero lits, out-of-range vars, complementary pairs in a clause body)
//!    are rejected immediately.
//! 2. The step list must be non-empty and **end with the empty clause**.
//! 3. Starting from `original`, every step must be RUP w.r.t. the clauses accumulated so far;
//!    each non-empty accepted step is appended to the database.
//! 4. The terminal empty clause must itself be RUP (a root conflict).
//!
//! # Independence
//!
//! This module is deliberately **pure and small**: no CDCL, no watches, no I/O, no shared solver
//! state. It uses only signed DIMACS `i32` literals. The CDCL engine may later emit optional
//! LRAT-style hint chains; the checker **recomputes** RUP and never trusts unvalidated metadata.
//!
//! # Residual limitations
//!
//! - **RUP only** (no RAT / full DRAT). Sufficient for CDCL 1-UIP learned clauses.
//! - **No deletion steps**: the clause database only grows with accepted non-empty additions.
//! - Complexity O(steps × clauses × clause size) — appropriate for contract-scale CNFs.

/// One derived clause in the certificate (DIMACS literals; empty = empty clause).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LratStep {
    /// Clause identifier (monotonic in a well-formed emission; checker uses insertion order).
    pub id: u32,
    /// Clause body. Empty means the empty clause (unsat terminal).
    pub lits: Vec<i32>,
}

/// A self-contained Unsat certificate: original CNF + derived RUP steps ending in empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsatCert {
    pub num_vars: usize,
    /// Original clauses after solver load simplification (no tautologies), DIMACS lits.
    pub original: Vec<Vec<i32>>,
    /// Derived clauses in order; the last step **must** be empty for acceptance.
    pub steps: Vec<LratStep>,
}

/// Verify that `cert` is a valid RUP refutation of `cert.original`.
///
/// Accepts only if every clause is well-formed, every step is RUP, and the final step is the
/// empty clause. Fail-closed on truncated, forged, out-of-range, or non-RUP proofs.
pub fn check_proof(cert: &UnsatCert) -> bool {
    if cert.steps.is_empty() {
        return false;
    }
    // Reject non-empty terminal.
    if !cert
        .steps
        .last()
        .map(|s| s.lits.is_empty())
        .unwrap_or(false)
    {
        return false;
    }
    // Fail-closed structural validation of every clause before any RUP work.
    for cl in &cert.original {
        if !clause_ok(cert.num_vars, cl) {
            return false;
        }
    }
    for step in &cert.steps {
        if !clause_ok(cert.num_vars, &step.lits) {
            return false;
        }
    }

    let mut db: Vec<Vec<i32>> = cert.original.clone();
    for step in &cert.steps {
        if !is_rup(cert.num_vars, &db, &step.lits) {
            return false;
        }
        // Do not add a second empty; empty terminal is enough.
        if !step.lits.is_empty() {
            db.push(step.lits.clone());
        }
    }
    true
}

/// Literal is a non-zero DIMACS var index in `1..=num_vars` (when `num_vars > 0`).
#[inline]
fn lit_ok(num_vars: usize, lit: i32) -> bool {
    if lit == 0 {
        return false;
    }
    let v = lit.unsigned_abs() as usize;
    if num_vars == 0 {
        // No variables declared: only the empty clause is legal (handled by clause_ok).
        return false;
    }
    v >= 1 && v <= num_vars
}

/// Clause is well-formed: no zero lits, vars in range, no complementary pair `x` and `¬x`.
/// The empty clause is always well-formed (even when `num_vars == 0`).
fn clause_ok(num_vars: usize, lits: &[i32]) -> bool {
    if lits.is_empty() {
        return true;
    }
    if num_vars == 0 {
        return false;
    }
    // Track seen polarities per variable index (1-based via abs).
    // Use a tiny scan: contract CNFs are small; keep pure and allocation-light.
    for (i, &a) in lits.iter().enumerate() {
        if !lit_ok(num_vars, a) {
            return false;
        }
        for &b in &lits[i + 1..] {
            if !lit_ok(num_vars, b) {
                return false;
            }
            // Complementary pair in one clause body → reject (tautology / malformed cert).
            if a == -b {
                return false;
            }
        }
    }
    true
}

/// RUP: falsify every literal of `candidate`, unit-propagate; success iff conflict.
///
/// When `num_vars > 0`, all literals must already be in-range (callers validate via
/// [`clause_ok`]); this function still rejects zero / out-of-range on the candidate
/// as a fail-closed belt.
pub fn is_rup(num_vars: usize, clauses: &[Vec<i32>], candidate: &[i32]) -> bool {
    if num_vars == 0 {
        // No variables: empty candidate is RUP iff some clause is empty.
        return candidate.is_empty() && clauses.iter().any(|c| c.is_empty());
    }
    // Strict: do not expand past declared num_vars.
    let n = num_vars;
    // 0 = undef, 1 = true, -1 = false
    let mut assign = vec![0i8; n];
    for &lit in candidate {
        if !lit_ok(n, lit) {
            return false;
        }
        let v = (lit.unsigned_abs() as usize) - 1;
        let want: i8 = if lit > 0 { -1 } else { 1 }; // falsify `lit`
        if assign[v] == -want {
            // Complementary assumptions under falsification → candidate has both x and ¬x
            // (should already be rejected by clause_ok); treat as immediate conflict.
            return true;
        }
        assign[v] = want;
    }
    // Immediate conflict if formula contains empty clause.
    if clauses.iter().any(|c| c.is_empty()) {
        return true;
    }
    loop {
        let mut progress = false;
        for cl in clauses {
            match classify_clause(cl, &assign) {
                ClauseView::Satisfied => {}
                ClauseView::Conflict => return true,
                ClauseView::Unit(ulit) => {
                    if !lit_ok(n, ulit) {
                        return false;
                    }
                    let v = (ulit.unsigned_abs() as usize) - 1;
                    let want: i8 = if ulit > 0 { 1 } else { -1 };
                    if assign[v] == 0 {
                        assign[v] = want;
                        progress = true;
                    } else if assign[v] != want {
                        return true;
                    }
                }
                ClauseView::Other => {}
            }
        }
        if !progress {
            return false;
        }
    }
}

enum ClauseView {
    Satisfied,
    Conflict,
    Unit(i32),
    Other,
}

fn classify_clause(cl: &[i32], assign: &[i8]) -> ClauseView {
    if cl.is_empty() {
        return ClauseView::Conflict;
    }
    let mut undef: Option<i32> = None;
    let mut false_n = 0usize;
    for &lit in cl {
        // Zero / OOR literals are structural errors; treat as non-unit garbage (RUP fails
        // closed elsewhere via clause_ok). Do not skip zeros as if they were absent.
        if lit == 0 {
            return ClauseView::Other;
        }
        let v = (lit.unsigned_abs() as usize) - 1;
        if v >= assign.len() {
            return ClauseView::Other;
        }
        let val = assign[v];
        let lit_true = (lit > 0 && val == 1) || (lit < 0 && val == -1);
        let lit_false = (lit > 0 && val == -1) || (lit < 0 && val == 1);
        if lit_true {
            return ClauseView::Satisfied;
        }
        if lit_false {
            false_n += 1;
        } else {
            // undef
            if undef.is_some() {
                // ≥2 undef → not unit
                undef = Some(0); // mark multi
            } else {
                undef = Some(lit);
            }
        }
    }
    if false_n == cl.len() {
        return ClauseView::Conflict;
    }
    match undef {
        Some(0) => ClauseView::Other, // multi-undef sentinel
        Some(u) if false_n + 1 == cl.len() => ClauseView::Unit(u),
        Some(_) => ClauseView::Other,
        None => {
            if false_n == cl.len() {
                ClauseView::Conflict
            } else {
                ClauseView::Other
            }
        }
    }
}

/// Convert a positive-only DIMACS-style list helper for tests.
#[cfg(test)]
pub fn step(id: u32, lits: &[i32]) -> LratStep {
    LratStep {
        id,
        lits: lits.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sat::{Cnf, Lit, SatResult};

    #[test]
    fn empty_original_plus_empty_step_is_unsat() {
        let cert = UnsatCert {
            num_vars: 1,
            original: vec![vec![]],
            steps: vec![step(1, &[])],
        };
        assert!(check_proof(&cert));
    }

    #[test]
    fn unit_conflict_empty_is_rup() {
        // (x) ∧ (¬x)
        let cert = UnsatCert {
            num_vars: 1,
            original: vec![vec![1], vec![-1]],
            steps: vec![step(2, &[])],
        };
        assert!(check_proof(&cert));
        assert!(is_rup(1, &[vec![1], vec![-1]], &[]));
    }

    #[test]
    fn binary_resolution_chain() {
        // (x ∨ y) (¬x) (¬y) — learn empty via units
        let original = vec![vec![1, 2], vec![-1], vec![-2]];
        assert!(is_rup(2, &original, &[]));
        let cert = UnsatCert {
            num_vars: 2,
            original,
            steps: vec![step(4, &[])],
        };
        assert!(check_proof(&cert));
    }

    #[test]
    fn learned_clause_then_empty() {
        // (a∨b) (¬a∨b) (¬b)  ⇒ learn (b); then empty.
        let original = vec![vec![1, 2], vec![-1, 2], vec![-2]];
        assert!(is_rup(2, &original, &[2])); // learn (b)
        let mut db = original.clone();
        db.push(vec![2]);
        assert!(is_rup(2, &db, &[]));
        let cert = UnsatCert {
            num_vars: 2,
            original,
            steps: vec![step(4, &[2]), step(5, &[])],
        };
        assert!(check_proof(&cert));
    }

    #[test]
    fn adversarial_no_steps_rejected() {
        let cert = UnsatCert {
            num_vars: 1,
            original: vec![vec![1], vec![-1]],
            steps: vec![],
        };
        assert!(!check_proof(&cert));
    }

    #[test]
    fn adversarial_truncated_no_empty_rejected() {
        let cert = UnsatCert {
            num_vars: 2,
            original: vec![vec![1, 2], vec![-1, 2], vec![-2]],
            steps: vec![step(4, &[2])], // missing empty
        };
        assert!(!check_proof(&cert));
    }

    #[test]
    fn adversarial_forged_empty_on_sat_rejected() {
        // Single unit (x) is SAT; empty is not RUP.
        let cert = UnsatCert {
            num_vars: 1,
            original: vec![vec![1]],
            steps: vec![step(2, &[])],
        };
        assert!(!check_proof(&cert));
        assert!(!is_rup(1, &[vec![1]], &[]));
    }

    #[test]
    fn adversarial_mutated_learned_rejected() {
        // (x ∨ y) is SAT. Claiming a unit (x) as RUP is false.
        let original = vec![vec![1, 2]];
        let cert = UnsatCert {
            num_vars: 2,
            original,
            steps: vec![step(2, &[1]), step(3, &[])],
        };
        assert!(!is_rup(2, &[vec![1, 2]], &[1]));
        assert!(!check_proof(&cert));
    }

    #[test]
    fn adversarial_non_empty_terminal_rejected() {
        let cert = UnsatCert {
            num_vars: 1,
            original: vec![vec![1], vec![-1]],
            steps: vec![step(2, &[1])],
        };
        assert!(!check_proof(&cert));
    }

    #[test]
    fn adversarial_wrong_var_numbering_rejected() {
        // Formula uses var 2 but cert claims num_vars = 1.
        let cert = UnsatCert {
            num_vars: 1,
            original: vec![vec![1], vec![-1], vec![2]],
            steps: vec![step(3, &[])],
        };
        assert!(!check_proof(&cert));
        // Candidate step with out-of-range lit.
        let cert2 = UnsatCert {
            num_vars: 1,
            original: vec![vec![1], vec![-1]],
            steps: vec![step(2, &[99]), step(3, &[])],
        };
        assert!(!check_proof(&cert2));
    }

    #[test]
    fn adversarial_zero_literal_rejected() {
        let cert = UnsatCert {
            num_vars: 1,
            original: vec![vec![1], vec![0]],
            steps: vec![step(2, &[])],
        };
        assert!(!check_proof(&cert));
        let cert2 = UnsatCert {
            num_vars: 1,
            original: vec![vec![1], vec![-1]],
            steps: vec![step(2, &[0]), step(3, &[])],
        };
        assert!(!check_proof(&cert2));
    }

    #[test]
    fn adversarial_complementary_lits_in_step_rejected() {
        // Tautology "learned" (x ∨ ¬x) then empty — must reject the step body.
        let cert = UnsatCert {
            num_vars: 1,
            original: vec![vec![1], vec![-1]],
            steps: vec![step(2, &[1, -1]), step(3, &[])],
        };
        assert!(!check_proof(&cert));
    }

    #[test]
    fn adversarial_non_rup_intermediate_then_empty_rejected() {
        // (x∨y) is SAT. Forged unit (x) is not RUP; even if empty followed, reject.
        let cert = UnsatCert {
            num_vars: 2,
            original: vec![vec![1, 2]],
            steps: vec![step(2, &[1]), step(3, &[])],
        };
        assert!(!check_proof(&cert));
    }

    #[test]
    fn adversarial_extra_garbage_after_empty_as_terminal_rejected() {
        // If "garbage" is non-empty terminal, reject; empty must be last.
        let cert = UnsatCert {
            num_vars: 1,
            original: vec![vec![1], vec![-1]],
            steps: vec![step(2, &[]), step(3, &[1])],
        };
        assert!(!check_proof(&cert));
    }

    #[test]
    fn cdcl_unit_conflict_cert_verifies() {
        let mut c = Cnf::new();
        let x = c.new_var();
        c.add_clause(vec![Lit::pos(x)]);
        c.add_clause(vec![Lit::new(x, true)]);
        match c.solve(1000) {
            SatResult::Unsat(cert) => assert!(check_proof(&cert)),
            other => panic!("expected Unsat, got {other:?}"),
        }
    }

    #[test]
    fn cdcl_pigeon_cert_verifies() {
        let mut c = Cnf::new();
        let p1 = c.new_var();
        let p2 = c.new_var();
        c.add_clause(vec![Lit::pos(p1)]);
        c.add_clause(vec![Lit::pos(p2)]);
        c.add_clause(vec![Lit::new(p1, true), Lit::new(p2, true)]);
        match c.solve(1000) {
            SatResult::Unsat(cert) => assert!(check_proof(&cert)),
            other => panic!("expected Unsat, got {other:?}"),
        }
    }
}
