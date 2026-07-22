//! Phase-2 final slice: lethal-trifecta detection (the body-scan half).
//!
//! The "lethal trifecta" is the AI-agent exfiltration condition: a function that (1) reads PRIVATE
//! data, (2) is exposed to UNTRUSTED input, and (3) can COMMUNICATE EXTERNALLY, all at once. An
//! injection in the untrusted input can then steer the private read and the egress — even when no
//! literal value flows from the read to the send, which is exactly the case the value-flow taint
//! check (`ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY`) cannot see.
//!
//! Legs 1 (fs.read) and 3 (net.send) are read off the transitive effect row in `mod.rs` (closed in
//! the verified lane, where open rows were already rejected; in Safe an open row stays legal and leg
//! detection under-approximates — accept-biased). THIS module supplies the two body-scanned
//! signals: leg 2 (an untrusted source DISTINCT from the private file read — the attacker's steering
//! channel must be a different channel than the data it steers) and the escape hatch (a WELL-FORMED
//! `declassify(v, policy, reason)` present in the body). Presence scan only — no flow, no state.
//!
//! Completeness matters asymmetrically: missing a leg-2 source only fails to fire (accept-biased,
//! safe), but missing a well-formed declassify would OVER-reject — so the walk visits every node.
//! Self-contained and pure, mirroring effects.rs/capability.rs (a Phase-4 Anubis-port candidate).

use crate::frontend::{Expr, ForSource, Stmt};

/// The confidentiality label: `secret_source(v)` marks a value as PRIVATE — the dual of
/// `taint_source` (which marks a value untrusted). Its presence is leg 1 (private-data access), a
/// precise alternative to the coarse "a file was read" (`fs.read`) proxy the trifecta also accepts.
const SECRET_SOURCE: &str = "secret_source";

#[derive(Debug, Default)]
pub(crate) struct TrifectaLegs {
    /// Label of the first untrusted source that is NOT the private file read (`input`, `recv`,
    /// `env`, a `taint_source(..)`, or a `tainted<T>` parameter). `None` = no leg-2 channel found.
    pub leg2_untrusted: Option<String>,
    /// A well-formed `declassify(inner, policy, reason)` (both policy and reason present) appears
    /// somewhere in the body — the author's explicit, reviewed sanitization barrier.
    pub wellformed_declassify: bool,
    /// A `secret_source(..)` value appears in the body — leg 1 (private-data access) via the
    /// explicit confidentiality label, independent of any `fs.read`.
    pub secret_present: bool,
    /// Capability effects of functions CALLED THROUGH a function-value alias (`let f = reader; f()`).
    /// The name-keyed leg scan and the caller's inferred effects both miss such an aliased call, so
    /// these are unioned into the caller's leg-1/leg-3 (fs.read / net.send / shell) at the check site.
    pub aliased_effects: std::collections::BTreeSet<String>,
}

/// Interprocedural summaries consulted by the walk (both the confidentiality dual of `tainting_fns`
/// and the leg-2 exposure summary). `scan_legs` is called with the real sets from `SemanticContext`;
/// `compute_leg2_fns` fixpoint-builds the leg-2 set by calling the walk with an empty secret set and
/// the leg-2 set accumulated so far.
pub(crate) struct ScanCtx<'a> {
    /// Functions whose return carries a `secret_source` secret — a call to one is leg-1 private data.
    pub secret_fns: &'a std::collections::BTreeSet<String>,
    /// Functions whose body exposes an untrusted steering channel — a call to one is leg 2.
    pub leg2_fns: &'a std::collections::BTreeSet<String>,
    /// `let x = <known fn>` function-value aliases in this body: `x` → the function it names. A call
    /// `x()` is resolved through this so an aliased leg is detected (audit follow-up `task_fdb35824`).
    pub fn_aliases: &'a std::collections::BTreeMap<String, String>,
    /// Per-function transitive capability effects (name → {fs.read, net.send, shell, …}), so a call
    /// through an alias contributes the aliased function's effects to `aliased_effects`.
    pub fn_effects: &'a std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
}

/// Collect `let x = <known function name>` function-value aliases (recursively through nested bodies),
/// mapping the alias variable to the function it names; a chain `let g = f` resolves to `f`'s target.
fn collect_fn_aliases(
    stmts: &[Stmt],
    all_fns: &std::collections::BTreeSet<String>,
    out: &mut std::collections::BTreeMap<String, String>,
) {
    for s in stmts {
        if let Stmt::Let {
            name,
            init: Expr::Var(v),
            ..
        } = s
        {
            if all_fns.contains(v) {
                out.insert(name.clone(), v.clone());
            } else if let Some(t) = out.get(v).cloned() {
                out.insert(name.clone(), t);
            }
        }
        match s {
            Stmt::WhileLet { body, .. }
            | Stmt::While { body, .. }
            | Stmt::Loop { body, .. }
            | Stmt::For { body, .. }
            | Stmt::ResearchBlock { body, .. }
            | Stmt::ExploitBlock { body, .. } => collect_fn_aliases(body, all_fns, out),
            Stmt::If { then, else_, .. } => {
                collect_fn_aliases(then, all_fns, out);
                if let Some(e) = else_ {
                    collect_fn_aliases(e, all_fns, out);
                }
            }
            Stmt::HybridBlock { gpu, cpu, prove } => {
                for b in [gpu, cpu, prove].into_iter().flatten() {
                    collect_fn_aliases(b, all_fns, out);
                }
            }
            _ => {}
        }
    }
}

/// Scan one function body + its parameters for the body-side trifecta signals. `secret_fns`/`leg2_fns`
/// carry the interprocedural summaries so a secret or an untrusted-input exposure reached THROUGH a
/// helper is detected (a call to a `secret_fns` member is leg 1; a call to a `leg2_fns` member is
/// leg 2), not only a direct `secret_source`/`input` in this body.
pub(crate) fn scan_legs(
    body: &[Stmt],
    params: &[(String, String)],
    secret_fns: &std::collections::BTreeSet<String>,
    leg2_fns: &std::collections::BTreeSet<String>,
    all_fns: &std::collections::BTreeSet<String>,
    fn_effects: &std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
) -> TrifectaLegs {
    let mut fn_aliases = std::collections::BTreeMap::new();
    collect_fn_aliases(body, all_fns, &mut fn_aliases);
    let sc = ScanCtx {
        secret_fns,
        leg2_fns,
        fn_aliases: &fn_aliases,
        fn_effects,
    };
    let mut legs = TrifectaLegs::default();
    // A `tainted<T>` parameter is untrusted input arriving directly as an argument — a leg-2 channel.
    // (This param-side signal is DELIBERATELY not part of `compute_leg2_fns`: a function that RECEIVES
    // a tainted param does not itself SOURCE untrusted input for its caller — see that fixpoint.)
    for (pname, pty) in params {
        if super::is_tainted_type(Some(pty)) {
            legs.leg2_untrusted
                .get_or_insert_with(|| format!("tainted parameter `{pname}`"));
        }
        // A `secret<T>` parameter is PRIVATE DATA arriving directly as an argument — leg 1 (the
        // confidentiality dual of the `tainted<T>` → leg-2 signal above). Without this, a function
        // holding a `secret<T>` param + a distinct untrusted-input channel + an egress would not form
        // the no-flow lethal trifecta, even though a `secret_source(..)` value in the same shape does.
        if super::is_secret_type(Some(pty)) {
            legs.secret_present = true;
        }
    }
    walk_stmts(body, &mut legs, &sc);
    legs
}

/// Monotone fixpoint over the free functions: a function is leg-2-EXPOSING iff its body transitively
/// SOURCES untrusted input — a direct `is_leg2_source` call / `taint_source(..)`, or a call to an
/// already-marked leg-2 function. PRESENCE semantics (matching the intra-procedural leg-2): a helper
/// that reads `input()` and discards it still exposes its caller to steering. Uses `is_leg2_source`
/// (which excludes `read_file`/`open`), so a file-reading helper is never mis-marked as a steering
/// channel — the read_file/leg-2 conflation the design avoids. Parameter-side taint is NOT considered
/// here (receiving a tainted param does not make a function source untrusted input for its caller).
pub(crate) fn compute_leg2_fns(
    items: &[crate::frontend::Item],
) -> std::collections::BTreeSet<String> {
    let mut fns: Vec<(String, Vec<String>, &[Stmt])> = Vec::new();
    super::collect_fn_params_bodies(items, &mut fns);
    let empty_secret = std::collections::BTreeSet::new();
    let empty_aliases = std::collections::BTreeMap::new();
    let empty_effects = std::collections::BTreeMap::new();
    let mut leg2: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    loop {
        let mut newly: Vec<String> = Vec::new();
        for (name, _params, body) in &fns {
            if !leg2.contains(name) {
                // The leg-2 fixpoint uses direct names only; an interprocedural alias chain is a
                // further increment (its omission only under-detects, never a false positive).
                let sc = ScanCtx {
                    secret_fns: &empty_secret,
                    leg2_fns: &leg2,
                    fn_aliases: &empty_aliases,
                    fn_effects: &empty_effects,
                };
                // Body-only walk (no param check) — a returned/param-received taint is not a SOURCE.
                let mut legs = TrifectaLegs::default();
                walk_stmts(body, &mut legs, &sc);
                if legs.leg2_untrusted.is_some() {
                    newly.push(name.clone());
                }
            }
        }
        if newly.is_empty() {
            break;
        }
        leg2.extend(newly);
    }
    leg2
}

/// Whether a bare-name call is an untrusted taint source OTHER than the private file read.
/// `read_file`/`open` are leg 1 (private data), never leg 2 — the steering channel must be distinct.
fn is_leg2_source(callee: &str) -> bool {
    super::is_io_taint_source(callee) && callee != "read_file" && callee != "open"
}

fn walk_stmts(stmts: &[Stmt], legs: &mut TrifectaLegs, sc: &ScanCtx) {
    for s in stmts {
        walk_stmt(s, legs, sc);
    }
}

fn walk_stmt(stmt: &Stmt, legs: &mut TrifectaLegs, sc: &ScanCtx) {
    match stmt {
        Stmt::Let { init, .. } => walk_expr(init, legs, sc),
        Stmt::LetPattern { init, .. } => walk_expr(init, legs, sc),
        Stmt::WhileLet { expr, body, .. } => {
            walk_expr(expr, legs, sc);
            walk_stmts(body, legs, sc);
        }
        Stmt::Assign { target, value } => {
            walk_expr(target, legs, sc);
            walk_expr(value, legs, sc);
        }
        Stmt::If { cond, then, else_ } => {
            walk_expr(cond, legs, sc);
            walk_stmts(then, legs, sc);
            if let Some(e) = else_ {
                walk_stmts(e, legs, sc);
            }
        }
        Stmt::While {
            cond,
            body,
            invariant,
        } => {
            walk_expr(cond, legs, sc);
            for inv in invariant {
                walk_expr(inv, legs, sc);
            }
            walk_stmts(body, legs, sc);
        }
        Stmt::Loop { body, invariant } => {
            for inv in invariant {
                walk_expr(inv, legs, sc);
            }
            walk_stmts(body, legs, sc);
        }
        Stmt::For {
            source,
            body,
            invariant,
            ..
        } => {
            match source {
                ForSource::Range { start, end } => {
                    walk_expr(start, legs, sc);
                    walk_expr(end, legs, sc);
                }
                ForSource::Collection { expr } => walk_expr(expr, legs, sc),
            }
            for inv in invariant {
                walk_expr(inv, legs, sc);
            }
            walk_stmts(body, legs, sc);
        }
        Stmt::ResearchBlock { body, .. } | Stmt::ExploitBlock { body, .. } => {
            walk_stmts(body, legs, sc)
        }
        Stmt::HybridBlock { gpu, cpu, prove } => {
            for b in [gpu, cpu, prove].into_iter().flatten() {
                walk_stmts(b, legs, sc);
            }
        }
        Stmt::Break | Stmt::Continue | Stmt::SpecBlock { .. } => {}
        Stmt::ExprStmt(e) => walk_expr(e, legs, sc),
    }
}

fn walk_expr(expr: &Expr, legs: &mut TrifectaLegs, sc: &ScanCtx) {
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
            // A call `x()` whose name is a function-value alias (`let x = reader`) is resolved to the
            // aliased function for leg detection — otherwise the leg is laundered through the binding
            // (audit follow-up: closure/function-value-aliased legs). `resolved` is the effective
            // callee name; its effects are also credited so leg-1 fs.read / leg-3 egress are counted.
            let resolved: &str = sc
                .fn_aliases
                .get(callee)
                .map(String::as_str)
                .unwrap_or(callee);
            if sc.fn_aliases.contains_key(callee) {
                if let Some(eff) = sc.fn_effects.get(resolved) {
                    legs.aliased_effects.extend(eff.iter().cloned());
                }
            }
            // Leg 1 (private data): a direct `secret_source(..)` OR a call to a helper whose return
            // carries a secret (interprocedural `secret_fns`).
            if resolved == SECRET_SOURCE || sc.secret_fns.contains(resolved) {
                legs.secret_present = true;
            }
            // Leg 2 (untrusted steering): a direct steering source OR a call to a helper that
            // transitively sources untrusted input (interprocedural `leg2_fns`).
            if is_leg2_source(resolved) {
                legs.leg2_untrusted
                    .get_or_insert_with(|| resolved.to_string());
            } else if sc.leg2_fns.contains(resolved) {
                legs.leg2_untrusted
                    .get_or_insert_with(|| format!("{resolved}() (exposes untrusted input)"));
            }
            for a in args {
                walk_expr(a, legs, sc);
            }
        }
        Expr::CallExpr { callee, args } => {
            // Resolve a function-value alias (`let f = reader; f()`): a leg reached through the alias
            // is counted as if the aliased function were called by name — for the leg-1 (secret) and
            // leg-2 (untrusted) NAME scan, and for the aliased function's effects (leg-1 fs.read /
            // leg-3 egress), which the caller's inferred capabilities otherwise miss.
            if let Expr::Var(v) = callee.as_ref() {
                if let Some(target) = sc.fn_aliases.get(v) {
                    if target == SECRET_SOURCE || sc.secret_fns.contains(target) {
                        legs.secret_present = true;
                    }
                    if is_leg2_source(target) {
                        legs.leg2_untrusted.get_or_insert_with(|| target.clone());
                    } else if sc.leg2_fns.contains(target) {
                        legs.leg2_untrusted
                            .get_or_insert_with(|| format!("{target}() (exposes untrusted input)"));
                    }
                    if let Some(eff) = sc.fn_effects.get(target) {
                        legs.aliased_effects.extend(eff.iter().cloned());
                    }
                }
            }
            walk_expr(callee, legs, sc);
            for a in args {
                walk_expr(a, legs, sc);
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
            // Well-formed = both present AND non-empty (operator security fix 2026-07-20:
            // `declassify(x, "", "")` is a silent no-op, not a reviewed barrier).
            if matches!((policy, reason), (Some(p), Some(r)) if !p.trim().is_empty() && !r.trim().is_empty())
            {
                legs.wellformed_declassify = true;
                // A well-formed declassify RELEASES its inner value: a sanitized untrusted read is
                // NOT a leg-2 steering channel and a sanitized secret is NOT leg 1, so do not descend
                // into `inner` for leg detection. This makes the barrier hold ACROSS a helper boundary
                // — `compute_leg2_fns` must not mark `fn q(){ return declassify(input(),p,r); }` as a
                // leg-2 exposer, which would falsely reject a valid agent that calls it (the
                // over-rejection the implementation review confirmed). Intra-procedurally the outcome
                // is unchanged: a well-formed declassify already suppressed the trifecta via the flag.
            } else {
                // A malformed declassify is not a release — keep scanning `inner` for legs.
                walk_expr(inner, legs, sc);
            }
        }
        Expr::Tainted { inner, .. } => walk_expr(inner, legs, sc),
        Expr::Assume(inner) | Expr::Assert(inner) | Expr::Try(inner) => walk_expr(inner, legs, sc),
        Expr::Unary { expr, .. } | Expr::Cast { expr, .. } => walk_expr(expr, legs, sc),
        Expr::Binary { lhs, rhs, .. } => {
            walk_expr(lhs, legs, sc);
            walk_expr(rhs, legs, sc);
        }
        Expr::ArrayLiteral { elements } => {
            for e in elements {
                walk_expr(e, legs, sc);
            }
        }
        Expr::Index { base, index } => {
            walk_expr(base, legs, sc);
            walk_expr(index, legs, sc);
        }
        Expr::StructLiteral { fields, .. } => {
            for (_, e) in fields {
                walk_expr(e, legs, sc);
            }
        }
        Expr::FieldAccess { base, .. } => walk_expr(base, legs, sc),
        Expr::EnumConstruct { fields, .. } => {
            for e in fields {
                walk_expr(e, legs, sc);
            }
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            walk_expr(scrutinee, legs, sc);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    walk_expr(g, legs, sc);
                }
                walk_expr(&arm.body, legs, sc);
            }
        }
        Expr::If {
            cond, then, else_, ..
        } => {
            walk_expr(cond, legs, sc);
            walk_expr(then, legs, sc);
            walk_expr(else_, legs, sc);
        }
        Expr::IfLet {
            scrutinee,
            then,
            else_,
            ..
        } => {
            walk_expr(scrutinee, legs, sc);
            walk_expr(then, legs, sc);
            walk_expr(else_, legs, sc);
        }
        Expr::MapLiteral { entries, .. } => {
            for (k, v) in entries {
                walk_expr(k, legs, sc);
                walk_expr(v, legs, sc);
            }
        }
        Expr::Block { stmts, tail } => {
            walk_stmts(stmts, legs, sc);
            if let Some(t) = tail {
                walk_expr(t, legs, sc);
            }
        }
        Expr::Lambda { body, .. } => walk_expr(body, legs, sc),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend;
    use std::collections::BTreeSet;

    /// Scan `agent` with no interprocedural summaries (intra-procedural legs only).
    fn legs_of(src: &str) -> TrifectaLegs {
        legs_of_with(src, &BTreeSet::new(), &BTreeSet::new())
    }

    /// Scan `agent` with explicit `secret_fns` / `leg2_fns` summaries (interprocedural legs).
    fn legs_of_with(
        src: &str,
        secret_fns: &BTreeSet<String>,
        leg2_fns: &BTreeSet<String>,
    ) -> TrifectaLegs {
        let ast = frontend::parse_source(src).expect("parse");
        for item in &ast.items {
            if let frontend::Item::Fn {
                name, params, body, ..
            } = item
            {
                if name == "agent" {
                    // Existing tests exercise direct-name legs; alias resolution (empty here) is
                    // covered by the dedicated closure-aliased-leg fixture.
                    return scan_legs(
                        body,
                        params,
                        secret_fns,
                        leg2_fns,
                        &BTreeSet::new(),
                        &std::collections::BTreeMap::new(),
                    );
                }
            }
        }
        panic!("no `agent` fn");
    }

    #[test]
    fn detects_distinct_untrusted_source_not_the_read() {
        let legs = legs_of(
            r#"fn agent() { let s = input(); let d = read_file("x"); send("h", 80, "b"); }"#,
        );
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
        assert!(legs
            .leg2_untrusted
            .as_deref()
            .unwrap()
            .contains("tainted parameter"));
    }

    #[test]
    fn secret_param_is_leg1_confidentiality() {
        // The confidentiality dual of tainted_param_is_leg2: a secret<T> param is leg-1 private data.
        assert!(legs_of(r#"fn agent(k: secret<u64>) { let d = read_file("x"); }"#).secret_present);
        // A param merely NAMED with "secret" (not the qualifier) is not leg-1.
        assert!(
            !legs_of(r#"fn agent(secret_key: u64) { let d = read_file("x"); }"#).secret_present
        );
    }

    #[test]
    fn wellformed_declassify_detected_malformed_ignored() {
        assert!(
            legs_of(r#"fn agent() { let s = input(); let x = declassify(s, "p", "r"); }"#)
                .wellformed_declassify
        );
        assert!(
            !legs_of(r#"fn agent() { let s = input(); let x = declassify(s); }"#)
                .wellformed_declassify
        );
    }

    #[test]
    fn secret_source_is_leg1_confidentiality() {
        let legs =
            legs_of(r#"fn agent() { let k = secret_source("api_key"); send("h", 80, "b"); }"#);
        assert!(legs.secret_present);
        // A plain read with no secret_source: the label is absent (fs.read handles that leg in mod.rs).
        assert!(!legs_of(r#"fn agent() { let d = read_file("x"); }"#).secret_present);
    }

    #[test]
    fn scans_into_nested_branches_and_blocks() {
        let legs = legs_of(
            r#"fn agent(c: bool) { if c { let s = recv(); } else { let x = declassify(read_file("x"), "p", "r"); } }"#,
        );
        assert_eq!(legs.leg2_untrusted.as_deref(), Some("recv"));
        assert!(legs.wellformed_declassify);
    }

    // ── Interprocedural legs: leg 1 via a secret helper, leg 2 via an input-exposing helper ──────

    #[test]
    fn interproc_leg2_a_helper_exposing_input_counts() {
        // With `get_steer` in leg2_fns, calling it is leg 2 even though the agent body has no
        // direct input(). Without the summary it is missed (accept-biased).
        let leg2: BTreeSet<String> = ["get_steer".to_string()].into_iter().collect();
        let src = r#"fn agent() { let s = get_steer(); send("h", 80, "b"); }"#;
        assert!(legs_of_with(src, &BTreeSet::new(), &leg2)
            .leg2_untrusted
            .is_some());
        assert!(legs_of(src).leg2_untrusted.is_none()); // no summary => not seen
    }

    #[test]
    fn interproc_leg1_a_helper_returning_a_secret_counts() {
        let secret: BTreeSet<String> = ["get_key".to_string()].into_iter().collect();
        let src = r#"fn agent() { let k = get_key(); send("h", 80, "b"); }"#;
        assert!(legs_of_with(src, &secret, &BTreeSet::new()).secret_present);
        assert!(!legs_of(src).secret_present); // no summary => not seen
    }

    #[test]
    fn compute_leg2_fns_marks_input_helper_transitively_but_never_a_file_reader() {
        let ast = frontend::parse_source(
            r#"fn reads_input() { let s = input(); }
fn wraps_it() { reads_input(); }
fn reads_file() { let d = read_file("cfg"); }
fn discards_input() { input(); let x = 1; }
fn sanitizes_input() { let s = declassify(input(), "policy", "reviewed"); }"#,
        )
        .expect("parse");
        let leg2 = compute_leg2_fns(&ast.items);
        // A direct input source, and a transitive caller of it, are BOTH leg-2 exposers.
        assert!(leg2.contains("reads_input"), "direct input source");
        assert!(leg2.contains("wraps_it"), "transitive caller of a leg-2 fn");
        // PRESENCE, not return-flow: a helper that reads input and discards it still exposes.
        assert!(leg2.contains("discards_input"), "presence semantics");
        // The read_file/leg-2 conflation the design avoids: a file reader is leg 1, NEVER leg 2.
        assert!(
            !leg2.contains("reads_file"),
            "file read must not be a leg-2 steering channel"
        );
        // The declassify barrier holds across the helper boundary: a helper that SANITIZES its
        // untrusted read is not a leg-2 exposer (the confirmed over-rejection, now closed).
        assert!(
            !leg2.contains("sanitizes_input"),
            "a declassified read is not a leg-2 channel"
        );
    }
}
