//! Phase-2 slice 2: capability tokens as linear (use-once) values.
//!
//! A capability is an unforgeable linear token: minted only by `cap_acquire(...)` /
//! `cap_acquire_nonexportable(...)`, used exactly once, non-duplicable, and surrendered when
//! passed away. This module is the intraprocedural linearity checker that proves that discipline —
//! the Austral half of the capability-and-effect fusion.
//!
//! Phase-2 slice 3 (composition) joins this surface to the effect row: in VERIFIED mode, a function
//! that DIRECTLY performs a privileged effect must **causally spend** a live local capability of the
//! matching kind at the effect site (`ANUBIS_EFFECT_UNAUTHORIZED` if no live matching-kind token
//! exists). Kind comes only from a string-literal acquire kind — a parameter, return, or non-literal
//! value does NOT authorize (closes the forge vector). Direct-builtins-only: transitive effects
//! through callees are the callee's to authorize (interprocedural cap flow is residual).
//!
//! Non-exportable tokens (Depth / Keychain micro-slice): `cap_acquire_nonexportable("kind")` mints
//! a token that may still **authorize** effects by causal spend (token not in the arg list), but
//! must not flow **as data** to public sinks (`print` / `send` / `http_*` / `write_file`) without a
//! well-formed `cap_export(c, "reason")` peel (`ANUBIS_CAPABILITY_EXPORT`). Ordinary
//! `cap_acquire` stays exportable (compat). Keychain/SE hardware isolation is residual — not claimed
//! by this static discipline.
//!
//! Core rule: a *use* is any read-occurrence of a tracked capability variable, **except** a plain
//! rebind (`let y = c` / `y = c`), which MOVES the token (source consumed, target live). That one
//! unified definition closes aliasing (`let y = c`), aggregate (`[c, c]`), and per-occurrence
//! (`foo(c, c)`) laundering together — every other occurrence consumes, and a second consume is
//! `ANUBIS_CAPABILITY_REUSE`. `cap_use` on a provable non-capability is `ANUBIS_CAPABILITY_MISSING`.
//! A privileged builtin in verified mode also **consumes** a live matching-kind token (causal spend).
//!
//! Dual-mode, both directions held at once:
//!   - the REJECT DECISION is accept-biased (default lane): unknown provenance and uncertain
//!     consumption never produce a spurious reuse/missing rejection; Safe does **not** require
//!     causal spend (declaration-gated via `uses(...)` elsewhere); unknown-provenance never
//!     invents `non_exportable` (accept-biased on sealedness);
//!   - the LINEARITY resolves toward CONSUMED on uncertainty (verified lane): a branch may-consume
//!     and a loop-carried consume are treated as consumed, so a genuine reuse is never hidden.
//!
//! Self-contained and pure, mirroring effects.rs — it is one of the pieces Phase 4 ports into
//! Anubis itself. Linearity / causal spend remain INTRAPROCEDURAL for unknown-provenance
//! tokens (params still do not authorize effects). **Non-exportable sealedness** is closed
//! interprocedurally for the two named shapes: (1) a callee that returns a non-exportable mint
//! seeds the caller's binding as non-exportable; (2) a callee formal that reaches a public sink
//! without `cap_export` causes the call site to treat the corresponding arg as an export check.
//! Closure capture of caps and deep HO rebind remain residual.

use crate::frontend::{Expr, ForSource, Stmt};
use std::collections::{BTreeMap, BTreeSet};

/// Exportable capability constructor (unforgeable: nothing else mints a tracked token).
const CAP_ACQUIRE: &str = "cap_acquire";
/// Non-exportable mint: same as acquire, plus `non_exportable: true` (static; not Keychain/SE).
const CAP_ACQUIRE_NONEXPORTABLE: &str = "cap_acquire_nonexportable";
/// Peel non_exportable after a well-formed string-literal reason (mirror declassify discipline).
const CAP_EXPORT: &str = "cap_export";
/// The authorized capability consumer (requires a live token; consumes it).
const CAP_USE: &str = "cap_use";

/// Public sinks where a non-exportable token-as-argument is `ANUBIS_CAPABILITY_EXPORT`.
/// Minimal under-grant list — do not invent OS-level sinks here.
const EXPORT_SINKS: &[&str] = &["print", "send", "http_post", "http_get", "write_file"];

/// Liveness of a tracked capability local within one function body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CapState {
    Live,
    Consumed,
}

/// Tracked local capability: state + optional kind + non-exportable flag.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CapToken {
    state: CapState,
    /// Canonical effect kind when minted from a string-literal acquire; None for unknown provenance
    /// paths that should not authorize effects.
    kind: Option<String>,
    /// When true, token value must not appear as an argument to an export sink without
    /// `cap_export`. Causal spend (ambient authorization) is still allowed.
    non_exportable: bool,
}

/// A linearity / authorization violation, routed by the caller through `emit(.., shadow_gated)`.
pub(crate) struct Finding {
    pub code: &'static str,
    pub message: String,
    pub span: Option<(usize, usize)>,
}

type CapMap = BTreeMap<String, CapToken>;

struct Lin<'a> {
    verified: bool,
    span: (usize, usize),
    findings: &'a mut Vec<Finding>,
    /// User-function names, so a direct builtin call can be distinguished from a user call (a user
    /// function shadowing a builtin name must not be misclassified as performing that effect).
    all_fns: &'a BTreeSet<String>,
    /// Program-level non_exportable sealedness summaries (return + exporting formals).
    summary: &'a CapProgramSummary,
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
                "`cap_use` requires a live capability token, but its argument is not a capability (a capability can only be minted by `cap_acquire` / `cap_acquire_nonexportable`, never conjured)"
                    .to_string(),
            span: Some(self.span),
        });
    }
    fn unauthorized(&mut self, effect: &str) {
        self.findings.push(Finding {
            code: "ANUBIS_EFFECT_UNAUTHORIZED",
            message: format!(
                "verification lane: privileged effect `{effect}` requires a live capability token of kind `{effect}` at the use site (causal spend) — acquire it with `cap_acquire(\"{effect}\")` or `cap_acquire_nonexportable(\"{effect}\")` and hold it live until the effect. An unknown-provenance value (a parameter, return, or non-literal) does not authorize an effect in verified mode."
            ),
            span: Some(self.span),
        });
    }
    fn export(&mut self, name: &str) {
        self.findings.push(Finding {
            code: "ANUBIS_CAPABILITY_EXPORT",
            message: format!(
                "non-exportable capability `{name}` cannot flow as data to a public sink (print/send/http_*/write_file) — causal spend for effect authorization is still allowed when the token is not an argument; peel with `cap_export({name}, \"reason\")` before exporting the token value (language-level release; Keychain/SE hardware isolation is residual)"
            ),
            span: Some(self.span),
        });
    }
    fn export_malformed(&mut self) {
        self.findings.push(Finding {
            code: "ANUBIS_CAPABILITY_EXPORT_MALFORMED",
            message:
                "`cap_export` requires a live non-exportable capability and a non-empty string-literal reason — malformed release does not peel the non-exportable flag"
                    .to_string(),
            span: Some(self.span),
        });
    }
}

/// Whole-program summaries for non-exportable sealedness across call edges.
#[derive(Default, Clone, Debug)]
pub(crate) struct CapProgramSummary {
    /// User functions whose return value is a non-exportable capability (mint or alias).
    returns_nonexportable: BTreeSet<String>,
    /// User function → formal indices that flow as data to an export sink without peel.
    exports_formals: BTreeMap<String, BTreeSet<usize>>,
}

/// Check every function: linearity + verified causal spend + interproc non_exportable sealedness.
pub(crate) fn check_program(
    items: &[crate::frontend::Item],
    verified: bool,
) -> Vec<Finding> {
    let mut all_fns = BTreeSet::new();
    let mut fns: Vec<(&str, &[(String, String)], &[Stmt], (usize, usize))> = Vec::new();
    for item in items {
        if let crate::frontend::Item::Fn {
            name,
            params,
            body,
            span,
            ..
        } = item
        {
            all_fns.insert(name.clone());
            fns.push((name.as_str(), params.as_slice(), body.as_slice(), (span.start, span.end)));
        }
    }
    let summary = build_cap_program_summary_from_fns(&fns, &all_fns);
    let mut out = Vec::new();
    for (name, params, body, span) in fns {
        out.extend(check_linearity(
            params,
            body,
            verified,
            span,
            &all_fns,
            &summary,
            name,
        ));
    }
    out
}

/// Check one function body for capability linearity AND effect authorization (verified mode).
/// `params` seed the scope as unknown-provenance names for *authorization* (a param never
/// authorizes an effect). Non-exportable sealedness uses [`CapProgramSummary`] for call/return
/// edges. Pure: returns findings.
pub(crate) fn check_linearity(
    _params: &[(String, String)],
    body: &[Stmt],
    verified: bool,
    span: (usize, usize),
    all_fns: &BTreeSet<String>,
    summary: &CapProgramSummary,
    _fn_name: &str,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut lin = Lin {
        verified,
        span,
        findings: &mut findings,
        all_fns,
        summary,
    };
    let mut caps: CapMap = BTreeMap::new();
    walk_stmts(body, &mut caps, &mut lin);
    findings
}

pub(crate) fn program_summary(items: &[crate::frontend::Item]) -> CapProgramSummary {
    let mut all_fns = BTreeSet::new();
    let mut fns: Vec<(&str, &[(String, String)], &[Stmt], (usize, usize))> = Vec::new();
    collect_fn_refs(items, &mut all_fns, &mut fns);
    build_cap_program_summary_from_fns(&fns, &all_fns)
}

fn collect_fn_refs<'a>(
    items: &'a [crate::frontend::Item],
    all_fns: &mut BTreeSet<String>,
    fns: &mut Vec<(&'a str, &'a [(String, String)], &'a [Stmt], (usize, usize))>,
) {
    for item in items {
        match item {
            crate::frontend::Item::Fn {
                name,
                params,
                body,
                span,
                ..
            } => {
                all_fns.insert(name.clone());
                fns.push((
                    name.as_str(),
                    params.as_slice(),
                    body.as_slice(),
                    (span.start, span.end),
                ));
            }
            crate::frontend::Item::Module { items, .. } => collect_fn_refs(items, all_fns, fns),
            crate::frontend::Item::Impl { methods, .. } => {
                for m in methods {
                    if let crate::frontend::Item::Fn {
                        name,
                        params,
                        body,
                        span,
                        ..
                    } = m
                    {
                        all_fns.insert(name.clone());
                        fns.push((
                            name.as_str(),
                            params.as_slice(),
                            body.as_slice(),
                            (span.start, span.end),
                        ));
                    }
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn build_cap_program_summary_from_fns(
    fns: &[(&str, &[(String, String)], &[Stmt], (usize, usize))],
    all_fns: &BTreeSet<String>,
) -> CapProgramSummary {
    let mut summary = CapProgramSummary::default();
    // Formals that reach export sinks are independent of the return-NE fixpoint.
    for (name, params, body, _) in fns {
        let exported = formals_reaching_export_sink(params, body);
        if !exported.is_empty() {
            summary.exports_formals.insert((*name).to_string(), exported);
        }
    }
    // Fixpoint: returns_nonexportable may depend on callees.
    loop {
        let before = summary.returns_nonexportable.len();
        for (name, _params, body, _) in fns {
            if summary.returns_nonexportable.contains(*name) {
                continue;
            }
            if body_returns_nonexportable(body, all_fns, &summary.returns_nonexportable) {
                summary.returns_nonexportable.insert((*name).to_string());
            }
        }
        if summary.returns_nonexportable.len() == before {
            break;
        }
    }
    summary
}

/// Whether this body can return a non-exportable token (mint, move alias, or call of NE-returning fn).
fn body_returns_nonexportable(
    body: &[Stmt],
    all_fns: &BTreeSet<String>,
    returns_ne: &BTreeSet<String>,
) -> bool {
    let mut ne: BTreeSet<String> = BTreeSet::new();
    fn track_init(
        name: &str,
        init: &Expr,
        ne: &mut BTreeSet<String>,
        all_fns: &BTreeSet<String>,
        returns_ne: &BTreeSet<String>,
    ) {
        if is_nonexportable_acquire(init) {
            ne.insert(name.to_string());
            return;
        }
        if let Expr::Var(src) = init {
            if ne.contains(src) {
                ne.remove(src);
                ne.insert(name.to_string());
            } else {
                ne.remove(name);
            }
            return;
        }
        if let Expr::Call { callee, .. } = init {
            if all_fns.contains(callee) && returns_ne.contains(callee) {
                ne.insert(name.to_string());
                return;
            }
        }
        ne.remove(name);
    }
    fn expr_is_ne(
        e: &Expr,
        ne: &BTreeSet<String>,
        all_fns: &BTreeSet<String>,
        returns_ne: &BTreeSet<String>,
    ) -> bool {
        if is_nonexportable_acquire(e) {
            return true;
        }
        match e {
            Expr::Var(n) => ne.contains(n),
            Expr::Call { callee, .. } => all_fns.contains(callee) && returns_ne.contains(callee),
            _ => false,
        }
    }
    fn walk(
        stmts: &[Stmt],
        ne: &mut BTreeSet<String>,
        all_fns: &BTreeSet<String>,
        returns_ne: &BTreeSet<String>,
    ) -> bool {
        for s in stmts {
            match s {
                Stmt::Let { name, init, .. } => {
                    track_init(name, init, ne, all_fns, returns_ne);
                }
                Stmt::Assign {
                    target: Expr::Var(name),
                    value,
                } => {
                    track_init(name, value, ne, all_fns, returns_ne);
                }
                Stmt::ExprStmt(Expr::Call { callee, args }) if callee == "return" => {
                    if let Some(e) = args.first() {
                        if expr_is_ne(e, ne, all_fns, returns_ne) {
                            return true;
                        }
                    }
                }
                Stmt::If { then, else_, .. } => {
                    let mut tne = ne.clone();
                    if walk(then, &mut tne, all_fns, returns_ne) {
                        return true;
                    }
                    if let Some(e) = else_ {
                        let mut ene = ne.clone();
                        if walk(e, &mut ene, all_fns, returns_ne) {
                            return true;
                        }
                        // merge aliases conservatively (union of NE names)
                        for n in tne.intersection(&ene).cloned().collect::<Vec<_>>() {
                            ne.insert(n);
                        }
                    }
                }
                Stmt::ExprStmt(Expr::Block { stmts: bs, tail }) => {
                    if walk(bs, ne, all_fns, returns_ne) {
                        return true;
                    }
                    if let Some(t) = tail {
                        if expr_is_ne(t, ne, all_fns, returns_ne) {
                            return true;
                        }
                    }
                }
                _ => {}
            }
        }
        false
    }
    walk(body, &mut ne, all_fns, returns_ne)
}

/// Formal indices that reach print/send/http_*/write_file as data without a prior cap_export peel.
fn formals_reaching_export_sink(
    params: &[(String, String)],
    body: &[Stmt],
) -> BTreeSet<usize> {
    // name -> formal index still sealed (not peeled)
    let mut sealed: BTreeMap<String, usize> = BTreeMap::new();
    for (i, (n, _)) in params.iter().enumerate() {
        sealed.insert(n.clone(), i);
    }
    let mut exported = BTreeSet::new();
    fn peel_name(name: &str, sealed: &mut BTreeMap<String, usize>) {
        sealed.remove(name);
    }
    fn on_export_arg(e: &Expr, sealed: &BTreeMap<String, usize>, exported: &mut BTreeSet<usize>) {
        if let Expr::Var(n) = e {
            if let Some(i) = sealed.get(n) {
                exported.insert(*i);
            }
        }
    }
    fn walk(
        stmts: &[Stmt],
        sealed: &mut BTreeMap<String, usize>,
        exported: &mut BTreeSet<usize>,
    ) {
        for s in stmts {
            match s {
                Stmt::Let { name, init, .. } => match init {
                    Expr::Var(src) => {
                        if let Some(i) = sealed.get(src).cloned() {
                            sealed.remove(src);
                            sealed.insert(name.clone(), i);
                        } else {
                            sealed.remove(name);
                        }
                    }
                    Expr::Call { callee, args } if callee == CAP_EXPORT => {
                        if let Some(Expr::Var(src)) = args.first() {
                            peel_name(src, sealed);
                        }
                        sealed.remove(name);
                    }
                    _ => {
                        sealed.remove(name);
                    }
                },
                Stmt::Assign {
                    target: Expr::Var(name),
                    value,
                } => match value {
                    Expr::Var(src) => {
                        if let Some(i) = sealed.get(src).cloned() {
                            sealed.remove(src);
                            sealed.insert(name.clone(), i);
                        } else {
                            sealed.remove(name);
                        }
                    }
                    Expr::Call { callee, args } if callee == CAP_EXPORT => {
                        if let Some(Expr::Var(src)) = args.first() {
                            peel_name(src, sealed);
                        }
                        sealed.remove(name);
                    }
                    _ => {
                        sealed.remove(name);
                    }
                },
                Stmt::ExprStmt(e) => walk_expr_export(e, sealed, exported),
                Stmt::If { then, else_, .. } => {
                    let mut s1 = sealed.clone();
                    walk(then, &mut s1, exported);
                    if let Some(el) = else_ {
                        let mut s2 = sealed.clone();
                        walk(el, &mut s2, exported);
                    }
                }
                Stmt::While { body, .. }
                | Stmt::Loop { body, .. }
                | Stmt::For { body, .. }
                | Stmt::WhileLet { body, .. }
                | Stmt::ResearchBlock { body, .. }
                | Stmt::ExploitBlock { body, .. } => {
                    walk(body, sealed, exported);
                }
                _ => {}
            }
        }
    }
    fn walk_expr_export(
        e: &Expr,
        sealed: &mut BTreeMap<String, usize>,
        exported: &mut BTreeSet<usize>,
    ) {
        match e {
            Expr::Call { callee, args } => {
                if callee == CAP_EXPORT {
                    if let Some(Expr::Var(src)) = args.first() {
                        peel_name(src, sealed);
                    }
                } else if is_export_sink(callee) {
                    for a in args {
                        on_export_arg(a, sealed, exported);
                    }
                }
                for a in args {
                    walk_expr_export(a, sealed, exported);
                }
            }
            Expr::CallExpr { callee, args } => {
                walk_expr_export(callee, sealed, exported);
                for a in args {
                    walk_expr_export(a, sealed, exported);
                }
            }
            Expr::Binary { lhs, rhs, .. } => {
                walk_expr_export(lhs, sealed, exported);
                walk_expr_export(rhs, sealed, exported);
            }
            Expr::Unary { expr, .. }
            | Expr::Cast { expr, .. }
            | Expr::Tainted { inner: expr, .. }
            | Expr::Declassify { inner: expr, .. }
            | Expr::Assume(expr)
            | Expr::Assert(expr)
            | Expr::Try(expr) => walk_expr_export(expr, sealed, exported),
            Expr::ArrayLiteral { elements } => {
                for el in elements {
                    walk_expr_export(el, sealed, exported);
                }
            }
            Expr::Block { stmts, tail } => {
                walk(stmts, sealed, exported);
                if let Some(t) = tail {
                    walk_expr_export(t, sealed, exported);
                }
            }
            Expr::If {
                cond, then, else_, ..
            } => {
                walk_expr_export(cond, sealed, exported);
                walk_expr_export(then, sealed, exported);
                walk_expr_export(else_, sealed, exported);
            }
            _ => {}
        }
    }
    walk(body, &mut sealed, &mut exported);
    exported
}

/// A capability read-occurrence: `Live` → `Consumed`; a second consume is a reuse.
fn use_var(name: &str, caps: &mut CapMap, lin: &mut Lin) {
    match caps.get(name).cloned() {
        Some(t) if t.state == CapState::Live => {
            caps.insert(
                name.to_string(),
                CapToken {
                    state: CapState::Consumed,
                    kind: t.kind,
                    non_exportable: t.non_exportable,
                },
            );
        }
        Some(t) if t.state == CapState::Consumed => lin.reuse(name),
        _ => {} // not a tracked capability → unknown provenance, accept
    }
}

/// Whether `expr` is exactly `cap_acquire(...)` or `cap_acquire_nonexportable(...)`.
fn is_acquire(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Call { callee, .. }
            if callee == CAP_ACQUIRE || callee == CAP_ACQUIRE_NONEXPORTABLE
    )
}

fn is_nonexportable_acquire(expr: &Expr) -> bool {
    matches!(expr, Expr::Call { callee, .. } if callee == CAP_ACQUIRE_NONEXPORTABLE)
}

fn is_cap_export(expr: &Expr) -> bool {
    matches!(expr, Expr::Call { callee, .. } if callee == CAP_EXPORT)
}

fn is_export_sink(callee: &str) -> bool {
    EXPORT_SINKS.contains(&callee)
}

/// If `expr` is a bare var naming a Live non-exportable token, or a call of a function that
/// returns non-exportable, fire EXPORT (does not consume).
fn check_export_arg(expr: &Expr, caps: &CapMap, lin: &mut Lin) {
    match expr {
        Expr::Var(name) => {
            if let Some(t) = caps.get(name) {
                if t.state == CapState::Live && t.non_exportable {
                    lin.export(name);
                }
            }
        }
        Expr::Call { callee, .. }
            if lin.all_fns.contains(callee)
                && lin.summary.returns_nonexportable.contains(callee) =>
        {
            lin.export(callee);
        }
        _ => {}
    }
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

/// Causal spend: consume one Live token whose kind matches `effect` (verified only).
/// Deterministic: lexicographically first matching name. No match → UNAUTHORIZED.
fn causal_spend(effect: &str, caps: &mut CapMap, lin: &mut Lin) {
    let mut matches: Vec<String> = caps
        .iter()
        .filter(|(_, t)| t.state == CapState::Live && t.kind.as_deref() == Some(effect))
        .map(|(n, _)| n.clone())
        .collect();
    matches.sort();
    if let Some(name) = matches.first() {
        if let Some(t) = caps.get(name).cloned() {
            caps.insert(
                name.clone(),
                CapToken {
                    state: CapState::Consumed,
                    kind: t.kind,
                    non_exportable: t.non_exportable,
                },
            );
        }
    } else {
        lin.unauthorized(effect);
    }
}

/// Well-formed `cap_export(src, "reason")`: on success consume `src` and return peeled Live
/// exportable token. Malformed → diagnostic, leave source unpeeled (if still Live).
fn apply_cap_export(args: &[Expr], caps: &mut CapMap, lin: &mut Lin) -> Option<CapToken> {
    let src_name = match args.first() {
        Some(Expr::Var(n)) => n.clone(),
        _ => {
            lin.export_malformed();
            for a in args {
                walk_expr(a, caps, lin);
            }
            return None;
        }
    };
    let reason_ok = matches!(args.get(1), Some(Expr::StrLiteral(r)) if !r.is_empty());
    match caps.get(&src_name).cloned() {
        Some(t) if t.state == CapState::Live && t.non_exportable && reason_ok => {
            caps.insert(
                src_name,
                CapToken {
                    state: CapState::Consumed,
                    kind: t.kind.clone(),
                    non_exportable: true,
                },
            );
            for a in args.iter().skip(2) {
                walk_expr(a, caps, lin);
            }
            Some(CapToken {
                state: CapState::Live,
                kind: t.kind,
                non_exportable: false,
            })
        }
        Some(t) if t.state == CapState::Consumed => {
            lin.reuse(&src_name);
            for a in args.iter().skip(1) {
                walk_expr(a, caps, lin);
            }
            None
        }
        _ => {
            // Empty reason, exportable token, unknown provenance, etc. — no peel.
            lin.export_malformed();
            for a in args.iter().skip(1) {
                walk_expr(a, caps, lin);
            }
            None
        }
    }
}

/// Rebind `target` to the value of `init`, applying MOVE semantics when `init` is a bare tracked
/// capability variable, MINT when it is acquire / nonexportable, PEEL when `cap_export(...)`, and
/// otherwise walking `init` (which consumes any capabilities inside it) and dropping any prior
/// tracking of `target`.
fn rebind(target: &str, init: &Expr, caps: &mut CapMap, lin: &mut Lin) {
    if is_acquire(init) {
        let mut kind: Option<String> = None;
        let non_exportable = is_nonexportable_acquire(init);
        if let Expr::Call { args, .. } = init {
            // A string-literal kind is what authorizes an effect (composition). A non-literal kind
            // authorizes nothing specific — fail-closed.
            if let Some(Expr::StrLiteral(k)) = args.first() {
                kind = Some(super::normalize_effect_name(k));
            }
            for a in args {
                walk_expr(a, caps, lin);
            }
        }
        caps.insert(
            target.to_string(),
            CapToken {
                state: CapState::Live,
                kind,
                non_exportable,
            },
        );
        return;
    }
    if is_cap_export(init) {
        if let Expr::Call { args, .. } = init {
            if let Some(peeled) = apply_cap_export(args, caps, lin) {
                caps.insert(target.to_string(), peeled);
            } else {
                caps.remove(target);
            }
            return;
        }
    }
    if let Expr::Var(src) = init {
        match caps.get(src).cloned() {
            Some(t) if t.state == CapState::Live => {
                // MOVE: the token transfers from `src` to `target`, staying singular (kind + flag).
                caps.insert(
                    src.clone(),
                    CapToken {
                        state: CapState::Consumed,
                        kind: t.kind.clone(),
                        non_exportable: t.non_exportable,
                    },
                );
                caps.insert(
                    target.to_string(),
                    CapToken {
                        state: CapState::Live,
                        kind: t.kind,
                        non_exportable: t.non_exportable,
                    },
                );
                return;
            }
            Some(t) if t.state == CapState::Consumed => {
                lin.reuse(src);
                caps.remove(target);
                return;
            }
            _ => {
                caps.remove(target);
                return;
            }
        }
    }
    // Interproc sealedness: `let s = mint()` where mint returns non_exportable.
    if let Expr::Call { callee, args } = init {
        if lin.all_fns.contains(callee) && lin.summary.returns_nonexportable.contains(callee) {
            for a in args {
                walk_expr(a, caps, lin);
            }
            caps.insert(
                target.to_string(),
                CapToken {
                    state: CapState::Live,
                    kind: None, // kind not needed for export check
                    non_exportable: true,
                },
            );
            return;
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
        .filter(|(_, t)| t.state == CapState::Live)
        .map(|(k, _)| k.clone())
        .collect();
    let base = caps.clone();
    walk_stmts(body, caps, lin);
    if lin.verified {
        for name in &entry_live {
            if caps.get(name).map(|t| t.state) == Some(CapState::Consumed) {
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
            if caps.get(name).map(|t| t.state) == Some(CapState::Consumed) {
                if let Some(t) = post.get(name).cloned() {
                    post.insert(
                        name.clone(),
                        CapToken {
                            state: CapState::Consumed,
                            kind: t.kind,
                            non_exportable: t.non_exportable,
                        },
                    );
                }
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
    for (name, base_tok) in base.iter() {
        if base_tok.state == CapState::Consumed {
            continue;
        }
        let mut states: Vec<CapState> = branch_ends
            .iter()
            .map(|b| b.get(name).map(|t| t.state).unwrap_or(base_tok.state))
            .collect();
        if has_implicit_arm {
            states.push(base_tok.state);
        }
        let consumed = if verified {
            states.contains(&CapState::Consumed)
        } else {
            !states.is_empty() && states.iter().all(|s| *s == CapState::Consumed)
        };
        if consumed {
            out.insert(
                name.clone(),
                CapToken {
                    state: CapState::Consumed,
                    kind: base_tok.kind.clone(),
                    non_exportable: base_tok.non_exportable,
                },
            );
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
            // In-place peel: `cap_export(s, "reason");` rebinds `s` to the exportable token.
            if callee == CAP_EXPORT {
                if let Some(peeled) = apply_cap_export(args, caps, lin) {
                    if let Some(Expr::Var(n)) = args.first() {
                        caps.insert(n.clone(), peeled);
                    }
                }
                return;
            }
            // Export check BEFORE walking args (so Live non_exportable is still visible).
            if is_export_sink(callee) && !lin.all_fns.contains(callee) {
                for a in args {
                    check_export_arg(a, caps, lin);
                }
            }
            // Interproc: callee formals that reach export sinks → check corresponding args.
            if lin.all_fns.contains(callee) {
                if let Some(idxs) = lin.summary.exports_formals.get(callee) {
                    for &i in idxs {
                        if let Some(a) = args.get(i) {
                            check_export_arg(a, caps, lin);
                        }
                    }
                }
            }
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
                // Composition: a DIRECT builtin call performing a privileged effect.
                // Verified: causal spend of a live matching-kind token at this site.
                if let Some(effect) = super::effects::builtin_effect_of(callee) {
                    if lin.verified {
                        causal_spend(effect, caps, lin);
                    }
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
            walk_expr(then, caps, lin);
            walk_expr(else_, caps, lin);
        }
        Expr::IfLet {
            scrutinee,
            then,
            else_,
            ..
        } => {
            walk_expr(scrutinee, caps, lin);
            walk_expr(then, caps, lin);
            walk_expr(else_, caps, lin);
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            walk_expr(scrutinee, caps, lin);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    walk_expr(g, caps, lin);
                }
                walk_expr(&arm.body, caps, lin);
            }
        }
        Expr::Block { stmts, tail } => {
            walk_stmts(stmts, caps, lin);
            if let Some(t) = tail {
                walk_expr(t, caps, lin);
            }
        }
        // Non-exportable sealedness: definition-site walk of lambda bodies for export sinks
        // that mention free Live non_exportable tokens (does not consume — export check only).
        // Linearity of captured caps at application remains residual for HO rebind.
        Expr::Lambda { body, .. } => walk_export_seals(body, caps, lin),
    }
}

/// Export-seal walk: fire ANUBIS_CAPABILITY_EXPORT on sinks, without consuming tokens.
fn walk_export_seals(expr: &Expr, caps: &CapMap, lin: &mut Lin) {
    match expr {
        Expr::Call { callee, args } => {
            if is_export_sink(callee) && !lin.all_fns.contains(callee) {
                for a in args {
                    check_export_arg(a, caps, lin);
                }
            }
            if lin.all_fns.contains(callee) {
                if let Some(idxs) = lin.summary.exports_formals.get(callee) {
                    for &i in idxs {
                        if let Some(a) = args.get(i) {
                            check_export_arg(a, caps, lin);
                        }
                    }
                }
            }
            for a in args {
                walk_export_seals(a, caps, lin);
            }
        }
        Expr::CallExpr { callee, args } => {
            walk_export_seals(callee, caps, lin);
            for a in args {
                walk_export_seals(a, caps, lin);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            walk_export_seals(lhs, caps, lin);
            walk_export_seals(rhs, caps, lin);
        }
        Expr::Unary { expr, .. }
        | Expr::Cast { expr, .. }
        | Expr::Tainted { inner: expr, .. }
        | Expr::Declassify { inner: expr, .. }
        | Expr::Assume(expr)
        | Expr::Assert(expr)
        | Expr::Try(expr) => walk_export_seals(expr, caps, lin),
        Expr::ArrayLiteral { elements } => {
            for e in elements {
                walk_export_seals(e, caps, lin);
            }
        }
        Expr::Index { base, index } => {
            walk_export_seals(base, caps, lin);
            walk_export_seals(index, caps, lin);
        }
        Expr::StructLiteral { fields, .. } => {
            for (_, e) in fields {
                walk_export_seals(e, caps, lin);
            }
        }
        Expr::If {
            cond, then, else_, ..
        }
        | Expr::IfLet {
            scrutinee: cond,
            then,
            else_,
            ..
        } => {
            walk_export_seals(cond, caps, lin);
            walk_export_seals(then, caps, lin);
            walk_export_seals(else_, caps, lin);
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            walk_export_seals(scrutinee, caps, lin);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    walk_export_seals(g, caps, lin);
                }
                walk_export_seals(&arm.body, caps, lin);
            }
        }
        Expr::Block { stmts, tail } => {
            // Full stmt walk (incl. while/for/loop/while-let) — do not re-match a subset here.
            walk_export_seals_stmts(stmts, caps, lin);
            if let Some(t) = tail {
                walk_export_seals(t, caps, lin);
            }
        }
        Expr::Lambda { body, .. } => walk_export_seals(body, caps, lin),
        _ => {}
    }
}

fn walk_export_seals_stmts(stmts: &[Stmt], caps: &CapMap, lin: &mut Lin) {
    for s in stmts {
        match s {
            Stmt::Let { init, .. } | Stmt::Assign { value: init, .. } => {
                walk_export_seals(init, caps, lin);
            }
            Stmt::ExprStmt(e) => walk_export_seals(e, caps, lin),
            Stmt::If { then, else_, .. } => {
                walk_export_seals_stmts(then, caps, lin);
                if let Some(e) = else_ {
                    walk_export_seals_stmts(e, caps, lin);
                }
            }
            // Loop bodies (skeptic-0): NE capture → print/send inside while/for/loop/while-let
            // previously fell through `_` and fail-opened.
            Stmt::While { body, .. }
            | Stmt::Loop { body, .. }
            | Stmt::For { body, .. }
            | Stmt::WhileLet { body, .. }
            | Stmt::ResearchBlock { body, .. }
            | Stmt::ExploitBlock { body, .. } => {
                walk_export_seals_stmts(body, caps, lin);
            }
            Stmt::HybridBlock { gpu, cpu, prove } => {
                if let Some(b) = gpu {
                    walk_export_seals_stmts(b, caps, lin);
                }
                if let Some(b) = cpu {
                    walk_export_seals_stmts(b, caps, lin);
                }
                if let Some(b) = prove {
                    walk_export_seals_stmts(b, caps, lin);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::{self};

    fn findings_of(src: &str, verified: bool) -> Vec<Finding> {
        let ast = frontend::parse_source(src).expect("parse");
        check_program(&ast.items, verified)
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
        assert_eq!(
            codes(
                r#"fn main() { let c = cap_acquire("fs.read"); let y = c; cap_use(c); }"#,
                false
            ),
            ["ANUBIS_CAPABILITY_REUSE"]
        );
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
        let fresh = r#"fn f() { for i in 0..3 { let c = cap_acquire("x"); cap_use(c); } }"#;
        assert!(codes(fresh, false).is_empty());
        assert!(codes(fresh, true).is_empty());
    }

    // ── Slice 3: effect authorization (composition + causal spend) ─────────────────────────────

    #[test]
    fn verified_effect_without_acquisition_is_unauthorized() {
        let src = r#"fn f() { send("h", 80, "x"); }"#;
        assert!(
            codes(src, false).is_empty(),
            "default lane requires no authorization"
        );
        assert_eq!(codes(src, true), ["ANUBIS_EFFECT_UNAUTHORIZED"]);
    }

    #[test]
    fn verified_effect_with_live_matching_token_is_authorized() {
        // Causal spend: acquire then effect (effect is the spend; no trailing cap_use).
        let src = r#"fn f() { let n = cap_acquire("net.send"); send("h", 80, "x"); }"#;
        assert!(codes(src, true).is_empty());
        assert!(codes(src, false).is_empty());
    }

    #[test]
    fn verified_effect_then_cap_use_is_reuse() {
        // After causal spend at send, trailing cap_use is double-spend.
        let src = r#"fn f() { let n = cap_acquire("net.send"); send("h", 80, "x"); cap_use(n); }"#;
        assert_eq!(codes(src, true), ["ANUBIS_CAPABILITY_REUSE"]);
    }

    #[test]
    fn verified_wrong_kind_live_token_does_not_authorize() {
        let src = r#"fn f() { let c = cap_acquire("fs.read"); send("h", 80, "x"); }"#;
        assert_eq!(codes(src, true), ["ANUBIS_EFFECT_UNAUTHORIZED"]);
    }

    #[test]
    fn param_capability_does_not_authorize_an_effect() {
        let src = r#"fn f(netcap) { send("h", 80, "x"); }"#;
        assert!(codes(src, false).is_empty());
        assert_eq!(codes(src, true), ["ANUBIS_EFFECT_UNAUTHORIZED"]);
    }

    #[test]
    fn user_fn_shadowing_a_builtin_name_is_not_a_performed_effect() {
        let src = r#"fn send(x) { return x; }
fn f() { let y = send(3); }"#;
        assert!(codes(src, true).is_empty());
    }

    #[test]
    fn non_literal_acquisition_kind_does_not_authorize() {
        let src = r#"fn f(kind) { let n = cap_acquire(kind); send("h", 80, "x"); }"#;
        assert_eq!(codes(src, true), ["ANUBIS_EFFECT_UNAUTHORIZED"]);
    }

    #[test]
    fn explicit_cap_use_without_effect_still_linear() {
        // cap_use alone remains a valid explicit spend path.
        assert!(codes(
            r#"fn f() { let n = cap_acquire("net.send"); cap_use(n); }"#,
            true
        )
        .is_empty());
    }

    // ── Non-exportable capability (Depth / Keychain micro-slice primary) ───────────────────────

    #[test]
    fn nonexportable_causal_spend_without_token_in_args_accepts() {
        // Authorize effect ambiently; token is NOT a sink argument.
        let src =
            r#"fn f() { let n = cap_acquire_nonexportable("net.send"); send("h", 80, "x"); }"#;
        assert!(codes(src, true).is_empty());
        assert!(codes(src, false).is_empty());
    }

    #[test]
    fn nonexportable_token_as_print_arg_is_export() {
        let src = r#"fn f() { let s = cap_acquire_nonexportable("fs.write"); print(s); }"#;
        assert_eq!(codes(src, true), ["ANUBIS_CAPABILITY_EXPORT"]);
        assert_eq!(codes(src, false), ["ANUBIS_CAPABILITY_EXPORT"]);
    }

    #[test]
    fn nonexportable_token_as_send_payload_is_export() {
        let src = r#"fn f() { let s = cap_acquire_nonexportable("net.send"); send("h", 80, s); }"#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "expected EXPORT, got {c:?}"
        );
    }

    #[test]
    fn ordinary_acquire_token_as_print_arg_is_not_export() {
        // Exportable mint: token-as-arg still just consumes (surrender), no EXPORT.
        let src = r#"fn f() { let c = cap_acquire("fs.read"); print(c); }"#;
        assert!(codes(src, true).is_empty());
    }

    #[test]
    fn wellformed_cap_export_peels_then_print_accepts() {
        let src = r#"fn f() {
            let s = cap_acquire_nonexportable("fs.write");
            let e = cap_export(s, "audit: release for backup");
            print(e);
        }"#;
        assert!(codes(src, true).is_empty());
    }

    #[test]
    fn empty_reason_cap_export_does_not_peel() {
        let src = r#"fn f() {
            let s = cap_acquire_nonexportable("fs.write");
            let e = cap_export(s, "");
            print(s);
        }"#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT_MALFORMED"),
            "expected MALFORMED, got {c:?}"
        );
        // s remains non_exportable Live → print(s) still EXPORT.
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "expected EXPORT on print(s), got {c:?}"
        );
    }

    #[test]
    fn move_preserves_nonexportable() {
        let src = r#"fn f() {
            let s = cap_acquire_nonexportable("fs.write");
            let y = s;
            print(y);
        }"#;
        assert_eq!(codes(src, true), ["ANUBIS_CAPABILITY_EXPORT"]);
    }

    #[test]
    fn nonexportable_cap_use_is_not_export() {
        let src = r#"fn f() { let s = cap_acquire_nonexportable("fs.read"); cap_use(s); }"#;
        assert!(codes(src, true).is_empty());
    }

    #[test]
    fn nonexportable_double_spend_is_reuse() {
        let src = r#"fn f() {
            let n = cap_acquire_nonexportable("net.send");
            send("h", 80, "x");
            cap_use(n);
        }"#;
        assert_eq!(codes(src, true), ["ANUBIS_CAPABILITY_REUSE"]);
    }

    #[test]
    fn nonexportable_return_mint_print_is_export() {
        let src = r#"
            fn mint() { return cap_acquire_nonexportable("fs.write"); }
            fn f() { let s = mint(); print(s); }
        "#;
        for verified in [false, true] {
            let c = codes(src, verified);
            assert!(
                c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
                "return-mint print must EXPORT, verified={verified}, got {c:?}"
            );
        }
    }

    #[test]
    fn nonexportable_return_mint_direct_print_is_export() {
        let src = r#"
            fn mint() { return cap_acquire_nonexportable("fs.write"); }
            fn f() { print(mint()); }
        "#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "print(mint()) must EXPORT, got {c:?}"
        );
    }

    #[test]
    fn nonexportable_param_print_is_export() {
        let src = r#"
            fn leak(c) { print(c); }
            fn f() {
                let s = cap_acquire_nonexportable("fs.write");
                leak(s);
            }
        "#;
        for verified in [false, true] {
            let c = codes(src, verified);
            assert!(
                c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
                "param leak must EXPORT, verified={verified}, got {c:?}"
            );
        }
    }

    #[test]
    fn nonexportable_peel_before_interproc_print_accepts() {
        // Peel at caller (params are unknown-provenance for cap_export); callee prints exportable.
        let src = r#"
            fn leak(c) { print(c); }
            fn f() {
                let s = cap_acquire_nonexportable("fs.write");
                let e = cap_export(s, "audit");
                leak(e);
            }
        "#;
        assert!(
            codes(src, true).is_empty(),
            "peel-before-call must accept, got {:?}",
            codes(src, true)
        );
    }

    #[test]
    fn nonexportable_closure_capture_print_is_export() {
        let src = r#"fn f() {
            let s = cap_acquire_nonexportable("fs.write");
            let g = |x| print(s);
            g(0);
        }"#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "closure capture print must EXPORT, got {c:?}"
        );
    }

    #[test]
    fn nonexportable_returned_closure_capture_print_is_export() {
        let src = r#"
            fn mk() {
                let s = cap_acquire_nonexportable("fs.write");
                return |x| print(s);
            }
            fn f() {
                let g = mk();
                g(0);
            }
        "#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "returned closure capture must EXPORT at def, got {c:?}"
        );
    }

    #[test]
    fn nonexportable_closure_capture_print_in_while_is_export() {
        let src = r#"fn f() {
            let s = cap_acquire_nonexportable("fs.write");
            let g = |x| { while true { print(s); break; } };
            g(0);
        }"#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "while-body capture print must EXPORT, got {c:?}"
        );
    }

    #[test]
    fn nonexportable_closure_capture_print_in_for_is_export() {
        let src = r#"fn f() {
            let s = cap_acquire_nonexportable("fs.write");
            let g = |x| { for i in 0..1 { print(s); } };
            g(0);
        }"#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "for-body capture print must EXPORT, got {c:?}"
        );
    }

    #[test]
    fn nonexportable_closure_capture_print_in_loop_is_export() {
        let src = r#"fn f() {
            let s = cap_acquire_nonexportable("fs.write");
            let g = |x| { loop { print(s); break; } };
            g(0);
        }"#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "loop-body capture print must EXPORT, got {c:?}"
        );
    }

    #[test]
    fn nonexportable_closure_capture_print_in_while_let_is_export() {
        let src = r#"fn f() {
            let s = cap_acquire_nonexportable("fs.write");
            let g = |x| { while let Some(y) = Some(0) { print(s); break; } };
            g(0);
        }"#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "while-let-body capture print must EXPORT, got {c:?}"
        );
    }

    #[test]
    fn nonexportable_closure_capture_send_in_while_is_export() {
        let src = r#"fn f() {
            let s = cap_acquire_nonexportable("net.send");
            let g = |x| { while true { send("h", 80, s); break; } };
            g(0);
        }"#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "while-body capture send must EXPORT, got {c:?}"
        );
    }

    #[test]
    fn ordinary_acquire_closure_print_in_while_is_not_export() {
        // Clean dual: exportable mint may appear as print arg even inside loop bodies.
        let src = r#"fn f() {
            let c = cap_acquire("fs.read");
            let g = |x| { while true { print(c); break; } };
            g(0);
        }"#;
        assert!(
            codes(src, true).is_empty(),
            "exportable mint in while must not EXPORT, got {:?}",
            codes(src, true)
        );
    }

    #[test]
    fn ordinary_param_print_is_not_export() {
        let src = r#"
            fn leak(c) { print(c); }
            fn f() {
                let s = cap_acquire("fs.read");
                leak(s);
            }
        "#;
        assert!(codes(src, true).is_empty());
    }

    #[test]
    fn ordinary_return_acquire_print_is_not_export() {
        let src = r#"
            fn mint() { return cap_acquire("fs.read"); }
            fn f() { let s = mint(); print(s); }
        "#;
        assert!(codes(src, true).is_empty());
    }
}
