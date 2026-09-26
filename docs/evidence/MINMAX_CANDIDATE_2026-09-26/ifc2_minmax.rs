//! Comparator keys select the incumbent passed to subsequent min/max callbacks.
use anubis_compiler::{
    frontend::{parse_source, Mode},
    middle::{ifc2_findings, typecheck_ex},
};

fn findings(source: &str) -> Vec<(String, String)> {
    ifc2_findings(&parse_source(source).expect("valid fixture"), Mode::Safe)
}

#[test]
fn minmax_rechecks_secret_selected_callback_arguments() {
    for op in ["min_by", "max_by"] {
        for collection in ["[1, 2, 3]", "range(1, 4)"] {
            let source = format!(
                "fn main() {{ let s: secret<i64> = 42; \
                 let m = {op}({collection}, |x| {{ println(x); \
                 if x == 2 {{ s }} else {{ 100 }} }}); }}"
            );
            let found = findings(&source);
            assert!(
                found
                    .iter()
                    .any(|(code, _)| code == "ANUBIS_SECRET_EXFILTRATION"),
                "{source}: {found:?}"
            );
            let error = typecheck_ex(parse_source(&source).unwrap(), Mode::Safe, false)
                .expect_err("the complete Safe checker must reject the callback leak");
            assert!(error.contains("ANUBIS_SECRET_EXFILTRATION"), "{error}");
        }
    }
}

#[test]
fn minmax_preserves_fixed_callback_schedule() {
    for op in ["min_by", "max_by"] {
        for body in [
            "println(\"called\"); s",
            "println(x); declassify(s, \"test\", \"public ordering\")",
            "println(x); x",
        ] {
            let source = format!(
                "fn main() {{ let s: secret<i64> = 42; \
                 let m = {op}([1, 2, 3], |x| {{ {body} }}); }}"
            );
            let found = findings(&source);
            assert!(found.is_empty(), "{source}: {found:?}");
            let checked = typecheck_ex(parse_source(&source).unwrap(), Mode::Safe, false);
            assert!(checked.is_ok(), "{source}: {checked:?}");
        }
    }
}

#[test]
fn minmax_short_lists_do_not_feed_a_selected_winner_back() {
    for op in ["min_by", "max_by"] {
        for collection in ["[1]", "[1, 2]"] {
            let source = format!(
                "fn main() {{ let s: secret<i64> = 42; \
                 let m = {op}({collection}, |x| {{ println(x); s }}); }}"
            );
            let found = findings(&source);
            assert!(found.is_empty(), "{source}: {found:?}");
        }
        let source = format!(
            "fn main() {{ let s: secret<i64> = 42; \
             let m = {op}([1], |x| {{ println(s); x }}); }}"
        );
        let found = findings(&source);
        assert!(
            found.is_empty(),
            "singleton never invokes callback: {found:?}"
        );
    }
}

#[test]
fn minmax_feedback_covers_aliases_compound_keys_and_nested_arguments() {
    for op in ["min_by", "max_by"] {
        for (collection, body) in [
            ("[1, 2, 3]", "println(x); if x == 2 { [s] } else { [100] }"),
            (
                "[1, 2, 3]",
                "if x == 2 { println(\"selected\"); } if x == 2 { s } else { 100 }",
            ),
            (
                "[[1], [2], [3]]",
                "println(x[0]); if x[0] == 2 { s } else { 100 }",
            ),
        ] {
            let source = format!(
                "fn main() {{ let s: secret<i64> = 42; let choose = {op}; \
                 let key = |x| {{ {body} }}; let m = choose({collection}, key); }}"
            );
            let found = findings(&source);
            assert!(
                found
                    .iter()
                    .any(|(code, _)| code == "ANUBIS_SECRET_EXFILTRATION"
                        || code == "ANUBIS_IMPLICIT_FLOW"),
                "{source}: {found:?}"
            );
        }
    }
}

#[test]
fn minmax_equal_captured_keys_preserve_public_argument_transcript() {
    for op in ["min_by", "max_by"] {
        for key in ["s", "[s]"] {
            let source = format!(
                "fn main() {{ let s: secret<i64> = 42; \
                 let m = {op}([1, 2, 3], |x| {{ println(x); {key} }}); }}"
            );
            let found = findings(&source);
            assert!(
                found.is_empty(),
                "equal keys do not reveal s: {source}: {found:?}"
            );
        }
    }
}

#[test]
fn minmax_secret_length_still_controls_callback_schedule() {
    for op in ["min_by", "max_by"] {
        let source = format!(
            "fn main() {{ let s: secret<i64> = 42; \
             let m = {op}(range(1, s), |x| {{ println(\"called\"); 0 }}); }}"
        );
        let found = findings(&source);
        assert!(
            found.iter().any(|(code, _)| code == "ANUBIS_IMPLICIT_FLOW"),
            "{source}: {found:?}"
        );
    }
}
