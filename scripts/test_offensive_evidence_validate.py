#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import importlib.util
import json
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
TOOL = ROOT / "scripts/lib/offensive_evidence_validate.py"
TOOL_SPEC = importlib.util.spec_from_file_location("offensive_evidence_validate", TOOL)
assert TOOL_SPEC is not None and TOOL_SPEC.loader is not None
TOOL_MODULE = importlib.util.module_from_spec(TOOL_SPEC)
TOOL_SPEC.loader.exec_module(TOOL_MODULE)
BIN_SHA = "b" * 64
EXPECTED_CASES = [
    "t1_engage_certs", "t1_encrypt_default", "t1_agent_encrypt", "t1_encrypted_c2",
    "t7_console", "t2_launchagent", "t2_inject_plan", "t2_inject_live_double_auth",
    "t3_uds", "t3_dns", "t3_dns_doh_codec", "t7_operator_token_auth",
    "t1_mtls_rustls", "t4_lateral_deny", "t4_lateral_smb_plan", "t7_rbac_queue",
    "t5_pattern", "t5_offset", "t5_browser", "t6_packer", "t6_string_scramble",
    "exploit_run", "doctor_t17", "scope_targets", "t9_attck_catalog", "t9_opsec_score",
    "t9_malleable", "t9_campaign", "t9_phish_plan", "t9_lolbas", "t9_purple_report",
    "t9_recon_hostinfo", "t9_recon_scan", "t9_doctor_surfaces",
]


class OffensiveEvidenceValidateTests(unittest.TestCase):
    def make_evidence(self, root: Path) -> None:
        report = {
            "total": 34,
            "passed": 34,
            "failed": 0,
            "overall_verdict": "PASS",
            "cases": [{"name": name, "status": "PASS", "detail": "ok"} for name in EXPECTED_CASES],
            "binary_sha256": BIN_SHA,
            "isolation": "tart-disposable-guest",
            "mode": "tart-disposable-guest",
            "expected_total": 34,
            "teardown_status": "torn_down",
        }
        report_bytes = (json.dumps(report, indent=2, sort_keys=True) + "\n").encode()
        (root / "report.json").write_bytes(report_bytes)
        isolation = {
            "isolation": "tart-disposable-guest",
            "mode": "tart-disposable-guest",
            "guest": "anubis-offensive-gate-123",
            "cpu": 8,
            "memory_mib": 5120,
            "cargo_build_jobs": 3,
            "rayon_threads": 3,
            "binary_sha256": BIN_SHA,
            "teardown_status": "torn_down",
        }
        (root / "isolation.json").write_text(json.dumps(isolation) + "\n")
        (root / "guest_stdout.log").write_text(
            "Overall: PASS (34/34) isolation=tart-disposable-guest expected=34\n"
        )
        (root / "teardown_status.txt").write_text("torn_down\n")
        export = {
            "schema": "anubis-offensive-gate-export-v1",
            "secret_scan": "PASS",
            "files": [{
                "path": "report.json",
                "size_bytes": len(report_bytes),
                "sha256": hashlib.sha256(report_bytes).hexdigest(),
            }],
        }
        (root / "export_manifest.json").write_text(json.dumps(export) + "\n")

    def validate(self, mutate=None) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "evidence"
            root.mkdir()
            self.make_evidence(root)
            if mutate:
                mutate(root)
            result = subprocess.run(
                [
                    "python3", str(TOOL), "--evidence", str(root),
                    "--out", str(root / "offensive_verdict.json"),
                    "--expected-binary-sha256", BIN_SHA,
                    "--expected-memory-mib", "5120", "--expected-jobs", "3",
                ],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            verdict = root / "offensive_verdict.json"
            if verdict.exists():
                result.verdict = json.loads(verdict.read_text())  # type: ignore[attr-defined]
            return result

    def test_complete_receipt_passes(self) -> None:
        result = self.validate()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.verdict["verdict"], "PASS")  # type: ignore[attr-defined]

    def test_stale_pass_removed_when_evidence_directory_is_missing(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            verdict = root / "offensive_verdict.json"
            verdict.write_text('{"verdict":"PASS"}\n')
            result = subprocess.run(
                [
                    "python3", str(TOOL), "--evidence", str(root / "missing"),
                    "--out", str(verdict), "--expected-binary-sha256", BIN_SHA,
                    "--expected-memory-mib", "5120", "--expected-jobs", "3",
                ],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
            self.assertIn("cannot stat evidence directory", result.stderr)
            self.assertFalse(verdict.exists(), "early evidence failure left a stale PASS verdict")

    def test_stale_pass_removed_when_typed_argument_is_invalid(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            evidence = root / "evidence"
            evidence.mkdir()
            self.make_evidence(evidence)
            verdict = root / "offensive_verdict.json"
            verdict.write_text('{"verdict":"PASS"}\n')
            result = subprocess.run(
                [
                    "python3", str(TOOL), "--evidence", str(evidence),
                    "--out", str(verdict), "--expected-binary-sha256", BIN_SHA,
                    "--expected-memory-mib", "5120", "--expected-jobs", "not-an-int",
                ],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
            self.assertIn("invalid int value", result.stderr)
            self.assertFalse(verdict.exists(), "early argument failure left a stale PASS verdict")

    def test_all_prior_outputs_removed_before_dangling_repeated_out_fails(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            first = Path(tmp) / "first.json"
            first.write_text('{"verdict":"PASS"}\n')
            result = subprocess.run(
                ["python3", str(TOOL), "--out", str(first), "--out"],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
            self.assertIn("argument --out: expected one argument", result.stderr)
            self.assertFalse(first.exists(), "dangling repeated --out left the earlier PASS verdict")

    def test_every_repeated_separate_output_removed_before_type_failure(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            evidence = root / "evidence"
            evidence.mkdir()
            self.make_evidence(evidence)
            first = root / "first.json"
            second = root / "second.json"
            first.write_text('{"verdict":"PASS"}\n')
            second.write_text('{"verdict":"PASS"}\n')
            result = subprocess.run(
                [
                    "python3", str(TOOL), "--evidence", str(evidence),
                    "--out", str(first), "--out", str(second),
                    "--expected-binary-sha256", BIN_SHA,
                    "--expected-memory-mib", "5120", "--expected-jobs", "bad",
                ],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
            self.assertIn("invalid int value", result.stderr)
            self.assertFalse(first.exists(), "first repeated --out retained a stale PASS")
            self.assertFalse(second.exists(), "second repeated --out retained a stale PASS")

    def test_every_repeated_equals_output_removed_without_touching_other_values(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            evidence = root / "evidence.json"
            first = root / "first.json"
            second = root / "second.json"
            evidence.write_text('{"verdict":"PASS"}\n')
            first.write_text('{"verdict":"PASS"}\n')
            second.write_text('{"verdict":"PASS"}\n')
            result = subprocess.run(
                [
                    "python3", str(TOOL), "--evidence", str(evidence),
                    f"--out={first}", f"--out={second}",
                    "--expected-binary-sha256", BIN_SHA,
                    "--expected-memory-mib", "5120", "--expected-jobs", "bad",
                ],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
            self.assertIn("invalid int value", result.stderr)
            self.assertFalse(first.exists(), "first --out= target retained a stale PASS")
            self.assertFalse(second.exists(), "second --out= target retained a stale PASS")
            self.assertTrue(evidence.exists(), "an unrelated option value was treated as --out")

    def test_directory_refusal_does_not_shield_later_stale_pass(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            blocker = root / "blocker"
            stale = root / "stale.json"
            blocker.mkdir()
            stale.write_text('{"verdict":"PASS"}\n')
            result = subprocess.run(
                ["python3", str(TOOL), "--out", str(blocker), "--out", str(stale)],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
            self.assertIn("verdict output must not be a directory", result.stderr)
            self.assertTrue(blocker.is_dir(), "directory refusal did not preserve the directory")
            self.assertFalse(stale.exists(), "directory refusal shielded a later stale PASS")

    def test_unlink_failure_does_not_shield_later_stale_pass(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            blocked = root / "blocked.json"
            stale = root / "stale.json"
            blocked.write_text('{"verdict":"PASS"}\n')
            stale.write_text('{"verdict":"PASS"}\n')
            original_unlink = Path.unlink

            def selective_unlink(path: Path, *args, **kwargs) -> None:
                if path == blocked:
                    raise PermissionError("simulated unlink refusal")
                original_unlink(path, *args, **kwargs)

            with mock.patch.object(Path, "unlink", autospec=True, side_effect=selective_unlink):
                with self.assertRaises(SystemExit) as raised:
                    TOOL_MODULE.invalidate_requested_verdicts([blocked, stale])
            self.assertEqual(raised.exception.code, 2)
            self.assertTrue(blocked.exists(), "simulated ununlinkable output was unexpectedly removed")
            self.assertFalse(stale.exists(), "unlink failure shielded a later stale PASS")

    def test_argparse_positional_output_names_are_scrubbed(self) -> None:
        for output_name in ("-1", "-.5", "-0.5", "-1\n", "-foo bar", "--foo bar"):
            with self.subTest(output_name=output_name), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                evidence = root / "evidence"
                evidence.mkdir()
                self.make_evidence(evidence)
                stale = root / output_name
                stale.write_text('{"verdict":"PASS"}\n')
                result = subprocess.run(
                    [
                        "python3", str(TOOL), "--evidence", str(evidence),
                        "--out", output_name, "--expected-binary-sha256", BIN_SHA,
                        "--expected-memory-mib", "5120", "--expected-jobs", "bad",
                    ],
                    text=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    check=False,
                    cwd=root,
                )
                self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
                self.assertIn("invalid int value", result.stderr)
                self.assertFalse(stale.exists(), f"accepted output {output_name!r} retained stale PASS")

    def test_recognized_option_after_out_is_dangling_not_an_output_value(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            earlier = root / "earlier.json"
            option_named_file = root / "--expected-jobs"
            earlier.write_text('{"verdict":"PASS"}\n')
            option_named_file.write_text('{"verdict":"PASS"}\n')
            result = subprocess.run(
                [
                    "python3", str(TOOL), "--out", str(earlier),
                    "--out", "--expected-jobs", "bad",
                ],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
                cwd=root,
            )
            self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
            self.assertIn("argument --out: expected one argument", result.stderr)
            self.assertFalse(earlier.exists(), "dangling --out shielded an earlier PASS")
            self.assertTrue(option_named_file.exists(), "recognized option token was treated as output")

    def test_empty_equals_output_rejected_after_prior_outputs_are_scrubbed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            stale = root / "stale.json"
            stale.write_text('{"verdict":"PASS"}\n')
            result = subprocess.run(
                ["python3", str(TOOL), "--out", str(stale), "--out="],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
                cwd=root,
            )
            self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
            self.assertIn("--out requires a non-empty path", result.stderr)
            self.assertNotIn("Traceback", result.stderr)
            self.assertFalse(stale.exists(), "empty --out= shielded an earlier stale PASS")

    def assert_rejected(self, mutate, needle: str) -> None:
        result = self.validate(mutate)
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn(needle, "\n".join(result.verdict["errors"]))  # type: ignore[attr-defined]

    def test_report_count_mismatch_rejected(self) -> None:
        def mutate(root: Path) -> None:
            data = json.loads((root / "report.json").read_text())
            data["passed"] = 33
            (root / "report.json").write_text(json.dumps(data) + "\n")
        self.assert_rejected(mutate, "report")

    def test_export_hash_mismatch_rejected(self) -> None:
        def mutate(root: Path) -> None:
            data = json.loads((root / "export_manifest.json").read_text())
            data["files"][0]["sha256"] = "0" * 64
            (root / "export_manifest.json").write_text(json.dumps(data) + "\n")
        self.assert_rejected(mutate, "export")

    def test_symlink_required_file_rejected(self) -> None:
        def mutate(root: Path) -> None:
            target = root / "report.real"
            (root / "report.json").rename(target)
            (root / "report.json").symlink_to(target)
        self.assert_rejected(mutate, "symlink")

    def test_wrong_job_cap_rejected(self) -> None:
        def mutate(root: Path) -> None:
            data = json.loads((root / "isolation.json").read_text())
            data["cargo_build_jobs"] = 4
            (root / "isolation.json").write_text(json.dumps(data) + "\n")
        self.assert_rejected(mutate, "jobs")

    def test_missing_or_reordered_case_roster_rejected(self) -> None:
        def mutate(root: Path) -> None:
            data = json.loads((root / "report.json").read_text())
            data["cases"] = data["cases"][1:]
            (root / "report.json").write_text(json.dumps(data) + "\n")
            report_bytes = (root / "report.json").read_bytes()
            export = json.loads((root / "export_manifest.json").read_text())
            export["files"][0]["size_bytes"] = len(report_bytes)
            export["files"][0]["sha256"] = hashlib.sha256(report_bytes).hexdigest()
            (root / "export_manifest.json").write_text(json.dumps(export) + "\n")
        self.assert_rejected(mutate, "case roster")

    def test_duplicate_guest_overall_rejected(self) -> None:
        def mutate(root: Path) -> None:
            line = "Overall: PASS (34/34) isolation=tart-disposable-guest expected=34\n"
            (root / "guest_stdout.log").write_text(line + line)
        self.assert_rejected(mutate, "Overall")

    def test_guest_overall_boundary_fields_rejected(self) -> None:
        def mutate(root: Path) -> None:
            (root / "guest_stdout.log").write_text(
                "Overall: PASS (34/34) isolation=host expected=34\n"
            )
        self.assert_rejected(mutate, "Overall")

        def mutate_expected(root: Path) -> None:
            (root / "guest_stdout.log").write_text(
                "Overall: PASS (34/34) isolation=tart-disposable-guest expected=33\n"
            )
        self.assert_rejected(mutate_expected, "Overall")

        def mutate_count(root: Path) -> None:
            (root / "guest_stdout.log").write_text(
                "Overall: PASS (33/34) isolation=tart-disposable-guest expected=34\n"
            )
        self.assert_rejected(mutate_count, "Overall")

    def rewrite_export_for_report(self, root: Path) -> None:
        report_bytes = (root / "report.json").read_bytes()
        export = json.loads((root / "export_manifest.json").read_text())
        export["files"][0]["size_bytes"] = len(report_bytes)
        export["files"][0]["sha256"] = hashlib.sha256(report_bytes).hexdigest()
        (root / "export_manifest.json").write_text(json.dumps(export) + "\n")

    def test_duplicate_json_keys_rejected(self) -> None:
        def report_dup(root: Path) -> None:
            raw = (root / "report.json").read_text()
            raw = raw.replace('"overall_verdict": "PASS"', '"overall_verdict": "FAIL", "overall_verdict": "PASS"')
            raw = raw.replace('"failed": 0', '"failed": 99, "failed": 0')
            raw = raw.replace('"status": "PASS"', '"status": "FAIL", "status": "PASS"', 1)
            (root / "report.json").write_text(raw)
            self.rewrite_export_for_report(root)
        self.assert_rejected(report_dup, "duplicate JSON key")

        def isolation_dup(root: Path) -> None:
            raw = (root / "isolation.json").read_text().replace(
                '"teardown_status": "torn_down"',
                '"teardown_status": "kept", "teardown_status": "torn_down"',
            )
            (root / "isolation.json").write_text(raw)
        self.assert_rejected(isolation_dup, "duplicate JSON key")

        def export_dup(root: Path) -> None:
            raw = (root / "export_manifest.json").read_text().replace(
                '"sha256":',
                '"sha256": "0", "sha256":',
                1,
            )
            (root / "export_manifest.json").write_text(raw)
        self.assert_rejected(export_dup, "duplicate JSON key")

    def test_gate_scrubs_stale_pass_artifacts_on_prereq_failure(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp) / "home"
            out = Path(tmp) / "out"
            home.mkdir()
            out.mkdir()
            (out / "report.json").write_text('{"overall_verdict":"PASS","passed":34,"total":34,"failed":0}\n')
            (out / "offensive_verdict.json").write_text('{"verdict":"PASS"}\n')
            result = subprocess.run(
                ["bash", str(ROOT / "scripts/run_offensive_platform_gate.sh"), "--out", str(out)],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
                env={"HOME": str(home), "ANUBIS_OFFENSIVE_GATE_BUILD_JOBS": "bad", "PATH": "/usr/bin:/bin:/usr/sbin:/sbin"},
                cwd=str(ROOT),
            )
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertFalse((out / "report.json").exists())
            self.assertFalse((out / "offensive_verdict.json").exists())
            self.assertEqual((out / "teardown_status.txt").read_text(), "prereq_missing\n")

    def test_gate_requires_vulnerable_target_build_success(self) -> None:
        source = (ROOT / "scripts/run_offensive_platform_gate.sh").read_text()
        self.assertNotIn("poc_kit/build_vuln.sh >\"$out/vuln.log\" 2>&1 || true", source)
        self.assertIn("local vuln_build_rc", source)

    def test_gate_invokes_strict_final_validator(self) -> None:
        source = (ROOT / "scripts/run_offensive_platform_gate.sh").read_text()
        self.assertIn("scripts/lib/offensive_evidence_validate.py", source)
        self.assertIn("OFFENSIVE_VALIDATOR_RC", source)


if __name__ == "__main__":
    unittest.main(verbosity=2)
