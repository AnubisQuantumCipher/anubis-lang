//! Differential test for the native QF_FP comparison lane (fp→BV lowering) against z3's real
//! floating-point theory. Random IEEE-754 Float64 bit patterns — biased to include NaN, ±0, ±∞, and
//! subnormals, the exact cases the monotonic-key lowering must handle — are wired through random
//! comparison formulas, serialized once as QF_FP, and decided by BOTH native (which lowers to BV) and
//! z3 (native fp). Any disagreement is a lowering bug. `None` (native declines) is always allowed.

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

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
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

/// A random Float64 bit pattern, biased toward the special classes the lowering special-cases.
fn rand_fp_bits(rng: &mut Rng) -> u64 {
    match rng.below(8) {
        0 => 0x7FF8_0000_0000_0000 | (rng.next() & 0x7_FFFF_FFFF_FFFF), // a NaN (exp all 1s, mant != 0)
        1 => 0x7FF0_0000_0000_0000,                                     // +inf
        2 => 0xFFF0_0000_0000_0000,                                     // -inf
        3 => 0x0,                                                       // +0
        4 => 0x8000_0000_0000_0000,                                     // -0
        5 => rng.next() & 0x000F_FFFF_FFFF_FFFF, // subnormal-ish (exp 0, random mantissa)
        6 => {
            // a "nice" small magnitude value from a f64 in [-8, 8)
            let v = (rng.next() as f64 / u64::MAX as f64) * 16.0 - 8.0;
            v.to_bits()
        }
        _ => rng.next(), // fully random 64 bits
    }
}

/// An fp operand: a var, a `(fp …)` literal, or an exact `fp.neg`/`fp.abs` wrapper around one.
fn fp_operand(rng: &mut Rng) -> String {
    let base = if rng.below(2) == 0 {
        format!("x{}", rng.below(3))
    } else {
        let b = rand_fp_bits(rng);
        let sign = (b >> 63) & 1;
        let exp = (b >> 52) & 0x7FF;
        let mant = b & 0xF_FFFF_FFFF_FFFF;
        format!("(fp #b{:01b} #b{:011b} #b{:052b})", sign, exp, mant)
    };
    match rng.below(4) {
        0 => format!("(fp.neg {})", base),
        1 => format!("(fp.abs {})", base),
        _ => base,
    }
}

fn fp_cmp(rng: &mut Rng) -> String {
    let op = ["fp.lt", "fp.leq", "fp.gt", "fp.geq", "fp.eq"][rng.below(5) as usize];
    let a = fp_operand(rng);
    let b = fp_operand(rng);
    let p = format!("({} {} {})", op, a, b);
    if rng.below(2) == 0 {
        format!("(not {})", p)
    } else {
        p
    }
}

#[test]
fn native_fp_lowering_agrees_with_z3() {
    if !z3_available() {
        eprintln!("z3 not on PATH — skipping fp differential");
        return;
    }
    let mut rng = Rng(0x1D8E_4A2B_9F37_C561);
    let (mut decided, mut deferred, mut disagreements) = (0u64, 0u64, 0u64);
    let mut first_bad: Option<String> = None;
    for _ in 0..2000 {
        let nasserts = 1 + rng.below(3);
        let mut body = String::new();
        for _ in 0..nasserts {
            body.push_str(&format!("(assert {})\n", fp_cmp(&mut rng)));
        }
        let mut smt = String::from("(set-logic QF_FP)\n");
        for i in 0..3 {
            smt.push_str(&format!("(declare-const x{} (_ FloatingPoint 11 53))\n", i));
        }
        smt.push_str(&body);
        smt.push_str("(check-sat)\n");
        match native_check_sat_budget(&smt, 2_000_000) {
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
        "fp differential: decided-agree={} deferred={} DISAGREEMENTS={}",
        decided, deferred, disagreements
    );
    assert_eq!(
        disagreements,
        0,
        "native fp lowering disagreed with z3.\nFirst: {}",
        first_bad.unwrap_or_default()
    );
    assert!(
        decided > 500,
        "native decided too few fp formulas ({decided}) — lowering sanity"
    );
}
