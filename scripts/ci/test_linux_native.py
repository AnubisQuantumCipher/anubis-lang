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
from unittest.mock import Mock, patch

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

    def resources(self):
        return {"memory_events": "low 0\nhigh 0\nmax 0\noom 0\noom_kill 0\n",
                "memory_peak_bytes": 4096, "errors": []}

    def driver(self):
        args = type("Args", (), {"out": self.out, "arch": "aarch64", "sha": "b" * 40,
                                "unit": "synthetic.service"})()
        driver = ci.Driver(args)
        driver.scope = self.out
        driver.env = {}
        driver.receipt["environment"] = {"native_compilers": {}}
        return driver

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
        compilers = {variable: {"invocation_path": f"/synthetic/{name}",
                     "resolved_path": "/synthetic/clang-native", "sha256": "e" * 64,
                     "version": "Ubuntu clang version synthetic",
                     "elf": {"class": "ELF64", "byte_order": "little", "machine": "aarch64"}}
                     for variable, name in ci.COMPILER_POLICY.items()}
        for command in commands:
            command.update(resources_before=self.resources(), resources_after=self.resources(),
                           secondary_errors=[], timeout_seconds=120 if command["name"] == "harness-controls"
                           or command["name"].startswith("test-") else 3600)
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
                                   "compiler_policy": dict(ci.COMPILER_POLICY), "native_compilers": compilers,
                                   "payload_env": {"RUSTUP_TOOLCHAIN": channel, "CARGO_BUILD_JOBS": "2",
                                                   "RAYON_NUM_THREADS": "2",
                                                   **{v: r["invocation_path"] for v, r in compilers.items()}}},
                   "native_compilers_after": copy.deepcopy(compilers),
                   "source_before": source, "source_after": copy.deepcopy(source),
                   "artifacts": artifacts, "artifacts_after": copy.deepcopy(artifacts),
                   "artifacts_unchanged": True, "commands": commands,
                   "memory_events_before": "low 0\nhigh 0\nmax 0\noom 0\noom_kill 0\n",
                   "memory_events_after": "low 0\nhigh 0\nmax 0\noom 0\noom_kill 0\n",
                   "resources_before": self.resources(), "resources_after": self.resources(),
                   "scope": {"unit": "synthetic.service", "limits": self.limits}}
        ci.save(self.out / "launcher.json", {"unit": "synthetic.service", "exit_code": 0,
                "unit_removed": True, "load_state_after": "not-found", "query_exit_code": 0,
                "launch_exit_code": "0", "cleanup_required": False, "validation_exit_code": 0,
                "teardown_errors": []})
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

    def test_final_validation_keeps_existing_payload_error(self):
        receipt = self.receipt()
        receipt.update(verdict="FAIL", error="original payload failure")
        ci.save(self.out / "receipt.json", receipt)
        argv = ["linux_native.py", "finalize", "--out", str(self.out), "--sha", "b" * 40,
                "--arch", "aarch64"]
        with patch.object(sys, "argv", argv):
            with self.assertRaises(RuntimeError):
                ci.main()
        failed = json.loads((self.out / "receipt.json").read_text())
        self.assertEqual(failed["error"], "original payload failure")
        self.assertIn("final validation failed", failed["final_validation_error"])

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

    def test_rejects_compiler_policy_environment_and_identity_substitution(self):
        original = self.receipt()
        poisons = [lambda r: r["environment"]["compiler_policy"].update(CC="gcc"),
                   lambda r: r["environment"]["payload_env"].update(CXX="/synthetic/clang-native"),
                   lambda r: r["environment"]["payload_env"].update(CARGO_BUILD_JOBS="1"),
                   lambda r: r["environment"]["payload_env"].update(RAYON_NUM_THREADS="1"),
                   lambda r: r["environment"]["native_compilers"].pop("CXX"),
                   lambda r: r["environment"]["native_compilers"]["CC"].update(sha256="missing"),
                   lambda r: r["environment"]["native_compilers"]["CC"].update(resolved_path="relative"),
                   lambda r: r["environment"]["native_compilers"]["CC"].update(version="gcc synthetic"),
                   lambda r: r["environment"]["native_compilers"]["CXX"].update(invocation_path="/synthetic/clang"),
                   lambda r: r["environment"]["native_compilers"]["CC"]["elf"].update(machine="x86_64"),
                   lambda r: r["native_compilers_after"]["CC"].update(sha256="f" * 64),
                   lambda r: r.pop("native_compilers_after")]
        for poison in poisons:
            receipt = copy.deepcopy(original)
            poison(receipt)
            with self.assertRaises(RuntimeError):
                self.check(receipt)

    def test_compiler_selection_preserves_driver_name_and_detects_changed_symlink(self):
        binary = self.out / "clang-native"
        binary.write_bytes(b"\x7fELF\x02\x01" + bytes(12) + (183).to_bytes(2, "little"))
        binary.chmod(0o755)
        for name in ci.COMPILER_POLICY.values():
            (self.out / name).symlink_to(binary.name)
        env = {"PATH": str(self.out), "CC": "untrusted", "CXX": "untrusted"}
        with patch.object(ci, "output", return_value="Ubuntu clang version synthetic") as version:
            records = ci.select_compilers(env, "aarch64")
        self.assertEqual(env["CXX"], str(self.out / "clang++"))
        self.assertEqual(records["CXX"]["resolved_path"], str(binary))
        self.assertEqual(version.call_args_list[-1].args[0], [str(self.out / "clang++"), "--version"])
        ci.current_compilers(records, "aarch64")
        replacement = self.out / "other-native"
        replacement.write_bytes(binary.read_bytes() + b"different")
        (self.out / "clang++").unlink()
        (self.out / "clang++").symlink_to(replacement.name)
        with self.assertRaises(RuntimeError):
            ci.current_compilers(records, "aarch64")

    def test_rejects_missing_failed_or_changed_resource_snapshots(self):
        original = self.receipt()
        poisons = [lambda r: r.pop("resources_after"),
                   lambda r: r["commands"][0].pop("resources_before"),
                   lambda r: r["commands"][-1]["resources_after"].update(memory_events="max 1\n"),
                   lambda r: r["resources_before"].update(memory_peak_bytes=None),
                   lambda r: r["resources_after"]["errors"].append("unreadable memory.peak"),
                   lambda r: r["commands"][0].update(timeout_seconds=3600),
                   lambda r: r["commands"][0]["secondary_errors"].append("residual process")]
        for poison in poisons:
            receipt = copy.deepcopy(original)
            poison(receipt)
            with self.assertRaises(RuntimeError):
                self.check(receipt)

    def test_resource_snapshot_keeps_events_when_peak_read_fails(self):
        (self.out / "memory.events").write_text("max 1\noom 0\n")
        snapshot = ci.resource_snapshot(self.out)
        self.assertEqual(snapshot["memory_events"], "max 1\noom 0\n")
        self.assertIsNone(snapshot["memory_peak_bytes"])
        self.assertIn("memory.peak", snapshot["errors"][0])

    def test_failing_command_preserves_primary_with_residual_and_snapshot_errors(self):
        driver = self.driver()
        child = Mock()
        child.wait.return_value = 17
        failed_snapshot = {**self.resources(), "memory_peak_bytes": None, "errors": ["memory.peak unavailable"]}
        with patch.object(ci.subprocess, "Popen", return_value=child), \
                patch.object(ci, "current_compilers", return_value={}), \
                patch.object(ci, "resource_snapshot", side_effect=[self.resources(), failed_snapshot]), \
                patch.object(ci, "quiescent", side_effect=[None, RuntimeError("residual")]), \
                patch.object(ci, "residual_processes", return_value={"pids": [{"pid": 99}]}):
            with self.assertRaisesRegex(RuntimeError, "build-cli failed: exit=17"):
                driver.command("build-cli", ["synthetic"], 3600)
        record = driver.receipt["commands"][0]
        self.assertEqual(record["error"], "build-cli failed: exit=17")
        self.assertIn("memory.peak unavailable", record["secondary_errors"])
        self.assertTrue(any("residual" in e for e in record["secondary_errors"]))
        self.assertEqual(record["resources_before"], self.resources())
        self.assertEqual(record["resources_after"], failed_snapshot)

    def test_timeout_kills_then_waits_and_drains_with_primary_error_preserved(self):
        for leader_exits in [True, False]:
            with self.subTest(leader_exits=leader_exits):
                driver = self.driver()
                order = []
                child = Mock(pid=123)
                child.poll.return_value = None

                def wait(timeout):
                    order.append(("wait", timeout))
                    if len([entry for entry in order if entry[0] == "wait"]) == 1 or not leader_exits:
                        raise subprocess.TimeoutExpired(["synthetic"], timeout)
                    return -9

                child.wait.side_effect = wait
                def drain(scope, deadline):
                    order.append(("drain", deadline))
                    return {"quiescent": False, "pids": [os.getpid(), 123]}

                with patch.object(ci.subprocess, "Popen", return_value=child), \
                        patch.object(ci.os, "killpg", side_effect=lambda *args: order.append(("kill", args))), \
                        patch.object(ci, "current_compilers", return_value={}), \
                        patch.object(ci, "resource_snapshot", side_effect=lambda scope: self.resources()), \
                        patch.object(ci, "quiescent", side_effect=[None, RuntimeError("still draining")]), \
                        patch.object(ci, "drain_scope", side_effect=drain), \
                        patch.object(ci, "residual_processes", return_value={"pids": [{"pid": 123}]}):
                    with self.assertRaisesRegex(RuntimeError, "build-cli timed out after 3600s"):
                        driver.command("build-cli", ["synthetic"], 3600)
                self.assertEqual([entry[0] for entry in order], ["wait", "kill", "wait", "drain"])
                self.assertLessEqual(order[2][1], ci.EXIT_DRAIN_SECONDS)
                self.assertGreaterEqual(order[2][1], 0)
                record = driver.receipt["commands"][0]
                self.assertTrue(record["timed_out"])
                self.assertFalse(record["exit_drain"]["quiescent"])
                self.assertTrue(record["secondary_errors"])
                self.assertEqual(record["resources_after"], self.resources())

    def test_exit_drain_is_finite_and_can_observe_descendants_finish(self):
        own = {os.getpid()}
        with patch.object(ci, "scope_pids", side_effect=[own | {123}, own]), \
                patch.object(ci.time, "monotonic", return_value=1), patch.object(ci.time, "sleep") as sleep:
            self.assertTrue(ci.drain_scope(self.out, 2)["quiescent"])
            sleep.assert_called_once()
        with patch.object(ci, "scope_pids", return_value=own | {123}), \
                patch.object(ci.time, "monotonic", return_value=2), patch.object(ci.time, "sleep") as sleep:
            self.assertFalse(ci.drain_scope(self.out, 2)["quiescent"])
            sleep.assert_not_called()

    def test_failure_path_captures_resources_without_replacing_primary_error(self):
        driver = self.driver()
        failed = {**self.resources(), "memory_peak_bytes": None, "errors": ["memory.peak missing"]}
        with patch.object(ci, "scope_info", return_value=(self.out, {"unit": "synthetic.service"})), \
                patch.object(ci, "resource_snapshot", side_effect=[self.resources(), failed]), \
                patch.object(ci, "quiescent", side_effect=RuntimeError("primary admission failure")), \
                patch.object(ci, "residual_processes", return_value={"pids": []}), \
                patch.object(ci.Driver, "command") as command:
            self.assertEqual(driver.run(), 1)
        self.assertEqual(driver.receipt["error"], "RuntimeError: primary admission failure")
        self.assertEqual(driver.receipt["resources_failure"], failed)
        self.assertEqual(driver.receipt["memory_events_after"], failed["memory_events"])
        command.assert_not_called()

    def test_missing_command_resource_snapshot_prevents_payload_start(self):
        driver = self.driver()
        failed = {**self.resources(), "memory_peak_bytes": None, "errors": ["memory.peak missing"]}
        with patch.object(ci, "resource_snapshot", return_value=failed), \
                patch.object(ci, "quiescent"), patch.object(ci, "current_compilers", return_value={}), \
                patch.object(ci, "residual_processes", return_value={"pids": []}), \
                patch.object(ci.subprocess, "Popen") as popen:
            with self.assertRaisesRegex(RuntimeError, "resource snapshot failed"):
                driver.command("build-cli", ["synthetic"], 3600)
            popen.assert_not_called()

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
        evidence = self.out / "repo/out/linux-native"
        evidence.mkdir(parents=True)
        ci.save(evidence / "receipt.json", {"verdict": "FAIL", "error": "primary payload timeout"})
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
        receipt = json.loads((evidence / "launcher.json").read_text())
        self.assertEqual(receipt["exit_code"], 17)
        self.assertEqual(receipt["launch_exit_code"], "17")
        self.assertEqual(receipt["query_exit_code"], 23)
        self.assertFalse(receipt["unit_removed"])
        self.assertIn("synthetic-launch-failure", (evidence / "launcher.stderr.log").read_text())
        self.assertIn("synthetic-query-failure", (evidence / "teardown.stderr.log").read_text())
        payload = json.loads((evidence / "receipt.json").read_text())
        self.assertEqual(payload["error"], "primary payload timeout")
        self.assertIn("service collection query failed", payload["launcher_failure"]["teardown_errors"])

    def test_nonzero_service_exit_with_successful_collection_preserves_payload_error(self):
        scripts = self.out / "repo/scripts/ci"
        scripts.mkdir(parents=True)
        launcher = scripts / "linux_scope.sh"
        shutil.copyfile(Path(__file__).with_name("linux_scope.sh"), launcher)
        evidence = self.out / "repo/out/linux-native"
        evidence.mkdir(parents=True)
        ci.save(evidence / "receipt.json", {"verdict": "FAIL", "error": "memory control events changed during run"})
        stubs = self.out / "stubs"
        stubs.mkdir()
        for name, body in {"sudo": "exit 17", "systemctl": "echo not-found"}.items():
            path = stubs / name
            path.write_text("#!/bin/sh\n" + body + "\n")
            path.chmod(0o755)
        env = {**os.environ, "PATH": str(stubs) + os.pathsep + os.environ["PATH"],
               "GITHUB_RUN_ID": "payload", "GITHUB_RUN_ATTEMPT": "1", "RUNNER_ARCH": "ARM64",
               "RUNNER_TEMP": str(self.out / "runner"), "GITHUB_SHA": "b" * 40}
        result = subprocess.run(["bash", str(launcher), "aarch64"], env=env,
                                capture_output=True, timeout=15)
        self.assertNotEqual(result.returncode, 0)
        launch = json.loads((evidence / "launcher.json").read_text())
        payload = json.loads((evidence / "receipt.json").read_text())
        self.assertTrue(launch["unit_removed"])
        self.assertEqual(launch["teardown_errors"], [])
        self.assertEqual(payload["error"], "memory control events changed during run")
        self.assertEqual(payload["launcher_failure"]["launch_exit_code"], "17")
        self.assertEqual(payload["launcher_failure"]["teardown_errors"], [])

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
