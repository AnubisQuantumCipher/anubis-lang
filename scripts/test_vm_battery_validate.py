#!/usr/bin/env python3
from __future__ import annotations

import json
import hashlib
import os
import re
import runpy
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TOOL = ROOT / "scripts/lib/vm_battery_validate.py"
SHA_READER = ROOT / "scripts/lib/read_exact_sha256.py"
VALIDATOR_NAMESPACE = runpy.run_path(str(TOOL))
EXPECTED = tuple(VALIDATOR_NAMESPACE["EXPECTED_GATES"])
# Independent test witness, not a second runtime producer. Any gate identity/order change is a
# deliberate re-audit event that must update this oracle and its digest explicitly.
ORACLE_EXPECTED = (
    "pin-smoke", "cargo-test", "tool-test", "clippy", "build-rel", "language", "turing",
    "security", "stdlib", "shadow", "seal", "dogfood", "effect-sh", "capset-sh", "type-sh",
    "taint-sh", "stdlib-fc", "native-auth", "docs-drift", "walker", "formal", "formal-kernel",
    "correspondence",
)
ORACLE_ROSTER_SHA256 = "c2149a53575e39d2651a79f7240e4a459ed672769eaae252e4c4d1a81961bc25"
ORACLE_RUN_LINES_SHA256 = "f8b981434538cd1fc125d502ac8fc7be9602dc3b5b81a8a29d311c974b917d32"
FIXPOINT = "a" * 64
PIN = "vm/pins/anubis-" + "b" * 12 + "-src-" + "c" * 12
PIN_SHA256 = "d" * 64
PIN_META_SHA256 = "e" * 64


def clean_log() -> str:
    lines: list[str] = []
    for name in ORACLE_EXPECTED:
        lines.append(f"noise for {name}")
        if name == "seal":
            lines.append(
                f"binary_fixpoint sha256 (LC_UUID + ad-hoc-sig normalized): {FIXPOINT}"
            )
    return "\n".join(lines) + "\n"


def clean_protocol(log_text: str) -> str:
    lines = [
        "ANUBIS_VM_PROTOCOL_V1",
        "ANUBIS_VM_BUILD_JOBS=3",
        f"ANUBIS_VM_SELECTED_PIN {PIN} {PIN_SHA256} {PIN_META_SHA256}",
    ]
    for name in ORACLE_EXPECTED:
        lines.append(f"ANUBIS_VM_GATE_BEGIN {name}")
        if name == "seal":
            lines.append(f"ANUBIS_VM_SEAL_FIXPOINT {FIXPOINT}")
        lines.append(f"ANUBIS_VM_GATE_RESULT 0 {name}")
    raw = log_text.encode("utf-8")
    lines.append(f"ANUBIS_VM_LOG_SHA256 {hashlib.sha256(raw).hexdigest()} {len(raw)}")
    lines.append("ANUBIS_VM_BATTERY_DONE")
    return "\n".join(lines) + "\n"


class VmBatteryValidateTests(unittest.TestCase):
    def validate(
        self, text: str, protocol_text: str | None = None
    ) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as tmp:
            log = Path(tmp) / "battery.log"
            protocol = Path(tmp) / "battery.protocol"
            output = Path(tmp) / "verdict.json"
            log.write_text(text)
            protocol.write_text(protocol_text if protocol_text is not None else clean_protocol(text))
            result = subprocess.run(
                [
                    "python3", str(TOOL), "--log", str(log), "--protocol", str(protocol),
                    "--out", str(output), "--expected-fixpoint", FIXPOINT, "--expected-jobs", "3",
                    "--expected-pin", PIN, "--expected-pin-sha256", PIN_SHA256,
                    "--expected-pin-meta-sha256", PIN_META_SHA256,
                ],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            if output.exists():
                result.verdict = json.loads(output.read_text())  # type: ignore[attr-defined]
            return result

    def test_clean_exact_order_is_pass(self) -> None:
        result = self.validate(clean_log())
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.verdict["verdict"], "PASS")  # type: ignore[attr-defined]
        self.assertEqual(result.verdict["observed_gates"], list(EXPECTED))  # type: ignore[attr-defined]

    def test_production_roster_matches_independent_oracle_and_rejects_mutations(self) -> None:
        self.assertEqual(EXPECTED, ORACLE_EXPECTED)
        digest = hashlib.sha256(b"\0".join(x.encode("ascii") for x in EXPECTED)).hexdigest()
        self.assertEqual(digest, ORACLE_ROSTER_SHA256)
        roster_errors = VALIDATOR_NAMESPACE.get("expected_gate_roster_errors")
        if roster_errors is None:
            self.fail("production expected_gate_roster_errors() is missing")
        self.assertEqual(roster_errors(EXPECTED), [])
        poisoned = [
            (),
            EXPECTED[:-1],
            EXPECTED[:-1] + (EXPECTED[-2],),
            ("renamed-gate",) + EXPECTED[1:],
            (EXPECTED[1], EXPECTED[0]) + EXPECTED[2:],
        ]
        for roster in poisoned:
            with self.subTest(roster=roster):
                self.assertTrue(roster_errors(roster))

    def test_wrapper_protocol_is_authoritative_and_child_marker_spoof_is_ignored(self) -> None:
        child_log = clean_log() + "ANUBIS_VM_GATE_RESULT 0 surprise\n"
        protocol = clean_protocol(child_log)
        with tempfile.TemporaryDirectory() as tmp:
            log = Path(tmp) / "battery.log"
            protocol_path = Path(tmp) / "battery.protocol"
            output = Path(tmp) / "verdict.json"
            log.write_text(child_log)
            protocol_path.write_text(protocol)
            result = subprocess.run(
                [
                    "python3", str(TOOL), "--log", str(log), "--protocol", str(protocol_path),
                    "--out", str(output), "--expected-fixpoint", FIXPOINT, "--expected-jobs", "3",
                    "--expected-pin", PIN, "--expected-pin-sha256", PIN_SHA256,
                    "--expected-pin-meta-sha256", PIN_META_SHA256,
                ],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            verdict = json.loads(output.read_text())
            self.assertEqual(verdict["verdict"], "PASS")
            self.assertEqual(verdict["observed_gates"], list(ORACLE_EXPECTED))
            self.assertEqual(verdict["protocol_log_sha256"], hashlib.sha256(protocol.encode()).hexdigest())

    def assert_rejected(
        self, protocol_text: str, needle: str, log_text: str | None = None
    ) -> None:
        result = self.validate(log_text if log_text is not None else clean_log(), protocol_text)
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn(needle, "\n".join(result.verdict["errors"]))  # type: ignore[attr-defined]

    def test_nested_gate_separator_does_not_impersonate_protocol_marker(self) -> None:
        text = clean_log().replace("noise for cargo-test", "===== nested summary =====\nnoise for cargo-test")
        result = self.validate(text)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_duplicate_result_rejected(self) -> None:
        log = clean_log()
        protocol = clean_protocol(log).replace(
            "ANUBIS_VM_BATTERY_DONE",
            "ANUBIS_VM_GATE_RESULT 0 cargo-test\nANUBIS_VM_BATTERY_DONE",
        )
        self.assert_rejected(protocol, "duplicate")

    def test_unknown_gate_rejected(self) -> None:
        log = clean_log()
        protocol = clean_protocol(log).replace(
            "ANUBIS_VM_BATTERY_DONE",
            "ANUBIS_VM_GATE_BEGIN surprise\nANUBIS_VM_GATE_RESULT 0 surprise\nANUBIS_VM_BATTERY_DONE",
        )
        self.assert_rejected(protocol, "unknown")

    def test_out_of_order_gate_rejected(self) -> None:
        log = clean_log()
        protocol = clean_protocol(log).replace(
            "ANUBIS_VM_GATE_BEGIN cargo-test", "ANUBIS_VM_GATE_BEGIN tool-test", 1
        )
        self.assert_rejected(protocol, "order")

    def test_missing_done_rejected(self) -> None:
        log = clean_log()
        self.assert_rejected(clean_protocol(log).replace("ANUBIS_VM_BATTERY_DONE\n", ""), "BATTERY_DONE")

    def test_duplicate_done_rejected(self) -> None:
        log = clean_log()
        self.assert_rejected(clean_protocol(log) + "ANUBIS_VM_BATTERY_DONE\n", "BATTERY_DONE")

    def test_malformed_exit_marker_rejected(self) -> None:
        log = clean_log()
        protocol = clean_protocol(log).replace(
            "ANUBIS_VM_GATE_RESULT 0 cargo-test", "ANUBIS_VM_GATE_RESULT oops cargo-test"
        )
        self.assert_rejected(protocol, "malformed")

    def test_nonzero_exit_rejected(self) -> None:
        log = clean_log()
        protocol = clean_protocol(log).replace(
            "ANUBIS_VM_GATE_RESULT 0 walker", "ANUBIS_VM_GATE_RESULT 7 walker"
        )
        self.assert_rejected(protocol, "nonzero")

    def test_stale_preamble_result_rejected(self) -> None:
        log = clean_log()
        protocol = clean_protocol(log).replace(
            "ANUBIS_VM_BUILD_JOBS=3\n",
            "ANUBIS_VM_BUILD_JOBS=3\nANUBIS_VM_GATE_RESULT 0 cargo-test\n",
            1,
        )
        self.assert_rejected(protocol, "before header")

    def test_child_log_hash_binding_rejects_post_protocol_tamper(self) -> None:
        log = clean_log()
        self.assert_rejected(clean_protocol(log), "binding mismatch", log + "tamper\n")

    def test_fatal_rerun_does_not_leave_stale_pass_verdict(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            log = root / "battery.log"
            protocol = root / "battery.protocol"
            output = root / "verdict.json"
            log_text = clean_log()
            log.write_text(log_text)
            protocol.write_text(clean_protocol(log_text))
            command = [
                "python3", str(TOOL), "--log", str(log), "--protocol", str(protocol),
                "--out", str(output), "--expected-fixpoint", FIXPOINT, "--expected-jobs", "3",
                "--expected-pin", PIN, "--expected-pin-sha256", PIN_SHA256,
                "--expected-pin-meta-sha256", PIN_META_SHA256,
            ]
            first = subprocess.run(command, text=True, capture_output=True, check=False)
            self.assertEqual(first.returncode, 0, first.stdout + first.stderr)
            self.assertEqual(json.loads(output.read_text())["verdict"], "PASS")
            log.unlink()
            log.symlink_to(root / "outside-log")
            second = subprocess.run(command, text=True, capture_output=True, check=False)
            self.assertNotEqual(second.returncode, 0, second.stdout + second.stderr)
            if output.exists():
                self.assertNotEqual(json.loads(output.read_text()).get("verdict"), "PASS")

    def test_argument_parse_failure_does_not_leave_stale_pass_verdict(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            log = root / "battery.log"
            protocol = root / "battery.protocol"
            output = root / "verdict.json"
            log_text = clean_log()
            log.write_text(log_text)
            protocol.write_text(clean_protocol(log_text))
            command = [
                "python3", str(TOOL), "--log", str(log), "--protocol", str(protocol),
                "--out", str(output), "--expected-fixpoint", FIXPOINT,
                "--expected-pin", PIN, "--expected-pin-sha256", PIN_SHA256,
                "--expected-pin-meta-sha256", PIN_META_SHA256, "--expected-jobs", "3",
            ]
            first = subprocess.run(command, text=True, capture_output=True, check=False)
            self.assertEqual(first.returncode, 0, first.stdout + first.stderr)
            self.assertEqual(json.loads(output.read_text())["verdict"], "PASS")
            malformed = [*command[:-1], "not-an-int"]
            second = subprocess.run(malformed, text=True, capture_output=True, check=False)
            self.assertNotEqual(second.returncode, 0, second.stdout + second.stderr)
            self.assertFalse(output.exists(), "argument-parse failure left a stale verdict")

    def test_seal_fixpoint_outside_active_seal_gate_rejected(self) -> None:
        log = clean_log()
        marker = f"ANUBIS_VM_SEAL_FIXPOINT {FIXPOINT}\n"
        protocol = clean_protocol(log).replace(marker, "", 1).replace(
            "ANUBIS_VM_GATE_BEGIN cargo-test\n",
            "ANUBIS_VM_GATE_BEGIN cargo-test\n" + marker,
            1,
        )
        self.assert_rejected(protocol, "active gate")

    def test_selected_pin_identity_is_exact_and_preamble_only(self) -> None:
        log = clean_log()
        marker = f"ANUBIS_VM_SELECTED_PIN {PIN} {PIN_SHA256} {PIN_META_SHA256}"
        mutations = {
            "wrong path": marker.replace(PIN, PIN + "-release"),
            "wrong binary digest": marker.replace(PIN_SHA256, "0" * 64),
            "wrong metadata digest": marker.replace(PIN_META_SHA256, "1" * 64),
            "malformed": marker + " trailing",
        }
        for label, replacement in mutations.items():
            with self.subTest(label=label):
                protocol = clean_protocol(log).replace(marker, replacement, 1)
                self.assert_rejected(protocol, "selected-pin")
        duplicate = clean_protocol(log).replace(marker, marker + "\n" + marker, 1)
        self.assert_rejected(duplicate, "selected-pin")

    def test_run_slice_uses_strict_validator_not_remote_grep_heuristics(self) -> None:
        source = (ROOT / "scripts/vm/run-slice.sh").read_text()
        self.assertIn("scripts/lib/vm_battery_validate.py", source)
        self.assertNotIn("RAN=$(ssh", source)
        self.assertNotIn("DONE_MARK=$(ssh", source)
        self.assertNotIn("NFAIL=$(ssh", source)

    def test_run_slice_remote_gate_order_matches_validator_authority(self) -> None:
        source = (ROOT / "scripts/vm/run-slice.sh").read_text()
        self.assertEqual(source.count("<<'REMOTE'"), 1)
        self.assertEqual(len(re.findall(r"^REMOTE$", source, re.M)), 1)
        remote = source.split("<<'REMOTE'", 1)[1].split("\nREMOTE", 1)[0]
        run_lines = tuple(
            line.strip() for line in remote.splitlines() if re.match(r"^run\s+[a-z0-9-]+\s+", line)
        )
        runs = tuple(line.split()[1] for line in run_lines)
        self.assertEqual(runs, ORACLE_EXPECTED)
        digest = hashlib.sha256("\0".join(run_lines).encode()).hexdigest()
        self.assertEqual(digest, ORACLE_RUN_LINES_SHA256)

    def test_run_slice_uses_wrapper_only_protocol_and_binds_validator_rc(self) -> None:
        source = (ROOT / "scripts/vm/run-slice.sh").read_text()
        required = (
            'PROTOCOL_LOG="$HOST_EVIDENCE_DIR/battery.protocol"',
            'PROTOCOL_TMP="$(mktemp "$HOME/.battery.protocol.XXXXXX")"',
            'exec 3>>"$PROTOCOL_TMP"',
            'exec 4<"$PROTOCOL_TMP"',
            'rm -f "$PROTOCOL_TMP"',
            '>&3 || exit 125',
            '3>&- 4>&-',
            'cat <&4 || exit 125',
            'ANUBIS_VM_PROTOCOL_V1',
            'ANUBIS_VM_SELECTED_PIN',
            'ANUBIS_VM_SEAL_FIXPOINT',
            'ANUBIS_VM_LOG_SHA256',
            'remote_battery.stderr',
            'protocol_transport_exit_code.txt',
            '--protocol "$PROTOCOL_LOG"',
            '--expected-pin "$CURRENT_PIN"',
            '--expected-pin-sha256 "$CURRENT_PIN_SHA256"',
            '--expected-pin-meta-sha256 "$CURRENT_PIN_META_SHA256"',
            'if [[ $VALIDATOR_RC -ne 0 ]]; then',
        )
        for token in required:
            with self.subTest(token=token):
                self.assertIn(token, source)
        self.assertRegex(
            source,
            r'python3 "\$REPO/scripts/lib/vm_battery_validate\.py"\s*\\\n'
            r'\s*--log "\$BATTERY_LOG"\s*\\\n'
            r'\s*--protocol "\$PROTOCOL_LOG"\s*\\\n'
            r'\s*--out "\$HOST_EVIDENCE_DIR/battery_verdict\.json"',
        )
        self.assertRegex(
            source,
            r'(?s)if \[\[ \$VALIDATOR_RC -ne 0 \]\]; then.*?rc=1\s*\n\s*fi',
        )

    def test_run_slice_protocol_never_enters_child_writable_guest_namespace(self) -> None:
        source = (ROOT / "scripts/vm/run-slice.sh").read_text()
        remote = source.split("<<'REMOTE'", 1)[1].split("\nREMOTE", 1)[0]
        self.assertNotIn('PROTOCOL="$HOME/battery.protocol"', remote)
        self.assertNotIn('cat <&4 > "$PROTOCOL"', remote)
        self.assertNotRegex(source, r'scp[^\n]*:battery\.protocol')
        self.assertIn('cat <&4 || exit 125', remote)
        self.assertRegex(
            source,
            r"(?s)ssh .*?>\"\$PROTOCOL_LOG\"\s+2>\"\$HOST_EVIDENCE_DIR/remote_battery\.stderr\"\s+<<'REMOTE'",
        )

    def test_run_slice_captures_source_epoch_and_launcher_pid(self) -> None:
        source = (ROOT / "scripts/vm/run-slice.sh").read_text()
        self.assertIn("host_source_manifest_before.json", source)
        self.assertIn("host_source_manifest_after.json", source)
        self.assertIn("host_source_manifest_final.json", source)
        self.assertIn("source_tree_sha256", source)
        self.assertIn("TART_RUN_PID=$!", source)
        self.assertIn("tart_run_pid.txt", source)
        self.assertIn("wait \"$TART_RUN_PID\"", source)
        self.assertIn("rm -f \"$REPO/.ammit/cargo-test.json\"", source)
        self.assertIn('bundle_manifest.py" verify --bundle "$HOST_EVIDENCE_DIR"', source)
        self.assertLess(source.index("elif ! cleanup"), source.index("host_source_manifest_final.json"))

    def test_run_slice_parses_commented_expected_fixpoint_file(self) -> None:
        source = (ROOT / "scripts/vm/run-slice.sh").read_text()
        self.assertIn("grep -E '^[0-9a-f]{64}$'", source)
        self.assertIn("EXPECTED_HASH_LINES", source)

    def test_run_slice_consumes_exact_selfhost_fixpoint_artifact_not_human_prose(self) -> None:
        source = (ROOT / "scripts/vm/run-slice.sh").read_text()
        producer = (ROOT / "scripts/run_selfhost_gate.sh").read_text()
        self.assertIn('echo "$bh" >"$OUT/binary_fixpoint.sha256"', producer)
        self.assertIn(
            'fixpoint_file="out/selfhost_gate/binary_fixpoint.sha256"', source
        )
        self.assertIn('python3 scripts/lib/read_exact_sha256.py "$fixpoint_file"', source)
        self.assertNotIn("grep -Ec '^binary_fixpoint sha256", source)

    def test_exact_selfhost_fixpoint_artifact_reader_accepts_only_the_file_contract(self) -> None:
        valid = (FIXPOINT + "\n").encode()
        cases = {
            "valid": (valid, 0),
            "duplicate": (valid + valid, 2),
            "valid_plus_malformed": (valid + b"junk", 2),
            "malformed_only": (b"junk\n", 2),
            "uppercase": (("A" * 64 + "\n").encode(), 2),
            "short": (("a" * 63 + "\n").encode(), 2),
            "long": (("a" * 65 + "\n").encode(), 2),
            "missing_newline": (FIXPOINT.encode(), 2),
        }
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for name, (payload, expected_rc) in cases.items():
                with self.subTest(name=name):
                    artifact = root / name
                    artifact.write_bytes(payload)
                    result = subprocess.run(
                        ["python3", str(SHA_READER), str(artifact)],
                        text=True,
                        capture_output=True,
                        check=False,
                    )
                    self.assertEqual(result.returncode, expected_rc, result.stdout + result.stderr)
                    if expected_rc == 0:
                        self.assertEqual(result.stdout, FIXPOINT + "\n")
                    else:
                        self.assertEqual(result.stdout, "")

            target = root / "target"
            target.write_bytes(valid)
            alias = root / "alias"
            alias.symlink_to(target)
            result = subprocess.run(
                ["python3", str(SHA_READER), str(alias)],
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
            self.assertEqual(result.stdout, "")

            fifo = root / "fifo"
            os.mkfifo(fifo)
            result = subprocess.run(
                ["python3", str(SHA_READER), str(fifo)],
                text=True,
                capture_output=True,
                check=False,
                timeout=2,
            )
            self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
            self.assertEqual(result.stdout, "")

    def test_run_slice_keeps_bsd_tools_ahead_of_gnubin_and_binds_integer_log_size(self) -> None:
        source = (ROOT / "scripts/vm/run-slice.sh").read_text()
        remote = source.split("<<'REMOTE'", 1)[1].split("\nREMOTE", 1)[0]
        path_assignment = (
            "export PATH=/usr/bin:/bin:/usr/sbin:/sbin:"
            "/opt/homebrew/opt/coreutils/libexec/gnubin:/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
        )
        self.assertIn(path_assignment, remote)
        self.assertNotIn(
            "export PATH=/opt/homebrew/opt/coreutils/libexec/gnubin:/opt/homebrew/bin:$PATH",
            remote,
        )
        self.assertIn('LOG_BYTES="$(/usr/bin/stat -f \'%z\' "$LOG")"', remote)
        self.assertIn('STAT_BIN="$(command -v stat', remote)
        self.assertIn('[ "$STAT_BIN" = /usr/bin/stat ]', remote)
        self.assertIn(
            '[ "$TIMEOUT_BIN" = /opt/homebrew/opt/coreutils/libexec/gnubin/timeout ]',
            remote,
        )
        self.assertIn("GNU coreutils", remote)

        with tempfile.TemporaryDirectory() as tmp:
            fake_bin = Path(tmp) / "gnubin"
            fake_bin.mkdir()
            fake_stat = fake_bin / "stat"
            fake_stat.write_text("#!/bin/sh\necho fake-gnu-stat\n")
            fake_stat.chmod(0o755)
            result = subprocess.run(
                [
                    "/bin/bash",
                    "-c",
                    f"{path_assignment}; command -v stat; command -v timeout; timeout --version",
                ],
                text=True,
                capture_output=True,
                check=False,
                env={"PATH": str(fake_bin), "HOME": tmp},
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            resolved = result.stdout.splitlines()
            self.assertEqual(resolved[0], "/usr/bin/stat")
            self.assertEqual(
                resolved[1], "/opt/homebrew/opt/coreutils/libexec/gnubin/timeout"
            )
            self.assertTrue(any("GNU coreutils" in line for line in resolved[2:]))

    def test_duplicate_fixpoint_rejected(self) -> None:
        log = clean_log()
        marker = f"ANUBIS_VM_SEAL_FIXPOINT {FIXPOINT}"
        protocol = clean_protocol(log).replace(marker, f"{marker}\n{marker}", 1)
        self.assert_rejected(protocol, "fixpoint")


if __name__ == "__main__":
    unittest.main(verbosity=2)
