//! IFC v2: an information-flow interpreter that mirrors the runtime.
//!
//! The checker's other information-flow lanes (the ordinary lane in `middle/mod.rs`, the
//! whole-struct lane in `whole.rs`) reason about names and syntax. This one evaluates the program
//! the way `backends/run.rs` lowers it, over abstract values that carry security labels
//! ([`value`]): pure value semantics, closures that snapshot their captures at creation, the
//! runtime's call-resolution order, and every builtin's behaviour ([`builtins`]). It reports a
//! secret reaching an egress, an egress run under a condition a secret decides (decision D1 of
//! `docs/mission/DELEGATED_DECISIONS_2026-09-25.md`), and untrusted data reaching an integrity
//! sink. Its answer is sound by construction only where each piece mirrors the runtime; the
//! soundness matrix and independent review are what establish that.
//!
//! It runs in every Safe-mode check beside the other lanes (a program is refused if any lane finds
//! a flow); `anubis ifc2-report` shows its findings alone, for measurement.

mod builtins;
mod eval;
mod value;

use crate::frontend::Item;

pub(crate) use eval::Finding;

/// Analyze a Safe-mode program; every information flow found.
pub(crate) fn check(items: &[Item]) -> Vec<Finding> {
    let mut it = eval::Interp::new(items);
    it.run()
}
