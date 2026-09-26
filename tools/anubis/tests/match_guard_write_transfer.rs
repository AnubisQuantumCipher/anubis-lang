//! A failed value-position match guard may write an outer binding before a
//! later arm calls a contracted function. These Safe-only CLI tests require
//! both the useful satisfied path and the violated path to be classified.
use std::process::{Command, Output};

const VALID: &str =
    include_str!("../../../tests/soundness/matrix/cases/r2arm_guard_write_failed_valid.anb");
const INVALID: &str =
    include_str!("../../../tests/soundness/matrix/cases/r2arm_guard_write_failed_invalid.anb");
const DIRECT: &str = include_str!(
    "../../../tests/soundness/matrix/cases/r2arm_guard_write_failed_invalid.direct.anb"
);
const STALE_PRIOR_NONMATCH: &str = include_str!(
    "../../../tests/soundness/matrix/cases/r2arm_guard_write_stale_prior_nonmatch_invalid.anb"
);
const BINDER_SHADOW: &str = include_str!(
    "../../../tests/soundness/matrix/cases/r2arm_guard_write_binder_shadow_invalid.anb"
);
const GUARD_CALL: &str =
    include_str!("../../../tests/soundness/matrix/cases/r2arm_guard_write_call_invalid.anb");
const SHADOWED_IFLET_CALL: &str = include_str!(
    "../../../tests/soundness/matrix/cases/r2arm_nested_iflet_binder_call_invalid.anb"
);
const CALL_BEFORE_BINDER_WRITE: &str = include_str!(
    "../../../tests/soundness/matrix/cases/r2arm_guard_call_before_binder_write_valid.anb"
);

fn check(source: &str) -> Output {
    let dir = tempfile::tempdir().expect("create isolated Safe check directory");
    let file = dir.path().join("case.anb");
    std::fs::write(&file, source).expect("write Safe source");
    Command::new(env!("CARGO_BIN_EXE_anubis"))
        .arg("check")
        .arg(&file)
        .arg("--out")
        .arg(dir.path().join("out"))
        .output()
        .expect("execute Safe check")
}

fn output_text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn assert_disproved(source: &str) {
    let output = check(source);
    let text = output_text(&output);
    assert!(!output.status.success(), "violated call accepted: {text}");
    assert!(
        text.contains("ANUBIS_ASSERTION_DISPROVED"),
        "violated call was not a checked disproof: {text}"
    );
}

fn assert_undecided(source: &str) {
    let output = check(source);
    let text = output_text(&output);
    assert!(!output.status.success(), "unmodeled call accepted: {text}");
    assert!(
        text.contains("ANUBIS_ASSERTION_UNDECIDED"),
        "unmodeled call was not explicitly undecided: {text}"
    );
}

#[test]
fn exact_failed_guard_write_accepts_the_valid_twin_and_disproves_the_invalid_twins() {
    let output = check(VALID);
    assert!(
        output.status.success(),
        "valid path refused: {}",
        output_text(&output)
    );
    assert_disproved(INVALID);
    assert_disproved(DIRECT);
    assert_disproved(STALE_PRIOR_NONMATCH);
}

#[test]
fn failed_guard_still_checks_its_own_call_and_never_reuses_a_binder_as_outer_x() {
    assert_disproved(GUARD_CALL);
    // A pattern binder's assignment does not write the outer x. Its original
    // ground fact remains live for the next arm, which disproves f(0).
    assert_disproved(BINDER_SHADOW);

    let nested_binder_call = "fn f(v: i64) -> i64 requires(v > 0) { return v; }\n\
        fn main() { let x = 1; let y = 0; let z = match y {\n\
        x if if true { f(x); x = 1; false } else { false } => 0,\n\
        _ => 0 }; print(z); }";
    // The nested block's f(x) runs before its assignment. The x here is the
    // pattern binder (0), not the positive outer x.
    assert_disproved(nested_binder_call);

    // IfLet contains no statement but the walker descends its then value.
    // It must use the arm binder (0), not the same-named outer binding (1).
    assert_disproved(SHADOWED_IFLET_CALL);
}

#[test]
fn binder_guard_checks_a_call_before_its_later_write() {
    let output = check(CALL_BEFORE_BINDER_WRITE);
    assert!(
        output.status.success(),
        "valid call before guard write refused: {}",
        output_text(&output)
    );
}

#[test]
fn ambiguous_guard_writes_and_or_alternatives_remain_explicitly_unresolved() {
    let unknown_write = "fn f(v: i64) -> i64 requires(v > 0) { return v; }\n\
        fn pick(q: i64) -> i64 { let x = 0; let z = match x {\n\
        0 if if q > 0 { x = 1; false } else { false } => 0,\n\
        _ => f(x) }; return z; }\nfn main() { print(pick(1)); }";
    let or_pattern = "fn f(v: i64) -> i64 requires(v > 0) { return v; }\n\
        fn main() { let x = 0; let z = match x {\n\
        0 | 1 if if true { x = 1; false } else { false } => 0,\n\
        _ => f(x) }; print(z); }";
    let unknown_prior_guard = "fn gate() { return true; }\n\
        fn f(v: i64) -> i64 requires(v > 0) { return v; }\n\
        fn main() { let x = 0; let z = match x {\n\
        _ if gate() => 0,\n\
        0 if if true { x = 0; false } else { false } => 0,\n\
        _ => f(x) }; print(z); }";
    assert_undecided(unknown_write);
    assert_undecided(or_pattern);
    // An earlier unmodeled guard may have terminated the match. Its failed
    // path cannot be invented merely because a later exact write is known.
    assert_undecided(unknown_prior_guard);
}

#[test]
fn no_write_guard_keeps_precision_but_match_writes_do_not_escape_as_unconditional_facts() {
    let no_write = "fn f(v: i64) -> i64 requires(v > 0) { return v; }\n\
        fn main() { let x = 1; let z = match x { 0 if x > 0 => 0, _ => f(x) }; print(z); }";
    let output = check(no_write);
    assert!(
        output.status.success(),
        "no-write guard refused: {}",
        output_text(&output)
    );

    let post_match = "fn f(v: i64) -> i64 requires(v > 0) { return v; }\n\
        fn main() { let x = 1; let z = match x {\n\
        1 if if true { x = 0; false } else { false } => 0, _ => 0 };\n\
        f(x); print(z); }";
    assert_undecided(post_match);
}
