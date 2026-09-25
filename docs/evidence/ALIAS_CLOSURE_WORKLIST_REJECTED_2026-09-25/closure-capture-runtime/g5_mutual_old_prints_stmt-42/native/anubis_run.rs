#![allow(dead_code, unused_mut, unused_variables, unused_assignments, unreachable_code, unused_parens, unused_imports, non_snake_case, unused_braces)]
const __ANB_STACK_BUDGET: usize = 805306368;


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
            // A STRING key reads the field of that name. It used to be tried as a position first, and
            // a non-numeric string parses to 0, so `p["pub_n"]` returned the FIRST field (a secret
            // one, in the case that found it). Positions are for integer indexes only.
            AnubisValue::Struct { fields, .. } => {
                if let AnubisValue::Str(_) = &i {
                    let key = i.display_string();
                    return fields.iter().find(|(k, _)| k == &key).map(|(_, v)| v.clone()).unwrap_or(AnubisValue::Int(0));
                }
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


// Keychain / Secure Enclave bind for *non-exportable* capability tokens (native `anubis run` only).
//
// HONESTY (load-bearing):
// - Soft path always works: `__anubis_cap_ne_soft:<kind>:<nonce>`.
// - On macOS, mint tries Keychain generic-password storage (`__anubis_cap_ne_kc:…`) and, when
//   `ANUBIS_KEYCHAIN_SE=1`, a Secure Enclave–resident EC key handle (`__anubis_cap_ne_se:…`).
// - Success means "item/key created under the current process identity", NOT "signed app with
//   production SE ACL + attestation". Codesign + App Sandbox + keychain-access-groups still
//   required for host-enforced isolation (`apple_enforced_claim` remains false until signed).
// - Guest / non-macOS: soft only.

// Last NE mint bind mode for this process: "soft" | "kc" | "se" (not secret material).
static ANUBIS_LAST_NE_BIND: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

fn anubis_keychain_se_note_bind(mode: &str) {
    if let Ok(mut g) = ANUBIS_LAST_NE_BIND.lock() {
        *g = mode.to_string();
    }
}

/// Last `cap_acquire_nonexportable` bind mode for this process (`soft` / `kc` / `se`).
/// Does not take a token argument — not an export of capability material.
fn anubis_keychain_se_last_bind() -> AnubisValue {
    let s = ANUBIS_LAST_NE_BIND
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default();
    if s.is_empty() {
        anubis_mk_str("none".to_string())
    } else {
        anubis_mk_str(s)
    }
}

/// Probe result: 0 = soft-only, 1 = Keychain bind available, 2 = Secure Enclave path available.
fn anubis_keychain_se_probe() -> AnubisValue {
    AnubisValue::Int(anubis_keychain_se_probe_i64())
}

fn anubis_keychain_se_probe_i64() -> i64 {
    #[cfg(target_os = "macos")]
    {
        if anubis_kc_se_available() {
            return 2;
        }
        if anubis_kc_keychain_available() {
            return 1;
        }
    }
    0
}

/// Mint an exportable capability (no Keychain bind — software token only).
fn anubis_cap_acquire(kind: AnubisValue) -> AnubisValue {
    anubis_mk_str(format!("__anubis_cap:{}", kind.display_string()))
}

/// Mint a *non-exportable* capability: prefer Keychain/SE bind on macOS, soft fallback.
fn anubis_cap_acquire_nonexportable(kind: AnubisValue) -> AnubisValue {
    let k = kind.display_string();
    #[cfg(target_os = "macos")]
    {
        if std::env::var_os("ANUBIS_KEYCHAIN_SE")
            .map(|v| v != "0" && v != "false")
            .unwrap_or(false)
        {
            if let Ok(tok) = anubis_kc_mint_se(&k) {
                anubis_keychain_se_note_bind("se");
                return anubis_mk_str(tok);
            }
        }
        // Default on macOS: try Keychain bind (opt out with ANUBIS_KEYCHAIN_CAPS=0).
        let want_kc = std::env::var_os("ANUBIS_KEYCHAIN_CAPS")
            .map(|v| v != "0" && v != "false")
            .unwrap_or(true);
        if want_kc {
            if let Ok(tok) = anubis_kc_mint_keychain(&k) {
                anubis_keychain_se_note_bind("kc");
                return anubis_mk_str(tok);
            }
        }
    }
    let nonce = anubis_kc_nonce();
    anubis_keychain_se_note_bind("soft");
    anubis_mk_str(format!("__anubis_cap_ne_soft:{k}:{nonce}"))
}

/// Language peel is identity on the token value. Optional Keychain delete on export when
/// `ANUBIS_KEYCHAIN_DELETE_ON_EXPORT=1` and the token is a `kc:` / `se:` bind.
fn anubis_cap_export(cap: AnubisValue, _reason: AnubisValue) -> AnubisValue {
    #[cfg(target_os = "macos")]
    {
        if std::env::var_os("ANUBIS_KEYCHAIN_DELETE_ON_EXPORT")
            .map(|v| v == "1" || v == "true")
            .unwrap_or(false)
        {
            let s = cap.display_string();
            let _ = anubis_kc_delete_token(&s);
        }
    }
    cap
}

fn anubis_kc_nonce() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:x}-{}", t, std::process::id())
}

// ── macOS Security.framework FFI ──────────────────────────────────────────────

#[cfg(target_os = "macos")]
#[link(name = "Security", kind = "framework")]
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn SecItemAdd(attributes: *const std::ffi::c_void, result: *mut *const std::ffi::c_void) -> i32;
    fn SecItemDelete(query: *const std::ffi::c_void) -> i32;
    fn SecItemCopyMatching(
        query: *const std::ffi::c_void,
        result: *mut *const std::ffi::c_void,
    ) -> i32;
    fn SecKeyCreateRandomKey(
        parameters: *const std::ffi::c_void,
        error: *mut *const std::ffi::c_void,
    ) -> *const std::ffi::c_void;
    fn SecKeyCopyPublicKey(key: *const std::ffi::c_void) -> *const std::ffi::c_void;
    fn SecKeyCopyExternalRepresentation(
        key: *const std::ffi::c_void,
        error: *mut *const std::ffi::c_void,
    ) -> *const std::ffi::c_void;
    fn CFDictionaryCreate(
        allocator: *const std::ffi::c_void,
        keys: *const *const std::ffi::c_void,
        values: *const *const std::ffi::c_void,
        num_values: isize,
        key_callbacks: *const std::ffi::c_void,
        value_callbacks: *const std::ffi::c_void,
    ) -> *const std::ffi::c_void;
    fn CFStringCreateWithCString(
        alloc: *const std::ffi::c_void,
        c_str: *const i8,
        encoding: u32,
    ) -> *const std::ffi::c_void;
    fn CFDataCreate(
        alloc: *const std::ffi::c_void,
        bytes: *const u8,
        length: isize,
    ) -> *const std::ffi::c_void;
    fn CFDataGetLength(data: *const std::ffi::c_void) -> isize;
    fn CFDataGetBytePtr(data: *const std::ffi::c_void) -> *const u8;
    fn CFRelease(cf: *const std::ffi::c_void);
    fn CFBooleanGetValue(boolean: *const std::ffi::c_void) -> u8;
    static kCFBooleanTrue: *const std::ffi::c_void;
    static kCFTypeDictionaryKeyCallBacks: std::ffi::c_void;
    static kCFTypeDictionaryValueCallBacks: std::ffi::c_void;
    // Attribute keys (CFStringRef) — resolved at runtime via dlsym-style externs from Security.
    static kSecClass: *const std::ffi::c_void;
    static kSecClassGenericPassword: *const std::ffi::c_void;
    static kSecClassKey: *const std::ffi::c_void;
    static kSecAttrService: *const std::ffi::c_void;
    static kSecAttrAccount: *const std::ffi::c_void;
    static kSecValueData: *const std::ffi::c_void;
    static kSecReturnData: *const std::ffi::c_void;
    static kSecAttrIsPermanent: *const std::ffi::c_void;
    static kSecAttrApplicationTag: *const std::ffi::c_void;
    static kSecAttrKeyType: *const std::ffi::c_void;
    static kSecAttrKeyTypeECSECPrimeRandom: *const std::ffi::c_void;
    static kSecAttrKeySizeInBits: *const std::ffi::c_void;
    static kSecAttrTokenID: *const std::ffi::c_void;
    static kSecAttrTokenIDSecureEnclave: *const std::ffi::c_void;
    static kSecPrivateKeyAttrs: *const std::ffi::c_void;
    static kSecAttrAccessible: *const std::ffi::c_void;
    static kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly: *const std::ffi::c_void;
    static kSecAttrAccessGroup: *const std::ffi::c_void;
}

#[cfg(target_os = "macos")]
const kCFStringEncodingUTF8: u32 = 0x0800_0100;
#[cfg(target_os = "macos")]
const errSecSuccess: i32 = 0;
#[cfg(target_os = "macos")]
const errSecDuplicateItem: i32 = -25299;
#[cfg(target_os = "macos")]
const errSecItemNotFound: i32 = -25300;

#[cfg(target_os = "macos")]
fn anubis_kc_cfstr(s: &str) -> *const std::ffi::c_void {
    let c = std::ffi::CString::new(s).unwrap_or_default();
    unsafe { CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), kCFStringEncodingUTF8) }
}

#[cfg(target_os = "macos")]
fn anubis_kc_dict(pairs: &[(*const std::ffi::c_void, *const std::ffi::c_void)]) -> *const std::ffi::c_void {
    let keys: Vec<*const std::ffi::c_void> = pairs.iter().map(|(k, _)| *k).collect();
    let vals: Vec<*const std::ffi::c_void> = pairs.iter().map(|(_, v)| *v).collect();
    unsafe {
        CFDictionaryCreate(
            std::ptr::null(),
            keys.as_ptr(),
            vals.as_ptr(),
            pairs.len() as isize,
            &kCFTypeDictionaryKeyCallBacks as *const _ as *const std::ffi::c_void,
            &kCFTypeDictionaryValueCallBacks as *const _ as *const std::ffi::c_void,
        )
    }
}

#[cfg(target_os = "macos")]
fn anubis_kc_keychain_available() -> bool {
    // Smoke: add+delete a tiny probe item under a unique account.
    let acct = format!("anubis-probe-{}", anubis_kc_nonce());
    match anubis_kc_mint_keychain_account("probe", &acct) {
        Ok(tok) => {
            let _ = anubis_kc_delete_token(&tok);
            true
        }
        Err(_) => false,
    }
}

#[cfg(target_os = "macos")]
fn anubis_kc_se_available() -> bool {
    // Attempt SE key gen; delete immediately. Headless CI often fails → false.
    match anubis_kc_mint_se("probe") {
        Ok(tok) => {
            let _ = anubis_kc_delete_token(&tok);
            true
        }
        Err(_) => false,
    }
}

#[cfg(target_os = "macos")]
fn anubis_kc_mint_keychain(kind: &str) -> Result<String, i32> {
    let acct = format!("ne-{}-{}", kind, anubis_kc_nonce());
    anubis_kc_mint_keychain_account(kind, &acct)
}

#[cfg(target_os = "macos")]
fn anubis_kc_mint_keychain_account(kind: &str, account: &str) -> Result<String, i32> {
    unsafe {
        let service = anubis_kc_cfstr("anubis.capability.nonexportable");
        let acct = anubis_kc_cfstr(account);
        let payload = format!("kind={kind};pid={}", std::process::id());
        let data = CFDataCreate(
            std::ptr::null(),
            payload.as_ptr(),
            payload.len() as isize,
        );
        // Optional access group from signed-run path (ANUBIS_KEYCHAIN_ACCESS_GROUP=TEAM.anubis.capability).
        let group_env = std::env::var("ANUBIS_KEYCHAIN_ACCESS_GROUP").ok();
        let group_cf = group_env
            .as_ref()
            .map(|g| anubis_kc_cfstr(g));
        let mut pairs: Vec<(*const std::ffi::c_void, *const std::ffi::c_void)> = vec![
            (kSecClass, kSecClassGenericPassword),
            (kSecAttrService, service),
            (kSecAttrAccount, acct),
            (kSecValueData, data),
            (kSecAttrAccessible, kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly),
        ];
        if let Some(g) = group_cf {
            pairs.push((kSecAttrAccessGroup, g));
        }
        let attrs = anubis_kc_dict(&pairs);
        let status = SecItemAdd(attrs, std::ptr::null_mut());
        CFRelease(attrs);
        CFRelease(service);
        CFRelease(acct);
        CFRelease(data);
        if let Some(g) = group_cf {
            CFRelease(g);
        }
        if status == errSecSuccess || status == errSecDuplicateItem {
            Ok(format!("__anubis_cap_ne_kc:{account}"))
        } else {
            Err(status)
        }
    }
}

#[cfg(target_os = "macos")]
fn anubis_kc_mint_se(kind: &str) -> Result<String, i32> {
    unsafe {
        let tag_str = format!("anubis.ne.{kind}.{}", anubis_kc_nonce());
        let tag_data = CFDataCreate(
            std::ptr::null(),
            tag_str.as_ptr(),
            tag_str.len() as isize,
        );
        // bits as CFNumber — use a small helper via CFString for size to avoid CFNumber link complexity:
        // SecKeyCreateRandomKey accepts CFDictionary; kSecAttrKeySizeInBits as CFNumber is required.
        // Use 256-bit EC via integer CFNumber created from bytes — link CoreFoundation CFNumberCreate.
    }
    // Prefer a dedicated CFNumber path:
    anubis_kc_mint_se_inner(kind)
}

#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFNumberCreate(
        allocator: *const std::ffi::c_void,
        the_type: isize,
        value_ptr: *const std::ffi::c_void,
    ) -> *const std::ffi::c_void;
}

#[cfg(target_os = "macos")]
const kCFNumberSInt32Type: isize = 3;

#[cfg(target_os = "macos")]
fn anubis_kc_mint_se_inner(kind: &str) -> Result<String, i32> {
    unsafe {
        let tag_str = format!("anubis.ne.{kind}.{}", anubis_kc_nonce());
        let tag_data = CFDataCreate(
            std::ptr::null(),
            tag_str.as_ptr(),
            tag_str.len() as isize,
        );
        let bits: i32 = 256;
        let bits_num = CFNumberCreate(
            std::ptr::null(),
            kCFNumberSInt32Type,
            &bits as *const i32 as *const std::ffi::c_void,
        );
        let priv_attrs = anubis_kc_dict(&[
            (kSecAttrIsPermanent, kCFBooleanTrue),
            (kSecAttrApplicationTag, tag_data),
            (kSecAttrAccessible, kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly),
        ]);
        let params = anubis_kc_dict(&[
            (kSecAttrKeyType, kSecAttrKeyTypeECSECPrimeRandom),
            (kSecAttrKeySizeInBits, bits_num),
            (kSecAttrTokenID, kSecAttrTokenIDSecureEnclave),
            (kSecPrivateKeyAttrs, priv_attrs),
        ]);
        let mut err: *const std::ffi::c_void = std::ptr::null();
        let key = SecKeyCreateRandomKey(params, &mut err);
        CFRelease(params);
        CFRelease(priv_attrs);
        CFRelease(tag_data);
        CFRelease(bits_num);
        if key.is_null() {
            if !err.is_null() {
                CFRelease(err);
            }
            return Err(-1);
        }
        let pubk = SecKeyCopyPublicKey(key);
        let mut err2: *const std::ffi::c_void = std::ptr::null();
        let ext = if pubk.is_null() {
            std::ptr::null()
        } else {
            SecKeyCopyExternalRepresentation(pubk, &mut err2)
        };
        let mut hash_hex = String::new();
        if !ext.is_null() {
            let len = CFDataGetLength(ext) as usize;
            let ptr = CFDataGetBytePtr(ext);
            if !ptr.is_null() && len > 0 {
                let bytes = std::slice::from_raw_parts(ptr, len);
                // FNV-1a 64-bit fingerprint of public key bytes (not a crypto claim — handle id).
                let mut h: u64 = 0xcbf29ce484222325;
                for b in bytes {
                    h ^= *b as u64;
                    h = h.wrapping_mul(0x100000001b3);
                }
                hash_hex = format!("{h:016x}");
            }
            CFRelease(ext);
        }
        if !err2.is_null() {
            CFRelease(err2);
        }
        if !pubk.is_null() {
            CFRelease(pubk);
        }
        // Keep private key in SE keychain (permanent). Token is a handle, not the key material.
        CFRelease(key);
        if hash_hex.is_empty() {
            hash_hex = anubis_kc_nonce();
        }
        Ok(format!("__anubis_cap_ne_se:{kind}:{hash_hex}"))
    }
}

#[cfg(target_os = "macos")]
fn anubis_kc_delete_token(tok: &str) -> Result<(), i32> {
    if let Some(acct) = tok.strip_prefix("__anubis_cap_ne_kc:") {
        unsafe {
            let service = anubis_kc_cfstr("anubis.capability.nonexportable");
            let account = anubis_kc_cfstr(acct);
            let query = anubis_kc_dict(&[
                (kSecClass, kSecClassGenericPassword),
                (kSecAttrService, service),
                (kSecAttrAccount, account),
            ]);
            let status = SecItemDelete(query);
            CFRelease(query);
            CFRelease(service);
            CFRelease(account);
            if status == errSecSuccess || status == errSecItemNotFound {
                Ok(())
            } else {
                Err(status)
            }
        }
    } else if tok.starts_with("__anubis_cap_ne_se:") {
        // SE keys are permanent; best-effort delete by application tag is residual.
        Ok(())
    } else {
        Ok(())
    }
}

// ---- Cryptography via audited crates (RWC Ch16: don't roll your own) ----
// Native `anubis run` only. Crates: sha2, hmac, hkdf, chacha20poly1305, argon2,
// pbkdf2, getrandom, subtle, ed25519-dalek, x25519-dalek. Same AnubisValue surface
// as pure guest crypto for shared APIs; Ed25519 / X25519 / PHC are host-audited extras.
// Grounding: David Wong, Real-World Cryptography (Manning 2021).

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use x25519_dalek::{PublicKey as X25519Public, StaticSecret};

type HmacSha256 = Hmac<Sha256>;

fn anubis_hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

/// Canonical crypto bytes. List elements MUST be integers in 0..=255 — fail closed on
/// truncation (silent `as u8` was a real key-corruption footgun).
fn anubis_crypto_bytes(v: &AnubisValue) -> Vec<u8> {
    match v {
        AnubisValue::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, x) in items.iter().enumerate() {
                let n = x.as_i64();
                if n < 0 || n > 255 {
                    panic!(
                        "ANUBIS_CRYPTO_BYTE_RANGE: list element [{}] = {} not in 0..=255 \
                         (refusing silent truncation of key/nonce/tag material)",
                        i, n
                    );
                }
                out.push(n as u8);
            }
            out
        }
        AnubisValue::Str(s) => s.as_bytes().to_vec(),
        AnubisValue::Int(n) => {
            // Single integer is NOT key material — force callers to use byte lists / strings.
            panic!(
                "ANUBIS_CRYPTO_BYTES_KIND: bare integer {} is not accepted as crypto input; \
                 use a byte list [0..255, ...] or a string",
                n
            );
        }
        AnubisValue::Bool(_) => {
            panic!("ANUBIS_CRYPTO_BYTES_KIND: bool is not accepted as crypto input");
        }
        other => other.display_string().into_bytes(),
    }
}

fn anubis_bytes_list(bytes: &[u8]) -> AnubisValue {
    anubis_mk_list(bytes.iter().map(|b| AnubisValue::Int(*b as i64)).collect())
}

fn anubis_sha256_bytes(msg: Vec<u8>) -> [u8; 32] {
    let d = Sha256::digest(&msg);
    let mut out = [0u8; 32];
    out.copy_from_slice(&d);
    out
}

fn anubis_sha256(v: AnubisValue) -> AnubisValue {
    anubis_mk_str(anubis_hex_encode(&anubis_sha256_bytes(anubis_crypto_bytes(&v))))
}

/// Hex-encode arbitrary crypto bytes (for KATs / debugging — not a secret leak by itself).
fn anubis_bytes_hex(v: AnubisValue) -> AnubisValue {
    anubis_mk_str(anubis_hex_encode(&anubis_crypto_bytes(&v)))
}

fn anubis_sha256_bytes_val(v: AnubisValue) -> AnubisValue {
    anubis_bytes_list(&anubis_sha256_bytes(anubis_crypto_bytes(&v)))
}

fn anubis_hmac_sha256_raw(key: &[u8], msg: &[u8]) -> [u8; 32] {
    // RFC 2104: any key length is valid. Fail closed if the library rejects the key —
    // never silently substitute a zero key (that would authenticate under a known key).
    let mut mac = <HmacSha256 as Mac>::new_from_slice(key).unwrap_or_else(|e| {
        panic!("ANUBIS_CRYPTO_HMAC_KEY: {}", e);
    });
    Mac::update(&mut mac, msg);
    let result = mac.finalize().into_bytes();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

fn anubis_hmac_sha256(key: AnubisValue, msg: AnubisValue) -> AnubisValue {
    let tag = anubis_hmac_sha256_raw(&anubis_crypto_bytes(&key), &anubis_crypto_bytes(&msg));
    anubis_mk_str(anubis_hex_encode(&tag))
}

fn anubis_hmac_sha256_bytes(key: AnubisValue, msg: AnubisValue) -> AnubisValue {
    let tag = anubis_hmac_sha256_raw(&anubis_crypto_bytes(&key), &anubis_crypto_bytes(&msg));
    anubis_bytes_list(&tag)
}

fn anubis_ct_eq(a: AnubisValue, b: AnubisValue) -> AnubisValue {
    let aa = anubis_crypto_bytes(&a);
    let bb = anubis_crypto_bytes(&b);
    if aa.len() != bb.len() {
        return AnubisValue::Bool(false);
    }
    AnubisValue::Bool(bool::from(aa.ct_eq(&bb)))
}

fn anubis_hmac_sha256_verify(key: AnubisValue, msg: AnubisValue, tag: AnubisValue) -> AnubisValue {
    let expected = anubis_hmac_sha256_raw(&anubis_crypto_bytes(&key), &anubis_crypto_bytes(&msg));
    let got = {
        let t = anubis_crypto_bytes(&tag);
        if t.len() == 32 {
            t
        } else {
            let s = tag.display_string();
            let mut out = Vec::new();
            let chars: Vec<char> = s.chars().filter(|c| !c.is_whitespace()).collect();
            if chars.len() == 64 && chars.iter().all(|c| c.is_ascii_hexdigit()) {
                let mut i = 0;
                while i < 64 {
                    let byte = u8::from_str_radix(&format!("{}{}", chars[i], chars[i + 1]), 16)
                        .unwrap_or(0);
                    out.push(byte);
                    i += 2;
                }
            }
            out
        }
    };
    if got.len() != 32 {
        return AnubisValue::Bool(false);
    }
    AnubisValue::Bool(bool::from(expected.ct_eq(got.as_slice())))
}

fn anubis_hkdf_sha256(
    ikm: AnubisValue,
    salt: AnubisValue,
    info: AnubisValue,
    length: AnubisValue,
) -> AnubisValue {
    use hkdf::Hkdf;
    let ikm_b = anubis_crypto_bytes(&ikm);
    let salt_b = anubis_crypto_bytes(&salt);
    let info_b = anubis_crypto_bytes(&info);
    // RFC 5869 §2.3: L ∈ [1, 255*HashLen]. Prior code silently coerced negative L to 0 via
    // `.max(0)` and returned an empty byte list — a SILENT_WRONG that would feed a downstream
    // `ensures(len(key) == 32)` and let a contract hold "for the wrong reason" (the caller
    // sees an empty vec and never checks). Fail closed on non-positive length, matching
    // `anubis_random_bytes`'s posture. Kept parity with the pure lane so both host runtimes
    // enforce the same domain (a caller that verifies against pure and runs against audited —
    // or vice versa — cannot straddle a boundary at which one side silently accepts).
    let n_raw = length.as_i64();
    if n_raw < 1 {
        panic!(
            "ANUBIS_CRYPTO_HKDF_LENGTH: L must be >= 1 (RFC 5869), got {}",
            n_raw
        );
    }
    let n = n_raw as usize;
    if n > 255 * 32 {
        panic!(
            "ANUBIS_CRYPTO_HKDF_TOO_LONG: requested {} bytes (max {})",
            n,
            255 * 32
        );
    }
    let salt_opt: Option<&[u8]> = if salt_b.is_empty() {
        None
    } else {
        Some(salt_b.as_slice())
    };
    let hk = Hkdf::<Sha256>::new(salt_opt, &ikm_b);
    let mut okm = vec![0u8; n];
    if hk.expand(&info_b, &mut okm).is_err() {
        panic!("ANUBIS_CRYPTO_HKDF_EXPAND_FAILED");
    }
    anubis_bytes_list(&okm)
}

fn anubis_domain_hash(label: AnubisValue, data: AnubisValue) -> AnubisValue {
    let lab = anubis_crypto_bytes(&label);
    let dat = anubis_crypto_bytes(&data);
    if lab.len() > u32::MAX as usize || dat.len() > u32::MAX as usize {
        panic!("ANUBIS_CRYPTO_DOMAIN_HASH_TOO_LARGE");
    }
    let mut msg = Vec::with_capacity(1 + 4 + lab.len() + 4 + dat.len());
    msg.push(0x01);
    msg.extend_from_slice(&(lab.len() as u32).to_be_bytes());
    msg.extend_from_slice(&lab);
    msg.extend_from_slice(&(dat.len() as u32).to_be_bytes());
    msg.extend_from_slice(&dat);
    anubis_mk_str(anubis_hex_encode(&anubis_sha256_bytes(msg)))
}

/// TupleHash spirit (RWC Ch2): length-prefix each part so `H(a||b) ≠ H(ab)` ambiguity dies.
/// `parts` must be a list of strings or byte lists.
fn anubis_tuple_hash(label: AnubisValue, parts: AnubisValue) -> AnubisValue {
    let lab = anubis_crypto_bytes(&label);
    let AnubisValue::List(items) = parts else {
        panic!("ANUBIS_CRYPTO_TUPLE_HASH: parts must be a list");
    };
    if lab.len() > u32::MAX as usize || items.len() > u32::MAX as usize {
        panic!("ANUBIS_CRYPTO_TUPLE_HASH_TOO_LARGE");
    }
    let mut msg = Vec::new();
    msg.push(0x02); // domain version distinct from domain_hash
    msg.extend_from_slice(&(lab.len() as u32).to_be_bytes());
    msg.extend_from_slice(&lab);
    msg.extend_from_slice(&(items.len() as u32).to_be_bytes());
    for (i, p) in items.iter().enumerate() {
        let b = anubis_crypto_bytes(p);
        if b.len() > u32::MAX as usize {
            panic!("ANUBIS_CRYPTO_TUPLE_HASH_PART_TOO_LARGE: index {i}");
        }
        msg.extend_from_slice(&(b.len() as u32).to_be_bytes());
        msg.extend_from_slice(&b);
    }
    anubis_mk_str(anubis_hex_encode(&anubis_sha256_bytes(msg)))
}

/// 12-byte nonce from a 64-bit counter (RWC Ch4: unique per key). Layout: 4 zero bytes + BE u64.
/// Suitable for moderate sequential protocols; never reuse a counter under the same key.
fn anubis_aead_nonce_from_counter(counter: AnubisValue) -> AnubisValue {
    let c = counter.as_i64();
    if c < 0 {
        panic!("ANUBIS_CRYPTO_NONCE_COUNTER: counter must be >= 0");
    }
    let mut n = [0u8; 12];
    n[4..12].copy_from_slice(&(c as u64).to_be_bytes());
    anubis_bytes_list(&n)
}

fn anubis_random_bytes(n: AnubisValue) -> AnubisValue {
    let n_raw = n.as_i64();
    if n_raw < 0 {
        panic!("ANUBIS_CRYPTO_RANDOM_NEGATIVE_LENGTH: byte count must be non-negative, got {}", n_raw);
    }
    let n = n_raw as usize;
    if n > 1 << 20 {
        panic!("ANUBIS_CRYPTO_RANDOM_TOO_LARGE: max 1MiB per call");
    }
    let mut buf = vec![0u8; n];
    if let Err(e) = getrandom::getrandom(&mut buf) {
        panic!("ANUBIS_CRYPTO_RANDOM_FAILED: {}", e);
    }
    anubis_bytes_list(&buf)
}

fn anubis_aead_parse_key_nonce(key: &AnubisValue, nonce: &AnubisValue) -> ([u8; 32], [u8; 12]) {
    let kb = anubis_crypto_bytes(key);
    let nb = anubis_crypto_bytes(nonce);
    if kb.len() != 32 {
        panic!(
            "ANUBIS_CRYPTO_AEAD_KEY_LEN: ChaCha20-Poly1305 key must be 32 bytes, got {}",
            kb.len()
        );
    }
    if nb.len() != 12 {
        panic!(
            "ANUBIS_CRYPTO_AEAD_NONCE_LEN: nonce must be 12 bytes (RWC: unique per key), got {}",
            nb.len()
        );
    }
    let mut k = [0u8; 32];
    let mut n = [0u8; 12];
    k.copy_from_slice(&kb);
    n.copy_from_slice(&nb);
    (k, n)
}

fn anubis_aead_seal(
    key: AnubisValue,
    nonce: AnubisValue,
    aad: AnubisValue,
    plaintext: AnubisValue,
) -> AnubisValue {
    let (k, n) = anubis_aead_parse_key_nonce(&key, &nonce);
    let aad_b = anubis_crypto_bytes(&aad);
    let pt = anubis_crypto_bytes(&plaintext);
    let cipher = ChaCha20Poly1305::new((&k).into());
    let nonce = Nonce::from_slice(&n);
    let ct = cipher
        .encrypt(
            nonce,
            Payload {
                msg: &pt,
                aad: &aad_b,
            },
        )
        .unwrap_or_else(|_| panic!("ANUBIS_CRYPTO_AEAD_SEAL_FAILED"));
    anubis_bytes_list(&ct)
}

fn anubis_aead_open(
    key: AnubisValue,
    nonce: AnubisValue,
    aad: AnubisValue,
    ciphertext_and_tag: AnubisValue,
) -> AnubisValue {
    let (k, n) = anubis_aead_parse_key_nonce(&key, &nonce);
    let aad_b = anubis_crypto_bytes(&aad);
    let blob = anubis_crypto_bytes(&ciphertext_and_tag);
    if blob.len() < 16 {
        panic!("ANUBIS_CRYPTO_AEAD_OPEN_FAILED: ciphertext shorter than tag");
    }
    let cipher = ChaCha20Poly1305::new((&k).into());
    let nonce = Nonce::from_slice(&n);
    match cipher.decrypt(
        nonce,
        Payload {
            msg: &blob,
            aad: &aad_b,
        },
    ) {
        Ok(pt) => anubis_bytes_list(&pt),
        Err(_) => panic!("ANUBIS_CRYPTO_AEAD_OPEN_FAILED: authentication tag mismatch (fail closed)"),
    }
}

// ---- Password hashing: argon2 + pbkdf2 crates (RWC Ch8) ----

fn anubis_pbkdf2_hmac_sha256_raw(
    password: &[u8],
    salt: &[u8],
    iterations: u32,
    dk_len: usize,
) -> Vec<u8> {
    use pbkdf2::pbkdf2_hmac;
    if iterations < 1 {
        panic!("ANUBIS_CRYPTO_PBKDF2_ITERATIONS: must be >= 1");
    }
    if dk_len > 1024 * 1024 {
        panic!("ANUBIS_CRYPTO_PBKDF2_TOO_LONG: max 1MiB");
    }
    if salt.is_empty() {
        panic!("ANUBIS_CRYPTO_PBKDF2_SALT: salt must be non-empty (prefer >= 16 bytes)");
    }
    let mut okm = vec![0u8; dk_len];
    pbkdf2_hmac::<Sha256>(password, salt, iterations, &mut okm);
    okm
}

fn anubis_pbkdf2_hmac_sha256(
    password: AnubisValue,
    salt: AnubisValue,
    iterations: AnubisValue,
    length: AnubisValue,
) -> AnubisValue {
    let iters = iterations.as_i64();
    if iters < 1 || iters > u32::MAX as i64 {
        panic!("ANUBIS_CRYPTO_PBKDF2_ITERATIONS: must be in 1..2^32-1");
    }
    // RFC 8018 §5.2 dkLen: must be a positive integer, at most (2^32-1)*hLen. Prior code
    // silently coerced non-positive length to 0 via `.max(0)` and returned an empty byte
    // list — same SILENT_WRONG shape closed on HKDF. Kept parity with the pure lane so both
    // host runtimes enforce the same domain (a caller that verifies against pure and runs
    // against audited — or vice versa — cannot straddle a boundary at which one side silently
    // accepts).
    let n_raw = length.as_i64();
    if n_raw < 1 {
        panic!(
            "ANUBIS_CRYPTO_PBKDF2_LENGTH: dkLen must be >= 1 (RFC 8018), got {}",
            n_raw
        );
    }
    let n = n_raw as usize;
    let dk = anubis_pbkdf2_hmac_sha256_raw(
        &anubis_crypto_bytes(&password),
        &anubis_crypto_bytes(&salt),
        iters as u32,
        n,
    );
    anubis_bytes_list(&dk)
}

fn anubis_argon2id_raw(
    pwd: &[u8],
    salt: &[u8],
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
    out_len: usize,
) -> Vec<u8> {
    use argon2::{Algorithm, Argon2, Params, Version};
    if salt.len() < 8 {
        panic!("ANUBIS_CRYPTO_ARGON2_SALT: salt must be >= 8 bytes (prefer 16)");
    }
    if out_len < 4 || out_len > 1024 {
        panic!("ANUBIS_CRYPTO_ARGON2_OUTLEN: must be 4..1024");
    }
    if m_cost < 8 || m_cost > 256 * 1024 {
        panic!("ANUBIS_CRYPTO_ARGON2_M: m_kib must be in 8..262144");
    }
    if t_cost < 1 || p_cost < 1 {
        panic!("ANUBIS_CRYPTO_ARGON2_PARAMS: t and p must be >= 1");
    }
    let params = Params::new(m_cost, t_cost, p_cost, Some(out_len)).unwrap_or_else(|e| {
        panic!("ANUBIS_CRYPTO_ARGON2_PARAMS: {}", e);
    });
    let a2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut out = vec![0u8; out_len];
    a2.hash_password_into(pwd, salt, &mut out)
        .unwrap_or_else(|e| panic!("ANUBIS_CRYPTO_ARGON2_FAILED: {}", e));
    out
}

fn anubis_argon2id_hash(
    password: AnubisValue,
    salt: AnubisValue,
    m_kib: AnubisValue,
    t: AnubisValue,
    p: AnubisValue,
    out_len: AnubisValue,
) -> AnubisValue {
    let m = m_kib.as_i64();
    let tt = t.as_i64();
    let pp = p.as_i64();
    let ol = out_len.as_i64();
    if m < 8 || m > 256 * 1024 {
        panic!("ANUBIS_CRYPTO_ARGON2_M: m_kib must be in 8..262144");
    }
    if tt < 1 || tt > 100 {
        panic!("ANUBIS_CRYPTO_ARGON2_T: time cost must be in 1..100");
    }
    if pp < 1 || pp > 16 {
        panic!("ANUBIS_CRYPTO_ARGON2_P: parallelism must be in 1..16");
    }
    if ol < 4 || ol > 1024 {
        panic!("ANUBIS_CRYPTO_ARGON2_OUTLEN: must be 4..1024");
    }
    let hash = anubis_argon2id_raw(
        &anubis_crypto_bytes(&password),
        &anubis_crypto_bytes(&salt),
        m as u32,
        tt as u32,
        pp as u32,
        ol as usize,
    );
    anubis_bytes_list(&hash)
}

fn anubis_hex_decode_loose(s: &str) -> Option<Vec<u8>> {
    let chars: Vec<char> = s.chars().filter(|c| !c.is_whitespace()).collect();
    if chars.len() % 2 != 0 {
        return None;
    }
    if !chars.iter().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = Vec::with_capacity(chars.len() / 2);
    let mut i = 0;
    while i < chars.len() {
        let byte = u8::from_str_radix(&format!("{}{}", chars[i], chars[i + 1]), 16).ok()?;
        out.push(byte);
        i += 2;
    }
    Some(out)
}

/// Production password hash via argon2 crate (Argon2id, OWASP-class params).
fn anubis_password_hash_encode(password: AnubisValue) -> AnubisValue {
    let salt_v = anubis_random_bytes(AnubisValue::Int(16));
    let salt = anubis_crypto_bytes(&salt_v);
    let hash = anubis_argon2id_raw(&anubis_crypto_bytes(&password), &salt, 19456, 2, 1, 32);
    let enc = format!(
        "anubis$argon2id$v=19$m=19456,t=2,p=1${}${}",
        anubis_hex_encode(&salt),
        anubis_hex_encode(&hash)
    );
    anubis_mk_str(enc)
}

fn anubis_password_hash_pbkdf2_encode(password: AnubisValue) -> AnubisValue {
    let salt_v = anubis_random_bytes(AnubisValue::Int(16));
    let salt = anubis_crypto_bytes(&salt_v);
    let hash = anubis_pbkdf2_hmac_sha256_raw(&anubis_crypto_bytes(&password), &salt, 600_000, 32);
    let enc = format!(
        "anubis$pbkdf2-sha256$i=600000${}${}",
        anubis_hex_encode(&salt),
        anubis_hex_encode(&hash)
    );
    anubis_mk_str(enc)
}

/// Standard PHC string (`$argon2id$v=19$m=…`) via the argon2 crate's PasswordHasher —
/// interoperable with other tools that speak PHC. Prefer this for long-lived password stores.
fn anubis_password_hash_phc(password: AnubisValue) -> AnubisValue {
    use argon2::{
        password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
        Algorithm, Argon2, Params, Version,
    };
    let pwd = anubis_crypto_bytes(&password);
    let salt = SaltString::generate(&mut OsRng);
    let params = Params::new(19456, 2, 1, None).unwrap_or_else(|e| {
        panic!("ANUBIS_CRYPTO_ARGON2_PARAMS: {}", e);
    });
    let a2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let hash = a2
        .hash_password(&pwd, &salt)
        .unwrap_or_else(|e| panic!("ANUBIS_CRYPTO_PASSWORD_HASH_PHC: {}", e));
    anubis_mk_str(hash.to_string())
}

fn anubis_password_verify_phc_raw(password: &[u8], encoding: &str) -> bool {
    use argon2::{
        password_hash::{PasswordHash, PasswordVerifier},
        Argon2,
    };
    let Ok(parsed) = PasswordHash::new(encoding) else {
        return false;
    };
    Argon2::default()
        .verify_password(password, &parsed)
        .is_ok()
}

// Re-bind password_verify to also accept standard PHC strings (`$argon2id$…`).
fn anubis_password_verify_encoding(password: AnubisValue, encoding: AnubisValue) -> AnubisValue {
    let enc = encoding.display_string();
    let pwd = anubis_crypto_bytes(&password);
    // Standard PHC (argon2 crate / passlib / libsodium interop)
    if enc.starts_with("$argon2") {
        return AnubisValue::Bool(anubis_password_verify_phc_raw(&pwd, &enc));
    }
    // anubis$… custom encodings (argon2id / pbkdf2-sha256)
    let parts: Vec<&str> = enc.split('$').collect();
    if parts.len() < 5 || parts[0] != "anubis" {
        return AnubisValue::Bool(false);
    }
    let algo = parts[1];
    let salt = match anubis_hex_decode_loose(parts[parts.len() - 2]) {
        Some(s) if !s.is_empty() => s,
        _ => return AnubisValue::Bool(false),
    };
    let expected = match anubis_hex_decode_loose(parts[parts.len() - 1]) {
        Some(h) if !h.is_empty() => h,
        _ => return AnubisValue::Bool(false),
    };
    let got = if algo == "argon2id" {
        if parts.len() < 6 {
            return AnubisValue::Bool(false);
        }
        let mut m: Option<u32> = None;
        let mut t: Option<u32> = None;
        let mut p: Option<u32> = None;
        for kv in parts[3].split(',') {
            if let Some(v) = kv.strip_prefix("m=") {
                m = v.parse().ok();
            } else if let Some(v) = kv.strip_prefix("t=") {
                t = v.parse().ok();
            } else if let Some(v) = kv.strip_prefix("p=") {
                p = v.parse().ok();
            }
        }
        let (Some(m), Some(t), Some(p)) = (m, t, p) else {
            return AnubisValue::Bool(false);
        };
        anubis_argon2id_raw(&pwd, &salt, m, t, p, expected.len())
    } else if algo == "pbkdf2-sha256" {
        let iters: u32 = match parts[2].strip_prefix("i=").and_then(|v| v.parse().ok()) {
            Some(i) if i >= 1 => i,
            _ => return AnubisValue::Bool(false),
        };
        anubis_pbkdf2_hmac_sha256_raw(&pwd, &salt, iters, expected.len())
    } else {
        return AnubisValue::Bool(false);
    };
    if got.len() != expected.len() {
        return AnubisValue::Bool(false);
    }
    AnubisValue::Bool(bool::from(got.ct_eq(expected.as_slice())))
}

// ---- Ed25519 (RWC / modern signatures — audited ed25519-dalek) ----

fn anubis_ed25519_keygen() -> AnubisValue {
    let mut seed = [0u8; 32];
    if let Err(e) = getrandom::getrandom(&mut seed) {
        panic!("ANUBIS_CRYPTO_ED25519_RNG: {}", e);
    }
    let sk = SigningKey::from_bytes(&seed);
    let pk = sk.verifying_key();
    // Return [secret_key_32, public_key_32] as nested byte lists
    anubis_mk_list(vec![
        anubis_bytes_list(sk.to_bytes().as_slice()),
        anubis_bytes_list(pk.as_bytes()),
    ])
}

fn anubis_ed25519_public_key(secret_key: AnubisValue) -> AnubisValue {
    let sk_b = anubis_crypto_bytes(&secret_key);
    if sk_b.len() != 32 {
        panic!(
            "ANUBIS_CRYPTO_ED25519_SK_LEN: secret key must be 32 bytes, got {}",
            sk_b.len()
        );
    }
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&sk_b);
    let sk = SigningKey::from_bytes(&seed);
    anubis_bytes_list(sk.verifying_key().as_bytes())
}

fn anubis_ed25519_sign(secret_key: AnubisValue, msg: AnubisValue) -> AnubisValue {
    let sk_b = anubis_crypto_bytes(&secret_key);
    if sk_b.len() != 32 {
        panic!(
            "ANUBIS_CRYPTO_ED25519_SK_LEN: secret key must be 32 bytes, got {}",
            sk_b.len()
        );
    }
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&sk_b);
    let sk = SigningKey::from_bytes(&seed);
    let sig = sk.sign(&anubis_crypto_bytes(&msg));
    anubis_bytes_list(sig.to_bytes().as_slice())
}

fn anubis_ed25519_verify(public_key: AnubisValue, msg: AnubisValue, signature: AnubisValue) -> AnubisValue {
    let pk_b = anubis_crypto_bytes(&public_key);
    let sig_b = anubis_crypto_bytes(&signature);
    if pk_b.len() != 32 {
        return AnubisValue::Bool(false);
    }
    if sig_b.len() != 64 {
        return AnubisValue::Bool(false);
    }
    let mut pk_arr = [0u8; 32];
    pk_arr.copy_from_slice(&pk_b);
    let Ok(pk) = VerifyingKey::from_bytes(&pk_arr) else {
        return AnubisValue::Bool(false);
    };
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sig_b);
    let sig = Signature::from_bytes(&sig_arr);
    AnubisValue::Bool(pk.verify(&anubis_crypto_bytes(&msg), &sig).is_ok())
}

/// Host runtime identity — useful for tests asserting audited path is live.
fn anubis_crypto_backend() -> AnubisValue {
    anubis_mk_str("audited-crates".into())
}

// ---- X25519 ECDH (RWC Ch5) + hybrid envelope (RWC Ch6 ECIES spirit) ----

fn anubis_x25519_from_sk_bytes(sk_b: &[u8]) -> StaticSecret {
    if sk_b.len() != 32 {
        panic!(
            "ANUBIS_CRYPTO_X25519_SK_LEN: secret key must be 32 bytes, got {}",
            sk_b.len()
        );
    }
    let mut seed = [0u8; 32];
    seed.copy_from_slice(sk_b);
    StaticSecret::from(seed)
}

fn anubis_x25519_from_pk_bytes(pk_b: &[u8]) -> X25519Public {
    if pk_b.len() != 32 {
        panic!(
            "ANUBIS_CRYPTO_X25519_PK_LEN: public key must be 32 bytes, got {}",
            pk_b.len()
        );
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(pk_b);
    X25519Public::from(arr)
}

fn anubis_x25519_keygen() -> AnubisValue {
    let mut seed = [0u8; 32];
    if let Err(e) = getrandom::getrandom(&mut seed) {
        panic!("ANUBIS_CRYPTO_X25519_RNG: {}", e);
    }
    let sk = StaticSecret::from(seed);
    let pk = X25519Public::from(&sk);
    anubis_mk_list(vec![
        anubis_bytes_list(sk.to_bytes().as_slice()),
        anubis_bytes_list(pk.as_bytes()),
    ])
}

fn anubis_x25519_public_key(secret_key: AnubisValue) -> AnubisValue {
    let sk = anubis_x25519_from_sk_bytes(&anubis_crypto_bytes(&secret_key));
    let pk = X25519Public::from(&sk);
    anubis_bytes_list(pk.as_bytes())
}

/// Raw Diffie–Hellman shared secret. RWC: never use raw shared as AEAD key — HKDF first.
fn anubis_x25519_shared(secret_key: AnubisValue, peer_public: AnubisValue) -> AnubisValue {
    let sk = anubis_x25519_from_sk_bytes(&anubis_crypto_bytes(&secret_key));
    let pk = anubis_x25519_from_pk_bytes(&anubis_crypto_bytes(&peer_public));
    let shared = sk.diffie_hellman(&pk);
    anubis_bytes_list(shared.as_bytes())
}

fn anubis_hybrid_derive_key(shared: &[u8], eph_pk: &[u8], recip_pk: &[u8]) -> [u8; 32] {
    use hkdf::Hkdf;
    // IKM = shared; salt = eph_pk || recip_pk (binds both static identities into the transcript).
    let mut salt = Vec::with_capacity(64);
    salt.extend_from_slice(eph_pk);
    salt.extend_from_slice(recip_pk);
    let hk = Hkdf::<Sha256>::new(Some(&salt), shared);
    let mut okm = [0u8; 32];
    if hk
        .expand(b"anubis-hybrid-v1|chacha20-poly1305", &mut okm)
        .is_err()
    {
        panic!("ANUBIS_CRYPTO_HYBRID_HKDF_FAILED");
    }
    okm
}

/// Hybrid seal (ECIES spirit, RWC Ch6): ephemeral X25519 + HKDF + ChaCha20-Poly1305.
/// Returns [eph_public_32, nonce_12, ciphertext_and_tag].
fn anubis_hybrid_seal(
    recipient_public: AnubisValue,
    aad: AnubisValue,
    plaintext: AnubisValue,
) -> AnubisValue {
    let recip_pk_b = anubis_crypto_bytes(&recipient_public);
    let recip_pk = anubis_x25519_from_pk_bytes(&recip_pk_b);
    let mut eph_seed = [0u8; 32];
    if let Err(e) = getrandom::getrandom(&mut eph_seed) {
        panic!("ANUBIS_CRYPTO_HYBRID_RNG: {}", e);
    }
    let eph_sk = StaticSecret::from(eph_seed);
    let eph_pk = X25519Public::from(&eph_sk);
    let shared = eph_sk.diffie_hellman(&recip_pk);
    let key = anubis_hybrid_derive_key(shared.as_bytes(), eph_pk.as_bytes(), recip_pk.as_bytes());
    let mut nonce = [0u8; 12];
    if let Err(e) = getrandom::getrandom(&mut nonce) {
        panic!("ANUBIS_CRYPTO_HYBRID_NONCE_RNG: {}", e);
    }
    let cipher = ChaCha20Poly1305::new((&key).into());
    let aad_b = anubis_crypto_bytes(&aad);
    let pt = anubis_crypto_bytes(&plaintext);
    let ct = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &pt,
                aad: &aad_b,
            },
        )
        .unwrap_or_else(|_| panic!("ANUBIS_CRYPTO_HYBRID_SEAL_FAILED"));
    anubis_mk_list(vec![
        anubis_bytes_list(eph_pk.as_bytes()),
        anubis_bytes_list(&nonce),
        anubis_bytes_list(&ct),
    ])
}

/// Hybrid open: recipient static secret + envelope fields from hybrid_seal.
fn anubis_hybrid_open(
    recipient_secret: AnubisValue,
    eph_public: AnubisValue,
    aad: AnubisValue,
    nonce: AnubisValue,
    ciphertext_and_tag: AnubisValue,
) -> AnubisValue {
    let recip_sk = anubis_x25519_from_sk_bytes(&anubis_crypto_bytes(&recipient_secret));
    let recip_pk = X25519Public::from(&recip_sk);
    let eph_pk_b = anubis_crypto_bytes(&eph_public);
    let eph_pk = anubis_x25519_from_pk_bytes(&eph_pk_b);
    let shared = recip_sk.diffie_hellman(&eph_pk);
    let key = anubis_hybrid_derive_key(shared.as_bytes(), eph_pk.as_bytes(), recip_pk.as_bytes());
    let (k, n) = {
        let nb = anubis_crypto_bytes(&nonce);
        if nb.len() != 12 {
            panic!(
                "ANUBIS_CRYPTO_HYBRID_NONCE_LEN: expected 12 bytes, got {}",
                nb.len()
            );
        }
        let mut nn = [0u8; 12];
        nn.copy_from_slice(&nb);
        (key, nn)
    };
    let cipher = ChaCha20Poly1305::new((&k).into());
    let aad_b = anubis_crypto_bytes(&aad);
    let blob = anubis_crypto_bytes(&ciphertext_and_tag);
    if blob.len() < 16 {
        panic!("ANUBIS_CRYPTO_HYBRID_OPEN_FAILED: ciphertext shorter than tag");
    }
    match cipher.decrypt(
        Nonce::from_slice(&n),
        Payload {
            msg: &blob,
            aad: &aad_b,
        },
    ) {
        Ok(pt) => anubis_bytes_list(&pt),
        Err(_) => panic!(
            "ANUBIS_CRYPTO_HYBRID_OPEN_FAILED: authentication tag mismatch (fail closed)"
        ),
    }
}


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

fn anb_main() -> AnubisValue {
    __anb_stack_guard();
    let mut p = AnubisValue::Struct { ty: "S".to_string(), fields: vec![("k".to_string(), anubis_field_require_int(AnubisValue::Int(42), "k")), ("pub_n".to_string(), anubis_field_require_int(AnubisValue::Int(3), "pub_n"))] };
    let mut h = AnubisValue::Closure(std::rc::Rc::new(move |__args: Vec<AnubisValue>| -> AnubisValue { let mut s = __args.get(0usize).cloned().unwrap_or(AnubisValue::Int(0)); { println!("{}", s.clone().display_string());
 AnubisValue::Int(0) } }));
    let mut g = AnubisValue::Closure(std::rc::Rc::new(move |__args: Vec<AnubisValue>| -> AnubisValue { let mut s = __args.get(0usize).cloned().unwrap_or(AnubisValue::Int(0)); AnubisValue::Int(1) }));
    g = { let h = h.clone(); AnubisValue::Closure(std::rc::Rc::new(move |__args: Vec<AnubisValue>| -> AnubisValue { let mut h = h.clone(); let mut s = __args.get(0usize).cloned().unwrap_or(AnubisValue::Int(0)); h.call_closure(vec![s.clone()]) })) };
    h = { let g = g.clone(); AnubisValue::Closure(std::rc::Rc::new(move |__args: Vec<AnubisValue>| -> AnubisValue { let mut g = g.clone(); let mut s = __args.get(0usize).cloned().unwrap_or(AnubisValue::Int(0)); g.call_closure(vec![s.clone()]) })) };
    g.call_closure(vec![p.field_get("k")])
}


fn main() {
    let child = std::thread::Builder::new()
        .stack_size(1024 * 1024 * 1024)
        .spawn(|| { let _ = anb_main(); })
        .expect("anubis: failed to spawn main thread");
    if child.join().is_err() { std::process::exit(101); }
}
