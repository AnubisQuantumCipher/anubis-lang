//! A closure that reaches itself through a reassigned name used to overflow the checker's stack.
//! It is refused with `ANUBIS_ANALYSIS_LIMIT`, and the next request in the same process is clean.
//! (Adapted from the crash-diagnosis sessions' `source_recursion_limit.rs`, 2026-09-24.)
use anubis_compiler::{
    frontend::{parse_source, Mode},
    middle::typecheck_ex,
};

const CRASH: &str =
    include_str!("../../tests/soundness/matrix/cases/limit_recursive_closure_source.anb");

#[test]
fn recursive_closure_source_reports_limit_and_recovers() {
    for verified in [false, true] {
        let err = typecheck_ex(parse_source(CRASH).unwrap(), Mode::Safe, verified).unwrap_err();
        assert!(err.starts_with("ANUBIS_ANALYSIS_LIMIT"), "{err}");
        // LSP and batch users check more than one file in the same process.
        let clean = parse_source("fn main() { let x = 1; }").unwrap();
        let result = typecheck_ex(clean, Mode::Safe, verified);
        assert!(result.is_ok(), "verified={verified}: {result:?}");
    }
}

#[test]
fn recursive_effect_expansion_is_also_bounded() {
    for source in [
        "fn main() { let g = |x| x; g = |x| g(x); print(g(1)); }",
        "fn main() { let g = |x| x; let h = |x| x; g = |x| h(x); h = |x| g(x); print(h(1)); }",
        "fn main() { let g = |x| x; g = |x| if true { g(x) } else { x }; print(g(1)); }",
        "fn main() { let g = |x| x; g = |x| g(x); g(1); }",
    ] {
        let err = typecheck_ex(parse_source(source).unwrap(), Mode::Safe, false).unwrap_err();
        assert!(err.starts_with("ANUBIS_ANALYSIS_LIMIT"), "{source}: {err}");
    }
}

#[test]
fn deep_expressions_are_not_charged() {
    let sum = (0..40)
        .map(|i| format!("\"t{i}\" + str(x)"))
        .collect::<Vec<_>>()
        .join(" + ");
    let mut block = "x".to_string();
    for i in 0..20 {
        block = format!("if x > 0 {{ let y{i} = x; {block} }} else {{ 0 }}");
    }
    for source in [
        format!("fn main() {{ let x = 3; print({sum}); }}"),
        format!("fn main() {{ let x = 3; let v = {block}; print(v); }}"),
    ] {
        let result = typecheck_ex(parse_source(&source).unwrap(), Mode::Safe, false);
        assert!(result.is_ok(), "{result:?}");
    }
}

/// 70 closures whose bodies nest 60 lists deep overflowed the command line's 8 MiB stack (whole10,
/// whole12a). On a thread with little stack the guard refuses instead; the command line itself now
/// runs on 64 MiB, where this program gets its ordinary verdict.
#[test]
fn deep_bodies_on_a_small_stack_refuse_instead_of_overflowing() {
    let mut source = String::from("fn main() {\n    let f0 = |s| s;\n");
    for i in 1..70 {
        let call = format!("f{}(s)", i - 1);
        source.push_str(&format!(
            "    let f{i} = |s| {}{call}{};\n",
            "[".repeat(60),
            "]".repeat(60)
        ));
    }
    source.push_str("    print(f69(0));\n}\n");
    let result = std::thread::Builder::new()
        .stack_size(4 << 20)
        .spawn(move || typecheck_ex(parse_source(&source).unwrap(), Mode::Safe, false))
        .unwrap()
        .join()
        .expect("the checker must not overflow its stack");
    let err = result.unwrap_err();
    assert!(err.starts_with("ANUBIS_ANALYSIS_LIMIT"), "{err}");
}
