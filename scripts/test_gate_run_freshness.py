#!/usr/bin/env python3
from __future__ import annotations

import os
import shlex
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/gate_run_freshness.sh"
VALIDATOR = ROOT / "scripts/lib/seal_verdict_validate.py"


class GateRunFreshnessTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name) / "repo"
        (self.root / "scripts/lib").mkdir(parents=True)
        shutil.copy2(SCRIPT, self.root / "scripts/gate_run_freshness.sh")
        shutil.copy2(VALIDATOR, self.root / "scripts/lib/seal_verdict_validate.py")
        subprocess.run(["git", "init", "-q"], cwd=self.root, check=True)
        subprocess.run(
            ["git", "config", "user.email", "gate-freshness@example.invalid"],
            cwd=self.root,
            check=True,
        )
        subprocess.run(
            ["git", "config", "user.name", "gate-freshness-test"],
            cwd=self.root,
            check=True,
        )
        subprocess.run(["git", "add", "."], cwd=self.root, check=True)
        subprocess.run(["git", "commit", "-qm", "baseline"], cwd=self.root, check=True)
        self.initial_sha = self.git("rev-parse", "HEAD").strip()
        self.out = Path(self.temp.name) / "seal-output"
        self.out.mkdir()
        self.ledger = self.out / "gate_run_ledger.working"
        roster = subprocess.run(
            [
                "python3",
                "scripts/lib/seal_verdict_validate.py",
                "--profile",
                "core",
                "--print-roster",
            ],
            cwd=self.root,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=True,
        ).stdout.splitlines()
        self.roster = [gate for gate in roster if gate != "gate_run_freshness"]
        self.assertEqual(len(self.roster), 19)

    def tearDown(self) -> None:
        self.temp.cleanup()

    def git(self, *args: str) -> str:
        return subprocess.run(
            ["git", *args],
            cwd=self.root,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=True,
        ).stdout

    def run_gate(
        self,
        *args: str,
        configured: bool = True,
        environment_update: dict[str, str] | None = None,
    ) -> subprocess.CompletedProcess[str]:
        environment = os.environ.copy()
        if configured:
            environment.update(
                {
                    "ANUBIS_GATE_RUN_LEDGER": str(self.ledger),
                    "ANUBIS_GATE_RUN_PROFILE": "core",
                    "ANUBIS_SEAL_OUT": str(self.out),
                }
            )
        else:
            for name in (
                "ANUBIS_GATE_RUN_LEDGER",
                "ANUBIS_GATE_RUN_PROFILE",
                "ANUBIS_SEAL_OUT",
            ):
                environment.pop(name, None)
        if environment_update:
            environment.update(environment_update)
        return subprocess.run(
            ["bash", "scripts/gate_run_freshness.sh", *args],
            cwd=self.root,
            env=environment,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )

    def write_ledger(self, gates: list[str], sha: str | None = None) -> None:
        epoch = sha or self.git("rev-parse", "HEAD").strip()
        if self.ledger.exists():
            self.ledger.chmod(0o644)
        self.ledger.write_text("".join(f"{gate} {epoch} 1\n" for gate in gates))
        self.ledger.chmod(0o444)

    def test_missing_ledger_fails_closed(self) -> None:
        result = self.run_gate()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("ledger missing", result.stderr)

    def test_unconfigured_invocation_fails_closed(self) -> None:
        result = self.run_gate(configured=False)
        self.assertEqual(result.returncode, 2)
        self.assertIn("unconfigured", result.stderr)

    def test_exact_nineteen_row_core_roster_passes(self) -> None:
        self.write_ledger(self.roster)
        result = self.run_gate()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("19/19", result.stdout)

    def test_formal_and_native_authoritative_are_load_bearing(self) -> None:
        for missing in ("formal", "native_authoritative"):
            with self.subTest(missing=missing):
                self.write_ledger([gate for gate in self.roster if gate != missing])
                result = self.run_gate()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(missing, result.stdout + result.stderr)

    def test_duplicate_and_extra_rows_fail(self) -> None:
        for rows, needle in (
            (self.roster + [self.roster[0]], "duplicate"),
            (self.roster + ["surprise"], "unexpected"),
        ):
            with self.subTest(needle=needle):
                self.write_ledger(rows)
                result = self.run_gate()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(needle, result.stdout + result.stderr)

    def test_mixed_commit_epochs_fail(self) -> None:
        (self.root / "epoch").write_text("next\n")
        subprocess.run(["git", "add", "epoch"], cwd=self.root, check=True)
        subprocess.run(["git", "commit", "-qm", "next"], cwd=self.root, check=True)
        current = self.git("rev-parse", "HEAD").strip()
        rows = [f"{gate} {current} 1\n" for gate in self.roster]
        rows[0] = f"{self.roster[0]} {self.initial_sha} 1\n"
        self.ledger.write_text("".join(rows))
        result = self.run_gate()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("mixed commit epochs", result.stdout + result.stderr)

    def test_any_head_drift_fails(self) -> None:
        self.write_ledger(self.roster, self.initial_sha)
        (self.root / "epoch").write_text("next\n")
        subprocess.run(["git", "add", "epoch"], cwd=self.root, check=True)
        subprocess.run(["git", "commit", "-qm", "next"], cwd=self.root, check=True)
        result = self.run_gate()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("does not equal current HEAD", result.stdout)

    def test_path_git_shim_cannot_commit_then_spoof_old_head(self) -> None:
        self.write_ledger(self.roster)
        fake_bin = Path(self.temp.name) / "fake-bin"
        fake_bin.mkdir()
        marker = Path(self.temp.name) / "fake-git-invoked"
        fake_git = fake_bin / "git"
        trusted_git = shutil.which("git", path="/usr/bin:/bin:/usr/sbin:/sbin")
        self.assertIsNotNone(trusted_git)
        fake_git.write_text(
            "#!/usr/bin/env bash\n"
            f"printf invoked > {shlex.quote(str(marker))}\n"
            "if [[ \"${1:-}\" == \"rev-parse\" && \"${2:-}\" == \"HEAD\" ]]; then\n"
            "  printf poison > shim-race\n"
            f"  {shlex.quote(str(trusted_git))} add shim-race\n"
            f"  {shlex.quote(str(trusted_git))} commit -qm shim-race\n"
            f"  printf '%s\\n' {shlex.quote(self.initial_sha)}\n"
            "  exit 0\n"
            "fi\n"
            f"exec {shlex.quote(str(trusted_git))} \"$@\"\n"
        )
        fake_git.chmod(0o755)
        result = self.run_gate(
            environment_update={
                "PATH": f"{fake_bin}:{os.environ.get('PATH', '')}",
                "GIT_DIR": str(Path(self.temp.name) / "attacker-git-dir"),
            }
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertFalse(marker.exists(), "PATH-controlled Git shim was executed")
        self.assertEqual(self.git("rev-parse", "HEAD").strip(), self.initial_sha)

    def test_stamp_changes_only_explicit_output_ledger(self) -> None:
        before = self.git("status", "--porcelain=v1", "--untracked-files=all")
        result = self.run_gate("--stamp", self.roster[0])
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        after = self.git("status", "--porcelain=v1", "--untracked-files=all")
        self.assertEqual(before, after)
        self.assertTrue(self.ledger.is_file())
        self.assertFalse((self.root / "docs/.gate_run_ledger").exists())


if __name__ == "__main__":
    unittest.main(verbosity=2)
