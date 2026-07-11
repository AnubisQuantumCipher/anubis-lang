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

/// An INTEGER type the solver may soundly model as a 64-bit bit-vector (matching the i64 runtime).
/// Floats are deliberately excluded. `tainted<T>` is a qualifier — unwrap it first.
pub(crate) fn is_integer(ty: &str) -> bool {
    let inner = ty.trim();
    let inner = if let Some(rest) = inner.strip_prefix("tainted<") {
        rest.strip_suffix('>').unwrap_or(rest)
    } else {
        inner
    };
    matches!(normalize(inner).as_str(), "u8" | "u16" | "u32" | "u64")
}

/// A FLOAT type (`f32`/`f64`/`float`) — the complement of [`is_integer`] within [`is_numeric`].
/// Unwraps a `tainted<T>` qualifier first, exactly like [`is_integer`], so the two partition the
/// numeric types identically regardless of the taint wrapper.
pub(crate) fn is_float(ty: &str) -> bool {
    let inner = ty.trim();
    let inner = if let Some(rest) = inner.strip_prefix("tainted<") {
        rest.strip_suffix('>').unwrap_or(rest)
    } else {
        inner
    };
    matches!(normalize(inner).as_str(), "f32" | "f64" | "float")
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
