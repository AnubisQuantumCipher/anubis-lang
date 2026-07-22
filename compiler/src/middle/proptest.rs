//! Phase-4 B3: deterministic program generator for solver↔runtime differential tests.
//!
//! Properties enforced by the test harness (not here):
//! - **P_discharge:** obligations PASS ⇒ sampled inputs under `requires` never violate the ensures
//! - **P_disproof:** obligations FAIL with a model ⇒ the body does not satisfy the ensures at runtime

/// Tiny LCG (no external RNG) — same constants as the expression-level differential harness.
pub struct Lcg(pub u64);

impl Lcg {
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 11
    }
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() as usize) % n
        }
    }
    pub fn pick_i64(&mut self, pool: &[i64]) -> i64 {
        pool[self.below(pool.len())]
    }
}

/// Boundary-heavy i64 pool (wrap / shift / sign edges).
pub const VALUE_POOL: [i64; 14] = [
    0,
    1,
    2,
    3,
    7,
    -1,
    -2,
    -7,
    63,
    64,
    100,
    i64::MAX,
    i64::MIN + 1,
    -100,
];

fn lit(v: i64) -> String {
    if v == i64::MIN {
        "(0 - 9223372036854775807 - 1)".into()
    } else if v < 0 {
        format!("(0 - {})", v.wrapping_neg())
    } else {
        v.to_string()
    }
}

/// Build a pure-i64 expression string + oracle value (mirrors run.rs wrapping semantics).
pub fn build_expr(rng: &mut Lcg, depth: u32) -> (String, i64) {
    if depth == 0 || rng.below(3) == 0 {
        let v = rng.pick_i64(&VALUE_POOL);
        return (lit(v), v);
    }
    match rng.below(11) {
        0 => bin(rng, depth, "+", |a, b| a.wrapping_add(b)),
        1 => bin(rng, depth, "-", |a, b| a.wrapping_sub(b)),
        2 => bin(rng, depth, "*", |a, b| a.wrapping_mul(b)),
        3 => bin(rng, depth, "&", |a, b| a & b),
        4 => bin(rng, depth, "|", |a, b| a | b),
        5 => bin(rng, depth, "^", |a, b| a ^ b),
        6 => {
            let (ls, lv) = build_expr(rng, depth - 1);
            let (rs, rv) = build_expr(rng, depth - 1);
            let s = (rv.rem_euclid(64)) as u32;
            (format!("({ls} << {rs})"), lv.wrapping_shl(s))
        }
        7 => {
            let (ls, lv) = build_expr(rng, depth - 1);
            let (rs, rv) = build_expr(rng, depth - 1);
            let s = (rv.rem_euclid(64)) as u32;
            (format!("({ls} >> {rs})"), lv.wrapping_shr(s))
        }
        8 => {
            // Division: non-zero POSITIVE literal divisor only (modelable + no trap).
            let (ls, lv) = build_expr(rng, depth - 1);
            let d = [1i64, 2, 3, 4, 5, 7, 8, 10][rng.below(8)];
            (format!("({ls} / {d})"), lv.wrapping_div(d))
        }
        9 => {
            let (ls, lv) = build_expr(rng, depth - 1);
            let d = [1i64, 2, 3, 4, 5, 7, 8, 10][rng.below(8)];
            (format!("({ls} % {d})"), lv.wrapping_rem(d))
        }
        _ => {
            let (is, iv) = build_expr(rng, depth - 1);
            (format!("(- {is})"), iv.wrapping_neg())
        }
    }
}

fn bin(rng: &mut Lcg, depth: u32, op: &str, f: impl Fn(i64, i64) -> i64) -> (String, i64) {
    let (ls, lv) = build_expr(rng, depth - 1);
    let (rs, rv) = build_expr(rng, depth - 1);
    (format!("({ls} {op} {rs})"), f(lv, rv))
}

/// A true contract: body returns `e`, ensures `result == e` (same expression text).
pub fn gen_true_contract_program(seed: u64, depth: u32) -> (String, i64) {
    let mut rng = Lcg(seed);
    let (e, v) = build_expr(&mut rng, depth);
    let src = format!(
        "fn f() -> u32 ensures(result == {e}) {{ return {e}; }}\nfn main() {{ print(f()); }}\n"
    );
    (src, v)
}

/// A false contract: body returns `e`, ensures `result == e + 1` (always wrong unless wrap).
pub fn gen_false_contract_program(seed: u64, depth: u32) -> (String, i64) {
    let mut rng = Lcg(seed ^ 0x9e37_79b9_7f4a_7c15);
    let (e, v) = build_expr(&mut rng, depth);
    // ensures wrong constant oracle+1 (wrapping).
    let wrong = v.wrapping_add(1);
    let wrong_lit = lit(wrong);
    let src = format!(
        "fn f() -> u32 ensures(result == {wrong_lit}) {{ return {e}; }}\nfn main() {{ print(f()); }}\n"
    );
    (src, v)
}

/// Symbolic true increment (always modelable, always true).
pub fn gen_symbolic_true_program() -> String {
    "fn f(x: u32) -> u32 requires(x >= 0) requires(x <= 100) ensures(result == x + 1) { return x + 1; }\n\
     fn main() { print(f(7)); }\n"
        .into()
}

/// Symbolic false postcondition (always modelable, always false).
pub fn gen_symbolic_false_program() -> String {
    "fn f(x: u32) -> u32 requires(x >= 0) requires(x <= 100) ensures(result == x + 2) { return x + 1; }\n\
     fn main() { print(f(7)); }\n"
        .into()
}

// ── Phase-3 QF_FP float differential generator ──────────────────────────────────────────────────
// Same idea as the i64 generator, over IEEE-754 f64: the `ensures(result == <oracle>)` discharges ONLY
// IF z3's QF_FP evaluation of the generated arithmetic equals the Rust f64 oracle — so a wrong op
// mapping (e.g. `+` encoded as `fp.mul`) fails to discharge and the harness catches it, without any
// dependence on runtime print formatting.

/// Small finite float pool (kept away from `inf`/`NaN`; division uses non-zero positive divisors).
pub const FLOAT_POOL: [f64; 8] = [0.0, 1.0, -1.0, 0.5, 2.0, 3.0, -2.5, 4.0];

/// An Anubis float literal for a finite `v` (parenthesized `(0.0 - m)` for negatives — unambiguous in
/// an `ensures`). `{:?}` gives the shortest round-tripping decimal, so the literal re-parses to `v`.
fn flit(v: f64) -> String {
    if v.is_sign_negative() && v != 0.0 {
        format!("(0.0 - {:?})", -v)
    } else {
        format!("{v:?}")
    }
}

/// Build a pure-literal f64 arithmetic string + its exact Rust f64 oracle (mirrors run.rs `+ - * /`).
pub fn build_float_expr(rng: &mut Lcg, depth: u32) -> (String, f64) {
    if depth == 0 || rng.below(3) == 0 {
        let v = FLOAT_POOL[rng.below(FLOAT_POOL.len())];
        return (flit(v), v);
    }
    match rng.below(4) {
        0 => {
            let (l, lv) = build_float_expr(rng, depth - 1);
            let (r, rv) = build_float_expr(rng, depth - 1);
            (format!("({l} + {r})"), lv + rv)
        }
        1 => {
            let (l, lv) = build_float_expr(rng, depth - 1);
            let (r, rv) = build_float_expr(rng, depth - 1);
            (format!("({l} - {r})"), lv - rv)
        }
        2 => {
            let (l, lv) = build_float_expr(rng, depth - 1);
            let (r, rv) = build_float_expr(rng, depth - 1);
            (format!("({l} * {r})"), lv * rv)
        }
        _ => {
            let (l, lv) = build_float_expr(rng, depth - 1);
            let d = [1.0f64, 2.0, 4.0, 0.5, 8.0][rng.below(5)];
            (format!("({l} / {d:?})"), lv / d)
        }
    }
}

/// A TRUE float contract: `ensures(result == <oracle>)` over a random f64 arithmetic body. `None` when
/// the arithmetic overflows to ±inf/NaN or needs scientific notation (the literal encoder rejects those,
/// and the contract can only be built for a finite decimal-representable oracle).
pub fn gen_true_float_contract_program(seed: u64, depth: u32) -> Option<(String, f64)> {
    let mut rng = Lcg(seed);
    let (e, v) = build_float_expr(&mut rng, depth);
    if !v.is_finite() || format!("{v:?}").contains(['e', 'E']) {
        return None;
    }
    let vl = flit(v);
    Some((
        format!("fn f() -> f64 ensures(result == {vl}) {{ return {e}; }}\nfn main() {{ print(f()); }}\n"),
        v,
    ))
}

/// A FALSE float contract: `ensures(result == <oracle + 1.0>)` — wrong unless `v + 1.0 == v` (a huge `v`
/// where the `+1` rounds away, which the small pool avoids; skipped defensively). `None` on non-finite.
pub fn gen_false_float_contract_program(seed: u64, depth: u32) -> Option<(String, f64)> {
    let mut rng = Lcg(seed ^ 0x9e37_79b9_7f4a_7c15);
    let (e, v) = build_float_expr(&mut rng, depth);
    let wrong = v + 1.0;
    if !v.is_finite()
        || !wrong.is_finite()
        || wrong == v
        || format!("{wrong:?}").contains(['e', 'E'])
    {
        return None;
    }
    let wl = flit(wrong);
    Some((
        format!("fn f() -> f64 ensures(result == {wl}) {{ return {e}; }}\nfn main() {{ print(f()); }}\n"),
        v,
    ))
}

// ── Phase-3 QF_S string differential generator ──────────────────────────────────────────────────
// String `==` is EXACT structural equality both at runtime (AnubisValue::Str(Rc<String>) PartialEq) and
// in SMT QF_S `(= a b)`, so the differential property is INJECTIVITY of the encoder: any two
// runtime-DISTINCT strings must stay distinct in z3, and any string must equal itself. The pool is
// deliberately loaded with the encoder's real risk surface — a literal backslash, a doubled `"`, and a
// `\u{..}`-shaped literal that z3's Unicode-strings theory would re-decode if the backslash escape were
// dropped (the exact false-accept the review caught). A FALSE contract over a distinct pair therefore
// FAILS iff the encoder is injective; a collapsing encoder (unescaped `\u`) would wrongly discharge it
// and the harness catches it — no runtime run needed.

/// RUNTIME strings (as Rust `&str`, i.e. AFTER the Anubis lexer decodes escapes). All DISTINCT.
/// `"\\u{41}"` is the 6-char runtime string `\`,`u`,`{`,`4`,`1`,`}` — it collides with `"A"` iff the
/// SMT encoder fails to escape the backslash. Printable only (no raw control chars): the encoder's
/// modelable domain is exact for these; control-char handling is a documented residual.
pub const STRING_POOL: [&str; 15] = [
    "", "a", "b", "A", "B", "ok", "OK", "closed", "\\",      // one backslash
    "\\u{41}", // 6 chars: \ u { 4 1 }  — collision-bait vs "A"
    "\\u{42}", // 6 chars — collision-bait vs "B"
    "\\n",     // 2 chars: \ n  — vs a real newline (excluded) / distinct from "n"
    "\"",      // one double-quote
    "a\"b",    // embedded quote
    "x\\y",    // embedded backslash
];

/// Encode a runtime string into an Anubis SOURCE literal so the lexer reconstructs it EXACTLY: escape
/// `\` → `\\` and `"` → `\"` (the lexer's `lex_escape` inverts both). No `\u`/`\x`/`\n` is emitted, so
/// the runtime StrLiteral equals the input byte-for-byte.
pub fn anubis_str_literal(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// A TRUE string contract: `return "s"; ensures(result == "s")` — the same literal both sides, so the
/// QF_S obligation is `(= enc(s) enc(s))` and MUST discharge (reflexivity), whatever `s` contains.
pub fn gen_true_string_contract_program(seed: u64) -> (String, &'static str) {
    let mut rng = Lcg(seed);
    let s = STRING_POOL[rng.below(STRING_POOL.len())];
    let lit = anubis_str_literal(s);
    let src = format!(
        "fn f() -> string ensures(result == {lit}) {{ return {lit}; }}\nfn main() {{ print(f()); }}\n"
    );
    (src, s)
}

/// A FALSE string contract: `return "a"; ensures(result == "b")` for a DISTINCT pair `a != b`. It MUST
/// be disproved (FAIL). The index arithmetic guarantees `j != i` over the all-distinct pool, so the
/// runtime strings genuinely differ; a non-injective encoder (e.g. an unescaped `\u{..}`) would make z3
/// see them equal and wrongly discharge — which the harness flags.
pub fn gen_false_string_contract_program(seed: u64) -> (String, &'static str, &'static str) {
    let mut rng = Lcg(seed ^ 0x9e37_79b9_7f4a_7c15);
    let n = STRING_POOL.len();
    let i = rng.below(n);
    let j = (i + 1 + rng.below(n - 1)) % n; // always distinct from i for n >= 2
    let a = STRING_POOL[i];
    let b = STRING_POOL[j];
    let ret = anubis_str_literal(a);
    let ens = anubis_str_literal(b);
    let src = format!(
        "fn f() -> string ensures(result == {ens}) {{ return {ret}; }}\nfn main() {{ print(f()); }}\n"
    );
    (src, a, b)
}
