#!/usr/bin/env python3
"""Regression tests for `scripts/lib/phase3_label_census.py` and the
`scripts/run_phase3_label_census.sh` wrapper.

The tests exercise the precision issues raised in PR #27 review:

1. `FIELD_PATTERNS` must NOT match identifiers such as `.secret_fns`,
   `.secret_source`, `.secret_present`, `.tainted_call`, `.taint_source_of`,
   `.declassified_call`. The trailing negative-lookahead is the guard; a
   regression that drops it would inflate the census with dozens of
   false-positive readers (this was the pre-fix state on `.secret`).

2. A single source line containing multiple tracked accesses — including
   both a write and a read of the same field, as in
   `b.info.taint_source = b.info.taint_source.take().or(source);` — must be
   counted per occurrence, not once per line. A regression that reverts to
   first-match-only would undercount joins and merges (this was the pre-fix
   state).

3. `bash scripts/run_phase3_label_census.sh --update` must bootstrap a
   missing `docs/phase3/label_census.tsv`, writing every enumerated
   `(fn, field)` row with `<UNCLASSIFIED>` so a maintainer must
   hand-classify before landing.

The tests write synthetic Rust snippets to a temp directory and invoke the
tool with `--root` / `--source` at that path, so they do not depend on the
live `compiler/src/middle/mod.rs` contents.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TOOL = ROOT / "scripts" / "lib" / "phase3_label_census.py"
GATE = ROOT / "scripts" / "run_phase3_label_census.sh"


def run_tool(root: Path, source_rel: str) -> str:
    r = subprocess.run(
        ["python3", str(TOOL), "--root", str(root), "--source", source_rel],
        capture_output=True, text=True, check=True,
    )
    return r.stdout


def parse_rows(census: str):
    """Return {(fn, field): (writes, reads)} plus ("_totals",) key."""
    result = {}
    for line in census.strip().splitlines():
        parts = line.split("\t")
        if len(parts) != 4:
            continue
        fn, field, w, r = parts
        result[(fn, field)] = (int(w), int(r))
    return result


class LabelCensusPrecisionTests(unittest.TestCase):
    def test_word_boundary_excludes_secret_fns_and_friends(self) -> None:
        """FIELD_PATTERNS must reject `.secret_fns` etc. as false positives.

        Pre-fix (`\.secret` without a trailing negative-lookahead) counted
        `ctx.secret_fns.contains(...)` as a `secret` read and inflated the live
        census. The fix uses `(?![A-Za-z0-9_])` so an identifier suffix ends
        the match.
        """
        with tempfile.TemporaryDirectory(prefix="phase3-census-fpos-") as tmp:
            root = Path(tmp)
            src_dir = root / "compiler" / "src" / "middle"
            src_dir.mkdir(parents=True)
            src = src_dir / "mod.rs"
            src.write_text(
                "fn holder(ctx: &Ctx) {\n"
                "    if ctx.secret_fns.contains(&n) { return; }\n"
                "    let s = ctx.secret_source.clone();\n"
                "    let p = ctx.secret_present;\n"
                "    let t = ctx.tainted_call;\n"
                "    let u = ctx.taint_source_of(&x);\n"
                "    let d = ctx.declassified_call;\n"
                "}\n"
            )
            census = parse_rows(run_tool(root, "compiler/src/middle/mod.rs"))
            self.assertNotIn(("holder", "secret"), census)
            self.assertNotIn(("holder", "taint_source"), census)
            self.assertNotIn(("holder", "tainted"), census)
            self.assertNotIn(("holder", "declassified"), census)
            self.assertEqual(census.get(("__totals__", "-"), (0, 0)), (0, 0))

    def test_per_occurrence_counting_on_multi_access_line(self) -> None:
        """A line with N tracked accesses must contribute N counts.

        The canonical shape is the merge-preserving read/write pair
        `b.info.taint_source = b.info.taint_source.take().or(source);` —
        one write plus one read of the same field on one line. Pre-fix
        `enumerate_sites` recorded only the first match on each line, so
        this pair became a single write; the read was silently lost from
        the merge/join census.
        """
        with tempfile.TemporaryDirectory(prefix="phase3-census-multi-") as tmp:
            root = Path(tmp)
            src_dir = root / "compiler" / "src" / "middle"
            src_dir.mkdir(parents=True)
            src = src_dir / "mod.rs"
            src.write_text(
                "fn merge(b: &mut Binding, source: Option<String>) {\n"
                "    b.info.taint_source = b.info.taint_source.take().or(source);\n"
                "    b.info.tainted = b.info.tainted || true;\n"
                "    b.secret = b.secret || false;\n"
                "}\n"
            )
            census = parse_rows(run_tool(root, "compiler/src/middle/mod.rs"))
            self.assertEqual(census[("merge", "taint_source")], (1, 1))
            self.assertEqual(census[("merge", "tainted")], (1, 1))
            self.assertEqual(census[("merge", "secret")], (1, 1))
            # Totals reflect every occurrence, not one-per-line.
            self.assertEqual(census[("__totals__", "-")], (3, 3))

    def test_word_boundary_still_matches_the_real_fields(self) -> None:
        """Sanity: the negative-lookahead does not break legitimate matches."""
        with tempfile.TemporaryDirectory(prefix="phase3-census-real-") as tmp:
            root = Path(tmp)
            src_dir = root / "compiler" / "src" / "middle"
            src_dir.mkdir(parents=True)
            src = src_dir / "mod.rs"
            src.write_text(
                "fn writer(b: &mut Binding) {\n"
                "    b.info.tainted = true;\n"
                "    b.info.taint_source = Some(\"src\".to_string());\n"
                "    b.info.declassified = false;\n"
                "    b.secret = true;\n"
                "}\n"
                "fn reader(b: &Binding) -> bool {\n"
                "    let a = b.info.tainted;\n"
                "    let c = b.secret;\n"
                "    a || c\n"
                "}\n"
            )
            census = parse_rows(run_tool(root, "compiler/src/middle/mod.rs"))
            self.assertEqual(census[("writer", "tainted")], (1, 0))
            self.assertEqual(census[("writer", "taint_source")], (1, 0))
            self.assertEqual(census[("writer", "declassified")], (1, 0))
            self.assertEqual(census[("writer", "secret")], (1, 0))
            self.assertEqual(census[("reader", "tainted")], (0, 1))
            self.assertEqual(census[("reader", "secret")], (0, 1))
            self.assertEqual(census[("__totals__", "-")], (4, 2))

    def test_comparison_operators_are_not_writes(self) -> None:
        """`==`, `!=`, `<=`, `>=` after a field access must count as reads."""
        with tempfile.TemporaryDirectory(prefix="phase3-census-cmp-") as tmp:
            root = Path(tmp)
            src_dir = root / "compiler" / "src" / "middle"
            src_dir.mkdir(parents=True)
            src = src_dir / "mod.rs"
            src.write_text(
                "fn compare(b: &Binding) -> bool {\n"
                "    b.info.tainted == true\n"
                "        && b.secret != false\n"
                "}\n"
            )
            census = parse_rows(run_tool(root, "compiler/src/middle/mod.rs"))
            self.assertEqual(census[("compare", "tainted")], (0, 1))
            self.assertEqual(census[("compare", "secret")], (0, 1))

    def test_fat_arrow_after_access_is_a_read(self) -> None:
        """Macro-token `field =>` syntax must not look like assignment."""
        with tempfile.TemporaryDirectory(prefix="phase3-census-fat-arrow-") as tmp:
            root = Path(tmp)
            src = root / "mod.rs"
            src.write_text(
                "fn token_rule(b: &Binding) {\n"
                "    route!(b.secret => sink);\n"
                "}\n"
            )
            census = parse_rows(run_tool(root, "mod.rs"))
            self.assertEqual(census[("token_rule", "secret")], (0, 1))

    def test_struct_literal_label_initializers_are_writes(self) -> None:
        """Explicit and shorthand label fields belong to their literal owner."""
        with tempfile.TemporaryDirectory(prefix="phase3-census-literal-") as tmp:
            root = Path(tmp)
            src = root / "mod.rs"
            src.write_text(
                "struct ScopeBinding { secret: bool }\n"
                "impl ScopeBinding { fn is_secret(&self) -> bool { false } }\n"
                "fn constructors(taint_source: Option<String>, declassified: bool,\n"
                "                secret: bool) -> ScopeBinding {\n"
                "    ScopeBinding {\n"
                "        info: BindingInfo {\n"
                "            tainted: true,\n"
                "            taint_source,\n"
                "            declassified,\n"
                "        },\n"
                "        secret,\n"
                "    }\n"
                "}\n"
            )
            census = parse_rows(run_tool(root, "mod.rs"))
            self.assertEqual(census[("constructors", "tainted")], (1, 0))
            self.assertEqual(census[("constructors", "taint_source")], (1, 0))
            self.assertEqual(census[("constructors", "declassified")], (1, 0))
            self.assertEqual(census[("constructors", "secret")], (1, 0))
            self.assertNotIn(("<toplevel>", "secret"), census)

    def test_full_line_comment_lines_are_skipped(self) -> None:
        """`//` prose that mentions the fields must NOT count."""
        with tempfile.TemporaryDirectory(prefix="phase3-census-doc-") as tmp:
            root = Path(tmp)
            src_dir = root / "compiler" / "src" / "middle"
            src_dir.mkdir(parents=True)
            src = src_dir / "mod.rs"
            src.write_text(
                "fn documented() {\n"
                "    // this fn discusses b.info.tainted and b.secret in prose\n"
                "    /// same for a doc comment: b.info.taint_source\n"
                "    let x = 1;\n"
                "}\n"
            )
            census = parse_rows(run_tool(root, "compiler/src/middle/mod.rs"))
            self.assertEqual(census.get(("documented", "tainted")), None)
            self.assertEqual(census.get(("documented", "secret")), None)
            self.assertEqual(census.get(("documented", "taint_source")), None)

    def test_literals_and_inline_comments_do_not_count(self) -> None:
        """Tracked field spellings outside Rust code must not enter the census."""
        with tempfile.TemporaryDirectory(prefix="phase3-census-non-code-") as tmp:
            root = Path(tmp)
            src = root / "mod.rs"
            src.write_text(
                "fn prose() {\n"
                "    let normal = \"b.secret\";\n"
                "    let raw = r#\"b.info.tainted\"#;\n"
                "    let x = 1; // b.info.taint_source\n"
                "    /* b.info.declassified */\n"
                "}\n"
            )
            census = parse_rows(run_tool(root, "mod.rs"))
            self.assertEqual(census[("__totals__", "-")], (0, 0))

    def test_nested_local_fn_does_not_steal_following_outer_sites(self) -> None:
        """Field accesses after a local helper still belong to the outer function."""
        with tempfile.TemporaryDirectory(prefix="phase3-census-nested-fn-") as tmp:
            root = Path(tmp)
            src_dir = root / "compiler" / "src" / "middle"
            src_dir.mkdir(parents=True)
            src = src_dir / "mod.rs"
            src.write_text(
                "fn outer(b: &mut Binding) {\n"
                "    fn local_helper() { let brace = \"}\"; }\n"
                "    b.info.tainted = true;\n"
                "    if true { b.secret = true; }\n"
                "}\n"
                "fn sibling(b: &Binding) -> bool { b.info.tainted }\n"
            )
            census = parse_rows(run_tool(root, "compiler/src/middle/mod.rs"))
            self.assertEqual(census[("outer", "tainted")], (1, 0))
            self.assertEqual(census[("outer", "secret")], (1, 0))
            self.assertEqual(census[("sibling", "tainted")], (0, 1))
            self.assertNotIn(("local_helper", "tainted"), census)
            self.assertNotIn(("local_helper", "secret"), census)

    def test_array_semicolon_in_signature_does_not_end_function(self) -> None:
        """A `[T; N]` type is not a declaration-only function terminator."""
        with tempfile.TemporaryDirectory(prefix="phase3-census-array-signature-") as tmp:
            root = Path(tmp)
            src = root / "mod.rs"
            src.write_text(
                "fn array_param(buf: [u8; 4], b: &mut Binding) {\n"
                "    b.info.tainted = buf[0] != 0;\n"
                "}\n"
            )
            census = parse_rows(run_tool(root, "mod.rs"))
            self.assertEqual(census[("array_param", "tainted")], (1, 0))
            self.assertNotIn(("<toplevel>", "tainted"), census)

    def test_two_same_line_functions_keep_distinct_owners(self) -> None:
        """Each same-line `fn` owns its body and following lines."""
        with tempfile.TemporaryDirectory(prefix="phase3-census-same-line-fns-") as tmp:
            root = Path(tmp)
            src = root / "mod.rs"
            src.write_text(
                "fn first(a: &mut Binding) { a.secret = true; } "
                "fn second(b: &mut Binding) {\n"
                "    b.info.tainted = true;\n"
                "}\n"
            )
            census = parse_rows(run_tool(root, "mod.rs"))
            self.assertEqual(census[("first", "secret")], (1, 0))
            self.assertEqual(census[("second", "tainted")], (1, 0))
            self.assertNotIn(("<toplevel>", "tainted"), census)

    def test_escaped_backslash_char_does_not_expose_later_brace(self) -> None:
        """`'\\\\'` must not swallow a later char literal and expose its brace."""
        with tempfile.TemporaryDirectory(prefix="phase3-census-backslash-char-") as tmp:
            root = Path(tmp)
            src = root / "mod.rs"
            src.write_text(
                "fn chars(b: &mut Binding) {\n"
                "    let pair = ('\\\\', '{');\n"
                "    b.secret = true;\n"
                "}\n"
            )
            census = parse_rows(run_tool(root, "mod.rs"))
            self.assertEqual(census[("chars", "secret")], (1, 0))

    def test_parse_failure_is_reported_without_traceback(self) -> None:
        """Malformed input fails through the tool's stable rc=2 error path."""
        with tempfile.TemporaryDirectory(prefix="phase3-census-parse-error-") as tmp:
            root = Path(tmp)
            src = root / "mod.rs"
            src.write_text("fn broken(b: &Binding) {\n    b.secret\n")
            result = subprocess.run(
                ["python3", str(TOOL), "--root", str(root), "--source", "mod.rs"],
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 2)
            self.assertIn("phase3_label_census: cannot parse", result.stderr)
            self.assertNotIn("Traceback", result.stderr)


class LabelCensusGateBootstrapTests(unittest.TestCase):
    """`--update` on the wrapper must bootstrap a missing expectation."""

    def test_update_bootstraps_missing_expectation_file(self) -> None:
        with tempfile.TemporaryDirectory(prefix="phase3-census-boot-") as tmp:
            fake_root = Path(tmp)
            # Copy the required repo scaffolding into the temp root.
            (fake_root / "scripts" / "lib").mkdir(parents=True)
            (fake_root / "compiler" / "src" / "middle").mkdir(parents=True)
            shutil.copy(TOOL, fake_root / "scripts" / "lib" / TOOL.name)
            shutil.copy(GATE, fake_root / "scripts" / GATE.name)
            (fake_root / "scripts" / GATE.name).chmod(0o755)
            (fake_root / "compiler" / "src" / "middle" / "mod.rs").write_text(
                "fn one(b: &mut Binding) { b.info.tainted = true; }\n"
                "fn two(b: &Binding) -> bool { b.secret }\n"
            )
            expect = fake_root / "docs" / "phase3" / "label_census.tsv"
            self.assertFalse(expect.exists(), "expectation must be absent at start")

            env = dict(os.environ)
            env["GITHUB_WORKSPACE"] = str(fake_root)
            r = subprocess.run(
                ["bash", str(fake_root / "scripts" / GATE.name), "--update"],
                cwd=fake_root, capture_output=True, text=True, env=env,
            )
            self.assertEqual(r.returncode, 0, f"--update failed:\n{r.stdout}\n{r.stderr}")
            self.assertIn("PHASE_3_LABEL_CENSUS: UPDATED", r.stdout)
            self.assertTrue(expect.exists(), "bootstrap did not create the expectation")

            # Every enumerated row should be <UNCLASSIFIED>, so a normal
            # run must FAIL until a maintainer hand-classifies each row.
            r = subprocess.run(
                ["bash", str(fake_root / "scripts" / GATE.name)],
                cwd=fake_root, capture_output=True, text=True, env=env,
            )
            self.assertEqual(r.returncode, 1)
            self.assertIn("unclassified writer/reader", r.stdout)

    def test_missing_root_operand_emits_declared_failure(self) -> None:
        result = subprocess.run(
            ["bash", str(GATE), "--root"],
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("PHASE_3_LABEL_CENSUS: FAIL", result.stdout)
        self.assertNotIn("unbound variable", result.stderr)

    def test_parser_failure_emits_declared_gate_failure(self) -> None:
        with tempfile.TemporaryDirectory(prefix="phase3-census-gate-error-") as tmp:
            fake_root = Path(tmp)
            (fake_root / "scripts" / "lib").mkdir(parents=True)
            (fake_root / "compiler" / "src" / "middle").mkdir(parents=True)
            (fake_root / "docs" / "phase3").mkdir(parents=True)
            shutil.copy(TOOL, fake_root / "scripts" / "lib" / TOOL.name)
            shutil.copy(GATE, fake_root / "scripts" / GATE.name)
            (fake_root / "scripts" / GATE.name).chmod(0o755)
            (fake_root / "compiler" / "src" / "middle" / "mod.rs").write_text(
                "fn broken(b: &Binding) {\n    b.secret\n"
            )
            (fake_root / "docs" / "phase3" / "label_census.tsv").write_text(
                "fn\tfield\twrites\treads\tkind\ttarget_slice\tnotes\n"
                "__totals__\t-\t0\t0\t-\t-\ttotals\n"
            )

            for extra_args in ([], ["--update"]):
                with self.subTest(extra_args=extra_args):
                    result = subprocess.run(
                        ["bash", str(fake_root / "scripts" / GATE.name), *extra_args],
                        cwd=fake_root,
                        capture_output=True,
                        text=True,
                    )
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn("PHASE_3_LABEL_CENSUS: FAIL", result.stdout)


if __name__ == "__main__":
    unittest.main(verbosity=2)
