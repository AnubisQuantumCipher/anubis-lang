#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TOOL = ROOT / "scripts/lib/bundle_manifest.py"


class BundleManifestTests(unittest.TestCase):
    def run_tool(
        self, bundle: Path, command: str = "rehash"
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["python3", str(TOOL), command, "--bundle", str(bundle)],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )

    def test_rehash_is_complete_sorted_and_self_excluding(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            bundle = Path(tmp) / "bundle"
            (bundle / "nested").mkdir(parents=True)
            (bundle / "z.txt").write_text("z\n")
            (bundle / "nested/a.txt").write_text("a\n")
            (bundle / "MANIFEST.sha256").write_text("stale\n")
            result = self.run_tool(bundle)
            self.assertEqual(result.returncode, 0, result.stderr)
            lines = (bundle / "MANIFEST.sha256").read_text().splitlines()
            self.assertEqual([line.split("  ", 1)[1] for line in lines], ["nested/a.txt", "z.txt"])
            for line in lines:
                digest, rel = line.split("  ", 1)
                self.assertEqual(digest, hashlib.sha256((bundle / rel).read_bytes()).hexdigest())
            self.assertIn("BUNDLE_MANIFEST_REHASH_PASS", result.stdout)
            verified = self.run_tool(bundle, "verify")
            self.assertEqual(verified.returncode, 0, verified.stderr)
            self.assertIn("BUNDLE_MANIFEST_VERIFY_PASS", verified.stdout)

    def test_verify_rejects_digest_or_roster_drift(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            bundle = Path(tmp) / "bundle"
            bundle.mkdir()
            member = bundle / "member.txt"
            member.write_text("first\n")
            self.assertEqual(self.run_tool(bundle).returncode, 0)

            member.write_text("second\n")
            changed = self.run_tool(bundle, "verify")
            self.assertNotEqual(changed.returncode, 0)
            self.assertIn("do not exactly match", changed.stderr)

            member.write_text("first\n")
            (bundle / "extra.txt").write_text("extra\n")
            extra = self.run_tool(bundle, "verify")
            self.assertNotEqual(extra.returncode, 0)
            self.assertIn("do not exactly match", extra.stderr)

    def test_symlink_member_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            bundle = Path(tmp) / "bundle"
            bundle.mkdir()
            outside = Path(tmp) / "outside"
            outside.write_text("outside\n")
            (bundle / "link.txt").symlink_to(outside)
            result = self.run_tool(bundle)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("BUNDLE_MANIFEST_ERROR", result.stderr)
            self.assertFalse((bundle / "MANIFEST.sha256").exists())

    def test_non_directory_bundle_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            bundle = Path(tmp) / "bundle"
            bundle.write_text("not a directory\n")
            result = self.run_tool(bundle)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("BUNDLE_MANIFEST_ERROR", result.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
