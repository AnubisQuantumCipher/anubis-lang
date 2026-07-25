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
//! **Peel-of-param:** a formal may be released with well-formed `cap_export(param, "reason")`
//! (params never authorize effects; kind stays `None`).
//! **Ambient interproc causal spend (caller-pays):** a function that performs a privileged
//! builtin without a local string-literal acquire of that kind, and that is called from another
//! function, records the effect in `caller_pays_effects`. The effect site defers; each call site
//! must causal-spend a live matching-kind token (composed through callees). Roots (never called
//! from a different function) still self-authorize.
//! **Deep HO rebind (linear closures):** a lambda that free-uses a Live capability *moves*
//! those tokens into the closure binding; the binding is linear — second application is
//! `ANUBIS_CAPABILITY_REUSE`. Container-stored closures remain residual.

use crate::frontend::{Expr, ForSource, Pattern, Stmt};
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
    /// Locals that hold non-exportable capability *values* inside aggregates after a store
    /// (`let arr = [s]` / struct / map). Index/field project to an export sink is EXPORT —
    /// closes store-then-project laundering (dual-use seal).
    container_ne: BTreeSet<String>,
    /// Formal parameter names for this function. Used only for peel-of-param (`cap_export` on a
    /// formal that is not yet a Live NE local). Params never authorize effects.
    params: BTreeSet<String>,
    /// Current function name (caller-pays lookup + diagnostics).
    fn_name: String,
    /// Locals bound to lambdas that captured ≥1 free Live capability. Linear: first apply
    /// or MOVE consumes; second apply → REUSE. Closes deep HO rebind of use-once tokens.
    linear_closures: BTreeMap<String, CapState>,
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
    /// User function → container formal index → value formal indices stored into it
    /// (push/insert/literal/place of formal into formal container). Call sites seed
    /// `container_ne` on the corresponding args when value args hold Live NE.
    /// Direct-callee only (transitive stash-through-mid is residual unless composed).
    container_stores: BTreeMap<String, BTreeMap<usize, BTreeSet<usize>>>,
    /// User function → privileged effect kinds the function (transitively) needs a caller to
    /// causal-spend. Built for functions called from a *different* function; roots self-authorize.
    /// Local string-literal acquire of kind K removes K from that function's pays set.
    caller_pays_effects: BTreeMap<String, BTreeSet<String>>,
}

type CapFnRef<'a> = (&'a str, &'a [(String, String)], &'a [Stmt], (usize, usize));

/// Check every function: linearity + verified causal spend + interproc non_exportable sealedness.
#[cfg(test)]
pub(crate) fn check_program(items: &[crate::frontend::Item], verified: bool) -> Vec<Finding> {
    let mut all_fns = BTreeSet::new();
    let mut fns: Vec<CapFnRef<'_>> = Vec::new();
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
            fns.push((
                name.as_str(),
                params.as_slice(),
                body.as_slice(),
                (span.start, span.end),
            ));
        }
    }
    let summary = build_cap_program_summary_from_fns(&fns, &all_fns);
    let mut out = Vec::new();
    for (name, params, body, span) in fns {
        out.extend(check_linearity(
            params, body, verified, span, &all_fns, &summary, name,
        ));
    }
    out
}

/// Check one function body for capability linearity AND effect authorization (verified mode).
/// `params` seed the scope as unknown-provenance names for *authorization* (a param never
/// authorizes an effect) and as peelable formals for `cap_export`. Non-exportable sealedness
/// uses [`CapProgramSummary`] for call/return edges. Pure: returns findings.
pub(crate) fn check_linearity(
    params: &[(String, String)],
    body: &[Stmt],
    verified: bool,
    span: (usize, usize),
    all_fns: &BTreeSet<String>,
    summary: &CapProgramSummary,
    fn_name: &str,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    let param_names: BTreeSet<String> = params.iter().map(|(n, _)| n.clone()).collect();
    let mut lin = Lin {
        verified,
        span,
        findings: &mut findings,
        all_fns,
        summary,
        container_ne: BTreeSet::new(),
        params: param_names,
        fn_name: fn_name.to_string(),
        linear_closures: BTreeMap::new(),
    };
    let mut caps: CapMap = BTreeMap::new();
    walk_stmts(body, &mut caps, &mut lin);
    findings
}

pub(crate) fn program_summary(items: &[crate::frontend::Item]) -> CapProgramSummary {
    let mut all_fns = BTreeSet::new();
    let mut fns: Vec<CapFnRef<'_>> = Vec::new();
    collect_fn_refs(items, &mut all_fns, &mut fns);
    build_cap_program_summary_from_fns(&fns, &all_fns)
}

fn collect_fn_refs<'a>(
    items: &'a [crate::frontend::Item],
    all_fns: &mut BTreeSet<String>,
    fns: &mut Vec<CapFnRef<'a>>,
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
    fns: &[CapFnRef<'_>],
    all_fns: &BTreeSet<String>,
) -> CapProgramSummary {
    let mut summary = CapProgramSummary::default();
    // Fixpoint: container stores + export formals compose through callees
    // (stash(arr,s){push}; leak calls stash then print(arr[0])).
    loop {
        let before_stores = summary.container_stores.clone();
        let before_exports = summary.exports_formals.clone();
        for (name, params, body, _) in fns {
            let (exported, stores) =
                formals_export_and_container_stores(params, body, &summary.container_stores);
            if exported.is_empty() {
                summary.exports_formals.remove(*name);
            } else {
                summary
                    .exports_formals
                    .insert((*name).to_string(), exported);
            }
            if stores.is_empty() {
                summary.container_stores.remove(*name);
            } else {
                summary.container_stores.insert((*name).to_string(), stores);
            }
        }
        if summary.container_stores == before_stores && summary.exports_formals == before_exports {
            break;
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
    // Caller-pays ambient interproc causal spend.
    // - direct privileged builtins not covered by local string-literal acquire
    // - composed through callees (fixpoint)
    // - only for functions called from a *different* function (roots self-authorize;
    //   self-recursion alone does not mark external).
    let mut direct_effects: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut local_acquires: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut callees_of: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut externally_called: BTreeSet<String> = BTreeSet::new();
    for (name, _params, body, _) in fns {
        let mut de = BTreeSet::new();
        let mut acq = BTreeSet::new();
        let mut callees = BTreeSet::new();
        collect_body_cap_facts(body, all_fns, &mut de, &mut acq, &mut callees);
        for g in &callees {
            if g != *name {
                externally_called.insert(g.clone());
            }
        }
        direct_effects.insert((*name).to_string(), de);
        local_acquires.insert((*name).to_string(), acq);
        callees_of.insert((*name).to_string(), callees);
    }
    let mut pays: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    loop {
        let before = pays.clone();
        for (name, _, _, _) in fns {
            let mut set = direct_effects.get(*name).cloned().unwrap_or_default();
            if let Some(cs) = callees_of.get(*name) {
                for g in cs {
                    if let Some(gp) = pays.get(g) {
                        set.extend(gp.iter().cloned());
                    }
                }
            }
            if let Some(acq) = local_acquires.get(*name) {
                for k in acq {
                    set.remove(k);
                }
            }
            // Roots (never called from another fn) must self-authorize — not in pays map.
            if externally_called.contains(*name) && !set.is_empty() {
                pays.insert((*name).to_string(), set);
            } else {
                pays.remove(*name);
            }
        }
        if pays == before {
            break;
        }
    }
    summary.caller_pays_effects = pays;
    summary
}

/// Direct privileged builtins, local string-literal acquire kinds, and named user callees.
fn collect_body_cap_facts(
    body: &[Stmt],
    all_fns: &BTreeSet<String>,
    direct_effects: &mut BTreeSet<String>,
    local_acquires: &mut BTreeSet<String>,
    callees: &mut BTreeSet<String>,
) {
    fn walk_stmts(
        stmts: &[Stmt],
        all_fns: &BTreeSet<String>,
        de: &mut BTreeSet<String>,
        acq: &mut BTreeSet<String>,
        callees: &mut BTreeSet<String>,
    ) {
        for s in stmts {
            match s {
                Stmt::Let { init, .. } | Stmt::LetPattern { init, .. } => {
                    walk_expr(init, all_fns, de, acq, callees);
                }
                Stmt::Assign { target, value } => {
                    walk_expr(target, all_fns, de, acq, callees);
                    walk_expr(value, all_fns, de, acq, callees);
                }
                Stmt::ExprStmt(e) => walk_expr(e, all_fns, de, acq, callees),
                Stmt::If {
                    cond, then, else_, ..
                } => {
                    walk_expr(cond, all_fns, de, acq, callees);
                    walk_stmts(then, all_fns, de, acq, callees);
                    if let Some(el) = else_ {
                        walk_stmts(el, all_fns, de, acq, callees);
                    }
                }
                Stmt::While {
                    cond,
                    body,
                    invariant,
                } => {
                    walk_expr(cond, all_fns, de, acq, callees);
                    for inv in invariant {
                        walk_expr(inv, all_fns, de, acq, callees);
                    }
                    walk_stmts(body, all_fns, de, acq, callees);
                }
                Stmt::Loop { body, invariant } => {
                    for inv in invariant {
                        walk_expr(inv, all_fns, de, acq, callees);
                    }
                    walk_stmts(body, all_fns, de, acq, callees);
                }
                Stmt::For {
                    source,
                    body,
                    invariant,
                    ..
                } => {
                    match source {
                        ForSource::Range { start, end } => {
                            walk_expr(start, all_fns, de, acq, callees);
                            walk_expr(end, all_fns, de, acq, callees);
                        }
                        ForSource::Collection { expr } => {
                            walk_expr(expr, all_fns, de, acq, callees);
                        }
                    }
                    for inv in invariant {
                        walk_expr(inv, all_fns, de, acq, callees);
                    }
                    walk_stmts(body, all_fns, de, acq, callees);
                }
                Stmt::WhileLet { expr, body, .. } => {
                    walk_expr(expr, all_fns, de, acq, callees);
                    walk_stmts(body, all_fns, de, acq, callees);
                }
                Stmt::ResearchBlock { body, .. } | Stmt::ExploitBlock { body, .. } => {
                    walk_stmts(body, all_fns, de, acq, callees);
                }
                Stmt::HybridBlock { gpu, cpu, prove } => {
                    if let Some(b) = gpu {
                        walk_stmts(b, all_fns, de, acq, callees);
                    }
                    if let Some(b) = cpu {
                        walk_stmts(b, all_fns, de, acq, callees);
                    }
                    if let Some(b) = prove {
                        walk_stmts(b, all_fns, de, acq, callees);
                    }
                }
                Stmt::Break | Stmt::Continue | Stmt::SpecBlock { .. } => {}
            }
        }
    }
    fn walk_expr(
        e: &Expr,
        all_fns: &BTreeSet<String>,
        de: &mut BTreeSet<String>,
        acq: &mut BTreeSet<String>,
        callees: &mut BTreeSet<String>,
    ) {
        match e {
            Expr::Call { callee, args } => {
                if callee == CAP_ACQUIRE || callee == CAP_ACQUIRE_NONEXPORTABLE {
                    if let Some(Expr::StrLiteral(k)) = args.first() {
                        acq.insert(super::normalize_effect_name(k));
                    }
                } else if all_fns.contains(callee) {
                    callees.insert(callee.clone());
                } else if let Some(effect) = super::effects::builtin_effect_of(callee) {
                    de.insert(effect.to_string());
                }
                for a in args {
                    walk_expr(a, all_fns, de, acq, callees);
                }
            }
            Expr::CallExpr { callee, args } => {
                walk_expr(callee, all_fns, de, acq, callees);
                for a in args {
                    walk_expr(a, all_fns, de, acq, callees);
                }
            }
            Expr::Binary { lhs, rhs, .. } => {
                walk_expr(lhs, all_fns, de, acq, callees);
                walk_expr(rhs, all_fns, de, acq, callees);
            }
            Expr::Unary { expr, .. }
            | Expr::Cast { expr, .. }
            | Expr::Tainted { inner: expr, .. }
            | Expr::Declassify { inner: expr, .. }
            | Expr::Assume(expr)
            | Expr::Assert(expr)
            | Expr::Try(expr) => walk_expr(expr, all_fns, de, acq, callees),
            Expr::ArrayLiteral { elements }
            | Expr::EnumConstruct {
                fields: elements, ..
            } => {
                for el in elements {
                    walk_expr(el, all_fns, de, acq, callees);
                }
            }
            Expr::StructLiteral { fields, .. } => {
                for (_, v) in fields {
                    walk_expr(v, all_fns, de, acq, callees);
                }
            }
            Expr::MapLiteral { entries, .. } => {
                for (_, v) in entries {
                    walk_expr(v, all_fns, de, acq, callees);
                }
            }
            Expr::Index { base, index } => {
                walk_expr(base, all_fns, de, acq, callees);
                walk_expr(index, all_fns, de, acq, callees);
            }
            Expr::FieldAccess { base, .. } => walk_expr(base, all_fns, de, acq, callees),
            Expr::If {
                cond, then, else_, ..
            } => {
                walk_expr(cond, all_fns, de, acq, callees);
                walk_expr(then, all_fns, de, acq, callees);
                walk_expr(else_, all_fns, de, acq, callees);
            }
            Expr::IfLet {
                scrutinee,
                then,
                else_,
                ..
            } => {
                walk_expr(scrutinee, all_fns, de, acq, callees);
                walk_expr(then, all_fns, de, acq, callees);
                walk_expr(else_, all_fns, de, acq, callees);
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                walk_expr(scrutinee, all_fns, de, acq, callees);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        walk_expr(g, all_fns, de, acq, callees);
                    }
                    walk_expr(&arm.body, all_fns, de, acq, callees);
                }
            }
            Expr::Block { stmts, tail } => {
                walk_stmts(stmts, all_fns, de, acq, callees);
                if let Some(t) = tail {
                    walk_expr(t, all_fns, de, acq, callees);
                }
            }
            Expr::Lambda { body, .. } => walk_expr(body, all_fns, de, acq, callees),
            _ => {}
        }
    }
    walk_stmts(body, all_fns, direct_effects, local_acquires, callees);
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

/// Child of a `Stmt` that can host an export sink (value/header expr or nested body).
/// Shared by lambda export-seal and interproc formal export summary so control-flow
/// headers cannot diverge (strategist: one visitor, two clients).
enum ExportReachable<'a> {
    Expr(&'a Expr),
    Stmts(&'a [Stmt]),
}

/// Exhaustive `Stmt` decomposition for export-reachable positions.
/// Leaves (`Break`/`Continue`/`SpecBlock`) are intentional no-ops.
fn for_each_export_child(stmt: &Stmt, mut visit: impl FnMut(ExportReachable<'_>)) {
    match stmt {
        Stmt::Let { init, .. } | Stmt::LetPattern { init, .. } => {
            visit(ExportReachable::Expr(init));
        }
        Stmt::Assign { target, value } => {
            visit(ExportReachable::Expr(target));
            visit(ExportReachable::Expr(value));
        }
        Stmt::ExprStmt(e) => visit(ExportReachable::Expr(e)),
        Stmt::If {
            cond, then, else_, ..
        } => {
            visit(ExportReachable::Expr(cond));
            visit(ExportReachable::Stmts(then));
            if let Some(el) = else_ {
                visit(ExportReachable::Stmts(el));
            }
        }
        Stmt::While {
            cond,
            body,
            invariant,
        } => {
            visit(ExportReachable::Expr(cond));
            for inv in invariant {
                visit(ExportReachable::Expr(inv));
            }
            visit(ExportReachable::Stmts(body));
        }
        Stmt::Loop { body, invariant } => {
            for inv in invariant {
                visit(ExportReachable::Expr(inv));
            }
            visit(ExportReachable::Stmts(body));
        }
        Stmt::For {
            source,
            body,
            invariant,
            ..
        } => {
            match source {
                ForSource::Range { start, end } => {
                    visit(ExportReachable::Expr(start));
                    visit(ExportReachable::Expr(end));
                }
                ForSource::Collection { expr } => visit(ExportReachable::Expr(expr)),
            }
            for inv in invariant {
                visit(ExportReachable::Expr(inv));
            }
            visit(ExportReachable::Stmts(body));
        }
        Stmt::WhileLet { expr, body, .. } => {
            visit(ExportReachable::Expr(expr));
            visit(ExportReachable::Stmts(body));
        }
        Stmt::ResearchBlock { body, .. } | Stmt::ExploitBlock { body, .. } => {
            visit(ExportReachable::Stmts(body));
        }
        Stmt::HybridBlock { gpu, cpu, prove } => {
            if let Some(b) = gpu {
                visit(ExportReachable::Stmts(b));
            }
            if let Some(b) = cpu {
                visit(ExportReachable::Stmts(b));
            }
            if let Some(b) = prove {
                visit(ExportReachable::Stmts(b));
            }
        }
        Stmt::Break | Stmt::Continue | Stmt::SpecBlock { .. } => {}
    }
}

/// Formal export-sink reachability + formal-container store summary for one function body.
/// `known_container_stores` is the program summary so far (callee formal-container maps).
/// Returns `(exporting_formal_idxs, container_formal_idx → value_formal_idxs)`.
fn formals_export_and_container_stores(
    params: &[(String, String)],
    body: &[Stmt],
    known_container_stores: &BTreeMap<String, BTreeMap<usize, BTreeSet<usize>>>,
) -> (BTreeSet<usize>, BTreeMap<usize, BTreeSet<usize>>) {
    // name -> formal index still sealed (not peeled)
    let mut sealed: BTreeMap<String, usize> = BTreeMap::new();
    for (i, (n, _)) in params.iter().enumerate() {
        sealed.insert(n.clone(), i);
    }
    // name -> formal indices stored into this container (store-then-project / push / insert)
    let mut containers: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    let mut exported = BTreeSet::new();
    fn peel_name(
        name: &str,
        sealed: &mut BTreeMap<String, usize>,
        containers: &mut BTreeMap<String, BTreeSet<usize>>,
    ) {
        sealed.remove(name);
        containers.remove(name);
    }
    /// Formal indices embedded in `e` (direct sealed var, known container, aggregate,
    /// project from container, or if/match/block value-position).
    fn formals_in_expr(
        e: &Expr,
        sealed: &BTreeMap<String, usize>,
        containers: &BTreeMap<String, BTreeSet<usize>>,
    ) -> BTreeSet<usize> {
        match e {
            Expr::Var(n) => {
                let mut out = BTreeSet::new();
                if let Some(i) = sealed.get(n) {
                    out.insert(*i);
                }
                if let Some(idxs) = containers.get(n) {
                    out.extend(idxs.iter().copied());
                }
                out
            }
            Expr::ArrayLiteral { elements }
            | Expr::EnumConstruct {
                fields: elements, ..
            } => elements
                .iter()
                .flat_map(|el| formals_in_expr(el, sealed, containers))
                .collect(),
            Expr::StructLiteral { fields, .. } => fields
                .iter()
                .flat_map(|(_, v)| formals_in_expr(v, sealed, containers))
                .collect(),
            Expr::MapLiteral { entries, .. } => entries
                .iter()
                .flat_map(|(_, v)| formals_in_expr(v, sealed, containers))
                .collect(),
            // Project-bind: `let x = arr[0]` after arr holds formal → x carries those formals.
            Expr::Index { base, .. } | Expr::FieldAccess { base, .. } => {
                formals_in_expr(base, sealed, containers)
            }
            Expr::If { then, else_, .. } | Expr::IfLet { then, else_, .. } => {
                let mut out = formals_in_expr(then, sealed, containers);
                out.extend(formals_in_expr(else_, sealed, containers));
                out
            }
            Expr::Match { arms, .. } => arms
                .iter()
                .flat_map(|arm| formals_in_expr(&arm.body, sealed, containers))
                .collect(),
            Expr::Block { tail, .. } => tail
                .as_ref()
                .map(|t| formals_in_expr(t, sealed, containers))
                .unwrap_or_default(),
            Expr::Unary { expr, .. } | Expr::Cast { expr, .. } | Expr::Try(expr) => {
                formals_in_expr(expr, sealed, containers)
            }
            _ => BTreeSet::new(),
        }
    }
    /// Seed container tracking for free/method `push` / `insert` of sealed formals.
    fn note_formal_container_mutation(
        name: &str,
        args: &[Expr],
        method_base: Option<&Expr>,
        sealed: &BTreeMap<String, usize>,
        containers: &mut BTreeMap<String, BTreeSet<usize>>,
    ) {
        let (container, value) = match name {
            "push" => {
                let cont = method_base.or_else(|| args.first());
                let val = if method_base.is_some() {
                    args.first()
                } else {
                    args.get(1)
                };
                (cont, val)
            }
            "insert" => {
                let cont = method_base.or_else(|| args.first());
                let val = if method_base.is_some() {
                    args.get(1)
                } else {
                    args.get(2)
                };
                (cont, val)
            }
            _ => return,
        };
        let (Some(container), Some(value)) = (container, value) else {
            return;
        };
        let idxs = formals_in_expr(value, sealed, containers);
        if idxs.is_empty() {
            return;
        }
        // Named root: free/method push/insert on `arr` or `arrs[0]` → seed root.
        if let Some(root) = place_root_var(container) {
            containers.entry(root.to_string()).or_default().extend(idxs);
        }
    }
    /// Mirror `check_export_arg` aggregate + container project so print(arr[0]) marks formal s.
    fn on_export_arg(
        e: &Expr,
        sealed: &BTreeMap<String, usize>,
        containers: &BTreeMap<String, BTreeSet<usize>>,
        exported: &mut BTreeSet<usize>,
    ) {
        match e {
            Expr::Var(n) => {
                if let Some(i) = sealed.get(n) {
                    exported.insert(*i);
                }
                if let Some(idxs) = containers.get(n) {
                    exported.extend(idxs.iter().copied());
                }
            }
            Expr::ArrayLiteral { elements }
            | Expr::EnumConstruct {
                fields: elements, ..
            } => {
                for el in elements {
                    on_export_arg(el, sealed, containers, exported);
                }
            }
            Expr::StructLiteral { fields, .. } => {
                for (_, v) in fields {
                    on_export_arg(v, sealed, containers, exported);
                }
            }
            Expr::MapLiteral { entries, .. } => {
                for (_, v) in entries {
                    on_export_arg(v, sealed, containers, exported);
                }
            }
            Expr::Index { base, index } => {
                on_export_arg(base, sealed, containers, exported);
                on_export_arg(index, sealed, containers, exported);
                if let Expr::Var(n) = &**base {
                    if let Some(idxs) = containers.get(n) {
                        exported.extend(idxs.iter().copied());
                    }
                }
            }
            Expr::FieldAccess { base, .. } => {
                on_export_arg(base, sealed, containers, exported);
                if let Expr::Var(n) = &**base {
                    if let Some(idxs) = containers.get(n) {
                        exported.extend(idxs.iter().copied());
                    }
                }
            }
            Expr::If { then, else_, .. } | Expr::IfLet { then, else_, .. } => {
                on_export_arg(then, sealed, containers, exported);
                on_export_arg(else_, sealed, containers, exported);
            }
            Expr::Match { arms, .. } => {
                for arm in arms {
                    on_export_arg(&arm.body, sealed, containers, exported);
                }
            }
            Expr::Block { tail: Some(t), .. } => {
                on_export_arg(t, sealed, containers, exported);
            }
            Expr::Block { tail: None, .. } => {}
            Expr::Unary { expr, .. } | Expr::Cast { expr, .. } | Expr::Try(expr) => {
                on_export_arg(expr, sealed, containers, exported);
            }
            _ => {}
        }
    }
    fn rebind_or_scan_init(
        name: &str,
        init: &Expr,
        sealed: &mut BTreeMap<String, usize>,
        containers: &mut BTreeMap<String, BTreeSet<usize>>,
        exported: &mut BTreeSet<usize>,
        known: &BTreeMap<String, BTreeMap<usize, BTreeSet<usize>>>,
    ) {
        match init {
            Expr::Var(src) => {
                if let Some(i) = sealed.get(src).cloned() {
                    sealed.remove(src);
                    sealed.insert(name.to_string(), i);
                    containers.remove(name);
                } else if let Some(idxs) = containers.get(src).cloned() {
                    // MOVE of an NE-carrying *container*
                    containers.remove(src);
                    containers.insert(name.to_string(), idxs);
                    sealed.remove(name);
                } else {
                    sealed.remove(name);
                    containers.remove(name);
                }
            }
            Expr::Call { callee, args } if callee == CAP_EXPORT => {
                // Only well-formed peels (non-empty string reason) unseal the formal.
                let reason_ok = matches!(args.get(1), Some(Expr::StrLiteral(r)) if !r.is_empty());
                if reason_ok {
                    if let Some(Expr::Var(src)) = args.first() {
                        peel_name(src, sealed, containers);
                    }
                }
                sealed.remove(name);
                containers.remove(name);
            }
            other => {
                // Scan for export of sealed formals *before* dropping the name's tracking.
                walk_expr_export(other, sealed, containers, exported, known);
                let idxs = formals_in_expr(other, sealed, containers);
                if !idxs.is_empty() {
                    containers.insert(name.to_string(), idxs);
                } else {
                    containers.remove(name);
                }
                sealed.remove(name);
            }
        }
    }
    fn walk(
        stmts: &[Stmt],
        sealed: &mut BTreeMap<String, usize>,
        containers: &mut BTreeMap<String, BTreeSet<usize>>,
        exported: &mut BTreeSet<usize>,
        known: &BTreeMap<String, BTreeMap<usize, BTreeSet<usize>>>,
    ) {
        for s in stmts {
            match s {
                // Formal MOVE / peel bookkeeping — still must scan non-move inits.
                Stmt::Let { name, init, .. } => {
                    rebind_or_scan_init(name, init, sealed, containers, exported, known);
                }
                Stmt::LetPattern { init, .. } => {
                    // Pattern binds are not formals; still scan init for print(formal).
                    walk_expr_export(init, sealed, containers, exported, known);
                }
                Stmt::Assign {
                    target: Expr::Var(name),
                    value,
                } => {
                    rebind_or_scan_init(name, value, sealed, containers, exported, known);
                }
                Stmt::Assign { target, value } => {
                    // Place-write: `arr[i] = formal` / `b.f = formal` seeds container formals.
                    let idxs = formals_in_expr(value, sealed, containers);
                    if !idxs.is_empty() {
                        if let Some(root) = place_root_var(target) {
                            containers.entry(root.to_string()).or_default().extend(idxs);
                        }
                    }
                    walk_expr_export(target, sealed, containers, exported, known);
                    walk_expr_export(value, sealed, containers, exported, known);
                }
                Stmt::ExprStmt(e) => walk_expr_export(e, sealed, containers, exported, known),
                // Branch merge: peels in then do not poison else (clone sealed per arm).
                // Container stores union fail-closed into the parent (any-arm NE store sticks).
                Stmt::If {
                    cond, then, else_, ..
                } => {
                    walk_expr_export(cond, sealed, containers, exported, known);
                    let mut s1 = sealed.clone();
                    let mut c1 = containers.clone();
                    walk(then, &mut s1, &mut c1, exported, known);
                    for (k, v) in c1 {
                        containers.entry(k).or_default().extend(v);
                    }
                    if let Some(el) = else_ {
                        let mut s2 = sealed.clone();
                        let mut c2 = containers.clone();
                        walk(el, &mut s2, &mut c2, exported, known);
                        for (k, v) in c2 {
                            containers.entry(k).or_default().extend(v);
                        }
                    }
                }
                Stmt::While {
                    cond,
                    body,
                    invariant,
                } => {
                    walk_expr_export(cond, sealed, containers, exported, known);
                    for inv in invariant {
                        walk_expr_export(inv, sealed, containers, exported, known);
                    }
                    walk(body, sealed, containers, exported, known);
                }
                Stmt::Loop { body, invariant } => {
                    for inv in invariant {
                        walk_expr_export(inv, sealed, containers, exported, known);
                    }
                    walk(body, sealed, containers, exported, known);
                }
                Stmt::For {
                    source,
                    body,
                    invariant,
                    ..
                } => {
                    match source {
                        ForSource::Range { start, end } => {
                            walk_expr_export(start, sealed, containers, exported, known);
                            walk_expr_export(end, sealed, containers, exported, known);
                        }
                        ForSource::Collection { expr } => {
                            walk_expr_export(expr, sealed, containers, exported, known);
                        }
                    }
                    for inv in invariant {
                        walk_expr_export(inv, sealed, containers, exported, known);
                    }
                    walk(body, sealed, containers, exported, known);
                }
                Stmt::WhileLet { expr, body, .. } => {
                    walk_expr_export(expr, sealed, containers, exported, known);
                    walk(body, sealed, containers, exported, known);
                }
                Stmt::ResearchBlock { body, .. } | Stmt::ExploitBlock { body, .. } => {
                    walk(body, sealed, containers, exported, known);
                }
                Stmt::HybridBlock { gpu, cpu, prove } => {
                    if let Some(b) = gpu {
                        walk(b, sealed, containers, exported, known);
                    }
                    if let Some(b) = cpu {
                        walk(b, sealed, containers, exported, known);
                    }
                    if let Some(b) = prove {
                        walk(b, sealed, containers, exported, known);
                    }
                }
                Stmt::Break | Stmt::Continue | Stmt::SpecBlock { .. } => {}
            }
        }
    }
    fn walk_expr_export(
        e: &Expr,
        sealed: &mut BTreeMap<String, usize>,
        containers: &mut BTreeMap<String, BTreeSet<usize>>,
        exported: &mut BTreeSet<usize>,
        known: &BTreeMap<String, BTreeMap<usize, BTreeSet<usize>>>,
    ) {
        match e {
            Expr::Call { callee, args } => {
                if callee == CAP_EXPORT {
                    let reason_ok =
                        matches!(args.get(1), Some(Expr::StrLiteral(r)) if !r.is_empty());
                    if reason_ok {
                        if let Some(Expr::Var(src)) = args.first() {
                            peel_name(src, sealed, containers);
                        }
                    }
                } else if is_export_sink(callee) {
                    for a in args {
                        on_export_arg(a, sealed, containers, exported);
                    }
                }
                // Store-then-project via free push/insert of sealed formals.
                note_formal_container_mutation(callee, args, None, sealed, containers);
                // Compose callee formal-container stores into this function's containers map.
                if let Some(stores) = known.get(callee) {
                    for (&cont_i, val_idxs) in stores {
                        let Some(cont_arg) = args.get(cont_i) else {
                            continue;
                        };
                        let Some(cont_root) = place_root_var(cont_arg) else {
                            continue;
                        };
                        let mut held = BTreeSet::new();
                        for &vi in val_idxs {
                            if let Some(va) = args.get(vi) {
                                held.extend(formals_in_expr(va, sealed, containers));
                            }
                        }
                        if !held.is_empty() {
                            containers
                                .entry(cont_root.to_string())
                                .or_default()
                                .extend(held);
                        }
                    }
                }
                for a in args {
                    walk_expr_export(a, sealed, containers, exported, known);
                }
            }
            Expr::CallExpr { callee, args } => {
                if let Expr::FieldAccess { base, field, .. } = &**callee {
                    note_formal_container_mutation(field, args, Some(base), sealed, containers);
                }
                walk_expr_export(callee, sealed, containers, exported, known);
                for a in args {
                    walk_expr_export(a, sealed, containers, exported, known);
                }
            }
            Expr::Binary { lhs, rhs, .. } => {
                walk_expr_export(lhs, sealed, containers, exported, known);
                walk_expr_export(rhs, sealed, containers, exported, known);
            }
            Expr::Unary { expr, .. }
            | Expr::Cast { expr, .. }
            | Expr::Tainted { inner: expr, .. }
            | Expr::Declassify { inner: expr, .. }
            | Expr::Assume(expr)
            | Expr::Assert(expr)
            | Expr::Try(expr) => walk_expr_export(expr, sealed, containers, exported, known),
            Expr::ArrayLiteral { elements }
            | Expr::EnumConstruct {
                fields: elements, ..
            } => {
                for el in elements {
                    walk_expr_export(el, sealed, containers, exported, known);
                }
            }
            Expr::StructLiteral { fields, .. } => {
                for (_, v) in fields {
                    walk_expr_export(v, sealed, containers, exported, known);
                }
            }
            Expr::MapLiteral { entries, .. } => {
                for (k, v) in entries {
                    walk_expr_export(k, sealed, containers, exported, known);
                    walk_expr_export(v, sealed, containers, exported, known);
                }
            }
            Expr::Index { base, index } => {
                walk_expr_export(base, sealed, containers, exported, known);
                walk_expr_export(index, sealed, containers, exported, known);
            }
            Expr::FieldAccess { base, .. } => {
                walk_expr_export(base, sealed, containers, exported, known)
            }
            Expr::Block { stmts, tail } => {
                walk(stmts, sealed, containers, exported, known);
                if let Some(t) = tail {
                    walk_expr_export(t, sealed, containers, exported, known);
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
                walk_expr_export(cond, sealed, containers, exported, known);
                walk_expr_export(then, sealed, containers, exported, known);
                walk_expr_export(else_, sealed, containers, exported, known);
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                walk_expr_export(scrutinee, sealed, containers, exported, known);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        walk_expr_export(g, sealed, containers, exported, known);
                    }
                    walk_expr_export(&arm.body, sealed, containers, exported, known);
                }
            }
            Expr::Lambda { body, .. } => {
                walk_expr_export(body, sealed, containers, exported, known)
            }
            _ => {}
        }
    }
    walk(
        body,
        &mut sealed,
        &mut containers,
        &mut exported,
        known_container_stores,
    );
    // Map formal-name containers back to formal indices for interproc call-site seeding.
    let mut container_stores: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for (i, (n, _)) in params.iter().enumerate() {
        if let Some(idxs) = containers.get(n) {
            if !idxs.is_empty() {
                container_stores.insert(i, idxs.clone());
            }
        }
    }
    (exported, container_stores)
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

/// Root variable of a place (`arr[i].f` → `arr`). Used for index/field NE stores.
fn place_root_var(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Var(n) => Some(n.as_str()),
        Expr::Index { base, .. } | Expr::FieldAccess { base, .. } => place_root_var(base),
        _ => None,
    }
}

/// Seed `container_ne` when push/insert stores a Live NE value into a named container.
/// `method_base` is `Some` for `arr.push(v)` / `m.insert(k,v)`; free forms use `args[0]` as container.
fn note_container_ne_mutation(
    name: &str,
    args: &[Expr],
    method_base: Option<&Expr>,
    caps: &CapMap,
    lin: &mut Lin,
) {
    let (container, value) = match name {
        "push" => {
            let cont = method_base.or_else(|| args.first());
            let val = if method_base.is_some() {
                args.first()
            } else {
                args.get(1)
            };
            (cont, val)
        }
        "insert" => {
            let cont = method_base.or_else(|| args.first());
            let val = if method_base.is_some() {
                args.get(1) // m.insert(k, v)
            } else {
                args.get(2) // insert(m, k, v)
            };
            (cont, val)
        }
        _ => return,
    };
    let (Some(container), Some(value)) = (container, value) else {
        return;
    };
    if !expr_holds_live_ne(value, caps, &lin.container_ne) {
        return;
    }
    // Named root: `push(arr, v)` or `push(arrs[0], v)` → seed `arr` / `arrs`.
    if let Some(root) = place_root_var(container) {
        lin.container_ne.insert(root.to_string());
    }
}

/// True if `expr` is a Live non-exportable token, a known NE-carrying container, or an
/// aggregate that embeds either (pre-consume snapshot for store-then-project tracking).
/// Also: project from NE container (`arr[i]` / `b.f`), if/match/block value-position init.
fn expr_holds_live_ne(expr: &Expr, caps: &CapMap, container_ne: &BTreeSet<String>) -> bool {
    match expr {
        Expr::Var(n) => {
            matches!(
                caps.get(n),
                Some(t) if t.state == CapState::Live && t.non_exportable
            ) || container_ne.contains(n)
        }
        Expr::ArrayLiteral { elements }
        | Expr::EnumConstruct {
            fields: elements, ..
        } => elements
            .iter()
            .any(|e| expr_holds_live_ne(e, caps, container_ne)),
        Expr::StructLiteral { fields, .. } => fields
            .iter()
            .any(|(_, e)| expr_holds_live_ne(e, caps, container_ne)),
        Expr::MapLiteral { entries, .. } => entries
            .iter()
            .any(|(_, v)| expr_holds_live_ne(v, caps, container_ne)),
        // Project-bind: `let x = arr[0]` after arr holds NE must seed x.
        Expr::Index { base, .. } | Expr::FieldAccess { base, .. } => {
            expr_holds_live_ne(base, caps, container_ne)
                || matches!(&**base, Expr::Var(n) if container_ne.contains(n))
        }
        // if/match/block value-position container init (mirror taint/field_closures seed).
        Expr::If { then, else_, .. } | Expr::IfLet { then, else_, .. } => {
            expr_holds_live_ne(then, caps, container_ne)
                || expr_holds_live_ne(else_, caps, container_ne)
        }
        Expr::Match { arms, .. } => arms
            .iter()
            .any(|arm| expr_holds_live_ne(&arm.body, caps, container_ne)),
        Expr::Block { stmts: _, tail } => tail
            .as_ref()
            .is_some_and(|t| expr_holds_live_ne(t, caps, container_ne)),
        Expr::Unary { expr, .. } | Expr::Cast { expr, .. } | Expr::Try(expr) => {
            expr_holds_live_ne(expr, caps, container_ne)
        }
        _ => false,
    }
}

/// If `expr` names a Live non-exportable token (directly or nested in a value-position
/// aggregate), projects from an NE-carrying container (`arr[0]` after `arr=[s]`), or is a
/// call of a function that returns non-exportable, fire EXPORT (does not consume).
fn check_export_arg(expr: &Expr, caps: &CapMap, lin: &mut Lin) {
    match expr {
        Expr::Var(name) => {
            if let Some(t) = caps.get(name) {
                if t.state == CapState::Live && t.non_exportable {
                    lin.export(name);
                }
            }
            if lin.container_ne.contains(name) {
                lin.export(name);
            }
        }
        Expr::Call { callee, .. }
            if lin.all_fns.contains(callee)
                && lin.summary.returns_nonexportable.contains(callee) =>
        {
            lin.export(callee);
        }
        Expr::Index { base, index } => {
            check_export_arg(base, caps, lin);
            check_export_arg(index, caps, lin);
            if let Expr::Var(n) = &**base {
                if lin.container_ne.contains(n) {
                    lin.export(n);
                }
            }
        }
        Expr::FieldAccess { base, .. } => {
            check_export_arg(base, caps, lin);
            if let Expr::Var(n) = &**base {
                if lin.container_ne.contains(n) {
                    lin.export(n);
                }
            }
        }
        Expr::ArrayLiteral { elements }
        | Expr::EnumConstruct {
            fields: elements, ..
        } => {
            for e in elements {
                check_export_arg(e, caps, lin);
            }
        }
        Expr::StructLiteral { fields, .. } => {
            for (_, e) in fields {
                check_export_arg(e, caps, lin);
            }
        }
        Expr::MapLiteral { entries, .. } => {
            for (_, v) in entries {
                check_export_arg(v, caps, lin);
            }
        }
        Expr::If { then, else_, .. } => {
            check_export_arg(then, caps, lin);
            check_export_arg(else_, caps, lin);
        }
        Expr::IfLet { then, else_, .. } => {
            check_export_arg(then, caps, lin);
            check_export_arg(else_, caps, lin);
        }
        Expr::Match { arms, .. } => {
            for arm in arms {
                check_export_arg(&arm.body, caps, lin);
            }
        }
        Expr::Block { tail: Some(t), .. } => check_export_arg(t, caps, lin),
        Expr::Block { tail: None, .. } => {}
        // Grouping / unary wrappers that preserve a value position.
        Expr::Unary { expr, .. } | Expr::Cast { expr, .. } | Expr::Try(expr) => {
            check_export_arg(expr, caps, lin);
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
/// Deterministic: lexicographically first matching name.
/// No local match → if this function is in `caller_pays_effects` for `effect`, defer to callers;
/// otherwise UNAUTHORIZED.
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
    } else if lin
        .summary
        .caller_pays_effects
        .get(&lin.fn_name)
        .map(|s| s.contains(effect))
        .unwrap_or(false)
    {
        // Ambient interproc: callers causal-spend at the call site.
    } else {
        lin.unauthorized(effect);
    }
}

/// Well-formed `cap_export(src, "reason")`: on success consume `src` and return peeled Live
/// exportable token. Malformed → diagnostic, leave source unpeeled (if still Live).
///
/// Peel-of-param: a formal that is not yet a Live NE local may still be released — interproc
/// may pass NE into that slot; params never authorize effects (`kind: None`).
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
        // Peel-of-param: formal not tracked as Live NE local — still allow well-formed peel.
        None if reason_ok && lin.params.contains(&src_name) => {
            for a in args.iter().skip(2) {
                walk_expr(a, caps, lin);
            }
            Some(CapToken {
                state: CapState::Live,
                kind: None,
                non_exportable: false,
            })
        }
        _ => {
            // Empty reason, exportable token, non-param unknown provenance, etc. — no peel.
            lin.export_malformed();
            for a in args.iter().skip(1) {
                walk_expr(a, caps, lin);
            }
            None
        }
    }
}

/// Free Live capability names referenced inside a lambda body (explicit `cap_use` or any
/// occurrence of a tracked Live name). Used to MOVE those tokens into a linear closure.
fn free_live_caps_in_expr(expr: &Expr, caps: &CapMap, out: &mut BTreeSet<String>) {
    match expr {
        Expr::Var(n) => {
            if matches!(
                caps.get(n),
                Some(t) if t.state == CapState::Live
            ) {
                out.insert(n.clone());
            }
        }
        Expr::Call { callee, args } => {
            if callee == CAP_USE {
                if let Some(Expr::Var(n)) = args.first() {
                    if matches!(
                        caps.get(n),
                        Some(t) if t.state == CapState::Live
                    ) {
                        out.insert(n.clone());
                    }
                }
            }
            for a in args {
                free_live_caps_in_expr(a, caps, out);
            }
        }
        Expr::CallExpr { callee, args } => {
            free_live_caps_in_expr(callee, caps, out);
            for a in args {
                free_live_caps_in_expr(a, caps, out);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            free_live_caps_in_expr(lhs, caps, out);
            free_live_caps_in_expr(rhs, caps, out);
        }
        Expr::Unary { expr, .. }
        | Expr::Cast { expr, .. }
        | Expr::Tainted { inner: expr, .. }
        | Expr::Declassify { inner: expr, .. }
        | Expr::Assume(expr)
        | Expr::Assert(expr)
        | Expr::Try(expr) => free_live_caps_in_expr(expr, caps, out),
        Expr::ArrayLiteral { elements } | Expr::EnumConstruct { fields: elements, .. } => {
            for e in elements {
                free_live_caps_in_expr(e, caps, out);
            }
        }
        Expr::StructLiteral { fields, .. } => {
            for (_, v) in fields {
                free_live_caps_in_expr(v, caps, out);
            }
        }
        Expr::MapLiteral { entries, .. } => {
            for (_, v) in entries {
                free_live_caps_in_expr(v, caps, out);
            }
        }
        Expr::Index { base, index } => {
            free_live_caps_in_expr(base, caps, out);
            free_live_caps_in_expr(index, caps, out);
        }
        Expr::FieldAccess { base, .. } => free_live_caps_in_expr(base, caps, out),
        Expr::If {
            cond, then, else_, ..
        } => {
            free_live_caps_in_expr(cond, caps, out);
            free_live_caps_in_expr(then, caps, out);
            free_live_caps_in_expr(else_, caps, out);
        }
        Expr::IfLet {
            scrutinee,
            then,
            else_,
            ..
        } => {
            free_live_caps_in_expr(scrutinee, caps, out);
            free_live_caps_in_expr(then, caps, out);
            free_live_caps_in_expr(else_, caps, out);
        }
        Expr::Match { scrutinee, arms, .. } => {
            free_live_caps_in_expr(scrutinee, caps, out);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    free_live_caps_in_expr(g, caps, out);
                }
                free_live_caps_in_expr(&arm.body, caps, out);
            }
        }
        Expr::Block { stmts, tail } => {
            for s in stmts {
                free_live_caps_in_stmt(s, caps, out);
            }
            if let Some(t) = tail {
                free_live_caps_in_expr(t, caps, out);
            }
        }
        Expr::Lambda { body, .. } => free_live_caps_in_expr(body, caps, out),
        _ => {}
    }
}

fn free_live_caps_in_stmt(stmt: &Stmt, caps: &CapMap, out: &mut BTreeSet<String>) {
    match stmt {
        Stmt::Let { init, .. } | Stmt::LetPattern { init, .. } => {
            free_live_caps_in_expr(init, caps, out);
        }
        Stmt::Assign { target, value } => {
            free_live_caps_in_expr(target, caps, out);
            free_live_caps_in_expr(value, caps, out);
        }
        Stmt::ExprStmt(e) => free_live_caps_in_expr(e, caps, out),
        Stmt::If {
            cond, then, else_, ..
        } => {
            free_live_caps_in_expr(cond, caps, out);
            for s in then {
                free_live_caps_in_stmt(s, caps, out);
            }
            if let Some(el) = else_ {
                for s in el {
                    free_live_caps_in_stmt(s, caps, out);
                }
            }
        }
        Stmt::While {
            cond,
            body,
            invariant,
            ..
        } => {
            free_live_caps_in_expr(cond, caps, out);
            for inv in invariant {
                free_live_caps_in_expr(inv, caps, out);
            }
            for s in body {
                free_live_caps_in_stmt(s, caps, out);
            }
        }
        Stmt::Loop { body, invariant, .. } => {
            for inv in invariant {
                free_live_caps_in_expr(inv, caps, out);
            }
            for s in body {
                free_live_caps_in_stmt(s, caps, out);
            }
        }
        Stmt::For {
            source,
            body,
            invariant,
            ..
        } => {
            match source {
                ForSource::Range { start, end } => {
                    free_live_caps_in_expr(start, caps, out);
                    free_live_caps_in_expr(end, caps, out);
                }
                ForSource::Collection { expr } => free_live_caps_in_expr(expr, caps, out),
            }
            for inv in invariant {
                free_live_caps_in_expr(inv, caps, out);
            }
            for s in body {
                free_live_caps_in_stmt(s, caps, out);
            }
        }
        Stmt::WhileLet { expr, body, .. } => {
            free_live_caps_in_expr(expr, caps, out);
            for s in body {
                free_live_caps_in_stmt(s, caps, out);
            }
        }
        Stmt::ResearchBlock { body, .. } | Stmt::ExploitBlock { body, .. } => {
            for s in body {
                free_live_caps_in_stmt(s, caps, out);
            }
        }
        _ => {}
    }
}

/// MOVE free Live caps into a linear closure binding; mark free caps Consumed.
fn seal_linear_closure(target: &str, body: &Expr, caps: &mut CapMap, lin: &mut Lin) {
    let mut held = BTreeSet::new();
    free_live_caps_in_expr(body, caps, &mut held);
    // Always export-seal NE free uses at definition (existing path).
    let saved = lin.container_ne.clone();
    walk_export_seals(body, caps, lin);
    lin.container_ne = saved;
    if held.is_empty() {
        lin.linear_closures.remove(target);
        return;
    }
    for name in &held {
        if let Some(t) = caps.get(name).cloned() {
            if t.state == CapState::Live {
                caps.insert(
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
    lin.linear_closures
        .insert(target.to_string(), CapState::Live);
}

/// Apply a named linear closure: first call consumes it; second → REUSE.
fn apply_linear_closure(name: &str, lin: &mut Lin) {
    match lin.linear_closures.get(name).copied() {
        Some(CapState::Live) => {
            lin.linear_closures
                .insert(name.to_string(), CapState::Consumed);
        }
        Some(CapState::Consumed) => lin.reuse(name),
        None => {}
    }
}

/// Rebind `target` to the value of `init`, applying MOVE semantics when `init` is a bare tracked
/// capability variable, MINT when it is acquire / nonexportable, PEEL when `cap_export(...)`, and
/// otherwise walking `init` (which consumes any capabilities inside it) and dropping any prior
/// tracking of `target`.
fn rebind(target: &str, init: &Expr, caps: &mut CapMap, lin: &mut Lin) {
    // Drop prior linear-closure tracking on overwrite.
    lin.linear_closures.remove(target);
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
    if let Expr::Lambda { body, .. } = init {
        seal_linear_closure(target, body, caps, lin);
        caps.remove(target); // lambda binding is not a token; linear_closures tracks it
        return;
    }
    if let Expr::Var(src) = init {
        // MOVE of a linear closure binding (HO rebind).
        if let Some(st) = lin.linear_closures.get(src).copied() {
            match st {
                CapState::Live => {
                    lin.linear_closures
                        .insert(src.clone(), CapState::Consumed);
                    lin.linear_closures
                        .insert(target.to_string(), CapState::Live);
                }
                CapState::Consumed => {
                    lin.reuse(src);
                    lin.linear_closures.remove(target);
                }
            }
            caps.remove(target);
            return;
        }
        // MOVE of an NE-carrying *container* (not a token): transfer the container flag.
        if lin.container_ne.contains(src) {
            lin.container_ne.remove(src);
            lin.container_ne.insert(target.to_string());
            caps.remove(target);
            return;
        }
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
                lin.container_ne.remove(target);
                return;
            }
            Some(t) if t.state == CapState::Consumed => {
                lin.reuse(src);
                caps.remove(target);
                lin.container_ne.remove(target);
                return;
            }
            _ => {
                caps.remove(target);
                lin.container_ne.remove(target);
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
            lin.container_ne.remove(target);
            return;
        }
    }
    // Snapshot NE embedding *before* walk consumes element tokens, then mark container.
    let holds_ne = expr_holds_live_ne(init, caps, &lin.container_ne);
    walk_expr(init, caps, lin);
    caps.remove(target); // rebinding to a non-capability drops any prior capability tracking
    if holds_ne {
        lin.container_ne.insert(target.to_string());
    } else {
        lin.container_ne.remove(target);
    }
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
            // Positional list/tuple destructure: MOVE each Binding element the same way
            // `let a = s` would (so NE sealedness survives `let (a, _) = (s, 1)`).
            // Nested / non-list patterns stay residual: walk init and drop tracking.
            match (pattern, init) {
                (Pattern::List(pats), Expr::ArrayLiteral { elements })
                    if pats
                        .iter()
                        .all(|p| matches!(p, Pattern::Binding(_) | Pattern::Wildcard)) =>
                {
                    for (i, p) in pats.iter().enumerate() {
                        match (p, elements.get(i)) {
                            (Pattern::Binding(name), Some(e)) => rebind(name, e, caps, lin),
                            (Pattern::Wildcard, Some(e)) => walk_expr(e, caps, lin),
                            _ => {}
                        }
                    }
                    for e in elements.iter().skip(pats.len()) {
                        walk_expr(e, caps, lin);
                    }
                }
                _ => {
                    walk_expr(init, caps, lin);
                    for n in pattern.bound_names() {
                        caps.remove(&n);
                    }
                }
            }
        }
        Stmt::Assign { target, value } => {
            if let Expr::Var(name) = target {
                rebind(name, value, caps, lin);
            } else {
                // Place-write: `arr[i] = ne` / `b.f = ne` seeds container_ne on the root var.
                if expr_holds_live_ne(value, caps, &lin.container_ne) {
                    if let Some(root) = place_root_var(target) {
                        lin.container_ne.insert(root.to_string());
                    }
                }
                walk_expr(target, caps, lin);
                walk_expr(value, caps, lin);
            }
        }
        Stmt::If { cond, then, else_ } => {
            walk_expr(cond, caps, lin);
            let base = caps.clone();
            let base_c = lin.container_ne.clone();
            walk_stmts(then, caps, lin);
            let then_end = caps.clone();
            let then_c = lin.container_ne.clone();
            *caps = base.clone();
            lin.container_ne = base_c.clone();
            let else_end = if let Some(else_body) = else_ {
                walk_stmts(else_body, caps, lin);
                Some(caps.clone())
            } else {
                None
            };
            let else_c = lin.container_ne.clone();
            let has_implicit_arm = else_end.is_none();
            let mut ends = vec![then_end];
            if let Some(e) = else_end {
                ends.push(e);
            }
            *caps = merge_branches(&base, &ends, has_implicit_arm, lin.verified);
            // Fail-closed sealedness: NE-carrying container if *any* arm stores one.
            lin.container_ne = then_c.union(&else_c).cloned().collect();
            if has_implicit_arm {
                lin.container_ne = lin.container_ne.union(&base_c).cloned().collect();
            }
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
            // Linear closure application (deep HO): g(…) consumes the closure binding.
            apply_linear_closure(callee, lin);
            // Export check BEFORE walking args (so Live non_exportable is still visible).
            if is_export_sink(callee) && !lin.all_fns.contains(callee) {
                for a in args {
                    check_export_arg(a, caps, lin);
                }
            }
            // Interproc: callee formals that reach export sinks → check corresponding args.
            if lin.all_fns.contains(callee) {
                // Caller-pays: ambient interproc causal spend for callee's uncovered effects.
                if lin.verified {
                    if let Some(effects) = lin.summary.caller_pays_effects.get(callee) {
                        // Deterministic order for multi-effect callees.
                        for effect in effects.iter() {
                            causal_spend(effect, caps, lin);
                        }
                    }
                }
                if let Some(idxs) = lin.summary.exports_formals.get(callee) {
                    for &i in idxs {
                        if let Some(a) = args.get(i) {
                            check_export_arg(a, caps, lin);
                        }
                    }
                }
                // Interproc container mutation: callee formal container receives formal NE values
                // (e.g. stash(arr,s){push(arr,s)}). Seed caller's container arg *before* consume.
                if let Some(stores) = lin.summary.container_stores.get(callee) {
                    for (&cont_i, val_idxs) in stores {
                        let value_holds_ne = val_idxs.iter().any(|&vi| {
                            args.get(vi)
                                .is_some_and(|a| expr_holds_live_ne(a, caps, &lin.container_ne))
                        });
                        if value_holds_ne {
                            if let Some(cont_arg) = args.get(cont_i) {
                                if let Some(root) = place_root_var(cont_arg) {
                                    lin.container_ne.insert(root.to_string());
                                }
                            }
                        }
                    }
                }
            }
            // Store-then-project: free `push(arr, ne)` / `insert(m, k, ne)` seed container_ne
            // *before* args are consumed, so later arr[i]/m[k] to export sinks fail closed.
            note_container_ne_mutation(callee, args, None, caps, lin);
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
            // Method form: `arr.push(ne)` / `m.insert(k, ne)`.
            if let Expr::FieldAccess { base, field, .. } = &**callee {
                note_container_ne_mutation(field, args, Some(base), caps, lin);
            }
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
        // Non-exportable sealedness first (while free NE is still Live), then MOVE free Live
        // caps into the ephemeral lambda (consumed). Let-bound lambdas use seal_linear_closure.
        // Local container_ne seeds must not leak to outer.
        Expr::Lambda { body, .. } => {
            let saved = lin.container_ne.clone();
            walk_export_seals(body, caps, lin);
            lin.container_ne = saved;
            let mut held = BTreeSet::new();
            free_live_caps_in_expr(body, caps, &mut held);
            for name in &held {
                if let Some(t) = caps.get(name).cloned() {
                    if t.state == CapState::Live {
                        caps.insert(
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
    }
}

/// Export-seal walk: fire ANUBIS_CAPABILITY_EXPORT on sinks, without consuming tokens.
/// Also seeds `container_ne` for free-NE store-then-project inside lambda bodies
/// (`let arr = [s]; print(arr[0])` / `push(arr, s)`).
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
            // Seed container_ne *before* nested walks (mirror walk_expr).
            note_container_ne_mutation(callee, args, None, caps, lin);
            for a in args {
                walk_export_seals(a, caps, lin);
            }
        }
        Expr::CallExpr { callee, args } => {
            if let Expr::FieldAccess { base, field, .. } = &**callee {
                note_container_ne_mutation(field, args, Some(base), caps, lin);
            }
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
        Expr::FieldAccess { base, .. } => walk_export_seals(base, caps, lin),
        Expr::EnumConstruct { fields, .. } => {
            for e in fields {
                walk_export_seals(e, caps, lin);
            }
        }
        Expr::MapLiteral { entries, .. } => {
            for (k, v) in entries {
                walk_export_seals(k, caps, lin);
                walk_export_seals(v, caps, lin);
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
        Expr::Lambda { body, .. } => {
            // Nested lambda: outer free NE still visible via caps; local container_ne
            // seeds do not leak back to the outer export-seal frame.
            let saved = lin.container_ne.clone();
            walk_export_seals(body, caps, lin);
            lin.container_ne = saved;
        }
        _ => {}
    }
}

fn walk_export_seals_stmts(stmts: &[Stmt], caps: &CapMap, lin: &mut Lin) {
    // Seed container_ne on Let/Assign/push (store-then-project of free NE), then walk
    // export-reachable positions via the shared for_each_export_child shape for CF headers.
    for s in stmts {
        match s {
            Stmt::Let { name, init, .. } => {
                let holds = expr_holds_live_ne(init, caps, &lin.container_ne);
                walk_export_seals(init, caps, lin);
                if holds {
                    lin.container_ne.insert(name.clone());
                } else if let Expr::Var(src) = init {
                    if lin.container_ne.contains(src) {
                        lin.container_ne.remove(src);
                        lin.container_ne.insert(name.clone());
                    } else {
                        lin.container_ne.remove(name);
                    }
                } else {
                    lin.container_ne.remove(name);
                }
            }
            Stmt::Assign {
                target: Expr::Var(name),
                value,
            } => {
                let holds = expr_holds_live_ne(value, caps, &lin.container_ne);
                walk_export_seals(value, caps, lin);
                if holds {
                    lin.container_ne.insert(name.clone());
                } else if let Expr::Var(src) = value {
                    if lin.container_ne.contains(src) {
                        lin.container_ne.remove(src);
                        lin.container_ne.insert(name.clone());
                    } else {
                        lin.container_ne.remove(name);
                    }
                } else {
                    lin.container_ne.remove(name);
                }
            }
            Stmt::Assign { target, value } => {
                if expr_holds_live_ne(value, caps, &lin.container_ne) {
                    if let Some(root) = place_root_var(target) {
                        lin.container_ne.insert(root.to_string());
                    }
                }
                walk_export_seals(target, caps, lin);
                walk_export_seals(value, caps, lin);
            }
            Stmt::ExprStmt(e) => walk_export_seals(e, caps, lin),
            Stmt::If {
                cond, then, else_, ..
            } => {
                walk_export_seals(cond, caps, lin);
                let base_c = lin.container_ne.clone();
                walk_export_seals_stmts(then, caps, lin);
                let then_c = lin.container_ne.clone();
                lin.container_ne = base_c.clone();
                if let Some(el) = else_ {
                    walk_export_seals_stmts(el, caps, lin);
                }
                let else_c = lin.container_ne.clone();
                lin.container_ne = then_c.union(&else_c).cloned().collect();
                if else_.is_none() {
                    lin.container_ne = lin.container_ne.union(&base_c).cloned().collect();
                }
            }
            other => {
                for_each_export_child(other, |child| match child {
                    ExportReachable::Expr(e) => walk_export_seals(e, caps, lin),
                    ExportReachable::Stmts(ss) => walk_export_seals_stmts(ss, caps, lin),
                });
            }
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
        // Params still never authorize (forge closed). Root with no external caller self-auth.
        let src = r#"fn f(netcap) { send("h", 80, "x"); }"#;
        assert!(codes(src, false).is_empty());
        assert_eq!(codes(src, true), ["ANUBIS_EFFECT_UNAUTHORIZED"]);
    }

    /// Ambient interproc causal spend: caller holds matching token; callee has no local mint.
    #[test]
    fn interproc_caller_pays_ambient_causal_spend_accepts() {
        let src = r#"
            fn helper() { send("h", 80, "x"); }
            fn f() {
                let n = cap_acquire("net.send");
                helper();
            }
        "#;
        assert!(
            codes(src, true).is_empty(),
            "caller-pays ambient must accept, got {:?}",
            codes(src, true)
        );
        assert!(codes(src, false).is_empty());
    }

    /// Dual: caller without token still UNAUTHORIZED at call site.
    #[test]
    fn interproc_caller_pays_without_token_is_unauthorized() {
        let src = r#"
            fn helper() { send("h", 80, "x"); }
            fn f() { helper(); }
        "#;
        assert_eq!(codes(src, true), ["ANUBIS_EFFECT_UNAUTHORIZED"]);
    }

    /// Local acquire in callee covers effect — caller need not pay.
    #[test]
    fn interproc_callee_local_acquire_covers_without_caller_token() {
        let src = r#"
            fn helper() {
                let n = cap_acquire("net.send");
                send("h", 80, "x");
            }
            fn f() { helper(); }
        "#;
        assert!(
            codes(src, true).is_empty(),
            "callee-local acquire must cover, got {:?}",
            codes(src, true)
        );
    }

    /// Transitive mid: caller pays once at outer call.
    #[test]
    fn interproc_caller_pays_through_mid_accepts() {
        let src = r#"
            fn helper() { send("h", 80, "x"); }
            fn mid() { helper(); }
            fn f() {
                let n = cap_acquire("net.send");
                mid();
            }
        "#;
        assert!(
            codes(src, true).is_empty(),
            "transitive caller-pays must accept, got {:?}",
            codes(src, true)
        );
    }

    /// After caller-pays spend, second call without fresh token is unauthorized.
    #[test]
    fn interproc_caller_pays_double_call_is_unauthorized() {
        let src = r#"
            fn helper() { send("h", 80, "x"); }
            fn f() {
                let n = cap_acquire("net.send");
                helper();
                helper();
            }
        "#;
        assert_eq!(codes(src, true), ["ANUBIS_EFFECT_UNAUTHORIZED"]);
    }

    /// NE token ambient interproc spend (export-seal orthogonal).
    #[test]
    fn interproc_caller_pays_nonexportable_token_accepts() {
        let src = r#"
            fn helper() { send("h", 80, "x"); }
            fn f() {
                let n = cap_acquire_nonexportable("net.send");
                helper();
            }
        "#;
        assert!(codes(src, true).is_empty());
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

    /// Dual matrix: interproc formal NE × header positions (skeptic-2).
    #[test]
    fn nonexportable_param_print_in_if_cond_is_export() {
        let src = r#"
            fn leak(c) { if print(c) { let _ = 1; } }
            fn f() {
                let s = cap_acquire_nonexportable("fs.write");
                leak(s);
            }
        "#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "formal in if-cond must EXPORT at call site, got {c:?}"
        );
    }

    #[test]
    fn nonexportable_param_print_in_while_cond_is_export() {
        let src = r#"
            fn leak(c) { while print(c) { break; } }
            fn f() {
                let s = cap_acquire_nonexportable("fs.write");
                leak(s);
            }
        "#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "formal in while-cond must EXPORT at call site, got {c:?}"
        );
    }

    #[test]
    fn nonexportable_param_print_in_for_source_is_export() {
        let src = r#"
            fn leak(c) { for i in print(c)..1 { break; } }
            fn f() {
                let s = cap_acquire_nonexportable("fs.write");
                leak(s);
            }
        "#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "formal in for-source must EXPORT at call site, got {c:?}"
        );
    }

    #[test]
    fn nonexportable_param_print_in_while_let_expr_is_export() {
        let src = r#"
            fn leak(c) { while let Some(y) = Some(print(c)) { break; } }
            fn f() {
                let s = cap_acquire_nonexportable("fs.write");
                leak(s);
            }
        "#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "formal in while-let expr must EXPORT at call site, got {c:?}"
        );
    }

    #[test]
    fn nonexportable_param_print_in_if_body_is_export() {
        let src = r#"
            fn leak(c) { if true { print(c); } }
            fn f() {
                let s = cap_acquire_nonexportable("fs.write");
                leak(s);
            }
        "#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "formal in if-body must EXPORT at call site, got {c:?}"
        );
    }

    #[test]
    fn ordinary_param_print_in_if_cond_is_not_export() {
        let src = r#"
            fn leak(c) { if print(c) { let _ = 1; } }
            fn f() {
                let s = cap_acquire("fs.read");
                leak(s);
            }
        "#;
        assert!(
            codes(src, true).is_empty(),
            "exportable formal in if-cond must not EXPORT, got {:?}",
            codes(src, true)
        );
    }

    #[test]
    fn ordinary_param_print_in_while_cond_is_not_export() {
        let src = r#"
            fn leak(c) { while print(c) { break; } }
            fn f() {
                let s = cap_acquire("fs.read");
                leak(s);
            }
        "#;
        assert!(
            codes(src, true).is_empty(),
            "exportable formal in while-cond must not EXPORT, got {:?}",
            codes(src, true)
        );
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

    /// Peel-of-param: callee peels formal then prints — call site must accept (formal not exporting).
    #[test]
    fn nonexportable_peel_of_param_then_print_accepts() {
        let src = r#"
            fn release(s) {
                let e = cap_export(s, "audit");
                print(e);
            }
            fn f() {
                let s = cap_acquire_nonexportable("fs.write");
                release(s);
            }
        "#;
        for verified in [false, true] {
            let c = codes(src, verified);
            assert!(
                c.is_empty(),
                "peel-of-param then print must accept, verified={verified}, got {c:?}"
            );
        }
    }

    /// Peel-of-param in-place: `cap_export(s, "reason"); print(s)` rebinds formal as exportable.
    #[test]
    fn nonexportable_peel_of_param_inplace_then_print_accepts() {
        let src = r#"
            fn release(s) {
                cap_export(s, "audit");
                print(s);
            }
            fn f() {
                let s = cap_acquire_nonexportable("fs.write");
                release(s);
            }
        "#;
        assert!(
            codes(src, true).is_empty(),
            "inplace peel-of-param must accept, got {:?}",
            codes(src, true)
        );
    }

    /// Dual: empty reason on formal is MALFORMED; formal still sealed so print(s) EXPORT at call.
    #[test]
    fn nonexportable_peel_of_param_empty_reason_is_malformed() {
        let src = r#"
            fn release(s) {
                let e = cap_export(s, "");
                print(s);
            }
            fn f() {
                let s = cap_acquire_nonexportable("fs.write");
                release(s);
            }
        "#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT_MALFORMED"),
            "empty-reason peel-of-param must be MALFORMED, got {c:?}"
        );
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "empty peel must leave formal exporting at call site, got {c:?}"
        );
    }

    /// Dual: non-param unknown provenance still cannot peel.
    #[test]
    fn nonexportable_peel_of_return_unknown_is_malformed() {
        let src = r#"
            fn get() { return 1; }
            fn f() {
                let x = get();
                let e = cap_export(x, "audit");
                print(e);
            }
        "#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT_MALFORMED"),
            "peel of non-param unknown must MALFORMED, got {c:?}"
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
    /// Deep HO: double apply of a cap-capturing closure is REUSE.
    #[test]
    fn linear_closure_double_apply_is_reuse() {
        let src = r#"fn f() {
            let s = cap_acquire("fs.write");
            let g = |x| { cap_use(s); };
            g(0);
            g(0);
        }"#;
        assert_eq!(codes(src, false), ["ANUBIS_CAPABILITY_REUSE"]);
        assert_eq!(codes(src, true), ["ANUBIS_CAPABILITY_REUSE"]);
    }

    /// Deep HO: move closure then double-apply the alias is REUSE.
    #[test]
    fn linear_closure_move_then_double_apply_is_reuse() {
        let src = r#"fn f() {
            let s = cap_acquire("fs.write");
            let g = |x| { cap_use(s); };
            let h = g;
            h(0);
            h(0);
        }"#;
        assert_eq!(codes(src, true), ["ANUBIS_CAPABILITY_REUSE"]);
    }

    /// Dual: single apply of capturing closure accepts.
    #[test]
    fn linear_closure_single_apply_accepts() {
        let src = r#"fn f() {
            let s = cap_acquire("fs.write");
            let g = |x| { cap_use(s); };
            g(0);
        }"#;
        assert!(
            codes(src, true).is_empty(),
            "single apply must accept, got {:?}",
            codes(src, true)
        );
    }

    /// Dual: non-capturing closure may be applied twice.
    #[test]
    fn noncapturing_closure_double_apply_accepts() {
        let src = r#"fn f() {
            let g = |x| { let _ = x; };
            g(0);
            g(1);
        }"#;
        assert!(
            codes(src, true).is_empty(),
            "non-capturing double apply must accept, got {:?}",
            codes(src, true)
        );
    }

    /// After move to h, original g is consumed — apply g is REUSE.
    #[test]
    fn linear_closure_apply_after_move_is_reuse() {
        let src = r#"fn f() {
            let s = cap_acquire("fs.write");
            let g = |x| { cap_use(s); };
            let h = g;
            g(0);
        }"#;
        assert_eq!(codes(src, true), ["ANUBIS_CAPABILITY_REUSE"]);
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
    fn nonexportable_print_in_if_cond_inside_lambda_is_export() {
        // skeptic-1: If.cond must be sealed (not only then/else arms).
        let src = r#"fn f() {
            let s = cap_acquire_nonexportable("fs.write");
            let g = |x| { if print(s) { 1 } else { 0 }; };
            g(0);
        }"#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "if-cond print of NE in lambda must EXPORT, got {c:?}"
        );
    }

    #[test]
    fn nonexportable_print_in_while_cond_inside_lambda_is_export() {
        let src = r#"fn f() {
            let s = cap_acquire_nonexportable("fs.write");
            let g = |x| { while print(s) { break; } };
            g(0);
        }"#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "while-cond print of NE in lambda must EXPORT, got {c:?}"
        );
    }

    #[test]
    fn nonexportable_print_in_for_source_inside_lambda_is_export() {
        let src = r#"fn f() {
            let s = cap_acquire_nonexportable("fs.write");
            let g = |x| { for i in print(s)..1 { break; } };
            g(0);
        }"#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "for-source print of NE in lambda must EXPORT, got {c:?}"
        );
    }

    #[test]
    fn nonexportable_print_in_while_let_expr_inside_lambda_is_export() {
        let src = r#"fn f() {
            let s = cap_acquire_nonexportable("fs.write");
            let g = |x| { while let Some(y) = Some(print(s)) { break; } };
            g(0);
        }"#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "while-let expr print of NE in lambda must EXPORT, got {c:?}"
        );
    }

    #[test]
    fn ordinary_print_in_if_cond_inside_lambda_is_not_export() {
        let src = r#"fn f() {
            let c = cap_acquire("fs.read");
            let g = |x| { if print(c) { 1 } else { 0 }; };
            g(0);
        }"#;
        assert!(
            codes(src, true).is_empty(),
            "exportable mint in if-cond must not EXPORT, got {:?}",
            codes(src, true)
        );
    }

    #[test]
    fn ordinary_print_in_while_cond_inside_lambda_is_not_export() {
        let src = r#"fn f() {
            let c = cap_acquire("fs.read");
            let g = |x| { while print(c) { break; } };
            g(0);
        }"#;
        assert!(
            codes(src, true).is_empty(),
            "exportable mint in while-cond must not EXPORT, got {:?}",
            codes(src, true)
        );
    }

    #[test]
    fn nonexportable_tuple_arg_to_print_is_export() {
        // Aggregate peel: print((s, 1)) must not launder past bare-var check.
        let src = r#"fn f() {
            let s = cap_acquire_nonexportable("fs.write");
            print((s, 1));
        }"#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "tuple-wrapped NE print must EXPORT, got {c:?}"
        );
    }

    #[test]
    fn nonexportable_list_literal_arg_to_print_is_export() {
        let src = r#"fn f() {
            let s = cap_acquire_nonexportable("fs.write");
            print([s]);
        }"#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "list-wrapped NE print must EXPORT, got {c:?}"
        );
    }

    #[test]
    fn nonexportable_let_pattern_move_print_is_export() {
        // Positional destructure MOVE: let (a, _) = (s, 1); print(a)
        let src = r#"fn f() {
            let s = cap_acquire_nonexportable("fs.write");
            let (a, b) = (s, 1);
            print(a);
        }"#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "LetPattern move of NE must keep sealedness, got {c:?}"
        );
    }

    #[test]
    fn ordinary_tuple_arg_to_print_is_not_export() {
        // Clean dual: exportable mint may appear inside aggregate args.
        let src = r#"fn f() {
            let c = cap_acquire("fs.read");
            print((c, 1));
        }"#;
        assert!(
            codes(src, true).is_empty(),
            "exportable mint in tuple must not EXPORT, got {:?}",
            codes(src, true)
        );
    }

    #[test]
    fn ordinary_let_pattern_move_print_is_not_export() {
        let src = r#"fn f() {
            let c = cap_acquire("fs.read");
            let (a, b) = (c, 1);
            print(a);
        }"#;
        assert!(
            codes(src, true).is_empty(),
            "exportable LetPattern move must not EXPORT, got {:?}",
            codes(src, true)
        );
    }

    #[test]
    fn nonexportable_store_then_index_print_is_export() {
        // Dual-use seal: let arr = [s]; print(arr[0]) must not launder NE.
        let src = r#"fn f() {
            let s = cap_acquire_nonexportable("fs.write");
            let arr = [s];
            print(arr[0]);
        }"#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "store-then-index NE must EXPORT, got {c:?}"
        );
    }

    #[test]
    fn nonexportable_store_then_map_get_print_is_export() {
        let src = r#"fn f() {
            let s = cap_acquire_nonexportable("fs.write");
            let m = {"k": s};
            print(m["k"]);
        }"#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "store-then-map-get NE must EXPORT, got {c:?}"
        );
    }

    #[test]
    fn ordinary_store_then_index_print_is_not_export() {
        let src = r#"fn f() {
            let c = cap_acquire("fs.read");
            let arr = [c];
            print(arr[0]);
        }"#;
        assert!(
            codes(src, true).is_empty(),
            "exportable store-then-index must not EXPORT, got {:?}",
            codes(src, true)
        );
    }

    #[test]
    fn nonexportable_push_then_index_print_is_export() {
        let src = r#"fn f() {
            let s = cap_acquire_nonexportable("fs.write");
            let arr = [];
            push(arr, s);
            print(arr[0]);
        }"#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "push-then-index NE must EXPORT, got {c:?}"
        );
    }

    #[test]
    fn nonexportable_method_push_then_index_print_is_export() {
        let src = r#"fn f() {
            let s = cap_acquire_nonexportable("fs.write");
            let arr = [];
            arr.push(s);
            print(arr[0]);
        }"#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "method push-then-index NE must EXPORT, got {c:?}"
        );
    }

    #[test]
    fn nonexportable_insert_then_get_print_is_export() {
        let src = r#"fn f() {
            let s = cap_acquire_nonexportable("fs.write");
            let m = {};
            insert(m, "k", s);
            print(m["k"]);
        }"#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "insert-then-get NE must EXPORT, got {c:?}"
        );
    }

    #[test]
    fn nonexportable_push_then_print_container_is_export() {
        let src = r#"fn f() {
            let s = cap_acquire_nonexportable("fs.write");
            let arr = [];
            push(arr, s);
            print(arr);
        }"#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "push-then-print container must EXPORT, got {c:?}"
        );
    }

    #[test]
    fn ordinary_push_then_index_print_is_not_export() {
        let src = r#"fn f() {
            let c = cap_acquire("fs.read");
            let arr = [];
            push(arr, c);
            print(arr[0]);
        }"#;
        assert!(
            codes(src, true).is_empty(),
            "exportable push-then-index must not EXPORT, got {:?}",
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

    #[test]
    fn nonexportable_formal_push_then_index_print_is_export() {
        // Interproc store-then-project: formal s → push(arr,s) → print(arr[0]).
        let src = r#"
            fn leak(s) {
                let arr = [];
                push(arr, s);
                print(arr[0]);
            }
            fn f() {
                let t = cap_acquire_nonexportable("fs.write");
                leak(t);
            }
        "#;
        for verified in [false, true] {
            let c = codes(src, verified);
            assert!(
                c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
                "formal push-then-index must EXPORT, verified={verified}, got {c:?}"
            );
        }
    }

    #[test]
    fn nonexportable_formal_store_then_index_print_is_export() {
        let src = r#"
            fn leak(s) {
                let arr = [s];
                print(arr[0]);
            }
            fn f() {
                let t = cap_acquire_nonexportable("fs.write");
                leak(t);
            }
        "#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "formal store-then-index must EXPORT, got {c:?}"
        );
    }

    #[test]
    fn ordinary_formal_push_then_index_print_is_not_export() {
        let src = r#"
            fn leak(s) {
                let arr = [];
                push(arr, s);
                print(arr[0]);
            }
            fn f() {
                let t = cap_acquire("fs.read");
                leak(t);
            }
        "#;
        assert!(
            codes(src, true).is_empty(),
            "exportable formal push-then-index must not EXPORT, got {:?}",
            codes(src, true)
        );
    }

    #[test]
    fn nonexportable_lambda_store_then_project_print_is_export() {
        // Free NE stored into local container inside lambda body, then projected.
        let src = r#"fn f() {
            let s = cap_acquire_nonexportable("fs.write");
            let g = |x| {
                let arr = [s];
                print(arr[0]);
            };
            g(0);
        }"#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "lambda store-then-project free NE must EXPORT, got {c:?}"
        );
    }

    #[test]
    fn nonexportable_lambda_push_then_project_print_is_export() {
        let src = r#"fn f() {
            let s = cap_acquire_nonexportable("fs.write");
            let g = |x| {
                let arr = [];
                push(arr, s);
                print(arr[0]);
            };
            g(0);
        }"#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "lambda push-then-project free NE must EXPORT, got {c:?}"
        );
    }

    #[test]
    fn ordinary_lambda_store_then_project_print_is_not_export() {
        let src = r#"fn f() {
            let c = cap_acquire("fs.read");
            let g = |x| {
                let arr = [c];
                print(arr[0]);
            };
            g(0);
        }"#;
        assert!(
            codes(src, true).is_empty(),
            "exportable lambda store-then-project must not EXPORT, got {:?}",
            codes(src, true)
        );
    }

    #[test]
    fn nonexportable_project_bind_then_print_is_export() {
        // Hostile A15: let x = arr[0]; print(x) must not launder NE.
        let src = r#"fn f() {
            let s = cap_acquire_nonexportable("fs.write");
            let arr = [s];
            let x = arr[0];
            print(x);
        }"#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "project-bind then print must EXPORT, got {c:?}"
        );
    }

    #[test]
    fn nonexportable_formal_project_bind_then_print_is_export() {
        let src = r#"
            fn leak(s) {
                let arr = [s];
                let x = arr[0];
                print(x);
            }
            fn f() {
                let t = cap_acquire_nonexportable("fs.write");
                leak(t);
            }
        "#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "formal project-bind then print must EXPORT, got {c:?}"
        );
    }

    #[test]
    fn nonexportable_if_expr_store_then_index_print_is_export() {
        let src = r#"fn f() {
            let s = cap_acquire_nonexportable("fs.write");
            let arr = if true { [s] } else { [] };
            print(arr[0]);
        }"#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "if-expr container store must EXPORT on project, got {c:?}"
        );
    }

    #[test]
    fn nonexportable_formal_if_expr_store_then_index_print_is_export() {
        let src = r#"
            fn leak(s) {
                let arr = if true { [s] } else { [] };
                print(arr[0]);
            }
            fn f() {
                let t = cap_acquire_nonexportable("fs.write");
                leak(t);
            }
        "#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "formal if-expr store must EXPORT, got {c:?}"
        );
    }

    #[test]
    fn nonexportable_push_nested_place_then_print_is_export() {
        // push(arrs[0], s) seeds root arrs; project must EXPORT.
        let src = r#"fn f() {
            let s = cap_acquire_nonexportable("fs.write");
            let arrs = [[]];
            push(arrs[0], s);
            print(arrs[0][0]);
        }"#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "push on nested place must EXPORT, got {c:?}"
        );
    }

    #[test]
    fn ordinary_project_bind_then_print_is_not_export() {
        let src = r#"fn f() {
            let c = cap_acquire("fs.read");
            let arr = [c];
            let x = arr[0];
            print(x);
        }"#;
        assert!(
            codes(src, true).is_empty(),
            "exportable project-bind must not EXPORT, got {:?}",
            codes(src, true)
        );
    }

    #[test]
    fn ordinary_if_expr_store_then_index_print_is_not_export() {
        let src = r#"fn f() {
            let c = cap_acquire("fs.read");
            let arr = if true { [c] } else { [] };
            print(arr[0]);
        }"#;
        assert!(
            codes(src, true).is_empty(),
            "exportable if-expr store must not EXPORT, got {:?}",
            codes(src, true)
        );
    }

    #[test]
    fn nonexportable_interproc_stash_push_then_index_print_is_export() {
        // Direct-callee container mutation: stash(arr,s){push(arr,s)}; print(arr[0]).
        let src = r#"
            fn stash(arr, s) {
                push(arr, s);
            }
            fn leak(s) {
                let arr = [];
                stash(arr, s);
                print(arr[0]);
            }
            fn f() {
                let t = cap_acquire_nonexportable("fs.write");
                leak(t);
            }
        "#;
        for verified in [false, true] {
            let c = codes(src, verified);
            assert!(
                c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
                "interproc stash push must EXPORT, verified={verified}, got {c:?}"
            );
        }
    }

    #[test]
    fn nonexportable_interproc_stash_insert_then_get_print_is_export() {
        let src = r#"
            fn put(m, s) {
                insert(m, "k", s);
            }
            fn leak(s) {
                let m = {};
                put(m, s);
                print(m["k"]);
            }
            fn f() {
                let t = cap_acquire_nonexportable("fs.write");
                leak(t);
            }
        "#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "interproc stash insert must EXPORT, got {c:?}"
        );
    }

    #[test]
    fn nonexportable_interproc_stash_method_push_then_index_print_is_export() {
        let src = r#"
            fn stash(arr, s) {
                arr.push(s);
            }
            fn leak(s) {
                let arr = [];
                stash(arr, s);
                print(arr[0]);
            }
            fn f() {
                let t = cap_acquire_nonexportable("fs.write");
                leak(t);
            }
        "#;
        let c = codes(src, true);
        assert!(
            c.contains(&"ANUBIS_CAPABILITY_EXPORT"),
            "interproc method-push stash must EXPORT, got {c:?}"
        );
    }

    #[test]
    fn ordinary_interproc_stash_push_then_index_print_is_not_export() {
        let src = r#"
            fn stash(arr, s) {
                push(arr, s);
            }
            fn leak(s) {
                let arr = [];
                stash(arr, s);
                print(arr[0]);
            }
            fn f() {
                let t = cap_acquire("fs.read");
                leak(t);
            }
        "#;
        assert!(
            codes(src, true).is_empty(),
            "exportable interproc stash must not EXPORT, got {:?}",
            codes(src, true)
        );
    }
}
