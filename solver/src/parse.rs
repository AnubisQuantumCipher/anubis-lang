//! A focused SMT-LIB2 parser for exactly the QF_BV fragment `compiler/src/middle/mod.rs` emits.
//!
//! It is deliberately CONSERVATIVE: any construct it does not recognize — a non-BV sort (String,
//! Float, Array), an unsupported op, malformed syntax — makes the whole parse return `None`, so the
//! caller defers to z3. It never guesses. This keeps the native path sound: we only ever decide a
//! formula we fully understood.

use crate::bv::{Formula, Pred, Term};

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
                let logic = list.get(1)?.as_atom()?;
                // Accept only pure bit-vector logics. Anything mentioning FP/strings/arrays/UF is out.
                if !logic.contains("BV")
                    || logic.contains("FP")
                    || logic.contains("F")
                    || logic.contains('S') && logic != "QF_BV"
                {
                    // conservative: only QF_BV (and BV-only variants without F/S) are in-fragment
                    if logic != "QF_BV" && logic != "BV" && logic != "QF_UFBV" {
                        return None;
                    }
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
        // (_ BitVec w)
        Sexp::List(items) => {
            if items.len() == 3
                && items[0].as_atom()? == "_"
                && items[1].as_atom()? == "BitVec"
            {
                Some(Sort::Bv(items[2].as_atom()?.parse().ok()?))
            } else {
                None
            }
        }
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
                    _ => None,
                };
            }
            let head = items.first()?.as_atom()?;
            match head {
                // (_ bvN w)
                "_" => {
                    let lit = args.first()?.as_atom()?;
                    let val = lit.strip_prefix("bv")?;
                    let v: u128 = val.parse().ok()?;
                    let w: u32 = args.get(1)?.as_atom()?.parse().ok()?;
                    Some(Term::Const(v, w))
                }
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
