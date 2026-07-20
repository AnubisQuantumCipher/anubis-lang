//! Phase-2 slice 1: transitive effect inference.
//!
//! A function's *transitive* effect row — the canonical capability ids it performs through its own
//! body AND through every user function it (transitively) calls — computed as a monotone fixpoint
//! over the call graph, the same shape as `compute_tainting_fns`. The per-body enforcing check in
//! `middle/mod.rs` sees only direct builtins plus a direct callee's *declared* `uses(...)` caps, so
//! an unclaused helper (`fn helper() { time_now(); }`) launders its effects past every caller; the
//! rows computed here are what close that hole.
//!
//! The row is Koka-style in principle: a closed set of canonical capability ids, or *open* over an
//! unknown tail. `open` is set when the walk reaches a call it cannot resolve to a named user
//! function or an effect-classified builtin — a function-valued parameter, a let-bound closure, a
//! method call (`CallExpr`), or an unknown name. Open-ness widens the *effect set* (the callee may
//! perform anything) but must NEVER widen the *reject decision*: the declared-vs-inferred check
//! fires only on concrete caps, so effect-polymorphic higher-order code is never falsely rejected,
//! while a concrete cap discovered alongside an open tail still fires.
//!
//! Self-contained on purpose: this module is one of the pieces Phase 4 ports into Anubis itself.

use crate::frontend::{Expr, ForSource, Item, Stmt};
use std::collections::{BTreeMap, BTreeSet};

/// The six canonical capability ids a row may contain (the gated subset of effect tags — analysis
/// tags like taint/assume/loop are never row members). Mirrors `capability_effect`'s domain.
const CAPABILITY_IDS: [&str; 6] = ["fs.read", "fs.write", "net.send", "shell", "time.now", "rand.gen"];

/// A transitive effect row: the canonical capability ids a function performs, plus whether the row
/// is open over an unknown tail (unresolvable callee). Union is set-union with `open` OR-ed — the
/// join of the finite lattice (2^6 caps × open bit) the fixpoint converges over.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct EffectRow {
    pub effects: BTreeSet<String>,
    pub open: bool,
}

impl EffectRow {
    pub fn union(&mut self, other: &EffectRow) {
        self.effects.extend(other.effects.iter().cloned());
        self.open |= other.open;
    }
}

/// Canonical capability id a builtin callee name carries, if any. A pure mirror of the inline
/// classification arms in `analyze_stmts`'s `Expr::Call` handling (`middle/mod.rs`) — those arms
/// stay untouched (they also drive the enforcing Safe-mode gates); the parity unit test below pins
/// this mirror against the exact same name table so the two cannot drift silently.
pub(crate) fn builtin_effect_of(callee: &str) -> Option<&'static str> {
    if callee == "shell" || callee == "exec" || callee == "system" || callee == "target_run" {
        return Some("shell");
    }
    if callee == "read_file" || callee == "open" {
        return Some("fs.read");
    }
    if callee == "write_file" || callee == "write" || callee == "append_file" {
        return Some("fs.write");
    }
    if callee.contains("network")
        || callee == "send"
        || callee == "connect"
        || callee == "http_get"
        || callee == "http_post"
    {
        return Some("net.send");
    }
    if matches!(callee, "time" | "time_now" | "now") {
        return Some("time.now");
    }
    if matches!(callee, "rand" | "rand_gen" | "random") {
        return Some("rand.gen");
    }
    None
}

/// The argument positions at which a HIGHER-ORDER builtin receives a closure it APPLIES internally
/// (#65). Because the application happens inside the `anubis_*` runtime fn, there is no source-level
/// application node, so an inline `|x| send(..)` at one of these indices would otherwise be charged
/// NOTHING — defeating the trifecta + the Safe net.send gate. The checker descends into such inline
/// lambdas to charge their body's effects (effects.rs) and run the taint/secret/capability checks
/// (mod.rs `analyze_expr_effect`). Kept in lock-step with the closure-applying arms of
/// `backends::run::emit_builtin_call` (map/each/… all `fixed(anubis_*, .., N)` whose runtime body
/// calls `f.call_closure`). A unit test asserts the expected index set (it is a change-detector on
/// THIS set, not a structural diff against run.rs) — so when adding a closure-applying builtin to
/// run.rs, add it here in lock-step: a missing name under-fires (fail-open, safe) but SILENTLY. The
/// recognizer is consulted ONLY when the callee resolves to the actual builtin (the
/// `effects.rs` if/else chain and the mod.rs `!all_fns` guard exclude a user fn / local of the same
/// name), so a user-defined `fn apply`/`fn compose` is analyzed on its own row, never over-charged.
pub(crate) fn higher_order_closure_args(callee: &str) -> &'static [usize] {
    match callee {
        // list/map HOFs + `times`/`sort_by`/… — closure at index 1 (data first, closure second).
        "map" | "filter" | "each" | "find" | "any" | "all" | "count" | "sort_by" | "flat_map"
        | "take_while" | "drop_while" | "position" | "min_by" | "max_by" | "partition"
        | "map_values" | "reduce" | "times" => &[1],
        // `apply(f, args)` / `call(f, …)` — closure at index 0.
        "apply" | "call" => &[0],
        // `compose(f, g)` — BOTH indices are closures.
        "compose" => &[0, 1],
        _ => &[],
    }
}

/// Whether a normalized effect name is one of the six gated capability ids (custom `uses(...)`
/// tags pass through `normalize_effect_name` unchanged and are analysis-only — never row members,
/// exactly as `capability_effect` excludes them from the enforcing check's `caps_used`).
fn is_capability_id(canon: &str) -> bool {
    CAPABILITY_IDS.contains(&canon)
}

/// Lexical scope stack for the walk: a name bound by a `let`/pattern/param/loop-var in any live
/// frame shadows a same-named global fn AND builtin, so calling it is a closure/local call →
/// `open`, never that global's row (the flat-name closure-shadow bug class, held out by test).
struct Scope {
    frames: Vec<BTreeSet<String>>,
}

impl Scope {
    fn new(params: &[String]) -> Self {
        Scope {
            frames: vec![params.iter().cloned().collect()],
        }
    }
    fn bind(&mut self, name: &str) {
        if let Some(f) = self.frames.last_mut() {
            f.insert(name.to_string());
        }
    }
    fn contains(&self, name: &str) -> bool {
        self.frames.iter().any(|f| f.contains(name))
    }
    fn push(&mut self) {
        self.frames.push(BTreeSet::new());
    }
    fn pop(&mut self) {
        self.frames.pop();
    }
}

/// Everything the walker resolves callees against. `rows` is the in-progress fixpoint state.
struct WalkCtx<'a> {
    all_fns: &'a BTreeSet<String>,
    declared: &'a BTreeMap<String, Vec<String>>,
    rows: &'a BTreeMap<String, EffectRow>,
}

fn walk_stmts(stmts: &[Stmt], cx: &WalkCtx, scope: &mut Scope, row: &mut EffectRow) {
    for stmt in stmts {
        match stmt {
            Stmt::Let { name, init, .. } => {
                walk_expr(init, cx, scope, row);
                scope.bind(name);
            }
            Stmt::LetPattern { pattern, init, .. } => {
                walk_expr(init, cx, scope, row);
                for n in pattern.bound_names() {
                    scope.bind(&n);
                }
            }
            Stmt::WhileLet {
                pattern,
                expr,
                body,
            } => {
                walk_expr(expr, cx, scope, row);
                scope.push();
                for n in pattern.bound_names() {
                    scope.bind(&n);
                }
                walk_stmts(body, cx, scope, row);
                scope.pop();
            }
            Stmt::Assign { target, value } => {
                walk_expr(target, cx, scope, row);
                walk_expr(value, cx, scope, row);
            }
            Stmt::If { cond, then, else_ } => {
                walk_expr(cond, cx, scope, row);
                scope.push();
                walk_stmts(then, cx, scope, row);
                scope.pop();
                if let Some(else_body) = else_ {
                    scope.push();
                    walk_stmts(else_body, cx, scope, row);
                    scope.pop();
                }
            }
            Stmt::While {
                cond,
                body,
                invariant,
            } => {
                walk_expr(cond, cx, scope, row);
                for inv in invariant {
                    walk_expr(inv, cx, scope, row);
                }
                scope.push();
                walk_stmts(body, cx, scope, row);
                scope.pop();
            }
            Stmt::Loop { body, invariant } => {
                for inv in invariant {
                    walk_expr(inv, cx, scope, row);
                }
                scope.push();
                walk_stmts(body, cx, scope, row);
                scope.pop();
            }
            Stmt::For {
                var,
                source,
                body,
                invariant,
            } => {
                match source {
                    ForSource::Range { start, end } => {
                        walk_expr(start, cx, scope, row);
                        walk_expr(end, cx, scope, row);
                    }
                    ForSource::Collection { expr } => walk_expr(expr, cx, scope, row),
                }
                for inv in invariant {
                    walk_expr(inv, cx, scope, row);
                }
                scope.push();
                scope.bind(var);
                walk_stmts(body, cx, scope, row);
                scope.pop();
            }
            Stmt::Break | Stmt::Continue | Stmt::SpecBlock { .. } => {}
            Stmt::ResearchBlock { body, .. } | Stmt::ExploitBlock { body, .. } => {
                scope.push();
                walk_stmts(body, cx, scope, row);
                scope.pop();
            }
            Stmt::HybridBlock { gpu, cpu, prove } => {
                for b in [gpu, cpu, prove].into_iter().flatten() {
                    scope.push();
                    walk_stmts(b, cx, scope, row);
                    scope.pop();
                }
            }
            Stmt::ExprStmt(e) => walk_expr(e, cx, scope, row),
        }
    }
}

fn walk_expr(expr: &Expr, cx: &WalkCtx, scope: &mut Scope, row: &mut EffectRow) {
    match expr {
        Expr::Var(_)
        | Expr::Literal(_)
        | Expr::StrLiteral(_)
        | Expr::Symbolic { .. }
        | Expr::TaintSource { .. }
        | Expr::UnifiedBuffer { .. }
        | Expr::RawPtr { .. }
        | Expr::Other(_) => {}
        Expr::Call { callee, args } => {
            for a in args {
                walk_expr(a, cx, scope, row);
            }
            if scope.contains(callee) {
                // A local binding (param / let / closure) shadows any global fn or builtin of the
                // same name: a closure call — unknown effects, open tail, never the global's row.
                row.open = true;
            } else if cx.all_fns.contains(callee) {
                // Known user function: union its (in-progress) transitive row AND its declared
                // `uses(...)` caps — declared is the author's promise of possible effects, and the
                // shipped one-hop check already inherits declared caps, so 2+ hops must never be
                // weaker than 1 hop. Defensive: a name in `all_fns` with no computed row (should
                // not happen — both are built over the same combined items) opens the row rather
                // than silently claiming it is effect-free.
                match cx.rows.get(callee) {
                    Some(r) => row.union(r),
                    None => row.open = true,
                }
                for raw in cx.declared.get(callee).into_iter().flatten() {
                    let canon = super::normalize_effect_name(raw);
                    if is_capability_id(&canon) {
                        row.effects.insert(canon);
                    }
                }
            } else if let Some(cap) = builtin_effect_of(callee) {
                row.effects.insert(cap.to_string());
            } else if crate::backends::run::is_builtin_name(callee) {
                // A known builtin with no capability classification (`print`, `len`, `push`, …):
                // effect-free by the same registry the unknown-call check trusts — row stays closed.
                // BUT a HIGHER-ORDER builtin (`map`/`each`/`times`/…) APPLIES its closure argument
                // internally, so that inline lambda's body effects belong to THIS function's row (#65).
                // Reached only when `callee` is neither a local binding (line above) nor a user fn (the
                // `all_fns` branch above), so this fires solely for the real builtin — a user `fn map`
                // keeps its own computed row. The generic args walk already visited (and skipped, at the
                // `Expr::Lambda` arm) this lambda, so walking the body here charges it exactly once.
                for &i in higher_order_closure_args(callee) {
                    if let Some(Expr::Lambda { params, body }) = args.get(i) {
                        scope.push();
                        for p in params {
                            scope.bind(p);
                        }
                        walk_expr(body, cx, scope, row);
                        scope.pop();
                    }
                }
            } else {
                // Unknown bare name — nothing resolvable to charge, so the tail is open.
                row.open = true;
            }
        }
        Expr::CallExpr { callee, args } => {
            // Method / receiver / chained call (`obj.f(x)`, `f(a)(b)`): outside the flat fn table,
            // unresolvable here → open. The row must never stay CLOSED past an unclassified call.
            walk_expr(callee, cx, scope, row);
            for a in args {
                walk_expr(a, cx, scope, row);
            }
            row.open = true;
        }
        Expr::Binary { lhs, rhs, .. } => {
            walk_expr(lhs, cx, scope, row);
            walk_expr(rhs, cx, scope, row);
        }
        Expr::Unary { expr, .. } | Expr::Cast { expr, .. } => walk_expr(expr, cx, scope, row),
        Expr::Tainted { inner, .. } | Expr::Declassify { inner, .. } => {
            walk_expr(inner, cx, scope, row)
        }
        Expr::Assume(inner) | Expr::Assert(inner) | Expr::Try(inner) => {
            walk_expr(inner, cx, scope, row)
        }
        Expr::ArrayLiteral { elements } => {
            for e in elements {
                walk_expr(e, cx, scope, row);
            }
        }
        Expr::Index { base, index } => {
            walk_expr(base, cx, scope, row);
            walk_expr(index, cx, scope, row);
        }
        Expr::StructLiteral { fields, .. } => {
            for (_, e) in fields {
                walk_expr(e, cx, scope, row);
            }
        }
        Expr::FieldAccess { base, .. } => walk_expr(base, cx, scope, row),
        Expr::EnumConstruct { fields, .. } => {
            for e in fields {
                walk_expr(e, cx, scope, row);
            }
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            walk_expr(scrutinee, cx, scope, row);
            for arm in arms {
                scope.push();
                for n in arm.pattern.bound_names() {
                    scope.bind(&n);
                }
                if let Some(g) = &arm.guard {
                    walk_expr(g, cx, scope, row);
                }
                walk_expr(&arm.body, cx, scope, row);
                scope.pop();
            }
        }
        Expr::If {
            cond, then, else_, ..
        } => {
            walk_expr(cond, cx, scope, row);
            walk_expr(then, cx, scope, row);
            walk_expr(else_, cx, scope, row);
        }
        Expr::IfLet {
            pattern,
            scrutinee,
            then,
            else_,
            ..
        } => {
            walk_expr(scrutinee, cx, scope, row);
            scope.push();
            for n in pattern.bound_names() {
                scope.bind(&n);
            }
            walk_expr(then, cx, scope, row);
            scope.pop();
            walk_expr(else_, cx, scope, row);
        }
        Expr::MapLiteral { entries, .. } => {
            for (k, v) in entries {
                walk_expr(k, cx, scope, row);
                walk_expr(v, cx, scope, row);
            }
        }
        Expr::Block { stmts, tail } => {
            scope.push();
            walk_stmts(stmts, cx, scope, row);
            if let Some(t) = tail {
                walk_expr(t, cx, scope, row);
            }
            scope.pop();
        }
        Expr::Lambda { .. } => {
            // A closure LITERAL performs nothing at its definition site — its body's effects belong
            // to the closure value, charged (as `open`) wherever it is actually called. Charging
            // them here would fire on effects this function never executes (over-reject); skipping
            // keeps the row's meaning exact: caps this function's own execution concretely reaches.
        }
    }
}

/// Compute the transitive `EffectRow` for every free function, by monotone fixpoint: re-walk each
/// body against the current rows until nothing grows. Rows only ever grow (walks are monotone in
/// `rows`) and the lattice is finite (six caps + the open bit), so this converges — self and
/// mutual recursion included. Pure: reads the AST and the pass-1 tables, emits no diagnostics,
/// never touches per-function analysis state (`authorized_caps`, taint traces, Safe-mode gates).
pub(crate) fn compute_fn_effect_rows(
    items: &[Item],
    all_fns: &BTreeSet<String>,
    declared: &BTreeMap<String, Vec<String>>,
) -> BTreeMap<String, EffectRow> {
    let mut fns: Vec<(String, Vec<String>, &[Stmt])> = Vec::new();
    super::collect_fn_params_bodies(items, &mut fns);
    let mut rows: BTreeMap<String, EffectRow> = fns
        .iter()
        .map(|(name, _, _)| (name.clone(), EffectRow::default()))
        .collect();
    loop {
        let mut changed = false;
        for (name, params, body) in &fns {
            let mut row = EffectRow::default();
            let cx = WalkCtx {
                all_fns,
                declared,
                rows: &rows,
            };
            let mut scope = Scope::new(params);
            walk_stmts(body, &cx, &mut scope, &mut row);
            let cur = rows.get_mut(name).expect("row seeded above");
            if row != *cur {
                *cur = row;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    rows
}

/// The whole-program capability set the checker PROVES a program uses, plus the `open` bit — the
/// single source of truth for VZ confinement derivation (`package::confinement`). Unions every
/// function's transitive effect row (the exact `compute_fn_effect_rows` fixpoint the checker runs)
/// with every declared `uses(...)` capability, restricted to the six canonical `CAPABILITY_IDS`.
///
/// `open == true` when ANY row's open bit is set — a closure / parameter / unknown callee the effect
/// walk could not resolve, i.e. the program "may use anything". A confinement derivation MUST treat
/// `open` as unbounded and confine MOST restrictively (fail-closed), never permissively.
///
/// This is an over-approximation in the SAFE direction for confinement: a capability that is present
/// but MISSED by the fixpoint (e.g. a higher-order residual) yields a MORE restrictive hypervisor
/// grant, so a mis-analysed guest breaks rather than leaks. It reflects the DECLARED+inferred surface;
/// the hypervisor boundary is the backstop precisely for undeclared/higher-order flows.
pub(crate) fn program_capability_set(items: &[Item]) -> (BTreeSet<String>, bool) {
    let mut all_fns: BTreeSet<String> = BTreeSet::new();
    let mut declared: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut fns: Vec<(String, Vec<String>, &[Stmt])> = Vec::new();
    super::collect_fn_params_bodies(items, &mut fns);
    for (name, _, _) in &fns {
        all_fns.insert(name.clone());
    }
    for item in items {
        if let Item::Fn { name, effects, .. } = item {
            if !effects.is_empty() {
                declared.insert(name.clone(), effects.clone());
            }
        }
    }
    let rows = compute_fn_effect_rows(items, &all_fns, &declared);
    let mut caps: BTreeSet<String> = BTreeSet::new();
    let mut open = false;
    for row in rows.values() {
        for e in &row.effects {
            let canon = super::normalize_effect_name(e);
            if is_capability_id(&canon) {
                caps.insert(canon);
            }
        }
        open |= row.open;
    }
    // Fold declared uses(...) directly too — defensive against a declared cap on a fn whose body is
    // empty/opaque (declared ⊇ inferred is what the checker enforces; grant on the union either way).
    for decls in declared.values() {
        for d in decls {
            let canon = super::normalize_effect_name(d);
            if is_capability_id(&canon) {
                caps.insert(canon);
            }
        }
    }
    (caps, open)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend;

    fn rows_of(src: &str) -> BTreeMap<String, EffectRow> {
        let ast = frontend::parse_source(src).expect("parse");
        let mut all_fns = BTreeSet::new();
        let mut declared = BTreeMap::new();
        let mut fns: Vec<(String, Vec<String>, &[Stmt])> = Vec::new();
        super::super::collect_fn_params_bodies(&ast.items, &mut fns);
        for (name, _, _) in &fns {
            all_fns.insert(name.clone());
        }
        for item in &ast.items {
            if let frontend::Item::Fn { name, effects, .. } = item {
                if !effects.is_empty() {
                    declared.insert(name.clone(), effects.clone());
                }
            }
        }
        compute_fn_effect_rows(&ast.items, &all_fns, &declared)
    }

    #[test]
    fn effect_row_union_and_open_semantics() {
        let mut a = EffectRow {
            effects: ["fs.read".to_string()].into_iter().collect(),
            open: false,
        };
        let b = EffectRow {
            effects: ["net.send".to_string()].into_iter().collect(),
            open: true,
        };
        a.union(&b);
        assert!(a.effects.contains("fs.read") && a.effects.contains("net.send"));
        assert!(a.open, "open must OR");
        let mut c = b.clone();
        c.union(&EffectRow::default());
        assert_eq!(c, b, "union with bottom is identity");
    }

    #[test]
    fn builtin_classifier_parity_with_inline_arms() {
        // Pins the mirror against the exact name table of the inline arms in `analyze_stmts`
        // (`middle/mod.rs` Expr::Call handling). If an inline arm gains/loses a name, this table
        // must be updated in the same change — that is the point.
        let table: [(&str, Option<&str>); 22] = [
            ("shell", Some("shell")),
            ("exec", Some("shell")),
            ("system", Some("shell")),
            ("target_run", Some("shell")),
            ("read_file", Some("fs.read")),
            ("open", Some("fs.read")),
            ("write_file", Some("fs.write")),
            ("write", Some("fs.write")),
            ("append_file", Some("fs.write")),
            ("send", Some("net.send")),
            ("connect", Some("net.send")),
            ("network_send", Some("net.send")),
            ("http_get", Some("net.send")),
            ("http_post", Some("net.send")),
            ("time", Some("time.now")),
            ("time_now", Some("time.now")),
            ("now", Some("time.now")),
            ("rand", Some("rand.gen")),
            ("rand_gen", Some("rand.gen")),
            ("random", Some("rand.gen")),
            ("println", None),
            ("len", None),
        ];
        for (name, want) in table {
            assert_eq!(builtin_effect_of(name), want, "classifier({name})");
        }
    }

    #[test]
    fn fixpoint_converges_on_mutual_recursion() {
        // `a ↔ b` with the effect only in `b`: the fixpoint must land it in BOTH rows and
        // terminate (single-pass definition-order computation would miss `a`).
        let rows = rows_of(
            r#"fn a(n: i64) { if n > 0 { b(n - 1); } }
fn b(n: i64) { time_now(); if n > 0 { a(n - 1); } }
fn main() { a(3); }"#,
        );
        for f in ["a", "b", "main"] {
            let row = &rows[f];
            assert!(row.effects.contains("time.now"), "{f} must carry time.now");
            assert!(!row.open, "{f} row is fully resolved — must stay closed");
        }
    }

    #[test]
    fn local_shadow_of_global_fn_does_not_pull_row() {
        // A let-bound closure shadowing an effectful global's name: calling it is a closure call
        // (open), never the global's row — the flat-name closure-shadow bug class.
        let rows = rows_of(
            r#"fn helper() { time_now(); }
fn main() {
    let helper = |x| x + 1;
    let y = helper(3);
    print(y);
}"#,
        );
        let main = &rows["main"];
        assert!(
            !main.effects.contains("time.now"),
            "shadowed local call must not pull the global row"
        );
        assert!(main.open, "closure call opens the row");
        // …and the shadow is BLOCK-scoped: after the block, the name is the global again.
        let rows = rows_of(
            r#"fn helper() { time_now(); }
fn main(c: bool) {
    if c { let helper = |x| x + 1; let _ = helper(1); }
    helper();
}"#,
        );
        assert!(
            rows["main"].effects.contains("time.now"),
            "outside the block the global row applies"
        );
    }

    #[test]
    fn open_row_forms_and_lambda_definition_is_free() {
        let rows = rows_of(
            r#"fn apply(f, x: i64) { return f(x); }
fn main() { let _ = apply(0, 1); }"#,
        );
        assert!(rows["apply"].open, "calling a parameter opens the row");
        assert!(rows["apply"].effects.is_empty());
        // Defining (without calling) a lambda charges nothing.
        let rows = rows_of(r#"fn main() { let g = || time_now(); print(1); }"#);
        assert!(rows["main"].effects.is_empty(), "lambda literal is free");
        assert!(!rows["main"].open);
    }

    #[test]
    fn declared_caps_of_callee_union_in() {
        // A claused callee's declared caps are the author's promise of possible effects — they
        // union in even when its body is opaque about them (matches the shipped one-hop check).
        let rows = rows_of(
            r#"fn writer(p: string, d: string) uses(fs.write) { write_file(p, d); }
fn mid(p: string, d: string) { writer(p, d); }
fn main() { mid("a", "b"); }"#,
        );
        for f in ["writer", "mid", "main"] {
            assert!(rows[f].effects.contains("fs.write"), "{f} carries fs.write");
        }
    }
}
