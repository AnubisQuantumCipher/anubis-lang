#!/usr/bin/env bash
set -euo pipefail

# Micro-self-test for the tuple-variant extension. It intentionally feeds a tuple arm that drops
# its only field; the checker must report a problem rather than treating the arm as unparseable.
python3 - <<'PY'
import tempfile
from pathlib import Path

import scripts.lib.walker_completeness as walker
from scripts.lib.walker_completeness import enum_variants

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
    ast.write_text("""pub enum Expr {
    If {
        cond: Box<Expr>,
        then: Box<Expr>,
    },
}
""")
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
PY
