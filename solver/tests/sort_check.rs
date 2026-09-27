//! P-SORT-1: the native solver must never decide an ILL-SORTED query.
//!
//! The parser lowers Float64 to `BitVec 64` and String to `BitVec 32`, so without a sort check an
//! operator applied to the wrong sort (`bvsgt` over a Float64, `bvadd` over a String, `=` between a
//! Float64 and a bit-vector) bit-blasts as ordinary bit-vector logic and can come back UNSAT with a
//! valid refutation certificate. That certificate refutes the CNF, not the query: z3 rejects the same
//! query with a sort error. Every ill-sorted case below must DECLINE (`None`, defer to z3), and z3 —
//! when present — must confirm it is ill-sorted by answering `(error …)`.
//!
//! The well-sorted controls must still decide, and `=` over Float64 must follow SMT-LIB (all NaNs are
//! one value), not bit equality.

use anubis_solver::native_check_sat_budget;
use std::io::Write;
use std::process::{Command, Stdio};

const BUDGET: u64 = 100_000;

fn z3_first_line(smt: &str) -> Option<String> {
    let mut child = Command::new("z3")
        .args(["-in", "-smt2"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    child.stdin.as_mut()?.write_all(smt.as_bytes()).ok()?;
    let out = child.wait_with_output().ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .map(|l| l.trim().to_string())
}

fn assert_declines_ill_sorted(name: &str, smt: &str) {
    assert_eq!(
        native_check_sat_budget(smt, BUDGET),
        None,
        "{name}: native decided an ill-sorted query"
    );
    if let Some(z) = z3_first_line(smt) {
        assert!(
            z.starts_with("(error"),
            "{name}: z3 did not reject the query as ill-sorted (first line `{z}`) — the test \
             case itself is wrong"
        );
    }
}

fn assert_decides(name: &str, smt: &str, sat: bool) {
    assert_eq!(
        native_check_sat_budget(smt, BUDGET),
        Some(sat),
        "{name}: native did not give the expected verdict"
    );
    if let Some(z) = z3_first_line(smt) {
        assert_eq!(
            z,
            if sat { "sat" } else { "unsat" },
            "{name}: z3 disagrees with the expected verdict — the test case itself is wrong"
        );
    }
}

const FP: &str = "(_ FloatingPoint 11 53)";

/// The exact query retained in the evidence bundle of review reproducer q1 (pin anubis-pathprec,
/// 2026-09-23): filed as `rup_refutation`/PASS by the native solver although z3 rejects it.
#[test]
fn q1_retained_query_declines() {
    let smt = format!(
        "(set-logic QF_FP)\n(declare-const RNE {FP})\n(declare-const anb_x {FP})\n\
         (declare-const to_fp {FP})\n(assert (= anb_x ((_ to_fp 11 53) RNE 2.5)))\n\
         (assert (bvsgt anb_x (_ bv2 64)))\n(assert (not (bvsge anb_x (_ bv3 64))))\n\
         (check-sat)\n(get-model)\n"
    );
    assert_declines_ill_sorted("q1", &smt);
}

#[test]
fn bv_comparison_over_float_declines() {
    let smt = format!(
        "(set-logic QF_FP)\n(declare-const x {FP})\n(assert (bvsgt x (_ bv2 64)))\n\
         (assert (not (bvsge x (_ bv3 64))))\n(check-sat)\n"
    );
    assert_declines_ill_sorted("bvsgt-over-fp", &smt);
}

#[test]
fn bv_arithmetic_over_float_declines() {
    let smt = format!(
        "(set-logic QF_FP)\n(declare-const x {FP})\n(declare-const y (_ BitVec 64))\n\
         (assert (= y (bvadd x (_ bv1 64))))\n(check-sat)\n"
    );
    assert_declines_ill_sorted("bvadd-over-fp", &smt);
}

#[test]
fn bv_arithmetic_over_string_declines() {
    let smt = "(set-logic QF_S)\n(declare-const s String)\n\
               (assert (= (bvadd s (_ bv1 32)) (_ bv0 32)))\n(check-sat)\n";
    assert_declines_ill_sorted("bvadd-over-string", smt);
}

#[test]
fn equality_float_vs_bitvector_declines() {
    let smt = format!(
        "(set-logic QF_FP)\n(declare-const x {FP})\n(declare-const y (_ BitVec 64))\n\
         (assert (not (= x y)))\n(check-sat)\n"
    );
    assert_declines_ill_sorted("eq-fp-bv", &smt);
}

#[test]
fn equality_string_vs_bitvector_declines() {
    let smt = "(set-logic QF_S)\n(declare-const s String)\n(declare-const y (_ BitVec 32))\n\
               (assert (not (= s y)))\n(check-sat)\n";
    assert_declines_ill_sorted("eq-str-bv", smt);
}

#[test]
fn equality_float_vs_bitvector_literal_declines() {
    let smt = format!(
        "(set-logic QF_FP)\n(declare-const x {FP})\n(assert (= x (_ bv0 64)))\n(check-sat)\n"
    );
    assert_declines_ill_sorted("eq-fp-bvlit", &smt);
}

#[test]
fn fp_comparison_over_bitvector_declines() {
    let smt = "(set-logic QF_FP)\n(declare-const x (_ BitVec 64))\n\
               (assert (fp.lt x ((_ to_fp 11 53) RNE 1.0)))\n(check-sat)\n";
    assert_declines_ill_sorted("fp.lt-over-bv", smt);
}

#[test]
fn fp_neg_over_bitvector_declines() {
    let smt = "(set-logic QF_FP)\n(declare-const x (_ BitVec 64))\n\
               (assert (fp.isZero (fp.neg x)))\n(check-sat)\n";
    assert_declines_ill_sorted("fp.neg-over-bv", smt);
}

#[test]
fn ite_mixed_branch_sorts_declines() {
    let smt = format!(
        "(set-logic QF_FP)\n(declare-const c Bool)\n(declare-const x {FP})\n\
         (declare-const y (_ BitVec 64))\n(assert (= y (ite c x (_ bv0 64))))\n(check-sat)\n"
    );
    assert_declines_ill_sorted("ite-mixed", &smt);
}

#[test]
fn declared_rounding_mode_name_declines() {
    // `RNE` declared as a Float64 constant, then used where a rounding mode is required.
    let smt = format!(
        "(set-logic QF_FP)\n(declare-const RNE {FP})\n(declare-const x {FP})\n\
         (assert (fp.lt x ((_ to_fp 11 53) RNE 2.5)))\n(check-sat)\n"
    );
    assert_declines_ill_sorted("declared-RNE", &smt);
}

#[test]
fn non_rounding_mode_argument_declines() {
    let smt = format!(
        "(set-logic QF_FP)\n(declare-const x {FP})\n\
         (assert (fp.lt x ((_ to_fp 11 53) x 2.5)))\n(check-sat)\n"
    );
    assert_declines_ill_sorted("to_fp-var-as-rm", &smt);
}

#[test]
fn duplicate_declaration_declines() {
    let smt =
        "(set-logic QF_BV)\n(declare-const x (_ BitVec 64))\n(declare-const x (_ BitVec 8))\n\
               (assert (= x (_ bv1 64)))\n(check-sat)\n";
    assert_declines_ill_sorted("duplicate-decl", smt);
}

#[test]
fn extract_out_of_range_declines() {
    let smt = "(set-logic QF_BV)\n(declare-const x (_ BitVec 8))\n\
               (assert (= ((_ extract 15 8) x) (_ bv1 8)))\n(check-sat)\n";
    assert_declines_ill_sorted("extract-oob", smt);
}

#[test]
fn fp_literal_bad_field_widths_declines() {
    // Sign field 2 bits wide: not a Float64 literal.
    let smt = format!(
        "(set-logic QF_FP)\n(declare-const x {FP})\n\
         (assert (fp.lt x (fp #b00 #b01111111111 #b{})))\n(check-sat)\n",
        "0".repeat(52)
    );
    assert_declines_ill_sorted("fp-literal-widths", &smt);
}

#[test]
fn bv_width_mismatch_declines() {
    let smt =
        "(set-logic QF_BV)\n(declare-const x (_ BitVec 64))\n(declare-const y (_ BitVec 32))\n\
               (assert (bvult x y))\n(check-sat)\n";
    assert_declines_ill_sorted("width-mismatch", smt);
}

// ---- well-sorted controls: the checker must not over-decline ----

#[test]
fn well_sorted_bv_decides() {
    let smt = "(set-logic QF_BV)\n(declare-const x (_ BitVec 64))\n\
               (assert (bvsgt x (_ bv2 64)))\n(assert (not (bvsge x (_ bv3 64))))\n(check-sat)\n";
    assert_decides("bv-control", smt, false);
}

#[test]
fn well_sorted_fp_decides() {
    let smt = format!(
        "(set-logic QF_FP)\n(declare-const x {FP})\n(assert (= x ((_ to_fp 11 53) RNE 2.5)))\n\
         (assert (not (fp.gt x ((_ to_fp 11 53) RNE 2.0))))\n(check-sat)\n"
    );
    assert_decides("fp-control", &smt, false);
}

#[test]
fn well_sorted_string_decides() {
    let smt = "(set-logic QF_S)\n(declare-const s String)\n(declare-const t String)\n\
               (assert (= s \"a\"))\n(assert (= t s))\n(assert (not (= t \"a\")))\n(check-sat)\n";
    assert_decides("str-control", smt, false);
}

#[test]
fn well_sorted_extract_concat_decides() {
    let smt = "(set-logic QF_BV)\n(declare-const x (_ BitVec 8))\n\
               (assert (not (= (concat ((_ extract 7 4) x) ((_ extract 3 0) x)) x)))\n(check-sat)\n";
    assert_decides("extract-concat-control", smt, false);
}

#[test]
fn well_sorted_fp_literal_fields_decides() {
    // (fp #b0 #b01111111111 #b0…0) is 1.0.
    let smt = format!(
        "(set-logic QF_FP)\n(declare-const x {FP})\n\
         (assert (= x (fp #b0 #b01111111111 #b{})))\n\
         (assert (not (fp.eq x ((_ to_fp 11 53) RNE 1.0))))\n(check-sat)\n",
        "0".repeat(52)
    );
    assert_decides("fp-literal-control", &smt, false);
}

#[test]
fn well_sorted_ite_term_decides() {
    let smt = "(set-logic QF_BV)\n(declare-const c Bool)\n(declare-const x (_ BitVec 64))\n\
               (assert (= x (ite c (_ bv1 64) (_ bv2 64))))\n\
               (assert (bvugt x (_ bv2 64)))\n(check-sat)\n";
    assert_decides("ite-control", smt, false);
}

// ---- SMT-LIB `=` over Float64: every NaN is the same value ----

/// A NaN with a payload different from the canonical `(_ NaN 11 53)` pattern.
fn payload_nan() -> String {
    format!("(fp #b0 #b11111111111 #b{}1)", "0".repeat(51))
}

#[test]
fn fp_equality_nan_payloads_are_equal() {
    // In SMT-LIB every NaN is one value, so `(= nan1 nan2)` is TRUE: asserting it is SAT. Bit
    // equality says the patterns differ — UNSAT, i.e. a false PROOF of any obligation whose
    // assumptions contain this equality.
    let smt = format!(
        "(set-logic QF_FP)\n(assert (= {} (_ NaN 11 53)))\n(check-sat)\n",
        payload_nan()
    );
    assert_decides("nan-eq-sat", &smt, true);
}

#[test]
fn fp_disequality_nan_payloads_is_unsat() {
    let smt = format!(
        "(set-logic QF_FP)\n(declare-const x {FP})\n(assert (= x {}))\n\
         (assert (not (= x (_ NaN 11 53))))\n(check-sat)\n",
        payload_nan()
    );
    assert_decides("nan-neq-unsat", &smt, false);
}

#[test]
fn fp_equality_distinguishes_signed_zeros() {
    // SMT-LIB `=` (unlike fp.eq) distinguishes +0 and -0.
    let smt = format!(
        "(set-logic QF_FP)\n(declare-const x {FP})\n(assert (= x (_ +zero 11 53)))\n\
         (assert (= x (_ -zero 11 53)))\n(check-sat)\n"
    );
    assert_decides("zeros-distinct", &smt, false);
}

// ---- review round 1 (2026-09-23): inputs where native read the script differently from z3 ----

#[test]
fn quoted_symbol_resembling_a_literal_declines() {
    // `|#x0000000000000000|` is a SYMBOL (declared Float64 here), not the bit-vector literal it
    // resembles; stripping the bars typed it as the literal.
    let smt = format!(
        "(set-logic QF_FP)\n(declare-const |#x0000000000000000| {FP})\n\
         (assert (bvult |#x0000000000000000| (_ bv1 64)))\n(check-sat)\n"
    );
    assert_declines_ill_sorted("quoted-symbol", &smt);
}

#[test]
fn quoted_symbol_declines_even_when_well_sorted() {
    // Declined conservatively: the compiler never emits one.
    let smt = "(declare-const |x y| (_ BitVec 8))\n(assert (= |x y| #x01))\n(check-sat)\n";
    assert_eq!(native_check_sat_budget(smt, BUDGET), None);
}

#[test]
fn zero_argument_connectives_decline() {
    for smt in [
        "(assert (and))\n(check-sat)\n",
        "(assert (or))\n(check-sat)\n",
    ] {
        assert_declines_ill_sorted("zero-arg", smt);
    }
}

#[test]
fn stray_close_paren_declines_and_terminates() {
    // Used to loop forever: an unmatched `)` read as an empty atom without being consumed.
    let smt = "(declare-const x (_ BitVec 8))\n(assert (= x #x01))\n(check-sat))\n";
    assert_eq!(native_check_sat_budget(smt, BUDGET), None);
}

#[test]
fn unterminated_string_declines() {
    let smt = "(declare-const s String)\n(assert (= s \"abc))\n(check-sat)\n";
    assert_eq!(native_check_sat_budget(smt, BUDGET), None);
}

/// Scripts whose assertion set is not simply "every assert": the native solver conjoins all of
/// them, which is not what z3 checks. Each must decline.
#[test]
fn assertion_stack_and_multi_query_scripts_decline() {
    let base = "(declare-const x (_ BitVec 8))\n(assert (= x #x01))\n";
    for (name, tail) in [
        (
            "assert-after-check",
            "(check-sat)\n(assert (= x #x02))\n(check-sat)\n",
        ),
        (
            "push-pop",
            "(push 1)\n(assert (= x #x02))\n(pop 1)\n(check-sat)\n",
        ),
        (
            "reset",
            "(reset)\n(declare-const y (_ BitVec 8))\n(check-sat)\n",
        ),
        (
            "exit-before-check",
            "(exit)\n(assert (= x #x02))\n(check-sat)\n",
        ),
    ] {
        let smt = format!("{base}{tail}");
        assert_eq!(
            native_check_sat_budget(&smt, BUDGET),
            None,
            "{name}: native decided a multi-query / assertion-stack script"
        );
    }
    // Control: the compiler's shape (`check-sat` then `get-model`), with a trailing `exit`, decides.
    let ok = format!("{base}(check-sat)\n(get-model)\n(exit)\n");
    assert_eq!(native_check_sat_budget(&ok, BUDGET), Some(true));
}

#[test]
fn negated_real_zero_is_positive_zero() {
    // `(- 0.0)` is the real zero; `to_fp` maps it to +0 (z3: sat). Flipping the sign bit gave -0.
    let smt = "(assert (= ((_ to_fp 11 53) RNE (- 0.0)) (_ +zero 11 53)))\n(check-sat)\n";
    assert_decides("neg-real-zero", smt, true);
    let smt = "(assert (fp.isNegative ((_ to_fp 11 53) RNE (- 0.5))))\n(check-sat)\n";
    // fp.isNegative is outside the lowered fragment: it must decline, not guess.
    assert_eq!(native_check_sat_budget(smt, BUDGET), None);
    // Control: a non-zero negated literal is negative.
    let smt = "(assert (fp.lt ((_ to_fp 11 53) RNE (- 0.5)) (_ +zero 11 53)))\n(check-sat)\n";
    assert_decides("neg-half", smt, true);
}

/// String escapes are decoded exactly as SMT-LIB 2.6 / z3 do: `\u{d..}` with 1-5 hex digits or
/// `\udddd` with exactly 4, for a code point <= 0x2FFFF; everything else is literal text.
#[test]
fn string_escapes_match_z3() {
    let one_char = |lit: &str| {
        format!(
            "(declare-const s String)\n(assert (= s \"{lit}\"))\n(assert (not (= s \"A\")))\n(check-sat)\n"
        )
    };
    // These are the single character `A` — asserting s = lit and s != "A" is UNSAT.
    for lit in ["\\u{41}", "\\u{00041}", "\\u0041"] {
        assert_decides(lit, &one_char(lit), false);
    }
    // These are NOT the single character `A` — s = lit and s != "A" is SAT.
    for lit in ["\\u{+41}", "\\u{000041}", "\\u{}", "\\u041", "\\u+041"] {
        assert_decides(lit, &one_char(lit), true);
    }
    // A code point above 0x2FFFF is literal text, not one character.
    let smt = "(declare-const s String)\n(assert (= s \"\\u{30000}\"))\n\
               (assert (= s \"\\u{0}\"))\n(check-sat)\n";
    assert_decides("above-2ffff", smt, false);
    // A surrogate is a valid SMT-LIB character a Rust char cannot hold: decline.
    let smt = "(declare-const s String)\n(assert (= s \"\\u{D800}\"))\n(check-sat)\n";
    assert_eq!(native_check_sat_budget(smt, BUDGET), None);
}

// ---- review round 2 (2026-09-23) ----

#[test]
fn negated_decimal_that_underflows_is_negative_zero() {
    // A NON-zero real that rounds to zero keeps its sign: -0, which SMT-LIB `=` distinguishes from +0.
    // (Round 1's fix tested the rounded magnitude and made this +0.)
    let tiny = format!("0.{}1", "0".repeat(400));
    let smt = format!(
        "(set-logic QF_FP)\n(assert (not (= ((_ to_fp 11 53) RNE (- {tiny})) (_ +zero 11 53))))\n\
         (check-sat)\n"
    );
    assert_decides("neg-underflow", &smt, true);
    let smt = format!(
        "(set-logic QF_FP)\n(assert (= ((_ to_fp 11 53) RNE (- {tiny})) (_ -zero 11 53)))\n\
         (check-sat)\n"
    );
    assert_decides("neg-underflow-is-minus-zero", &smt, true);
}

#[test]
fn semicolon_ends_an_atom() {
    // z3 reads `p;x` as the symbol `p` followed by a comment to end of line.
    let smt =
        "(declare-const p;x Bool) (assert (not p;x)) (declare-const q\n Bool)\n(assert p;x\n)\n\
               (check-sat)\n";
    let native = native_check_sat_budget(smt, BUDGET);
    let z = z3_first_line(smt);
    if let Some(z) = z {
        let z = match z.as_str() {
            "sat" => Some(true),
            "unsat" => Some(false),
            _ => None,
        };
        assert!(
            native.is_none() || native == z,
            "native {native:?} vs z3 {z:?}"
        );
    }
    assert_ne!(
        native,
        Some(false),
        "native proved a script z3 finds satisfiable"
    );
}

#[test]
fn echo_and_set_option_decline() {
    // `echo` output becomes z3's first line, which callers read as the verdict; `set-option` can change
    // string semantics (`:encoding`). The compiler emits neither.
    for smt in [
        "(set-logic QF_BV)\n(declare-const x (_ BitVec 8))\n(echo \"unsat\")\n(assert (= x #x01))\n(check-sat)\n",
        "(set-option :encoding bmp)\n(declare-const s String)\n(assert (= s \"a\"))\n(check-sat)\n",
    ] {
        assert_eq!(native_check_sat_budget(smt, BUDGET), None, "{smt}");
    }
}

#[test]
fn sorts_and_theories_outside_the_declared_logic_decline() {
    for (name, smt) in [
        ("qf_s-bitvec", "(set-logic QF_S)\n(declare-const x (_ BitVec 8))\n(assert (not (= x x)))\n(check-sat)\n"),
        ("qf_bv-fp", "(set-logic QF_BV)\n(declare-const x (_ FloatingPoint 11 53))\n(assert (not (= x x)))\n(check-sat)\n"),
        ("qf_fp-string", "(set-logic QF_FP)\n(declare-const s String)\n(assert (not (= s s)))\n(check-sat)\n"),
        ("qf_fp-bvult", "(set-logic QF_FP)\n(assert (bvult #x01 #x02))\n(check-sat)\n"),
        ("qf_s-bvult", "(set-logic QF_S)\n(assert (bvult #x01 #x02))\n(check-sat)\n"),
        ("qf_bv-fp-literal", "(set-logic QF_BV)\n(assert (fp.isNaN (_ NaN 11 53)))\n(check-sat)\n"),
        ("double-set-logic", "(set-logic QF_BV)\n(set-logic QF_FP)\n(declare-const x (_ BitVec 8))\n(assert (not (= x x)))\n(check-sat)\n"),
    ] {
        assert_declines_ill_sorted(name, smt);
    }
    // Controls: the logics the compiler emits, with their own theories, still decide.
    assert_decides(
        "qf_bv-control",
        "(set-logic QF_BV)\n(assert (bvult #x01 #x02))\n(check-sat)\n",
        true,
    );
    assert_decides(
        "qf_fp-control",
        "(set-logic QF_FP)\n(assert (fp.isNaN (_ NaN 11 53)))\n(check-sat)\n",
        true,
    );
    assert_decides(
        "qf_s-control",
        "(set-logic QF_S)\n(declare-const s String)\n(assert (= s \"a\"))\n(check-sat)\n",
        true,
    );
}

#[test]
fn nesting_beyond_the_cap_declines_without_aborting() {
    // Used to overflow the stack and abort the process around 20,000 levels.
    let deep = 200_000;
    let smt = format!(
        "(assert {}true{})\n(check-sat)\n",
        "(not ".repeat(deep),
        ")".repeat(deep)
    );
    assert_eq!(native_check_sat_budget(&smt, BUDGET), None);
}

#[test]
fn nesting_just_under_the_cap_fits_a_default_thread_stack() {
    // The deepest ACCEPTED query must not overflow the CALLER's stack — here a 2 MiB thread, in a debug
    // build — anywhere in parse → check → blast → evaluate: the solver runs on its own stack. It must
    // also still be DECIDED, not declined.
    let depth = 1022; // even: an even number of `not`s is `true`; the cap is 1024
    let smt = format!(
        "(declare-const x (_ BitVec 8))\n(assert {}(= x #x01){})\n(check-sat)\n",
        "(not ".repeat(depth),
        ")".repeat(depth)
    );
    let r = std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || native_check_sat_budget(&smt, BUDGET))
        .unwrap()
        .join()
        .expect("native solver overflowed a 2 MiB stack below the nesting cap");
    assert_eq!(
        r,
        Some(true),
        "a query under the nesting cap must still be decided"
    );
}

// ---- review round 3 (2026-09-23) ----

#[test]
fn bar_or_quote_inside_an_atom_ends_it() {
    // z3 reads `:source|` as `:source` followed by a quoted symbol running to the next `|`, which
    // swallows `(assert false)`: z3 says sat. Reading straight through the `|` made native see the
    // assertion and prove unsat.
    for q in ["|", "\""] {
        let smt = format!(
            "(set-logic QF_BV)\n(set-info :source{q})\n(assert false)\n(set-info :x y{q})\n(check-sat)\n"
        );
        assert_ne!(
            native_check_sat_budget(&smt, BUDGET),
            Some(false),
            "native proved a script in which z3 sees no assertion ({q})"
        );
    }
}

#[test]
fn check_sat_with_arguments_declines() {
    let smt = "(declare-const x (_ BitVec 8))\n(assert (= x #x01))\n(check-sat foo)\n";
    assert_declines_ill_sorted("check-sat-args", smt);
}

#[test]
fn nested_float_equality_is_decided_quickly() {
    // Round 3: `(ite (= t y) x z)` nested k deep blew up as 3^k under the NaN-aware `=` and aborted
    // at k = 13 after 31 s. Shared operands keep it linear.
    for k in [13, 16, 32] {
        let mut t = "t".to_owned();
        for _ in 0..k {
            t = format!("(ite (= {t} y) x z)");
        }
        let smt = format!(
            "(set-logic QF_FP)\n(declare-const t {FP})\n(declare-const x {FP})\n(declare-const y {FP})\n\
             (declare-const z {FP})\n(assert (not (= {t} {t})))\n(check-sat)\n"
        );
        let start = std::time::Instant::now();
        assert_decides(&format!("nested-eq-{k}"), &smt, false);
        assert!(
            start.elapsed().as_secs() < 30,
            "k={k} took {:?}",
            start.elapsed()
        );
    }
}

// ---- workflow review (2026-09-23): solver-API divergences in the blaster's inputs ----

#[test]
fn oversized_bv_literal_denotes_its_value_mod_2_pow_w() {
    // `(_ bv256 8)` is 0, so shifting by it is the identity; unreduced, every bit was shifted out and
    // native "proved" (with a certificate) a query z3 finds satisfiable.
    for op in ["bvshl", "bvlshr"] {
        let smt = format!(
            "(declare-const x (_ BitVec 8))\n(assert (not (= ({op} x (_ bv256 8)) #x00)))\n(check-sat)\n"
        );
        assert_decides(op, &smt, true);
    }
    assert_decides(
        "literal-wraps",
        "(assert (= (_ bv257 8) #x01))\n(check-sat)\n",
        true,
    );
}

#[test]
fn shifts_wider_than_64_bits_decline() {
    // The variable-shift blaster reads at most 64 shift-amount bits: a 128-bit amount with only bit 64
    // set was treated as 0.
    let smt = "(declare-const x (_ BitVec 128))\n(declare-const s (_ BitVec 128))\n\
               (assert (= s (bvshl #x00000000000000000000000000000001 (_ bv64 128))))\n\
               (assert (not (= (bvshl x s) x)))\n(check-sat)\n";
    assert_eq!(native_check_sat_budget(smt, BUDGET), None);
    // Control: 64-bit shifts, which the compiler emits, still decide.
    assert_decides(
        "shl-64",
        "(declare-const x (_ BitVec 64))\n(assert (not (= (bvshl x (_ bv0 64)) x)))\n(check-sat)\n",
        false,
    );
}
