//! Phase-2 final slice: lethal-trifecta detection (the body-scan half).
//!
//! The "lethal trifecta" is the AI-agent exfiltration condition: a function that (1) reads PRIVATE
//! data, (2) is exposed to UNTRUSTED input, and (3) can COMMUNICATE EXTERNALLY, all at once. An
//! injection in the untrusted input can then steer the private read and the egress — even when no
//! literal value flows from the read to the send, which is exactly the case the value-flow taint
//! check (`ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY`) cannot see.
//!
//! Legs 1 (fs.read) and 3 (net.send) are read off the CLOSED transitive effect row in `mod.rs`
//! (open rows are already rejected in verified mode). THIS module supplies the two body-scanned
//! signals: leg 2 (an untrusted source DISTINCT from the private file read — the attacker's steering
//! channel must be a different channel than the data it steers) and the escape hatch (a WELL-FORMED
//! `declassify(v, policy, reason)` present in the body). Presence scan only — no flow, no state.
//!
//! Completeness matters asymmetrically: missing a leg-2 source only fails to fire (accept-biased,
//! safe), but missing a well-formed declassify would OVER-reject — so the walk visits every node.
//! Self-contained and pure, mirroring effects.rs/capability.rs (a Phase-4 Anubis-port candidate).

use crate::frontend::{Expr, ForSource, Stmt};

#[derive(Debug, Default)]
pub(crate) struct TrifectaLegs {
    /// Label of the first untrusted source that is NOT the private file read (`input`, `recv`,
    /// `env`, a `taint_source(..)`, or a `tainted<T>` parameter). `None` = no leg-2 channel found.
    pub leg2_untrusted: Option<String>,
    /// A well-formed `declassify(inner, policy, reason)` (both policy and reason present) appears
    /// somewhere in the body — the author's explicit, reviewed sanitization barrier.
    pub wellformed_declassify: bool,
}

/// Scan one function body + its parameters for the two body-side trifecta signals.
pub(crate) fn scan_legs(body: &[Stmt], params: &[(String, String)]) -> TrifectaLegs {
    let mut legs = TrifectaLegs::default();
    // A `tainted<T>` parameter is untrusted input arriving directly as an argument — a leg-2 channel.
    for (pname, pty) in params {
        if super::is_tainted_type(Some(pty)) {
            legs.leg2_untrusted
                .get_or_insert_with(|| format!("tainted parameter `{pname}`"));
        }
    }
    walk_stmts(body, &mut legs);
    legs
}

/// Whether a bare-name call is an untrusted taint source OTHER than the private file read.
/// `read_file`/`open` are leg 1 (private data), never leg 2 — the steering channel must be distinct.
fn is_leg2_source(callee: &str) -> bool {
    super::is_io_taint_source(callee) && callee != "read_file" && callee != "open"
}

fn walk_stmts(stmts: &[Stmt], legs: &mut TrifectaLegs) {
    for s in stmts {
        walk_stmt(s, legs);
    }
}

fn walk_stmt(stmt: &Stmt, legs: &mut TrifectaLegs) {
    match stmt {
        Stmt::Let { init, .. } => walk_expr(init, legs),
        Stmt::LetPattern { init, .. } => walk_expr(init, legs),
        Stmt::WhileLet { expr, body, .. } => {
            walk_expr(expr, legs);
            walk_stmts(body, legs);
        }
        Stmt::Assign { target, value } => {
            walk_expr(target, legs);
            walk_expr(value, legs);
        }
        Stmt::If { cond, then, else_ } => {
            walk_expr(cond, legs);
            walk_stmts(then, legs);
            if let Some(e) = else_ {
                walk_stmts(e, legs);
            }
        }
        Stmt::While {
            cond,
            body,
            invariant,
        } => {
            walk_expr(cond, legs);
            for inv in invariant {
                walk_expr(inv, legs);
            }
            walk_stmts(body, legs);
        }
        Stmt::Loop { body, invariant } => {
            for inv in invariant {
                walk_expr(inv, legs);
            }
            walk_stmts(body, legs);
        }
        Stmt::For {
            source,
            body,
            invariant,
            ..
        } => {
            match source {
                ForSource::Range { start, end } => {
                    walk_expr(start, legs);
                    walk_expr(end, legs);
                }
                ForSource::Collection { expr } => walk_expr(expr, legs),
            }
            for inv in invariant {
                walk_expr(inv, legs);
            }
            walk_stmts(body, legs);
        }
        Stmt::ResearchBlock { body, .. } | Stmt::ExploitBlock { body, .. } => walk_stmts(body, legs),
        Stmt::HybridBlock { gpu, cpu, prove } => {
            for b in [gpu, cpu, prove].into_iter().flatten() {
                walk_stmts(b, legs);
            }
        }
        Stmt::Break | Stmt::Continue | Stmt::SpecBlock { .. } => {}
        Stmt::ExprStmt(e) => walk_expr(e, legs),
    }
}

fn walk_expr(expr: &Expr, legs: &mut TrifectaLegs) {
    match expr {
        Expr::Var(_)
        | Expr::Literal(_)
        | Expr::StrLiteral(_)
        | Expr::Symbolic { .. }
        | Expr::UnifiedBuffer { .. }
        | Expr::RawPtr { .. }
        | Expr::Other(_) => {}
        Expr::TaintSource { label } => {
            legs.leg2_untrusted
                .get_or_insert_with(|| format!("taint_source(\"{label}\")"));
        }
        Expr::Call { callee, args } => {
            if is_leg2_source(callee) {
                legs.leg2_untrusted.get_or_insert_with(|| callee.clone());
            }
            for a in args {
                walk_expr(a, legs);
            }
        }
        Expr::CallExpr { callee, args } => {
            walk_expr(callee, legs);
            for a in args {
                walk_expr(a, legs);
            }
        }
        Expr::Declassify {
            inner,
            policy,
            reason,
        } => {
            // The escape hatch: a WELL-FORMED declassify (both policy AND reason) is the author's
            // reviewed sanitization barrier. A malformed `declassify(x)` does NOT discharge — that
            // was the forge the adversarial review caught (the "declassify" effect tag is pushed
            // even for malformed ones, so we must inspect the AST shape, not the tag).
            if policy.is_some() && reason.is_some() {
                legs.wellformed_declassify = true;
            }
            walk_expr(inner, legs);
        }
        Expr::Tainted { inner, .. } => walk_expr(inner, legs),
        Expr::Assume(inner) | Expr::Assert(inner) | Expr::Try(inner) => walk_expr(inner, legs),
        Expr::Unary { expr, .. } | Expr::Cast { expr, .. } => walk_expr(expr, legs),
        Expr::Binary { lhs, rhs, .. } => {
            walk_expr(lhs, legs);
            walk_expr(rhs, legs);
        }
        Expr::ArrayLiteral { elements } => {
            for e in elements {
                walk_expr(e, legs);
            }
        }
        Expr::Index { base, index } => {
            walk_expr(base, legs);
            walk_expr(index, legs);
        }
        Expr::StructLiteral { fields, .. } => {
            for (_, e) in fields {
                walk_expr(e, legs);
            }
        }
        Expr::FieldAccess { base, .. } => walk_expr(base, legs),
        Expr::EnumConstruct { fields, .. } => {
            for e in fields {
                walk_expr(e, legs);
            }
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            walk_expr(scrutinee, legs);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    walk_expr(g, legs);
                }
                walk_expr(&arm.body, legs);
            }
        }
        Expr::If {
            cond, then, else_, ..
        } => {
            walk_expr(cond, legs);
            walk_expr(then, legs);
            walk_expr(else_, legs);
        }
        Expr::IfLet {
            scrutinee,
            then,
            else_,
            ..
        } => {
            walk_expr(scrutinee, legs);
            walk_expr(then, legs);
            walk_expr(else_, legs);
        }
        Expr::MapLiteral { entries, .. } => {
            for (k, v) in entries {
                walk_expr(k, legs);
                walk_expr(v, legs);
            }
        }
        Expr::Block { stmts, tail } => {
            walk_stmts(stmts, legs);
            if let Some(t) = tail {
                walk_expr(t, legs);
            }
        }
        Expr::Lambda { body, .. } => walk_expr(body, legs),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend;

    fn legs_of(src: &str) -> TrifectaLegs {
        let ast = frontend::parse_source(src).expect("parse");
        for item in &ast.items {
            if let frontend::Item::Fn {
                name, params, body, ..
            } = item
            {
                if name == "agent" {
                    return scan_legs(body, params);
                }
            }
        }
        panic!("no `agent` fn");
    }

    #[test]
    fn detects_distinct_untrusted_source_not_the_read() {
        let legs = legs_of(r#"fn agent() { let s = input(); let d = read_file("x"); send("h", 80, "b"); }"#);
        assert_eq!(legs.leg2_untrusted.as_deref(), Some("input"));
    }

    #[test]
    fn file_read_alone_is_not_leg2() {
        // read_file/open are leg 1 (private data), never the leg-2 steering channel.
        let legs = legs_of(r#"fn agent() { let d = read_file("x"); send("h", 80, d); }"#);
        assert!(legs.leg2_untrusted.is_none());
    }

    #[test]
    fn tainted_param_is_leg2() {
        let legs = legs_of(r#"fn agent(q: tainted<string>) { let d = read_file("x"); }"#);
        assert!(legs.leg2_untrusted.as_deref().unwrap().contains("tainted parameter"));
    }

    #[test]
    fn wellformed_declassify_detected_malformed_ignored() {
        assert!(legs_of(r#"fn agent() { let s = input(); let x = declassify(s, "p", "r"); }"#).wellformed_declassify);
        assert!(!legs_of(r#"fn agent() { let s = input(); let x = declassify(s); }"#).wellformed_declassify);
    }

    #[test]
    fn scans_into_nested_branches_and_blocks() {
        let legs = legs_of(r#"fn agent(c: bool) { if c { let s = recv(); } else { let x = declassify(read_file("x"), "p", "r"); } }"#);
        assert_eq!(legs.leg2_untrusted.as_deref(), Some("recv"));
        assert!(legs.wellformed_declassify);
    }
}
