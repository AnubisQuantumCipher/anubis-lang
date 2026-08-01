#!/usr/bin/env bash
set -euo pipefail

# Micro-self-test for the tuple-variant extension. It intentionally feeds a tuple arm that drops
# its only field; the checker must report a problem rather than treating the arm as unparseable.
python3 - <<'PY'
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

import scripts.lib.walker_completeness as walker
from scripts.lib.walker_completeness import enum_variants

ORIGINAL_AST = walker.AST
ORIGINAL_MID = walker.MID
ORIGINAL_SIBLINGS = list(walker.SIBLINGS)
repo = Path.cwd()

identity_probe = subprocess.run(
    [
        sys.executable,
        "-c",
        "import scripts.lib.walker_completeness as w; print(w.MID.as_posix())",
    ],
    cwd=repo,
    env={**os.environ, "ANUBIS_WALKER_MID": "/tmp/untrusted-middle.rs"},
    text=True,
    capture_output=True,
    check=False,
)
if identity_probe.returncode != 0 or identity_probe.stdout.strip() != "compiler/src/middle/mod.rs":
    raise SystemExit(
        "walker_canonical_source_identity: FAIL "
        f"rc={identity_probe.returncode} stdout={identity_probe.stdout!r} "
        f"stderr={identity_probe.stderr!r}"
    )
print("walker_canonical_source_identity: PASS")

if not hasattr(walker, "scrub_source"):
    raise SystemExit("walker_source_scrub_cache_isolation: FAIL scrub_source API missing")
walker.scrub_source.cache_clear()
walker.scrub_rust.cache_clear()
source_sentinel = "fn canonical() { /* source-sized sentinel */ }"
walker.scrub_source(source_sentinel)
for index in range(32):
    walker.scrub_rust(f"fn fragment_{index}() {{}}")
hits_before = walker.scrub_source.cache_info().hits
walker.scrub_source(source_sentinel)
hits_after = walker.scrub_source.cache_info().hits
if hits_after != hits_before + 1:
    raise SystemExit(
        "walker_source_scrub_cache_isolation: FAIL source entry was evicted by fragment churn"
    )
print("walker_source_scrub_cache_isolation: PASS")

src = """pub enum Pattern {
    Binding(String),
    List(Vec<Pattern>),
}
"""
found = enum_variants(src, "Pattern")
expected = {
    "Binding": [("_0", "String")],
    "List": [("_0", "Vec<Pattern>")],
}
if found != expected:
    raise SystemExit(f"tuple_variant_parser: FAIL expected={expected!r} actual={found!r}")
print("tuple_variant_parser: PASS")

poisoned = src.replace("Binding(String)", "Binding(String, SourceSpan)")
poisoned_found = enum_variants(poisoned, "Pattern")
if poisoned_found == found or poisoned_found["Binding"] != [
    ("_0", "String"),
    ("_1", "SourceSpan"),
]:
    raise SystemExit(
        f"tuple_variant_poison: FAIL baseline={found!r} poisoned={poisoned_found!r}"
    )
print("tuple_variant_poison: PASS")

# Exercise the real AST→walker checker against a scratch source pair. This is
# the structural test the gate exists to provide: a code-holding field hidden
# behind `..` must be named as a concrete problem, while the complete twin is
# accepted. No live repository source is edited.
with tempfile.TemporaryDirectory(prefix="anubis-walker-selftest.") as tmp:
    root = Path(tmp)
    ast = root / "frontend.rs"
    mid = root / "middle.rs"
    cache_probe = root / "cache-probe.rs"
    cache_probe.write_text("fn clean() {}\n")
    cached_clean = walker.read_source(cache_probe)
    cache_stat = cache_probe.stat()
    cache_probe.write_text("fn dirty() {}\n")
    os.utime(cache_probe, ns=(cache_stat.st_atime_ns, cache_stat.st_mtime_ns))
    cached_dirty = walker.read_source(cache_probe)
    if cached_clean == cached_dirty or cached_dirty != "fn dirty() {}\n":
        raise SystemExit(
            "walker_content_identity_cache_poison: FAIL "
            f"clean={cached_clean!r} dirty={cached_dirty!r}"
        )
    print("walker_content_identity_cache_poison: PASS")
    baseline_ast = """pub enum Expr {
    If {
        cond: Box<Expr>,
        then: Box<Expr>,
    },
}
"""
    ast.write_text(baseline_ast)
    mid.write_text("""fn synthetic_walk(expr: &Expr) {
    match expr {
        Expr::If { then, .. } => synthetic_walk(then),
    }
}
""")
    walker.AST = ast
    walker.MID = mid
    walker.SIBLINGS = []
    problems = walker.check("synthetic_walk", "expr")
    if not any("never binds `cond`" in problem for problem in problems):
        raise SystemExit(f"walker_omission_detection: FAIL problems={problems!r}")
    print("walker_omission_detection: PASS")

    mid.write_text("""fn outer(expr: &Expr) {
    fn synthetic_walk(expr: &Expr) {
        match expr {
            Expr::If { cond, then } => { synthetic_walk(cond); synthetic_walk(then); },
        }
    }
    synthetic_walk(expr);
}

fn synthetic_walk(expr: &Expr) {
    match expr {
        Expr::If { then, .. } => synthetic_walk(then),
    }
}
""")
    walker._read_source.cache_clear()
    walker.walker_body.cache_clear()
    problems = walker.check("synthetic_walk", "expr")
    if not any("WALKER_UNBOUND_FIELD" in problem and "field=cond" in problem for problem in problems):
        raise SystemExit(f"walker_top_level_identity_poison: FAIL problems={problems!r}")
    print("walker_top_level_identity_poison: PASS")

    mid.write_text("""fn synthetic_walk(expr: &Expr) {
    match expr {
        Expr::If { cond, then } => { synthetic_walk(cond); synthetic_walk(then); },
    }
}
fn synthetic_walk(expr: &Expr) {
    match expr {
        Expr::If { then, .. } => synthetic_walk(then),
    }
}
""")
    walker._read_source.cache_clear()
    walker.walker_body.cache_clear()
    try:
        walker.check("synthetic_walk", "expr")
    except SystemExit as exc:
        if "AMBIGUOUS" not in str(exc):
            raise SystemExit(f"walker_duplicate_top_level_poison: FAIL wrong error={exc}")
    else:
        raise SystemExit("walker_duplicate_top_level_poison: FAIL duplicate definitions accepted")
    print("walker_duplicate_top_level_poison: PASS")

    mid.write_text("""fn outer(expr: &Expr) {
    fn synthetic_walk(expr: &Expr) {
        match expr {
            Expr::If { cond, then } => { synthetic_walk(cond); synthetic_walk(then); },
        }
    }
    synthetic_walk(expr);
}
""")
    walker._read_source.cache_clear()
    walker.walker_body.cache_clear()
    problems = walker.check("synthetic_walk", "expr")
    if problems:
        raise SystemExit(f"walker_unique_nested_identity: FAIL problems={problems!r}")
    print("walker_unique_nested_identity: PASS")

    mid.write_text("""fn synthetic_walk <'a>(expr: &'a Expr) {
    match expr {
        Expr::If { cond, then } => { synthetic_walk(cond); synthetic_walk(then); },
    }
}
""")
    walker._read_source.cache_clear()
    walker.walker_body.cache_clear()
    walker.variant_arms.cache_clear()
    problems = walker.check("synthetic_walk", "expr")
    if problems:
        raise SystemExit(f"walker_legal_function_spacing_generics: FAIL problems={problems!r}")
    print("walker_legal_function_spacing_generics: PASS")

    mid.write_text("""fn synthetic_walk(expr: &Expr) {
    match expr {
        Expr::Literal => match expr {
            Expr::If { cond, then } => { synthetic_walk(cond); synthetic_walk(then); },
            _ => {},
        },
        _ => {},
    }
}
""")
    walker._read_source.cache_clear()
    walker.walker_body.cache_clear()
    walker.variant_arms.cache_clear()
    problems = walker.check("synthetic_walk", "expr")
    if not any("WALKER_VARIANT_UNMATCHED" in problem and "Expr::If" in problem for problem in problems):
        raise SystemExit(f"walker_nested_match_ownership_poison: FAIL problems={problems!r}")
    print("walker_nested_match_ownership_poison: PASS")

    mid.write_text("""fn synthetic_walk(expr: &Expr) {
    match expr {
        Expr::If { cond: _, then } => synthetic_walk(then),
    }
}
""")
    problems = walker.check("synthetic_walk", "expr")
    if not any("never binds `cond`" in problem for problem in problems):
        raise SystemExit(f"walker_wildcard_field_poison: FAIL problems={problems!r}")
    print("walker_wildcard_field_poison: PASS")

    mid.write_text("""fn synthetic_walk(expr: &Expr) {
    match expr {
        Expr::If { cond: ns::CONST, then } => { ns; synthetic_walk(then); },
    }
}
""")
    walker._read_source.cache_clear()
    walker.walker_body.cache_clear()
    problems = walker.check("synthetic_walk", "expr")
    if not any("WALKER_UNBOUND_FIELD" in problem and "field=cond" in problem for problem in problems):
        raise SystemExit(f"walker_path_pattern_binding_poison: FAIL problems={problems!r}")
    print("walker_path_pattern_binding_poison: PASS")

    ast.write_text("""pub enum Expr {
    Var(String),
    If { cond: Box<Expr>, then: Box<Expr> },
}
""")
    mid.write_text("""fn synthetic_walk(expr: &Expr) {
    match expr {
        Expr::If { cond: Expr::Var(_name), then } => { synthetic_walk(then); },
    }
}
""")
    walker._read_source.cache_clear()
    walker.walker_body.cache_clear()
    walker.enum_variants.cache_clear()
    problems = walker.check("synthetic_walk", "partial-expr")
    if problems:
        raise SystemExit(f"walker_nested_terminal_pattern: FAIL problems={problems!r}")
    print("walker_nested_terminal_pattern: PASS")

    ast.write_text("""pub enum Expr {
    Leaf,
    Bundle { left: Box<Expr>, right: Box<Expr> },
    If { cond: Box<Expr>, then: Box<Expr> },
}
""")
    mid.write_text("""fn synthetic_walk(expr: &Expr) {
    match expr {
        Expr::If { cond: Expr::Bundle { left, right }, then } => {
            synthetic_walk(left);
            synthetic_walk(right);
            synthetic_walk(then);
        },
    }
}
""")
    walker._read_source.cache_clear()
    walker.walker_body.cache_clear()
    walker.enum_variants.cache_clear()
    problems = walker.check("synthetic_walk", "partial-expr")
    if problems:
        raise SystemExit(f"walker_nested_code_pattern_complete: FAIL problems={problems!r}")
    print("walker_nested_code_pattern_complete: PASS")

    mid.write_text("""fn synthetic_walk(expr: &Expr) {
    match expr {
        Expr::If { cond: Expr::Bundle { left, right }, then } => {
            synthetic_walk(left);
            synthetic_walk(then);
        },
    }
}
""")
    walker._read_source.cache_clear()
    walker.walker_body.cache_clear()
    problems = walker.check("synthetic_walk", "partial-expr")
    if not any("WALKER_UNDISPOSED_FIELD" in problem and "field=cond" in problem for problem in problems):
        raise SystemExit(f"walker_nested_code_use_poison: FAIL problems={problems!r}")
    print("walker_nested_code_use_poison: PASS")

    ast.write_text(baseline_ast)
    walker._read_source.cache_clear()
    walker.enum_variants.cache_clear()

    mid.write_text("""fn synthetic_walk(expr: &Expr) {
    match expr {
        Expr::Literal => true,
    }
}
""")
    walker._read_source.cache_clear()
    walker.walker_body.cache_clear()
    problems = walker.check("synthetic_walk", "partial-expr")
    if not any("WALKER_PARTIAL_VACUOUS" in problem for problem in problems):
        raise SystemExit(f"walker_partial_vacuity_poison: FAIL problems={problems!r}")
    print("walker_partial_vacuity_poison: PASS")

    # Pattern is a supported scope name, but the checker does not yet model recursive Pattern
    # fields or scalar pattern identity as code-bearing. A full scope used to turn that missing
    # model into a false green while only the partial twin failed. Both must now refuse vacuity.
    ast.write_text("""pub enum Pattern {
    Binding(String),
    List(Vec<Pattern>),
}
""")
    mid.write_text("""fn synthetic_pattern_walk(pattern: &Pattern) {
    match pattern {
        Pattern::Binding(name) => { consume(name); },
        Pattern::List(patterns) => { patterns.iter().for_each(synthetic_pattern_walk); },
    }
}
""")
    walker._read_source.cache_clear()
    walker.walker_body.cache_clear()
    walker.variant_arms.cache_clear()
    walker.enum_variants.cache_clear()
    full_pattern_problems = walker.check("synthetic_pattern_walk", "pattern")
    if not any("WALKER_SCOPE_VACUOUS" in problem for problem in full_pattern_problems):
        raise SystemExit(
            "walker_full_pattern_vacuity_poison: FAIL "
            f"problems={full_pattern_problems!r}"
        )
    print("walker_full_pattern_vacuity_poison: PASS")
    partial_pattern_problems = walker.check("synthetic_pattern_walk", "partial-pattern")
    if not any("WALKER_PARTIAL_VACUOUS" in problem for problem in partial_pattern_problems):
        raise SystemExit(
            "walker_partial_pattern_vacuity_poison: FAIL "
            f"problems={partial_pattern_problems!r}"
        )
    print("walker_partial_pattern_vacuity_poison: PASS")

    ast.write_text(baseline_ast)
    walker._read_source.cache_clear()
    walker.walker_body.cache_clear()
    walker.variant_arms.cache_clear()
    walker.enum_variants.cache_clear()

    mid.write_text("""fn synthetic_walk(expr: &Expr) {
    match expr {
        Expr::If { then, .. } => {
            let _unrelated = Expr::If { cond: then.clone(), then: then.clone() };
            synthetic_walk(then);
        },
    }
}
""")
    problems = walker.check("synthetic_walk", "expr")
    if not any("never binds `cond`" in problem for problem in problems):
        raise SystemExit(f"walker_nested_constructor_poison: FAIL problems={problems!r}")
    print("walker_nested_constructor_poison: PASS")

    mid.write_text("""fn synthetic_walk(expr: &Expr) {
    match expr {
        Expr::If { cond: _cond, then } => synthetic_walk(then),
    }
}
""")
    problems = walker.check("synthetic_walk", "expr")
    if not any("WALKER_UNDISPOSED_FIELD" in problem and "field=cond" in problem for problem in problems):
        raise SystemExit(f"walker_bound_but_unused_poison: FAIL problems={problems!r}")
    print("walker_bound_but_unused_poison: PASS")

    mid.write_text("""fn synthetic_walk(expr: &Expr) {
    match expr {
        Expr::If { cond, then } => { let _ = cond; synthetic_walk(then); },
    }
}
""")
    problems = walker.check("synthetic_walk", "expr")
    if not any("WALKER_UNDISPOSED_FIELD" in problem and "field=cond" in problem for problem in problems):
        raise SystemExit(f"walker_discard_only_poison: FAIL problems={problems!r}")
    print("walker_discard_only_poison: PASS")

    mid.write_text("""fn synthetic_summary(expr: &Expr) -> bool { synthetic_walk(expr); true }
fn synthetic_walk(expr: &Expr) {
    match expr {
        Expr::If { cond, then } => { drop(synthetic_summary(cond)); synthetic_walk(then); },
    }
}
""")
    walker._read_source.cache_clear()
    walker.walker_body.cache_clear()
    problems = walker.check("synthetic_walk", "expr")
    if problems:
        raise SystemExit(f"walker_drop_semantic_consumer: FAIL problems={problems!r}")
    print("walker_drop_semantic_consumer: PASS")

    mid.write_text("""fn synthetic_walk(expr: &Expr) {
    match expr {
        Expr::If { cond, then } => {
            let _ = { synthetic_walk(cond); true };
            synthetic_walk(then);
        },
    }
}
""")
    walker._read_source.cache_clear()
    walker.walker_body.cache_clear()
    walker.variant_arms.cache_clear()
    problems = walker.check("synthetic_walk", "expr")
    if problems:
        raise SystemExit(f"walker_discarded_block_consumer_twin: FAIL problems={problems!r}")
    print("walker_discarded_block_consumer_twin: PASS")

    mid.write_text("""fn synthetic_walk(expr: &Expr) {
    match expr {
        Expr::If { cond, then } => { cond; synthetic_walk(then); },
    }
}
""")
    walker._read_source.cache_clear()
    walker.walker_body.cache_clear()
    problems = walker.check("synthetic_walk", "expr")
    if not any("WALKER_UNDISPOSED_FIELD" in problem and "field=cond" in problem for problem in problems):
        raise SystemExit(f"walker_standalone_expression_poison: FAIL problems={problems!r}")
    print("walker_standalone_expression_poison: PASS")

    mid.write_text("""fn synthetic_walk(expr: &Expr) {
    match expr {
        Expr::If { cond, then } => { cond.clone(); synthetic_walk(then); },
    }
}
""")
    walker._read_source.cache_clear()
    walker.walker_body.cache_clear()
    walker.variant_arms.cache_clear()
    problems = walker.check("synthetic_walk", "expr")
    if not any("WALKER_UNDISPOSED_FIELD" in problem and "field=cond" in problem for problem in problems):
        raise SystemExit(f"walker_discarded_clone_poison: FAIL problems={problems!r}")
    print("walker_discarded_clone_poison: PASS")

    mid.write_text("""fn synthetic_walk(expr: &Expr) {
    match expr {
        Expr::If { cond, then } => { let _discard: &Expr = cond; synthetic_walk(then); },
    }
}
""")
    walker._read_source.cache_clear()
    walker.walker_body.cache_clear()
    problems = walker.check("synthetic_walk", "expr")
    if not any("WALKER_UNDISPOSED_FIELD" in problem and "field=cond" in problem for problem in problems):
        raise SystemExit(f"walker_typed_discard_poison: FAIL problems={problems!r}")
    print("walker_typed_discard_poison: PASS")

    mid.write_text("""fn synthetic_walk(expr: &Expr) {
    match expr {
        Expr::If { cond, then } => { let cond = then; synthetic_walk(then); },
    }
}
""")
    walker._read_source.cache_clear()
    walker.walker_body.cache_clear()
    problems = walker.check("synthetic_walk", "expr")
    if not any("WALKER_UNDISPOSED_FIELD" in problem and "field=cond" in problem for problem in problems):
        raise SystemExit(f"walker_shadow_binding_poison: FAIL problems={problems!r}")
    print("walker_shadow_binding_poison: PASS")

    mid.write_text("""fn synthetic_walk(expr: &Expr) {
    match expr {
        Expr::If { cond, then } => { let cond = then; synthetic_walk(cond); synthetic_walk(then); },
    }
}
""")
    walker._read_source.cache_clear()
    walker.walker_body.cache_clear()
    problems = walker.check("synthetic_walk", "expr")
    if not any("WALKER_UNDISPOSED_FIELD" in problem and "field=cond" in problem for problem in problems):
        raise SystemExit(f"walker_post_shadow_use_poison: FAIL problems={problems!r}")
    print("walker_post_shadow_use_poison: PASS")

    mid.write_text("""fn synthetic_walk(expr: &Expr) {
    match expr {
        Expr::If { cond, then } => {
            let (cond, _) = (then, then);
            synthetic_walk(cond);
            synthetic_walk(then);
        },
    }
}
""")
    walker._read_source.cache_clear()
    walker.walker_body.cache_clear()
    walker.variant_arms.cache_clear()
    problems = walker.check("synthetic_walk", "expr")
    if not any("WALKER_UNDISPOSED_FIELD" in problem and "field=cond" in problem for problem in problems):
        raise SystemExit(f"walker_destructuring_shadow_poison: FAIL problems={problems!r}")
    print("walker_destructuring_shadow_poison: PASS")

    mid.write_text("""fn synthetic_summary(expr: &Expr) -> bool { synthetic_walk(expr); true }
fn synthetic_walk(expr: &Expr) {
    match expr {
        Expr::If { cond, then } => {
            let cond = synthetic_summary(cond);
            drop(cond);
            synthetic_walk(then);
        },
    }
}
""")
    walker._read_source.cache_clear()
    walker.walker_body.cache_clear()
    problems = walker.check("synthetic_walk", "expr")
    if problems:
        raise SystemExit(f"walker_pre_shadow_initializer_use: FAIL problems={problems!r}")
    print("walker_pre_shadow_initializer_use: PASS")

    mid.write_text("""fn synthetic_walk(expr: &Expr) {
    match expr {
        Expr::If { cond, then } => { let _discard = cond.clone(); synthetic_walk(then); },
    }
}
""")
    problems = walker.check("synthetic_walk", "expr")
    if not any("WALKER_UNDISPOSED_FIELD" in problem and "field=cond" in problem for problem in problems):
        raise SystemExit(f"walker_derived_discard_poison: FAIL problems={problems!r}")
    print("walker_derived_discard_poison: PASS")

    mid.write_text("""fn synthetic_walk(expr: &Expr) {
    match expr {
        Expr::If { cond, then } => {
            let rebuilt = Expr::If { cond: cond.clone(), then: then.clone() };
            drop(rebuilt);
            synthetic_walk(then);
        },
    }
}
""")
    walker._read_source.cache_clear()
    walker.walker_body.cache_clear()
    problems = walker.check("synthetic_walk", "expr")
    if not any("WALKER_UNDISPOSED_FIELD" in problem and "field=cond" in problem for problem in problems):
        raise SystemExit(f"walker_reconstructed_ast_poison: FAIL problems={problems!r}")
    print("walker_reconstructed_ast_poison: PASS")

    mid.write_text("""fn synthetic_walk(expr: &Expr) {
    match expr {
        Expr::If { cond, then } => {
            let held = Some(cond);
            drop(held);
            synthetic_walk(then);
        },
    }
}
""")
    walker._read_source.cache_clear()
    walker.walker_body.cache_clear()
    walker.variant_arms.cache_clear()
    problems = walker.check("synthetic_walk", "expr")
    if not any("WALKER_UNDISPOSED_FIELD" in problem and "field=cond" in problem for problem in problems):
        raise SystemExit(f"walker_named_constructor_drop_poison: FAIL problems={problems!r}")
    print("walker_named_constructor_drop_poison: PASS")

    mid.write_text("""fn synthetic_walk(expr: &Expr) {
    match expr {
        Expr::If { cond, then } => { synthetic_walk(cond); synthetic_walk(then); },
    }
}
""")
    problems = walker.check("synthetic_walk", "expr")
    if problems:
        raise SystemExit(f"walker_complete_twin: FAIL problems={problems!r}")
    print("walker_complete_twin: PASS")

    if not hasattr(walker, "DeferredDisposition") or not hasattr(walker, "DEFERRED_FIELDS"):
        raise SystemExit("walker_deferred_contract: FAIL explicit DEFERRED contract API missing")
    original_deferred = dict(walker.DEFERRED_FIELDS)
    key = ("synthetic_walk", "Expr::If", "cond")
    try:
        synthetic_contract = walker.DeferredDisposition(
            kind="DEFERRED_AT_DEFINITION",
            rationale="synthetic definition-site deferral",
            consumer_patterns=(("alternate_consumer", "synthetic_walk", r"synthetic_alternate_consumer\s*\(\s*expr\s*\)"),),
            fallback_patterns=(("fail_closed_fallback", "synthetic_walk", r"synthetic_fail_closed\s*\(\s*expr\s*\)"),),
        )
        walker.DEFERRED_FIELDS[key] = synthetic_contract
        deferred_source = """fn synthetic_walk(expr: &Expr) {
    synthetic_alternate_consumer(expr);
    synthetic_fail_closed(expr);
    match expr {
        Expr::If { cond: _cond, then } => synthetic_walk(then),
    }
}
"""
        mid.write_text(deferred_source)
        problems = walker.check("synthetic_walk", "expr")
        if problems:
            raise SystemExit(f"walker_deferred_complete_twin: FAIL problems={problems!r}")
        print("walker_deferred_complete_twin: PASS")

        walker.DEFERRED_FIELDS[key] = walker.DeferredDisposition(
            kind="DEFERRED_FAKE",
            rationale=synthetic_contract.rationale,
            consumer_patterns=synthetic_contract.consumer_patterns,
            fallback_patterns=synthetic_contract.fallback_patterns,
        )
        problems = walker.check("synthetic_walk", "expr")
        if not any(
            "WALKER_DEFERRED_CONTRACT_MISSING" in problem
            and "requirement=kind" in problem
            for problem in problems
        ):
            raise SystemExit(f"walker_deferred_kind_poison: FAIL problems={problems!r}")
        print("walker_deferred_kind_poison: PASS")
        walker.DEFERRED_FIELDS[key] = synthetic_contract

        phantom_key = ("phantom_walk", "Expr::If", "cond")
        walker.DEFERRED_FIELDS[phantom_key] = synthetic_contract
        if not hasattr(walker, "unregistered_deferred_problems"):
            raise SystemExit("walker_deferred_unregistered_poison: FAIL registry checker missing")
        registry_problems = walker.unregistered_deferred_problems({"synthetic_walk"})
        if not any(
            "WALKER_DEFERRED_CONTRACT_UNREGISTERED" in problem
            and "walker=phantom_walk" in problem
            for problem in registry_problems
        ):
            raise SystemExit(
                f"walker_deferred_unregistered_poison: FAIL problems={registry_problems!r}"
            )
        print("walker_deferred_unregistered_poison: PASS")
        del walker.DEFERRED_FIELDS[phantom_key]

        relocated_source = deferred_source.replace(
            "    synthetic_alternate_consumer(expr);\n", ""
        ) + """
fn unrelated(expr: &Expr) {
    synthetic_alternate_consumer(expr);
}
"""
        mid.write_text(relocated_source)
        walker._read_source.cache_clear()
        walker.walker_body.cache_clear()
        problems = walker.check("synthetic_walk", "expr")
        if not any(
            "WALKER_DEFERRED_CONTRACT_MISSING" in problem
            and "requirement=alternate_consumer" in problem
            for problem in problems
        ):
            raise SystemExit(f"walker_deferred_scope_poison: FAIL problems={problems!r}")
        print("walker_deferred_scope_poison: PASS")

        mid.write_text(deferred_source.replace("    synthetic_alternate_consumer(expr);\n", ""))
        problems = walker.check("synthetic_walk", "expr")
        if not any(
            "WALKER_DEFERRED_CONTRACT_MISSING" in problem
            and "requirement=alternate_consumer" in problem
            for problem in problems
        ):
            raise SystemExit(f"walker_deferred_consumer_poison: FAIL problems={problems!r}")
        print("walker_deferred_consumer_poison: PASS")

        mid.write_text("""fn synthetic_walk(expr: &Expr) {
    match expr {
        Expr::If { cond, then } => { synthetic_walk(cond); synthetic_walk(then); },
    }
}
""")
        problems = walker.check("synthetic_walk", "expr")
        if not any("WALKER_DEFERRED_CONTRACT_UNUSED" in problem for problem in problems):
            raise SystemExit(f"walker_deferred_stale_poison: FAIL problems={problems!r}")
        print("walker_deferred_stale_poison: PASS")
    finally:
        walker.DEFERRED_FIELDS.clear()
        walker.DEFERRED_FIELDS.update(original_deferred)

walker.AST = ORIGINAL_AST
walker.MID = ORIGINAL_MID
walker.SIBLINGS = list(ORIGINAL_SIBLINGS)

for fn, scope in (("effects::walk_expr", "partial-expr"), ("analyze_expr_effect", "expr")):
    problems = walker.check(fn, scope)
    if problems:
        raise SystemExit(f"walker_real_deferred_contract_{fn}: FAIL problems={problems!r}")
print("walker_real_deferred_contracts: PASS")

with tempfile.TemporaryDirectory(prefix="anubis-walker-deferred-poison.") as tmp:
    root = Path(tmp)
    effects = root / "effects.rs"
    effects_source = (repo / "compiler/src/middle/effects.rs").read_text()
    effects.write_text(effects_source.replace(
        "                        walk_expr(body, cx, scope, row);",
        "                        let _ = body;",
        1,
    ))
    walker.SIBLINGS = [effects]
    problems = walker.check("effects::walk_expr", "partial-expr")
    if not any(
        "WALKER_DEFERRED_CONTRACT_MISSING" in problem
        and "requirement=known_hof_consumer" in problem
        for problem in problems
    ):
        raise SystemExit(f"walker_effects_hof_consumer_poison: FAIL problems={problems!r}")
    print("walker_effects_hof_consumer_poison: PASS")

    fallback_old = """            } else {
                // Unknown bare name — nothing resolvable to charge, so the tail is open.
                row.open = true;
            }
        }
        Expr::CallExpr"""
    fallback_new = """            } else {
                // poison: removed unknown-call fail-closed fallback
            }
        }
        Expr::CallExpr"""
    if fallback_old not in effects_source:
        raise SystemExit("walker_effects_unknown_fallback_poison: FAIL source anchor missing")
    effects.write_text(effects_source.replace(fallback_old, fallback_new, 1))
    walker._read_source.cache_clear()
    walker.walker_body.cache_clear()
    walker.variant_arms.cache_clear()
    problems = walker.check("effects::walk_expr", "partial-expr")
    if not any(
        "WALKER_DEFERRED_CONTRACT_MISSING" in problem
        and "requirement=unknown_bare_fallback" in problem
        for problem in problems
    ):
        raise SystemExit(f"walker_effects_unknown_fallback_poison: FAIL problems={problems!r}")
    print("walker_effects_unknown_fallback_poison: PASS")

    middle = root / "mod.rs"
    middle_source = (repo / "compiler/src/middle/mod.rs").read_text()
    hof_anchor = "for &i in effects::higher_order_closure_args(callee)"
    hof_at = middle_source.find(hof_anchor)
    consumer = "analyze_expr_effect(body, mode, &local, effects, ctx);"
    consumer_at = middle_source.find(consumer, hof_at)
    if hof_at < 0 or consumer_at < 0:
        raise SystemExit("walker_analyze_hof_consumer_poison: FAIL source anchor missing")
    middle.write_text(
        middle_source[:consumer_at] + "let _ = body;" + middle_source[consumer_at + len(consumer):]
    )
    walker.MID = middle
    walker.SIBLINGS = list(ORIGINAL_SIBLINGS)
    walker._read_source.cache_clear()
    walker.walker_body.cache_clear()
    walker.variant_arms.cache_clear()
    problems = walker.check("analyze_expr_effect", "expr")
    if not any(
        "WALKER_DEFERRED_CONTRACT_MISSING" in problem
        and "requirement=known_hof_consumer" in problem
        for problem in problems
    ):
        raise SystemExit(f"walker_analyze_hof_consumer_poison: FAIL problems={problems!r}")
    print("walker_analyze_hof_consumer_poison: PASS")

walker.AST = ORIGINAL_AST
walker.MID = ORIGINAL_MID
walker.SIBLINGS = list(ORIGINAL_SIBLINGS)
walker._read_source.cache_clear()
walker.walker_body.cache_clear()
walker.variant_arms.cache_clear()

# The two value-block label lanes must be implemented by one registered total
# statement walker. Keeping two independently registered copies is the drift
# mechanism this gate is meant to retire, not a successful burndown.
repo = Path.cwd()
gate = (repo / "scripts/run_walker_completeness_gate.sh").read_text()
middle = (repo / "compiler/src/middle/mod.rs").read_text()
registry_match = re.search(r"(?ms)^WALKERS=\(\n(.*?)^\)", gate)
if not registry_match:
    raise SystemExit("walker_shared_registration: FAIL executable WALKERS array missing")
registered = [
    code
    for line in registry_match.group(1).splitlines()
    if (code := line.split("#", 1)[0].strip())
]
expected_registered = [
    "body_has_mode_elevator",
    "analyze_expr_effect:expr",
    "walk_block_labels:stmt",
    "expr_taint_source_m:expr",
    "expr_secret_source_m:expr",
    "expr_param_flow:expr",
    "stmt_value_secret:partial-stmt",
    "stmt_value_taint:partial-stmt",
    "effects::walk_expr:partial-expr",
    "capability::walk_expr:partial-expr",
    "trifecta::walk_expr:partial-expr",
]
if registered != expected_registered:
    raise SystemExit(
        "walker_shared_registration: FAIL executable registry mismatch "
        f"expected={expected_registered!r} actual={registered!r}"
    )
if "walk_block_labels:stmt" not in registered:
    raise SystemExit("walker_shared_registration: FAIL shared walker is not registered")
if "walk_block_taint:stmt" in registered or "walk_block_secret:stmt" in registered:
    raise SystemExit("walker_shared_registration: FAIL legacy sibling walkers remain registered")
for required in (
    "if s.count(old) != 1:",
    "planted_problem_count=",
    "WALKER_COMPLETENESS: FAIL (1)",
):
    if required not in gate:
        raise SystemExit(
            f"walker_selftest_calibration: FAIL required shell invariant missing: {required}"
        )
for wrapper in ("fn walk_block_taint(", "fn walk_block_secret("):
    start = middle.find(wrapper)
    if start < 0:
        raise SystemExit(f"walker_shared_registration: FAIL missing wrapper {wrapper}")
    body = middle[start : middle.find("\n}\n", start) + 3]
    if "walk_block_labels(" not in body:
        raise SystemExit(f"walker_shared_registration: FAIL {wrapper} bypasses shared walker")
print("walker_shared_registration: PASS")
PY
