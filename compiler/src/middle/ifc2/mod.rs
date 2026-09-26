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
mod repeatable;
mod value;

use crate::frontend::Item;
use std::collections::BTreeMap;

pub(crate) use eval::Finding;

/// Analyze a Safe-mode program; every information flow found.
pub(crate) fn check(items: &[Item]) -> Vec<Finding> {
    let mut it = eval::Interp::new(items);
    it.run()
}

/// Ephemeral addresses identify the same free-function Items before and after the D9 pass mutates
/// parameter annotations in place. They are never serialized or used as program identities.
/// A missing address later prevents repeatability admission rather than assuming a raw signature.
pub(crate) type OriginalUnannotatedParams = BTreeMap<usize, Vec<bool>>;

pub(crate) fn original_unannotated_params(items: &[Item]) -> OriginalUnannotatedParams {
    fn visit(items: &[Item], out: &mut OriginalUnannotatedParams) {
        for item in items {
            match item {
                Item::Module { items, .. } => visit(items, out),
                Item::Fn { params, .. } => {
                    out.insert(
                        item as *const Item as usize,
                        params.iter().map(|(_, ty)| ty.is_empty()).collect(),
                    );
                }
                Item::Import { .. }
                | Item::Struct { .. }
                | Item::Enum { .. }
                | Item::Impl { .. }
                | Item::Trait { .. } => {}
            }
        }
    }
    let mut out = BTreeMap::new();
    visit(items, &mut out);
    out
}

pub(crate) fn check_with_original_params(
    items: &[Item],
    original: &OriginalUnannotatedParams,
) -> Vec<Finding> {
    let mut it = eval::Interp::new_with_original_params(items, Some(original));
    it.run()
}
