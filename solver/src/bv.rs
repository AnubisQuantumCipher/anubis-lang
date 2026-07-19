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
            Add(a, b) | Sub(a, b) | Mul(a, b) | And(a, b) | Or(a, b) | Xor(a, b) | Shl(a, b)
            | Lshr(a, b) | Ashr(a, b) | Udiv(a, b) | Urem(a, b) | Sdiv(a, b) | Srem(a, b) => {
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
