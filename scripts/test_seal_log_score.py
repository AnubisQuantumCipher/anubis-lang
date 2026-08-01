#!/usr/bin/env python3
from __future__ import annotations

import json
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TOOL = ROOT / "scripts/lib/seal_log_score.py"


class SealLogScoreTests(unittest.TestCase):
    def score(self, text: str) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as tmp:
            log = Path(tmp) / "gate.log"
            out = Path(tmp) / "score.json"
            log.write_text(text)
            result = subprocess.run(
                [
                    "python3", str(TOOL),
                    "--log", str(log),
                    "--pass-re", r"^GATE: PASS$",
                    "--fail-re", r"^GATE: FAIL$",
                    "--out", str(out),
                ],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            if out.exists():
                result.score = json.loads(out.read_text())  # type: ignore[attr-defined]
            return result

    def classify_via_seal_function(
        self, text: str
    ) -> tuple[subprocess.CompletedProcess[str], list[str]]:
        source = (ROOT / "scripts/run_seal_checklist.sh").read_text()
        start = source.index("classify_verdict() {")
        end = source.index("\n_instrument_guards()", start)
        function = source[start:end]
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            log = tmp_path / "gate.log"
            score = tmp_path / "score.json"
            driver = tmp_path / "driver.sh"
            log.write_text(text)
            driver.write_text(
                "#!/bin/bash\n"
                "set -eo pipefail\n"
                'ROOT="$1"\n'
                'log="$2"\n'
                'score="$3"\n'
                + function
                + "\n"
                + "classify_verdict \"$log\" '^GATE: PASS$' '^GATE: FAIL$' \"$score\"\n"
                + "printf '%s\\n' \"$_v_status\" \"$_v_line\" \"$_v_reason\" \"${_v_marker_count:-unset}\"\n"
            )
            result = subprocess.run(
                ["/bin/bash", str(driver), str(ROOT), str(log), str(score)],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            return result, result.stdout.splitlines()

    def test_one_pass_marker_scores_pass(self) -> None:
        result = self.score("noise exp=FAIL\nGATE: PASS\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.score["status"], "PASS")  # type: ignore[attr-defined]

    def test_no_marker_scores_fail(self) -> None:
        result = self.score("noise exp=FAIL\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.score["status"], "FAIL")  # type: ignore[attr-defined]
        self.assertEqual(result.score["reason"], "no_declared_verdict_line")  # type: ignore[attr-defined]

    def test_duplicate_pass_rejected_as_malformed(self) -> None:
        result = self.score("GATE: PASS\nGATE: PASS\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.score["status"], "FAIL")  # type: ignore[attr-defined]
        self.assertIn("duplicate", result.score["reason"])  # type: ignore[attr-defined]

    def test_contradictory_pass_fail_rejected_as_malformed(self) -> None:
        result = self.score("GATE: PASS\nGATE: FAIL\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.score["status"], "FAIL")  # type: ignore[attr-defined]
        self.assertIn("contradictory", result.score["reason"])  # type: ignore[attr-defined]

    def test_run_seal_uses_structured_log_scorer(self) -> None:
        source = (ROOT / "scripts/run_seal_checklist.sh").read_text()
        self.assertIn("scripts/lib/seal_log_score.py", source)
        self.assertIn("declared_marker_count", source)

    def test_run_seal_avoids_bash4_only_mapfile(self) -> None:
        source = (ROOT / "scripts/run_seal_checklist.sh").read_text()
        self.assertNotRegex(source, r"(?m)^\s*mapfile\b")
        self.assertIn("IFS= read -r _score_status", source)

    def test_run_seal_keeps_declared_fail_reason_policy_comparable(self) -> None:
        result, fields = self.classify_via_seal_function("GATE: FAIL\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(fields, ["FAIL", "GATE: FAIL", "declared_FAIL_line", "1"])

    def test_run_seal_keeps_no_marker_reason_policy_comparable(self) -> None:
        result, fields = self.classify_via_seal_function("noise exp=FAIL\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(fields, ["FAIL", "", "no_declared_verdict_line", "0"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
