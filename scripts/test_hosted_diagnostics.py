#!/usr/bin/env python3
"""Controlled failure-artifact checks; no compiler or hosted gate is executed."""

import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


MODULE_PATH = Path(__file__).with_name("prepare_hosted_diagnostics.py")
SPEC = importlib.util.spec_from_file_location("hosted_diagnostics", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class HostedDiagnosticsTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        # Darwin's temporary path may begin at the /var symlink. Use its
        # physical fixture root; dedicated symlink controls remain explicit.
        self.root = Path(self.temp.name).resolve()
        self.source = self.root / "ci_gate"
        self.source.mkdir()
        self.output = self.root / "diagnostics"

    def prepare(self):
        MODULE.prepare(self.source, self.output, {"GITHUB_RUN_ID": "fixture-run"})
        return json.loads((self.output / "DIAGNOSTIC_ONLY.json").read_text())

    def test_failed_report_retained_without_pass_attestation_or_raw_gate_material(self):
        report = b'{"verdict":"FAIL","gates":[{"gate":"G3_test","status":"FAIL"}]}\n'
        (self.source / "gate_report.json").write_bytes(report)
        (self.source / "gate_log.txt").write_text("FAIL G3_test\n")
        (self.source / "g3_test.log").write_text("PRIVATE_RAW_TEST_OUTPUT\n")
        (self.source / "private-key.pem").write_text("PRIVATE_TEST_KEY\n")
        before = {p.name: p.read_bytes() for p in self.source.iterdir()}
        marker = self.prepare()
        self.assertEqual((self.output / "unvalidated-gate_report.json").read_bytes(), report)
        self.assertFalse(marker["is_release_attestation"])
        self.assertEqual(marker["gate_validation"], "not_performed")
        self.assertEqual(marker["files"]["attestation_identity.txt"]["state"], "missing")
        self.assertEqual(before, {p.name: p.read_bytes() for p in self.source.iterdir()})
        all_output = b"".join(p.read_bytes() for p in self.output.iterdir())
        self.assertNotIn(b"PRIVATE_RAW_TEST_OUTPUT", all_output)
        self.assertNotIn(b"PRIVATE_TEST_KEY", all_output)
        self.assertFalse((self.root / "ci_public").exists())

    def test_forged_success_and_malformed_reports_are_only_unvalidated_diagnostics(self):
        for content in [b'{"verdict":"HOSTED_PASS","pass":30}', b'{"truncated":']:
            with self.subTest(content=content):
                output = self.root / hashlib.sha256(content).hexdigest()
                (self.source / "gate_report.json").write_bytes(content)
                MODULE.prepare(self.source, output, {})
                marker = json.loads((output / "DIAGNOSTIC_ONLY.json").read_text())
                self.assertFalse(marker["is_release_attestation"])
                self.assertEqual(marker["gate_validation"], "not_performed")
                self.assertEqual((output / "unvalidated-gate_report.json").read_bytes(), content)

    def test_symlink_and_nonregular_allowlisted_inputs_are_not_followed(self):
        private = self.root / "private.txt"
        private.write_text("PRIVATE_SYMLINK_TARGET\n")
        (self.source / "gate_log.txt").symlink_to(private)
        (self.source / "gate_report.json").mkdir()
        os.mkfifo(self.source / "profile_environment.txt")
        marker = self.prepare()
        for name in ["gate_log.txt", "gate_report.json", "profile_environment.txt"]:
            self.assertEqual(marker["files"][name]["state"], "omitted")
            self.assertFalse((self.output / ("unvalidated-" + name)).exists())
        self.assertNotIn(b"PRIVATE_SYMLINK_TARGET", (self.output / "DIAGNOSTIC_ONLY.json").read_bytes())

    def test_symlinked_parent_refused_and_existing_destination_preserved(self):
        link = self.root / "linked"
        link.symlink_to(self.root, target_is_directory=True)
        with self.assertRaises(ValueError):
            MODULE.prepare(link / "ci_gate", self.output, {})
        self.assertFalse(self.output.exists())
        self.output.mkdir()
        marker = self.output / "keep.txt"
        marker.write_text("unrelated")
        with self.assertRaises(ValueError):
            self.prepare()
        self.assertEqual(marker.read_text(), "unrelated")

    def test_manifest_binds_every_retained_payload_and_no_extra_environment(self):
        (self.source / "gate_log.txt").write_text("FAIL G3_test\n")
        MODULE.prepare(self.source, self.output, {"GITHUB_RUN_ID": "fixture-run", "PRIVATE_ENV": "PRIVATE_ENV_VALUE"})
        listed = set()
        for line in (self.output / "MANIFEST.sha256").read_text().splitlines():
            digest, name = line.split("  ", 1)
            self.assertEqual(digest, hashlib.sha256((self.output / name).read_bytes()).hexdigest())
            listed.add(name)
        self.assertEqual(listed, {p.name for p in self.output.iterdir() if p.name != "MANIFEST.sha256"})
        self.assertNotIn("PRIVATE_ENV_VALUE", (self.output / "DIAGNOSTIC_ONLY.json").read_text())

    def test_missing_source_produces_explicit_missing_evidence(self):
        self.source.rmdir()
        marker = self.prepare()
        self.assertTrue(all(row["state"] == "missing" for row in marker["files"].values()))
        self.assertFalse(marker["is_release_attestation"])

    def test_cli_packaging_success_does_not_overwrite_prior_gate_exit(self):
        (self.source / "gate_report.json").write_text('{"verdict":"FAIL"}\n')
        shell = '\n'.join([
            'false',
            'gate_rc=$?',
            '"$1" "$2" --source "$3" --out "$4" || exit "$?"',
            'exit "$gate_rc"',
        ])
        result = subprocess.run(
            ["bash", "-c", shell, "diagnostic-test", sys.executable, str(MODULE_PATH), str(self.source), str(self.output)],
            capture_output=True, check=False,
        )
        self.assertEqual(result.returncode, 1)
        self.assertTrue((self.output / "DIAGNOSTIC_ONLY.json").is_file())

    def test_existing_success_validator_still_rejects_failed_incomplete_and_forged_reports(self):
        workflow = MODULE_PATH.parent.parent / ".github/workflows/ci.yml"
        text = workflow.read_text()
        start = "          python3 - out/ci_gate/gate_report.json <<'PY'\n"
        embedded = text.split(start, 1)[1].split("          PY\n", 1)[0]
        validator = "\n".join(line.removeprefix("          ") for line in embedded.splitlines())
        names = [
            "G1_fmt", "G2_clippy", "G3_test", "G4_build_release",
            "G5_language_fixtures", "G6_turing_core", "G7_pca",
            "G8_security_fixtures", "G9_poc_kit", "G10_prove", "G11_enum_match",
            "G12_for_in", "G13_lang_trio", "G14_offensive", "G15_dogfood_feel",
            "G16_docs_drift", "G17_stdlib_failclosed", "G18_native_authoritative",
            "G19_walker_completeness", "G20_gate_common_adoption", "G21_formal",
            "G22_fixture_preflight", "G23_carrier_totality", "G24_promise_coherence",
            "G25_formal_kernel", "G26_proof_correspondence", "G27_phase_metrics_ledger",
            "G28_corpus_inventory_binding", "G29_host_resource_contract",
            "G30_phase3_label_census", "G31_security_label_correspondence",
        ]
        valid = {
            "profile": "hosted", "verdict": "HOSTED_PASS", "pass": 30,
            "fail": 0, "skip": 0, "external": 1, "total": 31,
            "gates": [{"gate": name, "status": "EXTERNAL" if name == "G9_poc_kit" else "PASS"} for name in names],
        }
        variants = [("valid", valid, True)]
        failed = json.loads(json.dumps(valid))
        failed.update(verdict="FAIL", **{"pass": 29, "fail": 1})
        failed["gates"][2]["status"] = "FAIL"
        variants.append(("failed", failed, False))
        missing = json.loads(json.dumps(valid))
        missing["gates"].pop()
        variants.append(("missing_gate", missing, False))
        forged = json.loads(json.dumps(valid))
        forged["gates"][2]["status"] = "FAIL"
        variants.append(("forged_pass", forged, False))
        external = json.loads(json.dumps(valid))
        external["gates"][20]["status"] = "EXTERNAL"
        variants.append(("extra_external", external, False))
        report_path = self.source / "gate_report.json"
        for name, report, accepted in variants:
            with self.subTest(name=name):
                report_path.write_text(json.dumps(report))
                result = subprocess.run(
                    [sys.executable, "-", str(report_path)], input=validator,
                    text=True, capture_output=True, check=False,
                )
                self.assertEqual(result.returncode == 0, accepted, result.stderr)

    def test_diagnostics_cannot_supply_the_existing_release_consumer_layout(self):
        (self.source / "gate_report.json").write_text('{"verdict":"HOSTED_PASS"}\n')
        (self.source / "attestation_identity.txt").write_text("github_sha=fixture\n")
        (self.source / "profile_environment.txt").write_text("fixture\n")
        self.prepare()
        script = MODULE_PATH.parent.parent / "scripts/build_public_release.sh"
        body = script.read_text().split('# ---------------------------------------------------------------- evidence\n', 1)[1]
        body = body.split('# ---------------------------------------------------------------- leak and integrity gate', 1)[0]
        prefix = 'set -eu\nCI_ARTIFACT="$1"\nSTAGE="$2"\nCOMMIT=fixture\ndie() { echo "$*" >&2; exit 1; }\n'
        result = subprocess.run(
            ["bash", "-c", prefix + body, "consumer-test", str(self.output), str(self.root / "release")],
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("missing gate_report.json", result.stderr)

    def test_g3_extracts_only_structured_results_and_hashes_complete_log(self):
        raw = b'''error[E0425]: PRIVATE_COMPILER_MESSAGE /private/path
running 1 test
test actual::pass ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
running 2 tests
test actual::failure ... FAILED
test second::failure ... FAILED
failures:
---- actual::failure stdout ----
PRIVATE_PANIC_BODY
test forged::failure ... FAILED
error[E0999]: PRIVATE_ASSERTION_BODY
test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
running 1 test
test PRIVATE_MARKER ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
  process didn't exit successfully: /private/command (signal: 6, SIGABRT: process abort signal)
'''
        (self.source / "g3_test.log").write_bytes(raw)
        (self.source / "g3_test.exit").write_bytes(b"101\n")
        g3 = self.prepare()["g3"]
        self.assertEqual(g3["failed_tests"], ["actual::failure", "second::failure"])
        self.assertEqual(g3["rust_error_codes"], ["E0425"])
        self.assertEqual(g3["raw_log"]["sha256"], hashlib.sha256(raw).hexdigest())
        self.assertEqual(g3["exit_status"], 101)
        self.assertEqual(g3["termination_categories"], ["rustc_error_reported", "libtest_failure_reported"])
        published = b"".join(p.read_bytes() for p in self.output.iterdir())
        for marker in [b"PRIVATE_", b"/private/", b"forged::failure", b"E0999"]:
            self.assertNotIn(marker, published)

    def test_g3_reports_only_a_literal_cargo_signal_suffix_outside_captured_output(self):
        (self.source / "g3_test.log").write_bytes(
            b"  process didn't exit successfully: /private/command (signal: 6, SIGABRT: process abort signal)\n"
        )
        self.assertEqual(self.prepare()["g3"]["termination_categories"], ["cargo_reported_SIGABRT"])

    def test_g3_ignores_noise_and_never_infers_a_signal_from_exit(self):
        raw = b'''test not_in_suite ... FAILED
running 1 test
 test indented::name ... FAILED
"test quoted::name ... FAILED"
prefix test prefixed::name ... FAILED
test /private/path ... FAILED
test \x1b[31mcolored::name ... FAILED
test utf8_\xc3\xa9 ... FAILED
quoted: process didn't exit successfully: /private/cmd (signal: 6, SIGABRT: text)
'''
        (self.source / "g3_test.log").write_bytes(raw)
        (self.source / "g3_test.exit").write_bytes(b"137\n")
        g3 = self.prepare()["g3"]
        self.assertEqual(g3["failed_tests"], [])
        self.assertEqual(g3["termination_categories"], ["unclassified_nonzero_exit"])

    def test_g3_exit_absence_malformed_and_nonregular_are_distinct(self):
        self.assertEqual(MODULE.g3_diagnostics(self.source)["exit_status_state"], "missing")
        for raw in [b"-1\n", b"256\n", b"101 trailing", b"1" * 100, b"00\n", b""]:
            with self.subTest(raw=raw):
                (self.source / "g3_test.exit").write_bytes(raw)
                g3 = MODULE.g3_diagnostics(self.source)
                self.assertIsNone(g3["exit_status"])
                self.assertEqual(g3["exit_status_state"], "malformed")
        (self.source / "g3_test.exit").unlink()
        os.mkfifo(self.source / "g3_test.exit")
        self.assertEqual(MODULE.g3_diagnostics(self.source)["exit_status_state"], "unreadable_or_nonregular")

    def test_g3_raw_log_symlink_fifo_and_empty(self):
        path = self.source / "g3_test.log"
        private = self.root / "private"
        private.write_text("PRIVATE_RAW_TARGET")
        path.symlink_to(private)
        self.assertEqual(MODULE.g3_diagnostics(self.source)["raw_log"]["state"], "omitted")
        path.unlink()
        os.mkfifo(path)
        self.assertEqual(MODULE.g3_diagnostics(self.source)["raw_log"]["state"], "omitted")
        path.unlink()
        path.write_bytes(b"")
        self.assertEqual(MODULE.g3_diagnostics(self.source)["raw_log"]["sha256"], hashlib.sha256(b"").hexdigest())

    def test_g3_limits_extraction_but_hashes_every_raw_byte(self):
        raw = b"running 9999 tests\n" + b"test " + b"x" * (MODULE.MAX_LINE_BYTES + 50) + b" ... FAILED\n"
        raw += b"test " + b"n" * (MODULE.MAX_NAME_BYTES + 1) + b" ... FAILED\n"
        raw += b"".join(f"test sample::n{i} ... FAILED\n".encode() for i in range(MODULE.MAX_ENTRIES + 5))
        (self.source / "g3_test.log").write_bytes(raw)
        g3 = self.prepare()["g3"]
        self.assertTrue(g3["extraction_truncated"])
        self.assertEqual(len(g3["failed_tests"]), MODULE.MAX_ENTRIES)
        self.assertEqual(g3["raw_log"]["sha256"], hashlib.sha256(raw).hexdigest())

    def test_g3_status_capture_precedes_other_commands_in_real_gate(self):
        script = (MODULE_PATH.parent.parent / "scripts/audit_unified.sh").read_text()
        body = script.split('# ── G3: cargo test ──', 1)[1].split('# ── G4:', 1)[0]
        body = body.replace('cargo test --all', 'bash -c "exit 23"')
        prefix = 'OUT="$1"\ngate() { test "$2" = FAIL; }\n'
        result = subprocess.run(["bash", "-c", prefix + body, "capture-test", str(self.source)], capture_output=True, check=False)
        self.assertEqual(result.returncode, 0)
        self.assertEqual((self.source / "g3_test.exit").read_bytes(), b"23\n")


if __name__ == "__main__":
    unittest.main()
