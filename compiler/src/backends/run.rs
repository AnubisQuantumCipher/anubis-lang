//! Anubis run/transpile backend.
//!
//! Lowers a parsed Anubis program to a self-contained Rust program for native
//! execution (`anubis run`) or to a RISC0 zkVM guest (`anubis prove`). This is the
//! executable semantics of Anubis. It lives in the compiler crate (not the CLI) so the
//! whole language is unit-testable without the heavy risc0 workspace.

use crate::frontend::{Expr, Item, Stmt};
use anyhow::{anyhow, Result};

/// A borrowed view of one Anubis function: (name, params, body).
type FnDef<'a> = (&'a str, &'a [(String, String)], &'a [Stmt]);

/// Recursively collect every `fn` item (including inside modules) as (name, params, body).
fn collect_fns<'a>(items: &'a [Item], out: &mut Vec<FnDef<'a>>) {
    for item in items {
        match item {
            Item::Fn {
                name, params, body, ..
            } => out.push((name.as_str(), params.as_slice(), body.as_slice())),
            Item::Module { items, .. } => collect_fns(items, out),
            _ => {}
        }
    }
}

/// Emit one Anubis function as a Rust function returning `AnubisValue`.
/// The trailing `AnubisValue::Int(0)` is the implicit return for functions that
/// fall off the end without an explicit `return`.
fn emit_fn(
    name: &str,
    params: &[(String, String)],
    body: &[Stmt],
    allow_research: bool,
) -> Result<String> {
    let mut sig = Vec::new();
    for (p, _ty) in params {
        sig.push(format!("mut {}: AnubisValue", sanitize_ident(p)?));
    }
    let mut body_src = String::new();
    for stmt in body {
        emit_safe_run_stmt(stmt, 1, &mut body_src, allow_research)?;
    }
    Ok(format!(
        "fn anb_{}({}) -> AnubisValue {{\n{}    AnubisValue::Int(0)\n}}\n",
        sanitize_ident(name)?,
        sig.join(", "),
        body_src,
    ))
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
    if !fns.iter().any(|(name, _, _)| *name == "main") {
        return Err(unsupported_run("program has no `fn main()` to run"));
    }
    let mut functions_src = String::new();
    for (name, params, body) in &fns {
        functions_src.push_str(&emit_fn(name, params, body, allow_research)?);
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
        header = "#![allow(dead_code, unused_mut, unused_variables, unused_assignments, unreachable_code, unused_parens, unused_imports)]\n",
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
#[derive(Clone, Debug)]
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
}

impl AnubisValue {
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
                if fields.is_empty() {
                    format!("{}::{}", ty, tag)
                } else if !field_names.is_empty() {
                    let parts: Vec<String> = field_names.iter().zip(fields.iter())
                        .map(|(n, v)| format!("{}: {}", n, v.display_string()))
                        .collect();
                    format!("{}::{} {{ {} }}", ty, tag, parts.join(", "))
                } else {
                    let parts: Vec<String> = fields.iter().map(|x| x.display_string()).collect();
                    format!("{}::{}({})", ty, tag, parts.join(", "))
                }
            }
            AnubisValue::Struct { ty, fields } => {
                let parts: Vec<String> = fields.iter()
                    .map(|(n, v)| format!("{}: {}", n, v.display_string()))
                    .collect();
                format!("{} {{ {} }}", ty, parts.join(", "))
            }
            AnubisValue::Map(m) => {
                let parts: Vec<String> = m.iter()
                    .map(|(k, v)| format!("{}: {}", k, v.display_string()))
                    .collect();
                format!("{{{}}}", parts.join(", "))
            }
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

fn anubis_cmp(op: &str, lhs: AnubisValue, rhs: AnubisValue) -> AnubisValue {
    let both_numeric = lhs.is_numeric() && rhs.is_numeric();
    let result = match op {
        "<" => lhs.as_f64() < rhs.as_f64(),
        "<=" => lhs.as_f64() <= rhs.as_f64(),
        ">" => lhs.as_f64() > rhs.as_f64(),
        ">=" => lhs.as_f64() >= rhs.as_f64(),
        "==" => if both_numeric { lhs.as_f64() == rhs.as_f64() } else { lhs.display_string() == rhs.display_string() },
        "!=" => if both_numeric { lhs.as_f64() != rhs.as_f64() } else { lhs.display_string() != rhs.display_string() },
        _ => false,
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
    allow_research: bool,
) -> Result<()> {
    let pad = "    ".repeat(indent);
    match stmt {
        Stmt::Let { name, init, .. } => {
            out.push_str(&format!(
                "{pad}let mut {} = {};\n",
                sanitize_ident(name)?,
                safe_run_expr(init, allow_research)?
            ));
            Ok(())
        }
        Stmt::Assign { target, value } => {
            let rhs = safe_run_expr(value, allow_research)?;
            match target {
                // Plain variable: direct rebinding (cheap, common case).
                Expr::Var(name) => {
                    out.push_str(&format!("{pad}{} = {};\n", sanitize_ident(name)?, rhs));
                }
                // Any nested place (`a[i]`, `a.b`, `a.b[i].c`, …): descend and set in place.
                _ => {
                    let (root, segs) = emit_place(target, allow_research)?;
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
                        safe_run_expr(&args[1], allow_research)?
                    ));
                    return Ok(());
                }
            }
            Err(unsupported_run(
                "push(list, value) requires a variable list as its first argument",
            ))
        }
        Stmt::ExprStmt(Expr::Call { callee, args }) if callee == "print" => {
            let arg = args
                .first()
                .ok_or_else(|| unsupported_run("print requires one argument"))?;
            out.push_str(&format!(
                "{pad}println!(\"{{}}\", {}.display_string());\n",
                safe_run_expr(arg, allow_research)?
            ));
            Ok(())
        }
        Stmt::ExprStmt(Expr::Call { callee, args }) if callee == "return" => {
            let val = match args.first() {
                Some(expr) => safe_run_expr(expr, allow_research)?,
                None => "AnubisValue::Int(0)".to_string(),
            };
            out.push_str(&format!("{pad}return {};\n", val));
            Ok(())
        }
        Stmt::ExprStmt(expr) => {
            out.push_str(&format!("{pad}let _ = {};\n", safe_run_expr(expr, allow_research)?));
            Ok(())
        }
        Stmt::If { cond, then, else_ } => {
            out.push_str(&format!("{pad}if {}.as_bool() {{\n", safe_run_expr(cond, allow_research)?));
            for stmt in then {
                emit_safe_run_stmt(stmt, indent + 1, out, allow_research)?;
            }
            out.push_str(&format!("{pad}}}"));
            if let Some(else_body) = else_ {
                out.push_str(" else {\n");
                for stmt in else_body {
                    emit_safe_run_stmt(stmt, indent + 1, out, allow_research)?;
                }
                out.push_str(&format!("{pad}}}\n"));
            } else {
                out.push('\n');
            }
            Ok(())
        }
        Stmt::While { cond, body } => {
            out.push_str(&format!(
                "{pad}while {}.as_bool() {{\n",
                safe_run_expr(cond, allow_research)?
            ));
            for stmt in body {
                emit_safe_run_stmt(stmt, indent + 1, out, allow_research)?;
            }
            out.push_str(&format!("{pad}}}\n"));
            Ok(())
        }
        Stmt::Loop { body } => {
            out.push_str(&format!("{pad}loop {{\n"));
            for stmt in body {
                emit_safe_run_stmt(stmt, indent + 1, out, allow_research)?;
            }
            out.push_str(&format!("{pad}}}\n"));
            Ok(())
        }
        Stmt::For { var, source, body } => {
            use crate::frontend::ForSource;
            let v = sanitize_ident(var)?;
            match source {
                ForSource::Range { start, end } => {
                    // `for v in a..b` — half-open range, bound evaluated once.
                    let endtmp = format!("__anb_for_end_{}", indent);
                    out.push_str(&format!(
                        "{pad}let mut {} = {};\n",
                        v,
                        safe_run_expr(start, allow_research)?
                    ));
                    out.push_str(&format!(
                        "{pad}let {} = {};\n",
                        endtmp,
                        safe_run_expr(end, allow_research)?
                    ));
                    out.push_str(&format!(
                        "{pad}while anubis_cmp(\"<\", {}.clone(), {}.clone()).as_bool() {{\n",
                        v, endtmp
                    ));
                    for stmt in body {
                        emit_safe_run_stmt(stmt, indent + 1, out, allow_research)?;
                    }
                    out.push_str(&format!(
                        "{pad}    {} = anubis_add({}.clone(), AnubisValue::Int(1));\n",
                        v, v
                    ));
                    out.push_str(&format!("{pad}}}\n"));
                    Ok(())
                }
                ForSource::Collection { expr } => {
                    // `for v in xs` — index walk over list/string/map-keys.
                    let col = format!("__anb_for_col_{}", indent);
                    let idx = format!("__anb_for_i_{}", indent);
                    let len = format!("__anb_for_len_{}", indent);
                    // Maps iterate keys (as string list) so body can index values.
                    out.push_str(&format!(
                        "{pad}let {} = {{ let __c = {}; match &__c {{ AnubisValue::Map(_) => __c.map_keys(), _ => __c }} }};\n",
                        col,
                        safe_run_expr(expr, allow_research)?
                    ));
                    out.push_str(&format!(
                        "{pad}let mut {} = AnubisValue::Int(0);\n",
                        idx
                    ));
                    out.push_str(&format!(
                        "{pad}let {} = {}.len_val();\n",
                        len, col
                    ));
                    out.push_str(&format!(
                        "{pad}while anubis_cmp(\"<\", {}.clone(), {}.clone()).as_bool() {{\n",
                        idx, len
                    ));
                    out.push_str(&format!(
                        "{pad}    let mut {} = {}.index_get({}.clone());\n",
                        v, col, idx
                    ));
                    for stmt in body {
                        emit_safe_run_stmt(stmt, indent + 1, out, allow_research)?;
                    }
                    out.push_str(&format!(
                        "{pad}    {} = anubis_add({}.clone(), AnubisValue::Int(1));\n",
                        idx, idx
                    ));
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
            if !allow_research {
                return Err(unsupported_run(
                    "research/exploit blocks require `anubis run --allow-research`",
                ));
            }
            for stmt in body {
                emit_safe_run_stmt(stmt, indent, out, allow_research)?;
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
fn emit_place(target: &Expr, allow_research: bool) -> Result<(String, Vec<String>)> {
    match target {
        Expr::Var(name) => Ok((sanitize_ident(name)?, Vec::new())),
        Expr::FieldAccess { base, field, .. } => {
            let (root, mut segs) = emit_place(base, allow_research)?;
            segs.push(format!(
                "AnubisPathSeg::Field({}.to_string())",
                rust_string_lit(field)?
            ));
            Ok((root, segs))
        }
        Expr::Index { base, index } => {
            let (root, mut segs) = emit_place(base, allow_research)?;
            segs.push(format!(
                "AnubisPathSeg::Index({})",
                safe_run_expr(index, allow_research)?
            ));
            Ok((root, segs))
        }
        _ => Err(unsupported_run(
            "assignment target must be a variable, field access, or index place",
        )),
    }
}

fn safe_run_expr(expr: &Expr, allow_research: bool) -> Result<String> {
    match expr {
        Expr::Literal(value) => Ok(literal_to_anubis_value(value)),
        Expr::StrLiteral(s) => Ok(format!(
            "AnubisValue::Str({}.to_string())",
            rust_string_lit(s)?
        )),
        Expr::Var(name) => Ok(format!("{}.clone()", sanitize_ident(name)?)),
        Expr::Unary { op, expr } => {
            let inner = safe_run_expr(expr, allow_research)?;
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
            let lhs = safe_run_expr(lhs, allow_research)?;
            let rhs = safe_run_expr(rhs, allow_research)?;
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
            if callee == "len" {
                let a = args
                    .first()
                    .ok_or_else(|| unsupported_run("len requires one argument"))?;
                return Ok(format!(
                    "({}).len_val()",
                    safe_run_expr(a, allow_research)?
                ));
            }
            if is_proof_input_builtin(callee) {
                if callee == "proof_assert" {
                    if args.len() != 1 {
                        return Err(unsupported_run("proof_assert requires one condition"));
                    }
                    let c = safe_run_expr(&args[0], allow_research)?;
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
                    let val = safe_run_expr(&args[1], allow_research)?;
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
                if !allow_research {
                    return Err(unsupported_run(format!(
                        "PoC kit builtin `{callee}` requires `anubis run --allow-research`"
                    )));
                }
                let mut lowered = Vec::new();
                for arg in args {
                    lowered.push(safe_run_expr(arg, allow_research)?);
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
                if allow_research && matches!(callee.as_str(), "taint_source" | "declassify" | "sink")
                {
                    // Modeling no-ops in research execution path.
                    if callee == "taint_source" {
                        let a = args.first().map(|e| safe_run_expr(e, allow_research)).transpose()?;
                        return Ok(a.unwrap_or_else(|| {
                            "AnubisValue::Str(\"tainted\".to_string())".into()
                        }));
                    }
                    if let Some(first) = args.first() {
                        return safe_run_expr(first, allow_research);
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
                lowered.push(safe_run_expr(arg, allow_research)?);
            }
            Ok(format!(
                "anb_{}({})",
                sanitize_ident(callee)?,
                lowered.join(", ")
            ))
        }
        Expr::ArrayLiteral { elements } => {
            let mut lowered = Vec::new();
            for el in elements {
                lowered.push(safe_run_expr(el, allow_research)?);
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
                fs.push(safe_run_expr(f, allow_research)?);
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
        } => lower_match_expr(scrutinee, arms, allow_research),
        Expr::If {
            cond, then, else_, ..
        } => {
            let c = safe_run_expr(cond, allow_research)?;
            let t = safe_run_expr(then, allow_research)?;
            let e = safe_run_expr(else_, allow_research)?;
            Ok(format!(
                "if ({c}).as_bool() {{ {t} }} else {{ {e} }}"
            ))
        }
        Expr::MapLiteral { entries, .. } => {
            let mut pairs = Vec::new();
            for (k, v) in entries {
                let ks = safe_run_expr(k, allow_research)?;
                let vs = safe_run_expr(v, allow_research)?;
                pairs.push(format!("(({ks}).display_string(), {vs})"));
            }
            Ok(format!(
                "AnubisValue::Map(vec![{}])",
                pairs.join(", ")
            ))
        }
        // Block expression: run the statements, then yield the tail value (or Int(0)).
        Expr::Block { stmts, tail } => {
            let mut body = String::new();
            for s in stmts {
                emit_safe_run_stmt(s, 0, &mut body, allow_research)?;
            }
            let tail_src = match tail {
                Some(t) => safe_run_expr(t, allow_research)?,
                None => "AnubisValue::Int(0)".to_string(),
            };
            Ok(format!("{{ {} {} }}", body, tail_src))
        }
        Expr::Index { base, index } => Ok(format!(
            "({}).index_get({})",
            safe_run_expr(base, allow_research)?,
            safe_run_expr(index, allow_research)?
        )),
        Expr::Cast { expr, .. } => safe_run_expr(expr, allow_research),
        // Nominal struct construction: `Name { f: e, ... }`.
        Expr::StructLiteral { name, fields, .. } => {
            let mut fs = Vec::new();
            for (fname, fexpr) in fields {
                fs.push(format!(
                    "({}.to_string(), {})",
                    rust_string_lit(fname)?,
                    safe_run_expr(fexpr, allow_research)?
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
            safe_run_expr(base, allow_research)?,
            rust_string_lit(field)?
        )),
        Expr::TaintSource { label } if allow_research => Ok(format!(
            "AnubisValue::Str({}.to_string())",
            rust_string_lit(label)?
        )),
        Expr::Declassify { inner, .. } if allow_research => safe_run_expr(inner, allow_research),
        Expr::Tainted { .. }
        | Expr::Symbolic { .. }
        | Expr::Assume(_)
        | Expr::Assert(_)
        | Expr::Declassify { .. }
        | Expr::TaintSource { .. }
        | Expr::UnifiedBuffer { .. }
        | Expr::RawPtr { .. }
        | Expr::Other(_) => Err(unsupported_run(format!(
            "unsupported expression for run: {:?}",
            std::mem::discriminant(expr)
        ))),
    }
}

fn lower_match_expr(
    scrutinee: &Expr,
    arms: &[crate::frontend::MatchArm],
    allow_research: bool,
) -> Result<String> {
    use crate::frontend::Pattern;
    let scr = safe_run_expr(scrutinee, allow_research)?;
    // Nested if-else chain over enum tag / wildcard.
    let mut out = format!("{{ let __anb_m = {scr}; ");
    let mut first = true;
    for arm in arms {
        let body = safe_run_expr(&arm.body, allow_research)?;
        let cond = match &arm.pattern {
            Pattern::Wildcard => "true".to_string(),
            Pattern::EnumVariant {
                enum_name,
                variant,
                ..
            } => {
                format!(
                    "matches!(&__anb_m, AnubisValue::Enum {{ ty, tag, .. }} if ty == {} && tag == {})",
                    rust_string_lit(enum_name)?,
                    rust_string_lit(variant)?
                )
            }
        };
        if !first {
            out.push_str(" else ");
        }
        first = false;
        out.push_str(&format!("if {cond} {{ "));
        // Bind tuple / struct fields for EnumVariant arms.
        if let Pattern::EnumVariant {
            bindings,
            named_bindings,
            ..
        } = &arm.pattern
        {
            for (i, b) in bindings.iter().enumerate() {
                let bn = sanitize_ident(b)?;
                out.push_str(&format!(
                    "let mut {bn} = match &__anb_m {{ AnubisValue::Enum {{ fields, .. }} if fields.len() > {i} => fields[{i}].clone(), _ => AnubisValue::Int(0) }}; "
                ));
            }
            for (fname, bname) in named_bindings {
                let bn = sanitize_ident(bname)?;
                let fstr = rust_string_lit(fname)?;
                out.push_str(&format!(
                    "let mut {bn} = match &__anb_m {{ AnubisValue::Enum {{ fields, field_names, .. }} => {{ \
                        let mut __v = AnubisValue::Int(0); \
                        for (__i, __n) in field_names.iter().enumerate() {{ \
                            if __n == &{fstr} {{ if let Some(__f) = fields.get(__i) {{ __v = __f.clone(); }} break; }} \
                        }} \
                        __v \
                    }}, _ => AnubisValue::Int(0) }}; "
                ));
            }
        }
        out.push_str(&format!("{body} }}"));
    }
    if first {
        // no arms
        out.push_str("AnubisValue::Int(0)");
    } else {
        out.push_str(" else { AnubisValue::Int(0) }");
    }
    out.push_str(" }");
    Ok(out)
}

/// Lower a numeric/boolean literal's text to an `AnubisValue` constructor.
/// (String literals are handled separately via `Expr::StrLiteral`.)
fn literal_to_anubis_value(value: &str) -> String {
    if value == "true" || value == "false" {
        format!("AnubisValue::Bool({value})")
    } else if value.parse::<i64>().is_ok() {
        format!("AnubisValue::Int({value})")
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
    if valid {
        Ok(name.to_string())
    } else {
        Err(unsupported_run(format!("invalid identifier `{}`", name)))
    }
}

fn unsupported_run(detail: impl Into<String>) -> anyhow::Error {
    anyhow!("ANUBIS_UNSUPPORTED_NATIVE_LOWERING: {}", detail.into())
}

fn rust_string_lit(value: &str) -> Result<String> {
    serde_json::to_string(value).map_err(|e| anyhow!("string literal encode: {}", e))
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
}
