//! IEEE-754 **Float64 → QF_BV lowering** for the comparison subset of QF_FP.
//!
//! A `Float64` is exactly a `BitVec 64`, so a floating-point obligation over comparisons is decided by
//! REWRITING it into a pure bit-vector formula and handing it to the existing (machine-checked) BV
//! bit-blaster — no new gates, no rounding logic, nothing new to trust. The only floating-point
//! subtlety is the *order*: IEEE comparison is not the bit order. We use the standard monotonic-key
//! transform so that unsigned BV order on the key matches float order, then special-case NaN
//! (unordered) and ±0 (equal).
//!
//! Rounding arithmetic (`fp.add/sub/mul/div/rem/fma/sqrt/roundToIntegral`) is NOT lowered — the parser
//! declines it (→ `None` → defer to z3). We only ever decide what we fully and faithfully rewrote.
//!
//! Bit layout (MSB first): bit 63 = sign, bits 62..52 = exponent (11 bits), bits 51..0 = mantissa.

use crate::bv::{Pred, Term};

pub const W: u32 = 64;
const EXP_ONES: u128 = 0x7FF; // 11-bit exponent all ones
const SIGN_MASK: u128 = 0x8000_0000_0000_0000;

/// Bit patterns of the special floating-point literals (`(_ +oo 11 53)` etc.).
pub fn plus_inf() -> u128 {
    0x7FF0_0000_0000_0000
}
pub fn minus_inf() -> u128 {
    0xFFF0_0000_0000_0000
}
pub fn plus_zero() -> u128 {
    0x0
}
pub fn minus_zero() -> u128 {
    SIGN_MASK
}
/// A canonical quiet NaN (any NaN pattern is equivalent under the comparisons we lower).
pub fn nan() -> u128 {
    0x7FF8_0000_0000_0000
}

/// Parse an SMT-LIB decimal/rational floating-point literal (`4.0`, `-3.5`, `1.0`, `10`) to its
/// Float64 bit pattern. Anything not exactly representable as a Rust `f64` parse declines (`None`).
pub fn decimal_to_bits(s: &str) -> Option<u128> {
    let v: f64 = s.parse().ok()?;
    if v.is_nan() {
        return None; // a NaN spelled as a decimal is unexpected — decline rather than guess
    }
    Some(v.to_bits() as u128)
}

fn extract(hi: u32, lo: u32, x: &Term) -> Term {
    Term::Extract(hi, lo, Box::new(x.clone()))
}
fn exp(x: &Term) -> Term {
    extract(62, 52, x)
}
fn mant(x: &Term) -> Term {
    extract(51, 0, x)
}
/// The sign bit is set (value is negative or -0).
fn sign_set(x: &Term) -> Pred {
    Pred::Eq(extract(63, 63, x), Term::Const(1, 1))
}
fn exp_all_ones(x: &Term) -> Pred {
    Pred::Eq(exp(x), Term::Const(EXP_ONES, 11))
}
fn exp_zero(x: &Term) -> Pred {
    Pred::Eq(exp(x), Term::Const(0, 11))
}
fn mant_zero(x: &Term) -> Pred {
    Pred::Eq(mant(x), Term::Const(0, 52))
}

/// `x` is NaN: exponent all ones AND mantissa non-zero.
pub fn is_nan(x: &Term) -> Pred {
    Pred::And(vec![exp_all_ones(x), Pred::Not(Box::new(mant_zero(x)))])
}
/// `x` is ±∞: exponent all ones AND mantissa zero.
pub fn is_inf(x: &Term) -> Pred {
    Pred::And(vec![exp_all_ones(x), mant_zero(x)])
}
/// `x` is ±0: exponent zero AND mantissa zero.
pub fn is_zero(x: &Term) -> Pred {
    Pred::And(vec![exp_zero(x), mant_zero(x)])
}

/// The monotonic ordering key: `sign ? ~x : x ^ 0x8000…0`. For any two non-NaN values, unsigned `<`
/// on the keys equals IEEE `<` — EXCEPT that it orders -0 below +0, which the callers correct with an
/// explicit both-zero guard.
fn key(x: &Term) -> Term {
    Term::Ite(
        Box::new(sign_set(x)),
        Box::new(Term::Not(Box::new(x.clone()))),
        Box::new(Term::Xor(
            Box::new(x.clone()),
            Box::new(Term::Const(SIGN_MASK, W)),
        )),
    )
}

fn both_zero(a: &Term, b: &Term) -> Pred {
    Pred::And(vec![is_zero(a), is_zero(b)])
}
fn neither_nan(a: &Term, b: &Term) -> Pred {
    Pred::And(vec![
        Pred::Not(Box::new(is_nan(a))),
        Pred::Not(Box::new(is_nan(b))),
    ])
}

/// `fp.lt a b`: neither NaN, not both zero, and key(a) <u key(b).
pub fn fp_lt(a: &Term, b: &Term) -> Pred {
    Pred::And(vec![
        neither_nan(a, b),
        Pred::Not(Box::new(both_zero(a, b))),
        Pred::Ult(key(a), key(b)),
    ])
}

/// `fp.eq a b` (IEEE equality): neither NaN, and (same bits OR both zero — so +0 == -0).
pub fn fp_eq(a: &Term, b: &Term) -> Pred {
    Pred::And(vec![
        neither_nan(a, b),
        Pred::Or(vec![Pred::Eq(a.clone(), b.clone()), both_zero(a, b)]),
    ])
}

pub fn fp_leq(a: &Term, b: &Term) -> Pred {
    Pred::Or(vec![fp_lt(a, b), fp_eq(a, b)])
}
pub fn fp_gt(a: &Term, b: &Term) -> Pred {
    fp_lt(b, a)
}
pub fn fp_geq(a: &Term, b: &Term) -> Pred {
    fp_leq(b, a)
}
