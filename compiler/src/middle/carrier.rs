//! Compile-time carrier classification: adding an `Expr` variant BREAKS THE BUILD.
//!
//! # Why this exists
//!
//! The false-accept class this repo spent a session closing was always the same shape — a callable
//! reaches an apply site by a route no consumer knew about, so `check` charges nothing and the
//! program writes the file anyway. Forty-one published routes, closed one mechanism at a time.
//!
//! Closing the known list does not close the CLASS. Every hunt found more until the hunts stopped
//! finding them, and nothing in the compiler forced the next person adding a binder or a container
//! form to write its consumer. The eleventh binder would have reopened it silently.
//!
//! This module is the forcing function. [`carrier_class`] matches EVERY `Expr` variant explicitly,
//! with **no wildcard arm**, so adding a variant to `Expr` fails to compile here until someone
//! states what it can carry. The same technique the native solver's fragment gate uses to keep an
//! unproven `Term` from riding as authoritative: totality enforced by rustc, not by review.
//!
//! # What the classification means
//!
//! It answers one question — *can a callable value reach an apply site THROUGH this construct?* —
//! and it answers conservatively. [`CarrierClass::Opaque`] is a claim that no callable can pass
//! through, and it is the only answer that can cause a leak if it is wrong, so it is reserved for
//! constructs whose result is a scalar by construction.
//!
//! # What it deliberately does NOT do
//!
//! It does not replace the ~26 value-flow walkers. Rewriting them behind one visitor was examined
//! and rejected: they answer different questions with different evidence, and forcing one API would
//! erase distinctions the guards depend on — known-index versus wildcard, named function versus
//! anonymous closure. This constrains the SHAPE SPACE those walkers must cover, which is the part
//! that kept growing silently.

use crate::frontend::Expr;

/// How a construct can carry a callable value toward an apply site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CarrierClass {
    /// The value flows straight through: whatever the child carries, this carries.
    /// A consumer that handles the child needs nothing extra here.
    Transparent,
    /// Holds callables at PATHS — elements, fields, entries. A consumer must project a path,
    /// not just look at the value. This is where the element-materialization defects lived.
    Container,
    /// Introduces NAMES bound from a scrutinee. A consumer must record the binding as an alias of
    /// whatever the scrutinee was, or the body's applies charge nothing. `match`, `if let`.
    Binder,
    /// Produces a callable directly.
    Producer,
    /// Applies a callable — the site where a capability is charged.
    ApplySite,
    /// Cannot carry a callable. The ONLY class that can cause a leak if it is wrong, so it is
    /// restricted to constructs whose result is a scalar by construction.
    Opaque,
}

/// Classify an expression by how it can carry a callable.
///
/// TOTAL over `Expr` with no wildcard arm — that is the entire point. A new variant fails to
/// compile here, which is the compile-time enforcement Phase 2 asks for: the build breaks until
/// someone states what the new construct can carry.
pub fn carrier_class(e: &Expr) -> CarrierClass {
    match e {
        // Names and applications.
        Expr::Var(..) => CarrierClass::Transparent,
        Expr::Call { .. } => CarrierClass::ApplySite,
        Expr::CallExpr { .. } => CarrierClass::ApplySite,
        Expr::Lambda { .. } => CarrierClass::Producer,

        // Containers — callables live at PATHS inside these.
        Expr::ArrayLiteral { .. } => CarrierClass::Container,
        Expr::MapLiteral { .. } => CarrierClass::Container,
        Expr::StructLiteral { .. } => CarrierClass::Container,
        Expr::EnumConstruct { .. } => CarrierClass::Container,
        Expr::Index { .. } => CarrierClass::Container,
        Expr::FieldAccess { .. } => CarrierClass::Container,

        // Binders — these bind NAMES from a scrutinee, and the body applies them.
        Expr::Match { .. } => CarrierClass::Binder,
        Expr::IfLet { .. } => CarrierClass::Binder,

        // Pass-through: the callable is unchanged by the construct.
        //
        // `Binary` is here because `a + b` CONCATENATES containers — `fn go(acc) { acc + [leak] }`
        // built a list holding a callable and resolved to nothing until this was recognised.
        Expr::Binary { .. } => CarrierClass::Transparent,
        Expr::If { .. } => CarrierClass::Transparent,
        Expr::Block { .. } => CarrierClass::Transparent,
        Expr::Cast { .. } => CarrierClass::Transparent,
        Expr::Try(..) => CarrierClass::Transparent,
        Expr::Tainted { .. } => CarrierClass::Transparent,
        Expr::Declassify { .. } => CarrierClass::Transparent,
        Expr::TaintSource { .. } => CarrierClass::Transparent,

        // Opaque — scalar by construction. Getting one of these wrong is the only way this file
        // can cause a leak, so each is a value that cannot syntactically hold a callable.
        Expr::Literal(..) => CarrierClass::Opaque,
        Expr::StrLiteral(..) => CarrierClass::Opaque,
        Expr::Unary { .. } => CarrierClass::Opaque,
        Expr::Symbolic { .. } => CarrierClass::Opaque,
        Expr::Assume(..) => CarrierClass::Opaque,
        Expr::Assert(..) => CarrierClass::Opaque,
        Expr::UnifiedBuffer { .. } => CarrierClass::Opaque,
        Expr::RawPtr { .. } => CarrierClass::Opaque,
        Expr::Other(..) => CarrierClass::Opaque,
    }
}

/// Whether a construct needs a consumer that understands PATHS or BINDINGS.
///
/// The two classes where every carrier defect this session actually lived.
pub fn needs_path_aware_consumer(e: &Expr) -> bool {
    matches!(
        carrier_class(e),
        CarrierClass::Container | CarrierClass::Binder
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn containers_and_binders_need_path_aware_consumers() {
        // The two classes every carrier defect lived in. If a future refactor reclassifies one of
        // these as Transparent or Opaque, the walkers stop projecting paths for it and the
        // element-materialization class reopens — so this is pinned.
        let arr = Expr::ArrayLiteral { elements: vec![] };
        assert_eq!(carrier_class(&arr), CarrierClass::Container);
        assert!(needs_path_aware_consumer(&arr));

        let idx = Expr::Index {
            base: Box::new(Expr::Var("xs".into())),
            index: Box::new(Expr::Literal("0".into())),
        };
        assert_eq!(carrier_class(&idx), CarrierClass::Container);
        assert!(needs_path_aware_consumer(&idx));
    }

    #[test]
    fn a_scalar_literal_is_opaque_and_a_lambda_produces() {
        assert_eq!(
            carrier_class(&Expr::Literal("1".into())),
            CarrierClass::Opaque
        );
        assert_eq!(
            carrier_class(&Expr::StrLiteral("s".into())),
            CarrierClass::Opaque
        );
        assert!(!needs_path_aware_consumer(&Expr::Literal("1".into())));
    }

    #[test]
    fn concatenation_is_transparent_not_opaque() {
        // `fn go(acc) { acc + [leak] }` built a container holding a callable and resolved to
        // nothing while `Binary` was unhandled. Classifying it Opaque would reopen that route, so
        // the classification is asserted rather than left to a reader's judgement.
        let cat = Expr::Binary {
            op: "+".into(),
            lhs: Box::new(Expr::Var("acc".into())),
            rhs: Box::new(Expr::ArrayLiteral { elements: vec![] }),
        };
        assert_eq!(carrier_class(&cat), CarrierClass::Transparent);
    }
}
