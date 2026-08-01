#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import importlib.util
import io
import json
import subprocess
import tempfile
import unittest
from contextlib import redirect_stderr
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
TOOL = ROOT / "scripts/lib/gate_run_ledger_promote.py"
SPEC = importlib.util.spec_from_file_location("gate_run_ledger_promote", TOOL)
assert SPEC is not None and SPEC.loader is not None
PROMOTER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PROMOTER)
COMMIT = "a" * 40
LEDGER = f"formal {COMMIT} 1\nnative_authoritative {COMMIT} 2\n".encode("ascii")


class GateRunLedgerPromoteTests(unittest.TestCase):
    def run_tool(
        self,
        root: Path,
        *,
        expected_sha: str | None = None,
        expected_commit: str = COMMIT,
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                "python3",
                str(TOOL),
                "--source",
                str(root / "gate_run_ledger.working"),
                "--destination",
                str(root / "gate_run_ledger.validated"),
                "--expected-sha256",
                expected_sha or hashlib.sha256(LEDGER).hexdigest(),
                "--expected-commit",
                expected_commit,
            ],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )

    def write_source(self, root: Path) -> Path:
        source = root / "gate_run_ledger.working"
        source.write_bytes(LEDGER)
        source.chmod(0o444)
        return source

    def test_promotes_private_snapshot_as_regular_nonwritable_file(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = self.write_source(root)
            result = self.run_tool(root)
            self.assertEqual(result.returncode, 0, result.stderr)
            receipt = json.loads(result.stdout)
            destination = root / "gate_run_ledger.validated"
            self.assertFalse(source.exists())
            self.assertTrue(destination.is_file())
            self.assertFalse(destination.is_symlink())
            self.assertEqual(destination.read_bytes(), LEDGER)
            self.assertEqual(destination.stat().st_mode & 0o222, 0)
            self.assertEqual(receipt["sha256"], hashlib.sha256(LEDGER).hexdigest())

    def test_source_symlink_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            target = root / "outside"
            target.write_bytes(LEDGER)
            target.chmod(0o444)
            (root / "gate_run_ledger.working").symlink_to(target)
            result = self.run_tool(root)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("regular non-symlink", result.stderr)
            self.assertFalse((root / "gate_run_ledger.validated").exists())

    def test_nonregular_source_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "gate_run_ledger.working").mkdir()
            result = self.run_tool(root)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("regular non-symlink", result.stderr)
            self.assertFalse((root / "gate_run_ledger.validated").exists())

    def test_existing_destination_is_never_replaced(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.write_source(root)
            destination = root / "gate_run_ledger.validated"
            destination.write_text("sentinel\n")
            result = self.run_tool(root)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(destination.read_text(), "sentinel\n")

    def test_destination_created_after_precheck_is_not_replaced(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = self.write_source(root)
            destination = root / "gate_run_ledger.validated"
            outside = root / "outside"
            outside.write_text("sentinel\n")
            real_link = PROMOTER.os.link

            def plant_destination_then_link(
                temporary: Path, published: Path, *, follow_symlinks: bool
            ) -> None:
                destination.symlink_to(outside)
                real_link(temporary, published, follow_symlinks=follow_symlinks)

            with mock.patch.object(PROMOTER.os, "link", side_effect=plant_destination_then_link):
                with redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
                    PROMOTER.promote(
                        source,
                        destination,
                        hashlib.sha256(LEDGER).hexdigest(),
                        COMMIT,
                    )

            self.assertTrue(destination.is_symlink())
            self.assertEqual(outside.read_text(), "sentinel\n")

    def test_digest_and_commit_mismatch_are_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.write_source(root)
            digest_result = self.run_tool(root, expected_sha="0" * 64)
            self.assertNotEqual(digest_result.returncode, 0)
            self.assertFalse((root / "gate_run_ledger.validated").exists())
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.write_source(root)
            commit_result = self.run_tool(root, expected_commit="b" * 40)
            self.assertNotEqual(commit_result.returncode, 0)
            self.assertFalse((root / "gate_run_ledger.validated").exists())

    def test_source_swapped_to_symlink_at_publish_cannot_control_destination(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = self.write_source(root)
            destination = root / "gate_run_ledger.validated"
            outside = root / "attacker-controlled"
            attacker_bytes = b"attacker\n"
            outside.write_bytes(attacker_bytes)
            outside.chmod(0o444)
            real_link = PROMOTER.os.link

            def swap_source_then_link(
                temporary: Path, published: Path, *, follow_symlinks: bool
            ) -> None:
                source.unlink()
                source.symlink_to(outside)
                real_link(temporary, published, follow_symlinks=follow_symlinks)

            with mock.patch.object(PROMOTER.os, "link", side_effect=swap_source_then_link):
                receipt = PROMOTER.promote(
                    source,
                    destination,
                    hashlib.sha256(LEDGER).hexdigest(),
                    COMMIT,
                )

            self.assertEqual(receipt["sha256"], hashlib.sha256(LEDGER).hexdigest())
            self.assertTrue(destination.is_file())
            self.assertFalse(destination.is_symlink())
            self.assertEqual(destination.read_bytes(), LEDGER)
            self.assertEqual(outside.read_bytes(), attacker_bytes)
            self.assertFalse(source.exists())

    def test_private_snapshot_mutation_at_publish_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = self.write_source(root)
            destination = root / "gate_run_ledger.validated"
            real_link = PROMOTER.os.link

            def mutate_snapshot_then_link(
                temporary: Path, published: Path, *, follow_symlinks: bool
            ) -> None:
                temporary.chmod(0o644)
                temporary.write_bytes(b"x" * len(LEDGER))
                temporary.chmod(0o444)
                real_link(temporary, published, follow_symlinks=follow_symlinks)

            with mock.patch.object(PROMOTER.os, "link", side_effect=mutate_snapshot_then_link):
                with redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
                    PROMOTER.promote(
                        source,
                        destination,
                        hashlib.sha256(LEDGER).hexdigest(),
                        COMMIT,
                    )

            self.assertTrue(destination.exists())
            self.assertFalse(destination.is_symlink())


if __name__ == "__main__":
    unittest.main(verbosity=2)
