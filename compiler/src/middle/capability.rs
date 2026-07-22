//! Phase-2 slice 2: capability tokens as linear (use-once) values.
//!
//! A capability is an unforgeable linear token: minted only by `cap_acquire(...)`, used exactly
//! once, non-duplicable, and surrendered when passed away. This module is the intraprocedural
//! linearity checker that proves that discipline — the Austral half of the capability-and-effect
//! fusion.
//!
//! Phase-2 slice 3 (composition) joins this surface to the effect row: in VERIFIED mode, a function
//! that DIRECTLY performs a privileged effect must have genuinely ACQUIRED the capability
//! authorizing it (`ANUBIS_EFFECT_UNAUTHORIZED`). "Genuinely acquired" means a `cap_acquire("<id>")`
//! with a string-literal kind bound in this body — a parameter, a return, or a non-literal value
//! does NOT authorize, which closes the forge vector slice 2 flagged (an unknown-provenance token
//! must not authorize an effect it was never granted). The property is KIND-level, path-insensitive,
//! and DIRECT-builtins-only: it checks that the body contains a matching acquisition, not that a
//! live token is held at the exact call; and it authorizes only effects performed by direct builtins
//! here (transitive effects through callees are the callee's to authorize — interprocedural cap flow
//! is a later slice; declaration stays transitive via the effect row, acquisition is direct-only).
//!
//! Core rule: a *use* is any read-occurrence of a tracked capability variable, **except** a plain
//! rebind (`let y = c` / `y = c`), which MOVES the token (source consumed, target live). That one
//! unified definition closes aliasing (`let y = c`), aggregate (`[c, c]`), and per-occurrence
//! (`foo(c, c)`) laundering together — every other occurrence consumes, and a second consume is
//! `ANUBIS_CAPABILITY_REUSE`. `cap_use` on a provable non-capability is `ANUBIS_CAPABILITY_MISSING`.
//!
//! Dual-mode, both directions held at once:
//!   - the REJECT DECISION is accept-biased (default lane): unknown provenance and uncertain
//!     consumption never produce a spurious reuse/missing rejection;
//!   - the LINEARITY resolves toward CONSUMED on uncertainty (verified lane): a branch may-consume
//!     and a loop-carried consume are treated as consumed, so a genuine reuse is never hidden.
//!
//! Self-contained and pure, mirroring effects.rs — it is one of the pieces Phase 4 ports into
//! Anubis itself. Linearity here is INTRAPROCEDURAL: a token returned, passed as a parameter, or
//! captured by a closure crosses a function boundary and arrives with unknown provenance (accept).

use crate::frontend::{Expr, ForSource, Stmt};
use std::collections::{BTreeMap, BTreeSet};

/// The only capability constructor (unforgeable: nothing else mints a tracked token).
const CAP_ACQUIRE: &str = "cap_acquire";
/// The authorized capability consumer (requires a live token; consumes it).
const CAP_USE: &str = "cap_use";

/// Liveness of a tracked capability local within one function body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CapState {
    Live,
    Consumed,
}

/// A linearity violation, routed by the caller through `emit(.., shadow_gated)`.
pub(crate) struct Finding {
    pub code: &'static str,
    pub message: String,
    pub span: Option<(usize, usize)>,
}

type CapMap = BTreeMap<String, CapState>;

struct Lin<'a> {
    verified: bool,
    span: (usize, usize),
    findings: &'a mut Vec<Finding>,
    /// User-function names, so a direct builtin call can be distinguished from a user call (a user
    /// function shadowing a builtin name must not be misclassified as performing that effect).
    all_fns: &'a BTreeSet<String>,
    /// Canonical capability ids genuinely acquired in this body — `cap_acquire("<id>")` with a
    /// string-literal kind bound to a local. The ONLY thing that authorizes an effect (unknown
    /// provenance never enters here). Path-insensitive union across branches.
    acquired: BTreeSet<String>,
    /// Canonical capability ids performed by DIRECT builtin calls in this body.
    performed: BTreeSet<String>,
}

impl Lin<'_> {
    fn reuse(&mut self, name: &str) {
        self.findings.push(Finding {
            code: "ANUBIS_CAPABILITY_REUSE",
            message: format!(
                "capability `{name}` is used after it was already consumed — a linear capability token may be used exactly once (non-duplicable)"
            ),
            span: Some(self.span),
        });
    }
    fn missing(&mut self) {
        self.findings.push(Finding {
            code: "ANUBIS_CAPABILITY_MISSING",
            message:
                "`cap_use` requires a live capability token, but its argument is not a capability (a capability can only be minted by `cap_acquire`, never conjured)"
                    .to_string(),
            span: Some(self.span),
        });
    }
}

/// Check one function body for capability linearity AND effect authorization (verified mode).
/// `params` seed the scope as unknown-provenance names (never tracked as capabilities — an incoming
/// param arrives with unknown provenance, so `cap_use(param)` accepts, and a param never authorizes
/// an effect). `all_fns` distinguishes direct builtin calls from user calls. Pure: returns findings.
pub(crate) fn check_linearity(
    _params: &[(String, String)],
    body: &[Stmt],
    verified: bool,
    span: (usize, usize),
    all_fns: &BTreeSet<String>,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut lin = Lin {
        verified,
        span,
        findings: &mut findings,
        all_fns,
        acquired: BTreeSet::new(),
        performed: BTreeSet::new(),
    };
    let mut caps: CapMap = BTreeMap::new();
    walk_stmts(body, &mut caps, &mut lin);
    // Composition (verified mode only): each directly-performed privileged effect must have a
    // matching genuine acquisition in this body. `performed \ acquired` is the unauthorized set.
    if verified {
        let unauthorized: Vec<String> = lin.performed.difference(&lin.acquired).cloned().collect();
        for effect in unauthorized {
            lin.findings.push(Finding {
                code: "ANUBIS_EFFECT_UNAUTHORIZED",
                message: format!(
                    "verification lane: function performs privileged effect `{effect}` but never acquires the capability authorizing it — acquire it with `cap_acquire(\"{effect}\")` in scope. An unknown-provenance value (a parameter, return, or non-literal) does not authorize an effect in verified mode."
                ),
                span: Some(span),
            });
        }
    }
    findings
}

/// A capability read-occurrence: `Live` → `Consumed`; a second consume is a reuse.
fn use_var(name: &str, caps: &mut CapMap, lin: &mut Lin) {
    match caps.get(name).copied() {
        Some(CapState::Live) => {
            caps.insert(name.to_string(), CapState::Consumed);
        }
        Some(CapState::Consumed) => lin.reuse(name),
        None => {} // not a tracked capability → unknown provenance, accept
    }
}

/// Whether `expr` is exactly `cap_acquire(...)` — the mint form for a `let`/assign initializer.
fn is_acquire(expr: &Expr) -> bool {
    matches!(expr, Expr::Call { callee, .. } if callee == CAP_ACQUIRE)
}

/// Whether `expr` is provably NOT a capability (a literal or pure arithmetic over such): the only
/// shape that makes `cap_use(expr)` a MISSING. A bare variable or any call is unknown-provenance
/// (accept), never provably-non-capability.
fn provably_non_capability(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(_) | Expr::StrLiteral(_) => true,
        Expr::Unary { expr, .. } => provably_non_capability(expr),
        Expr::Binary { op, lhs, rhs } => {
            matches!(op.as_str(), "+" | "-" | "*" | "/" | "%")
                && provably_non_capability(lhs)
                && provably_non_capability(rhs)
        }
        _ => false,
    }
}

/// Rebind `target` to the value of `init`, applying MOVE semantics when `init` is a bare tracked
/// capability variable, MINT when it is `cap_acquire(...)`, and otherwise walking `init` (which
/// consumes any capabilities inside it) and dropping any prior tracking of `target`.
fn rebind(target: &str, init: &Expr, caps: &mut CapMap, lin: &mut Lin) {
    if is_acquire(init) {
        if let Expr::Call { args, .. } = init {
            // A string-literal kind is what authorizes an effect (composition). A non-literal kind
            // authorizes nothing specific — fail-closed. Provenance is genuine: only cap_acquire
            // bound to a local ever populates `acquired`.
            if let Some(Expr::StrLiteral(kind)) = args.first() {
                lin.acquired.insert(super::normalize_effect_name(kind));
            }
            for a in args {
                walk_expr(a, caps, lin);
            }
        }
        caps.insert(target.to_string(), CapState::Live);
        return;
    }
    if let Expr::Var(src) = init {
        match caps.get(src).copied() {
            Some(CapState::Live) => {
                // MOVE: the token transfers from `src` to `target`, staying singular.
                caps.insert(src.clone(), CapState::Consumed);
                caps.insert(target.to_string(), CapState::Live);
                return;
            }
            Some(CapState::Consumed) => {
                // Moving out of an already-consumed token is a reuse; target holds nothing.
                lin.reuse(src);
                caps.remove(target);
                return;
            }
            None => {
                // `src` is unknown-provenance; `target` inherits nothing trackable.
                caps.remove(target);
                return;
            }
        }
    }
    walk_expr(init, caps, lin);
    caps.remove(target); // rebinding to a non-capability drops any prior capability tracking
}

fn walk_stmts(stmts: &[Stmt], caps: &mut CapMap, lin: &mut Lin) {
    for stmt in stmts {
        walk_stmt(stmt, caps, lin);
    }
}

fn walk_stmt(stmt: &Stmt, caps: &mut CapMap, lin: &mut Lin) {
    match stmt {
        Stmt::Let { name, init, .. } => rebind(name, init, caps, lin),
        Stmt::LetPattern { pattern, init, .. } => {
            walk_expr(init, caps, lin);
            for n in pattern.bound_names() {
                caps.remove(&n); // destructured bindings are not capabilities
            }
        }
        Stmt::Assign { target, value } => {
            if let Expr::Var(name) = target {
                rebind(name, value, caps, lin);
            } else {
                walk_expr(target, caps, lin);
                walk_expr(value, caps, lin);
            }
        }
        Stmt::If { cond, then, else_ } => {
            walk_expr(cond, caps, lin);
            let base = caps.clone();
            walk_stmts(then, caps, lin);
            let then_end = caps.clone();
            *caps = base.clone();
            let else_end = if let Some(else_body) = else_ {
                walk_stmts(else_body, caps, lin);
                Some(caps.clone())
            } else {
                None
            };
            let has_implicit_arm = else_end.is_none();
            let mut ends = vec![then_end];
            if let Some(e) = else_end {
                ends.push(e);
            }
            *caps = merge_branches(&base, &ends, has_implicit_arm, lin.verified);
        }
        Stmt::While {
            cond,
            body,
            invariant,
        } => {
            walk_expr(cond, caps, lin);
            for inv in invariant {
                walk_expr(inv, caps, lin);
            }
            walk_loop_body(body, caps, lin);
        }
        Stmt::Loop { body, invariant } => {
            for inv in invariant {
                walk_expr(inv, caps, lin);
            }
            walk_loop_body(body, caps, lin);
        }
        Stmt::WhileLet {
            pattern,
            expr,
            body,
        } => {
            walk_expr(expr, caps, lin);
            for n in pattern.bound_names() {
                caps.remove(&n);
            }
            walk_loop_body(body, caps, lin);
        }
        Stmt::For {
            var,
            source,
            body,
            invariant,
        } => {
            match source {
                ForSource::Range { start, end } => {
                    walk_expr(start, caps, lin);
                    walk_expr(end, caps, lin);
                }
                ForSource::Collection { expr } => walk_expr(expr, caps, lin),
            }
            for inv in invariant {
                walk_expr(inv, caps, lin);
            }
            caps.remove(var);
            walk_loop_body(body, caps, lin);
        }
        Stmt::ResearchBlock { body, .. } | Stmt::ExploitBlock { body, .. } => {
            walk_stmts(body, caps, lin);
        }
        Stmt::HybridBlock { gpu, cpu, prove } => {
            for b in [gpu, cpu, prove].iter().copied().flatten() {
                walk_stmts(b, caps, lin);
            }
        }
        Stmt::Break | Stmt::Continue | Stmt::SpecBlock { .. } => {}
        Stmt::ExprStmt(e) => walk_expr(e, caps, lin),
    }
}

/// Walk a loop body with loop-carried linearity: a capability that is Live at body entry and
/// becomes Consumed in the body would be re-consumed on the next iteration — a reuse the verified
/// lane rejects. A capability acquired *inside* the body is fresh per iteration (never flagged).
fn walk_loop_body(body: &[Stmt], caps: &mut CapMap, lin: &mut Lin) {
    let entry_live: BTreeSet<String> = caps
        .iter()
        .filter(|(_, s)| **s == CapState::Live)
        .map(|(k, _)| k.clone())
        .collect();
    let base = caps.clone();
    walk_stmts(body, caps, lin);
    if lin.verified {
        for name in &entry_live {
            if caps.get(name) == Some(&CapState::Consumed) {
                lin.reuse(name);
            }
        }
    }
    // Post-loop state over the pre-loop scope (drop body-local mints). The loop may run zero times,
    // so in the default lane a consumed-in-body capability stays Live (accept-bias); the verified
    // lane keeps it Consumed (fail-closed).
    let mut post = base.clone();
    if lin.verified {
        for name in base.keys() {
            if caps.get(name) == Some(&CapState::Consumed) {
                post.insert(name.clone(), CapState::Consumed);
            }
        }
    }
    *caps = post;
}

/// Merge the per-branch end states back over the pre-branch scope. Default lane = must-consume
/// (Consumed only if consumed on ALL arms); verified lane = may-consume (Consumed if consumed on
/// ANY arm). Capabilities minted inside a branch are block-local and dropped (only `base` keys
/// survive). `has_implicit_arm` adds a not-taken path that leaves the capability Live.
fn merge_branches(
    base: &CapMap,
    branch_ends: &[CapMap],
    has_implicit_arm: bool,
    verified: bool,
) -> CapMap {
    let mut out = base.clone();
    for (name, &base_state) in base.iter() {
        if base_state == CapState::Consumed {
            continue;
        }
        let mut states: Vec<CapState> = branch_ends
            .iter()
            .map(|b| b.get(name).copied().unwrap_or(base_state))
            .collect();
        if has_implicit_arm {
            states.push(base_state);
        }
        let consumed = if verified {
            states.contains(&CapState::Consumed)
        } else {
            !states.is_empty() && states.iter().all(|s| *s == CapState::Consumed)
        };
        if consumed {
            out.insert(name.clone(), CapState::Consumed);
        }
    }
    out
}

fn walk_expr(expr: &Expr, caps: &mut CapMap, lin: &mut Lin) {
    match expr {
        Expr::Var(name) => use_var(name, caps, lin),
        Expr::Literal(_)
        | Expr::StrLiteral(_)
        | Expr::Symbolic { .. }
        | Expr::TaintSource { .. }
        | Expr::UnifiedBuffer { .. }
        | Expr::RawPtr { .. }
        | Expr::Other(_) => {}
        Expr::Call { callee, args } => {
            for a in args {
                walk_expr(a, caps, lin);
            }
            if callee == CAP_USE {
                // The authorized consumer requires a live token. Its argument is consumed by the
                // walk above (or flagged reuse). A provably-non-capability argument is a MISSING —
                // a token conjured from nothing. Unknown provenance (a var/param/call) accepts.
                if let Some(arg) = args.first() {
                    if provably_non_capability(arg) {
                        lin.missing();
                    }
                }
            } else if !lin.all_fns.contains(callee) {
                // Composition: a DIRECT builtin call performing a privileged effect. The
                // `callee ∉ all_fns` guard means a user function shadowing a builtin name is a user
                // call, not a performed builtin effect. (cap_acquire/cap_use classify as None.)
                if let Some(effect) = super::effects::builtin_effect_of(callee) {
                    lin.performed.insert(effect.to_string());
                }
            }
        }
        Expr::CallExpr { callee, args } => {
            walk_expr(callee, caps, lin);
            for a in args {
                walk_expr(a, caps, lin);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            walk_expr(lhs, caps, lin);
            walk_expr(rhs, caps, lin);
        }
        Expr::Unary { expr, .. } | Expr::Cast { expr, .. } => walk_expr(expr, caps, lin),
        Expr::Tainted { inner, .. } | Expr::Declassify { inner, .. } => walk_expr(inner, caps, lin),
        Expr::Assume(inner) | Expr::Assert(inner) | Expr::Try(inner) => walk_expr(inner, caps, lin),
        Expr::ArrayLiteral { elements } => {
            for e in elements {
                walk_expr(e, caps, lin);
            }
        }
        Expr::Index { base, index } => {
            walk_expr(base, caps, lin);
            walk_expr(index, caps, lin);
        }
        Expr::StructLiteral { fields, .. } => {
            for (_, e) in fields {
                walk_expr(e, caps, lin);
            }
        }
        Expr::FieldAccess { base, .. } => walk_expr(base, caps, lin),
        Expr::EnumConstruct { fields, .. } => {
            for e in fields {
                walk_expr(e, caps, lin);
            }
        }
        Expr::MapLiteral { entries, .. } => {
            for (k, v) in entries {
                walk_expr(k, caps, lin);
                walk_expr(v, caps, lin);
            }
        }
        Expr::If {
            cond, then, else_, ..
        } => {
            walk_expr(cond, caps, lin);
            let base = caps.clone();
            walk_expr(then, caps, lin);
            let then_end = caps.clone();
            *caps = base.clone();
            walk_expr(else_, caps, lin);
            let else_end = caps.clone();
            *caps = merge_branches(&base, &[then_end, else_end], false, lin.verified);
        }
        Expr::IfLet {
            pattern,
            scrutinee,
            then,
            else_,
            ..
        } => {
            walk_expr(scrutinee, caps, lin);
            let base = caps.clone();
            for n in pattern.bound_names() {
                caps.remove(&n);
            }
            walk_expr(then, caps, lin);
            let then_end = caps.clone();
            *caps = base.clone();
            walk_expr(else_, caps, lin);
            let else_end = caps.clone();
            *caps = merge_branches(&base, &[then_end, else_end], false, lin.verified);
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            walk_expr(scrutinee, caps, lin);
            let base = caps.clone();
            let mut ends = Vec::new();
            for arm in arms {
                *caps = base.clone();
                for n in arm.pattern.bound_names() {
                    caps.remove(&n);
                }
                if let Some(g) = &arm.guard {
                    walk_expr(g, caps, lin);
                }
                walk_expr(&arm.body, caps, lin);
                ends.push(caps.clone());
            }
            // A match may or may not be exhaustive; assume a not-taken path (accept-biased default).
            *caps = merge_branches(&base, &ends, true, lin.verified);
        }
        Expr::Block { stmts, tail } => {
            walk_stmts(stmts, caps, lin);
            if let Some(t) = tail {
                walk_expr(t, caps, lin);
            }
        }
        Expr::Lambda { .. } => {
            // A closure literal performs nothing at its definition site; a capability captured by a
            // closure crosses the function boundary (intraprocedural boundary — accepted).
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend;

    fn findings_of(src: &str, verified: bool) -> Vec<Finding> {
        let ast = frontend::parse_source(src).expect("parse");
        let mut all_fns = BTreeSet::new();
        for item in &ast.items {
            if let frontend::Item::Fn { name, .. } = item {
                all_fns.insert(name.clone());
            }
        }
        let mut out = Vec::new();
        for item in &ast.items {
            if let frontend::Item::Fn {
                params, body, span, ..
            } = item
            {
                out.extend(check_linearity(
                    params,
                    body,
                    verified,
                    (span.start, span.end),
                    &all_fns,
                ));
            }
        }
        out
    }

    fn codes(src: &str, verified: bool) -> Vec<&'static str> {
        findings_of(src, verified).iter().map(|f| f.code).collect()
    }

    #[test]
    fn straight_line_double_use_is_reuse_in_both_lanes() {
        let src = r#"fn main() { let c = cap_acquire("fs.read"); cap_use(c); cap_use(c); }"#;
        for verified in [false, true] {
            assert_eq!(
                codes(src, verified),
                ["ANUBIS_CAPABILITY_REUSE"],
                "verified={verified}"
            );
        }
    }

    #[test]
    fn used_once_and_surrendered_accept() {
        assert!(codes(
            r#"fn main() { let c = cap_acquire("fs.read"); cap_use(c); }"#,
            true
        )
        .is_empty());
        // Passing the token to a callee consumes it here; not used again → accept.
        assert!(codes(
            r#"fn main() { let c = cap_acquire("fs.read"); sink(c); }"#,
            true
        )
        .is_empty());
    }

    #[test]
    fn missing_on_provable_non_capability() {
        assert_eq!(
            codes(r#"fn main() { cap_use(5); }"#, false),
            ["ANUBIS_CAPABILITY_MISSING"]
        );
        assert_eq!(
            codes(r#"fn main() { cap_use(1 + 2); }"#, false),
            ["ANUBIS_CAPABILITY_MISSING"]
        );
        // Unknown provenance (a param) → accept in both lanes (never MISSING on ignorance).
        assert!(codes(r#"fn handler(c) { cap_use(c); }"#, true).is_empty());
    }

    #[test]
    fn move_on_rebind_keeps_the_token_singular() {
        // Use of the moved-from variable after the move is a reuse (aliasing cannot launder).
        assert_eq!(
            codes(
                r#"fn main() { let c = cap_acquire("fs.read"); let y = c; cap_use(c); }"#,
                false
            ),
            ["ANUBIS_CAPABILITY_REUSE"]
        );
        // Using the token via its new name exactly once accepts.
        assert!(codes(
            r#"fn main() { let c = cap_acquire("fs.read"); let y = c; cap_use(y); }"#,
            true
        )
        .is_empty());
    }

    #[test]
    fn aggregate_double_use_is_reuse() {
        assert_eq!(
            codes(
                r#"fn main() { let c = cap_acquire("fs.read"); let pair = [c, c]; }"#,
                false
            ),
            ["ANUBIS_CAPABILITY_REUSE"]
        );
    }

    #[test]
    fn branch_dual_default_accepts_verified_rejects() {
        let src = r#"fn f(cond) { let c = cap_acquire("x"); if cond { cap_use(c); } cap_use(c); }"#;
        assert!(
            codes(src, false).is_empty(),
            "default lane must-consume accepts"
        );
        assert_eq!(
            codes(src, true),
            ["ANUBIS_CAPABILITY_REUSE"],
            "verified lane may-consume rejects"
        );
    }

    #[test]
    fn both_branches_consume_is_linear_not_reuse() {
        // Consuming in exactly one arm at runtime is linear; sequential-walk must not false-fire.
        let src = r#"fn f(cond) { let c = cap_acquire("x"); if cond { cap_use(c); } else { cap_use(c); } }"#;
        assert!(codes(src, false).is_empty());
        assert!(codes(src, true).is_empty());
    }

    #[test]
    fn loop_carried_consume_rejects_in_verified_only() {
        let carried = r#"fn f() { let c = cap_acquire("x"); for i in 0..3 { cap_use(c); } }"#;
        assert!(
            codes(carried, false).is_empty(),
            "default lane accepts (loop may not run)"
        );
        assert_eq!(
            codes(carried, true),
            ["ANUBIS_CAPABILITY_REUSE"],
            "verified lane rejects loop-carried consume"
        );
        // A fresh token minted each iteration is linear in both lanes.
        let fresh = r#"fn f() { for i in 0..3 { let c = cap_acquire("x"); cap_use(c); } }"#;
        assert!(codes(fresh, false).is_empty());
        assert!(codes(fresh, true).is_empty());
    }

    // ── Slice 3: effect authorization (composition) ─────────────────────────────────────────────

    #[test]
    fn verified_effect_without_acquisition_is_unauthorized() {
        // Performs net.send directly with no genuine acquisition → UNAUTHORIZED in verified only.
        let src = r#"fn f() { send("h", 80, "x"); }"#;
        assert!(
            codes(src, false).is_empty(),
            "default lane requires no authorization"
        );
        assert_eq!(codes(src, true), ["ANUBIS_EFFECT_UNAUTHORIZED"]);
    }

    #[test]
    fn verified_effect_with_genuine_acquisition_is_authorized() {
        // A matching cap_acquire in scope authorizes the effect; the token is used once (linear).
        let src = r#"fn f() { let n = cap_acquire("net.send"); send("h", 80, "x"); cap_use(n); }"#;
        assert!(codes(src, true).is_empty());
    }

    #[test]
    fn param_capability_does_not_authorize_an_effect() {
        // THE FORGE: an unknown-provenance param is not a genuine acquisition. Performing net with
        // only `netcap` in hand is UNAUTHORIZED in verified; the same body accepts in default lane.
        let src = r#"fn f(netcap) { send("h", 80, "x"); }"#;
        assert!(codes(src, false).is_empty());
        assert_eq!(codes(src, true), ["ANUBIS_EFFECT_UNAUTHORIZED"]);
    }

    #[test]
    fn user_fn_shadowing_a_builtin_name_is_not_a_performed_effect() {
        // `send` is a user function here (in all_fns), so calling it is a user call, not a net
        // effect — no authorization required.
        let src = r#"fn send(x) { return x; }
fn f() { let y = send(3); }"#;
        assert!(codes(src, true).is_empty());
    }

    #[test]
    fn non_literal_acquisition_kind_does_not_authorize() {
        // A non-literal cap_acquire kind authorizes nothing specific (fail-closed).
        let src = r#"fn f(kind) { let n = cap_acquire(kind); send("h", 80, "x"); cap_use(n); }"#;
        assert_eq!(codes(src, true), ["ANUBIS_EFFECT_UNAUTHORIZED"]);
    }
}
