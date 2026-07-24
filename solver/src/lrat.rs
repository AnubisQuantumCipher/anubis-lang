//! Independent RUP/LRAT-style Unsat certificate checker.
//!
//! The certificate is a sequence of clause additions. Each addition must be **RUP**
//! (reverse unit propagation) with respect to the original formula plus prior additions.
//! The proof is accepted only if it ends with the empty clause (and that empty clause is RUP).
//!
//! This module is deliberately pure and small: no CDCL, no watches, no I/O. The solver may
//! emit optional LRAT-style hints later; the checker **recomputes** RUP and never trusts
//! unvalidated solver metadata.

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
/// Accepts only if every step is RUP and the final step is the empty clause.
/// Also accepts when the original formula already contains the empty clause **and**
/// the proof ends with an empty step that is RUP (immediate).
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

/// RUP: falsify every literal of `candidate`, unit-propagate; success iff conflict.
pub fn is_rup(num_vars: usize, clauses: &[Vec<i32>], candidate: &[i32]) -> bool {
    let n = num_vars.max(max_var(clauses).max(max_var_lits(candidate)));
    if n == 0 {
        // No variables: empty candidate is RUP iff some clause is empty.
        return candidate.is_empty() && clauses.iter().any(|c| c.is_empty());
    }
    // 0 = undef, 1 = true, -1 = false
    let mut assign = vec![0i8; n];
    for &lit in candidate {
        if lit == 0 {
            return false;
        }
        let v = (lit.unsigned_abs() as usize) - 1;
        if v >= n {
            return false;
        }
        let want: i8 = if lit > 0 { -1 } else { 1 }; // falsify `lit`
        if assign[v] == -want {
            return true; // opposite already → conflict under assumptions
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
                    let v = (ulit.unsigned_abs() as usize) - 1;
                    if v >= n {
                        return false;
                    }
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
        if lit == 0 {
            continue;
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
            // no undef and not all false and not sat → shouldn't happen
            if false_n == cl.len() {
                ClauseView::Conflict
            } else {
                ClauseView::Other
            }
        }
    }
}

fn max_var(clauses: &[Vec<i32>]) -> usize {
    clauses.iter().map(|c| max_var_lits(c)).max().unwrap_or(0)
}

fn max_var_lits(lits: &[i32]) -> usize {
    lits.iter()
        .map(|l| l.unsigned_abs() as usize)
        .max()
        .unwrap_or(0)
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
        // (a∨b) (¬a∨b) (¬b)  ⇒ b is forced, then empty? (¬b) and b from units of first two?
        // Actually: (a∨b),(¬a∨b) RUP-learn (b); then (b)∧(¬b) → empty.
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
        // (x ∨ y) is SAT. Claiming a unit (x) as RUP is false (set ¬x, y free ⇒ no conflict).
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
}
