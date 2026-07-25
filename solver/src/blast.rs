//! Bit-blaster: QF_BV `Formula` → CNF, via Tseitin gate encoding. Each bit-vector term becomes a
//! `Vec<Lit>` (LSB first), each predicate a single `Lit`, and the whole formula is asserted true.
//!
//! The gate semantics MUST match the runtime (and z3): the ripple-carry adder is `i64::wrapping_add`,
//! the comparators are the signed/unsigned orders of `formal/Anubis/Encoding.lean`. Slice 1 supports
//! the ops that dominate contract obligations — const/var, and/or/xor/not, neg, add/sub, all six
//! signed+unsigned comparisons, eq, extract/concat/{zero,sign}_extend, ite, and CONSTANT shifts.
//! Anything else (mul, div/rem, variable shifts) returns `None`, so the caller defers to z3 — never a
//! wrong answer.

use crate::bv::{Formula, Pred, Term};
use crate::sat::{Cnf, Lit, Var};
use std::collections::HashMap;

pub fn blast(f: &Formula, cnf: &mut Cnf) -> Option<()> {
    blast_with_map(f, cnf).map(|_| ())
}

/// The variable→literal correspondence produced by a successful blast: which CNF literal carries each
/// bit of each declared bit-vector (LSB first), and each boolean. Reading these literals out of a SAT
/// assignment reconstructs a concrete SMT-level model (see `lib.rs::native_check_sat_model`).
pub struct BlastMap {
    pub bv: HashMap<String, Vec<Lit>>,
    pub bools: HashMap<String, Lit>,
}

pub fn blast_with_map(f: &Formula, cnf: &mut Cnf) -> Option<BlastMap> {
    let mut b = Blaster::new(cnf);
    // Declare bit-vars for every symbolic constant.
    for (name, w) in &f.bv_vars {
        let bits: Vec<Lit> = (0..*w).map(|_| Lit::pos(b.cnf.new_var())).collect();
        b.vars.insert(name.clone(), bits);
    }
    for name in &f.bool_vars {
        b.bool_vars.insert(name.clone(), Lit::pos(b.cnf.new_var()));
    }
    // Assert every predicate (implicit conjunction): unit-clause each to true.
    for p in &f.asserts {
        let l = b.blast_pred(p)?;
        b.cnf.add_clause(vec![l]);
    }
    Some(BlastMap {
        bv: b.vars,
        bools: b.bool_vars,
    })
}

struct Blaster<'a> {
    cnf: &'a mut Cnf,
    vars: HashMap<String, Vec<Lit>>,
    bool_vars: HashMap<String, Lit>,
    /// A literal forced TRUE (its negation is the constant FALSE), for constant bits.
    ctrue: Lit,
}

impl<'a> Blaster<'a> {
    fn new(cnf: &'a mut Cnf) -> Blaster<'a> {
        let t: Var = cnf.new_var();
        let ctrue = Lit::pos(t);
        cnf.add_clause(vec![ctrue]); // force true
        Blaster {
            cnf,
            vars: HashMap::new(),
            bool_vars: HashMap::new(),
            ctrue,
        }
    }

    #[inline]
    fn tt(&self) -> Lit {
        self.ctrue
    }
    #[inline]
    fn ff(&self) -> Lit {
        self.ctrue.negate()
    }

    // ---- Tseitin gates: fresh output literal constrained to the gate's function ----

    fn and2(&mut self, a: Lit, b: Lit) -> Lit {
        let c = Lit::pos(self.cnf.new_var());
        // c ↔ (a ∧ b)
        self.cnf.add_clause(vec![c.negate(), a]);
        self.cnf.add_clause(vec![c.negate(), b]);
        self.cnf.add_clause(vec![c, a.negate(), b.negate()]);
        c
    }

    fn or2(&mut self, a: Lit, b: Lit) -> Lit {
        let c = Lit::pos(self.cnf.new_var());
        // c ↔ (a ∨ b)
        self.cnf.add_clause(vec![c, a.negate()]);
        self.cnf.add_clause(vec![c, b.negate()]);
        self.cnf.add_clause(vec![c.negate(), a, b]);
        c
    }

    fn xor2(&mut self, a: Lit, b: Lit) -> Lit {
        let c = Lit::pos(self.cnf.new_var());
        // c ↔ (a ⊕ b)
        self.cnf.add_clause(vec![c.negate(), a, b]);
        self.cnf
            .add_clause(vec![c.negate(), a.negate(), b.negate()]);
        self.cnf.add_clause(vec![c, a.negate(), b]);
        self.cnf.add_clause(vec![c, a, b.negate()]);
        c
    }

    /// `sel ? a : b`.
    fn mux(&mut self, sel: Lit, a: Lit, b: Lit) -> Lit {
        let c = Lit::pos(self.cnf.new_var());
        // sel → (c ↔ a) ; ¬sel → (c ↔ b)
        self.cnf.add_clause(vec![sel.negate(), c.negate(), a]);
        self.cnf.add_clause(vec![sel.negate(), c, a.negate()]);
        self.cnf.add_clause(vec![sel, c.negate(), b]);
        self.cnf.add_clause(vec![sel, c, b.negate()]);
        c
    }

    fn full_adder(&mut self, a: Lit, b: Lit, cin: Lit) -> (Lit, Lit) {
        let axb = self.xor2(a, b);
        let sum = self.xor2(axb, cin);
        // cout = (a ∧ b) ∨ (cin ∧ (a ⊕ b))
        let ab = self.and2(a, b);
        let cx = self.and2(cin, axb);
        let cout = self.or2(ab, cx);
        (sum, cout)
    }

    /// Ripple-carry add of equal-width bit-vectors (LSB first). Returns (sum bits, carry-out).
    fn add_carry(&mut self, a: &[Lit], b: &[Lit], cin: Lit) -> (Vec<Lit>, Lit) {
        let mut carry = cin;
        let mut sum = Vec::with_capacity(a.len());
        for i in 0..a.len() {
            let (s, c) = self.full_adder(a[i], b[i], carry);
            sum.push(s);
            carry = c;
        }
        (sum, carry)
    }

    fn add(&mut self, a: &[Lit], b: &[Lit]) -> Vec<Lit> {
        let ff = self.ff();
        self.add_carry(a, b, ff).0
    }

    /// Multiply by a CONSTANT multiplier: `x * c = Σ_{i : bit i of c set} (x << i)`, accumulated with
    /// the ripple adder (so the result is taken mod 2^w — exactly bit-vector multiply).
    fn const_mul(&mut self, a: &Term, b: &Term) -> Option<Vec<Lit>> {
        let (x_term, c, w) = match (a, b) {
            (_, Term::Const(c, w)) => (a, *c, *w as usize),
            (Term::Const(c, w), _) => (b, *c, *w as usize),
            _ => return None, // variable × variable → var_mul
        };
        let x = self.blast_term(x_term)?;
        if x.len() != w {
            return None;
        }
        let ff = self.ff();
        let mut acc = vec![ff; w];
        for i in 0..w {
            if (c >> i) & 1 == 1 {
                // x << i, truncated to w bits (LSB-first): bits [i..w) = x[0..w-i], low i bits zero.
                let mut shifted = vec![ff; w];
                shifted[i..].copy_from_slice(&x[..w - i]);
                acc = self.add(&acc, &shifted);
            }
        }
        Some(acc)
    }

    /// Variable × variable schoolbook multiply: `x * y = Σ_i (y[i] ? (x << i) : 0)` mod 2^w.
    /// Reuses const left-shift wiring, per-bit AND, and the ripple adder — the same family Lean
    /// checks for `mulConst` / `addBits` / `shlConst`. See `mulVar_correct` in BitBlast.lean.
    fn var_mul(&mut self, a: &Term, b: &Term) -> Option<Vec<Lit>> {
        let (av, bv) = (self.blast_term(a)?, self.blast_term(b)?);
        if av.len() != bv.len() || av.is_empty() {
            return None;
        }
        let w = av.len();
        let ff = self.ff();
        let mut acc = vec![ff; w];
        for i in 0..w {
            // partial = y[i] ? (x << i) : 0  — AND each bit of shifted x with selector y[i]
            let mut shifted = vec![ff; w];
            shifted[i..].copy_from_slice(&av[..w - i]);
            let partial: Vec<Lit> = shifted.iter().map(|&bit| self.and2(bit, bv[i])).collect();
            acc = self.add(&acc, &partial);
        }
        Some(acc)
    }

    /// a - b = a + ~b + 1. Returns (diff, carry-out); carry-out = 1 iff a >= b (unsigned).
    fn sub_carry(&mut self, a: &[Lit], b: &[Lit]) -> (Vec<Lit>, Lit) {
        let nb: Vec<Lit> = b.iter().map(|l| l.negate()).collect();
        let tt = self.tt();
        self.add_carry(a, &nb, tt)
    }

    /// AND-reduce a list of literals into one (true iff all true). Empty ⇒ true.
    fn and_all(&mut self, lits: &[Lit]) -> Lit {
        if lits.is_empty() {
            return self.tt();
        }
        let mut acc = lits[0];
        for &l in &lits[1..] {
            acc = self.and2(acc, l);
        }
        acc
    }

    fn eq_bits(&mut self, a: &[Lit], b: &[Lit]) -> Option<Lit> {
        if a.len() != b.len() {
            return None;
        }
        let xnors: Vec<Lit> = (0..a.len())
            .map(|i| self.xor2(a[i], b[i]).negate())
            .collect();
        Some(self.and_all(&xnors))
    }

    /// Unsigned a < b: a - b borrows, i.e. the add a + ~b + 1 has carry-out 0.
    fn ult(&mut self, a: &[Lit], b: &[Lit]) -> Option<Lit> {
        if a.len() != b.len() {
            return None;
        }
        let (_, cout) = self.sub_carry(a, b);
        Some(cout.negate())
    }

    /// Signed a < b: flip the sign bits (add 2^(w-1) to both) then unsigned-compare.
    fn slt(&mut self, a: &[Lit], b: &[Lit]) -> Option<Lit> {
        if a.len() != b.len() || a.is_empty() {
            return None;
        }
        let mut a2 = a.to_vec();
        let mut b2 = b.to_vec();
        let top = a.len() - 1;
        a2[top] = a2[top].negate();
        b2[top] = b2[top].negate();
        self.ult(&a2, &b2)
    }

    // ---- Terms → bit literals (LSB first) ----

    fn blast_term(&mut self, t: &Term) -> Option<Vec<Lit>> {
        let w = t.width()?;
        match t {
            Term::Var(name, _) => self.vars.get(name).cloned(),
            Term::Const(v, w) => {
                let (tt, ff) = (self.tt(), self.ff());
                Some(
                    (0..*w)
                        .map(|i| if (v >> i) & 1 == 1 { tt } else { ff })
                        .collect(),
                )
            }
            Term::Not(a) => Some(self.blast_term(a)?.iter().map(|l| l.negate()).collect()),
            Term::And(a, b) => self.bitwise(a, b, |s, x, y| s.and2(x, y)),
            Term::Or(a, b) => self.bitwise(a, b, |s, x, y| s.or2(x, y)),
            Term::Xor(a, b) => self.bitwise(a, b, |s, x, y| s.xor2(x, y)),
            Term::Add(a, b) => {
                let (av, bv) = (self.blast_term(a)?, self.blast_term(b)?);
                if av.len() != bv.len() {
                    return None;
                }
                Some(self.add(&av, &bv))
            }
            Term::Sub(a, b) => {
                let (av, bv) = (self.blast_term(a)?, self.blast_term(b)?);
                if av.len() != bv.len() {
                    return None;
                }
                Some(self.sub_carry(&av, &bv).0)
            }
            Term::Neg(a) => {
                let av = self.blast_term(a)?;
                let ff = self.ff();
                let zeros = vec![ff; av.len()];
                Some(self.sub_carry(&zeros, &av).0)
            }
            Term::Extract(hi, lo, a) => {
                let av = self.blast_term(a)?;
                if (*hi as usize) >= av.len() || hi < lo {
                    return None;
                }
                Some(av[(*lo as usize)..=(*hi as usize)].to_vec())
            }
            Term::Concat(a, b) => {
                // a is the HIGH part; result LSB-first is [b bits, a bits].
                let (av, bv) = (self.blast_term(a)?, self.blast_term(b)?);
                let mut out = bv;
                out.extend(av);
                Some(out)
            }
            Term::ZeroExtend(n, a) => {
                let mut av = self.blast_term(a)?;
                let ff = self.ff();
                av.extend(std::iter::repeat_n(ff, *n as usize));
                Some(av)
            }
            Term::SignExtend(n, a) => {
                let av = self.blast_term(a)?;
                let sign = *av.last()?;
                let mut out = av.clone();
                out.extend(std::iter::repeat_n(sign, *n as usize));
                Some(out)
            }
            // Constant shift amount → cheap direct wiring; variable amount → barrel shifter.
            Term::Shl(a, b) => self
                .const_shift(a, b, ShiftKind::Left)
                .or_else(|| self.var_shift(a, b, ShiftKind::Left)),
            Term::Lshr(a, b) => self
                .const_shift(a, b, ShiftKind::LogicalRight)
                .or_else(|| self.var_shift(a, b, ShiftKind::LogicalRight)),
            Term::Ashr(a, b) => self
                .const_shift(a, b, ShiftKind::ArithRight)
                .or_else(|| self.var_shift(a, b, ShiftKind::ArithRight)),
            Term::Ite(p, a, b) => {
                let sel = self.blast_pred(p)?;
                let (av, bv) = (self.blast_term(a)?, self.blast_term(b)?);
                if av.len() != bv.len() {
                    return None;
                }
                Some((0..av.len()).map(|i| self.mux(sel, av[i], bv[i])).collect())
            }
            // Multiply: const path preferred (fewer gates); else schoolbook var×var.
            // Division / remainder still deferred to z3 (no Lean blast proof yet).
            Term::Mul(a, b) => self.const_mul(a, b).or_else(|| self.var_mul(a, b)),
            Term::Udiv(..) | Term::Urem(..) | Term::Sdiv(..) | Term::Srem(..) => {
                let _ = w;
                None
            }
        }
    }

    fn bitwise(
        &mut self,
        a: &Term,
        b: &Term,
        gate: fn(&mut Self, Lit, Lit) -> Lit,
    ) -> Option<Vec<Lit>> {
        let (av, bv) = (self.blast_term(a)?, self.blast_term(b)?);
        if av.len() != bv.len() {
            return None;
        }
        Some((0..av.len()).map(|i| gate(self, av[i], bv[i])).collect())
    }

    /// Shift by a CONSTANT amount only (a variable shift amount needs a barrel shifter — deferred).
    fn const_shift(&mut self, a: &Term, b: &Term, kind: ShiftKind) -> Option<Vec<Lit>> {
        let amount = match b {
            Term::Const(v, _) => *v as usize,
            _ => return None, // variable shift → defer
        };
        let av = self.blast_term(a)?;
        let w = av.len();
        let ff = self.ff();
        let fill = match kind {
            ShiftKind::ArithRight => *av.last()?,
            _ => ff,
        };
        let mut out = vec![ff; w];
        for (i, cell) in out.iter_mut().enumerate() {
            *cell = match kind {
                ShiftKind::Left => {
                    if i >= amount {
                        av[i - amount]
                    } else {
                        ff
                    }
                }
                ShiftKind::LogicalRight | ShiftKind::ArithRight => {
                    if i + amount < w {
                        av[i + amount]
                    } else {
                        fill
                    }
                }
            };
        }
        Some(out)
    }

    /// Shift by a VARIABLE amount via a log-depth barrel shifter: for each bit `k` of the amount `b`,
    /// conditionally shift the running value by `2^k` (a `mux` on bit `k`). This matches SMT-LIB
    /// `bvshl`/`bvlshr`/`bvashr` — including "shift ≥ width ⇒ 0 (or all-sign, arithmetic)", because a
    /// layer whose `2^k ≥ w` shifts every bit out, and any set amount-bit at position `≥ log2 w` (up to
    /// bit w-1) selects that all-out layer. Every gate is the already-proven `mux`.
    fn var_shift(&mut self, a: &Term, b: &Term, kind: ShiftKind) -> Option<Vec<Lit>> {
        let av = self.blast_term(a)?;
        let bv = self.blast_term(b)?;
        let w = av.len();
        if bv.len() != w {
            return None;
        }
        let ff = self.ff();
        let mut cur = av;
        for (k, &bk) in bv.iter().enumerate() {
            if k >= usize::BITS as usize {
                break; // 2^k unrepresentable; unreachable for the ≤64-bit widths this system emits
            }
            let amount = 1usize << k;
            // For arithmetic right, fill with the sign bit — right-shifting preserves the MSB, so the
            // running value's MSB is still the original sign at every layer.
            let fill = match kind {
                ShiftKind::ArithRight => *cur.last()?,
                _ => ff,
            };
            let shifted: Vec<Lit> = (0..w)
                .map(|i| match kind {
                    ShiftKind::Left => {
                        if i >= amount {
                            cur[i - amount]
                        } else {
                            ff
                        }
                    }
                    ShiftKind::LogicalRight | ShiftKind::ArithRight => {
                        if i + amount < w {
                            cur[i + amount]
                        } else {
                            fill
                        }
                    }
                })
                .collect();
            cur = (0..w).map(|i| self.mux(bk, shifted[i], cur[i])).collect();
        }
        Some(cur)
    }

    // ---- Predicates → one literal (true iff the predicate holds) ----

    fn blast_pred(&mut self, p: &Pred) -> Option<Lit> {
        match p {
            Pred::Const(true) => Some(self.tt()),
            Pred::Const(false) => Some(self.ff()),
            Pred::BoolVar(name) => self.bool_vars.get(name).copied(),
            Pred::Not(q) => Some(self.blast_pred(q)?.negate()),
            Pred::And(qs) => {
                let ls: Vec<Lit> = qs
                    .iter()
                    .map(|q| self.blast_pred(q))
                    .collect::<Option<_>>()?;
                Some(self.and_all(&ls))
            }
            Pred::Or(qs) => {
                let ls: Vec<Lit> = qs
                    .iter()
                    .map(|q| self.blast_pred(q))
                    .collect::<Option<_>>()?;
                if ls.is_empty() {
                    return Some(self.ff());
                }
                let mut acc = ls[0];
                for &l in &ls[1..] {
                    acc = self.or2(acc, l);
                }
                Some(acc)
            }
            Pred::Eq(a, b) => {
                let (av, bv) = (self.blast_term(a)?, self.blast_term(b)?);
                self.eq_bits(&av, &bv)
            }
            Pred::Ult(a, b) => self.cmp(a, b, Cmp::Ult),
            Pred::Ule(a, b) => self.cmp(a, b, Cmp::Ule),
            Pred::Ugt(a, b) => self.cmp(b, a, Cmp::Ult),
            Pred::Uge(a, b) => self.cmp(b, a, Cmp::Ule),
            Pred::Slt(a, b) => self.cmp(a, b, Cmp::Slt),
            Pred::Sle(a, b) => self.cmp(a, b, Cmp::Sle),
            Pred::Sgt(a, b) => self.cmp(b, a, Cmp::Slt),
            Pred::Sge(a, b) => self.cmp(b, a, Cmp::Sle),
        }
    }

    fn cmp(&mut self, a: &Term, b: &Term, kind: Cmp) -> Option<Lit> {
        let (av, bv) = (self.blast_term(a)?, self.blast_term(b)?);
        if av.len() != bv.len() {
            return None;
        }
        match kind {
            Cmp::Ult => self.ult(&av, &bv),
            Cmp::Slt => self.slt(&av, &bv),
            // a <= b  ≡  ¬(b < a)
            Cmp::Ule => Some(self.ult(&bv, &av)?.negate()),
            Cmp::Sle => Some(self.slt(&bv, &av)?.negate()),
        }
    }
}

#[derive(Clone, Copy)]
enum ShiftKind {
    Left,
    LogicalRight,
    ArithRight,
}

#[derive(Clone, Copy)]
enum Cmp {
    Ult,
    Ule,
    Slt,
    Sle,
}
