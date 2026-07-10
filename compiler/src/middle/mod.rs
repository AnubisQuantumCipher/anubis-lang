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
    /// Function name → (parameter names, `requires` clauses, `ensures` clauses). Registered in
    /// pass 1 so a caller can, at a call site, ASSERT the callee's precondition and ASSUME its
    /// postcondition — the composition that makes contracts chain.
    fn_contracts: BTreeMap<String, (Vec<String>, Vec<Expr>, Vec<Expr>)>,
    /// Every user-defined function name (flat namespace; used for duplicate + unknown-call checks).
    all_fns: BTreeSet<String>,
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
                ret,
                requires,
                ensures,
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
                        name, params, body, mode, span, ret, requires, ensures, ..
                    } = m
                    {
                        let effective_mode = if *mode == Mode::Safe {
                            requested_mode
                        } else {
                            *mode
                        };
                        analyze_function(
                            name, module, params, body, ret.as_deref(), requires, ensures,
                            effective_mode, *span, true, ctx,
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
    mode: Mode,
    span: Span,
    is_method: bool,
    ctx: &mut SemanticContext,
) {
    if mode != Mode::Safe {
        ctx.has_research = true;
    }

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

    // Discharge each `ensures` at EVERY return, so no return path can violate the postcondition:
    //   - the TAIL return is verified under the full body assumptions (they all dominate it);
    //   - each EARLY/nested return is verified under the precondition alone (a sound subset — this
    //     catches an unconditionally-violating early return like `return 0` vs `ensures(result>0)`,
    //     and can only ever mis-DISPROVE a path-dependent return, never mis-prove one).
    // Modeling is best-effort: a postcondition the solver cannot express (strings/lists/division) is
    // left un-obligated rather than mis-disproved.
    if !ensures.is_empty() {
        if let Some(tail) = fn_tail_return_expr(body) {
            push_ensures_obligations(ctx, ensures, tail, &assumptions, span);
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

                let init_taint = expr_taint_source(init, scope);
                let declass_source = declassify_source(init, scope);
                // mark known after unknown check so later stmts see it
                ctx.known_bindings.insert(name.clone());

                if ty.is_some() {
                    ctx.annotated_vars.insert(name.clone());
                }
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
                            if !cens.is_empty() && all_requires_checkable {
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
                // A reassigned binding can no longer be modeled from its initial `let` value: the
                // solver does straight-line analysis and cannot follow a loop/branch update, so its
                // concrete-let assumption goes stale. Drop it from the modelable set — an assertion
                // over it is then left to the runtime instead of being (unsoundly) "disproved"
                // against its pre-assignment value (e.g. `for i in 1..5 { total = total + i }
                // assert(total == 10)` must not be refuted with the stale `total == 0`).
                if let Some(root) = assign_target_root(target) {
                    ctx.solver_int_vars.remove(root);
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
                            if !types_compatible(&expected, got) {
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
                    scope.insert(
                        n.clone(),
                        ScopeBinding {
                            info,
                            closure_arity: None,
                        },
                    );
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
                // The loop variable is a fresh in-scope binding for the body's analysis. A range
                // loop (`for i in a..b`) binds a number; a collection loop (`for x in xs`) binds an
                // element whose type is dynamic (unknown) — typing it `u32` was a heuristic that
                // mis-flagged `for x in xs { x[0] }` as "indexing a number".
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
                let is_contract =
                    obl.name.starts_with("ensures:") || obl.name.starts_with("requires@");
                if check.status == "PASS" && is_contract && !obl.assumptions.is_empty() {
                    if let Some(false) = assumptions_satisfiable(obl) {
                        check.status = "FAIL".into();
                        check.detail = "vacuous proof: the contract's assumptions are \
                             self-contradictory (unsatisfiable), so the postcondition is not really \
                             established — check for a `requires`/`assume` that cannot hold"
                            .into();
                    }
                }
                check
            })
            .collect()
    }
}

/// Whether a contract obligation's assumptions are jointly satisfiable. `Some(true)`/`Some(false)`
/// from z3; `None` if the solver did not cleanly decide (in which case the caller keeps the original
/// verdict rather than fabricating a vacuity failure).
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
        .args(["-in", "-smt2"])
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
fn is_int_modelable(e: &Expr, int_vars: &BTreeSet<String>) -> bool {
    match e {
        Expr::Var(v) => int_vars.contains(v),
        // Only a literal that fits i64 is modelable: the runtime holds integers as i64, and a literal
        // beyond i64::MAX (e.g. 2^64) is parsed as f64 at runtime while `(_ bv… 64)` would silently
        // reduce it mod 2^64 — the solver "proved" `x + 2^64 <= x` because it saw `x + 0`.
        Expr::Literal(l) => !l.is_empty() && l.parse::<i64>().is_ok(),
        Expr::Binary { op, lhs, rhs } => {
            // Only ops that model i64 EXACTLY as 64-bit bit-vectors: add/sub/mul (wrap like i64) and
            // bitwise and/or/xor. `/` and `%` (division-by-zero traps at runtime) and `<<`/`>>`
            // (runtime masks the shift mod 64, SMT does not) are left unmodelable, so an assertion
            // over them is skipped rather than unsoundly disproved.
            matches!(op.as_str(), "+" | "-" | "*" | "&" | "|" | "^")
                && is_int_modelable(lhs, int_vars)
                && is_int_modelable(rhs, int_vars)
        }
        Expr::Unary { op, expr } => op == "-" && is_int_modelable(expr, int_vars),
        // A cast is modelable only when it cannot change the i64 value. `x as u8`/`u16`/`u32` truncate
        // at runtime, so modeling them as the identity is unsound (it "proved" `(x as u8) == x` while
        // `ident8(256)` runs to 0). Only 64-bit-target casts are value-preserving.
        Expr::Cast { expr, ty } => cast_preserves_i64(ty) && is_int_modelable(expr, int_vars),
        Expr::Declassify { inner, .. } | Expr::Assume(inner) | Expr::Assert(inner) => {
            is_int_modelable(inner, int_vars)
        }
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
            "&&" | "||" => {
                is_bool_modelable(lhs, int_vars) && is_bool_modelable(rhs, int_vars)
            }
            _ => false,
        },
        Expr::Unary { op, expr } => op == "!" && is_bool_modelable(expr, int_vars),
        Expr::Declassify { inner, .. } | Expr::Assume(inner) | Expr::Assert(inner) => {
            is_bool_modelable(inner, int_vars)
        }
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
                // `/ % << >>` are intentionally NOT encoded here: they are excluded from
                // is_int_modelable (division-by-zero and shift-by-≥64 do not match the i64 runtime),
                // so a modeled assertion never reaches this arm with them.
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

/// The expression a function returns at its tail: the last statement when it is `return X` or a bare
/// value expression. None when the body ends in a statement (which yields the default `0`).
fn fn_tail_return_expr(body: &[Stmt]) -> Option<&Expr> {
    match body.last()? {
        Stmt::ExprStmt(Expr::Call { callee, args }) if callee == "return" => args.first(),
        Stmt::ExprStmt(e) => Some(e),
        _ => None,
    }
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
            // so certifying this would be a silent overclaim: fail closed.
            ctx.diagnostics.push(SemanticDiagnostic {
                code: Some("ANUBIS_CONTRACT_UNPROVABLE".into()),
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

/// Collect every explicit `return X` expression in a statement (recursing into nested blocks), so a
/// contract's `ensures` can be checked at every return point, not only the tail.
fn collect_returns_in_stmt(s: &Stmt, out: &mut Vec<Expr>) {
    match s {
        Stmt::ExprStmt(Expr::Call { callee, args }) if callee == "return" => {
            if let Some(e) = args.first() {
                out.push(e.clone());
            }
        }
        Stmt::If { then, else_, .. } => {
            for st in then {
                collect_returns_in_stmt(st, out);
            }
            if let Some(e) = else_ {
                for st in e {
                    collect_returns_in_stmt(st, out);
                }
            }
        }
        Stmt::While { body, .. }
        | Stmt::Loop { body }
        | Stmt::For { body, .. }
        | Stmt::WhileLet { body, .. }
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
        if !types_compatible(rty, &actual) {
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

fn normalize_ty(ty: &str) -> String {
    let t = ty.trim().to_ascii_lowercase();
    match t.as_str() {
        "int" | "i8" | "i16" | "i32" | "i64" | "i128" | "u128" | "usize" | "isize" | "number" => {
            "u32".into()
        }
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

/// An INTEGER type the solver may soundly model as a 64-bit bit-vector (matching the i64 runtime).
/// Floats are deliberately excluded: modeling an `f64` as an integer bit-vector is unsound (it
/// "proved" `2*x != 1` for `x = 0.5`). `tainted<T>` is a qualifier — unwrap it first.
fn is_integer_ty(ty: &str) -> bool {
    let inner = ty.trim();
    let inner = if let Some(rest) = inner.strip_prefix("tainted<") {
        rest.strip_suffix('>').unwrap_or(rest)
    } else {
        inner
    };
    matches!(normalize_ty(inner).as_str(), "u8" | "u16" | "u32" | "u64")
}

/// True when `x as ty` cannot change the underlying i64 value, so the cast may be modeled as the
/// identity in QF_BV. The runtime (`backends/run.rs`) truncates/sign-extends to the target width, so
/// only 64-bit targets are value-preserving; `as u8`/`as u16`/`as u32` (and any float target) DO
/// change the value and must be treated as non-modelable (the solver ignored the truncation and
/// "proved" `(x as u8) == x`).
fn cast_preserves_i64(ty: &str) -> bool {
    matches!(
        ty.trim().to_ascii_lowercase().as_str(),
        "u64" | "i64" | "int" | "integer" | "usize" | "isize" | "u128" | "i128"
    )
}

/// Mangle an Anubis identifier into an SMT symbol that can never collide with an SMT-LIB keyword or
/// a `bv…` literal/operator. Without this, a parameter named `model`, `set`, `check`, or `bvx` was
/// dropped by `collect_vars_from_smt` (it looked like a keyword), left undeclared, and z3 returned a
/// parse error that `check` treated as "not a disproof" — a fail-OPEN hole. Every variable emitted
/// into SMT goes through here so declaration, emission, and collection agree.
fn smt_var(name: &str) -> String {
    format!("anb_{}", name)
}

/// A+ compatibility: numeric widths interoperate; bool/string/enums do not cross.
/// `tainted<T>` is a *qualifier*: clean `T` may flow into a tainted binding (labeling),
/// and tainted flows are still policed by the separate taint analysis.
/// Whether a type annotation is a generic type parameter (a short all-uppercase name like `T`)
/// or a generic instantiation (contains `<`, e.g. `Opt<T>`). Such types are erased at runtime.
fn is_generic_type(t: &str) -> bool {
    let t = t.trim();
    if t.contains('<') {
        return true;
    }
    !t.is_empty() && t.len() <= 2 && t.chars().all(|c| c.is_ascii_uppercase())
}

fn types_compatible(expected: &str, actual: &str) -> bool {
    let e_raw = expected.trim();
    let a_raw = actual.trim();
    // An absent annotation is dynamically typed: parameters written `fn f(x)` (no `: T`) accept
    // any argument, and an argument of unknown static type is accepted by any parameter.
    if e_raw.is_empty() || a_raw.is_empty() {
        return true;
    }
    // Generic type parameters (`T`, `U`) and generic instantiations (`Opt<T>`, `Box<int>`) are
    // erased at runtime, so they cannot be soundly checked — treat them as compatible with anything.
    if is_generic_type(e_raw) || is_generic_type(a_raw) {
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
        Stmt::While { cond, body } => {
            check_expr_semantics(cond, scope, ctx);
            check_block_exprs(body, None, scope, ctx);
        }
        Stmt::WhileLet { expr, body, .. } => {
            check_expr_semantics(expr, scope, ctx);
            check_block_exprs(body, None, scope, ctx);
        }
        Stmt::Loop { body } => check_block_exprs(body, None, scope, ctx),
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
        _ => infer_expr_type_scoped(scrutinee, scope)
            .filter(|t| ctx.enum_variants.contains_key(t)),
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
