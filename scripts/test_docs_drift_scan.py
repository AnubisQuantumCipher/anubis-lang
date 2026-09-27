#!/usr/bin/env python3
import importlib.util
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).parent / "lib" / "docs_drift_scan.py"
SPEC = importlib.util.spec_from_file_location("docs_drift_scan", MODULE_PATH)
assert SPEC and SPEC.loader
SCAN = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SCAN)


class DocsDriftScannerTests(unittest.TestCase):
    def test_historical_clause_cannot_exempt_a_current_language_claim(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "README.md").write_text(
                "Language core 259/259 is the current live count; historical W1 was smaller.\n",
                encoding="utf-8",
            )
            failures, stamps, _ = SCAN.scan(root, {"language": 271})
            self.assertEqual(stamps, 1)
            self.assertTrue(any("STAMP_DRIFT README.md:1 language" in f for f in failures))

    def test_named_old_pin_cannot_exempt_a_current_language_claim(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "README.md").write_text(
                "Language core 259/259 is current on anubis-abcdef123456.\n",
                encoding="utf-8",
            )
            failures, stamps, _ = SCAN.scan(root, {"language": 271})
            self.assertEqual(stamps, 1)
            self.assertTrue(any("STAMP_DRIFT README.md:1 language" in f for f in failures))

    def test_dated_language_measurement_remains_exempt(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "README.md").write_text(
                "Historical W1 language corpus 259/259 PASS.\n", encoding="utf-8"
            )
            failures, stamps, _ = SCAN.scan(root, {"language": 271})
            self.assertEqual((failures, stamps), ([], 0))

    def test_named_current_language_inventory_cannot_disappear(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            docs = root / "docs"
            docs.mkdir()
            claims = docs / "CLAIMS.md"
            claims.write_text(
                "| **Language core** | Current fixture inventory: **271** files |\n",
                encoding="utf-8",
            )
            failures, stamps, _ = SCAN.scan(root, {"language": 271})
            self.assertEqual((failures, stamps), ([], 1))
            claims.write_text(
                "| **Language core** | Current fixture inventory: **259** files |\n",
                encoding="utf-8",
            )
            failures, _, _ = SCAN.scan(root, {"language": 271})
            self.assertTrue(any("language-core-inventory claimed 259" in f for f in failures))
            claims.write_text("No current inventory row.\n", encoding="utf-8")
            failures, _, _ = SCAN.scan(root, {"language": 271})
            self.assertTrue(any("LIVE_CLAIM_ANCHOR" in f for f in failures))
            claims.write_text(
                "```md\n| **Language core** | Current fixture inventory: **271** files |\n```\n",
                encoding="utf-8",
            )
            failures, _, _ = SCAN.scan(root, {"language": 271})
            self.assertTrue(any("LIVE_CLAIM_ANCHOR" in f for f in failures))

    def test_strict_scan_rejects_symlinked_owned_doc(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            target = root / "real.md"
            target.write_text("content\n", encoding="utf-8")
            owned = root / "AGENTS.md"
            owned.symlink_to(target)
            failures, _, _ = SCAN.scan(root, {}, require_owned_files=True)
            self.assertIn("SYMLINK_OWNED_DOC AGENTS.md", failures)

    def test_strict_scan_rejects_non_utf8_owned_doc(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "AGENTS.md").write_bytes(b"\xff\xfe")
            failures, _, _ = SCAN.scan(root, {}, require_owned_files=True)
            self.assertTrue(
                any(item.startswith("UNREADABLE_OWNED_DOC AGENTS.md:") for item in failures),
                failures,
            )

    def test_strict_scan_rejects_symlinked_parent_of_owned_doc(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            external = base / "external"
            (external / "docs").mkdir(parents=True)
            (external / "docs" / "CLAIMS.md").write_text("content\n", encoding="utf-8")
            root = base / "root"
            root.mkdir()
            (root / "docs").symlink_to(external / "docs", target_is_directory=True)
            failures, _, _ = SCAN.scan(root, {}, require_owned_files=True)
            self.assertIn("SYMLINK_OWNED_DOC docs/CLAIMS.md", failures)


if __name__ == "__main__":
    unittest.main()
