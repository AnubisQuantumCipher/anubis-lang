//! Does a block definitely NOT fall through?
//!
//! # Why this exists
//!
//! Wrap-safety analyses each statement of a function body under the accumulated assumptions, and an
//! **early-return guard used to contribute nothing to the statements after it**:
//!
//! ```text
//! fn abs_i(x: i64) -> i64 ensures(result >= 0) {
//!     if x == i64::MIN { return i64::MAX; }   // knowledge discarded here
//!     if x < 0 { return 0 - x; }              // analysed WITHOUT knowing x != i64::MIN
//!     return x;
//! }
//! ```
//!
//! The second statement can only execute when the first guard was FALSE, so `x != i64::MIN` holds
//! there — but the walker passed the same assumption set to every statement, and reported
//! `ANUBIS_WRAP_RISK` with counterexample `x = i64::MIN` on a negation the source had already
//! guarded. A checker that cannot read a guard the programmer wrote teaches people to delete the
//! guard, or to reach for `ANUBIS_WRAP_SAFETY=0`.
//!
//! [`block_diverges`] answers the one question that makes the refinement sound: if the `then` arm
//! cannot fall through, then everything after the `if` runs only when the condition was false.
//!
//! # Conservative direction
//!
//! `false` is always safe: it adds no fact, and the analysis stays exactly as strong as it was.
//! `true` asserts that control cannot continue, and a wrong `true` would inject an assumption that
//! does not hold — turning a real wrap into a proof. So every uncertain case answers `false`, and
//! the loop forms answer `false` even when a human can see the loop never exits.
//!
//! The match over `Stmt` is TOTAL with **no wildcard arm**, the same discipline `carrier.rs` applies
//! to `Expr` and `loopctl.rs` to statements: a new statement variant fails to compile here until
//! someone states whether it can fall through. A wildcard would default new constructs to `false`,
//! which is safe — but it would also silently keep them from ever refining, and the next person
//! would have no signal that the question was asked.

use crate::frontend::{Expr, Stmt};

/// Whether a block definitely does not fall through to the statement after it.
///
/// If ANY statement in the block diverges, the block diverges — statements past it are dead.
pub fn block_diverges(stmts: &[Stmt]) -> bool {
    stmts.iter().any(stmt_diverges)
}

fn stmt_diverges(s: &Stmt) -> bool {
    match s {
        // The unconditional exits.
        Stmt::Break => true,
        Stmt::Continue => true,

        // `return` is not a statement in this AST — the parser lowers it to a call with the callee
        // literally named `return` (frontend/mod.rs:3323, :3775).
        Stmt::ExprStmt(e) => expr_diverges(e),

        // An `if` diverges only when EVERY path through it diverges, which requires an else arm.
        // With no else, the false path falls straight through.
        Stmt::If { then, else_, .. } => {
            block_diverges(then) && else_.as_ref().is_some_and(|e| block_diverges(e))
        }

        // Blocks that carry statements without changing control flow around them.
        Stmt::ResearchBlock { body, .. } => block_diverges(body),
        Stmt::ExploitBlock { body, .. } => block_diverges(body),

        // Loops answer `false` even when a human can see they never exit. `loop {}` with no `break`
        // does diverge, but proving that here means proving the absence of a reachable `break`
        // through arbitrarily nested bodies, and getting it wrong injects a false assumption. The
        // cost of `false` is only a missed refinement.
        Stmt::While { .. } => false,
        Stmt::For { .. } => false,
        Stmt::WhileLet { .. } => false,
        Stmt::Loop { .. } => false,

        // A hybrid block picks one of gpu/cpu/prove at runtime; "all arms diverge" is not a
        // property this analysis needs, and guessing it buys nothing.
        Stmt::HybridBlock { .. } => false,

        // Cannot transfer control.
        Stmt::Let { .. } => false,
        Stmt::LetPattern { .. } => false,
        Stmt::Assign { .. } => false,
        Stmt::SpecBlock { .. } => false,
    }
}

fn expr_diverges(e: &Expr) -> bool {
    matches!(e, Expr::Call { callee, .. } if callee == "return")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::Span;

    fn ret() -> Stmt {
        Stmt::ExprStmt(Expr::Call {
            callee: "return".into(),
            args: vec![Expr::Literal("1".into())],
        })
    }
    fn lit() -> Expr {
        Expr::Literal("1".into())
    }
    fn let_x() -> Stmt {
        Stmt::Let {
            name: "x".into(),
            ty: None,
            init: lit(),
            span: Span { start: 0, end: 0 },
        }
    }

    #[test]
    fn a_return_diverges_and_a_let_does_not() {
        assert!(block_diverges(&[ret()]));
        assert!(!block_diverges(&[let_x()]));
        assert!(block_diverges(&[let_x(), ret()]));
    }

    #[test]
    fn break_and_continue_diverge() {
        assert!(block_diverges(&[Stmt::Break]));
        assert!(block_diverges(&[Stmt::Continue]));
    }

    #[test]
    fn an_if_needs_both_arms_to_diverge() {
        // The exact shape the wrap-safety refinement depends on: a guard whose THEN returns but
        // which has no else does NOT itself diverge — control falls through when the guard is
        // false, which is precisely the case the refinement exploits.
        let guard_no_else = Stmt::If {
            cond: lit(),
            then: vec![ret()],
            else_: None,
        };
        assert!(!block_diverges(&[guard_no_else]));

        let both = Stmt::If {
            cond: lit(),
            then: vec![ret()],
            else_: Some(vec![ret()]),
        };
        assert!(block_diverges(&[both]));

        let one_arm = Stmt::If {
            cond: lit(),
            then: vec![ret()],
            else_: Some(vec![let_x()]),
        };
        assert!(!block_diverges(&[one_arm]));
    }

    #[test]
    fn loops_are_conservatively_non_diverging() {
        // `loop { }` with no break does diverge, but answering `true` here would inject an
        // assumption on a path the analysis has not proven unreachable. Pinned so a future
        // "improvement" has to argue with a test.
        let l = Stmt::Loop {
            body: vec![],
            invariant: vec![],
        };
        assert!(!block_diverges(&[l]));

        let w = Stmt::While {
            cond: lit(),
            body: vec![ret()],
            invariant: vec![],
        };
        assert!(!block_diverges(&[w]));
    }
}
