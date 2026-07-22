//! QF_BV formula AST — the fixed interface between the SMT-LIB2 parser and the bit-blaster.
//!
//! Anubis's contract obligations are emitted as SMT-LIB2 in the theory of fixed-width bit-vectors
//! (all vars are `(_ BitVec 64)`; the encoder in `compiler/src/middle/mod.rs` matches the runtime's
//! `i64::wrapping_*` semantics — see `formal/Anubis/Encoding.lean`). This AST captures exactly that
//! fragment. Terms are bit-vectors of a known width; predicates are booleans over them.

/// A bit-vector term of a fixed width (bits). Every constructor's result width is determined by its
/// operands and recorded so the bit-blaster allocates the right number of SAT variables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Term {
    /// A declared constant `(declare-const name (_ BitVec w))` — a symbolic bit-vector variable.
    Var(String, u32),
    /// A literal `(_ bvN w)` or `#xHEX` / `#bBIN` — value masked to `w` bits.
    Const(u128, u32),
    /// Bitwise/arith binary ops, all width-preserving (both operands and result share `w`).
    Add(Box<Term>, Box<Term>),
    Sub(Box<Term>, Box<Term>),
    Mul(Box<Term>, Box<Term>),
    And(Box<Term>, Box<Term>),
    Or(Box<Term>, Box<Term>),
    Xor(Box<Term>, Box<Term>),
    Shl(Box<Term>, Box<Term>),
    /// Logical (zero-filling) right shift `bvlshr`.
    Lshr(Box<Term>, Box<Term>),
    /// Arithmetic (sign-filling) right shift `bvashr`.
    Ashr(Box<Term>, Box<Term>),
    /// Unsigned division / remainder (`bvudiv`/`bvurem`) — division by zero yields all-ones / the
    /// dividend, per SMT-LIB (the encoder guards real divides, but the theory total-izes them).
    Udiv(Box<Term>, Box<Term>),
    Urem(Box<Term>, Box<Term>),
    /// Signed division / remainder (`bvsdiv`/`bvsrem`, truncating toward zero, sign-of-dividend).
    Sdiv(Box<Term>, Box<Term>),
    Srem(Box<Term>, Box<Term>),
    /// Unary negation `bvneg` (two's complement) and bitwise not `bvnot`.
    Neg(Box<Term>),
    Not(Box<Term>),
    /// `((_ extract hi lo) t)` — bits [lo, hi], result width `hi - lo + 1`.
    Extract(u32, u32, Box<Term>),
    /// `(concat a b)` — result width `wa + wb`, `a` the high bits.
    Concat(Box<Term>, Box<Term>),
    /// `((_ zero_extend n) t)` / `((_ sign_extend n) t)` — result width `w + n`.
    ZeroExtend(u32, Box<Term>),
    SignExtend(u32, Box<Term>),
    /// `(ite p a b)` — a bit-vector if-then-else selected by a predicate.
    Ite(Box<Pred>, Box<Term>, Box<Term>),
}

/// A boolean predicate over bit-vector terms — the assertions in the obligation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pred {
    Const(bool),
    /// Equality of two same-width terms.
    Eq(Term, Term),
    /// Unsigned / signed comparisons.
    Ult(Term, Term),
    Ule(Term, Term),
    Ugt(Term, Term),
    Uge(Term, Term),
    Slt(Term, Term),
    Sle(Term, Term),
    Sgt(Term, Term),
    Sge(Term, Term),
    Not(Box<Pred>),
    And(Vec<Pred>),
    Or(Vec<Pred>),
    /// A boolean `(declare-const name Bool)` variable (rare in the BV fragment but permitted).
    BoolVar(String),
}

impl Term {
    /// The bit-width of this term, computed structurally. `None` if the term is malformed (mismatched
    /// operand widths) — the bit-blaster treats that as out-of-fragment and defers to z3.
    pub fn width(&self) -> Option<u32> {
        use Term::*;
        match self {
            Var(_, w) | Const(_, w) => Some(*w),
            Add(a, b)
            | Sub(a, b)
            | Mul(a, b)
            | And(a, b)
            | Or(a, b)
            | Xor(a, b)
            | Shl(a, b)
            | Lshr(a, b)
            | Ashr(a, b)
            | Udiv(a, b)
            | Urem(a, b)
            | Sdiv(a, b)
            | Srem(a, b) => {
                let (wa, wb) = (a.width()?, b.width()?);
                if wa == wb {
                    Some(wa)
                } else {
                    None
                }
            }
            Neg(a) | Not(a) => a.width(),
            Extract(hi, lo, a) => {
                let _ = a.width()?;
                if hi >= lo {
                    Some(hi - lo + 1)
                } else {
                    None
                }
            }
            Concat(a, b) => Some(a.width()? + b.width()?),
            ZeroExtend(n, a) | SignExtend(n, a) => Some(a.width()? + n),
            Ite(_, a, b) => {
                let (wa, wb) = (a.width()?, b.width()?);
                if wa == wb {
                    Some(wa)
                } else {
                    None
                }
            }
        }
    }
}

/// A parsed obligation: the declared variables and the conjoined assertions. Satisfiable iff there is
/// an assignment to the vars making every assertion true. (Anubis asks: is `assumptions ∧ ¬property`
/// SAT? UNSAT ⇒ the property is proven.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Formula {
    /// Declared bit-vector constants: (name, width).
    pub bv_vars: Vec<(String, u32)>,
    /// Declared boolean constants.
    pub bool_vars: Vec<String>,
    /// The asserted predicates (implicitly conjoined).
    pub asserts: Vec<Pred>,
}

// ---- Concrete evaluation (the native REPLAY): evaluate the formula under a ground model. ----
//
// This is the independent check that a SAT model actually satisfies the formula — the native
// equivalent of the compiler's z3 counterexample replay (Phase-4 B1: every FAIL model must replay).
// Semantics mirror SMT-LIB (and hence `blast.rs` and `run.rs`) exactly. `Udiv/Urem/Sdiv/Srem` return
// `None` (decline): the bit-blaster never emits them, so a model of a blasted formula cannot contain
// them — refusing beats risking a divergent re-implementation of their total-ized semantics.

/// Mask a value to `w` bits (`w` in 1..=128).
fn mask(v: u128, w: u32) -> u128 {
    if w >= 128 {
        v
    } else {
        v & ((1u128 << w) - 1)
    }
}

impl Term {
    /// Evaluate under `env` (bit-vector variable values, already in-range for their widths).
    /// Returns `(value, width)` with the value masked to the width, or `None` if the term is
    /// malformed (width mismatch, oversize, div/rem, unbound var).
    pub fn eval(
        &self,
        env: &std::collections::HashMap<String, u128>,
        bool_env: &std::collections::HashMap<String, bool>,
    ) -> Option<(u128, u32)> {
        use Term::*;
        // Width-preserving binary ops share this shape: evaluate both, require equal widths.
        let bin = |a: &Term, b: &Term| -> Option<(u128, u128, u32)> {
            let (va, wa) = a.eval(env, bool_env)?;
            let (vb, wb) = b.eval(env, bool_env)?;
            if wa == wb {
                Some((va, vb, wa))
            } else {
                None
            }
        };
        match self {
            Var(name, w) => Some((mask(*env.get(name)?, *w), *w)),
            Const(v, w) => Some((mask(*v, *w), *w)),
            Add(a, b) => bin(a, b).map(|(x, y, w)| (mask(x.wrapping_add(y), w), w)),
            Sub(a, b) => bin(a, b).map(|(x, y, w)| (mask(x.wrapping_sub(y), w), w)),
            Mul(a, b) => bin(a, b).map(|(x, y, w)| (mask(x.wrapping_mul(y), w), w)),
            And(a, b) => bin(a, b).map(|(x, y, w)| (x & y, w)),
            Or(a, b) => bin(a, b).map(|(x, y, w)| (x | y, w)),
            Xor(a, b) => bin(a, b).map(|(x, y, w)| (x ^ y, w)),
            Shl(a, b) => bin(a, b).map(|(x, y, w)| {
                if y >= w as u128 {
                    (0, w)
                } else {
                    (mask(x << y, w), w)
                }
            }),
            Lshr(a, b) => bin(a, b).map(
                |(x, y, w)| {
                    if y >= w as u128 {
                        (0, w)
                    } else {
                        (x >> y, w)
                    }
                },
            ),
            Ashr(a, b) => bin(a, b).and_then(|(x, y, w)| {
                if w == 0 {
                    return None;
                }
                let m = mask(u128::MAX, w);
                let sign = (x >> (w - 1)) & 1 == 1;
                if y >= w as u128 {
                    Some((if sign { m } else { 0 }, w))
                } else if y == 0 {
                    Some((x, w))
                } else {
                    let sh = x >> y;
                    // Sign-fill the vacated high `y` bits: the mask's high-y-bit slice.
                    Some((if sign { sh | (m ^ (m >> y)) } else { sh }, w))
                }
            }),
            Udiv(..) | Urem(..) | Sdiv(..) | Srem(..) => None,
            Neg(a) => {
                let (v, w) = a.eval(env, bool_env)?;
                Some((mask(v.wrapping_neg(), w), w))
            }
            Not(a) => {
                let (v, w) = a.eval(env, bool_env)?;
                Some((v ^ mask(u128::MAX, w), w))
            }
            Extract(hi, lo, a) => {
                let (v, w) = a.eval(env, bool_env)?;
                if *hi >= w || hi < lo {
                    return None;
                }
                let ow = hi - lo + 1;
                Some((mask(v >> lo, ow), ow))
            }
            Concat(a, b) => {
                let (va, wa) = a.eval(env, bool_env)?;
                let (vb, wb) = b.eval(env, bool_env)?;
                if wa + wb > 128 {
                    return None;
                }
                Some(((va << wb) | vb, wa + wb))
            }
            ZeroExtend(n, a) => {
                let (v, w) = a.eval(env, bool_env)?;
                if w + n > 128 {
                    return None;
                }
                Some((v, w + n))
            }
            SignExtend(n, a) => {
                let (v, w) = a.eval(env, bool_env)?;
                let ow = w + n;
                if ow > 128 || w == 0 {
                    return None;
                }
                let sign = (v >> (w - 1)) & 1 == 1;
                Some((
                    if sign {
                        v | (mask(u128::MAX, ow) ^ mask(u128::MAX, w))
                    } else {
                        v
                    },
                    ow,
                ))
            }
            Ite(p, a, b) => {
                if p.eval(env, bool_env)? {
                    a.eval(env, bool_env)
                } else {
                    b.eval(env, bool_env)
                }
            }
        }
    }
}

impl Pred {
    /// Evaluate under the model. Signed orders use the flip-MSB (offset binary) mapping — the same
    /// trick `blast.rs::slt` uses and `formal/Anubis/BitBlast.lean::slt_correct` proves.
    pub fn eval(
        &self,
        env: &std::collections::HashMap<String, u128>,
        bool_env: &std::collections::HashMap<String, bool>,
    ) -> Option<bool> {
        use Pred::*;
        // Evaluate a comparison pair to same-width values; signed variants flip the MSB first.
        let pair = |a: &Term, b: &Term, signed: bool| -> Option<(u128, u128)> {
            let (va, wa) = a.eval(env, bool_env)?;
            let (vb, wb) = b.eval(env, bool_env)?;
            if wa != wb || wa == 0 {
                return None;
            }
            if signed {
                let flip = 1u128 << (wa - 1);
                Some((va ^ flip, vb ^ flip))
            } else {
                Some((va, vb))
            }
        };
        match self {
            Const(b) => Some(*b),
            Eq(a, b) => pair(a, b, false).map(|(x, y)| x == y),
            Ult(a, b) => pair(a, b, false).map(|(x, y)| x < y),
            Ule(a, b) => pair(a, b, false).map(|(x, y)| x <= y),
            Ugt(a, b) => pair(a, b, false).map(|(x, y)| x > y),
            Uge(a, b) => pair(a, b, false).map(|(x, y)| x >= y),
            Slt(a, b) => pair(a, b, true).map(|(x, y)| x < y),
            Sle(a, b) => pair(a, b, true).map(|(x, y)| x <= y),
            Sgt(a, b) => pair(a, b, true).map(|(x, y)| x > y),
            Sge(a, b) => pair(a, b, true).map(|(x, y)| x >= y),
            Not(p) => p.eval(env, bool_env).map(|b| !b),
            And(ps) => {
                for p in ps {
                    if !p.eval(env, bool_env)? {
                        return Some(false);
                    }
                }
                Some(true)
            }
            Or(ps) => {
                for p in ps {
                    if p.eval(env, bool_env)? {
                        return Some(true);
                    }
                }
                Some(false)
            }
            BoolVar(name) => bool_env.get(name).copied(),
        }
    }
}

impl Formula {
    /// Does the model satisfy EVERY assertion? `None` if any assertion cannot be evaluated.
    pub fn eval(
        &self,
        env: &std::collections::HashMap<String, u128>,
        bool_env: &std::collections::HashMap<String, bool>,
    ) -> Option<bool> {
        for p in &self.asserts {
            if !p.eval(env, bool_env)? {
                return Some(false);
            }
        }
        Some(true)
    }
}
