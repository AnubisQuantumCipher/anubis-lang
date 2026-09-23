//! A focused SMT-LIB2 parser for exactly the QF_BV fragment `compiler/src/middle/mod.rs` emits.
//!
//! It is deliberately CONSERVATIVE: any construct it does not recognize — a non-BV sort (String,
//! Float, Array), an unsupported op, malformed syntax — makes the whole parse return `None`, so the
//! caller defers to z3. It never guesses. This keeps the native path sound: we only ever decide a
//! formula we fully understood.

use crate::bv::{Formula, Pred, Term};
use crate::fp;
use std::cell::RefCell;
use std::collections::HashMap;

/// String terms are decided as EQUALITY LOGIC lowered to fixed-width bit-vectors: each DISTINCT string
/// literal becomes a distinct constant, each `String` variable a free bit-vector, and `=` is bit-vector
/// equality. 32 bits gives 2^32 possible values — vastly more than the number of string terms in any
/// obligation — so a variable can always take any literal's id OR a fresh value (a string not among the
/// literals), which is exactly string-equality satisfiability. Only `=` is supported; any `str.*`
/// operation is out of fragment (declines to z3). A native bug here cannot cause a false *proof* as
/// long as the domain exceeds the term count, which `MAX_STR_TERMS` enforces (else decline).
const STR_W: u32 = 32;
const MAX_STR_TERMS: usize = 1 << 20; // 2^32 domain ≫ this ⇒ vars can always be forced pairwise-distinct

#[derive(Default)]
struct StrInterner {
    ids: HashMap<String, u128>,
}
impl StrInterner {
    /// Assign (or look up) a stable distinct id for a decoded string.
    fn intern(&mut self, s: String) -> u128 {
        let n = self.ids.len() as u128;
        *self.ids.entry(s).or_insert(n)
    }
}

/// Turn an SMT-LIB string literal atom (including its surrounding quotes) into its canonical character
/// content, so two spellings of the same string intern to the same id and match z3's equality. SMT-LIB
/// escapes: a doubled quote `""` is one `"`, and `\u{H..}` / `\uHHHH` are code points; every other
/// character is literal.
fn unquote_and_decode(atom: &str) -> Option<String> {
    let inner = atom.strip_prefix('"')?.strip_suffix('"')?;
    let cs: Vec<char> = inner.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < cs.len() {
        if cs[i] == '"' {
            // A `""` inside the literal is one quote character; a lone `"` shouldn't occur here.
            if cs.get(i + 1) == Some(&'"') {
                out.push('"');
                i += 2;
                continue;
            }
            return None;
        }
        if cs[i] == '\\' && cs.get(i + 1) == Some(&'u') {
            // SMT-LIB 2.6 (and z3): `\u{d}` .. `\u{ddddd}` (1-5 hex digits) or `\udddd` (exactly 4),
            // denoting a code point <= 0x2FFFF. Anything else is literal text — `\u{+41}` and
            // `\u{30000}` are several characters, not one. A surrogate code point (0xD800-0xDFFF) is a
            // valid SMT-LIB character that a Rust `char` cannot hold: decline rather than misread it.
            let hex_run = |from: usize, max: usize| {
                cs[from.min(cs.len())..]
                    .iter()
                    .take(max)
                    .take_while(|c| c.is_ascii_hexdigit())
                    .count()
            };
            let escape = if cs.get(i + 2) == Some(&'{') {
                let n = hex_run(i + 3, 6);
                (1..=5)
                    .contains(&n)
                    .then_some(())
                    .filter(|_| cs.get(i + 3 + n) == Some(&'}'))
                    .map(|_| (i + 3, n, 3 + n + 1))
            } else {
                (hex_run(i + 2, 4) == 4).then_some((i + 2, 4, 6))
            };
            if let Some((start, n, len)) = escape {
                let hex: String = cs[start..start + n].iter().collect();
                let cp = u32::from_str_radix(&hex, 16).ok()?;
                if cp <= 0x2FFFF {
                    out.push(char::from_u32(cp)?);
                    i += len;
                    continue;
                }
            }
        }
        out.push(cs[i]);
        i += 1;
    }
    Some(out)
}

/// Definitions introduced by the lowering so that no operand tree is ever COPIED (see `share_term`).
#[derive(Default)]
struct Shared {
    next: usize,
    bv_vars: Vec<(String, u32)>,
    bool_vars: Vec<String>,
    defs: Vec<Pred>,
    /// Hash-consing: an identical operand reuses its variable, so two textual copies of the same
    /// subterm lower to ONE node (as in z3) and the solver never has to prove them equivalent.
    terms: HashMap<Term, Term>,
    preds: HashMap<Pred, Pred>,
}

/// Prefix of every variable the lowering introduces. It contains a space, which no symbol this parser
/// accepts can contain (atoms end at whitespace and quoted `|symbols|` are declined), so an introduced
/// name can never collide with a declared one — and callers can recognise it (`is_introduced`).
const INTRODUCED_PREFIX: &str = "anubis share ";

/// Whether `name` is a variable the lowering introduced rather than one the query declared. Such
/// variables are internal: they are stripped from a SAT model before it leaves the solver.
pub fn is_introduced(name: &str) -> bool {
    name.starts_with(INTRODUCED_PREFIX)
}

/// Bind a non-atomic TERM to a fresh bit-vector variable `v` with the definition `v = t`, and return
/// `v`. Lowerings that mention an operand several times (Float64 `=`, the `fp.*` comparisons, the
/// NaN/Inf/zero tests) use the variable instead of cloning the tree, so nesting them grows the lowered
/// formula linearly instead of exponentially. Sound in every polarity: `v` is fresh and `t` is a total
/// function of the other variables, so every model of the original extends uniquely to the definition.
fn share_term(t: Term, ctx: &Ctx) -> Option<Term> {
    if matches!(t, Term::Var(..) | Term::Const(..)) {
        return Some(t);
    }
    let w = t.width()?;
    let mut sh = ctx.shared.borrow_mut();
    if let Some(v) = sh.terms.get(&t) {
        return Some(v.clone());
    }
    let name = format!("{INTRODUCED_PREFIX}{}", sh.next);
    sh.next += 1;
    let v = Term::Var(name.clone(), w);
    sh.bv_vars.push((name, w));
    sh.defs.push(Pred::Eq(v.clone(), t.clone()));
    sh.terms.insert(t, v.clone());
    Some(v)
}

/// Bind a non-atomic PREDICATE to a fresh Boolean `b` with the definition `b ⇔ p`, and return `b`.
/// Same purpose and soundness argument as `share_term`, for Boolean `=` and `ite`. The definition holds
/// `p` twice, but `p`'s own duplicated operands are already variables, so the total stays linear.
fn share_pred(p: Pred, ctx: &Ctx) -> Pred {
    if matches!(p, Pred::Const(_) | Pred::BoolVar(_)) {
        return p;
    }
    let mut sh = ctx.shared.borrow_mut();
    if let Some(b) = sh.preds.get(&p) {
        return b.clone();
    }
    let name = format!("{INTRODUCED_PREFIX}{}", sh.next);
    sh.next += 1;
    sh.bool_vars.push(name.clone());
    let b = Pred::BoolVar(name);
    sh.defs
        .push(Pred::Or(vec![Pred::Not(Box::new(b.clone())), p.clone()]));
    sh.defs
        .push(Pred::Or(vec![b.clone(), Pred::Not(Box::new(p.clone()))]));
    sh.preds.insert(p, b.clone());
    b
}

/// Parse an SMT-LIB2 script into a QF_BV `Formula`, or `None` if it is outside the supported fragment.
///
/// Runs on the solver's own stack (`crate::on_solver_stack`), so the nesting this accepts
/// (`MAX_SEXP_DEPTH`) cannot overflow the caller's thread, whatever its stack size.
pub fn parse_smt2(input: &str) -> Option<Formula> {
    crate::on_solver_stack(|| parse_smt2_here(input)).flatten()
}

/// `parse_smt2` on the CURRENT stack — for callers already running on the solver stack.
pub(crate) fn parse_smt2_here(input: &str) -> Option<Formula> {
    let sexps = tokenize(input)?;
    let mut bv_vars: Vec<(String, u32)> = Vec::new();
    let mut bool_vars: Vec<String> = Vec::new();
    // The DECLARED sort of every term variable, kept because lowering erases it (see `TermSort`).
    let mut sorts: HashMap<String, TermSort> = HashMap::new();
    let mut asserts: Vec<Pred> = Vec::new();
    let strings: RefCell<StrInterner> = RefCell::new(StrInterner::default());
    let mut str_var_count = 0usize;
    let mut checked = false;
    // The declared logic restricts which theories z3 accepts; a query using one outside it is an
    // error in z3 ("unknown sort", "unknown constant bvult"), so it must decline here too.
    let mut logic: Option<Logic> = None;
    let mut first_command = true;
    let shared: RefCell<Shared> = RefCell::new(Shared::default());

    for cmd in &sexps {
        let list = cmd.as_list()?;
        let head = list.first()?.as_atom()?;
        // The formula decided is the conjunction of every assertion, so the script must be ONE
        // query: nothing may be declared or asserted after `check-sat` (a second query), and
        // assertion-stack commands (`push`/`pop`/`reset`) and `exit` — which change or cut off the set
        // of assertions z3 checks — are declined rather than ignored.
        if checked && !matches!(head, "get-model" | "get-value" | "set-info" | "exit") {
            return None;
        }
        let is_first = std::mem::replace(&mut first_command, false);
        match head {
            "set-logic" => {
                // Bit-vector logics, plus QF_FP (Float64 lowered to BitVec 64 — see `fp.rs`) and QF_S
                // (string EQUALITY lowered to bit-vectors by interning — see `unquote_and_decode`).
                // Everything else (arrays, reals, UF beyond QF_UFBV) is out of fragment.
                // Once, and first: z3 rejects a second `set-logic`, and a logic set after
                // declarations would not govern them.
                if !is_first || list.len() != 2 {
                    return None;
                }
                logic = Some(match list[1].as_atom()? {
                    "QF_BV" | "BV" | "QF_UFBV" => Logic::Bv,
                    "QF_FP" => Logic::Fp,
                    "QF_S" => Logic::Str,
                    _ => return None,
                });
            }
            "declare-const" | "declare-fun" => {
                // (declare-const name sort) / (declare-fun name () sort) — nullary only.
                let name = list.get(1)?.as_atom()?.to_string();
                let sort = if head == "declare-const" {
                    if list.len() != 3 {
                        return None;
                    }
                    parse_sort(&list[2])?
                } else {
                    if list.len() != 4 || !list[2].as_list()?.is_empty() {
                        return None; // a real function, not a constant → out of fragment
                    }
                    parse_sort(&list[3])?
                };
                // z3 rejects a sort its declared logic does not include (`unknown sort`).
                let allowed = matches!(
                    (logic, &sort),
                    (None, _)
                        | (_, Sort::Bool)
                        | (Some(Logic::Bv), Sort::Bv(_))
                        | (Some(Logic::Fp), Sort::Fp)
                        | (Some(Logic::Str), Sort::Str)
                );
                if !allowed {
                    return None;
                }
                // A second declaration of a name is an error in SMT-LIB (z3 rejects it); here it
                // would silently resolve every use to the FIRST declaration's sort.
                if sorts.contains_key(&name) || bool_vars.contains(&name) {
                    return None;
                }
                match sort {
                    Sort::Bv(w) => {
                        sorts.insert(name.clone(), TermSort::Bv(w));
                        bv_vars.push((name, w));
                    }
                    Sort::Fp => {
                        sorts.insert(name.clone(), TermSort::Fp);
                        bv_vars.push((name, fp::W));
                    }
                    Sort::Bool => bool_vars.push(name),
                    Sort::Str => {
                        sorts.insert(name.clone(), TermSort::Str);
                        bv_vars.push((name, STR_W));
                        str_var_count += 1;
                    }
                }
            }
            "assert" => {
                if list.len() != 2 {
                    return None;
                }
                let ctx = Ctx {
                    bv_vars: &bv_vars,
                    bool_vars: &bool_vars,
                    sorts: &sorts,
                    strings: &strings,
                    logic,
                    shared: &shared,
                };
                // P-SORT-1: decide only a WELL-SORTED assertion. The lowering below maps Float64 to
                // `BitVec 64` and String to `BitVec 32`, so an operator applied to the wrong sort
                // (`bvsgt` over a Float64) would otherwise bit-blast as ordinary bit-vector logic
                // and could be "proved" with a valid certificate for a CNF the query never meant.
                if !pred_well_sorted(&list[1], &ctx) {
                    return None;
                }
                asserts.push(parse_pred(&list[1], &ctx)?);
            }
            // `(check-sat)` only: z3 rejects arguments.
            "check-sat" if list.len() == 1 => checked = true,
            // A trailing `exit` after the query changes nothing.
            "exit" if checked => {}
            // Non-constraint commands we can safely ignore. `set-option` (`:encoding` changes string
            // semantics) and `echo` (its text becomes z3's first output line, which callers read as
            // the verdict) are NOT ignorable, and the compiler emits neither: they decline.
            "get-model" | "get-value" | "set-info" => {}
            // An unknown command is out-of-fragment.
            _ => return None,
        }
    }
    // Soundness guard for the string-equality encoding: the BV domain (2^STR_W) must exceed the number
    // of string TERMS so that variables can always be forced pairwise-distinct (else BV could be
    // spuriously UNSAT — a false proof). With STR_W = 32 this bound is astronomically slack; if it is
    // ever exceeded, decline to z3 rather than risk it.
    if strings.borrow().ids.len() + str_var_count > MAX_STR_TERMS {
        return None;
    }
    let sh = shared.into_inner();
    let (mut bv_vars, mut bool_vars, mut asserts) = (bv_vars, bool_vars, asserts);
    bv_vars.extend(sh.bv_vars);
    bool_vars.extend(sh.bool_vars);
    asserts.extend(sh.defs);
    Some(Formula {
        bv_vars,
        bool_vars,
        asserts,
    })
}

enum Sort {
    Bv(u32),
    Fp,
    Bool,
    Str,
}

/// The theory family a `set-logic` admits, as z3 enforces it: a bit-vector logic has no floating-point
/// operators, `QF_FP` and `QF_S` have no bit-vector operators, and `QF_S` has no floating point.
/// (Bit-vector and string LITERALS are accepted under every logic, and so are they here.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Logic {
    Bv,
    Fp,
    Str,
}

/// The SMT-LIB sort of a term, as the query DECLARES it. Lowering erases the difference between a
/// Float64 and a `BitVec 64` (and between a String and a `BitVec 32`), so sort agreement is checked
/// on the source s-expressions, before lowering, by `term_sort` / `pred_well_sorted`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TermSort {
    Bv(u32),
    Fp,
    Str,
}

fn parse_sort(s: &Sexp) -> Option<Sort> {
    match s {
        Sexp::Atom(a) if a == "Bool" => Some(Sort::Bool),
        Sexp::Atom(a) if a == "String" => Some(Sort::Str),
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
                return Some(Sort::Fp);
            }
            None
        }
        // `Float64` is an alias some frontends emit for `(_ FloatingPoint 11 53)`.
        Sexp::Atom(a) if a == "Float64" => Some(Sort::Fp),
        _ => None,
    }
}

struct Ctx<'a> {
    bv_vars: &'a [(String, u32)],
    bool_vars: &'a [String],
    sorts: &'a HashMap<String, TermSort>,
    strings: &'a RefCell<StrInterner>,
    logic: Option<Logic>,
    shared: &'a RefCell<Shared>,
}

impl<'a> Ctx<'a> {
    fn bv_width(&self, name: &str) -> Option<u32> {
        self.bv_vars
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, w)| *w)
    }
    fn is_bool(&self, name: &str) -> bool {
        self.bool_vars.iter().any(|n| n == name)
    }
    /// Bit-vector operators (`bvadd`, `bvult`, `extract`, `concat`, …) exist under this logic.
    fn bv_ops(&self) -> bool {
        matches!(self.logic, None | Some(Logic::Bv))
    }
    /// Floating-point operators and literals (`fp.lt`, `to_fp`, `fp`, `(_ NaN 11 53)`) exist.
    fn fp_ops(&self) -> bool {
        matches!(self.logic, None | Some(Logic::Fp))
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
                "and" => Some(Pred::And(
                    args.iter()
                        .map(|a| parse_pred(a, ctx))
                        .collect::<Option<_>>()?,
                )),
                "or" => Some(Pred::Or(
                    args.iter()
                        .map(|a| parse_pred(a, ctx))
                        .collect::<Option<_>>()?,
                )),
                "=" if args.len() == 2 => {
                    // BV / Float64 / String equality, or Boolean equivalence: chosen up front
                    // (`is_bool_shaped`) so no operand is parsed twice.
                    let boolean = is_bool_shaped(&args[0], ctx) || is_bool_shaped(&args[1], ctx);
                    if let (false, Some(a), Some(b)) = (
                        boolean,
                        (!boolean).then(|| parse_term(&args[0], ctx)).flatten(),
                        (!boolean).then(|| parse_term(&args[1], ctx)).flatten(),
                    ) {
                        if term_sort(&args[0], ctx) == Some(TermSort::Fp) {
                            // SMT-LIB `=` over Float64 is equality of VALUES: every NaN is the same
                            // value (whatever its bit pattern), while +0 and -0 are different values.
                            // So: same bits, or both NaN. Plain bit equality would make two NaNs
                            // with different payloads unequal — a false UNSAT. The operands are
                            // mentioned twice each, so they are shared, not copied.
                            let (a, b) = (share_term(a, ctx)?, share_term(b, ctx)?);
                            Some(Pred::Or(vec![
                                Pred::Eq(a.clone(), b.clone()),
                                Pred::And(vec![fp::is_nan(&a), fp::is_nan(&b)]),
                            ]))
                        } else {
                            Some(Pred::Eq(a, b))
                        }
                    } else {
                        // boolean =: a ↔ b  ≡  (a→b)∧(b→a). Represent via And/Or.
                        let a = share_pred(parse_pred(&args[0], ctx)?, ctx);
                        let b = share_pred(parse_pred(&args[1], ctx)?, ctx);
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
                    &share_term(parse_term(&args[0], ctx)?, ctx)?,
                    &share_term(parse_term(&args[1], ctx)?, ctx)?,
                )),
                "fp.leq" if args.len() == 2 => Some(fp::fp_leq(
                    &share_term(parse_term(&args[0], ctx)?, ctx)?,
                    &share_term(parse_term(&args[1], ctx)?, ctx)?,
                )),
                "fp.gt" if args.len() == 2 => Some(fp::fp_gt(
                    &share_term(parse_term(&args[0], ctx)?, ctx)?,
                    &share_term(parse_term(&args[1], ctx)?, ctx)?,
                )),
                "fp.geq" if args.len() == 2 => Some(fp::fp_geq(
                    &share_term(parse_term(&args[0], ctx)?, ctx)?,
                    &share_term(parse_term(&args[1], ctx)?, ctx)?,
                )),
                "fp.eq" if args.len() == 2 => Some(fp::fp_eq(
                    &share_term(parse_term(&args[0], ctx)?, ctx)?,
                    &share_term(parse_term(&args[1], ctx)?, ctx)?,
                )),
                "fp.isNaN" if args.len() == 1 => {
                    Some(fp::is_nan(&share_term(parse_term(&args[0], ctx)?, ctx)?))
                }
                "fp.isInfinite" if args.len() == 1 => {
                    Some(fp::is_inf(&share_term(parse_term(&args[0], ctx)?, ctx)?))
                }
                "fp.isZero" if args.len() == 1 => {
                    Some(fp::is_zero(&share_term(parse_term(&args[0], ctx)?, ctx)?))
                }
                "ite" if args.len() == 3 => {
                    // boolean ite → (c∧t)∨(¬c∧e); the condition appears twice, so it is shared.
                    let c = share_pred(parse_pred(&args[0], ctx)?, ctx);
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

/// Whether `s` is syntactically Boolean-valued: a Boolean constant or declared Boolean, a predicate
/// application, or an `ite` whose then-branch is (following only the then-branch spine). This decides
/// between the Boolean and the term reading of `=` / `ite` in time proportional to the spine, instead
/// of attempting the term reading, failing deep inside, and re-parsing the subtree as a predicate.
fn is_bool_shaped(s: &Sexp, ctx: &Ctx) -> bool {
    let mut cur = s;
    loop {
        match cur {
            Sexp::Atom(a) => {
                return matches!(a.as_str(), "true" | "false")
                    || (ctx.is_bool(a) && !ctx.sorts.contains_key(a.as_str()))
            }
            Sexp::List(items) => {
                let Some(head) = items.first().and_then(Sexp::as_atom) else {
                    return false;
                };
                match head {
                    "ite" if items.len() == 4 => cur = &items[2],
                    "not" | "and" | "or" | "=" | "bvult" | "bvule" | "bvugt" | "bvuge"
                    | "bvslt" | "bvsle" | "bvsgt" | "bvsge" | "fp.lt" | "fp.leq" | "fp.gt"
                    | "fp.geq" | "fp.eq" | "fp.isNaN" | "fp.isInfinite" | "fp.isZero" => {
                        return true
                    }
                    _ => return false,
                }
            }
        }
    }
}

/// Whether a predicate s-expression is WELL-SORTED over the declared sorts: every operator gets
/// operands of the sort it requires (bit-vector ops: bit-vectors of equal width; `fp.*`: Float64;
/// `=` and `ite`: two operands of the same sort). It mirrors the grammar `parse_pred` accepts and is
/// deliberately strict: anything it does not recognize is NOT well-sorted, so the query declines to
/// z3 (which rejects a genuinely ill-sorted query with an error, and the compiler fails closed on it).
fn pred_well_sorted(s: &Sexp, ctx: &Ctx) -> bool {
    match s {
        Sexp::Atom(a) => {
            matches!(a.as_str(), "true" | "false")
                || (ctx.is_bool(a) && !ctx.sorts.contains_key(a.as_str()))
        }
        Sexp::List(items) => {
            let Some(head) = items.first().and_then(Sexp::as_atom) else {
                return false;
            };
            let args = &items[1..];
            let both = |want: fn(TermSort) -> bool| {
                args.len() == 2
                    && match (term_sort(&args[0], ctx), term_sort(&args[1], ctx)) {
                        (Some(x), Some(y)) => x == y && want(x),
                        _ => false,
                    }
            };
            match head {
                "not" => args.len() == 1 && pred_well_sorted(&args[0], ctx),
                // z3 rejects a zero-argument `(and)` / `(or)` ("arguments missing").
                "and" | "or" => !args.is_empty() && args.iter().all(|a| pred_well_sorted(a, ctx)),
                "=" => {
                    // Choose the reading BEFORE recursing (see `is_bool_shaped`): trying the term
                    // reading first and falling back to the Boolean one re-checked every nested
                    // operand twice per level — exponential time on `(= (ite … p q) r)` chains.
                    args.len() == 2
                        && if is_bool_shaped(&args[0], ctx) || is_bool_shaped(&args[1], ctx) {
                            pred_well_sorted(&args[0], ctx) && pred_well_sorted(&args[1], ctx)
                        } else {
                            match (term_sort(&args[0], ctx), term_sort(&args[1], ctx)) {
                                (Some(x), Some(y)) => x == y,
                                _ => false,
                            }
                        }
                }
                "bvult" | "bvule" | "bvugt" | "bvuge" | "bvslt" | "bvsle" | "bvsgt" | "bvsge" => {
                    ctx.bv_ops() && both(|x| matches!(x, TermSort::Bv(_)))
                }
                "fp.lt" | "fp.leq" | "fp.gt" | "fp.geq" | "fp.eq" => {
                    ctx.fp_ops() && both(|x| x == TermSort::Fp)
                }
                "fp.isNaN" | "fp.isInfinite" | "fp.isZero" => {
                    ctx.fp_ops()
                        && args.len() == 1
                        && term_sort(&args[0], ctx) == Some(TermSort::Fp)
                }
                "ite" => args.len() == 3 && args.iter().all(|a| pred_well_sorted(a, ctx)),
                _ => false,
            }
        }
    }
}

/// The sort of a term s-expression, or `None` if it is ill-sorted or outside the grammar
/// `parse_term` accepts. Bit-vector widths are checked here too (extract bounds, equal operand
/// widths, at most 128 bits), independently of `Term::width`.
fn term_sort(s: &Sexp, ctx: &Ctx) -> Option<TermSort> {
    const MAX_W: u32 = 128;
    let bv = |w: u32| (1..=MAX_W).contains(&w).then_some(TermSort::Bv(w));
    match s {
        Sexp::Atom(a) => {
            if let Some(hex) = a.strip_prefix("#x") {
                (!hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit()))
                    .then(|| bv(u32::try_from(hex.len()).ok()?.checked_mul(4)?))?
            } else if let Some(bin) = a.strip_prefix("#b") {
                (!bin.is_empty() && bin.chars().all(|c| c == '0' || c == '1'))
                    .then(|| bv(u32::try_from(bin.len()).ok()?))?
            } else if a.starts_with('"') {
                Some(TermSort::Str)
            } else {
                ctx.sorts.get(a.as_str()).copied()
            }
        }
        Sexp::List(items) => {
            let args = &items[1..];
            if let Some(Sexp::List(op)) = items.first() {
                if op.first()?.as_atom()? != "_" {
                    return None;
                }
                let num = |i: usize| -> Option<u32> { op.get(i)?.as_atom()?.parse().ok() };
                let indexed = op.get(1)?.as_atom()?;
                // `to_fp` is a floating-point operator; `extract` / `*_extend` are bit-vector ones.
                let theory_ok = if indexed == "to_fp" {
                    ctx.fp_ops()
                } else {
                    ctx.bv_ops()
                };
                if !theory_ok {
                    return None;
                }
                return match indexed {
                    "extract" if op.len() == 4 && args.len() == 1 => {
                        let (hi, lo) = (num(2)?, num(3)?);
                        match term_sort(&args[0], ctx)? {
                            TermSort::Bv(w) if lo <= hi && hi < w => bv(hi - lo + 1),
                            _ => None,
                        }
                    }
                    "zero_extend" | "sign_extend" if op.len() == 3 && args.len() == 1 => {
                        match term_sort(&args[0], ctx)? {
                            TermSort::Bv(w) => bv(w.checked_add(num(2)?)?),
                            _ => None,
                        }
                    }
                    // ((_ to_fp 11 53) RNE <decimal>): the only rounding mode accepted is
                    // round-nearest-ties-to-even, because the literal is converted with Rust's f64
                    // parse, which rounds that way. Any other mode would round an inexact decimal
                    // differently. A DECLARED name in the rounding-mode position is not a rounding
                    // mode at all.
                    "to_fp"
                        if op.len() == 4 && num(2)? == 11 && num(3)? == 53 && args.len() == 2 =>
                    {
                        let rm = args[0].as_atom()?;
                        let rm_ok = matches!(rm, "RNE" | "roundNearestTiesToEven")
                            && !ctx.sorts.contains_key(rm)
                            && !ctx.is_bool(rm);
                        let lit_ok = match &args[1] {
                            Sexp::Atom(d) => is_smt_decimal(d),
                            Sexp::List(neg) => {
                                neg.len() == 2
                                    && neg[0].as_atom() == Some("-")
                                    && neg[1].as_atom().is_some_and(is_smt_decimal)
                            }
                        };
                        (rm_ok && lit_ok).then_some(TermSort::Fp)
                    }
                    _ => None,
                };
            }
            let head = items.first()?.as_atom()?;
            match head {
                "_" => {
                    let lit = args.first()?.as_atom()?;
                    if let Some(val) = lit.strip_prefix("bv") {
                        if args.len() != 2
                            || val.is_empty()
                            || !val.bytes().all(|b| b.is_ascii_digit())
                        {
                            return None;
                        }
                        bv(args[1].as_atom()?.parse().ok()?)
                    } else if ctx.fp_ops()
                        && args.len() == 3
                        && args[1].as_atom()? == "11"
                        && args[2].as_atom()? == "53"
                        && matches!(lit, "+oo" | "-oo" | "+zero" | "-zero" | "NaN")
                    {
                        Some(TermSort::Fp)
                    } else {
                        None
                    }
                }
                // (fp sign exponent significand): bit-vector literals of widths 1, 11 and 52.
                "fp" if ctx.fp_ops() && args.len() == 3 => {
                    let widths = [1, 11, 52];
                    args.iter()
                        .zip(widths)
                        .all(|(a, w)| {
                            a.as_atom().is_some()
                                && term_sort(a, ctx) == Some(TermSort::Bv(w))
                                && !ctx.sorts.contains_key(a.as_atom().unwrap_or(""))
                        })
                        .then_some(TermSort::Fp)
                }
                "fp.neg" | "fp.abs" if ctx.fp_ops() && args.len() == 1 => {
                    (term_sort(&args[0], ctx)? == TermSort::Fp).then_some(TermSort::Fp)
                }
                "bvadd" | "bvsub" | "bvmul" | "bvand" | "bvor" | "bvxor" | "bvshl" | "bvlshr"
                | "bvashr" | "bvudiv" | "bvurem" | "bvsdiv" | "bvsrem"
                    if ctx.bv_ops() && args.len() == 2 =>
                {
                    match (term_sort(&args[0], ctx)?, term_sort(&args[1], ctx)?) {
                        // The variable-shift blaster reads at most 64 shift-amount bits, so a wider
                        // shift (never emitted: the compiler's shifts are 64-bit) declines.
                        (TermSort::Bv(a), TermSort::Bv(b))
                            if a == b
                                && !(matches!(head, "bvshl" | "bvlshr" | "bvashr") && a > 64) =>
                        {
                            bv(a)
                        }
                        _ => None,
                    }
                }
                "bvneg" | "bvnot" if ctx.bv_ops() && args.len() == 1 => {
                    match term_sort(&args[0], ctx)? {
                        TermSort::Bv(w) => bv(w),
                        _ => None,
                    }
                }
                "concat" if ctx.bv_ops() && args.len() == 2 => {
                    match (term_sort(&args[0], ctx)?, term_sort(&args[1], ctx)?) {
                        (TermSort::Bv(a), TermSort::Bv(b)) => bv(a.checked_add(b)?),
                        _ => None,
                    }
                }
                "ite" if args.len() == 3 => {
                    // A Boolean-valued `ite` is not a term: say so before checking the condition,
                    // which the caller's Boolean reading will check once.
                    if is_bool_shaped(&args[1], ctx) || !pred_well_sorted(&args[0], ctx) {
                        return None;
                    }
                    let (t, e) = (term_sort(&args[1], ctx)?, term_sort(&args[2], ctx)?);
                    (t == e).then_some(t)
                }
                _ => None,
            }
        }
    }
}

/// An SMT-LIB numeral or decimal (`7`, `2.5`): digits with at most one interior `.`. Rust's f64
/// parse also accepts `inf`, `1e5`, `+3` and similar spellings that are not SMT-LIB literals.
fn is_smt_decimal(s: &str) -> bool {
    let mut parts = s.splitn(2, '.');
    let int = parts.next().unwrap_or("");
    let frac = parts.next();
    !int.is_empty()
        && int.bytes().all(|b| b.is_ascii_digit())
        && frac.is_none_or(|f| !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit()))
}

fn bin_pred(args: &[Sexp], ctx: &Ctx, f: fn(Term, Term) -> Pred) -> Option<Pred> {
    Some(f(parse_term(&args[0], ctx)?, parse_term(&args[1], ctx)?))
}

fn parse_term(s: &Sexp, ctx: &Ctx) -> Option<Term> {
    match s {
        Sexp::Atom(a) => {
            // #xHEX / #bBIN literals, a string literal, or a declared BV/String var.
            if let Some(hex) = a.strip_prefix("#x") {
                let v = u128::from_str_radix(hex, 16).ok()?;
                Some(Term::Const(v, (hex.len() as u32) * 4))
            } else if let Some(bin) = a.strip_prefix("#b") {
                let v = u128::from_str_radix(bin, 2).ok()?;
                Some(Term::Const(v, bin.len() as u32))
            } else if a.starts_with('"') {
                // A string literal: intern its decoded content to a distinct id (see STR_W).
                let decoded = unquote_and_decode(a)?;
                let id = ctx.strings.borrow_mut().intern(decoded);
                Some(Term::Const(id, STR_W))
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
                        Some(Term::Extract(
                            hi,
                            lo,
                            Box::new(parse_term(args.first()?, ctx)?),
                        ))
                    }
                    "zero_extend" => {
                        let n: u32 = op.get(2)?.as_atom()?.parse().ok()?;
                        Some(Term::ZeroExtend(
                            n,
                            Box::new(parse_term(args.first()?, ctx)?),
                        ))
                    }
                    "sign_extend" => {
                        let n: u32 = op.get(2)?.as_atom()?.parse().ok()?;
                        Some(Term::SignExtend(
                            n,
                            Box::new(parse_term(args.first()?, ctx)?),
                        ))
                    }
                    // ((_ to_fp 11 53) <rounding> <decimal>) — a Float64 literal. The rounding mode is
                    // irrelevant for a decimal (Rust's f64 parse is round-to-nearest-even, matching
                    // RNE). Only Float64 and a plain (or negated) decimal literal are accepted.
                    "to_fp" if op.get(2)?.as_atom()? == "11" && op.get(3)?.as_atom()? == "53" => {
                        let lit = args.get(1)?;
                        let bits = match lit {
                            Sexp::Atom(a) => fp::decimal_to_bits(a)?,
                            // (- 4.0) — a negated decimal literal.
                            Sexp::List(neg) if neg.len() == 2 && neg[0].as_atom()? == "-" => {
                                let d = neg[1].as_atom()?;
                                let m = fp::decimal_to_bits(d)?;
                                // The operand is a REAL: `(- 0.0)` is the real zero, which `to_fp`
                                // maps to +0. A non-zero real that ROUNDS to zero (`(- 0.000…1)`)
                                // still rounds to -0, so the test is on the decimal's digits, not on
                                // the rounded magnitude.
                                if d.bytes().all(|b| b == b'0' || b == b'.') {
                                    0
                                } else {
                                    m ^ 0x8000_0000_0000_0000
                                }
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
                        // SMT-LIB `(_ bvN w)` denotes N mod 2^w (z3 agrees). Keeping N unreduced let a
                        // constant shift by `(_ bv256 8)` (which is 0) shift every bit out.
                        let v = if w >= 128 { v } else { v & ((1u128 << w) - 1) };
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
            Some('(') => out.push(read_sexp(&mut chars, 0)?),
            Some(_) => out.push(Sexp::Atom(read_atom(&mut chars)?)),
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

/// Deepest s-expression nesting accepted. Parsing, sort checking, lowering, bit-blasting and model
/// evaluation each recurse once per level; deeper input declines (z3 decides) instead of exhausting
/// the stack. Every entry point runs on the solver's own `SOLVER_STACK_BYTES` thread
/// (`crate::on_solver_stack`), so this bound does not depend on the caller's stack. The compiler's
/// ordinary queries nest at most about 15 deep (a ten-flag sum in the nexus example), but a long left-associated sum nests one level per term, and
/// 1024 keeps such contracts machine-certified.
pub const MAX_SEXP_DEPTH: usize = 1024;

fn read_sexp(chars: &mut std::iter::Peekable<std::str::Chars>, depth: usize) -> Option<Sexp> {
    if depth >= MAX_SEXP_DEPTH {
        return None;
    }
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
            Some('(') => items.push(read_sexp(chars, depth + 1)?),
            Some(_) => items.push(Sexp::Atom(read_atom(chars)?)),
        }
    }
}

/// One atom, or `None` (decline) for input this reader cannot represent faithfully.
fn read_atom(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<String> {
    let mut s = String::new();
    // A |quoted symbol| is declined. Stripping the bars made `|#x01|` read as the bit-vector LITERAL
    // `#x01` and `|"a"|` as a string literal — a symbol typed as whatever it resembles, which z3
    // rejects. The compiler never emits one.
    if chars.peek() == Some(&'|') {
        return None;
    }
    // "string literal" — a doubled quote `""` is an escaped quote (SMT-LIB), NOT the terminator, so it
    // is kept in the atom text (verbatim, both quotes) and collapsed later by `unquote_and_decode`.
    if chars.peek() == Some(&'"') {
        s.push('"');
        chars.next();
        loop {
            match chars.next() {
                None => return None, // unterminated string literal
                Some('"') => {
                    if chars.peek() == Some(&'"') {
                        s.push('"');
                        s.push('"');
                        chars.next();
                    } else {
                        s.push('"'); // closing quote
                        break;
                    }
                }
                Some(c) => s.push(c),
            }
        }
        return Some(s);
    }
    while let Some(&c) = chars.peek() {
        // `;`, `|` and `"` are not symbol characters: in z3 they start a comment, a quoted symbol
        // and a string literal even directly after an atom, so they end it here too.
        if c.is_whitespace() || c == '(' || c == ')' || c == ';' || c == '|' || c == '"' {
            break;
        }
        s.push(c);
        chars.next();
    }
    // Empty means the next character is an unmatched `)`: nothing was consumed, so returning an
    // empty atom would make the caller loop forever.
    (!s.is_empty()).then_some(s)
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
    fn string_equality_parses() {
        // Pure string equality is in-fragment (lowered to bit-vectors by interning).
        let f = parse_smt2(
            "(set-logic QF_S)\n(declare-const s String)\n(assert (= s \"a\"))\n(check-sat)\n",
        )
        .unwrap();
        assert_eq!(f.bv_vars, vec![("s".to_string(), STR_W)]);
        assert_eq!(f.asserts.len(), 1);
    }

    #[test]
    fn declines_string_operations() {
        // A non-equality string OPERATION (str.++/contains/len/…) is out of fragment → decline.
        assert!(parse_smt2(
            "(set-logic QF_S)\n(declare-const s String)\n(assert (= (str.++ s \"x\") \"ax\"))\n(check-sat)\n"
        )
        .is_none());
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
