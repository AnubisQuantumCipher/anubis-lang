//! A focused SMT-LIB2 parser for exactly the QF_BV fragment `compiler/src/middle/mod.rs` emits.
//!
//! It is deliberately CONSERVATIVE: any construct it does not recognize — a non-BV sort (String,
//! Float, Array), an unsupported op, malformed syntax — makes the whole parse return `None`, so the
//! caller defers to z3. It never guesses. This keeps the native path sound: we only ever decide a
//! formula we fully understood.

use crate::bv::{Formula, Pred, Term};
use crate::fp;

/// Parse an SMT-LIB2 script into a QF_BV `Formula`, or `None` if it is outside the supported fragment.
pub fn parse_smt2(input: &str) -> Option<Formula> {
    let sexps = tokenize(input)?;
    let mut bv_vars: Vec<(String, u32)> = Vec::new();
    let mut bool_vars: Vec<String> = Vec::new();
    let mut asserts: Vec<Pred> = Vec::new();

    for cmd in &sexps {
        let list = cmd.as_list()?;
        let head = list.first()?.as_atom()?;
        match head {
            "set-logic" => {
                // Bit-vector logics, plus QF_FP (Float64 is lowered to BitVec 64 — see `fp.rs`).
                // Everything else (strings, arrays, reals, UF beyond QF_UFBV) is out of fragment.
                let logic = list.get(1)?.as_atom()?;
                if !matches!(logic, "QF_BV" | "BV" | "QF_UFBV" | "QF_FP") {
                    return None;
                }
            }
            "declare-const" => {
                let name = list.get(1)?.as_atom()?.to_string();
                let sort = list.get(2)?;
                match parse_sort(sort)? {
                    Sort::Bv(w) => bv_vars.push((name, w)),
                    Sort::Bool => bool_vars.push(name),
                }
            }
            "declare-fun" => {
                // (declare-fun name () sort) — nullary only.
                let name = list.get(1)?.as_atom()?.to_string();
                if !list.get(2)?.as_list()?.is_empty() {
                    return None; // a real function, not a constant → out of fragment
                }
                match parse_sort(list.get(3)?)? {
                    Sort::Bv(w) => bv_vars.push((name, w)),
                    Sort::Bool => bool_vars.push(name),
                }
            }
            "assert" => {
                let ctx = Ctx {
                    bv_vars: &bv_vars,
                    bool_vars: &bool_vars,
                };
                asserts.push(parse_pred(list.get(1)?, &ctx)?);
            }
            // Non-constraint commands we can safely ignore.
            "check-sat" | "get-model" | "get-value" | "set-option" | "set-info" | "push"
            | "pop" | "exit" | "reset" | "echo" => {}
            // An unknown command is out-of-fragment.
            _ => return None,
        }
    }
    Some(Formula {
        bv_vars,
        bool_vars,
        asserts,
    })
}

enum Sort {
    Bv(u32),
    Bool,
}

fn parse_sort(s: &Sexp) -> Option<Sort> {
    match s {
        Sexp::Atom(a) if a == "Bool" => Some(Sort::Bool),
        Sexp::List(items) => {
            let head = items.first()?.as_atom()?;
            // (_ BitVec w)
            if items.len() == 3 && head == "_" && items[1].as_atom()? == "BitVec" {
                let w: u32 = items[2].as_atom()?.parse().ok()?;
                // Width 0 is degenerate (no sign bit, no value bits) and width > 128 exceeds the
                // evaluator's u128 model values — both out of the supported fragment.
                if w == 0 || w > 128 {
                    return None;
                }
                return Some(Sort::Bv(w));
            }
            // (_ FloatingPoint 11 53) — Float64 only; lowered to a 64-bit bit-vector (see fp.rs).
            if items.len() == 4
                && head == "_"
                && items[1].as_atom()? == "FloatingPoint"
                && items[2].as_atom()? == "11"
                && items[3].as_atom()? == "53"
            {
                return Some(Sort::Bv(fp::W));
            }
            None
        }
        // `Float64` is an alias some frontends emit for `(_ FloatingPoint 11 53)`.
        Sexp::Atom(a) if a == "Float64" => Some(Sort::Bv(fp::W)),
        _ => None,
    }
}

struct Ctx<'a> {
    bv_vars: &'a [(String, u32)],
    bool_vars: &'a [String],
}

impl<'a> Ctx<'a> {
    fn bv_width(&self, name: &str) -> Option<u32> {
        self.bv_vars.iter().find(|(n, _)| n == name).map(|(_, w)| *w)
    }
    fn is_bool(&self, name: &str) -> bool {
        self.bool_vars.iter().any(|n| n == name)
    }
}

fn parse_pred(s: &Sexp, ctx: &Ctx) -> Option<Pred> {
    match s {
        Sexp::Atom(a) => match a.as_str() {
            "true" => Some(Pred::Const(true)),
            "false" => Some(Pred::Const(false)),
            name if ctx.is_bool(name) => Some(Pred::BoolVar(name.to_string())),
            _ => None,
        },
        Sexp::List(items) => {
            let head = items.first()?.as_atom()?;
            let args = &items[1..];
            match head {
                "not" if args.len() == 1 => Some(Pred::Not(Box::new(parse_pred(&args[0], ctx)?))),
                "and" => Some(Pred::And(args.iter().map(|a| parse_pred(a, ctx)).collect::<Option<_>>()?)),
                "or" => Some(Pred::Or(args.iter().map(|a| parse_pred(a, ctx)).collect::<Option<_>>()?)),
                "=" if args.len() == 2 => {
                    // could be BV equality or boolean equality; try BV first
                    if let (Some(a), Some(b)) = (parse_term(&args[0], ctx), parse_term(&args[1], ctx)) {
                        Some(Pred::Eq(a, b))
                    } else {
                        // boolean =: a ↔ b  ≡  (a→b)∧(b→a). Represent via And/Or.
                        let a = parse_pred(&args[0], ctx)?;
                        let b = parse_pred(&args[1], ctx)?;
                        Some(Pred::And(vec![
                            Pred::Or(vec![Pred::Not(Box::new(a.clone())), b.clone()]),
                            Pred::Or(vec![Pred::Not(Box::new(b)), a]),
                        ]))
                    }
                }
                "bvult" if args.len() == 2 => bin_pred(args, ctx, Pred::Ult),
                "bvule" if args.len() == 2 => bin_pred(args, ctx, Pred::Ule),
                "bvugt" if args.len() == 2 => bin_pred(args, ctx, Pred::Ugt),
                "bvuge" if args.len() == 2 => bin_pred(args, ctx, Pred::Uge),
                "bvslt" if args.len() == 2 => bin_pred(args, ctx, Pred::Slt),
                "bvsle" if args.len() == 2 => bin_pred(args, ctx, Pred::Sle),
                "bvsgt" if args.len() == 2 => bin_pred(args, ctx, Pred::Sgt),
                "bvsge" if args.len() == 2 => bin_pred(args, ctx, Pred::Sge),
                // Floating-point comparisons, lowered to BV via the monotonic-key transform (fp.rs).
                // Operands parse as ordinary 64-bit BV terms (an fp var/const IS a BitVec 64); an fp
                // arithmetic subterm parses to None here, declining the whole predicate → defer to z3.
                "fp.lt" if args.len() == 2 => Some(fp::fp_lt(
                    &parse_term(&args[0], ctx)?,
                    &parse_term(&args[1], ctx)?,
                )),
                "fp.leq" if args.len() == 2 => Some(fp::fp_leq(
                    &parse_term(&args[0], ctx)?,
                    &parse_term(&args[1], ctx)?,
                )),
                "fp.gt" if args.len() == 2 => Some(fp::fp_gt(
                    &parse_term(&args[0], ctx)?,
                    &parse_term(&args[1], ctx)?,
                )),
                "fp.geq" if args.len() == 2 => Some(fp::fp_geq(
                    &parse_term(&args[0], ctx)?,
                    &parse_term(&args[1], ctx)?,
                )),
                "fp.eq" if args.len() == 2 => Some(fp::fp_eq(
                    &parse_term(&args[0], ctx)?,
                    &parse_term(&args[1], ctx)?,
                )),
                "fp.isNaN" if args.len() == 1 => Some(fp::is_nan(&parse_term(&args[0], ctx)?)),
                "fp.isInfinite" if args.len() == 1 => Some(fp::is_inf(&parse_term(&args[0], ctx)?)),
                "fp.isZero" if args.len() == 1 => Some(fp::is_zero(&parse_term(&args[0], ctx)?)),
                "ite" if args.len() == 3 => {
                    // boolean ite → (c∧t)∨(¬c∧e)
                    let c = parse_pred(&args[0], ctx)?;
                    let t = parse_pred(&args[1], ctx)?;
                    let e = parse_pred(&args[2], ctx)?;
                    Some(Pred::Or(vec![
                        Pred::And(vec![c.clone(), t]),
                        Pred::And(vec![Pred::Not(Box::new(c)), e]),
                    ]))
                }
                _ => None,
            }
        }
    }
}

fn bin_pred(args: &[Sexp], ctx: &Ctx, f: fn(Term, Term) -> Pred) -> Option<Pred> {
    Some(f(parse_term(&args[0], ctx)?, parse_term(&args[1], ctx)?))
}

fn parse_term(s: &Sexp, ctx: &Ctx) -> Option<Term> {
    match s {
        Sexp::Atom(a) => {
            // #xHEX / #bBIN literals, or a declared BV var.
            if let Some(hex) = a.strip_prefix("#x") {
                let v = u128::from_str_radix(hex, 16).ok()?;
                Some(Term::Const(v, (hex.len() as u32) * 4))
            } else if let Some(bin) = a.strip_prefix("#b") {
                let v = u128::from_str_radix(bin, 2).ok()?;
                Some(Term::Const(v, bin.len() as u32))
            } else {
                ctx.bv_width(a).map(|w| Term::Var(a.to_string(), w))
            }
        }
        Sexp::List(items) => {
            let args = &items[1..];
            // Indexed op: ((_ extract hi lo) t) / ((_ zero_extend n) t) / ((_ sign_extend n) t) —
            // the head is itself a list, so handle it before treating the head as an atom.
            if let Some(Sexp::List(op)) = items.first() {
                let opname = op.get(1)?.as_atom()?;
                return match opname {
                    "extract" => {
                        let hi: u32 = op.get(2)?.as_atom()?.parse().ok()?;
                        let lo: u32 = op.get(3)?.as_atom()?.parse().ok()?;
                        Some(Term::Extract(hi, lo, Box::new(parse_term(args.first()?, ctx)?)))
                    }
                    "zero_extend" => {
                        let n: u32 = op.get(2)?.as_atom()?.parse().ok()?;
                        Some(Term::ZeroExtend(n, Box::new(parse_term(args.first()?, ctx)?)))
                    }
                    "sign_extend" => {
                        let n: u32 = op.get(2)?.as_atom()?.parse().ok()?;
                        Some(Term::SignExtend(n, Box::new(parse_term(args.first()?, ctx)?)))
                    }
                    // ((_ to_fp 11 53) <rounding> <decimal>) — a Float64 literal. The rounding mode is
                    // irrelevant for a decimal (Rust's f64 parse is round-to-nearest-even, matching
                    // RNE). Only Float64 and a plain (or negated) decimal literal are accepted.
                    "to_fp"
                        if op.get(2)?.as_atom()? == "11" && op.get(3)?.as_atom()? == "53" =>
                    {
                        let lit = args.get(1)?;
                        let bits = match lit {
                            Sexp::Atom(a) => fp::decimal_to_bits(a)?,
                            // (- 4.0) — a negated decimal literal.
                            Sexp::List(neg)
                                if neg.len() == 2 && neg[0].as_atom()? == "-" =>
                            {
                                let m = fp::decimal_to_bits(neg[1].as_atom()?)?;
                                m ^ 0x8000_0000_0000_0000 // flip the sign bit
                            }
                            _ => return None,
                        };
                        Some(Term::Const(bits, fp::W))
                    }
                    _ => None,
                };
            }
            let head = items.first()?.as_atom()?;
            match head {
                // (_ bvN w) bit-vector literal, or a Float64 special value (_ +oo 11 53) etc.
                "_" => {
                    let lit = args.first()?.as_atom()?;
                    if let Some(val) = lit.strip_prefix("bv") {
                        let v: u128 = val.parse().ok()?;
                        let w: u32 = args.get(1)?.as_atom()?.parse().ok()?;
                        return Some(Term::Const(v, w));
                    }
                    // Float64 special values (11-bit exp, 53-bit significand tag).
                    if args.get(1)?.as_atom()? == "11" && args.get(2)?.as_atom()? == "53" {
                        let bits = match lit {
                            "+oo" => fp::plus_inf(),
                            "-oo" => fp::minus_inf(),
                            "+zero" => fp::plus_zero(),
                            "-zero" => fp::minus_zero(),
                            "NaN" => fp::nan(),
                            _ => return None,
                        };
                        return Some(Term::Const(bits, fp::W));
                    }
                    None
                }
                // (fp #b<sign,1> #b<exp,11> #b<significand,52>) — a Float64 literal from three
                // bit-vector fields; concatenated into the 64-bit pattern.
                "fp" if args.len() == 3 => {
                    let s = bv_lit_value(&args[0])?;
                    let e = bv_lit_value(&args[1])?;
                    let m = bv_lit_value(&args[2])?;
                    Some(Term::Const((s << 63) | (e << 52) | m, fp::W))
                }
                // fp.neg / fp.abs are EXACT sign-bit operations (no rounding), so they lower to bvxor
                // / bvand with the sign mask — flipping/clearing bit 63. (NaN stays NaN, ±0 flip.) All
                // other fp arithmetic rounds and is declined (falls through to `_ => None`).
                "fp.neg" if args.len() == 1 => Some(Term::Xor(
                    Box::new(parse_term(&args[0], ctx)?),
                    Box::new(Term::Const(0x8000_0000_0000_0000, fp::W)),
                )),
                "fp.abs" if args.len() == 1 => Some(Term::And(
                    Box::new(parse_term(&args[0], ctx)?),
                    Box::new(Term::Const(0x7FFF_FFFF_FFFF_FFFF, fp::W)),
                )),
                "bvadd" => bin_term(args, ctx, Term::Add),
                "bvsub" => bin_term(args, ctx, Term::Sub),
                "bvmul" => bin_term(args, ctx, Term::Mul),
                "bvand" => bin_term(args, ctx, Term::And),
                "bvor" => bin_term(args, ctx, Term::Or),
                "bvxor" => bin_term(args, ctx, Term::Xor),
                "bvshl" => bin_term(args, ctx, Term::Shl),
                "bvlshr" => bin_term(args, ctx, Term::Lshr),
                "bvashr" => bin_term(args, ctx, Term::Ashr),
                "bvudiv" => bin_term(args, ctx, Term::Udiv),
                "bvurem" => bin_term(args, ctx, Term::Urem),
                "bvsdiv" => bin_term(args, ctx, Term::Sdiv),
                "bvsrem" => bin_term(args, ctx, Term::Srem),
                "bvneg" if args.len() == 1 => Some(Term::Neg(Box::new(parse_term(&args[0], ctx)?))),
                "bvnot" if args.len() == 1 => Some(Term::Not(Box::new(parse_term(&args[0], ctx)?))),
                "concat" if args.len() == 2 => bin_term(args, ctx, Term::Concat),
                "ite" if args.len() == 3 => {
                    let c = parse_pred(&args[0], ctx)?;
                    let t = parse_term(&args[1], ctx)?;
                    let e = parse_term(&args[2], ctx)?;
                    Some(Term::Ite(Box::new(c), Box::new(t), Box::new(e)))
                }
                _ => None,
            }
        }
    }
}

/// Parse a `#x…` / `#b…` bit-vector literal atom to its numeric value (ignoring width).
fn bv_lit_value(s: &Sexp) -> Option<u128> {
    let a = s.as_atom()?;
    if let Some(hex) = a.strip_prefix("#x") {
        u128::from_str_radix(hex, 16).ok()
    } else if let Some(bin) = a.strip_prefix("#b") {
        u128::from_str_radix(bin, 2).ok()
    } else {
        None
    }
}

fn bin_term(args: &[Sexp], ctx: &Ctx, f: fn(Box<Term>, Box<Term>) -> Term) -> Option<Term> {
    if args.len() != 2 {
        return None;
    }
    Some(f(
        Box::new(parse_term(&args[0], ctx)?),
        Box::new(parse_term(&args[1], ctx)?),
    ))
}

// ---- minimal s-expression reader ----

#[derive(Debug, Clone)]
enum Sexp {
    Atom(String),
    List(Vec<Sexp>),
}

impl Sexp {
    fn as_atom(&self) -> Option<&str> {
        match self {
            Sexp::Atom(a) => Some(a),
            _ => None,
        }
    }
    fn as_list(&self) -> Option<&[Sexp]> {
        match self {
            Sexp::List(l) => Some(l),
            _ => None,
        }
    }
}

/// Read all top-level s-expressions. Returns `None` on unbalanced parens.
fn tokenize(input: &str) -> Option<Vec<Sexp>> {
    let mut chars = input.chars().peekable();
    let mut out = Vec::new();
    loop {
        skip_ws(&mut chars);
        match chars.peek() {
            None => break,
            Some('(') => out.push(read_sexp(&mut chars)?),
            Some(_) => out.push(Sexp::Atom(read_atom(&mut chars))),
        }
    }
    Some(out)
}

fn skip_ws(chars: &mut std::iter::Peekable<std::str::Chars>) {
    while let Some(&c) = chars.peek() {
        if c == ';' {
            // line comment
            for c in chars.by_ref() {
                if c == '\n' {
                    break;
                }
            }
        } else if c.is_whitespace() {
            chars.next();
        } else {
            break;
        }
    }
}

fn read_sexp(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<Sexp> {
    // consume '('
    chars.next();
    let mut items = Vec::new();
    loop {
        skip_ws(chars);
        match chars.peek() {
            None => return None, // unbalanced
            Some(')') => {
                chars.next();
                return Some(Sexp::List(items));
            }
            Some('(') => items.push(read_sexp(chars)?),
            Some(_) => items.push(Sexp::Atom(read_atom(chars))),
        }
    }
}

fn read_atom(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut s = String::new();
    // |quoted symbol|
    if chars.peek() == Some(&'|') {
        chars.next();
        for c in chars.by_ref() {
            if c == '|' {
                break;
            }
            s.push(c);
        }
        return s;
    }
    // "string literal" — kept verbatim (BV fragment shouldn't contain these; parse_term ignores them)
    if chars.peek() == Some(&'"') {
        s.push('"');
        chars.next();
        while let Some(&c) = chars.peek() {
            s.push(c);
            chars.next();
            if c == '"' {
                break;
            }
        }
        return s;
    }
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() || c == '(' || c == ')' {
            break;
        }
        s.push(c);
        chars.next();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_declare_and_assert() {
        let f = parse_smt2(
            "(set-logic QF_BV)\n(declare-const x (_ BitVec 64))\n(assert (= (bvadd x (_ bv1 64)) (_ bv3 64)))\n(check-sat)\n",
        )
        .unwrap();
        assert_eq!(f.bv_vars, vec![("x".to_string(), 64)]);
        assert_eq!(f.asserts.len(), 1);
    }

    #[test]
    fn declines_string_theory() {
        assert!(parse_smt2("(set-logic QF_S)\n(declare-const s String)\n(check-sat)\n").is_none());
    }

    #[test]
    fn parses_extract() {
        let f = parse_smt2(
            "(declare-const x (_ BitVec 64))\n(assert (= ((_ extract 31 0) x) (_ bv0 32)))\n",
        )
        .unwrap();
        assert_eq!(f.asserts.len(), 1);
    }
}
