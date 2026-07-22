//! A small, clearly-correct CNF SAT solver — the engine every bit-blasted QF_BV obligation reduces to.
//!
//! The engine is a proper CDCL solver: two-watched-literal unit propagation, 1-UIP conflict analysis
//! with non-chronological backjumping, a VSIDS activity decision heuristic, phase saving, and Luby
//! restarts. It is bounded by a *conflict* budget — a formula that exceeds the budget returns
//! `Unknown` (NOT a verdict), which the caller maps to "defer to z3". So the solver is SOUND by
//! construction: it only ever answers `Sat` (with a witnessed model) or `Unsat` when it has actually
//! decided the formula (SAT = a full satisfying assignment; UNSAT = a conflict derived at decision
//! level 0, i.e. a resolution refutation); anything it cannot finish within budget is `Unknown`,
//! never a guessed verdict.

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
    /// Raw code (0..2*num_vars); used to index watch lists. Private to the module.
    #[inline]
    fn code(self) -> usize {
        self.0
    }
}

/// Three-valued cell for an assignment or a literal evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LBool {
    True,
    False,
    Undef,
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

    /// Solve with a *conflict* budget (max number of conflicts before giving up). Returns `Unknown`
    /// if the budget is exhausted — sound: an undecided formula is never a verdict.
    pub fn solve(&self, budget: u64) -> SatResult {
        let mut solver = Solver::new(self.num_vars);
        // Load and lightly simplify the clause database.
        for raw in &self.clauses {
            let mut lits: Vec<Lit> = Vec::with_capacity(raw.len());
            let mut tautology = false;
            for &l in raw {
                if lits.contains(&l) {
                    continue; // drop duplicate literal
                }
                if lits.contains(&l.negate()) {
                    tautology = true; // clause contains l and ¬l ⇒ always true
                    break;
                }
                lits.push(l);
            }
            if tautology {
                continue;
            }
            match lits.len() {
                0 => return SatResult::Unsat, // empty clause ⇒ UNSAT
                1 => {
                    // Unit clause: assign at decision level 0. Conflicting units ⇒ UNSAT.
                    if !solver.add_unit(lits[0]) {
                        return SatResult::Unsat;
                    }
                }
                _ => {
                    solver.add_clause_internal(lits);
                }
            }
        }
        solver.search(budget)
    }
}

/// A watch entry: a clause plus a cached "blocker" literal. If the blocker is already true the clause
/// is satisfied and we can skip inspecting it (a standard MiniSAT-style optimization).
#[derive(Clone, Copy)]
struct Watcher {
    clause: usize,
    blocker: Lit,
}

const VAR_DECAY: f64 = 0.95;
const RESTART_BASE: u64 = 100;
const ACTIVITY_RESCALE_LIMIT: f64 = 1e100;

/// Evaluate a literal under a variable assignment. A free function (not a method) so it can be used
/// while other `Solver` fields are borrowed mutably.
#[inline]
fn lit_val(assigns: &[LBool], l: Lit) -> LBool {
    match assigns[l.var()] {
        LBool::Undef => LBool::Undef,
        LBool::True => {
            if l.is_neg() {
                LBool::False
            } else {
                LBool::True
            }
        }
        LBool::False => {
            if l.is_neg() {
                LBool::True
            } else {
                LBool::False
            }
        }
    }
}

/// The i-th term (1-indexed) of the Luby sequence: 1,1,2,1,1,2,4,1,1,2,1,1,2,4,8,...
fn luby(i: u64) -> u64 {
    let mut k = 1u64;
    loop {
        let pow = 1u64 << k; // 2^k
        if i == pow - 1 {
            return 1u64 << (k - 1);
        }
        let lo = 1u64 << (k - 1);
        if (lo..pow - 1).contains(&i) {
            return luby(i - lo + 1);
        }
        k += 1;
    }
}

/// An indexed binary max-heap over variables, keyed by VSIDS activity. Supports O(log n) insert,
/// pop-max, and sift-up after an activity bump.
struct VarHeap {
    heap: Vec<Var>,
    indices: Vec<i32>, // position of a var in `heap`, or -1 if absent
}

impl VarHeap {
    fn new(n: usize) -> VarHeap {
        VarHeap {
            heap: Vec::with_capacity(n),
            indices: vec![-1; n],
        }
    }

    fn up(&mut self, act: &[f64], mut i: usize) {
        let x = self.heap[i];
        let xa = act[x];
        while i > 0 {
            let par = (i - 1) / 2;
            if act[self.heap[par]] >= xa {
                break;
            }
            self.heap[i] = self.heap[par];
            self.indices[self.heap[i]] = i as i32;
            i = par;
        }
        self.heap[i] = x;
        self.indices[x] = i as i32;
    }

    fn down(&mut self, act: &[f64], mut i: usize) {
        let x = self.heap[i];
        let xa = act[x];
        let len = self.heap.len();
        loop {
            let l = 2 * i + 1;
            if l >= len {
                break;
            }
            let r = l + 1;
            let child = if r < len && act[self.heap[r]] > act[self.heap[l]] {
                r
            } else {
                l
            };
            if act[self.heap[child]] <= xa {
                break;
            }
            self.heap[i] = self.heap[child];
            self.indices[self.heap[i]] = i as i32;
            i = child;
        }
        self.heap[i] = x;
        self.indices[x] = i as i32;
    }

    fn insert(&mut self, act: &[f64], v: Var) {
        if self.indices[v] >= 0 {
            return;
        }
        let i = self.heap.len();
        self.heap.push(v);
        self.indices[v] = i as i32;
        self.up(act, i);
    }

    /// Sift a variable up after its activity increased.
    fn increase(&mut self, act: &[f64], v: Var) {
        let idx = self.indices[v];
        if idx >= 0 {
            self.up(act, idx as usize);
        }
    }

    fn pop_max(&mut self, act: &[f64]) -> Option<Var> {
        if self.heap.is_empty() {
            return None;
        }
        let top = self.heap[0];
        self.indices[top] = -1;
        let last = self.heap.pop().unwrap();
        if !self.heap.is_empty() {
            self.heap[0] = last;
            self.indices[last] = 0;
            self.down(act, 0);
        }
        Some(top)
    }
}

/// The CDCL solver. Owns a private copy of the clause database so `Cnf::solve(&self, ..)` stays
/// immutable in its public signature.
struct Solver {
    num_vars: usize,
    /// All clauses, original and learned. Each has length >= 2 (units are handled by direct
    /// level-0 assignment, never stored here).
    clauses: Vec<Vec<Lit>>,
    /// `watches[lit.code()]` holds every clause that has `lit.negate()` as one of its two watched
    /// literals — i.e. the clauses to inspect when `lit` is assigned true.
    watches: Vec<Vec<Watcher>>,
    assigns: Vec<LBool>,
    level: Vec<u32>,
    reason: Vec<Option<usize>>, // reason clause index, or None for decisions / level-0 units
    trail: Vec<Lit>,
    trail_lim: Vec<usize>, // trail index at which each decision level begins
    qhead: usize,          // propagation queue head into `trail`
    activity: Vec<f64>,
    var_inc: f64,
    order: VarHeap,
    polarity: Vec<bool>, // saved phase (the `neg` bit to reuse when re-deciding a var)
    seen: Vec<bool>,     // scratch for conflict analysis
    conflicts: u64,
}

impl Solver {
    fn new(n: usize) -> Solver {
        let activity = vec![0.0; n];
        let mut order = VarHeap::new(n);
        for v in 0..n {
            order.insert(&activity, v);
        }
        Solver {
            num_vars: n,
            clauses: Vec::new(),
            watches: vec![Vec::new(); 2 * n],
            assigns: vec![LBool::Undef; n],
            level: vec![0; n],
            reason: vec![None; n],
            trail: Vec::with_capacity(n),
            trail_lim: Vec::new(),
            qhead: 0,
            activity,
            var_inc: 1.0,
            order,
            polarity: vec![false; n],
            seen: vec![false; n],
            conflicts: 0,
        }
    }

    #[inline]
    fn decision_level(&self) -> u32 {
        self.trail_lim.len() as u32
    }

    /// Register a clause (length >= 2) in the database and set up its two watches. Returns its index.
    fn add_clause_internal(&mut self, lits: Vec<Lit>) -> usize {
        let cidx = self.clauses.len();
        let w0 = lits[0].negate();
        let w1 = lits[1].negate();
        self.watches[w0.code()].push(Watcher {
            clause: cidx,
            blocker: lits[1],
        });
        self.watches[w1.code()].push(Watcher {
            clause: cidx,
            blocker: lits[0],
        });
        self.clauses.push(lits);
        cidx
    }

    /// Assign a unit literal at level 0. Returns false if it directly contradicts an earlier unit.
    fn add_unit(&mut self, l: Lit) -> bool {
        match lit_val(&self.assigns, l) {
            LBool::True => true,
            LBool::False => false,
            LBool::Undef => {
                self.enqueue(l, None);
                true
            }
        }
    }

    /// Assign `l` true with the given reason, recording it on the trail at the current level.
    #[inline]
    fn enqueue(&mut self, l: Lit, reason: Option<usize>) {
        let v = l.var();
        self.assigns[v] = if l.is_neg() {
            LBool::False
        } else {
            LBool::True
        };
        self.level[v] = self.decision_level();
        self.reason[v] = reason;
        self.trail.push(l);
    }

    #[inline]
    fn new_decision_level(&mut self) {
        self.trail_lim.push(self.trail.len());
    }

    /// Two-watched-literal unit propagation to a fixpoint. Returns the index of a conflicting clause,
    /// or `None` if propagation completed with no conflict.
    fn propagate(&mut self) -> Option<usize> {
        while self.qhead < self.trail.len() {
            let p = self.trail[self.qhead];
            self.qhead += 1;
            let false_lit = p.negate();
            // Take ownership of p's watch list so we can mutate other watch lists while walking it.
            let mut ws = std::mem::take(&mut self.watches[p.code()]);
            let mut i = 0usize;
            let mut j = 0usize;
            let mut conflict: Option<usize> = None;

            while i < ws.len() {
                let w = ws[i];
                // Fast path: if the blocker is already true the clause is satisfied.
                if lit_val(&self.assigns, w.blocker) == LBool::True {
                    ws[j] = w;
                    i += 1;
                    j += 1;
                    continue;
                }
                let cidx = w.clause;
                // Make sure the falsified literal sits at position 1.
                if self.clauses[cidx][0] == false_lit {
                    self.clauses[cidx].swap(0, 1);
                }
                let first = self.clauses[cidx][0];
                let w2 = Watcher {
                    clause: cidx,
                    blocker: first,
                };
                // If the other watched literal is true, the clause is already satisfied.
                if first != w.blocker && lit_val(&self.assigns, first) == LBool::True {
                    ws[j] = w2;
                    i += 1;
                    j += 1;
                    continue;
                }
                // Search for a fresh, non-false literal to watch.
                let clen = self.clauses[cidx].len();
                let mut k = 2usize;
                let mut found = false;
                while k < clen {
                    let lk = self.clauses[cidx][k];
                    if lit_val(&self.assigns, lk) != LBool::False {
                        self.clauses[cidx].swap(1, k);
                        let nl = self.clauses[cidx][1].negate();
                        self.watches[nl.code()].push(w2);
                        found = true;
                        break;
                    }
                    k += 1;
                }
                if found {
                    i += 1;
                    continue;
                }
                // No new watch: the clause is unit (propagate `first`) or fully false (conflict).
                ws[j] = w2;
                i += 1;
                j += 1;
                match lit_val(&self.assigns, first) {
                    LBool::False => {
                        conflict = Some(cidx);
                        // Preserve the rest of the watch list unchanged.
                        while i < ws.len() {
                            ws[j] = ws[i];
                            i += 1;
                            j += 1;
                        }
                        break;
                    }
                    _ => self.enqueue(first, Some(cidx)),
                }
            }

            ws.truncate(j);
            self.watches[p.code()] = ws;
            if let Some(c) = conflict {
                self.qhead = self.trail.len();
                return Some(c);
            }
        }
        None
    }

    /// Bump a variable's VSIDS activity, rescaling all activities if it overflows.
    fn bump_var(&mut self, v: Var) {
        self.activity[v] += self.var_inc;
        if self.activity[v] > ACTIVITY_RESCALE_LIMIT {
            for a in self.activity.iter_mut() {
                *a *= 1e-100;
            }
            self.var_inc *= 1e-100;
        }
        self.order.increase(&self.activity, v);
    }

    #[inline]
    fn decay_var_inc(&mut self) {
        self.var_inc *= 1.0 / VAR_DECAY;
    }

    /// 1-UIP conflict analysis. Returns the learned clause (asserting literal at index 0) and the
    /// non-chronological backjump level.
    fn analyze(&mut self, conflict: usize) -> (Vec<Lit>, u32) {
        let dl = self.decision_level();
        let mut learnt: Vec<Lit> = vec![Lit::pos(0)]; // slot 0 reserved for the asserting literal
        let mut path_c: i32 = 0;
        let mut p: Option<Lit> = None;
        let mut index = self.trail.len();
        let mut confl = conflict;

        loop {
            // For a reason clause the propagated literal sits at index 0 — skip it.
            let start = if p.is_none() { 0 } else { 1 };
            let clen = self.clauses[confl].len();
            let mut k = start;
            while k < clen {
                let q = self.clauses[confl][k];
                let v = q.var();
                if !self.seen[v] && self.level[v] > 0 {
                    self.bump_var(v);
                    self.seen[v] = true;
                    if self.level[v] >= dl {
                        path_c += 1;
                    } else {
                        learnt.push(q);
                    }
                }
                k += 1;
            }

            // Walk back along the trail to the most recent seen literal at this level.
            loop {
                index -= 1;
                if self.seen[self.trail[index].var()] {
                    break;
                }
            }
            let plit = self.trail[index];
            let vp = plit.var();
            self.seen[vp] = false;
            path_c -= 1;
            p = Some(plit);
            if path_c <= 0 {
                break;
            }
            // A non-decision seen variable must have a reason clause.
            confl = self.reason[vp].expect("implied literal must have a reason");
        }

        // The remaining single literal at the current level is the 1-UIP; assert its negation.
        learnt[0] = p.unwrap().negate();

        let bt_level = if learnt.len() == 1 {
            0
        } else {
            // Move the highest-level literal (other than the asserting one) to index 1 so the two
            // watches are correct, and backjump to its level.
            let mut max_i = 1;
            let mut m = 2;
            while m < learnt.len() {
                if self.level[learnt[m].var()] > self.level[learnt[max_i].var()] {
                    max_i = m;
                }
                m += 1;
            }
            learnt.swap(1, max_i);
            self.level[learnt[1].var()]
        };

        for l in &learnt {
            self.seen[l.var()] = false;
        }
        (learnt, bt_level)
    }

    /// Undo all assignments made above `level`, restoring their variables to the decision heap and
    /// saving their phases.
    fn cancel_until(&mut self, level: u32) {
        if self.decision_level() <= level {
            return;
        }
        let target = self.trail_lim[level as usize];
        let mut c = self.trail.len();
        while c > target {
            c -= 1;
            let l = self.trail[c];
            let v = l.var();
            self.polarity[v] = self.assigns[v] == LBool::False; // remember the sign to reuse
            self.assigns[v] = LBool::Undef;
            self.reason[v] = None;
            self.order.insert(&self.activity, v);
        }
        self.trail.truncate(target);
        self.trail_lim.truncate(level as usize);
        self.qhead = target;
    }

    /// Pick the highest-activity unassigned variable, phased by its saved polarity.
    fn pick_branch(&mut self) -> Option<Lit> {
        loop {
            let v = self.order.pop_max(&self.activity)?;
            if self.assigns[v] == LBool::Undef {
                return Some(Lit::new(v, self.polarity[v]));
            }
        }
    }

    /// The CDCL search loop, bounded by a conflict budget.
    fn search(&mut self, budget: u64) -> SatResult {
        let mut conflicts_since_restart: u64 = 0;
        let mut restart_num: u64 = 1;
        let mut restart_limit = luby(restart_num) * RESTART_BASE;

        loop {
            match self.propagate() {
                Some(confl) => {
                    self.conflicts += 1;
                    conflicts_since_restart += 1;
                    if self.decision_level() == 0 {
                        // A conflict with no decisions in play is a root refutation ⇒ UNSAT.
                        return SatResult::Unsat;
                    }
                    if self.conflicts > budget {
                        return SatResult::Unknown;
                    }
                    let (learnt, bt_level) = self.analyze(confl);
                    self.cancel_until(bt_level);
                    if learnt.len() == 1 {
                        self.enqueue(learnt[0], None);
                    } else {
                        let asserting = learnt[0];
                        let cref = self.add_clause_internal(learnt);
                        self.enqueue(asserting, Some(cref));
                    }
                    self.decay_var_inc();
                }
                None => {
                    if conflicts_since_restart >= restart_limit {
                        self.cancel_until(0);
                        conflicts_since_restart = 0;
                        restart_num += 1;
                        restart_limit = luby(restart_num) * RESTART_BASE;
                        continue;
                    }
                    match self.pick_branch() {
                        None => {
                            // Every variable is assigned with no conflict ⇒ SAT. Extract the model.
                            let model = (0..self.num_vars)
                                .map(|v| self.assigns[v] == LBool::True)
                                .collect();
                            return SatResult::Sat(model);
                        }
                        Some(dlit) => {
                            self.new_decision_level();
                            self.enqueue(dlit, None);
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

    /// Evaluate a full model against a clause set: every clause must have a true literal.
    fn model_satisfies(clauses: &[Vec<Lit>], model: &[bool]) -> bool {
        clauses
            .iter()
            .all(|cl| cl.iter().any(|l| model[l.var()] ^ l.is_neg()))
    }

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
                assert!(model_satisfies(&c.clauses, &m));
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

    #[test]
    #[allow(clippy::needless_range_loop)] // combinatorial pigeon/hole indices read clearest as ranges
    fn pigeonhole_4_into_3_is_unsat() {
        // 4 pigeons into 3 holes: unsatisfiable. x[p][h] = pigeon p sits in hole h.
        let mut c = Cnf::new();
        let mut x = [[0usize; 3]; 4];
        for row in x.iter_mut() {
            for cell in row.iter_mut() {
                *cell = c.new_var();
            }
        }
        // At least one hole per pigeon.
        for row in &x {
            c.add_clause(row.iter().map(|&v| Lit::pos(v)).collect());
        }
        // At most one pigeon per hole.
        for h in 0..3 {
            for p in 0..4 {
                for q in (p + 1)..4 {
                    c.add_clause(vec![Lit::new(x[p][h], true), Lit::new(x[q][h], true)]);
                }
            }
        }
        assert_eq!(c.solve(100_000), SatResult::Unsat);
    }

    /// Deterministic xorshift64 RNG — no external crate.
    struct XorShift(u64);
    impl XorShift {
        fn new(seed: u64) -> XorShift {
            XorShift(seed)
        }
        fn next_u64(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
        fn range(&mut self, n: u64) -> u64 {
            self.next_u64() % n
        }
    }

    /// Exhaustive 2^vars truth-table check. Returns true iff the formula is satisfiable.
    fn brute_force_sat(num_vars: usize, clauses: &[Vec<Lit>]) -> bool {
        for mask in 0u32..(1u32 << num_vars) {
            let model: Vec<bool> = (0..num_vars).map(|v| (mask >> v) & 1 == 1).collect();
            if model_satisfies(clauses, &model) {
                return true;
            }
        }
        false
    }

    #[test]
    fn differential_vs_bruteforce() {
        let mut rng = XorShift::new(0x9E37_79B9_7F4A_7C15);
        let instances = 2000;
        let mut disagreements = 0;
        let mut checked = 0;
        for _ in 0..instances {
            let num_vars = 1 + rng.range(12) as usize; // 1..=12
            let num_clauses = rng.range(41) as usize; // 0..=40
            let mut c = Cnf::new();
            for _ in 0..num_vars {
                c.new_var();
            }
            for _ in 0..num_clauses {
                let k = 1 + rng.range(4) as usize; // 1..=4 literals
                let mut lits = Vec::with_capacity(k);
                for _ in 0..k {
                    let v = rng.range(num_vars as u64) as usize;
                    let neg = rng.range(2) == 1;
                    lits.push(Lit::new(v, neg));
                }
                c.add_clause(lits);
            }

            let bf = brute_force_sat(num_vars, &c.clauses);
            match c.solve(1_000_000) {
                SatResult::Sat(m) => {
                    // Every returned model MUST satisfy every clause.
                    assert!(
                        model_satisfies(&c.clauses, &m),
                        "CDCL returned an invalid model"
                    );
                    checked += 1;
                    if !bf {
                        disagreements += 1;
                    }
                }
                SatResult::Unsat => {
                    checked += 1;
                    if bf {
                        disagreements += 1;
                    }
                }
                SatResult::Unknown => { /* over budget — allowed, not compared */ }
            }
        }
        assert_eq!(
            disagreements, 0,
            "CDCL disagreed with brute force ({checked} definite verdicts checked)"
        );
        assert!(checked > 0, "differential test decided nothing");
    }
}
