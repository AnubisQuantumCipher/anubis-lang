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
