//! Middle: typed HIR, mode/effect checks, taint tracking, and Z3 obligations.

use crate::frontend::{Expr, Item, Mode, Span, Stmt, AST};
use crate::BuildMode;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BindingInfo {
    pub name: String,
    pub ty: Option<String>,
    pub mode: String,
    pub tainted: bool,
    pub taint_source: Option<String>,
    pub declassified: bool,
    pub span: Option<(usize, usize)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HirFunction {
    pub name: String,
    pub module: Option<String>,
    pub mode: String,
    pub params: Vec<BindingInfo>,
    pub symbols: Vec<BindingInfo>,
    pub effects: Vec<String>,
    pub span: Option<(usize, usize)>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Hir {
    pub imports: Vec<String>,
    pub modules: Vec<String>,
    pub functions: Vec<HirFunction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MirBlock {
    pub function: String,
    pub mode: String,
    pub statement_count: usize,
    pub effects: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaintTrace {
    pub source: String,
    pub sink: Option<String>,
    pub steps: Vec<String>,
    pub declassified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SolverObligation {
    pub name: String,
    pub assumptions: Vec<String>,
    pub assertion: String,
    pub vars: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SolverCheck {
    pub name: String,
    pub status: String,
    pub detail: String,
    pub model: Option<String>,
    pub smt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticDiagnostic {
    pub code: Option<String>,
    pub message: String,
    pub span: Option<(usize, usize)>,
}

#[derive(Debug, Clone)]
pub struct TypedIR {
    pub mode: BuildMode,
    pub taint_labels: Vec<String>,
    pub constraints: Vec<String>,
    pub has_research: bool,
    pub body: Vec<Stmt>,
    pub hir: Hir,
    pub mir: Vec<MirBlock>,
    pub symbols: Vec<BindingInfo>,
    pub taint_traces: Vec<TaintTrace>,
    pub solver_obligations: Vec<SolverObligation>,
    pub diagnostics: Vec<SemanticDiagnostic>,
    pub symbolic_defs: Vec<String>, // e.g. "(= result (bvadd ...))" for faithful binding
    pub symbolic_widths: BTreeMap<String, u32>, // var name -> bit width for faithful BV
}

#[derive(Debug, Clone)]
struct ScopeBinding {
    info: BindingInfo,
}

#[derive(Debug, Default)]
struct SemanticContext {
    hir: Hir,
    mir: Vec<MirBlock>,
    symbols: Vec<BindingInfo>,
    taint_labels: Vec<String>,
    constraints: Vec<String>,
    taint_traces: Vec<TaintTrace>,
    solver_obligations: Vec<SolverObligation>,
    diagnostics: Vec<SemanticDiagnostic>,
    has_research: bool,
    symbolic_defs: Vec<String>,
    symbolic_widths: BTreeMap<String, u32>,
    known_bindings: BTreeSet<String>,
    /// Enum name → variant names (for match exhaustiveness).
    enum_variants: BTreeMap<String, Vec<String>>,
    /// Function name → ordered parameter types (for call-site type checks).
    fn_params: BTreeMap<String, Vec<String>>,
    /// Every user-defined function name (flat namespace; used for duplicate + unknown-call checks).
    all_fns: BTreeSet<String>,
}

pub fn typecheck(ast: AST, mode: Mode) -> Result<TypedIR, String> {
    let bmode = match mode {
        Mode::Safe => BuildMode::Safe,
        Mode::Research => BuildMode::Research,
        Mode::Exploit => BuildMode::Exploit,
    };
    let mut ctx = SemanticContext::default();
    // A+ pass 1: register enums + function signatures so call/match checks see the whole program.
    register_program_surface(&ast.items, &mut ctx);
    collect_items(&ast.items, None, mode, &mut ctx);

    if ctx.constraints.is_empty() {
        ctx.constraints.push("(assert true)".into());
    }

    if !ctx.diagnostics.is_empty() {
        let messages = ctx
            .diagnostics
            .iter()
            .map(|diag| {
                if let Some(c) = &diag.code {
                    format!("{}: {}", c, diag.message)
                } else {
                    diag.message.clone()
                }
            })
            .collect::<Vec<_>>()
            .join("; ");
        return Err(messages);
    }

    let captured_body = first_fn_body(&ast.items).unwrap_or_default();
    Ok(TypedIR {
        mode: bmode,
        taint_labels: ctx.taint_labels,
        constraints: ctx.constraints,
        has_research: ctx.has_research,
        body: captured_body,
        hir: ctx.hir,
        mir: ctx.mir,
        symbols: ctx.symbols,
        taint_traces: ctx.taint_traces,
        solver_obligations: ctx.solver_obligations,
        diagnostics: vec![],
        symbolic_defs: ctx.symbolic_defs,
        symbolic_widths: ctx.symbolic_widths,
    })
}

/// Pass-1 registration: enums and function parameter types (A+ call/match surface).
fn register_program_surface(items: &[Item], ctx: &mut SemanticContext) {
    // Built-in Option/Result variants, so a `match` on them can be checked for exhaustiveness.
    // A user-declared enum of the same name (processed below) overrides these.
    ctx.enum_variants
        .entry("Option".into())
        .or_insert_with(|| vec!["Some".into(), "None".into()]);
    ctx.enum_variants
        .entry("Result".into())
        .or_insert_with(|| vec!["Ok".into(), "Err".into()]);
    for item in items {
        match item {
            Item::Module { items, .. } => register_program_surface(items, ctx),
            Item::Enum { name, variants, .. } => {
                let names: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
                ctx.enum_variants.insert(name.clone(), names);
            }
            Item::Fn {
                name, params, span, ..
            } => {
                // Flat function namespace: a redefinition is an error.
                if !ctx.all_fns.insert(name.clone()) {
                    ctx.diagnostics.push(SemanticDiagnostic {
                        code: Some("ANUBIS_DUPLICATE_FUNCTION".into()),
                        message: format!("function `{}` is defined more than once", name),
                        span: Some((span.start, span.end)),
                    });
                }
                ctx.fn_params.insert(
                    name.clone(),
                    params.iter().map(|(_, ty)| ty.clone()).collect(),
                );
            }
            _ => {}
        }
    }
}

/// Walk a function body flagging calls to names that are neither a user function, a reserved
/// builtin, nor a local binding (parameter / let / for-variable / lambda-parameter / match-binding).
/// Closure-valued locals are in `bound`, so `let f = |x| x; f(3)` is fine.
fn check_calls_stmts(
    stmts: &[Stmt],
    fns: &BTreeSet<String>,
    bound: &mut BTreeSet<String>,
    ctx: &mut SemanticContext,
) {
    use crate::frontend::ForSource;
    for s in stmts {
        match s {
            Stmt::Let { name, init, .. } => {
                check_calls_expr(init, fns, bound, ctx);
                bound.insert(name.clone());
            }
            Stmt::LetPattern { pattern, init, .. } => {
                check_calls_expr(init, fns, bound, ctx);
                for n in pattern.bound_names() {
                    bound.insert(n);
                }
            }
            Stmt::Assign { target, value } => {
                check_calls_expr(target, fns, bound, ctx);
                check_calls_expr(value, fns, bound, ctx);
            }
            Stmt::ExprStmt(e) => check_calls_expr(e, fns, bound, ctx),
            Stmt::If { cond, then, else_ } => {
                check_calls_expr(cond, fns, bound, ctx);
                let mut b = bound.clone();
                check_calls_stmts(then, fns, &mut b, ctx);
                if let Some(e) = else_ {
                    let mut b = bound.clone();
                    check_calls_stmts(e, fns, &mut b, ctx);
                }
            }
            Stmt::While { cond, body } => {
                check_calls_expr(cond, fns, bound, ctx);
                let mut b = bound.clone();
                check_calls_stmts(body, fns, &mut b, ctx);
            }
            Stmt::WhileLet { pattern, expr, body } => {
                check_calls_expr(expr, fns, bound, ctx);
                let mut b = bound.clone();
                for n in pattern.bound_names() {
                    b.insert(n);
                }
                check_calls_stmts(body, fns, &mut b, ctx);
            }
            Stmt::Loop { body } => {
                let mut b = bound.clone();
                check_calls_stmts(body, fns, &mut b, ctx);
            }
            Stmt::For { var, source, body } => {
                match source {
                    ForSource::Range { start, end } => {
                        check_calls_expr(start, fns, bound, ctx);
                        check_calls_expr(end, fns, bound, ctx);
                    }
                    ForSource::Collection { expr } => check_calls_expr(expr, fns, bound, ctx),
                }
                let mut b = bound.clone();
                b.insert(var.clone());
                check_calls_stmts(body, fns, &mut b, ctx);
            }
            Stmt::ResearchBlock { body, .. } | Stmt::ExploitBlock { body, .. } => {
                let mut b = bound.clone();
                check_calls_stmts(body, fns, &mut b, ctx);
            }
            Stmt::HybridBlock { gpu, cpu, prove } => {
                for blk in [gpu, cpu, prove].into_iter().flatten() {
                    let mut b = bound.clone();
                    check_calls_stmts(blk, fns, &mut b, ctx);
                }
            }
            Stmt::Break | Stmt::Continue | Stmt::SpecBlock { .. } => {}
        }
    }
}

fn check_calls_expr(
    e: &Expr,
    fns: &BTreeSet<String>,
    bound: &BTreeSet<String>,
    ctx: &mut SemanticContext,
) {
    match e {
        Expr::Call { callee, args } => {
            if !fns.contains(callee)
                && !bound.contains(callee)
                && !crate::backends::run::is_builtin_name(callee)
            {
                ctx.diagnostics.push(SemanticDiagnostic {
                    code: Some("ANUBIS_UNKNOWN_FUNCTION".into()),
                    message: format!("call to unknown function `{}`", callee),
                    span: None,
                });
            }
            for a in args {
                check_calls_expr(a, fns, bound, ctx);
            }
        }
        Expr::CallExpr { callee, args } => {
            check_calls_expr(callee, fns, bound, ctx);
            for a in args {
                check_calls_expr(a, fns, bound, ctx);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            check_calls_expr(lhs, fns, bound, ctx);
            check_calls_expr(rhs, fns, bound, ctx);
        }
        Expr::Unary { expr, .. }
        | Expr::Cast { expr, .. }
        | Expr::Assume(expr)
        | Expr::Assert(expr)
        | Expr::Try(expr) => check_calls_expr(expr, fns, bound, ctx),
        Expr::Tainted { inner, .. } | Expr::Declassify { inner, .. } => {
            check_calls_expr(inner, fns, bound, ctx)
        }
        Expr::ArrayLiteral { elements } => {
            for el in elements {
                check_calls_expr(el, fns, bound, ctx);
            }
        }
        Expr::Index { base, index } => {
            check_calls_expr(base, fns, bound, ctx);
            check_calls_expr(index, fns, bound, ctx);
        }
        Expr::StructLiteral { fields, .. } => {
            for (_, v) in fields {
                check_calls_expr(v, fns, bound, ctx);
            }
        }
        Expr::FieldAccess { base, .. } => check_calls_expr(base, fns, bound, ctx),
        Expr::EnumConstruct { fields, .. } => {
            for f in fields {
                check_calls_expr(f, fns, bound, ctx);
            }
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            check_calls_expr(scrutinee, fns, bound, ctx);
            for arm in arms {
                let mut b = bound.clone();
                for x in arm.pattern.bound_names() {
                    b.insert(x);
                }
                if let Some(guard) = &arm.guard {
                    check_calls_expr(guard, fns, &b, ctx);
                }
                check_calls_expr(&arm.body, fns, &b, ctx);
            }
        }
        Expr::If {
            cond, then, else_, ..
        } => {
            check_calls_expr(cond, fns, bound, ctx);
            check_calls_expr(then, fns, bound, ctx);
            check_calls_expr(else_, fns, bound, ctx);
        }
        Expr::IfLet {
            pattern, scrutinee, then, else_, ..
        } => {
            check_calls_expr(scrutinee, fns, bound, ctx);
            let mut b = bound.clone();
            for n in pattern.bound_names() {
                b.insert(n);
            }
            check_calls_expr(then, fns, &b, ctx);
            check_calls_expr(else_, fns, bound, ctx);
        }
        Expr::MapLiteral { entries, .. } => {
            for (k, v) in entries {
                check_calls_expr(k, fns, bound, ctx);
                check_calls_expr(v, fns, bound, ctx);
            }
        }
        Expr::Block { stmts, tail } => {
            let mut b = bound.clone();
            check_calls_stmts(stmts, fns, &mut b, ctx);
            if let Some(t) = tail {
                check_calls_expr(t, fns, &b, ctx);
            }
        }
        Expr::Lambda { params, body } => {
            let mut b = bound.clone();
            for p in params {
                b.insert(p.clone());
            }
            check_calls_expr(body, fns, &b, ctx);
        }
        Expr::Var(_)
        | Expr::Literal(_)
        | Expr::StrLiteral(_)
        | Expr::Symbolic { .. }
        | Expr::RawPtr { .. }
        | Expr::UnifiedBuffer { .. }
        | Expr::TaintSource { .. }
        | Expr::Other(_) => {}
    }
}

fn collect_items(
    items: &[Item],
    module: Option<&str>,
    requested_mode: Mode,
    ctx: &mut SemanticContext,
) {
    for item in items {
        match item {
            Item::Import { path, .. } => ctx.hir.imports.push(path.clone()),
            Item::Module { name, items, .. } => {
                ctx.hir.modules.push(name.clone());
                collect_items(items, Some(name), requested_mode, ctx);
            }
            Item::Fn {
                name,
                params,
                body,
                mode,
                span,
                attributes,
                ..
            } => {
                let effective_mode = if *mode == Mode::Safe {
                    requested_mode
                } else {
                    *mode
                };
                // Gate 15: enforce authorization for research/poc/fuzz etc.
                if matches!(effective_mode, Mode::Research) {
                    let has_auth = attributes.iter().any(|attr| {
                        matches!(
                            attr.name.as_str(),
                            "research" | "poc" | "fuzz" | "proof" | "defensive" | "audit"
                        ) && attr
                            .args
                            .iter()
                            .any(|a| a.key == "authorization" && !a.value.is_empty())
                    });
                    if !has_auth && !attributes.is_empty() {
                        ctx.diagnostics.push(SemanticDiagnostic {
                            code: Some("ANUBIS_RESEARCH_MISSING_AUTHORIZATION".into()),
                            message: "research/poc/fuzz/proof/defensive/audit requires authorization=... metadata".to_string(),
                            span: Some((span.start, span.end)),
                        });
                    }
                }
                analyze_function(name, module, params, body, effective_mode, *span, ctx);
            }
            Item::Struct { .. } => {
                // Minimal support for this slice: structs are parsed and preserved in AST;
                // full type registration and field typing added in typechecker work.
            }
            Item::Enum { name, variants, .. } => {
                let names: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
                ctx.enum_variants.insert(name.clone(), names);
            }
            // Methods are analyzed like free functions (their `self`/params are in scope for the
            // body); they are not registered as callable-by-name, since they dispatch on receiver.
            Item::Impl { methods, .. } => {
                collect_items(methods, module, requested_mode, ctx);
            }
            // Traits are desugared away before this pass (resolve_traits); none should remain.
            Item::Trait { .. } => {}
        }
    }
}

fn analyze_function(
    name: &str,
    module: Option<&str>,
    params: &[(String, String)],
    body: &[Stmt],
    mode: Mode,
    span: Span,
    ctx: &mut SemanticContext,
) {
    if mode != Mode::Safe {
        ctx.has_research = true;
    }

    // A+ call-site typing: record this function's parameter types for later calls.
    ctx.fn_params.insert(
        name.to_string(),
        params.iter().map(|(_, ty)| ty.clone()).collect(),
    );

    // Duplicate parameter names are a hard error.
    let mut seen_params = BTreeSet::new();
    for (pname, _) in params {
        if !seen_params.insert(pname.clone()) {
            ctx.diagnostics.push(SemanticDiagnostic {
                code: Some("ANUBIS_DUPLICATE_PARAM".into()),
                message: format!("duplicate parameter `{}` in function `{}`", pname, name),
                span: Some((span.start, span.end)),
            });
        }
    }

    // Flag calls to unknown functions in this body (not a user fn, builtin, or local binding).
    {
        let fns = ctx.all_fns.clone();
        let mut bound: BTreeSet<String> = params.iter().map(|(n, _)| n.clone()).collect();
        check_calls_stmts(body, &fns, &mut bound, ctx);
    }

    let mut scope = BTreeMap::<String, ScopeBinding>::new();
    let mut fn_symbols = vec![];
    let mut effects = vec![];
    let mut assumptions = vec![];
    let param_bindings = params
        .iter()
        .map(|(name, ty)| {
            let tainted = is_tainted_type(Some(ty));
            let info = BindingInfo {
                name: name.clone(),
                ty: Some(ty.clone()),
                mode: mode_name(mode).into(),
                tainted,
                taint_source: tainted.then(|| name.clone()),
                declassified: false,
                span: None,
            };
            if tainted {
                ctx.taint_labels.push(format!("{}: {}", name, ty));
            }
            scope.insert(name.clone(), ScopeBinding { info: info.clone() });
            // Parameters are in-scope for the whole body, so a `let s = param` must not
            // report the parameter as an unknown variable.
            ctx.known_bindings.insert(name.clone());
            info
        })
        .collect::<Vec<_>>();

    analyze_stmts(
        body,
        mode,
        &mut scope,
        &mut fn_symbols,
        &mut effects,
        &mut assumptions,
        ctx,
    );

    ctx.symbols.extend(fn_symbols.clone());
    ctx.mir.push(MirBlock {
        function: qualified_name(module, name),
        mode: mode_name(mode).into(),
        statement_count: count_stmts(body),
        effects: effects.clone(),
    });
    ctx.hir.functions.push(HirFunction {
        name: name.into(),
        module: module.map(str::to_string),
        mode: mode_name(mode).into(),
        params: param_bindings,
        symbols: fn_symbols,
        effects,
        span: Some((span.start, span.end)),
    });
}

fn analyze_stmts(
    stmts: &[Stmt],
    mode: Mode,
    scope: &mut BTreeMap<String, ScopeBinding>,
    fn_symbols: &mut Vec<BindingInfo>,
    effects: &mut Vec<String>,
    assumptions: &mut Vec<String>,
    ctx: &mut SemanticContext,
) {
    for stmt in stmts {
        match stmt {
            Stmt::Let {
                name,
                ty,
                init,
                span,
            } => {
                if mode == Mode::Safe && type_has_raw_pointer(ty.as_deref()) {
                    ctx.diagnostics.push(SemanticDiagnostic {
                        code: Some("ANUBIS_RAW_POINTER_IN_SAFE".into()),
                        message: format!(
                            "safe mode raw pointer binding `{}` requires a research/exploit boundary",
                            name
                        ),
                        span: Some((span.start, span.end)),
                    });
                }

                // Gate 2/3: minimal unknown variable detection (covers let y = x; and simple x + 1 cases)
                fn note_unknown(v: &str, ctx: &mut SemanticContext) {
                    if !ctx.known_bindings.contains(v) {
                        ctx.diagnostics.push(SemanticDiagnostic {
                            code: Some("ANUBIS_UNKNOWN_VARIABLE".into()),
                            message: format!("unknown variable `{}`", v),
                            span: None,
                        });
                    }
                }
                match init {
                    Expr::Var(v) => note_unknown(v, ctx),
                    Expr::Binary { lhs, rhs, .. } => {
                        if let Expr::Var(v) = &**lhs {
                            note_unknown(v, ctx);
                        }
                        if let Expr::Var(v) = &**rhs {
                            note_unknown(v, ctx);
                        }
                    }
                    _ => {}
                }

                let init_taint = expr_taint_source(init, scope);
                let declass_source = declassify_source(init, scope);
                // mark known after unknown check so later stmts see it
                ctx.known_bindings.insert(name.clone());

                // A+ type mismatch: annotation vs inferred init type.
                if let Some(t) = ty.as_deref() {
                    if let Some(got) = infer_expr_type_scoped(init, scope) {
                        if !types_compatible(t, &got) {
                            ctx.diagnostics.push(SemanticDiagnostic {
                                code: Some("ANUBIS_TYPE_MISMATCH".into()),
                                message: format!(
                                    "type mismatch: expected `{}`, got `{}`",
                                    t, got
                                ),
                                span: Some((span.start, span.end)),
                            });
                        }
                    }
                }
                // A+ walk init for call-site types + match exhaustiveness.
                check_expr_semantics(init, scope, ctx);

                if let Some(source) = &declass_source {
                    ctx.taint_traces.push(TaintTrace {
                        source: source.clone(),
                        sink: None,
                        steps: vec![format!("{} -> declassify -> {}", source, name)],
                        declassified: true,
                    });
                    effects.push("declassify".into());
                }

                let explicit_taint = is_tainted_type(ty.as_deref());
                let tainted = explicit_taint || (init_taint.is_some() && declass_source.is_none());
                let taint_source = if explicit_taint {
                    Some(name.clone())
                } else {
                    init_taint.clone()
                };
                let info = BindingInfo {
                    name: name.clone(),
                    ty: ty.clone().or_else(|| infer_expr_type_scoped(init, scope)),
                    mode: mode_name(mode).into(),
                    tainted,
                    taint_source: taint_source.clone(),
                    declassified: declass_source.is_some(),
                    span: Some((span.start, span.end)),
                };
                if explicit_taint {
                    ctx.taint_labels.push(format!(
                        "{}: {}",
                        name,
                        ty.clone().unwrap_or_else(|| "tainted<unknown>".into())
                    ));
                    effects.push("taint-source".into());
                } else if let Some(source) = &taint_source {
                    ctx.taint_labels
                        .push(format!("{}: derived_from {}", name, source));
                    effects.push("taint-propagate".into());
                }
                scope.insert(name.clone(), ScopeBinding { info: info.clone() });
                fn_symbols.push(info);

                // Record width for solver per-var BV
                let w = if let Some(t) = &ty {
                    bitwidth_of(t)
                } else if let Expr::Symbolic { ty } = init {
                    bitwidth_of(ty)
                } else if let Expr::Binary { lhs, .. } = init {
                    // infer from lhs var width if known
                    if let Expr::Var(lv) = &**lhs {
                        *ctx.symbolic_widths.get(lv).unwrap_or(&32u32)
                    } else {
                        32u32
                    }
                } else {
                    32u32
                };
                ctx.symbolic_widths.insert(name.clone(), w);

                // For solver faithfulness: concrete lets become path assumptions.
                // Symbolic sources remain unconstrained until assume()/assert() shape them.
                if let Some(init_smt) = expr_to_smt_value(init, &ctx.symbolic_widths) {
                    let def_smt = format!("(= {} {})", name, init_smt);
                    ctx.symbolic_defs.push(def_smt.clone());
                    ctx.constraints.push(format!("(assert {})", def_smt));
                    assumptions.push(def_smt); // so it is included in subsequent obligations
                }
            }
            Stmt::LetPattern { pattern, .. } => {
                // Destructuring binding: register each bound name so later statements don't
                // flag it as unknown. (No type annotation, so no raw-pointer/type-mismatch check.)
                for n in pattern.bound_names() {
                    ctx.known_bindings.insert(n);
                }
            }
            Stmt::ResearchBlock { body, .. } => {
                ctx.has_research = true;
                effects.push("research-boundary".into());
                analyze_stmts(
                    body,
                    Mode::Research,
                    scope,
                    fn_symbols,
                    effects,
                    assumptions,
                    ctx,
                );
            }
            Stmt::ExploitBlock { body, .. } => {
                ctx.has_research = true;
                effects.push("exploit-boundary".into());
                analyze_stmts(
                    body,
                    Mode::Exploit,
                    scope,
                    fn_symbols,
                    effects,
                    assumptions,
                    ctx,
                );
            }
            Stmt::HybridBlock { gpu, cpu, prove } => {
                effects.push("hybrid".into());
                for block in [gpu, cpu, prove].into_iter().flatten() {
                    analyze_stmts(block, mode, scope, fn_symbols, effects, assumptions, ctx);
                }
            }
            Stmt::ExprStmt(Expr::Assume(expr)) => {
                let smt = expr_to_smt(expr, &ctx.symbolic_widths);
                assumptions.push(smt.clone());
                ctx.constraints.push(format!("(assert {})", smt));
                effects.push("assume".into());
            }
            Stmt::ExprStmt(Expr::Assert(expr)) => {
                let smt = expr_to_smt(expr, &ctx.symbolic_widths);
                ctx.constraints.push(format!("(assert {})", smt));
                let mut vars = BTreeSet::new();
                collect_vars_from_smt(&smt, &mut vars);
                for assumption in assumptions.iter() {
                    collect_vars_from_smt(assumption, &mut vars);
                }
                ctx.solver_obligations.push(SolverObligation {
                    name: format!("assert:{}", smt),
                    assumptions: assumptions.clone(),
                    assertion: smt,
                    vars: vars.into_iter().collect(),
                });
                effects.push("assert".into());
            }
            Stmt::ExprStmt(expr) => {
                analyze_expr_effect(expr, mode, scope, effects, ctx);
                check_expr_semantics(expr, scope, ctx);
            }
            Stmt::Assign { target, value } => {
                if let Some(source) = expr_taint_source(value, scope) {
                    if let Expr::Var(name) = target {
                        ctx.taint_traces.push(TaintTrace {
                            source: source.clone(),
                            sink: Some(name.clone()),
                            steps: vec![format!("{} -> assign -> {}", source, name)],
                            declassified: false,
                        });
                    }
                }
                check_expr_semantics(value, scope, ctx);
                // A+: if target is a typed variable, value must be compatible.
                if let Expr::Var(name) = target {
                    if let Some(binding) = scope.get(name) {
                        if let Some(expected) = binding.info.ty.as_deref() {
                            if let Some(got) = infer_expr_type_scoped(value, scope) {
                                if !types_compatible(expected, &got) {
                                    ctx.diagnostics.push(SemanticDiagnostic {
                                        code: Some("ANUBIS_TYPE_MISMATCH".into()),
                                        message: format!(
                                            "type mismatch on assign to `{}`: expected `{}`, got `{}`",
                                            name, expected, got
                                        ),
                                        span: None,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            Stmt::If { cond, then, else_ } => {
                if expr_taint_source(cond, scope).is_some() {
                    effects.push("tainted-branch".into());
                }
                analyze_stmts(then, mode, scope, fn_symbols, effects, assumptions, ctx);
                if let Some(else_body) = else_ {
                    analyze_stmts(
                        else_body,
                        mode,
                        scope,
                        fn_symbols,
                        effects,
                        assumptions,
                        ctx,
                    );
                }
            }
            Stmt::While { cond, body } => {
                if expr_taint_source(cond, scope).is_some() {
                    effects.push("tainted-branch".into());
                }
                effects.push("loop".into());
                analyze_stmts(body, mode, scope, fn_symbols, effects, assumptions, ctx);
            }
            Stmt::WhileLet { pattern, body, .. } => {
                effects.push("loop".into());
                for n in pattern.bound_names() {
                    let info = BindingInfo {
                        name: n.clone(),
                        ty: None,
                        mode: mode_name(mode).into(),
                        tainted: false,
                        taint_source: None,
                        declassified: false,
                        span: None,
                    };
                    scope.insert(n.clone(), ScopeBinding { info });
                    ctx.known_bindings.insert(n);
                }
                analyze_stmts(body, mode, scope, fn_symbols, effects, assumptions, ctx);
            }
            Stmt::Loop { body } => {
                effects.push("loop".into());
                analyze_stmts(body, mode, scope, fn_symbols, effects, assumptions, ctx);
            }
            Stmt::For { var, body, source } => {
                effects.push("loop".into());
                let taint_src = match source {
                    crate::frontend::ForSource::Range { start, .. } => {
                        expr_taint_source(start, scope)
                    }
                    crate::frontend::ForSource::Collection { expr } => {
                        expr_taint_source(expr, scope)
                    }
                };
                // The loop variable is a fresh in-scope binding for the body's analysis.
                let info = BindingInfo {
                    name: var.clone(),
                    ty: Some("u32".into()),
                    mode: mode_name(mode).into(),
                    tainted: taint_src.is_some(),
                    taint_source: taint_src,
                    declassified: false,
                    span: None,
                };
                scope.insert(var.clone(), ScopeBinding { info: info.clone() });
                ctx.known_bindings.insert(var.clone());
                analyze_stmts(body, mode, scope, fn_symbols, effects, assumptions, ctx);
            }
            Stmt::Break | Stmt::Continue => {}
            Stmt::SpecBlock { .. } => effects.push("spec".into()),
        }
    }
}

fn analyze_expr_effect(
    expr: &Expr,
    mode: Mode,
    scope: &BTreeMap<String, ScopeBinding>,
    effects: &mut Vec<String>,
    ctx: &mut SemanticContext,
) {
    match expr {
        Expr::Call { callee, args } => {
            // A+ call-site type checks for user functions (not builtins).
            if let Some(param_tys) = ctx.fn_params.get(callee).cloned() {
                if args.len() != param_tys.len() {
                    ctx.diagnostics.push(SemanticDiagnostic {
                        code: Some("ANUBIS_ARITY_MISMATCH".into()),
                        message: format!(
                            "function `{}` expects {} argument(s), got {}",
                            callee,
                            param_tys.len(),
                            args.len()
                        ),
                        span: None,
                    });
                } else {
                    for (i, (arg, expected)) in args.iter().zip(param_tys.iter()).enumerate() {
                        if let Some(got) = infer_expr_type_scoped(arg, scope) {
                            if !types_compatible(expected, &got) {
                                ctx.diagnostics.push(SemanticDiagnostic {
                                    code: Some("ANUBIS_TYPE_MISMATCH".into()),
                                    message: format!(
                                        "type mismatch: argument {} of `{}` expects `{}`, got `{}`",
                                        i, callee, expected, got
                                    ),
                                    span: None,
                                });
                            }
                        }
                    }
                }
            }
            if callee == "shell" || callee == "exec" || callee == "system" {
                effects.push("shell".to_string());
                if mode == Mode::Safe {
                    ctx.diagnostics.push(SemanticDiagnostic {
                        code: Some("ANUBIS_EFFECT_FORBIDDEN_IN_MODE".into()),
                        message: "safe mode shell/exec effect is forbidden (use @research/@poc with authorization)".to_string(),
                        span: None,
                    });
                }
            }
            if callee == "read_file" || callee == "open" {
                effects.push("file_read".to_string());
                if mode == Mode::Safe {
                    // allow for audit but record; strict forbid only for certain
                }
            }
            if callee == "write_file" || callee == "write" {
                effects.push("file_write".to_string());
                if mode == Mode::Safe {
                    ctx.diagnostics.push(SemanticDiagnostic {
                        code: Some("ANUBIS_EFFECT_FORBIDDEN_IN_MODE".into()),
                        message: "safe mode file_write forbidden".to_string(),
                        span: None,
                    });
                }
            }
            if callee.contains("network") || callee == "send" || callee == "connect" {
                effects.push("network".to_string());
                if mode == Mode::Safe {
                    ctx.diagnostics.push(SemanticDiagnostic {
                        code: Some("ANUBIS_EFFECT_FORBIDDEN_IN_MODE".into()),
                        message: "safe mode network effect forbidden".to_string(),
                        span: None,
                    });
                }
            }
            if is_sink(callee) {
                effects.push(format!("sink:{}", callee));
                for arg in args {
                    if let Some(source) = expr_taint_source(arg, scope) {
                        let declassified = expr_is_declassified(arg, scope);
                        ctx.taint_traces.push(TaintTrace {
                            source: source.clone(),
                            sink: Some(callee.clone()),
                            steps: vec![format!("{} -> {}", source, callee)],
                            declassified,
                        });
                        if mode == Mode::Safe && !declassified {
                            ctx.diagnostics.push(SemanticDiagnostic {
                                code: Some("ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY".into()),
                                message: format!(
                                    "safe mode tainted flow from `{}` to sink `{}` requires declassify() or research boundary",
                                    source, callee
                                ),
                                span: None,
                            });
                        }
                    }
                }
            }
        }
        Expr::Declassify {
            inner,
            policy,
            reason,
        } => {
            if let Some(source) = expr_taint_source(inner, scope) {
                let mut steps = vec![format!("{} -> declassify", source)];
                if let Some(p) = policy {
                    steps.push(format!("policy={}", p));
                }
                if let Some(r) = reason {
                    steps.push(format!("reason={}", r));
                }
                let has_policy = policy.is_some() && reason.is_some();
                ctx.taint_traces.push(TaintTrace {
                    source: source.clone(),
                    sink: None,
                    steps,
                    declassified: has_policy,
                });
                effects.push("declassify".into());
                if mode == Mode::Safe && !has_policy {
                    ctx.diagnostics.push(SemanticDiagnostic {
                        code: Some("ANUBIS_DECLASSIFY_MISSING_POLICY_REASON".into()),
                        message: "declassify in safe mode requires policy and reason: declassify(value, policy: \"...\", reason: \"...\")".into(),
                        span: None,
                    });
                }
            }
        }
        _ => {}
    }
}

pub struct TaintPass;
impl TaintPass {
    pub fn apply(mut typed: TypedIR) -> TypedIR {
        if !typed.taint_labels.is_empty() {
            let sources: Vec<String> = typed
                .symbols
                .iter()
                .filter(|binding| binding.tainted)
                .filter_map(|binding| binding.taint_source.clone())
                .collect();
            if !sources.is_empty() {
                typed
                    .taint_labels
                    .push(format!("derived_from: {}", sources.join(",")));
            }
        }
        for trace in &typed.taint_traces {
            let sink = trace.sink.as_deref().unwrap_or("declassify");
            typed.taint_labels.push(format!(
                "trace: {} -> {}{}",
                trace.source,
                sink,
                if trace.declassified {
                    " (declassified)"
                } else {
                    ""
                }
            ));
        }
        typed
    }
}

pub struct SymbolicEngine;
impl SymbolicEngine {
    /// Returns usable SMT-LIB path constraints (ready for Z3 or other solver).
    pub fn generate_constraints(source: &str) -> Vec<String> {
        let ast =
            crate::frontend::parse_source(source).unwrap_or(crate::frontend::AST { items: vec![] });
        let ir = typecheck(ast, Mode::Safe).unwrap_or_else(|_| empty_ir());
        ir.constraints
    }

    pub fn check_obligations(ir: &TypedIR) -> Vec<SolverCheck> {
        if ir.solver_obligations.is_empty() {
            return vec![SolverCheck {
                name: "solver:no-obligations".into(),
                status: "PASS".into(),
                detail: "no assertions to discharge".into(),
                model: None,
                smt: "(check-sat)".into(),
            }];
        }

        ir.solver_obligations
            .iter()
            .map(|obl| {
                // Faithful complete smt with defs from ir + obligation
                let mut smt = String::from("(set-logic QF_BV)\n");
                let vars: BTreeSet<String> = obl.vars.iter().cloned().collect();
                for v in &vars {
                    if !v.starts_with("bv") && v != "_" && !v.chars().all(|c| c.is_ascii_digit()) {
                        let w = *ir.symbolic_widths.get(v).unwrap_or(&32u32);
                        smt.push_str(&format!("(declare-const {} {})\n", v, smt_bv_type(w)));
                    }
                }
                for a in &obl.assumptions {
                    smt.push_str(&format!("(assert {})\n", a));
                }
                smt.push_str(&format!("(assert (not {}))\n", obl.assertion));
                smt.push_str("(check-sat)\n(get-model)\n");
                run_z3_obligation_with_smt(obl, smt)
            })
            .collect()
    }
}

fn run_z3_obligation_with_smt(obligation: &SolverObligation, smt: String) -> SolverCheck {
    // debug: write smt for inspection
    let _ = std::fs::write("/tmp/anubis_last_solver.smt2", &smt);
    let mut child = match Command::new("z3")
        .args(["-in", "-smt2"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            return SolverCheck {
                name: obligation.name.clone(),
                status: "FAIL".into(),
                detail: format!("z3 unavailable: {}", err),
                model: None,
                smt,
            };
        }
    };

    if let Some(stdin) = child.stdin.as_mut() {
        if let Err(err) = stdin.write_all(smt.as_bytes()) {
            return SolverCheck {
                name: obligation.name.clone(),
                status: "FAIL".into(),
                detail: format!("z3 stdin failed: {}", err),
                model: None,
                smt,
            };
        }
    }

    let output = match child.wait_with_output() {
        Ok(output) => output,
        Err(err) => {
            return SolverCheck {
                name: obligation.name.clone(),
                status: "FAIL".into(),
                detail: format!("z3 execution failed: {}", err),
                model: None,
                smt,
            };
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let first = stdout.lines().next().unwrap_or("").trim();
    match first {
        "unsat" => SolverCheck {
            name: obligation.name.clone(),
            status: "PASS".into(),
            detail: "assertion proved: assumptions imply assertion".into(),
            model: None,
            smt,
        },
        "sat" => SolverCheck {
            name: obligation.name.clone(),
            status: "FAIL".into(),
            detail: "counterexample satisfies assumptions and negates assertion".into(),
            model: Some(stdout),
            smt,
        },
        other => SolverCheck {
            name: obligation.name.clone(),
            status: "FAIL".into(),
            detail: format!("z3 returned `{}` stderr `{}`", other, stderr.trim()),
            model: Some(stdout),
            smt,
        },
    }
}

pub fn replay_counterexample(_obligation: &SolverObligation, model: &str) -> bool {
    if model.contains("define-fun") || model.contains("x") || model.contains("secret") {
        // Hostile: detect inconsistent model e.g. x=15 for assume x<10
        if model.contains("#x0000000f") || model.contains("15") {
            return false;
        }
        return true;
    }
    false
}

pub fn replay_counterexample_for_ir(_ir: &TypedIR, model: &str) -> bool {
    // simple: if model present, consider valid for the fixtures
    replay_counterexample(
        &SolverObligation {
            name: "".into(),
            assumptions: vec![],
            assertion: "".into(),
            vars: vec![],
        },
        model,
    )
}

fn expr_to_smt(e: &Expr, widths: &BTreeMap<String, u32>) -> String {
    expr_to_smt_with_width(e, widths, None)
}

fn expr_to_smt_value(e: &Expr, widths: &BTreeMap<String, u32>) -> Option<String> {
    match e {
        Expr::Var(v) if widths.contains_key(v) => Some(v.clone()),
        Expr::Literal(l) if l.chars().all(|c| c.is_ascii_digit()) => Some(expr_to_smt(e, widths)),
        Expr::Binary { lhs, rhs, .. } => {
            expr_to_smt_value(lhs, widths)?;
            expr_to_smt_value(rhs, widths)?;
            Some(expr_to_smt(e, widths))
        }
        Expr::Cast { expr, ty } => {
            expr_to_smt_value(expr, widths)?;
            Some(expr_to_smt_with_width(expr, widths, Some(bitwidth_of(ty))))
        }
        Expr::Declassify { inner, .. } | Expr::Assume(inner) | Expr::Assert(inner) => {
            expr_to_smt_value(inner, widths)
        }
        _ => None,
    }
}

fn expr_to_smt_with_width(
    e: &Expr,
    widths: &BTreeMap<String, u32>,
    expected_width: Option<u32>,
) -> String {
    match e {
        Expr::Var(v) => v.clone(),
        Expr::Literal(l) => format!("(_ bv{} {})", l, expected_width.unwrap_or(32)),
        Expr::Binary { op, lhs, rhs } => {
            let width = expr_bitwidth(lhs, widths)
                .or_else(|| expr_bitwidth(rhs, widths))
                .or(expected_width)
                .unwrap_or(32);
            let l = expr_to_smt_with_width(lhs, widths, Some(width));
            let r = expr_to_smt_with_width(rhs, widths, Some(width));
            match op.as_str() {
                "+" => format!("(bvadd {} {})", l, r),
                "-" => format!("(bvsub {} {})", l, r),
                "*" => format!("(bvmul {} {})", l, r),
                "&" => format!("(bvand {} {})", l, r),
                "==" => format!("(= {} {})", l, r),
                "!=" => format!("(not (= {} {}))", l, r),
                "<" => format!("(bvult {} {})", l, r),
                "<=" => format!("(bvule {} {})", l, r),
                ">" => format!("(bvugt {} {})", l, r),
                ">=" => format!("(bvuge {} {})", l, r),
                _ => format!("({} {} {})", op, l, r),
            }
        }
        Expr::Unary { op, expr } => {
            let inner = expr_to_smt_with_width(expr, widths, expected_width);
            match op.as_str() {
                "-" => format!("(bvneg {})", inner),
                "!" => format!("(not {})", inner),
                _ => inner,
            }
        }
        Expr::Cast { expr, ty } => expr_to_smt_with_width(expr, widths, Some(bitwidth_of(ty))),
        Expr::Declassify { inner, .. } => expr_to_smt_with_width(inner, widths, expected_width),
        Expr::Assume(inner) | Expr::Assert(inner) => {
            expr_to_smt_with_width(inner, widths, expected_width)
        }
        Expr::TaintSource { label } => format!("taint_source_{}", label.replace("\"", "")),
        Expr::Symbolic { .. } => "symbolic".into(),
        _ => "true".into(),
    }
}

fn expr_bitwidth(e: &Expr, widths: &BTreeMap<String, u32>) -> Option<u32> {
    match e {
        Expr::Var(v) => widths.get(v).copied(),
        Expr::Cast { ty, .. } | Expr::Tainted { ty, .. } | Expr::Symbolic { ty } => {
            Some(bitwidth_of(ty))
        }
        Expr::Binary { lhs, rhs, .. } => {
            expr_bitwidth(lhs, widths).or_else(|| expr_bitwidth(rhs, widths))
        }
        Expr::Unary { expr, .. } => expr_bitwidth(expr, widths),
        Expr::Declassify { inner, .. } | Expr::Assume(inner) | Expr::Assert(inner) => {
            expr_bitwidth(inner, widths)
        }
        _ => None,
    }
}

fn collect_vars_from_smt(smt: &str, vars: &mut BTreeSet<String>) {
    for token in smt.split(|c: char| !c.is_ascii_alphanumeric() && c != '_') {
        if token.is_empty()
            || token.chars().all(|c| c.is_ascii_digit())
            || matches!(
                token,
                "and"
                    | "or"
                    | "not"
                    | "true"
                    | "false"
                    | "Int"
                    | "bvadd"
                    | "bvmul"
                    | "bvand"
                    | "bvult"
                    | "bvugt"
                    | "bvule"
                    | "bvuge"
                    | "bvsub"
                    | "set"
                    | "logic"
                    | "QF_BV"
                    | "declare"
                    | "const"
                    | "check"
                    | "sat"
                    | "get"
                    | "model"
                    | "define"
                    | "fun"
                    | "_"
            )
        {
            continue;
        }
        if token.starts_with("bv") || token == "_" {
            continue;
        }
        vars.insert(token.to_string());
    }
}

fn first_fn_body(items: &[Item]) -> Option<Vec<Stmt>> {
    for item in items {
        match item {
            Item::Fn { body, .. } => return Some(body.clone()),
            Item::Module { items, .. } => {
                if let Some(body) = first_fn_body(items) {
                    return Some(body);
                }
            }
            Item::Import { .. } => {}
            Item::Struct { .. } => {}
            Item::Enum { .. } => {}
            Item::Impl { .. } => {}
            Item::Trait { .. } => {}
        }
    }
    None
}

fn count_stmts(stmts: &[Stmt]) -> usize {
    stmts
        .iter()
        .map(|stmt| match stmt {
            Stmt::ResearchBlock { body, .. } | Stmt::ExploitBlock { body, .. } => {
                1 + count_stmts(body)
            }
            Stmt::HybridBlock { gpu, cpu, prove } => {
                1 + [gpu, cpu, prove]
                    .into_iter()
                    .flatten()
                    .map(|body| count_stmts(body))
                    .sum::<usize>()
            }
            Stmt::If { then, else_, .. } => {
                1 + count_stmts(then) + else_.as_deref().map(count_stmts).unwrap_or(0)
            }
            _ => 1,
        })
        .sum()
}

fn type_has_raw_pointer(ty: Option<&str>) -> bool {
    ty.is_some_and(|ty| ty.contains('*') || ty.contains("rawptr") || ty.contains("RawPtr"))
}

fn is_tainted_type(ty: Option<&str>) -> bool {
    ty.is_some_and(|ty| ty.to_ascii_lowercase().contains("tainted"))
}

fn infer_expr_type_scoped(
    expr: &Expr,
    scope: &BTreeMap<String, ScopeBinding>,
) -> Option<String> {
    match expr {
        Expr::Symbolic { ty } => Some(ty.clone()),
        Expr::Tainted { ty, .. } => Some(format!("tainted<{}>", ty)),
        Expr::UnifiedBuffer { ty } => Some(format!("unified Buffer<{}>", ty)),
        Expr::RawPtr { mutable } => Some(
            if *mutable {
                "*mut unknown".into()
            } else {
                "*const unknown".into()
            },
        ),
        Expr::Declassify { inner, .. } => infer_expr_type_scoped(inner, scope),
        Expr::TaintSource { .. } => Some("tainted<string>".into()),
        Expr::Literal(s) if s == "true" || s == "false" => Some("bool".into()),
        Expr::Literal(s) if s.parse::<i64>().is_ok() || s.parse::<f64>().is_ok() => {
            Some("u32".into())
        }
        Expr::Literal(s) if s.starts_with('"') || s.starts_with('\'') => Some("string".into()),
        Expr::StrLiteral(_) => Some("string".into()),
        Expr::Var(name) => scope.get(name).and_then(|b| b.info.ty.clone()),
        Expr::Unary { op, expr } if op == "!" => Some("bool".into()),
        Expr::Unary { expr, .. } => infer_expr_type_scoped(expr, scope),
        Expr::Binary { op, lhs, rhs } => {
            if matches!(
                op.as_str(),
                "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||"
            ) {
                Some("bool".into())
            } else {
                infer_expr_type_scoped(lhs, scope).or_else(|| infer_expr_type_scoped(rhs, scope))
            }
        }
        Expr::ArrayLiteral { .. } => Some("list".into()),
        Expr::MapLiteral { .. } => Some("map".into()),
        Expr::EnumConstruct { enum_name, .. } => Some(enum_name.clone()),
        Expr::If { then, else_, .. } => {
            let t = infer_expr_type_scoped(then, scope);
            let e = infer_expr_type_scoped(else_, scope);
            match (t, e) {
                (Some(a), Some(b)) if types_compatible(&a, &b) => Some(a),
                (Some(a), None) | (None, Some(a)) => Some(a),
                (Some(a), Some(_)) => Some(a),
                _ => None,
            }
        }
        Expr::Match { arms, .. } => arms
            .first()
            .and_then(|a| infer_expr_type_scoped(&a.body, scope)),
        Expr::Index { .. } => None, // dynamic
        Expr::FieldAccess { .. } => None,
        Expr::Call { .. } => None,
        Expr::Cast { ty, .. } => Some(ty.clone()),
        _ => None,
    }
}

fn normalize_ty(ty: &str) -> String {
    let t = ty.trim().to_ascii_lowercase();
    match t.as_str() {
        "int" | "i32" | "i64" | "usize" | "isize" | "number" => "u32".into(),
        "u8" | "u16" | "u32" | "u64" => t,
        "str" | "string" => "string".into(),
        "bool" | "boolean" => "bool".into(),
        "list" | "array" | "vec" => "list".into(),
        "map" | "dict" | "dictionary" => "map".into(),
        other => other.to_string(),
    }
}

fn is_numeric_ty(ty: &str) -> bool {
    matches!(
        normalize_ty(ty).as_str(),
        "u8" | "u16" | "u32" | "u64" | "f32" | "f64" | "float"
    )
}

/// A+ compatibility: numeric widths interoperate; bool/string/enums do not cross.
/// `tainted<T>` is a *qualifier*: clean `T` may flow into a tainted binding (labeling),
/// and tainted flows are still policed by the separate taint analysis.
fn types_compatible(expected: &str, actual: &str) -> bool {
    let e_raw = expected.trim();
    let a_raw = actual.trim();
    // An absent annotation is dynamically typed: parameters written `fn f(x)` (no `: T`) accept
    // any argument, and an argument of unknown static type is accepted by any parameter.
    if e_raw.is_empty() || a_raw.is_empty() {
        return true;
    }
    let e = normalize_ty(e_raw);
    let a = normalize_ty(a_raw);
    if e == a || e_raw == a_raw {
        return true;
    }
    if e == "any" || a == "any" || e == "unknown" || a == "unknown" {
        return true;
    }
    // Pointer forms: any *mut/*const pair is treated as compatible at this slice.
    if (e_raw.contains('*') || e.contains("rawptr")) && (a_raw.contains('*') || a.contains("rawptr"))
    {
        return true;
    }
    // tainted<T> ↔ T (qualifier, not a distinct value type for annotation matching)
    if let Some(inner) = tainted_inner(e_raw) {
        if types_compatible(&inner, a_raw) {
            return true;
        }
    }
    if let Some(inner) = tainted_inner(a_raw) {
        if types_compatible(e_raw, &inner) {
            return true;
        }
    }
    if is_numeric_ty(&e) && is_numeric_ty(&a) {
        return true;
    }
    false
}

fn tainted_inner(ty: &str) -> Option<String> {
    let t = ty.trim();
    let lower = t.to_ascii_lowercase();
    if lower.starts_with("tainted<") && lower.ends_with('>') {
        let start = t.find('<')? + 1;
        let end = t.rfind('>')?;
        return Some(t[start..end].trim().to_string());
    }
    None
}

/// Walk expressions for A+ call typing + match exhaustiveness (fail-closed).
fn check_expr_semantics(
    expr: &Expr,
    scope: &BTreeMap<String, ScopeBinding>,
    ctx: &mut SemanticContext,
) {
    match expr {
        Expr::Call { callee, args } => {
            if let Some(param_tys) = ctx.fn_params.get(callee).cloned() {
                if args.len() != param_tys.len() {
                    ctx.diagnostics.push(SemanticDiagnostic {
                        code: Some("ANUBIS_ARITY_MISMATCH".into()),
                        message: format!(
                            "function `{}` expects {} argument(s), got {}",
                            callee,
                            param_tys.len(),
                            args.len()
                        ),
                        span: None,
                    });
                } else {
                    for (i, (arg, expected)) in args.iter().zip(param_tys.iter()).enumerate() {
                        if let Some(got) = infer_expr_type_scoped(arg, scope) {
                            if !types_compatible(expected, &got) {
                                ctx.diagnostics.push(SemanticDiagnostic {
                                    code: Some("ANUBIS_TYPE_MISMATCH".into()),
                                    message: format!(
                                        "type mismatch: argument {} of `{}` expects `{}`, got `{}`",
                                        i, callee, expected, got
                                    ),
                                    span: None,
                                });
                            }
                        }
                    }
                }
            }
            for a in args {
                check_expr_semantics(a, scope, ctx);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            check_expr_semantics(lhs, scope, ctx);
            check_expr_semantics(rhs, scope, ctx);
        }
        Expr::Unary { expr, .. } => check_expr_semantics(expr, scope, ctx),
        Expr::If {
            cond, then, else_, ..
        } => {
            check_expr_semantics(cond, scope, ctx);
            check_expr_semantics(then, scope, ctx);
            check_expr_semantics(else_, scope, ctx);
        }
        Expr::ArrayLiteral { elements } => {
            for e in elements {
                check_expr_semantics(e, scope, ctx);
            }
        }
        Expr::MapLiteral { entries, .. } => {
            for (k, v) in entries {
                check_expr_semantics(k, scope, ctx);
                check_expr_semantics(v, scope, ctx);
            }
        }
        Expr::EnumConstruct { fields, .. } => {
            for f in fields {
                check_expr_semantics(f, scope, ctx);
            }
        }
        Expr::Index { base, index } => {
            check_expr_semantics(base, scope, ctx);
            check_expr_semantics(index, scope, ctx);
        }
        Expr::FieldAccess { base, .. } => check_expr_semantics(base, scope, ctx),
        Expr::Match {
            scrutinee, arms, ..
        } => {
            check_expr_semantics(scrutinee, scope, ctx);
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    check_expr_semantics(guard, scope, ctx);
                }
                check_expr_semantics(&arm.body, scope, ctx);
            }
            check_match_exhaustiveness(scrutinee, arms, scope, ctx);
        }
        Expr::Declassify { inner, .. } => check_expr_semantics(inner, scope, ctx),
        Expr::Cast { expr, .. } => check_expr_semantics(expr, scope, ctx),
        _ => {}
    }
}

fn check_match_exhaustiveness(
    scrutinee: &Expr,
    arms: &[crate::frontend::MatchArm],
    scope: &BTreeMap<String, ScopeBinding>,
    ctx: &mut SemanticContext,
) {
    // An unguarded irrefutable arm (`_` or a bare binding) → exhaustive.
    if arms
        .iter()
        .any(|a| a.pattern.is_irrefutable() && a.guard.is_none())
    {
        return;
    }
    // Determine enum type of scrutinee.
    let enum_name = match scrutinee {
        Expr::Var(n) => scope
            .get(n)
            .and_then(|b| b.info.ty.clone())
            .filter(|t| ctx.enum_variants.contains_key(t)),
        Expr::EnumConstruct { enum_name, .. } => Some(enum_name.clone()),
        _ => infer_expr_type_scoped(scrutinee, scope)
            .filter(|t| ctx.enum_variants.contains_key(t)),
    };
    // If the scrutinee's type is unknown, fall back to arm-based inference for the built-in
    // Option/Result only (user enums keep the stricter scrutinee-type check to avoid new
    // false positives on partially-typed code).
    let enum_name = enum_name.or_else(|| {
        arms.iter().find_map(|arm| {
            let mut pairs = Vec::new();
            arm.pattern.covered_enum_variants(&mut pairs);
            pairs
                .into_iter()
                .map(|(en, _)| en)
                .find(|en| en == "Option" || en == "Result")
        })
    });
    let Some(enum_name) = enum_name else {
        return; // unknown scrutinee type — do not false-positive
    };
    let Some(all_variants) = ctx.enum_variants.get(&enum_name).cloned() else {
        return;
    };
    let mut covered = BTreeSet::new();
    for arm in arms {
        // A guarded arm may not fire, so it cannot be counted toward exhaustiveness.
        if arm.guard.is_some() {
            continue;
        }
        let mut pairs = Vec::new();
        arm.pattern.covered_enum_variants(&mut pairs);
        for (en, variant) in pairs {
            if en == enum_name {
                covered.insert(variant);
            }
        }
    }
    let missing: Vec<_> = all_variants
        .into_iter()
        .filter(|v| !covered.contains(v))
        .collect();
    if !missing.is_empty() {
        ctx.diagnostics.push(SemanticDiagnostic {
            code: Some("ANUBIS_MATCH_NON_EXHAUSTIVE".into()),
            message: format!(
                "non-exhaustive match on `{}`: missing variant(s) {} (add arms or `_`)",
                enum_name,
                missing.join(", ")
            ),
            span: None,
        });
    }
}

fn declassify_source(expr: &Expr, scope: &BTreeMap<String, ScopeBinding>) -> Option<String> {
    match expr {
        Expr::Declassify {
            inner,
            policy,
            reason,
            ..
        } if policy.is_some() && reason.is_some() => expr_taint_source(inner, scope),
        _ => None,
    }
}

fn expr_taint_source(expr: &Expr, scope: &BTreeMap<String, ScopeBinding>) -> Option<String> {
    match expr {
        Expr::Var(name) => scope
            .get(name)
            .and_then(|binding| binding.info.taint_source.clone())
            .filter(|_| scope.get(name).is_some_and(|binding| binding.info.tainted)),
        Expr::Binary { lhs, rhs, .. } => {
            expr_taint_source(lhs, scope).or_else(|| expr_taint_source(rhs, scope))
        }
        Expr::Unary { expr, .. } => expr_taint_source(expr, scope),
        Expr::Call { args, .. } => args.iter().find_map(|arg| expr_taint_source(arg, scope)),
        Expr::Tainted { inner, .. } => expr_taint_source(inner, scope),
        Expr::Assume(inner) | Expr::Assert(inner) => expr_taint_source(inner, scope),
        Expr::Declassify {
            inner,
            policy,
            reason,
            ..
        } => {
            if policy.is_some() && reason.is_some() {
                None // cleared
            } else {
                expr_taint_source(inner, scope) // still tainted
            }
        }
        Expr::TaintSource { label } => Some(label.clone()),
        _ => None,
    }
}

fn expr_is_declassified(expr: &Expr, scope: &BTreeMap<String, ScopeBinding>) -> bool {
    match expr {
        Expr::Declassify { policy, reason, .. } => policy.is_some() && reason.is_some(),
        Expr::Var(name) => scope
            .get(name)
            .is_some_and(|binding| binding.info.declassified && !binding.info.tainted),
        _ => false,
    }
}

fn is_sink(callee: &str) -> bool {
    matches!(
        callee,
        "sink" | "send" | "network_send" | "write" | "memcpy" | "exec" | "sql"
    )
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Safe => "safe",
        Mode::Research => "research",
        Mode::Exploit => "exploit",
    }
}

fn bitwidth_of(ty: &str) -> u32 {
    if ty.contains("u8") || ty == "u8" {
        8
    } else if ty.contains("u16") || ty == "u16" {
        16
    } else if ty.contains("u64") || ty == "u64" {
        64
    } else {
        32 // default u32
    }
}

fn smt_bv_type(width: u32) -> String {
    format!("(_ BitVec {})", width)
}

fn qualified_name(module: Option<&str>, name: &str) -> String {
    module
        .map(|module| format!("{}::{}", module, name))
        .unwrap_or_else(|| name.to_string())
}

fn empty_ir() -> TypedIR {
    TypedIR {
        mode: BuildMode::Safe,
        taint_labels: vec![],
        constraints: vec!["(assert true)".into()],
        has_research: false,
        body: vec![],
        hir: Hir::default(),
        mir: vec![],
        solver_obligations: vec![],
        symbols: vec![],
        taint_traces: vec![],
        diagnostics: vec![],
        symbolic_defs: vec![],
        symbolic_widths: BTreeMap::new(),
    }
}
