//! Differential test for the native QF_S string-EQUALITY lane against z3's real string theory. Random
//! equality/inequality formulas over a few `String` vars and a pool of literals (including an escaped
//! backslash and a doubled-quote escape, to exercise `unquote_and_decode` + the tokenizer) are
//! serialized once as QF_S and decided by BOTH native (which lowers by interning) and z3. Any
//! disagreement is a lowering bug. `None` (native declines) is always allowed.

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

// A pool of string literals (as they appear in SMT text). `\u{5c}` is a backslash, `""` is an escaped
// quote, `""`(empty) is the empty string — all canonicalized by the native decoder to match z3.
const LITERALS: &[&str] = &[
    "\"a\"",
    "\"b\"",
    "\"open\"",
    "\"\"", // empty string
    "\"hello\"",
    "\"\\u{5c}\"", // a single backslash
    "\"x\"\"y\"",  // the string  x"y  (doubled-quote escape)
];

fn operand(rng: &mut Rng) -> String {
    if rng.below(2) == 0 {
        format!("s{}", rng.below(3))
    } else {
        LITERALS[rng.below(LITERALS.len() as u64) as usize].to_string()
    }
}

fn atom(rng: &mut Rng) -> String {
    let p = format!("(= {} {})", operand(rng), operand(rng));
    if rng.below(2) == 0 {
        format!("(not {})", p)
    } else {
        p
    }
}

#[test]
fn native_string_equality_agrees_with_z3() {
    if !z3_available() {
        eprintln!("z3 not on PATH — skipping string differential");
        return;
    }
    let mut rng = Rng(0x9E37_79B9_7F4A_7C16);
    let (mut decided, mut deferred, mut disagreements) = (0u64, 0u64, 0u64);
    let mut first_bad: Option<String> = None;
    for _ in 0..2000 {
        let nasserts = 1 + rng.below(4);
        let mut body = String::new();
        for _ in 0..nasserts {
            body.push_str(&format!("(assert {})\n", atom(&mut rng)));
        }
        let mut smt = String::from("(set-logic QF_S)\n");
        for i in 0..3 {
            smt.push_str(&format!("(declare-const s{} String)\n", i));
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
        "string differential: decided-agree={} deferred={} DISAGREEMENTS={}",
        decided, deferred, disagreements
    );
    assert_eq!(
        disagreements,
        0,
        "native string lowering disagreed with z3.\nFirst: {}",
        first_bad.unwrap_or_default()
    );
    assert!(
        decided > 500,
        "native decided too few string formulas ({decided}) — lowering sanity"
    );
}
