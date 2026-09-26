"""Pure, Safe checks for the public hosted-failure-ID minimizer."""

from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import emit_hosted_failure_ids as emitter  # noqa: E402


def fixture_report(*, failed: bool = True) -> dict[str, object]:
    bad_status = "FAIL" if failed else "PASS"
    return {
        "overall_verdict": "FAIL",
        "total": 2,
        "passed": 1 if failed else 2,
        "failed": 1 if failed else 0,
        "fixtures": [
            {"name": "good", "expected": "PASS", "actual": "PASS", "status": "PASS"},
            {"name": "bad", "expected": "PASS", "actual": bad_status, "status": bad_status},
        ],
    }


def gate_report(*, g3: str = "PASS", g5: str = "FAIL") -> dict[str, object]:
    return {
        "profile": "hosted",
        "verdict": "FAIL",
        "gates": [
            {"gate": "G3_test", "status": g3, "detail": "/private/secret"},
            {"gate": "G5_language_fixtures", "status": g5, "detail": "/private/secret"},
        ],
    }


class HostedFailureIdsTests(unittest.TestCase):
    def test_exact_tracked_fixture_id_only(self) -> None:
        result = emitter.build_diagnostic(
            gate_report(), fixture_report(), {"good", "bad"}, 2
        )
        self.assertEqual(result["g3_test"], {"status": "not_failed"})
        self.assertEqual(
            result["g5_language_fixtures"],
            {
                "status": "identified",
                "fixture_ids": ["bad"],
                "truncated": False,
                "corpus_floor": False,
            },
        )
        self.assertNotIn("/private/secret", json.dumps(result))

    def test_g3_has_no_unreviewed_log_parser(self) -> None:
        result = emitter.build_diagnostic(
            gate_report(g3="FAIL", g5="PASS"), None, None, None
        )
        self.assertEqual(
            result["g3_test"],
            {"status": "unavailable", "reason": "no_reviewed_public_test_id_allowlist"},
        )
        self.assertEqual(result["g5_language_fixtures"], {"status": "not_failed"})

    def test_untracked_or_private_fixture_name_withheld(self) -> None:
        report = fixture_report()
        report["fixtures"][1]["name"] = "/home/private/token"
        result = emitter.summarize_g5(report, {"good", "bad"}, 2)
        self.assertEqual(result, emitter.unavailable("invalid_fixture_report"))
        self.assertNotIn("token", json.dumps(result))

    def test_missing_row_is_incomplete_not_inferred(self) -> None:
        report = fixture_report()
        report["fixtures"].pop()
        self.assertEqual(
            emitter.summarize_g5(report, {"good", "bad"}, 2),
            emitter.unavailable("incomplete_fixture_report"),
        )

    def test_floor_failure_without_fixture_mismatch_is_distinct(self) -> None:
        report = fixture_report(failed=False)
        self.assertEqual(
            emitter.summarize_g5(report, {"good", "bad"}, 3),
            emitter.unavailable("corpus_floor"),
        )

    def test_missing_fixture_report_is_distinct(self) -> None:
        result = emitter.build_diagnostic(
            gate_report(), None, {"good", "bad"}, 2, fixture_error="missing"
        )
        self.assertEqual(
            result["g5_language_fixtures"], emitter.unavailable("missing_fixture_report")
        )

    def test_malformed_gate_report_does_not_publish_fixture_id(self) -> None:
        report = gate_report()
        report["gates"].append({"gate": "G5_language_fixtures", "status": "FAIL"})
        result = emitter.build_diagnostic(report, fixture_report(), {"good", "bad"}, 2)
        self.assertEqual(
            result["g5_language_fixtures"], emitter.unavailable("invalid_gate_report")
        )

    def test_hosted_pass_cannot_contain_failed_target_gate(self) -> None:
        report = gate_report()
        report["verdict"] = "HOSTED_PASS"
        result = emitter.build_diagnostic(report, fixture_report(), {"good", "bad"}, 2)
        self.assertEqual(
            result["g5_language_fixtures"], emitter.unavailable("invalid_gate_report")
        )

    def test_mislabeled_pass_row_cannot_hide_failed_fixture(self) -> None:
        report = fixture_report()
        report["fixtures"][0]["status"] = "FAIL"
        report["fixtures"][1]["status"] = "PASS"
        self.assertEqual(
            emitter.summarize_g5(report, {"good", "bad"}, 2),
            emitter.unavailable("invalid_fixture_report"),
        )

    def test_symlink_and_oversize_inputs_are_not_read(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            secret = root / "secret.json"
            secret.write_text('{"token":"private"}', encoding="utf-8")
            link = root / "gate.json"
            link.symlink_to(secret)
            self.assertEqual(emitter.read_json(link), (None, "invalid"))
            self.assertEqual(emitter.read_bounded(secret, 1), (None, "invalid"))

    def test_duplicate_json_keys_are_invalid(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            report = Path(directory) / "gate.json"
            report.write_text('{"gate":{"status":"PASS","status":"FAIL"}}')
            self.assertEqual(emitter.read_json(report), (None, "invalid"))

    @unittest.skipUnless(hasattr(os, "mkfifo"), "requires POSIX FIFO support")
    def test_fifo_report_returns_without_blocking(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            fifo = Path(directory) / "gate.json"
            os.mkfifo(fifo)
            child = (
                "from pathlib import Path; import sys; "
                "sys.path.insert(0, sys.argv[1]); "
                "import emit_hosted_failure_ids as emitter; "
                "assert emitter.read_bounded(Path(sys.argv[2]), 64) == (None, 'invalid')"
            )
            subprocess.run(
                [sys.executable, "-c", child, str(Path(__file__).resolve().parents[1]), str(fifo)],
                check=True,
                timeout=3,
                capture_output=True,
                text=True,
            )

    def test_output_is_bounded_and_sorted(self) -> None:
        names = {f"case_{index}" for index in range(emitter.MAX_PUBLIC_IDS + 1)}
        report = {
            "overall_verdict": "FAIL",
            "total": len(names),
            "passed": 0,
            "failed": len(names),
            "fixtures": [
                {"name": name, "expected": "PASS", "actual": "FAIL", "status": "FAIL"}
                for name in reversed(sorted(names))
            ],
        }
        result = emitter.summarize_g5(report, names, 0)
        self.assertEqual(result["status"], "partial")
        self.assertEqual(result["fixture_ids"], sorted(names)[: emitter.MAX_PUBLIC_IDS])
        self.assertTrue(result["truncated"])


if __name__ == "__main__":
    unittest.main()
