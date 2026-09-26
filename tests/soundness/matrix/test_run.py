#!/usr/bin/env python3
"""Fake-checker tests for provisional matrix grading; never runs Anubis."""

import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import tempfile
import time
import unittest


SOURCE_RUNNER = Path(__file__).with_name("run.sh")
REGISTRY_HEADER = "id\tfamily\tcategory\tintent\tforms\tnotes\n"
HISTORY_HEADER = "id\tform\tlabel\tsource_commit\tbinary_sha256\tobserved\tresult\n"
FAKE_CHECKER = r"""#!/usr/bin/env python3
import json
import os
from pathlib import Path
import subprocess
import sys
import time

if sys.argv[1:] == ["--version"]:
    print("fake-checker 1")
    sys.exit(0)
if (len(sys.argv) != 4 or sys.argv[1] != "check" or
        sys.argv[3] != "--message-format=json"):
    print("fake checker expected typed JSON mode", file=sys.stderr)
    sys.exit(127)
mode = json.loads(os.environ["FAKE_MODES"])[Path(sys.argv[2]).name]
if mode.get("sleep"):
    time.sleep(mode["sleep"])
if mode.get("mutate"):
    with open(sys.argv[0], "a", encoding="utf-8") as stream:
        stream.write("\n# changed during matrix run\n")
if mode.get("background_marker"):
    subprocess.Popen(
        [sys.executable, "-c",
         "import pathlib,sys,time;time.sleep(1);pathlib.Path(sys.argv[1]).write_text('escaped')",
         mode["background_marker"]],
        stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
if mode.get("make_out_readonly"):
    Path(os.environ["FAKE_OUT"]).chmod(0o500)
if mode.get("stderr"):
    print(mode["stderr"], file=sys.stderr)
if "raw_stdout" in mode:
    sys.stdout.write(mode["raw_stdout"])
else:
    details = {
        "ANUBIS_ASSERTION_DISPROVED": ("contract", "disproved", "program", "repair_program"),
        "ANUBIS_ASSERTION_UNDECIDED": ("contract", "undecided", "capability",
                                       "restate_or_raise_budget"),
        "ANUBIS_ASSERTION_UNPROVEN": ("contract", "refused", "program", "repair_program"),
        "ANUBIS_SECRET_EXFILTRATION": ("frontend", "refused", "program", "repair_program"),
        "ANUBIS_PARSE_ERROR": ("frontend", "refused", "program", "repair_program"),
        "ANUBIS_PARSE_DEPTH_LIMIT": ("frontend", "refused", "program", "repair_program"),
        "ANUBIS_ANALYSIS_LIMIT": ("frontend", "refused", "capability",
                                  "restate_or_raise_budget"),
        "ANUBIS_CHECK_FAILED": ("frontend", "refused", "program", "repair_program"),
    }
    diagnostics = []
    for code in mode.get("codes", []):
        family, status, locus, action = details[code]
        diagnostic = {"$type": "anubis.diagnostic", "schema": "anubis-diagnostics/1",
                      "code": code, "family": family, "status": status,
                      "defect_locus": locus, "agent_action": action, "severity": "error",
                      "build_blocking": True, "message": mode.get("message", code),
                      "suggestions": []}
        if code.startswith("ANUBIS_ASSERTION_"):
            diagnostic["obligation"] = {
                "name": "case:" + Path(sys.argv[2]).name,
                "smt": "(assert false)", "declared_vars": [],
            }
        else:
            diagnostic["location"] = {
                "file": "cases/" + Path(sys.argv[2]).name,
                "line": 1, "column": 1, "span_start": 0, "span_end": 0,
            }
        diagnostic.update(mode.get("diagnostic_override", {}))
        diagnostics.append(diagnostic)
    counts = {"total": len(diagnostics), "disproved": 0, "undecided": 0,
              "replay_mismatch": 0, "refused": 0}
    for diagnostic in diagnostics:
        counts[diagnostic["status"]] += 1
        print(json.dumps(diagnostic, separators=(",", ":")))
    counts.update(mode.get("counts_override", {}))
    summary = {"$type": "anubis.summary", "schema": "anubis-diagnostics/1",
               "verdict": "fail" if diagnostics else "pass", "counts": counts}
    summary.update(mode.get("summary_override", {}))
    print(json.dumps(summary, separators=(",", ":")))
sys.exit(mode["rc"])
"""


def passed():
    return {"rc": 0}


def refused(code, **extra):
    return {"rc": 1, "codes": [code], **extra}


def expected_tuple(code, source):
    # These are authored test intents. They are never inferred from a checker
    # result, and this helper does not produce the real registry's expectations.
    details = {
        "ANUBIS_ASSERTION_DISPROVED": ("contract", "disproved", "program",
                                        "repair_program"),
        "ANUBIS_SECRET_EXFILTRATION": ("frontend", "refused", "program",
                                       "repair_program"),
        "ANUBIS_ASSERTION_UNDECIDED": ("contract", "undecided", "capability",
                                       "restate_or_raise_budget"),
        "ANUBIS_ASSERTION_UNPROVEN": ("contract", "refused", "program",
                                     "repair_program"),
        "ANUBIS_PARSE_ERROR": ("frontend", "refused", "program",
                               "repair_program"),
        "ANUBIS_PARSE_DEPTH_LIMIT": ("frontend", "refused", "program",
                                     "repair_program"),
        "ANUBIS_ANALYSIS_LIMIT": ("frontend", "refused", "capability",
                                  "restate_or_raise_budget"),
    }
    family, status, locus, action = details[code]
    if code.startswith("ANUBIS_ASSERTION_"):
        locator = {
            "kind": "obligation", "name": "case:" + Path(source).name,
            "smt_sha256": hashlib.sha256(b"(assert false)").hexdigest(),
        }
    else:
        locator = {"kind": "span", "file": source,
                   "span_start": 0, "span_end": 0}
    return {"code": code, "family": family, "status": status,
            "defect_locus": locus, "agent_action": action, "locator": locator}


class MatrixRunnerTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="matrix-run-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.matrix = self.root / "matrix"
        (self.matrix / "cases").mkdir(parents=True)
        shutil.copy2(SOURCE_RUNNER, self.matrix / "run.sh")
        (self.matrix / "run.sh").chmod(0o755)
        (self.matrix / "registry.tsv").write_text(REGISTRY_HEADER, encoding="utf-8")
        (self.matrix / "history.tsv").write_text(HISTORY_HEADER, encoding="utf-8")
        self.binary = self.root / "fake-checker"
        self.binary.write_text(FAKE_CHECKER, encoding="utf-8")
        self.binary.chmod(0o755)
        self.modes = {}
        self.expectations = {}

    def add_case(self, case_id, intent, carrier, direct=None, create_source=True,
                 expected_carrier="ANUBIS_ASSERTION_DISPROVED",
                 expected_direct="ANUBIS_ASSERTION_DISPROVED"):
        forms = "carrier,direct" if direct is not None else "carrier"
        with (self.matrix / "registry.tsv").open("a", encoding="utf-8") as stream:
            stream.write(f"{case_id}\tfake\tunit\t{intent}\t{forms}\tfake source\n")
        if create_source:
            (self.matrix / "cases" / f"{case_id}.anb").write_text(
                "fn main() {}\n", encoding="utf-8")
        self.modes[f"{case_id}.anb"] = carrier
        if intent != "ACCEPT":
            source = self.matrix / "cases" / f"{case_id}.anb"
            source_sha = (hashlib.sha256(source.read_bytes()).hexdigest()
                          if source.exists() else "0" * 64)
            self.expectations[f"{case_id}/carrier"] = {
                "source_sha256": source_sha,
                "diagnostics": [expected_tuple(expected_carrier,
                                               f"cases/{case_id}.anb")],
            }
        if direct is not None:
            (self.matrix / "cases" / f"{case_id}.direct.anb").write_text(
                "fn main() {}\n", encoding="utf-8")
            self.modes[f"{case_id}.direct.anb"] = direct
            if intent != "ACCEPT":
                source = self.matrix / "cases" / f"{case_id}.direct.anb"
                self.expectations[f"{case_id}/direct"] = {
                    "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                    "diagnostics": [expected_tuple(expected_direct,
                                                   f"cases/{case_id}.direct.anb")],
                }

    def registry_digest(self):
        return hashlib.sha256((self.matrix / "registry.tsv").read_bytes()).hexdigest()

    def invoke(self, *, record=False, timeout="2", expected="current",
               expected_classes="current"):
        out = self.root / "output"
        command = [str(self.matrix / "run.sh"), str(self.binary), "--out", str(out),
                   "--timeout", timeout]
        if expected is not None:
            digest = self.registry_digest() if expected == "current" else expected
            command.extend(["--expected-registry-sha256", digest])
        if expected_classes is not None:
            classes = (self.expectations if expected_classes == "current" else
                       expected_classes)
            manifest = self.root / "expected-classes.json"
            manifest.write_text(json.dumps({
                "schema": "anubis-matrix-expected-diagnostics/2",
                "registry_sha256": self.registry_digest(), "non_accept": classes,
            }, sort_keys=True), encoding="utf-8")
            classes_digest = hashlib.sha256(manifest.read_bytes()).hexdigest()
            command.extend(["--expected-classes", str(manifest),
                            "--expected-classes-sha256", classes_digest])
        if record:
            command.extend(["--record", "fake-baseline", "a" * 40])
        environment = dict(os.environ)
        environment["FAKE_MODES"] = json.dumps(self.modes)
        environment["FAKE_OUT"] = str(out)
        completed = subprocess.run(command, cwd=self.root, env=environment,
                                   text=True, capture_output=True, timeout=20)
        receipt = json.loads((out / "receipt.json").read_text(encoding="utf-8"))
        return completed, receipt, out

    def test_all_provisionally_met_keeps_private_split_transcripts_and_hashes(self):
        self.add_case("bad", "REJECT",
                      refused("ANUBIS_ASSERTION_DISPROVED"),
                      refused("ANUBIS_ASSERTION_DISPROVED"))
        self.add_case("good", "ACCEPT", passed())
        self.add_case("syntax", "MALFORMED", refused("ANUBIS_PARSE_ERROR"),
                      expected_carrier="ANUBIS_PARSE_ERROR")
        self.add_case("limit", "LIMIT", refused("ANUBIS_ANALYSIS_LIMIT"),
                      expected_carrier="ANUBIS_ANALYSIS_LIMIT")
        completed, receipt, out = self.invoke()
        self.assertEqual(completed.returncode, 2, completed.stderr)
        self.assertEqual(receipt["status"], "INCOMPLETE")
        self.assertEqual(receipt["coverage_scope"], "provisional-operator-supplied")
        self.assertEqual(receipt["row_grade_status"], "provisional_all_matched")
        self.assertNotIn("PASS", [e["result"] for e in receipt["entries"]])
        self.assertFalse(receipt["canonical_manifest_integrated"])
        self.assertEqual(receipt["expected_forms"], receipt["completed_forms"])
        self.assertEqual(receipt["binary_sha256_before"], receipt["binary_sha256_after"])
        self.assertTrue(receipt["runner_sha256"])
        self.assertTrue(receipt["registry_sha256"])
        self.assertFalse(receipt["history_appended"])
        self.assertEqual(receipt["history_state"], "not_requested")
        self.assertFalse(receipt["source_commit_bound"])
        self.assertFalse((out / "history-stage.tsv").exists())
        self.assertNotIn(str(self.root), json.dumps(receipt))
        self.assertEqual(stat.S_IMODE(out.stat().st_mode), 0o700)
        self.assertEqual(stat.S_IMODE((out / "logs").stat().st_mode), 0o700)
        for entry in receipt["entries"]:
            self.assertFalse(Path(entry["source"]).is_absolute())
            for field in ("stdout", "stderr"):
                self.assertFalse(Path(entry[field]).is_absolute())
                self.assertTrue((out / entry[field]).is_file())
                self.assertEqual(stat.S_IMODE((out / entry[field]).stat().st_mode), 0o600)
            self.assertTrue(entry["source_sha256"])
            self.assertTrue(entry["stdout_sha256"])
            self.assertTrue(entry["stderr_sha256"])
            self.assertTrue(entry["transcript_sha256"])
        result_rows = (out / "results.tsv").read_text(encoding="utf-8").splitlines()
        self.assertEqual(len(result_rows) - 1, receipt["completed_forms"])

    def test_known_false_accept_has_failed_row_and_retained_receipt(self):
        self.add_case("leak", "REJECT", passed())
        completed, receipt, out = self.invoke()
        self.assertEqual(completed.returncode, 2, completed.stderr)
        self.assertEqual(receipt["status"], "INCOMPLETE")
        self.assertEqual(receipt["row_grade_status"], "has_failures")
        self.assertEqual(receipt["entries"][0]["result"], "FAIL-silent-accept")
        self.assertFalse(receipt["history_appended"])
        self.assertFalse((out / "history-stage.tsv").exists())

    def test_direct_reject_twin_accepting_has_failed_row(self):
        self.add_case("pair", "REJECT", refused("ANUBIS_ASSERTION_DISPROVED"), passed())
        completed, receipt, _out = self.invoke()
        self.assertEqual(completed.returncode, 2, completed.stderr)
        self.assertEqual(receipt["entries"][0]["result"], "MATCH-provisional")
        self.assertEqual(receipt["entries"][1]["result"], "FAIL-silent-accept")
        self.assertEqual(receipt["status"], "INCOMPLETE")
        self.assertEqual(receipt["row_grade_status"], "has_failures")

    def test_contract_disproof_cannot_satisfy_frozen_ifc_expectation(self):
        self.add_case("ifc", "REJECT", refused("ANUBIS_ASSERTION_DISPROVED"),
                      expected_carrier="ANUBIS_SECRET_EXFILTRATION")
        completed, receipt, _out = self.invoke()
        self.assertEqual(completed.returncode, 2, completed.stderr)
        self.assertEqual(receipt["entries"][0]["observed"], "DISPROVED")
        self.assertEqual(receipt["entries"][0]["result"], "FAIL-wrong-class")

    def test_reject_without_frozen_class_tuple_is_incomplete(self):
        self.add_case("unbound", "REJECT", refused("ANUBIS_ASSERTION_DISPROVED"))
        completed, receipt, _out = self.invoke(expected_classes=None)
        self.assertEqual(completed.returncode, 2, completed.stderr)
        self.assertEqual(receipt["entries"][0]["result"], "INCOMPLETE-class-unbound")
        self.assertEqual(receipt["status"], "INCOMPLETE")

    def test_digest_bound_but_partial_class_file_is_incomplete(self):
        self.add_case("missing_tuple", "REJECT",
                      refused("ANUBIS_ASSERTION_DISPROVED"))
        completed, receipt, _out = self.invoke(expected_classes={})
        self.assertEqual(completed.returncode, 2, completed.stderr)
        self.assertEqual(receipt["entries"][0]["result"], "INCOMPLETE-class-unbound")
        self.assertEqual(receipt["unbound_non_accept_forms"], ["missing_tuple/carrier"])

    def test_frozen_class_expectation_rejects_changed_source(self):
        self.add_case("changed", "REJECT", refused("ANUBIS_ASSERTION_DISPROVED"))
        (self.matrix / "cases" / "changed.anb").write_text(
            "fn main() { assert(false); }\n", encoding="utf-8")
        completed, receipt, _out = self.invoke()
        self.assertEqual(completed.returncode, 2, completed.stderr)
        self.assertEqual(receipt["completed_forms"], 0)
        self.assertEqual(receipt["status"], "INCOMPLETE")

    def test_timeout_is_incomplete_and_never_recorded(self):
        self.add_case("slow", "REJECT", refused(
            "ANUBIS_ASSERTION_DISPROVED", sleep=3))
        completed, receipt, out = self.invoke(record=True, timeout="1")
        self.assertEqual(completed.returncode, 2, completed.stderr)
        self.assertEqual(receipt["status"], "INCOMPLETE")
        self.assertEqual(receipt["entries"][0]["observed"], "TOOL_TIMEOUT")
        self.assertFalse(receipt["history_appended"])
        self.assertFalse((out / "history-stage.tsv").exists())
        self.assertEqual((self.matrix / "history.tsv").read_text(), HISTORY_HEADER)

    def test_stderr_echoed_security_code_and_tool_error_cannot_reject(self):
        self.add_case("broken", "REJECT", {"rc": 127,
                      "stderr": "ANUBIS_ASSERTION_DISPROVED"})
        completed, receipt, out = self.invoke()
        self.assertEqual(completed.returncode, 2, completed.stderr)
        self.assertEqual(receipt["entries"][0]["observed"], "TOOL_EXIT:127")
        self.assertEqual(receipt["status"], "INCOMPLETE")
        self.assertIn("ANUBIS_ASSERTION_DISPROVED",
                      (out / receipt["entries"][0]["stderr"]).read_text())

    def test_echoed_code_on_stdout_is_not_a_json_diagnostic(self):
        self.add_case("echo", "REJECT", {"rc": 1,
                      "raw_stdout": "ANUBIS_ASSERTION_DISPROVED\n"})
        completed, receipt, _out = self.invoke()
        self.assertEqual(completed.returncode, 2, completed.stderr)
        self.assertEqual(receipt["entries"][0]["observed"], "TOOL_PROTOCOL")

    def test_summary_count_and_exit_mismatch_are_incomplete(self):
        self.add_case("counter", "REJECT", refused(
            "ANUBIS_ASSERTION_DISPROVED", counts_override={"total": 0}))
        completed, receipt, _out = self.invoke()
        self.assertEqual(completed.returncode, 2, completed.stderr)
        self.assertEqual(receipt["entries"][0]["observed"], "TOOL_PROTOCOL")

    def test_pass_with_undischarged_coverage_is_protocol_error(self):
        self.add_case("unsettled", "ACCEPT", {
            "rc": 0, "summary_override": {"coverage": {
                "certified": 0, "trusted_to_solver": 0, "discharged": 0,
                "not_discharged": 1, "uncertified": [],
                "witnesses_retained": False,
            }},
        })
        completed, receipt, _out = self.invoke()
        self.assertEqual(completed.returncode, 2, completed.stderr)
        self.assertEqual(receipt["entries"][0]["observed"], "TOOL_PROTOCOL")

    def test_typed_security_locus_mismatch_is_incomplete(self):
        self.add_case("mismatch", "REJECT", refused(
            "ANUBIS_SECRET_EXFILTRATION",
            diagnostic_override={"defect_locus": "compiler"}))
        completed, receipt, _out = self.invoke()
        self.assertEqual(completed.returncode, 2, completed.stderr)
        self.assertEqual(receipt["entries"][0]["observed"], "TOOL_PROTOCOL")

    def test_unproven_and_mixed_do_not_complete_reject(self):
        self.add_case("unproven", "REJECT", refused("ANUBIS_ASSERTION_UNPROVEN"))
        self.add_case("mixed", "REJECT", {"rc": 1,
                      "codes": ["ANUBIS_ASSERTION_DISPROVED",
                                "ANUBIS_ASSERTION_UNDECIDED"]})
        completed, receipt, _out = self.invoke()
        self.assertEqual(completed.returncode, 2, completed.stderr)
        self.assertEqual([e["observed"] for e in receipt["entries"]],
                         ["UNPROVEN", "MIXED"])
        self.assertEqual([e["result"] for e in receipt["entries"]],
                         ["FAIL-wrong-class", "FAIL-wrong-class"])

    def test_malformed_requires_parse_and_limit_requires_named_limit(self):
        self.add_case("wrongparse", "MALFORMED", refused("ANUBIS_CHECK_FAILED"),
                      expected_carrier="ANUBIS_PARSE_ERROR")
        self.add_case("wronglimit", "LIMIT", refused("ANUBIS_PARSE_ERROR"),
                      expected_carrier="ANUBIS_ANALYSIS_LIMIT")
        completed, receipt, _out = self.invoke()
        self.assertEqual(completed.returncode, 2, completed.stderr)
        self.assertEqual([e["result"] for e in receipt["entries"]],
                         ["INVALID", "INCOMPLETE-class-unbound"])

    def test_english_limit_phrase_cannot_convert_generic_parse_error(self):
        self.add_case("phrase", "LIMIT", refused(
            "ANUBIS_PARSE_ERROR", message="program is nested too deeply"),
            expected_carrier="ANUBIS_PARSE_DEPTH_LIMIT")
        completed, receipt, _out = self.invoke()
        self.assertEqual(completed.returncode, 2, completed.stderr)
        self.assertEqual(receipt["entries"][0]["observed"], "PARSE")
        self.assertEqual(receipt["entries"][0]["result"], "INCOMPLETE-class-unbound")

    def test_undecided_requires_exact_intended_obligation(self):
        self.add_case("undecided", "UNRES", refused("ANUBIS_ASSERTION_UNDECIDED"),
                      expected_carrier="ANUBIS_ASSERTION_UNDECIDED")
        self.add_case("wrong_obligation", "UNRES", refused(
            "ANUBIS_ASSERTION_UNDECIDED", diagnostic_override={"obligation": {
                "name": "some-other-obligation", "smt": "(assert false)",
                "declared_vars": [],
            }}), expected_carrier="ANUBIS_ASSERTION_UNDECIDED")
        completed, receipt, _out = self.invoke()
        self.assertEqual(completed.returncode, 2, completed.stderr)
        self.assertEqual([e["result"] for e in receipt["entries"]],
                         ["MATCH-provisional", "FAIL-wrong-obligation"])
        self.assertEqual(receipt["row_grade_status"], "has_failures")
        self.assertEqual(receipt["status"], "INCOMPLETE")

    def test_reject_or_unresolved_cannot_pass_without_intended_obligation(self):
        self.add_case("either", "REJ|UNRES", refused("ANUBIS_ASSERTION_UNDECIDED"),
                      expected_carrier="ANUBIS_ASSERTION_UNDECIDED")
        self.add_case("unlocated", "REJ|UNRES", refused(
            "ANUBIS_ASSERTION_UNDECIDED", diagnostic_override={"obligation": None}),
            expected_carrier="ANUBIS_ASSERTION_UNDECIDED")
        completed, receipt, _out = self.invoke()
        self.assertEqual(completed.returncode, 2, completed.stderr)
        self.assertEqual([e["result"] for e in receipt["entries"]],
                         ["MATCH-provisional", "INCOMPLETE-obligation-unbound"])
        self.assertEqual(receipt["status"], "INCOMPLETE")

    def test_reject_or_unresolved_direct_form_needs_its_own_tuple(self):
        self.add_case("two_forms", "REJ|UNRES",
                      refused("ANUBIS_ASSERTION_UNDECIDED"),
                      refused("ANUBIS_ASSERTION_DISPROVED"),
                      expected_carrier="ANUBIS_ASSERTION_UNDECIDED",
                      expected_direct="ANUBIS_ASSERTION_DISPROVED")
        provisional = dict(self.expectations)
        provisional.pop("two_forms/direct")
        completed, receipt, _out = self.invoke(expected_classes=provisional)
        self.assertEqual(completed.returncode, 2, completed.stderr)
        self.assertEqual([e["result"] for e in receipt["entries"]],
                         ["MATCH-provisional", "INCOMPLETE-class-unbound"])
        self.assertEqual(receipt["unbound_non_accept_forms"],
                         ["two_forms/direct"])

    def test_non_accept_intent_without_frozen_tuple_is_incomplete(self):
        self.add_case("unbound_unres", "UNRES",
                      refused("ANUBIS_ASSERTION_UNDECIDED"),
                      expected_carrier="ANUBIS_ASSERTION_UNDECIDED")
        completed, receipt, _out = self.invoke(expected_classes={})
        self.assertEqual(completed.returncode, 2, completed.stderr)
        self.assertEqual(receipt["entries"][0]["result"], "INCOMPLETE-class-unbound")
        self.assertEqual(receipt["unbound_non_accept_forms"],
                         ["unbound_unres/carrier"])

    def test_caller_rehashed_partial_registry_cannot_claim_completion(self):
        self.add_case("partial_only", "ACCEPT", passed())
        completed, receipt, _out = self.invoke()
        self.assertEqual(completed.returncode, 2, completed.stderr)
        self.assertEqual(receipt["row_grade_status"], "provisional_all_matched")
        self.assertEqual(receipt["coverage_scope"], "provisional-operator-supplied")
        self.assertFalse(receipt["canonical_manifest_integrated"])
        self.assertEqual(receipt["status"], "INCOMPLETE")

    def test_normal_exit_cleans_checker_background_child(self):
        marker = self.root / "escaped.txt"
        self.add_case("child", "ACCEPT", {
            "rc": 0, "background_marker": str(marker),
        })
        completed, receipt, _out = self.invoke()
        self.assertEqual(completed.returncode, 2, completed.stderr)
        self.assertEqual(receipt["status"], "INCOMPLETE")
        self.assertEqual(receipt["entries"][0]["observed"], "TOOL_CHILD_CLEANUP")
        self.assertEqual(receipt["entries"][0]["result"], "INCOMPLETE-tool")
        time.sleep(1.3)
        self.assertFalse(marker.exists())

    def test_record_syntax_is_withheld_without_source_bound_manifest(self):
        self.add_case("good", "ACCEPT", passed())
        completed, receipt, out = self.invoke(record=True)
        self.assertEqual(completed.returncode, 2, completed.stderr)
        self.assertEqual(receipt["row_grade_status"], "provisional_all_matched")
        self.assertEqual(receipt["history_state"], "disabled-unbound-source-commit")
        self.assertFalse(receipt["history_appended"])
        self.assertFalse((out / "history-stage.tsv").exists())
        self.assertEqual((self.matrix / "history.tsv").read_text(), HISTORY_HEADER)

    def test_preliminary_receipt_survives_final_publication_failure(self):
        self.add_case("readonly", "ACCEPT", {
            "rc": 0, "make_out_readonly": True,
        })
        out = self.root / "output"
        try:
            completed, receipt, _out = self.invoke()
            self.assertEqual(completed.returncode, 2, completed.stderr)
            self.assertEqual(receipt["phase"], "running")
            self.assertEqual(receipt["status"], "INCOMPLETE")
        finally:
            if out.exists():
                out.chmod(0o700)

    def test_missing_case_is_preflight_incomplete_without_history(self):
        self.add_case("missing", "REJECT", refused("ANUBIS_ASSERTION_DISPROVED"),
                      create_source=False)
        completed, receipt, out = self.invoke(record=True)
        self.assertEqual(completed.returncode, 2, completed.stderr)
        self.assertEqual(receipt["status"], "INCOMPLETE")
        self.assertEqual(receipt["completed_forms"], 0)
        self.assertFalse(receipt["history_appended"])
        self.assertFalse((out / "history-stage.tsv").exists())

    def test_unregistered_case_source_is_preflight_incomplete(self):
        self.add_case("registered", "ACCEPT", passed())
        (self.matrix / "cases" / "unregistered.anb").write_text(
            "fn main() {}\n", encoding="utf-8")
        completed, receipt, _out = self.invoke()
        self.assertEqual(completed.returncode, 2, completed.stderr)
        self.assertEqual(receipt["completed_forms"], 0)
        self.assertTrue(any("registry/cases mismatch" in error
                            for error in receipt["errors"]))

    def test_binary_change_invalidates_complete_looking_results(self):
        self.add_case("mutate", "ACCEPT", {"rc": 0, "mutate": True})
        completed, receipt, _out = self.invoke(record=True)
        self.assertEqual(completed.returncode, 2, completed.stderr)
        self.assertEqual(receipt["status"], "INCOMPLETE")
        self.assertNotEqual(receipt["binary_sha256_before"],
                            receipt["binary_sha256_after"])
        self.assertFalse(receipt["history_appended"])

    def test_unbound_registry_is_scoped_and_incomplete(self):
        self.add_case("good", "ACCEPT", passed())
        completed, receipt, _out = self.invoke(expected=None)
        self.assertEqual(completed.returncode, 2, completed.stderr)
        self.assertEqual(receipt["row_grade_status"], "provisional_all_matched")
        self.assertEqual(receipt["status"], "INCOMPLETE")
        self.assertEqual(receipt["coverage_scope"], "registry-unbound")

    def test_partial_registry_cannot_match_frozen_digest(self):
        self.add_case("frozen", "ACCEPT", passed())
        frozen_digest = self.registry_digest()
        (self.matrix / "registry.tsv").write_text(REGISTRY_HEADER, encoding="utf-8")
        self.add_case("partial", "ACCEPT", passed())
        completed, receipt, _out = self.invoke(expected=frozen_digest)
        self.assertEqual(completed.returncode, 2, completed.stderr)
        self.assertEqual(receipt["status"], "INCOMPLETE")
        self.assertNotEqual(receipt["coverage_scope"], "provisional-operator-supplied")
        self.assertEqual(receipt["completed_forms"], 0)


if __name__ == "__main__":
    unittest.main()
