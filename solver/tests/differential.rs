//! Differential test: the native QF_BV solver vs z3, over a large battery of hand-crafted edge cases
//! and randomly-generated formulas. This is the empirical soundness gate for the bit-blaster — for
//! EVERY formula where native returns a definite verdict, it MUST match z3. A single disagreement is a
//! bit-blaster bug (or a solver bug) and fails the test. `None` (native defers) is always allowed.
//!
//! Skipped automatically if z3 is not on PATH (so CI without z3 still builds).

use anubis_solver::bv::{Formula, Pred, Term};
use anubis_solver::native_check_sat_budget;
use std::io::Write;
use std::process::{Command, Stdio};

fn z3_available() -> bool {
    Command::new("z3")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Run z3 on an SMT-LIB2 string; Some(true)=sat, Some(false)=unsat, None=unknown/error.
fn z3(smt: &str) -> Option<bool> {
    let mut child = Command::new("z3")
        .args(["-in", "-smt2"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    child.stdin.as_mut()?.write_all(smt.as_bytes()).ok()?;
    let out = child.wait_with_output().ok()?;
    match String::from_utf8_lossy(&out.stdout).lines().next()?.trim() {
        "sat" => Some(true),
        "unsat" => Some(false),
        _ => None,
    }
}

// ---- SMT-LIB2 serializer (Formula → string), so native + z3 consume the identical text ----

fn ser_term(t: &Term) -> String {
    use Term::*;
    let bin = |op: &str, a: &Term, b: &Term| format!("({} {} {})", op, ser_term(a), ser_term(b));
    match t {
        Var(n, _) => n.clone(),
        Const(v, w) => format!("(_ bv{} {})", v, w),
        Add(a, b) => bin("bvadd", a, b),
        Sub(a, b) => bin("bvsub", a, b),
        Mul(a, b) => bin("bvmul", a, b),
        And(a, b) => bin("bvand", a, b),
        Or(a, b) => bin("bvor", a, b),
        Xor(a, b) => bin("bvxor", a, b),
        Shl(a, b) => bin("bvshl", a, b),
        Lshr(a, b) => bin("bvlshr", a, b),
        Ashr(a, b) => bin("bvashr", a, b),
        Udiv(a, b) => bin("bvudiv", a, b),
        Urem(a, b) => bin("bvurem", a, b),
        Sdiv(a, b) => bin("bvsdiv", a, b),
        Srem(a, b) => bin("bvsrem", a, b),
        Neg(a) => format!("(bvneg {})", ser_term(a)),
        Not(a) => format!("(bvnot {})", ser_term(a)),
        Extract(hi, lo, a) => format!("((_ extract {} {}) {})", hi, lo, ser_term(a)),
        Concat(a, b) => bin("concat", a, b),
        ZeroExtend(n, a) => format!("((_ zero_extend {}) {})", n, ser_term(a)),
        SignExtend(n, a) => format!("((_ sign_extend {}) {})", n, ser_term(a)),
        Ite(p, a, b) => format!("(ite {} {} {})", ser_pred(p), ser_term(a), ser_term(b)),
    }
}

fn ser_pred(p: &Pred) -> String {
    use Pred::*;
    let bin = |op: &str, a: &Term, b: &Term| format!("({} {} {})", op, ser_term(a), ser_term(b));
    match p {
        Const(true) => "true".into(),
        Const(false) => "false".into(),
        BoolVar(n) => n.clone(),
        Eq(a, b) => bin("=", a, b),
        Ult(a, b) => bin("bvult", a, b),
        Ule(a, b) => bin("bvule", a, b),
        Ugt(a, b) => bin("bvugt", a, b),
        Uge(a, b) => bin("bvuge", a, b),
        Slt(a, b) => bin("bvslt", a, b),
        Sle(a, b) => bin("bvsle", a, b),
        Sgt(a, b) => bin("bvsgt", a, b),
        Sge(a, b) => bin("bvsge", a, b),
        Not(q) => format!("(not {})", ser_pred(q)),
        And(qs) => format!("(and {})", qs.iter().map(ser_pred).collect::<Vec<_>>().join(" ")),
        Or(qs) => format!("(or {})", qs.iter().map(ser_pred).collect::<Vec<_>>().join(" ")),
    }
}

fn serialize(f: &Formula) -> String {
    let mut s = String::from("(set-logic QF_BV)\n");
    for (n, w) in &f.bv_vars {
        s.push_str(&format!("(declare-const {} (_ BitVec {}))\n", n, w));
    }
    for n in &f.bool_vars {
        s.push_str(&format!("(declare-const {} Bool)\n", n));
    }
    for a in &f.asserts {
        s.push_str(&format!("(assert {})\n", ser_pred(a)));
    }
    s.push_str("(check-sat)\n");
    s
}

// ---- tiny deterministic PRNG (no external crate) ----

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

// ---- random well-formed formula generator (small widths for fast DPLL) ----

fn gen_term(rng: &mut Rng, w: u32, depth: u32) -> Term {
    if depth == 0 || rng.below(3) == 0 {
        // leaf
        if rng.below(2) == 0 {
            Term::Var(format!("x{}", rng.below(3)), w)
        } else {
            Term::Const((rng.next() as u128) & ((1u128 << w) - 1), w)
        }
    } else {
        let a = Box::new(gen_term(rng, w, depth - 1));
        let b = Box::new(gen_term(rng, w, depth - 1));
        match rng.below(10) {
            0 => Term::Add(a, b),
            1 => Term::Sub(a, b),
            2 => Term::And(a, b),
            3 => Term::Or(a, b),
            4 => Term::Xor(a, b),
            5 => Term::Not(a),
            6 => Term::Neg(a),
            7 => Term::Shl(a, Box::new(Term::Const(rng.below(w as u64) as u128, w))),
            8 => Term::Lshr(a, Box::new(Term::Const(rng.below(w as u64) as u128, w))),
            _ => Term::Ashr(a, Box::new(Term::Const(rng.below(w as u64) as u128, w))),
        }
    }
}

fn gen_pred(rng: &mut Rng, w: u32) -> Pred {
    let a = gen_term(rng, w, 3);
    let b = gen_term(rng, w, 3);
    match rng.below(9) {
        0 => Pred::Eq(a, b),
        1 => Pred::Ult(a, b),
        2 => Pred::Ule(a, b),
        3 => Pred::Ugt(a, b),
        4 => Pred::Uge(a, b),
        5 => Pred::Slt(a, b),
        6 => Pred::Sle(a, b),
        7 => Pred::Sgt(a, b),
        _ => Pred::Sge(a, b),
    }
}

fn gen_formula_w(rng: &mut Rng, widths: &[u32]) -> Formula {
    // The bit-blast logic (full adder, comparators) is per-bit and width-generic, so the same gates
    // are exercised at any width; `widths` lets a caller pick the regime (small for the historical
    // DPLL parity test, wide for the CDCL stress test). Hand-crafted 64-bit edge cases live in
    // `edge_cases`.
    let w = widths[rng.below(widths.len() as u64) as usize];
    let nasserts = 1 + rng.below(2);
    let asserts: Vec<Pred> = (0..nasserts)
        .map(|_| {
            let p = gen_pred(rng, w);
            if rng.below(2) == 0 {
                Pred::Not(Box::new(p))
            } else {
                p
            }
        })
        .collect();
    Formula {
        bv_vars: (0..3).map(|i| (format!("x{}", i), w)).collect(),
        bool_vars: vec![],
        asserts,
    }
}

#[test]
fn native_agrees_with_z3_on_64bit_edge_cases() {
    if !z3_available() {
        return;
    }
    // The widths + shapes the compiler actually emits: 64-bit overflow, signed boundaries, the u32
    // mask, extract 31 0, sign_extend, MIN/-1. Native must match z3 on every one.
    let cases: &[&str] = &[
        // wrapping overflow: exists x. x + 1 == 0  (x = 2^64-1). SAT.
        "(declare-const x (_ BitVec 64))(assert (= (bvadd x (_ bv1 64)) (_ bv0 64)))",
        // no x with x+1 < x unless wrap: x+1 <u x iff x = all-ones. SAT.
        "(declare-const x (_ BitVec 64))(assert (bvult (bvadd x (_ bv1 64)) x))",
        // signed: MIN <s 0. proven ⇒ ¬(MIN <s 0) unsat.
        "(assert (not (bvslt (_ bv9223372036854775808 64) (_ bv0 64))))",
        // the A1 u32 mask lands in [0, 2^32): (x & 0xFFFFFFFF) <u 2^32 for all x ⇒ negation unsat.
        "(declare-const x (_ BitVec 64))(assert (not (bvult (bvand x (_ bv4294967295 64)) (_ bv4294967296 64))))",
        // extract: low 32 bits of (x zero-extended stuff)… low 32 of (bv5) == bv5:32
        "(assert (not (= ((_ extract 31 0) (_ bv5 64)) (_ bv5 32))))",
        // sign_extend: sign_extend 32 of a negative 32-bit stays negative
        "(assert (bvslt ((_ sign_extend 32) (_ bv4294967295 32)) (_ bv0 64)))",
        // 2^53+1 rounding has nothing to do with BV — but 2^53+1 > 2^53 in BV is true.
        "(assert (bvugt (_ bv9007199254740993 64) (_ bv9007199254740992 64)))",
    ];
    for smt in cases {
        let full = format!("(set-logic QF_BV)\n{}\n(check-sat)\n", smt);
        let nat = native_check_sat_budget(&full, 2_000_000);
        let zv = z3(&full);
        if let (Some(n), Some(z)) = (nat, zv) {
            assert_eq!(n, z, "native≠z3 on:\n{}", full);
        } else {
            assert!(nat.is_some() && zv.is_some(), "native deferred a 64-bit edge case:\n{}", full);
        }
    }
}

#[test]
fn native_agrees_with_z3_on_random_battery() {
    if !z3_available() {
        eprintln!("z3 not on PATH — skipping differential test");
        return;
    }
    let mut rng = Rng(0x9E3779B97F4A7C15);
    let (mut decided, mut deferred, mut disagreements) = (0u64, 0u64, 0u64);
    let mut first_bad: Option<String> = None;
    for _ in 0..2000 {
        let f = gen_formula_w(&mut rng, &[4, 6, 8]);
        let smt = serialize(&f);
        let native = native_check_sat_budget(&smt, 60_000);
        match native {
            None => deferred += 1,
            Some(nat) => {
                let z = z3(&smt);
                if let Some(zv) = z {
                    if nat != zv {
                        disagreements += 1;
                        if first_bad.is_none() {
                            first_bad = Some(format!(
                                "native={} z3={} on:\n{}",
                                nat, zv, smt
                            ));
                        }
                    } else {
                        decided += 1;
                    }
                }
            }
        }
    }
    eprintln!(
        "differential: decided-agree={} deferred={} DISAGREEMENTS={}",
        decided, deferred, disagreements
    );
    assert_eq!(
        disagreements, 0,
        "native disagreed with z3.\nFirst: {}",
        first_bad.unwrap_or_default()
    );
    assert!(decided > 100, "native decided too few ({decided}) — sanity");
}

/// CDCL stress: WIDE bit-vectors (16/24/32-bit) at a full conflict budget. The historical DPLL
/// choked on these (each decision re-scanned every clause), so the small-width battery above was all
/// it could sustain. With the CDCL engine (watched literals + clause learning), the native solver
/// must now DECIDE most wide instances — not defer them — and still agree with z3 on every verdict.
/// This is the evidence that the CDCL engine is real: it proves/refutes 32-bit adder+comparator
/// formulas in-budget, which is the regime the real compiler emits (`u32` obligations are 32-bit).
#[test]
fn native_agrees_with_z3_on_wide_battery() {
    if !z3_available() {
        eprintln!("z3 not on PATH — skipping wide differential test");
        return;
    }
    let mut rng = Rng(0xD1B54A32D192ED03);
    let (mut decided, mut deferred, mut disagreements) = (0u64, 0u64, 0u64);
    let mut first_bad: Option<String> = None;
    for _ in 0..600 {
        let f = gen_formula_w(&mut rng, &[16, 24, 32]);
        let smt = serialize(&f);
        let native = native_check_sat_budget(&smt, 2_000_000);
        match native {
            None => deferred += 1,
            Some(nat) => {
                if let Some(zv) = z3(&smt) {
                    if nat != zv {
                        disagreements += 1;
                        if first_bad.is_none() {
                            first_bad = Some(format!("native={} z3={} on:\n{}", nat, zv, smt));
                        }
                    } else {
                        decided += 1;
                    }
                }
            }
        }
    }
    eprintln!(
        "wide differential: decided-agree={} deferred={} DISAGREEMENTS={}",
        decided, deferred, disagreements
    );
    assert_eq!(
        disagreements, 0,
        "native disagreed with z3 on a wide formula.\nFirst: {}",
        first_bad.unwrap_or_default()
    );
    // The whole point of CDCL: the wide regime is now decided, not deferred.
    assert!(
        decided > deferred,
        "CDCL still defers most wide formulas (decided={decided} deferred={deferred}) — \
         the engine is not carrying its weight"
    );
}
