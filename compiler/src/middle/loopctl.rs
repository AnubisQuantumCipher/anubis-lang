//! Loop-control placement: `break`/`continue` with no loop to break out of.
//!
//! # What this closes
//!
//! The builtin-surface matrix graded every name by the PAIR (`check`, `run`). Two cells were
//! neither proof-lane constructs nor arity mistakes — they were plain misplacement:
//!
//! ```text
//! fn main() { break; }      check = 0    run = 1  ANUBIS_UNSUPPORTED_NATIVE_LOWERING
//! fn main() { continue; }   check = 0    run = 1  ANUBIS_UNSUPPORTED_NATIVE_LOWERING
//! ```
//!
//! The runtime refused, so nothing unsound happened and the published promise held. But `check`
//! accepted a program that cannot run, and it is a static property — there is no loop, and no input
//! makes one appear. Deciding it at runtime is deciding it in the one place the answer is useless.
//!
//! # Why it is a separate module with a TOTAL match
//!
//! `carrier.rs` forces every `Expr` variant to state what it can carry. Nothing did that for `Stmt`,
//! so a new statement form carrying a body — the eleventh block construct — would silently become a
//! place `break` could hide. [`unenclosed_loop_control`] matches all 15 `Stmt` variants with **no
//! wildcard arm**: adding one fails to compile here until someone says whether it encloses a loop.
//!
//! # Deliberately conservative
//!
//! A `break` inside a lambda is NOT reported. Whether a closure body may break its defining loop is
//! a real language-design question this module does not get to answer by accident, and guessing
//! wrong would reject working programs. Silence here costs a runtime refusal that already exists;
//! guessing costs a false rejection, which is worse.

use crate::frontend::Stmt;

/// Which loop-control keywords appear with no enclosing loop.
///
/// Returns the offending keyword names, deduplicated and in source order of first appearance.
pub fn unenclosed_loop_control(body: &[Stmt]) -> Vec<&'static str> {
    let mut found = Vec::new();
    scan(body, &mut found);
    found
}

fn note(found: &mut Vec<&'static str>, kw: &'static str) {
    if !found.contains(&kw) {
        found.push(kw);
    }
}

/// Walk statements that are NOT inside a loop. Descending into a loop body simply stops — anything
/// under it is legally enclosed, so there is nothing further to report down that branch.
fn scan(stmts: &[Stmt], found: &mut Vec<&'static str>) {
    for s in stmts {
        match s {
            // The two we are looking for: reached here means no loop enclosed them.
            Stmt::Break => note(found, "break"),
            Stmt::Continue => note(found, "continue"),

            // Loop forms — everything inside is enclosed. Do not descend.
            Stmt::While { .. } => {}
            Stmt::Loop { .. } => {}
            Stmt::For { .. } => {}
            Stmt::WhileLet { .. } => {}

            // Blocks that carry statements WITHOUT introducing a loop: a `break` inside one of
            // these is still unenclosed, so the scan continues through them.
            Stmt::If { then, else_, .. } => {
                scan(then, found);
                if let Some(e) = else_ {
                    scan(e, found);
                }
            }
            Stmt::ResearchBlock { body, .. } => scan(body, found),
            Stmt::ExploitBlock { body, .. } => scan(body, found),
            Stmt::HybridBlock { gpu, cpu, prove } => {
                for arm in [gpu, cpu, prove].into_iter().flatten() {
                    scan(arm, found);
                }
            }

            // Statements that cannot contain a statement body. `ExprStmt` can hold a `match`/`if`
            // expression whose arms hold statements, but loop control does not reach through an
            // expression arm in this AST — those arms are `Expr`, not `Vec<Stmt>`.
            Stmt::Let { .. } => {}
            Stmt::LetPattern { .. } => {}
            Stmt::Assign { .. } => {}
            Stmt::SpecBlock { .. } => {}
            Stmt::ExprStmt(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::{Expr, Span};

    fn lit() -> Expr {
        Expr::Literal("1".into())
    }

    #[test]
    fn bare_break_and_continue_are_reported() {
        assert_eq!(unenclosed_loop_control(&[Stmt::Break]), vec!["break"]);
        assert_eq!(unenclosed_loop_control(&[Stmt::Continue]), vec!["continue"]);
    }

    #[test]
    fn inside_a_loop_is_silent() {
        // The whole point of the check is that it must not fire on correct programs. Every loop
        // form is pinned, because adding a form and forgetting it here would reject working code.
        let w = Stmt::While {
            cond: lit(),
            body: vec![Stmt::Break, Stmt::Continue],
            invariant: vec![],
        };
        assert!(unenclosed_loop_control(&[w]).is_empty());

        let l = Stmt::Loop {
            body: vec![Stmt::Break],
            invariant: vec![],
        };
        assert!(unenclosed_loop_control(&[l]).is_empty());

        let wl = Stmt::WhileLet {
            pattern: crate::frontend::Pattern::Wildcard,
            expr: lit(),
            body: vec![Stmt::Continue],
        };
        assert!(unenclosed_loop_control(&[wl]).is_empty());
    }

    #[test]
    fn a_break_nested_in_an_if_outside_any_loop_is_still_reported() {
        // The shape that makes this worth a walker rather than a top-level glance.
        let s = Stmt::If {
            cond: lit(),
            then: vec![Stmt::Break],
            else_: Some(vec![Stmt::Continue]),
        };
        assert_eq!(unenclosed_loop_control(&[s]), vec!["break", "continue"]);
    }

    #[test]
    fn an_if_inside_a_loop_is_silent() {
        // The false-rejection direction of the same nesting, pinned separately: `if` descends, but
        // only until a loop encloses it.
        let inner = Stmt::If {
            cond: lit(),
            then: vec![Stmt::Break],
            else_: None,
        };
        let outer = Stmt::While {
            cond: lit(),
            body: vec![inner],
            invariant: vec![],
        };
        assert!(unenclosed_loop_control(&[outer]).is_empty());
    }

    #[test]
    fn a_let_does_not_hide_a_break() {
        // Guards the "statements after a non-block statement still get scanned" property.
        let s = vec![
            Stmt::Let {
                name: "x".into(),
                ty: None,
                init: lit(),
                span: Span { start: 0, end: 0 },
            },
            Stmt::Break,
        ];
        assert_eq!(unenclosed_loop_control(&s), vec!["break"]);
    }
}
