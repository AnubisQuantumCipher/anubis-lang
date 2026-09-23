//! Total, read-only traversal of the syntax tree.
//!
//! The analysis layer grew many hand-written walkers that must each handle every construct, and a
//! construct one walker forgot (a match arm, a lambda body, a value block) has repeatedly been a
//! soundness hole. These helpers visit EVERY expression reachable from an item, statement or
//! expression. Every `match` below is exhaustive with no wildcard arm, so adding a variant to
//! `Expr`, `Stmt`, `ForSource` or `Item` fails to compile until it is classified here.
//!
//! Visiting order is pre-order (a node before its children). Patterns carry no expressions and are
//! not visited.

use crate::frontend::{Expr, ForSource, Item, Stmt};

/// Call `f` on `e` and on every expression nested inside it, including expressions inside nested
/// statements (blocks, match arms, lambda bodies).
pub(crate) fn each_expr<'a>(e: &'a Expr, f: &mut dyn FnMut(&'a Expr)) {
    f(e);
    match e {
        Expr::Var(_)
        | Expr::Literal(_)
        | Expr::StrLiteral(_)
        | Expr::Symbolic { .. }
        | Expr::TaintSource { .. }
        | Expr::UnifiedBuffer { .. }
        | Expr::RawPtr { .. }
        | Expr::Other(_) => {}
        Expr::Call { args, .. } => {
            for a in args {
                each_expr(a, f);
            }
        }
        Expr::CallExpr { callee, args } => {
            each_expr(callee, f);
            for a in args {
                each_expr(a, f);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            each_expr(lhs, f);
            each_expr(rhs, f);
        }
        Expr::Unary { expr, .. }
        | Expr::Cast { expr, .. }
        | Expr::Tainted { inner: expr, .. }
        | Expr::Assume(expr)
        | Expr::Assert(expr)
        | Expr::Declassify { inner: expr, .. }
        | Expr::Try(expr) => each_expr(expr, f),
        Expr::ArrayLiteral { elements } => {
            for x in elements {
                each_expr(x, f);
            }
        }
        Expr::Index { base, index } => {
            each_expr(base, f);
            each_expr(index, f);
        }
        Expr::StructLiteral { fields, .. } => {
            for (_, x) in fields {
                each_expr(x, f);
            }
        }
        Expr::FieldAccess { base, .. } => each_expr(base, f),
        Expr::EnumConstruct { fields, .. } => {
            for x in fields {
                each_expr(x, f);
            }
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            each_expr(scrutinee, f);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    each_expr(g, f);
                }
                each_expr(&arm.body, f);
            }
        }
        Expr::If {
            cond, then, else_, ..
        } => {
            each_expr(cond, f);
            each_expr(then, f);
            each_expr(else_, f);
        }
        Expr::MapLiteral { entries, .. } => {
            for (k, v) in entries {
                each_expr(k, f);
                each_expr(v, f);
            }
        }
        Expr::Block { stmts, tail } => {
            each_expr_in_stmts(stmts, f);
            if let Some(t) = tail {
                each_expr(t, f);
            }
        }
        Expr::Lambda { body, .. } => each_expr(body, f),
        Expr::IfLet {
            scrutinee,
            then,
            else_,
            ..
        } => {
            each_expr(scrutinee, f);
            each_expr(then, f);
            each_expr(else_, f);
        }
    }
}

/// Call `f` on every expression in `stmts`, recursively.
pub(crate) fn each_expr_in_stmts<'a>(stmts: &'a [Stmt], f: &mut dyn FnMut(&'a Expr)) {
    for s in stmts {
        each_expr_in_stmt(s, f);
    }
}

/// Call `f` on every expression in `s`, recursively.
pub(crate) fn each_expr_in_stmt<'a>(s: &'a Stmt, f: &mut dyn FnMut(&'a Expr)) {
    match s {
        Stmt::Let { init, .. } | Stmt::LetPattern { init, .. } => each_expr(init, f),
        Stmt::WhileLet { expr, body, .. } => {
            each_expr(expr, f);
            each_expr_in_stmts(body, f);
        }
        Stmt::Assign { target, value } => {
            each_expr(target, f);
            each_expr(value, f);
        }
        Stmt::If { cond, then, else_ } => {
            each_expr(cond, f);
            each_expr_in_stmts(then, f);
            if let Some(e) = else_ {
                each_expr_in_stmts(e, f);
            }
        }
        Stmt::While {
            cond,
            body,
            invariant,
        } => {
            each_expr(cond, f);
            for i in invariant {
                each_expr(i, f);
            }
            each_expr_in_stmts(body, f);
        }
        Stmt::Loop { body, invariant } => {
            for i in invariant {
                each_expr(i, f);
            }
            each_expr_in_stmts(body, f);
        }
        Stmt::For {
            source,
            body,
            invariant,
            ..
        } => {
            match source {
                ForSource::Range { start, end } => {
                    each_expr(start, f);
                    each_expr(end, f);
                }
                ForSource::Collection { expr } => each_expr(expr, f),
            }
            for i in invariant {
                each_expr(i, f);
            }
            each_expr_in_stmts(body, f);
        }
        Stmt::ResearchBlock { body, .. } | Stmt::ExploitBlock { body, .. } => {
            each_expr_in_stmts(body, f)
        }
        Stmt::HybridBlock { gpu, cpu, prove } => {
            for b in [gpu, cpu, prove].into_iter().flatten() {
                each_expr_in_stmts(b, f);
            }
        }
        Stmt::ExprStmt(e) => each_expr(e, f),
        Stmt::Break | Stmt::Continue | Stmt::SpecBlock { .. } => {}
    }
}

/// Call `f` on every function item in `items`: free functions, functions inside modules, impl
/// methods and trait (default) methods.
pub(crate) fn each_fn_item<'a>(items: &'a [Item], f: &mut dyn FnMut(&'a Item)) {
    for item in items {
        match item {
            Item::Fn { .. } => f(item),
            Item::Module { items, .. } => each_fn_item(items, f),
            Item::Impl { methods, .. } | Item::Trait { methods, .. } => each_fn_item(methods, f),
            Item::Import { .. } | Item::Struct { .. } | Item::Enum { .. } => {}
        }
    }
}

/// Call `f` on every expression anywhere in `items`: every function's body and its `requires` /
/// `ensures` clauses.
pub(crate) fn each_expr_in_items<'a>(items: &'a [Item], f: &mut dyn FnMut(&'a Expr)) {
    each_fn_item(items, &mut |item| {
        if let Item::Fn {
            body,
            requires,
            ensures,
            ..
        } = item
        {
            for c in requires.iter().chain(ensures) {
                each_expr(c, f);
            }
            each_expr_in_stmts(body, f);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::parse_source;

    fn vars_in(src: &str) -> Vec<String> {
        let ast = parse_source(src).expect("parse");
        let mut out = Vec::new();
        each_expr_in_items(&ast.items, &mut |e| {
            if let Expr::Var(v) = e {
                out.push(v.clone());
            }
        });
        out
    }

    /// Every construct that can hold an expression is reached: a name placed in each position must
    /// be seen.
    #[test]
    fn reaches_every_position() {
        let src = r#"
struct S { a: i64 }
fn f(x: i64) -> i64 requires(r0 > 0) ensures(r1 > 0) { return x; }
fn main() {
    let a = v1;
    let [p, q] = v2;
    a = v3;
    if v4 { print(v5); } else { print(v6); }
    while v7 { print(v8); }
    for i in v9..v10 { print(v11); }
    for e in v12 { print(v13); }
    loop { print(v14); break; }
    let m = match v15 { 1 if v16 => v17, _ => v18 };
    let g = |z| v19;
    let b = if v20 { v21 } else { v22 };
    let t = if v36 { let u = v23; v24 } else { 0 };
    let s = S { a: v25 };
    let c = v26.a;
    let d = v27[v28];
    let k = [v29];
    let w = (v30)(v31);
    if let 1 = v32 { print(v33); }
    while let 1 = v34 { print(v35); }
}
"#;
        let seen = vars_in(src);
        for i in 1..=36 {
            let want = format!("v{i}");
            assert!(seen.contains(&want), "missed {want}; saw {seen:?}");
        }
        assert!(seen.contains(&"r0".to_string()) && seen.contains(&"r1".to_string()));
    }
}
