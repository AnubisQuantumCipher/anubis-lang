//! Middle: typed HIR, mode/effect checks, taint tracking, and Z3 obligations.

use crate::frontend::{Expr, Item, Mode, Span, Stmt, AST};
use crate::BuildMode;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::process::{Command, Stdio};

pub(crate) mod ty;

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
    /// Arity of the value when it is a closure / first-class function bound here (a lambda literal or
    /// a named-function reference); `None` when unknown. Used to arity-check direct closure calls.
    closure_arity: Option<usize>,
}

/// Arity of an initializer if it is a closure or first-class function reference, else `None`.
/// Conservative: only a lambda literal, a named-function reference, or an alias of a known-arity
/// closure yields an arity — anything else is unknown and left unchecked (no false positives).
fn closure_arity_of(
    init: &Expr,
    scope: &BTreeMap<String, ScopeBinding>,
    ctx: &SemanticContext,
) -> Option<usize> {
    match init {
        Expr::Lambda { params, .. } => Some(params.len()),
        Expr::Var(n) => ctx
            .fn_params
            .get(n)
            .map(|p| p.len())
            .or_else(|| scope.get(n).and_then(|b| b.closure_arity)),
        _ => None,
    }
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
    /// Variables that are genuinely modelable as bit-vectors for the solver (a `symbolic()` source
    /// or an integer-arithmetic let over already-modelable vars). Distinct from `symbolic_widths`,
    /// which records a width for EVERY let — including string/bool/list bindings — and so cannot be
    /// used to decide whether an assertion is soundly modelable in QF_BV.
    solver_int_vars: BTreeSet<String>,
    /// Variables given an EXPLICIT `: T` annotation. Only these have their reassignments
    /// type-checked (the user opted into the type); an inferred binding is dynamic and reassignable
    /// to any type, so enforcing type-stability on it would be a false positive.
    annotated_vars: BTreeSet<String>,
    known_bindings: BTreeSet<String>,
    /// Enum name → variant names (for match exhaustiveness).
    enum_variants: BTreeMap<String, Vec<String>>,
    /// Function name → ordered parameter types (for call-site type checks).
    fn_params: BTreeMap<String, Vec<String>>,
    /// Function name → declared return type (`-> T`, empty if omitted). A call-result binding may be
    /// modeled as a solver integer ONLY when this is an integer type — otherwise a float-returning
    /// callee (`frac -> f64`) would seed a float into the integer domain via composition.
    fn_ret_types: BTreeMap<String, String>,
    /// Function name → (parameter names, `requires` clauses, `ensures` clauses). Registered in
    /// pass 1 so a caller can, at a call site, ASSERT the callee's precondition and ASSUME its
    /// postcondition — the composition that makes contracts chain.
    #[allow(clippy::type_complexity)]
    fn_contracts: BTreeMap<String, (Vec<String>, Vec<Expr>, Vec<Expr>)>,
    /// Every user-defined function name (flat namespace; used for duplicate + unknown-call checks).
    all_fns: BTreeSet<String>,
    /// Interprocedural taint summary: functions whose RETURN value carries INTERNAL taint (from a
    /// `taint_source()`/`tainted<T>` local, or a return of another such function), computed by a
    /// monotone fixpoint pre-pass before per-function analysis. `expr_taint_source`'s `Call` arm
    /// consults it so `sink(get_secret())` is flagged even with no tainted argument. Monotone (only
    /// grows), so no control-flow-merge hazard — the return-value over-approximation is the safe
    /// direction for a security check.
    tainting_fns: BTreeSet<String>,
    /// Interprocedural param→sink summary (Phase-3 A1): for each function, the set of formal
    /// parameter indices that can flow to a sink (builtin `is_sink`, or a call argument position
    /// that another function's summary marks as sinking) without declassify. Monotone fixpoint.
    /// Call sites consult it: `log(tainted)` is `ANUBIS_INTERPROC_SINK` when `fn log(x){sink(x);}`.
    param_sinks: BTreeMap<String, BTreeSet<usize>>,
    /// Interprocedural param→return summary (Phase-3 A2): for each function, the set of formal
    /// parameter indices that can flow to the return value without declassify. Monotone fixpoint.
    /// Combined at call sites with argument taint: `wrap` with `returns_taint_of_params={0}` makes
    /// `wrap(tainted)` a taint source (even through further let/return chains).
    param_return_taint: BTreeMap<String, BTreeSet<usize>>,
    /// Method name → parameter count (including `self`). `None` marks a name defined with more than
    /// one arity across impls, so its direct-call arity is ambiguous and left unchecked.
    method_arities: BTreeMap<String, Option<usize>>,
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
    // Pass 1.5: interprocedural taint summaries (return-taint + param→sink), computed before
    // per-function analysis so every `Call` the analysis sees can consult them.
    compute_tainting_fns(&ast.items, &mut ctx);
    compute_param_sinks(&ast.items, &mut ctx);
    compute_param_return_taint(&ast.items, &mut ctx);
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
                name,
                params,
                span,
                requires,
                ensures,
                ret,
                ..
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
                ctx.fn_ret_types
                    .insert(name.clone(), ret.clone().unwrap_or_default());
                if !requires.is_empty() || !ensures.is_empty() {
                    ctx.fn_contracts.insert(
                        name.clone(),
                        (
                            params.iter().map(|(n, _)| n.clone()).collect(),
                            requires.clone(),
                            ensures.clone(),
                        ),
                    );
                }
            }
            // Collect method arities (including `self`) so direct method calls can be arity-checked;
            // a name defined with differing arities across impls is marked ambiguous (None).
            Item::Impl { methods, .. } => {
                for m in methods {
                    if let Item::Fn { name, params, .. } = m {
                        let arity = params.len();
                        ctx.method_arities
                            .entry(name.clone())
                            .and_modify(|e| {
                                if *e != Some(arity) {
                                    *e = None;
                                }
                            })
                            .or_insert(Some(arity));
                    }
                }
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
            Stmt::While { cond, body, .. } => {
                check_calls_expr(cond, fns, bound, ctx);
                let mut b = bound.clone();
                check_calls_stmts(body, fns, &mut b, ctx);
            }
            Stmt::WhileLet {
                pattern,
                expr,
                body,
            } => {
                check_calls_expr(expr, fns, bound, ctx);
                let mut b = bound.clone();
                for n in pattern.bound_names() {
                    b.insert(n);
                }
                check_calls_stmts(body, fns, &mut b, ctx);
            }
            Stmt::Loop { body, .. } => {
                let mut b = bound.clone();
                check_calls_stmts(body, fns, &mut b, ctx);
            }
            Stmt::For {
                var, source, body, ..
            } => {
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
        Expr::EnumConstruct {
            enum_name,
            variant,
            fields,
            ..
        } => {
            // Fail-closed: `Foo::Bar` must name a declared enum and a real variant. An unknown
            // enum name is either a typo or a Rust-style qualified call (`math::double(...)`) —
            // the call namespace is flat, so neither is valid. Without this check both silently
            // lower to a stringy enum value at runtime instead of trapping.
            match ctx.enum_variants.get(enum_name).cloned() {
                None => ctx.diagnostics.push(SemanticDiagnostic {
                    code: Some("ANUBIS_UNKNOWN_ENUM".into()),
                    message: format!(
                        "`{enum_name}::{variant}` refers to unknown type `{enum_name}` \
                         (declare `enum {enum_name}`, or call `{variant}(...)` directly — \
                         the call namespace is flat, there are no `::`-qualified calls)"
                    ),
                    span: None,
                }),
                Some(variants) if !variants.contains(variant) => {
                    ctx.diagnostics.push(SemanticDiagnostic {
                        code: Some("ANUBIS_UNKNOWN_VARIANT".into()),
                        message: format!(
                            "enum `{enum_name}` has no variant `{variant}` (known: {})",
                            variants.join(", ")
                        ),
                        span: None,
                    })
                }
                _ => {}
            }
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
            pattern,
            scrutinee,
            then,
            else_,
            ..
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
                ret,
                requires,
                ensures,
                effects: declared_effects,
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
                analyze_function(
                    name,
                    module,
                    params,
                    body,
                    ret.as_deref(),
                    requires,
                    ensures,
                    declared_effects,
                    effective_mode,
                    *span,
                    false,
                    ctx,
                );
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
            // body) but flagged `is_method`, so they are not registered as callable-by-name —
            // they dispatch on the receiver and must not shadow a same-named builtin.
            Item::Impl { methods, .. } => {
                for m in methods {
                    if let Item::Fn {
                        name,
                        params,
                        body,
                        mode,
                        span,
                        ret,
                        requires,
                        ensures,
                        effects: declared_effects,
                        ..
                    } = m
                    {
                        let effective_mode = if *mode == Mode::Safe {
                            requested_mode
                        } else {
                            *mode
                        };
                        analyze_function(
                            name,
                            module,
                            params,
                            body,
                            ret.as_deref(),
                            requires,
                            ensures,
                            declared_effects,
                            effective_mode,
                            *span,
                            true,
                            ctx,
                        );
                    }
                }
            }
            // Traits are desugared away before this pass (resolve_traits); none should remain.
            Item::Trait { .. } => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn analyze_function(
    name: &str,
    module: Option<&str>,
    params: &[(String, String)],
    body: &[Stmt],
    ret: Option<&str>,
    requires: &[Expr],
    ensures: &[Expr],
    declared_effects: &[String],
    mode: Mode,
    span: Span,
    is_method: bool,
    ctx: &mut SemanticContext,
) {
    if mode != Mode::Safe {
        ctx.has_research = true;
    }

    // Solver integer-modelability and symbolic widths are FUNCTION-LOCAL: a variable modeled as an
    // i64 in one function must not leak that modelability to a same-named binding in another function
    // (which could hold a string/list/bool), or an integer predicate over the second would be "proved"
    // against the first's model. Reset per function. (Obligations/constraints accumulate globally.)
    ctx.solver_int_vars.clear();
    ctx.symbolic_widths.clear();

    // A declared `-> T` return type is checked against any return value that is a LITERAL of an
    // unambiguously incompatible type (a bare string/number/bool/list/map/enum). Non-literal
    // returns (variables, calls, if/match, a trailing statement that yields 0) are left unchecked
    // — the type is dynamic — so this catches `fn f() -> u32 { "s" }` with zero false positives.
    if let Some(rty) = ret {
        let pscope: BTreeMap<String, ScopeBinding> = params
            .iter()
            .map(|(n, t)| {
                (
                    n.clone(),
                    ScopeBinding {
                        info: BindingInfo {
                            name: n.clone(),
                            ty: Some(t.clone()),
                            mode: String::new(),
                            tainted: false,
                            taint_source: None,
                            declassified: false,
                            span: None,
                        },
                        closure_arity: None,
                    },
                )
            })
            .collect();
        check_return_types(body, rty, &pscope, span, ctx);
    }

    // The `?` operator unwraps `Some`/`Ok` and early-returns `None`/`Err`, so it only makes sense in
    // a function that returns `Option`/`Result`. If a function declares a CONCRETE non-Option/Result
    // return type and uses `?`, it can only fail closed at runtime (`ANUBIS_TRY_ON_NON_OPTION_RESULT`)
    // — reject it statically. A function with no declared return type is dynamic, and a generic or
    // opaque (`any`/`unknown`) return is left alone, so no working dynamic program is newly rejected.
    if let Some(rty) = ret {
        let r = rty.trim();
        let norm = normalize_ty(r);
        let result_like = r.starts_with("Option") || r.starts_with("Result");
        let opaque = norm == "any" || norm == "unknown";
        if !r.is_empty() && !result_like && !opaque && !ty::is_generic(r) && body_contains_try(body)
        {
            ctx.diagnostics.push(SemanticDiagnostic {
                code: Some("ANUBIS_TRY_OUTSIDE_RESULT".into()),
                message: format!(
                    "`{name}` uses the `?` operator but declares `-> {r}`; `?` requires the function to return `Option` or `Result`"
                ),
                span: Some((span.start, span.end)),
            });
        }
    }

    // A+ call-site typing: record this function's parameter types for later calls. Methods are
    // NOT recorded — they are only reachable via `recv.m(...)`, never a bare call, so recording
    // them would shadow a same-named stdlib builtin at free call sites.
    if !is_method {
        ctx.fn_params.insert(
            name.to_string(),
            params.iter().map(|(_, ty)| ty.clone()).collect(),
        );
    }

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
            scope.insert(
                name.clone(),
                ScopeBinding {
                    info: info.clone(),
                    closure_arity: None,
                },
            );
            // Parameters are in-scope for the whole body, so a `let s = param` must not
            // report the parameter as an unknown variable.
            ctx.known_bindings.insert(name.clone());
            info
        })
        .collect::<Vec<_>>();

    // B2 contracts: make integer parameters solver-modelable, assume each `requires` precondition,
    // then (after the body) assert each `ensures` postcondition at the tail return. The body plus
    // the precondition must PROVE the postcondition — discharged by the (now-sound i64) solver.
    // Only functions that DECLARE a contract model their parameters symbolically, so a plain
    // function's assertions keep their prior (param-opaque) semantics — no regression.
    let has_contract = !requires.is_empty() || !ensures.is_empty();
    if has_contract {
        // Make integer parameters solver-modelable. NOTE: a `u32`/`u8` annotation is INERT at
        // runtime (a parameter holds any i64; the call boundary applies no width clamp), so we must
        // NOT assume it lies in [0, 2^w-1] — doing so let the solver "prove" `x + 1 > x` while
        // `f(i64::MAX)` wraps and violates it. A contract that needs bounds must state them via
        // `requires`; unbounded i64 arithmetic that can overflow is (correctly) not provable.
        for (pname, pty) in params {
            // Only INTEGER params are solver-modelable. A float param must NOT be modeled as an i64
            // bit-vector (that "proved" `2*x != 1` for `x = 0.5`); an integer `ensures` that then
            // references it becomes non-modelable and fails closed below.
            if is_integer_ty(pty) {
                ctx.solver_int_vars.insert(pname.clone());
                ctx.symbolic_widths.insert(pname.clone(), 64);
            }
        }
        for req in requires {
            if is_bool_modelable(req, &ctx.solver_int_vars) {
                assumptions.push(expr_to_smt(req, &ctx.symbolic_widths));
            }
        }
        // Mark parameters that a `requires` guard proves non-zero AS DIVISORS, so `x / v` / `x % v`
        // become modelable. Sound only if the guarantee holds at the division: require the parameter
        // to be a modeled integer AND never reassigned or shadowed in the body (else the entry guard
        // need not hold later). Because such a variable is stable, the mark never needs removal.
        let mut rebound = BTreeSet::new();
        collect_assigned_roots(body, &mut rebound);
        collect_let_bound(body, &mut rebound);
        for req in requires {
            if let Some(v) = requires_nonzero_var(req) {
                if ctx.solver_int_vars.contains(&v) && !rebound.contains(&v) {
                    ctx.solver_int_vars.insert(nzdiv_mark(&v));
                }
            }
        }
    }
    // The precondition (parameter ranges + `requires`) dominates EVERY return; the body assumptions
    // added below (lets, composition) only dominate the tail return.
    let precondition_assumptions = assumptions.clone();

    analyze_stmts(
        body,
        mode,
        &mut scope,
        &mut fn_symbols,
        &mut effects,
        &mut assumptions,
        ctx,
    );

    // Phase-3 C2: declared-vs-inferred effect check. When a function has a `uses(...)` clause,
    // every capability effect inferred from the body must be ⊆ the declared set. Missing
    // declaration of a used capability → `ANUBIS_UNDECLARED_EFFECT`. Absent `uses` skips the
    // check (C5 verified mode will require declarations). Internal analysis tags (taint-*,
    // assume, assert, loop, declassify, …) are not capability effects and are not gated here.
    if !declared_effects.is_empty() {
        let declared: BTreeSet<String> = declared_effects
            .iter()
            .map(|e| normalize_effect_name(e))
            .collect();
        let mut seen_undeclared = BTreeSet::new();
        for inf in &effects {
            if let Some(cap) = capability_effect(inf) {
                if !declared.contains(&cap) && seen_undeclared.insert(cap.clone()) {
                    ctx.diagnostics.push(SemanticDiagnostic {
                        code: Some("ANUBIS_UNDECLARED_EFFECT".into()),
                        message: format!(
                            "function `{name}` uses effect `{cap}` but does not declare it in `uses(...)` (declared: {})",
                            declared_effects.join(", ")
                        ),
                        span: Some((span.start, span.end)),
                    });
                }
            }
        }
    }

    // Discharge each `ensures` at EVERY return, so no return path can violate the postcondition:
    //   - the TAIL return is verified under the full body assumptions (they all dominate it);
    //   - each EARLY/nested return is verified under the precondition alone (a sound subset — this
    //     catches an unconditionally-violating early return like `return 0` vs `ensures(result>0)`,
    //     and can only ever mis-DISPROVE a path-dependent return, never mis-prove one).
    // Modeling is best-effort: a postcondition the solver cannot express (strings/lists/division) is
    // left un-obligated rather than mis-disproved.
    if !ensures.is_empty() {
        // A parameter named in an `ensures` denotes the CALL-ENTRY value — composition substitutes the
        // caller's original argument into the callee's `ensures`. Anubis has no `old()`, so if the body
        // REASSIGNS or SHADOWS such a parameter, its `ensures` would be discharged against the mutated
        // value while the caller assumes the entry value — a false certification laundered through
        // composition (`ensures(result == x) { x = 9; return x; }`). Fail closed.
        let param_names: BTreeSet<String> = params.iter().map(|(n, _)| n.clone()).collect();
        let mut ensures_vars = BTreeSet::new();
        for e in ensures {
            collect_expr_vars(e, &mut ensures_vars);
        }
        let mut rebound = BTreeSet::new();
        collect_assigned_roots(body, &mut rebound);
        collect_let_bound(body, &mut rebound);
        for p in ensures_vars.intersection(&param_names) {
            if rebound.contains(p) {
                ctx.diagnostics.push(SemanticDiagnostic {
                    code: Some("ANUBIS_CONTRACT_UNPROVABLE".into()),
                    message: format!(
                        "cannot verify a postcondition over parameter `{p}`: it is reassigned or \
                         shadowed in the body, but `ensures` refers to the parameter's call-entry \
                         value (there is no `old()`). Keep the parameter unmodified and return a \
                         local instead (`let r = ...; return r;`)"
                    ),
                    span: Some((span.start, span.end)),
                });
            }
        }
        // Every value the body can yield at its tail (a bare tail `if`/`match`'s arms, a block tail,
        // or `0` when it falls off the end) is checked under the full body assumptions.
        let mut tail_vals = Vec::new();
        tail_values(body, true, &mut tail_vals);
        for tv in &tail_vals {
            push_ensures_obligations(ctx, ensures, tv, &assumptions, span);
        }
        // Every explicit return except the tail return-call (the last statement).
        let n = body.len();
        let mut early = Vec::new();
        for (i, s) in body.iter().enumerate() {
            let is_tail_ret = i + 1 == n
                && matches!(s, Stmt::ExprStmt(Expr::Call { callee, .. }) if callee == "return");
            if !is_tail_ret {
                collect_returns_in_stmt(s, &mut early);
            }
        }
        for r in &early {
            push_ensures_obligations(ctx, ensures, r, &precondition_assumptions, span);
        }
    }

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

/// Restore the lexical binding scope after analyzing a block (`if`/`else`/loop body/etc.).
///
/// A block-scoped `let` (including a name that shadows an outer binding) must not leak past the
/// block: `let x = 5; if c { let x = taint(); } sink(x);` must see the OUTER clean `x`, not the
/// inner tainted one. Mirrors the snapshot/restore that `body_returns_taint` already does for the
/// interprocedural summary. This ONLY rewrites the `scope` map (BindingInfo / closure_arity); it
/// does NOT touch solver `assumptions` or `solver_int_vars` — those have their own snapshot path
/// via `drop_written_after_scope` / `havoc_loop_written` and must stay undisturbed here.
fn restore_block_scope(
    scope: &mut BTreeMap<String, ScopeBinding>,
    saved: &BTreeMap<String, ScopeBinding>,
) {
    *scope = saved.clone();
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

                // Unknown-variable detection (covers `let y = x;` and simple `x + 1` cases). A bare
                // name is only unknown if it is neither a local binding, a user-defined function,
                // nor a stdlib builtin — named functions and builtins are first-class values and may
                // be bound by name (`let f = double;`), mirroring the unknown-*call* check below.
                fn note_unknown(v: &str, ctx: &mut SemanticContext) {
                    if !ctx.known_bindings.contains(v)
                        && !ctx.all_fns.contains(v)
                        && !crate::backends::run::is_builtin_name(v)
                    {
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

                let init_taint =
                    expr_taint_source(init, scope, &ctx.tainting_fns, &ctx.param_return_taint);
                let declass_source =
                    declassify_source(init, scope, &ctx.tainting_fns, &ctx.param_return_taint);
                // Effect inference must see calls in let-initializers (`let d = read_file(p)`),
                // not only bare expression statements — otherwise uses(...) checks miss real I/O.
                analyze_expr_effect(init, mode, scope, effects, ctx);
                // mark known after unknown check so later stmts see it
                ctx.known_bindings.insert(name.clone());

                if ty.is_some() {
                    ctx.annotated_vars.insert(name.clone());
                }
                // A+ type mismatch: annotation vs inferred init type.
                if let Some(t) = ty.as_deref() {
                    if let Some(got) = infer_expr_type_scoped(init, scope) {
                        if !types_assignable(t, &got) {
                            ctx.diagnostics.push(SemanticDiagnostic {
                                code: Some("ANUBIS_TYPE_MISMATCH".into()),
                                message: format!("type mismatch: expected `{}`, got `{}`", t, got),
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
                let ca = closure_arity_of(init, scope, ctx);
                scope.insert(
                    name.clone(),
                    ScopeBinding {
                        info: info.clone(),
                        closure_arity: ca,
                    },
                );
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
                // A `let` that SHADOWS an existing binding invalidates the old one's solver state:
                // drop its modelability and any stale fact, so an integer predicate over the NEW
                // binding (which may hold a string/list/bool) is not "proved" against the shadowed
                // integer's model (e.g. `let v = 0; let v = "hi"; assert(v + 0 == v)`).
                ctx.solver_int_vars.remove(name);
                {
                    let mangled = smt_var(name);
                    assumptions.retain(|a| {
                        let mut vs = BTreeSet::new();
                        collect_vars_from_smt(a, &mut vs);
                        !vs.contains(&mangled)
                    });
                }
                ctx.symbolic_widths.insert(name.clone(), w);

                // Track whether this binding is genuinely integer-modelable for the solver: a
                // `symbolic()` source, or an integer-arithmetic init over already-modelable vars.
                // String/bool/list lets are excluded, so an assertion over them is never
                // (unsoundly) "disproved" by a fabricated bit-vector counterexample.
                if matches!(init, Expr::Symbolic { .. })
                    || is_int_modelable(init, &ctx.solver_int_vars)
                {
                    ctx.solver_int_vars.insert(name.clone());
                }

                // For solver faithfulness: concrete lets become path assumptions.
                // Symbolic sources remain unconstrained until assume()/assert() shape them.
                if let Some(init_smt) = expr_to_smt_value(init, &ctx.symbolic_widths) {
                    let def_smt = format!("(= {} {})", smt_var(name), init_smt);
                    ctx.symbolic_defs.push(def_smt.clone());
                    ctx.constraints.push(format!("(assert {})", def_smt));
                    assumptions.push(def_smt); // so it is included in subsequent obligations
                }
                // NOTE: a symbolic input's `u8`/`u32` type annotation is NOT turned into a
                // [0, 2^w-1] range assumption — the annotation is runtime-inert, so assuming a range
                // the runtime does not enforce would be unsound (it would let the solver "prove"
                // overflow-free facts that the i64 runtime violates). The value is modeled as an
                // unconstrained i64.

                // B2 composition: when the initializer calls a CONTRACTED function, specialize the
                // callee's contract to this call — ASSERT its precondition (the caller must satisfy
                // it) and ASSUME its postcondition with `result` bound to this variable, so a later
                // assertion can rely on it. This is how one function's `ensures` satisfies the next.
                if let Expr::Call { callee, args } = init {
                    if let Some((pnames, creq, cens)) = ctx.fn_contracts.get(callee).cloned() {
                        if pnames.len() == args.len() {
                            let mut sub: BTreeMap<String, Expr> =
                                pnames.iter().cloned().zip(args.iter().cloned()).collect();
                            // ASSERT each precondition; note whether ALL were checkable.
                            let mut all_requires_checkable = true;
                            for req in &creq {
                                let concrete = substitute_vars(req, &sub);
                                if is_bool_modelable(&concrete, &ctx.solver_int_vars) {
                                    let smt = expr_to_smt(&concrete, &ctx.symbolic_widths);
                                    let mut vars = BTreeSet::new();
                                    collect_vars_from_smt(&smt, &mut vars);
                                    for a in assumptions.iter() {
                                        collect_vars_from_smt(a, &mut vars);
                                    }
                                    ctx.solver_obligations.push(SolverObligation {
                                        name: format!("requires@{callee}:{smt}"),
                                        assumptions: assumptions.clone(),
                                        assertion: smt,
                                        vars: vars.into_iter().collect(),
                                    });
                                } else {
                                    all_requires_checkable = false;
                                }
                            }
                            // ASSUME the postcondition ONLY when every precondition was verifiable:
                            // the ensures holds only under the precondition, so assuming it when a
                            // `requires` was SKIPPED (a dynamic/unmodelable argument) would be an
                            // unsound false proof (the caller could be violating the precondition).
                            // Model this binding as a solver integer ONLY if the callee DECLARES an
                            // integer return type. Return types are inert at runtime, so a `-> u32`
                            // body is separately runtime-guarded (anubis_require_int_ret); but a
                            // `-> f64` callee must NOT seed a float into the integer domain here (its
                            // `ensures` may not even mention `result`, leaving the binding unconstrained
                            // yet modeled as i64 — a certified-false cast/bitwise identity at runtime).
                            let callee_returns_int = ctx
                                .fn_ret_types
                                .get(callee)
                                .map(|t| is_integer_ty(t))
                                .unwrap_or(false);
                            if !cens.is_empty() && all_requires_checkable && callee_returns_int {
                                // The callee guarantees an integer postcondition about its result,
                                // so this binding is solver-modelable.
                                ctx.solver_int_vars.insert(name.clone());
                                sub.insert("result".to_string(), Expr::Var(name.clone()));
                                for ens in &cens {
                                    let concrete = substitute_vars(ens, &sub);
                                    if is_bool_modelable(&concrete, &ctx.solver_int_vars) {
                                        let smt = expr_to_smt(&concrete, &ctx.symbolic_widths);
                                        ctx.constraints.push(format!("(assert {})", smt));
                                        assumptions.push(smt);
                                    }
                                }
                            }
                        }
                    }
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
                // Lexical block: a `let` inside `@research { ... }` must not escape.
                let snap_scope = scope.clone();
                analyze_stmts(
                    body,
                    Mode::Research,
                    scope,
                    fn_symbols,
                    effects,
                    assumptions,
                    ctx,
                );
                restore_block_scope(scope, &snap_scope);
            }
            Stmt::ExploitBlock { body, .. } => {
                ctx.has_research = true;
                effects.push("exploit-boundary".into());
                let snap_scope = scope.clone();
                analyze_stmts(
                    body,
                    Mode::Exploit,
                    scope,
                    fn_symbols,
                    effects,
                    assumptions,
                    ctx,
                );
                restore_block_scope(scope, &snap_scope);
            }
            Stmt::HybridBlock { gpu, cpu, prove } => {
                effects.push("hybrid".into());
                for block in [gpu, cpu, prove].into_iter().flatten() {
                    let snap_scope = scope.clone();
                    analyze_stmts(block, mode, scope, fn_symbols, effects, assumptions, ctx);
                    restore_block_scope(scope, &snap_scope);
                }
            }
            Stmt::ExprStmt(Expr::Assume(expr)) => {
                // Only ASSUME what the solver can model SOUNDLY (mirrors the assert handler below). An
                // unmodelable assumption — e.g. `assume((x as u8) == 0)`, whose truncating cast has no
                // sound i64 identity — would otherwise be lowered as if `x == 0` and let the solver
                // certify a violated contract (`ensures(result == 0)` while f(256) returns 256). An
                // unmodelable assume is still enforced at runtime (anubis_assume), just not trusted here.
                if is_bool_modelable(expr, &ctx.solver_int_vars) {
                    let smt = expr_to_smt(expr, &ctx.symbolic_widths);
                    assumptions.push(smt.clone());
                    ctx.constraints.push(format!("(assert {})", smt));
                }
                effects.push("assume".into());
            }
            Stmt::ExprStmt(Expr::Assert(expr)) => {
                // Only discharge an assertion the solver can soundly model in QF_BV (a boolean
                // formula over integer-modelable terms). A bare bool var, a string comparison, or
                // any other value is left to the runtime `assert` — the checker must not fabricate
                // a bit-vector counterexample and "disprove" a statement it cannot faithfully model
                // (that would make `check` unsound, e.g. disproving `assert(true)`).
                if is_bool_modelable(expr, &ctx.solver_int_vars) {
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
                }
                effects.push("assert".into());
            }
            Stmt::ExprStmt(expr) => {
                analyze_expr_effect(expr, mode, scope, effects, ctx);
                check_expr_semantics(expr, scope, ctx);
            }
            Stmt::Assign { target, value } => {
                analyze_expr_effect(value, mode, scope, effects, ctx);
                if let Some(source) =
                    expr_taint_source(value, scope, &ctx.tainting_fns, &ctx.param_return_taint)
                {
                    if let Expr::Var(name) = target {
                        ctx.taint_traces.push(TaintTrace {
                            source: source.clone(),
                            sink: Some(name.clone()),
                            steps: vec![format!("{} -> assign -> {}", source, name)],
                            declassified: false,
                        });
                    }
                }
                // A reassigned binding can no longer be modeled from its initial `let` value: the
                // solver does straight-line analysis and cannot follow a loop/branch update, so its
                // concrete-let assumption goes stale. Drop it from the modelable set AND remove any
                // stale fact about it from the assumptions — an assertion over it is then left to the
                // runtime instead of being (unsoundly) "disproved" against its pre-assignment value
                // (e.g. `for i in 1..5 { total = total + i } assert(total == 10)` must not be refuted
                // with the stale `total == 0`). Removing the stale fact — not just dropping
                // modelability — is essential: a loop invariant later RE-MODELS the variable, and a
                // surviving `x == <old>` would then launder a false invariant/postcondition.
                if let Some(root) = assign_target_root(target) {
                    ctx.solver_int_vars.remove(root);
                    let mangled = smt_var(root);
                    assumptions.retain(|a| {
                        let mut vs = BTreeSet::new();
                        collect_vars_from_smt(a, &mut vs);
                        !vs.contains(&mangled)
                    });
                    // Re-establish a fresh fact when the new value is modelable and does NOT reference
                    // the reassigned variable itself — a constant or an expression over OTHER modelable
                    // variables (an `x`-referencing RHS is now unmodelable, since `x` was just removed
                    // from the modelable set, so `x = x + 1` correctly adds nothing). This keeps the
                    // common `i = 0;` reset before a counted loop provable without introducing a
                    // self-referential false fact.
                    if matches!(target, Expr::Var(_))
                        && is_int_modelable(value, &ctx.solver_int_vars)
                    {
                        let smt = expr_to_smt(value, &ctx.symbolic_widths);
                        let def = format!("(= {} {})", mangled, smt);
                        ctx.solver_int_vars.insert(root.to_string());
                        ctx.symbolic_widths.entry(root.to_string()).or_insert(64);
                        ctx.constraints.push(format!("(assert {})", def));
                        assumptions.push(def);
                    }
                }
                check_expr_semantics(value, scope, ctx);
                // Reassignment changes what a closure-valued binding holds: recompute its arity
                // (or clear it) so a later direct call checks the current value, not a stale one.
                if let Expr::Var(name) = target {
                    if scope.contains_key(name) {
                        let ca = closure_arity_of(value, scope, ctx);
                        if let Some(b) = scope.get_mut(name) {
                            b.closure_arity = ca;
                        }
                    }
                }
                // A+: reassignment type-checking. Only an EXPLICITLY-annotated variable is held to
                // its declared type (a `let mut acc = 0` with an INFERRED type is dynamic and may be
                // reassigned to any type — enforcing stability there was a false positive). For an
                // inferred variable, update its tracked type to the new value's type (or clear it
                // when dynamic) so later uses see the current type rather than a stale one.
                if let Expr::Var(name) = target {
                    let got = infer_expr_type_scoped(value, scope);
                    if ctx.annotated_vars.contains(name) {
                        if let (Some(expected), Some(got)) = (
                            scope.get(name).and_then(|b| b.info.ty.clone()),
                            got.as_ref(),
                        ) {
                            if !types_assignable(&expected, got) {
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
                    } else if let Some(b) = scope.get_mut(name) {
                        b.info.ty = got; // flow-sensitive: track the reassigned type (None if dynamic)
                    }
                }
            }
            Stmt::If { cond, then, else_ } => {
                if expr_taint_source(cond, scope, &ctx.tainting_fns, &ctx.param_return_taint)
                    .is_some()
                {
                    effects.push("tainted-branch".into());
                }
                // A branch may not execute, so a fact it asserts (e.g. `x = 5`) must not leak out as
                // unconditional. Analyze each branch under the pre-`if` assumptions (the branches are
                // ALTERNATIVES — reset between them so `then`'s facts don't leak into `else`), then
                // discard the branch facts and drop every variable either branch conditionally writes.
                //
                // Taint scope is snapshotted/restored the same way `body_returns_taint` does: a
                // block-scoped `let` (incl. shadowing) must not escape the branch. Without this,
                // `let x=5; if c { let x=taint(); } sink(x);` was a false-positive reject — the
                // outer clean `x` was overwritten by the inner tainted binding. Solver assumptions
                // stay on their own snapshot path below; this only restores BindingInfo scope.
                let snapshot = assumptions.clone();
                let snap_scope = scope.clone();
                analyze_stmts(then, mode, scope, fn_symbols, effects, assumptions, ctx);
                if let Some(else_body) = else_ {
                    *assumptions = snapshot.clone();
                    restore_block_scope(scope, &snap_scope);
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
                restore_block_scope(scope, &snap_scope);
                let else_slice: &[Stmt] = else_.as_deref().unwrap_or(&[]);
                drop_written_after_scope(ctx, assumptions, snapshot, &[then, else_slice]);
            }
            Stmt::While {
                cond,
                body,
                invariant,
            } => {
                if expr_taint_source(cond, scope, &ctx.tainting_fns, &ctx.param_return_taint)
                    .is_some()
                {
                    effects.push("tainted-branch".into());
                }
                effects.push("loop".into());
                // B3: verify loop invariants (base case + preservation) BEFORE the body drops the
                // loop-carried variables, so the base case sees their pre-loop state.
                let admit = if invariant.is_empty() {
                    None
                } else {
                    verify_while_invariants(ctx, cond, invariant, body, assumptions)
                };
                // Snapshot the pre-loop assumptions. A loop can run ZERO times, so no fact its body
                // accumulates survives it: after analysis we restore this snapshot and drop every
                // written variable. Havoc the written variables first so an in-body `assert` is not
                // discharged against a stale pre-loop value the loop mutates each iteration.
                // Same for taint scope: a loop-body `let` is block-scoped and must not escape.
                let snapshot = assumptions.clone();
                let snap_scope = scope.clone();
                havoc_loop_written(ctx, assumptions, body);
                analyze_stmts(body, mode, scope, fn_symbols, effects, assumptions, ctx);
                restore_block_scope(scope, &snap_scope);
                drop_written_after_scope(ctx, assumptions, snapshot, &[body]);
                if let Some((post, _written, readmit)) = admit {
                    // A VERIFIED invariant DOES hold after the loop: re-model the tracked variables
                    // (constrained by the proved invariants ∧ ¬cond) so a later `ensures`/`assert` can
                    // rely on them.
                    for v in &readmit {
                        ctx.solver_int_vars.insert(v.clone());
                        ctx.symbolic_widths.entry(v.clone()).or_insert(64);
                    }
                    for a in post {
                        ctx.constraints.push(format!("(assert {})", a));
                        assumptions.push(a);
                    }
                }
            }
            Stmt::WhileLet { pattern, body, .. } => {
                effects.push("loop".into());
                // Snapshot BEFORE inserting pattern bindings so they do not leak past the loop.
                let snap_scope = scope.clone();
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
                    scope.insert(
                        n.clone(),
                        ScopeBinding {
                            info,
                            closure_arity: None,
                        },
                    );
                    ctx.known_bindings.insert(n);
                }
                let snapshot = assumptions.clone();
                havoc_loop_written(ctx, assumptions, body);
                analyze_stmts(body, mode, scope, fn_symbols, effects, assumptions, ctx);
                restore_block_scope(scope, &snap_scope);
                drop_written_after_scope(ctx, assumptions, snapshot, &[body]);
            }
            Stmt::Loop { body, invariant } => {
                effects.push("loop".into());
                if !invariant.is_empty() {
                    ctx.diagnostics.push(SemanticDiagnostic {
                        code: Some("ANUBIS_LOOP_INVARIANT_UNVERIFIABLE".into()),
                        message: "an unbounded `loop` has no exit condition to assume, so an \
                             invariant cannot be discharged inductively — use a `while` loop with an \
                             explicit condition and invariant instead"
                            .into(),
                        span: None,
                    });
                }
                let snapshot = assumptions.clone();
                let snap_scope = scope.clone();
                havoc_loop_written(ctx, assumptions, body);
                analyze_stmts(body, mode, scope, fn_symbols, effects, assumptions, ctx);
                restore_block_scope(scope, &snap_scope);
                drop_written_after_scope(ctx, assumptions, snapshot, &[body]);
            }
            Stmt::For {
                var,
                body,
                source,
                invariant,
            } => {
                effects.push("loop".into());
                if !invariant.is_empty() {
                    ctx.diagnostics.push(SemanticDiagnostic {
                        code: Some("ANUBIS_LOOP_INVARIANT_UNVERIFIABLE".into()),
                        message: "loop invariants are currently verified on `while` loops only; \
                             rewrite this `for` as a `while` with an explicit counter to attach an \
                             invariant (a green check must not silently ignore an invariant)"
                            .into(),
                        span: None,
                    });
                }
                let taint_src = match source {
                    crate::frontend::ForSource::Range { start, .. } => {
                        expr_taint_source(start, scope, &ctx.tainting_fns, &ctx.param_return_taint)
                    }
                    crate::frontend::ForSource::Collection { expr } => {
                        expr_taint_source(expr, scope, &ctx.tainting_fns, &ctx.param_return_taint)
                    }
                };
                // The loop variable is a fresh in-scope binding for the body's analysis. A range
                // loop (`for i in a..b`) binds a number; a collection loop (`for x in xs`) binds an
                // element whose type is dynamic (unknown) — typing it `u32` was a heuristic that
                // mis-flagged `for x in xs { x[0] }` as "indexing a number".
                // Snapshot BEFORE inserting the loop var so it (and any body `let`) do not escape.
                let snap_scope = scope.clone();
                let var_ty = match source {
                    crate::frontend::ForSource::Range { .. } => Some("u32".into()),
                    crate::frontend::ForSource::Collection { .. } => None,
                };
                let info = BindingInfo {
                    name: var.clone(),
                    ty: var_ty,
                    mode: mode_name(mode).into(),
                    tainted: taint_src.is_some(),
                    taint_source: taint_src,
                    declassified: false,
                    span: None,
                };
                scope.insert(
                    var.clone(),
                    ScopeBinding {
                        info: info.clone(),
                        closure_arity: None,
                    },
                );
                ctx.known_bindings.insert(var.clone());
                let snapshot = assumptions.clone();
                havoc_loop_written(ctx, assumptions, body);
                analyze_stmts(body, mode, scope, fn_symbols, effects, assumptions, ctx);
                restore_block_scope(scope, &snap_scope);
                drop_written_after_scope(ctx, assumptions, snapshot, &[body]);
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
                            if !types_assignable(expected, &got) {
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
                    // file_read is allowed in safe when declared via uses(fs.read) (C2); record only.
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
            if matches!(callee.as_str(), "time" | "time_now" | "now") {
                effects.push("time".to_string());
            }
            if matches!(callee.as_str(), "rand" | "rand_gen" | "random") {
                effects.push("rand".to_string());
            }
            if is_sink(callee) {
                effects.push(format!("sink:{}", callee));
                for arg in args {
                    if let Some(source) =
                        expr_taint_source(arg, scope, &ctx.tainting_fns, &ctx.param_return_taint)
                    {
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
            // Phase-3 A1: interprocedural param→sink. A callee whose formal N reaches a sink
            // makes the call site a sink for argument N — even though the actual `sink(...)` is
            // inside the callee. Distinct code from the direct-sink check so callers can see
            // `ANUBIS_INTERPROC_SINK` (the leak is at the call boundary, not a local sink name).
            if let Some(sink_params) = ctx.param_sinks.get(callee).cloned() {
                for i in sink_params {
                    if let Some(arg) = args.get(i) {
                        if let Some(source) = expr_taint_source(
                            arg,
                            scope,
                            &ctx.tainting_fns,
                            &ctx.param_return_taint,
                        ) {
                            let declassified = expr_is_declassified(arg, scope);
                            ctx.taint_traces.push(TaintTrace {
                                source: source.clone(),
                                sink: Some(format!("{}(param {})", callee, i)),
                                steps: vec![format!(
                                    "{} -> call `{}` param {} -> sink",
                                    source, callee, i
                                )],
                                declassified,
                            });
                            if mode == Mode::Safe && !declassified {
                                ctx.diagnostics.push(SemanticDiagnostic {
                                    code: Some("ANUBIS_INTERPROC_SINK".into()),
                                    message: format!(
                                        "safe mode tainted flow from `{}` into parameter {} of `{}`, which reaches a sink without declassify",
                                        source, i, callee
                                    ),
                                    span: None,
                                });
                            }
                        }
                    }
                }
            }
            // Nested calls in arguments also produce effects (`return read_file(p)`, `sink(read_file(p))`).
            for arg in args {
                analyze_expr_effect(arg, mode, scope, effects, ctx);
            }
        }
        Expr::Declassify {
            inner,
            policy,
            reason,
        } => {
            if let Some(source) =
                expr_taint_source(inner, scope, &ctx.tainting_fns, &ctx.param_return_taint)
            {
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
                        // Every integer variable is a 64-bit bit-vector: the runtime is i64 and
                        // type-annotation widths are inert, so a narrower declaration would be an
                        // unsound abstraction. (A contract that needs a bound must state it via
                        // `requires`; parameter type widths carry no range assumption.)
                        smt.push_str(&format!("(declare-const {} {})\n", v, smt_bv_type(64)));
                    }
                }
                for a in &obl.assumptions {
                    smt.push_str(&format!("(assert {})\n", a));
                }
                smt.push_str(&format!("(assert (not {}))\n", obl.assertion));
                smt.push_str("(check-sat)\n(get-model)\n");
                let mut check = run_z3_obligation_with_smt(obl, smt);
                // Vacuity guard for CONTRACT obligations: `A ⟹ P` is proved by `A ∧ ¬P` UNSAT, but
                // that is also UNSAT when the assumptions `A` are self-contradictory — a VACUOUS
                // "proof". A precondition + `assume` that cannot both hold (e.g. `requires(x < 100)`
                // with `assume(x > 1000)`) would otherwise certify any postcondition while the code
                // runs and violates it. If a passing contract obligation has contradictory
                // assumptions, fail closed.
                // The loop-invariant BASE case uses the pre-loop assumptions; a contradictory
                // pre-loop state (a false `assume`/`requires` about a loop-carried variable) would
                // otherwise let the base pass vacuously, the bogus invariant be assumed after the
                // loop, and a false postcondition be certified. (The preservation STEP is NOT
                // vacuity-checked: a loop whose invariant implies ¬cond legitimately never iterates.)
                let is_contract = obl.name.starts_with("ensures:")
                    || obl.name.starts_with("requires@")
                    || obl.name.starts_with("loop-invariant-base:")
                    || obl.name.starts_with("assert:");
                if check.status == "PASS" && is_contract && !obl.assumptions.is_empty() {
                    if let Some(false) = assumptions_satisfiable(obl) {
                        check.status = "FAIL".into();
                        check.detail = "vacuous proof: the contract's assumptions are \
                             self-contradictory (unsatisfiable), so the postcondition is not really \
                             established — check for a `requires`/`assume` that cannot hold"
                            .into();
                    }
                }
                // A contract obligation the solver could not DECIDE (z3 `unknown`, e.g. a per-query
                // timeout on a hard symbolic division/remainder — see `Z3_ARGS`) is NOT proven. The
                // proof-carrying gate fails closed on it rather than accept an unverified postcondition.
                // It was not disproved (no counterexample), only undecided within budget — say so, and
                // clear any model. This branch became reachable once queries got a time budget.
                if check.status == "UNKNOWN" && is_contract {
                    check.status = "FAIL".into();
                    check.detail = "solver could not decide this contract within its time budget (z3 \
                         returned `unknown`, typically a hard symbolic division/remainder); failing \
                         closed — an undecided postcondition is not a proof. Restate it as a simpler \
                         or better-bounded obligation"
                        .into();
                    check.model = None;
                }
                check
            })
            .collect()
    }
}

/// Whether a contract obligation's assumptions are jointly satisfiable. `Some(true)`/`Some(false)`
/// from z3; `None` if the solver did not cleanly decide (in which case the caller keeps the original
/// verdict rather than fabricating a vacuity failure).
/// z3 CLI args for every obligation query. `-t` is a per-check SOFT timeout (ms) and `-T` a HARD
/// wall-clock backstop (s): a query z3 cannot decide in budget returns `unknown` (or the process is
/// killed and yields empty output) instead of hanging the checker indefinitely. Bit-blasting a
/// symbolic `bvsdiv`/`bvsrem` over two free 64-bit operands can otherwise blow up unpredictably.
/// Both timeout outcomes are handled FAIL-CLOSED downstream (UNKNOWN / None — never a proof).
const Z3_ARGS: [&str; 4] = ["-in", "-smt2", "-t:10000", "-T:20"];

fn assumptions_satisfiable(obl: &SolverObligation) -> Option<bool> {
    let mut smt = String::from("(set-logic QF_BV)\n");
    for v in obl.vars.iter().collect::<BTreeSet<_>>() {
        if !v.starts_with("bv") && v != "_" && !v.chars().all(|c| c.is_ascii_digit()) {
            smt.push_str(&format!("(declare-const {} {})\n", v, smt_bv_type(64)));
        }
    }
    for a in &obl.assumptions {
        smt.push_str(&format!("(assert {})\n", a));
    }
    smt.push_str("(check-sat)\n");
    let out = Command::new("z3")
        .args(Z3_ARGS)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()
        .and_then(|mut child| {
            child.stdin.as_mut()?.write_all(smt.as_bytes()).ok()?;
            child.wait_with_output().ok()
        })?;
    match String::from_utf8_lossy(&out.stdout).lines().next()?.trim() {
        "sat" => Some(true),
        "unsat" => Some(false),
        _ => None,
    }
}

fn run_z3_obligation_with_smt(obligation: &SolverObligation, smt: String) -> SolverCheck {
    // Optional debug dump of the exact SMT handed to z3. Opt-in (ANUBIS_DUMP_SMT) and written to a
    // per-process path so concurrent `anubis check` runs never clobber a shared /tmp file.
    if std::env::var_os("ANUBIS_DUMP_SMT").is_some() {
        let path = std::env::temp_dir().join(format!("anubis_solver_{}.smt2", std::process::id()));
        let _ = std::fs::write(path, &smt);
    }
    let mut child = match Command::new("z3")
        .args(Z3_ARGS)
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
        // A z3 parse/sort ERROR means the SMT WE emitted is malformed (e.g. an undeclared symbol).
        // That is our bug, not an undecidable query — treat it as FAIL so it fails CLOSED. Emitting a
        // malformed obligation and then calling it "not a disproof" was the fail-OPEN hole that let a
        // parameter named `model`/`set`/`bvx` slip an unverified overflow contract past `check`.
        other if other.starts_with("(error") || stderr.contains("error") => SolverCheck {
            name: obligation.name.clone(),
            status: "FAIL".into(),
            detail: format!(
                "solver rejected the emitted SMT (z3: `{}` stderr `{}`); failing closed — a \
                 malformed obligation is not a proof",
                other,
                stderr.trim()
            ),
            model: None,
            smt,
        },
        // A genuine `unknown` (or empty output) on a well-formed query is NOT a counterexample.
        // Reporting it as FAIL would be an unsound "disproof". A runtime `assert` is still enforced at
        // runtime; QF_BV is decidable, so this branch is effectively unreachable for our obligations.
        other => SolverCheck {
            name: obligation.name.clone(),
            status: "UNKNOWN".into(),
            detail: format!(
                "solver did not decide this obligation (z3 returned `{}` stderr `{}`); not a disproof",
                other,
                stderr.trim()
            ),
            model: None,
            smt,
        },
    }
}

/// Parses a z3 `(get-model)` response into a map from declared variable name to its literal
/// SMT-LIB bit-vector value. Every variable this checker declares is a 64-bit bit-vector
/// (`smt_bv_type(64)`, see `check_obligations`), so each model entry has the fixed shape
/// `(define-fun <name> () (_ BitVec 64) <value>)`, with `<value>` either a `#x…`/`#b…` literal
/// or a `(_ bvN 64)` term, and the whole entry possibly wrapped across lines. Anchoring on the
/// fixed `(_ BitVec 64)` return-type tag (rather than a general-purpose SMT-LIB parser) is
/// sufficient because that invariant already holds everywhere else in this module.
fn parse_z3_model(model: &str) -> BTreeMap<String, String> {
    let mut bindings = BTreeMap::new();
    const MARK: &str = "(define-fun ";
    const TYPE_TAG: &str = "(_ BitVec 64)";
    let mut cursor = model;
    while let Some(rel) = cursor.find(MARK) {
        let after_mark = &cursor[rel + MARK.len()..];
        let Some(name) = after_mark.split_whitespace().next() else {
            break;
        };
        let name = name.to_string();
        let Some(type_rel) = after_mark.find(TYPE_TAG) else {
            break;
        };
        let after_type = after_mark[type_rel + TYPE_TAG.len()..].trim_start();
        let (value, tail) = if let Some(inner) = after_type.strip_prefix('(') {
            // `(_ bvDECIMAL 64)` nested-literal form — capture through its own close paren,
            // then skip the outer define-fun's closing paren.
            match inner.find(')') {
                Some(close) => {
                    let value = format!("({}", &inner[..=close]);
                    let after_value = &inner[close + 1..];
                    let tail = after_value.strip_prefix(')').unwrap_or(after_value);
                    (value, tail)
                }
                None => break,
            }
        } else {
            // `#x…`/`#b…` literal form — capture up to the define-fun's closing paren.
            match after_type.find(')') {
                Some(close) => (
                    after_type[..close].trim().to_string(),
                    &after_type[close + 1..],
                ),
                None => break,
            }
        };
        if !value.is_empty() {
            bindings.insert(name, value);
        }
        cursor = tail;
    }
    bindings
}

/// Runs `smt` through z3 and returns the first line of its stdout (`sat`/`unsat`/`unknown`),
/// or `None` if z3 could not be spawned or its output could not be read.
fn z3_check_sat_raw(smt: &str) -> Option<String> {
    let mut child = Command::new("z3")
        .args(Z3_ARGS)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    child.stdin.as_mut()?.write_all(smt.as_bytes()).ok()?;
    let output = child.wait_with_output().ok()?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(|l| l.trim().to_string())
}

/// Real counterexample replay: independent re-verification of a `sat` result, not a trust-the-model
/// string match. `smt` is the EXACT query the solver already decided as `sat` (assumptions ∧
/// ¬assertion, still carrying its own trailing `(check-sat)(get-model)`); `model` is the raw
/// `(get-model)` response z3 returned for it. This parses the concrete witness z3 assigned to each
/// variable, pins every variable to that literal value on top of the SAME assumptions and negated
/// assertion the solver checked, and asks z3 to re-decide the now fully-ground formula.
///
/// A genuine counterexample stays `sat` under its own witness (evaluating a ground formula is
/// decidable, not really "solving"). A bogus, hostile, or internally-inconsistent model — one that
/// doesn't actually satisfy the assumptions, or doesn't actually violate the assertion — makes the
/// ground formula `unsat`, and this returns `false`. Unlike the model text, this does not depend on
/// variable names or on any pre-known "bad" values; it re-derives the answer from the query itself.
pub fn replay_counterexample(smt: &str, model: &str) -> bool {
    let bindings = parse_z3_model(model);
    if bindings.is_empty() {
        // No parseable witness — cannot confirm the counterexample; fail closed.
        return false;
    }
    let base = match smt.find("(check-sat)") {
        Some(idx) => &smt[..idx],
        None => smt,
    };
    let mut replay_smt = base.to_string();
    for (name, value) in &bindings {
        replay_smt.push_str(&format!("(assert (= {name} {value}))\n"));
    }
    replay_smt.push_str("(check-sat)\n");
    matches!(z3_check_sat_raw(&replay_smt).as_deref(), Some("sat"))
}

fn expr_to_smt(e: &Expr, widths: &BTreeMap<String, u32>) -> String {
    expr_to_smt_with_width(e, widths, None)
}

/// The root variable of an assignment place (`x` in `x`, `xs[i]`, `p.f.g`), if any. Used to drop
/// a reassigned binding from the solver's modelable set.
fn assign_target_root(e: &Expr) -> Option<&str> {
    match e {
        Expr::Var(v) => Some(v),
        Expr::Index { base, .. } => assign_target_root(base),
        Expr::FieldAccess { base, .. } => assign_target_root(base),
        _ => None,
    }
}

/// True when `e` is a genuine integer term over solver-modelable variables: an integer literal,
/// a modelable variable, or arithmetic/bitwise composition of such. Used to decide whether an
/// assertion can be soundly encoded in QF_BV — a var that is NOT here (e.g. a string or bool
/// binding) must not be silently treated as a 32-bit integer.
/// A non-zero integer literal — a statically safe divisor for `/` and `%`.
fn is_nonzero_int_literal(e: &Expr) -> bool {
    matches!(e, Expr::Literal(l) if l.parse::<i64>().map(|n| n != 0).unwrap_or(false))
}

/// Sentinel key inserted into `solver_int_vars` to mark a variable as a PROVEN non-zero divisor — a
/// parameter guarded by `requires(v != 0)`/`requires(v > 0)` that the body never reassigns or shadows.
/// The `\u{1}` prefix is not a valid Anubis identifier, so the key can never collide with a real
/// variable and stays inert in the SMT (nothing references it); it only gates variable-divisor modeling.
fn nzdiv_mark(v: &str) -> String {
    format!("\u{1}nzdiv:{v}")
}

/// If `req` directly guarantees a bare variable is non-zero, return that variable. Recognizes a
/// comparison of a variable against an integer literal whose truth EXCLUDES 0 — `v != 0`, `v > k`
/// (k≥0), `v >= k` (k≥1), `v < k` (k≤0), `v <= k` (k≤−1), and the mirror `k OP v`. Conservative: a
/// form that does NOT exclude 0 (`v >= 0`, `v > -1`, `v != 5`, …) returns None, so an unproven divisor
/// stays fail-closed. Soundness: only a modeled-integer variable is ever marked (see the call site),
/// so this same clause is a modelable assumption in every obligation, and z3 therefore evaluates
/// `bvsdiv`/`bvsrem` only over models where the divisor is non-zero.
fn requires_nonzero_var(req: &Expr) -> Option<String> {
    fn as_var(e: &Expr) -> Option<String> {
        match e {
            Expr::Var(v) => Some(v.clone()),
            _ => None,
        }
    }
    fn as_int_lit(e: &Expr) -> Option<i64> {
        match e {
            Expr::Literal(l) => l.parse::<i64>().ok(),
            // A negative literal often parses as unary minus over a non-negative literal.
            Expr::Unary { op, expr } if op == "-" => match expr.as_ref() {
                Expr::Literal(l) => l.parse::<i64>().ok().map(i64::wrapping_neg),
                _ => None,
            },
            _ => None,
        }
    }
    // `k OP v` is the same relation as `v FLIP(OP) k`.
    fn flip(op: &str) -> &str {
        match op {
            ">" => "<",
            ">=" => "<=",
            "<" => ">",
            "<=" => ">=",
            other => other, // `==`/`!=` are symmetric
        }
    }
    let Expr::Binary { op, lhs, rhs } = req else {
        return None;
    };
    let (var, op, k) = if let (Some(v), Some(k)) = (as_var(lhs), as_int_lit(rhs)) {
        (v, op.as_str(), k)
    } else if let (Some(k), Some(v)) = (as_int_lit(lhs), as_var(rhs)) {
        (v, flip(op.as_str()), k)
    } else {
        return None;
    };
    let excludes_zero = match op {
        "!=" => k == 0,
        ">" => k >= 0,  // v > k ≥ 0  ⟹  v ≥ 1
        ">=" => k >= 1, // v ≥ k ≥ 1
        "<" => k <= 0,  // v < k ≤ 0  ⟹  v ≤ -1
        "<=" => k <= -1,
        _ => false,
    };
    excludes_zero.then_some(var)
}

fn is_int_modelable(e: &Expr, int_vars: &BTreeSet<String>) -> bool {
    match e {
        Expr::Var(v) => int_vars.contains(v),
        // Only a literal that fits i64 is modelable: the runtime holds integers as i64, and a literal
        // beyond i64::MAX (e.g. 2^64) is parsed as f64 at runtime while `(_ bv… 64)` would silently
        // reduce it mod 2^64 — the solver "proved" `x + 2^64 <= x` because it saw `x + 0`.
        Expr::Literal(l) => !l.is_empty() && l.parse::<i64>().is_ok(),
        Expr::Binary { op, lhs, rhs } => {
            // Ops that model i64 EXACTLY as 64-bit bit-vectors: add/sub/mul (wrap like i64), bitwise
            // and/or/xor, and the shifts `<<`/`>>` (mod-64 mask + arithmetic right shift — see the
            // encoder). `/` and `%` are modelable only with a statically NON-ZERO divisor (a non-zero
            // integer literal): then bvsdiv/bvsrem match wrapping_div/wrapping_rem and never model the
            // runtime's division-by-zero trap. A variable divisor needs a proof it is non-zero first
            // (a later increment), so it stays unmodelable — the contract fails closed, not unsound.
            match op.as_str() {
                "+" | "-" | "*" | "&" | "|" | "^" | "<<" | ">>" => {
                    is_int_modelable(lhs, int_vars) && is_int_modelable(rhs, int_vars)
                }
                // `/`/`%` model soundly (bvsdiv/bvsrem match wrapping_div/wrapping_rem and never model
                // the runtime's division-by-zero trap) only when the divisor is statically non-zero: a
                // non-zero integer literal, or a variable proven non-zero by a `requires` guard (marked
                // via `nzdiv_mark` in `int_vars`). A bare variable with no such guard stays fail-closed.
                "/" | "%" => {
                    is_int_modelable(lhs, int_vars)
                        && (is_nonzero_int_literal(rhs)
                            || matches!(rhs.as_ref(), Expr::Var(v) if int_vars.contains(&nzdiv_mark(v))))
                }
                _ => false,
            }
        }
        // Unary negation (`-`) and bitwise NOT (`~`, i.e. `!v` on i64 = -v-1) model exactly.
        Expr::Unary { op, expr } => (op == "-" || op == "~") && is_int_modelable(expr, int_vars),
        // A cast is modelable only when it cannot change the i64 value. `x as u8`/`u16`/`u32` truncate
        // at runtime, so modeling them as the identity is unsound (it "proved" `(x as u8) == x` while
        // `ident8(256)` runs to 0). Only 64-bit-target casts are value-preserving.
        Expr::Cast { expr, ty } => cast_preserves_i64(ty) && is_int_modelable(expr, int_vars),
        // `declassify(x)` forwards x's value, so it is int-modelable iff x is. But `assume(E)`/`assert(E)`
        // in VALUE position evaluate to Bool(true) at runtime (NOT E), so they are never integer-valued —
        // modeling them as E let `return assume(x)` certify `result == x`. They fall through to `false`.
        Expr::Declassify { inner, .. } => is_int_modelable(inner, int_vars),
        // Pure integer builtins that select/negate operands: `abs(x)` (wrapping_abs -> bvneg, wraps at
        // MIN identically), and `min`/`max` of two i64 args (signed `bvsle` select, matching
        // anubis_value_cmp). Only these exact callee/arity shapes; any other call stays unmodelable.
        Expr::Call { callee, args } => match (callee.as_str(), args.len()) {
            ("abs", 1) => is_int_modelable(&args[0], int_vars),
            ("min", 2) | ("max", 2) => args.iter().all(|a| is_int_modelable(a, int_vars)),
            _ => false,
        },
        _ => false,
    }
}

/// True when `e` is a boolean formula the solver can soundly discharge: a boolean literal, a
/// comparison of integer-modelable terms, or a boolean combination of such. A bare variable, a
/// string comparison, or anything else is NOT modelable — the checker must decline to prove or
/// disprove it (it is still enforced at runtime) rather than fabricate a bit-vector counterexample.
fn is_bool_modelable(e: &Expr, int_vars: &BTreeSet<String>) -> bool {
    match e {
        Expr::Literal(l) => l == "true" || l == "false",
        Expr::Binary { op, lhs, rhs } => match op.as_str() {
            "==" | "!=" | "<" | "<=" | ">" | ">=" => {
                is_int_modelable(lhs, int_vars) && is_int_modelable(rhs, int_vars)
            }
            "&&" | "||" => is_bool_modelable(lhs, int_vars) && is_bool_modelable(rhs, int_vars),
            _ => false,
        },
        Expr::Unary { op, expr } => op == "!" && is_bool_modelable(expr, int_vars),
        Expr::Declassify { inner, .. } => is_bool_modelable(inner, int_vars),
        // `assume(E)`/`assert(E)` evaluate to Bool(true) at runtime regardless of E, so as a VALUE they
        // are the boolean literal `true` — modelable, but as `true`, never as E (see the encoder).
        Expr::Assume(_) | Expr::Assert(_) => true,
        _ => false,
    }
}

fn expr_to_smt_value(e: &Expr, widths: &BTreeMap<String, u32>) -> Option<String> {
    match e {
        Expr::Var(v) if widths.contains_key(v) => Some(smt_var(v)),
        Expr::Literal(l) if !l.is_empty() && l.parse::<i64>().is_ok() => {
            Some(expr_to_smt(e, widths))
        }
        Expr::Binary { lhs, rhs, .. } => {
            expr_to_smt_value(lhs, widths)?;
            expr_to_smt_value(rhs, widths)?;
            Some(expr_to_smt(e, widths))
        }
        Expr::Unary { op, expr } if op == "-" || op == "!" || op == "~" => {
            expr_to_smt_value(expr, widths)?;
            Some(expr_to_smt(e, widths))
        }
        // A modeled builtin value: `let y = abs(x)` (or min/max) MUST emit its defining fact `y == abs(x)`
        // via the shared encoder — otherwise `is_int_modelable` admits `y` into the modelable set as a
        // FREE variable and a later contract over `y` is checked against an unconstrained symbol (a false
        // ALARM). Mirror the exact callee/arity set is_int_modelable admits, so the two never diverge.
        Expr::Call { callee, args }
            if (callee == "abs" && args.len() == 1)
                || ((callee == "min" || callee == "max") && args.len() == 2) =>
        {
            for a in args {
                expr_to_smt_value(a, widths)?;
            }
            Some(expr_to_smt(e, widths))
        }
        Expr::Cast { expr, ty } => {
            // A TRUNCATING cast (`x as u8`) has NO sound integer value fact — modeling it as the
            // identity recorded a false `y == x` that a loop invariant could later force-model and
            // "prove" against the pre-truncation value. Only a value-preserving (64-bit) cast keeps
            // the inner's value. (Mirrors `is_int_modelable`'s cast rule.)
            if !cast_preserves_i64(ty) {
                return None;
            }
            expr_to_smt_value(expr, widths)
        }
        Expr::Declassify { inner, .. } => expr_to_smt_value(inner, widths),
        // `assume(E)`/`assert(E)` as a value are Bool(true) at runtime, not E (see is_int_modelable).
        Expr::Assume(_) | Expr::Assert(_) => Some("true".to_string()),
        _ => None,
    }
}

#[allow(clippy::only_used_in_recursion)]
fn expr_to_smt_with_width(
    e: &Expr,
    widths: &BTreeMap<String, u32>,
    expected_width: Option<u32>,
) -> String {
    match e {
        Expr::Var(v) => smt_var(v),
        // Boolean literals are SMT `Bool`, not bit-vectors: emitting `(_ bvtrue 32)` produced the
        // Z3 error "unknown constant bvtrue" that made `check` reject `assert(true)`.
        Expr::Literal(l) if l == "true" || l == "false" => l.clone(),
        // The runtime represents EVERY integer as i64 (type annotations like u8/u32 are inert —
        // plain arithmetic never wraps to the annotated width, e.g. `let x: u8 = 200; x + 100` is
        // 300, not 44). So integers are modeled as 64-bit bit-vectors with SIGNED comparisons,
        // matching i64 exactly. A 32-bit unsigned model was unsound: it "disproved" true statements
        // like `65536 * 65536 != 0` (wrapped to 0) and `0 - 1 < 0` (unsigned bvult).
        Expr::Literal(l) => format!("(_ bv{} 64)", l),
        Expr::Binary { op, lhs, rhs } => {
            // Logical connectives combine Bool operands, not bit-vectors.
            if op == "&&" || op == "||" {
                let l = expr_to_smt_with_width(lhs, widths, None);
                let r = expr_to_smt_with_width(rhs, widths, None);
                let smt_op = if op == "&&" { "and" } else { "or" };
                return format!("({} {} {})", smt_op, l, r);
            }
            let l = expr_to_smt_with_width(lhs, widths, Some(64));
            let r = expr_to_smt_with_width(rhs, widths, Some(64));
            match op.as_str() {
                "+" => format!("(bvadd {} {})", l, r),
                "-" => format!("(bvsub {} {})", l, r),
                "*" => format!("(bvmul {} {})", l, r),
                "&" => format!("(bvand {} {})", l, r),
                "|" => format!("(bvor {} {})", l, r),
                "^" => format!("(bvxor {} {})", l, r),
                "==" => format!("(= {} {})", l, r),
                "!=" => format!("(not (= {} {}))", l, r),
                "<" => format!("(bvslt {} {})", l, r),
                "<=" => format!("(bvsle {} {})", l, r),
                ">" => format!("(bvsgt {} {})", l, r),
                ">=" => format!("(bvsge {} {})", l, r),
                // Shifts mask the shift amount mod 64 (matching the runtime's `rem_euclid(64)`, which
                // equals the low 6 bits via unsigned `bvurem`), and `>>` is ARITHMETIC — the runtime
                // uses `i64::wrapping_shr`, which sign-extends. `bvlshr` (logical) would be UNSOUND
                // (it would "prove" `(-8 >> 1) == 4` while the program computes -4).
                "<<" => format!("(bvshl {} (bvurem {} (_ bv64 64)))", l, r),
                ">>" => format!("(bvashr {} (bvurem {} (_ bv64 64)))", l, r),
                // Division/modulo, reached only with a non-zero literal divisor (is_int_modelable).
                // bvsdiv = truncated toward zero and bvsdiv(MIN,-1)=MIN, matching i64::wrapping_div;
                // bvsrem takes the sign of the dividend, matching i64::wrapping_rem (NOT bvsmod,
                // which takes the sign of the divisor).
                "/" => format!("(bvsdiv {} {})", l, r),
                "%" => format!("(bvsrem {} {})", l, r),
                _ => format!("({} {} {})", op, l, r),
            }
        }
        Expr::Unary { op, expr } => {
            let inner = expr_to_smt_with_width(expr, widths, expected_width);
            match op.as_str() {
                "-" => format!("(bvneg {})", inner),
                "~" => format!("(bvnot {})", inner),
                "!" => format!("(not {})", inner),
                _ => inner,
            }
        }
        Expr::Cast { expr, ty } => expr_to_smt_with_width(expr, widths, Some(bitwidth_of(ty))),
        Expr::Declassify { inner, .. } => expr_to_smt_with_width(inner, widths, expected_width),
        // `assume(E)`/`assert(E)` in value position evaluate to Bool(true) at runtime, not E.
        Expr::Assume(_) | Expr::Assert(_) => "true".to_string(),
        // `abs`/`min`/`max` builtins as ite (vetted by is_int_modelable, so the shapes always match).
        // `abs`: `bvneg` wraps at MIN exactly like `wrapping_abs`. `min`/`max`: signed `bvsle` select,
        // matching `anubis_value_cmp`'s i64 ordering (min picks the smaller, max the larger).
        Expr::Call { callee, args } if callee == "abs" && args.len() == 1 => {
            let x = expr_to_smt_with_width(&args[0], widths, Some(64));
            format!("(ite (bvslt {x} (_ bv0 64)) (bvneg {x}) {x})")
        }
        Expr::Call { callee, args } if (callee == "min" || callee == "max") && args.len() == 2 => {
            let a = expr_to_smt_with_width(&args[0], widths, Some(64));
            let b = expr_to_smt_with_width(&args[1], widths, Some(64));
            if callee == "min" {
                format!("(ite (bvsle {a} {b}) {a} {b})")
            } else {
                format!("(ite (bvsle {a} {b}) {b} {a})")
            }
        }
        Expr::TaintSource { label } => format!("taint_source_{}", label.replace("\"", "")),
        Expr::Symbolic { .. } => "symbolic".into(),
        _ => "true".into(),
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
                    | "ite"
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

/// Whether a declared type annotation carries the `tainted<T>` qualifier. Delegates to the anchored
/// `ty::is_tainted` rather than a bare `.contains("tainted")` substring test — the substring version
/// this replaced false-positived on any type merely NAMED with that substring (e.g. a struct called
/// `TaintedRecord` would have been wrongly seeded as tainted).
fn is_tainted_type(ty: Option<&str>) -> bool {
    ty.is_some_and(ty::is_tainted)
}

/// The expression a function returns at its tail: the last statement when it is `return X` or a bare
/// value expression. None when the body ends in a statement (which yields the default `0`).
/// Collect the IMPLICIT tail values a function body (or an `if`-arm) can yield: every branch of a
/// tail `if`/`match`, a block's tail expression, or the literal `0` when the body falls off the end
/// (ends in a `let`/assign/loop, or a tail `if` with no `else`). Without this, a function whose body
/// is a bare tail `if/else` (the idiomatic tail expression) has its `ensures` obligated at ZERO points
/// and is silently certified. `collect_tail_return` collects a bare tail `return X` value here (true
/// at the function level, where the early-return scan excludes it); inside an `if`-arm it is false
/// (the early-return scan already covers those explicit returns — avoids a double, weaker check).
fn tail_values(body: &[Stmt], collect_tail_return: bool, out: &mut Vec<Expr>) {
    match body.last() {
        None => out.push(zero_literal()),
        Some(Stmt::ExprStmt(Expr::Call { callee, args })) if callee == "return" => {
            if collect_tail_return {
                if let Some(e) = args.first() {
                    out.push(e.clone());
                }
            }
        }
        Some(Stmt::ExprStmt(e)) => expr_tail_values(e, out),
        Some(Stmt::If { then, else_, .. }) => {
            tail_values(then, false, out);
            match else_ {
                Some(e) => tail_values(e, false, out),
                None => out.push(zero_literal()),
            }
        }
        // A tail `let`/assign/`while`/`for`/`loop`/etc. yields the default `0`.
        Some(_) => out.push(zero_literal()),
    }
}

/// The tail values of an expression in value position (an `if`/`match`/block used as a tail value).
fn expr_tail_values(e: &Expr, out: &mut Vec<Expr>) {
    match e {
        Expr::If { then, else_, .. } | Expr::IfLet { then, else_, .. } => {
            expr_tail_values(then, out);
            expr_tail_values(else_, out);
        }
        Expr::Match { arms, .. } => {
            for a in arms {
                expr_tail_values(&a.body, out);
            }
        }
        Expr::Block { stmts, tail } => match tail {
            Some(t) => expr_tail_values(t, out),
            None => tail_values(stmts, false, out),
        },
        // An explicit `return` in expression position is handled by the early-return scan.
        Expr::Call { callee, .. } if callee == "return" => {}
        other => out.push(other.clone()),
    }
}

fn zero_literal() -> Expr {
    Expr::Literal("0".to_string())
}

/// Substitute variables in a contract expression by name. Used to specialize a callee's contract at
/// a call site (`result` -> the returned expression, each parameter -> its argument), so
/// `ensures(result > x)` over `return x + 1` becomes `(x + 1) > x`, and a caller's
/// `ensures(result > 0)` with `x := 5` at `let a = f(5)` becomes `a > 0`.
fn substitute_vars(e: &Expr, map: &BTreeMap<String, Expr>) -> Expr {
    match e {
        Expr::Var(v) => map.get(v).cloned().unwrap_or_else(|| e.clone()),
        Expr::Binary { op, lhs, rhs } => Expr::Binary {
            op: op.clone(),
            lhs: Box::new(substitute_vars(lhs, map)),
            rhs: Box::new(substitute_vars(rhs, map)),
        },
        Expr::Unary { op, expr } => Expr::Unary {
            op: op.clone(),
            expr: Box::new(substitute_vars(expr, map)),
        },
        Expr::Cast { expr, ty } => Expr::Cast {
            expr: Box::new(substitute_vars(expr, map)),
            ty: ty.clone(),
        },
        // SOUNDNESS-CRITICAL: substitute_vars MUST recurse into every form that `is_int_modelable`/
        // `is_bool_modelable` admit. Composition substitutes the callee's params/`result` into its
        // contract; if a MODELABLE subterm is cloned WITHOUT substituting, its callee-parameter names
        // survive and re-bind to the caller's scope — a precondition bypass + a certified-false
        // postcondition (e.g. `g(150)` against `requires(abs(x) < 100)` was checked as `abs(5) < 100`).
        // The modelable set is exactly: Var/Literal/Binary/Unary/Cast (above) + the abs/min/max builtin
        // Call + Declassify/Assume/Assert. Any NON-modelable form is safely cloned by the catch-all: a
        // mapped var hidden inside it makes the whole contract non-modelable, so the gate rejects it
        // fail-closed rather than mis-model it. If a new modelable form is added to the gate, ADD IT HERE.
        Expr::Call { callee, args } => Expr::Call {
            callee: callee.clone(),
            args: args.iter().map(|a| substitute_vars(a, map)).collect(),
        },
        Expr::Declassify {
            inner,
            policy,
            reason,
        } => Expr::Declassify {
            inner: Box::new(substitute_vars(inner, map)),
            policy: policy.clone(),
            reason: reason.clone(),
        },
        Expr::Assume(inner) => Expr::Assume(Box::new(substitute_vars(inner, map))),
        Expr::Assert(inner) => Expr::Assert(Box::new(substitute_vars(inner, map))),
        other => other.clone(),
    }
}

/// Convenience: substitute a single `result` variable.
fn substitute_result(e: &Expr, repl: &Expr) -> Expr {
    let mut m = BTreeMap::new();
    m.insert("result".to_string(), repl.clone());
    substitute_vars(e, &m)
}

/// Create an `ensures` obligation for a return: substitute `result` with the returned expression and
/// assert it under the given assumptions.
///
/// Soundness rule (FAIL-CLOSED — discharge or reject, never silently skip). `ensures`/`requires`
/// contracts are compile-time only: the transpiler (`backends/run.rs`) emits NO runtime check for
/// them, so a contract the checker does not prove is enforced NOWHERE. Therefore every `ensures`
/// must be either discharged by the solver or reported as an error — a silent skip would let a green
/// `anubis check` certify a postcondition that is false at runtime (e.g. `ensures(result == "wrong")`
/// returning `"ok"`, or a float/cast/untyped-param contract the bit-vector solver cannot model).
///
/// So: substitute `result` with the returned expression; if the concrete predicate is modelable in
/// QF_BV, emit the obligation (the solver proves or disproves it); otherwise REJECT with
/// `ANUBIS_CONTRACT_UNPROVABLE`. A postcondition that needs a value the solver cannot reason about
/// (a string/list, a float, a truncating cast, a call whose contract we did not carry) must be
/// rewritten as a provable integer bound, or expressed as a runtime `assert` in the body (which IS
/// enforced at runtime), not as an `ensures`.
fn push_ensures_obligations(
    ctx: &mut SemanticContext,
    ensures: &[Expr],
    ret_expr: &Expr,
    assumptions: &[String],
    span: Span,
) {
    for ens in ensures {
        let concrete = substitute_result(ens, ret_expr);
        if is_bool_modelable(&concrete, &ctx.solver_int_vars) {
            let smt = expr_to_smt(&concrete, &ctx.symbolic_widths);
            let mut vars = BTreeSet::new();
            collect_vars_from_smt(&smt, &mut vars);
            for a in assumptions {
                collect_vars_from_smt(a, &mut vars);
            }
            ctx.solver_obligations.push(SolverObligation {
                name: format!("ensures:{smt}"),
                assumptions: assumptions.to_vec(),
                assertion: smt,
                vars: vars.into_iter().collect(),
            });
        } else {
            // A postcondition the solver cannot faithfully model. Contracts are NOT runtime-enforced,
            // so certifying this would be a silent overclaim: fail closed. Name the detectable cause
            // precisely (float vs string) so the diagnostic tells the truth about *why*, rather than
            // lumping every non-modelable case under one code.
            ctx.diagnostics.push(SemanticDiagnostic {
                code: Some(unmodelable_contract_code(&concrete).into()),
                message: "cannot verify this `ensures` postcondition: it is not statically \
                     modelable (a float, a string/list, a truncating cast, an unmodeled or reassigned \
                     variable, or a value from a call whose contract is not carried). Contracts are \
                     compile-time only and are never checked at runtime, so an unprovable one is \
                     rejected — restate it as a provable integer bound, or use a runtime `assert` in \
                     the body instead"
                    .to_string(),
                span: Some((span.start, span.end)),
            });
        }
    }
}

/// Best-effort precise diagnostic code for a non-modelable `ensures`. A float or string literal in
/// the (result-substituted) predicate is the common, cheaply-detectable cause; anything else
/// (truncating cast, reassigned/unmodeled variable, uncarried call contract) stays under the
/// general `ANUBIS_CONTRACT_UNPROVABLE`. Honest by construction: a specific code is emitted only
/// when that specific cause is actually present in the predicate.
fn unmodelable_contract_code(e: &Expr) -> &'static str {
    fn scan(e: &Expr) -> Option<&'static str> {
        match e {
            Expr::StrLiteral(_) => Some("ANUBIS_STRING_CONTRACT_UNMODELED"),
            Expr::Literal(s) => {
                let t = s.trim();
                if t.starts_with('"') || t.starts_with('\'') {
                    Some("ANUBIS_STRING_CONTRACT_UNMODELED")
                } else if t.parse::<i64>().is_err() && t.parse::<f64>().is_ok() {
                    Some("ANUBIS_FLOAT_CONTRACT_UNMODELED")
                } else {
                    None
                }
            }
            Expr::Binary { lhs, rhs, .. } => scan(lhs).or_else(|| scan(rhs)),
            Expr::Unary { expr, .. } | Expr::Cast { expr, .. } => scan(expr),
            _ => None,
        }
    }
    scan(e).unwrap_or("ANUBIS_CONTRACT_UNPROVABLE")
}

/// Collect the variable names referenced by an expression (for deciding which loop-carried
/// variables an invariant / condition constrains).
fn collect_expr_vars(e: &Expr, out: &mut BTreeSet<String>) {
    match e {
        Expr::Var(v) => {
            out.insert(v.clone());
        }
        Expr::Binary { lhs, rhs, .. } => {
            collect_expr_vars(lhs, out);
            collect_expr_vars(rhs, out);
        }
        Expr::Unary { expr, .. } | Expr::Cast { expr, .. } => collect_expr_vars(expr, out),
        Expr::Declassify { inner, .. } | Expr::Assume(inner) | Expr::Assert(inner) => {
            collect_expr_vars(inner, out)
        }
        // Must descend into the abs/min/max builtin Call: this set drives the "ensures references a
        // reassigned/shadowed parameter" fail-closed check, and MISSING a variable (under-approx) is
        // unsound — it would let `ensures(result == abs(x)) { x = 0-5; ... }` be certified against the
        // mutated `x` while a caller assumes the entry value. Over-approx (collecting too many) only
        // makes that check more conservative. Same modelable-form coupling as `substitute_vars`.
        Expr::Call { args, .. } => {
            for a in args {
                collect_expr_vars(a, out);
            }
        }
        _ => {}
    }
}

/// True when an expression contains NO statement-bearing construct — no block, `if`/`match`/`if let`
/// expression, or lambda — so it cannot hide an assignment or an escape. A loop body made only of
/// such simple expressions (plus plain assignments) is a flat straight-line sequence the transition
/// extractor can model soundly. This is the robust guard against writes hidden inside expressions
/// (e.g. `let z = if c { x = x + 1; 0 } else { 0 }`), which a statement-only scan never sees.
fn expr_is_simple(e: &Expr) -> bool {
    match e {
        Expr::Block { .. }
        | Expr::If { .. }
        | Expr::Match { .. }
        | Expr::IfLet { .. }
        | Expr::Lambda { .. } => false,
        Expr::Call { args, .. } | Expr::ArrayLiteral { elements: args } => {
            args.iter().all(expr_is_simple)
        }
        Expr::EnumConstruct { fields, .. } => fields.iter().all(expr_is_simple),
        Expr::CallExpr { callee, args } => {
            expr_is_simple(callee) && args.iter().all(expr_is_simple)
        }
        Expr::Binary { lhs, rhs, .. } => expr_is_simple(lhs) && expr_is_simple(rhs),
        Expr::Unary { expr, .. } | Expr::Cast { expr, .. } => expr_is_simple(expr),
        Expr::Try(expr) | Expr::Assume(expr) | Expr::Assert(expr) => expr_is_simple(expr),
        Expr::Tainted { inner, .. } | Expr::Declassify { inner, .. } => expr_is_simple(inner),
        Expr::Index { base, index } => expr_is_simple(base) && expr_is_simple(index),
        Expr::FieldAccess { base, .. } => expr_is_simple(base),
        Expr::StructLiteral { fields, .. } => fields.iter().all(|(_, e)| expr_is_simple(e)),
        Expr::MapLiteral { entries, .. } => entries
            .iter()
            .all(|(k, v)| expr_is_simple(k) && expr_is_simple(v)),
        // Leaves: Var / Literal / StrLiteral / Symbolic / TaintSource / UnifiedBuffer / RawPtr / Other.
        _ => true,
    }
}

/// Collect the ROOT of every place the loop body assigns, at ANY depth and in ANY form (a plain
/// `x = ...`, a compound `x += ...`, an `a[i] = ...` / `p.f = ...` place, or a write nested in an
/// `if`/nested loop). These are exactly the variables whose value can change across iterations, so a
/// pre-loop fact about any of them is stale inside the loop and after it — even an AUXILIARY variable
/// that no invariant mentions but a loop-carried variable reads (`x = x + z`). Without this, such a
/// frozen auxiliary let the checker "prove" a false invariant.
fn collect_assigned_roots(body: &[Stmt], out: &mut BTreeSet<String>) {
    for s in body {
        match s {
            Stmt::Assign { target, value } => {
                if let Some(r) = assign_target_root(target) {
                    out.insert(r.to_string());
                }
                expr_assigned_roots(target, out);
                expr_assigned_roots(value, out);
            }
            // A `let`/`let-pattern`/expression statement can hide a write inside an `if`/`match`/block
            // EXPRESSION (`let z = if c { x = x + 1; 0 } else { 0 };`) which a statement-only scan would
            // miss — walk the expressions too.
            Stmt::Let { init, .. } | Stmt::LetPattern { init, .. } => {
                expr_assigned_roots(init, out)
            }
            Stmt::ExprStmt(e) => expr_assigned_roots(e, out),
            Stmt::If { cond, then, else_ } => {
                expr_assigned_roots(cond, out);
                collect_assigned_roots(then, out);
                if let Some(e) = else_ {
                    collect_assigned_roots(e, out);
                }
            }
            Stmt::While { cond, body, .. } => {
                expr_assigned_roots(cond, out);
                collect_assigned_roots(body, out);
            }
            Stmt::WhileLet { expr, body, .. } => {
                expr_assigned_roots(expr, out);
                collect_assigned_roots(body, out);
            }
            Stmt::For { source, body, .. } => {
                match source {
                    crate::frontend::ForSource::Range { start, end } => {
                        expr_assigned_roots(start, out);
                        expr_assigned_roots(end, out);
                    }
                    crate::frontend::ForSource::Collection { expr } => {
                        expr_assigned_roots(expr, out)
                    }
                }
                collect_assigned_roots(body, out);
            }
            Stmt::Loop { body, .. }
            | Stmt::ResearchBlock { body, .. }
            | Stmt::ExploitBlock { body, .. } => collect_assigned_roots(body, out),
            Stmt::HybridBlock { gpu, cpu, prove } => {
                for b in [gpu, cpu, prove].into_iter().flatten() {
                    collect_assigned_roots(b, out);
                }
            }
            _ => {}
        }
    }
}

/// Collect every name a body BINDS with `let`/`let (…)` at any depth (used to detect a `let` that
/// shadows a parameter named in an `ensures`).
/// Collect every name a body BINDS at any depth — `let`/`let (…)`, a `for`/`while let` binder, AND
/// (crucially) every binder introduced in EXPRESSION position: a `match`-arm or `if let` pattern, a
/// lambda parameter, or a `let` inside an expression-position block. SOUNDNESS: this set is the rebind
/// set gating both the "ensures over a reassigned/shadowed parameter" fail-closed rejection AND
/// guarded-divisor nzdiv eligibility. Missing an expression-position binder let `match 2 { n => 6/n }`
/// shadow a guarded parameter `n` and certify a false contract (the arm rebinds n, so the runtime body
/// is `6/2`, not `6/entry-n`). COMPLETE recursion — every prior incremental fix (for-loop var, while-let)
/// missed a variant, so this walks the full statement AND expression tree; over-collection is safe.
fn collect_let_bound(body: &[Stmt], out: &mut BTreeSet<String>) {
    for s in body {
        match s {
            Stmt::Let { name, init, .. } => {
                out.insert(name.clone());
                expr_let_bound(init, out);
            }
            Stmt::LetPattern { pattern, init, .. } => {
                for n in pattern.bound_names() {
                    out.insert(n);
                }
                expr_let_bound(init, out);
            }
            Stmt::If { cond, then, else_ } => {
                expr_let_bound(cond, out);
                collect_let_bound(then, out);
                if let Some(e) = else_ {
                    collect_let_bound(e, out);
                }
            }
            Stmt::For {
                var, source, body, ..
            } => {
                out.insert(var.clone());
                match source {
                    crate::frontend::ForSource::Range { start, end } => {
                        expr_let_bound(start, out);
                        expr_let_bound(end, out);
                    }
                    crate::frontend::ForSource::Collection { expr } => expr_let_bound(expr, out),
                }
                collect_let_bound(body, out);
            }
            Stmt::While { cond, body, .. } => {
                expr_let_bound(cond, out);
                collect_let_bound(body, out);
            }
            Stmt::WhileLet {
                pattern,
                expr,
                body,
            } => {
                for n in pattern.bound_names() {
                    out.insert(n);
                }
                expr_let_bound(expr, out);
                collect_let_bound(body, out);
            }
            Stmt::Loop { body, .. }
            | Stmt::ResearchBlock { body, .. }
            | Stmt::ExploitBlock { body, .. } => collect_let_bound(body, out),
            Stmt::HybridBlock { gpu, cpu, prove } => {
                for b in [gpu, cpu, prove].into_iter().flatten() {
                    collect_let_bound(b, out);
                }
            }
            Stmt::Assign { target, value } => {
                expr_let_bound(target, out);
                expr_let_bound(value, out);
            }
            Stmt::ExprStmt(e) => expr_let_bound(e, out),
            // Break / Continue / SpecBlock bind no runtime names.
            _ => {}
        }
    }
}

/// Collect names BOUND by patterns/lambdas inside an EXPRESSION (match arms, `if let`, lambda params,
/// and `let`s in a block-expr). Mirrors `expr_assigned_roots`' full traversal so no subexpression is
/// missed. See `collect_let_bound` for why under-collection is unsound and over-collection is safe.
fn expr_let_bound(e: &Expr, out: &mut BTreeSet<String>) {
    match e {
        Expr::Block { stmts, tail } => {
            collect_let_bound(stmts, out);
            if let Some(t) = tail {
                expr_let_bound(t, out);
            }
        }
        Expr::If {
            cond, then, else_, ..
        } => {
            expr_let_bound(cond, out);
            expr_let_bound(then, out);
            expr_let_bound(else_, out);
        }
        Expr::IfLet {
            pattern,
            scrutinee,
            then,
            else_,
            ..
        } => {
            for n in pattern.bound_names() {
                out.insert(n);
            }
            expr_let_bound(scrutinee, out);
            expr_let_bound(then, out);
            expr_let_bound(else_, out);
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            expr_let_bound(scrutinee, out);
            for a in arms {
                for n in a.pattern.bound_names() {
                    out.insert(n);
                }
                if let Some(g) = &a.guard {
                    expr_let_bound(g, out);
                }
                expr_let_bound(&a.body, out);
            }
        }
        Expr::Lambda { params, body } => {
            for p in params {
                out.insert(p.clone());
            }
            expr_let_bound(body, out);
        }
        Expr::Binary { lhs, rhs, .. } => {
            expr_let_bound(lhs, out);
            expr_let_bound(rhs, out);
        }
        Expr::Unary { expr, .. } | Expr::Cast { expr, .. } => expr_let_bound(expr, out),
        Expr::Try(expr) | Expr::Assume(expr) | Expr::Assert(expr) => expr_let_bound(expr, out),
        Expr::Tainted { inner, .. } | Expr::Declassify { inner, .. } => expr_let_bound(inner, out),
        Expr::Index { base, index } => {
            expr_let_bound(base, out);
            expr_let_bound(index, out);
        }
        Expr::FieldAccess { base, .. } => expr_let_bound(base, out),
        Expr::Call { args, .. }
        | Expr::ArrayLiteral { elements: args }
        | Expr::EnumConstruct { fields: args, .. } => {
            for x in args {
                expr_let_bound(x, out);
            }
        }
        Expr::CallExpr { callee, args } => {
            expr_let_bound(callee, out);
            for x in args {
                expr_let_bound(x, out);
            }
        }
        Expr::StructLiteral { fields, .. } => {
            for (_, x) in fields {
                expr_let_bound(x, out);
            }
        }
        Expr::MapLiteral { entries, .. } => {
            for (k, v) in entries {
                expr_let_bound(k, out);
                expr_let_bound(v, out);
            }
        }
        _ => {}
    }
}

/// Collect assignment roots hidden INSIDE an expression — an assignment can live in a block, `if`,
/// `match`, or `if let` used in expression position (a `let` initializer, a call argument, a branch
/// value). Mutually recursive with `collect_assigned_roots` via `Expr::Block`.
fn expr_assigned_roots(e: &Expr, out: &mut BTreeSet<String>) {
    match e {
        Expr::Block { stmts, tail } => {
            collect_assigned_roots(stmts, out);
            if let Some(t) = tail {
                expr_assigned_roots(t, out);
            }
        }
        Expr::If {
            cond, then, else_, ..
        } => {
            expr_assigned_roots(cond, out);
            expr_assigned_roots(then, out);
            expr_assigned_roots(else_, out);
        }
        Expr::IfLet {
            scrutinee,
            then,
            else_,
            ..
        } => {
            expr_assigned_roots(scrutinee, out);
            expr_assigned_roots(then, out);
            expr_assigned_roots(else_, out);
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            expr_assigned_roots(scrutinee, out);
            for a in arms {
                if let Some(g) = &a.guard {
                    expr_assigned_roots(g, out);
                }
                expr_assigned_roots(&a.body, out);
            }
        }
        Expr::Lambda { body, .. } => expr_assigned_roots(body, out),
        Expr::Call { args, .. }
        | Expr::ArrayLiteral { elements: args }
        | Expr::EnumConstruct { fields: args, .. } => {
            for x in args {
                expr_assigned_roots(x, out);
            }
        }
        Expr::CallExpr { callee, args } => {
            expr_assigned_roots(callee, out);
            for x in args {
                expr_assigned_roots(x, out);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            expr_assigned_roots(lhs, out);
            expr_assigned_roots(rhs, out);
        }
        Expr::Unary { expr, .. } | Expr::Cast { expr, .. } => expr_assigned_roots(expr, out),
        Expr::Try(expr) | Expr::Assume(expr) | Expr::Assert(expr) => expr_assigned_roots(expr, out),
        Expr::Tainted { inner, .. } | Expr::Declassify { inner, .. } => {
            expr_assigned_roots(inner, out)
        }
        Expr::Index { base, index } => {
            expr_assigned_roots(base, out);
            expr_assigned_roots(index, out);
        }
        Expr::FieldAccess { base, .. } => expr_assigned_roots(base, out),
        Expr::StructLiteral { fields, .. } => {
            for (_, x) in fields {
                expr_assigned_roots(x, out);
            }
        }
        Expr::MapLiteral { entries, .. } => {
            for (k, v) in entries {
                expr_assigned_roots(k, out);
                expr_assigned_roots(v, out);
            }
        }
        _ => {}
    }
}

/// After a scope that MAY NOT fully execute — a loop body that can run zero times, or an `if` branch
/// that may not be taken — the variables it writes are UNCERTAIN. Restore the pre-scope assumptions
/// (discarding whatever facts the scope accumulated) and drop the fact + modelability of every
/// variable the scope writes. Without this, a fact asserted on a conditional path (e.g. `x = 5` inside
/// `while i < n { … }` or `if c { … }`) would leak out as an UNCONDITIONAL fact, letting `check`
/// certify an `ensures`/`assert` that is false when the path does not run.
fn drop_written_after_scope(
    ctx: &mut SemanticContext,
    assumptions: &mut Vec<String>,
    snapshot: Vec<String>,
    bodies: &[&[Stmt]],
) {
    *assumptions = snapshot;
    let mut written = BTreeSet::new();
    for b in bodies {
        collect_assigned_roots(b, &mut written);
    }
    let wm: BTreeSet<String> = written.iter().map(|v| smt_var(v)).collect();
    assumptions.retain(|a| {
        let mut vs = BTreeSet::new();
        collect_vars_from_smt(a, &mut vs);
        vs.is_disjoint(&wm)
    });
    for v in &written {
        ctx.solver_int_vars.remove(v);
    }
}

/// Havoc (invalidate) every variable a loop body writes BEFORE the body is analyzed: drop it from the
/// modelable set and remove its stale pre-loop fact. Without this, an obligation INSIDE the loop body
/// (e.g. `assert(x < 2)` before `x = x + 1`) is discharged against the pre-loop value of a variable
/// the loop mutates every iteration — a false proof that `check` accepts but the runtime panics on.
/// After havoc, an in-body assertion over a loop-written variable is left to the runtime (which does
/// enforce `assert`), rather than "proved" from a value the loop has moved past.
fn havoc_loop_written(ctx: &mut SemanticContext, assumptions: &mut Vec<String>, body: &[Stmt]) {
    let mut written = BTreeSet::new();
    collect_assigned_roots(body, &mut written);
    let mangled: BTreeSet<String> = written.iter().map(|v| smt_var(v)).collect();
    for v in &written {
        ctx.solver_int_vars.remove(v);
    }
    assumptions.retain(|a| {
        let mut vs = BTreeSet::new();
        collect_vars_from_smt(a, &mut vs);
        vs.is_disjoint(&mangled)
    });
}

/// True when statement `s` can break/continue/return OUT of the enclosing loop being analyzed. A
/// `break`/`continue` nested in an `if` still targets THIS loop (so it counts), but one inside a
/// NESTED loop targets the inner loop (so it does not) — only a `return` inside a nested loop escapes
/// further. Any such escape makes the per-iteration transition (and the post-loop `¬cond` assumption)
/// unsound, because the loop can exit while its condition is still true.
fn stmt_escapes_loop(s: &Stmt) -> bool {
    match s {
        Stmt::Break | Stmt::Continue => true,
        Stmt::ExprStmt(Expr::Call { callee, .. }) if callee == "return" => true,
        Stmt::If { then, else_, .. } => {
            then.iter().any(stmt_escapes_loop)
                || else_
                    .as_ref()
                    .is_some_and(|e| e.iter().any(stmt_escapes_loop))
        }
        Stmt::While { body, .. }
        | Stmt::Loop { body, .. }
        | Stmt::For { body, .. }
        | Stmt::WhileLet { body, .. } => body.iter().any(stmt_contains_return),
        _ => false,
    }
}

/// True when statement `s` contains a `return` at any depth (a return escapes the whole function, so
/// even one inside a nested loop invalidates the outer loop's straight-line transition).
fn stmt_contains_return(s: &Stmt) -> bool {
    match s {
        Stmt::ExprStmt(Expr::Call { callee, .. }) if callee == "return" => true,
        Stmt::If { then, else_, .. } => {
            then.iter().any(stmt_contains_return)
                || else_
                    .as_ref()
                    .is_some_and(|e| e.iter().any(stmt_contains_return))
        }
        Stmt::While { body, .. }
        | Stmt::Loop { body, .. }
        | Stmt::For { body, .. }
        | Stmt::WhileLet { body, .. } => body.iter().any(stmt_contains_return),
        _ => false,
    }
}

/// Extract the straight-line transition a loop body applies to integer variables: a map from each
/// reassigned variable to its post-iteration value as an expression of the pre-iteration values.
/// Returns None when the body updates a variable in a way the checker cannot model soundly — a
/// branch/nested-loop write to a tracked variable, a non-modelable right-hand side, or any
/// break/continue/return — so the invariant cannot be verified inductively and the loop is rejected.
fn extract_loop_transition(
    body: &[Stmt],
    tracked: &BTreeSet<String>,
    model_vars: &BTreeSet<String>,
) -> Option<BTreeMap<String, Expr>> {
    // A break/continue/return anywhere in the body (including nested inside an `if`) can exit the
    // loop while its condition is still true, so the post-loop `¬cond` assumption would be unsound.
    if body.iter().any(stmt_escapes_loop) {
        return None;
    }
    // The body must be a FLAT, SIMPLE straight-line sequence: only plain `x = <simple expr>`
    // assignments, `let` bindings with a simple initializer, and neutral expression statements
    // (print/assert with no embedded assignment). A branch (`if`), a nested loop, a `match`, an
    // index/field-place assignment, or ANY expression that embeds a block/`if`/`match` (which could
    // hide a conditional write, e.g. `let z = if c { x = x + 1; 0 }`) is not soundly modelable as a
    // single unconditional transition — reject. This flat-body rule is the robust guard: it does not
    // depend on chasing writes through every expression form.
    let mut sub: BTreeMap<String, Expr> = BTreeMap::new();
    for st in body {
        match st {
            Stmt::Assign {
                target: Expr::Var(v),
                value,
            } => {
                if !expr_is_simple(value) {
                    return None;
                }
                let concrete = substitute_vars(value, &sub);
                // A non-modelable update to a TRACKED variable defeats verification. A non-modelable
                // update to an auxiliary is allowed here, but its stale fact is dropped by the
                // caller's `written`/frame handling so it cannot be read as a frozen constant.
                if tracked.contains(v) && !is_int_modelable(&concrete, model_vars) {
                    return None;
                }
                if is_int_modelable(&concrete, model_vars) {
                    sub.insert(v.clone(), concrete);
                } else {
                    sub.remove(v);
                }
            }
            // A `let` with a simple initializer is a loop-local binding, neutral for the transition —
            // UNLESS it SHADOWS a modeled variable. A shadowing `let y = 5` would make a later in-body
            // read of `y` resolve, in the model, to the outer symbolic (carrying its stale pre-loop
            // fact `y == 0`) while the runtime uses the shadow (5), certifying a false invariant.
            // Reject such a shadow rather than mis-model it (a fresh, non-shadowing name is fine).
            Stmt::Let { name, init, .. } => {
                if !expr_is_simple(init) || model_vars.contains(name) {
                    return None;
                }
            }
            Stmt::LetPattern { pattern, init, .. } => {
                if !expr_is_simple(init)
                    || pattern.bound_names().iter().any(|n| model_vars.contains(n))
                {
                    return None;
                }
            }
            // A print/assert/assume with a simple argument cannot mutate an integer binding (Anubis
            // is by-value); a call whose argument embeds a block could, so require simplicity.
            Stmt::ExprStmt(e) => {
                if !expr_is_simple(e) {
                    return None;
                }
            }
            // Anything else — a branch, a nested loop, a `match` statement, an index/field-place
            // assignment, a bare break/continue — is not a flat straight-line statement.
            _ => return None,
        }
    }
    Some(sub)
}

/// Verify a `while` loop's invariants by the Hoare rule and, on success, return the assumptions to
/// admit AFTER the loop (each invariant, plus the negated condition) together with the loop-carried
/// variables to re-model. Emits base-case and preservation obligations for the solver to discharge;
/// rejects (fail-closed) when the invariant or loop cannot be modeled inductively.
fn verify_while_invariants(
    ctx: &mut SemanticContext,
    cond: &Expr,
    invariants: &[Expr],
    body: &[Stmt],
    outer_assumptions: &[String],
) -> Option<(Vec<String>, Vec<String>, Vec<String>)> {
    let reject = |ctx: &mut SemanticContext, why: &str| {
        ctx.diagnostics.push(SemanticDiagnostic {
            code: Some("ANUBIS_LOOP_INVARIANT_UNVERIFIABLE".into()),
            message: format!(
                "cannot verify this loop invariant inductively: {why}. Invariants are supported on \
                 `while` loops whose body is straight-line integer assignments (no branch/nested-loop \
                 write to a loop-carried variable, no break/continue/return); state the invariant \
                 over integer variables the solver can model"
            ),
            span: None,
        });
    };

    // The loop-carried variables the invariant / condition constrain.
    let mut tracked = BTreeSet::new();
    collect_expr_vars(cond, &mut tracked);
    for inv in invariants {
        collect_expr_vars(inv, &mut tracked);
    }
    // Model those variables as fresh 64-bit symbolics for the inductive step.
    let mut model_vars = ctx.solver_int_vars.clone();
    for v in &tracked {
        model_vars.insert(v.clone());
        ctx.symbolic_widths.entry(v.clone()).or_insert(64);
    }

    // The condition and every invariant must be modelable, else induction is impossible.
    if !is_bool_modelable(cond, &model_vars) {
        reject(
            ctx,
            "the loop condition is not an integer formula the solver can model",
        );
        return None;
    }
    for inv in invariants {
        if !is_bool_modelable(inv, &model_vars) {
            reject(
                ctx,
                "an invariant is not an integer formula the solver can model",
            );
            return None;
        }
    }

    let push_ob = |ctx: &mut SemanticContext, name: String, asm: Vec<String>, assertion: String| {
        let mut vars = BTreeSet::new();
        collect_vars_from_smt(&assertion, &mut vars);
        for a in &asm {
            collect_vars_from_smt(a, &mut vars);
        }
        ctx.solver_obligations.push(SolverObligation {
            name,
            assumptions: asm,
            assertion,
            vars: vars.into_iter().collect(),
        });
    };

    // BASE CASE: on entry, the pre-loop state implies each invariant.
    for inv in invariants {
        let smt = expr_to_smt(inv, &ctx.symbolic_widths);
        push_ob(
            ctx,
            format!("loop-invariant-base:{smt}"),
            outer_assumptions.to_vec(),
            smt,
        );
    }

    // TRANSITION: the straight-line effect of one iteration on the tracked variables.
    let transition = match extract_loop_transition(body, &tracked, &model_vars) {
        Some(t) => t,
        None => {
            reject(
                ctx,
                "the loop body is not straight-line integer assignments",
            );
            return None;
        }
    };

    // PRESERVATION: assuming the invariants, the loop condition, and the loop's FRAME, each invariant
    // still holds after one iteration. The WRITTEN variables (every variable the body assigns at any
    // depth — including an auxiliary the transition does not capture, e.g. one written in a branch or
    // via a non-modelable RHS) are fresh symbolic: their concrete pre-loop values are stale and must
    // be dropped. Only an outer fact about a variable the loop NEVER writes (e.g. `requires(n < 100)`
    // while the loop touches `i`/`total`) holds every iteration and stays in scope — without it a
    // bound like `total <= n` could not be shown overflow-free.
    let mut written: BTreeSet<String> = BTreeSet::new();
    collect_assigned_roots(body, &mut written);
    let written_mangled: BTreeSet<String> = written.iter().map(|v| smt_var(v)).collect();
    let frame: Vec<String> = outer_assumptions
        .iter()
        .filter(|a| {
            let mut vs = BTreeSet::new();
            collect_vars_from_smt(a, &mut vs);
            vs.is_disjoint(&written_mangled)
        })
        .cloned()
        .collect();
    let cond_smt = expr_to_smt(cond, &ctx.symbolic_widths);
    let inv_smts: Vec<String> = invariants
        .iter()
        .map(|i| expr_to_smt(i, &ctx.symbolic_widths))
        .collect();
    let mut step_assumptions = inv_smts.clone();
    step_assumptions.push(cond_smt.clone());
    step_assumptions.extend(frame);
    for inv in invariants {
        let stepped = substitute_vars(inv, &transition);
        if !is_bool_modelable(&stepped, &model_vars) {
            reject(
                ctx,
                "an invariant is not modelable after the loop body's update",
            );
            return None;
        }
        let smt = expr_to_smt(&stepped, &ctx.symbolic_widths);
        push_ob(
            ctx,
            format!("loop-invariant-step:{smt}"),
            step_assumptions.clone(),
            smt,
        );
    }

    // SUCCESS: after the loop the invariants hold and the loop has exited (¬cond). Return (1) EVERY
    // written variable — a stale pre-loop fact about any of them (even an auxiliary) must be dropped
    // after the loop, while a fact about an unwritten variable (e.g. `n < 1000`) stays true — and
    // (2) the tracked variables to re-model so the post-loop invariant assumptions are usable.
    let mut post = inv_smts;
    post.push(format!("(not {cond_smt})"));
    let written_vars: Vec<String> = written.into_iter().collect();
    let readmit: Vec<String> = tracked.into_iter().collect();
    Some((post, written_vars, readmit))
}

/// Collect every explicit `return X` expression in a statement (recursing into nested blocks), so a
/// contract's `ensures` can be checked at every return point, not only the tail.
fn collect_returns_in_stmt(s: &Stmt, out: &mut Vec<Expr>) {
    match s {
        // A statement's expressions can hide a `return` inside a `match`-arm / `if`/block expression or
        // a `let`/assign initializer — walk them, else such a return escapes the `ensures` check.
        Stmt::ExprStmt(e) => expr_returns(e, out),
        Stmt::Let { init, .. } | Stmt::LetPattern { init, .. } => expr_returns(init, out),
        Stmt::Assign { target, value } => {
            expr_returns(target, out);
            expr_returns(value, out);
        }
        Stmt::If { cond, then, else_ } => {
            expr_returns(cond, out);
            for st in then {
                collect_returns_in_stmt(st, out);
            }
            if let Some(e) = else_ {
                for st in e {
                    collect_returns_in_stmt(st, out);
                }
            }
        }
        Stmt::While { cond, body, .. } => {
            expr_returns(cond, out);
            for st in body {
                collect_returns_in_stmt(st, out);
            }
        }
        Stmt::WhileLet { expr, body, .. } => {
            expr_returns(expr, out);
            for st in body {
                collect_returns_in_stmt(st, out);
            }
        }
        Stmt::For { source, body, .. } => {
            match source {
                crate::frontend::ForSource::Range { start, end } => {
                    expr_returns(start, out);
                    expr_returns(end, out);
                }
                crate::frontend::ForSource::Collection { expr } => expr_returns(expr, out),
            }
            for st in body {
                collect_returns_in_stmt(st, out);
            }
        }
        Stmt::Loop { body, .. }
        | Stmt::ResearchBlock { body, .. }
        | Stmt::ExploitBlock { body, .. } => {
            for st in body {
                collect_returns_in_stmt(st, out);
            }
        }
        Stmt::HybridBlock { gpu, cpu, prove } => {
            for b in [gpu, cpu, prove].into_iter().flatten() {
                for st in b {
                    collect_returns_in_stmt(st, out);
                }
            }
        }
        _ => {}
    }
}

/// Collect the values of `return X` calls hidden INSIDE an expression — a `match` arm, an `if`/block
/// expression, or any subexpression. Mirrors `expr_assigned_roots`; without it, a postcondition is not
/// checked at a return embedded in expression position (e.g. `match c { 0 => return 0, _ => 1 }`). A
/// `Lambda` body is NOT descended into — its `return` belongs to the closure, not the enclosing fn.
fn expr_returns(e: &Expr, out: &mut Vec<Expr>) {
    match e {
        Expr::Call { callee, args } if callee == "return" => {
            if let Some(first) = args.first() {
                out.push(first.clone());
            }
            for a in args {
                expr_returns(a, out);
            }
        }
        Expr::Block { stmts, tail } => {
            for st in stmts {
                collect_returns_in_stmt(st, out);
            }
            if let Some(t) = tail {
                expr_returns(t, out);
            }
        }
        Expr::If {
            cond, then, else_, ..
        } => {
            expr_returns(cond, out);
            expr_returns(then, out);
            expr_returns(else_, out);
        }
        Expr::IfLet {
            scrutinee,
            then,
            else_,
            ..
        } => {
            expr_returns(scrutinee, out);
            expr_returns(then, out);
            expr_returns(else_, out);
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            expr_returns(scrutinee, out);
            for a in arms {
                if let Some(g) = &a.guard {
                    expr_returns(g, out);
                }
                expr_returns(&a.body, out);
            }
        }
        Expr::Lambda { .. } => {}
        Expr::Call { args, .. }
        | Expr::ArrayLiteral { elements: args }
        | Expr::EnumConstruct { fields: args, .. } => {
            for x in args {
                expr_returns(x, out);
            }
        }
        Expr::CallExpr { callee, args } => {
            expr_returns(callee, out);
            for x in args {
                expr_returns(x, out);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            expr_returns(lhs, out);
            expr_returns(rhs, out);
        }
        Expr::Unary { expr, .. } | Expr::Cast { expr, .. } => expr_returns(expr, out),
        Expr::Try(expr) | Expr::Assume(expr) | Expr::Assert(expr) => expr_returns(expr, out),
        Expr::Tainted { inner, .. } | Expr::Declassify { inner, .. } => expr_returns(inner, out),
        Expr::Index { base, index } => {
            expr_returns(base, out);
            expr_returns(index, out);
        }
        Expr::FieldAccess { base, .. } => expr_returns(base, out),
        Expr::StructLiteral { fields, .. } => {
            for (_, x) in fields {
                expr_returns(x, out);
            }
        }
        Expr::MapLiteral { entries, .. } => {
            for (k, v) in entries {
                expr_returns(k, out);
                expr_returns(v, out);
            }
        }
        _ => {}
    }
}

/// Whether a function body uses the `?` operator, NOT descending into nested lambdas (a `?` inside a
/// closure early-returns from the closure, not this function).
fn body_contains_try(body: &[Stmt]) -> bool {
    body.iter().any(stmt_contains_try)
}

fn stmt_contains_try(s: &Stmt) -> bool {
    match s {
        Stmt::Let { init, .. } | Stmt::LetPattern { init, .. } => expr_contains_try(init),
        Stmt::Assign { target, value } => expr_contains_try(target) || expr_contains_try(value),
        Stmt::If { cond, then, else_ } => {
            expr_contains_try(cond)
                || then.iter().any(stmt_contains_try)
                || else_
                    .as_ref()
                    .is_some_and(|e| e.iter().any(stmt_contains_try))
        }
        Stmt::While { cond, body, .. } => {
            expr_contains_try(cond) || body.iter().any(stmt_contains_try)
        }
        Stmt::WhileLet { expr, body, .. } => {
            expr_contains_try(expr) || body.iter().any(stmt_contains_try)
        }
        Stmt::For { source, body, .. } => {
            let in_source = match source {
                crate::frontend::ForSource::Range { start, end } => {
                    expr_contains_try(start) || expr_contains_try(end)
                }
                crate::frontend::ForSource::Collection { expr } => expr_contains_try(expr),
            };
            in_source || body.iter().any(stmt_contains_try)
        }
        Stmt::Loop { body, .. }
        | Stmt::ResearchBlock { body, .. }
        | Stmt::ExploitBlock { body, .. } => body.iter().any(stmt_contains_try),
        Stmt::HybridBlock { gpu, cpu, prove } => [gpu, cpu, prove]
            .into_iter()
            .flatten()
            .any(|b| b.iter().any(stmt_contains_try)),
        Stmt::ExprStmt(e) => expr_contains_try(e),
        Stmt::Break | Stmt::Continue | Stmt::SpecBlock { .. } => false,
    }
}

fn expr_contains_try(e: &Expr) -> bool {
    match e {
        Expr::Try(_) => true,
        Expr::Lambda { .. } => false, // a `?` in a nested closure belongs to the closure
        Expr::Binary { lhs, rhs, .. } => expr_contains_try(lhs) || expr_contains_try(rhs),
        Expr::Unary { expr, .. } | Expr::Cast { expr, .. } => expr_contains_try(expr),
        Expr::Call { args, .. } | Expr::ArrayLiteral { elements: args } => {
            args.iter().any(expr_contains_try)
        }
        Expr::CallExpr { callee, args } => {
            expr_contains_try(callee) || args.iter().any(expr_contains_try)
        }
        Expr::EnumConstruct { fields, .. } => fields.iter().any(expr_contains_try),
        Expr::Index { base, index } => expr_contains_try(base) || expr_contains_try(index),
        Expr::FieldAccess { base, .. } => expr_contains_try(base),
        Expr::Tainted { inner, .. } | Expr::Declassify { inner, .. } => expr_contains_try(inner),
        Expr::Assume(x) | Expr::Assert(x) => expr_contains_try(x),
        Expr::StructLiteral { fields, .. } => fields.iter().any(|(_, v)| expr_contains_try(v)),
        Expr::Match {
            scrutinee, arms, ..
        } => {
            expr_contains_try(scrutinee)
                || arms.iter().any(|a| {
                    a.guard.as_ref().is_some_and(expr_contains_try) || expr_contains_try(&a.body)
                })
        }
        Expr::If {
            cond, then, else_, ..
        } => expr_contains_try(cond) || expr_contains_try(then) || expr_contains_try(else_),
        Expr::IfLet {
            scrutinee,
            then,
            else_,
            ..
        } => expr_contains_try(scrutinee) || expr_contains_try(then) || expr_contains_try(else_),
        Expr::MapLiteral { entries, .. } => entries
            .iter()
            .any(|(k, v)| expr_contains_try(k) || expr_contains_try(v)),
        Expr::Block { stmts, tail } => {
            stmts.iter().any(stmt_contains_try)
                || tail.as_ref().is_some_and(|t| expr_contains_try(t))
        }
        _ => false,
    }
}

/// Check a function's declared `-> rty` against the value it returns, but only where that value is
/// a LITERAL of a statically-known type (so a dynamic return is never falsely rejected). Covers the
/// implicit tail expression and top-level explicit `return X;` (which parses as a `return(...)` call).
fn check_return_types(
    body: &[Stmt],
    rty: &str,
    scope: &BTreeMap<String, ScopeBinding>,
    span: Span,
    ctx: &mut SemanticContext,
) {
    // Implicit-return tail: the last statement when it is a bare value expression.
    if let Some(Stmt::ExprStmt(e)) = body.last() {
        check_one_return(e, rty, scope, span, ctx);
    }
    // Explicit `return X;` at the top level (deeper returns are left dynamic — conservative).
    for st in body {
        if let Stmt::ExprStmt(Expr::Call { callee, args }) = st {
            if callee == "return" {
                if let Some(v) = args.first() {
                    check_one_return(v, rty, scope, span, ctx);
                }
            }
        }
    }
}

fn check_one_return(
    expr: &Expr,
    rty: &str,
    scope: &BTreeMap<String, ScopeBinding>,
    span: Span,
    ctx: &mut SemanticContext,
) {
    // Only a CONSTANT has a reliable, stable static type; anything dynamic (variable, call, if/match
    // over variables, a trailing statement that yields the default 0) is left unchecked. This also
    // catches `return 5 as u32` from a `-> string` fn — a cast constant the checker trusts elsewhere.
    if !is_constant_expr(expr) {
        return;
    }
    if let Some(actual) = infer_expr_type_scoped(expr, scope) {
        if !types_assignable(rty, &actual) {
            ctx.diagnostics.push(SemanticDiagnostic {
                code: Some("ANUBIS_RETURN_TYPE_MISMATCH".into()),
                message: format!(
                    "function declared `-> {}` but returns a value of type `{}`",
                    rty, actual
                ),
                span: Some((span.start, span.end)),
            });
        }
    }
}

fn infer_expr_type_scoped(expr: &Expr, scope: &BTreeMap<String, ScopeBinding>) -> Option<String> {
    match expr {
        Expr::Symbolic { ty } => Some(ty.clone()),
        Expr::Tainted { ty, .. } => Some(format!("tainted<{}>", ty)),
        Expr::UnifiedBuffer { ty } => Some(format!("unified Buffer<{}>", ty)),
        Expr::RawPtr { mutable } => Some(if *mutable {
            "*mut unknown".into()
        } else {
            "*const unknown".into()
        }),
        Expr::Declassify { inner, .. } => infer_expr_type_scoped(inner, scope),
        Expr::TaintSource { .. } => Some("tainted<string>".into()),
        Expr::Literal(s) if s == "true" || s == "false" => Some("bool".into()),
        // Integer literal (i64, or a u64 bit-pattern for magnitudes above i64::MAX) → the
        // width-polymorphic integer default. A literal that is ONLY f64-parseable (`3.14`, `1e9`)
        // is a FLOAT: typing it as an integer would let the solver model it as an i64 bit-vector
        // (unsound — it "proved" `2*x != 1` for x = 0.5). Mirrors the runtime discrimination in
        // `literal_to_anubis_value`, so a float is kept out of the solver's integer domain.
        Expr::Literal(s) if s.parse::<i64>().is_ok() || s.parse::<u64>().is_ok() => {
            Some("u32".into())
        }
        Expr::Literal(s) if s.parse::<f64>().is_ok() => Some("f64".into()),
        Expr::Literal(s) if s.starts_with('"') || s.starts_with('\'') => Some("string".into()),
        Expr::StrLiteral(_) => Some("string".into()),
        Expr::Var(name) => scope.get(name).and_then(|b| b.info.ty.clone()),
        Expr::Unary { op, expr } if op == "!" => Some("bool".into()),
        // Bitwise-not is integer at runtime (anubis_bnot `as_i64()`s and returns `Int`); unary `-`
        // (anubis_neg) is float iff its operand is float, so it propagates.
        Expr::Unary { op, .. } if op == "~" => Some("u32".into()),
        Expr::Unary { expr, .. } => infer_expr_type_scoped(expr, scope),
        Expr::Binary { op, lhs, rhs } => {
            if matches!(
                op.as_str(),
                "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||"
            ) {
                Some("bool".into())
            } else if op == "+" {
                // `+` is overloaded (anubis_add): string concat if EITHER operand is a string, list
                // concat if either is a list, otherwise numeric. Inferring the result from the lhs
                // alone wrongly typed `1 + "a"` as a number (accepting it into a u32 slot) and
                // `404 + ": x"` as a number (rejecting it from a string slot).
                let lt = infer_expr_type_scoped(lhs, scope).map(|t| normalize_ty(&t));
                let rt = infer_expr_type_scoped(rhs, scope).map(|t| normalize_ty(&t));
                if lt.as_deref() == Some("string") || rt.as_deref() == Some("string") {
                    Some("string".into())
                } else if lt.as_deref() == Some("list") || rt.as_deref() == Some("list") {
                    Some("list".into())
                } else {
                    lt.or(rt)
                }
            } else if matches!(op.as_str(), "&" | "|" | "^" | "<<" | ">>") {
                // Bitwise/shift are INTEGER at runtime regardless of operands: anubis_band/bor/bxor/
                // shl/shr (run.rs) `as_i64()` both operands and unconditionally return `Int`. So
                // `avg & 7` is an integer even when `avg` is a float — inferring it from the float
                // operand (the arithmetic `else` below) wrongly typed it f64 and made the float→int
                // narrowing rule reject a program that runs and yields an integer.
                Some("u32".into())
            } else {
                // Arithmetic (`- * / %`): float iff an operand is float (anubis_sub/mul/div/mod),
                // so propagating the operand type is faithful to the runtime.
                infer_expr_type_scoped(lhs, scope).or_else(|| infer_expr_type_scoped(rhs, scope))
            }
        }
        Expr::ArrayLiteral { .. } => Some("list".into()),
        Expr::MapLiteral { .. } => Some("map".into()),
        Expr::EnumConstruct { enum_name, .. } => Some(enum_name.clone()),
        Expr::If { then, else_, .. } => value_branch_type(&[
            infer_expr_type_scoped(then, scope),
            infer_expr_type_scoped(else_, scope),
        ]),
        Expr::Match { arms, .. } => value_branch_type(
            &arms
                .iter()
                .map(|a| infer_expr_type_scoped(&a.body, scope))
                .collect::<Vec<_>>(),
        ),
        // A block used as a value (e.g. an `if`/`match` branch `{ let a = 3.14; a }`) has the type
        // of its trailing expression; a statement block with no tail has no value. Without this,
        // block-wrapped branches inferred `None`, letting an all-float nested `if` escape the
        // float→int narrowing rule.
        Expr::Block { tail, .. } => tail.as_ref().and_then(|t| infer_expr_type_scoped(t, scope)),
        Expr::Index { .. } => None, // dynamic
        Expr::FieldAccess { .. } => None,
        Expr::Call { .. } => None,
        Expr::Cast { ty, .. } => Some(ty.clone()),
        _ => None,
    }
}

/// A CONSTANT expression — one built solely from literals and operators, with NO variables, calls,
/// or index/field accesses. Its type is intrinsic and immutable, so B1 can act on it soundly. B1
/// only acts on constants: a variable's type is NOT stable in a language with `let mut` rebinding
/// (a `let mut v = 0` reassigned from a dynamic value keeps its stale numeric type), so trusting a
/// variable's inferred type would produce false positives. This widens the earlier bare-literal
/// gate so nested-but-still-constant errors like `(2 + 3)[0]` and `("a" + "b") - 1` are caught.
fn is_constant_expr(e: &Expr) -> bool {
    match e {
        Expr::Literal(_) | Expr::StrLiteral(_) => true,
        Expr::ArrayLiteral { elements } => elements.iter().all(is_constant_expr),
        Expr::MapLiteral { entries, .. } => entries
            .iter()
            .all(|(k, v)| is_constant_expr(k) && is_constant_expr(v)),
        Expr::Binary { lhs, rhs, .. } => is_constant_expr(lhs) && is_constant_expr(rhs),
        Expr::Unary { expr, .. } => is_constant_expr(expr),
        Expr::If {
            cond, then, else_, ..
        } => is_constant_expr(cond) && is_constant_expr(then) && is_constant_expr(else_),
        Expr::Cast { expr, .. } => is_constant_expr(expr),
        _ => false, // Var, Call, Index, FieldAccess, Match, … are dynamic
    }
}

/// B1 static type checking. A statically-known string/list/map CONSTANT is never a valid arithmetic
/// operand (a number/bool constant is fine — bool 0/1 arithmetic is idiomatic). A non-constant
/// (variable, call, index) returns None and is left untouched — zero false positives.
fn static_non_numeric_operand(
    expr: &Expr,
    scope: &BTreeMap<String, ScopeBinding>,
) -> Option<String> {
    if !is_constant_expr(expr) {
        return None;
    }
    let n = normalize_ty(&infer_expr_type_scoped(expr, scope)?);
    matches!(n.as_str(), "string" | "list" | "map").then_some(n)
}

/// B1: a statically-known non-indexable CONSTANT (a number or bool). Only lists/strings/maps/structs
/// are indexable. A non-constant base returns None (dynamic bases stay fail-closed at runtime).
fn static_non_indexable(expr: &Expr, scope: &BTreeMap<String, ScopeBinding>) -> Option<String> {
    if !is_constant_expr(expr) {
        return None;
    }
    let n = normalize_ty(&infer_expr_type_scoped(expr, scope)?);
    (is_numeric_ty(&n) || n == "bool").then_some(n)
}

// The type-reasoning predicates now live in `middle/ty.rs` (the single source of truth and the
// foundation for the structured `Ty` enum). These are thin shims delegating to it; behavior is
// pinned identical by the `ty_parity` test below.
fn normalize_ty(ty: &str) -> String {
    ty::normalize(ty)
}

fn is_numeric_ty(ty: &str) -> bool {
    ty::is_numeric(ty)
}

fn is_integer_ty(ty: &str) -> bool {
    ty::is_integer(ty)
}

fn cast_preserves_i64(ty: &str) -> bool {
    ty::cast_preserves_i64(ty)
}

/// Mangle an Anubis identifier into an SMT symbol that can never collide with an SMT-LIB keyword or
/// a `bv…` literal/operator. Without this, a parameter named `model`, `set`, `check`, or `bvx` was
/// dropped by `collect_vars_from_smt` (it looked like a keyword), left undeclared, and z3 returned a
/// parse error that `check` treated as "not a disproof" — a fail-OPEN hole. Every variable emitted
/// into SMT goes through here so declaration, emission, and collection agree.
fn smt_var(name: &str) -> String {
    format!("anb_{}", name)
}

/// Directional assignability for binding contexts (let-init, assignment to an annotated variable,
/// call arguments, returns): `ty::compatible` (numeric widths interoperate; bool/string/enums do
/// not cross; `tainted<T>` is a qualifier still policed by the taint analysis), refined with the
/// one directional Phase-2 rule — a float value may not narrow into an integer annotation. Used
/// only where a value flows INTO a declared type; the `if`/`match` arm-type join during inference
/// is a symmetric context and uses `value_branch_type` instead. See `ty::assignable`.
fn types_assignable(expected: &str, actual: &str) -> bool {
    ty::assignable(expected, actual)
}

/// The inferred type of an `if`/`match` used as a value. The runtime takes exactly ONE branch, so
/// the value is *definitely* float only when EVERY branch is a known float — order-independently.
/// This drives the float→int narrowing rule, so it must be exact in both directions:
///
/// - every branch a known float → that float type (a real `let x: u32 = if c { 3.14 } else { 2.71 }`
///   lie is caught regardless of branch order or block nesting);
/// - a float branch mixed with a definite non-float, OR with an unseeable (`None`) branch → NOT
///   definitely float (the taken branch may be the non-float one), so never report a float: prefer
///   a known non-float type (keeps numeric-into-string/bool mismatches catchable), else `None`.
///   This is what stops the Round-1 false positive `if c { 3.14 } else { 5 }` from being rejected;
/// - no float branch → the first known branch's type (ordinary, historical inference — also
///   restores the old `(None, Some(a)) => Some(a)` fallback that a first-`None` branch needs).
///
/// A branch whose type the checker cannot see (`None` — e.g. a call/index result) makes the value
/// not-definitely-float and is therefore NOT narrowed; that residual float→int case is a documented
/// completeness gap, not a soundness hole (the solver still fails closed on such a value).
fn value_branch_type(branches: &[Option<String>]) -> Option<String> {
    let known: Vec<&String> = branches.iter().flatten().collect();
    let first_known = (*known.first()?).clone();
    let any_float = known.iter().any(|t| ty::is_float(t));
    if !any_float {
        return Some(first_known);
    }
    let all_known = known.len() == branches.len();
    let any_nonfloat = known.iter().any(|t| !ty::is_float(t));
    if all_known && !any_nonfloat {
        return Some(first_known); // every branch a known float → definitely float
    }
    known
        .iter()
        .find(|t| !ty::is_float(t))
        .map(|t| (*t).clone())
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
                            if !types_assignable(expected, &got) {
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
            } else if let Some(arity) = scope.get(callee).and_then(|b| b.closure_arity) {
                // Direct call of a closure-valued local (`let f = |x, y| …; f(1)`): arity-check it.
                // Higher-order use (`map(xs, f)`) is an internal call, not a source `f(args)`, so it
                // still pads — matching the strict-direct / pad-higher-order arity policy.
                if args.len() != arity {
                    ctx.diagnostics.push(SemanticDiagnostic {
                        code: Some("ANUBIS_ARITY_MISMATCH".into()),
                        message: format!(
                            "closure `{}` expects {} argument(s), got {}",
                            callee,
                            arity,
                            args.len()
                        ),
                        span: None,
                    });
                }
            }
            for a in args {
                check_expr_semantics(a, scope, ctx);
            }
        }
        Expr::Binary { op, lhs, rhs } => {
            check_expr_semantics(lhs, scope, ctx);
            check_expr_semantics(rhs, scope, ctx);
            // B1: arithmetic/bitwise operators require numeric operands. `+` is overloaded
            // (string/list concat), comparisons and `&&`/`||` are lenient, so only these numeric-only
            // operators are checked — and only against a statically-known string/list/map operand.
            if matches!(
                op.as_str(),
                "-" | "*" | "/" | "%" | "&" | "|" | "^" | "<<" | ">>"
            ) {
                for operand in [lhs.as_ref(), rhs.as_ref()] {
                    if let Some(bad) = static_non_numeric_operand(operand, scope) {
                        ctx.diagnostics.push(SemanticDiagnostic {
                            code: Some("ANUBIS_TYPE_MISMATCH".into()),
                            message: format!(
                                "operator `{}` requires numeric operands, but an operand has type `{}`",
                                op, bad
                            ),
                            span: None,
                        });
                        break;
                    }
                }
            }
        }
        Expr::Unary { op, expr } => {
            check_expr_semantics(expr, scope, ctx);
            // B1: unary `-` requires a numeric operand.
            if op == "-" {
                if let Some(bad) = static_non_numeric_operand(expr, scope) {
                    ctx.diagnostics.push(SemanticDiagnostic {
                        code: Some("ANUBIS_TYPE_MISMATCH".into()),
                        message: format!("unary `-` requires a numeric operand, got `{}`", bad),
                        span: None,
                    });
                }
            }
        }
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
            // B1: only lists, strings, maps, and structs are indexable. A statically-known numeric
            // or bool base is a type error (dynamic bases are left to the fail-closed runtime).
            if let Some(bad) = static_non_indexable(base, scope) {
                ctx.diagnostics.push(SemanticDiagnostic {
                    code: Some("ANUBIS_TYPE_MISMATCH".into()),
                    message: format!(
                        "cannot index a value of type `{}` (only lists, strings, and maps are indexable)",
                        bad
                    ),
                    span: None,
                });
            }
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
        Expr::CallExpr { callee, args } => {
            check_expr_semantics(callee, scope, ctx);
            for a in args {
                check_expr_semantics(a, scope, ctx);
            }
            // Direct method call `recv.method(args)`: arity-check when the method name resolves to a
            // single known arity. `self` is the receiver, so `args.len() + 1` must equal that arity.
            if let Expr::FieldAccess { field, .. } = &**callee {
                if let Some(Some(arity)) = ctx.method_arities.get(field).copied() {
                    if args.len() + 1 != arity {
                        ctx.diagnostics.push(SemanticDiagnostic {
                            code: Some("ANUBIS_ARITY_MISMATCH".into()),
                            message: format!(
                                "method `{}` expects {} argument(s), got {}",
                                field,
                                arity.saturating_sub(1),
                                args.len()
                            ),
                            span: None,
                        });
                    }
                }
            }
        }
        Expr::Declassify { inner, .. } => check_expr_semantics(inner, scope, ctx),
        Expr::Cast { expr, .. } => check_expr_semantics(expr, scope, ctx),
        Expr::StructLiteral { name, fields, .. } => {
            let mut seen = BTreeSet::new();
            for (fname, fexpr) in fields {
                if !seen.insert(fname.clone()) {
                    ctx.diagnostics.push(SemanticDiagnostic {
                        code: Some("ANUBIS_DUPLICATE_FIELD".into()),
                        message: format!(
                            "duplicate field `{}` in `{}` struct literal",
                            fname, name
                        ),
                        span: None,
                    });
                }
                check_expr_semantics(fexpr, scope, ctx);
            }
        }
        // Descend into closure and block bodies so B1's constant-type checks apply there too —
        // otherwise `|q| 5[0]` or `{ let z = 7[2]; z }` slipped past the checker and crashed at run.
        Expr::Lambda { body, .. } => check_expr_semantics(body, scope, ctx),
        Expr::Block { stmts, tail } => check_block_exprs(stmts, tail.as_deref(), scope, ctx),
        _ => {}
    }
}

/// Walk the expressions inside a block / closure body for B1's constant-type checks. The checks are
/// constant-only (they never flag a variable), so re-using the enclosing scope is sound.
fn check_block_exprs(
    stmts: &[Stmt],
    tail: Option<&Expr>,
    scope: &BTreeMap<String, ScopeBinding>,
    ctx: &mut SemanticContext,
) {
    for s in stmts {
        check_stmt_exprs(s, scope, ctx);
    }
    if let Some(t) = tail {
        check_expr_semantics(t, scope, ctx);
    }
}

fn check_stmt_exprs(s: &Stmt, scope: &BTreeMap<String, ScopeBinding>, ctx: &mut SemanticContext) {
    use crate::frontend::ForSource;
    match s {
        Stmt::Let { init, .. } | Stmt::LetPattern { init, .. } => {
            check_expr_semantics(init, scope, ctx)
        }
        Stmt::Assign { value, .. } => check_expr_semantics(value, scope, ctx),
        Stmt::ExprStmt(e) => check_expr_semantics(e, scope, ctx),
        Stmt::If { cond, then, else_ } => {
            check_expr_semantics(cond, scope, ctx);
            check_block_exprs(then, None, scope, ctx);
            if let Some(e) = else_ {
                check_block_exprs(e, None, scope, ctx);
            }
        }
        Stmt::While { cond, body, .. } => {
            check_expr_semantics(cond, scope, ctx);
            check_block_exprs(body, None, scope, ctx);
        }
        Stmt::WhileLet { expr, body, .. } => {
            check_expr_semantics(expr, scope, ctx);
            check_block_exprs(body, None, scope, ctx);
        }
        Stmt::Loop { body, .. } => check_block_exprs(body, None, scope, ctx),
        Stmt::For { source, body, .. } => {
            match source {
                ForSource::Range { start, end } => {
                    check_expr_semantics(start, scope, ctx);
                    check_expr_semantics(end, scope, ctx);
                }
                ForSource::Collection { expr } => check_expr_semantics(expr, scope, ctx),
            }
            check_block_exprs(body, None, scope, ctx);
        }
        Stmt::ResearchBlock { body, .. } | Stmt::ExploitBlock { body, .. } => {
            check_block_exprs(body, None, scope, ctx)
        }
        Stmt::HybridBlock { gpu, cpu, prove } => {
            for b in [gpu, cpu, prove].into_iter().flatten() {
                check_block_exprs(b, None, scope, ctx);
            }
        }
        Stmt::Break | Stmt::Continue | Stmt::SpecBlock { .. } => {}
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
        _ => infer_expr_type_scoped(scrutinee, scope).filter(|t| ctx.enum_variants.contains_key(t)),
    };
    // If the scrutinee's type is unknown, fall back to arm-based inference: if the arms cover
    // variants of a declared enum (or built-in Option/Result), that is the enum being matched.
    let enum_name = enum_name.or_else(|| {
        arms.iter().find_map(|arm| {
            let mut pairs = Vec::new();
            arm.pattern.covered_enum_variants(&mut pairs);
            pairs
                .into_iter()
                .map(|(en, _)| en)
                .find(|en| ctx.enum_variants.contains_key(en))
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

fn declassify_source(
    expr: &Expr,
    scope: &BTreeMap<String, ScopeBinding>,
    tainting_fns: &BTreeSet<String>,
    param_return_taint: &BTreeMap<String, BTreeSet<usize>>,
) -> Option<String> {
    match expr {
        Expr::Declassify {
            inner,
            policy,
            reason,
            ..
        } if policy.is_some() && reason.is_some() => {
            expr_taint_source(inner, scope, tainting_fns, param_return_taint)
        }
        _ => None,
    }
}

/// The taint-source label of an expression, or `None` if clean.
///
/// - `tainting_fns`: functions whose return carries INTERNAL taint (`sink(get_secret())`).
/// - `param_return_taint`: functions → which formal params flow to the return value. A call is
///   tainted from argument i only when i is in this set (Phase-3 A2). When the map has no entry for
///   a callee (builtins / bootstrap before the summary runs), any tainted argument conservatively
///   taints the call (fail-closed over-approx). When the map HAS an entry (even empty), only the
///   summarized params apply — so `fn ignore(x){return 5;}` no longer falsely taints `ignore(secret)`.
fn expr_taint_source(
    expr: &Expr,
    scope: &BTreeMap<String, ScopeBinding>,
    tainting_fns: &BTreeSet<String>,
    param_return_taint: &BTreeMap<String, BTreeSet<usize>>,
) -> Option<String> {
    match expr {
        Expr::Var(name) => scope
            .get(name)
            .and_then(|binding| binding.info.taint_source.clone())
            .filter(|_| scope.get(name).is_some_and(|binding| binding.info.tainted)),
        Expr::Binary { lhs, rhs, .. } => {
            expr_taint_source(lhs, scope, tainting_fns, param_return_taint)
                .or_else(|| expr_taint_source(rhs, scope, tainting_fns, param_return_taint))
        }
        Expr::Unary { expr, .. } => {
            expr_taint_source(expr, scope, tainting_fns, param_return_taint)
        }
        Expr::Call { callee, args } => {
            if tainting_fns.contains(callee) {
                Some(format!("return value of `{}`", callee))
            } else if let Some(rets) = param_return_taint.get(callee) {
                // Known user function: only params that the summary says reach the return.
                rets.iter().find_map(|&i| {
                    args.get(i)
                        .and_then(|a| expr_taint_source(a, scope, tainting_fns, param_return_taint))
                })
            } else {
                // Builtin / not-yet-summarized: any tainted argument taints the call (conservative).
                args.iter()
                    .find_map(|arg| expr_taint_source(arg, scope, tainting_fns, param_return_taint))
            }
        }
        Expr::Tainted { inner, .. } => {
            expr_taint_source(inner, scope, tainting_fns, param_return_taint)
        }
        Expr::Assume(inner) | Expr::Assert(inner) => {
            expr_taint_source(inner, scope, tainting_fns, param_return_taint)
        }
        Expr::Declassify {
            inner,
            policy,
            reason,
            ..
        } => {
            if policy.is_some() && reason.is_some() {
                None // cleared
            } else {
                expr_taint_source(inner, scope, tainting_fns, param_return_taint)
            }
        }
        Expr::TaintSource { label } => Some(label.clone()),
        // Indexing/field-access on a tainted binding must not launder the taint — without these
        // arms, `sink(tainted_arr[i])` / `sink(tainted_struct.field)` fell through to the catch-all
        // below and silently escaped `ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY` (a real fail-open gap).
        // `Index` checks both operands (like `Binary`, not `Unary`'s single-operand shape): a tainted
        // INDEX into an otherwise-clean array (`sink(arr[tainted_offset])`) is an equally real leak.
        // Whole-binding granularity only: a struct's OWN field individually declared `tainted<T>` in
        // its type definition does not, by itself, make `.field` access on an otherwise-clean instance
        // tainted — only a binding whose own `let`/param annotation (or tainted initializer) seeded it
        // tainted propagates here, matching how every other walker in this file treats field/struct
        // definitions as opaque to flow analysis.
        Expr::Index { base, index } => {
            expr_taint_source(base, scope, tainting_fns, param_return_taint)
                .or_else(|| expr_taint_source(index, scope, tainting_fns, param_return_taint))
        }
        Expr::FieldAccess { base, .. } => {
            expr_taint_source(base, scope, tainting_fns, param_return_taint)
        }
        // A cast reinterprets a value without changing its provenance — `secret as u64` is still the
        // secret. Without this arm, `sink(s as u64)` (and `return s as u64` interprocedurally)
        // laundered taint through the cast (adversary-found fail-open, both intra- and inter-procedural).
        Expr::Cast { expr, .. } => expr_taint_source(expr, scope, tainting_fns, param_return_taint),
        _ => None,
    }
}

// ── Interprocedural return-taint summary (`ctx.tainting_fns`) ────────────────────────────────────
// A monotone fixpoint pre-pass (see `compute_tainting_fns`, run in `typecheck` before per-function
// analysis) marks each function whose RETURN value carries INTERNAL taint — a `taint_source()` /
// `tainted<T>` local returned directly, or a return of another already-marked function. Consumed by
// `expr_taint_source`'s `Call` arm so `sink(get_secret())` is flagged even with no tainted argument.
// It is deliberately whole-value + reassignment-insensitive + declassify-aware, exactly matching the
// intra-procedural analysis, and MONOTONE (only grows) so it needs no control-flow-merge join.

/// Whether an expression is a `return X` (Anubis models `return` as a call to the pseudo-function
/// named `"return"`, never a real function).
fn is_return_call(e: &Expr) -> bool {
    matches!(e, Expr::Call { callee, .. } if callee == "return")
}

/// Seed one `let` binding's taint into `scope`, mirroring the real let-seeding (annotation OR a
/// tainted, non-declassified initializer). Params are never seeded here — a returned parameter is
/// arg-flow, handled at each call site; this isolates taint a function produces INTERNALLY.
fn seed_one_let(
    name: &str,
    ty: Option<&str>,
    init: &Expr,
    scope: &mut BTreeMap<String, ScopeBinding>,
    tainting_fns: &BTreeSet<String>,
    param_return_taint: &BTreeMap<String, BTreeSet<usize>>,
) {
    let explicit = is_tainted_type(ty);
    let init_taint = expr_taint_source(init, scope, tainting_fns, param_return_taint);
    let declassified = declassify_source(init, scope, tainting_fns, param_return_taint).is_some();
    let tainted = explicit || (init_taint.is_some() && !declassified);
    let taint_source = if explicit {
        Some(name.to_string())
    } else if tainted {
        init_taint
    } else {
        None
    };
    scope.insert(
        name.to_string(),
        ScopeBinding {
            info: BindingInfo {
                name: name.to_string(),
                ty: ty.map(str::to_string),
                mode: String::new(),
                tainted,
                taint_source,
                declassified,
                span: None,
            },
            closure_arity: None,
        },
    );
}

/// Whether any value a function body can RETURN carries internal taint, given the summary so far —
/// respecting LEXICAL BLOCK SCOPE. A `let` inside an `if`/loop body shadows an outer same-named
/// binding only within that block: the scope is snapshot/restored around every block, so a
/// `return x` AFTER the block sees the outer binding (an adversary found the flat version wrongly
/// marked `fn f(c){ let x=5; if c { let x=taint(); } return x; }`, which provably returns the clean
/// outer 5). `tail` marks whether this statement sequence is in the function's tail position, so a
/// bare trailing expression counts as an implicit return only when it truly is one (never a
/// mid-function side-effecting statement, nor a loop body's last statement). Declassify-before-return
/// reads clean automatically via `expr_taint_source`'s `Declassify` arm. Monotone in `tainting_fns`.
fn body_returns_taint(
    stmts: &[Stmt],
    scope: &mut BTreeMap<String, ScopeBinding>,
    tainting_fns: &BTreeSet<String>,
    param_return_taint: &BTreeMap<String, BTreeSet<usize>>,
    tail: bool,
) -> bool {
    let n = stmts.len();
    for (i, stmt) in stmts.iter().enumerate() {
        let stmt_is_tail = tail && i + 1 == n;
        match stmt {
            Stmt::Let { name, ty, init, .. } => {
                // A `return` can hide inside the initializer (a `match`/`if` arm).
                let mut rets = Vec::new();
                expr_returns(init, &mut rets);
                if rets.iter().any(|e| {
                    expr_taint_source(e, scope, tainting_fns, param_return_taint).is_some()
                }) {
                    return true;
                }
                seed_one_let(
                    name,
                    ty.as_deref(),
                    init,
                    scope,
                    tainting_fns,
                    param_return_taint,
                );
            }
            Stmt::If { then, else_, .. } => {
                // Branches inherit tail position; block-scoped `let`s must not leak past the `if`.
                let saved = scope.clone();
                if body_returns_taint(then, scope, tainting_fns, param_return_taint, stmt_is_tail) {
                    return true;
                }
                *scope = saved.clone();
                if let Some(else_body) = else_ {
                    if body_returns_taint(
                        else_body,
                        scope,
                        tainting_fns,
                        param_return_taint,
                        stmt_is_tail,
                    ) {
                        return true;
                    }
                }
                *scope = saved;
            }
            Stmt::While { body, .. }
            | Stmt::WhileLet { body, .. }
            | Stmt::Loop { body, .. }
            | Stmt::For { body, .. }
            | Stmt::ResearchBlock { body, .. }
            | Stmt::ExploitBlock { body, .. } => {
                // A loop/research body is never the function's implicit return value (tail = false);
                // only an explicit `return` inside it counts. Its `let`s are block-scoped.
                let saved = scope.clone();
                if body_returns_taint(body, scope, tainting_fns, param_return_taint, false) {
                    return true;
                }
                *scope = saved;
            }
            Stmt::HybridBlock { gpu, cpu, prove } => {
                for b in [gpu, cpu, prove].into_iter().flatten() {
                    let saved = scope.clone();
                    if body_returns_taint(b, scope, tainting_fns, param_return_taint, false) {
                        return true;
                    }
                    *scope = saved;
                }
            }
            _ => {
                // Explicit `return X` in this (non-block) statement — statement position or hidden in
                // an expression (match/if arm) — checked against the CURRENT lexical scope.
                let mut rets = Vec::new();
                collect_returns_in_stmt(stmt, &mut rets);
                if rets.iter().any(|e| {
                    expr_taint_source(e, scope, tainting_fns, param_return_taint).is_some()
                }) {
                    return true;
                }
                // Implicit tail return: a bare trailing expression in tail position. (An `if`/`match`
                // tail expression is not tracked — `expr_taint_source` has no such arm — a documented
                // boundary, so only a direct bare expr matters here.)
                if stmt_is_tail {
                    if let Stmt::ExprStmt(e) = stmt {
                        if !is_return_call(e)
                            && expr_taint_source(e, scope, tainting_fns, param_return_taint)
                                .is_some()
                        {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

/// Whether a function's return value carries internal taint (scope-aware; the function body is in
/// tail position). Params-clean scope, so this isolates internal taint from arg-flow.
fn fn_returns_taint(body: &[Stmt], tainting_fns: &BTreeSet<String>) -> bool {
    let mut scope: BTreeMap<String, ScopeBinding> = BTreeMap::new();
    let empty: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    body_returns_taint(body, &mut scope, tainting_fns, &empty, true)
}

/// Collect `(name, body)` for every free function (recursing into modules), keyed by the same flat/
/// mangled name the call `callee` and `all_fns` use. Impl methods are excluded — they are reached
/// only through `CallExpr` (receiver syntax), which the bare-name `Call` arm never matches.
fn collect_fn_bodies<'a>(items: &'a [Item], out: &mut Vec<(String, &'a [Stmt])>) {
    for item in items {
        match item {
            Item::Fn { name, body, .. } => out.push((name.clone(), body.as_slice())),
            Item::Module { items, .. } => collect_fn_bodies(items, out),
            _ => {}
        }
    }
}

/// Populate `ctx.tainting_fns` by a monotone fixpoint: repeatedly mark any not-yet-marked function
/// whose return carries taint under the current summary, until no function is added. Converges in at
/// most one round per function because the set only grows. Run once, before per-function analysis, so
/// every `Call` the analysis later sees consults a complete summary.
fn compute_tainting_fns(items: &[Item], ctx: &mut SemanticContext) {
    let mut fns: Vec<(String, &[Stmt])> = Vec::new();
    collect_fn_bodies(items, &mut fns);
    loop {
        let mut newly: Vec<String> = Vec::new();
        for (name, body) in &fns {
            if !ctx.tainting_fns.contains(name) && fn_returns_taint(body, &ctx.tainting_fns) {
                newly.push(name.clone());
            }
        }
        if newly.is_empty() {
            break;
        }
        ctx.tainting_fns.extend(newly);
    }
}

// ── Interprocedural param→sink summary (`ctx.param_sinks`) ───────────────────────────────────────
// Monotone fixpoint: for each function, which formal parameters flow to a sink without declassify
// (a builtin `is_sink`, or a call argument position another function's summary marks as sinking).
// Call sites then reject `log(tainted)` when `fn log(x){sink(x);}` — `ANUBIS_INTERPROC_SINK`.

/// Collect `(name, param_names, body)` for free functions (modules mangled the same way as calls).
fn collect_fn_params_bodies<'a>(
    items: &'a [Item],
    out: &mut Vec<(String, Vec<String>, &'a [Stmt])>,
) {
    for item in items {
        match item {
            Item::Fn {
                name, params, body, ..
            } => {
                out.push((
                    name.clone(),
                    params.iter().map(|(n, _)| n.clone()).collect(),
                    body.as_slice(),
                ));
            }
            Item::Module { items, .. } => collect_fn_params_bodies(items, out),
            _ => {}
        }
    }
}

/// Parameter indices that flow through `expr` under the current param-flow scope. Declassify clears;
/// calls pass through argument taint (and, for known sink-params of the callee, only those positions
/// matter at the *call site* — here we only need "which params are in this value").
fn expr_param_flow(expr: &Expr, flow: &BTreeMap<String, BTreeSet<usize>>) -> BTreeSet<usize> {
    match expr {
        Expr::Var(name) => flow.get(name).cloned().unwrap_or_default(),
        Expr::Binary { lhs, rhs, .. } => {
            let mut s = expr_param_flow(lhs, flow);
            s.extend(expr_param_flow(rhs, flow));
            s
        }
        Expr::Unary { expr, .. } | Expr::Cast { expr, .. } | Expr::Tainted { inner: expr, .. } => {
            expr_param_flow(expr, flow)
        }
        Expr::Assume(inner) | Expr::Assert(inner) => expr_param_flow(inner, flow),
        Expr::Declassify {
            inner,
            policy,
            reason,
            ..
        } => {
            if policy.is_some() && reason.is_some() {
                BTreeSet::new() // cleared
            } else {
                expr_param_flow(inner, flow)
            }
        }
        Expr::Call { args, .. } => {
            // Union of arg flows (conservative over-approx of the call's value). Sink detection
            // for user callees is handled separately at call sites via `known_param_sinks`.
            args.iter().fold(BTreeSet::new(), |mut acc, a| {
                acc.extend(expr_param_flow(a, flow));
                acc
            })
        }
        Expr::Index { base, index } => {
            let mut s = expr_param_flow(base, flow);
            s.extend(expr_param_flow(index, flow));
            s
        }
        Expr::FieldAccess { base, .. } => expr_param_flow(base, flow),
        _ => BTreeSet::new(),
    }
}

/// Seed a let into the param-flow map (union of init's params; declassify clears).
fn seed_param_flow_let(name: &str, init: &Expr, flow: &mut BTreeMap<String, BTreeSet<usize>>) {
    // Mirror declassify_source: a full declassify clears param provenance.
    let cleared = matches!(
        init,
        Expr::Declassify {
            policy: Some(_),
            reason: Some(_),
            ..
        }
    );
    if cleared {
        flow.insert(name.to_string(), BTreeSet::new());
    } else {
        flow.insert(name.to_string(), expr_param_flow(init, flow));
    }
}

/// Walk a body collecting parameter indices that reach a sink under the current
/// `known_param_sinks` summary. Scope-aware (snapshot/restore around blocks).
fn body_param_sinks(
    stmts: &[Stmt],
    flow: &mut BTreeMap<String, BTreeSet<usize>>,
    known_param_sinks: &BTreeMap<String, BTreeSet<usize>>,
    found: &mut BTreeSet<usize>,
) {
    for stmt in stmts {
        match stmt {
            Stmt::Let { name, init, .. } => {
                // A sink can hide inside the initializer (e.g. `let _ = sink(x)`).
                collect_param_sinks_in_expr(init, flow, known_param_sinks, found);
                seed_param_flow_let(name, init, flow);
            }
            Stmt::If {
                then, else_, cond, ..
            } => {
                collect_param_sinks_in_expr(cond, flow, known_param_sinks, found);
                let saved = flow.clone();
                body_param_sinks(then, flow, known_param_sinks, found);
                *flow = saved.clone();
                if let Some(else_body) = else_ {
                    body_param_sinks(else_body, flow, known_param_sinks, found);
                }
                *flow = saved;
            }
            Stmt::While { body, cond, .. } => {
                collect_param_sinks_in_expr(cond, flow, known_param_sinks, found);
                let saved = flow.clone();
                body_param_sinks(body, flow, known_param_sinks, found);
                *flow = saved;
            }
            Stmt::WhileLet { body, .. }
            | Stmt::Loop { body, .. }
            | Stmt::ResearchBlock { body, .. }
            | Stmt::ExploitBlock { body, .. } => {
                let saved = flow.clone();
                body_param_sinks(body, flow, known_param_sinks, found);
                *flow = saved;
            }
            Stmt::For {
                body, source, var, ..
            } => {
                let saved = flow.clone();
                // Loop var inherits collection/range param flow (conservative).
                let src_flow = match source {
                    crate::frontend::ForSource::Range { start, end } => {
                        let mut s = expr_param_flow(start, flow);
                        s.extend(expr_param_flow(end, flow));
                        s
                    }
                    crate::frontend::ForSource::Collection { expr } => expr_param_flow(expr, flow),
                };
                flow.insert(var.clone(), src_flow);
                body_param_sinks(body, flow, known_param_sinks, found);
                *flow = saved;
            }
            Stmt::HybridBlock { gpu, cpu, prove } => {
                for b in [gpu, cpu, prove].into_iter().flatten() {
                    let saved = flow.clone();
                    body_param_sinks(b, flow, known_param_sinks, found);
                    *flow = saved;
                }
            }
            Stmt::Assign { target, value } => {
                // Reassignment-insensitive for taint *clearing*, but we DO propagate param flow
                // into an existing name when the RHS carries params (monotone add). Clearing is
                // never performed — same discipline as the main taint analysis.
                collect_param_sinks_in_expr(value, flow, known_param_sinks, found);
                if let Expr::Var(name) = target {
                    let rhs = expr_param_flow(value, flow);
                    flow.entry(name.clone()).or_default().extend(rhs);
                }
            }
            Stmt::ExprStmt(e) => {
                collect_param_sinks_in_expr(e, flow, known_param_sinks, found);
            }
            _ => {
                // Best-effort: scan any nested expressions in other stmt forms for sink calls.
                let mut rets = Vec::new();
                collect_returns_in_stmt(stmt, &mut rets);
                for r in rets {
                    collect_param_sinks_in_expr(&r, flow, known_param_sinks, found);
                }
            }
        }
    }
}

/// If `expr` is (or contains) a sink of params, record those indices. Handles direct `is_sink`
/// builtins and calls to functions whose param_sinks summary marks specific argument positions.
fn collect_param_sinks_in_expr(
    expr: &Expr,
    flow: &BTreeMap<String, BTreeSet<usize>>,
    known_param_sinks: &BTreeMap<String, BTreeSet<usize>>,
    found: &mut BTreeSet<usize>,
) {
    match expr {
        Expr::Call { callee, args } => {
            if is_sink(callee) {
                for arg in args {
                    found.extend(expr_param_flow(arg, flow));
                }
            }
            if let Some(sink_params) = known_param_sinks.get(callee) {
                for &i in sink_params {
                    if let Some(arg) = args.get(i) {
                        found.extend(expr_param_flow(arg, flow));
                    }
                }
            }
            for arg in args {
                collect_param_sinks_in_expr(arg, flow, known_param_sinks, found);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            collect_param_sinks_in_expr(lhs, flow, known_param_sinks, found);
            collect_param_sinks_in_expr(rhs, flow, known_param_sinks, found);
        }
        Expr::Unary { expr, .. }
        | Expr::Cast { expr, .. }
        | Expr::Tainted { inner: expr, .. }
        | Expr::Assume(expr)
        | Expr::Assert(expr)
        | Expr::FieldAccess { base: expr, .. } => {
            collect_param_sinks_in_expr(expr, flow, known_param_sinks, found);
        }
        Expr::Declassify { inner, .. } => {
            collect_param_sinks_in_expr(inner, flow, known_param_sinks, found);
        }
        Expr::Index { base, index } => {
            collect_param_sinks_in_expr(base, flow, known_param_sinks, found);
            collect_param_sinks_in_expr(index, flow, known_param_sinks, found);
        }
        _ => {}
    }
}

/// Populate `ctx.param_sinks` by a monotone fixpoint over free functions.
fn compute_param_sinks(items: &[Item], ctx: &mut SemanticContext) {
    let mut fns: Vec<(String, Vec<String>, &[Stmt])> = Vec::new();
    collect_fn_params_bodies(items, &mut fns);
    loop {
        let mut changed = false;
        // Snapshot so we can consult a stable summary while updating.
        let known = ctx.param_sinks.clone();
        for (name, params, body) in &fns {
            let mut flow: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
            for (i, p) in params.iter().enumerate() {
                flow.insert(p.clone(), BTreeSet::from([i]));
            }
            let mut found = BTreeSet::new();
            body_param_sinks(body, &mut flow, &known, &mut found);
            let entry = ctx.param_sinks.entry(name.clone()).or_default();
            for i in found {
                if entry.insert(i) {
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
}

// ── Interprocedural param→return summary (`ctx.param_return_taint`) ──────────────────────────────
// Monotone fixpoint: which formal parameters of each function can flow to its return value without
// declassify. Call sites combine this with argument taint so `fn wrap(x){return x;}` makes
// `wrap(tainted)` a taint source — and only those params (not every arg) taint the call, fixing the
// `fn ignore(x){return 5;} ignore(secret)` false positive of the historical any-arg rule.

/// Param-flow through an expression, consulting `known_param_return` so a call only carries params
/// that the callee summary says reach its return (when the callee is known).
fn expr_param_return_flow(
    expr: &Expr,
    flow: &BTreeMap<String, BTreeSet<usize>>,
    known_param_return: &BTreeMap<String, BTreeSet<usize>>,
) -> BTreeSet<usize> {
    match expr {
        Expr::Var(name) => flow.get(name).cloned().unwrap_or_default(),
        Expr::Binary { lhs, rhs, .. } => {
            let mut s = expr_param_return_flow(lhs, flow, known_param_return);
            s.extend(expr_param_return_flow(rhs, flow, known_param_return));
            s
        }
        Expr::Unary { expr, .. } | Expr::Cast { expr, .. } | Expr::Tainted { inner: expr, .. } => {
            expr_param_return_flow(expr, flow, known_param_return)
        }
        Expr::Assume(inner) | Expr::Assert(inner) => {
            expr_param_return_flow(inner, flow, known_param_return)
        }
        Expr::Declassify {
            inner,
            policy,
            reason,
            ..
        } => {
            if policy.is_some() && reason.is_some() {
                BTreeSet::new()
            } else {
                expr_param_return_flow(inner, flow, known_param_return)
            }
        }
        Expr::Call { callee, args } => {
            if known_param_return.contains_key(callee) {
                // Known user function (entry may be empty): only summarized return-params.
                let mut s = BTreeSet::new();
                if let Some(rets) = known_param_return.get(callee) {
                    for &i in rets {
                        if let Some(arg) = args.get(i) {
                            s.extend(expr_param_return_flow(arg, flow, known_param_return));
                        }
                    }
                }
                s
            } else {
                // Unknown/builtin during bootstrap: conservative union of args.
                args.iter().fold(BTreeSet::new(), |mut acc, a| {
                    acc.extend(expr_param_return_flow(a, flow, known_param_return));
                    acc
                })
            }
        }
        Expr::Index { base, index } => {
            let mut s = expr_param_return_flow(base, flow, known_param_return);
            s.extend(expr_param_return_flow(index, flow, known_param_return));
            s
        }
        Expr::FieldAccess { base, .. } => expr_param_return_flow(base, flow, known_param_return),
        _ => BTreeSet::new(),
    }
}

/// Collect parameter indices that a function body can RETURN under `known_param_return`.
fn body_param_returns(
    stmts: &[Stmt],
    flow: &mut BTreeMap<String, BTreeSet<usize>>,
    known_param_return: &BTreeMap<String, BTreeSet<usize>>,
    found: &mut BTreeSet<usize>,
    tail: bool,
) {
    let n = stmts.len();
    for (i, stmt) in stmts.iter().enumerate() {
        let stmt_is_tail = tail && i + 1 == n;
        match stmt {
            Stmt::Let { name, init, .. } => {
                // Hidden returns inside the initializer.
                let mut rets = Vec::new();
                expr_returns(init, &mut rets);
                for r in rets {
                    found.extend(expr_param_return_flow(&r, flow, known_param_return));
                }
                let cleared = matches!(
                    init,
                    Expr::Declassify {
                        policy: Some(_),
                        reason: Some(_),
                        ..
                    }
                );
                if cleared {
                    flow.insert(name.clone(), BTreeSet::new());
                } else {
                    flow.insert(
                        name.clone(),
                        expr_param_return_flow(init, flow, known_param_return),
                    );
                }
            }
            Stmt::If { then, else_, .. } => {
                let saved = flow.clone();
                body_param_returns(then, flow, known_param_return, found, stmt_is_tail);
                *flow = saved.clone();
                if let Some(else_body) = else_ {
                    body_param_returns(else_body, flow, known_param_return, found, stmt_is_tail);
                }
                *flow = saved;
            }
            Stmt::While { body, .. }
            | Stmt::WhileLet { body, .. }
            | Stmt::Loop { body, .. }
            | Stmt::ResearchBlock { body, .. }
            | Stmt::ExploitBlock { body, .. } => {
                let saved = flow.clone();
                body_param_returns(body, flow, known_param_return, found, false);
                *flow = saved;
            }
            Stmt::For {
                body, var, source, ..
            } => {
                let saved = flow.clone();
                let src_flow = match source {
                    crate::frontend::ForSource::Range { start, end } => {
                        let mut s = expr_param_return_flow(start, flow, known_param_return);
                        s.extend(expr_param_return_flow(end, flow, known_param_return));
                        s
                    }
                    crate::frontend::ForSource::Collection { expr } => {
                        expr_param_return_flow(expr, flow, known_param_return)
                    }
                };
                flow.insert(var.clone(), src_flow);
                body_param_returns(body, flow, known_param_return, found, false);
                *flow = saved;
            }
            Stmt::HybridBlock { gpu, cpu, prove } => {
                for b in [gpu, cpu, prove].into_iter().flatten() {
                    let saved = flow.clone();
                    body_param_returns(b, flow, known_param_return, found, false);
                    *flow = saved;
                }
            }
            Stmt::Assign { target, value } => {
                if let Expr::Var(name) = target {
                    let rhs = expr_param_return_flow(value, flow, known_param_return);
                    flow.entry(name.clone()).or_default().extend(rhs);
                }
            }
            _ => {
                let mut rets = Vec::new();
                collect_returns_in_stmt(stmt, &mut rets);
                for r in rets {
                    found.extend(expr_param_return_flow(&r, flow, known_param_return));
                }
                if stmt_is_tail {
                    if let Stmt::ExprStmt(e) = stmt {
                        if !is_return_call(e) {
                            found.extend(expr_param_return_flow(e, flow, known_param_return));
                        }
                    }
                }
            }
        }
    }
}

/// Populate `ctx.param_return_taint` by a monotone fixpoint. Every free function gets an entry
/// (possibly empty) so the Call arm can distinguish "known, returns no params" from "unknown".
fn compute_param_return_taint(items: &[Item], ctx: &mut SemanticContext) {
    let mut fns: Vec<(String, Vec<String>, &[Stmt])> = Vec::new();
    collect_fn_params_bodies(items, &mut fns);
    // Ensure every function is present so analysis sees an entry (even empty).
    for (name, _, _) in &fns {
        ctx.param_return_taint.entry(name.clone()).or_default();
    }
    loop {
        let mut changed = false;
        let known = ctx.param_return_taint.clone();
        for (name, params, body) in &fns {
            let mut flow: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
            for (i, p) in params.iter().enumerate() {
                flow.insert(p.clone(), BTreeSet::from([i]));
            }
            let mut found = BTreeSet::new();
            body_param_returns(body, &mut flow, &known, &mut found, true);
            let entry = ctx.param_return_taint.entry(name.clone()).or_default();
            for i in found {
                if entry.insert(i) {
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
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

/// Normalize a declared `uses(...)` effect name (or an inferred effect tag) to a canonical
/// capability id used for declared ⊆ inferred checking.
fn normalize_effect_name(raw: &str) -> String {
    let s = raw.trim().to_ascii_lowercase();
    match s.as_str() {
        "fs.read" | "file_read" | "read_file" | "open" => "fs.read".into(),
        "fs.write" | "file_write" | "write_file" => "fs.write".into(),
        "net.send" | "net.connect" | "network" | "send" | "connect" | "network_send" => {
            "net.send".into()
        }
        "shell" | "exec" | "system" => "shell".into(),
        "time.now" | "time" => "time.now".into(),
        "rand.gen" | "rand" | "random" => "rand.gen".into(),
        other => other.to_string(),
    }
}

/// If `inferred` is a capability effect that must be declared in `uses(...)`, return its canonical
/// name; otherwise `None` (analysis-only tags like taint/assume/loop are not gated).
fn capability_effect(inferred: &str) -> Option<String> {
    let base = inferred.split(':').next().unwrap_or(inferred);
    match base {
        "file_read" | "file_write" | "network" | "shell" => Some(normalize_effect_name(base)),
        "time" | "rand" => Some(normalize_effect_name(base)),
        // Direct sink tags are taint machinery, not I/O capabilities.
        _ => None,
    }
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Safe => "safe",
        Mode::Research => "research",
        Mode::Exploit => "exploit",
    }
}

fn bitwidth_of(ty: &str) -> u32 {
    ty::bitwidth(ty)
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
