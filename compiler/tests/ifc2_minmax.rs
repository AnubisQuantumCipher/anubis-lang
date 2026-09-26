//! Comparator keys select the incumbent passed to subsequent min/max callbacks.
use anubis_compiler::{
    frontend::{parse_source, Mode},
    middle::{ifc2_findings, typecheck_ex},
};

fn findings(source: &str) -> Vec<(String, String)> {
    ifc2_findings(&parse_source(source).expect("valid fixture"), Mode::Safe)
}

fn assert_accepts(source: &str) {
    let found = findings(source);
    assert!(found.is_empty(), "{source}: {found:?}");
    let checked = typecheck_ex(parse_source(source).unwrap(), Mode::Safe, false);
    assert!(checked.is_ok(), "{source}: {checked:?}");
}

// These valid workloads are still refused by the older ordinary lane, including on the
// baseline pin. Keep their ACCEPT matrix intents; IFC2 precision is tested separately here.
fn assert_ifc2_accepts_with_open_union_precision(source: &str) {
    let found = findings(source);
    assert!(found.is_empty(), "{source}: {found:?}");
}

fn assert_secret_exfiltration(source: &str) {
    let found = findings(source);
    assert!(
        found
            .iter()
            .any(|(code, _)| code == "ANUBIS_SECRET_EXFILTRATION"),
        "{source}: {found:?}"
    );
    let error = typecheck_ex(parse_source(source).unwrap(), Mode::Safe, false)
        .expect_err("the complete Safe checker must reject secret egress");
    assert!(error.contains("ANUBIS_SECRET_EXFILTRATION"), "{error}");
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
            assert_accepts(&source);
        }
        let source = format!(
            "fn main() {{ let s: secret<i64> = 42; \
             let m = {op}([1], |x| {{ println(s); x }}); }}"
        );
        assert_ifc2_accepts_with_open_union_precision(&source);
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
        for collection in ["[1, 2, 3]", "range(1, 4)"] {
            for key in ["s", "[s]", "[[s], [s, 100]]"] {
                let source = format!(
                    "fn main() {{ let s: secret<i64> = 42; \
                     let m = {op}({collection}, |x| {{ println(x); {key} }}); }}"
                );
                assert_accepts(&source);
            }
        }
    }
}

#[test]
fn minmax_repeatable_keys_follow_aliases_rebinding_and_shadowing() {
    for op in ["min_by", "max_by"] {
        for body in [
            "println(x); let key = s; key",
            "println(x); let key = 0; key = s; key",
            "println(x); let key = x; key = s; key",
            "println(x); let key = s; let s = x; key",
            "println(x); let s = [s]; s",
            "println(x); let key = [s]; let alias = key; [[alias], [key]]",
        ] {
            let source = format!(
                "fn main() {{ let s: secret<i64> = 42; let choose = {op}; \
                 let key = |x| {{ {body} }}; let alias = key; \
                 let m = choose([1, 2, 3], alias); }}"
            );
            assert_accepts(&source);
        }
    }
}

#[test]
fn minmax_repeatable_explicit_returns_preserve_public_argument_transcript() {
    for op in ["min_by", "max_by"] {
        for body in [
            "println(x); return s;",
            "let key = s; println(x); return [key];",
            "println(x); return s; x",
        ] {
            let source = format!(
                "fn main() {{ let s: secret<i64> = 42; \
                 let m = {op}([1, 2, 3], |x| {{ {body} }}); }}"
            );
            assert_accepts(&source);
        }
    }
}

#[test]
fn minmax_repeatability_is_per_fixed_capture_snapshot() {
    for op in ["min_by", "max_by"] {
        let source = format!(
            "fn make(s) {{ return |x| {{ println(x); s }}; }} \
             fn main() {{ let a: secret<i64> = 42; let b: secret<i64> = 142; \
             let left = make(a); let right = make(b); \
             let key = if true {{ left }} else {{ right }}; \
             let m = {op}([1, 2, 3], key); }}"
        );
        assert_accepts(&source);
    }
}

#[test]
fn minmax_observation_captured_before_callback_is_fixed() {
    for op in ["min_by", "max_by"] {
        let source = format!(
            "fn main() uses(rand.gen) {{ let s: secret<f64> = random(); \
             let m = {op}([1, 2, 3], |x| {{ println(x); s }}); }}"
        );
        assert_accepts(&source);
    }
}

#[test]
fn minmax_repeatable_keys_preserve_selected_value_labels() {
    for op in ["min_by", "max_by"] {
        for collection in ["[1, 2, 3]", "range(1, 4)"] {
            let source = format!(
                "fn main() {{ let s: secret<i64> = 42; \
                 let m = {op}({collection}, |x| s); println(m); }}"
            );
            assert_ifc2_accepts_with_open_union_precision(&source);
            let source = format!(
                "fn main() {{ let s: secret<i64> = 42; \
                 let m = {op}({collection}, |x| if x == 2 {{ s }} else {{ 100 }}); \
                 println(m); }}"
            );
            assert_secret_exfiltration(&source);
        }
        let source = format!(
            "fn main() {{ let s: secret<i64> = 42; \
             let m = {op}([s, 100, 200], |x| {{ println(x); 0 }}); }}"
        );
        assert_secret_exfiltration(&source);
    }
}

#[test]
fn minmax_mutation_and_early_return_cannot_retain_a_stale_repeatability_proof() {
    for op in ["min_by", "max_by"] {
        for body in [
            "println(x); s = if x == 2 { s } else { 100 }; s",
            "println(x); let saved = s; s = x; if s == 2 { saved } else { 100 }",
            "println(x); key[0] = if x == 2 { s } else { 100 }; key",
            "println(x); if x != 2 { s = 100; } s",
            "println(x); let key = [0]; push(key, if x == 2 { s } else { 100 }); key",
            "println(if true { key[0] = if x == 2 { s } else { 100 }; x } else { x }); key",
            "println(x); return if x == 2 { s } else { 100 }; s",
            "println(x); println(if true { return if x == 2 { s } else { 100 }; x } else { x }); s",
        ] {
            // Each normally used key varies with the current argument and secret.
            // Merely overwriting a capture with public x would not be a leak.
            let source = format!(
                "fn main() {{ let s: secret<i64> = 42; let key = [s]; \
                 let m = {op}([1, 2, 3], |x| {{ {body} }}); }}"
            );
            assert_secret_exfiltration(&source);
        }
    }
}

#[test]
fn minmax_unmodeled_calls_do_not_prove_secret_keys_repeatable() {
    for op in ["min_by", "max_by"] {
        for (helper, body) in [
            (
                "fn key(s, x) { return if x == 2 { s } else { 100 }; }",
                "println(x); key(s, x)",
            ),
            (
                "fn random(s, x) { return if x == 2 { s } else { 100 }; }",
                "println(x); random(s, x)",
            ),
            ("", "println(x); if x == 2 { s } else { random() }"),
        ] {
            let source = format!(
                "{helper} fn main() uses(rand.gen) {{ let s: secret<i64> = 42; \
                 let m = {op}([1, 2, 3], |x| {{ {body} }}); }}"
            );
            assert_secret_exfiltration(&source);
        }
    }
}

#[test]
fn minmax_repeatable_keys_do_not_authorize_direct_key_egress() {
    for op in ["min_by", "max_by"] {
        for body in ["println(s); s", "let key = [s]; println(key); key"] {
            let source = format!(
                "fn main() {{ let s: secret<i64> = 42; \
                 let m = {op}([1, 2, 3], |x| {{ {body} }}); }}"
            );
            assert_secret_exfiltration(&source);
        }
    }
}

#[test]
fn minmax_repeatable_keys_preserve_secret_callable_control() {
    for op in ["min_by", "max_by"] {
        let source = format!(
            "fn main() {{ let s: secret<i64> = 42; \
             let key = if s == 42 {{ |x| {{ println(\"left\"); s }} }} \
             else {{ |x| {{ println(\"right\"); s }} }}; \
             let m = {op}([1, 2, 3], key); }}"
        );
        let found = findings(&source);
        assert!(
            found.iter().any(|(code, _)| code == "ANUBIS_IMPLICIT_FLOW"),
            "{source}: {found:?}"
        );
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

#[test]
fn minmax_unknown_length_rechecks_lifted_callback_continuation() {
    for op in ["min_by", "max_by"] {
        let source = format!(
            "fn main() {{ let s: secret<i64> = 42; \
             let m = {op}(range(1, 4), |x| {{ println(\"called\"); \
             if s == 42 {{ exit(0); }} 0 }}); }}"
        );
        let found = findings(&source);
        assert!(
            found.iter().any(|(code, _)| code == "ANUBIS_IMPLICIT_FLOW"),
            "{source}: {found:?}"
        );
    }
}

#[test]
fn minmax_comparison_of_fixed_capture_remains_valid() {
    for op in ["min_by", "max_by"] {
        assert_accepts(&format!(
            "fn main() {{ let s: secret<i64> = 42; \
             let m = {op}([1, 2, 3], |x| {{ println(x); s > 0 }}); }}"
        ));
    }
}

#[test]
fn minmax_negation_of_fixed_capture_remains_valid() {
    for op in ["min_by", "max_by"] {
        assert_accepts(&format!(
            "fn main() {{ let s: secret<i64> = 42; \
             let m = {op}([1, 2, 3], |x| {{ println(x); -s }}); }}"
        ));
    }
}

#[test]
fn minmax_fixed_capture_through_deterministic_helper_remains_valid() {
    for op in ["min_by", "max_by"] {
        assert_accepts(&format!(
            "fn fixed(s) {{ s }} fn main() {{ let s: secret<i64> = 42; \
             let m = {op}([1, 2, 3], |x| {{ println(x); fixed(s) }}); }}"
        ));
    }
}

#[test]
fn minmax_padded_lambda_formals_keep_fixed_secret_keys_valid() {
    // These are the exact frozen sources with source-bound baseline PASS and identical public
    // callback transcripts for distinct captured secret values. Only x is supplied by min/max.
    for source in [
        include_str!(
            "../../tests/soundness/matrix/cases/ifc2r2_minmax_min_by_padded_fixed_key_valid.anb"
        ),
        include_str!(
            "../../tests/soundness/matrix/cases/ifc2r2_minmax_max_by_padded_fixed_key_valid.anb"
        ),
    ] {
        assert_accepts(source);
    }
}

#[test]
fn minmax_fixed_keys_keep_other_admitted_unary_comparison_and_helper_forms() {
    for op in ["min_by", "max_by"] {
        for key in [
            "!s",
            "~s",
            "s < 0",
            "s <= 0",
            "s >= 0",
            "s == 0",
            "s != 0",
            "[[-s], [-s]]",
        ] {
            assert_accepts(&format!(
                "fn main() {{ let s: secret<i64> = 42; \
                 let m = {op}([1, 2, 3], |x| {{ println(x); {key} }}); }}"
            ));
        }
        for (helper, key) in [
            ("fn fixed(v) { return v; }", "fixed(s)"),
            ("fn fixed(v) { v } fn forward(v) { fixed(v) }", "forward(s)"),
            ("fn choose(ignored, fixed) { fixed }", "choose(x, s)"),
            ("fn fixed(v, public) { println(public); v }", "fixed(s, x)"),
        ] {
            assert_accepts(&format!(
                "{helper} fn main() {{ let s: secret<i64> = 42; \
                 let m = {op}([1, 2, 3], |x| {{ println(x); {key} }}); }}"
            ));
        }
    }
}

#[test]
fn minmax_varying_keys_through_admitted_terms_or_helpers_still_reject() {
    for op in ["min_by", "max_by"] {
        for (helper, key) in [
            ("", "if x == 2 { s } else { 100 }"),
            ("", "if x == 2 { s > 0 } else { false }"),
            ("fn key(v, x) { if x == 2 { v } else { 100 } }", "key(s, x)"),
            (
                "fn key(v, x) { if x == 2 { -v } else { 100 } }",
                "key(s, x)",
            ),
        ] {
            assert_secret_exfiltration(&format!(
                "{helper} fn main() {{ let s: secret<i64> = 42; \
                 let m = {op}([1, 2, 3], |x| {{ println(x); {key} }}); }}"
            ));
        }
    }
}

#[test]
fn minmax_unmodeled_helper_boundaries_do_not_prove_repeatability() {
    for op in ["min_by", "max_by"] {
        for helper in [
            "fn key(v: u32, x) { if x == 2 { v } else { 100 } }",
            "fn key(v, x) -> u32 { if x == 2 { v } else { 100 } }",
            "fn key<T>(v: T, x) { if x == 2 { v } else { 100 } }",
        ] {
            assert_secret_exfiltration(&format!(
                "{helper} fn main() {{ let s: secret<i64> = 42; \
                 let m = {op}([1, 2, 3], |x| {{ println(x); key(s, x) }}); }}"
            ));
        }
    }
}

#[test]
fn minmax_originally_unannotated_struct_helper_keeps_public_callback_logging() {
    // D9 currently infers public `Row` for the unannotated helper, so the complete checker
    // refuses these valid sources at ANUBIS_SECRET_TO_PUBLIC on both the pre-change and
    // candidate pins. They remain registered ACCEPT precision defects. Check this lane's
    // contribution here; do not turn the full-check refusal into an expected security result.
    for source in [
        include_str!(
            "../../tests/soundness/matrix/cases/ifc2r2_minmax_min_by_unannotated_row_helper_valid.anb"
        ),
        include_str!(
            "../../tests/soundness/matrix/cases/ifc2r2_minmax_max_by_unannotated_row_helper_valid.anb"
        ),
    ] {
        assert_ifc2_accepts_with_open_union_precision(source);
    }
}
