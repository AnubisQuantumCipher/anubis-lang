//! A proof bool commit returns a scalar, which takes the method-call fallback at runtime.
use anubis_compiler::{
    frontend::{parse_source, Mode},
    middle::{ifc2_findings, typecheck_ex},
};

// Copied byte-for-byte from the recovered r2p_13 fixture; inlined so CI classification
// binds the program to this test source hash.
const ORIGINAL: &str = r#"struct P { v: i64 }
struct Q { v: i64 }
impl P {
    fn m(self) {
        0
    }
}
impl Q {
    fn m(self, x) {
        0
    }
}
fn pick(c) {
    let v = if c { 7 } else { 3 };
    v
}
fn main() {
    let s: secret<i64> = 42;
    let r = proof_commit_bool("flag", P { v: 1 });
    let y = r.m(println(pick(s > 100)));
    println("end");
}
"#;
const COMMIT: &str = "proof_commit_bool(\"flag\", P { v: 1 })";
const PRINT_SECRET: &str = "println(pick(s > 100))";

fn replace_once(source: &str, from: &str, to: &str) -> String {
    assert!(source.contains(from), "fixture no longer contains {from:?}");
    source.replacen(from, to, 1)
}

fn assert_accepts(source: &str) {
    let ast = parse_source(source).expect("valid fixture");
    let findings = ifc2_findings(&ast, Mode::Safe);
    assert!(findings.is_empty(), "{source}: {findings:?}");
    let result = typecheck_ex(ast, Mode::Safe, false);
    assert!(result.is_ok(), "{source}: {result:?}");
}

fn assert_secret_exfiltration(source: &str) {
    let ast = parse_source(source).expect("valid fixture");
    let findings = ifc2_findings(&ast, Mode::Safe);
    assert!(
        findings
            .iter()
            .any(|(code, _)| code == "ANUBIS_SECRET_EXFILTRATION"),
        "{source}: {findings:?}"
    );
    let error = typecheck_ex(ast, Mode::Safe, false)
        .expect_err("the complete Safe checker must reject secret egress");
    assert!(
        error.contains("ANUBIS_SECRET_EXFILTRATION"),
        "{source}: {error}"
    );
}

#[test]
fn bool_commit_result_evaluates_method_fallback_arguments() {
    // The runtime turns the public struct into Int. It evaluates the fallback's argument,
    // unlike P::m(self), which drops the extra argument without evaluating it.
    assert_secret_exfiltration(ORIGINAL);
}

#[test]
fn scalar_receiver_and_direct_print_validate_the_rejection_controls() {
    assert_secret_exfiltration(&replace_once(ORIGINAL, COMMIT, "1"));
    assert_secret_exfiltration(
        "fn pick(c) { let v = if c { 7 } else { 3 }; v } \
         fn main() { let s: secret<i64> = 42; println(pick(s > 100)); }",
    );
}

#[test]
fn bool_commit_result_kind_survives_helper_returns() {
    let source = replace_once(ORIGINAL, COMMIT, "committed()");
    let source = format!("fn committed() {{ {COMMIT} }}\n{source}");
    assert_secret_exfiltration(&source);
}

#[test]
fn bool_commit_preserves_journal_rejection_for_secret_truth() {
    assert_secret_exfiltration(
        "fn main() { let s: secret<i64> = 42; proof_commit_bool(\"flag\", s); }",
    );
    assert_secret_exfiltration(
        "fn main() { let s: secret<i64> = 42; \
         let values = if s > 100 { [1] } else { [] }; \
         proof_commit_bool(\"flag\", values); }",
    );
}

#[test]
fn bool_commit_preserves_public_and_declassified_fallback_arguments() {
    assert_accepts(&replace_once(ORIGINAL, "secret<i64>", "i64"));
    assert_accepts(&replace_once(
        ORIGINAL,
        PRINT_SECRET,
        "println(declassify(pick(s > 100), \"test\", \"public output\"))",
    ));
}

#[test]
fn ordinary_struct_method_still_drops_unused_arguments() {
    assert_accepts(&replace_once(ORIGINAL, COMMIT, "P { v: 1 }"));
}

#[test]
fn bool_commit_accepts_public_inputs_of_supported_kinds() {
    for value in [
        "false",
        "1",
        "\"flag\"",
        "[1]",
        "{\"flag\": 1}",
        "P { v: 1 }",
    ] {
        let source = format!(
            "struct P {{ v: i64 }} fn main() {{ \
             let value = {value}; let result = proof_commit_bool(\"flag\", value); \
             println(result); }}"
        );
        assert_accepts(&source);
    }
}
