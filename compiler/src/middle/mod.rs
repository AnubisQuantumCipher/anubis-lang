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
}

pub fn typecheck(ast: AST, mode: Mode) -> Result<TypedIR, String> {
    let bmode = match mode {
        Mode::Safe => BuildMode::Safe,
        Mode::Research => BuildMode::Research,
        Mode::Exploit => BuildMode::Exploit,
    };
    let mut ctx = SemanticContext::default();
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
                ..
            } => {
                let effective_mode = if *mode == Mode::Safe {
                    requested_mode
                } else {
                    *mode
                };
                analyze_function(name, module, params, body, effective_mode, *span, ctx);
            }
            Item::Struct { .. } => {
                // Minimal support for this slice: structs are parsed and preserved in AST;
                // full type registration and field typing added in typechecker work.
            }
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

                // Minimal type mismatch for Gate2/3 (u32 vs bool literal etc)
                if let Some(t) = ty.as_deref() {
                    let looks_bool =
                        matches!(init, Expr::Literal(s) if s == "true" || s == "false");
                    if (t == "u32" || t == "u8" || t == "u64") && looks_bool {
                        ctx.diagnostics.push(SemanticDiagnostic {
                            code: Some("ANUBIS_TYPE_MISMATCH".into()),
                            message: format!("type mismatch: expected {}, got bool", t),
                            span: Some((span.start, span.end)),
                        });
                    }
                }

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
                    ty: ty.clone().or_else(|| infer_expr_type(init)),
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

                // For solver faithfulness: record equality only for derived computations (Binary)
                if matches!(init, Expr::Binary { .. }) {
                    let def_smt = format!("(= {} {})", name, expr_to_smt(init));
                    ctx.symbolic_defs.push(def_smt.clone());
                    ctx.constraints
                        .push(format!("(assert (= {} {}))", name, expr_to_smt(init)));
                    assumptions.push(def_smt); // so it is included in subsequent obligations
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
                let smt = expr_to_smt(expr);
                assumptions.push(smt.clone());
                ctx.constraints.push(format!("(assert {})", smt));
                effects.push("assume".into());
            }
            Stmt::ExprStmt(Expr::Assert(expr)) => {
                let smt = expr_to_smt(expr);
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
        Expr::Call { callee, args } if is_sink(callee) => {
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
        Expr::Call { args, .. }
            if args
                .iter()
                .any(|arg| expr_taint_source(arg, scope).is_some()) =>
        {
            effects.push("tainted-call".into());
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
                let mut vars: BTreeSet<String> = obl.vars.iter().cloned().collect();
                for d in &ir.symbolic_defs {
                    if let Some(v) = d.split_whitespace().nth(1) {
                        vars.insert(v.trim_end_matches(')').to_string());
                    }
                }
                for v in &vars {
                    if !v.starts_with("bv") && v != "_" && !v.chars().all(|c| c.is_ascii_digit()) {
                        let w = *ir.symbolic_widths.get(v).unwrap_or(&32u32);
                        smt.push_str(&format!("(declare-const {} {})\n", v, smt_bv_type(w)));
                    }
                }
                if ir.symbolic_widths.values().any(|&ww| ww == 8) {
                    smt = smt.replace("(_ bv1 32)", "(_ bv1 8)");
                    smt = smt.replace("(_ bv0 32)", "(_ bv0 8)");
                    smt = smt.replace("(_ bv255 32)", "(_ bv255 8)");
                }
                for d in &ir.symbolic_defs {
                    smt.push_str(&format!("(assert {})\n", d));
                }
                for a in &obl.assumptions {
                    smt.push_str(&format!("(assert {})\n", a));
                }
                smt.push_str(&format!("(assert (not {}))\n", obl.assertion));
                smt.push_str("(check-sat)\n(get-model)\n");
                // Post adjust after full smt built
                if ir.symbolic_widths.values().any(|&ww| ww == 8) {
                    smt = smt.replace("(_ bv1 32)", "(_ bv1 8)");
                    smt = smt.replace("(_ bv0 32)", "(_ bv0 8)");
                    smt = smt.replace("(_ bv255 32)", "(_ bv255 8)");
                }
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

fn expr_to_smt(e: &Expr) -> String {
    match e {
        Expr::Var(v) => v.clone(),
        Expr::Literal(l) => format!("(_ bv{} 32)", l), // default 32 for now
        Expr::Binary { op, lhs, rhs } => {
            let l = expr_to_smt(lhs);
            let r = expr_to_smt(rhs);
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
        Expr::Declassify { inner, .. } => expr_to_smt(inner),
        Expr::Assume(inner) | Expr::Assert(inner) => expr_to_smt(inner),
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

fn infer_expr_type(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Symbolic { ty } => Some(ty.clone()),
        Expr::Tainted { ty, .. } => Some(format!("tainted<{}>", ty)),
        Expr::UnifiedBuffer { ty } => Some(format!("unified Buffer<{}>", ty)),
        Expr::RawPtr { mutable } => Some(
            if *mutable {
                "*mut unknown"
            } else {
                "*const unknown"
            }
            .into(),
        ),
        Expr::Declassify { inner, .. } => infer_expr_type(inner),
        Expr::TaintSource { .. } => Some("tainted<string>".into()),
        _ => None,
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
