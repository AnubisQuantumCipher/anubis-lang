//! A small, clearly-correct CNF SAT solver — the engine every bit-blasted QF_BV obligation reduces to.
//!
//! Slice 1 is a classic DPLL: unit propagation + chronological backtracking, bounded by a decision
//! budget. Correctness over speed — a formula that exceeds the budget returns `Unknown` (NOT a
//! verdict), which the caller maps to "defer to z3". So the solver is SOUND by construction: it only
//! ever answers `Sat` (with a witnessed model) or `Unsat` when it has actually decided the formula;
//! anything it cannot finish in budget is `Unknown`, never a guessed verdict. A later slice upgrades
//! this to CDCL (clause learning + watched literals) for the harder multiplier-heavy instances.

/// A boolean variable, 0-indexed.
pub type Var = usize;

/// A literal: a variable together with a sign. Encoded as `2*var + (neg as usize)` so it packs into a
/// `usize` and negation is a single xor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Lit(usize);

impl Lit {
    #[inline]
    pub fn new(var: Var, neg: bool) -> Lit {
        Lit(var * 2 + neg as usize)
    }
    #[inline]
    pub fn pos(var: Var) -> Lit {
        Lit::new(var, false)
    }
    #[inline]
    pub fn negate(self) -> Lit {
        Lit(self.0 ^ 1)
    }
    #[inline]
    pub fn var(self) -> Var {
        self.0 / 2
    }
    #[inline]
    pub fn is_neg(self) -> bool {
        self.0 & 1 == 1
    }
}

/// Three-valued assignment cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Val {
    True,
    False,
    Unset,
}

/// The result of a solve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SatResult {
    /// Satisfiable, with a full model (one bool per variable).
    Sat(Vec<bool>),
    /// Unsatisfiable — decided (a real proof of no model within the finite bit-vector domain).
    Unsat,
    /// Not decided within the budget — the caller must fall back (to z3). Never a guessed verdict.
    Unknown,
}

/// A CNF instance built incrementally.
#[derive(Debug, Clone, Default)]
pub struct Cnf {
    num_vars: usize,
    clauses: Vec<Vec<Lit>>,
}

impl Cnf {
    pub fn new() -> Cnf {
        Cnf::default()
    }
    /// Allocate a fresh variable.
    pub fn new_var(&mut self) -> Var {
        let v = self.num_vars;
        self.num_vars += 1;
        v
    }
    /// Add a clause (a disjunction of literals). An empty clause makes the instance trivially UNSAT.
    pub fn add_clause(&mut self, lits: Vec<Lit>) {
        self.clauses.push(lits);
    }
    pub fn num_vars(&self) -> usize {
        self.num_vars
    }
    pub fn num_clauses(&self) -> usize {
        self.clauses.len()
    }

    /// Solve with a decision budget (max number of decision-level assignments before giving up).
    /// Returns `Unknown` if the budget is exhausted — sound: an undecided formula is never a verdict.
    pub fn solve(&self, budget: u64) -> SatResult {
        let mut solver = Dpll {
            num_vars: self.num_vars,
            clauses: &self.clauses,
            assign: vec![Val::Unset; self.num_vars],
            trail: Vec::with_capacity(self.num_vars),
            decisions: 0,
            budget,
        };
        solver.run()
    }
}

struct Dpll<'a> {
    num_vars: usize,
    clauses: &'a [Vec<Lit>],
    assign: Vec<Val>,
    /// Assigned literals in order, with a marker (`decision`) for backtracking.
    trail: Vec<(Lit, bool)>,
    decisions: u64,
    budget: u64,
}

impl<'a> Dpll<'a> {
    #[inline]
    fn value(&self, l: Lit) -> Val {
        match self.assign[l.var()] {
            Val::Unset => Val::Unset,
            v => {
                if l.is_neg() {
                    match v {
                        Val::True => Val::False,
                        Val::False => Val::True,
                        Val::Unset => Val::Unset,
                    }
                } else {
                    v
                }
            }
        }
    }

    #[inline]
    fn assign_lit(&mut self, l: Lit, decision: bool) {
        self.assign[l.var()] = if l.is_neg() { Val::False } else { Val::True };
        self.trail.push((l, decision));
    }

    /// Unit propagation to a fixpoint. Returns `false` on conflict (some clause is fully false).
    fn propagate(&mut self) -> bool {
        loop {
            let mut progressed = false;
            for clause in self.clauses {
                let mut unassigned: Option<Lit> = None;
                let mut count_unassigned = 0;
                let mut satisfied = false;
                for &lit in clause {
                    match self.value(lit) {
                        Val::True => {
                            satisfied = true;
                            break;
                        }
                        Val::False => {}
                        Val::Unset => {
                            count_unassigned += 1;
                            unassigned = Some(lit);
                        }
                    }
                }
                if satisfied {
                    continue;
                }
                if count_unassigned == 0 {
                    return false; // conflict: all literals false
                }
                if count_unassigned == 1 {
                    let u = unassigned.unwrap();
                    self.assign_lit(u, false);
                    progressed = true;
                }
            }
            if !progressed {
                return true;
            }
        }
    }

    /// Backtrack to (and including) the most recent decision, flipping it. Returns the flipped literal
    /// to try, or `None` if there is no decision left (formula exhausted → UNSAT).
    fn backtrack(&mut self) -> Option<Lit> {
        while let Some(&(lit, decision)) = self.trail.last() {
            self.trail.pop();
            self.assign[lit.var()] = Val::Unset;
            if decision {
                return Some(lit.negate());
            }
        }
        None
    }

    /// Pick the first unassigned variable (positive phase). Simple + deterministic; a later slice adds
    /// activity-based heuristics.
    fn pick(&self) -> Option<Lit> {
        (0..self.num_vars)
            .find(|&v| self.assign[v] == Val::Unset)
            .map(Lit::pos)
    }

    fn run(&mut self) -> SatResult {
        // Initial propagation of any unit clauses.
        if !self.propagate() {
            return SatResult::Unsat;
        }
        loop {
            match self.pick() {
                None => {
                    // All variables assigned and no conflict ⇒ SAT. Extract the model.
                    let model = (0..self.num_vars)
                        .map(|v| self.assign[v] == Val::True)
                        .collect();
                    return SatResult::Sat(model);
                }
                Some(decision_lit) => {
                    self.decisions += 1;
                    if self.decisions > self.budget {
                        return SatResult::Unknown;
                    }
                    let mut next = decision_lit;
                    let mut is_decision = true;
                    loop {
                        self.assign_lit(next, is_decision);
                        if self.propagate() {
                            break; // no conflict — go pick the next variable
                        }
                        // Conflict: backtrack to the last decision and flip it.
                        match self.backtrack() {
                            Some(flipped) => {
                                next = flipped;
                                is_decision = false; // the flip is a forced (implied) assignment
                            }
                            None => return SatResult::Unsat,
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_sat() {
        assert!(matches!(Cnf::new().solve(1000), SatResult::Sat(_)));
    }

    #[test]
    fn empty_clause_is_unsat() {
        let mut c = Cnf::new();
        c.new_var();
        c.add_clause(vec![]);
        assert_eq!(c.solve(1000), SatResult::Unsat);
    }

    #[test]
    fn unit_conflict_is_unsat() {
        // (x) ∧ (¬x)
        let mut c = Cnf::new();
        let x = c.new_var();
        c.add_clause(vec![Lit::pos(x)]);
        c.add_clause(vec![Lit::new(x, true)]);
        assert_eq!(c.solve(1000), SatResult::Unsat);
    }

    #[test]
    fn simple_sat_has_valid_model() {
        // (x ∨ y) ∧ (¬x ∨ y) ∧ (x ∨ ¬y)  ⇒ forces y=true, x free-ish
        let mut c = Cnf::new();
        let x = c.new_var();
        let y = c.new_var();
        c.add_clause(vec![Lit::pos(x), Lit::pos(y)]);
        c.add_clause(vec![Lit::new(x, true), Lit::pos(y)]);
        c.add_clause(vec![Lit::pos(x), Lit::new(y, true)]);
        match c.solve(1000) {
            SatResult::Sat(m) => {
                // verify the model satisfies every clause
                let val = |l: Lit| m[l.var()] ^ l.is_neg();
                assert!(val(Lit::pos(x)) || val(Lit::pos(y)));
                assert!(val(Lit::new(x, true)) || val(Lit::pos(y)));
                assert!(val(Lit::pos(x)) || val(Lit::new(y, true)));
            }
            other => panic!("expected SAT, got {other:?}"),
        }
    }

    #[test]
    fn pigeonhole_2_into_1_is_unsat() {
        // 2 pigeons, 1 hole: p1 ∧ p2 (each pigeon in the hole) ∧ ¬(p1 ∧ p2) is unsat.
        // Encode: (p1) (p2) (¬p1 ∨ ¬p2)
        let mut c = Cnf::new();
        let p1 = c.new_var();
        let p2 = c.new_var();
        c.add_clause(vec![Lit::pos(p1)]);
        c.add_clause(vec![Lit::pos(p2)]);
        c.add_clause(vec![Lit::new(p1, true), Lit::new(p2, true)]);
        assert_eq!(c.solve(1000), SatResult::Unsat);
    }
}
