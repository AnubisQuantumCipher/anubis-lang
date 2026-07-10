//! Anubis run/transpile backend.
//!
//! Lowers a parsed Anubis program to a self-contained Rust program for native
//! execution (`anubis run`) or to a RISC0 zkVM guest (`anubis prove`). This is the
//! executable semantics of Anubis. It lives in the compiler crate (not the CLI) so the
//! whole language is unit-testable without the heavy risc0 workspace.

use crate::frontend::{Expr, Item, Stmt};
use anyhow::{anyhow, Result};

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
            Stmt::While { cond, body } => {
                collect_bound_in_expr(cond, out);
                collect_bound_in_stmts(body, out);
            }
            Stmt::Loop { body } => collect_bound_in_stmts(body, out),
            Stmt::For { var, source, body } => {
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
            Stmt::WhileLet { pattern, expr, body } => {
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
        Expr::IfLet { pattern, scrutinee, then, else_, .. } => {
            for n in pattern.bound_names() {
                out.insert(n);
            }
            collect_bound_in_expr(scrutinee, out);
            collect_bound_in_expr(then, out);
            collect_bound_in_expr(else_, out);
        }
        Expr::Match { scrutinee, arms, .. } => {
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
        Expr::If { cond, then, else_, .. } => {
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
}

/// The emitted Rust function name for an Anubis function or method.
fn fn_rust_name(name: &str, impl_type: Option<&str>) -> Result<String> {
    match impl_type {
        Some(ty) => Ok(format!("anb_{}__method__{}", sanitize_ident(ty)?, sanitize_ident(name)?)),
        None => Ok(format!("anb_{}", sanitize_ident(name)?)),
    }
}

/// Recursively collect every `fn` item (including inside modules and `impl` blocks).
fn collect_fns<'a>(items: &'a [Item], out: &mut Vec<FnDef<'a>>) {
    for item in items {
        match item {
            Item::Fn {
                name, params, body, ..
            } => out.push(FnDef {
                name: name.as_str(),
                params: params.as_slice(),
                body: body.as_slice(),
                impl_type: None,
            }),
            Item::Impl { type_name, methods, .. } => {
                for m in methods {
                    if let Item::Fn {
                        name, params, body, ..
                    } = m
                    {
                        out.push(FnDef {
                            name: name.as_str(),
                            params: params.as_slice(),
                            body: body.as_slice(),
                            impl_type: Some(type_name.as_str()),
                        });
                    }
                }
            }
            Item::Module { items, .. } => collect_fns(items, out),
            _ => {}
        }
    }
}

/// Build the method registry: method name -> `(type, param_count)` for each defining type.
fn collect_methods(
    items: &[Item],
    out: &mut std::collections::BTreeMap<String, Vec<(String, usize)>>,
) {
    for item in items {
        match item {
            Item::Impl { type_name, methods, .. } => {
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
    // Per-function local scope: params + everything bound in the body. A call to one of these
    // names is a closure application, not a builtin.
    let locals = collect_local_names(def.params, def.body);
    let ctx = &EmitCtx {
        allow_research: base.allow_research,
        fns: base.fns,
        fn_arities: base.fn_arities,
        methods: base.methods,
        locals: &locals,
    };
    let mut sig = Vec::new();
    for (p, _ty) in def.params {
        sig.push(format!("mut {}: AnubisValue", sanitize_ident(p)?));
    }
    let (head, tail) = split_tail_expr(def.body);
    let mut body_src = String::new();
    for stmt in &head {
        emit_safe_run_stmt(stmt, 1, &mut body_src, ctx)?;
    }
    // Implicit return: a bare trailing expression is the function's value (like Rust/ML).
    // Falls back to Int(0) for bodies that end in a statement or are empty.
    let tail_src = match &tail {
        Some(expr) => safe_run_expr(expr, ctx)?,
        None => "AnubisValue::Int(0)".to_string(),
    };
    Ok(format!(
        "fn {}({}) -> AnubisValue {{\n{}    {}\n}}\n",
        fn_rust_name(def.name, def.impl_type)?,
        sig.join(", "),
        body_src,
        tail_src,
    ))
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
    lower_program_with_entry(
        items,
        "",
        "fn main() {\n    let _ = anb_main();\n}\n",
        allow_research,
        false,
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
) -> Result<String> {
    let mut fns = Vec::new();
    collect_fns(items, &mut fns);
    if !fns.iter().any(|d| d.name == "main" && d.impl_type.is_none()) {
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
    let mut methods = std::collections::BTreeMap::new();
    collect_methods(items, &mut methods);
    let empty_locals = std::collections::BTreeSet::new();
    let ctx = EmitCtx {
        allow_research,
        fns: &fn_names,
        fn_arities: &fn_arities,
        methods: &methods,
        locals: &empty_locals,
    };
    let mut functions_src = String::new();
    for def in &fns {
        functions_src.push_str(&emit_fn(def, &ctx)?);
        functions_src.push('\n');
    }
    let poc_kit_runtime = if allow_research {
        POC_KIT_RUNTIME_RS
    } else {
        ""
    };
    let proof_input_runtime = if guest_proof_inputs {
        PROOF_INPUT_GUEST_RUNTIME_RS
    } else {
        // Native `anubis run`: commits are no-op (return value); asserts still fail-closed.
        NATIVE_PROOF_STUBS_RS
    };
    Ok(format!(
        "{header}{prelude}\n{core}\n{poc}\n{proof}\n{functions}\n{entry}",
        header = "#![allow(dead_code, unused_mut, unused_variables, unused_assignments, unreachable_code, unused_parens, unused_imports, non_snake_case, unused_braces)]\n",
        prelude = prelude,
        core = ANUBIS_CORE_RUNTIME_RS,
        poc = poc_kit_runtime,
        proof = proof_input_runtime,
        functions = functions_src,
        entry = entry,
    ))
}

/// The Anubis runtime value model + operator helpers, shared by native `run` and RISC0
/// guest lowering. Emitted verbatim into every generated Rust program.
const ANUBIS_CORE_RUNTIME_RS: &str = r#"
#[derive(Clone)]
enum AnubisValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    List(Vec<AnubisValue>),
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
    /// Dictionary: string keys (via display_string) -> values.
    Map(Vec<(String, AnubisValue)>),
    /// A first-class function value (lambda), callable with a positional argument vector.
    Closure(std::rc::Rc<dyn Fn(Vec<AnubisValue>) -> AnubisValue>),
}

impl std::fmt::Debug for AnubisValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_string())
    }
}

impl AnubisValue {
    /// Apply a closure value to positional arguments. Non-closures return Int(0).
    fn call_closure(&self, args: Vec<AnubisValue>) -> AnubisValue {
        match self {
            AnubisValue::Closure(f) => f(args),
            _ => AnubisValue::Int(0),
        }
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
            AnubisValue::Str(v) => v.clone(),
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
            AnubisValue::List(v) => {
                let idx = anubis_norm_index(i.as_i64(), v.len());
                match idx { Some(k) => v[k].clone(), None => AnubisValue::Int(0) }
            }
            AnubisValue::Str(s) => {
                let chars: Vec<char> = s.chars().collect();
                let idx = anubis_norm_index(i.as_i64(), chars.len());
                match idx { Some(k) => AnubisValue::Str(chars[k].to_string()), None => AnubisValue::Str(String::new()) }
            }
            AnubisValue::Map(m) => {
                let key = i.display_string();
                m.iter().find(|(k, _)| k == &key).map(|(_, v)| v.clone()).unwrap_or(AnubisValue::Int(0))
            }
            // A+: struct field order supports list-style r[0] (TargetRun and friends).
            AnubisValue::Struct { fields, .. } => {
                let idx = i.as_i64();
                if idx >= 0 && (idx as usize) < fields.len() {
                    fields[idx as usize].1.clone()
                } else {
                    let key = i.display_string();
                    fields.iter().find(|(k, _)| k == &key).map(|(_, v)| v.clone()).unwrap_or(AnubisValue::Int(0))
                }
            }
            _ => AnubisValue::Int(0),
        }
    }

    fn index_set(&mut self, i: AnubisValue, val: AnubisValue) {
        match self {
            AnubisValue::List(v) => {
                if let Some(k) = anubis_norm_index(i.as_i64(), v.len()) {
                    v[k] = val;
                }
            }
            AnubisValue::Map(m) => {
                let key = i.display_string();
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
                if let Some(slot) = m.iter_mut().find(|(k, _)| k == name) { slot.1 = val; }
                else { m.push((name.to_string(), val)); }
            }
            _ => {}
        }
    }

    fn push_val(&mut self, val: AnubisValue) {
        if let AnubisValue::List(v) = self {
            v.push(val);
        }
    }

    fn len_val(&self) -> AnubisValue {
        match self {
            AnubisValue::List(v) => AnubisValue::Int(v.len() as i64),
            AnubisValue::Str(s) => AnubisValue::Int(s.chars().count() as i64),
            AnubisValue::Map(m) => AnubisValue::Int(m.len() as i64),
            AnubisValue::Struct { fields, .. } => AnubisValue::Int(fields.len() as i64),
            AnubisValue::Enum { fields, .. } => AnubisValue::Int(fields.len() as i64),
            _ => AnubisValue::Int(0),
        }
    }

    /// Keys of a map as a list of strings (for `for k in m`).
    fn map_keys(&self) -> AnubisValue {
        match self {
            AnubisValue::Map(m) => AnubisValue::List(
                m.iter().map(|(k, _)| AnubisValue::Str(k.clone())).collect()
            ),
            _ => AnubisValue::List(vec![]),
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
        (AnubisValue::List(mut a), AnubisValue::List(b)) => { a.extend(b); AnubisValue::List(a) }
        (AnubisValue::List(mut a), b) => { a.push(b); AnubisValue::List(a) }
        (AnubisValue::Str(a), b) => AnubisValue::Str(format!("{}{}", a, b.display_string())),
        (a, AnubisValue::Str(b)) => AnubisValue::Str(format!("{}{}", a.display_string(), b)),
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
                        v[k].set_at(rest, val);
                    }
                }
                AnubisValue::Map(m) => {
                    let key = i.display_string();
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
                            *s = chars.into_iter().collect();
                        }
                    }
                }
                _ => {}
            },
        }
    }
}

// ---- Anubis standard library runtime (shared by native run + guest) ----

fn anubis_str(v: AnubisValue) -> AnubisValue { AnubisValue::Str(v.display_string()) }
fn anubis_int(v: AnubisValue) -> AnubisValue { AnubisValue::Int(v.as_i64()) }
fn anubis_float(v: AnubisValue) -> AnubisValue { AnubisValue::Float(v.as_f64()) }
fn anubis_bool_of(v: AnubisValue) -> AnubisValue { AnubisValue::Bool(v.as_bool()) }
fn anubis_type_of(v: AnubisValue) -> AnubisValue { AnubisValue::Str(v.type_name().to_string()) }

fn anubis_abs(v: AnubisValue) -> AnubisValue {
    if v.is_float() { AnubisValue::Float(v.as_f64().abs()) } else { AnubisValue::Int(v.as_i64().wrapping_abs()) }
}
// Ordered via `anubis_value_cmp` — the same comparator `sort`/`min_by` use — so Int/Int compares
// exactly as i64 (an f64 round-trip loses distinctions above 2^53) and strings order lexically.
fn anubis_min2(a: AnubisValue, b: AnubisValue) -> AnubisValue { if anubis_value_cmp(&a, &b) != std::cmp::Ordering::Greater { a } else { b } }
fn anubis_max2(a: AnubisValue, b: AnubisValue) -> AnubisValue { if anubis_value_cmp(&a, &b) != std::cmp::Ordering::Less { a } else { b } }
fn anubis_seq(items: Vec<AnubisValue>) -> Vec<AnubisValue> {
    if items.len() == 1 { if let AnubisValue::List(l) = &items[0] { return l.clone(); } }
    items
}
fn anubis_min(items: Vec<AnubisValue>) -> AnubisValue {
    anubis_seq(items).into_iter().reduce(anubis_min2).unwrap_or(AnubisValue::Int(0))
}
fn anubis_max(items: Vec<AnubisValue>) -> AnubisValue {
    anubis_seq(items).into_iter().reduce(anubis_max2).unwrap_or(AnubisValue::Int(0))
}
fn anubis_pow(base: AnubisValue, exp: AnubisValue) -> AnubisValue {
    if base.is_float() || exp.is_float() {
        AnubisValue::Float(base.as_f64().powf(exp.as_f64()))
    } else {
        let e = exp.as_i64();
        if e < 0 { AnubisValue::Float(base.as_f64().powi(e as i32)) }
        else { AnubisValue::Int(base.as_i64().wrapping_pow(e as u32)) }
    }
}
fn anubis_sqrt(v: AnubisValue) -> AnubisValue { AnubisValue::Float(v.as_f64().sqrt()) }
// floor/ceil/round/trunc are the identity on an integer (an i64 has no fractional part, and
// routing it through f64 would corrupt magnitudes above 2^53). Only floats are rounded.
fn anubis_floor(v: AnubisValue) -> AnubisValue { match v { AnubisValue::Int(n) => AnubisValue::Int(n), _ => AnubisValue::Int(v.as_f64().floor() as i64) } }
fn anubis_ceil(v: AnubisValue) -> AnubisValue { match v { AnubisValue::Int(n) => AnubisValue::Int(n), _ => AnubisValue::Int(v.as_f64().ceil() as i64) } }
fn anubis_round(v: AnubisValue) -> AnubisValue { match v { AnubisValue::Int(n) => AnubisValue::Int(n), _ => AnubisValue::Int(v.as_f64().round() as i64) } }
fn anubis_gcd(a: AnubisValue, b: AnubisValue) -> AnubisValue {
    let (mut x, mut y) = (a.as_i64().wrapping_abs(), b.as_i64().wrapping_abs());
    while y != 0 { let t = y; y = x % y; x = t; }
    AnubisValue::Int(x)
}

fn anubis_upper(v: AnubisValue) -> AnubisValue { AnubisValue::Str(v.display_string().to_uppercase()) }
fn anubis_lower(v: AnubisValue) -> AnubisValue { AnubisValue::Str(v.display_string().to_lowercase()) }
fn anubis_trim(v: AnubisValue) -> AnubisValue { AnubisValue::Str(v.display_string().trim().to_string()) }
fn anubis_split(s: AnubisValue, sep: AnubisValue) -> AnubisValue {
    let hay = s.display_string();
    let sp = sep.display_string();
    let parts: Vec<AnubisValue> = if sp.is_empty() {
        hay.chars().map(|c| AnubisValue::Str(c.to_string())).collect()
    } else {
        hay.split(sp.as_str()).map(|p| AnubisValue::Str(p.to_string())).collect()
    };
    AnubisValue::List(parts)
}
fn anubis_join(list: AnubisValue, sep: AnubisValue) -> AnubisValue {
    let sp = sep.display_string();
    match list {
        AnubisValue::List(items) => AnubisValue::Str(
            items.iter().map(|x| x.display_string()).collect::<Vec<_>>().join(sp.as_str())
        ),
        other => AnubisValue::Str(other.display_string()),
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
        _ => false,
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
    AnubisValue::Str(s.display_string().replace(from.display_string().as_str(), to.display_string().as_str()))
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
        _ => AnubisValue::Int(-1),
    }
}
fn anubis_ord(v: AnubisValue) -> AnubisValue {
    AnubisValue::Int(v.display_string().chars().next().map(|c| c as i64).unwrap_or(0))
}
fn anubis_chr(v: AnubisValue) -> AnubisValue {
    AnubisValue::Str(char::from_u32(v.as_i64() as u32).map(|c| c.to_string()).unwrap_or_default())
}
fn anubis_repeat(s: AnubisValue, n: AnubisValue) -> AnubisValue {
    let count = n.as_i64().max(0) as usize;
    match s {
        AnubisValue::List(items) => {
            let mut out = Vec::new();
            for _ in 0..count { out.extend(items.iter().cloned()); }
            AnubisValue::List(out)
        }
        other => AnubisValue::Str(other.display_string().repeat(count)),
    }
}
fn anubis_substr(s: AnubisValue, start: AnubisValue, len: AnubisValue) -> AnubisValue {
    let chars: Vec<char> = s.display_string().chars().collect();
    let st = start.as_i64().max(0) as usize;
    let ln = len.as_i64().max(0) as usize;
    AnubisValue::Str(chars.into_iter().skip(st).take(ln).collect())
}
fn anubis_slice(x: AnubisValue, a: AnubisValue, b: AnubisValue) -> AnubisValue {
    let (ai, bi) = (a.as_i64(), b.as_i64());
    let bound = |i: i64, n: i64| -> usize { (if i < 0 { (i + n).max(0) } else { i.min(n) }) as usize };
    match x {
        AnubisValue::List(items) => {
            let n = items.len() as i64;
            let (lo, hi) = (bound(ai, n), bound(bi, n));
            AnubisValue::List(if lo <= hi { items[lo..hi].to_vec() } else { vec![] })
        }
        AnubisValue::Str(s) => {
            let chars: Vec<char> = s.chars().collect();
            let n = chars.len() as i64;
            let (lo, hi) = (bound(ai, n), bound(bi, n));
            AnubisValue::Str(if lo <= hi { chars[lo..hi].iter().collect() } else { String::new() })
        }
        other => other,
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

fn anubis_range(a: AnubisValue, b: AnubisValue) -> AnubisValue {
    let (mut i, hi) = (a.as_i64(), b.as_i64());
    let mut out = Vec::new();
    while i < hi { out.push(AnubisValue::Int(i)); i += 1; }
    AnubisValue::List(out)
}
fn anubis_range_step(a: AnubisValue, b: AnubisValue, step: AnubisValue) -> AnubisValue {
    let (mut i, hi, st) = (a.as_i64(), b.as_i64(), step.as_i64());
    let mut out = Vec::new();
    if st > 0 { while i < hi { out.push(AnubisValue::Int(i)); i += st; } }
    else if st < 0 { while i > hi { out.push(AnubisValue::Int(i)); i += st; } }
    AnubisValue::List(out)
}
fn anubis_reverse(x: AnubisValue) -> AnubisValue {
    match x {
        AnubisValue::List(mut items) => { items.reverse(); AnubisValue::List(items) }
        AnubisValue::Str(s) => AnubisValue::Str(s.chars().rev().collect()),
        other => other,
    }
}
fn anubis_sort(x: AnubisValue) -> AnubisValue {
    match x {
        AnubisValue::List(mut items) => {
            items.sort_by(anubis_value_cmp);
            AnubisValue::List(items)
        }
        other => other,
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
        other => other,
    }
}
fn anubis_keys(m: AnubisValue) -> AnubisValue { m.map_keys() }
fn anubis_values(m: AnubisValue) -> AnubisValue {
    match m { AnubisValue::Map(e) => AnubisValue::List(e.into_iter().map(|(_, v)| v).collect()), _ => AnubisValue::List(vec![]) }
}
fn anubis_has_key(m: AnubisValue, k: AnubisValue) -> AnubisValue {
    let key = k.display_string();
    match m { AnubisValue::Map(e) => AnubisValue::Bool(e.iter().any(|(kk, _)| kk == &key)), _ => AnubisValue::Bool(false) }
}

fn anubis_pop(v: &mut AnubisValue) -> AnubisValue {
    if let AnubisValue::List(l) = v { l.pop().unwrap_or(AnubisValue::Int(0)) } else { AnubisValue::Int(0) }
}
fn anubis_insert(v: &mut AnubisValue, i: AnubisValue, val: AnubisValue) -> AnubisValue {
    if let AnubisValue::List(l) = v {
        let raw = i.as_i64();
        let len = l.len() as i64;
        // Negative indices count from the end (consistent with element indexing).
        let idx = if raw < 0 { (raw + len).max(0) } else { raw.min(len) } as usize;
        l.insert(idx, val);
    }
    AnubisValue::Int(0)
}
fn anubis_remove(v: &mut AnubisValue, key: AnubisValue) -> AnubisValue {
    match v {
        AnubisValue::List(l) => {
            match anubis_norm_index(key.as_i64(), l.len()) { Some(k) => l.remove(k), None => AnubisValue::Int(0) }
        }
        AnubisValue::Map(m) => {
            let k = key.display_string();
            match m.iter().position(|(kk, _)| kk == &k) { Some(pos) => m.remove(pos).1, None => AnubisValue::Int(0) }
        }
        _ => AnubisValue::Int(0),
    }
}

fn anubis_assert(cond: AnubisValue) -> AnubisValue {
    if !cond.as_bool() { panic!("ANUBIS_ASSERT_FAILED"); }
    AnubisValue::Bool(true)
}
fn anubis_panic(msg: AnubisValue) -> AnubisValue { panic!("ANUBIS_PANIC: {}", msg.display_string()); }

fn anubis_input() -> AnubisValue {
    use std::io::BufRead;
    let mut line = String::new();
    let _ = std::io::stdin().lock().read_line(&mut line);
    while line.ends_with('\n') || line.ends_with('\r') { line.pop(); }
    AnubisValue::Str(line)
}
fn anubis_args() -> AnubisValue {
    AnubisValue::List(std::env::args().skip(1).map(AnubisValue::Str).collect())
}

// ---- Higher-order functions over closures ----

fn anubis_map(list: AnubisValue, f: AnubisValue) -> AnubisValue {
    AnubisValue::List(anubis_iter(list).into_iter().map(|x| f.call_closure(vec![x])).collect())
}
fn anubis_filter(list: AnubisValue, f: AnubisValue) -> AnubisValue {
    AnubisValue::List(anubis_iter(list).into_iter().filter(|x| f.call_closure(vec![x.clone()]).as_bool()).collect())
}
fn anubis_reduce(list: AnubisValue, f: AnubisValue, init: AnubisValue) -> AnubisValue {
    let mut acc = init;
    for x in anubis_iter(list) { acc = f.call_closure(vec![acc, x]); }
    acc
}
fn anubis_each(list: AnubisValue, f: AnubisValue) -> AnubisValue {
    for x in anubis_iter(list) { let _ = f.call_closure(vec![x]); }
    AnubisValue::Int(0)
}
fn anubis_find(list: AnubisValue, f: AnubisValue) -> AnubisValue {
    for x in anubis_iter(list) { if f.call_closure(vec![x.clone()]).as_bool() { return x; } }
    AnubisValue::Int(0)
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
        AnubisValue::List(mut items) => {
            items.sort_by(|a, b| {
                let ka = f.call_closure(vec![a.clone()]);
                let kb = f.call_closure(vec![b.clone()]);
                anubis_value_cmp(&ka, &kb)
            });
            AnubisValue::List(items)
        }
        other => other,
    }
}
fn anubis_apply(f: AnubisValue, args: AnubisValue) -> AnubisValue {
    match args {
        AnubisValue::List(items) => f.call_closure(items),
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
    AnubisValue::Map(out)
}

/// Materialize a value's iteration elements: list items, string characters, or map keys.
fn anubis_iter(v: AnubisValue) -> Vec<AnubisValue> {
    match v {
        AnubisValue::List(items) => items,
        AnubisValue::Str(s) => s.chars().map(|c| AnubisValue::Str(c.to_string())).collect(),
        AnubisValue::Map(m) => m.into_iter().map(|(k, _)| AnubisValue::Str(k)).collect(),
        other => vec![other],
    }
}

// ---- math ----
fn anubis_sin(x: AnubisValue) -> AnubisValue { AnubisValue::Float(x.as_f64().sin()) }
fn anubis_cos(x: AnubisValue) -> AnubisValue { AnubisValue::Float(x.as_f64().cos()) }
fn anubis_tan(x: AnubisValue) -> AnubisValue { AnubisValue::Float(x.as_f64().tan()) }
fn anubis_asin(x: AnubisValue) -> AnubisValue { AnubisValue::Float(x.as_f64().asin()) }
fn anubis_acos(x: AnubisValue) -> AnubisValue { AnubisValue::Float(x.as_f64().acos()) }
fn anubis_atan(x: AnubisValue) -> AnubisValue { AnubisValue::Float(x.as_f64().atan()) }
fn anubis_atan2(y: AnubisValue, x: AnubisValue) -> AnubisValue { AnubisValue::Float(y.as_f64().atan2(x.as_f64())) }
fn anubis_exp(x: AnubisValue) -> AnubisValue { AnubisValue::Float(x.as_f64().exp()) }
fn anubis_ln(x: AnubisValue) -> AnubisValue { AnubisValue::Float(x.as_f64().ln()) }
fn anubis_log10(x: AnubisValue) -> AnubisValue { AnubisValue::Float(x.as_f64().log10()) }
fn anubis_log2(x: AnubisValue) -> AnubisValue { AnubisValue::Float(x.as_f64().log2()) }
fn anubis_logb(x: AnubisValue, base: AnubisValue) -> AnubisValue { AnubisValue::Float(x.as_f64().log(base.as_f64())) }
fn anubis_cbrt(x: AnubisValue) -> AnubisValue { AnubisValue::Float(x.as_f64().cbrt()) }
fn anubis_hypot(x: AnubisValue, y: AnubisValue) -> AnubisValue { AnubisValue::Float(x.as_f64().hypot(y.as_f64())) }
fn anubis_trunc(x: AnubisValue) -> AnubisValue { match x { AnubisValue::Int(n) => AnubisValue::Int(n), _ => AnubisValue::Int(x.as_f64().trunc() as i64) } }
fn anubis_sign(x: AnubisValue) -> AnubisValue { let v = x.as_f64(); AnubisValue::Int(if v > 0.0 { 1 } else if v < 0.0 { -1 } else { 0 }) }
fn anubis_clamp(x: AnubisValue, lo: AnubisValue, hi: AnubisValue) -> AnubisValue {
    if x.is_float() || lo.is_float() || hi.is_float() {
        AnubisValue::Float(x.as_f64().max(lo.as_f64()).min(hi.as_f64()))
    } else {
        AnubisValue::Int(x.as_i64().max(lo.as_i64()).min(hi.as_i64()))
    }
}
fn anubis_pi() -> AnubisValue { AnubisValue::Float(std::f64::consts::PI) }
fn anubis_e() -> AnubisValue { AnubisValue::Float(std::f64::consts::E) }
fn anubis_factorial(n: AnubisValue) -> AnubisValue {
    let n = n.as_i64().max(0);
    let mut acc: i64 = 1;
    let mut i = 2;
    while i <= n { acc = acc.wrapping_mul(i); i += 1; }
    AnubisValue::Int(acc)
}

// ---- strings ----
fn anubis_chars(s: AnubisValue) -> AnubisValue {
    AnubisValue::List(s.display_string().chars().map(|c| AnubisValue::Str(c.to_string())).collect())
}
fn anubis_words(s: AnubisValue) -> AnubisValue {
    AnubisValue::List(s.display_string().split_whitespace().map(|w| AnubisValue::Str(w.to_string())).collect())
}
fn anubis_lines(s: AnubisValue) -> AnubisValue {
    AnubisValue::List(s.display_string().lines().map(|l| AnubisValue::Str(l.to_string())).collect())
}
fn anubis_capitalize(s: AnubisValue) -> AnubisValue {
    let s = s.display_string();
    let mut ch = s.chars();
    match ch.next() {
        Some(f) => AnubisValue::Str(f.to_uppercase().collect::<String>() + &ch.as_str().to_lowercase()),
        None => AnubisValue::Str(String::new()),
    }
}
fn anubis_pad(s: AnubisValue, width: AnubisValue, pad: AnubisValue, at_start: bool) -> AnubisValue {
    let s = s.display_string();
    let w = width.as_i64().max(0) as usize;
    let p = { let ps = pad.display_string(); if ps.is_empty() { " ".to_string() } else { ps } };
    let have = s.chars().count();
    if have >= w { return AnubisValue::Str(s); }
    let mut fill = String::new();
    while fill.chars().count() < w - have { fill.push_str(&p); }
    let fill: String = fill.chars().take(w - have).collect();
    AnubisValue::Str(if at_start { format!("{}{}", fill, s) } else { format!("{}{}", s, fill) })
}

// ---- lists ----
fn anubis_zip(a: AnubisValue, b: AnubisValue) -> AnubisValue {
    let bv = anubis_iter(b);
    AnubisValue::List(anubis_iter(a).into_iter().zip(bv).map(|(x, y)| AnubisValue::List(vec![x, y])).collect())
}
fn anubis_enumerate(a: AnubisValue) -> AnubisValue {
    AnubisValue::List(anubis_iter(a).into_iter().enumerate().map(|(i, x)| AnubisValue::List(vec![AnubisValue::Int(i as i64), x])).collect())
}
fn anubis_flatten(a: AnubisValue) -> AnubisValue {
    let mut out = Vec::new();
    for x in anubis_iter(a) { for y in anubis_iter(x) { out.push(y); } }
    AnubisValue::List(out)
}
fn anubis_flat_map(a: AnubisValue, f: AnubisValue) -> AnubisValue {
    let mut out = Vec::new();
    for x in anubis_iter(a) { for y in anubis_iter(f.call_closure(vec![x])) { out.push(y); } }
    AnubisValue::List(out)
}
fn anubis_unique(a: AnubisValue) -> AnubisValue {
    let mut out: Vec<AnubisValue> = Vec::new();
    for x in anubis_iter(a) {
        // Deduplicate by structural equality (matching `==`), not display form: `1` and `"1"`
        // are distinct, while `1` and `1.0` are the same.
        if !out.iter().any(|y| anubis_value_eq(y, &x)) { out.push(x); }
    }
    AnubisValue::List(out)
}
fn anubis_take(a: AnubisValue, n: AnubisValue) -> AnubisValue {
    let n = n.as_i64().max(0) as usize;
    AnubisValue::List(anubis_iter(a).into_iter().take(n).collect())
}
fn anubis_drop(a: AnubisValue, n: AnubisValue) -> AnubisValue {
    let n = n.as_i64().max(0) as usize;
    AnubisValue::List(anubis_iter(a).into_iter().skip(n).collect())
}
fn anubis_take_while(a: AnubisValue, f: AnubisValue) -> AnubisValue {
    let mut out = Vec::new();
    for x in anubis_iter(a) {
        if f.call_closure(vec![x.clone()]).as_bool() { out.push(x); } else { break; }
    }
    AnubisValue::List(out)
}
fn anubis_drop_while(a: AnubisValue, f: AnubisValue) -> AnubisValue {
    let items = anubis_iter(a);
    let mut i = 0;
    while i < items.len() && f.call_closure(vec![items[i].clone()]).as_bool() { i += 1; }
    AnubisValue::List(items[i..].to_vec())
}
fn anubis_chunk(a: AnubisValue, n: AnubisValue) -> AnubisValue {
    let n = n.as_i64().max(1) as usize;
    AnubisValue::List(anubis_iter(a).chunks(n).map(|c| AnubisValue::List(c.to_vec())).collect())
}
fn anubis_window(a: AnubisValue, n: AnubisValue) -> AnubisValue {
    let n = n.as_i64().max(1) as usize;
    let items = anubis_iter(a);
    if items.len() < n { return AnubisValue::List(vec![]); }
    AnubisValue::List(items.windows(n).map(|w| AnubisValue::List(w.to_vec())).collect())
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
fn anubis_first(a: AnubisValue) -> AnubisValue { anubis_iter(a).into_iter().next().unwrap_or(AnubisValue::Int(0)) }
fn anubis_last(a: AnubisValue) -> AnubisValue { anubis_iter(a).into_iter().last().unwrap_or(AnubisValue::Int(0)) }
fn anubis_concat(a: AnubisValue, b: AnubisValue) -> AnubisValue {
    let mut out = anubis_iter(a);
    out.extend(anubis_iter(b));
    AnubisValue::List(out)
}
fn anubis_min_by(a: AnubisValue, f: AnubisValue) -> AnubisValue {
    anubis_iter(a).into_iter()
        .min_by(|x, y| anubis_value_cmp(&f.call_closure(vec![x.clone()]), &f.call_closure(vec![y.clone()])))
        .unwrap_or(AnubisValue::Int(0))
}
fn anubis_max_by(a: AnubisValue, f: AnubisValue) -> AnubisValue {
    anubis_iter(a).into_iter()
        .max_by(|x, y| anubis_value_cmp(&f.call_closure(vec![x.clone()]), &f.call_closure(vec![y.clone()])))
        .unwrap_or(AnubisValue::Int(0))
}
fn anubis_partition(a: AnubisValue, f: AnubisValue) -> AnubisValue {
    let mut yes = Vec::new();
    let mut no = Vec::new();
    for x in anubis_iter(a) {
        if f.call_closure(vec![x.clone()]).as_bool() { yes.push(x); } else { no.push(x); }
    }
    AnubisValue::List(vec![AnubisValue::List(yes), AnubisValue::List(no)])
}

// ---- maps ----
fn anubis_entries(m: AnubisValue) -> AnubisValue {
    match m {
        AnubisValue::Map(m) => AnubisValue::List(m.into_iter().map(|(k, v)| AnubisValue::List(vec![AnubisValue::Str(k), v])).collect()),
        _ => AnubisValue::List(vec![]),
    }
}
fn anubis_get(m: AnubisValue, k: AnubisValue, default: AnubisValue) -> AnubisValue {
    match &m {
        AnubisValue::Map(mm) => {
            let key = k.display_string();
            mm.iter().find(|(kk, _)| kk == &key).map(|(_, v)| v.clone()).unwrap_or(default)
        }
        _ => default,
    }
}
fn anubis_merge(a: AnubisValue, b: AnubisValue) -> AnubisValue {
    let mut out = match a { AnubisValue::Map(m) => m, _ => vec![] };
    if let AnubisValue::Map(bm) = b {
        for (k, v) in bm {
            if let Some(slot) = out.iter_mut().find(|(kk, _)| kk == &k) { slot.1 = v; } else { out.push((k, v)); }
        }
    }
    AnubisValue::Map(out)
}
fn anubis_map_values(m: AnubisValue, f: AnubisValue) -> AnubisValue {
    match m {
        AnubisValue::Map(mm) => AnubisValue::Map(mm.into_iter().map(|(k, v)| (k, f.call_closure(vec![v]))).collect()),
        other => other,
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
    let n = n.as_i64().max(0);
    AnubisValue::List((0..n).map(|i| f.call_closure(vec![AnubisValue::Int(i)])).collect())
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
            for item in items {
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
        AnubisValue::List(items) => items.iter().map(|x| (x.as_i64() as u8)).collect(),
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

fn anubis_p8(v: AnubisValue) -> AnubisValue {
    AnubisValue::List(vec![AnubisValue::Int((v.as_i64() as u8) as i64)])
}
fn anubis_p16(v: AnubisValue) -> AnubisValue {
    let n = v.as_i64() as u16;
    AnubisValue::List(n.to_le_bytes().iter().map(|b| AnubisValue::Int(*b as i64)).collect())
}
fn anubis_p32(v: AnubisValue) -> AnubisValue {
    let n = v.as_i64() as u32;
    AnubisValue::List(n.to_le_bytes().iter().map(|b| AnubisValue::Int(*b as i64)).collect())
}
fn anubis_p64(v: AnubisValue) -> AnubisValue {
    let n = v.as_i64() as u64;
    AnubisValue::List(n.to_le_bytes().iter().map(|b| AnubisValue::Int(*b as i64)).collect())
}
fn anubis_cyclic(v: AnubisValue) -> AnubisValue {
    let n = v.as_i64().max(0) as usize;
    let alphabet = b"abcdefghijklmnopqrstuvwxyz";
    AnubisValue::List((0..n).map(|i| AnubisValue::Int(alphabet[i % alphabet.len()] as i64)).collect())
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

fn emit_safe_run_stmt(
    stmt: &Stmt,
    indent: usize,
    out: &mut String,
    ctx: &EmitCtx,
) -> Result<()> {
    let pad = "    ".repeat(indent);
    match stmt {
        Stmt::Let { name, init, .. } => {
            out.push_str(&format!(
                "{pad}let mut {} = {};\n",
                sanitize_ident(name)?,
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
            out.push_str(&format!("{pad}if {}.as_bool() {{\n", safe_run_expr(cond, ctx)?));
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
        Stmt::WhileLet { pattern, expr, body } => {
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
        Stmt::While { cond, body } => {
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
        Stmt::Loop { body } => {
            out.push_str(&format!("{pad}loop {{\n"));
            for stmt in body {
                emit_safe_run_stmt(stmt, indent + 1, out, ctx)?;
            }
            out.push_str(&format!("{pad}}}\n"));
            Ok(())
        }
        Stmt::For { var, source, body } => {
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
                        "{pad}    let mut {} = AnubisValue::Int({});\n",
                        v, iv
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
                        "{pad}for mut {} in anubis_iter({}) {{\n",
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
            | "read_file"
            | "write_file"
            | "open"
            | "write"
            | "send"
            | "connect"
            | "network_send"
            | "memcpy"
            | "sql"
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
            "len" | "pop" | "push" | "insert" | "remove" | "print" | "println" | "eprint"
                | "eprintln" | "return" | "break" | "continue"
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
        Expr::Match { scrutinee, arms, .. } => {
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
            pattern, scrutinee, then, else_, ..
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
            Stmt::While { cond, body } => {
                collect_free_expr(cond, bound, vars, callees);
                let mut b = bound.clone();
                collect_free_stmts(body, &mut b, vars, callees);
            }
            Stmt::WhileLet { pattern, expr, body } => {
                collect_free_expr(expr, bound, vars, callees);
                let mut b = bound.clone();
                for n in pattern.bound_names() {
                    b.insert(n);
                }
                collect_free_stmts(body, &mut b, vars, callees);
            }
            Stmt::Loop { body } => {
                let mut b = bound.clone();
                collect_free_stmts(body, &mut b, vars, callees);
            }
            Stmt::For { var, source, body } => {
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
        "range" if args.len() == 3 => {
            Ok(format!("anubis_range_step({}, {}, {})", args[0], args[1], args[2]))
        }
        "range" => Err(unsupported_run("`range` expects 2 or 3 arguments")),
        // maps
        "keys" => fixed("anubis_keys", callee, args, 1),
        "values" => fixed("anubis_values", callee, args, 1),
        "has_key" => fixed("anubis_has_key", callee, args, 2),
        // higher-order (closures)
        "map" => fixed("anubis_map", callee, args, 2),
        "filter" => fixed("anubis_filter", callee, args, 2),
        "reduce" => fixed("anubis_reduce", callee, args, 3),
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
            "anubis_pad({}, {}, AnubisValue::Str(\" \".to_string()), {})",
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
        let mac = if name.starts_with('e') { "eprintln" } else { "println" };
        return Ok(format!(
            "AnubisValue::Closure(std::rc::Rc::new(move |__args: Vec<AnubisValue>| -> AnubisValue {{ {mac}!(\"{{}}\", __args.iter().map(|a| a.display_string()).collect::<Vec<_>>().join(\" \")); AnubisValue::Int(0) }}))"
        ));
    }
    if name == "len" {
        return Ok(
            "AnubisValue::Closure(std::rc::Rc::new(move |__args: Vec<AnubisValue>| -> AnubisValue { __args[0usize].len_val() }))"
                .to_string(),
        );
    }
    // Any other stdlib builtin → a closure dispatching on argument count across every arity the
    // builtin accepts (probed through `emit_builtin_call`, e.g. `range` takes 2 or 3).
    {
        let mut arms = String::new();
        for k in 1..=6usize {
            let args: Vec<String> = (0..k).map(|i| format!("__args[{i}usize].clone()")).collect();
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
            "AnubisValue::Str({}.to_string())",
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
                let lowered = args
                    .iter()
                    .map(|a| safe_run_expr(a, ctx))
                    .collect::<Result<Vec<_>>>()?;
                return Ok(format!("anb_{}({})", sanitize_ident(callee)?, lowered.join(", ")));
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
                return Ok(format!(
                    "({}).len_val()",
                    safe_run_expr(a, ctx)?
                ));
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
                    ("push", 1) => {
                        Ok(format!("{{ {}.push_val({}); AnubisValue::Int(0) }}", var, rest[0]))
                    }
                    ("insert", 2) => {
                        Ok(format!("anubis_insert(&mut {}, {}, {})", var, rest[0], rest[1]))
                    }
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
                        return Err(unsupported_run(
                            "proof_commit_* requires (\"name\", value)",
                        ));
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
                    return Ok(format!(
                        "{fn_name}({}, {})",
                        rust_string_lit(&key)?,
                        val
                    ));
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
                    _ => Err(unsupported_run(format!("unknown proof input builtin `{callee}`"))),
                };
            }
            if is_poc_kit_builtin(callee) {
                if !ctx.allow_research {
                    return Err(unsupported_run(format!(
                        "PoC kit builtin `{callee}` requires `anubis run --allow-research`"
                    )));
                }
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
                        "AnubisValue::List(anubis_to_bytes(&{}).into_iter().map(|b| AnubisValue::Int(b as i64)).collect())",
                        lowered[0]
                    )),
                    "target_run" if lowered.len() == 2 => Ok(format!(
                        "anubis_target_run({}, {})",
                        lowered[0], lowered[1]
                    )),
                    _ => Err(unsupported_run(format!(
                        "PoC kit builtin `{callee}` arity mismatch"
                    ))),
                };
            }
            if is_non_run_builtin(callee) {
                if ctx.allow_research && matches!(callee.as_str(), "taint_source" | "declassify" | "sink")
                {
                    // Modeling no-ops in research execution path.
                    if callee == "taint_source" {
                        let a = args.first().map(|e| safe_run_expr(e, ctx)).transpose()?;
                        return Ok(a.unwrap_or_else(|| {
                            "AnubisValue::Str(\"tainted\".to_string())".into()
                        }));
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
                                lowered.get(k).cloned().unwrap_or_else(|| "AnubisValue::Int(0)".to_string()),
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
                        "__anb_recv.field_get({}).call_closure(vec![{}])",
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
            Ok(format!("AnubisValue::List(vec![{}])", lowered.join(", ")))
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
                .map(|n| format!("{}.to_string()", rust_string_lit(n).unwrap_or_else(|_| "\"\"".into())))
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
            Ok(format!(
                "if ({c}).as_bool() {{ {t} }} else {{ {e} }}"
            ))
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
            Ok(format!(
                "anubis_map_lit(vec![{}])",
                pairs.join(", ")
            ))
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
                binds.push_str(&format!(
                    "let mut {} = __args.get({}usize).cloned().unwrap_or(AnubisValue::Int(0)); ",
                    sanitize_ident(p)?,
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
        Expr::Index { base, index } => Ok(format!(
            "({}).index_get({})",
            safe_run_expr(base, ctx)?,
            safe_run_expr(index, ctx)?
        )),
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
            for (fname, fexpr) in fields {
                fs.push(format!(
                    "({}.to_string(), {})",
                    rust_string_lit(fname)?,
                    safe_run_expr(fexpr, ctx)?
                ));
            }
            Ok(format!(
                "AnubisValue::Struct {{ ty: {}.to_string(), fields: vec![{}] }}",
                rust_string_lit(name)?,
                fs.join(", ")
            ))
        }
        // Field read: struct / struct-enum-variant / map field.
        Expr::FieldAccess { base, field, .. } => Ok(format!(
            "({}).field_get({})",
            safe_run_expr(base, ctx)?,
            rust_string_lit(field)?
        )),
        Expr::TaintSource { label } if ctx.allow_research => Ok(format!(
            "AnubisValue::Str({}.to_string())",
            rust_string_lit(label)?
        )),
        Expr::Declassify { inner, .. } if ctx.allow_research => safe_run_expr(inner, ctx),
        // Runtime assertion: `assert(cond)` panics (fail-closed) when the condition is false.
        Expr::Assert(inner) => Ok(format!(
            "anubis_assert({})",
            safe_run_expr(inner, ctx)?
        )),
        // `assume(cond)` is a solver hint; at runtime it evaluates the expression and yields true.
        Expr::Assume(inner) => Ok(format!(
            "{{ let _ = {}; AnubisValue::Bool(true) }}",
            safe_run_expr(inner, ctx)?
        )),
        Expr::Tainted { .. }
        | Expr::Symbolic { .. }
        | Expr::Declassify { .. }
        | Expr::TaintSource { .. }
        | Expr::UnifiedBuffer { .. }
        | Expr::RawPtr { .. } => Err(unsupported_run(
            "research-only construct (tainted / symbolic / declassify / unified-buffer / raw \
             pointer) is not available in `anubis run`; use the check or prove path"
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
    let mut out = format!(
        "{{ let {m} = {scr}; let mut {r} = AnubisValue::Int(0); let mut {done} = false; "
    );
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
fn pattern_test_and_binds(
    pat: &crate::frontend::Pattern,
    scr: &str,
) -> Result<(String, String)> {
    use crate::frontend::Pattern;
    match pat {
        Pattern::Wildcard => Ok(("true".to_string(), String::new())),
        Pattern::Binding(name) => {
            let bn = sanitize_ident(name)?;
            Ok(("true".to_string(), format!("let mut {bn} = {scr}.clone(); ")))
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
            "AnubisValue::Str({}.to_string())",
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

/// Transpile an Anubis source to Rust, compile it with `rustc`, and execute the binary.
/// Returns the raw process `Output`. Fails closed if lowering or `rustc` fails.
pub fn compile_and_run_source(
    source: &str,
    allow_research: bool,
    args: &[String],
) -> Result<std::process::Output> {
    let ast = crate::frontend::parse_source(source).map_err(|e| anyhow!("parse: {}", e))?;
    let rust_source = lower_program_to_rust(&ast.items, allow_research)?;
    let dir = std::env::temp_dir().join(format!("anubis-run-{}", anubis_unique_suffix()));
    std::fs::create_dir_all(&dir)?;
    let rs = dir.join("anubis_run.rs");
    let exe = dir.join("anubis_run");
    std::fs::write(&rs, rust_source)?;
    let build = std::process::Command::new("rustc")
        .arg(&rs)
        .arg("--edition")
        .arg("2021")
        .arg("-o")
        .arg(&exe)
        .output()
        .map_err(|e| anyhow!("rustc spawn failed: {}", e))?;
    if !build.status.success() {
        let stderr = String::from_utf8_lossy(&build.stderr).to_string();
        let _ = std::fs::remove_dir_all(&dir);
        return Err(anyhow!("ANUBIS_UNSUPPORTED_NATIVE_LOWERING: rustc failed:\n{}", stderr));
    }
    let out = std::process::Command::new(&exe)
        .args(args)
        .output()
        .map_err(|e| anyhow!("run spawn failed: {}", e))?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(out)
}

#[cfg(test)]
mod run_tests {
    use super::*;

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

    #[test]
    fn arithmetic_precedence() {
        assert_eq!(run("fn main() { print(2 + 3 * 4 - 1); }"), "13");
    }

    #[test]
    fn recursion_fibonacci() {
        let src = "fn fib(n: u32) { if n < 2 { return n; } return fib(n-1) + fib(n-2); } \
                   fn main() { print(fib(10)); }";
        assert_eq!(run(src), "55");
    }

    #[test]
    fn while_loop_mutation() {
        let src = "fn main() { let i = 0; let s = 0; while i < 5 { s = s + i; i = i + 1; } print(s); }";
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
        assert_eq!(run("fn main() { print(match 5 { \"5\" => \"str\", 5 => \"int\", _ => \"no\" }) }"), "int");
        assert_eq!(run("fn main() { print(match \"5\" { 5 => \"int\", \"5\" => \"str\", _ => \"no\" }) }"), "str");
        assert_eq!(run("fn main() { print(match 1 { true => \"T\", 1 => \"one\", _ => \"no\" }) }"), "one");
        // …but same-kind literals still match, and int/float stay numerically comparable.
        assert_eq!(
            run("fn main() { print(match true { true => \"y\", _ => \"n\" }); \
                 print(match 5 { 5 => \"i\", _ => \"n\" }); \
                 print(match \"hi\" { \"hi\" => \"s\", _ => \"n\" }) }"),
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
        let src = "fn main() { print(contains([1, 2, 3], \"2\")); print(index_of([1, 2, 3], \"2\")); \
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
        let ast = crate::frontend::parse_source(
            "fn f(o) { match o { Some(v) => v } } fn main() { }",
        )
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
        let src = "fn dbl(n) { n * 2 } struct P { x: int, y: int } fn mk(a, b) { P { x: a, y: b } } \
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
        assert_eq!(run(src), "[[a, 1], [b, 2]]\n2\n-1\n10\n9\n11\n[0, 1, 4]\n42");
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
        let src = "fn main() { print(reduce([5, 1, 9, 3], max, 0)); print(apply(min, [5, 1, 9, 3])); \
                   print(map([[3, 1], [9, 2], [4, 8]], max)); }";
        assert_eq!(run(src), "9\n1\n[3, 9, 8]");
    }

    #[test]
    fn multi_arity_builtin_as_first_class_value() {
        // range accepts 2 or 3 args; a first-class reference dispatches on the actual count.
        let src = "fn main() { let r = range; print(apply(r, [1, 5])); print(apply(r, [0, 10, 3])); }";
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
        let src = "struct Bag { items: list } impl Bag { fn count(self) { len(self.items) + 100 } } \
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
        let src = "fn main() { let r = if 3 > 2 { let a = 10; let b = 5; a + b } else { 0 }; print(r); }";
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
        assert_eq!(run("fn main() { print(len(split(\"a,b,c\", \",\"))); }"), "3");
        assert_eq!(
            run("fn main() { print(join([\"a\", \"b\", \"c\"], \"-\")); }"),
            "a-b-c"
        );
        assert_eq!(
            run("fn main() { print(replace(\"aXbXc\", \"X\", \"_\")); }"),
            "a_b_c"
        );
        assert_eq!(run("fn main() { print(contains(\"hello\", \"ell\")); }"), "true");
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
        assert_eq!(run("fn main() { print(range(0, 10, 2)); }"), "[0, 2, 4, 6, 8]");
        assert_eq!(run("fn main() { print(slice([1, 2, 3, 4, 5], 1, 4)); }"), "[2, 3, 4]");
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
        let src = "fn main() { print(sort([9007199254740993, 9007199254740992, 9007199254740994])); }";
        assert_eq!(run(src), "[9007199254740992, 9007199254740993, 9007199254740994]");
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
        let src = "fn main() { let m = { \"a\": 1, \"a\": 2 }; print(m[\"a\"]); print(len(keys(m))); }";
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
        assert_eq!(run("fn main() { print(any([1, 2, 3], |x| x > 2)); }"), "true");
        assert_eq!(run("fn main() { print(all([1, 2, 3], |x| x > 0)); }"), "true");
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
            let got = String::from_utf8_lossy(&out.stdout).trim().replace('\n', "|");
            assert_eq!(got, expect, "output mismatch for {:?}", path);
            count += 1;
        }
        assert!(count >= 10, "expected >= 10 tour programs, ran {}", count);
    }

    #[test]
    fn research_path_compiles_poc_kit_runtime() {
        // Exercises the allow_research lowering so the PoC-kit runtime (which contains its own
        // exhaustive matches over AnubisValue) is compiled — guards against a missing variant arm.
        let out = compile_and_run_source(
            "fn main() { let p = p32(65); print(len(p)); }",
            true,
            &[],
        )
        .expect("compile+run research");
        assert!(
            out.status.success(),
            "research-path program failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "4");
    }
}
