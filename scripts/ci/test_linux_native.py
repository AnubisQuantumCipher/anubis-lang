"""Harmless receipt/control tests: no Cargo, Anubis, VM, or crash payloads."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("linux_native", Path(__file__).with_name("linux_native.py"))
ci = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ci)


def events(name):
    return [{"type": "suite", "event": "started", "test_count": 1},
            {"type": "test", "event": "started", "name": name},
            {"type": "test", "event": "ok", "name": name},
            {"type": "suite", "event": "ok", "passed": 1, "failed": 0, "ignored": 0, "measured": 0}]


class NativeControls(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.out = Path(self.temp.name)
        self.manifest = json.loads(ci.MANIFEST.read_text())
        self.limits = {"memory.max": "6000000000", "memory.swap.max": "0",
                       "cpu.max": "200000 100000", "pids.max": "512"}

    def log(self, name, stream):
        path = self.out / name
        path.write_text("".join(json.dumps(e) + "\n" for e in stream))
        return ci.digest(path)

    def receipt(self):
        artifacts = {name: {"path": f"/synthetic/{name}", "sha256": "a" * 64,
                           "elf": {"class": "ELF64", "byte_order": "little", "machine": "aarch64"}}
                     for name in ["anubis", *(t["target"] for t in self.manifest["targets"]) ]}
        commands = []
        names = ["harness-controls", "build-cli", *(f"compile-{p}" for p in
                 dict.fromkeys(t["package"] for t in self.manifest["targets"]))]
        for name in names:
            commands.append({"name": name, "argv": [], "return_code": 0,
                             "signal": None, "timed_out": False,
                             "logs": {name + ".stdout.log": self.log(name + ".stdout.log", []),
                                      name + ".stderr.log": self.log(name + ".stderr.log", [])}})
        commands[0]["argv"] = ci.CONTROL_ARGV
        control_log = self.out / "harness-controls.stderr.log"
        control_log.write_text("Ran 1 test in 0.001s\n\nOK\n")
        commands[0]["logs"][control_log.name] = ci.digest(control_log)
        for name, argv in ci.build_commands(self.manifest):
            wanted = ["anubis"] if name == "build-cli" else [
                t["target"] for t in self.manifest["targets"] if name == f"compile-{t['package']}"]
            stream = [{"reason": "compiler-artifact", "target": {
                "name": n, "kind": ["bin"] if name == "build-cli" else ["test"]},
                "profile": {"test": name != "build-cli"}, "executable": artifacts[n]["path"]} for n in wanted]
            stream.append({"reason": "build-finished", "success": True})
            command = next(c for c in commands if c["name"] == name)
            command["argv"] = argv
            command["logs"][name + ".stdout.log"] = self.log(name + ".stdout.log", stream)
        for target in self.manifest["targets"]:
            for test in target["tests"]:
                name = f"test-{target['target']}-{test}"
                commands.append({"name": name,
                                 "argv": [artifacts[target["target"]]["path"], test, "--exact",
                                          "--test-threads=1", "-Z", "unstable-options", "--format=json"],
                                 "return_code": 0, "signal": None, "timed_out": False,
                                 "logs": {name + ".stdout.log": self.log(name + ".stdout.log", events(test)),
                                          name + ".stderr.log": self.log(name + ".stderr.log", [])}})
        channel = ci.tomllib.loads((ci.ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
        source = {"commit": "b" * 40, "tree": "c" * 40, "files": {
            name: {"sha256": "d" * 64} for name in
            ["Cargo.lock", "rust-toolchain.toml", "scripts/ci/linux_native.py"]}}
        receipt = {"schema": ci.SCHEMA, "verdict": ci.CLAIM, "external": ci.EXTERNAL,
                   "tests": ci.classified_tests(ci.ROOT, self.manifest),
                   "manifest_sha256": ci.digest(ci.MANIFEST),
                   "expected_arch": "aarch64", "expected_sha": "b" * 40,
                   "environment": {"machine": "aarch64", "system": "Linux", "channel": channel,
                                   "target": ci.TRIPLES["aarch64"], "rustc": "host: " + ci.TRIPLES["aarch64"],
                                   "cargo": "cargo synthetic", "z3": "Z3 version 4.15.4 synthetic",
                                   "z3_binary_sha256": "a" * 64, "features": "default",
                                   "rust_min_stack": "unset", "tmp_fstype": "ext4",
                                   "payload_env": {"RUSTUP_TOOLCHAIN": channel}},
                   "source_before": source, "source_after": copy.deepcopy(source),
                   "artifacts": artifacts, "artifacts_after": copy.deepcopy(artifacts),
                   "artifacts_unchanged": True, "commands": commands,
                   "memory_events_before": "low 0\nhigh 0\nmax 0\noom 0\noom_kill 0\n",
                   "memory_events_after": "low 0\nhigh 0\nmax 0\noom 0\noom_kill 0\n",
                   "scope": {"unit": "synthetic.service", "limits": self.limits}}
        ci.save(self.out / "launcher.json", {"unit": "synthetic.service", "exit_code": 0,
                "unit_removed": True, "load_state_after": "not-found", "query_exit_code": 0,
                "launch_exit_code": "0", "cleanup_required": False, "validation_exit_code": 0})
        return receipt

    def check(self, receipt):
        ci.save(self.out / "receipt.json", receipt)
        ci.validate(self.out, "b" * 40, "aarch64")

    def test_synthetic_complete_receipt(self):
        self.check(self.receipt())

    def test_final_source_failure_cannot_leave_a_pass_receipt(self):
        receipt = self.receipt()
        receipt["verdict"] = ci.PAYLOAD_COMPLETE
        ci.save(self.out / "receipt.json", receipt)
        argv = ["linux_native.py", "finalize", "--out", str(self.out), "--sha", "b" * 40,
                "--arch", "aarch64"]
        with patch.object(sys, "argv", argv), patch.object(ci, "source_identity", return_value={}):
            with self.assertRaises(RuntimeError):
                ci.main()
        self.assertEqual(json.loads((self.out / "receipt.json").read_text())["verdict"], "FAIL")
        self.assertEqual(json.loads((self.out / "launcher.json").read_text())["validation_exit_code"], 1)

    def test_unfinalized_payload_cannot_validate_as_pass(self):
        receipt = self.receipt()
        receipt["verdict"] = ci.PAYLOAD_COMPLETE
        with self.assertRaises(RuntimeError):
            self.check(receipt)

    def test_rejects_failed_signal_timeout_and_missing_command(self):
        original = self.receipt()
        for field, value in [("return_code", 1), ("return_code", -6), ("signal", 6), ("timed_out", True)]:
            with self.subTest(field=field, value=value):
                receipt = copy.deepcopy(original)
                receipt["commands"][-1][field] = value
                with self.assertRaises(RuntimeError):
                    self.check(receipt)
        original["commands"].pop()
        with self.assertRaises(RuntimeError):
            self.check(original)

    def test_rejects_scope_arch_source_and_instrument_substitution(self):
        original = self.receipt()
        poisons = [lambda r: r["environment"].update(machine="x86_64"),
                   lambda r: r["source_after"].update(commit="c" * 40),
                   lambda r: r["artifacts_after"]["anubis"].update(sha256="d" * 64),
                   lambda r: r["scope"]["limits"].update({"memory.max": "max"}),
                   lambda r: r.update(expected_sha="c" * 40),
                   lambda r: r["commands"][1].update(argv=["true"]),
                   lambda r: r.update(memory_events_after="oom 1\n"),
                   lambda r: r["environment"].update(channel="other"),
                   lambda r: r["environment"].update(z3="other"),
                   lambda r: r["artifacts"]["anubis"]["elf"].update(machine="x86_64"),
                   lambda r: r.update(external=[]),
                   lambda r: r["commands"][-1]["argv"].remove("--exact")]
        for poison in poisons:
            receipt = copy.deepcopy(original)
            poison(receipt)
            with self.assertRaises(RuntimeError):
                self.check(receipt)

    def test_rejects_cargo_log_without_build_or_wrong_executable(self):
        receipt = self.receipt()
        record = receipt["commands"][1]
        for stream in [[], [{"reason": "build-finished", "success": True}]]:
            record["logs"]["build-cli.stdout.log"] = self.log("build-cli.stdout.log", stream)
            with self.assertRaises(RuntimeError):
                self.check(receipt)

    def test_rejects_missing_teardown_and_failed_launcher(self):
        receipt = self.receipt()
        launch = json.loads((self.out / "launcher.json").read_text())
        del launch["validation_exit_code"]
        ci.save(self.out / "launcher.json", launch)
        with self.assertRaises(RuntimeError):
            self.check(receipt)
        (self.out / "launcher.json").unlink()
        with self.assertRaises(RuntimeError):
            self.check(receipt)
        ci.save(self.out / "launcher.json", {"exit_code": 1, "unit_removed": False})
        with self.assertRaises(RuntimeError):
            self.check(receipt)

    def test_rejects_changed_or_missing_log(self):
        receipt = self.receipt()
        log = self.out / next(iter(receipt["commands"][-1]["logs"]))
        log.write_text("truncated\n")
        with self.assertRaises(RuntimeError):
            self.check(receipt)
        log.unlink()
        with self.assertRaises(RuntimeError):
            self.check(receipt)

    def test_rejects_vacuous_duplicate_and_wrong_named_test(self):
        for stream in [[], events("expected") + events("expected"), events("other"),
                       events("expected")[:-1]]:
            self.log("test.log", stream)
            with self.assertRaises(RuntimeError):
                ci.test_result(self.out / "test.log", "expected")

    def test_changed_source_requires_reclassification(self):
        target = self.manifest["targets"][0]
        path = self.out / target["source"]
        path.parent.mkdir(parents=True)
        path.write_text("// changed test source\n")
        with self.assertRaises(RuntimeError):
            ci.classified_tests(self.out, {**self.manifest, "targets": [target]})
        with self.assertRaises(RuntimeError):
            ci.classified_tests(ci.ROOT, {**self.manifest, "targets": []})

    def test_elf_architecture_must_match(self):
        instrument = self.out / "synthetic-elf"
        # Header bytes only: this file is never executed.
        instrument.write_bytes(b"\x7fELF\x02\x01" + bytes(12) + (183).to_bytes(2, "little"))
        self.assertEqual(ci.elf_identity(instrument, "aarch64")["machine"], "aarch64")
        with self.assertRaises(RuntimeError):
            ci.elf_identity(instrument, "x86_64")
        instrument.write_bytes(b"not-an-elf")
        with self.assertRaises(RuntimeError):
            ci.elf_identity(instrument, "aarch64")

    def test_scope_membership_and_caps_verified_before_payload(self):
        proc = self.out / "proc"
        proc.write_text("0::/system.slice/synthetic.service\n")
        scope = self.out / "cgroup/system.slice/synthetic.service"
        scope.mkdir(parents=True)
        for name, value in self.limits.items():
            (scope / name).write_text(value)
        self.assertEqual(ci.scope_info("synthetic.service", proc, self.out / "cgroup")[0], scope)
        for key, value in [("memory.max", "max"), ("memory.swap.max", "max"),
                           ("cpu.max", "max 100000"), ("pids.max", "max")]:
            (scope / key).write_text(value)
            with self.assertRaises(RuntimeError):
                ci.scope_info("synthetic.service", proc, self.out / "cgroup")
            (scope / key).write_text(self.limits[key])
        with self.assertRaises(RuntimeError):
            ci.scope_info("wrong.service", proc, self.out / "cgroup")

    def test_remaining_descendants_refuse_success(self):
        (self.out / "cgroup.procs").write_text(str(os.getpid()))
        ci.quiescent(self.out)
        (self.out / "cgroup.procs").write_text(f"{os.getpid()}\n99999999\n")
        with self.assertRaises(RuntimeError):
            ci.quiescent(self.out)

    def test_missing_scope_stops_before_command(self):
        args = type("Args", (), {"out": self.out, "arch": "aarch64", "sha": "b" * 40,
                                "unit": "synthetic.service"})()
        with patch.object(ci, "scope_info", side_effect=RuntimeError("missing scope")), \
                patch.object(ci.Driver, "command") as command:
            self.assertEqual(ci.Driver(args).run(), 1)
            command.assert_not_called()

    def test_launcher_preserves_failed_launch_and_failed_teardown_query(self):
        # All external service operations are harmless shell stubs. No sudo/service is invoked.
        scripts = self.out / "repo/scripts/ci"
        scripts.mkdir(parents=True)
        launcher = scripts / "linux_scope.sh"
        shutil.copyfile(Path(__file__).with_name("linux_scope.sh"), launcher)
        stubs = self.out / "stubs"
        stubs.mkdir()
        for name, body in {"sudo": "echo synthetic-launch-failure >&2; exit 17",
                           "systemctl": 'if [ ! -f "$MOCK_ADMITTED" ]; then touch "$MOCK_ADMITTED"; echo not-found; exit 0; fi; echo synthetic-query-failure >&2; exit 23'}.items():
            path = stubs / name
            path.write_text("#!/bin/sh\n" + body + "\n")
            path.chmod(0o755)
        env = {**os.environ, "PATH": str(stubs) + os.pathsep + os.environ["PATH"],
               "GITHUB_RUN_ID": "synthetic", "GITHUB_RUN_ATTEMPT": "1", "RUNNER_ARCH": "ARM64",
               "RUNNER_TEMP": str(self.out / "runner"), "GITHUB_SHA": "b" * 40,
               "MOCK_ADMITTED": str(self.out / "admitted")}
        result = subprocess.run(["bash", str(launcher), "aarch64"], env=env,
                                capture_output=True, timeout=15)
        self.assertNotEqual(result.returncode, 0)
        evidence = self.out / "repo/out/linux-native"
        receipt = json.loads((evidence / "launcher.json").read_text())
        self.assertEqual(receipt["exit_code"], 17)
        self.assertEqual(receipt["launch_exit_code"], "17")
        self.assertEqual(receipt["query_exit_code"], 23)
        self.assertFalse(receipt["unit_removed"])
        self.assertIn("synthetic-launch-failure", (evidence / "launcher.stderr.log").read_text())
        self.assertIn("synthetic-query-failure", (evidence / "teardown.stderr.log").read_text())

    def test_launcher_cancellation_cleans_only_its_owned_service(self):
        scripts = self.out / "repo/scripts/ci"
        scripts.mkdir(parents=True)
        launcher = scripts / "linux_scope.sh"
        shutil.copyfile(Path(__file__).with_name("linux_scope.sh"), launcher)
        stubs = self.out / "stubs"
        stubs.mkdir()
        bodies = {
            "sudo": 'if [ "$1" = systemd-run ]; then touch "$MOCK_ACTIVE"; kill -TERM "$PPID"; exit 143; fi; if [ "$1" = systemctl ] && [ "$2" = stop ]; then printf "%s\\n" "$3" > "$MOCK_STOPPED"; rm "$MOCK_ACTIVE"; exit 0; fi; exit 99',
            "systemctl": 'if [ -f "$MOCK_ACTIVE" ]; then echo loaded; else echo not-found; fi',
        }
        for name, body in bodies.items():
            path = stubs / name
            path.write_text("#!/bin/sh\n" + body + "\n")
            path.chmod(0o755)
        env = {**os.environ, "PATH": str(stubs) + os.pathsep + os.environ["PATH"],
               "GITHUB_RUN_ID": "cancel", "GITHUB_RUN_ATTEMPT": "1", "RUNNER_ARCH": "ARM64",
               "RUNNER_TEMP": str(self.out / "runner"), "GITHUB_SHA": "b" * 40,
               "MOCK_ACTIVE": str(self.out / "active"), "MOCK_STOPPED": str(self.out / "stopped")}
        result = subprocess.run(["bash", str(launcher), "aarch64"], env=env,
                                capture_output=True, timeout=15)
        self.assertNotEqual(result.returncode, 0)
        receipt = json.loads((self.out / "repo/out/linux-native/launcher.json").read_text())
        self.assertEqual(receipt["exit_code"], 143)
        self.assertTrue(receipt["cleanup_required"])
        self.assertTrue(receipt["unit_removed"])
        self.assertEqual((self.out / "stopped").read_text().strip(), receipt["unit"])
        self.assertFalse((self.out / "active").exists())


if __name__ == "__main__":
    unittest.main()
