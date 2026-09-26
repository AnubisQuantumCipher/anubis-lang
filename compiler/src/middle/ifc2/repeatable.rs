//! A deliberately small, callback-relative proof of repeatable comparator keys.
//!
//! Capture atoms quantify over each fixed runtime closure snapshot, including every snapshot
//! represented by a joined IFC environment. They are not identities for IFC abstract values.
//! The admitted fragment has a single control path: reads, exact literal construction, ordered
//! arrays, tagged total unary/eager-comparison terms, lexical blocks, bindings, plain rebinding,
//! isolated eligible direct helpers, returns and statement-form output. Every evaluated
//! unsupported transfer abandons the whole proof, even when its result is discarded.
//! Ordinary IFC evaluation must still check both comparator calls and their effects.

use crate::frontend::{Expr, Stmt};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

// Precision budgets, not assumptions about runtime execution or program termination.
const MAX_DEPTH: usize = 128;
const MAX_TERMS: usize = 4096;
const MAX_WORK: usize = 16384;
const MAX_CALL_DEPTH: usize = 32;

/// The work allowance is shared by every fixed callable alternative in a key query.
/// Exhaustion only disables the repeatability shortcut.
pub(super) struct SummaryBudget {
    remaining: usize,
}

impl Default for SummaryBudget {
    fn default() -> Self {
        Self {
            remaining: MAX_WORK,
        }
    }
}

/// Immutable free-function view from the interpreter's existing resolved registry. A known but
/// ineligible function must still win over a same-named local or builtin during resolution.
pub(super) struct FreeFnView<'a> {
    pub id: Rc<str>,
    pub params: &'a [(String, String)],
    pub body: &'a [Stmt],
    pub locals: Rc<BTreeSet<String>>,
    pub eligible: bool,
}

#[derive(Clone, Copy, Debug)]
struct Unsupported;

type Analysis<T> = Result<T, Unsupported>;

/// Interned construction terms are local to one summary. Ordered child IDs preserve exact
/// construction; no arithmetic, abstract-value equality or comparator transitivity is used.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Term<'a> {
    Capture(usize),
    Literal(&'a str),
    String(&'a str),
    Array(Vec<usize>),
    Unary(&'a str, usize),
    Compare(&'a str, usize, usize),
    DefaultZero,
}

#[derive(Clone, Copy)]
struct Stable {
    id: usize,
    depth: usize,
}

#[derive(Clone, Copy)]
enum Value {
    Stable(Stable),
    Varying,
}

enum Outcome {
    Normal(Value),
    Return(Value),
}

enum Control {
    Continue,
    Return(Value),
}

struct Summary<'a, 'f> {
    scopes: Vec<BTreeMap<&'a str, Value>>,
    terms: BTreeMap<Term<'a>, Stable>,
    remaining: usize,
    max_depth: usize,
    max_terms: usize,
    owner_locals: Rc<BTreeSet<String>>,
    free_functions: &'f dyn Fn(&str) -> Option<FreeFnView<'a>>,
    active_helpers: BTreeSet<Rc<str>>,
    max_call_depth: usize,
}

/// True means identical key construction on normally returning calls of each fixed closure
/// instance, independent of its arguments and of fresh external observations. It makes no
/// termination or IFC-egress claim. Callers must check every possible fixed callable separately.
///
/// Expression-position calls resolve the complete free-function registry, then the current
/// frame's local names, then builtins. Statement-position output and return have special
/// precedence regardless of those names.
pub(super) fn key_is_repeatable<'a>(
    params: &'a [String],
    body: &'a Expr,
    captures: &'a [String],
    owner_locals: Rc<BTreeSet<String>>,
    free_functions: &dyn Fn(&str) -> Option<FreeFnView<'a>>,
    budget: &mut SummaryBudget,
) -> bool {
    let mut summary = Summary {
        scopes: vec![BTreeMap::new()],
        terms: BTreeMap::new(),
        remaining: budget.remaining,
        max_depth: MAX_DEPTH,
        max_terms: MAX_TERMS,
        owner_locals,
        free_functions,
        active_helpers: BTreeSet::new(),
        max_call_depth: MAX_CALL_DEPTH,
    };
    let result = (|| {
        for (slot, name) in captures.iter().enumerate() {
            let value = summary.intern(Term::Capture(slot), 0)?;
            summary.bind(name, Value::Stable(value))?;
        }
        // min_by/max_by pass exactly one comparator argument. The native lambda wrapper binds
        // later formals from absent slots to Int(0), after re-cloning captures; a formal can
        // shadow a capture. Treating those padded values as varying rejects valid fixed keys.
        for (index, name) in params.iter().enumerate() {
            let value = if index == 0 {
                Value::Varying
            } else {
                Value::Stable(summary.intern(Term::DefaultZero, 0)?)
            };
            summary.bind(name, value)?;
        }
        summary.expr(body, 0)
    })();
    budget.remaining = summary.remaining;
    matches!(
        result,
        Ok(Outcome::Normal(Value::Stable(_)) | Outcome::Return(Value::Stable(_)))
    )
}

impl<'a> Summary<'a, '_> {
    fn spend(&mut self, amount: usize) -> Analysis<()> {
        self.remaining = self.remaining.checked_sub(amount).ok_or(Unsupported)?;
        Ok(())
    }

    fn visit(&mut self, depth: usize) -> Analysis<()> {
        self.spend(1)?;
        if depth > self.max_depth {
            return Err(Unsupported);
        }
        Ok(())
    }

    fn intern(&mut self, term: Term<'a>, depth: usize) -> Analysis<Stable> {
        self.spend(1)?;
        if depth > self.max_depth {
            return Err(Unsupported);
        }
        if let Some(value) = self.terms.get(&term) {
            return Ok(*value);
        }
        if self.terms.len() >= self.max_terms {
            return Err(Unsupported);
        }
        let value = Stable {
            id: self.terms.len(),
            depth,
        };
        self.terms.insert(term, value);
        Ok(value)
    }

    fn zero(&mut self) -> Analysis<Outcome> {
        Ok(Outcome::Normal(Value::Stable(
            self.intern(Term::DefaultZero, 0)?,
        )))
    }

    fn bind(&mut self, name: &'a str, value: Value) -> Analysis<()> {
        self.spend(1)?;
        self.spend(name.len())?;
        self.scopes
            .last_mut()
            .ok_or(Unsupported)?
            .insert(name, value);
        Ok(())
    }

    fn lookup(&mut self, name: &str) -> Analysis<Option<Value>> {
        self.spend(name.len())?;
        for index in (0..self.scopes.len()).rev() {
            self.spend(1)?;
            if let Some(value) = self.scopes[index].get(name) {
                return Ok(Some(*value));
            }
        }
        Ok(None)
    }

    fn assign(&mut self, name: &str, value: Value) -> Analysis<()> {
        self.spend(name.len())?;
        for index in (0..self.scopes.len()).rev() {
            self.spend(1)?;
            if let Some(binding) = self.scopes[index].get_mut(name) {
                *binding = value;
                return Ok(());
            }
        }
        Err(Unsupported)
    }

    fn returned(&mut self, args: &'a [Expr], depth: usize) -> Analysis<Outcome> {
        // Both runtime return lowerings evaluate only the first argument. Missing means Int(0).
        let outcome = match args.first() {
            Some(value) => self.expr(value, depth)?,
            None => self.zero()?,
        };
        let (Outcome::Normal(value) | Outcome::Return(value)) = outcome;
        Ok(Outcome::Return(value))
    }

    fn helper_call(
        &mut self,
        callee: FreeFnView<'a>,
        args: &'a [Expr],
        depth: usize,
    ) -> Analysis<Outcome> {
        // Direct, nongeneric boxed calls have a fixed Rust signature. Do not apply the IFC
        // interpreter's padding/truncation convention for first-class closure wrappers here.
        if !callee.eligible
            || callee.params.len() != args.len()
            || self.active_helpers.len() >= self.max_call_depth
            || self.active_helpers.contains(&callee.id)
        {
            return Err(Unsupported);
        }
        self.spend(callee.id.len())?;
        self.spend(args.len())?;
        let mut actuals = Vec::with_capacity(args.len());
        for arg in args {
            match self.expr(arg, depth + 1)? {
                Outcome::Normal(value) => actuals.push(value),
                returned @ Outcome::Return(_) => return Ok(returned),
            }
        }

        // A helper has no implicit access to the callback's captures or caller's locals. Its
        // formal values are the symbolic results of already evaluated actual arguments.
        self.active_helpers.insert(callee.id.clone());
        let caller_scopes = std::mem::replace(&mut self.scopes, vec![BTreeMap::new()]);
        let caller_locals = std::mem::replace(&mut self.owner_locals, callee.locals);
        let result = (|| {
            for ((name, _), value) in callee.params.iter().zip(actuals) {
                self.bind(name, value)?;
            }
            self.block(callee.body, None, depth + 1)
        })();
        self.scopes = caller_scopes;
        self.owner_locals = caller_locals;
        self.active_helpers.remove(&callee.id);
        // An explicit return in the helper is a normal value in its caller. A return while
        // evaluating an actual above remains a return from the callback itself.
        let (Outcome::Normal(value) | Outcome::Return(value)) = result?;
        Ok(Outcome::Normal(value))
    }

    fn expr(&mut self, expr: &'a Expr, depth: usize) -> Analysis<Outcome> {
        self.visit(depth)?;
        match expr {
            Expr::Var(name) => Ok(Outcome::Normal(self.lookup(name)?.ok_or(Unsupported)?)),
            Expr::Literal(text) => {
                self.spend(text.len())?;
                Ok(Outcome::Normal(Value::Stable(
                    self.intern(Term::Literal(text), 0)?,
                )))
            }
            Expr::StrLiteral(text) => {
                self.spend(text.len())?;
                Ok(Outcome::Normal(Value::Stable(
                    self.intern(Term::String(text), 0)?,
                )))
            }
            Expr::ArrayLiteral { elements } => {
                let mut children = Vec::new();
                let mut term_depth = 0;
                let mut varying = false;
                for element in elements {
                    match self.expr(element, depth + 1)? {
                        Outcome::Normal(Value::Stable(term)) => {
                            children.push(term.id);
                            term_depth = term_depth.max(term.depth);
                        }
                        Outcome::Normal(Value::Varying) => varying = true,
                        returned @ Outcome::Return(_) => return Ok(returned),
                    }
                }
                if varying {
                    Ok(Outcome::Normal(Value::Varying))
                } else {
                    Ok(Outcome::Normal(Value::Stable(
                        self.intern(Term::Array(children), term_depth + 1)?,
                    )))
                }
            }
            Expr::Unary { op, expr } if matches!(op.as_str(), "-" | "!" | "~") => {
                match self.expr(expr, depth + 1)? {
                    Outcome::Normal(Value::Stable(term)) => Ok(Outcome::Normal(Value::Stable(
                        self.intern(Term::Unary(op, term.id), term.depth + 1)?,
                    ))),
                    Outcome::Normal(Value::Varying) => Ok(Outcome::Normal(Value::Varying)),
                    returned @ Outcome::Return(_) => Ok(returned),
                }
            }
            Expr::Binary { op, lhs, rhs }
                if matches!(op.as_str(), "<" | "<=" | ">" | ">=" | "==" | "!=") =>
            {
                let left = match self.expr(lhs, depth + 1)? {
                    Outcome::Normal(value) => value,
                    returned @ Outcome::Return(_) => return Ok(returned),
                };
                let right = match self.expr(rhs, depth + 1)? {
                    Outcome::Normal(value) => value,
                    returned @ Outcome::Return(_) => return Ok(returned),
                };
                match (left, right) {
                    (Value::Stable(left), Value::Stable(right)) => {
                        Ok(Outcome::Normal(Value::Stable(self.intern(
                            Term::Compare(op, left.id, right.id),
                            left.depth.max(right.depth) + 1,
                        )?)))
                    }
                    _ => Ok(Outcome::Normal(Value::Varying)),
                }
            }
            Expr::Block { stmts, tail } => {
                self.scopes.push(BTreeMap::new());
                let result = self.block(stmts, tail.as_deref(), depth + 1);
                self.scopes.pop();
                result
            }
            Expr::Call { callee, args } => {
                self.spend(callee.len())?;
                // The complete free-function namespace wins, even if a definition is ineligible.
                // A local callable wins over builtins and remains outside this summary.
                if let Some(function) = (self.free_functions)(callee) {
                    self.helper_call(function, args, depth + 1)
                } else if self.owner_locals.contains(callee) {
                    Err(Unsupported)
                } else if callee == "return" {
                    self.returned(args, depth + 1)
                } else {
                    // A call may read fresh state, mutate bindings, or return early. Discarding its
                    // result cannot preserve the old store. No spelling alone grants determinism.
                    Err(Unsupported)
                }
            }
            Expr::CallExpr { .. }
            | Expr::Binary { .. }
            | Expr::Unary { .. }
            | Expr::Index { .. }
            | Expr::Cast { .. }
            | Expr::Tainted { .. }
            | Expr::Symbolic { .. }
            | Expr::Assume(_)
            | Expr::Assert(_)
            | Expr::Declassify { .. }
            | Expr::TaintSource { .. }
            | Expr::UnifiedBuffer { .. }
            | Expr::RawPtr { .. }
            | Expr::StructLiteral { .. }
            | Expr::FieldAccess { .. }
            | Expr::EnumConstruct { .. }
            | Expr::Match { .. }
            | Expr::If { .. }
            | Expr::MapLiteral { .. }
            | Expr::Lambda { .. }
            | Expr::Try(_)
            | Expr::IfLet { .. }
            | Expr::Other(_) => Err(Unsupported),
        }
    }

    fn stmt(&mut self, stmt: &'a Stmt, depth: usize) -> Analysis<Control> {
        self.visit(depth)?;
        match stmt {
            Stmt::Let { name, init, .. } => match self.expr(init, depth + 1)? {
                Outcome::Normal(value) => {
                    self.bind(name, value)?;
                    Ok(Control::Continue)
                }
                Outcome::Return(value) => Ok(Control::Return(value)),
            },
            Stmt::Assign { target, value } => {
                let Expr::Var(name) = target else {
                    return Err(Unsupported);
                };
                match self.expr(value, depth + 1)? {
                    Outcome::Normal(value) => {
                        self.assign(name, value)?;
                        Ok(Control::Continue)
                    }
                    Outcome::Return(value) => Ok(Control::Return(value)),
                }
            }
            Stmt::ExprStmt(expr) => {
                if let Expr::Call { callee, args } = expr {
                    // These statement lowerings precede user-function and local-name lookup.
                    if callee == "push" {
                        // The statement lowering mutates its first argument directly. Even a
                        // same-named free function does not run in this position.
                        return Err(Unsupported);
                    }
                    if matches!(callee.as_str(), "print" | "println" | "eprint" | "eprintln") {
                        for arg in args {
                            if let Outcome::Return(value) = self.expr(arg, depth + 1)? {
                                return Ok(Control::Return(value));
                            }
                        }
                        return Ok(Control::Continue);
                    }
                    if callee == "return" {
                        let Outcome::Return(value) = self.returned(args, depth + 1)? else {
                            return Err(Unsupported);
                        };
                        return Ok(Control::Return(value));
                    }
                }
                match self.expr(expr, depth + 1)? {
                    Outcome::Normal(_) => Ok(Control::Continue),
                    Outcome::Return(value) => Ok(Control::Return(value)),
                }
            }
            Stmt::LetPattern { .. }
            | Stmt::WhileLet { .. }
            | Stmt::If { .. }
            | Stmt::While { .. }
            | Stmt::Loop { .. }
            | Stmt::For { .. }
            | Stmt::Break
            | Stmt::Continue
            | Stmt::ResearchBlock { .. }
            | Stmt::ExploitBlock { .. }
            | Stmt::HybridBlock { .. }
            | Stmt::SpecBlock { .. } => Err(Unsupported),
        }
    }

    fn block(
        &mut self,
        stmts: &'a [Stmt],
        tail: Option<&'a Expr>,
        depth: usize,
    ) -> Analysis<Outcome> {
        self.visit(depth)?;
        for (index, stmt) in stmts.iter().enumerate() {
            if tail.is_none() && index == stmts.len() - 1 {
                // Runtime split_tail_expr keeps return/output statements in the head. Other
                // trailing expression statements become values even with a source semicolon.
                if let Stmt::ExprStmt(expr) = stmt {
                    let statement_only = matches!(
                        expr,
                        Expr::Call { callee, .. }
                            if matches!(callee.as_str(),
                                "return" | "print" | "println" | "eprint" | "eprintln")
                    );
                    if !statement_only {
                        return self.expr(expr, depth + 1);
                    }
                }
                // A trailing if/else also becomes a value at runtime. All branches are outside
                // this summary's fragment and stmt() refuses it, so none can turn into zero here.
            }
            if let Control::Return(value) = self.stmt(stmt, depth + 1)? {
                return Ok(Outcome::Return(value));
            }
        }
        match tail {
            Some(expr) => self.expr(expr, depth + 1),
            None => self.zero(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::{parse_source, Item, Span};

    fn check(body: &str) -> bool {
        check_with_params("x", body)
    }

    fn check_with_params(formals: &str, body: &str) -> bool {
        let source = format!("fn main() {{ let f = |{formals}| {{ {body} }}; }}");
        let ast = parse_source(&source).expect("summary fixture must parse");
        let Item::Fn { body, .. } = &ast.items[0] else {
            panic!("expected function");
        };
        let Stmt::Let {
            init: Expr::Lambda { params, body },
            ..
        } = &body[0]
        else {
            panic!("expected lambda binding");
        };
        key_is_repeatable(
            params,
            body,
            &["s".into(), "t".into()],
            Rc::new(BTreeSet::new()),
            &|_| None,
            &mut SummaryBudget::default(),
        )
    }

    fn var(name: &str) -> Expr {
        Expr::Var(name.into())
    }

    fn call(name: &str, args: Vec<Expr>) -> Expr {
        Expr::Call {
            callee: name.into(),
            args,
        }
    }

    fn block(stmts: Vec<Stmt>, tail: Expr) -> Expr {
        Expr::Block {
            stmts,
            tail: Some(Box::new(tail)),
        }
    }

    fn check_ast(body: &Expr) -> bool {
        key_is_repeatable(
            &["x".into()],
            body,
            &["s".into()],
            Rc::new(BTreeSet::new()),
            &|_| None,
            &mut SummaryBudget::default(),
        )
    }

    fn check_ast_with_helper<'a>(
        body: &'a Expr,
        helper_name: &str,
        params: &'a [(String, String)],
        helper_body: &'a [Stmt],
    ) -> bool {
        let resolve = |name: &str| {
            (name == helper_name).then(|| FreeFnView {
                id: Rc::from(helper_name),
                params,
                body: helper_body,
                locals: Rc::new(crate::backends::run::collect_local_names(
                    params,
                    helper_body,
                )),
                eligible: true,
            })
        };
        key_is_repeatable(
            &["x".into()],
            body,
            &["s".into()],
            Rc::new(BTreeSet::new()),
            &resolve,
            &mut SummaryBudget::default(),
        )
    }

    fn check_with_helpers(body: &str, helpers: &str) -> bool {
        let source = format!("{helpers} fn main() {{ let f = |x| {{ {body} }}; }}");
        let ast = parse_source(&source).expect("helper summary fixture must parse");
        let main = ast
            .items
            .iter()
            .find(|item| matches!(item, Item::Fn { name, .. } if name == "main"))
            .expect("main");
        let Item::Fn {
            body: main_body, ..
        } = main
        else {
            unreachable!();
        };
        let Stmt::Let {
            init: Expr::Lambda { params, body },
            ..
        } = &main_body[0]
        else {
            panic!("expected lambda binding");
        };
        let resolve = |query: &str| {
            ast.items.iter().find_map(|item| {
                let Item::Fn {
                    name,
                    params,
                    body,
                    ret,
                    mode,
                    generics,
                    generic_bounds,
                    attributes,
                    ..
                } = item
                else {
                    return None;
                };
                (name == query).then(|| FreeFnView {
                    id: Rc::from(name.as_str()),
                    params,
                    body,
                    locals: Rc::new(crate::backends::run::collect_local_names(params, body)),
                    eligible: *mode == crate::frontend::Mode::Safe
                        && generics.is_empty()
                        && generic_bounds.is_empty()
                        && attributes.is_empty()
                        && ret.is_none()
                        && params.iter().all(|(_, ty)| ty.is_empty()),
                })
            })
        };
        key_is_repeatable(
            params,
            body,
            &["s".into()],
            Rc::new(BTreeSet::new()),
            &resolve,
            &mut SummaryBudget::default(),
        )
    }

    #[test]
    fn capture_literals_and_ordered_constructors_are_repeatable() {
        for body in [
            "s",
            "[s]",
            "[[s], [t, s]]",
            "42",
            "true",
            "\"value\"",
            "let key = [s]; println(x); key",
            "let key = x; key = s; key",
            "s = t; s",
            "let x = s; x",
            "let y = x; s",
            "println(x); s",
            "eprintln(x, x); [s]",
            "return s; x",
            "return [s]; random()",
        ] {
            assert!(check(body), "expected repeatable: {body}");
        }
    }

    #[test]
    fn exact_unary_and_eager_comparison_terms_preserve_fixed_keys() {
        for body in [
            "-s",
            "!s",
            "~s",
            "s < 0",
            "s <= 0",
            "s > 0",
            "s >= 0",
            "s == 0",
            "s != 0",
            "[s > 0, [-s, !s]]",
        ] {
            assert!(check(body), "expected repeatable: {body}");
        }
        for body in ["-x", "!x", "~x", "x > 0", "s == x", "x == x"] {
            assert!(!check(body), "expected varying key: {body}");
        }
        for body in ["s / 1", "s % 1", "s && true", "s || false", "s + 0"] {
            assert!(!check(body), "operator remains outside fragment: {body}");
        }
    }

    #[test]
    fn comparator_padding_is_exact_zero_and_only_the_supplied_arg_varies() {
        for body in ["y", "[s, y]", "[s, y, z]", "-y", "y == 0"] {
            assert!(
                check_with_params("x, y, z", body),
                "padded formals are fixed: {body}"
            );
        }
        for body in ["x", "[s, x]", "x == y", "-x"] {
            assert!(
                !check_with_params("x, y", body),
                "supplied argument remains varying: {body}"
            );
        }
    }

    #[test]
    fn helpers_substitute_arguments_in_isolated_frames() {
        assert!(check_with_helpers("fixed(s)", "fn fixed(v) { v }"));
        assert!(check_with_helpers(
            "forward(s)",
            "fn fixed(v) { v } fn forward(v) { fixed(v) }",
        ));
        assert!(!check_with_helpers("fixed(x)", "fn fixed(v) { v }"));
        assert!(check_with_helpers(
            "choose(x, s)",
            "fn choose(ignored, fixed) { fixed }",
        ));
        assert!(check_with_helpers(
            "log_key(s)",
            "fn log_key(v) { println(v); return(v); random() }",
        ));
        assert!(check_with_helpers(
            "local(x); s",
            "fn local(v) { let s = v; s = v; s }",
        ));
        assert!(!check_with_helpers("typed(s)", "fn typed(v: u32) { v }"));
        assert!(!check_with_helpers("typed(s)", "fn typed(v) -> u32 { v }"));
        assert!(!check_with_helpers("fresh(s)", "fn fresh(v) { now(); v }"));
        assert!(!check_with_helpers("trap(s)", "fn trap(v) { v / 0; v }"));
        assert!(!check_with_helpers("fixed()", "fn fixed(v) { v }"));
        assert!(!check_with_helpers("fixed(s, x)", "fn fixed(v) { v }"));
        assert!(!check_with_helpers("cycle(s)", "fn cycle(v) { cycle(v) }"));
        assert!(!check_with_helpers(
            "first(s)",
            "fn first(v) { second(v) } fn second(v) { first(v) }",
        ));
        assert!(check_with_helpers(
            "let fixed = x; fixed(s)",
            "fn fixed(v) { v }",
        )); // A free function wins over the complete local-name set.
    }

    #[test]
    fn eager_operands_and_helper_actuals_thread_store_and_return() {
        let mutate_s = block(
            vec![Stmt::Assign {
                target: var("s"),
                value: var("x"),
            }],
            Expr::Literal("0".into()),
        );
        let compare = Expr::Binary {
            op: ">".into(),
            lhs: Box::new(mutate_s),
            rhs: Box::new(var("s")),
        };
        assert!(!check_ast(&compare));

        let early_left = Expr::Binary {
            op: ">".into(),
            lhs: Box::new(call("return", vec![var("s")])),
            rhs: Box::new(call("random", vec![])),
        };
        assert!(check_ast(&early_left));
        let early_right = Expr::Binary {
            op: ">".into(),
            lhs: Box::new(Expr::Literal("0".into())),
            rhs: Box::new(call("return", vec![var("s")])),
        };
        assert!(check_ast(&early_right));

        assert!(!check_with_helpers(
            "choose(if true { s = x; 0 } else { 0 }, s)",
            "fn choose(ignored, fixed) { fixed }",
        )); // Source `if` is deliberately outside the admitted fragment.

        let params = vec![("ignored".into(), "".into()), ("fixed".into(), "".into())];
        let helper_body = vec![Stmt::ExprStmt(var("fixed"))];
        let first_actual = block(
            vec![Stmt::Assign {
                target: var("s"),
                value: var("x"),
            }],
            Expr::Literal("0".into()),
        );
        let call_with_write = call("choose", vec![first_actual, var("s")]);
        assert!(!check_ast_with_helper(
            &call_with_write,
            "choose",
            &params,
            &helper_body,
        ));

        let return_actual = call("choose", vec![call("return", vec![var("s")]), var("x")]);
        assert!(check_ast_with_helper(
            &return_actual,
            "choose",
            &params,
            &helper_body,
        )); // The second actual and helper body are unreachable.

        let helper_returns = vec![
            Stmt::ExprStmt(call("return", vec![var("fixed")])),
            Stmt::ExprStmt(call("random", vec![])),
        ];
        let call_and_continue = block(
            vec![Stmt::ExprStmt(call(
                "choose",
                vec![Expr::Literal("0".into()), var("s")],
            ))],
            var("x"),
        );
        assert!(!check_ast_with_helper(
            &call_and_continue,
            "choose",
            &params,
            &helper_returns,
        )); // The helper's return does not exit the callback.
    }

    #[test]
    fn varying_values_and_unsupported_transfers_do_not_become_stable() {
        for body in [
            "x",
            "[s, x]",
            "s = x; s",
            "let s = x; s",
            "return x; s",
            "random()",
            "now()",
            "input()",
            "random(); s",
            "let y = random(); s",
            "helper(s); s",
            "push(s, x); s",
            "s[0] = x; s",
            "if true { s = x; } s",
            "println(if true { s = x; x } else { x }); s",
            "println(random()); s",
            "let y = println(x); s",
            "if true { s } else { s }",
            "s + s",
        ] {
            assert!(!check(body), "expected conservative fallback: {body}");
        }
    }

    #[test]
    fn statement_output_and_expression_output_have_different_resolution() {
        assert!(check("let println = x; println(x); s"));
        assert!(!check("let println = x; let y = println(x); s"));
        assert!(!check("println(x)")); // Explicit expression tail has ordinary resolution.
        assert!(check("println(x);")); // Statement tail leaves the default return value.
    }

    #[test]
    fn implicit_statement_tail_and_default_zero_match_lowering() {
        assert!(!check("x;"));
        assert!(check("s;"));
        assert!(check("let y = x;"));
        assert!(check("s = x;"));
        assert!(!check("if true { x; } else { s; }"));
    }

    #[test]
    fn printed_blocks_preserve_outer_mutation_and_lexical_shadowing() {
        let mutation = block(
            vec![Stmt::Assign {
                target: var("s"),
                value: var("x"),
            }],
            var("x"),
        );
        assert!(!check_ast(&block(
            vec![Stmt::ExprStmt(call("println", vec![mutation]))],
            var("s"),
        )));
        let shadow = block(
            vec![Stmt::Let {
                name: "s".into(),
                ty: None,
                init: var("x"),
                span: Span::default(),
            }],
            var("s"),
        );
        assert!(check_ast(&block(
            vec![Stmt::ExprStmt(call("println", vec![shadow]))],
            var("s"),
        )));
    }

    #[test]
    fn returns_in_print_arguments_and_initializers_stop_continuation() {
        assert!(!check_ast(&block(
            vec![Stmt::ExprStmt(call(
                "println",
                vec![call("return", vec![var("x")])]
            ))],
            var("s"),
        )));
        assert!(check_ast(&block(
            vec![Stmt::ExprStmt(call(
                "println",
                vec![call("return", vec![var("s")]), call("random", vec![]),]
            ))],
            var("x"),
        )));
        assert!(!check_ast(&block(
            vec![Stmt::Let {
                name: "y".into(),
                ty: None,
                init: call("return", vec![var("x")]),
                span: Span::default(),
            }],
            var("s"),
        )));
    }

    #[test]
    fn return_namespace_is_required_only_in_expression_position() {
        let return_s = call("return", vec![var("s")]);
        let local_return = Rc::new(BTreeSet::from(["return".to_string()]));
        assert!(!key_is_repeatable(
            &[],
            &return_s,
            &["s".into()],
            local_return,
            &|_| None,
            &mut SummaryBudget::default(),
        ));
        assert!(!key_is_repeatable(
            &["return".into()],
            &return_s,
            &["s".into()],
            Rc::new(BTreeSet::from(["return".to_string()])),
            &|_| None,
            &mut SummaryBudget::default(),
        ));
        let statement = block(vec![Stmt::ExprStmt(return_s)], var("x"));
        assert!(key_is_repeatable(
            &["x".into(), "return".into()],
            &statement,
            &["s".into()],
            Rc::new(BTreeSet::from(["return".to_string()])),
            &|_| None,
            &mut SummaryBudget::default(),
        ));
    }

    #[test]
    fn bounded_fallback_covers_depth_work_and_shared_term_growth() {
        // Exercise refusal paths with small injected budgets, not a stress/crash workload.
        let no_functions = |_: &str| None;
        let summary = || Summary {
            scopes: vec![BTreeMap::new()],
            terms: BTreeMap::new(),
            remaining: MAX_WORK,
            max_depth: MAX_DEPTH,
            max_terms: MAX_TERMS,
            owner_locals: Rc::new(BTreeSet::new()),
            free_functions: &no_functions,
            active_helpers: BTreeSet::new(),
            max_call_depth: MAX_CALL_DEPTH,
        };
        let array = Expr::ArrayLiteral {
            elements: vec![Expr::Literal("1".into())],
        };
        let mut shallow = summary();
        shallow.max_depth = 0;
        assert!(shallow.expr(&array, 0).is_err());
        let mut short_work = summary();
        short_work.remaining = 1;
        assert!(short_work.expr(&array, 0).is_err());
        let mut short_terms = summary();
        short_terms.max_terms = 1;
        assert!(short_terms.expr(&array, 0).is_err());

        // Rebinding can deepen a shared construction without deepening the source AST.
        let assignments = (0..5)
            .map(|_| Stmt::Assign {
                target: var("s"),
                value: Expr::ArrayLiteral {
                    elements: vec![var("s"), var("s")],
                },
            })
            .collect();
        let shared = block(assignments, var("s"));
        let mut short_term_depth = summary();
        short_term_depth.max_depth = 4;
        let capture = short_term_depth.intern(Term::Capture(0), 0).unwrap();
        short_term_depth.bind("s", Value::Stable(capture)).unwrap();
        assert!(short_term_depth.expr(&shared, 0).is_err());
    }
}
