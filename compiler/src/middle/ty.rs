//! Structured type representation for Anubis.
//!
//! Phase 0 of the type-system overhaul (see the language roadmap). Two things live here:
//!
//! 1. A structured [`Ty`] enum with faithful [`Ty::parse`] / [`Ty::to_annotation`] round-tripping.
//!    This is the *foundation* the later phases build on — Phase 2 adds [`Ty::Var`]-based
//!    Hindley-Milner-lite inference and unification, Phase 3 uses [`Ty::Tainted`] as a real
//!    information-flow qualifier. Nothing in the checker consumes the structured form yet.
//!
//! 2. The type-reasoning predicates (`normalize`, `compatible`, `is_numeric`, `is_integer`,
//!    `is_generic`, `cast_preserves_i64`, `tainted_inner`, `bitwidth`) as the single source of
//!    truth. These are a *verbatim* relocation of the former free functions in `middle/mod.rs`;
//!    `middle/mod.rs` now delegates to them. Their behavior is a proven refinement of the old
//!    string logic — the `ty_parity` test in `middle/mod.rs` pins every predicate against a
//!    frozen copy of the original implementation over an exhaustive type-string matrix, so a
//!    future refactor that drifts from the historical semantics fails closed.
//!
//! The invariant for this phase: **no observable behavior change.** Every predicate returns
//! exactly what its `middle/mod.rs` ancestor returned.

// Phase-2 scaffolding (Var, Fn, Option/Result, unify) is defined now so the enum shape is stable
// for later phases, but is not yet consumed by the checker.
#![allow(dead_code)]

/// A structured Anubis type. Parsed from the raw `String` annotations the AST still carries;
/// renders back via [`Ty::to_annotation`]. Runtime is dynamically typed (`AnubisValue`), so `Ty`
/// is a *middle-end* analysis structure, never a codegen input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Ty {
    // Fixed-width unsigned integers that survive `normalize` unchanged.
    U8,
    U16,
    U32,
    U64,
    // Integer aliases that `normalize` collapses to `u32` (int, i8..i64, i128, u128, usize, isize,
    // number). The raw lowercased token is preserved because `cast_preserves_i64` and raw-equality
    // comparison distinguish e.g. `i64` from `i8`.
    IntAlias(String),
    // Floats. `f32`/`f64`/`float` are numeric but `normalize` leaves them as their lowercased token.
    Float(String),
    Bool,
    Str,
    List(Box<Ty>),
    Map(Box<Ty>, Box<Ty>),
    Struct(String),
    Enum(String),
    Fn(Vec<Ty>, Box<Ty>),
    Option(Box<Ty>),
    Result(Box<Ty>, Box<Ty>),
    /// A declared generic parameter (`T`, `U`) or a generic instantiation (`Opt<T>`). Erased at
    /// runtime; treated as compatible with everything until Phase 2 gives it real instantiation.
    Generic(String),
    /// Information-flow qualifier `tainted<T>` — a wrapper, not a distinct value type.
    Tainted(Box<Ty>),
    RawPtr {
        mutable: bool,
    },
    /// A unification variable. Phase 2 inference only; never produced by [`Ty::parse`].
    Var(u32),
    /// An absent annotation — dynamically typed, compatible with everything.
    Any,
    /// An unrecognized named type (enum/struct/opaque). The raw, original-case token is preserved
    /// so raw-equality comparisons (`e_raw == a_raw`) match the historical behavior.
    Named(String),
}

impl Ty {
    /// Parse a raw type annotation into structured form. Total: unrecognized input becomes
    /// [`Ty::Named`] (original case preserved), the empty string becomes [`Ty::Any`].
    pub(crate) fn parse(s: &str) -> Ty {
        let raw = s.trim();
        if raw.is_empty() {
            return Ty::Any;
        }
        let lower = raw.to_ascii_lowercase();

        // tainted<T> qualifier (case-insensitive tag, inner preserved).
        if lower.starts_with("tainted<") && lower.ends_with('>') {
            if let (Some(lt), Some(gt)) = (raw.find('<'), raw.rfind('>')) {
                let inner = raw[lt + 1..gt].trim();
                return Ty::Tainted(Box::new(Ty::parse(inner)));
            }
        }
        // Pointer forms.
        if raw.starts_with("*mut") {
            return Ty::RawPtr { mutable: true };
        }
        if raw.starts_with("*const") {
            return Ty::RawPtr { mutable: false };
        }
        // Generic parameter (`T`) or instantiation (`Opt<T>`) — matches `is_generic`.
        if raw.contains('<') || (raw.len() <= 2 && raw.chars().all(|c| c.is_ascii_uppercase())) {
            return Ty::Generic(raw.to_string());
        }
        match lower.as_str() {
            "u8" => Ty::U8,
            "u16" => Ty::U16,
            "u32" => Ty::U32,
            "u64" => Ty::U64,
            "int" | "i8" | "i16" | "i32" | "i64" | "i128" | "u128" | "usize" | "isize"
            | "number" => Ty::IntAlias(lower),
            "f32" | "f64" | "float" => Ty::Float(lower),
            "bool" | "boolean" => Ty::Bool,
            "str" | "string" => Ty::Str,
            "list" | "array" | "vec" => Ty::List(Box::new(Ty::Any)),
            "map" | "dict" | "dictionary" => Ty::Map(Box::new(Ty::Any), Box::new(Ty::Any)),
            _ => Ty::Named(raw.to_string()),
        }
    }

    /// Render back to a raw annotation string (round-trips [`Ty::parse`] up to the canonicalization
    /// that `parse` itself performs — e.g. `array` parses to a list and renders as `list`).
    pub(crate) fn to_annotation(&self) -> String {
        match self {
            Ty::U8 => "u8".into(),
            Ty::U16 => "u16".into(),
            Ty::U32 => "u32".into(),
            Ty::U64 => "u64".into(),
            Ty::IntAlias(s) => s.clone(),
            Ty::Float(s) => s.clone(),
            Ty::Bool => "bool".into(),
            Ty::Str => "string".into(),
            Ty::List(_) => "list".into(),
            Ty::Map(..) => "map".into(),
            Ty::Struct(n) | Ty::Enum(n) | Ty::Named(n) | Ty::Generic(n) => n.clone(),
            Ty::Fn(_, _) => "fn".into(),
            Ty::Option(inner) => format!("Option<{}>", inner.to_annotation()),
            Ty::Result(a, b) => {
                format!("Result<{}, {}>", a.to_annotation(), b.to_annotation())
            }
            Ty::Tainted(inner) => format!("tainted<{}>", inner.to_annotation()),
            Ty::RawPtr { mutable: true } => "*mut unknown".into(),
            Ty::RawPtr { mutable: false } => "*const unknown".into(),
            Ty::Var(n) => format!("?{}", n),
            Ty::Any => String::new(),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Type-reasoning predicates — the single source of truth. Verbatim relocation of the former
// `middle/mod.rs` free functions; `middle/mod.rs` delegates here. Kept string-in/string-out for
// this phase so behavior is byte-identical (proven by `ty_parity`); Phase 2 migrates the bodies
// to operate structurally on `Ty` while preserving these signatures.
// ---------------------------------------------------------------------------------------------

/// Canonicalize a type annotation: collapse the signed/wide integer family to `u32`, keep
/// `u8/u16/u32/u64`, unify string/list/map spellings, lowercase everything else.
pub(crate) fn normalize(ty: &str) -> String {
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

pub(crate) fn is_numeric(ty: &str) -> bool {
    matches!(
        normalize(ty).as_str(),
        "u8" | "u16" | "u32" | "u64" | "f32" | "f64" | "float"
    )
}

/// Strip a `tainted<T>` OR `secret<T>` information-flow qualifier to the base type `T`. Both are
/// pure LABELS — the underlying runtime value is `T` — so a `secret<i64>`/`tainted<i64>` param is a
/// modelable integer for the SOLVER while its confidentiality/integrity label lives separately. This
/// lets a contract be proved OVER secret data without leaking it (operator directive 2026-07-20: the
/// contracts+secrets combined demo). Non-qualified types pass through unchanged.
pub(crate) fn strip_flow_qualifier(ty: &str) -> &str {
    let inner = ty.trim();
    for q in ["tainted<", "secret<"] {
        if let Some(rest) = inner.strip_prefix(q) {
            return rest.strip_suffix('>').unwrap_or(rest);
        }
    }
    inner
}

/// Strip every whole-value `tainted<T>` / `secret<T>` wrapper, but only when each wrapper is a
/// balanced, single-argument generic. Unlike [`strip_flow_qualifier`], this helper is for nominal
/// security lookups and therefore returns `None` rather than guessing through malformed syntax.
pub(crate) fn strip_flow_qualifiers_exact(mut ty: &str) -> Option<&str> {
    loop {
        let current = ty.trim();
        if current.is_empty() {
            return None;
        }
        let lower = current.to_ascii_lowercase();
        let prefix_len = if lower.starts_with("tainted<") {
            "tainted<".len()
        } else if lower.starts_with("secret<") {
            "secret<".len()
        } else {
            return Some(current);
        };
        let inner = current[prefix_len..].strip_suffix('>')?.trim();
        if inner.is_empty() || !top_level_generic_commas(inner)?.is_empty() {
            return None;
        }
        ty = inner;
    }
}

/// Exact nominal registry key for a place type. Flow qualifiers are labels and are peeled; generic
/// struct arguments are retained only long enough to validate balanced syntax before returning the
/// bare declaration key (`secret<Box<i64>>` -> `Box`).
pub(crate) fn nominal_place_type_head(ty: &str) -> Option<&str> {
    let unqualified = strip_flow_qualifiers_exact(ty)?;
    if !generic_type_spelling_is_well_formed(unqualified) {
        return None;
    }
    let Some(open) = unqualified.find('<') else {
        return (!unqualified.contains('>')).then_some(unqualified);
    };
    let head = unqualified[..open].trim();
    let body = unqualified[open + 1..].strip_suffix('>')?.trim();
    if head.is_empty() || body.is_empty() {
        return None;
    }
    top_level_generic_commas(body)?;
    Some(head)
}

/// The ELEMENT type an index expression yields: `list<T>` → `T`, `map<K,V>` → `V` (indexing a map
/// yields its VALUE). Anything else → `None`, which is the conservative no-guess answer: an unknown
/// spelling preserves the caller's existing unknown-type behavior. That boundary avoids inventing
/// a nominal qualifier; it does not make an otherwise fail-open unknown place fail closed.
///
/// Added for `place_struct_type`'s `Expr::Index` arm. Without it, `Expr::Index` fell to `_ => None`
/// and the enclosing `FieldAccess`'s `?` short-circuited, so a declared `secret<T>`/`tainted<T>`
/// STRUCT FIELD read off a container element (`xs[0].k`, `m["a"].k`, `xs[0].inner.k`) was never
/// looked up and the qualifier went uncharged — runtime-proven, it printed the value.
/// See `docs/CLAIMS.md` item 21 root cause 8.
///
/// Nested generics are handled by tracking `<`/`>` depth so `map<string, list<S>>` yields
/// `list<S>` rather than splitting on the wrong comma.
pub(crate) fn container_element_type(ty: &str) -> Option<String> {
    let t = strip_flow_qualifiers_exact(ty)?;
    if !generic_type_spelling_is_well_formed(t) {
        return None;
    }
    if let Some(rest) = t.strip_prefix("list<").and_then(|r| r.strip_suffix('>')) {
        let inner = rest.trim();
        let commas = top_level_generic_commas(inner)?;
        return (!inner.is_empty() && commas.is_empty()).then(|| inner.to_string());
    }
    if let Some(rest) = t.strip_prefix("map<").and_then(|r| r.strip_suffix('>')) {
        let commas = top_level_generic_commas(rest)?;
        let [comma] = commas.as_slice() else {
            return None;
        };
        let key = rest[..*comma].trim();
        let value = rest[*comma + 1..].trim();
        return (!key.is_empty() && !value.is_empty()).then(|| value.to_string());
    }
    None
}

/// Return every comma at generic depth zero, or `None` for unbalanced angle delimiters. Keeping
/// malformed and multi-argument spellings out of `container_element_type` is security-relevant: a
/// partial parse must not invent the nominal type used to charge a declared field qualifier.
fn top_level_generic_commas(body: &str) -> Option<Vec<usize>> {
    let mut depth = 0usize;
    let mut commas = Vec::new();
    for (i, c) in body.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => {
                depth = depth.checked_sub(1)?;
            }
            ',' if depth == 0 => commas.push(i),
            _ => {}
        }
    }
    (depth == 0).then_some(commas)
}

/// Validate generic delimiter structure and require every argument slot at every nesting depth to
/// contain a well-formed type spelling. This is intentionally syntax-only: arity and declaration
/// existence are checked elsewhere, but an empty/doubled slot must never recover a nominal key.
fn generic_type_spelling_is_well_formed(ty: &str) -> bool {
    let ty = ty.trim();
    if ty.is_empty() {
        return false;
    }
    let Some(open) = ty.find('<') else {
        return !ty.chars().any(|c| matches!(c, '<' | '>' | ','));
    };
    let head = ty[..open].trim();
    let Some(body) = ty[open + 1..].strip_suffix('>') else {
        return false;
    };
    if head.is_empty() || head.chars().any(|c| matches!(c, '<' | '>' | ',')) {
        return false;
    }
    let Some(commas) = top_level_generic_commas(body) else {
        return false;
    };
    let mut start = 0usize;
    for end in commas.into_iter().chain(std::iter::once(body.len())) {
        if !generic_type_spelling_is_well_formed(&body[start..end]) {
            return false;
        }
        start = end + 1;
    }
    true
}

/// An INTEGER type the solver may soundly model as a 64-bit bit-vector (matching the i64 runtime).
/// Floats are deliberately excluded. `tainted<T>`/`secret<T>` are qualifiers — unwrap them first.
pub(crate) fn is_integer(ty: &str) -> bool {
    matches!(
        normalize(strip_flow_qualifier(ty)).as_str(),
        "u8" | "u16" | "u32" | "u64"
    )
}

/// A FLOAT type (`f32`/`f64`/`float`) — the complement of [`is_integer`] within [`is_numeric`].
/// Unwraps a `tainted<T>`/`secret<T>` qualifier first, exactly like [`is_integer`], so the two
/// partition the numeric types identically regardless of the flow-label wrapper.
pub(crate) fn is_float(ty: &str) -> bool {
    matches!(
        normalize(strip_flow_qualifier(ty)).as_str(),
        "f32" | "f64" | "float"
    )
}

/// The bit-width of a LITERALLY-UNSIGNED fixed-width integer annotation — `u8`→8, `u16`→16,
/// `u32`→32 — for the A1 boundary-coercion lane, else `None`. Deliberately narrow. It checks the RAW
/// spelling (unwrapping only a `tainted<>` qualifier), NOT [`normalize`], which collapses
/// `int`/`i64`/all signed types to `u32` — so a signed/default integer is NEVER range-injected (that
/// would be a false-accept: the runtime lets an `int` hold −1). It excludes `u64`, whose full range
/// [0, 2^64) does not fit the non-negative signed i64 range, so its masked value could not be compared
/// with the solver's SIGNED bv64 operators. Widths ≤ 32 land the masked value in [0, 2^32) ⊂ [0,
/// 2^63), where `bvsge`/`bvsle` are exact. A `Some(w)` result means the runtime masks this value to
/// [0, 2^w) at the param-entry boundary (`anubis_coerce_uint_param`), so the solver may soundly assume
/// `0 ≤ v < 2^w` inside the callee and when substituting the arg into the callee's contract.
pub(crate) fn unsigned_mask_width(ty: &str) -> Option<u32> {
    let inner = ty.trim();
    let inner = inner
        .strip_prefix("tainted<")
        .and_then(|r| r.strip_suffix('>'))
        .unwrap_or(inner);
    match inner.trim().to_ascii_lowercase().as_str() {
        "u8" => Some(8),
        "u16" => Some(16),
        "u32" => Some(32),
        _ => None,
    }
}

/// Width of a SIGNED narrow integer type (`i8`/`i16`/`i32`), else `None`. A `x as iN` cast keeps the
/// low `N` bits and REINTERPRETS the top bit as the sign (two's complement) — so it is modeled as a
/// sign-extension of the low `N` bits, matching `anubis_cast_int(.., signed=true)`. Distinct from
/// `unsigned_mask_width` (which zero-extends).
pub(crate) fn signed_narrow_width(ty: &str) -> Option<u32> {
    let inner = ty.trim();
    let inner = inner
        .strip_prefix("tainted<")
        .and_then(|r| r.strip_suffix('>'))
        .unwrap_or(inner);
    match inner.trim().to_ascii_lowercase().as_str() {
        "i8" => Some(8),
        "i16" => Some(16),
        "i32" => Some(32),
        _ => None,
    }
}

/// True when `x as ty` cannot change the underlying i64 value, so the cast may be modeled as the
/// identity in QF_BV.
pub(crate) fn cast_preserves_i64(ty: &str) -> bool {
    matches!(
        ty.trim().to_ascii_lowercase().as_str(),
        "u64" | "i64" | "int" | "integer" | "usize" | "isize" | "u128" | "i128"
    )
}

/// Whether an annotation is a generic type parameter (a short all-uppercase name like `T`) or a
/// generic instantiation (contains `<`, e.g. `Opt<T>`). Such types are erased at runtime.
pub(crate) fn is_generic(t: &str) -> bool {
    let t = t.trim();
    if t.contains('<') {
        return true;
    }
    !t.is_empty() && t.len() <= 2 && t.chars().all(|c| c.is_ascii_uppercase())
}

/// The success (`Ok`/`Some`) inner type of an `Option<T>`/`Result<T, E>` annotation — what the `?`
/// operator unwraps to. Returns `None` for any other type, so a `?` on an unknown/non-Result value
/// leaves the result untyped (dynamic) rather than mis-typed.
pub(crate) fn try_unwrap_ok(ty: &str) -> Option<String> {
    let t = ty.trim();
    let inner = t
        .strip_prefix("Option<")
        .or_else(|| t.strip_prefix("Result<"))
        .and_then(|rest| rest.strip_suffix('>'))?;
    // First top-level type argument (Result<T, E> -> T), respecting nested `<...>`.
    let mut depth = 0usize;
    for (i, c) in inner.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => return Some(inner[..i].trim().to_string()),
            _ => {}
        }
    }
    Some(inner.trim().to_string())
}

/// The inner type of a `tainted<T>` annotation, if any.
pub(crate) fn tainted_inner(ty: &str) -> Option<String> {
    let t = ty.trim();
    let lower = t.to_ascii_lowercase();
    if lower.starts_with("tainted<") && lower.ends_with('>') {
        let start = t.find('<')? + 1;
        let end = t.rfind('>')?;
        return Some(t[start..end].trim().to_string());
    }
    None
}

/// Whether a raw annotation carries the `tainted<T>` qualifier — as the whole annotation
/// (`tainted<u32>`) OR nested inside an outer container/generic (`list<tainted<u32>>`,
/// `Option<tainted<u32>>`, `Map<string, tainted<u32>>`).
///
/// ADVERSARIALLY CORRECTED (2026-07-11): the first version of this predicate delegated to
/// [`tainted_inner`]'s anchored "whole-string" guard (`starts_with("tainted<") && ends_with('>')`).
/// That is exactly right for `tainted_inner`'s own use inside [`compatible`] (a symmetric
/// annotation-compatibility question), but it is WRONG for this predicate's actual job — gating
/// whether `middle::is_tainted_type` seeds a param/let binding as tainted, which in turn gates the
/// `ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY` sink check. An adversarial workflow found that
/// delegating to the anchored guard is a real, parser-producible SECURITY REGRESSION: a parameter
/// declared `list<tainted<u32>>` was previously (correctly, if over-approximately) flagged by the
/// substring check this predicate replaced, but the anchored version stopped seeing it, silently
/// letting a genuinely tainted value reach a sink. `Ty::parse` cannot recurse into container/generic
/// inner types today (its List/Map/Option/Result/Generic arms don't retain them at all — a
/// materially larger, separate change), so the correct fix here is a substring check anchored on the
/// qualifier's own opening bracket (`"tainted<"`), not on the whole annotation.
///
/// This still fixes the ORIGINAL bug this predicate exists to fix — a type merely NAMED with the
/// substring "tainted" (`TaintedRecord`, `UntaintedBuffer`, `tainted_flag`) is NOT flagged, because
/// none of those have `<` immediately following the word "tainted". The one residual, deliberately
/// accepted edge case: a hypothetical FUTURE generic type whose name itself ends in "...tainted"
/// immediately before its own generic bracket (e.g. `SomeTainted<T>`) would still be over-flagged.
/// No such type exists anywhere in this codebase's corpus, and over-flagging (forcing an unnecessary
/// `declassify()`) is the SAFE direction for a security check — the opposite of the false negative
/// above, which is why this predicate deliberately leans toward over-approximation rather than
/// precision.
pub(crate) fn is_tainted(ty: &str) -> bool {
    ty.trim().to_ascii_lowercase().contains("tainted<")
}

/// Whether a declared type carries the `secret<T>` QUALIFIER — the confidentiality dual of
/// [`is_tainted`]. A `secret<T>`-annotated param or `let` is treated as a secret value WITHOUT an
/// explicit `secret_source(..)` seed, so `send(x)` on a `secret<T>` binding is exfiltration.
///
/// Anchored on `secret<` (not the bare word "secret") for the same reason [`is_tainted`] anchors on
/// `tainted<`: a variable named `secret_key`, a `SecretManager` struct, or a `secret_source(..)` call
/// is NOT a type qualifier and must not be flagged. This is a NEW sibling predicate — it is outside
/// the frozen `ty_parity` oracle (which pins `compatible`/`normalize`, never the qualifier
/// predicates), and `secret<T>` already type-checks via the generic-erasure path in [`compatible`],
/// so no change to the frozen surface is needed. It inherits the same deliberately-accepted, safe
/// (over-approximating) residual as [`is_tainted`]: a hypothetical future generic whose name ends in
/// "...secret" immediately before its own bracket (`MySecret<T>`) would be over-flagged; no such type
/// exists in the corpus, and over-flagging (forcing a `declassify`) is the safe direction.
pub(crate) fn is_secret(ty: &str) -> bool {
    ty.trim().to_ascii_lowercase().contains("secret<")
}

/// A+ compatibility: numeric widths interoperate; bool/string/enums do not cross. `tainted<T>` is
/// a qualifier: clean `T` may flow into a tainted binding, and tainted flows are still policed by
/// the separate taint analysis.
pub(crate) fn compatible(expected: &str, actual: &str) -> bool {
    let e_raw = expected.trim();
    let a_raw = actual.trim();
    // An absent annotation is dynamically typed.
    if e_raw.is_empty() || a_raw.is_empty() {
        return true;
    }
    // Generic parameters/instantiations are erased at runtime — compatible with anything.
    if is_generic(e_raw) || is_generic(a_raw) {
        return true;
    }
    let e = normalize(e_raw);
    let a = normalize(a_raw);
    if e == a || e_raw == a_raw {
        return true;
    }
    if e == "any" || a == "any" || e == "unknown" || a == "unknown" {
        return true;
    }
    // Pointer forms: any *mut/*const pair is compatible at this slice.
    if (e_raw.contains('*') || e.contains("rawptr"))
        && (a_raw.contains('*') || a.contains("rawptr"))
    {
        return true;
    }
    // tainted<T> ↔ T (qualifier, not a distinct value type for annotation matching)
    if let Some(inner) = tainted_inner(e_raw) {
        if compatible(&inner, a_raw) {
            return true;
        }
    }
    if let Some(inner) = tainted_inner(a_raw) {
        if compatible(e_raw, &inner) {
            return true;
        }
    }
    if is_numeric(&e) && is_numeric(&a) {
        return true;
    }
    false
}

/// Directional assignability: may a value of type `actual` be bound where type `expected` is
/// declared — a `let` initializer, an assignment to an annotated variable, a call argument, or a
/// return value — WITHOUT a lossy representation change?
///
/// This is [`compatible`] plus the one directional rule Phase 2 introduces: a **float value must
/// not narrow into an integer annotation.** `let x: u32 = 3.14` is a type lie — the runtime keeps
/// the float (annotations are inert), while the annotation claims an integer. Integer→float
/// widening (`let r: f64 = 3`) stays allowed, as does every integer width-interop and every
/// non-numeric case [`compatible`] already governs. `is_integer`/`is_float` both unwrap
/// `tainted<T>`, so `tainted<u32> = 3.14` is caught too.
///
/// Unlike [`compatible`] (a symmetric "could these interoperate" relation, pinned byte-for-byte to
/// the historical string logic by the `ty_parity` frozen oracle), this is deliberately asymmetric
/// and NEW — it is the first place the checker rejects on a structural, directional type rule.
/// It only ever refuses MORE than `compatible`, and only for a definitely-float value flowing into
/// a definitely-integer annotation; when the value's type is unknown the caller infers `None` and
/// never reaches here, so an undecidable program is still accepted.
pub(crate) fn assignable(expected: &str, actual: &str) -> bool {
    if !compatible(expected, actual) {
        return false;
    }
    !(is_integer(expected) && is_float(actual))
}

/// The solver bit-vector width for an integer type (defaults to 32). Substring-based to match the
/// historical behavior exactly.
pub(crate) fn bitwidth(ty: &str) -> u32 {
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

// =================================================================================================
// Bidirectional inference core — union-find over `Ty::Var` + `synth`/`check`/arm-join `unify`.
//
// This is the foundation the remaining type-system workstreams (generics, traits, typed `?`) build
// on, and it is deliberately kept SMALL and dependency-light so it can be re-expressed in Anubis
// itself in the port phase: a `Vec`-backed union-find, a plain-map environment (no closures, no
// trait objects), and a `synth` that is a flat recursive `match` over `Expr`.
//
// The absolute discipline is FAIL-CLOSED TOWARD ACCEPT: any type that resolves to `Any`, an unbound
// unification variable, a generic parameter, or an unknown widens to ACCEPT — a working dynamic
// program is never rejected on the checker's ignorance. Concrete-vs-concrete compatibility delegates
// to [`compatible`] (the same relation the rest of the checker uses), so the only new rejection power
// the core exposes is a genuine cross-category clash (`string` vs `u32`, an enum vs a different enum)
// surfaced through arm-join and check-direction — and even that lands in shadow mode first.
// =================================================================================================

use crate::frontend::{Expr, MatchArm};
use std::collections::BTreeMap;

/// The typing environment `synth`/`check` consult, as three plain borrowed maps. Kept as raw string
/// annotations (never `ScopeBinding`/`SemanticContext`) so the core stays decoupled from the
/// middle-end and trivial to port: `vars` is variable-name → annotation, `fns` is function-name →
/// declared return type, `structs` is struct-name → (field-name → field type).
pub(crate) struct InferEnv<'a> {
    pub vars: &'a BTreeMap<String, String>,
    pub fns: &'a BTreeMap<String, String>,
    pub structs: &'a BTreeMap<String, BTreeMap<String, String>>,
}

/// A union-find over `Ty::Var(id)`. Each `id` indexes `parent` (union-find links; `parent[i]==i` is a
/// root) and `binding` (per-root optional resolved type). By construction a `binding` entry is never
/// itself a `Ty::Var`, so [`InferCtx::resolve`] terminates in one hop after `find`.
pub(crate) struct InferCtx {
    parent: Vec<u32>,
    binding: Vec<Option<Ty>>,
}

impl Default for InferCtx {
    fn default() -> Self {
        Self::new()
    }
}

impl InferCtx {
    pub(crate) fn new() -> Self {
        InferCtx {
            parent: Vec::new(),
            binding: Vec::new(),
        }
    }

    /// Allocate a fresh, unbound unification variable.
    pub(crate) fn fresh(&mut self) -> Ty {
        let id = self.parent.len() as u32;
        self.parent.push(id);
        self.binding.push(None);
        Ty::Var(id)
    }

    fn find(&self, mut x: u32) -> u32 {
        while self.parent[x as usize] != x {
            x = self.parent[x as usize];
        }
        x
    }

    fn union(&mut self, x: u32, y: u32) {
        let rx = self.find(x);
        let ry = self.find(y);
        if rx == ry {
            return;
        }
        // Both roots are unbound whenever `union` runs (it is only reached from the Var–Var arm of
        // `unify`, where both sides resolved to bare vars); carrying `ry`'s binding is defensive.
        if self.binding[rx as usize].is_none() {
            self.binding[rx as usize] = self.binding[ry as usize].take();
        }
        self.parent[ry as usize] = rx;
    }

    fn bind(&mut self, x: u32, t: Ty) {
        let r = self.find(x);
        self.binding[r as usize] = Some(t);
    }

    /// Follow a type through the union-find: a bound var yields its (non-var) binding, an unbound var
    /// yields its representative `Ty::Var(root)`, and a non-var is returned unchanged.
    pub(crate) fn resolve(&self, ty: &Ty) -> Ty {
        match ty {
            Ty::Var(x) => {
                let r = self.find(*x);
                match &self.binding[r as usize] {
                    Some(t) => t.clone(),
                    None => Ty::Var(r),
                }
            }
            other => other.clone(),
        }
    }

    /// Unify two types, accept-biased. A unification variable binds; `Any` and a generic parameter
    /// absorb (they are compatible with everything, erased at runtime); two concretes agree exactly
    /// when [`compatible`] says so (numeric widths interoperate, `tainted<T>`↔`T`, pointers). The one
    /// `Err` case is a genuine cross-category clash of two concrete types — the new rejection power.
    /// On success returns the joined representative type.
    pub(crate) fn unify(&mut self, a: &Ty, b: &Ty) -> Result<Ty, (String, String)> {
        let a = self.resolve(a);
        let b = self.resolve(b);
        match (&a, &b) {
            (Ty::Var(x), Ty::Var(y)) => {
                if x != y {
                    self.union(*x, *y);
                }
                Ok(self.resolve(&a))
            }
            (Ty::Var(x), _) => {
                self.bind(*x, b.clone());
                Ok(b)
            }
            (_, Ty::Var(y)) => {
                self.bind(*y, a.clone());
                Ok(a)
            }
            // Dynamic / erased types absorb — the accept direction. `Any` on either side means the
            // checker cannot see a type, and a generic parameter is runtime-erased; neither may drive
            // a rejection, so the join is the *other* side (or `Any`).
            (Ty::Any, _) => Ok(b),
            (_, Ty::Any) => Ok(a),
            (Ty::Generic(_), _) | (_, Ty::Generic(_)) => Ok(Ty::Any),
            // Two concrete types: agree exactly when the historical compatibility relation says so.
            _ => {
                if compatible(&a.to_annotation(), &b.to_annotation()) {
                    Ok(a)
                } else {
                    Err((a.to_annotation(), b.to_annotation()))
                }
            }
        }
    }
}

/// SYNTH direction: produce a type for `expr`. Total and accept-biased — every arm the checker cannot
/// pin down (an unknown variable, a call to an unknown function, an index into a non-sequence, a
/// field of an unknown struct, a `?`/closure/`CallExpr`) yields [`Ty::Any`], which unifies with
/// everything. `Call`/`Index`/`FieldAccess` — which the legacy flat inference hard-returned `None`
/// for — are synthesized here: a call yields its function's declared return type, an index yields the
/// element type of what it indexes, a field access yields the field's declared type on the struct.
pub(crate) fn synth(ictx: &mut InferCtx, env: &InferEnv, expr: &Expr) -> Ty {
    match expr {
        Expr::Literal(s) if s == "true" || s == "false" => Ty::Bool,
        // Mirror the runtime literal discrimination: an i64/u64-parseable literal is the
        // width-polymorphic integer default; a float-only literal is a float; a quoted literal is a
        // string. Unary-minus over an integer literal is signed i64 so generic monomorphization does
        // not reinterpret `-1` as the unsigned default and zero-extend it at runtime.
        Expr::Literal(s) if s.parse::<i64>().is_ok() || s.parse::<u64>().is_ok() => Ty::U32,
        Expr::Literal(s) if s.parse::<f64>().is_ok() => Ty::Float("f64".into()),
        Expr::Literal(s) if s.starts_with('"') || s.starts_with('\'') => Ty::Str,
        Expr::Literal(_) => Ty::Any,
        Expr::StrLiteral(_) => Ty::Str,
        Expr::Var(n) => env.vars.get(n).map(|t| Ty::parse(t)).unwrap_or(Ty::Any),
        Expr::Unary { op, .. } if op == "!" => Ty::Bool,
        Expr::Unary { op, .. } if op == "~" => Ty::U32,
        Expr::Unary { op, expr, .. } if op == "-" && integer_literal_expr(expr) => {
            Ty::IntAlias("i64".into())
        }
        Expr::Unary { expr, .. } => synth(ictx, env, expr),
        Expr::Binary { op, lhs, rhs } => synth_binary(ictx, env, op, lhs, rhs),
        Expr::ArrayLiteral { .. } => Ty::List(Box::new(Ty::Any)),
        Expr::MapLiteral { .. } => Ty::Map(Box::new(Ty::Any), Box::new(Ty::Any)),
        Expr::EnumConstruct { enum_name, .. } => Ty::parse(enum_name),
        Expr::Cast { ty, .. } => Ty::parse(ty),
        Expr::Symbolic { ty } => Ty::parse(ty),
        Expr::Tainted { ty, .. } => Ty::Tainted(Box::new(Ty::parse(ty))),
        Expr::TaintSource { .. } => Ty::Tainted(Box::new(Ty::Str)),
        Expr::RawPtr { mutable } => Ty::RawPtr { mutable: *mutable },
        Expr::Declassify { inner, .. } => synth(ictx, env, inner),
        // `if`/`match` used as a value: the arm-join. This computes the branch type by unifying the
        // arms; a *conflict* degrades to `Any` here (accept, no side effect) — the diagnostic is
        // raised by the dedicated arm-join driver, not by synthesis.
        Expr::If { then, else_, .. } => {
            let t = synth(ictx, env, then);
            let e = synth(ictx, env, else_);
            ictx.unify(&t, &e).unwrap_or(Ty::Any)
        }
        Expr::Match { arms, .. } => synth_arms(ictx, env, arms),
        Expr::Block { tail, .. } => tail
            .as_ref()
            .map(|t| synth(ictx, env, t))
            .unwrap_or(Ty::Any),
        // Newly synthesizable — the flat inference returned `None` for all three.
        Expr::Call { callee, .. } => env.fns.get(callee).map(|t| Ty::parse(t)).unwrap_or(Ty::Any),
        Expr::Index { base, .. } => {
            let base_ty = synth(ictx, env, base);
            match ictx.resolve(&base_ty) {
                Ty::List(inner) => *inner,
                Ty::Map(_, val) => *val,
                Ty::Str => Ty::Str, // indexing a string yields a (one-char) string at runtime
                _ => Ty::Any,       // unknown or non-sequence base → accept
            }
        }
        Expr::FieldAccess { base, field, .. } => {
            let base_ty = synth(ictx, env, base);
            match ictx.resolve(&base_ty) {
                Ty::Named(n) | Ty::Struct(n) => env
                    .structs
                    .get(&n)
                    .and_then(|fields| fields.get(field))
                    .map(|t| Ty::parse(t))
                    .unwrap_or(Ty::Any),
                _ => Ty::Any,
            }
        }
        // Typed-`?`: the `?` operator unwraps the operand's `Result<T, E>` / `Option<T>` to its
        // success payload `T`, which is the value the enclosing expression binds. Synthesize the
        // operand, take its annotation (a container instantiation resolves to `Ty::Generic("Result<…>")`
        // whose annotation still begins with the container name — see `synth_container_kind`), and peel
        // the first top-level type argument via `try_unwrap_ok`. An operand `?` cannot pin to a
        // container (a call to an unknown fn, `Any`, a non-container type) yields `None` → `Ty::Any`,
        // the accept direction. This is what lets `let x: WrongT = f()?` be caught the same way
        // `let x: WrongT = f()` already was.
        Expr::Try(inner) => {
            let inner_ty = synth(ictx, env, inner);
            let ann = ictx.resolve(&inner_ty).to_annotation();
            match try_unwrap_ok(&ann) {
                Some(ok) => Ty::parse(&ok),
                None => Ty::Any,
            }
        }
        // Everything else — `CallExpr` (first-class closure call), `Lambda`, `StructLiteral`,
        // `UnifiedBuffer`, `Assume`/`Assert`, `IfLet`, … — is left dynamic. Accept.
        _ => Ty::Any,
    }
}

fn integer_literal_expr(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(s) => {
            let s = s.trim();
            s.parse::<i64>().is_ok()
                || s.strip_prefix("0x")
                    .or_else(|| s.strip_prefix("0X"))
                    .map(|h| i64::from_str_radix(h, 16).is_ok())
                    .unwrap_or(false)
        }
        _ => false,
    }
}

/// Synthesize the type of a binary operator application, faithful to the runtime overloads:
/// comparisons/logicals are `bool`; `+` is string-concat if either side is a string, list-concat if
/// either is a list, else numeric; bitwise/shift are always integer; other arithmetic propagates an
/// operand type (float iff an operand is float).
fn synth_binary(ictx: &mut InferCtx, env: &InferEnv, op: &str, lhs: &Expr, rhs: &Expr) -> Ty {
    match op {
        "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||" => Ty::Bool,
        "+" => {
            let l = synth(ictx, env, lhs);
            let r = synth(ictx, env, rhs);
            let ln = normalize(&ictx.resolve(&l).to_annotation());
            let rn = normalize(&ictx.resolve(&r).to_annotation());
            if ln == "string" || rn == "string" {
                Ty::Str
            } else if ln == "list" || rn == "list" {
                Ty::List(Box::new(Ty::Any))
            } else if !matches!(l, Ty::Any) {
                l
            } else {
                r
            }
        }
        "&" | "|" | "^" | "<<" | ">>" => Ty::U32,
        _ => {
            let l = synth(ictx, env, lhs);
            if matches!(l, Ty::Any) {
                synth(ictx, env, rhs)
            } else {
                l
            }
        }
    }
}

/// Join a `match`'s arm types via genuine union-find: seed a fresh var, unify each arm's type into
/// it, and return the resolved join. A conflict degrades to `Any` (accept) — like `synth`'s `if`
/// arm, emission is the driver's job, not synthesis's.
fn synth_arms(ictx: &mut InferCtx, env: &InferEnv, arms: &[MatchArm]) -> Ty {
    let acc = ictx.fresh();
    for arm in arms {
        let t = synth(ictx, env, &arm.body);
        match ictx.unify(&acc, &t) {
            Ok(_) => {}
            Err(_) => return Ty::Any,
        }
    }
    ictx.resolve(&acc)
}

/// The arm-join CONFLICT check driving the `ANUBIS_ARM_TYPE_CONFLICT` diagnostic: unify all branch
/// types of an `if`/`match` through a fresh accumulator var and return the first genuine
/// cross-category clash as `(left, right)` annotations, or `None` if the arms join cleanly (which
/// includes any arm the checker cannot see — those resolve to `Any` and absorb). This is the one
/// place the core exposes NEW rejection power over the corpus.
pub(crate) fn arm_join_conflict(env: &InferEnv, branches: &[&Expr]) -> Option<(String, String)> {
    let mut ictx = InferCtx::new();
    let acc = ictx.fresh();
    for br in branches {
        let t = synth(&mut ictx, env, br);
        match ictx.unify(&acc, &t) {
            Ok(_) => {}
            Err(conflict) => return Some(conflict),
        }
    }
    None
}

/// CHECK direction: does `expr` have a type assignable to `expected`? Returns the synthesized type as
/// an annotation when it is CONCRETE and NOT assignable to `expected` (the mismatch to report), and
/// `None` whenever the synthesized type resolves toward accept (`Any`/unbound-var/generic) or is
/// assignable. `expected` empty (no annotation) is dynamic ⇒ always accept. This is what lets a
/// `Call`/`Index`/`FieldAccess` argument or return — invisible to the flat inference — be checked.
pub(crate) fn check_mismatch(env: &InferEnv, expr: &Expr, expected: &str) -> Option<String> {
    if expected.trim().is_empty() {
        return None;
    }
    let got = synth_concrete(env, expr)?;
    if assignable(expected, &got) {
        None
    } else {
        Some(got)
    }
}

/// Run `synth` for `expr` and return its type as an annotation ONLY when it resolves to something
/// concrete; `Any`, an unbound unification variable, and a generic parameter all return `None` (the
/// accept direction). The single boundary where "cannot determine ⇒ accept" is enforced for callers.
pub(crate) fn synth_concrete(env: &InferEnv, expr: &Expr) -> Option<String> {
    let mut ictx = InferCtx::new();
    let synthesized = synth(&mut ictx, env, expr);
    let t = ictx.resolve(&synthesized);
    match t {
        Ty::Any | Ty::Var(_) | Ty::Generic(_) => None,
        other => {
            let a = other.to_annotation();
            if a.trim().is_empty() {
                None
            } else {
                Some(a)
            }
        }
    }
}

/// The outer `Result`/`Option` constructor of `expr`'s synthesized type, for typed-`?` checking:
/// `Some("Result")` / `Some("Option")` when the operand's synthesized type is (or names) one of those
/// two containers, else `None`. `None` is the accept direction — an operand the checker cannot pin to
/// a container (a call to an unknown fn, a variable of unknown type, `Any`) never drives a mismatch.
/// Note the annotation for a container instantiation like `Result<u32, string>` parses to
/// `Ty::Generic("Result<u32, string>")`, whose annotation still begins with `Result`, so the prefix
/// test sees through the generic wrapper the structured `Ty` does not yet retain.
pub(crate) fn synth_container_kind(env: &InferEnv, expr: &Expr) -> Option<String> {
    let mut ictx = InferCtx::new();
    let synthesized = synth(&mut ictx, env, expr);
    let ann = ictx.resolve(&synthesized).to_annotation();
    let a = ann.trim();
    if a.starts_with("Result") {
        Some("Result".into())
    } else if a.starts_with("Option") {
        Some("Option".into())
    } else {
        None
    }
}

/// Monomorphization by type-argument substitution, in the checker only: at a call to a generic
/// function, each declared type parameter becomes one fresh unification variable, and every argument
/// whose declared parameter type IS that bare type parameter is unified into it. A parameter used in
/// two positions with two incompatible CONCRETE arguments (`fn same<T>(a: T, b: T)` called as
/// `same(1, "x")`) makes the variable bind `u32` then clash with `string` — returned as `(param,
/// first, second)` for `ANUBIS_GENERIC_CONFLICT`. Accept-biased throughout: an argument the core
/// cannot type synthesizes to `Any` and absorbs (never a spurious conflict), and a parameter whose
/// declared type is concrete or a container is left to the ordinary argument checks — only bare
/// type-parameter positions participate here.
pub(crate) fn generic_call_conflict(
    env: &InferEnv,
    generics: &[String],
    params: &[String],
    args: &[Expr],
) -> Option<(String, String, String)> {
    let mut ictx = InferCtx::new();
    let mut var_of: BTreeMap<&str, Ty> = BTreeMap::new();
    for g in generics {
        let v = ictx.fresh();
        var_of.insert(g.as_str(), v);
    }
    for (param_ty, arg) in params.iter().zip(args.iter()) {
        if let Some(var) = var_of.get(param_ty.trim()).cloned() {
            let t = synth(&mut ictx, env, arg);
            if let Err((first, second)) = ictx.unify(&var, &t) {
                return Some((param_ty.trim().to_string(), first, second));
            }
        }
    }
    None
}

/// The concrete type each generic parameter is bound to at a call, for trait-bound checking
/// (`ANUBIS_TRAIT_BOUND_UNSATISFIED`). Mirrors `generic_call_conflict`'s monomorphization: a generic
/// whose bare type parameter is a declared argument position takes that argument's SYNTHESIZED concrete
/// type via `synth_concrete` — which is `None` for `Any`/`Var`/`Generic`, so an argument the checker
/// cannot pin contributes NO binding and the bound check therefore accepts it (never a false reject).
/// The FIRST concrete binding wins (a later incompatible one is an `ANUBIS_GENERIC_CONFLICT`, reported
/// separately). A generic used only in return/nested (`list<T>`) position gets no binding here. Returns
/// generic-name → concrete type annotation.
pub(crate) fn generic_call_bindings(
    env: &InferEnv,
    generics: &[String],
    params: &[String],
    args: &[Expr],
) -> BTreeMap<String, String> {
    let gset: std::collections::BTreeSet<&str> = generics.iter().map(|s| s.as_str()).collect();
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    for (param_ty, arg) in params.iter().zip(args.iter()) {
        let pt = param_ty.trim();
        if gset.contains(pt) {
            // Resolve the argument's concrete type. `synth` leaves a struct/enum LITERAL dynamic
            // (`Ty::Any`, ty.rs:604-606), so read the nominal name directly from those two forms before
            // falling back to the inferencer (which pins a typed variable or a declared-return call). An
            // argument the checker still cannot pin contributes NO binding → the bound is accepted.
            let concrete = match arg {
                Expr::StructLiteral { name, .. } => Some(name.clone()),
                Expr::EnumConstruct { enum_name, .. } => Some(enum_name.clone()),
                _ => synth_concrete(env, arg),
            };
            if let Some(concrete) = concrete {
                out.entry(pt.to_string()).or_insert(concrete);
            }
        }
    }
    out
}

/// Arity check for a generic-type instantiation in a type annotation: if `annotation` names a user
/// generic type `Base<…>` (present in `type_generics` with a declared parameter count) whose supplied
/// type-argument count differs, return `(base, declared, given)` for `ANUBIS_GENERIC_ARITY`. Returns
/// `None` for everything else — a bare type parameter (`T`, no `<`), a built-in container
/// (`Result`/`Option`/`list`/`Map`/`tainted`, absent from `type_generics`), or a matching arity —
/// the accept direction. Counts only the OUTERMOST instantiation's top-level type arguments, so
/// `Pair<Box<u32>, string>` counts 2.
pub(crate) fn generic_arity_mismatch(
    annotation: &str,
    type_generics: &BTreeMap<String, usize>,
) -> Option<(String, usize, usize)> {
    let a = annotation.trim();
    let lt = a.find('<')?;
    let base = a[..lt].trim();
    let declared = *type_generics.get(base)?;
    let inner = &a[lt + 1..];
    let close = inner.rfind('>')?;
    let given = count_top_level_type_args(&inner[..close]);
    if given == declared {
        None
    } else {
        Some((base.to_string(), declared, given))
    }
}

/// Count the top-level (depth-0) comma-separated type arguments in the inside of a `<…>`. Empty ⇒ 0.
fn count_top_level_type_args(inner: &str) -> usize {
    let inner = inner.trim();
    if inner.is_empty() {
        return 0;
    }
    let mut depth = 0i32;
    let mut count = 1usize;
    for c in inner.chars() {
        match c {
            '<' => depth += 1,
            '>' => depth -= 1,
            ',' if depth == 0 => count += 1,
            _ => {}
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_unwrap_ok_extracts_the_ok_type() {
        assert_eq!(try_unwrap_ok("Option<u64>").as_deref(), Some("u64"));
        assert_eq!(try_unwrap_ok("Result<u64, string>").as_deref(), Some("u64"));
        // Nested generics in the Ok slot are preserved; the E slot is ignored.
        assert_eq!(
            try_unwrap_ok("Result<list<u32>, MyError>").as_deref(),
            Some("list<u32>")
        );
        // Non-Option/Result annotations are not unwrapped (left dynamic).
        assert_eq!(try_unwrap_ok("u64"), None);
        assert_eq!(try_unwrap_ok("string"), None);
        assert_eq!(try_unwrap_ok("Optional<u64>"), None);
    }

    #[test]
    fn parse_to_annotation_round_trips_the_core_vocabulary() {
        // parse -> to_annotation is stable for canonical spellings.
        for canonical in [
            "u8", "u16", "u32", "u64", "i64", "f64", "float", "bool", "string", "list", "map",
        ] {
            let re = Ty::parse(canonical).to_annotation();
            let re2 = Ty::parse(&re).to_annotation();
            assert_eq!(re, re2, "annotation not idempotent for `{canonical}`");
        }
        // Structural forms.
        assert_eq!(Ty::parse("tainted<u32>"), Ty::Tainted(Box::new(Ty::U32)));
        assert_eq!(Ty::parse("tainted<u32>").to_annotation(), "tainted<u32>");
        assert_eq!(Ty::parse("T"), Ty::Generic("T".into()));
        assert_eq!(Ty::parse("Opt<T>"), Ty::Generic("Opt<T>".into()));
        assert_eq!(Ty::parse("Color"), Ty::Named("Color".into()));
        assert_eq!(Ty::parse("Color").to_annotation(), "Color");
        assert_eq!(Ty::parse(""), Ty::Any);
        assert_eq!(Ty::parse("*mut unknown"), Ty::RawPtr { mutable: true });
    }

    #[test]
    fn numeric_and_integer_classification_matches_intent() {
        assert!(is_numeric("f64") && is_numeric("u8") && is_numeric("i64"));
        assert!(!is_numeric("string") && !is_numeric("Color"));
        assert!(is_integer("u32") && is_integer("tainted<u64>"));
        assert!(!is_integer("f64") && !is_integer("string"));
    }

    #[test]
    fn is_float_partitions_the_numerics_against_is_integer() {
        // `is_float`, like `is_integer`, unwraps `tainted<T>` — so the two partition every numeric
        // type (bare or taint-wrapped) into exactly one class. (Note `is_numeric` does NOT unwrap
        // tainted — that historical behavior is pinned by the `ty_parity` oracle — which is exactly
        // why `assignable("u32","tainted<f64>")` still catches the narrowing.)
        for t in ["f32", "f64", "float", "tainted<f64>"] {
            assert!(is_float(t), "{t} must be float");
            assert!(!is_integer(t), "{t} must not be integer");
        }
        for t in ["f32", "f64", "float"] {
            assert!(is_numeric(t), "bare {t} must be numeric");
        }
        for t in ["u8", "u16", "u32", "u64", "i64", "int", "tainted<u32>"] {
            assert!(!is_float(t), "{t} must not be float");
            assert!(is_integer(t), "{t} must be integer");
        }
        // Non-numerics are neither.
        for t in ["string", "bool", "Color", ""] {
            assert!(
                !is_float(t) && !is_integer(t),
                "{t} is neither int nor float"
            );
        }
    }

    #[test]
    fn is_tainted_recognizes_the_qualifier_and_rejects_substring_lookalikes() {
        // The bugfix: a type merely NAMED with the substring "tainted" is not the qualifier. The
        // predicate it replaces (`ty.to_ascii_lowercase().contains("tainted")` in
        // `middle/mod.rs::is_tainted_type`) would wrongly flag both of these as tainted today.
        for not_a_qualifier in [
            "TaintedRecord",
            "UntaintedBuffer",
            "tainted_flag",
            "u32",
            "",
            "Color",
        ] {
            assert!(
                !is_tainted(not_a_qualifier),
                "{not_a_qualifier} is not the tainted<T> qualifier"
            );
        }
        // Real qualifiers, case-insensitive, over the vocabulary the checker actually seeds.
        for qualifier in [
            "tainted<u32>",
            "Tainted<U32>",
            "tainted<string>",
            "tainted<*mut u8>",
            "  tainted<u32>  ",
        ] {
            assert!(is_tainted(qualifier), "{qualifier} must be recognized");
        }
    }

    #[test]
    fn is_secret_recognizes_the_qualifier_and_rejects_substring_lookalikes() {
        // The confidentiality dual of is_tainted: anchored on `secret<`, so an identifier or struct
        // merely NAMED with the substring "secret" is not the qualifier.
        for not_a_qualifier in [
            "secret_key",
            "SecretManager",
            "TopSecretFlag",
            "u64",
            "",
            "secret_source",
        ] {
            assert!(
                !is_secret(not_a_qualifier),
                "{not_a_qualifier} is not the secret<T> qualifier"
            );
        }
        // Real qualifiers, case-insensitive, and nested inside an outer container.
        for qualifier in [
            "secret<u64>",
            "Secret<U64>",
            "secret<string>",
            "list<secret<u32>>",
            "  secret<u64>  ",
        ] {
            assert!(is_secret(qualifier), "{qualifier} must be recognized");
        }
    }

    #[test]
    fn is_tainted_sees_through_an_outer_container() {
        // Regression for the adversarially-found false negative: an EARLIER version of this
        // predicate delegated to `tainted_inner`'s anchored "whole-string" guard, which missed a
        // taint qualifier nested inside a container/generic — a real, parser-producible security
        // regression (a `list<tainted<u32>>` parameter was silently NOT seeded as tainted, letting an
        // unsafe flow reach a sink undetected). The current substring-anchored-on-bracket
        // implementation must catch all of these.
        for nested in [
            "list<tainted<u32>>",
            "Option<tainted<u32>>",
            "Map<string, tainted<u32>>",
        ] {
            assert!(
                is_tainted(nested),
                "{nested} must be detected (nested qualifier)"
            );
        }
    }

    #[test]
    fn is_tainted_leans_toward_over_approximation_not_under() {
        // The accepted, deliberate residual (documented on `is_tainted`'s doc comment): a
        // hypothetical generic type whose OWN name ends in "...tainted" immediately before its own
        // generic bracket is over-flagged. This is the SAFE direction for a security check (forces an
        // unnecessary declassify rather than silently missing a real leak) and is pinned here so a
        // future attempt to "tighten" this predicate doesn't accidentally reintroduce the dangerous
        // false-negative direction instead.
        assert!(
            is_tainted("SomeTainted<u32>"),
            "over-approximation is the accepted, safe direction"
        );
    }

    #[test]
    fn assignable_rejects_float_to_int_narrowing_only() {
        // The one new rejection: a float value into an integer annotation (lossy type lie).
        assert!(!assignable("u32", "f64"), "float must not narrow into u32");
        assert!(!assignable("u8", "float"), "float must not narrow into u8");
        assert!(
            !assignable("tainted<u32>", "f64"),
            "tainted wrapper must not hide the narrowing"
        );
        // Integer→float widening stays allowed (3 is representable as f64).
        assert!(assignable("f64", "u32"), "int widens into float");
        assert!(assignable("float", "u8"), "int widens into float");
        // Integer width-interop and same-type are unaffected.
        assert!(assignable("u32", "u8") && assignable("u64", "i64") && assignable("u32", "u32"));
        // Float→float and non-numeric cases behave exactly as `compatible`.
        assert!(assignable("f64", "f32"));
        assert!(assignable("string", "string") && !assignable("u32", "string"));
        // Unknown/absent value type is dynamically compatible (never reached with a float in
        // practice; here it documents that Any is not narrowed).
        assert!(assignable("u32", "") && assignable("", "f64"));
        // `assignable` only ever refuses MORE than `compatible`, never less.
        for e in ["u8", "u32", "u64", "f64", "string", "bool", "tainted<u32>"] {
            for a in ["u8", "u32", "f64", "float", "string", "tainted<f64>"] {
                if assignable(e, a) {
                    assert!(compatible(e, a), "assignable({e},{a}) implies compatible");
                }
            }
        }
    }

    // --- Bidirectional inference core tests. Exercise the union-find, `synth` (including the newly
    // synthesizable `Call`/`Index`/`FieldAccess`), arm-join `unify`, and the accept-biased boundary.

    fn empty_env<'a>(
        vars: &'a BTreeMap<String, String>,
        fns: &'a BTreeMap<String, String>,
        structs: &'a BTreeMap<String, BTreeMap<String, String>>,
    ) -> InferEnv<'a> {
        InferEnv { vars, fns, structs }
    }

    #[test]
    fn unify_is_accept_biased_and_clashes_only_cross_category() {
        let mut c = InferCtx::new();
        // Numeric widths interoperate (no conflict) — the historical `compatible` relation.
        assert!(c.unify(&Ty::U8, &Ty::U32).is_ok());
        assert!(c.unify(&Ty::U32, &Ty::Float("f64".into())).is_ok());
        // `Any` and a generic parameter absorb — the accept direction.
        assert_eq!(c.unify(&Ty::Any, &Ty::Str).unwrap(), Ty::Str);
        assert_eq!(c.unify(&Ty::Str, &Ty::Any).unwrap(), Ty::Str);
        assert!(c.unify(&Ty::Generic("T".into()), &Ty::Str).is_ok());
        // A genuine cross-category clash is the ONE rejection the core exposes.
        assert_eq!(
            c.unify(&Ty::Str, &Ty::U32).unwrap_err(),
            ("string".into(), "u32".into())
        );
        assert!(c.unify(&Ty::Bool, &Ty::Str).is_err());
    }

    #[test]
    fn unify_binds_variables_then_a_second_incompatible_arm_conflicts() {
        let mut c = InferCtx::new();
        let v = c.fresh();
        // A fresh var unifies with anything and binds.
        assert_eq!(c.unify(&v, &Ty::Str).unwrap(), Ty::Str);
        assert_eq!(c.resolve(&v), Ty::Str);
        // Once bound to `string`, a later `u32` clashes — the arm-join mechanism.
        assert!(c.unify(&v, &Ty::U32).is_err());
        // An unbound var stays a var and resolves toward accept at the boundary (`synth_concrete`).
        let w = c.fresh();
        assert!(matches!(c.resolve(&w), Ty::Var(_)));
    }

    #[test]
    fn synth_types_literals_calls_indices_and_fields() {
        let mut vars = BTreeMap::new();
        vars.insert("xs".to_string(), "list".to_string());
        vars.insert("name".to_string(), "string".to_string());
        vars.insert("p".to_string(), "Point".to_string());
        let mut fns = BTreeMap::new();
        fns.insert("area".to_string(), "u32".to_string());
        fns.insert("label".to_string(), "string".to_string());
        fns.insert("mystery".to_string(), String::new()); // no declared return → Any
        let mut structs = BTreeMap::new();
        let mut point_fields = BTreeMap::new();
        point_fields.insert("x".to_string(), "u32".to_string());
        structs.insert("Point".to_string(), point_fields);
        let env = empty_env(&vars, &fns, &structs);
        let mut c = InferCtx::new();

        // Literals.
        assert_eq!(synth(&mut c, &env, &Expr::Literal("1".into())), Ty::U32);
        assert_eq!(
            synth(
                &mut c,
                &env,
                &Expr::Unary {
                    op: "-".into(),
                    expr: Box::new(Expr::Literal("1".into())),
                }
            ),
            Ty::IntAlias("i64".into())
        );
        assert_eq!(synth(&mut c, &env, &Expr::StrLiteral("a".into())), Ty::Str);
        // Call → declared return type; unknown/blank-return → Any (accept).
        assert_eq!(
            synth(
                &mut c,
                &env,
                &Expr::Call {
                    callee: "area".into(),
                    args: vec![]
                }
            ),
            Ty::U32
        );
        assert_eq!(
            synth(
                &mut c,
                &env,
                &Expr::Call {
                    callee: "unknown_fn".into(),
                    args: vec![]
                }
            ),
            Ty::Any
        );
        assert_eq!(
            synth(
                &mut c,
                &env,
                &Expr::Call {
                    callee: "mystery".into(),
                    args: vec![]
                }
            ),
            Ty::Any
        );
        // Index into a bare `list` → element `Any` (annotations lose element types) ⇒ accept.
        assert_eq!(
            synth(
                &mut c,
                &env,
                &Expr::Index {
                    base: Box::new(Expr::Var("xs".into())),
                    index: Box::new(Expr::Literal("0".into())),
                }
            ),
            Ty::Any
        );
        // Field access → the field's declared type on the struct; unknown field ⇒ accept.
        assert_eq!(
            synth(
                &mut c,
                &env,
                &Expr::FieldAccess {
                    base: Box::new(Expr::Var("p".into())),
                    field: "x".into(),
                    span: crate::frontend::Span { start: 0, end: 0 },
                }
            ),
            Ty::U32
        );
        assert_eq!(
            synth(
                &mut c,
                &env,
                &Expr::FieldAccess {
                    base: Box::new(Expr::Var("p".into())),
                    field: "missing".into(),
                    span: crate::frontend::Span { start: 0, end: 0 },
                }
            ),
            Ty::Any
        );
    }

    #[test]
    fn list_field_type_is_not_synthesized_as_a_modelable_integer() {
        // Soundness contract downstream of the `collect_type_until` fix: once the parser preserves the
        // brackets, a `[int]` field type reaches the inference core as the string `"[int]"`. It MUST
        // NOT resolve to a modelable integer (`U32`/`IntAlias`) — that was the latent false-accept
        // vector (a list field `s.log` typed as `u32`, numeric-compatible with a scalar slot). Parsing
        // yields the opaque `Ty::Named("[int]")`, so a `[int]` value can never pass `check_mismatch`
        // into a `u32` annotation. (Paired with the parser test that proves the string is `"[int]"`.)
        let base = "[int]";
        assert_eq!(Ty::parse(base), Ty::Named("[int]".into()));
        assert!(
            !is_integer(base),
            "a list annotation must not read as an integer"
        );
        assert!(
            !is_numeric(base),
            "a list annotation must not read as numeric"
        );

        let mut vars = BTreeMap::new();
        vars.insert("s".to_string(), "Seat".to_string());
        let fns = BTreeMap::new();
        let mut structs = BTreeMap::new();
        let mut seat_fields = BTreeMap::new();
        seat_fields.insert("log".to_string(), "[int]".to_string());
        structs.insert("Seat".to_string(), seat_fields);
        let env = empty_env(&vars, &fns, &structs);

        let s_log = Expr::FieldAccess {
            base: Box::new(Expr::Var("s".into())),
            field: "log".into(),
            span: crate::frontend::Span { start: 0, end: 0 },
        };
        // Synth resolves the field to its (non-numeric) declared type, not to `U32`.
        let mut c = InferCtx::new();
        assert_eq!(synth(&mut c, &env, &s_log), Ty::Named("[int]".into()));
        // Therefore `Seat { …, log: s.log }`-style flow into a `u32` slot is a genuine mismatch: the
        // check DECIDES (returns the got type) rather than silently accepting a list as an integer.
        assert_eq!(check_mismatch(&env, &s_log, "u32"), Some("[int]".into()));
        // And the SOUND direction is preserved: the same list value flows cleanly into a `[int]` slot.
        assert_eq!(check_mismatch(&env, &s_log, "[int]"), None);
    }

    #[test]
    fn arm_join_flags_string_vs_int_but_not_numeric_widths_or_unknowns() {
        let vars = BTreeMap::new();
        let fns = BTreeMap::new();
        let structs = BTreeMap::new();
        let env = empty_env(&vars, &fns, &structs);
        // `if true { "a" } else { 1 }` — the headline conflict, silently accepted today.
        let s = Expr::StrLiteral("a".into());
        let one = Expr::Literal("1".into());
        assert_eq!(
            arm_join_conflict(&env, &[&s, &one]),
            Some(("string".into(), "u32".into()))
        );
        // Numeric widths and int/float join cleanly — no conflict.
        let two = Expr::Literal("2".into());
        let pi = Expr::Literal("3.14".into());
        assert_eq!(arm_join_conflict(&env, &[&one, &two]), None);
        assert_eq!(arm_join_conflict(&env, &[&one, &pi]), None);
        // An unseeable arm (call to an unknown fn) absorbs — never a spurious conflict.
        let unknown = Expr::Call {
            callee: "who".into(),
            args: vec![],
        };
        assert_eq!(arm_join_conflict(&env, &[&s, &unknown]), None);
    }

    #[test]
    fn check_mismatch_sees_call_returns_and_stays_accept_biased() {
        let vars = BTreeMap::new();
        let mut fns = BTreeMap::new();
        fns.insert("s".to_string(), "string".to_string());
        fns.insert("n".to_string(), "u32".to_string());
        fns.insert("blank".to_string(), String::new());
        let structs = BTreeMap::new();
        let env = empty_env(&vars, &fns, &structs);
        // A `string`-returning call flowing where `u32` is expected — a mismatch the flat inference
        // (which returned `None` for every `Call`) could never see.
        assert_eq!(
            check_mismatch(
                &env,
                &Expr::Call {
                    callee: "s".into(),
                    args: vec![]
                },
                "u32"
            ),
            Some("string".into())
        );
        // Same type ⇒ no mismatch; blank return / unknown expected ⇒ accept (None).
        assert_eq!(
            check_mismatch(
                &env,
                &Expr::Call {
                    callee: "n".into(),
                    args: vec![]
                },
                "u32"
            ),
            None
        );
        assert_eq!(
            check_mismatch(
                &env,
                &Expr::Call {
                    callee: "blank".into(),
                    args: vec![]
                },
                "u32"
            ),
            None
        );
        assert_eq!(
            check_mismatch(
                &env,
                &Expr::Call {
                    callee: "s".into(),
                    args: vec![]
                },
                ""
            ),
            None
        );
    }

    #[test]
    fn synth_container_kind_classifies_result_option_and_accepts_the_rest() {
        let vars = BTreeMap::new();
        let mut fns = BTreeMap::new();
        fns.insert("ropt".to_string(), "Option<u32>".to_string());
        fns.insert("rres".to_string(), "Result<u32, string>".to_string());
        fns.insert("rint".to_string(), "u32".to_string());
        fns.insert("rblank".to_string(), String::new());
        let structs = BTreeMap::new();
        let env = empty_env(&vars, &fns, &structs);
        // A call whose declared return is `Option<...>`/`Result<...>` classifies by its outer
        // constructor (the annotation begins with the container name even though `Ty` wraps it as a
        // generic) — this is what the typed-`?` check compares against the enclosing return.
        assert_eq!(
            synth_container_kind(
                &env,
                &Expr::Call {
                    callee: "ropt".into(),
                    args: vec![]
                }
            ),
            Some("Option".into())
        );
        assert_eq!(
            synth_container_kind(
                &env,
                &Expr::Call {
                    callee: "rres".into(),
                    args: vec![]
                }
            ),
            Some("Result".into())
        );
        // A non-container return, a blank return, and an unknown call all resolve toward accept (None).
        assert_eq!(
            synth_container_kind(
                &env,
                &Expr::Call {
                    callee: "rint".into(),
                    args: vec![]
                }
            ),
            None
        );
        assert_eq!(
            synth_container_kind(
                &env,
                &Expr::Call {
                    callee: "rblank".into(),
                    args: vec![]
                }
            ),
            None
        );
        assert_eq!(
            synth_container_kind(
                &env,
                &Expr::Call {
                    callee: "who".into(),
                    args: vec![]
                }
            ),
            None
        );
    }

    #[test]
    fn typed_question_mark_unwraps_the_call_return_and_catches_the_mismatch() {
        let vars = BTreeMap::new();
        let mut fns = BTreeMap::new();
        fns.insert("rres".to_string(), "Result<u32, string>".to_string());
        fns.insert("ropt".to_string(), "Option<u32>".to_string());
        fns.insert("rbare".to_string(), "u32".to_string());
        fns.insert("rblank".to_string(), String::new());
        let structs = BTreeMap::new();
        let env = empty_env(&vars, &fns, &structs);
        let try_of = |callee: &str| {
            Expr::Try(Box::new(Expr::Call {
                callee: callee.into(),
                args: vec![],
            }))
        };
        // `let x: string = rres()?` — the `?` unwraps `Result<u32, string>` to its Ok type `u32`,
        // which does NOT flow where `string` is expected. Before the Try arm the operator resolved to
        // `Any` (accept) and this leak of a wrong-typed unwrap was invisible.
        assert_eq!(
            check_mismatch(&env, &try_of("rres"), "string"),
            Some("u32".into())
        );
        assert_eq!(
            check_mismatch(&env, &try_of("ropt"), "string"),
            Some("u32".into())
        );
        // The matching annotation accepts: `let x: u32 = rres()?` is exactly the Ok type.
        assert_eq!(check_mismatch(&env, &try_of("rres"), "u32"), None);
        assert_eq!(check_mismatch(&env, &try_of("ropt"), "u32"), None);
        // Accept-bias preserved on the operands `?` cannot pin: a `?` on a non-container return
        // (`u32`), a blank-return call, and an unknown call all resolve toward accept (None).
        assert_eq!(check_mismatch(&env, &try_of("rbare"), "string"), None);
        assert_eq!(check_mismatch(&env, &try_of("rblank"), "string"), None);
        assert_eq!(check_mismatch(&env, &try_of("who"), "string"), None);
    }

    // --- Frozen reference implementations: verbatim copies of the former `middle/mod.rs` free
    // functions as they stood before the relocation. The parity test below asserts the live
    // `ty::` predicates agree with these over an exhaustive matrix, so any future drift from the
    // historical semantics fails closed. Do NOT "simplify" these — they are a fixed oracle. ---

    fn ref_normalize(ty: &str) -> String {
        let t = ty.trim().to_ascii_lowercase();
        match t.as_str() {
            "int" | "i8" | "i16" | "i32" | "i64" | "i128" | "u128" | "usize" | "isize"
            | "number" => "u32".into(),
            "u8" | "u16" | "u32" | "u64" => t,
            "str" | "string" => "string".into(),
            "bool" | "boolean" => "bool".into(),
            "list" | "array" | "vec" => "list".into(),
            "map" | "dict" | "dictionary" => "map".into(),
            other => other.to_string(),
        }
    }
    fn ref_is_numeric(ty: &str) -> bool {
        matches!(
            ref_normalize(ty).as_str(),
            "u8" | "u16" | "u32" | "u64" | "f32" | "f64" | "float"
        )
    }
    fn ref_is_integer(ty: &str) -> bool {
        let inner = ty.trim();
        let inner = if let Some(rest) = inner.strip_prefix("tainted<") {
            rest.strip_suffix('>').unwrap_or(rest)
        } else {
            inner
        };
        matches!(ref_normalize(inner).as_str(), "u8" | "u16" | "u32" | "u64")
    }
    fn ref_cast_preserves_i64(ty: &str) -> bool {
        matches!(
            ty.trim().to_ascii_lowercase().as_str(),
            "u64" | "i64" | "int" | "integer" | "usize" | "isize" | "u128" | "i128"
        )
    }
    fn ref_is_generic(t: &str) -> bool {
        let t = t.trim();
        if t.contains('<') {
            return true;
        }
        !t.is_empty() && t.len() <= 2 && t.chars().all(|c| c.is_ascii_uppercase())
    }
    fn ref_tainted_inner(ty: &str) -> Option<String> {
        let t = ty.trim();
        let lower = t.to_ascii_lowercase();
        if lower.starts_with("tainted<") && lower.ends_with('>') {
            let start = t.find('<')? + 1;
            let end = t.rfind('>')?;
            return Some(t[start..end].trim().to_string());
        }
        None
    }
    fn ref_compatible(expected: &str, actual: &str) -> bool {
        let e_raw = expected.trim();
        let a_raw = actual.trim();
        if e_raw.is_empty() || a_raw.is_empty() {
            return true;
        }
        if ref_is_generic(e_raw) || ref_is_generic(a_raw) {
            return true;
        }
        let e = ref_normalize(e_raw);
        let a = ref_normalize(a_raw);
        if e == a || e_raw == a_raw {
            return true;
        }
        if e == "any" || a == "any" || e == "unknown" || a == "unknown" {
            return true;
        }
        if (e_raw.contains('*') || e.contains("rawptr"))
            && (a_raw.contains('*') || a.contains("rawptr"))
        {
            return true;
        }
        if let Some(inner) = ref_tainted_inner(e_raw) {
            if ref_compatible(&inner, a_raw) {
                return true;
            }
        }
        if let Some(inner) = ref_tainted_inner(a_raw) {
            if ref_compatible(e_raw, &inner) {
                return true;
            }
        }
        if ref_is_numeric(&e) && ref_is_numeric(&a) {
            return true;
        }
        false
    }
    fn ref_bitwidth(ty: &str) -> u32 {
        if ty.contains("u8") || ty == "u8" {
            8
        } else if ty.contains("u16") || ty == "u16" {
            16
        } else if ty.contains("u64") || ty == "u64" {
            64
        } else {
            32
        }
    }

    /// The type-string vocabulary the checker actually encounters, plus adversarial edge cases.
    const VOCAB: &[&str] = &[
        "",
        "u8",
        "u16",
        "u32",
        "u64",
        "i8",
        "i16",
        "i32",
        "i64",
        "i128",
        "u128",
        "usize",
        "isize",
        "int",
        "integer",
        "number",
        "f32",
        "f64",
        "float",
        "bool",
        "boolean",
        "str",
        "string",
        "list",
        "array",
        "vec",
        "map",
        "dict",
        "dictionary",
        "any",
        "unknown",
        "T",
        "U",
        "AB",
        "Opt<T>",
        "Box<int>",
        "tainted<u32>",
        "tainted<string>",
        "tainted<u8>",
        "tainted<i64>",
        "*mut unknown",
        "*const unknown",
        "Color",
        "Status",
        "rawptr",
        "Foo",
        "x",
        "  u32  ",
        "STRING",
        "Tainted<U32>",
    ];

    #[test]
    fn is_tainted_exhaustive_over_vocab() {
        // Expectation-based (literal expected booleans), NOT a diff against a frozen snapshot — see
        // `is_tainted`'s doc comment for why it is deliberately excluded from
        // `ty_parity_exhaustive_against_frozen_reference` below. Exactly the 5 `tainted<...>`-wrapper
        // entries in VOCAB (case-insensitive) are true; every other entry, including the adversarial
        // `"Opt<T>"`/`"Box<int>"` (contain `<` but don't start with the qualifier) and the
        // substring-lookalike-adjacent `"STRING"`/`"Foo"`/`"x"`, is false.
        const TAINTED_VOCAB_ENTRIES: &[&str] = &[
            "tainted<u32>",
            "tainted<string>",
            "tainted<u8>",
            "tainted<i64>",
            "Tainted<U32>",
        ];
        for &s in VOCAB {
            let expected = TAINTED_VOCAB_ENTRIES.contains(&s);
            assert_eq!(is_tainted(s), expected, "is_tainted({s:?})");
        }
    }

    #[test]
    fn ty_parity_exhaustive_against_frozen_reference() {
        // Single-argument predicates over the whole vocabulary.
        for &s in VOCAB {
            assert_eq!(normalize(s), ref_normalize(s), "normalize({s:?})");
            assert_eq!(is_numeric(s), ref_is_numeric(s), "is_numeric({s:?})");
            assert_eq!(is_integer(s), ref_is_integer(s), "is_integer({s:?})");
            assert_eq!(
                cast_preserves_i64(s),
                ref_cast_preserves_i64(s),
                "cast_preserves_i64({s:?})"
            );
            assert_eq!(is_generic(s), ref_is_generic(s), "is_generic({s:?})");
            assert_eq!(
                tainted_inner(s),
                ref_tainted_inner(s),
                "tainted_inner({s:?})"
            );
            assert_eq!(bitwidth(s), ref_bitwidth(s), "bitwidth({s:?})");
        }
        // The pairwise `compatible` relation over the full matrix (VOCAB × VOCAB).
        for &e in VOCAB {
            for &a in VOCAB {
                assert_eq!(
                    compatible(e, a),
                    ref_compatible(e, a),
                    "compatible({e:?}, {a:?})"
                );
            }
        }
    }
    /// `container_element_type` backs `place_struct_type`'s `Expr::Index` arm (CLAIMS 21 root
    /// cause 8). Every `None` here is load-bearing: it restores the pre-fix behaviour rather than
    /// guessing an element type, so an unrecognised spelling can never invent a qualifier.
    #[test]
    fn container_element_type_resolves_list_and_map_value() {
        assert_eq!(container_element_type("list<S>").as_deref(), Some("S"));
        assert_eq!(container_element_type("list< S >").as_deref(), Some("S"));
        assert_eq!(
            container_element_type("map<string,S>").as_deref(),
            Some("S")
        );
        assert_eq!(
            container_element_type("map<string, S>").as_deref(),
            Some("S")
        );
    }

    #[test]
    fn container_element_type_tracks_generic_depth() {
        // The nested comma is deliberately in the MAP KEY, before the outer delimiter. A naive
        // first-comma split would return `i64>, S` rather than `S`.
        assert_eq!(
            container_element_type("map<map<string, i64>, S>").as_deref(),
            Some("S")
        );
        // Nested values must be returned intact as the indexed element type.
        assert_eq!(
            container_element_type("map<string, list<S>>").as_deref(),
            Some("list<S>")
        );
        assert_eq!(
            container_element_type("list<map<string,S>>").as_deref(),
            Some("map<string,S>")
        );
    }

    #[test]
    fn container_element_type_rejects_malformed_or_ambiguous_arguments() {
        assert_eq!(container_element_type("map<,S>"), None);
        assert_eq!(container_element_type("map<string,>"), None);
        assert_eq!(container_element_type("map<string,S,T>"), None);
        assert_eq!(container_element_type("map<map<string,i64>,S>>"), None);
        assert_eq!(container_element_type("map<map<string,i64,S>"), None);
        assert_eq!(container_element_type("secret<list<S>>>"), None);
        assert_eq!(container_element_type("secret<list<S>"), None);
        assert_eq!(container_element_type("list<Box<,>>"), None);
        assert_eq!(container_element_type("list<Box<T,>>"), None);
        assert_eq!(container_element_type("map<string,Box<,T>>"), None);
        assert_eq!(container_element_type("map<string,Box<T,,U>>"), None);
        assert_eq!(container_element_type("list<Box<Result<,>>>"), None);
    }

    #[test]
    fn container_element_type_sees_through_a_flow_qualifier() {
        assert_eq!(
            container_element_type("secret<list<S>>").as_deref(),
            Some("S")
        );
        assert_eq!(
            container_element_type("tainted<list<S>>").as_deref(),
            Some("S")
        );
    }

    #[test]
    fn nominal_place_type_head_peels_exact_flow_labels_and_validates_generics() {
        assert_eq!(nominal_place_type_head("S"), Some("S"));
        assert_eq!(nominal_place_type_head("Box<i64>"), Some("Box"));
        assert_eq!(nominal_place_type_head("secret<S>"), Some("S"));
        assert_eq!(
            nominal_place_type_head("tainted<secret<Box<i64>>>",),
            Some("Box")
        );
        for malformed in [
            "secret<>",
            "secret<S",
            "secret<S>>",
            "secret<S,T>",
            "Box<>",
            "Box<i64",
            "Box<i64>>",
            "Box<,>",
            "Box<T,>",
            "Box<,T>",
            "Box<T,,U>",
            "Box<Result<,>>",
        ] {
            assert_eq!(
                nominal_place_type_head(malformed),
                None,
                "malformed nominal type must not produce a registry key: {malformed}"
            );
        }
    }

    #[test]
    fn container_element_type_is_none_for_anything_unrecognised() {
        // A bare `list` (what an UNANNOTATED array literal infers) carries no element parameter —
        // it must stay None, which is why the unannotated form remains an open residual.
        assert_eq!(container_element_type("list"), None);
        assert_eq!(container_element_type("map"), None);
        assert_eq!(container_element_type("S"), None);
        assert_eq!(container_element_type("i64"), None);
        assert_eq!(container_element_type(""), None);
        assert_eq!(container_element_type("list<>"), None);
        assert_eq!(container_element_type("map<string>"), None);
    }
}
