#![allow(dead_code, unused_mut, unused_variables, unused_assignments, unreachable_code, unused_parens, unused_imports, non_snake_case, unused_braces)]
use risc0_zkvm::guest::env;
use std::collections::HashMap;
use std::sync::OnceLock;


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
                    Some(k) => AnubisValue::Str(chars[k].to_string()),
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
/// True when a collection has no elements (empty ⟺ `len == 0`, matching `len`'s type coverage).
/// Lets programs guard `pop`/`last`/index access without hand-writing `len(xs) > 0` everywhere.
fn anubis_is_empty(v: AnubisValue) -> AnubisValue {
    let n = match &v {
        AnubisValue::List(l) => l.len(),
        AnubisValue::Str(s) => s.chars().count(),
        AnubisValue::Map(m) => m.len(),
        AnubisValue::Struct { fields, .. } => fields.len(),
        AnubisValue::Enum { fields, .. } => fields.len(),
        _ => 0,
    };
    AnubisValue::Bool(n == 0)
}
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
                Some(idx) => AnubisValue::Str(chars[idx].to_string()),
                None => default,
            }
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

fn anb_factorial(mut n: AnubisValue) -> AnubisValue {
    if anubis_cmp("<=", n.clone(), AnubisValue::Int(1)).as_bool() {
        return AnubisValue::Int(1);
    }
    return anubis_mul(n.clone(), anb_factorial(anubis_sub(n.clone(), AnubisValue::Int(1))));
    AnubisValue::Int(0)
}

fn anb_main() -> AnubisValue {
    let mut n = anubis_proof_input_u32_val("n");
    return anb_factorial(n.clone());
    AnubisValue::Int(0)
}


fn main() {
    anubis_load_proof_inputs();
    let __anubis_result = anb_main();
    anubis_commit_journal(__anubis_result);
}
