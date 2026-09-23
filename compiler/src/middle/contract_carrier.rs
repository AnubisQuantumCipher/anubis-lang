//! Item 21 Family 1: every place a function-valued parameter's contract can be reached, collected
//! from the callee's body so the call site can discharge it.
//!
//! The collector this replaces saw only applications that run unconditionally and only callees whose
//! NAME was a formal, so a guard, a loop, an `else` arm, a `match` arm, a lambda or a local alias hid
//! the call and the precondition was never checked. This one is TOTAL over the AST and records four
//! kinds of site ([`CarrierKind`]).
//!
//! Its facts are sound by construction. A guard or argument is kept only if it cannot change during
//! one execution of the callee: a formal that is never assigned, a `let`-bound local that is never
//! assigned (replaced by its initializer), a literal, or a `for` variable over stable bounds (a fresh
//! symbol constrained per iteration). Every other callee-internal name becomes an unmodeled
//! placeholder, so no fact about a mutable variable is ever used and no callee name can capture a
//! caller name once formals are substituted at the call site. A clause over a placeholder is not
//! modelable, which the discharge reports as an explicit unresolved obligation, never a silent pass.

use super::{collect_assigned_roots, match_arm_pattern_fact};
use crate::frontend::{Expr, ForSource, Stmt};
use std::collections::{BTreeMap, BTreeSet};

/// Prefix of the placeholder that stands for a value this analysis does not model.
pub(super) const UNMODELED_PREFIX: &str = "__anubis_carrier_unmodeled_";
/// Prefix of the per-loop symbol that stands for a `for` variable over stable bounds.
pub(super) const FOR_VAR_PREFIX: &str = "__anubis_carrier_forvar_";

/// A path condition over stable values: `expr` (or `!expr` when `negate`) holds wherever the site runs.
#[derive(Debug, Clone)]
pub(super) struct Guard {
    pub expr: Expr,
    pub negate: bool,
}

/// A `for` variable over a stable range. At the call site `sym` is modeled, with `start <= sym < end`,
/// only when both bounds are integer-modelable after substitution; otherwise it stays unmodeled.
#[derive(Debug, Clone)]
pub(super) struct ForVar {
    pub sym: String,
    pub start: Expr,
    pub end: Expr,
}

#[derive(Debug, Clone)]
pub(super) enum CarrierKind {
    /// A call whose callee may be a formal-derived function value. `callee` is resolved after
    /// substitution at the call site.
    Apply { callee: Expr, args: Vec<Expr> },
    /// A call to a global name with a formal-derived argument. A user function is followed into its
    /// own carrier sites; anything else (a builtin) is an escape of that argument.
    CallNamed { callee: String, args: Vec<Expr> },
    /// A formal-derived value that reaches a place this analysis cannot follow.
    Escape { value: Expr, reason: &'static str },
    /// A formal-derived value returned to the caller.
    Return { value: Expr },
}

#[derive(Debug, Clone)]
pub(super) struct CarrierItem {
    pub kind: CarrierKind,
    pub guards: Vec<Guard>,
    pub for_vars: Vec<ForVar>,
}

/// What function values a name or expression may hold, expressed over the callee's formals.
#[derive(Debug, Clone, Default)]
struct Denot {
    /// Formal-rooted expressions (after resolution) the value may be.
    members: Vec<Expr>,
    /// Some formal-derived value flows here in a shape that cannot be written as an expression over
    /// formals (a reassigned alias inside a compound expression).
    unexpressible: bool,
}

impl Denot {
    fn carries_formal(&self) -> bool {
        !self.members.is_empty() || self.unexpressible
    }
    fn join(&mut self, other: &Denot) {
        for m in &other.members {
            let key = format!("{m:?}");
            if !self.members.iter().any(|x| format!("{x:?}") == key) {
                self.members.push(m.clone());
            }
        }
        self.unexpressible |= other.unexpressible;
    }
}

#[derive(Debug, Clone)]
enum Binding {
    /// Never assigned: its value is `value`, an expression over formals, literals and function names.
    Stable { value: Expr, denot: Denot },
    /// Assigned somewhere in the body: its value is not modeled; `denot` tracks the function values it
    /// may hold, flow-sensitively (strong update, union at joins, widened at loop entry).
    Reassigned { denot: Denot, placeholder: String },
    /// A pattern binder, lambda parameter or other value this analysis does not track.
    Opaque { placeholder: String },
}

struct Collector<'a> {
    reassigned: BTreeSet<String>,
    scopes: Vec<BTreeMap<String, Binding>>,
    guards: Vec<Guard>,
    for_vars: Vec<ForVar>,
    out: Vec<CarrierItem>,
    counter: usize,
    fn_name: &'a str,
}

/// Collect every carrier site of `body`, the body of function `fn_name` with the given formals.
pub(super) fn collect_carrier_items(
    fn_name: &str,
    formals: &[String],
    body: &[Stmt],
) -> Vec<CarrierItem> {
    let formal_set: BTreeSet<String> = formals.iter().cloned().collect();
    let mut reassigned = BTreeSet::new();
    collect_assigned_roots(body, &mut reassigned);
    // `collect_assigned_roots` counts the free-function mutators `push`/`pop`/`insert`/`remove` but
    // not their method-call syntax (`xs.pop()` parses as a `CallExpr` on a `FieldAccess`), which would
    // leave a mutated receiver looking stable. Add those receivers so a mutated value is never treated
    // as a stable fact.
    collect_method_mutated_roots(body, &mut reassigned);
    // Only formals actually USED as functions carry a contract; a value parameter used purely as data
    // (`bs[i]`, `len(bs)`, an argument to a non-higher-order builtin) does not, and seeding it would
    // make ordinary code look like a carrier site. Over-approximate toward function-like (an unknown
    // callee is treated as a user function that might apply its argument); the call-site early-out and
    // the recursion, which returns cleanly when a callee has no carrier items, keep that from causing a
    // false rejection.
    let func_like = function_like_formals(&formal_set, body);
    let mut c = Collector {
        reassigned,
        scopes: vec![BTreeMap::new()],
        guards: Vec::new(),
        for_vars: Vec::new(),
        out: Vec::new(),
        counter: 0,
        fn_name,
    };
    for f in formals {
        if !func_like.contains(f) {
            // A pure-data parameter is still a STABLE VALUE (it substitutes to the call-site argument),
            // just one that carries no function — so a guard or loop bound over it keeps resolving,
            // while its uses produce no carrier sites.
            c.scopes[0].insert(
                f.clone(),
                Binding::Stable {
                    value: Expr::Var(f.clone()),
                    denot: Denot::default(),
                },
            );
            continue;
        }
        let denot = Denot {
            members: vec![Expr::Var(f.clone())],
            unexpressible: false,
        };
        let b = if c.reassigned.contains(f) {
            let placeholder = c.fresh_placeholder(f);
            Binding::Reassigned { denot, placeholder }
        } else {
            Binding::Stable {
                value: Expr::Var(f.clone()),
                denot,
            }
        };
        c.scopes[0].insert(f.clone(), b);
    }
    c.walk_stmts(body);
    c.out
}

/// Whether `e` mentions an unmodeled placeholder.
pub(super) fn mentions_unmodeled(e: &Expr) -> bool {
    let mut vars = BTreeSet::new();
    super::collect_expr_vars(e, &mut vars);
    vars.iter().any(|v| v.starts_with(UNMODELED_PREFIX))
}

impl Collector<'_> {
    fn fresh_placeholder(&mut self, base: &str) -> String {
        self.counter += 1;
        format!(
            "{UNMODELED_PREFIX}{}_{}_{}",
            self.fn_name, base, self.counter
        )
    }

    fn lookup(&self, name: &str) -> Option<&Binding> {
        self.scopes.iter().rev().find_map(|s| s.get(name))
    }

    fn push(&mut self, kind: CarrierKind) {
        self.out.push(CarrierItem {
            kind,
            guards: self.guards.clone(),
            for_vars: self.for_vars.clone(),
        });
    }

    fn bind(&mut self, name: &str, b: Binding) {
        self.scopes
            .last_mut()
            .expect("collector always has a scope")
            .insert(name.to_string(), b);
    }

    fn bind_opaque(&mut self, name: &str) {
        let placeholder = self.fresh_placeholder(name);
        self.bind(name, Binding::Opaque { placeholder });
    }

    // ---- resolution -------------------------------------------------------------------------

    /// `e` with every callee-internal name replaced: stable names by their value, everything else by
    /// its placeholder. Global names (functions, builtins) are kept. Blocks, `match` and `if let`
    /// values, and lambdas, are not modeled as values.
    fn resolve(&mut self, e: &Expr) -> Expr {
        match e {
            Expr::Var(n) => match self.lookup(n) {
                Some(Binding::Stable { value, .. }) => value.clone(),
                Some(Binding::Reassigned { placeholder, .. })
                | Some(Binding::Opaque { placeholder }) => Expr::Var(placeholder.clone()),
                None => e.clone(),
            },
            Expr::Call { callee, args } => {
                let args = args.iter().map(|a| self.resolve(a)).collect();
                match self.lookup(callee) {
                    // A call through a local: keep it as a first-class call on the local's value.
                    Some(_) => Expr::CallExpr {
                        callee: Box::new(self.resolve(&Expr::Var(callee.clone()))),
                        args,
                    },
                    None => Expr::Call {
                        callee: callee.clone(),
                        args,
                    },
                }
            }
            Expr::CallExpr { callee, args } => Expr::CallExpr {
                callee: Box::new(self.resolve(callee)),
                args: args.iter().map(|a| self.resolve(a)).collect(),
            },
            Expr::Binary { op, lhs, rhs } => Expr::Binary {
                op: op.clone(),
                lhs: Box::new(self.resolve(lhs)),
                rhs: Box::new(self.resolve(rhs)),
            },
            Expr::Unary { op, expr } => Expr::Unary {
                op: op.clone(),
                expr: Box::new(self.resolve(expr)),
            },
            Expr::ArrayLiteral { elements } => Expr::ArrayLiteral {
                elements: elements.iter().map(|x| self.resolve(x)).collect(),
            },
            Expr::Index { base, index } => Expr::Index {
                base: Box::new(self.resolve(base)),
                index: Box::new(self.resolve(index)),
            },
            Expr::Cast { expr, ty } => Expr::Cast {
                expr: Box::new(self.resolve(expr)),
                ty: ty.clone(),
            },
            Expr::FieldAccess { base, field, span } => Expr::FieldAccess {
                base: Box::new(self.resolve(base)),
                field: field.clone(),
                span: *span,
            },
            Expr::StructLiteral { name, fields, span } => Expr::StructLiteral {
                name: name.clone(),
                fields: fields
                    .iter()
                    .map(|(k, v)| (k.clone(), Box::new(self.resolve(v))))
                    .collect(),
                span: *span,
            },
            Expr::MapLiteral { entries, span } => Expr::MapLiteral {
                entries: entries
                    .iter()
                    .map(|(k, v)| (self.resolve(k), self.resolve(v)))
                    .collect(),
                span: *span,
            },
            Expr::EnumConstruct {
                enum_name,
                variant,
                fields,
                field_names,
                span,
            } => Expr::EnumConstruct {
                enum_name: enum_name.clone(),
                variant: variant.clone(),
                fields: fields.iter().map(|x| self.resolve(x)).collect(),
                field_names: field_names.clone(),
                span: *span,
            },
            Expr::If {
                cond,
                then,
                else_,
                span,
            } => Expr::If {
                cond: Box::new(self.resolve(cond)),
                then: Box::new(self.resolve(then)),
                else_: Box::new(self.resolve(else_)),
                span: *span,
            },
            Expr::Tainted { ty, inner } => Expr::Tainted {
                ty: ty.clone(),
                inner: Box::new(self.resolve(inner)),
            },
            Expr::Declassify {
                inner,
                policy,
                reason,
            } => Expr::Declassify {
                inner: Box::new(self.resolve(inner)),
                policy: policy.clone(),
                reason: reason.clone(),
            },
            Expr::Try(inner) => Expr::Try(Box::new(self.resolve(inner))),
            Expr::Literal(_)
            | Expr::StrLiteral(_)
            | Expr::Symbolic { .. }
            | Expr::TaintSource { .. }
            | Expr::UnifiedBuffer { .. }
            | Expr::RawPtr { .. } => e.clone(),
            Expr::Block { .. }
            | Expr::Match { .. }
            | Expr::IfLet { .. }
            | Expr::Lambda { .. }
            | Expr::Assume(_)
            | Expr::Assert(_)
            | Expr::Other(_) => {
                let p = self.fresh_placeholder("value");
                Expr::Var(p)
            }
        }
    }

    /// Whether a name currently may hold a formal-derived function value.
    fn name_carries(&self, n: &str) -> bool {
        match self.lookup(n) {
            Some(Binding::Stable { denot, .. }) | Some(Binding::Reassigned { denot, .. }) => {
                denot.carries_formal()
            }
            _ => false,
        }
    }

    /// Whether the VALUE of `e` may be (or contain) a formal-derived function. Lambda bodies are
    /// excluded: they are walked, and every use inside them is recorded where it happens. A call's
    /// result counts when its callee or an argument carries one (it may return it); the call site
    /// filters by the resolved identity, so a scalar result is not reported.
    fn carries(&self, e: &Expr) -> bool {
        match e {
            Expr::Var(n) => self.name_carries(n),
            Expr::Lambda { .. } => false,
            Expr::Call { callee, args } => {
                self.name_carries(callee) || args.iter().any(|a| self.carries(a))
            }
            _ => {
                let mut found = false;
                for_each_child_expr(e, &mut |c| found |= self.carries(c));
                found
            }
        }
    }

    /// Whether `e` mentions a reassigned alias that may hold a formal-derived function — such a value
    /// cannot be written as an expression over formals.
    fn mentions_reassigned_carrier(&self, e: &Expr) -> bool {
        match e {
            Expr::Var(n) => matches!(
                self.lookup(n),
                Some(Binding::Reassigned { denot, .. }) if denot.carries_formal()
            ),
            Expr::Lambda { .. } => false,
            Expr::Call { callee, args } => {
                matches!(
                    self.lookup(callee),
                    Some(Binding::Reassigned { denot, .. }) if denot.carries_formal()
                ) || args.iter().any(|a| self.mentions_reassigned_carrier(a))
            }
            _ => {
                let mut found = false;
                for_each_child_expr(e, &mut |c| found |= self.mentions_reassigned_carrier(c));
                found
            }
        }
    }

    /// The function values `e` may denote, over formals.
    fn denot_of(&mut self, e: &Expr) -> Denot {
        match e {
            Expr::Var(n) => match self.lookup(n) {
                Some(Binding::Stable { denot, .. }) | Some(Binding::Reassigned { denot, .. }) => {
                    denot.clone()
                }
                _ => Denot::default(),
            },
            Expr::If { then, else_, .. } => {
                let mut d = self.denot_of(then);
                let e2 = self.denot_of(else_);
                d.join(&e2);
                d
            }
            _ if !self.carries(e) => Denot::default(),
            _ if self.mentions_reassigned_carrier(e) => Denot {
                members: vec![],
                unexpressible: true,
            },
            Expr::Block { .. } | Expr::Match { .. } | Expr::IfLet { .. } => Denot {
                members: vec![],
                unexpressible: true,
            },
            _ => Denot {
                members: vec![self.resolve(e)],
                unexpressible: false,
            },
        }
    }

    /// Guard usable iff it mentions only stable values after resolution.
    fn stable_guard(&mut self, cond: &Expr) -> Option<Expr> {
        let r = self.resolve(cond);
        (!mentions_unmodeled(&r)).then_some(r)
    }

    // ---- recording --------------------------------------------------------------------------

    fn record_args_escape(&mut self, args: &[Expr], reason: &'static str) {
        for a in args {
            if self.carries(a) {
                let value = self.resolve(a);
                self.push(CarrierKind::Escape { value, reason });
            }
        }
    }

    fn record_apply(&mut self, d: &Denot, args: &[Expr]) {
        let rargs: Vec<Expr> = args.iter().map(|a| self.resolve(a)).collect();
        for m in &d.members {
            self.push(CarrierKind::Apply {
                callee: m.clone(),
                args: rargs.clone(),
            });
        }
        if d.unexpressible {
            let value = Expr::Var(self.fresh_placeholder("alias"));
            self.push(CarrierKind::Escape {
                value,
                reason: "called through an alias this analysis cannot express",
            });
        }
        self.record_args_escape(args, "passed to a function-valued parameter");
    }

    // ---- walking ----------------------------------------------------------------------------

    fn walk_expr(&mut self, e: &Expr) {
        match e {
            Expr::Call { callee, args } => {
                for a in args {
                    self.walk_expr(a);
                }
                if callee == "return" {
                    for a in args {
                        if self.carries(a) {
                            let value = self.resolve(a);
                            self.push(CarrierKind::Return { value });
                        }
                    }
                    return;
                }
                if self.lookup(callee).is_some() {
                    let d = self.denot_of(&Expr::Var(callee.clone()));
                    if d.carries_formal() {
                        self.record_apply(&d, args);
                    } else {
                        self.record_args_escape(args, "passed to a local callee");
                    }
                } else if args.iter().any(|a| self.carries(a)) {
                    let rargs = args.iter().map(|a| self.resolve(a)).collect();
                    self.push(CarrierKind::CallNamed {
                        callee: callee.clone(),
                        args: rargs,
                    });
                }
            }
            Expr::CallExpr { callee, args } => {
                self.walk_expr(callee);
                for a in args {
                    self.walk_expr(a);
                }
                if let Expr::FieldAccess { base, .. } = callee.as_ref() {
                    if self.carries(base) {
                        // `b.h(..)` on a formal-derived base: a stored function or a method; resolved
                        // (and, if it cannot be resolved, refused) at the call site.
                        let d = Denot {
                            members: vec![self.resolve(callee)],
                            unexpressible: self.mentions_reassigned_carrier(base),
                        };
                        self.record_apply(&d, args);
                    } else {
                        self.record_args_escape(args, "passed to a method");
                    }
                    return;
                }
                let d = self.denot_of(callee);
                if d.carries_formal() {
                    self.record_apply(&d, args);
                } else {
                    self.record_args_escape(
                        args,
                        "passed to a callee this analysis cannot resolve",
                    );
                }
            }
            Expr::Binary { op, lhs, rhs } => {
                self.walk_expr(lhs);
                if op == "&&" || op == "||" {
                    let g0 = self.guards.len();
                    if let Some(g) = self.stable_guard(lhs) {
                        self.guards.push(Guard {
                            expr: g,
                            negate: op == "||",
                        });
                    }
                    self.walk_expr(rhs);
                    self.guards.truncate(g0);
                } else {
                    self.walk_expr(rhs);
                }
            }
            Expr::If {
                cond, then, else_, ..
            } => {
                self.walk_expr(cond);
                let g = self.stable_guard(cond);
                for (branch, negate) in [(then.as_ref(), false), (else_.as_ref(), true)] {
                    let g0 = self.guards.len();
                    if let Some(g) = &g {
                        self.guards.push(Guard {
                            expr: g.clone(),
                            negate,
                        });
                    }
                    self.walk_expr(branch);
                    self.guards.truncate(g0);
                }
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                self.walk_expr(scrutinee);
                if self.carries(scrutinee) {
                    let value = self.resolve(scrutinee);
                    self.push(CarrierKind::Escape {
                        value,
                        reason: "destructured by a match",
                    });
                }
                let scrut = self.resolve(scrutinee);
                let scrut_stable = !mentions_unmodeled(&scrut);
                for arm in arms {
                    let g0 = self.guards.len();
                    self.scopes.push(BTreeMap::new());
                    if scrut_stable {
                        if let Some(fact) = match_arm_pattern_fact(&scrut, &arm.pattern) {
                            self.guards.push(Guard {
                                expr: fact,
                                negate: false,
                            });
                        }
                    }
                    for n in arm.pattern.bound_names() {
                        self.bind_opaque(&n);
                    }
                    if let Some(guard) = &arm.guard {
                        self.walk_expr(guard);
                        if let Some(g) = self.stable_guard(guard) {
                            self.guards.push(Guard {
                                expr: g,
                                negate: false,
                            });
                        }
                    }
                    self.walk_expr(&arm.body);
                    self.scopes.pop();
                    self.guards.truncate(g0);
                }
            }
            Expr::IfLet {
                pattern,
                scrutinee,
                then,
                else_,
                ..
            } => {
                self.walk_expr(scrutinee);
                if self.carries(scrutinee) {
                    let value = self.resolve(scrutinee);
                    self.push(CarrierKind::Escape {
                        value,
                        reason: "destructured by an if-let",
                    });
                }
                self.scopes.push(BTreeMap::new());
                for n in pattern.bound_names() {
                    self.bind_opaque(&n);
                }
                self.walk_expr(then);
                self.scopes.pop();
                self.walk_expr(else_);
            }
            Expr::Block { stmts, tail } => {
                self.scopes.push(BTreeMap::new());
                let g0 = self.guards.len();
                let terminated = self.walk_stmt_list(stmts);
                if !terminated {
                    if let Some(t) = tail {
                        self.walk_expr(t);
                    }
                }
                self.guards.truncate(g0);
                self.scopes.pop();
            }
            Expr::Lambda { params, body } => {
                self.scopes.push(BTreeMap::new());
                for p in params {
                    self.bind_opaque(p);
                }
                self.walk_expr(body);
                self.scopes.pop();
            }
            _ => {
                let mut children: Vec<Expr> = Vec::new();
                for_each_child_expr(e, &mut |c| children.push(c.clone()));
                for c in &children {
                    self.walk_expr(c);
                }
            }
        }
    }

    fn walk_stmts(&mut self, stmts: &[Stmt]) {
        let g0 = self.guards.len();
        self.walk_stmt_list(stmts);
        self.guards.truncate(g0);
    }

    fn walk_scoped(&mut self, stmts: &[Stmt]) -> bool {
        self.scopes.push(BTreeMap::new());
        let g0 = self.guards.len();
        let t = self.walk_stmt_list(stmts);
        self.guards.truncate(g0);
        self.scopes.pop();
        t
    }

    /// Walk a statement list in the current scope. Returns true when the list definitely does not
    /// fall through (it ends in, or reaches, `return` / `break` / `continue` on every path).
    fn walk_stmt_list(&mut self, stmts: &[Stmt]) -> bool {
        for st in stmts {
            match st {
                Stmt::Let { name, init, .. } => {
                    self.walk_expr(init);
                    let denot = self.denot_of(init);
                    let b = if self.reassigned.contains(name) {
                        let placeholder = self.fresh_placeholder(name);
                        Binding::Reassigned { denot, placeholder }
                    } else {
                        let value = self.resolve(init);
                        Binding::Stable { value, denot }
                    };
                    self.bind(name, b);
                }
                Stmt::LetPattern { pattern, init, .. } => {
                    self.walk_expr(init);
                    if self.carries(init) {
                        let value = self.resolve(init);
                        self.push(CarrierKind::Escape {
                            value,
                            reason: "destructured by a let pattern",
                        });
                    }
                    for n in pattern.bound_names() {
                        self.bind_opaque(&n);
                    }
                }
                Stmt::Assign { target, value } => {
                    self.walk_expr(value);
                    match target {
                        Expr::Var(n) => {
                            let d = self.denot_of(value);
                            self.strong_update(n, d);
                        }
                        _ => {
                            let mut children: Vec<Expr> = Vec::new();
                            for_each_child_expr(target, &mut |c| children.push(c.clone()));
                            for c in &children {
                                self.walk_expr(c);
                            }
                            if self.carries(value) {
                                let v = self.resolve(value);
                                self.push(CarrierKind::Escape {
                                    value: v,
                                    reason: "written into a field or index",
                                });
                            }
                        }
                    }
                }
                Stmt::If { cond, then, else_ } => {
                    self.walk_expr(cond);
                    let g = self.stable_guard(cond);
                    let before = self.scopes.clone();
                    let t_term = self.walk_guarded(then, g.as_ref(), false);
                    let after_then = std::mem::replace(&mut self.scopes, before);
                    let e_term = match else_ {
                        Some(e) => self.walk_guarded(e, g.as_ref(), true),
                        None => false,
                    };
                    self.join_scopes(&after_then);
                    if t_term && e_term {
                        return true;
                    }
                    if let Some(g) = g {
                        if t_term {
                            self.guards.push(Guard {
                                expr: g,
                                negate: true,
                            });
                        } else if e_term {
                            self.guards.push(Guard {
                                expr: g,
                                negate: false,
                            });
                        }
                    }
                }
                Stmt::While { cond, body, .. } => {
                    self.walk_expr(cond);
                    let widened = self.widen_for_loop(body);
                    let g = self.stable_guard(cond);
                    self.walk_guarded(body, g.as_ref(), false);
                    self.restore_reassigned(widened);
                }
                Stmt::Loop { body, .. } => {
                    let widened = self.widen_for_loop(body);
                    self.walk_scoped(body);
                    self.restore_reassigned(widened);
                }
                Stmt::WhileLet {
                    pattern,
                    expr,
                    body,
                } => {
                    self.walk_expr(expr);
                    if self.carries(expr) {
                        let value = self.resolve(expr);
                        self.push(CarrierKind::Escape {
                            value,
                            reason: "destructured by a while-let",
                        });
                    }
                    let widened = self.widen_for_loop(body);
                    self.scopes.push(BTreeMap::new());
                    for n in pattern.bound_names() {
                        self.bind_opaque(&n);
                    }
                    let g0 = self.guards.len();
                    self.walk_stmt_list(body);
                    self.guards.truncate(g0);
                    self.scopes.pop();
                    self.restore_reassigned(widened);
                }
                Stmt::For {
                    var, source, body, ..
                } => {
                    let widened = self.widen_for_loop(body);
                    self.scopes.push(BTreeMap::new());
                    let f0 = self.for_vars.len();
                    // A `for` variable is rebound by the loop but constant WITHIN one iteration, so it
                    // is a stable value there — unless the body reassigns it.
                    let mut body_writes = BTreeSet::new();
                    collect_assigned_roots(body, &mut body_writes);
                    match source {
                        ForSource::Range { start, end } => {
                            self.walk_expr(start);
                            self.walk_expr(end);
                            let s = self.stable_guard(start);
                            let e = self.stable_guard(end);
                            match (s, e) {
                                (Some(start), Some(end)) if !body_writes.contains(var) => {
                                    self.counter += 1;
                                    let sym = format!(
                                        "{FOR_VAR_PREFIX}{}_{}_{}",
                                        self.fn_name, var, self.counter
                                    );
                                    self.for_vars.push(ForVar {
                                        sym: sym.clone(),
                                        start,
                                        end,
                                    });
                                    self.bind(
                                        var,
                                        Binding::Stable {
                                            value: Expr::Var(sym),
                                            denot: Denot::default(),
                                        },
                                    );
                                }
                                _ => self.bind_opaque(var),
                            }
                        }
                        ForSource::Collection { expr } => {
                            self.walk_expr(expr);
                            if self.carries(expr) {
                                let value = self.resolve(expr);
                                self.push(CarrierKind::Escape {
                                    value,
                                    reason: "iterated by a for loop",
                                });
                            }
                            self.bind_opaque(var);
                        }
                    }
                    let g0 = self.guards.len();
                    self.walk_stmt_list(body);
                    self.guards.truncate(g0);
                    self.for_vars.truncate(f0);
                    self.scopes.pop();
                    self.restore_reassigned(widened);
                }
                Stmt::Break | Stmt::Continue => return true,
                Stmt::ExprStmt(e) => {
                    self.walk_expr(e);
                    if matches!(e, Expr::Call { callee, .. } if callee == "return") {
                        return true;
                    }
                }
                Stmt::ResearchBlock { body, .. } | Stmt::ExploitBlock { body, .. } => {
                    self.walk_scoped(body);
                }
                Stmt::HybridBlock { gpu, cpu, prove } => {
                    for b in [gpu, cpu, prove].into_iter().flatten() {
                        self.walk_scoped(b);
                    }
                }
                Stmt::SpecBlock { .. } => {}
            }
        }
        false
    }

    /// Walk a branch body in its own scope under `cond` (negated when `negate`) if it is stable.
    fn walk_guarded(&mut self, body: &[Stmt], cond: Option<&Expr>, negate: bool) -> bool {
        let g0 = self.guards.len();
        if let Some(c) = cond {
            self.guards.push(Guard {
                expr: c.clone(),
                negate,
            });
        }
        let t = self.walk_scoped(body);
        self.guards.truncate(g0);
        t
    }

    fn strong_update(&mut self, name: &str, d: Denot) {
        for s in self.scopes.iter_mut().rev() {
            if let Some(b) = s.get_mut(name) {
                if let Binding::Reassigned { denot, .. } = b {
                    *denot = d;
                }
                return;
            }
        }
    }

    /// After two branches: every reassigned binding may hold what either branch left in it.
    fn join_scopes(&mut self, other: &[BTreeMap<String, Binding>]) {
        for (mine, theirs) in self.scopes.iter_mut().zip(other.iter()) {
            for (name, b) in mine.iter_mut() {
                if let (
                    Binding::Reassigned { denot, .. },
                    Some(Binding::Reassigned { denot: d2, .. }),
                ) = (b, theirs.get(name))
                {
                    denot.join(d2);
                }
            }
        }
    }

    /// Before a loop body: a reassigned binding may hold, on any iteration, its pre-loop value or any
    /// value the body assigns to it. Returns the widened bindings so they can be restored after the
    /// body (the body's strong updates are only valid within one pass).
    fn widen_for_loop(&mut self, body: &[Stmt]) -> Vec<(usize, String, Denot)> {
        let mut assigned: Vec<(String, Expr)> = Vec::new();
        collect_var_assignments(body, &mut assigned);
        // Two rounds so an alias assigned from another in-loop alias picks up its widened set.
        for _ in 0..2 {
            for (name, value) in &assigned {
                let d = self.denot_of(value);
                for s in self.scopes.iter_mut().rev() {
                    if let Some(b) = s.get_mut(name) {
                        if let Binding::Reassigned { denot, .. } = b {
                            denot.join(&d);
                        }
                        break;
                    }
                }
            }
        }
        let mut snap = Vec::new();
        for (i, s) in self.scopes.iter().enumerate() {
            for (name, b) in s {
                if let Binding::Reassigned { denot, .. } = b {
                    snap.push((i, name.clone(), denot.clone()));
                }
            }
        }
        snap
    }

    fn restore_reassigned(&mut self, snap: Vec<(usize, String, Denot)>) {
        for (i, name, d) in snap {
            if let Some(Binding::Reassigned { denot, .. }) =
                self.scopes.get_mut(i).and_then(|s| s.get_mut(&name))
            {
                denot.join(&d);
            }
        }
    }
}

/// Every `name = value` plain-variable assignment in `body`, at any depth (lambda bodies included:
/// a closure can run inside the loop).
fn collect_var_assignments(body: &[Stmt], out: &mut Vec<(String, Expr)>) {
    for st in body {
        match st {
            Stmt::Assign {
                target: Expr::Var(n),
                value,
            } => {
                out.push((n.clone(), value.clone()));
                collect_var_assignments_expr(value, out);
            }
            Stmt::Assign { target, value } => {
                collect_var_assignments_expr(target, out);
                collect_var_assignments_expr(value, out);
            }
            Stmt::Let { init, .. } | Stmt::LetPattern { init, .. } => {
                collect_var_assignments_expr(init, out)
            }
            Stmt::ExprStmt(e) => collect_var_assignments_expr(e, out),
            Stmt::If { cond, then, else_ } => {
                collect_var_assignments_expr(cond, out);
                collect_var_assignments(then, out);
                if let Some(e) = else_ {
                    collect_var_assignments(e, out);
                }
            }
            Stmt::While { cond, body, .. } => {
                collect_var_assignments_expr(cond, out);
                collect_var_assignments(body, out);
            }
            Stmt::WhileLet { expr, body, .. } => {
                collect_var_assignments_expr(expr, out);
                collect_var_assignments(body, out);
            }
            Stmt::For { source, body, .. } => {
                match source {
                    ForSource::Range { start, end } => {
                        collect_var_assignments_expr(start, out);
                        collect_var_assignments_expr(end, out);
                    }
                    ForSource::Collection { expr } => collect_var_assignments_expr(expr, out),
                }
                collect_var_assignments(body, out);
            }
            Stmt::Loop { body, .. }
            | Stmt::ResearchBlock { body, .. }
            | Stmt::ExploitBlock { body, .. } => collect_var_assignments(body, out),
            Stmt::HybridBlock { gpu, cpu, prove } => {
                for b in [gpu, cpu, prove].into_iter().flatten() {
                    collect_var_assignments(b, out);
                }
            }
            Stmt::Break | Stmt::Continue | Stmt::SpecBlock { .. } => {}
        }
    }
}

fn collect_var_assignments_expr(e: &Expr, out: &mut Vec<(String, Expr)>) {
    match e {
        Expr::Block { stmts, tail } => {
            collect_var_assignments(stmts, out);
            if let Some(t) = tail {
                collect_var_assignments_expr(t, out);
            }
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            collect_var_assignments_expr(scrutinee, out);
            for a in arms {
                if let Some(g) = &a.guard {
                    collect_var_assignments_expr(g, out);
                }
                collect_var_assignments_expr(&a.body, out);
            }
        }
        Expr::IfLet {
            scrutinee,
            then,
            else_,
            ..
        } => {
            collect_var_assignments_expr(scrutinee, out);
            collect_var_assignments_expr(then, out);
            collect_var_assignments_expr(else_, out);
        }
        Expr::Lambda { body, .. } => collect_var_assignments_expr(body, out),
        _ => for_each_child_expr(e, &mut |c| collect_var_assignments_expr(c, out)),
    }
}

/// Visit the direct sub-expressions of `e` that are evaluated as expressions. Statement-bearing forms
/// (`Block`, `Match`, `IfLet`, `Lambda`) are visited through their expression children only; the
/// callers that need their statements handle them explicitly. TOTAL over [`Expr`]: a new variant must
/// be classified here, or this fails to compile.
fn for_each_child_expr<'e>(e: &'e Expr, f: &mut dyn FnMut(&'e Expr)) {
    match e {
        Expr::Call { args, .. } => {
            for a in args {
                f(a);
            }
        }
        Expr::CallExpr { callee, args } => {
            f(callee);
            for a in args {
                f(a);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            f(lhs);
            f(rhs);
        }
        Expr::Unary { expr, .. } | Expr::Cast { expr, .. } => f(expr),
        Expr::ArrayLiteral { elements } => {
            for x in elements {
                f(x);
            }
        }
        Expr::Index { base, index } => {
            f(base);
            f(index);
        }
        Expr::FieldAccess { base, .. } => f(base),
        Expr::StructLiteral { fields, .. } => fields.iter().for_each(|(_, v)| f(v)),
        Expr::MapLiteral { entries, .. } => entries.iter().for_each(|(k, v)| {
            f(k);
            f(v);
        }),
        Expr::EnumConstruct { fields, .. } => {
            for x in fields {
                f(x);
            }
        }
        Expr::If {
            cond, then, else_, ..
        } => {
            f(cond);
            f(then);
            f(else_);
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            f(scrutinee);
            for a in arms {
                if let Some(g) = &a.guard {
                    f(g);
                }
                f(&a.body);
            }
        }
        Expr::IfLet {
            scrutinee,
            then,
            else_,
            ..
        } => {
            f(scrutinee);
            f(then);
            f(else_);
        }
        Expr::Block { tail, .. } => {
            if let Some(t) = tail {
                f(t);
            }
        }
        Expr::Lambda { body, .. } => f(body),
        Expr::Tainted { inner, .. }
        | Expr::Declassify { inner, .. }
        | Expr::Assume(inner)
        | Expr::Assert(inner)
        | Expr::Try(inner) => f(inner),
        Expr::Var(_)
        | Expr::Literal(_)
        | Expr::StrLiteral(_)
        | Expr::Symbolic { .. }
        | Expr::TaintSource { .. }
        | Expr::UnifiedBuffer { .. }
        | Expr::RawPtr { .. }
        | Expr::Other(_) => {}
    }
}

/// The subset of `formals` that are used as FUNCTIONS in `body`, and so may carry a contract: called
/// (directly or through a `let`-alias chain), passed as a bare argument to a user function or a
/// higher-order builtin (which may apply it), returned, or stored into a container/struct/field/place.
/// A formal used only as data — an index base, a `len`/`push`/`print` argument, an arithmetic operand —
/// is excluded. Flow-insensitive and deliberately over-approximate: an unknown callee counts as a user
/// function. Missing a genuine function use would be unsound, so every non-data use marks the formal.
fn function_like_formals(formals: &BTreeSet<String>, body: &[Stmt]) -> BTreeSet<String> {
    // Local `let` alias closure: a name that may denote a formal.
    let mut alias: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for f in formals {
        alias.insert(f.clone(), BTreeSet::from([f.clone()]));
    }
    // Two passes so `let h = g; let k = h` reaches `g`.
    for _ in 0..2 {
        collect_alias_bindings(body, &mut alias);
    }
    let roots = |name: &str| -> BTreeSet<String> { alias.get(name).cloned().unwrap_or_default() };
    let mut out = BTreeSet::new();
    let mut mark = |e: &Expr, out: &mut BTreeSet<String>| {
        if let Expr::Var(n) = e {
            for r in roots(n) {
                out.insert(r);
            }
        }
    };
    let mut visit = FnLikeVisitor {
        out: &mut out,
        roots: &roots,
        mark: &mut mark,
    };
    visit.stmts(body);
    out
}

/// Collect `let x = <expr>` bindings where `<expr>` may denote a formal, extending the alias map.
fn collect_alias_bindings(body: &[Stmt], alias: &mut BTreeMap<String, BTreeSet<String>>) {
    fn expr_roots(e: &Expr, alias: &BTreeMap<String, BTreeSet<String>>) -> BTreeSet<String> {
        match e {
            Expr::Var(n) => alias.get(n).cloned().unwrap_or_default(),
            Expr::If { then, else_, .. } => {
                let mut s = expr_roots(then, alias);
                s.extend(expr_roots(else_, alias));
                s
            }
            _ => BTreeSet::new(),
        }
    }
    fn walk(body: &[Stmt], alias: &mut BTreeMap<String, BTreeSet<String>>) {
        for st in body {
            match st {
                Stmt::Let { name, init, .. } => {
                    let r = expr_roots(init, alias);
                    if !r.is_empty() {
                        alias.entry(name.clone()).or_default().extend(r);
                    }
                }
                Stmt::Assign {
                    target: Expr::Var(n),
                    value,
                } => {
                    let r = expr_roots(value, alias);
                    if !r.is_empty() {
                        alias.entry(n.clone()).or_default().extend(r);
                    }
                }
                Stmt::If { then, else_, .. } => {
                    walk(then, alias);
                    if let Some(e) = else_ {
                        walk(e, alias);
                    }
                }
                Stmt::While { body, .. }
                | Stmt::Loop { body, .. }
                | Stmt::WhileLet { body, .. }
                | Stmt::For { body, .. }
                | Stmt::ResearchBlock { body, .. }
                | Stmt::ExploitBlock { body, .. } => walk(body, alias),
                Stmt::HybridBlock { gpu, cpu, prove } => {
                    for b in [gpu, cpu, prove].into_iter().flatten() {
                        walk(b, alias);
                    }
                }
                _ => {}
            }
        }
    }
    walk(body, alias);
}

struct FnLikeVisitor<'a> {
    out: &'a mut BTreeSet<String>,
    roots: &'a dyn Fn(&str) -> BTreeSet<String>,
    mark: &'a mut dyn FnMut(&Expr, &mut BTreeSet<String>),
}

impl FnLikeVisitor<'_> {
    fn stmts(&mut self, body: &[Stmt]) {
        for st in body {
            match st {
                Stmt::Let { init, .. } | Stmt::LetPattern { init, .. } => self.expr(init),
                Stmt::Assign { target, value } => {
                    // A formal stored into a place (field/index/var) may be invoked later.
                    (self.mark)(value, self.out);
                    self.data_children(target);
                    self.expr(value);
                }
                Stmt::If { cond, then, else_ } => {
                    self.expr(cond);
                    self.stmts(then);
                    if let Some(e) = else_ {
                        self.stmts(e);
                    }
                }
                Stmt::While { cond, body, .. } => {
                    self.expr(cond);
                    self.stmts(body);
                }
                Stmt::WhileLet { expr, body, .. } => {
                    self.expr(expr);
                    self.stmts(body);
                }
                Stmt::For { source, body, .. } => {
                    match source {
                        ForSource::Range { start, end } => {
                            self.expr(start);
                            self.expr(end);
                        }
                        ForSource::Collection { expr } => self.expr(expr),
                    }
                    self.stmts(body);
                }
                Stmt::Loop { body, .. }
                | Stmt::ResearchBlock { body, .. }
                | Stmt::ExploitBlock { body, .. } => self.stmts(body),
                Stmt::HybridBlock { gpu, cpu, prove } => {
                    for b in [gpu, cpu, prove].into_iter().flatten() {
                        self.stmts(b);
                    }
                }
                Stmt::ExprStmt(e) => {
                    if let Expr::Call { callee, args } = e {
                        if callee == "return" {
                            for a in args {
                                (self.mark)(a, self.out);
                                self.expr(a);
                            }
                            continue;
                        }
                    }
                    self.expr(e);
                }
                Stmt::Break | Stmt::Continue | Stmt::SpecBlock { .. } => {}
            }
        }
    }

    /// A call/store: mark formals in FUNCTION positions (callee, applyable args, stored values); recurse
    /// into the rest as data.
    fn expr(&mut self, e: &Expr) {
        match e {
            Expr::Call { callee, args } => {
                // `callee` is a name; if it is a formal/alias it is applied.
                if self.roots_nonempty(callee) {
                    if let Some(set) = Some((self.roots)(callee)) {
                        for r in set {
                            self.out.insert(r);
                        }
                    }
                }
                // Mark every bare formal argument as possibly-applied. Enumerating exactly which
                // builtins apply a function argument is a standing under-approximation hazard (the 13
                // HOF names are not the whole surface — `apply`/`call`/`times`/`compose`/… also apply
                // their argument), and a MISS here is unsound (a formal applied only through such a
                // builtin would never be seeded). Over-marking is safe: the call-site `could_carry`
                // gate discharges only KNOWN function identities, so a pure-data parameter passed to
                // `len`/`push` is marked but produces no obligation.
                for a in args {
                    (self.mark)(a, self.out);
                    self.expr(a);
                }
            }
            Expr::CallExpr { callee, args } => {
                // The value in callee position is applied: mark the root of its access path
                // (`fs[0](..)`, `b.h(..)`, `g(..)`).
                self.mark_access_root(callee);
                self.expr(callee);
                for a in args {
                    (self.mark)(a, self.out);
                    self.expr(a);
                }
            }
            // Stored into an aggregate: may be invoked after extraction.
            Expr::ArrayLiteral { elements } => {
                for x in elements {
                    (self.mark)(x, self.out);
                    self.expr(x);
                }
            }
            Expr::StructLiteral { fields, .. } => {
                for (_, v) in fields {
                    (self.mark)(v, self.out);
                    self.expr(v);
                }
            }
            Expr::MapLiteral { entries, .. } => {
                for (k, v) in entries {
                    (self.mark)(v, self.out);
                    self.expr(k);
                    self.expr(v);
                }
            }
            Expr::EnumConstruct { fields, .. } => {
                for x in fields {
                    (self.mark)(x, self.out);
                    self.expr(x);
                }
            }
            Expr::Lambda { body, .. } => self.expr(body),
            Expr::Block { stmts, tail } => {
                self.stmts(stmts);
                if let Some(t) = tail {
                    self.expr(t);
                }
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                // A formal that is matched on may be bound and invoked in an arm; be conservative.
                (self.mark)(scrutinee, self.out);
                self.expr(scrutinee);
                for a in arms {
                    if let Some(g) = &a.guard {
                        self.expr(g);
                    }
                    self.expr(&a.body);
                }
            }
            Expr::IfLet {
                scrutinee,
                then,
                else_,
                ..
            } => {
                (self.mark)(scrutinee, self.out);
                self.expr(scrutinee);
                self.expr(then);
                self.expr(else_);
            }
            _ => {
                let mut kids: Vec<Expr> = Vec::new();
                for_each_child_expr(e, &mut |c| kids.push(c.clone()));
                for c in &kids {
                    self.expr(c);
                }
            }
        }
    }

    /// Mark the root variable of an access path used in callee position.
    fn mark_access_root(&mut self, e: &Expr) {
        match e {
            Expr::Var(_) => (self.mark)(e, self.out),
            Expr::Index { base, .. } | Expr::FieldAccess { base, .. } => {
                self.mark_access_root(base)
            }
            _ => (self.mark)(e, self.out),
        }
    }

    fn roots_nonempty(&self, n: &str) -> bool {
        !(self.roots)(n).is_empty()
    }

    /// Recurse the data sub-parts of a place target without marking (an index base / field base read).
    fn data_children(&mut self, e: &Expr) {
        let mut kids: Vec<Expr> = Vec::new();
        for_each_child_expr(e, &mut |c| kids.push(c.clone()));
        for c in &kids {
            self.expr(c);
        }
    }
}

/// Add the ROOT variable of any receiver mutated through method-call syntax `recv.push/pop/insert/remove(..)`
/// to `out` (the free-function forms are covered by `collect_assigned_roots`).
fn collect_method_mutated_roots(body: &[Stmt], out: &mut BTreeSet<String>) {
    fn root_of(e: &Expr) -> Option<String> {
        match e {
            Expr::Var(n) => Some(n.clone()),
            Expr::Index { base, .. } | Expr::FieldAccess { base, .. } => root_of(base),
            _ => None,
        }
    }
    fn walk_expr(e: &Expr, out: &mut BTreeSet<String>) {
        if let Expr::CallExpr { callee, .. } = e {
            if let Expr::FieldAccess { base, field, .. } = callee.as_ref() {
                if matches!(field.as_str(), "push" | "pop" | "insert" | "remove") {
                    if let Some(r) = root_of(base) {
                        out.insert(r);
                    }
                }
            }
        }
        for_each_child_expr(e, &mut |c| walk_expr(c, out));
    }
    fn walk_stmt(st: &Stmt, out: &mut BTreeSet<String>) {
        match st {
            Stmt::Let { init, .. } | Stmt::LetPattern { init, .. } => walk_expr(init, out),
            Stmt::Assign { target, value } => {
                walk_expr(target, out);
                walk_expr(value, out);
            }
            Stmt::ExprStmt(e) => walk_expr(e, out),
            Stmt::If { cond, then, else_ } => {
                walk_expr(cond, out);
                then.iter().for_each(|s| walk_stmt(s, out));
                if let Some(e) = else_ {
                    e.iter().for_each(|s| walk_stmt(s, out));
                }
            }
            Stmt::While { cond, body, .. } => {
                walk_expr(cond, out);
                body.iter().for_each(|s| walk_stmt(s, out));
            }
            Stmt::WhileLet { expr, body, .. } => {
                walk_expr(expr, out);
                body.iter().for_each(|s| walk_stmt(s, out));
            }
            Stmt::For { source, body, .. } => {
                match source {
                    ForSource::Range { start, end } => {
                        walk_expr(start, out);
                        walk_expr(end, out);
                    }
                    ForSource::Collection { expr } => walk_expr(expr, out),
                }
                body.iter().for_each(|s| walk_stmt(s, out));
            }
            Stmt::Loop { body, .. }
            | Stmt::ResearchBlock { body, .. }
            | Stmt::ExploitBlock { body, .. } => body.iter().for_each(|s| walk_stmt(s, out)),
            Stmt::HybridBlock { gpu, cpu, prove } => {
                for b in [gpu, cpu, prove].into_iter().flatten() {
                    b.iter().for_each(|s| walk_stmt(s, out));
                }
            }
            Stmt::Break | Stmt::Continue | Stmt::SpecBlock { .. } => {}
        }
    }
    body.iter().for_each(|s| walk_stmt(s, out));
}
