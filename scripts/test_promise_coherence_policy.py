#!/usr/bin/env python3
"""Behavioral tests for promise-gate scan-surface disclosure."""

from __future__ import annotations

import re
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GATE = ROOT / "scripts" / "run_promise_coherence_gate.sh"


class PromiseCoherencePolicyTests(unittest.TestCase):
    def make_fixture(self, base: Path) -> None:
        docs = base / "docs"
        docs.mkdir(parents=True)
        (docs / "CLAIMS.md").write_text(
            "Green means no KNOWN defects.\n\n"
            "## Known open issues — load-bearing\n\n1. Still open.\n",
            encoding="utf-8",
        )
        qualified = (
            "check passing means Anubis found no way for the program to violate its contracts. "
            "This is not a totality claim; see docs/CLAIMS.md for open issues.\n"
        )
        for index in range(5):
            (docs / f"promise-{index}.md").write_text(qualified, encoding="utf-8")
        universal = (
            "`anubis check` PASS means the program cannot violate its stated contracts, effects, "
            "capabilities, or information-flow policy at runtime.\n"
        )
        for directory in (".hermes", "scratchpad", "implementer", "vendor"):
            path = base / directory
            path.mkdir()
            (path / "quoted.md").write_text(universal, encoding="utf-8")

    def run_gate(self, fixture: Path) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["bash", str(GATE), "--scan-root", str(fixture)],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )

    @staticmethod
    def field(output: str, label: str) -> int:
        match = re.search(rf"^{re.escape(label)}: (\d+)$", output, re.MULTILINE)
        if match is None:
            raise AssertionError(f"missing disclosure field {label!r} in:\n{output}")
        return int(match.group(1))

    def test_excluded_universal_forms_are_counted_and_each_new_skip_is_named(self) -> None:
        with tempfile.TemporaryDirectory(prefix="anubis-promise-policy-") as raw:
            fixture = Path(raw)
            self.make_fixture(fixture)
            result = self.run_gate(fixture)
            self.assertEqual(result.returncode, 0, result.stdout)
            self.assertEqual(self.field(result.stdout, "repo-wide asserted universal forms observed"), 4)
            self.assertEqual(self.field(result.stdout, "asserted universal forms checked"), 0)
            self.assertEqual(self.field(result.stdout, "asserted universal forms excluded by policy"), 4)
            self.assertEqual(self.field(result.stdout, "policy skip roots excluded"), 4)
            self.assertNotIn("phase-0", result.stdout.lower())
            for directory in (".hermes", "scratchpad", "implementer", "vendor"):
                self.assertRegex(
                    result.stdout,
                    rf"(?m)^  {re.escape(directory)}: universal=1 product=0 — .+$",
                )

    def test_excluded_product_promises_are_counted_by_reason(self) -> None:
        with tempfile.TemporaryDirectory(prefix="anubis-promise-policy-") as raw:
            fixture = Path(raw)
            self.make_fixture(fixture)
            hidden_product = (
                "check passing means Anubis found no way for the program to violate its contracts.\n"
            )
            for directory in (".hermes", "scratchpad", "implementer", "vendor"):
                (fixture / directory / "product.md").write_text(
                    hidden_product, encoding="utf-8"
                )

            result = self.run_gate(fixture)
            self.assertEqual(result.returncode, 0, result.stdout)
            self.assertEqual(
                self.field(result.stdout, "repo-wide product promise restatements observed"), 9
            )
            self.assertEqual(
                self.field(result.stdout, "product promise restatements checked"), 5
            )
            self.assertEqual(
                self.field(result.stdout, "product promise restatements excluded by policy"), 4
            )
            self.assertEqual(
                self.field(result.stdout, "policy skip roots excluded product promises"), 4
            )
            for directory in (".hermes", "scratchpad", "implementer", "vendor"):
                self.assertRegex(
                    result.stdout,
                    rf"(?m)^  {re.escape(directory)}: universal=1 product=1 — .+$",
                )

    def test_live_universal_form_still_fails_while_excluded_inventory_remains_visible(self) -> None:
        with tempfile.TemporaryDirectory(prefix="anubis-promise-policy-") as raw:
            fixture = Path(raw)
            self.make_fixture(fixture)
            (fixture / "docs" / "live-universal.md").write_text(
                "anubis check passing => the program cannot violate its contracts.\n",
                encoding="utf-8",
            )
            result = self.run_gate(fixture)
            self.assertEqual(result.returncode, 1, result.stdout)
            self.assertEqual(self.field(result.stdout, "repo-wide asserted universal forms observed"), 5)
            self.assertEqual(self.field(result.stdout, "asserted universal forms checked"), 1)
            self.assertEqual(self.field(result.stdout, "asserted universal forms excluded by policy"), 4)
            self.assertIn("banned-universal-restatement", result.stdout)

    def test_scan_error_in_excluded_tree_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory(prefix="anubis-promise-policy-") as raw:
            fixture = Path(raw)
            self.make_fixture(fixture)
            (fixture / "vendor" / "invalid.md").write_bytes(b"\xff\xfe\x00")
            result = self.run_gate(fixture)
            self.assertEqual(result.returncode, 1, result.stdout)
            self.assertEqual(self.field(result.stdout, "scan errors"), 1)
            self.assertIn("SCAN ERROR", result.stdout)

    def test_emphasis_does_not_change_universal_findings_or_exclusion_counts(self) -> None:
        with tempfile.TemporaryDirectory(prefix="anubis-promise-policy-") as raw:
            fixture = Path(raw)
            self.make_fixture(fixture)
            active = fixture / "docs" / "live-universal.md"
            plain = "`anubis check` PASS => the program cannot violate its contracts.\n"
            emphasized = "`anubis check` *PASS* => the program cannot violate its contracts.\n"

            active.write_text(plain, encoding="utf-8")
            plain_result = self.run_gate(fixture)
            self.assertEqual(plain_result.returncode, 1, plain_result.stdout)
            self.assertEqual(
                self.field(plain_result.stdout, "asserted universal forms checked"), 1
            )
            self.assertEqual(
                self.field(plain_result.stdout, "asserted universal forms excluded by policy"), 4
            )

            active.write_text(emphasized, encoding="utf-8")
            for directory in (".hermes", "scratchpad", "implementer", "vendor"):
                (fixture / directory / "quoted.md").write_text(
                    emphasized, encoding="utf-8"
                )
            emphasized_result = self.run_gate(fixture)
            self.assertEqual(emphasized_result.returncode, 1, emphasized_result.stdout)
            self.assertEqual(
                self.field(emphasized_result.stdout, "asserted universal forms checked"), 1
            )
            self.assertEqual(
                self.field(emphasized_result.stdout, "asserted universal forms excluded by policy"),
                4,
            )


if __name__ == "__main__":
    unittest.main(verbosity=2)
