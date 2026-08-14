#!/usr/bin/env python3
"""Static adoption tests for cheap Phase-0 guards in the unified audit."""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


class Phase0GateWiringTests(unittest.TestCase):
    @staticmethod
    def g27_body() -> str:
        text = (ROOT / "scripts" / "audit_unified.sh").read_text(encoding="utf-8")
        match = re.search(r"# G27\b(?P<body>.*?)# G28\b", text, re.DOTALL)
        if match is None:
            return ""
        return match.group("body")

    @staticmethod
    def g28_body() -> str:
        text = (ROOT / "scripts" / "audit_unified.sh").read_text(encoding="utf-8")
        match = re.search(r"# G28\b(?P<body>.*?)# G29\b", text, re.DOTALL)
        if match is None:
            return ""
        return match.group("body")

    @staticmethod
    def g29_body() -> str:
        text = (ROOT / "scripts" / "audit_unified.sh").read_text(encoding="utf-8")
        match = re.search(r"# G29\b(?P<body>.*?)# G30\b", text, re.DOTALL)
        if match is None:
            return ""
        return match.group("body")

    @staticmethod
    def g30_body() -> str:
        text = (ROOT / "scripts" / "audit_unified.sh").read_text(encoding="utf-8")
        match = re.search(r"# G30\b(?P<body>.*?)# ── Report ──", text, re.DOTALL)
        if match is None:
            return ""
        return match.group("body")

    def test_g24_runs_red_guard_before_live_promise_gate(self) -> None:
        text = (ROOT / "scripts" / "audit_unified.sh").read_text(encoding="utf-8")
        match = re.search(r"# G24\b(?P<body>.*?)# G25\b", text, re.DOTALL)
        self.assertIsNotNone(match, "G24 block is missing")
        assert match is not None
        body = match.group("body")
        self_test = body.find("run_promise_coherence_gate.sh --self-test")
        live = body.find('run_promise_coherence_gate.sh >')
        self.assertGreaterEqual(self_test, 0, "G24 does not invoke the promise self-test")
        self.assertGreater(live, self_test, "G24 live scan must run after its RED guard")
        self.assertIn("g24_promise_selftest.log", body)

    def test_neutered_detector_is_caught_even_when_its_live_scan_passes(self) -> None:
        source = (ROOT / "scripts" / "run_promise_coherence_gate.sh").read_text(
            encoding="utf-8"
        )
        neutered, replacements = re.subn(
            r"UNIVERSAL = re\.compile\(.*?\n\)\n\n# A qualifier",
            lambda _: 'UNIVERSAL = re.compile(r"\\A\\b\\B")\n\n# A qualifier',
            source,
            count=1,
            flags=re.DOTALL,
        )
        self.assertEqual(replacements, 1, "failed to plant a neutered universal detector")

        with tempfile.TemporaryDirectory(prefix="anubis-g24-neutered-") as raw:
            base = Path(raw)
            (base / "scripts" / "lib").mkdir(parents=True)
            (base / "scripts" / "floors").mkdir()
            gate = base / "scripts" / "run_promise_coherence_gate.sh"
            gate.write_text(neutered, encoding="utf-8")
            shutil.copy2(
                ROOT / "scripts" / "lib" / "gate_common.sh",
                base / "scripts" / "lib" / "gate_common.sh",
            )
            (base / "scripts" / "floors" / "promise_coherence.count_floor").write_text(
                "5\n", encoding="utf-8"
            )

            self_test = subprocess.run(
                ["bash", str(gate), "--self-test"],
                cwd=base,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                check=False,
            )
            self.assertEqual(self_test.returncode, 1, self_test.stdout)
            self.assertIn("asserted universal promise was not reported", self_test.stdout)

            fixture = base / "fixture"
            docs = fixture / "docs"
            docs.mkdir(parents=True)
            (docs / "CLAIMS.md").write_text(
                "Green means no KNOWN defects.\n\n"
                "## Known open issues — load-bearing\n\n1. Open.\n",
                encoding="utf-8",
            )
            qualified = (
                "check passing means Anubis found no way for the program to violate its contracts. "
                "This is not a totality claim; see docs/CLAIMS.md.\n"
            )
            for index in range(5):
                (docs / f"promise-{index}.md").write_text(qualified, encoding="utf-8")
            (docs / "banned.md").write_text(
                "`anubis check` PASS means the program cannot violate its contracts.\n",
                encoding="utf-8",
            )
            live = subprocess.run(
                ["bash", str(gate), "--scan-root", str(fixture)],
                cwd=base,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                check=False,
            )
            self.assertEqual(live.returncode, 0, live.stdout)
            self.assertIn("asserted universal promise forms checked: 0", live.stdout)

    def test_g27_registers_the_nonzero_ledger_fault_suite(self) -> None:
        text = (ROOT / "scripts" / "audit_unified.sh").read_text(encoding="utf-8")
        body = self.g27_body()
        self.assertTrue(body, "G27 ledger-fault block is missing")
        self.assertIn("scripts/test_phase_metrics_ledger.sh", body)
        self.assertIn("PHASE_METRICS_LEDGER_TESTS", body)
        self.assertIn('gate "G27_phase_metrics_ledger"', body)
        expected = re.search(r'^EXPECTED_GATES="([^"]+)"', text, re.MULTILINE)
        self.assertIsNotNone(expected, "EXPECTED_GATES is missing")
        assert expected is not None
        self.assertIn("G27_phase_metrics_ledger", expected.group(1).split())

    def test_g27_rejects_zero_work_even_when_the_test_script_exits_zero(self) -> None:
        body = self.g27_body()
        self.assertTrue(body, "G27 ledger-fault block is missing")
        if not body:
            return
        with tempfile.TemporaryDirectory(prefix="anubis-g27-ledger-") as raw:
            base = Path(raw)
            scripts = base / "scripts"
            scripts.mkdir()
            test_script = scripts / "test_phase_metrics_ledger.sh"
            out = base / "out"
            out.mkdir()

            def run_block(script_body: str) -> subprocess.CompletedProcess[str]:
                test_script.write_text(script_body, encoding="utf-8")
                return subprocess.run(
                    [
                        "bash",
                        "-c",
                        'gate() { printf "%s:%s\\n" "$1" "$2"; }\n' + body,
                    ],
                    cwd=base,
                    env={**dict(os.environ), "OUT": str(out)},
                    text=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.STDOUT,
                    check=False,
                )

            zero_work = run_block("#!/usr/bin/env bash\nexit 0\n")
            self.assertIn("G27_phase_metrics_ledger:FAIL", zero_work.stdout)

            real_summary = run_block(
                "#!/usr/bin/env bash\n"
                "printf 'PHASE_METRICS_LEDGER_TESTS: 8 passed, 0 failed\\n'\n"
            )
            self.assertIn("G27_phase_metrics_ledger:PASS", real_summary.stdout)

            contradictory = run_block(
                "#!/usr/bin/env bash\n"
                "printf 'PHASE_METRICS_LEDGER_TESTS: 8 passed, 0 failed\\n'\n"
                "printf 'PHASE_METRICS_LEDGER_TESTS: 8 passed, 1 failed\\n'\n"
            )
            self.assertIn("G27_phase_metrics_ledger:FAIL", contradictory.stdout)

            malformed = run_block(
                "#!/usr/bin/env bash\n"
                "printf 'PHASE_METRICS_LEDGER_TESTS: malformed\\n'\n"
            )
            self.assertIn("G27_phase_metrics_ledger:FAIL", malformed.stdout)

    def test_g28_registers_the_corpus_inventory_poison_suite(self) -> None:
        text = (ROOT / "scripts" / "audit_unified.sh").read_text(encoding="utf-8")
        body = self.g28_body()
        self.assertTrue(body, "G28 corpus-inventory block is missing")
        self.assertIn("scripts/test_corpus_inventory_binding.sh", body)
        self.assertIn("CORPUS_INVENTORY_BINDING", body)
        self.assertIn('gate "G28_corpus_inventory_binding"', body)
        expected = re.search(r'^EXPECTED_GATES="([^"]+)"', text, re.MULTILINE)
        self.assertIsNotNone(expected, "EXPECTED_GATES is missing")
        assert expected is not None
        self.assertIn("G28_corpus_inventory_binding", expected.group(1).split())

    def test_corpus_inventory_poison_is_load_bearing_in_the_seal_checklist(self) -> None:
        text = (ROOT / "scripts" / "run_seal_checklist.sh").read_text(encoding="utf-8")
        match = re.search(
            r"run_gate corpus_inventory_binding\b(?P<body>.*?)# NOTE \(R47\)",
            text,
            re.DOTALL,
        )
        self.assertIsNotNone(match, "corpus inventory poison is missing from the seal checklist")
        assert match is not None
        body = match.group("body")
        self.assertIn("scripts/test_corpus_inventory_binding.sh", body)
        self.assertIn("0 failed", body)

    def test_g28_rejects_zero_work_and_contradictory_summaries(self) -> None:
        body = self.g28_body()
        self.assertTrue(body, "G28 corpus-inventory block is missing")
        if not body:
            return
        with tempfile.TemporaryDirectory(prefix="anubis-g28-corpus-") as raw:
            base = Path(raw)
            scripts = base / "scripts"
            scripts.mkdir()
            test_script = scripts / "test_corpus_inventory_binding.sh"
            out = base / "out"
            out.mkdir()

            def run_block(script_body: str) -> subprocess.CompletedProcess[str]:
                test_script.write_text(script_body, encoding="utf-8")
                return subprocess.run(
                    [
                        "bash",
                        "-c",
                        'gate() { printf "%s:%s\\n" "$1" "$2"; }\n' + body,
                    ],
                    cwd=base,
                    env={**dict(os.environ), "OUT": str(out)},
                    text=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.STDOUT,
                    check=False,
                )

            zero_work = run_block("#!/usr/bin/env bash\nexit 0\n")
            self.assertIn("G28_corpus_inventory_binding:FAIL", zero_work.stdout)

            real_summary = run_block(
                "#!/usr/bin/env bash\n"
                "printf 'CORPUS_INVENTORY_BINDING: 7 passed, 0 failed\\n'\n"
            )
            self.assertIn("G28_corpus_inventory_binding:PASS", real_summary.stdout)

            contradictory = run_block(
                "#!/usr/bin/env bash\n"
                "printf 'CORPUS_INVENTORY_BINDING: 7 passed, 0 failed\\n'\n"
                "printf 'CORPUS_INVENTORY_BINDING: 7 passed, 1 failed\\n'\n"
            )
            self.assertIn("G28_corpus_inventory_binding:FAIL", contradictory.stdout)

    def test_g29_registers_host_resource_contract_poison(self) -> None:
        text = (ROOT / "scripts" / "audit_unified.sh").read_text(encoding="utf-8")
        body = self.g29_body()
        self.assertTrue(body, "G29 host-resource block is missing")
        self.assertIn("scripts/test_host_resource_guard.sh", body)
        self.assertIn("HOST_RESOURCE_GUARD_SELFTEST", body)
        self.assertIn('gate "G29_host_resource_contract"', body)
        expected = re.search(r'^EXPECTED_GATES="([^"]+)"', text, re.MULTILINE)
        self.assertIsNotNone(expected, "EXPECTED_GATES is missing")
        assert expected is not None
        self.assertIn("G29_host_resource_contract", expected.group(1).split())

    def test_g30_registers_phase3_label_census_gate(self) -> None:
        text = (ROOT / "scripts" / "audit_unified.sh").read_text(encoding="utf-8")
        body = self.g30_body()
        self.assertTrue(body, "G30 phase3-label-census block is missing")
        self.assertIn("scripts/run_phase3_label_census.sh", body)
        self.assertIn('gate "G30_phase3_label_census"', body)
        expected = re.search(r'^EXPECTED_GATES="([^"]+)"', text, re.MULTILINE)
        self.assertIsNotNone(expected, "EXPECTED_GATES is missing")
        assert expected is not None
        self.assertIn("G30_phase3_label_census", expected.group(1).split())

    def test_phase3_label_census_files_exist(self) -> None:
        tool = ROOT / "scripts" / "lib" / "phase3_label_census.py"
        gate = ROOT / "scripts" / "run_phase3_label_census.sh"
        expect = ROOT / "docs" / "phase3" / "label_census.tsv"
        for path in (tool, gate, expect):
            self.assertTrue(path.exists(), f"missing Phase 3 census artifact: {path}")
        header = expect.read_text(encoding="utf-8").splitlines()[0].split("\t")
        self.assertEqual(
            header,
            ["fn", "field", "writes", "reads", "kind", "target_slice", "notes"],
        )

    def test_host_resource_contract_is_load_bearing_in_seal_checklist(self) -> None:
        text = (ROOT / "scripts" / "run_seal_checklist.sh").read_text(encoding="utf-8")
        match = re.search(
            r"run_gate host_resource_contract\b(?P<body>.*?)# NOTE \(R47\)",
            text,
            re.DOTALL,
        )
        self.assertIsNotNone(match, "host-resource contract is missing from the seal checklist")
        assert match is not None
        body = match.group("body")
        self.assertIn("scripts/test_host_resource_guard.sh", body)
        self.assertIn("fail=0", body)


if __name__ == "__main__":
    unittest.main(verbosity=2)
