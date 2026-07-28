//! Anubis run/transpile backend.
//!
//! Lowers a parsed Anubis program to a self-contained Rust program for native
//! execution (`anubis run`) or to a RISC0 zkVM guest (`anubis prove`). This is the
//! executable semantics of Anubis. It lives in the compiler crate (not the CLI) so the
//! whole language is unit-testable without the heavy risc0 workspace.

use crate::frontend::{Expr, Item, Stmt};
use anyhow::{anyhow, Result};

type MonoTypeArgs = Vec<(String, String)>;
type MonoSiteQueue = Vec<(String, MonoTypeArgs)>;
type MonoSitesByCaller = std::collections::BTreeMap<String, MonoSiteQueue>;

/// Emit-time context threaded through lowering: the research gate plus the set of user function
/// names, so a call resolves to (in priority order) a user function, a stdlib builtin, or the
/// application of a closure-valued variable.
#[derive(Clone, Copy)]
struct EmitCtx<'a> {
    allow_research: bool,
    fns: &'a std::collections::BTreeSet<String>,
    /// free-function name -> parameter count. Used to synthesize a closure value when a
    /// function is referenced by bare name in value position (`map(xs, my_fn)`).
    fn_arities: &'a std::collections::BTreeMap<String, usize>,
    /// method name -> the `(type, param_count)` of each type defining a method of that name
    /// (for dispatching `obj.m(..)`). `param_count` includes `self`.
    methods: &'a std::collections::BTreeMap<String, Vec<(String, usize)>>,
    /// Names bound locally in the current function (params + all let/for/match/… bindings). A
    /// call to one of these is a closure application, shadowing any builtin of the same name.
    locals: &'a std::collections::BTreeSet<String>,
    /// struct name → (field name → declared type). Used to enforce a struct-literal field's declared
    /// NUMERIC kind at construction (task #34 dual, extended to the struct-field boundary): a float
    /// field coerces an Int value to Float, an integer field fail-closes on a non-Int. This keeps the
    /// solver's per-field SMT model (QF_FP vs QF_BV) sound even when the value is an opaque expression
    /// (a `Call`/`FieldAccess`/`Index` the checker cannot type) that smuggles the wrong runtime kind.
    struct_field_types:
        &'a std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
    /// Static monomorphization dispatch table from the type checker.
    /// Call sites that pin args to a specialization invoke the mangled clone instead of the
    /// generic `anb_*` body. Primitive specializations may use an unboxed native ABI.
    mono: &'a [MonoEmitSpec],
    /// Free-function parameter type annotations (for mono call matching).
    fn_param_types: &'a std::collections::BTreeMap<String, Vec<String>>,
    /// Function currently being emitted (for ordered mono call-site queues).
    current_fn: Option<&'a str>,
    /// Per-caller ordered generic call sites: callee name + type_args pairs.
    mono_sites_by_caller: &'a MonoSitesByCaller,
    /// Per-caller consumption cursor into `mono_sites_by_caller`.
    mono_cursors: &'a std::collections::BTreeMap<String, std::cell::Cell<usize>>,
}

/// One monomorphized specialization ready for emit + call-site rewrite.
#[derive(Clone, Debug)]
struct MonoEmitSpec {
    function: String,
    type_args: Vec<(String, String)>,
    rust_name: String,
    /// Native ABI when every specialized param + return is a primitive.
    unboxed: Option<MonoUnboxedAbi>,
}

/// Unboxed monomorphization ABI: native Rust types at the function boundary.
#[derive(Clone, Debug)]
struct MonoUnboxedAbi {
    params: Vec<MonoPrim>,
    ret: MonoPrim,
}

/// Primitive kinds eligible for unboxed monomorphized ABI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MonoPrim {
    Int,
    Float,
    Bool,
    String,
}

impl MonoPrim {
    fn rust_ty(self) -> &'static str {
        match self {
            MonoPrim::Int => "i64",
            MonoPrim::Float => "f64",
            MonoPrim::Bool => "bool",
            MonoPrim::String => "String",
        }
    }

    /// Wrap an AnubisValue expression into this native type.
    fn coerce_from_anubis(self, expr: &str) -> String {
        match self {
            MonoPrim::Int => format!("({expr}).as_i64()"),
            MonoPrim::Float => format!("({expr}).as_f64()"),
            MonoPrim::Bool => format!("({expr}).as_bool()"),
            MonoPrim::String => format!("({expr}).display_string()"),
        }
    }

    /// Wrap a native expression into AnubisValue.
    fn to_anubis(self, expr: &str) -> String {
        match self {
            MonoPrim::Int => format!("AnubisValue::Int({expr})"),
            MonoPrim::Float => format!("AnubisValue::Float({expr})"),
            MonoPrim::Bool => format!("AnubisValue::Bool({expr})"),
            MonoPrim::String => format!("anubis_mk_str({expr})"),
        }
    }
}

/// Classify a specialized type annotation for unboxed ABI eligibility.
fn mono_prim_of(annotation: &str) -> Option<MonoPrim> {
    let t = annotation.trim();
    if t == "bool" {
        return Some(MonoPrim::Bool);
    }
    if t == "string" || t == "str" {
        return Some(MonoPrim::String);
    }
    if crate::middle::ty::is_float(t) || t == "float" || t == "f64" {
        return Some(MonoPrim::Float);
    }
    if crate::middle::ty::is_integer(t) || t == "int" || t == "i64" || t == "i32" {
        return Some(MonoPrim::Int);
    }
    None
}

/// Build unboxed ABI when every specialized param and the return type are primitives.
fn mono_unboxed_abi(params: &[(String, String)], ret: Option<&str>) -> Option<MonoUnboxedAbi> {
    let ret_ann = ret?;
    let ret = mono_prim_of(ret_ann)?;
    let mut param_prims = Vec::with_capacity(params.len());
    for (_, ty) in params {
        param_prims.push(mono_prim_of(ty)?);
    }
    Some(MonoUnboxedAbi {
        params: param_prims,
        ret,
    })
}

/// Collect every name bound anywhere in a function (params + let/for/match/if-let/while-let/
/// lambda bindings). Over-approximates scope per function, which is enough to let a local shadow
/// a builtin of the same name at call sites.
fn collect_local_names(
    params: &[(String, String)],
    body: &[Stmt],
) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    for (p, _) in params {
        out.insert(p.clone());
    }
    collect_bound_in_stmts(body, &mut out);
    out
}

fn collect_bound_in_stmts(stmts: &[Stmt], out: &mut std::collections::BTreeSet<String>) {
    use crate::frontend::ForSource;
    for s in stmts {
        match s {
            Stmt::Let { name, init, .. } => {
                out.insert(name.clone());
                collect_bound_in_expr(init, out);
            }
            Stmt::LetPattern { pattern, init, .. } => {
                for n in pattern.bound_names() {
                    out.insert(n);
                }
                collect_bound_in_expr(init, out);
            }
            Stmt::Assign { target, value } => {
                collect_bound_in_expr(target, out);
                collect_bound_in_expr(value, out);
            }
            Stmt::ExprStmt(e) => collect_bound_in_expr(e, out),
            Stmt::If { cond, then, else_ } => {
                collect_bound_in_expr(cond, out);
                collect_bound_in_stmts(then, out);
                if let Some(e) = else_ {
                    collect_bound_in_stmts(e, out);
                }
            }
            Stmt::While { cond, body, .. } => {
                collect_bound_in_expr(cond, out);
                collect_bound_in_stmts(body, out);
            }
            Stmt::Loop { body, .. } => collect_bound_in_stmts(body, out),
            Stmt::For {
                var, source, body, ..
            } => {
                out.insert(var.clone());
                match source {
                    ForSource::Range { start, end } => {
                        collect_bound_in_expr(start, out);
                        collect_bound_in_expr(end, out);
                    }
                    ForSource::Collection { expr } => collect_bound_in_expr(expr, out),
                }
                collect_bound_in_stmts(body, out);
            }
            Stmt::WhileLet {
                pattern,
                expr,
                body,
            } => {
                for n in pattern.bound_names() {
                    out.insert(n);
                }
                collect_bound_in_expr(expr, out);
                collect_bound_in_stmts(body, out);
            }
            _ => {}
        }
    }
}

fn collect_bound_in_expr(e: &Expr, out: &mut std::collections::BTreeSet<String>) {
    match e {
        Expr::Lambda { params, body } => {
            for p in params {
                out.insert(p.clone());
            }
            collect_bound_in_expr(body, out);
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
            collect_bound_in_expr(scrutinee, out);
            collect_bound_in_expr(then, out);
            collect_bound_in_expr(else_, out);
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            collect_bound_in_expr(scrutinee, out);
            for a in arms {
                for n in a.pattern.bound_names() {
                    out.insert(n);
                }
                if let Some(g) = &a.guard {
                    collect_bound_in_expr(g, out);
                }
                collect_bound_in_expr(&a.body, out);
            }
        }
        Expr::Block { stmts, tail } => {
            collect_bound_in_stmts(stmts, out);
            if let Some(t) = tail {
                collect_bound_in_expr(t, out);
            }
        }
        Expr::If {
            cond, then, else_, ..
        } => {
            collect_bound_in_expr(cond, out);
            collect_bound_in_expr(then, out);
            collect_bound_in_expr(else_, out);
        }
        Expr::Binary { lhs, rhs, .. } => {
            collect_bound_in_expr(lhs, out);
            collect_bound_in_expr(rhs, out);
        }
        Expr::Unary { expr, .. } | Expr::Cast { expr, .. } | Expr::Try(expr) => {
            collect_bound_in_expr(expr, out)
        }
        Expr::Call { args, .. } => {
            for a in args {
                collect_bound_in_expr(a, out);
            }
        }
        Expr::CallExpr { callee, args } => {
            collect_bound_in_expr(callee, out);
            for a in args {
                collect_bound_in_expr(a, out);
            }
        }
        Expr::Index { base, index } => {
            collect_bound_in_expr(base, out);
            collect_bound_in_expr(index, out);
        }
        Expr::FieldAccess { base, .. } => collect_bound_in_expr(base, out),
        Expr::ArrayLiteral { elements } => {
            for el in elements {
                collect_bound_in_expr(el, out);
            }
        }
        Expr::MapLiteral { entries, .. } => {
            for (k, v) in entries {
                collect_bound_in_expr(k, out);
                collect_bound_in_expr(v, out);
            }
        }
        Expr::StructLiteral { fields, .. } => {
            for (_, v) in fields {
                collect_bound_in_expr(v, out);
            }
        }
        Expr::EnumConstruct { fields, .. } => {
            for f in fields {
                collect_bound_in_expr(f, out);
            }
        }
        _ => {}
    }
}

/// A borrowed view of one Anubis function. `impl_type` is `Some(TypeName)` for a method defined
/// in an `impl TypeName { ... }` block, which mangles its emitted name and takes `self` first.
struct FnDef<'a> {
    name: &'a str,
    params: &'a [(String, String)],
    body: &'a [Stmt],
    impl_type: Option<&'a str>,
    /// Declared return type (`-> T`), or `None`. When it is an integer type, the runtime enforces the
    /// value returned is actually an integer (see the entry guard's RC6/RC7 rationale).
    ret_type: Option<&'a str>,
    /// Generic type parameters (`fn id<T>(…)` → `["T"]`). Empty for monomorphic functions.
    generics: &'a [String],
}

/// The emitted Rust function name for an Anubis function or method.
fn fn_rust_name(name: &str, impl_type: Option<&str>) -> Result<String> {
    match impl_type {
        Some(ty) => Ok(format!(
            "anb_{}__method__{}",
            sanitize_ident(ty)?,
            sanitize_ident(name)?
        )),
        None => Ok(format!("anb_{}", sanitize_ident(name)?)),
    }
}

/// Recursively collect every `fn` item (including inside modules and `impl` blocks).
fn collect_fns<'a>(items: &'a [Item], out: &mut Vec<FnDef<'a>>) {
    for item in items {
        match item {
            Item::Fn {
                name,
                params,
                body,
                ret,
                generics,
                ..
            } => out.push(FnDef {
                name: name.as_str(),
                params: params.as_slice(),
                body: body.as_slice(),
                impl_type: None,
                ret_type: ret.as_deref(),
                generics: generics.as_slice(),
            }),
            Item::Impl {
                type_name, methods, ..
            } => {
                for m in methods {
                    if let Item::Fn {
                        name,
                        params,
                        body,
                        ret,
                        generics,
                        ..
                    } = m
                    {
                        out.push(FnDef {
                            name: name.as_str(),
                            params: params.as_slice(),
                            body: body.as_slice(),
                            impl_type: Some(type_name.as_str()),
                            ret_type: ret.as_deref(),
                            generics: generics.as_slice(),
                        });
                    }
                }
            }
            Item::Module { items, .. } => collect_fns(items, out),
            _ => {}
        }
    }
}

/// Substitute a free type parameter annotation using monomorphization bindings.
/// Bare `T` becomes the concrete type; compound annotations are left unchanged (accept-biased).
fn subst_type_param(
    annotation: &str,
    type_args: &std::collections::BTreeMap<String, String>,
) -> String {
    let t = annotation.trim();
    type_args
        .get(t)
        .cloned()
        .unwrap_or_else(|| annotation.to_string())
}

/// Mangled Rust name for a monomorphized clone: `anb_id__mono__T_u32`.
fn mono_rust_name(
    fn_name: &str,
    type_args: &std::collections::BTreeMap<String, String>,
) -> Result<String> {
    let mut parts: Vec<String> = type_args.iter().map(|(k, v)| format!("{k}_{v}")).collect();
    parts.sort();
    let tag = parts.join("__");
    Ok(format!(
        "anb_{}__mono__{}",
        sanitize_ident(fn_name)?,
        sanitize_ident(&tag)?
    ))
}

/// Whether a source expression is a literal that pins to `expected` annotation at mono dispatch.
fn mono_literal_matches(arg: &Expr, expected: &str) -> bool {
    let t = expected.trim();
    match arg {
        Expr::Literal(s) => {
            let s = s.trim();
            if s == "true" || s == "false" {
                return t == "bool";
            }
            if s.contains('.') {
                return crate::middle::ty::is_float(t) || t == "float" || t == "f64";
            }
            // Decimal / hex integer literal
            if s.parse::<i64>().is_ok()
                || s.strip_prefix("0x")
                    .or_else(|| s.strip_prefix("0X"))
                    .map(|h| i64::from_str_radix(h, 16).is_ok())
                    .unwrap_or(false)
            {
                return crate::middle::ty::is_integer(t) || t == "int" || t == "i64" || t == "i32";
            }
            false
        }
        Expr::Unary { op, expr, .. } if op == "-" && matches!(expr.as_ref(), Expr::Literal(_)) => {
            crate::middle::ty::is_integer(t) || t == "int" || t == "i64" || t == "i32"
        }
        Expr::StrLiteral(_) => t == "string" || t == "str",
        _ => false,
    }
}

/// Resolved monomorphized call: rust name + optional unboxed ABI for wrap/unwrap.
struct MonoCallResolved {
    rust_name: String,
    unboxed: Option<MonoUnboxedAbi>,
}

/// Look up a mono table entry by callee + type_args pairs.
fn mono_spec_for(
    callee: &str,
    pairs: &[(String, String)],
    ctx: &EmitCtx<'_>,
) -> Option<MonoCallResolved> {
    for spec in ctx.mono {
        if spec.function == callee && spec.type_args.as_slice() == pairs {
            return Some(MonoCallResolved {
                rust_name: spec.rust_name.clone(),
                unboxed: spec.unboxed.clone(),
            });
        }
    }
    None
}

/// Pick a monomorphized specialization: prefer ordered checker call-site queue (variable-pinned),
/// then literal-arg matching as a fallback.
fn resolve_mono_call(callee: &str, args: &[Expr], ctx: &EmitCtx<'_>) -> Option<MonoCallResolved> {
    // 1) Ordered queue from typecheck (handles `id(x)` when the checker pinned `x`).
    if let Some(caller) = ctx.current_fn {
        if let (Some(sites), Some(cursor)) = (
            ctx.mono_sites_by_caller.get(caller),
            ctx.mono_cursors.get(caller),
        ) {
            let ix = cursor.get();
            if ix < sites.len() {
                let (site_callee, pairs) = &sites[ix];
                if site_callee == callee {
                    cursor.set(ix + 1);
                    if !pairs.is_empty() {
                        if let Some(resolved) = mono_spec_for(callee, pairs, ctx) {
                            return Some(resolved);
                        }
                    }
                    // Consumed unpinned site — fall through to generic (or literal fallback).
                    return None;
                }
            }
        }
    }

    // 2) Literal-arg fallback when queue is absent/mismatched.
    let param_tys = ctx.fn_param_types.get(callee)?;
    if param_tys.len() != args.len() {
        return None;
    }
    for spec in ctx.mono {
        if spec.function != callee {
            continue;
        }
        let map: std::collections::BTreeMap<String, String> =
            spec.type_args.iter().cloned().collect();
        if map.is_empty() {
            continue;
        }
        let mut ok = true;
        let mut pinned = 0usize;
        for (pty, arg) in param_tys.iter().zip(args.iter()) {
            let base = pty.trim();
            if let Some(concrete) = map.get(base) {
                if !mono_literal_matches(arg, concrete) {
                    ok = false;
                    break;
                }
                pinned += 1;
            }
        }
        if ok && pinned > 0 {
            return Some(MonoCallResolved {
                rust_name: spec.rust_name.clone(),
                unboxed: spec.unboxed.clone(),
            });
        }
    }
    None
}

/// Whether `expr` is a pure expression of live names + literals only (no calls/IO).
fn mono_expr_is_full_native_eligible(
    expr: &Expr,
    live: &std::collections::BTreeSet<String>,
) -> bool {
    match expr {
        Expr::Var(n) => live.contains(n),
        Expr::Literal(_) | Expr::StrLiteral(_) => true,
        Expr::Unary { op, expr, .. } if op == "-" || op == "!" || op == "~" => {
            mono_expr_is_full_native_eligible(expr, live)
        }
        Expr::Binary { op, lhs, rhs, .. } => {
            matches!(
                op.as_str(),
                "+" | "-"
                    | "*"
                    | "/"
                    | "%"
                    | "&"
                    | "|"
                    | "^"
                    | "<<"
                    | ">>"
                    | "&&"
                    | "||"
                    | "=="
                    | "!="
                    | "<"
                    | "<="
                    | ">"
                    | ">="
            ) && mono_expr_is_full_native_eligible(lhs, live)
                && mono_expr_is_full_native_eligible(rhs, live)
        }
        // Pure if-expression: cond and both arms must be pure.
        Expr::If {
            cond, then, else_, ..
        } => {
            mono_expr_is_full_native_eligible(cond, live)
                && mono_expr_is_full_native_eligible(then, live)
                && mono_expr_is_full_native_eligible(else_, live)
        }
        // Block expression used as if-branch: only a pure tail (no side-effect stmts).
        Expr::Block { stmts, tail } => {
            stmts.is_empty()
                && tail
                    .as_ref()
                    .map(|t| mono_expr_is_full_native_eligible(t, live))
                    .unwrap_or(false)
        }
        _ => false,
    }
}

/// Emit a native Rust expression for a full-unbox mono body (params already native-typed).
fn emit_mono_native_expr(expr: &Expr) -> Result<String> {
    match expr {
        Expr::Var(n) => sanitize_ident(n),
        Expr::Literal(s) => {
            let s = s.trim();
            if s == "true" || s == "false" {
                return Ok(s.to_string());
            }
            if s.parse::<i64>().is_ok()
                || s.strip_prefix("0x")
                    .or_else(|| s.strip_prefix("0X"))
                    .map(|h| i64::from_str_radix(h, 16).is_ok())
                    .unwrap_or(false)
            {
                return Ok(s.to_string());
            }
            if s.parse::<f64>().is_ok() {
                return Ok(s.to_string());
            }
            Err(anyhow!("mono native: unsupported literal `{s}`"))
        }
        Expr::StrLiteral(s) => Ok(format!("{}.to_string()", rust_string_lit(s)?)),
        Expr::Unary { op, expr, .. } => {
            let inner = emit_mono_native_expr(expr)?;
            match op.as_str() {
                "-" => Ok(format!("-({inner})")),
                "!" => Ok(format!("!({inner})")),
                "~" => Ok(format!("!({inner})")),
                other => Err(anyhow!("mono native: unsupported unary `{other}`")),
            }
        }
        Expr::Binary { op, lhs, rhs, .. } => {
            let l = emit_mono_native_expr(lhs)?;
            let r = emit_mono_native_expr(rhs)?;
            match op.as_str() {
                "+" | "-" | "*" | "/" | "%" | "&" | "|" | "^" | "<<" | ">>" | "&&" | "||"
                | "==" | "!=" | "<" | "<=" | ">" | ">=" => Ok(format!("({l} {op} {r})")),
                other => Err(anyhow!("mono native: unsupported binary `{other}`")),
            }
        }
        Expr::If {
            cond, then, else_, ..
        } => {
            let c = emit_mono_native_expr(cond)?;
            let t = emit_mono_native_expr(then)?;
            let e = emit_mono_native_expr(else_)?;
            Ok(format!("if {c} {{ {t} }} else {{ {e} }}"))
        }
        Expr::Block { stmts, tail } if stmts.is_empty() => match tail {
            Some(t) => emit_mono_native_expr(t),
            None => Err(anyhow!("mono native: empty block")),
        },
        _ => Err(anyhow!("mono native: unsupported expression form")),
    }
}

/// Extract a pure return expression from a statement block (single return / bare expr only).
fn mono_block_pure_expr(stmts: &[Stmt]) -> Option<&Expr> {
    match stmts {
        [Stmt::ExprStmt(Expr::Call { callee, args })] if callee == "return" => args.first(),
        [Stmt::ExprStmt(e)] => Some(e),
        _ => None,
    }
}

/// Fully native mono body (no AnubisValue) for pure specializations:
/// - single return/expr
/// - `let` chains of pure inits + final return
/// - trailing pure `if`/`else` (stmt or expr form)
///
/// Returns `None` to fall back to the AnubisValue-inner unboxed wrapper.
fn try_emit_mono_full_native_body(
    params: &[(String, String)],
    body: &[Stmt],
    abi: &MonoUnboxedAbi,
) -> Result<Option<String>> {
    if body.is_empty() {
        return Ok(None);
    }

    let mut live: std::collections::BTreeSet<String> =
        params.iter().map(|(n, _)| n.clone()).collect();

    let mut out = String::new();
    // Preserve u8/u16/u32 range honesty without AnubisValue.
    for ((p, ty), prim) in params.iter().zip(abi.params.iter()) {
        if *prim == MonoPrim::Int {
            if let Some(w) = crate::middle::ty::unsigned_mask_width(ty) {
                let id = sanitize_ident(p)?;
                out.push_str(&format!("    let {id} = {id} & ((1i64 << {w}) - 1);\n"));
            }
        }
    }

    let n = body.len();
    for (i, stmt) in body.iter().enumerate() {
        let is_last = i + 1 == n;
        match stmt {
            // Intermediate pure lets: `let y = x + 1;`
            Stmt::Let { name, init, .. } if !is_last => {
                if !mono_expr_is_full_native_eligible(init, &live) {
                    return Ok(None);
                }
                let id = sanitize_ident(name)?;
                let e = match emit_mono_native_expr(init) {
                    Ok(s) => s,
                    Err(_) => return Ok(None),
                };
                out.push_str(&format!("    let {id} = {e};\n"));
                live.insert(name.clone());
            }
            // Final pure if-stmt with pure then/else arms.
            Stmt::If {
                cond, then, else_, ..
            } if is_last => {
                let else_body = match else_ {
                    Some(e) => e.as_slice(),
                    None => return Ok(None),
                };
                let then_e = match mono_block_pure_expr(then) {
                    Some(e) => e,
                    None => return Ok(None),
                };
                let else_e = match mono_block_pure_expr(else_body) {
                    Some(e) => e,
                    None => return Ok(None),
                };
                if !mono_expr_is_full_native_eligible(cond, &live)
                    || !mono_expr_is_full_native_eligible(then_e, &live)
                    || !mono_expr_is_full_native_eligible(else_e, &live)
                {
                    return Ok(None);
                }
                let c = match emit_mono_native_expr(cond) {
                    Ok(s) => s,
                    Err(_) => return Ok(None),
                };
                let t = match emit_mono_native_expr(then_e) {
                    Ok(s) => s,
                    Err(_) => return Ok(None),
                };
                let e = match emit_mono_native_expr(else_e) {
                    Ok(s) => s,
                    Err(_) => return Ok(None),
                };
                out.push_str(&format!("    if {c} {{ {t} }} else {{ {e} }}\n"));
            }
            // Final return / bare expression.
            Stmt::ExprStmt(Expr::Call { callee, args }) if callee == "return" && is_last => {
                match args.as_slice() {
                    [] => {
                        if matches!(abi.ret, MonoPrim::Int) {
                            out.push_str("    0\n");
                        } else {
                            return Ok(None);
                        }
                    }
                    [e] => {
                        if !mono_expr_is_full_native_eligible(e, &live) {
                            return Ok(None);
                        }
                        let expr_src = match emit_mono_native_expr(e) {
                            Ok(s) => s,
                            Err(_) => return Ok(None),
                        };
                        out.push_str(&format!("    {expr_src}\n"));
                    }
                    _ => return Ok(None),
                }
            }
            Stmt::ExprStmt(e) if is_last => {
                if !mono_expr_is_full_native_eligible(e, &live) {
                    return Ok(None);
                }
                let expr_src = match emit_mono_native_expr(e) {
                    Ok(s) => s,
                    Err(_) => return Ok(None),
                };
                out.push_str(&format!("    {expr_src}\n"));
            }
            _ => return Ok(None),
        }
    }
    Ok(Some(out))
}

/// Emit one argument for an unboxed mono call (prefer bare literals; else unwrap AnubisValue).
fn emit_mono_arg_unboxed(arg: &Expr, prim: MonoPrim, ctx: &EmitCtx<'_>) -> Result<String> {
    match (arg, prim) {
        (Expr::Literal(s), MonoPrim::Int) => {
            let s = s.trim();
            if s.parse::<i64>().is_ok()
                || s.strip_prefix("0x")
                    .or_else(|| s.strip_prefix("0X"))
                    .map(|h| i64::from_str_radix(h, 16).is_ok())
                    .unwrap_or(false)
            {
                return Ok(s.to_string());
            }
        }
        (Expr::Unary { op, expr, .. }, MonoPrim::Int)
            if op == "-" && matches!(expr.as_ref(), Expr::Literal(_)) =>
        {
            if let Expr::Literal(s) = expr.as_ref() {
                return Ok(format!("-{}", s.trim()));
            }
        }
        (Expr::Literal(s), MonoPrim::Float) if s.contains('.') || s.parse::<f64>().is_ok() => {
            return Ok(s.trim().to_string());
        }
        (Expr::Literal(s), MonoPrim::Bool) if s == "true" || s == "false" => {
            return Ok(s.trim().to_string());
        }
        (Expr::StrLiteral(s), MonoPrim::String) => {
            return Ok(format!("{}.to_string()", rust_string_lit(s)?));
        }
        _ => {}
    }
    let v = safe_run_expr(arg, ctx)?;
    Ok(prim.coerce_from_anubis(&v))
}

/// Build the method registry: method name -> `(type, param_count)` for each defining type.
fn collect_methods(
    items: &[Item],
    out: &mut std::collections::BTreeMap<String, Vec<(String, usize)>>,
) {
    for item in items {
        match item {
            Item::Impl {
                type_name, methods, ..
            } => {
                for m in methods {
                    if let Item::Fn { name, params, .. } = m {
                        let types = out.entry(name.clone()).or_default();
                        if !types.iter().any(|(t, _)| t == type_name) {
                            types.push((type_name.clone(), params.len()));
                        }
                    }
                }
            }
            Item::Module { items, .. } => collect_methods(items, out),
            _ => {}
        }
    }
}

/// Emit one Anubis function as a Rust function returning `AnubisValue`.
/// The trailing `AnubisValue::Int(0)` is the implicit return for functions that
/// fall off the end without an explicit `return`.
fn emit_fn(def: &FnDef, base: &EmitCtx) -> Result<String> {
    let rust_name = fn_rust_name(def.name, def.impl_type)?;
    emit_fn_core(
        def.name,
        def.params,
        def.body,
        def.ret_type,
        &rust_name,
        base,
        None,
    )
}

/// Core emitter shared by monomorphic functions and monomorphized clones.
/// When `unboxed` is set, the function ABI uses native Rust types at the boundary; the body still
/// runs on `AnubisValue` locals (converted at entry / unwrapped at return).
fn emit_fn_core(
    name: &str,
    params: &[(String, String)],
    body: &[Stmt],
    ret_type: Option<&str>,
    rust_name: &str,
    base: &EmitCtx,
    unboxed: Option<&MonoUnboxedAbi>,
) -> Result<String> {
    // Per-function local scope: params + everything bound in the body. A call to one of these
    // names is a closure application, not a builtin.
    let locals = collect_local_names(params, body);
    let ctx = &EmitCtx {
        allow_research: base.allow_research,
        fns: base.fns,
        fn_arities: base.fn_arities,
        methods: base.methods,
        locals: &locals,
        struct_field_types: base.struct_field_types,
        mono: base.mono,
        fn_param_types: base.fn_param_types,
        current_fn: base.current_fn,
        mono_sites_by_caller: base.mono_sites_by_caller,
        mono_cursors: base.mono_cursors,
    };
    // Inner body always uses AnubisValue (returns / guards stay valid). Unboxed ABI is an outer
    // wrapper that converts native params → AnubisValue and unwraps the AnubisValue result.
    let mut sig = Vec::new();
    for (p, _ty) in params {
        let id = sanitize_ident(p)?;
        sig.push(format!("{}{}: AnubisValue", mut_prefix(&id), id));
    }
    let (head, tail) = split_tail_expr(body);
    let mut body_src = String::new();
    // RC4 soundness: a parameter the checker models as an integer (u8/u16/u32/u64) is proved over a
    // pure i64. Guard at entry that it actually holds an integer, so a float/string/... argument fails
    // closed (ANUBIS_TYPE_VIOLATION) instead of taking a divergent runtime path that violates the
    // proven contract. Uses the SAME predicate the solver's param-modeling gate uses, so they align.
    for (p, ty) in params {
        if let Some(w) = crate::middle::ty::unsigned_mask_width(ty) {
            // A1 (task #50): an unsigned fixed-width param (u8/u16/u32) is masked to [0, 2^w) at
            // entry, so the checker's injected range (`0 <= x < 2^w`) is genuinely what the runtime
            // holds — the `requires(x >= 0)` tax disappears. Shadows the binding (mut for reassign).
            let id = sanitize_ident(p)?;
            body_src.push_str(&format!(
                "    let mut {id} = anubis_coerce_uint_param({id}, {}, {w});\n",
                rust_string_lit(p)?
            ));
        } else if crate::middle::ty::is_integer(ty) {
            let id = sanitize_ident(p)?;
            body_src.push_str(&format!(
                "    anubis_require_int(&{id}, {});\n",
                rust_string_lit(p)?
            ));
        } else if crate::middle::ty::is_float(ty) {
            // Operator policy (task #34): coerce an Int argument to a Float for a float-typed param, so the
            // checker's QF_FP model (which treats the param as a float) is sound — `f(7)` binds x = 7.0, not
            // Int(7), so `x / 2` is float division. Shadows the param binding (mut suppressed if unused).
            let id = sanitize_ident(p)?;
            body_src.push_str(&format!(
                "    let mut {id} = anubis_coerce_float_param({id}, {});\n",
                rust_string_lit(p)?
            ));
        }
    }
    for stmt in &head {
        emit_safe_run_stmt(stmt, 1, &mut body_src, ctx)?;
    }
    // Implicit return: a bare trailing expression is the function's value (like Rust/ML).
    // Falls back to Int(0) for bodies that end in a statement or are empty.
    let tail_src = match &tail {
        Some(expr) => safe_run_expr(expr, ctx)?,
        None => "AnubisValue::Int(0)".to_string(),
    };
    let inner_sig = sig.join(", ");

    // Unboxed monomorphized ABI.
    if let Some(abi) = unboxed {
        let mut outer_params = Vec::new();
        for ((p, _), prim) in params.iter().zip(abi.params.iter()) {
            let id = sanitize_ident(p)?;
            outer_params.push(format!("{id}: {}", prim.rust_ty()));
        }
        let ret_ty = abi.ret.rust_ty();
        let outer = outer_params.join(", ");

        // Full native body when the function is a simple pure expression of params/literals
        // (no AnubisValue inner). Falls back to native outer + AnubisValue inner otherwise.
        if let Some(native_body) = try_emit_mono_full_native_body(params, body, abi)? {
            return Ok(format!(
                "fn {rust_name}({outer}) -> {ret_ty} {{\n    __anb_stack_guard();\n{native_body}}}\n"
            ));
        }

        let mut fwd = Vec::new();
        for ((p, _), prim) in params.iter().zip(abi.params.iter()) {
            let id = sanitize_ident(p)?;
            // Convert native arg → AnubisValue for the shared body.
            fwd.push(match prim {
                MonoPrim::Int => format!("AnubisValue::Int({id})"),
                MonoPrim::Float => format!("AnubisValue::Float({id})"),
                MonoPrim::Bool => format!("AnubisValue::Bool({id})"),
                MonoPrim::String => format!("anubis_mk_str({id})"),
            });
        }
        let unwrap = abi.ret.coerce_from_anubis("__anb_ret");
        return Ok(format!(
            "fn {rust_name}({outer}) -> {ret_ty} {{\n    __anb_stack_guard();\n    fn __anb_body({inner_sig}) -> AnubisValue {{\n{body_src}    {tail_src}\n    }}\n    let __anb_ret = __anb_body({fwd});\n    {unwrap}\n}}\n",
            fwd = fwd.join(", "),
        ));
    }

    // RC6/RC7 soundness: if the function DECLARES an integer return type, the solver may model its
    // result (and a call-site binding of it) as an i64 — but the return type is INERT at runtime, so a
    // body could return a float (`return 2.5` from a `-> u32` fn) or Bool(true) (`return assume(x)`),
    // poisoning a proof. Enforce the model on EVERY return path by emitting the body as an inner fn and
    // guarding its result (covers the tail AND every explicit `return` uniformly). A non-integer return
    // fails closed (ANUBIS_TYPE_VIOLATION). The outer params drop `mut` since they are only forwarded.
    // An INTEGER return guards fail-closed (anubis_require_int_ret); a FLOAT return COERCES an Int result
    // to a Float (anubis_coerce_float_ret — task #34 dual of the param coercion), so an f64-declared body
    // that yields `Int(7)` still binds a float at the call site and the checker's f64 model stays sound.
    // A1 (task #50): a u32 RETURN is NOT masked at runtime — only unsigned PARAMS are boundary-coerced
    // (that is where the `requires(x >= 0)` tax lives, and where the solver's injected range must be
    // enforced). Returns/locals keep the canonical unbounded-i64 semantics (`u32` is Anubis's default
    // integer spelling — `int`/`i64` normalize to it — so masking returns would silently change every
    // program that returns a negative/overflowing value from a `-> u32` function). So a u32 return uses
    // the plain integer guard `anubis_require_int_ret` (fail-closed on a non-integer, no mask).
    let ret_guard = ret_type.and_then(|t| {
        if crate::middle::ty::is_integer(t) {
            Some("anubis_require_int_ret")
        } else if crate::middle::ty::is_float(t) {
            Some("anubis_coerce_float_ret")
        } else {
            None
        }
    });
    if let Some(guard_fn) = ret_guard {
        let mut outer_params = Vec::new();
        let mut fwd = Vec::new();
        for (p, _ty) in params {
            let id = sanitize_ident(p)?;
            outer_params.push(format!("{id}: AnubisValue"));
            fwd.push(id);
        }
        Ok(format!(
            "fn {rust_name}({outer}) -> AnubisValue {{\n    __anb_stack_guard();\n    fn __anb_body({inner_sig}) -> AnubisValue {{\n{body_src}    {tail_src}\n    }}\n    {guard_fn}(__anb_body({fwd}), {namelit})\n}}\n",
            outer = outer_params.join(", "),
            fwd = fwd.join(", "),
            namelit = rust_string_lit(name)?,
        ))
    } else {
        Ok(format!(
            "fn {rust_name}({inner_sig}) -> AnubisValue {{\n    __anb_stack_guard();\n{body_src}    {tail_src}\n}}\n"
        ))
    }
}

/// Split a function body into (head statements, optional trailing tail expression).
/// The tail is the value the function implicitly returns. A trailing statement that is
/// already a `return`, or a side-effecting void builtin (`print`/`println`/`eprint`/
/// `eprintln`), is kept in the head so its lowering and observable return value are
/// unchanged; every other bare trailing expression becomes the implicit return value.
fn split_tail_expr(body: &[Stmt]) -> (Vec<Stmt>, Option<Expr>) {
    if let Some((last, head)) = body.split_last() {
        if let Some(tail) = stmt_as_tail_expr(last) {
            return (head.to_vec(), Some(tail));
        }
    }
    (body.to_vec(), None)
}

/// Convert a trailing statement into the expression a block/function implicitly returns.
/// A bare expression statement is its own value; a trailing `if/else` becomes an `if`
/// expression whose branches are themselves tail-valued blocks (so `if c { a } else { b }`
/// at the end of a function returns `a` or `b`). Everything else has no tail value.
fn stmt_as_tail_expr(stmt: &Stmt) -> Option<Expr> {
    match stmt {
        Stmt::ExprStmt(e) => {
            let statement_only = matches!(
                e,
                Expr::Call { callee, .. }
                    if matches!(
                        callee.as_str(),
                        "return" | "print" | "println" | "eprint" | "eprintln"
                    )
            );
            if statement_only {
                None
            } else {
                Some(e.clone())
            }
        }
        Stmt::If {
            cond,
            then,
            else_: Some(else_),
        } => Some(Expr::If {
            cond: Box::new(cond.clone()),
            then: Box::new(block_as_tail_expr(then)),
            else_: Box::new(block_as_tail_expr(else_)),
            span: crate::frontend::Span::default(),
        }),
        _ => None,
    }
}

/// Turn a statement block into a block expression whose value is its trailing tail (or Int(0)).
fn block_as_tail_expr(stmts: &[Stmt]) -> Expr {
    if let Some((last, head)) = stmts.split_last() {
        if let Some(tail) = stmt_as_tail_expr(last) {
            return Expr::Block {
                stmts: head.to_vec(),
                tail: Some(Box::new(tail)),
            };
        }
    }
    Expr::Block {
        stmts: stmts.to_vec(),
        tail: None,
    }
}

/// Lower an entire Anubis program to a self-contained Rust program for `anubis run`.
///
/// Every Anubis function becomes a Rust function returning `AnubisValue`, so user-defined
/// calls and recursion execute on the Rust call stack; `let` bindings are `mut` so assignment
/// works; `while`/`loop` map to native Rust loops. Together with conditionals and unbounded
/// heap growth (`AnubisValue::Str`/recursion depth), this makes the executable language
/// Turing-complete. `anb_main` is the entry function; real `fn main()` just calls it.
///
/// When `allow_research` is true, the PoC kit surface is enabled: `target_run`, packing
/// (`p8`/`p16`/`p32`/`p64`), `cyclic`, research/exploit block bodies, and local-only process control.
pub fn lower_program_to_rust(items: &[Item], allow_research: bool) -> Result<String> {
    lower_program_to_rust_with_mono(items, allow_research, &[], &[])
}

/// Lower with a static monomorphization inventory from typecheck (`TypedIR.mono_specializations`
/// + ordered `mono_call_sites` for variable-pinned dispatch).
///
/// Emits specialized clones and rewrites call sites (literal and variable-pinned) to those clones.
pub fn lower_program_to_rust_with_mono(
    items: &[Item],
    allow_research: bool,
    mono: &[crate::middle::MonoSpecialization],
    mono_call_sites: &[crate::middle::MonoCallSite],
) -> Result<String> {
    // Run the program on a worker thread with a large (1 GiB) stack instead of the OS main-thread
    // stack (8 MiB on macOS), so naturally-recursive Anubis code reaches ~1M frames before
    // overflowing (the native call stack IS the recursion, per the Turing-completeness claim). The
    // stack is lazily committed, so this reserves address space, not RAM. A fail-closed trap panics
    // the worker: the default panic hook still prints the ANUBIS_* message to stderr, `join()`
    // returns Err, and we exit non-zero — so fail-closed behavior and diagnostics are preserved.
    // No AnubisValue crosses the thread boundary (values are created and dropped inside the worker),
    // so the Rc in AnubisValue::Closure never needs to be Send.
    lower_program_with_entry(
        items,
        "",
        "fn main() {\n    \
             let child = std::thread::Builder::new()\n        \
                 .stack_size(1024 * 1024 * 1024)\n        \
                 .spawn(|| { let _ = anb_main(); })\n        \
                 .expect(\"anubis: failed to spawn main thread\");\n    \
             if child.join().is_err() { std::process::exit(101); }\n}\n",
        allow_research,
        false,
        mono,
        mono_call_sites,
        // 768 MiB of the 1 GiB worker stack. The 256 MiB left over is deliberate headroom: the
        // trap must have room to panic, unwind and print, which is the whole point of trapping
        // before the real ceiling instead of after it.
        768 * 1024 * 1024,
    )
}

/// Lower an Anubis program's `main` into a RISC0 zkVM guest that runs the real program and
/// commits its result to the journal. risc0-build derives the ImageID from this guest's ELF,
/// so the ImageID — and therefore the receipt — is cryptographically bound to THIS program,
/// not a fixed demonstration circuit. Uses the reference guest's `std` feature.
///
/// Parameterized inputs: guest first runs `anubis_load_proof_inputs()` which reads
/// `(u32 n, (String,i64)*n)` from `env::read`, then `proof_input_u32("k")` looks up keys.
///
/// Journal (v2 multi-field):
/// - scalar `return` → one `env::commit(u32)` (v1-compatible)
/// - list `return [a, b, …]` → one `env::commit(u32)` per element (public multi-field journal)
pub fn lower_program_to_guest(items: &[Item]) -> Result<String> {
    lower_program_with_entry(
        items,
        "use risc0_zkvm::guest::env;\nuse std::collections::HashMap;\nuse std::sync::OnceLock;\n",
        concat!(
            "fn main() {\n",
            "    anubis_load_proof_inputs();\n",
            "    let __anubis_result = anb_main();\n",
            "    anubis_commit_journal(__anubis_result);\n",
            "}\n",
        ),
        false, // no process PoC kit inside zkVM guest
        true,  // inject proof-input runtime for guest
        &[],   // guest path: no mono inventory yet (safe default)
        &[],
        // 0 = guard disabled. The zkVM guest's stack size is not something this file measures, and
        // a budget picked by guessing would trap correct programs or miss the case it exists for.
        // Left explicitly unguarded and stated, rather than guarded on an invented number.
        0,
    )
}

/// Shared lowering: emit the AnubisValue runtime + every function, framed by a caller-provided
/// `prelude` (e.g. a guest `use`) and `entry` (the real `fn main`).
fn lower_program_with_entry(
    items: &[Item],
    prelude: &str,
    entry: &str,
    allow_research: bool,
    guest_proof_inputs: bool,
    mono: &[crate::middle::MonoSpecialization],
    mono_call_sites: &[crate::middle::MonoCallSite],
    stack_budget_bytes: usize,
) -> Result<String> {
    let mut fns = Vec::new();
    collect_fns(items, &mut fns);
    if !fns
        .iter()
        .any(|d| d.name == "main" && d.impl_type.is_none())
    {
        return Err(unsupported_run("program has no `fn main()` to run"));
    }
    // Free-function names only (methods are dispatched by receiver type, never called bare).
    let fn_names: std::collections::BTreeSet<String> = fns
        .iter()
        .filter(|d| d.impl_type.is_none())
        .map(|d| d.name.to_string())
        .collect();
    // Parameter counts for those free functions, so a bare-name reference in value position
    // (`map(xs, my_fn)`) can be lowered into a closure with the right arity.
    let fn_arities: std::collections::BTreeMap<String, usize> = fns
        .iter()
        .filter(|d| d.impl_type.is_none())
        .map(|d| (d.name.to_string(), d.params.len()))
        .collect();
    let fn_param_types: std::collections::BTreeMap<String, Vec<String>> = fns
        .iter()
        .filter(|d| d.impl_type.is_none())
        .map(|d| {
            (
                d.name.to_string(),
                d.params.iter().map(|(_, t)| t.clone()).collect(),
            )
        })
        .collect();
    // Build mono dispatch table (sorted for deterministic emit/match).
    let mut mono_table: Vec<MonoEmitSpec> = Vec::new();
    let mut mono_seen = std::collections::BTreeSet::new();
    for m in mono {
        if m.type_args.is_empty() {
            continue;
        }
        // Only free functions that exist with generics.
        let Some(def) = fns
            .iter()
            .find(|d| d.name == m.function && d.impl_type.is_none() && !d.generics.is_empty())
        else {
            continue;
        };
        let pairs: Vec<(String, String)> = m
            .type_args
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let key = (m.function.clone(), pairs.clone());
        if !mono_seen.insert(key) {
            continue;
        }
        let rust_name = mono_rust_name(def.name, &m.type_args)?;
        let params_owned: Vec<(String, String)> = def
            .params
            .iter()
            .map(|(n, t)| (n.clone(), subst_type_param(t, &m.type_args)))
            .collect();
        let ret_owned = def.ret_type.map(|t| subst_type_param(t, &m.type_args));
        let unboxed = mono_unboxed_abi(&params_owned, ret_owned.as_deref());
        mono_table.push(MonoEmitSpec {
            function: m.function.clone(),
            type_args: pairs,
            rust_name,
            unboxed,
        });
    }
    mono_table.sort_by(|a, b| {
        a.function
            .cmp(&b.function)
            .then(a.rust_name.cmp(&b.rust_name))
    });
    // Ordered mono call sites by enclosing caller (typecheck walk order).
    let mut mono_sites_by_caller: MonoSitesByCaller = std::collections::BTreeMap::new();
    for site in mono_call_sites {
        let pairs: Vec<(String, String)> = site
            .type_args
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        mono_sites_by_caller
            .entry(site.caller.clone())
            .or_default()
            .push((site.function.clone(), pairs));
    }
    let mono_cursors: std::collections::BTreeMap<String, std::cell::Cell<usize>> =
        mono_sites_by_caller
            .keys()
            .map(|k| (k.clone(), std::cell::Cell::new(0)))
            .collect();
    let mut methods = std::collections::BTreeMap::new();
    collect_methods(items, &mut methods);
    // struct name → (field → declared type), for the construction-boundary numeric-kind coercion.
    let mut struct_field_types: std::collections::BTreeMap<
        String,
        std::collections::BTreeMap<String, String>,
    > = std::collections::BTreeMap::new();
    for item in items {
        if let Item::Struct { name, fields, .. } = item {
            struct_field_types.insert(
                name.clone(),
                fields.iter().map(|(f, t)| (f.clone(), t.clone())).collect(),
            );
        }
    }
    let empty_locals = std::collections::BTreeSet::new();
    let base_ctx = EmitCtx {
        allow_research,
        fns: &fn_names,
        fn_arities: &fn_arities,
        methods: &methods,
        locals: &empty_locals,
        struct_field_types: &struct_field_types,
        mono: &mono_table,
        fn_param_types: &fn_param_types,
        current_fn: None,
        mono_sites_by_caller: &mono_sites_by_caller,
        mono_cursors: &mono_cursors,
    };
    let mut functions_src = String::new();
    for def in &fns {
        let ctx = EmitCtx {
            current_fn: Some(def.name),
            ..base_ctx
        };
        functions_src.push_str(&emit_fn(def, &ctx)?);
        functions_src.push('\n');
    }
    // Monomorphized clones: specialized params/return; primitive sets use unboxed native ABI.
    // Fresh mono cursors per clone so nested generic calls rewrite the same way as in the
    // original body (shared cursors would already be exhausted by the generic emit).
    for spec in &mono_table {
        let Some(def) = fns
            .iter()
            .find(|d| d.name == spec.function && d.impl_type.is_none())
        else {
            continue;
        };
        let type_args: std::collections::BTreeMap<String, String> =
            spec.type_args.iter().cloned().collect();
        let params_owned: Vec<(String, String)> = def
            .params
            .iter()
            .map(|(n, t)| (n.clone(), subst_type_param(t, &type_args)))
            .collect();
        let ret_owned = def.ret_type.map(|t| subst_type_param(t, &type_args));
        let clone_cursors: std::collections::BTreeMap<String, std::cell::Cell<usize>> =
            mono_sites_by_caller
                .keys()
                .map(|k| (k.clone(), std::cell::Cell::new(0)))
                .collect();
        let ctx = EmitCtx {
            current_fn: Some(def.name),
            mono_cursors: &clone_cursors,
            ..base_ctx
        };
        functions_src.push_str(&emit_fn_core(
            def.name,
            &params_owned,
            def.body,
            ret_owned.as_deref(),
            &spec.rust_name,
            &ctx,
            spec.unboxed.as_ref(),
        )?);
        functions_src.push('\n');
    }
    // Packing/cyclic helpers are always available (local data transforms). Process spawn
    // (`target_run`) remains gated at the call site by `allow_research` (see `safe_run_expr`).
    // Always inject the runtime so `std.pwn` pure helpers and unused pack wrappers in the same
    // module lower without requiring `--allow-research` for the whole program.
    let poc_kit_runtime = POC_KIT_RUNTIME_RS;
    let proof_input_runtime = if guest_proof_inputs {
        PROOF_INPUT_GUEST_RUNTIME_RS
    } else {
        // Native `anubis run`: commits are no-op (return value); asserts still fail-closed.
        NATIVE_PROOF_STUBS_RS
    };
    // Native run: audited crates (RWC Ch16). RISC0 guest: pure DIY (no cargo deps on guest).
    let crypto_runtime = if guest_proof_inputs {
        format!(
            "{pure}\n{pwd}",
            pure = ANUBIS_PURE_CRYPTO_RS,
            pwd = ANUBIS_PASSWORD_CRYPTO_PURE_RS
        )
    } else {
        ANUBIS_AUDITED_CRYPTO_RS.to_string()
    };
    // Keychain/SE runtime is native-only (Security.framework). RISC0 guest keeps soft tokens.
    let keychain_se = if guest_proof_inputs {
        // Guest: soft nonexportable mint only (no Security.framework in zkVM).
        r#"
fn anubis_keychain_se_probe() -> AnubisValue { AnubisValue::Int(0) }
fn anubis_keychain_se_last_bind() -> AnubisValue { anubis_mk_str("soft".to_string()) }
fn anubis_cap_acquire(kind: AnubisValue) -> AnubisValue {
    anubis_mk_str(format!("__anubis_cap:{}", kind.display_string()))
}
fn anubis_cap_acquire_nonexportable(kind: AnubisValue) -> AnubisValue {
    anubis_mk_str(format!("__anubis_cap_ne_soft:{}", kind.display_string()))
}
fn anubis_cap_export(cap: AnubisValue, _reason: AnubisValue) -> AnubisValue { cap }
"#
        .to_string()
    } else {
        ANUBIS_KEYCHAIN_SE_RS.to_string()
    };
    Ok(format!(
        "{header}{prelude}\n{core}\n{keychain}\n{crypto}\n{poc}\n{proof}\n{functions}\n{entry}",
        header = format!("#![allow(dead_code, unused_mut, unused_variables, unused_assignments, unreachable_code, unused_parens, unused_imports, non_snake_case, unused_braces)]\nconst __ANB_STACK_BUDGET: usize = {stack_budget_bytes};\n"),
        prelude = prelude,
        core = ANUBIS_CORE_RUNTIME_RS,
        keychain = keychain_se,
        crypto = crypto_runtime,
        poc = poc_kit_runtime,
        proof = proof_input_runtime,
        functions = functions_src,
        entry = entry,
    ))
}

/// Pure SHA-256/HMAC/HKDF/AEAD for RISC0 guests (no external crates).
const ANUBIS_PURE_CRYPTO_RS: &str = include_str!("pure_crypto_runtime.inc.rs");
/// Pure password KDFs for guests (DIY Argon2id/PBKDF2 — zkVM has no argon2 crate lane).
const ANUBIS_PASSWORD_CRYPTO_PURE_RS: &str = include_str!("password_crypto_runtime.inc.rs");
/// Native `anubis run`: audited crates only (argon2, chacha20poly1305, hmac, sha2, hkdf, …).
const ANUBIS_AUDITED_CRYPTO_RS: &str = include_str!("audited_crypto_runtime.inc.rs");
/// Keychain / Secure Enclave bind for non-exportable caps (macOS; soft fallback elsewhere).
const ANUBIS_KEYCHAIN_SE_RS: &str = include_str!("keychain_se_runtime.inc.rs");

/// The Anubis runtime value model + operator helpers, shared by native `run` and RISC0
/// guest lowering. Emitted verbatim into every generated Rust program.
const ANUBIS_CORE_RUNTIME_RS: &str = r#"
#[derive(Clone)]
enum AnubisValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    // The three heap-backed kinds share their payload through `Rc`, so cloning an AnubisValue
    // (which the generated code does on every variable read and argument pass) is an O(1) refcount
    // bump rather than a deep copy. Mutation goes through `Rc::make_mut` (copy-on-write): a uniquely
    // held payload is edited in place, a shared one is cloned first. Observable semantics are
    // identical to owning `String`/`Vec` directly; only the cost of clone changes.
    Str(std::rc::Rc<String>),
    List(std::rc::Rc<Vec<AnubisValue>>),
    /// Algebraic data: unit/tuple/struct variants.
    /// `field_names` non-empty only for struct-like variants (parallel to `fields`).
    Enum {
        ty: String,
        tag: String,
        fields: Vec<AnubisValue>,
        field_names: Vec<String>,
    },
    /// A nominal struct value with ordered, named fields.
    Struct {
        ty: String,
        fields: Vec<(String, AnubisValue)>,
    },
    /// Dictionary: string keys (via display_string) -> values, insertion-ordered.
    Map(std::rc::Rc<Vec<(String, AnubisValue)>>),
    /// A first-class function value (lambda), callable with a positional argument vector.
    Closure(std::rc::Rc<dyn Fn(Vec<AnubisValue>) -> AnubisValue>),
}

/// Construct the Rc-backed heap kinds. Named with an `anubis_` prefix (never `anb_<ident>`, the
/// shape reserved for lowered user functions) so they cannot collide with a user-defined function.
#[inline]
fn anubis_mk_str(s: String) -> AnubisValue { AnubisValue::Str(std::rc::Rc::new(s)) }
#[inline]
fn anubis_mk_list(v: Vec<AnubisValue>) -> AnubisValue { AnubisValue::List(std::rc::Rc::new(v)) }
#[inline]
fn anubis_mk_map(v: Vec<(String, AnubisValue)>) -> AnubisValue { AnubisValue::Map(std::rc::Rc::new(v)) }
/// Move the contents out of an `Rc` without cloning when it is uniquely held; clone only when the
/// payload is still shared (copy-on-write for the by-value consuming builtins).
#[inline]
fn anubis_rc_take<T: Clone>(rc: std::rc::Rc<T>) -> T {
    std::rc::Rc::try_unwrap(rc).unwrap_or_else(|rc| (*rc).clone())
}

impl std::fmt::Debug for AnubisValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_string())
    }
}

impl AnubisValue {
    fn call_closure(&self, args: Vec<AnubisValue>) -> AnubisValue {
        match self {
            AnubisValue::Closure(f) => f(args),
            _ => panic!("ANUBIS_TYPE_ERROR: expected closure, got {}", self.type_name()),
        }
    }

    fn try_call_closure(&self, args: Vec<AnubisValue>) -> AnubisValue {
        match self {
            AnubisValue::Closure(f) => f(args),
            _ => AnubisValue::Int(0),
        }
    }

    #[inline]
    fn is_closure(&self) -> bool {
        matches!(self, AnubisValue::Closure(_))
    }

    fn as_i64(&self) -> i64 {
        match self {
            AnubisValue::Int(v) => *v,
            AnubisValue::Float(v) => *v as i64,
            AnubisValue::Bool(v) => i64::from(*v),
            AnubisValue::Str(v) => v.trim().parse::<i64>().unwrap_or_else(|_| v.trim().parse::<f64>().map(|f| f as i64).unwrap_or(0)),
            AnubisValue::List(v) => v.len() as i64,
            AnubisValue::Enum { fields, .. } => fields.first().map(|f| f.as_i64()).unwrap_or(0),
            AnubisValue::Struct { fields, .. } => fields.len() as i64,
            AnubisValue::Map(m) => m.len() as i64,
            AnubisValue::Closure(_) => 0,
        }
    }

    fn as_f64(&self) -> f64 {
        match self {
            AnubisValue::Float(v) => *v,
            AnubisValue::Int(v) => *v as f64,
            AnubisValue::Bool(v) => if *v { 1.0 } else { 0.0 },
            AnubisValue::Str(v) => v.trim().parse::<f64>().unwrap_or(0.0),
            other => other.as_i64() as f64,
        }
    }

    fn is_numeric(&self) -> bool {
        matches!(self, AnubisValue::Int(_) | AnubisValue::Float(_) | AnubisValue::Bool(_))
    }

    fn is_float(&self) -> bool {
        matches!(self, AnubisValue::Float(_))
    }

    fn as_bool(&self) -> bool {
        match self {
            AnubisValue::Bool(v) => *v,
            AnubisValue::Int(v) => *v != 0,
            AnubisValue::Float(v) => *v != 0.0,
            AnubisValue::Str(v) => !v.is_empty(),
            AnubisValue::List(v) => !v.is_empty(),
            AnubisValue::Enum { .. } => true,
            AnubisValue::Struct { .. } => true,
            AnubisValue::Map(m) => !m.is_empty(),
            AnubisValue::Closure(_) => true,
        }
    }

    fn type_name(&self) -> &'static str {
        match self {
            AnubisValue::Int(_) => "int",
            AnubisValue::Float(_) => "float",
            AnubisValue::Bool(_) => "bool",
            AnubisValue::Str(_) => "string",
            AnubisValue::List(_) => "list",
            AnubisValue::Enum { .. } => "enum",
            AnubisValue::Struct { .. } => "struct",
            AnubisValue::Map(_) => "map",
            AnubisValue::Closure(_) => "closure",
        }
    }

    fn display_string(&self) -> String {
        match self {
            AnubisValue::Int(v) => v.to_string(),
            AnubisValue::Float(v) => anubis_float_str(*v),
            AnubisValue::Bool(v) => v.to_string(),
            AnubisValue::Str(v) => v.to_string(),
            AnubisValue::List(v) => {
                let parts: Vec<String> = v.iter().map(|x| x.display_string()).collect();
                format!("[{}]", parts.join(", "))
            }
            AnubisValue::Enum { ty, tag, fields, field_names } => {
                // The built-in Option/Result prelude variants are written and matched bare
                // (`Some(x)`, `None`, `Ok(x)`, `Err(e)`), so they render bare too; user enums
                // render as `Type::Variant`, the form you construct them with.
                let prefix = if ty.as_str() == "Option" || ty.as_str() == "Result" {
                    String::new()
                } else {
                    format!("{}::", ty)
                };
                if fields.is_empty() {
                    format!("{}{}", prefix, tag)
                } else if !field_names.is_empty() {
                    let parts: Vec<String> = field_names.iter().zip(fields.iter())
                        .map(|(n, v)| format!("{}: {}", n, v.display_string()))
                        .collect();
                    format!("{}{} {{ {} }}", prefix, tag, parts.join(", "))
                } else {
                    let parts: Vec<String> = fields.iter().map(|x| x.display_string()).collect();
                    format!("{}{}({})", prefix, tag, parts.join(", "))
                }
            }
            AnubisValue::Struct { ty, fields } => {
                let parts: Vec<String> = fields.iter()
                    .map(|(n, v)| format!("{}: {}", n, v.display_string()))
                    .collect();
                format!("{} {{ {} }}", ty, parts.join(", "))
            }
            AnubisValue::Map(m) => {
                // Quote keys so the printed form matches the map literal you'd write: {"a": 1}.
                let parts: Vec<String> = m.iter()
                    .map(|(k, v)| format!("{:?}: {}", k, v.display_string()))
                    .collect();
                format!("{{{}}}", parts.join(", "))
            }
            AnubisValue::Closure(_) => "<closure>".to_string(),
        }
    }

    /// Positional element access for list/tuple destructuring: only lists yield elements.
    /// Any non-list value, or an out-of-range index, yields the default `0` — this is the
    /// irrefutable "not-a-list -> 0" contract, and (unlike `index_get`) never char-slices a string.
    fn list_elem(&self, i: i64) -> AnubisValue {
        match self {
            AnubisValue::List(v) if i >= 0 && (i as usize) < v.len() => v[i as usize].clone(),
            _ => AnubisValue::Int(0),
        }
    }

    fn index_get(&self, i: AnubisValue) -> AnubisValue {
        match self {
            // Fail-closed: an explicit `xs[i]` on a list asserts `i` is in range.
            // Out-of-bounds is a bug, not a silent 0. Use get(xs, i, default) for optional access.
            AnubisValue::List(v) => {
                match anubis_norm_index(i.as_i64(), v.len()) {
                    Some(k) => v[k].clone(),
                    None => panic!(
                        "ANUBIS_INDEX_OUT_OF_BOUNDS: index {} is out of bounds for a list of length {} (use get(xs, i, default) for optional access)",
                        i.as_i64(), v.len()
                    ),
                }
            }
            // Fail-closed: `s[i]` / char_at(s, i) asserts `i` is a valid character position.
            AnubisValue::Str(s) => {
                let chars: Vec<char> = s.chars().collect();
                match anubis_norm_index(i.as_i64(), chars.len()) {
                    Some(k) => anubis_mk_str(chars[k].to_string()),
                    None => panic!(
                        "ANUBIS_INDEX_OUT_OF_BOUNDS: index {} is out of bounds for a string of length {}",
                        i.as_i64(), chars.len()
                    ),
                }
            }
            // Fail-closed: `m[k]` asserts key `k` is present. Missing key is a bug, not a silent 0.
            // Use get(m, k, default) or has_key(m, k) for optional access.
            AnubisValue::Map(m) => {
                let key = i.display_string();
                match m.iter().find(|(k, _)| k == &key) {
                    Some((_, v)) => v.clone(),
                    None => panic!(
                        "ANUBIS_MISSING_KEY: map has no key {:?} (use get(m, k, default) or has_key(m, k) for optional access)",
                        key
                    ),
                }
            }
            // A+: struct field order supports list-style r[0] (TargetRun and friends).
            // Kept as a compat accessor: a missing struct index/key stays 0 (documented list-view semantics).
            AnubisValue::Struct { fields, .. } => {
                let idx = i.as_i64();
                if idx >= 0 && (idx as usize) < fields.len() {
                    fields[idx as usize].1.clone()
                } else {
                    let key = i.display_string();
                    fields.iter().find(|(k, _)| k == &key).map(|(_, v)| v.clone()).unwrap_or(AnubisValue::Int(0))
                }
            }
            // Fail-closed: indexing a value that is not a collection is a type error, not a silent 0.
            other => panic!(
                "ANUBIS_NOT_INDEXABLE: cannot index a value of type {} with []",
                other.type_name()
            ),
        }
    }

    fn index_set(&mut self, i: AnubisValue, val: AnubisValue) {
        match self {
            AnubisValue::List(v) => {
                if let Some(k) = anubis_norm_index(i.as_i64(), v.len()) {
                    std::rc::Rc::make_mut(v)[k] = val;
                }
            }
            AnubisValue::Map(m) => {
                let key = i.display_string();
                let m = std::rc::Rc::make_mut(m);
                if let Some(slot) = m.iter_mut().find(|(k, _)| k == &key) {
                    slot.1 = val;
                } else {
                    m.push((key, val));
                }
            }
            _ => {}
        }
    }

    /// Read a named field of a struct, struct-enum variant, or map.
    fn field_get(&self, name: &str) -> AnubisValue {
        match self {
            AnubisValue::Struct { fields, .. } =>
                fields.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone()).unwrap_or(AnubisValue::Int(0)),
            AnubisValue::Enum { fields, field_names, .. } =>
                field_names.iter().position(|n| n == name).and_then(|i| fields.get(i)).cloned().unwrap_or(AnubisValue::Int(0)),
            AnubisValue::Map(m) =>
                m.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone()).unwrap_or(AnubisValue::Int(0)),
            _ => AnubisValue::Int(0),
        }
    }

    /// Mutate a named field of a struct (or map). No-op on other kinds.
    fn field_set(&mut self, name: &str, val: AnubisValue) {
        match self {
            AnubisValue::Struct { fields, .. } => {
                if let Some(slot) = fields.iter_mut().find(|(k, _)| k == name) { slot.1 = val; }
                else { fields.push((name.to_string(), val)); }
            }
            AnubisValue::Map(m) => {
                let m = std::rc::Rc::make_mut(m);
                if let Some(slot) = m.iter_mut().find(|(k, _)| k == name) { slot.1 = val; }
                else { m.push((name.to_string(), val)); }
            }
            _ => {}
        }
    }

    fn push_val(&mut self, val: AnubisValue) {
        match self {
            AnubisValue::List(v) => { std::rc::Rc::make_mut(v).push(val); }
            other => panic!("ANUBIS_TYPE_ERROR: push expects a list, got {}", other.type_name()),
        }
    }

    fn len_val(&self) -> AnubisValue {
        match self {
            AnubisValue::List(v) => AnubisValue::Int(v.len() as i64),
            AnubisValue::Str(s) => AnubisValue::Int(s.chars().count() as i64),
            AnubisValue::Map(m) => AnubisValue::Int(m.len() as i64),
            AnubisValue::Struct { fields, .. } => AnubisValue::Int(fields.len() as i64),
            AnubisValue::Enum { fields, .. } => AnubisValue::Int(fields.len() as i64),
            // Was `Int(0)` — `len(42)` / `len(true)` silently reported empty (Phase-5 SILENT_WRONG).
            other => panic!(
                "ANUBIS_TYPE_ERROR: len expects a list, string, map, struct, or enum, got {}",
                other.type_name()
            ),
        }
    }

    /// Keys of a map as a list of strings (for `for k in m`).
    fn map_keys(&self) -> AnubisValue {
        match self {
            AnubisValue::Map(m) => anubis_mk_list(
                m.iter().map(|(k, _)| anubis_mk_str(k.clone())).collect()
            ),
            other => panic!("ANUBIS_TYPE_ERROR: keys expects a map, got {}", other.type_name()),
        }
    }
}

/// Render an f64 so it always reads back as a float (whole values keep a trailing `.0`).
fn anubis_float_str(v: f64) -> String {
    if v.is_nan() { return "NaN".to_string(); }
    if v.is_infinite() { return if v < 0.0 { "-inf".to_string() } else { "inf".to_string() }; }
    let s = format!("{}", v);
    if s.contains('.') || s.contains('e') || s.contains('E') { s } else { format!("{}.0", s) }
}

/// Normalize an index against a length: supports negative indexing from the end.
/// Returns None when out of range.
fn anubis_norm_index(idx: i64, len: usize) -> Option<usize> {
    let k = if idx < 0 { idx + len as i64 } else { idx };
    if k >= 0 && (k as usize) < len { Some(k as usize) } else { None }
}

fn anubis_add(lhs: AnubisValue, rhs: AnubisValue) -> AnubisValue {
    match (lhs, rhs) {
        (AnubisValue::List(a), AnubisValue::List(b)) => { let mut a = anubis_rc_take(a); a.extend(anubis_rc_take(b)); anubis_mk_list(a) }
        (AnubisValue::List(a), b) => { let mut a = anubis_rc_take(a); a.push(b); anubis_mk_list(a) }
        (AnubisValue::Str(a), b) => anubis_mk_str(format!("{}{}", a, b.display_string())),
        (a, AnubisValue::Str(b)) => anubis_mk_str(format!("{}{}", a.display_string(), b)),
        (a, b) => {
            if a.is_float() || b.is_float() {
                AnubisValue::Float(a.as_f64() + b.as_f64())
            } else {
                AnubisValue::Int(a.as_i64().wrapping_add(b.as_i64()))
            }
        }
    }
}

fn anubis_sub(lhs: AnubisValue, rhs: AnubisValue) -> AnubisValue {
    if lhs.is_float() || rhs.is_float() {
        AnubisValue::Float(lhs.as_f64() - rhs.as_f64())
    } else {
        AnubisValue::Int(lhs.as_i64().wrapping_sub(rhs.as_i64()))
    }
}

fn anubis_mul(lhs: AnubisValue, rhs: AnubisValue) -> AnubisValue {
    if lhs.is_float() || rhs.is_float() {
        AnubisValue::Float(lhs.as_f64() * rhs.as_f64())
    } else {
        AnubisValue::Int(lhs.as_i64().wrapping_mul(rhs.as_i64()))
    }
}

fn anubis_div(lhs: AnubisValue, rhs: AnubisValue) -> AnubisValue {
    if lhs.is_float() || rhs.is_float() {
        AnubisValue::Float(lhs.as_f64() / rhs.as_f64())
    } else {
        let d = rhs.as_i64();
        if d == 0 { panic!("ANUBIS_DIV_BY_ZERO: integer division by zero"); }
        AnubisValue::Int(lhs.as_i64().wrapping_div(d))
    }
}

fn anubis_mod(lhs: AnubisValue, rhs: AnubisValue) -> AnubisValue {
    if lhs.is_float() || rhs.is_float() {
        AnubisValue::Float(lhs.as_f64() % rhs.as_f64())
    } else {
        let d = rhs.as_i64();
        if d == 0 { panic!("ANUBIS_MOD_BY_ZERO: integer remainder by zero"); }
        AnubisValue::Int(lhs.as_i64().wrapping_rem(d))
    }
}

fn anubis_band(lhs: AnubisValue, rhs: AnubisValue) -> AnubisValue {
    AnubisValue::Int(lhs.as_i64() & rhs.as_i64())
}
fn anubis_bor(lhs: AnubisValue, rhs: AnubisValue) -> AnubisValue {
    AnubisValue::Int(lhs.as_i64() | rhs.as_i64())
}
fn anubis_bxor(lhs: AnubisValue, rhs: AnubisValue) -> AnubisValue {
    AnubisValue::Int(lhs.as_i64() ^ rhs.as_i64())
}
fn anubis_shl(lhs: AnubisValue, rhs: AnubisValue) -> AnubisValue {
    let s = rhs.as_i64().rem_euclid(64) as u32;
    AnubisValue::Int(lhs.as_i64().wrapping_shl(s))
}
fn anubis_shr(lhs: AnubisValue, rhs: AnubisValue) -> AnubisValue {
    let s = rhs.as_i64().rem_euclid(64) as u32;
    AnubisValue::Int(lhs.as_i64().wrapping_shr(s))
}
fn anubis_bnot(v: AnubisValue) -> AnubisValue {
    AnubisValue::Int(!v.as_i64())
}

fn anubis_neg(v: AnubisValue) -> AnubisValue {
    if v.is_float() { AnubisValue::Float(-v.as_f64()) }
    else { AnubisValue::Int(v.as_i64().wrapping_neg()) }
}

fn anubis_is_int(v: &AnubisValue) -> bool {
    matches!(v, AnubisValue::Int(_) | AnubisValue::Bool(_))
}

/// Total order over two values. Integer/integer stays exact (no f64 precision loss above 2^53);
/// mixed numeric uses f64; two lists compare element-wise (lexicographic over element order, each
/// element by this same order — consistent with structural equality, so a tuple/list sort key
/// like `[grp, val]` orders as expected); everything else compares by display form.
fn anubis_value_cmp(a: &AnubisValue, b: &AnubisValue) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    if anubis_is_int(a) && anubis_is_int(b) {
        a.as_i64().cmp(&b.as_i64())
    } else if a.is_numeric() && b.is_numeric() {
        a.as_f64().partial_cmp(&b.as_f64()).unwrap_or(Ordering::Equal)
    } else if let (AnubisValue::List(x), AnubisValue::List(y)) = (a, b) {
        for (p, q) in x.iter().zip(y.iter()) {
            match anubis_value_cmp(p, q) {
                Ordering::Equal => continue,
                ord => return ord,
            }
        }
        x.len().cmp(&y.len())
    } else {
        a.display_string().cmp(&b.display_string())
    }
}

/// Structural, type-aware equality (backs `==`/`!=`). Unlike the ordering used for `< > <= >=`
/// (which falls back to display form to give a total order), equality does NOT collapse across
/// types: a string never equals a number, a bool never equals an int, and compound values are
/// compared element-by-element. Int and float remain equal when numerically equal (`5 == 5.0`).
fn anubis_value_eq(a: &AnubisValue, b: &AnubisValue) -> bool {
    match (a, b) {
        (AnubisValue::Int(x), AnubisValue::Int(y)) => x == y,
        (AnubisValue::Bool(x), AnubisValue::Bool(y)) => x == y,
        (AnubisValue::Float(_), AnubisValue::Float(_))
        | (AnubisValue::Int(_), AnubisValue::Float(_))
        | (AnubisValue::Float(_), AnubisValue::Int(_)) => a.as_f64() == b.as_f64(),
        (AnubisValue::Str(x), AnubisValue::Str(y)) => x == y,
        (AnubisValue::List(x), AnubisValue::List(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| anubis_value_eq(p, q))
        }
        (AnubisValue::Map(x), AnubisValue::Map(y)) => {
            x.len() == y.len()
                && x.iter().all(|(k, v)| {
                    y.iter().any(|(k2, v2)| k == k2 && anubis_value_eq(v, v2))
                })
        }
        (
            AnubisValue::Enum { ty, tag, fields, .. },
            AnubisValue::Enum { ty: ty2, tag: tag2, fields: f2, .. },
        ) => {
            ty == ty2
                && tag == tag2
                && fields.len() == f2.len()
                && fields.iter().zip(f2.iter()).all(|(p, q)| anubis_value_eq(p, q))
        }
        (
            AnubisValue::Struct { ty, fields },
            AnubisValue::Struct { ty: ty2, fields: f2 },
        ) => {
            // Structs have named fields, so equality is by name — order-independent — matching
            // field access, struct patterns, and let-destructuring (all name-based). Field names
            // are unique per struct, so a name-match with equal values on every field is exact.
            ty == ty2
                && fields.len() == f2.len()
                && fields.iter().all(|(n, v)| {
                    f2.iter().any(|(n2, v2)| n == n2 && anubis_value_eq(v, v2))
                })
        }
        // Closures are never equal; mismatched kinds (string vs int, bool vs int, …) are not equal.
        _ => false,
    }
}

fn anubis_cmp(op: &str, lhs: AnubisValue, rhs: AnubisValue) -> AnubisValue {
    use std::cmp::Ordering;
    let result = match op {
        "==" => anubis_value_eq(&lhs, &rhs),
        "!=" => !anubis_value_eq(&lhs, &rhs),
        _ => {
            let ord = anubis_value_cmp(&lhs, &rhs);
            match op {
                "<" => ord == Ordering::Less,
                "<=" => ord != Ordering::Greater,
                ">" => ord == Ordering::Greater,
                ">=" => ord != Ordering::Less,
                _ => false,
            }
        }
    };
    AnubisValue::Bool(result)
}

/// One step of an lvalue path: a named field or an index.
enum AnubisPathSeg {
    Field(String),
    Index(AnubisValue),
}

impl AnubisValue {
    /// Assign `val` at the given path, descending through structs, maps, lists, and strings,
    /// mutating in place. An empty path replaces the whole value.
    fn set_at(&mut self, path: &[AnubisPathSeg], val: AnubisValue) {
        match path.split_first() {
            None => {
                *self = val;
            }
            Some((AnubisPathSeg::Field(name), rest)) => match self {
                AnubisValue::Struct { fields, .. } => {
                    if let Some(slot) = fields.iter_mut().find(|(k, _)| k == name) {
                        slot.1.set_at(rest, val);
                    } else if rest.is_empty() {
                        fields.push((name.clone(), val));
                    }
                }
                AnubisValue::Map(m) => {
                    let m = std::rc::Rc::make_mut(m);
                    if let Some(slot) = m.iter_mut().find(|(k, _)| k == name) {
                        slot.1.set_at(rest, val);
                    } else if rest.is_empty() {
                        m.push((name.clone(), val));
                    }
                }
                _ => {}
            },
            Some((AnubisPathSeg::Index(i), rest)) => match self {
                AnubisValue::List(v) => {
                    if let Some(k) = anubis_norm_index(i.as_i64(), v.len()) {
                        std::rc::Rc::make_mut(v)[k].set_at(rest, val);
                    }
                }
                AnubisValue::Map(m) => {
                    let key = i.display_string();
                    let m = std::rc::Rc::make_mut(m);
                    if let Some(slot) = m.iter_mut().find(|(k, _)| k == &key) {
                        slot.1.set_at(rest, val);
                    } else if rest.is_empty() {
                        m.push((key, val));
                    }
                }
                AnubisValue::Str(s) if rest.is_empty() => {
                    let mut chars: Vec<char> = s.chars().collect();
                    if let Some(k) = anubis_norm_index(i.as_i64(), chars.len()) {
                        if let Some(c) = val.display_string().chars().next() {
                            chars[k] = c;
                            *std::rc::Rc::make_mut(s) = chars.into_iter().collect();
                        }
                    }
                }
                _ => {}
            },
        }
    }
}

// ---- Anubis standard library runtime (shared by native run + guest) ----

fn anubis_str(v: AnubisValue) -> AnubisValue { anubis_mk_str(v.display_string()) }
fn anubis_int(v: AnubisValue) -> AnubisValue { AnubisValue::Int(v.as_i64()) }
fn anubis_float(v: AnubisValue) -> AnubisValue { AnubisValue::Float(v.as_f64()) }
fn anubis_bool_of(v: AnubisValue) -> AnubisValue { AnubisValue::Bool(v.as_bool()) }
fn anubis_type_of(v: AnubisValue) -> AnubisValue { anubis_mk_str(v.type_name().to_string()) }

/// Fail closed when a math builtin is given a non-numeric value. Soft `as_f64`/`as_i64` would
/// coerce strings/lists/maps to 0 and let contracts discharge on the wrong input.
fn anubis_require_numeric(v: &AnubisValue, name: &str) {
    if !v.is_numeric() {
        panic!(
            "ANUBIS_TYPE_ERROR: {} expects a numeric argument, got {}",
            name,
            v.type_name()
        );
    }
}

fn anubis_abs(v: AnubisValue) -> AnubisValue {
    if !v.is_numeric() {
        panic!("ANUBIS_TYPE_ERROR: abs expects a numeric argument, got {}", v.type_name());
    }
    if v.is_float() { AnubisValue::Float(v.as_f64().abs()) } else { AnubisValue::Int(v.as_i64().wrapping_abs()) }
}
// Ordered via `anubis_value_cmp` — the same comparator `sort`/`min_by` use — so Int/Int compares
// exactly as i64 (an f64 round-trip loses distinctions above 2^53) and strings order lexically.
fn anubis_min2(a: AnubisValue, b: AnubisValue) -> AnubisValue { if anubis_value_cmp(&a, &b) != std::cmp::Ordering::Greater { a } else { b } }
fn anubis_max2(a: AnubisValue, b: AnubisValue) -> AnubisValue { if anubis_value_cmp(&a, &b) != std::cmp::Ordering::Less { a } else { b } }
fn anubis_seq(items: Vec<AnubisValue>) -> Vec<AnubisValue> {
    if items.len() == 1 { if let AnubisValue::List(l) = &items[0] { return (**l).clone(); } }
    items
}
fn anubis_min(items: Vec<AnubisValue>) -> AnubisValue {
    anubis_seq(items).into_iter().reduce(anubis_min2).unwrap_or_else(|| {
        panic!("ANUBIS_EMPTY_COLLECTION: min has no element — the collection is empty (use is_empty(xs) to guard)")
    })
}
fn anubis_max(items: Vec<AnubisValue>) -> AnubisValue {
    anubis_seq(items).into_iter().reduce(anubis_max2).unwrap_or_else(|| {
        panic!("ANUBIS_EMPTY_COLLECTION: max has no element — the collection is empty (use is_empty(xs) to guard)")
    })
}
fn anubis_pow(base: AnubisValue, exp: AnubisValue) -> AnubisValue {
    anubis_require_numeric(&base, "pow");
    anubis_require_numeric(&exp, "pow");
    if base.is_float() || exp.is_float() {
        AnubisValue::Float(base.as_f64().powf(exp.as_f64()))
    } else {
        let e = exp.as_i64();
        if e < 0 { AnubisValue::Float(base.as_f64().powi(e as i32)) }
        else { AnubisValue::Int(base.as_i64().wrapping_pow(e as u32)) }
    }
}
fn anubis_sqrt(v: AnubisValue) -> AnubisValue { anubis_require_numeric(&v, "sqrt"); AnubisValue::Float(v.as_f64().sqrt()) }
// floor/ceil/round/trunc are the identity on an integer (an i64 has no fractional part, and
// routing it through f64 would corrupt magnitudes above 2^53). Only floats are rounded.
fn anubis_floor(v: AnubisValue) -> AnubisValue { anubis_require_numeric(&v, "floor"); match v { AnubisValue::Int(n) => AnubisValue::Int(n), _ => AnubisValue::Int(v.as_f64().floor() as i64) } }
fn anubis_ceil(v: AnubisValue) -> AnubisValue { anubis_require_numeric(&v, "ceil"); match v { AnubisValue::Int(n) => AnubisValue::Int(n), _ => AnubisValue::Int(v.as_f64().ceil() as i64) } }
fn anubis_round(v: AnubisValue) -> AnubisValue { anubis_require_numeric(&v, "round"); match v { AnubisValue::Int(n) => AnubisValue::Int(n), _ => AnubisValue::Int(v.as_f64().round() as i64) } }
fn anubis_gcd(a: AnubisValue, b: AnubisValue) -> AnubisValue {
    anubis_require_numeric(&a, "gcd");
    anubis_require_numeric(&b, "gcd");
    let (mut x, mut y) = (a.as_i64().wrapping_abs(), b.as_i64().wrapping_abs());
    while y != 0 { let t = y; y = x % y; x = t; }
    AnubisValue::Int(x)
}

fn anubis_upper(v: AnubisValue) -> AnubisValue { anubis_mk_str(v.display_string().to_uppercase()) }
fn anubis_lower(v: AnubisValue) -> AnubisValue { anubis_mk_str(v.display_string().to_lowercase()) }
fn anubis_trim(v: AnubisValue) -> AnubisValue { anubis_mk_str(v.display_string().trim().to_string()) }
fn anubis_split(s: AnubisValue, sep: AnubisValue) -> AnubisValue {
    let hay = s.display_string();
    let sp = sep.display_string();
    let parts: Vec<AnubisValue> = if sp.is_empty() {
        hay.chars().map(|c| anubis_mk_str(c.to_string())).collect()
    } else {
        hay.split(sp.as_str()).map(|p| anubis_mk_str(p.to_string())).collect()
    };
    anubis_mk_list(parts)
}
fn anubis_join(list: AnubisValue, sep: AnubisValue) -> AnubisValue {
    let sp = sep.display_string();
    match list {
        AnubisValue::List(items) => anubis_mk_str(
            items.iter().map(|x| x.display_string()).collect::<Vec<_>>().join(sp.as_str())
        ),
        other => panic!(
            "ANUBIS_TYPE_ERROR: join expects a list as its first argument, got {}",
            other.type_name()
        ),
    }
}
fn anubis_contains(hay: AnubisValue, needle: AnubisValue) -> AnubisValue {
    let result = match &hay {
        // Substring test for strings; structural (`==`) membership for a list, so `2 != "2"`.
        AnubisValue::Str(s) => s.contains(needle.display_string().as_str()),
        AnubisValue::List(items) => items.iter().any(|x| anubis_value_eq(x, &needle)),
        AnubisValue::Map(m) => {
            let n = needle.display_string();
            m.iter().any(|(k, _)| k == &n)
        }
        other => panic!(
            "ANUBIS_TYPE_ERROR: contains expects a list, string, or map, got {}",
            other.type_name()
        ),
    };
    AnubisValue::Bool(result)
}
fn anubis_starts_with(s: AnubisValue, p: AnubisValue) -> AnubisValue {
    AnubisValue::Bool(s.display_string().starts_with(p.display_string().as_str()))
}
fn anubis_ends_with(s: AnubisValue, p: AnubisValue) -> AnubisValue {
    AnubisValue::Bool(s.display_string().ends_with(p.display_string().as_str()))
}
fn anubis_replace(s: AnubisValue, from: AnubisValue, to: AnubisValue) -> AnubisValue {
    anubis_mk_str(s.display_string().replace(from.display_string().as_str(), to.display_string().as_str()))
}
fn anubis_index_of(hay: AnubisValue, needle: AnubisValue) -> AnubisValue {
    match &hay {
        AnubisValue::Str(s) => {
            let n = needle.display_string();
            match s.find(n.as_str()) {
                Some(byte) => AnubisValue::Int(s[..byte].chars().count() as i64),
                None => AnubisValue::Int(-1),
            }
        }
        AnubisValue::List(items) => {
            match items.iter().position(|x| anubis_value_eq(x, &needle)) {
                Some(i) => AnubisValue::Int(i as i64),
                None => AnubisValue::Int(-1),
            }
        }
        other => panic!(
            "ANUBIS_TYPE_ERROR: index_of expects a list or string, got {} (do not confuse with not-found which is -1)",
            other.type_name()
        ),
    }
}
fn anubis_ord(v: AnubisValue) -> AnubisValue {
    match v.display_string().chars().next() {
        Some(c) => AnubisValue::Int(c as i64),
        None => panic!("ANUBIS_EMPTY_COLLECTION: ord(\"\") — the empty string has no first character"),
    }
}
fn anubis_chr(v: AnubisValue) -> AnubisValue {
    let n = v.as_i64();
    match char::from_u32(n as u32) {
        Some(c) => anubis_mk_str(c.to_string()),
        None => panic!("ANUBIS_INVALID_CODEPOINT: {} is not a valid Unicode scalar value (surrogate range D800-DFFF, negative, or > 0x10FFFF)", n),
    }
}
fn anubis_repeat(s: AnubisValue, n: AnubisValue) -> AnubisValue {
    let count_raw = n.as_i64();
    if count_raw < 0 {
        panic!("ANUBIS_INVALID_ARGUMENT: repeat count must be non-negative, got {}", count_raw);
    }
    let count = count_raw as usize;
    match s {
        AnubisValue::List(items) => {
            let mut out = Vec::new();
            for _ in 0..count { out.extend(items.iter().cloned()); }
            anubis_mk_list(out)
        }
        other => anubis_mk_str(other.display_string().repeat(count)),
    }
}
fn anubis_substr(s: AnubisValue, start: AnubisValue, len: AnubisValue) -> AnubisValue {
    let chars: Vec<char> = s.display_string().chars().collect();
    // Was `.max(0)` — negative start/len silently became empty-prefix (Phase-5 M–Z SILENT_WRONG).
    let st_raw = start.as_i64();
    if st_raw < 0 {
        panic!("ANUBIS_INVALID_ARGUMENT: substr start must be non-negative, got {}", st_raw);
    }
    let ln_raw = len.as_i64();
    if ln_raw < 0 {
        panic!("ANUBIS_INVALID_ARGUMENT: substr length must be non-negative, got {}", ln_raw);
    }
    let st = st_raw as usize;
    let ln = ln_raw as usize;
    anubis_mk_str(chars.into_iter().skip(st).take(ln).collect())
}
fn anubis_slice(x: AnubisValue, a: AnubisValue, b: AnubisValue) -> AnubisValue {
    let (ai, bi) = (a.as_i64(), b.as_i64());
    let bound = |i: i64, n: i64| -> usize { (if i < 0 { (i + n).max(0) } else { i.min(n) }) as usize };
    match x {
        AnubisValue::List(items) => {
            let n = items.len() as i64;
            let (lo, hi) = (bound(ai, n), bound(bi, n));
            anubis_mk_list(if lo <= hi { items[lo..hi].to_vec() } else { vec![] })
        }
        AnubisValue::Str(s) => {
            let chars: Vec<char> = s.chars().collect();
            let n = chars.len() as i64;
            let (lo, hi) = (bound(ai, n), bound(bi, n));
            anubis_mk_str(if lo <= hi { chars[lo..hi].iter().collect() } else { String::new() })
        }
        other => panic!(
            "ANUBIS_TYPE_ERROR: slice expects a list or string, got {}",
            other.type_name()
        ),
    }
}
fn anubis_parse_int(v: AnubisValue) -> AnubisValue {
    AnubisValue::Int(v.display_string().trim().parse::<i64>().unwrap_or(0))
}
/// Cast to an integer type of the given bit width: truncate floats toward zero, then wrap into the
/// unsigned range of `bits` (so `300 as u8` == 44, `-1 as u8` == 255). `bits >= 64` = no wrap.
fn anubis_cast_int(v: AnubisValue, bits: u32, signed: bool) -> AnubisValue {
    let n = v.as_i64();
    if bits == 0 || bits >= 64 {
        return AnubisValue::Int(n);
    }
    let mask: i64 = (1i64 << bits) - 1;
    let masked = n & mask;
    // A signed target reinterprets the top bit as the sign (two's complement), so `255 as i8` is
    // -1; an unsigned target keeps the plain masked value, so `300 as u8` is 44.
    if signed && (masked & (1i64 << (bits - 1))) != 0 {
        AnubisValue::Int(masked - (1i64 << bits))
    } else {
        AnubisValue::Int(masked)
    }
}
fn anubis_parse_float(v: AnubisValue) -> AnubisValue {
    AnubisValue::Float(v.display_string().trim().parse::<f64>().unwrap_or(0.0))
}
/// Fail-closed parse: `Some(n)` on success, `None` on malformed input (unlike lenient `parse_int`,
/// which returns 0). Lets a program distinguish "the number 0" from "not a number".
fn anubis_parse_int_opt(v: AnubisValue) -> AnubisValue {
    match v.display_string().trim().parse::<i64>() {
        Ok(n) => AnubisValue::Enum {
            ty: "Option".to_string(),
            tag: "Some".to_string(),
            fields: vec![AnubisValue::Int(n)],
            field_names: vec![],
        },
        Err(_) => AnubisValue::Enum {
            ty: "Option".to_string(),
            tag: "None".to_string(),
            fields: vec![],
            field_names: vec![],
        },
    }
}
fn anubis_parse_float_opt(v: AnubisValue) -> AnubisValue {
    match v.display_string().trim().parse::<f64>() {
        Ok(f) => AnubisValue::Enum {
            ty: "Option".to_string(),
            tag: "Some".to_string(),
            fields: vec![AnubisValue::Float(f)],
            field_names: vec![],
        },
        Err(_) => AnubisValue::Enum {
            ty: "Option".to_string(),
            tag: "None".to_string(),
            fields: vec![],
            field_names: vec![],
        },
    }
}

fn anubis_range(a: AnubisValue, b: AnubisValue) -> AnubisValue {
    anubis_require_numeric(&a, "range");
    anubis_require_numeric(&b, "range");
    let (mut i, hi) = (a.as_i64(), b.as_i64());
    let mut out = Vec::new();
    while i < hi { out.push(AnubisValue::Int(i)); i += 1; }
    anubis_mk_list(out)
}
fn anubis_range_step(a: AnubisValue, b: AnubisValue, step: AnubisValue) -> AnubisValue {
    anubis_require_numeric(&a, "range");
    anubis_require_numeric(&b, "range");
    anubis_require_numeric(&step, "range");
    let (mut i, hi, st) = (a.as_i64(), b.as_i64(), step.as_i64());
    if st == 0 {
        panic!("ANUBIS_INVALID_ARGUMENT: range step must be non-zero, got 0");
    }
    let mut out = Vec::new();
    if st > 0 { while i < hi { out.push(AnubisValue::Int(i)); i += st; } }
    else { while i > hi { out.push(AnubisValue::Int(i)); i += st; } }
    anubis_mk_list(out)
}
fn anubis_reverse(x: AnubisValue) -> AnubisValue {
    match x {
        AnubisValue::List(items) => { let mut items = anubis_rc_take(items); items.reverse(); anubis_mk_list(items) }
        AnubisValue::Str(s) => anubis_mk_str(s.chars().rev().collect()),
        other => panic!(
            "ANUBIS_TYPE_ERROR: reverse expects a list or string, got {}",
            other.type_name()
        ),
    }
}
fn anubis_sort(x: AnubisValue) -> AnubisValue {
    match x {
        AnubisValue::List(items) => {
            let mut items = anubis_rc_take(items);
            items.sort_by(anubis_value_cmp);
            anubis_mk_list(items)
        }
        other => panic!("ANUBIS_TYPE_ERROR: sort expects a list, got {}", other.type_name()),
    }
}
fn anubis_sum(x: AnubisValue) -> AnubisValue {
    match x {
        AnubisValue::List(items) => {
            if items.iter().any(|v| v.is_float()) {
                AnubisValue::Float(items.iter().map(|v| v.as_f64()).sum())
            } else {
                AnubisValue::Int(items.iter().map(|v| v.as_i64()).sum())
            }
        }
        other => panic!("ANUBIS_TYPE_ERROR: sum expects a list, got {}", other.type_name()),
    }
}
fn anubis_keys(m: AnubisValue) -> AnubisValue { m.map_keys() }
fn anubis_values(m: AnubisValue) -> AnubisValue {
    match m {
        AnubisValue::Map(e) => anubis_mk_list(anubis_rc_take(e).into_iter().map(|(_, v)| v).collect()),
        other => panic!("ANUBIS_TYPE_ERROR: values expects a map, got {}", other.type_name()),
    }
}
fn anubis_has_key(m: AnubisValue, k: AnubisValue) -> AnubisValue {
    let key = k.display_string();
    match m {
        AnubisValue::Map(e) => AnubisValue::Bool(e.iter().any(|(kk, _)| kk == &key)),
        other => panic!("ANUBIS_TYPE_ERROR: has_key expects a map, got {}", other.type_name()),
    }
}

fn anubis_pop(v: &mut AnubisValue) -> AnubisValue {
    match v {
        AnubisValue::List(l) => std::rc::Rc::make_mut(l).pop().unwrap_or_else(|| {
            panic!("ANUBIS_EMPTY_COLLECTION: pop on an empty list (use is_empty(xs) to guard)")
        }),
        other => panic!("ANUBIS_TYPE_ERROR: pop expects a list, got {}", other.type_name()),
    }
}
fn anubis_insert(v: &mut AnubisValue, i: AnubisValue, val: AnubisValue) -> AnubisValue {
    match v {
        AnubisValue::List(l) => {
            let raw = i.as_i64();
            let len = l.len() as i64;
            // Negative indices count from the end (consistent with element indexing).
            let idx = if raw < 0 { (raw + len).max(0) } else { raw.min(len) } as usize;
            std::rc::Rc::make_mut(l).insert(idx, val);
        }
        other => panic!("ANUBIS_TYPE_ERROR: insert expects a list, got {}", other.type_name()),
    }
    AnubisValue::Int(0)
}
fn anubis_remove(v: &mut AnubisValue, key: AnubisValue) -> AnubisValue {
    match v {
        AnubisValue::List(l) => {
            match anubis_norm_index(key.as_i64(), l.len()) {
                Some(k) => std::rc::Rc::make_mut(l).remove(k),
                None => panic!(
                    "ANUBIS_INDEX_OUT_OF_BOUNDS: index {} is out of bounds for a list of length {} (use get(xs, i, default) for optional access)",
                    key.as_i64(), l.len()
                ),
            }
        }
        AnubisValue::Map(m) => {
            let k = key.display_string();
            match m.iter().position(|(kk, _)| kk == &k) {
                Some(pos) => std::rc::Rc::make_mut(m).remove(pos).1,
                None => panic!(
                    "ANUBIS_MISSING_KEY: key `{}` is not present in the map (use get(m, k, default) for optional access)",
                    k
                ),
            }
        }
        other => panic!("ANUBIS_TYPE_ERROR: remove expects a list or map, got {}", other.type_name()),
    }
}

fn anubis_assert(cond: AnubisValue) -> AnubisValue {
    if !cond.as_bool() { panic!("ANUBIS_ASSERT_FAILED"); }
    AnubisValue::Bool(true)
}
// The checker adds every `assume(cond)` to the solver as a trusted axiom. For that trust to be SOUND
// the runtime must guarantee the assumption actually holds — otherwise a satisfiable-but-false
// `assume` (e.g. `assume(x < 100)` reached with x = i64::MAX) silently certifies a violated contract.
// So `assume` fails closed at runtime, exactly like `assert`; it still yields `true` for value use.
fn anubis_assume(cond: AnubisValue) -> AnubisValue {
    if !cond.as_bool() { panic!("ANUBIS_ASSUME_VIOLATED: an `assume(...)` was false at runtime; the checker trusts assumptions, so this fails closed rather than silently certify a false contract"); }
    AnubisValue::Bool(true)
}
// A parameter the checker models as an integer (u8/u16/u32/u64) is proved over a pure i64 bit-vector.
// The runtime is dynamically typed, so a float/string/list argument would take a DIVERGENT arithmetic
// path (float remainder, `+` concatenation/append) and violate the proven integer contract. Enforce
// the model at entry: an integer-typed parameter must hold an integer, else fail closed.
fn anubis_require_int(v: &AnubisValue, name: &str) {
    if !matches!(v, AnubisValue::Int(_)) {
        panic!("ANUBIS_TYPE_VIOLATION: integer parameter `{}` received a non-integer value at runtime; the checker models it as an i64, so a float/string/other argument is fail-closed rather than silently mis-proved", name);
    }
}
// Unbounded recursion must fail CLOSED like every other runtime trap, not abort the process.
//
// The whole trap design rests on one sentence in `lower_program_to_rust`: a fail-closed trap panics
// the worker, the hook prints the ANUBIS_* code, `join()` returns Err, we exit non-zero. That is
// true of panics. It is NOT true of a stack overflow: Rust's overflow handler ABORTS immediately
// without unwinding, so the process dies with `fatal runtime error: stack overflow` and none of the
// diagnostic path runs. The one failure that most needs an attributable message is exactly the one
// that bypasses it -- measured on a mutual-return cycle `check` accepts (CLAIMS item 13).
//
// So guard the resource itself. The stack grows DOWN on every target this runs on, so
// `base - here` is the bytes consumed; comparing against a budget below the real ceiling traps
// while there is still room to panic, unwind, and print. Guarding BYTES rather than a frame COUNT
// is what makes this correct regardless of frame size: a function with large locals trips after
// fewer calls, which is the right answer, and a shallow-frame function still gets its full depth.
//
// The base is captured lazily on the first user-function entry rather than injected by the entry
// stub, so no lowering can silently opt out by forgetting to initialize it.
thread_local! {
    static __ANB_STACK_BASE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
#[inline]
fn __anb_stack_guard() {
    if __ANB_STACK_BUDGET == 0 {
        return;
    }
    // `&0u8` would NOT work here: Rust const-promotes it to a 'static reference, so it reports a
    // rodata address and the guard silently never fires. It must be a real stack local, kept from
    // being optimized away.
    let here_marker: u8 = 0;
    let here = std::hint::black_box(&here_marker) as *const u8 as usize;
    __ANB_STACK_BASE.with(|b| {
        let base = b.get();
        if base == 0 {
            b.set(here);
        } else if base.saturating_sub(here) > __ANB_STACK_BUDGET {
            panic!("ANUBIS_RECURSION_LIMIT: recursion consumed more than {} MiB of stack without returning; `anubis check` does not prove termination, so a non-terminating program can pass the checker and this trap is how it fails closed rather than aborting the process", __ANB_STACK_BUDGET / (1024 * 1024));
        }
    });
}
// Same guard on a function's RETURN value (the model is only sound if an integer-typed function
// actually yields an integer). Returns the value through so it can wrap any return path.
fn anubis_require_int_ret(v: AnubisValue, name: &str) -> AnubisValue {
    if !matches!(v, AnubisValue::Int(_)) {
        panic!("ANUBIS_TYPE_VIOLATION: function `{}` declares an integer return type but returned a non-integer at runtime; the checker models its result as an i64, so this is fail-closed rather than silently mis-proved", name);
    }
    v
}
// The FLOAT dual of anubis_require_int (operator policy, task #34): a float-typed parameter is modeled by
// the checker as an f64, but the dynamically-typed runtime would otherwise let `f(7)` bind an Int(7),
// making `x / 2` INTEGER division (3) instead of float (3.5) — a checker/runtime divergence. COERCE an Int
// argument to a Float at the boundary (lossless for |n| < 2^53), so the param genuinely holds a float and
// the model is sound. A non-numeric argument (string/list/…) fails closed, exactly like the int guard.
fn anubis_coerce_float_param(v: AnubisValue, name: &str) -> AnubisValue {
    match v {
        AnubisValue::Int(n) => AnubisValue::Float(n as f64),
        AnubisValue::Float(_) => v,
        _ => panic!("ANUBIS_TYPE_VIOLATION: float parameter `{}` received a non-numeric value at runtime; the checker models it as an f64, so a string/list/other argument is fail-closed rather than silently mis-proved", name),
    }
}
// Same coercion on a float-typed function's RETURN value (the model is only sound if a float-returning
// function actually yields a float): coerce an Int return to a Float, fail closed on a non-numeric.
fn anubis_coerce_float_ret(v: AnubisValue, name: &str) -> AnubisValue {
    match v {
        AnubisValue::Int(n) => AnubisValue::Float(n as f64),
        AnubisValue::Float(_) => v,
        _ => panic!("ANUBIS_TYPE_VIOLATION: function `{}` declares a float return type but returned a non-numeric value at runtime; the checker models its result as an f64, so this is fail-closed rather than silently mis-proved", name),
    }
}
// A1 (task #50) — UNSIGNED fixed-width PARAM boundary coercion. An `u8`/`u16`/`u32` parameter is made
// a GENUINE [0, 2^w) value at entry, so the checker may soundly assume that range (dropping the
// `requires(x >= 0)` tax). The mask `n & (2^w - 1)` is exactly the low-`w` bits: −1 → 2^w−1, an
// oversized value → its value mod 2^w — always landing in [0, 2^w) ⊂ [0, 2^63), the non-negative
// signed range the solver's `bvsge`/`bvsle` model. `width` is 8/16/32 (never 64: masking a u64 into
// an i64 slot cannot represent [2^63, 2^64), so u64 keeps unbounded-i64 semantics). Fails closed on
// a non-integer, exactly like `anubis_require_int`. The int→f64 boundary coercion (task #34) is the
// float twin of this. Only PARAMS are masked (not returns/locals): that is where the tax lives, and
// `u32` is Anubis's default integer spelling, so masking returns would change every program that
// returns a negative/overflowing value from a `-> u32` function. A caller passing an out-of-range
// argument is handled in the checker by masking the arg when it is substituted into the callee's
// `requires`/`ensures` (so the composed contract matches this runtime mask — see mod.rs).
fn anubis_coerce_uint_param(v: AnubisValue, name: &str, width: u32) -> AnubisValue {
    match v {
        AnubisValue::Int(n) => {
            let mask: i64 = (1i64 << width) - 1;
            AnubisValue::Int(n & mask)
        }
        _ => panic!("ANUBIS_TYPE_VIOLATION: unsigned parameter `{}` received a non-integer value at runtime; the checker models it as a [0, 2^{}) integer, so a float/string/other argument is fail-closed rather than silently mis-proved", name, width),
    }
}
// STRUCT-FIELD numeric-kind guards (task #34 dual, extended to the construction boundary). They are
// deliberately GENTLER than the param/return guards above, because a struct field's declared type is
// unreliable: the parser stores a list type `[int]` as its element `int` (the brackets are dropped), so a
// genuine LIST field looks integer-typed. We therefore act on the VALUE and enforce ONLY the confirmed
// numeric-kind smuggle — a Float in an INTEGER field (float→int: the solver's QF_BV `bvsdiv` model would
// diverge from the runtime's float `/`) fails closed; every other value (Int, List, String, Bool, Struct)
// passes UNCHANGED, so a list/string/bool in an int-typed field (the parser quirk, or a dynamic value the
// solver does not model as a scalar int) is not spuriously trapped.
fn anubis_field_require_int(v: AnubisValue, name: &str) -> AnubisValue {
    if matches!(v, AnubisValue::Float(_)) {
        panic!("ANUBIS_TYPE_VIOLATION: integer field `{}` received a float value at runtime; the checker models it as an i64, so a float is fail-closed rather than silently mis-proved", name);
    }
    v
}
// The float dual: COERCE an Int value in a FLOAT field to a Float (so the QF_FP model is sound and
// `P{x: 7}` binds 7.0, exactly like a float param `f(7)`); pass every other value UNCHANGED (a list/string
// in a float-typed field is the parser quirk or a dynamic value — not the int→float smuggle).
fn anubis_field_coerce_float(v: AnubisValue, _name: &str) -> AnubisValue {
    match v {
        AnubisValue::Int(n) => AnubisValue::Float(n as f64),
        other => other,
    }
}
fn anubis_panic(msg: AnubisValue) -> AnubisValue { panic!("ANUBIS_PANIC: {}", msg.display_string()); }

fn anubis_input() -> AnubisValue {
    use std::io::BufRead;
    let mut line = String::new();
    let _ = std::io::stdin().lock().read_line(&mut line);
    while line.ends_with('\n') || line.ends_with('\r') { line.pop(); }
    anubis_mk_str(line)
}
fn anubis_args() -> AnubisValue {
    anubis_mk_list(std::env::args().skip(1).map(anubis_mk_str).collect())
}

// ---- Governed capability I/O (Phase-3 C3) — additive builtins; AnubisValue path unchanged ----
fn anubis_read_file(path: AnubisValue) -> AnubisValue {
    match std::fs::read_to_string(path.display_string()) {
        Ok(s) => anubis_mk_str(s),
        Err(e) => panic!("ANUBIS_IO_ERROR: read_file({}): {}", path.display_string(), e),
    }
}
fn anubis_write_file(path: AnubisValue, contents: AnubisValue) -> AnubisValue {
    match std::fs::write(path.display_string(), contents.display_string()) {
        Ok(()) => AnubisValue::Int(0),
        Err(e) => panic!("ANUBIS_IO_ERROR: write_file({}): {}", path.display_string(), e),
    }
}
/// Unlink a path. Shares the `fs.write` capability (filesystem mutation). Missing path is success
/// (idempotent destroy). Returns 0 on success, panics only on hard errors (permission, etc.).
fn anubis_delete_file(path: AnubisValue) -> AnubisValue {
    let p = path.display_string();
    match std::fs::remove_file(&p) {
        Ok(()) => AnubisValue::Int(0),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => AnubisValue::Int(0),
        Err(e) => panic!("ANUBIS_IO_ERROR: delete_file({}): {}", p, e),
    }
}
// Capability mint/export/Keychain-SE bind: see keychain_se_runtime.inc.rs (injected after core).

/// Consume a capability token. Linearity is checked at `check --verified`; runtime is the
/// authorized use-once sink so programs with caps lower and execute.
fn anubis_cap_use(cap: AnubisValue) -> AnubisValue {
    let _ = cap;
    AnubisValue::Int(0)
}
/// Confidentiality label mint (checker-side leg-1). Runtime is identity — the secret type system
/// and egress analysis run at check time.
fn anubis_secret_source(v: AnubisValue) -> AnubisValue {
    v
}
fn anubis_open(path: AnubisValue) -> AnubisValue {
    // `open` is a path-existence / openability probe that returns the path string on success
    // (contents are read via read_file). Fail-closed on missing/unreadable paths.
    match std::fs::File::open(path.display_string()) {
        Ok(_) => anubis_mk_str(path.display_string()),
        Err(e) => panic!("ANUBIS_IO_ERROR: open({}): {}", path.display_string(), e),
    }
}
fn anubis_time_now() -> AnubisValue {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    AnubisValue::Int(secs)
}
fn anubis_rand_gen() -> AnubisValue {
    // Prefer getrandom when available at compile of the generated binary; fall back to a
    // process-local seed from the clock so the program still runs without the crate.
    let mut buf = [0u8; 8];
    // Seed from clock + pid so successive runs differ without an external dep.
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mixed = t
        ^ ((std::process::id() as u64) << 32)
        ^ 0x9e37_79b9_7f4a_7c15;
    buf.copy_from_slice(&mixed.to_le_bytes());
    AnubisValue::Int(i64::from_le_bytes(buf))
}
fn anubis_net_send(host: AnubisValue, port: AnubisValue, payload: AnubisValue) -> AnubisValue {
    use std::io::Write;
    use std::net::TcpStream;
    let addr = format!("{}:{}", host.display_string(), port.as_i64());
    match TcpStream::connect(&addr) {
        Ok(mut stream) => {
            if let Err(e) = stream.write_all(payload.display_string().as_bytes()) {
                panic!("ANUBIS_IO_ERROR: send({}): {}", addr, e);
            }
            AnubisValue::Int(0)
        }
        Err(e) => panic!("ANUBIS_IO_ERROR: send({}): {}", addr, e),
    }
}
fn anubis_net_connect(host: AnubisValue, port: AnubisValue) -> AnubisValue {
    use std::net::TcpStream;
    let addr = format!("{}:{}", host.display_string(), port.as_i64());
    match TcpStream::connect(&addr) {
        Ok(_) => anubis_mk_str(addr),
        Err(e) => panic!("ANUBIS_IO_ERROR: connect({}): {}", addr, e),
    }
}
// HTTP: cleartext over pure std TCP; HTTPS via host `curl` (system TLS TCB — SecureTransport/
// LibreSSL/OpenSSL depending on host). Same honesty as package-registry HTTPS. No DIY TLS.
// URL shape: http(s)://host[:port]/path[?query]] — path defaults to `/`.
fn anubis_http_parse_url(url: &str) -> (bool, String, u16, String) {
    let (https, rest) = if let Some(r) = url.strip_prefix("https://") {
        (true, r)
    } else if let Some(r) = url.strip_prefix("http://") {
        (false, r)
    } else {
        panic!(
            "ANUBIS_IO_ERROR: http_get/http_post URL must start with http:// or https:// (got {})",
            url
        );
    };
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i..].to_string()),
        None => (rest, "/".to_string()),
    };
    if authority.is_empty() {
        panic!("ANUBIS_IO_ERROR: http URL missing host: {}", url);
    }
    let default_port = if https { 443u16 } else { 80u16 };
    let (host, port) = if let Some(i) = authority.rfind(':') {
        if authority.starts_with('[') {
            panic!(
                "ANUBIS_IO_ERROR: http_get/http_post does not parse IPv6 authorities: {}",
                url
            );
        }
        let (h, p) = authority.split_at(i);
        let pnum: u16 = p[1..].parse().unwrap_or_else(|_| {
            panic!("ANUBIS_IO_ERROR: invalid port in URL: {}", url);
        });
        (h.to_string(), pnum)
    } else {
        (authority.to_string(), default_port)
    };
    let path = if path.is_empty() { "/".to_string() } else { path };
    (https, host, port, path)
}
/// HTTPS via host curl — body only on stdout; fail-closed on non-zero exit.
fn anubis_http_via_curl(method: &str, url: &str, body: Option<&str>) -> AnubisValue {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut cmd = Command::new("curl");
    cmd.args(["-fsSL", "--max-time", "30", "-X", method, url]);
    // SECURITY (#75): the request body is written to curl's STDIN and referenced by the FIXED literal
    // `@-`, never passed inline as `--data-binary <body>`. curl interprets a `@`-prefixed data value as
    // a FILENAME, so an inline body that merely BEGINS with `@` made curl read an arbitrary LOCAL FILE
    // and transmit it — escalating the `net.send` capability into arbitrary local file read plus
    // egress, with no fs.read capability and no diagnostic. Because `@-` is a constant, no
    // program-controlled string can reach curl's filename parser at all.
    if body.is_some() {
        cmd.args([
            "-H",
            "Content-Type: application/octet-stream",
            "--data-binary",
            "@-",
        ]);
        cmd.stdin(Stdio::piped());
    } else {
        cmd.stdin(Stdio::null());
    }
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => panic!(
            "ANUBIS_IO_ERROR: https requires host `curl` on PATH (system TLS TCB): {}",
            e
        ),
    };
    if let Some(b) = body {
        // Dropping the handle closes the pipe so curl sees EOF and stops reading.
        if let Some(mut si) = child.stdin.take() {
            if let Err(e) = si.write_all(b.as_bytes()) {
                panic!("ANUBIS_IO_ERROR: https curl body write failed: {}", e);
            }
        }
    }
    match child.wait_with_output() {
        Ok(out) if out.status.success() => {
            anubis_mk_str(String::from_utf8_lossy(&out.stdout).into_owned())
        }
        Ok(out) => panic!(
            "ANUBIS_IO_ERROR: https curl failed (exit {:?}): {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        ),
        Err(e) => panic!(
            "ANUBIS_IO_ERROR: https requires host `curl` on PATH (system TLS TCB): {}",
            e
        ),
    }
}
fn anubis_http_exchange(method: &str, url: AnubisValue, body: Option<AnubisValue>) -> AnubisValue {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;
    let url_s = url.display_string();
    let (https, host, port, path) = anubis_http_parse_url(&url_s);
    let body_s = body.map(|b| b.display_string());
    if https {
        // Rebuild absolute URL for curl (preserves original form).
        return anubis_http_via_curl(method, &url_s, body_s.as_deref());
    }
    let addr = format!("{}:{}", host, port);
    let mut stream = match TcpStream::connect(&addr) {
        Ok(s) => s,
        Err(e) => panic!("ANUBIS_IO_ERROR: http connect({}): {}", addr, e),
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
    let body_owned = body_s.unwrap_or_default();
    let req = if method == "POST" {
        format!(
            "POST {} HTTP/1.0\r\nHost: {}\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            path,
            host,
            body_owned.len(),
            body_owned
        )
    } else {
        format!(
            "GET {} HTTP/1.0\r\nHost: {}\r\nConnection: close\r\n\r\n",
            path, host
        )
    };
    if let Err(e) = stream.write_all(req.as_bytes()) {
        panic!("ANUBIS_IO_ERROR: http write({}): {}", addr, e);
    }
    let mut buf = Vec::new();
    if let Err(e) = stream.read_to_end(&mut buf) {
        panic!("ANUBIS_IO_ERROR: http read({}): {}", addr, e);
    }
    let raw = String::from_utf8_lossy(&buf);
    if let Some(idx) = raw.find("\r\n\r\n") {
        anubis_mk_str(raw[idx + 4..].to_string())
    } else if let Some(idx) = raw.find("\n\n") {
        anubis_mk_str(raw[idx + 2..].to_string())
    } else {
        panic!(
            "ANUBIS_IO_ERROR: http response missing header/body separator from {}",
            addr
        );
    }
}
fn anubis_http_get(url: AnubisValue) -> AnubisValue {
    anubis_http_exchange("GET", url, None)
}
fn anubis_http_post(url: AnubisValue, body: AnubisValue) -> AnubisValue {
    anubis_http_exchange("POST", url, Some(body))
}

// ---- Higher-order functions over closures ----

fn anubis_map(list: AnubisValue, f: AnubisValue) -> AnubisValue {
    anubis_mk_list(anubis_iter(list).into_iter().map(|x| f.call_closure(vec![x])).collect())
}
fn anubis_filter(list: AnubisValue, f: AnubisValue) -> AnubisValue {
    anubis_mk_list(anubis_iter(list).into_iter().filter(|x| f.call_closure(vec![x.clone()]).as_bool()).collect())
}
// `reduce(list, closure, seed)` folds `closure(acc, x)` over the list from `seed`. ORDER-AGNOSTIC on the
// two non-list arguments: the closure may be the 2nd arg (Anubis-native `reduce(list, f, seed)`) OR the
// 3rd (the JS/Rust-fold-natural `reduce(list, seed, f)`). Whichever argument IS a closure is the fold
// function; the other is the seed. This fixes the reported crash where the seed-first order sent an int
// where a closure was expected. If NEITHER is a closure it is a genuine type error with a message that
// names both accepted forms (was a bare `expected closure, got int`).
fn anubis_reduce(list: AnubisValue, a: AnubisValue, b: AnubisValue) -> AnubisValue {
    let (f, mut acc) = match (a.is_closure(), b.is_closure()) {
        (true, _) => (a, b),
        (false, true) => (b, a),
        (false, false) => panic!(
            "ANUBIS_TYPE_ERROR: reduce expects a closure argument — reduce(list, closure, seed) or reduce(list, seed, closure)"
        ),
    };
    for x in anubis_iter(list) { acc = f.call_closure(vec![acc, x]); }
    acc
}
// Seedless `reduce(list, closure)`: the FIRST element seeds the accumulator and the closure folds the
// rest (standard seedless reduce, mirroring the semantics used when no initial value is supplied). An
// empty list has no defined seed — fail closed (do not invent Int(0); that is only the additive
// identity for numeric folds and is wrong for non-numeric reduce). Use reduce(list, closure, seed).
fn anubis_reduce2(list: AnubisValue, f: AnubisValue) -> AnubisValue {
    if !f.is_closure() {
        panic!("ANUBIS_TYPE_ERROR: reduce(list, closure) expects a closure as the second argument, got {}", f.type_name());
    }
    let mut it = anubis_iter(list).into_iter();
    let mut acc = match it.next() {
        Some(x) => x,
        None => panic!("ANUBIS_EMPTY_COLLECTION: reduce(list, closure) has no seed — the list is empty; use reduce(list, closure, seed) to supply one"),
    };
    for x in it { acc = f.call_closure(vec![acc, x]); }
    acc
}
fn anubis_each(list: AnubisValue, f: AnubisValue) -> AnubisValue {
    for x in anubis_iter(list) { let _ = f.call_closure(vec![x]); }
    AnubisValue::Int(0)
}
fn anubis_find(list: AnubisValue, f: AnubisValue) -> AnubisValue {
    for x in anubis_iter(list) { if f.call_closure(vec![x.clone()]).as_bool() { return x; } }
    panic!("ANUBIS_NO_MATCH: find() — no element satisfies the predicate (guard with any(xs, pred) first, or use position(xs, pred) if you only need the index)")
}
fn anubis_any(list: AnubisValue, f: AnubisValue) -> AnubisValue {
    AnubisValue::Bool(anubis_iter(list).into_iter().any(|x| f.call_closure(vec![x]).as_bool()))
}
fn anubis_all(list: AnubisValue, f: AnubisValue) -> AnubisValue {
    AnubisValue::Bool(anubis_iter(list).into_iter().all(|x| f.call_closure(vec![x]).as_bool()))
}
fn anubis_count_by(list: AnubisValue, f: AnubisValue) -> AnubisValue {
    AnubisValue::Int(anubis_iter(list).into_iter().filter(|x| f.call_closure(vec![x.clone()]).as_bool()).count() as i64)
}
fn anubis_sort_by(list: AnubisValue, f: AnubisValue) -> AnubisValue {
    match list {
        AnubisValue::List(items) => {
            let mut items = anubis_rc_take(items);
            items.sort_by(|a, b| {
                let ka = f.call_closure(vec![a.clone()]);
                let kb = f.call_closure(vec![b.clone()]);
                anubis_value_cmp(&ka, &kb)
            });
            anubis_mk_list(items)
        }
        // Fail CLOSED on a non-list first argument (was `other => other`, which silently returned the
        // argument unsorted — leaking a `<closure>` on a swapped `sort_by(closure, list)` call, or a
        // string/map unchanged — an HOF-audit silent-wrong-output bug).
        other => panic!("ANUBIS_TYPE_ERROR: sort_by expects a list as its first argument, got {}", other.type_name()),
    }
}
fn anubis_apply(f: AnubisValue, args: AnubisValue) -> AnubisValue {
    match args {
        AnubisValue::List(items) => f.call_closure(anubis_rc_take(items)),
        other => f.call_closure(vec![other]),
    }
}

/// Build a map from literal entries, deduplicating keys (last value wins) so `{ "a": 1, "a": 2 }`
/// is a well-formed single-entry map.
fn anubis_map_lit(pairs: Vec<(String, AnubisValue)>) -> AnubisValue {
    let mut out: Vec<(String, AnubisValue)> = Vec::new();
    for (k, v) in pairs {
        if let Some(slot) = out.iter_mut().find(|(kk, _)| kk == &k) {
            slot.1 = v;
        } else {
            out.push((k, v));
        }
    }
    anubis_mk_map(out)
}

/// Materialize a value's iteration elements: list items, string characters, or map keys.
fn anubis_iter(v: AnubisValue) -> Vec<AnubisValue> {
    match v {
        AnubisValue::List(items) => anubis_rc_take(items),
        AnubisValue::Str(s) => s.chars().map(|c| anubis_mk_str(c.to_string())).collect(),
        AnubisValue::Map(m) => anubis_rc_take(m).into_iter().map(|(k, _)| anubis_mk_str(k)).collect(),
        // A CLOSURE is never iterable — reaching here means a higher-order call was given a closure
        // where the collection was expected (the classic swapped-argument mistake, e.g.
        // `min_by(|x| x, list)`). Fail CLOSED with a message that names the likely cause, instead of
        // the old `other => vec![other]` which silently wrapped the closure as a 1-element sequence and
        // returned it unexamined (a silent-wrong-output bug the HOF audit surfaced).
        AnubisValue::Closure(_) => panic!(
            "ANUBIS_TYPE_ERROR: a closure is not iterable — check the argument order (the collection must come before the closure)"
        ),
        other => panic!(
            "ANUBIS_TYPE_ERROR: expected a list, string, or map, got {} — check the argument order or that this value is actually a collection",
            other.type_name()
        ),
    }
}

// ---- math ----
fn anubis_sin(x: AnubisValue) -> AnubisValue { anubis_require_numeric(&x, "sin"); AnubisValue::Float(x.as_f64().sin()) }
fn anubis_cos(x: AnubisValue) -> AnubisValue { anubis_require_numeric(&x, "cos"); AnubisValue::Float(x.as_f64().cos()) }
fn anubis_tan(x: AnubisValue) -> AnubisValue { anubis_require_numeric(&x, "tan"); AnubisValue::Float(x.as_f64().tan()) }
fn anubis_asin(x: AnubisValue) -> AnubisValue { anubis_require_numeric(&x, "asin"); AnubisValue::Float(x.as_f64().asin()) }
fn anubis_acos(x: AnubisValue) -> AnubisValue { anubis_require_numeric(&x, "acos"); AnubisValue::Float(x.as_f64().acos()) }
fn anubis_atan(x: AnubisValue) -> AnubisValue { anubis_require_numeric(&x, "atan"); AnubisValue::Float(x.as_f64().atan()) }
fn anubis_atan2(y: AnubisValue, x: AnubisValue) -> AnubisValue { anubis_require_numeric(&y, "atan2"); anubis_require_numeric(&x, "atan2"); AnubisValue::Float(y.as_f64().atan2(x.as_f64())) }
fn anubis_exp(x: AnubisValue) -> AnubisValue { anubis_require_numeric(&x, "exp"); AnubisValue::Float(x.as_f64().exp()) }
fn anubis_ln(x: AnubisValue) -> AnubisValue { anubis_require_numeric(&x, "ln"); AnubisValue::Float(x.as_f64().ln()) }
fn anubis_log10(x: AnubisValue) -> AnubisValue { anubis_require_numeric(&x, "log10"); AnubisValue::Float(x.as_f64().log10()) }
fn anubis_log2(x: AnubisValue) -> AnubisValue { anubis_require_numeric(&x, "log2"); AnubisValue::Float(x.as_f64().log2()) }
fn anubis_logb(x: AnubisValue, base: AnubisValue) -> AnubisValue { anubis_require_numeric(&x, "log"); anubis_require_numeric(&base, "log"); AnubisValue::Float(x.as_f64().log(base.as_f64())) }
fn anubis_cbrt(x: AnubisValue) -> AnubisValue { anubis_require_numeric(&x, "cbrt"); AnubisValue::Float(x.as_f64().cbrt()) }
fn anubis_hypot(x: AnubisValue, y: AnubisValue) -> AnubisValue { anubis_require_numeric(&x, "hypot"); anubis_require_numeric(&y, "hypot"); AnubisValue::Float(x.as_f64().hypot(y.as_f64())) }
fn anubis_trunc(x: AnubisValue) -> AnubisValue { anubis_require_numeric(&x, "trunc"); match x { AnubisValue::Int(n) => AnubisValue::Int(n), _ => AnubisValue::Int(x.as_f64().trunc() as i64) } }
fn anubis_sign(x: AnubisValue) -> AnubisValue { anubis_require_numeric(&x, "sign"); let v = x.as_f64(); AnubisValue::Int(if v > 0.0 { 1 } else if v < 0.0 { -1 } else { 0 }) }
fn anubis_clamp(x: AnubisValue, lo: AnubisValue, hi: AnubisValue) -> AnubisValue {
    anubis_require_numeric(&x, "clamp");
    anubis_require_numeric(&lo, "clamp");
    anubis_require_numeric(&hi, "clamp");
    if x.is_float() || lo.is_float() || hi.is_float() {
        let (lo_f, hi_f) = (lo.as_f64(), hi.as_f64());
        if lo_f > hi_f {
            panic!("ANUBIS_INVALID_ARGUMENT: clamp bounds are inverted — lo ({}) > hi ({})", lo_f, hi_f);
        }
        AnubisValue::Float(x.as_f64().max(lo_f).min(hi_f))
    } else {
        let (lo_i, hi_i) = (lo.as_i64(), hi.as_i64());
        if lo_i > hi_i {
            panic!("ANUBIS_INVALID_ARGUMENT: clamp bounds are inverted — lo ({}) > hi ({})", lo_i, hi_i);
        }
        AnubisValue::Int(x.as_i64().max(lo_i).min(hi_i))
    }
}
fn anubis_pi() -> AnubisValue { AnubisValue::Float(std::f64::consts::PI) }
fn anubis_e() -> AnubisValue { AnubisValue::Float(std::f64::consts::E) }
fn anubis_factorial(n: AnubisValue) -> AnubisValue {
    // Reject soft-coerced strings (`factorial("5")` used to return 120 via as_i64).
    let n_raw = match n {
        AnubisValue::Int(v) => v,
        other => panic!(
            "ANUBIS_TYPE_ERROR: factorial expects an int argument, got {}",
            other.type_name()
        ),
    };
    if n_raw < 0 {
        panic!("ANUBIS_DOMAIN_ERROR: factorial is undefined for negative integers, got {}", n_raw);
    }
    let n = n_raw;
    let mut acc: i64 = 1;
    let mut i: i64 = 2;
    while i <= n {
        acc = match acc.checked_mul(i) {
            Some(v) => v,
            None => panic!("ANUBIS_OVERFLOW: factorial({}) overflows i64 (i64::MAX is 9223372036854775807, reached between 20! and 21!)", n),
        };
        i += 1;
    }
    AnubisValue::Int(acc)
}

// ---- strings ----
fn anubis_chars(s: AnubisValue) -> AnubisValue {
    anubis_mk_list(s.display_string().chars().map(|c| anubis_mk_str(c.to_string())).collect())
}
fn anubis_words(s: AnubisValue) -> AnubisValue {
    anubis_mk_list(s.display_string().split_whitespace().map(|w| anubis_mk_str(w.to_string())).collect())
}
fn anubis_lines(s: AnubisValue) -> AnubisValue {
    anubis_mk_list(s.display_string().lines().map(|l| anubis_mk_str(l.to_string())).collect())
}
fn anubis_capitalize(s: AnubisValue) -> AnubisValue {
    let s = s.display_string();
    let mut ch = s.chars();
    match ch.next() {
        Some(f) => anubis_mk_str(f.to_uppercase().collect::<String>() + &ch.as_str().to_lowercase()),
        None => anubis_mk_str(String::new()),
    }
}
fn anubis_pad(s: AnubisValue, width: AnubisValue, pad: AnubisValue, at_start: bool) -> AnubisValue {
    let s = s.display_string();
    // Was `.max(0)` — negative width silently became a no-op (Phase-5 M–Z SILENT_WRONG).
    let w_raw = width.as_i64();
    if w_raw < 0 {
        panic!("ANUBIS_INVALID_ARGUMENT: pad width must be non-negative, got {}", w_raw);
    }
    let w = w_raw as usize;
    let p = { let ps = pad.display_string(); if ps.is_empty() { " ".to_string() } else { ps } };
    let have = s.chars().count();
    if have >= w { return anubis_mk_str(s); }
    let mut fill = String::new();
    while fill.chars().count() < w - have { fill.push_str(&p); }
    let fill: String = fill.chars().take(w - have).collect();
    anubis_mk_str(if at_start { format!("{}{}", fill, s) } else { format!("{}{}", s, fill) })
}

// ---- lists ----
fn anubis_zip(a: AnubisValue, b: AnubisValue) -> AnubisValue {
    let bv = anubis_iter(b);
    anubis_mk_list(anubis_iter(a).into_iter().zip(bv).map(|(x, y)| anubis_mk_list(vec![x, y])).collect())
}
fn anubis_enumerate(a: AnubisValue) -> AnubisValue {
    anubis_mk_list(anubis_iter(a).into_iter().enumerate().map(|(i, x)| anubis_mk_list(vec![AnubisValue::Int(i as i64), x])).collect())
}
fn anubis_flatten(a: AnubisValue) -> AnubisValue {
    let mut out = Vec::new();
    for x in anubis_iter(a) { for y in anubis_iter(x) { out.push(y); } }
    anubis_mk_list(out)
}
fn anubis_flat_map(a: AnubisValue, f: AnubisValue) -> AnubisValue {
    let mut out = Vec::new();
    for x in anubis_iter(a) { for y in anubis_iter(f.call_closure(vec![x])) { out.push(y); } }
    anubis_mk_list(out)
}
fn anubis_unique(a: AnubisValue) -> AnubisValue {
    let mut out: Vec<AnubisValue> = Vec::new();
    for x in anubis_iter(a) {
        // Deduplicate by structural equality (matching `==`), not display form: `1` and `"1"`
        // are distinct, while `1` and `1.0` are the same.
        if !out.iter().any(|y| anubis_value_eq(y, &x)) { out.push(x); }
    }
    anubis_mk_list(out)
}
fn anubis_take(a: AnubisValue, n: AnubisValue) -> AnubisValue {
    let n_raw = n.as_i64();
    if n_raw < 0 {
        panic!("ANUBIS_INVALID_ARGUMENT: take count must be non-negative, got {}", n_raw);
    }
    let n = n_raw as usize;
    anubis_mk_list(anubis_iter(a).into_iter().take(n).collect())
}
fn anubis_drop(a: AnubisValue, n: AnubisValue) -> AnubisValue {
    let n_raw = n.as_i64();
    if n_raw < 0 {
        panic!("ANUBIS_INVALID_ARGUMENT: drop count must be non-negative, got {}", n_raw);
    }
    let n = n_raw as usize;
    anubis_mk_list(anubis_iter(a).into_iter().skip(n).collect())
}
fn anubis_take_while(a: AnubisValue, f: AnubisValue) -> AnubisValue {
    let mut out = Vec::new();
    for x in anubis_iter(a) {
        if f.call_closure(vec![x.clone()]).as_bool() { out.push(x); } else { break; }
    }
    anubis_mk_list(out)
}
fn anubis_drop_while(a: AnubisValue, f: AnubisValue) -> AnubisValue {
    let items = anubis_iter(a);
    let mut i = 0;
    while i < items.len() && f.call_closure(vec![items[i].clone()]).as_bool() { i += 1; }
    anubis_mk_list(items[i..].to_vec())
}
fn anubis_chunk(a: AnubisValue, n: AnubisValue) -> AnubisValue {
    let n_raw = n.as_i64();
    if n_raw <= 0 {
        panic!("ANUBIS_INVALID_ARGUMENT: chunk size must be positive, got {}", n_raw);
    }
    let n = n_raw as usize;
    anubis_mk_list(anubis_iter(a).chunks(n).map(|c| anubis_mk_list(c.to_vec())).collect())
}
fn anubis_window(a: AnubisValue, n: AnubisValue) -> AnubisValue {
    let n_raw = n.as_i64();
    if n_raw <= 0 {
        panic!("ANUBIS_INVALID_ARGUMENT: window size must be positive, got {}", n_raw);
    }
    let n = n_raw as usize;
    let items = anubis_iter(a);
    if items.len() < n { return anubis_mk_list(vec![]); }
    anubis_mk_list(items.windows(n).map(|w| anubis_mk_list(w.to_vec())).collect())
}
fn anubis_position(a: AnubisValue, f: AnubisValue) -> AnubisValue {
    for (i, x) in anubis_iter(a).into_iter().enumerate() {
        if f.call_closure(vec![x]).as_bool() { return AnubisValue::Int(i as i64); }
    }
    AnubisValue::Int(-1)
}
fn anubis_product(a: AnubisValue) -> AnubisValue {
    let items = anubis_iter(a);
    if items.iter().any(|v| v.is_float()) {
        AnubisValue::Float(items.iter().map(|v| v.as_f64()).product())
    } else {
        AnubisValue::Int(items.iter().map(|v| v.as_i64()).product())
    }
}
fn anubis_first(a: AnubisValue) -> AnubisValue {
    anubis_iter(a).into_iter().next().unwrap_or_else(|| {
        panic!("ANUBIS_EMPTY_COLLECTION: first has no element — the collection is empty (use is_empty(xs) to guard)")
    })
}
fn anubis_last(a: AnubisValue) -> AnubisValue {
    anubis_iter(a).into_iter().last().unwrap_or_else(|| {
        panic!("ANUBIS_EMPTY_COLLECTION: last has no element — the collection is empty (use is_empty(xs) to guard)")
    })
}
/// True when a collection has no elements (empty ⟺ `len == 0`, matching `len`'s type coverage).
/// Lets programs guard `pop`/`last`/index access without hand-writing `len(xs) > 0` everywhere.
fn anubis_is_empty(v: AnubisValue) -> AnubisValue {
    let n = match &v {
        AnubisValue::List(l) => l.len(),
        AnubisValue::Str(s) => s.chars().count(),
        AnubisValue::Map(m) => m.len(),
        AnubisValue::Struct { fields, .. } => fields.len(),
        AnubisValue::Enum { fields, .. } => fields.len(),
        // Was `_ => 0` so `is_empty(42)` / `is_empty(true)` returned true (Phase-5 SILENT_WRONG).
        other => panic!(
            "ANUBIS_TYPE_ERROR: is_empty expects a list, string, map, struct, or enum, got {}",
            other.type_name()
        ),
    };
    AnubisValue::Bool(n == 0)
}
fn anubis_concat(a: AnubisValue, b: AnubisValue) -> AnubisValue {
    let mut out = anubis_iter(a);
    out.extend(anubis_iter(b));
    anubis_mk_list(out)
}
fn anubis_min_by(a: AnubisValue, f: AnubisValue) -> AnubisValue {
    anubis_iter(a).into_iter()
        .min_by(|x, y| anubis_value_cmp(&f.call_closure(vec![x.clone()]), &f.call_closure(vec![y.clone()])))
        .unwrap_or_else(|| panic!("ANUBIS_EMPTY_COLLECTION: min_by has no element — the collection is empty (use is_empty(xs) to guard)"))
}
fn anubis_max_by(a: AnubisValue, f: AnubisValue) -> AnubisValue {
    anubis_iter(a).into_iter()
        .max_by(|x, y| anubis_value_cmp(&f.call_closure(vec![x.clone()]), &f.call_closure(vec![y.clone()])))
        .unwrap_or_else(|| panic!("ANUBIS_EMPTY_COLLECTION: max_by has no element — the collection is empty (use is_empty(xs) to guard)"))
}
fn anubis_partition(a: AnubisValue, f: AnubisValue) -> AnubisValue {
    let mut yes = Vec::new();
    let mut no = Vec::new();
    for x in anubis_iter(a) {
        if f.call_closure(vec![x.clone()]).as_bool() { yes.push(x); } else { no.push(x); }
    }
    anubis_mk_list(vec![anubis_mk_list(yes), anubis_mk_list(no)])
}

// ---- maps ----
fn anubis_entries(m: AnubisValue) -> AnubisValue {
    match m {
        AnubisValue::Map(m) => anubis_mk_list(anubis_rc_take(m).into_iter().map(|(k, v)| anubis_mk_list(vec![anubis_mk_str(k), v])).collect()),
        other => panic!(
            "ANUBIS_TYPE_ERROR: entries expects a map, got {}",
            other.type_name()
        ),
    }
}
// The fail-SOFT counterpart to fail-closed `coll[key]`: returns the element if the key is present
// (map) or the index is in range (list/string, negatives allowed), else the caller's `default`.
fn anubis_get(m: AnubisValue, k: AnubisValue, default: AnubisValue) -> AnubisValue {
    match &m {
        AnubisValue::Map(mm) => {
            let key = k.display_string();
            mm.iter().find(|(kk, _)| kk == &key).map(|(_, v)| v.clone()).unwrap_or(default)
        }
        AnubisValue::List(v) => match anubis_norm_index(k.as_i64(), v.len()) {
            Some(idx) => v[idx].clone(),
            None => default,
        },
        AnubisValue::Str(s) => {
            let chars: Vec<char> = s.chars().collect();
            match anubis_norm_index(k.as_i64(), chars.len()) {
                Some(idx) => anubis_mk_str(chars[idx].to_string()),
                None => default,
            }
        }
        _ => default,
    }
}
fn anubis_merge(a: AnubisValue, b: AnubisValue) -> AnubisValue {
    let mut out = match a {
        AnubisValue::Map(m) => anubis_rc_take(m),
        other => panic!(
            "ANUBIS_TYPE_ERROR: merge expects a map as its first argument, got {}",
            other.type_name()
        ),
    };
    match b {
        AnubisValue::Map(bm) => {
            for (k, v) in anubis_rc_take(bm) {
                if let Some(slot) = out.iter_mut().find(|(kk, _)| kk == &k) { slot.1 = v; } else { out.push((k, v)); }
            }
        }
        other => panic!(
            "ANUBIS_TYPE_ERROR: merge expects a map as its second argument, got {}",
            other.type_name()
        ),
    }
    anubis_mk_map(out)
}
fn anubis_map_values(m: AnubisValue, f: AnubisValue) -> AnubisValue {
    match m {
        AnubisValue::Map(mm) => anubis_mk_map(anubis_rc_take(mm).into_iter().map(|(k, v)| (k, f.call_closure(vec![v]))).collect()),
        // Fail CLOSED on a non-map first argument (was `other => other`, which silently returned e.g. a
        // list unchanged with the closure never applied — an HOF-audit silent-wrong-output bug).
        other => panic!("ANUBIS_TYPE_ERROR: map_values expects a map as its first argument, got {}", other.type_name()),
    }
}

// ---- functional ----
fn anubis_identity(x: AnubisValue) -> AnubisValue { x }
fn anubis_compose(f: AnubisValue, g: AnubisValue) -> AnubisValue {
    AnubisValue::Closure(std::rc::Rc::new(move |args: Vec<AnubisValue>| {
        let gx = g.call_closure(args);
        f.call_closure(vec![gx])
    }))
}
fn anubis_times(n: AnubisValue, f: AnubisValue) -> AnubisValue {
    // Fail CLOSED when the count slot holds a closure — the swapped `times(closure, n)` mistake. Without
    // this, `n.as_i64()` coerced the closure to 0 and returned an empty list at exit 0 (a silent-wrong
    // HOF-audit bug). The canonical order is `times(count, closure)`.
    if n.is_closure() {
        panic!("ANUBIS_TYPE_ERROR: times expects a count as its first argument — times(count, closure), got a closure");
    }
    // Was `n.as_i64().max(0)` — `times(-1, f)` returned `[]` and `times("2", f)` soft-coerced the
    // string to 2 and ran the body (Phase-5 M–Z SILENT_WRONG).
    let n_raw = match n {
        AnubisValue::Int(v) => v,
        other => panic!(
            "ANUBIS_TYPE_ERROR: times expects an int count as its first argument, got {}",
            other.type_name()
        ),
    };
    if n_raw < 0 {
        panic!("ANUBIS_INVALID_ARGUMENT: times count must be non-negative, got {}", n_raw);
    }
    let n = n_raw;
    anubis_mk_list((0..n).map(|i| f.call_closure(vec![AnubisValue::Int(i)])).collect())
}

// CRYPTO_RUNTIME_INJECTED_BELOW — pure (guest) or audited crates (native run)

fn anubis_append_file(path: AnubisValue, contents: AnubisValue) -> AnubisValue {
    use std::io::Write;
    let p = path.display_string();
    let mut f = match std::fs::OpenOptions::new().create(true).append(true).open(&p) {
        Ok(f) => f,
        Err(e) => panic!("ANUBIS_IO_ERROR: append_file({}): {}", p, e),
    };
    if let Err(e) = write!(f, "{}", contents.display_string()) {
        panic!("ANUBIS_IO_ERROR: append_file({}): {}", p, e);
    }
    AnubisValue::Int(0)
}

fn anubis_env(name: AnubisValue) -> AnubisValue {
    match std::env::var(name.display_string()) {
        Ok(v) => anubis_mk_str(v),
        Err(_) => anubis_mk_str(String::new()),
    }
}

"#;

const NATIVE_PROOF_STUBS_RS: &str = r#"
fn anubis_proof_input_u32_val(name: &str) -> AnubisValue {
    // Lightweight env map: ANUBIS_PROOF_INPUTS="k=v,k2=v2"
    if let Ok(raw) = std::env::var("ANUBIS_PROOF_INPUTS") {
        for part in raw.split(',') {
            let mut it = part.splitn(2, '=');
            if let (Some(k), Some(v)) = (it.next(), it.next()) {
                if k.trim() == name {
                    if let Ok(n) = v.trim().parse::<i64>() {
                        return AnubisValue::Int(n);
                    }
                }
            }
        }
    }
    panic!(
        "ANUBIS_PROOF_INPUT_MISSING: key `{}` (set ANUBIS_PROOF_INPUTS=k=v for run, or use prove --input-json)",
        name
    );
}
fn anubis_proof_input_bool_val(name: &str) -> AnubisValue {
    AnubisValue::Bool(anubis_proof_input_u32_val(name).as_i64() != 0)
}
fn anubis_proof_commit_u32(_name: &str, v: AnubisValue) -> AnubisValue { v }
fn anubis_proof_commit_bool(_name: &str, v: AnubisValue) -> AnubisValue {
    AnubisValue::Int(if v.as_bool() { 1 } else { 0 })
}
fn anubis_proof_assert(cond: AnubisValue) -> AnubisValue {
    if !cond.as_bool() {
        panic!("ANUBIS_PROOF_ASSERT_FAILED");
    }
    AnubisValue::Bool(true)
}
"#;

/// Injected into RISC0 guests so `proof_input_u32` / `proof_input_bool` read host-supplied inputs
/// and so journals can be multi-field (`return [..]` commits each u32).
const PROOF_INPUT_GUEST_RUNTIME_RS: &str = r#"
static ANUBIS_PROOF_INPUTS: OnceLock<HashMap<String, i64>> = OnceLock::new();

fn anubis_load_proof_inputs() {
    let n: u32 = env::read();
    let mut m = HashMap::new();
    for _ in 0..n {
        let k: String = env::read();
        let v: i64 = env::read();
        m.insert(k, v);
    }
    let _ = ANUBIS_PROOF_INPUTS.set(m);
}

fn anubis_proof_input_i64(name: &str) -> i64 {
    let m = ANUBIS_PROOF_INPUTS
        .get()
        .expect("ANUBIS_PROOF_INPUT_MISSING: inputs not loaded");
    match m.get(name) {
        Some(v) => *v,
        None => panic!("ANUBIS_PROOF_INPUT_MISSING: key `{}`", name),
    }
}

fn anubis_proof_input_u32_val(name: &str) -> AnubisValue {
    AnubisValue::Int(anubis_proof_input_i64(name))
}

fn anubis_proof_input_bool_val(name: &str) -> AnubisValue {
    AnubisValue::Bool(anubis_proof_input_i64(name) != 0)
}

/// Named public output: commits one u32 to the journal.
/// The string name is for host-side journal_fields binding (extracted from guest source).
/// Order of proof_commit_u32 calls = order of journal u32 slots.
static ANUBIS_NAMED_COMMITS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

fn anubis_proof_commit_u32(_name: &str, v: AnubisValue) -> AnubisValue {
    let w: u32 = v.as_i64() as u32;
    env::commit(&w);
    ANUBIS_NAMED_COMMITS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    AnubisValue::Int(w as i64)
}

fn anubis_proof_commit_bool(name: &str, v: AnubisValue) -> AnubisValue {
    let b: u32 = if v.as_bool() { 1 } else { 0 };
    anubis_proof_commit_u32(name, AnubisValue::Int(b as i64))
}

/// In-circuit / guest assertion: false → panic → no valid receipt.
fn anubis_proof_assert(cond: AnubisValue) -> AnubisValue {
    if !cond.as_bool() {
        panic!("ANUBIS_PROOF_ASSERT_FAILED");
    }
    AnubisValue::Bool(true)
}

/// Commit public outputs to the RISC0 journal.
/// - If any `proof_commit_u32` ran, return is ignored (named fields already committed).
/// - Scalar int/bool/str → one little-endian u32 (v1-compatible).
/// - List → one u32 per element (multi-field journal). Nested lists use length as u32.
fn anubis_commit_journal(result: AnubisValue) {
    if ANUBIS_NAMED_COMMITS.load(std::sync::atomic::Ordering::SeqCst) > 0 {
        return;
    }
    match result {
        AnubisValue::List(items) => {
            for item in anubis_rc_take(items) {
                let w: u32 = match item {
                    AnubisValue::List(inner) => inner.len() as u32,
                    other => other.as_i64() as u32,
                };
                env::commit(&w);
            }
        }
        other => {
            let w: u32 = other.as_i64() as u32;
            env::commit(&w);
        }
    }
}
"#;

/// Injected into lowered programs when `--allow-research` enables the PoC kit.
/// Local process harness only; network URLs are rejected at runtime.
const POC_KIT_RUNTIME_RS: &str = r#"
use std::io::Write;
use std::process::{Command, Stdio};
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

fn anubis_to_bytes(v: &AnubisValue) -> Vec<u8> {
    match v {
        // RECURSE, matching the Enum/Struct/Map arms below. Mapping `as_i64() as u8` over the
        // elements silently coerced a NESTED list to its LENGTH, because `as_i64()` on a list
        // returns its element count: `flat([[1,2],[3]])` produced `[2, 1]` — the two inner
        // lengths — where payload assembly needs `[1, 2, 3]`.
        //
        // `flat` is how PoC payloads are built, so this is worse than a wrong answer: an exploit
        // "proves" something about bytes nobody assembled, and a proof-carrying language emits a
        // proof about the wrong artifact.
        //
        // Flat lists are unaffected — an `Int` element serialises to `vec![n as u8]` either way —
        // so this fixes nesting without changing the common case.
        AnubisValue::List(items) => items.iter().flat_map(anubis_to_bytes).collect(),
        AnubisValue::Str(s) => s.as_bytes().to_vec(),
        AnubisValue::Int(n) => vec![*n as u8],
        AnubisValue::Float(n) => vec![(*n as i64) as u8],
        AnubisValue::Bool(b) => vec![if *b { 1 } else { 0 }],
        // Non-byte payloads: flatten structured fields for research harness only.
        AnubisValue::Enum { fields, .. } => {
            fields.iter().flat_map(|x| anubis_to_bytes(x)).collect()
        }
        AnubisValue::Struct { fields, .. } => {
            fields.iter().flat_map(|(_, x)| anubis_to_bytes(x)).collect()
        }
        AnubisValue::Map(m) => m.iter().flat_map(|(_, x)| anubis_to_bytes(x)).collect(),
        AnubisValue::Closure(_) => vec![],
    }
}

/// Fail closed on a non-numeric argument to a pack/cyclic call. `.as_i64()` on a List returns
/// the list's LENGTH, on a Map returns the entry count, on a Struct returns the field count —
/// so `p8([9,9,9])` silently produced `[3]`, `p32([1,2,3,4,5])` produced `[5, 0, 0, 0]`, and
/// `cyclic({"a":1,"b":2})` produced a 2-char pattern. That is worse than a crash for the same
/// reason the `flat` recursion bug was: `flat`/`p*`/`cyclic` compose the bytes an exploit
/// asserts things about, so a silently-wrong pack means a proof-carrying program emits a proof
/// about the wrong artifact. Booleans and numeric strings are still accepted (they are
/// documented-lenient numeric coercions per LANGUAGE.md); only structured values are refused.
fn anubis_pack_require_numeric(fn_name: &str, v: &AnubisValue) {
    match v {
        AnubisValue::Int(_) | AnubisValue::Float(_) | AnubisValue::Bool(_) => {}
        AnubisValue::Str(s) => {
            let trimmed = s.trim();
            if trimmed.parse::<i64>().is_err() && trimmed.parse::<f64>().is_err() {
                panic!(
                    "ANUBIS_POC_PACK_TYPE: `{fn_name}` requires a numeric argument; got string `{s}` which does not parse as a number"
                );
            }
        }
        AnubisValue::List(_) => panic!(
            "ANUBIS_POC_PACK_TYPE: `{fn_name}` requires a numeric argument; got a list (use flat(list) to concatenate bytes, or pass an integer)"
        ),
        AnubisValue::Map(_) => panic!(
            "ANUBIS_POC_PACK_TYPE: `{fn_name}` requires a numeric argument; got a map"
        ),
        AnubisValue::Struct { ty, .. } => panic!(
            "ANUBIS_POC_PACK_TYPE: `{fn_name}` requires a numeric argument; got struct `{ty}`"
        ),
        AnubisValue::Enum { ty, tag, .. } => panic!(
            "ANUBIS_POC_PACK_TYPE: `{fn_name}` requires a numeric argument; got enum variant `{ty}::{tag}`"
        ),
        AnubisValue::Closure(_) => panic!(
            "ANUBIS_POC_PACK_TYPE: `{fn_name}` requires a numeric argument; got a closure"
        ),
    }
}

fn anubis_p8(v: AnubisValue) -> AnubisValue {
    anubis_pack_require_numeric("p8", &v);
    anubis_mk_list(vec![AnubisValue::Int((v.as_i64() as u8) as i64)])
}
fn anubis_p16(v: AnubisValue) -> AnubisValue {
    anubis_pack_require_numeric("p16", &v);
    let n = v.as_i64() as u16;
    anubis_mk_list(n.to_le_bytes().iter().map(|b| AnubisValue::Int(*b as i64)).collect())
}
fn anubis_p32(v: AnubisValue) -> AnubisValue {
    anubis_pack_require_numeric("p32", &v);
    let n = v.as_i64() as u32;
    anubis_mk_list(n.to_le_bytes().iter().map(|b| AnubisValue::Int(*b as i64)).collect())
}
fn anubis_p64(v: AnubisValue) -> AnubisValue {
    anubis_pack_require_numeric("p64", &v);
    let n = v.as_i64() as u64;
    anubis_mk_list(n.to_le_bytes().iter().map(|b| AnubisValue::Int(*b as i64)).collect())
}
fn anubis_cyclic(v: AnubisValue) -> AnubisValue {
    anubis_pack_require_numeric("cyclic", &v);
    // `.max(0)` silently coerced a negative length to 0 and returned `[]` — same shape as the
    // HKDF / PBKDF2 fixes: a caller that passes a signed-overflow value or a computed length
    // otherwise silently got an empty pattern, which cyclic_find would then report "not found"
    // for, hiding the real bug (bad length arithmetic) behind an already-known negative code path.
    let n_raw = v.as_i64();
    if n_raw < 0 {
        panic!(
            "ANUBIS_POC_CYCLIC_LENGTH: cyclic length must be >= 0, got {}",
            n_raw
        );
    }
    let n = n_raw as usize;
    let alphabet = b"abcdefghijklmnopqrstuvwxyz";
    anubis_mk_list((0..n).map(|i| AnubisValue::Int(alphabet[i % alphabet.len()] as i64)).collect())
}

/// A+ target_run result: named struct fields (and list-index compatible).
/// Fields (order preserved for r[0]..):
///   crashed (0/1), signal, exit_code, payload_len, timed_out (0/1)
fn anubis_target_run_result(
    crashed: i64,
    signal: i64,
    exit_code: i64,
    payload_len: i64,
    timed_out: i64,
) -> AnubisValue {
    AnubisValue::Struct {
        ty: "TargetRun".to_string(),
        fields: vec![
            ("crashed".to_string(), AnubisValue::Int(crashed)),
            ("signal".to_string(), AnubisValue::Int(signal)),
            ("exit_code".to_string(), AnubisValue::Int(exit_code)),
            ("payload_len".to_string(), AnubisValue::Int(payload_len)),
            ("timed_out".to_string(), AnubisValue::Int(timed_out)),
        ],
    }
}

/// target_run(path, payload) -> TargetRun struct
/// Named: r.crashed / r.signal / r.exit_code / r.payload_len / r.timed_out
/// Positional (compat): r[0]..r[3] via struct field order.
fn anubis_target_run(path_v: AnubisValue, payload_v: AnubisValue) -> AnubisValue {
    let path = path_v.display_string();
    if path.contains("://") || path.starts_with("http") {
        eprintln!("ANUBIS_POC_NETWORK_FORBIDDEN: target must be a local filesystem path");
        return anubis_target_run_result(0, -1, -1, 0, 0);
    }
    let payload = anubis_to_bytes(&payload_v);
    let mut child = match Command::new(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("ANUBIS_POC_SPAWN_FAILED: {}: {}", path, e);
            return anubis_target_run_result(0, -1, -1, payload.len() as i64, 0);
        }
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(&payload);
    }
    let start = std::time::Instant::now();
    let timeout_ms = 2000u128;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if start.elapsed().as_millis() > timeout_ms {
                    let _ = child.kill();
                    let _ = child.wait();
                    eprintln!("ANUBIS_POC_TIMEOUT");
                    return anubis_target_run_result(0, -1, -1, payload.len() as i64, 1);
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            Err(e) => {
                eprintln!("ANUBIS_POC_WAIT_FAILED: {}", e);
                return anubis_target_run_result(0, -1, -1, payload.len() as i64, 0);
            }
        }
    }
    let status = match child.wait() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ANUBIS_POC_WAIT_FAILED: {}", e);
            return anubis_target_run_result(0, -1, -1, payload.len() as i64, 0);
        }
    };
    #[cfg(unix)]
    let signal = status.signal().unwrap_or(-1);
    #[cfg(not(unix))]
    let signal = -1i32;
    let exit_code = status.code().unwrap_or(-1);
    let crashed = if signal > 0 { 1 } else { 0 };
    anubis_target_run_result(
        crashed,
        signal as i64,
        exit_code as i64,
        payload.len() as i64,
        0,
    )
}
"#;

fn emit_safe_run_stmt(stmt: &Stmt, indent: usize, out: &mut String, ctx: &EmitCtx) -> Result<()> {
    let pad = "    ".repeat(indent);
    match stmt {
        Stmt::Let { name, init, .. } => {
            let id = sanitize_ident(name)?;
            out.push_str(&format!(
                "{pad}let {}{} = {};\n",
                mut_prefix(&id),
                id,
                safe_run_expr(init, ctx)?
            ));
            Ok(())
        }
        Stmt::LetPattern { pattern, init, .. } => {
            // Destructuring binding: evaluate the initializer once into a temp, then bind the
            // pattern's names into the current scope (irrefutable — the test is not enforced).
            let init_src = safe_run_expr(init, ctx)?;
            let tmp = format!("__anb_dst{}", next_temp_id());
            out.push_str(&format!("{pad}let {tmp} = {init_src};\n"));
            let (_test, binds) = pattern_test_and_binds(pattern, &tmp)?;
            if !binds.trim().is_empty() {
                out.push_str(&format!("{pad}{binds}\n"));
            }
            Ok(())
        }
        Stmt::Assign { target, value } => {
            let rhs = safe_run_expr(value, ctx)?;
            match target {
                // Plain variable: direct rebinding (cheap, common case).
                Expr::Var(name) => {
                    out.push_str(&format!("{pad}{} = {};\n", sanitize_ident(name)?, rhs));
                }
                // Any nested place (`a[i]`, `a.b`, `a.b[i].c`, …): descend and set in place.
                _ => {
                    let (root, segs) = emit_place(target, ctx)?;
                    out.push_str(&format!(
                        "{pad}{}.set_at(&[{}], {});\n",
                        root,
                        segs.join(", "),
                        rhs
                    ));
                }
            }
            Ok(())
        }
        Stmt::ExprStmt(Expr::Call { callee, args }) if callee == "push" => {
            if args.len() == 2 {
                if let Expr::Var(name) = &args[0] {
                    out.push_str(&format!(
                        "{pad}{}.push_val({});\n",
                        sanitize_ident(name)?,
                        safe_run_expr(&args[1], ctx)?
                    ));
                    return Ok(());
                }
            }
            Err(unsupported_run(
                "push(list, value) requires a variable list as its first argument",
            ))
        }
        Stmt::ExprStmt(Expr::Call { callee, args })
            if matches!(callee.as_str(), "print" | "println" | "eprint" | "eprintln") =>
        {
            // `print`/`println` -> stdout, `eprint`/`eprintln` -> stderr. Multiple arguments are
            // space-separated; zero arguments emit a blank line.
            let macro_name = if callee.starts_with('e') {
                "eprintln"
            } else {
                "println"
            };
            let mut parts = Vec::new();
            for a in args {
                parts.push(format!("{}.display_string()", safe_run_expr(a, ctx)?));
            }
            if parts.is_empty() {
                out.push_str(&format!("{pad}{}!();\n", macro_name));
            } else {
                let fmt = vec!["{}"; parts.len()].join(" ");
                out.push_str(&format!(
                    "{pad}{}!(\"{}\", {});\n",
                    macro_name,
                    fmt,
                    parts.join(", ")
                ));
            }
            Ok(())
        }
        Stmt::ExprStmt(Expr::Call { callee, args }) if callee == "return" => {
            let val = match args.first() {
                Some(expr) => safe_run_expr(expr, ctx)?,
                None => "AnubisValue::Int(0)".to_string(),
            };
            out.push_str(&format!("{pad}return {};\n", val));
            Ok(())
        }
        Stmt::ExprStmt(expr) => {
            out.push_str(&format!("{pad}let _ = {};\n", safe_run_expr(expr, ctx)?));
            Ok(())
        }
        Stmt::If { cond, then, else_ } => {
            out.push_str(&format!(
                "{pad}if {}.as_bool() {{\n",
                safe_run_expr(cond, ctx)?
            ));
            for stmt in then {
                emit_safe_run_stmt(stmt, indent + 1, out, ctx)?;
            }
            out.push_str(&format!("{pad}}}"));
            if let Some(else_body) = else_ {
                out.push_str(" else {\n");
                for stmt in else_body {
                    emit_safe_run_stmt(stmt, indent + 1, out, ctx)?;
                }
                out.push_str(&format!("{pad}}}\n"));
            } else {
                out.push('\n');
            }
            Ok(())
        }
        Stmt::WhileLet {
            pattern,
            expr,
            body,
        } => {
            let tmp = format!("__anb_wl{}", next_temp_id());
            out.push_str(&format!("{pad}loop {{\n"));
            let scr = safe_run_expr(expr, ctx)?;
            let (test, binds) = pattern_test_and_binds(pattern, &tmp)?;
            out.push_str(&format!("{pad}    let {tmp} = {scr};\n"));
            out.push_str(&format!("{pad}    if {test} {{\n{pad}        {binds}\n"));
            for stmt in body {
                emit_safe_run_stmt(stmt, indent + 2, out, ctx)?;
            }
            out.push_str(&format!("{pad}    }} else {{ break; }}\n{pad}}}\n"));
            Ok(())
        }
        Stmt::While { cond, body, .. } => {
            out.push_str(&format!(
                "{pad}while {}.as_bool() {{\n",
                safe_run_expr(cond, ctx)?
            ));
            for stmt in body {
                emit_safe_run_stmt(stmt, indent + 1, out, ctx)?;
            }
            out.push_str(&format!("{pad}}}\n"));
            Ok(())
        }
        Stmt::Loop { body, .. } => {
            out.push_str(&format!("{pad}loop {{\n"));
            for stmt in body {
                emit_safe_run_stmt(stmt, indent + 1, out, ctx)?;
            }
            out.push_str(&format!("{pad}}}\n"));
            Ok(())
        }
        Stmt::For {
            var, source, body, ..
        } => {
            use crate::frontend::ForSource;
            let v = sanitize_ident(var)?;
            // Both forms lower to a native Rust `for`, so `break`/`continue` behave correctly
            // (in particular `continue` advances the iterator instead of skipping an increment).
            match source {
                ForSource::Range { start, end } => {
                    let iv = format!("__anb_i_{}", indent);
                    out.push_str(&format!(
                        "{pad}for {} in ({}).as_i64()..({}).as_i64() {{\n",
                        iv,
                        safe_run_expr(start, ctx)?,
                        safe_run_expr(end, ctx)?
                    ));
                    out.push_str(&format!(
                        "{pad}    let {}{} = AnubisValue::Int({});\n",
                        mut_prefix(&v),
                        v,
                        iv
                    ));
                    for stmt in body {
                        emit_safe_run_stmt(stmt, indent + 1, out, ctx)?;
                    }
                    out.push_str(&format!("{pad}}}\n"));
                    Ok(())
                }
                ForSource::Collection { expr } => {
                    // Iterate list items / string characters / map keys.
                    out.push_str(&format!(
                        "{pad}for {}{} in anubis_iter({}) {{\n",
                        mut_prefix(&v),
                        v,
                        safe_run_expr(expr, ctx)?
                    ));
                    for stmt in body {
                        emit_safe_run_stmt(stmt, indent + 1, out, ctx)?;
                    }
                    out.push_str(&format!("{pad}}}\n"));
                    Ok(())
                }
            }
        }
        Stmt::Break => {
            out.push_str(&format!("{pad}break;\n"));
            Ok(())
        }
        Stmt::Continue => {
            out.push_str(&format!("{pad}continue;\n"));
            Ok(())
        }
        Stmt::ResearchBlock { body, .. } | Stmt::ExploitBlock { body, .. } => {
            if !ctx.allow_research {
                return Err(unsupported_run(
                    "research/exploit blocks require `anubis run --allow-research`",
                ));
            }
            for stmt in body {
                emit_safe_run_stmt(stmt, indent, out, ctx)?;
            }
            Ok(())
        }
        Stmt::HybridBlock { .. } | Stmt::SpecBlock { .. } => Err(unsupported_run(format!(
            "unsupported statement for run: {:?}",
            std::mem::discriminant(stmt)
        ))),
    }
}

/// Names that are analysis/proof constructs, not executable user functions in the safe run path.
/// Phase-3 C3: capability I/O (`read_file`/`write_file`/`open`/`send`/`connect`/`time`/`rand`) is
/// no longer rejected here — those emit real stdlib calls via `emit_builtin_call`. Shell/exec/sql
/// and pure analysis constructs remain non-run.
fn is_non_run_builtin(callee: &str) -> bool {
    matches!(
        callee,
        "symbolic"
            | "assume"
            | "assert"
            | "taint_source"
            | "declassify"
            | "sink"
            | "shell"
            | "exec"
            | "system"
            | "memcpy"
            | "sql" // `http_get`/`http_post` lower via `anubis_http_*` (cleartext http:// only).
                    // `shell`/`exec` remain non-run by design.
    )
}

fn is_poc_kit_builtin(callee: &str) -> bool {
    matches!(
        callee,
        "p8" | "p16" | "p32" | "p64" | "cyclic" | "target_run" | "flat"
    )
}

fn is_proof_input_builtin(callee: &str) -> bool {
    matches!(
        callee,
        "proof_input_u32"
            | "proof_input_bool"
            | "proof_input_u64"
            | "proof_commit_u32"
            | "proof_commit_bool"
            | "proof_assert"
    )
}

/// Decompose an lvalue expression into `(root_variable, path_segments)`, where each segment is
/// emitted Rust of type `AnubisPathSeg` (`Field` or `Index`). Supports arbitrary nesting such as
/// `a.b[i].c`. Errors if the place does not bottom out at a variable.
fn emit_place(target: &Expr, ctx: &EmitCtx) -> Result<(String, Vec<String>)> {
    match target {
        Expr::Var(name) => Ok((sanitize_ident(name)?, Vec::new())),
        Expr::FieldAccess { base, field, .. } => {
            let (root, mut segs) = emit_place(base, ctx)?;
            segs.push(format!(
                "AnubisPathSeg::Field({}.to_string())",
                rust_string_lit(field)?
            ));
            Ok((root, segs))
        }
        Expr::Index { base, index } => {
            let (root, mut segs) = emit_place(base, ctx)?;
            segs.push(format!(
                "AnubisPathSeg::Index({})",
                safe_run_expr(index, ctx)?
            ));
            Ok((root, segs))
        }
        _ => Err(unsupported_run(
            "assignment target must be a variable, field access, or index place",
        )),
    }
}

/// True if `name` is a reserved builtin (stdlib, statement builtin, proof/poc/analysis construct).
/// Used to avoid capturing builtin names as free variables in lambdas, and by the typechecker to
/// decide whether an unknown call is a real error.
pub fn is_builtin_name(name: &str) -> bool {
    emit_builtin_call(name, &[]).is_some()
        || matches!(
            name,
            "len"
                | "pop"
                | "push"
                | "insert"
                | "remove"
                | "print"
                | "println"
                | "eprint"
                | "eprintln"
                | "return"
                | "break"
                | "continue"
                // Phase-2 capability + confidentiality labels: also lowered via emit_builtin_call
                // (anubis_cap_acquire / anubis_cap_use / anubis_secret_source). Kept here as a
                // belt-and-braces name set if emit dispatch is ever probed with empty args only.
                | "cap_acquire"
                | "cap_acquire_nonexportable"
                | "cap_export"
                | "cap_use"
                | "keychain_se_probe"
                | "keychain_se_last_bind"
                | "secret_source"
        )
        || is_proof_input_builtin(name)
        || is_poc_kit_builtin(name)
        || is_non_run_builtin(name)
}

/// Collect the free identifiers of an expression, split into value uses (`vars`, always captured
/// by a lambda) and callee uses (`callees`, captured only when they are not a user function or a
/// builtin — i.e. when they name a closure-valued local). A name used as a value takes precedence.
fn collect_free_expr(
    e: &Expr,
    bound: &std::collections::BTreeSet<String>,
    vars: &mut std::collections::BTreeSet<String>,
    callees: &mut std::collections::BTreeSet<String>,
) {
    match e {
        Expr::Var(n) => {
            if !bound.contains(n) {
                vars.insert(n.clone());
            }
        }
        Expr::Literal(_)
        | Expr::StrLiteral(_)
        | Expr::Symbolic { .. }
        | Expr::RawPtr { .. }
        | Expr::UnifiedBuffer { .. }
        | Expr::TaintSource { .. }
        | Expr::Other(_) => {}
        Expr::Call { callee, args } => {
            if !bound.contains(callee) {
                callees.insert(callee.clone());
            }
            for a in args {
                collect_free_expr(a, bound, vars, callees);
            }
        }
        Expr::CallExpr { callee, args } => {
            collect_free_expr(callee, bound, vars, callees);
            for a in args {
                collect_free_expr(a, bound, vars, callees);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            collect_free_expr(lhs, bound, vars, callees);
            collect_free_expr(rhs, bound, vars, callees);
        }
        Expr::Unary { expr, .. }
        | Expr::Cast { expr, .. }
        | Expr::Assume(expr)
        | Expr::Assert(expr)
        | Expr::Try(expr) => collect_free_expr(expr, bound, vars, callees),
        Expr::Tainted { inner, .. } | Expr::Declassify { inner, .. } => {
            collect_free_expr(inner, bound, vars, callees)
        }
        Expr::ArrayLiteral { elements } => {
            for el in elements {
                collect_free_expr(el, bound, vars, callees);
            }
        }
        Expr::Index { base, index } => {
            collect_free_expr(base, bound, vars, callees);
            collect_free_expr(index, bound, vars, callees);
        }
        Expr::StructLiteral { fields, .. } => {
            for (_, v) in fields {
                collect_free_expr(v, bound, vars, callees);
            }
        }
        Expr::FieldAccess { base, .. } => collect_free_expr(base, bound, vars, callees),
        Expr::EnumConstruct { fields, .. } => {
            for f in fields {
                collect_free_expr(f, bound, vars, callees);
            }
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            collect_free_expr(scrutinee, bound, vars, callees);
            for arm in arms {
                let mut b2 = bound.clone();
                for x in arm.pattern.bound_names() {
                    b2.insert(x);
                }
                if let Some(guard) = &arm.guard {
                    collect_free_expr(guard, &b2, vars, callees);
                }
                collect_free_expr(&arm.body, &b2, vars, callees);
            }
        }
        Expr::If {
            cond, then, else_, ..
        } => {
            collect_free_expr(cond, bound, vars, callees);
            collect_free_expr(then, bound, vars, callees);
            collect_free_expr(else_, bound, vars, callees);
        }
        Expr::IfLet {
            pattern,
            scrutinee,
            then,
            else_,
            ..
        } => {
            collect_free_expr(scrutinee, bound, vars, callees);
            let mut b2 = bound.clone();
            for n in pattern.bound_names() {
                b2.insert(n);
            }
            collect_free_expr(then, &b2, vars, callees);
            collect_free_expr(else_, bound, vars, callees);
        }
        Expr::MapLiteral { entries, .. } => {
            for (k, v) in entries {
                collect_free_expr(k, bound, vars, callees);
                collect_free_expr(v, bound, vars, callees);
            }
        }
        Expr::Block { stmts, tail } => {
            let mut b2 = bound.clone();
            collect_free_stmts(stmts, &mut b2, vars, callees);
            if let Some(t) = tail {
                collect_free_expr(t, &b2, vars, callees);
            }
        }
        Expr::Lambda { params, body } => {
            let mut b2 = bound.clone();
            for p in params {
                b2.insert(p.clone());
            }
            collect_free_expr(body, &b2, vars, callees);
        }
    }
}

fn collect_free_stmts(
    stmts: &[Stmt],
    bound: &mut std::collections::BTreeSet<String>,
    vars: &mut std::collections::BTreeSet<String>,
    callees: &mut std::collections::BTreeSet<String>,
) {
    use crate::frontend::ForSource;
    for s in stmts {
        match s {
            Stmt::Let { name, init, .. } => {
                collect_free_expr(init, bound, vars, callees);
                bound.insert(name.clone());
            }
            Stmt::LetPattern { pattern, init, .. } => {
                collect_free_expr(init, bound, vars, callees);
                for n in pattern.bound_names() {
                    bound.insert(n);
                }
            }
            Stmt::Assign { target, value } => {
                collect_free_expr(target, bound, vars, callees);
                collect_free_expr(value, bound, vars, callees);
            }
            Stmt::ExprStmt(e) => collect_free_expr(e, bound, vars, callees),
            Stmt::If { cond, then, else_ } => {
                collect_free_expr(cond, bound, vars, callees);
                let mut b = bound.clone();
                collect_free_stmts(then, &mut b, vars, callees);
                if let Some(e) = else_ {
                    let mut b = bound.clone();
                    collect_free_stmts(e, &mut b, vars, callees);
                }
            }
            Stmt::While { cond, body, .. } => {
                collect_free_expr(cond, bound, vars, callees);
                let mut b = bound.clone();
                collect_free_stmts(body, &mut b, vars, callees);
            }
            Stmt::WhileLet {
                pattern,
                expr,
                body,
            } => {
                collect_free_expr(expr, bound, vars, callees);
                let mut b = bound.clone();
                for n in pattern.bound_names() {
                    b.insert(n);
                }
                collect_free_stmts(body, &mut b, vars, callees);
            }
            Stmt::Loop { body, .. } => {
                let mut b = bound.clone();
                collect_free_stmts(body, &mut b, vars, callees);
            }
            Stmt::For {
                var, source, body, ..
            } => {
                match source {
                    ForSource::Range { start, end } => {
                        collect_free_expr(start, bound, vars, callees);
                        collect_free_expr(end, bound, vars, callees);
                    }
                    ForSource::Collection { expr } => collect_free_expr(expr, bound, vars, callees),
                }
                let mut b = bound.clone();
                b.insert(var.clone());
                collect_free_stmts(body, &mut b, vars, callees);
            }
            Stmt::ResearchBlock { body, .. } | Stmt::ExploitBlock { body, .. } => {
                let mut b = bound.clone();
                collect_free_stmts(body, &mut b, vars, callees);
            }
            Stmt::HybridBlock { gpu, cpu, prove } => {
                for blk in [gpu, cpu, prove].into_iter().flatten() {
                    let mut b = bound.clone();
                    collect_free_stmts(blk, &mut b, vars, callees);
                }
            }
            Stmt::Break | Stmt::Continue | Stmt::SpecBlock { .. } => {}
        }
    }
}

/// Dispatch a standard-library builtin call. Returns `None` if `callee` is not a builtin (so it
/// falls through to a user-defined function call). `args` are already-lowered Rust expressions.
fn emit_builtin_call(callee: &str, args: &[String]) -> Option<Result<String>> {
    // Fixed-arity builtin → `anubis_fn(args...)`, with an arity check.
    fn fixed(fn_name: &str, callee: &str, args: &[String], arity: usize) -> Result<String> {
        if args.len() != arity {
            return Err(unsupported_run(format!(
                "`{}` expects {} argument(s), got {}",
                callee,
                arity,
                args.len()
            )));
        }
        Ok(format!("{}({})", fn_name, args.join(", ")))
    }
    let r = match callee {
        // conversions / reflection
        "str" => fixed("anubis_str", callee, args, 1),
        "int" => fixed("anubis_int", callee, args, 1),
        "float" => fixed("anubis_float", callee, args, 1),
        "bool" => fixed("anubis_bool_of", callee, args, 1),
        "type" => fixed("anubis_type_of", callee, args, 1),
        "parse_int" => fixed("anubis_parse_int", callee, args, 1),
        "parse_float" => fixed("anubis_parse_float", callee, args, 1),
        "parse_int_opt" => fixed("anubis_parse_int_opt", callee, args, 1),
        "parse_float_opt" => fixed("anubis_parse_float_opt", callee, args, 1),
        // math
        "abs" => fixed("anubis_abs", callee, args, 1),
        "sqrt" => fixed("anubis_sqrt", callee, args, 1),
        "floor" => fixed("anubis_floor", callee, args, 1),
        "ceil" => fixed("anubis_ceil", callee, args, 1),
        "round" => fixed("anubis_round", callee, args, 1),
        "pow" => fixed("anubis_pow", callee, args, 2),
        "gcd" => fixed("anubis_gcd", callee, args, 2),
        "min" => Ok(format!("anubis_min(vec![{}])", args.join(", "))),
        "max" => Ok(format!("anubis_max(vec![{}])", args.join(", "))),
        // strings
        "upper" => fixed("anubis_upper", callee, args, 1),
        "lower" => fixed("anubis_lower", callee, args, 1),
        "trim" => fixed("anubis_trim", callee, args, 1),
        "split" => fixed("anubis_split", callee, args, 2),
        "join" => fixed("anubis_join", callee, args, 2),
        "contains" => fixed("anubis_contains", callee, args, 2),
        "starts_with" => fixed("anubis_starts_with", callee, args, 2),
        "ends_with" => fixed("anubis_ends_with", callee, args, 2),
        "replace" => fixed("anubis_replace", callee, args, 3),
        "index_of" => fixed("anubis_index_of", callee, args, 2),
        "ord" => fixed("anubis_ord", callee, args, 1),
        "chr" => fixed("anubis_chr", callee, args, 1),
        "repeat" => fixed("anubis_repeat", callee, args, 2),
        "substr" => fixed("anubis_substr", callee, args, 3),
        "char_at" if args.len() == 2 => Ok(format!("({}).index_get({})", args[0], args[1])),
        "char_at" => Err(unsupported_run("`char_at` expects 2 arguments")),
        // sequences
        "slice" => fixed("anubis_slice", callee, args, 3),
        "reverse" => fixed("anubis_reverse", callee, args, 1),
        "sort" => fixed("anubis_sort", callee, args, 1),
        "sum" => fixed("anubis_sum", callee, args, 1),
        "range" if args.len() == 2 => Ok(format!("anubis_range({}, {})", args[0], args[1])),
        "range" if args.len() == 3 => Ok(format!(
            "anubis_range_step({}, {}, {})",
            args[0], args[1], args[2]
        )),
        "range" => Err(unsupported_run("`range` expects 2 or 3 arguments")),
        // maps
        "keys" => fixed("anubis_keys", callee, args, 1),
        "values" => fixed("anubis_values", callee, args, 1),
        "has_key" => fixed("anubis_has_key", callee, args, 2),
        // higher-order (closures)
        "map" => fixed("anubis_map", callee, args, 2),
        "filter" => fixed("anubis_filter", callee, args, 2),
        "reduce" if args.len() == 2 => fixed("anubis_reduce2", callee, args, 2),
        "reduce" if args.len() == 3 => fixed("anubis_reduce", callee, args, 3),
        "reduce" => Err(unsupported_run(
            "`reduce` expects 2 or 3 arguments: reduce(list, closure) or reduce(list, closure, seed)",
        )),
        "each" => fixed("anubis_each", callee, args, 2),
        "find" => fixed("anubis_find", callee, args, 2),
        "any" => fixed("anubis_any", callee, args, 2),
        "all" => fixed("anubis_all", callee, args, 2),
        "count" => fixed("anubis_count_by", callee, args, 2),
        "sort_by" => fixed("anubis_sort_by", callee, args, 2),
        "apply" => fixed("anubis_apply", callee, args, 2),
        "call" if !args.is_empty() => Ok(format!(
            "({}).call_closure(vec![{}])",
            args[0],
            args[1..].join(", ")
        )),
        "call" => Err(unsupported_run("`call` requires a closure argument")),
        // control / io
        "assert" => fixed("anubis_assert", callee, args, 1),
        "panic" => fixed("anubis_panic", callee, args, 1),
        "input" | "read_line" => fixed("anubis_input", callee, args, 0),
        "args" => fixed("anubis_args", callee, args, 0),
        // Phase-8: process exit for fail-closed CLI tools (self-host driver)
        "exit" => {
            if args.len() == 1 {
                Ok(format!(
                    "{{ std::process::exit(({}).as_i64() as i32); AnubisValue::Int(0) }}",
                    args[0]
                ))
            } else if args.is_empty() {
                // Probe from is_builtin_name(&[], ...) — treat as known builtin.
                Ok("/* exit */ AnubisValue::Int(0)".into())
            } else {
                Err(unsupported_run("`exit` expects 1 argument"))
            }
        }
        // Phase-3 C3: governed capability I/O (additive; programs without these names emit unchanged)
        "read_file" => fixed("anubis_read_file", callee, args, 1),
        "write_file" | "write" => fixed("anubis_write_file", callee, args, 2),
        "append_file" => fixed("anubis_append_file", callee, args, 2),
        "delete_file" | "remove_file" => fixed("anubis_delete_file", callee, args, 1),
        "open" => fixed("anubis_open", callee, args, 1),
        // Verified-lane linear capabilities + confidentiality mint — fully executable.
        // nonexportable uses Keychain/SE bind on macOS (see keychain_se_runtime.inc.rs).
        "cap_acquire" => fixed("anubis_cap_acquire", callee, args, 1),
        "cap_acquire_nonexportable" => fixed("anubis_cap_acquire_nonexportable", callee, args, 1),
        "cap_export" => fixed("anubis_cap_export", callee, args, 2),
        "cap_use" => fixed("anubis_cap_use", callee, args, 1),
        "keychain_se_probe" => fixed("anubis_keychain_se_probe", callee, args, 0),
        "keychain_se_last_bind" => fixed("anubis_keychain_se_last_bind", callee, args, 0),
        "secret_source" => fixed("anubis_secret_source", callee, args, 1),
        // Cryptography (SHA-256 / HMAC-SHA256) — pure std in emitted runtime
        // Cryptography — RWC-aligned surface (pure embedded runtime; no cargo deps in `anubis run`)
        // hash_sha256 is the STDLIB_CORE name; sha256 / sha256_hex are aliases (NIST vector unit).
        "sha256" | "sha256_hex" | "hash_sha256" => fixed("anubis_sha256", callee, args, 1),
        "sha256_bytes" => fixed("anubis_sha256_bytes_val", callee, args, 1),
        "bytes_hex" | "to_hex" => fixed("anubis_bytes_hex", callee, args, 1),
        "hmac_sha256" | "hmac_sha256_hex" => fixed("anubis_hmac_sha256", callee, args, 2),
        "hmac_sha256_bytes" => fixed("anubis_hmac_sha256_bytes", callee, args, 2),
        "hmac_sha256_verify" => fixed("anubis_hmac_sha256_verify", callee, args, 3),
        "ct_eq" | "constant_time_eq" => fixed("anubis_ct_eq", callee, args, 2),
        "hkdf_sha256" => fixed("anubis_hkdf_sha256", callee, args, 4),
        "domain_hash" => fixed("anubis_domain_hash", callee, args, 2),
        "tuple_hash" => fixed("anubis_tuple_hash", callee, args, 2),
        "random_bytes" => fixed("anubis_random_bytes", callee, args, 1),
        "aead_nonce_from_counter" => fixed("anubis_aead_nonce_from_counter", callee, args, 1),
        "aead_seal" | "chacha20_poly1305_seal" => fixed("anubis_aead_seal", callee, args, 4),
        "aead_open" | "chacha20_poly1305_open" => fixed("anubis_aead_open", callee, args, 4),
        "x25519_keygen" => fixed("anubis_x25519_keygen", callee, args, 0),
        "x25519_public_key" => fixed("anubis_x25519_public_key", callee, args, 1),
        "x25519_shared" => fixed("anubis_x25519_shared", callee, args, 2),
        "hybrid_seal" => fixed("anubis_hybrid_seal", callee, args, 3),
        "hybrid_open" => fixed("anubis_hybrid_open", callee, args, 5),
        // Password hashing (RWC: Argon2id preferred; PBKDF2-HMAC-SHA256 acceptable with high iters)
        "pbkdf2_hmac_sha256" => fixed("anubis_pbkdf2_hmac_sha256", callee, args, 4),
        "argon2id_hash" => fixed("anubis_argon2id_hash", callee, args, 6),
        "password_hash_encode" | "password_hash" => {
            fixed("anubis_password_hash_encode", callee, args, 1)
        }
        "password_verify_encoding" | "password_verify" => {
            fixed("anubis_password_verify_encoding", callee, args, 2)
        }
        "password_hash_pbkdf2_encode" => fixed("anubis_password_hash_pbkdf2_encode", callee, args, 1),
        "password_hash_phc" | "password_hash_phc_raw" => {
            fixed("anubis_password_hash_phc", callee, args, 1)
        }
        "ed25519_keygen" => fixed("anubis_ed25519_keygen", callee, args, 0),
        "ed25519_public_key" => fixed("anubis_ed25519_public_key", callee, args, 1),
        "ed25519_sign" => fixed("anubis_ed25519_sign", callee, args, 2),
        "ed25519_verify" => fixed("anubis_ed25519_verify", callee, args, 3),
        "crypto_backend" => fixed("anubis_crypto_backend", callee, args, 0),
        "env" | "getenv" => fixed("anubis_env", callee, args, 1),
        "send" | "network_send" if args.len() == 3 => Ok(format!(
            "anubis_net_send({}, {}, {})",
            args[0], args[1], args[2]
        )),
        "send" | "network_send" => Err(unsupported_run(
            "`send` expects 3 arguments (host, port, payload)",
        )),
        "connect" if args.len() == 2 => Ok(format!("anubis_net_connect({}, {})", args[0], args[1])),
        "connect" => Err(unsupported_run(
            "`connect` expects 2 arguments (host, port)",
        )),
        "http_get" if args.len() == 1 => Ok(format!("anubis_http_get({})", args[0])),
        "http_get" => Err(unsupported_run("`http_get` expects 1 argument (url)")),
        "http_post" if args.len() == 2 => {
            Ok(format!("anubis_http_post({}, {})", args[0], args[1]))
        }
        "http_post" => Err(unsupported_run(
            "`http_post` expects 2 arguments (url, body)",
        )),
        "time" | "time_now" | "now" => fixed("anubis_time_now", callee, args, 0),
        "rand" | "rand_gen" | "random" => fixed("anubis_rand_gen", callee, args, 0),
        // extended math
        "sin" => fixed("anubis_sin", callee, args, 1),
        "cos" => fixed("anubis_cos", callee, args, 1),
        "tan" => fixed("anubis_tan", callee, args, 1),
        "asin" => fixed("anubis_asin", callee, args, 1),
        "acos" => fixed("anubis_acos", callee, args, 1),
        "atan" => fixed("anubis_atan", callee, args, 1),
        "atan2" => fixed("anubis_atan2", callee, args, 2),
        "exp" => fixed("anubis_exp", callee, args, 1),
        "ln" => fixed("anubis_ln", callee, args, 1),
        "log10" => fixed("anubis_log10", callee, args, 1),
        "log2" => fixed("anubis_log2", callee, args, 1),
        "log" => fixed("anubis_logb", callee, args, 2),
        "cbrt" => fixed("anubis_cbrt", callee, args, 1),
        "hypot" => fixed("anubis_hypot", callee, args, 2),
        "trunc" => fixed("anubis_trunc", callee, args, 1),
        "sign" => fixed("anubis_sign", callee, args, 1),
        "clamp" => fixed("anubis_clamp", callee, args, 3),
        "pi" => fixed("anubis_pi", callee, args, 0),
        "e" => fixed("anubis_e", callee, args, 0),
        "factorial" => fixed("anubis_factorial", callee, args, 1),
        // extended strings
        "chars" => fixed("anubis_chars", callee, args, 1),
        "words" => fixed("anubis_words", callee, args, 1),
        "lines" => fixed("anubis_lines", callee, args, 1),
        "capitalize" => fixed("anubis_capitalize", callee, args, 1),
        "pad_start" => fixed_pad(callee, args, true),
        "pad_end" => fixed_pad(callee, args, false),
        // extended lists
        "zip" => fixed("anubis_zip", callee, args, 2),
        "enumerate" => fixed("anubis_enumerate", callee, args, 1),
        "flatten" => fixed("anubis_flatten", callee, args, 1),
        "flat_map" => fixed("anubis_flat_map", callee, args, 2),
        "unique" => fixed("anubis_unique", callee, args, 1),
        "take" => fixed("anubis_take", callee, args, 2),
        "drop" => fixed("anubis_drop", callee, args, 2),
        "take_while" => fixed("anubis_take_while", callee, args, 2),
        "drop_while" => fixed("anubis_drop_while", callee, args, 2),
        "chunk" => fixed("anubis_chunk", callee, args, 2),
        "window" => fixed("anubis_window", callee, args, 2),
        "position" => fixed("anubis_position", callee, args, 2),
        "product" => fixed("anubis_product", callee, args, 1),
        "first" => fixed("anubis_first", callee, args, 1),
        "last" => fixed("anubis_last", callee, args, 1),
        "is_empty" => fixed("anubis_is_empty", callee, args, 1),
        "concat" => fixed("anubis_concat", callee, args, 2),
        "min_by" => fixed("anubis_min_by", callee, args, 2),
        "max_by" => fixed("anubis_max_by", callee, args, 2),
        "partition" => fixed("anubis_partition", callee, args, 2),
        // extended maps
        "entries" => fixed("anubis_entries", callee, args, 1),
        "get" => fixed("anubis_get", callee, args, 3),
        "merge" => fixed("anubis_merge", callee, args, 2),
        "map_values" => fixed("anubis_map_values", callee, args, 2),
        // functional
        "identity" => fixed("anubis_identity", callee, args, 1),
        "compose" => fixed("anubis_compose", callee, args, 2),
        "times" => fixed("anubis_times", callee, args, 2),
        _ => return None,
    };
    Some(r)
}

/// `pad_start`/`pad_end` accept `(s, width)` (space fill) or `(s, width, pad)`.
fn fixed_pad(callee: &str, args: &[String], at_start: bool) -> Result<String> {
    match args.len() {
        2 => Ok(format!(
            "anubis_pad({}, {}, anubis_mk_str(\" \".to_string()), {})",
            args[0], args[1], at_start
        )),
        3 => Ok(format!(
            "anubis_pad({}, {}, {}, {})",
            args[0], args[1], args[2], at_start
        )),
        n => Err(unsupported_run(format!(
            "`{}` expects 2 or 3 arguments, got {}",
            callee, n
        ))),
    }
}

/// Lower a bare identifier used in *value* position (not as a call target). A local binding is a
/// plain cloned value; a free function or a stdlib builtin referenced by name becomes a first-class
/// closure value, so it can be handed to a higher-order function (`map(xs, my_fn)`,
/// `compose(f, identity)`); any other bare name is undefined and rejected with a clean diagnostic
/// (rather than leaking a raw rustc "cannot find value" error).
fn var_as_value(name: &str, ctx: &EmitCtx) -> Result<String> {
    // A local (param / let / loop / match binding) shadows any function or builtin of the same
    // name and is simply cloned.
    if ctx.locals.contains(name) {
        return Ok(format!("{}.clone()", sanitize_ident(name)?));
    }
    // A free function referenced by bare name → a closure that calls it with its declared arity.
    // Missing arguments default to Int(0) (as method dispatch does) so passing an N-ary function
    // where fewer arguments are supplied — e.g. `map([1,2,3], add)` — pads rather than panicking.
    if let Some(&arity) = ctx.fn_arities.get(name) {
        let args: Vec<String> = (0..arity)
            .map(|i| format!("__args.get({i}usize).cloned().unwrap_or(AnubisValue::Int(0))"))
            .collect();
        return Ok(format!(
            "AnubisValue::Closure(std::rc::Rc::new(move |__args: Vec<AnubisValue>| -> AnubisValue {{ anb_{}({}) }}))",
            sanitize_ident(name)?,
            args.join(", ")
        ));
    }
    // Variadic builtins (`min`/`max` accept any number of arguments, or a single list) forward
    // the entire argument vector, so `reduce(xs, max, seed)` sees both (acc, x) and
    // `apply(min, xs)` sees every element.
    if matches!(name, "min" | "max") {
        return Ok(format!(
            "AnubisValue::Closure(std::rc::Rc::new(move |__args: Vec<AnubisValue>| -> AnubisValue {{ anubis_{name}(__args) }}))"
        ));
    }
    // Output builtins as values (`each(xs, print)`): print all arguments, yield Int(0).
    if matches!(name, "print" | "println" | "eprint" | "eprintln") {
        let mac = if name.starts_with('e') {
            "eprintln"
        } else {
            "println"
        };
        return Ok(format!(
            "AnubisValue::Closure(std::rc::Rc::new(move |__args: Vec<AnubisValue>| -> AnubisValue {{ {mac}!(\"{{}}\", __args.iter().map(|a| a.display_string()).collect::<Vec<_>>().join(\" \")); AnubisValue::Int(0) }}))"
        ));
    }
    if name == "len" {
        // Guard the argument access: the direct `__args[0usize]` aborted with a raw Rust
        // index-out-of-bounds panic (leaking generated-source line + backtrace) when `len` was used as a
        // first-class value and applied with zero args (`apply(len, [])`). Emit the clean ANUBIS_ARITY
        // diagnostic its sibling builtin value-forms produce instead (HOF-audit finding).
        return Ok(
            "AnubisValue::Closure(std::rc::Rc::new(move |__args: Vec<AnubisValue>| -> AnubisValue { match __args.first() { Some(a) => a.len_val(), None => panic!(\"ANUBIS_ARITY: builtin `len` cannot take 0 argument(s)\") } }))"
                .to_string(),
        );
    }
    // Any other stdlib builtin → a closure dispatching on argument count across every arity the
    // builtin accepts (probed through `emit_builtin_call`, e.g. `range` takes 2 or 3).
    {
        let mut arms = String::new();
        for k in 1..=6usize {
            let args: Vec<String> = (0..k)
                .map(|i| format!("__args[{i}usize].clone()"))
                .collect();
            if let Some(Ok(call)) = emit_builtin_call(name, &args) {
                arms.push_str(&format!("{k}usize => {{ {call} }}, "));
            }
        }
        if !arms.is_empty() {
            return Ok(format!(
                "AnubisValue::Closure(std::rc::Rc::new(move |__args: Vec<AnubisValue>| -> AnubisValue {{ match __args.len() {{ {arms} n => panic!(\"ANUBIS_ARITY: builtin `{name}` cannot take {{}} argument(s)\", n) }} }}))"
            ));
        }
    }
    if is_builtin_name(name) {
        // e.g. `push`/`pop`/`insert`/`remove`, which mutate a named binding in place and have no
        // meaningful value-capture form.
        return Err(unsupported_run(format!(
            "builtin `{}` cannot be used as a first-class value",
            name
        )));
    }
    Err(unsupported_run(format!(
        "unknown name `{}` used as a value",
        name
    )))
}

fn safe_run_expr(expr: &Expr, ctx: &EmitCtx) -> Result<String> {
    match expr {
        Expr::Literal(value) => Ok(literal_to_anubis_value(value)),
        Expr::StrLiteral(s) => Ok(format!(
            "anubis_mk_str({}.to_string())",
            rust_string_lit(s)?
        )),
        Expr::Var(name) => var_as_value(name, ctx),
        Expr::Unary { op, expr } => {
            let inner = safe_run_expr(expr, ctx)?;
            match op.as_str() {
                "-" => Ok(format!("anubis_neg({inner})")),
                "!" => Ok(format!("AnubisValue::Bool(!({inner}).as_bool())")),
                "~" => Ok(format!("anubis_bnot({inner})")),
                other => Err(unsupported_run(format!(
                    "unsupported unary operator `{}`",
                    other
                ))),
            }
        }
        Expr::Binary { op, lhs, rhs } => {
            let lhs = safe_run_expr(lhs, ctx)?;
            let rhs = safe_run_expr(rhs, ctx)?;
            match op.as_str() {
                "+" => Ok(format!("anubis_add({lhs}, {rhs})")),
                "-" => Ok(format!("anubis_sub({lhs}, {rhs})")),
                "*" => Ok(format!("anubis_mul({lhs}, {rhs})")),
                "/" => Ok(format!("anubis_div({lhs}, {rhs})")),
                "%" => Ok(format!("anubis_mod({lhs}, {rhs})")),
                "&" => Ok(format!("anubis_band({lhs}, {rhs})")),
                "|" => Ok(format!("anubis_bor({lhs}, {rhs})")),
                "^" => Ok(format!("anubis_bxor({lhs}, {rhs})")),
                "<<" => Ok(format!("anubis_shl({lhs}, {rhs})")),
                ">>" => Ok(format!("anubis_shr({lhs}, {rhs})")),
                "&&" => Ok(format!(
                    "AnubisValue::Bool(({lhs}).as_bool() && ({rhs}).as_bool())"
                )),
                "||" => Ok(format!(
                    "AnubisValue::Bool(({lhs}).as_bool() || ({rhs}).as_bool())"
                )),
                "<" | "<=" | ">" | ">=" | "==" | "!=" => Ok(format!(
                    "anubis_cmp({}, {lhs}, {rhs})",
                    rust_string_lit(op)?
                )),
                other => Err(unsupported_run(format!(
                    "unsupported binary operator `{}`",
                    other
                ))),
            }
        }
        Expr::Call { callee, args } => {
            // User-defined functions take precedence over every builtin name.
            if ctx.fns.contains(callee.as_str()) {
                // Prefer a monomorphized clone when the checker inventory + literal args pin types.
                if let Some(mono) = resolve_mono_call(callee, args, ctx) {
                    if let Some(abi) = &mono.unboxed {
                        // Unboxed ABI: pass native args, wrap result back to AnubisValue.
                        let native_args = args
                            .iter()
                            .zip(abi.params.iter())
                            .map(|(a, p)| emit_mono_arg_unboxed(a, *p, ctx))
                            .collect::<Result<Vec<_>>>()?;
                        let call = format!("{}({})", mono.rust_name, native_args.join(", "));
                        return Ok(abi.ret.to_anubis(&call));
                    }
                    let lowered = args
                        .iter()
                        .map(|a| safe_run_expr(a, ctx))
                        .collect::<Result<Vec<_>>>()?;
                    return Ok(format!("{}({})", mono.rust_name, lowered.join(", ")));
                }
                let lowered = args
                    .iter()
                    .map(|a| safe_run_expr(a, ctx))
                    .collect::<Result<Vec<_>>>()?;
                return Ok(format!(
                    "anb_{}({})",
                    sanitize_ident(callee)?,
                    lowered.join(", ")
                ));
            }
            // A local binding (parameter or let-bound variable) shadows any builtin of the same
            // name — `fn f(map, x) { map(x) }` calls the parameter, not the stdlib `map`.
            if ctx.locals.contains(callee.as_str()) {
                let lowered = args
                    .iter()
                    .map(|a| safe_run_expr(a, ctx))
                    .collect::<Result<Vec<_>>>()?;
                return Ok(format!(
                    "{}.call_closure(vec![{}])",
                    sanitize_ident(callee)?,
                    lowered.join(", ")
                ));
            }
            // Void output builtins in expression position (e.g. a `match` arm body or a
            // block-tail): perform the side effect, then yield Int(0) so the expression
            // still has an AnubisValue type.
            if matches!(callee.as_str(), "print" | "println" | "eprint" | "eprintln") {
                let macro_name = if callee.starts_with('e') {
                    "eprintln"
                } else {
                    "println"
                };
                let mut parts = Vec::new();
                for a in args {
                    parts.push(format!("{}.display_string()", safe_run_expr(a, ctx)?));
                }
                let call = if parts.is_empty() {
                    format!("{}!()", macro_name)
                } else {
                    let fmt = vec!["{}"; parts.len()].join(" ");
                    format!("{}!(\"{}\", {})", macro_name, fmt, parts.join(", "))
                };
                return Ok(format!("{{ {}; AnubisValue::Int(0) }}", call));
            }
            // `return` in expression position (e.g. `match x { _ => return 5 }`) diverges
            // out of the enclosing function; `!` coerces to AnubisValue at the use site.
            if callee == "return" {
                let val = match args.first() {
                    Some(e) => safe_run_expr(e, ctx)?,
                    None => "AnubisValue::Int(0)".to_string(),
                };
                return Ok(format!("return {}", val));
            }
            // `break`/`continue` in expression position (a braceless match arm body inside a
            // loop: `n if n == 11 => break`). Rust `break`/`continue` are `!`-typed expressions,
            // so they coerce to AnubisValue at the use site and bind to the user's enclosing
            // loop (the match desugar deliberately introduces no loop of its own).
            if callee == "break" {
                return Ok("break".to_string());
            }
            if callee == "continue" {
                return Ok("continue".to_string());
            }
            if callee == "len" {
                let a = args
                    .first()
                    .ok_or_else(|| unsupported_run("len requires one argument"))?;
                return Ok(format!("({}).len_val()", safe_run_expr(a, ctx)?));
            }
            // Mutating collection builtins operate on a bound variable by `&mut`.
            if matches!(callee.as_str(), "pop" | "push" | "insert" | "remove") {
                let Some(Expr::Var(name)) = args.first() else {
                    return Err(unsupported_run(format!(
                        "`{}` requires a variable as its first argument",
                        callee
                    )));
                };
                let var = sanitize_ident(name)?;
                let rest = args[1..]
                    .iter()
                    .map(|a| safe_run_expr(a, ctx))
                    .collect::<Result<Vec<_>>>()?;
                return match (callee.as_str(), rest.len()) {
                    ("pop", 0) => Ok(format!("anubis_pop(&mut {})", var)),
                    // Return the CONTAINER, not a placeholder. This arm used to yield
                    // `AnubisValue::Int(0)`, so `let ys = push(xs, 3); len(ys)` silently bound a
                    // non-container: `check` passed and `run` panicked — a check/run divergence
                    // produced by the lowering, not by the program.
                    //
                    // Functional-style use is the natural reading of an expression-position call,
                    // and every sibling here already returns something meaningful (`pop` the
                    // element, `insert`/`remove` the result). `push` was the only one handing back
                    // a placeholder.
                    //
                    // Statement-position `push(xs, v)` is lowered separately and is unaffected;
                    // `AnubisValue` is `Rc`-backed, so returning the container is an O(1) clone.
                    ("push", 1) => Ok(format!(
                        "{{ {}.push_val({}); {}.clone() }}",
                        var, rest[0], var
                    )),
                    ("insert", 2) => Ok(format!(
                        "anubis_insert(&mut {}, {}, {})",
                        var, rest[0], rest[1]
                    )),
                    ("remove", 1) => Ok(format!("anubis_remove(&mut {}, {})", var, rest[0])),
                    _ => Err(unsupported_run(format!("`{}` arity mismatch", callee))),
                };
            }
            if is_proof_input_builtin(callee) {
                if callee == "proof_assert" {
                    if args.len() != 1 {
                        return Err(unsupported_run("proof_assert requires one condition"));
                    }
                    let c = safe_run_expr(&args[0], ctx)?;
                    return Ok(format!("anubis_proof_assert({c})"));
                }
                if callee == "proof_commit_u32" || callee == "proof_commit_bool" {
                    if args.len() != 2 {
                        return Err(unsupported_run("proof_commit_* requires (\"name\", value)"));
                    }
                    let key = match &args[0] {
                        Expr::StrLiteral(s) | Expr::Literal(s) => s.trim_matches('"').to_string(),
                        Expr::Var(s) => s.clone(),
                        _ => {
                            return Err(unsupported_run(
                                "proof_commit_* name must be a string literal",
                            ))
                        }
                    };
                    let val = safe_run_expr(&args[1], ctx)?;
                    let fn_name = if callee == "proof_commit_bool" {
                        "anubis_proof_commit_bool"
                    } else {
                        "anubis_proof_commit_u32"
                    };
                    return Ok(format!("{fn_name}({}, {})", rust_string_lit(&key)?, val));
                }
                let key = match args.first() {
                    Some(Expr::StrLiteral(s)) | Some(Expr::Literal(s)) => {
                        s.trim_matches('"').to_string()
                    }
                    Some(Expr::Var(s)) => s.clone(),
                    _ => {
                        return Err(unsupported_run(
                            "proof_input_* requires a string key literal",
                        ))
                    }
                };
                // Host/native run path: allow simulation via ANUBIS_PROOF_INPUT_JSON env if present;
                // otherwise fail closed (these builtins are for prove guests).
                return match callee.as_str() {
                    "proof_input_u32" | "proof_input_u64" => Ok(format!(
                        "anubis_proof_input_u32_val({})",
                        rust_string_lit(&key)?
                    )),
                    "proof_input_bool" => Ok(format!(
                        "anubis_proof_input_bool_val({})",
                        rust_string_lit(&key)?
                    )),
                    _ => Err(unsupported_run(format!(
                        "unknown proof input builtin `{callee}`"
                    ))),
                };
            }
            if is_poc_kit_builtin(callee) {
                // Packing/cyclic/flat always lower. `target_run` without `--allow-research`
                // lowers to a runtime panic so `std.pwn` modules that *define* run_local still
                // compile when only pure helpers are called; invoking run_local still fails closed.
                let mut lowered = Vec::new();
                for arg in args {
                    lowered.push(safe_run_expr(arg, ctx)?);
                }
                return match callee.as_str() {
                    "p8" if lowered.len() == 1 => Ok(format!("anubis_p8({})", lowered[0])),
                    "p16" if lowered.len() == 1 => Ok(format!("anubis_p16({})", lowered[0])),
                    "p32" if lowered.len() == 1 => Ok(format!("anubis_p32({})", lowered[0])),
                    "p64" if lowered.len() == 1 => Ok(format!("anubis_p64({})", lowered[0])),
                    "cyclic" if lowered.len() == 1 => Ok(format!("anubis_cyclic({})", lowered[0])),
                    "flat" if lowered.len() == 1 => Ok(format!(
                        "anubis_mk_list(anubis_to_bytes(&{}).into_iter().map(|b| AnubisValue::Int(b as i64)).collect())",
                        lowered[0]
                    )),
                    "target_run" if lowered.len() == 2 => {
                        if ctx.allow_research {
                            Ok(format!(
                                "anubis_target_run({}, {})",
                                lowered[0], lowered[1]
                            ))
                        } else {
                            // Must type as AnubisValue: std.pwn wrappers bind `let r = target_run(...)`
                            // even when only pure helpers run. panic! alone is untyped.
                            Ok(format!(
                                "{{ let _p = {}; let _q = {}; panic!(\"ANUBIS_POC_REQUIRES_ALLOW_RESEARCH: target_run requires `anubis run --allow-research`\"); #[allow(unreachable_code)] AnubisValue::Int(0) }}",
                                lowered[0], lowered[1]
                            ))
                        }
                    }
                    _ => Err(unsupported_run(format!(
                        "PoC kit builtin `{callee}` arity mismatch"
                    ))),
                };
            }
            if is_non_run_builtin(callee) {
                if matches!(callee.as_str(), "taint_source" | "declassify" | "sink") {
                    // Analysis labels have no privileged runtime effect. The shared verifier has
                    // already approved the flow, so preserve/evaluate the carried value faithfully.
                    if callee == "taint_source" {
                        let a = args.first().map(|e| safe_run_expr(e, ctx)).transpose()?;
                        return Ok(
                            a.unwrap_or_else(|| "anubis_mk_str(\"tainted\".to_string())".into())
                        );
                    }
                    if let Some(first) = args.first() {
                        return safe_run_expr(first, ctx);
                    }
                    return Ok("AnubisValue::Int(0)".into());
                }
                return Err(unsupported_run(format!(
                    "builtin `{}` is a proof/analysis construct, not available in `run`",
                    callee
                )));
            }
            let mut lowered = Vec::new();
            for arg in args {
                lowered.push(safe_run_expr(arg, ctx)?);
            }
            // Not a user function (checked first) → try a stdlib builtin.
            if let Some(result) = emit_builtin_call(callee, &lowered) {
                return result;
            }
            // Otherwise it is the application of a closure-valued variable: `f(x)`.
            Ok(format!(
                "{}.call_closure(vec![{}])",
                sanitize_ident(callee)?,
                lowered.join(", ")
            ))
        }
        // Application of an arbitrary callee expression: `obj.f(x)`, `arr[i](x)`, `f(a)(b)`.
        Expr::CallExpr { callee, args } => {
            // Method call `recv.method(args)`: `method` is defined in some `impl` block. Dispatch
            // on the receiver's runtime struct/enum type; `recv` becomes the method's `self`.
            if let Expr::FieldAccess { base, field, .. } = callee.as_ref() {
                if let Some(types) = ctx.methods.get(field) {
                    let recv = safe_run_expr(base, ctx)?;
                    let mut lowered = Vec::new();
                    for arg in args {
                        lowered.push(safe_run_expr(arg, ctx)?);
                    }
                    let mut arms = String::new();
                    for (ty, arity) in types {
                        // The method wants `arity` params including self, i.e. `arity - 1` after
                        // self. Take that many actual args, padding with 0 (each arm is type-checked
                        // by Rust even though only the matching one runs, so counts must be exact).
                        let want = arity.saturating_sub(1);
                        let mut call_args = vec!["__anb_recv".to_string()];
                        for k in 0..want {
                            call_args.push(
                                lowered
                                    .get(k)
                                    .cloned()
                                    .unwrap_or_else(|| "AnubisValue::Int(0)".to_string()),
                            );
                        }
                        arms.push_str(&format!(
                            "{} => {}({}), ",
                            rust_string_lit(ty)?,
                            fn_rust_name(field, Some(ty))?,
                            call_args.join(", ")
                        ));
                    }
                    // Fallback: the receiver's type has no such method — treat `obj.field(args)` as
                    // reading the field (which may hold a closure) and calling it. This keeps
                    // `obj.f()` on a closure-valued field working even when some other type defines
                    // a method named `f`.
                    let closure_fallback = format!(
                        "__anb_recv.field_get({}).try_call_closure(vec![{}])",
                        rust_string_lit(field)?,
                        lowered.join(", ")
                    );
                    return Ok(format!(
                        "{{ let __anb_recv = {recv}; \
                         let __anb_ty = match &__anb_recv {{ \
                             AnubisValue::Struct {{ ty, .. }} | AnubisValue::Enum {{ ty, .. }} => ty.clone(), \
                             _ => String::new() }}; \
                         match __anb_ty.as_str() {{ {arms} _ => {closure_fallback} }} }}"
                    ));
                }
            }
            let callee_src = safe_run_expr(callee, ctx)?;
            let mut lowered = Vec::new();
            for arg in args {
                lowered.push(safe_run_expr(arg, ctx)?);
            }
            Ok(format!(
                "({}).call_closure(vec![{}])",
                callee_src,
                lowered.join(", ")
            ))
        }
        Expr::ArrayLiteral { elements } => {
            let mut lowered = Vec::new();
            for el in elements {
                lowered.push(safe_run_expr(el, ctx)?);
            }
            Ok(format!("anubis_mk_list(vec![{}])", lowered.join(", ")))
        }
        Expr::EnumConstruct {
            enum_name,
            variant,
            fields,
            field_names,
            ..
        } => {
            let mut fs = Vec::new();
            for f in fields {
                fs.push(safe_run_expr(f, ctx)?);
            }
            let names: Vec<String> = field_names
                .iter()
                .map(|n| {
                    format!(
                        "{}.to_string()",
                        rust_string_lit(n).unwrap_or_else(|_| "\"\"".into())
                    )
                })
                .collect();
            Ok(format!(
                "AnubisValue::Enum {{ ty: {}.to_string(), tag: {}.to_string(), fields: vec![{}], field_names: vec![{}] }}",
                rust_string_lit(enum_name)?,
                rust_string_lit(variant)?,
                fs.join(", "),
                names.join(", ")
            ))
        }
        Expr::Match {
            scrutinee, arms, ..
        } => lower_match_expr(scrutinee, arms, ctx),
        Expr::If {
            cond, then, else_, ..
        } => {
            let c = safe_run_expr(cond, ctx)?;
            let t = safe_run_expr(then, ctx)?;
            let e = safe_run_expr(else_, ctx)?;
            Ok(format!("if ({c}).as_bool() {{ {t} }} else {{ {e} }}"))
        }
        // `if let PATTERN = scrutinee { then } else { else_ }` as a value: bind the scrutinee once,
        // yield the matching branch (pattern bindings scoped to `then`).
        Expr::IfLet {
            pattern,
            scrutinee,
            then,
            else_,
            ..
        } => {
            let scr = safe_run_expr(scrutinee, ctx)?;
            let tmp = format!("__anb_il{}", next_temp_id());
            let (test, binds) = pattern_test_and_binds(pattern, &tmp)?;
            let then_src = safe_run_expr(then, ctx)?;
            let else_src = safe_run_expr(else_, ctx)?;
            Ok(format!(
                "{{ let {tmp} = {scr}; if {test} {{ {binds} {then_src} }} else {{ {else_src} }} }}"
            ))
        }
        // Error propagation `expr?`: unwrap `Ok(v)`/`Some(v)`, propagate `Err`/`None` by returning
        // it from the enclosing function; any other value passes through unchanged.
        Expr::Try(inner) => {
            let v = safe_run_expr(inner, ctx)?;
            // Only the built-in Option/Result enums participate: a user enum that merely happens
            // to name a variant `Ok`/`None` is a distinct type and passes through unchanged.
            Ok(format!(
                "{{ let __anb_q = {v}; match &__anb_q {{ \
                    AnubisValue::Enum {{ ty, tag, fields, .. }} \
                        if (ty == \"Result\" && tag == \"Ok\") || (ty == \"Option\" && tag == \"Some\") => \
                        fields.first().cloned().unwrap_or(AnubisValue::Int(0)), \
                    AnubisValue::Enum {{ ty, tag, .. }} \
                        if (ty == \"Result\" && tag == \"Err\") || (ty == \"Option\" && tag == \"None\") => \
                        return __anb_q, \
                    AnubisValue::Enum {{ .. }} => __anb_q, \
                    _ => panic!(\"ANUBIS_TRY_ON_NON_OPTION_RESULT: `?` requires an Option, Result, or enum value\") \
                }} }}"
            ))
        }
        Expr::MapLiteral { entries, .. } => {
            let mut pairs = Vec::new();
            for (k, v) in entries {
                let ks = safe_run_expr(k, ctx)?;
                let vs = safe_run_expr(v, ctx)?;
                pairs.push(format!("(({ks}).display_string(), {vs})"));
            }
            Ok(format!("anubis_map_lit(vec![{}])", pairs.join(", ")))
        }
        // Block expression: run the statements, then yield the tail value (or Int(0)).
        Expr::Block { stmts, tail } => {
            let mut body = String::new();
            let tail_src = match tail {
                Some(t) => {
                    for s in stmts {
                        emit_safe_run_stmt(s, 0, &mut body, ctx)?;
                    }
                    safe_run_expr(t, ctx)?
                }
                // No explicit tail: a trailing bare expression or `if/else` statement is still the
                // block's value (so `{ if c { return x } y }` yields y, and `{ if c { a } else { b } }`
                // yields a/b), matching function-body implicit-return semantics.
                None => {
                    let (head, tail_expr) = split_tail_expr(stmts);
                    for s in &head {
                        emit_safe_run_stmt(s, 0, &mut body, ctx)?;
                    }
                    match tail_expr {
                        Some(e) => safe_run_expr(&e, ctx)?,
                        None => "AnubisValue::Int(0)".to_string(),
                    }
                }
            };
            Ok(format!("{{ {} {} }}", body, tail_src))
        }
        // Lambda: capture free variables by clone, bind positional params, then run the body.
        Expr::Lambda { params, body } => {
            let mut bound = std::collections::BTreeSet::new();
            for p in params {
                bound.insert(p.clone());
            }
            let mut vars = std::collections::BTreeSet::new();
            let mut callees = std::collections::BTreeSet::new();
            collect_free_expr(body, &bound, &mut vars, &mut callees);
            // Capture every value-use (always a local, even if its name shadows a builtin), plus
            // callee-uses that name a closure-valued local (a local binding, or a name that is
            // neither a user function nor a builtin).
            let mut to_capture = vars;
            for c in callees {
                if ctx.locals.contains(&c) || (!ctx.fns.contains(&c) && !is_builtin_name(&c)) {
                    to_capture.insert(c);
                }
            }
            // Outer capture block: snapshot each free var by clone, then `move` it into the closure.
            let mut captures = String::new();
            // Inside the closure, re-clone each captured var into a fresh `mut` local per call, so a
            // body that mutates a captured binding compiles (value-capture: the mutation is on the
            // per-call copy) while the closure stays `Fn` (it only reads the captured snapshot).
            let mut reclone = String::new();
            for v in &to_capture {
                let id = sanitize_ident(v)?;
                captures.push_str(&format!("let {id} = {id}.clone(); "));
                reclone.push_str(&format!("let mut {id} = {id}.clone(); "));
            }
            let mut binds = String::new();
            for (i, p) in params.iter().enumerate() {
                let id = sanitize_ident(p)?;
                binds.push_str(&format!(
                    "let {}{} = __args.get({}usize).cloned().unwrap_or(AnubisValue::Int(0)); ",
                    mut_prefix(&id),
                    id,
                    i
                ));
            }
            let body_src = safe_run_expr(body, ctx)?;
            let closure = format!(
                "AnubisValue::Closure(std::rc::Rc::new(move |__args: Vec<AnubisValue>| -> AnubisValue {{ {reclone}{binds}{body_src} }}))"
            );
            // Only introduce a capture block when there is something to capture, so lambdas passed
            // directly as arguments don't trigger `unused_braces`.
            if captures.is_empty() {
                Ok(closure)
            } else {
                Ok(format!("{{ {captures}{closure} }}"))
            }
        }
        Expr::Index { base, index } => {
            let idx = safe_run_expr(index, ctx)?;
            // Fast path: indexing a local binding borrows it and clones only the element. The
            // generic path routes the base through var_as_value, which clones the WHOLE collection
            // (`(a.clone()).index_get(i)`) — turning `a[i]` inside a loop into O(n^2). index_get
            // takes `&self`, so `a.index_get(i)` reads in place with the same value semantics.
            if let Expr::Var(name) = base.as_ref() {
                if ctx.locals.contains(name) {
                    return Ok(format!("{}.index_get({})", sanitize_ident(name)?, idx));
                }
            }
            Ok(format!(
                "({}).index_get({})",
                safe_run_expr(base, ctx)?,
                idx
            ))
        }
        // `expr as T` — numeric conversions truncate/wrap; pointer casts pass through unchanged.
        Expr::Cast { expr, ty } => {
            let inner = safe_run_expr(expr, ctx)?;
            let t = ty.to_ascii_lowercase();
            if t.contains('*') || t.contains("ptr") {
                Ok(inner)
            } else if matches!(t.as_str(), "f32" | "f64" | "float" | "double") {
                Ok(format!("anubis_float({})", inner))
            } else {
                let bits: u32 = match t.as_str() {
                    "u8" | "i8" => 8,
                    "u16" | "i16" => 16,
                    "u32" | "i32" => 32,
                    "u64" | "i64" | "u128" | "i128" | "usize" | "isize" | "int" | "integer" => 64,
                    _ => 0,
                };
                // Signed targets (i8/i16/i32) sign-extend the narrowed value; unsigned keep it.
                let signed = t.starts_with('i');
                if bits == 0 {
                    Ok(inner) // unrecognized target type: leave the value unchanged
                } else {
                    Ok(format!("anubis_cast_int({}, {}, {})", inner, bits, signed))
                }
            }
        }
        // Nominal struct construction: `Name { f: e, ... }`.
        Expr::StructLiteral { name, fields, .. } => {
            let mut fs = Vec::new();
            let field_tys = ctx.struct_field_types.get(name);
            for (fname, fexpr) in fields {
                let val = safe_run_expr(fexpr, ctx)?;
                // Enforce the declared NUMERIC kind at the struct-construction boundary (task #34 dual):
                // a float field coerces an Int value to Float (so the solver's QF_FP model is sound), an
                // integer field fail-closes on a non-Int (so its QF_BV model is sound). Comprehensive
                // across ALL value shapes — literal, var, `Call`, `FieldAccess`, `Index` — because it acts
                // on the runtime VALUE, not the checker's (partial) static type. Non-numeric fields are
                // left untouched. `anubis_require_int_ret` is the identity on an Int, so this is inert on
                // the self-host (no float fields; every int field already receives an Int).
                let declared = field_tys.and_then(|m| m.get(fname));
                let wrapped = match declared {
                    Some(t) if crate::middle::ty::is_integer(t) => {
                        format!(
                            "anubis_field_require_int({}, {})",
                            val,
                            rust_string_lit(fname)?
                        )
                    }
                    Some(t) if crate::middle::ty::is_float(t) => {
                        format!(
                            "anubis_field_coerce_float({}, {})",
                            val,
                            rust_string_lit(fname)?
                        )
                    }
                    _ => val,
                };
                fs.push(format!(
                    "({}.to_string(), {})",
                    rust_string_lit(fname)?,
                    wrapped
                ));
            }
            Ok(format!(
                "AnubisValue::Struct {{ ty: {}.to_string(), fields: vec![{}] }}",
                rust_string_lit(name)?,
                fs.join(", ")
            ))
        }
        // Field read: struct / struct-enum-variant / map field.
        Expr::FieldAccess { base, field, .. } => {
            let fname = rust_string_lit(field)?;
            // Fast path: read a field off a local by borrow instead of cloning the whole struct
            // (same reasoning as indexing — field_get takes `&self` and clones only the field).
            if let Expr::Var(name) = base.as_ref() {
                if ctx.locals.contains(name) {
                    return Ok(format!("{}.field_get({})", sanitize_ident(name)?, fname));
                }
            }
            Ok(format!(
                "({}).field_get({})",
                safe_run_expr(base, ctx)?,
                fname
            ))
        }
        // Taint and declassification are analysis labels, not privileged effects. Once the shared
        // execution checker validates the flow, lowering preserves the runtime value faithfully.
        Expr::TaintSource { label } => Ok(format!(
            "anubis_mk_str({}.to_string())",
            rust_string_lit(label)?
        )),
        Expr::Tainted { inner, .. } | Expr::Declassify { inner, .. } => safe_run_expr(inner, ctx),
        // Runtime assertion: `assert(cond)` panics (fail-closed) when the condition is false.
        Expr::Assert(inner) => Ok(format!("anubis_assert({})", safe_run_expr(inner, ctx)?)),
        // `assume(cond)` is trusted by the solver, so the runtime enforces it (fail-closed) — an
        // assumption that is false at runtime would otherwise silently certify a violated contract.
        Expr::Assume(inner) => Ok(format!("anubis_assume({})", safe_run_expr(inner, ctx)?)),
        Expr::Symbolic { .. }
        | Expr::UnifiedBuffer { .. }
        | Expr::RawPtr { .. } => Err(unsupported_run(
            "verification-only or privileged construct (symbolic / unified-buffer / raw pointer) \
             has no faithful ordinary native value; use proof inputs, check/prove, or the explicit \
             research isolation lane as appropriate"
                .to_string(),
        )),
        // A placeholder the parser emits when it could not build a real expression — surface the
        // captured detail so the message is actionable instead of an opaque discriminant.
        Expr::Other(detail) => Err(unsupported_run(format!(
            "could not lower expression (`{detail}`) — this syntax is not supported in `anubis run`"
        ))),
    }
}

fn lower_match_expr(
    scrutinee: &Expr,
    arms: &[crate::frontend::MatchArm],
    ctx: &EmitCtx,
) -> Result<String> {
    let scr = safe_run_expr(scrutinee, ctx)?;
    // Bind the scrutinee once, then walk the arms in order using a linear chain of plain `if`s
    // gated by a `__done` flag. The first matching arm assigns its body to `__r` and sets the flag;
    // later arms are skipped. Guard failures leave the flag unset and fall through to the next arm.
    //
    // This desugar deliberately introduces NO Rust `loop`, labeled block, or closure between an
    // arm body and the enclosing code. That transparency is essential: a `break`/`continue` written
    // inside an arm body must bind to the USER's enclosing `for`/`while` loop. The arm body is
    // emitted as the right-hand side of `__r = <body>;`, so if the body diverges (`break`,
    // `continue`, `return`), it diverges out of the match to the user's loop or function — exactly
    // as if the control-flow statement had been written directly in the loop body. (A `loop` or
    // labeled block would instead capture the `break`, and Rust rejects an unlabeled `break` that
    // tries to escape a labeled block: E0695.) Names are suffixed with a unique id so nested
    // matches never collide.
    let id = next_temp_id();
    let m = format!("__anb_m{id}");
    let r = format!("__anb_r{id}");
    let done = format!("__anb_done{id}");
    let mut out =
        format!("{{ let {m} = {scr}; let mut {r} = AnubisValue::Int(0); let mut {done} = false; ");
    for arm in arms {
        // A top-level or-pattern arm `A | B => body` desugars to one sub-arm per alternative,
        // so alternatives may bind (each is matched and bound with its own sub-pattern).
        let alts: Vec<&crate::frontend::Pattern> = match &arm.pattern {
            crate::frontend::Pattern::Or(ps) => ps.iter().collect(),
            p => vec![p],
        };
        for pat in alts {
            let (cond, binds) = pattern_test_and_binds(pat, &m)?;
            out.push_str(&format!("if !{done} {{ if {cond} {{ "));
            out.push_str(&binds);
            let body = safe_run_expr(&arm.body, ctx)?;
            match &arm.guard {
                Some(guard) => {
                    let gs = safe_run_expr(guard, ctx)?;
                    out.push_str(&format!(
                        "if ({gs}).as_bool() {{ {r} = ({body}); {done} = true; }} "
                    ));
                }
                None => {
                    out.push_str(&format!("{r} = ({body}); {done} = true; "));
                }
            }
            out.push_str("} } ");
        }
    }
    // Fail closed: a match where no arm matched has no defensible value. Enum scrutinees are
    // already rejected at compile time by exhaustiveness checking; non-enum scrutinees (list/
    // int/string shapes) can't be statically enumerated, so they trap at runtime instead of
    // silently fabricating a value.
    out.push_str(&format!(
        "if !{done} {{ panic!(\"ANUBIS_MATCH_UNMATCHED: no match arm matched value `{{}}` (add a `_` arm)\", ({m}).display_string()); }} "
    ));
    // The block's tail expression is the selected arm's value.
    out.push_str(&format!("{r} }}"));
    Ok(out)
}

/// Lower a single pattern to `(test_expr, binding_statements)` against a scrutinee
/// variable already in scope. `test_expr` is a `bool` Rust expression; the binding
/// statements are emitted inside the arm body's block after the test passes.
fn pattern_test_and_binds(pat: &crate::frontend::Pattern, scr: &str) -> Result<(String, String)> {
    use crate::frontend::Pattern;
    match pat {
        Pattern::Wildcard => Ok(("true".to_string(), String::new())),
        Pattern::Binding(name) => {
            let bn = sanitize_ident(name)?;
            Ok((
                "true".to_string(),
                format!("let mut {bn} = {scr}.clone(); "),
            ))
        }
        Pattern::Struct { name, fields } => {
            // Access a named field: its value if present on a struct, else the default 0.
            let field_expr = |fname: &str| -> Result<String> {
                Ok(format!(
                    "(match &{scr} {{ AnubisValue::Struct {{ fields, .. }} => \
                        fields.iter().find(|(__n, _)| __n == &{}).map(|(_, __v)| __v.clone()).unwrap_or(AnubisValue::Int(0)), \
                        _ => AnubisValue::Int(0) }})",
                    rust_string_lit(fname)?
                ))
            };
            let mut cond = format!(
                "matches!(&{scr}, AnubisValue::Struct {{ ty, .. }} if ty == {})",
                rust_string_lit(name)?
            );
            // Each field's sub-pattern imposes a test on that field's value.
            for (fname, sub) in fields {
                let (sub_test, _) = pattern_test_and_binds(sub, &field_expr(fname)?)?;
                if sub_test != "true" {
                    cond.push_str(&format!(" && ({sub_test})"));
                }
            }
            // Bindings extract each field into a path-unique temp, then apply the sub-pattern's binds.
            let mut binds = String::new();
            for (fname, sub) in fields {
                if sub.bound_names().is_empty() {
                    continue;
                }
                let temp = format!("{scr}_f_{}", sanitize_ident(fname)?);
                binds.push_str(&format!("let {temp} = {}; ", field_expr(fname)?));
                let (_, sub_binds) = pattern_test_and_binds(sub, &temp)?;
                binds.push_str(&sub_binds);
            }
            Ok((cond, binds))
        }
        Pattern::Literal(text) => {
            // Type-exact literal matching: a literal pattern matches only a value of the same
            // kind. Numeric literals match Int/Float scrutinees by numeric value (int and float
            // interchangeable when equal), but a bool never matches a number and a string never
            // matches a number. (The general `==` operator coerces across types; pattern
            // matching deliberately does not, so `match 5 { "5" => .. }` and
            // `match 1 { true => .. }` do NOT match.)
            let test = if text == "true" || text == "false" {
                format!("matches!(&{scr}, AnubisValue::Bool(__b) if *__b == {text})")
            } else if let Ok(i) = text.parse::<i64>() {
                format!(
                    "(match &{scr} {{ AnubisValue::Int(__x) => *__x == {i}i64, AnubisValue::Float(__x) => *__x == {i}i64 as f64, _ => false }})"
                )
            } else if let Ok(f) = text.parse::<f64>() {
                format!(
                    "(match &{scr} {{ AnubisValue::Float(__x) => *__x == {f}f64, AnubisValue::Int(__x) => (*__x as f64) == {f}f64, _ => false }})"
                )
            } else {
                // Not numeric or boolean — treat as a string value.
                format!(
                    "matches!(&{scr}, AnubisValue::Str(__s) if __s.as_str() == {})",
                    rust_string_lit(text)?
                )
            };
            Ok((test, String::new()))
        }
        Pattern::StrLiteral(s) => {
            // String/char literal patterns match only string values, exactly.
            Ok((
                format!(
                    "matches!(&{scr}, AnubisValue::Str(__s) if __s.as_str() == {})",
                    rust_string_lit(s)?
                ),
                String::new(),
            ))
        }
        Pattern::Or(pats) => {
            let mut conds = Vec::new();
            for p in pats {
                let (c, b) = pattern_test_and_binds(p, scr)?;
                if !b.trim().is_empty() {
                    return Err(unsupported_run(
                        "or-patterns may not bind variables (use literals, `_`, or unit variants)",
                    ));
                }
                conds.push(format!("({c})"));
            }
            if conds.is_empty() {
                Ok(("false".to_string(), String::new()))
            } else {
                Ok((conds.join(" || "), String::new()))
            }
        }
        Pattern::List(subs) => {
            // A list/tuple pattern matches a list of exactly this length, matching each
            // element against its sub-pattern. Structural test short-circuits before any
            // element access; bindings extract elements into path-unique temps so nested
            // list patterns never collide.
            let n = subs.len();
            let mut cond =
                format!("matches!(&{scr}, AnubisValue::List(__anb_lv) if __anb_lv.len() == {n})");
            for (i, sub) in subs.iter().enumerate() {
                let elem = format!("{scr}.list_elem({i}i64)");
                let (sub_test, _) = pattern_test_and_binds(sub, &elem)?;
                if sub_test != "true" {
                    cond.push_str(&format!(" && ({sub_test})"));
                }
            }
            let mut binds = String::new();
            for (i, sub) in subs.iter().enumerate() {
                if sub.bound_names().is_empty() {
                    continue; // wildcard / literal element binds nothing
                }
                let temp = format!("{scr}_el{i}");
                binds.push_str(&format!("let {temp} = {scr}.list_elem({i}i64); "));
                let (_, sub_binds) = pattern_test_and_binds(sub, &temp)?;
                binds.push_str(&sub_binds);
            }
            Ok((cond, binds))
        }
        Pattern::EnumVariant {
            enum_name,
            variant,
            bindings,
            named_bindings,
        } => {
            // Positional payload field i, or the default 0 if absent.
            let payload_expr = |i: usize| -> String {
                format!(
                    "(match &{scr} {{ AnubisValue::Enum {{ fields, .. }} if fields.len() > {i} => fields[{i}].clone(), _ => AnubisValue::Int(0) }})"
                )
            };
            let mut cond = format!(
                "matches!(&{scr}, AnubisValue::Enum {{ ty, tag, .. }} if ty == {} && tag == {})",
                rust_string_lit(enum_name)?,
                rust_string_lit(variant)?
            );
            // Each positional arg is a sub-pattern: it may bind (Some(x)) or test (Some(0), Some(Point{..})).
            for (i, sub) in bindings.iter().enumerate() {
                let (sub_test, _) = pattern_test_and_binds(sub, &payload_expr(i))?;
                if sub_test != "true" {
                    cond.push_str(&format!(" && ({sub_test})"));
                }
            }
            let mut binds = String::new();
            for (i, sub) in bindings.iter().enumerate() {
                if sub.bound_names().is_empty() {
                    continue;
                }
                let temp = format!("{scr}_p{i}");
                binds.push_str(&format!("let {temp} = {}; ", payload_expr(i)));
                let (_, sub_binds) = pattern_test_and_binds(sub, &temp)?;
                binds.push_str(&sub_binds);
            }
            // A struct-variant field's value by name, or the default 0.
            let named_field_expr = |fname: &str| -> Result<String> {
                Ok(format!(
                    "(match &{scr} {{ AnubisValue::Enum {{ fields, field_names, .. }} => {{ \
                        let mut __v = AnubisValue::Int(0); \
                        for (__i, __n) in field_names.iter().enumerate() {{ \
                            if __n == &{} {{ if let Some(__f) = fields.get(__i) {{ __v = __f.clone(); }} break; }} \
                        }} \
                        __v \
                    }}, _ => AnubisValue::Int(0) }})",
                    rust_string_lit(fname)?
                ))
            };
            // Each named field's sub-pattern imposes a test and may bind.
            for (fname, sub) in named_bindings {
                let (sub_test, _) = pattern_test_and_binds(sub, &named_field_expr(fname)?)?;
                if sub_test != "true" {
                    cond.push_str(&format!(" && ({sub_test})"));
                }
                if !sub.bound_names().is_empty() {
                    let temp = format!("{scr}_nf_{}", sanitize_ident(fname)?);
                    binds.push_str(&format!("let {temp} = {}; ", named_field_expr(fname)?));
                    let (_, sub_binds) = pattern_test_and_binds(sub, &temp)?;
                    binds.push_str(&sub_binds);
                }
            }
            Ok((cond, binds))
        }
    }
}

/// Lower a numeric/boolean literal's text to an `AnubisValue` constructor.
/// (String literals are handled separately via `Expr::StrLiteral`.)
fn literal_to_anubis_value(value: &str) -> String {
    if value == "true" || value == "false" {
        format!("AnubisValue::Bool({value})")
    } else if value.parse::<i64>().is_ok() {
        format!("AnubisValue::Int({value})")
    } else if let Ok(u) = value.parse::<u64>() {
        // Magnitudes in (i64::MAX, u64::MAX] — e.g. 2^63, the magnitude of i64::MIN, or a full-width
        // hex literal — reinterpret their bit pattern as i64 rather than losing precision to f64.
        format!("AnubisValue::Int({u}u64 as i64)")
    } else if let Ok(f) = value.parse::<f64>() {
        format!("AnubisValue::Float({}f64)", f)
    } else {
        format!(
            "anubis_mk_str({}.to_string())",
            rust_string_lit(value).expect("string literal serialization cannot fail")
        )
    }
}

fn sanitize_ident(name: &str) -> Result<String> {
    let valid = !name.is_empty()
        && name.chars().all(|c| c == '_' || c.is_ascii_alphanumeric())
        && !name.chars().next().is_some_and(|c| c.is_ascii_digit());
    if !valid {
        return Err(unsupported_run(format!("invalid identifier `{}`", name)));
    }
    // A legal Anubis identifier may be a Rust keyword (`type`, `move`, `ref`, `impl`, `self`, …).
    // Escape those with a reserved suffix so the emitted Rust is well-formed. The transform is
    // deterministic, so a binding site and its uses stay consistent, and it composes with the
    // `anb_` function-name prefix (`fn type` → `anb_type__anbkw`). A user identifier can never
    // end in `__anbkw` unless they wrote it literally, so this is collision-free in practice.
    if is_rust_keyword(name) {
        Ok(format!("{name}__anbkw"))
    } else {
        Ok(name.to_string())
    }
}

/// The `mut` prefix for a binding site. Every real Anubis binding is `mut` (assignment is
/// pervasive), but Rust forbids `mut _` — the wildcard is never mutable — so a `_` binding
/// (`let _ = e;`, `for _ in …`, `|_| …`, `fn f(_)`) must be emitted bare.
fn mut_prefix(sanitized_ident: &str) -> &'static str {
    if sanitized_ident == "_" {
        ""
    } else {
        "mut "
    }
}

/// A process-wide counter for generating unique codegen temporaries (e.g. destructuring
/// scrutinees). Order is deterministic within a single compilation, so output is stable.
fn next_temp_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// Rust 2021 keywords/reserved words, plus prelude items in the value namespace that a `let`
/// binding cannot shadow (`None`/`Some`/`Ok`/`Err` are prelude variants — `let mut None = …`
/// is rejected by rustc as E0530). Any of these, used as an Anubis identifier, must be escaped.
fn is_rust_keyword(name: &str) -> bool {
    matches!(
        name,
        "as" | "break" | "const" | "continue" | "crate" | "dyn" | "else" | "enum" | "extern"
            | "false" | "fn" | "for" | "if" | "impl" | "in" | "let" | "loop" | "match" | "mod"
            | "move" | "mut" | "pub" | "ref" | "return" | "self" | "Self" | "static" | "struct"
            | "super" | "trait" | "true" | "type" | "unsafe" | "use" | "where" | "while"
            | "async" | "await" | "gen" | "abstract" | "become" | "box" | "do" | "final"
            | "macro" | "override" | "priv" | "typeof" | "unsized" | "virtual" | "yield"
            | "try" | "union"
            // Prelude constructors/variants (value namespace; cannot be shadowed by `let`).
            | "None" | "Some" | "Ok" | "Err"
    )
}

fn unsupported_run(detail: impl Into<String>) -> anyhow::Error {
    anyhow!("ANUBIS_UNSUPPORTED_NATIVE_LOWERING: {}", detail.into())
}

fn rust_string_lit(value: &str) -> Result<String> {
    // Emit a valid *Rust* string literal. Rust's Debug for &str uses escape_debug, which
    // escapes control characters in Rust's own unicode-escape form, handles quotes and
    // backslashes, and leaves printable Unicode intact -- exactly the Rust literal grammar.
    // (serde_json emits JSON-style control-char escapes, which Rust's lexer rejects.)
    Ok(format!("{:?}", value))
}

// ---------------------------------------------------------------------------
// Test / embedder harness: transpile → rustc → execute.
// Kept in the compiler crate so the whole language is testable without risc0.
// ---------------------------------------------------------------------------

fn anubis_unique_suffix() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("{}-{}-{}", std::process::id(), nanos, n)
}

/// Result of running a child process under a wall-clock budget.
pub struct CappedRun {
    /// Captured exit status plus stdout/stderr up to (and including) the kill.
    pub output: std::process::Output,
    /// True when the watchdog fired: the child overran its budget and was killed.
    pub timed_out: bool,
}

/// Parse a raw `ANUBIS_RUN_TIMEOUT_SECS` value into a wall-clock budget.
///
/// Absent or unparseable → the 3600s work-class default. `0` → `None`
/// (unbounded opt-out). Kept pure so the policy is unit-testable without
/// mutating process-global environment state.
fn parse_run_timeout_secs(raw: Option<&str>) -> Option<std::time::Duration> {
    const DEFAULT_SECS: u64 = 3600;
    let secs = raw
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(DEFAULT_SECS);
    if secs == 0 {
        None
    } else {
        Some(std::time::Duration::from_secs(secs))
    }
}

/// Wall-clock budget for an executed Anubis program.
///
/// Defaults to 3600s (the operator work-class-timeout invariant) so a runaway
/// program fails closed within an hour instead of leaking a CPU-pinning orphan
/// forever. Override with `ANUBIS_RUN_TIMEOUT_SECS`: any positive integer, or
/// `0` to disable the cap (e.g. a long-lived interactive session).
pub fn resolved_run_timeout() -> Option<std::time::Duration> {
    parse_run_timeout_secs(std::env::var("ANUBIS_RUN_TIMEOUT_SECS").ok().as_deref())
}

/// Run a prepared child `Command` to completion under an optional wall-clock
/// budget, capturing stdout and stderr.
///
/// `timeout == None` waits forever (the historical behavior, for callers that
/// deliberately opt out). With a budget, a watchdog polls for exit and, once
/// the deadline passes, SIGKILLs and reaps the child — so `anubis run` can
/// never leave a runaway native binary that outlives its parent and spins a
/// core indefinitely. The caller's stdin choice is preserved; only stdout and
/// stderr are forced to pipes so they can be drained without a pipe-buffer
/// deadlock.
///
/// Limitation: only the direct child is signalled. An Anubis program that
/// itself spawns a long-lived grandchild is out of scope here (the research
/// `target_run` builtin already caps its own probes); the leaks this closes
/// are leaf compute binaries.
pub fn run_child_capped(
    mut cmd: std::process::Command,
    timeout: Option<std::time::Duration>,
) -> std::io::Result<CappedRun> {
    use std::io::Read;
    use std::process::Stdio;
    use std::time::Instant;

    let Some(budget) = timeout else {
        // Unbounded opt-out: keep the simple blocking capture.
        return Ok(CappedRun {
            output: cmd.output()?,
            timed_out: false,
        });
    };

    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;

    // Drain both pipes on dedicated threads so a chatty child cannot wedge by
    // filling a pipe buffer while the main thread polls for exit.
    let mut out_pipe = child.stdout.take().expect("stdout was set to piped");
    let mut err_pipe = child.stderr.take().expect("stderr was set to piped");
    let out_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = out_pipe.read_to_end(&mut buf);
        buf
    });
    let err_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = err_pipe.read_to_end(&mut buf);
        buf
    });

    let deadline = Instant::now() + budget;
    let mut timed_out = false;
    let status = loop {
        match child.try_wait()? {
            Some(status) => break status,
            None => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let status = child.wait()?; // reap the zombie
                    timed_out = true;
                    break status;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }
    };

    // Both write ends are now closed (child exited or was killed), so the
    // reader threads observe EOF and return promptly.
    let stdout = out_reader.join().unwrap_or_default();
    let stderr = err_reader.join().unwrap_or_default();
    Ok(CappedRun {
        output: std::process::Output {
            status,
            stdout,
            stderr,
        },
        timed_out,
    })
}

/// Transpile an Anubis source to Rust, compile it with cargo (audited crypto deps), and execute.
/// Returns the raw process `Output`. Fails closed if lowering or build fails.
pub fn compile_and_run_source(
    source: &str,
    allow_research: bool,
    args: &[String],
) -> Result<std::process::Output> {
    let ast = crate::frontend::parse_source(source).map_err(|e| anyhow!("parse: {}", e))?;
    // Prefer mono inventory when typecheck succeeds; fall back to empty mono on check errors
    // so runtime tests can still exercise fail-closed paths that only lower.
    let (mono, sites) = crate::middle::typecheck(ast.clone(), crate::frontend::Mode::Safe)
        .map(|ir| (ir.mono_specializations, ir.mono_call_sites))
        .unwrap_or_default();
    compile_and_run_items_with_mono(&ast.items, allow_research, args, &mono, &sites)
}

/// Bump when crypto dependency set or audited runtime changes (invalidates run cache).
pub const ANUBIS_RUN_CRYPTO_CACHE_TAG: &str = "audited-crypto-v3";

/// Shared cargo target dir so audited deps download once per machine.
pub fn anubis_run_shared_target_dir() -> std::path::PathBuf {
    if let Some(path) = std::env::var_os("ANUBIS_RUN_CARGO_TARGET_DIR") {
        return std::path::PathBuf::from(path);
    }
    let tag: String = ANUBIS_RUN_CRYPTO_CACHE_TAG
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    std::env::temp_dir().join(format!("anubis-run-cargo-target-{tag}"))
}

struct RunCargoBuildLock {
    lock_dir: std::path::PathBuf,
}

impl Drop for RunCargoBuildLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.lock_dir);
    }
}

fn parse_run_build_timeout_secs(raw: Option<&str>) -> Option<std::time::Duration> {
    const DEFAULT_SECS: u64 = 1800;
    let secs = raw
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(DEFAULT_SECS);
    if secs == 0 {
        None
    } else {
        Some(std::time::Duration::from_secs(secs))
    }
}

fn resolved_run_build_timeout() -> Option<std::time::Duration> {
    parse_run_build_timeout_secs(
        std::env::var("ANUBIS_RUN_BUILD_TIMEOUT_SECS")
            .ok()
            .as_deref(),
    )
}

fn pid_is_alive(pid: u32) -> bool {
    #[cfg(not(unix))]
    {
        let _ = pid;
        return true;
    }
    #[cfg(unix)]
    {
        std::process::Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
}

fn stale_run_build_lock_owner(lock_dir: &std::path::Path) -> bool {
    let owner = match std::fs::read_to_string(lock_dir.join("owner")) {
        Ok(owner) => owner,
        Err(_) => return false,
    };
    let Some(pid) = owner
        .lines()
        .find_map(|line| line.strip_prefix("pid="))
        .and_then(|pid| pid.trim().parse::<u32>().ok())
    else {
        return false;
    };
    !pid_is_alive(pid)
}

fn acquire_run_cargo_build_lock(target_dir: &std::path::Path) -> Result<RunCargoBuildLock> {
    let lock_dir = target_dir.join(".anubis-build-mutex");
    std::fs::create_dir_all(target_dir).map_err(|e| {
        anyhow!(
            "ANUBIS_RUN_BUILD_LOCK_FAILED: create target dir {}: {e}",
            target_dir.display()
        )
    })?;
    let start = std::time::Instant::now();
    let deadline = resolved_run_build_timeout().map(|timeout| start + timeout);
    loop {
        match std::fs::create_dir(&lock_dir) {
            Ok(()) => {
                let owner = format!(
                    "pid={}\nstarted_unix_ms={}\n",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or(0)
                );
                let _ = std::fs::write(lock_dir.join("owner"), owner);
                return Ok(RunCargoBuildLock { lock_dir });
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if stale_run_build_lock_owner(&lock_dir) {
                    let _ = std::fs::remove_dir_all(&lock_dir);
                    continue;
                }
                if deadline.is_some_and(|deadline| std::time::Instant::now() >= deadline) {
                    return Err(anyhow!(
                        "ANUBIS_RUN_BUILD_LOCK_TIMEOUT: waited for generated-run cargo lock at {}",
                        lock_dir.display()
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => {
                return Err(anyhow!(
                    "ANUBIS_RUN_BUILD_LOCK_FAILED: create {}: {e}",
                    lock_dir.display()
                ));
            }
        }
    }
}

fn anubis_run_cargo_toml(package_name: &str) -> String {
    // Unique package name per build so parallel `cargo build`s never clobber the same binary
    // under a shared CARGO_TARGET_DIR.
    format!(
        r#"[package]
name = "{package_name}"
version = "0.1.0"
edition = "2021"
publish = false

[dependencies]
sha2 = "0.10"
hmac = "0.12"
hkdf = "0.12"
chacha20poly1305 = "0.10"
argon2 = {{ version = "0.5", features = ["std", "password-hash", "rand"] }}
pbkdf2 = {{ version = "0.12", default-features = false, features = ["hmac"] }}
getrandom = "0.2"
subtle = "2"
ed25519-dalek = {{ version = "2", features = ["std", "rand_core"] }}
x25519-dalek = {{ version = "2", features = ["static_secrets"] }}

[profile.release]
opt-level = 2
lto = false
"#
    )
}

/// Codesign a macOS binary with the given entitlements plist.
/// `identity` is `"-"` for ad-hoc or an Apple Development identity name.
#[cfg(target_os = "macos")]
pub fn codesign_macos_binary(
    exe: &std::path::Path,
    entitlements_plist: &std::path::Path,
    identity: &str,
) -> Result<()> {
    let out = std::process::Command::new("codesign")
        .args(["--force", "--sign", identity, "--entitlements"])
        .arg(entitlements_plist)
        .arg(exe)
        .output()
        .map_err(|e| anyhow!("codesign spawn failed: {e}"))?;
    if !out.status.success() {
        return Err(anyhow!(
            "ANUBIS_CODESIGN_FAILED: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(())
}

/// Prefer `ANUBIS_CODESIGN_IDENTITY`, else first quoted `Apple Development: …` identity, else ad-hoc `"-"`.
#[cfg(target_os = "macos")]
pub fn resolve_codesign_identity() -> String {
    if let Ok(id) = std::env::var("ANUBIS_CODESIGN_IDENTITY") {
        if !id.trim().is_empty() {
            return id.trim().to_string();
        }
    }
    let out = std::process::Command::new("security")
        .args(["find-identity", "-v", "-p", "codesigning"])
        .output();
    if let Ok(out) = out {
        let s = String::from_utf8_lossy(&out.stdout);
        for line in s.lines() {
            // `  1) HASH "Apple Development: email (XXXX)"`
            if !line.contains("Apple Development:") {
                continue;
            }
            if let Some(start) = line.find('"') {
                let rest = &line[start + 1..];
                if let Some(end) = rest.find('"') {
                    return rest[..end].to_string();
                }
            }
        }
    }
    "-".to_string()
}

/// TeamIdentifier after signing (from `codesign -dvvv`). None for ad-hoc.
#[cfg(target_os = "macos")]
pub fn codesign_team_id_of(exe: &std::path::Path) -> Option<String> {
    let out = std::process::Command::new("codesign")
        .args(["-dvvv"])
        .arg(exe)
        .output()
        .ok()?;
    // codesign writes details to stderr
    let s = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix("TeamIdentifier=") {
            let t = rest.trim();
            if t != "not set" && !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    None
}

/// Minimal entitlements for signed CLI `anubis run` binaries.
/// Restricted keys (e.g. `com.apple.developer.secure-enclave`) kill unsigned/ad-hoc processes
/// under AMFI — omit them. App Sandbox is off (bare CLI, not a .app container).
#[cfg(target_os = "macos")]
pub fn signed_run_keychain_entitlements_xml() -> String {
    r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>com.apple.security.app-sandbox</key>
	<false/>
	<key>com.apple.security.get-task-allow</key>
	<true/>
</dict>
</plist>
"#
    .to_string()
}

/// Compile Anubis source → native binary → codesign with NE Keychain/SE entitlements → run.
/// On non-macOS, falls back to unsigned `compile_and_run_source`.
///
/// Evidence for the signed Keychain bind path (partial SE when hardware + identity allow).
pub fn compile_sign_and_run_source(
    src: &str,
    allow_research: bool,
    args: &[String],
) -> Result<std::process::Output> {
    #[cfg(not(target_os = "macos"))]
    {
        return compile_and_run_source(src, allow_research, args);
    }
    #[cfg(target_os = "macos")]
    {
        let ast = crate::frontend::parse_source(src).map_err(|e| anyhow!("parse failed: {e}"))?;
        let (mono, sites) = crate::middle::typecheck(ast.clone(), crate::frontend::Mode::Safe)
            .map(|ir| (ir.mono_specializations, ir.mono_call_sites))
            .unwrap_or_default();
        let rust_source =
            lower_program_to_rust_with_mono(&ast.items, allow_research, &mono, &sites)?;
        let dir =
            std::env::temp_dir().join(format!("anubis-signed-run-{}", anubis_unique_suffix()));
        std::fs::create_dir_all(&dir)?;
        let exe = dir.join("anubis_run");
        compile_native_rust_to_exe(&rust_source, &exe)?;

        // Safe CLI entitlements (no restricted SE key — that AMFI-kills without provisioning).
        let plist = signed_run_keychain_entitlements_xml();
        let plist_path = dir.join("program.entitlements");
        std::fs::write(&plist_path, &plist)?;
        let identity = resolve_codesign_identity();
        codesign_macos_binary(&exe, &plist_path, &identity)?;

        // Prefer Keychain bind under signed identity.
        // Do NOT set ANUBIS_KEYCHAIN_ACCESS_GROUP by default: login-keychain generic
        // passwords work without an access group; a team-prefixed group requires an
        // app id / provisioning match and soft-fails otherwise.
        let mut cmd = std::process::Command::new(&exe);
        cmd.args(args)
            .stdin(std::process::Stdio::null())
            .env("ANUBIS_KEYCHAIN_CAPS", "1")
            .env_remove("ANUBIS_KEYCHAIN_ACCESS_GROUP");
        if std::env::var_os("ANUBIS_KEYCHAIN_SE")
            .map(|v| v != "0" && v != "false")
            .unwrap_or(false)
        {
            cmd.env("ANUBIS_KEYCHAIN_SE", "1");
        }
        let _team = codesign_team_id_of(&exe); // available for forensics / future ACL slice
        let capped = run_child_capped(cmd, resolved_run_timeout())
            .map_err(|e| anyhow!("run spawn failed: {e}"))?;
        if std::env::var_os("ANUBIS_KEEP_SIGNED_RUN").is_none() {
            let _ = std::fs::remove_dir_all(&dir);
        }
        if capped.timed_out {
            return Err(anyhow!(
                "ANUBIS_RUN_TIMEOUT: signed program exceeded wall-clock budget"
            ));
        }
        Ok(capped.output)
    }
}

/// Compile lowered native Rust (with audited crypto) into `out_exe` via cargo.
pub fn compile_native_rust_to_exe(rust_source: &str, out_exe: &std::path::Path) -> Result<()> {
    let suffix = anubis_unique_suffix().replace('-', "_");
    // Cargo package names must be valid identifiers (no leading digits after renames).
    let package_name = format!("anubis_run_{suffix}");
    let dir = std::env::temp_dir().join(format!("anubis-run-build-{suffix}"));
    std::fs::create_dir_all(dir.join("src"))?;
    std::fs::write(dir.join("Cargo.toml"), anubis_run_cargo_toml(&package_name))?;
    std::fs::write(dir.join("src/main.rs"), rust_source)?;

    let target_dir = anubis_run_shared_target_dir();
    let _build_lock = acquire_run_cargo_build_lock(&target_dir)?;
    let cargo_build = |offline: bool| -> Result<std::process::Output> {
        let mut command = std::process::Command::new("cargo");
        command.args(["build", "--release", "--quiet"]);
        if offline {
            command.arg("--offline");
        }
        command
            .current_dir(&dir)
            .env("CARGO_TARGET_DIR", &target_dir);
        let capped = run_child_capped(command, resolved_run_build_timeout())
            .map_err(|e| anyhow!("cargo spawn failed: {}", e))?;
        if capped.timed_out {
            return Err(anyhow!(
                "ANUBIS_RUN_BUILD_TIMEOUT: cargo build exceeded wall-clock budget"
            ));
        }
        Ok(capped.output)
    };

    // Generated native projects use the audited dependency set already populated by the Anubis
    // build and the shared run cache. Prefer that cache so a transient registry/DNS outage cannot
    // break an otherwise reproducible Safe run. A fresh machine may have an incomplete cache, so
    // retry online unless the operator explicitly required Cargo offline mode.
    let operator_forced_offline = std::env::var("CARGO_NET_OFFLINE")
        .ok()
        .is_some_and(|value| !matches!(value.as_str(), "" | "0" | "false" | "FALSE"));
    let offline_build = cargo_build(true)?;
    let build = if offline_build.status.success() || operator_forced_offline {
        offline_build
    } else {
        let online_build = cargo_build(false)?;
        if online_build.status.success() {
            online_build
        } else {
            let offline_stderr = String::from_utf8_lossy(&offline_build.stderr);
            let online_stderr = String::from_utf8_lossy(&online_build.stderr);
            let _ = std::fs::remove_dir_all(&dir);
            return Err(anyhow!(
                "ANUBIS_UNSUPPORTED_NATIVE_LOWERING: cargo build failed (audited crypto deps):\n\
                 offline cache attempt:\n{}\n\
                 online fallback attempt:\n{}",
                offline_stderr,
                online_stderr
            ));
        }
    };
    if !build.status.success() {
        let stderr = String::from_utf8_lossy(&build.stderr).to_string();
        let _ = std::fs::remove_dir_all(&dir);
        return Err(anyhow!(
            "ANUBIS_UNSUPPORTED_NATIVE_LOWERING: cargo build failed (audited crypto deps):\n{}",
            stderr
        ));
    }
    let built = target_dir.join("release").join(&package_name);
    let built = if built.exists() {
        built
    } else {
        let alt = target_dir
            .join("release")
            .join(format!("{package_name}.exe"));
        if !alt.exists() {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(anyhow!(
                "ANUBIS_UNSUPPORTED_NATIVE_LOWERING: cargo reported success but binary missing at {}",
                built.display()
            ));
        }
        alt
    };
    if let Some(parent) = out_exe.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(&built, out_exe).map_err(|e| {
        let _ = std::fs::remove_dir_all(&dir);
        anyhow!("copy binary failed: {}", e)
    })?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// Compile+run an already-assembled item list (e.g. a multi-file program combined by `resolve`).
///
/// Native path builds a temporary Cargo project so the runtime can link **audited** crypto crates
/// (argon2, chacha20poly1305, hmac/sha2, hkdf, getrandom, subtle). RISC0 guests keep pure crypto.
pub fn compile_and_run_items(
    items: &[Item],
    allow_research: bool,
    args: &[String],
) -> Result<std::process::Output> {
    compile_and_run_items_with_mono(items, allow_research, args, &[], &[])
}

pub fn compile_and_run_items_with_mono(
    items: &[Item],
    allow_research: bool,
    args: &[String],
    mono: &[crate::middle::MonoSpecialization],
    mono_call_sites: &[crate::middle::MonoCallSite],
) -> Result<std::process::Output> {
    let rust_source =
        lower_program_to_rust_with_mono(items, allow_research, mono, mono_call_sites)?;
    let dir = std::env::temp_dir().join(format!("anubis-run-{}", anubis_unique_suffix()));
    std::fs::create_dir_all(&dir)?;
    let exe = dir.join("anubis_run");
    compile_native_rust_to_exe(&rust_source, &exe)?;
    let mut cmd = std::process::Command::new(&exe);
    // No interactive input on this path: mirror the old `.output()` semantics
    // where a child that reads stdin sees an immediate EOF rather than blocking.
    cmd.args(args).stdin(std::process::Stdio::null());
    let capped = run_child_capped(cmd, resolved_run_timeout())
        .map_err(|e| anyhow!("run spawn failed: {}", e))?;
    let _ = std::fs::remove_dir_all(&dir);
    if capped.timed_out {
        return Err(anyhow!(
            "ANUBIS_RUN_TIMEOUT: program exceeded its wall-clock budget and was killed \
             (raise or disable via ANUBIS_RUN_TIMEOUT_SECS)"
        ));
    }
    Ok(capped.output)
}

#[cfg(test)]
mod run_tests {
    use super::*;

    #[test]
    fn run_timeout_policy_defaults_and_opt_out() {
        use std::time::Duration;
        // Absent / unparseable → 3600s work-class default.
        assert_eq!(
            parse_run_timeout_secs(None),
            Some(Duration::from_secs(3600))
        );
        assert_eq!(
            parse_run_timeout_secs(Some("not-a-number")),
            Some(Duration::from_secs(3600))
        );
        // Explicit positive value is honored (whitespace tolerated).
        assert_eq!(
            parse_run_timeout_secs(Some(" 5 ")),
            Some(Duration::from_secs(5))
        );
        // Zero is the documented opt-out: unbounded.
        assert_eq!(parse_run_timeout_secs(Some("0")), None);
    }

    #[test]
    fn run_child_capped_returns_output_when_program_is_fast() {
        use std::time::Duration;
        let mut cmd = std::process::Command::new("echo");
        cmd.arg("hello-from-child");
        let capped = run_child_capped(cmd, Some(Duration::from_secs(30))).expect("spawn");
        assert!(
            !capped.timed_out,
            "a fast child must not be flagged timed_out"
        );
        assert!(capped.output.status.success());
        assert!(String::from_utf8_lossy(&capped.output.stdout).contains("hello-from-child"));
    }

    #[test]
    fn run_child_capped_kills_runaway_native_binary() {
        use std::time::{Duration, Instant};
        // Compile a genuine spinning native binary — exactly the shape `anubis run`
        // emits for `while true {}` — and prove the watchdog SIGKILLs it well inside
        // the budget instead of blocking forever (the orphaned-process leak this fixes).
        let dir =
            std::env::temp_dir().join(format!("anubis-timeout-test-{}", anubis_unique_suffix()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let rs = dir.join("spin.rs");
        let exe = dir.join("spin");
        std::fs::write(&rs, "fn main() { loop { std::hint::spin_loop(); } }").expect("write");
        let build = std::process::Command::new("rustc")
            .arg(&rs)
            .arg("-o")
            .arg(&exe)
            .output()
            .expect("rustc");
        assert!(
            build.status.success(),
            "rustc failed: {}",
            String::from_utf8_lossy(&build.stderr)
        );
        let cmd = std::process::Command::new(&exe);
        let start = Instant::now();
        let capped = run_child_capped(cmd, Some(Duration::from_millis(400))).expect("spawn");
        let elapsed = start.elapsed();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            capped.timed_out,
            "spinning binary should hit the wall-clock cap"
        );
        assert!(
            elapsed < Duration::from_secs(5),
            "watchdog must not hang; killed after {elapsed:?}"
        );
    }

    /// Compile+run an Anubis program and return trimmed stdout, asserting success.
    fn run(src: &str) -> String {
        let out = compile_and_run_source(src, false, &[]).expect("compile+run");
        assert!(
            out.status.success(),
            "program exited nonzero.\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// Compile+run a program that is expected to FAIL CLOSED at runtime:
    /// assert the process exits nonzero and its stderr carries `needle`.
    fn run_expect_trap(src: &str, needle: &str) {
        let out = compile_and_run_source(src, false, &[]).expect("compile+run");
        assert!(
            !out.status.success(),
            "program was expected to trap ({needle}) but exited 0.\nstdout: {}",
            String::from_utf8_lossy(&out.stdout)
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains(needle),
            "expected trap {needle}, got stderr:\n{stderr}"
        );
    }

    #[test]
    fn runtime_enforces_assume_and_integer_param_soundness() {
        // REGRESSION — soundness audit 2026-07-11. RC3: the checker trusts every `assume` as an
        // axiom, so the runtime must enforce it; a satisfiable-but-false assume would otherwise
        // silently certify a violated contract. `assume(x < 100)` reached with x = i64::MAX fails closed.
        run_expect_trap(
            "fn f(x: i64) -> i64 requires(x > 0) ensures(result > x) { assume(x < 100); return x + 1; } \
             fn main() { print(f(9223372036854775807)); }",
            "ANUBIS_ASSUME_VIOLATED",
        );
        // A true assume does not trap and still yields a value in expression position.
        assert_eq!(
            run("fn f(x: u32) -> u32 { assume(x > 0); return x; } fn main() { print(f(5)); }"),
            "5"
        );
        // RC4: an integer-typed parameter is modeled as a pure i64; a float/string argument would take
        // a divergent runtime path (float remainder, `+` concatenation) and violate the proof — fail closed.
        run_expect_trap(
            "fn parity(x: u32) -> u32 ensures(result == 0 || result == 1) { return x % 2; } \
             fn main() { print(parity(0.5)); }",
            "ANUBIS_TYPE_VIOLATION",
        );
        run_expect_trap(
            "fn twice(x: u32) -> u32 ensures(result == 2 * x) { return x + x; } \
             fn main() { print(twice(\"5\")); }",
            "ANUBIS_TYPE_VIOLATION",
        );
        // Valid integer arguments still run and compute correctly (no false positives).
        assert_eq!(
            run("fn parity(x: u32) -> u32 ensures(result == 0 || result == 1) { return x % 2; } fn main() { print(parity(7)); print(parity(8)); }"),
            "1\n0"
        );
    }

    #[test]
    fn unsigned_and_signed_integer_boundaries_match_solver_contract() {
        let src = "fn u(x:u32)->u32 { return x; } fn s(x:i64)->i64 { return x; } \
                   fn main(){ print(u(-1)); print(u(2147483648)); print(u(4294967296)); \
                   print(u(5000000000)); print(s(9223372036854775807)); \
                   print(s(-9223372036854775807-1)); }";
        assert_eq!(
            run(src),
            "4294967295\n2147483648\n0\n705032704\n9223372036854775807\n-9223372036854775808"
        );
    }

    #[test]
    fn runtime_enforces_integer_return_type() {
        // REGRESSION — fix-adversary re-audit 2026-07-11 (RC6). An integer-typed function's result is
        // modeled as an i64 (directly and via composition), but return types are INERT at runtime — a
        // body returning a non-integer would poison that model. Every return path is guarded.
        // Direct: a `-> u32` function that returns a float fails closed (check passes; return type inert).
        run_expect_trap(
            "fn g() -> u32 { return 2.5; } fn main() { print(g()); }",
            "ANUBIS_TYPE_VIOLATION",
        );
        // Via composition: `let name = g(5)` where g returns f64, then returned from a `-> u32` fn.
        run_expect_trap(
            "fn g(a: u32) -> f64 { return 2.5; } fn f() -> u32 { let name = g(5); return name; } \
             fn main() { print(f()); }",
            "ANUBIS_TYPE_VIOLATION",
        );
        // `return assume(x)` yields Bool(true), not an integer — fail closed at the return boundary too.
        run_expect_trap(
            "fn f(x: u32) -> u32 { return assume(x); } fn main() { print(f(5)); }",
            "ANUBIS_TYPE_VIOLATION",
        );
        // Valid integer returns still work; a non-integer RETURN TYPE is unguarded (floats are legal).
        assert_eq!(
            run("fn sq(x: u32) -> u32 { return x * x; } fn main() { print(sq(6)); }"),
            "36"
        );
        assert_eq!(
            run("fn half() -> f64 { return 2.5; } fn main() { print(half()); }"),
            "2.5"
        );
    }

    /// Compile+run a program feeding `stdin_bytes` to its standard input, returning trimmed stdout.
    /// Mirrors the CLI `run` path, which inherits stdin so `input()`/`read_line()` work.
    ///
    /// Compile through the SAME cargo path the CLI uses (`compile_native_rust_to_exe`),
    /// not bare `rustc`: the native runtime links audited crypto crates (argon2,
    /// getrandom, subtle, …) that only resolve against the generated Cargo.toml. Bare
    /// `rustc` has no dependency graph, so it cannot compile the emitted runtime — the
    /// cargo path is what `anubis run` actually does.
    fn run_with_stdin(src: &str, stdin_bytes: &[u8]) -> String {
        use std::io::Write;
        let ast = crate::frontend::parse_source(src).expect("parse");
        let rust_source = lower_program_to_rust(&ast.items, false).expect("lower");
        let dir = std::env::temp_dir().join(format!("anubis-stdin-{}", anubis_unique_suffix()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("anubis_run");
        compile_native_rust_to_exe(&rust_source, &exe)
            .expect("compile via cargo (audited crypto deps)");
        let mut child = std::process::Command::new(&exe)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn");
        child.stdin.take().unwrap().write_all(stdin_bytes).unwrap();
        let out = child.wait_with_output().expect("wait");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            out.status.success(),
            "program exited nonzero.\nstderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    #[test]
    fn read_line_reads_stdin() {
        // `read_line()` must consume real stdin (the CLI inherits it). Two lines -> 40 + 2 = 42.
        assert_eq!(
            run_with_stdin(
                "fn main() { let a = int(read_line()); let b = int(read_line()); print(a + b); }",
                b"40\n2\n",
            ),
            "42"
        );
        // `input` is an alias and strips the trailing newline.
        assert_eq!(
            run_with_stdin("fn main() { print(\"hi \" + input()); }", b"there\n"),
            "hi there"
        );
    }

    #[test]
    fn arithmetic_precedence() {
        assert_eq!(run("fn main() { print(2 + 3 * 4 - 1); }"), "13");
    }

    #[test]
    fn index_out_of_bounds_fails_closed() {
        // Fail-closed: an explicit list index past the end traps, it does not silently return 0.
        run_expect_trap(
            "fn main() { let xs = [10, 20, 30]; print(xs[10]); }",
            "ANUBIS_INDEX_OUT_OF_BOUNDS",
        );
        // Negative-out-of-range also traps.
        run_expect_trap(
            "fn main() { let xs = [1, 2]; print(xs[-9]); }",
            "ANUBIS_INDEX_OUT_OF_BOUNDS",
        );
        // String index past the end traps too.
        run_expect_trap(
            "fn main() { print(char_at(\"abc\", 7)); }",
            "ANUBIS_INDEX_OUT_OF_BOUNDS",
        );
    }

    #[test]
    fn missing_map_key_fails_closed() {
        // Fail-closed: `m[k]` on an absent key traps instead of returning 0.
        run_expect_trap(
            "fn main() { let mut m = {}; m[\"a\"] = 1; print(m[\"zzz\"]); }",
            "ANUBIS_MISSING_KEY",
        );
    }

    #[test]
    fn safe_accessors_survive_fail_closed_indexing() {
        // get()/has_key() and valid negative indexing remain the optional-access path.
        let src = "fn main() { \
            let xs = [10, 20, 30]; let mut m = {}; m[\"a\"] = 1; \
            print(get(xs, 10, -1)); print(get(m, \"zzz\", -2)); \
            print(has_key(m, \"a\")); print(xs[-1]); }";
        assert_eq!(run(src), "-1\n-2\ntrue\n30");
    }

    #[test]
    fn indexed_reads_fast_path_is_correct() {
        // The borrow-not-clone fast path for `local[i]` must preserve value semantics, including
        // in-place algorithms and an index expression that reads the same local.
        let insort = "fn main() { let mut a = [5, 3, 1, 4, 2]; let n = 5; let mut k = 1; \
            while k < n { let key = a[k]; let mut j = k - 1; \
            while j >= 0 && a[j] > key { a[j + 1] = a[j]; j = j - 1; } a[j + 1] = key; k = k + 1; } \
            print(a); }";
        assert_eq!(run(insort), "[1, 2, 3, 4, 5]");
        // self-referential index: a[0] == 2, so a[a[0]] == a[2] == 9
        assert_eq!(
            run("fn main() { let a = [2, 7, 9, 4]; print(a[a[0]]); }"),
            "9"
        );
        // reading an element is a copy, not an alias: mutating the source afterward is independent
        assert_eq!(
            run("fn main() { let mut a = [1, 2, 3]; let x = a[1]; a[1] = 99; print(x + a[1]); }"),
            "101"
        );
    }

    #[test]
    fn research_words_usable_as_identifiers() {
        // Soft research keywords must work as ordinary variables and user-defined function names on
        // the run path (they only form constructs in their specific syntactic form).
        assert_eq!(
            run("fn unified(a, b) { a + b } fn main() { let unified = unified(3, 4); print(unified); }"),
            "7"
        );
        assert_eq!(
            run(
                "fn main() { let cpu = 2; let gpu = 3; let prove = 5; let spec = 7; \
                 let symbolic = 9; let declassify = 11; \
                 print(cpu + gpu + prove + spec + symbolic + declassify); }"
            ),
            "37"
        );
    }

    #[test]
    fn is_empty_covers_collections() {
        let src = "fn main() { \
            print(is_empty([])); print(is_empty([1])); \
            print(is_empty(\"\")); print(is_empty(\"x\")); \
            let mut m = {}; print(is_empty(m)); m[\"a\"] = 1; print(is_empty(m)); }";
        assert_eq!(run(src), "true\nfalse\ntrue\nfalse\ntrue\nfalse");
    }

    #[test]
    fn parse_opt_returns_matchable_option() {
        // Fail-closed parse variants return Some(n)/None so a program can tell "the number 0"
        // from "not a number" (unlike lenient parse_int, which returns 0 for both).
        let src = "fn go(s) { match parse_int_opt(s) { Some(n) => n, None => -1 } } \
                   fn main() { print(go(\"42\")); print(go(\"0\")); print(go(\"abc\")); print(go(\"12x\")); }";
        assert_eq!(run(src), "42\n0\n-1\n-1");
        let fsrc =
            "fn main() { print(match parse_float_opt(\"3.5\") { Some(f) => f, None => 0.0 }); \
                    print(match parse_float_opt(\"x\") { Some(f) => f, None => -1.0 }); }";
        assert_eq!(run(fsrc), "3.5\n-1.0");
    }

    #[test]
    fn float_param_and_return_coerce_int() {
        // task #34: an Int argument to a float-typed PARAM is coerced to a Float at the boundary, so the
        // checker's f64 model is sound — the param genuinely holds a float and `x / 2.0` is float division,
        // not integer. `half(7)` binds x = 7.0 → 3.5 (was a checker/runtime divergence: the checker proved
        // 3.5 while the runtime did Int(7)/2 = 3).
        assert_eq!(
            run("fn half(x: f64) -> f64 { return x / 2.0; } fn main() { print(half(7)); }"),
            "3.5"
        );
        assert_eq!(
            run("fn f(x: f64) -> f64 { return x + 1.0; } fn main() { print(f(2)); }"),
            "3.0"
        );
        // a float RETURN coerces an Int result to a Float likewise (dual of anubis_require_int_ret).
        assert_eq!(
            run("fn g() -> f64 { return 7; } fn main() { print(g()); }"),
            "7.0"
        );
    }

    #[test]
    fn match_statement_in_closure_and_arm_bodies() {
        // A `match` used as a non-final statement inside a closure body or a match-arm body must
        // parse (it is a block-like statement, no `;` needed) — regression: it was mis-parsed as
        // the block tail, breaking the following statement.
        assert_eq!(
            run("fn main() { let g = |x| { match x { 1 => print(\"one\"), _ => print(\"other\"), } \
                 print(\"after\"); }; g(1); }"),
            "one\nafter"
        );
        assert_eq!(
            run("fn main() { let x = 1; match x { \
                 1 => { match x { 1 => print(\"inner\"), _ => print(\"io\"), } print(\"armtail\"); } \
                 _ => print(\"other\"), } }"),
            "inner\narmtail"
        );
        // `match` as a block VALUE (tail) is still returned, not swallowed as a statement.
        assert_eq!(
            run(
                "fn main() { let f = |n| { match n { 0 => \"zero\", _ => \"nonzero\" } }; \
                 print(f(0)); print(f(5)); }"
            ),
            "zero\nnonzero"
        );
    }

    #[test]
    fn every_emitted_user_fn_carries_the_stack_guard() {
        // The recursion trap is only as good as its coverage: `emit_fn_core` has FOUR emission
        // shapes (full-native mono, boxed mono, return-guarded, plain) and a fifth added later
        // that forgets the guard would reopen the process-abort behaviour silently. Count the
        // emitted `fn ` definitions against the guard calls instead of trusting review.
        //
        // The program below is chosen to exercise several shapes at once: a monomorphizable
        // arithmetic fn, a declared-integer-return fn, and an untyped fn.
        let src = "fn sq(x: u32) -> u32 { return x * x; } \
                   fn tag(s) { return s; } \
                   fn main() { print(sq(3)); print(tag(1)); }";
        let toks = crate::frontend::lex(src);
        let items = crate::frontend::parse(toks).expect("parse");
        let rust = lower_program_to_rust(&items.items, false).expect("lower");
        // Only the user-function definitions, not the runtime's own helpers: user fns are emitted
        // into `functions_src` with the `anb_` prefix that `sanitize_ident`/`rust_name` applies.
        let user_fns = rust.matches("\nfn anb_").count();
        let guards = rust.matches("__anb_stack_guard();").count();
        assert!(
            user_fns >= 3,
            "expected the 3 user fns to be emitted, saw {user_fns}"
        );
        assert_eq!(
            guards, user_fns,
            "every emitted user function must call __anb_stack_guard(); {user_fns} fns but \
             {guards} guards — an emission shape is missing it, which silently restores the \
             process-abort-on-stack-overflow behaviour"
        );
    }

    #[test]
    fn unbounded_recursion_traps_instead_of_aborting() {
        // CLAIMS item 13: `check` ACCEPTS a mutual-return cycle, and `run` used to die with
        // `fatal runtime error: stack overflow, aborting` — an abort, not a panic, so none of the
        // fail-closed diagnostic path ran. It must now surface an attributable ANUBIS_* code.
        run_expect_trap(
            "fn ping() -> i64 { return pong(); } \
             fn pong() -> i64 { return ping(); } \
             fn main() { print(ping()); }",
            "ANUBIS_RECURSION_LIMIT",
        );
    }

    #[test]
    fn deep_recursion_runs_on_large_stack() {
        // The program runs on a 1 GiB worker stack, so recursion far past the 8 MiB main-thread
        // ceiling (~8500 frames) succeeds. 100k deep would overflow the OS main stack.
        assert_eq!(
            run(
                "fn walk(n, acc) { if n == 0 { acc } else { walk(n - 1, acc + 1) } } \
                 fn main() { print(walk(100000, 0)); }"
            ),
            "100000"
        );
    }

    #[test]
    fn trap_still_fails_closed_on_worker_thread() {
        // A fail-closed trap must still surface (stderr message + nonzero exit) now that the
        // program runs on a worker thread rather than the main thread.
        run_expect_trap(
            "fn main() { let xs = [1, 2]; print(xs[9]); }",
            "ANUBIS_INDEX_OUT_OF_BOUNDS",
        );
    }

    #[test]
    fn wildcard_binding_lowers_without_mut() {
        // Rust forbids `mut _`; the wildcard `_` in a let, for-loop, closure param, or fn param
        // must lower bare. Regression: the emitter used to emit `let mut _` / `for mut _`.
        assert_eq!(run("fn main() { let _ = 1; print(42); }"), "42");
        assert_eq!(
            run("fn main() { let mut n = 0; for _ in 0..3 { n = n + 1; } print(n); }"),
            "3"
        );
        // wildcard over a collection
        assert_eq!(
            run("fn main() { let mut c = 0; for _ in [10, 20, 30] { c = c + 1; } print(c); }"),
            "3"
        );
        // wildcard closure param
        assert_eq!(run("fn main() { let f = |_| 7; print(f(999)); }"), "7");
        // wildcard fn param
        assert_eq!(
            run("fn g(_, y) { y * 2 } fn main() { print(g(100, 21)); }"),
            "42"
        );
    }

    #[test]
    fn get_returns_element_for_lists_strings_and_maps() {
        // `get` is the fail-SOFT accessor: in-bounds / present keys must return the ELEMENT,
        // not the default (regression: it used to only handle maps and defaulted lists/strings).
        let src = "fn main() { \
            let xs = [10, 20, 30]; \
            print(get(xs, 0, -1)); print(get(xs, 2, -1)); print(get(xs, -1, -1)); \
            print(get(\"hello\", 1, \"?\")); \
            let mut m = {}; m[\"a\"] = 7; print(get(m, \"a\", -1)); \
            print(get(xs, 99, -1)); print(get(\"hi\", 9, \"?\")); print(get(m, \"z\", -1)); }";
        // in-bounds: 10, 30, 30(last), "e", 7 ; out-of-range/missing fall to default: -1, "?", -1
        assert_eq!(run(src), "10\n30\n30\ne\n7\n-1\n?\n-1");
    }

    #[test]
    fn recursion_fibonacci() {
        let src = "fn fib(n: u32) { if n < 2 { return n; } return fib(n-1) + fib(n-2); } \
                   fn main() { print(fib(10)); }";
        assert_eq!(run(src), "55");
    }

    #[test]
    fn while_loop_mutation() {
        let src =
            "fn main() { let i = 0; let s = 0; while i < 5 { s = s + i; i = i + 1; } print(s); }";
        assert_eq!(run(src), "10");
    }

    #[test]
    fn arrays_and_indexing() {
        let src = "fn main() { let a = [1,2,3]; a[1] = 9; print(a[0] + a[1] + a[2]); }";
        assert_eq!(run(src), "13");
    }

    #[test]
    fn map_iteration() {
        let src = "fn main() { let m = { \"a\": 1, \"b\": 2 }; m[\"c\"] = 3; let s = 0; \
                   for k in m { s = s + m[k]; } print(s); }";
        assert_eq!(run(src), "6");
    }

    #[test]
    fn enum_match_binding() {
        let src = "enum E { A, B(u32) } fn main() { let e = E::B(7); \
                   let r = match e { E::A => 0, E::B(n) => n, _ => 1 }; print(r); }";
        assert_eq!(run(src), "7");
    }

    #[test]
    fn match_literal_patterns() {
        let src = "fn name(n) { return match n { 0 => \"zero\", 1 => \"one\", _ => \"many\" }; } \
                   fn main() { print(name(0)); print(name(1)); print(name(9)); }";
        assert_eq!(run(src), "zero\none\nmany");
    }

    #[test]
    fn match_or_patterns() {
        let src = "fn kind(n) { return match n { 1 | 2 | 3 => \"small\", 4 | 5 => \"mid\", _ => \"big\" }; } \
                   fn main() { print(kind(2)); print(kind(5)); print(kind(99)); }";
        assert_eq!(run(src), "small\nmid\nbig");
    }

    #[test]
    fn match_guards_fall_through_in_order() {
        let src = "fn grade(n) { return match n { \
                     n if n >= 90 => \"A\", n if n >= 80 => \"B\", n if n >= 70 => \"C\", _ => \"F\" }; } \
                   fn main() { print(grade(95)); print(grade(85)); print(grade(50)); }";
        assert_eq!(run(src), "A\nB\nF");
    }

    #[test]
    fn match_binding_catchall_binds_scrutinee() {
        let src = "fn echo(s) { return match s { \"hi\" => \"greeting\", other => other }; } \
                   fn main() { print(echo(\"hi\")); print(echo(\"xyz\")); }";
        assert_eq!(run(src), "greeting\nxyz");
    }

    #[test]
    fn match_negative_literal_pattern() {
        let src = "fn sign(n) { return match n { -1 => \"neg\", 0 => \"zero\", _ => \"pos\" }; } \
                   fn main() { print(sign(-1)); print(sign(0)); print(sign(5)); }";
        assert_eq!(run(src), "neg\nzero\npos");
    }

    #[test]
    fn match_struct_variant_and_ignored_field() {
        let src = "enum Shape { Circle(u32), Rect { w: u32, h: u32 }, Dot } \
                   fn area(s) { return match s { \
                     Shape::Circle(r) => 3 * r * r, \
                     Shape::Rect { w: w, h: h } => w * h, \
                     Shape::Dot => 0 }; } \
                   fn main() { print(area(Shape::Circle(10))); \
                     print(area(Shape::Rect { w: 4, h: 5 })); print(area(Shape::Dot)); }";
        assert_eq!(run(src), "300\n20\n0");
    }

    #[test]
    fn implicit_tail_return_no_keyword() {
        // A bare trailing expression is the function's value — no `return` needed.
        let src = "fn double(n) { n * 2 } fn main() { print(double(21)); }";
        assert_eq!(run(src), "42");
    }

    #[test]
    fn match_as_statement_side_effect() {
        let src = "fn main() { let n = 2; \
                   match n { 1 => print(\"one\"), 2 => print(\"two\"), _ => print(\"other\") } }";
        assert_eq!(run(src), "two");
    }

    #[test]
    fn nested_match_no_label_collision() {
        // Two matches nested in one arm must not collide on any generated control label.
        let src = "fn f(a, b) { return match a { \
                     0 => match b { 0 => \"00\", _ => \"0x\" }, \
                     _ => match b { 0 => \"x0\", _ => \"xx\" } }; } \
                   fn main() { print(f(0,0)); print(f(0,9)); print(f(9,0)); print(f(9,9)); }";
        assert_eq!(run(src), "00\n0x\nx0\nxx");
    }

    #[test]
    fn match_arm_block_body() {
        // A `{`-led arm body is a block (statements + tail value), not a map literal.
        let src = "enum T { N(int), Op(string) } \
                   fn ev(t, a, b) { return match t { \
                     T::N(n) => n, \
                     T::Op(o) => { let r = a + b; r * 2 } }; } \
                   fn main() { print(ev(T::N(7), 0, 0)); print(ev(T::Op(\"+\"), 3, 4)); }";
        assert_eq!(run(src), "7\n14");
    }

    #[test]
    fn match_binding_named_rust_keyword() {
        // A binding whose name is a Rust keyword (`type`, `ref`) must be escaped, not emitted raw.
        let src = "fn f(n) { match n { type if type > 5 => \"big\", _ => \"small\" } } \
                   fn main() { print(f(9)); print(f(2)); } ";
        assert_eq!(run(src), "big\nsmall");
        let src2 = "enum E { A(int) } \
                    fn main() { match E::A(7) { E::A(ref) => print(ref) } print(\"end\"); }";
        assert_eq!(run(src2), "7\nend");
    }

    #[test]
    fn let_binding_named_rust_keyword() {
        // Same escaping must apply to ordinary variables and parameters, not just match bindings.
        let src = "fn make(move) { move * 2 } fn main() { let type = make(21); print(type); }";
        assert_eq!(run(src), "42");
    }

    #[test]
    fn match_literals_are_type_exact() {
        // A literal pattern matches only a value of the same kind — no cross-type coercion.
        assert_eq!(
            run("fn main() { print(match 5 { \"5\" => \"str\", 5 => \"int\", _ => \"no\" }) }"),
            "int"
        );
        assert_eq!(
            run("fn main() { print(match \"5\" { 5 => \"int\", \"5\" => \"str\", _ => \"no\" }) }"),
            "str"
        );
        assert_eq!(
            run("fn main() { print(match 1 { true => \"T\", 1 => \"one\", _ => \"no\" }) }"),
            "one"
        );
        // …but same-kind literals still match, and int/float stay numerically comparable.
        assert_eq!(
            run(
                "fn main() { print(match true { true => \"y\", _ => \"n\" }); \
                 print(match 5 { 5 => \"i\", _ => \"n\" }); \
                 print(match \"hi\" { \"hi\" => \"s\", _ => \"n\" }) }"
            ),
            "y\ni\ns"
        );
    }

    #[test]
    fn string_literal_with_control_escapes() {
        // NUL and other control-char escapes must round-trip through the transpiler.
        let src = "fn main() { print(match \"a\\0b\" { \"a\\0b\" => \"nul\", _ => \"no\" }) }";
        assert_eq!(run(src), "nul");
    }

    #[test]
    fn match_underscore_struct_variant_field() {
        let src = "enum Rec { Full { x: int, y: int } } \
                   fn main() { print(match Rec::Full { x: 9, y: 4 } { Rec::Full { x: a, y: _ } => a }) }";
        assert_eq!(run(src), "9");
    }

    #[test]
    fn match_scrutinee_disambiguates_unit_vs_struct_variant() {
        // Unit-variant scrutinee: the `{` opens the match body (no fields to parse).
        assert_eq!(
            run("enum St { A, B } fn main() { print(match St::A { St::A => 1, St::B => 2 }) }"),
            "1"
        );
        // Struct-variant scrutinee: `{ x: 7 }` is the construction; the following `{` is the body.
        assert_eq!(
            run("enum R { F { x: int } } fn main() { print(match R::F { x: 7 } { R::F { x: a } => a }) }"),
            "7"
        );
    }

    #[test]
    fn implicit_return_trailing_if() {
        let src = "fn sign(n) { if n < 0 { 111 } else { 222 } } \
                   fn main() { print(sign(-3)); print(sign(4)); }";
        assert_eq!(run(src), "111\n222");
        // else-if chains and branches with their own statements
        let src2 = "fn g(n) { if n < 0 { \"neg\" } else if n == 0 { let z = \"ze\"; z } else { \"pos\" } } \
                    fn main() { print(g(-1)); print(g(0)); print(g(5)); }";
        assert_eq!(run(src2), "neg\nze\npos");
    }

    #[test]
    fn tuple_values_and_list_patterns() {
        // `(a, b)` is a list value; list/tuple patterns match by length and destructure.
        let src = "fn kind(p) { return match p { [0, 0] => \"origin\", [0, y] => \"y-axis\", \
                     [x, 0] => \"x-axis\", [x, y] => \"point\", _ => \"?\" }; } \
                   fn main() { print(kind((0,0))); print(kind((0,5))); print(kind((3,0))); print(kind((3,4))); }";
        assert_eq!(run(src), "origin\ny-axis\nx-axis\npoint");
    }

    #[test]
    fn list_pattern_by_arity() {
        let src = "fn sz(xs) { return match xs { [] => \"none\", [a] => \"one\", [a, b] => \"two\", _ => \"many\" }; } \
                   fn main() { print(sz([])); print(sz([1])); print(sz([1,2])); print(sz([1,2,3])); }";
        assert_eq!(run(src), "none\none\ntwo\nmany");
    }

    #[test]
    fn let_destructuring_and_multiple_return() {
        let src = "fn bounds(xs) { let lo = xs[0]; let hi = xs[0]; \
                     for x in xs { if x < lo { lo = x } if x > hi { hi = x } } (lo, hi) } \
                   fn main() { let (lo, hi) = bounds([5,2,9,1,7]); print(lo); print(hi); \
                     let [a, b, c] = [10, 20, 30]; print(a + b + c); \
                     let (_, y) = (\"x\", \"y\"); print(y); }";
        assert_eq!(run(src), "1\n9\n60\ny");
    }

    #[test]
    fn nested_destructuring() {
        let src = "fn main() { let [[p, q], r] = [[1, 2], 3]; print(p + q + r); }";
        assert_eq!(run(src), "6");
    }

    #[test]
    fn equality_is_structural_and_type_exact() {
        // `==` compares by structure/type, not by display string.
        let src = "fn main() { \
                     print((1, 2) == [\"1, 2\"]); \
                     print(\"5\" == 5); \
                     print(true == 1); \
                     print([1, [2, 3]] == [1, [2, 3]]); \
                     print([1, 2] == [1, 2, 3]); \
                     print(5 == 5.0); }";
        assert_eq!(run(src), "false\nfalse\nfalse\ntrue\nfalse\ntrue");
    }

    #[test]
    fn destructure_non_list_defaults_to_zero() {
        // Destructuring a non-list (or too-short list) binds the default 0 — no string char-slicing.
        let src = "fn main() { let [a, b] = \"xy\"; let [c, d] = 42; let [e, f, g] = [1]; \
                   print(a, b, c, d, e, f, g); }";
        assert_eq!(run(src), "0 0 0 0 1 0 0");
    }

    #[test]
    fn list_pattern_with_literal_element() {
        // A literal element in a list pattern constrains that position.
        let src = "fn f(cmd) { return match cmd { [\"add\", a, b] => a + b, [\"neg\", a] => 0 - a, _ => -1 }; } \
                   fn main() { print(f([\"add\", 3, 4])); print(f([\"neg\", 5])); print(f([\"x\"])); }";
        assert_eq!(run(src), "7\n-5\n-1");
    }

    #[test]
    fn option_result_construct_and_match() {
        let src = "fn div(a, b) { if b == 0 { return None } Some(a / b) } \
                   fn show(o) { return match o { Some(v) => v, None => -1 }; } \
                   fn main() { print(show(div(10, 2))); print(show(div(1, 0))); \
                     print(match Ok(7) { Ok(v) => v, Err(e) => 0 }); \
                     print(match Err(9) { Ok(v) => v, Err(e) => e }); }";
        assert_eq!(run(src), "5\n-1\n7\n9");
    }

    #[test]
    fn question_operator_propagates_and_unwraps() {
        let src = "fn div(a, b) { if b == 0 { return None } Some(a / b) } \
                   fn add1(a, b) { let q = div(a, b)?; Some(q + 1) } \
                   fn show(o) { return match o { Some(v) => v, None => -1 }; } \
                   fn main() { print(show(add1(10, 2))); print(show(add1(10, 0))); }";
        assert_eq!(run(src), "6\n-1");
    }

    #[test]
    fn if_let_binds_on_match() {
        let src = "fn first_even(xs) { for x in xs { if x % 2 == 0 { return Some(x) } } None } \
                   fn main() { \
                     if let Some(v) = first_even([1, 3, 4, 7]) { print(v) } else { print(\"no\") } \
                     if let Some(v) = first_even([1, 3, 5]) { print(v) } else { print(\"no\") } }";
        assert_eq!(run(src), "4\nno");
    }

    #[test]
    fn while_let_loops_until_none() {
        let src = "fn last(xs) { if len(xs) == 0 { return None } Some(xs[len(xs) - 1]) } \
                   fn ini(xs) { let mut o = []; let mut i = 0; while i < len(xs) - 1 { push(o, xs[i]); i = i + 1; } o } \
                   fn main() { let mut s = [1, 2, 3]; \
                     while let Some(t) = last(s) { print(t); s = ini(s); } }";
        assert_eq!(run(src), "3\n2\n1");
    }

    #[test]
    fn nested_option_pattern() {
        // `Some(None)` is a nested pattern (a Some wrapping a None), matched precisely.
        let src = "fn f(o) { return match o { Some(None) => 1, Some(v) => v, None => 0 }; } \
                   fn main() { print(f(Some(None))); print(f(Some(7))); print(f(None)); }";
        assert_eq!(run(src), "1\n7\n0");
    }

    #[test]
    fn let_binding_named_prelude_constructor_compiles() {
        // A `let` bound to a prelude-constructor name must be escaped, not crash rustc.
        let src = "fn main() { let None = 5; print(\"ok\"); }";
        assert_eq!(run(src), "ok");
    }

    #[test]
    fn enum_struct_variant_shorthand() {
        // `E::V { a, b }` shorthand binds each field to a variable of the same name.
        let src = "enum E { Add { l: int, r: int }, Zero } \
                   fn ev(e) { return match e { E::Add { l, r } => l + r, E::Zero => 0 }; } \
                   fn main() { print(ev(E::Add { l: 3, r: 4 })); print(ev(E::Zero)); }";
        assert_eq!(run(src), "7\n0");
    }

    #[test]
    fn enum_struct_variant_field_subpatterns() {
        // A struct-variant field may hold any sub-pattern (literal, nested), like a plain struct.
        let src = "enum Shape { Circle { r: int }, Rect { w: int, h: int } } \
                   fn f(s) { return match s { Shape::Circle { r: 0 } => \"point\", \
                     Shape::Circle { r } => \"circle \" + str(r), \
                     Shape::Rect { w, h } => \"rect \" + str(w * h) }; } \
                   fn main() { print(f(Shape::Circle { r: 0 })); print(f(Shape::Circle { r: 5 })); \
                     print(f(Shape::Rect { w: 3, h: 4 })); }";
        assert_eq!(run(src), "point\ncircle 5\nrect 12");
    }

    #[test]
    fn list_membership_is_structural() {
        // contains/index_of honor `==` (type-exact), not display form.
        let src =
            "fn main() { print(contains([1, 2, 3], \"2\")); print(index_of([1, 2, 3], \"2\")); \
                   print(contains([1, 2, 3], 2)); print(index_of([1, 2, 3], 3)); \
                   print(contains([\"a\", \"b\"], \"b\")); }";
        assert_eq!(run(src), "false\n-1\ntrue\n2\ntrue");
    }

    #[test]
    fn ordering_compares_lists_elementwise() {
        // sort_by/min_by/max_by with list (tuple) keys order element-wise, not by display string.
        let src = "fn main() { print(sort_by([9, 100, 25, 8], |x| [x])); \
                   print(min_by([[9], [100], [25]], |p| p)); print(max_by([[9], [100], [25]], |p| p)); \
                   print(map(sort_by([[\"a\", 90], [\"a\", 100], [\"a\", 85]], |r| r), |r| r[1])); }";
        assert_eq!(run(src), "[8, 9, 25, 100]\n[9]\n[100]\n[85, 90, 100]");
    }

    #[test]
    fn patterns_nest_fully() {
        // A sub-pattern (struct, list, literal) may appear inside an enum payload.
        let src = "struct P { x: int, y: int } fn mk(a, b) { P { x: a, y: b } } \
                   fn f(o) { return match o { Some(P { x: 0, y }) => y, Some(P { x, y }) => x + y, None => -1 }; } \
                   fn g(r) { return match r { Ok([a, b]) => a * b, Ok(xs) => 0, Err(e) => 0 - e }; } \
                   fn main() { print(f(Some(mk(0, 5)))); print(f(Some(mk(3, 4)))); print(f(None)); \
                     print(g(Ok([6, 7]))); print(g(Err(2))); }";
        assert_eq!(run(src), "5\n7\n-1\n42\n-2");
    }

    #[test]
    fn question_operator_respects_enum_type() {
        // `?` only acts on built-in Option/Result; a user enum with an `Ok`/`None` variant
        // is a distinct type and passes through unchanged.
        let src = "enum W { Ok(u32), Bad } fn probe(w) { let x = w?; x } \
                   fn main() { print(probe(W::Ok(7))); }";
        assert_eq!(run(src), "W::Ok(7)");
    }

    #[test]
    fn if_let_as_expression() {
        // if-let yields a value: as a function tail and as a let-initializer.
        let src = "fn label(o) { if let Some(v) = o { v } else { -1 } } \
                   fn main() { print(label(Some(5))); print(label(None)); \
                     let r = if let Some(v) = Some(3) { v + 1 } else { 0 }; print(r); }";
        assert_eq!(run(src), "5\n-1\n4");
    }

    #[test]
    fn if_let_while_let_or_patterns() {
        let src = "fn main() { let mut n = 1; while let 1 | 2 | 3 = n { print(n); n = n + 1; } \
                     if let 4 | 5 = n { print(\"reached\") } else { print(\"no\") } }";
        assert_eq!(run(src), "1\n2\n3\nreached");
    }

    #[test]
    fn typecheck_rejects_non_exhaustive_option_match() {
        let ast =
            crate::frontend::parse_source("fn f(o) { match o { Some(v) => v } } fn main() { }")
                .unwrap();
        let err = crate::typecheck(ast, crate::frontend::Mode::Safe).unwrap_err();
        assert!(err.contains("ANUBIS_MATCH_NON_EXHAUSTIVE"), "{}", err);
    }

    #[test]
    fn string_interpolation_basic() {
        let src = "fn main() { let name = \"Anubis\"; let age = 3; \
                   print(\"hi ${name}, age ${age}\"); print(\"sum=${2 + 3 * 4}\"); \
                   print(\"${name}!\"); print(\"no interp\"); }";
        assert_eq!(run(src), "hi Anubis, age 3\nsum=14\nAnubis!\nno interp");
    }

    #[test]
    fn string_interpolation_nested_and_calls() {
        // Nested strings, calls, field access, and list display inside ${...}.
        let src =
            "fn dbl(n) { n * 2 } struct P { x: int, y: int } fn mk(a, b) { P { x: a, y: b } } \
                   fn main() { let p = mk(3, 4); let xs = [1, 2]; \
                     print(\"pt (${p.x}, ${p.y}) d=${dbl(p.x)}\"); \
                     print(\"xs=${xs} pick=${if p.x > 2 { \"big\" } else { \"small\" }}\"); }";
        assert_eq!(run(src), "pt (3, 4) d=6\nxs=[1, 2] pick=big");
    }

    #[test]
    fn dollar_without_brace_is_literal() {
        let src = "fn main() { print(\"cost is $5 total\"); }";
        assert_eq!(run(src), "cost is $5 total");
    }

    #[test]
    fn interpolation_with_escaped_quotes() {
        // Quotes inside ${...} are written escaped (they're inside the outer string); they must
        // be unescaped, and a `}` inside such a nested string must not close the interpolation.
        let src = r#"fn main() { print("call ${upper(\"hi\")}"); print("brace ${\"a}b\"}"); }"#;
        assert_eq!(run(src), "call HI\nbrace a}b");
    }

    #[test]
    fn struct_pattern_in_match() {
        // Field sub-patterns: literals constrain, identifiers bind, shorthand binds field->var.
        let src = "struct P { x: int, y: int } fn mk(a, b) { P { x: a, y: b } } \
                   fn quad(p) { return match p { \
                     P { x: 0, y: 0 } => \"origin\", P { x: 0, y } => \"y-axis\", \
                     P { x, y: 0 } => \"x-axis\", P { x, y } => \"other\" }; } \
                   fn main() { print(quad(mk(0,0))); print(quad(mk(0,5))); \
                     print(quad(mk(3,0))); print(quad(mk(3,4))); }";
        assert_eq!(run(src), "origin\ny-axis\nx-axis\nother");
    }

    #[test]
    fn struct_pattern_let_and_if_let() {
        let src = "struct P { x: int, y: int } fn mk(a, b) { P { x: a, y: b } } \
                   fn main() { \
                     let P { x, y } = mk(7, 9); print(x + y); \
                     let P { x: a, y: b } = mk(2, 3); print(a * b); \
                     if let P { x, y } = mk(1, 2) { print(x * 10 + y); } }";
        assert_eq!(run(src), "16\n6\n12");
    }

    #[test]
    fn let_binds_a_parameter() {
        // Regression: `let s = param` must not report the parameter as unknown.
        let src = "fn f(xs) { let s = xs; s[0] + 1 } fn main() { print(f([41, 9])); }";
        assert_eq!(run(src), "42");
    }

    #[test]
    fn stdlib_math_extras() {
        let src = "fn main() { print(clamp(15, 0, 10)); print(sign(-7)); print(factorial(5)); \
                   print(round(log(8.0, 2.0))); print(floor(exp(1.0))); print(trunc(3.9)); }";
        assert_eq!(run(src), "10\n-1\n120\n3\n2\n3");
    }

    #[test]
    fn stdlib_string_extras() {
        let src = "fn main() { print(chars(\"abc\")); print(words(\"a b  c\")); \
                   print(capitalize(\"hELLO\")); print(pad_start(\"7\", 3, \"0\")); print(pad_end(\"7\", 3, \".\")); }";
        assert_eq!(run(src), "[a, b, c]\n[a, b, c]\nHello\n007\n7..");
    }

    #[test]
    fn stdlib_list_extras() {
        let src = "fn main() { \
                   print(zip([1,2],[\"a\",\"b\"])); print(enumerate([\"x\",\"y\"])); \
                   print(flatten([[1,2],[3]])); print(unique([1,1,2,3,3])); \
                   print(take([1,2,3,4],2)); print(drop([1,2,3,4],2)); \
                   print(take_while([1,2,9,1],|x| x<5)); print(chunk([1,2,3,4,5],2)); \
                   print(window([1,2,3],2)); print(position([5,6,7],|x| x==6)); \
                   print(product([1,2,3,4])); print(first([9,8])); print(last([9,8])); \
                   print(concat([1],[2,3])); print(min_by([[1,5],[2,1]],|p| p[1])); \
                   print(partition([1,2,3,4],|x| x%2==0)); print(flat_map([1,2],|x| [x,x])); }";
        assert_eq!(
            run(src),
            "[[1, a], [2, b]]\n[[0, x], [1, y]]\n[1, 2, 3]\n[1, 2, 3]\n[1, 2]\n[3, 4]\n[1, 2]\n[[1, 2], [3, 4], [5]]\n[[1, 2], [2, 3]]\n1\n24\n9\n8\n[1, 2, 3]\n[2, 1]\n[[2, 4], [1, 3]]\n[1, 1, 2, 2]"
        );
    }

    #[test]
    fn unique_uses_structural_equality() {
        // `unique` dedups by `==` (structural/type-exact), not display form.
        let src = "fn main() { print(len(unique([1, \"1\"]))); print(unique([1, 1.0])); \
                   print(unique([2, \"2\", 2, 2.0])); }";
        assert_eq!(run(src), "2\n[1]\n[2, 2]");
    }

    #[test]
    fn rounding_preserves_large_ints() {
        // floor/ceil/round/trunc are the identity on an int (no f64 precision loss above 2^53).
        let src = "fn main() { let n = 9007199254740993; \
                   print(trunc(n)); print(floor(n)); print(ceil(n)); print(round(n)); }";
        assert_eq!(
            run(src),
            "9007199254740993\n9007199254740993\n9007199254740993\n9007199254740993"
        );
    }

    #[test]
    fn local_shadows_builtin_when_called() {
        // A parameter or let-binding named like a builtin, when called, is the local closure.
        let src = "fn use_it(map, x) { return map(x); } \
                   fn main() { print(use_it(|v| v * 3, 10)); \
                     let first = |xs| xs[0] * 10; print(first([4, 5])); \
                     print(first([9, 8])); }"; // last: builtin `first` (no local in main? there is: shadowed)
                                               // In main, `first` is a local closure, so both `first(...)` calls use it.
        assert_eq!(run(src), "30\n40\n90");
    }

    #[test]
    fn stdlib_map_and_functional_extras() {
        let src = "fn main() { let m = { \"a\": 1, \"b\": 2 }; \
                   print(entries(m)); print(get(m, \"b\", 0)); print(get(m, \"z\", -1)); \
                   print(map_values(m, |v| v * 10)[\"a\"]); print(merge(m, { \"b\": 9, \"c\": 3 })[\"b\"]); \
                   let f = compose(|x| x + 1, |x| x * 2); print(f(5)); \
                   print(times(3, |i| i * i)); print(identity(42)); }";
        assert_eq!(
            run(src),
            "[[a, 1], [b, 2]]\n2\n-1\n10\n9\n11\n[0, 1, 4]\n42"
        );
    }

    #[test]
    fn min_max_exact_above_f64_precision() {
        // 2^53 and 2^53+1 are distinct i64s but collapse in f64; min/max must stay exact.
        let src = "fn main() { let a = 9007199254740993; let b = 9007199254740992; \
                   print(min(a, b)); print(max(b, a)); print(sort([a, b])); }";
        assert_eq!(
            run(src),
            "9007199254740992\n9007199254740993\n[9007199254740992, 9007199254740993]"
        );
    }

    #[test]
    fn variadic_builtin_as_first_class_value() {
        // min/max as values must forward every argument: reduce passes (acc, x), apply spreads.
        let src =
            "fn main() { print(reduce([5, 1, 9, 3], max, 0)); print(apply(min, [5, 1, 9, 3])); \
                   print(map([[3, 1], [9, 2], [4, 8]], max)); }";
        assert_eq!(run(src), "9\n1\n[3, 9, 8]");
    }

    #[test]
    fn multi_arity_builtin_as_first_class_value() {
        // range accepts 2 or 3 args; a first-class reference dispatches on the actual count.
        let src =
            "fn main() { let r = range; print(apply(r, [1, 5])); print(apply(r, [0, 10, 3])); }";
        assert_eq!(run(src), "[1, 2, 3, 4]\n[0, 3, 6, 9]");
    }

    #[test]
    fn unmatched_match_fails_closed() {
        // A match with no matching arm and no `_` must trap, not fabricate a value.
        let src = "fn eval(cmd) { match cmd { [\"add\", a, b] => a + b } } \
                   fn main() { print(eval([\"add\", 3, 4])); print(eval([\"sub\", 1, 2])); }";
        let out = compile_and_run_source(src, false, &[]).expect("compile+run");
        assert!(!out.status.success(), "unmatched match must exit nonzero");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("ANUBIS_MATCH_UNMATCHED"),
            "stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "7");
    }

    #[test]
    fn interpolation_nested_string_escapes_verbatim() {
        // Bare-delimited nested strings inside ${...}: escaped quotes and backslashes are
        // content, resolved exactly once (by the fragment re-lex, not the outer lexer too).
        let src = r#"fn main() { print("say ${"he said \"hi\""}"); let d = "C:"; print("path=${d + "\\" + "dir"}"); }"#;
        assert_eq!(run(src), "say he said \"hi\"\npath=C:\\dir");
    }

    #[test]
    fn braceless_control_flow_match_arm_bodies() {
        // `=> return v`, `=> break`, `=> continue` without braces parse and bind to the
        // enclosing function/loop.
        let src = "fn pick(i) { let v = match i { 3 => return 999, n => n * n }; v + 1 } \
                   fn main() { print(pick(2)); print(pick(3)); \
                     let out = []; let mut i = 0; \
                     while i < 12 { i = i + 1; \
                       let v = match i { n if n % 4 == 0 => continue, n if n == 11 => break, n => n * n }; \
                       push(out, v); } \
                     print(out); }";
        assert_eq!(run(src), "5\n999\n[1, 4, 9, 25, 36, 49, 81, 100]");
    }

    #[test]
    fn print_and_len_as_first_class_values() {
        let src = "fn main() { each([1, 2], print); print(map([[1, 2, 3], [4]], len)); }";
        assert_eq!(run(src), "1\n2\n[3, 1]");
    }

    #[test]
    fn break_inside_match_arm_binds_to_enclosing_loop() {
        let src = "fn main() { let mut total = 0; \
                   for x in [1, 2, 3, 4, 5] { match x { 4 => { break } _ => {} } total = total + x; } \
                   print(total); }";
        assert_eq!(run(src), "6");
    }

    #[test]
    fn continue_inside_match_arm_binds_to_enclosing_loop() {
        // The match must not desugar to a Rust `loop`, or `continue` would spin forever.
        let src = "fn main() { let mut out = []; \
                   for x in [1, 2, 3, 4, 5, 6] { match x { n if n % 2 == 0 => { continue } _ => {} } push(out, x); } \
                   print(out); }";
        assert_eq!(run(src), "[1, 3, 5]");
    }

    #[test]
    fn break_inside_match_arm_within_while() {
        let src = "fn main() { let mut i = 0; let mut sum = 0; \
                   while i < 100 { i = i + 1; match i { n if n > 5 => { break } _ => {} } sum = sum + i; } \
                   print(sum); }";
        assert_eq!(run(src), "15");
    }

    #[test]
    fn nested_matches_do_not_collide() {
        let src = "fn classify(x, y) { match x { \
                     0 => match y { 0 => \"origin\", _ => \"y-axis\" }, \
                     _ => match y { 0 => \"x-axis\", _ => \"quadrant\" } } } \
                   fn main() { print(classify(0,0)); print(classify(0,5)); print(classify(3,0)); print(classify(3,4)); }";
        assert_eq!(run(src), "origin\ny-axis\nx-axis\nquadrant");
    }

    #[test]
    fn named_function_is_first_class_value() {
        let src = "fn double(x) { x * 2 } fn is_odd(x) { x % 2 == 1 } fn add(a, b) { a + b } \
                   fn main() { print(map([1, 2, 3, 4], double)); print(filter([1, 2, 3, 4, 5], is_odd)); \
                     print(sort_by([3, 1, 2], double)); print(reduce([1, 2, 3, 4], add, 0)); }";
        assert_eq!(run(src), "[2, 4, 6, 8]\n[1, 3, 5]\n[1, 2, 3]\n10");
    }

    #[test]
    fn builtin_is_first_class_value() {
        let src = "fn main() { let g = compose(|x| x + 1, identity); print(g(41)); }";
        assert_eq!(run(src), "42");
    }

    #[test]
    fn named_functions_in_a_list_are_callable() {
        let src = "fn sq(x) { x * x } \
                   fn main() { let fns = [sq, |x| x + 100]; print(fns[0](5)); print(fns[1](5)); }";
        assert_eq!(run(src), "25\n105");
    }

    #[test]
    fn unknown_name_as_value_is_a_clean_error() {
        // Referencing an undefined name in value position yields an Anubis diagnostic, not a
        // leaked rustc "cannot find value" error.
        let src = "fn main() { print(nonexistent_thing); }";
        let ast = crate::frontend::parse_source(src).expect("parse");
        let err = lower_program_to_rust(&ast.items, false)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(err.contains("unknown name"), "got: {err}");
    }

    #[test]
    fn where_clause_on_all_item_forms() {
        let src = "struct Box<T> where T: Ord { value: T } \
                   enum Opt<T> where T: Ord { Has(T), Empty } \
                   trait D<X> where X: Ord { fn val(self); fn describe(self) { \"v=\" + str(self.val()) } } \
                   struct W { inner: int } impl D<int> for W where int: Ord { fn val(self) { self.inner } } \
                   fn id<T>(x: T) -> T where T: Ord { x } \
                   fn main() { print((Box { value: 5 }).value); print((W { inner: 7 }).describe()); print(id(9)); }";
        assert_eq!(run(src), "5\nv=7\n9");
    }

    #[test]
    fn closure_mutating_captured_var_compiles() {
        // Closures capture by value: mutating a captured binding compiles and mutates the closure's
        // own copy (the outer binding is unchanged), rather than failing to compile.
        let src = "fn main() { let mut count = 0; let inc = || { count = count + 1; count }; \
                   print(inc()); print(inc()); print(count); \
                   let acc = []; each([1, 2, 3], |x| push(acc, x)); print(\"ok\"); }";
        assert_eq!(run(src), "1\n1\n0\nok");
    }

    #[test]
    fn or_pattern_with_bindings() {
        // Alternatives of an or-pattern may bind the same variable (desugared to one arm each).
        let src = "fn combine(a, b) { return match (a, b) { \
                     (Some(x), Some(y)) => Some(x + y), \
                     (Some(x), None) | (None, Some(x)) => Some(x), \
                     (None, None) => None }; } \
                   fn show(o) { return match o { Some(v) => v, None => -1 }; } \
                   fn main() { print(show(combine(Some(3), Some(4)))); print(show(combine(Some(3), None))); \
                     print(show(combine(None, Some(9)))); print(show(combine(None, None))); }";
        assert_eq!(run(src), "7\n3\n9\n-1");
    }

    #[test]
    fn local_closure_named_like_builtin_captured_in_lambda() {
        let src = "fn main() { let f = |x| x + 1; let map = |g, xs| g(xs[0]); \
                   let lam = || map(f, [10, 20, 30]); print(lam()); }";
        assert_eq!(run(src), "11");
    }

    #[test]
    fn generic_syntax_is_accepted_and_erased() {
        // Generic parameters, bounds, `where`, and generic types on struct/enum/impl/trait all
        // parse and run (types are erased at runtime).
        let src = "fn pick<T>(a: T, b: T, first: bool) -> T where T: Ord { if first { a } else { b } } \
                   struct Box<T> { val: T } impl<T> Box<T> { fn get(self) { self.val } } \
                   enum Opt<T> { Has(T), Empty } \
                   fn unwrap<T>(o: Opt<T>, d: T) -> T { return match o { Opt::Has(v) => v, Opt::Empty => d }; } \
                   fn main() { print(pick(3, 7, true)); print(pick(\"a\", \"b\", false)); \
                     print((Box { val: 42 }).get()); \
                     print(unwrap(Opt::Has(5), 0)); print(unwrap(Opt::Empty, -1)); }";
        assert_eq!(run(src), "3\nb\n42\n5\n-1");
    }

    #[test]
    fn generic_trait_with_nested_params() {
        let src = "trait Seq<T> { fn head(self); fn empty(self) { false } } \
                   struct Stack<T> { items: list } \
                   impl<T> Seq<T> for Stack<T> { fn head(self) { self.items[0] } } \
                   fn wrap<T>(x: T) -> Box<Box<T>> { x } struct Box<T> { v: T } \
                   fn main() { print((Stack { items: [10, 20] }).head()); \
                     print((Stack { items: [1] }).empty()); print(wrap(99)); }";
        assert_eq!(run(src), "10\nfalse\n99");
    }

    #[test]
    fn traits_default_methods_and_overrides() {
        let src = "trait Describable { fn name(self); fn describe(self) { \"a \" + self.name() } \
                     fn shout(self) { upper(self.describe()) } } \
                   struct Dog { } impl Describable for Dog { fn name(self) { \"dog\" } } \
                   struct Cat { } impl Describable for Cat { fn name(self) { \"cat\" } \
                     fn describe(self) { \"the mighty \" + self.name() } } \
                   fn main() { print((Dog {}).describe()); print((Dog {}).shout()); \
                     print((Cat {}).describe()); print((Cat {}).shout()); }";
        assert_eq!(run(src), "a dog\nA DOG\nthe mighty cat\nTHE MIGHTY CAT");
    }

    #[test]
    fn method_name_does_not_shadow_builtin_freecall() {
        // A method named like a builtin must not break a free call to that builtin.
        let src =
            "struct Bag { items: list } impl Bag { fn count(self) { len(self.items) + 100 } } \
                   fn main() { let b = Bag { items: [1, 2, 3] }; print(b.count()); \
                     print(count([1, 2, 3], |x| x > 1)); }";
        assert_eq!(run(src), "103\n2");
    }

    #[test]
    fn inherent_method_beats_trait_default() {
        // An inherent method and two traits sharing a default must not emit duplicate functions.
        let src = "trait T { fn go(self) { \"trait\" } fn req(self) { 0 } } \
                   struct X { } impl X { fn go(self) { \"inherent\" } } impl T for X { } \
                   trait A { fn kind(self) { \"a\" } } trait B { fn kind(self) { \"b\" } } \
                   struct Y { } impl A for Y { } impl B for Y { } \
                   fn main() { print((X {}).go()); print((X {}).req()); print((Y {}).kind()); }";
        assert_eq!(run(src), "inherent\n0\na");
    }

    #[test]
    fn bare_if_statement_in_match_arm_block() {
        // A no-else `if` guard inside a match-arm block body parses as a statement.
        let src = "fn f(n) { return match n { _ => { if n < 0 { return 111 } 222 } }; } \
                   fn main() { print(f(-1)); print(f(5)); }";
        assert_eq!(run(src), "111\n222");
    }

    #[test]
    fn typecheck_rejects_non_exhaustive_user_enum() {
        let ast = crate::frontend::parse_source(
            "enum E { A, B, C } fn f(x) { match x { E::A => 1, E::B => 2 } } fn main() { }",
        )
        .unwrap();
        let err = crate::typecheck(ast, crate::frontend::Mode::Safe).unwrap_err();
        assert!(err.contains("ANUBIS_MATCH_NON_EXHAUSTIVE"), "{}", err);
    }

    #[test]
    fn hofs_accept_strings_and_maps() {
        // The closure-taking list HOFs also iterate a string's chars or a map's keys.
        let src = "fn main() { print(map(\"abc\", |c| upper(c))); print(filter(\"hello\", |c| c != \"l\")); \
                   print(any(\"abc\", |c| c == \"b\")); print(count(\"banana\", |c| c == \"a\")); \
                   print(sort(map({ \"a\": 1, \"b\": 2 }, |k| upper(k)))); }";
        assert_eq!(run(src), "[A, B, C]\n[h, e, o]\ntrue\n3\n[A, B]");
    }

    #[test]
    fn traits_polymorphism_and_enums() {
        // A trait implemented by structs and enums; heterogeneous list dispatches per element.
        let src = "trait Shape { fn area(self); fn twice(self) { self.area() * 2 } } \
                   struct Sq { s: int } impl Shape for Sq { fn area(self) { self.s * self.s } } \
                   enum Circle { R(int) } impl Shape for Circle { fn area(self) { return match self { Circle::R(r) => 3*r*r }; } } \
                   fn main() { print((Sq { s: 4 }).twice()); print(Circle::R(10).area()); \
                     print(map([Sq { s: 2 }, Sq { s: 3 }], |x| x.area())); }";
        assert_eq!(run(src), "32\n300\n[4, 9]");
    }

    #[test]
    fn methods_dispatch_on_receiver_type() {
        let src = "struct Point { x: int, y: int } \
                   impl Point { fn dist2(self) { self.x * self.x + self.y * self.y } \
                                fn translate(self, dx, dy) { Point { x: self.x + dx, y: self.y + dy } } } \
                   struct Circle { r: int } \
                   impl Circle { fn area(self) { 3 * self.r * self.r } } \
                   fn main() { let p = Point { x: 3, y: 4 }; print(p.dist2()); \
                     print(p.translate(1, 1).dist2()); print((Circle { r: 10 }).area()); }";
        assert_eq!(run(src), "25\n41\n300");
    }

    #[test]
    fn methods_on_enum_and_self_calls() {
        let src = "enum Shape { Circle(int), Square(int) } \
                   impl Shape { fn area(self) { return match self { Shape::Circle(r) => 3*r*r, Shape::Square(s) => s*s }; } \
                                fn twice(self) { self.area() + self.area() } } \
                   fn main() { print(Shape::Circle(10).area()); print(Shape::Square(5).twice()); }";
        assert_eq!(run(src), "300\n50");
    }

    #[test]
    fn method_on_non_matching_receiver_is_zero() {
        // Calling a method on a value with no such method (e.g. an int) yields 0, not a crash.
        let src = "struct P { x: int } impl P { fn area(self) { self.x } } \
                   fn main() { let n = 5; print(n.area()); }";
        assert_eq!(run(src), "0");
    }

    #[test]
    fn methods_same_name_different_arity() {
        // Two types share a method name with different arities — each dispatch arm must be
        // emitted with that type's own argument count.
        let src = "struct S { v: int } struct C { v: int } \
                   impl S { fn tag(self) { 1 } } impl C { fn tag(self, e) { e } } \
                   fn main() { print((S { v: 0 }).tag()); print((C { v: 0 }).tag(9)); }";
        assert_eq!(run(src), "1\n9");
    }

    #[test]
    fn field_closure_not_hijacked_by_method_name() {
        // `h.f()` on a closure-valued field keeps working even when an unrelated type has a `f` method.
        let src = "struct Holder { f: int } struct Other { z: int } \
                   impl Other { fn f(self) { 99 } } \
                   fn main() { let h = Holder { f: || 42 }; print(h.f()); print((Other { z: 0 }).f()); }";
        assert_eq!(run(src), "42\n99");
    }

    #[test]
    fn bare_trailing_map_literal_returns() {
        // A `{ k: v }` map literal as a function/method tail is a value, not a mis-parsed block.
        let src = "fn cfg() { { \"x\": 10, \"y\": 20 } } fn main() { print(cfg()[\"y\"]); }";
        assert_eq!(run(src), "20");
    }

    #[test]
    fn let_without_semicolon_is_valid() {
        // Trailing `;` is optional on every statement kind, including `let`.
        let src = "fn main() {\n  let a = 5\n  let b = a + 1\n  print(b)\n}";
        assert_eq!(run(src), "6");
    }

    #[test]
    fn struct_construct_read_mutate() {
        let src = "struct P { x: u32, y: u32 } \
                   fn main() { let p = P { x: 3, y: 4 }; p.x = p.x + 10; print(p.x + p.y); }";
        assert_eq!(run(src), "17");
    }

    #[test]
    fn struct_nested_place_assignment() {
        // struct-in-list and list-in-struct nested lvalues
        let src = "struct Bag { items: u32 } \
                   fn main() { \
                     let bags = [ Bag { items: 1 }, Bag { items: 2 } ]; \
                     bags[0].items = 40; \
                     bags[1].items = bags[1].items + 5; \
                     print(bags[0].items + bags[1].items); \
                   }";
        assert_eq!(run(src), "47");
    }

    #[test]
    fn struct_in_map_and_deep_path() {
        let src = "struct Cell { v: u32 } \
                   fn main() { \
                     let grid = { \"a\": Cell { v: 1 } }; \
                     grid[\"a\"].v = 99; \
                     print(grid[\"a\"].v); \
                   }";
        assert_eq!(run(src), "99");
    }

    #[test]
    fn negative_indexing() {
        let src = "fn main() { let a = [10, 20, 30]; print(a[-1] + a[-2]); }";
        assert_eq!(run(src), "50");
    }

    #[test]
    fn float_arithmetic_and_promotion() {
        assert_eq!(run("fn main() { print(3.5 * 2.0); }"), "7.0");
        assert_eq!(run("fn main() { print(1 + 2.5); }"), "3.5");
        assert_eq!(run("fn main() { print(7.0 / 2.0); }"), "3.5");
        assert_eq!(run("fn main() { print(-2.5 + 1.0); }"), "-1.5");
        assert_eq!(run("fn main() { print(1.5e3); }"), "1500.0");
    }

    #[test]
    fn integer_division_stays_integer() {
        assert_eq!(run("fn main() { print(7 / 2); }"), "3");
        assert_eq!(run("fn main() { print(7 % 3); }"), "1");
    }

    #[test]
    fn numeric_string_stays_string() {
        // Regression: "3" must be a string, not the integer 3 (len is 1, not the value).
        assert_eq!(run("fn main() { let s = \"3\"; print(len(s)); }"), "1");
        assert_eq!(run("fn main() { print(\"007\"); }"), "007");
    }

    #[test]
    fn string_escapes() {
        // \n, \t, \" decode; "a\nb" has length 3.
        assert_eq!(run("fn main() { print(len(\"a\\nb\")); }"), "3");
        assert_eq!(run("fn main() { print(\"q\\\"x\"); }"), "q\"x");
    }

    #[test]
    fn char_literal_is_one_char_string() {
        assert_eq!(run("fn main() { let c = 'A'; print(c); }"), "A");
        assert_eq!(run("fn main() { print(len('z')); }"), "1");
    }

    #[test]
    fn block_comments_nested() {
        let src = "fn main() { /* outer /* inner */ still comment */ print(42); }";
        assert_eq!(run(src), "42");
    }

    #[test]
    fn numeric_base_prefixes() {
        assert_eq!(run("fn main() { print(0xFF + 0b1010 + 0o17); }"), "280");
    }

    #[test]
    fn ranges_still_lex_after_float_support() {
        let src = "fn main() { let s = 0; for i in 0..5 { s = s + i; } print(s); }";
        assert_eq!(run(src), "10");
    }

    #[test]
    fn compound_assignment_scalar() {
        let src = "fn main() { let x = 10; x += 5; x -= 3; x *= 2; x /= 4; print(x); }";
        assert_eq!(run(src), "6"); // ((10+5-3)*2)/4 = 6
    }

    #[test]
    fn compound_assignment_on_place() {
        let src = "fn main() { let a = [1, 2, 3]; a[1] += 10; a[1] *= 2; print(a[1]); }";
        assert_eq!(run(src), "24"); // (2+10)*2
    }

    #[test]
    fn compound_assignment_on_field() {
        let src = "struct C { v: u32 } fn main() { let c = C { v: 5 }; c.v += 100; print(c.v); }";
        assert_eq!(run(src), "105");
    }

    #[test]
    fn bitwise_operators() {
        // (6&3)=2, (6|1)=7, (5^1)=4, (1<<4)=16, (256>>2)=64 -> 93
        let src = "fn main() { print((6 & 3) + (6 | 1) + (5 ^ 1) + (1 << 4) + (256 >> 2)); }";
        assert_eq!(run(src), "93");
    }

    #[test]
    fn bitwise_not_and_precedence() {
        assert_eq!(run("fn main() { print(~0); }"), "-1");
        // shift binds looser than +, tighter than |: (1+2)<<1 = 6
        assert_eq!(run("fn main() { print(1 + 2 << 1); }"), "6");
        // '|' looser than '*': (2*3)|1 = 7
        assert_eq!(run("fn main() { print(2 * 3 | 1); }"), "7");
    }

    #[test]
    fn untyped_params_and_return_type() {
        // Parameters without `: T` and a declared `-> u32` both parse and run.
        let src = "fn add(a, b) -> u32 { return a + b; } fn main() { print(add(40, 2)); }";
        assert_eq!(run(src), "42");
    }

    #[test]
    fn block_expression_with_statements() {
        // if-expression branch with local statements before its trailing value.
        let src =
            "fn main() { let r = if 3 > 2 { let a = 10; let b = 5; a + b } else { 0 }; print(r); }";
        assert_eq!(run(src), "15");
    }

    #[test]
    fn module_scoped_struct_and_fn() {
        let src = "module m { struct P { v: u32 } fn mk() { return P { v: 7 }; } } \
                   fn main() { let p = mk(); print(p.v); }";
        assert_eq!(run(src), "7");
    }

    #[test]
    fn stdlib_strings() {
        assert_eq!(run("fn main() { print(upper(\"abc\")); }"), "ABC");
        assert_eq!(run("fn main() { print(lower(\"ABC\")); }"), "abc");
        assert_eq!(run("fn main() { print(trim(\"  hi  \")); }"), "hi");
        assert_eq!(
            run("fn main() { print(len(split(\"a,b,c\", \",\"))); }"),
            "3"
        );
        assert_eq!(
            run("fn main() { print(join([\"a\", \"b\", \"c\"], \"-\")); }"),
            "a-b-c"
        );
        assert_eq!(
            run("fn main() { print(replace(\"aXbXc\", \"X\", \"_\")); }"),
            "a_b_c"
        );
        assert_eq!(
            run("fn main() { print(contains(\"hello\", \"ell\")); }"),
            "true"
        );
        assert_eq!(run("fn main() { print(index_of(\"hello\", \"l\")); }"), "2");
        assert_eq!(run("fn main() { print(substr(\"hello\", 1, 3)); }"), "ell");
        assert_eq!(run("fn main() { print(repeat(\"ab\", 3)); }"), "ababab");
        assert_eq!(run("fn main() { print(ord(\"A\")); }"), "65");
        assert_eq!(run("fn main() { print(chr(66)); }"), "B");
        assert_eq!(run("fn main() { print(parse_int(\"42\") + 1); }"), "43");
    }

    #[test]
    fn stdlib_math() {
        assert_eq!(run("fn main() { print(abs(-7)); }"), "7");
        assert_eq!(run("fn main() { print(pow(2, 10)); }"), "1024");
        assert_eq!(run("fn main() { print(gcd(48, 36)); }"), "12");
        assert_eq!(run("fn main() { print(min(3, 1, 2)); }"), "1");
        assert_eq!(run("fn main() { print(max([4, 9, 2])); }"), "9");
        assert_eq!(run("fn main() { print(sqrt(9.0)); }"), "3.0");
        assert_eq!(run("fn main() { print(floor(3.7)); }"), "3");
        assert_eq!(run("fn main() { print(ceil(3.2)); }"), "4");
    }

    #[test]
    fn stdlib_lists() {
        assert_eq!(run("fn main() { print(sum([1, 2, 3, 4])); }"), "10");
        assert_eq!(run("fn main() { print(reverse([1, 2, 3])); }"), "[3, 2, 1]");
        assert_eq!(run("fn main() { print(sort([3, 1, 2])); }"), "[1, 2, 3]");
        assert_eq!(run("fn main() { print(range(0, 5)); }"), "[0, 1, 2, 3, 4]");
        assert_eq!(
            run("fn main() { print(range(0, 10, 2)); }"),
            "[0, 2, 4, 6, 8]"
        );
        assert_eq!(
            run("fn main() { print(slice([1, 2, 3, 4, 5], 1, 4)); }"),
            "[2, 3, 4]"
        );
        assert_eq!(
            run("fn main() { let a = [1, 2, 3]; let x = pop(a); print(x + len(a)); }"),
            "5"
        );
        assert_eq!(
            run("fn main() { let a = [1, 2, 3]; insert(a, 1, 9); print(a); }"),
            "[1, 9, 2, 3]"
        );
        assert_eq!(
            run("fn main() { let a = [1, 2, 3]; let r = remove(a, 1); print(r); print(a); }"),
            "2\n[1, 3]"
        );
    }

    #[test]
    fn display_forms_option_result_map_and_user_enum() {
        // Built-in Option/Result variants render bare (as they are constructed and matched).
        assert_eq!(
            run("fn main() { print(Some(8)); print(None); print(Ok(1)); print(Err(2)); }"),
            "Some(8)\nNone\nOk(1)\nErr(2)"
        );
        // Maps render with quoted keys, matching the literal syntax you'd write.
        assert_eq!(
            run("fn main() { print({ \"a\": 1, \"b\": 2 }); }"),
            "{\"a\": 1, \"b\": 2}"
        );
        // User-defined enums still render as `Type::Variant`.
        assert_eq!(
            run("enum S { A, B(u32) } fn main() { print(S::A); print(S::B(3)); }"),
            "S::A\nS::B(3)"
        );
        // Nested: an Option holding a map with a list value.
        assert_eq!(
            run("fn main() { print(Some({ \"k\": [1, 2] })); }"),
            "Some({\"k\": [1, 2]})"
        );
    }

    #[test]
    fn cast_binds_tighter_than_binary_ops() {
        // `as` used to swallow the following operator+operand into the "type" and void the cast.
        assert_eq!(run("fn main(){ print(300 as u8 + 1); }"), "45");
        assert_eq!(run("fn main(){ print(10 as i64 * 5); }"), "50");
        assert_eq!(
            run("fn main(){ print(2 as f64 / 3.0); }"),
            "0.6666666666666666"
        );
        assert_eq!(run("fn main(){ print(10 as i64 == 10); }"), "true");
        assert_eq!(
            run("fn main(){ if 7 as u8 == 7 { print(\"y\"); } else { print(\"n\"); } }"),
            "y"
        );
        assert_eq!(run("fn main(){ print(300 as u8 as i64 + 1); }"), "45");
    }

    #[test]
    fn struct_equality_is_field_order_independent() {
        assert_eq!(
            run("struct P { x: int, y: int } fn main(){ print(P { x: 3, y: 4 } == P { y: 4, x: 3 }); }"),
            "true"
        );
        assert_eq!(
            run("struct P { x: int, y: int } fn main(){ print(P { x: 3, y: 4 } == P { x: 3, y: 5 }); }"),
            "false"
        );
    }

    #[test]
    fn named_functions_bind_by_name_in_let() {
        assert_eq!(
            run("fn double(x){ x + x } fn main(){ let f = double; print(f(21)); }"),
            "42"
        );
        assert_eq!(run("fn main(){ let g = abs; print(g(-7)); }"), "7");
    }

    #[test]
    fn compound_assign_evaluates_index_once() {
        // pop(sel) must fire once: the write index is 2 (sel -> [0]); xs[2] becomes 35.
        assert_eq!(
            run("fn main(){ let sel=[0,2]; let xs=[10,20,30]; xs[pop(sel)] += 5; print(xs); print(sel); }"),
            "[10, 20, 35]\n[0]"
        );
        // A simple variable index is not hoisted and still works.
        assert_eq!(
            run("fn main(){ let i=1; let xs=[10,20,30]; xs[i] += 5; print(xs); }"),
            "[10, 25, 30]"
        );
        // Nested indexed place.
        assert_eq!(
            run("fn main(){ let g=[[1,2],[3,4]]; g[0][1] += 100; print(g); }"),
            "[[1, 102], [3, 4]]"
        );
    }

    #[test]
    fn integer_casts_and_wide_literals() {
        // Signed narrowing sign-extends; unsigned keeps the masked value.
        assert_eq!(run("fn main(){ print(255 as i8); }"), "-1");
        assert_eq!(run("fn main(){ print(128 as i8); }"), "-128");
        assert_eq!(run("fn main(){ print(300 as u8); }"), "44");
        assert_eq!(run("fn main(){ print(-1 as u8); }"), "255");
        // Full-width radix literal reinterprets its bit pattern instead of collapsing to 0.
        assert_eq!(run("fn main(){ print(0xFFFFFFFFFFFFFFFF); }"), "-1");
        assert_eq!(
            run("fn main(){ print(0x7FFFFFFFFFFFFFFF); }"),
            "9223372036854775807"
        );
        // i64::MIN as a decimal literal is exact, not coerced to f64.
        assert_eq!(
            run("fn main(){ print(-9223372036854775808); }"),
            "-9223372036854775808"
        );
    }

    #[test]
    fn named_function_arity_pads_not_panics() {
        // `map` passes one argument to a 2-ary function; the missing arg pads to 0 rather than
        // panicking with an out-of-bounds index.
        assert_eq!(
            run("fn add(a,b){ a+b } fn main(){ print(map([1,2,3], add)); }"),
            "[1, 2, 3]"
        );
    }

    #[test]
    fn assert_and_assume_work_in_expression_position() {
        assert_eq!(
            run("fn main(){ let ok = assert(1 > 0); print(ok); }"),
            "true"
        );
        assert_eq!(run("fn main(){ let h = assume(2 > 1); print(h); }"), "true");
    }

    #[test]
    fn empty_interpolation_and_empty_string_are_handled() {
        // `${}` with no expression is a clean diagnostic, not a crash.
        let out = crate::frontend::parse_source_detailed("fn main(){ print(\"x=${}\"); }");
        assert!(out
            .diagnostics
            .iter()
            .any(|d| d.message.contains("empty interpolation")));
        // The empty string literal itself lowers fine.
        assert!(run("fn main(){ print(\"\"); print(\"ok\"); }").contains("ok"));
    }

    #[test]
    fn wave2_generics_patterns_and_try() {
        // Multi-argument generics parse as type annotations (erased at runtime).
        assert_eq!(
            run("fn f(p: Map<int, string>) { 0 } fn main(){ print(f(0)); }"),
            "0"
        );
        assert_eq!(
            run("struct H { data: Map<int, string> } fn main(){ let h = H { data: 5 }; print(h.data); }"),
            "5"
        );
        // An or-pattern containing a wildcard is exhaustive and matches anything.
        assert_eq!(
            run("enum Color { Red, Green, Blue } fn f(c){ match c { Color::Red | _ => \"any\" } } fn main(){ print(f(Color::Green)); }"),
            "any"
        );
        // `?` on a real Option still unwraps / short-circuits.
        assert_eq!(
            run("fn sd(a,b){ if b==0 { None } else { Some(a/b) } } fn g(a,b){ let x=sd(a,b)?; Some(x+1) } fn main(){ print(g(10,2)); print(g(1,0)); }"),
            "Some(6)\nNone"
        );
    }

    #[test]
    fn duplicate_struct_field_is_rejected() {
        let bad = crate::frontend::parse_source(
            "struct P { x: int } fn main(){ let p = P { x: 1, x: 2 }; print(0); }",
        )
        .unwrap();
        assert!(crate::middle::typecheck(bad, crate::frontend::Mode::Safe).is_err());
        let good = crate::frontend::parse_source(
            "struct P { x: int, y: int } fn main(){ let p = P { x: 1, y: 2 }; print(0); }",
        )
        .unwrap();
        assert!(crate::middle::typecheck(good, crate::frontend::Mode::Safe).is_ok());
    }

    #[test]
    fn direct_method_call_arity_is_checked() {
        let bad = crate::frontend::parse_source(
            "struct P { x: int } impl P { fn add(self, a, b) { self.x + a + b } } \
             fn main(){ print(P { x: 1 }.add(2)); }",
        )
        .unwrap();
        assert!(crate::middle::typecheck(bad, crate::frontend::Mode::Safe).is_err());
        let good = crate::frontend::parse_source(
            "struct P { x: int } impl P { fn add(self, a, b) { self.x + a + b } } \
             fn main(){ print(P { x: 1 }.add(2, 3)); }",
        )
        .unwrap();
        assert!(crate::middle::typecheck(good, crate::frontend::Mode::Safe).is_ok());
    }

    #[test]
    fn direct_closure_call_arity_is_checked() {
        let tc = |s: &str| {
            crate::middle::typecheck(
                crate::frontend::parse_source(s).unwrap(),
                crate::frontend::Mode::Safe,
            )
        };
        // Direct call with the wrong arity errors.
        assert!(tc("fn main(){ let f = |x, y| x + y; print(f(1)); }").is_err());
        // Correct arity is fine.
        assert!(tc("fn main(){ let f = |x, y| x + y; print(f(1, 2)); }").is_ok());
        // Higher-order use still pads (no error) — strict-direct, pad-HOF policy.
        assert!(tc("fn main(){ print(map([1, 2, 3], |x| x + x)); }").is_ok());
        // A named-function reference is arity-checked too.
        assert!(tc("fn add(a, b) { a + b } fn main(){ let h = add; print(h(1)); }").is_err());
        // Reassigning to a different-arity closure does not false-positive.
        assert!(tc("fn main(){ let f = |x, y| x + y; f = |z| z; print(f(3)); }").is_ok());
    }

    #[test]
    fn b1_catches_nested_constants_and_closure_bodies() {
        let tc = |s: &str| {
            crate::middle::typecheck(
                crate::frontend::parse_source(s).unwrap(),
                crate::frontend::Mode::Safe,
            )
        };
        let rej = |s: &str| tc(s).unwrap_err().contains("ANUBIS_TYPE_MISMATCH");
        // Constant (variable-free) errors nested one level deep are caught.
        assert!(
            rej("fn main(){ let y = (2 + 3)[0]; print(y); }"),
            "index a constant number"
        );
        assert!(
            rej("fn main(){ print((\"a\" + \"b\") - 1); }"),
            "constant string in `-`"
        );
        assert!(
            rej("fn main(){ let _x = (1 == 1)[0]; print(0); }"),
            "index a constant bool"
        );
        // Constant errors inside closure and block bodies are caught (the body is now walked).
        assert!(
            rej("fn main(){ let f = |q| 5[0]; print(f(0)); }"),
            "index in closure body"
        );
        assert!(
            rej("fn main(){ print(map([1, 2, 3], |x| 9[0])); }"),
            "index in map closure"
        );
        assert!(
            rej("fn main(){ let f = |q| { let z = 7[2]; z + 1 }; print(f(0)); }"),
            "index in block closure"
        );
        // Dynamic operands inside closures/blocks are still untouched (zero false positives).
        assert!(
            tc("fn main(){ let xs = [1, 2, 3]; let f = |i| xs[i]; print(f(0)); }").is_ok(),
            "closure over var index"
        );
        assert!(
            tc("fn main(){ let n = 5; let f = |x| x + n; print(f(1)); }").is_ok(),
            "closure captured arithmetic"
        );
        assert!(
            tc("fn main(){ print(map([1, 2, 3], |x| x * 2)); }").is_ok(),
            "valid map closure"
        );
    }

    #[test]
    fn b1_type_coercions_and_reassignment() {
        let tc = |s: &str| {
            crate::middle::typecheck(
                crate::frontend::parse_source(s).unwrap(),
                crate::frontend::Mode::Safe,
            )
        };
        let rej = |s: &str| tc(s).unwrap_err().contains("ANUBIS_TYPE_MISMATCH");
        // i8/i16 are numeric and interoperate with other integer widths.
        assert!(
            tc("fn f(b: i8){ print(b); } fn main(){ f(5); }").is_ok(),
            "i8 param"
        );
        assert!(
            tc("fn main(){ let x: u32 = 4000 as i16; print(x); }").is_ok(),
            "as i16"
        );
        // `+` is overloaded: `num + str` is a string.
        assert!(
            tc("fn main(){ let m: string = 404 + \": x\"; print(m); }").is_ok(),
            "num+str is string"
        );
        assert!(
            rej("fn main(){ let n: u32 = 1 + \"a\"; print(n); }"),
            "string into u32 slot"
        );
        // Reassignment: an INFERRED binding is dynamic (reassignable to any type); an EXPLICITLY
        // annotated one is held to its declared type.
        assert!(
            tc("fn main(){ let mut acc = 0; acc = \"hi\"; print(acc); }").is_ok(),
            "inferred reassign"
        );
        assert!(
            rej("fn main(){ let x: u32 = 5; x = \"a\"; print(x); }"),
            "annotated reassign"
        );
        // A stale inferred type must not linger past a reassignment to a dynamic value.
        assert!(
            tc("fn src(){ return 5; } fn need(n: u32){ print(n + 1); } \
                fn main(){ let mut x = \"p\"; x = src(); need(x); }")
            .is_ok(),
            "reassigned-to-dynamic clears the stale type"
        );
    }

    #[test]
    fn b1_static_type_checks_arithmetic_and_indexing() {
        let tc = |s: &str| {
            crate::middle::typecheck(
                crate::frontend::parse_source(s).unwrap(),
                crate::frontend::Mode::Safe,
            )
        };
        let rejected = |s: &str| tc(s).unwrap_err().contains("ANUBIS_TYPE_MISMATCH");
        // Statically-known LITERAL type errors are rejected (a literal's type is immutable).
        assert!(
            rejected("fn main(){ print(\"a\" - 1); }"),
            "string literal in `-`"
        );
        assert!(
            rejected("fn main(){ print([1, 2] * 3); }"),
            "list literal in `*`"
        );
        assert!(
            rejected("fn main(){ print({\"a\": 1} & 1); }"),
            "map literal in bitwise"
        );
        assert!(
            rejected("fn main(){ print(-\"x\"); }"),
            "unary minus on string literal"
        );
        assert!(
            rejected("fn main(){ print(5[0]); }"),
            "index a number literal"
        );
        assert!(
            rejected("fn main(){ print(true[0]); }"),
            "index a bool literal"
        );
        // Dynamic code (variables, calls, indices) is left untouched — zero false positives. A
        // variable's type is NOT stable (it may be reassigned a dynamic value), so B1 never trusts
        // it: this is the reassignment idiom (`v` starts numeric, becomes a list, is indexed) that a
        // variable-based check false-flagged.
        assert!(
            tc("fn f(x){ x - 1 } fn main(){ print(f(5)); }").is_ok(),
            "dynamic param"
        );
        assert!(
            tc("fn main(){ let s = \"hi\"; print(len(s) % 2); }").is_ok(),
            "variable untouched"
        );
        assert!(
            tc("fn main(){ let scopes = [[1, 2], [3, 4]]; let mut v = 0; v = scopes[0]; print(v[1]); }")
                .is_ok(),
            "reassignment idiom (numeric var later holds a list) is not flagged"
        );
        assert!(
            tc("fn main(){ let xs = [1, 2, 3]; print(xs[0] - 1); }").is_ok(),
            "indexed element"
        );
        assert!(
            tc("fn main(){ let xs = [1, 2]; print(len(xs) - 1); }").is_ok(),
            "call result"
        );
        assert!(
            tc("fn main(){ print(\"a\" + 1); }").is_ok(),
            "`+` is overloaded concat"
        );
        assert!(
            tc("fn main(){ let hit = 3 > 2; print(hit - 0); }").is_ok(),
            "bool 0/1 arithmetic"
        );
        assert!(
            tc("fn main(){ let x = 5; print(x * 2 - 1); }").is_ok(),
            "numeric"
        );
        assert!(tc("fn main(){ print(3.5 * 2.0); }").is_ok(), "float");
        assert!(
            tc("fn main(){ print([10, 20, 30][1]); }").is_ok(),
            "index a list literal"
        );
    }

    #[test]
    fn return_type_literal_mismatch_is_rejected() {
        let tc = |s: &str| {
            crate::middle::typecheck(
                crate::frontend::parse_source(s).unwrap(),
                crate::frontend::Mode::Safe,
            )
        };
        // A literal return of an unambiguously wrong type is rejected.
        assert!(tc("fn f() -> u32 { \"s\" } fn main(){ print(f()); }")
            .unwrap_err()
            .contains("ANUBIS_RETURN_TYPE_MISMATCH"));
        assert!(tc("fn g() -> bool { return 42; } fn main(){ print(g()); }")
            .unwrap_err()
            .contains("ANUBIS_RETURN_TYPE_MISMATCH"));
        // A cast constant is checked too: `return 5 as u32` from a `-> string` fn is rejected.
        assert!(
            tc("fn h() -> string { return 5 as u32; } fn main(){ let r = h(); print(0); }")
                .unwrap_err()
                .contains("ANUBIS_RETURN_TYPE_MISMATCH")
        );
        assert!(tc("fn ok() -> u32 { return 5 as u32; } fn main(){ print(ok()); }").is_ok());
        // Dynamic and correctly-typed returns pass (no false positives): typed literals, a numeric
        // literal into a float, a variable, a call, an if-expression, a list, and the
        // trailing-statement-yields-0 case.
        assert!(tc(
            "fn a() -> u32 { 42 } fn b() -> string { \"hi\" } fn c() -> f64 { 3.5 } \
             fn d(x) -> u32 { x + 1 } fn e() -> u32 { a() } fn g2(n) -> u32 { if n > 0 { 1 } else { 0 } } \
             fn h() -> list { [1, 2, 3] } fn lg(m) -> u32 { print(m); 0 } \
             fn main(){ print(a()); }"
        )
        .is_ok());
    }

    #[test]
    fn enum_construct_is_validated() {
        let tc = |s: &str| {
            crate::middle::typecheck(
                crate::frontend::parse_source(s).unwrap(),
                crate::frontend::Mode::Safe,
            )
        };
        // Unknown enum type (also the Rust-style `math::double(21)` qualified-call footgun):
        // the call namespace is flat, so `X::y` where X is not a declared enum fails closed.
        let e = tc("fn double(x) { x * 2 } fn main(){ print(double::double(21)); }").unwrap_err();
        assert!(e.contains("ANUBIS_UNKNOWN_ENUM"), "got: {e}");
        // Undefined variant of a real enum.
        let e =
            tc("enum Color { Red, Green, Blue } fn main(){ print(Color::Purple); }").unwrap_err();
        assert!(e.contains("ANUBIS_UNKNOWN_VARIANT"), "got: {e}");
        // Real enums (unit, tuple, recursive) and builtin Option/Result still type-check.
        assert!(tc("enum Color { Red, Green } fn main(){ print(Color::Red); }").is_ok());
        assert!(tc("enum Tree { Leaf(u32), Node(Tree, Tree) } \
             fn s(t){ match t { Tree::Leaf(v) => v, Tree::Node(l, r) => s(l) + s(r) } } \
             fn main(){ print(s(Tree::Node(Tree::Leaf(1), Tree::Leaf(2)))); }")
        .is_ok());
        assert!(
            tc("fn main(){ let r = Ok(1); print(match r { Ok(v) => v, Err(e) => 0 }); }").is_ok()
        );
    }

    #[test]
    fn stdlib_maps() {
        assert_eq!(
            run("fn main() { let m = { \"a\": 1, \"b\": 2 }; print(len(keys(m))); }"),
            "2"
        );
        assert_eq!(
            run("fn main() { let m = { \"a\": 1, \"b\": 2 }; print(sum(values(m))); }"),
            "3"
        );
        assert_eq!(
            run("fn main() { let m = { \"a\": 1 }; print(has_key(m, \"a\")); print(has_key(m, \"z\")); }"),
            "true\nfalse"
        );
        assert_eq!(
            run("fn main() { let m = { \"a\": 1, \"b\": 2 }; remove(m, \"a\"); print(len(keys(m))); }"),
            "1"
        );
    }

    #[test]
    fn stdlib_assert_and_type() {
        assert_eq!(run("fn main() { assert(1 + 1 == 2); print(42); }"), "42");
        assert_eq!(run("fn main() { print(type([1, 2])); }"), "list");
        assert_eq!(run("fn main() { print(type(3.5)); }"), "float");
        assert_eq!(run("fn main() { print(type(\"x\")); }"), "string");
    }

    #[test]
    fn assert_failure_panics() {
        let out = compile_and_run_source("fn main() { assert(1 == 2); print(1); }", false, &[])
            .expect("compile+run");
        assert!(!out.status.success(), "false assert must fail the program");
    }

    #[test]
    fn lambda_direct_call() {
        assert_eq!(
            run("fn main() { let inc = |x| x + 1; print(inc(41)); }"),
            "42"
        );
    }

    #[test]
    fn lambda_captures_environment() {
        // Capture-by-value: n is captured when the closure is created.
        let src = "fn main() { let n = 10; let addn = |x| x + n; let n = 999; print(addn(5)); }";
        assert_eq!(run(src), "15");
    }

    #[test]
    fn higher_order_map_filter_reduce() {
        assert_eq!(
            run("fn main() { print(map([1, 2, 3], |x| x * x)); }"),
            "[1, 4, 9]"
        );
        assert_eq!(
            run("fn main() { print(filter([1, 2, 3, 4, 5, 6], |x| x % 2 == 0)); }"),
            "[2, 4, 6]"
        );
        assert_eq!(
            run("fn main() { print(reduce([1, 2, 3, 4], |a, b| a + b, 0)); }"),
            "10"
        );
    }

    #[test]
    fn reduce_is_argument_order_agnostic_and_seedless() {
        // reduce is order-agnostic on its two non-list args: whichever IS a closure is the fold fn.
        // Anubis-native closure-first order:
        assert_eq!(
            run("fn main() { print(reduce([1, 2, 3, 4], |a, b| a + b, 0)); }"),
            "10"
        );
        // JS/Rust-fold-natural seed-first order (previously crashed `expected closure, got int`):
        assert_eq!(
            run("fn main() { print(reduce([1, 2, 3, 4], 0, |a, b| a + b)); }"),
            "10"
        );
        // Seedless 2-arg form: first element seeds, fold the rest.
        assert_eq!(
            run("fn main() { print(reduce([1, 2, 3, 4], |a, b| a + b)); }"),
            "10"
        );
        assert_eq!(
            run("fn main() { print(reduce([42], |a, b| a + b)); }"),
            "42"
        );
        // A SEEDLESS reduce over an EMPTY list has no answer to give. This once returned "0",
        // which is only right if the closure happens to be `+`: the same code returns 0 for a
        // multiply fold (identity is 1) and a type error for a string-concat fold. reduce cannot
        // know the closure's identity element, so it fails closed — and this assertion, which
        // encoded the old silent-wrong result, is the stale half.
        run_expect_trap(
            "fn main() { print(reduce([], |a, b| a + b)); }",
            "ANUBIS_EMPTY_COLLECTION",
        );
        // Named 2-param function in either position:
        assert_eq!(
            run("fn add(a, b) { return a + b; } fn main() { print(reduce([1,2,3], add, 0)); }"),
            "6"
        );
        assert_eq!(
            run("fn add(a, b) { return a + b; } fn main() { print(reduce([1,2,3], 0, add)); }"),
            "6"
        );
    }

    #[test]
    fn hof_correct_usage_still_works_after_failclosed_hardening() {
        // Regression guard: the fail-closed hardening of sort_by/map_values/times/min_by/max_by (which
        // replaced silent-wrong-output fall-throughs) must not disturb their correct forms.
        assert_eq!(
            run("fn main() { print(sort_by([3, 1, 2], |x| x)); }"),
            "[1, 2, 3]"
        );
        assert_eq!(run("fn main() { print(min_by([3, 1, 2], |x| x)); }"), "1");
        assert_eq!(run("fn main() { print(max_by([3, 1, 2], |x| x)); }"), "3");
        assert_eq!(
            run("fn main() { print(times(3, |i| i * i)); }"),
            "[0, 1, 4]"
        );
    }

    #[test]
    fn higher_order_composition_and_block_body() {
        // pipeline: square evens then sum
        let src = "fn main() { \
            let xs = range(1, 7); \
            let evens = filter(xs, |x| x % 2 == 0); \
            let squares = map(evens, |x| { let y = x * x; y }); \
            print(reduce(squares, |a, b| a + b, 0)); \
        }";
        assert_eq!(run(src), "56"); // 2^2 + 4^2 + 6^2 = 4 + 16 + 36 = 56
    }

    #[test]
    fn closure_returned_from_function() {
        // A function returns a closure that captures its parameter.
        let src = "fn adder(n) { return |x| x + n; } \
                   fn main() { let add10 = adder(10); print(add10(5)); }";
        assert_eq!(run(src), "15");
    }

    #[test]
    fn user_function_shadows_builtin_name() {
        // A user-defined `sort` takes precedence over the stdlib builtin.
        let src = "fn sort(x) { return 777; } fn main() { print(sort([3, 1, 2])); }";
        assert_eq!(run(src), "777");
    }

    // ---- Regressions for bugs found by the adversarial verification sweep ----

    #[test]
    fn for_loop_continue_advances() {
        // `continue` must advance the iterator, not infinite-loop.
        let src = "fn main() { let s = 0; for i in 0..6 { if i == 2 { continue; } s = s + i; } print(s); }";
        assert_eq!(run(src), "13"); // 0+1+3+4+5
    }

    #[test]
    fn integer_comparison_is_exact_above_2_53() {
        let src = "fn main() { let a = 9007199254740993; let b = 9007199254740992; \
                   print(a > b); print(a == b); print(a != b); print(b < a); }";
        assert_eq!(run(src), "true\nfalse\ntrue\ntrue");
    }

    #[test]
    fn string_relational_is_lexicographic() {
        assert_eq!(run("fn main() { print(\"apple\" < \"banana\"); }"), "true");
        assert_eq!(run("fn main() { print(\"b\" < \"a\"); }"), "false");
        assert_eq!(
            run("fn main() { print(sort([\"banana\", \"apple\", \"cherry\"])); }"),
            "[apple, banana, cherry]"
        );
    }

    #[test]
    fn sort_preserves_large_integers() {
        let src =
            "fn main() { print(sort([9007199254740993, 9007199254740992, 9007199254740994])); }";
        assert_eq!(
            run(src),
            "[9007199254740992, 9007199254740993, 9007199254740994]"
        );
    }

    #[test]
    fn closure_capture_of_shadowing_name() {
        // `map` is a builtin name but here a local variable — it must still be captured by clone.
        let src = "fn main() { let map = 100; let f = |x| x + map; let g = |x| x - map; \
                   print(f(5)); print(g(5)); }";
        assert_eq!(run(src), "105\n-95");
    }

    #[test]
    fn map_literal_dedups_keys() {
        let src =
            "fn main() { let m = { \"a\": 1, \"a\": 2 }; print(m[\"a\"]); print(len(keys(m))); }";
        assert_eq!(run(src), "2\n1");
    }

    #[test]
    fn cast_to_integer_types() {
        assert_eq!(run("fn main() { print((3.9 as u32) + 1); }"), "4");
        assert_eq!(run("fn main() { print(300 as u8); }"), "44");
        assert_eq!(run("fn main() { print(-1 as u8); }"), "255");
        assert_eq!(run("fn main() { print(type(3.9 as u32)); }"), "int");
    }

    #[test]
    fn call_arbitrary_callee_expression() {
        // Call a closure obtained from a map, a struct field, and a chained call.
        assert_eq!(
            run("fn main() { let ops = { \"add\": |a, b| a + b }; print(ops[\"add\"](2, 3)); }"),
            "5"
        );
        assert_eq!(
            run("struct B { f: u32 } fn main() { let b = B { f: |x| x * 10 }; print(b.f(4)); }"),
            "40"
        );
        assert_eq!(
            run("fn main() { let curry = |a| |b| a + b; print(curry(10)(5)); }"),
            "15"
        );
    }

    #[test]
    fn sort_by_and_any_all() {
        assert_eq!(
            run("fn main() { print(sort_by([3, 1, 2], |x| 0 - x)); }"),
            "[3, 2, 1]"
        );
        assert_eq!(
            run("fn main() { print(any([1, 2, 3], |x| x > 2)); }"),
            "true"
        );
        assert_eq!(
            run("fn main() { print(all([1, 2, 3], |x| x > 0)); }"),
            "true"
        );
    }

    #[test]
    fn typecheck_rejects_duplicate_function() {
        let ast = crate::frontend::parse_source("fn f() { } fn f() { } fn main() { }").unwrap();
        let err = crate::typecheck(ast, crate::frontend::Mode::Safe).unwrap_err();
        assert!(err.contains("ANUBIS_DUPLICATE_FUNCTION"), "{}", err);
    }

    #[test]
    fn typecheck_rejects_duplicate_param() {
        let ast = crate::frontend::parse_source("fn f(x, x) { } fn main() { }").unwrap();
        let err = crate::typecheck(ast, crate::frontend::Mode::Safe).unwrap_err();
        assert!(err.contains("ANUBIS_DUPLICATE_PARAM"), "{}", err);
    }

    #[test]
    fn typecheck_rejects_unknown_function() {
        let ast = crate::frontend::parse_source("fn main() { let x = nonexistent(1); }").unwrap();
        let err = crate::typecheck(ast, crate::frontend::Mode::Safe).unwrap_err();
        assert!(err.contains("ANUBIS_UNKNOWN_FUNCTION"), "{}", err);
    }

    #[test]
    fn typecheck_accepts_closure_and_builtin_calls() {
        // A local closure `f` and a stdlib builtin `upper` must not be flagged as unknown.
        let ast = crate::frontend::parse_source(
            "fn main() { let f = |x| x + 1; let y = f(2); let s = upper(\"hi\"); }",
        )
        .unwrap();
        assert!(crate::typecheck(ast, crate::frontend::Mode::Safe).is_ok());
    }

    #[test]
    fn golden_tour_programs() {
        // Every program in examples/tour/ carries a `// EXPECT: line1|line2|...` header; run it and
        // assert its stdout (newlines -> `|`) matches. Guards the whole language surface end-to-end.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/tour");
        let mut count = 0;
        for entry in std::fs::read_dir(&dir).expect("examples/tour directory") {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) != Some("anub") {
                continue;
            }
            let src = std::fs::read_to_string(&path).unwrap();
            let expect = src
                .lines()
                .next()
                .and_then(|l| l.trim().strip_prefix("// EXPECT:"))
                .unwrap_or_else(|| panic!("{:?} missing `// EXPECT:` header", path))
                .trim()
                .to_string();
            let out = compile_and_run_source(&src, false, &[]).expect("run tour program");
            assert!(
                out.status.success(),
                "{:?} exited nonzero: {}",
                path,
                String::from_utf8_lossy(&out.stderr)
            );
            let got = String::from_utf8_lossy(&out.stdout)
                .trim()
                .replace('\n', "|");
            assert_eq!(got, expect, "output mismatch for {:?}", path);
            count += 1;
        }
        assert!(count >= 10, "expected >= 10 tour programs, ran {}", count);
    }

    #[test]
    fn golden_program_examples() {
        // The examples/programs/ corpus — larger, real programs (a recursive-descent calculator, a
        // BST over a recursive enum, a stack VM, grid BFS, merge sort, trait polymorphism, `?`
        // chains) — each with a `// EXPECT:` header. Dogfooding kept as regression coverage: proves
        // the language runs substantial real programs end-to-end and never silently regresses.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/programs");
        let mut count = 0;
        for entry in std::fs::read_dir(&dir).expect("examples/programs directory") {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) != Some("anub") {
                continue;
            }
            let src = std::fs::read_to_string(&path).unwrap();
            let expect = src
                .lines()
                .next()
                .and_then(|l| l.trim().strip_prefix("// EXPECT:"))
                .unwrap_or_else(|| panic!("{:?} missing `// EXPECT:` header", path))
                .trim()
                .to_string();
            let out = compile_and_run_source(&src, false, &[]).expect("run program example");
            assert!(
                out.status.success(),
                "{:?} exited nonzero: {}",
                path,
                String::from_utf8_lossy(&out.stderr)
            );
            let got = String::from_utf8_lossy(&out.stdout)
                .trim()
                .replace('\n', "|");
            assert_eq!(got, expect, "output mismatch for {:?}", path);
            count += 1;
        }
        assert!(count >= 8, "expected >= 8 program examples, ran {}", count);
    }

    #[test]
    fn research_path_compiles_poc_kit_runtime() {
        // Exercises the allow_research lowering so the PoC-kit runtime (which contains its own
        // exhaustive matches over AnubisValue) is compiled — guards against a missing variant arm.
        let out =
            compile_and_run_source("fn main() { let p = p32(65); print(len(p)); }", true, &[])
                .expect("compile+run research");
        assert!(
            out.status.success(),
            "research-path program failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "4");
    }

    #[test]
    fn sha256_and_hmac_match_nist_and_rfc() {
        // SHA-256("abc") = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
        let out = compile_and_run_source(
            r#"fn main() {
                print(sha256("abc"));
                print(hmac_sha256("key", "The quick brown fox jumps over the lazy dog"));
            }"#,
            false,
            &[],
        )
        .expect("compile+run crypto");
        assert!(
            out.status.success(),
            "crypto program failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let lines: Vec<_> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        assert_eq!(
            lines[0], "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            "SHA-256(abc)"
        );
        // RFC 4231-style known vector for key="key", data=fox sentence
        assert_eq!(
            lines[1], "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8",
            "HMAC-SHA256(key, fox)"
        );
    }

    #[test]
    fn http_get_and_post_lower_against_local_listener() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::mpsc;
        use std::thread;
        use std::time::Duration;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = mpsc::channel::<String>();
        thread::spawn(move || {
            // Serve one GET then one POST (order matches the Anubis program below).
            for expected_method in ["GET", "POST"] {
                let (mut stream, _) = listener.accept().expect("accept");
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let _ = tx.send(req.clone());
                assert!(
                    req.starts_with(expected_method),
                    "expected {expected_method} request, got: {req}"
                );
                let body = if expected_method == "GET" {
                    "hello-get"
                } else {
                    "hello-post"
                };
                let resp = format!(
                    "HTTP/1.0 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(resp.as_bytes());
            }
        });

        // Small settle so the accept thread is ready.
        thread::sleep(Duration::from_millis(50));
        let src = format!(
            r#"fn main() uses(net.send) {{
                let g = http_get("http://127.0.0.1:{port}/ping");
                print(g);
                let p = http_post("http://127.0.0.1:{port}/echo", "payload");
                print(p);
            }}"#
        );
        let out = compile_and_run_source(&src, false, &[]).expect("compile+run http");
        assert!(
            out.status.success(),
            "http program failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let lines: Vec<_> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        assert_eq!(
            lines,
            vec!["hello-get".to_string(), "hello-post".to_string()]
        );

        // Confirm both methods actually hit the listener (not a fake).
        let r1 = rx.recv_timeout(Duration::from_secs(5)).expect("get req");
        let r2 = rx.recv_timeout(Duration::from_secs(5)).expect("post req");
        assert!(r1.contains("GET /ping"), "GET path: {r1}");
        assert!(r2.contains("POST /echo"), "POST path: {r2}");
        assert!(r2.contains("payload"), "POST body: {r2}");
    }

    #[test]
    fn http_https_via_system_curl() {
        // Network-dependent: skip if offline / curl blocked.
        if std::process::Command::new("curl")
            .args([
                "-fsSL",
                "--max-time",
                "10",
                "-o",
                "/dev/null",
                "https://example.com/",
            ])
            .status()
            .map(|s| !s.success())
            .unwrap_or(true)
        {
            eprintln!("skip http_https_via_system_curl: host cannot reach https://example.com/");
            return;
        }
        let out = compile_and_run_source(
            r#"fn main() uses(net.send) {
                let b = http_get("https://example.com/");
                // example.com returns HTML; non-empty body is the witness.
                print(len(b) > 0);
            }"#,
            false,
            &[],
        )
        .expect("compile+run https");
        assert!(
            out.status.success(),
            "https program failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "true");
    }

    #[test]
    fn http_bad_scheme_fails_closed() {
        run_expect_trap(
            r#"fn main() uses(net.send) {
                let _ = http_get("ftp://example.com/");
            }"#,
            "ANUBIS_IO_ERROR",
        );
    }

    #[test]
    fn keychain_se_probe_and_ne_acquire_run() {
        // Soft path (opt-out of Keychain) must always run.
        // SAFETY: test-only env override for deterministic soft tokens.
        unsafe {
            std::env::set_var("ANUBIS_KEYCHAIN_CAPS", "0");
            std::env::set_var("ANUBIS_KEYCHAIN_SE", "0");
        }
        let out = compile_and_run_source(
            r#"fn main() {
                let p = keychain_se_probe();
                print(p);
                let s = cap_acquire_nonexportable("fs.write");
                print(s);
                let c = cap_acquire("fs.read");
                print(c);
            }"#,
            false,
            &[],
        )
        .expect("compile+run soft keychain path");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success(),
            "stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            stdout.contains('0') || stdout.contains('1') || stdout.contains('2'),
            "probe line missing: {stdout}"
        );
        assert!(
            stdout.contains("__anubis_cap_ne_soft:")
                || stdout.contains("__anubis_cap_ne_kc:")
                || stdout.contains("__anubis_cap_ne_se:"),
            "ne token missing: {stdout}"
        );
        assert!(
            stdout.contains("__anubis_cap:fs.read"),
            "exportable token missing: {stdout}"
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn keychain_se_signed_run_binds_keychain() {
        // Signed compile→codesign(Apple Development)→run must mint a Keychain-backed NE token.
        let out = compile_sign_and_run_source(
            r#"fn main() {
                let p = keychain_se_probe();
                print(p);
                let s = cap_acquire_nonexportable("fs.write");
                print(s);
            }"#,
            false,
            &[],
        )
        .expect("signed compile+run");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.status.success(),
            "signed run failed:\nstdout={stdout}\nstderr={stderr}"
        );
        // Under a real Development identity, Keychain bind is expected (not soft).
        // Ad-hoc-only hosts may soft-fallback — accept soft only when identity is "-".
        let id = resolve_codesign_identity();
        if id != "-" {
            assert!(
                stdout.contains("__anubis_cap_ne_kc:") || stdout.contains("__anubis_cap_ne_se:"),
                "expected Keychain/SE bind under Development identity {id:?}, got: {stdout}"
            );
        } else {
            assert!(
                stdout.contains("__anubis_cap_ne_soft:")
                    || stdout.contains("__anubis_cap_ne_kc:")
                    || stdout.contains("__anubis_cap_ne_se:"),
                "expected NE token: {stdout}"
            );
        }
    }

    // ── CLAIMS-15: carrier immunity for gated builtins ──────────────────────
    //
    // `var_as_value` wraps any name with an `emit_builtin_call` arm into a
    // closure.  Gated builtins (`is_non_run_builtin` ∪ `is_poc_kit_builtin`)
    // are kept OUT of `emit_builtin_call` so that carrier forms like
    // `let f = shell; f("cmd")` cannot compile.
    //
    // Adding a gated builtin to `emit_builtin_call` closes the call-site gate
    // (Surface 1) while silently opening the value-position carrier (Surface 2).
    // The MODE key (`--allow-research`) is call-site only and cannot protect
    // value-position bindings.
    //
    // To add runtime behavior for a gated builtin, put it in `safe_run_expr`
    // (call-site dispatch), NOT `emit_builtin_call`.  See `p8`..`flat` and
    // `target_run` in `safe_run_expr` for the pattern.
    //
    // `assert` is excluded: it has a benign lowering (`anubis_assert` — panics
    // on false, no data exfiltration) and is already in `emit_builtin_call`.
    #[test]
    fn gated_builtins_must_not_lower_in_emit_builtin_call() {
        let carrier_critical: &[&str] = &[
            // is_non_run_builtin (minus assert — benign, already lowered)
            "symbolic", "assume", "taint_source", "declassify", "sink",
            "shell", "exec", "system", "memcpy", "sql",
            // is_poc_kit_builtin
            "p8", "p16", "p32", "p64", "cyclic", "target_run", "flat",
        ];
        for name in carrier_critical {
            for arity in 0..=6usize {
                let args: Vec<String> =
                    (0..arity).map(|i| format!("__a{i}")).collect();
                assert!(
                    emit_builtin_call(name, &args).is_none(),
                    "CLAIMS-15 VIOLATION: gated builtin `{name}` gained a lowering \
                     in emit_builtin_call (arity {arity}).  This opens the \
                     builtin-carrier class: `let f = {name}; f(...)` compiles via \
                     var_as_value, bypassing the call-site gate.  --allow-research \
                     cannot protect value-position bindings.  To add runtime \
                     behavior for `{name}`, gate it in safe_run_expr, NOT \
                     emit_builtin_call.",
                );
            }
        }
    }
}
